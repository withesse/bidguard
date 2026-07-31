// 元数据指纹交叉分析：多份标书共用作者/最后保存者/模板/打包结构、创建时间邻近、
// rsid 修订标识交集、PDF 血缘（trailer ID/XMP GUID/字体子集标签）
// → 围标嫌疑信号（写 risk_flags，部分另出结构化命中对）。
use crate::engine::report::{DocInfo, Fingerprint};
use std::collections::{HashMap, HashSet};

const LABELS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];

// —— 交叉规则阈值（⚠️ 未经实证校准，与 collusion.rs 权重区同一口径）——
/// created 时间邻近阈值（分钟）：≤10 分钟视为「同一台机器同一批生成」候选（拍板值待校准）
const CREATED_PROXIMITY_MIN: i64 = 10;
/// 「TotalTime=0 但修订号高」疑似元数据清洗的修订号下限（弱标记）
const REVISION_SUSPECT_MIN: u32 = 10;
/// rsid 弱档最低共享数：<3 视为噪声不产生命中（审查修正；rsidRoot 相同除外）
pub const RSID_MIN_SHARED: usize = 3;

fn label(i: usize) -> &'static str {
    LABELS.get(i).copied().unwrap_or("?")
}

/// 一对文档的 rsid 交集命中（a < b 为 docs 下标）。
/// root_match=true 表示 w:rsidRoot 相同——高度指示派生自同一母文件（强档）。
pub struct RsidHit {
    pub a: usize,
    pub b: usize,
    pub shared_count: usize,
    pub root_match: bool,
}

/// 两两求 rsid 交集：root_match 或共享数 ≥ RSID_MIN_SHARED 记命中，
/// 并给命中双方追加「rsid 交集」风险标记。
/// exempt_rsids：招标方统一模板的 rsid 豁免集合（M4 招标文件对减接线；当前调用方恒传空），
/// 交集计算前先从各文档 rsid 集合中剔除——统一下发的投标模板天然共享模板 rsid，不算串标。
pub fn rsid_pairs(docs: &mut [DocInfo], exempt_rsids: &HashSet<String>) -> Vec<RsidHit> {
    let mut hits: Vec<RsidHit> = Vec::new();
    {
        let sets: Vec<HashSet<&str>> = docs
            .iter()
            .map(|d| {
                d.fingerprint
                    .rsids
                    .iter()
                    .filter(|r| !exempt_rsids.contains(*r))
                    .map(String::as_str)
                    .collect()
            })
            .collect();
        for a in 0..docs.len() {
            for b in (a + 1)..docs.len() {
                let shared_count = sets[a].intersection(&sets[b]).count();
                let root_match = match (&docs[a].fingerprint.rsid_root, &docs[b].fingerprint.rsid_root)
                {
                    (Some(x), Some(y)) => !exempt_rsids.contains(x) && x == y,
                    _ => false,
                };
                if root_match || shared_count >= RSID_MIN_SHARED {
                    hits.push(RsidHit { a, b, shared_count, root_match });
                }
            }
        }
    }
    for h in &hits {
        let flag = format!(
            "rsid 交集 {}·{}：共享 {} 个修订标识{}",
            label(h.a),
            label(h.b),
            h.shared_count,
            if h.root_match { "，rsidRoot 相同" } else { "" }
        );
        docs[h.a].fingerprint.risk_flags.push(flag.clone());
        docs[h.b].fingerprint.risk_flags.push(flag);
    }
    hits
}

/// 一对 PDF 的血缘命中（a < b 为 docs 下标）。
/// hard_evidence 非空 = 硬命中「同一母文件」：XMP DocumentID / DerivedFrom 指向同一
/// GUID / trailer /ID 首半相同——GUID 碰撞概率趋近于零，eDiscovery 行业标准鉴定项。
/// shared_subset_tags 非空 = 中命中「同一次生成环境」：共享字体子集标签
/// （部分生成器前缀固定不随机，存在系统性误报，故只作中档）。
pub struct LineageHit {
    pub a: usize,
    pub b: usize,
    pub hard_evidence: Vec<String>,
    pub shared_subset_tags: Vec<String>,
}

