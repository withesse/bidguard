# BidGuard 注册激活与授权方案设计（v2 · 高安全版）

> 本文取代此前"离线 Ed25519 机器码 + 激活码"的简化提案。新增：试用期、在线激活、租约（lease）续期、防重置/防时钟回拨的纵深防御。所有结论均已吸收对抗性验证（verification）的修正，不再重复已被证伪的假设。

---

## 1. 概述与安全目标

### 1.1 威胁模型：三级攻击者

| 层级 | 画像 | 典型行为 | 本方案的目标 |
|---|---|---|---|
| T1 普通用户 | 试用到期继续用 | 重装、改时钟 | **完全阻止**（签名 + HWM + 服务端锚定） |
| T2 密钥分享者（**主要威胁**） | 付费代理机构把一份授权装 5 台机 / 转给同行 | 复制安装目录、共享 license 文件 | **有效阻止**（节点锁定 + 服务端席位计数 + 导出报告水印溯源） |
| T3 破解者 | 逆向、patch 二进制、NOP 校验分支 | 二进制补丁、Frida hook | **只能提高成本，无法阻止**（诚实承认） |

### 1.2 诚实的安全上限（必须内部对齐的三句话）

1. **二进制跑在攻击者机器上，任何本地校验在数学上都可被 patch 掉。** 密码学能做到的是：没有私钥就**造不出**合法 license（消灭 keygen 这一攻击类），攻击者被迫降级为改二进制——不可批量分发、对小众 B2B 工具动机极低。
2. **纯离线试用从根本上可被重置**（VM 快照回滚一次性重置所有本地状态，含 vTPM——vTPM 状态就是 VM 文件，已验证）。唯一稳健的防重置是**服务端锚定首次运行记录**；air-gapped 场景只能靠签名文件 + 人工发放摩擦来兜底。
3. **可撤销性 = min(lease TTL, 离线文件剩余寿命)。** 对永不联网的节点，撤销延迟等于离线文件寿命本身——所以离线文件寿命是安全参数，**绝不签发永久离线文件**。

### 1.3 指导原则

- **所有授权决策只在 Rust 层做**。React 前端不可信（可直接 `invoke()` 或替换前端），前端守卫仅为 UX。
- **信任签名，不信任传输**。完整性锚点是 payload 的 Ed25519 签名 + 内嵌公钥，不是 TLS。按 OWASP 现行指导**不做证书 pinning**（对机主为零防伪价值、且有 pin 轮换砸自己脚的运维风险）；普通 TLS 只用于对第三方网络攻击者保护客户数据。
- **强杀伤开关 = 服务端停止续签租约**（revocation by non-renewal）。联网节点 TTL 到期即死；离线节点靠短寿命签名文件 + 到期换发。
- **在线只做加严，永不做门槛**。全程离线是产品核心承诺（评标现场普遍禁外网），任何联网路径必须是可选增强，且断网时优雅降级到离线层。
- **纵深防御按 ROI 排序**：签名许可 > 服务端计数 > 节点锁定 > 报告水印溯源 > HWM 时钟防回拨 > 字符串混淆（obfstr）。**不做**重型混淆 / anti-debug / 自校验（对 Rust 工具链脆弱、易触发政企 AV 误报、收益趋零）。

---

## 2. 授权模型

### 2.1 四种商业形态的统一表达

所有形态用**同一个签名结构**表达（plan + 三个可选限制维度的组合），验证逻辑只有一份：

| 商业形态 | plan | expiresAt | maxUses | 示例 |
|---|---|---|---|---|
| 试用 | `trial` | 有（短） | 有（小） | 7 天 / 10 次，先到为准 |
| 按期 | `timed` | 有 | null | 1 年 |
| 按次 | `counted` | null 或有 | 有 | 200 次 |
| 期+次 | `timed_counted` | 有 | 有 | 1 年 200 次，先到为准 |
| 买断 | `perpetual` | null | null | 永久不限次（仍有 lease，见下） |

**组合语义**：所有存在的限制维度取交集（AND），任一命中即拒绝。次数以"成功完成的比对任务"计（失败/取消退还，见 §8.4）。

### 2.2 两层令牌：License（权利凭证）与 Lease（运行租约）

关键设计：**权利期限（entitlement term）与文件寿命（lease TTL）分离**——这是离线场景下唯一的撤销杠杆（已验证：Keygen/Cryptolens 模式）。

**License Token（长期，签发一次）** —— armored 文件 `bidguard.lic`：

```
-----BEGIN BIDGUARD LICENSE-----
base64url( header_json . payload_b64 . sig_b64 )
-----END BIDGUARD LICENSE-----
```

```jsonc
// header（明文，先于验签断言）
{ "v": 1, "alg": "Ed25519", "kid": "lic-2026a" }

// payload —— 签名覆盖的是 payload 的原始字节（base64url 内容），
// 验签通过后才反序列化，杜绝 JSON canonicalization 歧义
{
  "licenseId": "uuid-v4",
  "licenseeName": "XX招标代理有限公司",      // 同时用于导出报告水印
  "plan": "timed_counted",
  "issuedAt": "2026-07-10T00:00:00Z",       // 绝对 UTC 时刻（非"安装后 N 天"）
  "expiresAt": "2027-07-10T00:00:00Z",      // null = 不限期
  "maxUses": 200,                            // null = 不限次
  "maxMachines": 1,
  "machine": {                               // 节点锁定（离线签发时必填；在线激活由服务端回填进 lease）
    "anchorHash": "sha256hex",
    "componentHashes": ["h1","h2","h3","h4"],
    "matchThreshold": 2                      // anchor 必须命中 + 副件 ≥2/4
  },
  "policyProfile": "offline_strict",         // offline_strict | connected（按客户连通性分层签发）
  "leaseTtlDays": 90,                        // offline_strict: 90；connected: 14
  "graceDays": 14,                           // lease 过期后的宽限（只警告不拦截）
  "features": ["compare"]
}
```

