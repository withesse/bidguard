// 围标综合判定：把文本相似度、跨文档雷同条款、元数据同源、共有特征词、
// 报价梯度（金额接近但条款雷同）、rsid 修订标识交集、PDF 血缘加权成一个结论。
use crate::engine::fingerprint::{LineageHit, RsidHit, RSID_MIN_SHARED};
use crate::engine::report::{Cluster, Collusion, CollusionSignal, DocInfo, SharedTerm};
use std::collections::HashSet;

/// 文档天干代号（信号 detail 中指代具体文档对）。
const STEMS: [&str; 10] = ["甲", "乙", "丙", "丁", "戊", "己", "庚", "辛", "壬", "癸"];
fn stem(i: usize) -> &'static str {
    STEMS.get(i).copied().unwrap_or("?")
}

/// 报价梯度信号：两文档报价差距很小（围标常见的「陪标价」），且多处条款雷同。
pub struct PriceProximity {
    pub a: usize,
    pub b: usize,
    pub amount_a: u64,
    pub amount_b: u64,
    pub gap_pct: f32,
}

/// 参评某文档的一张图片指纹（比对期输入，从 document_images 读出适配）。
pub struct ImageFp {
    pub sha256: String,
    /// None = 整页扫描图，只做 exact 不做 near（防「都是空白页/同制式表格」互撞误报）。
    pub dhash: Option<u64>,
    pub page: Option<u32>,
}

/// 一张图在两文档间的同源命中（a < b 为 docs 下标）。exact=true 为 sha256 相等（硬命中，
/// 跨容器稳定的精确指纹），false 为 dHash 近似命中。page_* 为各自页码（docx 无页码 None）。
pub struct ImageHit {
    pub a: usize,
    pub b: usize,
    pub page_a: Option<u32>,
    pub page_b: Option<u32>,
    pub exact: bool,
}

/// 两两跨文档碰撞图片指纹：sha256 相等（exact）或 dHash 汉明距离 ≤ IMAGE_NEAR_MAX_HAMMING
/// 且双方 dhash 均非 None（near）判命中。每张 a 图匹配到即计一对（去重，避免一图撞多图重复
/// 计数）。exempt_hashes：招标文件统一提供的图片哈希豁免集（M4 招标对减接线；当前调用方恒空），
/// 命中前先剔除——招标方统一下发的效果图/区位图各家照贴属合规雷同，不算串标。
pub fn image_pairs(per_doc: &[Vec<ImageFp>], exempt_hashes: &HashSet<String>) -> Vec<ImageHit> {
    let mut hits: Vec<ImageHit> = Vec::new();
    for a in 0..per_doc.len() {
        for b in (a + 1)..per_doc.len() {
            for ia in per_doc[a].iter().filter(|i| !exempt_hashes.contains(&i.sha256)) {
                for ib in per_doc[b].iter().filter(|i| !exempt_hashes.contains(&i.sha256)) {
                    let exact = ia.sha256 == ib.sha256;
                    let near = !exact
                        && match (ia.dhash, ib.dhash) {
                            (Some(x), Some(y)) => (x ^ y).count_ones() <= IMAGE_NEAR_MAX_HAMMING,
                            _ => false,
                        };
                    if exact || near {
                        hits.push(ImageHit {
                            a,
                            b,
                            page_a: ia.page,
                            page_b: ib.page,
                            exact,
                        });
                        break; // 同一张 a 图记一对即止，移到下一张 a 图
                    }
                }
            }
        }
    }
    hits
}

/// 同源图片位置文案：PDF 用页码，docx 内嵌图无页码。
fn img_loc(page: Option<u32>) -> String {
    match page {
        Some(p) => format!("第{p}页"),
        None => "内嵌图".to_string(),
    }
}

