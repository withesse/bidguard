# 内置 embedding 模型（随安装包）

放这里的模型会**随安装包内置**，语义查重即可开箱离线可用（无需联网下载）。
运行时通过 `embed::init_local` 以 fastembed 的 user-defined 方式加载，pooling 与内置模型对齐后，
向量与「HF 联网下载版」**逐位等价**（见 `src-tauri/src/engine/embed.rs`）。

## 目录约定

每个模型一个子目录，目录名 = 该模型的 `id`（见 `embed.rs` 的 `MODELS`），内含 **5 个文件**：

```
models/embeddings/<id>/
  ├── model.onnx                 # ONNX 权重
  ├── tokenizer.json
  ├── config.json
  ├── special_tokens_map.json
  └── tokenizer_config.json
```

内置档为 `bge-small-zh-v1.5`（~90MB，`MODELS` 里 key=`bge-zh` 的默认档）——**语义比对默认开启
依赖它**（rewrite/洗稿分类需要语义维；模型缺失时降级纯词面并置 `semantic_degraded`）。

## 如何取得这 5 个文件（脚本，固定 sha256）

```bash
./scripts/fetch-embedding-model.sh     # 幂等；HF 不可达时 BIDGUARD_HF_BASE=https://hf-mirror.com
```

脚本从 HF `Qdrant/bge-small-zh-v1.5` 拉取 fastembed 同款文件（onnx 远端名 `model_optimized.onnx`，
落地统一 `model.onnx`），逐文件按脚本内钉死的 sha256 校验后才落位。`release.yml` 打包前自动执行；
本地打包前手动跑一次即可。

> ⚠️ 摘要即契约：必须是 fastembed 实际下载的那份 onnx/tokenizer（脚本保证），勿从别处找同名
> 模型——否则 tokenizer/权重细节不同会导致向量与下载版不可比（DB 里按
> `(normalized_hash, model_id)` 缓存的向量会串味）。升级模型版本时重钉脚本内摘要并 bump `id`。

## 大模型（bge-large / e5-*）

体积 1~2GB，不建议内置（安装包爆炸、macOS 公证慢）。两种分发：
- **HF 联网**：默认已支持（`allowCloudModel=true` 时工具屏预下载）。
- **自托管下载**：把上面 5 个文件打成一个 `.tar`，传到你可控的 URL，填到 `embed.rs` 对应
  `EmbedModelSpec.download_url`；工具屏「下载」即走该源（离线内网友好，不依赖 HF）。

## 这些文件为什么不入库

模型是大二进制、可再生资源，不适合进 git。仓库只放本说明；打包前按上面步骤放入实际文件。
（`.gitignore` 应忽略本目录下的 `*.onnx` 等，仅保留 README——按你的仓库策略配置。）
