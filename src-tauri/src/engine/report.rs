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
    /// M2 入口对抗层：从 documents.evasion_json 判级后的规避特征摘要（None = 无发现/旧任务）。
    /// serde(default)：旧比对任务的 DocInfo（无此字段）反序列化天然兼容（取 None）。
    #[serde(default)]
    pub evasion: Option<EvasionSummary>,
}

/// 规避特征严重级（§1.5：机器不下「规避/串通」定性，severity 仅驱动呈现权重与围标信号强度）。
pub const SEVERITY_NONE: &str = "none";
pub const SEVERITY_SUSPECT: &str = "suspect";
pub const SEVERITY_CONFIRMED: &str = "confirmed";

// —— 规避判级线（集中于此，沿用 collusion 权重的「⚠️ 未经校准」惯例）——
// ⚠️ 未经语料校准（等 scheme §9.3 合成对抗语料回测）：以下均为基于攻击面的经验初值。
/// confirmed：PDF 隐藏文字层占比达此值即强证据（成规模的隐藏正文注入）。
const CONFIRMED_HIDDEN_RATIO: f64 = 0.05;
/// confirmed：同词混合脚本红旗词达此数即强证据（跨脚本同形替换是刻意行为，非误触）。
const CONFIRMED_MIXED_SCRIPT: u32 = 3;
/// suspect：隐形码点（零宽/双向/Tags/变体）总数达此值方计弱证据。
const SUSPECT_INVISIBLE_MIN: u32 = 10;
/// suspect 聚集度过滤：最大单块改写浓度达此值即视为聚集扰动（防复制粘贴零宽残留误判）。
const SUSPECT_CONCENTRATION_MIN: f64 = 0.02;
/// suspect 聚集度过滤：受影响段块数达此值即视为系统性扰动而非单处零星残留。
const SUSPECT_AFFECTED_MIN: u32 = 2;

/// 文档级规避特征摘要：比对期从 documents.evasion_json（W2 入口对抗层落库的冻结通道）解析、
/// 判级后挂到 DocInfo，供 evasion 围标信号与前端呈现消费。§1.5 产品纪律：severity 只驱动呈现
/// 权重，措辞统一「检测到疑似规避特征，请人工复核」，机器不下「规避/串通/清白」结论。
/// 全字段 serde(default)：旧 evasion_json 缺 pdfAudit/xcheck 子对象时反序列化取零值兜底。
#[derive(Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EvasionSummary {
    /// 零宽字符数（U+200B–200F/FEFF）。
    #[serde(default)]
    pub zero_width: u32,
    /// 双向控制符数。
    #[serde(default)]
    pub bidi: u32,
    /// Tags 隐写块字符数。
    #[serde(default)]
    pub tags: u32,
    /// 变体选择符数。
    #[serde(default)]
    pub variation: u32,
    /// 跨脚本同形字折叠命中数。
    #[serde(default)]
    pub confusable_folds: u32,
    /// 同词内混合脚本红旗数。
    #[serde(default)]
    pub mixed_script_words: u32,
    /// 有任一发现的段落级分块数（聚集度判据）。
    #[serde(default)]
    pub affected_chunks: u32,
    /// 最大单块改写浓度（聚集度判据）。
    #[serde(default)]
    pub max_chunk_concentration: f64,
    /// PDF 隐藏文字层占比（无 pdfAudit 时 0）。
    #[serde(default)]
    pub pdf_hidden_ratio: f64,
    /// PDF 隐藏字符数（无 pdfAudit 时 0）。
    #[serde(default)]
    pub pdf_hidden_chars: u64,
    /// 渲染-OCR 交叉验证命中种类（fontRemap/coordShuffle 机器标识；None = 未命中/未做）。
    #[serde(default)]
    pub xcheck_kind: Option<String>,
    /// 渲染-OCR 交叉验证命中中文标签（呈现用；None = 未命中/未做，不做清白背书）。
    #[serde(default)]
    pub xcheck_label: Option<String>,
    /// 严重级：none | suspect | confirmed（由 from_evasion_json 判级填入）。
    pub severity: String,
}

