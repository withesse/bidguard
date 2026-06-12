// 语义向量缓存：按 (normalized_hash, model_id) 全局复用 —— 同段落跨文档/跨任务零成本命中。
use crate::db::now_iso;
use crate::error::AppResult;
use rusqlite::params;
use std::collections::HashMap;

fn to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

fn from_blob(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

/// 批量查缓存。分批 IN 查询（SQLite 绑定变量上限 999，留余量），返回 hash → 向量。
pub fn get_many(
    conn: &rusqlite::Connection,
    hashes: &[String],
    model_id: &str,
) -> AppResult<HashMap<String, Vec<f32>>> {
    let mut out = HashMap::new();
    for batch in hashes.chunks(400) {
        let placeholders: String = (0..batch.len())
            .map(|i| format!("?{}", i + 2))
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT normalized_hash, vector FROM embeddings
             WHERE model_id = ?1 AND normalized_hash IN ({placeholders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&model_id];
        for h in batch {
            params_vec.push(h);
        }
        let rows = stmt.query_map(params_vec.as_slice(), |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Vec<u8>>(1)?))
        })?;
        for row in rows {
            let (h, b) = row?;
            out.insert(h, from_blob(&b));
        }
    }
    Ok(out)
}

pub fn insert_many(
    conn: &rusqlite::Connection,
    items: &[(String, Vec<f32>)],
    model_id: &str,
) -> AppResult<()> {
    let now = now_iso();
    let mut stmt = conn.prepare(
        "INSERT OR IGNORE INTO embeddings (normalized_hash, model_id, dim, vector, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (h, v) in items {
        stmt.execute(params![h, model_id, v.len() as i64, to_blob(v), now])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn roundtrip_and_dedup() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let v = vec![0.1f32, -0.5, 2.0];
        insert_many(&conn, &[("h1".into(), v.clone())], "m").unwrap();
        insert_many(&conn, &[("h1".into(), vec![9.0])], "m").unwrap(); // IGNORE 不覆盖
        let got = get_many(&conn, &["h1".into(), "h2".into()], "m").unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got["h1"], v);
        // 不同模型互不命中
        assert!(get_many(&conn, &["h1".into()], "other").unwrap().is_empty());
    }
}
