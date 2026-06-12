// 分块特征：字符 n-gram（哈希集合）、MinHash、实体抽取、TF-IDF。
// n-gram 在比对期由 normalized_text 现算（廉价，不落库）；MinHash 与实体在导入期落库。
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

pub fn hash64(s: &str) -> u64 {
    twox_hash::XxHash64::oneshot(0, s.as_bytes())
}

/// 字符 2-gram ∪ 3-gram 的哈希集合（中文短文本相似度的主力特征）。
pub fn char_ngrams(s: &str) -> HashSet<u64> {
    let chars: Vec<char> = s.chars().collect();
    let mut out = HashSet::new();
    for n in [2usize, 3] {
        if chars.len() < n {
            continue;
        }
        for w in chars.windows(n) {
            out.insert(hash64(&w.iter().collect::<String>()));
        }
    }
    out
}

pub fn jaccard(a: &HashSet<u64>, b: &HashSet<u64>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let inter = a.intersection(b).count();
    let union = a.len() + b.len() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f32 / union as f32
    }
}

pub const MINHASH_K: usize = 128;

/// 128 维 MinHash 签名：用 splitmix64 派生 128 组仿射变换（确定性、无依赖）。
pub fn minhash(ngrams: &HashSet<u64>) -> Vec<u64> {
    let mut sig = vec![u64::MAX; MINHASH_K];
    if ngrams.is_empty() {
        return sig;
    }
    let params = minhash_params();
    for &g in ngrams {
        for (k, &(a, b)) in params.iter().enumerate() {
            let v = g.wrapping_mul(a).wrapping_add(b);
            if v < sig[k] {
                sig[k] = v;
            }
        }
    }
    sig
}

fn minhash_params() -> &'static [(u64, u64); MINHASH_K] {
    static PARAMS: OnceLock<[(u64, u64); MINHASH_K]> = OnceLock::new();
    PARAMS.get_or_init(|| {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = || {
            state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        std::array::from_fn(|_| (next() | 1, next()))
    })
}

/// MinHash 估计的 Jaccard 相似度。
pub fn minhash_sim(a: &[u64], b: &[u64]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let same = a.iter().zip(b).filter(|(x, y)| x == y).count();
    same as f32 / a.len() as f32
}

pub fn minhash_to_blob(sig: &[u64]) -> Vec<u8> {
    sig.iter().flat_map(|v| v.to_le_bytes()).collect()
}

pub fn blob_to_minhash(blob: &[u8]) -> Vec<u64> {
    blob.chunks_exact(8)
        .map(|c| u64::from_le_bytes(c.try_into().unwrap()))
        .collect()
}

// —— 实体抽取（金额/日期/工期/百分比的规则雏形，事实冲突检测在其上扩展）——

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entity {
    pub kind: String, // amount | percentage | date | duration
    pub value: String,
}

fn entity_regexes() -> &'static [(&'static str, Regex)] {
    static RES: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    RES.get_or_init(|| {
        vec![
            // 顺序即优先级：先长后短，避免「3年」被「3」截走
            // 「2026年」纯年份（招标里常见的交付里程碑）也算日期
            ("date", Regex::new(r"\d{4}年(?:\d{1,2}月(?:\d{1,2}日)?)?|\d{4}[-/]\d{1,2}(?:[-/]\d{1,2})?|\d{1,2}月\d{1,2}日").unwrap()),
            ("amount", Regex::new(r"\d+(?:\.\d+)?\s*万?元").unwrap()),
            ("percentage", Regex::new(r"\d+(?:\.\d+)?\s*[%％]").unwrap()),
            ("duration", Regex::new(r"\d+\s*个?(?:日历日|工作日|小时|日|天|月|年|周)").unwrap()),
        ]
    })
}

/// 在归一化文本上抽取实体（数字已是阿拉伯形态）。同一片段只归入第一个命中的类别。
pub fn extract_entities(normalized: &str) -> Vec<Entity> {
    let mut taken: Vec<(usize, usize)> = Vec::new();
    let mut out = Vec::new();
    for (kind, re) in entity_regexes() {
        for m in re.find_iter(normalized) {
            let span = (m.start(), m.end());
            if taken.iter().any(|&(s, e)| span.0 < e && s < span.1) {
                continue; // 与更高优先级的命中重叠
            }
            taken.push(span);
            out.push(Entity {
                kind: kind.to_string(),
                value: m.as_str().replace([' ', '　'], ""),
            });
        }
    }
    out
}

/// 实体集合重合度（Jaccard）。返回 None 表示双方都没有实体（该维不可测）。
pub fn entity_sim(a: &[Entity], b: &[Entity]) -> Option<f32> {
    if a.is_empty() && b.is_empty() {
        return None;
    }
    let sa: HashSet<&Entity> = a.iter().collect();
    let sb: HashSet<&Entity> = b.iter().collect();
    let inter = sa.intersection(&sb).count();
    let union = sa.len() + sb.len() - inter;
    Some(if union == 0 { 0.0 } else { inter as f32 / union as f32 })
}

// —— TF-IDF ——