impl EvasionSummary {
    /// 从 documents.evasion_json 解析并判级。None = json 缺失或无法解析（视作无发现）；
    /// 解析成功恒返回 Some（severity 可能为 none——存在弱发现但未过判级线，前端不打徽标、
    /// 围标信号不计权，但计数仍保留供人工下钻）。
    pub fn from_evasion_json(json: &str) -> Option<Self> {
        let v: serde_json::Value = serde_json::from_str(json).ok()?;
        let u32_at = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
        let mut s = EvasionSummary {
            zero_width: u32_at("zeroWidth"),
            bidi: u32_at("bidi"),
            tags: u32_at("tags"),
            variation: u32_at("variation"),
            confusable_folds: u32_at("confusableFolds"),
            mixed_script_words: u32_at("mixedScriptWords"),
            affected_chunks: u32_at("affectedChunks"),
            max_chunk_concentration: v
                .get("maxChunkConcentration")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
            pdf_hidden_ratio: v
                .pointer("/pdfAudit/hiddenRatio")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(0.0),
            pdf_hidden_chars: v
                .pointer("/pdfAudit/hiddenChars")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0),
            xcheck_kind: v
                .pointer("/xcheck/verdict/kind")
                .and_then(serde_json::Value::as_str)
                .map(String::from),
            xcheck_label: v
                .pointer("/xcheck/verdict/label")
                .and_then(serde_json::Value::as_str)
                .map(String::from),
            severity: SEVERITY_NONE.to_string(),
        };
        s.severity = s.grade().to_string();
        Some(s)
    }

    /// 隐形码点（剥离类）总数。
    fn invisible_total(&self) -> u32 {
        self.zero_width + self.bidi + self.tags + self.variation
    }

    /// 判级：confirmed（强证据）> suspect（弱证据，经聚集度过滤）> none。判级常量集中于本模块。
    fn grade(&self) -> &'static str {
        // confirmed：xcheck 命中 / PDF 隐藏文本占比高 / 多处跨脚本同形替换——任一即强证据。
        if self.xcheck_kind.is_some()
            || self.pdf_hidden_ratio >= CONFIRMED_HIDDEN_RATIO
            || self.mixed_script_words >= CONFIRMED_MIXED_SCRIPT
        {
            return SEVERITY_CONFIRMED;
        }
        // suspect：隐形码点多 / 同形字折叠命中，且经聚集度过滤排除零星复制粘贴残留。
        let aggregated = self.max_chunk_concentration >= SUSPECT_CONCENTRATION_MIN
            || self.affected_chunks >= SUSPECT_AFFECTED_MIN;
        if aggregated && (self.invisible_total() >= SUSPECT_INVISIBLE_MIN || self.confusable_folds > 0)
        {
            return SEVERITY_SUSPECT;
        }
        SEVERITY_NONE
    }

    pub fn is_confirmed(&self) -> bool {
        self.severity == SEVERITY_CONFIRMED
    }

    pub fn is_suspect(&self) -> bool {
        self.severity == SEVERITY_SUSPECT
    }

    /// 达判级线（confirmed 或 suspect）——供围标信号与前端徽标判据。
    pub fn is_flagged(&self) -> bool {
        self.is_confirmed() || self.is_suspect()
    }

    /// 命中的证据种类中文短标签（供围标信号 detail 列举「证据种类」；flagged 时恒非空）。
    pub fn evidence_kinds(&self) -> Vec<&'static str> {
        let mut kinds = Vec::new();
        if self.invisible_total() > 0 {
            kinds.push("隐形码点");
        }
        if self.confusable_folds > 0 {
            kinds.push("同形字");
        }
        if self.mixed_script_words > 0 {
            kinds.push("混合脚本");
        }
        if self.pdf_hidden_chars > 0 {
            kinds.push("PDF隐藏文字");
        }
        if self.xcheck_kind.is_some() {
            kinds.push("渲染-OCR交叉验证");
        }
        kinds
    }
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
    /// k-共现查证（W3-3）：本簇经查证命中招标/背景库属合法共享 → 退出围标信号②计数
    /// （不再按「≥3 家共有强雷同」加分）。默认 false。
    #[serde(default)]
    pub exempted: bool,
    /// k-共现查证（W3-3）：≥3 家共有且两库皆查不到出处、查证质量闸门通过 → 归入独立
    /// multiDocAnomaly 信号（不计入信号②，不自动 high）。默认 false。
    #[serde(default)]
    pub anomaly: bool,
}

