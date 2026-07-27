// 文档解析：抽取段块 + 元数据指纹。parse_file_blocks 产出结构化段块（标题层级 + 页码 +
// 协作式取消）供导入管线分块；legacy_text 是与段块解耦的全文（用于字数统计与早期校验）。
// docx(zip+XML) / txt·md(UTF-8 或 GBK) / PDF(pdfium → pdf-extract → OCR 三级回落)。
use crate::engine::report::Fingerprint;
use pdfium_render::prelude::*;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// 一个解析段块。docx 按段落产出（带标题层级），PDF/OCR 按页产出（带页码）。
/// docx 表格按行产出（单元格以「 | 」连接，is_table_row=true）；
/// docx 编号/项目符号段落（w:numPr）标记 is_list_item。
/// 注意：docx 自动编号的序号文本不在文档流中（由 numbering.xml 渲染期生成），
/// 无法还原「第 1 条」的数字本身——仅结构标记，不伪造编号文本。
pub struct Block {
    pub text: String,
    pub heading_level: Option<u8>,
    pub page: Option<u32>,
    pub is_table_row: bool,
    pub is_list_item: bool,
}

/// 一张内嵌位图的同源指纹（导入期提取，落 document_images）。source="docx" 为
/// word/media/ 位图（page=None）；source="pdf" 为 pdfium 页对象位图（page=页码，1 起）。
/// sha256 = sha256(宽 LE + 高 LE + RGB8 像素字节)，跨容器格式稳定；dhash 为 64 位
/// 感知哈希，None 表示整页扫描图（图面积/页面积>0.8）——只做 exact 不做 near，
/// 防「都是空白页/同制式表格」互撞误报。
pub struct ImageHash {
    pub source: &'static str, // docx | pdf
    pub page: Option<u32>,
    pub width: u32,
    pub height: u32,
    pub sha256: String,
    pub dhash: Option<u64>,
}

pub struct ParsedBlocks {
    pub blocks: Vec<Block>,
    pub pages: u32,
    pub fingerprint: Fingerprint,
    pub method: &'static str, // docx | text | pdfium | pdf-extract | ocr
    /// 全文（含空段落/空页换行，不做过滤）：blocks 为分块做了裁剪，两种表示解耦。
    /// 字数统计与解析早期校验用此字段。
    pub legacy_text: String,
    /// 扫描件 OCR 行级版面（每页一组归一化坐标行），JSON 序列化后随文档入库，
    /// 供原文版式预览在页图上叠加隐形可选中文本层；非 OCR 路径为 None。
    pub ocr_layout_json: Option<String>,
    /// 截断提示：扫描件超 OCR 上限时「仅比对前 N 页」的告知语。随文档入库、前端以
    /// 警示条展示，但【不进 blocks/分块】——若作为正文参与比对，多份总页数相同的
    /// 截断扫描件其提示文本逐字节相同，会被聚成假 same 雷同条款并触发假围标信号。
    pub truncation_notice: Option<String>,
    /// 内嵌图片同源指纹（导入期提取，落 document_images）：docx word/media 位图 +
    /// PDF 页对象位图。非 docx/pdf 或提取器不可用（pdfium 缺）时为空。
    pub image_hashes: Vec<ImageHash>,
    /// PDF 隐藏文字层审计（W2-3，与抽取方式正交，parse_pdf 分发后统一填充）：Tr=3/白字/
    /// 出画布/极小字号计数 + OCR 双层页归类。非 PDF 路径或损坏 PDF 为 None。
    pub pdf_audit: Option<crate::engine::pdf_audit::PdfHiddenStats>,
    /// 渲染-OCR 抽样交叉验证结果（W2-4，仅 pdf_cross_check 开启且前两级有文字层时填充）：
    /// 抽样页 + 逐页失配/顺序分 + verdict/skipped。命中即已回落 OCR（method=ocr-fallback）。
    /// 未开启/扫描件/非 PDF 为 None。命中时并入 documents.evasion_json 的 xcheck 子对象。
    pub xcheck: Option<crate::engine::pdf_xcheck::XCheckResult>,
}

/// 新 API：结构化段块 + 取消旗标（OCR/栅格化等长阶段逐页检查）。
/// 取消时尽快返回 Err；调用方应先自查旗标再决定如何归类该错误。
/// 导入前的轻量预览（parse_meta）与测试走此薄包装：pdf_cross_check=false，不做渲染-OCR
/// 交叉验证（那是导入期的重活，会改变 method/文本）。
pub fn parse_file_blocks(path: &Path, cancel: &AtomicBool) -> Result<ParsedBlocks, String> {
    parse_file_blocks_opt(
        path,
        cancel,
        false,
        crate::engine::ocr::resolve(crate::engine::ocr::DEFAULT_OCR_MODEL),
        false,
    )
}

/// 带选项的解析。ocr_docx_images=true 时对 docx 内嵌图片做 OCR（截图式表格/资质里的文字）。
/// ocr_model 选定扫描件/图片 OCR 的档位（PP-OCRv6 tiny/small/medium）。
/// pdf_cross_check=true 时对文字版 PDF 跑渲染-OCR 抽样交叉验证（W2-4，命中回落 OCR）。
pub fn parse_file_blocks_opt(
    path: &Path,
    cancel: &AtomicBool,
    ocr_docx_images: bool,
    ocr_model: &'static crate::engine::ocr::OcrModelSpec,
    pdf_cross_check: bool,
) -> Result<ParsedBlocks, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "docx" => parse_docx(path, cancel, ocr_docx_images, ocr_model),
        "txt" | "md" => parse_txt(path),
        "pdf" => parse_pdf(path, cancel, ocr_model, pdf_cross_check),
        "xlsx" | "xls" => parse_spreadsheet(path),
        other => Err(format!("暂不支持的文件类型: .{other}")),
    }
}

/// 电子表格（xlsx/xls，calamine）：每个工作表名作一级标题（进章节路径），
/// 每个非空行 → 表格行块（与 docx 表格同一条管线：列对齐 diff + 金额冲突检测）。
/// 「页码」即工作表序号，预览与定位按表跳转。
fn parse_spreadsheet(path: &Path) -> Result<ParsedBlocks, String> {
    use calamine::{open_workbook_auto, Reader};
    let mut wb = open_workbook_auto(path).map_err(|e| format!("无法打开表格文件：{e}"))?;
    let names: Vec<String> = wb.sheet_names().to_vec();
    let mut blocks: Vec<Block> = Vec::new();
    let mut legacy = String::new();
    for (si, name) in names.iter().enumerate() {
        let Ok(range) = wb.worksheet_range(name) else {
            continue; // 图表页等无数据区的表直接跳过
        };
        let page = Some(si as u32 + 1);
        blocks.push(Block {
            text: name.clone(),
            heading_level: Some(1),
            page,
            is_table_row: false,
            is_list_item: false,
        });
        legacy.push_str(name);
        legacy.push('\n');
        for row in range.rows() {
            let mut cells: Vec<String> = row.iter().map(fmt_cell).collect();
            while cells.last().is_some_and(|c| c.is_empty()) {
                cells.pop(); // 尾部空列不参与（中间空列保留以对齐列序）
            }
            if cells.iter().all(|c| c.is_empty()) {
                continue;
            }
            let text = cells.join(" | ");
            legacy.push_str(&text);
            legacy.push('\n');
            blocks.push(Block {
                text,
                heading_level: None,
                page,
                is_table_row: true,
                is_list_item: false,
            });
        }
    }
    if legacy.trim().is_empty() {
        return Err("表格文件没有可读取的数据".into());
    }
    let pages = (names.len() as u32).max(1);
    Ok(ParsedBlocks {
        blocks,
        pages,
        fingerprint: Fingerprint::default(),
        method: "xlsx",
        legacy_text: legacy,
        ocr_layout_json: None,
        truncation_notice: None,
        image_hashes: Vec::new(),
        pdf_audit: None,
        xcheck: None,
    })
}

/// 单元格 → 文本：数字去浮点尾巴（64000.0→64000），日期转 ISO，错误单元格留空。
fn fmt_cell(d: &calamine::Data) -> String {
    use calamine::Data;
    match d {
        Data::Empty | Data::Error(_) => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => format!("{f}"),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => (if *b { "是" } else { "否" }).to_string(),
        Data::DateTime(dt) => match dt.as_datetime() {
            Some(t) if t.time() == chrono::NaiveTime::MIN => t.format("%Y-%m-%d").to_string(),
            Some(t) => t.format("%Y-%m-%d %H:%M").to_string(),
            None => format!("{}", dt.as_f64()),
        },
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
    }
}

const CANCELLED: &str = "已取消";

fn parse_txt(path: &Path) -> Result<ParsedBlocks, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = decode_text(&bytes);
    let pages = ((text.chars().count() / 1500) as u32).max(1);
    Ok(ParsedBlocks {
        blocks: vec![Block {
            text: text.clone(),
            heading_level: None,
            page: None,
            is_table_row: false,
            is_list_item: false,
        }],
        pages,
        fingerprint: Fingerprint::default(),
        method: "text",
        legacy_text: text,
        ocr_layout_json: None,
        truncation_notice: None,
        image_hashes: Vec::new(),
        pdf_audit: None,
        xcheck: None,
    })
}

/// 页眉页脚清理（parser.removeHeaderFooter，设计文档 §8.3 规则 8）：
/// 仅对「逐页产出」的块集生效（pdfium/OCR 每块一页）——docx 不读 header/footer 部件、
/// txt 无页概念，天然无需处理。两类目标：
/// 1) 跨页重复的首行/尾行（出现于 ≥60% 且 ≥3 页）视为页眉/页脚；
/// 2) 页首/页尾两行内的纯页码行（「3」「- 3 -」「第 3 页」「3 / 12」）。
pub fn strip_header_footer(blocks: &mut [Block]) {
    use std::collections::HashMap;
    let paged: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, b)| b.page.is_some() && !b.is_table_row)
        .map(|(i, _)| i)
        .collect();
    if paged.len() < 3 {
        return;
    }

    // 页眉/页脚候选必须短（长行更可能是正文首句）
    let key = |line: &str| -> Option<String> {
        let t = line.trim();
        let n = t.chars().count();
        if (2..=60).contains(&n) {
            Some(t.to_string())
        } else {
            None
        }
    };
    let mut first_freq: HashMap<String, usize> = HashMap::new();
    let mut last_freq: HashMap<String, usize> = HashMap::new();
    for &i in &paged {
        let mut lines = blocks[i].text.lines().filter(|l| !l.trim().is_empty());
        if let Some(k) = lines.next().and_then(key) {
            *first_freq.entry(k).or_insert(0) += 1;
        }
        if let Some(k) = blocks[i]
            .text
            .lines()
            .rfind(|l| !l.trim().is_empty())
            .and_then(key)
        {
            *last_freq.entry(k).or_insert(0) += 1;
        }
    }
    let threshold = (paged.len() * 3).div_ceil(5).max(3); // ≥60% 且 ≥3 页
    let headers: std::collections::HashSet<String> = first_freq
        .into_iter()
        .filter(|(_, c)| *c >= threshold)
        .map(|(k, _)| k)
        .collect();
    let footers: std::collections::HashSet<String> = last_freq
        .into_iter()
        .filter(|(_, c)| *c >= threshold)
        .map(|(k, _)| k)
        .collect();

    for &i in &paged {
        let lines: Vec<&str> = blocks[i].text.lines().collect();
        let n_nonempty = lines.iter().filter(|l| !l.trim().is_empty()).count();
        let mut keep: Vec<&str> = Vec::with_capacity(lines.len());
        let mut seen_nonempty = 0usize;
        for l in &lines {
            let t = l.trim();
            if t.is_empty() {
                keep.push(l);
                continue;
            }
            seen_nonempty += 1;
            let at_head = seen_nonempty == 1;
            let at_tail = seen_nonempty == n_nonempty;
            let near_edge = seen_nonempty <= 2 || seen_nonempty + 1 >= n_nonempty;
            let is_repeat = (at_head && headers.contains(t)) || (at_tail && footers.contains(t));
            if is_repeat || (near_edge && is_page_number_line(t)) {
                continue;
            }
            keep.push(l);
        }
        blocks[i].text = keep.join("\n");
    }
}

/// 软换行回流（PDF/OCR 文本层）：pdfium/pdf-extract/OCR 按「视觉行」断行，每行尾都是 `\n`，
/// 直接分块会把一个自然段拆成每行一段。这里把同一段的多行重新拼回，仅在真正的段落边界保留 `\n`。
/// 段落边界判定：空行，或行尾是句末标点（。！？!?；;…）/右引号/冒号——中文公文里这些强烈指示段末。
/// 拼接时中英混排按需补空格（西文词间补，CJK 相邻不补），并消解西文行尾连字符 `-`。
/// docx/txt/md 的 `\n` 是真实段落边界，不走此回流。
pub fn reflow_wrapped_lines(text: &str) -> String {
    let mut paras: Vec<String> = Vec::new();
    let mut cur = String::new();
    let is_break_end = |c: char| matches!(c, '。' | '！' | '？' | '.' | '!' | '?' | '；' | ';' | '…' | '：' | ':' | '”' | '』' | '」' | '）' | ')');
    for line in text.split('\n') {
        let t = line.trim();
        if t.is_empty() {
            if !cur.is_empty() {
                paras.push(std::mem::take(&mut cur));
            }
            continue;
        }
        if !cur.is_empty() {
            let prev = cur.chars().next_back().unwrap_or(' ');
            let next = t.chars().next().unwrap_or(' ');
            if prev == '-' && cur.chars().rev().nth(1).is_some_and(|c| c.is_ascii_alphabetic()) {
                cur.pop(); // 西文行尾连字符断词 → 去连字符直接拼
            } else if prev.is_ascii_alphanumeric() && next.is_ascii_alphanumeric() {
                cur.push(' '); // 西文词间补空格
            }
        }
        cur.push_str(t);
        if cur.chars().next_back().is_some_and(is_break_end) {
            paras.push(std::mem::take(&mut cur));
        }
    }
    if !cur.is_empty() {
        paras.push(cur);
    }
    paras.join("\n")
}

