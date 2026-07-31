// 招标文件对减（W3-2）：字符级 k-gram winnowing 指纹。
// 对投标块的 normalized_text 提指纹，与招标文件段落级分块的指纹集碰撞，
// 得每块「引用招标文件」的字符覆盖率——覆盖率高的块是对招标条款的合法逐字应答，
// 从残差比对中剔除（标记不删除，仍落 chunk_exemptions 供人工复核）。
//
// winnowing（Schleimer et al. 2003）形式保证：窗口 W 内取最小哈希（并列取最右保确定性），
// 任何 ≥ K+W-1 字的共享片段在两侧至少留一个共同指纹。哈希复用 features::hash64（XxHash64）。
use crate::engine::features;
use std::collections::HashSet;

/// 字符级 k-gram 长度。与 W 一起决定形式保证：任何 ≥ K+W-1=24 字的共享片段必有共同指纹。
pub const K: usize = 15;
/// winnowing 窗口宽度（窗内取最小哈希）。指纹密度 ≤ 2/(W+1)。
pub const W: usize = 10;
/// 覆盖率豁免线：命中招标指纹覆盖 ≥ 此比例的块判「引用招标文件」，从残差比对剔除。
pub const COVERAGE_EXEMPT: f32 = 0.8;

/// 对文本做字符级 k-gram winnowing，返回选中的 (指纹哈希, k-gram 起始字符下标)。
/// 窗内取最小哈希、并列取最右（确定性）；标准去重：仅在选中位置变化时记录一次。
/// 文本不足 K 字返回空；K ≤ len < K+W-1 的短文本退化为「全局最小」单指纹
/// （形式保证只覆盖 ≥ K+W-1，短块产一枚以免全空、仍可精确匹配等长短引用）。
pub fn fingerprints(text: &str) -> Vec<(u64, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    if n < K {
        return Vec::new();
    }
    // k-gram 哈希序列：h[i] = hash(chars[i..i+K])，i ∈ 0..=n-K
    let n_grams = n - K + 1;
    let mut h = Vec::with_capacity(n_grams);
    let mut buf = String::with_capacity(K * 4);
    for i in 0..n_grams {
        buf.clear();
        buf.extend(&chars[i..i + K]);
        h.push(features::hash64(&buf));
    }

    // 序列短于一个完整窗口：取全局最小（并列取最右），产一枚指纹。
    if n_grams < W {
        let mut min_pos = 0usize;
        for i in 1..n_grams {
            if h[i] <= h[min_pos] {
                min_pos = i;
            }
        }
        return vec![(h[min_pos], min_pos)];
    }

    // 逐窗取窗内最小（并列取最右）；位置变化才记录（标准 winnowing 去重）。
    let mut out: Vec<(u64, usize)> = Vec::new();
    let mut prev_selected: Option<usize> = None;
    for s in 0..=(n_grams - W) {
        let mut min_pos = s;
        for i in (s + 1)..(s + W) {
            if h[i] <= h[min_pos] {
                min_pos = i;
            }
        }
        if prev_selected != Some(min_pos) {
            out.push((h[min_pos], min_pos));
            prev_selected = Some(min_pos);
        }
    }
    out
}

/// 招标文件指纹索引：招标/补遗全部段落级分块 winnowing 指纹的并集（HashSet）。
pub struct TenderIndex {
    hashes: HashSet<u64>,
}

impl TenderIndex {
    /// 从招标文件段落 normalized_text 迭代器构建。
    pub fn build<'a>(texts: impl Iterator<Item = &'a str>) -> Self {
        let mut hashes = HashSet::new();
        for t in texts {
            for (fp, _) in fingerprints(t) {
                hashes.insert(fp);
            }
        }
        TenderIndex { hashes }
    }

    pub fn is_empty(&self) -> bool {
        self.hashes.is_empty()
    }

    pub fn contains(&self, fp: u64) -> bool {
        self.hashes.contains(&fp)
    }
}

