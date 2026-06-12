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
}

/// 新 API：结构化段块 + 取消旗标（OCR/栅格化等长阶段逐页检查）。
/// 取消时尽快返回 Err；调用方应先自查旗标再决定如何归类该错误。
pub fn parse_file_blocks(path: &Path, cancel: &AtomicBool) -> Result<ParsedBlocks, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "docx" => parse_docx(path),
        "txt" | "md" => parse_txt(path),
        "pdf" => parse_pdf(path, cancel),
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
            .filter(|l| !l.trim().is_empty())
            .next_back()
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

/// 解码文本：优先 UTF-8（含 BOM），无效时回落 GB18030（覆盖 GBK/GB2312）。
fn decode_text(bytes: &[u8]) -> String {
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

fn parse_pdf(path: &Path, cancel: &AtomicBool) -> Result<ParsedBlocks, String> {
    // 1) pdfium 文本（最鲁棒）；2) pdf-extract 回落；3) 扫描件 → OCR
    if let Some(pd) = parse_pdf_pdfium(path, cancel) {
        return Ok(pd);
    }
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.into());
    }
    if let Ok(pd) = parse_pdf_extract(path) {
        return Ok(pd);
    }
    parse_pdf_ocr(path, cancel)
}

/// 扫描件路径：pdfium 栅格化每页 → oar-ocr 识别 → 按页拼接文本 + 行级版面。
fn parse_pdf_ocr(path: &Path, cancel: &AtomicBool) -> Result<ParsedBlocks, String> {
    let imgs =
        rasterize_pdf(path, cancel).ok_or_else(|| "无法栅格化 PDF（pdfium 不可用）".to_string())?;
    if imgs.is_empty() {
        return Err("PDF 无可渲染页面".into());
    }
    let pages = imgs.len() as u32;
    let ocr_pages = crate::engine::ocr::ocr_images(imgs, cancel)
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
    Ok(ParsedBlocks {
        blocks,
        pages,
        fingerprint: pdf_fingerprint(path),
        method: "ocr",
        legacy_text,
        ocr_layout_json,
    })
}

/// 用 pdfium 把 PDF 各页渲染为 RgbImage（手动 BGRA→RGB，避开 image 特性耦合）。
fn rasterize_pdf(path: &Path, cancel: &AtomicBool) -> Option<Vec<image::RgbImage>> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium.load_pdf_from_file(path.to_str()?, None).ok()?;
    let cfg = PdfRenderConfig::new()
        .set_target_width(1600)
        .set_maximum_height(2400);
    let mut imgs = Vec::new();
    for page in doc.pages().iter() {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let bm = match page.render_with_config(&cfg) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let w = bm.width() as u32;
        let h = bm.height() as u32;
        let raw = bm.as_raw_bytes();
        let need = (w as usize) * (h as usize) * 4;
        if w == 0 || h == 0 || raw.len() < need {
            continue;
        }
        let mut rgb = image::RgbImage::new(w, h);
        for (i, px) in rgb.pixels_mut().enumerate() {
            let o = i * 4;
            *px = image::Rgb([raw[o + 2], raw[o + 1], raw[o]]); // BGRA → RGB
        }
        imgs.push(rgb);
        if imgs.len() >= 20 {
            break; // 限制扫描页数，控制耗时
        }
    }
    Some(imgs)
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

/// 读 PDF Info 字典作为元数据指纹（作者/Producer/创建/修改时间）。
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
    fp
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

fn parse_docx(path: &Path) -> Result<ParsedBlocks, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("非法 docx (zip): {e}"))?;

    let doc_xml = read_zip(&mut zip, "word/document.xml")
        .ok_or_else(|| "docx 缺少 word/document.xml".to_string())?;
    let (blocks, legacy_text) = docx_blocks(&doc_xml);

    let mut fp = Fingerprint::default();
    if let Some(core) = read_zip(&mut zip, "docProps/core.xml") {
        fill_core(&core, &mut fp);
    }
    let mut pages = 0u32;
    if let Some(app) = read_zip(&mut zip, "docProps/app.xml") {
        pages = fill_app(&app, &mut fp);
    }
    if pages == 0 {
        pages = ((legacy_text.chars().count() / 1500) as u32).max(1);
    }

    Ok(ParsedBlocks {
        blocks,
        pages,
        fingerprint: fp,
        method: "docx",
        legacy_text,
        ocr_layout_json: None,
    })
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
fn docx_blocks(xml: &[u8]) -> (Vec<Block>, String) {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut blocks: Vec<Block> = Vec::new();
    let mut legacy = String::new();
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
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    (blocks, legacy)
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

/// 解析 docProps/app.xml：应用、总编辑时长、页数。返回页数（0 表示未知）。
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
    fn docx_blocks_extract_heading_levels() {
        // 英文样式 id / 中文数字样式 id / outlineLvl 三种来源
        let xml = r#"<?xml version="1.0"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body>
<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>第一章 项目概述</w:t></w:r></w:p>
<w:p><w:pPr><w:pStyle w:val="2"/></w:pPr><w:r><w:t>1.1 建设目标</w:t></w:r></w:p>
<w:p><w:pPr><w:outlineLvl w:val="2"/></w:pPr><w:r><w:t>1.1.1 总体要求</w:t></w:r></w:p>
<w:p><w:r><w:t>本项目采用微服务架构。</w:t></w:r></w:p>
</w:body></w:document>"#;
        let (blocks, legacy) = docx_blocks(xml.as_bytes());
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
        let (blocks, _) = docx_blocks(xml.as_bytes());
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
        let (blocks, legacy) = docx_blocks(xml.as_bytes());
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
        let (blocks, legacy) = docx_blocks(xml.as_bytes());
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
        let imgs = rasterize_pdf(&fixture, &no_cancel()).expect("应能栅格化 PDF");
        assert!(!imgs.is_empty(), "应渲染出至少一页");
        let pages = crate::engine::ocr::ocr_images(imgs, &no_cancel())
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
}
