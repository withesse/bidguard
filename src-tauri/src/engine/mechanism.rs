// 机制感知筛查（W5-5）：评标办法配置 + 反事实基准价重算（C&D 单场退化版）。
//
// 【产品纪律 · 首版边界】（执行方案 §2 后置池裁决 + §1.5）：本层【只产出「基准价敏感性」
// 描述性分析】，结果写入 jobs.numeric_json.mechanism 仅供屏幕与报告引用，
// 【绝不进入 collusion 围标信号】——collusion::NumericEvidence.mechanism_flip_prob 保持恒 None，
// 不接线、不加权、不改分级。理由：v1 只支持一族评标公式；评标办法靠人工录入（录错即误导）；
// 嫌疑组由文本证据先验圈定，有循环论证观感。在数值层未经真实语料验证前不入分级。
//
// 纯函数层：无 DB、无 IO、【无随机数】——系数区间上取均匀格点而非随机采样，保证同输入
// 逐字节同输出（可复现是举证的前提）。
use serde::{Deserialize, Serialize};

/// v1 支持的唯一公式族：(去 m 高 n 低后) 算术平均 × 系数，最接近基准价者价格分最高。
pub const METHOD_AVG_BENCHMARK: &str = "avg_benchmark";
/// 最低评标价法：本层只做「最低价孤立度」描述，【禁用一切均值类统计】（机制分流写死在路由层）。
pub const METHOD_LOWEST: &str = "lowest";

/// 系数区间上的格点数（≥200；取奇数使区间端点与中点都恰好落在格点上）。
pub const COEFF_GRID_POINTS: usize = 201;
/// 断崖判定倍数：与次邻报价的间距 > 该倍数 × 中位间距 → support-bid（陪衬价）形态。
/// ⚠️ 经验值，未经真实语料校准；本节不进分级，仅作描述性标记。
const CLIFF_GAP_RATIO: f64 = 2.0;
/// 候选嫌疑组的规模（C&D 第一种候选组构造法：|g|∈{2,3}）。
const GROUP_MIN_SIZE: usize = 2;
const GROUP_MAX_SIZE: usize = 3;
/// 均值基准价至少需要的有效报价家数——2 家时剔除任一组都无从重算基准价。
const MIN_BIDS_FOR_BENCHMARK: usize = 3;
/// 候选组的文本证据门槛：文档级相似度（剔除招标引用后的主口径）达此值即算「已有文档证据」。
/// ⚠️ 经验值；本节不进分级，门槛只影响列出哪些组供人工核对。
pub const GROUP_TEXT_PEAK_MIN: f32 = 0.6;
/// 系数区间的合法边界（评标系数不可能为 0 或负、也不会超过 2）。
const COEFF_ABS_MIN: f64 = 0.01;
const COEFF_ABS_MAX: f64 = 2.0;

// —— §1.5 强制措辞：随数据一起落库，任何呈现层（UI / 六格式导出）都不得省略 ——

/// 本节性质声明（【最重要的一条】：不参与围标分级 + 人工录入需核对）。
pub const MECHANISM_NOTE: &str =
    "本节为反事实解释性分析，不参与围标分级；评标办法为人工录入，请核对公式与参数。";
/// 候选组构造依据声明（防「循环论证」观感）。
pub const GROUP_BASIS_NOTE: &str = "候选组由【已有文档证据】圈定（文本相似峰值 / 逐项单价雷同率 / 元数据同源），\
每组的构造依据已逐条标明；组的构造依据本身不构成串通认定。";
/// 投标总价来源声明（举证前提：读者要能核对锚定的是不是封面报价）。
pub const PRICE_SOURCE_NOTE: &str =
    "投标总价来源已逐份打标（取自投标总价行 / 取自清单合计 / 启发式回落），请核对是否与投标文件封面报价一致。";
/// 反事实口径声明（不是 p 值、不是概率）。
pub const COUNTERFACTUAL_NOTE: &str = "「翻转比例」是在系数区间的均匀格点上重算基准价得到的【反事实占比】，\
不是统计显著性、也不是串通概率；2–5 份标书的统计功效天然有限，须结合取证类证据由评标委员会依法认定。";
/// support-bid 形态说明。
pub const SUPPORT_BID_NOTE: &str = "「断崖式报价」指该报价位于分布端点且与次邻报价的间距超过中位间距的 2 倍；\
极端报价可能源于成本结构差异或对招标文件的不同理解，单独不构成陪标认定。";

/// 单次比对的评标办法（【仅请求级配置】——每个项目评标办法不同，不进全局默认）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EvaluationConfig {
    /// "avg_benchmark"（截尾均值×系数）| "lowest"（最低评标价）。其余值一律「不适用」。
    pub method: String,
    /// 计算基准价前去掉的最低报价个数 n。
    pub trim_lowest: usize,
    /// 计算基准价前去掉的最高报价个数 m。
    pub trim_highest: usize,
    /// 系数区间（含端点）。招标文件常给一个区间或一组抽取值，本层在区间上取均匀格点积分。
    pub coeff_min: f64,
    pub coeff_max: f64,
}

