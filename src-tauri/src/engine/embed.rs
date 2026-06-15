// 语义查重：embedding + 余弦。首次使用某模型会下载（受 security.allowCloudModel 控制）。
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

/// 一个可选语义模型的规格。id 进 embeddings 表主键，换模型不脏读且各自缓存。
#[derive(Debug, Clone)]
pub struct EmbedModelSpec {
    pub key: &'static str,   // 配置值（前后端一致）
    pub id: &'static str,    // 缓存 model_id（稳定，勿改，否则旧缓存失配）
    pub label: &'static str, // UI 展示
    pub model: EmbeddingModel,
}

/// 可选模型注册表。默认项（e5-small）的 id 沿用历史值，保证旧缓存继续命中。
pub const MODELS: &[EmbedModelSpec] = &[
    EmbedModelSpec {
        key: "e5-small",
        id: "multilingual-e5-small",
        label: "E5 多语种 · 小（默认，快）",
        model: EmbeddingModel::MultilingualE5Small,
    },
    EmbedModelSpec {
        key: "e5-base",
        id: "multilingual-e5-base",
        label: "E5 多语种 · 大（更准，慢）",
        model: EmbeddingModel::MultilingualE5Base,
    },
    EmbedModelSpec {
        key: "bge-zh",
        id: "bge-small-zh-v1.5",
        label: "BGE 中文优化（中文标书更准）",
        model: EmbeddingModel::BGESmallZHV15,
    },
];

/// 配置值 → 模型规格；未知值回落默认。
pub fn resolve(key: &str) -> &'static EmbedModelSpec {
    MODELS.iter().find(|m| m.key == key).unwrap_or(&MODELS[0])
}

/// 常驻语义模型：记录已加载的 model_id，换模型时丢弃重载。
pub type LoadedEmbedder = Option<(String, TextEmbedding)>;

fn cache_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cache/bidguard/fastembed"))
}

/// 语义模型缓存目录（工具屏展示用）。
pub fn cache_dir_path() -> Option<std::path::PathBuf> {
    cache_dir()
}

/// 递归收集缓存目录下所有文件 (路径, 字节)，最深 5 层。
fn walk_files(dir: &std::path::Path, depth: u8, out: &mut Vec<(std::path::PathBuf, u64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        match e.file_type() {
            Ok(t) if t.is_dir() && depth > 0 => walk_files(&p, depth - 1, out),
            Ok(t) if t.is_file() => {
                let sz = e.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((p, sz));
            }
            _ => {}
        }
    }
}

/// 模型文件是否已在本地缓存（任一模型，无需联网即可加载）。
pub fn model_cached() -> bool {
    let Some(d) = cache_dir() else { return false };
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    files.iter().any(|(p, _)| p.extension().is_some_and(|x| x == "onnx"))
}

/// 指定模型是否已缓存（fastembed 缓存目录名含模型 id，据此匹配）。
pub fn model_cached_for(spec: &EmbedModelSpec) -> bool {
    let Some(d) = cache_dir() else { return false };
    let id = spec.id.to_ascii_lowercase();
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    files.iter().any(|(p, _)| {
        p.extension().is_some_and(|x| x == "onnx")
            && p.to_string_lossy().to_ascii_lowercase().contains(&id)
    })
}

/// 指定模型缓存占用字节数（0 = 未缓存）。
pub fn model_cache_bytes(spec: &EmbedModelSpec) -> u64 {
    let Some(d) = cache_dir() else { return 0 };
    let id = spec.id.to_ascii_lowercase();
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    files
        .iter()
        .filter(|(p, _)| p.to_string_lossy().to_ascii_lowercase().contains(&id))
        .map(|(_, sz)| *sz)
        .sum()
}

/// 删除指定模型的本地缓存（含其 fastembed 目录）。返回删除的字节数。
pub fn clear_model_cache(spec: &EmbedModelSpec) -> u64 {
    let Some(d) = cache_dir() else { return 0 };
    let id = spec.id.to_ascii_lowercase();
    let mut removed = 0u64;
    let Ok(entries) = std::fs::read_dir(&d) else { return 0 };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() && p.to_string_lossy().to_ascii_lowercase().contains(&id) {
            let mut files = Vec::new();
            walk_files(&p, 5, &mut files);
            removed += files.iter().map(|(_, sz)| *sz).sum::<u64>();
            let _ = std::fs::remove_dir_all(&p);
        }
    }
    removed
}

fn init_model(model: EmbeddingModel) -> Option<TextEmbedding> {
    let mut opts = InitOptions::new(model).with_show_download_progress(false);
    // 稳定的绝对缓存目录：打包后 cwd=/ 不可写，必须显式指定，否则模型加载/下载失败
    if let Some(dir) = cache_dir() {
        opts = opts.with_cache_dir(dir);
    }
    TextEmbedding::try_new(opts).ok()
}

/// 确保槽位里有「指定模型」的常驻实例：已加载同一模型则复用；不同则丢弃重载。
/// allow_download=false（security.allowCloudModel，设计文档 §15.1）且本地无缓存时
/// 不发起联网下载，调用方走语义降级路径并在报告注明。
pub fn ensure<'a>(
    slot: &'a mut LoadedEmbedder,
    spec: &EmbedModelSpec,
    allow_download: bool,
) -> Option<&'a mut TextEmbedding> {
    let loaded_other = slot.as_ref().is_some_and(|(id, _)| id != spec.id);
    if loaded_other {
        *slot = None; // 换了模型，释放旧实例
    }
    if slot.is_none() {
        if !allow_download && !model_cached() {
            return None;
        }
        if let Some(m) = init_model(spec.model.clone()) {
            *slot = Some((spec.id.to_string(), m));
        }
    }
    slot.as_mut().map(|(_, m)| m)
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
        let model = ensure(&mut slot, &MODELS[0], true).expect("应能加载模型");
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

    #[test]
    fn resolve_known_and_unknown() {
        assert_eq!(resolve("bge-zh").id, "bge-small-zh-v1.5");
        assert_eq!(resolve("e5-base").id, "multilingual-e5-base");
        assert_eq!(resolve("不存在").key, "e5-small", "未知值回落默认");
        // 默认 id 必须沿用历史值（multilingual-e5-small），否则旧缓存失配
        assert_eq!(resolve("e5-small").id, "multilingual-e5-small");
    }
}
