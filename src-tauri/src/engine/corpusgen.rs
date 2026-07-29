//! 合成对抗语料生成器（执行方案 §8 W6-1 / M3 校准语料）。
//!
//! 对手写的合成标书式章节施加**已知强度**的确定性文本变换，产出带标签的段对语料
//! （fixtures/corpus/pairs.jsonl），供后续 reranker 阈值 / LR 权重 / 概率校准拟合与
//! 回归门禁使用。全部变换确定性（RNG 复用 candidate::splitmix64，零新依赖），两次运行
//! 逐字节一致。
//!
//! 重要局限（执行方案 §8 风险①）：变换器与检测器共享同一套改写直觉，自造语料上的指标
//! 系统性偏乐观，只能证伪不能证真——合成语料指标是**下界回归基线，不是真实检出率**。
//!
//! 六类变换：①同义替换 ②句序打乱 ③数字微调 ④全半角/标点扰动 ⑤母文件改抬头（文本层）
//! ⑥OCR 噪声注入。标签分级 {same, minor_change, changed, rewrite, unrelated} 与 diff.rs
//! 的八类聚类定义对齐（same/minor_change/changed/rewrite 同名，unrelated 为负类）。
//!
//! 文档集级样本（docsets，generate_docsets）：K 份 docx 组，围标正样本组保留相同 rsidRoot+
//! 共享 rsid、相同 Template、相同 lastModifiedBy、共享同一张图片、清单单价整组乘同一系数、
//! 创建时间邻近、注入零宽规避；独立负样本组各自独立撰写、无同源痕迹。让 M1 取证信号 /
//! M2 evasion 信号 / 未来 M6 数值信号在合成语料上有正负样本可评测（供 M7 LR 融合拟合）。
//!
//! 门控：仅 `#[cfg(any(test, feature = "dev-tools"))]` 编译，不进发布二进制。

use crate::engine::{candidate, chunker, features};
use jieba_rs::Jieba;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// —— 回归门禁（W6-5）：无模型层跑全语料算出分层指标，与 baseline_metrics.json 逐项对比 ——
use crate::engine::corpus::{fill_tfidf, CmpChunk};
use crate::engine::report::{Cluster as RCluster, ClusterSeg, DocInfo, EvasionSummary};
use crate::engine::{calibrate, clustering, collusion, diff, fingerprint, matrix, parse, scoring};
use std::collections::{BTreeMap, BTreeSet, HashSet};

/// 每个标签的目标产出对数。5 标签 × 350 = 1750 对（≥1500），四类核心标签各 ≥300。
const TARGET_PER_LABEL: usize = 350;

/// 虚构投标人抬头（全部为通用构造名，无任何真实企业信息）——变换⑤专用。
const COMPANIES: &[&str] = &[
    "华信建设集团",
    "恒源智能科技",
    "中正工程技术",
    "博远数字系统",
    "泰和信息技术",
    "广盛建工集团",
    "瑞邦软件工程",
    "昆仑网络科技",
    "嘉源建设发展",
    "星辰智慧科技",
];

/// 禁止在其两侧插入空格的字符集：中文数字 + 常见单位 + 「之/分」（保护 normalize
/// 的中文数字归一——「三年」若被拆成「三 年」会导致归一结果偏离，破坏 same 可逆性）。
const NO_SPACE: &str = "零一二三四五六七八九十百千万亿两壹贰貳叁肆伍陆柒捌玖拾佰仟之分日天月年个时秒周次期项条款名家元圆";

// —— 确定性 RNG（复用 candidate::splitmix64，禁用 rand/Math.random）——

struct Rng {
    state: u64,
}

impl Rng {
    fn seeded(seed: u64) -> Self {
        Rng { state: seed }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        candidate::splitmix64(self.state)
    }
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// [0, n) 均匀整数（n>0）。
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }
    fn chance(&mut self, p: f64) -> bool {
        self.next_f64() < p
    }
}

// —— 同义表（自建；加载时排序去重，二分查找）——

pub struct SynonymTable {
    entries: Vec<(String, Vec<String>)>,
}

impl SynonymTable {
    pub fn load() -> Self {
        let raw = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/corpus/synonyms.tsv"));
        let mut entries: Vec<(String, Vec<String>)> = Vec::new();
        for line in raw.lines() {
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split('\t');
            let (Some(word), Some(alts)) = (it.next(), it.next()) else {
                continue;
            };
            let word = word.trim();
            if word.is_empty() {
                continue;
            }
            let alts: Vec<String> = alts
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            if alts.is_empty() {
                continue;
            }
            entries.push((word.to_string(), alts));
        }
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries.dedup_by(|a, b| a.0 == b.0);
        SynonymTable { entries }
    }

    /// 二分查找词的候选同义词（None = 不在表内）。
    pub fn lookup(&self, word: &str) -> Option<&[String]> {
        self.entries
            .binary_search_by(|(k, _)| k.as_str().cmp(word))
            .ok()
            .map(|i| self.entries[i].1.as_slice())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn is_sorted(&self) -> bool {
        self.entries.windows(2).all(|w| w[0].0 <= w[1].0)
    }
}

// —— OCR 形近字混淆表 ——

struct OcrTable {
    map: HashMap<char, Vec<char>>,
}

impl OcrTable {
    fn load() -> Self {
        let raw =
            include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/corpus/ocr_confusions.tsv"));
        let mut map: HashMap<char, Vec<char>> = HashMap::new();
        for line in raw.lines() {
            let line = line.trim_end_matches(['\r', '\n']);
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut it = line.split('\t');
            let (Some(key), Some(alts)) = (it.next(), it.next()) else {
                continue;
            };
            let Some(kc) = key.chars().next() else {
                continue;
            };
            let alts: Vec<char> = alts.split(',').filter_map(|s| s.trim().chars().next()).collect();
            if alts.is_empty() {
                continue;
            }
            map.entry(kc).or_default().extend(alts);
        }
        OcrTable { map }
    }
    fn get(&self, c: char) -> Option<&[char]> {
        self.map.get(&c).map(|v| v.as_slice())
    }
}

// —— 种子章节加载 ——

struct Base {
    seed_id: String,
    text: String,
    /// RNG 派生键（seed_id#para_idx），稳定唯一。
    key: String,
}

/// 种子目录：BIDGUARD_CALIB_DIR override（真实脱敏语料）优先，否则仓库合成种子。
fn seeds_dir() -> PathBuf {
    if let Ok(d) = std::env::var("BIDGUARD_CALIB_DIR") {
        if !d.trim().is_empty() {
            return PathBuf::from(d);
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/corpus/seeds")
}

/// 读种子目录下全部 *.txt（按文件名排序），每个空行分隔的段落成为一个基准单元。
fn load_bases() -> Vec<Base> {
    let dir = seeds_dir();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read seeds dir {dir:?}: {e}"))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("txt"))
        .collect();
    files.sort();
    let mut bases = Vec::new();
    for f in files {
        let sid = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
        let content = std::fs::read_to_string(&f).unwrap_or_default();
        for (i, para) in content
            .split("\n\n")
            .map(|p| p.trim())
            .filter(|p| p.chars().count() >= 10)
            .enumerate()
        {
            bases.push(Base {
                seed_id: sid.clone(),
                text: para.to_string(),
                key: format!("{sid}#{i}"),
            });
        }
    }
    bases
}

// —— 六类确定性变换 ——

#[inline]
fn is_ideograph(c: char) -> bool {
    let u = c as u32;
    (0x3400..=0x9FFF).contains(&u)
}

#[inline]
fn no_space_char(c: char) -> bool {
    NO_SPACE.contains(c)
}

/// CJK 标点 → 其 ASCII 等价（normalize 在 ignore_punctuation 下两者都会被剥离，故可逆）。
fn cjk_punct_ascii(c: char) -> Option<char> {
    Some(match c {
        '。' => '.',
        '，' => ',',
        '、' => ',',
        '；' => ';',
        '：' => ':',
        '？' => '?',
        '！' => '!',
        '（' => '(',
        '）' => ')',
        '《' => '<',
        '》' => '>',
        _ => return None,
    })
}

/// 变换④ 全半角/标点扰动（NFKC 归一的逆操作，**可逆**——normalize 归一后与原文一致）。
/// 三种安全扰动：ASCII 数字→全角、CJK 标点→ASCII、安全汉字间插空格。
/// 刻意不碰 `.`/`,`（数字内小数点/千分位保留规则）与数字/单位邻接位，保证 same 可逆。
fn punct_perturb(text: &str, rng: &mut Rng) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len() + 16);
    for i in 0..chars.len() {
        let c = chars[i];
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        if c.is_ascii_digit() {
            if rng.chance(0.5) {
                out.push(char::from_u32(c as u32 - 0x30 + 0xFF10).unwrap());
            } else {
                out.push(c);
            }
            continue;
        }
        if let Some(rep) = cjk_punct_ascii(c) {
            let neighbor_digit = prev.is_some_and(|p| p.is_ascii_digit())
                || next.is_some_and(|nx| nx.is_ascii_digit());
            if !neighbor_digit && rng.chance(0.5) {
                out.push(rep);
            } else {
                out.push(c);
            }
            continue;
        }
        out.push(c);
        if is_ideograph(c) {
            if let Some(nx) = next {
                if is_ideograph(nx)
                    && !no_space_char(c)
                    && !no_space_char(nx)
                    && rng.chance(0.3)
                {
                    out.push(if rng.next_u64() & 1 == 0 { ' ' } else { '　' });
                }
            }
        }
    }
    out
}

/// 变换① 同义替换：jieba 分词 → 表内词按比例 p 替换（避开实体 span，最多 max_swaps 次）。
/// 返回（新文本, 实际替换次数）。
fn synonym_replace(
    jieba: &Jieba,
    text: &str,
    syn: &SynonymTable,
    p: f64,
    max_swaps: usize,
    rng: &mut Rng,
) -> (String, usize) {
    let spans = features::entity_spans(text);
    let tokens = jieba.cut(text, false);
    let mut out = String::with_capacity(text.len());
    let mut off = 0usize;
    let mut swaps = 0usize;
    for tok in tokens {
        let start = off;
        let end = off + tok.len();
        off = end;
        let in_entity = spans.iter().any(|&(s, e, _)| start < e && s < end);
        if !in_entity && swaps < max_swaps {
            if let Some(alts) = syn.lookup(tok) {
                if rng.chance(p) {
                    let pick = &alts[rng.below(alts.len())];
                    out.push_str(pick);
                    swaps += 1;
                    continue;
                }
            }
        }
        out.push_str(tok);
    }
    (out, swaps)
}

/// 变换② 句序打乱：复用 chunker 句边界，段内 Fisher-Yates 置换。
fn sentence_reorder(text: &str, rng: &mut Rng) -> String {
    let sents: Vec<&str> = chunker::split_sentences(text);
    if sents.len() < 2 {
        return text.to_string();
    }
    let mut idx: Vec<usize> = (0..sents.len()).collect();
    for i in (1..idx.len()).rev() {
        let j = rng.below(i + 1);
        idx.swap(i, j);
    }
    if idx.iter().enumerate().all(|(i, &v)| i == v) {
        idx.swap(0, 1);
    }
    let mut out = String::with_capacity(text.len());
    for &i in &idx {
        out.push_str(sents[i]);
    }
    out
}

/// 实体 span [s,e) 内首个 ASCII 数字串的字节区间。
fn find_digit_run(text: &str, s: usize, e: usize) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut i = s;
    while i < e && !bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i >= e {
        return None;
    }
    let start = i;
    while i < e && bytes[i].is_ascii_digit() {
        i += 1;
    }
    Some((start, i))
}

/// 变换③ 数字微调：实体 span（金额/工期/百分比）内数值 ±1–5%。返回（新文本, 是否改动）。
fn numeric_tweak(text: &str, rng: &mut Rng) -> (String, bool) {
    let spans = features::entity_spans(text);
    let mut cands: Vec<(usize, usize)> = Vec::new();
    for &(s, e, kind) in &spans {
        if matches!(kind, "amount" | "duration" | "percentage")
            && find_digit_run(text, s, e).is_some()
        {
            cands.push((s, e));
        }
    }
    if cands.is_empty() {
        return (text.to_string(), false);
    }
    let (s, e) = cands[rng.below(cands.len())];
    let (ds, de) = find_digit_run(text, s, e).expect("candidate has digit run");
    let num: i64 = text[ds..de].parse().unwrap_or(0);
    if num <= 0 {
        return (text.to_string(), false);
    }
    let delta_pct = 1 + rng.below(5) as i64; // 1..=5
    let sign: i64 = if rng.next_u64() & 1 == 0 { 1 } else { -1 };
    let mut d = num * delta_pct / 100;
    if d == 0 {
        d = 1;
    }
    let mut new = num + sign * d;
    if new <= 0 {
        new = num + d;
    }
    if new == num {
        new = num + 1;
    }
    let mut out = String::with_capacity(text.len() + 2);
    out.push_str(&text[..ds]);
    out.push_str(&new.to_string());
    out.push_str(&text[de..]);
    (out, true)
}

/// 无 RNG 的确定性兜底：首个含形近字的字符替换为其首选形近字，否则首个汉字后插空格。
fn force_one_ocr(text: &str, ocr: &OcrTable) -> String {
    let mut out = String::with_capacity(text.len() + 1);
    let mut done = false;
    for c in text.chars() {
        if !done {
            if let Some(alts) = ocr.get(c) {
                out.push(alts[0]);
                done = true;
                continue;
            }
        }
        out.push(c);
        if !done && is_ideograph(c) {
            out.push(' ');
            done = true;
        }
    }
    out
}

/// 变换⑥ OCR 噪声：形近字替换 + 随机漏字 + 插空格（模拟扫描件误识，不可逆）。
fn ocr_noise(
    text: &str,
    ocr: &OcrTable,
    p_sub: f64,
    p_del: f64,
    p_space: f64,
    rng: &mut Rng,
) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    let mut changed = false;
    for c in text.chars() {
        if let Some(alts) = ocr.get(c) {
            if rng.chance(p_sub) {
                out.push(alts[rng.below(alts.len())]);
                changed = true;
                continue;
            }
        }
        if (is_ideograph(c) || c.is_ascii_alphanumeric()) && rng.chance(p_del) {
            changed = true;
            continue;
        }
        out.push(c);
        if is_ideograph(c) && rng.chance(p_space) {
            out.push(' ');
            changed = true;
        }
    }
    if !changed {
        return force_one_ocr(text, ocr);
    }
    out
}

/// 变换⑤ 母文件改抬头（文本层）：同一母段配不同投标人抬头 + 少量独立编辑 → 高相似度对。
fn header_pair(base: &Base, jieba: &Jieba, syn: &SynonymTable, rng: &mut Rng) -> (String, String) {
    let ia = rng.below(COMPANIES.len());
    let mut ib = rng.below(COMPANIES.len());
    if ib == ia {
        ib = (ia + 1) % COMPANIES.len();
    }
    let a = format!("投标人：{}。\n{}", COMPANIES[ia], base.text);
    let (edited, _) = synonym_replace(jieba, &base.text, syn, 1.0, 1, rng);
    let b = format!("投标人：{}。\n{}", COMPANIES[ib], edited);
    (a, b)
}

// —— 段对样本 ——

#[derive(Clone, Copy)]
enum Label {
    Same,
    MinorChange,
    Changed,
    Rewrite,
    Unrelated,
}

impl Label {
    fn as_str(self) -> &'static str {
        match self {
            Label::Same => "same",
            Label::MinorChange => "minor_change",
            Label::Changed => "changed",
            Label::Rewrite => "rewrite",
            Label::Unrelated => "unrelated",
        }
    }
}

const LABELS: [Label; 5] = [
    Label::Same,
    Label::MinorChange,
    Label::Changed,
    Label::Rewrite,
    Label::Unrelated,
];

/// 段对标注记录（JSONL 一行一条）。字段顺序即 JSON 键顺序，serde 保序 → 确定性。
#[derive(Serialize, Deserialize, Clone)]
pub struct PairRecord {
    pub id: String,
    pub seed_id: String,
    pub transform: String,
    pub label: String,
    pub text_a: String,
    pub text_b: String,
}

#[allow(clippy::too_many_arguments)]
fn build_pair(
    label: Label,
    base_idx: usize,
    bases: &[Base],
    variant: usize,
    jieba: &Jieba,
    syn: &SynonymTable,
    ocr: &OcrTable,
    rng: &mut Rng,
) -> Option<(String, String, String)> {
    let base = &bases[base_idx];
    match label {
        Label::Same => {
            let b = punct_perturb(&base.text, rng);
            if b == base.text {
                return None;
            }
            Some(("punct".to_string(), base.text.clone(), b))
        }
        Label::MinorChange => match variant % 4 {
            0 => {
                let (b, ok) = numeric_tweak(&base.text, rng);
                if ok {
                    Some(("numeric".to_string(), base.text.clone(), b))
                } else {
                    let (s, n) = synonym_replace(jieba, &base.text, syn, 1.0, 1, rng);
                    (n > 0).then(|| ("synonym".to_string(), base.text.clone(), s))
                }
            }
            1 => {
                let (a, b) = header_pair(base, jieba, syn, rng);
                (a != b).then_some(("header".to_string(), a, b))
            }
            2 => {
                let b = ocr_noise(&base.text, ocr, 0.10, 0.02, 0.03, rng);
                (b != base.text).then(|| ("ocr".to_string(), base.text.clone(), b))
            }
            _ => {
                let (s, n) = synonym_replace(jieba, &base.text, syn, 0.9, 2, rng);
                (n > 0).then(|| ("synonym".to_string(), base.text.clone(), s))
            }
        },
        Label::Changed => match variant % 3 {
            0 => {
                let (s, ns) = synonym_replace(jieba, &base.text, syn, 0.55, 64, rng);
                let (b, _) = numeric_tweak(&s, rng);
                (ns > 0 && b != base.text)
                    .then(|| ("synonym+numeric".to_string(), base.text.clone(), b))
            }
            1 => {
                let b = ocr_noise(&base.text, ocr, 0.28, 0.06, 0.05, rng);
                (b != base.text).then(|| ("ocr".to_string(), base.text.clone(), b))
            }
            _ => {
                let (s, n) = synonym_replace(jieba, &base.text, syn, 0.6, 64, rng);
                (n > 0 && s != base.text)
                    .then(|| ("synonym".to_string(), base.text.clone(), s))
            }
        },
        Label::Rewrite => {
            let (s, _ns) = synonym_replace(jieba, &base.text, syn, 0.95, 128, rng);
            let r = sentence_reorder(&s, rng);
            if r == base.text {
                return None;
            }
            Some(("synonym+reorder".to_string(), base.text.clone(), r))
        }
        Label::Unrelated => {
            let n = bases.len();
            let mut j = base_idx
                .wrapping_mul(31)
                .wrapping_add(variant.wrapping_mul(97))
                .wrapping_add(7)
                % n;
            let mut tries = 0;
            while bases[j].seed_id == base.seed_id && tries < n {
                j = (j + 1) % n;
                tries += 1;
            }
            if bases[j].seed_id == base.seed_id {
                return None;
            }
            Some(("unrelated".to_string(), base.text.clone(), bases[j].text.clone()))
        }
    }
}

