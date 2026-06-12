// 比对服务：算法管线 v2 的编排。
// 读分块 → TF-IDF → （可选）语义向量（缓存命中跳过）→ 多通道召回 → 五维精排 →
// 并查集聚类 → 八类分类 + 分级 diff → 单事务落库 → 矩阵/围标/共有词/章节热力聚合。
// 取消或失败时清掉本任务的全部半成品。
use crate::db::repo::compare_repo::{self, NewCluster, NewDiff, NewEdge, NewMember};
use crate::db::repo::{chunk_repo, document_repo, embedding_repo, job_repo};
use crate::db::repo::document_repo::DocumentRow;
use crate::engine::clustering::{self, ScoredEdge};
use crate::engine::corpus::{self, CmpChunk};
use crate::engine::report::{Cluster as RCluster, ClusterSeg, DocInfo, Fingerprint, SectionStat, SharedTerm};
use crate::engine::{candidate, collusion, diff, embed, fact, fingerprint, matrix, scoring};
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::jobs::JobCtx;
use fastembed::TextEmbedding;
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
    /// security.allowCloudModel：是否允许联网下载语义模型（本地已缓存时不受限）。
    #[serde(default)]
    pub allow_model_download: bool,
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

/// 一对分块的有效相似阈值：任一侧为短文本则上浮（封顶 0.98 防不可达）。
fn effective_threshold(base: f32, a: &CmpChunk, b: &CmpChunk) -> f32 {
    if a.char_count.min(b.char_count) < SHORT_TEXT_CHARS {
        (base + SHORT_TEXT_BUMP).min(0.98)
    } else {
        base
    }
}
/// 视为「基准文档内容缺失」前，允许的最高近似分（有更高的近似 → uncertain 而非 deleted）
const DELETED_FLOOR: f32 = 0.55;

pub fn run_compare(
    ctx: &JobCtx,
    jieba: Arc<Jieba>,
    embedder: Arc<Mutex<Option<TextEmbedding>>>,
    workspace_id: &str,
    cfg: &CompareRunConfig,
) -> AppResult<()> {
    let r = run_inner(ctx, &jieba, &embedder, workspace_id, cfg);
    if r.is_err() {
        if let Ok(conn) = ctx.db.get() {
            let _ = compare_repo::delete_job_results(&conn, &ctx.job_id);
        }
    }
    r
}

