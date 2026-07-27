// 导出服务：从 DB 装配 ExportData（一次 join 取全聚类，逐对明细复用 pair_texts + 即时 diff），
// 再分发给目标格式写器。
use crate::db::repo::{compare_repo, document_repo, job_repo, segment_repo};
use crate::db::now_iso;
use crate::engine::diff::graded_diff;
use crate::engine::fact::FactConflict;
use crate::engine::report::{Collusion, DocInfo, EvasionSummary, Fingerprint, PairDetail, SectionStat, SegMatch, SharedTerm};
use crate::engine::fingerprint;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::export::data::{
    EvasionDoc, EvasionSection, ExportCluster, ExportData, ExportDoc, ExportMember, ForensicDoc,
    ForensicHit, ForensicSection, MethodsAndLimitations, SegmentEntry, SegmentPair,
    SegmentsSection, VerbatimEntry,
};
use crate::export::{self};
use crate::services::compare_service::CompareSummary;
use jieba_rs::Jieba;
use std::collections::HashMap;

const REPORT_VERSION: &str = "2.0";

pub fn export_to(
    conn: &rusqlite::Connection,
    jieba: &Jieba,
    job_id: &str,
    format: &str,
    path: &str,
    include_raw_text: Option<bool>,
    include_config: Option<bool>,
) -> AppResult<()> {
    let data = assemble(conn, jieba, job_id, include_raw_text, include_config)?;
    export::write(&data, format, path)
}

pub fn assemble(
    conn: &rusqlite::Connection,
    jieba: &Jieba,
    job_id: &str,
    include_raw_text: Option<bool>,
    include_config: Option<bool>,
) -> AppResult<ExportData> {
    let job = job_repo::get(conn, job_id)?;
    if job.status != "completed" {
        return Err(AppError::new(AppErrorCode::ExportFailed, "任务尚未完成，无法导出"));
    }
    let config: serde_json::Value = serde_json::from_str(&job.config_json).unwrap_or_default();
    let doc_ids: Vec<String> = config["documentIds"]
        .as_array()
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    let idx_of: HashMap<&str, usize> =
        doc_ids.iter().enumerate().map(|(i, id)| (id.as_str(), i)).collect();

    // 文档 + 指纹交叉标记（与报告页一致）
    let mut doc_infos: Vec<DocInfo> = Vec::new();
    let mut docs_meta: Vec<(i64, i64, Option<String>)> = Vec::new(); // pages, chars, method
    for id in &doc_ids {
        let d = document_repo::get(conn, id)?;
        doc_infos.push(DocInfo {
            id: d.id.clone(),
            name: d.file_name.clone(),
            doc_type: d.file_type.clone(),
            pages: d.page_count.unwrap_or(0) as u32,
            char_count: d.char_count.unwrap_or(0) as usize,
            fingerprint: d
                .fingerprint_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Fingerprint>(s).ok())
                .unwrap_or_default(),
            parse_error: None,
            evasion: d.evasion_json.as_deref().and_then(EvasionSummary::from_evasion_json),
        });
        docs_meta.push((d.page_count.unwrap_or(0), d.char_count.unwrap_or(0), d.parse_method));
    }
    fingerprint::cross_flags(&mut doc_infos);
    // 与报告页同一套风险标记口径：rsid 交集/PDF 血缘既写进导出的指纹表，也充当「取证证据」节的
    // 结构化逐对命中（围标分级仍取自落库 collusion_json；此处仅供导出逐对列示，不改判定）。
    let rsid_hits = fingerprint::rsid_pairs(&mut doc_infos, &Default::default());
    let lineage_hits = fingerprint::lineage_pairs(&mut doc_infos);
    let documents: Vec<ExportDoc> = doc_infos
        .iter()
        .zip(&docs_meta)
        .enumerate()
        .map(|(i, (d, meta))| ExportDoc {
            tag: crate::export::data_tag(i),
            name: d.name.clone(),
            file_type: d.doc_type.clone(),
            pages: meta.0,
            char_count: meta.1,
            parse_method: meta.2.clone(),
            risk_flags: d.fingerprint.risk_flags.clone(),
        })
        .collect();

    // 聚合结果 JSON
    let r = job_repo::get_result_jsons(conn, job_id)?;
    let summary: Option<CompareSummary> =
        r.summary_json.as_deref().and_then(|s| serde_json::from_str(s).ok());
    let collusion: Collusion = r
        .collusion_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let shared_terms: Vec<SharedTerm> = r
        .shared_terms_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let sections: Vec<SectionStat> = r
        .sections_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let (matrix, peak) = r
        .matrix_json
        .as_deref()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
        .map(|v| {
            (
                serde_json::from_value::<Vec<Vec<f32>>>(v["matrix"].clone()).unwrap_or_default(),
                v["peak"].as_f64().unwrap_or(0.0) as f32,
            )
        })
        .unwrap_or_default();

    // 聚合条款：平铺行折叠成 ExportCluster
    let mut clusters: Vec<ExportCluster> = Vec::new();
    let mut cur_id: Option<String> = None;
    for row in compare_repo::export_rows(conn, job_id)? {
        if cur_id.as_deref() != Some(row.cluster_id.as_str()) {
            cur_id = Some(row.cluster_id.clone());
            clusters.push(ExportCluster {
                index: clusters.len() + 1,
                cluster_type: row.cluster_type.clone(),
                severity: row.severity.clone(),
                topic: row.topic.clone(),
                summary: row.summary.clone(),
                score: row.score,
                review_status: row.review_status.clone(),
                section_kind: row.section_kind.clone(),
                conflict: row
                    .conflict_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<FactConflict>(s).ok()),
                exempt_reason: row.exempt_reason.clone(),
                multi_doc_anomaly: row.multi_doc_anomaly,
                members: Vec::new(),
            });
        }
        let doc = idx_of.get(row.document_id.as_str()).copied().unwrap_or(0);
        let section_path: Vec<String> = row
            .section_path
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        if let Some(c) = clusters.last_mut() {
            c.members.push(ExportMember {
                doc,
                tag: crate::export::data_tag(doc),
                text: row.text,
                page: row.page,
                section_path,
                role: row.role,
            });
        }
    }

    // 逐对明细（即时 diff，与逐对对比屏同源）
    let mut pairs: Vec<PairDetail> = Vec::new();
    for a in 0..doc_ids.len() {
        for b in (a + 1)..doc_ids.len() {
            let rows = compare_repo::pair_texts(conn, job_id, &doc_ids[a], &doc_ids[b])?;
            let matches: Vec<SegMatch> = rows
                .into_iter()
                .map(|(score, ta, tb)| {
                    let (_g, ops) = graded_diff(jieba, &ta, &tb);
                    SegMatch {
                        text_a: ta,
                        text_b: tb,
                        score: score as f32,
                        diff: ops,
                    }
                })
                .collect();
            let score = matrix
                .get(a)
                .and_then(|row| row.get(b))
                .copied()
                .unwrap_or(0.0);
            pairs.push(PairDetail { a, b, score, matches });
        }
    }

    // 附录 A 冻结节（M2 落地 HTML/JSON）：取证/规避从已装配的 doc_infos 与落库 collusion 派生；
    // 「检查方法与局限」§1.5 常驻（无条件）。
    let forensic = build_forensic(&doc_infos, &rsid_hits, &lineage_hits, &collusion);
    let evasion = build_evasion(&doc_infos);
    // M5「对齐区段与逐字证据」节：从 segment_repo 取区段摘要 + 逐字区间清单（含页码），
    // 逐对装配为天干标签口径。无区段/逐字 → None（不渲染空章节）。
    let segments = build_segments(conn, job_id, &doc_ids)?;
    let methods_and_limitations = MethodsAndLimitations::standard();

    let mut data = ExportData {
        report_version: REPORT_VERSION,
        app_version: env!("CARGO_PKG_VERSION"),
        generated_at: now_iso(),
        workspace_id: job.workspace_id.clone(),
        job_id: job.id.clone(),
        job_name: job.name.clone(),
        documents,
        config,
        summary,
        matrix,
        peak,
        collusion,
        shared_terms,
        sections,
        clusters,
        pairs,
        forensic,
        evasion,
        segments,
        methods_and_limitations,
    };
    apply_export_prefs(conn, &mut data, include_raw_text, include_config)?;
    Ok(data)
}

