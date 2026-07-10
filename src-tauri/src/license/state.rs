// 授权可变状态：HMAC-SHA256 防篡改 + 双写防删除 + fail-closed 读取。
// 诚实定位：HMAC 密钥可从二进制逆向恢复 → 这是 tamper-evidence（防手编文件/删库重置），
// 不是不可伪造边界。真正的强撤销靠 v1.1 的服务端租约非续签。
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::license::keys::STATE_HMAC_SALT;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

type HmacSha256 = Hmac<Sha256>;

pub const STATE_VERSION: u32 = 1;

/// 本地可变授权状态（enforcement 的次数计数在此，非 SQLite——DB 可被直接改写）。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct LicenseState {
    pub version: u32,
    pub install_id: String,
    pub tamper_flag: bool,
    pub clock_tamper: bool,
    /// 时间高水位（只增），anti-rollback 基准。
    pub time_hwm: String,
    // —— 试用（本地，MVP 接受可重置）——
    pub trial_started_at: Option<String>,
    pub trial_expires_at: Option<String>,
    pub trial_max_uses: u64,
    pub trial_used: u64,
    pub trial_exhausted: bool,
    // —— 已装许可的计数镜像 ——
    pub license_id: Option<String>,
    pub used_count: u64,
    /// 毛用量高水位（只增，退款不回落）：删状态重建时用于识别重置。
    pub used_count_hwm: u64,
    /// 曾初始化过：区分「全新首装」与「状态被删」，后者 fail-closed。
    pub initialized: bool,
}

pub struct StateStore {
    key: [u8; 32],
    paths: Vec<PathBuf>,
}

impl StateStore {
    /// key = SHA-256(STATE_HMAC_SALT || anchor_raw)，与机器绑定。
    pub fn new(base: &Path, anchor_raw: &str) -> Self {
        let mut h = Sha256::new();
        h.update(STATE_HMAC_SALT);
        h.update(anchor_raw.as_bytes());
        let key: [u8; 32] = h.finalize().into();
        Self {
            key,
            paths: vec![primary_path(base), secondary_path(base)],
        }
    }

    fn mac(&self, data: &[u8]) -> Vec<u8> {
        let mut m = HmacSha256::new_from_slice(&self.key).expect("HMAC 接受任意长度密钥");
        m.update(data);
        m.finalize().into_bytes().to_vec()
    }

    fn read_one(&self, path: &Path) -> Option<LicenseState> {
        let content = std::fs::read(path).ok()?;
        let text = String::from_utf8(content).ok()?;
        let (mac_b64, json) = text.split_once('\n')?;
        let expected = data_encoding::BASE64URL_NOPAD
            .decode(mac_b64.trim().as_bytes())
            .ok()?;
        let actual = self.mac(json.as_bytes());
        if !constant_time_eq(&expected, &actual) {
            return None; // HMAC 失配：被篡改或非本机
        }
        serde_json::from_str(json).ok()
    }

    /// 是否至少有一个状态文件存在于磁盘（用于区分「全新首装」与「被删/被篡改」）。
    pub fn any_file_exists(&self) -> bool {
        self.paths.iter().any(|p| p.exists())
    }

    /// 读双写副本，合并为「较严」态；两份都无效（缺失或 HMAC 失配）返回 None。
    pub fn load(&self) -> Option<LicenseState> {
        self.paths
            .iter()
            .filter_map(|p| self.read_one(p))
            .reduce(stricter)
    }

    /// 双写落盘；主副本写入并可读回即视为成功。
    pub fn save(&self, state: &LicenseState) -> AppResult<()> {
        let json = serde_json::to_string(state).map_err(|e| {
            AppError::new(AppErrorCode::LicenseInvalid, "授权状态序列化失败").with_detail(e.to_string())
        })?;
        let mac_b64 = data_encoding::BASE64URL_NOPAD.encode(&self.mac(json.as_bytes()));
        let content = format!("{mac_b64}\n{json}");
        for p in &self.paths {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, content.as_bytes());
        }
        if std::fs::read(&self.paths[0]).map(|c| c == content.as_bytes()).unwrap_or(false) {
            Ok(())
        } else {
            Err(AppError::new(AppErrorCode::LicenseInvalid, "授权状态写入失败"))
        }
    }
}

fn primary_path(base: &Path) -> PathBuf {
    base.join("license").join("state.bin")
}

/// 次副本：放到 app_data 根下的隐藏文件，与主副本不同目录，抬高「同时删两处」成本。
fn secondary_path(base: &Path) -> PathBuf {
    base.join(".bidguard.lst")
}

