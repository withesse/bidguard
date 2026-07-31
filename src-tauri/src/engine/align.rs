// 种子—链化—对齐段落对齐（W4-2，M5a）：把「散点雷同块」成型为「乙第3章 ↔ 丙第3章整体雷同
// （覆盖 82%）」的连续对齐区段——证据形态贴近评标人心智模型（PAN seed–extend–filter /
// minimap2 seed-chain-align 范式）。
//
// 定位：区段是【新增证据层，不替代聚类】。聚类承载八类分类/事实冲突/人工三态复核/批注/围标信号②
// （多文档粒度）；区段是文档对粒度的证据成型，两者经 chunk_id 互链、各司其职。围标信号①（peak）
// 本层仍走 cluster/残差口径，segmentPeak 只做展示层（W4-4，本任务不涉及）。
//
// 种子来源三类：
//   · 阶段 4 的残差 ScoredEdge（chunk 对 + 各自文档内稠密行序 rank）——W3 桥接后的残差边（不含
//     双方均引用招标文件的合法共享边）；
//   · 步骤 1 verbatim 区间映射到 chunk 范围的满分锚点（score=1.0, kind=verbatim）；
//   · 精排 fold 中 final_score ∈ [threshold−SOFT_SEED_BAND, threshold) 的软种子（仅链化用，不入
//     candidate_edges、不参与聚类）——提升链化连续性。
//
// 链化：每文档对按 a_rank 排序做 minimap2 式稀疏 DP——
//   chain_score = Σ(anchor_score × min_chars) − λ·|Δa−Δb| − μ·max(Δa,Δb)
// 回看窗口 LOOKBACK 控 O(h·k)，任一侧 gap > MAX_GAP_CHUNKS 强制断链；贪心取最优链、剔除已用
// 锚点迭代，直到无 ≥MIN_ANCHORS 且 ≥MIN_CHARS 的链。确定性：文档对序（BTreeMap）、锚点序
// （(a_rank,b_rank) 稳定排序）、链选择（严格 dp 改进 + 首个最大者胜）均无随机源。
use crate::engine::clustering::ScoredEdge;
use crate::engine::corpus::CmpChunk;
use std::collections::{BTreeMap, HashMap, HashSet};

// —— 链化常量集中区（暂不进配置面板；等 W-校准语料工作流回测后再固化）——
/// 任一侧 rank gap 超过此值强制断链（相邻锚点间容许至多 MAX_GAP_CHUNKS−1 个未命中块）。
pub const MAX_GAP_CHUNKS: usize = 8;
/// 一条链成段的最小锚点数（低于此不成区段，避免孤立命中冒充区段）。
pub const MIN_ANCHORS: usize = 2;
/// 一条链成段的最小覆盖字符数（低序文档侧被命中块字符和）。
pub const MIN_CHARS: usize = 120;
/// DP 回看窗口 h：按 (a_rank,b_rank) 排序后每锚点只回看前 LOOKBACK 个候选前驱，控 O(h·k)。
pub const LOOKBACK: usize = 50;
/// gap 代价系数 λ：错位罚（|Δa−Δb|），压制偏离对角线的跳链。
pub const GAP_LAMBDA: f32 = 1.0;
/// gap 代价系数 μ：跨距罚（max(Δa,Δb)），鼓励紧凑连续链。
pub const GAP_MU: f32 = 0.5;
/// 软种子保留带宽：精排中 final_score ∈ [threshold−SOFT_SEED_BAND, threshold) 的边作软种子。
pub const SOFT_SEED_BAND: f32 = 0.15;

/// 浮点严格改进阈值：DP 择优与链选择用，保证同分不抖动（确定性）。
const EPS: f32 = 1e-6;

/// 锚点来源。edge=残差精排边；soft=软种子带；verbatim=逐字铁证区间满分锚点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnchorKind {
    Edge,
    Soft,
    Verbatim,
}

