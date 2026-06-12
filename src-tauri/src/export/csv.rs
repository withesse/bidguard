// CSV 报告：条款级平铺（每个成员一行），UTF-8 BOM 让 Excel 直接识别中文。
use super::data::ExportData;
use super::shared::{review_cn, section_cn, severity_cn, type_cn};

fn esc(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub fn write(data: &ExportData, path: &str) -> Result<(), String> {
    let mut out = String::from("\u{feff}");
    out.push_str("组号,类型,风险,确认状态,标段,组内相似,主题,涉及文档,文档,角色,页码,章节路径,文本\n");
    for c in &data.clusters {
        let docs: Vec<&str> = {
            let mut seen: Vec<&str> = Vec::new();
            for m in &c.members {
                if !seen.contains(&m.tag.as_str()) {
                    seen.push(&m.tag);
                }
            }
            seen
        };
        for m in &c.members {
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                c.index,
                type_cn(&c.cluster_type),
                severity_cn(c.severity.as_deref().unwrap_or("none")),
                review_cn(&c.review_status),
                section_cn(c.section_kind.as_deref().unwrap_or("other")),
                c.score.map(|s| format!("{:.0}%", s * 100.0)).unwrap_or_default(),
                esc(c.topic.as_deref().unwrap_or("")),
                docs.join("·"),
                m.tag,
                if m.role == "primary" { "主" } else { "重复" },
                m.page.map(|p| p.to_string()).unwrap_or_default(),
                esc(&m.section_path.join(" › ")),
                esc(&m.text),
            ));
        }
    }
    std::fs::write(path, out).map_err(|e| e.to_string())
}
