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
    /// 可选子类（如样板语料的 verbatim / sibling / cross_chapter），供分档报告。
    #[serde(default)]
    pub subclass: String,
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

/// 单边误报探针：全负样本语料（合法共享的样板文本）在运行阈值处的误报率与分数分布。
///
/// 为什么不套用 ROC/PR-AUC：这类语料【没有正类】——官方范本条款在多份标书中重复出现是
/// 合法的，不是串标证据。两侧判别指标在此无定义（AUC 回落 0.5、PR-AUC 恒 0），会把
/// 「无正类」误读成「判别失败」。这里要回答的是另一个问题：**多大比例的合法样板会越过
/// 阈值被标记**——即误报代价。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct FalsePositiveProbe {
    pub source: String,
    pub scorer: String,
    /// 子类（verbatim / sibling / cross_chapter；"all" 为该来源汇总）。
    pub subclass: String,
    pub pairs_count: usize,
    pub threshold: f32,
    /// 分数 ≥ 阈值的对数（会被判为可疑）。
    pub flagged: usize,
    /// 误报率 = flagged / pairs_count。
    pub fpr: f64,
    pub mean_score: f64,
    pub median_score: f64,
    pub p90_score: f64,
    pub max_score: f64,
}

fn quantile(sorted: &[f32], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)] as f64
}

/// 汇总一组（全负样本）分数在 `threshold` 处的误报率与分布。
pub fn false_positive_probe(
    source: &str,
    scorer: &str,
    subclass: &str,
    scores: &[f32],
    threshold: f32,
) -> FalsePositiveProbe {
    let n = scores.len();
    let flagged = scores.iter().filter(|s| **s >= threshold).count();
    let mut sorted: Vec<f32> = scores.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mean = if n == 0 { 0.0 } else { scores.iter().map(|s| *s as f64).sum::<f64>() / n as f64 };
    FalsePositiveProbe {
        source: source.to_string(),
        scorer: scorer.to_string(),
        subclass: subclass.to_string(),
        pairs_count: n,
        threshold,
        flagged,
        fpr: if n == 0 { 0.0 } else { flagged as f64 / n as f64 },
        mean_score: mean,
        median_score: quantile(&sorted, 0.5),
        p90_score: quantile(&sorted, 0.9),
        max_score: sorted.last().copied().unwrap_or(0.0) as f64,
    }
}

// —— 概率校准（执行方案 item 4）——
//
// 打分器 emit 的是 [0,1] 的加权相似度，不是概率：`0.8` 不代表「80% 概率是雷同」。
// 实测 ECE 佐证了这点（词面档 0.231、裸余弦 0.495）。校准就是学一个单调映射
// score → P(正类)，让分数能当置信度读，也让「标红/待复核/放行」三带阈值有据可依。
//
// 两种做法各有取舍：Platt 拟合单个 sigmoid，参数少、外推平滑，但假定 logit 与分数线性；
// isotonic 只要求单调、能贴任意形状，代价是易过拟合且在数据稀疏处呈阶梯。

/// Platt scaling 参数：P(y=1|s) = 1 / (1 + exp(a·s + b))。
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default)]
pub struct PlattParams {
    pub a: f64,
    pub b: f64,
}

impl PlattParams {
    pub fn apply(&self, score: f32) -> f32 {
        (1.0 / (1.0 + (self.a * score as f64 + self.b).exp())) as f32
    }
}

/// 用牛顿法拟合 Platt scaling（Lin/Weng/Lin 2007 的正则化目标：以 t± 替代 0/1 硬标签，
/// 避免完全可分时参数发散）。样本不足或退化时回落恒等映射的近似参数。
pub fn fit_platt(scored: &[(f32, bool)]) -> PlattParams {
    let (np, nn) = (
        scored.iter().filter(|(_, y)| *y).count() as f64,
        scored.iter().filter(|(_, y)| !*y).count() as f64,
    );
    if np == 0.0 || nn == 0.0 {
        return PlattParams { a: -1.0, b: 0.0 };
    }
    // 目标值：正类 (np+1)/(np+2)，负类 1/(nn+2)
    let (hi, lo) = ((np + 1.0) / (np + 2.0), 1.0 / (nn + 2.0));
    let t: Vec<f64> = scored.iter().map(|(_, y)| if *y { hi } else { lo }).collect();
    let s: Vec<f64> = scored.iter().map(|(x, _)| *x as f64).collect();

    let (mut a, mut b) = (-1.0f64, (nn / np).ln().max(-10.0).min(10.0));
    for _ in 0..100 {
        let (mut g1, mut g2, mut h11, mut h22, mut h12) = (0.0, 0.0, 1e-12, 1e-12, 0.0);
        for (si, ti) in s.iter().zip(&t) {
            let z = a * si + b;
            let p = 1.0 / (1.0 + z.exp());
            // dL/dz = t - p（由 dL/dp · dp/dz 推得，dp/dz = -p(1-p) 抵消了负号）。
            // 写成 p - t 会让牛顿步朝梯度上升走，拟合出反向映射。
            let d = ti - p;
            let w = p * (1.0 - p);
            g1 += si * d;
            g2 += d;
            h11 += si * si * w;
            h22 += w;
            h12 += si * w;
        }
        if g1.abs() < 1e-9 && g2.abs() < 1e-9 {
            break;
        }
        let det = h11 * h22 - h12 * h12;
        if det.abs() < 1e-18 {
            break;
        }
        a -= (h22 * g1 - h12 * g2) / det;
        b -= (h11 * g2 - h12 * g1) / det;
    }
    PlattParams { a, b }
}

