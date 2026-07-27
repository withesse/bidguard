# BidGuard v2 升级执行方案（Execution Plan）

> 定稿日期：2026-07-06 · 基线：v0.5.0（migrations 已到 V11）
>
> 产出方法：6 个设计代理各负责一个工作流方向，先读取真实源码再产出文件级实现设计（32 个任务条目，文件/行号锚点经工程审查逐一核实基本全部命中）；随后 3 个审查代理分别从工程可行性、产品与合规、排期与依赖三个视角审查合并方案，提出 17 处问题（含 5 处 HIGH）。
>
> 配套文档：[bid-comparison-scheme.md](bid-comparison-scheme.md)（方案与 v2 路线）、[bid-comparison-sota-survey.md](bid-comparison-sota-survey.md)（SOTA 调研，方法依据）。
>
> **阅读提示**：第 1–2 章是吸收审查修正后的**定稿计划**；第 3–8 章是六个工作流的**原始设计明细**（全量保留，被修正处以第 1–2 章为准，例如原始迁移编号、被上调的工时、被条件化的 floor 规则）；第 9 章是三视角审查的完整记录。工时口径均为"熟悉本仓库的单人工程师净编码日"。

## 1. 全局裁决（开工前必须先定的五件事）

这五项是审查发现的主要返工源，任何条目动工前先落实：

1. **迁移台账一次性预分配**。六个工作流的原始设计全部抢占 V12 槽位。按里程碑顺序固定分配并写死进各条目（**2026-07-06 修订**：V12 已被注册激活 MVP 的 license_usage 表占用——commit 6e18dfc 先行合入，重编已发布迁移会损坏现有用户库，故整体顺延一位）：V13=documents.evasion_json（✅ M0 已落地）、V14=documents.doc_role（✅ M0 已落地）、V15=document_images 表（✅ M1 已落地）、V16=chunk_exemptions 表（✅ M4 已落地）、V17=clusters 豁免列（exempt_reason/multi_doc_anomaly）（✅ M4 已落地）、V18=verbatim_matches 表（✅ M5 已落地）、V19=aligned_segments（含 anchors）（✅ M5 已落地）、V20=segment_diffs 表（✅ M5 已落地）、V21=boq_items 表（✅ M6 已落地）、V22=jobs.numeric_json（✅ M6 已落地）、V23=clusters.rerank_score/confidence/band 合批（✅ M7 已落地）。删除一切"编号以合入时下一空位为准"的浮动表述。
2. **collusion 融合架构定终态**。M0 先做 `CollusionInputs` 单结构体重构（assess_with 签名只改这一次）；所有新信号从第一天起按**连续特征 x∈[0,1]** 定义（不是定值触发），M7 的逻辑回归对**全量特征**一次性拟合；FORENSIC_CAP/数值封顶/条件化 floor 作为 LR 之外的少数显式规则保留并文档化。避免原始设计中"四个工作流轮番改签名、LR 只拟合旧五信号"的互斥。
3. **options_hash 只 bump 一次（v5→v6）**。W2 归一化改动、W1 取证指纹版本、pdf_cross_check 配置键合并进同一次 bump。关键依据（工程审查 HIGH）：不 bump 时跨工作区缓存复用路径（import_service.rs persist_cached）会原样复制旧 fingerprint_json，重新导入也拿不到新取证字段；document_images 在缓存路径需同步重算/复制。
4. **matrix_json 与 ExportData 的 schema 在 M0 冻结**。剔除前/剔除后/区段三套口径一次定形：`{documentIds, matrix(剔除后·聚类口径，风险分级用它), matrixOriginal, segmentMatrix, peak/peakOriginal/segmentPeak, mode}`；六格式导出的 forensic/evasion/numeric/segments 节先定形状，后续里程碑按节填充，避免 docx 排版三次返工。Matrix.tsx 的双口径 UI（角标 + Pill 切换 + tooltip 对照）合并为一次改造，放在 M5 统一实现。
5. **指控性输出的产品纪律**（合规审查四条 HIGH 的落地）：
   - 硬命中 floor=medium **条件化**：仅当工作区已导入招标文件且豁免对减生效后启用；豁免不可用时硬命中只作信号展示，不设等级下限、不进验收断言；
   - "多家异常一致"**不自动 high**：强制"涉嫌"措辞 + "需评标委员会依法认定"脚注；招标文件为 OCR/扫描件或对减覆盖率抽样低于阈值时禁用升级，降级为中性提示；
   - 共形三带更名为**低优先级抽查 / 需人工复核 / 重点标红**，pass 带只做排序折叠不隐藏任何簇，禁用"自动放行/漏检保证"字样；α 相关文案强制限定"在合成校准语料上测得"；
   - 围标分数展示为**证据强度口头等级**（ENFSI 式：弱/中等/较强/强支持同源编制），概率数值仅保留在 JSON 技术字段；
   - 导出报告固定加**「检查方法与局限」**一节（无论是否命中）：列出已执行的取证/对抗检查项、各自可清除性说明、"未命中不构成清白证明"声明——堵住"沉默背书"。

## 2. 里程碑计划

**总量**：8 个里程碑约 90 净编码日（单人约 4.5–5 个月）。32 个原始条目经审查后砍 2 项、后置 3 项、新增 5 项胶水条目、上调 4 项工时。每个里程碑是可独立发布的切面，M3 之后每个里程碑落地时重写一次回归基线（基线 diff 随 PR 评审）。

| 里程碑 | 主题 | 净编码日 | 发布价值 |
|--------|------|---------|---------|
| M0 | 公共地基（破坏性变更一次吃掉） | ~8.5 | 归一化对抗加固上线 |
| M1 | 取证硬证据层 | ~10 | rsid / PDF 血缘 / 图片 / 共同错误四类新信号 |
| M2 | 入口对抗层 + 取证统一呈现 | ~12 | 规避检测 + 取证面板 + 报告章节 |
| M3 | 校准语料 + 回归门禁 | ~6.5 | 此后所有阈值改动有指标护栏 |
| M4 | 合法共享剥离层 | ~13.5 | 最大误报源消除，双口径矩阵 |
| M5 | 铁证层 + 对齐成型 | ~18（可拆 M5a/M5b） | "3.2 节 800 字逐字相同"级证据 |
| M6 | 商务标数值层 | ~12.5 | 清单雷同率 / 规律性差异 / 散点图 |
| M7 | 洗稿复核 + 融合校准 | ~9.5 | reranker 复核带 + LR 融合 + 三带 |

### M0 公共地基（~8.5d）

- **CollusionInputs 收敛重构**（1d，新增条目）：assess_with 收拢为单结构体入参，全部 collusion 单测一次性适配。
- **matrix_json / ExportData schema 冻结**（1.5d，新增条目）：只定形状不实现。
- **Unicode 安全归一化强化**（2d，W2-1）：NFKC 后显式剥离零宽/双向控制符/Tags/变体选择符，块级统计 + 文档级 evasion_json（V12）。
- **同形字防线**（1.5d，W2-2）：静态 confusables 映射表 + 同词内混合脚本红旗。
- **doc_role 文档角色**（2d，W3-1，V13）：招标文件/补遗角色贯穿导入→比对校验→CompareSetup 分组 UI。
- 同版完成 options_hash v5→v6（含 pdf_cross_check 预置键与取证指纹版本）。

### M1 取证硬证据层（~10d）

- **rsid 提取与交集**（1.5d，W1-1）：弱档要求 shared_count≥3（审查修正），rsidRoot 相同为强档；免责文案"另存为即可清除，未命中不代表清白"。
- **docx 深度元数据**（1d，W1-2）：Template 名/created 邻近/zip 条目序列指纹，只并入 metadata 信号不独立加权。
- **PDF 血缘取证**（2d，W1-3）：trailer /ID、XMP GUID、字体子集标签三级命中；国产工具 XMP 宽松解析。
- **图片同源 dHash**（3d，W1-4，V14）：sha256 精确 + 自实现 dHash；整页扫描图只做精确匹配。
- **共同错误指纹**（2.5d，W1-5）：词典外词/异常标点/引用错误三类，豁免接口留桩（M4 接线）。
- 硬命中 floor 规则休眠（条件化，M4 激活）。

### M2 入口对抗层 + 取证统一呈现（~12d）

- **PDF 隐藏文字层审计**（4d，W2-3 上调自 3d）：内容流状态机（Tr=3/白字/出画布/极小字号）+ "OCR 双层页"合法模式识别。
- **渲染-OCR 交叉验证**（3.5d，W2-4）：确定性抽样 5 页，Dice + 顺序分；**命中回落 OCR 时解除 20 页上限**（审查修正）并明示告知。
- **evasion 围标信号**（2d，W2-5）：判级改用浓度与聚集度；suspect 级不打 Library 徽标不挂告警条（审查修正）。
- **取证统一接入与呈现**（4d，W1-6 上调自 2d）：FORENSIC_CAP=0.45、Matrix 取证折叠区、HTML+JSON 导出章节（其余格式后置）、「检查方法与局限」常驻章节（+0.5d 已计入）。

### M3 校准语料 + 回归门禁（~6.5d，从计划末尾提前——排期审查 MEDIUM）

- **合成对抗语料生成器**（5d，W6-1 含 +1d 扩展）：六类文本变换 + 取证/规避/数值信号变换（母文件保留 rsid、零宽注入、共享图片、清单乘系数——审查发现原设计缺失，LR 融合会没料可拟）；splitmix64 确定性；种子脱敏 checklist。
- **回归门禁**（1.5d，W6-5）：CI 快档 <60s 对比 baseline_metrics.json，慢档本地含模型层；语料 hash 校验防基线不同步。

### M4 合法共享剥离层（~13.5d）

- **招标文件对减**（6d，W3-2 上调自 4d——全计划改动面最大单条）：winnowing（k=15/w=10）、全量/残差双边集双聚类、矩阵双数字（风险分级用剔除后口径）、V15 chunk_exemptions。
- **内置静态范本背景库**（1.5d，W3-4 降级版）：砍掉跨工作区增量 DF 记账（审查裁决：破坏可复现可举证），保留随包版本化静态库 + 双阈值。
- **豁免接线**（1d，新增条目——审查发现的缺失桥接）：招标文件 rsid 集/图片哈希集/token 集喂回 M1 三处豁免参数；激活条件化 floor。
- **k-共现查证**（2.5d，W3-3，V16）：≥3 家共有先查证，查不到标"待复核·多家共有段落"（不自动 high）。
- **分区分层阈值**（2.5d，W3-5）：五区分类（legal/price/tech/business/other），legal 区阈值 +0.12。price 区**不上浮、不因套话文本计分**，但表格行**维持现阈值**以保留金额事实冲突通道（fact.rs 依赖 price 表格行聚类出金额冲突）——逐项数值雷同率/相关性留 M6 数值层，不在 M4 切断 price 文本聚类（否则会静默废掉已有的金额冲突检测并破两条基线测试）。

### M5 铁证层 + 对齐成型（~18d，可拆 M5a 铁证+链化 / M5b 视图+导出）

- **逐字雷同区间**（3d，W4-1，V17）：手写后缀自动机，≥30 汉字极大公共子串带 chunk 锚点。
- **seed-chain-align 链化**（3.5d，W4-2，V18）：minimap2 式稀疏 DP，verbatim 命中作满分锚点。
- **区段内带状细化**（2.5d，W4-3，V19）：句级带状 NW + char_diff。
- **覆盖率矩阵升级**（1.5d，W4-4）：区段口径 + 旧口径开关；围标信号①暂不切换（等 M7 回测）。
- **W3 桥接**（1d，新增条目——排期审查 HIGH：不做则"对招标条款的合法逐字应答"以铁证形态还魂）：verbatim/链化尊重 tender_coverage 豁免。
- **区段视图**（5d，W4-5 上调自 4d）：新屏 PairSegments 双栏高亮；区段是新增证据层不替代聚类，复核三态仍只挂聚类。
- **区段/逐字证据导出**（2d，新增条目）：HTML/DOCX 两主格式（"屏幕上有、报告里没有"违反报告即证据）。

### M6 商务标数值层（~12.5d）

- **清单识别与行对齐**（3d，W5-1，V20）：表头同义词典 + 编码对齐；扫描件 PDF 不覆盖（后置池）。
- **逐项雷同率 + 相同算术错误**（2d，W5-2，V21）：80% 告警线可配；**0.35 权重要求 ≥2 条独立算术错误**（审查修正：单条可能是同款计价软件舍入惯例）。
- **规律性差异**（1.5d，W5-3 砍 Benford 后）：等差/等比/恒定折扣 R²≥0.999 + 尾数聚集。
- **相关性 + 散点图**（3d，W5-4）：Pearson/Spearman + 纯 SVG 对角线散点图。
- **数值信号接入 + 导出**（3d，W5-6）：替换"报价梯度"（无 BOQ 保留回落），数值类封顶 0.45；mechanism_flip_prob 定义为 Option（W5-5 后置不阻塞闭环）。

### M7 洗稿复核 + 融合校准（~9.5d）

- **cross-encoder 复核带**（4d，W6-2）：默认档 int8 量化（~300MB，审查实测 fp32 延迟被低估 2–6 倍）、截断 256 token、只跑 uncertain 带、默认关；**自动改判降级为"复核建议"**（人工确认才改分类）；顺带补 rerank+embed 模型下载 sha256 校验（0.5d 已计入）。
- **LR 融合**（2.5d，W6-3）：对 M1–M6 全量特征拟合；score 展示为证据强度口头等级；权重文件随包固化，负权重人工审查拦截。
- **校准 + 三带**（3d，W6-4，V22）：Platt 起步 + split conformal；带命名与文案按第 1 章裁决 5。

### 后置池（二期）

| 条目 | 处置 | 理由 |
|------|------|------|
| 跨工作区增量 DF 背景库 | 砍（保留静态版） | 破坏"同输入同输出"可复现性；冷启动长期空转；跨项目计数外溢的保密观感 |
| Benford 首位卡方 | 砍 | 单价只跨 2–3 个数量级前提弱；2–5 份场景恒为噪声，易被对方律师攻击 |
| 机制感知反事实基准价（W5-5，4d） | 后置二期 | v1 只支持一族公式 + 评标办法人工录入风险 + 循环论证观感；flip_prob 接口留 Option |
| 中文形近/同音字归一 | 后置 | UTS#39 与 NFKC 均不覆盖，独立立项 |
| 扫描件表格结构识别（PP-StructureV3 类） | 后置 | 新 ONNX 模型与体积；先在产品上声明数值层支持范围（xlsx/docx/文本 PDF） |
| CSV/MD/XLSX 导出章节补齐 | 后置小项 | HTML/DOCX/JSON 主格式先行 |

### 已拍板决策（2026-07-06 与产品负责人逐项确认）

1. **pdf_cross_check 默认开启**：Settings 提供关闭开关，导入进度条注明耗时原因（每份文字版 PDF +5–10s）；
2. **reranker 采用 bge-reranker-base int8 量化档（~300MB 按需下载）+ 默认关闭**：在真实语料上验证精度后再考虑默认开；8GB 机型强制串行加载（rerank 前卸载 embedder）；
3. **语料种子从 BIDGUARD_CALIB_DIR 现有 8 份测试标书截取章节**：按脱敏 checklist（主体/项目名/金额/人名全替换而非遮盖）处理后入库 fixtures；
4. **清单雷同率默认告警线 0.80（可配置）**：帮助文案注明「参照地方雷同认定口径，针对逐项单价相同率」，避免越权定性；
5. **数值证据面板为独立屏**（BusinessNumeric.tsx），Matrix 页提供入口跳转，与「矩阵→聚类→详情」导航层级保持一致；
6. **内置公开范本随包进静态背景库**：九部委标准招标文件等政府发布的规范性公开文本，版权风险低、体积可忽略；随包版本化以保证豁免集合可复现。


## 3. 设计明细：W1 取证硬证据层

> 设计代理的工作流综述：W1 取证硬证据层：在现有 fingerprint_json → cross_flags → collusion 信号 → Matrix/导出 的链路上，新增 rsid、docx 深度元数据、PDF 血缘、图片同源、共同错误五类取证级信号及其统一融合与呈现。全部复用既有依赖（quick-xml/zip/lopdf/pdfium-render/image/jieba-rs/sha2），仅 1 个新表迁移（document_images），其余靠 schemaless JSON 字段 + serde default 向后兼容；合计约 12 个净编码日。

### OOXML rsid 提取与两两交集同源信号（1.5d）

- **价值**：rsid 是取证级硬证据（Joun 2021, J Forensic Sci；调研 §5 CONFIRMED）：两份 docx 共享任一 rsid（尤其 rsidRoot）即高度指示派生自同一母文件，改作者、换抬头、同义改写都洗不掉，与现有文本相似度信号完全正交。scheme §9.1 将其列为性价比最高的证据层升级第一项。
- **设计**：解析期：parse_docx（src-tauri/src/engine/parse.rs:600-647）已持有 ZipArchive，用现成 read_zip 读 word/settings.xml，新增 fn fill_rsids 以 quick-xml 提取 <w:rsids> 下全部 w:rsid 的 w:val 与 w:rsidRoot（去重、大写归一、上限 2048 个）。Fingerprint（engine/report.rs:7-16）新增 #[serde(default)] 字段 rsids: Vec<String> 与 rsid_root: Option<String>，随 fingerprint_json 落库（无 schema 变更，旧 JSON 反序列化靠 default 兼容）。比对期：engine/fingerprint.rs 新增 pub fn rsid_pairs(docs: &[DocInfo]) -> Vec<RsidHit>，两两求交集，产出 {a, b, shared_count, root_match} 并给命中文档追加 risk_flags；compare_service.rs run_compare 在 cross_flags（约 311 行）后调用，结果传入 collusion::assess_with。collusion.rs 新增信号 kind="rsid"：shared_count≥1 记 0.20，shared_count≥10 或 root_match 记 0.35（新增常量 RSID_WEIGHT/RSID_STRONG_WEIGHT，集中在现有权重常量区 14-32 行）。呈现纪律：只在命中时产生信号；detail 文案固定附注「未命中不代表清白：另存为新文件即可清除 rsid」；不输出任何『rsid 检查通过』类表述。
- **改动文件**：
  - `src-tauri/src/engine/parse.rs`：parse_docx 内 read_zip("word/settings.xml") + 新增 fill_rsids（quick-xml 提取 w:rsid/w:rsidRoot，去重限量）；新增手造 settings.xml 的 docx fixture 测试
  - `src-tauri/src/engine/report.rs`：Fingerprint 增 #[serde(default)] rsids: Vec<String>、rsid_root: Option<String>
  - `src-tauri/src/engine/fingerprint.rs`：新增 RsidHit 结构与 rsid_pairs()：两两交集、rsidRoot 相等判强命中、写 risk_flags
  - `src-tauri/src/engine/collusion.rs`：assess_with 增 rsid_hits 参数与 kind="rsid" 信号（0.20/0.35 两档权重常量）；补单测
  - `src-tauri/src/services/compare_service.rs`：run_compare 第 311 行 cross_flags 后调用 rsid_pairs 并传入 assess_with（约 337 行）；更新 1327 行起的管线测试
  - `src/engine.ts`：Fingerprint 接口增 rsids/rsidRoot 可选字段（供 Matrix 面板后续消费）
- **DB 改动**：无（rsid 存 documents.fingerprint_json，schemaless；旧行缺字段由 serde default 兜底）
- **UI 改动**：本条仅保证信号经现有 insights 通道展示（未知 kind 走 Matrix.tsx:126-135 默认分支）；专属面板与免责文案归「取证信号统一呈现」条目
- **配置**：无
- **新依赖**：无（quick-xml、zip、serde 均已有）
- **风险**：最大误报源：招标代理统一下发的投标文件格式模板（docx）会让各家标书天然共享模板的 rsid——W3 招标文件角色落地前，detail 文案须写明「同一母版可能为招标方提供的统一模板」，并预留 exempt_rsids: &HashSet<String> 参数（W3 接入后用招标文件的 rsid 集合做减法）；权重未经语料校准，0.20/0.35 为初始经验值（与文件头 ⚠️ 注释同一口径）；WPS 生成的 docx 可能无 rsids 节点（信号缺席而非报错）。
- **验收标准**：1) cargo test 新增用例通过：两份手造 docx 共享 3 个 rsid → collusion.signals 含 kind="rsid" 且 weight=0.20；rsidRoot 相同 → weight=0.35；无 settings.xml → 无该信号且 detail 不出现「清白/通过」字样；2) 旧 fingerprint_json（无 rsids 字段）反序列化不报错（回归测试）；3) rsid 命中的两文档 risk_flags 各含一条「rsid 交集」标记；4) detail 文本包含「另存为」免责语；5) cargo clippy 与现有 compare_service 管线测试（collusion_pipeline_on_generated_bids_v2）全绿。

### docx 深度元数据扩展：Template/编辑时长/时间邻近/zip 条目序列指纹（1d）

- **价值**：现有 docx 指纹只有作者/保存者/软件/修订号（parse.rs fill_core/fill_app），交叉规则只查作者与最后保存者两项（fingerprint.rs:12-19）。补 Template 名、创建时间邻近、zip 条目序列指纹后，「同一台机器同一批生成」的围标特征可被捕捉，且全部是解析期顺手提取、零新依赖。
- **设计**：提取：fill_app（parse.rs:901-926）补读 app.xml 的 <Template>（存 Fingerprint.template_name）；parse_docx 内以 zip.by_index(i) 按中央目录顺序遍历条目名，sha256(条目名以\n连接) 得 zip_entry_fp（同一生成工具/同一打包管线的稳定指纹，调研 §5 PDF 结构指纹的 docx 对应物），连同条目数存 Fingerprint（均 #[serde(default)]）。交叉：fingerprint.rs cross_flags 新增三条规则——template_name 相同且非 Normal/Normal.dotm；created 时间差 ≤10 分钟（解析 W3CDTF，常量 CREATED_PROXIMITY_MIN）；zip_entry_fp 完全相同。命中写 risk_flags，继续走现有 kind="metadata" 信号（META_WEIGHT=0.25，collusion.rs:24-25），不新增信号种类但把 detail 从笼统一句改为列出具体命中项。revision/total_edit_minutes 已提取，此条补交叉规则：revision 相同且 >1、或 TotalTime=0 但 revision 高（疑似元数据清洗）作弱标记（只进 risk_flags 不计权）。
- **改动文件**：
  - `src-tauri/src/engine/parse.rs`：fill_app 增 Template 分支；parse_docx 增 zip 条目序列指纹计算（by_index 顺序 + sha2，已有依赖）
  - `src-tauri/src/engine/report.rs`：Fingerprint 增 #[serde(default)] template_name/zip_entry_fp/zip_entry_count 字段
  - `src-tauri/src/engine/fingerprint.rs`：cross_flags 增 flag_shared(template_name)、created 邻近比较、flag_shared(zip_entry_fp)；revision/TotalTime 弱标记规则
  - `src-tauri/src/engine/collusion.rs`：metadata 信号 detail 改为枚举具体命中项（作者/模板/创建时间邻近/包结构一致）
  - `src/engine.ts`：Fingerprint 接口增 templateName/zipEntryFp 可选字段
- **DB 改动**：无（全部进 fingerprint_json）
- **UI 改动**：Matrix.tsx:430-436 参评标书卡的 FpChip 行增「模板」chip（templateName 非空时）；其余归统一呈现条目
- **配置**：无
- **新依赖**：无（sha2/zip/quick-xml 已有）
- **风险**：Template 大量文档就是 Normal.dotm（已排除）但地方定制模板名也可能来自招标方统一下发（同 rsid 的模板豁免问题）；zip 条目序列对「同一软件同版本」区分度有限（Word 同版本产物普遍一致），必须作为弱证据只并入 metadata 信号而非独立加权，否则推高误报；created 邻近对「拷贝模板再改」场景会漏（created 继承模板），阈值 10 分钟为拍板值待校准。
- **验收标准**：1) 单测：两份手造 docx Template 均为 "投标文件模板.dotx" → 双方 risk_flags 含「模板相同」，Template 为 Normal.dotm 时不打标；created 相差 5 分钟打标、相差 2 小时不打标；zip 条目序列一致打标；2) metadata 信号 detail 中能看到具体命中项列表（管线测试断言子串）；3) 旧 fingerprint_json 兼容回归通过；4) npm run build 与 cargo test 全绿。

### PDF 血缘取证：XMP GUID / trailer ID / 字体子集标签（2d）

- **价值**：GUID 碰撞概率趋近于零：DocumentID/DerivedFrom/trailer /ID 前半相同即「单点定案」级同源证据（调研 §5，eDiscovery 行业标准鉴定项），是 PDF 侧对 rsid 的对等物；BidGuard 大量输入是 PDF，当前 pdf_fingerprint（parse.rs:557-584）只读 Info 字典四个字段，证据密度远低于 docx。
- **设计**：扩展 pdf_fingerprint（三个 PDF 解析路径 parse.rs:403/468/510 均已调用，扩展自动全覆盖）：(1) trailer /ID——lopdf doc.trailer.get(b"ID") 取数组两半，hex 存 pdf_id_first/pdf_id_second（首半创建时生成、再保存不变，是血缘键）；(2) XMP——trailer Root → catalog /Metadata 流，取流字节用 quick-xml 宽松提取 xmpMM:DocumentID、InstanceID、DerivedFrom(stRef:documentID)、xmp:CreatorTool（XMP 通常未压缩，压缩流用 lopdf decompressed_content 兜底）；(3) 字体——doc.get_pages() 逐页 get_page_fonts()，收 BaseFont 名集合，正则 ^[A-Z]{6}\+ 抽子集前缀存 font_subset_tags（去重）。交叉（fingerprint.rs 新增 lineage_pairs()）分三级：DocumentID 相同 / DerivedFrom 指向同一 GUID / pdf_id_first 相同 → 硬命中「同一母文件」；共享 ≥1 个相同的字体子集标签（同 6 字母前缀+同 BaseFont）→ 中命中「同一次生成环境」；CreatorTool+Producer+字体全集一致且创建时间邻近 → 弱命中。collusion.rs 新增 kind="pdfLineage"：硬命中 0.35、仅中命中 0.20、仅弱命中并入 metadata。detail 同样固定附注「元数据可被抹除，未命中不代表清白」。
- **改动文件**：
  - `src-tauri/src/engine/parse.rs`：pdf_fingerprint 扩展：trailer /ID 提取、XMP 流定位与 quick-xml 解析（新增 fn fill_xmp）、逐页字体名与子集前缀收集；fixtures 增带 XMP/ID 的最小 PDF
  - `src-tauri/src/engine/report.rs`：Fingerprint 增 #[serde(default)] pdf_id_first/xmp_document_id/xmp_instance_id/xmp_derived_from/creator_tool/font_names/font_subset_tags
  - `src-tauri/src/engine/fingerprint.rs`：新增 LineageHit 与 lineage_pairs()：三级命中规则 + risk_flags
  - `src-tauri/src/engine/collusion.rs`：assess_with 增 lineage_hits 参数与 kind="pdfLineage" 信号（0.35/0.20 两档）；单测
  - `src-tauri/src/services/compare_service.rs`：run_compare 调用 lineage_pairs 并传入 assess_with
  - `src/engine.ts`：Fingerprint 接口增对应可选字段
