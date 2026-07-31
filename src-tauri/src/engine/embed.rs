// 语义查重：embedding + 余弦。模型三种来源，按优先级：
//  1) 本地内置（随安装包，src-tauri/models/embeddings/<id>/ 五个文件）——离线开箱可用；
//  2) 本地已下载（工具屏按需下载到 ~/.cache/bidguard/embeddings/<id>/）；
//  3) HF 联网下载（fastembed，受 security.allowCloudModel 控制）——1/2 都没有时的回落。
// 1/2 走 try_new_from_user_defined 直接喂字节，pooling 与内置模型对齐后向量与 HF 版逐位等价。
use fastembed::{
    EmbeddingModel, InitOptions, InitOptionsUserDefined, Pooling, TextEmbedding, TokenizerFiles,
    UserDefinedEmbeddingModel,
};
use std::path::{Path, PathBuf};

/// 一个可选语义模型的规格。id 进 embeddings 表主键，换模型不脏读且各自缓存。
#[derive(Debug, Clone)]
pub struct EmbedModelSpec {
    pub key: &'static str,   // 配置值（前后端一致）
    pub id: &'static str,    // 缓存 model_id（稳定，勿改，否则旧缓存失配）
    pub label: &'static str, // UI 展示
    pub model: EmbeddingModel,
    /// 池化策略：本地(user-defined)加载时必须与内置模型一致，否则向量漂移、与 HF 缓存不可比。
    /// BGE 系 = Cls；E5 系 = Mean（取自 fastembed get_default_pooling_method）。
    pub pooling: Pooling,
    /// 自托管按需下载源（5 文件打成的 .tar：model.onnx + 4 个 tokenizer 文件）。
    /// None = 不提供自托管下载（仍可走 HF 或内置）。离线内网可把归档放此 URL 或直接内置。
    pub download_url: Option<&'static str>,
    /// 归档期望 sha256（十六进制）。Some 时下载后逐字节校验，不符即整目录丢弃——模型是判读
    /// 结论的上游，被截断/被替换的权重会让查重结论不可举证（W6-2 顺带补齐的取证短板）。
    pub sha256: Option<&'static str>,
}