/// Isotonic 回归（PAV，保序合并）产出的分段常数映射：(分数上界, 概率) 升序。
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct IsotonicMap {
    pub points: Vec<(f32, f32)>,
}

impl IsotonicMap {
    /// 查表：落在哪一段取该段概率；超出右端取最后一段（单调外推）。
    pub fn apply(&self, score: f32) -> f32 {
        if self.points.is_empty() {
            return score;
        }
        for (ub, p) in &self.points {
            if score <= *ub {
                return *p;
            }
        }
        self.points.last().map(|(_, p)| *p).unwrap_or(score)
    }
}

/// PAV（pool adjacent violators）拟合保序回归。
pub fn fit_isotonic(scored: &[(f32, bool)]) -> IsotonicMap {
    if scored.is_empty() {
        return IsotonicMap::default();
    }
    let mut v: Vec<(f32, f64)> =
        scored.iter().map(|(s, y)| (*s, if *y { 1.0 } else { 0.0 })).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    // 每块：(分数上界, 和, 计数)
    let mut blocks: Vec<(f32, f64, f64)> = Vec::with_capacity(v.len());
    for (s, y) in v {
        blocks.push((s, y, 1.0));
        // 违反单调则与前一块合并
        while blocks.len() >= 2 {
            let n = blocks.len();
            let (_, s2, c2) = blocks[n - 1];
            let (_, s1, c1) = blocks[n - 2];
            if s1 / c1 <= s2 / c2 {
                break;
            }
            let ub = blocks[n - 1].0;
            blocks.truncate(n - 2);
            blocks.push((ub, s1 + s2, c1 + c2));
        }
    }
    IsotonicMap {
        points: blocks.iter().map(|(ub, s, c)| (*ub, (s / c) as f32)).collect(),
    }
}

/// 校准前后的 ECE 对照（含训练/测试切分，避免在拟合数据上自评）。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationReport {
    pub source: String,
    pub scorer: String,
    pub train_count: usize,
    pub test_count: usize,
    /// 未校准分数在测试集上的 ECE。
    pub ece_raw: f64,
    pub ece_platt: f64,
    pub ece_isotonic: f64,
    pub platt: PlattParams,
    /// 校准是单调变换，不改排序 → ROC-AUC 不变，这里记录以便核对。
    pub roc_auc: f64,
}

