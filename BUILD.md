# 构建与发布 · 原本 · 标书查重 (BidGuard)

跨平台桌面应用：Tauri 2 + React 19 + Rust。全程本地处理，不上传任何文件。

## 开发

```bash
npm install
npm run tauri dev      # 启动桌面应用（热重载）
npm run dev            # 仅前端（浏览器预览，降级到演示数据）
```

## 测试

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib                 # 引擎单元测试
cargo test --manifest-path src-tauri/Cargo.toml --lib -- --ignored    # 含语义/OCR（较慢，需模型）
npm run build                                                          # 前端类型检查 + 打包
npm run lint                                                           # ESLint（CI 会卡 error）
```

**语料回归门禁**——改动检测算法（阈值 / 权重 / 归一化 / 特征）前后必跑，CI 同样会跑：

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib --features dev-tools corpus_regression
```

它在内置合成对抗语料上比对召回率、per-label F1 与围标 AUC，任一指标漂移即失败。指标确应变化时用
`BIDGUARD_WRITE_BASELINE=1` 重跑同一命令重写基线，并在提交信息里写明原因（基线 diff 进评审正是其用意）。
语料本身可重新生成：

```bash
cargo run --example corpusgen --features dev-tools            # 段对语料 + 文档集
cargo run --example corpusgen --features dev-tools -- fit-collusion   # 重新拟合围标融合权重
cargo run --example corpusgen --features dev-tools -- fit-calib       # 重新拟合校准与三带阈值
```

## 本地打包

```bash
npm run tauri build            # 当前平台产物（.app/.dmg 或 .msi/.exe）
```

产物位于 `src-tauri/target/release/bundle/`。本机无更新签名私钥时结尾会报
`A public key has been found, but no private key`——包已打好，仅未生成更新产物；要跳过可加
`--config '{"bundle":{"createUpdaterArtifacts":false}}'`（CI 打包冒烟即如此）。

> ⚠️ **包内必须只有一个 bin**。tauri 从 Cargo 清单的 bin 目标里挑主程序，且**不看
> `required-features`**——多出一个 bin 就可能选错并在打包阶段失败（v0.6.0 首次发布即因 dev-tools
> 门控的 `corpusgen` 被当成主程序而三平台全挂）。开发用 CLI 一律声明为 `[[example]]`；CI 有
> 「单一应用二进制守卫」在提交时拦截。

## 随包原生资源（已入库，开箱即用）

- `src-tauri/binaries/`：`libpdfium.dylib`(macOS) / `pdfium.dll`(Windows x64) —— 鲁棒 PDF 解析。
  ⚠️ **macOS 仅 arm64(Apple 芯片)**：Intel/x86 不支持——ort/ONNX Runtime 无 x86_64-apple-darwin
  预编译库，Intel 支持需从源码编译 ONNX Runtime，不提供。
  ⚠️ **Windows 仅 x64**：Win11-on-ARM 可用系统自带 x64 仿真运行此 x64 包（Win10-on-ARM 无 x64
  仿真，不支持）。暂不出原生 arm64——并非 ort 所限（ort 确有 `aarch64-pc-windows-msvc` 静态预编译库），
  而是需 MSVC ARM64 工具链 + 仅 NSIS 打包（WiX/MSI 不支持 arm64）+ 换 arm64 版 `pdfium.dll`，成本暂不做。
  ⚠️ **Linux 暂不构建**（用户决定）；若恢复需补 `libpdfium.so`(pdfium-binaries，选 linux-x64)。
- `src-tauri/models/`：PaddleOCR ONNX（检测 + 识别）+ 中文字典 —— 扫描件 OCR

以上通过 `tauri.conf.json` 的 `bundle.resources`（`models/**/*`）打进安装包；运行时按候选目录解析
（dev：`src-tauri/`；macOS：`*.app/Contents/Resources`；Windows：exe 同级；Linux：`../lib`）。

### 语义 embedding 模型（三种来源，按优先级）

1. **随包内置**（默认档 `bge-small-zh-v1.5`，~90MB，**语义比对默认开启依赖它**）：

   ```bash
   ./scripts/fetch-embedding-model.sh    # 从 HF 拉 5 个文件，逐文件 sha256 校验后落位
   ```

   落进 `src-tauri/models/embeddings/bge-small-zh-v1.5/`（模型不入 git，本地打包前跑一次即可，
   幂等；HF 不可达时 `BIDGUARD_HF_BASE=https://hf-mirror.com` 走镜像，摘要不变）。release.yml
   已内置该前置步骤——校验失败即终止发布，不会出「静默无语义」的安装包。运行时以 fastembed
   user-defined 方式加载，pooling 对齐后向量与下载版逐位等价；细节见
   `src-tauri/models/embeddings/README.md`。
