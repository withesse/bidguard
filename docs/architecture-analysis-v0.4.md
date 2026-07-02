# BidGuard（原本·标书查重）深度业务与架构分析报告

> 分析日期：2026-07-02 · 基线：v0.4.0（main d277dad）· 生成方式：7 个子系统读码 agent 并行深读全仓源码后合成，所有结论附 file:line 依据。
> 注意：本文是时点快照——file:line 引用与"风险与短板"随代码演进会过时，修复后请更新或标记对应条目。

> 基于 v0.4.0 源码逐层深读（Rust 后端 62 文件 13,163 行 / 前端 52 文件约 9,959 行），所有结论均有 file:line 依据。

---

## 1. 产品与业务：一页讲透

**定位**：面向评标专家、招标代理与监管审计人员的**围标串标取证工具**——一次导入 2~10 份不同投标人的标书（`config.rs:8` MAX_DOCS=10），交叉找出雷同条款、事实矛盾与围标迹象，输出可举证的报告。

**与市面工具的根本分野**在两条产品假设上：

1. **证据在投标人之间，不在文献库里**。知网/PaperPass 是"单文档 vs 库"的自查，BidGuard 是 N 份互查——聚类阶段直接丢弃同文档内部的相似分量（`clustering.rs:156-158`），因为一份标书自己抄自己不构成串通证据。这一行代码就是整个产品与学术查重的分水岭。
2. **文本相似只是证据之一**。深度改写可以规避文本查重，所以引入三路文本外旁证：docx/PDF 元数据指纹（同一作者/最后保存者，`fingerprint.rs:12-19`）、跨文档共有特征词、**报价梯度**（两家内容抄来抄去、报价只差不到 3%——评标实务里"陪标价"的经典形态，`compare_service.rs:670-678`）。

**离线是业务刚需而非技术偏好**：标书是高敏感商业文件，评标现场普遍禁外网。因此全栈进程内运行（fastembed/oar-ocr 纯 Rust，无 sidecar），唯一联网点是模型下载且受 `security.allowCloudModel` 闸门控制、默认 false（`config.rs:102`）；日志"永不记录标书正文"被刻意做成**不可配置**——`config.rs:96-97` 注释的理由值得抄录："可配置反而暗示存在记录正文的路径"。

**核心业务流**：建工作区 → 导入标书（去重+解析+分块）→ 配置比对 → 后台任务跑 8 阶段管线 → 矩阵/条款组/围标结论 → 人工复核（三态 review_status）→ 六格式导出归档。结果展示用十天干（甲乙丙丁…）对投标人匿名编号（`collusion.rs:81`）——中立化叙事贴合评审语境，也顺带解释了 10 份上限的由来（天干用尽）。

**产品气质**：全链路"宁转人工不误告"。模板剔除、子集非冲突、阵营归一、短文本加阈、低置信转 review 五道误报闸门，每一道的业务理由都写进了注释（如 `fact.rs:253`"孤立数字缺乏同类条款上下文佐证"）。这在判定类工具里是正确的价值观——误报直接损害评审公信力。

---

## 2. 总体架构与端到端数据流

```
┌─ 前端 (React 19 + TS, ~10k 行) ─────────────────────────────┐
│ 15 条路由(HashRouter) ← 43 个 TanStack Query hooks           │
│        ← api/index.ts 47 个薄封装 ← client.ts call(invoke)   │
│ zustand progressStore：接进度事件 → invalidateQueries        │
└──────────────────── IPC (48 个 #[tauri::command]) ──────────┘
┌─ 命令面 (commands/, 1064 行) ── 薄壳：校验+取连接+转发 ──────┐
├─ 服务/任务层 (services/ + jobs/, ~3471 行)                   │
│   JobManager 6 态状态机 · 协作式取消 · 进度节流               │
│   import_service / compare_service(1465 行编排) / export     │
├─ 算法引擎 (engine/, 19 文件 5295 行, 零 Tauri 依赖)          │
│   parse/ocr → normalize → chunker → candidate → scoring      │
│   → clustering → diff/fact → matrix/collusion                │
├─ 数据层 (db/, 2256 行)：r2d2 池(8) + V1→V8 迁移 + 10 仓储    │
└─ SQLite 单文件 (WAL, 14 表 20 索引) ─────────────────────────┘
```

