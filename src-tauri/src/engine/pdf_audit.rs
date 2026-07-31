// PDF 隐藏文字层内容流审计（W2-3）：逐页 Content::decode 走一个小型图形状态机，
// 跟踪文本渲染模式(Tr)、字号(Tf)、文本矩阵(Tm/Td/TD/T*)、CTM(cm/q/Q)、填充色(g/rg/k/sc/scn)
// 与页面 MediaBox；对每个展示串(Tj/TJ/'/")按当前状态归类：Tr=3 不可见 / 填充亮度≥0.97 白字 /
// 文本原点出 MediaBox / 有效字号<1pt。攻击者可注入两套内容（可见给评标人、隐藏污染查重，
// 或反向把雷同正文藏进不可见层），内容流级审计把"可见 vs 抽取"差集定位到具体页。
//
// 防误报关键：识别 "OCR 双层页" 合法模式（整页图像 XObject + 全页隐藏文本 = 扫描件 OCR 层）
// 单独归入 ocr_layer_pages 不计规避；只有"同页可见文本与隐藏文本共存"或"隐藏文本页无整页
// 图像"才算注入嫌疑。产出为线索级证据（白字判定不知真实背景、CID 字符数按展示串字节近似），
// 呈现措辞必须是"检测到疑似规避特征，请人工复核"，绝不下"规避/串通"结论。
//
// 容错语义与 pdf_fingerprint 一致：损坏/加密 PDF 解析失败 → audit 返回 None 静默降级，
// 绝不 panic、绝不阻塞导入。上限 500 页、单页内容流 10MB，超限记 partial。
use lopdf::{Dictionary, Document, Object, ObjectId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// 解析审计 schema 版本：并入 ImportOptions::options_hash 的 pav 键。pdfAudit 与 W2-4 的
/// xcheck（渲染-OCR 交叉验证）都是解析期新产出的数据，cache-hit 的旧文档不会有它——bump
/// 此值让 options_hash 变化、旧缓存整体失效重建（做法对齐 report::FINGERPRINT_SCHEMA_VERSION，
/// 只改 VALUE 不动 v6 前缀）。
/// v1→v2：新增渲染-OCR 抽样交叉验证（W2-4），evasion_json 增 xcheck 子对象。
pub const PDF_AUDIT_SCHEMA_VERSION: u32 = 2;

/// 审计上限：超过则只处理前 MAX_PAGES 页并记 partial（其余页未审计）。
const MAX_PAGES: usize = 500;
/// 单页内容流字节上限：超过则跳过该页并记 partial（避免解码超大流拖垮导入）。
const MAX_STREAM_BYTES: usize = 10 * 1024 * 1024;
/// 白字亮度阈值：填充色相对亮度 ≥ 此值视为白字（线索级，深色底设计稿可能误报）。
const WHITE_LUM_THRESHOLD: f64 = 0.97;
/// 极小字号阈值（pt）：有效字号（Tf × 矩阵缩放）< 此值视为不可读。
const TINY_SIZE_PT: f64 = 1.0;
/// 出画布容差（pt）：文本原点越出 MediaBox 超过此容差才计出画布（避免贴边正文误报）。
const OFF_CANVAS_MARGIN: f64 = 3.0;
/// 整页图像判定：Do 绘制的图像 XObject 经 CTM 变换后覆盖面积 ≥ 页面积此比例即视为整页图。
const FULL_PAGE_IMAGE_RATIO: f64 = 0.8;

/// 单页命中：页码（1 起）+ 该页计入规避嫌疑的隐藏字符数。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PdfHiddenPage {
    pub page: u32,
    pub hidden_chars: u64,
}

