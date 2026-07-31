// 多通道候选召回（设计文档 §9.3）：
// 候选集合 = hash 命中 ∪ n-gram 倒排 ∪ TF-IDF TopK ∪ embedding TopK（可选）。
// 避免对全部 chunk 做 O(M²) 精排；每 chunk 候选数受 top_k 约束。
use crate::engine::corpus::CmpChunk;
use crate::engine::features;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

pub struct RecallParams {
    pub top_k: usize,
    /// embedding 通道的最低余弦（语义召回只为抓改写，宁缺毋滥）
    pub semantic_floor: f32,
    /// n-gram 通道至少共享多少个 gram 才算候选
    pub min_shared_grams: usize,
    /// 倒排表过长的 gram 视为停用（模板高频片段），不参与召回
    pub stop_gram_df: usize,
}

impl Default for RecallParams {
    fn default() -> Self {
        Self {
            top_k: 100,
            semantic_floor: 0.78,
            min_shared_grams: 3,
            stop_gram_df: 256,
        }
    }
}

/// 返回跨文档候选对（i<j，chunk 下标）。调用方应已过滤模板与空 token 分块。
pub fn recall(
    chunks: &[CmpChunk],
    embeddings: Option<&[Option<Vec<f32>>]>,
    p: &RecallParams,
) -> HashSet<(u32, u32)> {
    let mut out: HashSet<(u32, u32)> = HashSet::new();
    let push = |out: &mut HashSet<(u32, u32)>, i: u32, j: u32| {
        if i != j {
            out.insert((i.min(j), i.max(j)));
        }
    };

    // 通道 1/2：exact / normalized hash 桶 —— 相同文本直接候选
    for key_of in [
        (|c: &CmpChunk| c.exact_hash.clone()) as fn(&CmpChunk) -> String,
        |c| c.normalized_hash.clone(),
    ] {
        let mut buckets: HashMap<String, Vec<u32>> = HashMap::new();
        for (i, c) in chunks.iter().enumerate() {
            let k = key_of(c);
            if !k.is_empty() {
                buckets.entry(k).or_default().push(i as u32);
            }
        }
        for idxs in buckets.values() {
            for (x, &i) in idxs.iter().enumerate() {
                for &j in &idxs[x + 1..] {
                    if chunks[i as usize].doc != chunks[j as usize].doc {
                        push(&mut out, i, j);
                    }
                }
            }
        }
    }

    // 通道 3：字符 n-gram 倒排索引，按 MinHash 估计排序取每 chunk TopK
    {
        let mut inverted: HashMap<u64, Vec<u32>> = HashMap::new();
        for (i, c) in chunks.iter().enumerate() {
            for &g in &c.ngrams {
                inverted.entry(g).or_default().push(i as u32);
            }
        }
        inverted.retain(|_, v| v.len() <= p.stop_gram_df);

        let pairs: Vec<Vec<(u32, u32)>> = chunks
            .par_iter()
            .enumerate()
            .map(|(i, c)| {
                let mut shared: HashMap<u32, u32> = HashMap::new();
                for g in &c.ngrams {
                    if let Some(post) = inverted.get(g) {
                        for &j in post {
                            // 只统计 j > i，整体对数减半
                            if j as usize > i && chunks[j as usize].doc != c.doc {
                                *shared.entry(j).or_insert(0) += 1;
                            }
                        }
                    }
                }
                let mut cands: Vec<(u32, f32)> = shared
                    .into_iter()
                    .filter(|(_, n)| *n as usize >= p.min_shared_grams)
                    .map(|(j, _)| {
                        (j, features::minhash_sim(&c.minhash, &chunks[j as usize].minhash))
                    })
                    .collect();
                cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                cands.truncate(p.top_k);
                cands.into_iter().map(|(j, _)| (i as u32, j)).collect()
            })
            .collect();
        for v in pairs {
            for (i, j) in v {
                push(&mut out, i, j);
            }
        }
    }

    // 通道 4：TF-IDF TopK —— 词面相似但 n-gram 稀疏（改换措辞）时兜底
    {
        let mut inverted: HashMap<&str, Vec<(u32, f32)>> = HashMap::new();
        for (i, c) in chunks.iter().enumerate() {
            for (t, w) in &c.tfidf {
                inverted.entry(t.as_str()).or_default().push((i as u32, *w));
            }
        }
        // 与通道 3 的 stop_gram_df 对称：posting 过长的高频词（模板/通用词）视为停用词剔除，
        // 否则每 chunk 每 token 无条件扫全 posting，最坏 Σdf² 退化 O(n²)。高频词 IDF 权重本就低，
        // 对召回几乎无损。
        inverted.retain(|_, v| v.len() <= p.stop_gram_df);
        let pairs: Vec<Vec<(u32, u32)>> = chunks
            .par_iter()
            .enumerate()
            .map(|(i, c)| {
                let mut dot: HashMap<u32, f32> = HashMap::new();
                for (t, w) in &c.tfidf {
                    if let Some(post) = inverted.get(t.as_str()) {
                        for &(j, wj) in post {
                            if j as usize > i && chunks[j as usize].doc != c.doc {
                                *dot.entry(j).or_insert(0.0) += w * wj;
                            }
                        }
                    }
                }
                let mut cands: Vec<(u32, f32)> =
                    dot.into_iter().filter(|(_, s)| *s >= 0.25).collect();
                cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                cands.truncate(p.top_k);
                cands.into_iter().map(|(j, _)| (i as u32, j)).collect()
            })
            .collect();
        for v in pairs {
            for (i, j) in v {
                push(&mut out, i, j);
            }
        }
    }

    // 通道 5（可选）：embedding TopK —— 抓字面几乎不重合的改写
    if let Some(embs) = embeddings {
        // n 小时精确全比对（覆盖测试与典型 2~10 份文档，结果与历史一致）；n 大时用固定种子的
        // SimHash LSH 粗聚、仅桶内做精确余弦，避免全比对的 O(n²) 性能悬崖（sentence 粒度大语料）。
        let pairs: Vec<Vec<(u32, u32)>> = if chunks.len() <= EMBED_EXACT_MAX {
            chunks
                .par_iter()
                .enumerate()
                .map(|(i, c)| {
                    let Some(Some(ei)) = embs.get(i) else { return Vec::new() };
                    let mut cands: Vec<(u32, f32)> = Vec::new();
                    for (j, cj) in chunks.iter().enumerate() {
                        if j <= i || cj.doc == c.doc {
                            continue;
                        }
                        let Some(Some(ej)) = embs.get(j) else { continue };
                        let cos = crate::engine::embed::cosine(ei, ej);
                        if cos >= p.semantic_floor {
                            cands.push((j as u32, cos));
                        }
                    }
                    cands.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                    cands.truncate(5);
                    cands.into_iter().map(|(j, _)| (i as u32, j)).collect()
                })
                .collect()
        } else {
            lsh_embed_recall(chunks, embs, p)
        };
        for v in pairs {
            for (i, j) in v {
                push(&mut out, i, j);
            }
        }
    }

    out
}

