// 内嵌 Ed25519 验签公钥集与应用盐。
// 私钥永不进入本仓库/CI：形态 A 的签发私钥在离线运营机（bidguard-keygen），
// 形态 B/C 的签发私钥在激活服务器 KMS（独立私有仓库）。
//
// 密钥轮换：新增一把新 kid 的公钥（放数组首位），旧 kid 保留到存量安装升级完毕再移除。
// 公钥更换即使旧许可失效，是可接受的一次性代价（对比被泄漏私钥可无限伪造，划算得多）。

/// 机器指纹哈希盐：参与 anchor/组件的 SHA-256。改动会使既有机器码/绑定失配。
pub const FP_SALT: &[u8] = b"bidguard.fingerprint.salt.v1";

/// 授权状态文件 HMAC 密钥派生盐：key = SHA-256(STATE_HMAC_SALT || anchor_raw)。
/// 与机器绑定，状态文件复制到他机即 HMAC 失配 → fail-closed。
pub const STATE_HMAC_SALT: &[u8] = b"bidguard.state.hmac.salt.v1";

/// 受信签发公钥集。首项为当前签发钥；其余为轮换预留/历史钥。
/// 注意：lic-dev-2026a 为开发密钥；正式发布前必须用离线机新生成一把并替换，且开发私钥作废。
pub const TRUSTED_KEYS: &[(&str, [u8; 32])] = &[(
    "lic-dev-2026a",
    [
        0x34, 0xa3, 0x95, 0x2d, 0xd1, 0xce, 0x77, 0x48, 0xf5, 0x53, 0x12, 0x7c, 0xdc, 0x41, 0x62,
        0x0f, 0xd5, 0x58, 0x61, 0xf3, 0x1e, 0xa6, 0xd4, 0x85, 0x72, 0x80, 0x60, 0x6e, 0xeb, 0x8e,
        0x36, 0x49,
    ],
)];

/// 按 kid 取受信公钥；未知 kid 返回 None（验签直接判 LicenseInvalid，防降级/替换）。
pub fn public_key_for(kid: &str) -> Option<[u8; 32]> {
    TRUSTED_KEYS.iter().find(|(k, _)| *k == kid).map(|(_, v)| *v)
}
