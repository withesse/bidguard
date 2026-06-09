// 段落级分析：分段 → 逐对对齐（含字符级 diff） + 跨文档聚类。
use crate::engine::report::{
    Cluster, ClusterSeg, DiffOp, PairDetail, SectionStat, SegMatch, SharedTerm,
};
use crate::engine::similarity::{cosine, tokenize};
use jieba_rs::Jieba;
use similar::{ChangeTag, TextDiff};
use std::collections::{BTreeSet, HashMap};

const MAX_SEGS_PER_DOC: usize = 300; // 控制 O(N²) 规模
const MIN_SEG_CHARS: usize = 10; // 过短段落视为噪声
const TEMPLATE_MATCH: f32 = 0.7; // 命中查重源/通用模板的阈值
const MAX_MATCHES: usize = 40;
const MAX_CLUSTERS: usize = 40;

/// 段落所属标段（用于「比对范围」过滤）。
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Tech,
    Business,
    Other,
}

const BIZ_KW: &[&str] = &[
    "报价", "价格", "费用", "金额", "商务", "资质", "营业执照", "法定代表人", "法人",
    "投标函", "投标保证金", "财务", "审计", "纳税", "社保", "承诺函", "授权委托", "信誉",
    "业绩", "注册资本", "报价表",
];
const TECH_KW: &[&str] = &[
    "技术", "方案", "架构", "系统", "设计", "实施", "部署", "接口", "性能", "安全", "数据",
    "功能", "平台", "集成", "运维", "网络", "服务器", "算法", "模块", "容灾",
];

/// 关键词启发式：判段落属技术标 / 商务标 / 其他。
pub fn classify(text: &str) -> Section {
    let b = BIZ_KW.iter().filter(|k| text.contains(**k)).count();
    let t = TECH_KW.iter().filter(|k| text.contains(**k)).count();
    if t > b {
        Section::Tech
    } else if b > t {
        Section::Business
    } else {
        Section::Other
    }
}

pub struct Segment {
    pub text: String,
    pub tokens: Vec<String>,
    pub section: Section,
}

/// 按段落 / 句切分并分词，过滤过短噪声段；命中查重源模板的段落视为样板剔除。
pub fn segment(jieba: &Jieba, text: &str, template_tokens: &[Vec<String>]) -> Vec<Segment> {
    let mut segs = Vec::new();
    for piece in text.split(|c| matches!(c, '\n' | '。' | '！' | '？' | '；' | ';')) {
        let t = piece.trim();
        if t.chars().count() < MIN_SEG_CHARS {
            continue;
        }
        let tokens = tokenize(jieba, t);
        if tokens.is_empty() {
            continue;
        }
        // 命中通用模板 / 基准库 → 非可疑样板，跳过（减少误报）
        if template_tokens.iter().any(|tt| cosine(&tokens, tt) >= TEMPLATE_MATCH) {
            continue;
        }
        let section = classify(t);
        segs.push(Segment {
            text: t.to_string(),
            tokens,
            section,
        });
        if segs.len() >= MAX_SEGS_PER_DOC {
            break;
        }
    }
    segs
}

/// 字符级 diff，合并连续同类为 run。op: eq | ins(B 有) | del(A 有)。
fn char_diff(a: &str, b: &str) -> Vec<DiffOp> {
    let diff = TextDiff::from_chars(a, b);
    let mut ops: Vec<DiffOp> = Vec::new();
    for ch in diff.iter_all_changes() {
        let op = match ch.tag() {
            ChangeTag::Equal => "eq",
            ChangeTag::Insert => "ins",
            ChangeTag::Delete => "del",
        };
        let val = ch.value();
        if let Some(last) = ops.last_mut() {
            if last.op == op {
                last.text.push_str(val);
                continue;
            }
        }
        ops.push(DiffOp {
            op: op.to_string(),
            text: val.to_string(),
        });
    }
    ops
}

/// 对齐两份文档段落：每个 A 段在 B 中找最相似段，超阈值即记一处匹配（含字符级 diff）。
pub fn align_pair(
    ai: usize,
    bi: usize,
    score: f32,
    a: &[Segment],
    b: &[Segment],
    match_min: f32,
) -> PairDetail {
    let mut matches: Vec<SegMatch> = Vec::new();
    for sa in a {
        let mut best = 0.0f32;
        let mut bj = usize::MAX;
        for (j, sb) in b.iter().enumerate() {
            let s = cosine(&sa.tokens, &sb.tokens);
            if s > best {
                best = s;
                bj = j;
            }
        }
        if bj != usize::MAX && best >= match_min {
            matches.push(SegMatch {
                text_a: sa.text.clone(),
                text_b: b[bj].text.clone(),
                score: best,
                diff: char_diff(&sa.text, &b[bj].text),
            });
        }
    }
    matches.sort_by(|x, y| y.score.partial_cmp(&x.score).unwrap_or(std::cmp::Ordering::Equal));
    matches.truncate(MAX_MATCHES);
    PairDetail {
        a: ai,
        b: bi,
        score,
        matches,
    }
}

