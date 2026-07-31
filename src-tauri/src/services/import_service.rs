// 文档导入：校验 → sha256 哈希 → 去重（批内 / 工作区内 / 跨工作区缓存）→ 解析 →
// 结构化分块（三档粒度 + 标题路径 + 模板标记 + 特征）→ 批量事务入库。
// 解析失败只标记该文档 failed，不中断整个任务；取消时未入库的文档不留半成品。
use crate::db::repo::{chunk_repo, document_repo, image_repo, template_repo, workspace_repo};
use crate::db::repo::chunk_repo::NewChunk;
use crate::engine::chunker::{self, ChunkerOptions};
use crate::engine::parse;
use crate::engine::similarity::tokenize;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::jobs::JobCtx;
use jieba_rs::Jieba;
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

const ACCEPTED: &[&str] = &["docx", "pdf", "txt", "md", "xlsx", "xls"];

/// 导入期生效的解析配置（来自四层配置合并；见 ImportOptions::from_config）。
#[derive(Debug, Clone)]
pub struct ImportOptions {
    pub min_paragraph_chars: usize,
    pub normalize: crate::engine::normalize::NormalizeOptions,
    pub detect_table: bool,
    pub preserve_page_number: bool,
    pub remove_header_footer: bool,
    pub ocr_docx_images: bool,
    /// OCR 档位 key（PP-OCRv6 tiny/small/medium）。
    pub ocr_model: String,
    pub language: String, // auto | zh | en
    /// PDF 渲染-OCR 抽样交叉验证（W2-4）。本里程碑只预置键并计入 options_hash，
    /// 交叉验证行为在 M2 实现（执行方案全局裁决 3：options_hash 只 bump 一次 v5→v6）。
    pub pdf_cross_check: bool,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self::from_config(&crate::config::AppConfig::default())
    }
}

impl ImportOptions {
    pub fn from_config(cfg: &crate::config::AppConfig) -> Self {
        Self {
            min_paragraph_chars: cfg.parser.min_paragraph_length,
            normalize: crate::engine::normalize::NormalizeOptions {
                ignore_case: cfg.compare.ignore_case,
                ignore_punctuation: cfg.compare.ignore_punctuation,
                ignore_whitespace: cfg.compare.ignore_whitespace,
            },
            detect_table: cfg.parser.detect_table,
            preserve_page_number: cfg.parser.preserve_page_number,
            remove_header_footer: cfg.parser.remove_header_footer,
            ocr_docx_images: cfg.parser.ocr_docx_images,
            ocr_model: cfg.parser.ocr_model.clone(),
            language: cfg.compare.language.clone(),
            pdf_cross_check: cfg.parser.pdf_cross_check,
        }
    }

    /// 配置指纹：跨工作区分块缓存复用的匹配键（配置不同 → 分块不可互换）。
    /// templates_digest 是启用中查重源模板集的摘要——模板集是分块的真实输入（决定 is_template
    /// 标记），必须并入指纹：否则工作区 A 导入后增删模板、工作区 B 导入同一文件命中旧缓存，
    /// 会复用过期的 is_template 标记 → 新增模板不生效(误报)、停用模板仍剔除(漏报)。
    /// v5→v6 一次合并三件事（执行方案全局裁决 3「只 bump 一次」）：W2 归一化流水线变更
    /// （隐形码点剥离 + 同形字折叠改变 normalized_text/tokens）+ pdf_cross_check 预置键 +
    /// 取证指纹版本预置键（fpv，见 report::FINGERPRINT_SCHEMA_VERSION——M1 扩展 Fingerprint
    /// 时只把值 1→2，不再动版本前缀），旧缓存的分块/统计/指纹不可复用，必须整体失效重建；
    /// embedding 缓存按 normalized_hash 寻址，随之自然失效。
    pub fn options_hash(&self, templates_digest: &str) -> String {
        self.options_hash_with_versions(
            templates_digest,
            crate::engine::report::FINGERPRINT_SCHEMA_VERSION,
            crate::engine::pdf_audit::PDF_AUDIT_SCHEMA_VERSION,
        )
    }

    /// fpv + pav 双版本入参：常量在单测里无法「变化」，参数化让「fpv/pav 变则 hash 变」可直接
    /// 断言。fpv=取证指纹 schema 版本；pav=PDF 隐藏文字层审计 schema 版本——pdfAudit 是解析期
    /// 新产出、cache-hit 旧文档不会有它，bump pav 让 options_hash 变化、旧缓存整体失效重建
    /// （做法对齐 fpv，只改 VALUE 不动 v6 前缀）。生产路径经 options_hash 走两常量当前值。
    fn options_hash_with_versions(&self, templates_digest: &str, fpv: u32, pav: u32) -> String {
        let s = format!(
            "v6|min={}|case={}|punct={}|ws={}|tbl={}|page={}|hf={}|img={}|ocr={}|xchk={}|fpv={}|pav={}|lang={}|tpl={}",
            self.min_paragraph_chars,
            self.normalize.ignore_case,
            self.normalize.ignore_punctuation,
            self.normalize.ignore_whitespace,
            self.detect_table,
            self.preserve_page_number,
            self.remove_header_footer,
            self.ocr_docx_images,
            self.ocr_model,
            self.pdf_cross_check,
            fpv,
            pav,
            self.language,
            templates_digest,
        );
        crate::engine::normalize::sha256_hex(s.as_bytes())
    }
}

struct WorkItem {
    path: String,
    file_name: String,
    file_type: String,
    file_hash: String,
}