// —— 围标判定权重与分级线（集中于此，便于校准） ——
// ⚠️ 未经实证校准：以下均为基于评标经验的初始默认值，尚无带标注的真实案例语料回测。
// 校准方法见 docs/architecture-analysis-v0.4.md(S1-1)：收集有标注(真围标/非围标)的历史比对
// 语料，用正负向样本回测这些权重与三条分级线后再固化。改这里即可整体调参。
const SIM_FLOOR: f32 = 0.6; // 相似度峰值起算线
const SIM_WEIGHT: f32 = 0.4; // 相似度信号满权重
const CLUSTER_MULTI_DOCS: usize = 3; // ≥N 份共现算强雷同
const CLUSTER_BASE: f32 = 0.1; // 有雷同条款的基础权重
const CLUSTER_SCALE: f32 = 0.3; // 强雷同随数量增长的权重
const CLUSTER_SCALE_CAP: f32 = 5.0; // multi/CAP 封顶到 1
const META_MIN_DOCS: usize = 2; // ≥N 份元数据同源才计
const META_WEIGHT: f32 = 0.25;
const SHARED_TERMS_MIN: usize = 5; // ≥N 个共有特征词才计
const SHARED_TERMS_WEIGHT: f32 = 0.1;
// 共同错误指纹信号（M1 取证，连续特征）：贡献 = SHARED_ERRORS_WEIGHT × x，
// x = min(Σ稀有度 / SHARED_ERRORS_SATURATION, 1)。词典外词/异常标点/错误引用的跨文档共现，
// 共用同一处罕见错误比共用正确词证明力高一个量级（调研 §5/§13：identical wrong answers）；
// 稀有度加权避免高频「新词/术语」误报，措辞「疑似错误」不直接定性。
const SHARED_ERRORS_WEIGHT: f32 = 0.25;
const SHARED_ERRORS_SATURATION: f32 = 5.0; // 加权错误数达 5 即满档
const SHARED_ERRORS_SHOW_MAX: usize = 3; // detail 最多列出几条疑似错误
const PRICE_WEIGHT: f32 = 0.15;
const PRICE_SHOW_MAX: usize = 3; // 报价梯度对最多列出几对
// rsid 信号（M1 取证，连续特征）：贡献 = RSID_WEIGHT × x，
// x = root_match ? 1.0 : min(shared_count / RSID_SHARED_SATURATION, 1)；多对取最强不叠加
const RSID_WEIGHT: f32 = 0.35;
const RSID_SHARED_SATURATION: f32 = 10.0; // 共享数达 10 即满档
const RSID_SHOW_MAX: usize = 3; // rsid 命中对最多列出几对
// PDF 血缘信号（M1 取证，连续特征）：贡献 = PDF_LINEAGE_WEIGHT × x，
// x = 硬命中(同一母文件 GUID/trailer ID) ? 1.0 : PDF_LINEAGE_MID_X(仅共享字体子集标签)；
// 多对取最强不叠加。中档比例对应原设计 0.20/0.35 两档（审查修正为连续特征）。
const PDF_LINEAGE_WEIGHT: f32 = 0.35;
const PDF_LINEAGE_MID_X: f32 = 0.55;
const PDF_LINEAGE_SHOW_MAX: usize = 3; // 血缘命中对最多列出几对
// 内嵌图片同源信号（M1 取证，连续特征）：贡献 = IMAGE_REUSE_WEIGHT × x，
// x = min(命中图对数 / IMAGE_REUSE_SATURATION, 1)。命中图对数是跨文档两两碰撞去重后的
// 匹配图对数——共用同一张现场照/资质扫描/公章完全绕开文本比对，是高证明力围标信号。
const IMAGE_REUSE_WEIGHT: f32 = 0.25;
const IMAGE_REUSE_SATURATION: f32 = 3.0; // 3 对命中即满档
const IMAGE_REUSE_SHOW_MAX: usize = 3; // detail 最多列出几组同源图
/// 近似命中的 dHash 汉明距离上限：≤ 此值视为视觉同源（拍板值，未经校准）。
const IMAGE_NEAR_MAX_HAMMING: u32 = 10;
/// metadata 信号只认这些强命中类别的 risk_flags 前缀——rsid 交集/PDF 血缘有独立信号
/// （防双计），「修订号相同（弱）/疑似元数据清洗」是弱标记只供人工核对（不计权）。
/// 「生成环境一致」是 PDF 血缘的弱命中档：不足以独立成信号，并入 metadata 计权。
const META_FLAG_CATEGORIES: [&str; 6] =
    ["作者相同", "最后保存者相同", "模板相同", "创建时间邻近", "包结构一致", "生成环境一致"];
const LEVEL_HIGH: f32 = 0.6;
const LEVEL_MEDIUM: f32 = 0.35;
const LEVEL_LOW: f32 = 0.1; // score > LEVEL_LOW → low
/// 取证类信号（rsid / PDF 血缘 / 内嵌图片同源 / 共同错误指纹）对总分的合计封顶：
/// 四类各自满档相加可达 1.20，不封顶则任意两三类叠满即直接 high，越过「单点定案需人工核实」
/// 的产品边界。此处只封顶四类对 score 的合计贡献（各信号 detail 仍呈现原始权重供人工判断），
/// 条件化 floor 规则本里程碑不启用（M4 招标豁免落地后再激活）。⚠️ 0.45 未经语料校准。
const FORENSIC_CAP: f32 = 0.45;

/// assess_with 的统一入参（执行方案 §1.2 裁决：签名只改这一次）。
/// 后续里程碑将按「连续特征 x∈[0,1]」往此结构体追加字段组——M1 取证 forensic、
/// M2 规避 evasion、M6 数值 numeric——各工作流只加字段、不再改 assess_with 签名。
/// 借用而非持有：调用方（compare_service）在比对结束时已持有全部聚合结果，
/// 判定为一次性读取，无需克隆。派生 Default（空切片 + 0.0）供单测按
/// `..Default::default()` 构造，新增字段组不再触发既有测试适配。
#[derive(Default)]
pub struct CollusionInputs<'a> {
    /// 文档级相似度峰值（doc_matrix 输出的 peak）
    pub peak: f32,
    /// 跨文档雷同条款簇（已适配为 report::Cluster 形态）
    pub clusters: &'a [Cluster],
    /// 各文档信息（fingerprint.risk_flags 需已经过 cross_flags 标记）
    pub docs: &'a [DocInfo],
    /// 多份标书共有的罕见特征词
    pub shared_terms: &'a [SharedTerm],
    /// 报价梯度接近对（金额接近但条款雷同的「陪标价」候选）
    pub price_pairs: &'a [PriceProximity],
    /// —— M1 取证：rsid 修订标识交集命中对（fingerprint::rsid_pairs 输出，
    /// 已按「共享 ≥3 或 rsidRoot 相同」过滤；豁免减法在 rsid_pairs 侧完成）——
    pub rsid_hits: &'a [RsidHit],
    /// —— M1 取证：PDF 血缘命中对（fingerprint::lineage_pairs 输出：硬=同一母文件
    /// GUID/trailer ID、中=共享字体子集标签；弱命中不在此——已并入 metadata 风险标记）——
    pub lineage_hits: &'a [LineageHit],
    /// —— M1 取证：内嵌图片同源命中对（collusion::image_pairs 输出：sha256 精确/dHash 近似；
    /// 招标文件图片豁免在 image_pairs 侧完成）——
    pub image_hits: &'a [ImageHit],
    /// —— M1 取证：共同错误指纹（compare_service::shared_error_fingerprints 输出，
    /// kind="sharedErrors"、rarity 稀有度归一分、context 前后文；招标文件笔误豁免在检测侧完成）——
    pub shared_errors: &'a [SharedTerm],
}

