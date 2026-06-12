// JSON 报告：ExportData 全量序列化（系统集成 / 二次处理用，§14.3 超集）。
use super::data::ExportData;

pub fn write(data: &ExportData, path: &str) -> Result<(), String> {
    let s = serde_json::to_string_pretty(data).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}
