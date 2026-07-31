// PDF 渲染-OCR 抽样交叉验证（W2-4）：文字层前两级抽取（pdfium/pdf-extract）成功后，
// 对确定性抽样的若干页做「渲染成图 → OCR 识别 → 与文字层逐页比对」，封堵两类让文字层
// 100% 失真而下游算法无声失效的攻击：
//   - PDF Mirage（字体 ToUnicode 重映射 / 图片化正文）：渲染一套、抽取另一套 →
//     文字层与 OCR 内容对不上 → 中位内容失配 > 0.35。
//   - PDFuzz（坐标乱序）：字符集合对、顺序错 → Dice 高但顺序分低。
//
// 本模块是纯逻辑层（确定性抽样 + 2-gram Dice + LCS 近似顺序分 + 判定阈值），不触碰
// pdfium / OCR / 磁盘——渲染与 OCR 由 parse.rs 注入逐页文本后调 evaluate_*，使核心逻辑
// 可用假 OCR 文本离线单测，不联网。
//
// §1.5 产品纪律：命中是「检测到疑似规避特征，请人工复核」的线索级结论，绝不下
// 「规避/串通」定性；跳过（pdfium/OCR 不可用）不产生「检查通过/清白」背书。
use crate::engine::normalize::{normalize_sanitized, sanitize_with_stats, NormalizeOptions};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};

/// 抽样页数上限：K = min(SAMPLE_K, 页数)。首页 + 末页 + 均匀间隔页，可疑页优先顶替间隔页。
pub const SAMPLE_K: usize = 5;
/// 中位内容失配阈值：> 此值判「字体重映射/图片化正文」。⚠️ 未经语料校准（等 scheme §9.3
/// 合成语料回测）：低质打印/密集表格/印章覆盖会推高 OCR 噪声，判定措辞须为「请人工复核」。
pub const MISMATCH_THRESHOLD: f64 = 0.35;
/// 坐标乱序判定：中位 Dice ≥ 此值（内容对得上）。⚠️ 未经校准。
pub const SHUFFLE_DICE_MIN: f64 = 0.80;
/// 坐标乱序判定：中位顺序分 < 此值（顺序对不上）。⚠️ 未经校准。
pub const SHUFFLE_ORDER_MAX: f64 = 0.50;

/// 顺序分 LCS 的字符截断上限（控 O(n·m)：单页正常正文 <2000 字，截断兜住畸形超长页）。
const ORDER_TRUNCATE: usize = 4000;
/// pdf-extract 路径（块无页码）的 shingle 长度：OCR 页 8-gram 在全文文字层的包含率。
const SHINGLE_K: usize = 8;
/// 参与中位统计的最小归一化字符数：低于此的近空页（都空/都是页码）不携带信号，排除出
/// 中位（否则大量空页会把中位拉向「干净」掩盖注入，或把「都空」误判为对得上）。
const MIN_INFORMATIVE_CHARS: usize = 8;

/// 判定种类（机器标识；verdict.kind 落 evasion_json，供 W2-5 evasion 信号消费）。
pub const KIND_FONT_REMAP: &str = "fontRemap";
pub const KIND_COORD_SHUFFLE: &str = "coordShuffle";

/// 逐页交叉验证明细。mismatch = 1 - Dice（内容失配），order_score = LCS 近似顺序分。
/// informative=false 的近空页不计入中位（仍列出供下钻）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct XCheckPage {
    /// 页码（1 起）。
    pub page: u32,
    /// 内容失配 = 1 - 2-gram Dice。
    pub mismatch: f64,
    /// 顺序分（LCS / min 长度）；pdf-extract 路径无逐页顺序，恒 1.0（不判乱序）。
    pub order_score: f64,
    /// 是否计入中位（归一化后有足量可比文本）。
    pub informative: bool,
}

/// 命中结论：kind 为机器标识，label 为呈现用中文（§1.5 线索级措辞）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct XCheckVerdict {
    pub kind: String,
    pub label: String,
}

