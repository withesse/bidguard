// 端到端授权流程测试：keygen 签发 → verify_strict → 导入 → 计次消费 → 用尽 → 退款。
// 在测试内自签（等价于 bidguard-keygen issue），既验证流程，也充当
// 「keygen 令牌格式 == app 验签格式」的契约测试。
//
// 两条运行路径：
// 1) 临时密钥对（固定种子）+ load_with_keys 注入 —— 恒运行，CI 无需任何真实私钥；
// 2) 内嵌开发公钥路径 —— 需私钥种子（base64url）：环境变量 BIDGUARD_DEV_PRIV，
//    未设置则跳过。BIDGUARD_DEV_PRIV=<seed> cargo test --test license_flow -- --nocapture
use bidguard_lib::db;
use bidguard_lib::error::AppErrorCode;
use bidguard_lib::license::fingerprint::machine_code_encoding;
use bidguard_lib::license::{token, GrantKind, LicenseManager};
use data_encoding::BASE64URL_NOPAD;
use ed25519_dalek::{Signer, SigningKey};

fn dev_key() -> Option<SigningKey> {
    let seed_b64 = std::env::var("BIDGUARD_DEV_PRIV").ok()?;
    let seed = BASE64URL_NOPAD.decode(seed_b64.trim().as_bytes()).ok()?;
    let seed: [u8; 32] = seed.try_into().ok()?;
    Some(SigningKey::from_bytes(&seed))
}

// —— 临时密钥对路径（恒运行）——

const TEST_SEED: [u8; 32] = *b"bidguard.flow.integ.test.seed.01";
const TEST_KID: &str = "lic-test-ci";

fn test_signing_key() -> SigningKey {
    SigningKey::from_bytes(&TEST_SEED)
}

/// 注入给 LicenseManager 的 kid→公钥解析器（fn 指针，故公钥由固定种子现场重导出）。
fn test_resolve(kid: &str) -> Option<[u8; 32]> {
    (kid == TEST_KID).then(|| test_signing_key().verifying_key().to_bytes())
}

/// 解码 app 机器码（BG2 · Crockford base32）→ 取 anchorHash / componentHashes。
/// keygen 的解码侧必须与 machine_code_encoding() 一致——本函数即该契约的测试化身。
fn decode_machine_code(code: &str) -> (String, Vec<String>) {
    let body = code.strip_prefix("BG2-").unwrap_or(code).replace('-', "");
    let bytes = machine_code_encoding().decode(body.as_bytes()).expect("机器码 Crockford base32");
    let v: serde_json::Value = serde_json::from_slice(&bytes).expect("机器码 json");
    let anchor = v["anchorHash"].as_str().unwrap_or_default().to_string();
    let comps = v["componentHashes"]
        .as_array()
        .map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect())
        .unwrap_or_default();
    (anchor, comps)
}

