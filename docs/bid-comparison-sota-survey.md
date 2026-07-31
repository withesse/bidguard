# 标书比对方案与算法全景调研（SOTA Survey）

> 调研日期：2026-07-05 · 范围：不受 BidGuard 现有实现约束，记录学界与工业界当前最优方案
>
> 方法：7 个维度并行网络调研 → 每维度关键声明由独立核查员反向验证 → 完整性批评 → 4 轮补漏调研。
> 共 19 个研究代理、457 次工具调用（以检索为主）、约 40 条关键声明经核查。
>
> 配套文档：[bid-comparison-scheme.md](bid-comparison-scheme.md)（BidGuard 现行方案与 v2 落地路线）。
> 本文是"上限参考"，落地取舍以 scheme 文档为准。

## 目录

1. 执行摘要
2. 调研方法与阅读提示
3. 近重复与文本重用检测
4. 语义相似与洗稿改写检测
5. 文档取证与作者归属
6. 围标串标筛查（非文本信号）
7. 文档对齐与证据定位
8. 证据融合、校准与不确定性
9. LLM 时代的文档比对流水线
10. 完整性批评：识别出的遗漏方向
11–14. 补漏调研（清单数值比对 / 均值基准价机制 / 对抗规避 / 合法共享剔除）

## 1. 执行摘要

### 1.1 三个范式级发现

**（一）所有纯文本比对方案有共同死穴：默认"抽取到的文本 = 文档真实内容"。** 该假设已被系统性攻破：零宽字符/同形字扰动（Bad Characters, IEEE S&P 2022——1~3 个不可见字符即可击穿商用 NLP 系统）、PDF 字体 ToUnicode 重映射（渲染给评标人一套、抽给查重系统另一套）、字符坐标乱序（PDFuzz 2025——检测器准确率从 93.6% 打到 50.4%，视觉零变化）。更好的方案必须有**入口对抗层**：渲染后 OCR ↔ 文字层交叉验证，不一致本身即"刻意规避"的强证据。经典查重文献不覆盖此维度。

**（二）报价筛查必须"评标办法感知"，否则反向误导。** 经典 Imhof 方差/CV 筛查只对最低价类评标成立（串标 → 报价收紧 → CV 下降）。中国综合评分法普遍以"投标均价×系数"为基准价，与意大利均值拍卖同构——此机制下卡特尔最优策略是投极端陪衬报价把基准价拉向自家，报价方差反而变大，同一套 CV 阈值会把串标判成"竞争激烈"。正确算法：Conley & Decarolis 成组检验 + **反事实基准价重算**（剔除嫌疑组报价后按招标文件公式重算基准价，看中标人是否翻转）——单项目 2–5 份标书即可运行，产出可直接引用的法务证据。

**（三）若干"常识选型"已被证伪。** SimHash 已被边缘化（AISTATS 2014 理论 + 实测均输给 MinHash，2023–2026 主流数据管线无一采用）；扫描件解析上专用小模型碾压通用大模型（OmniDocBench v1.6：MinerU2.5-Pro 1.2B 得 95.75 > Gemini 3 Pro 92.91 > GPT-5.2 86.59）；"全靠 LLM 逐对比对"既贵又不准（PAWS 类对抗改写上通用 LLM 仅略高于抛硬币，正确位置是终审仲裁）。

### 1.2 理想架构（证据强度分层）

```
入口层   对抗检测（OCR×文字层交叉验证、Unicode/同形字归一、隐藏文字层审计）
解析层   docx 原生解析（保元数据）｜PDF/扫描件 → MinerU2.5 / PaddleOCR-VL（带 bbox）
剥离层   招标文件对减（winnowing 指纹）＋ 范本背景库 ＋ k-共现过滤 → 只比"残差文本"
  ├── 铁证层   后缀数组 ExactSubstr：输出逐字重复区间（起止位置，零概率误报）
  ├── 洗稿层   bi-encoder 召回 + cross-encoder 精判（NEWS-COPY：93.7 vs LSH 73.7 ARI）
  ├── 成型层   seed–chain–align（minimap2 范式）+ 带状 Smith-Waterman → 连续雷同段+覆盖率
  ├── 取证层   rsid / PDF 血缘 GUID / 图片 PDQ 哈希 / 文体计量 / 共同错误
  ├── 数值层   清单逐项雷同率、组价链一致性、机制感知报价筛查、数字分布检验
  └── 终审层   LLM 仲裁（低温+成对交换+引证强校验），只处理过滤后的边界段对
融合层   各通道 → 似然比（LR）+ 逻辑回归校准 → 共形预测三带输出（放行/人工/标红）
```

### 1.3 各层最优选型速览

| 层 | 首选方案 | 关键依据 |
|----|---------|---------|
| 入口对抗层 | OCR×文字层交叉验证 + Unicode/同形字归一 | Bad Characters (S&P 2022)、PDF Mirage、PDFuzz (2025) |
| 解析层 | docx 原生解析 + MinerU2.5 / PaddleOCR-VL | OmniDocBench v1.6：95.75 vs GPT-5.2 86.59 |
| 剥离层 | 招标文件对减 (winnowing) + k-共现过滤 + SemDeDup 语义豁免 | MOSS/JPlag 20+ 年实战范式 |
| 铁证层 | 后缀数组 ExactSubstr | Lee et al., ACL 2022；零概率误报、可指认区间 |
| 洗稿层 | bi-encoder (BGE-M3) + cross-encoder 重排 | NEWS-COPY：ARI 93.7 vs LSH 73.7 |
| 成型层 | seed–chain–align + 带状 SW；Vecalign/Bertalign | minimap2 范式；线性时空句对齐 |
| 取证层 | rsid、PDF 血缘 GUID、PDQ 图片哈希、共同错误、文体计量 | 司法级证据；条例第 40 条法定情形 |
| 数值层 | 逐项雷同率、组价链一致性、机制感知筛查、数字分布检验 | 地方法定 80% 线；C&D 反事实检验 |
| 行为层（平台级） | screens+ML、GAT、共同投标网络 | 正确分类率 84–95%；GAT 8 市场均值 91% |
| 融合层 | 似然比 LR + 逻辑回归校准 + 共形预测三带 | 说话人识别事实标准；ICLR 2024 CRC |
| 终审层 | LLM 仲裁（成对交换、低温、引证强校验） | PlagBench：GPT-4 级近满分 vs 商用工具略高于随机 |

### 1.4 对 scheme 文档 v2 路线的修正

v2 路线中凭经验提出的方向（rsid、共同错误、报价向量、段落对齐、cross-encoder、合成语料）全部获独立佐证；新增四个此前遗漏的方向，按重要性排：

1. **对抗规避威胁模型**（入口必修，v2 完全未提）；
2. **机制感知报价筛查**（均值基准价下经典方差筛查会反向失效）；
3. **招标文件对减**应从"模板标记"升格为独立的前置剥离阶段；
4. **共形预测**替代拍脑袋的"uncertain 带"阈值（给出可审计的漏检率保证）。

### 1.5 政策窗口

八部委发改法规〔2026〕195 号（2026-02）将"围串标识别"列为招投标 AI 20 个推广场景之一（场景 17：商务标报价特征比对＋技术方案语义相似性分析），要求 2026 年底部分省市全覆盖、2027 年底全国推广，并明确"模型生成的结论不替代招标人自主判断""严防算法歧视和模型幻觉"——"文本+取证+数值"三层架构正在成为官方钦定形态。

## 2. 调研方法与阅读提示

- 第 3–9 章为七个维度的调研发现，每章末尾附**核查记录**：CONFIRMED（找到原始来源支持）/ REFUTED（找到相反证据）/ UNCLEAR（查不到可靠来源）。
- **正文个别数字被核查驳回的，以核查记录中的修正为准**（正文保留原样存档）。已知修正：PAN 2025 交叉评测用的旧数据集是 PAN 2012（非 2015）；批不变算子确定性实验用的模型是 Qwen3-235B-A22B（非 Qwen3-8B）；Nugroho et al. (EMSE 2020) 的结论范围是"推荐代码变更用 --histogram"，非全面速度/质量占优。
- 第 10 章为完整性批评发现的遗漏方向，第 11–14 章为对应补漏调研。**补漏部分未经第二轮独立核查**，引用其中数字时建议自行溯源。
- 各 finding 的"成熟度"分级：工业级（有生产部署）/ 研究级（论文验证）/ 竞赛级（评测任务）。

## 3. 近重复与文本重用检测

### MinHash + LSH（n-gram shingle 近重复检测，LLM 数据管线事实标准）

- **成熟度**：工业级（datasketch/text-dedup/gaoya/Spark 实现成熟，万亿 token 级验证）
- **原理**：把文档切成 n-gram 集合，用 MinHash 签名近似 Jaccard 相似度，再用 LSH banding 把两两比较降为近似 O(n) 的桶内碰撞检测。
- **为什么更好**：相比逐对精确 Jaccard，规模从 O(n^2) 降到近线性；相比 SimHash，在高相似区（正是查重关心的区间）检出更准——Shrivastava & Li (AISTATS 2014) 理论+实验证明二值数据下 MinHash 几乎全面优于 SimHash；RETSim 论文的 NEWS-COPY 基准上 MinHash ARI 0.737 vs SimHash 0.695。FineWeb (2024)、BigCode/BigScience、RefinedWeb 等主流 LLM 预训练数据管线全部选 MinHash-LSH 而非 SimHash。经典但仍是工程首选（文档级近重复）。
- **标书场景用法**：标书场景 2-5 份交叉比对时不需要 LSH 索引——直接对段落/章节做两两 MinHash 或精确 Jaccard 即可，粒度建议'段落级 + 字符 3~5-gram shingle'（中文可免分词，字符 n-gram 对中文更稳；也可 jieba 分词后 word-gram）。用 Jaccard containment（而非对称 Jaccard）处理长短段落不对称。若做成平台对历史标书库全量查重，才引入 LSH/LSHBloom。
- **参考**：
  - Shrivastava & Li, In Defense of MinHash Over SimHash, AISTATS 2014, https://arxiv.org/abs/1407.4416
  - FineWeb (NeurIPS 2024 D&B) 采用 MinHash-LSH，https://kili-technology.com/blog/fineweb2-dataset-guide
  - ChenghaoMou/text-dedup（BigCode/BigScience 衍生，Apache 2.0），https://github.com/ChenghaoMou/text-dedup

### 后缀数组精确重复串检测 ExactSubstr（Lee et al. 2022）

- **成熟度**：工业级（google-research/deduplicate-text-datasets 开源 Rust 实现，被 RefinedWeb 等大量管线复用）
- **原理**：将全部文档拼接后建后缀数组，线性时间找出所有长度超过阈值（原文 50 token）的逐字重复子串，输出精确的重复区间。
- **为什么更好**：相比 n-gram 指纹只给'相似分数'，它给出可指认的逐字雷同证据（起止位置+原文），且无概率误报；相比朴素两两比较的 O(n^2)，后缀数组线性构建。Lee et al. 用它发现 C4 中一句 61 词的句子重复 6 万+次；已成为 LLM 数据去重的标准组件（Google deduplicate-text-datasets Rust 工具，支持 64-bit 超大语料）。经典但仍是首选（逐字雷同层）。
- **标书场景用法**：标书比对的'铁证层'：把 2-5 份标书拼接建后缀数组（或对每对文档建广义后缀自动机求所有长公共子串），阈值设 20-40 个汉字，直接输出'A 第 3.2 节与 B 第 3.2 节存在 800 字逐字相同'这类可写进评标报告的证据。这是围标认定最有说服力的文本证据，应作为流水线第一层。
- **参考**：
  - Lee et al., Deduplicating Training Data Makes Language Models Better, ACL 2022, https://arxiv.org/abs/2107.06499
  - https://github.com/google-research/deduplicate-text-datasets

### RETSim / UniSim（Google 神经字符指纹）

- **成熟度**：工业级偏研究（Google 开源 google/unisim，MIT 协议，ICLR 2024 论文；有 PyTorch 移植，但生态远小于 MinHash）
- **原理**：RETVec 字符级编码 + 小型 Transformer 产出 256 维度量嵌入，把近重复检测变成向量近邻检索，天然抗错字、同形字替换、空格插入等对抗性小改动。
- **为什么更好**：对'刻意规避检测的小改动'和 OCR 噪声远比 n-gram 指纹稳健：NEWS-COPY 上 ARI 0.831 vs MinHash 0.737、SimHash 0.695；在其 W4NT3D 多语言对抗近重复基准上宣称 SOTA；且无需像 MinHash 那样重调 n-gram/阈值参数。多语言（含中文）单模型覆盖。
- **标书场景用法**：标书场景两个痛点正中其靶：扫描件 OCR 噪声、投标人故意做微改写（改标点/空格/同义字符）躲避字面查重。做法：段落级切块过 RETSim 得嵌入，两两余弦（或 USearch 近邻）出相似对，再回到原文用对齐算法定位。可作为 MinHash 之上的'抗规避层'。
- **参考**：
  - Bursztein et al., RETSim: Resilient and Efficient Text Similarity, ICLR 2024, https://arxiv.org/abs/2311.17264
  - https://github.com/google/unisim

### 神经双编码器 + 交叉编码器重排去重（Silcock et al., NEWS-COPY）

- **成熟度**：研究级（NBER/OpenReview 论文+开源代码与数据，方法论已被工业界吸收，但无现成产品化工具）
- **原理**：对比学习训练的 bi-encoder 先召回候选相似对，再用 cross-encoder 精判，最后图聚类成重复簇——把去重当检索+重排问题。
- **为什么更好**：在含 OCR 噪声与改写转载的 NEWS-COPY 基准上：bi-encoder ARI 91.5、重排法 93.7，对比 LSH 73.7、n-gram overlap 75.0——是'哈希被神经方法大幅超越'的最干净证据；bi-encoder 单 GPU 可扩展到千万文档。
- **标书场景用法**：洗稿改写层的直接模板：用中文模型（BGE-M3/bge-large-zh 做双编码器 + bge-reranker 做交叉编码器）复刻该两级架构，对标书段落做召回+精判，能抓住'换句式、调语序、同义替换'的洗稿段落，这是 MinHash/后缀数组完全漏掉的部分。2-5 份文档规模下交叉编码器可全量两两跑，精度天花板最高。
- **参考**：
  - Silcock, D'Amico-Wong, Yang & Dell, Noise-Robust De-Duplication at Scale, 2022/2023, https://arxiv.org/abs/2210.04261
  - https://github.com/dell-research-harvard/NEWS-COPY

### Winnowing 指纹（MOSS，Schleimer et al. SIGMOD 2003）

- **成熟度**：工业级（MOSS 服务运行 25+ 年；实现极简，各语言均有）
- **原理**：对 k-gram 哈希序列开滑动窗口、每窗取最小哈希作指纹，保证任何长于阈值 t 的匹配至少留下一个共同指纹，且指纹带位置信息。
- **为什么更好**：与随机采样指纹相比有'≥t 的匹配必被检出'的理论保证，指纹自带位置可直接高亮证据；20+ 年后仍是代码抄袭检测的语法层 SOTA 基线（2025-2026 论文如 LLM 代码溯源仍以 winnowing 为核心组件/基线）。经典但仍是首选（需要位置化指纹、增量入库场景）；纯改写检测上已被嵌入方法取代。
- **标书场景用法**：标书平台化时的折中方案：对每份入库标书存 winnowing 指纹（k≈15 汉字、窗口 w≈10），新标书来时查倒排指纹表即得'与库内哪份的哪个位置雷同'，兼顾存储、速度与证据定位；单次 2-5 份比对则不如后缀数组直接。对报价清单、施工组织设计等半结构化文本尤其合适。
- **参考**：
  - Schleimer, Wilkerson & Aiken, Winnowing: Local Algorithms for Document Fingerprinting, SIGMOD 2003, https://theory.stanford.edu/~aiken/publications/papers/sigmod03.pdf

### PAN 文本对齐 seed-extension-filter 框架 → PAN 2025 嵌入分块范式

- **成熟度**：seed-extension：工业级（各查重系统内核）；嵌入分块检测 LLM 改写：竞赛级/研究级（PAN 2025 仅 4 队参赛，指标未成熟）
- **原理**：经典范式：先用共同 n-gram/句子相似作'种子'，再贪心/动态规划（Smith-Waterman 类）向两侧扩展成最大对齐段，最后过滤重叠；PAN 2025 起转为'分块嵌入相似+相邻块合并'检测 LLM 改写。
- **为什么更好**：seed-extension 是唯一能输出'源段落↔嫌疑段落'成对对齐区间（plagdet>0.8，PAN 2014 冠军 Sanchez-Perez）的成熟框架，这正是查重报告需要的形态；PAN 2025 的教训是：面对 LLM 改写，字面法失效，嵌入法 recall 可达 0.8 但 precision 仅 ~0.5（最佳基线 Linq-Embed-Mistral plagdet 0.61），且所有 2025 方法在 2015 字面数据集上反而大幅掉分——结论：字面层与语义层必须并联，谁也不能单独替代谁。
- **标书场景用法**：标书比对的'证据成型层'：无论种子来自后缀数组命中、winnowing 指纹还是嵌入相似块，都用 seed-extension 把碎片合并成连续雷同段并计算段级重复率；对疑似 LLM 代写/改写的标书，用中文嵌入分块 + 相邻合并作补充，但要接受 precision 有限、需人工复核。
- **参考**：
  - Sanchez-Perez et al., PAN 2014 text alignment 冠军, https://www.researchgate.net/publication/282725592
  - Overview of the Plagiarism Detection Task at PAN 2025, https://arxiv.org/html/2510.06805v1
  - Overview of PAN 2026（含 Generative Plagiarism Detection）, https://arxiv.org/pdf/2602.09147

### 带保证的子序列级对齐：Allign（SIGMOD 2021）与 MONO（2025，加权 Jaccard）

- **成熟度**：研究级（VLDB/SIGMOD 系论文，无生产化开源生态）
- **原理**：把 min-hash 从'整文档'下沉到'所有子序列'：按子序列 min-hash 分组建索引，保证召回所有估计 Jaccard 超阈值的相似片段；MONO 用 consistent weighted sampling 扩展到 TF-IDF 加权 Jaccard 并证明分组数 O(n+n log f) 最优。
- **为什么更好**：相比 seed-extension 的启发式（无召回保证、参数难调），这类方法有理论召回保证；MONO 对比前代 hash 对齐方法建索引快至 26x、索引小 30%、查询快 3x。加权 Jaccard 可压低'投标须知、规范条文'等公共模板词的权重，专抓真正可疑的雷同。
- **标书场景用法**：若自研标书查重内核且要'漏检可解释、召回有保证'，这是替代启发式 seed-extension 的前沿路线；加权思想可立即借用：对标书语料统计 IDF，把法定模板句（招标文件抄录、国标条文）降权，显著减少误报——这是标书场景最大的实际噪声源。
- **参考**：
  - Feng & Deng, Allign: Aligning All-Pair Near-Duplicate Passages in Long Texts, SIGMOD 2021
  - Zhang, Qiao, Peng & Deng, Near-Duplicate Text Alignment under Weighted Jaccard Similarity, 2025, https://arxiv.org/abs/2509.00627

### LSHBloom（每 band 一个 Bloom filter 的 MinHashLSH 替身）

- **成熟度**：研究级偏工业（PVLDB 论文，Argonne 出品，尚无广泛生产部署报告）
- **原理**：用轻量 Bloom filter 替换 MinHashLSH 昂贵的桶索引，每个签名 band 独立进 Bloom filter 做成员测试。
- **为什么更好**：peS2o 3900 万文档上比标准 MinHashLSH 快 12x（3 小时 vs 37+ 小时）、磁盘省 18x（11GB vs 200GB），误报率可控（可压到 1e-10 量级），外推可行至 50 亿文档——代表 2024-2025 '极限规模去重'方向：LSH 索引本身正在被替换。
- **标书场景用法**：仅当标书产品演进为省级/全国级平台（对千万级历史标书做全量互查）才需要；单项目 2-5 份比对完全用不上，列入以备架构演进参考。
- **参考**：
  - Khan et al., LSHBloom: Internet-Scale Text Deduplication, 2024, https://arxiv.org/abs/2411.04257

### SimHash（Charikar 2002 / Google 2007 网页去重）——已被边缘化

- **成熟度**：工业级但地位衰退（中文互联网工程博客仍常推荐，属惯性）
- **原理**：对特征加权投影到 64-bit 指纹，用汉明距离≤k 判近重复。
- **为什么更好**：反向结论：它是'被替代者'。优点仅剩指纹极小（64 bit/文档）、汉明距离查询快；但高相似区检出弱于 MinHash（AISTATS 2014 理论证明；NEWS-COPY 上 0.695 vs 0.737 ARI），对短文本和中文近义改写更不敏感。2023-2026 主流 LLM 数据管线（FineWeb、Dolma、BigCode、RefinedWeb）无一选 SimHash 做近重复层——Dolma 选 Bloom filter 做精确层、FineWeb 选 MinHash 做近重复层。
- **标书场景用法**：标书场景不建议新采用：文档少不需要省内存，其唯一优势不成立；仅当需要给海量历史库存一个 8 字节级指纹做粗筛时才考虑。谁被替代——本维度答案之一就是 SimHash 被 MinHash（近重复层）和神经指纹（抗改写层）夹击替代。
- **参考**：
  - Shrivastava & Li, AISTATS 2014, https://arxiv.org/abs/1407.4416
  - RETSim 论文 NEWS-COPY 对比表, https://arxiv.org/abs/2311.17264

### 中国电子招投标'清标/雷同性分析'实战组合（文本+元数据特征码）

- **成熟度**：工业级（省级公共资源交易平台普遍部署，2024-2025 多省通报案例）
- **原理**：评标系统对投标文件同时比对文本重复率与 8 类非文本特征码：制作机器码（MAC/CPU/硬盘/主板序列号）、文件创建标识码、上传 IP、上传时间、报价规律性、工程量清单错误一致性等。
- **为什么更好**：实战检出率远高于纯文本查重：围标团伙常互抄电子标书或同一代理制作，机器码/IP 一致是监管认可的直接认定依据（多省公共资源交易平台与政府采购电子系统已内置风险预警），而纯文本雷同还需解释'是否抄招标文件模板'。这是被中国监管实战验证的最有效组合，学术文献几乎不覆盖。
- **标书场景用法**：BidGuard 类产品的差异化要点：docx/pdf 元数据提取（作者、公司、创建/修改时间、修订记录、生产者软件指纹）+ 文本雷同层并联输出；扫描件走 OCR 后仅剩文本层，故元数据层要对'可提取文件'尽量榨取。报告按'逐字雷同段 / 改写相似段 / 元数据巧合'三级呈现，对应监管认定强度。
- **参考**：
  - 福建省发改委：电子招投标中认定围标串标的 8 种方式（2025-09）, https://fgw.fujian.gov.cn/ztzl/cjzxjyzx/zfjzcg/202509/t20250919_7012524.htm
  - 贵州公共资源交易：科技助力围标串标治理（2024-05）, https://ggzy.guizhou.gov.cn/zhdt/szzxdt/202405/t20240523_84674197.html

### 中文语义嵌入层：BGE / BGE-M3（C-Pack, BAAI）

- **成熟度**：工业级（FlagEmbedding 开源，国内 RAG 事实标准）
- **原理**：C-MTEB 上 SOTA 的中文/多语嵌入模型族，BGE-M3 支持稠密+稀疏+多向量三模式检索，用于段落级语义相似召回。
- **为什么更好**：在 C-MTEB（31 数据集）中文语义任务上长期领先，是 Silcock 式 bi-encoder 架构在中文的现成替换件；BGE-M3 的稀疏(lexical)+稠密混合打分对'半改写'文本比纯稠密更稳。相比通用多语模型（如 RETSim 的 256 维小模型），中文改写捕捉能力显著更强，但算力成本更高。
- **标书场景用法**：洗稿层召回器：标书段落 → BGE-M3 嵌入 → 两两余弦 + 稀疏分混合 → 过阈值段落对交给 bge-reranker 或 LLM 复核。与项目现有 bge-large-zh 技术栈直接兼容。
- **参考**：
  - Xiao et al., C-Pack: Packed Resources for General Chinese Embeddings, SIGIR 2024, https://arxiv.org/abs/2309.07597
  - Chen et al., BGE M3-Embedding, 2024, https://arxiv.org/abs/2402.03216

### 核查记录（近重复与文本重用检测）

- **[CONFIRMED]** RETSim(ICLR 2024,arXiv 2311.17264)在 NEWS-COPY 上 ARI 0.831,高于 MinHash 0.737 与 SimHash 0.695,MIT 协议开源于 github.com/google/unisim
  - 核查依据：论文 Table 3 载 RETSim(Partial-Dup) 0.831、MinHash*(引自 Silcock et al.)0.737、SimHash 0.695(论文自调 MinHash 为 0.783),摘要写明 MIT 协议开源于 github.com/google/unisim,ICLR 2024 poster 与 proceedings 均收录(来源:arxiv.org/abs/2311.17264、iclr.cc/virtual/2024/poster/19560)。
- **[CONFIRMED]** Silcock et al.《Noise-Robust De-Duplication at Scale》:NEWS-COPY 上 bi-encoder ARI 91.5、bi+cross re-rank 93.7、LSH 73.7、n-gram overlap 75.0
  - 核查依据：论文原文(arXiv 2210.04261,第 2 页及 Table 2)逐字写明 'ARI for the re-rank model is 93.7 and for the bi-encoder model is 91.5, versus 73.7 for LSH and 75.0 for N-gram overlap',四个数字完全吻合(来源:arxiv.org/pdf/2210.04261)。
- **[CONFIRMED]** Lee et al. (ACL 2022) ExactSubstr 用后缀数组、50-token 阈值,发现 C4 中一句 61 词英文句子重复超 6 万次,官方 Rust 工具为 google-research/deduplicate-text-datasets
  - 核查依据：ACL Anthology 收录为 2022.acl-long.577,摘要写明 'a single 61 word English sentence that is repeated over 60,000 times' 从 C4 移除,官方仓库 google-research/deduplicate-text-datasets 核心为 Rust 实现的后缀数组,README 写明阈值 50 tokens(即 100 bytes)(来源:aclanthology.org/2022.acl-long.577、github.com/google-research/deduplicate-text-datasets)。
- **[REFUTED]** PAN 2025 生成式抄袭检测:最佳基线 Linq-Embed-Mistral plagdet 约 0.61(recall 0.82 / precision 0.58),且所有 2025 参赛方法在 2015 经典字面数据集上显著掉分
  - 核查依据：基线数字属实(overview 论文 arXiv 2510.06805:Linq micro plagdet 0.61 / recall 0.82 / precision 0.58,优于所有提交)且掉分现象属实,但交叉评测用的旧数据集是 PAN 2012(原文:'Table 3 shows the same results on the old PAN12 dataset…All submissions (except the original PAN12 baseline) face a significant drop in performance'),并非 2015 数据集,该细节与来源矛盾(来源:arxiv.org/html/2510.06805v1)。
- **[CONFIRMED]** LSHBloom(arXiv 2411.04257)在 peS2o(3900 万文档)上比 MinHashLSH 快 12 倍、磁盘占用少 18 倍(11GB vs 200GB)
  - 核查依据：论文原文写明 peS2o 约 39M 文档、'LSHBloom takes just three hours, whereas MinHashLSH takes over 37 hours—over a 12× speedup'、'LSHBloom requires just 11 GB of disk while MinHashLSH uses over 200 GB—an 18× reduction',数字完全吻合(来源:arxiv.org/html/2411.04257)。

## 4. 语义相似与洗稿改写检测

### BGE-M3 三模混合检索（dense + sparse/lexical + multi-vector）

- **成熟度**：工业级（BAAI 开源，MIT 协议，RAG 生态广泛部署，有 ONNX/GGUF 移植）
- **原理**：单一模型同时输出稠密语义向量、可解释的词级稀疏权重（类 BM25）和 ColBERT 式多向量，三路得分加权融合，通过 self-knowledge distillation 联合训练；支持 100+ 语言、单段最长 8192 token。
- **为什么更好**：在 MIRACL（18 语种，nDCG@10）上 dense 单路 67.8（对比 mE5-large 65.4），三路混合达 70.0；一个模型同时抓住『字面抄袭』（sparse 命中相同措辞）和『洗稿改写』（dense/multi-vector 命中语义），而传统 BM25+单向量方案需要两套系统且对改写召回差。
- **标书场景用法**：标书切成段落/条款后，每段同时算三路表示：sparse 得分高+dense 得分高 = 直接抄袭；sparse 低但 dense/multi-vector 高 = 洗稿改写嫌疑。三路得分差本身就是『改写程度』的信号，可直接作为围标证据分级依据。中文标书是其训练主力语种之一，8192 token 覆盖标书长段落。
- **参考**：
  - BGE M3-Embedding: Multi-Lingual, Multi-Functionality, Multi-Granularity Text Embeddings Through Self-Knowledge Distillation (arXiv:2402.03216, 2024)
  - https://huggingface.co/BAAI/bge-m3