fn run_inner(
    ctx: &JobCtx,
    jieba: &Jieba,
    embedder: &Arc<Mutex<Option<TextEmbedding>>>,
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

    let mut comparable: Vec<CmpChunk> = Vec::new();
    {
        let conn = ctx.db.get()?;
        for (di, d) in docs.iter().enumerate() {
            let rows = chunk_repo::load_for_compare(&conn, &d.id, &cfg.chunk_level)?;
            let total = rows.len();
            for row in rows {
                let c = corpus::from_row(row, di, total);
                // 比对范围：tech 排除商务段，business 排除技术段
                let keep_scope = match cfg.scope.as_str() {
                    "tech" => c.section_kind != "business",
                    "business" => c.section_kind != "tech",
                    _ => true,
                };
                let keep_template = !(cfg.ignore_templates && c.is_template);
                if keep_scope && keep_template && !c.tokens.is_empty() {
                    comparable.push(c);
                }
            }
        }
    }
    ctx.check()?;
    corpus::fill_tfidf(&mut comparable);

    // 2) 语义向量（可选；按 normalized_hash 全局缓存）
    let (embeddings, semantic_degraded) = if cfg.enable_semantic {
        embed_chunks(ctx, embedder, &comparable, cfg.allow_model_download)?
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
    let (edges, best): (Vec<ScoredEdge>, HashMap<u32, f32>) = cands
        .par_iter()
        .fold(
            || (Vec::new(), HashMap::new()),
            |(mut es, mut best), &(i, j)| {
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
                if parts.final_score
                    >= effective_threshold(threshold, &comparable[i as usize], &comparable[j as usize])
                {
                    es.push(ScoredEdge { a: i, b: j, parts });
                }
                (es, best)
            },
        )
        .reduce(
            || (Vec::new(), HashMap::new()),
            |(mut e1, mut b1), (e2, b2)| {
                e1.extend(e2);
                for (k, v) in b2 {
                    let e = b1.entry(k).or_insert(0.0f32);
                    *e = e.max(v);
                }
                (e1, b1)
            },
        );
    ctx.check()?;

    // 5) 聚类
    ctx.progress("cluster", 0, 1, "聚合雷同条款");
    let mut raw = clustering::cluster(&comparable, &edges, cfg.similarity_threshold);
    raw.truncate(MAX_STORE_CLUSTERS);

    // 6) 分类 + diff + 组装入库结构
    let base_idx = cfg
        .base_document_id
        .as_ref()
        .and_then(|id| docs.iter().position(|d| d.id == *id));
    let mut new_clusters =
        build_clusters(jieba, &comparable, &docs, &raw, base_idx, cfg.detect_moved_paragraph);

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
        crate::db::repo::fact_repo::replace_for_chunks(&tx, &fact_rows)?;
        tx.commit()?;
    }
    ctx.check()?;

    // 8) 聚合：矩阵 / 章节热力 / 共有特征词 / 围标判定 / 总览
    let (m, peak) = matrix::doc_matrix(docs.len(), &comparable, &raw);
    let sections = section_stats(&comparable, &best);
    let shared = shared_terms_of(&comparable);

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
        })
        .collect();
    fingerprint::cross_flags(&mut doc_infos);

    // 围标判定复用旧引擎的信号加权（输入适配为 report::Cluster）
    let r_clusters: Vec<RCluster> = raw
        .iter()
        .map(|rc| {
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
            }
        })
        .collect();
    let r_shared: Vec<SharedTerm> = shared.clone();
    // 报价梯度信号：金额接近但不同 + 多处条款雷同（典型陪标价特征）
    let price_pairs = price_proximity(&comparable, docs.len(), &raw);
    let collusion =
        collusion::assess_with(peak, &r_clusters, &doc_infos, &r_shared, &price_pairs);

    let mut summary = CompareSummary {
        document_count: docs.len(),
        chunk_count: comparable.len(),
        cluster_count: new_clusters.len() + deleted.len(),
        semantic_degraded,
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
        if c.severity == "high" {
            summary.high_risk_count += 1;
        }
    }

    let matrix_json = serde_json::json!({
        "documentIds": cfg.document_ids,
        "matrix": m,
        "peak": peak,
    });
    {
        let conn = ctx.db.get()?;
        job_repo::set_compare_results(
            &conn,
            &ctx.job_id,
            &serde_json::to_string(&summary).unwrap_or_default(),
            &matrix_json.to_string(),
            &serde_json::to_string(&collusion).unwrap_or_default(),
            &serde_json::to_string(&shared).unwrap_or_default(),
            &serde_json::to_string(&sections).unwrap_or_default(),
        )?;
    }
    ctx.progress("done", 1, 1, "完成");
    Ok(())
}