pub fn assess_with(inputs: CollusionInputs) -> Collusion {
    let CollusionInputs {
        peak,
        clusters,
        docs,
        shared_terms,
        price_pairs,
        rsid_hits,
        lineage_hits,
        image_hits,
        shared_errors,
    } = inputs;
    let mut signals = Vec::new();
    let mut score = 0.0f32;
    // 取证四类（rsid / PDF 血缘 / 内嵌图片 / 共同错误）各自贡献先累加到此，最后按
    // FORENSIC_CAP 封顶后并入 score；各信号 detail 仍带原始权重（呈现证明力，不受封顶影响）。
    let mut forensic = 0.0f32;

    // 1) 文本相似度峰值（SIM_FLOOR→0，1.0→满分 SIM_WEIGHT）
    if peak >= SIM_FLOOR {
        let w = SIM_WEIGHT * ((peak - SIM_FLOOR) / (1.0 - SIM_FLOOR)).clamp(0.0, 1.0);
        score += w;
        signals.push(CollusionSignal {
            kind: "similarity".into(),
            detail: format!("两份标书整体相似度峰值 {:.0}%", peak * 100.0),
            weight: w,
        });
    }

    // 2) 跨文档雷同条款（3 份及以上的聚类是强信号）
    let multi = clusters.iter().filter(|c| c.docs.len() >= CLUSTER_MULTI_DOCS).count();
    if multi > 0 {
        let w = CLUSTER_BASE + CLUSTER_SCALE * (multi as f32 / CLUSTER_SCALE_CAP).clamp(0.0, 1.0);
        score += w;
        signals.push(CollusionSignal {
            kind: "cluster".into(),
            detail: format!("{multi} 处条款在 {CLUSTER_MULTI_DOCS} 份及以上标书间高度雷同"),
            weight: w,
        });
    } else if !clusters.is_empty() {
        score += CLUSTER_BASE;
        signals.push(CollusionSignal {
            kind: "cluster".into(),
            detail: format!("{} 处跨标书雷同条款", clusters.len()),
            weight: CLUSTER_BASE,
        });
    }

    // 3) 元数据同源：只认强命中类别的风险标记（rsid 有独立信号、弱标记不计权），
    //    detail 枚举具体命中项而非笼统一句
    let has_meta_flag = |d: &DocInfo| {
        META_FLAG_CATEGORIES
            .iter()
            .any(|c| d.fingerprint.risk_flags.iter().any(|f| f.starts_with(c)))
    };
    let meta = docs.iter().filter(|d| has_meta_flag(d)).count();
    if meta >= META_MIN_DOCS {
        let cats: Vec<&str> = META_FLAG_CATEGORIES
            .iter()
            .copied()
            .filter(|c| {
                docs.iter()
                    .any(|d| d.fingerprint.risk_flags.iter().any(|f| f.starts_with(c)))
            })
            .collect();
        score += META_WEIGHT;
        signals.push(CollusionSignal {
            kind: "metadata".into(),
            detail: format!(
                "多份文档元数据同源：{}；元数据可被编辑清除，未命中不代表清白",
                cats.join("、")
            ),
            weight: META_WEIGHT,
        });
    }

    // 4) 共有特征词 / 疑似共用笔误
    if shared_terms.len() >= SHARED_TERMS_MIN {
        score += SHARED_TERMS_WEIGHT;
        signals.push(CollusionSignal {
            kind: "sharedTerms".into(),
            detail: format!("{} 个罕见特征词被多份标书共用", shared_terms.len()),
            weight: SHARED_TERMS_WEIGHT,
        });
    }

    // 5) 报价梯度雷同：金额仅差几个百分点 + 多处条款雷同，是典型的围标陪标特征。
    // 多对接近时全部列出（最多 3 对），权重记一次（同一类证据不叠加）。
    if !price_pairs.is_empty() {
        let shown: Vec<String> = price_pairs
            .iter()
            .take(PRICE_SHOW_MAX)
            .map(|p| {
                format!(
                    "「{}」「{}」差 {:.1}%（{} vs {} 元）",
                    stem(p.a),
                    stem(p.b),
                    p.gap_pct * 100.0,
                    p.amount_a,
                    p.amount_b
                )
            })
            .collect();
        let w = PRICE_WEIGHT;
        score += w;
        signals.push(CollusionSignal {
            kind: "facts".into(),
            detail: format!("报价梯度雷同：{}，且相关文档多处条款雷同", shown.join("；")),
            weight: w,
        });
    }

    // 6) rsid 修订标识交集（M1 取证）：连续特征 x = root_match ? 1.0 : min(shared/10, 1)，
    //    多对取最强不叠加（同一类证据记一次）；rsid_pairs 已过滤弱档，此处再防御一次。
    let rsid_valid: Vec<&RsidHit> = rsid_hits
        .iter()
        .filter(|h| h.root_match || h.shared_count >= RSID_MIN_SHARED)
        .collect();
    if !rsid_valid.is_empty() {
        let x_of = |h: &RsidHit| {
            if h.root_match {
                1.0
            } else {
                (h.shared_count as f32 / RSID_SHARED_SATURATION).min(1.0)
            }
        };
        let x = rsid_valid.iter().map(|h| x_of(h)).fold(0.0f32, f32::max);
        let shown: Vec<String> = rsid_valid
            .iter()
            .take(RSID_SHOW_MAX)
            .map(|h| {
                format!(
                    "「{}」「{}」共享 {} 个 rsid{}",
                    stem(h.a),
                    stem(h.b),
                    h.shared_count,
                    if h.root_match { "（rsidRoot 相同，高度指示同一母文件）" } else { "" }
                )
            })
            .collect();
        let w = RSID_WEIGHT * x;
        forensic += w;
        signals.push(CollusionSignal {
            kind: "rsid".into(),
            detail: format!(
                "docx 修订标识（rsid）交集：{}。注意：同一母版可能为招标方提供的统一模板；\
                 rsid 另存为即可清除，未命中不代表清白",
                shown.join("；")
            ),
            weight: w,
        });
    }

    // 7) PDF 血缘（M1 取证）：连续特征 x = 硬命中 ? 1.0 : PDF_LINEAGE_MID_X，
    //    多对取最强不叠加；防御过滤掉两档证据皆空的无效命中。
    let lineage_valid: Vec<&LineageHit> = lineage_hits
        .iter()
        .filter(|h| h.is_hard() || !h.shared_subset_tags.is_empty())
        .collect();
    if !lineage_valid.is_empty() {
        let x = lineage_valid
            .iter()
            .map(|h| if h.is_hard() { 1.0f32 } else { PDF_LINEAGE_MID_X })
            .fold(0.0f32, f32::max);
        let shown: Vec<String> = lineage_valid
            .iter()
            .take(PDF_LINEAGE_SHOW_MAX)
            .map(|h| {
                if h.is_hard() {
                    format!(
                        "「{}」「{}」{}（同一母文件）",
                        stem(h.a),
                        stem(h.b),
                        h.hard_evidence.join("、")
                    )
                } else {
                    format!(
                        "「{}」「{}」共享字体子集标签（同一次生成环境）",
                        stem(h.a),
                        stem(h.b)
                    )
                }
            })
            .collect();
        let w = PDF_LINEAGE_WEIGHT * x;
        forensic += w;
        signals.push(CollusionSignal {
            kind: "pdfLineage".into(),
            detail: format!(
                "PDF 血缘同源：{}。注意：同一母文件亦可能来自招标方统一模板或同一\
                 代理/打印机构，请评标人核实；元数据可被抹除，未命中不代表清白",
                shown.join("；")
            ),
            weight: w,
        });
    }

    // 8) 内嵌图片同源（M1 取证）：连续特征 x = min(命中图对数 / IMAGE_REUSE_SATURATION, 1)。
    //    共用同一张现场照/资质扫描/公章完全绕开文本比对，是高证明力围标信号；
    //    detail 列样例并附「请核对是否来自招标文件」核对提示与「未命中不代表清白」纪律文案。
    if !image_hits.is_empty() {
        let x = (image_hits.len() as f32 / IMAGE_REUSE_SATURATION).min(1.0);
        let shown: Vec<String> = image_hits
            .iter()
            .take(IMAGE_REUSE_SHOW_MAX)
            .map(|h| {
                format!(
                    "「{}」{} ↔ 「{}」{}{}",
                    stem(h.a),
                    img_loc(h.page_a),
                    stem(h.b),
                    img_loc(h.page_b),
                    if h.exact { "" } else { "（近似）" }
                )
            })
            .collect();
        let w = IMAGE_REUSE_WEIGHT * x;
        forensic += w;
        signals.push(CollusionSignal {
            kind: "imageReuse".into(),
            detail: format!(
                "内嵌图片同源：{}。请核对该图是否来自招标文件统一提供\
                 （效果图/区位图各家照贴属合规）；未命中不代表清白",
                shown.join("；")
            ),
            weight: w,
        });
    }

    // 9) 共同错误指纹（M1 取证）：连续特征 x = min(Σ稀有度 / SHARED_ERRORS_SATURATION, 1)。
    //    词典外词/异常标点/错误引用的跨文档共现——共用同一处罕见错误远比共用正确词可疑；
    //    detail 列样例+前后文供人工核对，措辞「疑似错误」不定性，并附招标文件笔误豁免与
    //    「未命中不代表清白」纪律文案。缺 rarity 的旧条目按 1.0 兜底（不静默丢分）。
    if !shared_errors.is_empty() {
        let weighted: f32 = shared_errors
            .iter()
            .map(|t| t.rarity.unwrap_or(1.0).clamp(0.0, 1.0))
            .sum();
        let x = (weighted / SHARED_ERRORS_SATURATION).min(1.0);
        let shown: Vec<String> = shared_errors
            .iter()
            .take(SHARED_ERRORS_SHOW_MAX)
            .map(|t| match t.context.as_deref() {
                Some(ctx) if !ctx.is_empty() => format!("「{}」（{}）", t.term, ctx),
                _ => format!("「{}」", t.term),
            })
            .collect();
        let w = SHARED_ERRORS_WEIGHT * x;
        forensic += w;
        signals.push(CollusionSignal {
            kind: "sharedErrors".into(),
            detail: format!(
                "多份标书疑似共用错误（词典外词/异常标点/错误引用）：{}。\
                 疑似错误仅供人工核对、未必构成串标；招标文件原生笔误各家照抄应予豁免，\
                 未命中不代表清白",
                shown.join("；")
            ),
            weight: w,
        });
    }

    // 取证四类合计封顶后并入总分（防叠满直接 high；各信号 detail 权重不受此影响）
    score += forensic.min(FORENSIC_CAP);
    let score = score.clamp(0.0, 1.0);
    let level = if score >= LEVEL_HIGH {
        "high"
    } else if score >= LEVEL_MEDIUM {
        "medium"
    } else if score > LEVEL_LOW {
        "low"
    } else {
        "none"
    };
    Collusion {
        level: level.into(),
        score,
        signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::report::Fingerprint;

    fn cluster(docs: Vec<usize>) -> Cluster {
        Cluster { avg_score: 0.9, peak: 0.9, docs, segments: vec![] }
    }
    fn doc(flags: Vec<&str>) -> DocInfo {
        DocInfo {
            id: "d".into(),
            name: "n".into(),
            doc_type: "docx".into(),
            pages: 1,
            char_count: 100,
            fingerprint: Fingerprint {
                risk_flags: flags.into_iter().map(String::from).collect(),
                ..Default::default()
            },
            parse_error: None,
        }
    }
    fn weight_of(c: &Collusion, kind: &str) -> Option<f32> {
        c.signals.iter().find(|s| s.kind == kind).map(|s| s.weight)
    }

    #[test]
    fn empty_inputs_score_none() {
        let c = assess_with(CollusionInputs::default());
        assert_eq!(c.level, "none");
        assert_eq!(c.score, 0.0);
        assert!(c.signals.is_empty());
    }

    #[test]
    fn peak_below_floor_yields_no_similarity_signal() {
        let c = assess_with(CollusionInputs { peak: SIM_FLOOR - 0.01, ..Default::default() });
        assert!(weight_of(&c, "similarity").is_none());
        assert_eq!(c.level, "none");
    }

    #[test]
    fn full_peak_similarity_weight_is_max_and_medium() {
        let c = assess_with(CollusionInputs { peak: 1.0, ..Default::default() });
        let w = weight_of(&c, "similarity").expect("应有相似度信号");
        assert!((w - SIM_WEIGHT).abs() < 1e-6, "满峰值权重应为 SIM_WEIGHT");
        assert_eq!(c.level, "medium"); // 0.40 ≥ LEVEL_MEDIUM(0.35)
    }

    #[test]
    fn single_multi_doc_cluster_triggers_low() {
        // 一个横跨 3 份文档的雷同条款：w = 0.1 + 0.3*(1/5) = 0.16 > LEVEL_LOW(0.1) → low
        let c = assess_with(CollusionInputs {
            clusters: &[cluster(vec![0, 1, 2])],
            ..Default::default()
        });
        let w = weight_of(&c, "cluster").expect("应有 cluster 信号");
        assert!((w - 0.16).abs() < 1e-6);
        assert_eq!(c.level, "low");
    }

    #[test]
    fn two_doc_cluster_only_base_weight() {
        // 仅跨 2 份文档（<CLUSTER_MULTI_DOCS）：走 else 分支，权重 = CLUSTER_BASE
        let c = assess_with(CollusionInputs {
            clusters: &[cluster(vec![0, 1])],
            ..Default::default()
        });
        let w = weight_of(&c, "cluster").expect("应有 cluster 信号");
        assert!((w - CLUSTER_BASE).abs() < 1e-6);
    }

    #[test]
    fn metadata_needs_min_docs() {
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let c = assess_with(CollusionInputs { docs: &two, ..Default::default() });
        assert!((weight_of(&c, "metadata").unwrap() - META_WEIGHT).abs() < 1e-6);
        // 仅 1 份带风险标记 → 不计元数据信号
        let one = vec![doc(vec!["作者相同"]), doc(vec![])];
        let c1 = assess_with(CollusionInputs { docs: &one, ..Default::default() });
        assert!(weight_of(&c1, "metadata").is_none());
    }

    #[test]
    fn shared_terms_threshold() {
        let mk = |n: usize| -> Vec<SharedTerm> {
            (0..n)
                .map(|i| SharedTerm { term: format!("t{i}"), docs: vec![0, 1], ..Default::default() })
                .collect()
        };
        let at = assess_with(CollusionInputs {
            shared_terms: &mk(SHARED_TERMS_MIN),
            ..Default::default()
        });
        assert!((weight_of(&at, "sharedTerms").unwrap() - SHARED_TERMS_WEIGHT).abs() < 1e-6);
        let below = assess_with(CollusionInputs {
            shared_terms: &mk(SHARED_TERMS_MIN - 1),
            ..Default::default()
        });
        assert!(weight_of(&below, "sharedTerms").is_none());
    }

    #[test]
    fn price_proximity_signal() {
        let pp = [PriceProximity { a: 0, b: 1, amount_a: 1_000_000, amount_b: 1_020_000, gap_pct: 0.02 }];
        let c = assess_with(CollusionInputs { price_pairs: &pp, ..Default::default() });
        assert!((weight_of(&c, "facts").unwrap() - PRICE_WEIGHT).abs() < 1e-6);
    }

    #[test]
    fn level_thresholds_high_medium_low_none() {
        // 峰值满分(0.40) + 元数据(0.25) = 0.65 ≥ LEVEL_HIGH(0.6) → high
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let high = assess_with(CollusionInputs { peak: 1.0, docs: &two, ..Default::default() });
        assert_eq!(high.level, "high");
        assert!(high.score <= 1.0, "score 必须 clamp 到 ≤1");
    }

    // —— M1 取证：rsid 信号（连续特征 x = root_match ? 1 : min(shared/10, 1)）——

    fn rsid_hit(shared: usize, root: bool) -> RsidHit {
        RsidHit { a: 0, b: 1, shared_count: shared, root_match: root }
    }

    #[test]
    fn rsid_root_match_takes_full_weight_and_carries_disclaimers() {
        let hits = [rsid_hit(0, true)];
        let c = assess_with(CollusionInputs { rsid_hits: &hits, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "rsid").expect("应有 rsid 信号");
        assert!((s.weight - RSID_WEIGHT).abs() < 1e-6, "root_match → x=1 满权重");
        assert!(s.detail.contains("另存为"), "detail 应含「另存为即可清除」免责语");
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(s.detail.contains("统一模板"), "detail 应注明招标方统一模板可能");
        assert!(s.detail.contains("rsidRoot 相同"));
    }

    #[test]
    fn rsid_weight_scales_continuously_with_shared_count() {
        // shared=3 → x=0.3；shared=10 → x=1；shared=25 → 封顶 x=1（不叠加、不越界）
        let w_of = |hits: &[RsidHit]| {
            let c = assess_with(CollusionInputs { rsid_hits: hits, ..Default::default() });
            weight_of(&c, "rsid")
        };
        let w3 = w_of(&[rsid_hit(3, false)]).expect("shared=3 应有信号");
        assert!((w3 - RSID_WEIGHT * 0.3).abs() < 1e-6, "shared=3 → 0.35×0.3");
        let w10 = w_of(&[rsid_hit(10, false)]).unwrap();
        assert!((w10 - RSID_WEIGHT).abs() < 1e-6, "shared=10 饱和到满权重");
        let w25 = w_of(&[rsid_hit(25, false)]).unwrap();
        assert!((w25 - RSID_WEIGHT).abs() < 1e-6, "超饱和不越界");
        // 多对取最强：3 与 10 并存 → 仍是满权重一次，不叠加
        let multi = w_of(&[rsid_hit(3, false), rsid_hit(10, false)]).unwrap();
        assert!((multi - RSID_WEIGHT).abs() < 1e-6);
    }

    #[test]
    fn rsid_below_min_shared_without_root_yields_no_signal() {
        // 防御过滤：即便调用方传入未过滤的弱命中（shared<3 且非 root）也不产生信号
        let hits = [rsid_hit(2, false)];
        let c = assess_with(CollusionInputs { rsid_hits: &hits, ..Default::default() });
        assert!(weight_of(&c, "rsid").is_none());
        assert_eq!(c.level, "none");
    }

    // —— M1 取证：metadata 信号只认强类别、detail 枚举具体命中项 ——

    #[test]
    fn metadata_detail_enumerates_hit_categories() {
        let two = vec![
            doc(vec!["作者相同「张三」: 甲·乙", "模板相同「投标模板.dotx」: 甲·乙"]),
            doc(vec!["作者相同「张三」: 甲·乙", "创建时间邻近（≤10 分钟）: 甲·乙"]),
        ];
        let c = assess_with(CollusionInputs { docs: &two, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "metadata").expect("应有 metadata 信号");
        assert!(s.detail.contains("作者相同"), "detail 应枚举命中项：{}", s.detail);
        assert!(s.detail.contains("模板相同"));
        assert!(s.detail.contains("创建时间邻近"));
        assert!(s.detail.contains("未命中不代表清白"));
    }

    #[test]
    fn rsid_and_weak_flags_do_not_trigger_metadata_signal() {
        // rsid 有独立信号、弱标记不计权：只有这些标记时不得再计 metadata（防双计）
        let two = vec![
            doc(vec!["rsid 交集 甲·乙：共享 5 个修订标识", "修订号相同（弱）「7」: 甲·乙"]),
            doc(vec!["rsid 交集 甲·乙：共享 5 个修订标识", "疑似元数据清洗：总编辑时长为 0 但修订号达 12（弱）"]),
        ];
        let c = assess_with(CollusionInputs { docs: &two, ..Default::default() });
        assert!(weight_of(&c, "metadata").is_none(), "rsid/弱标记不应计入 metadata");
    }

    // —— M1 取证：PDF 血缘信号（连续特征 x = 硬命中 ? 1.0 : PDF_LINEAGE_MID_X）——

    fn lineage_hit(hard: bool, tags: &[&str]) -> LineageHit {
        LineageHit {
            a: 0,
            b: 1,
            hard_evidence: if hard { vec!["XMP DocumentID 相同".into()] } else { vec![] },
            shared_subset_tags: tags.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn pdf_lineage_hard_hit_takes_full_weight_and_carries_disclaimers() {
        let hits = [lineage_hit(true, &[])];
        let c = assess_with(CollusionInputs { lineage_hits: &hits, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "pdfLineage").expect("应有 pdfLineage 信号");
        assert!((s.weight - PDF_LINEAGE_WEIGHT).abs() < 1e-6, "硬命中 x=1 满权重");
        assert!(s.detail.contains("同一母文件"));
        assert!(s.detail.contains("元数据可被抹除"), "detail 应含免责语：{}", s.detail);
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(s.detail.contains("统一模板"), "detail 应把判定权留给评标人");
        assert_eq!(c.level, "medium", "0.35 ≥ LEVEL_MEDIUM");
    }

    #[test]
    fn pdf_lineage_mid_only_scales_down_and_takes_max_not_sum() {
        // 仅中命中（共享字体子集标签）→ x=PDF_LINEAGE_MID_X
        let mid = [lineage_hit(false, &["ABCDEF+SimSun"])];
        let c = assess_with(CollusionInputs { lineage_hits: &mid, ..Default::default() });
        let w = weight_of(&c, "pdfLineage").expect("中命中应有信号");
        assert!((w - PDF_LINEAGE_WEIGHT * PDF_LINEAGE_MID_X).abs() < 1e-6, "实际 {w}");
        // 中 + 硬并存：取最强一次，不叠加
        let both = [lineage_hit(false, &["ABCDEF+SimSun"]), lineage_hit(true, &[])];
        let cb = assess_with(CollusionInputs { lineage_hits: &both, ..Default::default() });
        let wb = weight_of(&cb, "pdfLineage").unwrap();
        assert!((wb - PDF_LINEAGE_WEIGHT).abs() < 1e-6, "多对取最强不叠加，实际 {wb}");
    }

    #[test]
    fn pdf_lineage_empty_evidence_hit_is_defensively_ignored() {
        // 防御过滤：两档证据皆空的无效命中不产生信号
        let hits = [lineage_hit(false, &[])];
        let c = assess_with(CollusionInputs { lineage_hits: &hits, ..Default::default() });
        assert!(weight_of(&c, "pdfLineage").is_none());
        assert_eq!(c.level, "none");
    }

    #[test]
    fn generation_env_weak_flag_counts_into_metadata_not_pdf_lineage() {
        // PDF 血缘弱命中（生成环境一致）并入 metadata 计权，不产生 pdfLineage 信号
        let two = vec![
            doc(vec!["生成环境一致（CreatorTool/Producer/字体一致且创建时间邻近）: 甲·乙"]),
            doc(vec!["生成环境一致（CreatorTool/Producer/字体一致且创建时间邻近）: 甲·乙"]),
        ];
        let c = assess_with(CollusionInputs { docs: &two, ..Default::default() });
        assert!((weight_of(&c, "metadata").unwrap() - META_WEIGHT).abs() < 1e-6);
        assert!(weight_of(&c, "pdfLineage").is_none());
    }

    #[test]
    fn pdf_lineage_flags_do_not_trigger_metadata_signal() {
        // PDF 血缘硬/中命中有独立信号：其风险标记不得再计 metadata（防双计）
        let two = vec![
            doc(vec!["PDF 血缘 甲·乙：XMP DocumentID 相同（同一母文件）"]),
            doc(vec!["PDF 血缘 甲·乙：共享字体子集标签「ABCDEF+SimSun」"]),
        ];
        let c = assess_with(CollusionInputs { docs: &two, ..Default::default() });
        assert!(weight_of(&c, "metadata").is_none(), "血缘标记不应计入 metadata");
    }

    // —— M1 取证：内嵌图片同源信号（两两碰撞 → 连续特征 x = min(命中图对数/3, 1)）——

    fn img(sha: &str, dhash: Option<u64>, page: Option<u32>) -> ImageFp {
        ImageFp { sha256: sha.into(), dhash, page }
    }
    fn no_exempt() -> HashSet<String> {
        HashSet::new()
    }

    #[test]
    fn image_exact_hit_produces_signal_with_scaled_weight() {
        // 两文档同一张图（sha256 相等）→ 1 对命中，x=1/3，weight = 0.25×1/3
        let per_doc = vec![
            vec![img("SAME", Some(0), Some(3))],
            vec![img("SAME", Some(0), Some(5))],
        ];
        let hits = image_pairs(&per_doc, &no_exempt());
        assert_eq!(hits.len(), 1);
        assert!(hits[0].exact, "sha256 相等应为精确命中");
        let c = assess_with(CollusionInputs { image_hits: &hits, ..Default::default() });
        let w = weight_of(&c, "imageReuse").expect("应有 imageReuse 信号");
        assert!((w - IMAGE_REUSE_WEIGHT / 3.0).abs() < 1e-6, "1 对 → 0.25×1/3，实际 {w}");
    }

    #[test]
    fn image_near_hit_within_hamming_threshold() {
        // dHash 汉明距离 3（≤10）且非整页图 → 近似命中；sha256 不同
        let per_doc = vec![
            vec![img("A", Some(0), None)],
            vec![img("B", Some(0b111), None)],
        ];
        let hits = image_pairs(&per_doc, &no_exempt());
        assert_eq!(hits.len(), 1);
        assert!(!hits[0].exact, "sha256 不同、dHash 近似应为近似命中");
    }

    #[test]
    fn image_far_dhash_produces_no_hit_or_signal() {
        // 汉明距离 32（>10）+ sha256 不同 → 随机噪声图不命中、无信号
        let per_doc = vec![
            vec![img("A", Some(0), None)],
            vec![img("B", Some(0xFFFF_FFFF), None)],
        ];
        let hits = image_pairs(&per_doc, &no_exempt());
        assert!(hits.is_empty(), "远距 dHash 不应命中");
        let c = assess_with(CollusionInputs { image_hits: &hits, ..Default::default() });
        assert!(weight_of(&c, "imageReuse").is_none());
        assert_eq!(c.level, "none");
    }

    #[test]
    fn full_page_image_only_exact_never_near() {
        // 整页图 dhash=None：即便像素相近也不做 near；仅 sha256 相等才命中
        let near_but_full = vec![
            vec![img("P", None, Some(1))],
            vec![img("Q", None, Some(1))], // sha 不同 + 无 dhash → 不命中
        ];
        assert!(image_pairs(&near_but_full, &no_exempt()).is_empty(), "整页图不做 near");
        let exact_full = vec![
            vec![img("S", None, Some(1))],
            vec![img("S", None, Some(2))], // sha 相同 → exact 仍命中
        ];
        let hits = image_pairs(&exact_full, &no_exempt());
        assert_eq!(hits.len(), 1);
        assert!(hits[0].exact);
    }

    #[test]
    fn image_reuse_weight_saturates_at_three_pairs() {
        // ≥3 对命中 → x=1，weight 封顶 IMAGE_REUSE_WEIGHT（三文档共用同一图产生 3 对）
        let per_doc = vec![
            vec![img("Z", Some(0), Some(1))],
            vec![img("Z", Some(0), Some(2))],
            vec![img("Z", Some(0), Some(3))],
        ];
        let hits = image_pairs(&per_doc, &no_exempt());
        assert_eq!(hits.len(), 3, "3 文档共用同一图 → 3 对");
        let c = assess_with(CollusionInputs { image_hits: &hits, ..Default::default() });
        let w = weight_of(&c, "imageReuse").unwrap();
        assert!((w - IMAGE_REUSE_WEIGHT).abs() < 1e-6, "3 对封顶满权重，实际 {w}");
    }

    #[test]
    fn image_exempt_hash_removes_hit() {
        // 招标文件统一提供的图哈希在豁免集中 → 命中被剔除（M4 对减接线预留）
        let per_doc = vec![
            vec![img("TENDER_IMG", Some(0), Some(1))],
            vec![img("TENDER_IMG", Some(0), Some(1))],
        ];
        let mut exempt = HashSet::new();
        exempt.insert("TENDER_IMG".to_string());
        assert!(image_pairs(&per_doc, &exempt).is_empty(), "豁免图不算串标");
    }

    #[test]
    fn image_reuse_detail_carries_check_and_disclaimer() {
        let per_doc = vec![
            vec![img("SAME", Some(0), Some(3))],
            vec![img("SAME", Some(0), Some(5))],
        ];
        let hits = image_pairs(&per_doc, &no_exempt());
        let c = assess_with(CollusionInputs { image_hits: &hits, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "imageReuse").expect("应有 imageReuse 信号");
        assert!(s.detail.contains("请核对"), "应提示核对是否来自招标文件：{}", s.detail);
        assert!(s.detail.contains("招标文件"));
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(s.detail.contains("第3页") && s.detail.contains("第5页"), "detail 应含天干+页码：{}", s.detail);
        // 不得输出背书式表述
        assert!(!s.detail.contains("检查通过") && !s.detail.contains("清白证明"));
    }

    #[test]
    fn image_hit_dedups_one_source_image_against_many() {
        // 同一张 a 图撞 b 中多张（近似 + 精确）只计一对，避免虚增命中数
        let per_doc = vec![
            vec![img("A", Some(0), Some(1))],
            vec![img("A", Some(0), Some(2)), img("B", Some(0b1), Some(3))],
        ];
        let hits = image_pairs(&per_doc, &no_exempt());
        assert_eq!(hits.len(), 1, "一张 a 图记一对即止");
    }

    // —— M1 取证：共同错误指纹信号（连续特征 x = min(Σ稀有度/5, 1)）——

    fn shared_error(term: &str, rarity: f32, ctx: Option<&str>) -> SharedTerm {
        SharedTerm {
            term: term.into(),
            docs: vec![0, 1],
            kind: Some("sharedErrors".into()),
            rarity: Some(rarity),
            context: ctx.map(String::from),
        }
    }

    #[test]
    fn shared_errors_signal_scales_with_rarity_and_carries_disclaimer() {
        // 单条满稀有度错误：x = min(1.0/5, 1) = 0.2 → weight = 0.25×0.2 = 0.05（连续特征，无 floor）
        let errs = [shared_error("施工枝术", 1.0, Some("的施工枝术方案"))];
        let c = assess_with(CollusionInputs { shared_errors: &errs, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "sharedErrors").expect("应有 sharedErrors 信号");
        assert!((s.weight - SHARED_ERRORS_WEIGHT * 0.2).abs() < 1e-6, "单条满稀有度 → 0.25×0.2，实际 {}", s.weight);
        assert!(s.detail.contains("疑似"), "措辞应为「疑似错误」不定性：{}", s.detail);
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(s.detail.contains("豁免"), "应附招标文件笔误豁免说明：{}", s.detail);
        assert!(s.detail.contains("施工枝术") && s.detail.contains("的施工枝术方案"), "detail 应含错误串与前后文：{}", s.detail);
        assert!(!s.detail.contains("检查通过") && !s.detail.contains("清白证明"));
    }

    #[test]
    fn shared_errors_weight_saturates_at_five_weighted() {
        // Σ稀有度 = 5（5 条满稀有度）→ x=1 → weight 封顶 SHARED_ERRORS_WEIGHT；超饱和不越界
        let five: Vec<SharedTerm> = (0..5).map(|i| shared_error(&format!("错{i}"), 1.0, None)).collect();
        let c = assess_with(CollusionInputs { shared_errors: &five, ..Default::default() });
        let w = weight_of(&c, "sharedErrors").unwrap();
        assert!((w - SHARED_ERRORS_WEIGHT).abs() < 1e-6, "5 条满稀有度饱和到满权重，实际 {w}");
        let seven: Vec<SharedTerm> = (0..7).map(|i| shared_error(&format!("错{i}"), 1.0, None)).collect();
        let c7 = assess_with(CollusionInputs { shared_errors: &seven, ..Default::default() });
        assert!((weight_of(&c7, "sharedErrors").unwrap() - SHARED_ERRORS_WEIGHT).abs() < 1e-6, "超饱和不越界");
    }

    #[test]
    fn shared_errors_empty_yields_no_signal() {
        let c = assess_with(CollusionInputs { shared_errors: &[], ..Default::default() });
        assert!(weight_of(&c, "sharedErrors").is_none());
        assert_eq!(c.level, "none");
    }

    // —— 取证封顶：四类取证信号全满时，取证部分对总分的合计贡献 ≤ FORENSIC_CAP ——

    #[test]
    fn forensic_cap_limits_combined_forensic_contribution() {
        // rsid(0.35)+pdfLineage(0.35)+imageReuse(0.25)+sharedErrors(0.25) 原始合计 1.20，
        // 封顶后对 score 的贡献应为 FORENSIC_CAP(0.45)。无其它信号 → score == 0.45 → medium（非 high）。
        let rsid = [rsid_hit(0, true)]; // x=1 → 0.35
        let lineage = [lineage_hit(true, &[])]; // 硬命中 x=1 → 0.35
        let images = [
            ImageHit { a: 0, b: 1, page_a: None, page_b: None, exact: true },
            ImageHit { a: 0, b: 2, page_a: None, page_b: None, exact: true },
            ImageHit { a: 1, b: 2, page_a: None, page_b: None, exact: true },
        ]; // 3 对 → x=1 → 0.25
        let errs: Vec<SharedTerm> = (0..5).map(|i| shared_error(&format!("错{i}"), 1.0, None)).collect(); // x=1 → 0.25
        let c = assess_with(CollusionInputs {
            rsid_hits: &rsid,
            lineage_hits: &lineage,
            image_hits: &images,
            shared_errors: &errs,
            ..Default::default()
        });
        // 各信号 detail 仍呈现原始权重（不受封顶影响）
        assert!((weight_of(&c, "rsid").unwrap() - RSID_WEIGHT).abs() < 1e-6);
        assert!((weight_of(&c, "pdfLineage").unwrap() - PDF_LINEAGE_WEIGHT).abs() < 1e-6);
        assert!((weight_of(&c, "imageReuse").unwrap() - IMAGE_REUSE_WEIGHT).abs() < 1e-6);
        assert!((weight_of(&c, "sharedErrors").unwrap() - SHARED_ERRORS_WEIGHT).abs() < 1e-6);
        // 取证部分封顶：总分恰为 FORENSIC_CAP
        assert!((c.score - FORENSIC_CAP).abs() < 1e-6, "取证部分应封顶到 {FORENSIC_CAP}，实际 {}", c.score);
        assert_eq!(c.level, "medium", "四类取证叠满仍为 medium（封顶防直接 high）");
    }

    #[test]
    fn forensic_cap_does_not_touch_non_forensic_score() {
        // 非取证信号（相似度 0.40 + 元数据 0.25 = 0.65）不受封顶影响，仍可达 high
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let c = assess_with(CollusionInputs { peak: 1.0, docs: &two, ..Default::default() });
        assert_eq!(c.level, "high", "非取证信号不封顶，实际 {:.2}", c.score);
    }
}