impl Default for EvaluationConfig {
    fn default() -> Self {
        EvaluationConfig {
            method: METHOD_AVG_BENCHMARK.to_string(),
            trim_lowest: 0,
            trim_highest: 0,
            coeff_min: 0.9,
            coeff_max: 1.0,
        }
    }
}

impl EvaluationConfig {
    /// 配置层面的合法性（命令层校验与本层「不适用」判定共用同一口径）。
    /// 返回 Err(原因) 时本节输出「不适用」而非硬算。
    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.method.as_str(), METHOD_AVG_BENCHMARK | METHOD_LOWEST) {
            return Err(format!("评标办法「{}」不属 v1 支持的公式族", self.method));
        }
        if self.method == METHOD_LOWEST {
            return Ok(());
        }
        if !(self.coeff_min.is_finite() && self.coeff_max.is_finite()) {
            return Err("系数区间不是有效数值".into());
        }
        if self.coeff_min < COEFF_ABS_MIN
            || self.coeff_max > COEFF_ABS_MAX
            || self.coeff_max < self.coeff_min
        {
            return Err(format!(
                "系数区间不合法（须满足 {COEFF_ABS_MIN} ≤ 下限 ≤ 上限 ≤ {COEFF_ABS_MAX}）"
            ));
        }
        Ok(())
    }

    /// 公式全文（【人工录入必须回显】——录错即误导，UI 与导出配置快照都要能逐字核对）。
    pub fn formula_text(&self) -> String {
        match self.method.as_str() {
            METHOD_AVG_BENCHMARK => format!(
                "基准价 =（全部有效投标总价去掉 {} 个最高、{} 个最低后的算术平均）× 系数 c，c ∈ [{:.4}, {:.4}]；\
投标总价最接近基准价者价格分最高。",
                self.trim_highest, self.trim_lowest, self.coeff_min, self.coeff_max
            ),
            METHOD_LOWEST => {
                "最低评标价法：投标总价最低者价格分最高。本节只作「最低价孤立度」描述，不计算均值基准价。"
                    .to_string()
            }
            other => format!("未识别的评标办法「{other}」。"),
        }
    }
}

/// 投标总价的来源打标（举证用：读者要知道这个数是从哪读出来的）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceSource {
    /// 清单中含「投标总价 / 投标报价」等字样的行的合价。
    TotalRow,
    /// 报价清单 Σ 合价。
    BoqSum,
    /// 回落：全文最大金额排除法（注册资本/业绩/保证金等语境已排除）。
    Heuristic,
}

impl PriceSource {
    pub fn as_str(self) -> &'static str {
        match self {
            PriceSource::TotalRow => "totalRow",
            PriceSource::BoqSum => "boqSum",
            PriceSource::Heuristic => "heuristic",
        }
    }
    /// 中文来源标签（措辞唯一来源，UI 与六格式导出共用）。
    pub fn label(self) -> &'static str {
        match self {
            PriceSource::TotalRow => "取自投标总价行",
            PriceSource::BoqSum => "取自清单合计",
            PriceSource::Heuristic => "启发式回落（全文最大金额）",
        }
    }
}

/// 一份文档的投标总价锚定结果。
#[derive(Debug, Clone, PartialEq)]
pub struct BidPrice {
    /// 文档在本次任务请求次序里的位次（十天干口径）。
    pub doc: usize,
    pub total: f64,
    pub source: PriceSource,
}

/// 候选组构造依据的一条（kind 稳定标识 + 可读描述）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceBasis {
    /// textPeak | identicalRate | metadata
    pub kind: String,
    pub detail: String,
}

/// 一对文档的既有证据（由 compare_service 从文本峰值 / W5-2 雷同率 / 元数据风险标记构造）。
/// basis 为空的对不参与候选组构造（C&D 第一种候选组构造法要求组内每对都有证据）。
#[derive(Debug, Clone, PartialEq)]
pub struct PairEvidence {
    pub a: usize,
    pub b: usize,
    pub basis: Vec<EvidenceBasis>,
}

/// 逐份投标总价（含来源标签，报告直接引用）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceEntry {
    pub doc_index: usize,
    pub total: f64,
    pub source: String,
    pub source_label: String,
}