impl AnchorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AnchorKind::Edge => "edge",
            AnchorKind::Soft => "soft",
            AnchorKind::Verbatim => "verbatim",
        }
    }
}

/// verbatim 区间映射到链化的满分锚点输入：两侧起块 id + 去空白后逐字字符数。
/// a/b 侧不预设文档序——chain 按 comparable 的 doc 重定规范化（doc_a<doc_b）。
pub struct VerbatimSeed {
    pub a_chunk_id: String,
    pub b_chunk_id: String,
    pub char_len: usize,
}

/// 一条对齐区段（文档对粒度）。orders 为各文档内稠密行序（rank）闭区间。
/// coverage=被命中块字符和 / 区间总字符和（∈[0,1]），是无重复计数的覆盖率基础（W4-4 消费）。
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedSegment {
    pub doc_a: usize,
    pub doc_b: usize,
    pub a_start_order: usize,
    pub a_end_order: usize,
    pub b_start_order: usize,
    pub b_end_order: usize,
    pub a_start_chunk_id: String,
    pub a_end_chunk_id: String,
    pub b_start_chunk_id: String,
    pub b_end_chunk_id: String,
    pub anchor_count: usize,
    pub verbatim_chars: usize,
    pub a_covered_chars: usize,
    pub b_covered_chars: usize,
    pub a_coverage: f32,
    pub b_coverage: f32,
    pub avg_score: f32,
    pub a_section_path: Option<String>,
    pub b_section_path: Option<String>,
    pub a_page_start: Option<u32>,
    pub a_page_end: Option<u32>,
    pub b_page_start: Option<u32>,
    pub b_page_end: Option<u32>,
    pub anchors: Vec<SegmentAnchor>,
    /// 相邻锚点之间「未被任何锚点命中」的 gap 块（供 W4-3 带状字符级细化定位）。
    /// 两侧全空的 gap（锚点相邻）不产出；至少一侧非空。
    pub gaps: Vec<GapPair>,
    /// 区间总字符和（a_prefix[end+1]−a_prefix[start]）：细化后覆盖率回填的分母（不落库，仅传递）。
    pub a_span: usize,
    pub b_span: usize,
}

/// 区段内一个 gap 的两侧未命中 chunk（W4-3）。细化时按 id 取原文拼接、句级带状对齐。
#[derive(Debug, Clone, PartialEq)]
pub struct GapPair {
    pub a_chunk_ids: Vec<String>,
    pub b_chunk_ids: Vec<String>,
}

/// 区段内一条锚点（供落库 segment_anchors 与按 chunk 反查 cluster_members）。
#[derive(Debug, Clone, PartialEq)]
pub struct SegmentAnchor {
    pub a_chunk_id: String,
    pub b_chunk_id: String,
    pub kind: AnchorKind,
    pub score: f32,
}

/// 内部锚点：两侧 comparable 下标 + 各自文档内 rank + 分/权/来源。
#[derive(Clone)]
struct Anchor {
    a_rank: usize,
    b_rank: usize,
    a_idx: usize,
    b_idx: usize,
    score: f32,
    /// 链化权（min_chars）：edge/soft 取两侧块字符数较小者，verbatim 取逐字长度。
    weight: usize,
    kind: AnchorKind,
    verbatim_chars: usize,
}