/// 纯页码行：仅由数字、空白与少量装饰字符（- – — / 第 页 共 .）组成且含数字。
fn is_page_number_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || t.chars().count() > 12 {
        return false;
    }
    let mut has_digit = false;
    for c in t.chars() {
        if c.is_ascii_digit() {
            has_digit = true;
        } else if !matches!(c, '-' | '–' | '—' | '/' | '.' | ' ' | '\t' | '第' | '页' | '共' | '(' | ')') {
            return false;
        }
    }
    has_digit
}

/// 解码文本：UTF-16 BOM → UTF-8（含 BOM）→ GB18030（覆盖 GBK/GB2312）回落。
/// UTF-16 必须先判：带 BOM 的 UTF-16（Windows 记事本「Unicode」存档）若落到 GB18030
/// 会被硬解成乱码静默入库（GB18030 几乎不解码失败），永远比对不上 → 无声漏报。
pub fn decode_text(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    let body = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes
    };
    if let Ok(s) = std::str::from_utf8(body) {
        return s.to_string();
    }
    let (cow, _, _) = encoding_rs::GB18030.decode(body);
    cow.into_owned()
}

fn parse_pdf(
    path: &Path,
    cancel: &AtomicBool,
    ocr_model: &'static crate::engine::ocr::OcrModelSpec,
    pdf_cross_check: bool,
) -> Result<ParsedBlocks, String> {
    // 隐藏文字层审计：与抽取方式正交，三级回落前先跑（损坏/加密 PDF 返回 None 静默降级，
    // 与 pdf_fingerprint 同容错语义，不阻塞导入）。
    let audit = crate::engine::pdf_audit::audit(path);
    // 1) pdfium 文本（最鲁棒）；2) pdf-extract 回落；3) 扫描件 → OCR（上限 OCR_MAX_PAGES）
    let mut pb = if let Some(pd) = parse_pdf_pdfium(path, cancel) {
        pd
    } else if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.into());
    } else if let Ok(pd) = parse_pdf_extract(path) {
        pd
    } else {
        parse_pdf_ocr(path, cancel, ocr_model, Some(OCR_MAX_PAGES))?
    };
    pb.pdf_audit = audit;
    // 渲染-OCR 抽样交叉验证（W2-4）：仅在前两级抽出文字层（pdfium/pdf-extract）后触发——
    // 扫描件本就是 OCR 文本，无「渲染 vs 抽取」差集可验。命中即回落整文档 OCR；
    // 关闭时记 skipped（供方法与局限章节如实说明「未执行」，非清白背书），零耗时。
    if matches!(pb.method, "pdfium" | "pdf-extract") && !cancel.load(Ordering::SeqCst) {
        if pdf_cross_check {
            pb = run_pdf_cross_check(path, pb, cancel, ocr_model);
        } else {
            pb.xcheck =
                Some(crate::engine::pdf_xcheck::XCheckResult::skipped("配置未开启渲染交叉验证"));
        }
    }
    // 内嵌图片同源指纹：与文本抽取路径正交，统一在此提取（pdfium 不可用则空、与 OCR 同降级）。
    // 已取消则不再额外解码图片，尽快返回。
    if !cancel.load(Ordering::SeqCst) {
        pb.image_hashes = collect_image_hashes_pdf(path, cancel);
    }
    Ok(pb)
}

/// 渲染-OCR 抽样交叉验证（W2-4）：确定性抽样若干页 → 栅格化 → OCR → 与文字层逐页比对。
/// 命中（字体重映射/坐标乱序）→ 整文档回落 OCR 并【解除 OCR_MAX_PAGES 上限】（取证需要，
/// 非普通扫描件），method 记 ocr-fallback + 「文字层不可信」提示；未命中/跳过保留文字层。
/// pdfium 不可绑定或 OCR 模型缺失时记 xcheck.skipped，不阻塞导入、不做清白背书。
fn run_pdf_cross_check(
    path: &Path,
    mut pb: ParsedBlocks,
    cancel: &AtomicBool,
    ocr_model: &'static crate::engine::ocr::OcrModelSpec,
) -> ParsedBlocks {
    use crate::engine::pdf_xcheck;
    let total = pb.pages as usize;
    if total == 0 {
        pb.xcheck = Some(pdf_xcheck::XCheckResult::skipped("无页面"));
        return pb;
    }
    // 可疑页优先顶替间隔页：pdf_audit 命中页按隐藏字符数降序（最可疑者先占间隔槽），0 基页索引。
    let suspect: Vec<usize> = match &pb.pdf_audit {
        Some(a) => {
            let mut hits = a.hit_pages.clone();
            hits.sort_by(|x, y| y.hidden_chars.cmp(&x.hidden_chars).then(x.page.cmp(&y.page)));
            hits.iter().map(|p| p.page.saturating_sub(1) as usize).collect()
        }
        None => Vec::new(),
    };
    let sampled_idx = pdf_xcheck::sample_pages(total, &suspect, pdf_xcheck::SAMPLE_K);
    if sampled_idx.is_empty() {
        pb.xcheck = Some(pdf_xcheck::XCheckResult::skipped("无可抽样页"));
        return pb;
    }
    let sampled_1based: Vec<u32> = sampled_idx.iter().map(|&i| i as u32 + 1).collect();
    // 栅格化抽样页（pdfium 不可用则跳过）
    let imgs = match rasterize_pages(path, &sampled_idx, cancel) {
        Some(v) if v.len() == sampled_idx.len() => v,
        _ => {
            if cancel.load(Ordering::SeqCst) {
                return pb; // 取消：不记 xcheck，交由上层归类取消
            }
            pb.xcheck = Some(pdf_xcheck::XCheckResult::skipped("pdfium 不可用或抽样页渲染失败"));
            return pb;
        }
    };
    // OCR 抽样页（模型缺失/识别失败/取消 → None）
    let ocr_texts: Vec<String> = match crate::engine::ocr::ocr_images(imgs, cancel, ocr_model) {
        Some(pages) => pages.into_iter().map(|p| p.text).collect(),
        None => {
            if cancel.load(Ordering::SeqCst) {
                return pb;
            }
            pb.xcheck = Some(pdf_xcheck::XCheckResult::skipped("OCR 不可用（缺模型或识别失败）"));
            return pb;
        }
    };
    // 逐页比对：pdfium 路径块带页码可逐页取文字层；pdf-extract 路径块无页码退化为 shingle 包含率
    let xr = if pb.method == "pdfium" {
        let layer_pages: Vec<String> =
            sampled_idx.iter().map(|&i| page_text_from_blocks(&pb.blocks, i as u32 + 1)).collect();
        pdf_xcheck::evaluate_paged(&sampled_1based, &layer_pages, &ocr_texts)
    } else {
        pdf_xcheck::evaluate_shingle(&sampled_1based, &pb.legacy_text, &ocr_texts)
    };
    if !xr.is_hit() || cancel.load(Ordering::SeqCst) {
        pb.xcheck = Some(xr); // 未命中：保留文字层，xcheck 供日志/方法与局限章节（不写 evasion）
        return pb;
    }
    // 命中：整文档回落 OCR，解除 20 页上限（取证需要）。回落失败则保留文字层但仍记命中的
    // xcheck，evasion 信号照常从 verdict 触发（降级但不静默）。
    let notice = format!(
        "本文档 PDF 文字层与渲染内容不一致（检测到{}），已改用 OCR 识别的文本参与比对，请人工复核。",
        xr.hit_label()
    );
    match parse_pdf_ocr(path, cancel, ocr_model, None) {
        Ok(mut ocr_pb) => {
            ocr_pb.pdf_audit = pb.pdf_audit.take();
            ocr_pb.method = "ocr-fallback";
            // 回落提示优先于（可能不存在的）截断提示；解除上限后一般不会再截断
            ocr_pb.truncation_notice = Some(match ocr_pb.truncation_notice {
                Some(t) => format!("{notice}（{t}）"),
                None => notice,
            });
            ocr_pb.xcheck = Some(xr);
            ocr_pb
        }
        Err(_) => {
            pb.xcheck = Some(xr);
            pb
        }
    }
}

/// 从逐页块集拼接某页（1 起）的文字层文本（pdfium 路径块带页码）。
fn page_text_from_blocks(blocks: &[Block], page: u32) -> String {
    let mut out = String::new();
    for b in blocks.iter().filter(|b| b.page == Some(page)) {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&b.text);
    }
    out
}

/// 扫描件路径：pdfium 栅格化每页 → oar-ocr 识别 → 按页拼接文本 + 行级版面。
/// max_pages=Some(n) 限制渲染前 n 页（普通扫描件控耗时）；None 解除上限（W2-4 交叉验证
/// 命中回落时用——取证需要，超长规避文档后置页不能因 20 页上限退出比对）。
fn parse_pdf_ocr(
    path: &Path,
    cancel: &AtomicBool,
    ocr_model: &'static crate::engine::ocr::OcrModelSpec,
    max_pages: Option<usize>,
) -> Result<ParsedBlocks, String> {
    let (imgs, total_pages) = rasterize_pdf_capped(path, cancel, max_pages)
        .ok_or_else(|| "无法栅格化 PDF（pdfium 不可用）".to_string())?;
    if imgs.is_empty() {
        return Err("PDF 无可渲染页面".into());
    }
    let rendered = imgs.len();
    let truncated = total_pages > rendered;
    // 如实上报总页数（而非被 OCR 上限截断后的数量）
    let pages = total_pages.max(rendered) as u32;
    let ocr_pages = crate::engine::ocr::ocr_images(imgs, cancel, ocr_model)
        .ok_or_else(|| "OCR 不可用（缺模型或识别失败）".to_string())?;
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.into());
    }
    // 旧实现是所有识别行直接拼接（每行带 \n，空页无贡献），逐字符复刻
    let legacy_text: String = ocr_pages.iter().map(|p| p.text.as_str()).collect();
    if legacy_text.trim().is_empty() {
        return Err("OCR 未识别出文本".into());
    }
    // 行级版面按原始页序全量保留（含空页），页码即下标+1
    let layout: Vec<&[crate::engine::ocr::OcrLine]> =
        ocr_pages.iter().map(|p| p.lines.as_slice()).collect();
    let ocr_layout_json = serde_json::to_string(&layout).ok();
    // enumerate 在 filter 之前：保留的是原始页码
    let blocks: Vec<Block> = ocr_pages
        .into_iter()
        .enumerate()
        .filter(|(_, p)| !p.text.trim().is_empty())
        .map(|(i, p)| Block {
            text: p.text,
            heading_level: None,
            page: Some(i as u32 + 1),
            is_table_row: false,
            is_list_item: false,
        })
        .collect();
    // 扫描件超出 OCR 上限：不进正文（避免多份截断件的相同提示语被聚成假雷同/假围标），
    // 改随文档入库并由前端以警示条展示，让用户知晓仅比对了前 N 页。
    let truncation_notice = truncated.then(|| {
        format!(
            "本文档为扫描件，因性能上限仅识别并比对了前 {rendered} 页（共 {total_pages} 页），其余 {} 页未参与查重，请人工复核。",
            total_pages - rendered
        )
    });
    Ok(ParsedBlocks {
        blocks,
        pages,
        fingerprint: pdf_fingerprint(path),
        method: "ocr",
        legacy_text,
        ocr_layout_json,
        truncation_notice,
        // parse_pdf 在分发后统一填充图片指纹与隐藏文字层审计（与文本抽取路径正交）
        image_hashes: Vec::new(),
        pdf_audit: None,
        xcheck: None,
    })
}

/// 扫描件 OCR 的最大渲染页数（控制耗时）。超出的页不参与查重，会在文本首插入醒目提示。
const OCR_MAX_PAGES: usize = 20;

/// PDF 页栅格化的统一渲染配置（宽 1600、限高 2400）；xcheck 抽样页与全量扫描共用。
fn pdf_render_config() -> PdfRenderConfig {
    PdfRenderConfig::new().set_target_width(1600).set_maximum_height(2400)
}

/// pdfium 位图 → RgbImage（手动 BGRA→RGB，避开 image 特性耦合）。尺寸/字节数不合法返回 None。
fn bitmap_to_rgb(bm: &pdfium_render::prelude::PdfBitmap) -> Option<image::RgbImage> {
    let w = bm.width() as u32;
    let h = bm.height() as u32;
    let raw = bm.as_raw_bytes();
    let need = (w as usize) * (h as usize) * 4;
    if w == 0 || h == 0 || raw.len() < need {
        return None;
    }
    let mut rgb = image::RgbImage::new(w, h);
    for (i, px) in rgb.pixels_mut().enumerate() {
        let o = i * 4;
        *px = image::Rgb([raw[o + 2], raw[o + 1], raw[o]]); // BGRA → RGB
    }
    Some(rgb)
}

/// 用 pdfium 把 PDF 各页渲染为 RgbImage（上限 OCR_MAX_PAGES）。仅测试在用——生产路径
/// parse_pdf_ocr 直接调 rasterize_pdf_capped 以参数化页上限（W2-4 回落传 None 解除上限）。
#[cfg(test)]
fn rasterize_pdf(path: &Path, cancel: &AtomicBool) -> Option<(Vec<image::RgbImage>, usize)> {
    rasterize_pdf_capped(path, cancel, Some(OCR_MAX_PAGES))
}

/// 按索引栅格化指定页（W2-4 交叉验证抽样，复用 rasterize 的渲染配置）。返回与 indices 逐一
/// 对应的页图（越界索引跳过 → 长度可能短于 indices，调用方据此判断是否可用）；pdfium 不可
/// 绑定返回 None（与全量栅格化同降级语义）。
fn rasterize_pages(
    path: &Path,
    indices: &[usize],
    cancel: &AtomicBool,
) -> Option<Vec<image::RgbImage>> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium.load_pdf_from_file(path.to_str()?, None).ok()?;
    let pages = doc.pages();
    let total = pages.len() as usize;
    let cfg = pdf_render_config();
    let mut out = Vec::with_capacity(indices.len());
    for &idx in indices {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        if idx >= total {
            continue;
        }
        let Ok(page) = pages.get(idx as u16) else { continue };
        let Ok(bm) = page.render_with_config(&cfg) else { continue };
        let Some(rgb) = bitmap_to_rgb(&bm) else { continue };
        out.push(rgb);
    }
    Some(out)
}