/// 自签一张许可（与 keygen 完全同格式）。
fn issue(sk: &SigningKey, kid: &str, machine_code: &str, plan: &str, uses: Option<u64>, days: Option<i64>) -> String {
    let (anchor, comps) = decode_machine_code(machine_code);
    let now = chrono::Utc::now();
    let issued = now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let expires = days.map(|d| (now + chrono::Duration::days(d)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    let threshold = comps.len() as u32;

    let header = serde_json::json!({ "v": 1, "alg": "Ed25519", "kid": kid });
    let mut payload = serde_json::json!({
        "licenseId": uuid::Uuid::new_v4().to_string(),
        "licenseeName": "测试代理机构",
        "plan": plan,
        "issuedAt": issued,
        "maxMachines": 1,
        "machine": { "anchorHash": anchor, "componentHashes": comps, "matchThreshold": threshold },
        "leaseTtlDays": 90,
        "graceDays": 14,
        "features": ["compare"],
    });
    if let Some(u) = uses {
        payload["maxUses"] = serde_json::json!(u);
    }
    if let Some(e) = expires {
        payload["expiresAt"] = serde_json::json!(e);
    }

    let h = BASE64URL_NOPAD.encode(&serde_json::to_vec(&header).unwrap());
    let p = BASE64URL_NOPAD.encode(&serde_json::to_vec(&payload).unwrap());
    let signing_input = format!("{h}.{p}");
    let sig = sk.sign(signing_input.as_bytes());
    let body = format!("{signing_input}.{}", BASE64URL_NOPAD.encode(&sig.to_bytes()));
    format!("-----BEGIN BIDGUARD LICENSE-----\n{body}\n-----END BIDGUARD LICENSE-----\n")
}

fn temp_base(tag: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(format!("bidguard-flow-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&p);
    std::fs::create_dir_all(&p).unwrap();
    p
}

#[test]
fn counted_license_consume_exhaust_refund() {
    let Some(sk) = dev_key() else {
        eprintln!("跳过：未设置 BIDGUARD_DEV_PRIV");
        return;
    };
    let base = temp_base("counted");
    let pool = db::open(&base).unwrap();
    let mgr = LicenseManager::load(&base, &pool);

    // 首启即试用态
    let s0 = mgr.status(&pool);
    assert_eq!(s0.state, "trial", "首启应为试用，实际 {}", s0.state);

    // 签发 3 次的按次许可并导入
    let code = mgr.machine_code();
    let lic = issue(&sk, "lic-dev-2026a", &code, "counted", Some(3), None);
    let s1 = mgr.import_license(&pool, &lic).unwrap();
    assert_eq!(s1.state, "licensed");
    assert_eq!(s1.remaining_uses, Some(3));
    assert_eq!(s1.licensee_name.as_deref(), Some("测试代理机构"));

    // 消费 3 次
    let g1 = mgr.check_and_consume(&pool).unwrap();
    assert_eq!(g1.kind, GrantKind::Licensed);
    let _g2 = mgr.check_and_consume(&pool).unwrap();
    let _g3 = mgr.check_and_consume(&pool).unwrap();
    assert_eq!(mgr.status(&pool).remaining_uses, Some(0));

    // 第 4 次：用尽
    let err = mgr.check_and_consume(&pool).unwrap_err();
    assert_eq!(err.code, AppErrorCode::LicenseExhausted);

    // 退款一次 → 余 1，可再消费
    mgr.refund(&pool, g1);
    assert_eq!(mgr.status(&pool).remaining_uses, Some(1));
    let _g = mgr.check_and_consume(&pool).unwrap();
    assert_eq!(mgr.status(&pool).remaining_uses, Some(0));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn machine_mismatch_rejected() {
    let Some(sk) = dev_key() else {
        return;
    };
    let base = temp_base("mismatch");
    let pool = db::open(&base).unwrap();
    let mgr = LicenseManager::load(&base, &pool);

    // 用一个错误的机器码签发（anchor 不属于本机）
    let bogus = "BG2-".to_string()
        + &machine_code_encoding().encode(
            serde_json::to_vec(&serde_json::json!({
                "v":2, "anchorHash":"deadbeef", "componentHashes":[], "appVersion":"x"
            }))
            .unwrap()
            .as_slice(),
        );
    let lic = issue(&sk, "lic-dev-2026a", &bogus, "counted", Some(5), None);
    let err = mgr.import_license(&pool, &lic).unwrap_err();
    assert_eq!(err.code, AppErrorCode::LicenseMachineMismatch);

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn tampered_signature_rejected() {
    // 不需私钥：构造一张签名被改的许可，验签必失败
    let fake = "-----BEGIN BIDGUARD LICENSE-----\neyJ2IjoxLCJhbGciOiJFZDI1NTE5Iiwia2lkIjoibGljLWRldi0yMDI2YSJ9.eyJhIjoxfQ.AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA\n-----END BIDGUARD LICENSE-----";
    assert_eq!(token::verify_license(fake).unwrap_err().code, AppErrorCode::LicenseInvalid);
}

// —— 以下用临时密钥对 + load_with_keys 注入，恒运行（CI 覆盖营收闭环）——

#[test]
fn counted_flow_with_ephemeral_key() {
    let sk = test_signing_key();
    let base = temp_base("ephemeral");
    let pool = db::open(&base).unwrap();
    let mgr = LicenseManager::load_with_keys(&base, &pool, test_resolve);

    // 首启即试用态
    assert_eq!(mgr.status(&pool).state, "trial");

    // 签发 2 次的按次许可并导入
    let code = mgr.machine_code();
    let lic = issue(&sk, TEST_KID, &code, "counted", Some(2), None);
    let s1 = mgr.import_license(&pool, &lic).unwrap();
    assert_eq!(s1.state, "licensed");
    assert_eq!(s1.remaining_uses, Some(2));

    // 消费 2 次 → 用尽
    let g1 = mgr.check_and_consume(&pool).unwrap();
    assert_eq!(g1.kind, GrantKind::Licensed);
    let _g2 = mgr.check_and_consume(&pool).unwrap();
    assert_eq!(mgr.check_and_consume(&pool).unwrap_err().code, AppErrorCode::LicenseExhausted);

    // 退款一次 → 余 1
    mgr.refund(&pool, g1);
    assert_eq!(mgr.status(&pool).remaining_uses, Some(1));

    // 重启装载（read_installed 同走注入公钥），计数与许可均持久
    drop(mgr);
    let mgr2 = LicenseManager::load_with_keys(&base, &pool, test_resolve);
    let s2 = mgr2.status(&pool);
    assert_eq!(s2.state, "licensed");
    assert_eq!(s2.remaining_uses, Some(1));

    // 生产验签路径（内嵌 TRUSTED_KEYS）必须不认这张临时密钥许可：装载后退回非 licensed 态
    drop(mgr2);
    let mgr3 = LicenseManager::load(&base, &pool);
    assert_ne!(mgr3.status(&pool).state, "licensed");

    let _ = std::fs::remove_dir_all(&base);
}

/// 试用时钟自首次比对起算：装好只看不用，不烧试用期。
#[test]
fn trial_clock_starts_on_first_consume_not_on_status() {
    let base = temp_base("trial-clock");
    let pool = db::open(&base).unwrap();

    // 看状态若干次 + 重启装载：均不得起算时钟
    let mgr = LicenseManager::load_with_keys(&base, &pool, test_resolve);
    let s0 = mgr.status(&pool);
    assert_eq!(s0.state, "trial");
    assert!(s0.active);
    assert!(s0.trial_expires_at.is_none(), "仅查看状态不应开始计时");
    let _ = mgr.status(&pool);
    drop(mgr);
    let mgr = LicenseManager::load_with_keys(&base, &pool, test_resolve);
    assert!(mgr.status(&pool).trial_expires_at.is_none(), "重启装载也不应开始计时");

    // 首次消费才起算
    let g = mgr.check_and_consume(&pool).unwrap();
    assert_eq!(g.kind, GrantKind::Trial);
    let s1 = mgr.status(&pool);
    assert!(s1.trial_expires_at.is_some(), "首次比对后时钟应已起算");
    assert_eq!(s1.remaining_uses, Some(9));

    let _ = std::fs::remove_dir_all(&base);
}

/// 删掉两份 HMAC 状态文件后，账本证人从审计记录恢复较严计数（按次许可不清零、试用不复活）。
#[test]
fn deleting_state_files_does_not_reset_counters() {
    let sk = test_signing_key();
    let base = temp_base("witness");
    let pool = db::open(&base).unwrap();

    // 试用消费 1 次 + 导入 3 次许可再消费 2 次
    let mgr = LicenseManager::load_with_keys(&base, &pool, test_resolve);
    let _ = mgr.check_and_consume(&pool).unwrap(); // trial
    let code = mgr.machine_code();
    let lic = issue(&sk, TEST_KID, &code, "counted", Some(3), None);
    mgr.import_license(&pool, &lic).unwrap();
    let _ = mgr.check_and_consume(&pool).unwrap();
    let _ = mgr.check_and_consume(&pool).unwrap();
    assert_eq!(mgr.status(&pool).remaining_uses, Some(1));
    drop(mgr);

    // 攻击：删两份状态文件（保留 current.lic 与 DB）
    std::fs::remove_file(base.join("license").join("state.bin")).unwrap();
    std::fs::remove_file(base.join(".bidguard.lst")).unwrap();

    // 重启：计数由账本恢复为 2（而非清零），并标记 tamper
    let mgr2 = LicenseManager::load_with_keys(&base, &pool, test_resolve);
    let s = mgr2.status(&pool);
    assert_eq!(s.state, "licensed");
    assert_eq!(s.remaining_uses, Some(1), "删状态文件不得重置已用次数");
    assert!(s.tamper, "状态重建应标记 tamper");

    // 「删状态→重导入同一张许可」也不得洗计数
    mgr2.import_license(&pool, &lic).unwrap();
    assert_eq!(mgr2.status(&pool).remaining_uses, Some(1), "重导入同一许可不得清零计数");
    drop(mgr2);

    // 移除许可后：试用不复活（账本有历史试用行 → fail-closed）
    std::fs::remove_file(base.join("license").join("state.bin")).unwrap();
    std::fs::remove_file(base.join(".bidguard.lst")).unwrap();
    std::fs::remove_file(base.join("license").join("current.lic")).unwrap();
    let mgr3 = LicenseManager::load_with_keys(&base, &pool, test_resolve);
    let s3 = mgr3.status(&pool);
    assert_eq!(s3.state, "unlicensed", "试用不得复活，实际 {}", s3.state);
    assert!(!s3.active);

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn machine_mismatch_rejected_with_ephemeral_key() {
    let sk = test_signing_key();
    let base = temp_base("ephemeral-mismatch");
    let pool = db::open(&base).unwrap();
    let mgr = LicenseManager::load_with_keys(&base, &pool, test_resolve);

    let bogus = "BG2-".to_string()
        + &machine_code_encoding().encode(
            serde_json::to_vec(&serde_json::json!({
                "v":2, "anchorHash":"deadbeef", "componentHashes":[], "appVersion":"x"
            }))
            .unwrap()
            .as_slice(),
        );
    let lic = issue(&sk, TEST_KID, &bogus, "counted", Some(5), None);
    assert_eq!(mgr.import_license(&pool, &lic).unwrap_err().code, AppErrorCode::LicenseMachineMismatch);

    let _ = std::fs::remove_dir_all(&base);
}
