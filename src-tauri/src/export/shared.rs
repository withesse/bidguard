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

// ── 证据章节共用文案与标签 ──
// §1.5 铁律：屏幕可见的证据类型必须在正式报告格式中可引用，且六格式措辞同源——
// 导语/免责语只此一份，写器不得自造弱化表述。

/// 取证证据章导语。
pub const FORENSIC_NOTE: &str = "取证信号为线索级同源证据（rsid / PDF 血缘 / 图片同源 / 共同错误）。请核对是否源自招标方统一模板或同一代理机构；未命中不构成清白证明（另存为 / 元数据清洗可消除痕迹）。";

/// 规避特征复核章导语。
pub const EVASION_NOTE: &str =
    "检测到疑似规避特征，请人工复核；本工具不作「规避 / 串通」定性结论。未命中不构成清白证明。";

/// 对齐区段与逐字证据章导语（纯文本格式用；HTML 另有带三级色标的同义版本）。
pub const SEGMENTS_NOTE: &str = "对齐区段为与聚类并存的独立证据层（按 chunk 去重的真实覆盖）。深红＝逐字铁证、橙＝锚点雷同、黄＝gap 细化差异；标注「引用招标文件」者落在招标豁免块，系对同一招标条款的合法应答，非串通证据。未命中不构成清白证明。";

/// 共享算术错误清单提示（§1.5：必须随清单出现）。
pub const ARITH_ERROR_NOTE: &str = "同一清单项在两份文件中工程量、综合单价与（算错的）合价三者到分全等。检测已排除可由常见舍入规则解释的差值；请核对是否源自同一计价软件舍入惯例或招标文件，单条命中不构成串通投标认定。";

/// 逐文档单价尾数分布章脚注。
pub const DIGIT_TAIL_NOTE: &str = "尾数聚集反映报价的取整习惯（如统一取整到角/元），单独不构成串通认定，需结合取证类证据；本工具未做 Benford 首位检验（单价通常只跨 2–3 个数量级，前提不成立）。";

/// 商务标数值证据章导语（条目数与告警线取本次配置快照，报告可复现）。
pub fn numeric_intro(nm: &crate::export::data::NumericSection) -> String {
    format!(
        "报价清单逐项比对：共识别清单条目 {} 条、跨文档对齐 {} 条；雷同率告警线 {:.0}%，可比条目不足 {} 项的文档对不出结论。",
        nm.item_count,
        nm.aligned_item_count,
        nm.identical_rate_alarm * 100.0,
        nm.min_comparable
    )
}

/// 规律性差异形态的中文标签（numeric.pairs[].pattern.kind）。
pub fn numeric_pattern_cn(kind: &str) -> &str {
    match kind {
        "arith_seq" => "等差（各项差额恒定）",
        "geo_discount" => "等比 / 恒定折扣（各项系数恒定）",
        "affine" => "仿射（系数与差额均非平凡）",
        other => other,
    }
}

/// 雷同率缺席原因的中文标签（numeric.pairs[].reason）。
pub fn numeric_reason_cn(reason: Option<&str>) -> &str {
    match reason {
        Some("insufficient") => "可比条目不足，不出结论",
        Some(other) => other,
        None => "无可比条目",
    }
}

/// 逐项雷同率单元格：出不了结论时写明原因，绝不留空（留空会被读成「无异常」）。
pub fn numeric_rate_cell(p: &crate::export::data::NumericPairEntry) -> String {
    match p.identical_rate {
        Some(r) => format!("{:.1}%", r * 100.0),
        None => format!("—（{}）", numeric_reason_cn(p.reason.as_deref())),
    }
}

// ── 基准价敏感性（W5-5 机制感知筛查）小节：措辞与单元格文案的唯一来源 ──
// §2 后置池裁决：本小节【只作描述性解释】，六格式与 UI 的措辞必须同源、不得弱化。

