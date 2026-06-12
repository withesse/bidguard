// 元数据指纹交叉分析：多份标书共用作者/最后保存者 → 围标嫌疑信号。
use crate::engine::report::DocInfo;
use std::collections::HashMap;

const LABELS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];

fn label(i: usize) -> &'static str {
    LABELS.get(i).copied().unwrap_or("?")
}

/// 跨文档检测：若 ≥2 份共享同一作者或最后保存者，给相关文档打风险标记。
pub fn cross_flags(docs: &mut [DocInfo]) {
    flag_shared(docs, |d| d.fingerprint.author.clone(), "作者相同");
    flag_shared(
        docs,
        |d| d.fingerprint.last_modified_by.clone(),
        "最后保存者相同",
    );
}

fn flag_shared<F>(docs: &mut [DocInfo], key_of: F, reason: &str)
where
    F: Fn(&DocInfo) -> Option<String>,
{
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, d) in docs.iter().enumerate() {
        if let Some(k) = key_of(d) {
            if !k.trim().is_empty() {
                groups.entry(k).or_default().push(i);
            }
        }
    }
    for (key, idxs) in groups {
        if idxs.len() >= 2 {
            let who: Vec<&str> = idxs.iter().map(|&i| label(i)).collect();
            for &i in &idxs {
                docs[i]
                    .fingerprint
                    .risk_flags
                    .push(format!("{reason}「{key}」: {}", who.join("·")));
            }
        }
    }
}
