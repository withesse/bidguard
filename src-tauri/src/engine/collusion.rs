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

pub fn assess_with(
    peak: f32,
    clusters: &[Cluster],
    docs: &[DocInfo],
    shared_terms: &[SharedTerm],
    price_pairs: &[PriceProximity],
) -> Collusion {
    let mut signals = Vec::new();
    let mut score = 0.0f32;

    // 1) 文本相似度峰值（0.6→0，1.0→满分 0.4）
    if peak >= 0.6 {
        let w = 0.4 * ((peak - 0.6) / 0.4).clamp(0.0, 1.0);
        score += w;
        signals.push(CollusionSignal {
            kind: "similarity".into(),
            detail: format!("两份标书整体相似度峰值 {:.0}%", peak * 100.0),
            weight: w,
        });
    }

    // 2) 跨文档雷同条款（3 份及以上的聚类是强信号）
    let multi = clusters.iter().filter(|c| c.docs.len() >= 3).count();
    if multi > 0 {
        let w = 0.1 + 0.3 * (multi as f32 / 5.0).clamp(0.0, 1.0);
        score += w;
        signals.push(CollusionSignal {
            kind: "cluster".into(),
            detail: format!("{multi} 处条款在 3 份及以上标书间高度雷同"),
            weight: w,
        });
    } else if !clusters.is_empty() {
        score += 0.1;
        signals.push(CollusionSignal {
            kind: "cluster".into(),
            detail: format!("{} 处跨标书雷同条款", clusters.len()),
            weight: 0.1,
        });
    }

    // 3) 元数据同源（作者 / 最后修改人 / 制作软件一致）
    let meta = docs
        .iter()
        .filter(|d| !d.fingerprint.risk_flags.is_empty())
        .count();
    if meta >= 2 {
        score += 0.25;
        signals.push(CollusionSignal {
            kind: "metadata".into(),
            detail: "多份文档元数据同源（作者 / 修改人 / 制作软件一致）".into(),
            weight: 0.25,
        });
    }

    // 4) 共有特征词 / 疑似共用笔误
    if shared_terms.len() >= 5 {
        score += 0.1;
        signals.push(CollusionSignal {
            kind: "sharedTerms".into(),
            detail: format!("{} 个罕见特征词被多份标书共用", shared_terms.len()),
            weight: 0.1,
        });
    }

    // 5) 报价梯度雷同：金额仅差几个百分点 + 多处条款雷同，是典型的围标陪标特征。
    // 多对接近时全部列出（最多 3 对），权重记一次（同一类证据不叠加）。
    if !price_pairs.is_empty() {
        const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
        let tag = |i: usize| STEMS.get(i).copied().unwrap_or("?");
        let shown: Vec<String> = price_pairs
            .iter()
            .take(3)
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
        let w = 0.15;
        score += w;
        signals.push(CollusionSignal {
            kind: "facts".into(),
            detail: format!("报价梯度雷同：{}，且相关文档多处条款雷同", shown.join("；")),
            weight: w,
        });
    }

    let score = score.clamp(0.0, 1.0);
    let level = if score >= 0.6 {
        "high"
    } else if score >= 0.35 {
        "medium"
    } else if score > 0.1 {
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
