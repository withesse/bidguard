// 设置与模板 commands：用户全局配置（app_settings 表 "config" 键）、查重源模板、应用信息。
use super::conn;
use crate::db::repo::{settings_repo, template_repo};
use crate::db::repo::template_repo::TemplateRow;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

/// 用户全局配置 patch（覆盖内置默认；工作区/任务层再往上叠）。
#[tauri::command]
pub async fn get_app_settings(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    Ok(settings_repo::get(&*conn(&state)?, "config")?.unwrap_or(serde_json::Value::Null))
}

#[tauri::command]
pub async fn set_app_settings(
    settings: serde_json::Value,
    state: State<'_, AppState>,
) -> AppResult<()> {
    // 入库前先用全量合并校验类型，避免坏配置入库后到处报错
    crate::config::resolve(Some(&settings), None, None)?;
    settings_repo::set(&*conn(&state)?, "config", &settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub max_docs: usize,
    pub min_docs: usize,
}

#[tauri::command]
pub async fn get_app_info() -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        max_docs: crate::config::MAX_DOCS,
        min_docs: crate::config::MIN_DOCS,
    })
}

// —— 查重源模板 ——

#[tauri::command]
pub async fn list_source_templates(state: State<'_, AppState>) -> AppResult<Vec<TemplateRow>> {
    template_repo::list(&*conn(&state)?)
}

#[tauri::command]
pub async fn save_source_template(
    id: Option<String>,
    name: String,
    text: String,
    state: State<'_, AppState>,
) -> AppResult<TemplateRow> {
    let name = name.trim();
    let text = text.trim();
    if name.is_empty() || text.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "模板名称与内容不能为空"));
    }
    template_repo::save(&*conn(&state)?, id.as_deref(), name, text)
}

#[tauri::command]
pub async fn delete_source_template(id: String, state: State<'_, AppState>) -> AppResult<()> {
    template_repo::delete(&*conn(&state)?, &id)
}
