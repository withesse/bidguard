// 文本标准化（设计文档 §8.3）：NFKC、全半角、空白、标点、中文数字归一。
// 目标是让「每月十日前支付」与「每月 10 日前 支付」归一到同一形态，
// 降低无意义差异对相似度与 hash 命中的干扰。
//
// W2 入口对抗层（执行方案 §4 条目 1/2）：NFKC 不删除零宽/双向控制符，1-3 个隐形码点
// 即可让 exact_hash / normalized_hash / MinHash / embedding 全部失配（Bad Characters,
// IEEE S&P 2022）。故 NFKC 之后显式剥离隐形码点并做跨脚本同形字折叠，逐类计数——
// 正常标书不含这些码点，非零计数本身就是高置信规避证据（供围标 evasion 信号消费）。
use crate::engine::confusables;
use serde::Serialize;
use sha2::{Digest, Sha256};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone)]
pub struct NormalizeOptions {
    pub ignore_case: bool,
    pub ignore_punctuation: bool,
    pub ignore_whitespace: bool,
}

impl Default for NormalizeOptions {
    fn default() -> Self {
        Self {
            ignore_case: true,
            ignore_punctuation: true,
            ignore_whitespace: true,
        }
    }
}

/// 隐形码点剥离 + 同形字折叠 + 混合脚本红旗的逐类统计（W2 入口对抗层）。
/// 字段口径同时用于块级分布（chunk_features.extra_json）与文档级聚合
/// （documents.evasion_json），serde camelCase 与前端 DTO 惯例一致。
/// 全零即「无发现」——干净文本不产生任何统计负担。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InvisibleStats {
    /// 零宽字符：U+200B–U+200D、U+200E/200F、U+FEFF。
    pub zero_width: u32,
    /// 双向控制符：U+202A–U+202E、U+2066–U+2069。
    pub bidi: u32,
    /// Tags 块：U+E0000–U+E007F。
    pub tags: u32,
    /// 变体选择符：U+FE00–U+FE0F、U+E0100–U+E01EF。
    pub variation: u32,
    /// 跨脚本同形字折叠命中数（confusables::fold）。
    pub confusable_folds: u32,
    /// 同词内混合脚本红旗数（confusables::scan_mixed_script）。
    pub mixed_script_words: u32,
    /// 混合脚本采样词（证据下钻用；块内去重、有上限，见 confusables::SAMPLE_MAX）。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub mixed_script_samples: Vec<String>,
}

impl InvisibleStats {
    /// 被剥离的隐形码点总数。
    pub fn stripped_total(&self) -> u32 {
        self.zero_width + self.bidi + self.tags + self.variation
    }

    /// 改写类命中总数（剥离 + 折叠），用于「单块浓度」——混合脚本红旗是
    /// 检测信号不改写文本，不计入浓度分母口径。
    pub fn perturbation_total(&self) -> u32 {
        self.stripped_total() + self.confusable_folds
    }

    /// 无任何发现（块级不落 extra_json、文档级不写 evasion_json 的判据）。
    pub fn is_clean(&self) -> bool {
        self.perturbation_total() == 0 && self.mixed_script_words == 0
    }
}

/// 隐形码点分类（W2-1 剥离集合）。范围取自执行方案拍板值：Bad Characters 攻击的
/// 全部注入面 + 方向控制符 + Tags 隐写块 + 变体选择符。emoji ZWJ 序列 / 真实 RTL
/// 文段会被误剥离，但标书语料基本不含，且 chunks.text 保留原始字节可回查。
enum InvisibleClass {
    ZeroWidth,
    Bidi,
    Tags,
    Variation,
}

fn invisible_class(c: char) -> Option<InvisibleClass> {
    match c as u32 {
        0x200B..=0x200F | 0xFEFF => Some(InvisibleClass::ZeroWidth),
        0x202A..=0x202E | 0x2066..=0x2069 => Some(InvisibleClass::Bidi),
        0xE0000..=0xE007F => Some(InvisibleClass::Tags),
        0xFE00..=0xFE0F | 0xE0100..=0xE01EF => Some(InvisibleClass::Variation),
        _ => None,
    }
}

