// 比对结果仓储：candidate_edges / clusters / cluster_members / diffs。
// 写入由 compare_service 包在单事务里；查询面向分页过滤的结果屏。
use crate::db::now_iso;
use crate::engine::scoring::ScoreParts;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

pub struct NewEdge {
    pub source_chunk_id: String,
    pub target_chunk_id: String,
    pub parts: ScoreParts,
}

pub struct NewMember {
    pub document_id: String,
    pub chunk_id: String,
    pub role: String, // primary | duplicate_candidate | missing
    pub score: Option<f32>,
}

pub struct NewDiff {
    pub base_chunk_id: Option<String>,
    pub target_chunk_id: Option<String>,
    pub diff_type: String, // char | word | sentence
    pub diff_json: String,
    pub summary: Option<String>,
}

pub struct NewCluster {
    pub cluster_type: String,
    pub topic: Option<String>,
    pub summary: Option<String>,
    pub severity: String,
    pub score: f32,
    pub section_kind: Option<String>,
    pub conflict_json: Option<String>,
    /// 底版分块的位置（「第一章 › 1.1 报价」格式），供列表行内展示
    pub base_section_path: Option<String>,
    pub base_page: Option<i64>,
    pub members: Vec<NewMember>,
    pub diffs: Vec<NewDiff>,
}

pub fn insert_edges(conn: &rusqlite::Connection, job_id: &str, edges: &[NewEdge]) -> AppResult<()> {
    let now = now_iso();
    let mut stmt = conn.prepare(
        "INSERT INTO candidate_edges (id, job_id, source_chunk_id, target_chunk_id, lexical_score,
         char_ngram_score, entity_score, structure_score, order_score, semantic_score, final_score, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
    )?;
    for e in edges {
        stmt.execute(params![
            uuid::Uuid::new_v4().to_string(),
            job_id,
            e.source_chunk_id,
            e.target_chunk_id,
            e.parts.lexical,
            e.parts.char_ngram,
            e.parts.entity,
            e.parts.structure,
            e.parts.order,
            e.parts.semantic,
            e.parts.final_score,
            now,
        ])?;
    }
    Ok(())
}

pub fn insert_clusters(
    conn: &rusqlite::Connection,
    job_id: &str,
    clusters: &[NewCluster],
) -> AppResult<Vec<String>> {
    let now = now_iso();
    let mut ins_cluster = conn.prepare(
        "INSERT INTO clusters (id, job_id, cluster_type, topic, summary, severity, score,
         section_kind, conflict_json, base_section_path, base_page, review_status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'pending', ?12)",
    )?;
    let mut ins_member = conn.prepare(
        "INSERT INTO cluster_members (cluster_id, document_id, chunk_id, role, score)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut ins_diff = conn.prepare(
        "INSERT INTO diffs (id, cluster_id, base_chunk_id, target_chunk_id, diff_type, diff_json, summary, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    let mut ids = Vec::with_capacity(clusters.len());
    for c in clusters {
        let id = uuid::Uuid::new_v4().to_string();
        ins_cluster.execute(params![
            id,
            job_id,
            c.cluster_type,
            c.topic,
            c.summary,
            c.severity,
            c.score,
            c.section_kind,
            c.conflict_json,
            c.base_section_path,
            c.base_page,
            now
        ])?;
        for m in &c.members {
            ins_member.execute(params![id, m.document_id, m.chunk_id, m.role, m.score])?;
        }
        for d in &c.diffs {
            ins_diff.execute(params![
                uuid::Uuid::new_v4().to_string(),
                id,
                d.base_chunk_id,
                d.target_chunk_id,
                d.diff_type,
                d.diff_json,
                d.summary,
                now,
            ])?;
        }
        ids.push(id);
    }
    Ok(ids)
}

/// 清理某任务的全部比对产物（取消/失败/重跑前调用）。clusters 级联清 members/diffs。
pub fn delete_job_results(conn: &rusqlite::Connection, job_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM candidate_edges WHERE job_id = ?1", [job_id])?;
    conn.execute("DELETE FROM clusters WHERE job_id = ?1", [job_id])?;
    Ok(())
}

