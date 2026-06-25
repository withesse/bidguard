// 工具命令：模型状态/下载/缓存、存储信息/清理、环境自检。
// 资源管理动作（与「设置」的偏好分开）：让 OCR/语义模型从「写死摸黑」变「可见可管」。
use super::conn;
use crate::db::repo::embedding_repo;
use crate::engine::{embed, ocr};
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbedModelStatus {
    pub key: String,
    pub label: String,
    pub cached: bool,
    pub size_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub ocr_present: bool,
    pub ocr_location: Option<String>,
    pub embed_cache_dir: Option<String>,
    pub embedding_models: Vec<EmbedModelStatus>,
}

/// OCR 与各语义模型的本地状态（工具屏「模型管理」）。
#[tauri::command]
pub async fn get_model_status() -> AppResult<ModelStatus> {
    Ok(ModelStatus {
        ocr_present: ocr::model_present(),
        ocr_location: ocr::model_location().map(|p| p.to_string_lossy().into_owned()),
        embed_cache_dir: embed::cache_dir_path().map(|p| p.to_string_lossy().into_owned()),
        embedding_models: embed::MODELS
            .iter()
            .map(|m| EmbedModelStatus {
                key: m.key.to_string(),
                label: m.label.to_string(),
                cached: embed::model_cached_for(m),
                size_bytes: embed::model_cache_bytes(m),
            })
            .collect(),
    })
}

/// 预热下载某语义模型（直接阻塞命令，前端转圈等待）。
/// 避免「首次比对时突然卡几分钟下模型」。在工具屏显式发起 → 视为授权联网。
/// fastembed 无细粒度下载进度，故不任务化，完成即返回。
#[tauri::command]
pub async fn download_embedding_model(
    model_key: String,
    state: State<'_, AppState>,
) -> AppResult<()> {
    let embedder = state.embedder();
    tauri::async_runtime::spawn_blocking(move || {
        let spec = embed::resolve(&model_key);
        let mut guard = embedder.lock().unwrap();
        embed::ensure(&mut guard, spec, true).map(|_| ()).ok_or_else(|| {
            AppError::new(AppErrorCode::CompareFailed, "模型下载/加载失败（检查网络或磁盘）")
        })
    })
    .await
    .map_err(|e| AppError::new(AppErrorCode::Unknown, "下载任务失败").with_detail(e.to_string()))?
}

/// 删除某语义模型的本地缓存。返回释放字节数。
#[tauri::command]
pub async fn clear_embedding_model(model_key: String) -> AppResult<u64> {
    Ok(embed::clear_model_cache(embed::resolve(&model_key)))
}

/// 按需下载某 OCR 高精档（medium）。阻塞下载/解压，放 spawn_blocking。返回写入字节数。
#[tauri::command]
pub async fn download_ocr_model(model_key: String) -> AppResult<u64> {
    tauri::async_runtime::spawn_blocking(move || {
        ocr::download_model(ocr::resolve(&model_key)).map_err(|e| {
            AppError::new(AppErrorCode::CompareFailed, "OCR 模型下载失败（检查网络或磁盘）")
                .with_detail(e)
        })
    })
    .await
    .map_err(|e| AppError::new(AppErrorCode::Unknown, "下载任务失败").with_detail(e.to_string()))?
}

/// 删除某已下载的 OCR 高精档。返回释放字节数。
#[tauri::command]
pub async fn clear_ocr_model(model_key: String) -> AppResult<u64> {
    Ok(ocr::clear_model(ocr::resolve(&model_key)))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    pub db_bytes: u64,
    pub embedding_rows: i64,
    pub document_count: i64,
    pub job_count: i64,
}

/// 数据库与缓存占用（工具屏「存储」）。
#[tauri::command]
pub async fn get_storage_info(state: State<'_, AppState>) -> AppResult<StorageInfo> {
    let c = conn(&state)?;
    // page_count × page_size = 数据库逻辑字节数
    let pages: i64 = c.query_row("PRAGMA page_count", [], |r| r.get(0))?;
    let page_size: i64 = c.query_row("PRAGMA page_size", [], |r| r.get(0))?;
    Ok(StorageInfo {
        db_bytes: (pages * page_size).max(0) as u64,
        embedding_rows: embedding_repo::count(&c)?,
        document_count: c.query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))?,
        job_count: c.query_row("SELECT COUNT(*) FROM jobs", [], |r| r.get(0))?,
    })
}

/// 清空语义向量缓存（DB 表），返回删除条数。下次比对按需重算。
#[tauri::command]
pub async fn clear_embedding_cache(state: State<'_, AppState>) -> AppResult<usize> {
    embedding_repo::clear(&*conn(&state)?)
}

/// 压缩数据库（VACUUM），回收删除任务/文档后的空洞空间。
#[tauri::command]
pub async fn vacuum_db(state: State<'_, AppState>) -> AppResult<()> {
    conn(&state)?.execute_batch("VACUUM")?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticItem {
    pub key: String,
    pub label: String,
    pub ok: bool,
    pub detail: String,
}

/// 环境自检（工具屏）：pdfium / OCR 模型 / 语义模型缓存 / 数据库 的可用性。
/// 用户自助排障「为什么扫描件没识别 / 语义查重不生效」。
#[tauri::command]
pub async fn run_diagnostics(state: State<'_, AppState>) -> AppResult<Vec<DiagnosticItem>> {
    let mut items = Vec::new();

    let pdfium = crate::engine::parse::pdfium_available();
    items.push(DiagnosticItem {
        key: "pdfium".into(),
        label: "PDF 引擎（pdfium）".into(),
        ok: pdfium,
        detail: if pdfium { "已就位，可解析/渲染 PDF".into() } else { "缺失，PDF 将回退或无法处理".into() },
    });

    let ocr = ocr::model_present();
    items.push(DiagnosticItem {
        key: "ocr".into(),
        label: "扫描件 OCR 模型".into(),
        ok: ocr,
        detail: if ocr { "三件套就位，扫描件可识别".into() } else { "缺失，扫描件/图片文字无法识别".into() },
    });

    let embed_cached = embed::model_cached();
    items.push(DiagnosticItem {
        key: "embedding".into(),
        label: "语义模型".into(),
        ok: embed_cached,
        detail: if embed_cached {
            "至少一个模型已缓存，语义查重可用".into()
        } else {
            "未缓存，启用语义查重时将首次下载".into()
        },
    });

    let db_ok = conn(&state)?.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
        .map(|s| s == "ok")
        .unwrap_or(false);
    items.push(DiagnosticItem {
        key: "db".into(),
        label: "数据库完整性".into(),
        ok: db_ok,
        detail: if db_ok { "integrity_check 通过".into() } else { "完整性检查未通过，建议备份".into() },
    });

    Ok(items)
}