/// 单遍剥离隐形码点并逐类计数。调用方已确认文本含隐形码点（见 normalize_with_stats
/// 的预扫快路径），故直接重建字符串。
fn strip_invisible(s: &str, stats: &mut InvisibleStats) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match invisible_class(c) {
            Some(InvisibleClass::ZeroWidth) => stats.zero_width += 1,
            Some(InvisibleClass::Bidi) => stats.bidi += 1,
            Some(InvisibleClass::Tags) => stats.tags += 1,
            Some(InvisibleClass::Variation) => stats.variation += 1,
            None => out.push(c),
        }
    }
    out
}

/// 归一化文本：NFKC（全角→半角）→ 中文数字+单位转阿拉伯 → 大小写/标点/空白处理。
/// 丢弃统计的薄包装；需要规避统计的调用方（chunker）用 normalize_with_stats。
// 生产入口 chunker 已改走 normalize_with_stats，本包装当前仅测试在用；保留为稳定
// 简单 API——M2 的渲染-OCR 交叉验证（W2-4）比对双方文本时将直接使用。
#[allow(dead_code)]
pub fn normalize(text: &str, opts: &NormalizeOptions) -> String {
    normalize_with_stats(text, opts).0
}

/// 带规避统计的归一化 = 前置清洗（sanitize_with_stats）+ 后半程（normalize_sanitized）。
/// 拆成两段的原因见各自注释；只要终态的调用方用本函数即可，行为与拆分前完全一致。
pub fn normalize_with_stats(text: &str, opts: &NormalizeOptions) -> (String, InvisibleStats) {
    let (s, stats) = sanitize_with_stats(text);
    (normalize_sanitized(&s, opts), stats)
}

/// 前置清洗（W2 入口对抗层）：NFKC → 隐形码点剥离 → 混合脚本扫描 → 同形字折叠。
/// 顺序约束：剥离在扫描/折叠之前（零宽插入不能拆散同形词的字符 run）；
/// 扫描在折叠之前（折叠会把西里尔改写成拉丁，先扫后折才能留下红旗证据）。
///
/// 单独暴露给分词方（chunker 分块 / import_service 模板）：token_json 也是特征列，
/// 必须基于清洗后文本（执行方案 §4 W2-1「全部特征基于清洗后文本」）——tokens 若直接
/// 来自原文，词内零宽/同形注入会把词拆碎/变形，lexical 通道（tfidf 余弦、共有词交集、
/// 模板余弦）在哈希一致性恢复后仍被击穿。但分词也不能吃归一化终态：cn_numbers 改写
/// 数词、去标点/空白粘连词边界，都会偏离既有分词口径，故以本中间产物为分词输入。
pub fn sanitize_with_stats(text: &str) -> (String, InvisibleStats) {
    let mut stats = InvisibleStats::default();
    let mut s: String = text.nfkc().collect();
    // 预扫快路径：隐形码点与希腊/西里尔字符在正常标书中都不出现，先一遍探测再决定
    // 是否走剥离/折叠重建，让干净文本只多付一次线性扫描（验收：10 万字增耗 <5%）。
    let mut has_invisible = false;
    let mut has_foreign = false;
    for c in s.chars() {
        if invisible_class(c).is_some() {
            has_invisible = true;
        } else if confusables::is_cyrillic(c) || confusables::is_greek(c) {
            has_foreign = true;
        }
    }
    if has_invisible {
        s = strip_invisible(&s, &mut stats);
    }
    if has_foreign {
        confusables::scan_mixed_script(&s, &mut stats);
        s = confusables::fold(&s, &mut stats);
    }
    (s, stats)
}