/// 导出偏好（内置 < 用户全局 < 工作区 < 本次导出覆盖）：
/// includeConfig=false → 报告不附比对配置快照；
/// includeRawText=false → 条款/逐对明细的正文截断为前 40 字摘要（保留可定位性，不含全文）。
/// override_* 为本次导出的临时覆盖（None 则沿用配置层）。
fn apply_export_prefs(
    conn: &rusqlite::Connection,
    data: &mut ExportData,
    override_raw_text: Option<bool>,
    override_config: Option<bool>,
) -> AppResult<()> {
    let user = crate::db::repo::settings_repo::get(conn, "config")?;
    let ws_patch = crate::db::repo::workspace_repo::get(conn, &data.workspace_id)
        .ok()
        .and_then(|w| w.settings_json)
        .and_then(|s| serde_json::from_str(&s).ok());
    let mut prefs = crate::config::resolve(user.as_ref(), ws_patch.as_ref(), None)?.export;
    if let Some(v) = override_raw_text {
        prefs.include_raw_text = v;
    }
    if let Some(v) = override_config {
        prefs.include_config = v;
    }

    if !prefs.include_config {
        data.config = serde_json::Value::Object(Default::default());
    }
    if !prefs.include_raw_text {
        let trim = |s: &mut String| {
            let cut: String = s.chars().take(40).collect();
            *s = if s.chars().count() > 40 { format!("{cut}…") } else { cut };
        };
        for cl in &mut data.clusters {
            for m in &mut cl.members {
                trim(&mut m.text);
            }
        }
        for p in &mut data.pairs {
            for m in &mut p.matches {
                trim(&mut m.text_a);
                trim(&mut m.text_b);
                m.diff.clear(); // diff 串含全文片段，一并省略
            }
        }
        // 逐字样本含全文片段：include_raw_text=false 时截断为前 40 字摘要（保留定位串/页码）。
        if let Some(seg) = &mut data.segments {
            for p in &mut seg.pairs {
                for v in &mut p.verbatims {
                    trim(&mut v.sample);
                }
            }
        }
    }
    Ok(())
}

