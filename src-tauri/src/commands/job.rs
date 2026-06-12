// 任务 commands：查询与取消。
use super::conn;
use crate::db::repo::job_repo::{self, JobRow};
use crate::error::AppResult;
use crate::state::AppState;
use tauri::State;

#[tauri::command]
pub async fn get_job(job_id: String, state: State<'_, AppState>) -> AppResult<JobRow> {
    job_repo::get(&*conn(&state)?, &job_id)
}

#[tauri::command]
pub async fn list_jobs(
    workspace_id: Option<String>,
    state: State<'_, AppState>,
) -> AppResult<Vec<JobRow>> {
    job_repo::list(&*conn(&state)?, workspace_id.as_deref())
}

/// 请求取消：协作式——任务体在下一个检查点收尾，状态先进入 cancelling。
#[tauri::command]
pub async fn cancel_job(job_id: String, state: State<'_, AppState>) -> AppResult<()> {
    state.jobs.cancel(&state.db, &job_id)
}

#[tauri::command]
pub async fn set_job_starred(
    job_id: String,
    starred: bool,
    state: State<'_, AppState>,
) -> AppResult<()> {
    job_repo::set_starred(&*conn(&state)?, &job_id, starred)
}

#[tauri::command]
pub async fn delete_job(job_id: String, state: State<'_, AppState>) -> AppResult<()> {
    job_repo::delete(&*conn(&state)?, &job_id)
}

/// 清理 N 天前已完结且未收藏的任务（设置页「自动清理」启动时调用）。返回清理数。
#[tauri::command]
pub async fn cleanup_old_jobs(days: u32, state: State<'_, AppState>) -> AppResult<usize> {
    let days = days.clamp(1, 3650);
    let n = job_repo::delete_finished_older_than(&*conn(&state)?, days)?;
    if n > 0 {
        log::info!("自动清理 {days} 天前任务：{n} 个");
    }
    Ok(n)
}