/// 可选模型注册表。默认项（e5-small）的 id 沿用历史值，保证旧缓存继续命中。
pub const MODELS: &[EmbedModelSpec] = &[
    EmbedModelSpec {
        key: "bge-zh",
        id: "bge-small-zh-v1.5",
        label: "BGE 中文 · 小（默认，快，~95MB）",
        model: EmbeddingModel::BGESmallZHV15,
        pooling: Pooling::Cls,
        // 默认档，建议随安装包内置（见 src-tauri/models/embeddings/README.md）；无内置文件时回落 HF
        download_url: None,
        sha256: None,
    },
    EmbedModelSpec {
        key: "bge-large-zh",
        id: "bge-large-zh-v1.5",
        label: "BGE 中文 · 大（中文最准，~1.2GB）",
        model: EmbeddingModel::BGELargeZHV15,
        pooling: Pooling::Cls,
        download_url: None, // 太大不内置；可填自托管 .tar 供内网/离线下载，或回落 HF
        sha256: None,
    },
    EmbedModelSpec {
        key: "e5-large",
        id: "multilingual-e5-large",
        label: "E5 多语种 · 大（中英混排最准，~2.1GB）",
        model: EmbeddingModel::MultilingualE5Large,
        pooling: Pooling::Mean,
        download_url: None,
        sha256: None,
    },
    EmbedModelSpec {
        key: "e5-small",
        id: "multilingual-e5-small",
        label: "E5 多语种 · 小（轻量，~450MB）",
        model: EmbeddingModel::MultilingualE5Small,
        pooling: Pooling::Mean,
        download_url: None,
        sha256: None,
    },
    EmbedModelSpec {
        key: "e5-base",
        id: "multilingual-e5-base",
        label: "E5 多语种 · 中（~1GB）",
        model: EmbeddingModel::MultilingualE5Base,
        pooling: Pooling::Mean,
        download_url: None,
        sha256: None,
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

/// user-defined 加载所需的 4 个 tokenizer 文件名（HF 仓库内一致命名）。
const TOKENIZER_FILES: [&str; 4] = [
    "tokenizer.json",
    "config.json",
    "special_tokens_map.json",
    "tokenizer_config.json",
];
const ONNX_FILE: &str = "model.onnx";

/// 自托管按需下载的落地目录（区别于 HF 缓存）：~/.cache/bidguard/embeddings/<id>/。
fn download_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache/bidguard/embeddings"))
}

/// 本地模型（内置随包 + 已下载）的候选基目录，按优先级；每个模型在 <base>/<id>/ 下放 5 个文件。
fn local_model_base_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(p) = std::env::var("BIDGUARD_EMBED_DIR") {
        dirs.push(PathBuf::from(p)); // 测试/内网覆盖
    }
    if let Some(d) = download_dir() {
        dirs.push(d); // 已下载档
    }
    // 随包内置：dev 用 manifest 目录；打包后按平台定位 Resources
    dirs.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("models/embeddings"));
    if let Ok(exe) = std::env::current_exe() {
        if let Some(d) = exe.parent() {
            dirs.push(d.join("models/embeddings"));
            dirs.push(d.join("../Resources/models/embeddings")); // macOS .app
            dirs.push(d.join("../lib/models/embeddings")); // Linux
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

/// 某模型是否有完整的本地文件（内置或已下载）；返回其目录。
fn local_model_dir(spec: &EmbedModelSpec) -> Option<PathBuf> {
    resolve_local(spec.id, &local_model_base_dirs())
}

/// 从本地目录用 user-defined 方式加载（绕过 HF）。pooling 取自 spec 与内置模型对齐，
/// quantization 默认 None、max_length 默认值——与 fastembed 内置 try_new 一致，向量等价。
fn init_local(dir: &Path, spec: &EmbedModelSpec) -> Option<TextEmbedding> {
    let onnx = std::fs::read(dir.join(ONNX_FILE)).ok()?;
    let tk = TokenizerFiles {
        tokenizer_file: std::fs::read(dir.join(TOKENIZER_FILES[0])).ok()?,
        config_file: std::fs::read(dir.join(TOKENIZER_FILES[1])).ok()?,
        special_tokens_map_file: std::fs::read(dir.join(TOKENIZER_FILES[2])).ok()?,
        tokenizer_config_file: std::fs::read(dir.join(TOKENIZER_FILES[3])).ok()?,
    };
    let ud = UserDefinedEmbeddingModel::new(onnx, tk).with_pooling(spec.pooling.clone());
    TextEmbedding::try_new_from_user_defined(ud, InitOptionsUserDefined::new()).ok()
}

/// 递归收集缓存目录下所有文件 (路径, 字节)，最深 5 层。
/// 用 fs::metadata（跟随符号链接）判定类型/大小：HF-hub 缓存把 snapshots/<hash>/.../model.onnx
/// 存为指向 blobs/<sha> 的符号链接，entry.file_type() 不跟随会漏掉这些 onnx（导致误判"未下载"）。
/// 复核模型（engine::rerank）走同一套缓存目录扫描口径，故 pub(crate) 复用而非再写一遍。
pub(crate) fn walk_files(
    dir: &std::path::Path,
    depth: u8,
    out: &mut Vec<(std::path::PathBuf, u64)>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        let Ok(meta) = std::fs::metadata(&p) else { continue };
        if meta.is_dir() {
            if depth > 0 {
                walk_files(&p, depth - 1, out);
            }
        } else if meta.is_file() {
            out.push((p, meta.len()));
        }
    }
}

/// 对文件列表按规范化路径去重后求字节和。
/// HF-hub 缓存里同一 blob 既以真文件 blobs/<sha> 出现、又被 snapshots/.. 符号链接指向，
/// walk_files 跟随符号链接会把同一份字节计两遍；canonicalize 后符号链接与目标 blob
/// 收敛到同一绝对路径，只计一次（detection 用 .any() 不受影响，故仅在求和处去重）。
pub(crate) fn dedup_bytes<'a>(
    files: impl IntoIterator<Item = &'a (std::path::PathBuf, u64)>,
) -> u64 {
    let mut seen = std::collections::HashSet::new();
    files
        .into_iter()
        .filter(|(p, _)| seen.insert(std::fs::canonicalize(p).unwrap_or_else(|_| p.clone())))
        .map(|(_, sz)| *sz)
        .sum()
}

