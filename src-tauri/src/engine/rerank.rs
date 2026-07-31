// cross-encoder 复核带（W6-2，M7）：对 uncertain 带的簇做「复核建议」重打分。
//
// 【本层不改判】——产品纪律 §1.5-3：cross-encoder 是黑盒模型，且训练目标是【检索相关性】，
// 「相关」≠「同源改写」。把它的输出直接翻成 rewrite（指控性分类）等于用一个不可解释的分数
// 做定性结论。本层只产出 rerank_score：簇保持 uncertain，UI 显示「AI 复核倾向」并据此排序
// 复核队列，人工确认后才改分类。
//
// 模型三级来源镜像 embed.rs：
//  1) 内置随包（src-tauri/models/rerankers/<id>/ 五个文件）——离线开箱可用；
//  2) 自托管按需下载（~/.cache/bidguard/rerankers/<id>/，.tar 原子落盘 + sha256 校验）；
//  3) HF 联网回落（fastembed，受 security.allowCloudModel 闸门）——1/2 都没有时的回落。
// 与语义模型不同，复核模型【不常驻】：默认档 int8 与语义模型同时驻留会顶爆 8GB 办公机，
// 故按决策 2 串行加载——比对期先卸载 embedder 再加载 reranker，本层跑完立即释放。
use crate::engine::embed::{dedup_bytes, walk_files};
use crate::engine::modelfetch;
use fastembed::{
    OnnxSource, RerankInitOptions, RerankInitOptionsUserDefined, RerankerModel, TextRerank,
    TokenizerFiles, UserDefinedRerankingModel,
};
use std::path::{Path, PathBuf};

/// 截断预算：cross-encoder 对 [query, doc] 拼接后编码，长文只看开头。256 token 是延迟与
/// 判别力的折中（审查实测 fp32 512token 延迟被低估 2–6 倍）。tokenizer 的 max_length 是
/// 权威截断点；下面的字符预截断只是省掉无谓的分词开销（CJK 下 1 字 ≈ 1 token）。
pub const MAX_TOKENS: usize = 256;
/// 送入分词器前的字符级预截断长度（与 MAX_TOKENS 同量级，纯性能优化，不改变判读口径）。
pub const MAX_CHARS: usize = MAX_TOKENS;

/// 一个可选复核模型的规格。
#[derive(Debug, Clone)]
pub struct RerankModelSpec {
    pub key: &'static str,   // 配置值（前后端一致）
    pub id: &'static str,    // 缓存目录名（稳定，勿改，否则旧缓存失配）
    pub label: &'static str, // UI 展示
    pub size_label: &'static str,
    /// HF 回落用的 fastembed 内置档。注意内置档是 fp32：int8 量化档只能走内置/自托管来源，
    /// HF 回落会拿到体积更大、更慢的 fp32 权重（工具屏文案已注明）。
    pub model: RerankerModel,
    /// 自托管按需下载源（5 文件打成的 .tar）。None = 不提供自托管下载（仍可走内置或 HF 回落）。
    pub download_url: Option<&'static str>,
    /// 归档期望 sha256（十六进制）。Some 时下载后逐字节校验，不符即整目录丢弃。
    pub sha256: Option<&'static str>,
}

/// 复核模型注册表。默认档 = int8 量化基础档（决策 2：~300MB 按需下载 + 默认关闭）。
pub const RERANK_MODELS: &[RerankModelSpec] = &[
    RerankModelSpec {
        key: "bge-reranker-base-int8",
        id: "bge-reranker-base-int8",
        label: "BGE 复核 · 基础档 int8 量化（默认，中英）",
        size_label: "~300MB",
        model: RerankerModel::BGERerankerBase,
        // 内网/离线部署把 5 文件打成 .tar 放此 URL 并填 sha256；缺省时走内置目录或 HF 回落。
        download_url: None,
        sha256: None,
    },
    RerankModelSpec {
        key: "bge-reranker-v2-m3",
        id: "bge-reranker-v2-m3",
        label: "BGE 复核 · v2-m3 高精档（多语种）",
        size_label: "~2.2GB",
        model: RerankerModel::BGERerankerV2M3,
        download_url: None,
        sha256: None,
    },
];

/// 配置值 → 模型规格；未知值回落默认档。
pub fn resolve(key: &str) -> &'static RerankModelSpec {
    RERANK_MODELS.iter().find(|m| m.key == key).unwrap_or(&RERANK_MODELS[0])
}

/// 复核模型槽位：(已加载 id, 实例)。与 embed 不同，本槽位是比对期的【局部】变量，
/// 用完即释放（串行加载纪律，见文件头）。
pub type LoadedReranker = Option<(String, TextRerank)>;