### 新一代 LLM-based 中文/多语种 embedding（Qwen3-Embedding、NV-Embed-v2、Conan-embedding、QZhou-Embedding 等）

- **成熟度**：工业级（Qwen3-Embedding 全系 Apache 2.0，0.6B/4B/8B 三档，0.6B 可本地 CPU/端侧跑；Conan/QZhou 部分闭源或权重受限）
- **原理**：以 7B/8B 级 LLM 为底座、经两阶段对比学习+合成数据训练的通用 embedding，语义区分力显著超过 BERT 系（bge-large-zh 等）小模型。
- **为什么更好**：Qwen3-Embedding-8B 发布即登顶 MTEB multilingual 榜（2025-06-05，70.58 分）；NV-Embed-v2 曾以 72.31 登顶英文 MTEB（2024-08-30）并提出 latent attention pooling；C-MTEB 中文榜 2025 年头部为 Seed 系、Qwen 系（8B/4B）、Conan v1/v2（腾讯）、QZhou-Embedding（2025-08 技术报告称同时登顶 MTEB 与 C-MTEB）。相比 bge-large-zh 一代，对『同义改写但换句式』的段落 cosine 区分度明显更高。
- **标书场景用法**：作为标书比对的第一层召回引擎：全部标书段落入向量库，跨标书 ANN 检索 top-k 相似段对。本地部署选 Qwen3-Embedding-0.6B（速度）或 4B/8B（精度）；云端在线可直接用 8B。注意 embedding 召回阈值只做候选，不做结论——模板化标书段落天然相似，需后续层过滤。
- **参考**：
  - Qwen3 Embedding: Advancing Text Embedding and Reranking Through Foundation Models (arXiv:2506.05176, 2025)
  - https://qwenlm.github.io/blog/qwen3-embedding/
  - NV-Embed: Improved Techniques for Training LLMs as Generalist Embedding Models (arXiv:2405.17428, ICLR 2025)
  - QZhou-Embedding Technical Report (arXiv:2508.21632, 2025)

### Late interaction：ColBERTv2/PLAID 与 Jina-ColBERT-v2（多语种）

- **成熟度**：工业级边缘/研究级之间（ColBERTv2+PLAID 学术工程成熟、Vespa/Qdrant 已支持 multi-vector；Jina-ColBERT-v2 权重 CC-BY-NC，商用需 API 或授权）
- **原理**：查询与文档均保留 token 级向量，用 MaxSim 逐 token 对齐打分——相关性由『哪些词对上了哪些词』累加而来，天然产出对齐证据。
- **为什么更好**：ColBERTv2 残差压缩把多向量存储降 6-10 倍并保持域内外 SOTA；PLAID 引擎再把检索延迟降 GPU 7x / CPU 45x（1.4 亿段规模下 GPU 数十毫秒）；Jina-ColBERT-v2 支持 89 语种（含中文）、8192 token，检索质量比原版 ColBERTv2 高 6.5%、存储再省 50%。相比单向量 cosine 只给一个分数，late interaction 能指出具体哪些 token/短语构成相似，这正是查重取证需要的。
- **标书场景用法**：用于标书段对的『对齐热力图』：对 embedding 召回的嫌疑段对跑 token 级 MaxSim，高亮双方互相对上的词块，直接生成给评标专家看的雷同证据标注（改写处会呈现『词不同但一一对齐』的模式）。也可作第二层重排。
- **参考**：
  - ColBERTv2: Effective and Efficient Retrieval via Lightweight Late Interaction (NAACL 2022, arXiv:2112.01488)
  - PLAID: An Efficient Engine for Late Interaction Retrieval (CIKM 2022, arXiv:2205.09707)
  - Jina-ColBERT-v2: A General-Purpose Multilingual Late Interaction Retriever (arXiv:2408.16672, MRL@EMNLP 2024)

### Cross-encoder 重排/成对判定（bge-reranker-v2-m3、Qwen3-Reranker）

- **成熟度**：工业级（RAG 两阶段标准架构；均开源可本地部署）
- **原理**：把两段文本拼接后送入同一模型联合编码直接输出相似/相关分数，token 间全交叉注意力，成对判别精度远高于双塔 cosine。
- **为什么更好**：bge-reranker-v2-m3（基于 bge-m3，约 0.6B，Apache 2.0）在 BEIR/CMTEB-retrieval/MIRACL 重排 top-100 均稳定提升；Qwen3-Reranker-8B 在 CMTEB-R 达 77.45。对 PAWS 类对抗样本（词面高度重叠但语义不同，或语义同但词面全换）双塔模型系统性失分，cross-encoder 是目前修正这类误判的标准手段。
- **标书场景用法**：作为标书比对第二层：embedding 召回的候选段对全部过 cross-encoder 打分，显著压低『模板段落误报』和『改写漏报』。可用 PAWS-X 中文子集 + 自建标书改写对微调成专用『是否同源改写』二分类器（bge-reranker 系列支持微调）。
- **参考**：
  - https://huggingface.co/BAAI/bge-reranker-v2-m3
  - Qwen3 Embedding 技术报告 (arXiv:2506.05176, 2025)
  - PAWS-X: A Cross-lingual Adversarial Dataset for Paraphrase Identification (EMNLP 2019, arXiv:1908.11828)

### LLM-as-judge 改写/抄袭终审（few-shot CoT 提示的 GPT-4 级模型）

- **成熟度**：研究级 benchmark + 工业级实践（判定 pipeline 需自建；国内智能评标产品已在用大模型做雷同复核）
- **原理**：把嫌疑段对交给大模型，用少样本思维链提示判定『是否同源、属直抄/改写/概括哪一类』并输出理由。
- **为什么更好**：PlagBench（2024，46,500 例）显示 GPT-4 Turbo few-shot CoT 下二分类接近满分、verbatim/paraphrase 类型判别 99-100%，而商业查重工具（GPTZero、Prepostseo）仅比随机猜高约 10%；LLM 同时给出可读理由，这是任何相似度分数都做不到的。弱点：概括式抄袭易与改写混淆、判分存在过度自信问题。
- **标书场景用法**：标书场景第三层（终审层）：仅对前两层筛出的高嫌疑段对调用（成本可控），输出结构化结论——改写类型、共同的事实性细节（相同错别字、相同异常数字、相同人名电话）、置信度与理由，直接生成评标专家可复核的报告。中文用 Qwen/DeepSeek 级模型或云端 Claude/GPT 均可。
- **参考**：
  - PlagBench: Exploring the Duality of Large Language Models in Plagiarism Generation and Detection (arXiv:2406.16288, 2024)
  - An Empirical Study of LLM-as-a-Judge for LLM Evaluation (arXiv:2403.02839, 2024)

### PAN 2025 Generative Plagiarism Detection 的经验（LLM 洗稿检测竞赛）

- **成熟度**：竞赛级（CLEF/PAN 2025 shared task，数据与方法公开）
- **原理**：在成段被 Llama/DeepSeek-R1/Mistral 改写的文档对中定位被改写段并回对源段——与『标书洗稿检测』几乎同构的任务设定。
- **为什么更好**：竞赛结论：朴素的 embedding 段向量相似度方法效果最好，可达 recall≈0.8 / precision≈0.5，胜过复杂方案——证明『段落 embedding + 对齐』检测 LLM 改写是当前可行上限的合理近似；但同一批方法在 PAN 2015 老式人工改写数据上显著劣化，说明泛化差、需按目标域（标书）自建校准集。
- **标书场景用法**：两点直接可搬：(1) 检测 LLM 洗稿不需要花哨模型，段级 embedding 相似度 + 双向对齐即可打底；(2) 必须自建『标书改写对』评测集（用多个 LLM 对真实标书段落做改写生成正样本、用同项目不同投标人合规段落做负样本）来定阈值，否则 precision 会失控。
- **参考**：
  - Overview of the Plagiarism Detection Task at PAN 2025 (arXiv:2510.06805, CLEF 2025)
  - https://pan.webis.de/clef25/pan25-web/generated-plagiarism-detection.html

### MinHash/LSH + SimHash + n-gram 指纹（经典但仍是首选的第一道粗筛）

- **成熟度**：工业级（datasketch、text-dedup 等成熟库；经典但仍是首选）
- **原理**：文档/段落做 shingling 后用局部敏感哈希近似 Jaccard 相似度，近线性复杂度筛出高字面重合对。
- **为什么更好**：至今仍是 LLM 训练数据万亿 token 级去重的默认方案（RefinedWeb、SlimPajama、GPT-3 等均用），2025 年对比评测显示 MinHash/LSH 在五种去重算法中综合最佳；对 2-5 份标书这种小规模是毫秒级，且结果可精确复现、可举证（重合 n-gram 可直接列出）。缺陷：对同义替换/句式重排（洗稿）几乎无召回，必须与语义层配合。
- **标书场景用法**：标书比对第 0 层：全量段落两两 MinHash 粗筛，直抄段落秒级定位并列出重合片段作硬证据；同时给语义层『排除已确认直抄段』减少重复计算。对扫描件需先 OCR，OCR 噪声下可放宽 shingle 归一化或改用字符级 SimHash。
- **参考**：
  - Evaluation of Document Deduplication Algorithms for Large Text Corpora (LOD 2024/2025, Springer)
  - https://zilliz.com/blog/data-deduplication-at-trillion-scale-solve-the-biggest-bottleneck-of-llm-training

### 中国围串标智能检测的工业实践（多维证据融合 + 引用内容过滤）

- **成熟度**：工业级（全国公共资源交易平台多地已上线）
- **原理**：国内评标系统的实战架构：语义指纹/cosine 相似度（常用 85% 阈值预警）+ 自动过滤『来自招标文件的引用内容』（通用承诺、资质证书等）+ 元数据/设备指纹（文档属性、MAC、机器码）+ 股东关联穿透，多维交叉定性。
- **为什么更好**：纯文本相似度在模板化标书上误报严重；过滤招标文件引用段后再比『投标人自撰内容』是国内系统降误报的关键工程经验。实战数字：佛山案例风险识别从 2 天压缩到 1.1 天、串通预警准确率超 80%、曾查出多家投标人文档相似率 89.42%；雄安围绕围串标建 11 项预警指标。政策面：发改委等八部门《关于加快招标投标领域人工智能推广应用的实施意见》（发改法规〔2026〕195号）列 20 个场景，要求 2026 年底围串标识别等场景在部分省市全覆盖。
- **标书场景用法**：直接采纳其两条工程铁律：(1) 比对前先用招标文件本身做『白名单语料』，凡与招标文件高相似的段落剔除出比对范围；(2) 文本相似只是围标证据之一，最终报告应与元数据（docx 作者/公司字段、创建时间、修订记录）、报价规律等并列呈现，让文本层证据可被交叉印证。
- **参考**：
  - 国家发改委发改法规〔2026〕195号 https://www.ndrc.gov.cn/xxgk/zcfb/tz/202602/t20260210_1403680.html
  - https://zhuanlan.zhihu.com/p/2012878716632593588
  - https://www.tocheck.cn/skills/3376.html

### AI 生成文本零样本检测（Fast-DetectGPT、Binoculars）

- **成熟度**：研究级（学术 SOTA 明确，但对最新商用大模型和中文域的实战可靠性未验证）
- **原理**：Fast-DetectGPT 用条件概率曲率（一次前向采样替代词比较条件概率）判定文本是否机器生成；Binoculars 用观察者/执行者双模型的 perplexity 与 cross-perplexity 之比做零样本判定。
- **为什么更好**：Fast-DetectGPT 比 DetectGPT 快 340 倍、相对精度提升约 75%（ICLR 2024）；Binoculars 零训练即在 0.01% 误报率下检出 >90% 的 ChatGPT 文本，优于 GPTZero/Ghostbuster（ICML 2024）。但硬限制：Binoculars 对 GPT-4 文本漏检率曾达 58.13%（2024-03 数据）、低资源/非英语语种召回低；RAID 基准显示同义替换攻击可使此类方法掉 36.1% 精度，PADBen（2025）证实改写攻击对两者均显著降级。中文标书场景还需换中文底座模型（如 Qwen 系）算困惑度，目前无权威中文标书域评测。
- **标书场景用法**：用途应严格限定为『混淆因子标注』而非证据：对每份标书段落打 AI 生成可能性分，(1) 两份标书对应段落都高度疑似 AI 生成时，将其相似度证据降权（可能是各自用 AI 写的巧合相似）；(2) 反之，人写特征明显却互相高度相似的段落权重上调。绝不可把『AI 生成』本身当围标结论输出。
- **参考**：
  - Fast-DetectGPT: Efficient Zero-Shot Detection of Machine-Generated Text via Conditional Probability Curvature (ICLR 2024, arXiv:2310.05130)
  - Spotting LLMs With Binoculars: Zero-Shot Detection of Machine-Generated Text (ICML 2024, arXiv:2401.12070)
  - RAID: A Shared Benchmark for Robust Evaluation of Machine-Generated Text Detectors (ACL 2024)
  - PADBen (arXiv:2511.00416, 2025)

### 『AI 代写导致无串通也相似』的甄别方法论（基线校准 + 稀有特征证据）

- **成熟度**：工业级方法论（国内评标系统的过滤实践 + 取证语言学经典原则的组合，无单一开源实现）
- **原理**：不看绝对相似度，而看『超出本项目全体投标人基线的异常相似』+ 只有抄袭/串通才能解释的稀有共同特征（相同错别字、相同异常数字、相同联系人、相同排版怪癖、相同 docx 元数据）。
- **为什么更好**：各投标人独立用 AI 工具（DeepSeek/钛投标等已普及）会天然产出结构和措辞相近的标书，业界已明确此类内容雷同根源是工具而非串通，评标端也在自动过滤通用承诺等段落防误伤；统计学上，全项目 pairwise 相似度分布做基线、只报显著离群对（z-score/分位数），可把『AI 同质化』这一系统性抬升从个别对的异常中分离出来。稀有特征（低概率共同错误）是文本取证（forensic linguistics）里区分『独立生成巧合』与『同源复制』的经典判据，AI 各自生成几乎不可能复现相同的错别字和相同的具体事实性错误。
- **标书场景用法**：落成三条规则：(1) 相似度报告一律以『本项目所有段对相似度分布』为基线，只报显著离群；(2) 单独跑『稀有共同特征扫描』——共同错别字、相同电话/人名/项目经历、相同非常规数字，命中即为强证据且不受 AI 同质化干扰；(3) 报告区分三档结论：直抄（字面证据）、疑似改写（语义+对齐证据）、AI 同质化可能（双方均高 AI 分且无稀有共同特征），把最终定性留给评标专家。
- **参考**：
  - https://zhuanlan.zhihu.com/p/1962185982360150068（AI写标书与串标认定的业界讨论）
  - 国家发改委 2026 年 AI 招投标实施意见（对通用内容过滤的官方要求）
  - PADBen/RAID 对检测器局限的证据（arXiv:2511.00416; ACL 2024）

### 改写检测评测基准体系（PAWS-X / PARAPHRASUS / PADBen）

- **成熟度**：研究级（评测资源，非产品）
- **原理**：PAWS-X 提供中文在内 6 语种的对抗性改写判别对（词面几乎相同但语义不同）；PARAPHRASUS（2024）证明单一基准会高估改写检测模型；PADBen（2025）专测改写攻击下的检测鲁棒性。
- **为什么更好**：PAWS 类对抗样本正是标书场景的两大误判源的镜像：模板段落『词同义不同』（应判不相似）与洗稿段落『义同词不同』（应判相似）。在 PAWS-X 上，简单 embedding cosine 接近失效，多语 BERT 微调后中文子集准确率约 83-91%，当前 SOTA 为 ByT5 XXL——量化说明为什么标书查重不能只靠 cosine 阈值，必须叠加成对判别层。
- **标书场景用法**：用 PAWS-X zh 作为选型试金石：候选 embedding/reranker/LLM-judge 在其上先跑分，再叠加自建的『标书 LLM 改写对』测试集（参照 PAN 2025 造数方式，用多个中文 LLM 改写真实标书段落），形成本域验收标准后才上线阈值。
- **参考**：
  - PAWS-X: A Cross-lingual Adversarial Dataset for Paraphrase Identification (EMNLP 2019, arXiv:1908.11828)
  - PARAPHRASUS: A Comprehensive Benchmark for Evaluating Paraphrase Detection Models (arXiv:2409.12060, COLING 2025)
  - PADBen (arXiv:2511.00416, 2025)

### 核查记录（语义相似与洗稿改写检测）

- **[CONFIRMED]** Qwen3-Embedding-8B 于 2025-06-05 发布时以 70.58 分排名 MTEB multilingual 榜第一，全系（0.6B/4B/8B，含 Qwen3-Reranker）Apache 2.0 开源；Qwen3-Reranker-8B 在 CMTEB-R 达 77.45
  - 核查依据：Qwen 官方博客原文：8B embedding 'ranks No.1 in the MTEB multilingual leaderboard (as of June 5, 2025, score 70.58)'，全系列以 Apache 2.0 开源（qwenlm.github.io/blog/qwen3-embedding/）；HF 模型卡评测表明确列出 Qwen3-Reranker-8B CMTEB-R=77.45（0.6B=71.31、4B=75.94），license apache-2.0（huggingface.co/Qwen/Qwen3-Reranker-8B）。
- **[CONFIRMED]** BGE-M3 在 MIRACL（18 语种，nDCG@10）上 dense 单路 67.8，dense+sparse+multi-vector 三路混合提升至 70.0（arXiv:2402.03216）
  - 核查依据：arXiv:2402.03216 Table 2（MIRACL dev set，18 语种，nDCG@10）平均分与声明完全一致：Dense 67.8、Sparse 53.9、Multi-vec 69.0、Dense+Sparse 68.9、All（三路混合）70.0（arxiv.org/html/2402.03216v3）。
- **[CONFIRMED]** Binoculars（ICML 2024）零样本在 0.01% FPR 下检出 >90% ChatGPT 文本，但论文 Table 8 显示对 GPT-4 文本漏检率 58.13%，且在保加利亚语/乌尔都语/俄语/阿拉伯语等非英语语种召回低——中文场景可用性需独立验证
  - 核查依据：论文（arXiv:2401.12070，ICML 2024，GitHub 标注 [ICML 2024]）摘要称 0.01% FPR 下检出 >90% ChatGPT 样本；附录 A.10 Table 8 显示对 GPT-4 文本 False Negative Rate 为 58.13%；论文用 M4 数据集测保加利亚语/乌尔都语/俄语/阿拉伯语，原文称 'low recall in all four languages'、机器文本常被误判为人类（arxiv.org/html/2401.12070v2）；论文确未测中文，'需独立验证'为合理推论。
- **[CONFIRMED]** PAN 2025 Generative Plagiarism Detection 竞赛中，基于 embedding 相似度的朴素方法最高达 recall 0.8 / precision 0.5，且这些方法在 PAN 2015 旧数据上显著劣化（arXiv:2510.06805）
  - 核查依据：arXiv:2510.06805（Overview of the Plagiarism Detection Task at PAN 2025）摘要原文：'naive semantic similarity approaches based on embedding vectors provide promising results of up to 0.8 recall and 0.5 precision'，且 'most of these approaches underperform significantly on the 2015 dataset, indicating a lack in generalizability'（arxiv.org/abs/2510.06805）。
- **[CONFIRMED]** PlagBench（arXiv:2406.16288）称 GPT-4 Turbo few-shot CoT 在改写抄袭二分类上接近满分、verbatim/paraphrase 类型判别 99-100%，而 GPTZero/Prepostseo 等商业工具仅比随机高约 10%
  - 核查依据：PlagBench 论文正文（arxiv.org/html/2406.16288v1）：GPT-4 Turbo（与 Llama3-70b-instruct）few-shot CoT 二分类 'achieve near-perfect performance'（约 99%），'identification of verbatim and paraphrase plagiarism achieve 99-100% accuracy'，商业检测器 GPTZero/Prepostseo 'roughly 10% higher performance than random guessing'（类型分类总体准确率仅 56.91%/58.39%）；注意近满分结论同时属于 Llama3-70b，非 GPT-4 Turbo 独有。

## 5. 文档取证与作者归属

### OOXML rsid 修订标识符同源取证（rsid/rsidRoot 交集匹配）

- **成熟度**：工业级/司法级（韩国法庭案例论文、中国有 rsid 检测专利 CN106203135A 与电子数据司法鉴定规范体系；国内评标系统的"文件创建标识码"比对即同类信号）
- **原理**：Word 2007+ 每次"编辑并保存"会话向 docx 的 settings.xml <w:rsids> 写入一个随机 rsid，rsidRoot 标记首次编辑会话；两份 docx 若共享任一 rsid（尤其 rsidRoot），说明二者派生自同一母文件或同一编辑链。
- **为什么更好**：与文本相似度完全正交：洗稿改写、同义替换都不会清除 rsid，一次交集命中即近乎确凿的同源硬证据，而文本查重只能给出百分比。Joun (2021, Journal of Forensic Sciences) 用真实案件验证可据此重建文件流转与人际关系网；只要有一个未被清除格式的字符残留即可判定同源。注意 Spennemann & Singh (IJDC 2024) 实验表明 rsid 数值并非严格递增，不能仅凭数值大小推断编辑先后，但"共享即同源"结论不受影响。
- **标书场景用法**：对每份 docx 解压读取 word/settings.xml 的全部 rsid 集合，N 份标书两两求交集；交集非空立即标红为"同一母版/同一编辑链"，rsidRoot 相同判"同一原始文档派生"。对 PDF 标书无效，需配合 PDF 侧信号。反取证提示：另存为/选择性粘贴到新文档可洗掉 rsid，因此 rsid 无命中不能排除串标。
- **参考**：
  - Joun et al., Relevance analysis using revision identifier in MS Word, Journal of Forensic Sciences, 2021, doi:10.1111/1556-4029.14584
  - Spennemann & Singh, The Generation of Revision Identifier (rsid) Numbers in MS Word: Implications for Document Analysis, International Journal of Digital Curation 18(1), 2024, https://ijdc.net/index.php/ijdc/article/view/870
  - Didriksen, Forensic Analysis of OOXML Documents (thesis), https://www.semanticscholar.org/paper/753b09eeaecd588449493b0449a2bbfc895705b2
  - 专利 CN106203135A 针对rsid隐藏信息的无源检测方法, https://patents.google.com/patent/CN106203135A/zh

### docx 元数据+生成环境指纹（core.xml/app.xml、Template、zip 条目结构与时间戳、机器码）

- **成熟度**：工业级（国内电子评标平台标配；知也云等商用标书查重系统覆盖文字/表格/图片/文档属性等12类比对维度）
- **原理**：docProps/core.xml 的 creator/lastModifiedBy、app.xml 的 Application/Template/Company/TotalTime、zip 条目顺序与内部时间戳（Word 写 1980-01-01 占位）共同构成文件生成环境指纹；电子招投标平台还会记录制作机器的 MAC 地址、CPU 序列号、硬盘序列号、CA 锁/计价软件加密锁序列号。
- **为什么更好**：这是中国清标环节实战验证最多、证明力被法规直接背书的一类信号：《招标投标法实施条例》第40条把"不同投标人的投标文件由同一单位或个人编制/异常一致"列为视为串通投标，多地规定"文件制作机器码一致"可直接认定同一单位编制，"创建标识码一致"交评标委员会综合评判；实现成本几乎为零，一致即强线索。相比纯文本查重可发现零文本重叠的串标（同一台电脑做了两家标书）。
- **标书场景用法**：解析每份 docx 的 core.xml/app.xml 全字段做两两相等性矩阵：author、lastModifiedBy、company、Template（非 Normal.dotm 的自定义模板名相同=强信号）、revision、totalTime、created/modified 时间接近度；再比较 zip 条目序列哈希（part 排列顺序是生成器指纹）。注意仅"创建码一致"可能是无意使用同一源文件（如招标代理下发的模板），需按码的类型分级：机器码一致>模板GUID一致>作者名一致。
- **参考**：
  - 投标文件创建码或机器码一致，可以认定为串标吗？, https://m.caigou2003.com/article/yllzc?articleId=721020440851513345
  - 电子招投标中认定"围标串标"的8种方式, 福建省发改委, https://fgw.fujian.gov.cn/ztzl/cjzxjyzx/zfjzcg/202509/t20250919_7012524.htm
  - Fu et al., Forensic investigation of OOXML format documents, Digital Investigation, 2011, https://www.researchgate.net/publication/220346075
  - 出现这些情况直接判定围标串标, https://www.sohu.com/a/752734186_121123686

### PDF coding-style 生成器指纹（不依赖元数据的 producer 检测）

- **成熟度**：研究级（论文+YARA 规则方法明确，可直接工程化复刻；无现成商业产品）
- **原理**：不同 PDF 生成器在 header 第二行二进制注释（producer magic number，如 Word 固定 %\xB5\xB5\xB5\xB5、Distiller 0xE2E3CFD3）、对象键排列顺序、转义风格、xref 有无、trailer 键集合（如 Word 特有双 trailer+/XRefStm、LibreOffice 特有 /DocChecksum）上留下稳定的"编码风格"，可在元数据被清除或伪造后仍识别生成工具乃至操作系统。
- **为什么更好**：metadata（Producer/Creator）用 exiftool 一条命令即可改掉，而 coding style 深嵌文件结构、极难伪装。Adhatarao & Lauradoux (2021) 用 192 条 YARA 规则在 508,836 份真实 PDF 上测试：LibreOffice 与 PDFLaTeX 识别 100%，Word 与 macOS Quartz >90%，总体 74%，并借此揭穿多个在线转换服务谎报自研引擎。对标书场景：两家公司 PDF 的 coding style 相同而 metadata 却声称不同软件 → 元数据被人为清洗的直接证据。
- **标书场景用法**：对每份 PDF 提取四段指纹：header 二进制注释、首个流对象的键序/转义模式、xref 形态、trailer 键集合序列；两两比较"结构指纹向量"，同一指纹+接近的创建时间+相同字体集 → 判同一制作环境。与 XMP 对照可发现元数据篡改（style 说是 Word、metadata 说是 WPS）。国产软件（WPS、永中）需自建规则库，这是本场景最值得补的工程点。
- **参考**：
  - Adhatarao & Lauradoux, Robust PDF Files Forensics Using Coding Style, arXiv:2103.02702, 2021, https://arxiv.org/pdf/2103.02702

### PDF XMP/DocInfo 血缘追踪（DocumentID/InstanceID/DerivedFrom/History + trailer /ID + 字体子集标签）

- **成熟度**：工业级（eDiscovery/司法取证常规项；解析只需 exiftool/pikepdf）
- **原理**：XMP 的 xmpMM:DocumentID 是标识"同一资源所有版本"的 GUID，InstanceID 每次保存更新，xmpMM:DerivedFrom 直接记录母文档的 DocumentID/InstanceID，xmpMM:History 记录编辑步骤；trailer /ID 首半部分在创建时生成且再保存不变；内嵌子集字体的 6 字母前缀（如 ABCDEF+SimSun）由生成器伪随机产生。
- **为什么更好**：GUID 碰撞概率趋近于零：两份"互相独立"的标书 PDF 若 DocumentID 相同、或 DerivedFrom 指向同一 GUID、或 trailer /ID 前半相同、或出现完全相同的字体子集标签+相同字形子集，即近乎确凿地证明同一母文件/同一次生成导出——这是单点即可定案的证据，文本相似度做不到。eDiscovery 行业（Meridian Discovery 等）已作为标准鉴定项；PDF Association 2025 年专门发布了 PDF 取证与元数据研讨材料。
- **标书场景用法**：提取每份 PDF 的 DocInfo+XMP 全字段与 trailer /ID、全部字体名及子集前缀，建立两两比较矩阵；命中规则分级：DocumentID/DerivedFrom/trailer-ID 相同=同一母文件（最高级）；CreatorTool+Producer+字体集完全一致+创建时间相近=同一制作环境（中级）；XMP 与 DocInfo 互相矛盾=有清洗痕迹（可疑）。注意元数据可被抹除，无命中不等于清白。
- **参考**：
  - PDF Forensic Analysis and XMP Metadata Streams, Meridian Discovery, https://www.meridiandiscovery.com/articles/pdf-forensic-analysis-xmp-metadata/
  - PDF Forensics & the Metadata Conundrum, PDF Association, 2025, https://pdfa.org/wp-content/uploads/2025/10/0-2-15_30-CherieEkholm-PDF_Forensics_and_the_Metadata_conundrum.pdf
  - PDF Metadata Forensics: A Complete Field-by-Field Reference, https://htpbe.tech/blog/pdf-metadata-fields-complete-reference

### 内嵌图片同源检测：精确哈希 + Meta PDQ 感知哈希 + 深度 copy-move 检测

