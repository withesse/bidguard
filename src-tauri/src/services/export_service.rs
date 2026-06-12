// 导出服务：从 DB 装配 ExportData（一次 join 取全聚类，逐对明细复用 pair_texts + 即时 diff），
// 再分发给目标格式写器。
use crate::db::repo::{compare_repo, document_repo, job_repo};
use crate::db::now_iso;
use crate::engine::diff::graded_diff;
use crate::engine::fact::FactConflict;
use crate::engine::report::{Collusion, DocInfo, Fingerprint, PairDetail, SectionStat, SegMatch, SharedTerm};
use crate::engine::fingerprint;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::export::data::{ExportCluster, ExportData, ExportDoc, ExportMember};
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
) -> AppResult<()> {
    let data = assemble(conn, jieba, job_id)?;
    export::write(&data, format, path)
}

pub fn assemble(
    conn: &rusqlite::Connection,
    jieba: &Jieba,
    job_id: &str,
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
        });
        docs_meta.push((d.page_count.unwrap_or(0), d.char_count.unwrap_or(0), d.parse_method));
    }
    fingerprint::cross_flags(&mut doc_infos);
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
    };
    apply_export_prefs(conn, &mut data)?;
    Ok(data)
}

/// 导出偏好（内置 < 用户全局 < 工作区）：
/// includeConfig=false → 报告不附比对配置快照；
/// includeRawText=false → 条款/逐对明细的正文截断为前 40 字摘要（保留可定位性，不含全文）。
fn apply_export_prefs(conn: &rusqlite::Connection, data: &mut ExportData) -> AppResult<()> {
    let user = crate::db::repo::settings_repo::get(conn, "config")?;
    let ws_patch = crate::db::repo::workspace_repo::get(conn, &data.workspace_id)
        .ok()
        .and_then(|w| w.settings_json)
        .and_then(|s| serde_json::from_str(&s).ok());
    let prefs = crate::config::resolve(user.as_ref(), ws_patch.as_ref(), None)?.export;

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
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default()).unwrap();
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
        let data = assemble(&conn, &jieba, &cctx.job_id).unwrap();
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
