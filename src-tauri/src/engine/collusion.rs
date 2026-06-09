// 围标综合判定：把文本相似度、跨文档雷同条款、元数据同源、共有特征词加权成一个结论。
use crate::engine::report::{Cluster, Collusion, CollusionSignal, DocInfo, SharedTerm};

pub fn assess(
    peak: f32,
    clusters: &[Cluster],
    docs: &[DocInfo],
    shared_terms: &[SharedTerm],
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
