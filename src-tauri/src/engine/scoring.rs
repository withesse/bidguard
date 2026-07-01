// 五维加权评分（设计文档 §9.4）：词面 / 字符 n-gram / 实体 / 结构 / 顺序（+ 可选语义）。
// 不可测维度（双方都无实体、任一方无章节路径、未启用语义）按权重重分配，
// 而不是给“中性分”——否则两段完全相同的纯文本永远到不了 same 阈值带。
// 取舍说明：重分配意味着「有实体的对」与「无实体的对」分母不同，跨对分数不严格可比；
// 但实体抽取是确定性的（相同文本必然同有或同无），替代方案（双空记 1.0）会让毫不相干
// 的两段短文本凭空+0.15 权重的满分，误报代价更高。
use crate::engine::corpus::CmpChunk;
use crate::engine::features;
use serde::Serialize;

pub struct Weights {
    pub lexical: f32,
    pub char_ngram: f32,
    pub entity: f32,
    pub structure: f32,
    pub order: f32,
    pub semantic: f32,
}

pub const W_LEXICAL: Weights = Weights {
    lexical: 0.40,
    char_ngram: 0.30,
    entity: 0.15,
    structure: 0.10,
    order: 0.05,
    semantic: 0.0,
};

pub const W_SEMANTIC: Weights = Weights {
    lexical: 0.25,
    char_ngram: 0.15,
    entity: 0.10,
    structure: 0.10,
    order: 0.05,
    semantic: 0.35,
};

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoreParts {
    pub lexical: f32,
    pub char_ngram: f32,
    pub entity: Option<f32>,
    pub structure: Option<f32>,
    pub order: f32,
    pub semantic: Option<f32>,
    pub final_score: f32,
}

pub fn score_pair(a: &CmpChunk, b: &CmpChunk, semantic: Option<f32>) -> ScoreParts {
    let lexical = features::sparse_dot(&a.tfidf, &b.tfidf).clamp(0.0, 1.0);
    let char_ngram = features::jaccard(&a.ngrams, &b.ngrams);
    let entity = features::entity_sim(&a.entities, &b.entities);
    let structure = structure_sim(&a.section_path, &b.section_path);
    let order = 1.0 - (a.rel_pos - b.rel_pos).abs();

    let w = if semantic.is_some() { &W_SEMANTIC } else { &W_LEXICAL };
    // 场景化加权（§9.5）：两侧都是表格行（报价表/清单）时实体维度权重翻倍——
    // 行内文字多为表头/品名等模板成分，真正区分「同一行抄没抄」的是金额/数量/日期。
    let entity_w = if a.is_table_row && b.is_table_row { w.entity * 2.0 } else { w.entity };
    let mut num = w.lexical * lexical + w.char_ngram * char_ngram + w.order * order;
    let mut den = w.lexical + w.char_ngram + w.order;
    if let Some(e) = entity {
        num += entity_w * e;
        den += entity_w;
    }
    if let Some(s) = structure {
        num += w.structure * s;
        den += w.structure;
    }
    if let Some(sem) = semantic {
        num += w.semantic * sem;
        den += w.semantic;
    }
    ScoreParts {
        lexical,
        char_ngram,
        entity,
        structure,
        order,
        semantic,
        final_score: (num / den).clamp(0.0, 1.0),
    }
}