pub fn run_import(
    ctx: &JobCtx,
    jieba: Arc<Jieba>,
    workspace_id: &str,
    paths: &[String],
    opts: &ImportOptions,
    // 本批文件的文档角色（'bid' | 'tender' | 'tender_supplement'，command 层已校验取值）。
    // 请求级参数而非 ImportOptions：角色不影响解析产物，绝不能进 options_hash——
    // 否则同一文件换角色导入会错过跨工作区分块缓存。
    doc_role: &str,
) -> AppResult<()> {
    if paths.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "未选择任何文件"));
    }
    // 启用中的查重源模板：既用于分块标记样板段，也并入 options_hash——模板集是分块的真实
    // 输入，缓存复用必须同集才安全。摘要按 id 排序后 hash（与集合内容一一对应、与加载顺序无关）。
    let (chunker_opts, templates_digest) = {
        let conn = ctx.db.get()?;
        let raw = template_repo::list_enabled(&conn)?; // Vec<(id, text)>
        let mut keyed: Vec<(&str, &str)> = raw.iter().map(|(id, t)| (id.as_str(), t.as_str())).collect();
        keyed.sort_unstable();
        let mut digest_src = String::new();
        for (id, t) in &keyed {
            digest_src.push_str(id);
            digest_src.push('\u{1f}');
            digest_src.push_str(t);
            digest_src.push('\u{1e}');
        }
        let templates_digest = crate::engine::normalize::sha256_hex(digest_src.as_bytes());
        let templates: Vec<(String, Vec<String>)> = raw
            .into_iter()
            .map(|(id, t)| {
                // 模板分词与分块分词（chunker::make）走同一 sanitize 口径（NFKC+隐形
                // 剥离+同形折叠），否则模板余弦两边词面不一致，is_template 匹配失准；
                // 模板是用户维护的查重源，其规避统计无证据意义，丢弃。
                let (clean, _) = crate::engine::normalize::sanitize_with_stats(&t);
                (id, tokenize(&jieba, &clean))
            })
            .filter(|(_, t)| !t.is_empty())
            .collect();
        let opts_out = ChunkerOptions {
            min_chars: opts.min_paragraph_chars,
            templates,
            normalize: opts.normalize.clone(),
            detect_table: opts.detect_table,
            preserve_page_number: opts.preserve_page_number,
            language: opts.language.clone(),
        };
        (opts_out, templates_digest)
    };
    let options_hash = opts.options_hash(&templates_digest);

    // 阶段 A：顺序校验 + 哈希 + 去重（批内同内容文件只保留第一个；工作区内已有的跳过）
    let total = paths.len();
    let mut seen_hashes: HashSet<String> = HashSet::new();
    let mut work: Vec<WorkItem> = Vec::new();
    let mut skipped = 0usize;
    for (i, p) in paths.iter().enumerate() {
        ctx.check()?;
        let path = Path::new(p);
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(p)
            .to_string();
        ctx.progress("hash", i, total, format!("校验 {file_name}"));

        let file_type = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !ACCEPTED.contains(&file_type.as_str()) {
            // 老二进制格式给出可执行的出路，而不是干巴巴的「不支持」
            let msg = if matches!(file_type.as_str(), "doc" | "wps" | "et" | "rtf") {
                format!("「{file_name}」是旧格式，请用 Word/WPS 另存为 .docx（表格为 .xlsx）后再导入")
            } else {
                format!("暂不支持的文件类型：{file_name}")
            };
            return Err(AppError::new(AppErrorCode::UnsupportedFileType, msg));
        }
        if !path.is_file() {
            return Err(AppError::new(
                AppErrorCode::FileNotFound,
                format!("文件不存在：{file_name}"),
            )
            .with_detail(p.clone()));
        }
        let file_hash = hash_file(path, ctx)?;

        let dup_in_batch = !seen_hashes.insert(file_hash.clone());
        // 连接即取即还：progress() 自己也要取连接，持有期间调用会饿死小连接池。
        // 去重按同角色收窄：同一文件可以以 bid / tender 两种角色各存一份
        let dup_in_ws = {
            let conn = ctx.db.get()?;
            document_repo::find_by_hash(&conn, workspace_id, &file_hash, doc_role)?.is_some()
        };
        if dup_in_batch || dup_in_ws {
            skipped += 1;
            ctx.progress("hash", i + 1, total, format!("{file_name} 已存在，跳过"));
            continue;
        }
        // 重试路径：同 hash 同角色的失败残留行先清掉，避免重试成功后失败行与新行并存
        {
            let conn = ctx.db.get()?;
            document_repo::remove_failed_by_hash(&conn, workspace_id, &file_hash, doc_role)?;
        }
        work.push(WorkItem {
            path: p.clone(),
            file_name,
            file_type,
            file_hash,
        });
    }

    // 阶段 B：按文件并行解析（CPU 密集），但 DB 写入串行。
    // SQLite 单写者：并发的大文档写事务会撞 busy_timeout（SQLITE_BUSY）。写入持进程级写锁
    // （ctx.write_lock，见 JobManager::db_write）串行——不止本任务内部，跨任务/跨工作区的
    // 导入与比对写事务也共用一把，解析仍在锁外并行，既快又不冲突。
    let parse_total = work.len();
    let done = AtomicUsize::new(0);
    let results: Vec<AppResult<()>> = work
        .par_iter()
        .map(|item| {
            let r =
                import_one(ctx, &jieba, workspace_id, item, &chunker_opts, opts, &options_hash, doc_role);
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            ctx.progress("parse", n, parse_total, format!("已解析 {n} / {parse_total}"));
            r
        })
        .collect();

    ctx.check()?;
    // 解析失败已按文档落库；这里只向上传播数据库级错误
    for r in results {
        r?;
    }

    {
        let conn = ctx.db.get()?;
        workspace_repo::touch(&conn, workspace_id)?;
    }
    let note = if skipped > 0 {
        format!("导入完成（{skipped} 个重复文件已跳过）")
    } else {
        "导入完成".to_string()
    };
    ctx.progress("done", 1, 1, note);
    Ok(())
}

