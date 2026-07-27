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
    /// k-共现查证（W3-3）：命中招标（'tender'）/背景库（'background'）的合法共享出处；None=未豁免。
    pub exempt_reason: Option<String>,
    /// k-共现查证（W3-3）：两库皆查不到出处且查证质量闸门通过 → true（『多家异常一致·待复核』）。
    pub multi_doc_anomaly: bool,
    pub members: Vec<NewMember>,
    pub diffs: Vec<NewDiff>,
}

/// 招标文件对减的豁免证据（W3-2）：一个投标分块「引用招标文件」的覆盖率与覆盖区间。
pub struct NewExemption {
    pub chunk_id: String,
    pub kind: String, // tender | background
    pub coverage: f32,
    pub spans_json: String,
}

/// 批量写入 job 级豁免证据（chunk_exemptions）。调用方需已开启事务。
/// 同 (job,chunk,kind) 幂等覆盖（INSERT OR REPLACE），重跑不重复堆积。
pub fn insert_exemptions(
    conn: &rusqlite::Connection,
    job_id: &str,
    exemptions: &[NewExemption],
) -> AppResult<()> {
    let mut stmt = conn.prepare(
        "INSERT OR REPLACE INTO chunk_exemptions (job_id, chunk_id, kind, coverage, spans_json)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for e in exemptions {
        stmt.execute(params![job_id, e.chunk_id, e.kind, e.coverage, e.spans_json])?;
    }
    Ok(())
}

/// 一条逐字雷同区间（W4-1 铁证层，M5a）。两侧锚点各自「起块 id + 块内起偏移 →
/// 止块 id + 块内止偏移(不含)」，char_len=去空白后匹配字符数，sample_text=匹配文本样本。
pub struct NewVerbatim {
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub a_start_chunk_id: String,
    pub a_start_offset: i64,
    pub a_end_chunk_id: String,
    pub a_end_offset: i64,
    pub b_start_chunk_id: String,
    pub b_start_offset: i64,
    pub b_end_chunk_id: String,
    pub b_end_offset: i64,
    pub char_len: i64,
    pub sample_text: String,
}

/// 批量写入逐字雷同区间（verbatim_matches）。调用方需已开启事务。segment_id 留空（M5b 回填）。
pub fn insert_verbatim_matches(
    conn: &rusqlite::Connection,
    job_id: &str,
    matches: &[NewVerbatim],
) -> AppResult<()> {
    let now = now_iso();
    let mut stmt = conn.prepare(
        "INSERT INTO verbatim_matches (id, job_id, doc_a_id, doc_b_id, a_start_chunk_id,
         a_start_offset, a_end_chunk_id, a_end_offset, b_start_chunk_id, b_start_offset,
         b_end_chunk_id, b_end_offset, char_len, sample_text, segment_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, NULL, ?15)",
    )?;
    for m in matches {
        stmt.execute(params![
            uuid::Uuid::new_v4().to_string(),
            job_id,
            m.doc_a_id,
            m.doc_b_id,
            m.a_start_chunk_id,
            m.a_start_offset,
            m.a_end_chunk_id,
            m.a_end_offset,
            m.b_start_chunk_id,
            m.b_start_offset,
            m.b_end_chunk_id,
            m.b_end_offset,
            m.char_len,
            m.sample_text,
            now,
        ])?;
    }
    Ok(())
}

/// 逐字层查询行（DTO 预留，M5b 区段视图消费）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerbatimRow {
    pub id: String,
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub a_start_chunk_id: String,
    pub a_start_offset: i64,
    pub a_end_chunk_id: String,
    pub a_end_offset: i64,
    pub b_start_chunk_id: String,
    pub b_start_offset: i64,
    pub b_end_chunk_id: String,
    pub b_end_offset: i64,
    pub char_len: i64,
    pub sample_text: String,
    pub segment_id: Option<String>,
}

