// 语义查重：中文 embedding + 余弦。首次运行会下载模型（multilingual-e5-small）。
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};

/// 嵌入一组文本（E5 推荐加 "passage: " 前缀）。模型不可用/失败返回 None。
pub fn embed(texts: &[String]) -> Option<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Some(Vec::new());
    }
    let mut model = TextEmbedding::try_new(
        InitOptions::new(EmbeddingModel::MultilingualE5Small).with_show_download_progress(false),
    )
    .ok()?;
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
        let embs = embed(&[
            "系统采用分层解耦的微服务架构，统一 API 网关对外暴露能力".to_string(),
            "本方案使用分层解耦的微服务体系，经由 API 网关统一对外提供能力".to_string(),
            "本项目聚焦数据治理与隐私合规，强调本地化部署与最小权限".to_string(),
        ])
        .expect("应能加载模型并嵌入");
        let para = cosine(&embs[0], &embs[1]);
        let diff = cosine(&embs[0], &embs[2]);
        assert!(para > diff, "改写句应比无关句更相似：para={para} diff={diff}");
    }
}
