// 事实仓储：facts 表（按 chunk 落库，重跑比对时整体替换）。
use crate::db::now_iso;
use crate::engine::fact::Fact;
use crate::error::AppResult;
use rusqlite::params;
use serde::Serialize;

/// 替换式写入：同 chunk 先删后插（事实抽取是确定性的，重复写入等价幂等）。
/// 调用方需已开启事务。
pub fn replace_for_chunks(
    conn: &rusqlite::Connection,
    items: &[(String, Fact)],
) -> AppResult<()> {
    let now = now_iso();
    let mut del = conn.prepare("DELETE FROM facts WHERE chunk_id = ?1")?;
    let mut ins = conn.prepare(
        "INSERT INTO facts (id, chunk_id, subject, action, object, amount, date_expr, duration,
         percentage, condition_expr, obligation_type, confidence, fact_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
    )?;
    for (chunk_id, f) in items {
        del.execute([chunk_id])?;
        ins.execute(params![
            uuid::Uuid::new_v4().to_string(),
            chunk_id,
            f.subject,
            f.action,
            f.object,
            f.amount,
            f.date,
            f.duration,
            f.percentage,
            f.condition,
            f.obligation_type,
            f.confidence,
            serde_json::to_string(f).ok(),
            now,
        ])?;
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FactRow {
    pub chunk_id: String,
    pub document_id: String,
    pub subject: Option<String>,
    pub action: Option<String>,
    pub object: Option<String>,
    pub amount: Option<String>,
    pub date: Option<String>,
    pub duration: Option<String>,
    pub percentage: Option<String>,
    pub condition: Option<String>,
    pub obligation_type: Option<String>,
    pub confidence: Option<f64>,
}

/// 某条款聚合涉及的全部事实（按成员 chunk 关联）。
pub fn list_for_cluster(conn: &rusqlite::Connection, cluster_id: &str) -> AppResult<Vec<FactRow>> {
    let mut stmt = conn.prepare(
        "SELECT f.chunk_id, m.document_id, f.subject, f.action, f.object, f.amount, f.date_expr,
         f.duration, f.percentage, f.condition_expr, f.obligation_type, f.confidence
         FROM facts f JOIN cluster_members m ON m.chunk_id = f.chunk_id
         WHERE m.cluster_id = ?1",
    )?;
    let rows = stmt
        .query_map([cluster_id], |r| {
            Ok(FactRow {
                chunk_id: r.get(0)?,
                document_id: r.get(1)?,
                subject: r.get(2)?,
                action: r.get(3)?,
                object: r.get(4)?,
                amount: r.get(5)?,
                date: r.get(6)?,
                duration: r.get(7)?,
                percentage: r.get(8)?,
                condition: r.get(9)?,
                obligation_type: r.get(10)?,
                confidence: r.get(11)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
