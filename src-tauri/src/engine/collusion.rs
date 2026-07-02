// 围标综合判定：把文本相似度、跨文档雷同条款、元数据同源、共有特征词、
// 报价梯度（金额接近但条款雷同）加权成一个结论。
use crate::engine::report::{Cluster, Collusion, CollusionSignal, DocInfo, SharedTerm};

/// 报价梯度信号：两文档报价差距很小（围标常见的「陪标价」），且多处条款雷同。
pub struct PriceProximity {
    pub a: usize,
    pub b: usize,
    pub amount_a: u64,
    pub amount_b: u64,
    pub gap_pct: f32,
}

// —— 围标判定权重与分级线（集中于此，便于校准） ——
// ⚠️ 未经实证校准：以下均为基于评标经验的初始默认值，尚无带标注的真实案例语料回测。
// 校准方法见 docs/architecture-analysis-v0.4.md(S1-1)：收集有标注(真围标/非围标)的历史比对
// 语料，用正负向样本回测这些权重与三条分级线后再固化。改这里即可整体调参。
const SIM_FLOOR: f32 = 0.6; // 相似度峰值起算线
const SIM_WEIGHT: f32 = 0.4; // 相似度信号满权重
const CLUSTER_MULTI_DOCS: usize = 3; // ≥N 份共现算强雷同
const CLUSTER_BASE: f32 = 0.1; // 有雷同条款的基础权重
const CLUSTER_SCALE: f32 = 0.3; // 强雷同随数量增长的权重
const CLUSTER_SCALE_CAP: f32 = 5.0; // multi/CAP 封顶到 1
const META_MIN_DOCS: usize = 2; // ≥N 份元数据同源才计
const META_WEIGHT: f32 = 0.25;
const SHARED_TERMS_MIN: usize = 5; // ≥N 个共有特征词才计
const SHARED_TERMS_WEIGHT: f32 = 0.1;
const PRICE_WEIGHT: f32 = 0.15;
const PRICE_SHOW_MAX: usize = 3; // 报价梯度对最多列出几对
const LEVEL_HIGH: f32 = 0.6;
const LEVEL_MEDIUM: f32 = 0.35;
const LEVEL_LOW: f32 = 0.1; // score > LEVEL_LOW → low

pub fn assess_with(
    peak: f32,
    clusters: &[Cluster],
    docs: &[DocInfo],
    shared_terms: &[SharedTerm],
    price_pairs: &[PriceProximity],
) -> Collusion {
    let mut signals = Vec::new();
    let mut score = 0.0f32;

    // 1) 文本相似度峰值（SIM_FLOOR→0，1.0→满分 SIM_WEIGHT）
    if peak >= SIM_FLOOR {
        let w = SIM_WEIGHT * ((peak - SIM_FLOOR) / (1.0 - SIM_FLOOR)).clamp(0.0, 1.0);
        score += w;
        signals.push(CollusionSignal {
            kind: "similarity".into(),
            detail: format!("两份标书整体相似度峰值 {:.0}%", peak * 100.0),
            weight: w,
        });
    }

    // 2) 跨文档雷同条款（3 份及以上的聚类是强信号）
    let multi = clusters.iter().filter(|c| c.docs.len() >= CLUSTER_MULTI_DOCS).count();
    if multi > 0 {
        let w = CLUSTER_BASE + CLUSTER_SCALE * (multi as f32 / CLUSTER_SCALE_CAP).clamp(0.0, 1.0);
        score += w;
        signals.push(CollusionSignal {
            kind: "cluster".into(),
            detail: format!("{multi} 处条款在 {CLUSTER_MULTI_DOCS} 份及以上标书间高度雷同"),
            weight: w,
        });
    } else if !clusters.is_empty() {
        score += CLUSTER_BASE;
        signals.push(CollusionSignal {
            kind: "cluster".into(),
            detail: format!("{} 处跨标书雷同条款", clusters.len()),
            weight: CLUSTER_BASE,
        });
    }

    // 3) 元数据同源（作者 / 最后修改人 / 制作软件一致）
    let meta = docs
        .iter()
        .filter(|d| !d.fingerprint.risk_flags.is_empty())
        .count();
    if meta >= META_MIN_DOCS {
        score += META_WEIGHT;
        signals.push(CollusionSignal {
            kind: "metadata".into(),
            detail: "多份文档元数据同源（作者 / 修改人 / 制作软件一致）".into(),
            weight: META_WEIGHT,
        });
    }

    // 4) 共有特征词 / 疑似共用笔误
    if shared_terms.len() >= SHARED_TERMS_MIN {
        score += SHARED_TERMS_WEIGHT;
        signals.push(CollusionSignal {
            kind: "sharedTerms".into(),
            detail: format!("{} 个罕见特征词被多份标书共用", shared_terms.len()),
            weight: SHARED_TERMS_WEIGHT,
        });
    }

    // 5) 报价梯度雷同：金额仅差几个百分点 + 多处条款雷同，是典型的围标陪标特征。
    // 多对接近时全部列出（最多 3 对），权重记一次（同一类证据不叠加）。
    if !price_pairs.is_empty() {
        const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
        let tag = |i: usize| STEMS.get(i).copied().unwrap_or("?");
        let shown: Vec<String> = price_pairs
            .iter()
            .take(PRICE_SHOW_MAX)
            .map(|p| {
                format!(
                    "「{}」「{}」差 {:.1}%（{} vs {} 元）",
                    tag(p.a),
                    tag(p.b),
                    p.gap_pct * 100.0,
                    p.amount_a,
                    p.amount_b
                )
            })
            .collect();
        let w = PRICE_WEIGHT;
        score += w;
        signals.push(CollusionSignal {
            kind: "facts".into(),
            detail: format!("报价梯度雷同：{}，且相关文档多处条款雷同", shown.join("；")),
            weight: w,
        });
    }

    let score = score.clamp(0.0, 1.0);
    let level = if score >= LEVEL_HIGH {
        "high"
    } else if score >= LEVEL_MEDIUM {
        "medium"
    } else if score > LEVEL_LOW {
        "low"
    } else {
        "none"
    };
    Collusion {
        level: level.into(),
        score,
        signals,
    }
}
