// 导出数据模型：从 DB 装配的完整报告（八类统计 / 事实冲突 / 配置快照 / 版本附录）。
// 直接 serde 序列化即 JSON 报告（设计文档 §14.3 的超集）。
use crate::engine::fact::FactConflict;
use crate::engine::report::{Collusion, PairDetail, SectionStat, SharedTerm};
use crate::services::compare_service::CompareSummary;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportData {
    pub report_version: &'static str,
    pub app_version: &'static str,
    pub generated_at: String,
    pub workspace_id: String,
    pub job_id: String,
    pub job_name: Option<String>,
    pub documents: Vec<ExportDoc>,
    pub config: serde_json::Value,
    pub summary: Option<CompareSummary>,
    pub matrix: Vec<Vec<f32>>,
    pub peak: f32,
    pub collusion: Collusion,
    pub shared_terms: Vec<SharedTerm>,
    pub sections: Vec<SectionStat>,
    pub clusters: Vec<ExportCluster>,
    /// 逐对明细（旧报告结构的延续，xlsx/docx 使用）
    pub pairs: Vec<PairDetail>,
    // —— M2 附录 A 冻结节（HTML/JSON 先行，其余格式后置；缺省=该里程碑未落地）——
    /// 取证证据节：rsid/PDF 血缘/图片同源/共同错误的命中明细 + 逐文档取证指纹。
    /// None = 本次无任何取证命中（§1.5：不渲染空「取证证据」表，避免沉默背书）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forensic: Option<ForensicSection>,
    /// 规避特征节：逐文档判级（none/suspect/confirmed）+ 各类计数。
    /// None = 本次无任何规避发现（§1.5：不渲染空表，未命中不构成清白证明）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evasion: Option<EvasionSection>,
    /// 对齐区段与逐字证据节（附录 A segments；M5 填充）。§1.5 铁律：屏幕可见证据须至少一种正式
    /// 报告格式可引用——区段摘要 + 逐字雷同区间清单进 HTML/DOCX 两主格式（JSON 随 serde 顺带）。
    /// None = 本次比对无任何对齐区段/逐字铁证（旧任务或未开对齐）：不渲染空章节，避免沉默背书。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<SegmentsSection>,
    /// 商务标数值证据节（附录 A numeric；M6 填充）：逐项单价雷同率、规律性/相关性结论、
    /// 共享算术错误清单、逐文档尾数分布。None = 本次无任何报价清单数据（纯技术标 / 扫描件
    /// PDF 的 OCR 路径 / 数值层关闭）：不渲染空章节，避免沉默背书。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub numeric: Option<NumericSection>,
    /// 复核路由三带节（附录 A calibration；M7 填充）。恒常驻（未校准时也写，如实说明
    /// 「本次未启用校准」），六格式导出统一从这里取文案与计数。
    pub calibration: CalibrationSection,
    /// 「检查方法与局限」节：§1.5 硬约束——无论是否命中恒常驻，列已执行检查项 +
    /// 可清除性说明 + 「未命中不构成清白证明」声明，堵住沉默背书。
    pub methods_and_limitations: MethodsAndLimitations,
}

/// 商务标数值证据节（附录 A numeric：pairs + docs digitStats）。
/// 数据源是 jobs.numeric_json（比对期落库的事实快照），此处只做天干标签化与结构化，不重算。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericSection {
    /// 本次任务生效的雷同率告警线（配置快照，报告可复现）。
    pub identical_rate_alarm: f64,
    /// 出雷同率结论所需的最小可比条目数。
    pub min_comparable: usize,
    pub item_count: usize,
    pub aligned_item_count: usize,
    pub pairs: Vec<NumericPairEntry>,
    pub docs: Vec<NumericDocEntry>,
    /// 机制感知筛查（W5-5）：「基准价敏感性」描述性小节。None = 本次未录入评标办法。
    /// 【不参与围标分级】——本块只是解释性分析，措辞由 notes 强制随数据下发。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mechanism: Option<NumericMechanism>,
    /// §1.5 强制措辞（雷同率口径 / 共享算术错误人工核对 / 数值层覆盖范围声明）：
    /// 随节下发，任何格式的写器都不得省略。
    pub notes: Vec<String>,
}

