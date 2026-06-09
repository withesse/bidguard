// 文档解析：抽取正文 + 元数据指纹。
// docx(zip+XML) / txt·md(UTF-8 或 GBK) / PDF(pdfium → pdf-extract → OCR 三级回落)。
use crate::engine::report::Fingerprint;
use pdfium_render::prelude::*;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct ParsedDoc {
    pub text: String,
    pub pages: u32,
    pub fingerprint: Fingerprint,
}

pub fn parse_file(path: &Path) -> Result<ParsedDoc, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "docx" => parse_docx(path),
        "txt" | "md" => parse_txt(path),
        "pdf" => parse_pdf(path),
        other => Err(format!("暂不支持的文件类型: .{other}")),
    }
}

fn parse_txt(path: &Path) -> Result<ParsedDoc, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let text = decode_text(&bytes);
    let pages = ((text.chars().count() / 1500) as u32).max(1);
    Ok(ParsedDoc {
        text,
        pages,
        fingerprint: Fingerprint::default(),
    })
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

fn parse_pdf(path: &Path) -> Result<ParsedDoc, String> {
    // 1) pdfium 文本（最鲁棒）；2) pdf-extract 回落；3) 扫描件 → OCR
    if let Some(pd) = parse_pdf_pdfium(path) {
        return Ok(pd);
    }
    if let Ok(pd) = parse_pdf_extract(path) {
        return Ok(pd);
    }
    parse_pdf_ocr(path)
}

/// 扫描件路径：pdfium 栅格化每页 → oar-ocr 识别 → 拼接文本。
fn parse_pdf_ocr(path: &Path) -> Result<ParsedDoc, String> {
    let imgs = rasterize_pdf(path).ok_or_else(|| "无法栅格化 PDF（pdfium 不可用）".to_string())?;
    if imgs.is_empty() {
        return Err("PDF 无可渲染页面".into());
    }
    let pages = imgs.len() as u32;
    let text = crate::engine::ocr::ocr_images(imgs)
        .ok_or_else(|| "OCR 不可用（缺模型或识别失败）".to_string())?;
    if text.trim().is_empty() {
        return Err("OCR 未识别出文本".into());
    }
    Ok(ParsedDoc {
        text,
        pages,
        fingerprint: pdf_fingerprint(path),
    })
}

/// 用 pdfium 把 PDF 各页渲染为 RgbImage（手动 BGRA→RGB，避开 image 特性耦合）。
fn rasterize_pdf(path: &Path) -> Option<Vec<image::RgbImage>> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium.load_pdf_from_file(path.to_str()?, None).ok()?;
    let cfg = PdfRenderConfig::new()
        .set_target_width(1600)
        .set_maximum_height(2400);
    let mut imgs = Vec::new();
    for page in doc.pages().iter() {
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

fn parse_pdf_extract(path: &Path) -> Result<ParsedDoc, String> {
    let text = pdf_extract::extract_text(path).map_err(|e| format!("PDF 解析失败：{e}"))?;
    if text.trim().is_empty() {
        // 无可提取文本：多半是扫描件（图片），需 OCR。
        return Err("PDF 无可提取文本（疑似扫描件，需 OCR）".into());
    }
    let pages = ((text.chars().count() / 1500) as u32).max(1);
    Ok(ParsedDoc {
        text,
        pages,
        fingerprint: pdf_fingerprint(path),
    })
}

/// 用 pdfium 抽取文本（逐页）。绑定失败或无文本返回 None。
fn parse_pdf_pdfium(path: &Path) -> Option<ParsedDoc> {
    let pdfium = bind_pdfium()?;
    let doc = pdfium.load_pdf_from_file(path.to_str()?, None).ok()?;
    let mut text = String::new();
    let mut pages = 0u32;
    for page in doc.pages().iter() {
        if let Ok(t) = page.text() {
            text.push_str(t.all().trim());
            text.push('\n');
        }
        pages += 1;
    }
    if text.trim().is_empty() {
        return None; // 扫描件 → 回落 / 后续 OCR
    }
    Some(ParsedDoc {
        text,
        pages: pages.max(1),
        fingerprint: pdf_fingerprint(path),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pdfium_binds_and_extracts() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        // 绑定 pdfium 并抽取；当前平台无对应原生库时优雅跳过（如 Linux CI）。
        let Some(pd) = parse_pdf_pdfium(&fixture) else {
            eprintln!("跳过 pdfium 测试：当前平台无可用 libpdfium");
            return;
        };
        let lower = pd.text.to_lowercase();
        assert!(
            lower.contains("bidguard") || lower.contains("gateway"),
            "pdfium 抽取文本应含已知词，实际：{:?}",
            pd.text
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
    #[ignore] // 加载 OCR 模型 + 推理，较慢；`cargo test ocr -- --ignored` 验证
    fn ocr_reads_rasterized_pdf() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        if !fixture.exists() {
            return;
        }
        let imgs = rasterize_pdf(&fixture).expect("应能栅格化 PDF");
        assert!(!imgs.is_empty(), "应渲染出至少一页");
        let text = crate::engine::ocr::ocr_images(imgs).expect("OCR 应可用（模型在 src-tauri/models）");
        let lower = text.to_lowercase();
        assert!(
            lower.contains("bidguard") || lower.contains("gateway"),
            "OCR 应识别出已知词，实际：{text:?}"
        );
    }
}

fn parse_docx(path: &Path) -> Result<ParsedDoc, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("非法 docx (zip): {e}"))?;

    let doc_xml = read_zip(&mut zip, "word/document.xml")
        .ok_or_else(|| "docx 缺少 word/document.xml".to_string())?;
    let text = docx_text(&doc_xml);

    let mut fp = Fingerprint::default();
    if let Some(core) = read_zip(&mut zip, "docProps/core.xml") {
        fill_core(&core, &mut fp);
    }
    let mut pages = 0u32;
    if let Some(app) = read_zip(&mut zip, "docProps/app.xml") {
        pages = fill_app(&app, &mut fp);
    }
    if pages == 0 {
        pages = ((text.chars().count() / 1500) as u32).max(1);
    }

    Ok(ParsedDoc {
        text,
        pages,
        fingerprint: fp,
    })
}

fn read_zip<R: Read + std::io::Seek>(zip: &mut zip::ZipArchive<R>, name: &str) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// 提取 word/document.xml 中所有 <w:t> 文本，按段落 <w:p> 换行。
fn docx_text(xml: &[u8]) -> String {
    let mut reader = Reader::from_reader(xml);
    let mut buf = Vec::new();
    let mut out = String::new();
    let mut in_t = false;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                if e.local_name().into_inner() == b"t" {
                    in_t = true;
                }
            }
            Ok(Event::End(e)) => {
                let ln = e.local_name();
                let n = ln.into_inner();
                if n == b"t" {
                    in_t = false;
                } else if n == b"p" {
                    out.push('\n');
                }
            }
            Ok(Event::Text(t)) => {
                if in_t {
                    if let Ok(s) = t.unescape() {
                        out.push_str(&s);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
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