**端到端数据流**（导入到导出）：

1. **导入**：对话框选文件 → `import_documents` 立即返回 pending JobRow（`commands/document.rs:17-44`）→ 流式 sha256 三级去重（批内/工作区/跨工作区按 `(file_hash, parse_options_hash)` 复制分块，`document_repo.rs:128-145`）→ rayon 并行解析，但所有 DB 写持一把进程级 Mutex 串行（SQLite 单写者，`import_service.rs:183`）→ 每块在**导入期**就备齐 normalized_text、双 hash、jieba tokens、实体、128 维 MinHash 落库（`chunker.rs:354-402`）。
2. **比对**：`start_compare` 服务端逐项设防（枚举白名单、threshold clamp 0.2-0.99，`commands/compare.rs:44-91`）→ config_json 快照入 jobs 表（支撑重试/追溯）→ 8 阶段管线（§3）→ 边/聚类/diff/facts **单事务**落库，失败或取消由 `delete_job_results` 兜底清残留（`compare_service.rs:96-103`）。
3. **呈现**：jobs 表把矩阵/围标/热力反规范化为 5 个 JSON 列（`migrations.rs:124-128`），列表页 `json_extract` 直取围标等级免 N+1（`job_repo.rs:30-33`）；前端事件驱动失效 + 800-1000ms 轮询双通道兜底（`data.ts:114-116`）。
4. **导出**：从 DB 装配（非内存态）分发 6 种格式写器，同步 spawn_blocking 不任务化（`export.rs:2` 注释："亚秒到数秒量级"）——对应"结论要归档可复查"的业务闭环。

这个分层最值得注意的纪律是：**services 层零 Tauri 依赖**（`services/mod.rs:1`），配合 `JobCtx::for_test` + CollectSink，全部管线可脱离 Tauri 离线单测——这是后面测试质量的结构性前提。

---

## 3. 算法核心：8 阶段管线与关键数值

### 3.1 导入期备料（阶段 1-4）

解析按格式分派：docx 走 zip+quick-xml 流式（含表格行/numPr/outlineLvl 标题识别，含中文 Word/WPS 样式 id '1'..'9'，`parse.rs:693-708`）；PDF 三级回落 pdfium→pdf-extract→OCR（`parse.rs:316-332`），method 字段留痕；页眉页脚按"≥60% 页重复且 ≥3 页"清除（`parse.rs:173-246`）；OCR 视觉行软换行回流拼回自然段。分块三档粒度（section≤6000 字/paragraph/sentence）同表共存；**表格行原子化**——不拆句、diff 走列对齐（`chunker.rs:307-332`, `diff.rs:49-77`），因为表格的证据价值在单元格里的数字而非行文。命中"查重源"通用样板（法规引用/资质承诺，词频余弦≥0.7）的块**标记不删除**（`chunker.rs:40`），比对期可剔除但库里可见可解释。

### 3.2 比对期（阶段 5-8）

**召回：五通道并集**，各抓一类逃逸——exact hash（全同）∪ normalized hash（格式差异）∪ 字符 n-gram 倒排（小改，共享≥3 gram、top_k=100、倒排过长停用且停用阈值随语料放大 max(256, N/10)，`compare_service.rs:171-173`）∪ TF-IDF TopK（换词序，点积≥0.25）∪ embedding TopK（改写，余弦≥0.78、每 chunk 只取 5——注释原话"宁缺毋滥"，`candidate.rs:19-28,160-166`）。复杂度从 O(M²) 压到 O(M·k)。

**精排：五维加权（可选六维）**。词面 0.40/字符 n-gram 0.30/实体 0.15/结构 0.10/顺序 0.05；启用语义后切换权重组，语义占 0.35（`scoring.rs:20-36`）。三个精巧决策：