/// 单文件导入。返回 Err 仅用于数据库错误 / 取消；解析失败落到 documents.status=failed。
/// 解析（含 OCR）在写锁外并行；所有 DB 写入持 ctx.write_lock() 串行，避免 SQLite 并发写冲突。
#[allow(clippy::too_many_arguments)] // 导入单文件的固有上下文集合
fn import_one(
    ctx: &JobCtx,
    jieba: &Jieba,
    workspace_id: &str,
    item: &WorkItem,
    chunker_opts: &ChunkerOptions,
    opts: &ImportOptions,
    options_hash: &str,
    doc_role: &str,
) -> AppResult<()> {
    ctx.check()?;

    // 跨工作区缓存：同内容、同解析配置的文件已解析过 → 复制分块与特征，跳过解析。
    // 缓存匹配按 hash+options、与角色无关（角色不影响分块产物）；新行的 doc_role
    // 取本次请求的角色，不继承缓存源——同一文件以另一角色复用缓存时角色必须是新的
    {
        let conn = ctx.db.get()?;
        if let Some(src) = document_repo::find_parsed_by_hash(&conn, &item.file_hash, options_hash)? {
            drop(conn);
            let _w = ctx.write_lock();
            let conn = ctx.db.get()?;
            let doc = document_repo::create_parsing(
                &conn,
                workspace_id,
                &item.file_name,
                &item.path,
                &item.file_hash,
                &item.file_type,
                options_hash,
                doc_role,
            )?;
            drop(conn);
            let mut conn = ctx.db.get()?;
            if let Err(e) = persist_cached(&mut conn, &src, &doc.id) {
                // 复制失败的半成品不保留，避免 status='parsing' 孤儿行
                let _ = document_repo::remove(&conn, &doc.id);
                return Err(e);
            }
            return Ok(());
        }
    }

    let doc = {
        let _w = ctx.write_lock();
        let conn = ctx.db.get()?;
        document_repo::create_parsing(
            &conn,
            workspace_id,
            &item.file_name,
            &item.path,
            &item.file_hash,
            &item.file_type,
            options_hash,
            doc_role,
        )?
    };

    // 解析 + OCR（最重，锁外并行）
    let parsed = parse::parse_file_blocks_opt(
        Path::new(&item.path),
        ctx.cancel_flag(),
        opts.ocr_docx_images,
        crate::engine::ocr::resolve(&opts.ocr_model),
        opts.pdf_cross_check,
    );
    if ctx.cancelled() {
        // 解析被打断的半成品不保留（该行还没有任何分块）
        let _w = ctx.write_lock();
        let conn = ctx.db.get()?;
        let _ = document_repo::remove(&conn, &doc.id);
        return Err(AppError::new(AppErrorCode::JobCancelled, "任务已取消"));
    }

    match parsed {
        Err(e) => {
            let _w = ctx.write_lock();
            let conn = ctx.db.get()?;
            document_repo::mark_failed(&conn, &doc.id, &e)?;
            Ok(())
        }
        Ok(mut pb) => {
            if opts.remove_header_footer {
                parse::strip_header_footer(&mut pb.blocks);
            }
            // PDF/OCR 文本层按视觉行断行，回流成自然段后再分块（页眉页脚清理之后做，
            // 否则页眉/页脚/页码会被拼进正文段落，破坏其识别）
            if matches!(pb.method, "pdfium" | "pdf-extract" | "ocr") {
                for b in pb.blocks.iter_mut().filter(|b| !b.is_table_row) {
                    b.text = parse::reflow_wrapped_lines(&b.text);
                }
            }
            let chunks = chunker::chunk(jieba, &pb.blocks, chunker_opts);
            let char_count = pb.legacy_text.chars().count();
            let fingerprint_json = serde_json::to_string(&pb.fingerprint)
                .unwrap_or_else(|_| "{}".to_string());
            let evasion_json =
                aggregate_evasion(&chunks, pb.pdf_audit.as_ref(), pb.xcheck.as_ref());
            // 写入持锁串行（大文档事务可达数秒，不能与他文档并发写）
            let _w = ctx.write_lock();
            let mut conn = ctx.db.get()?;
            if let Err(e) = persist_parsed(
                &mut conn,
                &doc.id,
                &chunks,
                pb.method,
                pb.pages,
                char_count,
                &fingerprint_json,
                pb.ocr_layout_json.as_deref(),
                pb.truncation_notice.as_deref(),
                evasion_json.as_deref(),
                &pb.image_hashes,
            ) {
                // 入库失败时把文档标失败（可见可重试），不留 'parsing' 孤儿
                let _ = document_repo::mark_failed(&conn, &doc.id, "解析结果入库失败");
                return Err(e);
            }
            Ok(())
        }
    }
}

/// 文档级规避统计聚合（写 documents.evasion_json）：隐形码点各类计数、受影响块数、最大单块
/// 浓度，以及 PDF 隐藏文字层审计（pdfAudit 子对象）。只聚合段落级分块：三档粒度相互包含
/// （sentence ⊂ paragraph ⊂ section），跨档求和会把同一处扰动计三次；段落级覆盖全部正文、
/// 表格行、标题，且是前端下钻的定位单位（低于 min_chars 的碎段只进 section 累计文本，其扰动
/// 不计入文档级——这些文本同样几乎不参与比对，可接受）。
///
/// pdf_audit 是解析期正交产物（与分块无关）：仅在有注入嫌疑（hidden_chars>0）时并入
/// pdfAudit 子对象——干净 PDF（含 OCR 双层页）不写，不做「检查通过/清白」背书（§1.5）。
/// xcheck（W2-4 渲染-OCR 交叉验证）同理：仅在命中（有 verdict）时并入 xcheck 子对象——
/// 未命中/跳过不写（跳过不代表清白）。无任何发现时返回 None（列保持 NULL，与老工作区一致）。
fn aggregate_evasion(
    chunks: &[NewChunk],
    pdf_audit: Option<&crate::engine::pdf_audit::PdfHiddenStats>,
    xcheck: Option<&crate::engine::pdf_xcheck::XCheckResult>,
) -> Option<String> {
    /// 文档级 evasion_json 结构：InvisibleStats 字段展平 + 分布口径 + PDF 隐藏层审计 + 交叉验证。
    /// 只做总数不做浓度分布证明力弱（执行方案风险条目），故必须落块级分布。
    #[derive(serde::Serialize)]
    #[serde(rename_all = "camelCase")]
    struct DocEvasion<'a> {
        #[serde(flatten)]
        stats: crate::engine::normalize::InvisibleStats,
        /// 有任一发现的段落级分块数。
        affected_chunks: u32,
        /// 最大单块浓度：改写类命中数（剥离+折叠）/ 块字符数——扰动聚集度证据。
        max_chunk_concentration: f64,
        /// PDF 隐藏文字层审计（仅有注入嫌疑时并入；干净 PDF/OCR 双层页/非 PDF 缺省）。
        #[serde(skip_serializing_if = "Option::is_none")]
        pdf_audit: Option<&'a crate::engine::pdf_audit::PdfHiddenStats>,
        /// 渲染-OCR 交叉验证（仅命中时并入；未命中/跳过缺省——不做清白背书）。
        #[serde(skip_serializing_if = "Option::is_none")]
        xcheck: Option<&'a crate::engine::pdf_xcheck::XCheckResult>,
    }

    // 文档级采样词多于块级上限，够呈现层列举即可
    const DOC_SAMPLE_MAX: usize = 10;
    let mut agg = crate::engine::normalize::InvisibleStats::default();
    let mut affected = 0u32;
    let mut max_concentration = 0f64;
    for c in chunks.iter().filter(|c| c.chunk_level == "paragraph") {
        let Some(e) = &c.evasion else { continue };
        agg.zero_width += e.zero_width;
        agg.bidi += e.bidi;
        agg.tags += e.tags;
        agg.variation += e.variation;
        agg.confusable_folds += e.confusable_folds;
        agg.mixed_script_words += e.mixed_script_words;
        for s in &e.mixed_script_samples {
            if agg.mixed_script_samples.len() < DOC_SAMPLE_MAX
                && !agg.mixed_script_samples.contains(s)
            {
                agg.mixed_script_samples.push(s.clone());
            }
        }
        affected += 1;
        let conc =
            f64::from(e.perturbation_total()) / c.text.chars().count().max(1) as f64;
        max_concentration = max_concentration.max(conc);
    }
    // PDF 隐藏层：仅有注入嫌疑时纳入（OCR 双层页/干净 PDF 的 has_suspect()=false 不写）
    let pdf_hit = pdf_audit.filter(|a| a.has_suspect());
    // 交叉验证：仅命中时纳入（跳过/未命中的 is_hit()=false 不写，不做清白背书）
    let xcheck_hit = xcheck.filter(|x| x.is_hit());
    if affected == 0 && pdf_hit.is_none() && xcheck_hit.is_none() {
        return None;
    }
    serde_json::to_string(&DocEvasion {
        stats: agg,
        affected_chunks: affected,
        max_chunk_concentration: max_concentration,
        pdf_audit: pdf_hit,
        xcheck: xcheck_hit,
    })
    .ok()
}