- **DB 改动**：无（fingerprint_json 承载）
- **UI 改动**：无独立 UI（信号走现有 insights；专属呈现归统一呈现条目）
- **配置**：无
- **新依赖**：无（lopdf 0.34、quick-xml、regex 均已有；子集前缀可用手写匹配避免 regex）
- **风险**：lopdf 对损坏/加密 PDF load 失败时现有代码返回空指纹（559-562 行），扩展保持同语义即可但意味着最需要取证的「洗过的 PDF」可能整体无指纹——需在 UI 免责语中覆盖；XMP 是自由格式 XML，WPS/永中等国产工具的写法未经验证，需按「取不到就留空」宽松解析，禁止 panic；字体子集前缀部分生成器固定不随机（如某些版本 LibreOffice），中命中档存在系统性误报，故只给 0.20 且必须与文本信号叠加才可能到 high；同一打印店代做多家标书（真实违规但也常见「代打印」灰色地带）会硬命中，结论文案要把判定权留给评标人。
- **验收标准**：1) 单测：手造两份 trailer /ID 首半相同的 PDF → signals 含 kind="pdfLineage" weight=0.35，risk_flags 含「同一母文件」；仅共享字体子集标签 → weight=0.20；均无命中 → 无该信号；2) 加密/损坏 PDF 走 fixture 断言不 panic、指纹为空；3) detail 含「未命中不代表清白」子串；4) 现有 pdf 解析三路径测试回归全绿。

### 内嵌图片同源检测：docx media / PDF 图片对象提取 + dHash 碰撞（3d）

- **价值**：两份标书共用同一张施工现场照片、扫描版资质证书或公章图，是完全绕开文本比对的高证明力围标信号（调研 §5：SHA/感知哈希工业级，Meta PDQ 实战验证；扫描章、签字页是重点对象）。现有管线只对图片做 OCR（parse.rs:655-722），像素同源信息全部丢弃。
- **设计**：提取与哈希（导入期）：docx 复用 collect_docx_images 的遍历骨架但独立限额（MAX_IMAGE_HASHES=200，含被 OCR 跳过的小图之外的全部 ≥80px 位图）；PDF 用 pdfium-render 遍历页对象取 PdfPageObject::Image 的位图（BGRA→RGB 转换复用 rasterize_pdf 的现成代码），pdfium 不可用时该文档跳过图片信号（与 OCR 同降级语义）。每图计算：exact = sha256(宽+高+RGB8 像素字节)（跨容器格式稳定），near = 64 位 dHash 自实现（灰度→缩放 9×8→横向梯度取位，用已有 image crate，约 30 行）。落库：V12 迁移新增表 document_images(document_id REFERENCES documents ON DELETE CASCADE, idx, source, page, width, height, sha256, dhash)，import_service persist 阶段写入。比对期：compare_service 加载参评文档的 document_images，两两跨文档碰撞——sha256 相等为硬命中；汉明距离(dhash)≤10 且双方非整页扫描图（图面积/页面积>0.8 的只做 exact 不做 near，防扫描件整页图互撞）为近似命中。collusion.rs 新增 kind="imageReuse"：distinct 命中图 1 张 0.15、≥3 张 0.25 封顶；detail 列出「甲 p3 ↔ 丙 p5 同图」样例（天干+页码，最多 3 组）。
- **改动文件**：
  - `src-tauri/src/engine/parse.rs`：新增 fn collect_image_hashes_docx / collect_image_hashes_pdf 与 dhash64()；ParsedBlocks 增 image_hashes: Vec<ImageHash> 字段
  - `src-tauri/src/db/migrations.rs`：新增 V12 常量 DOCUMENT_IMAGES_V12（建表+idx_document_images_doc 索引），追加进 MIGRATIONS 数组（编号以合入时下一空位为准，当前为 V12）
  - `src-tauri/src/db/repo/document_repo.rs`：新增 insert_images/list_images_for_docs（或独立 image_repo.rs，跟随 repo/mod.rs 注册）
  - `src-tauri/src/services/import_service.rs`：persist 阶段（mark_parsed 附近，约 319-365 行）写 document_images
  - `src-tauri/src/services/compare_service.rs`：阶段 8 聚合处加载参评文档图片哈希、两两碰撞出 ImageHit 列表，传入 assess_with
  - `src-tauri/src/engine/collusion.rs`：kind="imageReuse" 信号（0.15/0.25 两档）+ 单测
- **DB 改动**：V12（编号按合入时顺延）：CREATE TABLE document_images，外键 ON DELETE CASCADE 级联清理，document_id 建索引（与 V10 的级联外键须有索引同理）；旧文档无行 → 信号自然缺席，向后兼容
- **UI 改动**：无独立 UI（detail 文本经 insights 展示；图片证据缩略图对照归后续迭代，本条先保证文字证据可用）
- **配置**：无（限额用常量；若导入耗时实测超预期再评估开关）
- **新依赖**：无（image、sha2、pdfium-render、zip 均已有；dHash 自实现约 30 行，不引入 img_hash 等新 crate）
- **风险**：最大风险是误报语义：招标文件里的效果图/项目区位图被招标方统一提供、各家贴进标书属合规雷同——需预留 exempt_hashes 参数（W3 招标文件的图片哈希做减法），落地前 detail 固定提示「请核对该图是否来自招标文件」；扫描件 PDF 每页即一张大图，near 匹配会把「都是空白页/都是同一制式表格」判成同源，整页图只做 exact 的规则是关键防线但阈值 0.8 未经校准；pdfium 图片对象 API（get_raw_bitmap）在个别损坏 PDF 上可能慢或失败，需逐图 try + cancel 检查；每文档 200 图 × 5 文档的两两碰撞是 200²×10 次汉明距离，纳秒级无性能问题，但导入期解码 200 张图约增加数秒，需进现有进度上报。
- **验收标准**：1) 迁移测试：migrates_fresh_db_and_is_idempotent 扩展后通过，V12 幂等；2) 单测：两份 docx 嵌同一张 JPEG（重压缩为不同字节）→ dHash 距离 ≤10 判近似命中，signals 含 kind="imageReuse"；同字节图 → exact 命中；两张随机噪声图 → 无信号；3) 整页尺寸图仅 exact 生效（构造用例断言）；4) 删除文档后 document_images 级联清空（断言行数为 0）；5) 导入含 60+ 图 docx 的现有测试耗时不劣化超 50%。

### 共同错误指纹：词典外词/异常标点/引用错误的跨文档交集 + 稀有度加权（2.5d）

- **价值**：共同罕见错误是文本取证金标准（调研 §5/§13：identical wrong answers 判别功效远超总体一致率；黔发改法规〔2024〕296 号把「错误内容异常一致」直接列为条例第 40 条涉嫌情形）。现有 shared_terms_of（compare_service.rs:813-847）只筛「罕见长词」，未区分「罕见但正确」与「疑似错误」，证明力弱一个量级。
- **设计**：在 compare_service 新增 shared_error_fingerprints(chunks: &[CmpChunk], exempt: Option<&TenderExemption>)，三类检测器跑在已有内存数据上（CmpChunk 含 text/tokens/entities，零额外 IO）：(a) 词典外词——token 长度 ≥2、全中文、jieba_rs::Jieba::has_word()==false（已确认 0.7.4 提供此 API）、非实体（金额/日期用 entities 排除）、语料内块频 ≤3；(b) 异常标点——对 chunk.text 跑固定规则集（叠标点「。。/，，」、全半角混用「，,」、中文间夹半角空格、括号不配对），指纹 = 错误串+前后各 2 字上下文；(c) 引用错误——正则抽「第X章/X.Y节/附表X」引用，引用目标不在本文档 section_path 标题树中、却在 ≥2 份文档以完全相同字串出现。三类产出统一为扩展的 SharedTerm（report.rs:109-112 增 #[serde(default)] kind: Option<String>、rarity: Option<f32>、context: Option<String>）——复用 jobs.shared_terms_json 通道，零 DB 迁移。稀有度加权用已有 features::idf_of；豁免接口：TenderExemption{tokens, normalized_text} 中出现的错误直接剔除（招标文件本身的错误各家照抄不算串标——调研 §13 明文反向豁免；W3 落地前恒传 None）。collusion.rs 新增 kind="sharedErrors"：Σ(rarity 归一分) 映射 0.10-0.30，单条极罕见错误（块频=2 且仅 2 文档共有）即起算 0.15，与现有 sharedTerms 信号（0.10）并存但同类封顶。
- **改动文件**：
  - `src-tauri/src/services/compare_service.rs`：新增 shared_error_fingerprints() 与 TenderExemption 结构；阶段 8（293 行 shared_terms_of 旁）调用并把结果并入 shared 序列化（同一 shared_terms_json 通道）
  - `src-tauri/src/engine/report.rs`：SharedTerm 增 #[serde(default)] kind/rarity/context 可选字段（旧 JSON 兼容）
  - `src-tauri/src/engine/collusion.rs`：assess_with 按 kind 过滤 shared 集合：kind="sharedErrors" 新信号 0.10-0.30 稀有度加权；原 sharedTerms 逻辑只吃 kind 为空/term 的条目；单测
  - `src-tauri/src/engine/features.rs`：如 idf_of 粒度不够，补 chunk 级 df 统计小函数（复用现有 HashMap 风格）
  - `src/api/types.ts`：CompareSummaryDto.sharedTerms 已是 unknown[]，无需改；如做展示则定义 SharedTermDto
- **DB 改动**：无（复用 jobs.shared_terms_json；SharedTerm 新字段 serde default 向后兼容旧任务行）
- **UI 改动**：无独立 UI（本条保证数据与信号；「共同错误清单」面板归统一呈现条目）
- **配置**：无
- **新依赖**：无（jieba-rs 0.7.4 has_word、regex/手写匹配、serde 均已有）
- **风险**：词典外词 ≠ 错别字：行业新词、专有名词、楼盘/设备型号都在 jieba 词典外——靠「块频 ≤3 + ≥2 文档共有 + 非实体」三重过滤压误报，但仍会有漏网，detail 必须呈现上下文供人工判断而非直接定性「错误」；引用错误检测依赖标题树质量（PDF 无标题层级时 section_path 稀疏，检测器对 PDF 语料基本失效，需按 doc_type 降级）；豁免接口在 W3 之前是空转的，「各家照抄招标文件笔误」类误报在此期间存在，属已知且可解释的临时缺口；jieba has_word 对简繁混排文本行为未验证，需加用例。
- **验收标准**：1) 单测：两份文档共享虚构错词「施工枝术」（jieba 词典外、块频 2）→ shared 列表含 kind="sharedErrors" 条目且 rarity>0，signals 含 kind="sharedErrors" weight≥0.15；同错词出现在 exempt.tokens → 条目消失；2) 叠标点「。。」两文档同上下文共现 → 检出且 context 字段含前后文；3) 「按合同执行」等高频正确短语（块频>3）不被检出（负例断言）；4) 旧任务 shared_terms_json（无 kind 字段）反序列化回归通过；5) 全量管线测试独立组（collusion_pipeline v2 的第二组）等级仍为 none/low（防信号常开）。

### 取证信号统一接入：collusion 融合规则 + Matrix 取证面板 + 六格式导出（2d）

- **价值**：前五条产出 rsid/pdfLineage/imageReuse/sharedErrors 四类新信号，但 Matrix.tsx 对未知 kind 一律按「相似」渲染（126-135 行默认分支），导出报告的文档指纹表只有 risk_flags 一列——不统一呈现，取证证据的「单点定案」价值传递不到评标人；同时四类信号线性叠加会把总分轻易推满，需要融合层的封顶与等级规则。
- **设计**：融合（collusion.rs）：新增 ForensicInputs 结构收拢 rsid_hits/lineage_hits/image_hits/shared_errors，assess_with 签名收敛为 (peak, clusters, docs, shared_terms, price_pairs, forensic)；新增两条融合规则——(1) 取证类信号合计封顶 0.45（FORENSIC_CAP 常量，防四类叠满直接 high）；(2) 硬命中（rsid root_match 或 pdfLineage 硬档）强制等级下限 medium（不直接 high：W3 模板豁免落地前防招标模板误伤），detail 统一模板附「未命中不代表清白」与「请核对是否源自招标文件统一模板」两句纪律文案（常量集中，导出复用）。前端（Matrix.tsx）：insights kind→tag 映射补四项（rsid/pdfLineage→「取证」danger 色、imageReuse→「图片」、sharedErrors→「错误」）；参评标书卡（430-436 行）FpChip 追加「模板」「rsid×N」「字体×N」chip；关键洞察卡下新增「取证指纹」折叠区：逐对列出 rsid 交集数、GUID 命中、同源图片（天干+页码）、共同错误清单（term+context），底部固定免责行「取证信号未命中不构成清白证明（另存为/元数据清洗可消除痕迹）」。导出：ExportDoc（export/data.rs:32-40）增 template_name/rsid_count/pdf_id_first/font_subset_tags 等字段；ExportData 增 forensic 节（各类命中对明细）；html.rs/docx.rs 报告增「取证证据」章节（含免责段），json.rs 自动随 serde 输出，csv/markdown/xlsx 增对应列或工作表。
- **改动文件**：
  - `src-tauri/src/engine/collusion.rs`：ForensicInputs 结构、FORENSIC_CAP=0.45、硬命中 level floor=medium 规则、纪律文案常量；全部现有单测适配新签名
  - `src-tauri/src/services/compare_service.rs`：组装 ForensicInputs 并序列化命中明细进 collusion_json 的 signals[].detail（结构化明细存 CollusionSignal 新增 #[serde(default)] evidence: Option<serde_json::Value>）
  - `src-tauri/src/engine/report.rs`：CollusionSignal 增 evidence 可选字段（旧 JSON 兼容）
  - `src/screens/Matrix.tsx`：kind 映射四项新增；FpChip 扩展；新增取证指纹折叠区与免责行
  - `src/engine.ts`：CollusionSignal 增 evidence?: unknown；Fingerprint 补齐前五条新字段
  - `src-tauri/src/export/data.rs`：ExportDoc/ExportData 扩展取证字段与 forensic 节
  - `src-tauri/src/export/html.rs`：「取证证据」章节 + 免责段（docx.rs/markdown.rs/csv.rs/xlsx.rs 同步增列/节/表）
- **DB 改动**：无（collusion_json/fingerprint_json 均 schemaless，evidence 字段 serde default 兼容旧任务）
- **UI 改动**：Matrix 屏：洞察卡四类新 tag 配色、参评标书卡新 chip、「取证指纹」折叠区、固定免责行；空态（无任何取证命中）不渲染折叠区，不出现「检查通过」类表述
- **配置**：无
- **新依赖**：无
- **风险**：assess_with 签名变更波及全部 collusion 单测与两处管线测试，属机械但易漏的适配面；等级下限 floor 到 medium 是产品级决策（未经校准语料验证，写死前需用户确认——scheme §8.1 判定可信度是最高优先短板）；导出六格式逐个补节工作量易被低估（docx.rs 排版最费时）；旧任务的 collusion_json 无 evidence 字段，前端渲染须容忍 undefined，否则历史报告页崩溃。
- **验收标准**：1) cargo test：构造 rsidRoot 命中且 peak<0.6 的输入 → level ≥ medium（floor 生效）；四类取证信号全满时 score 中取证部分 ≤0.45（cap 断言）；2) 前端 npm run build 通过，Matrix 渲染旧格式 collusion_json（无 evidence）不报错（组件测试或手测记录）；3) 导出 JSON 含 forensic 节与 documents[].rsidCount 字段（golden 断言）；HTML 报告含「取证证据」标题与免责段子串；4) 无取证命中的任务：Matrix 不出现取证折叠区、导出不出现空「取证证据」表；5) 全量测试与 clippy 绿。

**该工作流的开放问题**：

- 硬证据命中（rsidRoot / PDF DocumentID / trailer ID 相同）强制围标等级下限设为 medium 还是 high？在 W3 招标文件豁免落地前，招标代理统一下发的投标模板会天然造成 rsid/模板名/zip 序列一致，建议先 medium——需产品拍板。
- 老工作区文档的 fingerprint_json 缺新取证字段，只有重新导入才生效：是否要做「重提指纹」批量动作（只重跑 parse 的指纹部分、不动分块与向量缓存），还是接受「仅新导入生效」并在 UI 提示？
- W3 的招标文件角色接口形态：本工作流按「豁免集合」参数预留（exempt_rsids / exempt_hashes / TenderExemption{tokens,text}），需与 W3 确认其提供的招标文件解析产物能否直接产出这三类集合。
- 扫描件 PDF 整页即一张大图：本方案对整页图只做 exact 匹配不做 dHash 近似（防「都是空白页/同制式表格」误报），阈值图面积/页面积>0.8 为拍板值——是否可接受「同一张资质证书被整页扫描但压缩参数不同」因此漏检？
- 图片哈希每文档上限 200 张（超出截断）：超限文档是否需要像 OCR 20 页截断一样给显式 truncation 提示？
- 迁移编号：文档写 V8/14 表，但仓库实际已到 V11（migrations.rs MIGRATIONS 共 11 项），本工作流新表按 V12 起排——与其他工作流的迁移需在合入时协调顺序。


## 4. 设计明细：W2 入口对抗层（威胁模型）

> 设计代理的工作流综述：W2 入口对抗层（威胁模型）：在文本进入任何相似度算法之前建立"抽取文本 ≠ 文档真实内容"的防线。五条依次为：Unicode 隐形码点剥离与统计、跨脚本同形字折叠与混合脚本红旗、PDF 隐藏文字层内容流审计、渲染-OCR 抽样交叉验证、以及把"检测到刻意规避"聚合为第 6 个围标信号并在前端告警。条目 1 定义 evasion_json 数据通道（V12 迁移，注意：代码中迁移已到 V11，scheme 文档写的 V8 已过时），条目 2/3/4 向同一通道追加证据，条目 5 消费。条目 1+2 会改变 normalized_text，必须同步 bump import_service.rs 中 options_hash 的版本前缀（v5→v6），否则跨工作区分块缓存会复用旧归一化产物。对导入耗时的影响：条目 1/2 为 O(n) 字符扫描（<5%），条目 3 为纯 CPU 内容流解析（100 页 <500ms），条目 4 是唯一显著项（每份文字版 PDF 约 +5–10s，可配置关闭），条目 5 零导入成本。

### Unicode 安全归一化强化：NFKC 后显式剥离隐形码点 + 剥离统计入库（2d）

- **价值**：封堵 Bad Characters（IEEE S&P 2022）类攻击：1-3 个零宽/双向控制符即可让 exact_hash、normalized_hash、MinHash、embedding 全部失配产生假阴性。现行 normalize.rs 只做 NFKC，而 NFKC 不删除零宽与 bidi 控制符——这是已被验证的缺口。同时'正常标书不含这些码点'，剥离计数本身是高置信规避证据，为条目 5 供料。
- **设计**：在 normalize.rs 的 NFKC 之后、normalize_cn_numbers 之前插入单遍剥离：零宽（U+200B–U+200D、U+FEFF、U+200E/200F）、双向控制符（U+202A–U+202E、U+2066–U+2069）、Tags 块（U+E0000–U+E007F）、变体选择符（U+FE00–U+FE0F、U+E0100–U+E01EF），逐类计数。新增 normalize_with_stats(text, opts) -> (String, InvisibleStats)，现有 normalize() 变为丢弃统计的薄包装（既有调用点零改动）。chunker.rs:363 改用带统计版本，把每块计数写进 NewChunk → chunk_features.extra_json（可定位'扰动集中在哪些块'）；import_service.rs 聚合成文档级 {各类计数, 受影响块数, 最大单块浓度} 写入 documents.evasion_json（V12 新列）。chunks.text 保留原始字节供取证下钻，normalized_text/normalized_hash/全部特征基于清洗后文本，恢复被扰动破坏的哈希一致性。options_hash 前缀 v5→v6，跨工作区缓存与 embedding 缓存（按 normalized_hash 寻址）自然失效重建。
- **改动文件**：
  - `src-tauri/src/engine/normalize.rs`：新增 InvisibleStats 结构与剥离遍（NFKC 之后）；新增 normalize_with_stats()，normalize() 改为包装
  - `src-tauri/src/engine/chunker.rs`：第 363 行 normalize 调用改为 normalize_with_stats，per-chunk 统计随 NewChunk 传出
  - `src-tauri/src/db/repo/chunk_repo.rs`：NewChunk 增剥离统计字段，insert_all 写入 chunk_features.extra_json
  - `src-tauri/src/services/import_service.rs`：import_one 聚合文档级统计；persist_parsed/persist_cached 传 evasion_json（缓存复用路径必须一并复制）；options_hash 版本 v5→v6
  - `src-tauri/src/db/repo/document_repo.rs`：mark_parsed 增 evasion_json 参数；DocumentRow 与 SELECT 增列
  - `src-tauri/src/db/migrations.rs`：V12：ALTER TABLE documents ADD COLUMN evasion_json TEXT
- **DB 改动**：V12 迁移：documents 增 evasion_json TEXT（可空，老工作区行为 NULL，完全向后兼容）。块级统计复用既有 chunk_features.extra_json，不动表结构。
- **UI 改动**：无（呈现统一放条目 5）。
- **配置**：无新配置项；import_service.rs options_hash 版本前缀 v5→v6（缓存正确性必需）。
- **新依赖**：无。纯字符遍历，std 即可。
- **风险**：剥离集合误伤合法文本的概率极低但非零（emoji ZWJ 序列、真实 RTL 文段），标书语料基本不含，且原文保留可回查；options_hash bump 使老工作区再导入同文件时全部重新解析分块（一次性成本，需在发版说明提示）；剥离统计若不做浓度分析（只有总数）证明力弱——必须落块级分布。
- **验收标准**：1) 单测：同段文本插入 3 个 U+200B/U+202E/U+FE0F 后 normalized_hash 与干净文本完全一致，InvisibleStats 逐类计数正确；2) normalize.rs 既有全部测试（cn_numbers、digit_punctuation_preserved 等 12 个）不改动即通过；3) 集成：导入含零宽字符的 docx 后 documents.evasion_json 非空且计数正确，chunk_features.extra_json 可定位到含扰动的块；4) migrations 测试：V12 在 V11 库上执行成功且幂等；5) 10 万字文本 normalize 耗时增幅 <5%（基准断言）。

### 同形字防线：静态 confusables 折叠 + 同词内混合脚本红旗（1.5d）

- **价值**：NFKC 折叠不了跨脚本同形字（西里尔 а/о/р、希腊 ο 等），攻击者用它们替换雷同段落中的拉丁字母/数字旁字符即可击穿全部词面通道。简化版 UTS#39 skeleton（静态表）恢复匹配能力；'同一词内拉丁+西里尔混排'在正常中文标书中不存在，是零训练成本的高置信红旗。
- **设计**：新建 src-tauri/src/engine/confusables.rs：静态 const 排序数组存 (char, char) 映射（西里尔全套 + 希腊高置信视觉同形 → 拉丁骨架，约 200–300 条，二分查找，无运行时加载），fold() 在 normalize_with_stats 的 NFKC+隐形剥离之后调用，命中计数并入条目 1 的 InvisibleStats 统计通道。混合脚本检测与 fold 解耦：把文本切成连续字母数字 run，逐字符标脚本（Han/Latin/Cyrillic/Greek），同一 run 内 Latin+Cyrillic 共存、或单个 Cyrillic/Greek 字符嵌在拉丁/数字序列中 → 红旗计数并采样词入库；Han+Latin 混排（'AI平台'）与整词希腊技术符号（10μm、Ω）明确不触发以控误报。数据流复用条目 1：块级计数 → 文档级聚合 → evasion_json。与条目 1 同版发布共用 v6 缓存指纹；分开发布则再 bump 一次。
- **改动文件**：
  - `src-tauri/src/engine/confusables.rs`：新文件：静态映射表、fold()、mixed_script_scan()（返回红旗计数与采样词）
  - `src-tauri/src/engine/mod.rs`：注册 confusables 模块
  - `src-tauri/src/engine/normalize.rs`：normalize_with_stats 接入 fold，命中计入 InvisibleStats
  - `src-tauri/src/services/import_service.rs`：evasion_json 聚合增 confusableFolds / mixedScriptWords / 采样词字段
- **DB 改动**：无新迁移（写条目 1 的 evasion_json 列）。
- **UI 改动**：无（条目 5 统一呈现，采样词作为可下钻证据）。
- **配置**：与条目 1 共用 options_hash bump；无新配置项。
- **新依赖**：无。静态表内置源码，拒绝引入 ICU 级依赖（体积与离线约束）。
- **风险**：静态子集覆盖不全（UTS#39 全表 6000+ 条），冷门脚本同形会漏——接受，命中即证据、漏检不恶化现状；俄文资质证书等合法西里尔片段可能触发红旗——以'同词内混排'而非'文档含西里尔'为判定单位可压住大部分，剩余靠人工复核措辞兜底；汉字内部形近/同音替换（survey §13 中文洗稿防线）明确不在本条范围，防止范围蔓延。
- **验收标准**：1) 单测：'Pагe'（拉丁 P + 西里尔 аге）fold 后与 'Page' 的 normalized_hash 相等；2) 单测：混合脚本对 'Дeposit'（词内混排）触发红旗，对 'AI平台'、'5G基站'、'10μm' 均不触发；3) 集成：导入含同形替换的 docx，两份仅差同形字的文档在 normalized_hash 通道召回命中，且 evasion_json.confusableFolds>0、mixedScriptSamples 含词样本；4) 全量既有测试通过；10 万字 fold 增耗 <5%。

### PDF 隐藏文字层内容流审计（Tr=3 / 白字 / 出画布 / 极小字号）（3d）

- **价值**：抽取工具照单全收隐藏文字层，攻击者可注入两套内容：可见给评标人、隐藏污染查重（或反向把雷同正文藏进不可见层）。内容流级审计把'可见 vs 抽取'差集定位到具体对象，是交叉验证（条目 4）告警的可解释证据来源，且纯 CPU 零模型依赖。
- **设计**：新建 src-tauri/src/engine/pdf_audit.rs，用已有 lopdf（parse.rs pdf_fingerprint 已在用）逐页 Content::decode 内容流，走小型图形状态机：跟踪 Tr（文本渲染模式）、Tf 字号、Tm/Td/TD/T* 文本矩阵、g/rg/k/sc/scn 填充色与页面 MediaBox；对每个 Tj/TJ/'/" 展示串按当前状态归类计数：Tr=3 不可见、填充亮度 ≥0.97（白字）、文本原点出 MediaBox、有效字号（Tf × 矩阵缩放）<1pt。产出 hidden_chars/total_chars 占比与逐页命中。关键防误报：识别'OCR 双层页'模式（整页图像 XObject + 全页隐藏文本 = 合法扫描件 OCR 层）单独归类 ocr_layer_pages 不计入规避；仅'同页可见文本与隐藏文本共存'或'隐藏文本页无整页图像'算注入嫌疑。审计上限 500 页/单流 10MB，超限记 partial。parse_pdf 三级回落前先跑审计（与抽取方式无关），结果挂 ParsedBlocks 新字段，import_service 合入 evasion_json。导入耗时：100 页文本 PDF <500ms，占比 <2%。
- **改动文件**：
  - `src-tauri/src/engine/pdf_audit.rs`：新文件：PdfHiddenStats、内容流操作符状态机、OCR 双层页归类、audit(path) 入口
  - `src-tauri/src/engine/parse.rs`：parse_pdf 调用 audit；ParsedBlocks 增 pdf_audit 字段（Option，非 PDF 路径 None）
  - `src-tauri/src/engine/mod.rs`：注册 pdf_audit 模块
  - `src-tauri/src/services/import_service.rs`：import_one 把 pdf_audit 统计合入 documents.evasion_json
