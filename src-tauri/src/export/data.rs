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
    /// 「检查方法与局限」节：§1.5 硬约束——无论是否命中恒常驻，列已执行检查项 +
    /// 可清除性说明 + 「未命中不构成清白证明」声明，堵住沉默背书。
    pub methods_and_limitations: MethodsAndLimitations,
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
    pub members: Vec<ExportMember>,
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
