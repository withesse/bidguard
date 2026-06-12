// 文档导入：校验 → sha256 哈希 → 去重（批内 / 工作区内 / 跨工作区缓存）→ 解析 →
// 结构化分块（三档粒度 + 标题路径 + 模板标记 + 特征）→ 批量事务入库。
// 解析失败只标记该文档 failed，不中断整个任务；取消时未入库的文档不留半成品。
use crate::db::repo::{chunk_repo, document_repo, template_repo, workspace_repo};
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
    pub language: String, // auto | zh | en
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
            language: cfg.compare.language.clone(),
        }
    }

    /// 配置指纹：跨工作区分块缓存复用的匹配键（配置不同 → 分块不可互换）。
    pub fn options_hash(&self) -> String {
        let s = format!(
            "v2|min={}|case={}|punct={}|ws={}|tbl={}|page={}|hf={}|lang={}",
            self.min_paragraph_chars,
            self.normalize.ignore_case,
            self.normalize.ignore_punctuation,
            self.normalize.ignore_whitespace,
            self.detect_table,
            self.preserve_page_number,
            self.remove_header_footer,
            self.language,
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
) -> AppResult<()> {
    if paths.is_empty() {
        return Err(AppError::new(AppErrorCode::InvalidConfig, "未选择任何文件"));
    }
    let options_hash = opts.options_hash();

    // 启用中的查重源模板 → 分词，供分块阶段标记样板段落
    let chunker_opts = {
        let conn = ctx.db.get()?;
        let template_tokens: Vec<Vec<String>> = template_repo::list_enabled_texts(&conn)?
            .iter()
            .map(|t| tokenize(&jieba, t))
            .filter(|t| !t.is_empty())
            .collect();
        ChunkerOptions {
            min_chars: opts.min_paragraph_chars,
            template_tokens,
            normalize: opts.normalize.clone(),
            detect_table: opts.detect_table,
            preserve_page_number: opts.preserve_page_number,
            language: opts.language.clone(),
        }
    };

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
        // 连接即取即还：progress() 自己也要取连接，持有期间调用会饿死小连接池
        let dup_in_ws = {
            let conn = ctx.db.get()?;
            document_repo::find_by_hash(&conn, workspace_id, &file_hash)?.is_some()
        };
        if dup_in_batch || dup_in_ws {
            skipped += 1;
            ctx.progress("hash", i + 1, total, format!("{file_name} 已存在，跳过"));
            continue;
        }
        // 重试路径：同 hash 的失败残留行先清掉，避免重试成功后失败行与新行并存
        {
            let conn = ctx.db.get()?;
            document_repo::remove_failed_by_hash(&conn, workspace_id, &file_hash)?;
        }
        work.push(WorkItem {
            path: p.clone(),
            file_name,
            file_type,
            file_hash,
        });
    }

    // 阶段 B：按文件并行解析入库
    let parse_total = work.len();
    let done = AtomicUsize::new(0);
    let results: Vec<AppResult<()>> = work
        .par_iter()
        .map(|item| {
            let r = import_one(ctx, &jieba, workspace_id, item, &chunker_opts, opts, &options_hash);
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
fn import_one(
    ctx: &JobCtx,
    jieba: &Jieba,
    workspace_id: &str,
    item: &WorkItem,
    chunker_opts: &ChunkerOptions,
    opts: &ImportOptions,
    options_hash: &str,
) -> AppResult<()> {
    ctx.check()?;

    // 跨工作区缓存：同内容、同解析配置的文件已解析过 → 复制分块与特征，跳过解析
    {
        let conn = ctx.db.get()?;
        if let Some(src) = document_repo::find_parsed_by_hash(&conn, &item.file_hash, options_hash)? {
            let doc = document_repo::create_parsing(
                &conn,
                workspace_id,
                &item.file_name,
                &item.path,
                &item.file_hash,
                &item.file_type,
                options_hash,
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
        let conn = ctx.db.get()?;
        document_repo::create_parsing(
            &conn,
            workspace_id,
            &item.file_name,
            &item.path,
            &item.file_hash,
            &item.file_type,
            options_hash,
        )?
    };

    let parsed = parse::parse_file_blocks(Path::new(&item.path), ctx.cancel_flag());
    if ctx.cancelled() {
        // 解析被打断的半成品不保留（该行还没有任何分块）
        let conn = ctx.db.get()?;
        let _ = document_repo::remove(&conn, &doc.id);
        return Err(AppError::new(AppErrorCode::JobCancelled, "任务已取消"));
    }

    match parsed {
        Err(e) => {
            let conn = ctx.db.get()?;
            document_repo::mark_failed(&conn, &doc.id, &e)?;
            Ok(())
        }
        Ok(mut pb) => {
            if opts.remove_header_footer {
                parse::strip_header_footer(&mut pb.blocks);
            }
            let chunks = chunker::chunk(jieba, &pb.blocks, chunker_opts);
            let char_count = pb.legacy_text.chars().count();
            let fingerprint_json = serde_json::to_string(&pb.fingerprint)
                .unwrap_or_else(|_| "{}".to_string());
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
            ) {
                // 入库失败时把文档标失败（可见可重试），不留 'parsing' 孤儿
                let _ = document_repo::mark_failed(&conn, &doc.id, "解析结果入库失败");
                return Err(e);
            }
            Ok(())
        }
    }
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
) -> AppResult<()> {
    let tx = conn.transaction()?;
    chunk_repo::insert_all(&tx, doc_id, chunks)?;
    document_repo::mark_parsed(
        &tx,
        doc_id,
        method,
        pages,
        char_count,
        fingerprint_json,
        ocr_layout_json,
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
        if blocks % 256 == 0 {
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
        run_import(&ctx, jieba.clone(), &ws, &[a.clone(), b, c], &Default::default()).unwrap();

        let conn = pool.get().unwrap();
        let docs = document_repo::list(&conn, &ws).unwrap();
        assert_eq!(docs.len(), 2, "重复内容只入库一次");
        assert!(docs.iter().all(|d| d.status == "parsed"));
        assert!(docs.iter().all(|d| d.chunk_count > 0), "应有分块");
        assert!(docs.iter().any(|d| d.parse_method.as_deref() == Some("text")));

        // 再次导入同一文件 → 工作区内去重，不新增
        drop(conn);
        let (ctx2, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx2, jieba.clone(), &ws, &[a], &Default::default()).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(document_repo::list(&conn, &ws).unwrap().len(), 2);
    }

    #[test]
    fn cross_workspace_reuses_parsed_chunks() {
        let (pool, ws1, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "shared.txt", "系统采用事件驱动与消息队列实现各子系统之间的异步协同与削峰填谷处理。");

        let (ctx, _) = ctx_for(&pool, &ws1, false);
        run_import(&ctx, jieba.clone(), &ws1, &[f.clone()], &Default::default()).unwrap();

        let ws2 = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "另一工作区").unwrap().id
        };
        let (ctx2, _) = ctx_for(&pool, &ws2, false);
        run_import(&ctx2, jieba, &ws2, &[f], &Default::default()).unwrap();

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
        run_import(&ctx, jieba, &ws, &[bad, good], &Default::default()).unwrap();

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
        run_import(&ctx, jieba, &ws, &[f], &Default::default()).unwrap();

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
            run_import(&ctx, jieba, &ws.id, &[f], &Default::default()).unwrap();
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
    fn legacy_doc_format_gets_actionable_error() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());
        let f = write(&dir, "投标书.doc", "x");
        let (ctx, _) = ctx_for(&pool, &ws, false);
        let err = run_import(&ctx, jieba, &ws, &[f], &Default::default()).unwrap_err();
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
        run_import(&ctx, jieba.clone(), &ws, &[bad.clone()], &Default::default()).unwrap();
        {
            let conn = pool.get().unwrap();
            let docs = document_repo::list(&conn, &ws).unwrap();
            assert_eq!(docs.len(), 1);
            assert_eq!(docs[0].status, "failed");
        }

        // 同一文件重试：不应被去重跳过，旧失败行应被清掉（仍失败但是新一次尝试）
        let (ctx2, _) = ctx_for(&pool, &ws, false);
        run_import(&ctx2, jieba.clone(), &ws, &[bad.clone()], &Default::default()).unwrap();
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
        run_import(&ctx3, jieba, &ws, &[fixed], &Default::default()).unwrap();
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
        let err = run_import(&ctx, jieba, &ws, &[f], &Default::default()).unwrap_err();
        assert_eq!(err.code, AppErrorCode::JobCancelled);
        let conn = pool.get().unwrap();
        assert!(document_repo::list(&conn, &ws).unwrap().is_empty(), "取消不应残留文档");
    }

    #[test]
    fn rejects_missing_and_unsupported_files() {
        let (pool, ws, dir) = setup();
        let jieba = Arc::new(Jieba::new());

        let (ctx, _) = ctx_for(&pool, &ws, false);
        let err = run_import(&ctx, jieba.clone(), &ws, &["/不存在/x.txt".into()], &Default::default()).unwrap_err();
        assert_eq!(err.code, AppErrorCode::FileNotFound);

        let exe = write(&dir, "evil.exe", "MZ");
        let (ctx2, _) = ctx_for(&pool, &ws, false);
        let err = run_import(&ctx2, jieba, &ws, &[exe], &Default::default()).unwrap_err();
        assert_eq!(err.code, AppErrorCode::UnsupportedFileType);
    }
}
