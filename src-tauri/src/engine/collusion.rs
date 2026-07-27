// 围标综合判定：把文本相似度、跨文档雷同条款、元数据同源、共有特征词、报价梯度、rsid 交集、
// PDF 血缘、图片同源、共同错误、规避特征、商务标数值层等【全量信号】融合成一个结论。
// M7 起融合规则为语料拟合的 log-LR（见下方「M7 融合层」注释块），不再是经验权重线性叠加。
use crate::engine::fingerprint::{LineageHit, RsidHit, RSID_MIN_SHARED};
use crate::engine::report::{Cluster, Collusion, CollusionSignal, DocInfo, EvasionSummary, SharedTerm};
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

/// 规律性差异形态的中文标签（boq::PatternKind::as_str 的展示侧映射）。
fn pattern_kind_cn(kind: &str) -> &str {
    match kind {
        "arith_seq" => "等差（各项差额恒定）",
        "geo_discount" => "等比 / 恒定折扣（各项系数恒定）",
        "affine" => "仿射（系数与差额均非平凡）",
        other => other,
    }
}

/// 同源图片位置文案：PDF 用页码，docx 内嵌图无页码。
fn img_loc(page: Option<u32>) -> String {
    match page {
        Some(p) => format!("第{p}页"),
        None => "内嵌图".to_string(),
    }
}

// —— 围标信号的【特征口径】与 v1 经验权重（集中于此） ——
// M7 起分工变了：以下「权重」常量不再直接决定分数，而是承担两件事——
//   ① 定义每个信号的【连续特征口径】：x_i = 该信号的 v1 贡献 / 满档权重 ⇒ x=1 恒为满档；
//   ② 作为融合层的【经验先验/回退权重】（EMPIRICAL_WEIGHTS，见下方 M7 融合层注释块）。
// 实际打分权重来自 fixtures/calibration/collusion_lr.json（合成语料拟合，实验性校准）。
// ⚠️ 以下经验值仍未经真实案例语料回测：调这里等于调「特征口径 + 回退档」，调完须重跑
// fit-collusion 与 corpus_regression。
const SIM_FLOOR: f32 = 0.6; // 相似度峰值起算线
const SIM_WEIGHT: f32 = 0.4; // 相似度信号满权重
const CLUSTER_MULTI_DOCS: usize = 3; // ≥N 份共现算强雷同
const CLUSTER_BASE: f32 = 0.1; // 有雷同条款的基础权重
const CLUSTER_SCALE: f32 = 0.3; // 强雷同随数量增长的权重
const CLUSTER_SCALE_CAP: f32 = 5.0; // multi/CAP 封顶到 1
// 多家异常一致信号（W3-3，连续特征）：贡献 = MULTI_ANOMALY_WEIGHT × min(异常簇数/SAT, 1)。
// ≥3 家共有且招标文件与行业范本库【均查不到出处】、且查证质量闸门通过（招标件已导入、非
// OCR/扫描件、对减覆盖率抽样达标）的簇，对应《招标投标法实施条例》第四十条『投标文件异常一致』
// 涉嫌情形。§1.5 铁律：单信号权重不达 high 线(0.6) 是有意设计——不自动 high、不定性，簇 severity
// 独立标『待复核』、不进 high 统计，最终认定权属评标委员会。
// ⚠️未经校准：0.30 为经验初值，尚无带标注真实案例语料回测（同现有五信号，随 scheme §9.3 校准）。
const MULTI_ANOMALY_WEIGHT: f32 = 0.30;
const MULTI_ANOMALY_SATURATION: f32 = 3.0; // 3 处异常一致即满档
const META_MIN_DOCS: usize = 2; // ≥N 份元数据同源才计
const META_WEIGHT: f32 = 0.25;
const SHARED_TERMS_MIN: usize = 5; // ≥N 个共有特征词才计
const SHARED_TERMS_WEIGHT: f32 = 0.1;
/// 共有罕见词满档线（M7 连续特征化）：达 15 个即满档，之间线性。
const SHARED_TERMS_SATURATION: f32 = 15.0;
// 共同错误指纹信号（M1 取证，连续特征）：贡献 = SHARED_ERRORS_WEIGHT × x，
// x = min(Σ稀有度 / SHARED_ERRORS_SATURATION, 1)。词典外词/异常标点/错误引用的跨文档共现，
// 共用同一处罕见错误比共用正确词证明力高一个量级（调研 §5/§13：identical wrong answers）；
// 稀有度加权避免高频「新词/术语」误报，措辞「疑似错误」不直接定性。
const SHARED_ERRORS_WEIGHT: f32 = 0.25;
const SHARED_ERRORS_SATURATION: f32 = 5.0; // 加权错误数达 5 即满档
const SHARED_ERRORS_SHOW_MAX: usize = 3; // detail 最多列出几条疑似错误
// 报价梯度（旧第 5 信号）——M6 起【降级为回落信号】：仅当本次比对拿不到任何报价清单数据
// （CollusionInputs.numeric = None：纯技术标 / 扫描件 PDF 的 OCR 路径 / 数值层关闭）时才计权。
// 有 BOQ 时由数值层四类信号取代：全文最大金额差 <3% 的启发式易被业绩金额劫持，证明力远低于
// 逐项单价雷同率与共享算术错误（scheme §8.4 已列为已知短板）。
const PRICE_WEIGHT: f32 = 0.15;
const PRICE_SHOW_MAX: usize = 3; // 报价梯度对最多列出几对
/// 报价梯度起算的最大差距（与 compare_service::price_proximity 的入选门槛一致，M7 连续特征化用作分母）。
const PRICE_GAP_MAX: f32 = 0.03;
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
// 三条分级线【定义在 v1 加权和尺度上】：M7 起不再直接与 score 比较，而是经
// LrModel::v1_line_equivalent 换算到当前模型的证据强度尺度后再比（见该函数注释）。
pub const LEVEL_HIGH: f32 = 0.6;
pub const LEVEL_MEDIUM: f32 = 0.35;
pub const LEVEL_LOW: f32 = 0.1; // score > LEVEL_LOW → low
/// 取证类信号（rsid / PDF 血缘 / 内嵌图片同源 / 共同错误指纹）的合计封顶：四类各自满档相加
/// 可达 1.20，不封顶则任意两三类叠满即直接 high，越过「单点定案需人工核实」的产品边界。
/// 【M7：这是 LR 之外保留的显式产品纪律，不是统计项】——在 log-odds 尺度上按同一比例
/// FORENSIC_CAP/FORENSIC_EMP_TOTAL 作用于该族权重之和（见 LrModel::finish），故换权重文件
/// 不会稀释该纪律。各信号 detail 仍呈现未封顶的原始贡献供人工判断。⚠️ 0.45 未经语料校准。
const FORENSIC_CAP: f32 = 0.45;
/// 条件化硬命中 floor（§1.5 铁律，M4 招标豁免落地后激活）：硬命中（rsid rsidRoot 相同 /
/// PDF DocumentID·DerivedFrom·trailer-ID 相同）在【工作区已导入招标文件且豁免对减生效】
/// (CollusionInputs.tender_exemption_active) 时，强制围标等级下限 medium（不直接 high）。
/// 招标文件不存在或豁免不可用时，硬命中只作信号展示（保留 rsid/pdfLineage 信号与免责文案），
/// 不设等级下限、不进等级判定——防「招标代理统一下发投标模板」这一主流合规场景被系统性抬级
/// （§9 排期审查 HIGH：豁免对减先于 floor 生效，扣除模板后仍硬命中才触发下限）。
const HARD_HIT_FLOOR_LEVEL: &str = "medium";
/// floor 触发时的取证纪律文案（常量集中，导出/UI 复用）。
const HARD_HIT_FLOOR_DETAIL: &str = "已扣除招标文件统一下发模板（rsid/图片/共同错误已对减）后仍存在硬命中（同一母文件），\
     围标等级下限置为 medium 供人工复核；此为等级下限规则、非定性结论，未命中不代表清白，\
     最终认定权属评标委员会";
// 检测到疑似规避特征（M2 入口对抗层聚合）：独立信号，【在 FORENSIC_CAP 之外】——规避行为
// 本身即极强串通证据（正常投标人不会做字体重映射/零宽注入/PDF 隐藏文字层），比文本相似度
// 更难抵赖。连续特征 x = 任一文档 confirmed ? 1.0 : 仅 suspect ? 0.5 : 0；同类证据不叠加。
// ⚠️ 0.25 未经校准；单信号不达 high 线(0.6) 是有意设计——单证据不定罪（§1.5）。
const EVASION_WEIGHT: f32 = 0.25;
const EVASION_SHOW_MAX: usize = 5; // detail 最多列出几份命中文档

// —— M6 商务标数值层信号（W5-6，连续特征 x∈[0,1]）——
// ⚠️ 未经实证校准：以下权重同为经验初值，随其余信号一起进 M7 的统一回测拟合。
//
// 共享算术错误：单条【降档】（§1.5 审查修正——同款计价软件的舍入惯例足以让两家在同一行
// 算出同样的错值），要求 ≥2 条【相互独立】（不同清单项）的错误才给满档 0.35。
// 检测侧（boq::pair_stats）已先排除可由常见舍入规则解释的差值，此处只做档位。
const NUMERIC_ARITH_ERROR_WEIGHT: f32 = 0.35;
const NUMERIC_ARITH_ERROR_SINGLE_WEIGHT: f32 = 0.15;
/// 给满档所需的相互独立错误条数（§1.5 硬约束）。
const NUMERIC_ARITH_ERROR_MIN_INDEPENDENT: usize = 2;
/// 逐项单价雷同率：达告警线起 NUMERIC_IDENTICAL_BASE，按超出幅度线性至 NUMERIC_IDENTICAL_MAX
/// （满档在 rate=1.0，即逐项单价完全一致）。告警线本身可配（默认 0.80，随任务配置快照）。
const NUMERIC_IDENTICAL_BASE: f32 = 0.20;
const NUMERIC_IDENTICAL_MAX: f32 = 0.30;
/// 规律性差异（等差/等比/仿射）：定位为【线索】，detail 强制附「统一下浮」提示。
const NUMERIC_PATTERN_WEIGHT: f32 = 0.15;
/// 相关性：只有 r>0.99 且比值 CV<0.5% 双条件同时成立才计权——投标人单价天然同源
/// （同一定额库/信息价）会让 r 普遍 0.9+，单看 r 是噪声。
const NUMERIC_CORRELATION_WEIGHT: f32 = 0.10;
pub const NUMERIC_CORRELATION_R_MIN: f64 = 0.99;
pub const NUMERIC_CORRELATION_CV_MAX: f64 = 0.005;
/// 机制感知反事实（W5-5，【后置二期】）：mechanism_flip_prob 缺席时该信号不出。
const NUMERIC_MECHANISM_WEIGHT: f32 = 0.15;
const NUMERIC_MECHANISM_FLIP_MIN: f32 = 0.5;
/// 数值类信号（雷同率/共享算术错误/规律性/相关性/机制）的合计封顶，【独立于 FORENSIC_CAP】：
/// 五类各自满档相加可达 1.05，且逐项雷同的清单行本身已抬高文本相似峰值与聚类数（数值证据与
/// 文本证据存在结构性双重计数），不封顶则数值层单独即可定案。同 FORENSIC_CAP，【M7 起是 LR
/// 之外保留的显式纪律】，按 NUMERIC_CAP/NUMERIC_EMP_TOTAL 比例作用于 log-odds 尺度。
/// ⚠️ 0.45 未经语料校准。各信号 detail 仍呈现原始贡献（证明力不受封顶影响）。
const NUMERIC_CAP: f32 = 0.45;
/// 共享算术错误 detail 最多列出几对。
const NUMERIC_ARITH_SHOW_MAX: usize = 3;
/// §1.5 强制措辞：雷同率口径声明——避免越权定性。
const NUMERIC_IDENTICAL_NOTE: &str = "该指标为逐项单价相同率（参照地方雷同认定口径，针对逐项单价相同率）；\
     达到告警线仅表示需重点核查，不构成串通投标认定，最终认定权属评标委员会";
/// §1.5 强制措辞：共享算术错误须先排除计价软件/招标文件来源。
const NUMERIC_ARITH_NOTE: &str = "请核对是否源自同一计价软件舍入惯例或招标文件；\
     检测已排除可由常见舍入规则解释的差值，但仍需人工核对原文";