// —— 查询面 ——

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ClusterFilter {
    pub cluster_type: Option<String>,
    pub severity: Option<String>,
    pub review_status: Option<String>,
    pub section_kind: Option<String>,
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSummaryRow {
    pub id: String,
    pub job_id: String,
    pub cluster_type: String,
    pub topic: Option<String>,
    pub summary: Option<String>,
    pub severity: Option<String>,
    pub score: Option<f64>,
    pub section_kind: Option<String>,
    pub review_status: String,
    pub section_path: Option<String>,
    pub page: Option<i64>,
    pub document_ids: Vec<String>,
    pub member_count: i64,
}

/// 动态过滤条件。占位符从 ?start 开始编号——调用方的固定参数占用 ?1..?(start-1)，
/// 两处必须一致，不要在调用方增删固定参数时忘了改 start。
fn filter_sql(f: &ClusterFilter, start: usize) -> (String, Vec<String>) {
    let mut cond = String::new();
    let mut binds: Vec<String> = Vec::new();
    let add = |c: &str, v: &Option<String>, binds: &mut Vec<String>, cond: &mut String| {
        if let Some(v) = v {
            binds.push(v.clone());
            cond.push_str(&format!(" AND {} = ?{}", c, start + binds.len() - 1));
        }
    };
    add("cl.cluster_type", &f.cluster_type, &mut binds, &mut cond);
    add("cl.severity", &f.severity, &mut binds, &mut cond);
    add("cl.review_status", &f.review_status, &mut binds, &mut cond);
    add("cl.section_kind", &f.section_kind, &mut binds, &mut cond);
    if let Some(doc) = &f.document_id {
        binds.push(doc.clone());
        cond.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM cluster_members m WHERE m.cluster_id = cl.id AND m.document_id = ?{})",
            start + binds.len() - 1
        ));
    }
    (cond, binds)
}

pub fn count_clusters(
    conn: &rusqlite::Connection,
    job_id: &str,
    filter: &ClusterFilter,
) -> AppResult<i64> {
    let (cond, binds) = filter_sql(filter, 2);
    let sql = format!("SELECT COUNT(*) FROM clusters cl WHERE cl.job_id = ?1{cond}");
    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&job_id];
    for b in &binds {
        params_vec.push(b);
    }
    Ok(stmt.query_row(params_vec.as_slice(), |r| r.get(0))?)
}

