# 外部真值相似度校准语料（external ground-truth）

本目录收录**独立于合成对抗生成器**的人工标注中文相似度语料，用于评估相似度打分器
本身的判别力与标定质量（ROC-AUC / PR-AUC / 阈值扫描 / ECE / Spearman），打破
「合成语料生成器与检测器同源 → 指标系统性偏乐观」的循环（见 `engine::corpusgen` §8
风险①、`engine::extcalib`）。

与生成器语料（`../pairs.jsonl` / `../docsets/`）的关键区别：这里的语料**不进** committed
语料的 `pairs_hash` / `docsets_hash` 确定性门禁，作为独立文件、独立基线接入，且**不是**
生成器的输入种子（`BIDGUARD_CALIB_DIR` 只影响生成器种子，与此无关）。

## 格式

每个 `<dataset>.jsonl`，一行一条 `ExternalPair`（见 `engine::extcalib::ExternalPair`）：

```json
{"text_a": "...", "text_b": "...", "label": 1.0, "source": "pawsx-zh"}
```

- `label`：归一化到 `[0,1]` 的真值相似度。二分类集用 `0.0` / `1.0`；分级集（如 STS-B 0–5）
  应在数据准备阶段除以满分归一后写入。
- `source`：来源数据集标识，供报告分组与许可声明。

## 数据集来源与许可

### pawsx-zh.jsonl
- **来源**：PAWS-X（Cross-lingual Paraphrase Adversaries from Word Scrambling），中文（`zh`）
  子集，`test` 划分。经 HuggingFace `datasets-server` 抽样前 300 条
  （`dataset=google-research-datasets/paws-x&config=zh&split=test&offset=0..300`）。
- **内容**：句对 + 二分类标签。`label=1` 为释义（语义等价，高相似）；`label=0` 为
  **对抗硬负样本**——表面词汇高度重叠但语义不同（如主宾互换）。正是压测「打分器会否把
  表面雷同误判为高相似」的关键，对应真实场景中「同模板标书表面雷同但非串标」的误报风险。
- **许可**：PAWS/PAWS-X 官方 LICENSE 原文——“The dataset may be freely used for any
  purpose, although acknowledgement of Google LLC ("Google") as the data source would
  be appreciated.” 可自由使用/再分发；来源致谢 Google LLC。
  原始 LICENSE：https://github.com/google-research-datasets/paws/blob/master/LICENSE
- **抽样规则**：`test` 划分按原始顺序取前 300 条，未打乱、未再筛选（正 123 / 负 177）。
- **致谢**：Yang et al., 2019, *PAWS-X: A Cross-lingual Adversarial Dataset for
  Paraphrase Identification*（EMNLP-IJCNLP 2019）。

## 已测得的结论（阈值 0.7）

三档打分：`lexical` = 无语义 `score_pair`；`fused` = 启用语义的生产融合分；
`cosine` = 裸 bge-small-zh-v1.5 嵌入余弦。

| scorer | ROC-AUC | PR-AUC | P@0.7 | R@0.7 | bestF1 | bestThr | ECE | Spearman |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| lexical | 0.576 | 0.500 | 0.477 | 0.415 | 0.591 | 0.120 | 0.231 | 0.129 |
| fused | 0.575 | 0.512 | 0.443 | 0.756 | 0.590 | 0.390 | 0.343 | 0.127 |
| cosine | 0.565 | 0.495 | 0.414 | 0.976 | 0.592 | 0.769 | 0.495 | 0.112 |

**语义维没有带来判别力提升**：三档 ROC-AUC 都在 0.57 附近（随机为 0.5），正类率 0.41 时
PR-AUC 也都在 0.50 上下。PAWS-X 正是为击穿「词袋 + 双编码器」而构造的——对抗负样本词汇
几乎完全重叠，仅靠论元顺序区分语义，而检索型双编码器对语序本就不敏感。

对 BidGuard 的含义：**若两份标书的雷同段落只在主宾/实体调换上有别，词面与 bge-small 语义
都区分不了**。同时注意 `cosine` 在 0.7 处 R=0.976 / P=0.414——裸余弦把几乎所有对判为正，
ECE 0.495 说明它完全不能当概率读；其最优阈值在 0.769。

**限定**：本结论只覆盖 bge-small-zh-v1.5（默认档）与 PAWS-X 这一对抗基准，不能外推为
「语义维无用」——语义维的价值本在同义改写检出（对应引擎 `rewrite` 标签），而 PAWS-X 恰恰
不测那个，且更大的 bge-large-zh 未测。

## 补充数据（未收录，说明边界）

LCQMC / BQ 等问句匹配集判别力对词面档参照价值高，但其数据使用协议（哈工大深圳等）
要求签署、**不宜直接再分发**，故不 commit；如需使用应走运行期本地读取（`BIDGUARD_GT_DIR`），
数据自备、不入库。