- **不可测维度踢出分母重分配**而非记 0 或记 1（`scoring.rs:63-74`）——文件头注释算过账：双空记 1.0 会给无关短文本凭空 +0.15 满分权重，记 0 则变相罚分。这是同类工具里少见的严谨。
- **结构维剥掉文档标题根**再算 Dice（`scoring.rs:86-104`）：标书 H1 必然不同，若计入会让所有跨文档边结构维恒 0，等于把逐字相同段落从 100% 系统性压到 ~89%。
- **表格行实体权重×2**（`scoring.rs:58-60`）：区分"同一行抄没抄"的是金额/数量，不是表头模板文字。

短文本（<30 字）阈值 +0.08 封顶 0.98（`compare_service.rs:73-85`）——"按合同执行。"这类短句词面重合天然高。打分即过滤：只保留 ≥ 阈值的边，但每 chunk 留最高分旁路供热力图与 deleted 判定复用（`compare_service.rs:180-227`），避免囤积百兆低分边。

**聚类：并查集 + 低内聚拆分**。边≥阈值（默认 0.7）入 DSU；成员≥4 且边密度<0.55 时迭代删最弱边递归拆分（`clustering.rs:52-53`），显式解决 A≈B≈C 但 A≉C 的传递性过桥问题。每文档取跨文档边分之和最高者为 primary。

**八类分类走阈值带而非模型**（`diff.rs:170-185`）：min_pair≥0.95→same；avg≥0.85→minor_change；≥0.70→changed；**语义≥0.80 且词面<0.50→rewrite**（专抓换句式的深度洗稿，只有开语义模型才能命中）；否则 uncertain 转人工。added/deleted 由基准模式覆盖，**conflict 由事实检测改判、优先级最高**。这不是技术分类，是给评审人的**证据强度分层**：conflict（同源漏改的事实矛盾）> same（原样照抄）> rewrite（洗稿）> uncertain（不硬判）。

**事实冲突**（`fact.rs:180-268`）：amount/duration/date/percentage 四类字段用"**集合互不包含**"语义——A 列三笔款、B 只提首笔 = 信息缺失 ≠ 矛盾；责任主体按阵营归一（招标人/甲方/发包人→owner），跨阵营互换才算冲突。业务含义精准：不同投标人的标书里同一条款金额对不上，往往是同一模板抄改时漏改的**同源铁证**，故风险直接 high。

**围标：五信号线性叠加**（`collusion.rs:25-114`）：①相似峰值（0.6 起算线性到 0.4 分，峰值来自覆盖率矩阵 sim=Σ(边分×较短块字数)/min(总字数)，`matrix.rs:48-63`）②≥3 份共现雷同条款（0.1+0.3·min(n/5,1)）③元数据同源固定 +0.25 ④共有特征词≥5 个 +0.1 ⑤报价梯度 +0.15（共享≥3 雷同条款且最大金额差 0<gap<3%）。总分 ≥0.6→high、≥0.35→medium。每信号带中文 detail+weight——**可解释性不是附加功能，是"评标需书面举证"场景的核心交付物**。

---

## 4. 各层深潜

### 4.1 引擎层：中文标书领域适配的深度

engine/ 的护城河不在算法新颖度（DSU、TF-IDF、MinHash 都是教科书内容），而在**领域细节的密度**：中文数字定向转换只认"数字串+单位"避免误伤"一致/统一"（`normalize.rs:95-98`）；逐位年份"二〇二六年"先于进位逻辑拦截否则算成 6（`normalize.rs:144-157`）；"5万元"与"五万元"双路径对称归一为"50000元"；"第 N 页/- 3 -"页码行正则；竖排标点兼容。确定性设计也到位：MinHash 用 splitmix64 派生 128 组仿射变换零依赖（`features.rs:42-73`），legacy_text 逐字符复刻旧实现保证跨版本字数统计稳定（`parse.rs:352`）。

### 4.2 服务与任务层：状态机的工程学

