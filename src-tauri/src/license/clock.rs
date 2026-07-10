// 时间完整性：单调高水位（HWM）+ 有效时刻（防回拨）。
// 离线场景无可用硬件单调计数器（已验证：Windows 用户态 NV counter 被挡、macOS SE 不暴露、
// vTPM 随快照回滚），故 anti-rollback 依赖 app 持久化的 HWM + 多证人，联网时再由签名 serverTime 收紧。
use chrono::{DateTime, Duration, Utc};

/// 回拨容差：当前时间早于 HWM 超过此值才判定为时钟回拨（应对 CMOS 电池死/重镜像等良性场景）。
pub const ROLLBACK_TOLERANCE: Duration = Duration::hours(48);

pub fn now() -> DateTime<Utc> {
    Utc::now()
}

pub fn parse(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s).ok().map(|d| d.with_timezone(&Utc))
}

pub fn to_iso(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// 有效时刻：max(现在, HWM)。回拨系统时钟不能让有效时刻倒退 → 到期判定不可被改钟绕过。
pub fn effective_now(time_hwm: &str) -> DateTime<Utc> {
    let n = now();
    match parse(time_hwm) {
        Some(hwm) if hwm > n => hwm,
        _ => n,
    }
}

/// 取两个 ISO 时刻的较晚者（同为 RFC3339 UTC，按时刻比较）。空串视为极早。
pub fn max_iso(a: &str, b: &str) -> String {
    match (parse(a), parse(b)) {
        (Some(x), Some(y)) => to_iso(x.max(y)),
        (Some(_), None) => a.to_string(),
        (None, Some(_)) => b.to_string(),
        (None, None) => a.to_string(),
    }
}

/// 是否发生了显著回拨（现在比 HWM 早超过容差）。
pub fn is_rollback(time_hwm: &str) -> bool {
    match parse(time_hwm) {
        Some(hwm) => now() < hwm - ROLLBACK_TOLERANCE,
        None => false,
    }
}