/// 小节标题（六格式共用）。
pub const MECHANISM_TITLE: &str = "基准价敏感性（反事实解释性分析）";
/// 小节表格标题。
pub const MECHANISM_PRICE_TITLE: &str = "投标总价与来源";
pub const MECHANISM_GROUP_TITLE: &str = "候选组反事实结果";
pub const MECHANISM_SUPPORT_TITLE: &str = "断崖式报价（support-bid 形态）";
/// 表头（六格式共用，列序一致）。
pub const MECHANISM_PRICE_HEADER: [&str; 3] = ["编号", "投标总价（元）", "来源"];
pub const MECHANISM_GROUP_HEADER: [&str; 6] =
    ["候选组", "构造依据", "中标人翻转比例", "基准价偏移", "同规模子集分位", "中标人（全量→剔除后）"];
pub const MECHANISM_SUPPORT_HEADER: [&str; 5] =
    ["编号", "投标总价（元）", "位置", "与次邻间距（元）", "偏离中位数"];

/// 小节导语（性质声明 + 公式全文 + 基准价锚点 / 不适用原因）。写器按顺序原样输出。
pub fn mechanism_lines(m: &crate::export::data::NumericMechanism) -> Vec<String> {
    let mut out = vec![format!("评标办法（人工录入）：{}", m.formula)];
    match (&m.not_applicable_reason, &m.benchmark, &m.lowest) {
        (Some(reason), _, _) => out.push(format!("不适用：{reason}")),
        (None, Some(b), _) => out.push(format!(
            "基准价（系数 {:.4}，取自区间 [{:.4}, {:.4}] 的中点）：{:.2} 元；去 {} 个最高、{} 个最低；系数区间取 {} 个均匀格点逐点重算；该系数下中标人为 {}。",
            b.coeff_mid,
            b.coeff_min,
            b.coeff_max,
            b.benchmark_mid,
            b.trim_highest,
            b.trim_lowest,
            b.grid_points,
            b.winner_mid
        )),
        (None, None, Some(l)) => out.push(mechanism_lowest_line(l)),
        (None, None, None) => {}
    }
    out.extend(m.notes.iter().cloned());
    out
}

/// 最低评标价法的孤立度描述（禁用均值类统计，只描述最低与次低的间距）。
pub fn mechanism_lowest_line(l: &crate::export::data::MechanismLowest) -> String {
    format!(
        "最低投标总价为 {}（{:.2} 元），次低 {:.2} 元，间距 {:.2} 元（相邻报价中位间距 {:.2} 元）：{}。",
        l.winner,
        l.lowest,
        l.second_lowest,
        l.gap,
        l.median_gap,
        if l.isolated { "最低价与其余报价断崖，建议核对成本构成" } else { "未见断崖式孤立" }
    )
}

/// 投标总价行的单元格（编号 / 金额 / 来源标签）。
pub fn mechanism_price_cells(p: &crate::export::data::MechanismPrice) -> [String; 3] {
    [p.tag.clone(), format!("{:.2}", p.total), p.source_label.clone()]
}

/// 候选组行的单元格。翻转比例是【反事实占比】，不是概率——措辞见 mechanism notes。
pub fn mechanism_group_cells(g: &crate::export::data::MechanismGroup) -> [String; 6] {
    [
        g.docs.join(" × "),
        if g.basis.is_empty() { "—".to_string() } else { g.basis.join("；") },
        format!("{:.1}%", g.flip_prob * 100.0),
        format!("{:+.2}%", g.benchmark_shift_pct),
        format!("{:.0}%（同规模子集 {} 个）", g.shift_percentile * 100.0, g.subsets_compared),
        format!("{} → {}", g.winner_full, g.winner_excluded),
    ]
}

