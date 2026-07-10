// 授权/激活总入口。所有授权决策只在 Rust 层做（前端不可信，仅做 UX）。
// 纵深防御：签名许可（不可伪造）> HMAC 状态文件（防删/防改）> 机器绑定 > 时钟高水位。
// MVP 覆盖形态 A（离线签名许可）+ 本地试用；在线激活/心跳/服务端锚定留 v1.1。
pub mod clock;
pub mod fingerprint;
pub mod keys;
pub mod ledger;
pub mod state;
pub mod token;

use crate::db::DbPool;
use crate::error::{AppError, AppErrorCode, AppResult};
use crate::jobs::progress::{JobProgress, JobTerminal, ProgressSink};
use chrono::Duration;
use fingerprint::Fingerprint;
use serde::Serialize;
use state::{LicenseState, StateStore, STATE_VERSION};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use token::LicensePayload;

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
/// MVP 试用：7 天 / 10 次，先到为准（离线本地，接受可重置；v1.1 服务端锚定）。
const TRIAL_DAYS: i64 = 7;
const TRIAL_USES: u64 = 10;

/// 授予结果（供 start_compare 闸门与失败退款使用）。
#[derive(Debug, Clone)]
pub struct Grant {
    pub usage_id: Option<String>, // None = 不限次，无需退款
    pub kind: GrantKind,
    pub license_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GrantKind {
    Licensed,
    Trial,
    Unlimited,
}

/// 前端授权状态卡 / 路由守卫用 DTO。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LicenseStatus {
    /// trial | licensed | grace | expired | exhausted | machineMismatch | unlicensed
    pub state: String,
    pub active: bool,
    pub plan: Option<String>,
    pub licensee_name: Option<String>,
    pub expires_at: Option<String>,
    pub remaining_uses: Option<i64>, // null = 不限次
    pub used_uses: Option<i64>,
    pub trial_expires_at: Option<String>,
    pub machine_code: String,
    pub clock_tamper: bool,
    pub tamper: bool,
    pub message: Option<String>,
}

struct Inner {
    st: LicenseState,
    installed: Option<LicensePayload>,
}

pub struct LicenseManager {
    inner: Mutex<Inner>,
    store: StateStore,
    fp: Fingerprint,
    base: PathBuf,
}

/// 内部裁决：把「状态展示」与「闸门放行」收敛到一份逻辑。
enum Access {
    Allow { consume: Option<GrantKind> }, // None = 不限次
    Deny { code: AppErrorCode, msg: String },
}

struct Assessment {
    state: &'static str,
    active: bool,
    access: Access,
    remaining_uses: Option<i64>,
    used_uses: Option<i64>,
    plan: Option<String>,
    licensee_name: Option<String>,
    expires_at: Option<String>,
    trial_expires_at: Option<String>,
    message: Option<String>,
}

impl LicenseManager {
    /// 启动装载：指纹 → 状态双写读取（fail-closed）→ 时间证人 → 已装许可验签 → 启动对账。
    pub fn load(base: &Path, pool: &DbPool) -> Self {
        let fp = Fingerprint::collect();
        let store = StateStore::new(base, fp.anchor_raw());

        let mut st = match store.load() {
            Some(s) => s,
            None if store.any_file_exists() => {
                // 文件在但两份都无效（被改/被删其一后损坏/换机）→ fail-closed，不白送试用
                log::warn!("授权状态文件校验失败，进入 fail-closed");
                LicenseState {
                    version: STATE_VERSION,
                    install_id: new_id(),
                    tamper_flag: true,
                    trial_exhausted: true,
                    time_hwm: clock::to_iso(clock::now()),
                    initialized: true,
                    ..Default::default()
                }
            }
            None => LicenseState {
                version: STATE_VERSION,
                install_id: new_id(),
                time_hwm: clock::to_iso(clock::now()),
                ..Default::default()
            },
        };

        // 时间证人：不低于 DB 内最新时间戳（删状态文件也不能把 HWM 拉回过去）
        if let Some(w) = db_time_witness(pool) {
            st.time_hwm = clock::max_iso(&st.time_hwm, &w);
        }

        // 已装许可：验签失败/换机不匹配都退化为「无许可」，交由试用/未激活分支
        let installed = read_installed(base);

        let _ = store.save(&st); // 持久化证人/初始化态；失败仅记录，不阻断启动

        let mgr = Self {
            inner: Mutex::new(Inner { st, installed }),
            store,
            fp,
            base: base.to_path_buf(),
        };
        mgr.reconcile_startup(pool);
        mgr
    }

    /// 机器码（形态 A：复制发给运营签发）。
    pub fn machine_code(&self) -> String {
        self.fp.machine_code(APP_VERSION)
    }

