// 比对期内存模型：把库里的分块行还原为带特征的 CmpChunk，并按语料填充 TF-IDF 向量。
use crate::db::repo::chunk_repo::CompareChunkRow;
use crate::engine::features::{self, Entity};
use std::collections::{HashMap, HashSet};

pub struct CmpChunk {
    pub id: String,
    pub doc: usize, // 参评文档序号（即十天干位次）
    pub rel_pos: f32, // 在本文档本粒度内的相对位置 0..1
    pub page: Option<u32>,
    pub text: String,
    pub exact_hash: String,
    pub normalized_hash: String,
    pub section_path: Vec<String>,
    pub section_kind: String,
    pub is_template: bool,
    /// 表格行（报价表/清单）：评分时提升实体权重，diff 走列对齐。
    pub is_table_row: bool,
    pub char_count: usize,
    pub tokens: Vec<String>,
    pub ngrams: HashSet<u64>,
    pub minhash: Vec<u64>,
    pub entities: Vec<Entity>,
    /// L2 归一化 tf-idf 稀疏向量；IDF 依赖整个参评语料，由 fill_tfidf 统一填充。
    pub tfidf: HashMap<String, f32>,
}

/// 行 → CmpChunk。ngram 由 normalized_text 现算；minhash 优先用落库值。
pub fn from_row(row: CompareChunkRow, doc: usize, doc_chunk_total: usize) -> CmpChunk {
    let tokens: Vec<String> = row
        .token_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let entities: Vec<Entity> = row
        .entity_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let section_path: Vec<String> = row
        .section_path
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    let ngrams = features::char_ngrams(&row.normalized_text);
    let minhash = match &row.minhash_blob {
        Some(b) if !b.is_empty() => features::blob_to_minhash(b),
        _ => features::minhash(&ngrams),
    };
    let rel_pos = if doc_chunk_total > 1 {
        row.order_index as f32 / (doc_chunk_total - 1) as f32
    } else {
        0.0
    };
    CmpChunk {
        id: row.id,
        doc,
        rel_pos,
        page: row.page.map(|p| p as u32),
        text: row.text,
        exact_hash: row.exact_hash,
        normalized_hash: row.normalized_hash,
        section_path,
        section_kind: row.section_kind.unwrap_or_else(|| "other".into()),
        is_template: row.is_template,
        is_table_row: row.chunk_type == "table_row",
        char_count: row.char_count as usize,
        tokens,
        ngrams,
        minhash,
        entities,
        tfidf: HashMap::new(),
    }
}

/// 按参评语料计算 IDF 并填充每个 chunk 的归一化 tf-idf 向量。
pub fn fill_tfidf(chunks: &mut [CmpChunk]) {
    let idf = features::idf_of(chunks.iter().map(|c| c.tokens.as_slice()));
    for c in chunks.iter_mut() {
        c.tfidf = features::weighted_vec(&c.tokens, &idf);
    }
}