`jobs/` 最核心的决策是 **execute() 同步核心与 spawn() 薄壳分离**（`jobs/mod.rs:3` 注释明示）：状态机、catch_unwind、终态映射全在可同步单测的 execute() 里。取消走协作式（AtomicBool + 检查点：哈希每 16MB、精排每 512 对、嵌入每批）而非杀线程——避免 SQLite 事务处于未定义状态。语义耐人寻味的一条：**worker 返回 Ok 即记 completed，即使取消旗标已置位**（`jobs/mod.rs:148-150`）——"工作确实做完了就如实记"，取消语义的责任交给任务体，状态机不猜。

进度事件节流（≥100ms 或 ≥1% 或 stage 变化）与 DB 落库共用阈值，且 DB 写用 `get_timeout(200ms)` 拿不到连接就只发事件（`jobs/mod.rs:90-93`）——8 连接小池下进度写永不阻塞任务体，`import_service.rs:155` 注释表明这是被并行导入抢连接的教训倒逼出来的。

配置四层合并（内置 < 用户全局 < 工作区 < 单次任务）有个不对称设计：前三层 JSON 深合并且 **fail-loud**（类型错报 InvalidConfig 而非静默回落，`config.rs:138-139` 注释："避免用户以为设置生效了"）；第四层却用强类型逐字段 unwrap_or + clamp（`commands/compare.rs:73-91`）——换编译期核对，代价是新增配置项改两处。

语义降级路径是"离线优先"哲学的落点：模型缺失/联网被禁 → 返回 (None, degraded=true) → 精排退回词面权重组 → `semantic_degraded` 一路带进报告（`compare_service.rs:401-405`）——"有结果但注明局限"优于"无结果"。

### 4.3 数据层：迁移纪律与写放大控制

迁移用 PRAGMA user_version + append-only 的 const SQL 数组（V1→V8），每步独立事务，version 超前拒开库报"请升级应用"（`migrations.rs:17-33`）——每条迁移头部写明业务动机（V3 解释解析配置指纹缺失会导致缓存错误复用，V8 解释前缀策略变更为何必须清 embedding 缓存且为何无损）。这种迁移纪律很多成熟项目都做不到。

仓储层的函数式约定（传入连接、纯 SQL、**不私开事务**，`repo/mod.rs:1`）把原子性边界上提给服务层——"status='parsed' 与 chunks 存在性永远一致"这类不变式被显式写出。两级内容寻址缓存是离线单机应用的关键性能杠杆：分块按 `(file_hash, options_hash)` 跨工作区复制，向量按 `(normalized_hash, model_id)` 全局命中且 INSERT OR IGNORE 幂等、每批即时落库（取消不丢已算向量，`embedding_repo.rs:59-73`）。

崩溃自愈闭环完整：启动时 `mark_stale_as_failed` + 孤儿 parsing 文档修复（`lib.rs:62-71`），推理链写在注释里——"导入任务不可能跨进程存活"。配合失败即删的兜底，任意时刻杀进程不留脏数据。

### 4.4 命令面与安全：收敛式攻击面

48 个命令严格薄壳（最大 compare.rs 也只 238 行）。安全策略是把文件访问从"声明式 scope"改为"**审计过的具体端点**"：不装 fs/shell/http 插件，`read_document_file` 按 document_id 间接寻址读盘（`document.rs:79` 注释："比放开 asset 协议全盘 scope 更收敛"）。联网出口全仓只有 3 处，各有门控或显式授权语义。服务端不信任前端：枚举白名单、数值 clamp、配置入库前全量 resolve 校验。更新走 minisign 签名闭环（公钥内置 `tauri.conf.json:31`）。

### 4.5 前端：把 TanStack Query 重配成 IPC 缓存层

两个关键决策定调：`networkMode:'always'` + `retry:false`（`main.tsx:24-28`）——invoke 走本地 IPC 不经网络，离线机器上绝不能因 navigator.onLine=false 挂起查询；本地命令失败是确定性的，重试只拖慢报错。HashRouter 而非 BrowserRouter（`router.tsx:1`：Tauri 生产用自定义协议，刷新会丢路径）。