    /// 当前授权状态（可能自动开启试用并持久化）。
    pub fn status(&self, pool: &DbPool) -> LicenseStatus {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        self.touch_time(&mut inner);
        self.ensure_trial_started(&mut inner);
        let eff = clock::effective_now(&inner.st.time_hwm);
        let a = self.assess(&inner, eff);
        let _ = self.store.save(&inner.st);
        let _ = pool; // 保留签名一致（未来在线校验用）
        LicenseStatus {
            state: a.state.to_string(),
            active: a.active,
            plan: a.plan,
            licensee_name: a.licensee_name,
            expires_at: a.expires_at,
            remaining_uses: a.remaining_uses,
            used_uses: a.used_uses,
            trial_expires_at: a.trial_expires_at,
            machine_code: self.fp.machine_code(APP_VERSION),
            clock_tamper: inner.st.clock_tamper,
            tamper: inner.st.tamper_flag,
            message: a.message,
        }
    }

    /// start_compare 闸门：校验通过才消费次数；返回 Grant 供失败退款。
    pub fn check_and_consume(&self, pool: &DbPool) -> AppResult<Grant> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        self.touch_time(&mut inner);
        self.ensure_trial_started(&mut inner);
        let eff = clock::effective_now(&inner.st.time_hwm);
        let a = self.assess(&inner, eff);

