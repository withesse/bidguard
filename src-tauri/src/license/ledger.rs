// 次数使用审计（license_usage 表）。
// 注意：enforcement 的次数计数在 HMAC 状态文件（license::state），DB 仅为审计与
// 「失败退款/启动对账」的落点——SQLite 可被直接改写，不作为强制计数源。
use crate::db::now_iso;
use crate::error::AppResult;
use rusqlite::params;

/// 记录一次消费（consumed）。job_id 稍后由 attach_job 回填。
pub fn record_consumed(
    conn: &rusqlite::Connection,
    usage_id: &str,
    license_id: &str,
    kind: &str, // licensed | trial
) -> AppResult<()> {
    let now = now_iso();
    conn.execute(
        "INSERT INTO license_usage (id, license_id, job_id, kind, state, created_at, updated_at)
         VALUES (?1, ?2, NULL, ?3, 'consumed', ?4, ?4)",
        params![usage_id, license_id, kind, now],
    )?;
    Ok(())
}

/// 回填 job_id（消费发生在 spawn 之前，job_id 之后才知）。尽力而为。
pub fn attach_job(conn: &rusqlite::Connection, usage_id: &str, job_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE license_usage SET job_id = ?1, updated_at = ?2 WHERE id = ?3 AND state = 'consumed'",
        params![job_id, now_iso(), usage_id],
    )?;
    Ok(())
}

/// 标记退款（幂等：仅对 consumed 行生效）。
pub fn mark_refunded(conn: &rusqlite::Connection, usage_id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE license_usage SET state = 'refunded', updated_at = ?1 WHERE id = ?2 AND state = 'consumed'",
        params![now_iso(), usage_id],
    )?;
    Ok(())
}

/// 某许可的净消费行数（consumed，不含 refunded）。
/// 启动时作为 HMAC 状态文件的「较严证人」：状态被整删重建后账本仍在（删 DB 会连工作区
/// 一起失去，代价自担），ledger > state 即证明计数被回滚，按账本恢复。
pub fn net_consumed_count(conn: &rusqlite::Connection, license_id: &str) -> AppResult<u64> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM license_usage WHERE license_id = ?1 AND state = 'consumed'",
        params![license_id],
        |r| r.get(0),
    )?;
    Ok(n.max(0) as u64)
}

/// 是否存在任何历史试用消费行（含已退款——行存在本身即证明试用曾开始过）。
/// 用于识别「删状态文件白拿新试用」：状态显示从未试用而账本有痕迹 → fail-closed。
pub fn trial_evidence_exists(conn: &rusqlite::Connection) -> AppResult<bool> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM license_usage WHERE kind = 'trial'", [], |r| {
        r.get(0)
    })?;
    Ok(n > 0)
}

/// 启动对账：进程被杀导致 RefundSink 未触发的消费行——其 job 现已 failed/cancelled，
/// 应退款。返回 (usage_id, kind) 供调用方回落计数。仅取 consumed 且 job 为失败/取消态。
pub fn consumed_for_failed_jobs(conn: &rusqlite::Connection) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT u.id, u.kind FROM license_usage u
         JOIN jobs j ON j.id = u.job_id
         WHERE u.state = 'consumed' AND j.status IN ('failed', 'cancelled')",
    )?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