/// §1.5 强制措辞：规律性差异只是线索。
const NUMERIC_PATTERN_NOTE: &str = "规律性差异属线索而非认定：可能源于对同一控制价/定额库的统一下浮，\
     需结合取证类证据综合判断";
/// §1.5 强制措辞：相关系数须与比值 CV、散点形态同屏判读。
const NUMERIC_CORRELATION_NOTE: &str = "投标人单价天然同源会使相关系数普遍偏高：\
     只有 r>0.99 且比值 CV≈0 才是强证据，须结合散点形态判读";

/// 商务标数值层证据（M6 / W5-6）：由 compare_service 从 boq::PairStats 聚合而来。
/// 全部字段均为「已聚合到文档集层面」的最强值——数值信号与其它信号一样，同类证据只记一次。
/// 缺席（None / 0）即该子信号不出，不产生任何分数。
#[derive(Debug, Clone, Default)]
pub struct NumericEvidence {
    /// 跨全部文档对的最大逐项单价雷同率；None = 所有对的可比条目都不足，不出结论。
    pub max_identical_rate: Option<f32>,
    /// 上述最大雷同率所在文档对（docs 下标，a<b）。
    pub max_identical_pair: Option<(usize, usize)>,
    /// 本次任务生效的雷同率告警线（默认 0.80，可配；随任务配置快照落库，保证报告可复现）。
    pub identical_alarm_line: f32,
    /// 【相互独立】的共享算术错误条数：跨全部文档对按清单项（align_key）去重后的条数。
    /// 同一清单项在多个文档对里重复命中只算一条——独立性口径是「不同清单项」。
    pub shared_arith_error_count: usize,
    /// 共享算术错误命中的文档对（去重，按 (a,b) 升序），供 detail 列出。
    pub shared_arith_error_pairs: Vec<(usize, usize)>,
    /// 规律性差异形态（"arith_seq" | "geo_discount" | "affine"）；None = 未达门槛。
    pub regularity_kind: Option<String>,
    pub regularity_pair: Option<(usize, usize)>,
    /// 规律性系数：等比取斜率 a（折扣系数），等差取截距 b（恒定差额，元）。仅供 detail 文案。
    pub regularity_coeff: Option<f64>,
    /// 【双条件已在构造侧过滤】的最大 Pearson：r>NUMERIC_CORRELATION_R_MIN
    /// 且比值 CV<NUMERIC_CORRELATION_CV_MAX。单看 r 高不入此字段。
    pub max_pearson_with_low_ratio_cv: Option<f32>,
    pub correlation_pair: Option<(usize, usize)>,
    /// 比值 CV（与上一字段同一对），detail 与 r 同屏展示（§1.5）。
    pub correlation_ratio_cv: Option<f64>,
    /// 机制感知反事实基准价的中标翻转概率（W5-5）。【W5-5 已后置二期】：当前恒为 None，
    /// 该信号不出；字段先行定义以免二期再改 assess_with 签名（§1.2 裁决）。
    pub mechanism_flip_prob: Option<f32>,
}

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
    /// 报价梯度接近对（金额接近但条款雷同的「陪标价」候选）。
    /// M6 起为【回落信号】：仅在 numeric = None（拿不到任何清单数据）时计权。
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
    /// —— M2 规避：各参评文档的规避特征摘要（compare_service 从 evasion_json 判级后传入，
    /// 与 docs 同序、同下标；元素 None = 该文档无 evasion_json/无发现）。独立信号在 FORENSIC_CAP
    /// 之外，见 EVASION_WEIGHT。空切片 = 无规避数据（旧任务/全清白）——
    pub evasion: &'a [Option<EvasionSummary>],
    /// —— M4 豁免接线：工作区已导入招标文件且三类豁免对减（rsid/图片/共同错误）已生效。
    /// 仅当为 true 时启用条件化硬命中 floor（见 HARD_HIT_FLOOR_LEVEL）；Default=false ⇒
    /// 招标文件不存在/未开对减时硬命中只作信号展示、不设等级下限——
    pub tender_exemption_active: bool,
    /// —— M6 数值：商务标数值层证据（compare_service 由 boq::PairStats 聚合）。
    /// None = 本次拿不到任何报价清单数据（纯技术标 / 扫描件 PDF / 数值层关闭）⇒ 数值四类信号
    /// 全不出，且旧「报价梯度」回落信号照常触发（保证无清单场景行为不回退）——
    pub numeric: Option<&'a NumericEvidence>,
}

// ————————————————————————————————————————————————————————————————————————
// M7 融合层（W6-3，§1.2 全局裁决「LR 最后落地、对全量特征一次性拟合」）
//
// 全部信号统一为连续特征 x_i∈[0,1]，融合规则由「经验权重线性叠加」改为语料拟合的 log-LR：
//     z = b + Σ w_i·x_i（取证族 / 数值族各自封顶后并入）
//     p = σ(z)
//     score = (p − σ(b)) / (1 − σ(b))    ← 以「零证据」为零点重基后的【校准后证据强度】
// 重基保证【全零输入 score 恒为 0.0、level=none】——截距不得抬底分（验收④）。score 语义
// 由此从「加权和」变为「校准后证据强度」，导出报告脚注与引擎版本号一并注明（§1.5-5）。
//
// 权重来源：fixtures/calibration/collusion_lr.json，include_str! 随包固化、运行时不可热换
// （保证同输入同输出、结果可举证）。加载时做【符号与量级审查】（§1.5-4：取证/数值类信号
// 的负权重在监管场景解释不通）；解析失败或校验不过 → log::warn + 回退 v1 经验权重先验，
// 比对不失败（验收③）。
//
// v1 经验权重经 PRIOR_SLOPE/PRIOR_REF 映射到 log-odds 尺度即【回退模型】，同时也是拟合时
// 的高斯先验中心（corpusgen fit-collusion 的 L2 向该先验收缩，而非向 0 收缩——向 0 收缩会
// 让语料里没有区分度的列«死掉»，例如无 BOQ 时才出的报价梯度信号）。该映射是严格单调仿射
// 变换、分级线同样取其像，故【回退路径与 v1 逐例等价】，只换了分数刻度。
//
// LR 之外保留的显式规则（产品纪律，不是统计项，§1.2 裁决）：FORENSIC_CAP / NUMERIC_CAP 的
// 分族封顶、条件化硬命中 floor。封顶在 log-odds 尺度上的等价表达见 cap_ratio 注释。
// ————————————————————————————————————————————————————————————————————————

/// 全信号特征列名（= CollusionSignal.kind，顺序即权重向量列序）：
/// 0..7 普通信号、7..11 取证四类（受 FORENSIC_CAP）、11..16 数值五类（受 NUMERIC_CAP）。
pub const FEATURE_KINDS: [&str; 16] = [
    "similarity",
    "cluster",
    "multiDocAnomaly",
    "metadata",
    "sharedTerms",
    "facts",
    "evasion",
    "rsid",
    "pdfLineage",
    "imageReuse",
    "sharedErrors",
    "numericArithError",
    "numericIdentical",
    "numericPattern",
    "numericCorrelation",
    "numericMechanism",
];
pub const FEATURE_COUNT: usize = FEATURE_KINDS.len();
const F_SIMILARITY: usize = 0;
const F_CLUSTER: usize = 1;
const F_MULTI_ANOMALY: usize = 2;
const F_METADATA: usize = 3;
const F_SHARED_TERMS: usize = 4;
const F_FACTS: usize = 5;
const F_EVASION: usize = 6;
const F_RSID: usize = 7;
const F_PDF_LINEAGE: usize = 8;
const F_IMAGE_REUSE: usize = 9;
const F_SHARED_ERRORS: usize = 10;
const F_NUMERIC_ARITH: usize = 11;
const F_NUMERIC_IDENTICAL: usize = 12;
const F_NUMERIC_PATTERN: usize = 13;
const F_NUMERIC_CORRELATION: usize = 14;
const F_NUMERIC_MECHANISM: usize = 15;
/// 取证族列区间（合计受 FORENSIC_CAP）。
const FORENSIC_COLS: std::ops::Range<usize> = F_RSID..F_NUMERIC_ARITH;
/// 数值族列区间（合计受 NUMERIC_CAP）。
const NUMERIC_COLS: std::ops::Range<usize> = F_NUMERIC_ARITH..FEATURE_COUNT;

/// v1 经验「满档」权重（与 FEATURE_KINDS 同序）：既是回退模型的权重来源，也是拟合的先验
/// 中心。连续特征的定义口径统一为 `x_i = v1 该信号贡献 / EMPIRICAL_WEIGHTS[i]`，因此
/// x=1 恒等于「该信号满档」。
pub const EMPIRICAL_WEIGHTS: [f32; FEATURE_COUNT] = [
    SIM_WEIGHT,
    CLUSTER_BASE + CLUSTER_SCALE,
    MULTI_ANOMALY_WEIGHT,
    META_WEIGHT,
    SHARED_TERMS_WEIGHT,
    PRICE_WEIGHT,
    EVASION_WEIGHT,
    RSID_WEIGHT,
    PDF_LINEAGE_WEIGHT,
    IMAGE_REUSE_WEIGHT,
    SHARED_ERRORS_WEIGHT,
    NUMERIC_ARITH_ERROR_WEIGHT,
    NUMERIC_IDENTICAL_MAX,
    NUMERIC_PATTERN_WEIGHT,
    NUMERIC_CORRELATION_WEIGHT,
    NUMERIC_MECHANISM_WEIGHT,
];

/// 经验权重尺度 → log-odds 尺度的换算斜率与参照点：z = PRIOR_SLOPE·(s − PRIOR_REF)，
/// s 为 v1 加权和。PRIOR_REF 取 medium(0.35)/high(0.6) 两线的中位区，斜率 8 使 s∈[0,1]
/// 映射到 z∈[−3.6, 4.4]（p 从 0.03 到 0.99），是量纲换算而非新的经验参数——它对分级判定
/// 没有影响（单调仿射 + 分级线取像），只决定回退路径下 score 的刻度形状。
const PRIOR_SLOPE: f32 = 8.0;
const PRIOR_REF: f32 = 0.45;
/// 校准来源标签（§1.5-6 实验性标签，随 DTO 下发给 UI/导出）。experimental-synthetic 由拟合侧
/// （corpusgen fit-collusion）写入权重文件，运行时只透传不重写。
#[cfg(any(test, feature = "dev-tools"))]
pub const CALIBRATION_EXPERIMENTAL: &str = "experimental-synthetic";
pub const CALIBRATION_EMPIRICAL: &str = "empirical-fallback";
/// 符号与量级审查阈值（§1.5-4）：权重必须非负（负权重解释不通）、量级有上限（防病态拟合）。
const WEIGHT_SIGN_EPS: f32 = 1e-6;
const WEIGHT_ABS_MAX: f32 = 20.0;
/// 截距下限（防权重文件把底分压到数值下溢）与上限（截距 ≥0 意味「零证据也倾向围标」，禁止）。
const INTERCEPT_MIN: f32 = -40.0;
/// 分级比较容差：避免压线用例因浮点末位在两级之间横跳（回退路径要与 v1 逐例等价）。
const LEVEL_EPS: f32 = 1e-6;
/// 取证四类经验满档合计（FORENSIC_CAP 的分母）。封顶纪律与权重尺度无关的等价表达是
/// 「本族至多计入其满档合计的 FORENSIC_CAP/FORENSIC_EMP_TOTAL」——在 v1 尺度上就是
/// 0.45 的绝对封顶，在 log-odds 尺度上按同一比例作用于该族权重之和。
const FORENSIC_EMP_TOTAL: f32 =
    RSID_WEIGHT + PDF_LINEAGE_WEIGHT + IMAGE_REUSE_WEIGHT + SHARED_ERRORS_WEIGHT;
/// 数值五类经验满档合计（NUMERIC_CAP 的分母），同上。
const NUMERIC_EMP_TOTAL: f32 = NUMERIC_ARITH_ERROR_WEIGHT
    + NUMERIC_IDENTICAL_MAX
    + NUMERIC_PATTERN_WEIGHT
    + NUMERIC_CORRELATION_WEIGHT
    + NUMERIC_MECHANISM_WEIGHT;

fn sigmoid(z: f32) -> f32 {
    1.0 / (1.0 + (-z).exp())
}