/// 结构相似度：比较「去掉文档标题根」后的章节路径，Dice 系数 2·lcp/(|a'|+|b'|)。
/// section_path 第 0 层是文档 H1 标题（文档身份，非章节结构）。标书标题必然不同，
/// 若把标题计入公共前缀，会让每条跨文档边的结构维恒为 0，等同对所有雷同内容加一记
/// 固定罚分（把逐字相同的段落从 100% 压到 ~89%）。故剥掉标题根再比。
///
/// 判为「不可测」(None → 踢出分母、不惩罚，交文本/语义维定夺) 的两种情形：任一方无
/// 章节路径或去根后为空（版本/日期等直挂标题下的行）；去根后无公共章节前缀（内容被
/// 挪到完全不同章节）。仅当双方共享章节前缀（如同模板围标）时结构维才加分。
fn structure_sim(a: &[String], b: &[String]) -> Option<f32> {
    let a = a.get(1..)?;
    let b = b.get(1..)?;
    if a.is_empty() || b.is_empty() {
        return None;
    }
    let lcp = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    if lcp == 0 {
        return None;
    }
    Some(2.0 * lcp as f32 / (a.len() + b.len()) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::corpus::fill_tfidf;
    use crate::engine::corpus::CmpChunk;
    use std::collections::{HashMap, HashSet};

    fn mk(doc: usize, text: &str, tokens: &[&str], path: &[&str], rel: f32) -> CmpChunk {
        let norm: String = text.split_whitespace().collect();
        CmpChunk {
            id: format!("c{doc}{rel}"),
            doc,
            rel_pos: rel,
            page: None,
            text: text.into(),
            exact_hash: String::new(),
            normalized_hash: String::new(),
            section_path: path.iter().map(|s| s.to_string()).collect(),
            section_kind: "other".into(),
            is_template: false,
            is_table_row: false,
            char_count: text.chars().count(),
            tokens: tokens.iter().map(|s| s.to_string()).collect(),
            ngrams: crate::engine::features::char_ngrams(&norm),
            minhash: Vec::new(),
            entities: crate::engine::features::extract_entities(&norm),
            tfidf: HashMap::new(),
        }
    }

    #[test]
    fn identical_text_scores_one() {
        let mut chunks = vec![
            mk(0, "系统采用分层解耦的微服务架构", &["系统", "分层", "解耦", "微服务", "架构"], &[], 0.2),
            mk(1, "系统采用分层解耦的微服务架构", &["系统", "分层", "解耦", "微服务", "架构"], &[], 0.2),
        ];
        fill_tfidf(&mut chunks);
        let p = score_pair(&chunks[0], &chunks[1], None);
        assert!(p.final_score > 0.99, "完全相同应≈1.0，实际 {}", p.final_score);
    }

    #[test]
    fn unrelated_text_scores_low() {
        let mut chunks = vec![
            mk(0, "系统采用分层解耦的微服务架构", &["系统", "分层", "解耦", "微服务", "架构"], &[], 0.1),
            mk(1, "田间环境监测帮助种植户增产增收", &["田间", "环境", "监测", "种植户", "增产"], &[], 0.9),
        ];
        fill_tfidf(&mut chunks);
        let p = score_pair(&chunks[0], &chunks[1], None);
        assert!(p.final_score < 0.2, "无关文本应很低，实际 {}", p.final_score);
    }

    #[test]
    fn semantic_lifts_rewrite() {
        let mut chunks = vec![
            mk(0, "本方案使用分层的微服务体系对外提供能力", &["方案", "分层", "微服务", "体系", "提供", "能力"], &[], 0.3),
            mk(1, "系统架构按服务化拆分并经网关统一暴露", &["系统", "架构", "服务化", "拆分", "网关", "暴露"], &[], 0.3),
        ];
        fill_tfidf(&mut chunks);
        let without = score_pair(&chunks[0], &chunks[1], None);
        let with = score_pair(&chunks[0], &chunks[1], Some(0.92));
        assert!(with.final_score > without.final_score + 0.2, "语义高分应显著抬升改写对：{} vs {}", with.final_score, without.final_score);
    }

    #[test]
    fn entity_mismatch_drags_score() {
        let mut chunks = vec![
            mk(0, "投标报价为12800000元，工期180个日历日", &["投标", "报价", "工期"], &[], 0.5),
            mk(1, "投标报价为9990000元，工期90个日历日", &["投标", "报价", "工期"], &[], 0.5),
        ];
        fill_tfidf(&mut chunks);
        let p = score_pair(&chunks[0], &chunks[1], None);
        assert_eq!(p.entity, Some(0.0), "金额工期全不同 → 实体重合 0");
        assert!(p.final_score < 0.9);
    }

    #[test]
    fn table_rows_weight_entities_higher() {
        // 同表头不同金额的两行：表格行模式下实体不匹配的拖累应更重
        let mk_row = |doc: usize, amount: &str, table: bool| {
            let text = format!("1 | 核心交换机 | {amount}");
            let mut c = mk(doc, &text, &["核心", "交换机"], &[], 0.5);
            c.is_table_row = table;
            c
        };
        let mut as_rows = vec![mk_row(0, "64000元", true), mk_row(1, "78000元", true)];
        let mut as_paras = vec![mk_row(0, "64000元", false), mk_row(1, "78000元", false)];
        fill_tfidf(&mut as_rows);
        fill_tfidf(&mut as_paras);
        let row_score = score_pair(&as_rows[0], &as_rows[1], None).final_score;
        let para_score = score_pair(&as_paras[0], &as_paras[1], None).final_score;
        assert!(
            row_score < para_score,
            "实体权重翻倍后，金额不同的表格行得分应低于按段落计：row={row_score} para={para_score}"
        );
        // 金额相同的表格行不受影响（实体全匹配 → 加权方向一致）
        let mut same = vec![mk_row(0, "64000元", true), mk_row(1, "64000元", true)];
        fill_tfidf(&mut same);
        assert!(score_pair(&same[0], &same[1], None).final_score > 0.99);
    }

    #[test]
    fn structure_sim_strips_title_and_reallocates() {
        let t = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<String>>();
        // 标题不同但子章节一致（同模板围标）→ 剥标题后前缀命中 → 有分
        assert_eq!(structure_sim(&t(&["标A", "3数据", "3.2表"]), &t(&["标B", "3数据", "3.2表"])), Some(1.0));
        // 标题不同、子章节也不同 → 剥后无公共前缀 → 不可测
        assert_eq!(structure_sim(&t(&["标A", "11数据模型"]), &t(&["标B", "5数据架构"])), None);
        // 只有标题（版本/日期行）→ 剥后为空 → 不可测
        assert_eq!(structure_sim(&t(&["标A"]), &t(&["标B"])), None);
        // 空路径 → 不可测
        assert_eq!(structure_sim(&[], &t(&["标B"])), None);
        // 部分子章节一致 → Dice 0.5
        assert_eq!(structure_sim(&t(&["A", "3数据", "3.2表"]), &t(&["B", "3数据", "3.9图"])), Some(0.5));
        let _ = HashSet::<u64>::new(); // 防未用导入告警
    }
}