/// 生成全部段对（确定性）：标签固定顺序 → 基准段轮转 → 变体递增 → RNG 由稳定键派生。
pub fn generate_pairs() -> Vec<PairRecord> {
    let jieba = Jieba::new();
    let syn = SynonymTable::load();
    let ocr = OcrTable::load();
    let bases = load_bases();
    assert!(!bases.is_empty(), "no seed paragraphs found under {:?}", seeds_dir());
    let n = bases.len();
    let cap = n.saturating_mul(80).max(2000);
    let mut records = Vec::new();
    for label in LABELS {
        let mut produced = 0usize;
        let mut attempt = 0usize;
        while produced < TARGET_PER_LABEL && attempt < cap {
            let base_idx = attempt % n;
            let variant = attempt / n;
            let base = &bases[base_idx];
            let seed = features::hash64(&format!("{}|{}|{}", label.as_str(), base.key, variant));
            let mut rng = Rng::seeded(seed);
            if let Some((transform, a, b)) =
                build_pair(label, base_idx, &bases, variant, &jieba, &syn, &ocr, &mut rng)
            {
                records.push(PairRecord {
                    id: format!("{}-{:04}", label.as_str(), produced),
                    seed_id: base.seed_id.clone(),
                    transform,
                    label: label.as_str().to_string(),
                    text_a: a,
                    text_b: b,
                });
                produced += 1;
            }
            attempt += 1;
        }
    }
    records
}

/// 段对 → JSONL（一行一条 serde_json，末尾换行）。serde 保序 → 逐字节确定性。
pub fn to_jsonl(records: &[PairRecord]) -> String {
    let mut s = String::new();
    for r in records {
        s.push_str(&serde_json::to_string(r).expect("serialize PairRecord"));
        s.push('\n');
    }
    s
}

// —— 文档集级样本（docsets）：围标正样本组 / 独立负样本组 ——
//
// 目的（执行方案 §8 W6-1 后半，审查发现原设计缺失）：让 M1 取证信号（rsid/imageReuse/
// zipEntryFp/lastModifiedBy/createdProximity）、M2 evasion 信号、未来 M6 数值信号在合成语料上
// 有【正负样本可评测】，供 M7 的 LR 融合拟合与回归门禁消费。
//
// 剧本：
//   · 围标正样本组（collusion）：同一「枪手」写多份 → 保留相同 rsidRoot+共享 rsid、相同
//     Template、相同 lastModifiedBy、共享同一张图片、清单单价整组乘同一系数（等比）、
//     创建时间邻近、注入零宽规避字符；各份仅换公司抬头 + 少量同义微编辑（文本层高相似）。
//   · 独立负样本组（independent）：各自独立撰写（不同种子段落）、rsid/root 互不相交、
//     无模板、各自不同图片、报价无系数关系、无零宽、打包结构各异、创建时间远隔。
//
// 确定性：全部由 (docset 序号, doc 序号) 稳定派生（features::hash64 + splitmix64），两次
// 运行逐字节一致。docx 用 Stored（不压缩）+ 固定时间戳打包，字节可复现。

/// 正/负各 DOCSET_GROUPS 组（总 2×GROUPS ≥ 10，正负各半）。
const DOCSET_GROUPS: usize = 6;
/// 每组文档份数（≥3 让 rsid 交集与多文档雷同信号成立）。
const DOCS_PER_SET: usize = 3;
/// 每组共享/独立的 rsid 数（≥ fingerprint::RSID_MIN_SHARED=3 才计命中）。
const RSIDS_PER_DOC: usize = 5;
/// 围标组注入的零宽字符数（evasion 正样本）。
const ZW_INJECT: usize = 4;

/// 一份生成的 docx（内存字节 + 组内文件名）。
pub struct GeneratedDoc {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// 文档集清单（docsets.jsonl 一行一条；字段顺序即 JSON 键顺序 → 确定性）。
#[derive(Serialize, Deserialize, Clone)]
pub struct DocsetManifest {
    pub docset_id: String,
    /// collusion | independent
    pub label: String,
    /// 组内文档文件名（相对 docsets/<docset_id>/）。
    pub docs: Vec<String>,
    /// 实际注入的信号种类（与生成逻辑一一对应，供 harness 断言「注入=可检」）。
    pub planted_signals: Vec<String>,
}

/// 一个文档集：清单 + 全部文档字节。
pub struct GeneratedDocset {
    pub manifest: DocsetManifest,
    pub docs: Vec<GeneratedDoc>,
}

fn xml_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            _ => o.push(c),
        }
    }
    o
}

/// 在文本中确定性插入 n 个零宽字符（U+200B），供 evasion 正样本。
fn inject_zero_width(text: &str, n: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if n == 0 || chars.len() < 2 {
        return text.to_string();
    }
    let step = (chars.len() / (n + 1)).max(1);
    let mut out = String::with_capacity(text.len() + n * 3);
    let mut injected = 0usize;
    for (i, c) in chars.iter().enumerate() {
        out.push(*c);
        if injected < n && i > 0 && i % step == 0 {
            out.push('\u{200B}');
            injected += 1;
        }
    }
    out
}

/// 确定性小图（96×96 双色棋盘，颜色由 seed 派生）：同 seed 逐字节一致 → 共享图片同 sha256；
/// 不同 seed 颜色不同 → 独立图片 sha256 互异。96≥MIN_IMAGE_HASH_PX(80)，PNG 编码确定。
fn make_png(seed: u64) -> Vec<u8> {
    use image::ImageEncoder;
    const W: u32 = 96;
    const H: u32 = 96;
    let a = features::hash64(&format!("imgA|{seed}"));
    let b = features::hash64(&format!("imgB|{seed}"));
    let ca = image::Rgb([(a & 0xff) as u8, ((a >> 8) & 0xff) as u8, ((a >> 16) & 0xff) as u8]);
    let cb = image::Rgb([(b & 0xff) as u8, ((b >> 8) & 0xff) as u8, ((b >> 16) & 0xff) as u8]);
    let img =
        image::RgbImage::from_fn(W, H, |x, y| if (x / 24 + y / 24) % 2 == 0 { ca } else { cb });
    let mut buf = Vec::new();
    image::codecs::png::PngEncoder::new(&mut buf)
        .write_image(img.as_raw(), W, H, image::ExtendedColorType::Rgb8)
        .expect("encode png");
    buf
}

/// 确定性打包 docx：Stored（不压缩）+ 固定时间戳 → 字节可复现。parts 顺序即 zip 条目顺序。
fn build_docx(parts: &[(String, Vec<u8>)]) -> Vec<u8> {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    let mut cur = Cursor::new(Vec::new());
    {
        let mut zw = zip::ZipWriter::new(&mut cur);
        let o = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (name, bytes) in parts {
            zw.start_file(name.as_str(), o).expect("zip start_file");
            zw.write_all(bytes).expect("zip write");
        }
        zw.finish().expect("zip finish");
    }
    cur.into_inner()
}

fn content_types_xml() -> &'static str {
    r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#
}

fn settings_xml(rsids: &[String], root: &str) -> String {
    let mut inner = format!(r#"<w:rsidRoot w:val="{root}"/>"#);
    for r in rsids {
        inner.push_str(&format!(r#"<w:rsid w:val="{r}"/>"#));
    }
    format!(
        r#"<?xml version="1.0"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:zoom w:percent="100"/><w:rsids>{inner}</w:rsids></w:settings>"#
    )
}

fn core_xml(author: &str, last_saved_by: &str, created: &str) -> String {
    format!(
        r#"<?xml version="1.0"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/"><dc:creator>{a}</dc:creator><cp:lastModifiedBy>{m}</cp:lastModifiedBy><dcterms:created>{c}</dcterms:created><dcterms:modified>{c}</dcterms:modified></cp:coreProperties>"#,
        a = xml_escape(author),
        m = xml_escape(last_saved_by),
        c = created,
    )
}

fn app_xml(template: Option<&str>) -> String {
    let tpl = template
        .map(|t| format!("<Template>{}</Template>", xml_escape(t)))
        .unwrap_or_default();
    format!(
        r#"<?xml version="1.0"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Office Word</Application>{tpl}<Pages>3</Pages></Properties>"#
    )
}

/// 正文 XML：若干段落 + 一张报价表（表头 + 明细行）。
fn body_xml(paragraphs: &[String], price_rows: &[Vec<String>]) -> String {
    let mut b = String::new();
    for p in paragraphs {
        b.push_str(&format!(
            r#"<w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p>"#,
            xml_escape(p)
        ));
    }
    if !price_rows.is_empty() {
        b.push_str("<w:tbl>");
        for row in price_rows {
            b.push_str("<w:tr>");
            for cell in row {
                b.push_str(&format!(
                    r#"<w:tc><w:p><w:r><w:t xml:space="preserve">{}</w:t></w:r></w:p></w:tc>"#,
                    xml_escape(cell)
                ));
            }
            b.push_str("</w:tr>");
        }
        b.push_str("</w:tbl>");
    }
    b
}

/// 报价清单明细行数：需 ≥10 以越过 W5-3/W5-4 规律性与相关性的 n≥10 门槛（少于此则数值层
/// 只出 insufficient，注入的等比证据无法被检出）。
const BOQ_ROWS: usize = 12;

/// 报价清单明细行（M6 数值层可解析口径）。表头用规范列名（项目编码/项目名称/单位/工程量/
/// 综合单价/合价）——旧版表头「序号|设备名称及服务内容|单价（元）|工期」只有 2 列能被
/// boq::extract_document 识别（&lt;3 列即判非清单表），注入的「等比乘系数」证据实际不可读，
/// 数值信号在门禁中恒不触发。
/// - 围标组（jitter=None）：各份共用同一基准单价序列、整组乘 ratio ⇒ 份间严格等比（y=kx），
///   触发 numericPattern/numericCorrelation。
/// - 独立组（jitter=Some(seed)）：逐行独立扰动 ⇒ 份间无线性关系，作负样本。
fn price_rows(base_units: &[i64], ratio_pct: i64, jitter: Option<u64>) -> Vec<Vec<String>> {
    let mut rows = vec![vec![
        "项目编码".into(),
        "项目名称".into(),
        "单位".into(),
        "工程量".into(),
        "综合单价".into(),
        "合价".into(),
    ]];
    let items = [
        "核心交换机及配套光模块安装调试",
        "机房精密空调供货与调试",
        "综合布线及线缆敷设",
        "UPS 不间断电源及电池组",
        "防火墙及入侵检测设备",
        "机柜及配电单元安装",
    ];
    let units = ["台", "项", "米", "套"];
    for i in 0..BOQ_ROWS {
        let seed_base = base_units[i % base_units.len()];
        let base = match jitter {
            // 围标组：确定性展开，份间一致 ⇒ 仅 ratio 造成差异。
            None => seed_base + (i as i64) * 137,
            // 独立组：逐行哈希扰动，破坏线性关系。
            Some(s) => {
                seed_base + (features::hash64(&format!("{s}|{i}")) % 900) as i64 + (i as i64) * 11
            }
        };
        let unit_price = base * ratio_pct / 100;
        let qty = 1 + (i as i64) % 5;
        rows.push(vec![
            // 12 位清单编码，份间一致 ⇒ 按编码精确对齐。
            format!("0304120{:05}", 1000 + i),
            items[i % items.len()].to_string(),
            units[i % units.len()].to_string(),
            qty.to_string(),
            unit_price.to_string(),
            // 合价 = 工程量 × 综合单价（严格自洽，不植入算术错误）。
            (qty * unit_price).to_string(),
        ]);
    }
    rows
}

/// 围标正样本组：同一母段 → K 份仅换抬头 + 微编辑，保留全部同源取证痕迹。
fn build_collusion_set(
    g: usize,
    bases: &[Base],
    jieba: &Jieba,
    syn: &SynonymTable,
) -> GeneratedDocset {
    let docset_id = format!("collusion-{g:02}");
    let master = &bases[(g.wrapping_mul(7) + 1) % bases.len()].text;
    // 全组共享的取证键（rsidRoot / rsids / 模板 / 打包操作机 / 图片）。
    let root = format!("AB{:02X}0000", g);
    let rsids: Vec<String> = (0..RSIDS_PER_DOC).map(|j| format!("AB{:02X}{:04X}", g, j)).collect();
    let template = format!("投标文件模板{}.dotx", ['甲', '乙', '丙', '丁', '戊', '己'][g % 6]);
    let last_saved_by = "投标编制专用机".to_string();
    let png = make_png(features::hash64(&format!("collusion-img|{g}")));
    let base_units = [1200i64, 3500, 800];

    let mut docs = Vec::new();
    for d in 0..DOCS_PER_SET {
        let mut rng = Rng::seeded(features::hash64(&format!("docset|{docset_id}|{d}")));
        let company = COMPANIES[(g.wrapping_mul(3) + d) % COMPANIES.len()];
        // 母段微编辑（0/1/2 次同义替换）：份间高相似但非逐字，贴近真实枪手改写。
        let (edited, _) = synonym_replace(jieba, master, syn, 0.4, d, &mut rng);
        let commit = inject_zero_width(
            &format!("我方（{company}）承诺严格按照招标文件要求组织施工并确保工程质量达到合格标准。"),
            ZW_INJECT,
        );
        let paragraphs = vec![format!("投标人：{company}"), edited, commit];
        // 等比：整组乘同一 ratio（份间不同 ratio，行内同 ratio）。
        let ratio = 100 + (d as i64) * 8; // 100% / 108% / 116%
        let rows = price_rows(&base_units, ratio, None);
        // 创建时间邻近：同日、分钟差 < 阈值。
        let created = format!("2024-03-15T09:{:02}:00Z", d * 3);
        let parts = vec![
            ("[Content_Types].xml".to_string(), content_types_xml().as_bytes().to_vec()),
            ("word/document.xml".to_string(), doc_xml(&paragraphs, &rows)),
            ("docProps/core.xml".to_string(), core_xml(company, &last_saved_by, &created).into_bytes()),
            ("docProps/app.xml".to_string(), app_xml(Some(&template)).into_bytes()),
            ("word/settings.xml".to_string(), settings_xml(&rsids, &root).into_bytes()),
            ("word/media/image1.png".to_string(), png.clone()),
        ];
        docs.push(GeneratedDoc { name: format!("bid_{}.docx", d + 1), bytes: build_docx(&parts) });
    }
    GeneratedDocset {
        manifest: DocsetManifest {
            docset_id,
            label: "collusion".to_string(),
            docs: docs.iter().map(|d| d.name.clone()).collect(),
            planted_signals: vec![
                "rsid".into(),
                "template".into(),
                "lastModifiedBy".into(),
                "zipEntryFp".into(),
                "createdProximity".into(),
                "imageReuse".into(),
                "numericRatio".into(),
                "evasion".into(),
            ],
        },
        docs,
    }
}

/// 独立负样本组：各份不同母段、rsid/root 互不相交、无模板、各自图片、报价无系数、无零宽、
/// 打包结构各异（每份加一个唯一惰性部件 → zip 条目指纹互不相同）、创建时间远隔。
fn build_independent_set(
    g: usize,
    bases: &[Base],
    jieba: &Jieba,
    syn: &SynonymTable,
) -> GeneratedDocset {
    let docset_id = format!("independent-{g:02}");
    let mut docs = Vec::new();
    for d in 0..DOCS_PER_SET {
        let mut rng = Rng::seeded(features::hash64(&format!("docset|{docset_id}|{d}")));
        let company = COMPANIES[(g.wrapping_mul(5) + d * 2 + 1) % COMPANIES.len()];
        // 各份取不同母段（跨种子），并各自改写 → 无同源文本痕迹。
        let base = &bases[(g.wrapping_mul(13) + d.wrapping_mul(29) + 3) % bases.len()].text;
        let (edited, _) = synonym_replace(jieba, base, syn, 0.5, 32, &mut rng);
        let paragraphs = vec![
            format!("投标人：{company}"),
            edited,
            format!("{company}将依法独立完成本项目全部建设内容。"),
        ];
        // 唯一取证键：rsid/root 用 (set,doc) 唯一前缀 → 跨份交集恒为空；无模板。
        let tag = (200 + g * DOCS_PER_SET + d) as u16;
        let root = format!("{tag:04X}FFFF");
        let rsids: Vec<String> = (0..RSIDS_PER_DOC).map(|j| format!("{tag:04X}{j:04X}")).collect();
        let png = make_png(features::hash64(&format!("independent-img|{docset_id}|{d}")));
        // 报价无系数关系：各行独立扰动（jitter 按文档取种子）⇒ 份间无线性关系。
        let rows = price_rows(
            &[900 + (d as i64) * 137, 3100 + (g as i64) * 91, 760],
            100,
            Some(features::hash64(&format!("{docset_id}|{d}"))),
        );
        let created = format!("202{}-0{}-1{}T1{}:20:00Z", d % 4 + 1, d % 8 + 1, g % 9, d % 9);
        // 唯一惰性部件：打乱 zip 条目名集合 → zip_entry_fp 各份互异（避免假「打包结构相同」）。
        let note = format!("word/note_{tag:04X}.xml");
        let parts = vec![
            ("[Content_Types].xml".to_string(), content_types_xml().as_bytes().to_vec()),
            ("word/document.xml".to_string(), doc_xml(&paragraphs, &rows)),
            ("docProps/core.xml".to_string(), core_xml(company, company, &created).into_bytes()),
            ("docProps/app.xml".to_string(), app_xml(None).into_bytes()),
            ("word/settings.xml".to_string(), settings_xml(&rsids, &root).into_bytes()),
            (note, br#"<?xml version="1.0"?><note/>"#.to_vec()),
            ("word/media/image1.png".to_string(), png),
        ];
        docs.push(GeneratedDoc { name: format!("bid_{}.docx", d + 1), bytes: build_docx(&parts) });
    }
    GeneratedDocset {
        manifest: DocsetManifest {
            docset_id,
            label: "independent".to_string(),
            docs: docs.iter().map(|d| d.name.clone()).collect(),
            planted_signals: Vec::new(),
        },
        docs,
    }
}

/// 完整正文 XML 字节（段落 + 报价表）。
fn doc_xml(paragraphs: &[String], price_rows: &[Vec<String>]) -> Vec<u8> {
    format!(
        r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{}</w:body></w:document>"#,
        body_xml(paragraphs, price_rows)
    )
    .into_bytes()
}

/// 生成全部文档集（确定性）：先 GROUPS 组围标正样本，再 GROUPS 组独立负样本。
pub fn generate_docsets() -> Vec<GeneratedDocset> {
    let jieba = Jieba::new();
    let syn = SynonymTable::load();
    let bases = load_bases();
    assert!(
        bases.len() >= DOCS_PER_SET * 2,
        "not enough seed paragraphs for docsets under {:?}",
        seeds_dir()
    );
    let mut out = Vec::new();
    for g in 0..DOCSET_GROUPS {
        out.push(build_collusion_set(g, &bases, &jieba, &syn));
    }
    for g in 0..DOCSET_GROUPS {
        out.push(build_independent_set(g, &bases, &jieba, &syn));
    }
    out
}

/// 文档集清单 → JSONL（一行一 manifest，末尾换行）。
pub fn docsets_to_jsonl(sets: &[GeneratedDocset]) -> String {
    let mut s = String::new();
    for set in sets {
        s.push_str(&serde_json::to_string(&set.manifest).expect("serialize DocsetManifest"));
        s.push('\n');
    }
    s
}

fn default_pairs_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/corpus/pairs.jsonl")
}

fn default_docsets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/corpus/docsets")
}

fn default_docsets_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/corpus/docsets.jsonl")
}

/// 写段对语料到 out（默认仓库 fixtures/corpus/pairs.jsonl）。
fn write_pairs_to(out: &Path) {
    let records = generate_pairs();
    let body = to_jsonl(&records);
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(out, body).unwrap_or_else(|e| panic!("write {out:?}: {e}"));
    let mut counts: std::collections::BTreeMap<String, usize> = Default::default();
    for r in &records {
        *counts.entry(r.label.clone()).or_default() += 1;
    }
    eprintln!("corpusgen: wrote {} pairs -> {}", records.len(), out.display());
    for (k, v) in counts {
        eprintln!("  {k}: {v}");
    }
}

/// 写文档集到 dir（docsets/<docset_id>/*.docx）+ 清单 manifest（docsets.jsonl）。
/// 先清空 docsets 目录再重建，避免旧组残留；产物由固定种子派生 → 两次运行逐字节一致。
fn write_docsets_to(dir: &Path, manifest_path: &Path) {
    let sets = generate_docsets();
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap_or_else(|e| panic!("mkdir {dir:?}: {e}"));
    for set in &sets {
        let sdir = dir.join(&set.manifest.docset_id);
        std::fs::create_dir_all(&sdir).unwrap_or_else(|e| panic!("mkdir {sdir:?}: {e}"));
        for doc in &set.docs {
            let p = sdir.join(&doc.name);
            std::fs::write(&p, &doc.bytes).unwrap_or_else(|e| panic!("write {p:?}: {e}"));
        }
    }
    let manifest = docsets_to_jsonl(&sets);
    if let Some(parent) = manifest_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(manifest_path, manifest)
        .unwrap_or_else(|e| panic!("write {manifest_path:?}: {e}"));
    let (col, ind) = sets.iter().fold((0, 0), |(c, i), s| match s.manifest.label.as_str() {
        "collusion" => (c + 1, i),
        _ => (c, i + 1),
    });
    let ndocs: usize = sets.iter().map(|s| s.docs.len()).sum();
    eprintln!(
        "corpusgen: wrote {} docsets ({} collusion / {} independent, {} docx) -> {}",
        sets.len(),
        col,
        ind,
        ndocs,
        dir.display()
    );
}

/// dev-tools bin 入口：子命令分发。
///   corpusgen                → 生成 pairs.jsonl + docsets（默认全量）
///   corpusgen pairs [路径]    → 仅段对语料
///   corpusgen docsets [目录]  → 仅文档集（清单固定为 docsets.jsonl）
///   corpusgen <路径>          → 向后兼容：裸路径视为 pairs 输出路径
pub fn run_cli() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(|s| s.as_str()) {
        None => {
            write_pairs_to(&default_pairs_path());
            write_docsets_to(&default_docsets_dir(), &default_docsets_manifest());
        }
        Some("pairs") => {
            let out = args.get(2).map(PathBuf::from).unwrap_or_else(default_pairs_path);
            write_pairs_to(&out);
        }
        Some("docsets") => {
            let dir = args.get(2).map(PathBuf::from).unwrap_or_else(default_docsets_dir);
            let manifest = dir.parent().map(|p| p.join("docsets.jsonl")).unwrap_or_else(default_docsets_manifest);
            write_docsets_to(&dir, &manifest);
        }
        Some("fit-collusion") => {
            let out = args.get(2).map(PathBuf::from).unwrap_or_else(default_lr_path);
            write_collusion_lr_to(&out);
        }
        Some("fit-calib") => {
            let out = args.get(2).map(PathBuf::from).unwrap_or_else(default_calib_path);
            write_calib_to(&out);
        }
        Some(other) if !other.starts_with('-') => write_pairs_to(&PathBuf::from(other)),
        Some(_) => {
            write_pairs_to(&default_pairs_path());
            write_docsets_to(&default_docsets_dir(), &default_docsets_manifest());
        }
    }
}