**Lease Token（短期，可反复换发）** —— 由 License 派生的运行许可，`bidguard.lease`：

```jsonc
{ "v": 1, "alg": "Ed25519", "kid": "lic-2026a" }
{
  "licenseId": "…",                 // 必须与已装 License 一致
  "machineAnchorHash": "…",         // 绑定本机
  "issuedAt": "2026-07-10T08:00:00Z",
  "leaseExpiresAt": "2026-10-08T08:00:00Z",
  "remainingUses": 173,             // 在线模式下为服务端权威值，用于校正本地账本
  "serverTime": "2026-07-10T08:00:00Z",  // 可信时间锚点（推进本地 HWM）
  "nonceEcho": "base64(32B)"        // 回显客户端 nonce，防预生成/重放（离线文件交换同样携带）
}
```

### 2.3 续期与撤销模型

```
撤销延迟 = min(lease TTL, 离线文件剩余寿命)
```

| 客户分层 | leaseTtl | 续期方式 | 撤销 SLA |
|---|---|---|---|
| connected（办公网可出网） | 14 天 | 后台心跳自动换发 | ≤14 天 |
| offline_strict（纯内网/涉密） | 90 天 | U 盘/二维码离线换发（Let's Encrypt 式 sneakernet） | ≤90 天 |

- lease 到期 → 进入 `graceDays` 宽限（每次启动黄条警告"请续期"）→ 宽限结束 `start_compare` 拒绝，其余功能（查看历史结果、导出已完成报告）**不封锁**——绝不扣押客户已付费产出。
- lease 文件同时是**粗粒度可信时间信标**（已验证修正：air-gapped ≠ 零外部时间信号；每次换发把"真实时间 ≥ 签发时刻"带进内网，推进 HWM）。
- 试用（trial plan）的 License 本身就是短寿命的，不单独发 lease。

---

## 3. 三种运行形态

### 形态 B（默认，最强）：在线激活 + 心跳续租

适用：办公网可出网的客户。服务端做**权威席位计数、试用去重、次数对账、撤销**。

```
客户端(Rust)                              激活服务器（私有仓库部署）
   |-- POST /v1/activate ------------------->|
   |   { licenseKey, fpAnchorHash,           |  校验 key、检查席位数(maxMachines)、
   |     fpComponentHashes[], nonce,         |  记录 fingerprint→licenseId 绑定
   |     appVersion }                        |
   |<-- 200 { license.lic, lease,  ----------|  Ed25519 签名，nonceEcho=nonce
   |          serverTime }                   |
   | 验签(verify_strict, 内嵌公钥, 断言 alg+kid)
   | 校验 nonceEcho == 本地 nonce
   | HWM := max(HWM, serverTime)
   | 落盘 license/lease + HMAC 状态文件
   |
   |== 之后每次启动/每24h 机会式心跳 =========|
   |-- POST /v1/heartbeat ------------------>|
   |   { licenseId, fpAnchorHash, nonce,     |  未撤销 → 换发新 lease（滚动TTL），
   |     localUsedCount }                    |  回传权威 remainingUses；
   |<-- 200 { lease' } ----------------------|  已撤销/退款 → 拒发，lease 自然到期即死
   |
   | 心跳失败(断网) → 静默降级为形态 A 逻辑，绝不因断网拦截
```

### 形态 A：纯离线签名许可（air-gapped 内网）

适用：涉密/纯内网。销售侧掌握客户身份 → 人工签发即天然限流审计。

```
客户内网机                销售/运营（离线 keygen 工具，私钥在离线机器）
   | 应用内"复制机器码" —— base32(Crockford) 编码的
   |   { fpAnchorHash, componentHashes, nonce }，可打印/拍照
   |------- 机器码（微信/邮件/纸面）--------->|
   |                                          | keygen 工具签发 license.lic
   |                                          | （机器绑定 + 绝对时刻 + nonceEcho）
   |<------ bidguard.lic（U盘/邮件）----------|
   | 导入 → 验签 → 指纹 M-of-N 匹配 → 生效
   |
   |== 每 ~90 天（leaseTtl）重复一次换发 ======| （到期前 30 天开始提醒）
```

### 形态 C：离线激活文件交换（request/response）

形态 A 的自助化版本：有激活服务器、但目标机不联网的客户，在任意联网设备上完成交换。

```
内网目标机                任意联网设备                 激活服务器
   | 导出 activation_request.json
   |   { licenseKey, fpHashes, nonce, ts }
   |---- U盘/二维码 ------->| 上传到 self-service portal --->|
   |                        |                                | 校验+席位计数+签发
   |                        |<--- activation_response.lic ---|（含 nonceEcho）
   |<--- U盘/二维码 --------|
   | 导入 → 验签 → nonceEcho 匹配 → 指纹匹配 → 生效
```

三种形态共用同一 Rust 验证路径（`license::verify`），差别只在令牌如何到达。

---

## 4. 密码学与协议

### 4.1 签名原语：Ed25519（客户端仅验签）

