// 比对 commands：发起比对（任务化）、总览、聚合分页、详情、人工确认、成对明细。
use super::{conn, effective_config};
use crate::config::{MAX_DOCS, MIN_DOCS};
use crate::db::repo::compare_repo::{self, ClusterDetail, ClusterFilter, ClusterSummaryRow};
use crate::db::repo::{document_repo, job_repo};
use crate::db::repo::document_repo::DocumentRow;
use crate::db::repo::job_repo::JobRow;
use crate::engine::diff::graded_diff;
use crate::engine::report::DiffOp;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::jobs::progress::TauriEventSink;
use crate::services::compare_service::{self, CompareRunConfig};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

/// 单次比对请求：未给的字段回落到「内置 < 用户全局 < 工作区」合并出的默认值。
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CompareRequest {
    pub document_ids: Vec<String>,
    pub name: Option<String>,
    pub base_document_id: Option<String>,
    pub chunk_level: Option<String>,
    pub similarity_threshold: Option<f32>,
    pub candidate_top_k: Option<usize>,
    pub enable_semantic: Option<bool>,
    pub enable_fact_conflict: Option<bool>,
    pub ignore_templates: Option<bool>,
    pub detect_moved_paragraph: Option<bool>,
    pub scope: Option<String>,
}

#[tauri::command]
pub async fn start_compare(
    workspace_id: String,
    request: CompareRequest,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<JobRow> {
    // 文档数与去重校验（保持请求顺序，顺序即十天干位次）
    let mut seen = std::collections::HashSet::new();
    let ids: Vec<String> = request
        .document_ids
        .iter()
        .filter(|id| seen.insert(id.as_str().to_string()))
        .cloned()
        .collect();
    if ids.len() < MIN_DOCS || ids.len() > MAX_DOCS {
        return Err(AppError::new(
            AppErrorCode::InvalidConfig,
            format!("参与比对的文档数需在 {MIN_DOCS}-{MAX_DOCS} 份之间"),
        ));
    }
    if let Some(base) = &request.base_document_id {
        if !ids.contains(base) {
            return Err(AppError::new(AppErrorCode::InvalidConfig, "基准文档必须在参评文档中"));
        }
    }

    let cfg_all = effective_config(&state, &workspace_id)?;
    let d = cfg_all.compare;
    let chunk_level = request.chunk_level.unwrap_or(d.default_chunk_level);
    if !matches!(chunk_level.as_str(), "section" | "paragraph" | "sentence") {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "分块粒度不合法"));
    }
    let scope = request.scope.unwrap_or(d.scope);
    if !matches!(scope.as_str(), "full" | "tech" | "business") {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "比对范围不合法"));
    }
    let run = CompareRunConfig {
        document_ids: ids.clone(),
        base_document_id: request.base_document_id,
        chunk_level,
        similarity_threshold: request
            .similarity_threshold
            .unwrap_or(d.similarity_threshold)
            .clamp(0.2, 0.99),
        candidate_top_k: request.candidate_top_k.unwrap_or(d.candidate_top_k).clamp(5, 1000),
        enable_semantic: request.enable_semantic.unwrap_or(d.enable_semantic),
        enable_fact_conflict: request.enable_fact_conflict.unwrap_or(d.enable_fact_conflict),
        ignore_templates: request.ignore_templates.unwrap_or(d.ignore_templates),
        detect_moved_paragraph: request
            .detect_moved_paragraph
            .unwrap_or(d.detect_moved_paragraph),
        scope,
        allow_model_download: cfg_all.security.allow_cloud_model,
    };
    let name = request
        .name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| format!("{} 份标书交叉比对", ids.len()));
    let config_json = serde_json::to_string(&run)
        .map_err(|e| AppError::new(AppErrorCode::InvalidConfig, "配置序列化失败").with_detail(e.to_string()))?;

    let jieba = state.jieba();
    let embedder = state.embedder();
    let sink = Arc::new(TauriEventSink::new(app));
    let ws = workspace_id.clone();
    state.jobs.spawn(
        &state.db,
        sink,
        &workspace_id,
        "compare",
        Some(&name),
        &config_json,
        move |ctx| compare_service::run_compare(ctx, jieba, embedder, &ws, &run),
    )
}

