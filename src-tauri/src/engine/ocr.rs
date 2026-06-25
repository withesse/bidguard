// 扫描件 OCR：oar-ocr（PaddleOCR ONNX via ort）。模型在 src-tauri/models（dev）/ 资源目录（打包）。
// 逐页推理：每页之间检查取消旗标，长扫描件可被及时中断。
use image::RgbImage;
use oar_ocr::prelude::*;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// OCR 下载缓存目录（按需下载的高精档存此；工具屏展示与清理用）。
pub fn ocr_cache_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/bidguard/ocr"))
}

fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("BIDGUARD_OCR_DIR") {
        dirs.push(PathBuf::from(p));
    }
    if let Some(c) = ocr_cache_dir() {
        dirs.push(c); // 按需下载的高精档（medium）存此
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

/// OCR 档位注册表：PaddleOCR PP-OCRv6 三档，按体积/精度权衡。
/// tiny 极速（字符集精简）；small 默认；medium 高精（按需下载，不打包，与 small 共用字典）。
pub struct OcrModelSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub det: &'static str,
    pub rec: &'static str,
    pub dict: &'static str,
    /// 是否随应用打包（false = 需联网下载到缓存目录）。
    pub bundled: bool,
    pub size_label: &'static str,
    /// 按需下载源（PaddleOCR 官方 BCE 的 .tar）；打包档位为 None。
    pub det_url: Option<&'static str>,
    pub rec_url: Option<&'static str>,
}

pub const OCR_MODELS: &[OcrModelSpec] = &[
    OcrModelSpec {
        key: "v6-tiny",
        label: "PP-OCRv6 极速档（tiny）",
        det: "pp-ocrv6_tiny_det.onnx",
        rec: "pp-ocrv6_tiny_rec.onnx",
        dict: "ppocrv6_tiny_dict.txt",
        bundled: true,
        size_label: "~6MB",
        det_url: None,
        rec_url: None,
    },
    OcrModelSpec {
        key: "v6-small",
        label: "PP-OCRv6 标准档（small，默认）",
        det: "pp-ocrv6_small_det.onnx",
        rec: "pp-ocrv6_small_rec.onnx",
        dict: "ppocrv6_dict.txt",
        bundled: true,
        size_label: "~30MB",
        det_url: None,
        rec_url: None,
    },
    OcrModelSpec {
        key: "v6-medium",
        label: "PP-OCRv6 高精档（medium，需下载）",
        det: "pp-ocrv6_medium_det.onnx",
        rec: "pp-ocrv6_medium_rec.onnx",
        dict: "ppocrv6_dict.txt", // 与 small 共用 18708 字字典（打包，无需另下）
        bundled: false,
        size_label: "~132MB",
        det_url: Some("https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv6_medium_det_onnx_infer.tar"),
        rec_url: Some("https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv6_medium_rec_onnx_infer.tar"),
    },
];

pub const DEFAULT_OCR_MODEL: &str = "v6-small";

/// 按 key 解析档位；未知回落默认 small。
pub fn resolve(key: &str) -> &'static OcrModelSpec {
    OCR_MODELS.iter().find(|m| m.key == key).unwrap_or(&OCR_MODELS[1])
}

/// 在所有候选目录里找某文件（命中第一个存在的）。
fn find_model_file(name: &str) -> Option<PathBuf> {
    model_dirs().into_iter().map(|d| d.join(name)).find(|p| p.exists())
}

/// 某档位的 (det, rec, dict) 路径，三者各自跨目录查找（全部就位才返回）。
/// 逐文件而非同目录：medium 的 det/rec 在下载缓存、dict 在打包目录，可能不在同一目录。
fn model_paths_for(spec: &OcrModelSpec) -> Option<(PathBuf, PathBuf, PathBuf)> {
    Some((
        find_model_file(spec.det)?,
        find_model_file(spec.rec)?,
        find_model_file(spec.dict)?,
    ))
}

/// 选定档位路径；缺失时回落任一已就位档位（medium 未下载时 OCR 不致直接失败）。
fn resolve_paths(spec: &OcrModelSpec) -> Option<(PathBuf, PathBuf, PathBuf)> {
    model_paths_for(spec).or_else(|| OCR_MODELS.iter().find_map(model_paths_for))
}

/// 是否有任一 OCR 档位就位（工具屏/自检用）。
pub fn model_present() -> bool {
    OCR_MODELS.iter().any(|s| model_paths_for(s).is_some())
}

/// 指定档位是否就位（按需下载状态展示用）。
pub fn model_present_for(key: &str) -> bool {
    model_paths_for(resolve(key)).is_some()
}

