// 比对服务：算法管线 v2 的编排。
// 读分块 → TF-IDF → （可选）语义向量（缓存命中跳过）→ 多通道召回 → 五维精排 →
// 并查集聚类 → 八类分类 + 分级 diff → 单事务落库 → 矩阵/围标/共有词/章节热力聚合。
// 取消或失败时清掉本任务的全部半成品。
use crate::db::repo::compare_repo::{self, NewCluster, NewDiff, NewEdge, NewMember};
use crate::db::repo::{chunk_repo, document_repo, embedding_repo, image_repo, job_repo};
use crate::db::repo::document_repo::DocumentRow;
use crate::engine::clustering::{self, ScoredEdge};
use crate::engine::corpus::{self, CmpChunk};
use crate::engine::report::{Cluster as RCluster, ClusterSeg, DocInfo, EvasionSummary, Fingerprint, SectionStat, SharedTerm};
use crate::engine::{
    align, background, candidate, collusion, diff, embed, fact, fingerprint, matrix, scoring,
    verbatim, winnow,
};
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::jobs::JobCtx;
use crate::engine::embed::LoadedEmbedder;
use jieba_rs::Jieba;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// 经四层配置合并后的单次比对配置（原样存入 jobs.config_json）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareRunConfig {
    pub document_ids: Vec<String>,
    pub base_document_id: Option<String>,
    pub chunk_level: String,
    pub similarity_threshold: f32,
    pub candidate_top_k: usize,
    pub enable_semantic: bool,
    pub enable_fact_conflict: bool,
    pub ignore_templates: bool,
    pub detect_moved_paragraph: bool,
    pub scope: String,
    /// 招标文件对减（W3-2）：剥离投标对招标条款的合法逐字应答，风险分级用剔除后口径。
    /// 旧任务 config_json 无此键 → 默认 true（口径与新任务一致；无招标文件时对减自然空转）。
    #[serde(default = "default_true")]
    pub subtract_tender: bool,
    /// 语义模型选择（compare.embeddingModel：bge-zh(默认) | bge-large-zh | e5-large | e5-small | e5-base）。
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,
    /// security.allowCloudModel：是否允许联网下载语义模型（本地已缓存时不受限）。
    #[serde(default)]
    pub allow_model_download: bool,
    /// 逐字雷同区间最小字符数（W4-1，M5a）：极大公共子串 ≥ 此长度才作铁证。默认 30 汉字
    /// （可配 20–40，CompareSetup 暂不暴露）。旧任务 config_json 无此键 → 走 serde 默认。
    #[serde(default = "default_verbatim_min_chars")]
    pub verbatim_min_chars: usize,
    /// 对齐区段链化（W4-2 seed-chain-align，M5a）：残差边∪软种子∪逐字锚点链化成连续对齐区段
    /// （新增证据层，不替代聚类）。CompareSetup 暂不暴露，走默认开启。旧任务无此键 → serde 默认 true。
    #[serde(default = "default_true")]
    pub enable_alignment: bool,
}

fn default_embedding_model() -> String {
    "bge-zh".to_string()
}

pub fn default_verbatim_min_chars() -> usize {
    30
}

fn default_true() -> bool {
    true
}

/// 总览统计（jobs.summary_json）。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompareSummary {
    pub document_count: usize,
    pub chunk_count: usize,
    pub cluster_count: usize,
    pub same_count: usize,
    pub minor_change_count: usize,
    pub rewrite_count: usize,
    pub changed_count: usize,
    pub added_count: usize,
    pub deleted_count: usize,
    pub conflict_count: usize,
    pub uncertain_count: usize,
    pub high_risk_count: usize,
    pub semantic_degraded: bool,
    /// 被识别为「引用招标文件」（覆盖率≥0.8）并从残差比对剔除的投标分块数（W3-2）。
    /// 前端在矩阵摘要注明「已剔除招标文件引用 N 块」；0 表示无对减发生。
    pub tender_ref_chunk_count: usize,
    /// 被内置静态范本背景库判为「行业范本套话」（boiler_fraction≥0.6）并从聚类剔除的分块数（W3-4）。
    /// 仅在 ignore_templates 开启时非零；0 表示无背景套话剔除。
    pub background_exempt_chunk_count: usize,
    // —— 分区分层五区簇计数（§5 W3-5）：含 deleted 在内的每个簇按 section_kind 归一区，
    //    五者之和恒等于 cluster_count（catch-all 落 other）。
    pub zone_legal_count: usize,
    pub zone_price_count: usize,
    pub zone_tech_count: usize,
    pub zone_business_count: usize,
    pub zone_other_count: usize,
}

/// 每 chunk 的可选语义向量（None=该 chunk 无嵌入，如模型缺失或文本为空）。
type ChunkEmbeddings = Vec<Option<Vec<f32>>>;

const MAX_STORE_CLUSTERS: usize = 5000;
const MAX_DELETED_CLUSTERS: usize = 200;
const EMBED_BATCH: usize = 128;
/// 短文本动态阈值（§9.5/§22）：低于此字数的对，相似阈值上浮 SHORT_TEXT_BUMP——
/// 短句词面重合本来就高（「按合同执行。」），同阈值下误报率显著高于长段。
const SHORT_TEXT_CHARS: usize = 30;
const SHORT_TEXT_BUMP: f32 = 0.08;

// —— 分区分层阈值（§5 W3-5）常量集中区 ——
/// 法定格式区（投标函 / 承诺 / 声明 / 资格审查等）阈值上浮：该类文本天然一致，抬高阈值只压
/// 「法定格式套话」雷同——【不压】法定格式内填空字段 / 错误一致（那是真信号，走共同错误指纹，
/// 本条只做阈值分层；边界见 docs §5 W3-5「风险」段）。
const LEGAL_ZONE_BUMP: f32 = 0.12;
/// zone 阈值上浮统一封顶（legal + 短文本叠加后），防阈值不可达。
const ZONE_BUMP_CAP: f32 = 0.98;

/// 一对分块的有效相似阈值：zone 感知（§5 W3-5）+ 短文本上浮（§9.5），封顶防不可达。
/// · legal 区法定格式天然一致：阈值 +LEGAL_ZONE_BUMP，只压套话雷同——【不压】法定格式内填空
///   字段 / 错误一致（那是真信号，走共同错误指纹，本条只做阈值分层，边界见 docs §5 W3-5「风险」段）；
/// · price 区【维持现阈值】（docs §5 W3-5）：其证据链主体是事实冲突 / 金额通道（数值层 M6）而非
///   文字雷同，故文本相似不做额外上浮、按基础阈值参与聚类，以保住「同一明细行金额不一致 →
///   事实冲突」价值链（等 M6 数值层落地后再把 price 文本相似从围标口径中剥离）；
/// · 短文本（任一侧 < SHORT_TEXT_CHARS）再 +SHORT_TEXT_BUMP；
/// · tech/business/other 用基础阈值。
fn effective_threshold(base: f32, a: &CmpChunk, b: &CmpChunk) -> f32 {
    let mut t = base;
    if a.section_kind == "legal" || b.section_kind == "legal" {
        t += LEGAL_ZONE_BUMP;
    }
    if a.char_count.min(b.char_count) < SHORT_TEXT_CHARS {
        t += SHORT_TEXT_BUMP;
    }
    t.min(ZONE_BUMP_CAP)
}

/// 比对范围（scope）与五区（§5 W3-5）的映射：business 家族含 legal/price。
/// tech 范围 = tech + other（排除 business/legal/price）；business 范围 = 非 tech（含 business/
/// legal/price/other）；完整（其他值）全保留。
fn zone_in_scope(section_kind: &str, scope: &str) -> bool {
    match scope {
        "tech" => !matches!(section_kind, "business" | "legal" | "price"),
        "business" => section_kind != "tech",
        _ => true,
    }
}

/// 簇 zone → summary 五区计数槽位（0=legal 1=price 2=tech 3=business 4=other，None/未知 → other）。
/// catch-all 落 other 保证五区计数之和恒等于簇总数（验收 §5 W3-5 (4)）。
fn zone_slot(section_kind: Option<&str>) -> usize {
    match section_kind {
        Some("legal") => 0,
        Some("price") => 1,
        Some("tech") => 2,
        Some("business") => 3,
        _ => 4,
    }
}
/// 视为「基准文档内容缺失」前，允许的最高近似分（有更高的近似 → uncertain 而非 deleted）
const DELETED_FLOOR: f32 = 0.55;

// —— k-共现过滤升级（W3-3）常量集中区 ——
/// ≥N 家共有才进入 k-共现查证（与 collusion::CLUSTER_MULTI_DOCS 对齐）。
const MULTI_DOC_MIN: usize = 3;
/// ≥3 家共有簇的出处判定门槛：多数成员（严格 >50%）命中同一库即视为合法共享 → 豁免。
const SHARED_EXEMPT_MAJORITY: f32 = 0.5;
/// 「多家异常一致」升级的查证质量闸门（§1.5 铁律）：招标对减覆盖率抽样下限。抽样口径=参评
/// 分块 tender_coverage 的最大值——招标文本若真被投标引用，至少有一块高覆盖；全场覆盖过低
/// （招标件错传 / OCR 乱码打断 k-gram 链）则降级中性提示、不升级，防止用不可信索引做指控。
const ANOMALY_COVERAGE_SAMPLE_FLOOR: f32 = 0.5;
/// 异常簇 severity 值：独立「待复核」标记（渲染为「待复核」），不自动 high、不进 high 风险统计。
const SEVERITY_REVIEW: &str = "review";
/// 异常簇 summary 追加（§1.5：强制「涉嫌」措辞 + 评标委员会脚注；法条引用在 collusion 信号）。
const ANOMALY_SUMMARY_SUFFIX: &str =
    " · 涉嫌多家异常一致（招标文件与行业范本库均无出处），此为线索级提示、非定性结论，需评标委员会依法认定";
/// 查证质量闸门未过（招标件 OCR/扫描件 或 覆盖率抽样过低）时的中性提示——不引法条、不升 severity。
const NEUTRAL_SUMMARY_SUFFIX: &str = " · 多家共有段落，出处未能核实";

pub fn run_compare(
    ctx: &JobCtx,
    jieba: Arc<Jieba>,
    embedder: Arc<Mutex<LoadedEmbedder>>,
    workspace_id: &str,
    cfg: &CompareRunConfig,
) -> AppResult<()> {
    let r = run_inner(ctx, &jieba, &embedder, workspace_id, cfg);
    if r.is_err() {
        // 失败/取消后清理半成品；清理本身失败不能静默（会留下残留结果），记日志（仅 job_id + 码）
        match ctx.db.get() {
            Ok(conn) => {
                if let Err(e) = compare_repo::delete_job_results(&conn, &ctx.job_id) {
                    log::error!("清理比对半成品失败 job_id={} code={:?}", ctx.job_id, e.code);
                }
            }
            Err(e) => log::error!("清理比对半成品取连接失败 job_id={}: {e}", ctx.job_id),
        }
    }
    r
}

/// 招标文件豁免物料（M4 接线）：一次加载招标/补遗文档产出四类对减依据——
/// winnowing 指纹索引（残差矩阵/覆盖率）、rsid 集（含 rsidRoot）、内嵌图片 sha256 集、
/// 共同错误豁免（tokens + 原文串）。`Some(_)` 即表示「工作区已导入招标文件且豁免对减生效」，
/// 是条件化硬命中 floor 的启用前提（§1.5）。无招标文件/未开对减/索引为空时整体为 None。
struct TenderRefs {
    index: winnow::TenderIndex,
    rsids: HashSet<String>,
    image_hashes: HashSet<String>,
    error_exempt: TenderExemption,
    /// 任一参评招标文件为 OCR/扫描件（parse_method 含 "ocr"）——查证质量闸门（§1.5）：
    /// OCR 错字打断精确 k-gram 指纹链，招标索引不可信 → 禁用「多家异常一致」升级，降级中性提示。
    ocr: bool,
}