impl LineageHit {
    pub fn is_hard(&self) -> bool {
        !self.hard_evidence.is_empty()
    }
}

/// GUID 归一：trim + 小写 + 剥离常见前缀（uuid: / xmp.did: / xmp.iid:）——
/// 同一 GUID 在不同工具的写法间可比。空值返回 None。
fn norm_guid(v: &Option<String>) -> Option<String> {
    let s = v.as_deref()?.trim().to_ascii_lowercase();
    let s = ["uuid:", "xmp.did:", "xmp.iid:"]
        .iter()
        .find_map(|p| s.strip_prefix(p))
        .unwrap_or(&s)
        .to_string();
    (!s.is_empty()).then_some(s)
}

/// 两两求 PDF 血缘：硬命中（同一母文件 GUID/trailer ID）或中命中（共享字体子集标签）
/// 记 LineageHit 并给双方追加「PDF 血缘」风险标记；两者皆无但生成环境一致
/// （CreatorTool+Producer+字体全集相同且创建时间邻近）时打「生成环境一致」弱标记
/// ——该前缀在 collusion 的 metadata 强类别里计权（弱命中并入 metadata 的设计）。
pub fn lineage_pairs(docs: &mut [DocInfo]) -> Vec<LineageHit> {
    let mut hits: Vec<LineageHit> = Vec::new();
    let mut weak_pairs: Vec<(usize, usize)> = Vec::new();
    for a in 0..docs.len() {
        for b in (a + 1)..docs.len() {
            let (fa, fb) = (&docs[a].fingerprint, &docs[b].fingerprint);
            let mut hard: Vec<String> = Vec::new();
            let (ida, idb) = (norm_guid(&fa.xmp_document_id), norm_guid(&fb.xmp_document_id));
            let (dfa, dfb) = (norm_guid(&fa.xmp_derived_from), norm_guid(&fb.xmp_derived_from));
            let eq = |x: &Option<String>, y: &Option<String>| {
                matches!((x, y), (Some(m), Some(n)) if m == n)
            };
            if eq(&ida, &idb) {
                hard.push("XMP DocumentID 相同".into());
            }
            // 派生关系：双方派生自同一母文件，或一方派生自另一方
            if eq(&dfa, &dfb) || eq(&dfa, &idb) || eq(&dfb, &ida) {
                hard.push("XMP DerivedFrom 指向同一母文件 GUID".into());
            }
            let id_first = |f: &Fingerprint| {
                f.pdf_id_first.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
            };
            if eq(&id_first(fa), &id_first(fb)) {
                hard.push("PDF trailer ID 首半相同".into());
            }
            let shared_tags: Vec<String> = fa
                .font_subset_tags
                .iter()
                .filter(|t| fb.font_subset_tags.contains(t))
                .cloned()
                .collect();
            if !hard.is_empty() || !shared_tags.is_empty() {
                hits.push(LineageHit { a, b, hard_evidence: hard, shared_subset_tags: shared_tags });
            } else if generation_env_match(fa, fb) {
                weak_pairs.push((a, b));
            }
        }
    }
    for h in &hits {
        let flag = if h.is_hard() {
            format!(
                "PDF 血缘 {}·{}：{}（同一母文件）",
                label(h.a),
                label(h.b),
                h.hard_evidence.join("、")
            )
        } else {
            format!(
                "PDF 血缘 {}·{}：共享字体子集标签「{}」",
                label(h.a),
                label(h.b),
                show_tags(&h.shared_subset_tags)
            )
        };
        docs[h.a].fingerprint.risk_flags.push(flag.clone());
        docs[h.b].fingerprint.risk_flags.push(flag);
    }
    for (a, b) in weak_pairs {
        let flag = format!(
            "生成环境一致（CreatorTool/Producer/字体一致且创建时间邻近）: {}·{}",
            label(a),
            label(b)
        );
        docs[a].fingerprint.risk_flags.push(flag.clone());
        docs[b].fingerprint.risk_flags.push(flag);
    }
    hits
}

