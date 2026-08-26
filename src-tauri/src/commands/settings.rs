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
    /// 构建时 git 短 SHA（build.rs 注入；源码包构建取不到时为 "unknown"）——
    /// 用户报告「结果不对」时能对回确切代码版本，semver 粒度不够。
    pub build_sha: String,
    /// 日志目录绝对路径（前端「打开日志目录」按钮用；路径解析失败为 None）。
    pub log_dir: Option<String>,
    pub max_docs: usize,
    pub min_docs: usize,
    /// 可选语义模型清单（前端选择器据此渲染，不硬编码）。
    pub embedding_models: Vec<EmbedModelInfo>,
    /// 可选 OCR 档位清单。
    pub ocr_models: Vec<OcrModelInfo>,
    pub default_ocr_model: String,
    /// 随包概率校准的只读台账（W6-4，M7）：设置页展示「哪一版校准、拿什么语料测的」。
    /// α/β 与阈值【不开放运行时调整】——改 α 即改承诺语义，须走版本发布（方案 §8 配置项）。
    pub calibration: CalibrationInfo,
}

/// 校准只读台账（设置页展示用）。available=false ⇒ 随包文件缺失/未过审查，本机比对不出三带。
#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationInfo {
    pub available: bool,
    pub version: String,
    /// 校准来源标签：experimental-synthetic = 合成语料拟合（实验性，真实判例回测前不摘标签）。
    pub kind: String,
    /// 校准器类型：platt | isotonic。
    pub calibrator: String,
    /// 分流模式：three-band | review-all。
    pub routing: String,
    pub alpha: f32,
    pub beta: f32,
    pub t_low: f32,
    pub t_high: f32,
    /// 训练语料 hash（前 8 位即可核对语料版本）。
    pub corpus_hash: String,
    /// 分流说明（§1.5-1 文案唯一来源，前端不得自造）。
    pub note: String,
}

impl CalibrationInfo {
    fn current() -> Self {
        match crate::engine::calibrate::active_calibration() {
            Some(m) => CalibrationInfo {
                available: true,
                version: m.version.clone(),
                kind: m.calibration_kind.clone(),
                calibrator: m.calibrator.kind_str().to_string(),
                routing: m.routing.as_str().to_string(),
                alpha: m.alpha,
                beta: m.beta,
                t_low: m.t_low,
                t_high: m.t_high,
                corpus_hash: m.corpus_hash.clone(),
                note: m.routing_note(),
            },
            None => CalibrationInfo {
                note: "随包校准文件不可用：本次安装不产出置信度与复核路由三带，条款按既有风险等级复核。"
                    .to_string(),
                ..Default::default()
            },
        }
    }
}

#[tauri::command]
pub async fn get_app_info(app: tauri::AppHandle) -> AppResult<AppInfo> {
    use tauri::Manager;
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        build_sha: option_env!("BIDGUARD_BUILD_SHA").unwrap_or("unknown").to_string(),
        log_dir: app
            .path()
            .app_log_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
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
        calibration: CalibrationInfo::current(),
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

/// 读取文本文件内容（批量导入选 .txt/.csv/.json/.md 时用）。UTF-8 优先，GB18030 兜底。
/// 纵深防御：限定扩展名与大小上限，收敛「任意路径读原语」——即便 webview 被攻陷，也不能借此
/// 读取 ~/.ssh/id_rsa、浏览器 cookie 等任意文件（与 export_report 的扩展名白名单同一威胁模型）。
#[tauri::command]
pub async fn read_text_file(path: String) -> AppResult<String> {
    const MAX_BYTES: u64 = 64 * 1024 * 1024; // 64MB：文本导入足够，挡住把任意大文件当文本读
    const ALLOWED: [&str; 6] = ["txt", "text", "csv", "json", "md", "markdown"];
    tauri::async_runtime::spawn_blocking(move || -> AppResult<String> {
        let p = std::path::Path::new(&path);
        let ext = p
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !ALLOWED.contains(&ext.as_str()) {
            return Err(AppError::new(
                AppErrorCode::InvalidConfig,
                "仅支持读取 .txt/.csv/.json/.md 文本文件",
            ));
        }
        let meta = std::fs::metadata(p).map_err(|e| {
            AppError::new(AppErrorCode::FileNotFound, "文件不存在或不可读").with_detail(e.to_string())
        })?;
        if meta.len() > MAX_BYTES {
            return Err(AppError::new(AppErrorCode::InvalidConfig, "文件过大（文本导入上限 64MB）"));
        }
        let bytes = std::fs::read(p).map_err(|e| {
            AppError::new(AppErrorCode::FileNotFound, "文件不存在或不可读").with_detail(e.to_string())
        })?;
        Ok(crate::engine::parse::decode_text(&bytes))
    })
    .await
    .map_err(|e| AppError::new(AppErrorCode::Unknown, "读取文件失败").with_detail(e.to_string()))?
}