- **DB 改动**：无新迁移（写条目 1 的 evasion_json 列，pdfAudit 子对象）。
- **UI 改动**：无（条目 5 统一呈现，逐页命中数供下钻）。
- **配置**：无。审计始终运行（成本可忽略），不设开关降低配置面。
- **新依赖**：无。lopdf 已是依赖（pdf_fingerprint 在用），MIT license，零增量体积。
- **风险**：PDF 方言多：Form XObject 嵌套（首版只下钻一层）、Type3 字体、CID 双字节编码使'字符数'只能按展示串字节近似——报告措辞用'占比'不承诺精确字数；白字判定不知真实背景（深色底设计稿会误报），启发式残余误报定位为'线索级'而非'确认级'；损坏 PDF 上 lopdf 解析失败必须静默降级 audit=None，不能阻塞导入（与 pdf_fingerprint 同容错语义）。
- **验收标准**：1) 单测：lopdf 程序化构造含 Tr=3 段、白字段、出画布段、0.5pt 字号段的 PDF，四类计数各自正确、hidden 占比符合构造预期；2) 单测：构造'整页图片+隐藏文本层'的 OCR 式 PDF → 归入 ocr_layer_pages 且规避计数为 0；3) tests/fixtures/sample.pdf（正常文字版）hidden_chars=0；4) 集成：导入后 evasion_json.pdfAudit 就位；伪造损坏 PDF 导入不失败；5) 100 页 PDF 审计耗时 <500ms（基准断言）。

### 渲染-OCR 抽样交叉验证：字体重映射/坐标乱序/图片化正文检出与 OCR 回落（3.5d）

- **价值**：封堵 PDF Mirage（ToUnicode 重映射：渲染一套、抽取另一套）与 PDFuzz（坐标乱序：字符集合对、顺序错）——这两类攻击让文字层 100% 失真而全部下游算法无声失效，是 survey 执行摘要第一发现的核心对策。命中后改用 OCR 文本参与比对，规避事实单列告警。
- **设计**：parse_pdf 前两级（pdfium/pdf-extract）成功后触发：确定性抽样 K=min(5, 页数) 页——首页+末页+均匀间隔页，条目 3 标记的可疑页优先顶替间隔页（同输入同样本，保可复现）。新增 rasterize_pages(path, indices)（现有 rasterize_pdf 的按索引变体，复用 PdfRenderConfig）→ 现有 ocr_images（档位随 parser.ocr_model）。逐页比对：双方文本各过条目 1/2 的 sanitize+normalize（忽略空白标点），算字符 2-gram 多重集 Dice（内容一致性）+ LCS 近似顺序分（截断 4000 字符控 O(n²)）；pdfium 路径逐页比，pdf-extract 路径（块无页码）退化为'OCR 页 8-gram shingle 在全文文字层的包含率'。判定：中位内容失配 >0.35 → '字体重映射/图片化正文'；Dice ≥0.8 且顺序分 <0.5 → '坐标乱序'。任一命中：整文档改走既有 parse_pdf_ocr（受 OCR_MAX_PAGES=20 与 truncation_notice 约束），method 记 'ocr-fallback'，结论写 evasion_json.xcheck；OCR 模型缺失或 pdfium 不可绑定时记 xcheck.skipped，不阻塞导入。耗时：每份文字版 PDF 约 +5–10s（5 页 × 渲染 0.2s + small 档 OCR 1–2s），文件级 rayon 并行摊薄，扫描件本就走 OCR 不受影响。
- **改动文件**：
  - `src-tauri/src/engine/parse.rs`：rasterize_pdf 拆出 rasterize_pages(path, indices)；parse_pdf 在 pdfium/pdf-extract 成功后按配置调用交叉验证，命中则回落 parse_pdf_ocr
  - `src-tauri/src/engine/pdf_xcheck.rs`：新文件：确定性抽样、2-gram Dice + 顺序分、shingle 包含率（pdf-extract 路径）、XCheckResult
  - `src-tauri/src/config.rs`：ParserDefaults 增 pdf_cross_check: bool（默认 true）
  - `src-tauri/src/services/import_service.rs`：ImportOptions 增 pdf_cross_check 并入 options_hash；xcheck 结果合入 evasion_json
- **DB 改动**：无新迁移（evasion_json.xcheck 子对象：sampled_pages、逐页失配率、verdict、skipped 原因）。
- **UI 改动**：Settings 屏解析选项区增'PDF 渲染交叉验证'开关（与 ocr_docx_images 同版式，附耗时说明）。
- **配置**：parser.pdf_cross_check（默认 true）；并入 options_hash（改开关后缓存不误复用）。
- **新依赖**：无。渲染用 pdfium-render、OCR 用 oar-ocr，全部现有。
- **风险**：0.35 失配阈值未经语料校准（与围标权重同病）：低质打印、密集表格页、印章覆盖会推高 OCR 噪声——判定措辞必须是'文字层不可信，已改用 OCR 文本并请人工复核'而非指控结论，阈值常量集中放置留待 scheme §9.3 合成语料回测；组合变音符攻击可反制 OCR（When Vision Fails）——其效果是推高失配率即'更敏感'方向，不产生漏报但可能误报，靠 OCR 置信度将来加权；OCR 回落受 20 页上限约束，超长规避文档后置页仍不可见（truncation_notice 已如实提示）；每 PDF +5–10s 是导入期最大新增成本，默认开需产品确认。
- **验收标准**：1) 集成：构造文字层含大段隐藏垃圾文本的 PDF（lopdf 注入）→ 失配 >0.35，parse_method=ocr-fallback，evasion_json.xcheck.verdict 就位；2) 构造逐字符 Td 乱序摆放（视觉正常）的 PDF：若 pdfium 抽取呈乱序则必须触发'坐标乱序'判定，若 pdfium 已按坐标重排则测试记录该路径免疫并断言不误报；3) 正常 sample.pdf 中位失配 <0.15 且不触发回落；4) 抽样确定性：同文件两次导入 sampled_pages 逐元素相等；5) pdf_cross_check=false 时 xcheck.skipped 且导入耗时与基线差 <1s；OCR 模型缺失时导入成功且 skipped 原因为模型缺失。

### '检测到刻意规避'独立围标信号 + 前端告警呈现（2d）

- **价值**：把条目 1–4 的证据从'埋在库里'变成结论：规避行为本身即极强串通证据（正常投标人不会做字体重映射/零宽注入），比文本相似度更难抵赖。缺这一条，前四条的产出用户永远看不见。
- **设计**：compare_service.rs 第 8 阶段构建 DocInfo 处（约 295 行）读取 documents.evasion_json；report.rs 增 EvasionSummary（各类计数 + xcheck verdict + 严重级 none/suspect/confirmed，serde camelCase），DocInfo 增 Option<EvasionSummary>（旧任务 JSON 反序列化天然兼容）。判级规则集中常量：confirmed = xcheck 命中 或 隐藏文本占比 ≥5% 或 同词混合脚本词 ≥3；suspect = 隐形码点 ≥10 或 confusable fold 命中等弱证据。collusion::assess_with 增第 6 信号 kind="evasion"：任一文档 confirmed 记满权 EVASION_WEIGHT=0.25，仅 suspect 记半权，同类证据不叠加（与报价信号同惯例），detail 用天干标签列出文档与证据种类；常量放进权重集中区并沿用'未经校准'注释。前端：Matrix.tsx insights 的 kind 映射增 evasion（最高严重档样式）；DocPreview.tsx 增告警条（复用 truncationNotice 版式 238–249 行）；Library 文档卡加徽标；types.ts DocumentDto 增 evasionSummary。导出：signals 数组走通用序列化自动带出，补 html/docx/markdown/xlsx 四处 kind→中文标签映射'检测到规避特征'。
- **改动文件**：
  - `src-tauri/src/engine/report.rs`：EvasionSummary 结构；DocInfo 增 evasion: Option<EvasionSummary>
  - `src-tauri/src/engine/collusion.rs`：assess_with 增 evasion 信号与 EVASION_WEIGHT=0.25 常量（集中权重区，标注未校准）
  - `src-tauri/src/services/compare_service.rs`：DocInfo 构建处解析 evasion_json 并传入 assess_with
  - `src-tauri/src/db/repo/document_repo.rs`：DocumentRow/SELECT 增 evasion_json（若条目 1 未覆盖 list 查询）
  - `src/api/types.ts`：DocumentDto 增 evasionSummary；Collusion signal kind 联合类型加 evasion
  - `src/screens/Matrix.tsx`：insights kind 映射增 evasion（红色最高档）
  - `src/screens/DocPreview.tsx`：evasion 告警条（复用 truncationNotice 版式）
  - `src-tauri/src/export/html.rs`：signal kind 中文标签映射补'检测到规避特征'（docx/markdown/xlsx 同步）
- **DB 改动**：无新迁移；仅查询增列（读 V12 的 evasion_json）。
- **UI 改动**：Matrix 围标信号分解区新增红色'规避特征'条目；Library 文档卡徽标；DocPreview 顶部告警条列出证据种类并可下钻（码点所在块/PDF 页码）。
- **配置**：无。
- **新依赖**：无。
- **风险**：产品风险最大的一条：'刻意规避'是指控性措辞，误报后果重——UI 与导出文案统一为'检测到疑似规避特征，请人工复核'，机器不下结论（与三态人工复核原则一致）；EVASION_WEIGHT=0.25 未校准，单信号不达 high 线 0.6 是有意设计（单证据不定罪）；旧比对任务 collusion_json 无此信号，前端 signals 数组遍历天然容忍缺失但需回归验证。
- **验收标准**：1) collusion.rs 单测：confirmed 文档 → signals 含 kind=evasion 权重 0.25，仅 suspect → 0.125，无 evasion → 无该信号，既有全部阈值测试不改动通过；2) 集成（compare_service 校准门禁测试区）：导入带零宽扰动 + 隐藏文字层夹具后 run_compare，collusion_json.signals 含 evasion 且 detail 含天干标签与证据种类；3) contract.test.ts：DocumentDto 断言含 evasionSummary；4) 导出测试：HTML/JSON 报告含 evasion 信号与中文标签字符串；5) 打开无 evasion 数据的旧工作区矩阵页无报错（前端回归）。

**该工作流的开放问题**：

- parser.pdf_cross_check 默认开还是关：每份文字版 PDF 增加约 5–10s 导入耗时（5 页抽样 × small 档 OCR），推荐默认开 + Settings 可关，需产品确认默认值与文案。
- 阈值先行还是校准先行：交叉验证失配率 0.35、隐藏文本占比 5%、混合脚本词 ≥3 等判级线均为经验值，是否等 scheme §9.3 合成对抗语料生成器（可程序化生成 ToUnicode 重映射/零宽注入样本）先落地再回测固化，还是先按经验值上线并沿用 collusion.rs 的'未校准'注释惯例。
- docx 侧隐藏文字（w:vanish 属性、w:color=FFFFFF 白字）不在本批条目范围但检测成本极低（docx_blocks 顺手读 rPr），是否追加为条目 3 的姊妹小项。
- 条目 1/2 若分两次发布，options_hash 需 bump 两次（v6→v7）并各自作废一次跨工作区缓存与 embedding 缓存；建议同版发布共用一次 v6，需确认排期允许。
- confusables 折叠是否也应用于 embedding 输入文本：会使既有 embeddings 缓存键（normalized_hash）全部失效并触发一次全量重推理，磁盘中旧向量成死数据，是否需要顺带清理迁移（类似 V8 EMBEDDINGS_RESET 先例）。
- 中文形近字/同音字替换归一（survey §13 中文洗稿防线，UTS#39 与 NFKC 均覆盖不到）明确未列入本批——是否在 W2 后续批次立独立条目。


## 5. 设计明细：W3 合法共享剥离层

> 设计代理的工作流综述：W3 合法共享内容剥离层：招标文件角色与对减、k-共现查证、段落级 IDF 背景加权、分区分层阈值（锚定 v0.5.0 真实代码：compare_service.rs 八阶段编排 / chunker.rs is_template / candidate.rs stop_gram_df / migrations.rs 现已到 V11，故新迁移从 V12 起编号）

### 文档角色 doc_role：招标文件（含补遗/答疑）导入与参评隔离（2d）

- **价值**：后续对减、k-共现查证全部依赖『哪些文档是招标文件』这一事实基础；同时杜绝招标文件被误选为投标文件参评，制造整片假雷同。这是 W3 其余条目的前置。
- **设计**：documents 表加 doc_role 列（'bid' 默认 | 'tender' 招标文件 | 'tender_supplement' 补遗/答疑），旧行靠 DEFAULT 'bid' 向后兼容。导入命令新增可选 docRole 参数，贯穿 run_import → import_one → document_repo::create_parsing（含跨工作区 cache 复用路径）；同 hash 去重收窄为同角色去重（find_by_hash 加 doc_role 条件），允许同一文件以两种角色各存一份。比对入口在 commands/compare.rs 校验 documentIds 全部为 bid 角色，否则返回 InvalidConfig。前端文档卡片网格实际在 CompareSetup.tsx（注意：Library.tsx 是查重源样板屏，不是文档列表），拆为『投标文件』『招标文件』两组：招标组带独立导入按钮与角色徽标、不可勾选、不占 2–10 参评名额；parsed 可选集过滤 docRole==='bid'。
- **改动文件**：
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/migrations.rs`：新增 DOC_ROLE_V12 迁移并追加进 MIGRATIONS 数组（现数组末项为 DOC_TRUNCATION_NOTICE_V11）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/repo/document_repo.rs`：DocumentRow 加 doc_role 字段；SELECT 常量与 map_row 扩列；create_parsing 增参；find_by_hash 加 AND d.doc_role=?3；新增 list_by_role(conn, workspace_id, roles)
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/services/import_service.rs`：run_import/import_one 贯穿 doc_role（默认 'bid'），两条 create_parsing 调用点（缓存复用路径 246-258 行与常规路径 270-282 行）都传角色
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/commands/document.rs`：导入命令加可选 docRole 参数并校验取值 ∈ {bid, tender, tender_supplement}
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/commands/compare.rs`：start_compare 校验所选 documentIds 角色均为 bid，招标文件混入时报 InvalidConfig 且给出文件名
  - `/Users/zt/zt-all/bidguard-app/src/api/types.ts`：DocumentDto 加 docRole 字段；导入请求参数类型加 docRole
  - `/Users/zt/zt-all/bidguard-app/src/queries/data.ts`：useImportDocuments 支持传 docRole
  - `/Users/zt/zt-all/bidguard-app/src/screens/CompareSetup.tsx`：文档网格分『投标文件/招标文件』两组；新增『导入招标文件（含补遗/答疑）』按钮（复用 pickBidFiles + doImport）；tender 卡片显示徽标、onToggle 禁用；parsed useMemo 过滤 docRole==='bid'
- **DB 改动**：V12：ALTER TABLE documents ADD COLUMN doc_role TEXT NOT NULL DEFAULT 'bid'。纯加列，旧工作区打开即兼容。（注意：任务书写 DB 改动走 V9+，但仓库迁移实际已到 V11——V9/V10/V11 已被索引清理、cluster_members 索引、truncation_notice 占用，本工作流从 V12 起。）
- **UI 改动**：CompareSetup.tsx：文档区分组展示；招标文件卡片带『招标/补遗』徽标、不可勾选、不计入『开始交叉比对（N）』计数；招标组空态引导文案。
- **配置**：无新增配置项；导入命令 docRole 为请求级参数。
- **新依赖**：无
- **风险**：用户把标底/招标文件误标为投标（或反之）只能靠 UI 引导，无法程序校验；同 hash 双角色并存后跨工作区 chunks 缓存复用按 hash+options 匹配（与角色无关），行为正确但需回归验证；补遗答疑常为扫描件，其解析/OCR 质量直接决定条目 2 的对减召回上限。
- **验收标准**：cargo test 新增并通过：(1) docRole='tender' 导入后 list_by_role 查回且 chunk_count>0；(2) 同一文件先后以 bid/tender 导入产生两行文档；(3) start_compare 的 documentIds 含 tender 文档时返回 InvalidConfig；(4) migrations 幂等测试扩展——V11 旧库升级后所有既有行 doc_role='bid'。前端：tender 文档不可勾选、不进参评计数（手测清单或组件测试）。

### 招标文件对减：winnowing 指纹库 + 残差比对 + 原始/剔除后双数字矩阵（4d）

- **价值**：直接消灭标书场景最大单一误报源——对招标条款的合法逐字应答与法定格式文本（SOTA 调研 §14 判定：缺此模块任何相似度引擎输出都被模板噪声淹没）。把现有『整块 is_template 余弦标记』升格为独立的前置剥离阶段。
- **设计**：新增 engine/winnow.rs：对 normalized_text 做字符级 k-gram（k=15）哈希（复用 features::hash64 的 XxHash64），窗口 w=10 取窗内最小哈希（并列取最右，保证确定性），密度 ≤2/(w+1)，形式保证：任何 ≥k+w-1=24 字的共享片段至少留一个共同指纹。run_inner 第 1 步后用 document_repo::list_by_role 加载本工作区 tender/tender_supplement 的段落级分块（chunk_repo::load_for_compare）构建 TenderIndex（HashSet<u64>）；对每个投标 CmpChunk 计算命中指纹覆盖区间（命中位置各覆盖 [p, p+k)，间隔 ≤k 合并），得 tender_coverage∈[0,1]（CmpChunk 新字段）。coverage≥0.8 的块标记『引用招标文件』；召回与精排照常跑全量，但边分两套：全量边与『双方 coverage<0.8』的残差边——clustering::cluster 跑两遍，残差簇落库驱动报告/分类/围标，全量簇只喂 matrix::doc_matrix 产出『原始相似度』；matrix_json 增 matrixOriginal/peakOriginal 字段，风险分级只用剔除后数字。豁免证据写入新表 chunk_exemptions（kind='tender'，含 coverage），供 UI 与导出解释、人工复核被剥离内容。
- **改动文件**：
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/winnow.rs`：新文件：fingerprints(text,k,w)->Vec<(u64,pos)>、TenderIndex 构建、coverage(text,&TenderIndex)->(f32, spans)；含确定性与形式保证的单元测试
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/corpus.rs`：CmpChunk 加 tender_coverage: f32 字段（默认 0.0）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/services/compare_service.rs`：run_inner：加载 tender 分块建索引→标注 coverage→边集拆全量/残差→clustering::cluster 两遍→matrix::doc_matrix 两遍→matrix_json 加 matrixOriginal/peakOriginal；CompareRunConfig 加 subtract_tender: bool（默认 true）；CompareSummary 加 tender_ref_chunk_count；persist 阶段写 chunk_exemptions
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/migrations.rs`：V13：CREATE TABLE chunk_exemptions(job_id REFERENCES jobs(id) ON DELETE CASCADE, chunk_id, kind, coverage REAL, spans_json TEXT, PRIMARY KEY(job_id,chunk_id,kind))
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/repo/compare_repo.rs`：chunk_exemptions 的批量插入与按 job/chunk 查询；delete_job_results 级联覆盖（ON DELETE CASCADE 已兜底，补断言测试）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/commands/compare.rs`：CompareRequest 映射 subtractTender→subtract_tender
  - `/Users/zt/zt-all/bidguard-app/src/api/types.ts`：CompareRequest 加 subtractTender?；CompareSummaryDto.matrix 类型加 matrixOriginal/peakOriginal；CompareSummary 加 tenderRefChunkCount
  - `/Users/zt/zt-all/bidguard-app/src/screens/Matrix.tsx`：单元格主显剔除后相似度，角标/tooltip 显示原始值；顶部摘要注明『已剔除招标文件引用 N 块』
  - `/Users/zt/zt-all/bidguard-app/src/screens/CompareSetup.tsx`：检测设置加『剔除招标文件内容』开关行（工作区存在 tender 文档时显示，默认开）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/export/`：HTML/DOCX/Markdown 模板加『原始相似度/剔除后相似度』双行；JSON/CSV 增字段（导出读 matrix_json，改动集中在模板层）
- **DB 改动**：V13 新表 chunk_exemptions（job 级豁免证据，随任务删除级联清理；kind 预留给条目 3/4 复用）。不改既有表。
- **UI 改动**：Matrix.tsx 双数字展示；CompareSetup.tsx 剔除开关；ClusterDetail.tsx/DocPreview.tsx 对 coverage≥0.8 的块显示『引用招标文件 · 覆盖 xx%』徽标（读 chunk_exemptions，不做区间内高亮——normalize 改变字符偏移，区间映射回原文留待后续）。
- **配置**：CompareRunConfig/CompareRequest 加 subtractTender（默认 true）；k=15/w=10/覆盖豁免线 0.8 以代码常量集中定义（对齐 collusion.rs 的常量集中风格），暂不开放 UI 调参。
- **新依赖**：无（winnowing 自实现约 150 行，哈希复用 twox-hash，已在依赖树中）
- **风险**：扫描件招标文件的 OCR 错字会打断精确 k-gram 指纹链导致漏剥离（k=15 有一定容忍度，PDF 文字层文档不受影响；OCR 招标文件的对减召回需在验收时单独测量并记录为已知限制）；对抗面：投标人可把串通内容混进大段招标引用中（79% 引用+21% 抄袭的块整块豁免）——0.8 阈值+豁免块在 UI 保持可见可复核是当前对冲；全量+残差两套聚类增加一次 union-find 与一次矩阵聚合（便宜）但精排会覆盖模板密集块对，最坏情形耗时上升需用 perf_smoke 基准验证；双数字对评标人的解释成本，导出措辞需明确『风险分级采用剔除后口径』。
- **验收标准**：cargo test：(1) winnow 单元测试——任意 ≥24 字共享子串两文本必有共同指纹、同输入两次运行指纹逐项相等；(2) 端到端——招标文件 T + 两份大量逐字引用 T 且另有私有雷同段的投标 A/B：断言 matrixOriginal[0][1] > matrix[0][1]、残差簇成员不含引用段文本、chunk_exemptions 行数等于 coverage≥0.8 的块数；(3) 工作区无招标文件或 subtractTender=false 时，现有全部 compare_service 测试不改动通过（结果与 v0.5.0 逐字节一致）；(4) matrix_json 同时含 matrix 与 matrixOriginal 两个矩阵且对角线均为 1。

### k-共现过滤升级：≥3 家共有段落先查证，查得到豁免、查不到升级『多家异常一致』（2.5d）

- **价值**：利用 2–5 份交叉比对独有的集合结构把模板噪声翻转为判别特征：≥3 家共有 ≈ 范本（须查证），恰好 2 家共有 ≈ 串标嫌疑；查不到出处的多家一致本身即《招标投标法实施条例》第四十条『投标文件异常一致』的法定涉嫌情形，从『笼统加分』变成『可引用法条的独立信号』。
- **设计**：现状：collusion.rs 信号②对 docs.len()≥3 的簇一律按强雷同加分（CLUSTER_MULTI_DOCS=3），不做出处查证。升级：build_clusters 后新增 apply_shared_exemptions()——对 docs_present≥3 的每个簇，逐成员查两库：(a) 条目 2 的 TenderIndex（多数成员 tender_coverage≥0.8）；(b) 条目 4 的背景库（多数成员 boiler_fraction≥0.6）。任一命中→簇写 exempt_reason='tender'|'background'，从围标信号②计数、残差矩阵聚合、high 风险统计中剔除，但簇保留落库、UI 置灰可筛（延续 is_template『标记不删除』哲学）。两查皆空且查证条件具备（存在招标文件 或 背景库文档数≥20）→ 簇标 multi_doc_anomaly=1、severity 升 high，collusion 新增 kind='multiDocAnomaly' 信号（常量 MULTI_ANOMALY_WEIGHT=0.30，detail 引用条例第四十条措辞）；查证条件不具备时维持现行为，防止无库可查的冷启动工作区全量误升级。恰好 2 家共有的簇不变，ClustersScreen 加『仅两家共有』快捷筛选作为首要证据视图。
- **改动文件**：
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/services/compare_service.rs`：新增 apply_shared_exemptions(&comparable, &raw, &mut new_clusters, tender_idx, bg_idx)，在 build_clusters 与 apply_fact_conflicts 之间调用；r_clusters 组装（314-333 行）带 exempted/anomaly 字段；残差矩阵与 summary.high_risk_count 排除豁免簇
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/report.rs`：Cluster 结构加 exempted: bool、anomaly: bool
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/collusion.rs`：assess_with 信号②只计非豁免簇；新增 multiDocAnomaly 信号分支与 MULTI_ANOMALY_WEIGHT 常量（放入现有『权重集中区』14-32 行，沿用未校准警示注释）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/repo/compare_repo.rs`：NewCluster/ClusterSummaryRow 加 exempt_reason、multi_doc_anomaly；insert_clusters/list_clusters/get_cluster_detail 扩列；ClusterFilter 支持按豁免/异常筛选
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/migrations.rs`：V14：ALTER TABLE clusters ADD COLUMN exempt_reason TEXT; ALTER TABLE clusters ADD COLUMN multi_doc_anomaly INTEGER NOT NULL DEFAULT 0
  - `/Users/zt/zt-all/bidguard-app/src/api/types.ts`：ClusterSummaryDto 加 exemptReason/multiDocAnomaly；ClusterFilter 扩展
  - `/Users/zt/zt-all/bidguard-app/src/screens/ClustersScreen.tsx`：豁免簇置灰+来源徽标（招标文件/背景库）；『多家异常一致』红色徽标与筛选；『仅两家共有』快捷筛选
  - `/Users/zt/zt-all/bidguard-app/src/screens/Matrix.tsx`：围标信号分解区渲染 multiDocAnomaly 信号（现有 kind/detail 通用渲染基本免改，补图标与法条角标）
- **DB 改动**：V14：clusters 加 exempt_reason（NULL=未豁免）与 multi_doc_anomaly 两列，纯加列向后兼容；豁免明细复用条目 2 的 chunk_exemptions。
- **UI 改动**：ClustersScreen 豁免置灰/异常标红/两家共有筛选；ClusterDetail 显示豁免出处（命中的招标文件段或背景判定）；导出报告新增『多家异常一致清单』小节（对应法定情形，评标人可直接引用）。
- **配置**：查证门槛常量（背景库最小文档数 20、多数成员比例）集中在 compare_service.rs 常量区；不新增用户配置。
- **新依赖**：无
- **风险**：围标团伙占 5 家中 3+ 家时若提前拿到招标文件并把串通内容伪装成逐字应答，仍可被豁免洗白（与条目 2 共通，靠豁免内容全程可见可复核对冲）；MULTI_ANOMALY_WEIGHT=0.30 与现有五信号一样未经标注语料回测（collusion.rs 已明示），须随 scheme §9.3 合成语料一起校准，上线初期建议 detail 文案用『涉嫌』措辞；豁免簇退出矩阵/高风险统计会改变既有任务与新任务的口径差异，导出需标注引擎版本。
- **验收标准**：cargo test：(1) 三份投标共享某段且该段在招标文件中 → 该簇 exempt_reason='tender'，collusion.signals 无 multiDocAnomaly，信号②计数不含该簇；(2) 同场景但该段不在招标文件、背景库 doc_count≥20 亦查不到 → multi_doc_anomaly=1、severity='high'、signals 含 kind='multiDocAnomaly'；(3) 无招标文件且背景库 doc_count<20 → collusion_pipeline_on_generated_bids_v2 等既有测试不改动通过；(4) V14 迁移后旧库 clusters 两新列默认值正确且 list_clusters 可查。

### is_template 升级为段落级 IDF 背景加权（背景语料：内置范本 + 历史文档增量 DF 库）（3.5d）

- **价值**：现状 is_template 只对 3 条内置样板做余弦≥0.7 整块二值匹配（chunker.rs TEMPLATE_MATCH），拦不住『招标文件里没有、但全行业都在抄』的范本套话。改为可复现的统计定义（Lang–Stice-Lawrence 双阈值 4-gram DF），背景库随使用自增强，且为条目 3 的查证提供第二个出处库。
- **设计**：建增量 DF 库：background_grams(gram i64 PK, df) + background_meta(doc_count)。每次导入解析成功后，对该文档段落级 normalized_text 的去重字符 4-gram 集（features.rs 新增 char_ngrams_n(s,4)，哈希复用 hash64）批量 UPSERT df+=1（在既有 ctx.write_lock 内）；同 file_hash 只计一次（缓存复用路径不 bump），首次启动回填存量 parsed 文档并把 source_templates 文本与内置范本资产各计 1 篇。比对时加载双阈值集：df/doc_count≥60% → boilerplate 集（行业套话），>80% → legal 集（法定必备表述，既不作证据也不作嫌疑）；对每块算 boiler_fraction=命中 boilerplate 集的 4-gram 占比，其中每个 gram 的 df 先扣除本次参评文档自身的贡献（在 comparable 内统计该 gram 的本场文档数并减去）——这一步是防自我豁免的关键：围标组共享的私有段落不会因『本场 3/5 家都有』而混入高 DF。boiler_fraction≥0.6 且 doc_count≥20 时视同 is_template（ignore_templates 开启则不进聚类），证据落 chunk_exemptions(kind='background', coverage=boiler_fraction)；现有查重源余弦通道保留并联（Library.tsx 查重源屏不动）。选择比对期计算而非导入期落库：背景库随时间增长，导入期落库会使 options_hash/跨工作区分块缓存永久失效并留下陈旧标记。
- **改动文件**：
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/migrations.rs`：V15：CREATE TABLE background_grams(gram INTEGER PRIMARY KEY, df INTEGER NOT NULL); CREATE TABLE background_meta(key TEXT PRIMARY KEY, value_json TEXT NOT NULL)
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/db/repo/background_repo.rs`：新文件：bump_doc(conn, grams)（事务化批量 UPSERT + doc_count+1 + 已计 file_hash 记账）、load_thresholded(conn)->(boiler: HashSet<u64>, legal: HashSet<u64>, doc_count)、rebuild(conn)（Tools 屏重建入口）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/features.rs`：新增 char_ngrams_n(s, n) 通用函数（现 char_ngrams 固定 2/3-gram，13-25 行）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/services/import_service.rs`：import_one 成功路径（persist_parsed 后）计算文档级 4-gram 去重集并 background_repo::bump_doc；缓存复用路径按 file_hash 判重不重复计数
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/services/compare_service.rs`：run_inner 加载双阈值集→对每块算 boiler_fraction（含本场 df 扣除）→ignore_templates 时排除 boiler_fraction≥0.6 的块→写 chunk_exemptions(kind='background')；jobs.config_json 补记 background doc_count 快照（复现口径）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/lib.rs`：启动时若 background_meta 为空触发一次性回填任务（复用 jobs 基建或同步执行，量小）
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/commands/tools.rs`：暴露背景库状态查询与『重建背景库』命令
  - `/Users/zt/zt-all/bidguard-app/src/screens/Tools.tsx`：背景库卡片：文档数/gram 数/重建按钮
