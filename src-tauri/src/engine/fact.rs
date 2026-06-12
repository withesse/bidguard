// 事实抽取与冲突检测（设计文档 §9.9）：规则 + 正则 + 词典，服务于差异风险识别，不做完整 NLP。
// 量化字段（金额/日期/工期/比例）来自导入期实体抽取；主体/动作/条件用词典就近匹配。
// 冲突判定看量化字段与主体「阵营」：同一雷同条款里「金额 1280 万 vs 1290 万」、
// 「甲方承担 vs 乙方承担」才是风险；「我司 vs 我公司」属同阵营表述差异（正常噪声），
// 经 subject_group 归一后不触发冲突。
use crate::engine::features::Entity;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fact {
    pub subject: Option<String>,
    pub action: Option<String>,
    pub object: Option<String>,
    /// 单值字段取首个实体（展示用）；冲突判定用下方完整集合，避免多金额条款漏判
    pub amount: Option<String>,
    pub date: Option<String>,
    pub duration: Option<String>,
    pub percentage: Option<String>,
    pub condition: Option<String>,
    pub obligation_type: Option<String>,
    pub confidence: f32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub amounts: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dates: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub durations: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub percentages: Vec<String>,
}

const SUBJECTS: &[&str] = &[
    "招标人", "投标人", "中标人", "采购人", "供应商", "承包人", "发包人", "建设单位",
    "甲方", "乙方", "丙方", "我公司", "我司", "我方", "本公司", "对方",
];

/// 主体阵营归一（§9.9「责任主体冲突」的判定单位）：
/// 同阵营换说法（我司/我公司/投标人）是表述差异；跨阵营互换（甲方付款 ↔ 乙方付款）
/// 才是抄改漏改的实质冲突。无法归阵营的主体（丙方/对方）不参与冲突判定。
fn subject_group(s: &str) -> Option<&'static str> {
    match s {
        "招标人" | "采购人" | "甲方" | "发包人" | "建设单位" => Some("owner"),
        "投标人" | "中标人" | "供应商" | "承包人" | "乙方" | "我公司" | "我司" | "我方"
        | "本公司" => Some("supplier"),
        _ => None,
    }
}

/// (动作词, 义务类型)。按文中最早出现者取。
const ACTIONS: &[(&str, &str)] = &[
    ("支付", "付款义务"),
    ("缴纳", "付款义务"),
    ("退还", "付款义务"),
    ("结算", "付款义务"),
    ("交付", "交付义务"),
    ("交货", "交付义务"),
    ("提交", "交付义务"),
    ("送达", "交付义务"),
    ("保密", "保密义务"),
    ("赔偿", "责任义务"),
    ("承担", "责任义务"),
    ("违约", "责任义务"),
    ("保证", "担保义务"),
    ("质保", "担保义务"),
    ("维护", "服务义务"),
    ("响应", "服务义务"),
    ("验收", "履约义务"),
    ("完成", "履约义务"),
    ("实施", "履约义务"),
    ("部署", "履约义务"),
];

const OBJECTS: &[&str] = &[
    "履约保证金", "投标保证金", "服务费用", "质保金", "预付款", "进度款", "尾款",
    "保证金", "报告", "材料", "文件", "货物", "设备", "系统", "软件",
];

const CONDITION_LEADS: &[&str] = &["如果", "如发生", "一旦", "倘若", "若", "如", "凡", "当"];

/// 最早出现者；同位置取最长词（「履约保证金」优先于其内含的「保证金」）。
/// 跨词典的内含误判（如名词内的动词字）属可接受噪声：这些字段只做上下文展示，不触发冲突。
fn earliest<'a>(text: &str, dict: &[&'a str]) -> Option<&'a str> {
    dict.iter()
        .filter_map(|w| text.find(*w).map(|pos| (pos, *w)))
        .min_by(|a, b| a.0.cmp(&b.0).then(b.1.len().cmp(&a.1.len())))
        .map(|(_, w)| w)
}