- **成熟度**：SHA/PDQ 工业级；深度 copy-move 研究级偏竞赛级（基准数据集与真实标书图片分布有差距）
- **原理**：抽取 docx/PDF 内嵌图片后三层匹配：字节级 SHA-256 精确匹配 → PDQ（256 位 DCT 感知哈希，Meta 2019 年 8 月开源，Hamming 距离 ≤31 判相似，随机对期望距离 128）抗压缩/缩放/轻裁剪的近重复匹配 → 深度 copy-move/splicing 检测定位图内拼接篡改（如 P 掉公司名的资质证书复用）。
- **为什么更好**：两家标书共用同一张施工现场照片、同一张扫描版资质证书是围标的高证明力信号，且完全绕开文本比对。PDQ 相对经典 pHash 增加质量评分与更强的规范化，是 Meta 生产环境（NCMEC 图片匹配）实战验证的工业方案，faiss 索引下约 4000 张/秒可扩展到全库比对；copy-move 深度方法（Deep Cross-Scale PatchMatch 2023、object-level inconsistency mining 2024）在 CASIA v2 等基准上准确率 96-99%，能发现"同图微改"这种人工几乎查不出的复用。
- **标书场景用法**：解析 word/media/ 与 PDF XObject 提取全部图片（含 EXIF：相机型号/序列号/GPS/拍摄时间相同=同一台设备拍摄的加分项）；全库建 PDQ+faiss 索引做跨投标人近重复检索；对命中对再跑 copy-move 定位篡改区域，输出可视化证据图。扫描章、签字页图片是重点比对对象。
- **参考**：
  - facebook/ThreatExchange PDQ, https://github.com/facebook/ThreatExchange/blob/main/pdq/README.md
  - PDQ & TMK+PDQF — A Test Drive of Facebook's Perceptual Hashing Algorithms, arXiv:1912.07745, 2019
  - Image Copy-Move Forgery Detection via Deep Cross-Scale PatchMatch, arXiv:2308.04188, 2023
  - Object-level Copy-Move Forgery Image Detection based on Inconsistency Mining, arXiv:2404.00611, 2024

### 扫描件同源痕迹：扫描仪传感器噪声指纹（PRNU/sensor pattern noise）+ 打印痕迹

- **成熟度**：研究级偏司法级（相机 PRNU 司法成熟，扫描仪场景论文验证充分但缺开箱即用工具；对低分辨率/强压缩扫描件性能下降明显）
- **原理**：扫描仪 CCD/CIS 传感器制造差异产生的 PRNU 噪声是设备唯一指纹，可从扫描图像中估计参考噪声模式并做相关匹配，判定两份扫描件是否出自同一台扫描仪；同理打印文档的墨迹/半色调纹理可归属打印机。
- **为什么更好**：对全扫描件标书（文本层不可比）这是少数能证明"物理同源"的手段：两家公司的盖章扫描页出自同一台扫描仪，直接指向同一操作场所。相机 PRNU 在司法上已被广泛接受，扫描仪版本方法（Purdue Khanna et al. 2007 起）在受控实验中识别率高；比只做 OCR 后文本比对多出一个不可伪造维度。
- **标书场景用法**：对各标书的扫描页（尤其证书页、签字盖章页）按扫描仪流程估计噪声残差，两两做归一化互相关聚类；聚成同簇的不同投标人 → 同一扫描设备。工程上建议作为二级人工复核信号而非自动定罪信号；同时比对扫描页的分辨率/边距/歪斜角/色彩曲线等弱指纹。
- **参考**：
  - Khanna et al., Scanner Identification Using Sensor Pattern Noise, SPIE, 2007, https://www.cerias.purdue.edu/assets/pdf/bibtex_archive/PSI65051K.pdf
  - Khanna et al., Source scanner identification for scanned documents, WIFS, 2009, https://www.researchgate.net/publication/224101491
  - Source Camera Identification with a Robust Device Fingerprint survey, 2023, https://pmc.ncbi.nlm.nih.gov/articles/PMC10490695/

### 文体计量：字符 n-gram 距离 + Burrows Delta（经典但仍是首选）

- **成熟度**：工业级（经典但仍是首选；stylo 等成熟工具链）；中文需自配分词或用字符级
- **原理**：Burrows Delta（2002）对最高频词/虚词的 z-score 频率向量求曼哈顿距离度量文体差异；字符 4-gram TF-IDF 余弦（PAN 的 cngdist 基线）不需分词、天然适配中文，用于判断两段自由文本是否同一作者。
- **为什么更好**：PAN 2023 跨语体作者验证的结果是最有力的证据：朴素字符 4-gram 余弦基线 overall 0.595，仅比冠军的 BERT+对比学习（0.623）低约 3 个点，还击败了 11 支参赛队中的 9 支——即"简单方法在作者验证上依然极具竞争力"，且零训练成本、完全可解释、对中文只需字符级即可。2024 年 arXiv 研究证明 Delta 在中古汉语诗歌上也有效（字符特征可用），中文场景传统上以虚词分布+字符 n-gram+标点风格为特征。
- **标书场景用法**：只对标书中的自由撰写段落（技术方案叙述、施工组织设计文字）做成对验证，先剥离招标文件引用、模板套话（否则模板文本会淹没作者信号）；对每对投标人计算字符 n-gram 余弦与虚词 Delta，超过校准阈值报"疑似同一写手"。作为洗稿/改写后的兜底信号：改写会破坏表层雷同但很难改变虚词习惯与字符分布。
- **参考**：
  - Burrows, 'Delta': A Measure of Stylistic Difference and a Guide to Likely Authorship, LLC 17(3), 2002
  - Stamatatos et al., Overview of the Authorship Verification Task at PAN 2023, CEUR Vol-3497, https://ceur-ws.org/Vol-3497/paper-199.pdf
  - How does Burrows' Delta work on medieval Chinese poetic texts?, arXiv:2407.08099, 2024
  - Evert et al., Understanding and explaining Delta measures for authorship attribution, DSH 32, 2017

### PAN 近年最佳作者验证方法：预训练模型嵌入 + 对比学习（S-BERT contrastive）与 LLM 辅助验证

- **成熟度**：研究级/竞赛级（PAN 冠军方案代码多公开；LLM 提示方案 2024-2025 论文阶段，无中文标书域验证）
- **原理**：PAN 2023 冠军 Ibrahim et al. 用 S-BERT 表征+对比学习训练作者验证器（overall 0.623/AUROC 0.616）；2024-2025 趋势是作者风格嵌入（embedding 空间偏向风格而非内容）与 LLM 直接提示做可解释作者验证（CAVE、AIDBench），SIGKDD Explorations 2024 有系统综述。
- **为什么更好**：在同语体、训练数据充足时神经方法显著超过传统基线（PAN 2020-2021 同人小说任务上 AUC 可达 0.9+）；LLM 方案免微调、跨域泛化、能输出"基于哪些文体特征判断"的自然语言理由——对需要向评标委员会解释的场景价值独特。但 PAN 2023 跨语体设定下仅领先字符 n-gram 基线约 3 点，说明其优势依赖域匹配；中文标书需自建训练对。
- **标书场景用法**：用中文预训练模型（如 bge/text2vec 系）在自建"同写手标书段落对"上做对比学习微调，得到风格嵌入做两两评分；或用 LLM 对高疑对做二审：提示其忽略主题、仅按虚词/句法/标点习惯判断是否同一写手并给出理由，作为人工复核的解释层。定位为 stylometry 基线之上的加强层，不单独定案。
- **参考**：
  - Stamatatos et al., Overview of the Authorship Verification Task at PAN 2023, CEUR Vol-3497, https://ceur-ws.org/Vol-3497/paper-199.pdf
  - Huang, Chen, Shu, Authorship Attribution in the Era of Large Language Models, SIGKDD Explorations, 2024, https://github.com/llm-authorship/survey
  - CAVE: Controllable Authorship Verification Explanations, arXiv:2406.16672, 2024
  - AIDBench, arXiv:2411.13226, 2024

### 共同错误取证（shared errors / forensic linguistics，经典且法规直接背书）

- **成熟度**：司法级/工业级（清标环节标准动作；自动化检测属工程组合而非现成产品）
- **原理**：两份独立完成的文件共享同一处拼写错误、错别字、标点错误、错误数字、张冠李戴（A 公司标书里出现 B 公司名或无关项目名）的概率极低，共享独特错误因此具有远超一般相似度的证明力；法庭语言学（Coulthard 等）以共享词汇比例、共享短语长度和独特错误作为抄袭/合谋的标准证据。
- **为什么更好**：单点定案能力：一处共享的罕见错误比 30% 的文本相似度更有说服力，且直接对应中国法规条款——《招标投标法实施条例》第40条"不同投标人的投标文件异常一致"、政府采购 87 号令等把"内容异常一致或错漏一致"列为视为串通投标情形，实务判例（如 79 家公司集体串标案）均以相同错误为核心证据。对洗稿也有效：改写者往往只改对的部分、留下错的部分。
- **标书场景用法**：工程化路径：对全部标书跑中文拼写/语法纠错模型（CSC 类）+ 规则检测（重复标点、错误单位、日期矛盾、错误的招标编号/项目名/公司名），得到每份文件的"错误集合"（位置+错误形式+纠正形式）；两两求交集并按错误在全库中的稀有度加权打分；命中"他人公司名/他项目残留"这类高稀有度错误直接置顶报告。这是标书场景性价比最高的强证据信号之一。
- **参考**：
  - Coulthard & Johnson, An Introduction to Forensic Linguistics: Language in Evidence, Routledge
  - Forensic linguistics, https://en.wikipedia.org/wiki/Forensic_linguistics
  - 79家公司集体串标案投标文件雷同判定规则详解, http://www.zhenyunjianshe.com/uploads/soft/240703/2-240F3093930.pdf
  - 《招标投标法实施条例》第40条; 电子招投标围串标19种行为, http://www.sanmen.gov.cn/art/2022/9/26/art_1229610745_59040154.html

### NCD 归一化压缩距离（经典但仍是工程首选之一）

- **成熟度**：工业级（实现约 20 行代码；已被用于抄袭检测、恶意软件聚类、钓鱼网站检测等多领域）
- **原理**：NCD(x,y)=(C(xy)−min(C(x),C(y)))/max(C(x),C(y))，用通用压缩器（bzip2/PPM/zstd）近似 Kolmogorov 复杂度，无参数、无需分词与特征工程地度量任意两份文件的信息重叠。
- **为什么更好**：对"换词不换结构"的洗稿和结构重排具有鲁棒性：2022 年编程作业查重研究中 NCD 以 p 值低至 0.002 的置信度在 369 份提交中标出 1.9% 的抄袭对，优于商业工具；PAN 历届将 PPM 压缩交叉熵列为官方基线（同语体设定下长期具有竞争力，PAN 2023 跨语体下退化明显，overall 0.402，说明其适用边界是同类型文本）。对中文标书免分词这一点是实际工程优势。
- **标书场景用法**：作为廉价的第一道全库粗筛：对标书按章节切块，两两算 NCD 距离矩阵并层次聚类，异常近的跨投标人章节对进入精比对流水线（对齐+错误比对+stylometry）。注意选择压缩器窗口大于文本长度（bzip2/PPMd 而非窗口受限的 gzip），并对模板章节先做全库背景扣除，否则模板会拉高所有对的相似度。
- **参考**：
  - Cilibrasi & Vitányi, Clustering by Compression, IEEE Trans. Information Theory 51(4), 2005
  - Plagiarism deterrence for introductory programming, arXiv:2206.02848, 2022
  - Stamatatos et al., PAN 2023 AV Overview (compressor baseline), https://ceur-ws.org/Vol-3497/paper-199.pdf

### 国内电子评标平台多维雷同检测体系（机器码/标识码/IP/CA 锁 + 12 类内容比对，工业实战基准）

- **成熟度**：工业级（监管方/交易平台已部署；具体算法闭源）
- **原理**：国内电子招投标平台与商用标书查重系统（如知也云）已形成标准化围串标检测流水线：硬件指纹（计价软件加密锁序列号、MAC、CPU 序列号、硬盘序列号）、上传 IP/CA 锁、文档属性（作者/最后保存者/公司/创建时间）、加上覆盖文字/表格/图片/公司信息等约 12 类内容的逐字+语义比对。
- **为什么更好**：这是本场景下被大规模实战检验的"事实工业 SOTA"：地方规范明确"工程量清单 XML 中加密锁序列号、MAC、CPU、硬盘序列号相同应认定为同一单位或个人编制"，即硬件指纹一致可直接触发法定认定，检出即定性，无需相似度阈值争论；语义比对层则负责捕捉同义替换/结构调整的洗稿。任何新方案都应以此为基线做增量。
- **标书场景用法**：离线比对工具虽拿不到平台侧上传 IP/CA 数据，但可完整复刻文件侧信号：docx/PDF 属性比对、工程量清单 XML 内嵌的软件锁/硬件序列号字段提取比对、12 类内容分层比对。产品上建议输出与法规条款对应的"认定级/线索级"两级结论，直接引用《招标投标法实施条例》第40条与地方细则，方便评标委员会援引。
- **参考**：
  - 知也云标书查重系统介绍, https://m.dxqxpt.com/h-nd-36588.html
  - 投标文件创建码或机器码一致，可以认定为串标吗？, https://m.caigou2003.com/article/yllzc?articleId=721020440851513345
  - 电子招投标中认定围标串标的8种方式, 福建省发改委, https://fgw.fujian.gov.cn/ztzl/cjzxjyzx/zfjzcg/202509/t20250919_7012524.htm
  - AI评委上岗：智能评标系统, https://www.tocheck.cn/skills/3376.html

### 核查记录（文档取证与作者归属）

- **[CONFIRMED]** PAN 2023 跨语体作者验证：冠军 Ibrahim et al.（预训练模型+对比学习）overall 0.623（AUROC 0.616），基线 cngdist 0.595，深度方法仅领先约 3 个百分点（CEUR-WS Vol-3497 paper-199 Table 4）
  - 核查依据：直接提取原始论文 PDF（https://ceur-ws.org/Vol-3497/paper-199.pdf）核对 Table 4：Ibrahim et al. 两个最佳 run（reduced-graph/resolving-globe）AUROC 0.616、Overall 0.623，BASELINE cngdist Overall 0.595（差 0.028），正文明确任务为 cross-discourse type authorship verification、获胜方法为 pre-trained language model + contrastive learning、cngdist 为最常见字符 4-gram + 余弦相似度的朴素基线（论文描述未明写 TF-IDF 加权，但其余数字与表述完全吻合）。
- **[CONFIRMED]** Adhatarao & Lauradoux (arXiv:2103.02702) 192 条 YARA 规则在 508,836 份 PDF 上：LibreOffice/PDFLaTeX 100%，Word 与 Quartz 超 90%，IACR 总体 74%，不依赖可抹除的元数据
  - 核查依据：arXiv:2103.02702《Robust PDF Files Forensics Using Coding Style》全文（https://ar5iv.labs.arxiv.org/html/2103.02702）逐项核实：192 条规则识别 11 种 PDF 生成器、测试 508,836 份预印本 PDF、部分生成器（含 LibreOffice/PDFLaTeX）100%、Table 10 显示 Word 96%、Quartz 98%、IACR 数据集正确率 74%，且明确称检测不使用元数据对象（元数据'可被篡改或删除，极不可靠'，仅用于验证）。
- **[CONFIRMED]** Meta 于 2019 年 8 月开源 PDQ：256 位哈希、随机图片对期望 Hamming 距离 128、推荐匹配阈值 ≤31、faiss 下约 4000 张/秒
  - 核查依据：官方 GitHub（https://github.com/facebook/ThreatExchange/blob/main/pdq/README.md）载明匹配阈值 '<=31'、'faiss matching implementation has been proven up to 4000 images/sec' 并引用 2019/08 Meta Newsroom 开源公告；独立评测 Dalins et al.（https://arxiv.org/abs/1912.07745）确认 2019 年 8 月开源、256 位哈希、随机哈希对统计期望距离 128（注意 4000 张/秒指 faiss 匹配吞吐而非哈希计算速度）。
- **[CONFIRMED]** Spennemann & Singh（IJDC 18(1), 2024）发现 MS Word 新分配 rsid 不严格递增（不符 OOXML 标准），不能凭 rsid 大小推断编辑先后；Joun et al.（J Forensic Sci, 2021）验证共享 rsid 可判定文档同源并重建流转关系
  - 核查依据：Spennemann & Singh《The Generation of Revision Identifier (rsid) Numbers in MS Word》确为 IJDC 18(1) 2024（https://ijdc.net/index.php/ijdc/article/view/870 ，摘要经 CSU 机构库 https://researchoutput.csu.edu.au/en/publications/the-generation-of-revision-identifier-rsid-numbers-in-ms-word-imp/ 核实）：'newly allocated rsid do not conform to the standard as the numerical value…may be lower than that of the previous save action'，且仅凭单一版本无法建立编辑时序；Joun, Chung, Park & Lee《Relevance analysis using revision identifier in MS word》确刊于 Journal of Forensic Sciences 66(1):323-335, 2021（https://onlinelibrary.wiley.com/doi/10.1111/1556-4029.14584 ），提出用共享 RSID 对 Word 文件分组判定关联并追踪文档编辑历史与流转。
- **[CONFIRMED]** 《招标投标法实施条例》第40条将'由同一单位或个人编制''投标文件异常一致'列为视为串通投标情形；部分地方规定加密锁序列号/MAC/CPU/硬盘序列号相同应认定为同一单位或个人编制，'创建标识码一致'通常交评标委员会综合评判
  - 核查依据：第40条原文经政府来源核实（深圳政府在线 https://www.sz.gov.cn/hdjl/ywzsk/gzw/jdjc/content/post_11533431.html ）：第(一)项'不同投标人的投标文件由同一单位或者个人编制'、第(四)项'投标文件异常一致或者投标报价呈规律性差异'；多个官方/行业来源（陕西省工信厅案例分析 https://gxt.shaanxi.gov.cn/cyfz/zbjg/202410/t20241009_3315471.html 、福建省发改委 https://fgw.fujian.gov.cn/ztzl/cjzxjyzx/zfjzcg/202509/t20250919_7012524.htm ）引用地方规定：已标价工程量清单电子文档记录的计价软件加密锁序列号，或网卡MAC地址、CPU序列号、硬盘序列号相同的应认定为第40条第(一)项情形；政府采购实务口径（https://m.caigou2003.com/article/yllzc?articleId=721020440851513345 ）明确'文件制作机器码一致可视为串标，文件创建标识码一致由评标委员会结合项目情况综合评判'，与声明表述一致。

## 6. 围标串标筛查（非文本信号）

### Imhof 简单统计筛查（变异系数 CV、峰度 KURT、极差 RD、前两低报价差 DIFF/DIFFP、偏度等标内报价分布筛查）

- **成熟度**：工业级（经典但仍是首选：任何围标筛查系统的第一层）
- **原理**：对同一标段内所有投标报价计算分布统计量：串标时陪标报价被人为抬高且相互靠拢，导致报价方差/变异系数异常低、最低价与次低价差距异常大（掩护报价特征），据此打分预警。
- **为什么更好**：相比人工翻阅标书，纯报价数据即可批量筛查且被实战验证：瑞士竞争委员会（COMCO）2013 年据此对 See-Gaster 地区公路建设开启调查并于 2016 年成功处罚围标企业——是全球少数由统计筛查直接引发处罚的案例；单用 CV+DIFF 简单规则在瑞士 Ticino 卡特尔数据上即可区分串标期与竞争期。
- **标书场景用法**：标书比对场景中最容易落地的非文本信号：从 2-5 份投标文件抽取总报价与分项报价，计算标段内 CV、RD、DIFF（最低价与次低价差 / 其余报价标准差）等指标；CV 异常低或 DIFF 异常大即挂'疑似掩护报价'红旗，与文本雷同信号做证据叠加。
- **参考**：
  - Imhof, 'Screening for Bid Rigging—Does it Work?' Journal of Competition Law & Economics, 2018, https://academic.oup.com/jcle/article-abstract/14/2/235/5058993
  - Imhof et al., 'Detecting Bid-Rigging Cartels with Descriptive Statistics', 2020

### Abrantes-Metz 方差筛查（variance screen / 低方差口袋检测）

- **成熟度**：工业级（经典但仍是首选，多国竞争执法机构在用）
- **原理**：利用'卡特尔存续期价格均值偏高且方差异常低'的经验规律，用滚动窗口在价格/报价时间序列中搜索'高均值+低标准差'的异常区段。
- **为什么更好**：有明确量化依据：冷冻鲈鱼围标案破裂后价格均值下降 16%、标准差上升 200% 以上，说明方差信号比均值信号灵敏得多；FTC 工作论文体系化后成为各国竞争执法机构价格筛查的标准工具。
- **标书场景用法**：适合跨项目维度：对同一批投标人历史多项目报价做滚动方差分析，发现某供应商群体报价长期'均值高、方差低'即提示存在价格协调；单项目 2-5 份标书内则退化为标内 CV 筛查。
- **参考**：
  - Abrantes-Metz, Froeb, Geweke & Taylor, 'A Variance Screen for Collusion', International Journal of Industrial Organization, 2006, https://www.ftc.gov/reports/variance-screen-collusion

### Bajari & Ye 条件独立性 + 可交换性检验

- **成熟度**：研究级偏工业级（执法与诉讼专家证据中使用；需要较多历史标段和成本协变量）
- **原理**：竞争性投标在控制成本因素（距离、产能利用率、规模等）后应满足两条可检验性质——不同投标人残差报价相互独立、且报价不随对手身份改变（可交换）；回归后检验残差相关性与系数对称性，拒绝即提示串谋。
- **为什么更好**：相比纯描述性红旗，它给出有统计推断基础的假设检验（结构计量识别），可对'谁和谁串'输出成对判定；在美国三州道路封层工程近乎全行业投标数据上实证可行，至今仍是学界串标检验的基准方法、后续 ML 方法的特征来源。
- **标书场景用法**：需要平台级历史数据：对频繁共同投标的企业对，回归剔除成本因素后检验其报价残差相关性（如皮尔逊相关显著为正）与非对称反应，输出'高危企业对'清单；单次 2-5 份标书不够，适合作为供应商画像后台。
- **参考**：
  - Bajari & Ye, 'Deciding Between Competition and Collusion', Review of Economics and Statistics, 2003, https://users.nber.org/~confer/2002/si2002/bajari.pdf
  - Emerald JoPP 'Cartel detection in public procurement – Evaluation of five econometric methods', 2026, https://www.emerald.com/jopp/article/26/1/1/1253829

### 统计筛查 + 监督机器学习（Imhof & Huber 瑞士方法：lasso/随机森林/SVM/super learner 集成）

- **成熟度**：工业级边缘（方法论成熟、多国竞争机构复现；西班牙 BRAVA 即此范式的产品化）
- **原理**：把标内报价筛查值（CV、DIFF、RD、偏度、峰度、KS 统计量等）作为特征，用已判决卡特尔案件（瑞士、日本冲绳等）标注训练分类器，直接预测'该标段是否串标'。
- **为什么更好**：比单一筛查阈值法显著更准且可跨市场迁移：瑞士 584 个建筑标段上正确分类率约 84%-90%+；面向不完整卡特尔（部分投标人清白）的 coalition 级方法在瑞士数据达 87-90%、冲绳 92-95%；跨国迁移（瑞士↔冲绳 8 市场）平均准确率约 91%——证明模型可以先在他人数据上训练再用于本地。
- **标书场景用法**：标书比对产品的核心可复制方案：以每标段报价筛查向量为特征、用公开处罚案例或规则标注冷启动，输出 0-100 串标概率分；论文特征表可直接照抄，2-5 份报价即可算全部筛查值。
- **参考**：
  - Huber & Imhof, 'Machine Learning with Screens for Detecting Bid-Rigging Cartels', International Journal of Industrial Organization, 2019, https://www.sciencedirect.com/science/article/abs/pii/S0167718719300219
  - Wallimann, Imhof & Huber, 'A Machine Learning Approach for Flagging Incomplete Bid-Rigging Cartels', Computational Economics, 2022, https://link.springer.com/article/10.1007/s10614-022-10315-w
  - Huber, Imhof & Ishii, 'Transnational machine learning with screens for flagging bid-rigging cartels', JRSS-A, 2022

### 图注意力网络 GAT 围标检测（Imhof, Viklund & Huber 2025，当前学术 SOTA）

- **成熟度**：研究级（2025 年 arXiv，但作者含瑞士竞争委员会经济学家，工程化路径清晰）
- **原理**：以标段为节点、共同投标人重叠为边构图，节点特征为该标的报价统计筛查值，用 Graph Attention Network 让'同一批人反复同场'的结构信息在相似标段间传播后分类串标/竞争。
- **为什么更好**：首次把'报价分布信号'与'投标人共现结构信号'统一建模：在 7 国 13 个市场数据上，多个指标相比 Huber et al. 2022 集成学习基线提升 14-20 个百分点；瑞士+冲绳 8 市场平均准确率约 91%，扩展到 12 市场仍保持约 84%，跨市场迁移 80-90%。
- **标书场景用法**：适合平台级部署：历史标段构成图后，新项目 2-5 份标书作为新节点接入，同时利用本标报价特征和这批投标人历史共现子图输出串标概率——正是'投标人共现网络+报价筛查'两个维度的融合器。
- **参考**：
  - Imhof, Viklund & Huber, 'Catching Bid-rigging Cartels with Graph Attention Neural Networks', arXiv:2507.12369, 2025, https://arxiv.org/abs/2507.12369

### 共同投标网络 + 社区凝聚度/排他性分析（Wachs & Kertész 网络卡特尔检测）

- **成熟度**：研究级偏工业级（方法简单可复现，中国反欺诈/审计领域的关联团伙挖掘即同思路）
- **原理**：以企业为节点、共同投标为边构建 co-bidding 网络，做社区发现后计算每个社群的 cohesion（内部反复同场程度）与 exclusivity（对外隔离程度），高凝聚+高排他的团体最可能是可持续卡特尔。
- **为什么更好**：无监督、无需判例标注即可全市场扫描：在格鲁吉亚约 15 万份公共合同上，高凝聚高排他团体显著更多呈现传统卡特尔标志（如中标轮换），并成功召回匈牙利学生奶已知卡特尔；比逐标筛查多出'团伙'视角，直接输出嫌疑企业集合而非单标红旗。
- **标书场景用法**：供应商图谱后台：累积历史投标记录构网，叠加工商股权、高管兼任、共用联系方式等边，用 Louvain/Leiden 社区发现 + 凝聚度/排他性打分；新项目 2-5 家投标人若同属一个高危社群立即预警。
- **参考**：
  - Wachs & Kertész, 'A network approach to cartel detection in public auction markets', Scientific Reports 9:10818, 2019, https://www.nature.com/articles/s41598-019-47198-1

### 投标轮换/在位者模式的断点识别（Kawai-Nakabayashi-Ortner-Chassang 'missing bids' RDD）

- **成熟度**：研究级（学界公认严谨，需大样本历史数据，尚非开箱即用工具）
- **原理**：只看'赢标价与次低价极接近'的标段：竞争下险胜险败双方事后表现应连续对称，而串标轮换会在'差一点点就赢'处造成投标密度缺口（missing bids）与回归断点，从而把成本差异导致的轮换与人为轮换区分开。
- **为什么更好**：解决了轮流中标筛查的最大痛点——'轮换也可能是成本波动的正常结果'：通过准实验设计给出因果层面的串谋证据而非相关性红旗，在日本国土交通省数十万标段数据上识别出大规模疑似串标群体，方法发表于顶级期刊。
- **标书场景用法**：平台级历史分析：统计各投标人'近胜/近败'附近的报价分布，检测密度缺口与轮换规律；产品里可简化为'固定班底轮流中标 + 报价差恒定'的规则版轮换检测（中国审计实务同款）。
- **参考**：
  - Kawai, Nakabayashi, Ortner & Chassang, 'Using Bid Rotation and Incumbency to Detect Collusion: A Regression Discontinuity Approach', NBER w29625 / Journal of Political Economy, 2022-2023, https://www.nber.org/papers/w29625

### 报价数字分析：Benford 定律 + 尾数/整数聚集 + 规律性差异（等差/等比/K 系数）检测

- **成熟度**：工业级（经典但仍是首选；中国清标系统标配'报价规律性分析'模块）
- **原理**：自然竞争形成的报价首位数字近似服从 Benford 分布、尾数近似均匀，人工编造的陪标价则出现首位数偏离、整数/特定尾数聚集；同标内报价呈等差、等比或固定系数（如统一乘 0.95/0.97）即中国条例所称'报价呈规律性差异'。
- **为什么更好**：计算成本几乎为零且法律效力强：'规律性差异'在中国《招标投标法实施条例》第 40 条中是可直接'视为串通投标'的法定情形，检出即可废标；Benford 检验在巴西等国采购审计中用于批量圈定异常项目（局限：单标样本量小，只适合跨项目聚合或分项清单级检验）。
- **标书场景用法**：对 2-5 份标书：检验总价及分项单价是否等差/等比、差值或比值是否恒定（拟合 price_i = a*price_j + b 的 R² 接近 1 即报警）、尾数分布是否异常聚集；对工程量清单上千个分项单价可做标内 Benford/卡方检验，样本量足够。
- **参考**：
  - 《招标投标法实施条例》第四十条（国务院令第 613 号，2011）, https://www.gov.cn/gongbao/content/2017/content_5219119.htm
  - Fazekas, Tóth & Wachs, 'Public procurement cartels' GTI Working Paper, 2023, https://www.govtransparency.eu/wp-content/uploads/2023/04/Fazekas-et-al_PP-cartel-detection_GTI-WP_2023.pdf
  - 'Benford's Law and Naturally Occurring Prices in Certain eBay Auctions', 2005

