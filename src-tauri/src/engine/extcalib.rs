//! 外部真值相似度校准评估（执行方案 §8 扩展）。
//!
//! 用【独立于合成对抗生成器】的人工标注中文相似度语料（PAWS-X / STS-B / LCQMC /
//! BQ / AFQMC / nli_zh 等）评估相似度打分器本身的判别力与标定质量，打破「合成语料
//! 生成器与检测器同源 → 指标系统性偏乐观」的循环（见 corpusgen §8 风险①）。
//!
//! 本模块只含【纯评估原语 + 外部语料读取】：不做打分、不联网、不碰生产阈值。
//! 打分接线（score_pair / embed::cosine）与门禁在后续步骤接入。仅测试/开发工具编译。

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::path::Path;

/// 外部相似度真值语料的一条记录（JSONL 一行一条）。
///
/// `label` 是【归一化到 [0,1] 的真值相似度】：二分类集用 0.0 / 1.0；分级集（如 STS-B
/// 的 0–5）除以满分归一。读取器不做归一，由数据准备脚本保证 label 落在 [0,1]。
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExternalPair {
    pub text_a: String,
    pub text_b: String,
    /// 归一化真值相似度 ∈ [0,1]。
    pub label: f32,
    /// 来源数据集标识（如 "pawsx-zh" / "stsb"），供报告分组与许可声明。
    #[serde(default)]
    pub source: String,
}

/// 把 label ≥ `pos_threshold` 视为正类（二分类度量用）。
pub fn is_positive(label: f32, pos_threshold: f32) -> bool {
    label >= pos_threshold
}

/// 读取外部真值语料（JSONL）。空行跳过；解析失败带文件:行号定位。
pub fn read_external_pairs(path: &Path) -> std::io::Result<Vec<ExternalPair>> {
    let raw = std::fs::read_to_string(path)?;
    let mut out = Vec::new();
    for (i, line) in raw.lines().enumerate() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let rec: ExternalPair = serde_json::from_str(t).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("{}:{} 解析失败: {e}", path.display(), i + 1),
            )
        })?;
        out.push(rec);
    }
    Ok(out)
}

/// 一次外部真值评估的完整报告（可序列化进 baseline_metrics_external.json）。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ExtCalibMetrics {
    /// 数据集来源标识。
    pub source: String,
    /// 打分口径："lexical"（score_pair 无语义）| "semantic"（embed cosine）| "fused"。
    pub scorer: String,
    pub pairs_count: usize,
    pub positives: usize,
    pub negatives: usize,
    /// ROC 曲线下面积（Mann-Whitney）。
    pub roc_auc: f64,
    /// P-R 曲线下面积（average precision）。
    pub pr_auc: f64,
    /// 在运行阈值处的 P/R/F1。
    pub threshold: f32,
    pub precision_at: f64,
    pub recall_at: f64,
    pub f1_at: f64,
    /// 全阈值扫描得到的最佳 F1 及其阈值。
    pub best_threshold: f32,
    pub best_f1: f64,
    /// 期望校准误差（把分数当概率，bins 等宽分箱）。
    pub ece: f64,
    /// 分数与分级真值的 Spearman 秩相关（二分类集意义有限，分级集为主指标）。
    pub spearman: f64,
}

/// Mann-Whitney U（含并列平均秩）→ ROC-AUC。复用 corpusgen 的实现以免重复。
/// 任一类为空回落 0.5。
pub fn roc_auc(scored: &[(f32, bool)]) -> f64 {
    let pos: Vec<f64> = scored.iter().filter(|(_, y)| *y).map(|(s, _)| *s as f64).collect();
    let neg: Vec<f64> = scored.iter().filter(|(_, y)| !*y).map(|(s, _)| *s as f64).collect();
    crate::engine::corpusgen::auc_score(&pos, &neg)
}