/// 链化入口：残差边 ∪ 软种子 ∪ verbatim 满分锚点 → 每文档对的连续对齐区段。
/// comparable 按 (doc, 稠密 rank) 分组有序（compare_service 的构建口径）。
pub fn chain(
    comparable: &[CmpChunk],
    edges: &[ScoredEdge],
    soft_seeds: &[ScoredEdge],
    verbatim: &[VerbatimSeed],
) -> Vec<AlignedSegment> {
    // 每文档：按 comparable 顺序（=稠密 rank 序）收集其块下标 → rank_of + doc_chunks + prefix。
    let mut rank_of = vec![0usize; comparable.len()];
    let mut doc_chunks: HashMap<usize, Vec<usize>> = HashMap::new();
    let mut id_to_idx: HashMap<&str, usize> = HashMap::new();
    for (idx, c) in comparable.iter().enumerate() {
        let v = doc_chunks.entry(c.doc).or_default();
        rank_of[idx] = v.len();
        v.push(idx);
        id_to_idx.insert(c.id.as_str(), idx);
    }
    // 每文档字符前缀和：prefix[doc][r] = ranks [0,r) 字符和，供 O(1) 区间总字符和。
    let mut prefix: HashMap<usize, Vec<u64>> = HashMap::new();
    for (&doc, idxs) in &doc_chunks {
        let mut p = Vec::with_capacity(idxs.len() + 1);
        let mut acc = 0u64;
        p.push(0);
        for &i in idxs {
            acc += comparable[i].char_count as u64;
            p.push(acc);
        }
        prefix.insert(doc, p);
    }

    // 锚点去重：同一 (a_idx,b_idx) 只留一条——verbatim 优先（携逐字字数），score/weight 取大。
    let mut merged: HashMap<(usize, usize), Anchor> = HashMap::new();
    let mut add = |i: usize, j: usize, score: f32, kind: AnchorKind, vchars: usize| {
        let (da, db) = (comparable[i].doc, comparable[j].doc);
        if da == db {
            return; // 只链跨文档锚点（同文档内相似不成区段）
        }
        // 规范化：低序文档为 a 侧、高序为 b 侧。
        let (ai, bi) = if da < db { (i, j) } else { (j, i) };
        let weight = if kind == AnchorKind::Verbatim {
            vchars
        } else {
            comparable[ai].char_count.min(comparable[bi].char_count)
        };
        let key = (ai, bi);
        merged
            .entry(key)
            .and_modify(|e| {
                let promote_v = kind == AnchorKind::Verbatim || e.kind == AnchorKind::Verbatim;
                if promote_v {
                    e.kind = AnchorKind::Verbatim;
                    e.verbatim_chars = e.verbatim_chars.max(vchars);
                }
                e.score = e.score.max(score);
                e.weight = e.weight.max(weight);
            })
            .or_insert(Anchor {
                a_rank: rank_of[ai],
                b_rank: rank_of[bi],
                a_idx: ai,
                b_idx: bi,
                score,
                weight,
                kind,
                verbatim_chars: if kind == AnchorKind::Verbatim { vchars } else { 0 },
            });
    };
    for e in edges {
        add(e.a as usize, e.b as usize, e.parts.final_score, AnchorKind::Edge, 0);
    }
    for e in soft_seeds {
        add(e.a as usize, e.b as usize, e.parts.final_score, AnchorKind::Soft, 0);
    }
    for v in verbatim {
        if let (Some(&i), Some(&j)) =
            (id_to_idx.get(v.a_chunk_id.as_str()), id_to_idx.get(v.b_chunk_id.as_str()))
        {
            add(i, j, 1.0, AnchorKind::Verbatim, v.char_len);
        }
    }

    // 按文档对分桶（BTreeMap 保证确定性输出序）。
    let mut groups: BTreeMap<(usize, usize), Vec<Anchor>> = BTreeMap::new();
    for a in merged.into_values() {
        let key = (comparable[a.a_idx].doc, comparable[a.b_idx].doc);
        groups.entry(key).or_default().push(a);
    }

    let mut out = Vec::new();
    for ((doc_a, doc_b), anchors) in groups {
        let a_pref = &prefix[&doc_a];
        let b_pref = &prefix[&doc_b];
        let a_chunks = &doc_chunks[&doc_a];
        let b_chunks = &doc_chunks[&doc_b];
        out.extend(extract_pair_segments(
            doc_a, doc_b, anchors, comparable, a_pref, b_pref, a_chunks, b_chunks,
        ));
    }
    out
}