### 大样本筛查有效性验证 + 通用 ML 筛查组合（Fazekas-Tóth-Wachs-Abdou，IJIO 2026）

- **成熟度**：研究级偏工业级（2026 年发表于 International Journal of Industrial Organization，附公开数据）
- **原理**：在 7 个欧洲国家 2004-2021 年 73 个已判决卡特尔与采购合同级数据上，系统检验数十种价格/投标行为筛查指标的泛化能力，并用 ML 组合成预测模型。
- **为什么更好**：回答了'哪些筛查指标在跨国家、普通质量数据上仍然有效'这一此前没有答案的问题：组合模型在真实（非精选）数据上达 70-84% 预测准确率，给出了工程上该优先实现哪些指标的证据排序；数据集已公开（Mendeley），可直接用于训练。
- **标书场景用法**：为标书比对产品选特征提供依据：优先实现其验证有效的投标者数量、单一投标率、中标集中度、报价离散度等指标；其公开的 73 卡特尔标注数据可作为冷启动训练集。
- **参考**：
  - Fazekas, Tóth, Wachs & Abdou, 'Public procurement cartels: A large-sample testing of screens using machine learning', International Journal of Industrial Organization, 2026, https://www.sciencedirect.com/science/article/pii/S0167718725000943
  - 数据集：https://data.mendeley.com/datasets/f3y4nrn3s6/2

### 国家级自动筛查系统：韩国 BRIAS、巴西 CADE Cérebro、西班牙 CNMC BRAVA、英国 CMA AI 工具

- **成熟度**：工业级（多国竞争执法机构生产环境运行）
- **原理**：接入本国电子采购平台全量数据的常态化筛查系统——BRIAS（2006 年起，按报价/预算价比、参与人数、竞争方式等加权指标对 KONEPS 上超 5 亿韩元标段逐月算围标概率分）；Cérebro（R/Python/neo4j 数据挖掘+图数据库，为突袭搜查提供线索）；BRAVA（2024 年上线，监督 ML+LIME/SHAP 可解释+图分析，训练于 700 余万标段）；CMA 2025 年 1 月试点 AI 反串通工具。
- **为什么更好**：是'被实战验证'的最强证据：BRAVA 已被 CNMC 用于产生实际处罚的案件且美国 DOJ 寻求其培训；BRIAS 运行近 20 年，KFTC 认为其对市场形成'每个标都被扫描'的威慑；KONEPS 上线后采购机构廉洁感知指数从 6.8 升至 8.5。相比学术模型，这些系统解决了数据管道、阈值运营、可解释性（供办案人员与法官理解）等工程问题。
- **标书场景用法**：产品架构范本：分层设计=指标加权分（BRIAS 式，冷启动无需标注）→ 监督 ML+SHAP 可解释（BRAVA 式，有案例后升级）→ 图数据库存投标人关系（Cérebro 的 neo4j 式）；输出给评标/监管人员的应是'红旗清单+归因解释'而非黑箱分。
- **参考**：
  - OECD, 'Data screening tools in competition investigations', 2022, https://one.oecd.org/document/DAF/COMP/WP3(2022)5/en/pdf
  - Addleshaw Goddard, 'Detecting bid-rigging through AI', 2025, https://www.addleshawgoddard.com/en/insights/insights-briefings/2025/competition/detecting-bid-rigging-through-ai-and-public-procurement-law-implications/
  - McCann FitzGerald, 'The growing use of AI by competition authorities', 2026, https://www.mccannfitzgerald.com/knowledge/antitrust-competition/levelling-the-playing-field-the-growing-use-of-ai-by-competition-authorities

### OECD 红旗清单与 2023 年理事会建议（Bid-Rigging Detection List, 2025 更新版）

- **成熟度**：工业级（政府间标准文件）
- **原理**：OECD 2023 年修订《打击公共采购围标建议》、2025 年 9 月更新配套指南，给出体系化红旗清单：异常投标/定价模式、雷同文件（相同笔误、相同排版、相同联系方式/元数据）、可疑陈述与行为，并要求成员国推动数字筛查与采购数据标准化。
- **为什么更好**：是国际公认的'检查项全集'与合规基准：把文本信号（文件雷同）与非文本信号（价格模式、行为）整合成可操作 checklist，被各国采购培训直接采用；对产品而言等于免费的需求规格说明与权威背书。
- **标书场景用法**：把红旗清单逐条映射为检测规则并在报告中引用 OECD 条目编号，可显著增强查重报告在监管/司法场景下的说服力；也用于确定'必须覆盖'的信号清单查缺补漏。
- **参考**：
  - OECD Guidelines for Fighting Bid Rigging in Public Procurement (2025 Update), https://www.oecd.org/en/publications/2025/09/oecd-guidelines-for-fighting-bid-rigging-in-public-procurement-2025-update_127880ea.html
  - OECD Recommendation on Fighting Bid Rigging in Public Procurement (2023 revision), https://members.wto.org/crnattachments/2023/GPA/GPA_158/OECD-LEGAL-0396-en.pdf

### EU DIGIWHIST/Opentender 腐败风险指标（CRI 红旗体系）

- **成熟度**：工业级（opentender.eu 持续运行，审计机构使用）
- **原理**：在 33 个欧洲辖区约 2000 万份合同上计算客观红旗指标——单一投标率、公告期过短、非公开程序、中标集中度、买方-供应商绑定等——聚合为合同级腐败/竞争风险分并开放仪表盘。
- **为什么更好**：证明了'仅用采购元数据（无标书文本）'即可做跨国规模化风险量化：单一投标作为最简竞争受限指标已被世界银行、欧洲审计院采纳；指标定义全部公开可复用，是学界引用最多的采购风险指标体系。
- **标书场景用法**：为项目级风险底色打分：即便拿到的只有 2-5 份标书，也可结合项目元数据（公告期长短、程序类型、历史单一投标率）给出'该项目本身是否高危'的先验，与标书内信号相乘。
- **参考**：
  - DIGIWHIST / opentender.eu, https://digiwhist.eu/
  - Fazekas & Kocsis, 'Uncovering High-Level Corruption: Cross-National Corruption Risk Indicators Using Public Procurement Data', British Journal of Political Science, 2020

### 中国电子招投标平台数字取证比对：IP/MAC/机器码/CA 锁/计价软件锁号/文档元数据/保证金账户（条例第 39/40 条落地）

- **成熟度**：工业级（中国省级公共资源交易平台普遍部署，多有行政处罚与判例支撑）
- **原理**：开评标系统自动比对各投标文件的上传与制作痕迹——下载/上传 IP、网卡 MAC、硬件机器码、CA 数字证书同源、工程量清单 XML 中记录的计价软件加密锁序列号、文档'作者/最后保存者'元数据、投标保证金是否从同一账户转出、电子保函投保记录——命中即触发《招标投标法实施条例》第 40 条'视为串通投标'的法定情形链。
- **为什么更好**：相比文本相似度，这类'机器指纹'信号伪造成本高、误报语义清晰、且在中国有直接法律效力（第 40 条为法律拟制，评标委员会可据此直接否决投标）；各省公共资源交易平台已把'同一 IP/MAC/锁号'比对做成标配清标功能，是实战中检出率最高的围标证据来源。注意实务共识：单一指标（如同一 IP 可能是共用 WiFi）不宜孤证定案，需与报价规律、文件雷同交叉验证。
- **标书场景用法**：对标书比对工具：解析 docx/pdf 元数据（作者、公司、创建/修改时间序列、生产软件）、工程量清单 XML 的软件锁号字段、扫描件的设备特征，做跨标书碰撞；若能接入平台日志则叠加 IP/MAC/CA 比对；输出直接对应第 40 条各项的证据映射表。
- **参考**：
  - 《招标投标法实施条例》第三十九、四十条, https://www.gov.cn/gongbao/content/2017/content_5219119.htm
  - 福建省发改委'电子招投标中认定围标串标的 8 种方式', 2025, https://fgw.fujian.gov.cn/ztzl/cjzxjyzx/zfjzcg/202509/t20250919_7012524.htm
  - 陕西省工信厅案例分析：投标文件网络地址或机器码相同能否认定串标, 2024, https://gxt.shaanxi.gov.cn/cyfz/zbjg/202410/t20241009_3315471.html

### 中国'主体+行为'综合预警体系（发改法规〔2026〕195 号围串标识别场景 + 关联图谱清标）

- **成熟度**：工业级（政策强制推广期；各省交易中心与审计机关已有'伴随投标''陪标专业户'大数据模型实践）
- **原理**：八部委 2026 年《关于加快招标投标领域人工智能推广应用的实施意见》定义的官方技术路线：多维数据碰撞 + 投标主体画像（工商股权穿透、高管兼任、联系方式共用）、投标行为异常（伴随投标、中标概率异常、专家打分倾向）、叠加对投标文件/工程量清单/报价清单的语义相似性与关键报价特征深度扫描。
- **为什么更好**：这是中国监管层钦定并给出时间表的落地方向——2026 年底招标文件检测、智能辅助评标、围串标识别在部分省市全覆盖、2027 年底全国推广：意味着平台数据接口、标注案例、市场需求都将快速成型；其'主体画像+行为分析+文本比对'三层框架与国际上 BRAVA（ML+图分析）的架构收敛，相互印证了技术路线正确性。
- **标书场景用法**：直接决定产品路线图：标书交叉查重工具应预留主体关联图谱（工商数据 API 股权/高管穿透）、历史伴随投标统计（同批企业共同出现频次、固定中标人）、专家打分倾向分析三个扩展维度，对齐 195 号文场景清单即对齐未来采购方需求。
- **参考**：
  - 国家发改委等八部门, 发改法规〔2026〕195 号, 2026, https://www.ndrc.gov.cn/xxgk/zcfb/tz/202602/t20260210_1403680.html
  - 吉林省审计厅'审计视角下串通投标行为的识别路径与方法', 2025, http://sjt.jl.gov.cn/sy/sjzc/ywjl/202507/t20250718_9284552.html

### 核查记录（围标串标筛查（非文本信号））

- **[CONFIRMED]** Imhof/Viklund/Huber 2025 GAT 论文（arXiv:2507.12369）：瑞士+冲绳 8 市场平均准确率约 91%、扩展至 7 国 12/13 市场约 84%，多个指标较 Huber et al. 2022 集成学习基线提升 14-20 个百分点
  - 核查依据：论文摘要证实 8 市场（瑞士+冲绳）最佳配置平均准确率 91%、扩展后约 84%，正文证实数据集覆盖 7 国 13 个市场（瑞士4、冲绳4、巴西1、芬兰1、瑞典1、美国2），且 GAT 较集成学习基线平均准确率提升约 15 个百分点（91.4% vs 76.3%）、ROC-AUC 提升 20 个百分点（94.1% vs 73.6%），与声明的 14-20pp 区间及 91%/84% 数字一致（来源：arxiv.org/abs/2507.12369 摘要与 arxiv.org/html/2507.12369 正文；细微出入：摘要称扩展集为 12 市场、正文一处称 11 市场均值 84.4%）。
- **[UNCLEAR]** CNMC 的 BRAVA 于 2024 年由经济情报部门上线、基于约 700 万标段训练并集成 LIME/SHAP；CNMC 主席 2026 年 2 月称其已用于产生实际处罚的案件且美国 DOJ 正寻求相关培训
  - 核查依据：前半部分可确认：CNMC 官方博客（blog.cnmc.es 2024-04-11）证实 BRAVA 由经济情报部门（UIE）开发，McCann FitzGerald 称其 2024 年上线，Stanford CodeX 文章（law.stanford.edu 2025-09-25）证实数据库超 700 万标段并集成 LIME/SHAP（但 CNMC 自己 2024 年博客称数据库为 350 万份合同，700 万标段之说未见 CNMC 一手来源）；关键的 2026 年 2 月主席表态（已用于处罚案件、DOJ 寻求培训）仅见于 McCann FitzGerald 律所文章（mccannfitzgerald.com）且未注明出处，在 CNMC 官网新闻稿/演讲中未能找到原始声明，故整体判 unclear。
- **[UNCLEAR]** KFTC 于 2006 年上线 BRIAS，按月对 KONEPS 中价值超 5 亿韩元的招标以加权指标（报价/预算价比、参与人数、竞争方式）计算围标概率分
  - 核查依据：2006 年上线已获一手来源证实（韩国政府政策简报网 korea.kr 载 KFTC 2006-01-17 新闻稿《입찰담합징후분석시스템 가동 개시》），OECD 2016 报告 Box 3（基于 KFTC 提交材料）证实加权指标求和打分机制（招标方式、投标人数、报价与预算价比等），但'5 亿韩元'门槛未见 KFTC 一手材料且 Chambers Cartels 2026 等来源显示门槛分类别（建设工程约 50 亿韩元、其他约 5 亿韩元），'按月'频率也未获证实（OECD 2016 称合同授予后 30 天内采集数据、月均标记 80+ 案件），故门槛与频率表述仍停留在二手且可能过于简化，判 unclear。
- **[CONFIRMED]** 瑞士 COMCO 于 2013 年基于 Imhof 统计筛查对 See-Gaster 地区开启围标调查并于 2016 年作出处罚，是统计筛查直接引发执法处罚的标志性案例
  - 核查依据：多个独立可靠来源一致证实：COMCO/WEKO 于 2013 年 4 月首次基于对开标记录的统计筛查（Imhof、Karagök、Rutz 基于圣加仑州 2004-2010 招标数据的分析）开启 See-Gaster 调查，2016 年 7 月 8 日决定对 8 家道路/土木工程企业处以合计约 500 万瑞郎罚款，且被普遍称为统计筛查首次直接引发执法的案例（来源：NZZ《Dank Datenanalyse Kartell aufgedeckt》、Zürichsee-Zeitung、südostschweiz.ch 2016 报道、WEKO 2016 年报 PDF 及 OECD 数据筛查文件、Imhof 等学术论文；未直接查阅 COMCO 决定书原文，但媒体+WEKO 年报+当事研究者论文三方互证）。
- **[CONFIRMED]** 中国八部委发改法规〔2026〕195 号（2026 年 2 月印发）将围串标识别列为 20 个重点场景之一，要求 2026 年底部分省市全覆盖、2027 年底全国推广
  - 核查依据：国家发改委官网（ndrc.gov.cn/xxgk/zcfb/tz/202602/t20260210_1403680.html）确证《关于加快招标投标领域人工智能推广应用的实施意见》发改法规〔2026〕195 号由发改委、工信部、住建部、交通运输部、水利部、农业农村部、商务部、国务院国资委八部门于 2026 年 2 月 6 日联合印发，全文列 20 个重点场景且含'围串标识别'，时间表原句为'2026 年底，招标文件检测、智能辅助评标、围串标识别等重点场景在部分省市实现全覆盖应用；2027 年底，更多重点场景在全国范围内推广应用'（另经发改委答记者问及 secrss.com/articles/87841 全文转载互证），与声明表述一致。

## 7. 文档对齐与证据定位

### Seed–Chain–Align 范式（seed-chain-align，以 minimap2 为代表）

- **成熟度**：工业级（生信领域事实标准；迁移到文本为研究级但架构成熟）
- **原理**：先用短种子锚点（seed）建立候选匹配，再把共线锚点串成链（chain），最后只在链内做碱基级/字符级动态规划对齐（align），把散点匹配天然收敛成连续对齐区段。
- **为什么更好**：相比 PAN 传统 seeding–extension–filtering 的一堆手工启发式与难调参数，本范式把'找散点→连成段→精对齐'解耦为三个可复杂度可控的阶段；minimap2 在长序列对齐上比 BLASR/NGMLR/GraphMap 快 ≥30×且精度相当，是长读比对的工业事实标准，其三段式流程被公认为全基因组比对首选架构。
- **标书场景用法**：标书比对建议整体采用此三段式：把每份标书切成句/段做嵌入或 n-gram 得到锚点→跨文档匹配锚点→共线链化→链内做带状对齐得到'连续雷同段落'。直接产出'对齐区段+起止位置'，是雷同证据定位的骨架路线，替代只输出散点相似句的做法。
- **参考**：
  - Li H., Minimap2: pairwise alignment for nucleotide sequences, Bioinformatics 2018, https://academic.oup.com/bioinformatics/article/34/18/3094/4994778
  - arXiv:1708.01492

### 共线链化 / 稀疏动态规划（Colinear chaining, sparse DP）

- **成熟度**：工业级（链化）+ 研究级（2022–2023 带 gap 代价改进）
- **原理**：在按位置排序的锚点上做 DP，递推 f(i)=max_j{f(j)+α(j,i)−β(j,i)}（α=匹配收益、β=gap 代价），选出得分最高的一串共线锚点即为一个连续对齐区段。
- **为什么更好**：这是把'散点相似块'升级为'连续对齐区段'的核心算法。minimap2 用'从 i−1 回看、连续 h≈50 步无改进即停'把 O(N²) 降到 O(hN)；2022–2023 的 gap-sensitive / overlap-and-gap-cost 链化（Jain 等）给出带 gap 代价与重叠处理的严格最优/近最优算法，比纯启发式扩展更可控、可给覆盖率与连续性保证。
- **标书场景用法**：标书比对里链化就是'覆盖率'的来源：一条链覆盖了源/目标文档的哪段区间、覆盖多少字符即为该雷同段的覆盖率与连续度。用 gap 代价容忍洗稿时插入的少量改写句，同时惩罚跨度过大的拼接，避免把不相关句子误连成一段。
- **参考**：
  - Jain C. et al., Algorithms for Colinear Chaining with Overlaps and Gap Costs, JCB 2022
  - Chandra & Jain, Gap-Sensitive Colinear Chaining for Acyclic Pangenome Graphs, 2023, https://doi.org/10.1089/cmb.2023.0186
  - Practical colinear chaining on sequences revisited, arXiv:2506.11750 (2025)

### Minimizer 种子采样（minimizer sketching）

- **成熟度**：工业级
- **原理**：对滑动窗口内的 k-gram 只保留哈希值最小者作为种子，在保证足够长的公共子串一定被采到的前提下，大幅减少锚点数量与索引体积。
- **为什么更好**：相比对全部 k-gram 建索引，minimizer 在保留长匹配可检出性的同时把索引与锚点数降一个量级，是 minimap2 线性时间的前提；2024 Genome Biology 综述系统论证其'少即是多'的采样-灵敏度权衡，已是读比对、去重、泛基因组的标准 sketch 手段。
- **标书场景用法**：中文标书可用字符 n-gram（或分词后词 n-gram）的 minimizer 作为种子，既抗少量字词改写又控制候选爆炸；作为 Seed 阶段喂给链化，兼顾召回与速度。对扫描件 OCR 噪声，可仿 minimap2 的 homopolymer-compression 思路做归一化（全半角/标点/空白压缩）提升种子命中率。
- **参考**：
  - Ndiaye M. et al., When less is more: sketching with minimizers in genomics, Genome Biology 2024, https://genomebiology.biomedcentral.com/articles/10.1186/s13059-024-03414-4
  - Roberts et al. 2004 (minimizers, 经典)

### 带状/向量化 Smith–Waterman–Gotoh 仿射 gap 对齐（banded/SIMD SW, KSW2, Suzuki–Kasahara 差分递推, Parasail）

- **成熟度**：工业级（KSW2/Parasail 生产可用；文本迁移成熟）
- **原理**：在链锚点之间只沿对角带做仿射 gap 的局部/半全局 DP，得到字符级最优对齐路径，仿射 gap 让插入/删除段更紧凑连续。
- **为什么更好**：仿射 gap（q+l·e 或两段式 min{q+l·e, q̃+l·ẽ}）鼓励雷同段落保持紧凑，避免碎片化；Suzuki–Kasahara 差分递推让 SIMD 与峰值分数无关地做 16 路 SSE 向量化，比 Parasail 4 路快约 3×；带状限制把二次复杂度压到近线性。这是把一条链变成可展示、可高亮的精确证据的关键。
- **标书场景用法**：标书比对的'证据定位'落点：拿到候选连续段后用带状半全局 SW 对齐，输出逐字符/逐词对齐路径，用于前端红黄高亮、计算相似字符占比、区分逐字复制 vs 少量改写。仿射 gap 让'整段照抄中夹几处改词'仍对齐成一段而非断成多块。
- **参考**：
  - Suzuki & Kasahara, Introducing difference recurrence relations for faster semi-global alignment, BMC Bioinformatics 2018, PMC5836832
  - Daily J., Parasail: SIMD C library for pairwise alignments, BMC Bioinformatics 2016, PMC4748600
  - minimap2/KSW2 (Li 2018)

### PAN 2014 冠军：自适应 seeding–extension–filtering（Sánchez-Pérez/Gelbukh/Sidorov）

- **成熟度**：竞赛级（经典但仍是首选基线；开源可复现）
- **原理**：以句为种子、用含停用词的 tf-idf 加权余弦/Dice 句相似度找匹配对，再用递归算法把相邻匹配句扩展成最大长度连续段落，最后用重叠消解过滤。
- **为什么更好**：文本对齐领域公认的强基线：在 PAN 2014 文本对齐赛全语料取得 PlagDet 0.87818，为当年冠军并超过 PAN 2013 最佳；相比早期纯 n-gram 指纹，其'句级种子+递归扩展到最大段落+过滤'直接产出连续段并处理 summary/改写场景。
- **标书场景用法**：作为标书比对最省事的连续段落基线：中文分句后算 tf-idf 句相似度找雷同句种子→递归向两侧扩展合并成连续段→过滤重叠。工程上可先跑它拿到 baseline 覆盖率，再用嵌入相似度替换 tf-idf 句相似以增强抗洗稿能力。
- **参考**：
  - Sánchez-Pérez, Gelbukh, Sidorov, Adaptive Algorithm for Plagiarism Detection: The Best-Performing Approach at PAN 2014 Text Alignment, CLEF 2015, https://link.springer.com/chapter/10.1007/978-3-319-24027-5_42
  - gelbukh.com/plagiarism-detection

### Vecalign（嵌入 + 由粗到细 DP 的线性时空句对齐）

- **成熟度**：研究级（广泛用于工业级平行语料挖掘 Bitextor/CCMatrix 管线）
- **原理**：对句子及'连续多句拼接（overlap）'求嵌入，用改造自 DTW 的由粗到细近似动态规划，在线性时间内找出允许 1-多/多-1/多-多的单调句对齐路径。
- **为什么更好**：相比二次复杂度且需机器翻译的旧 SOTA，Vecalign 在德-法测试集上高出约 5 F1，且线性时空、无需 MT；overlap 嵌入天然支持一句拆多句/多句并一句，正是洗稿改写常见的句边界变化。
- **标书场景用法**：标书洗稿检测：把两份标书按段切句、用中文句向量（如 LaBSE/BGE）求 overlap 嵌入，跑 Vecalign 得到单调句对齐路径；对齐路径的连续区间即连续雷同段，可直接算覆盖率。相比只做句对余弦匹配，能抗句子合并/拆分导致的错位。
- **参考**：
  - Thompson & Koehn, Vecalign: Improved Sentence Alignment in Linear Time and Space, EMNLP 2019, https://aclanthology.org/D19-1136/
  - github.com/thompsonb/vecalign

### Bertalign（LaBSE 嵌入 + 两步 DP 句对齐，含单语变体）

- **成熟度**：研究级（开源、社区常用；中文验证充分）
- **原理**：用 LaBSE 句向量，第一步用 top-k 相似句 DP 找 1-1 锚点路径，第二步在锚点约束的搜索带内恢复所有 1-多/多-多对齐。
- **为什么更好**：针对文学/自由译文这类非 1-1 映射设计，在多个评测集上 F1 最高；两步'先锚点后加宽搜索带'比单遍 DP 更稳、更快，天然处理中文里常见的长短句重组。对中文-中文单语改写同样适用（把源/目标都设中文即可）。
- **标书场景用法**：标书'同一模板不同措辞'的强检测器：源与目标都用中文标书，Bertalign 输出连续对齐段与多对多映射，识别把一句拆成三句/三句并一句的洗稿；对齐段连续跨度可作为串标的段落级证据。可与 Vecalign 二选一，Bertalign 在中文长句重组上更稳。
- **参考**：
  - Liu & Zhu, Bertalign: Improved word embedding-based sentence alignment for Chinese–English parallel corpora, DSH 2023, https://academic.oup.com/dsh/article-abstract/38/2/621/6965034
  - github.com/bfsujason/bertalign

### Passim（n-gram shingle 过滤 + 仿射 gap Smith–Waterman 的大规模文本重用）

- **成熟度**：工业级（开源，Spark 实现，海量语料实战）
- **原理**：先用 n-gram 指纹倒排只保留共享足够 n-gram 的文档对，再从命中的 n-gram 出发做字符级仿射 gap 的 Smith–Waterman 全对齐，输出对齐段。
- **为什么更好**：是数字人文界大规模文本重用的成熟开源工程实现：n-gram 分块过滤把候选对从 O(n²) 砍到可行规模，再用局部+全局对齐处理 gap 与变体；已在海量历史报纸语料实证可扩展。相比自研一次性脚本，Passim 提供了'过滤→对齐→聚合成大段'的完整可复用管线。
- **标书场景用法**：当比对范围从'一个项目 2-5 份'扩展到'跨项目/跨历史标书库'时，用 Passim 式两阶段：n-gram 倒排先做候选生成，再对候选对跑仿射 SW 得到连续雷同段与聚合大段。可直接借鉴其 shingle 参数与聚合逻辑，避免全量两两对齐。
- **参考**：
  - Smith D.A. et al., Detecting and Modeling Local Text Reuse (Viral Texts/Passim), JCDL 2014
  - github.com/dasmiq/passim
  - Programming Historian: Detecting Text Reuse with Passim

### Histogram / Patience / Myers 行级 diff

- **成熟度**：工业级（git 内置，经典但仍是首选）
- **原理**：Myers 求最小编辑脚本（默认）；Patience 只以唯一出现的关键行为锚求 LCS；Histogram 在 Patience 基础上支持低频公共行，速度更快、diff 更可读。
- **为什么更好**：针对'近似逐字照抄'的连续块，行级 diff 比语义方法更快更精确：实证研究（Nugroho 等, EMSE 2020）建议代码/结构化文本改用 --histogram，且 Histogram 比 Myers、Patience 都更快、对齐结果更贴合人类直觉。Patience/Histogram 用'唯一锚点'先钉住再补齐，能避免把不相关行乱配。
- **标书场景用法**：标书里格式化/条款化文本（技术参数表、资格条款、承诺函）逐行照抄的检测与展示：先归一化（去空白/全半角/编号）再跑 histogram diff，直接给出增删改的连续行块，作为最直观的雷同证据渲染。作为嵌入对齐之外的高精度补充通道。
- **参考**：
  - Nugroho, Hata, Matsumoto, How Different Are Different diff Algorithms in Git? Use --histogram for Code Changes, EMSE 2020, https://link.springer.com/article/10.1007/s10664-019-09772-z
  - Myers 1986; Bram Cohen patience diff

### MinHash-LSH / bottom-k sketch 分块过滤（含 TxtAlign、加权 Jaccard 近重复对齐）

- **成熟度**：工业级（MinHash-LSH 是 LLM 语料去重标准件）+ 研究级（TxtAlign/加权 Jaccard）
- **原理**：用 MinHash/SimHash/bottom-k 草图估计集合相似度并经 LSH 分桶，只把落同桶的文档/段落对送去做精对齐，从而避免全量两两比较。
- **为什么更好**：作为对齐前的候选生成层，把 O(n²) 段落对压到近线性候选集；2022 SIGMOD 的 TxtAlign 用 bottom-k 草图给出带准确率保证的近重复文本对齐检索，2025 进一步做加权 Jaccard，弥补传统 seeding–extension–filtering '无准确率保证、超参难调'的缺点。
- **标书场景用法**：标书库规模化时的前置 blocking：对每份标书分段做 MinHash 签名，LSH 找出候选雷同段落对，再交给链化+带状 SW 精对齐与覆盖率计算。单个招标项目 2-5 份可全量对齐，但跨项目围标排查（成百上千份历史标书）必须靠它降复杂度。
- **参考**：
  - Broder 1997 (MinHash, 经典); Charikar 2002 (SimHash)
  - Zhang et al., TxtAlign: Efficient Near-Duplicate Text Alignment Search via Bottom-k Sketches, SIGMOD 2022, https://dl.acm.org/doi/10.1145/3514221.3526178
  - Near-Duplicate Text Alignment under Weighted Jaccard Similarity, arXiv:2509.00627 (2025)