/// 一个候选嫌疑组的反事实结果（【描述性，不进分级】）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupOutcome {
    /// 组内文档位次（升序）。
    pub docs: Vec<usize>,
    /// 组的构造依据（组内各对证据去重合并）——报告必须标明，防循环论证观感。
    pub basis: Vec<EvidenceBasis>,
    /// 中标人翻转的格点占比 ∈[0,1]（在系数区间上积分）。
    pub flip_prob: f64,
    pub flipped_points: usize,
    /// 剔除该组后基准价相对全量基准价的偏移（%；正 = 剔除后基准价更高）。
    pub benchmark_shift_pct: f64,
    /// |偏移| 在「同规模子集穷举」中的分位 ∈[0,1]（n≤5 时为精确穷举，蒙特卡洛的退化精确版）。
    pub shift_percentile: f64,
    /// 参与分位比较的同规模子集数（含本组）。
    pub subsets_compared: usize,
    /// 系数区间中点处的中标人（全量口径）。
    pub winner_full: usize,
    /// 系数区间中点处、剔除本组后的中标人。
    pub winner_excluded: usize,
    /// 组内被标记为断崖式报价（support-bid 形态）的文档位次。
    pub support_bid_docs: Vec<usize>,
}

/// 一条 support-bid（断崖式极端报价）形态标记。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportBid {
    pub doc_index: usize,
    pub total: f64,
    /// "lowest" | "highest"：位于报价分布的哪一端。
    pub position: String,
    /// 与次邻报价的间距。
    pub gap: f64,
    /// 全体相邻报价间距的中位数（断崖判定的基准）。
    pub median_gap: f64,
    /// 相对全体报价中位数的偏离（%）。
    pub deviation_pct: f64,
}

/// 均值基准价一族的反事实块（method=lowest 时【整块缺席】，序列化不出任何均值基准字段）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkBlock {
    pub trim_lowest: usize,
    pub trim_highest: usize,
    pub coeff_min: f64,
    pub coeff_max: f64,
    pub grid_points: usize,
    /// 系数区间中点处的基准价（展示锚点；逐格点结果见各组 flipProb）。
    pub benchmark_mid: f64,
    pub coeff_mid: f64,
    /// 中点处的中标人（全量口径）。
    pub winner_mid: usize,
    /// 候选嫌疑组的反事实结果（按 flipProb 降序）。无证据组时为空数组。
    pub groups: Vec<GroupOutcome>,
}

/// 最低评标价法的「最低价孤立度」描述（禁用均值类统计）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LowestBlock {
    pub winner: usize,
    pub lowest: f64,
    pub second_lowest: f64,
    pub gap: f64,
    pub median_gap: f64,
    /// 最低价与次低价之间是否断崖（gap > 2× 中位间距）。
    pub isolated: bool,
}

/// 机制感知筛查结果（写入 numeric_json.mechanism，【仅供展示】）。
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanismResult {
    /// false = 公式不匹配 / 数据不足 → 只出 notApplicableReason，【不硬算任何数字】。
    pub applicable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_applicable_reason: Option<String>,
    pub method: String,
    /// 公式全文（人工录入回显，报告与配置快照逐字可核对）。
    pub formula: String,
    /// 逐份投标总价与来源打标。
    pub prices: Vec<PriceEntry>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<BenchmarkBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowest: Option<LowestBlock>,
    /// 断崖式报价（support-bid 形态）标记。
    pub support_bids: Vec<SupportBid>,
    /// §1.5 强制措辞（性质声明 / 组构造依据 / 总价来源 / 反事实口径 / 断崖口径）。
    pub notes: Vec<String>,
}

/// 四舍五入到小数点后 n 位（输出规整；同输入本就同输出，取整只为报告可读）。
fn round_to(v: f64, n: i32) -> f64 {
    let s = 10f64.powi(n);
    (v * s).round() / s
}

fn cmp_f64(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(std::cmp::Ordering::Equal)
}

/// 截尾算术平均：升序排序后去掉 trim_low 个最低、trim_high 个最高，剩余求算术平均。
/// 剩余为空 → None。求和顺序固定（升序）保证浮点结果可复现。
fn trimmed_mean(sorted_asc: &[f64], trim_low: usize, trim_high: usize) -> Option<f64> {
    if trim_low + trim_high >= sorted_asc.len() {
        return None;
    }
    let kept = &sorted_asc[trim_low..sorted_asc.len() - trim_high];
    if kept.is_empty() {
        return None;
    }
    let sum: f64 = kept.iter().sum();
    Some(sum / kept.len() as f64)
}

/// 基准价 = 截尾均值 × 系数。
fn benchmark_of(prices: &[BidPrice], trim_low: usize, trim_high: usize, coeff: f64) -> Option<f64> {
    let mut vals: Vec<f64> = prices.iter().map(|p| p.total).collect();
    vals.sort_by(|a, b| cmp_f64(*a, *b));
    trimmed_mean(&vals, trim_low, trim_high).map(|m| m * coeff)
}

/// 中标人 = 报价最接近基准价者。并列时取报价更低者，再并列取位次更小者（确定性）。
fn winner_of(prices: &[BidPrice], benchmark: f64) -> Option<usize> {
    prices
        .iter()
        .min_by(|x, y| {
            cmp_f64((x.total - benchmark).abs(), (y.total - benchmark).abs())
                .then(cmp_f64(x.total, y.total))
                .then(x.doc.cmp(&y.doc))
        })
        .map(|p| p.doc)
}