/// 围标判定的单条信号。
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CollusionSignal {
    pub kind: String, // similarity|cluster|metadata|sharedTerms|facts|rsid|pdfLineage|imageReuse|sharedErrors|evasion
    pub detail: String,
    pub weight: f32,
}

/// 围标综合判定（多信号加权，替代单一相似度阈值）。
#[derive(Serialize, Deserialize, Clone, Default)]
#[serde(rename_all = "camelCase")]
pub struct Collusion {
    pub level: String, // high | medium | low | none
    /// M7 起语义为【校准后证据强度】0..1（以「零证据」为零点重基的 σ(b+Σw·x)），
    /// 不再是 v1 的经验加权和；导出报告脚注与引擎版本号一并注明（§1.5-5）。
    pub score: f32,
    /// 信号分解：weight 字段 M7 起为该信号的 log-odds 贡献 w_i·x_i（DTO 形状不变）。
    pub signals: Vec<CollusionSignal>,
    /// 校准来源标签（§1.5-6 实验性标签）：experimental-synthetic = 合成语料拟合的 LR 权重；
    /// empirical-fallback = 权重文件不可用/未过符号审查时的 v1 经验权重回退路径。
    /// serde(default)：旧任务 collusion_json 缺该字段 → 空串，渲染侧按「未标注」处理。
    #[serde(default)]
    pub calibration_kind: String,
    /// 生效权重文件的版本号（§1.5-5：导出报告脚注需注明校准来源与版本，结论方可复现举证）。
    #[serde(default)]
    pub calibration_version: String,
    /// 技术字段（§1.5-2）：融合层原始概率 σ(z)。【在合成校准语料上测得，不是串通概率】——
    /// 仅供审计与二次分析，UI/报告一律不得展示为「串通概率 X%」。旧任务缺字段 → None。
    #[serde(default)]
    pub probability: Option<f32>,
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

    // —— M2 规避：EvasionSummary 判级（confirmed/suspect/none）与旧任务兼容 ——