/// 单文档对内迭代抽链：每轮稀疏 DP 取最优合格链、剔除已用锚点，直到无合格链。
#[allow(clippy::too_many_arguments)]
fn extract_pair_segments(
    doc_a: usize,
    doc_b: usize,
    mut anchors: Vec<Anchor>,
    comparable: &[CmpChunk],
    a_prefix: &[u64],
    b_prefix: &[u64],
    a_chunks: &[usize],
    b_chunks: &[usize],
) -> Vec<AlignedSegment> {
    let mut out = Vec::new();
    while anchors.len() >= MIN_ANCHORS {
        // (a_rank,b_rank) 升序：DP 只回看前驱，共线要求两侧 rank 严格递增。
        anchors.sort_by_key(|a| (a.a_rank, a.b_rank));
        let n = anchors.len();
        let mut dp = vec![0f32; n];
        let mut parent = vec![usize::MAX; n];
        let mut len = vec![1usize; n];
        let mut acov = vec![0u64; n]; // a 侧覆盖字符累计（MIN_CHARS 门禁用）
        for i in 0..n {
            let ai = &anchors[i];
            let self_gain = ai.score * ai.weight as f32;
            let a_char = comparable[ai.a_idx].char_count as u64;
            dp[i] = self_gain;
            len[i] = 1;
            acov[i] = a_char;
            let lo = i.saturating_sub(LOOKBACK);
            for j in lo..i {
                let aj = &anchors[j];
                // 共线：两侧 rank 严格递增
                if aj.a_rank >= ai.a_rank || aj.b_rank >= ai.b_rank {
                    continue;
                }
                let da = ai.a_rank - aj.a_rank;
                let db = ai.b_rank - aj.b_rank;
                if da > MAX_GAP_CHUNKS || db > MAX_GAP_CHUNKS {
                    continue; // 任一侧 gap 过大 → 断链
                }
                let cost = GAP_LAMBDA * (da as f32 - db as f32).abs()
                    + GAP_MU * da.max(db) as f32;
                let cand = dp[j] + self_gain - cost;
                if cand > dp[i] + EPS {
                    dp[i] = cand;
                    parent[i] = j;
                    len[i] = len[j] + 1;
                    acov[i] = acov[j] + a_char;
                }
            }
        }
        // 取最优合格链尾（锚点数与覆盖字符均达标；同分取首个 = 更靠前）。
        let mut best_i: Option<usize> = None;
        let mut best_dp = f32::MIN;
        for i in 0..n {
            if len[i] >= MIN_ANCHORS && acov[i] >= MIN_CHARS as u64 && dp[i] > best_dp + EPS {
                best_dp = dp[i];
                best_i = Some(i);
            }
        }
        let Some(end) = best_i else { break };
        // 回溯链
        let mut chain_idx = Vec::with_capacity(len[end]);
        let mut cur = end;
        while cur != usize::MAX {
            chain_idx.push(cur);
            cur = parent[cur];
        }
        chain_idx.reverse();

        out.push(build_segment(
            doc_a, doc_b, &chain_idx, &anchors, comparable, a_prefix, b_prefix, a_chunks, b_chunks,
        ));
        // 剔除已用锚点，迭代
        let used: HashSet<usize> = chain_idx.into_iter().collect();
        anchors = anchors
            .into_iter()
            .enumerate()
            .filter(|(k, _)| !used.contains(k))
            .map(|(_, a)| a)
            .collect();
    }
    out
}

