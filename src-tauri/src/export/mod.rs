// 报告导出：六种格式（xlsx/docx/html/json/markdown/csv）。
// write() 吃 ExportData（由 export_service 从 DB 装配，
// 含八类统计/事实冲突/配置快照/版本附录）。
mod csv;
pub mod data;
mod docx;
mod html;
mod json;
mod markdown;
mod shared;
mod xlsx;

/// 文档位次 → 天干标签（装配层用 String 形态）。
pub fn data_tag(i: usize) -> String {
    shared::label(i).to_string()
}

use crate::error::{AppError, AppErrorCode, AppResult};
use data::ExportData;

pub const FORMATS: &[&str] = &["xlsx", "docx", "html", "json", "markdown", "csv"];

pub fn write(data: &ExportData, format: &str, path: &str) -> AppResult<()> {
    let r = match format {
        "xlsx" => xlsx::write(data, path),
        "docx" => docx::write(data, path),
        "html" => html::write(data, path),
        "json" => json::write(data, path),
        "markdown" => markdown::write(data, path),
        "csv" => csv::write(data, path),
        other => {
            return Err(AppError::new(
                AppErrorCode::InvalidConfig,
                format!("不支持的导出格式：{other}"),
            ))
        }
    };
    r.map_err(|e| AppError::new(AppErrorCode::ExportFailed, "报告写入失败").with_detail(e))
}