2. **自托管下载**（大模型 bge-large/e5-*，1~2GB 不宜内置）：把 5 个文件打成 `.tar` 传到可控 URL，
   填 `embed.rs` 的 `EmbedModelSpec.download_url`；工具屏「下载」即走该源（离线内网友好），落地
   `~/.cache/bidguard/embeddings/<id>/`。
3. **HF 联网下载**（回落）：1/2 都没有时，`security.allowCloudModel=true` 下由 fastembed 从
   HuggingFace 拉取，缓存到 `~/.cache/bidguard/fastembed/`（应在 `.gitignore` 排除）。默认关闭。

## CI / 发布

- `.github/workflows/ci.yml`（push / PR；同 ref 新 push 取消旧运行）：
  - `test`（macOS）：npm 依赖审计（high 即失败）、前端类型检查 + 构建、ESLint、前端单测、
    Clippy（`-D warnings`）、引擎单测、集成测试（`--tests`，许可闭环用临时密钥对恒运行，
    CI 永不持有签发私钥）、语料回归门禁（与引擎单测同特征集，不重复编译）。
  - `cross-check`（Ubuntu + Windows）：Windows 跑引擎测试验证平台分叉（路径分隔 / GBK 文件名 / pdfium.dll）；
    Linux 编译检查 + **单一应用二进制守卫** + **打包冒烟**（debug 打到 `.deb`，覆盖资源路径 / 配置 schema /
    beforeBuildCommand）。冒烟是补上「打包问题只在打 tag 才暴露」的缺口。
  - `audit`：Rust 依赖供应链审计（`rustsec/audit-check`，阻断；需 `checks: write` 权限才能写 check-run）。
    npm 侧由 `test` job 的 `npm audit --audit-level=high` 对齐覆盖。
- `.github/workflows/release.yml`：打 `v*` tag 或手动触发 → **preflight 守卫**（内嵌验签公钥
  仍是开发钥 `lic-dev-*` 时直接失败，堵「带开发钥出厂」）→ macOS(arm64) / Windows(x64)
  构建（带 rust-cache）并发布为 GitHub Release **草稿**（人工点发布才生效，是最后一道闸门）。

```bash
git tag -a v0.6.0 -m "..." && git push origin v0.6.0    # 触发构建发布
```

发布前先过 `docs/license-activation-scheme-v2.md` 的**发布前检查清单**——其 A 组（许可验签密钥换发）
是硬性阻断项：当前二进制内嵌的仍是开发公钥 `lic-dev-2026a`，未换发即发布意味着任何持有该私钥的人
都能伪造合法许可。

### 代码签名（可选，配置 secrets 后自动启用；均未配置时照常产出未签名包）

`release.yml` 内有「secrets 非空才注入」的启用步骤（tauri CLI 对空串证书 env 行为未定义，
静态映射会让未配证书的发布翻车，故按需注入）。在仓库 Settings → Secrets 配置：

**macOS 签名 + 公证：**

| Secret | 说明 |
|---|---|
| `APPLE_CERTIFICATE` | base64 的 Developer ID Application 证书(.p12) |
| `APPLE_CERTIFICATE_PASSWORD` | 证书密码 |
| `APPLE_SIGNING_IDENTITY` | 形如 `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` | 公证用 Apple ID、App 专用密码、团队 ID |

（`KEYCHAIN_PASSWORD` 为 runner 一次性钥匙串口令，工作流现场生成，无需配置。）

**Windows Authenticode：**

| Secret | 说明 |
|---|---|
| `WINDOWS_CERTIFICATE` | base64 的代码签名证书(.pfx) |
| `WINDOWS_CERTIFICATE_PASSWORD` | .pfx 导出密码 |

工作流把证书导入 runner 证书库后，以 `--config` 文件注入 `certificateThumbprint`
（sha256 + digicert 时间戳）——证书轮换只换 secrets，不改仓库。

未配置对应平台的 secrets 时：macOS 包需用户手动放行 Gatekeeper，Windows 包会被
SmartScreen 提示未知发布者。

### 自动更新（**已启用**）

已完成配置，无需再设：`tauri.conf.json` 的 `plugins.updater.pubkey` 已填、`endpoints` 指向
`releases/latest/download/latest.json`，`bundle.createUpdaterArtifacts=true`，前后端插件已装，
签名私钥在仓库 Secrets（`TAURI_SIGNING_PRIVATE_KEY` / `_PASSWORD`）。

运作方式与注意事项：

- `release.yml` 随产物生成 `latest.json` 与各平台 `.sig`；**草稿发布后**该 `latest.json` 才对旧版本生效——
  这正是草稿式发布作为闸门的意义：点发布 = 向存量安装推送更新。
- 更新签名密钥与**许可验签密钥是两套东西**，不要混用：前者可进 GH Secrets，后者（license 私钥）
  **绝不可**进任何仓库或 CI，见 `docs/license-activation-scheme-v2.md`。
- 本地打包无私钥会在末尾报错，属预期；见「本地打包」一节的跳过写法。