/// 文档级 PDF 隐藏文字层审计结果（写 documents.evasion_json 的 pdfAudit 子对象）。
/// 各计数按展示串字节近似（CID/双字节字体会偏大，但对占比无偏），四类可重叠，
/// hidden_chars 是"任一类命中"的去重并集。ocr_layer_pages 归入合法扫描层不计规避。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PdfHiddenStats {
    /// Tr=3/7 不可见渲染模式展示的字符数
    pub tr_invisible_chars: u64,
    /// 填充亮度 ≥0.97 的白字字符数
    pub white_text_chars: u64,
    /// 文本原点落在 MediaBox 外的字符数
    pub off_canvas_chars: u64,
    /// 有效字号 <1pt 的极小字号字符数
    pub tiny_font_chars: u64,
    /// 计入规避嫌疑的隐藏字符数（四类去重并集，排除 OCR 双层页）
    pub hidden_chars: u64,
    /// 审计到的展示字符总数（排除 OCR 双层页），供 hidden_ratio 分母
    pub total_chars: u64,
    /// 隐藏占比 hidden_chars/total_chars（total 为 0 时为 0）
    pub hidden_ratio: f64,
    /// 逐页命中（仅注入嫌疑页）
    pub hit_pages: Vec<PdfHiddenPage>,
    /// OCR 双层页页码（整页图像 + 全页隐藏文本 = 合法扫描 OCR 层，不计规避）
    pub ocr_layer_pages: Vec<u32>,
    /// 超限降级：页数>500 或某页内容流>10MB 时置 true，统计为部分结果
    pub partial: bool,
}

impl PdfHiddenStats {
    /// 是否存在注入嫌疑（值得写入 evasion_json、供后续 evasion 围标信号消费）。
    /// OCR 双层页与 partial 都不算命中——不产生"检查通过/清白"背书。
    pub fn has_suspect(&self) -> bool {
        self.hidden_chars > 0
    }
}

/// 6 元仿射矩阵 [a b c d e f]，行向量约定：x' = a·x + c·y + e，y' = b·x + d·y + f。
type Matrix = [f64; 6];
const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