// ————————————————————————————————————————————————————————————————————————
// 回归测试基线（执行方案 §8 W6-5）
//
// 无模型层（features→candidate::recall→scoring::score_pair→diff::classify_cluster→
// collusion::assess_with）跑全语料，产出三层机械指标：召回层召回率、评分层 per-label
// P/R/F1、围标层 AUC。与 fixtures/corpus/baseline_metrics.json 逐项对比，任一 F1 绝对降
// >2pp / 召回率降 >1pp / AUC 降 >0.03 即门禁失败。
//
// 语料 hash：baseline 记录 pairs.jsonl 与 docsets（清单+全部 docx 字节）的内容 hash，
// 比指标前先校验——不匹配说明语料被改而基线没重生成，给出明确修复命令。
//
// 慢档（corpus_regression_full，#[ignore]）追加语义层，沿用 BIDGUARD_EMBED_DIR。
// 局限（§8 风险①）：合成语料指标系统性偏乐观，是【下界回归护栏】而非真实检出率。
// ————————————————————————————————————————————————————————————————————————

/// docsets 组内聚类的相似阈值：对齐 compare 默认 similarity_threshold（config.rs=0.7）。
const REGRESSION_THRESHOLD: f32 = 0.7;
/// 数值层逐项雷同率告警线：对齐 compare 默认 identical_rate_alarm（config.rs=0.80）。
const REGRESSION_ALARM_LINE: f64 = 0.80;
/// 计入「召回层召回率」的正类标签（unrelated 是负类，不计召回）。
const POSITIVE_LABELS: [&str; 4] = ["same", "minor_change", "changed", "rewrite"];
/// 评分层 per-label 指标覆盖的全部五类（含负类 unrelated）。
const ALL_LABELS: [&str; 5] = ["same", "minor_change", "changed", "rewrite", "unrelated"];

/// 评分层单标签 P/R/F1（support = 该真标签样本数）。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LabelMetric {
    pub precision: f64,
    pub recall: f64,
    pub f1: f64,
    pub support: usize,
}

/// 复核路由三带指标（W6-4）：把【随包生效的 score_calib.json】作用在全量段对上，
/// 量出「低优先级抽查带漏了多少正样本」「重点标红带误收了多少负样本」「复核带有多宽」。
/// 与拟合侧留出集指标的区别：这里跑【全量语料 + 落盘后的最终参数】，是门禁口径——
/// 任何改动 normalize/features/scoring 或换校准文件都会在这三个数上显形。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BandMetrics {
    /// three-band = 分流生效；review-all = 分流未启用（全部落需人工复核）；空 = 无校准文件。
    pub routing: String,
    /// 目标漏检率/误报率（【在合成校准语料上测得】）。
    pub alpha: f64,
    pub beta: f64,
    /// 正样本落入「低优先级抽查」带的比例（漏检率）。
    pub pass_fnr: f64,
    /// 负样本落入「重点标红」带的比例（误报率）。
    pub flag_fpr: f64,
    pub pass_share: f64,
    pub review_share: f64,
    pub flag_share: f64,
}

/// 一次回归评测的分层指标 + 语料 hash + 生成元数据（写入/读取 baseline_metrics.json）。
#[derive(Serialize, Deserialize, Clone, Default, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RegressionMetrics {
    /// fast（无模型层，进 CI）| full（追加语义层，本地手动）。
    pub lane: String,
    /// 召回层召回率：正类段对中被 candidate::recall 命中的比例。
    pub recall_rate: f64,
    /// 各正类标签的召回率。
    pub recall_by_label: BTreeMap<String, f64>,
    /// 评分层各标签 P/R/F1（预测标签 = classify_cluster 结果，uncertain→unrelated）。
    pub labels: BTreeMap<String, LabelMetric>,
    /// 五类 F1 宏平均（速览用，不单独设门禁）。
    pub macro_f1: f64,
    /// 复核路由三带层（W6-4）：随包 score_calib.json 作用在全量段对上的分带结果。
    /// 旧基线缺该块 → serde 默认全零（首次跑门禁会因漂移提示重新入库基线）。
    #[serde(default)]
    pub bands: BandMetrics,
    /// 围标层 AUC：docsets 上 collusion score 对 collusion(正)/independent(负) 的判别力。
    pub collusion_auc: f64,
    pub mean_collusion_score: f64,
    pub mean_independent_score: f64,
    pub pairs_count: usize,
    pub docsets_count: usize,
    /// pairs.jsonl 内容 hash。
    pub pairs_hash: String,
    /// docsets 内容 hash（清单 + 全部 docx 字节）。
    pub docsets_hash: String,
    pub git_rev: String,
    pub generated_at: String,
    pub note: String,
}

/// 真标签字符串 → 五类之一的 'static 名（未知归 unrelated），供混淆矩阵键零分配。
fn static_label(s: &str) -> &'static str {
    match s {
        "same" => "same",
        "minor_change" => "minor_change",
        "changed" => "changed",
        "rewrite" => "rewrite",
        _ => "unrelated",
    }
}

/// classify_cluster 的 cluster_type → 五类预测标签（uncertain/其它 → unrelated）。
fn map_pred(cluster_type: &str) -> &'static str {
    match cluster_type {
        "same" => "same",
        "minor_change" => "minor_change",
        "changed" => "changed",
        "rewrite" => "rewrite",
        _ => "unrelated",
    }
}

/// 文本 → 无模型层 CmpChunk（严格对齐 chunker::make 的特征口径：sanitize→normalize→
/// tokenize_lang→extract_entities→char_ngrams→minhash；section_kind 由 segment::classify_zone）。
fn regr_build_chunk(
    jieba: &Jieba,
    id: String,
    doc: usize,
    rel_pos: f32,
    text: &str,
    section_path: Vec<String>,
) -> CmpChunk {
    use crate::engine::normalize::{self, NormalizeOptions};
    use crate::engine::{segment, similarity};
    let (sanitized, _st) = normalize::sanitize_with_stats(text);
    let normalized = normalize::normalize_sanitized(&sanitized, &NormalizeOptions::default());
    let tokens = similarity::tokenize_lang(jieba, &sanitized, "auto");
    let entities = features::extract_entities(&normalized);
    // 五区分类（§5 W3-5）：与 chunker::make / corpus::from_row 同口径（标题优先、金额表格行→price），
    // 使回归语料反映分区阈值分层。regr 语料无表格行，is_table_row 恒 false。
    let has_amount = entities.iter().any(|e| e.kind == "amount");
    let section_kind = segment::section_kind_str(segment::classify_zone(&section_path, text, false, has_amount));
    let ngrams = features::char_ngrams(&normalized);
    let minhash = features::minhash(&ngrams);
    CmpChunk {
        id,
        doc,
        rel_pos,
        page: None,
        exact_hash: normalize::sha256_hex(text.as_bytes()),
        normalized_hash: normalize::sha256_hex(normalized.as_bytes()),
        section_path,
        section_kind: section_kind.to_string(),
        is_template: false,
        is_table_row: false,
        char_count: text.chars().count(),
        tokens,
        ngrams,
        minhash,
        entities,
        tfidf: HashMap::new(),
        tender_coverage: 0.0,
        boiler_fraction: 0.0,
        text: text.to_string(),
    }
}

/// 段对语料 → 两侧分块（TF-IDF 已按【全语料 IDF】填好）+ 可选语义。
///
/// 先建齐全部段对的两侧分块，再用全语料 IDF 填 TF-IDF——与生产 fill_tfidf 同口径。
/// 关键：逐对局部 IDF 会让 lexical(tfidf 余弦) 与 char_ngram(jaccard) 退化为近似同一信号，
/// W_LEXICAL/W_CHAR_NGRAM 的权重再分配对最终分几乎无影响（门禁对权重改动失灵）；全语料 IDF
/// 下常见模板词权重被压低，lexical 变为「区分性词」信号，与 ngram 分离，权重改动才真正传导。
///
/// 回归指标（pair_stats）与概率校准拟合（calib_samples）共用本函数：拟合所依据的分与门禁
/// 所测的分必须出自同一口径，否则校准阈值与运行时打分脱节。
#[allow(clippy::type_complexity)]
fn build_pair_chunks<F>(
    jieba: &Jieba,
    pairs: &[PairRecord],
    mut sem_provider: F,
) -> Vec<(Vec<CmpChunk>, Option<f32>, Option<Vec<Option<Vec<f32>>>>)>
where
    F: FnMut(&str, &str) -> (Option<f32>, Option<Vec<Option<Vec<f32>>>>),
{
    let mut built: Vec<(Vec<CmpChunk>, Option<f32>, Option<Vec<Option<Vec<f32>>>>)> =
        Vec::with_capacity(pairs.len());
    for r in pairs {
        let (sem_cos, embs) = sem_provider(&r.text_a, &r.text_b);
        let a = regr_build_chunk(jieba, format!("{}-a", r.id), 0, 0.0, &r.text_a, Vec::new());
        let b = regr_build_chunk(jieba, format!("{}-b", r.id), 1, 0.0, &r.text_b, Vec::new());
        built.push((vec![a, b], sem_cos, embs));
    }
    let idf = {
        let lists = built.iter().flat_map(|(v, _, _)| v.iter().map(|c| c.tokens.as_slice()));
        features::idf_of(lists)
    };
    for (v, _, _) in built.iter_mut() {
        v[0].tfidf = features::weighted_vec(&v[0].tokens, &idf);
        v[1].tfidf = features::weighted_vec(&v[1].tokens, &idf);
    }
    built
}

/// 段对层指标：对每个段对建两块 → fill_tfidf → recall 命中判定 + score_pair + classify_cluster
/// 混淆统计。sem_provider 提供可选语义（fast 恒返回 (None,None)；full 返回余弦 + 两侧向量）。
#[allow(clippy::type_complexity)]
fn pair_stats<F>(
    jieba: &Jieba,
    pairs: &[PairRecord],
    mut sem_provider: F,
) -> (f64, BTreeMap<String, f64>, BTreeMap<String, LabelMetric>, f64)
where
    F: FnMut(&str, &str) -> (Option<f32>, Option<Vec<Option<Vec<f32>>>>),
{
    let params = candidate::RecallParams::default();
    let mut recalled: BTreeMap<&str, usize> = BTreeMap::new();
    let mut total_lbl: BTreeMap<&str, usize> = BTreeMap::new();
    let mut conf: HashMap<(&str, &str), usize> = HashMap::new();

    let mut built = build_pair_chunks(jieba, pairs, &mut sem_provider);
    for ((v, sem_cos, embs), r) in built.iter_mut().zip(pairs) {
        let tl = static_label(&r.label);
        *total_lbl.entry(tl).or_default() += 1;
        let got = candidate::recall(v, embs.as_deref(), &params);
        if got.contains(&(0, 1)) && POSITIVE_LABELS.contains(&tl) {
            *recalled.entry(tl).or_default() += 1;
        }
        let parts = scoring::score_pair(&v[0], &v[1], *sem_cos);
        let same_hash = v[0].normalized_hash == v[1].normalized_hash;
        let cls =
            diff::classify_cluster(parts.final_score, parts.final_score, same_hash, parts.lexical, *sem_cos);
        *conf.entry((tl, map_pred(cls.cluster_type))).or_default() += 1;
    }
    // 召回层召回率（正类整体 + 分标签）
    let pos_total: usize = POSITIVE_LABELS.iter().map(|l| total_lbl.get(l).copied().unwrap_or(0)).sum();
    let pos_hit: usize = POSITIVE_LABELS.iter().map(|l| recalled.get(l).copied().unwrap_or(0)).sum();
    let recall_rate = if pos_total > 0 { pos_hit as f64 / pos_total as f64 } else { 0.0 };
    let mut recall_by_label = BTreeMap::new();
    for l in POSITIVE_LABELS {
        let t = total_lbl.get(l).copied().unwrap_or(0);
        let h = recalled.get(l).copied().unwrap_or(0);
        recall_by_label.insert(l.to_string(), if t > 0 { h as f64 / t as f64 } else { 0.0 });
    }
    // 评分层 per-label P/R/F1（混淆矩阵）
    let mut labels = BTreeMap::new();
    let mut macro_sum = 0.0f64;
    for l in ALL_LABELS {
        let tp = conf.get(&(l, l)).copied().unwrap_or(0);
        let pred_total: usize = ALL_LABELS.iter().map(|x| conf.get(&(x, l)).copied().unwrap_or(0)).sum();
        let true_total: usize = ALL_LABELS.iter().map(|x| conf.get(&(l, x)).copied().unwrap_or(0)).sum();
        let precision = if pred_total > 0 { tp as f64 / pred_total as f64 } else { 0.0 };
        let recall = if true_total > 0 { tp as f64 / true_total as f64 } else { 0.0 };
        let f1 = if precision + recall > 0.0 { 2.0 * precision * recall / (precision + recall) } else { 0.0 };
        macro_sum += f1;
        labels.insert(l.to_string(), LabelMetric { precision, recall, f1, support: true_total });
    }
    let macro_f1 = macro_sum / ALL_LABELS.len() as f64;
    (recall_rate, recall_by_label, labels, macro_f1)
}

/// 文档级隐形码点摘要（无 M2 全管线，用 sanitize_with_stats 逐文档聚合后交 grade 判级）。
fn regr_evasion(text: &str) -> Option<EvasionSummary> {
    let (_, st) = crate::engine::normalize::sanitize_with_stats(text);
    if st.is_clean() {
        return None;
    }
    let denom = text.chars().count().max(1) as f64;
    let json = serde_json::json!({
        "zeroWidth": st.zero_width,
        "bidi": st.bidi,
        "tags": st.tags,
        "variation": st.variation,
        "confusableFolds": st.confusable_folds,
        "mixedScriptWords": st.mixed_script_words,
        "affectedChunks": 1,
        "maxChunkConcentration": st.perturbation_total() as f64 / denom,
    });
    EvasionSummary::from_evasion_json(&json.to_string())
}

/// 单个文档集的围标 score（无模型层）：解析各 docx → 无模型层文本管线得 peak/clusters +
/// 取证信号（rsid/血缘/图片同源）+ 元数据同源 + evasion → collusion::assess_with。
fn docset_score(jieba: &Jieba, dir: &Path, manifest: &DocsetManifest) -> f32 {
    docset_eval(jieba, dir, &manifest.docset_id, &manifest.docs).0
}