fn run_inner(
    ctx: &JobCtx,
    jieba: &Jieba,
    embedder: &Arc<Mutex<LoadedEmbedder>>,
    workspace_id: &str,
    cfg: &CompareRunConfig,
) -> AppResult<()> {
    // 1) 读取文档与分块
    ctx.progress("load", 0, 1, "读取文档分块");
    let docs: Vec<DocumentRow> = {
        let conn = ctx.db.get()?;
        cfg.document_ids
            .iter()
            .map(|id| {
                let d = document_repo::get(&conn, id)?;
                if d.workspace_id != workspace_id {
                    return Err(AppError::new(AppErrorCode::InvalidConfig, "文档不属于该工作区"));
                }
                if d.status != "parsed" {
                    return Err(AppError::new(
                        AppErrorCode::CompareFailed,
                        format!("「{}」尚未解析成功，无法参与比对", d.file_name),
                    ));
                }
                Ok(d)
            })
            .collect::<AppResult<Vec<_>>>()?
    };

    // 1.5) 招标文件对减（W3-2 + M4 豁免接线）：加载本工作区招标/补遗文档，一次产出四类对减依据——
    //   · winnowing 指纹索引（残差矩阵/覆盖率）；· rsid 集（含 rsidRoot，投标间共享但源自招标模板的
    //     rsid 不再触发 rsid 信号）；· 内嵌图片 sha256 集（各家照贴招标方效果图/区位图不再触发 imageReuse）；
    //   · 共同错误豁免 TenderExemption（源自招标文件的共同笔误不再触发 sharedErrors）。
    // 无招标文件或未开对减时为 None，全流程退化为原口径（残差=全量、豁免集恒空，逐字节不变）。
    let tender_refs: Option<TenderRefs> = if cfg.subtract_tender {
        let conn = ctx.db.get()?;
        let tenders =
            document_repo::list_by_role(&conn, workspace_id, &["tender", "tender_supplement"])?;
        let mut texts: Vec<String> = Vec::new();
        let mut rsids: HashSet<String> = HashSet::new();
        let mut image_hashes: HashSet<String> = HashSet::new();
        let mut ex_tokens: HashSet<String> = HashSet::new();
        let mut ex_text = String::new();
        for t in &tenders {
            if t.status != "parsed" {
                continue; // 未解析成功的招标文件不贡献豁免依据
            }
            // rsid 集（含 rsidRoot）：招标模板下发的修订会话标识各家天然共享，须先减去
            if let Some(fp) = t
                .fingerprint_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Fingerprint>(s).ok())
            {
                rsids.extend(fp.rsids);
                if let Some(root) = fp.rsid_root {
                    rsids.insert(root);
                }
            }
            // 内嵌图片 sha256：招标方统一提供的图片各家照贴属合规雷同
            for img in image_repo::list_images(&conn, &t.id)? {
                image_hashes.insert(img.sha256);
            }
            // 分块：winnow 指纹源（normalized）+ 共同错误豁免（token_json 与投标同口径、原文串供上下文子串匹配）
            for row in chunk_repo::load_for_compare(&conn, &t.id, &cfg.chunk_level)? {
                if let Some(toks) = row
                    .token_json
                    .as_deref()
                    .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                {
                    ex_tokens.extend(toks);
                }
                ex_text.push_str(&row.text);
                ex_text.push('\n');
                texts.push(row.normalized_text);
            }
        }
        // 查证质量闸门（§1.5）：任一解析成功的招标文件为 OCR/扫描件 → 索引不可信。
        let ocr = tenders
            .iter()
            .filter(|t| t.status == "parsed")
            .any(|t| t.parse_method.as_deref().is_some_and(|m| m.contains("ocr")));
        let index = winnow::TenderIndex::build(texts.iter().map(|s| s.as_str()));
        // 索引为空（无实质招标内容）视为豁免不可用：退化为无对减、floor 不启用。
        (!index.is_empty()).then_some(TenderRefs {
            index,
            rsids,
            image_hashes,
            error_exempt: TenderExemption { tokens: ex_tokens, normalized_text: ex_text },
            ocr,
        })
    } else {
        None
    };
    let tender_index: Option<&winnow::TenderIndex> = tender_refs.as_ref().map(|r| &r.index);

    // 内置静态范本背景库（W3-4）：随包版本化、固定语料、双阈值 4-gram DF；进程级单例只算一次。
    let bg = background::load();

    let mut comparable: Vec<CmpChunk> = Vec::new();
    // 豁免证据：招标引用块（kind='tender'，标记不删除仍进 comparable）与背景套话块
    // （kind='background'，从聚类剔除但落库置灰可解释）。随任务落库，供 UI/导出与人工复核。
    let mut exemptions: Vec<compare_repo::NewExemption> = Vec::new();
    {
        let conn = ctx.db.get()?;
        for (di, d) in docs.iter().enumerate() {
            let rows = chunk_repo::load_for_compare(&conn, &d.id, &cfg.chunk_level)?;
            let total = rows.len();
            for (rank, mut row) in rows.into_iter().enumerate() {
                // rel_pos 用稠密行序：order_index 由 chunker 连同 heading 块一起编号，而
                // load_for_compare 已排除 heading（order_index 有空洞），直接用会使 rel_pos>1.0。
                // rows 已按 order_index 排序，按加载顺序重编稠密序即可。
                row.order_index = rank as i64;
                // 招标覆盖率与背景占比都在 normalized_text 被 from_row 消费前算出（避免克隆全文）。
                let (coverage, spans) = match tender_index {
                    Some(idx) => winnow::coverage(&row.normalized_text, idx),
                    None => (0.0, Vec::new()),
                };
                // 背景占比：ignore_templates 开启时用于套话剔除；已导入招标文件时另需它做 k-共现
                // 出处查证（W3-3：≥3 家共有簇多数成员 boiler≥0.6 → background 豁免而非异常升级）。
                // 两者皆不涉及时不计算（0.0），保冷启动确定性与 CPU。
                let boiler = if cfg.ignore_templates || tender_index.is_some() {
                    bg.boiler_fraction(&row.normalized_text)
                } else {
                    0.0
                };
                let mut c = corpus::from_row(row, di, total);
                c.tender_coverage = coverage;
                c.boiler_fraction = boiler;
                // 比对范围（§5 W3-5）：business 家族含 legal/price，故 business 范围仍纳报价段。
                let keep_scope = zone_in_scope(&c.section_kind, &cfg.scope);
                if !keep_scope || c.tokens.is_empty() {
                    continue;
                }
                // ignore_templates 语义扩展（W3-4）：样板余弦命中 OR 背景加权命中 → 不进聚类。
                let bg_hit = boiler >= background::BOILER_FRACTION_EXEMPT;
                if cfg.ignore_templates && (c.is_template || bg_hit) {
                    // 背景加权命中：落 chunk_exemptions(kind='background') 证据（延续 is_template
                    // 「标记不删除」哲学——簇里不出现，但 chunks 行保留、库中可见可解释、UI 置灰可筛）。
                    // 纯样板余弦命中沿用 chunks.is_template 标记，不额外落库。
                    if bg_hit {
                        exemptions.push(compare_repo::NewExemption {
                            chunk_id: c.id.clone(),
                            kind: "background".into(),
                            coverage: boiler,
                            spans_json: "[]".into(),
                        });
                    }
                    continue;
                }
                // 保留块：招标引用块（coverage≥0.8）记 tender 证据，仍进 comparable（全量口径可见），
                // 残差口径靠边集减法剔除。
                if coverage >= winnow::COVERAGE_EXEMPT {
                    exemptions.push(compare_repo::NewExemption {
                        chunk_id: c.id.clone(),
                        kind: "tender".into(),
                        coverage,
                        spans_json: serde_json::to_string(&spans).unwrap_or_else(|_| "[]".into()),
                    });
                }
                comparable.push(c);
            }
        }
    }
    // 背景库剔除的取证审计线（含语料版本+篇数）：支撑「同库同输入可复现、可举证」。
    let background_exempt_count = exemptions.iter().filter(|e| e.kind == "background").count();
    if background_exempt_count > 0 {
        log::info!(
            "背景范本库剔除 {background_exempt_count} 块（语料 {} · {} 篇 · boilerplate {} / legal {} 个 4-gram）",
            background::CORPUS_VERSION,
            bg.doc_count(),
            bg.boiler_gram_count(),
            bg.legal_gram_count(),
        );
    }
    ctx.check()?;
    corpus::fill_tfidf(&mut comparable);

    // 2) 语义向量（可选；按 (normalized_hash, model_id) 全局缓存）
    let (embeddings, semantic_degraded) = if cfg.enable_semantic {
        let spec = embed::resolve(&cfg.embedding_model);
        embed_chunks(ctx, embedder, &comparable, spec, cfg.allow_model_download)?
    } else {
        (None, false)
    };
    ctx.check()?;

    // 3) 候选召回
    ctx.progress("recall", 0, 1, "候选召回");
    let params = candidate::RecallParams {
        top_k: cfg.candidate_top_k,
        // 高频 gram 停用阈值随语料规模放大，否则大语料下真雷同的模板化条款会被整体停用
        stop_gram_df: (comparable.len() / 10).max(256),
        ..Default::default()
    };
    let cands: Vec<(u32, u32)> = candidate::recall(&comparable, embeddings.as_deref(), &params)
        .into_iter()
        .collect();
    ctx.check()?;

    // 4) 五维精排（rayon fold/reduce + 周期检查取消）。
    // 打分即过滤：只保留 ≥ 阈值的边，候选量大时不再囤积百兆级低分边；
    // 低于阈值的信息只保留「每 chunk 的最高分」，供章节热力与 deleted 判定使用。
    let total_pairs = cands.len().max(1);
    ctx.progress("score", 0, total_pairs, "精排打分");
    let done = AtomicUsize::new(0);
    let threshold = cfg.similarity_threshold;
    // 软种子（W4-2 链化用）：final_score ∈ [有效阈值−SOFT_SEED_BAND, 有效阈值) 的边，仅供 align 链化
    // 提升连续性，【不入】candidate_edges、不参与聚类。仅 enable_alignment 时收集（否则空转，逐字节不变）。
    let collect_soft = cfg.enable_alignment;
    #[allow(clippy::type_complexity)]
    let (edges, soft_seeds, best): (Vec<ScoredEdge>, Vec<ScoredEdge>, HashMap<u32, f32>) = cands
        .par_iter()
        .fold(
            || (Vec::new(), Vec::new(), HashMap::new()),
            |(mut es, mut softs, mut best), &(i, j)| {
                let n = done.fetch_add(1, Ordering::Relaxed);
                if n.is_multiple_of(512) {
                    ctx.progress("score", n, total_pairs, format!("已精排 {n} / {total_pairs}"));
                }
                let sem = embeddings.as_ref().and_then(|e| {
                    match (e[i as usize].as_ref(), e[j as usize].as_ref()) {
                        (Some(a), Some(b)) => Some(embed::cosine(a, b)),
                        _ => None,
                    }
                });
                let parts =
                    scoring::score_pair(&comparable[i as usize], &comparable[j as usize], sem);
                for idx in [i, j] {
                    let e = best.entry(idx).or_insert(0.0f32);
                    *e = e.max(parts.final_score);
                }
                // 短文本对用上浮后的阈值过滤（§9.5 场景化阈值）
                let eff = effective_threshold(threshold, &comparable[i as usize], &comparable[j as usize]);
                if parts.final_score >= eff {
                    es.push(ScoredEdge { a: i, b: j, parts });
                } else if collect_soft && parts.final_score >= eff - align::SOFT_SEED_BAND {
                    softs.push(ScoredEdge { a: i, b: j, parts });
                }
                (es, softs, best)
            },
        )
        .reduce(
            || (Vec::new(), Vec::new(), HashMap::new()),
            |(mut e1, mut s1, mut b1), (e2, s2, b2)| {
                e1.extend(e2);
                s1.extend(s2);
                for (k, v) in b2 {
                    let e = b1.entry(k).or_insert(0.0f32);
                    *e = e.max(v);
                }
                (e1, s1, b1)
            },
        );
    ctx.check()?;

    // 4.5) 逐字雷同区间（W4-1 铁证层 + W3 残差桥接，M5a）：paragraph 级原文求跨文档极大公共子串。
    //   独立的新证据层——不消费 edges/聚类，直接读 paragraph 分块建 SAM（较短文档侧），只报去空白后
    //   一字不差的长串（高精度低召回）。W3 桥接：完全落在引用招标块（tender_coverage≥0.8）或样板块
    //   （ignore_templates 开启）内的区间丢弃——对招标条款的合法逐字应答不得以「铁证」形态还魂。
    ctx.progress("verbatim", 0, 1, "逐字雷同区间");
    let verbatim_rows = compute_verbatim(ctx, &docs, cfg, tender_index)?;
    ctx.check()?;

    // 5) 聚类（招标对减双口径）：
    //  · 残差簇 `raw`：剔除「双方 coverage≥0.8」的边（双方都在逐字引用招标 → 合法共享），
    //    驱动分类/diff/围标/主矩阵与风险分级（剔除后口径）。
    //  · 全量簇 `raw_full`：不对减，仅供 matrixOriginal/peakOriginal（原始相似度）。
    //  无对减时残差即全量，raw_full=None，主矩阵直接复用（避免重复聚类，逐字节不变）。
    ctx.progress("cluster", 0, 1, "聚合雷同条款");
    let exempt = |i: u32| comparable[i as usize].tender_coverage >= winnow::COVERAGE_EXEMPT;
    let residual_edges: Option<Vec<ScoredEdge>> = tender_index.as_ref().map(|_| {
        edges
            .iter()
            .filter(|e| !(exempt(e.a) && exempt(e.b)))
            .map(|e| ScoredEdge { a: e.a, b: e.b, parts: e.parts })
            .collect()
    });
    let mut raw = clustering::cluster(
        &comparable,
        residual_edges.as_deref().unwrap_or(&edges),
        cfg.similarity_threshold,
    );
    raw.truncate(MAX_STORE_CLUSTERS);
    let raw_full: Option<Vec<clustering::RawCluster>> = residual_edges.as_ref().map(|_| {
        let mut rf = clustering::cluster(&comparable, &edges, cfg.similarity_threshold);
        rf.truncate(MAX_STORE_CLUSTERS);
        rf
    });

    // 5.5) 对齐区段链化（W4-2 seed-chain-align，M5a）：残差边∪软种子∪逐字锚点 → 连续对齐区段。
    //   新增证据层、不替代聚类：区段是文档对粒度的证据成型，经 chunk_id 与聚类互链。
    //   W3 桥接：种子喂【残差边】（residual_edges，剔除双方引用招标的合法共享边），软种子同口径
    //   残差过滤，verbatim 锚点已在 verbatim 层丢弃完全落在豁免块的区间——铁证不以合法共享还魂。
    let segment_rows: Vec<compare_repo::NewSegment> = if cfg.enable_alignment {
        ctx.progress("align", 0, 1, "对齐区段");
        let seed_edges: &[ScoredEdge] = residual_edges.as_deref().unwrap_or(&edges);
        let soft_use: Vec<ScoredEdge> = if tender_index.is_some() {
            soft_seeds
                .into_iter()
                .filter(|e| !(exempt(e.a) && exempt(e.b)))
                .collect()
        } else {
            soft_seeds
        };
        let vseeds: Vec<align::VerbatimSeed> = verbatim_rows
            .iter()
            .map(|m| align::VerbatimSeed {
                a_chunk_id: m.a_start_chunk_id.clone(),
                b_chunk_id: m.b_start_chunk_id.clone(),
                char_len: m.char_len.max(0) as usize,
            })
            .collect();
        let segments = align::chain(&comparable, seed_edges, &soft_use, &vseeds);
        // chunk_id → 原文（gap 细化取文本）。
        let text_of: HashMap<&str, &str> =
            comparable.iter().map(|c| (c.id.as_str(), c.text.as_str())).collect();
        // W4-3 区段内 gap 带状字符级细化：并行细化每区段的 gap（锚点间未命中块），按 Σeq_chars 把
        // 覆盖率从「锚点覆盖」升级为「细化后真实覆盖」，并组装 segment_diffs 行。eq 双侧对称（相同文本
        // 等长）→ 两侧同量累加，封顶区间总字符。全空 gap 不产 DiffOp（align 侧已不产，此处再滤空）。
        segments
            .into_par_iter()
            .map(|s| {
                let gap_texts: Vec<(String, String)> = s
                    .gaps
                    .iter()
                    .map(|g| {
                        let a: String = g
                            .a_chunk_ids
                            .iter()
                            .filter_map(|id| text_of.get(id.as_str()).copied())
                            .collect();
                        let b: String = g
                            .b_chunk_ids
                            .iter()
                            .filter_map(|id| text_of.get(id.as_str()).copied())
                            .collect();
                        (a, b)
                    })
                    .collect();
                let refined = diff::refine_segment_gaps(jieba, &gap_texts);
                let extra: usize = refined.iter().map(|r| r.eq_chars).sum();
                let a_covered = (s.a_covered_chars + extra).min(s.a_span);
                let b_covered = (s.b_covered_chars + extra).min(s.b_span);
                let a_coverage =
                    if s.a_span > 0 { (a_covered as f32 / s.a_span as f32).min(1.0) } else { 0.0 };
                let b_coverage =
                    if s.b_span > 0 { (b_covered as f32 / s.b_span as f32).min(1.0) } else { 0.0 };
                let diffs: Vec<compare_repo::NewSegmentDiff> = s
                    .gaps
                    .iter()
                    .zip(refined.iter())
                    .filter(|(_, r)| !r.ops.is_empty())
                    .map(|(g, r)| compare_repo::NewSegmentDiff {
                        a_chunk_id: g.a_chunk_ids.first().cloned(),
                        b_chunk_id: g.b_chunk_ids.first().cloned(),
                        diff_type: r.diff_type.to_string(),
                        diff_json: serde_json::to_string(&r.ops).unwrap_or_else(|_| "[]".into()),
                        eq_chars: r.eq_chars as i64,
                    })
                    .collect();
                compare_repo::NewSegment {
                    doc_a_id: docs[s.doc_a].id.clone(),
                    doc_b_id: docs[s.doc_b].id.clone(),
                    a_start_order: s.a_start_order as i64,
                    a_end_order: s.a_end_order as i64,
                    b_start_order: s.b_start_order as i64,
                    b_end_order: s.b_end_order as i64,
                    a_start_chunk_id: s.a_start_chunk_id.clone(),
                    a_end_chunk_id: s.a_end_chunk_id.clone(),
                    b_start_chunk_id: s.b_start_chunk_id.clone(),
                    b_end_chunk_id: s.b_end_chunk_id.clone(),
                    anchor_count: s.anchor_count as i64,
                    verbatim_chars: s.verbatim_chars as i64,
                    a_covered_chars: a_covered as i64,
                    b_covered_chars: b_covered as i64,
                    a_coverage,
                    b_coverage,
                    avg_score: s.avg_score,
                    a_section_path: s.a_section_path.clone(),
                    b_section_path: s.b_section_path.clone(),
                    a_page_start: s.a_page_start.map(|p| p as i64),
                    a_page_end: s.a_page_end.map(|p| p as i64),
                    b_page_start: s.b_page_start.map(|p| p as i64),
                    b_page_end: s.b_page_end.map(|p| p as i64),
                    anchors: s
                        .anchors
                        .iter()
                        .map(|a| compare_repo::NewSegmentAnchor {
                            a_chunk_id: a.a_chunk_id.clone(),
                            b_chunk_id: a.b_chunk_id.clone(),
                            kind: a.kind.as_str().to_string(),
                            score: a.score,
                        })
                        .collect(),
                    diffs,
                }
            })
            .collect()
    } else {
        Vec::new()
    };
    ctx.check()?;

    // 6) 分类 + diff + 组装入库结构
    let base_idx = cfg
        .base_document_id
        .as_ref()
        .and_then(|id| docs.iter().position(|d| d.id == *id));
    let mut new_clusters =
        build_clusters(jieba, &comparable, &docs, &raw, base_idx, cfg.detect_moved_paragraph);

    // 6.2) k-共现过滤升级（W3-3）：对 docs_present≥3 的每个残差簇逐成员查两库——命中招标/背景库
    //   → 写 exempt_reason（合法共享，退出信号②/残差矩阵/high 统计，簇保留落库置灰可筛）；
    //   两查皆空且查证质量闸门通过 → multi_doc_anomaly=1（『多家异常一致·待复核』，§1.5 不自动 high）。
    // 查证质量闸门（§1.5）：招标文件已导入（tender_refs=Some）且非 OCR/扫描件（!ocr）且对减覆盖率
    // 抽样达标（max tender_coverage ≥ FLOOR）时才允许升级；否则降级中性提示、不引法条、不升 severity。
    // 无招标文件（tender_refs=None）→ 完全不标记，维持既有行为（冷启动/合成 docset 逐字节不变）。
    let anomaly_gate_open = match tender_refs.as_ref() {
        Some(r) if !r.ocr => {
            comparable.iter().map(|c| c.tender_coverage).fold(0.0f32, f32::max)
                >= ANOMALY_COVERAGE_SAMPLE_FLOOR
        }
        _ => false,
    };
    apply_shared_exemptions(
        &comparable,
        &raw,
        &mut new_clusters,
        tender_refs.is_some(),
        anomaly_gate_open,
    );

    // 6.5) 事实抽取与冲突检测：量化字段（金额/工期/日期/比例）跨文档不一致 → conflict
    let mut fact_rows: Vec<(String, fact::Fact)> = Vec::new();
    if cfg.enable_fact_conflict {
        ctx.progress("facts", 0, 1, "事实冲突检测");
        apply_fact_conflicts(&comparable, &raw, &mut new_clusters, &mut fact_rows);
    }

    // 基准模式：基准文档中无任何近似命中的分块 → deleted 单块条目
    let deleted = if let Some(bi) = base_idx {
        build_deleted(&comparable, &docs, &raw, &best, bi)
    } else {
        Vec::new()
    };

    // 7) 单事务落库（边在打分阶段已按阈值过滤）
    ctx.progress("persist", 0, 1, "保存比对结果");
    {
        // 持进程级写锁：与导入 persist 共用一把，避免跨任务/跨工作区并发写撞 SQLITE_BUSY。
        let _w = ctx.write_lock();
        let mut conn = ctx.db.get()?;
        let tx = conn.transaction()?;
        let kept_edges: Vec<NewEdge> = edges
            .iter()
            .map(|e| NewEdge {
                source_chunk_id: comparable[e.a as usize].id.clone(),
                target_chunk_id: comparable[e.b as usize].id.clone(),
                parts: e.parts,
            })
            .collect();
        compare_repo::insert_edges(&tx, &ctx.job_id, &kept_edges)?;
        compare_repo::insert_clusters(&tx, &ctx.job_id, &new_clusters)?;
        compare_repo::insert_clusters(&tx, &ctx.job_id, &deleted)?;
        compare_repo::insert_exemptions(&tx, &ctx.job_id, &exemptions)?;
        compare_repo::insert_verbatim_matches(&tx, &ctx.job_id, &verbatim_rows)?;
        compare_repo::insert_segments(&tx, &ctx.job_id, &segment_rows)?;
        crate::db::repo::fact_repo::replace_for_chunks(&tx, &fact_rows)?;
        tx.commit()?;
    }
    ctx.check()?;

    // 8) 聚合：矩阵 / 章节热力 / 共有特征词 / 围标判定 / 总览
    // 主矩阵（残差·剔除后）：风险分级与围标信号①的唯一输入。k-共现豁免簇（引用招标/行业范本）
    // 属合法共享，不得抬升残差矩阵/peak——从矩阵聚合剔除（异常簇仍保留：其为真嫌疑）。
    // 无豁免簇时直接复用 raw（零克隆，冷启动/合成 docset 逐字节不变）。
    let any_exempt = new_clusters.iter().any(|c| c.exempt_reason.is_some());
    let (m, peak) = if any_exempt {
        let kept: Vec<clustering::RawCluster> = raw
            .iter()
            .zip(new_clusters.iter())
            .filter(|(_, nc)| nc.exempt_reason.is_none())
            .map(|(rc, _)| rc.clone())
            .collect();
        matrix::doc_matrix(docs.len(), &comparable, &kept)
    } else {
        matrix::doc_matrix(docs.len(), &comparable, &raw)
    };
    // 原始矩阵（未对减）：仅供对照展示。无对减时与主矩阵同源，直接复用。
    let (m_original, peak_original) = match &raw_full {
        Some(rf) => matrix::doc_matrix(docs.len(), &comparable, rf),
        None => (m.clone(), peak),
    };
    let sections = section_stats(&comparable, &best);
    let shared = shared_terms_of(&comparable);
    // 共同错误指纹（词典外词/异常标点/错误引用）：跑在已有内存分块上，产出 kind="sharedErrors"
    // 的 SharedTerm，与罕见词共用 shared_terms_json 通道。招标文件笔误豁免（M4 接线）：源自招标
    // 文件的共同笔误/词元/悬空引用在检测侧减去，不再触发 sharedErrors。
    let shared_errors =
        shared_error_fingerprints(jieba, &comparable, tender_refs.as_ref().map(|r| &r.error_exempt));

    let mut doc_infos: Vec<DocInfo> = docs
        .iter()
        .map(|d| DocInfo {
            id: d.id.clone(),
            name: d.file_name.clone(),
            doc_type: d.file_type.clone(),
            pages: d.page_count.unwrap_or(0) as u32,
            char_count: d.char_count.unwrap_or(0) as usize,
            fingerprint: d
                .fingerprint_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Fingerprint>(s).ok())
                .unwrap_or_default(),
            parse_error: None,
            // M2 规避：从 documents.evasion_json 判级出的规避特征摘要（清白文档 evasion_json
            // 为 NULL → None，不做「检查通过」背书）
            evasion: d.evasion_json.as_deref().and_then(EvasionSummary::from_evasion_json),
        })
        .collect();
    fingerprint::cross_flags(&mut doc_infos);
    // rsid 交集同源命中（M4 接线）：先减去招标文件模板的 rsid 集（含 rsidRoot），投标间共享但
    // 源自招标模板的 rsid 不再触发 rsid 信号；无招标文件时豁免集为空，行为不变。
    let empty_hashes: HashSet<String> = HashSet::new();
    let exempt_rsids: &HashSet<String> =
        tender_refs.as_ref().map(|r| &r.rsids).unwrap_or(&empty_hashes);
    let rsid_hits = fingerprint::rsid_pairs(&mut doc_infos, exempt_rsids);
    // PDF 血缘命中：trailer ID/XMP GUID（硬）、字体子集标签（中）；弱命中并入 metadata 标记
    let lineage_hits = fingerprint::lineage_pairs(&mut doc_infos);
    // 内嵌图片同源命中：加载参评文档图片指纹（与 docs 同序），两两跨文档碰撞
    // （sha256 精确 / dHash 近似）。M4 接线：先减去招标文件图片 sha256 集，各家照贴招标方统一
    // 提供的效果图/区位图不再触发 imageReuse；无招标文件时豁免集为空，行为不变。
    let doc_images: Vec<Vec<collusion::ImageFp>> = {
        let conn = ctx.db.get()?;
        docs.iter()
            .map(|d| {
                image_repo::list_images(&conn, &d.id).map(|imgs| {
                    imgs.into_iter()
                        .map(|r| collusion::ImageFp {
                            sha256: r.sha256,
                            dhash: r.dhash,
                            page: r.page,
                        })
                        .collect()
                })
            })
            .collect::<AppResult<Vec<_>>>()?
    };
    let exempt_hashes: &HashSet<String> =
        tender_refs.as_ref().map(|r| &r.image_hashes).unwrap_or(&empty_hashes);
    let image_hits = collusion::image_pairs(&doc_images, exempt_hashes);

    // 围标判定复用旧引擎的信号加权（输入适配为 report::Cluster）。
    // exempted/anomaly 取自 apply_shared_exemptions 落在 new_clusters 上的标记（与 raw 同序）：
    // 豁免簇退出信号②，异常簇归入独立 multiDocAnomaly 信号（§1.5：不自动 high）。
    let r_clusters: Vec<RCluster> = raw
        .iter()
        .zip(new_clusters.iter())
        .map(|(rc, nc)| {
            let docs_set: BTreeSet<usize> =
                rc.members.iter().map(|&i| comparable[i as usize].doc).collect();
            RCluster {
                avg_score: rc.avg,
                peak: rc.peak,
                docs: docs_set.into_iter().collect(),
                segments: rc
                    .members
                    .iter()
                    .map(|&i| ClusterSeg {
                        doc: comparable[i as usize].doc,
                        text: comparable[i as usize].text.clone(),
                    })
                    .collect(),
                exempted: nc.exempt_reason.is_some(),
                anomaly: nc.multi_doc_anomaly,
            }
        })
        .collect();
    let r_shared: Vec<SharedTerm> = shared.clone();
    // 报价梯度信号：金额接近但不同 + 多处条款雷同（典型陪标价特征）
    let price_pairs = price_proximity(&comparable, docs.len(), &raw);
    // M2 规避：各文档规避特征摘要（与 doc_infos/docs 同序），独立信号在 FORENSIC_CAP 之外
    let evasion: Vec<Option<EvasionSummary>> = doc_infos.iter().map(|d| d.evasion.clone()).collect();
    let collusion = collusion::assess_with(collusion::CollusionInputs {
        peak,
        clusters: &r_clusters,
        docs: &doc_infos,
        shared_terms: &r_shared,
        price_pairs: &price_pairs,
        rsid_hits: &rsid_hits,
        lineage_hits: &lineage_hits,
        image_hits: &image_hits,
        shared_errors: &shared_errors,
        evasion: &evasion,
        // 条件化硬命中 floor 启用前提（§1.5）：招标文件已导入且豁免对减已生效
        // （tender_refs=Some ⇒ 上述三处豁免均已作用于本轮信号提取）。
        tender_exemption_active: tender_refs.is_some(),
    });

    let mut summary = CompareSummary {
        document_count: docs.len(),
        chunk_count: comparable.len(),
        cluster_count: new_clusters.len() + deleted.len(),
        semantic_degraded,
        tender_ref_chunk_count: exemptions.iter().filter(|e| e.kind == "tender").count(),
        background_exempt_chunk_count: background_exempt_count,
        ..Default::default()
    };
    for c in new_clusters.iter().chain(deleted.iter()) {
        match c.cluster_type.as_str() {
            "same" => summary.same_count += 1,
            "minor_change" => summary.minor_change_count += 1,
            "rewrite" => summary.rewrite_count += 1,
            "changed" => summary.changed_count += 1,
            "added" => summary.added_count += 1,
            "deleted" => summary.deleted_count += 1,
            "conflict" => summary.conflict_count += 1,
            _ => summary.uncertain_count += 1,
        }
        // high 风险统计剔除 k-共现豁免簇（合法共享，§1.5）；异常簇 severity='review' 天然不计。
        if c.severity == "high" && c.exempt_reason.is_none() {
            summary.high_risk_count += 1;
        }
        // 五区簇计数（§5 W3-5）：每簇归一区，五者之和恒等于 cluster_count。
        match zone_slot(c.section_kind.as_deref()) {
            0 => summary.zone_legal_count += 1,
            1 => summary.zone_price_count += 1,
            2 => summary.zone_tech_count += 1,
            3 => summary.zone_business_count += 1,
            _ => summary.zone_other_count += 1,
        }
    }

    // 区段口径矩阵（W4-4，M5）：由对齐区段细化后覆盖字数聚合，与残差主矩阵同分母、同剔除口径
    // （区段种子已喂残差边、逐字锚点已丢弃招标豁免块）。展示层切换用（Matrix.tsx Pill）；围标信号①
    // 仍消费 peak（剔除后聚类口径），待校准语料回测后再决定是否切 segmentPeak。mode 反映默认展示口径：
    // 有区段时 "segment"，否则 "cluster"（无区段时 segmentMatrix 全 0，前端亦回退聚类口径）。
    let doc_index: HashMap<&str, usize> =
        docs.iter().enumerate().map(|(i, d)| (d.id.as_str(), i)).collect();
    let seg_cov: Vec<matrix::SegCoverage> = segment_rows
        .iter()
        .filter_map(|s| {
            Some(matrix::SegCoverage {
                doc_a: *doc_index.get(s.doc_a_id.as_str())?,
                doc_b: *doc_index.get(s.doc_b_id.as_str())?,
                a_covered_chars: s.a_covered_chars,
                b_covered_chars: s.b_covered_chars,
            })
        })
        .collect();
    let (seg_matrix, seg_peak) = matrix::doc_matrix_segments(docs.len(), &comparable, &seg_cov);
    let matrix_mode = if seg_cov.is_empty() { "cluster" } else { "segment" };

    // 附录 A 冻结 schema：matrix(剔除后·主口径) + matrixOriginal(未对减) + segmentMatrix(区段口径)
    // + peak/peakOriginal/segmentPeak + mode。旧任务缺新键 → 前端走缺省渲染。
    let matrix_json = serde_json::json!({
        "documentIds": cfg.document_ids,
        "matrix": m,
        "peak": peak,
        "matrixOriginal": m_original,
        "peakOriginal": peak_original,
        "segmentMatrix": seg_matrix,
        "segmentPeak": seg_peak,
        "mode": matrix_mode,
    });
    // 罕见词 + 共同错误指纹并入同一 shared_terms_json 通道（错误条目 kind="sharedErrors"）
    let mut shared_out = shared;
    shared_out.extend(shared_errors);
    {
        let conn = ctx.db.get()?;
        job_repo::set_compare_results(
            &conn,
            &ctx.job_id,
            &serde_json::to_string(&summary).unwrap_or_default(),
            &matrix_json.to_string(),
            &serde_json::to_string(&collusion).unwrap_or_default(),
            &serde_json::to_string(&shared_out).unwrap_or_default(),
            &serde_json::to_string(&sections).unwrap_or_default(),
        )?;
    }
    ctx.progress("done", 1, 1, "完成");
    Ok(())
}