const TOKENIZER_FILES: [&str; 4] = [
    "tokenizer.json",
    "config.json",
    "special_tokens_map.json",
    "tokenizer_config.json",
];
const ONNX_FILE: &str = "model.onnx";

/// HF 回落缓存目录（与语义模型共用一棵 fastembed 缓存树，各模型自带子目录）。
fn hf_cache_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/bidguard/fastembed"))
}

/// 复核模型缓存目录（工具屏展示用）。
pub fn cache_dir_path() -> Option<PathBuf> {
    download_dir()
}

/// 自托管按需下载的落地目录：~/.cache/bidguard/rerankers/<id>/。
fn download_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/bidguard/rerankers"))
}

/// 本地模型（内置随包 + 已下载）的候选基目录，按优先级；每个模型在 <base>/<id>/ 下放 5 个文件。
fn local_model_base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("BIDGUARD_RERANK_DIR") {
        dirs.push(PathBuf::from(p)); // 测试/内网覆盖
    }
    if let Some(d) = download_dir() {
        dirs.push(d); // 已下载档
    }
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("models/rerankers"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.join("models/rerankers"));
            dirs.push(d.join("../Resources/models/rerankers")); // macOS .app
            dirs.push(d.join("../lib/models/rerankers")); // Linux
        }
    }
    dirs
}

/// 在给定基目录集合里找某 id 的完整本地文件目录（纯函数，便于单测）。
fn resolve_local(id: &str, bases: &[PathBuf]) -> Option<PathBuf> {
    bases.iter().map(|b| b.join(id)).find(|dir| {
        dir.join(ONNX_FILE).is_file() && TOKENIZER_FILES.iter().all(|f| dir.join(f).is_file())
    })
}

fn local_model_dir(spec: &RerankModelSpec) -> Option<PathBuf> {
    resolve_local(spec.id, &local_model_base_dirs())
}

/// 某模型是否在 HF 缓存里（仅联网回落路径）。
fn hf_cached_for(spec: &RerankModelSpec) -> bool {
    let Some(d) = hf_cache_dir() else { return false };
    // fastembed 的 HF 缓存目录名取自 model_code（如 BAAI/bge-reranker-base）
    let id = spec.model.to_string().replace('/', "--").to_ascii_lowercase();
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    files.iter().any(|(p, _)| {
        p.extension().is_some_and(|x| x == "onnx")
            && p.to_string_lossy().to_ascii_lowercase().contains(&id)
    })
}

/// 指定复核模型是否已就绪（内置/已下载/HF 缓存任一），即离线可加载。
pub fn model_cached_for(spec: &RerankModelSpec) -> bool {
    local_model_dir(spec).is_some() || hf_cached_for(spec)
}

/// 指定复核模型占用字节数（0 = 未就绪）。
pub fn model_cache_bytes(spec: &RerankModelSpec) -> u64 {
    if let Some(dir) = local_model_dir(spec) {
        let mut files = Vec::new();
        walk_files(&dir, 5, &mut files);
        return dedup_bytes(&files);
    }
    let Some(d) = hf_cache_dir() else { return 0 };
    let id = spec.model.to_string().replace('/', "--").to_ascii_lowercase();
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    let matched: Vec<_> = files
        .into_iter()
        .filter(|(p, _)| p.to_string_lossy().to_ascii_lowercase().contains(&id))
        .collect();
    dedup_bytes(&matched)
}

/// 删除指定复核模型的本地缓存（自托管下载目录 + HF 缓存）。返回删除字节数。
/// 【不删】随包内置档（只读 Resources，是安装包的一部分）。
pub fn clear_model_cache(spec: &RerankModelSpec) -> u64 {
    let mut removed = 0u64;
    if let Some(dl) = download_dir().map(|d| d.join(spec.id)) {
        if dl.is_dir() {
            let mut files = Vec::new();
            walk_files(&dl, 5, &mut files);
            removed += dedup_bytes(&files);
            let _ = std::fs::remove_dir_all(&dl);
        }
    }
    let id = spec.model.to_string().replace('/', "--").to_ascii_lowercase();
    if let Some(d) = hf_cache_dir() {
        if let Ok(entries) = std::fs::read_dir(&d) {
            for e in entries.flatten() {
                let p = e.path();
                if p.is_dir() && p.to_string_lossy().to_ascii_lowercase().contains(&id) {
                    let mut files = Vec::new();
                    walk_files(&p, 5, &mut files);
                    removed += dedup_bytes(&files);
                    let _ = std::fs::remove_dir_all(&p);
                }
            }
        }
    }
    removed
}