pub fn list_clusters(
    conn: &rusqlite::Connection,
    job_id: &str,
    filter: &ClusterFilter,
    offset: i64,
    limit: i64,
) -> AppResult<Vec<ClusterSummaryRow>> {
    // 纵深防御：LIMIT/OFFSET 直接拼进 SQL，必须保证是受控整数
    let limit = limit.clamp(1, 500);
    let offset = offset.max(0);
    let (cond, binds) = filter_sql(filter, 2);
    // 排序：风险降序（high>medium>low>review>none）再按分数降序。
    // GROUP_CONCAT 用逗号切回列表：document_id 是 uuid v4，保证不含逗号
    let sql = format!(
        "SELECT cl.id, cl.job_id, cl.cluster_type, cl.topic, cl.summary, cl.severity, cl.score,
         cl.section_kind, cl.review_status, cl.base_section_path, cl.base_page,
         (SELECT GROUP_CONCAT(DISTINCT m.document_id) FROM cluster_members m WHERE m.cluster_id = cl.id),
         (SELECT COUNT(*) FROM cluster_members m WHERE m.cluster_id = cl.id)
         FROM clusters cl WHERE cl.job_id = ?1{cond}
         ORDER BY CASE cl.severity
            WHEN 'high' THEN 0 WHEN 'medium' THEN 1 WHEN 'low' THEN 2
            WHEN 'review' THEN 3 ELSE 4 END,
         cl.score DESC LIMIT {limit} OFFSET {offset}"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut params_vec: Vec<&dyn rusqlite::ToSql> = vec![&job_id];
    for b in &binds {
        params_vec.push(b);
    }
    let rows = stmt
        .query_map(params_vec.as_slice(), |r| {
            Ok(ClusterSummaryRow {
                id: r.get(0)?,
                job_id: r.get(1)?,
                cluster_type: r.get(2)?,
                topic: r.get(3)?,
                summary: r.get(4)?,
                severity: r.get(5)?,
                score: r.get(6)?,
                section_kind: r.get(7)?,
                review_status: r.get(8)?,
                section_path: r.get(9)?,
                page: r.get(10)?,
                document_ids: r
                    .get::<_, Option<String>>(11)?
                    .map(|s| s.split(',').map(str::to_string).collect())
                    .unwrap_or_default(),
                member_count: r.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 各 cluster_type 的计数（总览八类统计）。
pub fn type_counts(conn: &rusqlite::Connection, job_id: &str) -> AppResult<Vec<(String, i64)>> {
    let mut stmt = conn.prepare(
        "SELECT cluster_type, COUNT(*) FROM clusters WHERE job_id = ?1 GROUP BY cluster_type",
    )?;
    let rows = stmt
        .query_map([job_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberDetailRow {
    pub document_id: String,
    pub document_name: String,
    pub chunk_id: String,
    pub text: String,
    pub section_path: Option<String>,
    pub section_kind: Option<String>,
    pub page: Option<i64>,
    pub order_index: i64,
    pub role: String,
    pub score: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub base_chunk_id: Option<String>,
    pub target_chunk_id: Option<String>,
    pub diff_type: String,
    pub diff_json: String,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterDetail {
    pub cluster: ClusterSummaryRow,
    pub members: Vec<MemberDetailRow>,
    pub diffs: Vec<DiffRow>,
    pub facts: Vec<crate::db::repo::fact_repo::FactRow>,
    pub conflict_json: Option<String>,
}

pub fn get_cluster_detail(conn: &rusqlite::Connection, cluster_id: &str) -> AppResult<ClusterDetail> {
    let cluster = conn
        .query_row(
            "SELECT cl.id, cl.job_id, cl.cluster_type, cl.topic, cl.summary, cl.severity, cl.score,
             cl.section_kind, cl.review_status, cl.base_section_path, cl.base_page,
             (SELECT GROUP_CONCAT(DISTINCT m.document_id) FROM cluster_members m WHERE m.cluster_id = cl.id),
             (SELECT COUNT(*) FROM cluster_members m WHERE m.cluster_id = cl.id)
             FROM clusters cl WHERE cl.id = ?1",
            [cluster_id],
            |r| {
                Ok(ClusterSummaryRow {
                    id: r.get(0)?,
                    job_id: r.get(1)?,
                    cluster_type: r.get(2)?,
                    topic: r.get(3)?,
                    summary: r.get(4)?,
                    severity: r.get(5)?,
                    score: r.get(6)?,
                    section_kind: r.get(7)?,
                    review_status: r.get(8)?,
                    section_path: r.get(9)?,
                    page: r.get(10)?,
                    document_ids: r
                        .get::<_, Option<String>>(11)?
                        .map(|s| s.split(',').map(str::to_string).collect())
                        .unwrap_or_default(),
                    member_count: r.get(12)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("条款聚合"))?;

    let conflict_json: Option<String> = conn
        .query_row("SELECT conflict_json FROM clusters WHERE id = ?1", [cluster_id], |r| r.get(0))
        .optional()?
        .flatten();

    let mut stmt = conn.prepare(
        "SELECT m.document_id, d.file_name, m.chunk_id, c.text, c.section_path, c.section_kind,
         c.page, c.order_index, m.role, m.score
         FROM cluster_members m
         JOIN chunks c ON c.id = m.chunk_id
         JOIN documents d ON d.id = m.document_id
         WHERE m.cluster_id = ?1 ORDER BY m.document_id, c.order_index",
    )?;
    let members = stmt
        .query_map([cluster_id], |r| {
            Ok(MemberDetailRow {
                document_id: r.get(0)?,
                document_name: r.get(1)?,
                chunk_id: r.get(2)?,
                text: r.get(3)?,
                section_path: r.get(4)?,
                section_kind: r.get(5)?,
                page: r.get(6)?,
                order_index: r.get(7)?,
                role: r.get(8)?,
                score: r.get(9)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let mut stmt = conn.prepare(
        "SELECT base_chunk_id, target_chunk_id, diff_type, diff_json, summary
         FROM diffs WHERE cluster_id = ?1",
    )?;
    let diffs = stmt
        .query_map([cluster_id], |r| {
            Ok(DiffRow {
                base_chunk_id: r.get(0)?,
                target_chunk_id: r.get(1)?,
                diff_type: r.get(2)?,
                diff_json: r.get(3)?,
                summary: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    let facts = crate::db::repo::fact_repo::list_for_cluster(conn, cluster_id)?;

    Ok(ClusterDetail {
        cluster,
        members,
        diffs,
        facts,
        conflict_json,
    })
}

/// 导出用平铺行：clusters × members × chunks × documents 一次 join 取全（避免 N+1）。
pub struct ExportRow {
    pub cluster_id: String,
    pub cluster_type: String,
    pub severity: Option<String>,
    pub topic: Option<String>,
    pub summary: Option<String>,
    pub score: Option<f64>,
    pub review_status: String,
    pub section_kind: Option<String>,
    pub conflict_json: Option<String>,
    pub document_id: String,
    pub text: String,
    pub page: Option<i64>,
    pub section_path: Option<String>,
    pub role: String,
}

pub fn export_rows(conn: &rusqlite::Connection, job_id: &str) -> AppResult<Vec<ExportRow>> {
    let mut stmt = conn.prepare(
        "SELECT cl.id, cl.cluster_type, cl.severity, cl.topic, cl.summary, cl.score,
         cl.review_status, cl.section_kind, cl.conflict_json,
         m.document_id, c.text, c.page, c.section_path, m.role
         FROM clusters cl
         JOIN cluster_members m ON m.cluster_id = cl.id
         JOIN chunks c ON c.id = m.chunk_id
         WHERE cl.job_id = ?1
         ORDER BY CASE cl.severity
            WHEN 'high' THEN 0 WHEN 'medium' THEN 1 WHEN 'low' THEN 2
            WHEN 'review' THEN 3 ELSE 4 END,
         cl.score DESC, cl.id, m.document_id",
    )?;
    let rows = stmt
        .query_map([job_id], |r| {
            Ok(ExportRow {
                cluster_id: r.get(0)?,
                cluster_type: r.get(1)?,
                severity: r.get(2)?,
                topic: r.get(3)?,
                summary: r.get(4)?,
                score: r.get(5)?,
                review_status: r.get(6)?,
                section_kind: r.get(7)?,
                conflict_json: r.get(8)?,
                document_id: r.get(9)?,
                text: r.get(10)?,
                page: r.get(11)?,
                section_path: r.get(12)?,
                role: r.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 两文档在某任务下的 primary 段落对（按分数降序，限 40）。逐对明细与导出共用。
pub fn pair_texts(
    conn: &rusqlite::Connection,
    job_id: &str,
    document_a: &str,
    document_b: &str,
) -> AppResult<Vec<(f64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT cl.score, ca.text, cb.text FROM clusters cl
         JOIN cluster_members ma ON ma.cluster_id = cl.id AND ma.document_id = ?2 AND ma.role = 'primary'
         JOIN cluster_members mb ON mb.cluster_id = cl.id AND mb.document_id = ?3 AND mb.role = 'primary'
         JOIN chunks ca ON ca.id = ma.chunk_id
         JOIN chunks cb ON cb.id = mb.chunk_id
         WHERE cl.job_id = ?1 AND cl.cluster_type NOT IN ('added', 'deleted')
         ORDER BY cl.score DESC LIMIT 40",
    )?;
    let rows = stmt
        .query_map(rusqlite::params![job_id, document_a, document_b], |r| {
            Ok((r.get::<_, Option<f64>>(0)?.unwrap_or(0.0), r.get(1)?, r.get(2)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn set_review_status(conn: &rusqlite::Connection, cluster_id: &str, status: &str) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE clusters SET review_status = ?2 WHERE id = ?1",
        params![cluster_id, status],
    )?;
    if n == 0 {
        return Err(AppError::not_found("条款聚合"));
    }
    Ok(())
}
