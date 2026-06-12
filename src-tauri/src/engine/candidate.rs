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
        let pairs: Vec<Vec<(u32, u32)>> = chunks
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
            .collect();
        for v in pairs {
            for (i, j) in v {
                push(&mut out, i, j);
            }
        }
    }

    out
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