/// 归一化后半程：中文数字归一 → 大小写/标点/空白处理。
/// 输入必须是 sanitize_with_stats 的产物——chunker 先取中间产物喂分词、再走本函数
/// 得 normalized_text，两步复用同一次 NFKC/剥离/折叠，避免重复清洗。
pub fn normalize_sanitized(sanitized: &str, opts: &NormalizeOptions) -> String {
    let s = normalize_cn_numbers(sanitized);
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    for (i, &c) in chars.iter().enumerate() {
        if c.is_whitespace() {
            if !opts.ignore_whitespace {
                out.push(' ');
            }
            continue;
        }
        if is_punct(c) {
            // 数字内的小数点与千分位逗号保留：否则「1,000,000.00」被删成「100000000」腐化金额
            let digit_inner = (c == '.' || c == ',')
                && i > 0
                && chars[i - 1].is_ascii_digit()
                && chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if digit_inner {
                out.push(c);
                continue;
            }
            if !opts.ignore_punctuation {
                out.push(half_punct(c));
            }
            continue;
        }
        if opts.ignore_case {
            for lc in c.to_lowercase() {
                out.push(lc);
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// CJK 与 ASCII 常见标点（NFKC 之后大部分全角符号已映射为 ASCII）。
/// 兼容竖排呈现形式（U+FE30..FE4F，老式竖排 PDF 转换产物）。
fn is_punct(c: char) -> bool {
    c.is_ascii_punctuation()
        || matches!(c as u32, 0xFE30..=0xFE4F)
        || matches!(c,
            '。' | '，' | '、' | '；' | '：' | '？' | '！' | '“' | '”' | '‘' | '’'
            | '（' | '）' | '《' | '》' | '〈' | '〉' | '【' | '】' | '〔' | '〕'
            | '「' | '」' | '『' | '』' | '…' | '—' | '–' | '·' | '￥' | '～' | '〜')
}

/// 保留标点时的半角归一（NFKC 漏掉的 CJK 标点）。
fn half_punct(c: char) -> char {
    match c {
        '。' => '.',
        '，' | '、' => ',',
        '；' => ';',
        '：' => ':',
        '？' => '?',
        '！' => '!',
        '“' | '”' => '"',
        '‘' | '’' => '\'',
        '（' => '(',
        '）' => ')',
        '《' | '〈' | '「' | '『' | '【' | '〔' => '<',
        '》' | '〉' | '」' | '』' | '】' | '〕' => '>',
        '·' => '.',
        '～' | '〜' => '~',
        other => other,
    }
}

// 含法定大写（壹贰叁肆伍陆柒捌玖拾佰仟）——投标报价条款最常用大写，须走同一归一路径。
const CN_DIGITS: &str = "零一二三四五六七八九十百千万亿两壹贰貳叁肆伍陆柒捌玖拾佰仟";

/// 单位词表：只有「中文数字串 + 单位」才转换，避免误伤「一致」「统一」这类普通词。
/// 按长度降序做最长匹配。
const UNITS: &[&str] = &[
    "日历日", "工作日", "个月", "万元", "小时", "分钟", "日", "天", "月", "年", "元",
    "个", "周", "次", "期", "项", "条", "款", "名", "家", "%", "％",
];

/// 大写数字字符集（法定金额大写）；用于把大写数字转换限制在金额语境，避免误伤「陆家嘴」等专名。
const UPPER_CN: &str = "壹贰貳叁肆伍陆柒捌玖拾佰仟";

/// 逐位写法的数字字符（年份「二〇二六」/「贰零贰陆」按位拼接，不走 cn_to_num 进位逻辑）。
fn plain_digit(c: char) -> Option<char> {
    Some(match c {
        '零' | '〇' => '0',
        '一' | '壹' => '1',
        '二' | '贰' | '貳' => '2',
        '三' | '叁' => '3',
        '四' | '肆' => '4',
        '五' | '伍' => '5',
        '六' | '陆' => '6',
        '七' | '柒' => '7',
        '八' | '捌' => '8',
        '九' | '玖' => '9',
        _ => return None,
    })
}

/// 中文数字归一：
/// 1) 「百分之三十」→「30%」；
/// 2) 「一百八十个日历日」→「180个日历日」、「十日」→「10日」；
/// 3) 「5万元」→「50000元」（与中文数字路径对称，否则「五万元」「5万元」归一结果不一致）；
/// 4) 逐位年份「二〇二六年」「二零二六年」→「2026年」（cn_to_num 的进位逻辑会把
///    逐位串算错成 6，必须先于模式 2 拦截）。
fn normalize_cn_numbers(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        // 模式 1：百分之 + 中文数字
        if chars[i..].starts_with(&['百', '分', '之']) {
            let start = i + 3;
            let mut j = start;
            while j < chars.len() && CN_DIGITS.contains(chars[j]) {
                j += 1;
            }
            if j > start {
                if let Some(n) = cn_to_num(&chars[start..j]) {
                    out.push_str(&n.to_string());
                    out.push('%');
                    i = j;
                    continue;
                }
            }
        }
        // 模式 4：逐位数字串 + 年（≥2 位才视为逐位写法，「五年」仍走模式 2）
        if plain_digit(chars[i]).is_some() {
            let mut j = i;
            while j < chars.len() && plain_digit(chars[j]).is_some() {
                j += 1;
            }
            if j - i >= 2 && chars.get(j) == Some(&'年') {
                for &c in &chars[i..j] {
                    out.push(plain_digit(c).expect("已校验为逐位数字"));
                }
                out.push('年');
                i = j + 1;
                continue;
            }
        }
        // 模式 2：中文数字串 + 单位
        if CN_DIGITS.contains(chars[i]) {
            let mut j = i;
            while j < chars.len() && CN_DIGITS.contains(chars[j]) {
                j += 1;
            }
            if let Some(unit_len) = match_unit(&chars[j..]) {
                // 「十个亿」这类「数字+个+量级」是复合数词，拆开会产生畸形文本，跳过
                let unit_is_ge_scale = chars[j] == '个'
                    && chars
                        .get(j + unit_len)
                        .is_some_and(|c| *c == '万' || *c == '亿');
                // 大写数字仅在金额/百分比语境转换（后随 万/亿/元/圆/%）：否则「陆家嘴」「玖月」
                // 会被当数字腐化成「6家嘴」「9月」。小写数字不受此限（行为不变）。
                let run_has_upper = chars[i..j].iter().any(|c| UPPER_CN.contains(*c));
                let unit = &chars[j..j + unit_len];
                let money_ctx = unit
                    .iter()
                    .any(|c| matches!(c, '元' | '圆' | '万' | '亿' | '%' | '％'));
                let upper_ok = !run_has_upper || money_ctx;
                if !unit_is_ge_scale && upper_ok {
                    if let Some(n) = cn_to_num(&chars[i..j]) {
                        out.push_str(&n.to_string());
                        out.extend(&chars[j..j + unit_len]);
                        i = j + unit_len;
                        continue;
                    }
                }
            }
        }
        // 模式 3：阿拉伯数字 + 万/亿 + 单位
        if chars[i].is_ascii_digit() {
            let mut j = i;
            while j < chars.len() && (chars[j].is_ascii_digit() || chars[j] == '.') {
                j += 1;
            }
            if j < chars.len() && (chars[j] == '万' || chars[j] == '亿') {
                let scale: f64 = if chars[j] == '万' { 1e4 } else { 1e8 };
                if match_unit(&chars[j + 1..]).is_some() {
                    let num: String = chars[i..j].iter().collect();
                    if let Ok(v) = num.parse::<f64>() {
                        let total = v * scale;
                        if total.fract() == 0.0 && total > 0.0 && total < 9e15 {
                            out.push_str(&(total as u64).to_string());
                            i = j + 1; // 跳过 万/亿，单位原样保留
                            continue;
                        }
                    }
                }
            }
            out.extend(&chars[i..j]);
            i = j;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn match_unit(rest: &[char]) -> Option<usize> {
    for u in UNITS {
        let uc: Vec<char> = u.chars().collect();
        if rest.len() >= uc.len() && rest[..uc.len()] == uc[..] {
            return Some(uc.len());
        }
    }
    None
}

/// 中文数字 → 整数。支持 零一..九 十 百 千 万 亿 两，以及口语缩写
/// （「一万二」=12000、「一百八」=180；「零」显式取消缩写：「一百零八」=108）。
fn cn_to_num(chars: &[char]) -> Option<u64> {
    if chars.is_empty() {
        return None;
    }
    let digit = |c: char| -> Option<u64> {
        Some(match c {
            '零' => 0,
            '一' | '壹' => 1,
            '二' | '两' | '贰' | '貳' => 2,
            '三' | '叁' => 3,
            '四' | '肆' => 4,
            '五' | '伍' => 5,
            '六' | '陆' => 6,
            '七' | '柒' => 7,
            '八' | '捌' => 8,
            '九' | '玖' => 9,
            _ => return None,
        })
    };
    let mut total: u64 = 0; // 亿以上累计
    let mut section: u64 = 0; // 当前万以下小节
    let mut cur: u64 = 0; // 当前位数字
    let mut shorthand: u64 = 0; // 末尾裸数字的隐含倍率（紧跟 百/千/万/亿 时生效）
    let mut any = false;
    for &c in chars {
        if let Some(d) = digit(c) {
            if d == 0 {
                shorthand = 0; // 「零」显式归位
                cur = 0;
            } else {
                cur = d;
            }
            any = true;
            continue;
        }
        match c {
            '十' | '拾' => {
                // 「十」开头表示 1 十
                section += if cur == 0 { 10 } else { cur * 10 };
                cur = 0;
                shorthand = 0;
                any = true;
            }
            '百' | '佰' => {
                section += cur.checked_mul(100)?;
                cur = 0;
                shorthand = 10;
            }
            '千' | '仟' => {
                section += cur.checked_mul(1000)?;
                cur = 0;
                shorthand = 100;
            }
            '万' => {
                section = (section + cur).checked_mul(10_000)?;
                total = total.checked_add(section)?;
                section = 0;
                cur = 0;
                shorthand = 1000;
            }
            '亿' => {
                let v = (total + section + cur).checked_mul(100_000_000)?;
                total = v;
                section = 0;
                cur = 0;
                shorthand = 10_000_000;
            }
            _ => return None,
        }
    }
    if !any && total == 0 && section == 0 && cur == 0 {
        return None;
    }
    // 末尾裸数字紧跟量级单位 → 口语缩写（一万二 = 1万 + 2×1000）
    if cur > 0 && shorthand > 0 {
        return total.checked_add(section)?.checked_add(cur.checked_mul(shorthand)?);
    }
    Some(total + section + cur)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cn_numbers() {
        let cases: &[(&str, u64)] = &[
            ("十", 10),
            ("十五", 15),
            ("三十", 30),
            ("一百八十", 180),
            ("一千两百九十", 1290),
            ("一千两百八十万", 12_800_000),
            ("一亿二千万", 120_000_000),
            ("两百", 200),
            ("零", 0),
            // 口语缩写：末尾裸数字承接上一级量纲
            ("一万二", 12_000),
            ("两万三", 23_000),
            ("一千二", 1_200),
            ("一百八", 180),
            ("一亿二", 120_000_000),
            // 「零」显式取消缩写
            ("一百零八", 108),
            ("一千零五十", 1_050),
        ];
        for (s, want) in cases {
            let chars: Vec<char> = s.chars().collect();
            assert_eq!(cn_to_num(&chars), Some(*want), "{s}");
        }
        assert_eq!(cn_to_num(&['致']), None);
    }

    #[test]
    fn arabic_scale_units_normalize_symmetrically() {
        // 「5万元」与「五万元」必须归一到同一形态，否则实体比对误判
        assert_eq!(normalize_cn_numbers("投资5万元"), "投资50000元");
        assert_eq!(normalize_cn_numbers("投资五万元"), "投资50000元");
        assert_eq!(normalize_cn_numbers("预算1.2万元"), "预算12000元");
        assert_eq!(normalize_cn_numbers("总额5亿元"), "总额500000000元");
        // 无后续单位不转换（与中文路径对称）
        assert_eq!(normalize_cn_numbers("市值5万左右"), "市值5万左右");
        // 「十个亿」是复合数词，不应拆成畸形文本
        assert_eq!(normalize_cn_numbers("十个亿的市场"), "十个亿的市场");
    }

    #[test]
    fn number_unit_conversion_is_targeted() {
        assert_eq!(normalize_cn_numbers("每月十日前"), "每月10日前");
        assert_eq!(normalize_cn_numbers("工期一百八十个日历日"), "工期180个日历日");
        assert_eq!(
            normalize_cn_numbers("投标报价为人民币一千两百八十万元整"),
            "投标报价为人民币12800000元整"
        );
        assert_eq!(normalize_cn_numbers("百分之三十"), "30%");
        // 普通词不受影响：数字后无单位不转换
        assert_eq!(normalize_cn_numbers("方案保持一致"), "方案保持一致");
        assert_eq!(normalize_cn_numbers("统一接口网关"), "统一接口网关");
    }

    #[test]
    fn uppercase_cn_amounts_normalize() {
        // 法定大写（投标报价条款最常用）走同一路径归一为阿拉伯金额
        assert_eq!(normalize_cn_numbers("投资伍万元"), "投资50000元");
        assert_eq!(normalize_cn_numbers("合同价壹佰万元整"), "合同价1000000元整");
        assert_eq!(normalize_cn_numbers("金额壹仟贰佰捌拾万元"), "金额12800000元");
        assert_eq!(cn_to_num(&"壹佰万".chars().collect::<Vec<_>>()), Some(1_000_000));
        assert_eq!(cn_to_num(&"伍万".chars().collect::<Vec<_>>()), Some(50_000));
        // 金额语境仍转
        assert_eq!(normalize_cn_numbers("陆万元"), "60000元");
        // 非金额语境不转：大写数字碰短单位不腐化专名（陆家嘴/玖月/伍家渠）
        assert_eq!(normalize_cn_numbers("陆家嘴金融中心"), "陆家嘴金融中心");
        assert_eq!(normalize_cn_numbers("玖月奇迹项目"), "玖月奇迹项目");
        assert_eq!(normalize_cn_numbers("伍家渠市"), "伍家渠市");
        // 大写逐位年份走 plain_digit 拦截，不被进位算错
        assert_eq!(normalize_cn_numbers("贰零贰陆年"), "2026年");
    }

    #[test]
    fn digit_punctuation_preserved() {
        // 数字内的千分位与小数点保留，金额不被删标点腐化
        assert_eq!(normalize("¥1,000,000.00", &NormalizeOptions::default()), "¥1,000,000.00");
        assert_eq!(normalize("合同价1,000,000元", &NormalizeOptions::default()), "合同价1,000,000元");
        // 句末中文句号仍按标点处理（不保留）
        assert_eq!(normalize("完成。", &NormalizeOptions::default()), "完成");
    }

    #[test]
    fn digitwise_cn_dates_normalize() {
        // 逐位年份（公文常见写法），〇 与 零 两种写法都要归一
        assert_eq!(normalize_cn_numbers("二〇二六年六月十一日开工"), "2026年6月11日开工");
        assert_eq!(normalize_cn_numbers("二零二六年三月"), "2026年3月");
        // 回归：cn_to_num 的进位逻辑会把逐位串「二零二六」算错成 6，必须拦截
        assert_ne!(normalize_cn_numbers("二零二六年"), "6年");
        // 单数字/进位写法仍走原路径
        assert_eq!(normalize_cn_numbers("质保期五年"), "质保期5年");
        assert_eq!(normalize_cn_numbers("使用寿命三十年"), "使用寿命30年");
        assert_eq!(normalize_cn_numbers("两千年古城"), "2000年古城");
    }

    #[test]
    fn cn_and_arabic_dates_normalize_to_same_form() {
        let opts = NormalizeOptions::default();
        let a = normalize("竣工日期为二〇二六年十二月三十一日。", &opts);
        let b = normalize("竣工日期为 2026年12月31日。", &opts);
        assert_eq!(a, b);
    }

    #[test]
    fn doc_example_normalizes_to_same_form() {
        let opts = NormalizeOptions::default();
        let a = normalize("甲方应在每月十日前支付服务费用。", &opts);
        let b = normalize("甲方 应 在 每月 10 日前 支付 服务费用", &opts);
        assert_eq!(a, b);
        assert_eq!(a, "甲方应在每月10日前支付服务费用");
    }

    #[test]
    fn nfkc_case_punct_whitespace() {
        let opts = NormalizeOptions::default();
        assert_eq!(normalize("ＡＢＣ\u{3000}１２３", &opts), "abc123");
        assert_eq!(normalize("你好，世界。", &opts), "你好世界");
        // 保留标点时做半角归一
        let keep = NormalizeOptions {
            ignore_punctuation: false,
            ..Default::default()
        };
        assert_eq!(normalize("你好，世界。", &keep), "你好,世界.");
    }

    #[test]
    fn hashes_are_stable() {
        assert_eq!(sha256_hex(b"abc").len(), 64);
        assert_eq!(sha256_hex(b"abc"), sha256_hex(b"abc"));
    }

    #[test]
    fn invisible_codepoints_stripped_hash_matches_clean_text() {
        // 验收用例：同段文本插入 3 个隐形字符（U+200B/U+202E/U+FE0F）后
        // normalized_hash 与干净文本完全一致，InvisibleStats 逐类计数正确
        let opts = NormalizeOptions::default();
        let clean = "投标报价为人民币12800000元整，包含全部软硬件费用。";
        let dirty = "投标报价\u{200B}为人民币128\u{202E}00000元整，包含全部软硬件费\u{FE0F}用。";
        let (n_dirty, stats) = normalize_with_stats(dirty, &opts);
        assert_eq!(n_dirty, normalize(clean, &opts));
        assert_eq!(
            sha256_hex(n_dirty.as_bytes()),
            sha256_hex(normalize(clean, &opts).as_bytes())
        );
        assert_eq!(stats.zero_width, 1);
        assert_eq!(stats.bidi, 1);
        assert_eq!(stats.variation, 1);
        assert_eq!(stats.tags, 0);
        assert_eq!(stats.stripped_total(), 3);
        assert!(!stats.is_clean());
    }

    #[test]
    fn all_invisible_classes_counted() {
        let s = "a\u{200B}b\u{200C}c\u{200D}d\u{FEFF}e\u{200E}f\u{200F}g\
                 \u{202A}h\u{2066}i\u{E0001}j\u{FE00}k\u{E0100}l";
        let (n, st) = normalize_with_stats(s, &NormalizeOptions::default());
        assert_eq!(n, "abcdefghijkl");
        assert_eq!(st.zero_width, 6, "200B-200D + FEFF + 200E/200F");
        assert_eq!(st.bidi, 2, "202A + 2066");
        assert_eq!(st.tags, 1, "E0001");
        assert_eq!(st.variation, 2, "FE00 + E0100");
        assert_eq!(st.stripped_total(), 11);
    }

    #[test]
    fn sanitize_intermediate_contract() {
        // 分词输入口径契约：NFKC 归一 + 剥离隐形码点 + 折叠同形字，但保留大小写/
        // 标点/中文数词原貌（cn_numbers 与大小写/标点处理属后半程 normalize_sanitized）。
        // 契约破坏会让 token_json 与模板分词的口径漂移，见 sanitize_with_stats 注释。
        let (s, st) = sanitize_with_stats("Ｐage\u{200B}系统 一百八十天（P\u{0430}ge 编号）。");
        assert_eq!(s, "Page系统 一百八十天(Page 编号)。");
        assert_eq!(st.zero_width, 1);
        assert_eq!(st.confusable_folds, 1);
        // 与终态归一的组合关系：normalize_with_stats = sanitize + normalize_sanitized
        let opts = NormalizeOptions::default();
        let (full, _) = normalize_with_stats("Ｐage\u{200B}系统 一百八十天（P\u{0430}ge 编号）。", &opts);
        assert_eq!(full, normalize_sanitized(&s, &opts));
    }

    #[test]
    fn clean_text_produces_clean_stats() {
        let (_, st) = normalize_with_stats(
            "正常标书文本 normal bid text 123，金额1,000,000元。",
            &NormalizeOptions::default(),
        );
        assert!(st.is_clean());
        assert_eq!(st, InvisibleStats::default());
    }

    #[test]
    fn normalize_throughput_sane_on_100k_chars() {
        // 验收意图：10 万字文本 normalize 增耗 <5%。改版后无旧实现可对比，
        // 以宽松绝对上限拦截数量级退化（预扫快路径失效会直接撞线），
        // 口径与 compare_service 的 60s 性能测试同风格。
        let para = "本项目采用分层解耦的微服务总体架构设计，投标报价为人民币12800000元整。";
        let text = para.repeat(100_000 / para.chars().count() + 1);
        assert!(text.chars().count() >= 100_000);
        let t0 = std::time::Instant::now();
        let (_, st) = normalize_with_stats(&text, &NormalizeOptions::default());
        let elapsed = t0.elapsed();
        assert!(st.is_clean());
        assert!(
            elapsed.as_millis() < 2000,
            "10 万字归一化应远快于 2s（debug 档宽松上限），实际 {:?}",
            elapsed
        );
        eprintln!("[perf] 10 万字 normalize_with_stats 耗时 {elapsed:?}");
    }
}
