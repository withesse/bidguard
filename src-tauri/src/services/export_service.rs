// 导出服务：从 DB 装配 ExportData（一次 join 取全聚类，逐对明细复用 pair_texts + 即时 diff），
// 再分发给目标格式写器。
use crate::db::repo::{compare_repo, document_repo, job_repo};
use crate::db::now_iso;
use crate::engine::diff::graded_diff;
use crate::engine::fact::FactConflict;
use crate::engine::report::{Collusion, DocInfo, EvasionSummary, Fingerprint, PairDetail, SectionStat, SegMatch, SharedTerm};
use crate::engine::fingerprint;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::export::data::{
    EvasionDoc, EvasionSection, ExportCluster, ExportData, ExportDoc, ExportMember, ForensicDoc,
    ForensicHit, ForensicSection, MethodsAndLimitations,
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
    }
    Ok(())
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
            embedding_model: "e5-small".into(),
            allow_model_download: false,
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