/// 交替取样切分（按分数排序后隔一取一），保证训练/测试的分数分布一致——
/// 随机切分需要 RNG，而本 harness 要求确定性可复现。
pub fn calibrate_report(
    source: &str,
    scorer: &str,
    scored: &[(f32, bool)],
    ece_bins: usize,
) -> CalibrationReport {
    let mut v: Vec<(f32, bool)> = scored.to_vec();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    let train: Vec<(f32, bool)> = v.iter().step_by(2).copied().collect();
    let test: Vec<(f32, bool)> = v.iter().skip(1).step_by(2).copied().collect();

    let platt = fit_platt(&train);
    let iso = fit_isotonic(&train);
    let p_test: Vec<(f32, bool)> = test.iter().map(|(s, y)| (platt.apply(*s), *y)).collect();
    let i_test: Vec<(f32, bool)> = test.iter().map(|(s, y)| (iso.apply(*s), *y)).collect();

    CalibrationReport {
        source: source.to_string(),
        scorer: scorer.to_string(),
        train_count: train.len(),
        test_count: test.len(),
        ece_raw: ece(&test, ece_bins),
        ece_platt: ece(&p_test, ece_bins),
        ece_isotonic: ece(&i_test, ece_bins),
        platt,
        roc_auc: roc_auc(&test),
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
    fn false_positive_probe_counts_and_distribution() {
        // 5 个合法样板对，3 个越过 0.7 → FPR 0.6。
        let scores = vec![0.95f32, 0.80, 0.72, 0.40, 0.10];
        let p = false_positive_probe("tpl", "lexical", "verbatim", &scores, 0.7);
        assert_eq!(p.pairs_count, 5);
        assert_eq!(p.flagged, 3);
        assert!((p.fpr - 0.6).abs() < 1e-9);
        assert!((p.max_score - 0.95).abs() < 1e-6);
        assert!((p.median_score - 0.72).abs() < 1e-6);
        assert!(p.mean_score > 0.5 && p.mean_score < 0.7);
    }

    #[test]
    fn false_positive_probe_all_clean_is_zero() {
        let scores = vec![0.10f32, 0.20, 0.30];
        let p = false_positive_probe("tpl", "lexical", "cross_chapter", &scores, 0.7);
        assert_eq!(p.flagged, 0);
        assert_eq!(p.fpr, 0.0);
    }

    #[test]
    fn false_positive_probe_empty_is_safe() {
        let p = false_positive_probe("tpl", "lexical", "all", &[], 0.7);
        assert_eq!(p.pairs_count, 0);
        assert_eq!(p.fpr, 0.0);
        assert_eq!(p.max_score, 0.0);
    }

    #[test]
    fn platt_maps_scores_monotonically_to_probabilities() {
        // 低分多负、高分多正 → 拟合出的映射应单调递增且把两端拉向 0/1
        let mut data = Vec::new();
        for i in 0..50 {
            let s = i as f32 / 50.0;
            data.push((s, i >= 30)); // 0.6 以上为正
        }
        let p = fit_platt(&data);
        let (lo, mid, hi) = (p.apply(0.1), p.apply(0.6), p.apply(0.95));
        assert!(lo < mid && mid < hi, "应单调递增: {lo:.3} {mid:.3} {hi:.3}");
        assert!(lo < 0.3, "低分应映射到低概率，实测 {lo:.3}");
        assert!(hi > 0.7, "高分应映射到高概率，实测 {hi:.3}");
    }

    #[test]
    fn platt_handles_single_class_without_diverging() {
        let allneg = vec![(0.2f32, false), (0.5, false), (0.9, false)];
        let p = fit_platt(&allneg);
        assert!(p.a.is_finite() && p.b.is_finite(), "单类不应产生 NaN/inf");
    }

    #[test]
    fn isotonic_is_monotone_and_fits_steps() {
        let data = vec![
            (0.1f32, false), (0.2, false), (0.3, false),
            (0.7, true), (0.8, true), (0.9, true),
        ];
        let iso = fit_isotonic(&data);
        let (a, b) = (iso.apply(0.15), iso.apply(0.85));
        assert!(a < b, "应单调: {a:.3} < {b:.3}");
        assert!(a < 0.2 && b > 0.8, "完全可分时两端应贴近 0/1: {a:.3} {b:.3}");
    }

    #[test]
    fn isotonic_pools_violators() {
        // 分数升序但标签下降 → PAV 必须合并成同一概率（否则不单调）
        let data = vec![(0.4f32, true), (0.5, false), (0.6, true), (0.7, false)];
        let iso = fit_isotonic(&data);
        let ps: Vec<f32> = [0.4f32, 0.5, 0.6, 0.7].iter().map(|s| iso.apply(*s)).collect();
        for w in ps.windows(2) {
            assert!(w[1] >= w[0] - 1e-6, "输出必须非降: {ps:?}");
        }
    }

    #[test]
    fn calibration_reduces_ece_on_held_out_split() {
        // 两组都系统性偏离：0.9 组实际只有 1/3 为正（高估），0.1 组也有 1/3（低估）。
        // 标签周期取 3 而非 2：交替切分的周期是 2，若标签也按奇偶分布会导致训练/测试
        // 标签完全反相（训练全正、测试全负），那是构造出来的假象而非校准失效。
        let mut data = Vec::new();
        for i in 0..120 {
            data.push((0.9f32, i % 3 == 0));
        }
        for i in 0..120 {
            data.push((0.1f32, i % 3 == 0));
        }
        let r = calibrate_report("unit", "lexical", &data, 10);
        assert!(r.train_count > 0 && r.test_count > 0);
        assert!(r.ece_raw > 0.15, "构造数据原始 ECE 应偏高，实测 {:.3}", r.ece_raw);
        assert!(
            r.ece_platt < r.ece_raw && r.ece_isotonic < r.ece_raw,
            "校准应降低留出集 ECE：raw {:.3} platt {:.3} iso {:.3}",
            r.ece_raw, r.ece_platt, r.ece_isotonic
        );
    }

    #[test]
    fn calibration_preserves_ranking_when_signal_exists() {
        // 校准是分数的单调变换，不改排序 → AUC 不变。这正是校准【不能】提升判别力的原因：
        // 它只重标刻度，不重排样本。
        // 注意前提：Platt 拟合到正相关（a<0）时才是单调【递增】。若数据本身无信号，
        // 拟出的斜率是噪声，可能得到单调递减映射而把 AUC 翻成 1-AUC——那不是实现问题，
        // 而是「对随机数据谈保序方向」本就没有意义。故这里用带信号的数据。
        let data: Vec<(f32, bool)> =
            (0..60).map(|i| (i as f32 / 60.0, (i % 10) < (i / 10))).collect();
        let before = roc_auc(&data);
        assert!(before > 0.6, "前置条件：数据须带正相关信号，实测 AUC {before:.3}");
        let p = fit_platt(&data);
        assert!(p.a < 0.0, "正相关数据应拟合出单调递增映射（a<0），实测 a={:.3}", p.a);
        let after: Vec<(f32, bool)> = data.iter().map(|(s, y)| (p.apply(*s), *y)).collect();
        assert!((roc_auc(&after) - before).abs() < 1e-9, "Platt 不应改变 AUC");
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
