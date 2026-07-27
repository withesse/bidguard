// 对齐区段读侧仓储（W4-5，M5b）：区段列表 + 区段详情（双栏高亮所需的全部数据）。
// 只读证据层：区段不承载人工复核三态（仍挂 cluster），此处仅供展示 + 经 chunk_id 与聚类互链。
// 写侧（insert_segments / insert_verbatim_matches）在 compare_repo，比对流水线一并落库。
use crate::db::repo::compare_repo::{self, VerbatimRow};
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

/// 单条区段的列表摘要行（PairSegments 卡片列表消费）。两侧章节/页码范围 + 覆盖率双向 +
/// 锚点数 + 逐字字数，供列表行内展示「甲 3.2 ↔ 乙 3.2 · 覆盖 82% · 锚点 14 · 逐字 620 字」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentSummaryRow {
    pub id: String,
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub anchor_count: i64,
    pub verbatim_chars: i64,
    pub a_covered_chars: i64,
    pub b_covered_chars: i64,
    pub a_coverage: f64,
    pub b_coverage: f64,
    pub avg_score: f64,
    pub a_section_path: Option<String>,
    pub b_section_path: Option<String>,
    pub a_page_start: Option<i64>,
    pub a_page_end: Option<i64>,
    pub b_page_start: Option<i64>,
    pub b_page_end: Option<i64>,
}

/// 列出某任务的对齐区段（可选按文档对过滤，方向无关：任一存储朝向都命中）。
/// 排序：逐字字数降序 → A 侧覆盖字数降序（铁证多、覆盖广的区段排前，与验收口径一致）。
pub fn list_segments(
    conn: &rusqlite::Connection,
    job_id: &str,
    doc_a: Option<&str>,
    doc_b: Option<&str>,
) -> AppResult<Vec<SegmentSummaryRow>> {
    // 固定 SELECT + 排序；文档对过滤按是否两者皆给动态拼一段方向无关条件（绑定参数，无注入面）。
    let base = "SELECT id, doc_a_id, doc_b_id, anchor_count, verbatim_chars, a_covered_chars,
         b_covered_chars, a_coverage, b_coverage, avg_score, a_section_path, b_section_path,
         a_page_start, a_page_end, b_page_start, b_page_end
         FROM aligned_segments WHERE job_id = ?1";
    let order = " ORDER BY verbatim_chars DESC, a_covered_chars DESC, id";
    let map = |r: &rusqlite::Row| -> rusqlite::Result<SegmentSummaryRow> {
        Ok(SegmentSummaryRow {
            id: r.get(0)?,
            doc_a_id: r.get(1)?,
            doc_b_id: r.get(2)?,
            anchor_count: r.get(3)?,
            verbatim_chars: r.get(4)?,
            a_covered_chars: r.get(5)?,
            b_covered_chars: r.get(6)?,
            a_coverage: r.get(7)?,
            b_coverage: r.get(8)?,
            avg_score: r.get(9)?,
            a_section_path: r.get(10)?,
            b_section_path: r.get(11)?,
            a_page_start: r.get(12)?,
            a_page_end: r.get(13)?,
            b_page_start: r.get(14)?,
            b_page_end: r.get(15)?,
        })
    };
    match (doc_a, doc_b) {
        (Some(a), Some(b)) => {
            let sql = format!(
                "{base} AND ((doc_a_id = ?2 AND doc_b_id = ?3) OR (doc_a_id = ?3 AND doc_b_id = ?2)){order}"
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![job_id, a, b], map)?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
        _ => {
            let sql = format!("{base}{order}");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(params![job_id], map)?.collect::<Result<Vec<_>, _>>()?;
            Ok(rows)
        }
    }
}

/// 区段头（aligned_segments 整行，含两侧行序/首末块锚定/覆盖率/章节页码）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentHead {
    pub id: String,
    pub job_id: String,
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub a_start_chunk_id: String,
    pub a_end_chunk_id: String,
    pub b_start_chunk_id: String,
    pub b_end_chunk_id: String,
    pub anchor_count: i64,
    pub verbatim_chars: i64,
    pub a_covered_chars: i64,
    pub b_covered_chars: i64,
    pub a_coverage: f64,
    pub b_coverage: f64,
    pub avg_score: f64,
    pub a_section_path: Option<String>,
    pub b_section_path: Option<String>,
    pub a_page_start: Option<i64>,
    pub a_page_end: Option<i64>,
    pub b_page_start: Option<i64>,
    pub b_page_end: Option<i64>,
}

