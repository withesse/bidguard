// CSV 报告：条款级平铺（每个成员一行），UTF-8 BOM 让 Excel 直接识别中文。
use super::data::ExportData;
use super::shared::{review_cn, section_cn, severity_cn, type_cn};

fn esc(s: &str) -> String {
    // CWE-1236 CSV 公式注入防护：标书正文来自投标人（对抗方），若单元格以 = + - @ 或
    // TAB/CR 开头，Excel/WPS 打开时会当公式执行（可外带同表数据或诱导命令）。前置单引号中和，
    // 再做引号转义。必须先中和后转义，保证外层引号包裹逻辑不变。
    let neutralized = match s.chars().next() {
        Some('=') | Some('+') | Some('-') | Some('@') | Some('\t') | Some('\r') => {
            format!("'{s}")
        }
        _ => s.to_string(),
    };
    format!("\"{}\"", neutralized.replace('"', "\"\""))
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

#[cfg(test)]
mod tests {
    use super::esc;

    #[test]
    fn esc_neutralizes_formula_injection() {
        for payload in ["=1+1", "+cmd", "-2", "@SUM(A1)", "\tx", "\rx"] {
            let out = esc(payload);
            assert!(out.starts_with("\"'"), "危险前导字符应被前置单引号中和：{payload:?} → {out}");
        }
        // 典型攻击载荷：不以裸 = 开头
        assert!(!esc("=HYPERLINK(\"http://x\",\"y\")").starts_with("\"="));
    }

    #[test]
    fn esc_preserves_normal_text_and_quote_escaping() {
        assert_eq!(esc("甲方应在十日内支付"), "\"甲方应在十日内支付\"");
        // 正常正文不加前缀；内部双引号仍转义
        assert_eq!(esc("a\"b"), "\"a\"\"b\"");
        // 中间出现的 = 不受影响（只看首字符）
        assert_eq!(esc("x=y"), "\"x=y\"");
    }
}