/// 「基准价敏感性」小节（附录 A numeric 节的 M8 扩列；数据源 numeric_json.mechanism）。
/// 天干标签化后原样呈现，【不重算】任何数字。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericMechanism {
    /// false = 公式不匹配 / 数据不足 → 只写 notApplicableReason，不写任何计算结果。
    pub applicable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_applicable_reason: Option<String>,
    pub method: String,
    /// 评标办法公式全文（人工录入回显，供逐字核对）。
    pub formula: String,
    pub prices: Vec<MechanismPrice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub benchmark: Option<MechanismBenchmark>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lowest: Option<MechanismLowest>,
    pub support_bids: Vec<MechanismSupportBid>,
    /// §1.5 强制措辞（不参与围标分级 / 人工录入需核对 / 组构造依据 / 反事实口径 / 断崖口径）。
    pub notes: Vec<String>,
}

/// 一份投标总价及其来源打标。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanismPrice {
    pub tag: String,
    pub total: f64,
    /// 中文来源标签（取自投标总价行 / 取自清单合计 / 启发式回落）。
    pub source_label: String,
}

/// 均值基准价一族的反事实块（method=lowest 时缺席）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanismBenchmark {
    pub trim_lowest: usize,
    pub trim_highest: usize,
    pub coeff_min: f64,
    pub coeff_max: f64,
    pub grid_points: usize,
    pub coeff_mid: f64,
    pub benchmark_mid: f64,
    pub winner_mid: String,
    pub groups: Vec<MechanismGroup>,
}

/// 一个候选嫌疑组的反事实结果。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanismGroup {
    /// 组内文档天干标签。
    pub docs: Vec<String>,
    /// 组的构造依据（必须随组呈现，防循环论证观感）。
    pub basis: Vec<String>,
    pub flip_prob: f64,
    pub benchmark_shift_pct: f64,
    pub shift_percentile: f64,
    pub subsets_compared: usize,
    pub winner_full: String,
    pub winner_excluded: String,
    pub support_bid_docs: Vec<String>,
}

/// 最低评标价法的最低价孤立度。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanismLowest {
    pub winner: String,
    pub lowest: f64,
    pub second_lowest: f64,
    pub gap: f64,
    pub median_gap: f64,
    pub isolated: bool,
}

/// 一条断崖式报价（support-bid 形态）标记。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MechanismSupportBid {
    pub tag: String,
    pub total: f64,
    /// "lowest" | "highest"
    pub position: String,
    pub gap: f64,
    pub median_gap: f64,
    pub deviation_pct: f64,
}

/// 一对参评文档的数值比对结果（a/b 为天干标签）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericPairEntry {
    pub a: String,
    pub b: String,
    /// 可比条目数（双方均有单价、且非暂估价/信息价类的对齐项）。
    pub comparable: usize,
    pub identical: usize,
    /// None = 可比条目不足，不出结论（reason 给原因）。
    pub identical_rate: Option<f64>,
    pub alarm: bool,
    pub reason: Option<String>,
    pub pattern: Option<NumericPattern>,
    pub correlation: Option<NumericCorrelation>,
    pub shared_arith_errors: Vec<NumericArithError>,
}

/// 规律性差异（等差/等比/仿射）。note 是 §1.5「统一下浮」线索文案，随数据下发。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericPattern {
    pub kind: String,
    pub a: f64,
    pub b: f64,
    pub r2: f64,
    pub n: usize,
    pub corroborated: bool,
    pub note: String,
}

/// 单价向量相关性。ratio_cv 必须与 pearson 同屏（§1.5：只有 r>0.99 且 CV≈0 才是强证据）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericCorrelation {
    pub n: usize,
    pub pearson: f64,
    pub spearman: f64,
    pub ratio_cv: Option<f64>,
    pub note: String,
}