- **选型**：Ed25519 —— 32B 公钥、定长 64B 签名、~128-bit 强度、验签常数时间、无 ECDSA 的 nonce/DER 深坑、无 RSA 的体积负担。Keygen 对 license **文件**默认即 Ed25519（已验证；注意其 license **key** 的 scheme 默认为 null=不签名——我们发的是文件/结构化令牌，不踩这个坑）。
- **crate**：`ed25519-dalek 2.x`，`default-features = false, features = ["std"]`（verify-only 不拉 RNG）。已验证：纯 Rust、无 C 工具链、tier-1 目标（aarch64-apple-darwin / x86_64-pc-windows-msvc）干净编译；有 build.rs（生成预计算表）但无 native 构建负担。备选：直接提升 lockfile 中已有的 `ring 0.17`（零新增编译量），但 ring 是 BoringSSL 衍生的 Rust/C/汇编混合体、API 粗粝——不取。
- **必须 `verify_strict()`**，不是 `verify()`（后者接受低阶/非规范编码密钥）。
- **验签前先断言 `alg == "Ed25519"` 且 `kid` ∈ 内嵌信任集**（防算法替换/降级，JWT alg-confusion 教训）。
- **签名覆盖 payload 原始字节**，验签通过后才 JSON 反序列化 —— 结构性消灭 canonicalization 歧义。

### 4.2 密钥管理

- 二进制内嵌**多把**验签公钥（`kid = "lic-2026a"`、预置 `"lic-2026b"` 备转轮换），`const` 硬编码于 Rust（Keygen 明确建议硬编码而非外部文件——外部文件可被换 key）。
- 私钥**永不进入本仓库/CI**（仓库是公开的 withesse/bidguard）：签发私钥放离线运营机（形态 A 的 keygen CLI）+ 激活服务器 KMS（形态 B/C），存放于独立**私有仓库** `bidguard-license-server`。
- 泄漏应对：用 `kid` 切新钥签发 + 应用更新弃用旧 kid；接受"未更新的存量安装在弃用完成前可被旧钥伪造"的残余风险。

### 4.3 传输层态度（已验证修正）

- **不做证书 pinning**。验证结论：对机主（真正的授权对手）pinning 防伪价值为零（可 patch/换信任库），OWASP 现行指导为"几乎没有应该 pin 的场景"；防伪重量全部压在 **payload 签名 + nonce 回显**上——即使攻击者 hosts 重定向到假服务器返回"valid"，没有私钥也签不出合法 lease。
- 普通 TLS 保留：`ureq 3`（已是直接依赖）默认 rustls + ring provider（已核对 lockfile；措辞注意：rustls 纯 Rust，ring 非纯 Rust 但自包含、无 OpenSSL/OS TLS 链接）。**实现约束**：hf-hub 经 feature 统一拉入了 ureq 的 native-tls feature——激活代码必须使用默认 Agent（或显式 `TlsProvider::Rustls`），永不配置 native-tls；CI 锁 lockfile，防 minor 升级静默切到 aws-lc-rs（Windows 需 cmake/NASM）。
- 已知坑：ureq 默认根证书是 webpki-roots 而非 OS 信任库——企业 TLS 审查代理（MITM CA 只在 OS store）会导致激活失败。应对：激活失败的错误信息直接引导用户走形态 C 文件交换，不做静默重试。

### 4.4 可信时间（Roughtime 式签名时间戳）

- 心跳/激活响应内含 `serverTime`，签名覆盖 `(nonceEcho || serverTime || …)`：nonce 证新鲜（防重放旧时间戳），签名证真（机主装本地根 CA 也伪造不了）。收到即 `HWM := max(HWM, serverTime)` 并收紧回拨容差。
- 已验证修正：签名时间只给出**时间下界**，攻击者可断网拒收——因此它只用于"加严"，离线 HWM 层必须独立自洽（§6.4）。

---

## 5. 机器指纹与节点锁定

### 5.1 定位（已验证修正后的诚实定位）

指纹是**去重键 + 防随手复制**，不是安全边界：MachineGuid 是普通 REG_SZ 一条命令可改；IOPlatformUUID 在 VM 里可任意设。强制力来自**服务端席位计数**（形态 B/C）与**签名绑定**（复制的 lic 在指纹不符的机器上无效），本地匹配只拦 T2 级随手复制。

### 5.2 组合指纹（anchor + M-of-N 副件）

| | Windows x64 | macOS arm64 |
|---|---|---|
| **主锚 anchor（必须命中）** | **SMBIOS System UUID**（`GetSystemFirmwareTable(RSMB)` 解析，固件级、清装 OS 不变——已验证修正：优先于 MachineGuid，两平台重装存活性才对等） | **IOPlatformUUID**（`gethostuuid(2)` 直接系统调用，**不 spawn ioreg**；已验证：v5 UUID 由熔丝 ECID 派生，EACS/DFU/大版本升级均不变，仅主板更换改变） |
| 副件 1 | MachineGuid（HKLM…Cryptography，读取加 `KEY_READ\|KEY_WOW64_64KEY`；清装/Reset this PC 重生、克隆镜像重复、admin 可改——降级为低信任信号） | 硬件序列号（主板维修后序列号保留而 UUID 变——用作**自动批准一次 rebind** 的旁证） |
| 副件 2 | 系统盘 DiskDrive 序列号（部分 USB/RAID 层读空，容忍） | 启动盘卷 UUID |
| 副件 3 | 主板序列号（消费板常见 `"To be filled by O.E.M."`，读空即跳过） | 芯片型号 + 物理核数 |
| 副件 4 | 首个物理网卡永久 MAC（排除虚拟/随机化 MAC） | —（3 取 2） |