/// 逐字雷同区间计算（W4-1 + W3 桥接）：为每份参评文档加载 paragraph 级原文分块，逐块预置豁免标记
/// （引用招标 tender_coverage≥0.8 或 ignore_templates 下的样板块），交 verbatim::find_pairwise 求
/// 跨文档极大公共子串，映射回 (document_id, chunk 锚点) 的落库行。逐字层固定 paragraph 粒度、不受
/// cfg.chunk_level 影响；覆盖率复用 M4 的 winnow::TenderIndex（与残差口径同源）。
fn compute_verbatim(
    ctx: &JobCtx,
    docs: &[DocumentRow],
    cfg: &CompareRunConfig,
    tender_index: Option<&winnow::TenderIndex>,
) -> AppResult<Vec<compare_repo::NewVerbatim>> {
    let vdocs: Vec<verbatim::VbDoc> = {
        let conn = ctx.db.get()?;
        docs.iter()
            .map(|d| {
                let chunks = chunk_repo::load_texts(&conn, &d.id, "paragraph")?
                    .into_iter()
                    .map(|r| {
                        let coverage = match tender_index {
                            Some(idx) => winnow::coverage(&r.normalized_text, idx).0,
                            None => 0.0,
                        };
                        let exempt = (cfg.ignore_templates && r.is_template)
                            || coverage >= winnow::COVERAGE_EXEMPT;
                        verbatim::VbChunk { id: r.id, text: r.text, exempt }
                    })
                    .collect();
                Ok(verbatim::VbDoc { chunks })
            })
            .collect::<AppResult<Vec<_>>>()?
    };
    let min = cfg.verbatim_min_chars.max(1);
    let rows = verbatim::find_pairwise(&vdocs, min)
        .into_iter()
        .map(|m| compare_repo::NewVerbatim {
            doc_a_id: docs[m.doc_a].id.clone(),
            doc_b_id: docs[m.doc_b].id.clone(),
            a_start_chunk_id: m.a_start_chunk_id,
            a_start_offset: m.a_start_offset as i64,
            a_end_chunk_id: m.a_end_chunk_id,
            a_end_offset: m.a_end_offset as i64,
            b_start_chunk_id: m.b_start_chunk_id,
            b_start_offset: m.b_start_offset as i64,
            b_end_chunk_id: m.b_end_chunk_id,
            b_end_offset: m.b_end_offset as i64,
            char_len: m.char_len as i64,
            sample_text: m.sample_text,
        })
        .collect();
    Ok(rows)
}

/// 语义向量：唯一 normalized_hash 查缓存 → 缺失的批量嵌入并回写。
/// 模型不可用（含 allowCloudModel=false 且本地无缓存）时优雅降级
/// （返回 degraded=true，比对退回词面权重组）。
fn embed_chunks(
    ctx: &JobCtx,
    embedder: &Arc<Mutex<embed::LoadedEmbedder>>,
    chunks: &[CmpChunk],
    spec: &embed::EmbedModelSpec,
    allow_download: bool,
) -> AppResult<(Option<ChunkEmbeddings>, bool)> {
    let mut uniq: HashMap<&str, &str> = HashMap::new(); // hash → text
    for c in chunks {
        uniq.entry(&c.normalized_hash).or_insert(&c.text);
    }
    let hashes: Vec<String> = uniq.keys().map(|s| s.to_string()).collect();
    let mut cache = {
        let conn = ctx.db.get()?;
        embedding_repo::get_many(&conn, &hashes, spec.id)?
    };

    let missing: Vec<(String, String)> = uniq
        .iter()
        .filter(|(h, _)| !cache.contains_key(**h))
        .map(|(h, t)| (h.to_string(), t.to_string()))
        .collect();

    let total = missing.len().max(1);
    ctx.progress("semantic", 0, total, format!("语义向量（缓存命中 {}）", cache.len()));

    if !missing.is_empty() {
        // 比对路径【禁止隐式下载】：下载可达数分钟且此处持 embedder 锁不可取消，会让「取消比对」
        // 卡在 cancelling。故 ensure 传 allow_download=false——已加载/已缓存的模型正常使用，未缓存
        // 则降级并提示去工具屏预下载。unwrap_or_else 做毒化恢复，避免一次 panic 后语义功能永久失效。
        let mut guard = embedder.lock().unwrap_or_else(|e| e.into_inner());
        let Some(model) = embed::ensure(&mut guard, spec, false) else {
            let msg = if allow_download {
                "语义模型未缓存，已降级为词面比对；请在工具屏预下载该模型后重试"
            } else {
                "语义模型不可用（离线且无缓存），降级为词面比对"
            };
            ctx.progress("semantic", total, total, msg);
            return Ok((None, true));
        };
        for (bi, batch) in missing.chunks(EMBED_BATCH).enumerate() {
            ctx.check()?;
            let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
            let Some(vecs) = embed::embed_batch(model, &texts, spec.id) else {
                ctx.progress("semantic", total, total, "语义嵌入失败，降级为词面比对");
                return Ok((None, true));
            };
            let items: Vec<(String, Vec<f32>)> = batch
                .iter()
                .zip(vecs)
                .map(|((h, _), v)| (h.clone(), v))
                .collect();
            {
                // embedding 缓存回写也走进程级写锁，与导入/比对 persist 共用一把，避免跨工作区
                // 并发比对时此写与他任务的 persist 事务争 SQLite 单写者撞 SQLITE_BUSY。
                let _w = ctx.write_lock();
                let conn = ctx.db.get()?;
                embedding_repo::insert_many(&conn, &items, spec.id)?;
            }
            for (h, v) in items {
                cache.insert(h, v);
            }
            ctx.progress(
                "semantic",
                ((bi + 1) * EMBED_BATCH).min(total),
                total,
                format!("语义向量 {} / {}", ((bi + 1) * EMBED_BATCH).min(total), total),
            );
        }
    }

    let embs: Vec<Option<Vec<f32>>> = chunks
        .iter()
        .map(|c| cache.get(&c.normalized_hash).cloned())
        .collect();
    Ok((Some(embs), false))
}

/// RawCluster → 入库结构：分类、topic/summary、成员角色、各文档 primary 间的分级 diff。
#[allow(clippy::too_many_arguments)] // 聚类结果组装的固有输入（语料/文档/原始簇/基准/开关）
fn build_clusters(
    jieba: &Jieba,
    chunks: &[CmpChunk],
    docs: &[DocumentRow],
    raw: &[clustering::RawCluster],
    base_idx: Option<usize>,
    detect_moved: bool,
) -> Vec<NewCluster> {
    raw.iter()
        .map(|rc| {
            let member_chunks: Vec<&CmpChunk> =
                rc.members.iter().map(|&i| &chunks[i as usize]).collect();
            let all_same_hash = member_chunks
                .windows(2)
                .all(|w| w[0].normalized_hash == w[1].normalized_hash);
            let class = diff::classify_cluster(rc.avg, rc.min_pair, all_same_hash, rc.lex_avg, rc.sem_avg);

            let docs_present: BTreeSet<usize> = member_chunks.iter().map(|c| c.doc).collect();
            // 基准模式：基准文档缺席的条款 → added
            let (cluster_type, severity) = match base_idx {
                Some(bi) if !docs_present.contains(&bi) => ("added", "low"),
                _ => (class.cluster_type, class.severity),
            };

            // primary 成员（按文档序），diff 以最靠前文档（或基准）的 primary 为底版
            let mut primaries: Vec<&CmpChunk> = rc
                .members
                .iter()
                .filter(|m| rc.roles.get(m) == Some(&"primary"))
                .map(|&i| &chunks[i as usize])
                .collect();
            primaries.sort_by_key(|c| c.doc);
            let base_chunk = base_idx
                .and_then(|bi| primaries.iter().find(|c| c.doc == bi).copied())
                .or_else(|| primaries.first().copied());

            let topic = member_chunks
                .iter()
                .find_map(|c| c.section_path.last().cloned())
                .or_else(|| {
                    member_chunks.first().map(|c| {
                        let head: String = c.text.chars().take(18).collect();
                        if c.text.chars().count() > 18 { format!("{head}…") } else { head }
                    })
                });
            // 移动段落标注（detectMovedParagraph）：内容雷同但出现位置差异大
            // （跨文档 primary 的相对位置极差 > 0.25）→ 雷同之外还刻意挪了位置
            let moved = detect_moved
                && matches!(cluster_type, "same" | "minor_change")
                && {
                    let pos: Vec<f32> = rc
                        .members
                        .iter()
                        .filter(|m| rc.roles.get(m) == Some(&"primary"))
                        .map(|&i| chunks[i as usize].rel_pos)
                        .collect();
                    pos.len() >= 2 && {
                        let max = pos.iter().cloned().fold(f32::MIN, f32::max);
                        let min = pos.iter().cloned().fold(f32::MAX, f32::min);
                        max - min > 0.25
                    }
                };
            let summary = Some(format!(
                "{} 份文档 · 平均相似 {:.0}%{}",
                docs_present.len(),
                rc.avg * 100.0,
                if moved { " · 位置移动" } else { "" }
            ));
            // 多数成员的标段作为条款标段
            let mut kind_counts: HashMap<&str, usize> = HashMap::new();
            for c in &member_chunks {
                *kind_counts.entry(c.section_kind.as_str()).or_insert(0) += 1;
            }
            let section_kind = kind_counts
                .into_iter()
                .max_by_key(|(_, n)| *n)
                .map(|(k, _)| k.to_string());

            let members: Vec<NewMember> = rc
                .members
                .iter()
                .map(|&i| {
                    let c = &chunks[i as usize];
                    NewMember {
                        document_id: docs[c.doc].id.clone(),
                        chunk_id: c.id.clone(),
                        role: rc.roles.get(&i).copied().unwrap_or("primary").to_string(),
                        score: rc
                            .pair_scores
                            .iter()
                            .filter(|((a, b), _)| *a == i || *b == i)
                            .map(|(_, s)| *s)
                            .fold(None, |acc: Option<f32>, s| Some(acc.map_or(s, |a| a.max(s)))),
                    }
                })
                .collect();

            let diffs: Vec<NewDiff> = match base_chunk {
                Some(base) => primaries
                    .iter()
                    .filter(|c| c.id != base.id)
                    .map(|c| {
                        // 表格行对走列对齐 diff（§9.8），其余按长度分级
                        let (granularity, ops) = if base.is_table_row && c.is_table_row {
                            ("table", diff::table_row_diff(&base.text, &c.text))
                        } else {
                            diff::graded_diff(jieba, &base.text, &c.text)
                        };
                        NewDiff {
                            base_chunk_id: Some(base.id.clone()),
                            target_chunk_id: Some(c.id.clone()),
                            diff_type: granularity.to_string(),
                            diff_json: serde_json::to_string(&ops).unwrap_or_else(|_| "[]".into()),
                            summary: None,
                        }
                    })
                    .collect(),
                None => Vec::new(),
            };

            // 底版位置：列表行内直达「章节 + 页码」
            let base_section_path = base_chunk
                .filter(|c| !c.section_path.is_empty())
                .map(|c| c.section_path.join(" › "));
            let base_page = base_chunk.and_then(|c| c.page).map(|p| p as i64);

            NewCluster {
                cluster_type: cluster_type.to_string(),
                topic,
                summary,
                severity: severity.to_string(),
                score: rc.avg,
                section_kind,
                conflict_json: None,
                base_section_path,
                base_page,
                exempt_reason: None,
                multi_doc_anomaly: false,
                members,
                diffs,
            }
        })
        .collect()
}

/// k-共现过滤升级（W3-3）：对 docs_present≥3 的每个残差簇逐成员查两库并标记（raw 与 clusters
/// 一一对应，build_clusters 保序映射）。
///   · 多数成员 tender_coverage≥0.8 → exempt_reason='tender'（引用招标文件的合法共享）；
///   · 否则多数成员 boiler_fraction≥0.6 → exempt_reason='background'（行业范本套话）；
///   · 两查皆空且 anomaly_gate_open → multi_doc_anomaly=1、severity='review'（『待复核』），
///     summary 追加「涉嫌…需评标委员会依法认定」（§1.5：不自动 high、不进 high 统计）；
///   · 两查皆空但仅 tender_present（闸门未过：招标件 OCR/覆盖率过低）→ 中性提示，不升级；
///   · 无招标文件 → 不标记，维持既有行为。
/// 恰好 2 家共有的簇（docs_present<3）一律不动。
fn apply_shared_exemptions(
    chunks: &[CmpChunk],
    raw: &[clustering::RawCluster],
    clusters: &mut [NewCluster],
    tender_present: bool,
    anomaly_gate_open: bool,
) {
    for (rc, nc) in raw.iter().zip(clusters.iter_mut()) {
        let members: Vec<&CmpChunk> = rc.members.iter().map(|&i| &chunks[i as usize]).collect();
        let docs_present: BTreeSet<usize> = members.iter().map(|c| c.doc).collect();
        if docs_present.len() < MULTI_DOC_MIN {
            continue;
        }
        let n = members.len() as f32;
        let tender_frac = members
            .iter()
            .filter(|c| c.tender_coverage >= winnow::COVERAGE_EXEMPT)
            .count() as f32
            / n;
        let bg_frac = members
            .iter()
            .filter(|c| c.boiler_fraction >= background::BOILER_FRACTION_EXEMPT)
            .count() as f32
            / n;
        if tender_frac > SHARED_EXEMPT_MAJORITY {
            nc.exempt_reason = Some("tender".into());
        } else if bg_frac > SHARED_EXEMPT_MAJORITY {
            nc.exempt_reason = Some("background".into());
        } else if anomaly_gate_open {
            nc.multi_doc_anomaly = true;
            nc.severity = SEVERITY_REVIEW.into();
            let base = nc.summary.take().unwrap_or_default();
            nc.summary = Some(format!("{base}{ANOMALY_SUMMARY_SUFFIX}"));
        } else if tender_present {
            let base = nc.summary.take().unwrap_or_default();
            nc.summary = Some(format!("{base}{NEUTRAL_SUMMARY_SUFFIX}"));
        }
    }
}

/// 事实冲突：对每个跨文档 cluster 的 primary 成员抽取事实，量化字段不一致 → conflict。
/// raw 与 clusters 一一对应（build_clusters 保序映射）。
fn apply_fact_conflicts(
    chunks: &[CmpChunk],
    raw: &[clustering::RawCluster],
    clusters: &mut [NewCluster],
    fact_rows: &mut Vec<(String, fact::Fact)>,
) {
    for (rc, nc) in raw.iter().zip(clusters.iter_mut()) {
        let primaries: Vec<&CmpChunk> = rc
            .members
            .iter()
            .filter(|m| rc.roles.get(m) == Some(&"primary"))
            .map(|&i| &chunks[i as usize])
            .collect();
        let facts: Vec<(usize, fact::Fact)> = primaries
            .iter()
            .map(|c| (c.doc, fact::extract(&c.text, &c.entities)))
            .collect();
        for (c, (_, f)) in primaries.iter().zip(&facts) {
            fact_rows.push((c.id.clone(), f.clone()));
        }
        // added（基准缺席）也照样检测：基准没有该条款，但 B、C 之间数字不一致同样是风险；
        // conflict 标签比 added 更可执行，允许覆盖
        let refs: Vec<(usize, &fact::Fact)> = facts.iter().map(|(d, f)| (*d, f)).collect();
        if let Some(conflict) = fact::conflicts_between(&refs) {
            nc.cluster_type = "conflict".into();
            nc.severity = conflict.risk.clone();
            let labels: Vec<&str> = conflict
                .fields
                .iter()
                .map(|f| match f.field.as_str() {
                    "amount" => "金额",
                    "duration" => "工期",
                    "date" => "日期",
                    "percentage" => "比例",
                    "subject" => "责任主体",
                    _ => "其他",
                })
                .collect();
            // 纯主体冲突（甲方↔乙方互换）不是数字，措辞不能写「关键数字不一致」，否则举证描述失真
            let has_numeric = conflict
                .fields
                .iter()
                .any(|f| matches!(f.field.as_str(), "amount" | "duration" | "date" | "percentage"));
            let head = if has_numeric {
                "同一条款关键数字不一致"
            } else {
                "同一条款责任主体不一致"
            };
            nc.summary = Some(format!("{head}（{}）", labels.join("、")));
            nc.conflict_json = serde_json::to_string(&conflict).ok();
        }
    }
}

/// 非报价金额语境：这些词所在分块的金额不计入「投标价」（常见劫持全文最大值的大额）。
/// 用负向排除而非正向锚词同块：投标总价常在报价表数据行(与"总价"表头/标题分处不同 chunk)，
/// 正向同块会整份漏掉表格式报价；排除法既保住表格报价，又滤掉注册资本/业绩/保证金。
const PRICE_EXCLUDE: &[&str] = &[
    "注册资本", "注册资金", "实收资本", "净资产", "总资产", "资产总额",
    "营业收入", "营业额", "年产值", "合同额", "业绩", "纳税", "保证金",
];