/// 文档集（或其文档子集）的围标评估：返回 (score, 全量连续特征向量)。
/// 子集展开是 M7 拟合的样本来源之一——2–5 份是产品的真实使用形态，一组围标集的任意 2 份
/// 子集仍是围标样本、独立集的子集仍是独立样本，故子集可作为独立训练样本使用。
fn docset_eval(
    jieba: &Jieba,
    dir: &Path,
    docset_id: &str,
    doc_names: &[String],
) -> (f32, [f32; collusion::FEATURE_COUNT]) {
    use std::sync::atomic::AtomicBool;
    let cancel = AtomicBool::new(false);
    let sdir = dir.join("docsets").join(docset_id);
    let mut doc_infos: Vec<DocInfo> = Vec::new();
    let mut per_doc_images: Vec<Vec<collusion::ImageFp>> = Vec::new();
    let mut evasion: Vec<Option<EvasionSummary>> = Vec::new();
    let mut chunks: Vec<CmpChunk> = Vec::new();
    // 商务标数值层（M6）：docsets 的围标正样本注入了「清单单价整组乘系数」（见 price_rows），
    // 若此处不建 BOQ，numeric 恒 None ⇒ 五类数值信号在门禁里恒不触发、注入的等比证据被白白丢弃
    // （M7 拟合 LR 时数值特征会成为死列）。故与生产同口径抽取表格行 → extract → align → pair_stats。
    let mut per_doc_rows: Vec<Vec<crate::engine::boq::TableRowInput>> = Vec::new();
    for (di, name) in doc_names.iter().enumerate() {
        let path = sdir.join(name);
        let pb = parse::parse_file_blocks(&path, &cancel)
            .unwrap_or_else(|e| panic!("解析 {} 失败: {e}", path.display()));
        let blocks: Vec<&parse::Block> =
            pb.blocks.iter().filter(|b| b.text.chars().count() >= 2).collect();
        let total = blocks.len().max(1);
        let mut rows: Vec<crate::engine::boq::TableRowInput> = Vec::new();
        for (rank, b) in blocks.iter().enumerate() {
            if b.is_table_row {
                rows.push(crate::engine::boq::TableRowInput {
                    chunk_id: format!("{docset_id}-{di}-{rank}"),
                    text: b.text.clone(),
                    page: b.page.map(|p| p as i64),
                    order_index: rank as i64,
                });
            }
            let rel = if total > 1 { rank as f32 / (total - 1) as f32 } else { 0.0 };
            let mut c = regr_build_chunk(
                jieba,
                format!("{docset_id}-{di}-{rank}"),
                di,
                rel,
                &b.text,
                Vec::new(),
            );
            c.is_table_row = b.is_table_row;
            if !c.tokens.is_empty() {
                chunks.push(c);
            }
        }
        per_doc_images.push(
            pb.image_hashes
                .iter()
                .map(|h| collusion::ImageFp { sha256: h.sha256.clone(), dhash: h.dhash, page: h.page })
                .collect(),
        );
        per_doc_rows.push(rows);
        evasion.push(regr_evasion(&pb.legacy_text));
        doc_infos.push(DocInfo {
            id: format!("{docset_id}-{di}"),
            name: name.clone(),
            doc_type: "docx".into(),
            pages: pb.pages,
            char_count: pb.legacy_text.chars().count(),
            fingerprint: pb.fingerprint,
            parse_error: None,
            evasion: evasion[di].clone(),
        });
    }
    fill_tfidf(&mut chunks);
    let params = candidate::RecallParams {
        top_k: 100,
        stop_gram_df: (chunks.len() / 10).max(256),
        ..Default::default()
    };
    let cands = candidate::recall(&chunks, None, &params);
    let mut edges = Vec::new();
    for (i, j) in cands {
        let parts = scoring::score_pair(&chunks[i as usize], &chunks[j as usize], None);
        if parts.final_score >= REGRESSION_THRESHOLD {
            edges.push(clustering::ScoredEdge { a: i, b: j, parts });
        }
    }
    let raw = clustering::cluster(&chunks, &edges, REGRESSION_THRESHOLD);
    let (_m, peak) = matrix::doc_matrix(doc_infos.len(), &chunks, &raw);
    let r_clusters: Vec<RCluster> = raw
        .iter()
        .map(|rc| {
            let docs_set: BTreeSet<usize> = rc.members.iter().map(|&i| chunks[i as usize].doc).collect();
            RCluster {
                avg_score: rc.avg,
                peak: rc.peak,
                docs: docs_set.into_iter().collect(),
                segments: rc
                    .members
                    .iter()
                    .map(|&i| ClusterSeg { doc: chunks[i as usize].doc, text: chunks[i as usize].text.clone() })
                    .collect(),
                exempted: false,
                anomaly: false,
            }
        })
        .collect();
    fingerprint::cross_flags(&mut doc_infos);
    let empty: HashSet<String> = HashSet::new();
    let rsid = fingerprint::rsid_pairs(&mut doc_infos, &empty);
    let lineage = fingerprint::lineage_pairs(&mut doc_infos);
    let images = collusion::image_pairs(&per_doc_images, &empty);
    // 数值层聚合与生产同口径：extract → align → pair_stats → numeric_evidence_of（复用
    // compare_service 的聚合函数而非在此复制，避免门禁与生产口径漂移）。无表格行 ⇒ None，
    // 此时回落到旧「报价梯度」信号，与生产一致。
    let per_doc_items: Vec<Vec<crate::engine::boq::BoqItem>> = per_doc_rows
        .iter()
        .map(|rows| crate::engine::boq::extract_document(rows).items)
        .collect();
    let item_count: usize = per_doc_items.iter().map(|v| v.len()).sum();
    let numeric = (item_count > 0).then(|| {
        let aligned = crate::engine::boq::align(&per_doc_items);
        let pairs = crate::engine::boq::pair_stats(&per_doc_items, &aligned, REGRESSION_ALARM_LINE);
        crate::services::compare_service::numeric_evidence_of(&pairs, REGRESSION_ALARM_LINE)
    });
    let inputs = collusion::CollusionInputs {
        peak,
        clusters: &r_clusters,
        docs: &doc_infos,
        rsid_hits: &rsid,
        lineage_hits: &lineage,
        image_hits: &images,
        evasion: &evasion,
        numeric: numeric.as_ref(),
        ..Default::default()
    };
    let features = collusion::feature_vector(&inputs);
    (collusion::assess_with(inputs).score, features)
}

/// Mann-Whitney U（含并列平均秩）→ AUC；任一侧为空回落 0.5。
fn auc_score(pos: &[f64], neg: &[f64]) -> f64 {
    if pos.is_empty() || neg.is_empty() {
        return 0.5;
    }
    let mut all: Vec<(f64, bool)> =
        pos.iter().map(|&s| (s, true)).chain(neg.iter().map(|&s| (s, false))).collect();
    all.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let n = all.len();
    let mut ranks = vec![0f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i;
        while j + 1 < n && all[j + 1].0 == all[i].0 {
            j += 1;
        }
        let avg = ((i + 1) + (j + 1)) as f64 / 2.0; // 1-based 平均秩
        for r in ranks.iter_mut().take(j + 1).skip(i) {
            *r = avg;
        }
        i = j + 1;
    }
    let sum_pos: f64 = ranks.iter().zip(&all).filter(|(_, (_, p))| *p).map(|(r, _)| *r).sum();
    let (np, nn) = (pos.len() as f64, neg.len() as f64);
    (sum_pos - np * (np + 1.0) / 2.0) / (np * nn)
}

/// 围标层 AUC + 正负均分 + 组数（无模型层）。
fn docset_auc(jieba: &Jieba, dir: &Path) -> (f64, f64, f64, usize) {
    let manifests = read_docset_manifests(&dir.join("docsets.jsonl"));
    let (mut pos, mut neg) = (Vec::new(), Vec::new());
    for m in &manifests {
        let s = docset_score(jieba, dir, m) as f64;
        if m.label == "collusion" {
            pos.push(s);
        } else {
            neg.push(s);
        }
    }
    let mean = |v: &[f64]| if v.is_empty() { 0.0 } else { v.iter().sum::<f64>() / v.len() as f64 };
    (auc_score(&pos, &neg), mean(&pos), mean(&neg), manifests.len())
}

fn read_pairs(path: &Path) -> Vec<PairRecord> {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("解析 pairs 行失败: {e}")))
        .collect()
}

fn read_docset_manifests(path: &Path) -> Vec<DocsetManifest> {
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()));
    raw.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("解析 docsets 行失败: {e}")))
        .collect()
}

/// 文本夹具的换行归一：CRLF/CR → LF。守卫按【内容】而非字节判定，才能跨平台成立——
/// Windows 检出默认把文本转 CRLF，直接哈希原始字节会让同一提交在不同平台算出不同 hash
/// （CI 上表现为「语料已变更但基线未同步」的误报）。.gitattributes 已关闭 fixtures 的 EOL
/// 转换，这里是纵深防御：即便某处 checkout 设置漏网，守卫仍只对真实内容变更报警。
/// 仅用于文本夹具；docx 等二进制字节【不得】做此替换。
fn normalize_eol(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' => {
                out.push(b'\n');
                // CRLF 视作一个换行
                if bytes.get(i + 1) == Some(&b'\n') {
                    i += 1;
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    out
}

fn sha256_of_file(p: &Path) -> String {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", p.display()));
    crate::engine::normalize::sha256_hex(&normalize_eol(&bytes))
}

/// docsets 内容 hash：清单字节 ++ 逐组逐份（id + 文件名 + docx 字节）。
fn docsets_hash(dir: &Path) -> String {
    let manifest_path = dir.join("docsets.jsonl");
    // 清单是文本 → 换行归一（见 normalize_eol）；下面的 docx 是二进制，按原始字节参与。
    let mut buf = normalize_eol(&std::fs::read(&manifest_path).unwrap_or_default());
    for m in read_docset_manifests(&manifest_path) {
        for name in &m.docs {
            buf.extend_from_slice(m.docset_id.as_bytes());
            buf.extend_from_slice(name.as_bytes());
            let p = dir.join("docsets").join(&m.docset_id).join(name);
            buf.extend_from_slice(&std::fs::read(&p).unwrap_or_default());
        }
    }
    crate::engine::normalize::sha256_hex(&buf)
}

pub fn git_rev() -> String {
    std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

pub fn today_utc() -> String {
    std::process::Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| "unknown".into())
}

/// 语料目录（committed fixtures）。BIDGUARD_CALIB_DIR 只影响生成器种子，不影响回归读取
/// 的 committed 语料——基线与仓库内 pairs.jsonl/docsets 绑定。
pub fn corpus_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/corpus")
}

/// 三带指标：随包校准文件作用在全量段对上（无校准文件 → 全零 + routing 空串）。
pub fn band_metrics(jieba: &Jieba, pairs: &[PairRecord]) -> BandMetrics {
    let Some(model) = calibrate::active_calibration() else { return BandMetrics::default() };
    let samples = calib_samples(jieba, pairs);
    if samples.is_empty() {
        return BandMetrics::default();
    }
    let bands: Vec<&str> = samples.iter().map(|s| model.evaluate(s.score as f32).1).collect();
    let n = samples.len() as f64;
    let share = |b: &str| bands.iter().filter(|x| **x == b).count() as f64 / n;
    let rate = |want_pos: bool, b: &str| -> f64 {
        let idx: Vec<usize> =
            (0..samples.len()).filter(|&i| (samples[i].y > 0.5) == want_pos).collect();
        if idx.is_empty() {
            0.0
        } else {
            idx.iter().filter(|&&i| bands[i] == b).count() as f64 / idx.len() as f64
        }
    };
    BandMetrics {
        routing: model.routing.as_str().to_string(),
        // 定点化：α/β 由 f32 提升而来，直接写会带 f32 末位噪声（0.05000000074…），
        // 让基线文件的 diff 难读。
        alpha: round6_f64(model.alpha as f64),
        beta: round6_f64(model.beta as f64),
        pass_fnr: rate(true, calibrate::BAND_PASS),
        flag_fpr: rate(false, calibrate::BAND_FLAG),
        pass_share: share(calibrate::BAND_PASS),
        review_share: share(calibrate::BAND_REVIEW),
        flag_share: share(calibrate::BAND_FLAG),
    }
}

/// 无模型层跑全语料 → 分层指标（+ 语料 hash + count）。git_rev/generated_at 由调用方填。
pub fn compute_fast_metrics(jieba: &Jieba, dir: &Path) -> RegressionMetrics {
    let pairs = read_pairs(&dir.join("pairs.jsonl"));
    let (recall_rate, recall_by_label, labels, macro_f1) =
        pair_stats(jieba, &pairs, |_, _| (None, None));
    let bands = band_metrics(jieba, &pairs);
    let (auc, mean_pos, mean_neg, ndoc) = docset_auc(jieba, dir);
    RegressionMetrics {
        lane: "fast".into(),
        recall_rate,
        recall_by_label,
        labels,
        macro_f1,
        bands,
        collusion_auc: auc,
        mean_collusion_score: mean_pos,
        mean_independent_score: mean_neg,
        pairs_count: pairs.len(),
        docsets_count: ndoc,
        pairs_hash: sha256_of_file(&dir.join("pairs.jsonl")),
        docsets_hash: docsets_hash(dir),
        git_rev: String::new(),
        generated_at: String::new(),
        note: "无模型层：features→recall→score_pair→classify_cluster→assess_with（不含语义/rerank）"
            .into(),
    }
}

/// 门禁容差（执行方案 §8 验收①）。
const F1_TOL: f64 = 0.02; // per-label F1 漂移带（双向）
const RECALL_TOL: f64 = 0.01; // 召回率下降带（单向）
const AUC_TOL: f64 = 0.03; // AUC 下降带（单向）
/// 三带带内错误率漂移带（单向恶化）与复核带占比上限（执行方案 §8 验收②③）。
const BAND_TOL: f64 = 0.02;
const REVIEW_SHARE_MAX: f64 = 0.40;

/// 门禁判定：
/// - 召回率（整体 + 分标签）单向下降 >1pp、AUC 单向下降 >0.03 → 回退，失败。
/// - per-label F1 【双向漂移】>2pp → 失败：下降是回退；上升也失败是有意为之——合成语料上
///   「变好」可能是对生成器直觉过拟合、或掩盖了真实语料上的回退，且执行方案要求「指标变化
///   必须显式可见、不允许静默漂移」，故任何 >2pp 的 F1 变动都强制重新入库基线（评审可见）。
///   这也满足门禁灵敏度验收：W_LEXICAL 0.40→0.20 在本语料上抬升 minor_change F1 >4pp 而触发。
pub fn gate_failures(base: &RegressionMetrics, cur: &RegressionMetrics) -> Vec<String> {
    let mut f = Vec::new();
    if cur.recall_rate < base.recall_rate - RECALL_TOL {
        f.push(format!("召回层召回率下降 >1pp：{:.4} → {:.4}", base.recall_rate, cur.recall_rate));
    }
    for l in POSITIVE_LABELS {
        let b = base.recall_by_label.get(l).copied().unwrap_or(0.0);
        let c = cur.recall_by_label.get(l).copied().unwrap_or(0.0);
        if c < b - RECALL_TOL {
            f.push(format!("{l} 召回率下降 >1pp：{b:.4} → {c:.4}"));
        }
    }
    for l in ALL_LABELS {
        let b = base.labels.get(l).map(|m| m.f1).unwrap_or(0.0);
        let c = cur.labels.get(l).map(|m| m.f1).unwrap_or(0.0);
        if (c - b).abs() > F1_TOL {
            let dir = if c < b { "下降(回退)" } else { "上升(需重新入库基线)" };
            f.push(format!("{l} F1 漂移 >2pp {dir}：{b:.4} → {c:.4}"));
        }
    }
    if cur.collusion_auc < base.collusion_auc - AUC_TOL {
        f.push(format!("围标层 AUC 下降 >0.03：{:.4} → {:.4}", base.collusion_auc, cur.collusion_auc));
    }
    // 三带层（W6-4）：①分流模式变了必须显式重新入库基线（改 α 即改承诺语义，不容静默漂移）；
    // ②带内错误率相对基线恶化 >2pp；③【绝对线】分流生效时 pass 带漏检率 ≤α+2pp、flag 带
    // 误报率 ≤β+2pp、复核带占比 ≤40%（执行方案 §8 验收②③）。
    if cur.bands.routing != base.bands.routing {
        f.push(format!("三带分流模式变更：{} → {}", base.bands.routing, cur.bands.routing));
    }
    if cur.bands.pass_fnr > base.bands.pass_fnr + BAND_TOL {
        f.push(format!(
            "低优先级抽查带漏检率上升 >2pp：{:.4} → {:.4}",
            base.bands.pass_fnr, cur.bands.pass_fnr
        ));
    }
    if cur.bands.flag_fpr > base.bands.flag_fpr + BAND_TOL {
        f.push(format!(
            "重点标红带误报率上升 >2pp：{:.4} → {:.4}",
            base.bands.flag_fpr, cur.bands.flag_fpr
        ));
    }
    if cur.bands.routing == "three-band" {
        if cur.bands.pass_fnr > cur.bands.alpha + BAND_TOL {
            f.push(format!(
                "低优先级抽查带漏检率 {:.4} 超过 α+2pp（α={:.4}）",
                cur.bands.pass_fnr, cur.bands.alpha
            ));
        }
        if cur.bands.flag_fpr > cur.bands.beta + BAND_TOL {
            f.push(format!(
                "重点标红带误报率 {:.4} 超过 β+2pp（β={:.4}）",
                cur.bands.flag_fpr, cur.bands.beta
            ));
        }
        if cur.bands.review_share > REVIEW_SHARE_MAX {
            f.push(format!(
                "需人工复核带占比 {:.4} 超过上限 {REVIEW_SHARE_MAX}（放宽 α 或回退 review-all 并如实展示）",
                cur.bands.review_share
            ));
        }
    }
    f
}

/// 新旧指标对照全表（门禁失败或 --nocapture 时打印，便于定位）。
pub fn render_compare(base: &RegressionMetrics, cur: &RegressionMetrics) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(s, "== 回归指标对照（基线 {} / 当前 {}）==", base.lane, cur.lane);
    let _ = writeln!(s, "召回层召回率:      {:.4} → {:.4}", base.recall_rate, cur.recall_rate);
    for l in POSITIVE_LABELS {
        let b = base.recall_by_label.get(l).copied().unwrap_or(0.0);
        let c = cur.recall_by_label.get(l).copied().unwrap_or(0.0);
        let _ = writeln!(s, "  召回 {l:<13} {b:.4} → {c:.4}");
    }
    let _ = writeln!(s, "评分层 per-label F1（precision/recall/f1，support）:");
    for l in ALL_LABELS {
        let bm = base.labels.get(l).cloned().unwrap_or_default();
        let cm = cur.labels.get(l).cloned().unwrap_or_default();
        let _ = writeln!(
            s,
            "  {l:<13} P {:.3}→{:.3}  R {:.3}→{:.3}  F1 {:.4}→{:.4}  (n={})",
            bm.precision, cm.precision, bm.recall, cm.recall, bm.f1, cm.f1, cm.support
        );
    }
    let _ = writeln!(s, "macro-F1:          {:.4} → {:.4}", base.macro_f1, cur.macro_f1);
    let _ = writeln!(
        s,
        "三带层（分流 {} → {}）：漏检率 {:.4}→{:.4} / 误报率 {:.4}→{:.4} / 复核带占比 {:.4}→{:.4}",
        if base.bands.routing.is_empty() { "—" } else { &base.bands.routing },
        if cur.bands.routing.is_empty() { "—" } else { &cur.bands.routing },
        base.bands.pass_fnr, cur.bands.pass_fnr,
        base.bands.flag_fpr, cur.bands.flag_fpr,
        base.bands.review_share, cur.bands.review_share
    );
    let _ = writeln!(
        s,
        "围标层 AUC:        {:.4} → {:.4}  (collusion均分 {:.3}→{:.3} / independent均分 {:.3}→{:.3})",
        base.collusion_auc, cur.collusion_auc,
        base.mean_collusion_score, cur.mean_collusion_score,
        base.mean_independent_score, cur.mean_independent_score
    );
    s
}

