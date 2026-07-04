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
```

## 本地打包

```bash
npm run tauri build            # 当前平台产物（.app/.dmg、.msi/.exe、.deb/.AppImage）
```

产物位于 `src-tauri/target/release/bundle/`。

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

1. **随包内置**（推荐默认档 `bge-small-zh-v1.5`，~95MB）：把 5 个文件放进
   `src-tauri/models/embeddings/<id>/` 即随安装包内置、开箱离线可用。放法与提取步骤见
   `src-tauri/models/embeddings/README.md`。运行时以 fastembed user-defined 方式加载，pooling
   对齐后向量与下载版逐位等价。
2. **自托管下载**（大模型 bge-large/e5-*，1~2GB 不宜内置）：把 5 个文件打成 `.tar` 传到可控 URL，
   填 `embed.rs` 的 `EmbedModelSpec.download_url`；工具屏「下载」即走该源（离线内网友好），落地
   `~/.cache/bidguard/embeddings/<id>/`。
3. **HF 联网下载**（回落）：1/2 都没有时，`security.allowCloudModel=true` 下由 fastembed 从
   HuggingFace 拉取，缓存到 `~/.cache/bidguard/fastembed/`（应在 `.gitignore` 排除）。默认关闭。

## CI / 发布

- `.github/workflows/ci.yml`：push / PR 时跑前端构建 + 引擎测试（macOS runner）。
- `.github/workflows/release.yml`：打 `v*` tag 或手动触发 → macOS(arm64) / Windows(x64)
  构建并发布为 GitHub Release 草稿。

```bash
git tag v0.1.0 && git push origin v0.1.0    # 触发构建发布
```

### macOS 代码签名 / 公证（可选，配置后自动启用）

在仓库 Settings → Secrets 配置后，`release.yml` 自动签名公证：

| Secret | 说明 |
|---|---|
| `APPLE_CERTIFICATE` | base64 的 Developer ID Application 证书(.p12) |
| `APPLE_CERTIFICATE_PASSWORD` | 证书密码 |
| `APPLE_SIGNING_IDENTITY` | 形如 `Developer ID Application: Name (TEAMID)` |
| `APPLE_ID` / `APPLE_PASSWORD` / `APPLE_TEAM_ID` | 公证用 Apple ID、专用密码、团队 ID |

未配置时产出未签名包（本地可用，分发到他机需用户手动放行 Gatekeeper）。

### 自动更新（可选，需一次性配置）

1. 生成更新签名密钥：`npm run tauri signer generate -- -w ~/.bidguard/updater.key`
2. 把生成的 **公钥** 填入 `tauri.conf.json` 的 `plugins.updater.pubkey`，并设置
   `endpoints`（指向发布的 `latest.json`）。
3. 把 **私钥** 与其密码加入仓库 Secrets：`TAURI_SIGNING_PRIVATE_KEY`、
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`（`release.yml` 已预留）。
4. 安装 `@tauri-apps/plugin-updater` 与 `tauri-plugin-updater`，在启动时检查更新。

> 自动更新依赖你的发布托管（GitHub Release / 自有服务器），故默认未开启，按需配置。