/// 展示用：子集标签最多列 3 个，多余的折叠成「等 N 个」。
fn show_tags(tags: &[String]) -> String {
    const SHOW: usize = 3;
    if tags.len() <= SHOW {
        tags.join("、")
    } else {
        format!("{}等 {} 个", tags[..SHOW].join("、"), tags.len())
    }
}

/// 弱同源：生成工具（XMP CreatorTool）+ Producer + 字体全集一致，且创建时间邻近。
/// 字体全集在提取侧已排序去重，Vec 相等即集合相等；任一字段缺失即不命中（宽松）。
fn generation_env_match(fa: &Fingerprint, fb: &Fingerprint) -> bool {
    let same = |x: &Option<String>, y: &Option<String>| {
        matches!(
            (x.as_deref().map(str::trim), y.as_deref().map(str::trim)),
            (Some(m), Some(n)) if !m.is_empty() && m == n
        )
    };
    if !same(&fa.creator_tool, &fb.creator_tool) || !same(&fa.app, &fb.app) {
        return false;
    }
    if fa.pdf_fonts.is_empty() || fa.pdf_fonts != fb.pdf_fonts {
        return false;
    }
    let (Some(x), Some(y)) = (created_epoch(&fa.created), created_epoch(&fb.created)) else {
        return false;
    };
    (x - y).abs() <= CREATED_PROXIMITY_MIN * 60
}

/// created → Unix 秒：PDF 的 CreationDate 是 "D:YYYYMMDDHHmmSS±HH'mm'" 形态，
/// 兼容 W3CDTF（docx 口径）以防混装；解析失败返回 None 不参与判定。
fn created_epoch(v: &Option<String>) -> Option<i64> {
    let s = v.as_deref()?;
    w3c_epoch_secs(s).or_else(|| pdf_epoch_secs(s))
}

/// PDF 日期 → Unix 秒。粒度不足秒（如仅到日）或非常规形态返回 None（宽松不报错）。
fn pdf_epoch_secs(s: &str) -> Option<i64> {
    let t = s.trim();
    let t = t.strip_prefix("D:").unwrap_or(t);
    let b = t.as_bytes();
    if b.len() < 14 || !b[..14].iter().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let naive = chrono::NaiveDate::from_ymd_opt(
        t[0..4].parse().ok()?,
        t[4..6].parse().ok()?,
        t[6..8].parse().ok()?,
    )?
    .and_hms_opt(t[8..10].parse().ok()?, t[10..12].parse().ok()?, t[12..14].parse().ok()?)?;
    // 时区后缀：Z / +HH'mm' / -HH'mm'；缺失按 UTC 处理（同源文件通常同一写法）。
    // 仅前 14 字节校验过为 ASCII 数字，时区区段可能含多字节字符，必须用 get(..)
    // 而非区间索引——越界或落在非字符边界返回 None，宽松跳过绝不 panic。
    let mut offset = 0i64;
    if b.len() >= 17 && (b[14] == b'+' || b[14] == b'-') {
        let hh: i64 = t.get(15..17)?.parse().ok()?;
        let mm: i64 = t.get(18..20).and_then(|x| x.parse().ok()).unwrap_or(0);
        offset = hh * 3600 + mm * 60;
        if b[14] == b'-' {
            offset = -offset;
        }
    }
    Some(naive.and_utc().timestamp() - offset)
}