/// 组合 a 后 b（先 a 再 b）：结果映射 = b(a(点))。
fn mat_mul(a: &Matrix, b: &Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn translate(tx: f64, ty: f64) -> Matrix {
    [1.0, 0.0, 0.0, 1.0, tx, ty]
}

/// 线性部分行列式（面积缩放因子）。
fn det_linear(m: &Matrix) -> f64 {
    m[0] * m[3] - m[1] * m[2]
}

/// 从操作数取前 6 个数值组成矩阵（不足 6 个返回 None）。
fn matrix_from(ops: &[Object]) -> Option<Matrix> {
    if ops.len() < 6 {
        return None;
    }
    let mut m = [0.0f64; 6];
    for (i, slot) in m.iter_mut().enumerate() {
        *slot = ops[i].as_float().ok()? as f64;
    }
    Some(m)
}

fn num_at(ops: &[Object], i: usize) -> Option<f64> {
    ops.get(i).and_then(|o| o.as_float().ok()).map(|v| v as f64)
}

/// RGB → 相对亮度（Rec.601 luma）。
fn luma_rgb(r: f64, g: f64, b: f64) -> f64 {
    0.299 * r + 0.587 * g + 0.114 * b
}

/// CMYK → RGB → 亮度。
fn luma_cmyk(c: f64, m: f64, y: f64, k: f64) -> f64 {
    let r = (1.0 - c) * (1.0 - k);
    let g = (1.0 - m) * (1.0 - k);
    let b = (1.0 - y) * (1.0 - k);
    luma_rgb(r, g, b)
}

/// sc/scn 通用填充色：仅数值操作数（1=gray/3=rgb/4=cmyk）可算亮度；含 Name（图案/
/// 分色空间）无法判定 → None（保持前值）。
fn fill_from_sc(ops: &[Object]) -> Option<f64> {
    if ops.iter().any(|o| matches!(o, Object::Name(_))) {
        return None;
    }
    let nums: Vec<f64> = ops.iter().filter_map(|o| o.as_float().ok()).map(|v| v as f64).collect();
    match nums.len() {
        1 => Some(nums[0]),
        3 => Some(luma_rgb(nums[0], nums[1], nums[2])),
        4 => Some(luma_cmyk(nums[0], nums[1], nums[2], nums[3])),
        _ => None,
    }
}

/// 展示串字节数（按字节近似字符数）。String 取字节长，其余 0。
fn show_len(o: &Object) -> u64 {
    match o {
        Object::String(bytes, _) => bytes.len() as u64,
        _ => 0,
    }
}

/// TJ 数组：累加其中所有 String 元素字节数（数字为字距调整，不计）。
fn show_array_len(o: &Object) -> u64 {
    match o {
        Object::Array(arr) => arr.iter().map(show_len).sum(),
        _ => 0,
    }
}

/// 图形状态（受 q/Q 存取的子集）：CTM、填充亮度、文本渲染模式、字号。
/// 文本矩阵 Tm/Tlm 不属于图形状态（BT 重置），单独在页级线性维护。
#[derive(Clone)]
struct GState {
    ctm: Matrix,
    fill_lum: f64,
    tr: i64,
    font_size: f64,
}

impl Default for GState {
    fn default() -> Self {
        Self { ctm: IDENTITY, fill_lum: 0.0, tr: 0, font_size: 0.0 }
    }
}

/// 单页审计累加器。
#[derive(Default)]
struct PageAudit {
    tr: u64,
    white: u64,
    off: u64,
    tiny: u64,
    hidden: u64,
    visible: u64,
    total: u64,
    has_full_page_image: bool,
}

/// 审计入口：损坏/加密/非 PDF 一律返回 None（静默降级，不阻塞导入）。
pub fn audit(path: &std::path::Path) -> Option<PdfHiddenStats> {
    let doc = Document::load(path).ok()?;
    Some(audit_doc(&doc))
}

fn audit_doc(doc: &Document) -> PdfHiddenStats {
    let mut stats = PdfHiddenStats::default();
    let pages = doc.get_pages();
    for (i, (_, page_id)) in pages.into_iter().enumerate() {
        if i >= MAX_PAGES {
            stats.partial = true;
            break;
        }
        let page_no = (i as u32) + 1;
        audit_page(doc, page_id, page_no, &mut stats);
    }
    stats.hidden_ratio = if stats.total_chars > 0 {
        stats.hidden_chars as f64 / stats.total_chars as f64
    } else {
        0.0
    };
    stats
}

fn audit_page(doc: &Document, page_id: ObjectId, page_no: u32, stats: &mut PdfHiddenStats) {
    let content_bytes = match doc.get_page_content(page_id) {
        Ok(b) => b,
        Err(_) => return, // 无内容流/解码失败：跳过该页，不阻塞
    };
    if content_bytes.len() > MAX_STREAM_BYTES {
        stats.partial = true;
        return;
    }
    let content = match lopdf::content::Content::decode(&content_bytes) {
        Ok(c) => c,
        Err(_) => return, // 内容流方言解析失败：静默跳过该页
    };
    let media = page_media_box(doc, page_id);
    let page_area = ((media[2] - media[0]) * (media[3] - media[1])).abs();
    let image_xobjects = collect_image_xobjects(doc, page_id);

    let mut gs = GState::default();
    let mut stack: Vec<GState> = Vec::new();
    let mut tm = IDENTITY;
    let mut tlm = IDENTITY;
    let mut leading = 0.0f64;
    let mut pa = PageAudit::default();

    for op in &content.operations {
        let ops = &op.operands;
        match op.operator.as_str() {
            "q" => stack.push(gs.clone()),
            "Q" => {
                if let Some(s) = stack.pop() {
                    gs = s;
                }
            }
            "cm" => {
                if let Some(m) = matrix_from(ops) {
                    gs.ctm = mat_mul(&m, &gs.ctm);
                }
            }
            "BT" => {
                tm = IDENTITY;
                tlm = IDENTITY;
            }
            "Td" => {
                if let (Some(tx), Some(ty)) = (num_at(ops, 0), num_at(ops, 1)) {
                    tlm = mat_mul(&translate(tx, ty), &tlm);
                    tm = tlm;
                }
            }
            "TD" => {
                if let (Some(tx), Some(ty)) = (num_at(ops, 0), num_at(ops, 1)) {
                    leading = -ty;
                    tlm = mat_mul(&translate(tx, ty), &tlm);
                    tm = tlm;
                }
            }
            "Tm" => {
                if let Some(m) = matrix_from(ops) {
                    tlm = m;
                    tm = m;
                }
            }
            "T*" => {
                tlm = mat_mul(&translate(0.0, -leading), &tlm);
                tm = tlm;
            }
            "TL" => {
                if let Some(v) = num_at(ops, 0) {
                    leading = v;
                }
            }
            "Tf" => {
                if let Some(v) = num_at(ops, 1) {
                    gs.font_size = v;
                }
            }
            "Tr" => {
                if let Some(v) = ops.first().and_then(|o| o.as_i64().ok()) {
                    gs.tr = v;
                }
            }
            "g" => {
                if let Some(v) = num_at(ops, 0) {
                    gs.fill_lum = v;
                }
            }
            "rg" => {
                if let (Some(r), Some(g), Some(b)) = (num_at(ops, 0), num_at(ops, 1), num_at(ops, 2)) {
                    gs.fill_lum = luma_rgb(r, g, b);
                }
            }
            "k" => {
                if let (Some(c), Some(m), Some(y), Some(kk)) =
                    (num_at(ops, 0), num_at(ops, 1), num_at(ops, 2), num_at(ops, 3))
                {
                    gs.fill_lum = luma_cmyk(c, m, y, kk);
                }
            }
            "sc" | "scn" => {
                if let Some(l) = fill_from_sc(ops) {
                    gs.fill_lum = l;
                }
            }
            "Do" => {
                if let Some(name) = ops.first().and_then(|o| o.as_name().ok()) {
                    if image_xobjects.contains(name) && page_area > 0.0 {
                        // 图像 XObject 在单位方格 [0,1]² 内绘制，经 CTM 变换后面积 = |det(CTM)|
                        let area = det_linear(&gs.ctm).abs();
                        if area >= FULL_PAGE_IMAGE_RATIO * page_area {
                            pa.has_full_page_image = true;
                        }
                    }
                }
            }
            "Tj" => classify(ops.first().map(show_len).unwrap_or(0), &tm, &gs, &media, &mut pa),
            "TJ" => classify(ops.first().map(show_array_len).unwrap_or(0), &tm, &gs, &media, &mut pa),
            "'" => {
                tlm = mat_mul(&translate(0.0, -leading), &tlm);
                tm = tlm;
                classify(ops.first().map(show_len).unwrap_or(0), &tm, &gs, &media, &mut pa);
            }
            "\"" => {
                tlm = mat_mul(&translate(0.0, -leading), &tlm);
                tm = tlm;
                // 操作数：aw ac string
                classify(ops.get(2).map(show_len).unwrap_or(0), &tm, &gs, &media, &mut pa);
            }
            _ => {}
        }
    }

    // OCR 双层页判定：整页图像 + 全页隐藏文本（无任何可见文本）= 合法扫描 OCR 层，
    // 归入 ocr_layer_pages 不计规避；否则（可见+隐藏共存 / 隐藏页无整页图）算注入嫌疑。
    if pa.hidden > 0 && pa.has_full_page_image && pa.visible == 0 {
        stats.ocr_layer_pages.push(page_no);
        return;
    }
    stats.total_chars += pa.total;
    if pa.hidden > 0 {
        stats.tr_invisible_chars += pa.tr;
        stats.white_text_chars += pa.white;
        stats.off_canvas_chars += pa.off;
        stats.tiny_font_chars += pa.tiny;
        stats.hidden_chars += pa.hidden;
        stats.hit_pages.push(PdfHiddenPage { page: page_no, hidden_chars: pa.hidden });
    }
}

/// 按当前图形/文本状态归类一个展示串（nchars 为其字节近似字符数）。
fn classify(nchars: u64, tm: &Matrix, gs: &GState, media: &[f64; 4], pa: &mut PageAudit) {
    if nchars == 0 {
        return;
    }
    pa.total += nchars;
    // 文本原点在用户空间 = (0,0) 经 Tm×CTM 变换（取平移分量）
    let combined = mat_mul(tm, &gs.ctm);
    let origin_x = combined[4];
    let origin_y = combined[5];
    let scale = det_linear(&combined).abs().sqrt();
    let eff_size = gs.font_size * scale;

    let is_tr3 = gs.tr == 3 || gs.tr == 7;
    let is_white = gs.fill_lum >= WHITE_LUM_THRESHOLD;
    // font_size 未设置(0)时不判极小字号（合法流总会先 Tf，避免对畸形流全量误报）
    let is_tiny = gs.font_size > 0.0 && eff_size < TINY_SIZE_PT;
    let is_off = origin_x < media[0] - OFF_CANVAS_MARGIN
        || origin_x > media[2] + OFF_CANVAS_MARGIN
        || origin_y < media[1] - OFF_CANVAS_MARGIN
        || origin_y > media[3] + OFF_CANVAS_MARGIN;

    if is_tr3 {
        pa.tr += nchars;
    }
    if is_white {
        pa.white += nchars;
    }
    if is_off {
        pa.off += nchars;
    }
    if is_tiny {
        pa.tiny += nchars;
    }
    if is_tr3 || is_white || is_off || is_tiny {
        pa.hidden += nchars;
    } else {
        pa.visible += nchars;
    }
}

/// 解析页面 MediaBox（沿 /Parent 链继承，带环路保护），归一化为 [x0,y0,x1,y1]。
/// 取不到则回落 US Letter [0,0,612,792]。
fn page_media_box(doc: &Document, page_id: ObjectId) -> [f64; 4] {
    const DEFAULT: [f64; 4] = [0.0, 0.0, 612.0, 792.0];
    let mut seen: HashSet<ObjectId> = HashSet::new();
    let mut cur = Some(page_id);
    while let Some(id) = cur {
        if !seen.insert(id) {
            break;
        }
        let Ok(dict) = doc.get_dictionary(id) else { break };
        if let Ok(mb) = dict.get(b"MediaBox") {
            let arr = match mb {
                Object::Array(a) => Some(a),
                Object::Reference(rid) => doc.get_object(*rid).ok().and_then(|o| o.as_array().ok()),
                _ => None,
            };
            if let Some(a) = arr {
                if a.len() >= 4 {
                    let v: Vec<f64> =
                        a.iter().take(4).map(|o| o.as_float().map(|f| f as f64).unwrap_or(0.0)).collect();
                    return [v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])];
                }
            }
        }
        cur = dict.get(b"Parent").ok().and_then(|o| o.as_reference().ok());
    }
    DEFAULT
}