/// 装配「对齐区段与逐字证据」节：逐对（i&lt;j）取区段摘要 + 逐字区间清单，按 (i,j) 朝向归一化两侧
/// 页码/章节（存储朝向可能相反），映射为天干标签口径。无任何区段/逐字 → None（不渲染空章节）。
fn build_segments(
    conn: &rusqlite::Connection,
    job_id: &str,
    doc_ids: &[String],
) -> AppResult<Option<SegmentsSection>> {
    let mut pairs: Vec<SegmentPair> = Vec::new();
    for i in 0..doc_ids.len() {
        for j in (i + 1)..doc_ids.len() {
            let di = &doc_ids[i];
            let dj = &doc_ids[j];
            let seg_rows = segment_repo::list_segments_for_export(conn, job_id, di, dj)?;
            let ver_rows = segment_repo::list_verbatims_for_export(conn, job_id, di, dj)?;
            if seg_rows.is_empty() && ver_rows.is_empty() {
                continue;
            }
            let segments: Vec<SegmentEntry> = seg_rows
                .iter()
                .map(|r| {
                    let fwd = &r.doc_a_id == di; // 存储朝向与 (i,j) 一致？否则两侧互换。
                    let (a_sec, a_ps, a_pe, b_sec, b_ps, b_pe) = if fwd {
                        (r.a_section_path.as_deref(), r.a_page_start, r.a_page_end,
                         r.b_section_path.as_deref(), r.b_page_start, r.b_page_end)
                    } else {
                        (r.b_section_path.as_deref(), r.b_page_start, r.b_page_end,
                         r.a_section_path.as_deref(), r.a_page_start, r.a_page_end)
                    };
                    SegmentEntry {
                        a_range: fmt_range(a_sec, a_ps, a_pe),
                        b_range: fmt_range(b_sec, b_ps, b_pe),
                        coverage: r.a_coverage.max(r.b_coverage),
                        verbatim_chars: r.verbatim_chars,
                        anchor_count: r.anchor_count,
                        tender_quote: r.tender_quote,
                    }
                })
                .collect();
            let verbatims: Vec<VerbatimEntry> = ver_rows
                .iter()
                .map(|r| {
                    let fwd = &r.doc_a_id == di;
                    let (a_page, a_sec, b_page, b_sec) = if fwd {
                        (r.a_page, r.a_section_path.as_deref(), r.b_page, r.b_section_path.as_deref())
                    } else {
                        (r.b_page, r.b_section_path.as_deref(), r.a_page, r.a_section_path.as_deref())
                    };
                    VerbatimEntry {
                        a_page,
                        b_page,
                        a_section: a_sec.map(str::to_string),
                        b_section: b_sec.map(str::to_string),
                        char_len: r.char_len,
                        sample: r.sample_text.clone(),
                        tender_quote: r.tender_quote,
                    }
                })
                .collect();
            pairs.push(SegmentPair {
                a: crate::export::data_tag(i),
                b: crate::export::data_tag(j),
                segments,
                verbatims,
            });
        }
    }
    Ok(if pairs.is_empty() { None } else { Some(SegmentsSection { pairs }) })
}

/// 区段/逐字两侧定位串：章节路径 + 页码范围 → 报告可读的可引用定位（含页码）。
fn fmt_range(section: Option<&str>, p_start: Option<i64>, p_end: Option<i64>) -> String {
    let sec = section.map(str::trim).filter(|s| !s.is_empty());
    let pages = match (p_start, p_end) {
        (Some(s), Some(e)) if s == e => Some(format!("第{s}页")),
        (Some(s), Some(e)) => Some(format!("第{s}–{e}页")),
        (Some(s), None) | (None, Some(s)) => Some(format!("第{s}页")),
        (None, None) => None,
    };
    match (sec, pages) {
        (Some(s), Some(p)) => format!("{s} · {p}"),
        (Some(s), None) => s.to_string(),
        (None, Some(p)) => p,
        (None, None) => "—".to_string(),
    }
}

/// 组装「取证证据」节：rsid/PDF 血缘用结构化逐对命中（含 hard/mid 分级与天干对），
/// 内嵌图片同源/共同错误取自落库 collusion 信号明细（逐对结构未在导出侧重算，明细含天干对与
/// 免责文案）。逐文档取证指纹（rsid 数/模板/血缘 GUID）无论是否有跨文档命中都列出。
/// None = 无任何取证命中（§1.5：不渲染空「取证证据」表，避免沉默背书）。
fn build_forensic(
    doc_infos: &[DocInfo],
    rsid_hits: &[fingerprint::RsidHit],
    lineage_hits: &[fingerprint::LineageHit],
    collusion: &Collusion,
) -> Option<ForensicSection> {
    let tag = crate::export::data_tag;
    let mut hits: Vec<ForensicHit> = Vec::new();
    for h in rsid_hits {
        hits.push(ForensicHit {
            kind: "rsid".into(),
            doc_a: tag(h.a),
            doc_b: tag(h.b),
            level: if h.root_match { "hard" } else { "mid" }.into(),
            detail: format!(
                "共享 {} 个 rsid 修订标识{}",
                h.shared_count,
                if h.root_match { "，rsidRoot 相同（高度指示派生自同一母文件）" } else { "" }
            ),
        });
    }
    for h in lineage_hits {
        hits.push(ForensicHit {
            kind: "pdfLineage".into(),
            doc_a: tag(h.a),
            doc_b: tag(h.b),
            level: if h.is_hard() { "hard" } else { "mid" }.into(),
            detail: if h.is_hard() {
                format!("{}（同一母文件）", h.hard_evidence.join("、"))
            } else {
                format!("共享字体子集标签「{}」（同一次生成环境）", h.shared_subset_tags.join("、"))
            },
        });
    }
    // 内嵌图片同源 / 共同错误：逐对结构未在导出侧重算，取落库信号 detail（已含天干对与免责纪律）。
    for s in &collusion.signals {
        let level = match s.kind.as_str() {
            "imageReuse" => "mid",
            "sharedErrors" => "weak",
            _ => continue,
        };
        hits.push(ForensicHit {
            kind: s.kind.clone(),
            doc_a: String::new(),
            doc_b: String::new(),
            level: level.into(),
            detail: s.detail.clone(),
        });
    }
    if hits.is_empty() {
        return None; // 无跨文档取证命中：不渲染空表（逐文档指纹留待「检查方法与局限」交代已执行项）
    }
    let per_document: Vec<ForensicDoc> = doc_infos
        .iter()
        .enumerate()
        .map(|(i, d)| {
            let fp = &d.fingerprint;
            ForensicDoc {
                doc_id: d.id.clone(),
                tag: tag(i),
                rsid_count: fp.rsids.len(),
                template_name: fp.template_name.clone(),
                lineage: serde_json::json!({
                    "documentId": fp.xmp_document_id,
                    "idFirst": fp.pdf_id_first,
                    "derivedFrom": fp.xmp_derived_from,
                    "fontSubsetTags": fp.font_subset_tags,
                }),
            }
        })
        .collect();
    Some(ForensicSection { hits, per_document })
}