数据通路四层分明（screen → 43 hooks → 47 个"一 command 一函数"薄封装 → call()），乐观更新是教科书级实现（`data.ts:308-350`：cancelQueries→快照→同步改写详情缓存与全部 filter 变体的无限分页缓存→逐键回滚→settled 兜底）。进度用事件+轮询双通道，不信任单一通道。设计系统自绘弃 Arco，运行时依赖仅 14 个。大数据渲染有对应手段：万级条款组走 60 条/页 + react-virtual 虚拟滚动（`ClustersScreen.tsx:58-70`）。字号缩放走 webview setZoom 而非改 rem（`theme.tsx:69`："等比放大不撑破布局"）。

---

## 5. 工程质量现状

**测试**：Rust 118 个 `#[test]` 分布 25 文件，引擎层最重（parse 20、chunker 13）；7 个 `#[ignore]` 慢测试每个注释精确复现命令。测试的独特之处在于它是"**校准门禁**"：`compare_service.rs:794-795` 注释明言"权重/阈值改动必须正负向同时通过"；`test_fixtures.rs` 用 zip writer 手造合法 OOXML 让服务层测试穿过真实解析器；连"内置默认值是否匹配设计文档"都有断言（`config.rs:160-170`）。文件库并发写、DB 重开持久化、跨格式事实冲突端到端、真实语料校准、perf_smoke 性能冒烟均有覆盖。**前端测试是明显洼地**：仅 4 个 vitest 文件 26 用例 186 行，全是 utils 纯函数，无任何组件/交互测试。

**CI**：单 macOS runner 五步门禁（tsc 兼任前端 lint / vitest / clippy `-D warnings` / cargo test --lib），选 macOS 是为了让 pdfium/ONNX 测试真跑而非被环境缺失架空（`ci.yml:10-11` 注释写明动机）。无 Rust 编译缓存，8623 行 Cargo.lock 每次冷编译；用 `npm install` 而非 `npm ci`。

**发布**：tag 触发三平台矩阵 → tauri-action → 草稿 Release 安全阀（`release.yml:58`，人工发布草稿后 latest.json 才生效）→ minisign 签名的应用内更新。版本号三处手工同步无校验脚本（v0.4.0 changelog 里"关于区版本号硬编码"修复说明此坑已踩过）。

**文档**：设计文档 2220 行，§28 附录持续记录实现偏差且**自曝已知缺口**（如"OCR 静默回落"），业务承诺经 224 条逐条审计——文档-实现一致性罕见地好。

---

## 6. 架构优势：这个设计做对了什么

1. **误报控制是体系而非补丁**。五道闸门（模板剔除/子集非冲突/阵营归一/短文本加阈/低置信转 review）+ 不可测维度重分配 + "打分即过滤但保留最高分旁路"，每条规则有业务理由注释和正负向测试。对一个结论会写进评标报告的工具，这是最重要的正确性投资。
2. **分层纪律换来可测性**。commands 薄壳 → services 零 Tauri 依赖 → engine 纯函数，全管线可离线单测；测试因此能测真不变式（取消不留半成品、级联删除、状态机守卫）而非 mock 出来的空转。
3. **降级路径全部显式且用户可见**。语义降级标 `semantic_degraded` 进报告、PDF 三级回落留痕 method 字段、OCR 取消不产出半截结果（`ocr.rs:206-207`）、不支持的 .doc 报错给出"另存为 .docx"的出路——没有静默退化（除 §7 列出的两处例外）。
4. **两级内容寻址缓存把重算成本压到极限**：同一段落跨文档/跨任务/跨工作区零成本命中向量；同一文件换工作区免解析。对"评标现场反复调参数重跑"的使用模式，这是体验层面的决定性优化。
5. **注释回答"为什么"**。scoring.rs 头注释算误报账、config.rs 解释为何日志开关不该存在、import_service 记录连接池饿死教训、迁移写业务动机——考古成本极低，这本身就是架构资产。
6. **攻击面收敛与威胁模型意识**：把"标书文件是不可信输入"想清楚了（`MdView.tsx:1-31` 直接写出恶意 md → webview → IPC 的攻击链并 DOMPurify 消毒；OCR 下载先写 .part 再 rename）。

---

## 7. 风险与短板（按严重度排序）

