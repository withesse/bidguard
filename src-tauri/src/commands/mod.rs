// 新通路 Command 层：薄壳——参数校验 + 取连接 + 调服务/仓储，业务不在这里写。
pub mod annotation;
pub mod compare;
pub mod document;
pub mod export;
pub mod job;
pub mod settings;
pub mod workspace;

use crate::db::repo::{settings_repo, workspace_repo};
use crate::db::DbConn;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub(crate) fn conn(state: &AppState) -> AppResult<DbConn> {
    state.db.get().map_err(AppError::from)
}

/// 解析某工作区生效的配置（内置 < 用户全局 < 工作区）。
pub(crate) fn effective_config(
    state: &AppState,
    workspace_id: &str,
) -> AppResult<crate::config::AppConfig> {
    let c = conn(state)?;
    let user = settings_repo::get(&c, "config")?;
    let ws = workspace_repo::get(&c, workspace_id)?;
    let ws_patch = ws
        .settings_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());
    crate::config::resolve(user.as_ref(), ws_patch.as_ref(), None)
}
