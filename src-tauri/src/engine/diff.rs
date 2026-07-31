// 分级 diff（设计文档 §9.8）与八类差异分类（§9.5/§9.7 的阈值带）。
// 短文本字符级 / 中段落词级 / 长段落句级（句级中相邻的删改对再细化为字符级）。
use crate::engine::features::{char_ngrams, jaccard};
use crate::engine::report::DiffOp;
use jieba_rs::Jieba;
use similar::{ChangeTag, TextDiff};
use std::collections::HashSet;

const CHAR_MAX: usize = 60;
const WORD_MAX: usize = 400;

// —— 区段 gap 带状细化常量集中区（W4-3，M5a）——
/// 句级带状 NW 的额外带宽：band = |la−lb| + GAP_BAND_SLACK，容许锚点间少量句错位/一对多。
pub const GAP_BAND_SLACK: usize = 8;
/// 句配对最低相似（char-ngram Jaccard）：低于此不允许配对（记 ins/del，方向安全偏漏报）。
pub const GAP_SIM_MIN: f32 = 0.4;
/// gap 任一侧字符数超此 → 降级为整段 sentence_diff，防带状 DP 内存/时间峰值。
pub const GAP_MAX_SIDE_CHARS: usize = 4000;

/// 按文本长度选择 diff 粒度。返回 (粒度标识, 操作序列)。
pub fn graded_diff(jieba: &Jieba, a: &str, b: &str) -> (&'static str, Vec<DiffOp>) {
    let len = a.chars().count().max(b.chars().count());
    if len <= CHAR_MAX {
        ("char", char_diff(a, b))
    } else if len <= WORD_MAX {
        ("word", word_diff(jieba, a, b))
    } else {
        ("sentence", sentence_diff(a, b))
    }
}

fn push_op(ops: &mut Vec<DiffOp>, op: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    if let Some(last) = ops.last_mut() {
        if last.op == op {
            last.text.push_str(text);
            return;
        }
    }
    ops.push(DiffOp {
        op: op.to_string(),
        text: text.to_string(),
    });
}

fn tag_str(tag: ChangeTag) -> &'static str {
    match tag {
        ChangeTag::Equal => "eq",
        ChangeTag::Insert => "ins",
        ChangeTag::Delete => "del",
    }
}

/// 表格行列对齐 diff（§9.8）：两侧都是表格行时按「 | 」拆单元格逐列比较——
/// 相同单元格整体 eq；不同单元格内做字符级细化；列数不等时多出的列记 ins/del。
/// 分隔符归属：两侧都有该列 → eq，仅一侧有 → 随该列记 del/ins（保证 ops 可还原两侧原文）。
pub fn table_row_diff(a: &str, b: &str) -> Vec<DiffOp> {
    let ca: Vec<&str> = a.split(" | ").collect();
    let cb: Vec<&str> = b.split(" | ").collect();
    let mut ops = Vec::new();
    let n = ca.len().max(cb.len());
    for i in 0..n {
        let (x, y) = (ca.get(i), cb.get(i));
        if i > 0 {
            let sep_op = match (x, y) {
                (Some(_), Some(_)) => "eq",
                (Some(_), None) => "del",
                _ => "ins",
            };
            push_op(&mut ops, sep_op, " | ");
        }
        match (x, y) {
            (Some(x), Some(y)) if x == y => push_op(&mut ops, "eq", x),
            (Some(x), Some(y)) => {
                for op in char_diff(x, y) {
                    push_op(&mut ops, &op.op, &op.text);
                }
            }
            (Some(x), None) => push_op(&mut ops, "del", x),
            (None, Some(y)) => push_op(&mut ops, "ins", y),
            (None, None) => unreachable!("i < n 时至少一侧有该列"),
        }
    }
    ops
}