/// 某模型是否在 HF 缓存里（仅联网下载路径；内置/自托管下载走 local_model_dir）。
fn hf_cached_for(spec: &EmbedModelSpec) -> bool {
    let Some(d) = cache_dir() else { return false };
    let id = spec.id.to_ascii_lowercase();
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    files.iter().any(|(p, _)| {
        p.extension().is_some_and(|x| x == "onnx")
            && p.to_string_lossy().to_ascii_lowercase().contains(&id)
    })
}

/// 模型文件是否已在本地就绪（任一模型：内置/已下载/HF 缓存，无需联网即可加载）。
pub fn model_cached() -> bool {
    if MODELS.iter().any(|s| local_model_dir(s).is_some()) {
        return true;
    }
    let Some(d) = cache_dir() else { return false };
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    files.iter().any(|(p, _)| p.extension().is_some_and(|x| x == "onnx"))
}

/// 指定模型是否已就绪：内置/自托管下载（local_model_dir）或 HF 缓存任一即可。
/// 这决定离线闸门（allow_download=false 时能否加载）与工具屏「已缓存」展示。
pub fn model_cached_for(spec: &EmbedModelSpec) -> bool {
    local_model_dir(spec).is_some() || hf_cached_for(spec)
}

/// 指定模型占用字节数（0 = 未就绪）。优先算本地目录（内置/已下载），否则算 HF 缓存。
pub fn model_cache_bytes(spec: &EmbedModelSpec) -> u64 {
    if let Some(dir) = local_model_dir(spec) {
        let mut files = Vec::new();
        walk_files(&dir, 5, &mut files);
        return dedup_bytes(&files);
    }
    let Some(d) = cache_dir() else { return 0 };
    let id = spec.id.to_ascii_lowercase();
    let mut files = Vec::new();
    walk_files(&d, 5, &mut files);
    let matched: Vec<_> = files
        .into_iter()
        .filter(|(p, _)| p.to_string_lossy().to_ascii_lowercase().contains(&id))
        .collect();
    dedup_bytes(&matched)
}