/// 自托管按需下载：拉 spec.download_url 的 .tar（model.onnx + 4 个 tokenizer 文件），
/// 校验 sha256 后解到 ~/.cache/bidguard/rerankers/<id>/。已就绪返回 Ok(0)。
pub fn download_model(spec: &RerankModelSpec) -> Result<u64, String> {
    if local_model_dir(spec).is_some() {
        return Ok(0); // 内置或已下载
    }
    let url = spec.download_url.ok_or_else(|| {
        "该复核模型未提供自托管下载源；请启用联网由 HF 拉取，或将其文件放入 models/rerankers/".to_string()
    })?;
    let base = download_dir().ok_or_else(|| "无法定位下载目录".to_string())?;
    let dest = base.join(spec.id);
    let wanted: Vec<&str> = [ONNX_FILE].into_iter().chain(TOKENIZER_FILES).collect();
    let written = modelfetch::fetch_tar_into(url, spec.sha256, &dest, &wanted)?;
    if local_model_dir(spec).is_none() {
        let _ = std::fs::remove_dir_all(&dest); // 半套文件不留，避免被当就位
        return Err("下载归档缺少必需文件（需 model.onnx + 4 个 tokenizer 文件）".to_string());
    }
    Ok(written)
}

/// 从本地目录用 user-defined 方式加载（绕过 HF）。max_length 固定 MAX_TOKENS。
fn init_local(dir: &Path, spec: &RerankModelSpec) -> Option<TextRerank> {
    let onnx = std::fs::read(dir.join(ONNX_FILE)).ok()?;
    let tk = TokenizerFiles {
        tokenizer_file: std::fs::read(dir.join(TOKENIZER_FILES[0])).ok()?,
        config_file: std::fs::read(dir.join(TOKENIZER_FILES[1])).ok()?,
        special_tokens_map_file: std::fs::read(dir.join(TOKENIZER_FILES[2])).ok()?,
        tokenizer_config_file: std::fs::read(dir.join(TOKENIZER_FILES[3])).ok()?,
    };
    let ud = UserDefinedRerankingModel::new(OnnxSource::Memory(onnx), tk);
    let opts: RerankInitOptionsUserDefined =
        RerankInitOptions::new(spec.model.clone()).with_max_length(MAX_TOKENS).into();
    TextRerank::try_new_from_user_defined(ud, opts).ok()
}

fn init_hf(spec: &RerankModelSpec) -> Option<TextRerank> {
    let mut opts = RerankInitOptions::new(spec.model.clone())
        .with_max_length(MAX_TOKENS)
        .with_show_download_progress(false);
    // 稳定的绝对缓存目录：打包后 cwd=/ 不可写，不显式指定会加载/下载失败
    if let Some(dir) = hf_cache_dir() {
        opts = opts.with_cache_dir(dir);
    }
    TextRerank::try_new(opts).ok()
}

/// 离线闸门（纯函数，便于单测）：本地无文件、HF 无缓存且禁联网 ⇒ 不得加载，走降级。
/// 判定用「当前所选模型」而非「任一模型」——否则缓存了 A、禁联网选未缓存的 B 会误放行。
pub fn offline_blocked(local_present: bool, hf_cached: bool, allow_download: bool) -> bool {
    !local_present && !allow_download && !hf_cached
}

/// 确保槽位里有「指定复核模型」的实例：已加载同一模型则复用；不同则丢弃重载。
/// allow_download=false（比对路径恒为 false）且本地/HF 都无缓存时不联网，返回 None
/// 让调用方走降级路径（summary.rerank_degraded=true），【不静默失败】。
pub fn ensure<'a>(
    slot: &'a mut LoadedReranker,
    spec: &RerankModelSpec,
    allow_download: bool,
) -> Option<&'a mut TextRerank> {
    if slot.as_ref().is_some_and(|(id, _)| id != spec.id) {
        *slot = None; // 换了模型，释放旧实例
    }
    if slot.is_none() {
        let local = local_model_dir(spec);
        if offline_blocked(local.is_some(), hf_cached_for(spec), allow_download) {
            return None; // 离线闸门：守住「比对期绝不隐式下载」
        }
        let loaded = match &local {
            Some(dir) => init_local(dir, spec),
            None => init_hf(spec),
        };
        if let Some(m) = loaded {
            *slot = Some((spec.id.to_string(), m));
        }
    }
    slot.as_mut().map(|(_, m)| m)
}

/// logit → (0,1) 的复核建议分。cross-encoder 输出的是未归一 logit，直接展示无可读性；
/// 【sigmoid 后的数值仍不是「同源概率」】，只是同一模型内部可比的倾向强度。
pub fn sigmoid(logit: f32) -> f32 {
    let p = 1.0 / (1.0 + (-logit as f64).exp());
    ((p * 1e6).round() / 1e6) as f32
}

/// 送模型前的字符级预截断（按 char 边界，绝不切出半个字）。
pub fn truncate_for_rerank(text: &str) -> String {
    if text.chars().count() <= MAX_CHARS {
        return text.to_string();
    }
    text.chars().take(MAX_CHARS).collect()
}