/// 断崖式报价行的单元格。
pub fn mechanism_support_cells(s: &crate::export::data::MechanismSupportBid) -> [String; 5] {
    [
        s.tag.clone(),
        format!("{:.2}", s.total),
        mechanism_position_cn(&s.position).to_string(),
        format!("{:.2}（中位间距 {:.2}）", s.gap, s.median_gap),
        format!("{:+.1}%", s.deviation_pct),
    ]
}

/// 报价分布端点的中文标签。
pub fn mechanism_position_cn(position: &str) -> &str {
    match position {
        "lowest" => "报价最低端",
        "highest" => "报价最高端",
        other => other,
    }
}

/// 清单项可读标签：优先项目名称，回落对齐键（编码/名称+单位）。
pub fn numeric_item_label(name: Option<&str>, align_key: &str) -> String {
    match name.map(str::trim).filter(|s| !s.is_empty()) {
        Some(n) => format!("{n}（{align_key}）"),
        None => align_key.to_string(),
    }
}

/// 取证命中类型中文标签（forensic.hits[].kind）。
pub fn forensic_kind_cn(kind: &str) -> &str {
    match kind {
        "rsid" => "docx 修订标识（rsid）",
        "pdfLineage" => "PDF 血缘",
        "imageReuse" => "内嵌图片同源",
        "sharedErrors" => "共同错误指纹",
        other => other,
    }
}

/// 取证命中强度中文标签（forensic.hits[].level）。
pub fn forensic_level_cn(level: &str) -> &str {
    match level {
        "hard" => "硬命中",
        "mid" => "中命中",
        "weak" => "弱命中",
        other => other,
    }
}

/// 取证命中的文档对标签（逐对结构未落库时回落「见说明」，不留空）。
pub fn forensic_pair_label(doc_a: &str, doc_b: &str) -> String {
    if doc_a.is_empty() && doc_b.is_empty() {
        "见说明".to_string()
    } else {
        format!("{doc_a} ↔ {doc_b}")
    }
}

/// 规避判级中文标签（evasion.perDocument[].verdict）——§1.5：措辞不下定性结论。
pub fn evasion_verdict_cn(verdict: &str) -> &str {
    match verdict {
        "confirmed" => "需人工复核",
        "suspect" => "疑似（弱信号）",
        other => other,
    }
}

/// 逐字区间一侧定位串（页码 + 章节路径 → 单行；调用方按格式自行转义）。
pub fn verbatim_locator(page: Option<i64>, section: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(pg) = page {
        parts.push(format!("第{pg}页"));
    }
    if let Some(s) = section.map(str::trim).filter(|s| !s.is_empty()) {
        parts.push(s.to_string());
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" · ")
    }
}

/// 逐文档血缘键摘要（forensic.perDocument[].lineage → 单行可读串）。
pub fn lineage_summary(lineage: &serde_json::Value) -> String {
    let get = |k: &str| lineage.get(k).and_then(serde_json::Value::as_str).filter(|s| !s.is_empty());
    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = get("documentId") {
        parts.push(format!("GUID {v}"));
    }
    if let Some(v) = get("idFirst") {
        parts.push(format!("trailer ID {v}"));
    }
    if let Some(v) = get("derivedFrom") {
        parts.push(format!("派生自 {v}"));
    }
    let tags = lineage
        .get("fontSubsetTags")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.len())
        .unwrap_or(0);
    if tags > 0 {
        parts.push(format!("字体子集 {tags} 个"));
    }
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join("；")
    }
}

/// 尾数分布一行的可读数值（digitStats 原样透传的 JSON → 取数）。
pub fn digit_stat(stats: &serde_json::Value, key: &str) -> f64 {
    stats.get(key).and_then(serde_json::Value::as_f64).unwrap_or(0.0)
}

/// 尾数聚集结论标签（clustered 布尔 → 中文）。
pub fn digit_clustered_cn(stats: &serde_json::Value) -> &'static str {
    if stats.get("clustered").and_then(serde_json::Value::as_bool).unwrap_or(false) {
        "尾数聚集"
    } else {
        "未见聚集"
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