- **DB 改动**：V15：background_grams + background_meta 两新表（含已计 file_hash 记账，可放 background_meta value_json 或独立表）。存量库首启回填，不阻塞既有功能。
- **UI 改动**：Tools.tsx 背景库状态与重建；ClusterDetail/DocPreview 复用条目 2 的豁免徽标（『行业范本套话 · 背景占比 xx%』）；Library.tsx（查重源屏）不变，仅在提示文案中说明背景加权与样板剔除并联生效。
- **配置**：双阈值 60%/80%、豁免线 0.6、最小语料量 doc_count≥20 以常量集中定义；ignore_templates 语义扩展为『样板余弦命中 OR 背景加权命中』，CompareSetup.tsx 该开关 hint 文案同步更新。
- **新依赖**：无（4-gram 哈希与 UPSERT 全部复用现有 twox-hash/rusqlite）
- **风险**：df 只增不减：删除文档不回滚计数，长期漂移——以 Tools 重建命令对冲并在文档中声明；background_grams 行数随语料线性增长（百份标书量级约数百万行/几十 MB，SQLite 可承受，但 bump_doc 必须单事务批量执行避免逐行 UPSERT 拖慢导入）；比对结果随背景库演化而变化，破坏严格的『同输入同输出』——靠 config_json 记录 doc_count 快照声明口径，且 doc_count<20 时完全不启用保证冷启动行为不变；跨工作区聚合 df 存在『跨项目信息以计数形式外溢』的口径问题（不泄原文，需产品文案说明）。
- **验收标准**：cargo test：(1) 导入 25 份含同一套话段的历史文档后新开比对，该套话段 boiler_fraction≥0.6、出现在 chunk_exemptions(kind='background') 且不进聚类；(2) 仅本场 3 份参评文档共享、历史库中不存在的段落不被豁免（df 扣除自身贡献的直接断言）；(3) doc_count<20 时全部既有 compare 测试不改动通过；(4) 同一文件重复导入与缓存复用路径 doc_count 不重复增长；(5) 同库同输入连续两次比对豁免集合完全一致（确定性）。

### 分区分层阈值：五区章节分类器（规则先行）+ 每区独立阈值与证据链（2.5d）

- **价值**：监管规则本身是分层的（技术文件雷同率、清单报价雷同『与已标价工程量清单雷同的除外』、法定格式天然一致），全文单一阈值注定两头失败：法定格式区全是噪声、技术区又嫌太松。分区后每类证据可直接映射法定认定条款，报告即证据。
- **设计**：把 engine/segment.rs 的三分类（tech/business/other，纯正文关键词计数）扩展为五区 classify_zone(section_path_titles, text, is_table_row)：legal（标题/正文命中 投标函|法定代表人|授权委托|承诺书|声明|资格审查|廉政 等模式）、price（标题命中 报价|清单|工程量|单价|合价，或 is_table_row 且含 amount 实体）、tech、business、other；标题路径优先于正文关键词，规则确定性、无模型。存量兼容：chunks.section_kind 列为 TEXT 直接容纳新值，旧行不动——corpus::from_row 处比对期对每块现算 zone（廉价、确定性），新导入的块由 chunker.rs make() 直接写新值。阈值应用锚定 compare_service.rs 的 effective_threshold()（79-85 行，现只做短文本上浮）：扩展为 zone 感知——legal 区 +0.12（与 SHORT_TEXT_BUMP 叠加后统一封顶 0.98）、price 区表格行维持现阈值（其证据链主体是事实冲突/金额通道而非文字雷同）、tech/other 用基础阈值；scope 过滤（run_inner 152-158 行）映射更新：business 范围包含 legal+price。证据链分区：clusters.section_kind 写 zone 值，ClusterFilter.sectionKind 管道现成，CompareSummary 增 per-zone 计数，导出按区分组并对 legal 区标注『法定格式文本，阈值已上调』。
- **改动文件**：
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/segment.rs`：Section 枚举扩为 Tech/Business/LegalFormat/Price/Other；新增 classify_zone(titles, text, is_table_row, has_amount)，标题规则表+正文关键词回退；补齐正反例单元测试
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/chunker.rs`：make()（354-402 行）改调 classify_zone，section_kind 写五区值
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/engine/corpus.rs`：from_row 对旧值（tech/business/other 且有 section_path）比对期重算 zone，字段仍存 section_kind
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/services/compare_service.rs`：effective_threshold 扩展 zone 上浮（LEGAL_ZONE_BUMP=0.12 常量入 73-87 行常量区）；scope 过滤映射 business⊇{legal,price}；CompareSummary 加 per-zone 计数
  - `/Users/zt/zt-all/bidguard-app/src/api/types.ts`：ClusterSummaryDto.sectionKind 注释扩枚举；CompareSummary 加 zone 计数字段；ClusterFilter.sectionKind 值域扩展
  - `/Users/zt/zt-all/bidguard-app/src/screens/ClustersScreen.tsx`：标段筛选项从 技术/商务 扩为 技术/商务/法定格式/报价清单/其他
  - `/Users/zt/zt-all/bidguard-app/src/screens/ClusterDetail.tsx`：zone 徽标；legal 区簇附『法定格式，阈值已上调至 xx%』说明行
  - `/Users/zt/zt-all/bidguard-app/src/screens/CompareSetup.tsx`：『比对范围』hint 更新（336-338 行），说明法定格式区独立阈值
  - `/Users/zt/zt-all/bidguard-app/src-tauri/src/export/`：HTML/DOCX 报告按五区分组小节，legal 区口径标注
- **DB 改动**：无迁移：section_kind 为 TEXT 列直接容纳 'legal'/'price' 新值；旧行由比对期重算兼容，重新导入后自然固化。
- **UI 改动**：ClustersScreen 五区筛选、ClusterDetail zone 徽标与阈值说明、CompareSetup hint、导出分区分组。
- **配置**：LEGAL_ZONE_BUMP=0.12 等常量集中在 compare_service.rs；后续如需 UI 调参再入 AppConfig.compare（本条不做）。
- **新依赖**：无
- **风险**：规则分类器对无标题结构的 txt/OCR 碎段会大量落 other，分层退化为单阈值（保守回退，可接受但要在验收中量化 other 占比）；legal 区上调阈值会同时压掉『法定格式内填空字段雷同/错误一致』这一真信号——正确解法是该区只比填空字段与共同错误，属后续共同错误指纹条目，本条先做阈值分层并在文档明示边界；zone 与既有 scope（tech/business 二分）双轨并存，UI 措辞需避免用户混淆。
- **验收标准**：cargo test：(1) classify_zone 单元测试覆盖典型样例（『投标函』标题段→legal、含金额表格行→price、微服务正文→tech、无特征→other）；(2) 端到端正负对照：同一相似度 0.75 的段对，legal 区（0.7+0.12 阈值下）不聚类、tech 区照常聚类；(3) 旧库（section_kind 仅三值）不重导入直接比对，簇产出五区值且 scope='business' 仍包含报价段（回归断言）；(4) summary 各 zone 计数之和等于 clusterCount。

**该工作流的开放问题**：

- 任务书把文档列表 UI 写作 Library.tsx，但仓库中 Library.tsx 是『查重源』样板屏，文档卡片网格实际在 CompareSetup.tsx——招标文件管理按本方案落在 CompareSetup.tsx 分组区，是否需要独立的『项目文件』屏请确认。
- 迁移编号：任务书约定 V9+，但 migrations.rs 实际已用到 V11（V9 索引清理/V10 cluster_members 索引/V11 truncation_notice），本方案从 V12 起顺延（V12 角色、V13 chunk_exemptions、V14 clusters 豁免列、V15 背景库）。
- 『原始相似度』的精确口径：建议定义为『不做招标文件对减、其余（查重源样板剔除、scope 过滤）照旧』，即两数字只差对减一个变量——若需『完全不剔除任何东西』的第三口径请指出。
- 背景 DF 库按全库（跨工作区）聚合还是按工作区隔离？跨库聚合语料更足（doc_count≥20 更易达标）但存在跨项目计数外溢；本方案默认全库聚合并在文档声明，请确认。
- 背景库与比对结果的复现性：背景库随导入增长，同一比对在不同时点结果可能不同——当前以 jobs.config_json 记录 doc_count 快照声明口径，是否需要更强的『锁定背景库版本』机制（成本明显更高）？
- 条目 3 的『多家异常一致』权重 0.30 与豁免退出矩阵的口径变化都会影响既有验收基线（calibrate_real_corpus 手动校准测试），是否安排一次真实语料前后对比作为发布门禁？
- 内置范本资产（如九部委标准施工招标文件文本，约数百 KB）是否随安装包内置进背景库种子？涉及版权与包体，请拍板。


## 6. 设计明细：W4 铁证层 + 对齐成型层

> 设计代理的工作流综述：W4 铁证层 + 对齐成型层：逐字雷同区间（后缀自动机）、seed-chain-align 段落对齐、区段内带状细化、区段口径覆盖率矩阵、区段数据模型与前端视图

### 逐字雷同区间检测（铁证层）：后缀自动机求跨文档所有长公共子串（3d）

- **价值**：现有五通道只输出「相似分数」，评标人无法直接引用。逐字区间给出带起止位置、可写进评标报告的零概率误报证据（「甲第3.2节与乙第3.2节存在800字逐字相同」），是围标认定最有说服力的文本证据，也是 W4 对齐层的高置信种子来源。对应 scheme §9.2 与 SOTA survey §3 ExactSubstr（Lee et al. ACL 2022）。
- **设计**：新建 src-tauri/src/engine/verbatim.rs。每份参评文档取 paragraph 级分块（chunk_repo 新增轻量查询 load_texts(doc_id, level)，独立于 cfg.chunk_level），按 order_index 以 '\n' 连接成全文，做「仅剥离空白」的轻归一并记录 归一下标→(chunk_id, 块内字符偏移) 的映射表。对每个跨文档对（≤C(5,2)=10 对）以较短文档建后缀自动机（SAM，HashMap 转移，每状态存一个首次出现末位置），流式匹配另一文档得到匹配统计，收集长度 ≥ verbatim_min_chars（默认 30 汉字，可配 20–40）的极大公共子串；重叠/相邻区间合并，cfg.ignore_templates 时丢弃完全落在 is_template 块内的区间。产出 VerbatimMatch{doc_a, doc_b, 两侧(start_chunk_id, 块内偏移, end_chunk_id, 块内偏移), char_len, sample_text}。编排：compare_service.rs run_inner 在阶段 4 精排后新增 progress stage "verbatim"（rayon 按 pair 并行，ctx.check() 支持取消），阶段 7 同一事务落库；run_compare 失败清理走扩展后的 delete_job_results。SAM 构建 O(n)，5 份×15 万字规模内存与耗时可控；确定性构造（无随机源），满足取证可复现要求。
- **改动文件**：
  - `src-tauri/src/engine/verbatim.rs`：新文件：SAM 构建、匹配统计、极大区间收集与合并、归一偏移映射、单元测试（跨段落长串、阈值下界、模板剔除、确定性）
  - `src-tauri/src/db/repo/chunk_repo.rs`：新增 load_texts(conn, document_id, chunk_level) -> Vec<(id, text, page, section_path, order_index, is_template)> 轻量查询
  - `src-tauri/src/db/repo/compare_repo.rs`：新增 insert_verbatim_matches / list_verbatim_for_pair；delete_job_results 增加 DELETE FROM verbatim_matches WHERE job_id=?
  - `src-tauri/src/services/compare_service.rs`：run_inner 阶段 4 后新增 verbatim 阶段（读 CompareRunConfig.verbatim_min_chars，serde default 30）；阶段 7 事务内落库
  - `src-tauri/src/db/migrations.rs`：MIGRATIONS 追加 VERBATIM_V12（见 db_changes）
- **DB 改动**：V12 迁移新表 verbatim_matches(id TEXT PK, job_id REFERENCES jobs ON DELETE CASCADE, doc_a_id, doc_b_id REFERENCES documents ON DELETE CASCADE, a_start_chunk_id, a_start_offset, a_end_chunk_id, a_end_offset, b_start_chunk_id, b_start_offset, b_end_chunk_id, b_end_offset, char_len INTEGER, sample_text TEXT, segment_id TEXT NULL, created_at)；索引 (job_id)、(job_id, doc_a_id, doc_b_id)、外键级联所需 chunk 引用不设 FK（区间锚定块，块删除随文档级联由 job 级联覆盖）。纯增表，旧工作区兼容。
- **UI 改动**：本条不做独立 UI；证据在条目 5 的区段视图中以深红高亮呈现。
- **配置**：CompareRunConfig 新增 verbatim_min_chars: usize（serde default 30）；CompareSetup 暂不暴露，走默认值。
- **新依赖**：无。SAM 手写约 250 行纯 Rust（避免引入 suffix/suffix_array crate）；并行复用 rayon。
- **风险**：① PDF 抽取的空白/换行噪声：只剥空白仍可能因全角/半角标点差异打断长串（NFKC 已在 normalized_text 做，但本层刻意用原文保证「逐字」语义），漏检部分由 n-gram/语义层兜底——需在文档中明确本层是高精度低召回。② SAM 只回报 A 侧首次出现位置，同串多处出现时只锚一个位置（发生率低，聚类层可补）。③ heading 块不在 paragraph 序列里，跨标题的长串会断成两段（可接受，区间合并容忍 ≤ 标题长度的 gap 可后续优化）。④ 极端大扫描件 OCR 全文 + sentence 粒度无关——本层固定 paragraph 级，规模有上界。
- **验收标准**：cargo test 通过以下用例：(1) 两份合成文档共享一段跨两个段落的 100 字逐字文本 → 输出恰好 1 条合并区间，两侧 chunk 锚点与块内偏移正确、char_len=100；(2) 共享 29 字（低于默认阈值）→ 0 条；(3) 同输入跑两遍，除 id/created_at 外逐字段一致（确定性）；(4) ignore_templates=true 且区间完全在 is_template 块内 → 不输出；(5) 现有 perf_smoke_three_docs_100_pages_under_60s 加入 verbatim 阶段后仍 <60s；(6) 取消/失败后 verbatim_matches 无本任务残留（复用 cancelled_compare_leaves_no_partial_results 模式）。

### seed-chain-align 段落对齐：召回命中共线链化成连续对齐区段（3.5d）

- **价值**：把「137 个散点雷同块」成型为「乙第3章 ↔ 丙第3章整体雷同（覆盖82%）」——证据形态贴近评标人心智模型，是 scheme §9.2.1 的核心结构性改进（PAN seed–extend–filter / minimap2 seed-chain-align 范式，survey §7）。同时为条目 4 的覆盖率矩阵提供无重复计数的基础。
- **设计**：新建 src-tauri/src/engine/align.rs。种子 = 阶段 4 的 ScoredEdge（chunk 对 + 各自文档内稠密行序 rank，rank 由 compare_service 重编的 order_index 直接可得）∪ 条目 1 的 verbatim 区间映射到 chunk 范围后的满分锚点（score=1.0，标 kind=verbatim）；为提升链化连续性，精排 fold 中额外保留 final_score ∈ [threshold−0.15, threshold) 的边作为「仅链化用」软种子（不入 candidate_edges 不参与聚类）。对每个文档对：按 a_rank 排序做 minimap2 式稀疏 DP 链化——chain_score = Σ(anchor_score×min_chars) − gap 代价 λ·|Δa−Δb| − μ·max(Δa,Δb)，回看窗口 h=50 控制 O(h·k)，任一侧 gap > MAX_GAP_CHUNKS(默认 8) 强制断链；贪心取最优链、剔除已用锚点迭代，直到无 ≥MIN_ANCHORS(2) 且 ≥MIN_CHARS(120) 的链。产出 AlignedSegment{doc 对、两侧 chunk rank 区间与首末 chunk_id、anchor 数、verbatim 字数、两侧 covered_chars 与 coverage=covered/区间总字数、avg_score、两侧 section_path 首项与页码范围}。编排：run_inner 阶段 5 聚类后新增 stage "align"，阶段 7 同事务落库并回填 verbatim_matches.segment_id。区段与 cluster 的关系：新证据层，不替代聚类（详见条目 5）。
- **改动文件**：
  - `src-tauri/src/engine/align.rs`：新文件：Anchor/AlignedSegment 结构、按文档对分桶、稀疏 DP 链化（gap 代价 + 回看窗口）、贪心多链提取、覆盖率计算、单元测试
  - `src-tauri/src/services/compare_service.rs`：精排 fold/reduce 增加软种子保留带（阈值−0.15）；新增 align 阶段调用 align::chain(&comparable, &edges, &soft_seeds, &verbatim)；落库与 segment_id 回填
  - `src-tauri/src/db/repo/compare_repo.rs`：新增 insert_segments / insert_segment_anchors；delete_job_results 增加 aligned_segments/segment_anchors 清理（anchors 由 FK 级联）
  - `src-tauri/src/db/migrations.rs`：MIGRATIONS 追加 SEGMENTS_V13（见 db_changes）
- **DB 改动**：V13 迁移两张新表：aligned_segments(id PK, job_id REFERENCES jobs ON DELETE CASCADE, doc_a_id, doc_b_id, a_start_order, a_end_order, b_start_order, b_end_order, a_start_chunk_id, a_end_chunk_id, b_start_chunk_id, b_end_chunk_id, anchor_count INTEGER, verbatim_chars INTEGER, a_covered_chars, b_covered_chars, a_coverage REAL, b_coverage REAL, avg_score REAL, a_section_path TEXT, b_section_path TEXT, a_page_start, a_page_end, b_page_start, b_page_end, created_at)，索引 (job_id, doc_a_id, doc_b_id)；segment_anchors(segment_id REFERENCES aligned_segments ON DELETE CASCADE, a_chunk_id, b_chunk_id, kind TEXT/*edge|soft|verbatim*/, score REAL, PRIMARY KEY(segment_id, a_chunk_id, b_chunk_id))，索引 (a_chunk_id)、(b_chunk_id)（供与 cluster_members 按 chunk 互查）。纯增表，向后兼容。
- **UI 改动**：无（前端在条目 5）。
- **配置**：CompareRunConfig 新增 enable_alignment: bool（serde default true）；链化常数（MAX_GAP_CHUNKS/MIN_ANCHORS/λ/μ）作为 align.rs 顶部集中常量，暂不进配置面板。
- **新依赖**：无。
- **风险**：① sentence 粒度比对时种子密度暴涨、链化产出的区段过碎或过大——首版对 cfg.chunk_level=="sentence" 的运行按句锚点链化但 MIN_CHARS 提高一档，效果需真实语料回归；② gap 代价常数未经校准（与 scheme §8.1 同类问题），初值凭 minimap2 类比，等 W-校准语料工作流回测；③ 贪心多链提取可能在 B 侧产生重叠区段（A 侧因剔除已用锚点不重叠），覆盖率矩阵侧需按 chunk 去重（条目 4 已处理）；④ 软种子保留带增加内存（估 <20% 边量），大语料需观察。
- **验收标准**：cargo test：(1) 合成文档对——连续 10 个相似段落 + 相距 20 段的 2 个孤立命中 → 恰好 1 条 ≥10 锚点区段覆盖连续段，孤立命中不并入（gap>8 断链）；(2) 区段 coverage 与手算值差 <0.01；(3) 两文档段序整体平移（B 比 A 后移 5 段）仍成单链（共线性按相对序）；(4) verbatim 锚点存在时 kind=verbatim 且 verbatim_chars 累计正确；(5) delete_job_results 后两表无残留；(6) 同输入两遍输出确定性一致；(7) perf smoke 全管线仍 <60s。

### 区段内带状字符级对齐细化（扩展分级 diff）（2.5d）

- **价值**：链化区段的锚点之间存在「未被任何边命中」的 gap 块（洗稿插入句、小改段）。带状句级对齐 + 现有 char_diff 细化把这些 gap 变成可高亮的精确证据，让区段覆盖率从「锚点覆盖」升级为「细化后真实覆盖」，直接决定区段视图红黄高亮质量与矩阵口径准确度。对应 survey §7 带状 Smith-Waterman 的工程化替身（句级带状 DP + 字符级细化，复用 similar crate 与 diff.rs 既有分级）。
- **设计**：扩展 src-tauri/src/engine/diff.rs：新增 pub fn banded_gap_diff(jieba, a_sents: &[&str], b_sents: &[&str], band: usize) -> Vec<DiffOp> 与入口 refine_segment_gaps()。对每个区段相邻锚点之间的 gap（两侧各为一段连续未匹配 chunk 的文本）：用现有 split_sentences 切句，句间相似用 char_ngrams Jaccard（features.rs 已有）打分，做带宽 band=|la−lb|+8 的单调 Needleman-Wunsch（替换代价 1−sim、sim≥0.4 才允许配对），配对句再走现有 char_diff 细化，未配对句记 ins/del——即「句级带状对齐 + 字符级细化」，是 sentence_diff 的推广（sentence_diff 只细化相邻删改 run，无法处理错位/一对多）。细化产物：每 gap 一条 DiffOp 序列 + eq 字符数；compare_service 在 align 阶段后用 eq 字符数回填 aligned_segments 的 a/b_covered_chars 与 coverage（细化后口径），DiffOp 落 segment_diffs 表供前端渲染。gap 文本超长（>4000 字任一侧）时降级为整段 sentence_diff 防带状 DP 内存峰值。
- **改动文件**：
  - `src-tauri/src/engine/diff.rs`：新增 banded_gap_diff（带状句级 DP + char_diff 细化）与 refine_segment_gaps；split_sentences 提为 pub(crate)；单元测试（错位句、一改多、还原性、带宽边界）
  - `src-tauri/src/engine/align.rs`：AlignedSegment 增加 gaps: Vec<GapPair{a_chunk_ids, b_chunk_ids}> 供细化定位；coverage 字段改为细化后回填
  - `src-tauri/src/services/compare_service.rs`：align 阶段内对每区段 rayon 并行调用 refine_segment_gaps，回填覆盖率，组装 segment_diffs 行
  - `src-tauri/src/db/repo/compare_repo.rs`：新增 insert_segment_diffs / list_segment_diffs；delete_job_results 无需改（随 segment FK 级联）
  - `src-tauri/src/db/migrations.rs`：MIGRATIONS 追加 SEGMENT_DIFFS_V14：segment_diffs(id PK, segment_id REFERENCES aligned_segments ON DELETE CASCADE, a_chunk_id TEXT, b_chunk_id TEXT, diff_type TEXT/*gap-sentence|gap-char*/, diff_json TEXT NOT NULL, eq_chars INTEGER, created_at)，索引 (segment_id)
- **DB 改动**：V14 迁移新表 segment_diffs（结构见上）。不复用既有 diffs 表：其 cluster_id NOT NULL 且语义（底版 vs 目标）与 gap 对齐（双侧对称）不同，硬塞会破坏 ClusterDetail 的 diff 索引契约。
- **UI 改动**：无独立 UI；DiffOp 序列由条目 5 的区段详情用既有 diff 渲染逻辑（ClusterDetail.tsx 的 diffOfChunk 渲染模式）消费。
- **配置**：无新用户配置；带宽与 sim 门槛为 diff.rs 集中常量。
- **新依赖**：无（similar、jieba-rs、rayon 均已有）。
- **风险**：① 句级带状 DP 假设 gap 内句序单调——句序被打乱的洗稿会配不上（记 ins/del，覆盖率偏低而非误报，方向安全）；② sim≥0.4 的配对门槛偏松可能把无关句配成「替换」产生噪声高亮，需用小语料肉眼校验一轮；③ eq_chars 回填让 coverage 语义变化，条目 4 的矩阵与本条有落地顺序耦合（先 2 后 3 再 4，或 4 先用锚点覆盖口径再切换）。
- **验收标准**：cargo test：(1) gap 两侧 5 句 vs 6 句、中间插入 1 新句 + 1 句小改 → 新句整句 ins、小改句产生字符级 del/ins、其余句 eq；ops 过滤 ins 还原 A 侧、过滤 del 还原 B 侧（沿用 diff.rs 既有 join 断言模式）；(2) 两侧各 200 句的病态 gap 在 debug 模式 <1s 完成（带状约束生效）；(3) 细化后 segment.a_coverage ≥ 锚点口径值且 ≤1.0；(4) 全空 gap（锚点相邻）不产生 segment_diffs 行；(5) perf smoke 仍 <60s。

### 覆盖率矩阵升级：基于对齐区段的口径 + 旧口径开关（1.5d）

