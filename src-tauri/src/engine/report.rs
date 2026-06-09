// 报告数据模型（serde 序列化为 camelCase，供前端直接使用）
use serde::{Deserialize, Serialize};

/// 文档元数据指纹 —— 判断雷同/围标的关键信号，比正文相似度更难抵赖。
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Fingerprint {
    pub author: Option<String>,           // dc:creator
    pub last_modified_by: Option<String>, // cp:lastModifiedBy
    pub created: Option<String>,          // dcterms:created
    pub modified: Option<String>,         // dcterms:modified
    pub app: Option<String>,              // Application (app.xml)
    pub revision: Option<String>,         // cp:revision
    pub total_edit_minutes: Option<i64>,  // TotalTime（总编辑时长，分钟）
    pub risk_flags: Vec<String>,          // 交叉风险标记，如「作者相同」
}

/// 单份文档的解析结果与指纹。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DocInfo {
    pub id: String,
    pub name: String,
    pub doc_type: String, // "docx" | "pdf" | "txt" | ...
    pub pages: u32,
    pub char_count: usize,
    pub fingerprint: Fingerprint,
    pub parse_error: Option<String>, // 该份解析失败时的原因（不影响整体）
}

/// 进度事件（检测中实时反馈）。stage: parse | compare | cluster | done
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub stage: String,
    pub done: usize,
    pub total: usize,
    pub note: String,
}

/// 字符级差异片段。op: "eq"(相同) | "ins"(B 增) | "del"(A 删)。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DiffOp {
    pub op: String,
    pub text: String,
}

/// 逐对对比中的一处段落匹配。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SegMatch {
    pub text_a: String,
    pub text_b: String,
    pub score: f32,
    pub diff: Vec<DiffOp>,
}

/// 某一对文档的逐段对比明细。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PairDetail {
    pub a: usize,
    pub b: usize,
    pub score: f32,
    pub matches: Vec<SegMatch>,
}

/// 聚合中的一个雷同段落实例。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ClusterSeg {
    pub doc: usize,
    pub text: String,
}

/// 跨文档雷同条款聚合。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Cluster {
    pub avg_score: f32,
    pub peak: f32,
    pub docs: Vec<usize>,
    pub segments: Vec<ClusterSeg>,
}

/// 围标判定的单条信号。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CollusionSignal {
    pub kind: String, // similarity | cluster | metadata | sharedTerms
    pub detail: String,
    pub weight: f32,
}

/// 围标综合判定（多信号加权，替代单一相似度阈值）。
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Collusion {
    pub level: String, // high | medium | low | none
    pub score: f32,    // 0..1
    pub signals: Vec<CollusionSignal>,
}

/// 章节热力：某文档某标段的跨文档雷同强度。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SectionStat {
    pub doc: usize,
    pub section: String, // tech | business | other
    pub intensity: f32,  // 最大跨文档相似度 0..1
    pub matches: u32,    // 命中片段数
}

/// 共有特征词（多份标书共用的罕见多字词，疑似同源 / 共用笔误）。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SharedTerm {
    pub term: String,
    pub docs: Vec<usize>,
}

/// 交叉比对报告。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub docs: Vec<DocInfo>,        // 文档顺序对应矩阵行列（甲乙丙丁戊）
    pub matrix: Vec<Vec<f32>>,     // n×n 文档级相似度，对角线为 1.0
    pub peak: f32,                 // 非对角线最大相似度（峰值）
    pub pairs: Vec<PairDetail>,    // 逐对对比：每对文档的匹配段落 + diff
    pub clusters: Vec<Cluster>,    // 重复条款：跨文档雷同段落聚合
    #[serde(default)]
    pub collusion: Collusion, // 围标综合判定
    #[serde(default)]
    pub sections: Vec<SectionStat>, // 章节热力
    #[serde(default)]
    pub shared_terms: Vec<SharedTerm>, // 共有特征词
}
