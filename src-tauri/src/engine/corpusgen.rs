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
use crate::engine::{clustering, collusion, diff, fingerprint, matrix, parse, scoring};
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

    // 先建齐全部段对的两侧分块，再用【全语料 IDF】填 TF-IDF——与生产 fill_tfidf 同口径。
    // 关键：逐对局部 IDF 会让 lexical(tfidf 余弦) 与 char_ngram(jaccard) 退化为近似同一信号，
    // W_LEXICAL/W_CHAR_NGRAM 的权重再分配对最终分几乎无影响（门禁对权重改动失灵）；全语料 IDF
    // 下常见模板词权重被压低，lexical 变为「区分性词」信号，与 ngram 分离，权重改动才真正传导。
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
    for ((v, sem_cos, embs), r) in built.iter_mut().zip(pairs) {
        let tl = static_label(&r.label);
        *total_lbl.entry(tl).or_default() += 1;
        v[0].tfidf = features::weighted_vec(&v[0].tokens, &idf);
        v[1].tfidf = features::weighted_vec(&v[1].tokens, &idf);
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
    use std::sync::atomic::AtomicBool;
    let cancel = AtomicBool::new(false);
    let sdir = dir.join("docsets").join(&manifest.docset_id);
    let mut doc_infos: Vec<DocInfo> = Vec::new();
    let mut per_doc_images: Vec<Vec<collusion::ImageFp>> = Vec::new();
    let mut evasion: Vec<Option<EvasionSummary>> = Vec::new();
    let mut chunks: Vec<CmpChunk> = Vec::new();
    // 商务标数值层（M6）：docsets 的围标正样本注入了「清单单价整组乘系数」（见 price_rows），
    // 若此处不建 BOQ，numeric 恒 None ⇒ 五类数值信号在门禁里恒不触发、注入的等比证据被白白丢弃
    // （M7 拟合 LR 时数值特征会成为死列）。故与生产同口径抽取表格行 → extract → align → pair_stats。
    let mut per_doc_rows: Vec<Vec<crate::engine::boq::TableRowInput>> = Vec::new();
    for (di, name) in manifest.docs.iter().enumerate() {
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
                    chunk_id: format!("{}-{}-{}", manifest.docset_id, di, rank),
                    text: b.text.clone(),
                    page: b.page.map(|p| p as i64),
                    order_index: rank as i64,
                });
            }
            let rel = if total > 1 { rank as f32 / (total - 1) as f32 } else { 0.0 };
            let mut c = regr_build_chunk(
                jieba,
                format!("{}-{}-{}", manifest.docset_id, di, rank),
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
            id: format!("{}-{}", manifest.docset_id, di),
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
    let col = collusion::assess_with(collusion::CollusionInputs {
        peak,
        clusters: &r_clusters,
        docs: &doc_infos,
        rsid_hits: &rsid,
        lineage_hits: &lineage,
        image_hits: &images,
        evasion: &evasion,
        numeric: numeric.as_ref(),
        ..Default::default()
    });
    col.score
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

fn sha256_of_file(p: &Path) -> String {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("读取 {} 失败: {e}", p.display()));
    crate::engine::normalize::sha256_hex(&bytes)
}

/// docsets 内容 hash：清单字节 ++ 逐组逐份（id + 文件名 + docx 字节）。
fn docsets_hash(dir: &Path) -> String {
    let manifest_path = dir.join("docsets.jsonl");
    let mut buf = std::fs::read(&manifest_path).unwrap_or_default();
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

/// 无模型层跑全语料 → 分层指标（+ 语料 hash + count）。git_rev/generated_at 由调用方填。
pub fn compute_fast_metrics(jieba: &Jieba, dir: &Path) -> RegressionMetrics {
    let pairs = read_pairs(&dir.join("pairs.jsonl"));
    let (recall_rate, recall_by_label, labels, macro_f1) =
        pair_stats(jieba, &pairs, |_, _| (None, None));
    let (auc, mean_pos, mean_neg, ndoc) = docset_auc(jieba, dir);
    RegressionMetrics {
        lane: "fast".into(),
        recall_rate,
        recall_by_label,
        labels,
        macro_f1,
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
