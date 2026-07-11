// 报告数据模型（serde 序列化为 camelCase，供前端直接使用）
use serde::{Deserialize, Serialize};

/// 取证指纹 schema 版本：并入 ImportOptions::options_hash 的 v6 preimage（键 fpv）。
/// Fingerprint 每新增/变更提取字段（M1 的 rsid / PDF 血缘 / zip 条目指纹等）此值 +1，
/// 让跨工作区分块缓存整体失效重建——否则 persist_cached 按同 hash+同 options 命中
/// 旧行时会原样复制缺新字段的旧 fingerprint_json，「重新导入也拿不到新取证字段」
/// （执行方案全局裁决 3 引用的工程审查 HIGH）。后续只改此值，不动 options_hash 的
/// 版本前缀与格式（裁决 3「只 bump 一次 v5→v6」，做法对齐 templates_digest：值变即失效）。
/// v2：M1 新增 rsid/rsidRoot + Template/zip 条目序列指纹 + PDF 血缘
/// （trailer ID/XMP GUID/字体子集标签）（本里程碑内不再变更）。
pub const FINGERPRINT_SCHEMA_VERSION: u32 = 2;

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
    // —— M1 取证扩展（均 serde(default)：旧 fingerprint_json 缺字段时反序列化兜底）——
    /// word/settings.xml <w:rsids> 的全部 w:rsid 修订会话标识（去重、大写归一、上限 2048）
    #[serde(default)]
    pub rsids: Vec<String>,
    /// w:rsidRoot：文档创建时的根修订标识，相同即高度指示派生自同一母文件
    #[serde(default)]
    pub rsid_root: Option<String>,
    /// docProps/app.xml <Template>：文档模板名（Normal/Normal.dotm 为 Word 默认，不作信号）
    #[serde(default)]
    pub template_name: Option<String>,
    /// zip 条目名按中央目录顺序连接后的 sha256——同一生成工具/打包管线的稳定指纹
    #[serde(default)]
    pub zip_entry_fp: Option<String>,
    /// zip 条目总数（配合 zip_entry_fp 供人工核对）
    #[serde(default)]
    pub zip_entry_count: Option<u32>,
    // —— M1 PDF 血缘取证（同为 serde(default) 向后兼容）——
    /// PDF trailer /ID 首半（hex）：创建时生成、再保存不变——同一母文件的血缘键
    #[serde(default)]
    pub pdf_id_first: Option<String>,
    /// PDF trailer /ID 次半（hex）：每次保存都变化，仅供人工核对
    #[serde(default)]
    pub pdf_id_second: Option<String>,
    /// XMP xmpMM:DocumentID：文档 GUID，碰撞概率趋近于零的同源证据
    #[serde(default)]
    pub xmp_document_id: Option<String>,
    /// XMP xmpMM:InstanceID：本次保存实例的 GUID（供人工核对）
    #[serde(default)]
    pub xmp_instance_id: Option<String>,
    /// XMP xmpMM:DerivedFrom → stRef:documentID：派生自哪份母文件的 GUID
    #[serde(default)]
    pub xmp_derived_from: Option<String>,
    /// XMP xmp:CreatorTool：生成工具（弱同源信号的一部分）
    #[serde(default)]
    pub creator_tool: Option<String>,
    /// 逐页 /Resources /Font 的 BaseFont 名全集（去重排序、有上限）
    #[serde(default)]
    pub pdf_fonts: Vec<String>,
    /// 子集内嵌字体标签（形如 ABCDEF+SimSun 的 6 大写字母前缀 BaseFont，去重）
    /// ——前缀多数生成器随机，相同即指示「同一次生成环境」
    #[serde(default)]
    pub font_subset_tags: Vec<String>,
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


/// 字符级差异片段。op: "eq"(相同) | "ins"(B 增) | "del"(A 删)。
#[derive(Debug, Serialize, Deserialize, Clone)]
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
/// M1 共同错误指纹复用本结构承载三类检测器产出（词典外词 / 异常标点 / 引用错误）：
/// 原「罕见词」条目 kind=None，错误指纹条目 kind=Some("sharedErrors")；rarity 为稀有度
/// 归一分（越罕见越高，collusion 据此加权）；context 为错误串前后文（供人工判断，避免直接
/// 定性）。三个新字段均 serde(default)：旧任务 shared_terms_json 缺字段时反序列化取空值兜底。
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct SharedTerm {
    pub term: String,
    pub docs: Vec<usize>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub rarity: Option<f32>,
    #[serde(default)]
    pub context: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M1 之前落库的 fingerprint_json（无 rsids/templateName/zipEntryFp 等新字段）
    /// 必须能反序列化——serde(default) 兜底，新字段取空值（W1-1/W1-2 验收回归）。
    #[test]
    fn old_fingerprint_json_without_forensic_fields_deserializes() {
        let old = r#"{
            "author": "张三",
            "lastModifiedBy": "李四",
            "created": "2024-05-01T10:00:00Z",
            "modified": "2024-05-02T09:30:00Z",
            "app": "Microsoft Office Word",
            "revision": "3",
            "totalEditMinutes": 42,
            "riskFlags": ["作者相同「张三」: 甲·乙"]
        }"#;
        let fp: Fingerprint = serde_json::from_str(old).expect("旧 JSON 应兼容反序列化");
        assert_eq!(fp.author.as_deref(), Some("张三"));
        assert!(fp.rsids.is_empty(), "缺失字段应取 default 空值");
        assert!(fp.rsid_root.is_none());
        assert!(fp.template_name.is_none());
        assert!(fp.zip_entry_fp.is_none());
        assert!(fp.zip_entry_count.is_none());
        assert!(fp.pdf_id_first.is_none());
        assert!(fp.xmp_document_id.is_none());
        assert!(fp.xmp_derived_from.is_none());
        assert!(fp.creator_tool.is_none());
        assert!(fp.pdf_fonts.is_empty());
        assert!(fp.font_subset_tags.is_empty());
    }

    /// M1 之前落库的 shared_terms_json（只有 term/docs，无 kind/rarity/context）必须能
    /// 反序列化——serde(default) 兜底，新字段取 None（共同错误指纹条目验收 4：旧任务兼容）。
    #[test]
    fn old_shared_term_json_without_kind_deserializes() {
        let old = r#"[{"term":"微服务架构","docs":[0,1]},{"term":"数据中台","docs":[0,1,2]}]"#;
        let terms: Vec<SharedTerm> = serde_json::from_str(old).expect("旧 shared_terms_json 应兼容");
        assert_eq!(terms.len(), 2);
        assert_eq!(terms[0].term, "微服务架构");
        assert_eq!(terms[0].docs, vec![0, 1]);
        assert!(terms[0].kind.is_none(), "缺 kind 字段应取 None");
        assert!(terms[0].rarity.is_none());
        assert!(terms[0].context.is_none());
    }
}