/// 跨文档检测：共享作者/最后保存者/模板名/zip 打包结构、创建时间邻近 → 风险标记
/// （collusion 的 metadata 信号按这些前缀归类计权）；修订号相同、TotalTime=0 但修订号高
/// 属弱标记（只进 risk_flags 供人工核对，不计权）。
pub fn cross_flags(docs: &mut [DocInfo]) {
    flag_shared(docs, |d| d.fingerprint.author.clone(), "作者相同");
    flag_shared(
        docs,
        |d| d.fingerprint.last_modified_by.clone(),
        "最后保存者相同",
    );
    // 模板名相同（Word 默认模板 Normal/Normal.dotm 不作信号——人人都有）
    flag_shared(
        docs,
        |d| {
            d.fingerprint
                .template_name
                .clone()
                .filter(|t| !is_default_template(t))
        },
        "模板相同",
    );
    flag_created_proximity(docs);
    flag_zip_entry_fp(docs);
    // 弱标记：修订号完全相同且 >1（同一份文件各自另存的旁证，单独不构成证据）
    flag_shared(
        docs,
        |d| {
            d.fingerprint
                .revision
                .as_deref()
                .and_then(|r| r.trim().parse::<u32>().ok())
                .filter(|&r| r > 1)
                .map(|r| r.to_string())
        },
        "修订号相同（弱）",
    );
    // 弱标记：总编辑时长为 0 但修订号高 → 疑似元数据清洗（单文档特征，不成对）
    for d in docs.iter_mut() {
        let rev = d
            .fingerprint
            .revision
            .as_deref()
            .and_then(|r| r.trim().parse::<u32>().ok())
            .unwrap_or(0);
        if d.fingerprint.total_edit_minutes == Some(0) && rev >= REVISION_SUSPECT_MIN {
            d.fingerprint
                .risk_flags
                .push(format!("疑似元数据清洗：总编辑时长为 0 但修订号达 {rev}（弱）"));
        }
    }
}

/// Word 默认模板不作同源信号（几乎所有本机新建文档都是它）。
fn is_default_template(name: &str) -> bool {
    let n = name.trim().to_ascii_lowercase();
    // normal.dot 为 Word 97-2003 时代默认模板名，同一语义一并排除
    n == "normal" || n == "normal.dotm" || n == "normal.dot"
}

/// created 时间邻近（两两 |Δt| ≤ CREATED_PROXIMITY_MIN 分钟）→「同一批生成」候选标记。
/// W3CDTF 解析失败（缺时区/仅日期等）按无时间处理，不打标不报错。
fn flag_created_proximity(docs: &mut [DocInfo]) {
    let times: Vec<Option<i64>> = docs
        .iter()
        .map(|d| d.fingerprint.created.as_deref().and_then(w3c_epoch_secs))
        .collect();
    for a in 0..docs.len() {
        for b in (a + 1)..docs.len() {
            let (Some(x), Some(y)) = (times[a], times[b]) else { continue };
            if (x - y).abs() <= CREATED_PROXIMITY_MIN * 60 {
                let flag = format!(
                    "创建时间邻近（≤{CREATED_PROXIMITY_MIN} 分钟）: {}·{}",
                    label(a),
                    label(b)
                );
                docs[a].fingerprint.risk_flags.push(flag.clone());
                docs[b].fingerprint.risk_flags.push(flag);
            }
        }
    }
}

/// zip 条目序列指纹完全一致 →「同一生成工具/打包管线」标记（完整 sha256 分组，展示截前 12 位）。
fn flag_zip_entry_fp(docs: &mut [DocInfo]) {
    let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, d) in docs.iter().enumerate() {
        if let Some(fp) = &d.fingerprint.zip_entry_fp {
            if !fp.is_empty() {
                groups.entry(fp.clone()).or_default().push(i);
            }
        }
    }
    for (key, idxs) in groups {
        if idxs.len() >= 2 {
            let who: Vec<&str> = idxs.iter().map(|&i| label(i)).collect();
            let short: String = key.chars().take(12).collect();
            for &i in &idxs {
                docs[i]
                    .fingerprint
                    .risk_flags
                    .push(format!("包结构一致「{short}…」: {}", who.join("·")));
            }
        }
    }
}