/// 一条共享算术错误（同一清单项双方工程量/单价/算错的合价三者全等）。
/// chunk_ids 为双方原文锚点，供人工回原文核对。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericArithError {
    pub align_key: String,
    pub name: Option<String>,
    pub qty: f64,
    pub unit_price: f64,
    pub total: f64,
    pub expected_total: f64,
    pub chunk_ids: Vec<String>,
}

/// 逐文档数值画像（附录 A docs[]：docId + digitStats）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NumericDocEntry {
    pub doc_id: String,
    pub tag: String,
    /// 尾数分布检验；None = 单价样本不足，不出结论（原样透传比对期快照）。
    pub digit_stats: Option<serde_json::Value>,
}

/// 对齐区段与逐字证据节（附录 A：pairs[{a,b,segments}]，M5 按证据层扩 verbatims 子清单）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentsSection {
    pub pairs: Vec<SegmentPair>,
}

/// 一对参评文档的对齐区段 + 逐字区间清单（天干标签 a/b 为该对两文档位次）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentPair {
    pub a: String,
    pub b: String,
    pub segments: Vec<SegmentEntry>,
    /// 逐字雷同区间清单（含双侧页码）。§1.5：落在招标豁免块的部分标注「引用招标文件」。
    pub verbatims: Vec<VerbatimEntry>,
}

/// 一条对齐区段摘要（附录 A segments[]：aRange/bRange/coverage/verbatimChars + 证据层扩列）。
/// aRange/bRange 为两侧章节路径 + 页码范围的可读定位串（含页码，供报告直接引用）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentEntry {
    pub a_range: String,
    pub b_range: String,
    /// 覆盖率（双向取较大值 = 较小文档被对齐区段覆盖的比例，与区段视图/矩阵区段口径同源）。
    pub coverage: f64,
    pub verbatim_chars: i64,
    pub anchor_count: i64,
    /// 落在招标豁免块（tender_coverage≥0.8）：显示「引用招标文件」徽标（与 M4/屏幕一致）。
    pub tender_quote: bool,
}

/// 一条逐字雷同区间（深红铁证）：双侧起块页码 + 章节 + 去空白匹配字数 + 文本样本。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerbatimEntry {
    pub a_page: Option<i64>,
    pub b_page: Option<i64>,
    pub a_section: Option<String>,
    pub b_section: Option<String>,
    pub char_len: i64,
    pub sample: String,
    pub tender_quote: bool,
}

/// 取证证据节（附录 A：hits + perDocument）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicSection {
    pub hits: Vec<ForensicHit>,
    pub per_document: Vec<ForensicDoc>,
}

/// 一条取证命中（跨文档）。level：hard 硬命中（单点定案级）| mid 中命中 | weak 弱命中。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicHit {
    pub kind: String, // rsid | pdfLineage | imageReuse | sharedErrors
    pub doc_a: String, // 天干标签（image/sharedErrors 逐对结构未落库时留空，明细见 detail）
    pub doc_b: String,
    pub level: String, // hard | mid | weak
    pub detail: String,
}

/// 逐文档取证指纹（供报告「取证指纹」表列出 rsid 数/模板/血缘 GUID）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForensicDoc {
    pub doc_id: String,
    pub tag: String,
    pub rsid_count: usize,
    pub template_name: Option<String>,
    /// 血缘键快照：{documentId, idFirst, derivedFrom, fontSubsetTags}（缺失取 null/空）。
    pub lineage: serde_json::Value,
}

/// 规避特征节（附录 A：perDocument）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvasionSection {
    pub per_document: Vec<EvasionDoc>,
}

/// 逐文档规避判级 + 计数（原样透传 EvasionSummary 的计数快照）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvasionDoc {
    pub doc_id: String,
    pub tag: String,
    pub counts: serde_json::Value,
    pub verdict: String, // none | suspect | confirmed
    /// 命中证据种类中文短标签（隐形码点/同形字/混合脚本/PDF隐藏文字/渲染-OCR交叉验证）。
    pub evidence_kinds: Vec<String>,
}