/// 收集页面资源中 /Subtype /Image 的 XObject 名（含 /Parent 继承的资源；首版仅页级，
/// 不下钻 Form XObject 嵌套）。
fn collect_image_xobjects(doc: &Document, page_id: ObjectId) -> HashSet<Vec<u8>> {
    let mut images = HashSet::new();
    let Ok((direct, resource_ids)) = doc.get_page_resources(page_id) else {
        return images;
    };
    let mut resource_dicts: Vec<&Dictionary> = Vec::new();
    if let Some(d) = direct {
        resource_dicts.push(d);
    }
    for rid in resource_ids {
        if let Ok(d) = doc.get_dictionary(rid) {
            resource_dicts.push(d);
        }
    }
    for rd in resource_dicts {
        let xobj = match rd.get(b"XObject") {
            Ok(Object::Reference(id)) => doc.get_dictionary(*id).ok(),
            Ok(Object::Dictionary(dd)) => Some(dd),
            _ => None,
        };
        let Some(xd) = xobj else { continue };
        for (name, val) in xd.iter() {
            let subtype = match val {
                Object::Reference(id) => doc
                    .get_object(*id)
                    .ok()
                    .and_then(|o| o.as_stream().ok())
                    .and_then(|s| s.dict.get(b"Subtype").ok())
                    .and_then(|x| x.as_name().ok()),
                Object::Stream(s) => s.dict.get(b"Subtype").ok().and_then(|x| x.as_name().ok()),
                _ => None,
            };
            if subtype == Some(b"Image".as_ref()) {
                images.insert(name.clone());
            }
        }
    }
    images
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::{dictionary, Document, Object, Stream};
    use std::path::{Path, PathBuf};

    fn tmp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bg_pdfaudit_{tag}_{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// 用给定 content 字节与可选整页图像构造单页 PDF（MediaBox 612×792）。
    fn write_pdf(dir: &Path, name: &str, content: &[u8], with_full_page_image: bool) -> PathBuf {
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "BaseFont" => "Helvetica",
        });
        let mut resources = dictionary! {
            "Font" => dictionary! { "F1" => font_id },
        };
        if with_full_page_image {
            let img_id = doc.add_object(
                Stream::new(
                    dictionary! {
                        "Type" => "XObject",
                        "Subtype" => "Image",
                        "Width" => 8,
                        "Height" => 8,
                        "ColorSpace" => "DeviceGray",
                        "BitsPerComponent" => 8,
                    },
                    vec![0u8; 64],
                )
                .with_compression(false),
            );
            resources.set("XObject", dictionary! { "Im0" => img_id });
        }
        let content_id =
            doc.add_object(Stream::new(dictionary! {}, content.to_vec()).with_compression(false));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content_id,
            "Resources" => resources,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let p = dir.join(name);
        doc.save(&p).unwrap();
        p
    }

    #[test]
    fn classifies_four_hidden_categories_with_expected_counts() {
        // 五段：正常可见(7) / Tr=3 不可见(6) / 白字(5) / 出画布(8) / 0.5pt 极小字号(4)
        let content = b"\
BT /F1 12 Tf 0 0 0 rg 1 0 0 1 100 700 Tm 0 Tr (VISIBLE) Tj ET\n\
BT /F1 12 Tf 1 0 0 1 100 680 Tm 3 Tr (HIDDEN) Tj ET\n\
BT /F1 12 Tf 1 1 1 rg 1 0 0 1 100 660 Tm 0 Tr (WHITE) Tj ET\n\
BT /F1 12 Tf 0 0 0 rg 1 0 0 1 -500 -500 Tm 0 Tr (OFFCANVAS) Tj ET\n\
BT /F1 0.5 Tf 0 0 0 rg 1 0 0 1 100 640 Tm 0 Tr (TINY) Tj ET\n";
        let dir = tmp_dir("four");
        let p = write_pdf(&dir, "four.pdf", content, false);
        let s = audit(&p).expect("应产出审计");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(s.tr_invisible_chars, 6, "HIDDEN 6 字节 Tr=3");
        assert_eq!(s.white_text_chars, 5, "WHITE 5 字节白字");
        assert_eq!(s.off_canvas_chars, 9, "OFFCANVAS 9 字节出画布");
        assert_eq!(s.tiny_font_chars, 4, "TINY 4 字节 0.5pt");
        assert_eq!(s.hidden_chars, 6 + 5 + 9 + 4, "四类去重并集（互不重叠）");
        assert_eq!(s.total_chars, 7 + 6 + 5 + 9 + 4, "含可见 VISIBLE 7 字节");
        assert!(!s.partial);
        assert!(s.ocr_layer_pages.is_empty());
        assert_eq!(s.hit_pages.len(), 1);
        assert_eq!(s.hit_pages[0].page, 1);
        assert_eq!(s.hit_pages[0].hidden_chars, 24);
        let expected_ratio = 24.0 / 31.0;
        assert!((s.hidden_ratio - expected_ratio).abs() < 1e-9, "占比 {}", s.hidden_ratio);
        assert!(s.has_suspect());
    }

    #[test]
    fn ocr_double_layer_page_not_counted_as_evasion() {
        // 整页图像 XObject（612×792 cm 覆盖全页）+ 全页 Tr=3 隐藏文本 = 合法 OCR 双层页
        let content = b"\
q 612 0 0 792 0 0 cm /Im0 Do Q\n\
BT /F1 10 Tf 1 0 0 1 100 700 Tm 3 Tr (scanned ocr text layer) Tj ET\n";
        let dir = tmp_dir("ocr");
        let p = write_pdf(&dir, "ocr.pdf", content, true);
        let s = audit(&p).expect("应产出审计");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(s.ocr_layer_pages, vec![1], "整页图+全隐藏文本归入 OCR 双层页");
        assert_eq!(s.hidden_chars, 0, "OCR 层规避计数为 0");
        assert_eq!(s.tr_invisible_chars, 0);
        assert!(s.hit_pages.is_empty());
        assert!(!s.has_suspect(), "OCR 双层页不产生嫌疑");
    }

    #[test]
    fn visible_and_hidden_coexist_is_injection_even_with_image() {
        // 整页图 + 可见文本 + 隐藏文本共存 → 注入嫌疑（不豁免为 OCR 层）
        let content = b"\
q 612 0 0 792 0 0 cm /Im0 Do Q\n\
BT /F1 12 Tf 0 0 0 rg 1 0 0 1 100 700 Tm 0 Tr (real visible bid text) Tj ET\n\
BT /F1 12 Tf 1 0 0 1 100 680 Tm 3 Tr (injected) Tj ET\n";
        let dir = tmp_dir("coexist");
        let p = write_pdf(&dir, "coexist.pdf", content, true);
        let s = audit(&p).expect("应产出审计");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(s.ocr_layer_pages.is_empty(), "可见+隐藏共存不算 OCR 层");
        assert_eq!(s.hidden_chars, 8, "injected 8 字节计入规避");
        assert_eq!(s.hit_pages.len(), 1);
        assert!(s.has_suspect());
    }

    #[test]
    fn clean_pdf_has_zero_hidden() {
        let content = b"\
BT /F1 12 Tf 0 0 0 rg 1 0 0 1 72 720 Tm 0 Tr (Normal visible paragraph one.) Tj ET\n\
BT /F1 12 Tf 0 0 0 rg 1 0 0 1 72 700 Tm 0 Tr (Normal visible paragraph two.) Tj ET\n";
        let dir = tmp_dir("clean");
        let p = write_pdf(&dir, "clean.pdf", content, false);
        let s = audit(&p).expect("应产出审计");
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(s.hidden_chars, 0);
        assert!(s.hit_pages.is_empty());
        assert!(s.total_chars > 0);
        assert!((s.hidden_ratio - 0.0).abs() < 1e-12);
        assert!(!s.has_suspect(), "干净 PDF 不写 evasion（不做清白背书）");
    }

    #[test]
    fn corrupt_pdf_yields_none_without_panic() {
        let dir = tmp_dir("bad");
        let bad = dir.join("broken.pdf");
        std::fs::write(&bad, "这不是 PDF，只是伪装扩展名。").unwrap();
        assert!(audit(&bad).is_none(), "损坏 PDF 静默降级 None");
        let missing = dir.join("nope.pdf");
        assert!(audit(&missing).is_none(), "不存在的文件不 panic");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn hundred_page_audit_is_fast() {
        // 性能守卫（宽松绝对界）：100 页内容流审计应远低于此界，超时即提示 O(n²) 退化
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let per_page = b"BT /F1 12 Tf 0 0 0 rg 1 0 0 1 72 720 Tm 0 Tr (A line of visible body text on the page for timing.) Tj ET\nBT /F1 12 Tf 1 0 0 1 72 700 Tm 3 Tr (hidden injected fragment) Tj ET\n";
        let mut kids: Vec<Object> = Vec::new();
        for _ in 0..100 {
            let cid = doc
                .add_object(Stream::new(dictionary! {}, per_page.to_vec()).with_compression(false));
            let pid = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => cid,
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
            });
            kids.push(pid.into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let dir = tmp_dir("perf");
        let p = dir.join("big.pdf");
        doc.save(&p).unwrap();

        let t0 = std::time::Instant::now();
        let s = audit(&p).expect("应产出审计");
        let elapsed = t0.elapsed();
        let _ = std::fs::remove_dir_all(&dir);

        assert_eq!(s.hit_pages.len(), 100, "每页各有一处隐藏片段");
        assert!(!s.partial, "100 页未触发上限");
        assert!(elapsed.as_millis() < 5000, "100 页审计耗时 {}ms 过长", elapsed.as_millis());
    }

    #[test]
    fn real_sample_pdf_has_no_hidden_text() {
        // 验收：正常文字版 sample.pdf hidden_chars=0（不误报真实排版正文）
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sample.pdf");
        let s = audit(&fixture).expect("sample.pdf 应可审计");
        assert_eq!(s.hidden_chars, 0, "正常文字版无隐藏文本");
        assert!(!s.has_suspect(), "正常 PDF 不产生嫌疑");
        assert!(s.hit_pages.is_empty());
        assert!(s.total_chars > 0, "确实审计到了正文字符");
    }

    #[test]
    fn page_cap_marks_partial_and_stops_at_limit() {
        // 超过 MAX_PAGES(500) 记 partial 且只审计前 500 页：构造 501 页各含 1 处隐藏片段，
        // 命中页应恰为 500、partial=true（第 501 页未审计）
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let font_id = doc.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let per_page = b"BT /F1 12 Tf 1 0 0 1 72 700 Tm 3 Tr (h) Tj ET\n";
        let mut kids: Vec<Object> = Vec::new();
        for _ in 0..(MAX_PAGES + 1) {
            let cid = doc
                .add_object(Stream::new(dictionary! {}, per_page.to_vec()).with_compression(false));
            let pid = doc.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => pages_id,
                "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
                "Contents" => cid,
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
            });
            kids.push(pid.into());
        }
        let count = kids.len() as i64;
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let dir = tmp_dir("cap");
        let p = dir.join("cap.pdf");
        doc.save(&p).unwrap();
        let s = audit(&p).expect("应产出审计");
        let _ = std::fs::remove_dir_all(&dir);

        assert!(s.partial, "超过 500 页应记 partial");
        assert_eq!(s.hit_pages.len(), MAX_PAGES, "只审计前 500 页");
        assert_eq!(s.hidden_chars, MAX_PAGES as u64, "每页 1 字节隐藏，共 500");
    }
}