/// P-R 曲线下面积（average precision，阶梯式；并列分数合并处理避免阈值歧义）。
/// 无正类回落 0.0。
pub fn pr_auc(scored: &[(f32, bool)]) -> f64 {
    let total_pos = scored.iter().filter(|(_, y)| *y).count();
    if total_pos == 0 {
        return 0.0;
    }
    let mut v: Vec<(f32, bool)> = scored.to_vec();
    v.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal)); // 降序
    let (mut tp, mut fp) = (0usize, 0usize);
    let mut prev_recall = 0.0f64;
    let mut ap = 0.0f64;
    let n = v.len();
    let mut i = 0;
    while i < n {
        // 合并同分组：同一阈值下要么全预测正、要么全负，逐个累加避免歧义。
        let mut j = i;
        while j + 1 < n && v[j + 1].0 == v[i].0 {
            j += 1;
        }
        for item in v.iter().take(j + 1).skip(i) {
            if item.1 {
                tp += 1;
            } else {
                fp += 1;
            }
        }
        let recall = tp as f64 / total_pos as f64;
        let precision = tp as f64 / (tp + fp) as f64;
        ap += precision * (recall - prev_recall);
        prev_recall = recall;
        i = j + 1;
    }
    ap
}

/// 阈值扫描结果：给定运行阈值的 P/R/F1 + 全扫描最佳 F1 及阈值。
#[derive(Clone, Copy, Debug)]
pub struct SweepResult {
    pub precision_at: f64,
    pub recall_at: f64,
    pub f1_at: f64,
    pub best_threshold: f32,
    pub best_f1: f64,
}

/// 计算某阈值下的 (precision, recall, f1)：预测正 ⇔ score ≥ t。
fn prf_at(scored: &[(f32, bool)], t: f32) -> (f64, f64, f64) {
    let (mut tp, mut fp, mut fn_) = (0usize, 0usize, 0usize);
    for (s, y) in scored {
        match (*s >= t, *y) {
            (true, true) => tp += 1,
            (true, false) => fp += 1,
            (false, true) => fn_ += 1,
            (false, false) => {}
        }
    }
    let p = if tp + fp == 0 { 0.0 } else { tp as f64 / (tp + fp) as f64 };
    let r = if tp + fn_ == 0 { 0.0 } else { tp as f64 / (tp + fn_) as f64 };
    let f = if p + r == 0.0 { 0.0 } else { 2.0 * p * r / (p + r) };
    (p, r, f)
}

/// 扫描全部候选阈值（去重后的分数）找最佳 F1，并报告运行阈值 `at_threshold` 处的 P/R/F1。
pub fn threshold_sweep(scored: &[(f32, bool)], at_threshold: f32) -> SweepResult {
    let mut cands: Vec<f32> = scored.iter().map(|(s, _)| *s).collect();
    cands.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    cands.dedup();
    let (mut best_threshold, mut best_f1) = (at_threshold, -1.0f64);
    for &t in &cands {
        let (_, _, f) = prf_at(scored, t);
        if f > best_f1 {
            best_f1 = f;
            best_threshold = t;
        }
    }
    let (p, r, f) = prf_at(scored, at_threshold);
    SweepResult {
        precision_at: p,
        recall_at: r,
        f1_at: f,
        best_threshold,
        best_f1: best_f1.max(0.0),
    }
}

/// 期望校准误差（ECE）：把分数当预测概率，等宽分箱后加权 |准确率 − 平均置信度|。
/// 空输入或 bins=0 回落 0.0。
pub fn ece(scored: &[(f32, bool)], bins: usize) -> f64 {
    if scored.is_empty() || bins == 0 {
        return 0.0;
    }
    let n = scored.len() as f64;
    let mut cnt = vec![0usize; bins];
    let mut conf_sum = vec![0.0f64; bins];
    let mut acc_sum = vec![0.0f64; bins];
    for (s, y) in scored {
        let p = (*s).clamp(0.0, 1.0) as f64;
        let mut b = (p * bins as f64) as usize;
        if b >= bins {
            b = bins - 1; // p==1.0 落最后一箱
        }
        cnt[b] += 1;
        conf_sum[b] += p;
        acc_sum[b] += if *y { 1.0 } else { 0.0 };
    }
    let mut e = 0.0;
    for b in 0..bins {
        if cnt[b] == 0 {
            continue;
        }
        let c = cnt[b] as f64;
        e += (c / n) * (conf_sum[b] / c - acc_sum[b] / c).abs();
    }
    e
}

