// 五区章节分类器（规则先行、无模型，§5 W3-5）：把段落判入 legal/price/tech/business/other。
// 标题路径优先于正文关键词；legal（法定格式）与 price（报价清单）用于 compare_service 的分区
// 阈值分层——legal 区阈值上调只压套话雷同、price 区不做文本相似度（数值层 M6）。
// 供 chunker 导入期标注 section_kind、corpus 比对期重算（旧库不重导入即产出五区值）、
// compare_service 分区阈值与 scope 过滤。

/// 段落所属标段 / 分区。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Section {
    Tech,
    Business,
    /// 法定格式区：投标函 / 承诺 / 声明 / 资格审查 / 廉政等天然一致文本。阈值上调只压套话
    /// 雷同——不压法定格式内填空字段 / 错误一致（那是真信号，走共同错误指纹）。
    LegalFormat,
    /// 报价清单区：报价表 / 工程量清单 / 表格金额行。本里程碑不做文本相似度（数值层 M6），
    /// 该区文本雷同不进聚类 / 围标。
    Price,
    Other,
}

const BIZ_KW: &[&str] = &[
    "报价", "价格", "费用", "金额", "商务", "资质", "营业执照", "法定代表人", "法人",
    "投标函", "投标保证金", "财务", "审计", "纳税", "社保", "承诺函", "授权委托", "信誉",
    "业绩", "注册资本", "报价表",
];
const TECH_KW: &[&str] = &[
    "技术", "方案", "架构", "系统", "设计", "实施", "部署", "接口", "性能", "安全", "数据",
    "功能", "平台", "集成", "运维", "网络", "服务器", "算法", "模块", "容灾",
];
/// 法定格式区标志词（标题优先、正文回退）：命中即判 legal。
const LEGAL_KW: &[&str] = &[
    "投标函", "法定代表人", "授权委托", "承诺书", "承诺函", "声明", "资格审查", "廉政",
];
/// 报价清单区标题标志词：仅在标题路径命中时判 price（正文散见「报价」不足以定区，
/// 避免把商务陈述误压进不做文本比对的 price 区）。
const PRICE_TITLE_KW: &[&str] = &["报价", "清单", "工程量", "单价", "合价", "报价表"];

/// 关键词启发式：判段落属技术标 / 商务标 / 其他（classify_zone 的 tech/business/other 回退口径）。
pub fn classify(text: &str) -> Section {
    let b = BIZ_KW.iter().filter(|k| text.contains(**k)).count();
    let t = TECH_KW.iter().filter(|k| text.contains(**k)).count();
    if t > b {
        Section::Tech
    } else if b > t {
        Section::Business
    } else {
        Section::Other
    }
}

/// 五区分类（规则确定性、无模型）：标题路径优先于正文关键词。
/// 优先级：标题命中法定格式 → legal；标题命中报价标志词 → price；正文命中法定格式 → legal；
/// 表格行且含金额实体 → price（数值层证据，非文字雷同）；否则回退 tech/business/other 关键词多数决。
pub fn classify_zone(titles: &[String], text: &str, is_table_row: bool, has_amount: bool) -> Section {
    let title_hit = |kws: &[&str]| titles.iter().any(|t| kws.iter().any(|k| t.contains(k)));
    if title_hit(LEGAL_KW) {
        return Section::LegalFormat;
    }
    if title_hit(PRICE_TITLE_KW) {
        return Section::Price;
    }
    if LEGAL_KW.iter().any(|k| text.contains(k)) {
        return Section::LegalFormat;
    }
    if is_table_row && has_amount {
        return Section::Price;
    }
    classify(text)
}

/// Section → 落库 / 比对期 section_kind 字符串（chunker、corpus、compare_service 共用同一口径）。
pub fn section_kind_str(s: Section) -> &'static str {
    match s {
        Section::Tech => "tech",
        Section::Business => "business",
        Section::LegalFormat => "legal",
        Section::Price => "price",
        Section::Other => "other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_keyword_majority() {
        assert!(matches!(classify("系统采用分层解耦的微服务架构设计"), Section::Tech));
        assert!(matches!(classify("投标报价及投标保证金缴纳说明"), Section::Business));
        assert!(matches!(classify("本段为普通陈述无明显标段特征"), Section::Other));
    }

    #[test]
    fn classify_zone_title_legal_beats_body() {
        // 「投标函」标题 → legal（标题路径优先，即便正文全是技术词）。
        let z = classify_zone(&["投标函".into()], "系统架构设计技术方案", false, false);
        assert!(matches!(z, Section::LegalFormat));
    }

    #[test]
    fn classify_zone_amount_table_row_is_price() {
        // 含金额表格行 → price（数值层证据，即便无价目标题）。
        let z = classify_zone(&[], "分部分项 100000 元", true, true);
        assert!(matches!(z, Section::Price));
        // 标题命中报价标志词亦 → price。
        let z2 = classify_zone(&["已标价工程量清单".into()], "综合单价合价", false, false);
        assert!(matches!(z2, Section::Price));
    }

    #[test]
    fn classify_zone_tech_body_and_other_fallback() {
        // 微服务正文（无标题、无法定/价目特征）→ tech。
        let z = classify_zone(&[], "系统采用分层解耦的微服务架构设计", false, false);
        assert!(matches!(z, Section::Tech));
        // 无任何特征 → other。
        let z2 = classify_zone(&[], "本段为普通陈述无明显标段特征", false, false);
        assert!(matches!(z2, Section::Other));
    }

    #[test]
    fn classify_zone_non_table_amount_not_price() {
        // 非表格行即便含金额也不入 price（避免把普通报价陈述误压进不做文本比对的 price 区）。
        let z = classify_zone(&[], "本项目预算约 100000 元用于系统建设", false, true);
        assert!(!matches!(z, Section::Price));
    }

    #[test]
    fn section_kind_str_round_trip() {
        assert_eq!(section_kind_str(Section::LegalFormat), "legal");
        assert_eq!(section_kind_str(Section::Price), "price");
        assert_eq!(section_kind_str(Section::Tech), "tech");
        assert_eq!(section_kind_str(Section::Business), "business");
        assert_eq!(section_kind_str(Section::Other), "other");
    }
}