/// 区段跨度内的一个 chunk（按 order 顺序，供双栏渲染）。tender_coverage≥0.8 显示「引用招标文件」徽标。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentChunk {
    pub chunk_id: String,
    pub text: String,
    pub page: Option<i64>,
    pub section_path: Option<String>,
    pub order_index: i64,
    pub tender_coverage: Option<f64>,
}

/// 区段内一条链化锚点（kind: edge 残差边 | soft 软种子 | verbatim 逐字铁证）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentAnchorRow {
    pub a_chunk_id: String,
    pub b_chunk_id: String,
    pub kind: String,
    pub score: f64,
}

/// 区段内一条 gap 细化产物（diff_json=DiffOp 序列，前端解析后按黄底渲染细化差异）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDiffOut {
    pub a_chunk_id: Option<String>,
    pub b_chunk_id: Option<String>,
    pub diff_type: String,
    pub diff_json: String,
    pub eq_chars: i64,
}

/// 区段详情（双栏高亮 + 反向互链所需的全部只读数据）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentDetail {
    pub segment: SegmentHead,
    /// A/B 两侧跨度内的 chunk（按 order 顺序）。前端首版限渲染前 200 块 + 展开更多。
    pub a_chunks: Vec<SegmentChunk>,
    pub b_chunks: Vec<SegmentChunk>,
    pub anchors: Vec<SegmentAnchorRow>,
    /// 落在本区段跨度内的逐字铁证区间（深红底）。
    pub verbatims: Vec<VerbatimRow>,
    /// 锚点间 gap 的带状字符级细化（黄底差异）。
    pub diffs: Vec<SegmentDiffOut>,
    /// 经锚点 chunk_id 反查关联的聚类 id 集合（区段↔聚类互链，供「查看所属条款」跳转）。
    pub cluster_ids: Vec<String>,
}

/// 取某文档某 chunk 的 order_index（块被删除返回 None）。
fn order_of(conn: &rusqlite::Connection, chunk_id: &str) -> AppResult<Option<i64>> {
    Ok(conn
        .query_row("SELECT order_index FROM chunks WHERE id = ?1", [chunk_id], |r| r.get(0))
        .optional()?)
}