pub fn char_diff(a: &str, b: &str) -> Vec<DiffOp> {
    let diff = TextDiff::from_chars(a, b);
    let mut ops = Vec::new();
    for ch in diff.iter_all_changes() {
        push_op(&mut ops, tag_str(ch.tag()), ch.value());
    }
    ops
}

fn word_diff(jieba: &Jieba, a: &str, b: &str) -> Vec<DiffOp> {
    let aw = jieba.cut(a, false);
    let bw = jieba.cut(b, false);
    let diff = TextDiff::from_slices(&aw, &bw);
    let mut ops = Vec::new();
    for ch in diff.iter_all_changes() {
        push_op(&mut ops, tag_str(ch.tag()), ch.value());
    }
    ops
}

/// 切句（保留句末标点，保证 ops 拼接可还原原文）。
pub(crate) fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    for c in text.chars() {
        cur.push(c);
        if matches!(c, '。' | '！' | '？' | '；' | ';' | '\n') {
            out.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn sentence_diff(a: &str, b: &str) -> Vec<DiffOp> {
    let sa_owned = split_sentences(a);
    let sb_owned = split_sentences(b);
    let sa: Vec<&str> = sa_owned.iter().map(String::as_str).collect();
    let sb: Vec<&str> = sb_owned.iter().map(String::as_str).collect();
    let diff = TextDiff::from_slices(&sa, &sb);
    // 先收集句级 run，再把相邻「删 N 句 + 增 N 句」细化为字符级
    let mut runs: Vec<(ChangeTag, Vec<&str>)> = Vec::new();
    for ch in diff.iter_all_changes() {
        let v = ch.value();
        if let Some(last) = runs.last_mut() {
            if last.0 == ch.tag() {
                last.1.push(v);
                continue;
            }
        }
        runs.push((ch.tag(), vec![v]));
    }
    let mut ops = Vec::new();
    let mut i = 0;
    while i < runs.len() {
        let (tag, sents) = &runs[i];
        if *tag == ChangeTag::Delete {
            if let Some((ChangeTag::Insert, ins_sents)) = runs.get(i + 1).map(|r| (r.0, &r.1)) {
                // 相邻删改 → 字符级细化（典型的「同句小改」场景）
                for op in char_diff(&sents.concat(), &ins_sents.concat()) {
                    push_op(&mut ops, &op.op, &op.text);
                }
                i += 2;
                continue;
            }
        }
        for s in sents {
            push_op(&mut ops, tag_str(*tag), s);
        }
        i += 1;
    }
    ops
}

// —— 区段内 gap 带状字符级对齐细化（W4-3，M5a）——
// 链化区段的相邻锚点之间存在「未被任何边命中」的 gap 块（洗稿插入句、小改段）。带状句级对齐
// + 字符级细化把这些 gap 变成可高亮的精确证据：区段覆盖率从「锚点覆盖」升级为「细化后真实覆盖」。
// 是 sentence_diff 的推广——sentence_diff 只细化相邻删改 run，无法处理错位/一对多；本层用带宽约束
// 的单调 Needleman-Wunsch 允许跨错位配对。del=A 独有、ins=B 独有、eq=双方相同（与 char_diff 同向）：
// ops 过滤 ins 还原 A 侧、过滤 del 还原 B 侧。

/// 一个 gap 的细化结果：diff 序列 + eq 字符数 + 类型标识。
/// diff_type: "gap-sentence"（带状句级细化）| "gap-degraded"（超长降级整段句 diff）。
#[derive(Debug, Clone)]
pub struct GapRefinement {
    pub diff_type: &'static str,
    pub ops: Vec<DiffOp>,
    pub eq_chars: usize,
}

/// 句级带状 Needleman-Wunsch + 配对句字符级细化。band 约束 |i−j|≤band 的单调对齐：
/// 对角=配对（替换代价 1−sim，仅 sim≥GAP_SIM_MIN 允许，配对句走 graded_diff 细化——短句即
/// char_diff），上=del a[i−1]（整句 A 独有），左=ins b[j−1]（整句 B 独有）。indel 代价 1。
/// 每句 char-ngram 集合只算一次、仅带内计 Jaccard，复杂度 O((la+lb)·band)。
pub fn banded_gap_diff(jieba: &Jieba, a_sents: &[&str], b_sents: &[&str], band: usize) -> Vec<DiffOp> {
    let la = a_sents.len();
    let lb = b_sents.len();
    if la == 0 && lb == 0 {
        return Vec::new();
    }
    let mut ops = Vec::new();
    if la == 0 {
        for s in b_sents {
            push_op(&mut ops, "ins", s);
        }
        return ops;
    }
    if lb == 0 {
        for s in a_sents {
            push_op(&mut ops, "del", s);
        }
        return ops;
    }

    let a_grams: Vec<HashSet<u64>> = a_sents.iter().map(|s| char_ngrams(s)).collect();
    let b_grams: Vec<HashSet<u64>> = b_sents.iter().map(|s| char_ngrams(s)).collect();

    // cost[i][j]=对齐 a[0..i]、b[0..j] 的最小代价（带外恒 INF）；dir[i][j]：0=对角配对 1=del 2=ins。
    let inf = f32::INFINITY;
    let mut cost = vec![vec![inf; lb + 1]; la + 1];
    let mut dir = vec![vec![1u8; lb + 1]; la + 1];
    cost[0][0] = 0.0;
    for i in 1..=la.min(band) {
        cost[i][0] = i as f32;
        dir[i][0] = 1;
    }
    for j in 1..=lb.min(band) {
        cost[0][j] = j as f32;
        dir[0][j] = 2;
    }
    for i in 1..=la {
        let jlo = i.saturating_sub(band).max(1);
        let jhi = (i + band).min(lb);
        for j in jlo..=jhi {
            let mut best = inf;
            let mut bdir = 1u8;
            let diag = cost[i - 1][j - 1];
            if diag.is_finite() {
                let sim = jaccard(&a_grams[i - 1], &b_grams[j - 1]);
                if sim >= GAP_SIM_MIN {
                    let c = diag + (1.0 - sim);
                    if c < best {
                        best = c;
                        bdir = 0;
                    }
                }
            }
            let up = cost[i - 1][j];
            if up.is_finite() && up + 1.0 < best {
                best = up + 1.0;
                bdir = 1;
            }
            let left = cost[i][j - 1];
            if left.is_finite() && left + 1.0 < best {
                best = left + 1.0;
                bdir = 2;
            }
            cost[i][j] = best;
            dir[i][j] = bdir;
        }
    }

    // 回溯 (la,lb) → (0,0)，前序收集动作。
    let mut steps: Vec<(u8, usize, usize)> = Vec::new();
    let (mut i, mut j) = (la, lb);
    while i > 0 || j > 0 {
        let d = dir[i][j];
        steps.push((d, i, j));
        match d {
            0 => {
                i -= 1;
                j -= 1;
            }
            1 => i -= 1,
            _ => j -= 1,
        }
    }
    for (d, i, j) in steps.into_iter().rev() {
        match d {
            0 => {
                for op in graded_diff(jieba, a_sents[i - 1], b_sents[j - 1]).1 {
                    push_op(&mut ops, &op.op, &op.text);
                }
            }
            1 => push_op(&mut ops, "del", a_sents[i - 1]),
            _ => push_op(&mut ops, "ins", b_sents[j - 1]),
        }
    }
    ops
}

/// 单 gap 细化入口：切句 → band=|la−lb|+GAP_BAND_SLACK 的带状对齐；任一侧字符数 >GAP_MAX_SIDE_CHARS
/// 时降级整段 sentence_diff（防带状 DP 峰值）。返回 ops + eq 字符数（供覆盖率回填）。
pub fn refine_gap(jieba: &Jieba, a_text: &str, b_text: &str) -> GapRefinement {
    let too_long =
        a_text.chars().count() > GAP_MAX_SIDE_CHARS || b_text.chars().count() > GAP_MAX_SIDE_CHARS;
    let (diff_type, ops) = if too_long {
        ("gap-degraded", sentence_diff(a_text, b_text))
    } else {
        let a_owned = split_sentences(a_text);
        let b_owned = split_sentences(b_text);
        let a_sents: Vec<&str> = a_owned.iter().map(String::as_str).collect();
        let b_sents: Vec<&str> = b_owned.iter().map(String::as_str).collect();
        let band = a_sents.len().abs_diff(b_sents.len()) + GAP_BAND_SLACK;
        ("gap-sentence", banded_gap_diff(jieba, &a_sents, &b_sents, band))
    };
    let eq_chars = ops.iter().filter(|o| o.op == "eq").map(|o| o.text.chars().count()).sum();
    GapRefinement { diff_type, ops, eq_chars }
}

/// 区段级入口：对一个区段的全部 gap（各已解析为 (A 侧拼接文本, B 侧拼接文本)）逐个细化，
/// 与输入一一对应。调用方按 gap 的 chunk 定位落 segment_diffs、按 Σeq_chars 回填细化后覆盖率。
/// 全空 gap（锚点相邻）由链化侧不产出，本函数收到的 gap 至少一侧非空。
pub fn refine_segment_gaps(jieba: &Jieba, gaps: &[(String, String)]) -> Vec<GapRefinement> {
    gaps.iter().map(|(a, b)| refine_gap(jieba, a, b)).collect()
}

// —— 差异分类（§9.5 阈值带 + §9.7 规则；conflict 由事实冲突检测覆盖）——

pub struct ClusterClass {
    pub cluster_type: &'static str,
    pub severity: &'static str,
}

/// 按组内统计分类。base 模式下的 added/deleted 与事实 conflict 由上层另行覆盖。
pub fn classify_cluster(
    avg: f32,
    min_pair: f32,
    all_same_normalized_hash: bool,
    lex_avg: f32,
    sem_avg: Option<f32>,
) -> ClusterClass {
    if all_same_normalized_hash || min_pair >= 0.95 {
        return ClusterClass { cluster_type: "same", severity: "none" };
    }
    if let Some(sem) = sem_avg {
        if sem >= 0.80 && lex_avg < 0.50 {
            return ClusterClass { cluster_type: "rewrite", severity: "medium" };
        }
    }
    if avg >= 0.85 {
        return ClusterClass { cluster_type: "minor_change", severity: "low" };
    }
    if avg >= 0.70 {
        return ClusterClass { cluster_type: "changed", severity: "medium" };
    }
    ClusterClass { cluster_type: "uncertain", severity: "review" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn join(ops: &[DiffOp], skip: &str) -> String {
        ops.iter().filter(|o| o.op != skip).map(|o| o.text.as_str()).collect()
    }

    #[test]
    fn char_diff_reconstructs_both_sides() {
        let ops = char_diff("甲方应在每月十日前支付", "甲方应在每月十五日前支付");
        assert_eq!(join(&ops, "ins"), "甲方应在每月十日前支付");
        assert_eq!(join(&ops, "del"), "甲方应在每月十五日前支付");
        assert!(ops.iter().any(|o| o.op == "ins" && o.text.contains('五')));
    }

    #[test]
    fn table_row_diff_aligns_by_column() {
        // 仅价格列不同：其余单元格应整体 eq，差异收敛在该列内
        let a = "1 | 核心交换机 | 64000元 | 30天";
        let b = "1 | 核心交换机 | 78000元 | 30天";
        let ops = table_row_diff(a, b);
        assert_eq!(join(&ops, "ins"), a, "eq+del 应还原 A 侧");
        assert_eq!(join(&ops, "del"), b, "eq+ins 应还原 B 侧");
        assert!(
            ops.iter().any(|o| o.op == "eq" && o.text.contains("核心交换机")),
            "相同单元格应整体 eq（相邻 eq 合并后仍完整）：{ops:?}"
        );
        // 差异不应波及交换机列（字符级 diff 限于价格单元格内）
        assert!(ops.iter().filter(|o| o.op != "eq").all(|o| !o.text.contains("交换机")));

        // 完全相同 → 全 eq
        assert!(table_row_diff(a, a).iter().all(|o| o.op == "eq"));

        // 列数不等：B 多出的「备注」列整列 ins，含其前分隔符
        let c = "1 | 核心交换机 | 64000元 | 30天 | 含安装";
        let ops = table_row_diff(a, c);
        assert_eq!(join(&ops, "ins"), a);
        assert_eq!(join(&ops, "del"), c);
        assert!(ops.iter().any(|o| o.op == "ins" && o.text.contains("含安装")));
    }

    #[test]
    fn granularity_selection() {
        let jieba = Jieba::new();
        let short_a = "工期为180个日历日";
        let (g, _) = graded_diff(&jieba, short_a, "工期为90个日历日");
        assert_eq!(g, "char");

        let mid = "系统采用分层解耦的微服务总体架构，".repeat(8);
        let (g, ops) = graded_diff(&jieba, &mid, &mid);
        assert_eq!(g, "word");
        assert!(ops.iter().all(|o| o.op == "eq"));

        let long = "本项目严格遵循国家标准。".repeat(50);
        let (g, _) = graded_diff(&jieba, &long, &long);
        assert_eq!(g, "sentence");
    }

    #[test]
    fn sentence_diff_refines_adjacent_changes() {
        let a = "第一句完全一致。第二句甲方负责施工。第三句也一致。";
        let b = "第一句完全一致。第二句乙方负责施工。第三句也一致。";
        let ops = sentence_diff(a, b);
        assert_eq!(join(&ops, "ins"), a);
        assert_eq!(join(&ops, "del"), b);
        // 中间句应细化出字符级的 甲/乙 替换，而不是整句删整句增
        let del: String = ops.iter().filter(|o| o.op == "del").map(|o| o.text.as_str()).collect();
        assert!(del.chars().count() <= 2, "应只删「甲」级别的小片段，实际删了 {del:?}");
    }

    #[test]
    fn banded_gap_diff_insert_and_small_change() {
        // 验收 (1)：gap 两侧 5 句 vs 6 句——B 中插入 1 新句 + 1 句小改，其余 4 句一致。
        // 期望：新句整句 ins、小改句字符级 del/ins、其余 eq；ops 过滤 ins 还原 A、过滤 del 还原 B。
        let jieba = Jieba::new();
        let a = "第一句完全一致。第二句甲方负责施工。第三句也一致。第四句照旧。第五句结束。";
        let b = "第一句完全一致。这是全新插入的一句。第二句乙方负责施工。第三句也一致。第四句照旧。第五句结束。";
        let refined = refine_gap(&jieba, a, b);
        let ops = &refined.ops;
        assert_eq!(join(ops, "ins"), a, "过滤 ins 应还原 A 侧原文");
        assert_eq!(join(ops, "del"), b, "过滤 del 应还原 B 侧原文");
        // 新句整句 ins（作为一个未配对 B 句出现）
        assert!(
            ops.iter().any(|o| o.op == "ins" && o.text.contains("全新插入")),
            "新句应整句 ins：{ops:?}"
        );
        // 小改句只在「甲/乙」处产生字符级 del/ins，而非整句删改
        let del: String = ops.iter().filter(|o| o.op == "del").map(|o| o.text.as_str()).collect();
        assert!(
            del.chars().count() <= 2 && del.contains('甲'),
            "小改句应只删「甲」级别小片段，实际删 {del:?}"
        );
        assert!(ops.iter().any(|o| o.op == "ins" && o.text == "乙"), "小改句应字符级增「乙」");
        // eq 字符数 = 双方相同部分（应覆盖 4 整句 + 小改句里未变的字）
        assert!(refined.eq_chars > 0);
        assert_eq!(refined.diff_type, "gap-sentence");
    }

    #[test]
    fn banded_gap_diff_reconstructs_with_reordered_pairs() {
        // 未配对（sim<0.4）的不相关句应记 ins/del 而非误配为替换；还原性仍成立。
        let jieba = Jieba::new();
        let a = "施工组织设计方案概述。质量保证体系说明。";
        let b = "完全无关的另一段落内容。质量保证体系说明。安全生产额外条款。";
        let refined = refine_gap(&jieba, a, b);
        assert_eq!(join(&refined.ops, "ins"), a);
        assert_eq!(join(&refined.ops, "del"), b);
        // 相同句「质量保证体系说明。」应整体 eq
        assert!(refined.ops.iter().any(|o| o.op == "eq" && o.text.contains("质量保证体系")));
    }

    #[test]
    fn banded_gap_diff_pure_insert_and_delete() {
        // 一侧为空的 gap（纯插入/纯删除）：全 ins / 全 del，eq_chars=0。
        let jieba = Jieba::new();
        let only_b = refine_gap(&jieba, "", "乙方新增两句。这是第二句。");
        assert!(only_b.ops.iter().all(|o| o.op == "ins"));
        assert_eq!(only_b.eq_chars, 0);
        let only_a = refine_gap(&jieba, "甲方独有的一句。", "");
        assert!(only_a.ops.iter().all(|o| o.op == "del"));
        assert_eq!(only_a.eq_chars, 0);
    }

    #[test]
    fn banded_gap_diff_bounded_on_pathological_input() {
        // 验收 (2)：两侧各 200 句的病态 gap，带状约束（band=8）下 debug 模式应远 <1s。
        let jieba = Jieba::new();
        let a: String = (0..200).map(|i| format!("第{i}条施工技术要求说明。")).collect();
        let b: String = (0..200).map(|i| format!("第{i}条施工技术要求说明。")).collect();
        let t0 = std::time::Instant::now();
        let refined = refine_gap(&jieba, &a, &b);
        let dt = t0.elapsed();
        assert!(dt.as_secs() < 1, "带状约束应使 200×200 gap 亚秒完成，实际 {dt:?}");
        // 全等 → 全 eq、可还原
        assert_eq!(join(&refined.ops, "ins"), a);
        assert_eq!(join(&refined.ops, "del"), b);
    }

    #[test]
    fn refine_segment_gaps_maps_one_to_one() {
        let jieba = Jieba::new();
        let gaps = vec![
            ("甲方负责。".to_string(), "乙方负责。".to_string()),
            (String::new(), "纯插入一句。".to_string()),
        ];
        let out = refine_segment_gaps(&jieba, &gaps);
        assert_eq!(out.len(), 2, "输出应与输入 gap 一一对应");
        assert!(out[1].ops.iter().all(|o| o.op == "ins"));
    }

    #[test]
    fn classification_bands() {
        assert_eq!(classify_cluster(0.99, 0.99, false, 0.99, None).cluster_type, "same");
        assert_eq!(classify_cluster(0.9, 0.8, true, 0.9, None).cluster_type, "same");
        assert_eq!(classify_cluster(0.88, 0.8, false, 0.9, None).cluster_type, "minor_change");
        assert_eq!(classify_cluster(0.75, 0.7, false, 0.7, None).cluster_type, "changed");
        assert_eq!(classify_cluster(0.6, 0.5, false, 0.6, None).cluster_type, "uncertain");
        let rw = classify_cluster(0.75, 0.7, false, 0.3, Some(0.9));
        assert_eq!(rw.cluster_type, "rewrite");
        assert_eq!(rw.severity, "medium");
    }
}
