// 字面查重：jieba 分词 + 词频向量余弦相似度。
// 语义查重（embedding）后续用 fastembed/candle/ort 接入。
use jieba_rs::Jieba;
use std::collections::HashMap;

/// 中文分词，过滤标点与单字噪声。
pub fn tokenize(jieba: &Jieba, text: &str) -> Vec<String> {
    jieba
        .cut(text, true)
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| s.chars().count() >= 2 && s.chars().any(|c| c.is_alphanumeric()))
        .collect()
}

fn term_freq(tokens: &[String]) -> HashMap<&str, f32> {
    let mut m: HashMap<&str, f32> = HashMap::new();
    for t in tokens {
        *m.entry(t.as_str()).or_insert(0.0) += 1.0;
    }
    m
}

/// 两份文档词频向量的余弦相似度，范围 0..1。
pub fn cosine(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let ta = term_freq(a);
    let tb = term_freq(b);
    let mut dot = 0.0f32;
    for (k, va) in &ta {
        if let Some(vb) = tb.get(k) {
            dot += va * vb;
        }
    }
    let na: f32 = ta.values().map(|v| v * v).sum::<f32>().sqrt();
    let nb: f32 = tb.values().map(|v| v * v).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

/// 生成 n×n 相似度矩阵，返回 (矩阵, 非对角线峰值)。
pub fn matrix(token_docs: &[Vec<String>]) -> (Vec<Vec<f32>>, f32) {
    let n = token_docs.len();
    let mut m = vec![vec![0.0f32; n]; n];
    let mut peak = 0.0f32;
    for i in 0..n {
        m[i][i] = 1.0;
        for j in (i + 1)..n {
            let s = cosine(&token_docs[i], &token_docs[j]);
            m[i][j] = s;
            m[j][i] = s;
            if s > peak {
                peak = s;
            }
        }
    }
    (m, peak)
}