/// 融合权重模型（运行时只读）。分级线存于【score 尺度】（重基后的证据强度），随权重文件
/// 一起固化——分级线与权重必须同源，否则分级语义与拟合口径脱节。
#[derive(Debug, Clone)]
pub struct LrModel {
    pub calibration_kind: String,
    pub version: String,
    pub intercept: f32,
    pub weights: [f32; FEATURE_COUNT],
    pub level_high: f32,
    pub level_medium: f32,
    pub level_low: f32,
    /// σ(intercept)：零证据基线概率，score 重基的零点。
    base_p: f32,
    /// 取证族 log-odds 合计封顶。
    forensic_cap_z: f32,
    /// 数值族 log-odds 合计封顶。
    numeric_cap_z: f32,
}

impl LrModel {
    /// 由截距/权重派生基线概率与两族封顶（构造收口，保证任何来源的模型口径一致）。
    fn finish(mut self) -> Self {
        self.base_p = sigmoid(self.intercept);
        let sum = |r: std::ops::Range<usize>| -> f32 { r.map(|i| self.weights[i].max(0.0)).sum() };
        self.forensic_cap_z = sum(FORENSIC_COLS) * (FORENSIC_CAP / FORENSIC_EMP_TOTAL);
        self.numeric_cap_z = sum(NUMERIC_COLS) * (NUMERIC_CAP / NUMERIC_EMP_TOTAL);
        self
    }
    /// z → 校准后证据强度（零证据恒为 0.0，避免截距抬底分）。
    fn strength(&self, z: f32) -> f32 {
        ((sigmoid(z) - self.base_p) / (1.0 - self.base_p)).clamp(0.0, 1.0)
    }
    /// 特征向量 → log-odds（两族封顶在此生效，是 LR 之外保留的显式产品纪律）。
    fn z_of(&self, x: &[f32; FEATURE_COUNT]) -> f32 {
        let mut z = self.intercept;
        let (mut z_forensic, mut z_numeric) = (0.0f32, 0.0f32);
        for (i, xi) in x.iter().enumerate() {
            let w = self.weights[i] * xi;
            if FORENSIC_COLS.contains(&i) {
                z_forensic += w;
            } else if NUMERIC_COLS.contains(&i) {
                z_numeric += w;
            } else {
                z += w;
            }
        }
        z + z_forensic.min(self.forensic_cap_z) + z_numeric.min(self.numeric_cap_z)
    }
    /// log-odds → 证据强度（拟合侧据此把 v1 分级线换算到本模型的 score 尺度）。
    #[cfg(any(test, feature = "dev-tools"))]
    pub fn strength_at(&self, z: f32) -> f32 {
        self.strength(z)
    }
    /// v1 加权和尺度上的分级线 s，在本模型 score 尺度上的等效位置。证据量按经验尺度
    /// PRIOR_SLOPE 换算（与权重先验同尺度）：语料未改动的信号（死列保留先验）其分级行为
    /// 与 v1 逐例一致，被语料改动的信号则按其新权重自然移动——这是「分级语义变更可解释」
    /// （§1.5-5）的落地：变的是权重，不是评级尺子。
    pub fn v1_line_equivalent(&self, s: f32) -> f32 {
        self.strength(self.intercept + PRIOR_SLOPE * s)
    }
    /// 融合求值：(原始概率 σ(z)，重基后的证据强度)。assess_with 与拟合侧共用同一通道。
    pub fn evaluate(&self, x: &[f32; FEATURE_COUNT]) -> (f32, f32) {
        let z = self.z_of(x);
        (sigmoid(z), self.strength(z))
    }
    /// 证据强度 → 围标等级（不含条件化 floor —— floor 是 LR 之外的显式规则，由 assess_with 施加）。
    pub fn level_of(&self, score: f32) -> &'static str {
        if score >= self.level_high - LEVEL_EPS {
            "high"
        } else if score >= self.level_medium - LEVEL_EPS {
            "medium"
        } else if score > self.level_low {
            "low"
        } else {
            "none"
        }
    }
    /// 由拟合结果直接构造（仅拟合侧使用；运行时一律经 parse_lr_model 的符号/量级审查进入）。
    #[cfg(any(test, feature = "dev-tools"))]
    pub fn from_parts(
        calibration_kind: &str,
        version: &str,
        intercept: f32,
        weights: [f32; FEATURE_COUNT],
        levels: (f32, f32, f32),
    ) -> Self {
        LrModel {
            calibration_kind: calibration_kind.into(),
            version: version.into(),
            intercept,
            weights,
            level_high: levels.0,
            level_medium: levels.1,
            level_low: levels.2,
            base_p: 0.0,
            forensic_cap_z: 0.0,
            numeric_cap_z: 0.0,
        }
        .finish()
    }
}

/// v1 经验权重先验模型 = 回退路径。分级线取 v1 三条线在同一单调仿射映射下的像，因此本模型
/// 的分级判定与 v1 逐例等价（拟合时以它为 L2 收缩中心）。
pub fn empirical_prior() -> LrModel {
    let mut weights = [0f32; FEATURE_COUNT];
    for (i, w) in weights.iter_mut().enumerate() {
        *w = PRIOR_SLOPE * EMPIRICAL_WEIGHTS[i];
    }
    let seed = LrModel {
        calibration_kind: CALIBRATION_EMPIRICAL.into(),
        version: "v1-empirical".into(),
        intercept: -PRIOR_SLOPE * PRIOR_REF,
        weights,
        level_high: 0.0,
        level_medium: 0.0,
        level_low: 0.0,
        base_p: 0.0,
        forensic_cap_z: 0.0,
        numeric_cap_z: 0.0,
    }
    .finish();
    LrModel {
        level_high: seed.v1_line_equivalent(LEVEL_HIGH),
        level_medium: seed.v1_line_equivalent(LEVEL_MEDIUM),
        level_low: seed.v1_line_equivalent(LEVEL_LOW),
        ..seed
    }
}

/// 权重文件的磁盘形状。metrics/corpusHash/fittedAt 等台账字段运行时不读（只供评审与追溯）。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrFile {
    calibration_kind: String,
    version: String,
    intercept: f32,
    weights: std::collections::BTreeMap<String, f32>,
    levels: LrLevels,
}

#[derive(serde::Deserialize)]
struct LrLevels {
    high: f32,
    medium: f32,
    low: f32,
}

/// 解析 + 符号/量级审查（§1.5-4）。任一项不过即 Err，调用方回退经验先验并 log::warn。
/// 抽成纯函数便于单测「缺失/损坏 → fallback」而无需篡改磁盘文件。
pub fn parse_lr_model(raw: &str) -> Result<LrModel, String> {
    let f: LrFile = serde_json::from_str(raw).map_err(|e| format!("JSON 解析失败：{e}"))?;
    if f.calibration_kind.trim().is_empty() {
        return Err("缺少 calibrationKind 标签".into());
    }
    if !f.intercept.is_finite() || f.intercept >= 0.0 || f.intercept < INTERCEPT_MIN {
        return Err(format!("截距 {} 越界（须为负且 ≥{INTERCEPT_MIN}：零证据不得抬底分）", f.intercept));
    }
    if f.weights.len() != FEATURE_COUNT {
        return Err(format!("权重列数 {} ≠ 特征数 {FEATURE_COUNT}（特征集漂移）", f.weights.len()));
    }
    let mut weights = [0f32; FEATURE_COUNT];
    for (i, kind) in FEATURE_KINDS.iter().enumerate() {
        let w = *f.weights.get(*kind).ok_or_else(|| format!("缺少特征列 {kind}"))?;
        if !w.is_finite() || w.abs() > WEIGHT_ABS_MAX {
            return Err(format!("特征 {kind} 权重 {w} 量级异常（上限 {WEIGHT_ABS_MAX}）"));
        }
        if w < -WEIGHT_SIGN_EPS {
            return Err(format!("特征 {kind} 权重为负（{w}）：负权重在监管场景解释不通"));
        }
        weights[i] = w.max(0.0);
    }
    let (hi, mid, lo) = (f.levels.high, f.levels.medium, f.levels.low);
    let sane = [hi, mid, lo].iter().all(|v| v.is_finite() && *v > 0.0 && *v < 1.0);
    if !(sane && lo < mid && mid < hi) {
        return Err(format!("分级线越界或非单调：low={lo} medium={mid} high={hi}"));
    }
    Ok(LrModel {
        calibration_kind: f.calibration_kind,
        version: f.version,
        intercept: f.intercept,
        weights,
        level_high: hi,
        level_medium: mid,
        level_low: lo,
        base_p: 0.0,
        forensic_cap_z: 0.0,
        numeric_cap_z: 0.0,
    }
    .finish())
}

/// 随包固化的权重文件（不可运行时热换：保证同一安装包对同一输入恒定产出，结果可举证）。
const LR_WEIGHTS_JSON: &str = include_str!("../../fixtures/calibration/collusion_lr.json");

/// 生效模型：权重文件可用且过审查 → 拟合权重；否则 v1 经验先验 + 一次性 warn。
pub fn active_model() -> &'static LrModel {
    static MODEL: std::sync::OnceLock<LrModel> = std::sync::OnceLock::new();
    MODEL.get_or_init(|| match parse_lr_model(LR_WEIGHTS_JSON) {
        Ok(m) => m,
        Err(e) => {
            log::warn!(
                "围标融合权重不可用（{e}），已回退 v1 经验权重（calibrationKind={CALIBRATION_EMPIRICAL}）；\
                 比对照常进行，分级语义与 v1 一致"
            );
            empirical_prior()
        }
    })
}

/// 跨模块单测的期望权重口径：生效模型下某信号在连续特征 x 处的 log-odds 贡献
/// （M7 起 CollusionSignal.weight 的语义）。让集成测试断言【特征口径】而不是拟合出的具体
/// 数值——换一版权重文件时，端到端的特征口径回归仍然被钉死。
#[cfg(test)]
pub fn expected_contribution(kind: &str, x: f32) -> f32 {
    let i = FEATURE_KINDS
        .iter()
        .position(|k| *k == kind)
        .unwrap_or_else(|| panic!("未知特征列 {kind}"));
    active_model().weights[i] * x
}

/// 一条待融合的信号：col = 特征列，x = 连续特征值 ∈[0,1]，detail = 呈现文案。
struct Draft {
    col: usize,
    detail: String,
    x: f32,
}