/// 栅格化按页上限参数化：max_pages=Some(n) 渲染前 n 页；None 渲染全部（取证回落）。
fn rasterize_pdf_capped(
    path: &Path,
    cancel: &AtomicBool,
    max_pages: Option<usize>,
) -> Option<(Vec<image::RgbImage>, usize)> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium.load_pdf_from_file(path.to_str()?, None).ok()?;
    let total_pages = doc.pages().len() as usize;
    let cfg = pdf_render_config();
    let mut imgs = Vec::new();
    for page in doc.pages().iter() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let bm = match page.render_with_config(&cfg) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let Some(rgb) = bitmap_to_rgb(&bm) else { continue };
        imgs.push(rgb);
        if max_pages.is_some_and(|m| imgs.len() >= m) {
            break; // 限制渲染页数，控制耗时（None 则渲染全部）
        }
    }
    Some((imgs, total_pages))
}

fn parse_pdf_extract(path: &Path) -> Result<ParsedBlocks, String> {
    let text = pdf_extract::extract_text(path).map_err(|e| format!("PDF 解析失败：{e}"))?;
    if text.trim().is_empty() {
        // 无可提取文本：多半是扫描件（图片），需 OCR。
        return Err("PDF 无可提取文本（疑似扫描件，需 OCR）".into());
    }
    let pages = ((text.chars().count() / 1500) as u32).max(1);
    Ok(ParsedBlocks {
        blocks: vec![Block {
            text: text.clone(),
            heading_level: None,
            page: None,
            is_table_row: false,
            is_list_item: false,
        }],
        pages,
        fingerprint: pdf_fingerprint(path),
        method: "pdf-extract",
        legacy_text: text,
        ocr_layout_json: None,
        truncation_notice: None,
        image_hashes: Vec::new(), // parse_pdf 分发后统一填充
        pdf_audit: None,
        xcheck: None,
    })
}

/// 用 pdfium 抽取文本（逐页，块带页码）。绑定失败或无文本返回 None。
fn parse_pdf_pdfium(path: &Path, cancel: &AtomicBool) -> Option<ParsedBlocks> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium.load_pdf_from_file(path.to_str()?, None).ok()?;
    let mut blocks = Vec::new();
    // 旧实现对每个可读页都追加「文本+\n」（空页也留换行），逐字符复刻
    let mut legacy_text = String::new();
    let mut pages = 0u32;
    for page in doc.pages().iter() {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        pages += 1;
        if let Ok(t) = page.text() {
            let text = t.all().trim().to_string();
            legacy_text.push_str(&text);
            legacy_text.push('\n');
            if !text.is_empty() {
                blocks.push(Block {
                    text,
                    heading_level: None,
                    page: Some(pages),
                    is_table_row: false,
                    is_list_item: false,
                });
            }
        }
    }
    if legacy_text.trim().is_empty() {
        return None; // 扫描件 → 回落 / 后续 OCR
    }
    Some(ParsedBlocks {
        blocks,
        pages: pages.max(1),
        fingerprint: pdf_fingerprint(path),
        method: "pdfium",
        legacy_text,
        ocr_layout_json: None,
        truncation_notice: None,
        image_hashes: Vec::new(), // parse_pdf 分发后统一填充
        pdf_audit: None,
        xcheck: None,
    })
}

/// 在多个候选目录里查找并绑定 libpdfium（dev: src-tauri/binaries；打包: 资源目录）。
fn bind_pdfium() -> Option<Pdfium> {
    for dir in pdfium_dirs() {
        let lib = Pdfium::pdfium_platform_library_name_at_path(&dir);
        if let Ok(b) = Pdfium::bind_to_library(&lib) {
            return Some(Pdfium::new(b));
        }
    }
    None
}

/// pdfium 库是否可绑定（环境自检用）。
pub fn pdfium_available() -> bool {
    bind_pdfium().is_some()
}

fn pdfium_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("BIDGUARD_PDFIUM_DIR") {
        dirs.push(PathBuf::from(p));
    }
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("binaries"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.to_path_buf());
            dirs.push(d.join("binaries")); // Windows 安装目录/binaries
            dirs.push(d.join("../Resources")); // macOS .app
            dirs.push(d.join("../Resources/binaries"));
            dirs.push(d.join("../Frameworks"));
            dirs.push(d.join("../lib")); // Linux
            dirs.push(d.join("lib"));
        }
    }
    dirs.push(PathBuf::from("/usr/lib"));
    dirs.push(PathBuf::from("/usr/local/lib"));
    dirs
}

/// 读 PDF Info 字典作为元数据指纹（作者/Producer/创建/修改时间），
/// 并提取血缘取证字段：trailer /ID、XMP GUID（DocumentID/InstanceID/DerivedFrom/
/// CreatorTool）、逐页 BaseFont 与子集标签。损坏/加密 PDF 保持「load 失败 → 空指纹」
/// 的既有容错语义；各字段取不到即留空，绝不 panic。
fn pdf_fingerprint(path: &Path) -> Fingerprint {
    let mut fp = Fingerprint::default();
    let doc = match lopdf::Document::load(path) {
        Ok(d) => d,
        Err(_) => return fp,
    };
    let info = doc
        .trailer
        .get(b"Info")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|id| doc.get_object(id).ok())
        .and_then(|o| o.as_dict().ok());
    if let Some(dict) = info {
        let get = |k: &[u8]| {
            dict.get(k)
                .ok()
                .and_then(|o| o.as_str().ok())
                .map(pdf_decode_string)
                .filter(|s| !s.trim().is_empty())
        };
        fp.author = get(b"Author");
        fp.app = get(b"Producer").or_else(|| get(b"Creator"));
        fp.created = get(b"CreationDate");
        fp.modified = get(b"ModDate");
    }
    fill_pdf_trailer_id(&doc, &mut fp);
    fill_xmp(&doc, &mut fp);
    fill_pdf_fonts(&doc, &mut fp);
    fp
}

/// trailer /ID：两个字节串的数组，hex 存首半/次半。
/// 首半在创建时生成、之后每次保存保持不变——是「同一母文件」的血缘键。
fn fill_pdf_trailer_id(doc: &lopdf::Document, fp: &mut Fingerprint) {
    let Ok(obj) = doc.trailer.get(b"ID") else { return };
    // /ID 一般为直接数组，个别生成器写成间接引用
    let arr = match obj.as_array() {
        Ok(a) => a,
        Err(_) => {
            let Some(a) = obj
                .as_reference()
                .ok()
                .and_then(|id| doc.get_object(id).ok())
                .and_then(|o| o.as_array().ok())
            else {
                return;
            };
            a
        }
    };
    let hex_of = |o: &lopdf::Object| {
        o.as_str().ok().filter(|b| !b.is_empty()).map(hex_lower)
    };
    fp.pdf_id_first = arr.first().and_then(hex_of);
    fp.pdf_id_second = arr.get(1).and_then(hex_of);
}

fn hex_lower(bytes: &[u8]) -> String {
    use std::fmt::Write;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// 定位 catalog /Metadata 的 XMP 流并宽松解析。XMP 通常未压缩；
/// 带 Filter 的流用 decompressed_content 兜底，失败则按原始字节解析。
fn fill_xmp(doc: &lopdf::Document, fp: &mut Fingerprint) {
    let Some(bytes) = xmp_stream_bytes(doc) else { return };
    parse_xmp(&bytes, fp);
}

fn xmp_stream_bytes(doc: &lopdf::Document) -> Option<Vec<u8>> {
    let meta = doc.catalog().ok()?.get(b"Metadata").ok()?;
    let stream = match meta {
        lopdf::Object::Reference(id) => doc.get_object(*id).ok()?.as_stream().ok()?,
        o => o.as_stream().ok()?,
    };
    match stream.decompressed_content() {
        Ok(b) if !b.is_empty() => Some(b),
        _ => Some(stream.content.clone()), // 无 Filter/解压失败 → 原始字节兜底
    }
}

/// 宽松解析 XMP（自由格式 XML，Word/WPS/永中等写法各异）：
/// 同时接受「元素文本」与「rdf:Description 属性」两种形态；
/// xmpMM:DerivedFrom 的 stRef:documentID 同样两种形态都收。
/// 任何解析错误直接停止（已收到的字段保留），禁止 panic。
fn parse_xmp(xml: &[u8], fp: &mut Fingerprint) {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut cur: Vec<u8> = Vec::new(); // 当前元素 local name（小写）
    let mut in_derived = false; // 在 <xmpMM:DerivedFrom> 内：documentID 归 derived_from
    fn set(slot: &mut Option<String>, val: &str) {
        let v = val.trim();
        if slot.is_none() && !v.is_empty() {
            *slot = Some(v.to_string());
        }
    }
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.local_name().into_inner().to_ascii_lowercase();
                let elem_is_derived = name == b"derivedfrom";
                // 属性形态：<rdf:Description xmpMM:DocumentID="…" xmp:CreatorTool="…"/>
                // 与 <xmpMM:DerivedFrom stRef:documentID="…"/>
                for a in e.attributes().flatten() {
                    let key = a.key.local_name().into_inner().to_ascii_lowercase();
                    let Ok(val) = a.unescape_value() else { continue };
                    match key.as_slice() {
                        b"documentid" if elem_is_derived => set(&mut fp.xmp_derived_from, &val),
                        b"documentid" => set(&mut fp.xmp_document_id, &val),
                        b"instanceid" if !elem_is_derived => set(&mut fp.xmp_instance_id, &val),
                        b"creatortool" => set(&mut fp.creator_tool, &val),
                        _ => {}
                    }
                }
                if elem_is_derived {
                    in_derived = true;
                }
                cur = name;
            }
            Ok(Event::End(e)) => {
                if e.local_name().into_inner().eq_ignore_ascii_case(b"derivedfrom") {
                    in_derived = false;
                }
                cur.clear();
            }
            Ok(Event::Text(t)) => {
                let Ok(val) = t.unescape() else { continue };
                match cur.as_slice() {
                    b"documentid" if in_derived => set(&mut fp.xmp_derived_from, &val),
                    b"documentid" => set(&mut fp.xmp_document_id, &val),
                    b"instanceid" if !in_derived => set(&mut fp.xmp_instance_id, &val),
                    b"creatortool" => set(&mut fp.creator_tool, &val),
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break, // 宽松：坏 XML 保留已解析字段
            _ => {}
        }
        buf.clear();
    }
}

/// PDF 字体全集上限：正常标书几十个，异常文件截断防膨胀。
const MAX_PDF_FONTS: usize = 256;

/// 逐页收集 /Resources /Font 的 BaseFont 名（去重排序），
/// 并抽取 ^[A-Z]{6}\+ 子集标签字体（前缀多数生成器随机 → 同一次生成环境的指纹）。
fn fill_pdf_fonts(doc: &lopdf::Document, fp: &mut Fingerprint) {
    use std::collections::BTreeSet;
    let mut fonts: BTreeSet<String> = BTreeSet::new();
    'pages: for (_, page_id) in doc.get_pages() {
        let Ok(page_fonts) = doc.get_page_fonts(page_id) else { continue };
        for (_, font) in page_fonts {
            if let Ok(name) = font.get(b"BaseFont").and_then(|o| o.as_name()) {
                let s = String::from_utf8_lossy(name).trim().to_string();
                if !s.is_empty() {
                    fonts.insert(s);
                }
                if fonts.len() >= MAX_PDF_FONTS {
                    break 'pages;
                }
            }
        }
    }
    fp.font_subset_tags = fonts.iter().filter(|n| is_subset_tagged(n)).cloned().collect();
    fp.pdf_fonts = fonts.into_iter().collect();
}

/// BaseFont 是否带子集标签（形如 "ABCDEF+SimSun"：6 个大写字母 + '+' + 非空字体名）。
fn is_subset_tagged(name: &str) -> bool {
    let b = name.as_bytes();
    b.len() > 7 && b[6] == b'+' && b[..6].iter().all(|c| c.is_ascii_uppercase())
}

/// PDF 字符串可能是 UTF-16BE(带 BOM) 或 PDFDocEncoding，宽松解码。
fn pdf_decode_string(bytes: &[u8]) -> String {
    if bytes.len() >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
        let u16s: Vec<u16> = bytes[2..]
            .chunks(2)
            .filter(|c| c.len() == 2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u16s)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn parse_docx(
    path: &Path,
    cancel: &AtomicBool,
    ocr_images: bool,
    ocr_model: &'static crate::engine::ocr::OcrModelSpec,
) -> Result<ParsedBlocks, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("非法 docx (zip): {e}"))?;

    let doc_xml = read_zip(&mut zip, "word/document.xml")
        .ok_or_else(|| "docx 缺少 word/document.xml".to_string())?;
    let (mut blocks, mut legacy_text, xml_truncated) = docx_blocks(&doc_xml);

    // 内嵌图片 OCR：截图式报价表/资质/公章里的文字，否则纯文本管线完全看不到
    if ocr_images {
        let img_blocks = docx_image_ocr(&mut zip, cancel, ocr_model);
        for b in &img_blocks {
            legacy_text.push_str(&b.text);
            legacy_text.push('\n');
        }
        blocks.extend(img_blocks);
    }

    let mut fp = Fingerprint::default();
    if let Some(core) = read_zip(&mut zip, "docProps/core.xml") {
        fill_core(&core, &mut fp);
    }
    let mut pages = 0u32;
    if let Some(app) = read_zip(&mut zip, "docProps/app.xml") {
        pages = fill_app(&app, &mut fp);
    }
    // rsid 修订会话标识：取证级同源信号（WPS 等生成的 docx 可能无此节点 → 字段留空）
    if let Some(settings) = read_zip(&mut zip, "word/settings.xml") {
        fill_rsids(&settings, &mut fp);
    }
    // zip 条目序列指纹：中央目录顺序的条目名哈希——同一生成工具/同一打包管线产物稳定一致
    fp.zip_entry_fp = Some(zip_entry_fingerprint(&zip));
    fp.zip_entry_count = Some(zip.len() as u32);
    if pages == 0 {
        pages = ((legacy_text.chars().count() / 1500) as u32).max(1);
    }

    let truncation_notice = xml_truncated.then(|| {
        "文档正文 XML 解析中途出错，仅提取到部分内容，其余段落可能缺失，请人工复核。".to_string()
    });
    // 内嵌图片同源指纹（与 OCR 无关，恒提取）：word/media 位图 → sha256 + dHash
    let image_hashes = collect_image_hashes_docx(&mut zip, cancel);
    Ok(ParsedBlocks {
        blocks,
        pages,
        fingerprint: fp,
        method: "docx",
        legacy_text,
        ocr_layout_json: None,
        truncation_notice,
        image_hashes,
        pdf_audit: None,
        xcheck: None,
    })
}

