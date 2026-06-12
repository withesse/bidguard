// 语义查重：中文 embedding + 余弦。首次运行会下载模型（multilingual-e5-small）。
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

/// 语义缓存的模型标识（embeddings 表主键之一，换模型不脏读）。
pub const MODEL_ID: &str = "multilingual-e5-small";

fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache/bidguard/fastembed"))
}

/// 模型文件是否已在本地缓存（无需联网即可加载）。
pub fn model_cached() -> bool {
    fn has_onnx(dir: &std::path::Path, depth: u8) -> bool {
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth > 0 && has_onnx(&p, depth - 1) {
                    return true;
                }
            } else if p.extension().is_some_and(|x| x == "onnx") {
                return true;
            }
        }
        false
    }
    cache_dir().map(|d| has_onnx(&d, 4)).unwrap_or(false)
}

fn init_model() -> Option<TextEmbedding> {
    let mut opts =
        InitOptions::new(EmbeddingModel::MultilingualE5Small).with_show_download_progress(false);
    // 稳定的绝对缓存目录：打包后 cwd=/ 不可写，必须显式指定，否则模型加载/下载失败
    if let Some(dir) = cache_dir() {
        opts = opts.with_cache_dir(dir);
    }
    TextEmbedding::try_new(opts).ok()
}

/// 确保槽位里有常驻模型实例（加载/下载一次，任务间复用）。不可用返回 None。
/// allow_download=false（security.allowCloudModel，设计文档 §15.1）且本地无缓存时
/// 不发起联网下载，调用方走语义降级路径并在报告注明。
pub fn ensure(slot: &mut Option<TextEmbedding>, allow_download: bool) -> Option<&mut TextEmbedding> {
    if slot.is_none() {
        if !allow_download && !model_cached() {
            return None;
        }
        *slot = init_model();
    }
    slot.as_mut()
}

/// 用常驻模型嵌入一批文本（E5 推荐加 "passage: " 前缀）。
pub fn embed_batch(model: &mut TextEmbedding, texts: &[String]) -> Option<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Some(Vec::new());
    }
    let docs: Vec<String> = texts.iter().map(|t| format!("passage: {t}")).collect();
    model.embed(docs, None).ok()
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na * nb)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore] // 需下载模型；用 `cargo test -- --ignored` 手动验证
    fn paraphrase_more_similar_than_unrelated() {
        let mut slot = None;
        let model = ensure(&mut slot, true).expect("应能加载模型");
        let embs = embed_batch(
            model,
            &[
                "系统采用分层解耦的微服务架构，统一 API 网关对外暴露能力".to_string(),
                "本方案使用分层解耦的微服务体系，经由 API 网关统一对外提供能力".to_string(),
                "本项目聚焦数据治理与隐私合规，强调本地化部署与最小权限".to_string(),
            ],
        )
        .expect("应能嵌入");
        let para = cosine(&embs[0], &embs[1]);
        let diff = cosine(&embs[0], &embs[2]);
        assert!(para > diff, "改写句应比无关句更相似：para={para} diff={diff}");
    }
}