/// 一列数值的并列平均秩（1-based）。
fn average_ranks(vals: &[f32]) -> Vec<f64> {
    let mut idx: Vec<usize> = (0..vals.len()).collect();
    idx.sort_by(|&a, &b| vals[a].partial_cmp(&vals[b]).unwrap_or(Ordering::Equal));
    let mut ranks = vec![0.0f64; vals.len()];
    let mut i = 0;
    while i < idx.len() {
        let mut j = i;
        while j + 1 < idx.len() && vals[idx[j + 1]] == vals[idx[i]] {
            j += 1;
        }
        let avg = ((i + 1) + (j + 1)) as f64 / 2.0;
        for &k in idx.iter().take(j + 1).skip(i) {
            ranks[k] = avg;
        }
        i = j + 1;
    }
    ranks
}

/// 分数与分级真值的 Spearman 秩相关 ∈ [-1,1]。样本 < 2 或任一列常数回落 0.0。
pub fn spearman(pairs: &[(f32, f32)]) -> f64 {
    let n = pairs.len();
    if n < 2 {
        return 0.0;
    }
    let ra = average_ranks(&pairs.iter().map(|(a, _)| *a).collect::<Vec<_>>());
    let rb = average_ranks(&pairs.iter().map(|(_, b)| *b).collect::<Vec<_>>());
    let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len() as f64;
    let (ma, mb) = (mean(&ra), mean(&rb));
    let (mut cov, mut va, mut vb) = (0.0f64, 0.0f64, 0.0f64);
    for k in 0..n {
        let (da, db) = (ra[k] - ma, rb[k] - mb);
        cov += da * db;
        va += da * da;
        vb += db * db;
    }
    if va == 0.0 || vb == 0.0 {
        return 0.0;
    }
    cov / (va.sqrt() * vb.sqrt())
}

