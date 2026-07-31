// 授权/激活 commands（薄壳）：状态查询、机器码、许可导入。
// 授权判定全部下沉到 license::LicenseManager（Rust 层强制），前端仅据 status 做 UX。
use crate::error::AppResult;
use crate::license::LicenseStatus;
use crate::state::AppState;
use tauri::State;

/// 当前授权状态（首启会自动开启本地试用）。
#[tauri::command]
pub async fn get_license_status(state: State<'_, AppState>) -> AppResult<LicenseStatus> {
    Ok(state.license.status(&state.db))
}

/// 本机机器码（形态 A：复制/拍照发给运营，据此签发绑定本机的 .lic）。
#[tauri::command]
pub async fn get_machine_code(state: State<'_, AppState>) -> AppResult<String> {
    Ok(state.license.machine_code())
}

/// 导入许可：input 为 armored 许可文本（粘贴）或本机 .lic 文件路径。
#[tauri::command]
pub async fn import_license(input: String, state: State<'_, AppState>) -> AppResult<LicenseStatus> {
    state.license.import_license(&state.db, &input)
}