/// 跨文档聚类：贪心地把不同文档间相似度 ≥ 阈值的段落聚为一组（需 ≥2 份文档）。
pub fn cluster_segments(per_doc: &[Vec<Segment>], cluster_min: f32) -> Vec<Cluster> {
    struct Flat<'a> {
        doc: usize,
        seg: &'a Segment,
    }
    let mut all: Vec<Flat> = Vec::new();
    for (d, segs) in per_doc.iter().enumerate() {
        for seg in segs {
            all.push(Flat { doc: d, seg });
        }
    }
    let n = all.len();
    let mut used = vec![false; n];
    let mut clusters: Vec<Cluster> = Vec::new();

    for i in 0..n {
        if used[i] {
            continue;
        }
        let mut members = vec![i];
        for j in (i + 1)..n {
            if used[j] || all[j].doc == all[i].doc {
                continue;
            }
            if cosine(&all[i].seg.tokens, &all[j].seg.tokens) >= cluster_min {
                members.push(j);
            }
        }
        let docs: BTreeSet<usize> = members.iter().map(|&m| all[m].doc).collect();
        if docs.len() < 2 {
            continue; // 仅同一文档内相似不算雷同条款
        }
        for &m in &members {
            used[m] = true;
        }
        let mut sum = 0.0f32;
        let mut peak = 0.0f32;
        let mut cnt = 0u32;
        for x in 0..members.len() {
            for y in (x + 1)..members.len() {
                let s = cosine(&all[members[x]].seg.tokens, &all[members[y]].seg.tokens);
                sum += s;
                if s > peak {
                    peak = s;
                }
                cnt += 1;
            }
        }
        let avg = if cnt > 0 { sum / cnt as f32 } else { 0.0 };
        let segments: Vec<ClusterSeg> = members
            .iter()
            .map(|&m| ClusterSeg {
                doc: all[m].doc,
                text: all[m].seg.text.clone(),
            })
            .collect();
        clusters.push(Cluster {
            avg_score: avg,
            peak,
            docs: docs.into_iter().collect(),
            segments,
        });
    }
    clusters.sort_by(|x, y| y.avg_score.partial_cmp(&x.avg_score).unwrap_or(std::cmp::Ordering::Equal));
    clusters.truncate(MAX_CLUSTERS);
    clusters
}

/// Section → 字符串标识（供前端热力图使用）。
pub fn section_str(s: Section) -> &'static str {
    match s {
        Section::Tech => "tech",
        Section::Business => "business",
        Section::Other => "other",
    }
}

/// 共有特征词：≥4 字、被 ≥2 份标书共用的罕见词（疑似同源 / 共用笔误）。
pub fn shared_terms(scoped: &[Vec<Segment>]) -> Vec<SharedTerm> {
    let mut map: HashMap<String, BTreeSet<usize>> = HashMap::new();
    for (d, segs) in scoped.iter().enumerate() {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for s in segs {
            for tok in &s.tokens {
                if tok.chars().count() >= 4 && seen.insert(tok.clone()) {
                    map.entry(tok.clone()).or_default().insert(d);
                }
            }
        }
    }
    let mut out: Vec<SharedTerm> = map
        .into_iter()
        .filter(|(_, docs)| docs.len() >= 2)
        .map(|(term, docs)| SharedTerm {
            term,
            docs: docs.into_iter().collect(),
        })
        .collect();
    out.sort_by(|a, b| {
        b.docs
            .len()
            .cmp(&a.docs.len())
            .then(b.term.chars().count().cmp(&a.term.chars().count()))
    });
    out.truncate(30);
    out
}

/// 章节热力：每文档每标段的最大跨文档相似度 + 命中片段数。
pub fn section_stats(scoped: &[Vec<Segment>]) -> Vec<SectionStat> {
    let mut stats = Vec::new();
    for (d, segs) in scoped.iter().enumerate() {
        for sect in [Section::Tech, Section::Business, Section::Other] {
            let mine: Vec<&Segment> = segs.iter().filter(|s| s.section == sect).collect();
            if mine.is_empty() {
                continue;
            }
            let mut intensity = 0.0f32;
            let mut matches = 0u32;
            for s in &mine {
                let mut best = 0.0f32;
                for (od, osegs) in scoped.iter().enumerate() {
                    if od == d {
                        continue;
                    }
                    for os in osegs {
                        let c = cosine(&s.tokens, &os.tokens);
                        if c > best {
                            best = c;
                        }
                    }
                }
                if best > intensity {
                    intensity = best;
                }
                if best >= 0.5 {
                    matches += 1;
                }
            }
            stats.push(SectionStat {
                doc: d,
                section: section_str(sect).into(),
                intensity,
                matches,
            });
        }
    }
    stats
}