/// 从 (分数, 归一化真值) 序列汇总一份完整报告。`pos_threshold` 定义正类，
/// `op_threshold` 是要考核 P/R/F1 的运行阈值（一般对齐 similarity_threshold=0.7）。
pub fn evaluate(
    source: &str,
    scorer: &str,
    scored_labeled: &[(f32, f32)],
    pos_threshold: f32,
    op_threshold: f32,
    ece_bins: usize,
) -> ExtCalibMetrics {
    let binary: Vec<(f32, bool)> =
        scored_labeled.iter().map(|(s, l)| (*s, is_positive(*l, pos_threshold))).collect();
    let positives = binary.iter().filter(|(_, y)| *y).count();
    let sweep = threshold_sweep(&binary, op_threshold);
    ExtCalibMetrics {
        source: source.to_string(),
        scorer: scorer.to_string(),
        pairs_count: scored_labeled.len(),
        positives,
        negatives: binary.len() - positives,
        roc_auc: roc_auc(&binary),
        pr_auc: pr_auc(&binary),
        threshold: op_threshold,
        precision_at: sweep.precision_at,
        recall_at: sweep.recall_at,
        f1_at: sweep.f1_at,
        best_threshold: sweep.best_threshold,
        best_f1: sweep.best_f1,
        ece: ece(&binary, ece_bins),
        spearman: spearman(scored_labeled),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 完美可分：正类分数全高于负类 → ROC/PR = 1.0，最佳 F1 = 1.0。
    fn separable() -> Vec<(f32, bool)> {
        vec![(0.9, true), (0.8, true), (0.7, true), (0.3, false), (0.2, false), (0.1, false)]
    }

    #[test]
    fn roc_auc_perfect_and_reversed() {
        assert!((roc_auc(&separable()) - 1.0).abs() < 1e-9);
        let reversed: Vec<(f32, bool)> =
            separable().iter().map(|(s, y)| (1.0 - *s, *y)).collect();
        assert!(roc_auc(&reversed).abs() < 1e-9);
    }

    #[test]
    fn roc_auc_random_is_half() {
        // 交替、正负分数分布相同 → AUC = 0.5。
        let s = vec![(0.5, true), (0.5, false)];
        assert!((roc_auc(&s) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn pr_auc_perfect() {
        assert!((pr_auc(&separable()) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn pr_auc_no_positive_is_zero() {
        let s = vec![(0.9, false), (0.1, false)];
        assert_eq!(pr_auc(&s), 0.0);
    }

    #[test]
    fn threshold_sweep_finds_perfect_split() {
        let r = threshold_sweep(&separable(), 0.7);
        assert!((r.best_f1 - 1.0).abs() < 1e-9);
        // 运行阈值 0.7：三正类全 ≥0.7 命中、无负类越线 → P=R=F1=1。
        assert!((r.f1_at - 1.0).abs() < 1e-9);
        assert!(r.best_threshold > 0.3 && r.best_threshold <= 0.7);
    }

    #[test]
    fn threshold_sweep_operating_point_recall_loss() {
        // 一个正类分数偏低（0.4），运行阈值 0.7 会漏掉它 → recall < 1。
        let s = vec![(0.9, true), (0.4, true), (0.2, false)];
        let r = threshold_sweep(&s, 0.7);
        assert!((r.recall_at - 0.5).abs() < 1e-9);
        assert!((r.precision_at - 1.0).abs() < 1e-9);
        // 但存在阈值（≤0.4）能取回全部正类且不误报 → 最佳 F1 = 1。
        assert!((r.best_f1 - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ece_perfectly_calibrated_is_low() {
        // 每个分数即其经验准确率：0.0 全负、1.0 全正 → ECE = 0。
        let s = vec![(1.0, true), (1.0, true), (0.0, false), (0.0, false)];
        assert!(ece(&s, 10) < 1e-9);
    }

    #[test]
    fn ece_miscalibrated_is_high() {
        // 分数 0.9 但实际全错 → |0.9 - 0.0| ≈ 0.9（1e-6 容差吸收 f32→f64 精度）。
        let s = vec![(0.9, false), (0.9, false)];
        assert!((ece(&s, 10) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn spearman_monotonic_is_one() {
        let s = vec![(0.1, 0.2), (0.3, 0.4), (0.5, 0.9), (0.9, 1.0)];
        assert!((spearman(&s) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn spearman_reversed_is_minus_one() {
        let s = vec![(0.1, 1.0), (0.3, 0.7), (0.5, 0.4), (0.9, 0.1)];
        assert!((spearman(&s) + 1.0).abs() < 1e-9);
    }

    #[test]
    fn spearman_constant_column_is_zero() {
        let s = vec![(0.5, 0.1), (0.5, 0.9)];
        assert_eq!(spearman(&s), 0.0);
    }

    #[test]
    fn read_external_pairs_roundtrip() {
        let dir = std::env::temp_dir();
        let p = dir.join("bidguard_extcalib_test.jsonl");
        let body = "\
{\"text_a\":\"投标函正文甲\",\"text_b\":\"投标函正文甲\",\"label\":1.0,\"source\":\"pawsx-zh\"}\n\
\n\
{\"text_a\":\"技术方案 A\",\"text_b\":\"完全无关的段落\",\"label\":0.0,\"source\":\"pawsx-zh\"}\n";
        std::fs::write(&p, body).unwrap();
        let pairs = read_external_pairs(&p).unwrap();
        std::fs::remove_file(&p).ok();
        assert_eq!(pairs.len(), 2); // 空行被跳过
        assert_eq!(pairs[0].label, 1.0);
        assert_eq!(pairs[0].source, "pawsx-zh");
        assert_eq!(pairs[1].label, 0.0);
    }

    #[test]
    fn evaluate_end_to_end() {
        // 分级真值互异且与分数完美单调：ROC/PR/best_f1=1，Spearman=1。
        // 用互异分级标签（非 0/1），否则并列会让 Spearman 达不到 1。
        let data =
            vec![(0.95f32, 1.0f32), (0.85, 0.8), (0.72, 0.6), (0.30, 0.3), (0.10, 0.1)];
        let m = evaluate("unit", "lexical", &data, 0.5, 0.7, 10);
        assert_eq!(m.pairs_count, 5);
        assert_eq!(m.positives, 3); // label ≥ 0.5：1.0/0.8/0.6
        assert_eq!(m.negatives, 2);
        assert!((m.roc_auc - 1.0).abs() < 1e-9);
        assert!((m.pr_auc - 1.0).abs() < 1e-9);
        assert!((m.spearman - 1.0).abs() < 1e-9);
        assert!((m.f1_at - 1.0).abs() < 1e-9);
    }
}
