// 跨脚本同形字防线（执行方案 §4 W2-2）：NFKC 折叠不了跨脚本同形字（西里尔 а/о/р、
// 希腊 ο 等），攻击者用它们替换雷同段落中的拉丁字母/数字旁字符即可击穿全部词面通道。
// 两条独立防线：
// 1) fold()：简化版 UTS#39 skeleton——静态高置信映射表把同形字折回拉丁骨架，恢复
//    normalized_hash / MinHash / embedding 缓存键的匹配能力，命中计数入 InvisibleStats；
// 2) scan_mixed_script()：「同一词内拉丁+西里尔混排」在正常中文标书中不存在，是零训练
//    成本的高置信红旗；与 fold 解耦（fold 会把西里尔改写成拉丁，必须先扫后折）。
// 静态表内置源码（拒绝 ICU 级依赖：体积与离线约束）；UTS#39 全表 6000+ 条不追求覆盖，
// 只收视觉高置信映射——冷门脚本同形会漏，接受：命中即证据、漏检不恶化现状。
use crate::engine::normalize::InvisibleStats;

/// 混合脚本采样词上限（单次扫描，即单个分块）：证据下钻要的是样本不是全量，
/// 上限防住「整篇替换」时 extra_json 膨胀。
pub const SAMPLE_MAX: usize = 3;

/// 同形字映射：(同形字, 拉丁骨架)，按码点升序排列（二分查找的前提，有单测把关）。
/// 收录标准：与拉丁字母/数字视觉上难以区分（西里尔同形全套 + 希腊高置信子集）。
/// 明确不收：常用希腊技术符号（μ/Ω/α/β/γ/Δ/π/σ 等，中文技术标书合法出现，收录会把
/// 干净文档计成折叠命中）、与拉丁差异可辨的字母（б 除外——б/6 高置信）、带附加符号
/// 而视觉可辨的变体。折叠在小写化之前执行，映射保持原大小写。
/// 注：г→g 为俄文斜体形近（执行方案验收用例 'Pагe'→'Page' 拍板收录）。
static CONFUSABLES: &[(char, char)] = &[
    // —— 希腊 ——
    ('\u{0391}', 'A'), // Α
    ('\u{0392}', 'B'), // Β
    ('\u{0395}', 'E'), // Ε
    ('\u{0396}', 'Z'), // Ζ
    ('\u{0397}', 'H'), // Η
    ('\u{0399}', 'I'), // Ι
    ('\u{039A}', 'K'), // Κ
    ('\u{039C}', 'M'), // Μ
    ('\u{039D}', 'N'), // Ν
    ('\u{039F}', 'O'), // Ο
    ('\u{03A1}', 'P'), // Ρ
    ('\u{03A4}', 'T'), // Τ
    ('\u{03A5}', 'Y'), // Υ
    ('\u{03A7}', 'X'), // Χ
    ('\u{03B9}', 'i'), // ι
    ('\u{03BD}', 'v'), // ν
    ('\u{03BF}', 'o'), // ο
    ('\u{03C1}', 'p'), // ρ
    ('\u{03C5}', 'u'), // υ
    ('\u{03F2}', 'c'), // ϲ 弯月西格玛
    ('\u{03F9}', 'C'), // Ϲ
    // —— 西里尔 ——
    ('\u{0405}', 'S'), // Ѕ
    ('\u{0406}', 'I'), // І
    ('\u{0407}', 'I'), // Ї
    ('\u{0408}', 'J'), // Ј
    ('\u{0410}', 'A'), // А
    ('\u{0412}', 'B'), // В
    ('\u{0415}', 'E'), // Е
    ('\u{0417}', '3'), // З（数字 3——报价金额的数字替换面）
    ('\u{041A}', 'K'), // К
    ('\u{041C}', 'M'), // М
    ('\u{041D}', 'H'), // Н
    ('\u{041E}', 'O'), // О
    ('\u{0420}', 'P'), // Р
    ('\u{0421}', 'C'), // С
    ('\u{0422}', 'T'), // Т
    ('\u{0423}', 'Y'), // У
    ('\u{0425}', 'X'), // Х
    ('\u{042C}', 'b'), // Ь
    ('\u{0430}', 'a'), // а
    ('\u{0431}', '6'), // б（数字 6）
    ('\u{0433}', 'g'), // г
    ('\u{0435}', 'e'), // е
    ('\u{0437}', '3'), // з（数字 3）
    ('\u{043A}', 'k'), // к
    ('\u{043C}', 'm'), // м
    ('\u{043E}', 'o'), // о
    ('\u{043F}', 'n'), // п
    ('\u{0440}', 'p'), // р
    ('\u{0441}', 'c'), // с
    ('\u{0442}', 't'), // т
    ('\u{0443}', 'y'), // у
    ('\u{0445}', 'x'), // х
    ('\u{044C}', 'b'), // ь
    ('\u{0455}', 's'), // ѕ
    ('\u{0456}', 'i'), // і
    ('\u{0457}', 'i'), // ї
    ('\u{0458}', 'j'), // ј
    ('\u{0461}', 'w'), // ѡ
    ('\u{0474}', 'V'), // Ѵ
    ('\u{0475}', 'v'), // ѵ
    ('\u{04AE}', 'Y'), // Ү
    ('\u{04AF}', 'y'), // ү
    ('\u{04BA}', 'H'), // Һ
    ('\u{04BB}', 'h'), // һ
    ('\u{04C0}', 'I'), // Ӏ（palochka）
    ('\u{04CF}', 'l'), // ӏ
    ('\u{0500}', 'D'), // Ԁ
    ('\u{0501}', 'd'), // ԁ
    ('\u{051A}', 'Q'), // Ԛ
    ('\u{051B}', 'q'), // ԛ
    ('\u{051C}', 'W'), // Ԝ
    ('\u{051D}', 'w'), // ԝ
];