/// 「检查方法与局限」节（§1.5 常驻，无条件序列化）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MethodsAndLimitations {
    pub checks_run: Vec<String>,
    pub disclaimers: Vec<String>,
}

impl MethodsAndLimitations {
    /// §1.5 固定文案：已执行检查项 + 可清除性说明 + 「未命中不构成清白证明」声明。
    /// 无条件常驻，与是否命中无关——沉默即背书是本节要堵的漏洞。
    pub fn standard() -> Self {
        MethodsAndLimitations {
            checks_run: vec![
                "文档元数据指纹交叉（作者/最后保存者/模板名/创建时间邻近/打包结构一致）".into(),
                "docx 修订标识（rsid）两两交集".into(),
                "PDF 血缘取证（trailer /ID、XMP DocumentID/DerivedFrom、字体子集标签）".into(),
                "内嵌图片同源（精确 sha256 + 感知 dHash）".into(),
                "共同错误指纹（词典外词 / 异常标点 / 错误引用）".into(),
                "入口对抗规避特征（隐形码点剥离 / 同形字折叠 / 混合脚本红旗 / PDF 隐藏文字层审计 / 渲染-OCR 交叉验证）".into(),
            ],
            disclaimers: vec![
                "以上检查项均可被清除：另存为新文件即可清除 rsid，元数据可被编辑抹除，图片可重新截图，规避痕迹可人工去除。".into(),
                "未命中不构成清白证明——仅表示本工具在上述检查项下未发现该类特征，不排除以其他方式规避或本工具未覆盖的情形。".into(),
                "取证与规避信号均为线索级证据，是否构成围标/串通须由评标委员会依法认定，本报告不作定性结论。".into(),
            ],
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportDoc {
    pub tag: String, // 天干位次
    pub name: String,
    pub file_type: String,
    pub pages: i64,
    pub char_count: i64,
    pub parse_method: Option<String>,
    pub risk_flags: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportCluster {
    pub index: usize,
    pub cluster_type: String,
    pub severity: Option<String>,
    pub topic: Option<String>,
    pub summary: Option<String>,
    pub score: Option<f64>,
    pub review_status: String,
    pub section_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<FactConflict>,
    /// k-共现查证（W3-3）：合法共享出处（'tender'|'background'）→ 报告置灰、退出风险统计；None=未豁免。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exempt_reason: Option<String>,
    /// k-共现查证（W3-3）：『多家异常一致·待复核』——进导出「多家异常一致清单」小节（涉嫌措辞+脚注）。
    pub multi_doc_anomaly: bool,
    /// 复核路由三带（W6-4）：pass|review|flag；旧任务/未校准 → None（报告写「未校准」）。
    pub band: Option<String>,
    /// 校准置信度（W6-4）：【技术字段】——JSON 保留数值，人读格式只写三带口头名，
    /// 数值一律带「在合成校准语料上校准、非串通概率」的限定语（§1.5-2）。
    pub confidence: Option<f64>,
    pub members: Vec<ExportMember>,
}

/// 复核路由三带章节（W6-4，M7）。恒常驻：无论是否启用分流都要写，说明这份报告的条款
/// 是按什么口径排的队——不写等于让读者以为「重点标红」是系统的定性结论（§1.5-1/2）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationSection {
    /// 校准来源标签（experimental-synthetic = 合成语料拟合）与版本、语料 hash。
    pub calibration_kind: String,
    pub version: String,
    pub corpus_hash: String,
    /// three-band = 三带分流生效；review-all = 分流未启用，全部按需人工复核。
    pub routing: String,
    /// 目标漏检率/误报率（【在合成校准语料上测得】）。
    pub alpha: f32,
    pub beta: f32,
    /// 三带计数（含未校准档）。四者之和 = 报告内条款总数。
    pub pass_count: usize,
    pub review_count: usize,
    pub flag_count: usize,
    pub uncalibrated_count: usize,
    /// 三带中文名（写器不得自造文案，一律取此处）。
    pub pass_label: String,
    pub review_label: String,
    pub flag_label: String,
    /// §1.5 强制措辞：分流口径说明 + 「低优先级抽查带不隐藏任何条款」+ 概率语义限定。
    pub notes: Vec<String>,
}

impl CalibrationSection {
    /// 由【比对期落库的校准快照】+ 三带计数装配。
    ///
    /// 用快照而不是当前生效模型：报告必须复现「这份结论当时是按哪一版校准排的队」——
    /// 升级安装包后重新导出旧任务，若改写成新版本号，报告就与落库的 band 对不上了。
    /// corpus_hash 仅在快照版本与当前随包文件一致时补充（否则该 hash 无从考据，留空）。
    pub fn build(snapshot: Option<&CompareSummary>, counts: (usize, usize, usize, usize)) -> Self {
        use crate::engine::calibrate::{
            band_cn, band_hint, routing_note, Routing, BAND_FLAG, BAND_PASS, BAND_REVIEW,
        };
        let (pass_count, review_count, flag_count, uncalibrated_count) = counts;
        let snap = snapshot.filter(|s| !s.calibration_version.is_empty());
        let corpus_hash = snap
            .and_then(|s| {
                crate::engine::calibrate::active_calibration()
                    .filter(|m| m.version == s.calibration_version)
                    .map(|m| m.corpus_hash.clone())
            })
            .unwrap_or_default();
        let mut notes = vec![
            "复核路由三带是【复核优先级】维度，与条款分类、风险等级相互独立：三带不改变任何条款的分类与定性。"
                .to_string(),
            // 逐带释义走 calibrate::band_hint（三带文案的唯一来源，UI 与报告同一份）。
            format!("{}：{}", band_cn(BAND_FLAG), band_hint(BAND_FLAG)),
            format!("{}：{}", band_cn(BAND_REVIEW), band_hint(BAND_REVIEW)),
            format!("{}：{}", band_cn(BAND_PASS), band_hint(BAND_PASS)),
            format!(
                "「{}」带的条款【只排在最后并默认折叠，不被隐藏、不被剔除】：本报告已完整列出该带全部条款。",
                band_cn(BAND_PASS)
            ),
            "置信度为【在合成校准语料上校准】的数值，仅作复核排序参考，不是串通概率，也不构成对任何投标人的定性结论。"
                .to_string(),
        ];
        match snap {
            Some(s) => {
                let routing = if s.calibration_routing == "three-band" {
                    Routing::ThreeBand
                } else {
                    Routing::ReviewAll
                };
                notes.push(routing_note(routing, s.calibration_alpha, s.calibration_beta));
            }
            None => notes.push(
                "本次比对未启用概率校准（旧任务或校准文件不可用）：全部条款标记为「未校准」，按既有风险等级复核。"
                    .to_string(),
            ),
        }
        CalibrationSection {
            calibration_kind: snap.map(|s| s.calibration_kind.clone()).unwrap_or_default(),
            version: snap.map(|s| s.calibration_version.clone()).unwrap_or_default(),
            corpus_hash,
            routing: snap.map(|s| s.calibration_routing.clone()).unwrap_or_default(),
            alpha: snap.map(|s| s.calibration_alpha).unwrap_or(0.0),
            beta: snap.map(|s| s.calibration_beta).unwrap_or(0.0),
            pass_count,
            review_count,
            flag_count,
            uncalibrated_count,
            pass_label: band_cn(BAND_PASS).to_string(),
            review_label: band_cn(BAND_REVIEW).to_string(),
            flag_label: band_cn(BAND_FLAG).to_string(),
            notes,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportMember {
    pub doc: usize,
    pub tag: String,
    pub text: String,
    pub page: Option<i64>,
    pub section_path: Vec<String>,
    pub role: String,
}
