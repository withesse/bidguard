// 内嵌图片指纹仓储（document_images）：导入期写入 + 缓存复用复制 + 比对期读取。
// 不在内部开事务——写入方（import_service）负责把「图片写入 + 文档置 parsed」包进同一事务。
use crate::engine::parse::ImageHash;
use crate::error::AppResult;
use rusqlite::params;

/// 比对期读出的一张图片指纹。dhash 为 None 表示整页扫描图（导入期只存 exact，只做精确碰撞）。
pub struct ImageRecord {
    pub page: Option<u32>,
    pub sha256: String,
    pub dhash: Option<u64>,
}

/// 写入一个文档的全部图片指纹（idx 为文档内序号）。调用方需已开启事务。
/// dhash: u64 位型以 i64 存储（SQLite INTEGER），比对只做异或计数、符号无关。
pub fn insert_images(
    conn: &rusqlite::Connection,
    document_id: &str,
    images: &[ImageHash],
) -> AppResult<()> {
    if images.is_empty() {
        return Ok(());
    }
    let mut stmt = conn.prepare(
        "INSERT INTO document_images (id, document_id, idx, source, page, width, height, sha256, dhash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for (idx, img) in images.iter().enumerate() {
        stmt.execute(params![
            uuid::Uuid::new_v4().to_string(),
            document_id,
            idx as i64,
            img.source,
            img.page,
            img.width,
            img.height,
            img.sha256,
            img.dhash.map(|d| d as i64),
        ])?;
    }
    Ok(())
}

/// 缓存复用：把已解析文档的图片指纹复制给同 file_hash 的新文档。调用方需已开启事务。
/// 与 chunk_repo::copy_all 同一目的——复用路径若丢图片行，同一文件「重新导入也拿不到
/// 图片信号」（执行方案工程审查 HIGH 的缓存吞指纹问题）。
pub fn copy_images(conn: &rusqlite::Connection, from_doc: &str, to_doc: &str) -> AppResult<usize> {
    // 原样按存储列类型读出（dhash 保持 i64 位型），改 id 后重插——行数有限（单文档 ≤200）。
    struct Row {
        idx: i64,
        source: String,
        page: Option<i64>,
        width: i64,
        height: i64,
        sha256: String,
        dhash: Option<i64>,
    }
    let mut sel = conn.prepare(
        "SELECT idx, source, page, width, height, sha256, dhash
         FROM document_images WHERE document_id = ?1 ORDER BY idx",
    )?;
    let rows = sel
        .query_map([from_doc], |r| {
            Ok(Row {
                idx: r.get(0)?,
                source: r.get(1)?,
                page: r.get(2)?,
                width: r.get(3)?,
                height: r.get(4)?,
                sha256: r.get(5)?,
                dhash: r.get(6)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut ins = conn.prepare(
        "INSERT INTO document_images (id, document_id, idx, source, page, width, height, sha256, dhash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    let n = rows.len();
    for row in rows {
        ins.execute(params![
            uuid::Uuid::new_v4().to_string(),
            to_doc,
            row.idx,
            row.source,
            row.page,
            row.width,
            row.height,
            row.sha256,
            row.dhash,
        ])?;
    }
    Ok(n)
}

/// 比对期读取：某文档的全部图片指纹（按 idx 排序）。
pub fn list_images(conn: &rusqlite::Connection, document_id: &str) -> AppResult<Vec<ImageRecord>> {
    let mut stmt = conn.prepare(
        "SELECT page, sha256, dhash FROM document_images WHERE document_id = ?1 ORDER BY idx",
    )?;
    let rows = stmt
        .query_map([document_id], |r| {
            Ok(ImageRecord {
                page: r.get::<_, Option<i64>>(0)?.map(|p| p as u32),
                sha256: r.get(1)?,
                dhash: r.get::<_, Option<i64>>(2)?.map(|d| d as u64),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::db::repo::{document_repo, workspace_repo};

    fn mk(source: &'static str, page: Option<u32>, sha: &str, dhash: Option<u64>) -> ImageHash {
        ImageHash { source, page, width: 200, height: 160, sha256: sha.into(), dhash }
    }

    #[test]
    fn insert_list_copy_roundtrip_preserves_fields() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = workspace_repo::create(&conn, "w").unwrap();
        let d1 = document_repo::create_parsing(&conn, &ws.id, "a.docx", "/a", "h1", "docx", "oh", "bid")
            .unwrap();
        // 顶位置 1 的 dHash 验证 u64↔i64 位型往返；整页图 dhash=None 往返仍为 None
        let imgs = vec![
            mk("pdf", Some(3), "sha_a", Some(0x8000_0000_0000_0001)),
            mk("pdf", None, "sha_b", None),
        ];
        insert_images(&conn, &d1.id, &imgs).unwrap();
        let got = list_images(&conn, &d1.id).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].page, Some(3));
        assert_eq!(got[0].sha256, "sha_a");
        assert_eq!(got[0].dhash, Some(0x8000_0000_0000_0001), "高位 dHash 位型应无损往返");
        assert_eq!(got[1].page, None);
        assert_eq!(got[1].dhash, None, "整页图 dhash 往返仍为 None");

        // 缓存复用路径：复制到另一文档，行数与字段一致
        let d2 = document_repo::create_parsing(&conn, &ws.id, "b.docx", "/b", "h1", "docx", "oh", "bid")
            .unwrap();
        assert_eq!(copy_images(&conn, &d1.id, &d2.id).unwrap(), 2);
        let got2 = list_images(&conn, &d2.id).unwrap();
        assert_eq!(got2.len(), 2);
        assert_eq!(got2[0].dhash, Some(0x8000_0000_0000_0001));
        assert_eq!(got2[1].dhash, None);
    }

    #[test]
    fn insert_empty_is_noop() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = workspace_repo::create(&conn, "w").unwrap();
        let d = document_repo::create_parsing(&conn, &ws.id, "a.docx", "/a", "h", "docx", "oh", "bid")
            .unwrap();
        insert_images(&conn, &d.id, &[]).unwrap();
        assert!(list_images(&conn, &d.id).unwrap().is_empty());
    }
}