- **价值**：现行 matrix.rs 按 cluster primary 对累加 score×min_chars，散点重复与 primary 选择偏差会系统性抬高/漂移 sim 值；围标信号 ①（峰值 ≥0.6 起算）直接消费该值，口径失真即分级失真。区段口径按 chunk 去重后的真实覆盖字数计算，可解释为「较小文档被对齐区段覆盖的比例」，且与区段视图展示的数字一致（用户看到的 82% 就是矩阵里的 82%）。
- **设计**：matrix.rs 新增 pub fn doc_matrix_segments(n_docs, chunks, segments: &[AlignedSegment]) -> (Vec<Vec<f32>>, f32)：对每个文档对，取该对全部区段，两侧各用 HashSet<u32>（chunk 下标）对被覆盖 chunk 去重（防贪心多链在 B 侧重叠重复计数），matched = Σ(去重后 chunk 的 char_count × 该 chunk 所属区段 avg_score，细化后口径用 covered_chars 直接累加)，sim = matched / min(totalA, totalB)，总字数分母沿用现函数的 comparable 口径。compare_service 阶段 8 同时算两个矩阵：legacy = matrix::doc_matrix(...)（原函数不动），segment 版新函数；matrix_json 结构扩展为 {documentIds, matrix, peak, segmentMatrix, segmentPeak, mode}，mode 取 CompareRunConfig.matrix_mode（"segment" 默认 | "cluster" 旧口径），前端与围标信号 ① 按 mode 选用——首版围标继续吃 legacy peak（避免未校准的口径切换牵动分级线），仅展示层切换，待校准语料工作流回测后再切围标输入。
- **改动文件**：
  - `src-tauri/src/engine/matrix.rs`：新增 doc_matrix_segments（chunk 去重 + 覆盖字数累加）；原 doc_matrix 保持不动；单元测试（重叠区段去重、与手算值一致、空区段回退 0）
  - `src-tauri/src/services/compare_service.rs`：阶段 8 双矩阵计算；matrix_json 增 segmentMatrix/segmentPeak/mode 字段；CompareRunConfig 增 matrix_mode（serde default "segment"）；collusion::assess_with 继续传 legacy peak 并加注释说明切换条件
  - `src/screens/Matrix.tsx`：fromSummary() 读 segmentMatrix/mode；矩阵卡片头部加「区段口径/聚类口径」Pill 切换（本地 state，默认随 mode），BigMatrix 复用
  - `src/api/types.ts`：CompareSummaryDto.matrix 类型扩展 segmentMatrix?: number[][]; segmentPeak?: number; mode?: string（可选字段，旧任务 JSON 无此键仍可渲染）
- **DB 改动**：无新表；jobs.matrix_json 内 JSON 结构扩展（增可选键，旧行无键按 cluster 口径渲染，向后兼容）。
- **UI 改动**：Matrix.tsx 热力矩阵加口径切换 Pill；单元格 tooltip 显示两口径数值对照（差异 >10 个百分点时标注，帮助用户理解口径变更）。
- **配置**：CompareRunConfig.matrix_mode: String（serde default "segment"）；CompareSetup.tsx 高级选项暂不暴露，Settings 不动——开关先服务于回归对比与导出复现。
- **新依赖**：无。
- **风险**：① 两口径数值差异可能让老用户困惑（同一工作区重跑后峰值变了）——tooltip 对照 + 导出报告标注口径名缓解；② 围标信号 ① 暂不切换意味着展示值与判定输入值短期不一致，需在矩阵页围标卡片注明「判定基于聚类口径」；③ 区段口径依赖条目 2/3 质量，若链化漏配则 sim 系统性偏低（漏报方向），需与 legacy 口径回归对比把关；④ calibrate_real_corpus 门禁测试断言的是 legacy 值，需确认双矩阵改动不触碰其断言路径。
- **验收标准**：cargo test：(1) 构造同一 chunk 被两条区段覆盖的场景 → segment 口径该 chunk 只计一次，sim 等于手算值（±1e-4）；(2) matrix_mode="cluster" 时 matrix_json.matrix 与改动前基线逐字节一致（用 calibrate_real_corpus 语料快照对比）；(3) 无区段（enable_alignment=false）时 segmentMatrix 全 0 且前端回退 cluster 口径不报错；(4) 前端 npm run build + 现有 vitest 通过，Matrix.tsx 对旧任务（matrix_json 无新键）渲染不抛异常（组件测试补一条）。

### 对齐区段数据通路与前端区段视图（新证据层，与聚类并存）（4d）

- **价值**：把条目 1–3 的产物送到评标人面前：矩阵单元格点进去看到「甲 3.2 施工组织 ↔ 乙 3.2 施工组织 · 覆盖 82% · 锚点 14 · 逐字 620 字」的区段列表与并排高亮详情。明确架构决策：区段是新增证据层而非替代聚类——聚类承载八类分类、事实冲突、人工三态复核、批注与围标信号②，是多文档粒度；区段是文档对粒度的证据成型，两者经 chunk_id 互链，各司其职，不推倒重来。
- **设计**：后端：新建 src-tauri/src/db/repo/segment_repo.rs——list_segments(job_id, doc_a?, doc_b?)（按 verbatim_chars DESC, a_covered_chars DESC 排序）与 get_segment_detail(segment_id)（区段行 + anchors JOIN chunks 取文本/页码 + verbatim_matches + segment_diffs，并经 segment_anchors.a_chunk_id/b_chunk_id JOIN cluster_members 反查关联 cluster_id 集合）；commands/compare.rs 增 list_aligned_segments / get_segment_detail 两个 #[tauri::command]，lib.rs 注册。前端：types.ts 增 AlignedSegmentDto / SegmentDetailDto / VerbatimIntervalDto；queries/data.ts 增 useAlignedSegments(jobId, docA, docB) 与 useSegmentDetail(segmentId)；新屏 src/screens/PairSegments.tsx——上半部文档对选择（十天干 docTag 复用）+ 区段卡片列表（两侧章节路径、页码范围、覆盖率双向条形、锚点数、逐字字数），点击展开详情：双栏按 chunk 顺序同步滚动渲染，逐字区间深红底、锚点命中块橙色边、gap 细化 DiffOp 复用 ClusterDetail 的 diff 渲染样式，块级「查看所属条款」链接跳 ClusterDetail；入口：Matrix.tsx BigMatrix onCell 从跳 compare 改为跳 PairSegments(docA, docB)，ClustersScreen 顶部加「对齐区段」页签入口；ClusterDetail.tsx 成员卡片若其 chunk 命中某区段则显示「所在区段 · 覆盖 82%」Pill 反向跳转。
- **改动文件**：
  - `src-tauri/src/db/repo/segment_repo.rs`：新文件：SegmentSummaryRow/SegmentDetail 查询（含 anchors、verbatim、segment_diffs、关联 cluster_ids 反查）
  - `src-tauri/src/db/repo/mod.rs`：注册 segment_repo 模块
  - `src-tauri/src/commands/compare.rs`：新增 list_aligned_segments / get_segment_detail 命令（沿用 conn(&state) 模式）
  - `src-tauri/src/lib.rs`：invoke_handler 注册两个新命令
  - `src/api/types.ts`：新增 AlignedSegmentDto、SegmentDetailDto、SegmentAnchorDto、VerbatimIntervalDto、SegmentGapDiffDto（camelCase 与 serde 对齐）
  - `src/queries/data.ts`：新增 useAlignedSegments / useSegmentDetail hooks（沿用既有 invoke + react-query 模式）
  - `src/screens/PairSegments.tsx`：新屏：文档对选择、区段卡片列表、双栏区段详情（逐字/锚点/gap 三级高亮）、跳转 ClusterDetail
  - `src/screens/Matrix.tsx`：BigMatrix onCell 携带 (docA, docB) 跳转 PairSegments
  - `src/screens/ClusterDetail.tsx`：成员卡片增「所在区段」Pill（由 detail 接口新增的 segment 归属字段驱动，可选字段向后兼容）
  - `src/App.tsx（或路由定义处）`：注册 /ws/:wsId/job/:jobId/segments 路由（按仓库实际路由文件锚定）
- **DB 改动**：无新迁移（消费 V12–V14 的表）；get_cluster_detail 查询扩展一个可选的 segment 归属子查询（无 schema 变更）。
- **UI 改动**：新屏 PairSegments（区段列表 + 双栏高亮详情）；Matrix 单元格入口改跳区段视图；ClusterDetail 增区段归属 Pill 反向互链。深红=逐字铁证、橙=锚点雷同、黄=gap 细化差异的三级视觉语义在图例中说明。
- **配置**：无。
- **新依赖**：无（React 侧全部复用 primitives/Topbar/Pill/docTag/clusterUi 既有组件与工具）。
- **风险**：① 双栏同步滚动 + 长区段（数百 chunk）渲染性能，需虚拟化或分页（首版限制单区段渲染前 200 块 + 「展开更多」）；② 区段与聚类双入口可能造成复核动线分裂——复核三态仍只挂在 cluster 上，区段视图只读 + 跳转，避免两套状态；③ 导出六格式暂不包含区段（本条不扩导出，避免范围膨胀），报告与屏幕内容短期不一致，列为后续任务；④ ClusterDetail 归属字段扩展需保证旧任务（无区段数据）返回空数组不破坏渲染。
- **验收标准**：(1) cargo test：fixture 语料跑完整 compare 后 list_segments 返回非空、get_segment_detail 的 anchors/verbatim/gap diffs 与落库行数一致、关联 cluster_ids 可反查到既有 cluster；(2) V11 旧库升级到 V14 后打开旧工作区、旧任务全部屏幕无报错（migrations 幂等测试扩展断言三张新表存在）；(3) npm run build 与 tsc --noEmit 通过；(4) vitest 组件测试：PairSegments 对 mock 数据渲染出「覆盖 82%」格式文案、空区段列表渲染空态、旧任务（hooks 返回空）不抛异常；(5) Matrix 单元格点击导航到带正确 docA/docB 参数的路由（测试导航回调）。

**该工作流的开放问题**：

- 逐字层的文本基准：当前设计用「原文仅剥空白」保证严格的『逐字』举证语义（PDF 版面空白噪声容忍），是否需要再提供一档「NFKC 归一后逐字」模式（召回更高但报告措辞需改为『归一化后逐字一致』）？
- verbatim_min_chars（默认 30 汉字）与链化常数（MAX_GAP_CHUNKS=8、软种子带宽 0.15）是否要在 CompareSetup 高级选项暴露，还是等合成语料校准工作流（scheme §9.3）回测后固化？
- 落地顺序建议 1→2→3→4→5（矩阵条目依赖细化后覆盖口径），若需提前交付矩阵，条目 4 是否接受先按『锚点覆盖』口径上线、条目 3 合入后自动升级口径？
- 围标信号 ①（相似峰值）何时从聚类口径切换到区段口径峰值——是否作为校准工作流（W-校准）的输入一并回测，本工作流仅保留展示层切换？
- sentence 粒度比对下区段链化的 MIN_CHARS/MIN_ANCHORS 是否需要独立参数组（句锚点密度远高于段落），还是首版直接限定区段功能仅在 paragraph/section 粒度启用？
- 导出六格式（src-tauri/src/export/）何时纳入区段与逐字证据章节——建议作为独立后续任务（估 2 天），否则报告与屏幕证据不一致。


## 7. 设计明细：W5 商务标数值层

> 设计代理的工作流综述：W5 商务标数值层：报价清单结构化比对、机制感知筛查与围标信号接入（BOQ numeric evidence layer）

### W5-1 报价清单识别与跨文档行对齐（engine/boq.rs + V12 boq_items 表）（3d）

- **价值**：把商务标从『文本像』升级到『数值证据』的地基：所有后续雷同率/规律性/相关性/机制筛查都依赖对齐后的单价矩阵。表格行数据基础现成（xlsx 已由 calamine 逐行原子化、docx 表格行以 ' | ' 连接，chunk_type='table_row'），只差表头语义识别与跨文档对齐这一层。对齐率本身也是证据——连非标措施项拆分方式都一致即『同一单位编制』的结构性信号（SOTA 调研 §11 实体匹配条目）。
- **设计**：新建 src-tauri/src/engine/boq.rs。输入：按文档用 chunk_repo 新查询 load_table_rows(document_id)（chunk_type='table_row' AND chunk_level='paragraph'，按 order_index 排序，带 id/text/page/section_path），不走 cfg.chunk_level 与 scope 过滤，保证技术标比对时数值层仍可运行。表检测：按 order_index 邻接分组成表，对每行按 ' | ' 拆列，用同义词典把单元格映射到规范列（code：项目编码/清单编码/子目编码/定额编号；name：项目名称/名称；unit：单位/计量单位；qty：工程量/数量；unit_price：综合单价/单价/单价（元）；total：合价/金额/合计），命中 ≥3 个规范列且含 unit_price 或 total 即判定表头并锁定列序；支持两行复合表头合并。数据行解析出 BoqItem{code,name,unit,qty,unit_price,total_price,chunk_id,page}，数字解析剥 ¥/千分位/『元』尾缀（fmt_cell 已保证 xlsx 数字干净，docx 单元格复用 features.rs canon_amount 的归一逻辑）。跨文档对齐三层：12 位编码精确 → 前 9 位（GB50500 清单项目码）→ 无码/失配行按『单位相等 + 名称 jieba token Jaccard ≥0.6』贪心 1:1 召回；产出 AlignedItem{key, per_doc: Vec<Option<ItemRef>>}。集成点：compare_service.rs run_inner 在 6.5 事实冲突之后新增 'boq' 阶段（ctx.progress("boq", …)），结果经 V12 新表 boq_items 落库（job 作用域、含 chunk_id 供下钻举证）。
- **改动文件**：
  - `src-tauri/src/engine/boq.rs`：新建：表头同义词典、清单表检测、BoqItem 行解析、编码/名称三层跨文档对齐；纯函数 + 单测
  - `src-tauri/src/db/repo/chunk_repo.rs`：新增 load_table_rows(conn, document_id)：按 chunk_type='table_row'、chunk_level='paragraph' 有序加载
  - `src-tauri/src/db/repo/boq_repo.rs`：新建：insert_items(tx, job_id, items) / list_by_job；delete 随 jobs 级联
  - `src-tauri/src/services/compare_service.rs`：run_inner 新增 boq 阶段：加载各文档表格行 → boq::extract+align → 与阶段 7 同一事务落库；delete_job_results 级联覆盖由外键保证
  - `src-tauri/src/engine/mod.rs`：注册 boq 模块
- **DB 改动**：V12 迁移（migrations.rs MIGRATIONS 追加第 12 项，注意当前实际已到 V11 而非文档所称 V8）：CREATE TABLE boq_items(id TEXT PRIMARY KEY, job_id TEXT NOT NULL REFERENCES jobs(id) ON DELETE CASCADE, doc_index INTEGER NOT NULL, document_id TEXT NOT NULL, chunk_id TEXT NOT NULL, align_key TEXT, code TEXT, name TEXT, unit TEXT, qty REAL, unit_price REAL, total_price REAL, flags TEXT, created_at TEXT NOT NULL) + CREATE INDEX idx_boq_items_job ON boq_items(job_id)。纯新表，向后兼容。
- **UI 改动**：本条无 UI（数据层）；chunk_id 锚点保证后续 UI 可跳转 DocPreview 原文。
- **配置**：CompareRunConfig / commands/compare.rs CompareRequest / src/api/types.ts CompareRequest 增 enableNumeric?: boolean（默认 true），走既有『内置<用户全局<工作区<请求』四层合并（commands/mod.rs effective_config）。
- **新依赖**：无。表头识别是词典规则，数字解析复用现有 normalize/features 基建。
- **风险**：表头变体超出词典（措施项目表/主材表/暂估价表列名各异）→ 词典需按真实标书迭代，未识别的表静默跳过并在 flags 记原因；docx 合并单元格被拍平会错列 → 表头列数与数据行列数不一致时该表降级不解析；扫描件 PDF 走 OCR 不产表格行块，本条明确不覆盖（升级路径为 PP-StructureV3 类表格结构识别，另立条目）；序号列被误认编码 → 编码列要求 ≥9 位数字占多数行。
- **验收标准**：cargo test 通过新增单测：(1) 手造 xlsx（复用 write_min_xlsx 模式）含『项目编码|项目名称|单位|工程量|综合单价|合价』表头 → 全部数据行解析出 6 字段；(2) 表头同义变体（清单编码/金额）同样识别；(3) 三文档同编码 → 对齐条目数=行数，缺编码行凭名称+单位对齐；(4) 非清单表（无单价列）不产出条目；(5) e2e：run_compare 后 boq_items 表有行、chunk_id 能 JOIN 回 chunks 取回原文；(6) 空库升级到 V12 幂等（migrations 既有测试模式）。

### W5-2 逐项雷同率矩阵 + 相同算术错误检测（法定 80% 线）（2d）

- **价值**：直接对应中国监管的法定认定口径：青岛/贵州等地把『相同内容达 80%』作为雷同认定线，实案（中交二公局案）以清单雷同率 90.48% 定案罚款 202 万——这是任何 embedding 方法替代不了的可立案证据。『工程量×单价≈合价 错且错得一样』= 同一张源表的最强证据（SOTA §11 首条）。
- **设计**：boq.rs 增 pair_stats(aligned, n_docs)：对每个文档对 (a,b)，可比条目 = 双方均有 unit_price 的对齐项（剔除名称含『暂估』或标记为暂估价/信息价的行——招标人给定单价本来就相同）；相同 = 单价按分（×100 四舍五入）相等；identical_rate = 相同数/可比数，可比数 <10 时不出结论只出原因。算术校验逐行：qty/unit_price/total 齐备时 err = total − qty×unit_price，|err| > max(0.01, 0.005×|total|) 记算术错误；同一对齐项在两文档中 qty、unit_price、错误 total 三者到分全等 → shared_arith_error（逐条列出并携带双方 chunk_id）。结果写入 jobs 新列 numeric_json（结构 pairs:[{a,b,comparable,identical,identicalRate,alarm,sharedArithErrors:[…]}]），经 job_repo::set_compare_results 扩一参数落库。阈值 identical_rate_alarm 默认 0.80 可配。
- **改动文件**：
  - `src-tauri/src/engine/boq.rs`：增 pair_stats：可比分母规则、按分相等判定、行内代数校验、共享算术错误配对
  - `src-tauri/src/services/compare_service.rs`：boq 阶段后计算 pair_stats → 组装 numeric_json，随阶段 8 一起 set_compare_results 落库
  - `src-tauri/src/db/repo/job_repo.rs`：set_compare_results 增 numeric_json 参数与列写入；读侧 compare summary 带出
  - `src-tauri/src/commands/compare.rs`：CompareRequest 增 identicalRateAlarm 合并入 CompareRunConfig（clamp 0.5..1.0）
- **DB 改动**：V13 迁移：ALTER TABLE jobs ADD COLUMN numeric_json TEXT;（旧行 NULL，前端按缺省隐藏面板，向后兼容）。
- **UI 改动**：本条只保证 CompareSummaryDto 透出 numeric 字段（src/api/types.ts CompareSummaryDto 增 numeric: Record<string,unknown> | null）；可视化在 W5-4。
- **配置**：compare 配置族增 identical_rate_alarm 默认 0.80；写入 jobs.config_json 快照保证报告可复现。
- **新依赖**：无。
- **风险**：计价软件舍入惯例差异会制造假算术错误 → 容差带（绝对 1 分 + 相对 0.5%）且『相同算术错误』要求错得逐分一致；可比条目过少时 80% 线无意义 → 强制最小分母并在输出中说明；法定 80% 口径原文针对『电子投标文件相同内容』整体而非单价占比 → 报告措辞写『逐项单价雷同率（参照地方雷同认定口径）』避免越权定性。
- **验收标准**：单测：(1) 10 项中 8 项单价相同 → identicalRate=0.8 且 alarm=true（默认阈值）；(2) 暂估价行不进分母；(3) qty=100、price=25.5、两文档 total 均错为 2505 → sharedArithErrors 命中且含双方 chunk_id；两文档错得不同 → 不命中；(4) 可比数 9 → 不出 rate、原因=insufficient；e2e：两份 xlsx 清单跑 run_compare 后 jobs.numeric_json 含预期 rate；V13 升级幂等。

### W5-3 规律性差异检测：等差/等比/恒定折扣 + 尾数聚集 + 首位卡方（2d）

- **价值**：《招标投标法实施条例》第 40 条法定情形『不同投标人的投标报价呈规律性差异』的机器化：同一张成本表乘系数（等比）、加常数（等差）在单价向量层留下 R²≈1 的线性指纹，人工肉眼难查但代数上无所遁形。数字分布检验零成本（纯统计、无需训练数据），对『一人手工编多份标』最敏感（SOTA §11 TAB/Benford 条目）。
- **设计**：boq.rs（或拆 boq_stats 子模块）对每对齐向量 (x,y)（n≥10，剔除双方相等项后仍 n≥10 才判规律性，避免大面积雷同项掩盖）：最小二乘拟合 y=a·x+b 取 R²；R²≥0.999 且非恒等时分类——|a−1|<1e-6 且 |b|>ε → 等差（差额 b）；|b|<ε 且 a≠1 → 等比/恒定折扣（系数 a）；否则仿射规律。辅证：比值向量 CV<0.5% 佐证等比、差值向量极差<1 分佐证等差。单文档数字检验：全部单价的尾数（分位/角位）分布 χ² 均匀性检验 + 0/5 尾占比；首位数字对 Benford 期望 χ²（n≥30 才检，df=8 临界值硬编码）；多文档共享同一异常数字指纹 → 升级为串通线索。结果并入 numeric_json（pairs[].pattern:{kind,a,b,r2} 与 docs[].digitStats）。全部纯 Rust 手写，无新依赖。
- **改动文件**：
  - `src-tauri/src/engine/boq.rs`：增 regularity_of(x,y)（OLS+R²+分类+比值/差值佐证）与 digit_stats(prices)（尾数/首位 χ²）；确定性浮点实现附单测
  - `src-tauri/src/services/compare_service.rs`：boq 阶段调用并把 pattern/digitStats 并入 numeric_json
- **DB 改动**：无（复用 W5-2 的 jobs.numeric_json）。
- **UI 改动**：无独立 UI（W5-4 面板展示 pattern 标签与 digitStats 摘要）。
- **配置**：无新配置（R²/χ² 阈值作为 boq.rs 内常量集中放置，与 collusion.rs 权重常量同风格便于校准）。
- **新依赖**：无。OLS/χ² 均为几十行手写实现，临界值查表硬编码。
- **风险**：对控制价统一下浮是普遍且合法的报价策略，也会呈现等比指纹 → 输出定位为『线索』且文案强制附『可能源于对同一控制价/定额库的统一下浮，需结合取证类证据』；n 小导致伪规律 → n≥10 + R²≥0.999 双门槛；单价通常只跨 2-3 个数量级，Benford 前提偏弱 → 首位卡方只做弱信号、在 W5-6 中权重最低；浮点确定性 → 固定求和顺序保证同输入同输出（可复现验收指标）。
- **验收标准**：单测：(1) y=0.97x（含噪声 <1e-9）→ kind=geo_discount、a≈0.97；(2) y=x+500 → arith_seq、b≈500；(3) 随机扰动 5% → 无 pattern；(4) 全 0 尾数 40 项 → 尾数聚集命中；均匀随机尾数 → 不命中；(5) 剔除相等项后 n<10 → 不判规律性；全部断言确定性通过（连跑两次输出逐字节一致）。

### W5-4 单价向量相关性 + 归一化散点图（前端对角线图证据面板）（3d）

- **价值**：Pearson>0.99 + 比值 CV≈0 是『同一张成本表乘系数』的量化表达，比现行『总价差<3%』强得多（scheme §9.1.3 已列为 v2 硬证据）。归一化散点图是评标专家一眼能懂的证据形态：完全雷同=点全落对角线，恒定折扣=平行于对角线的直线带（SOTA §11 CNN screens 条目指出该图本身就是最直观证据，可不上模型先上图）。
- **设计**：后端：boq.rs 增 correlation(x,y)（Pearson + Spearman，秩计算带并列均秩），n≥10 才出值；散点数据按项计算全体投标人中位价 median_k，点 = (price_a/median_k, price_b/median_k)，裁剪至 [0,3]、每对下采样至 ≤2000 点，携带 alignKey/name 供悬停提示；并入 numeric_json（pairs[].correlation 与 pairs[].scatter）。前端：Matrix.tsx 所在结果页新增『商务标数值』面板（或 src/screens/BusinessNumeric.tsx 独立页）：雷同率两两热力表（复用 Matrix 热力样式）、文档对选择器、纯 SVG 散点图（对角线参考线 + 折扣带高亮，不引图表库）、相同项/共享算术错误明细表，行点击经既有路由跳 DocPreview 对应 chunk。types.ts 定义 NumericDto/PairNumericDto，src/queries/data.ts 增查询（数据随 CompareSummaryDto.numeric 一次带出，无新 command）。
- **改动文件**：
  - `src-tauri/src/engine/boq.rs`：增 correlation(x,y) 与 scatter_points(aligned, a, b)（中位价归一 + 裁剪 + 下采样）
  - `src-tauri/src/services/compare_service.rs`：numeric_json 并入 correlation/scatter 字段
  - `src/api/types.ts`：新增 NumericDto、PairNumericDto、BoqScatterPoint 等 DTO；CompareSummaryDto 增 numeric 字段
  - `src/queries/data.ts`：compare summary 查询透出 numeric；聚合派生（最大雷同率对）供徽标展示
  - `src/screens/BusinessNumeric.tsx`：新建数值证据面板：雷同率热力表 + SVG 散点对角线图 + 明细下钻表
  - `src/screens/Matrix.tsx`：入口挂接：数值面板 tab/跳转 + 雷同率峰值徽标
- **DB 改动**：无（复用 numeric_json）。
- **UI 改动**：新增『商务标数值』证据面板（如上）；散点图为纯 SVG + CSS，不引入图表库；空态（无清单表/可比项不足）给明确原因文案。
- **配置**：无。
- **新依赖**：无（散点图手写 SVG，前端零新依赖）。
- **风险**：投标人单价天然同源（同一定额库/信息价）导致 r 普遍 0.9+ → 面板必须同时展示比值 CV 与散点形态，文案写明只有 r>0.99 且比值 CV≈0 才是强证据，避免评标人误读；5 文档 10 对 × 2000 点的渲染压力 → 下采样 + 仅渲染选中对；Spearman 并列秩实现易错 → 与已知参考值对拍单测。
- **验收标准**：后端单测：Pearson/Spearman 对拍手算参考值（含并列秩用例）误差 <1e-9；scatter 点数 = min(可比数, 2000) 且坐标在 [0,3]。前端 Vitest：给定 numeric fixture 面板渲染出 N×N 热力表与散点 SVG（对角线元素存在）；空 numeric → 显示空态原因。e2e：xlsx 双文档比对后 UI 可见散点面板，点击明细行跳转 DocPreview 并高亮目标 chunk。

### W5-5 机制感知筛查：评标办法配置 + 反事实基准价重算（C&D 单场退化版）（4d）