/// 从一段条款文本 + 其实体抽取事实。entities 来自导入期（已归一文本上抽取，
/// 金额/数字已是阿拉伯形态）。量化字段保留完整集合供冲突判定。
pub fn extract(text: &str, entities: &[Entity]) -> Fact {
    let pick_all = |kind: &str| -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for e in entities.iter().filter(|e| e.kind == kind) {
            if !out.contains(&e.value) {
                out.push(e.value.clone());
            }
        }
        out
    };
    let subject = earliest(text, SUBJECTS).map(str::to_string);
    let (action, obligation_type) = ACTIONS
        .iter()
        .filter_map(|(w, ob)| text.find(w).map(|pos| (pos, *w, *ob)))
        .min_by(|a, b| a.0.cmp(&b.0).then(b.1.len().cmp(&a.1.len())))
        .map(|(_, w, ob)| (Some(w.to_string()), Some(ob.to_string())))
        .unwrap_or((None, None));
    let object = earliest(text, OBJECTS).map(str::to_string);
    // 条件：引导词起取一小段作为表达式（足够人工定位即可）
    let condition = CONDITION_LEADS
        .iter()
        .filter_map(|w| text.find(*w).map(|pos| (pos, w.len())))
        .min_by_key(|(pos, _)| *pos)
        .map(|(pos, _)| text[pos..].chars().take(16).collect::<String>());

    let amounts = pick_all("amount");
    let dates = pick_all("date");
    let durations = pick_all("duration");
    let percentages = pick_all("percentage");
    let amount = amounts.first().cloned();
    let date = dates.first().cloned();
    let duration = durations.first().cloned();
    let percentage = percentages.first().cloned();

    let hits = [
        subject.is_some(),
        action.is_some(),
        object.is_some(),
        amount.is_some(),
        date.is_some(),
        duration.is_some(),
        percentage.is_some(),
        condition.is_some(),
    ]
    .iter()
    .filter(|b| **b)
    .count();
    Fact {
        subject,
        action,
        object,
        amount,
        date,
        duration,
        percentage,
        condition,
        obligation_type,
        confidence: hits as f32 / 8.0,
        amounts,
        dates,
        durations,
        percentages,
    }
}