/// 两文档某任务下的逐字区间（按 char_len 降序）。方向无关：任一存储朝向都命中。
pub fn list_verbatim_for_pair(
    conn: &rusqlite::Connection,
    job_id: &str,
    document_a: &str,
    document_b: &str,
) -> AppResult<Vec<VerbatimRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, doc_a_id, doc_b_id, a_start_chunk_id, a_start_offset, a_end_chunk_id,
         a_end_offset, b_start_chunk_id, b_start_offset, b_end_chunk_id, b_end_offset,
         char_len, sample_text, segment_id
         FROM verbatim_matches
         WHERE job_id = ?1
           AND ((doc_a_id = ?2 AND doc_b_id = ?3) OR (doc_a_id = ?3 AND doc_b_id = ?2))
         ORDER BY char_len DESC, id",
    )?;
    let rows = stmt
        .query_map(params![job_id, document_a, document_b], |r| {
            Ok(VerbatimRow {
                id: r.get(0)?,
                doc_a_id: r.get(1)?,
                doc_b_id: r.get(2)?,
                a_start_chunk_id: r.get(3)?,
                a_start_offset: r.get(4)?,
                a_end_chunk_id: r.get(5)?,
                a_end_offset: r.get(6)?,
                b_start_chunk_id: r.get(7)?,
                b_start_offset: r.get(8)?,
                b_end_chunk_id: r.get(9)?,
                b_end_offset: r.get(10)?,
                char_len: r.get(11)?,
                sample_text: r.get(12)?,
                segment_id: r.get(13)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 一条对齐区段的链化锚点（W4-2，M5a）。kind: edge 残差边 | soft 软种子 | verbatim 逐字铁证。
pub struct NewSegmentAnchor {
    pub a_chunk_id: String,
    pub b_chunk_id: String,
    pub kind: String,
    pub score: f32,
}

/// 一条区段内 gap 的带状字符级细化产物（W4-3，M5a）。diff_json=DiffOp 序列（eq/ins/del），
/// eq_chars=该 gap 双方相同字符数（回填细化后覆盖率）。随区段一并落库（segment_diffs），
/// segment_id 由 insert_segments 生成的区段主键回填、随区段 FK 级联删除。
pub struct NewSegmentDiff {
    pub a_chunk_id: Option<String>,
    pub b_chunk_id: Option<String>,
    pub diff_type: String,
    pub diff_json: String,
    pub eq_chars: i64,
}

/// 一条对齐区段（W4-2 seed-chain-align，M5a）。两侧各以稠密行序区间 + 首末 chunk 锚定，
/// coverage=被命中块字符和/区间总字符和；anchors 随区段一并落库（segment_anchors）。
pub struct NewSegment {
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub a_start_order: i64,
    pub a_end_order: i64,
    pub b_start_order: i64,
    pub b_end_order: i64,
    pub a_start_chunk_id: String,
    pub a_end_chunk_id: String,
    pub b_start_chunk_id: String,
    pub b_end_chunk_id: String,
    pub anchor_count: i64,
    pub verbatim_chars: i64,
    pub a_covered_chars: i64,
    pub b_covered_chars: i64,
    pub a_coverage: f32,
    pub b_coverage: f32,
    pub avg_score: f32,
    pub a_section_path: Option<String>,
    pub b_section_path: Option<String>,
    pub a_page_start: Option<i64>,
    pub a_page_end: Option<i64>,
    pub b_page_start: Option<i64>,
    pub b_page_end: Option<i64>,
    pub anchors: Vec<NewSegmentAnchor>,
    /// 区段内各 gap 的带状字符级细化产物（W4-3）；随区段主键一并落 segment_diffs。
    pub diffs: Vec<NewSegmentDiff>,
}

/// 批量写入对齐区段及其锚点（aligned_segments + segment_anchors）。调用方需已开启事务。
/// 每区段生成 uuid 主键，锚点以 (segment_id, a_chunk_id, b_chunk_id) 去重（INSERT OR IGNORE
/// 容忍同一区段内极少见的重复锚点键，不因主键冲突整批回滚）。
pub fn insert_segments(
    conn: &rusqlite::Connection,
    job_id: &str,
    segments: &[NewSegment],
) -> AppResult<()> {
    let now = now_iso();
    let mut ins_seg = conn.prepare(
        "INSERT INTO aligned_segments (id, job_id, doc_a_id, doc_b_id, a_start_order, a_end_order,
         b_start_order, b_end_order, a_start_chunk_id, a_end_chunk_id, b_start_chunk_id,
         b_end_chunk_id, anchor_count, verbatim_chars, a_covered_chars, b_covered_chars,
         a_coverage, b_coverage, avg_score, a_section_path, b_section_path, a_page_start,
         a_page_end, b_page_start, b_page_end, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18,
         ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26)",
    )?;
    let mut ins_anchor = conn.prepare(
        "INSERT OR IGNORE INTO segment_anchors (segment_id, a_chunk_id, b_chunk_id, kind, score)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    let mut ins_diff = conn.prepare(
        "INSERT INTO segment_diffs (id, segment_id, a_chunk_id, b_chunk_id, diff_type, diff_json,
         eq_chars, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )?;
    for s in segments {
        let id = uuid::Uuid::new_v4().to_string();
        ins_seg.execute(params![
            id,
            job_id,
            s.doc_a_id,
            s.doc_b_id,
            s.a_start_order,
            s.a_end_order,
            s.b_start_order,
            s.b_end_order,
            s.a_start_chunk_id,
            s.a_end_chunk_id,
            s.b_start_chunk_id,
            s.b_end_chunk_id,
            s.anchor_count,
            s.verbatim_chars,
            s.a_covered_chars,
            s.b_covered_chars,
            s.a_coverage,
            s.b_coverage,
            s.avg_score,
            s.a_section_path,
            s.b_section_path,
            s.a_page_start,
            s.a_page_end,
            s.b_page_start,
            s.b_page_end,
            now,
        ])?;
        for a in &s.anchors {
            ins_anchor.execute(params![id, a.a_chunk_id, a.b_chunk_id, a.kind, a.score])?;
        }
        for d in &s.diffs {
            ins_diff.execute(params![
                uuid::Uuid::new_v4().to_string(),
                id,
                d.a_chunk_id,
                d.b_chunk_id,
                d.diff_type,
                d.diff_json,
                d.eq_chars,
                now,
            ])?;
        }
    }
    Ok(())
}

/// 一条区段 gap 细化产物（读侧，W4-3；供 M5b 区段详情渲染）。
pub struct SegmentDiffRow {
    pub a_chunk_id: Option<String>,
    pub b_chunk_id: Option<String>,
    pub diff_type: String,
    pub diff_json: String,
    pub eq_chars: i64,
}

/// 列出某区段的全部 gap 细化产物（按插入序 rowid，与区段内 gap 顺序一致）。
pub fn list_segment_diffs(
    conn: &rusqlite::Connection,
    segment_id: &str,
) -> AppResult<Vec<SegmentDiffRow>> {
    let mut stmt = conn.prepare(
        "SELECT a_chunk_id, b_chunk_id, diff_type, diff_json, eq_chars
         FROM segment_diffs WHERE segment_id = ?1 ORDER BY rowid",
    )?;
    let rows = stmt
        .query_map([segment_id], |r| {
            Ok(SegmentDiffRow {
                a_chunk_id: r.get(0)?,
                b_chunk_id: r.get(1)?,
                diff_type: r.get(2)?,
                diff_json: r.get(3)?,
                eq_chars: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
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
         section_kind, conflict_json, base_section_path, base_page, exempt_reason,
         multi_doc_anomaly, review_status, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, 'pending', ?14)",
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
            c.exempt_reason,
            c.multi_doc_anomaly as i64,
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
/// chunk_exemptions 的 job_id 外键 ON DELETE CASCADE 仅在删 job 行时触发，delete_job_results
/// 保留 job 行、只清结果，故显式删除（否则取消/重跑会残留上次的豁免证据）。
pub fn delete_job_results(conn: &rusqlite::Connection, job_id: &str) -> AppResult<()> {
    conn.execute("DELETE FROM candidate_edges WHERE job_id = ?1", [job_id])?;
    conn.execute("DELETE FROM clusters WHERE job_id = ?1", [job_id])?;
    conn.execute("DELETE FROM chunk_exemptions WHERE job_id = ?1", [job_id])?;
    // verbatim_matches 的 job_id 外键 ON DELETE CASCADE 仅在删 job 行时触发；delete_job_results
    // 保留 job 行、只清结果，故显式删除（否则取消/重跑会残留上次的逐字区间，与 chunk_exemptions 同理）。
    conn.execute("DELETE FROM verbatim_matches WHERE job_id = ?1", [job_id])?;
    // aligned_segments 同理显式删除（segment_anchors 随 segment_id 外键级联，无需单独删）。
    conn.execute("DELETE FROM aligned_segments WHERE job_id = ?1", [job_id])?;
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
    /// 按豁免出处筛选（W3-3）：'tender' | 'background'（精确匹配）。
    pub exempt_reason: Option<String>,
    /// 仅「多家异常一致·待复核」簇（W3-3）：Some(true)=只看异常簇。
    pub multi_doc_anomaly: Option<bool>,
    /// 仅「恰好两家共有」簇（W3-3 首要证据视图）：Some(true)=只看跨 2 份文档的簇。
    pub two_docs_only: Option<bool>,
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
    /// k-共现查证（W3-3）：命中招标（'tender'）/背景库（'background'）→ 合法共享，UI 置灰；None=未豁免。
    pub exempt_reason: Option<String>,
    /// k-共现查证（W3-3）：『多家异常一致·待复核』标记，前端红色徽标、可筛。
    pub multi_doc_anomaly: bool,
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
    add("cl.exempt_reason", &f.exempt_reason, &mut binds, &mut cond);
    if let Some(doc) = &f.document_id {
        binds.push(doc.clone());
        cond.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM cluster_members m WHERE m.cluster_id = cl.id AND m.document_id = ?{})",
            start + binds.len() - 1
        ));
    }
    // 布尔筛选无需绑定参数（W3-3）：多家异常一致 / 仅两家共有（跨 2 份文档）。
    if f.multi_doc_anomaly == Some(true) {
        cond.push_str(" AND cl.multi_doc_anomaly = 1");
    }
    if f.two_docs_only == Some(true) {
        cond.push_str(
            " AND (SELECT COUNT(DISTINCT m.document_id) FROM cluster_members m WHERE m.cluster_id = cl.id) = 2",
        );
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
         (SELECT COUNT(*) FROM cluster_members m WHERE m.cluster_id = cl.id),
         cl.exempt_reason, cl.multi_doc_anomaly
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
                exempt_reason: r.get(13)?,
                multi_doc_anomaly: r.get::<_, i64>(14)? != 0,
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
    /// 引用招标文件覆盖率（W3-2）：该成员分块命中招标指纹的字符占比；
    /// 非豁免块为 None。前端对 ≥0.8 的块显示「引用招标文件 · 覆盖 xx%」徽标。
    pub tender_coverage: Option<f64>,
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
             (SELECT COUNT(*) FROM cluster_members m WHERE m.cluster_id = cl.id),
             cl.exempt_reason, cl.multi_doc_anomaly
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
                    exempt_reason: r.get(13)?,
                    multi_doc_anomaly: r.get::<_, i64>(14)? != 0,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("条款聚合"))?;

    let conflict_json: Option<String> = conn
        .query_row("SELECT conflict_json FROM clusters WHERE id = ?1", [cluster_id], |r| r.get(0))
        .optional()?
        .flatten();

    // LEFT JOIN 招标豁免（同任务、tender kind）：命中块附覆盖率，供前端徽标。
    let mut stmt = conn.prepare(
        "SELECT m.document_id, d.file_name, m.chunk_id, c.text, c.section_path, c.section_kind,
         c.page, c.order_index, m.role, m.score, ce.coverage
         FROM cluster_members m
         JOIN chunks c ON c.id = m.chunk_id
         JOIN documents d ON d.id = m.document_id
         LEFT JOIN chunk_exemptions ce
           ON ce.chunk_id = m.chunk_id AND ce.job_id = ?2 AND ce.kind = 'tender'
         WHERE m.cluster_id = ?1 ORDER BY m.document_id, c.order_index",
    )?;
    let members = stmt
        .query_map(params![cluster_id, cluster.job_id], |r| {
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
                tender_coverage: r.get(10)?,
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
    /// k-共现查证（W3-3）：命中招标/背景库的合法共享出处（'tender'|'background'）；None=未豁免。
    pub exempt_reason: Option<String>,
    /// k-共现查证（W3-3）：『多家异常一致·待复核』标记。
    pub multi_doc_anomaly: bool,
    pub document_id: String,
    pub text: String,
    pub page: Option<i64>,
    pub section_path: Option<String>,
    pub role: String,
}

pub fn export_rows(conn: &rusqlite::Connection, job_id: &str) -> AppResult<Vec<ExportRow>> {
    let mut stmt = conn.prepare(
        "SELECT cl.id, cl.cluster_type, cl.severity, cl.topic, cl.summary, cl.score,
         cl.review_status, cl.section_kind, cl.conflict_json, cl.exempt_reason, cl.multi_doc_anomaly,
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
                exempt_reason: r.get(9)?,
                multi_doc_anomaly: r.get::<_, i64>(10)? != 0,
                document_id: r.get(11)?,
                text: r.get(12)?,
                page: r.get(13)?,
                section_path: r.get(14)?,
                role: r.get(15)?,
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
                "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
                 status, created_at, updated_at)
                 VALUES (?1,'w1','f','p','h','docx','parsed','t','t')",
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

    #[test]
    fn delete_job_results_clears_segments_and_anchors() {
        // 验收 (5)：delete_job_results 后 aligned_segments 与 segment_anchors 两表无残留。
        let conn = setup();
        let seg = NewSegment {
            doc_a_id: "d1".into(),
            doc_b_id: "d2".into(),
            a_start_order: 0,
            a_end_order: 9,
            b_start_order: 0,
            b_end_order: 9,
            a_start_chunk_id: "a0".into(),
            a_end_chunk_id: "a9".into(),
            b_start_chunk_id: "b0".into(),
            b_end_chunk_id: "b9".into(),
            anchor_count: 2,
            verbatim_chars: 80,
            a_covered_chars: 400,
            b_covered_chars: 400,
            a_coverage: 1.0,
            b_coverage: 1.0,
            avg_score: 0.9,
            a_section_path: Some("第三章 › 3.2 施工组织".into()),
            b_section_path: Some("第三章 › 3.2 施工组织".into()),
            a_page_start: Some(3),
            a_page_end: Some(5),
            b_page_start: Some(3),
            b_page_end: Some(5),
            anchors: vec![
                NewSegmentAnchor {
                    a_chunk_id: "a0".into(),
                    b_chunk_id: "b0".into(),
                    kind: "verbatim".into(),
                    score: 1.0,
                },
                NewSegmentAnchor {
                    a_chunk_id: "a1".into(),
                    b_chunk_id: "b1".into(),
                    kind: "edge".into(),
                    score: 0.9,
                },
            ],
            diffs: vec![NewSegmentDiff {
                a_chunk_id: Some("a5".into()),
                b_chunk_id: Some("b5".into()),
                diff_type: "gap-sentence".into(),
                diff_json: "[{\"op\":\"eq\",\"text\":\"甲\"}]".into(),
                eq_chars: 1,
            }],
        };
        insert_segments(&conn, "j1", &[seg]).unwrap();
        let segs: i64 =
            conn.query_row("SELECT COUNT(*) FROM aligned_segments", [], |r| r.get(0)).unwrap();
        let ancs: i64 =
            conn.query_row("SELECT COUNT(*) FROM segment_anchors", [], |r| r.get(0)).unwrap();
        let diffs: i64 =
            conn.query_row("SELECT COUNT(*) FROM segment_diffs", [], |r| r.get(0)).unwrap();
        assert_eq!(segs, 1);
        assert_eq!(ancs, 2);
        assert_eq!(diffs, 1);
        // list_segment_diffs 读回一致（取该区段主键）
        let seg_id: String =
            conn.query_row("SELECT id FROM aligned_segments", [], |r| r.get(0)).unwrap();
        let rows = list_segment_diffs(&conn, &seg_id).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].eq_chars, 1);
        assert_eq!(rows[0].diff_type, "gap-sentence");

        delete_job_results(&conn, "j1").unwrap();
        let segs_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM aligned_segments", [], |r| r.get(0)).unwrap();
        let ancs_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM segment_anchors", [], |r| r.get(0)).unwrap();
        let diffs_after: i64 =
            conn.query_row("SELECT COUNT(*) FROM segment_diffs", [], |r| r.get(0)).unwrap();
        assert_eq!(segs_after, 0, "delete_job_results 后 aligned_segments 应无残留");
        assert_eq!(ancs_after, 0, "区段清空后 segment_anchors 应随 FK 级联清空");
        assert_eq!(diffs_after, 0, "区段清空后 segment_diffs 应随 FK 级联清空");
    }
}