- **价值**：SOTA 调研范式级发现之二的落地：均值基准价机制下串标最优策略是投极端陪衬价拉基准，报价方差反而变大，经典 CV 筛查会反向误导（意大利均值拍卖上 F1 仅 0.57 vs 第一价格 0.99）。反事实重算『剔除嫌疑组后基准价移动多少、中标人是否翻转』是单项目 2-5 份标书即可运行、可直接引用的法务级证据（Conley & Decarolis 2016，都灵法院已判案验证）。也是 195 号文场景 17『商务标报价特征比对』的差异化能力。
- **设计**：配置：CompareRequest/CompareRunConfig 增 evaluation: Option<EvaluationConfig>{method: "lowest"|"avg_benchmark", trim_lowest, trim_highest, coeff_min, coeff_max}（v1 只支持『(去 m 高 n 低后)算术平均 × 系数，最接近基准价者价格分最高』一族公式；不匹配则明确输出『不适用』）。投标总价锚定：优先 BOQ Σ合价或含『投标总价/投标报价』行的合价（新 total_price_of(doc) 函数），回落到现行 price_proximity 的排除法最大金额（compare_service.rs:676，PRICE_EXCLUDE 列表），来源打标以便举证。嫌疑组生成：枚举 |g|∈{2,3} 的文档子集，组内每对需已有文档证据（文本相似峰值 ≥0.6、或 W5-2 雷同率 ≥ 告警线、或元数据同源 risk_flags 共现）——即 C&D 的第一种候选组构造法。反事实：在系数区间均匀取 ≥200 个格点，逐格点算全量基准价 vs 剔除 g 后基准价及各自中标人；输出 flip_prob（翻转格点占比）、基准价平均偏移百分比；n≤5 时对『随机剔除同规模子集』做精确穷举（≤10 个子集）得偏移分位数（蒙特卡洛的退化精确版）；support-bid 形态标记：g 内报价处于分布端点且与次邻报价断崖（间距 >2×中位间距）。method=lowest 分支只做『最低价孤立度』启发式（最低与次低断崖标记），且禁用均值类统计——机制分流写死在路由层。结果并入 numeric_json.mechanism，措辞遵循『不替代评标人判断』。
- **改动文件**：
  - `src-tauri/src/engine/boq.rs`：增 total_price_of(items)（Σ合价/总价行识别）
  - `src-tauri/src/engine/mechanism.rs`：新建：EvaluationConfig、基准价公式实现（截尾均值×系数）、嫌疑组枚举、系数格点反事实重算、精确穷举分位数、support-bid 形态标记；全部纯函数
  - `src-tauri/src/services/compare_service.rs`：阶段 8 前：由文本/元数据/W5-2 证据构造候选组 → mechanism::run → 并入 numeric_json.mechanism；总价锚定优先 BOQ、回落 price_proximity 启发式
  - `src-tauri/src/commands/compare.rs`：CompareRequest 增 evaluation 字段校验（trim 之和 < 文档数、系数区间合法）并入 CompareRunConfig
  - `src/api/types.ts`：CompareRequest 增 evaluation；MechanismResultDto 定义
  - `src/screens/CompareSetup.tsx`：新增折叠区『评标办法（可选）』：办法单选 + 去高/去低数 + 系数区间输入，带默认值与帮助文案
- **DB 改动**：无新表（evaluation 随 jobs.config_json 快照持久化，结果入 numeric_json.mechanism）。
- **UI 改动**：CompareSetup 增评标办法配置折叠区；W5-4 数值面板增机制块：基准价、各组 flip_prob/偏移分位、support-bid 标记，附『与均值操纵均衡一致/不适用』解释文案。
- **配置**：evaluation 仅请求级配置（不进全局默认——每个项目评标办法不同）；系数格点数、断崖倍数为 mechanism.rs 常量。
- **新依赖**：无。均值/截尾/穷举/格点全是初等计算；不引随机数依赖（格点代替随机采样，保证可复现）。
- **风险**：真实评标公式千差万别（二次平均、分段计分、随机抽取系数集合）→ v1 只覆盖截尾均值×系数一族，公式外项目显式『不适用』而非硬算；2-5 份标书统计功效天然弱 → 输出定位为反事实解释证据（『若剔除这 2 家，中标人在 83% 的系数取值下改变』），绝不包装成 p 值；评标办法靠人工录入（招标文件不在导入范围）→ 录错导致误导，UI 必须回显公式全文并写入导出配置快照；嫌疑组由文本证据先验圈定，存在循环论证观感 → 报告中标明组的构造依据。
- **验收标准**：单测（手算对拍）：(1) 5 价 [100,98,96,95,60]、去 1 高 1 低、c=0.95 → 基准价 = mean(98,96,95)×0.95，最接近者判中标；(2) 构造陪标对使剔除后中标人在全部格点翻转 → flip_prob=1.0；(3) 无证据组 → mechanism.groups 为空；(4) method=lowest 时输出不含任何均值基准字段；(5) 60 的断崖 support-bid 被标记；(6) 同输入两次运行 numeric_json.mechanism 逐字节一致。e2e：CompareSetup 录入公式 → 导出报告配置快照含 evaluation 原文。

### W5-6 数值层围标信号接入 collusion.rs（替换报价梯度）+ 六格式报告（3d）

- **价值**：闭环交付：让数值证据真正改变围标结论与导出报告。现行第 5 信号『报价梯度』（全文最大金额差<3%，权重 0.15）证明力弱且易被业绩金额劫持（scheme §8.4 已列为已知短板）；换成雷同率/共享算术错误/规律性/机制翻转四类强证据后，围标分级第一次有了『数值层』支柱，与文本层、取证层构成 SOTA 钦定的三层架构（195 号文场景 17）。
- **设计**：collusion.rs：assess_with 增参 numeric: Option<&NumericEvidence>{max_identical_rate 及对、shared_arith_error_count、regularity_kind、max_pearson_with_low_ratio_cv、mechanism_flip_prob}。新信号与权重（常量区集中，沿用『⚠️ 未经实证校准』注释惯例）：共享算术错误 ≥1 → 0.35（最强单证据）；identical_rate ≥ 告警线 → 0.20 起按超出幅度线性至 0.30；规律性差异 → 0.15；r>0.99 且比值 CV<0.5% → 0.10；flip_prob ≥0.5 → 0.15；数值类合计封顶 0.45 防与文本信号双重计数（雷同清单行本身已抬高文本峰值与聚类数）。旧 PriceProximity 降级为回落信号：仅当无任何 BOQ 数据时保留 0.15 权重，保证纯技术标/无清单场景行为不回退。信号 kind 取 numericIdentical/numericArithError/numericPattern/numericMechanism，Matrix.tsx 的 signals.map 自动列出，仅补图标与措辞映射。导出：export/data.rs 聚合 numeric_json → html/markdown/docx 增『商务标数值比对』章（雷同率对表、共享算术错误逐条清单含页码、机制反事实块）；xlsx.rs 新增『数值比对』工作表；json.rs 原样内嵌 numeric 对象；csv.rs 追加 pair 级指标行。compare_service.rs 校准门禁测试组（~850 行起）同步扩充正负样例。
- **改动文件**：
  - `src-tauri/src/engine/collusion.rs`：assess_with 增 numeric 参数与四类新信号常量/计分；PriceProximity 降级为无 BOQ 时的回落；单测全面覆盖
  - `src-tauri/src/services/compare_service.rs`：阶段 8 由 W5-2/3/4/5 产物构造 NumericEvidence 传入 assess_with；price_proximity 仅在 BOQ 缺席时调用
  - `src-tauri/src/export/data.rs`：聚合导出数据结构增 numeric 块（从 jobs.numeric_json 反序列化）
  - `src-tauri/src/export/html.rs`：增『商务标数值比对』章节渲染
  - `src-tauri/src/export/markdown.rs`：同上（表格 + 逐条证据）
  - `src-tauri/src/export/docx.rs`：同上
  - `src-tauri/src/export/xlsx.rs`：新增『数值比对』sheet：对级指标 + 相同项明细
  - `src-tauri/src/export/json.rs`：内嵌 numeric 原始对象
  - `src-tauri/src/export/csv.rs`：追加 pair 级数值指标
  - `src/screens/Matrix.tsx`：insights 映射补 4 个 numeric 信号的图标/中文措辞
- **DB 改动**：无（读 numeric_json）。
- **UI 改动**：Matrix 信号分解区自动出现数值信号；导出预览无结构变化。
- **配置**：无新配置；权重常量集中在 collusion.rs 供后续 W-校准工作流统一回测。
- **新依赖**：无。
- **风险**：新权重与 0.45 封顶仍是经验值（与 v1 已知短板 #1 同性质）→ 常量集中 + 校准门禁测试保证正负样例不回退，等合成语料工作流落地后统一拟合；数值证据与文本证据双重计数抬高误报 → 封顶 + 文档化推导；共享算术错误若来自双方引用同一招标控制价清单的印刷错误会误判『同源』→ 明细必须展示原文供人工排除，且该信号在报告中标注『需人工核对是否源自招标文件』；六格式导出改动面大 → 每格式一个快照测试防回归。
- **验收标准**：单测：(1) 仅 shared_arith_error=1 → 信号权重 0.35、level ≥ medium；(2) identical_rate=0.9 + 元数据同源 → high；(3) 数值信号全满时 score ≤1 且数值合计 ≤0.45；(4) 无 BOQ → 旧 price 信号照常触发，既有 collusion_pipeline_on_generated_bids_v2、price_proximity_signal 等测试不改断言仍过；(5) e2e：85% 雷同 xlsx 对 → collusion_json 含 numericIdentical 信号。导出：六格式各有测试断言含『商务标数值比对』内容；HTML/JSON 快照含共享算术错误明细与 chunk 定位信息。

**该工作流的开放问题**：

- 扫描件 PDF 中的报价表（OCR 路径不产 table_row 块）本工作流明确不覆盖：是否另立条目引入表格结构识别（PP-StructureV3 类，涉及新 ONNX 模型与体积），还是产品上声明『数值层仅支持 xlsx/docx/文本 PDF 清单』？
- 数值证据面板放 Matrix 页内 tab 还是独立 screen（BusinessNumeric.tsx）？影响导航与信息架构，需产品定夺。
- identical_rate 默认 80% 告警线引用的青岛/贵州口径原文针对『电子投标文件相同内容占比』整体而非严格的单价相同率——默认值沿用 0.80 是否可接受，还是保守调高并把法规口径写进帮助文案？
- 评标办法 v1 只支持『截尾算术平均×系数』一族公式且靠人工录入（招标文件不在导入范围）：是否接受？『随机抽取固定系数集合（如 0.90/0.92/…/0.98 抽一）』是否需要在 v1 就支持为离散格点？
- 旧『报价梯度』信号在有 BOQ 时被完全替换、无 BOQ 时保留回落——是否接受这种双轨过渡，还是希望彻底移除 PRICE_EXCLUDE 启发式？
- 文档地图与 scheme 文档称迁移到 V8，但仓库实际已到 V11（V9 删索引、V10 加索引、V11 truncation_notice）：本工作流按 V12/V13 排号，需要同步修订 docs/bid-comparison-scheme.md 的表述吗？


## 8. 设计明细：W6 洗稿增强 + 融合校准 + 校准语料

> 设计代理的工作流综述：W6 洗稿层增强 + 证据融合校准 + 校准语料（合成对抗语料 → cross-encoder 复核带 → 围标信号 log-LR 融合 → 概率校准与共形三带 → 回归基线）

### 合成对抗语料生成器 + 标注格式 + 分层评测 harness（4d）

- **价值**：整条 W6 的地基：scheme §8.1 明确指出五维权重与围标分级线"没有带标注案例的回测校准"，collusion.rs 头部注释也自认"未经实证校准"。真实围标判例拿不到，用确定性变换对真实标书章节施加已知强度的改写，可立即获得带标签的正负样本，供后续三条（reranker 阈值、LR 权重、Platt/共形阈值）拟合与回归。现有 calibrate_real_corpus（compare_service.rs #[ignore] 测试）只能人工看输出，无法机械回测。
- **设计**：变换器集合放 src-tauri/src/engine/corpusgen.rs（#[cfg(any(test, feature = "dev-tools"))] 门控，不进发布二进制），全部确定性：RNG 复用 candidate.rs 已有的 splitmix64（按 sample_id 派生种子，零新依赖）。六类变换：①同义替换（自建标书文体同义表 fixtures/corpus/synonyms.tsv ~400 条，jieba 分词后按比例 p 替换，避开 features::extract_entities 命中的实体 span）；②句序打乱（复用 chunker 的句边界切分，段内置换）；③数字微调（对实体 span 内金额/工期 ±1–5%，围标正样本则保持一致）；④全半角/标点扰动（normalize.rs NFKC 规则的逆操作：注入全角、互换中英标点、插空格）；⑤母文件改抬头（用 test_fixtures.rs 的 write_docx_body/write_docx_price_table 生成 K 份 docx——公司名/抬头替换 + 每份少量独立编辑 + 写入 docProps/core.xml 作者元数据，产出文档集级围标正样本；独立撰写+共用模板块为硬负样本）；⑥OCR 噪声（PaddleOCR 常见混淆表 fixtures/corpus/ocr_confusions.tsv：0/O、日/曰、己/已 等 + 随机删字/插空格）。标注格式两级：段对级 JSONL（fixtures/corpus/pairs.jsonl：{id, seed_id, transforms[], label∈{same,minor_change,rewrite,unrelated}, text_a, text_b}）；文档集级 fixtures/corpus/docsets/<case>/*.docx + manifest.json（label∈{collusion,independent}）。生成入口为 cargo bin：src-tauri/src/bin/corpusgen.rs（required-features=["dev-tools"]），固定种子输出逐字节可复现，fixtures 进仓库。评测 harness 在 corpusgen.rs 内：召回层（candidate::recall 对真对的召回率）、评分层（score_pair+classify_cluster 对四类标签的 per-label precision/recall/F1）、围标层（docsets 走 import+compare 全管线看 level），输出指标 JSON。
- **改动文件**：
  - `src-tauri/src/engine/corpusgen.rs`：新建：六类确定性变换 + 段对/文档集样本生成 + 分层指标计算（precision/recall/F1、召回率、围标混淆矩阵）
  - `src-tauri/src/bin/corpusgen.rs`：新建 dev 工具入口：读 fixtures/corpus/seeds/ 种子章节，固定种子生成 pairs.jsonl 与 docsets/，支持 --write-baseline
  - `src-tauri/src/test_fixtures.rs`：门控从 #[cfg(test)] 放宽为 #[cfg(any(test, feature = "dev-tools"))]，docx 写入函数 pub(crate)→pub，并新增 core.xml 元数据写入（供元数据同源信号训练）
  - `src-tauri/src/lib.rs`：test_fixtures 模块声明同步改 cfg(any(test, feature)) 门控
  - `src-tauri/Cargo.toml`：新增 [features] dev-tools = []；[[bin]] corpusgen required-features = ["dev-tools"]
  - `src-tauri/fixtures/corpus/`：新建：seeds/（脱敏真实标书章节）、synonyms.tsv、ocr_confusions.tsv、pairs.jsonl、docsets/（总体积预算 <5MB）
- **DB 改动**：无（生成器与 harness 不落应用库；docsets 走既有 import 管线临时内存库 open_in_memory）
- **UI 改动**：无（纯 dev 工具）
- **配置**：无运行时配置；生成参数（种子、各变换强度）写死在 bin 内并随 fixtures 版本化
- **新依赖**：无。RNG 用仓库既有 splitmix64；同义表/混淆表自建（规避《同义词词林》等外部词表的许可问题）；JSONL 读写用已有 serde_json
- **风险**：①循环论证：变换器与检测器共享同一套直觉（同义替换 vs 词面维），在自造语料上的指标会系统性偏乐观，只能证伪不能证真——须在文档注明"合成语料指标是下界回归基线，不是真实检出率"；②同义表覆盖不足会让 rewrite 样本改写强度不够、与 minor_change 标签边界模糊；③seeds 若含真实企业信息有泄露风险，入库前必须脱敏；④label 分级线本身依赖现行八类定义，后续 item 4 改带语义时标注格式需保持前向兼容（label 保留原值，band 另算）
- **验收标准**：①cargo run --bin corpusgen --features dev-tools 两次运行输出的 pairs.jsonl/docsets 逐字节一致；②pairs.jsonl ≥2000 对、四类标签各 ≥300、每类变换链 ≥200 对；docsets ≥10 组（围标正/独立负各半）；③cargo test --features dev-tools corpus_metrics -- --ignored --nocapture 输出各层指标 JSON（召回层召回率、评分层 per-label P/R/F1、围标层混淆矩阵）且无 panic；④现有 cargo test --lib 全绿（生成器不影响发布路径）；⑤fixtures 总体积 <5MB

### cross-encoder 复核带：reranker ONNX 接入模型管理器，只重打分 uncertain 带（4d）

- **价值**：diff.rs classify_cluster 中 avg∈[0.55,0.70) 一律判 uncertain 转人工，量大时是复核瓶颈（scheme §9.2.2）。cross-encoder 对"词面全换但同源"与"词面相近但无关"的判别力远高于双编码器余弦（SOTA survey §1.3：NEWS-COPY ARI 93.7 vs LSH 73.7），只跑窄带则 CPU 成本可控。fastembed 5.16 已内置 TextRerank + RerankerModel::{BGERerankerBase, BGERerankerV2M3} 及 try_new_from_user_defined，零新 crate。
- **设计**：新建 engine/rerank.rs，完全镜像 embed.rs 的三级来源模式：内置（models/rerankers/<id>/）→ 自托管下载（~/.cache/bidguard/rerankers/<id>/，复用 ureq+tar 的 .part+rename 原子落盘）→ HF 回落（受 allowCloudModel 闸门约束）；注册表 RERANK_MODELS 提供两档：bge-reranker-base（默认，~1.1GB fp32，中英）与 bge-reranker-v2-m3（~2.2GB，更准）。集成点在 compare_service.rs run_inner 第 6 步 build_clusters 之后：筛出 cluster_type=="uncertain" 的簇（avg∈[0.55,0.70)），对每簇取各文档 primary 成员的跨文档两两文本（≤C(5,2)=10 对/簇），截断至 512 token 喂 TextRerank::rerank，logit 过 sigmoid 得 rerank_score，簇内取均值 rerank_avg；classify_cluster 增参 rerank_avg: Option<f32>——uncertain 带内 rerank_avg≥T_hi 改判 rewrite（severity medium）、≤T_lo 保持 uncertain 但标记"复核倾向排除"，T_hi/T_lo 由 item 1 语料上的 P/R 曲线定初值。新增进度阶段 "rerank"（每簇 ctx.check() 支持取消），簇数封顶 RERANK_MAX_CLUSTERS=200（估算：base 档 CPU ~50–150ms/对，200 簇×≤10 对 ≈ 1.5–5 分钟上限，默认远低于此）。降级路径与语义层一致：比对中禁隐式下载，模型未缓存则跳过并在 CompareSummary 置 rerank_degraded=true 提示去工具屏预下载；embedder 与 reranker 各自 Arc<Mutex<Option<…>>> 槽位，比对结束不主动卸载（常驻换速度，内存峰值 +~1.5GB 需实测）。
- **改动文件**：
  - `src-tauri/src/engine/rerank.rs`：新建：RerankModelSpec 注册表（bge-reranker-base / bge-reranker-v2-m3）、resolve/ensure/model_cached_for/download_model/clear_model_cache/rerank_pairs，镜像 embed.rs 结构
  - `src-tauri/src/services/compare_service.rs`：run_inner 第 6 步后插入 rerank 阶段：uncertain 簇 primary 对重打分、写回 rerank_avg；CompareSummary 增 rerank_degraded；CompareRunConfig 增 enable_rerank/rerank_model
  - `src-tauri/src/engine/diff.rs`：classify_cluster 增 rerank_avg 参数与 T_hi/T_lo 改判逻辑 + 单测补带内改判用例
  - `src-tauri/src/state.rs`：AppState 增 reranker 常驻槽位（同 embedder 模式）
  - `src-tauri/src/commands/tools.rs`：get_model_status 增 reranker_models 字段；新增 download_reranker_model / clear_reranker_model 命令（复制 embedding 版）
  - `src/screens/Tools.tsx`：新增"复核模型 · 交叉编码器"卡片，复用现有下载/清理框架
  - `src/screens/CompareSetup.tsx`：新增"交叉复核（uncertain 带）"开关（默认关，模型未缓存时提示先下载）
  - `src/api/types.ts`：ModelStatus/CompareSummary/比对配置 DTO 增 rerank 字段；ClusterSummaryDto 增 rerankScore
  - `src/queries/data.ts`：新增 useDownloadRerankerModel 等 mutation/query
- **DB 改动**：V12 迁移（注意：migrations.rs 现已到 V11，非任务书所述 V8）：ALTER TABLE clusters ADD COLUMN rerank_score REAL NULL——可空列向后兼容，旧任务行读出 NULL 即"未复核"
- **UI 改动**：Tools.tsx 复核模型卡片（下载/体积/清理）；CompareSetup 开关；ClusterDetail/ClustersScreen 在 uncertain 簇上显示"交叉复核分 0.83 → 改判洗稿"徽标；矩阵页 summary 显示 rerank_degraded 降级提示
- **配置**：CompareRunConfig 增 enable_rerank: bool（默认 false）、rerank_model: String（默认 "bge-reranker-base"）；沿用 security.allowCloudModel 闸门，不新增安全开关
- **新依赖**：无新 crate（fastembed 5.16 已含 TextRerank/RerankerModel/UserDefinedRerankingModel，ort 栈不变）；新增模型资产：bge-reranker-base ONNX ~1.1GB 按需下载，不入安装包
- **风险**：①内存：embedder(bge-large-zh ~1.3GB)+reranker(~1.1GB) 同时常驻可能顶爆低配办公机，需实测决定"rerank 前临时卸载 embedder"；②512 token 截断：section 级长块被截断后重打分只看开头，须限定 paragraph 级或对超长块降级不复核；③bge-reranker 系为检索相关性训练，"相关"≠"同源改写"，在标书模板文体上 T_hi 误报未知——上线前必须过 item 1 语料，达不到精度就只做"排序提示"不做自动改判；④改判 rewrite 会改变 summary 八类计数分布，导出报告与旧任务对比口径变化需在报告注明；⑤HF 回落下载无 sha256 校验（scheme §8.2 已知短板，此处沿用，风险不新增但也未修）
- **验收标准**：①item 1 语料 uncertain 带段对上：开启复核后 rewrite 检出 recall ≥ 基线 +15pp 且 precision ≥85%，否则不得默认改判；②200 簇×10 对在 M1/普通 x64 办公机 CPU 上 ≤5 分钟，比对可取消（rerank 中途 cancel ≤2s 生效）；③模型未缓存+离线时比对正常完成且 summary.rerank_degraded=true；④V11 旧库升 V12 后旧比对任务照常打开；⑤cargo test --lib 全绿含 classify_cluster 新带内用例；⑥工具屏可下载/清理 reranker 且体积展示正确

### 围标五信号融合重构：线性加权 → 逻辑回归拟合的 log-LR，保留信号分解（2.5d）

- **价值**：collusion.rs 现为五信号硬阈值触发+定值线性叠加（如元数据信号 ≥2 份即整块 +0.25），权重是拍脑袋值且信号是 0/1 跳变。SOTA survey §8 确认逻辑回归融合是法庭语音比对二十年的标准做法：参数极少（每信号一权重+截距），几十~几百标注对即可稳定训练，输出可解释 log-LR。item 1 的 docsets 语料使拟合首次可行。
- **设计**：两步：先把五信号从触发式改为连续特征 x_i∈[0,1]——①峰值 (peak−0.6)/0.4 clamp；②多文档雷同簇数 min(multi/5,1)；③元数据同源份数占比（替代 ≥2 即满分）；④共有罕见词数 min(n/15,1)；⑤报价梯度 (3%−gap)/3% clamp（无信号记 0）。再做融合：z = b + Σ w_i·x_i，p = σ(z)，w/b 由 corpusgen bin 新增 fit-collusion 子命令在 docsets 语料上拟合（IRLS 或梯度下降 ~100 行手写，无新依赖），输出 src-tauri/fixtures/calibration/collusion_lr.json（含权重、训练语料 hash、AUC/Cllr 指标、拟合日期），collusion.rs 经 include_str!+OnceLock 加载。可解释性保留：CollusionSignal.weight 字段语义改为该信号的 log-odds 贡献 w_i·x_i（DTO 形状不变），detail 文案不动；assess_with 签名不变，内部换算。分级线 high/medium/low 暂以 p 的等效映射保持现行为（p≥0.6/0.35/0.1 起步），最终阈值交 item 4 的共形三带接管。collusion.rs 现有 10 个单测按新特征化逐条迁移，compare_service.rs 的"校准门禁"测试组保持正负向同过。
- **改动文件**：
  - `src-tauri/src/engine/collusion.rs`：五信号连续特征化（去掉 META_MIN_DOCS/SHARED_TERMS_MIN 等硬门槛改为斜坡）；线性叠加换 σ(b+Σw_i·x_i)；权重从 fixtures/calibration/collusion_lr.json 经 include_str! 加载（解析失败 fallback 到内置默认权重并 log::warn）；单测迁移
  - `src-tauri/src/bin/corpusgen.rs`：新增 fit-collusion 子命令：读 docsets 跑全管线取五特征向量，手写 IRLS 拟合 LR，输出 collusion_lr.json + AUC/Cllr 报告
  - `src-tauri/src/engine/corpusgen.rs`：新增 logistic 拟合与 AUC/Cllr 计算函数（dev-tools 门控；Cllr 供 item 4/5 复用）
  - `src-tauri/fixtures/calibration/collusion_lr.json`：新建：拟合系数 + 语料 hash + 指标 + 版本注记，进仓库
  - `src/screens/Matrix.tsx`：信号分解区图例文案改为"对数似然比贡献"，加 tooltip 解释校准来源与语料版本
- **DB 改动**：无迁移：collusion_json 存于 jobs 表 JSON 列，结构（level/score/signals[]）不变，旧任务照常渲染；score 语义从"加权和"变"校准概率"在导出报告脚注注明
- **UI 改动**：Matrix.tsx 围标结论卡：score 展示为"串通概率（合成语料校准）"，信号分解条改按 log-odds 贡献排序；导出六格式的围标段落同步文案（src-tauri/src/export/ 各 writer）
- **配置**：无新用户配置；权重文件版本随安装包固化，不可运行时热换（保证结果可复现可举证）
- **新依赖**：无（LR 拟合手写在 dev-tools 内，运行时只是一次 σ(w·x+b) 求值）
- **风险**：①合成 docsets 上拟合的权重可能病态（如元数据信号在合成集里区分度过强 → 权重畸高），需人工审查系数符号/量级并设 fallback 默认权重；②五特征样本量小且相关性高（相似峰值与雷同簇数强共线），要 L2 正则防权重对冲出负号——负权重的信号在监管场景解释不通；③score 数值分布相对 v1 整体移动，老用户对同一工作区复跑得到不同分级——发布说明必须给出对照表；④"母文件改抬头"合成正样本的元数据/报价特征分布与真实围标未必一致，权重上线后仍标注"实验性校准，真实判例回测前不作为唯一依据"
- **验收标准**：①docsets 留出集（拟合时 8/2 切分）上 AUC ≥0.90 且 Cllr < 线性基线；②compare_service.rs 校准门禁测试组正负向全过（真围标集 level≥medium，独立撰写集 level≤low）；③collusion_lr.json 缺失/损坏时 fallback 生效且 log 有 warn、比对不失败；④assess_with 对全零输入仍输出 level=none（截距不得抬底分）；⑤同输入两次比对 collusion_json 逐字节一致（可复现承诺）

### 概率校准（Platt 起步）+ 共形三带输出（放行/转人工/标红）（3d）