### Cross-Document Attention 多层次文本对齐（学习式神经对齐）

- **成熟度**：研究级
- **原理**：用带跨文档注意力的层次化编码器，弱监督地在句-句、句-篇、篇-篇多个粒度上学习对齐关系。
- **为什么更好**：传统对齐固定在单一预设粒度、无法跨层次；跨文档注意力在引用推荐与抄袭检测上均优于此前的层次化注意力编码器，能在难以用启发式规则捕捉的语义改写上给出对齐信号。是抗深度洗稿/生成式改写的研究前沿方向。
- **标书场景用法**：作为洗稿/LLM 改写标书的加权信号而非唯一裁决：在链化+SW 给出候选连续段后，用跨文档注意力打语义对齐分，辅助判断'语义等价但措辞全变'的段落，弥补词面对齐的漏检。落地需自备标注或弱监督数据，建议作为二级复核层。
- **参考**：
  - Zhou, Pappas, Smith, Multilevel Text Alignment with Cross-Document Attention, EMNLP 2020, https://aclanthology.org/2020.emnlp-main.407/

### GumTree 树编辑距离 / 结构化语义 diff

- **成熟度**：研究级（GumTree 在软件工程界工业可用；文档/表格迁移为研究级）
- **原理**：把结构化文档解析成树（AST/文档结构树），用自顶向下匹配最大同构子树+自底向上匹配子节点，输出 insert/delete/update/move 的结构化编辑脚本。
- **为什么更好**：纯文本 diff 无法对齐到文档结构；树 diff 能捕捉'表格行搬移、条款顺序调换'等结构级雷同，输出 move/update 等高层动作，比行级 diff 更贴合'换汤不换药'的结构抄袭。2024 有 refactoring-aware、SAT-based 等更准更快的改进版。
- **标书场景用法**：docx 标书的表格与章节结构比对：把 docx 解析为结构树（表格/段落/列表层级），用 GumTree 式匹配检测整块表格搬移、条款重排、单元格微改，识别'调换顺序规避查重'的串标手法。作为文字对齐之外的结构维度证据。
- **参考**：
  - Falleri et al., Fine-grained and Accurate Source Code Differencing (GumTree), ASE 2014
  - Refactoring-aware AST Differencing, arXiv:2403.05939 (2024)
  - SAT-DIFF, arXiv:2404.04731 (2024)

### 核查记录（文档对齐与证据定位）

- **[CONFIRMED]** Sánchez-Pérez/Gelbukh/Sidorov adaptive seeding–extension–filtering system won PAN 2014 text alignment with PlagDet ≈ 0.87818 and beat the best PAN 2013 system.
  - 核查依据：Winning notebook (CEUR-WS Vol-1180, CLEF2014wn-Pan-SanchezPerezEt2014.pdf) reports 'best result (Plagdet 0.87818 ... on corpus-2)', 'best-performing system at the PAN 2014 competition', and that it 'outperforms the best-performing system of the PAN 2013 competition'. Caveat: 0.87818 is the entire/all-obfuscation score on corpus-2 (rank #1 there); on the other PAN-2014 test corpus it scored 0.89197 and ranked 3rd. Source: ceur-ws.org/Vol-1180/CLEF2014wn-Pan-SanchezPerezEt2014.pdf; Springer 10.1007/978-3-319-24027-5_42.
- **[CONFIRMED]** Vecalign beats the prior MT-requiring quadratic SOTA by ~5 F1 on German–French and runs in linear time and space (EMNLP 2019).
  - 核查依据：EMNLP-IJCNLP 2019 abstract (ACL Anthology D19-1136, Thompson & Koehn) states Vecalign is 'linear in time and space' and 'outperforms the previous state-of-the-art method (which has quadratic time complexity and requires a machine translation system) by 5 F1 points' on a standard German–French test set. Source: aclanthology.org/D19-1136/.
- **[CONFIRMED]** minimap2 is ≥30× faster than BLASR/NGMLR/GraphMap at comparable accuracy; KSW2 uses the Suzuki–Kasahara difference recurrence for 16-way SSE vectorization, ~3× faster than Parasail's 4-way vectorization (Bioinformatics 2018).
  - 核查依据：minimap2 paper (Li, Bioinformatics 2018, 34:3094) states it is '≥30 times faster than long-read genomic or cDNA mappers' (BLASR/NGMLR/GraphMap etc.) and that 'our 16-way vectorized implementation of global alignment is three times as fast as Parasail's 4-way vectorization', using the Suzuki-Kasahara difference-recurrence for '16-way SSE vectorization regardless of the peak score'. Minor: for long reads the paper claims higher (not merely comparable) accuracy. Source: PMC6137996 / arXiv:1708.01492.
- **[REFUTED]** Nugroho et al. (Empirical Software Engineering 2020) conclude that Histogram diff beats Myers for code, AND that Histogram is faster than both Myers and Patience.
  - 核查依据：The paper (Nugroho, Hata, Matsumoto, EMSE 2020, 'How different are different diff algorithms in Git? Use --histogram for code changes', arXiv:1902.02467 / 10.1007/s10664-019-09772-z) does recommend Histogram over Myers for code changes, but explicitly states it 'only contrasted the two diff algorithms: Myers and Histogram' — Patience was NOT benchmarked — and 'better/more suitable' refers to diff QUALITY (recovering change operations), not execution speed. The paper reports no speed comparison of these algorithms, so the attributed 'Histogram faster than Myers and Patience' finding is not in the paper.
- **[CONFIRMED]** Passim (David A. Smith / Viral Texts) filters document pairs via an n-gram (character shingle) inverted index, then runs Smith–Waterman with affine gap penalties from the matched n-grams.
  - 核查依据：Passim README (author 'David A. Smith', github.com/dasmiq/passim): 'indexes spans of n input characters (i.e., n-grams) and then selects document pairs for alignment only if they share sufficient n-grams', then 'runs a full character-level alignment algorithm starting from the matching n-grams'. Smith–Waterman with affine gap is documented in the published methods (Cordell/Smith, 'Textual Criticism as Language Modeling'). David A. Smith is a Viral Texts co-PI and passim is its alignment engine, though the README itself does not name the project.

## 8. 证据融合、校准与不确定性

### 法庭科学似然比(LR)框架 + ENFSI 口头等级量表(用于文本比对证据陈述)

- **成熟度**：工业/司法级(法庭语音比对、DNA 已常规使用；文本比对为研究级偏应用，有 ENFSI 规范背书)
- **原理**：把每路证据表示为 LR = P(观测|同源/串通假设) / P(观测|独立撰写假设)，多路独立证据按贝叶斯法则相乘(对数相加)合成总强度，再映射到 ENFSI 六级口头量表(LR 1-10 弱支持 … >10^6 极强支持)以便在法律/监管语境陈述。
- **为什么更好**：相比输出一个无量纲'相似度85%'，LR 有明确概率语义、可跨证据类型(文本雷同、元数据、报价规律)统一合成、可被法庭/监管接受；且有标准验证协议(用 Cllr 度量判别力+校准度)。2024 年 Languages 期刊论文系统论证了法庭文本比对必须走'定量特征+统计模型+LR+实证验证'路线才能通过 Daubert 类审查；2024 年提出的 LambdaG(语法模型似然比做作者验证)在 12 个数据集上超过含深度学习在内的 7 个基线，说明 LR 框架本身可拿到 SOTA 判别力。
- **标书场景用法**：标书场景：为每对标书输出若干路 LR(n-gram 指纹重合 LR、语义改写 LR、罕见错别字共现 LR、格式/元数据 LR、报价结构 LR)，对数相加得总 log-LR，报告时译成'极强支持串通编写'等口头等级——这正是给招标监管方出具可申诉报告所需的形式；用历史已判定串标/正常对做 Cllr 验证。
- **参考**：
  - Ishihara et al., Validation in Forensic Text Comparison: Issues and Opportunities, Languages 9(2):47, 2024
  - Nini et al., Grammar as a Behavioral Biometric (LambdaG), arXiv:2403.08462, 2024
  - ENFSI Guideline for Evaluative Reporting in Forensic Science, 2015, https://enfsi.eu/wp-content/uploads/2016/09/m1_guideline.pdf
  - Ishihara, Likelihood ratio estimation for authorship text evidence: score- vs feature-based, Forensic Science International, 2022

### 逻辑回归校准与融合(logistic-regression calibration & fusion，Cllr 验证)——经典但仍是首选

- **成熟度**：工业级(NIST SRE 说话人识别评测和法庭语音比对的事实标准)
- **原理**：把多通道原始分数向量经带正则的逻辑回归线性组合并平移缩放，直接输出校准过的 log-LR；这是说话人识别/法庭语音比对沿用近二十年的标准'多系统融合'流程(FoCal/BOSARIS 工具链)。
- **为什么更好**：相比手工线性加权：权重由数据学得、自动折算各通道的可靠性差异，且输出是可解释的 log-LR 而非任意分数；文献报告校准可把 Cllr 改善高达 95-96%(法庭图像 LR 研究)。参数极少(每通道一个权重+一个截距)，几十~几百个标注对就能稳定训练，是标注稀缺场景的最稳健 learned fusion。
- **标书场景用法**：标书场景的首选融合基线：将'字面重合率、SimHash 距离、语义嵌入相似度、结构相似度、报价特征'等分数经逻辑回归融合为一个校准 log-LR；只需几十份历史判例即可训练；后续任何复杂模型都应与它对比 Cllr 才有资格上线。
- **参考**：
  - Morrison, Tutorial on logistic-regression calibration and fusion: converting a score to a likelihood ratio, Australian Journal of Forensic Sciences 45(2):173-197, 2013 (arXiv:2104.08846)
  - Brümmer & du Preez, Application-independent evaluation of speaker detection (Cllr), Computer Speech & Language, 2006

### 贝叶斯网络 + Noisy-OR 汇聚(对比线性加权)

- **成熟度**：研究级偏司法应用(DNA 混合物解析中的贝叶斯网络已是工业级；文本/串标场景需自建结构)
- **原理**：用有向图显式建模'串通(根因) → 各类可观测痕迹(文本雷同、同一 MAC/IP、报价等差)'的生成关系，多父节点用 Noisy-OR 假设(各原因独立起作用，P(A|B,C)=1-(1-P(A|B))(1-P(A|C)))把条件概率表参数从 O(2^n) 压到 O(n)。
- **为什么更好**：相比线性加权：(1) 输出是合法概率，不会溢出 [0,1]；(2) 能表达'解释消解'(explaining away)——若两份标书雷同已被'都抄了同一招标文件范本'解释，则串通的后验自动下调，线性加权做不到这种去重扣减；(3) Noisy-OR 的 leak 参数天然承载'巧合背景率'。缺点是结构需领域知识手工设计，这也是它在证据推理文献中相对 learned fusion 的定位：可解释性/可辩护性优先时选它。
- **标书场景用法**：标书场景：顶层节点'围标串标'，中间节点'同一编写者/同一机器/协调报价'，叶子为各检测通道输出；共同引用招标文件、行业模板等混杂因素建为独立父节点以吸收假阳性；输出可直接给出'在观测到全部证据下串通的后验概率'并能逐条解释哪路证据贡献最大。
- **参考**：
  - Taroni, Biedermann et al., Bayesian Networks for Probabilistic Inference and Decision Analysis in Forensic Science, 2nd ed., Wiley, 2014
  - Pearl, Probabilistic Reasoning in Intelligent Systems (Noisy-OR), 1988
  - NIST, A Probabilistic Network Forensic Model for Evidence Analysis, https://tsapps.nist.gov/publication/get_pdf.cfm?pub_id=919693

### Learned fusion：GBDT/随机森林/super-learner 集成 + 串标 screens(必要时用 LambdaMART 做可疑对排序)

- **成熟度**：工业级(树集成与 LambdaMART 均为成熟工程组件；串标 screens+ML 属经济学界实证验证、多国反垄断机构试点级)
- **原理**：把多通道分数与领域统计筛(screens：投标价变异系数、覆盖率、价差偏度等，含对每个 3-4 家子团伙分别算 screen 的'子团伙筛')一起作为特征，训练梯度提升树/随机森林/super-learner 集成直接分类'串通 vs 竞争'，或用 LambdaMART 按人工复核价值对标书对做排序。
- **为什么更好**：相比逻辑回归线性融合能自动学到特征间非线性与交互(如'语义相似高 且 报价互补'才危险)：瑞士/日本/意大利采购数据上 screens+ML 的正确分类率约 84%-95%，集成方法(含 super learner)报告 ~90%；2024 年俄罗斯 FAS 判例数据研究报告 91% 准确率并用 Shapley 值解释各因子贡献；不完整卡特尔(团伙外仍有竞争者)场景下'子团伙筛'显著优于整体筛。LambdaMART 至今仍是工业检索排序主力，适合'复核预算有限，按优先级排队'的输出形态。
- **标书场景用法**：标书场景：每对(或每组)投标人构造特征向量 = 文本各通道相似度 + 报价 screens + 元数据同源信号，XGBoost/LightGBM 输出串通分数并用 SHAP 出具因子解释；项目内所有投标对用 LambdaMART 排序生成'先查谁'清单；树模型输出再接校准层(见下条)变成概率。
- **参考**：
  - Huber & Imhof, Machine Learning with Screens for Detecting Bid-Rigging Cartels, International Journal of Industrial Organization, 2019
  - Wallimann, Imhof & Huber, A Machine Learning Approach for Flagging Incomplete Bid-rigging Cartels, Computational Economics 62, 2023 (arXiv:2004.05629)
  - Efimov, Detecting collusion in procurement auctions, arXiv:2411.10811, 2024
  - Burges, From RankNet to LambdaRank to LambdaMART: An Overview, MSR-TR-2010-82, 2010

### 概率校准：Platt scaling / isotonic regression / temperature scaling——经典但仍是首选

- **成熟度**：工业级(sklearn CalibratedClassifierCV 一行可用)
- **原理**：在留出校准集上学一个从原始分数到真实概率的单调映射：Platt 用两参数 sigmoid，isotonic 用非参数分段常数单调函数，temperature scaling 用单参数缩放 softmax logits。
- **为什么更好**：融合模型(尤其树集成与神经网络)的原始分数普遍过度自信，直接当概率展示会误导评标专家：实测神经网络经 Platt 校准 ECE 从 0.226 降到 0.040(改善 82%)，isotonic 达 0.052(77%)；大样本(≳千级)时 isotonic 在 ECE/Brier 上显著优于 Platt，小样本时 Platt 更抗过拟合——这一'小样本用 Platt、大样本用 isotonic'的经验法则自 Niculescu-Mizil & Caruana (2005) 起一直成立。校准是后续 LR 陈述与共形预测阈值都依赖的地基。
- **标书场景用法**：标书场景：融合分类器输出先过校准层再对外展示'该对标书由同一方编写的概率约 97%'；由于串标标注天然稀缺且类别失衡，建议先用 Platt/beta calibration，语料积累后切 isotonic；注意欠采样训练后需按先验回调(2024 年有专文讨论欠采样下 Platt 的偏差修正)。
- **参考**：
  - Niculescu-Mizil & Caruana, Predicting Good Probabilities with Supervised Learning, ICML 2005
  - Guo et al., On Calibration of Modern Neural Networks (temperature scaling), ICML 2017
  - Using Platt's scaling for calibration after undersampling, arXiv:2410.18144, 2024
  - Calibration Meets Reality, arXiv:2509.23665

### 共形预测(split conformal / conformal risk control)构建带统计保证的'转人工复核带'

- **成熟度**：研究级快速转工业级(MAPIE、crepes 等库成熟；金融风控/医疗已有生产部署；2024 TACL 有 NLP 应用综述)
- **原理**：在交换性假设下，用校准集的不一致性分数分位数为每个新样本给出预测集或拒识决策，保证长期错误率不超过用户设定的 α，且不依赖模型与数据分布(distribution-free, model-agnostic)。
- **为什么更好**：相比拍脑袋定'相似度>85% 报警'的阈值：共形预测给出可审计的定量承诺，如'被系统自动放行(判为无嫌疑)的标书对中，漏检率长期不超过 5%'——这是监管场景可写进 SLA 的保证；conformal risk control (ICLR 2024) 把保证从覆盖率推广到任意单调风险(如 FNR)；带拒识分类的共形化版本(2025)专门刻画'模型只对高置信样本自动决策、其余转人工'的三带输出；ICML 2024 实验还表明共形预测集能提升人类决策准确率。PAN 作者验证任务的 c@1 指标(奖励对难例弃权)佐证'允许不作答'在文本取证评测中已是标准实践。
- **标书场景用法**：标书场景：对'串通概率'输出做双阈值共形化——低于下分位自动放行(带漏检率保证)、高于上分位直接标红、中间带强制转人工复核；α 由监管容忍度设定；每个招标项目批次做在线校准以对抗行业/文体漂移(交换性被破坏时用自适应共形)。
- **参考**：
  - Angelopoulos, Bates et al., Conformal Risk Control, ICLR 2024
  - Campos et al., Conformal Prediction for Natural Language Processing: A Survey, TACL 2024 (arXiv:2405.01976)
  - Classification with reject option: distribution-free error guarantees via conformal prediction, 2025
  - Overview of the Cross-Domain Authorship Verification Task at PAN 2021 (c@1/Brier 指标), CEUR-WS Vol-2936

### 罕见短语/错误共现的统计显著性：Dunning log-likelihood ratio(G²) + 大背景语料——经典但仍是首选

- **成熟度**：工业级(语料库语言学与检索三十年标准件；'经典但仍是首选')
- **原理**：用 2×2 列联表比较'某短语在两份标书同时出现'的观测频次与大背景语料下的期望频次，G² = -2logλ 在稀有事件下仍近似 χ² 分布，可给'这个共现有多不可能是巧合'一个 p 值/显著性分数。
- **为什么更好**：Dunning (1993) 证明 χ² 和 z 检验在低频事件上严重失真、会高估罕见词显著性，而 LLR 对小计数依然有效；PMI 则对低频对过度敏感(单次共现即得高分)，不适合直接做证据强度。这正对应标书查重的核心直觉：两家共同用'确保工程质量'毫无证据价值，共同写错同一个冷僻错别字或同一串废标史描述则是强证据——LLR+背景语料把这种直觉变成可辩护的数字，且 G² 可近似转换为该路证据的 log-LR 融入总似然比框架。
- **标书场景用法**：标书场景：预先用大规模中文背景语料(通用网页语料 + 自建历史标书/招标文件语料)统计短语背景概率；对每对标书提取共享的低背景概率 n-gram(错别字、生僻表述、相同的错误数字/单位)，按 G² 打分累加成'罕见共现通道'分数；共同来源于招标文件原文或行业模板的短语先剔除(用招标文件本身当第一层背景)。
- **参考**：
  - Dunning, Accurate Methods for the Statistics of Surprise and Coincidence, Computational Linguistics 19(1), 1993, https://aclanthology.org/J93-1003.pdf
  - Evert, The Statistics of Word Cooccurrences, 2005

### LLM 合成数据构建标注语料(PlagBench 式'生成-筛选-评估'流水线)

- **成熟度**：研究级/竞赛级方法论，但已被工业界广泛用于冷启动(生成-筛选-评估三段式是通行实践)
- **原理**：用指令微调 LLM 对真实文档批量生成逐字抄袭、洗稿改写、摘要式抄袭三类正样本对(附带负样本)，经自动指标+人工抽检两级质检后作为训练/校准/评测语料。
- **为什么更好**：串标标注数据天然极稀缺(判例少、不公开)，合成数据是唯一可规模化的冷启动路径：PlagBench (NAACL 2025) 用 GPT-4 Turbo/GPT-3.5/Llama2-70b 生成 46.5K 对三类抄袭样本并验证了其质量足以暴露现有商用查重器对 LLM 级改写检测不足；PAN 2025 抄袭检测任务同样转向 LLM 生成的抄袭案例，说明这已成社区标准做法。相比只用人工改写语料：LLM 能按难度梯度(轻改写→深度洗稿→跨段重组)可控生成，正好用来标定各融合通道的失效边界和校准曲线。
- **标书场景用法**：标书场景：以历史真实标书章节为种子，用 LLM 按'同一枪手写两份、A 洗稿 B、共用模板但独立写'等剧本生成成对语料(中文为主，含技术方案/施工组织设计等文体)，再叠加 OCR 噪声模拟扫描件；用它训练融合模型、拟合校准曲线、设定共形阈值，并作为回归测试集持续评估'对新一代 LLM 洗稿'的检出率。
- **参考**：
  - Lee et al., PlagBench: Exploring the Duality of LLMs in Plagiarism Generation and Detection, NAACL 2025 (arXiv:2406.16288)
  - Overview of the Plagiarism Detection Task at PAN 2025, arXiv:2510.06805
  - Wahle et al., How Large Language Models are Transforming Machine-Paraphrased Plagiarism, EMNLP 2022

### 核查记录（证据融合、校准与不确定性）

- **[CONFIRMED]** 声明1: LambdaG (Nini et al., arXiv:2403.08462) 在12个数据集上优于含深度学习的7个基线，2026年发表于 Humanities and Social Sciences Communications。
  - 核查依据：arXiv:2403.08462 确为 Nini 等人所著，正式发表于 Humanities and Social Sciences Communications vol.13, art.455 (2026)（nature.com/articles/s41599-025-06340-3；phys.org 2026-04 报道）。论文确用12个数据集、7个基线（含神经网络/Siamese Transformer 等深度学习方法）。但精确表述是：在准确率+AUC上于12个中的11个数据集更优，仅topic-agnostic方法时才是全部12个——'超过全部7个基线'略强于原文，需按此理解。核心事实成立。来源: arxiv.org/abs/2403.08462, nature.com/articles/s41599-025-06340-3
- **[CONFIRMED]** 声明2: Morrison 校准与融合教程发表于 Australian Journal of Forensic Sciences 45(2):173-197 (2013)；法庭图像 LR 研究报告校准使 Cllr 改善最高达 95-96%。
  - 核查依据：Morrison 教程确刊于 AJFS Vol.45 No.2, pp.173-197 (2013)（tandfonline.com DOI 10.1080/00450618.2012.733025）。Cllr 95-96% 的数字属实：Ribeiro 等《Embedding Aggregation for Forensic Facial Comparison》(arXiv:2305.00352, 2023) 报告 CCTV 图像 Cllr 改善最高约95%(0.249→0.012)、社媒图像约96%(0.083→0.003)。需注意：改善主要来自 embedding 聚合（在已校准的 LR 系统内），并非单纯'校准'所致——归因表述稍有偏差，但数字与'法庭图像 LR 研究'的出处成立。来源: tandfonline.com/doi/full/10.1080/00450618.2012.733025, arxiv.org/abs/2305.00352
- **[CONFIRMED]** 声明3: Huber & Imhof screens+ML 在瑞士/日本采购数据上串通vs竞争正确分类率约84%-95%、集成(super learner)约90%；Efimov (arXiv:2411.10811) 报告91%但仅40场拍卖。
  - 核查依据：瑞士数据：Huber & Imhof (IJIO 2019) 平均约84%正确分类，且确用 SuperLearner 集成。日本/跨国：Huber-Imhof-Ishii (JRSS-A 185(3):1074-1114, 2022) 冲绳数据约88%-97%——故84%-95%区间与'super learner约90%'落在多篇论文的实测范围内（属近似汇总，非单一数字）。Efimov arXiv:2411.10811《Detecting collusion in procurement auctions》确报告91%准确率，基于俄罗斯FAS判例，仅40场拍卖按30/70划分训练/测试，样本极小之警示准确。来源: sciencedirect.com/science/article/abs/pii/S0167718719300219, academic.oup.com/jrsssa/article/185/3/1074, arxiv.org/abs/2411.10811
- **[CONFIRMED]** 声明4: PlagBench 含46.5K由 GPT-4 Turbo、GPT-3.5 Turbo、Llama2-70b-chat 生成的三类抄袭文本对，发表于 NAACL 2025 (arXiv:2406.16288)。
  - 核查依据：原文核实全部吻合：共46,500对(6100 SciXGen + 6100 ROCStories + 3300 TLDR × 3种抄袭类型)；三类为 verbatim/paraphrase/summary；三个生成模型正是 Llama2-70b-chat、GPT-3.5 Turbo、GPT-4 Turbo；发表于 NAACL 2025 (ACL Anthology 2025.naacl-long.384)，arXiv:2406.16288。来源: arxiv.org/html/2406.16288v1, aclanthology.org/2025.naacl-long.384
- **[CONFIRMED]** 声明5: Conformal Risk Control 发表于 ICLR 2024 (Angelopoulos, Bates 等)；Conformal Prediction for NLP 综述 TACL 2024 (arXiv:2405.01976)；ENFSI 2015《评估性报告指南》定义六级口头 LR 量表(1-10弱支持至>10^6极强支持)。
  - 核查依据：三项均核实：Conformal Risk Control (Angelopoulos, Bates, Fisch, Lei, Schuster) 确为 ICLR 2024 会议论文；Conformal Prediction for NLP: A Survey 确刊于 TACL 2024 (arXiv:2405.01976, ACL Anthology 2024.tacl-1.82)；ENFSI Guideline for Evaluative Reporting in Forensic Science (2015) 定义六级口头量表：weak(1-10)、moderate(10-100)、moderately strong(100-1000)、strong(1000-10^4)、very strong(10^4-10^6)、extremely strong(>10^6)。来源: proceedings.iclr.cc/paper_files/paper/2024 (Conformal Risk Control), aclanthology.org/2024.tacl-1.82, enfsi.eu/wp-content/uploads/2016/09/m1_guideline.pdf

## 9. LLM 时代的文档比对流水线

### 专用文档解析 VLM：MinerU2.5 / PaddleOCR-VL / GLM-OCR（扫描标书解析首选）

- **成熟度**：工业级（MinerU/PaddleOCR-VL 开源且被国内 RAG/文档系统广泛用于生产；GLM-OCR 2026 年发布）
- **原理**：用 1B 左右参数的专用视觉语言模型端到端把 PDF/扫描页解析为带版式结构的 Markdown/JSON（文本+表格+阅读顺序），替代'版面分析+OCR+表格识别'多模块流水线。
- **为什么更好**：OmniDocBench v1.6 全量榜（CVPR 2025 基准，含中英文 9 类文档、扫描/水印/模糊页）：MinerU2.5-Pro(1.2B) overall 95.75、GLM-OCR(0.9B) 95.22、PaddleOCR-VL-1.5(0.9B) 94.93，全面超过通用大模型 Gemini 3 Pro(92.91)、Qwen3-VL-235B(89.78)、GPT-5.2(86.59) 和传统 pipeline 工具 Marker(78.44)；即'专用小模型打赢通用大模型'。对中文页和复杂表格（TEDS）优势尤其明显，而通用 VLM 在 OmniDocBench v1.0 上中文页落后约 20-30%。MinerU 新版 OCR 后端升级到 PP-OCRv6 后 OCR 指标再提升约 11%。
- **标书场景用法**：标书交叉比对的第一级：docx 直接结构化解析，PDF/扫描件走 MinerU2.5 或 PaddleOCR-VL 转成分段落、分表格的 Markdown/JSON，保留页码与 bbox 坐标，作为后续雷同检测和抽取的'带定位证据'底座。注意 MinerU 团队明确未针对严重倾斜/极端模糊页优化，这类页需 fallback（见通用 VLM 条目）；商用前需核对各工具许可证。
- **参考**：
  - OmniDocBench: Benchmarking Diverse PDF Document Parsing with Comprehensive Annotations (CVPR 2025), https://github.com/opendatalab/OmniDocBench
  - MinerU2.5: A Decoupled Vision-Language Model for Efficient High-Resolution Document Parsing (2025), arXiv:2509.22186
  - GLM-OCR Technical Report (2026), arXiv:2603.10910; PaddleOCR-VL-1.5 (2026), arXiv:2601.21957

### olmOCR 2：RLVR（单元测试奖励）训练的 7B OCR 模型（低成本海量页转文本）

- **成熟度**：工业级（权重开放 HuggingFace，DeepInfra/Parasail 提供 API；但官方定位是英文印刷文档 SOTA，中文能力需自测）
- **原理**：AllenAI 用'合成文档+可验证单元测试'做强化学习（GRPO/RLVR）训练 7B VLM 做 OCR，通过率即奖励，专攻表格、多栏、旧扫描件等硬样例。
- **为什么更好**：olmOCR-Bench 上 82.4 分 vs Marker 76.1、MinerU 75.8；相对初版 olmOCR 表格 +22.6、多栏 +16.1、旧扫描数学 +14.8 个百分点；成本极低：单张 H100 FP8 跑 1 万页 <2 美元。证明'可验证奖励 RL'是 OCR 质量提升的新范式（后被 DeepSeek-OCR、GLM-OCR 等跟进）。
- **标书场景用法**：对英文附件/国际标可直接用；对中文标书主要价值是借鉴其'单元测试奖励'思路——可用招投标领域的合成文档（表格化报价单、盖章扫描页）微调专用解析模型。中文场景仍优先 MinerU/PaddleOCR-VL。
- **参考**：
  - olmOCR 2: Unit Test Rewards for Document OCR (2025), arXiv:2510.19817
  - https://allenai.org/blog/olmocr-2 (发布日期 2025-10-22)

### 通用前沿 VLM 兜底 + 直接看图比对（Qwen3-VL / Gemini / GPT-4o 后继）

- **成熟度**：工业级（Qwen3-VL 开源全尺寸 2B-235B，云 API 成熟）
- **原理**：把退化扫描页（模糊、倾斜、手写签字页、盖章页）整页喂给通用多模态大模型直接读取或直接两图对比，利用其超强鲁棒性补专用解析器的短板。
- **为什么更好**：OmniDocBench 结论：模糊扫描、水印、彩色背景页上通用 VLM（InternVL、Qwen-VL 系列）鲁棒性最好，手写页上通用 VLM（Qwen2-VL 0.298 vs MinerU 0.984 编辑距离）大幅领先 pipeline；Qwen3-VL 在退化图像 OCR 上 87-91% vs 竞品 82%，OCRBench 等综合 OCR 榜领先。传统 OCR（Tesseract/PP-OCR 单独用）在这类页面基本不可用。
- **标书场景用法**：两个用法：1) 解析置信度低的页面自动路由给 VLM 重读（按页路由，控制成本）；2) 图片级雷同取证——把两份标书的疑似雷同页面成对贴图给 VLM，让其指认相同的排版错误、相同的图表/印章复用，这是纯文本比对抓不到的围标证据。
- **参考**：
  - Qwen3-VL Technical Report (2025), arXiv:2511.21631
  - OmniDocBench (CVPR 2025), arXiv:2412.07626