**S1 — 业务结论可信度类**

1. **围标权重与分级线全是未校准魔法数**：0.4/0.25/0.15/0.1 各信号权重与 0.6/0.35/0.1 分级线（`collusion.rs:26-115`）无任何标注样本或实案回测支撑。"high"是本产品最重的输出，其可信度目前只有直觉背书。
2. **大写金额盲区**：CN_DIGITS 不含法定大写"壹贰叁肆伍陆柒捌玖拾佰仟"（`normalize.rs:91`），金额正则不识别千分位与"¥"前缀（`features.rs:110`）——"人民币壹佰万元整（¥1,000,000.00）"两种写法都抽不出实体，**事实冲突检测对最正式的报价条款失明**。chunker 测试里恰有"壹佰万元整"样例却只测了大小写（`chunker.rs:560`），未暴露此问题。
3. **报价梯度信号取"全文最大金额"当投标价**（`compare_service.rs:641-655`）：注册资本、历史业绩合同额会劫持该值，既漏报真陪标价也可能拿注册资本巧合误报，缺"投标报价"上下文锚定。
4. **共有特征词信号名不副实**：文案说"罕见特征词"（`collusion.rs:73`），实现只有"≥4 字且被 ≥2 文档共用"，无任何 IDF 过滤（`compare_service.rs:766-783`）——"技术方案""项目管理"必然凑满 5 个，信号④实为**常开的 +0.1 噪声权重**。同类问题：信号③文案称"作者/修改人/制作软件一致"但实现只比 author/last_modified_by（`fingerprint.rs:12-19`），且不关联"同源的是哪几份"与其他信号指向是否一致；办公环境默认用户名（Administrator）可稳定误触发。
5. **日期粒度误报**："2026年6月"与"2026年6月10日"互不为子集 → 兼容的粗细粒度被判 high 冲突（`fact.rs:195-208`）。

**S2 — 对抗鲁棒性与正确性类**

6. **扫描件 PDF 静默截断 20 页**（`parse.rs:412-414`）：第 21 页起完全不参与比对且无任何警告——对"把关键差异藏在后半本"的对抗场景是实质漏洞。同族问题：OCR medium 档未就绪静默回落 small 且 parse_method 仍记 'ocr'（设计文档 §28.7 自认）。
7. **离线承诺的缝隙**：`embed::ensure` 用 `model_cached()`（任一模型有 .onnx 即真）做下载闸门（`embed.rs:168`）——缓存了模型 A、禁联网时选未缓存的模型 B，闸门误放行触发联网尝试。
8. **cancel 竞态可把 completed 覆写成 cancelled**：`JobManager::cancel` 的孤儿分支调用无状态守卫的 `finish`（`jobs/mod.rs:232-241`, `job_repo.rs:134-139`）；另 spawn 的 has_active 检查非原子（双击即可建重复任务，`jobs/mod.rs:183-192`）。

**S3 — 安全纵深类**

9. **CSP 为 null**（`tauri.conf.json:26`）：XSS 防线只剩 React 转义 + DOMPurify 一层，任何渲染库（如 docx-preview）失守即可 invoke 全部 48 个命令。且存在放大链：`export_report` 接受任意写盘路径无 scope 校验 + opener 权限 path 为 `"**"`（`capabilities/default.json:12-19`）——webview 失守时"任意路径写文件 + 任意路径交系统打开"可落地执行。
10. **模型下载无完整性校验**（`ocr.rs:151-171` 只认 tar 内第一个 .onnx）：对结论敏感的取证工具，被投毒的模型可系统性压低相似度。

**S4 — 性能与可维护性类**