// —— 冲突判定 ——

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocValue {
    pub doc: usize,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldConflict {
    pub field: String, // amount | duration | date | percentage
    pub values: Vec<DocValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactConflict {
    pub risk: String, // high | medium | review（§9.9 风险表）
    pub fields: Vec<FieldConflict>,
}

/// 跨文档比较同一条款的事实（按字段的完整值集合）。
/// 冲突 = 存在两文档的集合互不包含（各自都有对方没有的值）；
/// 子集关系视为信息缺失而非矛盾（A 列了三笔款、B 只提了首笔 ≠ 冲突）。
/// 主体按阵营归一后比较：跨阵营互换（甲方 ↔ 乙方）才算冲突。
/// 金额/工期/日期/主体冲突 → high；仅比例冲突 → medium；涉事事实置信度过低 → review。
pub fn conflicts_between(facts: &[(usize, &Fact)]) -> Option<FactConflict> {
    use std::collections::BTreeSet;
    type FieldGetter = fn(&Fact) -> &Vec<String>;
    let mut fields: Vec<FieldConflict> = Vec::new();
    let getters: [(&str, FieldGetter); 4] = [
        ("amount", |f| &f.amounts),
        ("duration", |f| &f.durations),
        ("date", |f| &f.dates),
        ("percentage", |f| &f.percentages),
    ];
    for (name, get) in getters {
        let per_doc: Vec<(usize, BTreeSet<&str>)> = facts
            .iter()
            .map(|(doc, f)| (*doc, get(f).iter().map(String::as_str).collect::<BTreeSet<_>>()))
            .filter(|(_, s)| !s.is_empty())
            .collect();
        let mut conflicted = false;
        for (x, (_, sa)) in per_doc.iter().enumerate() {
            for (_, sb) in per_doc.iter().skip(x + 1).map(|(d, s)| (d, s)) {
                if !sa.is_subset(sb) && !sb.is_subset(sa) {
                    conflicted = true;
                }
            }
        }
        if conflicted {
            fields.push(FieldConflict {
                field: name.to_string(),
                values: per_doc
                    .iter()
                    .map(|(doc, s)| DocValue {
                        doc: *doc,
                        value: s.iter().copied().collect::<Vec<_>>().join("、"),
                    })
                    .collect(),
            });
        }
    }

    // 主体阵营冲突：两文档的主体归属不同阵营（甲方付款 ↔ 乙方付款）。
    {
        let per_doc: Vec<(usize, &str, &'static str)> = facts
            .iter()
            .filter_map(|(doc, f)| {
                let s = f.subject.as_deref()?;
                Some((*doc, s, subject_group(s)?))
            })
            .collect();
        let conflicted = per_doc
            .iter()
            .any(|(_, _, ga)| per_doc.iter().any(|(_, _, gb)| ga != gb));
        if conflicted {
            fields.push(FieldConflict {
                field: "subject".to_string(),
                values: per_doc
                    .iter()
                    .map(|(doc, s, _)| DocValue { doc: *doc, value: (*s).to_string() })
                    .collect(),
            });
        }
    }

    if fields.is_empty() {
        return None;
    }
    let involved_min_conf = facts
        .iter()
        .map(|(_, f)| f.confidence)
        .fold(f32::MAX, f32::min);
    // 0.2 ≈ 8 字段只命中 1 个：孤立数字缺乏「这是同一类条款」的上下文佐证，转人工复核
    let risk = if involved_min_conf < 0.2 {
        "review"
    } else if fields
        .iter()
        .any(|f| matches!(f.field.as_str(), "amount" | "duration" | "date" | "subject"))
    {
        "high"
    } else {
        "medium"
    };
    Some(FactConflict {
        risk: risk.to_string(),
        fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::features::extract_entities;

    fn fact_of(normalized: &str) -> Fact {
        extract(normalized, &extract_entities(normalized))
    }

    #[test]
    fn extracts_quantitative_fields() {
        // 金额 ×3
        for (t, want) in [
            ("投标报价为人民币12800000元整", "12800000元"),
            ("履约保证金为500000元", "500000元"),
            ("合同总价3200万元", "3200万元"),
        ] {
            assert_eq!(fact_of(t).amount.as_deref(), Some(want), "{t}");
        }
        // 工期 ×3
        for (t, want) in [
            ("建设周期为180个日历日", "180个日历日"),
            ("质保期为3年", "3年"),
            ("收到通知后7个工作日内答复", "7个工作日"),
        ] {
            assert_eq!(fact_of(t).duration.as_deref(), Some(want), "{t}");
        }
        // 日期 ×3
        for (t, want) in [
            ("于2026年6月10日前开工", "2026年6月10日"),
            ("2026-09-30交付", "2026-09-30"),
            ("计划2027年完成验收", "2027年"),
        ] {
            assert_eq!(fact_of(t).date.as_deref(), Some(want), "{t}");
        }
        // 比例 ×3
        for (t, want) in [
            ("质保金比例为5%", "5%"),
            ("预付款为合同价的30%", "30%"),
            ("违约金按0.5%每日计", "0.5%"),
        ] {
            assert_eq!(fact_of(t).percentage.as_deref(), Some(want), "{t}");
        }
    }

    #[test]
    fn extracts_context_fields() {
        let f = fact_of("如发生违约，乙方应在30日内向甲方支付服务费用的10%作为违约金");
        assert_eq!(f.subject.as_deref(), Some("乙方"), "取最早出现的主体");
        assert_eq!(f.action.as_deref(), Some("违约"));
        assert_eq!(f.obligation_type.as_deref(), Some("责任义务"));
        assert_eq!(f.object.as_deref(), Some("服务费用"));
        assert!(f.condition.as_deref().unwrap().starts_with("如发生"));
        assert!(f.confidence > 0.5, "命中多字段，置信度应较高：{}", f.confidence);
    }

    #[test]
    fn conflict_detection_and_grading() {
        let a = fact_of("投标人投标报价为12800000元，工期180个日历日");
        let b = fact_of("投标人投标报价为12900000元，工期180个日历日");
        let c = conflicts_between(&[(0, &a), (1, &b)]).expect("金额不同应判冲突");
        assert_eq!(c.risk, "high");
        assert_eq!(c.fields.len(), 1);
        assert_eq!(c.fields[0].field, "amount");
        assert_eq!(c.fields[0].values.len(), 2);

        // 仅比例不同 → medium
        let p1 = fact_of("投标人质保金比例为5%并承担维护");
        let p2 = fact_of("投标人质保金比例为10%并承担维护");
        let c = conflicts_between(&[(0, &p1), (1, &p2)]).unwrap();
        assert_eq!(c.risk, "medium");

        // 完全一致 → 无冲突
        assert!(conflicts_between(&[(0, &a), (1, &a)]).is_none());

        // 单方缺失字段 → 不算冲突（缺失 ≠ 不同）
        let no_amount = fact_of("投标人承诺优质服务并承担维护");
        assert!(conflicts_between(&[(0, &a), (1, &no_amount)]).is_none());

        // 多金额条款：集合互不包含 → 冲突；子集 → 信息缺失非冲突
        let m1 = fact_of("投标人预付款5000000元，进度款8000000元，尾款7000000元");
        let m2 = fact_of("投标人预付款5000000元，进度款8000000元，尾款6000000元");
        let c = conflicts_between(&[(0, &m1), (1, &m2)]).expect("尾款不同应判冲突");
        assert_eq!(c.risk, "high");
        assert!(c.fields[0].values[0].value.contains("7000000元"));
        let subset = fact_of("投标人预付款5000000元，进度款8000000元");
        assert!(
            conflicts_between(&[(0, &m1), (1, &subset)]).is_none(),
            "子集是信息缺失，不是矛盾"
        );

        // 同位置最长词优先：「履约保证金」不被内含的「保证金」截胡
        let f = fact_of("投标人缴纳的履约保证金为1000000元");
        assert_eq!(f.object.as_deref(), Some("履约保证金"));

        // 置信度过低 → review
        let bare1 = fact_of("5%");
        let bare2 = fact_of("8%");
        let c = conflicts_between(&[(0, &bare1), (1, &bare2)]).unwrap();
        assert_eq!(c.risk, "review", "孤立数字没有上下文，应转人工复核");
    }

    #[test]
    fn subject_camp_conflict_detection() {
        // 跨阵营互换（抄改漏改的典型）→ 冲突 high
        let a = fact_of("甲方承担本系统的全部维护费用与升级服务");
        let b = fact_of("乙方承担本系统的全部维护费用与升级服务");
        let c = conflicts_between(&[(0, &a), (1, &b)]).expect("甲乙互换应判主体冲突");
        assert_eq!(c.risk, "high");
        assert!(c.fields.iter().any(|f| f.field == "subject"));
        let sf = c.fields.iter().find(|f| f.field == "subject").unwrap();
        assert!(sf.values.iter().any(|v| v.value == "甲方"));
        assert!(sf.values.iter().any(|v| v.value == "乙方"));

        // 同阵营换说法（我司/我公司/投标人）→ 不触发
        let x = fact_of("我司承担质保期内的全部维护工作");
        let y = fact_of("我公司承担质保期内的全部维护工作");
        let z = fact_of("投标人承担质保期内的全部维护工作");
        assert!(conflicts_between(&[(0, &x), (1, &y)]).is_none(), "同阵营表述差异不是冲突");
        assert!(conflicts_between(&[(0, &x), (1, &z)]).is_none());

        // 无法归阵营的主体（对方）不参与判定
        let amb = fact_of("对方承担违约责任");
        assert!(conflicts_between(&[(0, &a), (1, &amb)]).is_none());

        // 主体冲突叠加金额冲突仍是 high，且两个字段都报
        let m1 = fact_of("甲方支付服务费用1000000元");
        let m2 = fact_of("乙方支付服务费用2000000元");
        let c = conflicts_between(&[(0, &m1), (1, &m2)]).unwrap();
        assert_eq!(c.risk, "high");
        assert!(c.fields.iter().any(|f| f.field == "amount"));
        assert!(c.fields.iter().any(|f| f.field == "subject"));
    }
}