/// 加载某文档 [lo, hi] order 区间内的段落级 chunk（跳过 heading），LEFT JOIN 招标豁免带出覆盖率。
/// 上限 3000 块兜底（正常区段远小于此），前端另做前 200 块 + 展开更多的渲染分页。
fn load_span(
    conn: &rusqlite::Connection,
    job_id: &str,
    document_id: &str,
    lo: i64,
    hi: i64,
) -> AppResult<Vec<SegmentChunk>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.text, c.page, c.section_path, c.order_index, ce.coverage
         FROM chunks c
         LEFT JOIN chunk_exemptions ce
           ON ce.chunk_id = c.id AND ce.job_id = ?2 AND ce.kind = 'tender'
         WHERE c.document_id = ?1 AND c.order_index BETWEEN ?3 AND ?4
           AND c.chunk_level = 'paragraph' AND c.chunk_type != 'heading'
         ORDER BY c.order_index LIMIT 3000",
    )?;
    let rows = stmt
        .query_map(params![document_id, job_id, lo, hi], |r| {
            Ok(SegmentChunk {
                chunk_id: r.get(0)?,
                text: r.get(1)?,
                page: r.get(2)?,
                section_path: r.get(3)?,
                order_index: r.get(4)?,
                tender_coverage: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 区段详情：头 + 两侧跨度 chunk + 锚点 + 逐字区间 + gap 细化 + 关联 cluster 集合。
pub fn get_segment_detail(conn: &rusqlite::Connection, segment_id: &str) -> AppResult<SegmentDetail> {
    let segment = conn
        .query_row(
            "SELECT id, job_id, doc_a_id, doc_b_id, a_start_chunk_id, a_end_chunk_id,
             b_start_chunk_id, b_end_chunk_id, anchor_count, verbatim_chars, a_covered_chars,
             b_covered_chars, a_coverage, b_coverage, avg_score, a_section_path, b_section_path,
             a_page_start, a_page_end, b_page_start, b_page_end
             FROM aligned_segments WHERE id = ?1",
            [segment_id],
            |r| {
                Ok(SegmentHead {
                    id: r.get(0)?,
                    job_id: r.get(1)?,
                    doc_a_id: r.get(2)?,
                    doc_b_id: r.get(3)?,
                    a_start_chunk_id: r.get(4)?,
                    a_end_chunk_id: r.get(5)?,
                    b_start_chunk_id: r.get(6)?,
                    b_end_chunk_id: r.get(7)?,
                    anchor_count: r.get(8)?,
                    verbatim_chars: r.get(9)?,
                    a_covered_chars: r.get(10)?,
                    b_covered_chars: r.get(11)?,
                    a_coverage: r.get(12)?,
                    b_coverage: r.get(13)?,
                    avg_score: r.get(14)?,
                    a_section_path: r.get(15)?,
                    b_section_path: r.get(16)?,
                    a_page_start: r.get(17)?,
                    a_page_end: r.get(18)?,
                    b_page_start: r.get(19)?,
                    b_page_end: r.get(20)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::not_found("对齐区段"))?;

    // 两侧跨度：以首末锚定块的 order_index 取 [min,max]（块删除时缺一端仍尽量渲染，两端皆缺则空）。
    let a_span = span_bounds(conn, &segment.a_start_chunk_id, &segment.a_end_chunk_id)?;
    let b_span = span_bounds(conn, &segment.b_start_chunk_id, &segment.b_end_chunk_id)?;
    let a_chunks = match a_span {
        Some((lo, hi)) => load_span(conn, &segment.job_id, &segment.doc_a_id, lo, hi)?,
        None => Vec::new(),
    };
    let b_chunks = match b_span {
        Some((lo, hi)) => load_span(conn, &segment.job_id, &segment.doc_b_id, lo, hi)?,
        None => Vec::new(),
    };

    // 锚点。
    let mut stmt = conn.prepare(
        "SELECT a_chunk_id, b_chunk_id, kind, score FROM segment_anchors WHERE segment_id = ?1",
    )?;
    let anchors = stmt
        .query_map([segment_id], |r| {
            Ok(SegmentAnchorRow {
                a_chunk_id: r.get(0)?,
                b_chunk_id: r.get(1)?,
                kind: r.get(2)?,
                score: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    // gap 细化产物。
    let diffs = compare_repo::list_segment_diffs(conn, segment_id)?
        .into_iter()
        .map(|d| SegmentDiffOut {
            a_chunk_id: d.a_chunk_id,
            b_chunk_id: d.b_chunk_id,
            diff_type: d.diff_type,
            diff_json: d.diff_json,
            eq_chars: d.eq_chars,
        })
        .collect();

    // 逐字铁证：取该文档对全部逐字区间，过滤到落在本区段跨度内的（两侧起块均在渲染集合中）。
    let a_ids: std::collections::HashSet<&str> = a_chunks.iter().map(|c| c.chunk_id.as_str()).collect();
    let b_ids: std::collections::HashSet<&str> = b_chunks.iter().map(|c| c.chunk_id.as_str()).collect();
    let verbatims = compare_repo::list_verbatim_for_pair(
        conn,
        &segment.job_id,
        &segment.doc_a_id,
        &segment.doc_b_id,
    )?
    .into_iter()
    .filter(|v| {
        // 存储朝向可能与本区段 doc_a/doc_b 相反：两种朝向都接受，只要两端起块各落在对应集合。
        (a_ids.contains(v.a_start_chunk_id.as_str()) && b_ids.contains(v.b_start_chunk_id.as_str()))
            || (a_ids.contains(v.b_start_chunk_id.as_str())
                && b_ids.contains(v.a_start_chunk_id.as_str()))
    })
    .collect();

    // 反向互链：锚点两侧 chunk_id JOIN cluster_members 反查同任务下关联的 cluster_id 集合。
    let mut anchor_chunks: Vec<String> = Vec::new();
    for a in &anchors {
        anchor_chunks.push(a.a_chunk_id.clone());
        anchor_chunks.push(a.b_chunk_id.clone());
    }
    let cluster_ids = related_cluster_ids(conn, &segment.job_id, &anchor_chunks)?;

    Ok(SegmentDetail {
        segment,
        a_chunks,
        b_chunks,
        anchors,
        verbatims,
        diffs,
        cluster_ids,
    })
}

/// 取两个锚定块 order_index 的 [min,max]；两端皆缺（块被删）返回 None。
fn span_bounds(
    conn: &rusqlite::Connection,
    start_chunk_id: &str,
    end_chunk_id: &str,
) -> AppResult<Option<(i64, i64)>> {
    let s = order_of(conn, start_chunk_id)?;
    let e = order_of(conn, end_chunk_id)?;
    Ok(match (s, e) {
        (Some(a), Some(b)) => Some((a.min(b), a.max(b))),
        (Some(a), None) | (None, Some(a)) => Some((a, a)),
        (None, None) => None,
    })
}

/// 锚点 chunk_id 集合反查同任务下关联的 cluster_id（去重、稳定序）。
fn related_cluster_ids(
    conn: &rusqlite::Connection,
    job_id: &str,
    chunk_ids: &[String],
) -> AppResult<Vec<String>> {
    if chunk_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = std::iter::repeat_n("?", chunk_ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT DISTINCT cm.cluster_id FROM cluster_members cm
         JOIN clusters cl ON cl.id = cm.cluster_id
         WHERE cl.job_id = ?1 AND cm.chunk_id IN ({placeholders})
         ORDER BY cm.cluster_id"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut binds: Vec<&dyn rusqlite::ToSql> = vec![&job_id];
    for c in chunk_ids {
        binds.push(c);
    }
    let rows = stmt
        .query_map(binds.as_slice(), |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 某聚类反查关联的对齐区段引用（ClusterDetail「所在区段」Pill 反向互链消费）。
/// 覆盖率取存储朝向的两侧（前端展示 max 即可），verbatim_chars 供排序与「逐字 N 字」提示。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSegmentRef {
    pub segment_id: String,
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub a_coverage: f64,
    pub b_coverage: f64,
    pub verbatim_chars: i64,
}

/// 反向互链（cluster → segments）：聚类成员 chunk_id JOIN segment_anchors 反查同任务下命中的
/// 对齐区段。get_segment_detail 是正向（segment → cluster_ids），本函数是其逆。旧任务（无区段
/// 数据）自然返回空 Vec。按逐字字数降序，铁证多的区段排前。
pub fn segments_for_cluster(
    conn: &rusqlite::Connection,
    cluster_id: &str,
) -> AppResult<Vec<ClusterSegmentRef>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT s.id, s.doc_a_id, s.doc_b_id, s.a_coverage, s.b_coverage, s.verbatim_chars
         FROM aligned_segments s
         JOIN segment_anchors sa ON sa.segment_id = s.id
         JOIN cluster_members cm ON (cm.chunk_id = sa.a_chunk_id OR cm.chunk_id = sa.b_chunk_id)
         JOIN clusters cl ON cl.id = cm.cluster_id
         WHERE cm.cluster_id = ?1 AND s.job_id = cl.job_id
         ORDER BY s.verbatim_chars DESC, s.id",
    )?;
    let rows = stmt
        .query_map([cluster_id], |r| {
            Ok(ClusterSegmentRef {
                segment_id: r.get(0)?,
                doc_a_id: r.get(1)?,
                doc_b_id: r.get(2)?,
                a_coverage: r.get(3)?,
                b_coverage: r.get(4)?,
                verbatim_chars: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 导出「对齐区段与逐字证据」章节的区段摘要行（区段摘要表数据源）。较 SegmentSummaryRow 精简为
/// 报告所需列 + tender_quote（区段跨度任一侧命中招标豁免块 coverage≥0.8）。
#[derive(Debug, Clone)]
pub struct SegmentExportRow {
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub a_section_path: Option<String>,
    pub b_section_path: Option<String>,
    pub a_page_start: Option<i64>,
    pub a_page_end: Option<i64>,
    pub b_page_start: Option<i64>,
    pub b_page_end: Option<i64>,
    pub a_coverage: f64,
    pub b_coverage: f64,
    pub anchor_count: i64,
    pub verbatim_chars: i64,
    pub tender_quote: bool,
}

/// 导出用：某文档对的对齐区段摘要（方向无关）。tender_quote 经区段行序跨度 JOIN chunk_exemptions
/// 判定（任一侧跨度内存在 tender 豁免块 coverage≥0.8）。排序与 list_segments 一致（铁证多、覆盖广排前），
/// 保证同任务两次导出内容确定一致。
pub fn list_segments_for_export(
    conn: &rusqlite::Connection,
    job_id: &str,
    doc_a: &str,
    doc_b: &str,
) -> AppResult<Vec<SegmentExportRow>> {
    let mut stmt = conn.prepare(
        "SELECT s.doc_a_id, s.doc_b_id, s.a_section_path, s.b_section_path,
             s.a_page_start, s.a_page_end, s.b_page_start, s.b_page_end,
             s.a_coverage, s.b_coverage, s.anchor_count, s.verbatim_chars,
             (EXISTS(SELECT 1 FROM chunks c JOIN chunk_exemptions ce ON ce.chunk_id = c.id
                     WHERE c.document_id = s.doc_a_id
                       AND c.order_index BETWEEN s.a_start_order AND s.a_end_order
                       AND ce.job_id = s.job_id AND ce.kind = 'tender' AND ce.coverage >= 0.8)
              OR EXISTS(SELECT 1 FROM chunks c JOIN chunk_exemptions ce ON ce.chunk_id = c.id
                     WHERE c.document_id = s.doc_b_id
                       AND c.order_index BETWEEN s.b_start_order AND s.b_end_order
                       AND ce.job_id = s.job_id AND ce.kind = 'tender' AND ce.coverage >= 0.8))
             AS tender_quote
         FROM aligned_segments s
         WHERE s.job_id = ?1
           AND ((s.doc_a_id = ?2 AND s.doc_b_id = ?3) OR (s.doc_a_id = ?3 AND s.doc_b_id = ?2))
         ORDER BY s.verbatim_chars DESC, s.a_covered_chars DESC, s.id",
    )?;
    let rows = stmt
        .query_map(params![job_id, doc_a, doc_b], |r| {
            Ok(SegmentExportRow {
                doc_a_id: r.get(0)?,
                doc_b_id: r.get(1)?,
                a_section_path: r.get(2)?,
                b_section_path: r.get(3)?,
                a_page_start: r.get(4)?,
                a_page_end: r.get(5)?,
                b_page_start: r.get(6)?,
                b_page_end: r.get(7)?,
                a_coverage: r.get(8)?,
                b_coverage: r.get(9)?,
                anchor_count: r.get(10)?,
                verbatim_chars: r.get(11)?,
                tender_quote: r.get(12)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 导出「逐字清单」的一条逐字区间（含双侧起块页码/章节）。tender_quote：任一侧起块命中招标豁免。
#[derive(Debug, Clone)]
pub struct VerbatimExportRow {
    pub doc_a_id: String,
    pub doc_b_id: String,
    pub a_page: Option<i64>,
    pub b_page: Option<i64>,
    pub a_section_path: Option<String>,
    pub b_section_path: Option<String>,
    pub char_len: i64,
    pub sample_text: String,
    pub tender_quote: bool,
}

/// 导出用：某文档对的逐字雷同区间清单（方向无关，含双侧起块页码/章节）。按 char_len 降序、id 稳定序，
/// 保证确定性。起块 JOIN chunks 取页码/章节；tender_quote 经起块 chunk_exemptions 判定。
pub fn list_verbatims_for_export(
    conn: &rusqlite::Connection,
    job_id: &str,
    doc_a: &str,
    doc_b: &str,
) -> AppResult<Vec<VerbatimExportRow>> {
    let mut stmt = conn.prepare(
        "SELECT v.doc_a_id, v.doc_b_id, ca.page, cb.page, ca.section_path, cb.section_path,
             v.char_len, v.sample_text,
             (EXISTS(SELECT 1 FROM chunk_exemptions ce
                     WHERE ce.job_id = v.job_id AND ce.kind = 'tender' AND ce.coverage >= 0.8
                       AND ce.chunk_id IN (v.a_start_chunk_id, v.b_start_chunk_id))) AS tender_quote
         FROM verbatim_matches v
         LEFT JOIN chunks ca ON ca.id = v.a_start_chunk_id
         LEFT JOIN chunks cb ON cb.id = v.b_start_chunk_id
         WHERE v.job_id = ?1
           AND ((v.doc_a_id = ?2 AND v.doc_b_id = ?3) OR (v.doc_a_id = ?3 AND v.doc_b_id = ?2))
         ORDER BY v.char_len DESC, v.id",
    )?;
    let rows = stmt
        .query_map(params![job_id, doc_a, doc_b], |r| {
            Ok(VerbatimExportRow {
                doc_a_id: r.get(0)?,
                doc_b_id: r.get(1)?,
                a_page: r.get(2)?,
                b_page: r.get(3)?,
                a_section_path: r.get(4)?,
                b_section_path: r.get(5)?,
                char_len: r.get(6)?,
                sample_text: r.get(7)?,
                tender_quote: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::compare_repo::{
        insert_clusters, insert_segments, insert_verbatim_matches, NewCluster, NewMember,
        NewSegment, NewSegmentAnchor, NewSegmentDiff, NewVerbatim,
    };

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
                 VALUES (?1,'w1','f','p',?1,'docx','parsed','t','t')",
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
        // 两文档各 4 段 paragraph chunk（order 0..3），文本可辨识。
        for (doc, prefix) in [("d1", "a"), ("d2", "b")] {
            for i in 0..4 {
                conn.execute(
                    "INSERT INTO chunks (id, document_id, chunk_type, chunk_level, text,
                     normalized_text, char_count, page, order_index, created_at)
                     VALUES (?1, ?2, 'paragraph', 'paragraph', ?3, ?3, 10, 1, ?4, 't')",
                    params![format!("{prefix}{i}"), doc, format!("{prefix}文本{i}"), i],
                )
                .unwrap();
            }
        }
        conn
    }

    fn sample_segment() -> NewSegment {
        NewSegment {
            doc_a_id: "d1".into(),
            doc_b_id: "d2".into(),
            a_start_order: 0,
            a_end_order: 3,
            b_start_order: 0,
            b_end_order: 3,
            a_start_chunk_id: "a0".into(),
            a_end_chunk_id: "a3".into(),
            b_start_chunk_id: "b0".into(),
            b_end_chunk_id: "b3".into(),
            anchor_count: 2,
            verbatim_chars: 30,
            a_covered_chars: 30,
            b_covered_chars: 30,
            a_coverage: 0.82,
            b_coverage: 0.8,
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
                    a_chunk_id: "a2".into(),
                    b_chunk_id: "b2".into(),
                    kind: "edge".into(),
                    score: 0.9,
                },
            ],
            diffs: vec![NewSegmentDiff {
                a_chunk_id: Some("a1".into()),
                b_chunk_id: Some("b1".into()),
                diff_type: "gap-sentence".into(),
                diff_json: "[{\"op\":\"eq\",\"text\":\"相同\"},{\"op\":\"ins\",\"text\":\"新增\"}]".into(),
                eq_chars: 2,
            }],
        }
    }

    #[test]
    fn list_segments_orders_and_filters_by_pair() {
        let conn = setup();
        insert_segments(&conn, "j1", &[sample_segment()]).unwrap();

        // 无文档对过滤 → 全部。
        let all = list_segments(&conn, "j1", None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].verbatim_chars, 30);
        assert!((all[0].a_coverage - 0.82).abs() < 1e-6);

        // 方向无关：反向文档对同样命中。
        let rev = list_segments(&conn, "j1", Some("d2"), Some("d1")).unwrap();
        assert_eq!(rev.len(), 1);

        // 不相干任务 → 空。
        let none = list_segments(&conn, "jX", None, None).unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn get_segment_detail_returns_chunks_anchors_verbatim_diffs_and_clusters() {
        let conn = setup();
        insert_segments(&conn, "j1", &[sample_segment()]).unwrap();
        // 逐字铁证一条（a0↔b0）。
        insert_verbatim_matches(
            &conn,
            "j1",
            &[NewVerbatim {
                doc_a_id: "d1".into(),
                doc_b_id: "d2".into(),
                a_start_chunk_id: "a0".into(),
                a_start_offset: 0,
                a_end_chunk_id: "a0".into(),
                a_end_offset: 3,
                b_start_chunk_id: "b0".into(),
                b_start_offset: 0,
                b_end_chunk_id: "b0".into(),
                b_end_offset: 3,
                char_len: 3,
                sample_text: "a文本".into(),
            }],
        )
        .unwrap();
        // 一个聚类，成员命中锚点块 a2 → 应被反查关联。
        insert_clusters(
            &conn,
            "j1",
            &[NewCluster {
                cluster_type: "same".into(),
                topic: Some("施工组织".into()),
                summary: None,
                severity: "medium".into(),
                score: 0.9,
                section_kind: Some("tech".into()),
                conflict_json: None,
                base_section_path: None,
                base_page: None,
                exempt_reason: None,
                multi_doc_anomaly: false,
                members: vec![
                    NewMember {
                        document_id: "d1".into(),
                        chunk_id: "a2".into(),
                        role: "primary".into(),
                        score: Some(0.9),
                    },
                    NewMember {
                        document_id: "d2".into(),
                        chunk_id: "b2".into(),
                        role: "duplicate_candidate".into(),
                        score: Some(0.9),
                    },
                ],
                diffs: vec![],
            }],
        )
        .unwrap();

        let seg_id: String =
            conn.query_row("SELECT id FROM aligned_segments", [], |r| r.get(0)).unwrap();
        let d = get_segment_detail(&conn, &seg_id).unwrap();
        assert_eq!(d.segment.doc_a_id, "d1");
        assert_eq!(d.a_chunks.len(), 4, "A 侧跨度 order 0..3 共 4 段");
        assert_eq!(d.b_chunks.len(), 4);
        assert_eq!(d.a_chunks[0].chunk_id, "a0");
        assert_eq!(d.anchors.len(), 2);
        assert_eq!(d.verbatims.len(), 1, "落在跨度内的逐字区间");
        assert_eq!(d.diffs.len(), 1);
        assert_eq!(d.diffs[0].diff_type, "gap-sentence");
        assert_eq!(d.cluster_ids.len(), 1, "锚点块 a2 反查到关联聚类");
    }

    #[test]
    fn get_segment_detail_missing_returns_not_found() {
        let conn = setup();
        assert!(get_segment_detail(&conn, "nope").is_err());
    }

    #[test]
    fn export_rows_carry_pages_verbatim_and_tender_flag() {
        let conn = setup();
        insert_segments(&conn, "j1", &[sample_segment()]).unwrap();
        insert_verbatim_matches(
            &conn,
            "j1",
            &[NewVerbatim {
                doc_a_id: "d1".into(),
                doc_b_id: "d2".into(),
                a_start_chunk_id: "a0".into(),
                a_start_offset: 0,
                a_end_chunk_id: "a0".into(),
                a_end_offset: 3,
                b_start_chunk_id: "b0".into(),
                b_start_offset: 0,
                b_end_chunk_id: "b0".into(),
                b_end_offset: 3,
                char_len: 3,
                sample_text: "a文本".into(),
            }],
        )
        .unwrap();

        // 区段摘要（方向无关）：默认无豁免 → tender_quote=false，页码/覆盖率/锚点/逐字字数带出。
        let segs = list_segments_for_export(&conn, "j1", "d2", "d1").unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].verbatim_chars, 30);
        assert_eq!(segs[0].anchor_count, 2);
        assert_eq!(segs[0].a_page_start, Some(3));
        assert!(!segs[0].tender_quote, "无招标豁免块 → 不标注引用招标文件");

        // 逐字清单：起块 JOIN 出页码（setup 中所有块 page=1），char_len 与样本带出。
        let vs = list_verbatims_for_export(&conn, "j1", "d1", "d2").unwrap();
        assert_eq!(vs.len(), 1);
        assert_eq!(vs[0].char_len, 3);
        assert_eq!(vs[0].a_page, Some(1));
        assert!(!vs[0].tender_quote);

        // 招标豁免块（a1 落在区段行序跨度 0..3；a0 为逐字起块）→ 两侧标注均翻真。
        conn.execute(
            "INSERT INTO chunk_exemptions (job_id, chunk_id, kind, coverage, spans_json)
             VALUES ('j1','a1','tender',0.9,NULL), ('j1','a0','tender',0.95,NULL)",
            [],
        )
        .unwrap();
        let segs = list_segments_for_export(&conn, "j1", "d1", "d2").unwrap();
        assert!(segs[0].tender_quote, "区段跨度含 tender 豁免块 → 标注引用招标文件");
        let vs = list_verbatims_for_export(&conn, "j1", "d1", "d2").unwrap();
        assert!(vs[0].tender_quote, "逐字起块为 tender 豁免块 → 标注引用招标文件");

        // 不相干任务 → 空。
        assert!(list_segments_for_export(&conn, "jX", "d1", "d2").unwrap().is_empty());
        assert!(list_verbatims_for_export(&conn, "jX", "d1", "d2").unwrap().is_empty());
    }

    #[test]
    fn segments_for_cluster_reverse_links_and_empty_for_unrelated() {
        let conn = setup();
        insert_segments(&conn, "j1", &[sample_segment()]).unwrap();
        // 聚类成员命中锚点块 a2（sample_segment 的 edge 锚点）→ 应反查到该区段。
        insert_clusters(
            &conn,
            "j1",
            &[NewCluster {
                cluster_type: "same".into(),
                topic: Some("施工组织".into()),
                summary: None,
                severity: "medium".into(),
                score: 0.9,
                section_kind: Some("tech".into()),
                conflict_json: None,
                base_section_path: None,
                base_page: None,
                exempt_reason: None,
                multi_doc_anomaly: false,
                members: vec![
                    NewMember {
                        document_id: "d1".into(),
                        chunk_id: "a2".into(),
                        role: "primary".into(),
                        score: Some(0.9),
                    },
                    NewMember {
                        document_id: "d2".into(),
                        chunk_id: "b2".into(),
                        role: "duplicate_candidate".into(),
                        score: Some(0.9),
                    },
                ],
                diffs: vec![],
            }],
        )
        .unwrap();
        let cid: String =
            conn.query_row("SELECT id FROM clusters", [], |r| r.get(0)).unwrap();
        let refs = segments_for_cluster(&conn, &cid).unwrap();
        assert_eq!(refs.len(), 1, "聚类成员 a2 反查到关联区段");
        assert_eq!(refs[0].verbatim_chars, 30);
        assert!((refs[0].a_coverage - 0.82).abs() < 1e-6);
        // 不相干聚类 id → 空数组（旧任务/无区段亦然）。
        assert!(segments_for_cluster(&conn, "nope").unwrap().is_empty());
    }
}