/// 组装「规避特征」节：仅列出达判级线（suspect/confirmed）的文档；none（弱发现未过线）不进表
/// （§1.5：不背书、不误报）。verdict 直接沿用 EvasionSummary 判级，counts 为其计数快照。
/// None = 无任何达线规避发现。
fn build_evasion(doc_infos: &[DocInfo]) -> Option<EvasionSection> {
    let tag = crate::export::data_tag;
    let per_document: Vec<EvasionDoc> = doc_infos
        .iter()
        .enumerate()
        .filter_map(|(i, d)| {
            let e = d.evasion.as_ref()?;
            if !e.is_flagged() {
                return None;
            }
            Some(EvasionDoc {
                doc_id: d.id.clone(),
                tag: tag(i),
                counts: serde_json::to_value(e).unwrap_or_default(),
                verdict: e.severity.clone(),
                evidence_kinds: e.evidence_kinds().iter().map(|s| s.to_string()).collect(),
            })
        })
        .collect();
    if per_document.is_empty() {
        return None;
    }
    Some(EvasionSection { per_document })
}

// 端到端：导入 → 比对（开事实冲突）→ 六格式导出，逐一断言含八类统计与冲突信息。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::db::repo::{job_repo as jr, settings_repo, workspace_repo};
    use crate::jobs::progress::CollectSink;
    use crate::jobs::JobCtx;
    use crate::services::compare_service::{self, CompareRunConfig};
    use crate::services::import_service;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Arc, Mutex};

    #[test]
    fn exports_all_six_formats_with_summary_and_conflict() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let jieba = Arc::new(jieba_rs::Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_export_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for (n, t) in [
            ("a.txt", "投标人投标报价为人民币12800000元整，工期180个日历日，按期支付服务费用。\n系统采用分层解耦的微服务总体架构设计方案。"),
            ("b.txt", "投标人投标报价为人民币12900000元整，工期180个日历日，按期支付服务费用。\n系统采用分层解耦的微服务总体架构设计方案。"),
        ] {
            let p = dir.join(n);
            std::fs::write(&p, t).unwrap();
            paths.push(p.to_string_lossy().into_owned());
        }
        let mk_ctx = |jt: &str| {
            let conn = pool.get().unwrap();
            let job = jr::create(&conn, &ws, jt, None, "{}").unwrap();
            drop(conn);
            JobCtx::for_test(
                job.id,
                jt.into(),
                pool.clone(),
                Arc::new(AtomicBool::new(false)),
                Arc::new(CollectSink::default()),
            )
        };
        let ictx = mk_ctx("import");
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            crate::db::repo::document_repo::list(&conn, &ws)
                .unwrap()
                .iter()
                .map(|d| d.id.clone())
                .collect()
        };
        let cfg = CompareRunConfig {
            document_ids: ids,
            base_document_id: None,
            chunk_level: "paragraph".into(),
            similarity_threshold: 0.5,
            candidate_top_k: 100,
            enable_semantic: false,
            enable_fact_conflict: true,
            ignore_templates: true,
            detect_moved_paragraph: true,
            scope: "full".into(),
            subtract_tender: true,
            embedding_model: "e5-small".into(),
            allow_model_download: false,
            verbatim_min_chars: 30,
            enable_alignment: true,
        };
        // 与 start_compare 一致：运行配置存入任务行（assemble 从这里取 documentIds）
        let cctx = {
            let conn = pool.get().unwrap();
            let job = jr::create(
                &conn,
                &ws,
                "compare",
                Some("导出测试"),
                &serde_json::to_string(&cfg).unwrap(),
            )
            .unwrap();
            drop(conn);
            JobCtx::for_test(
                job.id,
                "compare".into(),
                pool.clone(),
                Arc::new(AtomicBool::new(false)),
                Arc::new(CollectSink::default()),
            )
        };
        compare_service::run_compare(&cctx, jieba.clone(), Arc::new(Mutex::new(None)), &ws, &cfg)
            .unwrap();
        {
            let conn = pool.get().unwrap();
            jr::finish(&conn, &cctx.job_id, "completed", None, None).unwrap();
        }

        let conn = pool.get().unwrap();
        let data = assemble(&conn, &jieba, &cctx.job_id, None, None).unwrap();
        assert!(data.summary.is_some(), "应有八类统计");
        assert!(
            data.clusters.iter().any(|c| c.conflict.is_some()),
            "金额不同应产出冲突条款"
        );
        assert!(!data.pairs.is_empty() && !data.pairs[0].matches.is_empty());

        for fmt in crate::export::FORMATS {
            let ext = match *fmt {
                "markdown" => "md",
                other => other,
            };
            let p = dir.join(format!("report.{ext}"));
            crate::export::write(&data, fmt, p.to_str().unwrap()).unwrap();
            let bytes = std::fs::read(&p).unwrap();
            assert!(bytes.len() > 200, "{fmt} 报告过小：{}", bytes.len());
            match *fmt {
                "xlsx" | "docx" => assert_eq!(&bytes[0..2], b"PK", "{fmt} 应为 zip 包"),
                _ => {
                    let text = String::from_utf8_lossy(&bytes);
                    assert!(text.contains("12800000"), "{fmt} 应含冲突金额");
                    if *fmt != "csv" {
                        assert!(
                            text.contains("冲突") || text.contains("conflict"),
                            "{fmt} 应含冲突信息"
                        );
                    }
                }
            }
        }
        // docx 的 document.xml 里应能看到八类统计与冲突字样
        {
            let p = dir.join("report.docx");
            let f = std::fs::File::open(&p).unwrap();
            let mut z = zip::ZipArchive::new(f).unwrap();
            let mut xml = String::new();
            std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut xml)
                .unwrap();
            assert!(xml.contains("总览统计") && xml.contains("事实冲突"), "docx 应含升级结构");
        }
        // M2 附录 A：「检查方法与局限」§1.5 无条件常驻（验收：零命中仍在）；空表逻辑由下方
        // 合成用例确定性断言（本 e2e 的 txt 语料是否触发 sharedErrors 取决于信号阈值，不在此断言）。
        {
            let html = std::fs::read_to_string(dir.join("report.html")).unwrap();
            assert!(html.contains("检查方法与局限"), "HTML 应含常驻「检查方法与局限」章节");
            assert!(html.contains("未命中不构成清白证明"), "HTML 应含清白免责声明");
            let json = std::fs::read_to_string(dir.join("report.json")).unwrap();
            assert!(json.contains("methodsAndLimitations"), "JSON 应含常驻 methodsAndLimitations 节");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 附录 A 三节 golden（HTML/JSON）：直接构造带取证/规避命中的 ExportData 喂给写器——
    /// 与导入管线解耦、离线秒级，验证 forensic/evasion/methodsAndLimitations 三节的渲染与序列化。
    #[test]
    fn forensic_evasion_and_methods_sections_render_in_html_and_json() {
        use crate::engine::report::Collusion;
        let dir = std::env::temp_dir().join(format!("bg_forensic_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = |tag: &str, name: &str| ExportDoc {
            tag: tag.into(),
            name: name.into(),
            file_type: "docx".into(),
            pages: 10,
            char_count: 5000,
            parse_method: Some("docx".into()),
            risk_flags: vec![],
        };
        let data = ExportData {
            report_version: "2.0",
            app_version: "test",
            generated_at: "2026-07-11T00:00:00.000Z".into(),
            workspace_id: "w1".into(),
            job_id: "j1".into(),
            job_name: Some("取证测试".into()),
            documents: vec![doc("甲", "甲标.docx"), doc("乙", "乙标.docx")],
            config: serde_json::json!({}),
            summary: None,
            matrix: vec![vec![1.0, 0.9], vec![0.9, 1.0]],
            peak: 0.9,
            collusion: Collusion::default(),
            shared_terms: vec![],
            sections: vec![],
            clusters: vec![],
            pairs: vec![],
            forensic: Some(ForensicSection {
                hits: vec![
                    ForensicHit {
                        kind: "rsid".into(),
                        doc_a: "甲".into(),
                        doc_b: "乙".into(),
                        level: "hard".into(),
                        detail: "共享 5 个 rsid 修订标识，rsidRoot 相同".into(),
                    },
                    ForensicHit {
                        kind: "imageReuse".into(),
                        doc_a: String::new(),
                        doc_b: String::new(),
                        level: "mid".into(),
                        detail: "内嵌图片同源：「甲」第3页 ↔ 「乙」第5页".into(),
                    },
                ],
                per_document: vec![
                    ForensicDoc {
                        doc_id: "d1".into(),
                        tag: "甲".into(),
                        rsid_count: 5,
                        template_name: Some("投标模板.dotx".into()),
                        lineage: serde_json::json!({ "documentId": "uuid:ABC", "fontSubsetTags": ["ABCDEF+SimSun"] }),
                    },
                    ForensicDoc {
                        doc_id: "d2".into(),
                        tag: "乙".into(),
                        rsid_count: 5,
                        template_name: None,
                        lineage: serde_json::json!({}),
                    },
                ],
            }),
            evasion: Some(EvasionSection {
                per_document: vec![EvasionDoc {
                    doc_id: "d1".into(),
                    tag: "甲".into(),
                    counts: serde_json::json!({ "zeroWidth": 12 }),
                    verdict: "confirmed".into(),
                    evidence_kinds: vec!["隐形码点".into()],
                }],
            }),
            segments: None,
            methods_and_limitations: MethodsAndLimitations::standard(),
        };

        let hp = dir.join("f.html");
        crate::export::write(&data, "html", hp.to_str().unwrap()).unwrap();
        let html = std::fs::read_to_string(&hp).unwrap();
        assert!(html.contains("取证证据"), "HTML 应含「取证证据」标题");
        assert!(html.contains("规避特征复核"), "HTML 应含规避复核表");
        assert!(html.contains("检查方法与局限"), "HTML 应含常驻方法与局限");
        assert!(html.contains("投标模板.dotx"), "逐文档取证指纹应列出模板名");
        assert!(html.contains("隐形码点"), "规避表应列出证据种类");
        assert!(html.contains("未命中不构成清白证明"), "应含清白免责声明");

        let jp = dir.join("f.json");
        crate::export::write(&data, "json", jp.to_str().unwrap()).unwrap();
        let json = std::fs::read_to_string(&jp).unwrap();
        assert!(json.contains("\"forensic\""), "JSON 应含 forensic 节");
        assert!(json.contains("\"rsidCount\": 5"), "forensic.perDocument 应含 rsidCount");
        assert!(json.contains("\"evasion\""), "JSON 应含 evasion 节");
        assert!(json.contains("\"verdict\": \"confirmed\""), "evasion 应含判级 confirmed");
        assert!(json.contains("methodsAndLimitations"), "JSON 应含常驻 methodsAndLimitations");

        // 空态（验收 4）：forensic/evasion 缺省 → 不渲染空表，但方法与局限仍常驻。
        let empty = ExportData {
            forensic: None,
            evasion: None,
            documents: vec![doc("甲", "甲标.docx"), doc("乙", "乙标.docx")],
            job_name: Some("空态".into()),
            ..data
        };
        let ehp = dir.join("empty.html");
        crate::export::write(&empty, "html", ehp.to_str().unwrap()).unwrap();
        let ehtml = std::fs::read_to_string(&ehp).unwrap();
        assert!(!ehtml.contains("取证证据"), "无取证命中不渲染「取证证据」表");
        assert!(!ehtml.contains("规避特征复核"), "无规避命中不渲染规避表");
        assert!(ehtml.contains("检查方法与局限"), "方法与局限仍常驻");
        let ejp = dir.join("empty.json");
        crate::export::write(&empty, "json", ejp.to_str().unwrap()).unwrap();
        let ejson = std::fs::read_to_string(&ejp).unwrap();
        assert!(!ejson.contains("\"forensic\""), "空态 JSON 不含 forensic 节");
        assert!(!ejson.contains("\"evasion\""), "空态 JSON 不含 evasion 节");
        assert!(ejson.contains("methodsAndLimitations"), "空态 JSON 仍含 methodsAndLimitations");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 附录 A segments 节 golden（HTML/DOCX/JSON）：直接构造带对齐区段 + 逐字区间的 ExportData 喂给
    /// 写器——离线秒级，验证「对齐区段与逐字证据」章节（区段摘要表 + 逐字清单含页码 + 招标引用标注）
    /// 在两主格式渲染、JSON 随 serde 顺带、空态省略、且写器纯函数（同数据两次导出内容一致）。
    #[test]
    fn segments_section_renders_in_html_docx_json_and_omits_when_empty() {
        use crate::engine::report::Collusion;
        let dir = std::env::temp_dir().join(format!("bg_segments_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc = |tag: &str, name: &str| ExportDoc {
            tag: tag.into(),
            name: name.into(),
            file_type: "docx".into(),
            pages: 12,
            char_count: 6000,
            parse_method: Some("docx".into()),
            risk_flags: vec![],
        };
        let base = |segments: Option<SegmentsSection>| ExportData {
            report_version: "2.0",
            app_version: "test",
            generated_at: "2026-07-11T00:00:00.000Z".into(),
            workspace_id: "w1".into(),
            job_id: "j1".into(),
            job_name: Some("区段导出".into()),
            documents: vec![doc("甲", "甲标.docx"), doc("乙", "乙标.docx")],
            config: serde_json::json!({}),
            summary: None,
            matrix: vec![vec![1.0, 0.9], vec![0.9, 1.0]],
            peak: 0.9,
            collusion: Collusion::default(),
            shared_terms: vec![],
            sections: vec![],
            clusters: vec![],
            pairs: vec![],
            forensic: None,
            evasion: None,
            segments,
            methods_and_limitations: MethodsAndLimitations::standard(),
        };
        let data = base(Some(SegmentsSection {
            pairs: vec![SegmentPair {
                a: "甲".into(),
                b: "乙".into(),
                segments: vec![
                    SegmentEntry {
                        a_range: "第三章 › 3.2 施工组织 · 第3–5页".into(),
                        b_range: "第三章 › 3.2 施工组织 · 第4–6页".into(),
                        coverage: 0.82,
                        verbatim_chars: 620,
                        anchor_count: 14,
                        tender_quote: false,
                    },
                    SegmentEntry {
                        a_range: "第五章 › 5.1 质量保证 · 第9页".into(),
                        b_range: "第五章 › 5.1 质量保证 · 第9页".into(),
                        coverage: 0.5,
                        verbatim_chars: 0,
                        anchor_count: 3,
                        tender_quote: true,
                    },
                ],
                verbatims: vec![VerbatimEntry {
                    a_page: Some(3),
                    b_page: Some(4),
                    a_section: Some("3.2 施工组织".into()),
                    b_section: Some("3.2 施工组织".into()),
                    char_len: 620,
                    sample: "施工现场实行封闭管理并设置专职安全员全程旁站监督".into(),
                    tender_quote: false,
                }],
            }],
        }));

        let hp = dir.join("s.html");
        crate::export::write(&data, "html", hp.to_str().unwrap()).unwrap();
        let html = std::fs::read_to_string(&hp).unwrap();
        assert!(html.contains("对齐区段与逐字证据"), "HTML 应含章节标题");
        assert!(html.contains("甲 × 乙"), "HTML 应含文档对小标题");
        assert!(html.contains("3.2 施工组织"), "HTML 区段摘要应含定位（章节+页码）");
        assert!(html.contains("82%"), "HTML 区段摘要应含覆盖率");
        assert!(html.contains("施工现场实行封闭管理"), "HTML 逐字清单应含逐字样本");
        assert!(html.contains("引用招标文件"), "HTML 应对招标豁免区段标注引用招标文件");
        assert!(html.contains("深红＝逐字铁证"), "HTML 应含三级视觉图例");

        // 写器为纯函数：同数据两次导出 HTML 字节一致（同任务两次导出内容确定）。
        let hp2 = dir.join("s2.html");
        crate::export::write(&data, "html", hp2.to_str().unwrap()).unwrap();
        assert_eq!(html, std::fs::read_to_string(&hp2).unwrap(), "同数据两次导出 HTML 应一致");

        let dp = dir.join("s.docx");
        crate::export::write(&data, "docx", dp.to_str().unwrap()).unwrap();
        let read_docx_xml = |path: &std::path::Path| {
            let f = std::fs::File::open(path).unwrap();
            let mut z = zip::ZipArchive::new(f).unwrap();
            let mut xml = String::new();
            std::io::Read::read_to_string(&mut z.by_name("word/document.xml").unwrap(), &mut xml)
                .unwrap();
            xml
        };
        let dxml = read_docx_xml(&dp);
        assert!(dxml.contains("对齐区段与逐字证据"), "DOCX 应含章节标题");
        assert!(dxml.contains("施工现场实行封闭管理"), "DOCX 逐字清单应含逐字样本");
        assert!(dxml.contains("引用招标文件"), "DOCX 应对招标豁免区段标注");
        let dp2 = dir.join("s2.docx");
        crate::export::write(&data, "docx", dp2.to_str().unwrap()).unwrap();
        assert_eq!(dxml, read_docx_xml(&dp2), "同数据两次导出 DOCX 内容应一致");

        let jp = dir.join("s.json");
        crate::export::write(&data, "json", jp.to_str().unwrap()).unwrap();
        let json = std::fs::read_to_string(&jp).unwrap();
        assert!(json.contains("\"segments\""), "JSON 应含 segments 节");
        assert!(json.contains("\"aRange\""), "segments 应含 aRange 字段");
        assert!(json.contains("\"verbatimChars\": 620"), "segments 应含 verbatimChars");
        assert!(json.contains("\"verbatims\""), "segments.pair 应含逐字区间清单");

        // 空态（验收）：无区段 → 章节整体省略但不报错；JSON 不含 segments 键，方法与局限仍常驻。
        let empty = base(None);
        let ehp = dir.join("empty.html");
        crate::export::write(&empty, "html", ehp.to_str().unwrap()).unwrap();
        let ehtml = std::fs::read_to_string(&ehp).unwrap();
        assert!(!ehtml.contains("对齐区段与逐字证据"), "无区段不渲染该章节");
        assert!(ehtml.contains("检查方法与局限"), "方法与局限仍常驻");
        let ejp = dir.join("empty.json");
        crate::export::write(&empty, "json", ejp.to_str().unwrap()).unwrap();
        let ejson = std::fs::read_to_string(&ejp).unwrap();
        assert!(!ejson.contains("\"segments\""), "空态 JSON 不含 segments 节");
        let edp = dir.join("empty.docx");
        crate::export::write(&empty, "docx", edp.to_str().unwrap()).unwrap();
        assert!(!read_docx_xml(&edp).contains("对齐区段与逐字证据"), "空态 DOCX 不含该章节");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// build_segments 的存储朝向归一化 + 天干映射：区段以反向朝向（doc_a=d2, doc_b=d1）落库，
    /// 装配须归一化到 (i=d1, j=d2)——a 侧定位/页码对应 d1，逐字页码两侧对应正确。
    #[test]
    fn build_segments_normalizes_orientation_and_maps_tags() {
        use crate::db::repo::compare_repo::{
            insert_segments, insert_verbatim_matches, NewSegment, NewSegmentAnchor, NewVerbatim,
        };
        use rusqlite::params;
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = workspace_repo::create(&conn, "w").unwrap().id;
        for id in ["d1", "d2"] {
            conn.execute(
                "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type,
                 status, created_at, updated_at) VALUES (?1,?2,'f','p',?1,'docx','parsed','t','t')",
                params![id, ws],
            )
            .unwrap();
        }
        let job = jr::create(&conn, &ws, "compare", None, "{}").unwrap();
        // d1 页码 order+1，d2 页码 order+11（两侧可区分）。
        for (doc, prefix, base) in [("d1", "a", 1i64), ("d2", "b", 11i64)] {
            for i in 0..4 {
                conn.execute(
                    "INSERT INTO chunks (id, document_id, chunk_type, chunk_level, text,
                     normalized_text, char_count, page, order_index, created_at)
                     VALUES (?1,?2,'paragraph','paragraph',?3,?3,10,?4,?5,'t')",
                    params![format!("{prefix}{i}"), doc, format!("{prefix}{i}"), base + i, i],
                )
                .unwrap();
            }
        }
        // 反向存储：doc_a=d2（乙章/页11–14），doc_b=d1（甲章/页1–4）。
        insert_segments(
            &conn,
            &job.id,
            &[NewSegment {
                doc_a_id: "d2".into(),
                doc_b_id: "d1".into(),
                a_start_order: 0,
                a_end_order: 3,
                b_start_order: 0,
                b_end_order: 3,
                a_start_chunk_id: "b0".into(),
                a_end_chunk_id: "b3".into(),
                b_start_chunk_id: "a0".into(),
                b_end_chunk_id: "a3".into(),
                anchor_count: 2,
                verbatim_chars: 30,
                a_covered_chars: 30,
                b_covered_chars: 30,
                a_coverage: 0.7,
                b_coverage: 0.82,
                avg_score: 0.9,
                a_section_path: Some("乙章".into()),
                b_section_path: Some("甲章".into()),
                a_page_start: Some(11),
                a_page_end: Some(14),
                b_page_start: Some(1),
                b_page_end: Some(4),
                anchors: vec![NewSegmentAnchor {
                    a_chunk_id: "b0".into(),
                    b_chunk_id: "a0".into(),
                    kind: "edge".into(),
                    score: 0.9,
                }],
                diffs: vec![],
            }],
        )
        .unwrap();
        insert_verbatim_matches(
            &conn,
            &job.id,
            &[NewVerbatim {
                doc_a_id: "d2".into(),
                doc_b_id: "d1".into(),
                a_start_chunk_id: "b1".into(),
                a_start_offset: 0,
                a_end_chunk_id: "b1".into(),
                a_end_offset: 3,
                b_start_chunk_id: "a1".into(),
                b_start_offset: 0,
                b_end_chunk_id: "a1".into(),
                b_end_offset: 3,
                char_len: 30,
                sample_text: "逐字样本".into(),
            }],
        )
        .unwrap();

        let doc_ids = vec!["d1".to_string(), "d2".to_string()];
        let seg = build_segments(&conn, &job.id, &doc_ids).unwrap().expect("应装配出区段节");
        assert_eq!(seg.pairs.len(), 1);
        let p = &seg.pairs[0];
        assert_eq!((p.a.as_str(), p.b.as_str()), ("甲", "乙"), "天干按 doc_ids 位次");
        // a 侧归一化为 d1（甲章/第1–4页），尽管落库朝向相反。
        assert!(p.segments[0].a_range.contains("甲章"), "a 侧应归一化为 d1：{}", p.segments[0].a_range);
        assert!(p.segments[0].a_range.contains("第1–4页"));
        assert!(p.segments[0].b_range.contains("乙章"));
        assert!((p.segments[0].coverage - 0.82).abs() < 1e-6, "覆盖取双向较大值");
        // 逐字页码归一化：a 侧=d1(a1 页2)，b 侧=d2(b1 页12)。
        assert_eq!(p.verbatims[0].a_page, Some(2));
        assert_eq!(p.verbatims[0].b_page, Some(12));

        // 无区段任务 → None（不渲染空章节）。
        let empty_job = jr::create(&conn, &ws, "compare", None, "{}").unwrap();
        assert!(build_segments(&conn, &empty_job.id, &doc_ids).unwrap().is_none());
    }

    /// 配置四层在 DB 链路上的覆盖关系：工作区 patch 覆盖用户全局，任务请求再覆盖工作区。
    #[test]
    fn config_layering_through_db() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = workspace_repo::create(&conn, "w").unwrap();

        settings_repo::set(
            &conn,
            "config",
            &serde_json::json!({ "compare": { "similarityThreshold": 0.5, "scope": "tech" } }),
        )
        .unwrap();
        workspace_repo::set_settings(
            &conn,
            &ws.id,
            Some(r#"{"compare":{"similarityThreshold":0.6}}"#),
        )
        .unwrap();

        let user = settings_repo::get(&conn, "config").unwrap();
        let ws_row = workspace_repo::get(&conn, &ws.id).unwrap();
        let ws_patch: serde_json::Value =
            serde_json::from_str(ws_row.settings_json.as_deref().unwrap()).unwrap();

        // 无任务层：工作区 0.6 覆盖全局 0.5；scope 沿用全局 tech
        let c = crate::config::resolve(user.as_ref(), Some(&ws_patch), None).unwrap();
        assert_eq!(c.compare.similarity_threshold, 0.6);
        assert_eq!(c.compare.scope, "tech");

        // 任务层再覆盖
        let task = serde_json::json!({ "compare": { "similarityThreshold": 0.8 } });
        let c = crate::config::resolve(user.as_ref(), Some(&ws_patch), Some(&task)).unwrap();
        assert_eq!(c.compare.similarity_threshold, 0.8);
    }
}
