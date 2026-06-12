// 导出 command：从 DB 装配报告写盘（不再由前端回传整份 Report）。
// 本地导出在亚秒到数秒量级，spawn_blocking 同步完成即可，无需任务化。
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::export::FORMATS;
use crate::services::export_service;
use crate::state::AppState;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResultDto {
    pub path: String,
    pub format: String,
}

#[tauri::command]
pub async fn export_report(
    job_id: String,
    format: String,
    path: String,
    state: State<'_, AppState>,
) -> AppResult<ExportResultDto> {
    if !FORMATS.contains(&format.as_str()) {
        return Err(AppError::new(
            AppErrorCode::InvalidConfig,
            format!("不支持的导出格式：{format}"),
        ));
    }
    let db = state.db.clone();
    let jieba = state.jieba();
    let (job_id2, format2, path2) = (job_id, format.clone(), path.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db.get().map_err(AppError::from)?;
        export_service::export_to(&conn, &jieba, &job_id2, &format2, &path2)
    })
    .await
    .map_err(|e| AppError::new(AppErrorCode::ExportFailed, "导出任务失败").with_detail(e.to_string()))??;
    Ok(ExportResultDto { path, format })
}