/// 一段文本 vs 一组文本的复核建议分，返回顺序与 `docs` 一致。
/// fastembed 的 rerank 按分数降序返回，必须按 result.index 还原原序，否则分数会张冠李戴。
pub fn score_against(model: &mut TextRerank, query: &str, docs: &[String]) -> Option<Vec<f32>> {
    if docs.is_empty() {
        return Some(Vec::new());
    }
    let q = truncate_for_rerank(query);
    let d: Vec<String> = docs.iter().map(|t| truncate_for_rerank(t)).collect();
    let refs: Vec<&str> = d.iter().map(String::as_str).collect();
    let results = model.rerank(q.as_str(), refs, false, None).ok()?;
    let mut out = vec![f32::NAN; docs.len()];
    for r in results {
        if r.index >= out.len() {
            return None;
        }
        out[r.index] = sigmoid(r.score);
    }
    if out.iter().any(|s| !s.is_finite()) {
        return None; // 模型少还了分数，宁可整层降级也不写半套建议分
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_known_and_unknown() {
        assert_eq!(resolve("bge-reranker-base-int8").id, "bge-reranker-base-int8");
        assert_eq!(resolve("bge-reranker-v2-m3").id, "bge-reranker-v2-m3");
        // 未知值回落默认档（int8 量化基础档，决策 2）
        assert_eq!(resolve("不存在").key, "bge-reranker-base-int8");
        assert_eq!(RERANK_MODELS[0].key, "bge-reranker-base-int8", "默认档必须是 int8 量化档");
    }

    #[test]
    fn resolve_local_needs_all_five_files() {
        let root = std::env::temp_dir().join(format!("bg_rr_local_{}", uuid::Uuid::new_v4()));
        let dir = root.join("bge-reranker-base-int8");
        std::fs::create_dir_all(&dir).unwrap();
        let bases = vec![root.clone()];
        std::fs::write(dir.join("model.onnx"), b"x").unwrap();
        assert!(resolve_local("bge-reranker-base-int8", &bases).is_none(), "缺 tokenizer 不应就位");
        for f in TOKENIZER_FILES {
            std::fs::write(dir.join(f), b"{}").unwrap();
        }
        assert_eq!(resolve_local("bge-reranker-base-int8", &bases), Some(dir));
        assert!(resolve_local("bge-reranker-v2-m3", &bases).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sigmoid_is_monotonic_and_bounded() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(-8.0) < sigmoid(0.0) && sigmoid(0.0) < sigmoid(8.0));
        assert!(sigmoid(-40.0) >= 0.0 && sigmoid(40.0) <= 1.0);
        // 可复现承诺：同输入两次求值逐位一致（round6 后无浮点尾巴）
        assert_eq!(sigmoid(1.234_5), sigmoid(1.234_5));
    }

    #[test]
    fn truncate_keeps_char_boundary() {
        let long: String = "条款".repeat(400); // 800 字
        let t = truncate_for_rerank(&long);
        assert_eq!(t.chars().count(), MAX_CHARS);
        assert!(long.starts_with(&t), "截断只能砍尾部，不得改写内容");
        let short = "甲方应在每月十日前支付";
        assert_eq!(truncate_for_rerank(short), short, "短文本原样通过");
    }

    // 离线闸门：本地无文件 + HF 无缓存 + 禁联网 ⇒ 必须降级（不得为了跑复核而偷偷联网）。
    #[test]
    fn offline_gate_blocks_only_when_nothing_local() {
        assert!(offline_blocked(false, false, false), "三无 ⇒ 降级");
        assert!(!offline_blocked(true, false, false), "本地有文件 ⇒ 离线也能跑");
        assert!(!offline_blocked(false, true, false), "HF 已缓存 ⇒ 离线也能跑");
        assert!(!offline_blocked(false, false, true), "已授权联网 ⇒ 可下载");
    }

    #[test]
    #[ignore] // 需下载/放置真实复核模型；用 `cargo test -- --ignored` 手动验证
    fn rewrite_scores_higher_than_unrelated() {
        let mut slot: LoadedReranker = None;
        let model = ensure(&mut slot, &RERANK_MODELS[0], true).expect("应能加载复核模型");
        let scores = score_against(
            model,
            "系统采用分层解耦的微服务架构，统一 API 网关对外暴露能力",
            &[
                "本方案使用分层解耦的微服务体系，经由 API 网关统一对外提供能力".to_string(),
                "本项目聚焦数据治理与隐私合规，强调本地化部署与最小权限".to_string(),
            ],
        )
        .expect("应能打分");
        assert!(scores[0] > scores[1], "改写句复核分应高于无关句：{scores:?}");
    }
}
