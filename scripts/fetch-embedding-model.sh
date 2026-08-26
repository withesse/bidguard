#!/usr/bin/env bash
# 拉取随包内置的语义模型 bge-small-zh-v1.5（5 个文件）到 src-tauri/models/embeddings/，
# 每个文件按固定 sha256 校验——被替换/截断的模型会静默扭曲相似度，与 OCR/rerank 同纪律。
#
# 用途：release.yml 打包前置步骤（macOS/Windows runner 的 bash 均可跑）+ 本地打包（见 BUILD.md）。
# 幂等：文件已就位且校验通过则跳过（release 重跑不重下）。
# 模型文件按仓库策略不入 git（见 src-tauri/models/embeddings/README.md）。
#
# HF 直连不可达时（内网/网络管制）：
#   BIDGUARD_HF_BASE=https://hf-mirror.com ./scripts/fetch-embedding-model.sh
# 摘要固定不变，走镜像与走官方源落地内容逐字节一致。
set -euo pipefail

BASE="${BIDGUARD_HF_BASE:-https://huggingface.co}/Qdrant/bge-small-zh-v1.5/resolve/main"
DEST="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/models/embeddings/bge-small-zh-v1.5"

# 本地名|远端名|sha256（2026-08-26 自 HF 官方仓库钉死；onnx 远端名 model_optimized.onnx 即
# fastembed BGESmallZHV15 实际下载的那份，落地统一命名 model.onnx 供 embed::init_local 加载）
FILES=(
  "model.onnx|model_optimized.onnx|1294ea4b6331115a353d81f96b85e8c8d7fdcc284453d5b2fab5b016230aad38"
  "tokenizer.json|tokenizer.json|48cea5d44424912a6fd1ea647bf4fe50b55ab8b1e5879c3275f80e339e8fae26"
  "config.json|config.json|9088751d39abbf86ec3d19ffca92ad62ad19075f7e59712e6c71217fa125d1d3"
  "special_tokens_map.json|special_tokens_map.json|b6d346be366a7d1d48332dbc9fdf3bf8960b5d879522b7799ddba59e76237ee3"
  "tokenizer_config.json|tokenizer_config.json|e6f3b96db926a37d4039995fbf5ad17de158dfb8f6343d607e4dbaad18d75f5a"
)

sha() {
  # macOS 只有 shasum，Git Bash/Linux 只有 sha256sum——两头都认
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$DEST"
for spec in "${FILES[@]}"; do
  IFS='|' read -r local_name remote_name want <<<"$spec"
  path="$DEST/$local_name"
  if [ -f "$path" ] && [ "$(sha "$path")" = "$want" ]; then
    echo "已就位  $local_name"
    continue
  fi
  echo "下载    $remote_name → $local_name"
  curl -fSL --retry 3 -o "$path.part" "$BASE/$remote_name"
  got="$(sha "$path.part")"
  if [ "$got" != "$want" ]; then
    echo "sha256 校验失败：$local_name 期望 $want 实得 $got（已丢弃）" >&2
    rm -f "$path.part"
    exit 1
  fi
  mv "$path.part" "$path"
done
echo "内置语义模型就位：$DEST"
