// 设置与模板 commands：用户全局配置（app_settings 表 "config" 键）、查重源模板、应用信息。
use super::conn;
use crate::db::repo::template_repo::{BatchResult, NewTemplate, TemplateRow};
use crate::db::repo::{settings_repo, template_repo};
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::state::AppState;
use serde::{Deserialize, Serialize};
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
pub struct EmbedModelInfo {
    pub key: String,
    pub label: String,
}

/// OCR 档位（PP-OCRv6 tiny/small/medium）；前端选择器与档位管理据此渲染。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OcrModelInfo {
    pub key: String,
    pub label: String,
    pub size_label: String,
    /// 是否随应用打包（false = 需下载）。
    pub bundled: bool,
    /// 当前是否就位（打包或已下载）。
    pub present: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub max_docs: usize,
    pub min_docs: usize,
    /// 可选语义模型清单（前端选择器据此渲染，不硬编码）。
    pub embedding_models: Vec<EmbedModelInfo>,
    /// 可选 OCR 档位清单。
    pub ocr_models: Vec<OcrModelInfo>,
    pub default_ocr_model: String,
}

#[tauri::command]
pub async fn get_app_info() -> AppResult<AppInfo> {
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        max_docs: crate::config::MAX_DOCS,
        min_docs: crate::config::MIN_DOCS,
        embedding_models: crate::engine::embed::MODELS
            .iter()
            .map(|m| EmbedModelInfo { key: m.key.to_string(), label: m.label.to_string() })
            .collect(),
        ocr_models: crate::engine::ocr::OCR_MODELS
            .iter()
            .map(|m| OcrModelInfo {
                key: m.key.to_string(),
                label: m.label.to_string(),
                size_label: m.size_label.to_string(),
                bundled: m.bundled,
                present: crate::engine::ocr::model_present_for(m.key),
            })
            .collect(),
        default_ocr_model: crate::engine::ocr::DEFAULT_OCR_MODEL.to_string(),
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
    category: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<TemplateRow> {
    let name = name.trim();
    let text = text.trim();
    if name.is_empty() || text.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "模板名称与内容不能为空"));
    }
    template_repo::save(&*conn(&state)?, id.as_deref(), name, text, category.as_deref())
}

#[tauri::command]
pub async fn set_source_template_enabled(
    id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    template_repo::set_enabled(&*conn(&state)?, &id, enabled)
}

/// 批量导入的单条 DTO（前端解析后传入）。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewTemplateDto {
    #[serde(default)]
    pub category: Option<String>,
    pub name: String,
    pub text: String,
}

#[tauri::command]
pub async fn batch_save_source_templates(
    items: Vec<NewTemplateDto>,
    state: State<'_, AppState>,
) -> AppResult<BatchResult> {
    let rows: Vec<NewTemplate> = items
        .into_iter()
        .map(|d| NewTemplate { category: d.category, name: d.name, text: d.text })
        .collect();
    let mut c = conn(&state)?;
    template_repo::batch_save(&mut c, &rows)
}

#[tauri::command]
pub async fn delete_source_template(id: String, state: State<'_, AppState>) -> AppResult<()> {
    template_repo::delete(&*conn(&state)?, &id)
}

/// 读取文本文件内容（批量导入选 .txt/.csv/.json 时用）。UTF-8 优先，GB18030 兜底。
/// 与放开 fs 全盘 scope 相比更收敛：仅此一处按用户经对话框选定的路径读取。
#[tauri::command]
pub async fn read_text_file(path: String) -> AppResult<String> {
    tauri::async_runtime::spawn_blocking(move || std::fs::read(&path))
        .await
        .map_err(|e| AppError::new(AppErrorCode::Unknown, "读取文件失败").with_detail(e.to_string()))?
        .map(|bytes| crate::engine::parse::decode_text(&bytes))
        .map_err(|e| {
            AppError::new(AppErrorCode::FileNotFound, "文件不存在或不可读").with_detail(e.to_string())
        })
}