/// 单份指标速览（写基线后打印）。
pub fn render_single(m: &RegressionMetrics) -> String {
    render_compare(m, m)
}

pub fn baseline_write_mode() -> bool {
    std::env::var("BIDGUARD_WRITE_BASELINE").ok().as_deref() == Some("1")
}

/// 语料 hash 校验：baseline 与当前语料的 pairs/docsets 内容 hash 不一致 → 返回含【修复命令】
/// 的报错（说明语料被改而基线没重生成）；一致 → None。抽成纯函数便于单测（验收⑤）。
pub fn hash_mismatch_message(base: &RegressionMetrics, cur: &RegressionMetrics) -> Option<String> {
    if base.pairs_hash == cur.pairs_hash && base.docsets_hash == cur.docsets_hash {
        return None;
    }
    fn short8(s: &str) -> &str {
        &s[..s.len().min(8)]
    }
    Some(format!(
        "语料与基线 hash 不一致（pairs {}… vs {}…；docsets {}… vs {}…）：\
         fixtures/corpus 已变更但 baseline_metrics.json 未同步。\n\
         修复：BIDGUARD_WRITE_BASELINE=1 cargo test --manifest-path src-tauri/Cargo.toml --lib --features dev-tools corpus_regression",
        short8(&base.pairs_hash),
        short8(&cur.pairs_hash),
        short8(&base.docsets_hash),
        short8(&cur.docsets_hash),
    ))
}

// ————————————————————————————————————————————————————————————————————————
// 围标信号融合拟合（执行方案 §8 W6-3 / M7）：docsets 语料 → log-LR 权重
//
// 与设计的偏差（已在 §1.2 裁决内）：原设计只拟合「旧五信号」，此处对 M1–M6 全量 16 类特征
// 一次性拟合。三点工程约束写在这里，评审时一并看：
//  ① 样本量：docsets 只有 12 组，16 列拟合会退化。故按【文档子集展开】补样本——一组围标集
//     的任意 2 份子集仍是围标样本，独立集子集仍是独立样本，且 2–5 份正是产品真实形态。
//  ② 先验：L2 向【v1 经验权重】收缩而非向 0 收缩。向 0 收缩会让语料里没有区分度的列静默死掉
//     （例如「报价梯度」只在无 BOQ 时才出，docsets 恒有 BOQ ⇒ 该列全零），上线即等于删信号。
//     向经验先验收缩时，无数据的列自然保留经验权重，有数据的列才被语料推动（MAP 估计）。
//  ③ 符号：牛顿步后投影到 [0, 上限]，从构造上杜绝负权重（§1.5-4：负权重在监管场景解释不通），
//     运行时 parse_lr_model 再审查一次。
//
// 合成语料的系统性乐观（§8 风险①/④）照旧成立：权重文件标 calibrationKind=
// experimental-synthetic，真实判例回测前不摘实验性标签。
// ————————————————————————————————————————————————————————————————————————

/// 一条拟合样本：全量连续特征 + 标签（1=collusion / 0=independent）。
pub struct FitSample {
    pub id: String,
    pub x: Vec<f64>,
    pub y: f64,
}

/// L2 强度（向经验先验收缩）。取值依据：λ→0 时 16 列在 48 样本上近可分、权重发散；λ 过大则
/// 语料完全不起作用。0.5 是在留出集 AUC 与「文本层单证据仍达 medium」的产品锚点之间的取值。
const FIT_LAMBDA: f64 = 0.5;
/// 牛顿迭代上限与收敛阈。
const FIT_ITERS: usize = 200;
const FIT_TOL: f64 = 1e-9;
/// 留出集比例：每 5 条取 1 条（按标签分层、按固定序号取，确定性可复现）。
const FIT_HOLDOUT_EVERY: usize = 5;
/// 权重上限（与 collusion::parse_lr_model 的量级审查一致，投影时用）。
const FIT_WEIGHT_MAX: f64 = 20.0;
const FIT_INTERCEPT_MIN: f64 = -40.0;
/// 截距上限：必须为负（零证据不得抬底分，验收④）。
const FIT_INTERCEPT_MAX: f64 = -0.05;

/// docsets → 拟合样本（全集 + 各 2 份子集）。顺序固定 ⇒ 拟合结果可复现。
pub fn collusion_fit_samples(jieba: &Jieba, dir: &Path) -> Vec<FitSample> {
    let manifests = read_docset_manifests(&dir.join("docsets.jsonl"));
    let mut out = Vec::new();
    for m in &manifests {
        let y = if m.label == "collusion" { 1.0 } else { 0.0 };
        let mut combos: Vec<(String, Vec<String>)> = vec![("all".to_string(), m.docs.clone())];
        for i in 0..m.docs.len() {
            for j in (i + 1)..m.docs.len() {
                combos.push((format!("{i}{j}"), vec![m.docs[i].clone(), m.docs[j].clone()]));
            }
        }
        for (tag, docs) in combos {
            let (_, feats) = docset_eval(jieba, dir, &m.docset_id, &docs);
            out.push(FitSample {
                id: format!("{}#{tag}", m.docset_id),
                x: feats.iter().map(|v| *v as f64).collect(),
                y,
            });
        }
    }
    out
}

/// 高斯消元（部分主元）解 H·δ = g；H 奇异时返回 None。
fn solve_linear(mut h: Vec<Vec<f64>>, mut g: Vec<f64>) -> Option<Vec<f64>> {
    let n = g.len();
    for c in 0..n {
        let (mut piv, mut best) = (c, h[c][c].abs());
        for (r, row) in h.iter().enumerate().skip(c + 1) {
            if row[c].abs() > best {
                best = row[c].abs();
                piv = r;
            }
        }
        if best < 1e-12 {
            return None;
        }
        h.swap(c, piv);
        g.swap(c, piv);
        for r in (c + 1)..n {
            let f = h[r][c] / h[c][c];
            if f == 0.0 {
                continue;
            }
            let (upper, lower) = h.split_at_mut(r);
            let pivot = &upper[c];
            for (k, v) in lower[0].iter_mut().enumerate().skip(c) {
                *v -= f * pivot[k];
            }
            g[r] -= f * g[c];
        }
    }
    let mut out = vec![0.0; n];
    for c in (0..n).rev() {
        let mut s = g[c];
        for k in (c + 1)..n {
            s -= h[c][k] * out[k];
        }
        out[c] = s / h[c][c];
    }
    Some(out)
}

/// 逻辑回归 MAP 估计：最大化 Σ logLik − (λ/2)‖θ−θ₀‖²，牛顿法（IRLS）+ 每步投影
/// （权重非负且有上限、截距为负）。θ[0] 为截距，θ[1..] 与 FEATURE_KINDS 同序。
pub fn fit_logistic(samples: &[FitSample], prior: &[f64], lambda: f64) -> Vec<f64> {
    let d = prior.len();
    let mut theta = prior.to_vec();
    for _ in 0..FIT_ITERS {
        let mut grad = vec![0.0f64; d];
        let mut hess = vec![vec![0.0f64; d]; d];
        for s in samples {
            let mut z = theta[0];
            for (k, xi) in s.x.iter().enumerate() {
                z += theta[k + 1] * xi;
            }
            let p = 1.0 / (1.0 + (-z).exp());
            let w = (p * (1.0 - p)).max(1e-9);
            let xi: Vec<f64> = std::iter::once(1.0).chain(s.x.iter().copied()).collect();
            for a in 0..d {
                grad[a] += (s.y - p) * xi[a];
                for b in 0..d {
                    hess[a][b] += w * xi[a] * xi[b];
                }
            }
        }
        for a in 0..d {
            grad[a] -= lambda * (theta[a] - prior[a]);
            hess[a][a] += lambda;
        }
        let Some(step) = solve_linear(hess, grad) else { break };
        let mut delta = 0.0f64;
        for a in 0..d {
            let next = theta[a] + step[a];
            let clamped = if a == 0 {
                next.clamp(FIT_INTERCEPT_MIN, FIT_INTERCEPT_MAX)
            } else {
                next.clamp(0.0, FIT_WEIGHT_MAX)
            };
            delta = delta.max((clamped - theta[a]).abs());
            theta[a] = clamped;
        }
        if delta < FIT_TOL {
            break;
        }
    }
    theta
}

/// Cllr（对数似然比代价，法庭比对标准指标）：0 = 完美，1 = 与「不给信息」等价。
/// LR 由后验概率在等先验下换算：LR = p/(1−p)。
pub fn cllr(pos: &[f64], neg: &[f64]) -> f64 {
    if pos.is_empty() || neg.is_empty() {
        return f64::NAN;
    }
    let clamp = |p: f64| p.clamp(1e-9, 1.0 - 1e-9);
    let a: f64 = pos.iter().map(|&p| {
        let lr = clamp(p) / (1.0 - clamp(p));
        (1.0 + 1.0 / lr).log2()
    }).sum::<f64>() / pos.len() as f64;
    let b: f64 = neg.iter().map(|&p| {
        let lr = clamp(p) / (1.0 - clamp(p));
        (1.0 + lr).log2()
    }).sum::<f64>() / neg.len() as f64;
    0.5 * (a + b)
}

/// 拟合报告（打印 + 写入权重文件的台账段）。
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct FitReport {
    pub lambda: f64,
    pub samples: usize,
    pub train_samples: usize,
    pub holdout_samples: usize,
    pub train_auc: f64,
    pub holdout_auc: f64,
    pub holdout_cllr: f64,
    /// 线性基线（v1 经验权重经同一 σ 通道）的留出集 Cllr，供「Cllr < 线性基线」验收对照。
    pub holdout_cllr_linear_baseline: f64,
    /// 语料中无区分度（全零/零方差）的特征列：这些列的权重保持经验先验，不由语料决定。
    pub dead_columns: Vec<String>,
    /// 各特征列在训练集正/负类上的均值——【人工审查系数符号与量级的依据】（§1.5-4）：
    /// 正类均值明显高于负类却拿到小权重、或反之，都是需要人工介入的信号。
    pub feature_mean_positive: BTreeMap<String, f64>,
    pub feature_mean_negative: BTreeMap<String, f64>,
    /// 分级线来源：v1-lines-verified（留出集验证后沿用 v1 等级语义）| holdout-derived。
    pub level_source: String,
    pub docsets_hash: String,
    pub pairs_hash: String,
    pub git_rev: String,
    pub fitted_at: String,
}

/// 在 docsets 语料上拟合融合权重并写出 collusion_lr.json；返回 (模型, 报告)。
pub fn fit_collusion(jieba: &Jieba, dir: &Path) -> (collusion::LrModel, FitReport) {
    let samples = collusion_fit_samples(jieba, dir);
    let prior_model = collusion::empirical_prior();
    let mut prior: Vec<f64> = Vec::with_capacity(collusion::FEATURE_COUNT + 1);
    prior.push(prior_model.intercept as f64);
    prior.extend(prior_model.weights.iter().map(|w| *w as f64));

    // 分层 8/2 切分：同标签内按固定序号每 5 取 1 进留出集（确定性）。
    let (mut train, mut holdout): (Vec<&FitSample>, Vec<&FitSample>) = (Vec::new(), Vec::new());
    let (mut np, mut nn) = (0usize, 0usize);
    for s in &samples {
        let seq = if s.y > 0.5 {
            np += 1;
            np
        } else {
            nn += 1;
            nn
        };
        if seq % FIT_HOLDOUT_EVERY == 0 {
            holdout.push(s);
        } else {
            train.push(s);
        }
    }
    let train_owned: Vec<FitSample> = train
        .iter()
        .map(|s| FitSample { id: s.id.clone(), x: s.x.clone(), y: s.y })
        .collect();
    let theta = fit_logistic(&train_owned, &prior, FIT_LAMBDA);

    // 死列检测（列在训练集上全零 ⇒ 语料没给任何信息）：权重回落经验先验，避免上线即删信号。
    // 系数在此就地按 6 位小数定点化：与落盘精度一致 ⇒ 分级线由【落盘后的同一组系数】算出，
    // 不会因写文件的舍入而与运行时求值错位。
    let round6 = |v: f64| ((v * 1e6).round() / 1e6) as f32;
    let mut dead_columns = Vec::new();
    let mut weights = [0f32; collusion::FEATURE_COUNT];
    for (i, kind) in collusion::FEATURE_KINDS.iter().enumerate() {
        let col_max = train_owned.iter().map(|s| s.x[i]).fold(0.0f64, f64::max);
        if col_max <= 0.0 {
            dead_columns.push((*kind).to_string());
            weights[i] = round6(prior[i + 1]);
        } else {
            weights[i] = round6(theta[i + 1]);
        }
    }
    let intercept = round6(theta[0]);

    // 分级线：先取 v1 三线在【本模型 score 尺度】上的等效位置（v1_line_equivalent：证据量按
    // 经验尺度换算，保证「尺子没变、只是权重被语料调整」），再在留出集上验证——正样本不得掉
    // 到 medium 线下、负样本不得越过 medium 线；不满足才按留出集重定。
    let probe = collusion::LrModel::from_parts(
        collusion::CALIBRATION_EXPERIMENTAL,
        "probe",
        intercept,
        weights,
        (0.9, 0.5, 0.1),
    );
    let v1_lines = (
        probe.v1_line_equivalent(collusion::LEVEL_HIGH),
        probe.v1_line_equivalent(collusion::LEVEL_MEDIUM),
        probe.v1_line_equivalent(collusion::LEVEL_LOW),
    );
    let strength_of = |m: &collusion::LrModel, s: &FitSample| -> f64 {
        let mut x = [0f32; collusion::FEATURE_COUNT];
        for (i, v) in s.x.iter().enumerate() {
            x[i] = *v as f32;
        }
        m.evaluate(&x).1 as f64
    };
    let hpos: Vec<f64> = holdout.iter().filter(|s| s.y > 0.5).map(|s| strength_of(&probe, s)).collect();
    let hneg: Vec<f64> = holdout.iter().filter(|s| s.y <= 0.5).map(|s| strength_of(&probe, s)).collect();
    let min_pos = hpos.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_neg = hneg.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let v1_ok = !hpos.is_empty()
        && !hneg.is_empty()
        && min_pos >= v1_lines.1 as f64
        && max_neg < v1_lines.1 as f64;
    let (levels, level_source) = if v1_ok {
        (v1_lines, "v1-equivalent-verified".to_string())
    } else {
        // 留出集重定：medium 取正负分界中点，high 取中点与正样本下界的中点，low 取 medium 的半档。
        let mid = ((max_neg + min_pos) / 2.0).clamp(0.02, 0.95) as f32;
        let hi = ((mid as f64 + min_pos) / 2.0).clamp(mid as f64 + 0.01, 0.99) as f32;
        ((hi, mid, (mid * 0.5).max(1e-3)), "holdout-derived".to_string())
    };
    let version = format!("m7-lr-{}", &docsets_hash(dir)[..8]);
    let model = collusion::LrModel::from_parts(
        collusion::CALIBRATION_EXPERIMENTAL,
        &version,
        intercept,
        weights,
        levels,
    );

    let probs = |m: &collusion::LrModel, set: &[&FitSample], pos: bool| -> Vec<f64> {
        set.iter()
            .filter(|s| (s.y > 0.5) == pos)
            .map(|s| {
                let mut x = [0f32; collusion::FEATURE_COUNT];
                for (i, v) in s.x.iter().enumerate() {
                    x[i] = *v as f32;
                }
                m.evaluate(&x).0 as f64
            })
            .collect()
    };
    let train_refs: Vec<&FitSample> = train_owned.iter().collect();
    let col_mean = |pos: bool, i: usize| -> f64 {
        let v: Vec<f64> =
            train_owned.iter().filter(|s| (s.y > 0.5) == pos).map(|s| s.x[i]).collect();
        if v.is_empty() {
            0.0
        } else {
            (v.iter().sum::<f64>() / v.len() as f64 * 1e4).round() / 1e4
        }
    };
    let mut feature_mean_positive = BTreeMap::new();
    let mut feature_mean_negative = BTreeMap::new();
    for (i, k) in collusion::FEATURE_KINDS.iter().enumerate() {
        feature_mean_positive.insert((*k).to_string(), col_mean(true, i));
        feature_mean_negative.insert((*k).to_string(), col_mean(false, i));
    }
    let report = FitReport {
        lambda: FIT_LAMBDA,
        samples: samples.len(),
        train_samples: train_owned.len(),
        holdout_samples: holdout.len(),
        train_auc: auc_score(&probs(&model, &train_refs, true), &probs(&model, &train_refs, false)),
        holdout_auc: auc_score(&probs(&model, &holdout, true), &probs(&model, &holdout, false)),
        holdout_cllr: cllr(&probs(&model, &holdout, true), &probs(&model, &holdout, false)),
        holdout_cllr_linear_baseline: cllr(
            &probs(&prior_model, &holdout, true),
            &probs(&prior_model, &holdout, false),
        ),
        dead_columns,
        feature_mean_positive,
        feature_mean_negative,
        level_source,
        docsets_hash: docsets_hash(dir),
        pairs_hash: sha256_of_file(&dir.join("pairs.jsonl")),
        git_rev: git_rev(),
        fitted_at: today_utc(),
    };
    (model, report)
}

/// 权重文件序列化（6 位小数定点，便于评审 diff 且逐次可复现）。
pub fn lr_json(model: &collusion::LrModel, report: &FitReport) -> String {
    let round = |v: f32| (v as f64 * 1e6).round() / 1e6;
    let weights: serde_json::Map<String, serde_json::Value> = collusion::FEATURE_KINDS
        .iter()
        .enumerate()
        .map(|(i, k)| ((*k).to_string(), serde_json::json!(round(model.weights[i]))))
        .collect();
    let body = serde_json::json!({
        "calibrationKind": model.calibration_kind,
        "version": model.version,
        "note": "实验性校准（合成语料）：权重由 fixtures/corpus/docsets 拟合，L2 向 v1 经验权重收缩；\
                 概率为合成语料校准值、不是串通概率；真实判例回测前不作为唯一依据。\
                 重新生成：cargo run --bin corpusgen --features dev-tools -- fit-collusion",
        "intercept": round(model.intercept),
        "weights": serde_json::Value::Object(weights),
        "levels": {
            "high": round(model.level_high),
            "medium": round(model.level_medium),
            "low": round(model.level_low),
        },
        "fit": report,
    });
    format!("{}\n", serde_json::to_string_pretty(&body).expect("serialize lr json"))
}