        match a.access {
            Access::Deny { code, msg } => Err(AppError::new(code, msg)),
            Access::Allow { consume: None } => {
                // 不限次（perpetual / timed 无次数上限）：只校验期限，不消费
                let license_id = inner
                    .installed
                    .as_ref()
                    .map(|l| l.license_id.clone())
                    .unwrap_or_default();
                Ok(Grant { usage_id: None, kind: GrantKind::Unlimited, license_id })
            }
            Access::Allow { consume: Some(kind) } => {
                let usage_id = new_id();
                let license_id = match kind {
                    GrantKind::Licensed => {
                        inner.st.used_count += 1;
                        inner.st.used_count_hwm = inner.st.used_count_hwm.max(inner.st.used_count);
                        inner.installed.as_ref().map(|l| l.license_id.clone()).unwrap_or_default()
                    }
                    GrantKind::Trial => {
                        inner.st.trial_used += 1;
                        "trial".to_string()
                    }
                    GrantKind::Unlimited => unreachable!(),
                };
                self.store.save(&inner.st)?;
                // 审计落库（尽力而为，不因审计失败挡住已放行的任务）
                if let Ok(conn) = pool.get() {
                    let kstr = if kind == GrantKind::Trial { "trial" } else { "licensed" };
                    if let Err(e) = ledger::record_consumed(&conn, &usage_id, &license_id, kstr) {
                        log::warn!("次数审计写入失败 usage={usage_id} code={:?}", e.code);
                    }
                }
                Ok(Grant { usage_id: Some(usage_id), kind, license_id })
            }
        }
    }

    /// 失败/取消退款（幂等）。usage_id 为 None（不限次）时无操作。
    pub fn refund(&self, pool: &DbPool, grant: Grant) {
        let Some(usage_id) = grant.usage_id else { return };
        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            match grant.kind {
                GrantKind::Licensed => {
                    inner.st.used_count = inner.st.used_count.saturating_sub(1);
                }
                GrantKind::Trial => {
                    inner.st.trial_used = inner.st.trial_used.saturating_sub(1);
                }
                GrantKind::Unlimited => {}
            }
            // used_count_hwm 不回落（anti-rollback）
            let _ = self.store.save(&inner.st);
        }
        if let Ok(conn) = pool.get() {
            let _ = ledger::mark_refunded(&conn, &usage_id);
        }
    }

    /// spawn 成功后回填 job_id（审计关联，尽力而为）。
    pub fn attach_job(&self, pool: &DbPool, grant: &Grant, job_id: &str) {
        let Some(usage_id) = &grant.usage_id else { return };
        if let Ok(conn) = pool.get() {
            let _ = ledger::attach_job(&conn, usage_id, job_id);
        }
    }

    /// 导入许可（形态 A/C）：input 为 armored 文本或 .lic 文件路径。
    pub fn import_license(&self, pool: &DbPool, input: &str) -> AppResult<LicenseStatus> {
        let text = if input.contains("BEGIN BIDGUARD") {
            input.to_string()
        } else {
            read_license_path(input)?
        };
        let payload = token::verify_license(&text)?;
        if !self.fp.matches(&payload.machine) {
            return Err(AppError::new(
                AppErrorCode::LicenseMachineMismatch,
                "该许可未绑定到本机，请用本机机器码重新申请",
            ));
        }
        // 落盘 current.lic（下次启动 read_installed 复用）
        let lic_dir = self.base.join("license");
        std::fs::create_dir_all(&lic_dir)?;
        std::fs::write(lic_dir.join("current.lic"), text.as_bytes())?;

        {
            let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
            // 新 license_id → 重置计数（不同许可各自计次）；同 id 重导入 → 保留计数（防重导刷次数）
            if inner.st.license_id.as_deref() != Some(payload.license_id.as_str()) {
                inner.st.license_id = Some(payload.license_id.clone());
                inner.st.used_count = 0;
                inner.st.used_count_hwm = 0;
            }
            inner.st.initialized = true;
            inner.installed = Some(payload);
            self.store.save(&inner.st)?;
        }
        Ok(self.status(pool))
    }

    // —— 内部 ——

    /// 推进时间高水位；检测显著回拨则置 clock_tamper。
    fn touch_time(&self, inner: &mut Inner) {
        let now = clock::now();
        if clock::is_rollback(&inner.st.time_hwm) {
            inner.st.clock_tamper = true;
        }
        let now_iso = clock::to_iso(now);
        inner.st.time_hwm = clock::max_iso(&inner.st.time_hwm, &now_iso);
    }

    /// 无已装许可且从未开启试用 → 自动开启（一次性）。
    fn ensure_trial_started(&self, inner: &mut Inner) {
        if inner.installed.is_some() {
            return;
        }
        if inner.st.trial_started_at.is_none() && !inner.st.trial_exhausted {
            let now = clock::now();
            inner.st.trial_started_at = Some(clock::to_iso(now));
            inner.st.trial_expires_at = Some(clock::to_iso(now + Duration::days(TRIAL_DAYS)));
            inner.st.trial_max_uses = TRIAL_USES;
            inner.st.initialized = true;
        }
    }

    fn assess(&self, inner: &Inner, eff: chrono::DateTime<chrono::Utc>) -> Assessment {
        if let Some(lic) = &inner.installed {
            return self.assess_licensed(inner, lic, eff);
        }
        self.assess_trial(inner, eff)
    }

    fn assess_licensed(
        &self,
        inner: &Inner,
        lic: &LicensePayload,
        eff: chrono::DateTime<chrono::Utc>,
    ) -> Assessment {
        let base = |access, state: &'static str, active, remaining, used, message| Assessment {
            state,
            active,
            access,
            remaining_uses: remaining,
            used_uses: used,
            plan: Some(lic.plan.clone()),
            licensee_name: Some(lic.licensee_name.clone()),
            expires_at: lic.expires_at.clone(),
            trial_expires_at: None,
            message,
        };

        if !self.fp.matches(&lic.machine) {
            return base(
                Access::Deny {
                    code: AppErrorCode::LicenseMachineMismatch,
                    msg: "许可未绑定到本机，请重新激活".into(),
                },
                "machineMismatch",
                false,
                None,
                None,
                Some("此许可绑定的机器与当前设备不一致".into()),
            );
        }

        let remaining = lic.max_uses.map(|m| m as i64 - inner.st.used_count as i64);
        let used = Some(inner.st.used_count as i64);

        // 到期判定（绝对时刻，用有效时刻防回拨）
        let exp = lic.expires_at.as_deref().and_then(clock::parse);
        let expired = exp.map(|e| eff > e).unwrap_or(false);
        let in_grace = match (expired, exp) {
            (true, Some(e)) => eff <= e + Duration::days(lic.grace_days as i64),
            _ => false,
        };

        if expired && !in_grace {
            return base(
                Access::Deny {
                    code: AppErrorCode::LicenseExpired,
                    msg: "授权已到期，请续期".into(),
                },
                "expired",
                false,
                remaining,
                used,
                Some("授权已到期".into()),
            );
        }
        if let Some(r) = remaining {
            if r <= 0 {
                return base(
                    Access::Deny {
                        code: AppErrorCode::LicenseExhausted,
                        msg: "使用次数已用尽，请续购".into(),
                    },
                    "exhausted",
                    false,
                    Some(0),
                    used,
                    Some("使用次数已用尽".into()),
                );
            }
        }

        let consume = if lic.max_uses.is_some() {
            Some(GrantKind::Licensed)
        } else {
            None
        };
        let (state, message) = if in_grace {
            ("grace", Some("授权已到期，宽限期内仍可使用，请尽快续期".to_string()))
        } else if inner.st.clock_tamper {
            ("licensed", Some("检测到系统时间异常，请校正系统时钟".to_string()))
        } else {
            ("licensed", None)
        };
        base(Access::Allow { consume }, state, true, remaining, used, message)
    }

    fn assess_trial(&self, inner: &Inner, eff: chrono::DateTime<chrono::Utc>) -> Assessment {
        let mk = |access, state: &'static str, active, remaining, used, message| Assessment {
            state,
            active,
            access,
            remaining_uses: remaining,
            used_uses: used,
            plan: Some("trial".to_string()),
            licensee_name: None,
            expires_at: None,
            trial_expires_at: inner.st.trial_expires_at.clone(),
            message,
        };

        if inner.st.trial_exhausted {
            return mk(
                deny_required("试用已结束，请激活后使用"),
                "unlicensed",
                false,
                Some(0),
                Some(inner.st.trial_used as i64),
                Some("试用已结束".into()),
            );
        }

        let trial_expired = inner
            .st
            .trial_expires_at
            .as_deref()
            .and_then(clock::parse)
            .map(|e| eff > e)
            .unwrap_or(true);
        let remaining = inner.st.trial_max_uses as i64 - inner.st.trial_used as i64;
        let used = Some(inner.st.trial_used as i64);

        if trial_expired {
            return mk(
                deny_required("试用期已到期，请激活后使用"),
                "unlicensed",
                false,
                Some(remaining.max(0)),
                used,
                Some("试用期已到期".into()),
            );
        }
        if remaining <= 0 {
            return mk(
                deny_required("试用次数已用完，请激活后使用"),
                "unlicensed",
                false,
                Some(0),
                used,
                Some("试用次数已用完".into()),
            );
        }
        mk(
            Access::Allow { consume: Some(GrantKind::Trial) },
            "trial",
            true,
            Some(remaining),
            used,
            None,
        )
    }

    /// 启动对账：进程被杀致 RefundSink 未触发的消费行，其 job 现为失败/取消 → 退款。
    fn reconcile_startup(&self, pool: &DbPool) {
        let Ok(conn) = pool.get() else { return };
        let rows = match ledger::consumed_for_failed_jobs(&conn) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("启动对账查询失败 code={:?}", e.code);
                return;
            }
        };
        if rows.is_empty() {
            return;
        }
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        for (usage_id, kind) in rows {
            match kind.as_str() {
                "trial" => inner.st.trial_used = inner.st.trial_used.saturating_sub(1),
                _ => inner.st.used_count = inner.st.used_count.saturating_sub(1),
            }
            let _ = ledger::mark_refunded(&conn, &usage_id);
        }
        let _ = self.store.save(&inner.st);
    }
}