/// 同形字查表（二分，表已按码点升序）。
fn lookup(c: char) -> Option<char> {
    CONFUSABLES
        .binary_search_by_key(&c, |&(k, _)| k)
        .ok()
        .map(|i| CONFUSABLES[i].1)
}

/// 西里尔字符（主块 + 补充/扩展块，用于脚本判定与 normalize 的预扫快路径）。
pub fn is_cyrillic(c: char) -> bool {
    matches!(c as u32, 0x0400..=0x052F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F)
}

/// 希腊字符（主块 + 扩展块）。
pub fn is_greek(c: char) -> bool {
    matches!(c as u32, 0x0370..=0x03FF | 0x1F00..=0x1FFF)
}

/// 拉丁字母（ASCII + Latin-1/扩展）。数字单独判定，不在此。
fn is_latin(c: char) -> bool {
    c.is_ascii_alphabetic() || (matches!(c as u32, 0x00C0..=0x024F) && c.is_alphabetic())
}

/// 汉字（基本区 + 扩展 A + 兼容区）。
fn is_han(c: char) -> bool {
    matches!(c as u32, 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF)
}

/// 同形字折叠：命中静态表的字符替换为拉丁骨架并计数。
/// 在隐形码点剥离之后、小写化之前调用（见 normalize_with_stats 的顺序约束）。
pub fn fold(s: &str, stats: &mut InvisibleStats) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match lookup(c) {
            Some(latin) => {
                stats.confusable_folds += 1;
                out.push(latin);
            }
            None => out.push(c),
        }
    }
    out
}

/// 混合脚本红旗扫描：把文本切成连续字母数字 run，对每个 run 判定：
/// - 同一 run 内拉丁+西里尔共存（'Дeposit'、'UЅB接口'）→ 红旗；
/// - 单个西里尔/同形表内希腊字符嵌在纯拉丁/数字序列中（'35о12'）→ 红旗。
///
/// 明确不触发（控误报，均有单测）：Han+Latin 混排（'AI平台'/'5G基站'——中文标书的正常
/// 形态）、希腊技术符号（'10μm'/'10Ω'——μ/Ω 不在同形表内）、整词西里尔/希腊（俄文资质
/// 证书、希腊术语——run 内无拉丁/数字共存）。含汉字的 run 不走「单字符嵌入」规则：
/// 中文语境的单个外文字符多为合法符号，拉丁+西里尔共存规则仍兜底真实替换攻击。
pub fn scan_mixed_script(text: &str, stats: &mut InvisibleStats) {
    let mut run: Vec<char> = Vec::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            run.push(c);
        } else {
            flush_run(&run, stats);
            run.clear();
        }
    }
    flush_run(&run, stats);
}

fn flush_run(run: &[char], stats: &mut InvisibleStats) {
    if run.len() < 2 || !run_is_flagged(run) {
        return;
    }
    stats.mixed_script_words += 1;
    if stats.mixed_script_samples.len() < SAMPLE_MAX {
        // 采样截断到 20 字符：整行被拼成一个 run 时样本仍可读
        let sample: String = run.iter().take(20).collect();
        if !stats.mixed_script_samples.contains(&sample) {
            stats.mixed_script_samples.push(sample);
        }
    }
}

