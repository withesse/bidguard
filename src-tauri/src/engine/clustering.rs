// 多文档聚类（设计文档 §9.6）：并查集连通分量 + 同文档多成员的 primary 约束。
// 每篇文档默认 1 个 primary（与其他文档平均相似度最高者），其余 duplicate_candidate。
use crate::engine::corpus::CmpChunk;
use crate::engine::scoring::ScoreParts;
use std::collections::HashMap;

pub struct ScoredEdge {
    pub a: u32,
    pub b: u32,
    pub parts: ScoreParts,
}

pub struct RawCluster {
    pub members: Vec<u32>,
    /// chunk 下标 → 角色：primary | duplicate_candidate
    pub roles: HashMap<u32, &'static str>,
    pub avg: f32,
    pub peak: f32,
    pub min_pair: f32,
    pub lex_avg: f32,
    pub sem_avg: Option<f32>,
    /// 组内成员对的边分查找表（供矩阵聚合用）
    pub pair_scores: HashMap<(u32, u32), f32>,
}

struct Dsu(Vec<u32>);

impl Dsu {
    fn new(n: usize) -> Self {
        Dsu((0..n as u32).collect())
    }
    fn find(&mut self, x: u32) -> u32 {
        if self.0[x as usize] != x {
            let r = self.find(self.0[x as usize]);
            self.0[x as usize] = r;
            r
        } else {
            x
        }
    }
    fn union(&mut self, a: u32, b: u32) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra as usize] = rb;
        }
    }
}

/// 低内聚拆分参数（§9.6 步骤 3「cluster 清洗与拆分」）：
/// 并查集按传递性合并，链式过桥（A≈B≈C 但 A≉C）会串出大混合组。
/// 边密度（实际边数 / 完全图边数）低于阈值时迭代移除最弱边，断开即递归拆分。
const SPLIT_MIN_MEMBERS: usize = 4;
const SPLIT_DENSITY: f32 = 0.55;

