// 工作区 commands。
use super::conn;
use crate::db::repo::workspace_repo::{self, WorkspaceRow};
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::state::AppState;
use tauri::State;

fn valid_name(name: &str) -> AppResult<&str> {
    let n = name.trim();
    if n.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "名称不能为空"));
    }
    Ok(n)
}

#[tauri::command]
pub async fn create_workspace(name: String, state: State<'_, AppState>) -> AppResult<WorkspaceRow> {
    workspace_repo::create(&*conn(&state)?, valid_name(&name)?)
}

#[tauri::command]
pub async fn list_workspaces(state: State<'_, AppState>) -> AppResult<Vec<WorkspaceRow>> {
    workspace_repo::list(&*conn(&state)?)
}

#[tauri::command]
pub async fn get_workspace(workspace_id: String, state: State<'_, AppState>) -> AppResult<WorkspaceRow> {
    workspace_repo::get(&*conn(&state)?, &workspace_id)
}

#[tauri::command]
pub async fn rename_workspace(
    workspace_id: String,
    name: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    workspace_repo::rename(&*conn(&state)?, &workspace_id, valid_name(&name)?)
}

/// 工作区级配置 patch（JSON 文本，null 清除）。合法性在 config::resolve 层校验。
#[tauri::command]
pub async fn set_workspace_settings(
    workspace_id: String,
    settings_json: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<()> {
    if let Some(s) = &settings_json {
        let v: serde_json::Value = serde_json::from_str(s).map_err(|e| {
            AppError::new(AppErrorCode::InvalidConfig, "配置不是合法 JSON").with_detail(e.to_string())
        })?;
        // 提前用全量合并校验类型，避免坏配置入库后到处报错
        crate::config::resolve(None, Some(&v), None)?;
    }
    workspace_repo::set_settings(&*conn(&state)?, &workspace_id, settings_json.as_deref())
}

#[tauri::command]
pub async fn delete_workspace(workspace_id: String, state: State<'_, AppState>) -> AppResult<()> {
    workspace_repo::delete(&*conn(&state)?, &workspace_id)
}