- **价值**：现在段对分 0.8 不等于 80% 置信度，0.55/0.70 的 uncertain 带是拍脑袋线（scheme §9.3、survey §8）。Platt 两参数在小样本下最稳（Niculescu-Mizil & Caruana 经验法则），共形预测把"转人工带"从经验阈值变成"自动放行漏检率 ≤α"的可审计承诺——这是给评标监管方交代的语言。语料够后同一文件格式无缝切 isotonic。
- **设计**：新建 engine/calibrate.rs：Calibrator 枚举 {Platt{a,b}, Isotonic{breakpoints}}，从 src-tauri/fixtures/calibration/score_calib.json 加载（含类型、参数、训练语料 hash、α、共形阈值），格式前向兼容 isotonic。拟合侧：corpusgen bin 增 fit-calib 子命令，在 item 1 段对语料的校准切分上拟合 Platt（对 final_score→P(同源)），再做 split conformal：以正样本的 (1−p) 为不合格分，取 ceil((n+1)(1−α))/n 分位得 t_low（p<… 自动放行带的有限样本 FNR≤α 保证，α 默认 5%）；对负样本对称求 t_high 控制标红带误报 β（默认 5%）。运行时：build_clusters 分类完成后为每簇算 calibrated confidence = Calibrator(avg)（有 rerank_avg 时用 rerank 融合分），并派生 band：p<t_low→pass、p≥t_high→flag、其间→review；八类 cluster_type 不动（same/rewrite… 语义保留），band 是复核路由的正交维度，review 带取代 0.55/0.70 uncertain 线的"转人工"职能。围标结论同样过校准：item 3 的 p 直接套同一 Calibrator 框架的独立参数组。落库 V12（与 item 2 合并同一迁移）：clusters 增 confidence REAL NULL、band TEXT NULL。
- **改动文件**：
  - `src-tauri/src/engine/calibrate.rs`：新建：Calibrator{Platt,Isotonic} 加载/求值 + 三带判定 band_of(p, t_low, t_high)（纯函数，单测覆盖边界）
  - `src-tauri/src/services/compare_service.rs`：build_clusters 后为每簇计算 confidence/band 并入 NewCluster；CompareSummary 增 pass/review/flag 三带计数
  - `src-tauri/src/db/repo/compare_repo.rs`：insert_clusters/list_clusters 增 confidence、band 列读写；ClusterSummaryRow 增字段
  - `src-tauri/src/db/migrations.rs`：V12：clusters ADD COLUMN confidence REAL NULL / band TEXT NULL（与 item 2 的 rerank_score 同一迁移批）
  - `src-tauri/src/bin/corpusgen.rs`：fit-calib 子命令：Platt 拟合 + split conformal 求 t_low/t_high + ECE/可靠性曲线报告，输出 score_calib.json
  - `src/api/types.ts`：ClusterSummaryDto 增 confidence/band；CompareSummary DTO 增三带计数
  - `src/screens/ClustersScreen.tsx`：三带过滤 chips（自动放行/转人工/标红）+ 列表行 band 徽标，默认视图聚焦 review 带
  - `src/screens/ClusterDetail.tsx`：头部显示"校准置信度 ~X%（合成语料，漏检保证 α=5%）"
  - `src-tauri/src/export/`：六格式导出增三带汇总与逐簇 band 列（HTML/JSON/CSV/MD/XLSX/DOCX 各 writer）
- **DB 改动**：V12 迁移（同 item 2 合批）：clusters 表加 confidence REAL NULL + band TEXT NULL；旧行 NULL → 前端显示"未校准"，完全向后兼容；无回填
- **UI 改动**：ClustersScreen 三带 chips 与计数徽标；ClusterDetail 置信度条；Matrix 页 summary 三带统计；导出报告三带章节
- **配置**：α/β 与共形阈值固化在 score_calib.json 随包发布，不开放运行时调整（改 α 即改承诺语义，须走版本发布）；设置页只读展示当前校准版本与语料 hash
- **新依赖**：无（Platt 求值 1/(1+e^{-(a·s+b)})，conformal 是分位数计算；拟合在 dev-tools 手写）
- **风险**：①交换性假设：共形保证只在校准语料分布上成立，真实标书分布漂移时 α 承诺失效——所有 UI/报告文案必须写"在校准语料上的保证"，措辞不严谨在监管场景有法律暴露；②小样本下 review 带可能很宽（大部分簇转人工），用户观感是"功能退步"，需在验收里卡 review 带占比；③Platt 假设分数 logit 线性，final_score 在 0.95+ 处饱和堆积会导致高分区欠校准，必要时分段拟合；④与 item 2 改判逻辑的先后耦合：rerank 改判在前、校准在后，管线顺序必须固定并在测试锁定，否则同配置结果不可复现
- **验收标准**：①校准留出集 ECE ≤0.05（对照未校准基线下降 ≥50%）；②留出集上自动放行带实测 FNR ≤ α+2pp、标红带 FPR ≤ β+2pp；③review 带占比 ≤40%（达不到则放宽 α 或回退默认 review-all 并如实展示）；④V12 升级后旧任务打开不报错、band 显示"未校准"；⑤corpus_regression（item 5）新增三带指标断言全过；⑥同输入同配置两次比对 confidence/band 逐字节一致

### 回归测试基线：语料集指标对比进 CI + 本地全量脚本（1.5d）

- **价值**：把 item 1–4 的指标从"跑一次看看"变成机械门禁：任何触碰 normalize/features/candidate/scoring/diff/collusion/calibrate 的改动，CI 自动对比语料集指标，误报/漏报率回退直接红灯，替代凭感觉调参（scheme §9.3 与 §10 验收指标的测量手段）。compare_service.rs 现有"校准门禁"测试组只覆盖少量手造正负例，无法感知细粒度指标漂移。
- **设计**：两档执行：快档（进 CI）——非 ignored 单测 corpus_regression（位于 engine/corpusgen.rs 测试模块）读 fixtures/corpus/pairs.jsonl 与 docsets/，只跑无模型层（features→candidate→score_pair→classify_cluster→assess_with→calibrate），计算召回层召回率、评分层 per-label P/R/F1、围标层 AUC、三带 FNR/FPR，与 fixtures/corpus/baseline_metrics.json 逐项对比：任一 F1 绝对下降 >2pp 或召回率下降 >1pp 或三带 FNR 超 α+2pp 即断言失败，同时打印新旧全表便于定位；语料规模预算保证 CI 增时 <60s。慢档（本地手动）——--ignored 测试 corpus_regression_full 追加语义/rerank 依赖模型的层（要求本地已缓存模型，沿用 BIDGUARD_EMBED_DIR 覆盖机制）。基线更新流程：BIDGUARD_WRITE_BASELINE=1 环境变量下测试改为重写 baseline_metrics.json（含生成时间与 git rev），文件 diff 随 PR 评审——指标变化必须显式可见，不允许静默漂移。ci.yml test job（macOS）追加一步跑快档（需 --features dev-tools 编译测试）。
- **改动文件**：
  - `src-tauri/src/engine/corpusgen.rs`：测试模块新增 corpus_regression（快档，非 ignored）与 corpus_regression_full（--ignored 含模型层）；指标对比与容差断言；BIDGUARD_WRITE_BASELINE 重写分支
  - `src-tauri/fixtures/corpus/baseline_metrics.json`：新建：分层指标基线（含 git rev/语料 hash/生成时间），进仓库
  - `.github/workflows/ci.yml`：test job 增一步：cargo test --manifest-path src-tauri/Cargo.toml --lib --features dev-tools corpus_regression
  - `scripts/eval-corpus.sh`：新建本地脚本：串跑 corpusgen 重生成校验（确定性自检）+ 快档 + 慢档 + 指标全表输出
  - `docs/bid-comparison-scheme.md`：§10 验收指标补一段：指标由 corpus_regression 机械测量，基线更新流程说明
- **DB 改动**：无
- **UI 改动**：无
- **配置**：环境变量 BIDGUARD_WRITE_BASELINE（仅测试路径识别）；CI 无新 secret
- **新依赖**：无
- **风险**：①容差校准两难：2pp 太松漏掉真回退、太紧则重构性 PR 频繁误红——先按 2pp 跑一个月按误报率调；②快档不含模型层，embedding/rerank 相关回退 CI 不可见，只能靠发版前人工跑慢档，需在 release checklist 写死；③docsets 全管线（import+compare）在 CI macos runner 上耗时波动，超预算时得把围标层样本数砍半；④基线文件与语料文件不同步更新（改了 pairs.jsonl 忘了重写基线）会造成持续红灯——corpus_regression 先校验 baseline 内记录的语料 hash 匹配再比指标，不匹配给出明确修复指引
- **验收标准**：①CI 上 corpus_regression 稳定通过且该步骤增时 <60s；②人为把 W_LEXICAL.lexical 从 0.40 改 0.20 后本地跑快档必须失败并打印指标对照表（门禁灵敏度验证）；③BIDGUARD_WRITE_BASELINE=1 重生成的 baseline 在未改算法时与仓库版指标逐项一致（确定性验证）；④scripts/eval-corpus.sh 在缓存好模型的本地机一键跑完两档并输出全表；⑤语料 hash 不匹配时报错信息包含修复命令

**该工作流的开放问题**：

- 任务书写"DB 改动走 V9+"，但 migrations.rs 实际已到 V11（DROP_UNUSED_EDGE_INDEXES_V9/CLUSTER_MEMBERS_INDEX_V10/DOC_TRUNCATION_NOTICE_V11），本工作流的迁移将从 V12 开始——请确认无并行工作流也在抢 V12 槽位（rerank_score/confidence/band 建议合并为同一个 V12 批次）
- reranker 默认档选型：bge-reranker-base（~1.1GB，fastembed 内置支持，中英）够不够，还是直接上 bge-reranker-v2-m3（~2.2GB，更准但内存/下载翻倍）？建议 base 为默认、v2-m3 作可选档，最终以 item 1 语料上的带内 P/R 定夺
- 自动放行带的漏检率 α 是产品/合规决策（建议 5% 起步）：监管场景下"自动放行"四个字是否本身不可接受、必须叫"低优先级抽查"？影响 UI 文案与导出报告措辞
- 合成语料拟合的 LR 权重与校准阈值是否敢直接做默认行为？备选方案：上线时标"实验性校准"，保留 v1 线性权重为可选回退，攒到若干真实判例回测后再摘实验性标签
- 语料种子来源：fixtures/corpus/seeds/ 需要 20–40 段脱敏真实标书章节（技术方案/施工组织/商务条款各若干），是否可从 BIDGUARD_CALIB_DIR 现有 8 份测试标书截取脱敏，还是需要另行供给？
- embedder+reranker 双模型常驻的内存预算：低配 Windows x64 办公机（8GB）是否要强制串行加载（rerank 前卸载 embedder，牺牲二次比对速度）？需一台 8GB 实机实测后定

## 9. 三视角审查记录

三位审查代理共提出 17 处问题（5 HIGH / 10 MEDIUM / 2 LOW）。**全部 5 处 HIGH 与绝大多数 MEDIUM 已吸收进第 1–2 章定稿**，对应关系：迁移抢号与 collusion 融合互斥 → §1.1/§1.2；options_hash 缓存吞指纹 → §1.3；矩阵口径冲突与导出三次返工 → §1.4；硬命中 floor 倒挂、multiDocAnomaly 自动 high、"自动放行"承诺、"串通概率"措辞 → §1.5；工时上调、reranker int8、算术错误双条门槛、evasion suspect 降呈现、xcheck 解除 OCR 上限、背景库降级、Benford 砍除、W5-5 后置、豁免接线与 W3/W4 桥接条目、语料前置 → §2 各里程碑。未采纳项：无整条拒绝，个别 LOW 级（如 seeds 纯合成替代）以建议形式保留在决策点。

### 审查视角：工程可行性（Rust/Tauri/依赖/平台/工时）

**总评**：方案整体工程质量高：文件/行号锚点经逐一核实几乎全部命中（parse.rs 600/901/557 与 403/468/510、compare_service 311/338/293/813/676、chunker 363、Matrix.tsx 126-135/430-436、options_hash v5 等），全部'零新 crate'声明属实（fastembed 5.16 rerank、jieba-rs 0.7.4 has_word、lopdf 0.34、pdfium-render 0.8.37 的关键 API 均已在源码验证），单条迁移均为加表/加列向后兼容，迁移现状 V11 的纠正正确，双平台无新原生依赖故 macOS arm64 / Windows x64 无新增坑。真正会返工的是三处高危：①六个工作流抢占 V12 迁移槽位且编号写死进验收；②collusion.rs 上定值权重+封顶模型（W1/W2/W5）与 W6 的 LR 融合模型互斥、缺落地顺序裁决；③W1 取证指纹不 bump options_hash，会被跨工作区缓存（persist_cached 原样复制旧 fingerprint_json）吞掉，重导入也不生效。另有两处中危（W3-2/W4-4 对 matrix_json 与 Matrix.tsx 的口径冲突；W6-2 reranker CPU 延迟低估 2–4 倍、5 分钟验收在低配 x64 上难达标）和四条 effort_days 偏低（合计约需上调 7–9 天）。先解决三个高危裁决再开工，可避免大部分返工。

- **[HIGH]** 全部工作流 db_changes（src-tauri/src/db/migrations.rs）
  - 问题：六个工作流同时占用 V12（W1 document_images、W2 evasion_json、W3 doc_role、W4 verbatim_matches、W5 boq_items、W6 rerank_score/confidence/band），W3/W4/W5/W6 还连锁写死 V13–V15。虽然各自 open_questions 提到'合入时协调'，但 code_changes 与 acceptance 均硬编码编号。migrations.rs 的执行语义是按数组下标 skip(current) 顺序跑、'已发布迁移只增不改'——任何一支先合入，其余全部要重排编号并改验收测试；若开发者在中间状态跑过某分支构建，本地库 version 已计到 12+，重排后会静默跳过别支的迁移（加列类迁移不幂等报错，直接漏执行）。
  - 修正：开工前做一次全局迁移编号分配表（或合并为 2-3 个批次、明确各工作流落地顺序），写进各工作流文档；更稳的做法是把迁移框架改成按迁移名记账（schema_migrations 表）再动手，彻底消除编号抢占。
- **[HIGH]** src-tauri/src/engine/collusion.rs assess_with（W1-5 / W2-5 / W5-6 / W6-3 交叉）
  - 问题：W6-3 把评分模型重构为 σ(b+Σw_i·x_i) 且只对'五个旧信号'拟合 collusion_lr.json；而 W1/W2/W5 新增的 rsid/pdfLineage/imageReuse/sharedErrors/evasion/numeric* 约 7 类信号全部按'定值权重 + 分类封顶（FORENSIC_CAP=0.45、数值层 0.45）+ 等级下限 floor'的线性叠加模型设计，验收里写死 weight=0.20/0.35、EVASION_WEIGHT=0.25 等具体数值。两套模型互斥：W6-3 先落地则 W1/W2/W5 的设计与全部单测作废；后落地则这些定值权重/封顶在 LR 框架里无处安放、验收全部重写，且 LR 权重文件每加一个信号就要重新拟合。另外 W2-1/2 改 normalized_text 后，W6 的 collusion_lr.json / score_calib.json / baseline_metrics.json 全部失效，方案没有规定重拟合时序。
  - 修正：在计划层面裁决：W6-3 最后落地，其特征向量一次性枚举全部新信号（各工作流新信号从第一天就按连续特征 x∈[0,1] 定义而非定值触发）；assess_with 立即收敛为单一 inputs 结构体参数（W1-5 已有 ForensicInputs 雏形，扩成全量），避免四个工作流轮番改签名；明文规定'任何改归一化/信号的合入必须重跑 fit-collusion / fit-calib 并重写 baseline'的流程与责任人。
- **[HIGH]** W1 全部条目 × src-tauri/src/services/import_service.rs（options_hash:64-80、缓存复用 246-267、persist_cached 374-395）
  - 问题：W1 的 rsid/Template/zip 指纹/PDF 血缘/图片哈希全部在解析期产出，但方案明确不 bump options_hash（'无 schema 变更，靠 serde default 兼容'）。实测代码：跨工作区缓存按 (file_hash, options_hash='v5…') 命中 find_parsed_by_hash 后由 persist_cached 原样复制旧行的 fingerprint_json——同一文件只要历史上被任何工作区用旧版导入过，升级后重新导入也拿不到新取证字段；W1-4 的 document_images 更是既不会在缓存路径重算、方案也没让 persist_cached 复制。结果是取证信号随导入历史'时有时无'，且 W1 open question 里'重新导入才生效'的前提在现有缓存语义下不成立——上线后必然以 bug 形式返工。
  - 修正：W1 落地时同步 bump options_hash（建议与 W2 的 v5→v6 合并为一次，避免两次全量缓存作废），或把'指纹提取器版本'并入缓存匹配键/文档行并在复用时校验；persist_cached 需同步复制（或重算）document_images 行；acceptance 增加'旧缓存命中路径也产出新指纹'的用例。
- **[MEDIUM]** W6-2 cross-encoder 复核带（src-tauri/src/engine/rerank.rs 新建）
  - 问题：依赖与体积核实无误（fastembed 5.16.0 确有 TextRerank/BGERerankerBase/try_new_from_user_defined；base ~1.1GB fp32 / v2-m3 ~2.2GB 准确），但 CPU 延迟'~50–150ms/对'明显低估：278M 参数 cross-encoder 跑满 512 token，fp32 在普通 x64 办公机上更接近 300ms–1s/对（M 系芯片约 150–300ms），200 簇×10 对的上限工况是 10–30 分钟而非'1.5–5 分钟'，'≤5 分钟'验收在目标硬件上大概率不达标；且 fp32 会话 RSS 约 1.5–2GB，叠加 bge-large-zh 常驻后 8GB Windows 机接近交换阈值（方案自己也标为待实测）。首次加载 1.1GB 模型的会话初始化 10–30s 也未计入。
  - 修正：默认档改用 int8 量化 ONNX（~300MB，CPU 提速 2–4×，HF 上 bge-reranker-base 有现成量化产物，fastembed UserDefined 路径可直接加载）；截断上限降到 256 token（uncertain 带以段落为主够用）；RERANK_MAX_CLUSTERS 按实测回调；验收指标注明模型档位+硬件基线；8GB 机型把'rerank 前卸载 embedder'定为默认行为而非开放问题。
- **[MEDIUM]** jobs.matrix_json / src/screens/Matrix.tsx（W3-2 与 W4-4 冲突）
  - 问题：两个工作流都重写 compare_service 阶段 8 的矩阵组装与 Matrix.tsx 单元格主显：W3-2 加 matrixOriginal/peakOriginal 且规定'风险分级只用剔除后数字'（改变喂给 collusion 的 peak 来源），W4-4 加 segmentMatrix/segmentPeak/mode 且规定'围标继续吃 legacy peak'。叠加后 matrix_json 同时存在剔除前/剔除后/区段三套口径，'legacy peak'指全量还是残差未定义，两支的前端主显数字与角标/Pill 方案互相冲突——后合入的一方必须重做 JSON schema、Matrix.tsx 和各自的'逐字节一致'回归验收。
  - 修正：先裁决一张口径矩阵（展示口径 × 围标输入口径 × 导出口径）并冻结 matrix_json 最终 schema（建议：{documentIds, matrix(剔除后·聚类), matrixOriginal, segmentMatrix, peak 一律取自明确命名的字段}），两支按同一 schema 实现；calibrate_real_corpus 与 W6 基线以该 schema 为准一次性适配。
- **[MEDIUM]** effort_days（W1-5 / W3-2 / W2-3 / W4-5）
  - 问题：四条明显偏低：W1-5'取证统一接入'2d——assess_with 签名变更波及 collusion 全部单测+两处管线测试，再加六格式导出逐个补'取证证据'章节（docx.rs 排版自认最费时），2 天不现实（我的估计：4d）；W3-2'招标对减'4d——winnow 引擎+边集拆全量/残差+聚类和矩阵各跑两遍的编排改造+V13 新表+双数字 UI+六格式导出模板+'与 v0.5.0 逐字节一致'回归，是全计划改动面最大的单条（我的估计：6–7d）；W2-3'PDF 隐藏文字审计'3d——Tm/Tf 矩阵状态机、OCR 双层页判别、用 lopdf 程序化构造四类夹具 PDF 本身就是苦工（我的估计：4–5d）；W4-5'区段视图'4d——双栏同步滚动+数百 chunk 虚拟化+三级高亮新屏（我的估计：5–6d）。其余条目（W1-1/2、W5 各条、W6-1/3/5）估算合理。
  - 修正：按上述估计上调（净增约 7–9 个编码日），或对 W1-5/W3-2 做减法：导出六格式先只做 HTML+JSON 两格式、其余格式列为后续小项；W3-2 的双数字 UI 与导出双行可拆为独立 0.5–1d 条目，避免单条过载拖垮排期。

### 审查视角：产品与合规（误报/措辞/法条/离线）

**总评**：整体方向与 scheme/survey 高度对齐，命中路径的免责纪律（『另存为可清除』『元数据可抹除』『请核对是否源自招标文件』）做得比多数同类方案好，离线约束基本未被破坏。但有四处产品级硬伤必须先解决：① W1 硬证据 floor 与 W3 招标模板豁免的排期倒挂，会让『统一下发模板』这一主流合规场景被系统性抬到 medium（最大新增误报源）；② W3 multiDocAnomaly 自动 high + 引用条例第四十条，在招标文件 OCR 质量无闸门的前提下是法条被误用为自动定性；③ W6 的『自动放行（FNR≤5%）』与『串通概率』两处措辞把合成语料上的统计假设包装成对监管方的承诺/后验定罪概率，存在法律暴露；④ 四个工作流各改一套 collusion 融合范式（线性+cap/floor vs LR），校准只覆盖旧五信号，分级语义将多次漂移。点名建议砍掉/降级：W3 条目 4 跨工作区增量背景 DF 库（破坏可复现可举证、偏离 2–5 份核心场景最远）、W5-3 Benford 首位检验（自认前提弱，纯凑清单）、W5-5 机制反事实整条后置（公式覆盖窄+人工录入风险+循环论证观感）；另把 W6 reranker 的自动改判软化为复核建议、W4 逐字/区段证据的 HTML/DOCX 导出纳入本期，否则『报告即证据』不成立。

- **[HIGH]** W1 条目 6「取证信号统一接入」+ W1 条目 1/3（硬命中强制等级下限 medium）
  - 问题：在 W3 招标文件豁免（exempt_rsids / exempt_hashes / TenderExemption）落地之前，硬命中 floor=medium 会被『招标代理统一下发投标文件模板』这一最常见的合规场景系统性触发：全体投标人天然共享模板 rsid、Template 名、zip 条目序列，甚至同一批 PDF 母版的 trailer ID——每个使用统一模板的项目都被机器强制抬到 medium。这不是长尾误报，是主流程误报，会在上线初期就摧毁用户信任。方案自己在 open_questions 里承认了这个依赖，却仍把 floor 写进 W1 验收标准（『rsidRoot 命中且 peak<0.6 → level ≥ medium』），且 shared_count≥1 即记 0.20 权重过于激进。
  - 修正：把『硬命中 floor』设为条件化规则：仅当工作区已导入招标文件且完成 rsid/图片/模板豁免对减后才启用；豁免不可用时硬命中只作信号展示（保留 detail 与模板提示文案），不设等级下限、不进验收断言。rsid 弱档权重建议要求 shared_count≥3（root_match 除外）。排期上把 W1 条目 6 的 floor 规则移到 W3 条目 1/2 之后合入，或在 W1 内先落地一个最小版『招标文件 rsid/哈希豁免』。
- **[HIGH]** W3 条目 3「k-共现过滤升级：multiDocAnomaly」
  - 问题：『查不到出处 → 簇自动 severity=high + detail 引用条例第四十条 + 导出『多家异常一致清单（评标人可直接引用）』』是法条映射被用作自动定性的典型形态。第 40 条的『视为串通』认定权在评标委员会，而本条的查证质量完全依赖招标文件的解析/OCR 质量——补遗答疑常为扫描件（条目 1 风险自认），OCR 错字打断 winnowing 指纹链（条目 2 风险自认）后，合法的逐字应答会被升级成带法条引用的红色 high。误报后果是指控性的，链路上却没有任何质量闸门。方案只『建议上线初期用涉嫌措辞』，力度不够。
  - 修正：三条硬化：(1) 『涉嫌』措辞与『需评标委员会依法认定』脚注从建议改为强制常量，与 195 号文『结论不替代招标人自主判断』对齐；(2) 增加查证质量闸门——招标文件为 OCR/扫描件或对减覆盖率抽样低于阈值时，禁用 anomaly 升级，降级为中性『多家共有段落，出处未能核实』提示；(3) severity 不自动置 high，改为独立的『待复核』标记，不进 high 风险统计，等校准语料回测后再决定是否入分级。
- **[HIGH]** W6 条目 4「共形三带：自动放行」
  - 问题：『自动放行带 FNR≤α=5%』的承诺只在合成语料的交换性假设下成立，而合成语料是用与检测器同一套直觉造出来的（条目 1 风险自认『指标系统性偏乐观』）。SOTA 调研自己给的前提是『每个招标项目批次做在线校准对抗漂移』，本方案却把阈值固化在安装包里——承诺条件被静默丢掉了。监管场景下『自动放行』四个字等于机器替评标人免检，一旦某个真围标对落在 pass 带，『系统承诺过漏检率≤5%』会成为产品方的法律暴露而非护身符。UI 上『校准置信度 ~X%（漏检保证 α=5%）』同样是危险表述。
  - 修正：带命名改为『低优先级抽查 / 需人工复核 / 重点标红』，pass 带只做排序与折叠，不隐藏任何簇、不使用『放行/保证』字样；所有 α 相关文案强制限定『在合成校准语料上测得』；导出报告中三带只呈现为复核优先级建议。open_questions 里已提出此疑虑，应直接按保守答案落地而非留给排期。
- **[HIGH]** collusion 融合架构（W1 条目 6 / W2 条目 5 / W5-6 / W6 条目 3 交叉）
  - 问题：四个工作流各自改写 assess_with 的融合范式且互相冲突：W1 引入 ForensicInputs + 0.45 封顶 + 硬命中 floor（线性叠加范式），W2 加第 6 信号 EVASION_WEIGHT=0.25（线性），W5-6 加四个数值信号 + 0.45 封顶（线性），W6 条目 3 却把五个旧信号重构为逻辑回归 log-LR——LR 只拟合旧五信号，新增的取证/规避/数值信号仍是拍脑袋权重，最终产物是『半校准半经验』的混合体，且封顶/floor 与 σ(w·x+b) 两套语义无法共存。用户视角：同一工作区在几个版本间复跑，分级反复漂移，每次都要重新解释，信任耗尽。
  - 修正：产品层面先拍板融合架构的唯一终态与合入顺序：要么 W6 的 LR 框架最后统一收口（所有新信号以连续特征进入同一 LR，floor/cap 作为 LR 之外的少数显式规则保留并文档化），要么 W6 推迟到新信号齐备后一次性拟合。分级语义变更合并为一次发布，附新旧对照表与引擎版本号写入导出报告。
- **[MEDIUM]** W6 条目 3「串通概率」展示措辞
  - 问题：把 σ(z) 直接展示为『串通概率（合成语料校准）』违反了调研自己援引的 ENFSI/法庭 LR 框架：LR 表达的是『证据对假设的支持强度』，译成口头等级量表呈报；直接给出『串通概率 83%』是后验定罪概率表述（检察官谬误的产品化），且其校准基础是合成数据。评标专家和被质疑的投标人都会把这个数字当成指控结论。
  - 修正：UI 与导出改为『证据强度』+ 口头等级（如：弱/中等/较强/强支持同源编制），沿 ENFSI 量表映射；概率数值仅保留在 JSON 导出的技术字段并注明『合成语料校准、非串通概率』。信号分解继续按 log-odds 贡献排序展示，这部分设计是好的。