/// 计算文本被招标指纹覆盖的字符比例 ∈ [0,1] 及合并后的覆盖区间（字符下标）。
/// 命中指纹在位置 p 覆盖 [p, p+K)；相邻/间隔 ≤ K 的区间合并（跨小段私有插入桥接，随设计）。
pub fn coverage(text: &str, index: &TenderIndex) -> (f32, Vec<(usize, usize)>) {
    let total = text.chars().count();
    if total == 0 || index.is_empty() {
        return (0.0, Vec::new());
    }
    let mut spans: Vec<(usize, usize)> = fingerprints(text)
        .into_iter()
        .filter(|(fp, _)| index.contains(*fp))
        .map(|(_, p)| (p, (p + K).min(total)))
        .collect();
    if spans.is_empty() {
        return (0.0, Vec::new());
    }
    spans.sort_unstable();
    let mut merged: Vec<(usize, usize)> = Vec::new();
    for (s, e) in spans {
        if let Some(last) = merged.last_mut() {
            if s <= last.1 + K {
                last.1 = last.1.max(e);
                continue;
            }
        }
        merged.push((s, e));
    }
    let covered: usize = merged.iter().map(|(s, e)| e - s).sum();
    ((covered as f32 / total as f32).min(1.0), merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// 一段足够长的共享中文文本（远超 K+W-1=24 字）。
    const SHARED: &str = "投标人须严格按照本章技术规范逐项应答，所有设备必须为原厂全新正品并提供完整的出厂合格证明与检验报告，不得有任何负偏离否则按无效投标处理。";

    fn fp_set(text: &str) -> HashSet<u64> {
        fingerprints(text).into_iter().map(|(h, _)| h).collect()
    }

    #[test]
    fn deterministic_same_input_same_fingerprints() {
        // 同输入两次运行指纹逐项相等（哈希与算法均确定性）。
        let a = fingerprints(SHARED);
        let b = fingerprints(SHARED);
        assert_eq!(a, b, "同输入两次指纹应逐项相等");
        assert!(!a.is_empty());
    }

    #[test]
    fn shared_substring_over_24_chars_has_common_fingerprint() {
        // 形式保证：任意 ≥ K+W-1=24 字共享子串，两文本必有共同指纹。
        let doc_a = format!("甲方项目背景与总体要求概述如下。{SHARED}以上为甲方补充。");
        let doc_b = format!("乙方在此引用招标原文：{SHARED}乙方据此逐条响应完毕。");
        let common: Vec<u64> = fp_set(&doc_a).intersection(&fp_set(&doc_b)).copied().collect();
        assert!(!common.is_empty(), "≥24 字共享子串应至少留一个共同指纹");
    }

    #[test]
    fn exactly_24_char_shared_substring_detected() {
        // 恰好 K+W-1=24 字（一个完整窗口）也必命中。
        let shared24: String =
            "招标文件技术规范条款第一项应答内容完整无误如上所述且无任何负偏离".chars().take(24).collect();
        assert_eq!(shared24.chars().count(), 24);
        let a = format!("前缀甲{shared24}后缀甲内容");
        let b = format!("另起乙段{shared24}乙方尾部补充");
        let common: Vec<u64> = fp_set(&a).intersection(&fp_set(&b)).copied().collect();
        assert!(!common.is_empty(), "恰好 24 字共享子串也应有共同指纹");
    }

    #[test]
    fn density_bound_respected() {
        // 密度 ≤ 2/(W+1)：指纹数 ≤ 2/(W+1) × (n-K+1)，留富余上界避免脆断言。
        let fps = fingerprints(SHARED);
        let n = SHARED.chars().count();
        let n_grams = n - K + 1;
        let bound = (2.0 / (W as f32 + 1.0) * n_grams as f32).ceil() as usize + 1;
        assert!(fps.len() <= bound, "指纹数 {} 应 ≤ 密度上界 {bound}", fps.len());
    }

    #[test]
    fn coverage_full_quote_is_high_and_disjoint_is_zero() {
        let index = TenderIndex::build(std::iter::once(SHARED));
        assert!(!index.is_empty());
        // 完整逐字引用 → 覆盖率 ≥ 0.8（豁免线）
        let (cov_full, spans) = coverage(SHARED, &index);
        assert!(cov_full >= COVERAGE_EXEMPT, "完整引用覆盖率应 ≥ {COVERAGE_EXEMPT}，实际 {cov_full}");
        assert!(!spans.is_empty());
        // 完全无关文本 → 覆盖率 0
        let unrelated = "本公司自主研发的智能运维平台采用容器化部署与全链路可观测体系，与招标条款措辞完全不同。";
        let (cov_none, _) = coverage(unrelated, &index);
        assert_eq!(cov_none, 0.0, "无关文本覆盖率应为 0，实际 {cov_none}");
    }

    #[test]
    fn coverage_partial_quote_below_threshold() {
        // 半引用半私有：覆盖率应明显低于完整引用，且低于豁免线。
        let index = TenderIndex::build(std::iter::once(SHARED));
        let private = "本公司拥有一支经验丰富的实施团队并建立了完善的质量保证与应急响应机制确保项目按期高质量交付。";
        let head: String = SHARED.chars().take(SHARED.chars().count() / 3).collect();
        let mixed = format!("{head}{private}");
        let (cov, _) = coverage(&mixed, &index);
        assert!(cov < COVERAGE_EXEMPT, "半引用覆盖率应低于豁免线，实际 {cov}");
    }

    #[test]
    fn short_and_empty_text_safe() {
        let index = TenderIndex::build(std::iter::once(SHARED));
        assert_eq!(coverage("", &index), (0.0, Vec::new()));
        // 短于 K 的文本无指纹
        assert!(fingerprints("短文本").is_empty());
        // 空索引：覆盖率恒 0
        let empty = TenderIndex::build(std::iter::empty::<&str>());
        assert!(empty.is_empty());
        assert_eq!(coverage(SHARED, &empty), (0.0, Vec::new()));
    }
}
