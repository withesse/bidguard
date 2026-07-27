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

/// 围标结论的【证据强度口头等级】（ENFSI 式，§1.5-2）：M7 起 score 是校准后的证据强度，
/// 数值一律不作「串通概率 X%」呈现——报告只给口头等级，数值留在 JSON 技术字段供二次处理。
pub fn strength_phrase(level: &str) -> &'static str {
    match level {
        "high" => "强支持「同源编制」假设",
        "medium" => "中等支持「同源编制」假设",
        "low" => "弱支持「同源编制」假设",
        _ => "未见支持「同源编制」假设的证据",
    }
}

/// 单条信号的贡献标签：M7 起 weight 是该信号的对数似然比（log-odds）贡献，不是 0–1 权重占比。
pub fn contrib_label(weight: f32) -> String {
    format!("对数似然比贡献 +{weight:.2}")
}

/// 融合口径脚注（§1.5-5：分级语义变更需可解释 + §1.5-6 实验性标签）。随每份报告的综合判定段落输出。
pub fn calibration_note(kind: &str, version: &str, app_version: &str) -> String {
    let src = match kind {
        "experimental-synthetic" => "实验性校准（合成语料拟合的融合权重）",
        "empirical-fallback" => "经验权重回退档（本次未启用语料校准）",
        _ => "未标注校准来源（旧任务，按当时口径生成）",
    };
    let ver = if version.is_empty() { "—" } else { version };
    format!(
        "融合口径：{src}；权重版本 {ver}；引擎 {app_version}。证据强度由各信号的对数似然比贡献融合而来，         为在合成校准语料上测得的强度等级、不是串通概率；未命中不构成清白证明，是否构成串通投标须由评标委员会依法认定。"
    )
}

/// 一条条款的三带名（W6-4）。band=None（旧任务/未校准）→「未校准」，不留空白
/// ——空白会被读成「没问题」，而未校准只是没测过（§1.5-1 如实展示）。
pub fn band_cn_of(band: Option<&str>) -> &'static str {
    crate::engine::calibrate::band_cn(band.unwrap_or(""))
}

/// 三带章节的人读正文（六格式共用，文案只此一份，避免各写器各写一版而漂移）。
pub fn calibration_lines(c: &crate::export::data::CalibrationSection) -> Vec<String> {
    let mut out = vec![format!(
        "复核路由：{} {} 条 · {} {} 条 · {} {} 条 · 未校准 {} 条",
        c.flag_label, c.flag_count, c.review_label, c.review_count, c.pass_label, c.pass_count,
        c.uncalibrated_count
    )];
    if !c.version.is_empty() {
        let src = match c.calibration_kind.as_str() {
            "experimental-synthetic" => "实验性校准（合成语料）",
            "empirical-fallback" => "经验回退档",
            "" => "未标注",
            other => other,
        };
        out.push(format!(
            "校准来源：{src} · 版本 {}{}",
            c.version,
            if c.corpus_hash.is_empty() {
                String::new()
            } else {
                format!(" · 语料 {}", &c.corpus_hash[..c.corpus_hash.len().min(8)])
            }
        ));
    }
    out.extend(c.notes.iter().cloned());
    out
}

pub fn section_cn(s: &str) -> &'static str {
    match s {
        "tech" => "技术标",
        "business" => "商务标",
        "legal" => "法定格式",
        "price" => "报价清单",
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
