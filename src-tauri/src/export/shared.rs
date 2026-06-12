// 各导出格式共用的小工具：天干标签、转义、判定文案、docx 段落构造。
pub const LABELS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];

pub fn label(i: usize) -> &'static str {
    LABELS.get(i).copied().unwrap_or("?")
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub fn level_cn(l: &str) -> &'static str {
    match l {
        "high" => "围标嫌疑（高）",
        "medium" => "重点复核（中）",
        "low" => "轻度雷同（低）",
        _ => "未见明显围标",
    }
}

pub fn section_cn(s: &str) -> &'static str {
    match s {
        "tech" => "技术标",
        "business" => "商务标",
        _ => "其他",
    }
}

pub fn type_cn(t: &str) -> &'static str {
    match t {
        "same" => "相同",
        "minor_change" => "轻微修改",
        "rewrite" => "改写",
        "changed" => "修改",
        "added" => "基准缺失",
        "deleted" => "基准独有",
        "conflict" => "事实冲突",
        _ => "待复核",
    }
}

pub fn severity_cn(s: &str) -> &'static str {
    match s {
        "high" => "高",
        "medium" => "中",
        "low" => "低",
        "review" => "需人工",
        _ => "—",
    }
}

pub fn review_cn(s: &str) -> &'static str {
    match s {
        "confirmed" => "已确认",
        "ignored" => "已忽略",
        _ => "待确认",
    }
}

pub fn field_cn(f: &str) -> &'static str {
    match f {
        "amount" => "金额",
        "duration" => "工期",
        "date" => "日期",
        "percentage" => "比例",
        "subject" => "责任主体",
        _ => "其他",
    }
}

/// 极简 OOXML 段落（docx 写器共用）。
pub fn docx_p(out: &mut String, text: &str, bold: bool, size: u32) {
    let mut rpr = String::new();
    if bold || size > 0 {
        rpr.push_str("<w:rPr>");
        if bold {
            rpr.push_str("<w:b/>");
        }
        if size > 0 {
            rpr.push_str(&format!("<w:sz w:val=\"{size}\"/>"));
        }
        rpr.push_str("</w:rPr>");
    }
    out.push_str(&format!(
        "<w:p><w:r>{rpr}<w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
        xml_escape(text)
    ));
}

pub const DOCX_CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>";
pub const DOCX_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/></Relationships>";

/// 把 document.xml 打成最小合法 docx 包。
pub fn write_docx_package(path: &str, document_xml: &str) -> Result<(), String> {
    use std::io::Write;
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zipw = zip::ZipWriter::new(file);
    let opt = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, content) in [
        ("[Content_Types].xml", DOCX_CONTENT_TYPES),
        ("_rels/.rels", DOCX_RELS),
        ("word/document.xml", document_xml),
    ] {
        zipw.start_file(name, opt).map_err(|e| e.to_string())?;
        zipw.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    }
    zipw.finish().map_err(|e| e.to_string())?;
    Ok(())
}