// n≤此值走精确 O(n²)（覆盖全部测试与典型用法，结果与历史一致）；超过则走 LSH 近似召回。
const EMBED_EXACT_MAX: usize = 3000;
const LSH_BITS: usize = 16; // 每表签名位数
const LSH_TABLES: usize = 6; // 表数（多表提升召回，减少桶边界漏配）

#[inline]
pub(crate) fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E3779B97F4A7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
    z ^ (z >> 31)
}

/// 确定性随机超平面分量 ∈ [-1,1)：由 (plane, dim) 生成，无需随机数依赖且可复现
/// （取证工具须同输入同结果）。
#[inline]
fn hyperplane_component(plane: usize, d: usize) -> f32 {
    let h = splitmix64(((plane as u64) << 40) ^ (d as u64).wrapping_mul(0x100_0000_01B3));
    (h >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
}

/// 某向量在某表的 LSH_BITS 位 SimHash 签名（各位 = 与一个随机超平面点积的符号）。
fn lsh_signature(v: &[f32], table: usize) -> u16 {
    let mut sig = 0u16;
    for bit in 0..LSH_BITS {
        let plane = table * LSH_BITS + bit;
        let dot: f32 = v.iter().enumerate().map(|(d, &x)| x * hyperplane_component(plane, d)).sum();
        if dot >= 0.0 {
            sig |= 1 << bit;
        }
    }
    sig
}

/// 大语料语义召回：SimHash LSH 分桶 → 桶内精确余弦 → floor + 每 chunk top-5。
fn lsh_embed_recall(
    chunks: &[CmpChunk],
    embs: &[Option<Vec<f32>>],
    p: &RecallParams,
) -> Vec<Vec<(u32, u32)>> {
    // 每个 chunk 每张表的签名（无向量者为 None）
    let sigs: Vec<Option<[u16; LSH_TABLES]>> = (0..chunks.len())
        .into_par_iter()
        .map(|i| {
            let e = embs.get(i)?.as_ref()?;
            let mut s = [0u16; LSH_TABLES];
            for (t, slot) in s.iter_mut().enumerate() {
                *slot = lsh_signature(e, t);
            }
            Some(s)
        })
        .collect();
    // 分桶：每表 signature → 成员下标
    let mut buckets: Vec<HashMap<u16, Vec<u32>>> = vec![HashMap::new(); LSH_TABLES];
    for (i, sig) in sigs.iter().enumerate() {
        if let Some(s) = sig {
            for (t, &key) in s.iter().enumerate() {
                buckets[t].entry(key).or_default().push(i as u32);
            }
        }
    }
    // 每个 chunk：跨表收集同桶候选 → 去重 → 桶内精确余弦 → floor + top-5
    (0..chunks.len())
        .into_par_iter()
        .map(|i| {
            let Some(si) = &sigs[i] else { return Vec::new() };
            let Some(Some(ei)) = embs.get(i) else { return Vec::new() };
            let c = &chunks[i];
            let mut seen: HashSet<u32> = HashSet::new();
            for (t, &key) in si.iter().enumerate() {
                if let Some(post) = buckets[t].get(&key) {
                    for &j in post {
                        if j as usize != i && chunks[j as usize].doc != c.doc {
                            seen.insert(j);
                        }
                    }
                }
            }
            let mut cands: Vec<(u32, f32)> = seen
                .into_iter()
                .filter_map(|j| {
                    let ej = embs.get(j as usize)?.as_ref()?;
                    let cos = crate::engine::embed::cosine(ei, ej);
                    (cos >= p.semantic_floor).then_some((j, cos))
                })
                .collect();
            // 余弦降序，并列时按 j 升序打破平局：cands 来自 HashSet(随机迭代顺序)，若不加次级键，
            // top-5 截断在余弦并列(逐字抄的相同向量)时会跨进程漂移，破坏取证工具的结果可复现性。
            cands.sort_by(|a, b| {
                b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then(a.0.cmp(&b.0))
            });
            cands.truncate(5);
            cands.into_iter().map(|(j, _)| (i as u32, j)).collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::corpus::{fill_tfidf, from_row};
    use crate::db::repo::chunk_repo::CompareChunkRow;

    fn row(text: &str, tokens: &[&str]) -> CompareChunkRow {
        let norm: String = text.split_whitespace().collect();
        CompareChunkRow {
            id: uuid::Uuid::new_v4().to_string(),
            order_index: 0,
            text: text.into(),
            normalized_text: norm.clone(),
            exact_hash: crate::engine::normalize::sha256_hex(text.as_bytes()),
            normalized_hash: crate::engine::normalize::sha256_hex(norm.as_bytes()),
            section_path: None,
            section_kind: None,
            is_template: false,
            page: None,
            char_count: text.chars().count() as i64,
            token_json: serde_json::to_string(&tokens).ok(),
            entity_json: None,
            minhash_blob: None,
            chunk_type: "paragraph".into(),
        }
    }

    #[test]
    fn hash_and_ngram_channels_recall_similar_pairs() {
        let mut chunks = vec![
            from_row(row("系统采用分层解耦的微服务总体架构设计方案", &["系统", "分层", "解耦", "微服务", "架构"]), 0, 2),
            from_row(row("智慧农业物联网传感终端的研发与销售", &["智慧", "农业", "物联网", "传感", "终端"]), 0, 2),
            from_row(row("系统采用分层解耦的微服务总体架构设计方案", &["系统", "分层", "解耦", "微服务", "架构"]), 1, 2),
            from_row(row("系统采用分层解耦微服务的总体架构设计思路", &["系统", "分层", "解耦", "微服务", "架构", "思路"]), 2, 1),
        ];
        fill_tfidf(&mut chunks);
        let got = recall(&chunks, None, &RecallParams::default());
        assert!(got.contains(&(0, 2)), "完全相同 → hash 通道命中");
        assert!(got.contains(&(0, 3)), "高度相似 → n-gram/TF-IDF 通道命中");
        assert!(!got.contains(&(1, 2)), "无关段落不应成为候选");
        // 同文档内不召回
        assert!(!got.contains(&(0, 1)));
    }
}