- **匹配规则**：anchor 相等 **且** 副件命中 ≥ ⌈N/2⌉（Win 2/4、mac 2/3）。每个分量单独 `SHA-256(app_salt || label || value)`——绝不整体哈希成一个值（任一漂移全盘失配，machineid-rs 的坑）。
- **绝不纳入**：用户名、主机名、IP、卷序列号（重格式化即变）——全是误锁票根。
- **克隆现实**（已验证）：非 sysprep 的政企批量镜像会共享 MachineGuid 甚至 SMBIOS UUID → 指纹碰撞按"license 内去重"处理，不做全局唯一假设；同指纹多 install-id 并发激活时服务端按多席位计费/告警（首启生成随机 install-id 存 Keychain/DPAPI 辅助区分）。
- **VM 策略**：检测 `kern.hv_vmm_present`（mac）/常见 hypervisor 特征（win）；VM 内**拒绝离线试用**、允许付费激活但服务端打标签重点对账。Apple Silicon 上一条命令建新 VM = 全新指纹，是最廉价的试用重置路径（已验证），必须堵在试用侧。

### 5.3 Rebind（换绑）政策

指纹变化是**正常生命周期事件**（清装 Windows、Reset this PC、Mac 换主板），不是欺诈：

- 每 license 换绑额度：**1 次 / 90 天，累计 3 次**；超出走人工支持。
- mac 主板维修（序列号同、UUID 变）→ 自动批准，不消耗额度。
- 形态 B 提供 `deactivate_license` 自助释放席位；形态 A/C 换绑 = 重新签发（人工环路本身即审计点）。

### 5.4 隐私 / PIPL

- **离开设备的只有加盐 SHA-256 哈希**，原始序列号只存在内存中；哈希仍属个人信息 → 激活界面与隐私声明（离线可读）明示采集项、目的、法律依据。
- 形态 A **零外发**是第一等公民路径，不是降级路径——对涉密客户"连哈希都不能出网"的政策照样成立。
- 日志中禁止出现 license key、指纹原值（沿用设计文档 §15.2 日志纪律）。

---

## 6. 防篡改 / 防重置 / 防时钟回拨（分层，逐层标注残余风险）

### 6.1 层 0：服务端锚定（唯一真正稳健的层）

首次运行/激活时服务端记录 `fingerprint → trial_consumed / licenseId`（Keygen UNIQUE_PER_POLICY 模式）。重装、删库、快照回滚后下次联网即被识破。
**残余**：永不联网的机器锚不到；换指纹（VM）刷新试用只能靠身份+限速（试用发放要邮箱/公司名，服务端 velocity limit）。

### 6.2 层 1：签名令牌（不可伪造层）

一切权利以绝对 UTC 时刻 + 上限写死在 Ed25519 签名 payload 里。回拨时钟**买不到时间**，只能触发 fail-closed。
**残余**：patch 二进制跳过验签（T3，接受）。

### 6.3 层 2：HMAC 状态文件 + 双写防删

本地可变状态 `{ hwm, tamper_flag, trial_consumed, used_count_hwm, install_id }`：

- HMAC-SHA256（`hmac` crate + 已有 `sha2 0.11`），密钥 = HKDF(内嵌盐 ‖ anchor 原值)——机器绑定使复制到他机即失效。
- **双写**：`<app_data_dir>/license/state.bin` + 第二个 OS 惯例路径（mac `~/Library/Application Support/.com.yuanben.bidguard.st`；win `%LOCALAPPDATA%` 下第二处）。读取取**最严格**并集。
- **删除 fail-closed**：曾有状态而两份全无 → 视为 `trial_consumed = true`、要求（重新）激活，绝不视为新装白拿试用。全新安装（DB 也不存在）才允许初始化。
- **明确诚实**：HMAC 密钥可从二进制逆向恢复——这是 tamper-evidence（防手编 SQLite/文件），不是不可伪造边界。

### 6.4 层 3：时间高水位（HWM）+ 多证人融合

- 每次启动/开始比对/心跳：`HWM := max(HWM, now)`；证人：`max(updated_at)` over 全部 SQLite 表、app_data_dir 最新 mtime、boot_time+uptime 会话内单调校验（已验证：OS 单调钟重启归零，只用于会话内）。
- 判定：`now < HWM − grace` 或任一证人比 now 新超阈 → 锁存 tamper_flag。
- **分级响应，绝不 brick**（CMOS 电池死、重镜像、RTC-localtime 都是真实良性场景）：小漂移（<48h）静默容忍 → 大回拨警告横幅 + 14 天宽限 → 宽限内未通过换发/激活校正才拦截 `start_compare`，并给出人工解锁码通道。荒谬未来时钟（>已知构建时间+N年）同样锁存，防 HWM 被毒化到 2099。
- 内网 NTP/域时间**不可信**（管理员可整网一致回拨，已验证）——防回拨只依赖本地单调证据 + 签名时间/换发文件锚点。
- **不依赖 TPM/SE 计数器**（已验证确认为死路）：Windows 用户态 NV counter 被驱动固定白名单 + owner-auth 丢弃政策挡死，且中国市场 TCM 政策下 TPM 普及率不可假设；macOS SE 不向应用暴露任何单调计数器；vTPM 随快照回滚。

### 6.5 层 4：硬件绑定状态（v1.2 可选加固，只防复制不防 patch）

- **macOS**：Secure Enclave 生成 P-256 密钥（`security-framework`，`kSecAttrTokenIDSecureEnclave`），激活时用它**对状态 blob 做 ECDSA challenge 签名**。已验证修正：SE 其实也能经 ECIES 封装 HMAC 密钥，但选 ECDSA 是因为封装的对称钥每次使用都暴露在进程内存、提取一次即可跨机重放，而 SE 私钥永不出硬件——每次校验都证明"活的本机"。app_data 整目录拷到另一台 Mac 后签名无法再生 → 防克隆强。
- **Windows**：**不依赖 TPM**（原因见 6.4；检测到可用则机会性使用并上报服务端，仅作加分项）。基线 = DPAPI user-scope + `pOptionalEntropy`：已验证定位——只防 commodity 窃取器与离线盘拷贝，同用户进程（含 patched 副本）必然可解、清装 OS 后失效，纯 at-rest 混淆，方案正确性不得依赖它。
- **残余**：两者都只绑"状态"，不绑"逻辑"——同机 patched 二进制照样调用同一 SE 钥/DPAPI。定位为反克隆，不是反破解。