/// 文档级交叉验证结果（命中时并入 documents.evasion_json 的 xcheck 子对象）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct XCheckResult {
    /// 抽样页码（1 起，确定性升序）。
    pub sampled_pages: Vec<u32>,
    /// 逐页明细。
    pub pages: Vec<XCheckPage>,
    /// 中位内容失配（仅 informative 页）。
    pub median_mismatch: f64,
    /// 中位顺序分（仅 informative 页）。
    pub median_order: f64,
    /// 命中结论；None 表示未命中（不代表清白）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verdict: Option<XCheckVerdict>,
    /// 跳过原因（pdfium/OCR 不可用、无可比文本等）；Some 即未执行有效比对。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped: Option<String>,
}

impl XCheckResult {
    /// 跳过：记原因、不命中、不阻塞导入、不做清白背书。
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self { skipped: Some(reason.into()), ..Default::default() }
    }

    /// 是否命中（值得回落 OCR + 写 evasion_json + 供 evasion 围标信号消费）。
    pub fn is_hit(&self) -> bool {
        self.verdict.is_some()
    }

    /// 命中的中文标签（回落提示语用）；未命中返回空串。
    pub fn hit_label(&self) -> &str {
        self.verdict.as_ref().map(|v| v.label.as_str()).unwrap_or("")
    }
}

/// 确定性抽样：K = min(k_max, total) 页——首页 + 末页固定为锚点，其余间隔槽优先由
/// suspect（0 基页索引，调用方按可疑度排序）顶替，不足再用均匀间隔页补齐。
/// 同 (total, suspect) 输入产出逐元素相等的升序页索引（保可复现）。
pub fn sample_pages(total: usize, suspect: &[usize], k_max: usize) -> Vec<usize> {
    if total == 0 || k_max == 0 {
        return Vec::new();
    }
    let k = k_max.min(total);
    let last = total - 1;
    let mut chosen: BTreeSet<usize> = BTreeSet::new();
    chosen.insert(0);
    if k >= 2 {
        chosen.insert(last);
    }
    // 均匀基准（含首末），四舍五入取整；k==1 时退化为仅首页。
    let uniform_interior = (0..k).map(move |j| {
        if k <= 1 {
            0
        } else {
            (j * (total - 1) + (k - 1) / 2) / (k - 1)
        }
    });
    // 优先级：可疑页（调用方给定顺序）在前，均匀间隔页在后；范围内、非重复者填至 k 个。
    let priority = suspect
        .iter()
        .copied()
        .filter(|&p| p < total)
        .chain(uniform_interior.filter(|&p| p != 0 && p != last));
    for p in priority {
        if chosen.len() >= k {
            break;
        }
        chosen.insert(p);
    }
    chosen.into_iter().collect()
}

/// 字符 2-gram 多重集 Dice：2·|交集|/(|A|+|B|)，内容一致性（对局部乱序不敏感即视觉相同）。
/// 输入为已归一化文本。两侧均无 2-gram（各 <2 字）时退化为整串相等判定。
pub fn dice_2gram(a: &str, b: &str) -> f64 {
    let ma = char_bigrams(a);
    let mb = char_bigrams(b);
    let total_a: u64 = ma.values().sum();
    let total_b: u64 = mb.values().sum();
    if total_a == 0 && total_b == 0 {
        return if a == b { 1.0 } else { 0.0 };
    }
    if total_a == 0 || total_b == 0 {
        return 0.0;
    }
    let (small, large) = if ma.len() <= mb.len() { (&ma, &mb) } else { (&mb, &ma) };
    let mut inter: u64 = 0;
    for (k, &va) in small {
        if let Some(&vb) = large.get(k) {
            inter += va.min(vb);
        }
    }
    2.0 * inter as f64 / (total_a + total_b) as f64
}

fn char_bigrams(s: &str) -> HashMap<(char, char), u64> {
    let chars: Vec<char> = s.chars().collect();
    let mut m: HashMap<(char, char), u64> = HashMap::new();
    for w in chars.windows(2) {
        *m.entry((w[0], w[1])).or_insert(0) += 1;
    }
    m
}

/// 顺序分：LCS(a,b) / min(len)，顺序一致性（内容对得上但顺序被打乱时显著偏低）。
/// 输入为已归一化文本，各截断 ORDER_TRUNCATE 字符控 O(n·m)。
pub fn order_score(a: &str, b: &str) -> f64 {
    let ca: Vec<char> = a.chars().take(ORDER_TRUNCATE).collect();
    let cb: Vec<char> = b.chars().take(ORDER_TRUNCATE).collect();
    if ca.is_empty() || cb.is_empty() {
        return if ca.is_empty() && cb.is_empty() { 1.0 } else { 0.0 };
    }
    let lcs = lcs_len(&ca, &cb);
    lcs as f64 / ca.len().min(cb.len()) as f64
}