/// 任一就位档位所在目录（工具屏展示路径）。
pub fn model_location() -> Option<PathBuf> {
    OCR_MODELS
        .iter()
        .find_map(model_paths_for)
        .map(|(det, _, _)| det.parent().map(Path::to_path_buf).unwrap_or(det))
}

/// 按需下载某档位（目前仅 medium）。det/rec 各下一个 PaddleOCR .tar，解出 inference.onnx
/// 存缓存目录（字典复用打包的 ppocrv6_dict.txt，不另下）。已就位返回 0；返回新写入字节数。
pub fn download_model(spec: &OcrModelSpec) -> Result<u64, String> {
    if model_paths_for(spec).is_some() {
        return Ok(0); // 已就位（打包或已下载）
    }
    let cache = ocr_cache_dir().ok_or_else(|| "无法定位 OCR 缓存目录".to_string())?;
    std::fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let mut total = 0u64;
    for (url, fname) in [(spec.det_url, spec.det), (spec.rec_url, spec.rec)] {
        let dest = cache.join(fname);
        if dest.exists() {
            continue; // 断点：已下好的那一半跳过
        }
        let url = url.ok_or_else(|| "该档位不支持下载".to_string())?;
        total += fetch_tar_onnx(url, &dest)?;
    }
    Ok(total)
}

/// 流式下载 .tar，解出其中的 inference.onnx 写到 dest（先写 .part 再 rename，避免半截文件被当成就位）。
fn fetch_tar_onnx(url: &str, dest: &Path) -> Result<u64, String> {
    let resp = ureq::get(url).call().map_err(|e| format!("下载失败：{e}"))?;
    let reader = resp.into_body().into_reader();
    let mut archive = tar::Archive::new(reader);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut e = entry.map_err(|e| e.to_string())?;
        let is_onnx = e
            .path()
            .ok()
            .and_then(|p| p.extension().map(|x| x.eq_ignore_ascii_case("onnx")))
            .unwrap_or(false);
        if is_onnx {
            let tmp = dest.with_extension("part");
            let mut out = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
            let n = std::io::copy(&mut e, &mut out).map_err(|e| e.to_string())?;
            std::fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
            return Ok(n);
        }
    }
    Err("tar 包内未找到 .onnx".to_string())
}

/// 删除某档位已下载的 det/rec（打包档位无影响）。返回释放字节数。
pub fn clear_model(spec: &OcrModelSpec) -> u64 {
    let Some(cache) = ocr_cache_dir() else {
        return 0;
    };
    let mut freed = 0u64;
    for f in [spec.det, spec.rec] {
        let p = cache.join(f);
        if let Ok(m) = std::fs::metadata(&p) {
            freed += m.len();
            let _ = std::fs::remove_file(&p);
        }
    }
    freed
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
pub fn ocr_images(
    images: Vec<RgbImage>,
    cancel: &AtomicBool,
    spec: &OcrModelSpec,
) -> Option<Vec<OcrPage>> {
    if images.is_empty() {
        return Some(Vec::new());
    }
    let (det, rec, dict) = resolve_paths(spec)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_keys_and_default_resolve() {
        assert_eq!(resolve("v6-small").key, "v6-small");
        assert_eq!(resolve("不存在").key, DEFAULT_OCR_MODEL, "未知 key 回落默认");
        // medium 可下载、tiny/small 打包
        assert!(!resolve("v6-medium").bundled && resolve("v6-medium").det_url.is_some());
        assert!(resolve("v6-tiny").bundled && resolve("v6-small").bundled);
    }

    #[test]
    #[ignore] // 需联网：cargo test ocr_download -- --ignored（下载 ~1.7MB tiny det 验证 .tar 解出 onnx）
    fn ocr_download_extracts_onnx() {
        let url = "https://paddle-model-ecology.bj.bcebos.com/paddlex/official_inference_model/paddle3.0.0/PP-OCRv6_tiny_det_onnx_infer.tar";
        let dest =
            std::env::temp_dir().join(format!("bg_ocrdl_{}.onnx", uuid::Uuid::new_v4()));
        let n = fetch_tar_onnx(url, &dest).expect("应能下载并解出 onnx");
        assert!(n > 1_000_000, "onnx 应 >1MB，实际 {n}");
        let head = std::fs::read(&dest).unwrap();
        let _ = std::fs::remove_file(&dest);
        assert_eq!(&head[..2], &[0x08, 0x0a], "应是 ONNX(protobuf) 头");
    }
}