- **[MEDIUM]** W2 条目 5「evasion 信号」suspect 档的呈现（Library 徽标 / DocPreview 告警条）
  - 问题：suspect 判级线『隐形码点 ≥10 或 confusable fold 命中』太容易被良性来源触发：从网页/PDF 复制粘贴残留零宽字符、排版软件注入的 bidi 控制符、OCR 引入的个别西里尔形近字。按全文总数≥10 判 suspect，再在文档库卡片打『疑似规避』徽标、DocPreview 顶部挂告警条，等于给正常文档扣帽子——这是本方案里最直接『吓退用户』的一处，而『规避』本身是指控性词汇。
  - 修正：suspect 判级改用浓度与聚集度（条目 1 已落库『最大单块浓度/受影响块数』，判级却只用总数——用起来）：例如要求扰动集中于雷同候选块或单块浓度超阈值。suspect 级不在 Library 打徽标、不在 DocPreview 挂告警，仅作为比对期弱信号进 insights；文案区分『检测到异常字符（可能来自复制粘贴）』与 confirmed 级的『疑似规避特征，请人工复核』。
- **[MEDIUM]** W5-6「共享算术错误 0.35 + level≥medium」
  - 问题：单条 shared_arith_error 即给最强权重 0.35 并在验收里锚定 level≥medium，但『错得逐分一致』有一个未被风险清单覆盖的系统性良性解释：两家用同一款计价软件（广联达等市占极高）对招标工程量清单做相同精度的中间舍入，qty×price≠total 的『错误』会天然逐分一致——这是『同一软件』的证据，不是『同一张源表』的证据。风险清单只提到了招标控制价清单印刷错误一种来源。
  - 修正：0.35 档要求 ≥2 条相互独立的共享算术错误，单条降档；检测器先排除可由常见舍入规则解释的差值（如 total 等于按不同精度四舍五入的结果）；明细强制附『请核对是否源自同一计价软件舍入惯例或招标文件』提示。
- **[MEDIUM]** W2 条目 4「xcheck 命中 → 整文档 ocr-fallback」
  - 问题：误触发的代价不对称：失配阈值 0.35 未校准，低质打印/密集表格/印章覆盖都会推高 OCR 噪声（风险自认），一旦误触发，整份可用的文字层被替换成受 OCR_MAX_PAGES=20 截断的 OCR 文本——超过 20 页的文档后部内容直接退出比对。为了防一种规避，引入了更大且更隐蔽的漏检，用户看到的还是『比对完成』。
  - 修正：xcheck 触发回落时解除或大幅提高该文档的 OCR 页数上限（这是取证需要，不是普通扫描件导入）；或只对失配页替换文本、其余页保留文字层；导入完成态明确提示『该文档文字层不可信，已改用 OCR 文本（第 X–Y 页）』并要求用户知情。
- **[MEDIUM]** W3 条目 4「历史文档增量 DF 背景库（跨工作区）」——点名建议砍掉/降级
  - 问题：偏离『2–5 份交叉比对』核心场景最远的条目：其价值前提是同一台机器长期积累 ≥20 份跨项目历史标书，而比对结论随背景库演化漂移，直接破坏产品立身之本『同输入同输出、可举证』——config_json 记个 doc_count 快照无法让另一台机器复现豁免集合；跨项目 gram 计数外溢对招标代理/监管用户还有保密观感问题。它要解决的『行业范本套话』问题，大部分已被内置范本余弦 + W3 条目 2 招标文件对减覆盖。3.5 天成本换一个不可复现、冷启动长期空转的功能，性价比最差。
  - 修正：砍掉增量跨工作区 DF 记账与首启回填；保留『内置范本静态背景库』（九部委范本等随包版本化，双阈值逻辑照用，完全可复现）。若坚持保留增量库：默认按工作区隔离、显式开关、并把背景库内容哈希写入 job 快照作为复现凭据——但建议直接推迟到有真实部署反馈之后。
- **[MEDIUM]** W1 条目 6 / W2 条目 5：零命中时『无命中≠清白』语义整体缺失
  - 问题：免责纪律只存在于命中 detail 内：取证折叠区『空态不渲染』、导出『取证证据』章节零命中时整体省略、evasion 无命中时无任何表述。评标人拿到一份没有取证章节的报告，自然推断『查过了，干净』——这恰是 rsid（另存为即清除）、PDF 元数据（可抹除）、规避检测（高手零痕迹）最不成立的推断。『不输出检查通过』的纪律防住了机器背书，但没防住沉默背书。
  - 修正：导出报告固定加『检查方法与局限』一节（无论是否命中）：列出本次执行的取证/对抗检查项、各自的可清除性说明、以及『未命中不构成清白证明』声明；UI 在围标结论卡加同一句常驻脚注。这一节同时是给质疑报告的投标人看的程序正义证明，成本半天。
- **[MEDIUM]** W5-3「Benford 首位卡方」——点名建议砍掉；W5-5「机制感知反事实」建议降级为后置批次
  - 问题：Benford：方案自认『单价只跨 2-3 个数量级、前提偏弱、n≥30 才检、权重最低』——在 2–5 份标书的核心场景下它几乎恒为噪声，进 UI/报告只会让评标人困惑或被对方律师攻击，属于为凑 SOTA 清单的炫技子项。W5-5：方向有据（C&D），但 v1 只支持一族公式、评标办法靠人工录入（录错即误导且方案自认）、嫌疑组由文本证据先验圈定有循环论证观感、4 天成本，且其 flip_prob 还要以 0.15 权重进围标信号——在数值层地基（W5-1/2/3）尚未经真实语料验证前就把最复杂的推断链入分级，风险收益不匹配。
  - 修正：砍掉 Benford 首位检验（保留等差/等比/恒定折扣与尾数聚集，n 门槛照旧）；W5-5 整条移到 W5 二期：首版最多输出『基准价敏感性』描述性段落，flip_prob 不进 collusion 信号，待 W5-2/3 在真实语料上验证后再决定是否恢复。
- **[MEDIUM]** W4 条目 5：区段/逐字证据不进导出（六格式）
  - 问题：W4 的全部卖点是『可写进评标报告的铁证』（甲第3.2节与乙第3.2节 800 字逐字相同），但导出被明确排除在范围外——评标人在屏幕上看到最强证据，报告里却引用不了，这与『报告即证据』的产品定位直接矛盾。列为『后续任务』意味着一个发布周期内屏幕与报告不一致。
  - 修正：把 HTML/DOCX 两个主报告格式的『对齐区段与逐字证据』章节（区段摘要表 + 逐字区间清单含页码）纳入 W4 范围（约 +1.5 天），JSON 随 serde 顺带；CSV/MD/XLSX 可后置。发布门槛：屏幕可见的证据类型必须在至少一种正式报告格式中可引用。
- **[LOW]** W6 条目 2「reranker 自动改判 rewrite」
  - 问题：cross-encoder 是不可解释的黑盒分数，自动把 uncertain 改判为『rewrite（洗稿）』这一指控性分类，与『机器只给证据和分级、结论权留给评标人』的三态复核原则有张力；且 bge-reranker 为检索相关性训练，『相关≠同源改写』方案已自认。85% precision 的验收线意味着每 7 条自动改判就有 1 条冤枉。另外在 2–5 份标书场景 uncertain 簇量级有限，人工本可承受，1.1–2.2GB 模型对 8GB 办公机的内存风险实测前不宜默认铺开。
  - 修正：改判降级为『复核建议』：簇保持 uncertain 状态，UI 显示『AI 复核倾向：洗稿（0.83）』并按倾向排序复核队列，人工确认后才改分类；默认关的设定保留。这样条目价值（复核提效）不损失，指控性结论仍由人签发。
- **[LOW]** 离线承诺与模型下载（W6 条目 2）/ 语料种子入库（W6 条目 1）
  - 问题：全案未发现明目张胆破坏离线约束的设计——reranker 的 HF 回落沿用既有 allowCloudModel 闸门、比对中禁隐式下载，合规。但两个边缘暴露：(1) 新增一条 GB 级模型下载通道却继续沿用『下载无 sha256 校验』的已知短板，供应链暴露面扩大；(2) fixtures/corpus/seeds 收录『脱敏真实标书章节』进仓库——标书含商业秘密，脱敏不彻底即泄密，且『脱敏』无验收标准。
  - 修正：(1) 在 rerank.rs 注册表内为两档模型固化 sha256 并校验（顺手把 embed.rs 同缺口补掉，半天）；(2) seeds 建立脱敏 checklist（主体/项目名/金额/人名全替换而非遮盖）并在 fixtures README 声明来源授权，或改用纯合成种子。

### 审查视角：排期与依赖（顺序/缺失胶水/里程碑切分)

**总评**：总体判断：六个工作流内部条目顺序基本正确（W6 语料→融合→校准的方向没搞反，W1-5 对招标角色的依赖也正确留桩），真正的问题全在跨工作流缝隙上——六方齐抢 V12 迁移槽、四方轮番改 assess_with 签名而 W6 融合按旧五信号设计且校准语料不覆盖新信号、W1 豁免参数无人接线、W2 归一化 bump 排在背景 DF 库与回归基线之后会造成静默腐化、W4 铁证层对 W3 对减口径视而不见会让最大误报源以最高置信度还魂。另有 W3-3/W3-4 供需倒挂、六格式导出三次全量返工加 W4 导出 2 天未计入。原计划合计 85 净编码日，补齐胶水/桥接/导出缺口后约 93 天。建议单人里程碑（每期一个可发布切面，迁移号按此序分配）：M0 公共地基（~7d）：迁移台账+CollusionInputs 统一入参+matrix_json/ExportData schema 草案（新增 ~1.5d）、W2-1、W2-2（options_hash v5→v6 一次 bump 并预置 pdf_cross_check 键）、W3-1 doc_role——一次性吃掉全部破坏性变更。M1 取证硬证据（~10d）：W1-1+W1-2（合并 2.5d）、W1-3、W1-4、W1-5（豁免留桩）。M2 入口对抗+取证统一呈现（~10.5d）：W2-3、W2-4、W2-5、W1-6——evasion 与四类取证信号一次接入 collusion/Matrix/六格式导出。M3 校准语料与回归门禁提前（~6.5d）：W6-1（生成器扩展覆盖 rsid/零宽/隐藏层/图片/清单样本 +1d）、W6-5——此后每个里程碑受指标门禁保护。M4 招标剥离与背景库（~12.5d）：W3-2 → 招标豁免接线（新增 ~1d，激活 W1 三处豁免参数）→ W3-4 → W3-3（顺序修正）。M5 铁证与对齐引擎（~10d）：W4-1、W4-2、W4-3、W4×W3 残差口径桥接（新增 ~1d）。M6 口径统一与区段呈现（~10d）：W4-4 与 W3-2 双数字展示合并为一次 Matrix 改造、W4-5、W3-5、W4 区段导出补章（新增 2d）。M7 商务数值层（~13d）：W5-1、W5-2、W5-3、W5-4、W5-6（mechanism 输入置为 Option）。M8 复核与融合校准（~9.5d）：W6-2、W6-3（全信号特征向量）、W6-4。M9 机制筛查收尾（~4d，可选/可砍）：W5-5。合计约 93 净编码日，其中新增胶水约 8d 是原计划真实但未计价的成本。

- **[HIGH]** 全部工作流 / db/migrations.rs 迁移编号
  - 问题：六个工作流全部宣称从 V12 起排：W1 document_images=V12、W2 evasion_json=V12、W3 doc_role=V12(+V13/V14/V15)、W4 verbatim_matches=V12(+V13/V14)、W5 boq_items=V12(+V13)、W6 rerank_score/confidence/band=V12。各自的 open_questions 都只说『合入时协调』，没有任何人持有全局台账。MIGRATIONS 语义是 MIGRATIONS[i] 把库从 i 升到 i+1、已发布只增不改——编号一旦发错版就不可回收。
  - 修正：单人开发本质是串行，在排期定稿时一次性预分配迁移台账并写进各条目（按下方里程碑顺序：V12=evasion_json、V13=doc_role、V14=document_images、V15=chunk_exemptions、V16=clusters 豁免列、V17=background 两表、V18=verbatim、V19=segments、V20=segment_diffs、V21=boq_items、V22=numeric_json、V23=rerank/confidence/band 合批）。删除所有『编号以合入时下一空位为准』的浮动表述。
- **[HIGH]** collusion.rs assess_with：W1-6 / W2-5 / W5-6 / W6-3 四处签名与融合模型冲突
  - 问题：依赖搞反的核心案例：W6-3 的 LR 融合按『五信号』设计且声称『assess_with 签名不变』，明显是对着 v1 代码写的；但排在它前面的 W1-6 把签名收敛为 ForensicInputs（含取证 0.45 封顶+medium floor）、W2-5 加 evasion 参数、W5-6 加 numeric 参数并把 PriceProximity 降级——到 W6-3 执行时 assess_with 已有约 14 个信号、两个 0.45 封顶和一个等级 floor，五特征 LR 拟合装不下这些。更致命：W6-1 的语料生成器只产出六类文本变换+作者元数据+价格表 docsets，不生成 rsid/PDF 血缘/图片复用/隐藏文字层/零宽注入样本，导致 W1/W2 的信号在拟合语料里恒为零，LR 无法给它们估权重，最终会出现『LR 融合的老五信号 + 手工权重的新九信号』两套并存的融合体系。
  - 修正：三步：(1) 在 M0 先做一个 ~1 天的小重构：定义可扩展的 CollusionInputs 单结构体（peak/clusters/docs/shared/price/forensic/evasion/numeric 全收拢），W1/W2/W5 只往里加字段、不再各自改签名，全部 collusion 单测只适配一次；(2) 把 W6-1 生成器范围显式扩到 W1/W2/W5 信号类型（母文件保留 rsid、注入零宽/隐藏层、共享图片、清单乘系数——其中零宽注入 W2 open_questions 已提及但未列入 W6-1 变换清单），约 +1 天；(3) W6-3 改为『拟合全量特征向量，语料无法覆盖的信号显式保留为附加 log-odds 项并标注未校准』，删除『签名不变』的假设。
- **[HIGH]** W1-1/W1-4/W1-5 豁免参数 ←→ W3-1/W3-2 招标文件角色：只留桩、无人接线
  - 问题：共同错误豁免依赖招标文件角色这条依赖方向本身没搞反（W1-5 正确地『W3 落地前恒传 None』），但存在两处排期缺陷：(1) 全计划里没有任何条目真正生产并接入 exempt_rsids / exempt_hashes / TenderExemption 三个豁免集合——W1 三个条目只『预留参数』，W3-2 只为文本对减建 TenderIndex，从未提到把招标 docx 的 rsid 集、招标文件图片哈希、招标 token 集喂回 W1 信号，这是被两边都当成对方职责的缺失桥接工作；(2) W1 的三条风险栏都写明『招标模板会天然造成 rsid/图片/错词误报』，而解药 W3-1（doc_role，仅 2 天）被排在整个 W3 工作流里，若 W1→W2 先行，取证信号将带着已知误报源运行两个工作流之久。
  - 修正：把 W3-1（doc_role）提前到 M0 地基期；在 W3-2 之后新增一个 ~1 天的显式条目『招标文件豁免接线』：对 tender/tender_supplement 文档提取 rsid 集合与图片哈希集合（复用 W1-1/W1-4 的提取函数）、从 tender 分块产出 TenderExemption{tokens,text}，接入 rsid_pairs/图片碰撞/shared_error_fingerprints 三处的减法参数，并补『招标模板不再触发 rsid/imageReuse/sharedErrors』的回归用例。
- **[HIGH]** W2-1/W2-2（归一化 v5→v6） ←→ W3-4 背景 DF 库 / W6-1 回归基线：跨工作流顺序约束缺失
  - 问题：W2-1/2 修改 normalized_text 并 bump options_hash v5→v6（已核实现为 v5）。若按 JSON 顺序 W3-4 先落地增量背景 DF 库，之后 W2 再改归一化，则 background_grams 里新旧两种归一化口径的 4-gram 混存（df 只增不减、无版本标记），只能靠 Tools 全量重建挽救；W6-1/W6-5 的 baseline_metrics.json 同理会被后落地的归一化改动打翻重写。另外 W2-4 把 pdf_cross_check 并入 options_hash 是又一次全量缓存失效——若与 v6 bump 分两次发布，用户吃两次重解析成本（W2 open_questions 只提到条目 1/2 分两次的情形，漏了条目 4 这一次）。
  - 修正：把 W2-1+W2-2 固定进 M0（所有涉库工作流之前），并在同一次 v6 bump 中预置 pdf_cross_check 键（先带默认值入哈希，功能在 W2-4 再实现），保证整个计划期内 options_hash 只失效一次；W3-4 与 W6 基线一律在 v6 之后执行。
- **[HIGH]** W4-1/W4-2（verbatim/对齐） ←→ W3-2（招标对减）：残差口径未贯通
  - 问题：W4 的逐字铁证与对齐区段建在原始文本/全量 ScoredEdge 上，只豁免 is_template 块，完全不知道 tender_coverage 的存在。两份标书对同一招标条款的合法逐字应答会被 SAM 输出为『800 字逐字相同』的深红铁证、被链化成高覆盖区段——恰恰是 W3 要消灭的最大误报源以最高置信度形态还魂，直接动摇『铁证』的产品叙事。两个工作流的设计互相不引用对方，无论谁先落地都缺一次桥接。
  - 修正：增加 ~1 天桥接条目（排在 W3-2 与 W4 均落地后）：verbatim 阶段跳过或标记完全落在 coverage≥0.8 豁免块内的区间（复用 chunk_exemptions）；align 链化改喂残差边（或区段落库时附 tender_coverage 加权），区段视图对招标引用部分给出与 W3 一致的『引用招标文件』徽标；同时明确 W4-2 的软种子保留带也从残差边集取。
- **[MEDIUM]** W3 工作流内部条目顺序：W3-3 排在 W3-4 之前
  - 问题：W3-3（k-共现查证）的查证第二库是 W3-4 的背景库（bg_idx、boiler_fraction≥0.6、doc_count≥20 门槛），供给方却排在消费方之后。设计上有降级路径（查证条件不具备时维持现行为），不算致命，但按列出顺序执行意味着 W3-3 上线时『查不到出处→升级异常一致』的判定只剩招标文件单库，随后 W3-4 落地又会改变同一批任务的豁免/升级口径，同一功能对用户呈现两次行为变化。
  - 修正：执行顺序改为 W3-1 → W3-2 → W3-4 → W3-3 → W3-5，让 k-共现查证一步到位地双库齐备；W3-3 验收用例(3)（背景库<20 时既有测试不变）保留作为降级回归。
- **[MEDIUM]** 六格式导出：W1-6 / W5-6 / W6-4 三次全量改写 + W4 导出缺席未计入工期
  - 问题：html/docx/markdown/csv/xlsx/json 六个 writer 被 W1-6（取证章节）、W5-6（数值章节）、W6-4（三带列）各完整改一遍，W3-2/W3-5 还有双口径与分区小节；而 W4 明确不做导出（open_questions 自估 2 天）却没有列为条目——这 2 天不在任何工作流的 effort 合计里，且导致区段/逐字铁证『屏幕上有、报告里没有』，对以导出报告为最终交付物的评标场景是功能性缺口。docx.rs 排版三次返工的成本在 W1-6 风险栏已自认『易被低估』。
  - 修正：(1) 在 M0 一次性设计 ExportData 的 forensic/evasion/numeric/segments 节结构（只定形状不实现），后续三次改动按节填充、避免结构返工；(2) 把『W4 区段与逐字证据导出章节』补为显式条目（2 天）排入区段呈现里程碑；(3) 三带列（W6-4）与 LR 文案（W6-3）合并为同一次导出改动。
- **[MEDIUM]** jobs.matrix_json 与 Matrix.tsx：W3-2 与 W4-4 两套互不知情的双口径扩展
  - 问题：W3-2 给 matrix_json 加 matrixOriginal/peakOriginal（原始 vs 剔除招标后），W4-4 加 segmentMatrix/segmentPeak/mode（聚类 vs 区段口径），两者叠加后同一单元格存在最多四个数字、两个独立的『双数字展示』UI 概念，Matrix.tsx 被两个工作流各改一遍且交互语义可能打架（角标+tooltip vs Pill 切换）。另外 W3-3 豁免簇退出残差矩阵聚合，意味着 segmentMatrix 若不同步豁免口径，两套矩阵的『剔除』语义还会分叉。
  - 修正：在 M0 的 schema 草案里一次定稿 matrix_json 形状（documentIds/matrix/peak/matrixOriginal/segmentMatrix/mode + 各口径定义），W3-2 与 W4-4 按同一 schema 分两步填充；Matrix.tsx 的双口径 UI（角标+Pill+tooltip 对照）合并为一次改造，放在 W4-4 所在里程碑统一实现，W3-2 阶段先只出数据不做展示切换。
- **[MEDIUM]** W6-1 + W6-5（校准语料与回归门禁）排在全计划最末
  - 问题：W1–W5 至少有十几处阈值/权重自我标注『未经校准、等 scheme §9.3 合成语料回测』（rsid 0.20/0.35、dHash≤10、xcheck 0.35、identical_rate 0.80、链化 gap 常数、MULTI_ANOMALY 0.30……），而唯一能机械回测它们的语料生成器和回归门禁被排在最后一个工作流。这意味着 60+ 个编码日在没有任何指标护栏的状态下持续改动 normalize/candidate/scoring/collusion 核心路径，出了回退只能靠手造用例发现；W6-2 reranker 的 T_hi/T_lo 也要求 item 1 先行。W6 内部 1→3→4 的顺序（语料先于融合校准）本身是对的，问题是它相对 W3–W5 的全局位置。
  - 修正：把 W6-1+W6-5 前提到取证/对抗信号成型之后（建议 M3，共约 6.5 天含生成器扩展），接受每个后续里程碑落地时 BIDGUARD_WRITE_BASELINE 重写一次基线（基线 diff 随 PR 评审本来就是该机制的设计意图）；W6-2/3/4 仍留在尾部。
- **[MEDIUM]** W5-6 ←→ W5-5：NumericEvidence.mechanism_flip_prob 的可选性
  - 问题：W5-5（机制感知筛查，4 天）是全计划产品不确定性最高的条目（评标公式靠人工录入、v1 只支持一族公式、有循环论证观感），而 W5-6 的 NumericEvidence 把 mechanism_flip_prob 列为字段之一。若严格按 W5 条目顺序执行，风险最高的条目卡在闭环交付（W5-6）之前；若想先交付数值信号闭环，接口上又没写明 flip_prob 可缺席。
  - 修正：把 mechanism_flip_prob 在 W5-6 明确为 Option（无 mechanism 结果时该信号缺席），W5-5 后置为独立的收尾里程碑（可根据产品拍板结果砍掉或缩水），W5-1→2→3→4→6 先行闭环。
- **[LOW]** W1-1 与 W1-2 应合并
  - 问题：两条目改动文件集合几乎重合（parse.rs 的 docx 路径、report.rs Fingerprint 字段、fingerprint.rs cross_flags、engine.ts），都是解析期顺手提取+交叉规则，各自 1.5/1 天，分开做意味着同一批测试夹具（手造 docx fixture）与 Fingerprint serde 兼容回归跑两遍。
  - 修正：合并为『docx 取证指纹（rsid + 深度元数据）』单条目 2.5 天，一次动 parse/report/fingerprint/engine.ts，一套 docx fixture 覆盖两组用例。
- **[LOW]** W1-6 的等级 floor / 取证封顶 ←→ W6-3/W6-4 融合重构
  - 问题：W1-6 引入『硬命中强制 level≥medium』与 FORENSIC_CAP=0.45 两条产品级规则，W5-6 又加数值类 0.45 封顶；到 W6-3 换成 σ(b+Σw·x) 概率、W6-4 用共形三带接管阈值时，这些 floor/cap 都要在 LR/校准空间里重新表达（log-odds 里没有『分项封顶』的自然对应物），属于已知会返工的规则。计划各条目风险栏都没提这层耦合。
  - 修正：接受返工但控制成本：floor/cap 从一开始就实现为 assess_with 出口处的独立后处理层（而非揉进各信号计分），W6-3 重构时该层原样保留在概率输出之后，只需重新校准数值。
- **[LOW]** W2-2 与 W2-1 的发布耦合
  - 问题：W2 open_questions 已指出条目 1/2 分两次发布要 bump 两次 options_hash，但排期上没有落成硬约束；两条目合计仅 3.5 天且共用 InvisibleStats 通道与 normalize_with_stats 入口，没有分开的理由。
  - 修正：在里程碑里绑定为同一发布单元（本方案已并入 M0），并写明与 W2-4 的 pdf_cross_check 键共享同一次 v6 失效。

## 附录 A：M0 冻结的 schema 草案（2026-07-06 定稿，后续里程碑按节填充，不再改结构）

### A.1 jobs.matrix_json 终态形状

```jsonc
{
  "documentIds": ["..."],
  "matrix": [[0.0]],          // 主口径：剔除招标引用后（残差）× 聚类覆盖率。风险分级与围标信号①的唯一输入
  "peak": 0.0,
  "matrixOriginal": [[0.0]],  // 未对减口径（查重源样板剔除、scope 过滤照旧，只差对减一个变量）。M4 起填充
  "peakOriginal": 0.0,
  "segmentMatrix": [[0.0]],   // 区段口径：对齐区段按 chunk 去重后的覆盖率。M5 起填充；须与残差豁免口径同步
  "segmentPeak": 0.0,
  "mode": "cluster"           // 前端主显口径："cluster" | "segment"。旧任务无此键 → 按 cluster 渲染
}
```

规则：所有新键均为可选（旧任务缺键走缺省渲染）；围标信号①持续消费 `peak`（剔除后聚类口径），是否切换 `segmentPeak` 由 M7 回归基线回测后决定；Matrix.tsx 的双口径 UI（Pill 切换 + tooltip 对照 + 差异角标）在 M5 一次性实现，M4 阶段只填数据不做展示切换。

### A.2 ExportData 新增节骨架（六格式导出共用，每节 `Option`，缺省 = 该里程碑未落地）

```jsonc
{
  "forensic": {               // M2 填充（HTML/JSON 先行，DOCX M5 补齐）
    "hits": [{ "kind": "rsid|pdfLineage|imageReuse|sharedErrors", "docA": "", "docB": "", "level": "hard|mid|weak", "detail": "" }],
    "perDocument": [{ "docId": "", "rsidCount": 0, "templateName": null, "lineage": {} }]
  },
  "evasion": {                // M2 填充
    "perDocument": [{ "docId": "", "counts": {}, "verdict": "none|suspect|confirmed", "pages": [] }]
  },
  "numeric": {                // M6 填充
    "pairs": [{ "a": "", "b": "", "comparable": 0, "identicalRate": 0.0, "pattern": null, "correlation": null, "sharedArithErrors": [] }],
    "docs": [{ "docId": "", "digitStats": {} }]
  },
  "segments": {               // M5 填充
    "pairs": [{ "a": "", "b": "", "segments": [{ "aRange": "", "bRange": "", "coverage": 0.0, "verbatimChars": 0 }] }]
  },
  "methodsAndLimitations": {  // M2 起常驻（无论是否命中）：已执行检查项、可清除性说明、"未命中不构成清白证明"声明
    "checksRun": [""], "disclaimers": [""]
  }
}
```
