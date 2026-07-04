# 原本 · 标书查重 (BidGuard)

面向评标专家、招标代理与监管审计人员的**围标串标取证工具**。一次导入 2~10 份不同投标人的标书，交叉找出雷同条款、事实矛盾与围标迹象，输出可举证的报告。

> 与知网 / PaparPass 等「单文档 vs 文献库」的自查工具根本不同：BidGuard 做的是 **N 份标书互查**——证据在投标人之间，不在文献库里。

## 核心价值观

- **宁转人工不误告**：模板剔除、子集非冲突、阵营归一、短文本加阈、低置信转复核，五道误报闸门层层设防——误报直接损害评审公信力。
- **全程离线**：标书是高敏感商业文件，评标现场普遍禁外网。全栈进程内运行（PDF 解析、OCR、语义 embedding 均为纯 Rust，无 sidecar）；唯一联网点是可选的模型下载，受 `security.allowCloudModel` 闸门控制，**默认关闭**。
- **日志永不记录标书正文**：刻意做成不可配置——「可配置反而暗示存在记录正文的路径」。

## 能做什么

- **多格式导入**：docx / PDF（含扫描件 OCR）/ Excel 报价表，自动去重、解析、分块。
- **8 阶段比对管线**：五通道召回 → 五维（可选六维语义）加权精排 → 并查集聚类 → 八类证据强度分层 → 事实冲突检测（金额 / 日期 / 工期 / 责任主体）→ 报价梯度与围标信号。
- **可举证结果**：相似度矩阵、逐对高亮对比、重复条款聚合、围标结论；投标人以十天干（甲乙丙丁…）中立化匿名编号。
- **人工复核 + 六格式导出**：三态复核（待定 / 确认 / 排除），导出 HTML / Word / Excel / CSV / Markdown / JSON 归档。

## 技术栈

Tauri 2 + React 19 + TypeScript（Vite）前端；Rust 引擎（`src-tauri/src/engine/`，零 Tauri 依赖，可独立测试）；SQLite 单文件（WAL）本地存储。

## 下载与安装

从 [GitHub Releases](../../releases) 获取安装包（发布为草稿，正式发布后自动更新才对旧版本生效）：

| 平台 | 产物 | 说明 |
|---|---|---|
| macOS（Apple 芯片 / arm64） | `BidGuard_x.y.z_aarch64.dmg` | **Intel Mac 不支持**——ort/ONNX Runtime 无 x86_64-apple-darwin 预编译库 |
| Windows（x64） | `BidGuard_x.y.z_x64-setup.exe` / `_x64_en-US.msi` | Win11-on-ARM 可用系统自带 x64 仿真运行；暂不出原生 arm64 |

Linux 暂不构建（详见 [BUILD.md](BUILD.md)）。

## 开发与构建

```bash
npm install
npm run tauri dev      # 启动桌面应用（热重载）
npm test               # 前端单元测试
cargo test --manifest-path src-tauri/Cargo.toml --lib   # 引擎单元测试
npm run tauri build    # 打包当前平台产物
```

构建策略、随包原生资源（pdfium / OCR 模型）、平台限制与其技术原因详见 **[BUILD.md](BUILD.md)**。版本变更记录见 **[CHANGELOG.md](CHANGELOG.md)**，深度业务与架构分析见 **[docs/](docs/)**。