/// 系数格点（均匀，含端点）。区间退化为一点时只出一个格点。
fn coeff_grid(min: f64, max: f64) -> Vec<f64> {
    if (max - min).abs() < f64::EPSILON {
        return vec![min];
    }
    let n = COEFF_GRID_POINTS;
    (0..n).map(|i| min + (max - min) * (i as f64) / ((n - 1) as f64)).collect()
}

/// 相邻报价间距的中位数（断崖判定基准）。少于 2 家 → None。
fn median_gap_of(sorted_asc: &[f64]) -> Option<f64> {
    if sorted_asc.len() < 2 {
        return None;
    }
    let mut gaps: Vec<f64> = sorted_asc.windows(2).map(|w| w[1] - w[0]).collect();
    gaps.sort_by(|a, b| cmp_f64(*a, *b));
    let mid = gaps.len() / 2;
    Some(if gaps.len().is_multiple_of(2) { (gaps[mid - 1] + gaps[mid]) / 2.0 } else { gaps[mid] })
}

fn median_of(sorted_asc: &[f64]) -> f64 {
    if sorted_asc.is_empty() {
        return 0.0;
    }
    let mid = sorted_asc.len() / 2;
    if sorted_asc.len().is_multiple_of(2) {
        (sorted_asc[mid - 1] + sorted_asc[mid]) / 2.0
    } else {
        sorted_asc[mid]
    }
}

/// support-bid 形态标记：位于报价分布端点、且与次邻报价的间距 > 2× 中位间距。
/// 【描述性标记】——极端报价也可能源于成本结构差异，见 SUPPORT_BID_NOTE。
pub fn support_bids(prices: &[BidPrice]) -> Vec<SupportBid> {
    if prices.len() < 3 {
        return Vec::new();
    }
    let mut sorted: Vec<&BidPrice> = prices.iter().collect();
    sorted.sort_by(|a, b| cmp_f64(a.total, b.total).then(a.doc.cmp(&b.doc)));
    let vals: Vec<f64> = sorted.iter().map(|p| p.total).collect();
    let Some(median_gap) = median_gap_of(&vals) else {
        return Vec::new();
    };
    let median = median_of(&vals);
    let mut out = Vec::new();
    let last = sorted.len() - 1;
    for (i, position) in [(0usize, "lowest"), (last, "highest")] {
        let gap = if i == 0 { vals[1] - vals[0] } else { vals[last] - vals[last - 1] };
        if gap > 0.0 && gap > CLIFF_GAP_RATIO * median_gap {
            out.push(SupportBid {
                doc_index: sorted[i].doc,
                total: round_to(sorted[i].total, 2),
                position: position.to_string(),
                gap: round_to(gap, 2),
                median_gap: round_to(median_gap, 2),
                deviation_pct: if median.abs() > f64::EPSILON {
                    round_to((sorted[i].total - median) / median * 100.0, 4)
                } else {
                    0.0
                },
            });
        }
    }
    out.sort_by_key(|s| s.doc_index);
    out
}

/// 候选嫌疑组枚举（C&D 第一种候选组构造法）：|g|∈{2,3}，组内【每一对】都须有既有文档证据。
/// 只在有投标总价的文档间枚举（剔除后要能重算基准价）。输出按规模、字典序确定性排列。
pub fn candidate_groups(prices: &[BidPrice], evidence: &[PairEvidence]) -> Vec<Vec<usize>> {
    let mut docs: Vec<usize> = prices.iter().map(|p| p.doc).collect();
    docs.sort_unstable();
    docs.dedup();
    let has_evidence = |a: usize, b: usize| -> bool {
        let (lo, hi) = (a.min(b), a.max(b));
        evidence
            .iter()
            .any(|e| e.a.min(e.b) == lo && e.a.max(e.b) == hi && !e.basis.is_empty())
    };
    let mut out: Vec<Vec<usize>> = Vec::new();
    for size in GROUP_MIN_SIZE..=GROUP_MAX_SIZE {
        for combo in combinations(&docs, size) {
            let clique = combo.iter().enumerate().all(|(i, &x)| {
                combo[i + 1..].iter().all(|&y| has_evidence(x, y))
            });
            if clique {
                out.push(combo);
            }
        }
    }
    out
}

