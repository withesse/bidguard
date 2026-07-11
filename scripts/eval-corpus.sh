#!/usr/bin/env bash
# 语料回归一键评测（执行方案 §8 W6-5）。
# 快档：无模型层，进 CI，比 baseline_metrics.json（F1/召回/AUC）。
# 慢档：追加语义层（需本地缓存模型，设 BIDGUARD_EMBED_DIR），比 baseline_metrics_full.json（可选）。
#
# 用法：
#   scripts/eval-corpus.sh                # 跑两档并打印全表（无模型时慢档自动跳过）
#   scripts/eval-corpus.sh fast           # 仅快档
#   scripts/eval-corpus.sh full           # 仅慢档（需 BIDGUARD_EMBED_DIR）
#   BIDGUARD_WRITE_BASELINE=1 scripts/eval-corpus.sh fast   # 重写快档基线（改算法后）
#   BIDGUARD_EMBED_DIR=/path/to/models scripts/eval-corpus.sh full
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MANIFEST="$REPO_ROOT/src-tauri/Cargo.toml"
LANE="${1:-both}"

run_fast() {
  echo "==== 快档（无模型层，进 CI）===="
  cargo test --manifest-path "$MANIFEST" --lib --features dev-tools \
    corpus_regression -- --exact engine::corpusgen::tests::corpus_regression --nocapture
}

run_full() {
  echo "==== 慢档（追加语义层，需 BIDGUARD_EMBED_DIR）===="
  if [ -z "${BIDGUARD_EMBED_DIR:-}" ]; then
    echo "跳过：未设置 BIDGUARD_EMBED_DIR（指向本地缓存的语义模型目录）。" >&2
    return 0
  fi
  cargo test --manifest-path "$MANIFEST" --lib --features dev-tools \
    corpus_regression_full -- --ignored --exact \
    engine::corpusgen::tests::corpus_regression_full --nocapture
}

case "$LANE" in
  fast) run_fast ;;
  full) run_full ;;
  both) run_fast; run_full ;;
  *) echo "未知参数：$LANE（应为 fast|full|both）" >&2; exit 2 ;;
esac