/// 「分块写入 + 文档置 parsed」单事务：要么全有要么全无。
#[allow(clippy::too_many_arguments)] // 解析产物的固有字段集，拆结构体无收益
fn persist_parsed(
    conn: &mut crate::db::DbConn,
    doc_id: &str,
    chunks: &[NewChunk],
    method: &str,
    pages: u32,
    char_count: usize,
    fingerprint_json: &str,
    ocr_layout_json: Option<&str>,
    truncation_notice: Option<&str>,
    evasion_json: Option<&str>,
    image_hashes: &[parse::ImageHash],
) -> AppResult<()> {
    let tx = conn.transaction()?;
    chunk_repo::insert_all(&tx, doc_id, chunks)?;
    image_repo::insert_images(&tx, doc_id, image_hashes)?;
    document_repo::mark_parsed(
        &tx,
        doc_id,
        method,
        pages,
        char_count,
        fingerprint_json,
        ocr_layout_json,
        truncation_notice,
        evasion_json,
    )?;
    tx.commit()?;
    Ok(())
}

/// 「分块复制 + 文档置 parsed(cache)」单事务。
fn persist_cached(
    conn: &mut crate::db::DbConn,
    src: &crate::db::repo::document_repo::DocumentRow,
    doc_id: &str,
) -> AppResult<()> {
    let tx = conn.transaction()?;
    chunk_repo::copy_all(&tx, &src.id, doc_id)?;
    // 图片同源指纹随缓存一并复制（复用路径若丢行，同一文件「重新导入也拿不到图片信号」，
    // 与 evasion 同为执行方案工程审查 HIGH 的缓存吞指纹问题）
    image_repo::copy_images(&tx, &src.id, doc_id)?;
    // OCR 版面随缓存一并复制（扫描件复用解析时文本层不丢）
    let src_layout = document_repo::get_ocr_layout(&tx, &src.id)?;
    document_repo::mark_parsed(
        &tx,
        doc_id,
        "cache",
        src.page_count.unwrap_or(0) as u32,
        src.char_count.unwrap_or(0) as usize,
        src.fingerprint_json.as_deref().unwrap_or("{}"),
        src_layout.as_deref(),
        src.truncation_notice.as_deref(),
        // 规避统计随缓存一并复制：复用路径若丢字段，同一文件「重新导入也拿不到
        // 统计」（执行方案工程审查 HIGH 的缓存吞指纹问题，evasion 同理）
        src.evasion_json.as_deref(),
    )?;
    tx.commit()?;
    Ok(())
}

