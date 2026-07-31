// 比对期内存模型：把库里的分块行还原为带特征的 CmpChunk，并按语料填充 TF-IDF 向量。
use crate::db::repo::chunk_repo::CompareChunkRow;
use crate::engine::features::{self, Entity};
use crate::engine::segment;
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
    /// 引用招标文件的字符覆盖率 ∈ [0,1]（W3-2 招标对减）：命中招标 winnowing 指纹的覆盖占比。
    /// 由 compare_service 建 TenderIndex 后逐块填充；无招标文件/未开对减时恒 0.0。
    pub tender_coverage: f32,
    /// 行业范本背景库套话占比 ∈ [0,1]（W3-4）：命中内置静态背景库 boilerplate 4-gram 的占比。
    /// 由 compare_service 逐块填充（ignore_templates 开启 或 已导入招标文件时才计算）；否则恒 0.0。
    /// 供 k-共现查证（W3-3）判定 ≥3 家共有簇是否属行业范本套话（多数成员 ≥0.6 → background 豁免）。
    pub boiler_fraction: f32,
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
    // 契约：order_index 必须是本文档本粒度内的稠密行序（0..total-1）。chunker 的 order_index
    // 含 heading 编号有空洞，调用方（compare_service）在 load_for_compare 后按加载顺序重编，
    // 否则 rel_pos 会 >1.0，污染 order 维与「位置移动」判定。
    let rel_pos = if doc_chunk_total > 1 {
        row.order_index as f32 / (doc_chunk_total - 1) as f32
    } else {
        0.0
    };
    // 五区分类比对期重算（§5 W3-5）：旧库 section_kind 仅三值（tech/business/other），此处按
    // classify_zone 现算 zone（廉价确定性，同 chunker.make 口径）——旧库不重导入即产出 legal/price，
    // 新库幂等（导入期已写同值，重算结果一致）。无迁移，chunks.section_kind 保持原字节。
    let is_table_row = row.chunk_type == "table_row";
    let has_amount = entities.iter().any(|e| e.kind == "amount");
    let section_kind = segment::section_kind_str(segment::classify_zone(
        &section_path,
        &row.text,
        is_table_row,
        has_amount,
    ))
    .to_string();
    CmpChunk {
        id: row.id,
        doc,
        rel_pos,
        page: row.page.map(|p| p as u32),
        text: row.text,
        exact_hash: row.exact_hash,
        normalized_hash: row.normalized_hash,
        section_path,
        section_kind,
        is_template: row.is_template,
        is_table_row,
        char_count: row.char_count as usize,
        tokens,
        ngrams,
        minhash,
        entities,
        tfidf: HashMap::new(),
        tender_coverage: 0.0,
        boiler_fraction: 0.0,
    }
}

/// 按参评语料计算 IDF 并填充每个 chunk 的归一化 tf-idf 向量。
pub fn fill_tfidf(chunks: &mut [CmpChunk]) {
    let idf = features::idf_of(chunks.iter().map(|c| c.tokens.as_slice()));
    for c in chunks.iter_mut() {
        c.tfidf = features::weighted_vec(&c.tokens, &idf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(section_kind: &str, section_path: Option<&str>, text: &str, chunk_type: &str) -> CompareChunkRow {
        CompareChunkRow {
            id: "c1".into(),
            order_index: 0,
            text: text.into(),
            normalized_text: text.into(),
            exact_hash: String::new(),
            normalized_hash: String::new(),
            section_path: section_path.map(|s| serde_json::to_string(&vec![s]).unwrap()),
            section_kind: Some(section_kind.into()),
            is_template: false,
            page: None,
            char_count: text.chars().count() as i64,
            token_json: None,
            entity_json: None,
            minhash_blob: None,
            chunk_type: chunk_type.into(),
        }
    }

    #[test]
    fn from_row_recomputes_zone_for_legacy_three_value_rows() {
        // 旧库仅存 business（三值时代），标题为「投标函」→ 比对期重算应升为 legal（不重导入）。
        let c = from_row(row("business", Some("投标函"), "招标人名称：本公司承诺", "section"), 0, 1);
        assert_eq!(c.section_kind, "legal");
        // 旧库存 other，标题为「已标价工程量清单」→ 重算应为 price。
        let c2 = from_row(row("other", Some("已标价工程量清单"), "综合单价 合价", "section"), 0, 1);
        assert_eq!(c2.section_kind, "price");
        // 旧库存 business，无标题、纯技术正文 → 重算应回落 tech（关键词多数决）。
        let c3 = from_row(row("business", None, "系统采用分层解耦的微服务架构设计", "paragraph"), 0, 1);
        assert_eq!(c3.section_kind, "tech");
    }
}