/// 语义向量：唯一 normalized_hash 查缓存 → 缺失的批量嵌入并回写。
/// 模型不可用（含 allowCloudModel=false 且本地无缓存）时优雅降级
/// （返回 degraded=true，比对退回词面权重组）。
fn embed_chunks(
    ctx: &JobCtx,
    embedder: &Arc<Mutex<Option<TextEmbedding>>>,
    chunks: &[CmpChunk],
    allow_download: bool,
) -> AppResult<(Option<ChunkEmbeddings>, bool)> {
    let mut uniq: HashMap<&str, &str> = HashMap::new(); // hash → text
    for c in chunks {
        uniq.entry(&c.normalized_hash).or_insert(&c.text);
    }
    let hashes: Vec<String> = uniq.keys().map(|s| s.to_string()).collect();
    let mut cache = {
        let conn = ctx.db.get()?;
        embedding_repo::get_many(&conn, &hashes, embed::MODEL_ID)?
    };

    let missing: Vec<(String, String)> = uniq
        .iter()
        .filter(|(h, _)| !cache.contains_key(**h))
        .map(|(h, t)| (h.to_string(), t.to_string()))
        .collect();

    let total = missing.len().max(1);
    ctx.progress("semantic", 0, total, format!("语义向量（缓存命中 {}）", cache.len()));

    if !missing.is_empty() {
        let mut guard = embedder.lock().unwrap();
        let Some(model) = embed::ensure(&mut guard, allow_download) else {
            ctx.progress("semantic", total, total, "语义模型不可用，降级为词面比对");
            return Ok((None, true));
        };
        for (bi, batch) in missing.chunks(EMBED_BATCH).enumerate() {
            ctx.check()?;
            let texts: Vec<String> = batch.iter().map(|(_, t)| t.clone()).collect();
            let Some(vecs) = embed::embed_batch(model, &texts) else {
                ctx.progress("semantic", total, total, "语义嵌入失败，降级为词面比对");
                return Ok((None, true));
            };
            let items: Vec<(String, Vec<f32>)> = batch
                .iter()
                .zip(vecs)
                .map(|((h, _), v)| (h.clone(), v))
                .collect();
            {
                let conn = ctx.db.get()?;
                embedding_repo::insert_many(&conn, &items, embed::MODEL_ID)?;
            }
            for (h, v) in items {
                cache.insert(h, v);
            }
            ctx.progress(
                "semantic",
                (bi + 1) * EMBED_BATCH.min(total),
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
                members,
                diffs,
            }
        })
        .collect()
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
            nc.summary = Some(format!(
                "同一条款关键数字不一致（{}）",
                conflict
                    .fields
                    .iter()
                    .map(|f| match f.field.as_str() {
                        "amount" => "金额",
                        "duration" => "工期",
                        "date" => "日期",
                        _ => "比例",
                    })
                    .collect::<Vec<_>>()
                    .join("、")
            ));
            nc.conflict_json = serde_json::to_string(&conflict).ok();
        }
    }
}

/// 报价梯度：每文档取最大金额（投标报价通常是全文最大额），
/// 两文档共享 ≥3 个雷同条款且金额差 0 < gap < 3% → 信号。
fn price_proximity(
    chunks: &[CmpChunk],
    n_docs: usize,
    raw: &[clustering::RawCluster],
) -> Vec<collusion::PriceProximity> {
    let mut max_amount: Vec<Option<u64>> = vec![None; n_docs];
    for c in chunks {
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

/// 共有特征词：≥4 字、被 ≥2 份文档共用的词（疑似同源 / 共用笔误），top 30。
fn shared_terms_of(chunks: &[CmpChunk]) -> Vec<SharedTerm> {
    let mut map: HashMap<&str, BTreeSet<usize>> = HashMap::new();
    for c in chunks {
        for t in &c.tokens {
            if t.chars().count() >= 4 {
                map.entry(t.as_str()).or_default().insert(c.doc);
            }
        }
    }
    let mut out: Vec<SharedTerm> = map
        .into_iter()
        .filter(|(_, docs)| docs.len() >= 2)
        .map(|(term, docs)| SharedTerm {
            term: term.to_string(),
            docs: docs.into_iter().collect(),
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

// 端到端：导入（import_service）→ 比对（本服务）→ 校验断言从旧引擎测试逐条平移。
// 这组测试是阶段 3 的「校准门禁」：权重/阈值改动必须保证正负向同时通过。
#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::repo::workspace_repo;
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
            allow_model_download: false,
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
        import_service::run_import(&ictx, jieba.clone(), ws, &paths, &Default::default()).unwrap();

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
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default()).unwrap();
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
        import_service::run_import(&ictx, jieba.clone(), &ws, &[docx, xlsx], &Default::default())
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
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default()).unwrap();
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
        import_service::run_import(&ictx, jieba.clone(), &ws, &paths, &Default::default()).unwrap();
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
}
