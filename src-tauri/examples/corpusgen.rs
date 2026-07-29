//! 合成对抗语料生成器 CLI（dev-tools 门控，不进发布二进制）。
//!
//! 用法：cargo run --example corpusgen --features dev-tools [-- <子命令> [路径]]
//!   （无子命令）      → 段对 pairs.jsonl + 文档集 docsets（全量）
//!   pairs [路径]      → 仅段对语料（默认 fixtures/corpus/pairs.jsonl）
//!   docsets [目录]    → 仅文档集（默认 fixtures/corpus/docsets/ + docsets.jsonl）
//! 固定种子 → 两次运行逐字节一致。
//!
//! 段对：五类标签（same/minor_change/changed/rewrite/unrelated）供 reranker/LR/校准拟合。
//! 文档集：围标正样本组（共享 rsid/模板/图片、清单乘系数、零宽规避）与独立负样本组，供
//! M1 取证 / M2 evasion / M6 数值信号的正负样本评测。
//!
//! 种子来源：默认读仓库合成种子 fixtures/corpus/seeds/*.txt；设 BIDGUARD_CALIB_DIR
//! 指向真实脱敏语料目录可 override。

fn main() {
    bidguard_lib::corpusgen::run_cli();
}