### docx 原生结构解析 + 元数据取证（经典但仍是首选）

- **成熟度**：工业级（国内评标系统标配：机器码/文档 MAC 地址/制作机器指纹比对已写入多地清标规程）
- **原理**：对 docx 格式标书不走 OCR，直接解析 OOXML 得到无损文本、表格、修订记录、作者/公司/创建时间等元数据（python-docx / Docling 等，Docling 为 IBM 开源 MIT 许可，可统一处理 docx/pptx/pdf）。
- **为什么更好**：零识别误差、零成本，且 docx 元数据（creator、company、lastModifiedBy、修订痕迹、相同模板 GUID）本身就是围标串标的直接证据——这是中国评标实务中'软硬件 ID 比对'的核心手段，OCR 路线会把这些证据全部丢掉。任何 LLM 方案都应先走此路径。
- **标书场景用法**：流水线第 0 级：能拿到 docx 就解析 docx（含元数据入库比对）；只有 PDF/扫描件才降级到专用 VLM 解析。同一招标项目下比对各投标文件的作者、公司字段、创建时间聚集性、模板指纹。
- **参考**：
  - https://github.com/docling-project/docling (IBM, MIT license)
  - 智能评标'内容审查+关系排查+软硬件ID'三位一体实践：https://www.smartcity.team/solutions/llms/ai_bidding/

### Schema 约束 LLM 结构化抽取 + LangExtract 式源文本落位

- **成熟度**：工业级（LangExtract 生产可用且支持 Ollama 本地模型；ContractEval 为研究级基准）
- **原理**：用 JSON Schema/函数调用做约束解码，让 LLM 从标书抽取金额、工期、项目经理、业绩、承诺条款等字段，且每个字段强制回填原文字符区间（char offsets）实现可追溯。
- **为什么更好**：招投标文本无统一格式，正则'难写且易错'（多份中文工程实践一致结论）；LLM 语义抽取对表述变体（'工期 180 日历天'/'六个月内完工'）召回显著更高，工具调用式抽取自带类型校验与空值处理。Google LangExtract（2025-07-30 开源，Apache 2.0）把每个抽取实体映射回精确字符偏移，定位不到源文本时显式返回 char_interval=None——直接杜绝'抽取幻觉'。ContractEval（CUAD 数据集，19 个模型）给出关键校准：闭源模型正确性仍领先开源，量化会降正确性，'思考模式'反而可能降正确性。
- **标书场景用法**：对每份标书跑同一 schema 抽取，生成'投标人 × 字段'矩阵：报价构成、工期、项目班子、设备型号、业绩项目名。交叉比对矩阵即可程序化发现：多家报价尾数规律、同一项目经理出现在两家标书、业绩雷同——这正是 195 号文场景 17'商务标关键报价特征比对'的实现方式。所有字段带 offset，一键跳转原文举证。
- **参考**：
  - https://github.com/google/langextract (Apache 2.0)
  - ContractEval: Benchmarking LLMs for Clause-Level Legal Risk Identification in Commercial Contracts (NLLP Workshop 2025), arXiv:2508.03080
  - https://developers.googleblog.com/introducing-langextract-a-gemini-powered-information-extraction-library/

### 两级雷同召回：SimHash/MinHash + 中文微调向量 粗筛 → LLM 精判（经典但仍是首选）

- **成熟度**：工业级（学术查重、国内标书查重产品的通用架构）
- **原理**：先用指纹（SimHash/MinHash）抓逐字雷同、用中文 embedding（bge 系）近邻检索抓改写雷同，把 O(n²×段落²) 的比对压缩到少量候选对，再交给 LLM 精判。
- **为什么更好**：成本与精度分工：指纹/向量每千段成本近零；而研究表明通用 LLM 直接做改写判定并不可靠——PAWS-X 等对抗性改写基准上 LLM 仅略高于抛硬币、被小得多的微调 BERT 类模型大幅超越（PARAPHRASUS 2024/PlagBench 2024 结论），因此'全靠大模型逐对比'既贵又不准；正确架构是廉价高召回粗筛 + LLM 带证据精判。
- **标书场景用法**：对 2-5 份标书的技术标全文分段后：SimHash 抓复制粘贴（含错别字雷同——最强围标证据）；embedding 近邻抓洗稿改写候选；候选段落对连同上下文送 LLM 判定'是否实质雷同、雷同类型（模板通用语/抄袭/同源）'并给出双侧原文引证。
- **参考**：
  - PARAPHRASUS: A Comprehensive Benchmark for Evaluating Paraphrase Detection Models (2024), arXiv:2409.12060
  - PlagBench: Exploring the Duality of Large Language Models in Plagiarism Generation and Detection (2024), arXiv:2406.16288

### LLM-as-Judge 成对比对协议：位置对消 + 低温多数决 + 引证强校验

- **成熟度**：工业级（LLM 评估平台如 Comet/主流 eval 框架的标准做法；单篇结论为研究级）
- **原理**：对候选雷同段/矛盾点用 LLM 做成对裁决，工程上固定三件套：每对交换顺序跑两次取一致结果（position-consistency）、温度 0/低温 + 多次采样多数决、输出必须含逐字引文并用字符串匹配程序化验证引文真实存在。
- **为什么更好**：有量化依据：LLM 判官存在显著位置偏差，且在两候选质量接近时最严重（Judging the Judges, 2024）；跨判官成对偏好平均翻转 13.6%；高温显著破坏稳定性并增加格式解析错误，低温输出高度可复现（The Necessity of Setting Temperature in LLM-as-a-Judge, 2026）。相比无协议裸用 LLM，该协议把裁决从'不可复现的主观意见'变成可审计流程。
- **标书场景用法**：雷同精判与'谁抄谁/是否同源'判定必须成对交换跑两次；对'雷同/不雷同'边界样本自动升级为多模型合议或送人工；判决 JSON 中每条结论强制附双方原文 quote + 页码，后处理校验 quote 逐字命中解析文本，不命中即拒绝该结论重跑。
- **参考**：
  - Judging the Judges: A Systematic Study of Position Bias in LLM-as-a-Judge (2024), arXiv:2406.07791
  - The Necessity of Setting Temperature in LLM-as-a-Judge (2026), arXiv:2603.28304
  - The Coin Flip Judge? Reliability and Bias in LLM-as-a-Judge Evaluation (2026), arXiv:2606.13685

### 推理层确定性：批不变算子（batch-invariant ops）实现比特级可复现

- **成熟度**：研究级偏工程（开源库 batch-invariant-ops，vLLM 集成已演示；生产大规模使用尚少）
- **原理**：温度 0 仍不确定的根因是服务端批处理改变浮点累加顺序；Thinking Machines 开源 batch-invariant 的 RMSNorm/MatMul/Attention 替换算子并接入 vLLM 确定性模式，实现同 prompt 比特级相同输出。
- **为什么更好**：Qwen3-8B 上 1000 次重复推理从'数十种不同输出'变为 1000 次完全一致（代价约 61.5% 吞吐损失）。对'AI 出具围标嫌疑结论'这类可能被质疑/复议的场景，可复现性是合规刚需——传统只设 temperature=0 做不到。
- **标书场景用法**：本地部署评审模型时开启 vLLM 确定性模式 + 固定模型版本/提示词版本号，把每次裁决的（模型hash、prompt、输出）写入审计日志，保证监管复查时可逐字复现当初结论。云端 API 无法保证时，退而求其次：留存完整输入输出 + 多次采样一致率。
- **参考**：
  - Defeating Nondeterminism in LLM Inference, Thinking Machines Lab (2025), https://thinkingmachines.ai/blog/defeating-nondeterminism-in-llm-inference/

### 跨文档事实矛盾检测：LLM 长上下文 + NLI 混合

- **成熟度**：研究级（法律域 LegalWiz 2025、金融 AUDITFLOW 2026 等均为论文/原型；但'字段级对碰'的工程简化版可直接落地）
- **原理**：把两份文档的对齐单元（同一投标人的不同章节、或抽取后的同名字段）组成前提-假设对，用 NLI 模型/LLM 判定蕴含-矛盾，检测'前后矛盾'与'跨标书事实冲突'。
- **为什么更好**：纯靠 GPT-4 级模型整篇找矛盾不可靠：ContraDoc（2023）显示 GPT-4 对长文档中的细微自相矛盾表现挣扎、人工标注员也大量漏检；2024-2026 的改进路线（先结构化/抽取对齐再逐对判定，如 SIFiD、LegalWiz 多智能体矛盾检测）显著提升召回——即'先抽取对齐、再小单元判定'优于'整篇扔给大模型'。
- **标书场景用法**：两类用法：1) 单标书内部一致性（投标函报价 vs 报价明细表合计 vs 大写金额；工期承诺前后不一）——这是国内'智能清标'已落地的高价值点；2) 跨标书矛盾（同一台设备/同一人员同时承诺给两家投标人）作为围标信号。优先用结构化抽取字段做程序化对碰，语义类矛盾才走 LLM/NLI。
- **参考**：
  - ContraDoc: Understanding Self-Contradictions in Documents with Large Language Models (2023/2024), arXiv:2311.09182
  - LegalWiz: A Multi-Agent Generation Framework for Contradiction Detection in Legal Documents (2025), arXiv:2510.03418
  - SIFiD: Reassess Summary Factual Inconsistency Detection with LLM (2024), arXiv:2403.07557

### Agentic 多步审查流水线（国内'智能清标/辅助评标'已实战验证）

- **成熟度**：工业级（国内多厂商生产部署：广联达、北京筑龙等；学术框架为研究级）
- **原理**：把审查分解为专职智能体/阶段：解析→字段抽取→规则对碰→相似度召回→LLM 成对精判→矛盾检测→证据汇总报告，配 Reflection 与人工复核关口，替代单次长 prompt。
- **为什么更好**：多智能体分解在尽调/审计域已验证优于单次调用（VC 尽调多智能体编排 2026、多轮智能体审计 9 轮发现 51 处一致性缺陷的案例）；国内工业落地更直接：广联达基于 AecGPT 的 AI 交易大模型（智能清标+辅助评标）已在贵州、广州等多地评标现场部署，智能清标可自动筛出硬性不符、资料缺失与前后不一致；八部委 195 号文（2026-02）将'围串标识别（场景17：商务标报价特征比对+技术方案语义相似性分析+多维数据碰撞）'列为国家推广场景，等于官方背书这套流水线形态。
- **标书场景用法**：标书交叉比对系统的总体架构模板：每一步产出结构化中间产物（可单独测试、可缓存、可换模型），LLM 只做单步小任务；最终报告由确定性代码从中间产物组装，附全部证据链；'嫌疑'级结论必须过人工确认关口。
- **参考**：
  - A Multi-Agent Orchestration Framework for Venture Capital Due Diligence (2026), arXiv:2605.13110
  - 广联达 AI 交易整体解决方案（2024-10 发布，多地评标落地）：https://finance.sina.com.cn/jjxw/2024-10-26/doc-inctwhnz0664762.shtml
  - 发改法规〔2026〕195号《关于加快招标投标领域人工智能推广应用的实施意见》解读：https://www.smartcity.team/solutions/llms/ai_bidding/

### 本地部署选型：Qwen3-32B/30B-A3B 为抽取比对主力，前沿大模型只做终审

- **成熟度**：工业级（Qwen3/vLLM 生产成熟；具体任务精度需自建标书评测集验证）
- **原理**：抽取、相似判定、矛盾检测等'小步任务'用 32B 级开源模型本地跑（vLLM/SGLang 服务化），仅把最难的成对终审/报告生成路由到 235B 级或云端前沿模型。
- **为什么更好**：Qwen3（2025-04-29，Apache 2.0，0.6B-235B-A22B 全尺寸，官方称 Qwen3-4B 可比肩 Qwen2.5-72B-Instruct）大幅拉低了'够用'门槛；DeepSeek-R1-Distill-Qwen-32B（2025-01，MIT）量化后 16-24GB 显存即可跑，vLLM 吞吐可达 Transformers 数十倍。但 ContractEval 给出重要警示：法律类任务闭源模型正确性仍领先、开源模型更常漏答（'no related clause'偏多）、量化有可测的正确性损失——所以召回敏感环节（抽取、初筛）应放宽阈值让后级兜底，终审用最强模型。标书涉密性也决定了本地化是国内评标场景的硬约束。
- **标书场景用法**：参考配置：解析用 MinerU2.5(1.2B，单卡即可)；抽取+段落精判用 Qwen3-32B 或 30B-A3B（2×24GB 或单 48GB）；终审 judge 用 Qwen3-235B-A22B/云端模型；全链路 vLLM 部署并开确定性模式。避免在裁决环节用深度量化模型。
- **参考**：
  - Qwen3 官方发布（2025-04-29）：https://qwenlm.github.io/blog/qwen3/
  - DeepSeek-R1: Incentivizing Reasoning Capability in LLMs via Reinforcement Learning (2025), arXiv:2501.12948
  - ContractEval (2025), arXiv:2508.03080

### 可举证性与合规框架：引证三重校验 + 人机权责边界

- **成熟度**：工业级要求 + 研究级技术（引证校验流程可工程化落地；引证图谱/DPO 为前沿研究）
- **原理**：每条 AI 结论绑定'引文存在性（逐字命中原文）、引文相关性、引文时效性'三重程序化校验，AI 输出定位为'线索+证据'而非'定性结论'，最终定性由评标专家/招标人做出。
- **为什么更好**：法律域审计一致发现相当比例 LLM 声明无引证或错误归因，引证忠实度已成法律 AI 部署的硬性前提；引证图谱校验 + DPO 训练可把'识别伪造引证'的准确率做到 98.5%（Citation Grounding, 2026）。合规上这不是可选项：195 号文明确'模型生成的结论不替代招标人等的自主判断'、'严防算法歧视和模型幻觉'——把 AI 定位为举证辅助恰好同时满足技术上限与监管要求。
- **标书场景用法**：报告层设计：每个雷同/矛盾/围标信号 = {结论类型, 置信度, 双侧逐字引文, 页码/bbox 定位, 采用的模型与版本, 复现参数}；引文经字符串匹配校验后才允许进入报告；未通过校验的结论自动降级为'待人工核查'。对监管复议保留完整审计日志与确定性复现能力。
- **参考**：
  - Citation Grounding: Detecting and Reducing LLM Citation Hallucinations via Legal Citation Graphs (2026), arXiv:2606.00898
  - Citation-Closure Retrieval and Per-Rule Attribution for Real-World Regulatory Compliance QA (2026), arXiv:2605.29742
  - 发改法规〔2026〕195号（八部委，2026-02-06）

### 核查记录（LLM 时代的文档比对流水线）

- **[CONFIRMED]** OmniDocBench v1.6 排行榜：MinerU2.5-Pro(1.2B) 95.75、GLM-OCR(0.9B) 95.22、PaddleOCR-VL-1.5(0.9B) 94.93，高于 Gemini 3 Pro(92.91)、Qwen3-VL-235B(89.78)、GPT-5.2(86.59)
  - 核查依据：直接抓取 raw.githubusercontent.com 上 opendatalab/OmniDocBench 的 README 逐字 grep 验证：表 'Comprehensive evaluation of document parsing on OmniDocBench (v1.6_full)' 中六个 overall 分数与声明完全一致，MinerU2.5-Pro 链接到 HuggingFace 1.2B 权重（MinerU2.5-Pro-2605-1.2B）；注意该仓库 2026-04-30 已更新至 v1.7（新增 Qianfan-OCR 榜），但主榜仍标注 v1.6_full。
- **[CONFIRMED]** olmOCR 2 于 2025-10-22 发布，7B 模型经 RLVR（单元测试奖励）训练，olmOCR-Bench 82.4（Marker 76.1、MinerU 75.8），单张 H100 FP8 处理 1 万页成本低于 2 美元
  - 核查依据：allenai.org/blog/olmocr-2 官方博客证实：2025-10-22 发布、基于 Qwen2.5-VL-7B、用 GRPO 以单元测试 pass/fail 作奖励（RLVR）、olmOCR-Bench 82.4 分（Marker 76.1、MinerU 75.8）、单张 H100 FP8 处理 1 万页 'less than $2'；arXiv:2510.19817 标题即 'olmOCR 2: Unit Test Rewards for Document OCR'，摘要与训练方法一致。
- **[CONFIRMED]** 八部委发改法规〔2026〕195号（2026-02-06）定义招投标 AI 20 个应用场景，场景 17 为围串标识别（商务标报价特征比对+技术方案语义相似性分析），并明确'模型生成的结论不替代招标人自主判断'、'严防算法歧视和模型幻觉'
  - 核查依据：官方发布页存在于 ndrc.gov.cn 与 gov.cn（本环境 TLS 无法直连，经安全内参 secrss.com/articles/87841 全文转载逐字核对，含八部委落款与 2026年2月6日 成文日期）：确为 20 个场景，'17.围串标识别' 原文含'技术方案语义相似性分析、商务标关键报价特征比对'；'模型生成的结论不替代招标人、招标代理机构、投标人、评标专家等的自主判断'为原文（声明为缩略引用）；但'严防算法歧视和模型幻觉'非逐字原文，原文措辞为'有效防范和应对模型黑箱、幻觉和算法歧视等风险'——实质确认、该处引文属转述。
- **[CONFIRMED]** Google LangExtract 于 2025-07-30 开源（Apache 2.0），抽取结果强制映射回源文本精确字符偏移（char_interval），定位失败时显式返回 None
  - 核查依据：developers.googleblog.com 官方博文发布日期 2025-07-30，原文称 'Every extracted entity is mapped back to its exact character offsets in the source text'；github.com/google/langextract 确认 Apache 2.0 许可，文档明确 '无法在源文本中定位的抽取项 char_interval = None' 并给出过滤示例。
- **[REFUTED]** Thinking Machines batch-invariant-ops 使 Qwen3-8B 在 1000 次重复推理中输出比特级一致（此前温度 0 下数十种不同输出），代价约 61.5% 吞吐损失
  - 核查依据：抓取 thinkingmachines.ai 博客原文逐字核对：现象属实但两处关键细节与原文相反——1000 次重复实验（80 种唯一输出→启用 batch-invariant kernels 后 1000 条全部一致）用的是 Qwen3-235B-A22B-Instruct-2507 而非 Qwen3-8B（Qwen3-8B 仅用于性能测试）；'61.5% 吞吐损失'是误读：原文数据为 26s→42s（优化后确定性 vLLM），即运行时间增加 61.5%、等价吞吐下降约 38%（未优化版 55s 约 2.1 倍慢），原文也未对该实验用'比特级'措辞（bitwise 一词出现在 RL 训练一致性语境）。

## 10. 完整性批评：识别出的遗漏方向

- **工程量清单/分项报价表的结构化逐项雷同分析（表格级比对 + 单价向量统计）**：商务标的核心载体是大体量结构化表格（分部分项综合单价、措施费、取费费率、主材价），纯文本相似度方法对表格要么失效要么严重失真。这在中国已是法定化实践：如青岛公共资源交易平台规则明确'不同投标人清单报价达 80% 相同即视为电子投标文件内容雷同'并作为串通投标立案依据；国内清标软件的核心指标就是综合单价雷同率、组价链/取费一致性、相同计算错误项。它需要一个独立算法族：按清单编码逐项对齐 → 单价向量相关性/逐项雷同率 → 组价构成（人材机费率）一致性 → 不平衡报价模式，且此类数值证据比文本相似更贴近法定认定标准。现有 7 维只覆盖标内总价分布筛查（Imhof 等）和'计价软件锁号'，item 级数值比对完全缺位，会直接改变方案的模块划分（需表格解析+数值分析管线）。
- **均值基准价类评标规则下的串标模式检测（Conley & Decarolis 2016，评标办法感知的报价筛查）**：中国综合评分法普遍以投标均价（或去极值加权均价）为基准价，与意大利 average-bid auction 结构同构。该机制下卡特尔最优策略是用多个陪标报价刻意'做形状'抬/压基准价，使自家报价最接近基准——此时报价分布不是低方差口袋，Imhof 类方差/CV 筛查会失效甚至反向误导。Conley & Decarolis 'Detecting Bidders Groups in Collusive Auctions'（AEJ: Microeconomics 2016）正是针对该机制的成组统计检验（入场协调+报价协调联合检验），并在法院已判串标案上验证过效力。这会实质改变'报价筛查'子方案：必须按评标办法（最低评标价 vs 基准价评分）分流选用不同的筛查统计量，而现有第 4 维清单全部默认最低价/第一价格拍卖假设。
- **对抗性规避与预处理鲁棒性（威胁模型层：不可见字符攻击、PDF 文字层-渲染不一致、文字转图片）**：查重系统一旦投入使用，串标者会主动规避，而全部 7 维方案都隐含'抽取到的文本 = 文档真实内容'的假设。已被验证的攻击面：同形字/零宽字符/双向控制符（Boucher et al. 'Bad Characters: Imperceptible NLP Attacks', IEEE S&P 2022——1~3 个不可见扰动即可击穿包括查重在内的商用 NLP 系统）；PDF 字体 ToUnicode 重映射与隐藏文字层使渲染内容与抽取文本完全不同（'Complete Evasion, Zero Modification: PDF Attacks on AI Text Detection', arXiv 2508.01887，零视觉改动即可完全规避检测）；以及把雷同段落整段转成图片。对策是独立的预处理防线：Unicode 安全归一化（NFKC + confusables 同形字映射）、渲染后 OCR 与抽取文本交叉验证、不一致即报警——且'检测到刻意规避'本身就是极强的串通证据。这决定整个管线的入口架构，缺了它所有下游算法可被无声绕过。
- **合法共享内容的系统性剔除（招标文件对减 + 行业范本背景库 + k-共现过滤），需从'一句带过'升级为独立方法方向**：现有清单仅在维度 2 的一条工业实践里以'引用内容过滤'四个字带过，没有任何方法级覆盖，但这是误报控制的成败关键：标书天然存在大量合法雷同——对招标文件条款的逐字应答、法定格式文本、行业标准范本（九部委标准施工招标文件等）、证书模板，监管规则也明确豁免（如'与已标价工程量清单出现雷同的除外'）。需要独立算法研究：与招标文件全文对减比对（先剥离引用再算相似度）、范本/历史背景库 + 段落级 IDF 或显著性加权、MOSS base-file 式 k-共现过滤（在 2-5 份比对里：≥3 家共有的段落大概率是模板，恰好且仅 2 家共有的才可疑——这个组合逻辑本身就是强判别特征）、按章节类型（法定格式 vs 技术方案 vs 报价说明）分层设阈值。缺此模块，任何相似度引擎的输出都会被模板噪声淹没，产品不可用。

## 11. 补漏调研：工程量清单/分项报价表的结构化逐项雷同分析（表格级比对 + 单价向量统计）

### 法定化逐项雷同率筛查（清单编码对齐 + 综合单价逐项相同率 / pairwise identical-rate screening）

- **成熟度**：工业级（中国省市级电子评标平台法定实践，青岛、贵州等地已成文）
- **原理**：按 GB50500 十二位清单编码逐项对齐各投标人的已标价工程量清单，两两计算综合单价完全相同（或差额恒定/等比）的条目占比，超过监管阈值即触发雷同认定。
- **为什么更好**：纯文本相似度对大体量报价表要么失效要么失真，而逐项相同率直接对应中国监管的法定认定口径：青岛公共资源交易平台规则将'不同投标人电子投标文件相同内容达到80%'作为内容雷同、进而作为串通投标处理依据；实务案例（中交二公局案）以'报价清单雷同率90.48%'定案罚款202.01万元。数值证据可直接作为否决投标与行政立案证据链，这是任何 embedding/文本方法都替代不了的。
- **标书场景用法**：作为独立的'商务标数值比对'模块：解析各标书的分部分项/措施项目/主材表 → 按清单编码+项目特征对齐 → 输出两两投标人的逐项雷同率矩阵（相同单价数/可比条目数）、恒定差额和等比折扣检测，并按 80%（可配置）阈值出红色告警；同时统计'相同计算错误项'（错且错得一样）作为最强证据。
- **参考**：
  - 青岛市《关于工程建设项目招标投标活动电子投标文件雷同认定及处理的通知》 https://ggzy.qingdao.gov.cn/PortalQDManage/PortalQD/NoticeInfo?ItemId=c9cf2d16-9696-4e87-9e46-decbb60ab466
  - 贵州省发改委等十部门《关于工程建设项目电子投标文件雷同认定及处理的指导意见》(2024) https://fgw.guizhou.gov.cn/zwgk/gzhgfxwjsjk/gfxwjsjk/202407/t20240724_85153462.html
  - 串标案例汇编（中交二公局案，清单雷同率90.48%） https://www.cnblogs.com/newjpz/p/20069880

### 组价构成链一致性分析（cost build-up chain consistency，清标软件核心指标）

- **成熟度**：工业级（广联达/新点等国内清标软件已产品化，评标现场实战使用）
- **原理**：不看最终单价而比对每条清单项的组价过程——套用定额子目、补充定额、人材机含量调整、费率取定是否在不同投标人之间异常一致，从'结果雷同'深入到'过程同源'。
- **为什么更好**：串标者常在最终单价上做扰动（加随机小数、统一打折）绕过逐项相同率，但组价链（多定额组合、补充定额编号、含量调整值、补充人材机条目）来自同一份计价软件工程文件时几乎不可能自然重合；国内商用清标工具（广联达云清标等）把'多定额一致、补充定额一致、含量调整一致、补充人材机一致'做成标准维度并宣称围串标识别准确率超95%、效率较人工提升90%+。这是对'洗稿式改价'最有效的反制，且与计价软件加密锁号证据互相印证。
- **标书场景用法**：若能拿到计价文件（如广联达 GBQ、导出的 XML/接口标准格式）则直接比对组价树；只有 PDF/扫描件时退化为可提取层：每条清单项的人材机分析表、管理费利润率、主材单价表逐字段比对，输出'组价指纹'哈希后两两碰撞，一致项列为高权重证据。
- **参考**：
  - 《建设工程围串标乱象如何破解？数字化清标工具成关键抓手》 https://www.fwxgx.com/articles/243934
  - 《什么是清标？清标软件有什么用？》 https://www.sohu.com/a/822787244_121123906

### 报价向量统计筛查 + 机器学习（screens + ML，Imhof–Huber 谱系在 item 级的延伸）

