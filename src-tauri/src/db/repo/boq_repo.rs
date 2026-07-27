// 报价清单条目仓储（W5-1，M6 商务标数值层）：写侧随比对流水线落库，读侧供数值面板与
// 后续 W5-2/3/4 的雷同率/算术错误/相关性计算消费。
// 不在内部开事务——调用方（compare_service 阶段 7）把清单条目与聚类/边写进同一个事务。
use crate::db::now_iso;
use crate::error::AppResult;
use rusqlite::params;
use serde::Serialize;

/// 一条待入库的清单条目。align_key 为 None 表示该条目未跨文档对齐（单家独有）。
pub struct NewBoqItem {
    pub doc_index: i64,
    pub document_id: String,
    pub chunk_id: String,
    pub align_key: Option<String>,
    pub code: Option<String>,
    pub name: Option<String>,
    pub unit: Option<String>,
    pub qty: Option<f64>,
    pub unit_price: Option<f64>,
    pub total_price: Option<f64>,
    pub row_index: i64,
    pub page: Option<i64>,
    pub flags: Option<String>,
}

/// 批量写入清单条目。调用方需已开启事务。
pub fn insert_items(
    conn: &rusqlite::Connection,
    job_id: &str,
    items: &[NewBoqItem],
) -> AppResult<()> {
    if items.is_empty() {
        return Ok(());
    }
    let now = now_iso();
    let mut stmt = conn.prepare(
        "INSERT INTO boq_items (id, job_id, doc_index, document_id, chunk_id, align_key, code,
         name, unit, qty, unit_price, total_price, row_index, page, flags, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
    )?;
    for it in items {
        stmt.execute(params![
            uuid::Uuid::new_v4().to_string(),
            job_id,
            it.doc_index,
            it.document_id,
            it.chunk_id,
            it.align_key,
            it.code,
            it.name,
            it.unit,
            it.qty,
            it.unit_price,
            it.total_price,
            it.row_index,
            it.page,
            it.flags,
            now,
        ])?;
    }
    Ok(())
}

/// 清单条目查询行（DTO；数值面板与导出消费）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BoqItemRow {
    pub id: String,
    pub doc_index: i64,
    pub document_id: String,
    pub chunk_id: String,
    pub align_key: Option<String>,
    pub code: Option<String>,
    pub name: Option<String>,
    pub unit: Option<String>,
    pub qty: Option<f64>,
    pub unit_price: Option<f64>,
    pub total_price: Option<f64>,
    pub row_index: i64,
    pub page: Option<i64>,
    pub flags: Option<String>,
}

/// 列出某任务的全部清单条目（按 doc_index → row_index，即「各家文档内原始行序」）。
pub fn list_by_job(conn: &rusqlite::Connection, job_id: &str) -> AppResult<Vec<BoqItemRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, doc_index, document_id, chunk_id, align_key, code, name, unit, qty,
         unit_price, total_price, row_index, page, flags
         FROM boq_items WHERE job_id = ?1 ORDER BY doc_index, row_index",
    )?;
    let rows = stmt
        .query_map(params![job_id], |r| {
            Ok(BoqItemRow {
                id: r.get(0)?,
                doc_index: r.get(1)?,
                document_id: r.get(2)?,
                chunk_id: r.get(3)?,
                align_key: r.get(4)?,
                code: r.get(5)?,
                name: r.get(6)?,
                unit: r.get(7)?,
                qty: r.get(8)?,
                unit_price: r.get(9)?,
                total_price: r.get(10)?,
                row_index: r.get(11)?,
                page: r.get(12)?,
                flags: r.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> rusqlite::Connection {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
        crate::db::migrations::run(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES ('w1','测试','t','t')",
            [],
        )
        .unwrap();
        for id in ["d1", "d2"] {
            conn.execute(
                "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash,
                 file_type, status, created_at, updated_at)
                 VALUES (?1,'w1','f','p',?1,'xlsx','parsed','t','t')",
                [id],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO jobs (id, workspace_id, job_type, status, created_at)
             VALUES ('j1','w1','compare','completed','t')",
            [],
        )
        .unwrap();
        conn
    }

    fn item(doc: i64, row: i64, key: Option<&str>) -> NewBoqItem {
        NewBoqItem {
            doc_index: doc,
            document_id: if doc == 0 { "d1".into() } else { "d2".into() },
            chunk_id: format!("chunk-{doc}-{row}"),
            align_key: key.map(|s| s.to_string()),
            code: Some("010101001001".into()),
            name: Some("挖一般土方".into()),
            unit: Some("m3".into()),
            qty: Some(1200.0),
            unit_price: Some(25.5),
            total_price: Some(30600.0),
            row_index: row,
            page: Some(1),
            flags: None,
        }
    }

    #[test]
    fn insert_and_list_round_trip() {
        let conn = setup();
        insert_items(
            &conn,
            "j1",
            &[item(1, 0, Some("c12:010101001001#0")), item(0, 0, Some("c12:010101001001#0"))],
        )
        .unwrap();
        let rows = list_by_job(&conn, "j1").unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].doc_index, 0, "应按 doc_index 排序");
        assert_eq!(rows[0].unit_price, Some(25.5));
        assert_eq!(rows[0].align_key.as_deref(), Some("c12:010101001001#0"));
        assert_eq!(rows[0].chunk_id, "chunk-0-0");
        // 空写入是合法的 no-op（无清单表的任务）
        insert_items(&conn, "j1", &[]).unwrap();
        assert_eq!(list_by_job(&conn, "j1").unwrap().len(), 2);
        assert!(list_by_job(&conn, "nope").unwrap().is_empty());
    }
}
