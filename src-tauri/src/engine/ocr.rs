// 扫描件 OCR：oar-ocr（PaddleOCR ONNX via ort）。模型在 src-tauri/models（dev）/ 资源目录（打包）。
// 逐页推理：每页之间检查取消旗标，长扫描件可被及时中断。
use image::RgbImage;
use oar_ocr::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("BIDGUARD_OCR_DIR") {
        dirs.push(PathBuf::from(p));
    }
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("models"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.join("models"));
            dirs.push(d.join("../Resources/models")); // macOS .app
            dirs.push(d.join("../Resources"));
            dirs.push(d.join("../lib/models")); // Linux
        }
    }
    dirs
}

/// 返回 (det, rec, dict) 三个模型路径，全部存在才返回。
fn model_paths() -> Option<(PathBuf, PathBuf, PathBuf)> {
    for dir in model_dirs() {
        let det = dir.join("ch_PP-OCRv4_det.onnx");
        let rec = dir.join("ch_PP-OCRv4_rec.onnx");
        let dict = dir.join("ppocr_keys_v1.txt");
        if det.exists() && rec.exists() && dict.exists() {
            return Some((det, rec, dict));
        }
    }
    None
}

/// 一行识别文本及其在页内的归一化位置（0..1，原点左上）。
/// 供前端在页图上叠加隐形可选中文本层（原文版式预览）。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OcrLine {
    pub t: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

/// 一页 OCR 结果：拼接文本（入库参与比对）+ 行级版面（预览文本层）。
pub struct OcrPage {
    pub text: String,
    pub lines: Vec<OcrLine>,
}

/// 逐页 OCR，返回每页识别文本与行级版面（与输入页一一对应）。
/// 模型缺失/识别失败/被取消返回 None；取消时不返回部分结果，避免半截文本被当成全文入库。
pub fn ocr_images(images: Vec<RgbImage>, cancel: &AtomicBool) -> Option<Vec<OcrPage>> {
    if images.is_empty() {
        return Some(Vec::new());
    }
    let (det, rec, dict) = model_paths()?;
    let ocr = OAROCRBuilder::new(
        det.to_string_lossy().into_owned(),
        rec.to_string_lossy().into_owned(),
        dict.to_string_lossy().into_owned(),
    )
    .build()
    .ok()?;
    let mut pages = Vec::with_capacity(images.len());
    for img in images {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let (pw, ph) = (img.width() as f32, img.height() as f32);
        let results = ocr.predict(vec![img]).ok()?;
        let mut out = String::new();
        let mut lines: Vec<OcrLine> = Vec::new();
        for r in results {
            for region in r.text_regions {
                let Some(t) = region.text else { continue };
                let t = t.trim();
                if t.is_empty() {
                    continue;
                }
                out.push_str(t);
                out.push('\n');
                // 检测多边形 → 轴对齐矩形，按页尺寸归一化
                let pts = &region.bounding_box.points;
                if !pts.is_empty() && pw > 0.0 && ph > 0.0 {
                    let (mut x0, mut y0) = (f32::MAX, f32::MAX);
                    let (mut x1, mut y1) = (0.0f32, 0.0f32);
                    for p in pts {
                        x0 = x0.min(p.x);
                        y0 = y0.min(p.y);
                        x1 = x1.max(p.x);
                        y1 = y1.max(p.y);
                    }
                    lines.push(OcrLine {
                        t: t.to_string(),
                        x: (x0 / pw).clamp(0.0, 1.0),
                        y: (y0 / ph).clamp(0.0, 1.0),
                        w: ((x1 - x0) / pw).clamp(0.0, 1.0),
                        h: ((y1 - y0) / ph).clamp(0.0, 1.0),
                    });
                }
            }
        }
        pages.push(OcrPage { text: out, lines });
    }
    Some(pages)
}