/// 链 → AlignedSegment：区间端点、覆盖率、页码/章节范围、逐字字数、锚点明细。
/// 链已按 a_rank 严格递增（两侧共线）→ 首锚 = 两侧区间起点，末锚 = 两侧区间终点。
#[allow(clippy::too_many_arguments)]
fn build_segment(
    doc_a: usize,
    doc_b: usize,
    chain_idx: &[usize],
    anchors: &[Anchor],
    comparable: &[CmpChunk],
    a_prefix: &[u64],
    b_prefix: &[u64],
    a_chunks: &[usize],
    b_chunks: &[usize],
) -> AlignedSegment {
    let first = &anchors[chain_idx[0]];
    let last = &anchors[chain_idx[chain_idx.len() - 1]];
    let (a_start, a_end) = (first.a_rank, last.a_rank);
    let (b_start, b_end) = (first.b_rank, last.b_rank);

    let mut a_covered = 0u64;
    let mut b_covered = 0u64;
    let mut verbatim_chars = 0usize;
    let mut score_sum = 0f32;
    let mut a_page_lo: Option<u32> = None;
    let mut a_page_hi: Option<u32> = None;
    let mut b_page_lo: Option<u32> = None;
    let mut b_page_hi: Option<u32> = None;
    let mut anchors_out = Vec::with_capacity(chain_idx.len());
    for &k in chain_idx {
        let an = &anchors[k];
        let ca = &comparable[an.a_idx];
        let cb = &comparable[an.b_idx];
        a_covered += ca.char_count as u64;
        b_covered += cb.char_count as u64;
        verbatim_chars += an.verbatim_chars;
        score_sum += an.score;
        merge_page(&mut a_page_lo, &mut a_page_hi, ca.page);
        merge_page(&mut b_page_lo, &mut b_page_hi, cb.page);
        anchors_out.push(SegmentAnchor {
            a_chunk_id: ca.id.clone(),
            b_chunk_id: cb.id.clone(),
            kind: an.kind,
            score: an.score,
        });
    }
    let a_span = a_prefix[a_end + 1] - a_prefix[a_start];
    let b_span = b_prefix[b_end + 1] - b_prefix[b_start];
    let a_coverage = coverage(a_covered, a_span);
    let b_coverage = coverage(b_covered, b_span);

    // 相邻锚点之间的 gap：两侧各取 rank 严格介于两锚点之间的未命中 chunk（W4-3 细化定位）。
    // 链按 a_rank 严格递增、两侧共线 → gap rank 区间恒有效。两侧全空的 gap 不产出。
    let mut gaps = Vec::new();
    for w in chain_idx.windows(2) {
        let (lo, hi) = (&anchors[w[0]], &anchors[w[1]]);
        let a_ids: Vec<String> = (lo.a_rank + 1..hi.a_rank)
            .map(|r| comparable[a_chunks[r]].id.clone())
            .collect();
        let b_ids: Vec<String> = (lo.b_rank + 1..hi.b_rank)
            .map(|r| comparable[b_chunks[r]].id.clone())
            .collect();
        if a_ids.is_empty() && b_ids.is_empty() {
            continue;
        }
        gaps.push(GapPair { a_chunk_ids: a_ids, b_chunk_ids: b_ids });
    }

    AlignedSegment {
        doc_a,
        doc_b,
        a_start_order: a_start,
        a_end_order: a_end,
        b_start_order: b_start,
        b_end_order: b_end,
        a_start_chunk_id: comparable[first.a_idx].id.clone(),
        a_end_chunk_id: comparable[last.a_idx].id.clone(),
        b_start_chunk_id: comparable[first.b_idx].id.clone(),
        b_end_chunk_id: comparable[last.b_idx].id.clone(),
        anchor_count: chain_idx.len(),
        verbatim_chars,
        a_covered_chars: a_covered as usize,
        b_covered_chars: b_covered as usize,
        a_coverage,
        b_coverage,
        avg_score: score_sum / chain_idx.len() as f32,
        a_section_path: section_path_of(&comparable[first.a_idx]),
        b_section_path: section_path_of(&comparable[first.b_idx]),
        a_page_start: a_page_lo,
        a_page_end: a_page_hi,
        b_page_start: b_page_lo,
        b_page_end: b_page_hi,
        anchors: anchors_out,
        gaps,
        a_span: a_span as usize,
        b_span: b_span as usize,
    }
}

