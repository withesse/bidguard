// 机器指纹（节点锁定）。诚实定位：去重键 + 防随手复制，不是安全边界——
// 强制力来自签名绑定（复制的 .lic 在 anchor 不符的机器上验签后被 matches() 拒绝）。
// anchor_raw 仅存内存（派生状态文件 HMAC 密钥）；外发的只有盐化 SHA-256 哈希（PIPL）。
//
// MVP：anchor = machine-uid（macOS IOPlatformUUID / Windows MachineGuid）。
// 组件位（M-of-N 容差）暂留空，schema 前向兼容 v1.1 加入 SMBIOS UUID / 序列号等。
use crate::license::keys::FP_SALT;
use crate::license::token::MachineBinding;
use sha2::{Digest, Sha256};

pub struct Fingerprint {
    anchor_raw: String,
    pub anchor_hash: String,
    pub component_hashes: Vec<String>,
}

impl Fingerprint {
    pub fn collect() -> Self {
        let anchor_raw = machine_anchor();
        let anchor_hash = salted_hash("anchor", anchor_raw.as_bytes());
        Self {
            anchor_raw,
            anchor_hash,
            component_hashes: Vec::new(),
        }
    }

    /// 派生状态文件 HMAC 密钥用（仅内存）。
    pub fn anchor_raw(&self) -> &str {
        &self.anchor_raw
    }

    /// 与许可绑定信息比对：anchor 必须相等；组件为空或阈值 0 → 仅 anchor 判定。
    pub fn matches(&self, b: &MachineBinding) -> bool {
        if self.anchor_hash != b.anchor_hash {
            return false;
        }
        if b.component_hashes.is_empty() || b.match_threshold == 0 {
            return true;
        }
        let hits = b
            .component_hashes
            .iter()
            .filter(|h| self.component_hashes.contains(h))
            .count();
        hits as u32 >= b.match_threshold
    }

    /// 机器码：base32(Crockford 无歧义) 编码的 {anchorHash, componentHashes, appVersion}，
    /// 用户复制/拍照发给运营 → keygen 据此签发绑定本机的 .lic。
    pub fn machine_code(&self, app_version: &str) -> String {
        let mc = serde_json::json!({
            "v": 1,
            "anchorHash": self.anchor_hash,
            "componentHashes": self.component_hashes,
            "appVersion": app_version,
        });
        let bytes = serde_json::to_vec(&mc).unwrap_or_default();
        let b32 = data_encoding::BASE32_NOPAD.encode(&bytes);
        let grouped = b32
            .as_bytes()
            .chunks(5)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join("-");
        format!("BG1-{grouped}")
    }
}

fn salted_hash(label: &str, raw: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(FP_SALT);
    h.update(label.as_bytes());
    h.update(raw);
    hex(&h.finalize())
}

/// 主锚。machine-uid 在 macOS 读 IOPlatformUUID、Windows 读注册表 MachineGuid。
/// 读取失败（罕见沙箱/权限）回落到稳定占位串，使应用仍可运行（授权判定退化为不绑定机器）。
fn machine_anchor() -> String {
    machine_uid::get().unwrap_or_else(|_| "bidguard-unknown-machine".to_string())
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchor_only_match() {
        let fp = Fingerprint {
            anchor_raw: "raw".into(),
            anchor_hash: "abc".into(),
            component_hashes: vec![],
        };
        let ok = MachineBinding {
            anchor_hash: "abc".into(),
            component_hashes: vec![],
            match_threshold: 0,
        };
        let bad = MachineBinding {
            anchor_hash: "xyz".into(),
            component_hashes: vec![],
            match_threshold: 0,
        };
        assert!(fp.matches(&ok));
        assert!(!fp.matches(&bad));
    }

    #[test]
    fn m_of_n_components() {
        let fp = Fingerprint {
            anchor_raw: "raw".into(),
            anchor_hash: "a".into(),
            component_hashes: vec!["c1".into(), "c2".into(), "c3".into()],
        };
        // anchor 对 + 3 组件里命中 2 ≥ 阈值 2
        let b = MachineBinding {
            anchor_hash: "a".into(),
            component_hashes: vec!["c1".into(), "c2".into(), "cX".into()],
            match_threshold: 2,
        };
        assert!(fp.matches(&b));
        // 命中不足阈值
        let b2 = MachineBinding {
            anchor_hash: "a".into(),
            component_hashes: vec!["cX".into(), "cY".into(), "cZ".into()],
            match_threshold: 2,
        };
        assert!(!fp.matches(&b2));
    }

    #[test]
    fn machine_code_roundtrips_prefix() {
        let fp = Fingerprint::collect();
        let code = fp.machine_code("0.5.0");
        assert!(code.starts_with("BG1-"));
    }
}