/// 总览：任务行 + 参评文档（按位次）+ 五块聚合 JSON。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSummaryDto {
    pub job: JobRow,
    pub documents: Vec<DocumentRow>,
    pub config: serde_json::Value,
    pub summary: Option<serde_json::Value>,
    pub matrix: Option<serde_json::Value>,
    pub collusion: Option<serde_json::Value>,
    pub shared_terms: Option<serde_json::Value>,
    pub sections: Option<serde_json::Value>,
}

#[tauri::command]
pub async fn get_compare_summary(
    job_id: String,
    state: State<'_, AppState>,
) -> AppResult<CompareSummaryDto> {
    let c = conn(&state)?;
    let job = job_repo::get(&c, &job_id)?;
    let config: serde_json::Value = serde_json::from_str(&job.config_json).unwrap_or_default();
    let documents = config["documentIds"]
        .as_array()
        .map(|ids| {
            ids.iter()
                .filter_map(|v| v.as_str())
                .filter_map(|id| document_repo::get(&c, id).ok())
                .collect()
        })
        .unwrap_or_default();
    let r = job_repo::get_result_jsons(&c, &job_id)?;
    let parse = |s: Option<String>| s.and_then(|x| serde_json::from_str(&x).ok());
    Ok(CompareSummaryDto {
        job,
        documents,
        config,
        summary: parse(r.summary_json),
        matrix: parse(r.matrix_json),
        collusion: parse(r.collusion_json),
        shared_terms: parse(r.shared_terms_json),
        sections: parse(r.sections_json),
    })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageResult<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub offset: i64,
    pub limit: i64,
}

#[tauri::command]
pub async fn list_clusters(
    job_id: String,
    filter: Option<ClusterFilter>,
    offset: Option<i64>,
    limit: Option<i64>,
    state: State<'_, AppState>,
) -> AppResult<PageResult<ClusterSummaryRow>> {
    let c = conn(&state)?;
    let filter = filter.unwrap_or_default();
    let offset = offset.unwrap_or(0).max(0);
    let limit = limit.unwrap_or(50).clamp(1, 500);
    let total = compare_repo::count_clusters(&c, &job_id, &filter)?;
    let items = compare_repo::list_clusters(&c, &job_id, &filter, offset, limit)?;
    Ok(PageResult { items, total, offset, limit })
}

#[tauri::command]
pub async fn get_cluster_detail(
    cluster_id: String,
    state: State<'_, AppState>,
) -> AppResult<ClusterDetail> {
    compare_repo::get_cluster_detail(&*conn(&state)?, &cluster_id)
}

#[tauri::command]
pub async fn set_cluster_review_status(
    cluster_id: String,
    status: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    if !matches!(status.as_str(), "pending" | "confirmed" | "ignored") {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "确认状态不合法"));
    }
    compare_repo::set_review_status(&*conn(&state)?, &cluster_id, &status)
}

/// 成对明细：两文档的 primary 段落对 + 即时分级 diff（喂逐对对比屏）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairMatch {
    pub text_a: String,
    pub text_b: String,
    pub score: f64,
    pub diff_type: String,
    pub diff: Vec<DiffOp>,
}

#[tauri::command]
pub async fn get_pair_detail(
    job_id: String,
    document_a: String,
    document_b: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<PairMatch>> {
    let jieba = state.jieba();
    let rows = compare_repo::pair_texts(&*conn(&state)?, &job_id, &document_a, &document_b)?;
    Ok(rows
        .into_iter()
        .map(|(score, a, b)| {
            let (granularity, ops) = graded_diff(&jieba, &a, &b);
            PairMatch {
                text_a: a,
                text_b: b,
                score,
                diff_type: granularity.to_string(),
                diff: ops,
            }
        })
        .collect())
}