const MAX_DOCX_IMAGES: usize = 60; // OCR 图片数上限，防止图片墙文档拖垮导入
const MIN_OCR_IMAGE_PX: u32 = 80; // 短边阈值，跳过图标/项目符号/分隔线等装饰图

/// 提取 word/media/ 下的位图，逐张 OCR，识别文本各成一个块（追加在正文之后）。
/// 仅在 ocr_docx_images 开启时调用；模型缺失则静默返回空（与 PDF OCR 同语义）。
/// 按内容去重（页眉 logo 等重复图只 OCR 一次），跳过过小装饰图，逐张查取消。
fn docx_image_ocr<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    cancel: &AtomicBool,
    ocr_model: &'static crate::engine::ocr::OcrModelSpec,
) -> Vec<Block> {
    let imgs = collect_docx_images(zip, cancel);
    if imgs.is_empty() {
        return Vec::new();
    }
    let Some(pages) = crate::engine::ocr::ocr_images(imgs, cancel, ocr_model) else {
        return Vec::new(); // 模型不可用 / 被取消
    };
    pages
        .into_iter()
        .filter(|p| !p.text.trim().is_empty())
        .map(|p| Block {
            text: p.text.trim().to_string(),
            heading_level: None,
            page: None,
            is_table_row: false,
            is_list_item: false,
        })
        .collect()
}

/// 从 word/media/ 收集可 OCR 的位图：按内容去重、跳过装饰小图、限量。
fn collect_docx_images<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    cancel: &AtomicBool,
) -> Vec<image::RgbImage> {
    use std::collections::HashSet;
    let names: Vec<String> = zip
        .file_names()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.starts_with("word/media/")
                && [".png", ".jpg", ".jpeg", ".bmp", ".gif", ".tif", ".tiff"]
                    .iter()
                    .any(|e| l.ends_with(e))
        })
        .map(String::from)
        .collect();
    let mut imgs: Vec<image::RgbImage> = Vec::new();
    let mut seen: HashSet<u64> = HashSet::new();
    for name in names {
        if cancel.load(Ordering::SeqCst) || imgs.len() >= MAX_DOCX_IMAGES {
            break;
        }
        let Some(bytes) = read_zip(zip, &name) else { continue };
        // 内容去重：同一张图（如每页页眉 logo）只收一次
        let h = {
            use std::hash::{Hash, Hasher};
            let mut s = std::collections::hash_map::DefaultHasher::new();
            bytes.hash(&mut s);
            s.finish()
        };
        if !seen.insert(h) {
            continue;
        }
        // emf/wmf 矢量图、损坏图解码失败 → 跳过（无可 OCR 像素）
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        if img.width() < MIN_OCR_IMAGE_PX || img.height() < MIN_OCR_IMAGE_PX {
            continue;
        }
        imgs.push(img.to_rgb8());
    }
    imgs
}

// —— 内嵌图片同源检测（W1-4）：提取位图 → sha256 精确指纹 + 64 位 dHash 近似指纹 ——
/// 单文档图片指纹条数上限（独立于 OCR 的 MAX_DOCX_IMAGES）：两两碰撞是 N²，200 已足够
/// 覆盖典型标书的资质/公章/现场照片，且解码耗时可控。
const MAX_IMAGE_HASHES: usize = 200;
/// 短边阈值：跳过图标/项目符号/分隔线等装饰小图（与 OCR 口径一致）。
const MIN_IMAGE_HASH_PX: u32 = 80;
/// 整页图判定：图对象面积 / 页面积 > 此比例视为「整页扫描图」，只做 exact 不做 near，
/// 防「都是空白页/同制式表格」被 dHash 判成同源。拍板值，未经校准。
const FULL_PAGE_AREA_RATIO: f64 = 0.8;

/// 从 word/media/ 收集内嵌位图并计算同源指纹。复用 collect_docx_images 的遍历骨架，
/// 但独立限额（MAX_IMAGE_HASHES）、按像素内容去重（同一 logo 只留一条）。docx 无「整页」
/// 概念，dHash 恒有值。
fn collect_image_hashes_docx<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    cancel: &AtomicBool,
) -> Vec<ImageHash> {
    use std::collections::HashSet;
    let names: Vec<String> = zip
        .file_names()
        .filter(|n| {
            let l = n.to_ascii_lowercase();
            l.starts_with("word/media/")
                && [".png", ".jpg", ".jpeg", ".bmp", ".gif", ".tif", ".tiff"]
                    .iter()
                    .any(|e| l.ends_with(e))
        })
        .map(String::from)
        .collect();
    let mut out: Vec<ImageHash> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for name in names {
        if cancel.load(Ordering::SeqCst) || out.len() >= MAX_IMAGE_HASHES {
            break;
        }
        let Some(bytes) = read_zip(zip, &name) else { continue };
        // emf/wmf 矢量图、损坏图解码失败 → 跳过（无可比对像素）
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        let rgb = img.to_rgb8();
        let (w, h) = (rgb.width(), rgb.height());
        if w < MIN_IMAGE_HASH_PX || h < MIN_IMAGE_HASH_PX {
            continue;
        }
        let sha256 = image_sha256(&rgb);
        // 内容去重：同一张图（每页页眉 logo）只记一次，避免撑爆限额与虚增命中对
        if !seen.insert(sha256.clone()) {
            continue;
        }
        out.push(ImageHash {
            source: "docx",
            page: None,
            width: w,
            height: h,
            sha256,
            dhash: Some(dhash64(&rgb)),
        });
    }
    out
}

/// 遍历 PDF 各页的图片对象（pdfium）计算同源指纹。pdfium 不可用/文件损坏则返回空
/// （与 OCR 同降级语义）。逐图 try + cancel 检查；整页图（面积占比 > FULL_PAGE_AREA_RATIO）
/// 的 dHash 记 None（只做 exact）。
fn collect_image_hashes_pdf(path: &Path, cancel: &AtomicBool) -> Vec<ImageHash> {
    use std::collections::HashSet;
    let Some(pdfium) = bind_pdfium() else { return Vec::new() };
    let Some(pstr) = path.to_str() else { return Vec::new() };
    let Ok(doc) = pdfium.load_pdf_from_file(pstr, None) else { return Vec::new() };
    let mut out: Vec<ImageHash> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    'pages: for (pi, page) in doc.pages().iter().enumerate() {
        if out.len() >= MAX_IMAGE_HASHES {
            break;
        }
        let page_area = f64::from(page.width().value.max(0.0)) * f64::from(page.height().value.max(0.0));
        for obj in page.objects().iter() {
            if cancel.load(Ordering::SeqCst) || out.len() >= MAX_IMAGE_HASHES {
                break 'pages;
            }
            let PdfPageObject::Image(ref img_obj) = obj else { continue };
            // get_raw_image 内部按位图格式（BGRA/BGR/Gray）安全转换并返回 Result，不 panic；
            // 个别损坏图对象失败则跳过该图
            let Ok(dynimg) = img_obj.get_raw_image() else { continue };
            let rgb = dynimg.to_rgb8();
            let (w, h) = (rgb.width(), rgb.height());
            if w < MIN_IMAGE_HASH_PX || h < MIN_IMAGE_HASH_PX {
                continue;
            }
            // 整页图判定：对象在页面上的占面比（点² 口径）
            let full_page = img_obj
                .bounds()
                .ok()
                .map(|b| {
                    let obj_area =
                        f64::from(b.width().value.abs()) * f64::from(b.height().value.abs());
                    page_area > 0.0 && obj_area / page_area > FULL_PAGE_AREA_RATIO
                })
                .unwrap_or(false);
            let sha256 = image_sha256(&rgb);
            if !seen.insert(sha256.clone()) {
                continue; // 同页对象引用同一图 / 跨页重复 logo → 只记一次
            }
            out.push(ImageHash {
                source: "pdf",
                page: Some(pi as u32 + 1),
                width: w,
                height: h,
                sha256,
                dhash: (!full_page).then(|| dhash64(&rgb)),
            });
        }
    }
    out
}

/// 精确指纹：sha256(宽 LE ‖ 高 LE ‖ RGB8 像素字节)。带上尺寸避免不同尺寸的纯色图
/// 撞哈希；用像素而非文件字节，令同一图的不同容器编码（PNG/JPEG 无损重存）指纹一致。
fn image_sha256(rgb: &image::RgbImage) -> String {
    let mut buf = Vec::with_capacity(8 + rgb.as_raw().len());
    buf.extend_from_slice(&rgb.width().to_le_bytes());
    buf.extend_from_slice(&rgb.height().to_le_bytes());
    buf.extend_from_slice(rgb.as_raw());
    crate::engine::normalize::sha256_hex(&buf)
}

/// 64 位 dHash（difference hash）：灰度 → 缩放到 9×8 → 每行 8 对相邻像素取横向梯度符号。
/// 对缩放/轻度压缩/重编码稳健，两图汉明距离小即视觉近似。自实现（不引 img_hash crate）。
fn dhash64(rgb: &image::RgbImage) -> u64 {
    use image::imageops::{grayscale, resize, FilterType};
    let gray = grayscale(rgb);
    // 9 列 × 8 行：每行相邻两列比较得 8 位，共 64 位
    let small = resize(&gray, 9, 8, FilterType::Triangle);
    let mut bits = 0u64;
    for row in 0..8u32 {
        for col in 0..8u32 {
            let left = small.get_pixel(col, row)[0];
            let right = small.get_pixel(col + 1, row)[0];
            bits = (bits << 1) | u64::from(left > right);
        }
    }
    bits
}

fn read_zip<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// 从 pStyle 样式 id 推断标题层级。
/// 英文 Word 通常为 "Heading1".."Heading9"；中文 Word/WPS 的「标题 N」样式 id 常为 "1".."9"。
fn heading_level_of_style(val: &str) -> Option<u8> {
    let v = val.trim();
    let digits = if let Some(rest) = v
        .to_ascii_lowercase()
        .strip_prefix("heading")
    {
        rest.trim().to_string()
    } else {
        v.to_string()
    };
    match digits.parse::<u8>() {
        Ok(n) if (1..=9).contains(&n) => Some(n),
        _ => None,
    }
}

