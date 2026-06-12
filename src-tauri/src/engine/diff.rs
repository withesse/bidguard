// 分级 diff（设计文档 §9.8）与八类差异分类（§9.5/§9.7 的阈值带）。
// 短文本字符级 / 中段落词级 / 长段落句级（句级中相邻的删改对再细化为字符级）。
use crate::engine::report::DiffOp;
use jieba_rs::Jieba;
use similar::{ChangeTag, TextDiff};

const CHAR_MAX: usize = 60;
const WORD_MAX: usize = 400;

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
fn split_sentences(text: &str) -> Vec<String> {
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
