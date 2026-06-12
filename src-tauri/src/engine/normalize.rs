// 文本标准化（设计文档 §8.3）：NFKC、全半角、空白、标点、中文数字归一。
// 目标是让「每月十日前支付」与「每月 10 日前 支付」归一到同一形态，
// 降低无意义差异对相似度与 hash 命中的干扰。
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

/// 归一化文本：NFKC（全角→半角）→ 中文数字+单位转阿拉伯 → 大小写/标点/空白处理。
pub fn normalize(text: &str, opts: &NormalizeOptions) -> String {
    let s: String = text.nfkc().collect();
    let s = normalize_cn_numbers(&s);
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_whitespace() {
            if !opts.ignore_whitespace {
                out.push(' ');
            }
            continue;
        }
        if is_punct(c) {
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

const CN_DIGITS: &str = "零一二三四五六七八九十百千万亿两";

/// 单位词表：只有「中文数字串 + 单位」才转换，避免误伤「一致」「统一」这类普通词。
/// 按长度降序做最长匹配。
const UNITS: &[&str] = &[
    "日历日", "工作日", "个月", "万元", "小时", "分钟", "日", "天", "月", "年", "元",
    "个", "周", "次", "期", "项", "条", "款", "名", "家", "%", "％",
];

/// 逐位写法的数字字符（年份「二〇二六」按位拼接，不走 cn_to_num 进位逻辑）。
fn plain_digit(c: char) -> Option<char> {
    Some(match c {
        '零' | '〇' => '0',
        '一' => '1',
        '二' => '2',
        '三' => '3',
        '四' => '4',
        '五' => '5',
        '六' => '6',
        '七' => '7',
        '八' => '8',
        '九' => '9',
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
                if !unit_is_ge_scale {
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
            '一' => 1,
            '二' | '两' => 2,
            '三' => 3,
            '四' => 4,
            '五' => 5,
            '六' => 6,
            '七' => 7,
            '八' => 8,
            '九' => 9,
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
            '十' => {
                // 「十」开头表示 1 十
                section += if cur == 0 { 10 } else { cur * 10 };
                cur = 0;
                shorthand = 0;
                any = true;
            }
            '百' => {
                section += cur.checked_mul(100)?;
                cur = 0;
                shorthand = 10;
            }
            '千' => {
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
}