fn run_is_flagged(run: &[char]) -> bool {
    let (mut latin, mut cyr, mut greek_conf, mut greek_other, mut han, mut digit) =
        (0u32, 0u32, 0u32, 0u32, 0u32, 0u32);
    for &c in run {
        if c.is_ascii_digit() {
            digit += 1;
        } else if is_latin(c) {
            latin += 1;
        } else if is_cyrillic(c) {
            cyr += 1;
        } else if is_greek(c) {
            if lookup(c).is_some() {
                greek_conf += 1;
            } else {
                greek_other += 1;
            }
        } else if is_han(c) {
            han += 1;
        }
        // 其他脚本（假名/谚文等）不参与判定
    }
    // 规则 a：同 run 拉丁+西里尔共存（无论是否含汉字——'UЅB接口' 也要抓）
    if latin > 0 && cyr > 0 {
        return true;
    }
    // 规则 b：单个西里尔/同形希腊字符嵌在纯拉丁/数字序列中。
    // 要求无汉字且无非同形希腊（后者说明是技术符号语境）。
    han == 0 && greek_other == 0 && cyr + greek_conf == 1 && latin + digit >= 2
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::normalize::{self, NormalizeOptions};

    #[test]
    fn table_is_sorted_and_unique() {
        // 二分查找的前提；新增映射条目时此测试把关
        for w in CONFUSABLES.windows(2) {
            assert!(w[0].0 < w[1].0, "表须按码点严格升序：{:?} >= {:?}", w[0].0, w[1].0);
        }
        // 抽查映射可命中
        assert_eq!(lookup('а'), Some('a'));
        assert_eq!(lookup('О'), Some('O'));
        assert_eq!(lookup('a'), None, "拉丁字符不在表内");
        assert_eq!(lookup('中'), None);
    }

    #[test]
    fn folded_confusables_hash_like_latin() {
        // 验收用例：'Pагe'（拉丁 P + 西里尔 аге）fold 后与 'Page' 的 normalized_hash 相等。
        // 西里尔字符用转义写死：肉眼同形正是被测特性，字面量里混入拉丁字符测试就失真了
        let opts = NormalizeOptions::default();
        let page_cyr = "P\u{0430}\u{0433}\u{0435}"; // Pагe
        let (folded, stats) = normalize::normalize_with_stats(page_cyr, &opts);
        assert_eq!(folded, normalize::normalize("Page", &opts));
        assert_eq!(
            normalize::sha256_hex(folded.as_bytes()),
            normalize::sha256_hex(normalize::normalize("Page", &opts).as_bytes())
        );
        assert_eq!(stats.confusable_folds, 3);

        // 希腊同形与数字替换面：Ο→O、З→3
        let (a, _) = normalize::normalize_with_stats("ΟΚ 预算З0000元", &opts);
        assert_eq!(a, normalize::normalize("OK 预算30000元", &opts));
    }

    #[test]
    fn mixed_script_flags_intra_word_mixing_only() {
        let scan = |s: &str| {
            let mut st = crate::engine::normalize::InvisibleStats::default();
            scan_mixed_script(s, &mut st);
            st
        };
        // 词内拉丁+西里尔混排 → 红旗 + 采样词
        let st = scan("Дeposit 金额");
        assert_eq!(st.mixed_script_words, 1);
        assert_eq!(st.mixed_script_samples, vec!["Дeposit".to_string()]);
        // 单个西里尔字符嵌在数字序列中（金额数字替换）→ 红旗
        assert_eq!(scan("35о12").mixed_script_words, 1);
        // 中文语境的合法混排/技术符号明确不触发（验收用例）
        for s in ["AI平台", "5G基站", "10μm", "阻值10Ω", "ΔT≤5K"] {
            assert_eq!(scan(s).mixed_script_words, 0, "{s} 不应触发红旗");
        }
        // 整词西里尔（俄文资质证书片段）/整词希腊不触发——判定单位是「同词内混排」
        assert_eq!(scan("Москва 出具的证书").mixed_script_words, 0);
        assert_eq!(scan("διάμετρος").mixed_script_words, 0);
    }

    #[test]
    fn mixed_script_survives_zero_width_splitting() {
        // 零宽拆词反侦察：剥离在扫描之前，红旗仍在（normalize_with_stats 顺序约束）
        let opts = NormalizeOptions::default();
        let (_, stats) = normalize::normalize_with_stats("Дe\u{200B}posit", &opts);
        assert_eq!(stats.mixed_script_words, 1);
        assert_eq!(stats.zero_width, 1);
    }

    #[test]
    fn sample_cap_and_dedup() {
        let mut st = crate::engine::normalize::InvisibleStats::default();
        // 4 个不同混排词 + 1 个重复 → 计数 5、样本去重且不超上限
        scan_mixed_script("Дeposit P\u{0430}\u{0433}\u{0435} Ѕcan Тest Дeposit", &mut st);
        assert_eq!(st.mixed_script_words, 5);
        assert_eq!(st.mixed_script_samples.len(), SAMPLE_MAX);
    }
}
