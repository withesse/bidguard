// 字面相似度基元：jieba/单词分词 + 词频向量余弦（chunker / import_service 共用）。
use jieba_rs::Jieba;
use std::collections::HashMap;

/// 中文分词，过滤标点与单字噪声。
pub fn tokenize(jieba: &Jieba, text: &str) -> Vec<String> {
    jieba
        .cut(text, true)
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| s.chars().count() >= 2 && s.chars().any(|c| c.is_alphanumeric()))
        .collect()
}

/// 英文/拉丁文本分词：按非字母数字切分（CJK 在 Unicode 中也算 alphanumeric，
/// 故误入此路径的中文会整串成词——language=en 是用户显式选择，劣化可见可逆）。
pub fn tokenize_en(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.chars().count() >= 2)
        .map(str::to_string)
        .collect()
}

/// 文本语言判定（language=auto 用）：CJK 字符占字母数字类字符 ≥15% 视为中文。
/// 标书正文以中文为主，阈值放低以容忍大量英文术语/型号混排。
pub fn detect_language(text: &str) -> &'static str {
    let mut cjk = 0usize;
    let mut alnum = 0usize;
    for c in text.chars() {
        if c.is_alphanumeric() {
            alnum += 1;
            if matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF) {
                cjk += 1;
            }
        }
    }
    if alnum == 0 || cjk * 100 >= alnum * 15 {
        "zh"
    } else {
        "en"
    }
}

/// 按配置语言分词：zh → jieba；en → 单词切分；auto → 逐文本判定。
pub fn tokenize_lang(jieba: &Jieba, text: &str, language: &str) -> Vec<String> {
    let lang = match language {
        "zh" | "en" => language,
        _ => detect_language(text),
    };
    if lang == "en" {
        tokenize_en(text)
    } else {
        tokenize(jieba, text)
    }
}

fn term_freq(tokens: &[String]) -> HashMap<&str, f32> {
    let mut m: HashMap<&str, f32> = HashMap::new();
    for t in tokens {
        *m.entry(t.as_str()).or_insert(0.0) += 1.0;
    }
    m
}

/// 两份文档词频向量的余弦相似度，范围 0..1。
pub fn cosine(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let ta = term_freq(a);
    let tb = term_freq(b);
    let mut dot = 0.0f32;
    for (k, va) in &ta {
        if let Some(vb) = tb.get(k) {
            dot += va * vb;
        }
    }
    let na: f32 = ta.values().map(|v| v * v).sum::<f32>().sqrt();
    let nb: f32 = tb.values().map(|v| v * v).sum::<f32>().sqrt();
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
    fn tokenizes_chinese_english_and_mixed() {
        let jieba = Jieba::new();
        // 中文：常见复合词应整体切出
        let zh = tokenize(&jieba, "本项目采用微服务架构进行总体设计");
        assert!(zh.iter().any(|t| t == "微服务" || t == "架构"), "{zh:?}");
        // 英文：单词保留、大小写不变（大小写归一在 normalize 层）
        let en = tokenize(&jieba, "API Gateway provides unified access");
        assert!(en.iter().any(|t| t == "Gateway"), "{en:?}");
        // 混合：数字+单位、中英混排都应有产出
        let mixed = tokenize(&jieba, "系统支持 1000 并发，基于 Kubernetes 部署");
        assert!(mixed.iter().any(|t| t == "1000"), "{mixed:?}");
        assert!(mixed.iter().any(|t| t == "Kubernetes"), "{mixed:?}");
        // 过滤：单字与纯标点不产出
        let noise = tokenize(&jieba, "的，。！a b");
        assert!(noise.is_empty(), "单字/标点应过滤：{noise:?}");
    }

    #[test]
    fn language_detection_and_routing() {
        assert_eq!(detect_language("本项目采用微服务架构进行总体设计"), "zh");
        assert_eq!(detect_language("The system provides unified API access control"), "en");
        // 中文为主、夹带英文术语 → zh
        assert_eq!(detect_language("系统基于 Kubernetes 与 Prometheus 构建监控体系"), "zh");
        // 空文本回落 zh（产品主场景）
        assert_eq!(detect_language(""), "zh");

        let jieba = Jieba::new();
        let en = tokenize_lang(&jieba, "The gateway provides unified access", "en");
        assert!(en.iter().any(|t| t == "gateway") || en.iter().any(|t| t == "The"), "{en:?}");
        assert!(en.iter().all(|t| t.chars().count() >= 2));
        // auto：英文文本走单词切分，中文文本走 jieba
        let auto_en = tokenize_lang(&jieba, "unified access control list", "auto");
        assert!(auto_en.contains(&"unified".to_string()), "{auto_en:?}");
        // 中文走 jieba：多词产出且含常见复合词；若误入 en 路径会整串成单个 token
        let auto_zh = tokenize_lang(&jieba, "本项目采用微服务架构", "auto");
        assert!(auto_zh.len() > 1 && auto_zh.iter().any(|t| t == "架构"), "{auto_zh:?}");
    }

    #[test]
    fn cosine_basics() {
        let jieba = Jieba::new();
        let a = tokenize(&jieba, "采用分层解耦的微服务架构设计");
        let b = tokenize(&jieba, "采用分层解耦的微服务架构设计");
        let c = tokenize(&jieba, "数据治理与隐私合规审计");
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-6, "相同文本应为 1");
        assert!(cosine(&a, &c) < 0.3, "无关文本应接近 0");
        assert_eq!(cosine(&a, &[]), 0.0, "空集为 0");
    }
}
