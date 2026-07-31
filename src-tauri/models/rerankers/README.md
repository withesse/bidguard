# 内置复核模型（cross-encoder，W6-2）

放这里的模型会**随安装包内置**，「交叉复核」开箱离线可用（无需联网下载）。
运行时通过 `rerank::init_local` 以 fastembed 的 user-defined 方式加载
（见 `src-tauri/src/engine/rerank.rs`）。

复核层是**默认关闭**的可选层：它只给「待复核」条款一个**复核排序建议分**，
**不改判条款分类**（cross-encoder 是黑盒且为检索相关性训练，「相关」≠「同源改写」）。
不内置也不影响比对——模型缺失时比对照常完成，`summary.rerankDegraded=true`。

## 目录约定

每个模型一个子目录，目录名 = 该模型的 `id`（见 `rerank.rs` 的 `RERANK_MODELS`），
内含 **5 个文件**（与 embedding 内置档同构）：

```
models/rerankers/<id>/
  ├── model.onnx                 # ONNX 权重（默认档为 int8 量化，~300MB）
  ├── tokenizer.json
  ├── config.json
  ├── special_tokens_map.json
  └── tokenizer_config.json
```

默认档 `bge-reranker-base-int8`（`RERANK_MODELS` 里 key 同名）。
**int8 量化档只能走内置或自托管来源**：HF 回落拿到的是 fp32 权重（体积更大、CPU 推理慢 2–6 倍）。

## 三级来源与优先级

1. `BIDGUARD_RERANK_DIR` 指向的目录（测试 / 内网覆盖）；
2. 自托管按需下载：`~/.cache/bidguard/rerankers/<id>/`（工具箱「复核模型」卡片触发）；
3. 本目录（随包内置）与打包后的 Resources 副本；
4. 以上都没有时，回落 HF 联网下载 —— 受 `security.allowCloudModel` 闸门约束，
   且**比对期一律不隐式下载**（下载不可取消，会让「取消比对」卡死）。

## 自托管下载源与完整性校验

把上面 5 个文件打成一个 `.tar`，URL 填进 `RERANK_MODELS[i].download_url`，
并把该 `.tar` 的 sha256 填进 `sha256`。下载走 `engine::modelfetch`：
先写 `<name>.part` 再 rename（原子落盘），读完整流校验摘要，**摘要不符即整目录丢弃并报错**。
模型是判读结论的上游，被截断或被替换的权重会让查重结论不可举证 —— 宁可不可用，不可用错的模型出结论。

`sha256` 留空则只保证原子落盘、不做完整性校验（内网自建归档的过渡形态）。