/// 流式哈希整个文件（标书可达数百 MB，不整读进内存），按块响应取消。
fn hash_file(path: &Path, ctx: &JobCtx) -> AppResult<String> {
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut blocks = 0usize;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
        blocks += 1;
        // 每 ~16MB 检查一次取消，超大文件也能秒级响应
        if blocks.is_multiple_of(256) {
            ctx.check()?;
        }
    }
    Ok(hex(&h.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::db::repo::job_repo;
    use crate::db::DbPool;
    use crate::jobs::progress::CollectSink;
    use std::sync::atomic::AtomicBool;

    fn setup() -> (DbPool, String, std::path::PathBuf) {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "测试").unwrap()
        };
        let dir = std::env::temp_dir().join(format!("bidguard_import_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        (pool, ws.id, dir)
    }

    fn ctx_for(pool: &DbPool, ws: &str, cancelled: bool) -> (JobCtx, Arc<CollectSink>) {
        let conn = pool.get().unwrap();
        let job = job_repo::create(&conn, ws, "import", None, "{}").unwrap();
        drop(conn);
        let sink = Arc::new(CollectSink::default());
        let ctx = crate::jobs::JobCtx::for_test(
            job.id,
            "import".into(),
            pool.clone(),
            Arc::new(AtomicBool::new(cancelled)),
            sink.clone(),
        );
        (ctx, sink)
    }

    #[test]
    fn concurrent_import_of_large_docs_on_file_pool() {
        // 回归：文件库（8 连接）下并行导入多份大文档，写入必须串行不撞 SQLITE_BUSY。
        // 内存库（max_size=1）池内自动串行复现不了，必须用文件库。
        let dir = std::env::temp_dir().join(format!("bg_concimp_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let pool = crate::db::open(&dir).unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "并发").unwrap().id
        };
        let jieba = Arc::new(Jieba::new());
        // 每份 ~1200 段（大事务，足以让写锁占用可观时间），4 份内容各异避免去重，并行导入
        let paras: Vec<Vec<String>> = (0..4)
            .map(|i| {
                (0..1200)
                    .map(|n| format!("文档{i}第{n}段：本项目采用分层解耦的微服务总体架构，支持横向扩展与读写分离。"))
                    .collect()
            })
            .collect();
        let paths: Vec<String> = (0..4)
            .map(|i| {
                let refs: Vec<&str> = paras[i].iter().map(String::as_str).collect();
                write_min_docx(&dir, &format!("doc{i}.docx"), &refs)
            })
            .collect();

        let conn = pool.get().unwrap();
        let job = job_repo::create(&conn, &ws, "import", None, "{}").unwrap();
        drop(conn);
        let ctx = crate::jobs::JobCtx::for_test(
            job.id,
            "import".into(),
            pool.clone(),
            Arc::new(AtomicBool::new(false)),
            Arc::new(CollectSink::default()),
        );
        run_import(&ctx, jieba, &ws, &paths, &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs.len(), 4);
        assert!(docs.iter().all(|d| d.status == "parsed"), "并发导入应全部成功：{docs:?}");
        assert!(docs.iter().all(|d| d.chunk_count > 0));
        drop(conn);
        drop(pool);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stale_parsing_docs_marked_failed_on_restart() {
        let (pool, ws, _dir) = setup();
        let conn = pool.get().unwrap();
        // 手造一个卡在 parsing 的孤儿文档（模拟上次被杀）
        document_repo::create_parsing(&conn, &ws, "orphan.docx", "/x", "h", "docx", "oh", "bid")
            .unwrap();
        assert_eq!(document_repo::mark_stale_parsing_as_failed(&conn).unwrap(), 1);
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs[0].status, "failed");
        assert!(docs[0].parse_error.as_deref().unwrap().contains("中断"));
    }

    fn write(dir: &Path, name: &str, content: &str) -> String {
        let p = dir.join(name);
        std::fs::write(&p, content).unwrap();
        p.to_string_lossy().into_owned()
    }

    /// 程序化构造合法 docx（zip + word/document.xml），body 为 w:body 内的原始 XML。
    pub(crate) fn write_docx_body(dir: &Path, name: &str, body: &str) -> String {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let p = dir.join(name);
        let f = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = SimpleFileOptions::default();
        zw.start_file("[Content_Types].xml", opts).unwrap();
        zw.write_all(br#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#).unwrap();
        zw.start_file("word/document.xml", opts).unwrap();
        let xml = format!(
            r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body}</w:body></w:document>"#
        );
        zw.write_all(xml.as_bytes()).unwrap();
        zw.finish().unwrap();
        p.to_string_lossy().into_owned()
    }

    /// 最小 docx：每个字符串一个普通段落。
    pub(crate) fn write_min_docx(dir: &Path, name: &str, paragraphs: &[&str]) -> String {
        let body: String = paragraphs
            .iter()
            .map(|t| format!("<w:p><w:r><w:t>{t}</w:t></w:r></w:p>"))
            .collect();
        write_docx_body(dir, name, &body)
    }

    #[test]
    fn imports_parses_and_dedups() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let a = write(&dir, "a.txt", "本项目采用分层解耦的微服务总体架构设计。\n平台具备横向扩展能力，支持读写分离与多级缓存机制。");
        let b = write(&dir, "b.txt", "我公司具备信息系统集成一级资质，注册资本一亿元，近三年无重大违法记录，业绩覆盖全国。");
        // c 与 a 内容相同（不同文件名）→ 批内去重
        let c = write(&dir, "c.txt", "本项目采用分层解耦的微服务总体架构设计。\n平台具备横向扩展能力，支持读写分离与多级缓存机制。");

        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba.clone(), &ws, &[a.clone(), b, c], &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs.len(), 2, "重复内容只入库一次");
        assert!(docs.iter().all(|d| d.status == "parsed"));
        assert!(docs.iter().all(|d| d.chunk_count > 0), "应有分块");
        assert!(docs.iter().any(|d| d.parse_method.as_deref() == Some("text")));

        // 再次导入同一文件 → 工作区内去重，不新增
        drop(conn);
        let (ctx2, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx2, jieba.clone(), &ws, &[a], &Default::default(), "bid").unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(document_repo::list(&conn, &ws).unwrap().len(), 2);
    }

    #[test]
    fn tender_import_lists_by_role_with_chunks() {
        // 验收 (1)：docRole='tender' 导入后 list_by_role 查回且 chunk_count>0；
        // bid 角色查询不包含招标文件（参评可选集与对减语料互不渗透）
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "tender.txt", "招标文件：投标人须具备电子与智能化工程专业承包一级资质并提供近三年同类业绩证明。");
        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba, &ws, &[f], &Default::default(), "tender").unwrap();

        let conn = pool.get().unwrap();
        let tenders =
            document_repo::list_by_role(&conn, &ws, &["tender", "tender_supplement"]).unwrap();
        assert_eq!(tenders.len(), 1);
        assert_eq!(tenders[0].doc_role, "tender");
        assert_eq!(tenders[0].status, "parsed");
        assert!(tenders[0].chunk_count > 0, "招标文件同样要有分块（对减指纹库的语料）");
        assert!(document_repo::list_by_role(&conn, &ws, &["bid"]).unwrap().is_empty());
        assert!(document_repo::list_by_role(&conn, &ws, &[]).unwrap().is_empty());
    }

    #[test]
    fn same_file_imports_as_bid_and_tender_separately() {
        // 验收 (2)：同一文件先后以 bid/tender 导入产生两行（去重收窄为同角色）；
        // 同角色重复导入仍去重；第二个角色走跨工作区缓存复用路径（hash+options 匹配与角色无关），
        // 但新行的角色必须是本次请求的角色
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "dual.txt", "本项目采用分层解耦的微服务总体架构设计，支持横向扩展与读写分离机制。");

        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba.clone(), &ws, std::slice::from_ref(&f), &Default::default(), "bid").unwrap();
        let (ctx2, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx2, jieba.clone(), &ws, std::slice::from_ref(&f), &Default::default(), "tender").unwrap();

        {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            assert_eq!(docs.len(), 2, "同一文件双角色应各存一份");
            let roles: Vec<&str> = docs.iter().map(|d| d.doc_role.as_str()).collect();
            assert!(roles.contains(&"bid") && roles.contains(&"tender"));
            assert_eq!(docs[0].file_hash, docs[1].file_hash);
            let tender = docs.iter().find(|d| d.doc_role == "tender").unwrap();
            assert_eq!(tender.parse_method.as_deref(), Some("cache"), "同 hash 同配置应复用分块缓存");
            assert!(tender.chunk_count > 0);
        }

        // 同角色重复导入 → 去重跳过，不新增
        let (ctx3, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx3, jieba, &ws, &[f], &Default::default(), "tender").unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(document_repo::list(&conn, &ws).unwrap().len(), 2);
    }

    #[test]
    fn options_hash_covers_pdf_cross_check() {
        // v6 起 pdf_cross_check 计入配置指纹：改开关后跨工作区缓存不得误复用
        let a = ImportOptions::default();
        let b = ImportOptions { pdf_cross_check: false, ..ImportOptions::default() };
        assert!(a.pdf_cross_check, "默认开启");
        assert_ne!(a.options_hash("t"), b.options_hash("t"));
    }

    #[test]
    fn options_hash_covers_fingerprint_schema_version() {
        // v6 预置取证指纹版本键：M1 扩展 Fingerprint（rsid/PDF 血缘等）时只把 fpv 1→2，
        // 旧缓存随之失效，不再动版本前缀——否则 persist_cached 会按同 hash 命中旧行、
        // 复制缺新字段的旧 fingerprint_json（执行方案全局裁决 3「只 bump 一次」）
        let o = ImportOptions::default();
        let pav = crate::engine::pdf_audit::PDF_AUDIT_SCHEMA_VERSION;
        assert_ne!(
            o.options_hash_with_versions("t", 1, pav),
            o.options_hash_with_versions("t", 2, pav),
            "fpv 变则 hash 变"
        );
        assert_eq!(
            o.options_hash("t"),
            o.options_hash_with_versions("t", crate::engine::report::FINGERPRINT_SCHEMA_VERSION, pav),
            "生产路径经当前 schema 版本"
        );
    }

    #[test]
    fn options_hash_covers_pdf_audit_version() {
        // pav 预置解析审计版本键：pdfAudit 是解析期新产出、cache-hit 旧文档不会有它，
        // bump pav 让 options_hash 变化、旧缓存整体失效重建（做法对齐 fpv，不动 v6 前缀）
        let o = ImportOptions::default();
        let fpv = crate::engine::report::FINGERPRINT_SCHEMA_VERSION;
        assert_ne!(
            o.options_hash_with_versions("t", fpv, 1),
            o.options_hash_with_versions("t", fpv, 2),
            "pav 变则 hash 变"
        );
        assert_eq!(
            o.options_hash("t"),
            o.options_hash_with_versions(
                "t",
                fpv,
                crate::engine::pdf_audit::PDF_AUDIT_SCHEMA_VERSION,
            ),
            "生产路径经当前审计 schema 版本"
        );
    }

    #[test]
    fn template_matching_survives_evasion_in_template_text() {
        // 模板分词与分块分词同一 sanitize 口径（W2-1）：模板正文被贴入零宽/同形字时，
        // 导入干净的雷同段落仍须命中 is_template——两边口径不一致会让样板剔除失效
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let clean_tpl = "我方承诺提供7×24小时技术支持服务，质保期内免费维护，确保系统稳定运行";
        // 词内零宽 + 同形字（西里尔 а 转义写死，字面量混拉丁会让测试失真）
        let dirty_tpl = "我方承诺提供7×24小时技术支\u{200B}持服务，质保\u{200B}期内免费维护，\
                         确保系统稳定运行";
        {
            let conn = pool.get().unwrap();
            template_repo::save(&conn, None, "服务承诺", dirty_tpl, None).unwrap();
        }
        let f = write(&dir, "bid.txt", &format!("{clean_tpl}。\n本项目采用独有的边缘计算架构与自研调度算法平台。"));
        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba, &ws, &[f], &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs[0].status, "parsed");
        let rows = chunk_repo::load_for_compare(&conn, &docs[0].id, "paragraph").unwrap();
        let tpl_chunk = rows.iter().find(|c| c.text.contains("7×24")).expect("承诺段应有分块");
        assert!(tpl_chunk.is_template, "扰动模板经 sanitize 后仍应命中干净雷同段");
        let normal = rows.iter().find(|c| c.text.contains("边缘计算")).unwrap();
        assert!(!normal.is_template, "非样板段不受影响");
    }

    #[test]
    fn evasion_stats_aggregate_to_document_and_survive_cache_copy() {
        // 集成验收：导入含零宽/同形字扰动的 docx → documents.evasion_json 非空且计数正确；
        // chunk_features.extra_json 可定位受扰动块；跨工作区缓存复用路径必须一并复制
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        // 第一段：2 个零宽 + 词内混排 Дeposit（Д 无拉丁骨架，只计红旗）+ 同形字
        // Pагe（西里尔 аге 用转义写死，字面量混入拉丁字符会让折叠计数失真）
        let f = write_min_docx(&dir, "evasive.docx", &[
            "本项目采用分层\u{200B}解耦的微服务总体架构\u{200B}设计方案\
             （Дeposit 条款，P\u{0430}\u{0433}\u{0435} 编号）。",
            "平台具备横向扩展能力，支持读写分离与多级缓存机制。",
        ]);
        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba.clone(), &ws, std::slice::from_ref(&f), &Default::default(), "bid").unwrap();

        let evasion = {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            assert_eq!(docs.len(), 1);
            let ev = docs[0].evasion_json.clone().expect("扰动文档应有 evasion_json");
            let v: serde_json::Value = serde_json::from_str(&ev).unwrap();
            assert_eq!(v["zeroWidth"], 2);
            assert_eq!(v["mixedScriptWords"], 2, "Дeposit 与 Pагe 各计一面红旗");
            assert_eq!(v["confusableFolds"], 3, "Pагe 的 аге 折叠计数");
            assert_eq!(v["affectedChunks"], 1, "两个段落级块只有一个受扰动");
            assert!(v["maxChunkConcentration"].as_f64().unwrap() > 0.0);
            assert!(v["mixedScriptSamples"].to_string().contains("Дeposit"), "采样词可下钻");
            // 块级分布可定位到受扰动块（extra_json 只写有发现的块）
            let hit: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM chunks c JOIN chunk_features f ON f.chunk_id = c.id
                     WHERE c.document_id = ?1 AND c.chunk_level = 'paragraph'
                       AND f.extra_json LIKE '%zeroWidth%'",
                    [&docs[0].id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(hit, 1, "受扰动的段落级块恰好一个");
            let clean: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM chunks c JOIN chunk_features f ON f.chunk_id = c.id
                     WHERE c.document_id = ?1 AND f.extra_json IS NULL",
                    [&docs[0].id],
                    |r| r.get(0),
                )
                .unwrap();
            assert!(clean > 0, "干净块不写 extra_json");
            ev
        };

        // 跨工作区缓存复用：evasion_json 必须随分块一并复制（缓存吞统计=重导入也拿不到）
        let ws2 = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "另一工作区").unwrap().id
        };
        let (ctx2, _) = ctx_for(&pool, &ws2, false);
        run_import(&ctx2, jieba, &ws2, &[f], &Default::default(), "bid").unwrap();
        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws2).unwrap();
        assert_eq!(docs[0].parse_method.as_deref(), Some("cache"));
        assert_eq!(docs[0].evasion_json.as_deref(), Some(evasion.as_str()));
        let copied: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM chunks c JOIN chunk_features f ON f.chunk_id = c.id
                 WHERE c.document_id = ?1 AND f.extra_json LIKE '%zeroWidth%'",
                [&docs[0].id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(copied >= 1, "块级 extra_json 随缓存复制");
    }

    /// 程序化构造含"可见 + Tr=3 隐藏文本"的单页 PDF（可被 pdfium 抽取可见文本，
    /// 同时被 pdf_audit 判为注入嫌疑）。
    fn write_hidden_pdf(dir: &Path, name: &str) -> String {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = b"\
BT /F1 12 Tf 0 0 0 rg 1 0 0 1 72 720 Tm 0 Tr (Visible bid paragraph for evaluation.) Tj ET\n\
BT /F1 12 Tf 1 0 0 1 72 700 Tm 3 Tr (injected hidden duplicate clause) Tj ET\n";
        let content_id =
            doc.add_object(Stream::new(dictionary! {}, content.to_vec()).with_compression(false));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page_id.into()], "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let p = dir.join(name);
        doc.save(&p).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn pdf_hidden_layer_audit_lands_in_evasion_json() {
        // 集成验收（W2-3）：导入含隐藏文字层的 PDF → documents.evasion_json.pdfAudit 就位、
        // hiddenChars>0；伪造损坏 PDF 导入不失败（audit=None 静默降级，不阻塞）。
        // pdfium 不可用时解析可能走 OCR（需模型）——本环境自检不可用则跳过，避免门禁挂死。
        if !parse::pdfium_available() {
            eprintln!("跳过：pdfium 不可绑定，PDF 文本抽取不可用");
            return;
        }
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let hidden = write_hidden_pdf(&dir, "hidden.pdf");
        // 伪造损坏 PDF：审计 None、解析失败 → 文档标失败但 job 不报错
        let broken = write(&dir, "broken.pdf", "这不是 PDF，只是伪装扩展名。");
        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba, &ws, &[hidden, broken], &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        let hit = docs.iter().find(|d| d.file_name == "hidden.pdf").expect("含隐藏层的文档应入库");
        assert_eq!(hit.status, "parsed", "含隐藏层的 PDF 应解析成功");
        let ev = hit.evasion_json.clone().expect("含隐藏层文档应有 evasion_json");
        let v: serde_json::Value = serde_json::from_str(&ev).unwrap();
        let audit = &v["pdfAudit"];
        assert!(!audit.is_null(), "evasion_json.pdfAudit 就位");
        assert!(audit["hiddenChars"].as_u64().unwrap() > 0, "hiddenChars>0");
        assert!(audit["trInvisibleChars"].as_u64().unwrap() > 0, "Tr=3 计数");
        assert!(audit["ocrLayerPages"].as_array().unwrap().is_empty(), "非 OCR 双层页");

        let broken_doc = docs.iter().find(|d| d.file_name == "broken.pdf").expect("损坏文档应留痕");
        assert_eq!(broken_doc.status, "failed", "损坏 PDF 标失败但不阻塞 job");
    }

    #[test]
    fn xcheck_hit_merges_into_evasion_json_skipped_does_not() {
        // W2-4：交叉验证命中 → 即使无隐形码点/无 PDF 隐藏层，也产出 evasion_json 且含 xcheck.verdict；
        // 跳过/未命中不并入（不做清白背书），无其他发现时返回 None。
        use crate::engine::pdf_xcheck::{XCheckResult, XCheckVerdict, KIND_FONT_REMAP};
        let hit = XCheckResult {
            sampled_pages: vec![1, 5, 9],
            verdict: Some(XCheckVerdict {
                kind: KIND_FONT_REMAP.into(),
                label: "疑似字体重映射/图片化正文".into(),
            }),
            median_mismatch: 0.62,
            ..Default::default()
        };
        let json = aggregate_evasion(&[], None, Some(&hit)).expect("命中应产出 evasion_json");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["xcheck"]["verdict"]["kind"], KIND_FONT_REMAP);
        assert_eq!(v["xcheck"]["sampledPages"], serde_json::json!([1, 5, 9]));
        assert!(v["pdfAudit"].is_null(), "无隐藏层不写 pdfAudit");

        let skipped = XCheckResult::skipped("OCR 不可用（缺模型或识别失败）");
        assert!(
            aggregate_evasion(&[], None, Some(&skipped)).is_none(),
            "跳过/未命中不写 evasion_json（不做清白背书）"
        );
    }

    #[test]
    fn clean_document_keeps_null_evasion_json() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "clean.txt", "本项目采用分层解耦的微服务总体架构设计，支持横向扩展与读写分离机制。");
        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba, &ws, &[f], &Default::default(), "bid").unwrap();
        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs[0].status, "parsed");
        assert!(docs[0].evasion_json.is_none(), "无发现保持 NULL（与老工作区行为一致）");
    }

    #[test]
    fn cross_workspace_reuses_parsed_chunks() {
        let (pool, ws1, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "shared.txt", "系统采用事件驱动与消息队列实现各子系统之间的异步协同与削峰填谷处理。");

        let (ctx, _) = ctx_for(&pool, &ws1, false);
        run_import(&ctx, jieba.clone(), &ws1, std::slice::from_ref(&f), &Default::default(), "bid").unwrap();

        let ws2 = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "另一工作区").unwrap().id
        };
        let (ctx2, _) = ctx_for(&pool, &ws2, false);
        run_import(&ctx2, jieba, &ws2, &[f], &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws2).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].parse_method.as_deref(), Some("cache"), "应命中缓存");
        assert!(docs[0].chunk_count > 0, "缓存复用也要有分块");
    }

    #[test]
    fn parse_failure_marks_document_not_job() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        // 伪 docx：zip 打不开 → 解析失败
        let bad = write(&dir, "bad.docx", "这不是一个 zip 文件");
        let good = write(&dir, "good.txt", "本项目严格遵循国家信息安全等级保护三级标准与相关行业规范要求。");

        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba, &ws, &[bad, good], &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs.len(), 2);
        let failed: Vec<_> = docs.iter().filter(|d| d.status == "failed").collect();
        assert_eq!(failed.len(), 1);
        assert!(failed[0].parse_error.is_some());
        assert!(docs.iter().any(|d| d.status == "parsed"));
    }

    #[test]
    fn real_docx_imports_with_structure_end_to_end() {
        // 真实 docx（zip）端到端：标题→章节路径、段落、表格→行块、实体齐备
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let body = concat!(
            r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>第一章 商务部分</w:t></w:r></w:p>"#,
            r#"<w:p><w:r><w:t>投标报价为人民币12800000元整，包含全部软硬件费用与三年质保服务。</w:t></w:r></w:p>"#,
            r#"<w:tbl><w:tr><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>核心交换机及配套光模块</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>64000元</w:t></w:r></w:p></w:tc></w:tr></w:tbl>"#,
        );
        let f = write_docx_body(&dir, "bid.docx", body);
        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba, &ws, &[f], &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].status, "parsed");
        assert_eq!(docs[0].parse_method.as_deref(), Some("docx"));
        assert!(docs[0].chunk_count > 0);

        let rows = chunk_repo::load_for_compare(&conn, &docs[0].id, "paragraph").unwrap();
        let para = rows
            .iter()
            .find(|c| c.text.contains("投标报价"))
            .expect("应有报价段落分块");
        assert!(
            para.section_path.as_deref().unwrap_or("").contains("商务部分"),
            "标题应进章节路径：{:?}",
            para.section_path
        );
        assert!(para.entity_json.as_deref().unwrap_or("").contains("amount"));
        let row = rows
            .iter()
            .find(|c| c.text.contains("核心交换机"))
            .expect("表格应产出行块");
        assert_eq!(row.text, "1 | 核心交换机及配套光模块 | 64000元");
    }

    #[test]
    fn data_persists_across_db_reopen() {
        // 设计文档 §20.2 场景 6：关闭再打开（文件库重开），数据仍可查
        let dir = std::env::temp_dir().join(format!("bidguard_persist_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let jieba = Arc::new(Jieba::new());
        let ws_id;
        {
            let pool = crate::db::open(&dir).unwrap();
            let ws = {
                let conn = pool.get().unwrap();
                workspace_repo::create(&conn, "持久化测试").unwrap()
            };
            ws_id = ws.id.clone();
            let f = write(&dir, "p.txt", "本项目采用分层解耦的微服务总体架构设计，支持横向扩展与读写分离机制。");
            let (ctx, _) = ctx_for(&pool, &ws.id, false);
            run_import(&ctx, jieba, &ws.id, &[f], &Default::default(), "bid").unwrap();
        } // pool 整体 drop = 应用关闭

        let pool2 = crate::db::open(&dir).unwrap();
        let conn = pool2.get().unwrap();
        let docs = document_repo::list(&conn, &ws_id).unwrap();
        assert_eq!(docs.len(), 1, "重开后文档仍在");
        assert_eq!(docs[0].status, "parsed");
        assert!(docs[0].chunk_count > 0, "重开后分块仍在");
        let jobs = job_repo::list(&conn, Some(&ws_id)).unwrap();
        assert!(!jobs.is_empty(), "任务记录仍在");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore] // 需真实 PDF：BIDGUARD_PDF=<path> cargo test pdf_paragraph_reflow_stats -- --ignored --nocapture
    fn pdf_paragraph_reflow_stats() {
        let Ok(pdf) = std::env::var("BIDGUARD_PDF") else {
            eprintln!("跳过：未设 BIDGUARD_PDF");
            return;
        };
        if !std::path::Path::new(&pdf).exists() {
            eprintln!("跳过：{pdf} 不存在");
            return;
        }
        let (pool, ws, _dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba, &ws, std::slice::from_ref(&pdf), &Default::default(), "bid").unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        let d = &docs[0];
        println!("\n解析方式={:?} 页数={:?} 段块数={}", d.parse_method, d.page_count, d.chunk_count);
        let rows = crate::db::repo::chunk_repo::load_for_compare(&conn, &d.id, "paragraph").unwrap();
        let lens: Vec<usize> = rows.iter().map(|r| r.text.chars().count()).collect();
        let n = lens.len().max(1);
        let avg = lens.iter().sum::<usize>() / n;
        let short = lens.iter().filter(|&&l| l < 25).count();
        let mut sorted = lens.clone();
        sorted.sort_unstable();
        let median = sorted.get(n / 2).copied().unwrap_or(0);
        println!("段落级分块：{} 个，平均 {} 字，中位 {} 字，<25字的碎块占比 {:.0}%",
            n, avg, median, short as f32 / n as f32 * 100.0);
        println!("样例前 3 段：");
        for r in rows.iter().take(3) {
            println!("  [{}字] {}", r.text.chars().count(), r.text.chars().take(60).collect::<String>());
        }
        assert!(avg > 40, "回流后段落平均长度应 >40 字（修复前每行一段约 20-30 字），实际 {avg}");
    }

    #[test]
    fn legacy_doc_format_gets_actionable_error() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "投标书.doc", "x");
        let (ctx, _) = ctx_for(&pool, &ws, false);
        let err = run_import(&ctx, jieba, &ws, &[f], &Default::default(), "bid").unwrap_err();
        assert_eq!(err.code, AppErrorCode::UnsupportedFileType);
        assert!(err.message.contains("另存为"), "应给出可执行的出路：{}", err.message);
    }

    #[test]
    fn failed_document_can_be_retried_with_same_file() {
        // 回归：失败行曾把同 hash 文件挡在去重外，导致重试永远被「已存在」跳过
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let bad = write(&dir, "bid.docx", "这不是一个 zip 文件");

        let (ctx, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx, jieba.clone(), &ws, std::slice::from_ref(&bad), &Default::default(), "bid").unwrap();
        {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            assert_eq!(docs.len(), 1);
            assert_eq!(docs[0].status, "failed");
        }

        // 同一文件重试：不应被去重跳过，旧失败行应被清掉（仍失败但是新一次尝试）
        let (ctx2, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx2, jieba.clone(), &ws, std::slice::from_ref(&bad), &Default::default(), "bid").unwrap();
        let first_retry_id = {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            assert_eq!(docs.len(), 1, "重试不应残留多行同 hash 文档");
            assert_eq!(docs[0].status, "failed");
            docs[0].id.clone()
        };

        // 文件修好（换成合法 docx）后再重试 → 解析成功
        let fixed = write_min_docx(&dir, "bid.docx", &[
            "修复后的内容：本项目采用分层解耦的微服务总体架构设计方案。",
        ]);
        let (ctx3, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx3, jieba, &ws, &[fixed], &Default::default(), "bid").unwrap();
        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        // 修复后内容变了 → hash 不同，旧失败行（旧 hash）不再被本次清理；
        // 但旧失败行 + 新成功行并存时，列表应能区分出成功行
        assert!(docs.iter().any(|d| d.status == "parsed" && d.chunk_count > 0), "修复后应解析成功");
        assert!(docs.iter().all(|d| d.id != first_retry_id || d.status == "failed"));
    }

    #[test]
    fn cancelled_import_returns_job_cancelled() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "x.txt", "本工程建设周期为一百八十个日历日，完成全部交付与验收工作。");
        let (ctx, _) = ctx_for(&pool, &ws, true);
        let err = run_import(&ctx, jieba, &ws, &[f], &Default::default(), "bid").unwrap_err();
        assert_eq!(err.code, AppErrorCode::JobCancelled);
        let conn = pool.get().unwrap();
        assert!(document_repo::list(&conn, &ws).unwrap().is_empty(), "取消不应残留文档");
    }

    #[test]
    fn rejects_missing_and_unsupported_files() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());

        let (ctx, _) = ctx_for(&pool, &ws, false);
        let err = run_import(&ctx, jieba.clone(), &ws, &["/不存在/x.txt".into()], &Default::default(), "bid").unwrap_err();
        assert_eq!(err.code, AppErrorCode::FileNotFound);

        let exe = write(&dir, "evil.exe", "MZ");
        let (ctx2, _) = ctx_for(&pool, &ws, false);
        let err = run_import(&ctx2, jieba, &ws, &[exe], &Default::default(), "bid").unwrap_err();
        assert_eq!(err.code, AppErrorCode::UnsupportedFileType);
    }
}