/// 拟合指标速览（CLI 打印）。
pub fn render_fit(model: &collusion::LrModel, report: &FitReport) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "== 围标融合 LR 拟合（λ={} 样本 {}=训练 {}+留出 {}）==",
        report.lambda, report.samples, report.train_samples, report.holdout_samples
    );
    let _ = writeln!(s, "截距 b = {:.4}（零证据基线，score 已按其重基归零）", model.intercept);
    let prior = collusion::empirical_prior();
    for (i, k) in collusion::FEATURE_KINDS.iter().enumerate() {
        let dead = if report.dead_columns.iter().any(|d| d == k) { "  [死列→保留先验]" } else { "" };
        let _ = writeln!(
            s,
            "  {k:<20} w = {:>7.4}  (先验 {:>6.4})  正类均值 {:.3} / 负类均值 {:.3}{dead}",
            model.weights[i],
            prior.weights[i],
            report.feature_mean_positive.get(*k).copied().unwrap_or(0.0),
            report.feature_mean_negative.get(*k).copied().unwrap_or(0.0)
        );
    }
    let _ = writeln!(
        s,
        "留出集 AUC = {:.4}（训练集 {:.4}）；Cllr = {:.4} vs 线性基线 {:.4}",
        report.holdout_auc, report.train_auc, report.holdout_cllr, report.holdout_cllr_linear_baseline
    );
    let _ = writeln!(
        s,
        "分级线（证据强度尺度，来源 {}）：high={:.4} medium={:.4} low={:.4}",
        report.level_source, model.level_high, model.level_medium, model.level_low
    );
    s
}

/// fit-collusion 子命令入口：拟合并写 fixtures/calibration/collusion_lr.json。
fn write_collusion_lr_to(out: &Path) {
    let jieba = Jieba::new();
    let dir = corpus_dir();
    let (model, report) = fit_collusion(&jieba, &dir);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("建目录失败: {e}"));
    }
    std::fs::write(out, lr_json(&model, &report))
        .unwrap_or_else(|e| panic!("写 {} 失败: {e}", out.display()));
    eprintln!("{}", render_fit(&model, &report));
    eprintln!("[fit-collusion] 已写入 → {}", out.display());
}

fn default_lr_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/calibration/collusion_lr.json")
}

// ————————————————————————————————————————————————————————————————————————
// 概率校准 + 共形三带拟合（执行方案 §8 W6-4 / M7）：pairs 语料 → score_calib.json
//
// 三分切分（确定性，按语料内固定序号取模）：
//   idx%5 ∈ {0,1,2} → 训练集：拟合 Platt 两参数（final_score → P(同源)）
//   idx%5 == 3      → 共形校准集：算 t_low/t_high（split conformal 要求与训练集不相交，
//                     否则有限样本保证不成立——这是 split conformal 的前提，不是可选优化）
//   idx%5 == 4      → 留出集：ECE / 带内 FNR / 带内 FPR / 需人工复核带占比（验收①②③）
//
// 标签口径：正类 = 同源编制的四类（same/minor_change/changed/rewrite），负类 = unrelated。
// 与召回层 POSITIVE_LABELS 同一集合，避免「校准的正类」与「门禁的正类」两套定义。
//
// §1.5 纪律：合成语料上的保证不等于真实标书上的保证 —— 文件里 calibrationKind 标
// experimental-synthetic，note 与 UI/报告文案一律限定「在合成校准语料上测得」。
// ————————————————————————————————————————————————————————————————————————

/// 一条校准样本：排序分（score_pair 的 final_score）+ 标签（1=同源改写，0=无关）。
pub struct CalibSample {
    pub score: f64,
    pub y: f64,
}

/// L2 强度（向 0 收缩）：Platt 只有两个参数、样本上千，只需极弱正则防完全可分时参数发散。
const CALIB_LAMBDA: f64 = 1e-3;
const CALIB_ITERS: usize = 200;
const CALIB_TOL: f64 = 1e-12;
/// 目标漏检率 α（低优先级抽查带）与目标误报率 β（重点标红带）。
/// 【产品/合规决策，不开放运行时调整】：改 α 即改承诺语义，须走版本发布（方案 §8 配置项）。
pub const CALIB_ALPHA: f64 = 0.05;
pub const CALIB_BETA: f64 = 0.05;
/// ECE 分箱数（等宽）。
const ECE_BINS: usize = 15;
/// 运行域下界：簇的分数恒 ≥ 相似阈值（config 默认 0.70），且 classify_cluster 的「待复核」
/// 线在 0.55——低于 0.55 的分实际上不会成簇。三带分流【只在运行域内有意义】，故退化守卫
/// 只看这一段。
const OPERATING_FLOOR: f64 = 0.55;
/// 退化守卫线：运行域内任一带吃掉 ≥95% 的样本 ⇒ 三带没有分辨力 ⇒ 不上线分流（review-all）。
const ROUTING_DOMINANCE_MAX: f64 = 0.95;
/// 切分取模基数与两个切分点（idx%5：0–2 训练 / 3 共形校准 / 4 留出）。
const CALIB_SPLIT_MOD: usize = 5;
const CALIB_SPLIT_CONFORMAL: usize = 3;
const CALIB_SPLIT_HOLDOUT: usize = 4;

/// pairs 语料 → 校准样本（无模型层，与 corpus_regression 快档同口径）。
pub fn calib_samples(jieba: &Jieba, pairs: &[PairRecord]) -> Vec<CalibSample> {
    let built = build_pair_chunks(jieba, pairs, |_, _| (None, None));
    built
        .iter()
        .zip(pairs)
        .map(|((v, sem_cos, _), r)| {
            let parts = scoring::score_pair(&v[0], &v[1], *sem_cos);
            CalibSample {
                score: parts.final_score as f64,
                y: if POSITIVE_LABELS.contains(&static_label(&r.label)) { 1.0 } else { 0.0 },
            }
        })
        .collect()
}

/// Platt 拟合：p = σ(a·s + b)，牛顿法 + 目标平滑（Platt 1999：y⁺=(N⁺+1)/(N⁺+2)、
/// y⁻=1/(N⁻+2)）——平滑是 Platt 方法的组成部分，防完全可分时把概率钉死在 0/1（那会让
/// ECE 在高分区爆掉，也让共形阈值退化）。返回 (a, b)；a 经投影保证非负（单调不减）。
pub fn fit_platt(samples: &[CalibSample], lambda: f64) -> (f64, f64) {
    let np = samples.iter().filter(|s| s.y > 0.5).count() as f64;
    let nn = samples.len() as f64 - np;
    let (tp, tn) = ((np + 1.0) / (np + 2.0), 1.0 / (nn + 2.0));
    let (mut a, mut b) = (1.0f64, 0.0f64);
    for _ in 0..CALIB_ITERS {
        let (mut g0, mut g1) = (0.0f64, 0.0f64); // 对 b、a 的梯度
        let (mut h00, mut h01, mut h11) = (0.0f64, 0.0f64, 0.0f64);
        for s in samples {
            let t = if s.y > 0.5 { tp } else { tn };
            let p = 1.0 / (1.0 + (-(a * s.score + b)).exp());
            let w = (p * (1.0 - p)).max(1e-12);
            let r = t - p;
            g0 += r;
            g1 += r * s.score;
            h00 += w;
            h01 += w * s.score;
            h11 += w * s.score * s.score;
        }
        g0 -= lambda * b;
        g1 -= lambda * a;
        h00 += lambda;
        h11 += lambda;
        let det = h00 * h11 - h01 * h01;
        if det.abs() < 1e-15 {
            break;
        }
        let db = (h11 * g0 - h01 * g1) / det;
        let da = (h00 * g1 - h01 * g0) / det;
        let (nb, na) = (b + db, (a + da).max(0.0));
        let delta = (nb - b).abs().max((na - a).abs());
        b = nb;
        a = na;
        if delta < CALIB_TOL {
            break;
        }
    }
    (a, b)
}

/// split conformal 分位：不合格分升序，取第 ceil((n+1)(1−rate)) 名（1-based）。
/// 名次超出样本量 ⇒ 样本不足以给出该错误率的有限样本保证 → None（调用方退化为最保守阈值）。
pub fn conformal_quantile(sorted_asc: &[f64], rate: f64) -> Option<f64> {
    let n = sorted_asc.len();
    if n == 0 {
        return None;
    }
    let k = (((n + 1) as f64) * (1.0 - rate)).ceil() as usize;
    if k == 0 || k > n {
        return None;
    }
    Some(sorted_asc[k - 1])
}

/// 期望校准误差（等宽分箱）：Σ (n_b/n)·|实测同源率 − 平均预测概率|。
pub fn ece(probs: &[f64], ys: &[f64], bins: usize) -> f64 {
    if probs.is_empty() || probs.len() != ys.len() || bins == 0 {
        return f64::NAN;
    }
    let mut cnt = vec![0usize; bins];
    let mut sum_p = vec![0.0f64; bins];
    let mut sum_y = vec![0.0f64; bins];
    for (p, y) in probs.iter().zip(ys) {
        let b = ((p * bins as f64) as usize).min(bins - 1);
        cnt[b] += 1;
        sum_p[b] += *p;
        sum_y[b] += *y;
    }
    let n = probs.len() as f64;
    (0..bins)
        .filter(|&b| cnt[b] > 0)
        .map(|b| {
            let c = cnt[b] as f64;
            c / n * ((sum_y[b] / c) - (sum_p[b] / c)).abs()
        })
        .sum()
}

/// 校准拟合报告（打印 + 写入 score_calib.json 的台账段）。
#[derive(Serialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct CalibReport {
    pub samples: usize,
    pub train_samples: usize,
    pub conformal_samples: usize,
    pub holdout_samples: usize,
    /// 留出集 ECE：校准后 vs 未校准（把原始 final_score 当概率用）——验收①的两个数。
    pub holdout_ece: f64,
    pub holdout_ece_uncalibrated: f64,
    /// 相对下降比例（验收①要求 ≥50%）。
    pub holdout_ece_reduction: f64,
    /// 留出集上低优先级抽查带的实测漏检率（正样本落入 pass 带的比例，验收②≤α+2pp）。
    pub holdout_pass_fnr: f64,
    /// 留出集上重点标红带的实测误报率（负样本落入 flag 带的比例，验收②≤β+2pp）。
    pub holdout_flag_fpr: f64,
    /// 留出集三带占比（验收③：需人工复核带 ≤40%）。
    pub holdout_pass_share: f64,
    pub holdout_review_share: f64,
    pub holdout_flag_share: f64,
    /// 共形分位是否有足够样本给出保证（false ⇒ 阈值退化为最保守值，须扩语料）。
    pub conformal_low_sufficient: bool,
    pub conformal_high_sufficient: bool,
    /// 两条共形阈值是否倒挂（正负类在校准集上按 α/β 完全分离）。倒挂时取 min/max 互换，
    /// 两条带各自【收缩】，α/β 保证仍成立（子集的错误率不高于母集），复核带覆盖分离间隙。
    pub conformal_crossed: bool,
    /// 分流决策（three-band | review-all）与依据。
    pub routing: String,
    pub routing_reason: String,
    /// 运行域（final_score ≥ 0.55，即簇实际存在的分数区间）内的留出样本数与三带占比——
    /// 退化守卫的直接证据，评审据此判断「分流是否有分辨力」。
    pub operating_samples: usize,
    pub operating_pass_share: f64,
    pub operating_review_share: f64,
    pub operating_flag_share: f64,
    pub positive_labels: Vec<String>,
    pub pairs_hash: String,
    pub git_rev: String,
    pub fitted_at: String,
}

/// 六位定点（与运行时 evaluate 的定点一致，保证「文件里的数 = 运行时用的数」）。
fn round6_f64(v: f64) -> f64 {
    (v * 1e6).round() / 1e6
}

/// 在 pairs 语料上拟合 Platt + split conformal 三带阈值。
pub fn fit_calibration(jieba: &Jieba, dir: &Path) -> (calibrate::CalibrationModel, CalibReport) {
    let pairs_path = dir.join("pairs.jsonl");
    let pairs = read_pairs(&pairs_path);
    let samples = calib_samples(jieba, &pairs);

    let mut train: Vec<&CalibSample> = Vec::new();
    let mut conformal: Vec<&CalibSample> = Vec::new();
    let mut holdout: Vec<&CalibSample> = Vec::new();
    for (i, s) in samples.iter().enumerate() {
        match i % CALIB_SPLIT_MOD {
            CALIB_SPLIT_CONFORMAL => conformal.push(s),
            CALIB_SPLIT_HOLDOUT => holdout.push(s),
            _ => train.push(s),
        }
    }
    let train_owned: Vec<CalibSample> =
        train.iter().map(|s| CalibSample { score: s.score, y: s.y }).collect();
    let (a, b) = fit_platt(&train_owned, CALIB_LAMBDA);
    // 系数就地定点化：分位数与验收指标都由【落盘后的同一组系数】算出，避免写文件的舍入
    // 让运行时的带划分与报告里的 FNR/FPR 错位。
    let (a, b) = (round6_f64(a), round6_f64(b));
    let calibrator = calibrate::Calibrator::Platt { a: a as f32, b: b as f32 };
    let p_of = |s: &CalibSample| -> f64 { calibrator.probability(s.score as f32) as f64 };

    // split conformal：正样本以 (1−p) 为不合格分求 t_low；负样本以 p 为不合格分求 t_high。
    let mut pos_nc: Vec<f64> = conformal.iter().filter(|s| s.y > 0.5).map(|s| 1.0 - p_of(s)).collect();
    let mut neg_nc: Vec<f64> = conformal.iter().filter(|s| s.y <= 0.5).map(|s| p_of(s)).collect();
    pos_nc.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    neg_nc.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
    let q_low = conformal_quantile(&pos_nc, CALIB_ALPHA);
    let q_high = conformal_quantile(&neg_nc, CALIB_BETA);
    // 样本不足以支撑该错误率时退化为最保守：t_low=0（pass 带空）/ t_high=1（flag 带空），
    // 即「不给任何带内保证、全部转人工复核」，而不是硬凑一个没有覆盖率的阈值。
    let raw_low = round6_f64(q_low.map(|q| (1.0 - q).clamp(0.0, 1.0)).unwrap_or(0.0));
    // flag 带用 p ≥ t_high 判定，而共形保证的是 P(p_new > q) ≤ β：抬一个定点单位使
    // 「p ≥ t_high」⟺「p > q」，等号侧不吃掉保证。
    let raw_high = round6_f64(q_high.map(|q| (q + 1e-6).clamp(0.0, 1.0)).unwrap_or(1.0));
    // 倒挂（raw_high < raw_low）= 正负两类在校准集上按 α/β 完全分离。此时取 min/max 互换：
    // pass 带与 flag 带各自【收缩】为原带的子集 ⇒ 两个有限样本保证都仍然成立
    // （子集上的错误率不高于母集），而复核带正好覆盖两类之间的分离间隙，不会退化成空带。
    let conformal_crossed = raw_high < raw_low;
    let (t_low, t_high) = (raw_low.min(raw_high), raw_low.max(raw_high));

    let version = format!("m7-calib-{}", &sha256_of_file(&pairs_path)[..8]);
    let mut model = calibrate::CalibrationModel {
        calibration_kind: collusion::CALIBRATION_EXPERIMENTAL.to_string(),
        version,
        calibrator,
        alpha: CALIB_ALPHA as f32,
        beta: CALIB_BETA as f32,
        t_low: t_low as f32,
        t_high: t_high as f32,
        // 先按三带评估：退化守卫要看「若分流会怎样」，据此再决定是否真的上线分流。
        routing: calibrate::Routing::ThreeBand,
        corpus_hash: sha256_of_file(&pairs_path),
    };

    // —— 退化守卫（§1.5-1 如实展示）——
    // 三带只在【运行域】（final_score ≥ OPERATING_FLOOR，簇实际存在的分数区间）内有意义。
    // 若运行域内任一带吃掉 ≥95% 的样本，说明校准概率在这一段没有分辨力：此时上线分流等于
    // 把几乎所有条款推进同一条带（重点标红占满 = 告警疲劳；低优先级占满 = 变相放行）。
    // 守卫命中 → routing=review-all：机器就位、置信度照常展示，但不做分流断言。
    let op: Vec<&&CalibSample> =
        holdout.iter().filter(|s| s.score >= OPERATING_FLOOR).collect();
    let op_bands: Vec<&str> = op.iter().map(|s| model.evaluate(s.score as f32).1).collect();
    let op_share = |b: &str| -> f64 {
        if op_bands.is_empty() {
            0.0
        } else {
            op_bands.iter().filter(|x| **x == b).count() as f64 / op_bands.len() as f64
        }
    };
    let (op_pass, op_review, op_flag) = (
        op_share(calibrate::BAND_PASS),
        op_share(calibrate::BAND_REVIEW),
        op_share(calibrate::BAND_FLAG),
    );
    let dominant: f64 = op_pass.max(op_review).max(op_flag);
    let (routing, routing_reason) = if op_bands.is_empty() {
        (
            calibrate::Routing::ReviewAll,
            "留出集在运行域（分数 ≥0.55）内无样本，无法验证三带分辨力".to_string(),
        )
    } else if q_low.is_none() || q_high.is_none() {
        (
            calibrate::Routing::ReviewAll,
            "共形校准样本不足以给出 α/β 的有限样本保证".to_string(),
        )
    } else if dominant >= ROUTING_DOMINANCE_MAX {
        (
            calibrate::Routing::ReviewAll,
            format!(
                "运行域内单带占比 {:.1}% ≥ {:.0}%：相似度分在簇的分数区间内无分辨力\
                 （本语料缺「独立编制但表面相似」的难负样本，独立文档集里同样存在 avg=1.000 的共享范本簇），\
                 据此分流会把几乎所有条款推进同一条带 → 本版不上线分流，全部按需人工复核",
                dominant * 100.0,
                ROUTING_DOMINANCE_MAX * 100.0
            ),
        )
    } else {
        (calibrate::Routing::ThreeBand, "运行域内三带分布有分辨力，分流启用".to_string())
    };
    // 留出集验收指标：概率经 model.evaluate（与运行时逐字节同通道），三带指标按【假定分流生效】
    // 计算（评估的是两条阈值本身的性质；分流是否上线由上面的守卫单独记录，两者不要互相污染）。
    let hp: Vec<f64> = holdout.iter().map(|s| model.evaluate(s.score as f32).0 as f64).collect();
    let hy: Vec<f64> = holdout.iter().map(|s| s.y).collect();
    let raw: Vec<f64> = holdout.iter().map(|s| s.score.clamp(0.0, 1.0)).collect();
    let bands: Vec<&str> = holdout.iter().map(|s| model.evaluate(s.score as f32).1).collect();
    model.routing = routing;
    let share = |b: &str| -> f64 {
        if bands.is_empty() {
            0.0
        } else {
            bands.iter().filter(|x| **x == b).count() as f64 / bands.len() as f64
        }
    };
    let rate = |want_pos: bool, band: &str| -> f64 {
        let idx: Vec<usize> =
            (0..holdout.len()).filter(|&i| (hy[i] > 0.5) == want_pos).collect();
        if idx.is_empty() {
            0.0
        } else {
            idx.iter().filter(|&&i| bands[i] == band).count() as f64 / idx.len() as f64
        }
    };
    let ece_cal = ece(&hp, &hy, ECE_BINS);
    let ece_raw = ece(&raw, &hy, ECE_BINS);
    let report = CalibReport {
        samples: samples.len(),
        train_samples: train_owned.len(),
        conformal_samples: conformal.len(),
        holdout_samples: holdout.len(),
        holdout_ece: round6_f64(ece_cal),
        holdout_ece_uncalibrated: round6_f64(ece_raw),
        holdout_ece_reduction: round6_f64(if ece_raw > 0.0 { 1.0 - ece_cal / ece_raw } else { 0.0 }),
        holdout_pass_fnr: round6_f64(rate(true, calibrate::BAND_PASS)),
        holdout_flag_fpr: round6_f64(rate(false, calibrate::BAND_FLAG)),
        holdout_pass_share: round6_f64(share(calibrate::BAND_PASS)),
        holdout_review_share: round6_f64(share(calibrate::BAND_REVIEW)),
        holdout_flag_share: round6_f64(share(calibrate::BAND_FLAG)),
        conformal_low_sufficient: q_low.is_some(),
        conformal_high_sufficient: q_high.is_some(),
        conformal_crossed,
        routing: routing.as_str().to_string(),
        routing_reason,
        operating_samples: op_bands.len(),
        operating_pass_share: round6_f64(op_pass),
        operating_review_share: round6_f64(op_review),
        operating_flag_share: round6_f64(op_flag),
        positive_labels: POSITIVE_LABELS.iter().map(|s| (*s).to_string()).collect(),
        pairs_hash: sha256_of_file(&pairs_path),
        git_rev: git_rev(),
        fitted_at: today_utc(),
    };
    (model, report)
}