/// 最长公共子序列长度（滚动两行，O(n·m) 时间、O(m) 空间）。
fn lcs_len(a: &[char], b: &[char]) -> usize {
    let m = b.len();
    let mut prev = vec![0u32; m + 1];
    let mut cur = vec![0u32; m + 1];
    for &ca in a {
        for j in 1..=m {
            cur[j] = if ca == b[j - 1] {
                prev[j - 1] + 1
            } else {
                cur[j - 1].max(prev[j])
            };
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    prev[m] as usize
}

/// pdf-extract 路径（块无页码）：一页 OCR 文本的 8-gram shingle 在全文文字层的包含率。
/// 输入为原始文本（内部归一化）。文字层被字体重映射时包含率趋近 0。
/// 返回 None 表示 OCR 页无足量 shingle（近空，不携带信号）。
pub fn shingle_containment(ocr: &str, full_layer: &str) -> Option<f64> {
    let ocr_n = norm(ocr);
    let layer_n = norm(full_layer);
    let ocr_sh = char_shingles(&ocr_n, SHINGLE_K);
    if ocr_sh.is_empty() {
        return None;
    }
    let layer_sh = char_shingles(&layer_n, SHINGLE_K);
    let present = ocr_sh.iter().filter(|s| layer_sh.contains(*s)).count();
    Some(present as f64 / ocr_sh.len() as f64)
}

fn char_shingles(s: &str, k: usize) -> HashSet<Box<[char]>> {
    let chars: Vec<char> = s.chars().collect();
    let mut set: HashSet<Box<[char]>> = HashSet::new();
    if chars.len() < k {
        return set;
    }
    for w in chars.windows(k) {
        set.insert(w.to_vec().into_boxed_slice());
    }
    set
}

/// 归一化（忽略空白/标点/大小写，与 W2-1/2 入口对抗层同口径）：sanitize（NFKC + 隐形码点
/// 剥离 + 同形字折叠）→ 后半程（中文数字 + 去标点空白）。文字层与 OCR 各过一遍再比对。
fn norm(s: &str) -> String {
    let (sani, _) = sanitize_with_stats(s);
    normalize_sanitized(
        &sani,
        &NormalizeOptions { ignore_case: true, ignore_punctuation: true, ignore_whitespace: true },
    )
}

/// pdfium 路径（块带页码）：逐页比对文字层与 OCR 文本。三参数逐元素对应（同一抽样页）。
pub fn evaluate_paged(
    sampled_1based: &[u32],
    layer_pages: &[String],
    ocr_pages: &[String],
) -> XCheckResult {
    let mut result = XCheckResult {
        sampled_pages: sampled_1based.to_vec(),
        ..Default::default()
    };
    let n = sampled_1based.len().min(layer_pages.len()).min(ocr_pages.len());
    let mut mismatches: Vec<f64> = Vec::new();
    let mut orders: Vec<f64> = Vec::new();
    for i in 0..n {
        let layer = norm(&layer_pages[i]);
        let ocr = norm(&ocr_pages[i]);
        let informative = layer.chars().count().max(ocr.chars().count()) >= MIN_INFORMATIVE_CHARS;
        let dice = dice_2gram(&layer, &ocr);
        let order = order_score(&layer, &ocr);
        let mismatch = 1.0 - dice;
        if informative {
            mismatches.push(mismatch);
            orders.push(order);
        }
        result.pages.push(XCheckPage {
            page: sampled_1based[i],
            mismatch,
            order_score: order,
            informative,
        });
    }
    finalize(&mut result, mismatches, orders, true);
    result
}

/// pdf-extract 路径：逐 OCR 页算 8-gram 包含率（无逐页顺序 → 不判坐标乱序）。
pub fn evaluate_shingle(
    sampled_1based: &[u32],
    full_layer: &str,
    ocr_pages: &[String],
) -> XCheckResult {
    let mut result = XCheckResult {
        sampled_pages: sampled_1based.to_vec(),
        ..Default::default()
    };
    let n = sampled_1based.len().min(ocr_pages.len());
    let mut mismatches: Vec<f64> = Vec::new();
    for i in 0..n {
        match shingle_containment(&ocr_pages[i], full_layer) {
            Some(ratio) => {
                let mismatch = 1.0 - ratio;
                mismatches.push(mismatch);
                result.pages.push(XCheckPage {
                    page: sampled_1based[i],
                    mismatch,
                    order_score: 1.0, // 无逐页顺序信息，恒满分（不触发乱序判定）
                    informative: true,
                });
            }
            None => result.pages.push(XCheckPage {
                page: sampled_1based[i],
                mismatch: 0.0,
                order_score: 1.0,
                informative: false,
            }),
        }
    }
    finalize(&mut result, mismatches, Vec::new(), false);
    result
}

/// 收口：无 informative 页 → skipped；否则算中位并按阈值判 verdict。
/// order_capable=true 时才可能判坐标乱序（pdf-extract 路径无逐页顺序，传 false）。
fn finalize(result: &mut XCheckResult, mismatches: Vec<f64>, orders: Vec<f64>, order_capable: bool) {
    if mismatches.is_empty() {
        result.skipped = Some("抽样页无可比文本（近空/全为页码）".to_string());
        return;
    }
    result.median_mismatch = median(mismatches);
    result.median_order = if orders.is_empty() { 1.0 } else { median(orders) };
    // 字体重映射/图片化正文优先（内容整体对不上）。
    if result.median_mismatch > MISMATCH_THRESHOLD {
        result.verdict = Some(XCheckVerdict {
            kind: KIND_FONT_REMAP.to_string(),
            label: "疑似字体重映射/图片化正文".to_string(),
        });
        return;
    }
    // 坐标乱序：内容对得上（Dice ≥ 阈值即 mismatch ≤ 1-阈值）但顺序对不上。
    if order_capable
        && result.median_mismatch <= 1.0 - SHUFFLE_DICE_MIN
        && result.median_order < SHUFFLE_ORDER_MAX
    {
        result.verdict = Some(XCheckVerdict {
            kind: KIND_COORD_SHUFFLE.to_string(),
            label: "疑似坐标乱序".to_string(),
        });
    }
}

/// 中位数（非空 vec；偶数取中间两数均值）。
fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sampling_is_deterministic_first_last_and_uniform() {
        // 10 页、K=5、无可疑页：首 0 + 末 9 + 均匀间隔，两次调用逐元素相等
        let a = sample_pages(10, &[], SAMPLE_K);
        let b = sample_pages(10, &[], SAMPLE_K);
        assert_eq!(a, b, "同输入同样本可复现");
        assert_eq!(a.len(), 5);
        assert_eq!(a.first(), Some(&0), "含首页");
        assert_eq!(a.last(), Some(&9), "含末页");
        assert!(a.windows(2).all(|w| w[0] < w[1]), "升序去重");
    }

    #[test]
    fn sampling_prioritizes_suspect_pages_over_interval() {
        // 可疑页 4（0 基）应顶替某个均匀间隔页，首末锚点保留
        let base = sample_pages(10, &[], SAMPLE_K);
        let with_suspect = sample_pages(10, &[4], SAMPLE_K);
        assert_eq!(with_suspect.len(), 5);
        assert_eq!(with_suspect.first(), Some(&0));
        assert_eq!(with_suspect.last(), Some(&9));
        assert!(with_suspect.contains(&4), "可疑页顶替进抽样");
        assert!(!base.contains(&4), "无可疑时 4 不在均匀基准里（确认确实顶替）");
    }

    #[test]
    fn sampling_edge_cases() {
        assert_eq!(sample_pages(0, &[], 5), Vec::<usize>::new(), "无页面");
        assert_eq!(sample_pages(1, &[], 5), vec![0], "单页");
        assert_eq!(sample_pages(2, &[], 5), vec![0, 1], "两页");
        assert_eq!(sample_pages(3, &[], 5), vec![0, 1, 2], "三页取全");
        // 页数 < K：取全部页，不重复
        assert_eq!(sample_pages(4, &[], 5), vec![0, 1, 2, 3]);
        // 可疑页越界被过滤，不 panic
        assert_eq!(sample_pages(5, &[99], 5), vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn dice_identical_disjoint_and_partial() {
        let s = "投标报价为人民币一千两百八十万元整包含全部软硬件费用";
        assert!((dice_2gram(s, s) - 1.0).abs() < 1e-9, "同串 Dice=1");
        assert_eq!(dice_2gram("abcdefgh", "zyxwvuts"), 0.0, "无共同 2-gram Dice=0");
        let d = dice_2gram("投标报价为人民币", "投标报价含税合计");
        assert!(d > 0.0 && d < 1.0, "部分重叠介于 0..1，实际 {d}");
        // 单字退化：相等 1，不等 0
        assert_eq!(dice_2gram("甲", "甲"), 1.0);
        assert_eq!(dice_2gram("甲", "乙"), 0.0);
    }

    /// 坐标乱序模型：N 个短 run（每 run 内字符原序保留 → run 内 2-gram 不变），run 顺序整体
    /// 逆置 → 跨 run 顺序全乱。返回 (视觉正确串, run 逆序串)。字符全局唯一，Dice 高、LCS 低。
    fn block_shuffled(run_len: usize, runs: usize) -> (String, String) {
        let chars: Vec<char> = ('\u{4e00}'..).take(run_len * runs).collect();
        let visual: String = chars.iter().collect();
        let mut blocks: Vec<String> =
            chars.chunks(run_len).map(|c| c.iter().collect::<String>()).collect();
        blocks.reverse();
        (visual, blocks.concat())
    }

    #[test]
    fn order_score_detects_shuffle() {
        let s = "abcdefghijklmnop";
        assert!((order_score(s, s) - 1.0).abs() < 1e-9, "同串顺序分=1");
        // 完全逆序：LCS 极短 → 顺序分低
        let rev: String = s.chars().rev().collect();
        assert!(order_score(s, &rev) < 0.2, "逆序顺序分应很低，实际 {}", order_score(s, &rev));
        // run 顺序逆置（坐标乱序典型）：各 run 内字符原序保留、run 间顺序全乱 → 顺序分很低
        let (visual, shuffled) = block_shuffled(10, 20);
        assert!(order_score(&visual, &shuffled) < SHUFFLE_ORDER_MAX, "run 逆序顺序分应 <0.5，实际 {}", order_score(&visual, &shuffled));
    }

    #[test]
    fn paged_clean_document_does_not_trigger() {
        // 正常 PDF：OCR 与文字层近似一致（含少量 OCR 噪声）→ 中位失配 <0.15、不命中
        let layer = vec![
            "本项目采用分层解耦的微服务总体架构设计投标报价为人民币12800000元整".to_string(),
            "工期一百八十个日历日质保期三年提供七乘二十四小时响应服务".to_string(),
            "投标人承诺严格遵守招标文件的全部实质性要求并按期交付".to_string(),
        ];
        // OCR 版：个别字识别偏差，但整体高度一致
        let ocr = vec![
            "本项目采用分层解耦的微服务总体架构设计投标报价为人民币12800000元整".to_string(),
            "工期一百八十个日历日质保期三年提供七乘二十四小时响应服务".to_string(),
            "投标人承诺严格遵守招标文件的全部实质性要求并按期交付".to_string(),
        ];
        let r = evaluate_paged(&[1, 2, 3], &layer, &ocr);
        assert!(r.skipped.is_none());
        assert!(r.median_mismatch < 0.15, "中位失配应 <0.15，实际 {}", r.median_mismatch);
        assert!(r.verdict.is_none(), "正常文档不触发回落");
        assert!(!r.is_hit());
    }

    #[test]
    fn paged_font_remap_triggers_ocr_fallback() {
        // 字体重映射：文字层是与渲染无关的垃圾串，OCR 读出真实正文 → 中位失配 >0.35
        let layer = vec![
            "Xq7z Kp9w Rt2v Bn4m Ll8a Qs3d Wf6g Hj0c Zx5y".to_string(),
            "Vb1n Mk7l Op4i Ur2e Yt9w Ac6s Df3g Hj8k Ll0m".to_string(),
            "Qw2e Rt5y Ui8o Pa1s Df4g Hj7k Lz0x Cv3b Nm6q".to_string(),
        ];
        let ocr = vec![
            "本项目采用分层解耦的微服务总体架构设计投标报价为人民币12800000元整".to_string(),
            "工期一百八十个日历日质保期三年提供七乘二十四小时响应服务".to_string(),
            "投标人承诺严格遵守招标文件的全部实质性要求并按期交付".to_string(),
        ];
        let r = evaluate_paged(&[1, 2, 3], &layer, &ocr);
        assert!(r.median_mismatch > MISMATCH_THRESHOLD, "失配 {} 应 >0.35", r.median_mismatch);
        let v = r.verdict.as_ref().expect("应命中");
        assert_eq!(v.kind, KIND_FONT_REMAP);
        assert!(r.is_hit());
    }

    #[test]
    fn paged_coordinate_shuffle_triggers() {
        // 坐标乱序：OCR（视觉正确）与文字层字符集合相同但绘制顺序被打乱 → Dice 高、顺序分低。
        // run 内 2-gram 保留使 Dice ≥0.8，run 逆序使 LCS 塌到单 run 长度 → 顺序分很低。
        let (visual, shuffled) = block_shuffled(10, 20);
        let layer = vec![shuffled];
        let ocr = vec![visual];
        let r = evaluate_paged(&[1], &layer, &ocr);
        assert!(1.0 - r.median_mismatch >= SHUFFLE_DICE_MIN, "Dice {} 应 ≥0.8", 1.0 - r.median_mismatch);
        assert!(r.median_order < SHUFFLE_ORDER_MAX, "顺序分 {} 应 <0.5", r.median_order);
        let v = r.verdict.as_ref().expect("应命中坐标乱序");
        assert_eq!(v.kind, KIND_COORD_SHUFFLE);
    }

    #[test]
    fn paged_near_empty_pages_excluded_from_median() {
        // 近空页（都空/都是短串）不计入中位；若全为近空 → skipped 而非误判
        let layer = vec!["1".to_string(), "".to_string(), "  ".to_string()];
        let ocr = vec!["2".to_string(), "".to_string(), "".to_string()];
        let r = evaluate_paged(&[1, 2, 3], &layer, &ocr);
        assert!(r.skipped.is_some(), "全近空页应 skipped，不命中");
        assert!(r.verdict.is_none());
        assert!(!r.is_hit());
    }

    #[test]
    fn shingle_path_clean_and_remapped() {
        let full_layer = "本项目采用分层解耦的微服务总体架构设计投标报价为人民币12800000元整工期一百八十个日历日质保期三年";
        // OCR 页文本是文字层的子串 → 包含率高 → 不命中
        let clean = evaluate_shingle(&[1], full_layer, &["投标报价为人民币12800000元整工期一百八十个日历日".to_string()]);
        assert!(clean.median_mismatch < 0.2, "包含率高失配低，实际 {}", clean.median_mismatch);
        assert!(clean.verdict.is_none());
        // 文字层是垃圾（OCR 内容不在其中）→ 包含率≈0 → 命中字体重映射
        let remapped = evaluate_shingle(
            &[1],
            "Xq7zKp9wRt2vBn4mLl8aQs3dWf6gHj0cZx5yVb1nMk7lOp4i",
            &["投标报价为人民币12800000元整工期一百八十个日历日质保期三年".to_string()],
        );
        assert!(remapped.median_mismatch > MISMATCH_THRESHOLD, "失配 {} 应 >0.35", remapped.median_mismatch);
        assert_eq!(remapped.verdict.as_ref().unwrap().kind, KIND_FONT_REMAP);
        // shingle 路径不判坐标乱序（无逐页顺序）
        assert_ne!(remapped.verdict.as_ref().unwrap().kind, KIND_COORD_SHUFFLE);
    }

    #[test]
    fn skipped_is_not_a_hit_and_serializes_reason() {
        let s = XCheckResult::skipped("OCR 不可用（缺模型或识别失败）");
        assert!(!s.is_hit());
        assert!(s.verdict.is_none());
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("skipped"));
        assert!(!j.contains("verdict"), "未命中不序列化 verdict（不做清白背书）");
    }

    #[test]
    fn median_even_and_odd() {
        assert!((median(vec![0.2]) - 0.2).abs() < 1e-9);
        assert!((median(vec![0.1, 0.3, 0.5]) - 0.3).abs() < 1e-9);
        assert!((median(vec![0.1, 0.3]) - 0.2).abs() < 1e-9);
    }
}
