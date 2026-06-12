// 批注 commands：增 / 列（按工作区）/ 改 / 删。原文件只读，批注独立存库。
use super::conn;
use crate::db::repo::annotation_repo::{self, AnnotationRow, NewAnnotation};
use crate::db::repo::workspace_repo;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::state::AppState;
use tauri::State;

#[allow(clippy::too_many_arguments)] // 锚点字段的固有集合（文档/分块/条款组/页/引文）
#[tauri::command]
pub async fn add_annotation(
    workspace_id: String,
    note: String,
    document_id: Option<String>,
    chunk_id: Option<String>,
    cluster_id: Option<String>,
    page: Option<i64>,
    quote: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<AnnotationRow> {
    let n = note.trim();
    if n.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "批注内容不能为空"));
    }
    let c = conn(&state)?;
    workspace_repo::get(&c, &workspace_id)?;
    annotation_repo::add(
        &c,
        &NewAnnotation {
            workspace_id: &workspace_id,
            document_id: document_id.as_deref(),
            chunk_id: chunk_id.as_deref(),
            cluster_id: cluster_id.as_deref(),
            page,
            quote: quote.as_deref(),
            note: n,
        },
    )
}

#[tauri::command]
pub async fn list_annotations(
    workspace_id: String,
    state: State<'_, AppState>,
) -> AppResult<Vec<AnnotationRow>> {
    annotation_repo::list_by_workspace(&*conn(&state)?, &workspace_id)
}

#[tauri::command]
pub async fn update_annotation(
    annotation_id: String,
    note: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let n = note.trim();
    if n.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "批注内容不能为空"));
    }
    annotation_repo::update_note(&*conn(&state)?, &annotation_id, n)
}

#[tauri::command]
pub async fn delete_annotation(annotation_id: String, state: State<'_, AppState>) -> AppResult<()> {
    annotation_repo::remove(&*conn(&state)?, &annotation_id)
}