/// 删除指定模型的本地缓存：HF 缓存目录 + 自托管下载目录。返回删除的字节数。
/// 【不删】随包内置档（在只读 Resources 里，是安装包的一部分）。
pub fn clear_model_cache(spec: &EmbedModelSpec) -> u64 {
    let id = spec.id.to_ascii_lowercase();
    let mut removed = 0u64;
    // 自托管下载目录 <download>/<id>/
    if let Some(dl) = download_dir().map(|d| d.join(spec.id)) {
        if dl.is_dir() {
            let mut files = Vec::new();
            walk_files(&dl, 5, &mut files);
            removed += dedup_bytes(&files);
            let _ = std::fs::remove_dir_all(&dl);
        }
    }
    // HF 缓存里名字含 id 的目录
    if let Some(d) = cache_dir() {
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

fn init_model(model: EmbeddingModel) -> Option<TextEmbedding> {
    let mut opts = InitOptions::new(model).with_show_download_progress(false);
    // 稳定的绝对缓存目录：打包后 cwd=/ 不可写，必须显式指定，否则模型加载/下载失败
    if let Some(dir) = cache_dir() {
        opts = opts.with_cache_dir(dir);
    }
    TextEmbedding::try_new(opts).ok()
}

/// 自托管按需下载：拉 spec.download_url 的 .tar（含 model.onnx + 4 个 tokenizer 文件），解到
/// ~/.cache/bidguard/embeddings/<id>/。已就绪（内置/已下载）返回 Ok(0)，否则返回新写入字节数。
/// 无 download_url 的模型返回 Err（可改走 HF 联网，或把文件内置到安装包）。工具屏显式发起 = 授权联网。
pub fn download_model(spec: &EmbedModelSpec) -> Result<u64, String> {
    if local_model_dir(spec).is_some() {
        return Ok(0); // 内置或已下载
    }
    let url = spec
        .download_url
        .ok_or_else(|| "该模型未提供自托管下载源；请启用联网由 HF 拉取，或将其文件内置到安装包".to_string())?;
    let base = download_dir().ok_or_else(|| "无法定位下载目录".to_string())?;
    let dest = base.join(spec.id);
    // .part+rename 原子落盘与 sha256 校验统一走 engine::modelfetch（与复核模型同一条落盘路径）
    let wanted: Vec<&str> = [ONNX_FILE].into_iter().chain(TOKENIZER_FILES).collect();
    let written = crate::engine::modelfetch::fetch_tar_into(url, spec.sha256, &dest, &wanted)?;
    if local_model_dir(spec).is_none() {
        let _ = std::fs::remove_dir_all(&dest); // 半套文件不留，避免被当就位
        return Err("下载归档缺少必需文件（需 model.onnx + 4 个 tokenizer 文件）".to_string());
    }
    Ok(written)
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
        // 优先本地（内置随包 / 已自托管下载）：离线可用，且 pooling 对齐后向量与 HF 版等价。
        let local = local_model_dir(spec);
        // 离线闸门：本地无文件且 HF 也没缓存时，禁联网就降级（不触发下载），守住离线承诺。
        // 用「当前所选模型」判定而非「任一模型」——否则缓存了 A、禁联网选未缓存的 B 会误放行。
        if local.is_none() && !allow_download && !hf_cached_for(spec) {
            return None;
        }
        let loaded = match &local {
            Some(dir) => init_local(dir, spec),
            None => init_model(spec.model.clone()), // HF 缓存命中则加载、否则（allow_download）下载
        };
        if let Some(m) = loaded {
            *slot = Some((spec.id.to_string(), m));
        }
    }
    slot.as_mut().map(|(_, m)| m)
}

/// 用常驻模型嵌入一批文本。前缀按模型家族区分（id 传 spec.id）：
/// E5 系（multilingual-e5-*）对称相似两侧统一加 "query: "；BGE / 其它一律不加前缀
/// （"passage:" 是 E5 专属约定，对 BGE 是噪声会拉低中文精度）。
pub fn embed_batch(model: &mut TextEmbedding, texts: &[String], id: &str) -> Option<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Some(Vec::new());
    }
    let prefix = if id.starts_with("multilingual-e5-") { "query: " } else { "" };
    let docs: Vec<String> = texts.iter().map(|t| format!("{prefix}{t}")).collect();
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

    // 回归：HF-hub 缓存把 snapshots/<hash>/.../model.onnx 存为指向 blobs/<sha> 的符号链接。
    // walk_files 必须跟随符号链接，否则 model_cached_for 漏判→工具屏永远显示「未下载」。
    #[cfg(unix)]
    #[test]
    fn walk_files_follows_symlinked_onnx() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join("bidguard_walk_symlink_test");
        let _ = std::fs::remove_dir_all(&base);
        let blobs = base.join("blobs");
        let snap = base.join("snapshots/h/onnx");
        std::fs::create_dir_all(&blobs).unwrap();
        std::fs::create_dir_all(&snap).unwrap();
        std::fs::write(blobs.join("deadbeef"), vec![0u8; 1234]).unwrap();
        symlink("../../../blobs/deadbeef", snap.join("model.onnx")).unwrap();
        let mut files = Vec::new();
        walk_files(&base, 5, &mut files);
        let onnx = files.iter().find(|(p, _)| p.extension().is_some_and(|x| x == "onnx"));
        assert!(onnx.is_some(), "应通过符号链接发现 model.onnx");
        assert_eq!(onnx.unwrap().1, 1234, "符号链接 onnx 大小应为真 blob 大小");
        let _ = std::fs::remove_dir_all(&base);
    }

    // 回归：跟随符号链接后，同一 blob 既以 blobs/<sha> 真文件、又被 snapshots/.. 符号链接命中，
    // 朴素求和会翻倍（曾导致工具屏显示 ~2x 体积）。dedup_bytes 按规范化路径去重应只计一次。
    #[cfg(unix)]
    #[test]
    fn dedup_bytes_counts_symlinked_blob_once() {
        use std::os::unix::fs::symlink;
        let root = std::env::temp_dir().join("bidguard_dedup_bytes_test");
        let _ = std::fs::remove_dir_all(&root);
        let base = root.join("models--x--bge-small-zh-v1.5");
        let blobs = base.join("blobs");
        let snap = base.join("snapshots/h/onnx");
        std::fs::create_dir_all(&blobs).unwrap();
        std::fs::create_dir_all(&snap).unwrap();
        std::fs::write(blobs.join("deadbeef"), vec![0u8; 1000]).unwrap();
        symlink("../../../blobs/deadbeef", snap.join("model.onnx")).unwrap();
        let mut files = Vec::new();
        walk_files(&base, 5, &mut files);
        let naive: u64 = files.iter().map(|(_, sz)| *sz).sum();
        assert_eq!(naive, 2000, "前置条件：未去重时 blob + 符号链接翻倍");
        assert_eq!(dedup_bytes(&files), 1000, "去重后同一 blob 只计一次");
        let _ = std::fs::remove_dir_all(&root);
    }

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
            MODELS[0].id,
        )
        .expect("应能嵌入");
        let para = cosine(&embs[0], &embs[1]);
        let diff = cosine(&embs[0], &embs[2]);
        assert!(para > diff, "改写句应比无关句更相似：para={para} diff={diff}");
    }

    #[test]
    fn resolve_local_needs_all_five_files() {
        let root = std::env::temp_dir().join(format!("bg_embed_local_{}", uuid::Uuid::new_v4()));
        let dir = root.join("bge-small-zh-v1.5");
        std::fs::create_dir_all(&dir).unwrap();
        let bases = vec![root.clone()];
        // 缺文件 → 不认
        std::fs::write(dir.join("model.onnx"), b"x").unwrap();
        assert!(resolve_local("bge-small-zh-v1.5", &bases).is_none(), "缺 tokenizer 不应就位");
        // 补齐 5 个 → 认
        for f in TOKENIZER_FILES {
            std::fs::write(dir.join(f), b"{}").unwrap();
        }
        assert_eq!(resolve_local("bge-small-zh-v1.5", &bases), Some(dir));
        // 别的 id 仍不认
        assert!(resolve_local("multilingual-e5-large", &bases).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pooling_matches_model_family() {
        // 本地(user-defined)加载必须与 fastembed 内置 pooling 一致，否则向量与 HF 版不可比
        assert_eq!(resolve("bge-zh").pooling, Pooling::Cls);
        assert_eq!(resolve("bge-large-zh").pooling, Pooling::Cls);
        assert_eq!(resolve("e5-large").pooling, Pooling::Mean);
        assert_eq!(resolve("e5-small").pooling, Pooling::Mean);
        assert_eq!(resolve("e5-base").pooling, Pooling::Mean);
    }

    #[test]
    fn resolve_known_and_unknown() {
        assert_eq!(resolve("bge-zh").id, "bge-small-zh-v1.5");
        assert_eq!(resolve("e5-base").id, "multilingual-e5-base");
        assert_eq!(resolve("不存在").key, "bge-zh", "未知值回落默认（MODELS[0]=bge-zh）");
        // 各 id 必须沿用历史值（如 multilingual-e5-small），否则旧缓存按 model_id 失配
        assert_eq!(resolve("e5-small").id, "multilingual-e5-small");
        assert_eq!(resolve("bge-large-zh").id, "bge-large-zh-v1.5");
        assert_eq!(resolve("e5-large").id, "multilingual-e5-large");
    }
}