fn deny_required(msg: &str) -> Access {
    Access::Deny {
        code: AppErrorCode::LicenseRequired,
        msg: msg.into(),
    }
}

fn new_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 读取已装许可（current.lic）并验签；不存在/验签失败均返回 None。
fn read_installed(base: &Path) -> Option<LicensePayload> {
    let path = base.join("license").join("current.lic");
    let text = std::fs::read_to_string(path).ok()?;
    match token::verify_license(&text) {
        Ok(p) => Some(p),
        Err(e) => {
            log::warn!("已装许可验签失败 code={:?}", e.code);
            None
        }
    }
}

fn read_license_path(path: &str) -> AppResult<String> {
    let p = Path::new(path);
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    if !matches!(ext.as_str(), "lic" | "txt") {
        return Err(AppError::new(AppErrorCode::LicenseInvalid, "仅支持 .lic 许可文件"));
    }
    std::fs::read_to_string(p).map_err(|e| {
        AppError::new(AppErrorCode::LicenseInvalid, "读取许可文件失败").with_detail(e.to_string())
    })
}

fn db_time_witness(pool: &DbPool) -> Option<String> {
    let conn = pool.get().ok()?;
    let q = |sql: &str| -> Option<String> {
        conn.query_row(sql, [], |r| r.get::<_, Option<String>>(0)).ok().flatten()
    };
    [q("SELECT MAX(created_at) FROM jobs"), q("SELECT MAX(updated_at) FROM documents")]
        .into_iter()
        .flatten()
        .reduce(|x, y| clock::max_iso(&x, &y))
}

/// 失败退款装饰 sink：包裹真实 sink，终态非 completed 时退款（幂等，只取一次 Grant）。
pub struct RefundSink {
    inner: Arc<dyn ProgressSink>,
    license: Arc<LicenseManager>,
    db: DbPool,
    grant: Mutex<Option<Grant>>,
}

impl RefundSink {
    pub fn new(
        inner: Arc<dyn ProgressSink>,
        license: Arc<LicenseManager>,
        db: DbPool,
        grant: Grant,
    ) -> Self {
        Self {
            inner,
            license,
            db,
            grant: Mutex::new(Some(grant)),
        }
    }
}

impl ProgressSink for RefundSink {
    fn emit_progress(&self, p: &JobProgress) {
        self.inner.emit_progress(p);
    }
    fn emit_terminal(&self, t: &JobTerminal) {
        let grant = self.grant.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(g) = grant {
            if t.status != "completed" {
                self.license.refund(&self.db, g);
            }
            // completed：保留消费，Grant 丢弃即可
        }
        self.inner.emit_terminal(t);
    }
}