/// 报价梯度：每文档取「排除非报价语境后」的最大金额作为投标价，
/// 两文档共享 ≥3 个雷同条款且报价差 0 < gap < 3% → 信号。
fn price_proximity(
    chunks: &[CmpChunk],
    n_docs: usize,
    raw: &[clustering::RawCluster],
) -> Vec<collusion::PriceProximity> {
    let mut max_amount: Vec<Option<u64>> = vec![None; n_docs];
    for c in chunks {
        // 排除注册资本/业绩/保证金等非报价大额所在的块，避免劫持全文最大值
        if PRICE_EXCLUDE.iter().any(|w| c.text.contains(w)) {
            continue;
        }
        for e in &c.entities {
            if e.kind == "amount" {
                // 实体来自归一化文本：「3200万元」在导入期已展开为「32000000元」，
                // 这里直接取前缀数字即可
                let digits: String = e.value.chars().take_while(|ch| ch.is_ascii_digit()).collect();
                if let Ok(v) = digits.parse::<u64>() {
                    if max_amount[c.doc].is_none_or(|cur| v > cur) {
                        max_amount[c.doc] = Some(v);
                    }
                }
            }
        }
    }

    let mut overlap: HashMap<(usize, usize), u32> = HashMap::new();
    for rc in raw {
        let docs: BTreeSet<usize> = rc.members.iter().map(|&i| chunks[i as usize].doc).collect();
        let v: Vec<usize> = docs.into_iter().collect();
        for (x, &a) in v.iter().enumerate() {
            for &b in &v[x + 1..] {
                *overlap.entry((a, b)).or_insert(0) += 1;
            }
        }
    }

    let mut out = Vec::new();
    for ((a, b), n) in overlap {
        if n < 3 {
            continue;
        }
        if let (Some(ma), Some(mb)) = (max_amount[a], max_amount[b]) {
            if ma == mb || ma == 0 || mb == 0 {
                continue;
            }
            let gap = (ma.abs_diff(mb)) as f32 / ma.max(mb) as f32;
            if gap < 0.03 {
                out.push(collusion::PriceProximity {
                    a,
                    b,
                    amount_a: ma,
                    amount_b: mb,
                    gap_pct: gap,
                });
            }
        }
    }
    out.sort_by(|x, y| x.gap_pct.partial_cmp(&y.gap_pct).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// 基准模式的 deleted：基准文档中没有任何 ≥ DELETED_FLOOR（§9.5「不匹配」带的上界）
/// 近似命中的分块。score 记录该分块见到的最高近似分，便于人工复核可追踪。
fn build_deleted(
    chunks: &[CmpChunk],
    docs: &[DocumentRow],
    raw: &[clustering::RawCluster],
    best: &HashMap<u32, f32>,
    base_idx: usize,
) -> Vec<NewCluster> {
    let clustered: HashSet<u32> = raw.iter().flat_map(|rc| rc.members.iter().copied()).collect();
    chunks
        .iter()
        .enumerate()
        .filter(|(i, c)| {
            c.doc == base_idx
                && !clustered.contains(&(*i as u32))
                && best.get(&(*i as u32)).copied().unwrap_or(0.0) < DELETED_FLOOR
        })
        .take(MAX_DELETED_CLUSTERS)
        .map(|(i, c)| {
            let head: String = c.text.chars().take(18).collect();
            let nearest = best.get(&(i as u32)).copied().unwrap_or(0.0);
            NewCluster {
                cluster_type: "deleted".into(),
                topic: c.section_path.last().cloned().or(Some(head)),
                summary: Some("基准文档独有内容，其他文档未出现".into()),
                severity: "low".into(),
                score: nearest,
                section_kind: Some(c.section_kind.clone()),
                conflict_json: None,
                base_section_path: if c.section_path.is_empty() {
                    None
                } else {
                    Some(c.section_path.join(" › "))
                },
                base_page: c.page.map(|p| p as i64),
                exempt_reason: None,
                multi_doc_anomaly: false,
                members: vec![NewMember {
                    document_id: docs[base_idx].id.clone(),
                    chunk_id: c.id.clone(),
                    role: "primary".into(),
                    score: Some(nearest),
                }],
                diffs: Vec::new(),
            }
        })
        .collect()
}

/// 章节热力：每文档每标段的最大跨文档相似度 + 命中片段数（≥0.5 计数）。
/// best 为打分阶段累计的每 chunk 最高分（含低于阈值的边）。
fn section_stats(chunks: &[CmpChunk], best: &HashMap<u32, f32>) -> Vec<SectionStat> {
    let mut acc: HashMap<(usize, &str), (f32, u32)> = HashMap::new();
    for (i, c) in chunks.iter().enumerate() {
        let b = best.get(&(i as u32)).copied().unwrap_or(0.0);
        let entry = acc.entry((c.doc, c.section_kind.as_str())).or_insert((0.0, 0));
        entry.0 = entry.0.max(b);
        if b >= 0.5 {
            entry.1 += 1;
        }
    }
    let mut out: Vec<SectionStat> = acc
        .into_iter()
        .map(|((doc, kind), (intensity, matches))| SectionStat {
            doc,
            section: kind.to_string(),
            intensity,
            matches,
        })
        .collect();
    out.sort_by_key(|s| (s.doc, s.section.clone()));
    out
}

/// 共有特征词：≥4 字、被 ≥2 份文档共用、且足够罕见的词（疑似同源 / 共用笔误），top 30。
/// 罕见度过滤：出现在超过 ~20% 分块的词是通用模板词（「技术方案」「项目管理」等），
/// 必然被多份文档共用却无同源指示意义，剔除以免该信号沦为常开噪声。
fn shared_terms_of(chunks: &[CmpChunk]) -> Vec<SharedTerm> {
    let total = chunks.len().max(1);
    let common_ceil = (total / 5).max(3);
    let mut docs_of: HashMap<&str, BTreeSet<usize>> = HashMap::new();
    let mut chunk_df: HashMap<&str, usize> = HashMap::new();
    for c in chunks {
        let mut seen: HashSet<&str> = HashSet::new();
        for t in &c.tokens {
            if t.chars().count() >= 4 {
                docs_of.entry(t.as_str()).or_default().insert(c.doc);
                if seen.insert(t.as_str()) {
                    *chunk_df.entry(t.as_str()).or_insert(0) += 1;
                }
            }
        }
    }
    let mut out: Vec<SharedTerm> = docs_of
        .into_iter()
        .filter(|(term, docs)| {
            docs.len() >= 2 && chunk_df.get(*term).copied().unwrap_or(0) <= common_ceil
        })
        .map(|(term, docs)| SharedTerm {
            term: term.to_string(),
            docs: docs.into_iter().collect(),
            ..Default::default()
        })
        .collect();
    out.sort_by(|a, b| {
        b.docs
            .len()
            .cmp(&a.docs.len())
            .then(b.term.chars().count().cmp(&a.term.chars().count()))
    });
    out.truncate(30);
    out
}

/// 招标文件豁免（M4 招标对减接线预留）：招标文件本身的笔误/词元各家照抄不算串标
/// （调研 §13 反向豁免——错误内容一致的证明力恰恰依赖「不是各家都抄同一份母本」）。
/// `tokens` 供词典外词豁免，`normalized_text` 供异常标点/引用错误指纹的子串豁免。
/// 当前 run_compare 恒传 None；W3/M4 招标文件角色落地后由其解析产物填充。
pub struct TenderExemption {
    pub tokens: HashSet<String>,
    pub normalized_text: String,
}

/// 单字是否落在基本 CJK 统一表意区（标书正文足够，不含扩展区）。
fn is_han(c: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&c)
}
fn all_han(s: &str) -> bool {
    !s.is_empty() && s.chars().all(is_han)
}

/// 稀有度归一分 ∈ (0,1]：共有文档数越少、块频越低越罕见（最罕见=仅 2 文档且块频 2 → 1.0）。
/// ⚠️ 未经语料校准：仅供 collusion 连续特征加权，非定性依据。
fn error_rarity(doc_count: usize, block_df: usize) -> f32 {
    let d = (2.0 / doc_count.max(2) as f32).min(1.0);
    let b = (2.0 / block_df.max(2) as f32).min(1.0);
    d * b
}

/// 异常标点检测：在 chunk 原始文本上返回 (错误串, 前后各 2 字上下文)。三类可定位模式——
/// 叠标点（。。/，，）、全半角混用（。. / ，,）、中文间夹半角空格。上下文用于跨文档同指纹比对
/// 与人工核对。括号不配对无可定位的错误串+上下文指纹，本条不纳入（避免无锚点误报）。
fn punctuation_errors(text: &str) -> Vec<(String, String)> {
    const DUP: [char; 7] = ['。', '，', '、', '；', '：', '？', '！'];
    let half_of = |c: char| -> Option<char> {
        match c {
            '。' => Some('.'),
            '，' => Some(','),
            '；' => Some(';'),
            '：' => Some(':'),
            '？' => Some('?'),
            '！' => Some('!'),
            _ => None,
        }
    };
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();
    let ctx_of = |s: usize, e: usize| -> String {
        let lo = s.saturating_sub(2);
        let hi = (e + 2).min(n);
        chars[lo..hi].iter().collect()
    };
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + 1 < n {
        let a = chars[i];
        let b = chars[i + 1];
        if a == b && DUP.contains(&a) {
            out.push((format!("{a}{b}"), ctx_of(i, i + 2)));
            i += 1;
            continue;
        }
        let mix = half_of(a) == Some(b) || half_of(b) == Some(a);
        if mix {
            out.push((format!("{a}{b}"), ctx_of(i, i + 2)));
            i += 1;
            continue;
        }
        if b == ' ' && i + 2 < n && is_han(a) && is_han(chars[i + 2]) {
            out.push((format!("{a} {}", chars[i + 2]), ctx_of(i, i + 3)));
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

/// 抽取「第X章/第X节/第X条/附表X/X.Y节」等章节引用串（供跨文档悬空引用比对）。
fn reference_targets(text: &str) -> Vec<String> {
    static RES: std::sync::OnceLock<Vec<regex::Regex>> = std::sync::OnceLock::new();
    let res = RES.get_or_init(|| {
        vec![
            regex::Regex::new(r"第[0-9一二三四五六七八九十百零]+[章节条款]").unwrap(),
            regex::Regex::new(r"附表[0-9一二三四五六七八九十]+").unwrap(),
            regex::Regex::new(r"[0-9]+(?:\.[0-9]+)+\s*节").unwrap(),
        ]
    });
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for re in res {
        for m in re.find_iter(text) {
            let s = m.as_str().to_string();
            if seen.insert(s.clone()) {
                out.push(s);
            }
        }
    }
    out
}

/// 共同错误指纹：三类检测器跑在已有内存分块（CmpChunk 的 text/tokens/entities/section_path）
/// 上，零额外 IO。统一产出 SharedTerm{kind="sharedErrors", rarity, context}，与罕见词共用
/// jobs.shared_terms_json 通道。共用同一处罕见错误比共用正确词证明力高一个量级
/// （调研 §5/§13：identical wrong answers），但词典外 ≠ 错别字（行业新词/专有名词/型号亦在词典外），
/// 故只作「疑似」提示、detail 附上下文供人工判断，不直接定性。
/// - (a) 词典外词：token ≥2 字、全中文、jieba.has_word()==false、非实体、块频 ≤3、≥2 文档共有；
/// - (b) 异常标点：叠标点/全半角混用/中文间夹半角空格，指纹=错误串+前后 2 字，跨文档同指纹；
/// - (c) 引用错误：抽章节引用，目标不在本文档标题树、却在 ≥2 份文档以相同字串出现
///   （无标题层级的 PDF/纯文本按 doc 标题树为空降级跳过，避免标题树稀疏导致全量误报）。
///
/// exempt：招标文件豁免（当前恒 None，M4 接线）。结果按稀有度降序，上限 30 条。
fn shared_error_fingerprints(
    jieba: &Jieba,
    chunks: &[CmpChunk],
    exempt: Option<&TenderExemption>,
) -> Vec<SharedTerm> {
    const OOD_BLOCK_DF_MAX: usize = 3;
    let mut out: Vec<SharedTerm> = Vec::new();

    // —— (a) 词典外词 ——
    {
        let mut docs_of: HashMap<&str, BTreeSet<usize>> = HashMap::new();
        let mut chunk_df: HashMap<&str, usize> = HashMap::new();
        for c in chunks {
            let mut seen: HashSet<&str> = HashSet::new();
            for t in &c.tokens {
                if t.chars().count() < 2 || !all_han(t) {
                    continue;
                }
                if jieba.has_word(t) {
                    continue; // 词典内词：正常词，不视为疑似错误
                }
                if c.entities.iter().any(|e| e.value.contains(t.as_str())) {
                    continue; // 金额/日期/百分比/工期等实体不算错词
                }
                if exempt.map(|ex| ex.tokens.contains(t.as_str())).unwrap_or(false) {
                    continue; // 招标文件原生词/笔误各家照抄，豁免
                }
                docs_of.entry(t.as_str()).or_default().insert(c.doc);
                if seen.insert(t.as_str()) {
                    *chunk_df.entry(t.as_str()).or_insert(0) += 1;
                }
            }
        }
        for (term, docs) in &docs_of {
            let df = chunk_df.get(term).copied().unwrap_or(0);
            if docs.len() >= 2 && df <= OOD_BLOCK_DF_MAX {
                out.push(SharedTerm {
                    term: (*term).to_string(),
                    docs: docs.iter().copied().collect(),
                    kind: Some("sharedErrors".into()),
                    rarity: Some(error_rarity(docs.len(), df)),
                    context: None,
                });
            }
        }
    }

    // —— (b) 异常标点 —— 以「错误串+上下文」为跨文档同指纹的聚合键
    {
        let mut agg: HashMap<String, (String, BTreeSet<usize>, usize)> = HashMap::new();
        for c in chunks {
            let mut seen: HashSet<String> = HashSet::new();
            for (err, ctx) in punctuation_errors(&c.text) {
                if exempt.map(|ex| ex.normalized_text.contains(&ctx)).unwrap_or(false) {
                    continue;
                }
                let e = agg.entry(ctx.clone()).or_insert_with(|| (err, BTreeSet::new(), 0));
                e.1.insert(c.doc);
                if seen.insert(ctx) {
                    e.2 += 1;
                }
            }
        }
        for (ctx, (term, docs, df)) in agg {
            if docs.len() >= 2 {
                out.push(SharedTerm {
                    term,
                    docs: docs.iter().copied().collect(),
                    kind: Some("sharedErrors".into()),
                    rarity: Some(error_rarity(docs.len(), df)),
                    context: Some(ctx),
                });
            }
        }
    }

    // —— (c) 引用错误（悬空章节引用的跨文档共现）——
    {
        let mut titles: HashMap<usize, HashSet<String>> = HashMap::new();
        for c in chunks {
            let set = titles.entry(c.doc).or_default();
            for t in &c.section_path {
                set.insert(t.clone());
            }
        }
        let mut agg: HashMap<String, (BTreeSet<usize>, usize)> = HashMap::new();
        for c in chunks {
            let doc_titles = match titles.get(&c.doc) {
                Some(s) if !s.is_empty() => s, // 降级：无标题层级的文档不做引用错误检测
                _ => continue,
            };
            let mut seen: HashSet<String> = HashSet::new();
            for r in reference_targets(&c.text) {
                if doc_titles.iter().any(|t| t.contains(&r)) {
                    continue; // 引用目标在本文档标题树中 → 正常引用
                }
                if exempt.map(|ex| ex.normalized_text.contains(&r)).unwrap_or(false) {
                    continue;
                }
                let e = agg.entry(r.clone()).or_insert_with(|| (BTreeSet::new(), 0));
                e.0.insert(c.doc);
                if seen.insert(r) {
                    e.1 += 1;
                }
            }
        }
        for (r, (docs, df)) in agg {
            if docs.len() >= 2 {
                out.push(SharedTerm {
                    term: r.clone(),
                    docs: docs.iter().copied().collect(),
                    kind: Some("sharedErrors".into()),
                    rarity: Some(error_rarity(docs.len(), df)),
                    context: Some(format!("悬空引用「{r}」在本文档标题树中无对应目标")),
                });
            }
        }
    }

    out.sort_by(|a, b| {
        b.rarity
            .partial_cmp(&a.rarity)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.docs.len().cmp(&a.docs.len()))
    });
    out.truncate(30);
    out
}

// 端到端：导入（import_service）→ 比对（本服务）→ 校验断言从旧引擎测试逐条平移。
// 这组测试是阶段 3 的「校准门禁」：权重/阈值改动必须保证正负向同时通过。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::{segment_repo, workspace_repo};
    use crate::db::{open_in_memory, DbPool};
    use crate::jobs::progress::CollectSink;
    use crate::services::import_service;
    use std::sync::atomic::AtomicBool;

    fn ctx_for(pool: &DbPool, ws: &str, job_type: &str, cancelled: bool) -> JobCtx {
        let conn = pool.get().unwrap();
        let job = job_repo::create(&conn, ws, job_type, None, "{}").unwrap();
        drop(conn);
        JobCtx::for_test(
            job.id,
            job_type.into(),
            pool.clone(),
            Arc::new(AtomicBool::new(cancelled)),
            Arc::new(CollectSink::default()),
        )
    }

    fn cfg_with(ids: Vec<String>, threshold: f32) -> CompareRunConfig {
        CompareRunConfig {
            document_ids: ids,
            base_document_id: None,
            chunk_level: "paragraph".into(),
            similarity_threshold: threshold,
            candidate_top_k: 100,
            enable_semantic: false,
            enable_fact_conflict: false,
            ignore_templates: true,
            detect_moved_paragraph: true,
            scope: "full".into(),
            subtract_tender: true,
            embedding_model: "e5-small".into(),
            allow_model_download: false,
            verbatim_min_chars: 30,
            enable_alignment: true,
        }
    }

    /// 写文件 → 导入 → 跑比对，返回 (job_id, 按入参顺序的 document_ids)。
    fn import_and_compare(
        pool: &DbPool,
        ws: &str,
        files: &[(&str, String)],
        threshold: f32,
    ) -> (String, Vec<String>) {
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_cmp_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<String> = files
            .iter()
            .map(|(name, content)| {
                let p = dir.join(name);
                std::fs::write(&p, content).unwrap();
                p.to_string_lossy().into_owned()
            })
            .collect();
        let ictx = ctx_for(pool, ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), ws, &paths, &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, ws).unwrap();
        let ids: Vec<String> = files
            .iter()
            .map(|(name, _)| docs.iter().find(|d| d.file_name == *name).unwrap().id.clone())
            .collect();
        drop(conn);

        let cctx = ctx_for(pool, ws, "compare", false);
        let cfg = cfg_with(ids.clone(), threshold);
        run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), ws, &cfg).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        (cctx.job_id, ids)
    }

    fn matrix_peak(pool: &DbPool, job_id: &str) -> (Vec<Vec<f32>>, f32) {
        let conn = pool.get().unwrap();
        let r = job_repo::get_result_jsons(&conn, job_id).unwrap();
        let v: serde_json::Value = serde_json::from_str(&r.matrix_json.unwrap()).unwrap();
        let m: Vec<Vec<f32>> = serde_json::from_value(v["matrix"].clone()).unwrap();
        (m, v["peak"].as_f64().unwrap() as f32)
    }

    fn clusters_of(
        pool: &DbPool,
        job_id: &str,
    ) -> Vec<crate::db::repo::compare_repo::ClusterSummaryRow> {
        let conn = pool.get().unwrap();
        compare_repo::list_clusters(&conn, job_id, &Default::default(), 0, 500).unwrap()
    }

    /// 逐字铁证层端到端（W4-1 + M5a 接线）：导入两份共享一整段 120 字逐字文本、前后段落各异的
    /// 标书 → 跑 compare → verbatim_matches 落库正确（单块锚点、char_len=120、sample 与原文一致），
    /// 且 delete_job_results 后无残留（复用取消清理口径）。走真实 load_texts/coverage/落库路径，
    /// 补齐纯函数单测未覆盖的 DB 接线。
    #[test]
    fn verbatim_evidence_persisted_end_to_end() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "逐字铁证").unwrap().id
        };
        // 一整段 120 个连续 CJK 字符（块内无重复、块间可拼接），两份文档逐字相同
        let shared: String = (0..120u32).map(|k| char::from_u32(0x4E00 + k).unwrap()).collect();
        let a = format!("甲方投标文件封面独有内容第一段落文字\n\n{shared}\n\n甲方独有的结尾段落说明文字内容");
        let b = format!("乙方投标单位另类开头段落表述\n\n{shared}\n\n乙方独有收束段落陈述文本");
        let (job_id, ids) = import_and_compare(
            &pool,
            &ws,
            &[("甲.txt", a), ("乙.txt", b)],
            0.5,
        );

        let conn = pool.get().unwrap();
        let ms = compare_repo::list_verbatim_for_pair(&conn, &job_id, &ids[0], &ids[1]).unwrap();
        assert_eq!(ms.len(), 1, "应恰好落库 1 条逐字区间");
        let m = &ms[0];
        assert_eq!(m.char_len, 120);
        assert_eq!(m.sample_text, shared, "sample 应为去空白后的逐字文本");
        assert_eq!(m.a_start_chunk_id, m.a_end_chunk_id, "整段落命中 → 起止同块");
        assert_eq!(m.a_start_offset, 0);
        assert_eq!(m.a_end_offset, 120);
        // 锚点回指真实块：块内 char 切片 [start,end) 应还原逐字文本
        let text: String =
            conn.query_row("SELECT text FROM chunks WHERE id=?1", [&m.a_start_chunk_id], |r| {
                r.get(0)
            })
            .unwrap();
        let sliced: String = text
            .chars()
            .skip(m.a_start_offset as usize)
            .take((m.a_end_offset - m.a_start_offset) as usize)
            .collect();
        assert_eq!(sliced, shared, "块内偏移应精确锚定原文");
        // 方向无关查询：交换 a/b 仍命中同一条
        assert_eq!(
            compare_repo::list_verbatim_for_pair(&conn, &job_id, &ids[1], &ids[0]).unwrap().len(),
            1
        );
        drop(conn);

        // 清理口径：delete_job_results 后逐字区间无残留（取消/重跑安全）
        {
            let conn = pool.get().unwrap();
            compare_repo::delete_job_results(&conn, &job_id).unwrap();
            let after =
                compare_repo::list_verbatim_for_pair(&conn, &job_id, &ids[0], &ids[1]).unwrap();
            assert!(after.is_empty(), "delete_job_results 后 verbatim_matches 应清空");
        }
    }

    /// 对齐区段链化端到端（W4-2 + M5a 接线）：导入两份共享一整块连续 6 段（各异段落、逐字相同）
    /// 的标书 → 跑 compare → aligned_segments 落库为一条覆盖连续段的区段，segment_anchors 的
    /// chunk_id 能反查到 cluster_members（区段↔聚类经 chunk 互链），逐字锚点计入 verbatim_chars，
    /// 且 delete_job_results 后两表无残留。走真实残差边/软种子/verbatim 接线，补齐纯函数单测未覆盖的
    /// DB 通路。
    #[test]
    fn aligned_segments_persisted_end_to_end() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "对齐区段").unwrap().id
        };
        // 6 个各异真实段落（真实词汇 → 非空 token 进 comparable；段间不同 → 独立 chunk），
        // 两份文档该整块逐字相同 → hash 通道召回每段、连续 6 锚点链成一条区段。
        let shared_block = [
            "本工程施工组织设计依据现行国家标准规范以及招标文件的具体要求编制完成",
            "项目部配备专职安全员负责施工现场的安全生产管理与隐患排查治理工作",
            "主体结构采用框架剪力墙体系混凝土强度等级严格满足设计图纸相关要求",
            "施工现场实行封闭式管理进出人员及车辆均须登记并佩戴安全防护用品",
            "质量保证体系覆盖材料进场检验隐蔽工程验收及分部分项工程质量评定",
            "工程竣工后我方提供不少于两年的质量保修服务并建立回访跟踪机制",
        ]
        .join("\n\n");
        let a = format!("甲方投标文件封面独有内容第一段落说明文字信息\n\n{shared_block}\n\n甲方独有的结尾段落补充说明文字内容");
        let b = format!("乙方投标单位另类开头段落陈述表达文本信息\n\n{shared_block}\n\n乙方独有收束段落总结陈述内容表述");
        let (job_id, ids) =
            import_and_compare(&pool, &ws, &[("甲.txt", a), ("乙.txt", b)], 0.5);

        let conn = pool.get().unwrap();
        // 区段行
        let mut stmt = conn
            .prepare(
                "SELECT id, doc_a_id, doc_b_id, anchor_count, verbatim_chars, a_covered_chars,
                 a_coverage, b_coverage FROM aligned_segments WHERE job_id = ?1",
            )
            .unwrap();
        let segs: Vec<(String, String, String, i64, i64, i64, f64, f64)> = stmt
            .query_map([&job_id], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        drop(stmt);
        assert_eq!(segs.len(), 1, "共享连续块应成恰好 1 条区段");
        let (seg_id, doc_a_id, doc_b_id, anchor_count, verbatim_chars, a_covered, a_cov, b_cov) =
            &segs[0];
        assert_eq!((doc_a_id.as_str(), doc_b_id.as_str()), (ids[0].as_str(), ids[1].as_str()));
        assert!(*anchor_count >= 2, "区段锚点数应 ≥2，实际 {anchor_count}");
        assert!(*a_covered > 0);
        assert!(*a_cov > 0.0 && *a_cov <= 1.0 + 1e-6, "a_coverage 应 ∈(0,1]，实际 {a_cov}");
        assert!(*b_cov > 0.0 && *b_cov <= 1.0 + 1e-6);
        assert!(*verbatim_chars > 0, "逐字相同段落应计入 verbatim_chars，实际 {verbatim_chars}");

        // 锚点落库 + 区段↔聚类经 chunk 互链：至少一条锚点的 a_chunk_id 出现在 cluster_members。
        let anchor_chunks: Vec<String> = conn
            .prepare("SELECT a_chunk_id FROM segment_anchors WHERE segment_id = ?1")
            .unwrap()
            .query_map([seg_id], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(!anchor_chunks.is_empty(), "区段应有锚点落库");
        let linked = anchor_chunks.iter().any(|cid| {
            conn.query_row(
                "SELECT COUNT(*) FROM cluster_members m JOIN clusters cl ON cl.id = m.cluster_id
                 WHERE cl.job_id = ?1 AND m.chunk_id = ?2",
                rusqlite::params![job_id, cid],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
                > 0
        });
        assert!(linked, "区段锚点 chunk 应可反查到 cluster_members（互链）");

        // 区段口径矩阵（W4-4）：matrix_json 含 segmentMatrix/segmentPeak/mode；对角线为 1、
        // 峰值 ∈(0,1]、有区段时 mode="segment"。
        let mv: serde_json::Value = {
            let r = job_repo::get_result_jsons(&conn, &job_id).unwrap();
            serde_json::from_str(&r.matrix_json.unwrap()).unwrap()
        };
        let sm: Vec<Vec<f32>> = serde_json::from_value(mv["segmentMatrix"].clone()).unwrap();
        assert!((sm[0][0] - 1.0).abs() < 1e-6 && (sm[1][1] - 1.0).abs() < 1e-6, "segmentMatrix 对角线应为 1");
        let sp = mv["segmentPeak"].as_f64().unwrap();
        assert!(sp > 0.0 && sp <= 1.0 + 1e-6, "segmentPeak 应 ∈(0,1]，实际 {sp}");
        assert!((sm[0][1] as f64 - sp).abs() < 1e-4, "两文档时 segmentPeak 即该对相似度");
        assert_eq!(mv["mode"].as_str(), Some("segment"), "有区段时默认展示口径为 segment");

        // 读侧数据通路（验收 1）：list_segments 非空、get_segment_detail 与落库一致、cluster 可反查。
        let listed =
            segment_repo::list_segments(&conn, &job_id, Some(&ids[0]), Some(&ids[1])).unwrap();
        assert_eq!(listed.len(), 1, "list_segments 应返回该文档对的 1 条区段");
        let detail = segment_repo::get_segment_detail(&conn, seg_id).unwrap();
        assert_eq!(detail.anchors.len() as i64, *anchor_count, "详情锚点数应与落库一致");
        assert!(!detail.a_chunks.is_empty() && !detail.b_chunks.is_empty(), "双栏跨度应有 chunk");
        assert!(!detail.verbatims.is_empty(), "逐字相同段落应有落在跨度内的逐字区间");
        assert!(!detail.cluster_ids.is_empty(), "锚点 chunk 应反查到关联 cluster");
        // 旧任务（无区段数据）读路径：不相干任务 id 返回空数组，不报错。
        assert!(segment_repo::list_segments(&conn, "job-none", None, None).unwrap().is_empty());
        drop(conn);

        // 清理口径：delete_job_results 后区段与锚点无残留。
        {
            let conn = pool.get().unwrap();
            compare_repo::delete_job_results(&conn, &job_id).unwrap();
            let seg_after: i64 = conn
                .query_row("SELECT COUNT(*) FROM aligned_segments WHERE job_id=?1", [&job_id], |r| {
                    r.get(0)
                })
                .unwrap();
            let anc_after: i64 =
                conn.query_row("SELECT COUNT(*) FROM segment_anchors", [], |r| r.get(0)).unwrap();
            assert_eq!(seg_after, 0, "delete_job_results 后 aligned_segments 应清空");
            assert_eq!(anc_after, 0, "区段清空后 segment_anchors 应随 FK 级联清空");
        }
    }

    /// 区段 gap 带状细化端到端（W4-3 + M5a 接线）：两份文档共享 6 段（逐字相同→连续锚点），
    /// 但甲方在第 3、4 段之间【独有插入】一段乙方没有的内容 → 该段落成为锚点之间一个「A 侧非空、
    /// B 侧空」的 gap。验证：细化产出 1 条 segment_diffs（diff_type=gap-sentence、全 del、eq_chars=0）、
    /// 细化后 a_coverage <1.0（插入段计入 a_span 却未被覆盖，方向正确不虚高）、b_coverage≈1.0、
    /// list_segment_diffs 读回一致、delete_job_results 后 segment_diffs 随区段级联清空。
    /// 全等锚点相邻处不产 gap（否则 segment_diffs 会 >1）——即验收「全空 gap 不产 DiffOp」。
    #[test]
    fn segment_gap_diffs_persisted_end_to_end() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "区段细化").unwrap().id
        };
        let shared = [
            "本工程施工组织设计依据现行国家标准规范以及招标文件的具体要求编制完成",
            "项目部配备专职安全员负责施工现场的安全生产管理与隐患排查治理工作",
            "主体结构采用框架剪力墙体系混凝土强度等级严格满足设计图纸相关要求",
            "施工现场实行封闭式管理进出人员及车辆均须登记并佩戴安全防护用品",
            "质量保证体系覆盖材料进场检验隐蔽工程验收及分部分项工程质量评定",
            "工程竣工后我方提供不少于两年的质量保修服务并建立回访跟踪机制",
        ];
        // 甲方在第 3、4 段之间独有插入一段（乙方无、且与乙方任何段落均不相似）→ 形成 gap。
        let extra = "本节为甲方单独补充的施工进度专项激励承诺条款其余投标单位概不涉及此项内容";
        let a_middle = [shared[0], shared[1], shared[2], extra, shared[3], shared[4], shared[5]]
            .join("\n\n");
        let b_middle = shared.join("\n\n");
        let a = format!("甲方投标文件封面独有内容第一段落说明文字信息\n\n{a_middle}\n\n甲方独有的结尾段落补充说明文字内容");
        let b = format!("乙方投标单位另类开头段落陈述表达文本信息\n\n{b_middle}\n\n乙方独有收束段落总结陈述内容表述");
        let (job_id, _ids) =
            import_and_compare(&pool, &ws, &[("甲.txt", a), ("乙.txt", b)], 0.5);

        let conn = pool.get().unwrap();
        let seg: (String, f64, f64) = conn
            .query_row(
                "SELECT id, a_coverage, b_coverage FROM aligned_segments WHERE job_id = ?1",
                [&job_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        let (seg_id, a_cov, b_cov) = seg;
        // 细化后覆盖率：插入段计入 a_span 却未被覆盖 → a_coverage 明显 <1.0；b 侧无 gap → ≈1.0。
        assert!(a_cov < 0.99, "插入段应压低 a_coverage（未虚高），实际 {a_cov}");
        assert!(b_cov > 0.99, "B 侧无 gap，b_coverage 应≈1.0，实际 {b_cov}");

        let diffs = compare_repo::list_segment_diffs(&conn, &seg_id).unwrap();
        assert_eq!(diffs.len(), 1, "仅一个非空 gap（插入段）应产 1 条 segment_diffs");
        let d = &diffs[0];
        assert_eq!(d.diff_type, "gap-sentence");
        assert_eq!(d.eq_chars, 0, "B 侧无对应文本 → 无相同字符");
        // A 侧独有插入 → 全 del；过滤后可还原插入段原文
        let ops: serde_json::Value = serde_json::from_str(&d.diff_json).unwrap();
        let arr = ops.as_array().unwrap();
        assert!(!arr.is_empty());
        assert!(arr.iter().all(|o| o["op"] == "del"), "纯插入 gap 应全为 del：{arr:?}");
        let restored: String =
            arr.iter().map(|o| o["text"].as_str().unwrap()).collect();
        assert_eq!(restored, extra, "del 文本应还原甲方插入段");
        drop(conn);

        // 清理口径：delete_job_results 后 segment_diffs 随区段 FK 级联清空。
        {
            let conn = pool.get().unwrap();
            compare_repo::delete_job_results(&conn, &job_id).unwrap();
            let after: i64 =
                conn.query_row("SELECT COUNT(*) FROM segment_diffs", [], |r| r.get(0)).unwrap();
            assert_eq!(after, 0, "delete_job_results 后 segment_diffs 应级联清空");
        }
    }

    /// 真实标书语料校准（手动运行）：
    ///   cargo test -p bidguard --lib calibrate_real_corpus -- --ignored --nocapture
    /// 从 BIDGUARD_CALIB_DIR（默认 ~/Documents/bidguard-test-bids）读 8 份真实标书，
    /// 跑完整 import+compare 管线，dump 矩阵/围标/八类/冲突/耗时，对照标准答案分析。
    #[test]
    #[ignore]
    fn calibrate_real_corpus() {
        use std::time::Instant;
        let dir = std::env::var("BIDGUARD_CALIB_DIR").unwrap_or_else(|_| {
            format!("{}/Documents/bidguard-test-bids", std::env::var("HOME").unwrap())
        });
        let dir = std::path::Path::new(&dir);
        if !dir.exists() {
            eprintln!("跳过：语料目录不存在 {dir:?}");
            return;
        }
        // (文件名, 显示标签)；导入顺序即矩阵行列顺序
        let files = [
            ("甲-华信智联科技-投标文件.docx", "甲docx"),
            ("乙-启明数字技术-投标文件.docx", "乙docx"),
            ("丙-蓝海信息工程-投标文件.pdf", "丙pdf"),
            ("丁-中科软创-投标文件.docx", "丁docx"),
            ("戊-东方网御-投标文件.md", "戊md"),
            ("己-北辰系统集成-投标文件.txt", "己txt"),
            ("甲-报价清单.xlsx", "甲xls"),
            ("乙-报价清单.xlsx", "乙xls"),
        ];
        let paths: Vec<String> = files
            .iter()
            .map(|(f, _)| dir.join(f).to_string_lossy().into_owned())
            .collect();

        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "校准").unwrap().id
        };
        let jieba = Arc::new(Jieba::new());

        let t0 = Instant::now();
        let ictx = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default(), "bid").unwrap();
        let t_import = t0.elapsed();

        let docs = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws).unwrap()
        };
        println!("\n========== 解析结果（导入 {:.1}s）==========", t_import.as_secs_f32());
        let mut total_chars = 0i64;
        let mut total_chunks = 0i64;
        // 按预期文件顺序排列 id
        let mut ordered_ids = Vec::new();
        for (fname, label) in &files {
            let d = docs.iter().find(|d| &d.file_name == fname).unwrap();
            ordered_ids.push(d.id.clone());
            total_chars += d.char_count.unwrap_or(0);
            total_chunks += d.chunk_count;
            println!(
                "  {label:6} {:>4}页 {:>9}字 {:>6}块 [{:>10}] {}",
                d.page_count.unwrap_or(0),
                d.char_count.unwrap_or(0),
                d.chunk_count,
                d.parse_method.as_deref().unwrap_or("?"),
                if d.status == "parsed" { "✓" } else { &d.status }
            );
        }
        println!("  合计 {total_chars} 字 / {total_chunks} 块（段落级）");

        let t1 = Instant::now();
        let cctx = ctx_for(&pool, &ws, "compare", false);
        let cfg = CompareRunConfig {
            enable_fact_conflict: true,
            ..cfg_with(ordered_ids.clone(), 0.55)
        };
        run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), &ws, &cfg).unwrap();
        let t_compare = t1.elapsed();

        let (matrix, peak) = matrix_peak(&pool, &cctx.job_id);
        let conn = pool.get().unwrap();
        let r = job_repo::get_result_jsons(&conn, &cctx.job_id).unwrap();
        let summary: serde_json::Value = serde_json::from_str(r.summary_json.as_deref().unwrap_or("{}")).unwrap();
        let collusion: serde_json::Value = serde_json::from_str(r.collusion_json.as_deref().unwrap_or("{}")).unwrap();
        let shared: serde_json::Value = serde_json::from_str(r.shared_terms_json.as_deref().unwrap_or("[]")).unwrap();

        println!("\n========== 文档相似度矩阵（比对 {:.1}s, 峰值 {:.0}%）==========", t_compare.as_secs_f32(), peak * 100.0);
        print!("        ");
        for (_, l) in &files { print!("{l:>7}"); }
        println!();
        for (i, row) in matrix.iter().enumerate() {
            print!("  {:6}", files[i].1);
            for v in row { print!("{:>6.0}%", v * 100.0); }
            println!();
        }

        let clusters = clusters_of(&pool, &cctx.job_id);
        let mut by_type: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
        for c in &clusters { *by_type.entry(c.cluster_type.clone()).or_insert(0) += 1; }

        println!("\n========== 八类统计 ==========");
        for k in ["sameCount","minorChangeCount","changedCount","rewriteCount","conflictCount","uncertainCount","addedCount","deletedCount"] {
            println!("  {k:18} {}", summary[k].as_u64().unwrap_or(0));
        }
        println!("  clusterCount(总组) {}", summary["clusterCount"].as_u64().unwrap_or(0));

        println!("\n========== 围标判定 ==========");
        println!("  level={} score={:.2}", collusion["level"].as_str().unwrap_or("?"), collusion["score"].as_f64().unwrap_or(0.0));
        if let Some(sigs) = collusion["signals"].as_array() {
            for s in sigs {
                println!("  · [{}] {} (w={:.2})", s["kind"].as_str().unwrap_or("?"), s["detail"].as_str().unwrap_or(""), s["weight"].as_f64().unwrap_or(0.0));
            }
        }

        // 事实冲突详情（前 12 条）
        let conflict_clusters: Vec<_> = clusters.iter().filter(|c| c.cluster_type == "conflict").collect();
        println!("\n========== 事实冲突（{} 组，列前 12）==========", conflict_clusters.len());
        for c in conflict_clusters.iter().take(12) {
            let d = compare_repo::get_cluster_detail(&conn, &c.id).unwrap();
            let fields = d.conflict_json.as_deref().unwrap_or("");
            let topic = c.topic.as_deref().unwrap_or("");
            // 提取冲突字段名
            let kinds: Vec<&str> = ["amount","duration","date","percentage","subject"].iter().copied().filter(|k| fields.contains(*k)).collect();
            println!("  [{}] {} | 字段={:?}", c.severity.as_deref().unwrap_or("?"), &topic.chars().take(24).collect::<String>(), kinds);
        }

        println!("\n========== 共有罕见词（前 10）==========");
        if let Some(terms) = shared.as_array() {
            for t in terms.iter().take(10) {
                println!("  {} ×{}文档", t["term"].as_str().unwrap_or("?"), t["docs"].as_array().map(|a| a.len()).unwrap_or(0));
            }
        }
        println!("\n========== 性能 ==========");
        println!("  导入 {:.1}s + 比对 {:.1}s = 总 {:.1}s（{} 字 / {} 块）",
            t_import.as_secs_f32(), t_compare.as_secs_f32(), (t_import + t_compare).as_secs_f32(), total_chars, total_chunks);
    }

    #[test]
    fn similar_docs_score_higher_than_different_v2() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let common = "本项目采用分层解耦的微服务总体架构，系统自下而上划分为基础设施层、数据资源层、应用支撑层与业务应用层，所有业务能力对外以统一接口网关暴露，保证横向可扩展与纵向可演进。";
        let files = vec![
            ("a.txt", format!("{common}甲方在实施计划中补充了里程碑安排与质量保证措施。")),
            ("b.txt", format!("{common}乙方在实施计划中补充了里程碑安排与质量保证措施。")),
            ("c.txt", "本方案聚焦数据治理与隐私合规，强调本地化部署、最小权限与全链路审计，组织方式与技术选型均独立设计。".to_string()),
        ];
        let (job_id, _ids) = import_and_compare(&pool, &ws, &files, 0.35);

        let (m, peak) = matrix_peak(&pool, &job_id);
        assert!((m[0][0] - 1.0).abs() < 1e-6, "对角线应为 1");
        assert!(m[0][1] > 0.6, "甲乙相似度应较高，实际 {}", m[0][1]);
        assert!(m[0][2] < m[0][1], "甲丙应低于甲乙：ac={} ab={}", m[0][2], m[0][1]);
        assert!(peak > 0.6, "峰值应较高，实际 {peak}");

        let clusters = clusters_of(&pool, &job_id);
        assert!(!clusters.is_empty(), "应聚出跨文档雷同条款");
        assert!(clusters.iter().any(|c| c.document_ids.len() >= 2));

        // 详情含分级 diff，且有相同片段
        let conn = pool.get().unwrap();
        let detail = compare_repo::get_cluster_detail(&conn, &clusters[0].id).unwrap();
        assert!(!detail.members.is_empty());
        assert!(!detail.diffs.is_empty(), "应生成 diff");
        let ops: Vec<crate::engine::report::DiffOp> =
            serde_json::from_str(&detail.diffs[0].diff_json).unwrap();
        assert!(ops.iter().any(|o| o.op == "eq"), "匹配段落应含相同片段");
    }

    #[test]
    fn table_rows_cluster_and_amount_conflict_detected() {
        // 端到端：两份标书报价表同一明细行，金额不同 → 表格行聚类 + 事实冲突
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let mk = |price: &str| {
            format!(
                "报价清单如下表所示，所有设备均为原厂正品并提供三年质保服务。\n\
                 | 序号 | 设备名称及服务内容 | 总价 | 工期 |\n\
                 |---|---|---|---|\n\
                 | 1 | 核心交换机及配套光模块安装调试 | {price} | 30天 |\n"
            )
        };
        let files = [("a.txt", mk("64000元")), ("b.txt", mk("78000元"))];

        // import_and_compare 的默认配置关了事实冲突，这里手动开
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_tbl_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let paths: Vec<String> = files
            .iter()
            .map(|(name, content)| {
                let p = dir.join(name);
                std::fs::write(&p, content).unwrap();
                p.to_string_lossy().into_owned()
            })
            .collect();
        let ictx = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws)
                .unwrap()
                .into_iter()
                .map(|d| d.id)
                .collect()
        };
        let cctx = ctx_for(&pool, &ws, "compare", false);
        let cfg = CompareRunConfig {
            enable_fact_conflict: true,
            ..cfg_with(ids, 0.35)
        };
        run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), &ws, &cfg).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let clusters = clusters_of(&pool, &cctx.job_id);
        assert!(!clusters.is_empty(), "表格行应聚出跨文档雷同组");
        let conflict = clusters
            .iter()
            .find(|c| c.cluster_type == "conflict")
            .expect("金额不同的同一明细行应判为事实冲突");

        let conn = pool.get().unwrap();
        let detail = compare_repo::get_cluster_detail(&conn, &conflict.id).unwrap();
        assert!(
            detail.members.iter().any(|m| m.text.contains("核心交换机")),
            "冲突组成员应是该表格行"
        );
        let cj = detail.conflict_json.as_deref().expect("冲突组应带 conflict_json");
        assert!(cj.contains("amount"), "冲突字段应含金额：{cj}");
        assert!(cj.contains("64000") && cj.contains("78000"), "应给出两边的值：{cj}");
    }

    #[test]
    #[ignore] // 性能基准（§16.1）：cargo test --release perf_smoke -- --ignored（debug 构建数值无参考意义）
    fn perf_smoke_three_docs_100_pages_under_60s() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "perf").unwrap().id
        };
        // 3 份 ≈100 页文档：每份 300 段 × ~110 字 ≈ 3.3 万字/份；约 1/3 段落跨文档共享
        let topics = ["架构", "安全", "运维", "培训", "测试", "数据", "网络", "存储"];
        let mk_doc = |seed: usize| -> String {
            (0..300)
                .map(|i| {
                    let shared = i % 3 == 0;
                    let salt = if shared { 0 } else { seed };
                    format!(
                        "第{i}节 关于{}体系的说明（编号 {salt}-{i}）：本节针对{}专项给出实施方案与质量保障措施，\
                         明确角色分工、里程碑节点与验收标准，并对潜在风险提出预案与回退路径，确保交付质量满足合同约定。",
                        topics[i % topics.len()],
                        topics[(i + salt) % topics.len()],
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
        };
        let files = [
            ("a.txt", mk_doc(1)),
            ("b.txt", mk_doc(2)),
            ("c.txt", mk_doc(3)),
        ];
        let t0 = std::time::Instant::now();
        let (job_id, _) = import_and_compare(&pool, &ws, &files, 0.6);
        let elapsed = t0.elapsed();
        let clusters = clusters_of(&pool, &job_id);
        assert!(!clusters.is_empty(), "共享段应聚出条款组");
        assert!(
            elapsed.as_secs() < 60,
            "3 份 100 页文档导入+比对应在 60s 内完成，实际 {:.1}s",
            elapsed.as_secs_f32()
        );
        eprintln!("[perf] 3×300 段导入+比对耗时 {:.1}s，聚类 {} 组", elapsed.as_secs_f32(), clusters.len());
    }

    #[test]
    fn moved_paragraph_is_annotated() {
        // 同一段文字在甲篇开头、乙篇结尾 → same/minor_change 且标注「位置移动」
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let target = "本项目质量保证体系覆盖设计、开发、测试与交付全流程，并设立独立的质量监督岗位。";
        let fillers_a = [
            "甲篇第一部分阐述项目背景与建设目标的总体说明。",
            "甲篇第二部分给出组织架构与人员配置的具体安排。",
            "甲篇第三部分描述项目进度计划与里程碑设置情况。",
            "甲篇第四部分说明培训方案与知识转移的实施路径。",
        ];
        let fillers_b = [
            "乙篇开篇先交代售后服务承诺与响应时效的标准。",
            "乙篇随后介绍数据迁移策略与历史数据清洗规则。",
            "乙篇接着列出安全保障措施与等级保护合规说明。",
            "乙篇之后补充运维交接与文档交付的完整清单。",
        ];
        let files = [
            ("a.txt", format!("{target}\n{}", fillers_a.join("\n"))),
            ("b.txt", format!("{}\n{target}", fillers_b.join("\n"))),
        ];
        let (job_id, _ids) = import_and_compare(&pool, &ws, &files, 0.5);
        let clusters = clusters_of(&pool, &job_id);
        let hit = clusters
            .iter()
            .find(|c| matches!(c.cluster_type.as_str(), "same" | "minor_change"))
            .expect("目标段应聚类");
        assert!(
            hit.summary.as_deref().unwrap_or("").contains("位置移动"),
            "首尾位置差应标注移动：{:?}",
            hit.summary
        );
    }

    #[test]
    fn excel_vs_docx_price_table_conflict_e2e() {
        // 端到端（真实文件 + 真实解析器）：docx 报价表 64000元 vs Excel 同一明细行 78000元
        // → 跨格式聚成同一组 + 金额事实冲突。这是「报价 Excel 直接参与比对」的核心价值链。
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let dir = std::env::temp_dir().join(format!("bg_xfmt_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let docx = crate::test_fixtures::write_docx_price_table(&dir, "甲方报价.docx", "64000元");
        let xlsx = crate::test_fixtures::write_xlsx_rows(
            &dir,
            "乙方报价.xlsx",
            "报价清单",
            &[
                &["序号", "设备名称及服务内容", "总价", "工期"],
                &["1", "核心交换机及配套光模块安装调试", "78000元", "30天"],
            ],
        );

        let jieba = Arc::new(Jieba::new());
        let ictx = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), &ws, &[docx, xlsx], &Default::default(), "bid")
            .unwrap();
        let docs = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws).unwrap()
        };
        assert_eq!(docs.len(), 2);
        assert!(docs.iter().all(|d| d.status == "parsed"), "两份都应解析成功：{docs:?}");
        let type_of: std::collections::HashMap<String, String> =
            docs.iter().map(|d| (d.id.clone(), d.file_type.clone())).collect();

        let cctx = ctx_for(&pool, &ws, "compare", false);
        let cfg = CompareRunConfig {
            enable_fact_conflict: true,
            ..cfg_with(docs.iter().map(|d| d.id.clone()).collect(), 0.35)
        };
        run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), &ws, &cfg).unwrap();
        let _ = std::fs::remove_dir_all(&dir);

        let clusters = clusters_of(&pool, &cctx.job_id);
        let conflict = clusters
            .iter()
            .find(|c| c.cluster_type == "conflict")
            .expect("跨格式同一明细行金额不同应判事实冲突");

        let conn = pool.get().unwrap();
        let detail = compare_repo::get_cluster_detail(&conn, &conflict.id).unwrap();
        // 成员应横跨 docx 与 xlsx 两种来源
        let member_types: std::collections::HashSet<&str> = detail
            .members
            .iter()
            .map(|m| type_of[&m.document_id].as_str())
            .collect();
        assert!(
            member_types.contains("docx") && member_types.contains("xlsx"),
            "冲突组应同时含 docx 与 xlsx 成员：{member_types:?}"
        );
        assert!(detail.members.iter().any(|m| m.text.contains("核心交换机")));
        let cj = detail.conflict_json.as_deref().unwrap();
        assert!(cj.contains("amount") && cj.contains("64000") && cj.contains("78000"), "{cj}");
    }

    #[test]
    fn collusion_pipeline_on_generated_bids_v2() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        // —— 围标组：甲乙技术+商务条款近乎逐字雷同；甲乙丙共有合规声明与工期条款 ——
        let tech = "系统采用分层解耦的微服务架构设计自下而上划分为基础设施层数据资源层应用支撑层与业务应用层\n各层之间通过标准化接口解耦所有业务能力对外以统一接口网关暴露确保横向可扩展与纵向可演进\n平台采用读写分离与多级缓存机制保证高可用性与毫秒级的端到端响应";
        let compliance = "本项目严格遵循国家信息安全等级保护三级标准与相关行业规范要求";
        let schedule = "本工程建设周期为一百八十个日历日完成全部交付与验收工作";
        let qual = "我公司具备信息系统集成及服务一级资质与软件企业认定证书";
        let files = vec![
            ("甲_智慧城邦.txt", format!("本技术方案由智慧城邦科技有限公司编制\n{tech}\n{compliance}\n{schedule}\n投标报价为人民币一千两百八十万元整包含全部软硬件与三年运维服务费用\n{qual}")),
            ("乙_启明信息.txt", format!("本技术方案由启明信息技术股份公司编制\n{tech}\n{compliance}\n{schedule}\n投标报价为人民币一千两百九十万元整包含全部软硬件与三年运维服务费用\n{qual}")),
            ("丙_鸿信科技.txt", format!("本技术方案由鸿信科技集团独立编写完成\n我们基于云原生容器编排技术构建弹性可伸缩的整体解决方案\n采用事件驱动与消息队列实现各子系统之间的异步协同与削峰填谷\n数据治理方面引入数据中台统一汇聚清洗与共享交换各类政务数据资源\n{compliance}\n{schedule}\n投标报价为人民币一千一百五十万元整\n我公司持有建筑智能化工程专业承包资质")),
        ];
        let (job_id, ids) = import_and_compare(&pool, &ws, &files, 0.5);

        let (m, peak) = matrix_peak(&pool, &job_id);
        assert!(peak >= 0.75, "甲乙应高度同源，实际峰值 {peak}（甲乙={}）", m[0][1]);

        let clusters = clusters_of(&pool, &job_id);
        let ab = clusters
            .iter()
            .filter(|c| c.document_ids.contains(&ids[0]) && c.document_ids.contains(&ids[1]))
            .count();
        assert!(ab >= 4, "甲乙应有多处雷同条款，实际 {ab}");
        assert!(
            clusters.iter().any(|c| c.document_ids.len() >= 3),
            "应存在跨 3 份文档的雷同条款"
        );

        let conn = pool.get().unwrap();
        let r = job_repo::get_result_jsons(&conn, &job_id).unwrap();
        let sections: Vec<SectionStat> =
            serde_json::from_str(&r.sections_json.unwrap()).unwrap();
        assert!(sections.iter().any(|s| s.section == "tech"), "应识别出技术标段");
        assert!(sections.iter().any(|s| s.section == "business"), "应识别出商务标段");

        let collusion: crate::engine::report::Collusion =
            serde_json::from_str(&r.collusion_json.unwrap()).unwrap();
        eprintln!(
            "[围标组v2] 峰值={peak:.2} 判定={}({:.2}) 信号={} 聚类={}",
            collusion.level,
            collusion.score,
            collusion.signals.len(),
            clusters.len()
        );
        assert!(
            matches!(collusion.level.as_str(), "high" | "medium"),
            "围标组应判定为需复核(high/medium)，实际 {}",
            collusion.level
        );
        // 报价梯度信号：甲乙 1280 万 vs 1290 万（差 0.8%）且多处条款雷同
        assert!(
            collusion.signals.iter().any(|s| s.kind == "facts"),
            "应命中报价梯度信号，实际信号：{:?}",
            collusion.signals.iter().map(|s| s.kind.clone()).collect::<Vec<_>>()
        );

        // 八类统计自洽
        let summary: CompareSummary = serde_json::from_str(&r.summary_json.unwrap()).unwrap();
        let total = summary.same_count
            + summary.minor_change_count
            + summary.rewrite_count
            + summary.changed_count
            + summary.added_count
            + summary.deleted_count
            + summary.conflict_count
            + summary.uncertain_count;
        assert_eq!(total, summary.cluster_count, "分类计数之和应等于聚类总数");
        assert_eq!(summary.document_count, 3);
        drop(conn); // 测试池只有 1 个连接，进入负向段前必须归还

        // —— 负向对照：三份业务领域完全不同的独立标书，不应误判 ——
        let ws2 = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w2").unwrap().id
        };
        let neg = vec![
            ("独A.txt", "本公司专注于城市轨道交通信号系统的设计集成与现场实施工作\n依托自主研发的列车自动控制平台保障线路运行安全与准点率".to_string()),
            ("独B.txt", "我司主营医院信息化与电子病历平台的建设运营服务\n凭借多年三甲医院项目经验提供稳定的临床数据与诊疗支撑".to_string()),
            ("独C.txt", "团队从事智慧农业物联网传感终端的研发生产与销售\n通过田间环境监测与作物长势分析帮助种植户增产增收".to_string()),
        ];
        let (job2, _) = import_and_compare(&pool, &ws2, &neg, 0.5);
        let conn = pool.get().unwrap();
        let r2 = job_repo::get_result_jsons(&conn, &job2).unwrap();
        let collusion2: crate::engine::report::Collusion =
            serde_json::from_str(&r2.collusion_json.unwrap()).unwrap();
        eprintln!("[独立组v2] 判定={}({:.2})", collusion2.level, collusion2.score);
        assert!(
            matches!(collusion2.level.as_str(), "none" | "low"),
            "独立标书不应判围标，实际 {}",
            collusion2.level
        );
        drop(conn);
        let neg_clusters = clusters_of(&pool, &job2);
        assert!(
            neg_clusters.iter().all(|c| c.document_ids.len() < 3),
            "独立标书不应出现跨 3 份的雷同条款"
        );
    }

    /// 手造取证夹具 docx：正文 + settings.xml(rsids) + core.xml(created) + app.xml(Template)。
    fn write_forensic_docx(
        dir: &std::path::Path,
        name: &str,
        body_text: &str,
        rsids_inner: &str, // 空串 = 不写 settings.xml（WPS 无 rsids 场景）
        created: &str,
        template: &str,
    ) -> String {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let p = dir.join(name);
        let f = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let o = SimpleFileOptions::default();
        zw.start_file("[Content_Types].xml", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.as_bytes()).unwrap();
        zw.start_file("word/document.xml", o).unwrap();
        zw.write_all(format!(r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{body_text}</w:t></w:r></w:p></w:body></w:document>"#).as_bytes()).unwrap();
        if !rsids_inner.is_empty() {
            zw.start_file("word/settings.xml", o).unwrap();
            zw.write_all(format!(r#"<?xml version="1.0"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:rsids>{rsids_inner}</w:rsids></w:settings>"#).as_bytes()).unwrap();
        }
        zw.start_file("docProps/core.xml", o).unwrap();
        zw.write_all(format!(r#"<?xml version="1.0"?><cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/"><dc:creator>{name}的编制人</dc:creator><dcterms:created>{created}</dcterms:created></cp:coreProperties>"#).as_bytes()).unwrap();
        zw.start_file("docProps/app.xml", o).unwrap();
        zw.write_all(format!(r#"<?xml version="1.0"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Office Word</Application><Template>{template}</Template></Properties>"#).as_bytes()).unwrap();
        zw.finish().unwrap();
        p.to_string_lossy().into_owned()
    }

    /// M1 W1-1/W1-2 端到端：rsid 交集 + 模板/创建邻近/包结构进 collusion_json。
    #[test]
    fn forensic_docx_signals_flow_into_collusion() {
        let pool = open_in_memory().unwrap();
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_forensic_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // —— 正向：rsidRoot 相同 + 共享 3 个 rsid + 同模板 + created 相差 5 分钟 ——
        let shared_rsids = r#"<w:rsidRoot w:val="00AA0001"/><w:rsid w:val="00AA0001"/><w:rsid w:val="00AA0002"/><w:rsid w:val="00AA0003"/>"#;
        let a = write_forensic_docx(
            &dir, "a.docx",
            "甲公司的技术方案聚焦于城市轨道交通信号系统的设计集成与实施。",
            shared_rsids, "2024-05-01T10:00:00Z", "投标文件模板.dotx",
        );
        let b = write_forensic_docx(
            &dir, "b.docx",
            "乙公司主营医院信息化平台建设与临床数据运营支撑服务体系。",
            shared_rsids, "2024-05-01T10:05:00Z", "投标文件模板.dotx",
        );
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let ictx = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), &ws, &[a, b], &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws).unwrap().iter().map(|d| d.id.clone()).collect()
        };
        let cctx = ctx_for(&pool, &ws, "compare", false);
        run_compare(&cctx, jieba.clone(), Arc::new(Mutex::new(None)), &ws, &cfg_with(ids, 0.5)).unwrap();
        let collusion: crate::engine::report::Collusion = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &cctx.job_id).unwrap();
            serde_json::from_str(&r.collusion_json.unwrap()).unwrap()
        };
        let rsid = collusion.signals.iter().find(|s| s.kind == "rsid").expect("应有 rsid 信号");
        assert!((rsid.weight - 0.35).abs() < 1e-6, "rsidRoot 相同 → 满权重 0.35");
        assert!(rsid.detail.contains("另存为"), "免责语：{}", rsid.detail);
        assert!(rsid.detail.contains("未命中不代表清白"));
        let meta = collusion.signals.iter().find(|s| s.kind == "metadata").expect("应有 metadata 信号");
        assert!(meta.detail.contains("模板相同"), "detail 应枚举命中项：{}", meta.detail);
        assert!(meta.detail.contains("创建时间邻近"), "{}", meta.detail);
        assert!(meta.detail.contains("包结构一致"), "同一打包管线：{}", meta.detail);

        // —— 负向（WPS 场景）：无 rsids 节点 + 默认模板 + created 相差 2 天 → 两信号均缺席 ——
        let ws2 = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w2").unwrap().id
        };
        let c = write_forensic_docx(
            &dir, "c.docx",
            "丙公司专注智慧农业物联网传感终端的研发生产与销售推广。",
            "", "2024-05-01T10:00:00Z", "Normal.dotm",
        );
        let d = write_forensic_docx(
            &dir, "d.docx",
            "丁公司提供水利工程勘察设计与流域生态治理的整体咨询。",
            "", "2024-05-03T10:00:00Z", "Normal.dotm",
        );
        let ictx2 = ctx_for(&pool, &ws2, "import", false);
        import_service::run_import(&ictx2, jieba.clone(), &ws2, &[c, d], &Default::default(), "bid").unwrap();
        let ids2: Vec<String> = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws2).unwrap().iter().map(|d| d.id.clone()).collect()
        };
        let cctx2 = ctx_for(&pool, &ws2, "compare", false);
        run_compare(&cctx2, jieba, Arc::new(Mutex::new(None)), &ws2, &cfg_with(ids2, 0.5)).unwrap();
        let collusion2: crate::engine::report::Collusion = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &cctx2.job_id).unwrap();
            serde_json::from_str(&r.collusion_json.unwrap()).unwrap()
        };
        assert!(
            collusion2.signals.iter().all(|s| s.kind != "rsid"),
            "无 rsids 节点：信号缺席而非报错"
        );
        for s in &collusion2.signals {
            assert!(
                !s.detail.contains("检查通过") && !s.detail.contains("清白证明"),
                "不得输出背书式表述：{}",
                s.detail
            );
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 8 条竖带结构化灰度图的 PNG / JPEG 两版字节（同图重压 → 不同字节、dHash 近似）。
    fn banded_png_jpeg() -> (Vec<u8>, Vec<u8>) {
        let bands = [10u8, 200, 40, 160, 90, 230, 20, 250];
        let img = image::RgbImage::from_fn(220, 180, |x, _| {
            image::Rgb([bands[(x * 8 / 220).min(7) as usize]; 3])
        });
        let enc = |fmt| {
            let mut b = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img.clone()).write_to(&mut b, fmt).unwrap();
            b.into_inner()
        };
        (enc(image::ImageFormat::Png), enc(image::ImageFormat::Jpeg))
    }

    /// 手造带一张 word/media 图片的最小 docx（正文 + 单图）。
    fn write_docx_with_image(
        dir: &std::path::Path,
        name: &str,
        body_text: &str,
        media_name: &str,
        bytes: &[u8],
    ) -> String {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let p = dir.join(name);
        let f = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let o = SimpleFileOptions::default();
        zw.start_file("[Content_Types].xml", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Default Extension="jpg" ContentType="image/jpeg"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.as_bytes()).unwrap();
        zw.start_file("word/document.xml", o).unwrap();
        zw.write_all(format!(r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>{body_text}</w:t></w:r></w:p></w:body></w:document>"#).as_bytes()).unwrap();
        zw.start_file(format!("word/media/{media_name}"), o).unwrap();
        zw.write_all(bytes).unwrap();
        zw.finish().unwrap();
        p.to_string_lossy().into_owned()
    }

    /// W1-4 端到端：两份 docx 内嵌同一张图的 PNG/JPEG 两版（不同字节）→ imageReuse 信号入库；
    /// 换成两张无关图则信号缺席（防误报）。
    #[test]
    fn embedded_image_reuse_flows_into_collusion() {
        let pool = open_in_memory().unwrap();
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_imgreuse_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let (png, jpg) = banded_png_jpeg();

        // —— 正向：两文档共用同一张图（PNG vs JPEG 重压，不同字节）——
        let a = write_docx_with_image(
            &dir, "a.docx",
            "甲公司的技术方案聚焦于城市轨道交通信号系统的设计集成与实施。",
            "pic.png", &png,
        );
        let b = write_docx_with_image(
            &dir, "b.docx",
            "乙公司主营医院信息化平台建设与临床数据运营支撑服务体系。",
            "pic.jpg", &jpg,
        );
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let ictx = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), &ws, &[a, b], &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws).unwrap().iter().map(|d| d.id.clone()).collect()
        };
        let cctx = ctx_for(&pool, &ws, "compare", false);
        run_compare(&cctx, jieba.clone(), Arc::new(Mutex::new(None)), &ws, &cfg_with(ids, 0.5)).unwrap();
        let collusion: crate::engine::report::Collusion = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &cctx.job_id).unwrap();
            serde_json::from_str(&r.collusion_json.unwrap()).unwrap()
        };
        let img = collusion.signals.iter().find(|s| s.kind == "imageReuse").expect("应有 imageReuse 信号");
        assert!(img.weight > 0.0);
        assert!(img.detail.contains("请核对"), "detail 应提示核对：{}", img.detail);
        assert!(img.detail.contains("未命中不代表清白"));
        assert!(!img.detail.contains("检查通过") && !img.detail.contains("清白证明"));

        // —— 负向：两张无关图（纯色 vs 竖带）→ imageReuse 缺席 ——
        let solid = {
            let img = image::RgbImage::from_pixel(220, 180, image::Rgb([128, 128, 128]));
            let mut buf = std::io::Cursor::new(Vec::new());
            image::DynamicImage::ImageRgb8(img).write_to(&mut buf, image::ImageFormat::Png).unwrap();
            buf.into_inner()
        };
        let ws2 = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w2").unwrap().id
        };
        let c = write_docx_with_image(
            &dir, "c.docx",
            "丙公司专注智慧农业物联网传感终端的研发生产与销售推广。",
            "band.png", &png,
        );
        let d = write_docx_with_image(
            &dir, "d.docx",
            "丁公司提供水利工程勘察设计与流域生态治理的整体咨询。",
            "solid.png", &solid,
        );
        let ictx2 = ctx_for(&pool, &ws2, "import", false);
        import_service::run_import(&ictx2, jieba.clone(), &ws2, &[c, d], &Default::default(), "bid").unwrap();
        let ids2: Vec<String> = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws2).unwrap().iter().map(|d| d.id.clone()).collect()
        };
        let cctx2 = ctx_for(&pool, &ws2, "compare", false);
        run_compare(&cctx2, jieba, Arc::new(Mutex::new(None)), &ws2, &cfg_with(ids2, 0.5)).unwrap();
        let collusion2: crate::engine::report::Collusion = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &cctx2.job_id).unwrap();
            serde_json::from_str(&r.collusion_json.unwrap()).unwrap()
        };
        assert!(
            collusion2.signals.iter().all(|s| s.kind != "imageReuse"),
            "无关图不应产生 imageReuse 信号"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fact_conflict_marks_cluster_and_persists_facts() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_fact_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for (n, t) in [
            ("a.txt", "投标人投标报价为人民币12800000元整，工期180个日历日，按期支付服务费用。"),
            ("b.txt", "投标人投标报价为人民币12900000元整，工期180个日历日，按期支付服务费用。"),
        ] {
            let p = dir.join(n);
            std::fs::write(&p, t).unwrap();
            paths.push(p.to_string_lossy().into_owned());
        }
        let ictx = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws).unwrap().iter().map(|d| d.id.clone()).collect()
        };

        let cctx = ctx_for(&pool, &ws, "compare", false);
        let mut cfg = cfg_with(ids, 0.5);
        cfg.enable_fact_conflict = true;
        run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), &ws, &cfg).unwrap();

        let clusters = clusters_of(&pool, &cctx.job_id);
        let conflict = clusters
            .iter()
            .find(|c| c.cluster_type == "conflict")
            .expect("金额不同的雷同条款应判 conflict");
        assert_eq!(conflict.severity.as_deref(), Some("high"), "金额冲突 → high");

        let conn = pool.get().unwrap();
        let detail = compare_repo::get_cluster_detail(&conn, &conflict.id).unwrap();
        assert!(!detail.facts.is_empty(), "事实应落库可查");
        assert!(detail.facts.iter().any(|f| f.amount.is_some()));
        let cj = detail.conflict_json.expect("应有冲突详情");
        assert!(cj.contains("amount"), "冲突字段应含金额：{cj}");

        // 总览 conflict 计数
        let r = job_repo::get_result_jsons(&conn, &cctx.job_id).unwrap();
        let summary: CompareSummary = serde_json::from_str(&r.summary_json.unwrap()).unwrap();
        assert!(summary.conflict_count >= 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cancelled_compare_leaves_no_partial_results() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_cancel_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut paths = Vec::new();
        for (n, t) in [("x.txt", "本项目采用分层解耦的微服务总体架构设计方案"), ("y.txt", "本项目采用分层解耦的微服务总体架构设计方法")] {
            let p = dir.join(n);
            std::fs::write(&p, t).unwrap();
            paths.push(p.to_string_lossy().into_owned());
        }
        let ictx = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            document_repo::list(&conn, &ws).unwrap().iter().map(|d| d.id.clone()).collect()
        };

        let cctx = ctx_for(&pool, &ws, "compare", true); // 预置取消
        let err = run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), &ws, &cfg_with(ids, 0.5))
            .unwrap_err();
        assert_eq!(err.code, AppErrorCode::JobCancelled);

        let conn = pool.get().unwrap();
        let edges: i64 = conn
            .query_row("SELECT COUNT(*) FROM candidate_edges WHERE job_id = ?1", [&cctx.job_id], |r| r.get(0))
            .unwrap();
        let clusters: i64 = conn
            .query_row("SELECT COUNT(*) FROM clusters WHERE job_id = ?1", [&cctx.job_id], |r| r.get(0))
            .unwrap();
        assert_eq!((edges, clusters), (0, 0), "取消不应残留半成品");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // —— M1 共同错误指纹：shared_error_fingerprints 三类检测器（内存分块，零 IO）——

    fn ecmp(
        doc: usize,
        text: &str,
        tokens: &[&str],
        section_path: Vec<String>,
        entities: Vec<crate::engine::features::Entity>,
    ) -> CmpChunk {
        CmpChunk {
            id: String::new(),
            doc,
            rel_pos: 0.0,
            page: None,
            text: text.to_string(),
            exact_hash: String::new(),
            normalized_hash: String::new(),
            section_path,
            section_kind: "other".into(),
            is_template: false,
            is_table_row: false,
            char_count: text.chars().count(),
            tokens: tokens.iter().map(|s| s.to_string()).collect(),
            ngrams: HashSet::new(),
            minhash: vec![],
            entities,
            tfidf: HashMap::new(),
            tender_coverage: 0.0,
            boiler_fraction: 0.0,
        }
    }

    // —— 分区分层阈值（§5 W3-5）——

    fn zoned(doc: usize, text: &str, kind: &str) -> CmpChunk {
        let mut c = ecmp(doc, text, &[], vec![], vec![]);
        c.section_kind = kind.into();
        c
    }

    #[test]
    fn effective_threshold_legal_bump_and_price_gate() {
        let base = 0.7f32;
        let long = "字".repeat(40); // 40 字 > SHORT_TEXT_CHARS → 无短文本上浮，隔离 zone 效应
        // tech 区：基础阈值不变 → 同相似度 0.75 的段对照常聚类（0.75 ≥ 0.70）。
        let (ta, tb) = (zoned(0, &long, "tech"), zoned(1, &long, "tech"));
        assert!((effective_threshold(base, &ta, &tb) - base).abs() < 1e-6);
        assert!(0.75 >= effective_threshold(base, &ta, &tb), "tech 区 0.75 应聚类");
        // legal 区：阈值 +0.12 → 0.82，相似度 0.75 不聚类。
        let (la, lb) = (zoned(0, &long, "legal"), zoned(1, &long, "legal"));
        let lt = effective_threshold(base, &la, &lb);
        assert!((lt - 0.82).abs() < 1e-6, "legal 阈值应为 0.82，实际 {lt}");
        assert!(0.75 < lt, "legal 区 0.75 不应聚类");
        // price 区：维持现阈值（docs §5 W3-5）——不上浮、不阻断，按基础阈值参与聚类，保住
        // 「同一明细行金额不一致 → 事实冲突」价值链（数值层 M6 落地后再从围标口径剥离 price 文本相似）。
        let (pa, pb) = (zoned(0, &long, "price"), zoned(1, &long, "price"));
        assert!((effective_threshold(base, &pa, &pb) - base).abs() < 1e-6, "price 区应维持基础阈值");
    }

    #[test]
    fn effective_threshold_legal_short_text_capped() {
        // legal + 短文本叠加封顶 ZONE_BUMP_CAP（0.9+0.12+0.08=1.10 → 0.98），防阈值不可达。
        let (a, b) = (zoned(0, "短句", "legal"), zoned(1, "短句", "legal"));
        assert!((effective_threshold(0.9, &a, &b) - ZONE_BUMP_CAP).abs() < 1e-6);
    }

    #[test]
    fn zone_in_scope_business_family_and_tech() {
        // business 范围含 legal/price（验收 (3)：scope=business 仍含报价段）。
        for k in ["business", "legal", "price", "other"] {
            assert!(zone_in_scope(k, "business"), "business 范围应含 {k}");
        }
        assert!(!zone_in_scope("tech", "business"));
        // tech 范围排除 business 家族（含 legal/price）。
        assert!(zone_in_scope("tech", "tech"));
        assert!(zone_in_scope("other", "tech"));
        for k in ["business", "legal", "price"] {
            assert!(!zone_in_scope(k, "tech"), "tech 范围应排除 {k}");
        }
        // 完整范围全保留。
        assert!(zone_in_scope("price", "full"));
    }

    #[test]
    fn zone_slot_partition_sums_to_total() {
        // 五区槽位互斥且穷尽：任意 section_kind 序列五区计数之和 = 序列长度（验收 (4)）。
        let kinds = [
            Some("legal"), Some("price"), Some("tech"), Some("business"),
            Some("other"), None, Some("weird"),
        ];
        let mut counts = [0usize; 5];
        for k in kinds {
            counts[zone_slot(k)] += 1;
        }
        assert_eq!(counts.iter().sum::<usize>(), kinds.len());
        assert_eq!(counts[0], 1, "legal");
        assert_eq!(counts[1], 1, "price");
        assert_eq!(counts[4], 3, "other + None + 未知值 均归 other");
    }

    #[test]
    fn shared_out_of_dict_error_word_detected_and_exempted() {
        let jieba = Jieba::new();
        assert!(!jieba.has_word("施工枝术"), "夹具须为词典外虚构错词");
        let chunks = vec![
            ecmp(0, "本方案的施工枝术路线", &["方案", "施工枝术", "路线"], vec![], vec![]),
            ecmp(1, "我司施工枝术能力成熟", &["施工枝术", "能力"], vec![], vec![]),
        ];
        let errs = shared_error_fingerprints(&jieba, &chunks, None);
        let hit = errs.iter().find(|t| t.term == "施工枝术").expect("应检出共享虚构错词");
        assert_eq!(hit.kind.as_deref(), Some("sharedErrors"));
        assert!(hit.rarity.unwrap_or(0.0) > 0.0, "rarity 应 > 0，实际 {:?}", hit.rarity);
        assert_eq!(hit.docs, vec![0, 1]);
        // 豁免：错词在招标文件 tokens 中 → 条目消失（M4 接线路径可测）
        let ex = TenderExemption {
            tokens: HashSet::from(["施工枝术".to_string()]),
            normalized_text: String::new(),
        };
        let errs2 = shared_error_fingerprints(&jieba, &chunks, Some(&ex));
        assert!(errs2.iter().all(|t| t.term != "施工枝术"), "豁免后错词条目应消失");
    }

    #[test]
    fn dictionary_word_and_entity_not_flagged() {
        let jieba = Jieba::new();
        // 「微服务」在词典内 → 不算错；金额实体值即使词典外也不算错
        let ent = crate::engine::features::Entity { kind: "amount".into(), value: "一千两百万元".into() };
        let chunks = vec![
            ecmp(0, "采用微服务一千两百万元", &["微服务", "一千两百万元"], vec![], vec![ent.clone()]),
            ecmp(1, "同样微服务一千两百万元", &["微服务", "一千两百万元"], vec![], vec![ent]),
        ];
        let errs = shared_error_fingerprints(&jieba, &chunks, None);
        assert!(errs.iter().all(|t| t.term != "一千两百万元"), "实体值不应判错");
        // 「微服务」是否词典内取决于 jieba；若词典外仍会被判——仅断言实体豁免这一确定路径
    }

    #[test]
    fn shared_abnormal_punctuation_detected_with_context() {
        let jieba = Jieba::new();
        let chunks = vec![
            ecmp(0, "本期工作完成。。后续安排", &[], vec![], vec![]),
            ecmp(1, "本期工作完成。。后续安排", &[], vec![], vec![]),
        ];
        let errs = shared_error_fingerprints(&jieba, &chunks, None);
        let hit = errs.iter().find(|t| t.term.contains("。。")).expect("应检出叠标点共用错误");
        assert_eq!(hit.kind.as_deref(), Some("sharedErrors"));
        let ctx = hit.context.as_deref().unwrap_or("");
        assert!(ctx.contains("完成") && ctx.contains("后续"), "context 应含前后文，实际「{ctx}」");
        assert_eq!(hit.docs, vec![0, 1]);
    }

    #[test]
    fn abnormal_punctuation_needs_two_docs() {
        let jieba = Jieba::new();
        // 叠标点只出现在单份文档 → 不构成共用错误
        let chunks = vec![
            ecmp(0, "本期工作完成。。后续安排", &[], vec![], vec![]),
            ecmp(1, "本期工作顺利结束正常安排", &[], vec![], vec![]),
        ];
        let errs = shared_error_fingerprints(&jieba, &chunks, None);
        assert!(errs.iter().all(|t| !t.term.contains("。。")), "单份文档的错误不算共用");
    }

    #[test]
    fn high_freq_phrase_not_flagged_as_error() {
        let jieba = Jieba::new();
        let phrase = "按合同执行";
        // 词典外短语出现在 4 个块（块频 4 > 3）→ 高频短语负例，不判错
        let chunks = vec![
            ecmp(0, "甲款按合同执行", &["甲款", phrase], vec![], vec![]),
            ecmp(0, "乙款按合同执行", &["乙款", phrase], vec![], vec![]),
            ecmp(1, "丙款按合同执行", &["丙款", phrase], vec![], vec![]),
            ecmp(1, "丁款按合同执行", &["丁款", phrase], vec![], vec![]),
        ];
        let errs = shared_error_fingerprints(&jieba, &chunks, None);
        assert!(errs.iter().all(|t| t.term != phrase), "块频 4 的高频短语不应判错");
    }

    #[test]
    fn shared_dangling_reference_detected_and_flat_tree_degrades() {
        let jieba = Jieba::new();
        let titles = vec!["第1章 概述".to_string(), "第2章 方案".to_string(), "第3章 计划".to_string()];
        // 两份文档都引用「第9章」，各自标题树只到第3章 → 悬空引用跨文档共现
        let chunks = vec![
            ecmp(0, "详见第9章的实施说明", &[], titles.clone(), vec![]),
            ecmp(1, "参照第9章执行验收", &[], titles.clone(), vec![]),
        ];
        let errs = shared_error_fingerprints(&jieba, &chunks, None);
        assert!(errs.iter().any(|t| t.term == "第9章"), "应检出跨文档悬空引用「第9章」");
        // 降级：标题树为空（PDF/纯文本无层级）→ 不检出，避免标题树稀疏导致全量误报
        let flat = vec![
            ecmp(0, "详见第9章的实施说明", &[], vec![], vec![]),
            ecmp(1, "参照第9章执行验收", &[], vec![], vec![]),
        ];
        assert!(
            shared_error_fingerprints(&jieba, &flat, None).iter().all(|t| t.term != "第9章"),
            "无标题树的文档应降级跳过引用错误检测"
        );
    }

    /// M2 W2-5 端到端：导入带零宽注入的标书 → collusion_json.signals 含 kind=evasion，
    /// detail 含天干标签 + 证据种类 + §1.5 线索级措辞；清白工作区不产生该信号（前端容错）。
    #[test]
    fn evasion_signal_flows_into_collusion_json_end_to_end() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap().id
        };
        // 甲：一段内注入 18 个零宽字符（隐形码点 ≥10 且浓度高 → suspect，x=0.5）
        let evasive = "本项目\u{200B}采用\u{200B}分层\u{200B}解耦\u{200B}的\u{200B}微服务\u{200B}\
             总体\u{200B}架构\u{200B}设计\u{200B}方案\u{200B}，\u{200B}支持\u{200B}横向\u{200B}\
             扩展\u{200B}与\u{200B}读写\u{200B}分离\u{200B}机制。"
            .to_string();
        let clean = "我公司具备信息系统集成一级资质，注册资本一亿元，近三年无重大违法记录。".to_string();
        let (job_id, _ids) =
            import_and_compare(&pool, &ws, &[("evasive.txt", evasive), ("clean.txt", clean)], 0.5);
        let collusion: crate::engine::report::Collusion = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &job_id).unwrap();
            serde_json::from_str(&r.collusion_json.unwrap()).unwrap()
        };
        let ev = collusion
            .signals
            .iter()
            .find(|s| s.kind == "evasion")
            .expect("应含 evasion 信号");
        // 零宽注入 → suspect → 半权重 0.125（EVASION_WEIGHT 0.25 × 0.5）
        assert!((ev.weight - 0.125).abs() < 1e-6, "零宽 suspect → 0.125，实际 {}", ev.weight);
        assert!(ev.detail.contains("检测到疑似规避特征，请人工复核"), "§1.5 线索级措辞：{}", ev.detail);
        assert!(ev.detail.contains('甲'), "detail 应含命中文档天干：{}", ev.detail);
        assert!(ev.detail.contains("隐形码点"), "detail 应含证据种类：{}", ev.detail);
        assert!(ev.detail.contains("未命中不代表清白"));
        assert!(!ev.detail.contains("检查通过") && !ev.detail.contains("清白证明"), "不得输出背书式结论");

        // 清白工作区：两份干净标书 → collusion_json 不含 evasion 信号（旧任务/前端遍历天然容错）
        let ws2 = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "clean").unwrap().id
        };
        let (job2, _) = import_and_compare(
            &pool,
            &ws2,
            &[
                ("c1.txt", "系统采用事件驱动与消息队列实现子系统之间的异步协同与削峰填谷处理。".to_string()),
                ("c2.txt", "平台提供统一身份认证与细粒度权限管控构成的整体安全基座与审计能力。".to_string()),
            ],
            0.5,
        );
        let collusion2: crate::engine::report::Collusion = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &job2).unwrap();
            serde_json::from_str(&r.collusion_json.unwrap()).unwrap()
        };
        assert!(
            collusion2.signals.iter().all(|s| s.kind != "evasion"),
            "清白工作区不产生 evasion 信号（不做检查通过背书）"
        );
    }

    // —— W3-2 招标文件对减：winnowing 双口径矩阵 + chunk_exemptions ——

    /// 招标文件 T + 两份大量逐字引用 T 且另有私有雷同段的投标 A/B：
    /// 断言 matrixOriginal > matrix、残差簇不含引用段文本、chunk_exemptions 行数=引用块数。
    #[test]
    fn tender_subtraction_splits_matrix_and_records_exemptions() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "招标对减").unwrap().id
        };
        let jieba = Arc::new(Jieba::new());

        // 招标条款（一整段，逐字被两家引用）；含独特串「出厂合格证明」供残差断言。
        let tender_clause = "招标人要求所有投标人严格按照本章技术规范逐项应答，全部核心设备必须为原厂全新正品并随附完整的出厂合格证明与第三方检验报告，投标文件须对本节全部技术条款作出实质性响应，不得存在任何负偏离，否则将按无效投标处理并不予评审。";
        // 私有雷同段（A/B 完全相同，招标文件中不存在）。
        let private_clause = "本公司组建了一支经验丰富的专业实施团队，建立了覆盖需求分析、开发测试、上线运维全周期的质量管理体系与应急响应机制，确保项目按期高质量交付验收。";

        let dir = std::env::temp_dir().join(format!("bg_sub_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, content: &str| {
            let p = dir.join(name);
            std::fs::write(&p, content).unwrap();
            p.to_string_lossy().into_owned()
        };
        let tender_path = write("招标文件.txt", tender_clause);
        // 首行「投标单位」各异 → 文件 hash 不同 → 两份独立投标文档（否则同 hash 去重成一份）。
        let bid_a = write(
            "投标A.txt",
            &format!("投标单位甲：华信智联科技有限公司\n{tender_clause}\n{private_clause}"),
        );
        let bid_b = write(
            "投标B.txt",
            &format!("投标单位乙：启明数字技术有限公司\n{tender_clause}\n{private_clause}"),
        );

        // 招标文件以 tender 角色导入（不参评），投标以 bid 角色导入。
        let ictx_t = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx_t, jieba.clone(), &ws, &[tender_path], &Default::default(), "tender").unwrap();
        let ictx_b = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ictx_b, jieba.clone(), &ws, &[bid_a, bid_b], &Default::default(), "bid").unwrap();

        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            ["投标A.txt", "投标B.txt"]
                .iter()
                .map(|n| docs.iter().find(|d| &d.file_name == n).unwrap().id.clone())
                .collect()
        };

        let cctx = ctx_for(&pool, &ws, "compare", false);
        let cfg = cfg_with(ids.clone(), 0.5); // subtract_tender 默认 true
        run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), &ws, &cfg).unwrap();
        let job_id = cctx.job_id.clone();

        // (1)(4) matrix_json 双口径：对角线均为 1，matrixOriginal[0][1] > matrix[0][1]。
        let v: serde_json::Value = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &job_id).unwrap();
            serde_json::from_str(&r.matrix_json.unwrap()).unwrap()
        };
        let m: Vec<Vec<f32>> = serde_json::from_value(v["matrix"].clone()).unwrap();
        let mo: Vec<Vec<f32>> = serde_json::from_value(v["matrixOriginal"].clone()).unwrap();
        assert!((m[0][0] - 1.0).abs() < 1e-6 && (m[1][1] - 1.0).abs() < 1e-6, "matrix 对角线应为 1");
        assert!((mo[0][0] - 1.0).abs() < 1e-6 && (mo[1][1] - 1.0).abs() < 1e-6, "matrixOriginal 对角线应为 1");
        assert!(
            mo[0][1] > m[0][1] + 1e-4,
            "剔除招标引用后相似度应下降：原始 {} 剔除后 {}",
            mo[0][1],
            m[0][1]
        );
        assert!(
            v["peakOriginal"].as_f64().unwrap() > v["peak"].as_f64().unwrap(),
            "peakOriginal 应高于剔除后 peak"
        );

        // (2) 残差簇成员不含招标引用段文本（独特串「出厂合格证明」只在招标引用段出现）。
        let clusters = clusters_of(&pool, &job_id);
        assert!(!clusters.is_empty(), "残差应仍有私有雷同段聚类");
        {
            let conn = pool.get().unwrap();
            for c in &clusters {
                let detail = compare_repo::get_cluster_detail(&conn, &c.id).unwrap();
                for mem in &detail.members {
                    assert!(
                        !mem.text.contains("出厂合格证明"),
                        "残差簇不应含招标引用段：{}",
                        mem.text
                    );
                }
            }
        }
        // 但残差仍应聚出私有雷同段
        let has_private = {
            let conn = pool.get().unwrap();
            clusters.iter().any(|c| {
                compare_repo::get_cluster_detail(&conn, &c.id)
                    .unwrap()
                    .members
                    .iter()
                    .any(|m| m.text.contains("应急响应机制"))
            })
        };
        assert!(has_private, "残差应保留 A/B 私有雷同段");

        // (3) chunk_exemptions 行数 = coverage≥0.8 的引用块数（A、B 各一段引用 → 2）。
        let n_exempt: i64 = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM chunk_exemptions WHERE job_id=?1 AND kind='tender'",
                [&job_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(n_exempt, 2, "两家各一段逐字引用 → 2 条豁免证据");
        // summary 计数一致，且每条 coverage≥0.8
        let summary: CompareSummary = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &job_id).unwrap();
            serde_json::from_str(&r.summary_json.unwrap()).unwrap()
        };
        assert_eq!(summary.tender_ref_chunk_count, 2);
        {
            let conn = pool.get().unwrap();
            let min_cov: f64 = conn
                .query_row(
                    "SELECT MIN(coverage) FROM chunk_exemptions WHERE job_id=?1",
                    [&job_id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(min_cov >= 0.8, "豁免块覆盖率均应≥0.8，最低 {min_cov}");
        }

        // 关闭对减：matrixOriginal 与 matrix 逐格一致（口径回退，向后兼容）。
        let cctx_off = ctx_for(&pool, &ws, "compare", false);
        let cfg_off = CompareRunConfig { subtract_tender: false, ..cfg_with(ids, 0.5) };
        run_compare(&cctx_off, Arc::new(Jieba::new()), Arc::new(Mutex::new(None)), &ws, &cfg_off).unwrap();
        let voff: serde_json::Value = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &cctx_off.job_id).unwrap();
            serde_json::from_str(&r.matrix_json.unwrap()).unwrap()
        };
        assert_eq!(voff["matrix"], voff["matrixOriginal"], "关闭对减时两矩阵应一致");
        let n_off: i64 = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM chunk_exemptions WHERE job_id=?1",
                [&cctx_off.job_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(n_off, 0, "关闭对减不产生豁免证据");

        let _ = std::fs::remove_dir_all(&dir);
    }

    // —— W3-4 内置静态范本背景库：boiler_fraction 豁免 + 可复现 ——

    /// 三份投标共享同一法定套话段（廉政承诺，属静态背景库）与一段库中不存在的私有原创段：
    /// 套话段 boiler_fraction≥0.6 → 进 chunk_exemptions(kind='background')、不进聚类；
    /// 私有段不被豁免、照常聚类；同库同输入两次比对背景豁免集合逐字节一致（可复现）。
    #[test]
    fn background_boilerplate_is_exempted_private_content_is_not() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "背景库").unwrap().id
        };
        // 法定套话（与 fixtures/templates 中廉政承诺逐字一致 → 全 4-gram df=5 → boiler_fraction≈1）。
        let boiler = "为维护公平竞争的招标投标秩序，我方郑重承诺，在参与本次投标活动中自觉遵守国家有关法律法规和廉政建设的各项规定，不向招标人评标委员会成员及有关工作人员行贿或者提供其他不正当利益，不与其他投标人相互串通投标报价，不以任何方式排挤其他投标人的公平竞争，自觉维护招标投标活动的正常秩序。";
        // 库中不存在的私有原创段（三家共享 → 应聚类，且不得被背景库豁免）。
        let private = "本公司自主研发的智能边缘计算调度平台采用容器化微服务架构实现全链路可观测与弹性伸缩并通过自研分布式一致性算法保障多活数据中心的强一致性。";
        let mk = |head: &str| format!("{head}\n{boiler}\n{private}");
        let files = vec![
            ("投标A.txt", mk("投标单位甲：华信智联科技有限公司")),
            ("投标B.txt", mk("投标单位乙：启明数字技术有限公司")),
            ("投标C.txt", mk("投标单位丙：中科盛世信息股份公司")),
        ];
        let (job_id, ids) = import_and_compare(&pool, &ws, &files, 0.5);

        // (1) 背景套话段各家一段 → 3 条 kind='background' 证据，coverage(=boiler_fraction)≥0.6，无 tender。
        let (n_bg, min_cov): (i64, f64) = {
            let conn = pool.get().unwrap();
            let n = conn
                .query_row(
                    "SELECT COUNT(*) FROM chunk_exemptions WHERE job_id=?1 AND kind='background'",
                    [&job_id],
                    |r| r.get(0),
                )
                .unwrap();
            let c = conn
                .query_row(
                    "SELECT MIN(coverage) FROM chunk_exemptions WHERE job_id=?1 AND kind='background'",
                    [&job_id],
                    |r| r.get(0),
                )
                .unwrap();
            (n, c)
        };
        assert_eq!(n_bg, 3, "三家各一段法定套话 → 3 条背景豁免");
        assert!(min_cov >= 0.6, "背景豁免块 boiler_fraction 均应≥0.6，最低 {min_cov}");
        let n_tender: i64 = {
            let conn = pool.get().unwrap();
            conn.query_row(
                "SELECT COUNT(*) FROM chunk_exemptions WHERE job_id=?1 AND kind='tender'",
                [&job_id],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(n_tender, 0, "无招标文件 → 无 tender 豁免");

        let summary: CompareSummary = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &job_id).unwrap();
            serde_json::from_str(&r.summary_json.unwrap()).unwrap()
        };
        assert_eq!(summary.background_exempt_chunk_count, 3);
        assert_eq!(summary.tender_ref_chunk_count, 0);

        // (2) 套话段不进聚类（廉政承诺独有串「串通投标报价」不出现在任何簇成员）；
        //     库中不存在的私有段照常聚类（「边缘计算调度平台」出现在某簇）。
        let clusters = clusters_of(&pool, &job_id);
        let (mut saw_boiler, mut saw_private) = (false, false);
        {
            let conn = pool.get().unwrap();
            for c in &clusters {
                let detail = compare_repo::get_cluster_detail(&conn, &c.id).unwrap();
                for mem in &detail.members {
                    if mem.text.contains("串通投标报价") {
                        saw_boiler = true;
                    }
                    if mem.text.contains("边缘计算调度平台") {
                        saw_private = true;
                    }
                }
            }
        }
        assert!(!saw_boiler, "法定套话段应被背景库剔除，不应出现在任何聚类");
        assert!(saw_private, "库中不存在的本场私有段应照常聚类（未被误豁免）");

        // (3) 可复现：同库同输入再跑一次，kind='background' 的 (chunk_id, coverage) 集合逐字节一致。
        let bg_set = |jid: &str| -> Vec<(String, f64)> {
            let conn = pool.get().unwrap();
            let mut stmt = conn
                .prepare(
                    "SELECT chunk_id, coverage FROM chunk_exemptions
                     WHERE job_id=?1 AND kind='background' ORDER BY chunk_id",
                )
                .unwrap();
            let rows = stmt
                .query_map([jid], |r| Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?)))
                .unwrap();
            rows.map(|r| r.unwrap()).collect()
        };
        let first = bg_set(&job_id);
        assert_eq!(first.len(), 3);
        let cctx2 = ctx_for(&pool, &ws, "compare", false);
        run_compare(
            &cctx2,
            Arc::new(Jieba::new()),
            Arc::new(Mutex::new(None)),
            &ws,
            &cfg_with(ids, 0.5),
        )
        .unwrap();
        assert_eq!(first, bg_set(&cctx2.job_id), "同库同输入两次比对背景豁免集合应逐字节一致");
    }

    // —— M4 招标文件豁免接线 + 条件化硬命中 floor（§1.5）——

    fn collusion_of(pool: &DbPool, job_id: &str) -> crate::engine::report::Collusion {
        let conn = pool.get().unwrap();
        let r = job_repo::get_result_jsons(&conn, job_id).unwrap();
        serde_json::from_str(&r.collusion_json.unwrap()).unwrap()
    }

    /// 给某文档注入一张内嵌图片指纹（模拟招标方统一提供、各家照贴的同一张图）。
    fn seed_image(pool: &DbPool, doc_id: &str, sha: &str) {
        let mut conn = pool.get().unwrap();
        let tx = conn.transaction().unwrap();
        image_repo::insert_images(
            &tx,
            doc_id,
            &[crate::engine::parse::ImageHash {
                source: "docx",
                page: None,
                width: 320,
                height: 240,
                sha256: sha.to_string(),
                dhash: Some(0x0f0f_0f0f_0f0f_0f0f),
            }],
        )
        .unwrap();
        tx.commit().unwrap();
    }

    /// 招标文件 T（含模板 rsid 集 / 统一图片 / 共同标点笔误）+ 两份各自引用它的投标 C/D：
    /// 工作区已导入 T 且开启对减后，rsid / imageReuse / sharedErrors 均【不】因 T 的共享内容触发；
    /// 关闭对减（等价于无 T 可用）时同样输入照常触发（回归）。
    #[test]
    fn tender_shared_signals_are_exempted_and_fire_without_tender() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "招标豁免接线").unwrap().id
        };
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_ex_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        // 招标模板 rsid（含 rsidRoot），C/D 各自照用；共同标点笔误「质量。。管理」逐字源自 T。
        let tpl_rsids = r#"<w:rsidRoot w:val="00TP0001"/><w:rsid w:val="00TP0001"/><w:rsid w:val="00TP0002"/><w:rsid w:val="00TP0003"/>"#;
        let err_phrase = "严格执行质量。。管理体系认证";
        let tender = write_forensic_docx(
            &dir, "招标文件.docx",
            &format!("招标人要求投标人{err_phrase}并按本章逐项应答全部技术条款。"),
            tpl_rsids, "2024-05-01T09:00:00Z", "Normal.dotm",
        );
        let c = write_forensic_docx(
            &dir, "投标C.docx",
            &format!("华信智联科技公司专注城市轨道信号系统的设计集成；{err_phrase}。"),
            tpl_rsids, "2024-05-02T10:00:00Z", "Normal.dotm",
        );
        let d = write_forensic_docx(
            &dir, "投标D.docx",
            &format!("启明数字技术公司主营医院信息化平台建设运营；{err_phrase}。"),
            tpl_rsids, "2024-05-09T14:00:00Z", "Normal.dotm",
        );
        let it = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&it, jieba.clone(), &ws, &[tender], &Default::default(), "tender").unwrap();
        let ib = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ib, jieba.clone(), &ws, &[c, d], &Default::default(), "bid").unwrap();

        let (tender_id, ids) = {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            let tid = docs.iter().find(|x| x.file_name == "招标文件.docx").unwrap().id.clone();
            let bids: Vec<String> = ["投标C.docx", "投标D.docx"]
                .iter()
                .map(|n| docs.iter().find(|x| &x.file_name == n).unwrap().id.clone())
                .collect();
            (tid, bids)
        };
        // 招标方统一提供的同一张图片（同 sha）：T 与 C/D 各持一份。
        let img_sha = "aa11bb22cc33dd44ee55ff6600112233445566778899aabbccddeeff00112233";
        seed_image(&pool, &tender_id, img_sha);
        seed_image(&pool, &ids[0], img_sha);
        seed_image(&pool, &ids[1], img_sha);

        // (A) 开启对减（T 已导入）：三类信号均不因 T 的共享内容触发，亦无 forensicFloor。
        let on = ctx_for(&pool, &ws, "compare", false);
        run_compare(&on, jieba.clone(), Arc::new(Mutex::new(None)), &ws, &cfg_with(ids.clone(), 0.5)).unwrap();
        let col_on = collusion_of(&pool, &on.job_id);
        assert!(col_on.signals.iter().all(|s| s.kind != "rsid"), "招标模板 rsid 应被对减，不触发 rsid 信号");
        assert!(col_on.signals.iter().all(|s| s.kind != "imageReuse"), "招标方统一图片应被对减，不触发 imageReuse");
        assert!(col_on.signals.iter().all(|s| s.kind != "sharedErrors"), "源自 T 的共同笔误应被对减，不触发 sharedErrors");
        assert!(col_on.signals.iter().all(|s| s.kind != "forensicFloor"), "无残余硬命中 → 不置等级下限");

        // (B) 关闭对减（等价于无 T 可用）：同样输入照常触发三类信号（回归）。
        let off = ctx_for(&pool, &ws, "compare", false);
        let cfg_off = CompareRunConfig { subtract_tender: false, ..cfg_with(ids, 0.5) };
        run_compare(&off, jieba, Arc::new(Mutex::new(None)), &ws, &cfg_off).unwrap();
        let col_off = collusion_of(&pool, &off.job_id);
        assert!(col_off.signals.iter().any(|s| s.kind == "rsid"), "无对减 → 共享 rsid 照常触发");
        assert!(col_off.signals.iter().any(|s| s.kind == "imageReuse"), "无对减 → 同图照常触发");
        assert!(col_off.signals.iter().any(|s| s.kind == "sharedErrors"), "无对减 → 共同笔误照常触发");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// 硬命中（投标间共享一个【非招标模板】的 rsidRoot，扣除 T 的 rsid 集后仍命中）：
    /// 工作区已导入 T 且对减生效 → 强制等级下限 medium + forensicFloor 纪律信号；
    /// 关闭对减（无 T 可用）→ 硬命中仅作 rsid 信号展示，不置等级下限（无 forensicFloor）。
    #[test]
    fn conditional_hard_hit_floor_requires_tender_exemption() {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "条件化下限").unwrap().id
        };
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_floor_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();

        let tender = write_forensic_docx(
            &dir, "招标文件.docx",
            "招标人提供统一投标文件模板，投标人应据此编制并逐项响应技术条款。",
            r#"<w:rsidRoot w:val="00TP0001"/><w:rsid w:val="00TP0002"/><w:rsid w:val="00TP0003"/>"#,
            "2024-05-01T09:00:00Z", "Normal.dotm",
        );
        // E/F 共享一个非模板 rsidRoot 00CO0009（扣除 T 后仍硬命中），另各带模板 rsid（被对减）。
        let coll_rsids = r#"<w:rsidRoot w:val="00CO0009"/><w:rsid w:val="00CO0009"/><w:rsid w:val="00TP0002"/><w:rsid w:val="00TP0003"/>"#;
        let e = write_forensic_docx(
            &dir, "投标E.docx",
            "远东建工集团承担综合管廊与地下空间开发的施工总承包业务。",
            coll_rsids, "2024-05-02T10:00:00Z", "Normal.dotm",
        );
        let f = write_forensic_docx(
            &dir, "投标F.docx",
            "北方勘察设计院提供水利枢纽与灌区改造的全过程工程咨询。",
            coll_rsids, "2024-05-08T16:00:00Z", "Normal.dotm",
        );
        let it = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&it, jieba.clone(), &ws, &[tender], &Default::default(), "tender").unwrap();
        let ib = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ib, jieba.clone(), &ws, &[e, f], &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            ["投标E.docx", "投标F.docx"]
                .iter()
                .map(|n| docs.iter().find(|x| &x.file_name == n).unwrap().id.clone())
                .collect()
        };

        // (A) 对减生效：非模板 rsidRoot 存活 → 硬命中 → 等级下限 medium + forensicFloor。
        let on = ctx_for(&pool, &ws, "compare", false);
        run_compare(&on, jieba.clone(), Arc::new(Mutex::new(None)), &ws, &cfg_with(ids.clone(), 0.5)).unwrap();
        let col_on = collusion_of(&pool, &on.job_id);
        assert!(col_on.signals.iter().any(|s| s.kind == "rsid"), "非模板 rsidRoot 应存活并触发 rsid 信号");
        let floor = col_on
            .signals
            .iter()
            .find(|s| s.kind == "forensicFloor")
            .expect("对减生效+硬命中 → 应有 forensicFloor 信号");
        assert!(floor.detail.contains("已扣除招标文件统一下发模板"), "floor 文案应说明扣除模板后仍硬命中");
        assert!(matches!(col_on.level.as_str(), "medium" | "high"), "等级下限应≥medium，实际 {}", col_on.level);

        // (B) 关闭对减（无 T 可用）：硬命中仅作 rsid 信号展示，不置等级下限。
        let off = ctx_for(&pool, &ws, "compare", false);
        let cfg_off = CompareRunConfig { subtract_tender: false, ..cfg_with(ids, 0.5) };
        run_compare(&off, jieba, Arc::new(Mutex::new(None)), &ws, &cfg_off).unwrap();
        let col_off = collusion_of(&pool, &off.job_id);
        assert!(col_off.signals.iter().any(|s| s.kind == "rsid"), "无对减 → rsid 信号仍展示");
        assert!(
            col_off.signals.iter().all(|s| s.kind != "forensicFloor"),
            "无对减 → 不置等级下限（无 forensicFloor）"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // —— W3-3 k-共现过滤升级：≥3 家共有先查证，查得到豁免、查不到升级『多家异常一致·待复核』——

    /// 招标文件（可选）+ 3 份投标 → 导入 → 比对，返回 (pool, ws, job_id, bid_ids)。
    /// mark_tender_ocr=true 时把招标文件 parse_method 改为 'ocr'（模拟扫描件，触发查证质量闸门）。
    fn setup_kcooc(
        name: &str,
        tender: Option<&str>,
        bids: &[(&str, String)],
        mark_tender_ocr: bool,
    ) -> (DbPool, String, String, Vec<String>) {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, name).unwrap().id
        };
        let jieba = Arc::new(Jieba::new());
        let dir = std::env::temp_dir().join(format!("bg_kc_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |n: &str, c: &str| {
            let p = dir.join(n);
            std::fs::write(&p, c).unwrap();
            p.to_string_lossy().into_owned()
        };
        if let Some(t) = tender {
            let tp = write("招标文件.txt", t);
            let it = ctx_for(&pool, &ws, "import", false);
            import_service::run_import(&it, jieba.clone(), &ws, &[tp], &Default::default(), "tender").unwrap();
            if mark_tender_ocr {
                let conn = pool.get().unwrap();
                conn.execute("UPDATE documents SET parse_method='ocr' WHERE doc_role='tender'", []).unwrap();
            }
        }
        let bid_paths: Vec<String> = bids.iter().map(|(n, c)| write(n, c)).collect();
        let ib = ctx_for(&pool, &ws, "import", false);
        import_service::run_import(&ib, jieba.clone(), &ws, &bid_paths, &Default::default(), "bid").unwrap();
        let ids: Vec<String> = {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            bids.iter()
                .map(|(n, _)| docs.iter().find(|d| &d.file_name == n).unwrap().id.clone())
                .collect()
        };
        let cctx = ctx_for(&pool, &ws, "compare", false);
        run_compare(&cctx, jieba, Arc::new(Mutex::new(None)), &ws, &cfg_with(ids.clone(), 0.5)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        (pool, ws, cctx.job_id, ids)
    }

    /// 招标条款（三家共引，boiler<0.6、非套话）。
    const TENDER_X: &str = "招标人要求所有投标人严格按照本章技术规范逐项应答，全部核心设备必须为原厂全新正品并随附完整的出厂合格证明与第三方检验报告，投标文件须对本节全部技术条款作出实质性响应，不得存在任何负偏离，否则将按无效投标处理并不予评审。";
    /// 库中不存在的私有原创段（三家共享 → 查不到出处）。
    const PRIVATE_P: &str = "本公司自主研发的智能边缘计算调度平台采用容器化微服务架构实现全链路可观测与弹性伸缩并通过自研分布式一致性算法保障多活数据中心的强一致性与秒级故障切换能力。";

    /// (1) 三份投标共享某段且该段在招标文件中（多数成员覆盖率≥0.8）→ 簇 exempt_reason='tender'、
    ///     无 multiDocAnomaly 信号、信号②不含该簇。
    #[test]
    fn kcooc_tender_shared_cluster_is_exempted_not_anomaly() {
        let y = "此外我方将为本项目单独配置驻场质量总监并按周提交独立第三方质检报告以确保交付质量达到预期。";
        let bids = vec![
            ("投标A.txt", format!("华信智联\n{TENDER_X}")),
            ("投标B.txt", format!("启明数字\n{TENDER_X}")),
            // C 在同段追加私有句 → 覆盖率<0.8（多数成员仍≥0.8），且残差簇经 C 桥接存活可标记。
            ("投标C.txt", format!("中科盛世\n{TENDER_X}{y}")),
        ];
        let (pool, _ws, job_id, _ids) = setup_kcooc("kcooc豁免", Some(TENDER_X), &bids, false);
        let clusters = clusters_of(&pool, &job_id);
        let c3 = clusters
            .iter()
            .find(|c| c.document_ids.len() == 3)
            .expect("应有 3 家共有簇（引用招标段，经 C 桥接存活）");
        assert_eq!(c3.exempt_reason.as_deref(), Some("tender"), "多数成员引用招标文件 → tender 豁免");
        assert!(!c3.multi_doc_anomaly, "豁免簇不得标异常");
        let col = collusion_of(&pool, &job_id);
        assert!(col.signals.iter().all(|s| s.kind != "multiDocAnomaly"), "豁免簇不产生 multiDocAnomaly 信号");
        assert!(col.signals.iter().all(|s| s.kind != "cluster"), "豁免簇退出围标信号②（无 cluster 信号）");
    }

    /// (2) 同场景但该段不在招标/背景、且查证条件具备（招标文件已导入、非 OCR、覆盖率抽样达标）
    ///     → multi_doc_anomaly=1、severity='review'（待复核·非 high）、signals 含 multiDocAnomaly、
    ///     detail 含『涉嫌』+『评标委员会』。
    #[test]
    fn kcooc_private_shared_triggers_multi_doc_anomaly_review() {
        let bids = vec![
            ("投标A.txt", format!("华信智联\n{TENDER_X}\n{PRIVATE_P}")),
            ("投标B.txt", format!("启明数字\n{TENDER_X}\n{PRIVATE_P}")),
            ("投标C.txt", format!("中科盛世\n{TENDER_X}\n{PRIVATE_P}")),
        ];
        let (pool, _ws, job_id, _ids) = setup_kcooc("kcooc异常", Some(TENDER_X), &bids, false);
        let clusters = clusters_of(&pool, &job_id);
        let anom = clusters
            .iter()
            .find(|c| c.multi_doc_anomaly)
            .expect("私有共有段查不到出处 → 应升级为多家异常一致");
        assert_eq!(anom.document_ids.len(), 3);
        assert_eq!(anom.severity.as_deref(), Some("review"), "异常簇 severity='review'（待复核·非 high）");
        assert!(anom.exempt_reason.is_none(), "异常簇非豁免");
        let summary = anom.summary.clone().unwrap_or_default();
        assert!(
            summary.contains("涉嫌") && summary.contains("评标委员会"),
            "簇 summary 应含『涉嫌』+『评标委员会』脚注：{summary}"
        );
        // high 风险统计不含异常簇（§1.5：不自动 high）
        let summary_obj: CompareSummary = {
            let conn = pool.get().unwrap();
            let r = job_repo::get_result_jsons(&conn, &job_id).unwrap();
            serde_json::from_str(&r.summary_json.unwrap()).unwrap()
        };
        assert_eq!(summary_obj.high_risk_count, 0, "多家异常一致不进 high 风险统计");
        let col = collusion_of(&pool, &job_id);
        let s = col
            .signals
            .iter()
            .find(|s| s.kind == "multiDocAnomaly")
            .expect("signals 应含 multiDocAnomaly");
        assert!(
            s.detail.contains("涉嫌") && s.detail.contains("评标委员会") && s.detail.contains("第四十条"),
            "信号 detail 应含涉嫌+法条+评标委员会：{}",
            s.detail
        );
        assert!(col.signals.iter().all(|s| s.kind != "cluster"), "异常簇退出信号②");
        assert_ne!(col.level, "high", "多家异常一致不得把总判定自动抬为 high");
    }

    /// (3) 招标文件为 OCR/扫描件 → 禁用 anomaly 升级、降级中性提示『出处未能核实』（不带法条、不升 severity）。
    #[test]
    fn kcooc_ocr_tender_disables_upgrade_neutral_prompt() {
        let bids = vec![
            ("投标A.txt", format!("华信智联\n{TENDER_X}\n{PRIVATE_P}")),
            ("投标B.txt", format!("启明数字\n{TENDER_X}\n{PRIVATE_P}")),
            ("投标C.txt", format!("中科盛世\n{TENDER_X}\n{PRIVATE_P}")),
        ];
        let (pool, _ws, job_id, _ids) = setup_kcooc("kcoocOCR", Some(TENDER_X), &bids, true);
        let clusters = clusters_of(&pool, &job_id);
        let p3 = clusters
            .iter()
            .find(|c| c.document_ids.len() == 3 && c.exempt_reason.is_none())
            .expect("应有 3 家私有共有簇");
        assert!(!p3.multi_doc_anomaly, "招标件 OCR → 禁用异常升级");
        assert_ne!(p3.severity.as_deref(), Some("review"), "OCR 时不置待复核");
        let summary = p3.summary.clone().unwrap_or_default();
        assert!(summary.contains("出处未能核实"), "OCR 时降级中性提示：{summary}");
        assert!(!summary.contains("涉嫌"), "中性提示不带『涉嫌』措辞");
        let col = collusion_of(&pool, &job_id);
        assert!(col.signals.iter().all(|s| s.kind != "multiDocAnomaly"), "OCR 时无 multiDocAnomaly 信号");
    }

    /// (4) 无招标文件且背景库不可作出处升级 → 不升级异常，维持既有行为（≥3 家共有仍按信号②计数）。
    #[test]
    fn kcooc_no_tender_keeps_existing_behavior() {
        let bids = vec![
            ("投标A.txt", format!("华信智联\n{PRIVATE_P}")),
            ("投标B.txt", format!("启明数字\n{PRIVATE_P}")),
            ("投标C.txt", format!("中科盛世\n{PRIVATE_P}")),
        ];
        let (pool, _ws, job_id, _ids) = setup_kcooc("kcooc无招标", None, &bids, false);
        let clusters = clusters_of(&pool, &job_id);
        let c3 = clusters.iter().find(|c| c.document_ids.len() == 3).expect("应有 3 家共有簇");
        assert!(!c3.multi_doc_anomaly, "无招标文件 → 不升级异常");
        assert!(c3.exempt_reason.is_none(), "无招标文件 → 不豁免");
        let summary = c3.summary.clone().unwrap_or_default();
        assert!(
            !summary.contains("出处未能核实") && !summary.contains("涉嫌"),
            "无招标文件不加任何 k-共现提示：{summary}"
        );
        let col = collusion_of(&pool, &job_id);
        assert!(col.signals.iter().all(|s| s.kind != "multiDocAnomaly"), "无招标文件无异常信号");
        assert!(col.signals.iter().any(|s| s.kind == "cluster"), "≥3 家共有簇仍按信号②计数（既有行为不变）");
    }
}