/// 合并两份状态为较严者：以「更靠前」的一份为基（毛用量之和大），再对计数取 max、旗标取 OR。
fn stricter(a: LicenseState, b: LicenseState) -> LicenseState {
    let (mut base, other) = if advance(&a) >= advance(&b) { (a, b) } else { (b, a) };
    base.tamper_flag |= other.tamper_flag;
    base.clock_tamper |= other.clock_tamper;
    base.trial_exhausted |= other.trial_exhausted;
    base.trial_used = base.trial_used.max(other.trial_used);
    base.used_count = base.used_count.max(other.used_count);
    base.used_count_hwm = base.used_count_hwm.max(other.used_count_hwm).max(base.used_count);
    base.time_hwm = crate::license::clock::max_iso(&base.time_hwm, &other.time_hwm);
    base.initialized |= other.initialized;
    base
}

fn advance(s: &LicenseState) -> u64 {
    s.used_count + s.trial_used + s.used_count_hwm
}

/// 定长常数时间比较（HMAC 校验，避免计时侧信道）。
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile_like::TempDir;

    // 极简临时目录（不引入 tempfile 依赖）
    mod tempfile_like {
        use std::path::{Path, PathBuf};
        pub struct TempDir(PathBuf);
        impl TempDir {
            pub fn new(tag: &str) -> Self {
                let mut p = std::env::temp_dir();
                let uniq = format!("bidguard-test-{tag}-{}", std::process::id());
                p.push(uniq);
                let _ = std::fs::remove_dir_all(&p);
                std::fs::create_dir_all(&p).unwrap();
                TempDir(p)
            }
            pub fn path(&self) -> &Path {
                &self.0
            }
        }
        impl Drop for TempDir {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    fn sample() -> LicenseState {
        LicenseState {
            version: STATE_VERSION,
            install_id: "iid".into(),
            time_hwm: "2026-07-10T00:00:00.000Z".into(),
            trial_max_uses: 10,
            trial_used: 3,
            used_count: 5,
            used_count_hwm: 5,
            initialized: true,
            ..Default::default()
        }
    }

    #[test]
    fn save_load_roundtrip_and_double_write() {
        let dir = TempDir::new("rt");
        let store = StateStore::new(dir.path(), "anchor-x");
        let s = sample();
        store.save(&s).unwrap();
        let loaded = store.load().expect("应能读回");
        assert_eq!(loaded.used_count, 5);
        assert_eq!(loaded.trial_used, 3);
        // 双写：删主副本，仍能从次副本读回
        std::fs::remove_file(primary_path(dir.path())).unwrap();
        let loaded2 = store.load().expect("次副本应仍在");
        assert_eq!(loaded2.used_count, 5);
    }

    #[test]
    fn tamper_breaks_hmac() {
        let dir = TempDir::new("tamper");
        let store = StateStore::new(dir.path(), "anchor-y");
        store.save(&sample()).unwrap();
        // 手改主副本 JSON（伪造更多剩余次数）→ HMAC 失配 → 该副本判无效
        let p = primary_path(dir.path());
        let text = std::fs::read_to_string(&p).unwrap();
        let (_mac, json) = text.split_once('\n').unwrap();
        let hacked = json.replace("\"usedCount\":5", "\"usedCount\":0");
        std::fs::write(&p, format!("AAAA\n{hacked}")).unwrap();
        // 次副本仍有效 → load 取次副本（used_count=5）
        let loaded = store.load().expect("次副本有效");
        assert_eq!(loaded.used_count, 5);
    }

    #[test]
    fn different_machine_key_rejects() {
        let dir = TempDir::new("mach");
        StateStore::new(dir.path(), "machine-A").save(&sample()).unwrap();
        // 换机器（anchor 变）→ HMAC 密钥不同 → 两份都失配 → None（fail-closed）
        let other = StateStore::new(dir.path(), "machine-B");
        assert!(other.load().is_none());
        assert!(other.any_file_exists(), "文件在，但对新机器无效");
    }

    #[test]
    fn stricter_merges_max_counters() {
        let mut a = sample();
        a.used_count = 4;
        a.trial_used = 2;
        let mut b = sample();
        b.used_count = 7;
        b.trial_used = 1;
        b.tamper_flag = true;
        let m = stricter(a, b);
        assert_eq!(m.used_count, 7);
        assert_eq!(m.trial_used, 2);
        assert!(m.tamper_flag);
    }
}
