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