/// 提取 word/document.xml：按段落 <w:p> 产出块，识别标题层级；
/// 表格 <w:tbl> 按行产出（单元格以「 | 」连接），嵌套表格的文本并入外层单元格不丢字。
/// outlineLvl（大纲级别 0-8）优先于 pStyle 样式名推断，两者都有时取 outlineLvl。
/// 同步构建 legacy 全文：每个 </w:p> 追加「未裁剪段文+\n」（含空段落），与旧 docx_text 等价。
/// 返回 (段块, 全文, xml_truncated)。xml_truncated=true 表示正文 XML 解析中途出错
/// 提前中止——其后段落全部丢失，调用方须显式告知用户「内容可能不完整」而非静默当完整。
fn docx_blocks(xml: &[u8]) -> (Vec<Block>, String, bool) {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut legacy = String::new();
    let mut xml_truncated = false;
    let mut in_t = false;
    let mut para = String::new();
    let mut style_level: Option<u8> = None;
    let mut outline_level: Option<u8> = None;
    let mut is_list = false; // 当前段落带 w:numPr（编号/项目符号）
    // 表格状态：仅最外层（depth==1）跟踪行列结构，嵌套表格文本随段落落入外层单元格
    let mut tbl_depth = 0usize;
    let mut row_cells: Vec<String> = Vec::new();
    let mut cell = String::new();

    let attr_val = |e: &quick_xml::events::BytesStart| -> Option<String> {
        e.attributes()
            .flatten()
            .find(|a| a.key.local_name().into_inner() == b"val")
            .and_then(|a| String::from_utf8(a.value.into_owned()).ok())
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match e.local_name().into_inner() {
                b"t" => in_t = true,
                b"p" => {
                    para.clear();
                    style_level = None;
                    outline_level = None;
                    is_list = false;
                }
                b"numPr" => is_list = true,
                b"tbl" => tbl_depth += 1,
                b"tr" if tbl_depth == 1 => row_cells.clear(),
                b"tc" if tbl_depth == 1 => cell.clear(),
                _ => {}
            },
            // pStyle / outlineLvl 通常是自闭合标签 <w:pStyle w:val="..."/>
            Ok(Event::Empty(e)) => match e.local_name().into_inner() {
                b"pStyle" => {
                    style_level = attr_val(&e).as_deref().and_then(heading_level_of_style);
                }
                b"outlineLvl" => {
                    outline_level = attr_val(&e)
                        .and_then(|v| v.trim().parse::<u8>().ok())
                        .filter(|n| *n <= 8)
                        .map(|n| n + 1);
                }
                b"numPr" => is_list = true,
                _ => {}
            },
            Ok(Event::End(e)) => {
                let ln = e.local_name();
                let n = ln.into_inner();
                if n == b"t" {
                    in_t = false;
                } else if n == b"p" {
                    legacy.push_str(&para);
                    legacy.push('\n');
                    let text = para.trim();
                    if tbl_depth >= 1 {
                        // 表格内段落进当前单元格（多段落以空格连接），不产出普通块
                        if !text.is_empty() {
                            if !cell.is_empty() {
                                cell.push(' ');
                            }
                            cell.push_str(text);
                        }
                    } else if !text.is_empty() {
                        blocks.push(Block {
                            text: text.to_string(),
                            heading_level: outline_level.or(style_level),
                            page: None,
                            is_table_row: false,
                            is_list_item: is_list,
                        });
                    }
                } else if n == b"tc" && tbl_depth == 1 {
                    row_cells.push(std::mem::take(&mut cell));
                } else if n == b"tr" && tbl_depth == 1 {
                    if row_cells.iter().any(|c| !c.is_empty()) {
                        blocks.push(Block {
                            text: row_cells.join(" | "),
                            heading_level: None,
                            page: None,
                            is_table_row: true,
                            is_list_item: false,
                        });
                    }
                    row_cells.clear();
                } else if n == b"tbl" {
                    tbl_depth = tbl_depth.saturating_sub(1);
                }
            }
            Ok(Event::Text(t)) => {
                if in_t {
                    if let Ok(s) = t.unescape() {
                        para.push_str(&s);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                xml_truncated = true;
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    (blocks, legacy, xml_truncated)
}

/// 解析 docProps/core.xml：作者、最后保存者、创建/修改时间、修订号。
fn fill_core(xml: &[u8], fp: &mut Fingerprint) {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => cur = e.local_name().into_inner().to_vec(),
            Ok(Event::End(_)) => cur.clear(),
            Ok(Event::Text(t)) => {
                let val = t.unescape().map(|s| s.into_owned()).unwrap_or_default();
                if !val.trim().is_empty() {
                    match cur.as_slice() {
                        b"creator" => fp.author = Some(val),
                        b"lastModifiedBy" => fp.last_modified_by = Some(val),
                        b"created" => fp.created = Some(val),
                        b"modified" => fp.modified = Some(val),
                        b"revision" => fp.revision = Some(val),
                        _ => {}
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
}

/// 解析 docProps/app.xml：应用、总编辑时长、页数、模板名。返回页数（0 表示未知）。
fn fill_app(xml: &[u8], fp: &mut Fingerprint) -> u32 {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut pages = 0u32;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => cur = e.local_name().into_inner().to_vec(),
            Ok(Event::End(_)) => cur.clear(),
            Ok(Event::Text(t)) => {
                let val = t.unescape().map(|s| s.into_owned()).unwrap_or_default();
                match cur.as_slice() {
                    b"Application" => fp.app = Some(val),
                    b"TotalTime" => fp.total_edit_minutes = val.trim().parse::<i64>().ok(),
                    b"Pages" => pages = val.trim().parse::<u32>().unwrap_or(0),
                    b"Template" if !val.trim().is_empty() => {
                        fp.template_name = Some(val.trim().to_string())
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    pages
}

/// rsid 提取上限：settings.xml 的 <w:rsids> 正常几十到几百个，异常大文件截断防膨胀。
const MAX_RSIDS: usize = 2048;

/// 解析 word/settings.xml：提取 <w:rsids> 下全部 w:rsid 的 w:val 与 w:rsidRoot。
/// 去重、大写归一（Word 输出大小写不一）、上限 MAX_RSIDS；rsidRoot 同时并入 rsids
/// 集合（交集计数不因「root 单列」而漏算）。节点缺失（如 WPS 产物）时字段留空不报错。
fn fill_rsids(xml: &[u8], fp: &mut Fingerprint) {
    use std::collections::HashSet;
    /// 取 w:val 属性值：trim + 大写归一，空值视为无。
    fn rsid_val(e: &quick_xml::events::BytesStart) -> Option<String> {
        e.attributes()
            .flatten()
            .find_map(|a| {
                (a.key.local_name().into_inner() == b"val")
                    .then(|| String::from_utf8_lossy(&a.value).trim().to_ascii_uppercase())
            })
            .filter(|v| !v.is_empty())
    }
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut in_rsids = false;
    let mut seen: HashSet<String> = HashSet::new();
    let mut ordered: Vec<String> = Vec::new();
    let mut root: Option<String> = None;
    loop {
        // w:rsid/w:rsidRoot 均为自闭合空元素：Start 与 Empty 两种事件都要接
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                match e.local_name().into_inner() {
                    b"rsids" => in_rsids = true,
                    n @ (b"rsid" | b"rsidRoot") if in_rsids && ordered.len() < MAX_RSIDS => {
                        if let Some(val) = rsid_val(&e) {
                            if n == b"rsidRoot" {
                                root = Some(val.clone());
                            }
                            if seen.insert(val.clone()) {
                                ordered.push(val);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::End(e)) => {
                if e.local_name().into_inner() == b"rsids" {
                    in_rsids = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    fp.rsids = ordered;
    fp.rsid_root = root;
}

/// zip 条目序列指纹：按中央目录顺序连接条目名后 sha256。
/// 条目顺序与命名由生成工具决定——同一工具/同一打包管线产物稳定一致，改内容不改结构。
fn zip_entry_fingerprint<R: Read + std::io::Seek>(zip: &zip::ZipArchive<R>) -> String {
    let names: Vec<&str> = (0..zip.len()).filter_map(|i| zip.name_for_index(i)).collect();
    crate::engine::normalize::sha256_hex(names.join("\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn pdfium_binds_and_extracts() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        // 绑定 pdfium 并抽取；当前平台无对应原生库时优雅跳过（如 Linux CI）。
        let Some(pd) = parse_pdf_pdfium(&fixture, &no_cancel()) else {
            eprintln!("跳过 pdfium 测试：当前平台无可用 libpdfium");
            return;
        };
        let lower = pd.legacy_text.to_lowercase();
        assert!(
            lower.contains("bidguard") || lower.contains("gateway"),
            "pdfium 抽取文本应含已知词，实际：{:?}",
            pd.legacy_text
        );
        assert!(pd.blocks.iter().all(|b| b.page.is_some()), "pdfium 块应带页码");
        assert!(pd.legacy_text.ends_with('\n'), "旧格式每页以换行结尾");
    }

    #[test]
    fn parses_pdf_fixture_via_public_api() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return; // 无夹具时跳过
        }
        let parsed = parse_file_blocks(&fixture, &no_cancel()).expect("应能解析样例 PDF");
        assert!(!parsed.legacy_text.trim().is_empty(), "PDF 抽取文本不应为空");
        let lower = parsed.legacy_text.to_lowercase();
        assert!(
            lower.contains("bidguard") || lower.contains("gateway"),
            "应抽取到已知英文文本，实际：{:?}",
            parsed.legacy_text
        );
    }

    #[test]
    fn decodes_gbk_text() {
        let (gbk, _, _) = encoding_rs::GB18030.encode("投标文件 报价 1280 万元");
        let s = decode_text(&gbk);
        assert!(s.contains("投标文件") && s.contains("报价"), "GBK 解码失败：{s:?}");
        assert_eq!(decode_text("hello 中文".as_bytes()), "hello 中文");
    }

    #[test]
    fn decodes_utf16_bom_text() {
        // Windows 记事本「Unicode」存档：UTF-16LE/BE 带 BOM。旧实现落 GB18030 → 乱码静默入库。
        let text = "投标报价壹佰贰拾捌万元";
        let mut le = vec![0xFF, 0xFE];
        for u in text.encode_utf16() {
            le.extend_from_slice(&u.to_le_bytes());
        }
        assert_eq!(decode_text(&le), text, "UTF-16LE 解码失败");
        let mut be = vec![0xFE, 0xFF];
        for u in text.encode_utf16() {
            be.extend_from_slice(&u.to_be_bytes());
        }
        assert_eq!(decode_text(&be), text, "UTF-16BE 解码失败");
    }

    #[test]
    fn docx_blocks_flags_truncation_on_malformed_xml() {
        // 第二段用不匹配的结束标签(</w:BADEND> vs <w:p>) → quick-xml 报错，旧实现静默 break
        // 丢弃其后全部段落且不告知；修复后 truncated=true 供上层显式警示，同时保留已解析段落。
        let xml = "<?xml version=\"1.0\"?>\n<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body>\n<w:p><w:r><w:t>已解析的第一段</w:t></w:r></w:p>\n<w:p><w:r><w:t>丢失的第二段</w:t></w:r></w:BADEND></w:body></w:document>";
        let (blocks, _legacy, truncated) = docx_blocks(xml.as_bytes());
        assert!(truncated, "非法 XML 应标记 truncated");
        assert!(blocks.iter().any(|b| b.text.contains("已解析的第一段")), "错误点前的段落应保留");
        assert!(!blocks.iter().any(|b| b.text.contains("丢失的第二段")), "错误点后的段落确会丢失");
    }

    #[test]
    fn docx_blocks_extract_heading_levels() {
        // 英文样式 id / 中文数字样式 id / outlineLvl 三种来源
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>第一章 项目概述</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="2"/></w:pPr><w:r><w:t>1.1 建设目标</w:t></w:r></w:p>
<w:p><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:r><w:t>1.1.1 总体要求</w:t></w:r></w:p>
<w:p><w:r><w:t>本项目采用微服务架构。</w:t></w:r></w:p>
</w:body></w:document>"#;
        let (blocks, legacy, truncated) = docx_blocks(xml.as_bytes());
        assert!(!truncated, "合法 XML 不应标记截断");
        assert_eq!(blocks.len(), 4);
        assert_eq!(blocks[0].heading_level, Some(1));
        assert_eq!(blocks[1].heading_level, Some(2));
        assert_eq!(blocks[2].heading_level, Some(3));
        assert_eq!(blocks[3].heading_level, None);
        assert_eq!(blocks[0].text, "第一章 项目概述");
        assert_eq!(
            legacy,
            "第一章 项目概述\n1.1 建设目标\n1.1.1 总体要求\n本项目采用微服务架构。\n"
        );
    }

    fn page_block(page: u32, text: &str) -> Block {
        Block {
            text: text.to_string(),
            heading_level: None,
            page: Some(page),
            is_table_row: false,
            is_list_item: false,
        }
    }

    #[test]
    fn strips_repeating_headers_footers_and_page_numbers() {
        let mut blocks: Vec<Block> = (1..=4)
            .map(|p| {
                page_block(
                    p,
                    &format!(
                        "某某科技投标文件\n第 {p} 页\n这是关于系统架构设计方案的正文内容，章节编号 {p}。\n保密文件 请勿外传"
                    ),
                )
            })
            .collect();
        strip_header_footer(&mut blocks);
        for (i, b) in blocks.iter().enumerate() {
            assert!(!b.text.contains("某某科技投标文件"), "页眉应清除：{}", b.text);
            assert!(!b.text.contains("保密文件"), "页脚应清除：{}", b.text);
            assert!(!b.text.contains(&format!("第 {} 页", i + 1)), "页码行应清除：{}", b.text);
            assert!(b.text.contains("正文内容"), "正文应保留：{}", b.text);
        }
    }

    #[test]
    fn strip_header_footer_skips_short_docs_and_unique_lines() {
        // 仅 2 页 → 不处理
        let mut two = vec![page_block(1, "页眉\n正文一"), page_block(2, "页眉\n正文二")];
        strip_header_footer(&mut two);
        assert!(two[0].text.contains("页眉"), "不足 3 页不应清理");
        // 每页首行都不同 → 不视为页眉
        let mut uniq: Vec<Block> = (1..=4)
            .map(|p| page_block(p, &format!("第{p}章 标题各不相同\n正文内容第 {p} 部分说明")))
            .collect();
        strip_header_footer(&mut uniq);
        assert!(uniq[0].text.contains("第1章"), "非重复首行应保留");
        // 无页码的块（docx/txt）不动
        let mut plain = vec![
            Block { text: "公司名\n正文".into(), heading_level: None, page: None, is_table_row: false, is_list_item: false },
        ];
        strip_header_footer(&mut plain);
        assert!(plain[0].text.contains("公司名"));
    }

    #[test]
    fn reflow_joins_soft_wrapped_lines_into_paragraphs() {
        // pdfium 式按视觉行断行：一段被拆成多行，行尾无句号者应回流拼接
        let raw = "本项目建设目标是构建统一的智慧水务管理平台，实现各业务\n\
                   系统的数据汇聚与共享，全面提升运营管理水平。\n\
                   平台采用分层解耦的微服务架构，支持横向扩展。";
        let out = reflow_wrapped_lines(raw);
        let paras: Vec<&str> = out.split('\n').collect();
        assert_eq!(paras.len(), 2, "应回流成 2 段，而非 3 行：{paras:?}");
        assert_eq!(paras[0], "本项目建设目标是构建统一的智慧水务管理平台，实现各业务系统的数据汇聚与共享，全面提升运营管理水平。");
        assert_eq!(paras[1], "平台采用分层解耦的微服务架构，支持横向扩展。");
        // CJK 相邻拼接不补空格
        assert!(!paras[0].contains("业务 系统"));
    }

    #[test]
    fn reflow_handles_blank_lines_and_latin() {
        // 空行分段；西文词间补空格、行尾连字符消解
        let raw = "first para line one\nline two ends here.\n\nThe quick brown fox jum-\nped over.";
        let out = reflow_wrapped_lines(raw);
        let paras: Vec<&str> = out.split('\n').collect();
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0], "first para line one line two ends here.");
        assert_eq!(paras[1], "The quick brown fox jumped over.", "连字符断词应消解：{}", paras[1]);
    }

    #[test]
    fn reflow_via_chunker_yields_paragraph_not_per_line() {
        let jieba = jieba_rs::Jieba::new();
        let raw = "本项目采用分层解耦的微服务总体架构，平台自下而上划分为基础设施层、数据资源\n\
                   层、应用支撑层与业务应用层，所有能力对外以统一接口网关暴露。";
        let reflowed = reflow_wrapped_lines(raw);
        let block = vec![Block { text: reflowed, heading_level: None, page: Some(3), is_table_row: false, is_list_item: false }];
        let chunks = crate::engine::chunker::chunk(&jieba, &block, &Default::default());
        let paras: Vec<_> = chunks.iter().filter(|c| c.chunk_level == "paragraph" && c.chunk_type == "paragraph").collect();
        assert_eq!(paras.len(), 1, "应是 1 个完整段落，而非每行一段：{}", paras.len());
        assert!(paras[0].text.contains("基础设施层、数据资源层"), "跨行的词应被拼回：{}", paras[0].text);
    }

    #[test]
    fn page_number_line_detection() {
        for s in ["3", "- 3 -", "第 3 页", "3 / 12", "第3页 共12页"] {
            assert!(is_page_number_line(s), "{s}");
        }
        for s in ["3 年质保", "报价 3 万元", "第三章", "目录"] {
            assert!(!is_page_number_line(s), "{s}");
        }
    }

    /// 手造最小合法 xlsx（inline string + 数字单元格），免引入写表格的依赖。
    fn write_min_xlsx(dir: &Path, name: &str) -> PathBuf {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let p = dir.join(name);
        let f = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let o = SimpleFileOptions::default();
        zw.start_file("[Content_Types].xml", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#.as_bytes()).unwrap();
        zw.start_file("_rels/.rels", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.as_bytes()).unwrap();
        zw.start_file("xl/workbook.xml", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="报价清单" sheetId="1" r:id="rId1"/></sheets></workbook>"#.as_bytes()).unwrap();
        zw.start_file("xl/_rels/workbook.xml.rels", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#.as_bytes()).unwrap();
        zw.start_file("xl/worksheets/sheet1.xml", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>
<row r="1"><c r="A1" t="inlineStr"><is><t>序号</t></is></c><c r="B1" t="inlineStr"><is><t>设备名称</t></is></c><c r="C1" t="inlineStr"><is><t>单价</t></is></c></row>
<row r="2"><c r="A2"><v>1</v></c><c r="B2" t="inlineStr"><is><t>核心交换机</t></is></c><c r="C2"><v>64000</v></c></row>
<row r="3"><c r="A3"><v>2</v></c><c r="B3" t="inlineStr"><is><t>万兆光模块</t></is></c><c r="C3"><v>3500.5</v></c></row>
</sheetData></worksheet>"#.as_bytes()).unwrap();
        zw.finish().unwrap();
        p
    }

    /// 纯色 PNG 字节（用于尺寸/去重测试，无文字）。
    fn solid_png(w: u32, h: u32, lum: u8) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb([lum, lum, lum]));
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img)
            .write_to(&mut buf, image::ImageFormat::Png)
            .unwrap();
        buf.into_inner()
    }

    /// 手造带 word/media/ 图片的 docx（body_xml + 若干图片字节）。
    fn write_docx_with_media(dir: &Path, name: &str, body_xml: &str, media: &[(&str, Vec<u8>)]) -> String {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        let p = dir.join(name);
        let f = std::fs::File::create(&p).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let o = SimpleFileOptions::default();
        zw.start_file("[Content_Types].xml", o).unwrap();
        zw.write_all(r#"<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="png" ContentType="image/png"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>"#.as_bytes()).unwrap();
        zw.start_file("word/document.xml", o).unwrap();
        let xml = format!(r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body_xml}</w:body></w:document>"#);
        zw.write_all(xml.as_bytes()).unwrap();
        for (fname, bytes) in media {
            zw.start_file(format!("word/media/{fname}"), o).unwrap();
            zw.write_all(bytes).unwrap();
        }
        zw.finish().unwrap();
        p.to_string_lossy().into_owned()
    }

    fn open_zip(path: &str) -> zip::ZipArchive<std::fs::File> {
        zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap()
    }

    /// 手造 docx：正文 + 任意附加 zip 部件（settings.xml / docProps 等取证夹具用）。
    fn write_docx_with_parts(
        dir: &Path,
        name: &str,
        body_xml: &str,
        parts: &[(&str, String)],
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
        let xml = format!(r#"<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>{body_xml}</w:body></w:document>"#);
        zw.write_all(xml.as_bytes()).unwrap();
        for (fname, content) in parts {
            zw.start_file(*fname, o).unwrap();
            zw.write_all(content.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
        p.to_string_lossy().into_owned()
    }

    /// 手造带血缘取证特征的最小 PDF：trailer /ID、XMP Metadata 流、页面字体。
    /// 用 lopdf 现有创建 API 生成（免手写 xref 偏移）。
    fn write_lineage_pdf(
        dir: &Path,
        name: &str,
        id_first: &[u8],
        xmp: Option<&str>,
        base_fonts: &[&str],
    ) -> PathBuf {
        use lopdf::{dictionary, Document, Object, Stream, StringFormat};
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let mut font_dict = lopdf::Dictionary::new();
        for (i, bf) in base_fonts.iter().enumerate() {
            let fid = doc.add_object(dictionary! {
                "Type" => "Font",
                "Subtype" => "Type1",
                "BaseFont" => Object::Name(bf.as_bytes().to_vec()),
            });
            font_dict.set(format!("F{}", i + 1), fid);
        }
        let content_id = doc.add_object(Stream::new(dictionary! {}, b"BT ET".to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "Resources" => dictionary! { "Font" => font_dict },
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let mut catalog = dictionary! { "Type" => "Catalog", "Pages" => pages_id };
        if let Some(x) = xmp {
            let mid = doc.add_object(
                Stream::new(
                    dictionary! { "Type" => "Metadata", "Subtype" => "XML" },
                    x.as_bytes().to_vec(),
                )
                .with_compression(false),
            );
            catalog.set("Metadata", mid);
        }
        let catalog_id = doc.add_object(catalog);
        doc.trailer.set("Root", catalog_id);
        doc.trailer.set(
            "ID",
            Object::Array(vec![
                Object::String(id_first.to_vec(), StringFormat::Hexadecimal),
                Object::String(b"\x22\x22".to_vec(), StringFormat::Hexadecimal),
            ]),
        );
        let p = dir.join(name);
        doc.save(&p).unwrap();
        p
    }

    const XMP_ELEMENT_FORM: &str = r#"<?xpacket begin="" id="W5M0MpCehiHzreSzNTczkc9d"?>
<x:xmpmeta xmlns:x="adobe:ns:meta/">
 <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <rdf:Description xmlns:xmpMM="http://ns.adobe.com/xap/1.0/mm/" xmlns:xmp="http://ns.adobe.com/xap/1.0/" xmlns:stRef="http://ns.adobe.com/xap/1.0/sType/ResourceRef#">
   <xmp:CreatorTool>WPS 文字</xmp:CreatorTool>
   <xmpMM:DocumentID>uuid:AAAA-BBBB-CCCC</xmpMM:DocumentID>
   <xmpMM:InstanceID>uuid:1111-2222</xmpMM:InstanceID>
   <xmpMM:DerivedFrom stRef:instanceID="uuid:MOTHER-INST" stRef:documentID="uuid:MOTHER-GUID"/>
  </rdf:Description>
 </rdf:RDF>
</x:xmpmeta>
<?xpacket end="w"?>"#;

    #[test]
    fn pdf_lineage_fields_extracted() {
        let dir = std::env::temp_dir().join(format!("bg_pdflin_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_lineage_pdf(
            &dir,
            "lineage.pdf",
            b"\xAB\xCD\x12\x34",
            Some(XMP_ELEMENT_FORM),
            &["ABCDEF+SimSun", "Arial", "GHIJKL+KaiTi_GB2312"],
        );
        let fp = pdf_fingerprint(&p);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(fp.pdf_id_first.as_deref(), Some("abcd1234"), "trailer /ID 首半 hex");
        assert_eq!(fp.pdf_id_second.as_deref(), Some("2222"));
        assert_eq!(fp.xmp_document_id.as_deref(), Some("uuid:AAAA-BBBB-CCCC"));
        assert_eq!(fp.xmp_instance_id.as_deref(), Some("uuid:1111-2222"), "DerivedFrom 的 stRef:instanceID 不得覆盖");
        assert_eq!(fp.xmp_derived_from.as_deref(), Some("uuid:MOTHER-GUID"));
        assert_eq!(fp.creator_tool.as_deref(), Some("WPS 文字"));
        assert_eq!(
            fp.pdf_fonts,
            vec!["ABCDEF+SimSun", "Arial", "GHIJKL+KaiTi_GB2312"],
            "BaseFont 全集去重排序"
        );
        assert_eq!(
            fp.font_subset_tags,
            vec!["ABCDEF+SimSun", "GHIJKL+KaiTi_GB2312"],
            "^[A-Z]{{6}}\\+ 子集标签字体"
        );
    }

    #[test]
    fn xmp_attribute_form_and_absence_are_lenient() {
        // 国产/Adobe 工具常把 XMP 写成 rdf:Description 属性形态
        let mut fp = Fingerprint::default();
        parse_xmp(
            br#"<x:xmpmeta xmlns:x="adobe:ns:meta/"><rdf:RDF xmlns:rdf="r"><rdf:Description xmpMM:DocumentID="xmp.did:FIXED-1" xmp:CreatorTool="Microsoft Word 2016"/></rdf:RDF></x:xmpmeta>"#,
            &mut fp,
        );
        assert_eq!(fp.xmp_document_id.as_deref(), Some("xmp.did:FIXED-1"));
        assert_eq!(fp.creator_tool.as_deref(), Some("Microsoft Word 2016"));
        assert!(fp.xmp_derived_from.is_none());
        // 坏 XML / 无关内容：取不到就留空，禁止 panic
        let mut empty = Fingerprint::default();
        parse_xmp(b"\x00\x01 not xml at all <<", &mut empty);
        assert!(empty.xmp_document_id.is_none());
        // 无 XMP 流的 PDF：字段留空
        let dir = std::env::temp_dir().join(format!("bg_noxmp_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_lineage_pdf(&dir, "noxmp.pdf", b"\x01", None, &[]);
        let fp2 = pdf_fingerprint(&p);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(fp2.xmp_document_id.is_none());
        assert!(fp2.pdf_fonts.is_empty());
        assert_eq!(fp2.pdf_id_first.as_deref(), Some("01"));
    }

    #[test]
    fn subset_tag_detection() {
        assert!(is_subset_tagged("ABCDEF+SimSun"));
        assert!(!is_subset_tagged("Arial"));
        assert!(!is_subset_tagged("abcdef+SimSun"), "小写前缀不是子集标签");
        assert!(!is_subset_tagged("ABCDE+SimSun"), "5 字母不是子集标签");
        assert!(!is_subset_tagged("ABCDEF+"), "空字体名不算");
        assert!(!is_subset_tagged("ABCDEFG+X"), "第 7 位必须是 +");
    }

    #[test]
    fn corrupt_and_encrypted_pdf_yield_empty_fingerprint_without_panic() {
        let dir = std::env::temp_dir().join(format!("bg_badpdf_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 损坏：非 PDF 字节 → load 失败 → 空指纹（既有容错语义）
        let bad = dir.join("broken.pdf");
        std::fs::write(&bad, "这不是 PDF 文件，只是伪装了扩展名。").unwrap();
        let fp = pdf_fingerprint(&bad);
        assert!(fp.author.is_none() && fp.pdf_id_first.is_none() && fp.xmp_document_id.is_none());
        assert!(fp.pdf_fonts.is_empty() && fp.font_subset_tags.is_empty());
        // 「加密」：trailer 带 /Encrypt 的文件 → 提取过程绝不 panic（字段能取多少算多少）
        let enc_path = {
            let p = write_lineage_pdf(&dir, "enc_src.pdf", b"\x0f", None, &["ABCDEF+SimSun"]);
            let mut doc = lopdf::Document::load(&p).unwrap();
            let enc_id = doc.add_object(lopdf::dictionary! { "Filter" => "Standard", "V" => 1, "R" => 2 });
            doc.trailer.set("Encrypt", enc_id);
            let out = dir.join("encrypted.pdf");
            doc.save(&out).unwrap();
            out
        };
        let _ = pdf_fingerprint(&enc_path); // 断言不 panic
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_pdfs_same_trailer_id_hit_pdf_lineage_signal() {
        // 验收：两份 trailer /ID 首半相同的 PDF → kind="pdfLineage" 满权重；
        // 仅共享字体子集标签 → 中档；均无 → 无该信号
        use crate::engine::collusion::{assess_with, CollusionInputs};
        use crate::engine::fingerprint::lineage_pairs;
        use crate::engine::report::DocInfo;
        let dir = std::env::temp_dir().join(format!("bg_linsig_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let doc_of = |p: &Path| DocInfo {
            id: "d".into(),
            name: "n".into(),
            doc_type: "pdf".into(),
            pages: 1,
            char_count: 100,
            fingerprint: pdf_fingerprint(p),
            parse_error: None,
            evasion: None,
        };
        // 同一母文件：trailer /ID 首半相同（字体互不相同）
        let a = write_lineage_pdf(&dir, "a.pdf", b"\x99\x88", None, &["AAAAAA+SimSun"]);
        let b = write_lineage_pdf(&dir, "b.pdf", b"\x99\x88", None, &["BBBBBB+SimHei"]);
        let mut hard_docs = vec![doc_of(&a), doc_of(&b)];
        let hits = lineage_pairs(&mut hard_docs);
        let c = assess_with(CollusionInputs { lineage_hits: &hits, ..Default::default() });
        let s = c.signals.iter().find(|s| s.kind == "pdfLineage").expect("应有 pdfLineage 信号");
        let expect_hard = crate::engine::collusion::expected_contribution("pdfLineage", 1.0);
        assert!((s.weight - expect_hard).abs() < 1e-6, "硬命中 x=1 满档，实际 {}", s.weight);
        assert!(s.detail.contains("未命中不代表清白"));
        assert!(hard_docs[0].fingerprint.risk_flags.iter().any(|f| f.contains("同一母文件")));

        // 仅共享字体子集标签：ID 首半不同
        let m1 = write_lineage_pdf(&dir, "m1.pdf", b"\x01", None, &["CCCCCC+KaiTi", "Arial"]);
        let m2 = write_lineage_pdf(&dir, "m2.pdf", b"\x02", None, &["CCCCCC+KaiTi"]);
        let mut mid_docs = vec![doc_of(&m1), doc_of(&m2)];
        let mid_hits = lineage_pairs(&mut mid_docs);
        let cm = assess_with(CollusionInputs { lineage_hits: &mid_hits, ..Default::default() });
        let sm = cm.signals.iter().find(|s| s.kind == "pdfLineage").expect("中命中也应有信号");
        let expect_mid = crate::engine::collusion::expected_contribution("pdfLineage", 0.55);
        assert!((sm.weight - expect_mid).abs() < 1e-6, "仅中命中 x=0.55，实际 {}", sm.weight);

        // 均无命中：无该信号
        let n1 = write_lineage_pdf(&dir, "n1.pdf", b"\x03", None, &["DDDDDD+FangSong"]);
        let n2 = write_lineage_pdf(&dir, "n2.pdf", b"\x04", None, &["EEEEEE+FangSong"]);
        let mut none_docs = vec![doc_of(&n1), doc_of(&n2)];
        let none_hits = lineage_pairs(&mut none_docs);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(none_hits.is_empty());
        let cn = assess_with(CollusionInputs { lineage_hits: &none_hits, ..Default::default() });
        assert!(!cn.signals.iter().any(|s| s.kind == "pdfLineage"));
    }

    fn settings_xml(inner: &str) -> String {
        format!(r#"<?xml version="1.0"?><w:settings xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:zoom w:percent="100"/><w:rsids>{inner}</w:rsids></w:settings>"#)
    }

    #[test]
    fn docx_rsids_template_and_zip_fp_extracted() {
        let dir = std::env::temp_dir().join(format!("bg_rsid_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // rsid 大小写混写 + 重复：应大写归一并去重；rsidRoot 单列且并入集合
        let settings = settings_xml(
            r#"<w:rsidRoot w:val="00ab12cd"/><w:rsid w:val="00ab12cd"/><w:rsid w:val="00FF00aa"/><w:rsid w:val="00ff00aa"/><w:rsid w:val="00123456"/>"#,
        );
        let app = r#"<?xml version="1.0"?><Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>Microsoft Office Word</Application><Template>投标文件模板.dotx</Template><Pages>3</Pages></Properties>"#.to_string();
        let p = write_docx_with_parts(
            &dir,
            "rsid.docx",
            "<w:p><w:r><w:t>本项目采用分层解耦的微服务架构设计方案。</w:t></w:r></w:p>",
            &[("word/settings.xml", settings), ("docProps/app.xml", app)],
        );
        let parsed = parse_file_blocks(Path::new(&p), &no_cancel()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let fp = &parsed.fingerprint;
        assert_eq!(
            fp.rsids,
            vec!["00AB12CD", "00FF00AA", "00123456"],
            "去重 + 大写归一，root 并入集合"
        );
        assert_eq!(fp.rsid_root.as_deref(), Some("00AB12CD"));
        assert_eq!(fp.template_name.as_deref(), Some("投标文件模板.dotx"));
        assert_eq!(parsed.pages, 3);
        let zfp = fp.zip_entry_fp.as_deref().expect("docx 均应有 zip 条目指纹");
        assert_eq!(zfp.len(), 64, "sha256 hex");
        assert_eq!(fp.zip_entry_count, Some(4), "4 个 zip 条目");
    }

    #[test]
    fn docx_without_settings_has_no_rsids_and_no_error() {
        // WPS 等生成的 docx 可能无 settings.xml / rsids 节点：字段留空，解析不报错
        let dir = std::env::temp_dir().join(format!("bg_norsid_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_docx_with_parts(
            &dir,
            "plain.docx",
            "<w:p><w:r><w:t>无修订标识的文档正文内容。</w:t></w:r></w:p>",
            &[],
        );
        let parsed = parse_file_blocks(Path::new(&p), &no_cancel()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(parsed.fingerprint.rsids.is_empty());
        assert!(parsed.fingerprint.rsid_root.is_none());
        assert!(parsed.fingerprint.template_name.is_none());
        assert!(parsed.fingerprint.zip_entry_fp.is_some(), "zip 指纹与 rsids 无关，始终计算");
    }

    #[test]
    fn docx_rsids_capped_at_limit() {
        let dir = std::env::temp_dir().join(format!("bg_rsidcap_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let many: String = (0..(MAX_RSIDS + 50))
            .map(|i| format!(r#"<w:rsid w:val="{i:08X}"/>"#))
            .collect();
        let p = write_docx_with_parts(
            &dir,
            "cap.docx",
            "<w:p><w:r><w:t>大量修订标识的异常文档。</w:t></w:r></w:p>",
            &[("word/settings.xml", settings_xml(&many))],
        );
        let parsed = parse_file_blocks(Path::new(&p), &no_cancel()).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(parsed.fingerprint.rsids.len(), MAX_RSIDS, "超限截断防膨胀");
    }

    #[test]
    fn zip_entry_fp_stable_for_same_pipeline_differs_on_structure() {
        let dir = std::env::temp_dir().join(format!("bg_zfp_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let body_a = "<w:p><w:r><w:t>甲公司的技术方案正文。</w:t></w:r></w:p>";
        let body_b = "<w:p><w:r><w:t>乙公司完全不同的施工组织设计。</w:t></w:r></w:p>";
        let a = write_docx_with_parts(&dir, "a.docx", body_a, &[]);
        let b = write_docx_with_parts(&dir, "b.docx", body_b, &[]);
        // 同一「打包管线」（条目序列一致）：内容不同也应有相同 zip 指纹
        let c = write_docx_with_parts(
            &dir,
            "c.docx",
            body_a,
            &[("word/settings.xml", settings_xml(""))],
        );
        let fp_of = |p: &str| {
            parse_file_blocks(Path::new(p), &no_cancel()).unwrap().fingerprint.zip_entry_fp.unwrap()
        };
        let (fa, fb, fc) = (fp_of(&a), fp_of(&b), fp_of(&c));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(fa, fb, "条目序列相同 → 指纹一致（与正文内容无关）");
        assert_ne!(fa, fc, "多一个条目 → 指纹不同");
    }

    #[test]
    fn docx_image_collection_filters_and_dedups() {
        let dir = std::env::temp_dir().join(format!("bg_img_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let big = solid_png(200, 120, 200);
        let p = write_docx_with_media(
            &dir,
            "img.docx",
            "<w:p><w:r><w:t>正文</w:t></w:r></w:p>",
            &[
                ("image1.png", big.clone()),
                ("image2.png", big.clone()),    // 与 image1 内容相同 → 去重
                ("image3.png", solid_png(40, 40, 0)), // 短边 < 80 → 装饰图剔除
                ("logo.gif", solid_png(10, 10, 0)),    // 小图 → 剔除
            ],
        );
        let mut zip = open_zip(&p);
        let imgs = collect_docx_images(&mut zip, &no_cancel());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(imgs.len(), 1, "去重 + 小图过滤后应只剩 1 张");
        assert_eq!(imgs[0].dimensions(), (200, 120));
    }

    // —— W1-4 内嵌图片同源：位图指纹提取 + dHash 稳定性 ——

    /// 8 条竖带（亮度非单调）的结构化灰度图：dHash 位型有结构（非全 0），大块结构
    /// 在 JPEG 有损压缩后稳定，适合验证「重压缩仍近似」。
    fn banded_img(w: u32, h: u32) -> image::RgbImage {
        let bands = [10u8, 200, 40, 160, 90, 230, 20, 250];
        image::RgbImage::from_fn(w, h, |x, _y| {
            let v = bands[(x * 8 / w).min(7) as usize];
            image::Rgb([v, v, v])
        })
    }

    /// 确定性伪随机噪声图（LCG）：两个不同种子产生视觉无关的图，dHash 应相距很远。
    fn noise_img(w: u32, h: u32, seed: u64) -> image::RgbImage {
        let mut s = seed;
        image::RgbImage::from_fn(w, h, |_x, _y| {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let v = (s >> 33) as u8;
            image::Rgb([v, v, v])
        })
    }

    fn encode(img: &image::RgbImage, fmt: image::ImageFormat) -> Vec<u8> {
        let mut buf = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(img.clone())
            .write_to(&mut buf, fmt)
            .unwrap();
        buf.into_inner()
    }

    fn hamming(a: u64, b: u64) -> u32 {
        (a ^ b).count_ones()
    }

    #[test]
    fn dhash_stable_across_jpeg_recompression_but_far_from_noise() {
        let base = banded_img(200, 160);
        // 同一张图的 PNG（无损）与 JPEG（有损重压）解码后像素不同 → sha256 不同，
        // 但 dHash 汉明距离应 ≤10（视觉同源）
        let png = image::load_from_memory(&encode(&base, image::ImageFormat::Png))
            .unwrap()
            .to_rgb8();
        let jpg = image::load_from_memory(&encode(&base, image::ImageFormat::Jpeg))
            .unwrap()
            .to_rgb8();
        assert_ne!(image_sha256(&png), image_sha256(&jpg), "有损重压后精确指纹应不同");
        assert!(
            hamming(dhash64(&png), dhash64(&jpg)) <= 10,
            "重压缩后 dHash 应仍近似（≤10），实际 {}",
            hamming(dhash64(&png), dhash64(&jpg))
        );
        // 两张无关噪声图 dHash 相距很远（>10）→ 不会被判近似
        let n1 = noise_img(200, 160, 1);
        let n2 = noise_img(200, 160, 999);
        assert!(
            hamming(dhash64(&n1), dhash64(&n2)) > 10,
            "随机噪声图 dHash 应相距很远，实际 {}",
            hamming(dhash64(&n1), dhash64(&n2))
        );
    }

    #[test]
    fn docx_image_hashes_extracted_dedup_and_filtered() {
        let dir = std::env::temp_dir().join(format!("bg_imghash_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = banded_img(200, 160);
        let png = encode(&base, image::ImageFormat::Png);
        let jpg = encode(&base, image::ImageFormat::Jpeg); // 同图重压 → 不同字节/像素
        let p = write_docx_with_media(
            &dir,
            "imghash.docx",
            "<w:p><w:r><w:t>正文</w:t></w:r></w:p>",
            &[
                ("image1.png", png.clone()),
                ("image2.png", png.clone()),          // 同像素 → sha256 去重
                ("image3.jpg", jpg.clone()),          // 同图有损版 → 保留（sha 不同）
                ("image4.png", solid_png(40, 40, 0)), // 短边 <80 → 剔除
            ],
        );
        let mut zip = open_zip(&p);
        let hashes = collect_image_hashes_docx(&mut zip, &no_cancel());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(hashes.len(), 2, "去重 + 小图过滤后应剩 2 张（PNG + JPEG 版）");
        assert!(hashes.iter().all(|h| h.source == "docx" && h.page.is_none()));
        assert!(hashes.iter().all(|h| h.dhash.is_some()), "docx 图恒有 dHash");
        assert!(
            hamming(hashes[0].dhash.unwrap(), hashes[1].dhash.unwrap()) <= 10,
            "同图 PNG/JPEG 两版 dHash 应近似"
        );
    }

    #[test]
    fn pdf_image_object_extracted_with_page_and_dhash() {
        // pdfium 造一页含内嵌图（非整页）的 PDF → collect_image_hashes_pdf 应取到 1 张 pdf 图，
        // 带页码、非整页故有 dHash。当前平台无 libpdfium 时优雅跳过（与其他 pdfium 测试同）。
        let Some(pdfium) = bind_pdfium() else {
            eprintln!("跳过 pdf 图片提取测试：当前平台无可用 libpdfium");
            return;
        };
        let dir = std::env::temp_dir().join(format!("bg_pdfimg_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let dynimg = image::DynamicImage::ImageRgb8(banded_img(300, 240));
        let bytes = {
            let mut doc = pdfium.create_new_pdf().unwrap();
            {
                let mut page = doc
                    .pages_mut()
                    .create_page_at_index(PdfPagePaperSize::a4(), 0)
                    .unwrap();
                // A4≈595×842pt；图 200×160pt 置于 (50,50) → 占面比 ≈0.06 «0.8，非整页
                page.objects_mut()
                    .create_image_object(
                        PdfPoints::new(50.0),
                        PdfPoints::new(50.0),
                        &dynimg,
                        Some(PdfPoints::new(200.0)),
                        Some(PdfPoints::new(160.0)),
                    )
                    .unwrap();
            }
            doc.save_to_bytes().unwrap()
        };
        // pdfium 的线程安全 marshall 锁不可重入：造夹具用的 Pdfium 必须先释放，
        // 否则 collect_image_hashes_pdf 内的第二次 Pdfium::new 会死锁。
        // 生产路径同理——parse_pdf 先 drop 掉 Pdfium 再调本函数，故不受影响。
        drop(pdfium);
        let p = dir.join("img.pdf");
        std::fs::write(&p, bytes).unwrap();
        let hashes = collect_image_hashes_pdf(&p, &no_cancel());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(hashes.len(), 1, "应提取到 1 张 PDF 内嵌图");
        assert_eq!(hashes[0].source, "pdf");
        assert_eq!(hashes[0].page, Some(1), "页码 1 起");
        assert!(hashes[0].dhash.is_some(), "非整页图应有 dHash");
        assert!(hashes[0].width >= MIN_IMAGE_HASH_PX && hashes[0].height >= MIN_IMAGE_HASH_PX);
    }

    #[test]
    fn docx_without_images_is_zero_cost_and_unchanged() {
        let dir = std::env::temp_dir().join(format!("bg_noimg_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 无图片的 docx：开启 ocr_docx_images 不应改变结果、不应崩
        let body = "<w:p><w:r><w:t>本项目采用分层解耦的微服务架构设计方案。</w:t></w:r></w:p>";
        let p = write_docx_with_media(&dir, "plain.docx", body, &[]);
        let m = crate::engine::ocr::resolve("v6-small");
        let off = parse_file_blocks_opt(Path::new(&p), &no_cancel(), false, m, false).unwrap();
        let on = parse_file_blocks_opt(Path::new(&p), &no_cancel(), true, m, false).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(off.blocks.len(), on.blocks.len(), "无图片时开关不影响块数");
        assert!(on.blocks.iter().any(|b| b.text.contains("微服务架构")));
    }

    #[test]
    #[ignore] // 需 pdfium + OCR 模型：cargo test docx_image_ocr -- --ignored
    fn docx_embedded_image_text_is_ocr_recognized() {
        // 把样例 PDF 首页栅格成 PNG 贴进 docx → 开 OCR → 应识别出已知英文词
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        let imgs = match rasterize_pdf(&fixture, &no_cancel()) {
            Some((v, _)) if !v.is_empty() => v,
            _ => return, // pdfium 不可用则跳过
        };
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgb8(imgs.into_iter().next().unwrap())
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        let dir = std::env::temp_dir().join(format!("bg_imgocr_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_docx_with_media(
            &dir,
            "scan.docx",
            "<w:p><w:r><w:t>下表为截图。</w:t></w:r></w:p>",
            &[("page1.png", png.into_inner())],
        );
        let pb = parse_file_blocks_opt(
            Path::new(&p),
            &no_cancel(),
            true,
            crate::engine::ocr::resolve("v6-small"),
            false,
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        let joined = pb.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n").to_lowercase();
        assert!(
            joined.contains("bidguard") || joined.contains("gateway"),
            "图片内文字应被 OCR 识别进块，实际：{joined:?}"
        );
        // 文字块（正文）也仍在
        assert!(pb.blocks.iter().any(|b| b.text.contains("下表为截图")));
    }

    #[test]
    fn parses_xlsx_rows_as_table_blocks() {
        let dir = std::env::temp_dir().join(format!("bg_xlsx_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = write_min_xlsx(&dir, "报价.xlsx");
        let pb = parse_file_blocks(&p, &no_cancel()).expect("应能解析 xlsx");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(pb.method, "xlsx");
        // 工作表名作一级标题
        assert!(pb.blocks.iter().any(|b| b.heading_level == Some(1) && b.text == "报价清单"));
        let rows: Vec<_> = pb.blocks.iter().filter(|b| b.is_table_row).collect();
        assert_eq!(rows.len(), 3, "表头 + 两行数据");
        assert_eq!(rows[0].text, "序号 | 设备名称 | 单价");
        // 浮点整数不带 .0；真小数保留
        assert_eq!(rows[1].text, "1 | 核心交换机 | 64000");
        assert_eq!(rows[2].text, "2 | 万兆光模块 | 3500.5");
        assert!(rows.iter().all(|b| b.page == Some(1)), "页码=工作表序号");
        assert!(pb.legacy_text.contains("核心交换机"));
    }

    #[test]
    fn docx_numbered_paragraphs_marked_as_list_items() {
        let xml = r#"<w:document xmlns:w="urn:x"><w:body>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>提供原厂三年质保服务</w:t></w:r></w:p>
<w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>提供七乘二十四小时响应</w:t></w:r></w:p>
<w:p><w:r><w:t>以上承诺自合同签订之日起生效。</w:t></w:r></w:p>
</w:body></w:document>"#;
        let (blocks, _, _) = docx_blocks(xml.as_bytes());
        assert_eq!(blocks.len(), 3);
        assert!(blocks[0].is_list_item && blocks[1].is_list_item, "numPr 段应标记列表项");
        assert!(!blocks[2].is_list_item, "普通段不标记");
    }

    #[test]
    fn docx_tables_emit_row_blocks() {
        // 报价表两行 + 嵌套表格不丢字 + 表格前后普通段落不受影响
        let xml = r#"<w:document xmlns:w="urn:x"><w:body>
<w:p><w:r><w:t>报价明细如下：</w:t></w:r></w:p>
<w:tbl>
  <w:tr><w:tc><w:p><w:r><w:t>序号</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>设备名称</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>单价</w:t></w:r></w:p></w:tc></w:tr>
  <w:tr><w:tc><w:p><w:r><w:t>1</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>核心交换机</w:t></w:r></w:p><w:p><w:r><w:t>含安装调试</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>64000元</w:t></w:r></w:p></w:tc></w:tr>
  <w:tr><w:tc><w:p><w:r><w:t>2</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>内含表 </w:t></w:r></w:p><w:tbl><w:tr><w:tc><w:p><w:r><w:t>嵌套内容</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:tc><w:tc><w:p><w:r><w:t>100元</w:t></w:r></w:p></w:tc></w:tr>
</w:tbl>
<w:p><w:r><w:t>以上报价含税。</w:t></w:r></w:p>
</w:body></w:document>"#;
        let (blocks, legacy, _) = docx_blocks(xml.as_bytes());
        let rows: Vec<_> = blocks.iter().filter(|b| b.is_table_row).collect();
        assert_eq!(rows.len(), 3, "三行表格 → 三个行块（嵌套表并入外层单元格）");
        assert_eq!(rows[0].text, "序号 | 设备名称 | 单价");
        // 单元格内多段落以空格连接
        assert_eq!(rows[1].text, "1 | 核心交换机 含安装调试 | 64000元");
        // 嵌套表格文本并入外层单元格，不产出独立行块
        assert_eq!(rows[2].text, "2 | 内含表 嵌套内容 | 100元");
        // 表格前后普通段落正常
        let paras: Vec<_> = blocks.iter().filter(|b| !b.is_table_row).collect();
        assert_eq!(paras.len(), 2);
        assert_eq!(paras[0].text, "报价明细如下：");
        // legacy 全文仍含每个 w:p 的文本（含表格内段落）
        assert!(legacy.contains("核心交换机") && legacy.contains("嵌套内容"));
    }

    #[test]
    fn docx_legacy_text_keeps_empty_paragraph_newlines() {
        // 空段落在旧 docx_text 里贡献一个换行：legacy 必须保留，blocks 则过滤
        let xml = r#"<w:document xmlns:w="urn:x"><w:body>
<w:p><w:r><w:t>第一段</w:t></w:r></w:p>
<w:p></w:p>
<w:p><w:r><w:t>第三段</w:t></w:r></w:p>
</w:body></w:document>"#;
        let (blocks, legacy, _) = docx_blocks(xml.as_bytes());
        assert_eq!(blocks.len(), 2, "空段落不产出块");
        assert_eq!(legacy, "第一段\n\n第三段\n", "legacy 保留空段落换行");
    }

    #[test]
    fn heading_style_id_variants() {
        assert_eq!(heading_level_of_style("Heading1"), Some(1));
        assert_eq!(heading_level_of_style("heading 3"), Some(3));
        assert_eq!(heading_level_of_style("2"), Some(2));
        assert_eq!(heading_level_of_style("af0"), None);
        assert_eq!(heading_level_of_style("Heading10"), None);
        assert_eq!(heading_level_of_style("正文"), None);
    }

    #[test]
    fn cancelled_rasterize_returns_none() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        let cancelled = AtomicBool::new(true);
        assert!(rasterize_pdf(&fixture, &cancelled).is_none(), "已取消应立即返回");
    }

    #[test]
    #[ignore] // 加载 OCR 模型 + 推理，较慢；`cargo test ocr -- --ignored` 验证
    fn ocr_reads_rasterized_pdf() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        let (imgs, _) = rasterize_pdf(&fixture, &no_cancel()).expect("应能栅格化 PDF");
        assert!(!imgs.is_empty(), "应渲染出至少一页");
        let pages = crate::engine::ocr::ocr_images(imgs, &no_cancel(), crate::engine::ocr::resolve("v6-small"))
            .expect("OCR 应可用（模型在 src-tauri/models）");
        let text = pages.iter().map(|p| p.text.as_str()).collect::<Vec<_>>().join("\n").to_lowercase();
        assert!(
            text.contains("bidguard") || text.contains("gateway"),
            "OCR 应识别出已知词，实际：{text:?}"
        );
        // 行级版面应随文本一起产出（文本层数据源）
        assert!(
            pages.iter().any(|p| !p.lines.is_empty()),
            "应有带坐标的识别行"
        );
        let l = pages.iter().flat_map(|p| &p.lines).next().unwrap();
        assert!((0.0..=1.0).contains(&l.x) && (0.0..=1.0).contains(&l.y), "坐标应已归一化");
    }

    /// pdfium 造一个 N 页 PDF（各页 A4 空白，保证可渲染）；返回文件路径。造夹具的 Pdfium
    /// 必须在返回前释放（marshall 锁不可重入，否则后续 bind_pdfium 死锁）。
    fn write_multipage_pdf(dir: &Path, name: &str, n: usize) -> Option<PathBuf> {
        let pdfium = bind_pdfium()?;
        let bytes = {
            let mut doc = pdfium.create_new_pdf().unwrap();
            for i in 0..n {
                doc.pages_mut()
                    .create_page_at_index(PdfPagePaperSize::a4(), i as u16)
                    .unwrap();
            }
            doc.save_to_bytes().unwrap()
        };
        drop(pdfium);
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        Some(p)
    }

    #[test]
    fn rasterize_cap_lift_and_sampled_pages() {
        // W2-4「回落解除 OCR 页上限」的底层保证：max_pages 参数化 + 按索引抽样渲染。
        // 无 libpdfium 环境优雅跳过（与其他 pdfium 测试同），不联网、不加载 OCR 模型。
        let dir = std::env::temp_dir().join(format!("bg_capm_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let Some(p) = write_multipage_pdf(&dir, "multi.pdf", 4) else {
            eprintln!("跳过：当前平台无可用 libpdfium");
            let _ = std::fs::remove_dir_all(&dir);
            return;
        };
        // 上限 2：仅渲染前 2 页，但如实上报总页数 4
        let (capped, total) = rasterize_pdf_capped(&p, &no_cancel(), Some(2)).expect("应能栅格化");
        assert_eq!(total, 4, "总页数如实上报");
        assert_eq!(capped.len(), 2, "上限 2 只渲染前 2 页");
        // 解除上限（None）：渲染全部 4 页——这正是命中回落时的行为
        let (uncapped, total2) = rasterize_pdf_capped(&p, &no_cancel(), None).expect("应能栅格化");
        assert_eq!(total2, 4);
        assert_eq!(uncapped.len(), 4, "解除上限渲染全部页（超 20 页文档后部亦参与）");
        // 按索引抽样：只渲染指定页，越界索引跳过
        let sampled = rasterize_pages(&p, &[0, 3], &no_cancel()).expect("应能抽样渲染");
        assert_eq!(sampled.len(), 2, "抽样 2 页");
        let with_oob = rasterize_pages(&p, &[0, 99], &no_cancel()).expect("越界不 panic");
        assert_eq!(with_oob.len(), 1, "越界索引跳过，仅渲染有效页");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cross_check_disabled_records_skipped_without_ocr() {
        // pdf_cross_check=false：不跑渲染-OCR（零耗时、不联网），xcheck 记 skipped（非清白背书）。
        // 走 pdfium 或 pdf-extract 任一文字层路径都应命中此分支（sample.pdf 是正常文字版）。
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        let m = crate::engine::ocr::resolve("v6-small");
        let pb = parse_file_blocks_opt(&fixture, &no_cancel(), false, m, false).expect("应能解析");
        assert!(matches!(pb.method, "pdfium" | "pdf-extract"), "文字版应走文字层路径，实际 {}", pb.method);
        let xr = pb.xcheck.expect("关闭时也应记 xcheck（skipped）");
        assert!(xr.skipped.is_some(), "关闭应记 skipped 原因");
        assert!(!xr.is_hit(), "关闭不产生命中");
        assert!(pb.method != "ocr-fallback", "关闭不回落");
    }

    #[test]
    #[ignore] // 需 pdfium + OCR 模型：`cargo test cross_check -- --ignored`
    fn cross_check_normal_pdf_does_not_fallback() {
        // 验收 3：正常文字版 sample.pdf 开启交叉验证 → 中位失配低、不回落。
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        let m = crate::engine::ocr::resolve("v6-small");
        let pb = parse_file_blocks_opt(&fixture, &no_cancel(), false, m, true).expect("应能解析");
        assert_ne!(pb.method, "ocr-fallback", "正常文档不应回落 OCR");
        let xr = pb.xcheck.expect("开启应有 xcheck 结果");
        if xr.skipped.is_none() {
            assert!(xr.verdict.is_none(), "正常文档不命中，实际 {:?}", xr.verdict);
            assert!(xr.median_mismatch < 0.35, "中位失配应 <0.35，实际 {}", xr.median_mismatch);
        }
    }

    #[test]
    #[ignore] // 需 pdfium + OCR 模型：`cargo test cross_check -- --ignored`
    fn cross_check_imaged_text_with_garbage_layer_falls_back() {
        // 验收 1：图片化正文（渲染=真实文字图）+ 垃圾隐藏文字层 → 抽取得垃圾、OCR 读真文字 →
        // 中位失配 >0.35 → method=ocr-fallback、verdict 就位、evasion 提示写入。
        // 构造：把 sample.pdf 首页栅格成整页图贴进新 PDF，另加一层 Tr=3 隐藏垃圾文本。
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        let Some(pdfium) = bind_pdfium() else { return };
        let (imgs, _) = match rasterize_pdf_capped(&fixture, &no_cancel(), Some(1)) {
            Some(v) if !v.0.is_empty() => v,
            _ => return,
        };
        let dynimg = image::DynamicImage::ImageRgb8(imgs.into_iter().next().unwrap());
        let dir = std::env::temp_dir().join(format!("bg_xchk_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let bytes = {
            let mut doc = pdfium.create_new_pdf().unwrap();
            {
                let mut page =
                    doc.pages_mut().create_page_at_index(PdfPagePaperSize::a4(), 0).unwrap();
                let (pw, ph) = (page.width(), page.height());
                page.objects_mut()
                    .create_image_object(PdfPoints::new(0.0), PdfPoints::new(0.0), &dynimg, Some(pw), Some(ph))
                    .unwrap();
            }
            doc.save_to_bytes().unwrap()
        };
        drop(pdfium);
        // 注入 Tr=3 垃圾文字层（lopdf 追加内容流）：pdfium 抽取会读到垃圾串
        let garbage = "Xq7zKp9wRt2vBn4mLl8aQs3dWf6gHj0cZx5yVb1nMk7lOp4iUr2eYt9wAc6s".repeat(20);
        let p = dir.join("imaged.pdf");
        std::fs::write(&p, bytes).unwrap();
        inject_hidden_text(&p, &garbage);

        let m = crate::engine::ocr::resolve("v6-small");
        let pb = parse_file_blocks_opt(&p, &no_cancel(), false, m, true).expect("应能解析");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(pb.method, "ocr-fallback", "应回落 OCR，实际 {}", pb.method);
        let xr = pb.xcheck.expect("应有 xcheck");
        assert!(xr.is_hit(), "应命中");
        assert!(xr.median_mismatch > 0.35, "中位失配 {} 应 >0.35", xr.median_mismatch);
        assert!(pb.truncation_notice.as_deref().unwrap_or("").contains("OCR"), "应有回落提示");
    }

    /// 测试辅助：用 lopdf 给单页 PDF 追加一段 Tr=3 隐藏文本内容流（模拟垃圾文字层）。
    fn inject_hidden_text(path: &Path, text: &str) {
        use lopdf::content::{Content, Operation};
        use lopdf::{Object, Stream};
        let mut doc = lopdf::Document::load(path).unwrap();
        let pages: Vec<_> = doc.get_pages().into_iter().collect();
        let (_, page_id) = pages[0];
        // 需要一个字体资源；直接在内容里用未定义字体名，pdfium 抽取仍读 Tj 串
        let content = Content {
            operations: vec![
                Operation::new("BT", vec![]),
                Operation::new("Tf", vec!["F1".into(), 8.into()]),
                Operation::new("Tr", vec![3.into()]),
                Operation::new("Td", vec![72.into(), 700.into()]),
                Operation::new("Tj", vec![Object::string_literal(text)]),
                Operation::new("ET", vec![]),
            ],
        };
        let cid = doc.add_object(Stream::new(lopdf::dictionary! {}, content.encode().unwrap()));
        // 追加到页面 Contents（数组化）
        if let Ok(page_dict) = doc.get_dictionary_mut(page_id) {
            let existing = page_dict.get(b"Contents").ok().cloned();
            let new_contents = match existing {
                Some(Object::Array(mut a)) => {
                    a.push(cid.into());
                    Object::Array(a)
                }
                Some(other) => Object::Array(vec![other, cid.into()]),
                None => Object::Array(vec![cid.into()]),
            };
            page_dict.set("Contents", new_contents);
        }
        doc.save(path).unwrap();
    }
}