fn coverage(covered: u64, span: u64) -> f32 {
    if span == 0 {
        0.0
    } else {
        (covered as f32 / span as f32).min(1.0)
    }
}

fn merge_page(lo: &mut Option<u32>, hi: &mut Option<u32>, page: Option<u32>) {
    if let Some(p) = page {
        *lo = Some(lo.map_or(p, |x| x.min(p)));
        *hi = Some(hi.map_or(p, |x| x.max(p)));
    }
}

fn section_path_of(c: &CmpChunk) -> Option<String> {
    if c.section_path.is_empty() {
        None
    } else {
        Some(c.section_path.join(" › "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::scoring::ScoreParts;
    use std::collections::HashSet;

    /// 构造一个仅含链化所需字段的 CmpChunk（doc/id/char_count/page/section_path）。
    fn chunk(doc: usize, id: &str, chars: usize) -> CmpChunk {
        CmpChunk {
            id: id.into(),
            doc,
            rel_pos: 0.0,
            page: None,
            text: String::new(),
            exact_hash: String::new(),
            normalized_hash: String::new(),
            section_path: vec![],
            section_kind: "other".into(),
            is_template: false,
            is_table_row: false,
            char_count: chars,
            tokens: vec![],
            ngrams: HashSet::new(),
            minhash: vec![],
            entities: vec![],
            tfidf: Default::default(),
            tender_coverage: 0.0,
            boiler_fraction: 0.0,
        }
    }

    fn edge(a: usize, b: usize, score: f32) -> ScoredEdge {
        ScoredEdge {
            a: a as u32,
            b: b as u32,
            parts: ScoreParts {
                lexical: score,
                char_ngram: score,
                entity: None,
                structure: None,
                order: 1.0,
                semantic: None,
                final_score: score,
            },
        }
    }

    /// 两份文档各 n_a / n_b 块（每块 chars 字符），comparable 按 doc 分组有序。
    fn two_docs(n_a: usize, n_b: usize, chars: usize) -> Vec<CmpChunk> {
        let mut v = Vec::new();
        for r in 0..n_a {
            v.push(chunk(0, &format!("a{r}"), chars));
        }
        for r in 0..n_b {
            v.push(chunk(1, &format!("b{r}"), chars));
        }
        v
    }

    // comparable 下标：doc0 的 rank r == r；doc1 的 rank r == n_a + r。
    fn a_idx(r: usize) -> usize {
        r
    }
    fn b_idx(n_a: usize, r: usize) -> usize {
        n_a + r
    }

    #[test]
    fn consecutive_block_one_segment_isolated_hits_dropped() {
        // doc0/doc1 各 60 块（每块 40 字）。连续 10 块（rank 0..9 对 0..9）雷同 → 应成 1 条 ≥10 锚点区段；
        // 两个孤立命中（rank 30↔30、rank 50↔50，彼此及与块均 >8 断链）各 1 锚点 → 不成段。
        let n = 60;
        let comparable = two_docs(n, n, 40);
        let mut edges = Vec::new();
        for r in 0..10 {
            edges.push(edge(a_idx(r), b_idx(n, r), 0.9));
        }
        edges.push(edge(a_idx(30), b_idx(n, 30), 0.95)); // 孤立
        edges.push(edge(a_idx(50), b_idx(n, 50), 0.95)); // 孤立
        let segs = chain(&comparable, &edges, &[], &[]);
        assert_eq!(segs.len(), 1, "只应得连续块 1 条区段，孤立命中不成段");
        let s = &segs[0];
        assert!(s.anchor_count >= 10, "连续块锚点数应 ≥10，实际 {}", s.anchor_count);
        assert_eq!((s.doc_a, s.doc_b), (0, 1));
        assert_eq!((s.a_start_order, s.a_end_order), (0, 9));
        assert_eq!((s.b_start_order, s.b_end_order), (0, 9));
        assert_eq!(s.a_start_chunk_id, "a0");
        assert_eq!(s.a_end_chunk_id, "a9");
        assert_eq!(s.b_start_chunk_id, "b0");
    }

    #[test]
    fn coverage_matches_hand_computation() {
        // 区间 rank 0..=11（12 块 ×50 字 = 600 字总跨），命中 10 块（rank 0..=9 + 11，缺 rank 10）。
        // a_covered = 11 块 ×50 = 550？——为可控，命中 rank {0..=9,11} 共 11 块 → 覆盖 550/600。
        let n = 20;
        let comparable = two_docs(n, n, 50);
        let mut edges = Vec::new();
        for r in 0..10 {
            edges.push(edge(a_idx(r), b_idx(n, r), 0.8));
        }
        edges.push(edge(a_idx(11), b_idx(n, 11), 0.8)); // 与 rank9 gap=2 ≤8，仍连；跳过 rank10
        let segs = chain(&comparable, &edges, &[], &[]);
        assert_eq!(segs.len(), 1);
        let s = &segs[0];
        assert_eq!((s.a_start_order, s.a_end_order), (0, 11));
        assert_eq!(s.anchor_count, 11);
        assert_eq!(s.a_covered_chars, 11 * 50);
        // 区间 12 块 ×50 = 600；覆盖 550 → 0.9166..
        let expect = 550.0 / 600.0;
        assert!((s.a_coverage - expect).abs() < 0.01, "coverage={} 期望≈{}", s.a_coverage, expect);
        assert!((s.b_coverage - expect).abs() < 0.01);
    }

    #[test]
    fn gaps_capture_unmatched_chunks_between_anchors() {
        // 锚点在 rank {0..=9, 11}（跳过 rank10）→ 一条 gap，两侧各含未命中的 rank10 块（a10/b10）。
        // 两侧全空的相邻锚点（rank r→r+1）不产 gap。
        let n = 20;
        let comparable = two_docs(n, n, 50);
        let mut edges = Vec::new();
        for r in 0..10 {
            edges.push(edge(a_idx(r), b_idx(n, r), 0.8));
        }
        edges.push(edge(a_idx(11), b_idx(n, 11), 0.8));
        let segs = chain(&comparable, &edges, &[], &[]);
        assert_eq!(segs.len(), 1);
        let s = &segs[0];
        assert_eq!(s.gaps.len(), 1, "仅 rank9↔rank11 之间一条非空 gap");
        assert_eq!(s.gaps[0].a_chunk_ids, vec!["a10".to_string()]);
        assert_eq!(s.gaps[0].b_chunk_ids, vec!["b10".to_string()]);
        // a_span 用于覆盖率回填分母：12 块 ×50 = 600。
        assert_eq!(s.a_span, 600);
        assert_eq!(s.b_span, 600);
    }

    #[test]
    fn constant_shift_stays_single_chain() {
        // B 比 A 整体后移 5 段：rank r(doc0) ↔ rank r+5(doc1)，Δa=Δb=1（共线按相对序）→ 单链。
        let n = 40;
        let comparable = two_docs(n, n, 30);
        let mut edges = Vec::new();
        for r in 0..10 {
            edges.push(edge(a_idx(r), b_idx(n, r + 5), 0.85));
        }
        let segs = chain(&comparable, &edges, &[], &[]);
        assert_eq!(segs.len(), 1, "整体平移仍应成单链");
        let s = &segs[0];
        assert_eq!(s.anchor_count, 10);
        assert_eq!((s.a_start_order, s.a_end_order), (0, 9));
        assert_eq!((s.b_start_order, s.b_end_order), (5, 14));
    }

    #[test]
    fn verbatim_anchor_kind_and_chars_accumulate() {
        // 5 块连续边 + 其中两块另有 verbatim 满分锚点 → kind=verbatim、verbatim_chars 累计正确。
        let n = 20;
        let comparable = two_docs(n, n, 40);
        let mut edges = Vec::new();
        for r in 0..5 {
            edges.push(edge(a_idx(r), b_idx(n, r), 0.7));
        }
        let vseeds = vec![
            VerbatimSeed { a_chunk_id: "a1".into(), b_chunk_id: "b1".into(), char_len: 80 },
            VerbatimSeed { a_chunk_id: "a3".into(), b_chunk_id: "b3".into(), char_len: 120 },
        ];
        let segs = chain(&comparable, &edges, &[], &vseeds);
        assert_eq!(segs.len(), 1);
        let s = &segs[0];
        assert_eq!(s.verbatim_chars, 200, "两条 verbatim 应累计 80+120");
        let vcount = s.anchors.iter().filter(|a| a.kind == AnchorKind::Verbatim).count();
        assert_eq!(vcount, 2, "两个块应标为 verbatim 锚点");
        // verbatim 锚点 score 提升为 1.0
        for a in &s.anchors {
            if a.kind == AnchorKind::Verbatim {
                assert_eq!(a.score, 1.0);
            }
        }
    }

    #[test]
    fn deterministic_across_runs() {
        let n = 60;
        let comparable = two_docs(n, n, 40);
        let mut edges = Vec::new();
        for r in 0..10 {
            edges.push(edge(a_idx(r), b_idx(n, r), 0.9));
        }
        edges.push(edge(a_idx(30), b_idx(n, 31), 0.8));
        edges.push(edge(a_idx(31), b_idx(n, 32), 0.8));
        let s1 = chain(&comparable, &edges, &[], &[]);
        let s2 = chain(&comparable, &edges, &[], &[]);
        assert_eq!(s1, s2, "同输入两遍应逐字段一致");
    }

    #[test]
    fn soft_seeds_bridge_continuity() {
        // 硬边在 rank5 处缺口（无边），软种子补上该锚点 → 单链含全部 10 锚点（含软种子）。
        let n = 30;
        let comparable = two_docs(n, n, 40);
        let mut edges = Vec::new();
        for r in 0..10 {
            if r == 5 {
                continue; // rank5 无硬边
            }
            edges.push(edge(a_idx(r), b_idx(n, r), 0.85));
        }
        let soft = vec![edge(a_idx(5), b_idx(n, 5), 0.6)];
        let with_soft = chain(&comparable, &edges, &soft, &[]);
        assert_eq!(with_soft.len(), 1);
        let s = &with_soft[0];
        assert_eq!(s.anchor_count, 10, "软种子补缺后应含 10 锚点");
        // 软种子那条锚点 kind=soft
        assert!(s.anchors.iter().any(|a| a.kind == AnchorKind::Soft));
    }

    #[test]
    fn perf_guard_dense_anchors_bounded() {
        // 病态：doc0/doc1 各 400 块全对角命中 + 每 rank 额外几条噪声边 → 回看窗口约束住复杂度、秒级完成。
        let n = 400;
        let comparable = two_docs(n, n, 30);
        let mut edges = Vec::new();
        for r in 0..n {
            edges.push(edge(a_idx(r), b_idx(n, r), 0.85));
            // 噪声：错位小边（Δa≠Δb），链化应偏好对角线
            if r + 3 < n {
                edges.push(edge(a_idx(r), b_idx(n, r + 3), 0.55));
            }
        }
        let t0 = std::time::Instant::now();
        let segs = chain(&comparable, &edges, &[], &[]);
        let dt = t0.elapsed();
        assert!(!segs.is_empty());
        assert!(dt.as_secs() < 5, "稠密锚点应秒级完成，实际 {dt:?}");
    }

    #[test]
    fn empty_inputs_are_safe() {
        let comparable = two_docs(3, 3, 40);
        assert!(chain(&comparable, &[], &[], &[]).is_empty());
        // 单锚点 < MIN_ANCHORS → 不成段
        let edges = vec![edge(a_idx(0), b_idx(3, 0), 0.9)];
        assert!(chain(&comparable, &edges, &[], &[]).is_empty());
    }
}