### 6.6 层 5：报告水印溯源（对 B2B 最高 ROI，且破解不掉）

导出报告（`commands/export.rs` + rust_xlsxwriter）嵌入：页脚可见"授权：{licenseeName} · {licenseId 前 8 位}" + docProps 自定义元数据里 HMAC 派生的隐形指纹。泄露的报告/license 可回溯到签约主体——对有合同关系的具名机构，问责威慑 > 一切本地技术手段，且 NOP 掉校验的破解版仍然带水印。
**残余**：老练泄露者可剥可见水印（隐形元数据兜底）；彻底逆向水印格式可全剥。

### 6.7 轻量二进制加固（收尾薄层）

`obfstr` 编译期字符串加密覆盖 license 模块的错误串/公钥常量（防 grep 定位校验点）；校验在 `start_compare`、导出、job worker 三处自然冗余（单点 NOP 不完全生效）；macOS 尽快配齐 Developer ID 签名+公证、Windows 补 Authenticode（当前均未强制——无签名构建削弱一切防篡改假设，BUILD.md 已支持按 secrets 自动启用）。**跳过** OLLVM/anti-debug/自校验。

---

## 7. 威胁—缓解矩阵

| # | 攻击 | 缓解 | 残余风险 | 哪种形态收口 |
|---|---|---|---|---|
| 1 | 伪造 license / 自制 keygen | Ed25519 verify_strict + 内嵌公钥 + alg/kid 断言 | 私钥不泄则为零；泄漏走 kid 轮换 | A/B/C 全部 |
| 2 | license 文件复制到他机 | 指纹签入 payload + M-of-N 本地匹配 + SE/DPAPI 绑定状态 | VM/克隆镜像可仿指纹 | B（服务端席位计数）基本收口 |
| 3 | 一份授权装 N 台（T2 主威胁） | maxMachines 服务端计数 + 心跳 reap + 水印溯源 | 纯 A 形态只剩人工发放摩擦+水印问责 | B/C 收口；A 靠合同 |
| 4 | 删本地状态/重装重白嫖试用 | 双写 fail-closed + 服务端首启锚定 + Win SMBIOS UUID / mac ECID 派生 UUID 重装存活 | 纯离线 + 全删 + Windows 清装 = 可重置 | B 收口 |
| 5 | VM 快照回滚重置一切 | 服务端锚定（快照外的记忆）；试用侧 VM 检测拒发 | 纯离线不可防（诚实接受，含 vTPM） | 仅 B |
| 6 | 时钟回拨延长试用/授权 | 绝对时刻 + HWM + 多证人 + 签名 serverTime + 换发文件时间信标 | 冻结时钟可吃满 lease 剩余 + 宽限预算 | B 大幅收窄；A 上限=90天文件寿命 |
| 7 | 手编 SQLite 次数/到期 | 次数账本 HMAC + used_count_hwm + 服务端对账 | 逆向出 HMAC 钥可伪造（tamper-evidence 定位） | B 对账收口 |
| 8 | 假激活服务器 / hosts 重定向 | lease 签名 + nonce 回显（不靠 TLS/pinning） | 网络层伪造为零 | B/C |
| 9 | 重放旧激活响应/预生成响应 | nonceEcho 绑定本次请求 + 指纹 scope + issued/expiry | 无 | B/C |
| 10 | 退款/违约客户继续使用 | 停止续签 lease（TTL 到期即死） | 撤销延迟 = min(TTL, 文件寿命)；纯 air-gapped ≤90 天 | B 最快；A/C 有界 |
| 11 | patch 二进制 NOP 校验（T3） | 多点冗余校验 + obfstr + OS 代码签名 + 水印仍在 | **原则上不可防**——目标是成本>收益 | 无（诚实上限） |
| 12 | 指纹哈希外发合规风险（PIPL/涉密） | 盐化哈希 only + 形态 A 零外发 + 离线可读隐私声明 | 哈希仍是个人信息；涉密客户强制走 A | A 即合规路径 |

---

## 8. 落地设计（映射到真实代码路径）

### 8.1 Rust 模块布局（新增 `src-tauri/src/license/`）

```
src-tauri/src/license/
├── mod.rs          // pub LicenseState, LicenseStatus DTO; 初始化入口
├── token.rs        // License/Lease 结构、armored 编解码、verify_strict + alg/kid 断言
├── keys.rs         // const 内嵌公钥集 (kid → [u8;32])，obfstr 包裹
├── fingerprint.rs  // 平台指纹: win(SMBIOS UUID via GetSystemFirmwareTable + registry) /
│                   // mac(gethostuuid + IOKit 副件)；分量级 SHA-256；M-of-N 匹配
├── state.rs        // HMAC 状态文件、双写、fail-closed 读取、HWM/tamper_flag
├── clock.rs        // HWM 更新、多证人融合(max updated_at / mtime / uptime)、分级响应
├── ledger.rs       // 次数账本: 消费/退款、BEGIN IMMEDIATE 原子扣减、与 lease.remainingUses 对账
├── activation.rs   // 形态B: ureq(默认Agent, 强制rustls) activate/heartbeat + nonce
└── exchange.rs     // 形态A/C: 机器码导出(base32 Crockford)、request/response 文件
```

启动装载：`lib.rs` setup（60-79 行）在 `db::open(&base)` 后 `license::load(&base, &pool)`，结果以 `Arc<RwLock<LicenseState>>` 放入 `AppState`（`state.rs` 加一个字段），命令层零磁盘 IO 查询。

