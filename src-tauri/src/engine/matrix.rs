// 文档级相似度矩阵（设计文档 §13.2）：由 cluster 覆盖率聚合。
// sim(i,j) = Σ(命中条款的 分数×较短块字数) / min(两文档可比字数) —— 度量「较小文档被覆盖的比例」。
use crate::engine::clustering::RawCluster;
use crate::engine::corpus::CmpChunk;

pub fn doc_matrix(
    n_docs: usize,
    chunks: &[CmpChunk],
    clusters: &[RawCluster],
) -> (Vec<Vec<f32>>, f32) {
    let mut totals = vec![0f64; n_docs];
    for c in chunks {
        totals[c.doc] += c.char_count as f64;
    }

    let mut matched = vec![vec![0f64; n_docs]; n_docs];
    for cl in clusters {
        // 每文档的 primary 成员代表该条款
        let primaries: Vec<u32> = cl
            .members
            .iter()
            .copied()
            .filter(|m| cl.roles.get(m) == Some(&"primary"))
            .collect();
        for (x, &a) in primaries.iter().enumerate() {
            for &b in &primaries[x + 1..] {
                let (ca, cb) = (&chunks[a as usize], &chunks[b as usize]);
                if ca.doc == cb.doc {
                    continue;
                }
                let key = (a.min(b), a.max(b));
                // hub 拓扑下两个 primary 之间可能没有直接边：现算该对的真实分，
                // 不用组平均回落（组平均混入弱成员对，会系统性偏移）
                let score = cl
                    .pair_scores
                    .get(&key)
                    .copied()
                    .unwrap_or_else(|| {
                        crate::engine::scoring::score_pair(ca, cb, None).final_score
                    }) as f64;
                let weight = ca.char_count.min(cb.char_count) as f64;
                matched[ca.doc][cb.doc] += score * weight;
                matched[cb.doc][ca.doc] += score * weight;
            }
        }
    }

    let mut m = vec![vec![0f32; n_docs]; n_docs];
    let mut peak = 0f32;
    for i in 0..n_docs {
        m[i][i] = 1.0;
        for j in (i + 1)..n_docs {
            let den = totals[i].min(totals[j]);
            let sim = if den > 0.0 {
                (matched[i][j] / den).min(1.0) as f32
            } else {
                0.0
            };
            m[i][j] = sim;
            m[j][i] = sim;
            peak = peak.max(sim);
        }
    }
    (m, peak)
}

/// 区段口径矩阵输入：一条对齐区段在两文档间细化后的覆盖字数（doc 为文档位次）。
pub struct SegCoverage {
    pub doc_a: usize,
    pub doc_b: usize,
    pub a_covered_chars: i64,
    pub b_covered_chars: i64,
}

/// 区段口径的文档相似矩阵（W4-4，M5）：由对齐区段的细化后覆盖字数聚合，与 doc_matrix 同分母
/// （较小文档被覆盖比例）。每条区段贡献 min(两侧覆盖字数)（两侧近似相等，取小侧稳健），
/// 累加后 sim(i,j)=Σcov / min(totalA, totalB) 并 clamp 至 1（跨区段覆盖同块的极少见重叠由 clamp 兜底）。
/// 区段来源已是残差·剔除后口径（种子喂残差边、逐字锚点已丢弃落在招标豁免块的区间），与主矩阵同步。
/// 无区段时全 0（对角线 1），前端据此回退聚类口径。
pub fn doc_matrix_segments(
    n_docs: usize,
    chunks: &[CmpChunk],
    segs: &[SegCoverage],
) -> (Vec<Vec<f32>>, f32) {
    let mut totals = vec![0f64; n_docs];
    for c in chunks {
        totals[c.doc] += c.char_count as f64;
    }
    let mut matched = vec![vec![0f64; n_docs]; n_docs];
    for s in segs {
        if s.doc_a == s.doc_b || s.doc_a >= n_docs || s.doc_b >= n_docs {
            continue;
        }
        let cov = s.a_covered_chars.min(s.b_covered_chars).max(0) as f64;
        matched[s.doc_a][s.doc_b] += cov;
        matched[s.doc_b][s.doc_a] += cov;
    }
    let mut m = vec![vec![0f32; n_docs]; n_docs];
    let mut peak = 0f32;
    for i in 0..n_docs {
        m[i][i] = 1.0;
        for j in (i + 1)..n_docs {
            let den = totals[i].min(totals[j]);
            let sim = if den > 0.0 { (matched[i][j] / den).min(1.0) as f32 } else { 0.0 };
            m[i][j] = sim;
            m[j][i] = sim;
            peak = peak.max(sim);
        }
    }
    (m, peak)
}
