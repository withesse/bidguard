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
    /// 主锚是否真实读到。false = 用了兜底占位串——该占位串在所有读取失败的机器上相同，
    /// 若照常参与绑定判定，等于对这类机器整体解除节点锁定，故 matches/导入均拒绝。
    anchor_ok: bool,
}

impl Fingerprint {
    pub fn collect() -> Self {
        let (anchor_raw, anchor_ok) = machine_anchor();
        let anchor_hash = salted_hash("anchor", anchor_raw.as_bytes());
        Self {
            anchor_raw,
            anchor_hash,
            component_hashes: Vec::new(),
            anchor_ok,
        }
    }

    /// 派生状态文件 HMAC 密钥用（仅内存）。
    pub fn anchor_raw(&self) -> &str {
        &self.anchor_raw
    }

    /// 主锚是否真实读到（false 时许可绑定被拒绝，试用不受影响）。
    pub fn anchor_ok(&self) -> bool {
        self.anchor_ok
    }

    /// 与许可绑定信息比对：anchor 必须相等；组件为空或阈值 0 → 仅 anchor 判定。
    /// anchor 未真实读到 → 一律不匹配（防兜底常量成为共享万能锚）。
    pub fn matches(&self, b: &MachineBinding) -> bool {
        if !self.anchor_ok {
            return false;
        }
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

    /// 机器码：Crockford base32 编码的 {anchorHash, componentHashes, appVersion}，
    /// 用户复制/拍照发给运营 → keygen 据此签发绑定本机的 .lic。
    /// 前缀 BG2：BG1 曾误用 RFC4648 字母表（含 I/1、O/0 混淆字符，与「可口述/纸抄」目标相悖），
    /// 换 Crockford 后 bump 前缀，让未同步的 keygen 失败得响亮而非错解码。
    pub fn machine_code(&self, app_version: &str) -> String {
        if !self.anchor_ok {
            return "无法读取本机硬件标识，无法生成机器码（请联系支持）".to_string();
        }
        let mc = serde_json::json!({
            "v": 2,
            "anchorHash": self.anchor_hash,
            "componentHashes": self.component_hashes,
            "appVersion": app_version,
        });
        let bytes = serde_json::to_vec(&mc).unwrap_or_default();
        let b32 = machine_code_encoding().encode(&bytes);
        let grouped = b32
            .as_bytes()
            .chunks(5)
            .map(|c| String::from_utf8_lossy(c).to_string())
            .collect::<Vec<_>>()
            .join("-");
        format!("BG2-{grouped}")
    }
}

/// 机器码编码：Crockford base32（无 I/L/O/U），解码侧宽容——小写折叠、i/l→1、o→0。
/// keygen 的解码必须与此保持一致（tests/license_flow.rs 的契约测试即用本函数）。
pub fn machine_code_encoding() -> &'static data_encoding::Encoding {
    use std::sync::OnceLock;
    static ENC: OnceLock<data_encoding::Encoding> = OnceLock::new();
    ENC.get_or_init(|| {
        let mut spec = data_encoding::Specification::new();
        spec.symbols.push_str("0123456789ABCDEFGHJKMNPQRSTVWXYZ");
        spec.translate.from.push_str("IiLlOoabcdefghjkmnpqrstvwxyz");
        spec.translate.to.push_str("111100ABCDEFGHJKMNPQRSTVWXYZ");
        spec.encoding().expect("Crockford base32 规格恒有效")
    })
}

fn salted_hash(label: &str, raw: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(FP_SALT);
    h.update(label.as_bytes());
    h.update(raw);
    hex(&h.finalize())
}

/// 主锚。machine-uid 在 macOS 读 IOPlatformUUID、Windows 读注册表 MachineGuid。
/// 读取失败（罕见沙箱/权限）回落占位串仅为维持状态文件 HMAC 派生与试用可用；
/// 返回的 false 标记使许可绑定/机器码路径显式拒绝（占位串全网相同，不能当锚）。
fn machine_anchor() -> (String, bool) {
    match machine_uid::get() {
        Ok(v) => (v, true),
        Err(_) => ("bidguard-unknown-machine".to_string(), false),
    }
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
            anchor_ok: true,
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
    fn unresolved_anchor_never_matches() {
        // 兜底占位锚在所有读取失败的机器上相同——绑定判定必须拒绝，否则等于万能锚
        let fp = Fingerprint {
            anchor_raw: "bidguard-unknown-machine".into(),
            anchor_hash: salted_hash("anchor", b"bidguard-unknown-machine"),
            component_hashes: vec![],
            anchor_ok: false,
        };
        let b = MachineBinding {
            anchor_hash: fp.anchor_hash.clone(),
            component_hashes: vec![],
            match_threshold: 0,
        };
        assert!(!fp.matches(&b), "占位锚即使哈希相等也不得匹配");
        assert!(fp.machine_code("0.6.0").contains("无法"));
    }

    #[test]
    fn m_of_n_components() {
        let fp = Fingerprint {
            anchor_raw: "raw".into(),
            anchor_hash: "a".into(),
            component_hashes: vec!["c1".into(), "c2".into(), "c3".into()],
            anchor_ok: true,
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
    fn machine_code_crockford_roundtrip_tolerates_transcription() {
        let fp = Fingerprint::collect();
        if !fp.anchor_ok() {
            eprintln!("跳过：本机读不到 machine-uid（罕见沙箱环境）");
            return;
        }
        let code = fp.machine_code("0.6.0");
        assert!(code.starts_with("BG2-"), "编码语义变更必须随前缀 bump：{code}");
        let body = code.strip_prefix("BG2-").unwrap().replace('-', "");
        // 无歧义字母表：编码输出不含 I/L/O/U
        assert!(!body.chars().any(|c| matches!(c, 'I' | 'L' | 'O' | 'U')), "{body}");
        let strict = machine_code_encoding().decode(body.as_bytes()).expect("规范形式可解");
        // 抄写宽容：小写 + 0 抄成 O、1 抄成 l，解码侧折叠后与规范形式等价
        let sloppy = body.to_lowercase().replace('0', "O").replace('1', "l");
        let folded = machine_code_encoding().decode(sloppy.as_bytes()).expect("宽容形式可解");
        assert_eq!(strict, folded);
        let v: serde_json::Value = serde_json::from_slice(&strict).unwrap();
        assert_eq!(v["anchorHash"].as_str().unwrap(), fp.anchor_hash);
        assert_eq!(v["v"].as_i64(), Some(2));
    }
}