### 8.2 新增 Tauri 命令（`commands/license.rs`，`commands/mod.rs` 加 `pub mod license;`，全部追加进 `lib.rs` `generate_handler![]` 80-129 行）

| 命令 | 说明 |
|---|---|
| `get_license_status` | → `LicenseStatusDto`（plan、expiresAt、remainingUses、leaseExpiresAt、graceState、machineCode）camelCase |
| `get_machine_code` | base32 机器码（形态 A 用，含指纹哈希+nonce） |
| `activate_license_online` | 形态 B：key → 服务器 → 落盘验签 |
| `import_license_file` | 形态 A/C：导入 .lic/.lease，验签+指纹匹配 |
| `export_activation_request` | 形态 C：生成 request 文件 |
| `deactivate_license` | 形态 B：释放席位 |

### 8.3 `start_compare` 闸门（`commands/compare.rs`，插在 config_json 序列化 ~97 行与 `state.jobs.spawn` ~103 行之间）

```rust
// 全部请求校验通过后才消费次数（无效请求绝不扣次）
let grant = state.license.check_and_consume(&state.db)?;
// check_and_consume 内部:
// 1) 验 lease/license (内存态) → LicenseRequired/Expired/ClockTamper
// 2) BEGIN IMMEDIATE; INSERT ledger(job_id=待定) 前先
//    UPDATE license_ledger SET remaining = remaining - 1 WHERE remaining > 0;
//    rows_affected != 1 → LicenseExhausted   // 防 r2d2 并发双花
// 3) 记录 ledger 行 state='consumed', job_id 由 spawn 返回后回填
```

**JobConflict 缝隙**：`JobManager::spawn` 在校验通过后仍可能拒绝（同 workspace 同类型任务冲突）——`spawn` 返回 `Err` 时立即退款该 ledger 行。

### 8.4 失败退款（三条终局路径全覆盖，对应 grounding 的 jobs/mod.rs）

1. **正常失败/取消/panic**：包裹传给 `spawn` 的 worker 闭包——`run_compare` 返回 `Err`（含 JobCancelled）时经 `ctx.db` 将 ledger 行置 `refunded` 再传播（`execute()` 的 panic 分支同样落到 failed，由闭包外层 catch 不到 → 用 ProgressSink 装饰器兜底：`emit_terminal` 收到 `status != "completed"` 时退款，幂等）。
2. **启动清障路径**：`lib.rs` 67 行 `job_repo::mark_stale_as_failed` 绕过 `execute()`——同一事务内把这些 job_id 对应的 `consumed` ledger 行一并置 `refunded`。
3. 退款幂等：`UPDATE … SET state='refunded' WHERE job_id=? AND state='consumed'`。
4. `used_count_hwm`（HMAC 状态文件内）只在**消费**时抬升、退款不回落——防"删账本回滚次数"；本地 remaining 与账本、hwm 三方交叉校验，不一致锁 tamper_flag。

### 8.5 DB 迁移（`db/migrations.rs` 追加 `LICENSE_V12`，附 rationale 注释，append-only）

```sql
-- LICENSE_V12: usage ledger for count-limited licenses.
-- The signed license/lease live as files; only the mutable counter lives here,
-- cross-checked against the HMAC state file (DB alone is attacker-writable).
CREATE TABLE license_ledger (
  id INTEGER PRIMARY KEY,
  license_id TEXT NOT NULL,
  remaining INTEGER NOT NULL
);
CREATE TABLE license_usage (
  id TEXT PRIMARY KEY,            -- uuid
  license_id TEXT NOT NULL,
  job_id TEXT,                    -- jobs.id
  state TEXT NOT NULL,            -- consumed | refunded
  created_at TEXT NOT NULL,       -- db::now_iso()
  updated_at TEXT NOT NULL
);
```

（license/lease **不入** app_settings——签名文件为准，DB 只是可变账本。）

### 8.6 错误码（`error.rs` `AppErrorCode` 追加，camelCase 序列化，中文 message）

`LicenseRequired`（未激活/试用未开始）、`LicenseExpired`、`LicenseExhausted`（次数用尽）、`LicenseLeaseExpired`（需续期，宽限外）、`LicenseMachineMismatch`、`LicenseInvalid`（验签失败）、`LicenseClockTamper`。

### 8.7 前端（UX 层，非安全层）

- **路由**（`src/app/router.tsx` 211-233 行）：新增顶层 hash 路由 `/activate`；`<LicenseGuard>` 包裹 `Layout` 的 Outlet——`useLicenseStatus()` 非 active 时 redirect `/activate`，宽限期渲染黄条不拦截。
- **API 层**：`src/api/index.ts` 每命令一个函数；`src/api/types.ts` 加 `LicenseStatusDto`；`src/queries/data.ts` 加 `useLicenseStatus`（queryKey `["license"]`）、`useActivateLicense` 等 mutation（onSuccess invalidate `["license"]`）；错误经 `client.ts` 的 `ApiError.code` 分支。
- **Activate 页**：三 Tab 对应三形态（输入激活码在线激活 / 导入许可文件 / 生成机器码+导入响应文件），显示可复制机器码与隐私声明。
- **Settings 卡片**（`src/screens/Settings.tsx` cardBg/border 风格）：授权状态卡——plan、licensee、到期、剩余次数、lease 到期、换发/反激活按钮。
- `CompareSetup.tsx` 顶部渲染剩余次数（读同一 hook）。

### 8.8 新增依赖（按用户依赖政策逐一论证）

