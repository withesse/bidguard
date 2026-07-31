# 原本 · 标书查重 (BidGuard)

面向评标专家、招标代理与监管审计人员的**围标串标取证工具**。一次导入 2~10 份不同投标人的标书，交叉找出雷同条款、事实矛盾与围标迹象，输出可举证的报告。

> 与知网 / PaparPass 等「单文档 vs 文献库」的自查工具根本不同：BidGuard 做的是 **N 份标书互查**——证据在投标人之间，不在文献库里。

## 核心价值观

- **宁转人工不误告**：导入招标文件后，投标方对其逐字应答的段落被整段剔除——这是标书场景最大的一类误报源；其上再叠模板剔除、行业范本豁免、子集非冲突、阵营归一、短文本加阈、低置信转复核。误报直接损害评审公信力。
- **机器举证，人下结论**：所有结论均为线索级，措辞不作定性；命中项一律附「未命中不构成清白证明」与该痕迹的可清除方式，报告固定含「检查方法与局限」一节——避免「没有取证章节」被读成「查过了，干净」。
- **全程离线**：标书是高敏感商业文件，评标现场普遍禁外网。全栈进程内运行（PDF 解析、OCR、语义 embedding 均为纯 Rust，无 sidecar）；唯一联网点是可选的模型下载，受 `security.allowCloudModel` 闸门控制，**默认关闭**。
- **日志永不记录标书正文**：刻意做成不可配置——「可配置反而暗示存在记录正文的路径」。

## 能做什么

- **多格式导入**：docx / PDF（含扫描件 OCR）/ Excel 报价表，自动去重、解析、分块；招标文件与补遗单独成角色，用于剥离合法共享内容。
- **文本证据**：五通道召回 → 多维加权精排（可选语义）→ 聚类 → 八类差异分层；逐字雷同区间（「甲第 3.2 节与乙第 3.2 节 800 字逐字相同」）与连续对齐区段（覆盖率 + 三级高亮）；事实冲突检测（金额 / 日期 / 工期 / 责任主体）。
- **取证证据**（与文本正交，改抬头换措辞洗不掉）：docx 修订标识 rsid 与包结构指纹、PDF 血缘（XMP GUID / trailer ID / 字体子集）、内嵌图片同源、共同错误指纹。
- **数值证据**：工程量清单按编码跨文档对齐，逐项单价雷同率、共享算术错误（错且错得逐分一致）、等差/等比折扣规律与单价相关性散点。
- **入口对抗**：零宽字符与同形字归一、PDF 隐藏文字层审计、渲染-OCR 交叉验证——识别「渲染给评标人一套、抽给查重系统另一套」的规避手法。
- **可举证结果**：相似度矩阵（对减前后双口径）、逐对高亮、条款聚合、以证据强度口头等级呈现的围标结论；条款按「低优先级抽查 / 需人工复核 / 重点标红」排队；投标人以十天干（甲乙丙丁…）中立化匿名编号。
- **人工复核 + 六格式导出**：三态复核（待定 / 确认 / 排除），导出 HTML / Word / Excel / CSV / Markdown / JSON，各格式证据章节齐平。

## 技术栈

Tauri 2 + React 19 + TypeScript（Vite）前端；Rust 引擎（`src-tauri/src/engine/`，零 Tauri 依赖，可独立测试）；SQLite 单文件（WAL）本地存储。

## 下载与安装

从 [GitHub Releases](../../releases) 获取安装包：

| 平台 | 产物 | 说明 |
|---|---|---|
| macOS（Apple 芯片 / arm64） | `BidGuard_x.y.z_aarch64.dmg` | **Intel Mac 不支持**——ort/ONNX Runtime 无 x86_64-apple-darwin 预编译库 |
| Windows（x64） | `BidGuard_x.y.z_x64-setup.exe` / `_x64_en-US.msi` | Win11-on-ARM 可用系统自带 x64 仿真运行；暂不出原生 arm64 |

Linux 暂不构建（详见 [BUILD.md](BUILD.md)）。

首次启动自动发放 **7 天 / 10 次**本地试用；之后需导入许可文件激活（离线签名许可，无需联网校验）。比对任务按次扣减，失败或取消会自动退还。

## 开发与构建

```bash
npm install
npm run tauri dev      # 启动桌面应用（热重载）
npm test               # 前端单元测试
cargo test --manifest-path src-tauri/Cargo.toml --lib   # 引擎单元测试
npm run tauri build    # 打包当前平台产物
```

改动检测算法（阈值 / 权重 / 归一化）前后请跑**语料回归门禁**——它在内置合成对抗语料上比对召回率、分类 F1 与围标 AUC，指标漂移即失败，CI 同样会跑：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib --features dev-tools corpus_regression
```

指标确应变化时，用 `BIDGUARD_WRITE_BASELINE=1` 重跑同一命令重写基线并在提交信息里说明原因——基线 diff 进 PR 评审正是该机制的用意。开发用 CLI（语料生成、权重拟合）一律声明为 `[[example]]`：包内出现第二个 `bin` 会让 tauri 打包时选错主程序。

构建策略、随包原生资源（pdfium / OCR 模型）、平台限制与其技术原因详见 **[BUILD.md](BUILD.md)**；版本变更记录见 **[CHANGELOG.md](CHANGELOG.md)**。**[docs/](docs/)** 收录：比对方案与改进路线（`bid-comparison-scheme.md`）、学界与工业界方案调研（`bid-comparison-sota-survey.md`）、里程碑执行方案与设计明细（`bid-comparison-execution-plan.md`）、深度架构分析与授权激活方案。
