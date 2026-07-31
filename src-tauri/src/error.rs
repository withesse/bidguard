// 结构化错误：错误码 + 用户可读信息 + 可展开详情（前端按 code 分类处理，不暴露内部细节）。
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AppErrorCode {
    FileNotFound,
    FilePermissionDenied,
    UnsupportedFileType,
    ParseFailed,
    DatabaseError,
    InvalidConfig,
    JobCancelled,
    JobConflict,
    CompareFailed,
    ExportFailed,
    NotFound,
    // 授权 / 激活（license 模块，全部在 Rust 层强制，前端仅做 UX）
    LicenseRequired,       // 未激活 / 试用未开始或已结束
    LicenseExpired,        // 授权到期（超宽限）
    LicenseExhausted,      // 使用次数用尽
    LicenseMachineMismatch,// 许可未绑定到本机
    LicenseInvalid,        // 验签失败 / 许可格式错误
    LicenseClockTamper,    // 检测到系统时钟回拨
    Unknown,
}

/// 应用统一错误。message 面向用户，detail 仅供「展开详情 / 复制反馈」。
#[derive(Debug, Clone, Serialize, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[error("{message}")]
pub struct AppError {
    pub code: AppErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(code: AppErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn not_found(what: &str) -> Self {
        Self::new(AppErrorCode::NotFound, format!("{what}不存在"))
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        AppError::new(AppErrorCode::DatabaseError, "数据库操作失败").with_detail(e.to_string())
    }
}

impl From<r2d2::Error> for AppError {
    fn from(e: r2d2::Error) -> Self {
        AppError::new(AppErrorCode::DatabaseError, "数据库连接失败").with_detail(e.to_string())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => AppErrorCode::FileNotFound,
            std::io::ErrorKind::PermissionDenied => AppErrorCode::FilePermissionDenied,
            _ => AppErrorCode::Unknown,
        };
        AppError::new(code, "文件操作失败").with_detail(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_camel_case_and_skips_empty_detail() {
        let e = AppError::new(AppErrorCode::FileNotFound, "文件不存在");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "fileNotFound");
        assert_eq!(v["message"], "文件不存在");
        assert!(v.get("detail").is_none());

        let e = e.with_detail("/tmp/x.docx");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["detail"], "/tmp/x.docx");
    }

    #[test]
    fn maps_io_error_kind() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        assert_eq!(AppError::from(io).code, AppErrorCode::FileNotFound);
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "no");
        assert_eq!(AppError::from(io).code, AppErrorCode::FilePermissionDenied);
    }
}
