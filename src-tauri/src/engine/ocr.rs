// 扫描件 OCR：oar-ocr（PaddleOCR ONNX via ort）。模型在 src-tauri/models（dev）/ 资源目录（打包）。
use image::RgbImage;
use oar_ocr::prelude::*;
use std::path::{Path, PathBuf};

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

/// 对一组图片做 OCR，拼接识别文本。模型缺失/失败返回 None。
pub fn ocr_images(images: Vec<RgbImage>) -> Option<String> {
    if images.is_empty() {
        return Some(String::new());
    }
    let (det, rec, dict) = model_paths()?;
    let ocr = OAROCRBuilder::new(
        det.to_string_lossy().into_owned(),
        rec.to_string_lossy().into_owned(),
        dict.to_string_lossy().into_owned(),
    )
    .build()
    .ok()?;
    let results = ocr.predict(images).ok()?;
    let mut out = String::new();
    for r in results {
        for region in r.text_regions {
            if let Some(t) = region.text {
                let t = t.trim();
                if !t.is_empty() {
                    out.push_str(t);
                    out.push('\n');
                }
            }
        }
    }
    Some(out)
}
