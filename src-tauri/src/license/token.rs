// 许可令牌：armored 编码 + Ed25519 验签（仅公钥）+ 结构化 payload。
// 签名覆盖 ASCII 的 signing_input = b64url(header) + "." + b64url(payload)（JWT 式，
// 消灭 JSON canonicalization 歧义）；验签通过后才反序列化 payload。
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::license::keys;
use data_encoding::BASE64URL_NOPAD;
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;

#[derive(Deserialize)]
struct Header {
    #[allow(dead_code)]
    v: u8,
    alg: String,
    kid: String,
}

/// 机器绑定：anchor 必须相等，组件命中数 ≥ match_threshold（MVP 组件常为空 → 仅 anchor）。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineBinding {
    pub anchor_hash: String,
    #[serde(default)]
    pub component_hashes: Vec<String>,
    #[serde(default)]
    pub match_threshold: u32,
}

/// 许可权利凭证（签名 payload）。时间为绝对 UTC 时刻（非「安装后 N 天」），杜绝改钟延期。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LicensePayload {
    pub license_id: String,
    pub licensee_name: String,
    /// trial | timed | counted | timed_counted | perpetual
    pub plan: String,
    pub issued_at: String,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub max_uses: Option<u64>,
    #[serde(default = "one")]
    pub max_machines: u32,
    pub machine: MachineBinding,
    #[serde(default)]
    pub lease_ttl_days: Option<u32>,
    #[serde(default)]
    pub grace_days: u32,
    #[serde(default)]
    pub features: Vec<String>,
}

fn one() -> u32 {
    1
}

fn invalid(msg: &str) -> AppError {
    AppError::new(AppErrorCode::LicenseInvalid, msg)
}

/// 从 armored 许可文本验签并解析。任何失败（编码/算法/kid/签名/正文）→ LicenseInvalid。
pub fn verify_license(armored: &str) -> AppResult<LicensePayload> {
    let body = dearmor(armored, "BIDGUARD LICENSE")?;
    let parts: Vec<&str> = body.split('.').collect();
    if parts.len() != 3 {
        return Err(invalid("许可格式不正确"));
    }
    let (header_b64, payload_b64, sig_b64) = (parts[0], parts[1], parts[2]);

    let header_bytes = BASE64URL_NOPAD
        .decode(header_b64.as_bytes())
        .map_err(|_| invalid("许可头编码错误"))?;
    let header: Header = serde_json::from_slice(&header_bytes).map_err(|_| invalid("许可头解析失败"))?;

    // 先断言算法与 kid（防算法替换/降级），再取受信公钥
    if header.alg != "Ed25519" {
        return Err(invalid("不支持的许可签名算法"));
    }
    let pk = keys::public_key_for(&header.kid).ok_or_else(|| invalid("许可签名密钥不受信任"))?;
    let vk = VerifyingKey::from_bytes(&pk).map_err(|_| invalid("内嵌公钥无效"))?;

    let sig_bytes = BASE64URL_NOPAD
        .decode(sig_b64.as_bytes())
        .map_err(|_| invalid("签名编码错误"))?;
    let sig_arr: [u8; 64] = sig_bytes
        .as_slice()
        .try_into()
        .map_err(|_| invalid("签名长度错误"))?;
    let sig = Signature::from_bytes(&sig_arr);

    // verify_strict：拒绝低阶/非规范编码，比 verify 严格
    let signing_input = format!("{header_b64}.{payload_b64}");
    vk.verify_strict(signing_input.as_bytes(), &sig)
        .map_err(|_| invalid("许可签名验证失败"))?;

    let payload_bytes = BASE64URL_NOPAD
        .decode(payload_b64.as_bytes())
        .map_err(|_| invalid("许可正文编码错误"))?;
    serde_json::from_slice(&payload_bytes).map_err(|_| invalid("许可正文解析失败"))
}

/// 剥离 PEM 式 armor，返回单行 body。
fn dearmor(text: &str, label: &str) -> AppResult<String> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut in_body = false;
    let mut body = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t == begin {
            in_body = true;
            continue;
        }
        if t == end {
            break;
        }
        if in_body {
            body.push_str(t);
        }
    }
    if body.is_empty() {
        return Err(invalid("未找到许可内容"));
    }
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    // 由 bidguard-keygen 用 lic-dev-2026a 私钥签发的固定测试许可（anchor 绑定 "TESTANCHORHASH"）。
    // 见测试辅助 issue_test_license。
    #[test]
    fn rejects_tampered_and_unknown_kid() {
        // 空/垃圾输入
        assert!(verify_license("not a license").is_err());
        assert!(verify_license("-----BEGIN BIDGUARD LICENSE-----\nAAAA.BBBB\n-----END BIDGUARD LICENSE-----").is_err());
    }

    #[test]
    fn dearmor_extracts_body() {
        let armored = "-----BEGIN BIDGUARD LICENSE-----\naaa\nbbb\n-----END BIDGUARD LICENSE-----\n";
        assert_eq!(dearmor(armored, "BIDGUARD LICENSE").unwrap(), "aaabbb");
    }
}
