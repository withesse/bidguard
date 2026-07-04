# 安全说明 / Security Notes

BidGuard 全程本地处理，不上传任何文件。本文记录威胁模型与几处刻意的安全权衡，供审阅参考。

## 威胁模型

标书来自**外部投标方，默认视为不可信**。核心防护目标：即便某份标书内嵌恶意内容（脚本、公式、链接），也不能借渲染/导出/预览逃逸到本机文件系统或外部网络。

## 纵深防御现状

- **CSP**（`tauri.conf.json`）：生产 `script-src 'self'`（无 `unsafe-inline`）、`object-src 'none'`、`connect-src` 仅 `self` + `ipc`——在 webview 层强制离线，阻断 XSS 外传。
- **Markdown 渲染**：`MdView` 用 DOMPurify 白名单消毒（仅排版标签、`href`/`title`，禁 `data:` 与事件属性）。
- **原文版式链接**：`DocxView`/`MdView` 容器拦截 `<a>` 点击，`http(s)` 交系统浏览器外部打开、其余 scheme 丢弃，杜绝 webview 就地导航到钓鱼页（见 `src/components`）。
- **导出写盘**：扩展名白名单（html/docx/xlsx/json/md/csv），按「即便 webview 被攻陷传入任意 path」建模。
- **CSV 导出**：中和公式注入前导字符（`= + - @` TAB/CR），防 Excel/WPS 打开时执行公式（CWE-1236）。
- **任意文件读**：`read_text_file` 限定扩展名与大小上限，收敛任意路径读原语。
- **模型下载 / 更新**：受 `security.allowCloudModel`（默认关闭）闸门；updater 用内置 minisign 公钥校验签名。
- **日志**：永不记录标书正文（不可配置）；生产日志仅任务 ID / 计数 / 错误码。

## 刻意的权衡

### opener scope 为 `**`

`capabilities/default.json` 中 `opener:allow-open-path` / `opener:allow-reveal-item-in-dir` 的 scope 为通配 `**`。

- **原因**：标书常从 U 盘 / 网络盘 / 外部卷导入，导出也可能保存到任意卷。收窄到 `$HOME/**` 会使这些正常场景无法「用系统程序打开」/「在文件管理器中定位」（曾于收窄后回退，见 commit `7fe84a8`）。
- **残余风险**：被攻陷的 webview 可对任意本地路径发起系统「打开」（Windows/macOS 上对 `.exe`/`.app` 即执行），是「XSS → 本地执行」放大链的末端。
- **缓解**：该链的前置（脚本执行、外部导航、内联注入）已被上文 CSP + DOMPurify + 链接拦截多道压制；opener 是纵深防御的最后一环而非活漏洞。
- **后续可选强化**：把 open/reveal 改为服务端按文档 id / 导出记录解析路径的自定义命令（消除「webview 直接传任意 path」向量，同时保留外部卷可达）——尚未实施。

## 报告漏洞

发现安全问题请通过仓库 Issue 或维护者邮箱私下告知，勿公开 PoC 细节。
