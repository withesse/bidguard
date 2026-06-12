// 文档 commands：导入（任务化，立即返回 JobRow）、列表、预览、删除。
use super::{conn, effective_config};
use crate::db::repo::{chunk_repo, document_repo, workspace_repo};
use crate::db::repo::chunk_repo::ChunkRow;
use crate::db::repo::document_repo::DocumentRow;
use crate::db::repo::job_repo::JobRow;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::jobs::progress::TauriEventSink;
use crate::services::import_service;
use crate::state::AppState;
use serde::Serialize;
use std::sync::Arc;
use tauri::State;

/// 启动导入任务：立即返回 pending JobRow，进度走 document:import:* 全局事件。
#[tauri::command]
pub async fn import_documents(
    workspace_id: String,
    paths: Vec<String>,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> AppResult<JobRow> {
    if paths.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "未选择任何文件"));
    }
    // 工作区必须存在（同时把 NotFound 提前到任务创建前）
    workspace_repo::get(&*conn(&state)?, &workspace_id)?;
    // 导入期生效配置（归一开关/表格识别/页码/页眉清理/最短段长）在任务创建时快照
    let opts = import_service::ImportOptions::from_config(&effective_config(&state, &workspace_id)?);

    let jieba = state.jieba();
    let sink = Arc::new(TauriEventSink::new(app));
    let config_json = serde_json::json!({ "paths": &paths }).to_string();
    let ws = workspace_id.clone();
    state.jobs.spawn(
        &state.db,
        sink,
        &workspace_id,
        "import",
        Some("导入文档"),
        &config_json,
        move |ctx| import_service::run_import(ctx, jieba, &ws, &paths, &opts),
    )
}

#[tauri::command]
pub async fn list_documents(
    workspace_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<DocumentRow>> {
    document_repo::list(&*conn(&state)?, &workspace_id)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentPreview {
    pub document: DocumentRow,
    pub chunks: Vec<ChunkRow>,
}

#[tauri::command]
pub async fn get_document_preview(
    document_id: String,
    state: State<'_, AppState>,
) -> AppResult<DocumentPreview> {
    let c = conn(&state)?;
    let document = document_repo::get(&c, &document_id)?;
    // 预览屏前端虚拟滚动，段落级分块一次取回（5000 块 ≈ 百万字文档）
    let chunks = chunk_repo::preview(&c, &document_id, 5000)?;
    Ok(DocumentPreview { document, chunks })
}

#[tauri::command]
pub async fn remove_document(document_id: String, state: State<'_, AppState>) -> AppResult<()> {
    document_repo::remove(&*conn(&state)?, &document_id)
}

/// 原始文件字节（原文版式预览：pdf.js / docx-preview 的数据源）。
/// 按文档 id 取路径再读盘——比放开 asset 协议全盘 scope 更收敛。
#[tauri::command]
pub async fn read_document_file(
    document_id: String,
    state: State<'_, AppState>,
) -> AppResult<tauri::ipc::Response> {
    let d = document_repo::get(&*conn(&state)?, &document_id)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || std::fs::read(&d.file_path))
        .await
        .map_err(|e| AppError::new(AppErrorCode::Unknown, "读取任务失败").with_detail(e.to_string()))?
        .map_err(|e| {
            AppError::new(AppErrorCode::FileNotFound, "原文件不存在或不可读（可能已移动/删除）")
                .with_detail(e.to_string())
        })?;
    Ok(tauri::ipc::Response::new(bytes))
}

/// 扫描件 OCR 行级版面 JSON（每页一组归一化 {t,x,y,w,h}）；非扫描件返回 null。
#[tauri::command]
pub async fn get_document_ocr_layout(
    document_id: String,
    state: State<'_, AppState>,
) -> AppResult<Option<String>> {
    document_repo::get_ocr_layout(&*conn(&state)?, &document_id)
}