- **成熟度**：研究级偏工业级（瑞士竞争委员会 COMCO、OECD 竞争数据筛查报告背书，用于执法线索筛查）
- **原理**：对每个清单项计算跨投标人的离散度筛查量（变异系数、极差比、峰度、最低两价相对距离），再把整套单价向量的两两相关系数/余弦相似度、比值向量方差（检测固定折扣）等 screens 喂给随机森林/集成分类器输出串通概率。
- **为什么更好**：相比人工看总价分布，screens+ML 在瑞士/日本/意大利真实卡特尔数据上正确分类率 84%–95%（冲绳完全卡特尔 88%–97%）；仅用变异系数+相对距离两个筛查量的决策树也有 81.6%。将同一族统计量从'标的总价'下沉到'单价向量'后，还能检测规律性差异（等差数列报价、恒定百分比折扣）——这正是《招标投标法实施条例》第40条的法定情形。子集 coalition screens（对每 3–4 家投标人子组合算筛查量）可发现不完全围标（围标圈+独立陪衬并存），跨国验证准确率 85%–94%。
- **标书场景用法**：在对齐后的单价矩阵（行=清单项，列=投标人）上：逐项算 CV 并统计'低离散项占比'；两两投标人算 Pearson/Spearman/余弦相似度与比值向量的变异系数（≈0 即整体折扣克隆）；对每个投标人子集算 coalition screens 以定位围标圈成员；最后用规则分层或小型梯度提升模型合成'串通概率分'。无需标注数据时可先纯规则运行，积累案例后再训模型。
- **参考**：
  - Huber & Imhof, Machine learning with screens for detecting bid-rigging cartels, Int. J. Industrial Organization (2019) https://www.sciencedirect.com/science/article/abs/pii/S0167718719300219
  - Huber, Imhof & Ishii, Transnational machine learning with screens for flagging bid-rigging cartels, JRSS-A (2022) https://academic.oup.com/jrsssa/article/185/3/1074/7068943
  - Imhof & Wallimann, Detecting bid-rigging coalitions in different countries and auction formats (2021) https://arxiv.org/pdf/2105.00337
  - Wallimann, Imhof & Huber, A Machine Learning Approach for Flagging Incomplete Bid-Rigging Cartels, Computational Economics (2022) https://arxiv.org/pdf/2004.05629
  - Public procurement cartels: A large-sample testing of screens using machine learning (2025) https://www.sciencedirect.com/science/article/pii/S0167718725000943

### 两两报价交互图 + CNN（deep learning screens）

- **成熟度**：研究级
- **原理**：把参考投标人与其他投标人在同批清单项上的归一化价格画成两两散点图，用卷积神经网络做图像分类直接判别'串通 vs 竞争'的交互模式。
- **为什么更好**：免去手工设计筛查量，能捕捉筛查统计量表达不了的非线性共动结构；在日本+瑞士数据上单国/混合训练平均准确率约 90% 或更高，跨国迁移仍保持较高水平。对本场景的独特价值：单价向量天然构成高维散点图（每个清单项一个点），比原论文的'跨标段总价'数据密度高一个量级，图案（沿对角线的整体折扣带、分段平移带）可直接可视化为评标专家能看懂的证据图。
- **标书场景用法**：对每对投标人生成'归一化综合单价散点图'（x=甲单价/项目中位价，y=乙单价/项目中位价），CNN 输出串通概率；即便不上模型，这个散点图本身就是报告里最直观的雷同证据可视化（完全雷同=点全落在对角线上，固定折扣=平行于对角线的直线）。
- **参考**：
  - Huber & Imhof, Deep learning for detecting bid rigging: Flagging cartel participants based on convolutional neural networks (2021) https://arxiv.org/abs/2104.11142

### 不平衡报价识别（unbalanced bidding detection：基准价偏离 + 前重后轻模式 + MCDM/VIKOR）

- **成熟度**：工业级（清标软件与多省评标办法内置'综合单价合理性分析'环节）+ 研究级（MCDM/ML 变体）
- **原理**：以招标控制价或跨投标人稳健统计量（中位数/置信区间中心）为每项基准价，标记偏离超阈值（实务常用±20%–30%）的清单项，并识别前期项目抬价、后期压价、预期量增项抬价等策略性模式。
- **为什么更好**：雷同率抓'抄'，不平衡报价抓'算计'——两者互补构成商务标数值体检的完整闭环；相比拍脑袋看总价，逐项基准价偏离能定位具体风险条目并量化业主超支敞口。学术侧已有系统综述（ASCE 2024）与把'清单项为准则、投标人为备选'的 VIKOR 多准则模型；工程侧样本≥25 项时可做正态性检验+置信区间估计基准价（2017 年系统设计即已成型）。在围标场景中，多家标书呈现相同的不平衡模式本身又是一个强串通信号。
- **标书场景用法**：对每个清单项计算各投标人单价对基准价（控制价单价、全体投标人中位价）的偏离百分比，输出：单标'不平衡度'热力图（按分部分项排序看前重后轻）、超阈值条目清单及其对合同风险的金额敏感度；再对多标间的偏离方向向量做相似度，检出'同一套不平衡策略批量复制'的围标模式。
- **参考**：
  - Prevention and Detection of Unbalanced Pricing in Bidding for Construction Projects: A Review, ASCE J. Legal Affairs & Dispute Resolution (2024) https://ascelibrary.org/doi/abs/10.1061/JLADAH.LADR-1200
  - An identification model of unbalanced bidding based on VIKOR, J. Civil Engineering and Management https://journals.vilniustech.lt/index.php/JCEM/article/download/11568/9573/32797
  - 《不平衡报价识别方法系统设计》价值工程 (2017) https://m.fx361.com/news/2017/0504/1678098.html

### 数字分布检验（TAB / Benford-type digit screens on 单价）

- **成熟度**：研究级偏工业级（Benford 检验是法务审计标配功能，TAB 为学术验证的招投标专用变体）
- **原理**：对全部清单单价的首位/末位数字分布做 Benford 律与数字偏好检验，人为编造或系统性篡改的价格集合会显著偏离自然分布。
- **为什么更好**：几乎零成本（纯统计、无需对齐、无需训练数据），且对'一人编多份标、手工改数'这种最常见围标作业方式敏感——编造者倾向重复特定数字与整数尾数；ASCE 的 TAB（Test of Abnormal Bid）模型已在西弗吉尼亚沥青摊铺操纵案上验证可快速定位被操纵的材料单价。经典但仍是审计首选的低成本初筛。
- **标书场景用法**：作为管线最前端的廉价初筛：对每份标书的单价集合算首位数字χ²偏离、末位数字均匀性、整数/五数尾偏好；单标显著异常提示编造，多标呈现相同的异常数字指纹则升级为串通线索并进入逐项比对。
- **参考**：
  - TAB Bid Irregularity: Data-Driven Model and Its Application, ASCE J. Management in Engineering (2021) https://ascelibrary.org/doi/10.1061/%28ASCE%29ME.1943-5479.0000958
  - OECD, Data screening tools in competition investigations (2022) https://one.oecd.org/document/DAF/COMP/WP3(2022)5/en/pdf

### 表格结构化抽取管线（PP-StructureV3 / OmniDocBench 谱系）作为数值比对的前置层

- **成熟度**：工业级（PaddleOCR 3.0 开源发布，2025；OmniDocBench 为 CVPR 2025 公认基准）
- **原理**：用版面分析+表格识别管线（或表格专用 VLM）把 PDF/扫描件里的分部分项报价表、人材机分析表还原为带行列结构的 HTML/JSON，再进入数值分析。
- **为什么更好**：整个 item 级比对的成败取决于表格抽取质量：PP-StructureV3 在 CVPR 2025 基准 OmniDocBench 上是当前管线类方案的 SOTA，中英文文档解析显著优于其他 pipeline 工具、与主流专家 VLM 相当，且开源、可本地部署、对中文表格（合并单元格、跨页续表）优化充分——比通用 LLM 直接读 PDF 便宜且行列错位率低得多。对扫描件唯一可行路径就是这类 OCR+表格结构识别。
- **标书场景用法**：docx 直接读 XML 表格；PDF 先试文本层抽取（表格线+坐标聚类），失败或为扫描件时走 PP-StructureV3（版面检测→表格结构识别 PP-TableMagic→单元格 OCR）；抽取后做表头语义归一（项目编码/名称/单位/工程量/综合单价/合价列的自动识别），并用'工程量×单价≈合价'的行内代数约束自动校验与纠错 OCR 数字——该约束同时顺带发现'相同算术错误'证据。
- **参考**：
  - PaddleOCR 3.0 Technical Report (2025) https://arxiv.org/abs/2507.05595
  - OmniDocBench, CVPR 2025 https://github.com/opendatalab/OmniDocBench
  - PP-StructureV3 文档 https://paddlepaddle.github.io/PaddleOCR/main/en/version3.x/algorithm/PP-StructureV3/PP-StructureV3.html

### 语义实体匹配做清单行对齐（PLM/LLM entity matching：Ditto → GPT-4 级 LLM 匹配）

- **成熟度**：研究级偏工业级（实体匹配技术本身工业成熟，BOQ 领域应用为研究级）
- **原理**：当清单编码缺失、被改写或表为非标格式（措施费、主材价、暂估价表）时，用预训练语言模型/LLM 对'项目名称+特征描述+单位'做语义记录链接，把不同标书的行对齐到同一实体。
- **为什么更好**：编码精确匹配对规范清单够用，但围标者会微调项目特征描述、拆并条目来破坏对齐；Ditto（PVLDB 2020）用 BERT 做实体匹配比此前 SOTA 提升最多 29% F1，GPT-4 级 LLM 零样本/少样本匹配在 WDC 等基准上进一步领先且无需训练数据；建筑领域已验证可行——RoBERTa 对公路工程 BOQ 工项编码自动分类准确率 91%（ASCE JCEM 2023）。中文场景可用 bge 系列向量粗召回 + LLM 精判，成本可控。
- **标书场景用法**：对齐策略分三层：① 清单编码前9/12位精确匹配；② 编码失配时用中文 embedding（如 bge-large-zh）对'名称+项目特征+单位'做 top-k 召回；③ 召回歧义时 LLM 两两判断是否同一清单项。对齐率本身也是指标——两份标书若连非标措施项的拆分方式都完全一致，正是'同一单位编制'的结构性证据。
- **参考**：
  - Li et al., Deep Entity Matching with Pre-Trained Language Models (Ditto), PVLDB (2020)
  - Peeters & Bizer, Entity Matching using Large Language Models (2023/2024) https://arxiv.org/abs/2310.11244
  - Automatic Classification of Construction Work Codes in Bill of Quantities Based on Text Analysis, ASCE J. Construction Engineering and Management (2023) https://ascelibrary.org/doi/10.1061/JCEMD4.COENG-12730

## 12. 补漏调研：均值基准价类评标规则下的串标模式检测（Conley & Decarolis 2016，评标办法感知的报价筛查）

### Conley–Decarolis 成组协调检验（参与检验 + 留组外截尾均值报价检验）—— 均值基准价机制下的首选筛查（经典但仍是首选）

- **成熟度**：研究级偏工业级：发表于 AEJ: Microeconomics（顶刊），复现数据公开（openICPSR #114329），方法已被反垄断/司法调查场景引用；实现只需报价+投标人身份两列数据，工程化门槛低。
- **原理**：针对'报价最接近(截尾)均值者胜出'类机制的成组统计检验：报价检验计算剔除嫌疑组 g 全部报价后的截尾均值 A1^g，与从全体报价中随机抽同样数量报价重算的 A1^s 分布（≥1000 次模拟）比较取分位数，系统性偏向一侧即判定该组在'做形状'抬/压均值；参与检验计算组内全体成员同场投标频率 f^g，与同规模随机组的联合到场频率分布做置换检验；多标段用联合统计量 J^g（以共同到场为条件）聚合，≥10 个共同参与的标段即可获得很强的检验力。
- **为什么更好**：均值基准价机制下卡特尔最优策略是让部分成员投极端'support bids'（陪衬报价）把基准移向自家真实报价（论文命题3/4：把组内报价聚在分布同一侧、辅以极端报价是占优操纵方式），此时报价方差反而变大——Imhof 类 CV/方差筛查（为第一价格拍卖设计，串标时 CV 必然下降）会失效甚至反向误导。该检验在都灵法院 2008 年已判案（1999–2002 年 276 场均值拍卖、8 个卡特尔约 95 家企业）上验证：报价检验识别出 8 个卡特尔中的 7 个，唯一漏检的正是法院因'仅偶发协调'而轻判的那组；参与检验对全部 8 个卡特尔拒绝独立入场。旁证：2024 年 GNN 跨国基准中，同一套通用筛查特征在意大利均值拍卖数据上表现最差（GNN F1≈0.57 vs 瑞士 Ticino 第一价格数据 F1≈0.99），作者明确归因于 ABA 机制不同——证明'第一价格那套筛查搬到均值机制会崩'。
- **标书场景用法**：中国综合评分法价格分普遍以'有效投标均价（或去极值加权均价）×随机/下浮系数'为基准价，与意大利 ABA 结构同构，检验可直接移植：(1) 用 BidGuard 已有的跨文件雷同证据（文档指纹、同源元数据、清单锁号、股权/联系人关联）生成候选组——这正是 C&D 所说'第一种候选组构造法'；(2) 对每个标段按招标文件实际公式重算'剔除候选组报价后的基准价'，与随机同规模剔除的基准价分布比较取分位数；(3) 跨同一业主/区域的历史标段聚合成 J^g 联合检验；(4) 仅有单项目 2–5 份标书时退化为单场检验+极端 support bid 形态标记（报价与次邻报价断崖式跳变且处于分布顶/底端）。
- **参考**：
  - Conley & Decarolis, Detecting Bidders Groups in Collusive Auctions, AEJ: Microeconomics 8(2):1–38, 2016, https://www.aeaweb.org/articles?id=10.1257%2Fmic.20130254
  - 工作论文全文（含检验公式与都灵验证细节）: https://capcp.la.psu.edu/wp-content/uploads/sites/11/conferences-and-seminars/2011/decarolispaper.pdf
  - 复现数据: https://www.openicpsr.org/openicpsr/project/114329/version/V1/view

### 评标办法感知的筛查分流（mechanism-aware screen routing）—— 报价筛查子方案的架构性改动

- **成熟度**：工程设计原则（各分支方法分别为研究级/准工业级，均有同行评审证据支撑）；解析中国评标办法前附表提取价格分公式属常规 NLP/规则工程。
- **原理**：先解析每个项目的评标办法（最低评标价法 vs 综合评分法/基准价评分，及基准价公式参数：是否去极值、二次平均、随机下浮系数、偏离扣分是否对称），再按机制路由到匹配的统计筛查族：最低价/第一价格 → 方差类筛查（CV、SPD、DIFFP、峰度、KS）+ missing-bids 检验；均值基准价 → C&D 留组外基准偏移检验 + support-bid 形态标记；切勿混用。
- **为什么更好**：有量化证据说明不分流的代价：瑞士第一价格数据上 CV 等筛查+ML 可达约 84–95% 正确率（Huber & Imhof 系列），而同一套筛查特征在意大利均值拍卖上 F1 只有约 0.55–0.57（2024 GNN 论文表格），且理论上串标方向相反（第一价格压缩报价→CV 降低；均值机制发散报价→CV 升高），同一阈值会把均值机制下的串标当成'更竞争'。分流是把两支各自验证过的方法用回其成立的机制假设内。
- **标书场景用法**：BidGuard 第 4 维'报价筛查'改为两级：第一级从招标文件抽取评标办法与基准价公式（综合评分法项目占中国货物/服务类招标绝大多数，均值基准价极常见）；第二级按公式选统计量。综合评分法分支还需把随机系数当作分布而非常数——对系数抽样区间做全量模拟，计算候选组在'任意系数取值下'仍最接近基准价的概率，概率异常高即强信号。
- **参考**：
  - Huber & Imhof 系列与筛查指标定义: Wallimann, Imhof & Huber, A Machine Learning Approach for Flagging Incomplete Bid-Rigging Cartels, Computational Economics 2023, https://link.springer.com/article/10.1007/s10614-022-10315-w
  - 跨机制表现差异证据: Collusion Detection with Graph Neural Networks, arXiv:2410.07091 (2024), https://arxiv.org/html/2410.07091v1
  - 中国背景下均值类评标的指标研究: Detecting the collusive bidding behavior in below average bid auction (2018), https://www.researchgate.net/publication/326427927

### 反事实基准价重算 / 评分公式精确模拟筛查（C&D 思想在中国综合评分法下的工程化）

- **成熟度**：工程可落地（算法本身是确定性重算+蒙特卡洛，无训练数据需求）；作为检测手段属研究级创新组合，但每个组件都有已验证出处。
- **原理**：在系统内实现招标文件声明的确切价格分公式（去极值规则、二次平均、随机系数区间、上下偏离扣分斜率），对每份/每组投标做 leave-one-out 与 leave-group-out 重算：量化'某组报价的存在把基准价移动了多少、把哪家的价格分抬高了多少、在多大比例的随机系数取值下改变中标人'，输出可解释的反事实数值而非黑盒分数。
- **为什么更好**：通用矩统计（均值、CV、偏度）丢失了公式细节，而卡特尔的最优'做形状'策略恰恰是针对具体公式定制的（Hendricks & Porter 1989 起的共识：串标形态由拍卖规则决定，C&D 论文以此为方法论出发点）；精确模拟把检验统计量对准了操纵目标本身，单场（2–5 份标书）即可给出'去掉这 2 家陪标报价后中标人改变'这种法务上可直接引用的证据，比需要长历史序列的筛查更适合项目级查重产品。中国防串标实务中'去极值+随机系数+二次平均'等公式加固措施的存在本身即证明监管方认可该操纵路径的真实性。
- **标书场景用法**：BidGuard 单项目场景的主打报价筛查：输入 = 解析出的评标公式 + 各家报价（含分项报价）+ 文档雷同/关联证据生成的候选组；输出 = 反事实基准价偏移量、中标人翻转概率（对随机系数积分）、support-bid 形态标记；分项报价还可加'不同家报价单错位同构'（同一成本底稿乘不同系数）检测，与总价筛查互补。
- **参考**：
  - Conley & Decarolis 2016（leave-group-out 截尾均值统计量的原始出处）, https://www.aeaweb.org/articles?id=10.1257%2Fmic.20130254
  - 中国评标基准价公式实务（随机系数、低价优先法等）: 深圳市财政局综合评分法说明, http://szfb.sz.gov.cn/zxbs/ywgz_3_1/sfjdl_3/content/post_10214112.html
  - 价格得分计算方法综述: http://www.ztbcgpx.com/news/dynamics/424.html

### Missing bids / 孤立中标 robust screens（Chassang–Kawai–Nakabayashi–Ortner）—— 最低评标价法分支的 SOTA

- **成熟度**：研究级（Econometrica 2022 / JPE 2022 / ReStud），已进入竞争执法机构方法库（NBER 2025 综述将其列为程序性筛查三大族之一）。
- **原理**：检验中标价附近'紧邻败标报价缺失质量'（winning bids isolated）：竞争行为在任意信息结构下都不应产生系统性的'中标价旁边空一段'形态，据此构造对未观测异质性稳健的非竞争性检验；姊妹方法用重新报价/二次开标中'最低价者身份 95% 以上保持不变'及险胜者-险败者特征断点（RDD）识别预定中标人轮换。
- **为什么更好**：不需要成本估计、不需要卡特尔标签、对信息结构不做假设（传统结构计量检验最脆弱处），属'安全港'式稳健检验；在日本 2003–2006 全国工程数据上标记约 1000 家企业、约 40% 项目（约 190 亿美元）行为与竞争不相容，是迄今规模最大的实证串标检出；俄亥俄学校牛奶案中断点法精确复现已知卡特尔地理范围。
- **标书场景用法**：仅适用于最低价类评标（经评审的最低投标价法、以最低价为基准价的低价优先法）——这正是'按评标办法分流'的另一分支：BidGuard 在识别为最低价法的项目上检查最低报价与次低报价间距是否异常孤立、以及同一采购人历史标段中'险胜者恒为在位者'的断点形态；均值基准价项目禁用该族。
- **参考**：
  - Chassang, Kawai, Nakabayashi & Ortner, Robust Screens for Noncompetitive Bidding in Procurement Auctions, Econometrica 90(1):315–346, 2022, https://www.econometricsociety.org/publications/econometrica/2022/01/01/robust-screens-noncompetitive-bidding-procurement-auctions
  - Kawai & Nakabayashi, Detecting Large-Scale Collusion in Procurement Auctions, JPE 130(5), 2022, https://www.journals.uchicago.edu/doi/abs/10.1086/718913
  - NBER Reporter 2025(2) 综述 Collusion in Public Procurement, https://www.nber.org/reporter/2025number2/reporter/collusion-public-procurement

### Coalition-based screens + 集成机器学习（Wallimann–Imhof–Huber）—— 组级筛查特征 + 超学习器

- **成熟度**：研究级（作者 Imhof 供职瑞士竞争委员会，方法与 COMCO 筛查实务同源），有跨国复验；对均值机制的覆盖弱于 C&D 专用检验，宜作候选组生成/粗排层。
- **原理**：对每个标段枚举投标人联盟（coalition），在联盟层面（而非整场）计算方差/均匀性等筛查统计量，再用 lasso/SVM/随机森林/super learner 分类联盟是否为串标团伙；跨瑞士、日本、意大利多种拍卖格式训练与检验。
- **为什么更好**：把检测单位从'整场拍卖'降到'投标人子集'，能识别不完全卡特尔（场内混有清白企业时整场统计量被稀释的问题），论文报告约 90% 的串标/竞争联盟正确分类率，且发现联盟内方差与均匀性指标是跨国家、跨拍卖格式最稳定的预测特征；只需报价+身份数据即可运行。
- **标书场景用法**：在 BidGuard 中作为'候选组生成器'：历史库模式下枚举同场共现的投标人子集，用联盟级方差/均匀性特征粗排出可疑组，再交给 C&D 反事实基准价检验做机制精确复核；单项目模式下 2–5 家标书的全部子集可穷举，计算量可忽略。
- **参考**：
  - Wallimann, Imhof & Huber, Detecting bid-rigging coalitions in different countries and auction formats, arXiv:2105.00337 (2021), https://arxiv.org/abs/2105.00337
  - Wallimann, Imhof & Huber, A Machine Learning Approach for Flagging Incomplete Bid-Rigging Cartels, Computational Economics, 2023, https://link.springer.com/article/10.1007/s10614-022-10315-w

### 图神经网络串标检测（bid-graph GNN，2024）—— 历史大库场景的候选排序器

- **成熟度**：研究级/竞赛级：无监管机构落地记录，需已标注历史数据训练，OOD 泛化差。
- **原理**：以单笔报价为节点，按同标段/同企业相邻报价/同地域/同企业驻地四类关系连边，节点特征含报价值与 7 项经典筛查统计量（CV、SPD、DIFFP、KURT、SKEW、KSTEST 等），GNN 端到端分类串标报价；并测试跨国 zero-shot/迁移。
- **为什么更好**：在全部 6 个数据集上 GNN 显著优于同特征 MLP（日本 F1 0.40→0.70、巴西 0.71→0.81、瑞士 Ticino 0.91→0.99、美国 0.21→0.39），证明'串标是网络结构现象、图结构信息有真实增益'；但跨国迁移性能大幅退化，且在意大利均值拍卖上最弱（F1 0.57）——同时给出了该路线的适用边界。
- **标书场景用法**：仅当 BidGuard 积累了带裁决标签的本地历史库（如公开处罚名单回填）后，作为大规模历史扫描的候选排序层；图 schema（企业-标段-报价-地域）与 BidGuard 的关联证据图天然兼容；不应作为单项目定性证据，且在均值基准价项目上其输出权重应下调（论文自证该机制下表现最差）。
- **参考**：
  - Collusion Detection with Graph Neural Networks, arXiv:2410.07091 (2024), https://arxiv.org/abs/2410.07091（详表见 https://arxiv.org/html/2410.07091v1）

### 中国工业级'主体+行为'围串标大数据预警（公共资源交易平台实践 + 2026 八部门 AI 实施意见）

- **成熟度**：工业级（多省平台在运行，国家政策强制推广时间表明确）；但公开渠道无严格的准确率基准，指标阈值多为经验值。
- **原理**：部署在省市公共资源交易平台的预警体系：同 IP/MAC 上传投标文件、工程量清单'锁号'/文件指纹一致、投标文件制作机器码相同、股权-社保-联系人穿透式关联、'陪标专业户'（长期投标中标率异常低）与'标王'（中标率异常高）主体画像、技术方案语义相似度与商务标报价特征比对；2026 年 2 月八部门《关于加快招标投标领域人工智能推广应用的实施意见》把'围串标识别'列入 20 个重点应用场景，要求 2026 年底在部分省市全覆盖、2027 年底全国推广（雄安已上线 11 项围串标预警指标）。
- **为什么更好**：这是中国当下唯一成建制落地、经执法实战检验的工业级方案，其证据维度（身份/行为/文档指纹）与价格筛查正交——学界共识是价格筛查给'统计嫌疑'、身份行为证据给'定性锚点'，两者组合的查实率远高于任一单独使用；且这些信号正是 C&D 检验所需'候选组'的最佳构造来源（对应其论文中股权/地址/联合投标构组法）。
- **标书场景用法**：BidGuard 的文档层能力（跨标书雷同、元数据同源、作者痕迹）即该体系的核心子集，可直接产品化为'候选组生成 + 定性证据'层；再叠加本清单第 1/3 条的机制感知报价检验，形成'文档证据定组、反事实基准价定量'的完整围串标模块，与监管侧能力形成差异化（BidGuard 面向单项目离线比对，无需平台级 IP/MAC 数据也能跑通文档+报价双通道）。
- **参考**：
  - 八部门实施意见报道（2026-02-11，围串标识别列入 20 场景、2026 年底部分省市全覆盖）: https://www.yicai.com/news/103049450.html
  - 商务部转载: https://tradeinservices.mofcom.gov.cn/article/news/gnxw/202602/181743.html
  - 20 场景清单与文号（发改法规〔2026〕195号，需核实）: https://www.smartcity.team/solutions/llms/ai_bidding/
  - 地方大数据预警实践（三门峡公共资源交易中心）: https://www.smx.gov.cn/4036/616940640/1864087.html

### 均值机制→第一价格机制切换的市场层面证据（Decarolis，检测结论的解释与校准用）

- **成熟度**：研究级（IER 2018 发表），作为领域知识/先验使用，不是可执行算法。
- **原理**：意大利 2006 年起被欧盟要求逐步用第一价格拍卖替换均值拍卖后：场均投标人数从 57 骤降到 7（99 分位从 300 降到 20），中标折扣率从约 13% 升到约 30%，数百家'壳/陪标'企业退出市场；都灵切换的因果估计为中标折扣提高 6–12 个百分点。
- **为什么更好**：为'均值基准价机制本身催生成组报价'提供了机制层面的因果证据（不是个案）：均值机制下超高参与人数+报价高度聚集是操纵均值竞赛的均衡特征，而非充分竞争的表现——这直接校准了检测器的先验：在均值基准价项目里，'参与家数异常多+报价异常集中于窄带+少数极端报价'应升权为串标信号，而不是按第一价格直觉解读为竞争激烈。
- **标书场景用法**：写入 BidGuard 报价筛查的解释与评分逻辑：均值基准价项目中对'窄带聚集+断崖式极端报价并存'的形态给出专门文案（'与均值操纵均衡一致'），并把该形态与文档雷同证据做联合置信度提升。
- **参考**：
  - Decarolis, Comparing Public Procurement Auctions, International Economic Review, 2018, https://onlinelibrary.wiley.com/doi/abs/10.1111/iere.12274
  - 工作论文版: https://www.eief.it/files/2017/03/decarolis_ier_2017.pdf
  - Decarolis, When the Highest Bidder Loses the Auction, https://papers.ssrn.com/sol3/papers.cfm?abstract_id=1523216

## 13. 补漏调研：对抗性规避与预处理鲁棒性（威胁模型层：不可见字符攻击、PDF 文字层-渲染不一致、文字转图片）

### Bad Characters 不可见 NLP 扰动攻击（威胁模型基线）

- **成熟度**：研究级(攻击原理)/但已被工业系统验证有效，攻击工具开源(github.com/nickboucher/imperceptible)，实战门槛极低
- **原理**：用 Unicode 的四类不可见编码——零宽字符/隐形字符、同形字(homoglyph)、双向控制符重排(reordering)、删除控制符(deletion/退格)——在视觉零改动的前提下改写送入模型的底层字节序列。
- **为什么更好**：这不是'更好的做法'而是必须先假定存在的攻击面：论文实测在黑盒下仅注入 1 个不可见扰动即可显著降低模型性能，注入约 3 个即可让包括查重/分类/翻译在内的多数商用系统功能性失效，且对 Microsoft、Google、Facebook、IBM、HuggingFace 已部署系统均有效。串标者只需在雷同段落里插入零宽字符/同形字，纯文本 n-gram、SimHash、embedding 相似度全部读到不同字节而给出'不雷同'的假阴性——所有 7 维下游算法被无声绕过。
- **标书场景用法**：作为整条管线的入口威胁模型。必须在抽取文本进入任何相似度算法之前，先做归一化清洗；同时把'检测到刻意的不可见扰动'作为独立信号——正常投标文件不会出现零宽字符/双向覆盖符，一旦出现且集中在雷同段落，本身就是极强的串通/规避证据，应触发人工复核而非仅仅清洗后放行。
- **参考**：
  - Boucher, Shumailov, Anderson, Papernot, 'Bad Characters: Imperceptible NLP Attacks', IEEE S&P 2022
  - arXiv:2106.09898
  - https://github.com/nickboucher/imperceptible

