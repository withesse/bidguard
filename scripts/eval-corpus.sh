#!/usr/bin/env bash
# 语料回归一键评测（执行方案 §8 W6-5）。
# 快档：无模型层，进 CI，比 baseline_metrics.json（F1/召回/AUC）。
# 慢档：追加语义层（需本地缓存模型，设 BIDGUARD_EMBED_DIR），比 baseline_metrics_full.json（可选）。
# 外部档：用【独立于合成生成器】的人工标注真值语料（fixtures/corpus/external/*.jsonl，
#         或 BIDGUARD_GT_DIR 覆盖为本地非提交数据）评估打分器判别力，出 ROC-AUC/PR-AUC/
#         阈值扫描/ECE/Spearman，比 baseline_metrics_external.json。当前为词面 score_pair 档
#         （无需模型），打破合成指标系统性偏乐观循环。
#
# 用法：
#   scripts/eval-corpus.sh                # 跑两档并打印全表（无模型时慢档自动跳过）
#   scripts/eval-corpus.sh fast           # 仅快档
#   scripts/eval-corpus.sh full           # 仅慢档（需 BIDGUARD_EMBED_DIR）
#   scripts/eval-corpus.sh external       # 仅外部真值档（词面，无需模型）
#   BIDGUARD_WRITE_BASELINE=1 scripts/eval-corpus.sh fast       # 重写快档基线（改算法后）
#   BIDGUARD_WRITE_BASELINE=1 scripts/eval-corpus.sh external   # 重写外部真值基线
#   BIDGUARD_GT_DIR=/path/to/gt scripts/eval-corpus.sh external # 用本地非提交真值数据
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

run_external() {
  echo "==== 外部真值档（词面 score_pair，无需模型）===="
  cargo test --manifest-path "$MANIFEST" --lib --features dev-tools \
    external_calib -- --ignored --exact \
    engine::corpusgen::tests::external_calib --nocapture
}

case "$LANE" in
  fast) run_fast ;;
  full) run_full ;;
  external) run_external ;;
  both) run_fast; run_full ;;
  *) echo "未知参数：$LANE（应为 fast|full|external|both）" >&2; exit 2 ;;
esac