| crate | 版本 | 许可 | 重量 | 理由 |
|---|---|---|---|---|
| `ed25519-dalek` | 2.x，default-features=false, ["std"] | BSD-3 | 纯 Rust，拉 curve25519-dalek 等数个 RustCrypto crate，无 C | 唯一的验签原语；已验证两目标干净编译。生态信任度 > ed25519-compact；比直用 ring 干净 |
| `hmac` | 与 sha2 0.11 同代 RustCrypto | MIT/Apache-2.0 | 数百行薄封装 | 复用已有 sha2 做状态文件 tamper-evidence，零重量 |
| `data-encoding` | 2.x | MIT | 微小 | 机器码 base32 Crockford（无歧义字符，可口述/纸抄） |
| `obfstr` | 0.4 | MIT | 编译期宏，零运行时 | license 模块字符串/公钥防 grep |
| `security-framework`（仅 v1.2, mac target） | — | MIT/Apache-2.0 | 已在 Tauri 生态常见 | SE P-256 challenge 签名 |

不新增：HTTP（复用 ureq 3）、base64（0.22 已在树，提升为直接依赖即可）、keyring（v1 双写方案足够，v1.2 再评估并锁 3.x）、任何 machine-id crate（fingerprint.rs 自研 ~200 行，用已在树的 windows-sys 读注册表/固件表、libc `gethostuuid`——避免 machineid-rs 混入 Username/MachineName 等不稳定分量的坑）。Tauri capabilities/CSP **零改动**（Rust 侧网络不走 WebView）。

### 8.9 服务端与签发工具（独立私有仓库 `bidguard-license-server`）

- `keygen-cli`：离线签发工具（形态 A），跑在运营离线机，私钥本地加密存储。
- `activation-server`：`POST /v1/activate`、`/v1/heartbeat`、`/v1/deactivate`、`/v1/offline-exchange`（形态 C portal）；SQLite/Postgres 存 license→fingerprint→席位、trial 去重记录、试用发放限速；私钥入 KMS。
- 本仓库（公开）与 CI **永不接触私钥**；现有 `TAURI_SIGNING_PRIVATE_KEY` 的 GH Secrets 先例只适用于更新签名，license 私钥不进 GH Secrets。

---

## 9. 分阶段实施计划

| 阶段 | 内容 | 交付判据 | 估算 |
|---|---|---|---|
| **MVP（v0.6）** | `license/` 模块：token.rs + keys.rs + fingerprint.rs（anchor+副件、M-of-N）+ state.rs（HMAC 双写 fail-closed）+ clock.rs（HWM 基础版）+ ledger.rs（原子扣减+三路退款）；LICENSE_V12 迁移；错误码；`start_compare` 闸门；命令 get_license_status / get_machine_code / import_license_file；前端 /activate（形态 A Tab）+ Guard + Settings 卡；keygen-cli（私有仓库）；试用 = 首启自签发本地 trial 记录（服务端锚定留待 v1.1，接受可重置） | 形态 A 全流程可售：签发→导入→按期/按次强制→失败退款→到期换发 | **1.5–2 周** |
| **v1.1** | activation-server（activate/heartbeat/deactivate + 席位计数 + trial 指纹去重 + 限速）；activation.rs（nonce+验签+serverTime 锚定 HWM）；lease 滚动续期与对账；试用改为可选在线锚定；VM 检测拒发离线试用；rebind 额度逻辑；报告水印（export.rs） | 形态 B 端到端；撤销演练（停续签→TTL 到期拒绝）通过 | **2–3 周**（含服务端） |
| **v1.2** | 形态 C 文件交换（exchange.rs + self-service portal 页）；mac SE ECDSA 状态绑定；Win DPAPI+entropy at-rest；keyring 评估；clock.rs 证人融合完整版 + 分级响应打磨；obfstr 覆盖；CI 加 aarch64-apple-darwin / x86_64-pc-windows-msvc license 模块编译验证；Authenticode/公证收尾 | 形态 C 可自助；克隆 app_data 到第二台 Mac 失效 | **约 2 周** |

每阶段均满足全局规则：任何代码变更先经用户确认再执行；不自动 commit。

---

## 10. 需用户拍板的决策点

| # | 问题 | 选项 | **建议** |
|---|---|---|---|
| 1 | 试用形态 | a) 7天/10次 自助（离线可重置） b) 14天/20次 需在线激活 c) 仅销售发放签名试用文件 | **a+c 混合**：能联网的自助试用走服务端锚定（v1.1 起）；内网客户走销售签发试用 lic——人工摩擦即防滥用，契合 B2B 销售动线 |
| 2 | offline_strict 的 lease TTL（=air-gapped 撤销上限） | 30 / 90 / 365 天 / 等于授权期 | **90 天**（Keygen air-gapped 惯例；年付客户一年 3 次 U 盘换发可接受；365 天=实际放弃撤销）。connected 档 **14 天** |
| 3 | 换绑额度 | 严（1 次/年）↔ 宽（无限自助） | **1 次/90 天、累计 3 次 + mac 主板维修自动批准**；宁可支持工单多一点也不误锁评标现场 |
| 4 | 激活服务器托管 | 自建国内云（ICP/合规可控）/ Keygen 云（快但境外数据出境敏感）/ 先不做（只发 MVP 形态 A） | **自建国内轻量云**（单二进制 + SQLite 起步即可）；目标客户对境外服务敏感，Keygen 云基本不可行 |
| 5 | 时钟回拨严格度 | 检测即拦 ↔ 只警告 | **分级**（§6.4）：48h 容忍→警告+14 天宽限→拦截+人工解锁码。内网机器坏 RTC 是常态，误锁一次可能丢单 |
| 6 | 心跳是否默认开启（connected 档） | 默认开 / 默认关需用户勾选 | **激活时明示勾选**（"允许本机定期联网校验授权，仅传输授权编号与设备哈希"）——"全程离线"是产品卖点，任何静默外联都可能死在客户安全评审；拒绝勾选则自动落入 offline_strict 档 |
| 7 | 隐私声明 | 是否在激活页+文档明示指纹哈希采集 | **必须**（PIPL）：采集项、盐化哈希、用途、形态 A 零外发选项，离线可读 |
| 8 | 破解版应对预算 | 追加重混淆 / 接受现状 | **接受现状 + 水印溯源 + 法务条款**；对低装机量垂直工具，重混淆维护成本与 AV 误报风险 > 收益 |