    #[test]
    fn evasion_confirmed_by_xcheck_hit() {
        // xcheck 命中即 confirmed（即使无隐形码点/无 PDF 隐藏层）
        let json = r#"{"zeroWidth":0,"affectedChunks":0,"maxChunkConcentration":0.0,
            "xcheck":{"verdict":{"kind":"fontRemap","label":"疑似字体重映射/图片化正文"},"medianMismatch":0.62}}"#;
        let e = EvasionSummary::from_evasion_json(json).expect("应解析");
        assert!(e.is_confirmed(), "xcheck 命中 → confirmed");
        assert_eq!(e.xcheck_kind.as_deref(), Some("fontRemap"));
        assert!(e.evidence_kinds().contains(&"渲染-OCR交叉验证"));
    }

    #[test]
    fn evasion_confirmed_by_hidden_ratio_and_mixed_script() {
        // 隐藏文本占比 ≥5% → confirmed
        let ratio = r#"{"zeroWidth":0,"pdfAudit":{"hiddenChars":80,"hiddenRatio":0.06,"totalChars":1333}}"#;
        let e = EvasionSummary::from_evasion_json(ratio).expect("应解析");
        assert!(e.is_confirmed(), "隐藏占比 6% ≥ 5% → confirmed");
        assert!(e.evidence_kinds().contains(&"PDF隐藏文字"));
        // 占比不足但同词混合脚本 ≥3 → confirmed
        let mixed = r#"{"mixedScriptWords":3,"confusableFolds":5,"affectedChunks":1,"maxChunkConcentration":0.5}"#;
        let e2 = EvasionSummary::from_evasion_json(mixed).expect("应解析");
        assert!(e2.is_confirmed(), "混合脚本词 3 ≥3 → confirmed");
        assert!(e2.evidence_kinds().contains(&"混合脚本"));
    }

    #[test]
    fn evasion_suspect_requires_aggregation_filter() {
        // 隐形码点 ≥10 且浓度达标 → suspect
        let dense = r#"{"zeroWidth":12,"affectedChunks":1,"maxChunkConcentration":0.08}"#;
        let e = EvasionSummary::from_evasion_json(dense).expect("应解析");
        assert!(e.is_suspect(), "隐形码点多+浓度达标 → suspect");
        assert!(e.evidence_kinds().contains(&"隐形码点"));
        // 隐形码点 ≥10 但浓度极低、单块 → 复制粘贴残留，判 none（聚集度过滤）
        let sparse = r#"{"zeroWidth":11,"affectedChunks":1,"maxChunkConcentration":0.001}"#;
        let e2 = EvasionSummary::from_evasion_json(sparse).expect("应解析");
        assert_eq!(e2.severity, SEVERITY_NONE, "零星残留经聚集度过滤应为 none");
        // 受影响块数 ≥2 也算聚集（系统性扰动）
        let spread = r#"{"zeroWidth":10,"affectedChunks":2,"maxChunkConcentration":0.001}"#;
        let e3 = EvasionSummary::from_evasion_json(spread).expect("应解析");
        assert!(e3.is_suspect(), "多块受扰应过聚集度过滤 → suspect");
    }

    #[test]
    fn evasion_lone_confusable_without_aggregation_is_none() {
        // 单个同形字折叠、单块、浓度极低（疑似打字错误）→ none，不误报
        let json = r#"{"confusableFolds":1,"affectedChunks":1,"maxChunkConcentration":0.005}"#;
        let e = EvasionSummary::from_evasion_json(json).expect("应解析");
        assert_eq!(e.severity, SEVERITY_NONE, "孤立同形字未过聚集度过滤应为 none");
    }

    #[test]
    fn evasion_old_json_without_pdf_or_xcheck_deserializes() {
        // W2-1/2 早期 evasion_json（仅隐形码点统计，无 pdfAudit/xcheck 子对象）应解析且判级正确
        let old = r#"{"zeroWidth":15,"bidi":0,"tags":0,"variation":0,"confusableFolds":2,
            "mixedScriptWords":1,"affectedChunks":3,"maxChunkConcentration":0.03}"#;
        let e = EvasionSummary::from_evasion_json(old).expect("旧 evasion_json 应兼容");
        assert_eq!(e.pdf_hidden_chars, 0, "缺 pdfAudit 取零值");
        assert!(e.xcheck_kind.is_none(), "缺 xcheck 取 None");
        assert!(e.is_suspect(), "混合脚本仅 1（<3）→ 不 confirmed；隐形码点多+多块 → suspect");
    }

    #[test]
    fn old_doc_info_json_without_evasion_deserializes() {
        // 旧比对任务落库的 DocInfo（无 evasion 字段）反序列化应取 None（serde default）
        let old = r#"{"id":"d1","name":"标书.docx","docType":"docx","pages":10,
            "charCount":5000,"fingerprint":{"riskFlags":[]},"parseError":null}"#;
        let d: DocInfo = serde_json::from_str(old).expect("旧 DocInfo JSON 应兼容");
        assert!(d.evasion.is_none(), "缺 evasion 字段应取 None");
    }
}