/// 校准文件序列化（6 位定点，便于评审 diff 且逐次可复现）。
pub fn calib_json(model: &calibrate::CalibrationModel, report: &CalibReport) -> String {
    let params = match &model.calibrator {
        calibrate::Calibrator::Platt { a, b } => serde_json::json!({
            "platt": { "a": round6_f64(*a as f64), "b": round6_f64(*b as f64) },
        }),
        calibrate::Calibrator::Isotonic { breakpoints } => serde_json::json!({
            "isotonic": {
                "breakpoints": breakpoints
                    .iter()
                    .map(|(x, y)| serde_json::json!([round6_f64(*x as f64), round6_f64(*y as f64)]))
                    .collect::<Vec<_>>(),
            },
        }),
    };
    let mut body = serde_json::json!({
        "calibrationKind": model.calibration_kind,
        "version": model.version,
        "type": model.calibrator.kind_str(),
        "alpha": round6_f64(model.alpha as f64),
        "beta": round6_f64(model.beta as f64),
        "thresholds": {
            "tLow": round6_f64(model.t_low as f64),
            "tHigh": round6_f64(model.t_high as f64),
        },
        "routing": model.routing.as_str(),
        "note": "实验性校准（合成语料）：Platt 参数与三带阈值由 fixtures/corpus/pairs.jsonl 拟合。\
                 α/β 是【在合成校准语料上测得】的带内错误率目标，不是对真实标书的承诺；\
                 低优先级抽查带只做排序与折叠，不隐藏任何条款。\
                 routing=review-all 时三带分流不生效（见 fit.routingReason）：全部条款按需人工复核。\
                 重新生成：cargo run --bin corpusgen --features dev-tools -- fit-calib",
        "fit": report,
    });
    if let (Some(obj), Some(p)) = (body.as_object_mut(), params.as_object()) {
        for (k, v) in p {
            obj.insert(k.clone(), v.clone());
        }
    }
    format!("{}\n", serde_json::to_string_pretty(&body).expect("serialize calib json"))
}

/// 校准指标速览（CLI 打印）。
pub fn render_calib(model: &calibrate::CalibrationModel, report: &CalibReport) -> String {
    use std::fmt::Write;
    let mut s = String::new();
    let _ = writeln!(
        s,
        "== 概率校准（{}）+ 共形三带（样本 {}=训练 {}+共形 {}+留出 {}）==",
        model.calibrator.kind_str(),
        report.samples,
        report.train_samples,
        report.conformal_samples,
        report.holdout_samples
    );
    if let calibrate::Calibrator::Platt { a, b } = &model.calibrator {
        let _ = writeln!(s, "Platt: p = σ({a:.6}·s + {b:.6})");
    }
    let _ = writeln!(
        s,
        "三带阈值：tLow={:.6} tHigh={:.6}（α={:.2} β={:.2}，在合成校准语料上测得；共形样本充足 低{}/高{}）",
        model.t_low,
        model.t_high,
        model.alpha,
        model.beta,
        report.conformal_low_sufficient,
        report.conformal_high_sufficient
    );
    let _ = writeln!(
        s,
        "留出集 ECE：校准后 {:.4} vs 未校准 {:.4}（下降 {:.1}%）",
        report.holdout_ece,
        report.holdout_ece_uncalibrated,
        report.holdout_ece_reduction * 100.0
    );
    let _ = writeln!(
        s,
        "留出集带内错误率：低优先级抽查带 FNR={:.4}（目标 ≤{:.4}）／重点标红带 FPR={:.4}（目标 ≤{:.4}）",
        report.holdout_pass_fnr,
        CALIB_ALPHA + 0.02,
        report.holdout_flag_fpr,
        CALIB_BETA + 0.02
    );
    let _ = writeln!(
        s,
        "留出集三带占比：低优先级抽查 {:.1}% / 需人工复核 {:.1}% / 重点标红 {:.1}%（复核带上限 40%）",
        report.holdout_pass_share * 100.0,
        report.holdout_review_share * 100.0,
        report.holdout_flag_share * 100.0
    );
    let _ = writeln!(
        s,
        "运行域（分数 ≥{:.2}，n={}）三带占比：{:.1}% / {:.1}% / {:.1}%{}",
        OPERATING_FLOOR,
        report.operating_samples,
        report.operating_pass_share * 100.0,
        report.operating_review_share * 100.0,
        report.operating_flag_share * 100.0,
        if report.conformal_crossed { "（共形阈值倒挂，已按保守方向互换）" } else { "" }
    );
    let _ = writeln!(s, "分流决策：{} —— {}", report.routing, report.routing_reason);
    s
}

/// fit-calib 子命令入口：拟合并写 fixtures/calibration/score_calib.json。
fn write_calib_to(out: &Path) {
    let jieba = Jieba::new();
    let dir = corpus_dir();
    let (model, report) = fit_calibration(&jieba, &dir);
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("建目录失败: {e}"));
    }
    std::fs::write(out, calib_json(&model, &report))
        .unwrap_or_else(|e| panic!("写 {} 失败: {e}", out.display()));
    eprintln!("{}", render_calib(&model, &report));
    eprintln!("[fit-calib] 已写入 → {}", out.display());
}

