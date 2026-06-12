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