/// 定长组合枚举（字典序，确定性）。
fn combinations(items: &[usize], size: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    if size == 0 || size > items.len() {
        return out;
    }
    let mut idx: Vec<usize> = (0..size).collect();
    loop {
        out.push(idx.iter().map(|&i| items[i]).collect());
        let mut i = size;
        loop {
            if i == 0 {
                return out;
            }
            i -= 1;
            if idx[i] != i + items.len() - size {
                break;
            }
            if i == 0 {
                return out;
            }
        }
        idx[i] += 1;
        for j in i + 1..size {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

/// 组的构造依据（组内各对证据合并去重，顺序确定）。
fn basis_of(group: &[usize], evidence: &[PairEvidence]) -> Vec<EvidenceBasis> {
    let mut out: Vec<EvidenceBasis> = Vec::new();
    for (i, &a) in group.iter().enumerate() {
        for &b in &group[i + 1..] {
            let (lo, hi) = (a.min(b), a.max(b));
            for e in evidence.iter().filter(|e| e.a.min(e.b) == lo && e.a.max(e.b) == hi) {
                for basis in &e.basis {
                    if !out.contains(basis) {
                        out.push(basis.clone());
                    }
                }
            }
        }
    }
    out
}

/// 剔除某组后的基准价相对偏移（%）。系数在比值中约去，故与格点无关。
fn shift_pct_of(prices: &[BidPrice], group: &[usize], trim_low: usize, trim_high: usize) -> Option<f64> {
    let full = benchmark_of(prices, trim_low, trim_high, 1.0)?;
    let remaining: Vec<BidPrice> =
        prices.iter().filter(|p| !group.contains(&p.doc)).cloned().collect();
    let excl = benchmark_of(&remaining, trim_low, trim_high, 1.0)?;
    if full.abs() < f64::EPSILON {
        return None;
    }
    Some((excl - full) / full * 100.0)
}

/// 机制感知筛查主入口。cfg 不匹配 v1 公式族 / 数据不足 → applicable=false + 原因，
/// 【不硬算】。prices 需已按文档位次升序（compare_service 保证）。
pub fn run(cfg: &EvaluationConfig, prices: &[BidPrice], evidence: &[PairEvidence]) -> MechanismResult {
    let notes = vec![
        MECHANISM_NOTE.to_string(),
        PRICE_SOURCE_NOTE.to_string(),
        GROUP_BASIS_NOTE.to_string(),
        COUNTERFACTUAL_NOTE.to_string(),
        SUPPORT_BID_NOTE.to_string(),
    ];
    let price_entries: Vec<PriceEntry> = prices
        .iter()
        .map(|p| PriceEntry {
            doc_index: p.doc,
            total: round_to(p.total, 2),
            source: p.source.as_str().to_string(),
            source_label: p.source.label().to_string(),
        })
        .collect();
    let mut result = MechanismResult {
        applicable: false,
        not_applicable_reason: None,
        method: cfg.method.clone(),
        formula: cfg.formula_text(),
        prices: price_entries,
        benchmark: None,
        lowest: None,
        support_bids: support_bids(prices),
        notes,
    };
    if let Err(reason) = cfg.validate() {
        result.not_applicable_reason = Some(format!("{reason}：本节不适用，未作任何反事实计算。"));
        return result;
    }
    if prices.iter().any(|p| !p.total.is_finite() || p.total <= 0.0) {
        result.not_applicable_reason =
            Some("存在无法解析或非正的投标总价：本节不适用，未作任何反事实计算。".into());
        return result;
    }

    if cfg.method == METHOD_LOWEST {
        // 最低评标价法：只做最低价孤立度，【禁用均值类统计】（benchmark 块整块缺席）。
        if prices.len() < 2 {
            result.not_applicable_reason =
                Some("可取得投标总价的文档不足 2 家：本节不适用。".into());
            return result;
        }
        let mut sorted: Vec<&BidPrice> = prices.iter().collect();
        sorted.sort_by(|a, b| cmp_f64(a.total, b.total).then(a.doc.cmp(&b.doc)));
        let vals: Vec<f64> = sorted.iter().map(|p| p.total).collect();
        let median_gap = median_gap_of(&vals).unwrap_or(0.0);
        let gap = vals[1] - vals[0];
        result.applicable = true;
        result.lowest = Some(LowestBlock {
            winner: sorted[0].doc,
            lowest: round_to(vals[0], 2),
            second_lowest: round_to(vals[1], 2),
            gap: round_to(gap, 2),
            median_gap: round_to(median_gap, 2),
            isolated: gap > 0.0 && gap > CLIFF_GAP_RATIO * median_gap,
        });
        return result;
    }

    // —— 截尾均值 × 系数一族 ——
    if prices.len() < MIN_BIDS_FOR_BENCHMARK {
        result.not_applicable_reason = Some(format!(
            "可取得投标总价的文档不足 {MIN_BIDS_FOR_BENCHMARK} 家：均值基准价无从计算，本节不适用。"
        ));
        return result;
    }
    if cfg.trim_lowest + cfg.trim_highest >= prices.len() {
        result.not_applicable_reason = Some(format!(
            "去高（{}）与去低（{}）之和不小于有效报价家数（{}）：本节不适用。",
            cfg.trim_highest,
            cfg.trim_lowest,
            prices.len()
        ));
        return result;
    }
    let grid = coeff_grid(cfg.coeff_min, cfg.coeff_max);
    let coeff_mid = grid[grid.len() / 2];
    let (Some(bm_mid), Some(winner_mid)) = (
        benchmark_of(prices, cfg.trim_lowest, cfg.trim_highest, coeff_mid),
        benchmark_of(prices, cfg.trim_lowest, cfg.trim_highest, coeff_mid)
            .and_then(|b| winner_of(prices, b)),
    ) else {
        result.not_applicable_reason = Some("基准价无从计算（截尾后无剩余报价）：本节不适用。".into());
        return result;
    };

    let groups_raw = candidate_groups(prices, evidence);
    // 同规模子集穷举（n≤5 时 ≤10 个子集）：给「基准价偏移」一个分位参照，
    // 免得读者把「剔除 2 家后基准价动了 3%」当成异常——随便剔 2 家也会动那么多。
    let mut outcomes: Vec<GroupOutcome> = Vec::new();
    let all_docs: Vec<usize> = prices.iter().map(|p| p.doc).collect();
    for group in groups_raw {
        let remaining: Vec<BidPrice> =
            prices.iter().filter(|p| !group.contains(&p.doc)).cloned().collect();
        if remaining.len() <= cfg.trim_lowest + cfg.trim_highest || remaining.is_empty() {
            continue; // 剔除后无从重算基准价 —— 不硬算、不出结论
        }
        let mut flipped = 0usize;
        let mut winner_excluded = winner_mid;
        for (gi, &c) in grid.iter().enumerate() {
            let (Some(bf), Some(be)) = (
                benchmark_of(prices, cfg.trim_lowest, cfg.trim_highest, c),
                benchmark_of(&remaining, cfg.trim_lowest, cfg.trim_highest, c),
            ) else {
                continue;
            };
            let (Some(wf), Some(we)) = (winner_of(prices, bf), winner_of(&remaining, be)) else {
                continue;
            };
            if wf != we {
                flipped += 1;
            }
            if gi == grid.len() / 2 {
                winner_excluded = we;
            }
        }
        let shift = shift_pct_of(prices, &group, cfg.trim_lowest, cfg.trim_highest).unwrap_or(0.0);
        // 分位：同规模子集里，|偏移| 不超过本组的占比（含本组，故恒 >0）。
        let peers: Vec<Vec<usize>> = combinations(&all_docs, group.len());
        let mut compared = 0usize;
        let mut not_greater = 0usize;
        for peer in &peers {
            let Some(s) = shift_pct_of(prices, peer, cfg.trim_lowest, cfg.trim_highest) else {
                continue;
            };
            compared += 1;
            if s.abs() <= shift.abs() + 1e-9 {
                not_greater += 1;
            }
        }
        let support_in_group: Vec<usize> = result
            .support_bids
            .iter()
            .filter(|s| group.contains(&s.doc_index))
            .map(|s| s.doc_index)
            .collect();
        outcomes.push(GroupOutcome {
            basis: basis_of(&group, evidence),
            flip_prob: round_to(flipped as f64 / grid.len() as f64, 6),
            flipped_points: flipped,
            benchmark_shift_pct: round_to(shift, 4),
            shift_percentile: if compared == 0 {
                0.0
            } else {
                round_to(not_greater as f64 / compared as f64, 6)
            },
            subsets_compared: compared,
            winner_full: winner_mid,
            winner_excluded,
            support_bid_docs: support_in_group,
            docs: group,
        });
    }
    // 排序：翻转比例降序 → |偏移| 降序 → 位次升序（确定性）。
    outcomes.sort_by(|x, y| {
        cmp_f64(y.flip_prob, x.flip_prob)
            .then(cmp_f64(y.benchmark_shift_pct.abs(), x.benchmark_shift_pct.abs()))
            .then(x.docs.cmp(&y.docs))
    });

    result.applicable = true;
    result.benchmark = Some(BenchmarkBlock {
        trim_lowest: cfg.trim_lowest,
        trim_highest: cfg.trim_highest,
        coeff_min: cfg.coeff_min,
        coeff_max: cfg.coeff_max,
        grid_points: grid.len(),
        benchmark_mid: round_to(bm_mid, 4),
        coeff_mid: round_to(coeff_mid, 6),
        winner_mid,
        groups: outcomes,
    });
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn price(doc: usize, total: f64) -> BidPrice {
        BidPrice { doc, total, source: PriceSource::BoqSum }
    }

    fn ev(a: usize, b: usize) -> PairEvidence {
        PairEvidence {
            a,
            b,
            basis: vec![EvidenceBasis {
                kind: "textPeak".into(),
                detail: "文本相似峰值 0.72（≥0.60）".into(),
            }],
        }
    }

    #[test]
    fn trimmed_mean_benchmark_and_winner_match_hand_calculation() {
        // 验收①（手算对拍）：5 价 [100,98,96,95,60]、去 1 高 1 低、c=0.95
        // → 基准价 = mean(98,96,95)×0.95，最接近者（95）判中标。
        let prices = vec![price(0, 100.0), price(1, 98.0), price(2, 96.0), price(3, 95.0), price(4, 60.0)];
        let bm = benchmark_of(&prices, 1, 1, 0.95).unwrap();
        let expect = (98.0 + 96.0 + 95.0) / 3.0 * 0.95;
        assert!((bm - expect).abs() < 1e-12, "基准价 {bm} 应为 {expect}");
        assert_eq!(winner_of(&prices, bm), Some(3), "95 最接近基准价 {bm}");
    }

    #[test]
    fn support_bid_marks_cliff_extreme_price() {
        // 验收⑤：[100,98,96,95,60] 排序后间距 35/1/2/2，中位间距 2 → 60 断崖（35 > 2×2）。
        let prices = vec![price(0, 100.0), price(1, 98.0), price(2, 96.0), price(3, 95.0), price(4, 60.0)];
        let marks = support_bids(&prices);
        assert_eq!(marks.len(), 1, "只有 60 断崖：{marks:?}");
        assert_eq!(marks[0].doc_index, 4);
        assert_eq!(marks[0].position, "lowest");
        assert_eq!(marks[0].gap, 35.0);
        assert_eq!(marks[0].median_gap, 2.0);
        // 均匀分布（间距全 1）→ 无断崖
        let flat = vec![price(0, 100.0), price(1, 101.0), price(2, 102.0), price(3, 103.0)];
        assert!(support_bids(&flat).is_empty());
    }

    #[test]
    fn flip_prob_is_one_when_excluding_group_changes_winner_at_every_grid_point() {
        // 验收②：陪标对（丁 95 + 戊 60）把基准价拉低到 95 中标；剔除该组后剩 [100,98,96]，
        // 基准价升至 98c，中标人在【全部格点】改变 → flipProb=1.0。
        let prices = vec![price(0, 100.0), price(1, 98.0), price(2, 96.0), price(3, 95.0), price(4, 60.0)];
        let cfg = EvaluationConfig {
            method: METHOD_AVG_BENCHMARK.into(),
            trim_lowest: 0,
            trim_highest: 0,
            coeff_min: 0.9,
            coeff_max: 1.0,
        };
        let evidence = vec![ev(3, 4)];
        let r = run(&cfg, &prices, &evidence);
        assert!(r.applicable, "{:?}", r.not_applicable_reason);
        let b = r.benchmark.as_ref().unwrap();
        assert_eq!(b.grid_points, COEFF_GRID_POINTS, "格点而非随机采样");
        assert_eq!(b.groups.len(), 1, "只有丁×戊 有证据：{:?}", b.groups);
        let g = &b.groups[0];
        assert_eq!(g.docs, vec![3, 4]);
        assert_eq!(g.flip_prob, 1.0, "全部格点翻转：flipped={}", g.flipped_points);
        assert_eq!(g.flipped_points, COEFF_GRID_POINTS);
        assert!(g.benchmark_shift_pct > 0.0, "剔除低价组 → 基准价上移");
        assert_eq!(g.support_bid_docs, vec![4], "组内 60 是断崖式报价");
        assert!(!g.basis.is_empty(), "组的构造依据必须随组输出（防循环论证观感）");
        assert!(g.shift_percentile > 0.0 && g.shift_percentile <= 1.0);
        assert!(g.subsets_compared >= 2, "同规模子集穷举应有比较对象");
    }

    #[test]
    fn groups_are_empty_without_document_evidence() {
        // 验收③：无证据组 → groups 为空（嫌疑组必须由既有文档证据圈定）。
        let prices = vec![price(0, 100.0), price(1, 98.0), price(2, 96.0), price(3, 95.0)];
        let cfg = EvaluationConfig::default();
        let r = run(&cfg, &prices, &[]);
        assert!(r.applicable);
        assert!(r.benchmark.as_ref().unwrap().groups.is_empty());
        // 单侧有证据的三元组不成组（组内每对都需证据）
        let r2 = run(&cfg, &prices, &[ev(0, 1)]);
        let gs = &r2.benchmark.as_ref().unwrap().groups;
        assert_eq!(gs.len(), 1, "只有 0×1 成组：{gs:?}");
        assert_eq!(gs[0].docs, vec![0, 1]);
    }

    #[test]
    fn lowest_method_emits_no_mean_benchmark_fields() {
        // 验收④：method=lowest → 输出不含任何均值基准字段（benchmark 块整块缺席）。
        let prices = vec![price(0, 100.0), price(1, 99.0), price(2, 60.0), price(3, 98.0)];
        let cfg = EvaluationConfig { method: METHOD_LOWEST.into(), ..Default::default() };
        let r = run(&cfg, &prices, &[ev(0, 1)]);
        assert!(r.applicable);
        assert!(r.benchmark.is_none());
        let js = serde_json::to_string(&r).unwrap();
        assert!(!js.contains("benchmark"), "lowest 分支不得出现均值基准字段：{js}");
        assert!(!js.contains("trimLowest"));
        assert!(!js.contains("groups"));
        let lo = r.lowest.unwrap();
        assert_eq!(lo.winner, 2);
        assert!(lo.isolated, "60 与次低 98 断崖（间距 38 > 2× 中位间距 1）");
    }

    #[test]
    fn unsupported_formula_and_bad_params_report_not_applicable_without_computing() {
        // 【公式不匹配时明确输出「不适用」而非硬算】。
        let prices = vec![price(0, 100.0), price(1, 98.0), price(2, 96.0)];
        let odd = EvaluationConfig { method: "two_stage_avg".into(), ..Default::default() };
        let r = run(&odd, &prices, &[]);
        assert!(!r.applicable);
        assert!(r.benchmark.is_none() && r.lowest.is_none());
        assert!(r.not_applicable_reason.unwrap().contains("不属 v1 支持的公式族"));

        let bad_coeff = EvaluationConfig { coeff_min: 1.2, coeff_max: 0.9, ..Default::default() };
        assert!(!run(&bad_coeff, &prices, &[]).applicable);

        let bad_trim = EvaluationConfig { trim_lowest: 2, trim_highest: 1, ..Default::default() };
        let r = run(&bad_trim, &prices, &[]);
        assert!(!r.applicable);
        assert!(r.not_applicable_reason.unwrap().contains("不小于有效报价家数"));

        // 家数不足
        let two = vec![price(0, 100.0), price(1, 98.0)];
        let r = run(&EvaluationConfig::default(), &two, &[]);
        assert!(!r.applicable);
        assert!(r.not_applicable_reason.unwrap().contains("不足 3 家"));
    }

    #[test]
    fn output_is_byte_identical_across_runs() {
        // 验收⑥：同输入两次运行逐字节一致（格点、排序、求和顺序全部确定性）。
        let prices = vec![
            BidPrice { doc: 0, total: 1_000_000.0, source: PriceSource::TotalRow },
            BidPrice { doc: 1, total: 980_000.0, source: PriceSource::BoqSum },
            BidPrice { doc: 2, total: 960_500.5, source: PriceSource::Heuristic },
            BidPrice { doc: 3, total: 951_000.0, source: PriceSource::BoqSum },
            BidPrice { doc: 4, total: 600_000.0, source: PriceSource::BoqSum },
        ];
        let evidence = vec![ev(3, 4), ev(0, 1), ev(0, 3), ev(1, 3)];
        let cfg = EvaluationConfig {
            method: METHOD_AVG_BENCHMARK.into(),
            trim_lowest: 1,
            trim_highest: 0,
            coeff_min: 0.88,
            coeff_max: 0.98,
        };
        let a = serde_json::to_string(&run(&cfg, &prices, &evidence)).unwrap();
        let b = serde_json::to_string(&run(&cfg, &prices, &evidence)).unwrap();
        assert_eq!(a, b);
        // 组按 flipProb 降序排列且三元组也在候选内（0×1×3 每对都有证据）
        let r = run(&cfg, &prices, &evidence);
        let gs = &r.benchmark.as_ref().unwrap().groups;
        assert!(gs.iter().any(|g| g.docs == vec![0, 1, 3]), "三元组应成组：{gs:?}");
        // 剔除后剩余家数不足以重算基准价的组【不硬算】：去 1 高 1 低时三元组整体退出
        let tight = EvaluationConfig { trim_highest: 1, ..cfg.clone() };
        let r2 = run(&tight, &prices, &evidence);
        assert!(
            r2.benchmark.as_ref().unwrap().groups.iter().all(|g| g.docs.len() == 2),
            "剔除 3 家后仅剩 2 家、去高去低后无剩余 → 该组不出结论"
        );
        for w in gs.windows(2) {
            assert!(w[0].flip_prob >= w[1].flip_prob, "组按翻转比例降序");
        }
    }

    #[test]
    fn notes_carry_the_mandatory_wording() {
        // §1.5：性质声明（不参与围标分级 + 人工录入需核对）必须随数据下发。
        let r = run(&EvaluationConfig::default(), &[price(0, 100.0)], &[]);
        assert!(r.notes.iter().any(|n| n.contains("不参与围标分级")));
        assert!(r.notes.iter().any(|n| n.contains("评标办法为人工录入")));
        assert!(r.notes.iter().any(|n| n.contains("构造依据")));
        assert!(r.formula.contains("算术平均"), "公式全文须回显：{}", r.formula);
    }
}