/// 阈值过滤后的边 → 连通分量 → 低内聚拆分 → 跨 ≥2 文档的 cluster（含角色与统计）。
pub fn cluster(chunks: &[CmpChunk], edges: &[ScoredEdge], threshold: f32) -> Vec<RawCluster> {
    let mut dsu = Dsu::new(chunks.len());
    let kept: Vec<&ScoredEdge> = edges
        .iter()
        .filter(|e| e.parts.final_score >= threshold)
        .collect();
    for e in &kept {
        dsu.union(e.a, e.b);
    }

    let mut comp_edges: HashMap<u32, Vec<&ScoredEdge>> = HashMap::new();
    for e in &kept {
        comp_edges.entry(dsu.find(e.a)).or_default().push(e);
    }

    let mut out = Vec::new();
    for (_, es) in comp_edges {
        for group in cohesive_split(es) {
            if let Some(c) = build_raw(chunks, &group) {
                out.push(c);
            }
        }
    }
    out.sort_by(|a, b| b.avg.partial_cmp(&a.avg).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// 迭代移除最弱边直到密度达标或组够小；移除导致断开时递归处理子分量。
/// 每轮删一条边、边集严格缩小 → 必然终止。
fn cohesive_split(mut es: Vec<&ScoredEdge>) -> Vec<Vec<&ScoredEdge>> {
    loop {
        let mems: std::collections::HashSet<u32> = es.iter().flat_map(|e| [e.a, e.b]).collect();
        let n = mems.len();
        if n < SPLIT_MIN_MEMBERS {
            return vec![es];
        }
        let density = es.len() as f32 / (n * (n - 1) / 2) as f32;
        if density >= SPLIT_DENSITY {
            return vec![es];
        }
        let weakest = es
            .iter()
            .enumerate()
            .min_by(|a, b| {
                a.1.parts
                    .final_score
                    .partial_cmp(&b.1.parts.final_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
            .expect("成员数达标时边集必非空");
        es.remove(weakest);
        if es.is_empty() {
            return Vec::new();
        }
        let groups = group_by_component(&es);
        if groups.len() > 1 {
            return groups.into_iter().flat_map(cohesive_split).collect();
        }
        // 未断开 → 继续削弱边
    }
}

/// 把边集按连通性分组（局部并查集，成员 id 重映射为紧凑下标）。
fn group_by_component<'a>(es: &[&'a ScoredEdge]) -> Vec<Vec<&'a ScoredEdge>> {
    let ids: Vec<u32> = {
        let mut v: Vec<u32> = es.iter().flat_map(|e| [e.a, e.b]).collect();
        v.sort_unstable();
        v.dedup();
        v
    };
    let idx_of: HashMap<u32, u32> =
        ids.iter().enumerate().map(|(i, &m)| (m, i as u32)).collect();
    let mut dsu = Dsu::new(ids.len());
    for e in es {
        dsu.union(idx_of[&e.a], idx_of[&e.b]);
    }
    let mut groups: HashMap<u32, Vec<&ScoredEdge>> = HashMap::new();
    for e in es {
        groups.entry(dsu.find(idx_of[&e.a])).or_default().push(e);
    }
    groups.into_values().collect()
}

/// 一组内聚边 → RawCluster（跨 ≥2 文档才成组；含角色分配与统计）。
fn build_raw(chunks: &[CmpChunk], es: &[&ScoredEdge]) -> Option<RawCluster> {
    let mut members: Vec<u32> = Vec::new();
    for e in es {
        if !members.contains(&e.a) {
            members.push(e.a);
        }
        if !members.contains(&e.b) {
            members.push(e.b);
        }
    }
    members.sort_unstable();

    let mut docs: Vec<usize> = members.iter().map(|&m| chunks[m as usize].doc).collect();
    docs.sort_unstable();
    docs.dedup();
    if docs.len() < 2 {
        return None; // 仅同文档内相似不算跨文档雷同
    }

    // 统计
    let mut sum = 0.0f32;
    let mut peak = 0.0f32;
    let mut min_pair = f32::MAX;
    let mut lex_sum = 0.0f32;
    let mut sem_sum = 0.0f32;
    let mut sem_n = 0u32;
    let mut pair_scores: HashMap<(u32, u32), f32> = HashMap::new();
    for e in es {
        let s = e.parts.final_score;
        sum += s;
        lex_sum += e.parts.lexical;
        if let Some(x) = e.parts.semantic {
            sem_sum += x;
            sem_n += 1;
        }
        peak = peak.max(s);
        min_pair = min_pair.min(s);
        pair_scores.insert((e.a.min(e.b), e.a.max(e.b)), s);
    }
    let n = es.len() as f32;

    // 角色：每文档取「与其他文档成员的边分之和」最高者为 primary
    let mut strength: HashMap<u32, f32> = HashMap::new();
    for e in es {
        if chunks[e.a as usize].doc != chunks[e.b as usize].doc {
            *strength.entry(e.a).or_insert(0.0) += e.parts.final_score;
            *strength.entry(e.b).or_insert(0.0) += e.parts.final_score;
        }
    }
    let mut best_of_doc: HashMap<usize, (u32, f32)> = HashMap::new();
    for &m in &members {
        let s = strength.get(&m).copied().unwrap_or(0.0);
        let d = chunks[m as usize].doc;
        let cur = best_of_doc.entry(d).or_insert((m, s));
        if s > cur.1 {
            *cur = (m, s);
        }
    }
    let primaries: Vec<u32> = best_of_doc.values().map(|(m, _)| *m).collect();
    let roles: HashMap<u32, &'static str> = members
        .iter()
        .map(|&m| {
            (m, if primaries.contains(&m) { "primary" } else { "duplicate_candidate" })
        })
        .collect();

    Some(RawCluster {
        members,
        roles,
        avg: if n > 0.0 { sum / n } else { 0.0 },
        peak,
        min_pair: if min_pair.is_finite() { min_pair } else { 0.0 },
        lex_avg: if n > 0.0 { lex_sum / n } else { 0.0 },
        sem_avg: if sem_n > 0 { Some(sem_sum / sem_n as f32) } else { None },
        pair_scores,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::corpus::CmpChunk;
    use std::collections::HashSet;

    fn mk(doc: usize) -> CmpChunk {
        CmpChunk {
            id: uuid::Uuid::new_v4().to_string(),
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
            char_count: 50,
            tokens: vec![],
            ngrams: HashSet::new(),
            minhash: vec![],
            entities: vec![],
            tfidf: Default::default(),
        }
    }

    fn edge(a: u32, b: u32, s: f32) -> ScoredEdge {
        ScoredEdge {
            a,
            b,
            parts: crate::engine::scoring::ScoreParts {
                lexical: s,
                char_ngram: s,
                entity: None,
                structure: None,
                order: 1.0,
                semantic: None,
                final_score: s,
            },
        }
    }

    #[test]
    fn components_roles_and_constraints() {
        // chunks: 0,1 ∈ doc0; 2 ∈ doc1; 3 ∈ doc2; 4,5 ∈ doc3（4-5 仅同文档相似）
        let chunks = vec![mk(0), mk(0), mk(1), mk(2), mk(3), mk(3)];
        let edges = vec![
            edge(0, 2, 0.9),  // doc0-doc1
            edge(2, 3, 0.85), // doc1-doc2
            edge(0, 3, 0.7),  // doc0-doc2（补足密度，免触发低内聚拆分）
            edge(1, 2, 0.6),  // doc0 第二个成员，弱
            edge(4, 5, 0.95), // 同文档 → 该分量只有 doc3，应被丢弃
        ];
        let clusters = cluster(&chunks, &edges, 0.5);
        assert_eq!(clusters.len(), 1, "同文档分量不算雷同条款");
        let c = &clusters[0];
        assert_eq!(c.members, vec![0, 1, 2, 3]);
        // doc0 有两个成员：0 强（0.9+0.7）为 primary，1 为 duplicate_candidate
        assert_eq!(c.roles[&0], "primary");
        assert_eq!(c.roles[&1], "duplicate_candidate");
        assert_eq!(c.roles[&2], "primary");
        assert_eq!(c.roles[&3], "primary");
        assert!(c.peak >= 0.9 && c.min_pair <= 0.6);
    }

    #[test]
    fn chain_component_splits_at_weakest_link() {
        // 链式过桥：A-B(0.82) B-C(0.71) C-D(0.84)，A 与 C/D 毫无边 → 传递性串成一组
        // 密度 3/6=0.5 < 0.55 → 在最弱边 B-C 处断开成两组
        let chunks = vec![mk(0), mk(1), mk(2), mk(3)];
        let edges = vec![edge(0, 1, 0.82), edge(1, 2, 0.71), edge(2, 3, 0.84)];
        let mut clusters = cluster(&chunks, &edges, 0.5);
        clusters.sort_by(|a, b| a.members.cmp(&b.members));
        assert_eq!(clusters.len(), 2, "链应在最弱边断开");
        assert_eq!(clusters[0].members, vec![0, 1]);
        assert_eq!(clusters[1].members, vec![2, 3]);
    }

    #[test]
    fn dense_component_stays_whole() {
        // 5 成员 8 边（密度 0.8）：互相都相似的真雷同组不应被拆
        let chunks = vec![mk(0), mk(1), mk(2), mk(3), mk(4)];
        let mut edges = Vec::new();
        for i in 0..5u32 {
            for j in (i + 1)..5u32 {
                if (i, j) != (0, 4) && (i, j) != (1, 3) {
                    edges.push(edge(i, j, 0.8));
                }
            }
        }
        let clusters = cluster(&chunks, &edges, 0.5);
        assert_eq!(clusters.len(), 1, "高密度组保持完整");
        assert_eq!(clusters[0].members, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn threshold_filters_edges() {
        let chunks = vec![mk(0), mk(1)];
        let clusters = cluster(&chunks, &[edge(0, 1, 0.4)], 0.5);
        assert!(clusters.is_empty(), "低于阈值的边不建组");
    }
}
