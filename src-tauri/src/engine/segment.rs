// 标段分类：关键词启发式判段落属技术标 / 商务标 / 其他（供 chunker 标注 section_kind 与比对范围过滤）。

/// 段落所属标段。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Tech,
    Business,
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

/// 关键词启发式：判段落属技术标 / 商务标 / 其他。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_keyword_majority() {
        assert!(matches!(classify("系统采用分层解耦的微服务架构设计"), Section::Tech));
        assert!(matches!(classify("投标报价及投标保证金缴纳说明"), Section::Business));
        assert!(matches!(classify("本段为普通陈述无明显标段特征"), Section::Other));
    }
}