### Unicode 安全归一化：NFKC + 显式不可见字符剥离（防线第一层）

- **成熟度**：工业级：ICU、各大厂 AI 输入网关、Rust/GitHub 供应链防护均采用此模式；标准 Unicode Normalizer 库随处可得
- **原理**：对抽取文本先做 NFKC 兼容性归一化（把全角/上标/兼容变体折叠到规范形），再用码点白名单/黑名单显式剥离 NFKC 不会处理的隐形码点。
- **为什么更好**：关键工程陷阱：NFKC/NFC 本身并不删除零宽和双向控制符，很多实现误以为'归一化=清洗'从而留下缺口。正确做法是 NFKC 之后显式过滤零宽空格/连接符(U+200B–U+200D、U+FEFF)、双向覆盖符(U+202A–U+202E、U+2066–U+2069)、Tags 块(U+E0000–U+E007F)、变体选择符等；注意 U+200D 在 emoji 场景需保留（标书场景可直接删）。相比不做归一化，这一步把上一条攻击里的零宽/同形/退格类扰动在进入相似度计算前直接抹平，恢复被攻击破坏的字节一致性。
- **标书场景用法**：作为所有比对算法前置的强制预处理。对每份标书文本落地两份：清洗后用于相似度计算，原始字节用于取证。清洗时统计被剥离的可疑码点数量与分布，作为规避风险评分输入下游。
- **参考**：
  - Unicode UAX #15 Normalization Forms
  - CSA Research: 'Hidden Unicode Instruction Injection'
  - https://arxiv.org/abs/2508.14070 'Special-Character Adversarial Attacks on Open-Source LLMs' 2025

### UTS #39 confusables skeleton + 混合脚本检测（同形字防线）

- **成熟度**：工业级：ICU4C/ICU4J SpoofChecker 生产可用，Chrome/浏览器长期依赖
- **原理**：用 Unicode 官方 confusables.txt 把每个字符映射到视觉'骨架(skeleton)'，两串在 skeleton 相等即判为视觉同形；配合 mixed-script 检测识别单词内混入他脚本字符。
- **为什么更好**：定义 skeleton(X)=skeleton(Y) 即视觉混淆，能把 Cyrillic 'а'/Greek 'ο'/拉丁 'a/o' 这类跨脚本同形字统一到规范形——这是纯 NFKC 抓不到的（NFKC 不折叠跨脚本同形字）。注意实现细节：UTS#39 skeleton 用的是 NFD 而非 NFKC，二者需叠加使用而非二选一。ICU SpoofChecker 是权威实现，Chrome 用它防 IDN 同形域名。在标书里，把关键实体（公司名、法人、项目名、金额数字）先过 skeleton 归一，可击穿'用西里尔 о 替换中文数字旁拉丁 o / 用同形标点分词'这类精细规避。
- **标书场景用法**：对实体字段和西文/数字片段做 skeleton 归一后再比对；对全文做 mixed-script 异常扫描——正常中文标书不应在同一词内出现拉丁+西里尔混排，出现即标记为规避红旗。skeleton 命中同形替换本身作为串标证据。
- **参考**：
  - Unicode UTS #39 'Unicode Security Mechanisms' (skeleton 算法, confusables.txt)
  - ICU SpoofChecker API (getSkeleton, Highly Restrictive)
  - https://www.unicode.org/reports/tr39/

### PDF 字体 ToUnicode 重映射 / 内嵌字体内容掩蔽（PDF Mirage）

- **成熟度**：研究级(2017)但攻击原理经典且长期有效，PoC 开源；对策(OCR)工业界已知但常因成本被省略
- **原理**：在 PDF 内嵌自定义字体，让'字符编码→显示字形'的映射与 ToUnicode 表故意错位：屏幕渲染是 A 内容，文本抽取拿到的是完全不同的 B 内容。
- **为什么更好**：PDF 标准对内嵌字体的字形与文本串对应关系不做任何完整性校验，攻击者可任意重映射。论文明确演示可欺骗学术查重系统与 Bing/Yahoo/DuckDuckGo 索引——即渲染给评标人看的是投标正文，抽取给查重系统的是无关文字，雷同段落于是'查不出来'。这是比 Unicode 扰动更彻底的规避：不改一个可见像素就让抽取文本 100% 失真。作者指出很多系统为省算力'不做 OCR'正是被利用的前提。
- **标书场景用法**：必须假设 PDF 文字层不可信。对每份 PDF 做'渲染后 OCR ↔ 文字层抽取'交叉验证：两者字符级差异超阈值即判定字体重映射/内容掩蔽，触发以 OCR 结果为准并报警。ToUnicode 表缺失或异常、内嵌子集字体覆盖异常也作为特征。
- **参考**：
  - Markwood, Shen, Liu, Lu, 'PDF Mirage: Content Masking Attack Against Information-Based Online Services', USENIX Security 2017
  - https://www.usenix.org/system/files/conference/usenixsecurity17/sec17-markwood.pdf

### PDFuzz：字符定位打乱抽取顺序（Complete Evasion, Zero Modification, 2025）

- **成熟度**：研究级(2025-08，最新)，单检测器验证但机理通用
- **原理**：不改文本内容、只操纵 PDF 内字符的定位坐标，使人眼看到的排版正常，但文本抽取工具按内部顺序读出的却是被打乱/乱序的序列。
- **为什么更好**：证明'即便抽取到的字符集合正确，其顺序也可被攻破'——比 PDF Mirage 更隐蔽（连字体都不用改）。实测对 AI 文本检测器 ArguGPT：准确率从 93.6%±1.4 掉到 50.4%±3.2（等同随机），F1 从 0.938±0.014 掉到 0.0，视觉保真度完美。对标书查重意味着：串标者把雷同段落的字符坐标重排，抽取顺序被打乱，n-gram/序列对齐类比对直接失效。这条把'render-then-OCR 而非信任抽取顺序'从可选项变成必需项。
- **标书场景用法**：在交叉验证层，不仅比对 OCR 与文字层的字符集合，还要比对阅读顺序：用渲染坐标重建行/段的自然阅读序，与原始抽取序做序列对齐，错位率高即判定坐标重排规避。相似度计算应基于'按渲染坐标重排后'的文本而非原始抽取流。
- **参考**：
  - Aldan Creo, 'Complete Evasion, Zero Modification: PDF Attacks on AI Text Detection', arXiv:2508.01887, 2025-08
  - https://arxiv.org/abs/2508.01887

### 渲染后 OCR × 文字层交叉验证 + 感知哈希（防线核心架构 & 图片化正文检测）

- **成熟度**：工业级：国内清标系统(如广联达)已用图像 OCR + AI 识别做图片/图中文字查重；感知哈希+OCR 文档防伪有 IEEE 文献支撑
- **原理**：把每页 PDF 光栅化，对渲染图跑 OCR，得到'所见即所得'的文本，与 PDF 文字层/直接抽取文本做字符级 + 阅读序双重比对；对纯图片区域用感知哈希跨文档比对。
- **为什么更好**：这是同时封堵 PDF Mirage(字体重映射)、PDFuzz(坐标乱序)、隐藏文字层、以及'整段转成图片'的统一防线。三种典型规避的检出逻辑：(1)文字层与 OCR 严重不一致→字体/坐标类掩蔽；(2)渲染有可见文字但文字层为空/极少→图片化正文，改走 OCR 文本参与查重；(3)PDF 内容流里 text render mode=3(不可见)、白底白字、坐标出画布→隐藏文字层注入。对'图片化雷同段落'，感知哈希(pHash/文本图像形状哈希)能在不依赖 OCR 的情况下跨标书发现同一张被粘贴的图片。相比只信抽取文本，交叉验证把所有'渲染≠抽取'类攻击变成可检测的强信号。
- **标书场景用法**：作为入口管线的强制环节。产出三类告警：抽取-OCR 文本不一致、图片化正文、隐藏文字层。任一命中即'检测到刻意规避'，既以 OCR 文本喂给下游 7 维算法，又把规避事实本身作为串标证据单列。跨文档感知哈希用于抓被转成图片的雷同段落/公章/表格。
- **参考**：
  - 'An Authentic and Secure Printed Document ... Combining Perceptual Hash and OCR', IEEE 2020
  - Perceptual Text Image Hashing Based on Shape Recognition
  - 广联达/清标系统图像OCR查重 产品文档 2024

### When Vision Fails：针对 OCR/ViT 的组合变音符攻击（OCR 防线的反制，务必知悉）

- **成熟度**：研究级(2023，2025 ACM workshop 收录)，攻击开源(github.com/nickboucher/diacritics)
- **原理**：用 Unicode 组合变音符(combining diacritical marks)在渲染时叠加细微视觉扰动，使 OCR / 视觉 Transformer 误读，从而绕过'用 OCR 抵御 Unicode 攻击'的防御。
- **为什么更好**：直接反驳'OCR 是万能解药'的天真假设：论文证明本被寄望于忽略恶意 Unicode 的 OCR 视觉防线同样可被攻破。对本管线的意义是——render-then-OCR 交叉验证不能裸用，OCR 输入前也要对渲染文本做组合符归一化，且当 OCR 置信度异常/字符被大量组合符包裹时要单独告警，不能盲目信任 OCR 结果。
- **标书场景用法**：在交叉验证层加固：OCR 前先剥离/归一化组合变音符与变体选择符；对 OCR 低置信区域降权并提示人工核验；把'密集组合符'与'OCR 置信度骤降'作为规避特征而非噪声。
- **参考**：
  - Boucher, Blessing, Shumailov, Anderson, Papernot, 'When Vision Fails: Text Attacks Against ViT and OCR', 2023
  - arXiv:2306.07033
  - https://github.com/nickboucher/diacritics

### 双向控制符 / Trojan Source 检测（bidi 重排规避）

- **成熟度**：工业级：编译器/IDE/GitHub/ESLint 均已内置检测与告警，规则成熟
- **原理**：利用 Unicode 双向覆盖/隔离控制符(U+202A–U+202E, U+2066–U+2069)让逻辑字节顺序与人眼渲染顺序不一致，从而隐藏或重排内容。
- **为什么更好**：Trojan Source(CVE-2021-42574) 把这一手法从源码扩展为通用文本威胁。工业界给出的正是本场景可直接复用的检测范式：GitHub 对含 bidi 字符的文件加警示条，GCC 加 -Wbidi-chars，Rust 1.56.1 默认拒绝含此类字符的代码。相比'不管'，直接扫描 bidi 控制符的存在与配对合法性成本极低、假阳性极少——正常中文标书正文几乎不会出现双向覆盖符。
- **标书场景用法**：在归一化层顺带扫描 bidi 控制符：出现即从相似度文本中剥离(恢复逻辑序)，同时作为高置信规避红旗上报。可复用 ESLint/编译器同款黑名单码点集，无需自研模型。
- **参考**：
  - Boucher & Anderson, 'Trojan Source: Invisible Vulnerabilities', USENIX Security 2023 (CVE-2021-42574/42694)
  - GitHub bidi 警示 2021-10
  - GCC -Wbidi-chars / Rust 1.56.1 mitigation

### 中文特有的形近字/同音字/部首笔画替换归一化（中文洗稿式规避防线）

- **成熟度**：研究级(攻击, Journal of Software 2023)/工业级(防守：ChineseErrorCorrector、CSC 中文拼写纠错管线成熟，2026 ACL Oral)
- **原理**：针对中文，用形近字(视觉相似)、同音字(拼音相同)、部首/笔画微改来替换关键字，既躲雷同检测又不影响评标人阅读；防线是反向的中文纠错/归一化。
- **为什么更好**：英文的 homoglyph 主要靠跨脚本，中文规避更多靠'汉字内部'的形近/同音——UTS#39 与 NFKC 都覆盖不到。学界方法(如 CWordCheater、《基于汉语特征的中文对抗样本生成》)用 ConvAE 生成形近字候选池、用拼音相似与笔画/部首编辑距离建同音形近映射；防守方可复用同一套映射做归一化：把关键实体与正文按'拼音 + 笔画/部首骨架'折叠到规范字后再比对，从而识别'把「贰」写成「贰/貳」、把同音字互换'这类中文专属规避。相比只做字节级比对，能显著降低中文洗稿类假阴性。
- **标书场景用法**：对中文标书增设'形近/同音归一'通道：用中文纠错模型或拼音+笔画映射把关键字段折叠后比对；对雷同段落中出现的高频形近/同音替换密度做统计，异常高即判为刻意规避，作为串标特征。与前面的 Unicode 归一并行，覆盖中文/西文两类同形攻击。
- **参考**：
  - 《基于汉语特征的中文对抗样本生成方法》, 软件学报 2023 (jos.org.cn/6744)
  - CWordCheater (字音/字形/标点替换 + ConvAE 形近候选)
  - THUNLP OpenAttack 工具包
  - https://github.com/TW-NLP/ChineseErrorCorrector (2026 ACL Oral)

### 隐藏文字层 / 不可见渲染注入检测（PDF 内容流级取证）

- **成熟度**：研究级+工业可实现：PDF 结构解析(pdfium/PyMuPDF/pikepdf)成熟，判定规则明确
- **原理**：直接解析 PDF 内容流，检出被人为隐藏的文本：文本渲染模式 3(Tr 3=不可见)、与背景同色填充、定位到画布外、字号趋零、或 OCR 生成的 hidden 属性文字层。
- **为什么更好**：抽取工具通常'照单全收'隐藏文字层，攻击者据此注入两套内容：可见的一套给评标人、隐藏的一套污染或稀释查重结果（或反过来把雷同正文藏进不可见层规避比对）。相比只看抽取结果，在内容流层面检查渲染模式/颜色/坐标/字号能把'可见内容 vs 抽取内容'的差集精确定位到具体对象，是交叉验证告警的可解释证据来源。Purdue《Exploiting PDF Obfuscation in LLMs, arXiv, and More》系统梳理了这些混淆面对 LLM/arXiv/评审管线的影响。
- **标书场景用法**：在入口做 PDF 结构审计：标记所有 Tr=3、同色、出界、极小字号文本对象，量化'隐藏文本占比'；隐藏文本与可见/OCR 文本不一致即报警。既用于清洗(只保留真实可见内容参与查重)，也作为规避取证。
- **参考**：
  - Zhongtang Luo et al., 'Exploiting PDF Obfuscation in LLMs, arXiv, and More', Purdue (本地: tool-results/webfetch-1783259013813-rywja2.pdf)
  - PDF 32000 (ISO) Text Rendering Mode Tr
  - PyMuPDF/pikepdf 内容流解析

## 14. 补漏调研：合法共享内容的系统性剔除（招标文件对减 + 行业范本背景库 + k-共现过滤），需从'一句带过'升级为独立方法方向

### 招标文件对减比对（Base-file exclusion，MOSS/JPlag 范式移植）

- **成熟度**：工业级（经典但仍是首选；MOSS/JPlag 运行 20+ 年，国产清标工具已落地同类功能）
- **原理**：先把招标文件全套（各卷册、澄清答疑、附带范本表格）与投标文件在同一归一化 token 空间做 winnowing k-gram 指纹化，凡投标文件中与招标文件指纹匹配的片段一律剥离（视为对招标条款的合法逐字应答），只对剩余'自由文本'计算两两相似度。
- **为什么更好**：直接消灭标书场景最大单一误报源（对招标文件的逐字应答、法定格式文本），且几乎零漏报代价：winnowing 有形式化匹配保证（任何长度≥t 的共享片段至少留一个共同指纹，指纹密度≤2/(w+1)，压缩比约 0.03–0.1）；MOSS 自 2003 年、JPlag（--bc 参数）长期把 base-file 剔除作为核心功能，是代码查重界 20+ 年实战验证的首选做法；国内监管解读也明确要求评标时先人工核对'是否招标文件中有此类内容'，对减比对把这一步自动化。国产标书查重工具（如小鸽子标书对比王）已把'智能过滤招标文件'作为卖点，证明工程可行。
- **标书场景用法**：BidGuard 中作为相似度引擎前置的独立'引用剥离'阶段：解析招标文件（含补遗/答疑/范本附件）→ 文本归一化（去页眉页脚、空白、编号归一、OCR 纠偏）→ winnowing 指纹库 → 对每份投标文件标记并剥离匹配区间 → 交叉比对只在残差文本上进行；报告同时给出'原始相似度'与'剔除引用后相似度'两个数字（后者才用于风险分级），并把被剥离区间可视化供人工复核。
- **参考**：
  - Schleimer, Wilkerson, Aiken. Winnowing: Local Algorithms for Document Fingerprinting. SIGMOD 2003 (https://theory.stanford.edu/~aiken/publications/papers/sigmod03.pdf)
  - JPlag base code exclusion 文档 (https://github.com/jplag/JPlag/wiki/1.-How-to-Use-JPlag)
  - 小鸽子标书对比王'智能过滤招标文件'功能介绍 (https://zhuanlan.zhihu.com/p/1944699544601368096)

### k-共现过滤（MOSS -m / passim maxDF 式共享度阈值）

- **成熟度**：工业级（经典但仍是首选；MOSS/passim 的出厂功能，直接可移植）
- **原理**：统计每个段落/指纹在本项目全部 2–5 份投标文件（加上招标文件与背景库）中的共现家数：出现在超过阈值 N 家文件中的片段自动按'模板/行业惯用语'处理（等同 base file），恰好且仅 2 家共有的片段才是高可疑信号——共现计数本身就是最强判别特征。
- **为什么更好**：无需任何训练数据即可把'模板噪声'翻转成判别特征。这是被实战验证的成熟机制：MOSS 的 -m 参数（默认 10）规定'一个片段出现超过 N 份提交即视为 base file、永不报告'，-m 2 则'只报告恰好出现在两份程序中的片段'；winnowing 论文原文：'Fingerprints occurring in more than some threshold number of documents are ignored since they are likely to be common idioms or standard library code'；passim 用 --maxDF（默认 100）对 n-gram 做文档频率上限过滤以剔除公共模板。相比单纯两两相似度，它利用了'2-5 份交叉比对'这一场景独有的集合结构：≥3 家共有≈范本，仅 2 家共有≈串标。
- **标书场景用法**：在段落级指纹（shingle 哈希或 winnowing 指纹）上建共现计数表；两两相似度打分时按共现家数加权（如权重 ∝ 1/(共现家数-1)）而非硬剔除——因为围标团伙在 5 家中可能占 3+ 家，故'≥3 家共有'片段不直接豁免，而是先查招标文件/范本背景库：库中查得→豁免，查不得→整组标记为'多家异常一致'（本身即黔发改法规认定情形）；UI 上单列'仅两家共有段落'清单作为首要证据视图。
- **参考**：
  - MOSS 官方脚本 -m 参数文档：'the maximum number of times a given passage may appear before it is ignored'（默认 10；-m 2 只报恰好两份共有）(https://cs.uwaterloo.ca/twiki/view/ISG/MossBasics 及 moss.pl 脚本注释)
  - Winnowing SIGMOD 2003 论文中 MOSS 一节
  - passim --maxDF/--minDF 参数 (https://programminghistorian.org/en/lessons/detecting-text-reuse-with-passim)

### 行业范本/历史标书背景库 + 高频 n-gram 双阈值 boilerplate 过滤（Lang–Stice-Lawrence tetragram 度量法）

- **成熟度**：方法研究级但工程极简（计数+阈值）；同思想在 LLM 数据清洗管线中为工业级日常操作
- **原理**：建立背景语料库（九部委《标准施工招标文件》等行业范本、法定格式文本、证书/承诺函模板、历史投标文件），统计 4-gram（tetragram）在库内的文档频率：含'高 DF 但非极端 DF'n-gram 的句子判为范本套话并剔除/降权；极端高 DF（法定必备表述）与低 DF（个性内容）分别处理。
- **为什么更好**：给'什么算范本'一个可复现的统计定义，替代人工维护白名单。会计学界已在 42 国 15,000+ 家公司年报上大样本验证该度量：句子含'在本国≥60% 文档中出现'的 tetragram 即标记为 boilerplate，同时剔除'>80% 文档出现'的 tetragram（那是法定披露和无害语法短语）——这个双阈值设计恰好对应标书里'行业套话'与'法定格式文本'两层，可直接迁移；LLM 数据工程中的 line-level 高频去重（对跨文档重复行按频次阈值删除）是同思想的工业级日常实践。相比只对减招标文件，背景库还能拦住'招标文件里没有、但全行业都在抄'的范本内容。
- **标书场景用法**：冷启动用公开范本+可爬取的历史标书/中标公示构建背景库；对匹配片段按段落级 IDF 加权：得分贡献 ∝ log(N/df)，df 高的段落对总相似度几乎不贡献；随产品使用积累历史项目标书，背景库自增强；对中文用字/词双通道 n-gram（4-gram 字级对 OCR 噪声更稳）。
- **参考**：
  - Lang & Stice-Lawrence. Textual Analysis and International Financial Reporting: Large Sample Evidence. Journal of Accounting and Economics, 2015（BOILERPLATE 定义：≥60% DF 标记、>80% DF 剔除、按 boilerplate 句词数占比计分）
  - Detecting Text Reuse with Passim, Programming Historian 2021
  - 许雅思等. 基于大数据与智能语义识别的投标文件相似度比对软件系统设计研究. 2025（域停用词表 500+ 词、模板段落哈希去重）(https://www.anmaichuban.com/static/upload/file/20251202/1764634430370742.pdf)

### SemDeDup 式语义背景剔除（embedding 近邻判'洗稿范本'）+ 段落显著性加权

- **成熟度**：SemDeDup 本身工业级（LLM 数据管线广泛采用）；迁移到标书豁免场景属研究级组合创新
- **原理**：对字面对减漏掉的'改写过的范本'，用段落向量（bge-large-zh 等）在背景库 ANN 索引中查最近邻：若两份标书的雷同段落各自与背景库某范本段的余弦相似度都超过阈值（如 0.9），即判为'范本改写'豁免；反之，语义上既显著（背景库中无近邻）又仅两家共享的段落获得最高权重。
- **为什么更好**：字面 n-gram 对减对'洗稿'免疫力为零——把范本换几个词就绕过了。SemDeDup（Meta, 2023）证明预训练模型 embedding 能大规模找出'语义相同但字面不同'的重复：在 LAION/网络文本上可剔除约 50% 语义冗余而性能几乎无损，该技术已成为 LLM 数据管线标配。把同一机制反过来用在背景库上，就得到'合法雷同的语义豁免'，与字面对减互补，显著压低洗稿范本造成的误报。
- **标书场景用法**：BidGuard 已有本地 embedding（bge-large-zh），只需给背景库建一个 HNSW/faiss 索引；流程：交叉比对命中的段对 → 各自查背景库最近邻 → 双双命中高相似近邻则标记'疑似行业范本（改写）'并降权/豁免，同时把近邻范本原文附在证据里供人工确认；阈值用少量人工标注段对校准。
- **参考**：
  - Abbas et al. SemDeDup: Data-efficient learning at web-scale through semantic deduplication. arXiv:2303.09540, 2023

### 按章节类型分层阈值与分区比对（法定格式 / 技术方案 / 报价三通道）

- **成熟度**：工业级（监管规则明文分层；国产清标/稽核系统已按维度分别计分）
- **原理**：先用招标文件目录映射+标题模式（必要时加轻量分类器）把投标文件切分为法定格式文本、资格证明、技术方案（施工组织设计）、报价说明等区块，每类走不同的比对通道和阈值，而不是全文一个重复率。
- **为什么更好**：监管规则本身就是分层的，统一阈值注定两头失败：技术标阈值太松漏抄袭、法定格式阈值太紧全是噪声。实际规则示例——福建三明：技术文件内容雷同率超过 80% 属'投标文件异常一致'；青岛：投标清单报价达到 80% 相同视为雷同、但'与已标价工程量清单出现雷同的除外'；贵州十部门（黔发改法规〔2024〕296 号，2024-10-01 施行）：'出现错误内容异常一致'即涉嫌串通。分层输出可直接映射到评标委员会的法定认定条款，报告即证据；行业系统实测也按内容/格式/关键词多维度分别计分（安麦系统：ROC 曲线在 1000+ 真实围标案例上定阈，≥90 分高风险，召回 92%、精确 88%）。
- **标书场景用法**：技术方案区：字面+语义双通道、低阈值高敏；法定格式区：不比正文、只比'填空字段'与错误一致性（错别字、漏改处）；报价区：不做文本相似度，改做数值规律性筛查（等差/等比/规律性百分比——黔发改法规第(五)项情形）；已标价工程量清单区：默认豁免文字雷同（对应'除外'条款），只查计价软件锁号等元数据。每区独立阈值+独立证据链，最终按法条编号汇总。
- **参考**：
  - 省发展改革委等十部门关于工程建设项目电子投标文件雷同认定及处理的指导意见（黔发改法规〔2024〕296号，2024）(http://as.tianhecn.com/portal.php?mod=view&aid=775)
  - 三明市关于施工招标项目电子投标文件个性特征雷同认定与处理 (https://smggzy.sm.gov.cn/smwz/InfoDetail/?InfoID=2a6b0ecf-bac1-4940-a724-72d6e4cfd0c6)
  - 青岛市电子投标文件雷同认定规则（'投标清单报价达到80%相同（与已标价工程量清单出现雷同的除外）'）(https://ggzy.qingdao.gov.cn)
  - 许雅思等. 投标文件相似度比对软件系统设计研究. 2025

### 共同错误/低概率共享特征加权（identical rare errors，'一锤定音'证据）

- **成熟度**：监管认定情形属工业级需求；自动化提取（中文拼写/标点/引用校验）为研究级—工程实现
- **原理**：在剥离全部合法共享内容后，对'仅两家共有且背景概率趋近于零'的特征单独提权：相同错别字、相同错误标点、相同的条款号引用错误、相同的漏改处（如都忘了替换范本里的项目名），以及与招标文件笔误无关的一致性错误。
- **为什么更好**：单条'共同罕见错误'的证据力远超大面积重复率——这正是考试串通检测的经典统计结论（identical wrong answers 的判别功效远高于总体答案一致率，Angoff B-index/Wesolowsky 一脉），也是中国监管的明文认定情形：黔发改法规〔2024〕296 号第一条第(四)项把'出现错误内容异常一致'直接列为《招标投标法实施条例》第四十条'投标文件异常一致'的涉嫌情形。同时监管解读给出关键反向豁免：招标文件电子版本身有错、各家照抄导致的错误一致不能视为串标——所以每条共同错误必须先回查招标文件是否为错误源头。
- **标书场景用法**：残差文本上跑中文拼写检查、标点异常检测、条款/图号引用一致性校验；对每个候选错误：(1) 查招标文件——若源于招标文件则豁免；(2) 查背景库——若为行业常见笔误则降权；(3) 仅两家共有且两查皆空 → 置顶为'高置信串标证据'；报告中以'共同错误清单'形式呈现，直接对应评标专家的认定习惯。
- **参考**：
  - 黔发改法规〔2024〕296号 第一条第(四)项（'出现错误内容异常一致'）
  - 投标文件中的'异常一致'行为的认定 (https://zhuanlan.zhihu.com/p/623667549)
  - Wesolowsky. Detecting excessive similarity in answers on multiple choice exams. Journal of Applied Statistics, 2000（经典）

### LLM 终审仲裁层（合法共享 vs 实质抄袭的段对分类）

- **成熟度**：研究级/竞赛级（PAN、PlagBench 基准驱动），2024 起在工业查重产品中快速落地
- **原理**：对经过对减、k-共现、背景库过滤后仍存疑的 top-k 段对，用大模型做最后一道分类：该雷同能否被'招标文件应答/法定格式/行业惯用语'解释，输出类别（模板应答/范本改写/实质性抄袭/事实矛盾）+自然语言理由。
- **为什么更好**：规则与统计过滤处理不了长尾语义判断（如'两段都在描述同一国标施工工艺，措辞相近是否正常'）。PlagBench（2024，46.5K 段对基准，覆盖逐字/改写/摘要三类抄袭）显示 GPT-4 在抄袭识别上比商用查重工具高约 20%；PAN 2025/2026 生成式抄袭检测任务也表明段落级插入式抄袭用 embedding 相似度即可召回、难点在改写判别——LLM 恰好补这一段。作为末级精度过滤器只处理少量候选，成本可控，且理由文本可直接进评标报告。
- **标书场景用法**：prompt 中提供：段对原文+所在章节类型+招标文件对应条款检索结果+背景库最近邻范本；要求输出结构化 JSON（类别、置信度、依据）；仅对过滤后 top 50–200 段对调用（云端大模型或本地 Qwen 级模型均可）；LLM 判'模板'的段对进入人工抽检队列而非直接豁免，防止漏报。
- **参考**：
  - Lee et al. PlagBench: Exploring the Duality of Large Language Models in Plagiarism Generation and Detection. arXiv:2406.16288, 2024
  - Overview of PAN 2026: ... Generative Plagiarism Detection. arXiv:2602.09147