fn default_calib_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/calibration/score_calib.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::normalize::{normalize, NormalizeOptions};

    #[test]
    fn corpus_synonym_table_binary_search() {
        let t = SynonymTable::load();
        assert!(t.len() >= 300, "synonym table too small: {}", t.len());
        assert!(t.is_sorted(), "entries must be sorted for binary search");
        // 命中：表内词返回非空候选
        for w in ["施工", "质量", "确保", "采用", "工期", "验收"] {
            let alts = t.lookup(w).unwrap_or_else(|| panic!("missing key {w}"));
            assert!(!alts.is_empty(), "empty alts for {w}");
        }
        // 未命中：表外词返回 None
        assert!(t.lookup("这个词绝对不在同义表中xyz").is_none());
        assert!(t.lookup("").is_none());
    }

    #[test]
    fn corpus_ocr_table_loads() {
        let t = OcrTable::load();
        assert!(t.get('日').is_some(), "OCR confusions should map 日");
        assert!(t.get('0').is_some(), "OCR confusions should map 0");
        assert!(t.get('这').is_none());
    }

    #[test]
    fn corpus_generation_is_deterministic() {
        let a = generate_pairs();
        let b = generate_pairs();
        assert_eq!(a.len(), b.len(), "pair count must be stable");
        assert_eq!(to_jsonl(&a), to_jsonl(&b), "two runs must be byte-identical");
    }

    #[test]
    fn corpus_meets_label_and_size_targets() {
        let recs = generate_pairs();
        assert!(recs.len() >= 1500, "need >=1500 pairs, got {}", recs.len());
        for lbl in ["same", "minor_change", "rewrite", "unrelated"] {
            let c = recs.iter().filter(|r| r.label == lbl).count();
            assert!(c >= 300, "label {lbl} has only {c} pairs (need >=300)");
        }
        // 每对两侧非空，unrelated 两侧来自不同种子
        for r in &recs {
            assert!(!r.text_a.trim().is_empty() && !r.text_b.trim().is_empty(), "{}", r.id);
        }
    }

    #[test]
    fn corpus_same_pairs_normalize_identically() {
        // same 标签的两侧必须归一后逐字节一致（变换④可逆性验收），且原文层确有差异。
        let opts = NormalizeOptions::default();
        let recs = generate_pairs();
        let same: Vec<&PairRecord> = recs.iter().filter(|r| r.label == "same").collect();
        assert!(!same.is_empty());
        for r in same {
            let na = normalize(&r.text_a, &opts);
            let nb = normalize(&r.text_b, &opts);
            assert_eq!(na, nb, "same pair {} must normalize identically", r.id);
            assert_ne!(r.text_a, r.text_b, "same pair {} should differ at raw layer", r.id);
        }
    }

    #[test]
    fn corpus_numeric_tweak_changes_value_within_entity() {
        let mut rng = Rng::seeded(features::hash64("numeric-test"));
        let (out, ok) = numeric_tweak("本工程计划工期为240个日历日，投标报价为人民币1280万元。", &mut rng);
        assert!(ok, "should find a numeric entity to tweak");
        assert_ne!(out, "本工程计划工期为240个日历日，投标报价为人民币1280万元。");
        // 汉字未被破坏（只动了实体 span 内的数字）
        assert!(out.contains("个日历日") || out.contains("万元"));
    }

    #[test]
    fn corpus_synonym_replace_avoids_entity_spans() {
        // 数字/工期实体 span 内不应发生同义替换；只替换普通词。
        let jieba = Jieba::new();
        let syn = SynonymTable::load();
        let mut rng = Rng::seeded(features::hash64("syn-test"));
        let text = "我方采用先进技术，工期为180个日历日。";
        let (out, n) = synonym_replace(&jieba, text, &syn, 1.0, 8, &mut rng);
        assert!(n > 0, "should replace at least one synonym");
        assert!(out.contains("180个日历日"), "entity span must survive: {out}");
    }

    // —— docsets（文档集级正负样本）——

    use crate::engine::report::{DocInfo, Fingerprint};
    use crate::engine::{fingerprint, parse};
    use std::collections::HashSet;
    use std::path::Path as StdPath;
    use std::sync::atomic::AtomicBool;

    fn doc_info(fp: Fingerprint) -> DocInfo {
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

    /// 写一个文档集到目录并解析各份 → (指纹, 每份图片 sha256 集合, 每份全文)。
    #[allow(clippy::type_complexity)]
    fn write_and_parse(
        set: &GeneratedDocset,
        dir: &StdPath,
    ) -> (Vec<Fingerprint>, Vec<HashSet<String>>, Vec<String>) {
        std::fs::create_dir_all(dir).unwrap();
        let cancel = AtomicBool::new(false);
        let (mut fps, mut shas, mut texts) = (Vec::new(), Vec::new(), Vec::new());
        for d in &set.docs {
            let p = dir.join(&d.name);
            std::fs::write(&p, &d.bytes).unwrap();
            let pb = parse::parse_file_blocks(&p, &cancel).unwrap();
            shas.push(pb.image_hashes.iter().map(|h| h.sha256.clone()).collect::<HashSet<_>>());
            texts.push(pb.legacy_text.clone());
            fps.push(pb.fingerprint);
        }
        (fps, shas, texts)
    }

    #[test]
    fn corpus_docsets_are_deterministic() {
        let a = generate_docsets();
        let b = generate_docsets();
        assert_eq!(a.len(), b.len(), "docset count must be stable");
        assert_eq!(docsets_to_jsonl(&a), docsets_to_jsonl(&b), "manifests must be byte-identical");
        for (sa, sb) in a.iter().zip(&b) {
            assert_eq!(sa.docs.len(), sb.docs.len());
            for (da, db) in sa.docs.iter().zip(&sb.docs) {
                assert_eq!(da.name, db.name);
                assert_eq!(da.bytes, db.bytes, "docx {} must be byte-identical across runs", da.name);
            }
        }
    }

    #[test]
    fn corpus_docsets_meet_group_targets() {
        let sets = generate_docsets();
        assert!(sets.len() >= 10, "need >=10 docsets, got {}", sets.len());
        let col = sets.iter().filter(|s| s.manifest.label == "collusion").count();
        let ind = sets.iter().filter(|s| s.manifest.label == "independent").count();
        assert_eq!(col, ind, "collusion/independent must be balanced");
        assert!(col >= 5, "each label needs >=5 groups (positives={col})");
        for s in &sets {
            assert!(s.docs.len() >= 3, "docset {} needs >=3 docs", s.manifest.docset_id);
            if s.manifest.label == "collusion" {
                for sig in ["rsid", "imageReuse", "numericRatio", "evasion"] {
                    assert!(
                        s.manifest.planted_signals.iter().any(|x| x == sig),
                        "collusion set {} must plant {sig}",
                        s.manifest.docset_id
                    );
                }
            } else {
                assert!(
                    s.manifest.planted_signals.is_empty(),
                    "independent set {} must plant no signals",
                    s.manifest.docset_id
                );
            }
        }
    }

    #[test]
    fn corpus_docsets_planted_signals_materialize() {
        let sets = generate_docsets();
        let root = std::env::temp_dir().join(format!("bg_docsets_{}", uuid::Uuid::new_v4()));

        // 围标正样本组：rsid_pairs 命中（含 rsidRoot 相同）+ 全份共享同一图片 sha + 有零宽
        let col = sets.iter().find(|s| s.manifest.label == "collusion").unwrap();
        let (fps, shas, texts) = write_and_parse(col, &root.join(&col.manifest.docset_id));
        let mut docs: Vec<DocInfo> = fps.into_iter().map(doc_info).collect();
        let hits = fingerprint::rsid_pairs(&mut docs, &HashSet::new());
        assert!(!hits.is_empty(), "collusion docset must yield rsid_pairs hits");
        assert!(hits.iter().any(|h| h.root_match), "collusion rsidRoot must match");
        let shared: HashSet<&String> = shas[0].intersection(&shas[1]).collect();
        assert!(!shared.is_empty(), "collusion docs must share an image sha256");
        for s in &shas {
            assert!(s.iter().any(|x| shared.contains(x)), "every collusion doc reuses the image");
        }
        let (_, st) = crate::engine::normalize::sanitize_with_stats(&texts[0]);
        assert!(st.zero_width > 0, "collusion doc must carry injected zero-width, got {}", st.zero_width);

        // 独立负样本组：rsid_pairs 空 + 图片 sha 两两不交 + 无零宽
        let ind = sets.iter().find(|s| s.manifest.label == "independent").unwrap();
        let (fps2, shas2, texts2) = write_and_parse(ind, &root.join(&ind.manifest.docset_id));
        let mut docs2: Vec<DocInfo> = fps2.into_iter().map(doc_info).collect();
        let hits2 = fingerprint::rsid_pairs(&mut docs2, &HashSet::new());
        let _ = std::fs::remove_dir_all(&root);
        assert!(hits2.is_empty(), "independent docset must yield no rsid_pairs hits");
        for i in 0..shas2.len() {
            for j in (i + 1)..shas2.len() {
                assert!(shas2[i].is_disjoint(&shas2[j]), "independent docs must not share images");
            }
        }
        for t in &texts2 {
            let (_, s) = crate::engine::normalize::sanitize_with_stats(t);
            assert_eq!(s.zero_width, 0, "independent docs must have no zero-width");
        }
    }

    // —— 回归门禁（W6-5）——

    /// 快档（进 CI，非 ignored）：无模型层跑全语料，与 baseline_metrics.json 逐项对比。
    /// BIDGUARD_WRITE_BASELINE=1 时改为【写入】基线（不断言）。
    /// 门禁：任一 F1 降 >2pp / 召回率降 >1pp / AUC 降 >0.03；语料 hash 先行校验。
    #[test]
    fn corpus_regression() {
        let dir = corpus_dir();
        let jieba = Jieba::new();
        let mut cur = compute_fast_metrics(&jieba, &dir);
        cur.git_rev = git_rev();
        cur.generated_at = today_utc();
        let baseline_path = dir.join("baseline_metrics.json");

        if baseline_write_mode() {
            let body = serde_json::to_string_pretty(&cur).expect("serialize metrics");
            std::fs::write(&baseline_path, format!("{body}\n"))
                .unwrap_or_else(|e| panic!("写基线 {} 失败: {e}", baseline_path.display()));
            eprintln!("[corpus_regression] 已写入基线 → {}", baseline_path.display());
            eprintln!("{}", render_single(&cur));
            return;
        }

        let raw = std::fs::read_to_string(&baseline_path).unwrap_or_else(|_| {
            panic!(
                "缺少基线 {}。请先运行：BIDGUARD_WRITE_BASELINE=1 cargo test --manifest-path src-tauri/Cargo.toml --lib --features dev-tools corpus_regression",
                baseline_path.display()
            )
        });
        let base: RegressionMetrics =
            serde_json::from_str(&raw).expect("解析 baseline_metrics.json");

        // 语料 hash 校验（防基线与语料不同步）
        if let Some(msg) = hash_mismatch_message(&base, &cur) {
            panic!("{msg}");
        }

        let table = render_compare(&base, &cur);
        let failures = gate_failures(&base, &cur);
        if !failures.is_empty() {
            panic!("回归门禁失败（{} 项）：\n{}\n{}", failures.len(), failures.join("\n"), table);
        }
        eprintln!("[corpus_regression] 通过\n{table}");
    }

    // —— M7 融合拟合（W6-3）：纯函数层单测，不碰语料 IO，秒级 ——

    #[test]
    fn fit_logistic_recovers_separation_and_never_emits_negative_weights() {
        // 两列特征：第 1 列完全区分正负、第 2 列纯噪声。拟合应抬高第 1 列、并保持全部权重非负
        // （投影约束，§1.5-4）；截距保持为负（零证据不得抬底分）。
        let mk = |a: f64, b: f64, y: f64, i: usize| FitSample { id: format!("s{i}"), x: vec![a, b], y };
        let mut samples = Vec::new();
        for i in 0..12 {
            samples.push(mk(1.0, (i % 2) as f64, 1.0, i));
            samples.push(mk(0.0, ((i + 1) % 2) as f64, 0.0, 100 + i));
        }
        let prior = vec![-3.6, 1.0, 1.0];
        let theta = fit_logistic(&samples, &prior, FIT_LAMBDA);
        assert!(theta[0] < 0.0, "截距须为负，实际 {}", theta[0]);
        assert!(theta.iter().skip(1).all(|w| *w >= 0.0), "权重不得为负：{theta:?}");
        assert!(theta[1] > prior[1], "有区分度的列权重应被语料抬高：{}", theta[1]);
        assert!(theta[1] > theta[2], "区分列应显著强于噪声列：{} vs {}", theta[1], theta[2]);
        // 确定性：同输入两次拟合逐位一致
        let again = fit_logistic(&samples, &prior, FIT_LAMBDA);
        assert_eq!(theta, again, "拟合必须确定性可复现");
    }

    #[test]
    fn fitted_weights_file_round_trips_through_runtime_parser() {
        // 拟合侧写出的文件必须能被运行时加载器接受（含符号/量级/分级线审查）——否则上线即静默回退。
        let prior = collusion::empirical_prior();
        let model = collusion::LrModel::from_parts(
            collusion::CALIBRATION_EXPERIMENTAL,
            "roundtrip-test",
            prior.intercept,
            prior.weights,
            (
                prior.v1_line_equivalent(collusion::LEVEL_HIGH),
                prior.v1_line_equivalent(collusion::LEVEL_MEDIUM),
                prior.v1_line_equivalent(collusion::LEVEL_LOW),
            ),
        );
        let raw = lr_json(&model, &FitReport::default());
        let parsed = collusion::parse_lr_model(&raw).expect("拟合产物必须能被运行时加载");
        assert_eq!(parsed.calibration_kind, collusion::CALIBRATION_EXPERIMENTAL);
        for i in 0..collusion::FEATURE_COUNT {
            assert!((parsed.weights[i] - model.weights[i]).abs() < 1e-5, "权重列 {i} 往返丢失");
        }
        assert!(raw.ends_with('\n'), "文件应以换行收尾（diff 友好）");
    }

    #[test]
    fn cllr_rewards_confident_correct_and_punishes_confident_wrong() {
        // Cllr 口径自检：完美判别 ≈0；无信息（恒 0.5）=1；自信而错 >1。
        assert!(cllr(&[0.999, 0.999], &[0.001, 0.001]) < 0.02);
        assert!((cllr(&[0.5, 0.5], &[0.5, 0.5]) - 1.0).abs() < 1e-9);
        assert!(cllr(&[0.01, 0.02], &[0.98, 0.99]) > 1.0);
    }

    /// 语料 hash 不匹配时报错含修复命令（验收⑤），无需篡改磁盘文件。
    #[test]
    fn corpus_regression_hash_guard_reports_fix_command() {
        let base = RegressionMetrics {
            pairs_hash: "aaaaaaaa1111".into(),
            docsets_hash: "bbbbbbbb2222".into(),
            ..Default::default()
        };
        assert!(hash_mismatch_message(&base, &base.clone()).is_none(), "hash 一致应无报错");
        let changed = RegressionMetrics { pairs_hash: "cccccccc3333".into(), ..base.clone() };
        let msg = hash_mismatch_message(&base, &changed).expect("hash 不一致应报错");
        assert!(msg.contains("BIDGUARD_WRITE_BASELINE=1"), "报错须含修复命令：{msg}");
        assert!(msg.contains("corpus_regression"), "报错须含目标测试名：{msg}");
        let docs_changed = RegressionMetrics { docsets_hash: "dddddddd4444".into(), ..base.clone() };
        assert!(hash_mismatch_message(&base, &docs_changed).is_some(), "docsets 变更也应报错");
    }

    /// 门禁三向阈值：per-label F1 双向漂移 >2pp、召回率降 >1pp、AUC 降 >0.03 触发；带内不触发。
    #[test]
    fn corpus_regression_gate_thresholds() {
        let mk = |f1: f64, recall: f64, auc: f64| {
            let mut labels = BTreeMap::new();
            labels.insert("same".to_string(), LabelMetric { precision: 0.9, recall: 0.9, f1, support: 10 });
            let mut rbl = BTreeMap::new();
            rbl.insert("same".to_string(), recall);
            RegressionMetrics {
                recall_rate: recall,
                recall_by_label: rbl,
                labels,
                collusion_auc: auc,
                ..Default::default()
            }
        };
        let base = mk(0.90, 1.00, 1.00);
        assert!(gate_failures(&base, &base).is_empty(), "完全一致不触发");
        assert!(!gate_failures(&base, &mk(0.93, 1.00, 1.00)).is_empty(), "F1 上升 3pp 触发（双向）");
        assert!(!gate_failures(&base, &mk(0.87, 1.00, 1.00)).is_empty(), "F1 下降 3pp 触发");
        assert!(gate_failures(&base, &mk(0.91, 1.00, 1.00)).is_empty(), "F1 变动 1pp 不触发");
        assert!(!gate_failures(&base, &mk(0.90, 0.98, 1.00)).is_empty(), "召回率降 2pp 触发");
        assert!(gate_failures(&base, &mk(0.90, 1.00, 1.00)).is_empty(), "召回率不降不触发");
        assert!(!gate_failures(&base, &mk(0.90, 1.00, 0.95)).is_empty(), "AUC 降 0.05 触发");
        assert!(gate_failures(&base, &mk(0.90, 1.00, 1.00)).is_empty(), "AUC 不降不触发");
    }

    /// 三带门禁（W6-4 验收②③）：分流模式变更、带内错误率恶化、复核带过宽三类都要红灯；
    /// 未启用分流（review-all）时复核带占比 100% 不算失败（那是明示的回退路径，不是回退）。
    #[test]
    fn corpus_regression_band_gate() {
        let mk = |routing: &str, pass_fnr: f64, flag_fpr: f64, review: f64| RegressionMetrics {
            bands: BandMetrics {
                routing: routing.into(),
                alpha: 0.05,
                beta: 0.05,
                pass_fnr,
                flag_fpr,
                review_share: review,
                ..Default::default()
            },
            ..Default::default()
        };
        let base = mk("three-band", 0.03, 0.02, 0.20);
        assert!(gate_failures(&base, &base).is_empty(), "完全一致不触发");
        assert!(!gate_failures(&base, &mk("review-all", 0.0, 0.0, 1.0)).is_empty(), "分流模式变更必须显式入库");
        assert!(!gate_failures(&base, &mk("three-band", 0.06, 0.02, 0.20)).is_empty(), "漏检率涨 3pp 触发");
        assert!(gate_failures(&base, &mk("three-band", 0.04, 0.02, 0.20)).is_empty(), "漏检率涨 1pp 不触发");
        assert!(!gate_failures(&base, &mk("three-band", 0.03, 0.05, 0.20)).is_empty(), "误报率涨 3pp 触发");
        assert!(!gate_failures(&base, &mk("three-band", 0.03, 0.02, 0.45)).is_empty(), "复核带 45% 超上限触发");
        // 绝对线：α=5% + 2pp = 7%，8% 的漏检率即便基线更高也要红。
        let loose = mk("three-band", 0.09, 0.02, 0.20);
        assert!(!gate_failures(&loose, &mk("three-band", 0.08, 0.02, 0.20)).is_empty(), "漏检率超 α+2pp 绝对线");
        // review-all：复核带 100% 是明示回退路径，不判失败。
        let ra = mk("review-all", 0.0, 0.0, 1.0);
        assert!(gate_failures(&ra, &ra).is_empty(), "review-all 下复核带 100% 不算门禁失败");
    }

    /// Platt 拟合 + split conformal 分位 + ECE 三件套的数学正确性（离线、秒级、不读语料）。
    #[test]
    fn platt_conformal_and_ece_are_correct() {
        // 构造 logit 线性的合成数据：真参数 a=8、b=-4 ⇒ 拟合应把分界点还原到 s≈0.5 附近。
        let mut samples = Vec::new();
        for i in 0..=200 {
            let s = i as f64 / 200.0;
            let p = 1.0 / (1.0 + (-(8.0 * s - 4.0)).exp());
            // 每个分数点按真概率放置正负样本各若干，避免随机数带来的不可复现。
            let pos = (p * 10.0).round() as usize;
            for k in 0..10 {
                samples.push(CalibSample { score: s, y: if k < pos { 1.0 } else { 0.0 } });
            }
        }
        let (a, b) = fit_platt(&samples, 1e-6);
        assert!(a > 0.0, "斜率必须为正（单调不减）");
        let boundary = -b / a;
        assert!((boundary - 0.5).abs() < 0.05, "分界点应还原到 0.5 附近，实际 {boundary}");

        // 共形分位：n=19、rate=0.05 ⇒ k=ceil(20*0.95)=19 ⇒ 取最大值；n=18 时保证不成立 → None。
        let v: Vec<f64> = (1..=19).map(|i| i as f64 / 19.0).collect();
        assert_eq!(conformal_quantile(&v, 0.05), Some(v[18]));
        assert_eq!(conformal_quantile(&v[..18], 0.05), None, "样本不足以给出 5% 保证时必须返回 None");
        assert_eq!(conformal_quantile(&[], 0.05), None);
        // 覆盖率语义：不合格分 ≤ 分位的样本占比 ≥ 1−α。
        let n = 100usize;
        let v: Vec<f64> = (0..n).map(|i| i as f64 / n as f64).collect();
        let q = conformal_quantile(&v, 0.10).unwrap();
        let covered = v.iter().filter(|x| **x <= q).count() as f64 / n as f64;
        assert!(covered >= 0.90, "共形覆盖率不足：{covered}");

        // ECE：完美校准 → 0；恒定 0.9 但实际一半正样本 → 0.4。
        let probs = vec![0.1, 0.1, 0.9, 0.9];
        let ys = vec![0.0, 0.0, 1.0, 1.0];
        assert!(ece(&probs, &ys, 10) < 0.11);
        let over = vec![0.9, 0.9, 0.9, 0.9];
        let half = vec![1.0, 0.0, 1.0, 0.0];
        assert!((ece(&over, &half, 10) - 0.4).abs() < 1e-9, "过自信的 ECE 应为 |0.5−0.9|");
        assert!(ece(&[], &[], 10).is_nan());
    }

    /// 拟合确定性（验收⑥的拟合侧）：同语料两次 fit_calibration 产出逐字节一致的文件，
    /// 且随包 score_calib.json 与「当前语料 + 当前代码」重新拟合的结果一致
    /// （不一致 = 有人改了语料/打分却没重跑 fit-calib，校准与运行时口径已脱节）。
    #[test]
    fn fit_calibration_is_deterministic_and_matches_shipped_file() {
        let jieba = Jieba::new();
        let dir = corpus_dir();
        let (m1, r1) = fit_calibration(&jieba, &dir);
        let (m2, r2) = fit_calibration(&jieba, &dir);
        assert_eq!(calib_json(&m1, &r1), calib_json(&m2, &r2), "两次拟合必须逐字节一致");
        let shipped = std::fs::read_to_string(default_calib_path()).expect("随包校准文件应存在");
        let shipped: serde_json::Value = serde_json::from_str(&shipped).unwrap();
        let fresh: serde_json::Value = serde_json::from_str(&calib_json(&m1, &r1)).unwrap();
        for key in ["type", "platt", "isotonic", "thresholds", "alpha", "beta", "routing"] {
            assert_eq!(
                shipped.get(key),
                fresh.get(key),
                "随包 score_calib.json 的 {key} 与当前语料/代码重拟合结果不一致：\
                 请重跑 cargo run --bin corpusgen --features dev-tools -- fit-calib"
            );
        }
    }

    /// 语料 hash 守卫必须【跨平台】成立：Windows 检出会把文本夹具转成 CRLF，若按原始字节
    /// 哈希，同一提交在 Windows 上算出的 pairs_hash/docsets_hash 与 macOS 落库的基线不同，
    /// CI 会误报「语料已变更但基线未同步」（实测于 windows-latest）。故守卫按换行归一后的
    /// 内容判定；本测试钉死 CRLF/CR/LF 三种换行的哈希一致，且不误伤二进制字节。
    #[test]
    fn corpus_hash_is_line_ending_agnostic() {
        let lf = b"{\"a\":1}\n{\"b\":2}\n".to_vec();
        let crlf = b"{\"a\":1}\r\n{\"b\":2}\r\n".to_vec();
        let cr = b"{\"a\":1}\r{\"b\":2}\r".to_vec();
        let h = |b: &[u8]| crate::engine::normalize::sha256_hex(&normalize_eol(b));
        assert_eq!(h(&lf), h(&crlf), "CRLF 与 LF 内容相同，哈希须一致");
        assert_eq!(h(&lf), h(&cr), "CR 与 LF 内容相同，哈希须一致");
        // 真实内容变更仍须被检出（守卫不能因归一而失灵）。
        assert_ne!(h(&lf), h(b"{\"a\":2}\n{\"b\":2}\n"), "内容变更须改变哈希");
        // 二进制（docx 的 zip 头）不参与归一：这里仅验证 normalize_eol 不改无 CR 的字节。
        let zip_head = [0x50u8, 0x4B, 0x03, 0x04, 0x00, 0x0A];
        assert_eq!(normalize_eol(&zip_head), zip_head.to_vec());
    }

    /// 语料的报价清单必须能被 M6 数值层解析——否则 docsets 注入的「等比乘系数」证据不可读，
    /// numeric 恒 None、五类数值信号在门禁中恒不触发（M7 拟合 LR 时数值特征会成为死列）。
    /// 曾因表头用「序号|设备名称及服务内容|单价（元）|工期」（仅 2 列可识别、3 行 < n≥10 门槛）
    /// 而静默失效，故以本测试钉死：清单可解析 + 围标组呈严格等比 + 独立组无线性关系。
    #[test]
    fn docset_price_tables_are_boq_parsable_and_carry_planted_ratio() {
        use crate::engine::boq;
        let base_units = [1200i64, 3400, 760];
        let extract = |rows: &[Vec<String>]| -> Vec<boq::BoqItem> {
            let inputs: Vec<boq::TableRowInput> = rows
                .iter()
                .enumerate()
                .map(|(i, r)| boq::TableRowInput {
                    chunk_id: format!("c{i}"),
                    text: r.join(" | "),
                    page: None,
                    order_index: i as i64,
                })
                .collect();
            boq::extract_document(&inputs).items
        };
        // 围标组：同基准、份间不同 ratio ⇒ 可解析且严格等比。
        let a = extract(&price_rows(&base_units, 100, None));
        let b = extract(&price_rows(&base_units, 108, None));
        assert!(a.len() >= 10, "清单行数须 ≥10 以越过规律性/相关性 n≥10 门槛，实得 {}", a.len());
        assert_eq!(a.len(), b.len(), "同基准两份清单条目数应一致");
        assert!(a.iter().all(|it| it.unit_price.is_some() && it.total_price.is_some()));
        assert!(a.iter().all(|it| it.code.is_some()), "编码列须被识别（按编码对齐）");
        for (x, y) in a.iter().zip(b.iter()) {
            let (px, py) = (x.unit_price.unwrap(), y.unit_price.unwrap());
            // 整数截断带来 ≤1 元噪声，比值仍应贴近 1.08。
            assert!((py / px - 1.08).abs() < 0.01, "围标组份间应呈等比：{px} → {py}");
        }
        // 独立组：逐行扰动 ⇒ 比值离散（非恒定），不构成等比。
        let i1 = extract(&price_rows(&base_units, 100, Some(11)));
        let i2 = extract(&price_rows(&base_units, 100, Some(22)));
        let ratios: Vec<f64> = i1
            .iter()
            .zip(i2.iter())
            .map(|(x, y)| y.unit_price.unwrap() / x.unit_price.unwrap())
            .collect();
        let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
        let spread = ratios.iter().map(|r| (r - mean).abs()).fold(0.0f64, f64::max);
        assert!(spread > 0.02, "独立组比值应离散（无线性关系），实得最大偏离 {spread:.4}");
    }

    /// 慢档（本地手动，#[ignore]）：追加语义层（沿用 BIDGUARD_EMBED_DIR 本地缓存模型）。
    /// 打印含语义的分层全表；有 baseline_metrics_full.json 时对照门禁，否则仅速览。
    /// 围标层 AUC 仍走无模型层（取证信号为主，语义对其影响可忽略）。
    #[test]
    #[ignore] // 需本地缓存语义模型：BIDGUARD_EMBED_DIR=<dir> cargo test --features dev-tools corpus_regression_full -- --ignored --nocapture
    fn corpus_regression_full() {
        use crate::engine::embed;
        let dir = corpus_dir();
        let jieba = Jieba::new();
        let spec = embed::resolve("bge-zh");
        let mut slot: embed::LoadedEmbedder = None;
        let model = embed::ensure(&mut slot, spec, false).unwrap_or_else(|| {
            panic!(
                "语义模型不可用；请设置 BIDGUARD_EMBED_DIR 指向本地缓存的 {} 模型目录后重试",
                spec.id
            )
        });

        let pairs = read_pairs(&dir.join("pairs.jsonl"));
        let (recall_rate, recall_by_label, labels, macro_f1) =
            pair_stats(&jieba, &pairs, |a, b| {
                match embed::embed_batch(model, &[a.to_string(), b.to_string()], spec.id) {
                    Some(v) if v.len() == 2 => {
                        let cos = embed::cosine(&v[0], &v[1]);
                        (Some(cos), Some(vec![Some(v[0].clone()), Some(v[1].clone())]))
                    }
                    _ => (None, None),
                }
            });
        let (auc, mean_pos, mean_neg, ndoc) = docset_auc(&jieba, &dir);
        let cur = RegressionMetrics {
            lane: "full".into(),
            recall_rate,
            recall_by_label,
            labels,
            macro_f1,
            // 三带层用无模型层口径（校准输入是 final_score；慢档的语义分不参与分带阈值）。
            bands: band_metrics(&jieba, &pairs),
            collusion_auc: auc,
            mean_collusion_score: mean_pos,
            mean_independent_score: mean_neg,
            pairs_count: pairs.len(),
            docsets_count: ndoc,
            pairs_hash: sha256_of_file(&dir.join("pairs.jsonl")),
            docsets_hash: docsets_hash(&dir),
            git_rev: git_rev(),
            generated_at: today_utc(),
            note: format!("语义层：bge-zh({})；AUC 仍走无模型层", spec.id),
        };
        let baseline_path = dir.join("baseline_metrics_full.json");
        if baseline_write_mode() {
            let body = serde_json::to_string_pretty(&cur).expect("serialize");
            std::fs::write(&baseline_path, format!("{body}\n")).expect("写全档基线");
            eprintln!("[corpus_regression_full] 已写入全档基线 → {}", baseline_path.display());
            eprintln!("{}", render_single(&cur));
            return;
        }
        match std::fs::read_to_string(&baseline_path) {
            Ok(raw) => {
                let base: RegressionMetrics = serde_json::from_str(&raw).expect("解析全档基线");
                let table = render_compare(&base, &cur);
                let failures = gate_failures(&base, &cur);
                if !failures.is_empty() {
                    panic!("全档回归门禁失败（{} 项）：\n{}\n{}", failures.len(), failures.join("\n"), table);
                }
                eprintln!("[corpus_regression_full] 通过\n{table}");
            }
            Err(_) => {
                eprintln!(
                    "[corpus_regression_full] 无全档基线（{}），仅速览；如需入库：BIDGUARD_WRITE_BASELINE=1 ...",
                    baseline_path.display()
                );
                eprintln!("{}", render_single(&cur));
            }
        }
    }
}