/// 提取全量连续特征 + 呈现文案（融合与特征化解耦：拟合侧复用同一函数取特征向量，
/// 保证「拟合口径 == 生产口径」）。返回值第二项为硬命中标记（供条件化 floor）。
fn feature_drafts(inputs: &CollusionInputs) -> (Vec<Draft>, bool) {
    let &CollusionInputs {
        peak,
        clusters,
        docs,
        shared_terms,
        price_pairs,
        rsid_hits,
        lineage_hits,
        image_hits,
        shared_errors,
        evasion,
        tender_exemption_active: _,
        numeric,
    } = inputs;
    let mut drafts: Vec<Draft> = Vec::new();

    // 1) 文本相似度峰值：x = (peak − SIM_FLOOR)/(1 − SIM_FLOOR) clamp
    if peak >= SIM_FLOOR {
        let x = ((peak - SIM_FLOOR) / (1.0 - SIM_FLOOR)).clamp(0.0, 1.0);
        drafts.push(Draft {
            col: F_SIMILARITY,
            detail: format!("两份标书整体相似度峰值 {:.0}%", peak * 100.0),
            x,
        });
    }

    // 2) 跨文档雷同条款（3 份及以上的聚类是强信号）。k-共现查证（W3-3）：豁免簇（引用招标/
    //    行业范本，合法共享）与异常簇（归入独立 multiDocAnomaly 信号）均退出本信号计数——
    //    scoring_clusters 只含未豁免、未升级异常的普通雷同簇。无招标文件时二者恒 false，口径不变。
    //    连续特征：x = (CLUSTER_BASE + CLUSTER_SCALE·min(multi/5,1)) / 满档，即「有雷同条款」
    //    起 0.25 档、雷同簇数 5 处封顶到 1。
    let cluster_full = CLUSTER_BASE + CLUSTER_SCALE;
    let scoring_clusters: Vec<&Cluster> =
        clusters.iter().filter(|c| !c.exempted && !c.anomaly).collect();
    let multi = scoring_clusters.iter().filter(|c| c.docs.len() >= CLUSTER_MULTI_DOCS).count();
    if multi > 0 {
        let raw = CLUSTER_BASE + CLUSTER_SCALE * (multi as f32 / CLUSTER_SCALE_CAP).clamp(0.0, 1.0);
        drafts.push(Draft {
            col: F_CLUSTER,
            detail: format!("{multi} 处条款在 {CLUSTER_MULTI_DOCS} 份及以上标书间高度雷同"),
            x: raw / cluster_full,
        });
    } else if !scoring_clusters.is_empty() {
        drafts.push(Draft {
            col: F_CLUSTER,
            detail: format!("{} 处跨标书雷同条款", scoring_clusters.len()),
            x: CLUSTER_BASE / cluster_full,
        });
    }

    // 2.5) 多家异常一致（W3-3，连续特征）：≥3 家共有且两库皆查不到出处、查证质量闸门已通过的
    //      簇归入此独立信号（不计入信号②，不自动 high）。§1.5：强制「涉嫌」措辞 + 法条 +「需评标
    //      委员会依法认定」脚注；单信号不定性。查证质量闸门未过时簇不带 anomaly 标记，不入此信号。
    let anomaly_count = clusters.iter().filter(|c| c.anomaly).count();
    if anomaly_count > 0 {
        let x = (anomaly_count as f32 / MULTI_ANOMALY_SATURATION).clamp(0.0, 1.0);
        drafts.push(Draft {
            col: F_MULTI_ANOMALY,
            detail: format!(
                "{anomaly_count} 处段落在 {CLUSTER_MULTI_DOCS} 份及以上投标间高度雷同，招标文件与行业\
                 范本库均未查得出处，涉嫌《招标投标法实施条例》第四十条『投标文件异常一致』情形；\
                 此为线索级提示、非定性结论，未自动判为高风险，需评标委员会依法认定，未命中不代表清白"
            ),
            x,
        });
    }

    // 3) 元数据同源：只认强命中类别的风险标记（rsid 有独立信号、弱标记不计权），
    //    detail 枚举具体命中项而非笼统一句。连续特征（M7 补齐）：x = 同源份数占参评份数的
    //    比例——「5 份里 2 份作者相同」的证明力弱于「3 份全部作者相同」，v1 的 ≥2 份即满档
    //    在多份场景下高估；起算门槛 META_MIN_DOCS 保留。
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
        drafts.push(Draft {
            col: F_METADATA,
            detail: format!(
                "多份文档元数据同源：{}；元数据可被编辑清除，未命中不代表清白",
                cats.join("、")
            ),
            x: (meta as f32 / docs.len().max(1) as f32).clamp(0.0, 1.0),
        });
    }

    // 4) 共有特征词 / 疑似共用笔误。连续特征（M7 补齐）：x = min(n/SHARED_TERMS_SATURATION, 1)
    //    ——共用 5 个与共用 50 个罕见词的证明力不该同档；起算门槛 SHARED_TERMS_MIN 保留。
    if shared_terms.len() >= SHARED_TERMS_MIN {
        drafts.push(Draft {
            col: F_SHARED_TERMS,
            detail: format!("{} 个罕见特征词被多份标书共用", shared_terms.len()),
            x: (shared_terms.len() as f32 / SHARED_TERMS_SATURATION).min(1.0),
        });
    }

    // 5) 报价梯度雷同（【M6 起为回落信号】）：金额仅差几个百分点 + 多处条款雷同，是典型的围标
    // 陪标特征。仅当拿不到任何报价清单数据（numeric=None）时才计权——有 BOQ 时由数值层的逐项
    // 雷同率/共享算术错误取代（证明力更高且不会被业绩金额劫持）。多对接近时全部列出（最多 3 对），
    // 权重记一次（同一类证据不叠加）。
    if numeric.is_none() && !price_pairs.is_empty() {
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
        // 连续特征（M7 补齐）：x = (PRICE_GAP_MAX − gap)/PRICE_GAP_MAX，多对取最接近的一对
        //（差 0.1% 与差 2.9% 的「陪标价」形态强弱不同；调用方已按 gap<3% 过滤）。
        let x = price_pairs
            .iter()
            .map(|p| ((PRICE_GAP_MAX - p.gap_pct) / PRICE_GAP_MAX).clamp(0.0, 1.0))
            .fold(0.0f32, f32::max);
        drafts.push(Draft {
            col: F_FACTS,
            detail: format!("报价梯度雷同：{}，且相关文档多处条款雷同", shown.join("；")),
            x,
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
        drafts.push(Draft {
            col: F_RSID,
            detail: format!(
                "docx 修订标识（rsid）交集：{}。注意：同一母版可能为招标方提供的统一模板；\
                 rsid 另存为即可清除，未命中不代表清白",
                shown.join("；")
            ),
            x,
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
        drafts.push(Draft {
            col: F_PDF_LINEAGE,
            detail: format!(
                "PDF 血缘同源：{}。注意：同一母文件亦可能来自招标方统一模板或同一\
                 代理/打印机构，请评标人核实；元数据可被抹除，未命中不代表清白",
                shown.join("；")
            ),
            x,
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
        drafts.push(Draft {
            col: F_IMAGE_REUSE,
            detail: format!(
                "内嵌图片同源：{}。请核对该图是否来自招标文件统一提供\
                 （效果图/区位图各家照贴属合规）；未命中不代表清白",
                shown.join("；")
            ),
            x,
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
        drafts.push(Draft {
            col: F_SHARED_ERRORS,
            detail: format!(
                "多份标书疑似共用错误（词典外词/异常标点/错误引用）：{}。\
                 疑似错误仅供人工核对、未必构成串标；招标文件原生笔误各家照抄应予豁免，\
                 未命中不代表清白",
                shown.join("；")
            ),
            x,
        });
    }

    // 10) 检测到疑似规避特征（M2 入口对抗层）：独立信号，【在 FORENSIC_CAP 之外】直接并入
    //     score。连续特征 x = 任一文档 confirmed ? 1.0 : 仅 suspect ? 0.5 : 0（同类证据不叠加）。
    //     §1.5：命中是线索级结论——detail 措辞「检测到疑似规避特征，请人工复核」，绝不下
    //     「规避/串通」定性；单信号权重不达 high 线是有意设计（单证据不定罪）。
    let any_confirmed = evasion.iter().flatten().any(EvasionSummary::is_confirmed);
    let any_suspect = evasion.iter().flatten().any(EvasionSummary::is_suspect);
    if any_confirmed || any_suspect {
        let x = if any_confirmed { 1.0 } else { 0.5 };
        // 天干标签列出命中文档 + 各自证据种类（与 docs 同下标）
        let shown: Vec<String> = evasion
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let e = e.as_ref()?;
                if !e.is_flagged() {
                    return None;
                }
                Some(format!("「{}」{}", stem(i), e.evidence_kinds().join("/")))
            })
            .take(EVASION_SHOW_MAX)
            .collect();
        drafts.push(Draft {
            col: F_EVASION,
            detail: format!(
                "检测到疑似规避特征，请人工复核：{}。规避特征（零宽注入/同形字/字体重映射/\
                 PDF 隐藏文字层）可被清除，未命中不代表清白，请结合原文人工判断",
                shown.join("；")
            ),
            x,
        });
    }

    // 11) 商务标数值层（M6 / W5-6）：四类数值证据 + 后置的机制反事实。合计【独立封顶】
    //     NUMERIC_CAP 后并入 score（见常量注释：与文本信号存在结构性双重计数）。
    //     §1.5：每条 detail 都强制携带口径说明/线索定位/人工核对提示，呈现层丢不掉。
    if let Some(nm) = numeric {
        // 11.1 共享算术错误（最强单证据，但单条降档）：x = 单条降档比 / 满档
        if nm.shared_arith_error_count > 0 {
            let independent = nm.shared_arith_error_count >= NUMERIC_ARITH_ERROR_MIN_INDEPENDENT;
            let x = if independent {
                1.0
            } else {
                NUMERIC_ARITH_ERROR_SINGLE_WEIGHT / NUMERIC_ARITH_ERROR_WEIGHT
            };
            let pairs_txt: Vec<String> = nm
                .shared_arith_error_pairs
                .iter()
                .take(NUMERIC_ARITH_SHOW_MAX)
                .map(|(a, b)| format!("「{}」「{}」", stem(*a), stem(*b)))
                .collect();
            let grade = if independent {
                format!(
                    "共 {} 处【相互独立】的清单项（不同清单项）出现共享算术错误",
                    nm.shared_arith_error_count
                )
            } else {
                "仅 1 处清单项出现共享算术错误，已按单条降档计权（单条可能源自同款计价软件的舍入惯例）"
                    .to_string()
            };
            drafts.push(Draft {
                col: F_NUMERIC_ARITH,
                detail: format!(
                    "报价清单共享算术错误：{}{}——同一清单项的工程量、单价与（算错的）合价三者到分全等。{}；\
                     未命中不代表清白",
                    grade,
                    if pairs_txt.is_empty() {
                        String::new()
                    } else {
                        format!("（{}）", pairs_txt.join("、"))
                    },
                    NUMERIC_ARITH_NOTE
                ),
                x,
            });
        }

        // 11.2 逐项单价雷同率（达告警线才计；线性至满档）：x = 该斜坡贡献 / 满档
        //      （达线即起 BASE/MAX≈0.67 档，rate=1.0 到满档 1）
        if let Some(rate) = nm.max_identical_rate {
            let line = nm.identical_alarm_line;
            if rate >= line && line < 1.0 {
                let ramp = ((rate - line) / (1.0 - line)).clamp(0.0, 1.0);
                let raw = NUMERIC_IDENTICAL_BASE + (NUMERIC_IDENTICAL_MAX - NUMERIC_IDENTICAL_BASE) * ramp;
                let who = match nm.max_identical_pair {
                    Some((a, b)) => format!("「{}」「{}」", stem(a), stem(b)),
                    None => "参评标书".to_string(),
                };
                drafts.push(Draft {
                    col: F_NUMERIC_IDENTICAL,
                    detail: format!(
                        "{}逐项单价雷同率 {:.0}%，已达本次告警线 {:.0}%。{}",
                        who,
                        rate * 100.0,
                        line * 100.0,
                        NUMERIC_IDENTICAL_NOTE
                    ),
                    x: raw / NUMERIC_IDENTICAL_MAX,
                });
            }
        }

        // 11.3 规律性差异（等差/等比/仿射）——线索级
        if let Some(kind) = nm.regularity_kind.as_deref() {
            let who = match nm.regularity_pair {
                Some((a, b)) => format!("「{}」「{}」", stem(a), stem(b)),
                None => "参评标书".to_string(),
            };
            let coeff = match (kind, nm.regularity_coeff) {
                ("geo_discount", Some(a)) => format!("（系数 {a:.4}）"),
                ("arith_seq", Some(b)) => format!("（恒定差额 {b:.2} 元）"),
                _ => String::new(),
            };
            drafts.push(Draft {
                col: F_NUMERIC_PATTERN,
                detail: format!(
                    "{}的清单单价呈{}规律{}：逐项差异高度可由单一公式解释。{}",
                    who,
                    pattern_kind_cn(kind),
                    coeff,
                    NUMERIC_PATTERN_NOTE
                ),
                x: 1.0,
            });
        }

        // 11.4 相关性（双条件：r>0.99 且比值 CV<0.5%——构造侧已过滤，此处再防御一次）
        if let Some(r) = nm
            .max_pearson_with_low_ratio_cv
            .filter(|r| *r as f64 > NUMERIC_CORRELATION_R_MIN)
        {
            let who = match nm.correlation_pair {
                Some((a, b)) => format!("「{}」「{}」", stem(a), stem(b)),
                None => "参评标书".to_string(),
            };
            let cv = match nm.correlation_ratio_cv {
                Some(cv) => format!("、比值变异系数 {:.2}%", cv * 100.0),
                None => String::new(),
            };
            drafts.push(Draft {
                col: F_NUMERIC_CORRELATION,
                detail: format!(
                    "{}的清单单价向量相关系数 r={:.4}{}，同时满足强证据双条件。{}",
                    who, r, cv, NUMERIC_CORRELATION_NOTE
                ),
                x: 1.0,
            });
        }

        // 11.5 机制感知反事实（W5-5 后置二期）：flip_prob 缺席 ⇒ 本信号不出。
        if let Some(p) = nm.mechanism_flip_prob.filter(|p| *p >= NUMERIC_MECHANISM_FLIP_MIN) {
            drafts.push(Draft {
                col: F_NUMERIC_MECHANISM,
                detail: format!(
                    "评标机制反事实：剔除嫌疑组后，中标结果在 {:.0}% 的系数取值下发生改变。\
                     此为反事实解释性证据，不替代评标人判断，最终认定权属评标委员会",
                    p * 100.0
                ),
                x: 1.0,
            });
        }
    }

    // 硬命中（rsid rsidRoot 相同 / PDF 血缘硬档）：条件化 floor 的触发前提，随特征一并返回。
    let hard_hit =
        rsid_valid.iter().any(|h| h.root_match) || lineage_valid.iter().any(|h| h.is_hard());
    (drafts, hard_hit)
}

/// 全量连续特征向量（与 FEATURE_KINDS 同序，未触发信号为 0）。拟合侧（corpusgen
/// fit-collusion）用它取训练特征，保证「拟合口径 == 生产口径」；发布二进制不需要。
#[cfg(any(test, feature = "dev-tools"))]
pub fn feature_vector(inputs: &CollusionInputs) -> [f32; FEATURE_COUNT] {
    let (drafts, _) = feature_drafts(inputs);
    let mut v = [0f32; FEATURE_COUNT];
    for d in &drafts {
        v[d.col] = d.x;
    }
    v
}

pub fn assess_with(inputs: CollusionInputs) -> Collusion {
    let tender_exemption_active = inputs.tender_exemption_active;
    let m = active_model();
    let (drafts, hard_hit) = feature_drafts(&inputs);

    // log-LR 融合：z = b + Σ w_i·x_i。取证族与数值族的合计【分别封顶】后并入（LR 之外保留的
    // 显式产品纪律，见 FORENSIC_CAP/NUMERIC_CAP）；各信号 weight 仍是未封顶的原始 log-odds
    // 贡献，证明力呈现不受封顶影响。
    let mut x = [0f32; FEATURE_COUNT];
    let mut signals: Vec<CollusionSignal> = Vec::with_capacity(drafts.len() + 1);
    for d in drafts {
        x[d.col] = d.x;
        signals.push(CollusionSignal {
            kind: FEATURE_KINDS[d.col].into(),
            detail: d.detail,
            weight: m.weights[d.col] * d.x,
        });
    }
    // 按 log-odds 贡献降序（信号分解按贡献排序；稳定排序 + 固定提取顺序 ⇒ 同输入逐字节一致）
    signals.sort_by(|a, b| b.weight.partial_cmp(&a.weight).unwrap_or(std::cmp::Ordering::Equal));
    let (p, score) = m.evaluate(&x);
    let mut level = m.level_of(score);

    // 条件化硬命中 floor（§1.5 铁律，见 HARD_HIT_FLOOR_LEVEL/DETAIL）：硬命中 = rsid rsidRoot
    // 相同 或 PDF 血缘硬档（同一母文件 GUID/trailer-ID）。仅在豁免对减已生效时才置下限——
    // 招标模板产生的共享 rsid/图片/笔误已在信号提取侧对减，能走到这里的硬命中是扣除模板后仍存
    // 的同源证据。豁免不可用（tender_exemption_active=false）时不改等级、不加 floor 信号，硬命中
    // 仅由 rsid/pdfLineage 信号呈现。floor 是等级下限 max(level, medium)：只上抬 none/low 到 medium
    // （不直接 high），已 medium/high 不改；forensicFloor 信号在此前提下必出，承载纪律文案。
    if tender_exemption_active && hard_hit {
        if matches!(level, "none" | "low") {
            level = HARD_HIT_FLOOR_LEVEL;
        }
        signals.push(CollusionSignal {
            kind: "forensicFloor".into(),
            detail: HARD_HIT_FLOOR_DETAIL.into(),
            weight: 0.0,
        });
    }

    Collusion {
        level: level.into(),
        score,
        signals,
        calibration_kind: m.calibration_kind.clone(),
        calibration_version: m.version.clone(),
        probability: Some(p),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::report::{Fingerprint, SEVERITY_CONFIRMED, SEVERITY_NONE, SEVERITY_SUSPECT};

    fn cluster(docs: Vec<usize>) -> Cluster {
        Cluster { avg_score: 0.9, peak: 0.9, docs, segments: vec![], exempted: false, anomaly: false }
    }
    fn anomaly_cluster(docs: Vec<usize>) -> Cluster {
        Cluster { avg_score: 0.9, peak: 0.9, docs, segments: vec![], exempted: false, anomaly: true }
    }
    fn exempt_cluster(docs: Vec<usize>) -> Cluster {
        Cluster { avg_score: 0.9, peak: 0.9, docs, segments: vec![], exempted: true, anomaly: false }
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
            evasion: None,
        }
    }
    /// 造一个指定严重级 + 证据种类的 EvasionSummary（severity 直接设定，绕过判级——
    /// 本模块只消费 severity/evidence_kinds，判级逻辑由 report::EvasionSummary 单测覆盖）。
    fn ev(severity: &str) -> EvasionSummary {
        EvasionSummary {
            zero_width: 12,
            confusable_folds: 1,
            severity: severity.into(),
            ..Default::default()
        }
    }
    fn weight_of(c: &Collusion, kind: &str) -> Option<f32> {
        c.signals.iter().find(|s| s.kind == kind).map(|s| s.weight)
    }
    /// 特征列下标（按 kind 查表）。
    fn col(kind: &str) -> usize {
        FEATURE_KINDS.iter().position(|k| *k == kind).unwrap_or_else(|| panic!("未知特征列 {kind}"))
    }
    /// M7 新尺度下的期望信号权重 = 生效模型该列权重 × 连续特征 x。测试断言【特征口径】，
    /// 不把拟合出的具体数值硬编码——换一版权重文件时，特征口径的回归仍然被钉死。
    fn contrib(kind: &str, x: f32) -> f32 {
        active_model().weights[col(kind)] * x
    }
    /// 满档（x=1）贡献。
    fn full(kind: &str) -> f32 {
        contrib(kind, 1.0)
    }
    /// 一族证据叠满后【封顶生效】时的期望 score（封顶在 log-odds 尺度上按同比例作用）。
    fn capped_score(extra_z: f32, forensic: bool, numeric: bool) -> f32 {
        let m = active_model();
        let mut z = m.intercept + extra_z;
        if forensic {
            z += FORENSIC_COLS.map(|i| m.weights[i]).sum::<f32>() * (FORENSIC_CAP / FORENSIC_EMP_TOTAL);
        }
        if numeric {
            z += NUMERIC_COLS.map(|i| m.weights[i]).sum::<f32>() * (NUMERIC_CAP / NUMERIC_EMP_TOTAL);
        }
        m.strength_at(z)
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
        assert!((w - full("similarity")).abs() < 1e-6, "满峰值 → x=1 满档贡献");
        // v1 的 0.40 ≥ LEVEL_MEDIUM(0.35) 在新尺度上等价保留（分级线取 v1 等效位置）
        assert_eq!(c.level, "medium");
    }

    #[test]
    fn single_multi_doc_cluster_triggers_low() {
        // 一个横跨 3 份文档的雷同条款：x = (0.1 + 0.3×1/5)/0.4 = 0.4（v1 贡献 0.16 > 0.1 → low）
        let c = assess_with(CollusionInputs {
            clusters: &[cluster(vec![0, 1, 2])],
            ..Default::default()
        });
        let w = weight_of(&c, "cluster").expect("应有 cluster 信号");
        assert!((w - contrib("cluster", 0.4)).abs() < 1e-6, "实际 {w}");
        assert_eq!(c.level, "low");
    }

    #[test]
    fn two_doc_cluster_only_base_weight() {
        // 仅跨 2 份文档（<CLUSTER_MULTI_DOCS）：走 else 分支，x = CLUSTER_BASE/满档 = 0.25
        let c = assess_with(CollusionInputs {
            clusters: &[cluster(vec![0, 1])],
            ..Default::default()
        });
        let w = weight_of(&c, "cluster").expect("应有 cluster 信号");
        assert!((w - contrib("cluster", CLUSTER_BASE / (CLUSTER_BASE + CLUSTER_SCALE))).abs() < 1e-6);
    }

    // —— W3-3 k-共现过滤升级：豁免簇退出信号②、异常簇归入独立 multiDocAnomaly（不自动 high）——

    #[test]
    fn multi_doc_anomaly_emits_signal_and_never_auto_high() {
        // 「多家异常一致」簇 → multiDocAnomaly 信号（涉嫌措辞 + 法条 + 评标委员会），不自动 high，
        // 且退出信号②（无 cluster 信号）。
        let c = assess_with(CollusionInputs {
            clusters: &[anomaly_cluster(vec![0, 1, 2])],
            ..Default::default()
        });
        let s = c.signals.iter().find(|s| s.kind == "multiDocAnomaly").expect("应有 multiDocAnomaly 信号");
        assert!(s.weight > 0.0 && s.weight <= full("multiDocAnomaly"), "权重应在 (0, 满档]：{}", s.weight);
        assert!(s.detail.contains("涉嫌"), "detail 应含『涉嫌』措辞：{}", s.detail);
        assert!(s.detail.contains("第四十条"), "detail 应引《条例》第四十条");
        assert!(s.detail.contains("评标委员会"), "detail 应把最终认定权留给评标委员会");
        assert_ne!(c.level, "high", "多家异常一致不得自动判为 high（§1.5）");
        assert!(weight_of(&c, "cluster").is_none(), "异常簇不应计入信号②");
    }

    #[test]
    fn exempted_cluster_excluded_from_cluster_signal() {
        // 豁免簇（引用招标/行业范本，合法共享）退出信号②，也不进 multiDocAnomaly。
        let c = assess_with(CollusionInputs {
            clusters: &[exempt_cluster(vec![0, 1, 2])],
            ..Default::default()
        });
        assert!(weight_of(&c, "cluster").is_none(), "豁免簇不应计入信号②");
        assert!(weight_of(&c, "multiDocAnomaly").is_none(), "豁免簇不进异常信号");
        assert_eq!(c.level, "none");
    }

    #[test]
    fn multi_anomaly_weight_scales_and_saturates() {
        let w_of = |cl: &[Cluster]| {
            weight_of(&assess_with(CollusionInputs { clusters: cl, ..Default::default() }), "multiDocAnomaly")
        };
        let w1 = w_of(&[anomaly_cluster(vec![0, 1, 2])]).expect("1 处异常应有信号");
        assert!((w1 - contrib("multiDocAnomaly", 1.0 / 3.0)).abs() < 1e-6, "1 处 → x=1/3，实际 {w1}");
        let three = [
            anomaly_cluster(vec![0, 1, 2]),
            anomaly_cluster(vec![0, 1, 3]),
            anomaly_cluster(vec![0, 2, 3]),
        ];
        let w3 = w_of(&three).unwrap();
        assert!((w3 - full("multiDocAnomaly")).abs() < 1e-6, "3 处封顶满权重，实际 {w3}");
    }

    #[test]
    fn metadata_needs_min_docs() {
        // 2 份全部同源 → x = 2/2 = 1（满档）
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let c = assess_with(CollusionInputs { docs: &two, ..Default::default() });
        assert!((weight_of(&c, "metadata").unwrap() - full("metadata")).abs() < 1e-6);
        // 5 份里只有 2 份同源 → x = 0.4（连续特征：份数占比，M7 补齐）
        let mut mixed = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        mixed.extend((0..3).map(|_| doc(vec![])));
        let cm = assess_with(CollusionInputs { docs: &mixed, ..Default::default() });
        assert!((weight_of(&cm, "metadata").unwrap() - contrib("metadata", 0.4)).abs() < 1e-6);
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
        // 起算门槛 5 个 → x = 5/15；满档需 SHARED_TERMS_SATURATION 个（M7 连续特征化）
        let at = assess_with(CollusionInputs {
            shared_terms: &mk(SHARED_TERMS_MIN),
            ..Default::default()
        });
        let x = SHARED_TERMS_MIN as f32 / SHARED_TERMS_SATURATION;
        assert!((weight_of(&at, "sharedTerms").unwrap() - contrib("sharedTerms", x)).abs() < 1e-6);
        let sat = assess_with(CollusionInputs {
            shared_terms: &mk(SHARED_TERMS_SATURATION as usize + 5),
            ..Default::default()
        });
        assert!((weight_of(&sat, "sharedTerms").unwrap() - full("sharedTerms")).abs() < 1e-6, "超饱和不越界");
        let below = assess_with(CollusionInputs {
            shared_terms: &mk(SHARED_TERMS_MIN - 1),
            ..Default::default()
        });
        assert!(weight_of(&below, "sharedTerms").is_none());
    }

    #[test]
    fn price_proximity_signal() {
        // 差 2%（门槛 3%）→ x = (0.03−0.02)/0.03 = 1/3（M7 连续特征化：越接近越强）
        let pp = [PriceProximity { a: 0, b: 1, amount_a: 1_000_000, amount_b: 1_020_000, gap_pct: 0.02 }];
        let c = assess_with(CollusionInputs { price_pairs: &pp, ..Default::default() });
        let x = (PRICE_GAP_MAX - 0.02) / PRICE_GAP_MAX;
        assert!((weight_of(&c, "facts").unwrap() - contrib("facts", x)).abs() < 1e-5);
        // 几乎同价（差 0.1%）→ 接近满档
        let tight = [PriceProximity { a: 0, b: 1, amount_a: 1_000_000, amount_b: 1_001_000, gap_pct: 0.001 }];
        let ct = assess_with(CollusionInputs { price_pairs: &tight, ..Default::default() });
        assert!(weight_of(&ct, "facts").unwrap() > weight_of(&c, "facts").unwrap());
    }

    #[test]
    fn level_thresholds_high_medium_low_none() {
        // 峰值满分 + 元数据满档（v1 尺度 0.40+0.25 = 0.65 ≥ LEVEL_HIGH(0.6)）→ high
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
        assert!((s.weight - full("rsid")).abs() < 1e-6, "root_match → x=1 满权重");
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
        assert!((w3 - contrib("rsid", 0.3)).abs() < 1e-6, "shared=3 → x=0.3");
        let w10 = w_of(&[rsid_hit(10, false)]).unwrap();
        assert!((w10 - full("rsid")).abs() < 1e-6, "shared=10 饱和到满权重");
        let w25 = w_of(&[rsid_hit(25, false)]).unwrap();
        assert!((w25 - full("rsid")).abs() < 1e-6, "超饱和不越界");
        // 多对取最强：3 与 10 并存 → 仍是满权重一次，不叠加
        let multi = w_of(&[rsid_hit(3, false), rsid_hit(10, false)]).unwrap();
        assert!((multi - full("rsid")).abs() < 1e-6);
    }

    #[test]
    fn rsid_below_min_shared_without_root_yields_no_signal() {
        // 防御过滤：即便调用方传入未过滤的弱命中（shared<3 且非 root）也不产生信号
        let hits = [rsid_hit(2, false)];
        let c = assess_with(CollusionInputs { rsid_hits: &hits, ..Default::default() });
        assert!(weight_of(&c, "rsid").is_none());
        assert_eq!(c.level, "none");
    }

    // —— M4 豁免接线：条件化硬命中 floor（§1.5）——

    #[test]
    fn hard_hit_floor_active_forces_medium_and_emits_discipline_signal() {
        // 硬命中（rsidRoot 相同）+ 豁免对减已生效 → 等级下限 medium + forensicFloor 纪律信号。
        let hits = [rsid_hit(0, true)];
        let c = assess_with(CollusionInputs {
            rsid_hits: &hits,
            tender_exemption_active: true,
            ..Default::default()
        });
        assert!(matches!(c.level.as_str(), "medium" | "high"), "硬命中+豁免生效 → level ≥ medium");
        let s = c.signals.iter().find(|s| s.kind == "forensicFloor").expect("应有 forensicFloor 信号");
        assert!(s.detail.contains("已扣除招标文件统一下发模板"), "floor 文案应说明已扣除模板后仍硬命中");
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(s.detail.contains("评标委员会"), "应保留最终认定权归属");
    }

    #[test]
    fn hard_hit_floor_inactive_does_not_apply_no_discipline_signal() {
        // 招标文件不存在/豁免不可用 → 不加 forensicFloor 信号（floor 不启用），硬命中仅由 rsid 信号呈现。
        let hits = [rsid_hit(0, true)];
        let c = assess_with(CollusionInputs {
            rsid_hits: &hits,
            tender_exemption_active: false,
            ..Default::default()
        });
        assert!(
            c.signals.iter().all(|s| s.kind != "forensicFloor"),
            "豁免不可用时不应设等级下限（无 forensicFloor 信号）"
        );
        assert!(c.signals.iter().any(|s| s.kind == "rsid"), "硬命中仍作 rsid 信号展示");
    }

    #[test]
    fn pdf_lineage_hard_hit_also_triggers_conditional_floor() {
        // PDF 血缘硬档（同一母文件 GUID/trailer-ID）同为硬命中，豁免生效时同样置下限。
        let hits = [lineage_hit(true, &[])];
        let active = assess_with(CollusionInputs {
            lineage_hits: &hits,
            tender_exemption_active: true,
            ..Default::default()
        });
        assert!(active.signals.iter().any(|s| s.kind == "forensicFloor"));
        assert!(matches!(active.level.as_str(), "medium" | "high"));
        let inactive = assess_with(CollusionInputs {
            lineage_hits: &hits,
            tender_exemption_active: false,
            ..Default::default()
        });
        assert!(inactive.signals.iter().all(|s| s.kind != "forensicFloor"));
    }

    #[test]
    fn floor_not_triggered_without_hard_hit_even_when_active() {
        // 弱命中（PDF 血缘中档：仅共享字体子集标签，非硬档）不触发 floor，即便豁免已生效。
        let hits = [lineage_hit(false, &["ABCDEF+SimSun"])];
        let c = assess_with(CollusionInputs {
            lineage_hits: &hits,
            tender_exemption_active: true,
            ..Default::default()
        });
        assert!(
            c.signals.iter().all(|s| s.kind != "forensicFloor"),
            "非硬命中不触发条件化 floor"
        );
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
        assert!((s.weight - full("pdfLineage")).abs() < 1e-6, "硬命中 x=1 满权重");
        assert!(s.detail.contains("同一母文件"));
        assert!(s.detail.contains("元数据可被抹除"), "detail 应含免责语：{}", s.detail);
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(s.detail.contains("统一模板"), "detail 应把判定权留给评标人");
        assert_eq!(c.level, "medium", "满档取证单信号 = v1 0.35 线的等效位置 → medium");
    }

    #[test]
    fn pdf_lineage_mid_only_scales_down_and_takes_max_not_sum() {
        // 仅中命中（共享字体子集标签）→ x=PDF_LINEAGE_MID_X
        let mid = [lineage_hit(false, &["ABCDEF+SimSun"])];
        let c = assess_with(CollusionInputs { lineage_hits: &mid, ..Default::default() });
        let w = weight_of(&c, "pdfLineage").expect("中命中应有信号");
        assert!((w - contrib("pdfLineage", PDF_LINEAGE_MID_X)).abs() < 1e-6, "实际 {w}");
        // 中 + 硬并存：取最强一次，不叠加
        let both = [lineage_hit(false, &["ABCDEF+SimSun"]), lineage_hit(true, &[])];
        let cb = assess_with(CollusionInputs { lineage_hits: &both, ..Default::default() });
        let wb = weight_of(&cb, "pdfLineage").unwrap();
        assert!((wb - full("pdfLineage")).abs() < 1e-6, "多对取最强不叠加，实际 {wb}");
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
        assert!((weight_of(&c, "metadata").unwrap() - full("metadata")).abs() < 1e-6);
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
        // 两文档同一张图（sha256 相等）→ 1 对命中，x=1/3
        let per_doc = vec![
            vec![img("SAME", Some(0), Some(3))],
            vec![img("SAME", Some(0), Some(5))],
        ];
        let hits = image_pairs(&per_doc, &no_exempt());
        assert_eq!(hits.len(), 1);
        assert!(hits[0].exact, "sha256 相等应为精确命中");
        let c = assess_with(CollusionInputs { image_hits: &hits, ..Default::default() });
        let w = weight_of(&c, "imageReuse").expect("应有 imageReuse 信号");
        assert!((w - contrib("imageReuse", 1.0 / 3.0)).abs() < 1e-6, "1 对 → x=1/3，实际 {w}");
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
        assert!((w - full("imageReuse")).abs() < 1e-6, "3 对封顶满权重，实际 {w}");
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
        // 单条满稀有度错误：x = min(1.0/5, 1) = 0.2（连续特征，无 floor）
        let errs = [shared_error("施工枝术", 1.0, Some("的施工枝术方案"))];
        let c = assess_with(CollusionInputs { shared_errors: &errs, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "sharedErrors").expect("应有 sharedErrors 信号");
        assert!((s.weight - contrib("sharedErrors", 0.2)).abs() < 1e-6, "单条满稀有度 → x=0.2，实际 {}", s.weight);
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
        assert!((w - full("sharedErrors")).abs() < 1e-6, "5 条满稀有度饱和到满权重，实际 {w}");
        let seven: Vec<SharedTerm> = (0..7).map(|i| shared_error(&format!("错{i}"), 1.0, None)).collect();
        let c7 = assess_with(CollusionInputs { shared_errors: &seven, ..Default::default() });
        assert!((weight_of(&c7, "sharedErrors").unwrap() - full("sharedErrors")).abs() < 1e-6, "超饱和不越界");
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
        // 四类取证叠满（原始合计远超封顶）→ 对 score 的贡献按 FORENSIC_CAP/满档合计的比例封顶；
        // 无其它信号时 score 恰为该封顶位置的证据强度 → medium（非 high），与 v1 的 0.45 等价。
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
        // 各信号 detail 仍呈现原始（未封顶）log-odds 贡献
        assert!((weight_of(&c, "rsid").unwrap() - full("rsid")).abs() < 1e-6);
        assert!((weight_of(&c, "pdfLineage").unwrap() - full("pdfLineage")).abs() < 1e-6);
        assert!((weight_of(&c, "imageReuse").unwrap() - full("imageReuse")).abs() < 1e-6);
        assert!((weight_of(&c, "sharedErrors").unwrap() - full("sharedErrors")).abs() < 1e-6);
        let raw: f32 = ["rsid", "pdfLineage", "imageReuse", "sharedErrors"].iter().map(|k| full(k)).sum();
        // 取证部分封顶：总分恰为封顶位置，且显著低于「不封顶」的合计
        assert!((c.score - capped_score(0.0, true, false)).abs() < 1e-6, "取证应封顶，实际 {}", c.score);
        assert!(c.score < active_model().strength_at(active_model().intercept + raw), "封顶必须低于不封顶");
        assert_eq!(c.level, "medium", "四类取证叠满仍为 medium（封顶防直接 high）");
    }

    #[test]
    fn forensic_cap_does_not_touch_non_forensic_score() {
        // 非取证信号（相似度满档 + 元数据满档，v1 尺度 0.65）不受封顶影响，仍可达 high
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let c = assess_with(CollusionInputs { peak: 1.0, docs: &two, ..Default::default() });
        assert_eq!(c.level, "high", "非取证信号不封顶，实际 {:.2}", c.score);
    }

    // —— M2 规避：evasion 信号（x = confirmed?1.0 : suspect?0.5，权重 EVASION_WEIGHT）——

    #[test]
    fn evasion_confirmed_takes_full_weight() {
        // 任一文档 confirmed → x=1.0 → weight = 0.25
        let ev_docs = [None, Some(ev(SEVERITY_CONFIRMED)), Some(ev(SEVERITY_SUSPECT))];
        let c = assess_with(CollusionInputs { evasion: &ev_docs, ..Default::default() });
        let w = weight_of(&c, "evasion").expect("应有 evasion 信号");
        assert!((w - full("evasion")).abs() < 1e-6, "confirmed → x=1 满档，实际 {w}");
    }

    #[test]
    fn evasion_suspect_only_takes_half_weight() {
        // 仅 suspect（无 confirmed）→ x=0.5
        let ev_docs = [None, Some(ev(SEVERITY_SUSPECT))];
        let c = assess_with(CollusionInputs { evasion: &ev_docs, ..Default::default() });
        let w = weight_of(&c, "evasion").expect("应有 evasion 信号");
        assert!((w - contrib("evasion", 0.5)).abs() < 1e-6, "仅 suspect → 半档，实际 {w}");
    }

    #[test]
    fn evasion_none_yields_no_signal() {
        // 无 evasion 数据 / 均 severity none → 无信号
        let empty: [Option<EvasionSummary>; 0] = [];
        let c = assess_with(CollusionInputs { evasion: &empty, ..Default::default() });
        assert!(weight_of(&c, "evasion").is_none());
        let none_docs = [Some(ev(SEVERITY_NONE)), None];
        let c2 = assess_with(CollusionInputs { evasion: &none_docs, ..Default::default() });
        assert!(weight_of(&c2, "evasion").is_none(), "均未过判级线不产生信号");
        assert_eq!(c2.level, "none");
    }

    #[test]
    fn evasion_detail_carries_stems_evidence_and_disclaimer() {
        // detail 含天干标签 + 证据种类 + §1.5 线索级措辞 + 免责，且不含背书式表述
        let ev_docs = [Some(ev(SEVERITY_CONFIRMED)), None, Some(ev(SEVERITY_SUSPECT))];
        let c = assess_with(CollusionInputs { evasion: &ev_docs, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "evasion").expect("应有 evasion 信号");
        assert!(s.detail.contains("甲") && s.detail.contains("丙"), "detail 应含命中文档天干：{}", s.detail);
        assert!(!s.detail.contains("乙"), "未命中文档不列出：{}", s.detail);
        assert!(s.detail.contains("隐形码点") && s.detail.contains("同形字"), "detail 应含证据种类：{}", s.detail);
        assert!(s.detail.contains("检测到疑似规避特征，请人工复核"), "应含线索级措辞：{}", s.detail);
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(!s.detail.contains("检查通过") && !s.detail.contains("清白证明"), "不得输出背书式结论");
    }

    #[test]
    fn evasion_is_outside_forensic_cap() {
        // evasion 在 FORENSIC_CAP 之外：取证四类叠满(封顶) 之上再叠 evasion 满档（v1 尺度 0.45+0.25）。
        // 若 evasion 被错误并入取证封顶，则 score 只会停在取证封顶位置。
        let rsid = [rsid_hit(0, true)];
        let lineage = [lineage_hit(true, &[])];
        let images = [
            ImageHit { a: 0, b: 1, page_a: None, page_b: None, exact: true },
            ImageHit { a: 0, b: 2, page_a: None, page_b: None, exact: true },
            ImageHit { a: 1, b: 2, page_a: None, page_b: None, exact: true },
        ];
        let errs: Vec<SharedTerm> = (0..5).map(|i| shared_error(&format!("错{i}"), 1.0, None)).collect();
        let ev_docs = [Some(ev(SEVERITY_CONFIRMED))];
        let c = assess_with(CollusionInputs {
            rsid_hits: &rsid,
            lineage_hits: &lineage,
            image_hits: &images,
            shared_errors: &errs,
            evasion: &ev_docs,
            ..Default::default()
        });
        assert!(
            (c.score - capped_score(full("evasion"), true, false)).abs() < 1e-6,
            "取证封顶 + evasion 满档（evasion 不受封顶），实际 {}",
            c.score
        );
        assert!(c.score > capped_score(0.0, true, false), "evasion 必须在封顶之外另计");
        assert_eq!(c.level, "high", "等价于 v1 的 0.70 ≥ LEVEL_HIGH");
    }

    // —— M6 数值层信号（W5-6）——

    /// 造一份数值证据：默认告警线 0.80、其余字段缺席（缺席 ⇒ 该子信号不出）。
    fn numeric_ev() -> NumericEvidence {
        NumericEvidence { identical_alarm_line: 0.80, ..Default::default() }
    }

    #[test]
    fn numeric_single_arith_error_is_downgraded_not_full_weight() {
        // §1.5 审查修正：1 条共享算术错误可能是同款计价软件的舍入惯例 → 降档，绝不给 0.35。
        let nm = NumericEvidence {
            shared_arith_error_count: 1,
            shared_arith_error_pairs: vec![(0, 1)],
            ..numeric_ev()
        };
        let c = assess_with(CollusionInputs { numeric: Some(&nm), ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "numericArithError").expect("应有 numericArithError 信号");
        let x_single = NUMERIC_ARITH_ERROR_SINGLE_WEIGHT / NUMERIC_ARITH_ERROR_WEIGHT;
        assert!(
            (s.weight - contrib("numericArithError", x_single)).abs() < 1e-6,
            "单条应降档到 x={x_single}，实际 {}",
            s.weight
        );
        assert!(s.weight < full("numericArithError") - 1e-6, "单条不得给满档");
        assert!(s.detail.contains("降档"), "detail 应说明已降档：{}", s.detail);
        assert!(s.detail.contains("计价软件"), "detail 应附计价软件舍入惯例核对提示：{}", s.detail);
        assert!(s.detail.contains("招标文件"), "detail 应附招标文件来源核对提示");
        assert!(s.detail.contains("未命中不代表清白"));
    }

    #[test]
    fn numeric_two_independent_arith_errors_take_full_weight_and_reach_medium() {
        // ≥2 条相互独立（不同清单项）→ 满档 0.35 ≥ LEVEL_MEDIUM(0.35) → level ≥ medium。
        let nm = NumericEvidence {
            shared_arith_error_count: 2,
            shared_arith_error_pairs: vec![(0, 1)],
            ..numeric_ev()
        };
        let c = assess_with(CollusionInputs { numeric: Some(&nm), ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "numericArithError").expect("应有 numericArithError 信号");
        assert!((s.weight - full("numericArithError")).abs() < 1e-6, "≥2 条独立 → 满档，实际 {}", s.weight);
        assert!(s.detail.contains("相互独立"), "detail 应点明独立性口径：{}", s.detail);
        assert!(matches!(c.level.as_str(), "medium" | "high"), "满档（v1 0.35 线）→ level ≥ medium，实际 {}", c.level);
    }

    #[test]
    fn numeric_identical_rate_ramps_from_alarm_line_to_max() {
        let w_of = |rate: f32| {
            let nm = NumericEvidence {
                max_identical_rate: Some(rate),
                max_identical_pair: Some((0, 1)),
                ..numeric_ev()
            };
            let c = assess_with(CollusionInputs { numeric: Some(&nm), ..Default::default() });
            weight_of(&c, "numericIdentical")
        };
        assert!(w_of(0.79).is_none(), "未达告警线不出信号");
        let at = w_of(0.80).expect("刚好达线应出信号");
        let x_base = NUMERIC_IDENTICAL_BASE / NUMERIC_IDENTICAL_MAX;
        assert!((at - contrib("numericIdentical", x_base)).abs() < 1e-6, "达线取起档 x={x_base}，实际 {at}");
        let sat = w_of(1.0).unwrap();
        assert!((sat - full("numericIdentical")).abs() < 1e-6, "逐项全等取满档，实际 {sat}");
        let mid = w_of(0.90).unwrap();
        assert!((mid - contrib("numericIdentical", 0.25 / 0.30)).abs() < 1e-6, "0.90 → 0.25/0.30 档，实际 {mid}");
    }

    #[test]
    fn numeric_identical_detail_carries_local_criterion_wording() {
        // §1.5：必须写明「参照地方雷同认定口径，针对逐项单价相同率」，且不得表述为「认定串通」。
        let nm = NumericEvidence {
            max_identical_rate: Some(0.92),
            max_identical_pair: Some((0, 1)),
            ..numeric_ev()
        };
        let c = assess_with(CollusionInputs { numeric: Some(&nm), ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "numericIdentical").expect("应有 numericIdentical 信号");
        assert!(s.detail.contains("逐项单价相同率"), "detail 应写明口径：{}", s.detail);
        assert!(s.detail.contains("地方雷同认定口径"));
        assert!(s.detail.contains("不构成串通投标认定"), "§1.5 禁止越权定性：{}", s.detail);
        assert!(s.detail.contains("评标委员会"));
        assert!(s.detail.contains("甲") && s.detail.contains("乙"), "detail 应含天干文档对");
    }

    #[test]
    fn numeric_identical_with_shared_metadata_reaches_high() {
        // 验收 (2)：逐项单价雷同率 0.9 + 元数据同源 → high。
        // 0.9 雷同率意味着单价向量近乎重合，相关性双条件（r>0.99 且比值 CV≈0）必然同时成立——
        // 这两项是同一事实的两个刻度，构造证据时一并给出才是真实形态。
        // v1 尺度：0.25（雷同率）+ 0.10（相关性）+ 0.25（元数据）= 0.60 ≥ LEVEL_HIGH。
        let nm = NumericEvidence {
            max_identical_rate: Some(0.90),
            max_identical_pair: Some((0, 1)),
            max_pearson_with_low_ratio_cv: Some(0.9985),
            correlation_pair: Some((0, 1)),
            correlation_ratio_cv: Some(0.001),
            ..numeric_ev()
        };
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let c = assess_with(CollusionInputs {
            docs: &two,
            numeric: Some(&nm),
            ..Default::default()
        });
        assert_eq!(c.level, "high", "score={:.4} 应达 high", c.score);
    }

    #[test]
    fn numeric_correlation_requires_both_conditions() {
        // 只有 r>0.99 才计权：r=0.95（投标人单价天然同源的常态）不出信号。
        let weak = NumericEvidence {
            max_pearson_with_low_ratio_cv: Some(0.95),
            correlation_pair: Some((0, 1)),
            ..numeric_ev()
        };
        let c = assess_with(CollusionInputs { numeric: Some(&weak), ..Default::default() });
        assert!(weight_of(&c, "numericCorrelation").is_none(), "r≤0.99 不计权");
        let strong = NumericEvidence {
            max_pearson_with_low_ratio_cv: Some(0.995),
            correlation_pair: Some((0, 1)),
            correlation_ratio_cv: Some(0.002),
            ..numeric_ev()
        };
        let c2 = assess_with(CollusionInputs { numeric: Some(&strong), ..Default::default() });
        let s = c2.signals.iter().find(|s| s.kind == "numericCorrelation").expect("应有相关性信号");
        assert!((s.weight - full("numericCorrelation")).abs() < 1e-6);
        assert!(s.detail.contains("比值变异系数"), "detail 必须与比值 CV 同屏：{}", s.detail);
        assert!(s.detail.contains("r>0.99 且比值 CV≈0 才是强证据"), "detail 应写明强证据条件");
    }

    #[test]
    fn numeric_pattern_signal_is_clue_level_with_uniform_discount_note() {
        let nm = NumericEvidence {
            regularity_kind: Some("geo_discount".into()),
            regularity_pair: Some((0, 2)),
            regularity_coeff: Some(0.97),
            ..numeric_ev()
        };
        let c = assess_with(CollusionInputs { numeric: Some(&nm), ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "numericPattern").expect("应有 numericPattern 信号");
        assert!((s.weight - full("numericPattern")).abs() < 1e-6);
        assert!(s.detail.contains("等比"), "detail 应含形态中文标签：{}", s.detail);
        assert!(s.detail.contains("0.9700"), "detail 应含折扣系数：{}", s.detail);
        assert!(s.detail.contains("统一下浮"), "§1.5：必须附统一下浮提示：{}", s.detail);
        assert!(s.detail.contains("线索"), "§1.5：定位为线索而非认定");
        assert_ne!(c.level, "high", "单信号不得独自达 high 线");
    }

    #[test]
    fn numeric_mechanism_signal_absent_when_flip_prob_is_none() {
        // W5-5 后置二期：mechanism_flip_prob 缺席 ⇒ 该信号不出（不得凭空造分）。
        let nm = numeric_ev();
        let c = assess_with(CollusionInputs { numeric: Some(&nm), ..Default::default() });
        assert!(weight_of(&c, "numericMechanism").is_none());
        assert_eq!(c.level, "none");
        // 二期填入 flip_prob 后信号才出（接口预留有效性验证）
        let with = NumericEvidence { mechanism_flip_prob: Some(0.83), ..numeric_ev() };
        let c2 = assess_with(CollusionInputs { numeric: Some(&with), ..Default::default() });
        let s = c2.signals.iter().find(|s| s.kind == "numericMechanism").expect("flip_prob≥0.5 应出信号");
        assert!((s.weight - full("numericMechanism")).abs() < 1e-6);
        assert!(s.detail.contains("不替代评标人判断"));
    }

    #[test]
    fn numeric_cap_limits_combined_numeric_contribution() {
        // 验收 (3)：五类数值信号全满时对 score 的合计贡献按 NUMERIC_CAP 比例封顶，且 score ≤ 1。
        let nm = NumericEvidence {
            max_identical_rate: Some(1.0),
            max_identical_pair: Some((0, 1)),
            identical_alarm_line: 0.80,
            shared_arith_error_count: 4,
            shared_arith_error_pairs: vec![(0, 1)],
            regularity_kind: Some("geo_discount".into()),
            regularity_pair: Some((0, 1)),
            regularity_coeff: Some(0.95),
            max_pearson_with_low_ratio_cv: Some(0.999),
            correlation_pair: Some((0, 1)),
            correlation_ratio_cv: Some(0.0001),
            mechanism_flip_prob: Some(1.0),
        };
        let c = assess_with(CollusionInputs { numeric: Some(&nm), ..Default::default() });
        let raw: f32 = c
            .signals
            .iter()
            .filter(|s| s.kind.starts_with("numeric"))
            .map(|s| s.weight)
            .sum();
        let capped = capped_score(0.0, false, true);
        assert!(
            raw > active_model().strength_at(active_model().intercept + capped),
            "各信号 detail 应保留原始（未封顶）权重，合计 {raw}"
        );
        assert!((c.score - capped).abs() < 1e-6, "数值合计应封顶，实际 {}", c.score);
        assert!(c.score <= 1.0);
    }

    #[test]
    fn numeric_cap_is_independent_of_forensic_cap() {
        // 数值封顶与取证封顶各自独立：取证四类叠满 + 数值五类叠满（v1 尺度 0.45+0.45=0.90）。
        let rsid = [rsid_hit(0, true)];
        let lineage = [lineage_hit(true, &[])];
        let images = [
            ImageHit { a: 0, b: 1, page_a: None, page_b: None, exact: true },
            ImageHit { a: 0, b: 2, page_a: None, page_b: None, exact: true },
            ImageHit { a: 1, b: 2, page_a: None, page_b: None, exact: true },
        ];
        let errs: Vec<SharedTerm> = (0..5).map(|i| shared_error(&format!("错{i}"), 1.0, None)).collect();
        let nm = NumericEvidence {
            max_identical_rate: Some(1.0),
            max_identical_pair: Some((0, 1)),
            identical_alarm_line: 0.80,
            shared_arith_error_count: 3,
            shared_arith_error_pairs: vec![(0, 1)],
            regularity_kind: Some("arith_seq".into()),
            regularity_pair: Some((0, 1)),
            regularity_coeff: Some(500.0),
            max_pearson_with_low_ratio_cv: Some(0.999),
            correlation_pair: Some((0, 1)),
            correlation_ratio_cv: Some(0.0001),
            mechanism_flip_prob: Some(0.9),
        };
        let c = assess_with(CollusionInputs {
            rsid_hits: &rsid,
            lineage_hits: &lineage,
            image_hits: &images,
            shared_errors: &errs,
            numeric: Some(&nm),
            ..Default::default()
        });
        assert!(
            (c.score - capped_score(0.0, true, true)).abs() < 1e-6,
            "两个封顶各自独立叠加，实际 {}",
            c.score
        );
        assert!(c.score > capped_score(0.0, true, false), "数值族必须在取证封顶之外另计");
    }

    #[test]
    fn price_proximity_falls_back_only_without_boq_data() {
        // 验收 (4)：无 BOQ（numeric=None）→ 旧报价梯度信号照常触发；
        // 有 BOQ（哪怕数值证据全空）→ 旧信号退场，由数值层接管。
        let pp = [PriceProximity { a: 0, b: 1, amount_a: 1_000_000, amount_b: 1_020_000, gap_pct: 0.02 }];
        let without = assess_with(CollusionInputs { price_pairs: &pp, ..Default::default() });
        assert!(weight_of(&without, "facts").unwrap() > 0.0, "无 BOQ 时回落信号照常");
        let nm = numeric_ev();
        let with = assess_with(CollusionInputs {
            price_pairs: &pp,
            numeric: Some(&nm),
            ..Default::default()
        });
        assert!(weight_of(&with, "facts").is_none(), "有 BOQ 时旧报价梯度信号降级退场");
        assert_eq!(with.level, "none", "空数值证据不得凭空造分");
    }

    // —— M7 融合层：权重加载、符号审查、回退等价、可复现 ——

    #[test]
    fn shipped_lr_weights_load_and_pass_sign_review() {
        // 随包权重文件必须能加载（否则静默走回退，实验性校准形同虚设）且全部权重非负、截距为负。
        let m = active_model();
        assert_eq!(m.calibration_kind, CALIBRATION_EXPERIMENTAL, "随包权重应为拟合档，实际回退了");
        assert!(!m.version.is_empty(), "权重文件须带版本号（导出报告脚注要用）");
        for (i, k) in FEATURE_KINDS.iter().enumerate() {
            assert!(m.weights[i] >= 0.0, "特征 {k} 权重为负：{}（监管场景解释不通）", m.weights[i]);
            assert!(m.weights[i] <= WEIGHT_ABS_MAX, "特征 {k} 权重量级异常：{}", m.weights[i]);
        }
        assert!(m.intercept < 0.0, "截距必须为负：零证据不得抬底分");
        assert!(m.level_low < m.level_medium && m.level_medium < m.level_high, "分级线须单调");
    }

    #[test]
    fn corrupt_or_unreviewable_weights_are_rejected_so_runtime_falls_back() {
        // 验收③：文件缺失/损坏/未过符号审查 → parse 失败（调用方回退经验先验 + warn，比对不失败）。
        let good = serde_json::json!({
            "calibrationKind": "experimental-synthetic",
            "version": "t",
            "intercept": -4.0,
            "weights": FEATURE_KINDS
                .iter()
                .map(|k| ((*k).to_string(), serde_json::json!(1.0)))
                .collect::<serde_json::Map<String, serde_json::Value>>(),
            "levels": { "high": 0.6, "medium": 0.3, "low": 0.05 },
        });
        assert!(parse_lr_model(&good.to_string()).is_ok(), "合法文件应解析成功");
        assert!(parse_lr_model("").is_err(), "空文件（等价缺失）应拒绝");
        assert!(parse_lr_model("{ not json").is_err(), "损坏文件应拒绝");
        let mut neg = good.clone();
        neg["weights"]["rsid"] = serde_json::json!(-0.5);
        assert!(parse_lr_model(&neg.to_string()).is_err(), "负权重必须拒绝（§1.5-4 符号审查）");
        let mut huge = good.clone();
        huge["weights"]["rsid"] = serde_json::json!(999.0);
        assert!(parse_lr_model(&huge.to_string()).is_err(), "量级异常必须拒绝");
        let mut pos_b = good.clone();
        pos_b["intercept"] = serde_json::json!(0.5);
        assert!(parse_lr_model(&pos_b.to_string()).is_err(), "截距非负必须拒绝（会抬底分）");
        let mut missing = good.clone();
        missing["weights"].as_object_mut().unwrap().remove("evasion");
        assert!(parse_lr_model(&missing.to_string()).is_err(), "特征列缺失必须拒绝（特征集漂移）");
        let mut bad_levels = good.clone();
        bad_levels["levels"] = serde_json::json!({ "high": 0.2, "medium": 0.3, "low": 0.05 });
        assert!(parse_lr_model(&bad_levels.to_string()).is_err(), "分级线非单调必须拒绝");
    }

    #[test]
    fn empirical_fallback_reproduces_v1_levels() {
        // 回退路径（经验权重先验）的分级判定与 v1 逐例等价——回退不改变任何既有产品行为。
        let prior = empirical_prior();
        let level_of = |inputs: &CollusionInputs| -> &'static str {
            let x = feature_vector(inputs);
            prior.level_of(prior.evaluate(&x).1)
        };
        assert_eq!(level_of(&CollusionInputs::default()), "none", "零证据 → none");
        // 峰值满分（v1 0.40 ≥ 0.35）→ medium
        assert_eq!(level_of(&CollusionInputs { peak: 1.0, ..Default::default() }), "medium");
        // 1 处三份雷同簇（v1 0.16 > 0.1）→ low
        assert_eq!(
            level_of(&CollusionInputs { clusters: &[cluster(vec![0, 1, 2])], ..Default::default() }),
            "low"
        );
        // rsid 硬命中（v1 0.35 恰在 medium 线）→ medium
        let hits = [rsid_hit(0, true)];
        assert_eq!(level_of(&CollusionInputs { rsid_hits: &hits, ..Default::default() }), "medium");
        // 峰值满分 + 元数据同源（v1 0.65 ≥ 0.6）→ high
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        assert_eq!(
            level_of(&CollusionInputs { peak: 1.0, docs: &two, ..Default::default() }),
            "high"
        );
    }

    #[test]
    fn same_inputs_produce_byte_identical_collusion_json() {
        // 验收⑤：同输入两次比对的 collusion_json 逐字节一致（可复现承诺）。
        let two = vec![doc(vec!["作者相同"]), doc(vec!["模板相同"])];
        let errs: Vec<SharedTerm> = (0..3).map(|i| shared_error(&format!("错{i}"), 0.8, None)).collect();
        let hits = [rsid_hit(6, false)];
        let nm = NumericEvidence {
            max_identical_rate: Some(0.93),
            max_identical_pair: Some((0, 1)),
            identical_alarm_line: 0.80,
            ..Default::default()
        };
        let mk = || {
            assess_with(CollusionInputs {
                peak: 0.82,
                clusters: &[cluster(vec![0, 1, 2]), cluster(vec![0, 1])],
                docs: &two,
                shared_errors: &errs,
                rsid_hits: &hits,
                numeric: Some(&nm),
                ..Default::default()
            })
        };
        let a = serde_json::to_string(&mk()).unwrap();
        let b = serde_json::to_string(&mk()).unwrap();
        assert_eq!(a, b, "同输入两次求值必须逐字节一致");
        assert!(a.contains("calibrationKind"), "DTO 须带校准来源标签：{a}");
    }

    #[test]
    fn signals_are_sorted_by_log_odds_contribution() {
        // 信号分解按贡献降序（可解释性：先看最推分的证据）；forensicFloor（权重 0）殿后。
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let hits = [rsid_hit(0, true)];
        let c = assess_with(CollusionInputs {
            peak: 0.7,
            clusters: &[cluster(vec![0, 1])],
            docs: &two,
            rsid_hits: &hits,
            tender_exemption_active: true,
            ..Default::default()
        });
        let ws: Vec<f32> = c.signals.iter().map(|s| s.weight).collect();
        assert!(ws.windows(2).all(|w| w[0] >= w[1]), "信号未按贡献降序：{ws:?}");
        assert_eq!(c.signals.last().map(|s| s.kind.as_str()), Some("forensicFloor"));
    }

    #[test]
    fn feature_vector_is_all_zero_for_empty_inputs_and_bounded_otherwise() {
        // 拟合口径 == 生产口径：特征向量长度固定、全零输入全零、任一特征恒在 [0,1]。
        let empty = feature_vector(&CollusionInputs::default());
        assert_eq!(empty.len(), FEATURE_KINDS.len());
        assert!(empty.iter().all(|v| *v == 0.0), "零证据的特征向量必须全零");
        let two = vec![doc(vec!["作者相同"]), doc(vec!["作者相同"])];
        let hits = [rsid_hit(25, false)];
        let nm = NumericEvidence {
            max_identical_rate: Some(1.0),
            max_identical_pair: Some((0, 1)),
            identical_alarm_line: 0.80,
            shared_arith_error_count: 5,
            ..Default::default()
        };
        let v = feature_vector(&CollusionInputs {
            peak: 1.0,
            clusters: &[cluster(vec![0, 1, 2])],
            docs: &two,
            rsid_hits: &hits,
            numeric: Some(&nm),
            ..Default::default()
        });
        assert!(v.iter().all(|x| (0.0..=1.0).contains(x)), "特征必须落在 [0,1]：{v:?}");
        assert!(v[col("rsid")] > 0.0 && v[col("similarity")] > 0.0);
    }

    #[test]
    fn evasion_single_signal_alone_does_not_reach_high() {
        // 单一 evasion 信号（即便 confirmed）不达 high 线是有意设计（单证据不定罪）
        let ev_docs = [Some(ev(SEVERITY_CONFIRMED))];
        let c = assess_with(CollusionInputs { evasion: &ev_docs, ..Default::default() });
        assert_eq!(c.level, "low", "v1 尺度 0.25 < LEVEL_MEDIUM(0.35)，落 low");
        assert!(c.score < active_model().level_high);
    }
}