/// 按参与比对的 chunk 语料现算 IDF：idf = ln((N+1)/(df+1)) + 1。
pub fn idf_of<'a>(token_lists: impl Iterator<Item = &'a [String]>) -> HashMap<String, f32> {
    let mut df: HashMap<String, u32> = HashMap::new();
    let mut n = 0u32;
    for tokens in token_lists {
        n += 1;
        let uniq: HashSet<&String> = tokens.iter().collect();
        for t in uniq {
            *df.entry(t.clone()).or_insert(0) += 1;
        }
    }
    df.into_iter()
        .map(|(t, d)| (t, ((n as f32 + 1.0) / (d as f32 + 1.0)).ln() + 1.0))
        .collect()
}

/// 预计算 L2 归一化的 tf-idf 稀疏向量（比对期每 chunk 一次，pair 打分只做点积）。
pub fn weighted_vec(tokens: &[String], idf: &HashMap<String, f32>) -> HashMap<String, f32> {
    let mut tf: HashMap<&String, f32> = HashMap::new();
    for t in tokens {
        *tf.entry(t).or_insert(0.0) += 1.0;
    }
    let mut v: HashMap<String, f32> = tf
        .into_iter()
        .map(|(t, f)| {
            let w = f * idf.get(t).copied().unwrap_or(1.0);
            (t.clone(), w)
        })
        .collect();
    let norm: f32 = v.values().map(|w| w * w).sum::<f32>().sqrt();
    if norm > 0.0 {
        for w in v.values_mut() {
            *w /= norm;
        }
    }
    v
}

/// 两个 L2 归一化稀疏向量的余弦（点积）。
pub fn sparse_dot(a: &HashMap<String, f32>, b: &HashMap<String, f32>) -> f32 {
    let (small, big) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    small
        .iter()
        .filter_map(|(t, w)| big.get(t).map(|v| w * v))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ngrams_and_jaccard() {
        let a = char_ngrams("微服务架构设计");
        let b = char_ngrams("微服务架构设计");
        let c = char_ngrams("数据治理与合规");
        assert!((jaccard(&a, &b) - 1.0).abs() < 1e-6);
        assert!(jaccard(&a, &c) < 0.1);
        assert!(jaccard(&a, &HashSet::new()) == 0.0);
    }

    #[test]
    fn minhash_estimates_jaccard() {
        let a = char_ngrams("系统采用分层解耦的微服务架构统一网关对外暴露能力");
        let b = char_ngrams("系统采用分层解耦的微服务架构统一网关对外提供能力");
        let c = char_ngrams("智慧农业物联网传感终端的研发生产与销售");
        let real_ab = jaccard(&a, &b);
        let est_ab = minhash_sim(&minhash(&a), &minhash(&b));
        assert!((real_ab - est_ab).abs() < 0.2, "估计 {est_ab} vs 实际 {real_ab}");
        assert!(minhash_sim(&minhash(&a), &minhash(&c)) < 0.15);
        // blob 往返
        let sig = minhash(&a);
        assert_eq!(blob_to_minhash(&minhash_to_blob(&sig)), sig);
    }

    #[test]
    fn entities_extracted_with_priority() {
        let ents = extract_entities("投标报价为人民币12800000元整，工期180个日历日，质保金比例为5%，开工日期2026年6月10日");
        let kinds: Vec<&str> = ents.iter().map(|e| e.kind.as_str()).collect();
        assert!(kinds.contains(&"amount"));
        assert!(kinds.contains(&"duration"));
        assert!(kinds.contains(&"percentage"));
        assert!(kinds.contains(&"date"));
        assert!(ents.iter().any(|e| e.value == "12800000元"));
        assert!(ents.iter().any(|e| e.value == "180个日历日"));
    }

    #[test]
    fn entity_sim_semantics() {
        let a = extract_entities("报价12800000元，工期180个日历日");
        let b = extract_entities("报价12900000元，工期180个日历日");
        let sim = entity_sim(&a, &b).unwrap();
        assert!(sim > 0.0 && sim < 1.0, "部分重合：{sim}");
        assert!(entity_sim(&[], &[]).is_none(), "双方无实体 → 不可测");
        assert_eq!(entity_sim(&a, &[]), Some(0.0));
    }

    #[test]
    fn tfidf_downweights_common_tokens() {
        let docs: Vec<Vec<String>> = vec![
            vec!["项目".into(), "微服务".into(), "架构".into()],
            vec!["项目".into(), "数据".into(), "治理".into()],
            vec!["项目".into(), "传感".into(), "终端".into()],
        ];
        let idf = idf_of(docs.iter().map(|d| d.as_slice()));
        assert!(idf["项目"] < idf["微服务"], "高频词 IDF 应更低");
        let va = weighted_vec(&docs[0], &idf);
        let vb = weighted_vec(&docs[1], &idf);
        let sim = sparse_dot(&va, &vb);
        assert!(sim > 0.0 && sim < 0.5, "只共享高频词的相似度应被压低：{sim}");
        assert!((sparse_dot(&va, &va) - 1.0).abs() < 1e-5, "自身余弦应为 1");
    }
}