11. **embedding 召回是 O(N²) 暴力余弦**（`candidate.rs:151-166`），无 ANN/分桶——句子级粒度 10k chunk 即 5000 万次高维余弦，是五通道里唯一没有索引结构的性能悬崖；`cohesive_split` 最坏 O(E²)（`clustering.rs:85-117`）是第二个。
12. **candidate_edges 是只写表**：6 分数列 + 4 索引（其中 3 个无任何消费者，`migrations.rs:150-153`），却是比对事务最大写入项。
13. **前端积债**：无键 `invalidateQueries()` 连 staleTime:Infinity 的文档 ArrayBuffer 都重取（`progressStore.ts:50`）；render 体内调 `fetchNextPage()`（`ClustersScreen.tsx:66-70`）；暗色配色未进 tokens 全仓手写三元；设置双源（localStorage 与 DB）并存只迁移了一半（`main.tsx:33-55`）；"span 当 button"+ 交互控件嵌套违反 ARIA 蔓延 10+ 文件。
14. **发布链路缺口**：CI 只编译 macOS，Windows/Linux 首次编译验证在打 tag 时；`binaries/` 无 libpdfium.so → **Linux 永远静默走 pdf-extract 弱回落**（`parse.rs:484-492`）；BUILD.md 声称 universal 实际只产 aarch64，Intel Mac 无包。

---

## 8. 到 v1.0 的演进建议（架构视角）

**第一优先：让"high"结论配得上公信力（对应 S1）**
- 建立**围标判定校准集**：收集/构造有标注的真实案例语料，把 `collusion.rs` 的五组权重与三条分级线从魔法数变成回测过的参数；现有的 calibrate_real_corpus 门禁机制是现成骨架，扩充它。
- 补大写金额路径：CN_DIGITS 扩 "壹…仟"、金额正则加千分位/¥ 前缀，与现有"五万元/5万元"对称归一测试并轨——这是改动小、业务收益极高的单点修复。
- 报价梯度加"投标报价/报价总额"上下文锚定，找不到锚点时降权而非取全文最大值；共有特征词用已有的 TF-IDF/DF 基础设施加罕见度过滤（语料 DF 已在 `corpus.rs` 现算，成本近零）；日期冲突加粒度兼容判定（前缀包含即兼容）。
- 元数据同源信号从"计数"改为"文档对归属"，与条款雷同信号做同对交叉验证后再计权。

**第二优先：堵对抗与承诺缺口（对应 S2/S3）**
- 扫描件 20 页截断改为可配置 + 报告显著警示"仅比对前 N 页"；OCR 档位回落写真实 method。
- `model_cached()` → `model_cached_for(spec)` 一行修复离线闸门；`finish` 加状态守卫、spawn 冲突检查用唯一索引原子化。
- 设最小 CSP（script-src 'self'）、opener scope 收敛到导出目录、模型下载加 sha256 清单——三项都是配置级改动。
- 把 embedding 缓存键的教训吸收进设计：策略版本编进 model_id，根治"改前缀就得破坏性迁移"。

**第三优先：性能天花板与数据层清理（对应 S4）**
- embedding 召回换 ANN（HNSW 或按 MinHash 桶预分组），与其余四通道的索引结构对齐；cohesive_split 换批量删边或限制迭代轮数。
- candidate_edges 砍掉或只留调试开关 + 删 3 个无消费者索引；`insert_many` 包事务（对齐 template_repo 已有的正确写法）。

**第四优先：前端与工程收尾**
- 暗色令牌进 tokens.ts、设置收敛到 DB 单源、失效改精确键族、原生 `<button>` 化——这些是 v1.0 前必须止血的漂移，越晚越贵。
- CI 补三平台编译（至少 cargo check）+ rust-cache + npm ci + 版本号一致性脚本；补 libpdfium.so 与 macOS universal，让"三平台支持"名实相符；给 7 个 `#[ignore]` 测试建 nightly/发布前定时执行。
- 补业务闭环：复核进度统计与"已确认结论进报告"（export 装配区分 review_status），这是评审工作流的最后一公里。

**总评**：这是一个业务理解深度显著超过代码规模的项目——领域适配（表格行原子化、阵营归一、报价梯度）、误报控制体系和"为什么"注释文化是真正的护城河；分层与测试纪律给了它快速演进的底盘。当前最大的差距不在架构而在**判定可信度的实证基础**（未校准的围标权重、大写金额盲区）与**对抗鲁棒性**（20 页截断、CSP）——到 v1.0 的路线应该是"先让结论经得起质询，再扩能力面"。