---

## 11. 正式化 checklist（发布前必做）

> 现状：MVP 已实机验证通过（试用→激活→已授权），但内嵌的是**开发公钥** `lic-dev-2026a`，keygen 与开发私钥在本地 scratchpad。**未经本清单不可对外发布**——A 组为硬性阻断项，未完成则任何人用泄漏的开发私钥即可伪造合法许可。

### A. 密钥换发（硬性阻断，未完成不得发布）
- [ ] 离线机（不联网、非 CI）执行 `bidguard-keygen genkey --kid lic-2026a` 生成正式密钥对
- [ ] 正式私钥离线加密保管（age/GPG + 硬件密钥），**绝不进任何仓库 / CI / 云盘 / 聊天工具**
- [ ] 用正式公钥替换 `src-tauri/src/license/keys.rs` 的 `TRUSTED_KEYS`：kid 改 `lic-2026a`，移除 `lic-dev-2026a`
- [ ] 销毁 scratchpad 的 `lic-dev-2026a.priv` 并视开发钥为已泄漏（本文档、对话记录里出现过其种子）
- [ ] `FP_SALT` / `STATE_HMAC_SALT` 一次性定稿（改动会使既有激活/试用状态失效，发布后不可再改）

### B. 签发工具与服务端隔离
- [ ] keygen 从 scratchpad 迁入独立**私有**仓库 `bidguard-license-server`（连签发台账）
- [ ] 建立签发台账：`licenseId ↔ 客户名 ↔ 机器码 ↔ 期限/次数 ↔ 签发日`（换绑与撤销依据）
- [ ] 与更新签名私钥 `TAURI_SIGNING_PRIVATE_KEY` 区分：license 私钥**不**进 GH Secrets
- [ ] 换绑流程文档化：凭旧 `licenseId` 免费换绑，1 次/90 天、累计 3 次（§5.3）

### C. 代码签名与防篡改（防二进制 patch 的前置）
- [ ] macOS：Developer ID 签名 + 公证（notarization）——无签名构建削弱一切防篡改假设
- [ ] Windows：Authenticode 签名
- [ ] `tauri.conf.json` 的 `bundle.macOS.signingIdentity` 由 secrets 注入，本地保持 ad-hoc

### D. 参数固化（发布前定稿）
- [ ] 试用值：`license/mod.rs` 的 `TRIAL_DAYS=7` / `TRIAL_USES=10` 最终确认（影响存量首装）
- [ ] `clock.rs::ROLLBACK_TOLERANCE=48h` 与签发默认 `graceDays` 确认
- [ ] 商业形态×定价映射（plan × 期限 × 次数）写入签发台账模板

### E. CI / 仓库卫生
- [ ] CI 不携带任何 license 私钥；`tests/license_flow.rs` 的 `BIDGUARD_DEV_PRIV` gated 测试在无私钥时自动跳过（现状如此，CI 恒绿）
- [ ] 确认 `*.priv` / `*.lic` / scratchpad 密钥不会被误提交（检查忽略规则，勿改 .gitignore 除非必要）
- [ ] 换钥后重跑 `cargo test` + `tsc` + `vite build` 全绿

### F. 发布前回归验收（换钥后逐条实测）
- [ ] **正式私钥**对真机机器码签发 .lic → 导入 → 已授权
- [ ] **开发钥**签的旧 .lic 导入 → 被拒（`LicenseInvalid`），确认开发钥彻底失效
- [ ] 全新机（无状态文件）首启 → 试用态；次数用尽/到期 → 拒绝 + 路由守卫拦到激活页
- [ ] 一次失败/取消任务 → 次数自动退回；进程被杀留下的 consumed → 启动对账补退
- [ ] 篡改状态文件（手改 usedCount）→ HMAC 失配回落另一副本严格值；换机导入 → `LicenseMachineMismatch`
- [ ] 已知残余（不阻断发布，v1.1 服务端锚定关闭）：删双写两份 + 重装 = 试用可重置；时钟冻结可吃满 lease/宽限

### 不在本清单（v1.1+ 再做）
在线激活 + 心跳续租 + 服务端席位/试用锚定；真实指纹组件位（SMBIOS UUID）；报告水印溯源；硬件绑定状态（SE/DPAPI）；离线激活文件交换。

---

### 附：与旧方案（离线 Ed25519 机器码+激活码）的差异摘要

| | 旧方案 | 本方案 |
|---|---|---|
| 撤销 | 无 | lease 非续签，延迟 ≤ min(TTL, 文件寿命) |
| 试用 | 无 | 服务端锚定 + 签名试用文件双轨 |
| 次数 | 本地明文计数 | 签名上限 + HMAC 账本 + 原子扣减/退款 + 服务端对账 |
| 时钟 | 未处理 | HWM + 多证人 + 签名 serverTime + 换发时间信标 |
| 指纹 | 单一机器码 | anchor + M-of-N 组合、平台差异化（SMBIOS UUID / ECID 派生 UUID）、rebind 政策 |
| 密钥 | 单公钥 | 多公钥 + kid 轮换，私钥离库离 CI |