/// W3CDTF（docProps/core.xml dcterms:created，RFC3339 形态）→ Unix 秒。
/// Word/WPS 输出均带秒与时区（多为 Z）；非常规形态解析失败返回 None（宽松不报错）。
fn w3c_epoch_secs(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s.trim())
        .ok()
        .map(|t| t.timestamp())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::report::Fingerprint;

    fn doc(fp: Fingerprint) -> DocInfo {
        DocInfo {
            id: "d".into(),
            name: "n".into(),
            doc_type: "docx".into(),
            pages: 1,
            char_count: 100,
            fingerprint: fp,
            parse_error: None,
            evasion: None,
        }
    }
    fn rsid_doc(rsids: &[&str], root: Option<&str>) -> DocInfo {
        doc(Fingerprint {
            rsids: rsids.iter().map(|s| s.to_string()).collect(),
            rsid_root: root.map(String::from),
            ..Default::default()
        })
    }
    fn flags_of(d: &DocInfo) -> String {
        d.fingerprint.risk_flags.join(" | ")
    }

    #[test]
    fn rsid_pairs_requires_min_shared_unless_root_match() {
        // 共享 3 个 → 命中；共享 2 个且无 root → 不命中（弱档 ≥3 的审查修正）
        let mut three = vec![
            rsid_doc(&["00A1", "00A2", "00A3", "00A4"], None),
            rsid_doc(&["00A1", "00A2", "00A3", "00B9"], None),
        ];
        let hits = rsid_pairs(&mut three, &HashSet::new());
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].shared_count, 3);
        assert!(!hits[0].root_match);
        assert!(flags_of(&three[0]).contains("rsid 交集"), "双方应有 rsid 交集标记");
        assert!(flags_of(&three[1]).contains("rsid 交集"));

        let mut two = vec![
            rsid_doc(&["00A1", "00A2"], None),
            rsid_doc(&["00A1", "00A2"], None),
        ];
        assert!(rsid_pairs(&mut two, &HashSet::new()).is_empty(), "共享 2 个不足弱档");
        assert!(two.iter().all(|d| d.fingerprint.risk_flags.is_empty()));
    }

    #[test]
    fn rsid_root_match_hits_even_with_zero_shared() {
        let mut docs = vec![
            rsid_doc(&["00A1"], Some("00FF")),
            rsid_doc(&["00B1"], Some("00FF")),
        ];
        let hits = rsid_pairs(&mut docs, &HashSet::new());
        assert_eq!(hits.len(), 1);
        assert!(hits[0].root_match);
        assert!(flags_of(&docs[0]).contains("rsidRoot 相同"));
    }

    #[test]
    fn rsid_absence_yields_no_hit_and_no_flags() {
        // WPS 等生成的 docx 无 rsids 节点：信号缺席而非报错
        let mut docs = vec![rsid_doc(&[], None), rsid_doc(&[], None)];
        assert!(rsid_pairs(&mut docs, &HashSet::new()).is_empty());
        assert!(docs.iter().all(|d| d.fingerprint.risk_flags.is_empty()));
    }

    #[test]
    fn exempt_rsids_are_subtracted_before_intersection() {
        // 招标方统一模板的 rsid 剔除后不足弱档 → 不命中（M4 接线后的豁免语义）
        let exempt: HashSet<String> = ["00A1", "00A2"].iter().map(|s| s.to_string()).collect();
        let mut docs = vec![
            rsid_doc(&["00A1", "00A2", "00A3"], Some("00A1")),
            rsid_doc(&["00A1", "00A2", "00A3"], Some("00A1")),
        ];
        assert!(rsid_pairs(&mut docs, &exempt).is_empty(), "豁免 root 与共享 rsid 后应无命中");
    }

    #[test]
    fn template_flagged_unless_default_normal() {
        let tpl = |t: &str| {
            doc(Fingerprint { template_name: Some(t.into()), ..Default::default() })
        };
        let mut docs = vec![tpl("投标文件模板.dotx"), tpl("投标文件模板.dotx")];
        cross_flags(&mut docs);
        assert!(flags_of(&docs[0]).contains("模板相同"), "非默认模板相同应打标");

        let mut normals = vec![tpl("Normal.dotm"), tpl("Normal.dotm")];
        cross_flags(&mut normals);
        assert!(!flags_of(&normals[0]).contains("模板相同"), "Normal.dotm 不打标");
        let mut normals2 = vec![tpl("Normal"), tpl("normal")];
        cross_flags(&mut normals2);
        assert!(!flags_of(&normals2[0]).contains("模板相同"), "Normal 不打标");
    }

    #[test]
    fn created_proximity_within_10min_flags_beyond_does_not() {
        let at = |t: &str| {
            doc(Fingerprint { created: Some(t.into()), ..Default::default() })
        };
        let mut near = vec![
            at("2024-05-01T10:00:00Z"),
            at("2024-05-01T10:05:00Z"), // 相差 5 分钟
        ];
        cross_flags(&mut near);
        assert!(flags_of(&near[0]).contains("创建时间邻近"));
        assert!(flags_of(&near[1]).contains("创建时间邻近"));

        let mut far = vec![
            at("2024-05-01T10:00:00Z"),
            at("2024-05-01T12:00:00Z"), // 相差 2 小时
        ];
        cross_flags(&mut far);
        assert!(!flags_of(&far[0]).contains("创建时间邻近"));

        // 跨时区等价时刻也应命中（epoch 口径）
        let mut tz = vec![
            at("2024-05-01T10:00:00Z"),
            at("2024-05-01T18:03:00+08:00"),
        ];
        cross_flags(&mut tz);
        assert!(flags_of(&tz[0]).contains("创建时间邻近"));
    }

    #[test]
    fn zip_entry_fp_identical_flags_both() {
        let zfp = |h: &str| {
            doc(Fingerprint { zip_entry_fp: Some(h.into()), ..Default::default() })
        };
        let h = "a".repeat(64);
        let mut same = vec![zfp(&h), zfp(&h)];
        cross_flags(&mut same);
        assert!(flags_of(&same[0]).contains("包结构一致"));
        assert!(!flags_of(&same[0]).contains(&h), "展示应截断而非贴整段 sha256");

        let mut diff = vec![zfp(&"a".repeat(64)), zfp(&"b".repeat(64))];
        cross_flags(&mut diff);
        assert!(!flags_of(&diff[0]).contains("包结构一致"));
    }

    #[test]
    fn revision_weak_marks() {
        let mk = |rev: &str, total: Option<i64>| {
            doc(Fingerprint {
                revision: Some(rev.into()),
                total_edit_minutes: total,
                ..Default::default()
            })
        };
        // revision 相同且 >1 → 弱标记；revision=1（新建文档常态）不打
        let mut same = vec![mk("7", Some(30)), mk("7", Some(45))];
        cross_flags(&mut same);
        assert!(flags_of(&same[0]).contains("修订号相同（弱）"));
        let mut ones = vec![mk("1", Some(30)), mk("1", Some(45))];
        cross_flags(&mut ones);
        assert!(!flags_of(&ones[0]).contains("修订号相同"));
        // TotalTime=0 但修订号高 → 疑似元数据清洗（单文档弱标记）
        let mut washed = vec![mk("12", Some(0)), mk("3", Some(20))];
        cross_flags(&mut washed);
        assert!(flags_of(&washed[0]).contains("疑似元数据清洗"));
        assert!(!flags_of(&washed[1]).contains("疑似元数据清洗"));
    }

    #[test]
    fn w3c_parse_is_lenient() {
        assert!(w3c_epoch_secs("2024-05-01T10:00:00Z").is_some());
        assert!(w3c_epoch_secs("2024-05-01T10:00:00+08:00").is_some());
        assert!(w3c_epoch_secs("2024-05-01").is_none(), "仅日期粒度太粗，不参与邻近判定");
        assert!(w3c_epoch_secs("垃圾数据").is_none());
    }

    // —— M1 取证：PDF 血缘（lineage_pairs 三级命中）——

    fn lin_doc(f: Fingerprint) -> DocInfo {
        let mut d = doc(f);
        d.doc_type = "pdf".into();
        d
    }

    #[test]
    fn lineage_hard_via_document_id_normalizes_guid_prefix() {
        // uuid: 与 xmp.did: 前缀、大小写差异下同一 GUID 仍应硬命中
        let mut docs = vec![
            lin_doc(Fingerprint {
                xmp_document_id: Some("uuid:ABC-123".into()),
                ..Default::default()
            }),
            lin_doc(Fingerprint {
                xmp_document_id: Some("xmp.did:abc-123".into()),
                ..Default::default()
            }),
        ];
        let hits = lineage_pairs(&mut docs);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].is_hard());
        assert!(hits[0].hard_evidence.iter().any(|e| e.contains("DocumentID")));
        assert!(flags_of(&docs[0]).contains("PDF 血缘"), "双方应有血缘风险标记");
        assert!(flags_of(&docs[0]).contains("同一母文件"));
        assert!(flags_of(&docs[1]).contains("同一母文件"));
    }

    #[test]
    fn lineage_hard_via_derived_from_pointing_at_other_document() {
        // 乙派生自甲（DerivedFrom → 甲的 DocumentID）
        let mut docs = vec![
            lin_doc(Fingerprint {
                xmp_document_id: Some("uuid:MOTHER".into()),
                ..Default::default()
            }),
            lin_doc(Fingerprint {
                xmp_document_id: Some("uuid:CHILD".into()),
                xmp_derived_from: Some("uuid:MOTHER".into()),
                ..Default::default()
            }),
        ];
        let hits = lineage_pairs(&mut docs);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].hard_evidence.iter().any(|e| e.contains("DerivedFrom")));
        // 双方各自派生自同一母文件也应硬命中
        let mut sibs = vec![
            lin_doc(Fingerprint { xmp_derived_from: Some("uuid:M".into()), ..Default::default() }),
            lin_doc(Fingerprint { xmp_derived_from: Some("uuid:M".into()), ..Default::default() }),
        ];
        assert!(lineage_pairs(&mut sibs)[0].is_hard());
    }

    #[test]
    fn lineage_hard_via_trailer_id_first_half() {
        let mut docs = vec![
            lin_doc(Fingerprint {
                pdf_id_first: Some("abcd1234".into()),
                pdf_id_second: Some("1111".into()),
                ..Default::default()
            }),
            lin_doc(Fingerprint {
                pdf_id_first: Some("abcd1234".into()),
                pdf_id_second: Some("2222".into()), // 次半不同（每次保存都变）不影响
                ..Default::default()
            }),
        ];
        let hits = lineage_pairs(&mut docs);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].hard_evidence.iter().any(|e| e.contains("trailer ID")));
    }

    #[test]
    fn lineage_mid_via_shared_font_subset_tag() {
        let mut docs = vec![
            lin_doc(Fingerprint {
                font_subset_tags: vec!["ABCDEF+SimSun".into(), "XYZABC+SimHei".into()],
                ..Default::default()
            }),
            lin_doc(Fingerprint {
                font_subset_tags: vec!["ABCDEF+SimSun".into()],
                ..Default::default()
            }),
        ];
        let hits = lineage_pairs(&mut docs);
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].is_hard(), "仅共享子集标签是中命中");
        assert_eq!(hits[0].shared_subset_tags, vec!["ABCDEF+SimSun"]);
        assert!(flags_of(&docs[0]).contains("共享字体子集标签"));
    }

    #[test]
    fn lineage_absence_yields_no_hit_no_flags() {
        // 空指纹 / 互不相同：信号缺席而非报错
        let mut empty = vec![lin_doc(Fingerprint::default()), lin_doc(Fingerprint::default())];
        assert!(lineage_pairs(&mut empty).is_empty());
        assert!(empty.iter().all(|d| d.fingerprint.risk_flags.is_empty()));
        let mut diff = vec![
            lin_doc(Fingerprint {
                xmp_document_id: Some("uuid:A".into()),
                pdf_id_first: Some("01".into()),
                font_subset_tags: vec!["AAAAAA+X".into()],
                ..Default::default()
            }),
            lin_doc(Fingerprint {
                xmp_document_id: Some("uuid:B".into()),
                pdf_id_first: Some("02".into()),
                font_subset_tags: vec!["BBBBBB+X".into()],
                ..Default::default()
            }),
        ];
        assert!(lineage_pairs(&mut diff).is_empty());
        assert!(diff.iter().all(|d| d.fingerprint.risk_flags.is_empty()));
    }

    fn env_fp(created: &str) -> Fingerprint {
        Fingerprint {
            app: Some("Acrobat Distiller 21.0".into()),
            created: Some(created.into()),
            creator_tool: Some("WPS 文字".into()),
            pdf_fonts: vec!["Arial".into(), "SimSun".into()],
            ..Default::default()
        }
    }

    #[test]
    fn lineage_weak_generation_env_flags_metadata_category() {
        // CreatorTool+Producer+字体全集一致且创建时间邻近（PDF 日期格式）→ 弱标记
        let mut near = vec![
            lin_doc(env_fp("D:20240501100000+08'00'")),
            lin_doc(env_fp("D:20240501100500+08'00'")), // 相差 5 分钟
        ];
        assert!(lineage_pairs(&mut near).is_empty(), "弱命中不产生结构化命中对");
        assert!(flags_of(&near[0]).starts_with("生成环境一致"), "实际：{}", flags_of(&near[0]));
        assert!(flags_of(&near[1]).contains("生成环境一致"));

        // 创建时间相差 2 小时 → 不打标
        let mut far = vec![
            lin_doc(env_fp("D:20240501100000+08'00'")),
            lin_doc(env_fp("D:20240501120000+08'00'")),
        ];
        lineage_pairs(&mut far);
        assert!(!flags_of(&far[0]).contains("生成环境一致"));

        // 缺 CreatorTool → 不打标（宽松：缺字段即不命中）
        let mut miss = vec![lin_doc(env_fp("D:20240501100000Z")), lin_doc(env_fp("D:20240501100100Z"))];
        miss[0].fingerprint.creator_tool = None;
        lineage_pairs(&mut miss);
        assert!(!flags_of(&miss[0]).contains("生成环境一致"));
    }

    #[test]
    fn lineage_weak_flag_suppressed_when_hard_or_mid_hit() {
        // 同对已有硬命中：不再叠加「生成环境一致」弱标记（防同一证据双计）
        let mut docs = vec![lin_doc(env_fp("D:20240501100000Z")), lin_doc(env_fp("D:20240501100100Z"))];
        docs[0].fingerprint.xmp_document_id = Some("uuid:SAME".into());
        docs[1].fingerprint.xmp_document_id = Some("uuid:SAME".into());
        let hits = lineage_pairs(&mut docs);
        assert!(hits[0].is_hard());
        assert!(!flags_of(&docs[0]).contains("生成环境一致"));
    }

    #[test]
    fn pdf_date_parse_is_lenient() {
        // 跨时区等价时刻（18:00+08 == 10:00Z）
        assert_eq!(
            pdf_epoch_secs("D:20240501180000+08'00'"),
            pdf_epoch_secs("D:20240501100000Z")
        );
        assert!(pdf_epoch_secs("20240501100000").is_some(), "无 D: 前缀也接受");
        assert!(pdf_epoch_secs("D:202405").is_none(), "粒度不足秒不参与判定");
        assert!(pdf_epoch_secs("垃圾数据").is_none());
        // 畸形时区含多字节字符：区间切片会跨字符边界，必须宽松返回 None 而非 panic
        assert!(pdf_epoch_secs("D:20240501100000+中").is_none(), "时区区多字节字符不得 panic");
        assert!(created_epoch(&Some("2024-05-01T10:00:00Z".into())).is_some(), "W3CDTF 兜底");
    }
}
