// 任务仓储：jobs 表（import / compare / export 共用），状态机的每次跃迁都落库。
use crate::db::now_iso;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRow {
    pub id: String,
    pub workspace_id: String,
    pub job_type: String, // import | compare | export
    pub name: Option<String>,
    pub status: String, // pending | running | cancelling | cancelled | failed | completed
    pub config_json: String,
    pub progress: f64,
    pub message: Option<String>,
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub starred: bool,
    /// 列表迷你矩阵直接用，避免每行再查详情
    pub matrix_json: Option<String>,
    /// collusion_json 里提出来的 level（high|medium|low|none），列表「需复核」徽标用
    pub collusion_level: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
}

const SELECT: &str = "SELECT id, workspace_id, job_type, name, status, config_json, progress,
  message, error_message, error_code, starred, matrix_json,
  json_extract(collusion_json, '$.level'),
  created_at, started_at, finished_at FROM jobs";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<JobRow> {
    Ok(JobRow {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        job_type: r.get(2)?,
        name: r.get(3)?,
        status: r.get(4)?,
        config_json: r.get(5)?,
        progress: r.get(6)?,
        message: r.get(7)?,
        error_message: r.get(8)?,
        error_code: r.get(9)?,
        starred: r.get::<_, i64>(10)? != 0,
        matrix_json: r.get(11)?,
        collusion_level: r.get(12)?,
        created_at: r.get(13)?,
        started_at: r.get(14)?,
        finished_at: r.get(15)?,
    })
}

pub fn create(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    job_type: &str,
    name: Option<&str>,
    config_json: &str,
) -> AppResult<JobRow> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO jobs (id, workspace_id, job_type, name, status, config_json, created_at)
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)",
        params![id, workspace_id, job_type, name, config_json, now_iso()],
    )?;
    get(conn, &id)
}

pub fn get(conn: &rusqlite::Connection, id: &str) -> AppResult<JobRow> {
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [id], map_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("任务"))
}

pub fn list(conn: &rusqlite::Connection, workspace_id: Option<&str>) -> AppResult<Vec<JobRow>> {
    let sql = match workspace_id {
        Some(_) => format!("{SELECT} WHERE workspace_id = ?1 ORDER BY created_at DESC"),
        None => format!("{SELECT} ORDER BY created_at DESC"),
    };
    let mut stmt = conn.prepare(&sql)?;
    let rows = match workspace_id {
        Some(ws) => stmt.query_map([ws], map_row)?.collect::<Result<Vec<_>, _>>()?,
        None => stmt.query_map([], map_row)?.collect::<Result<Vec<_>, _>>()?,
    };
    Ok(rows)
}

/// 同工作区是否已有同类型的未完结任务（防止重复运行冲突任务）。
pub fn has_active(conn: &rusqlite::Connection, workspace_id: &str, job_type: &str) -> AppResult<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM jobs WHERE workspace_id = ?1 AND job_type = ?2
         AND status IN ('pending', 'running', 'cancelling')",
        params![workspace_id, job_type],
        |r| r.get(0),
    )?;
    Ok(n > 0)
}

pub fn set_running(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE jobs SET status = 'running', started_at = ?2 WHERE id = ?1",
        params![id, now_iso()],
    )?;
    Ok(())
}

pub fn set_cancelling(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE jobs SET status = 'cancelling' WHERE id = ?1 AND status IN ('pending', 'running')",
        [id],
    )?;
    Ok(())
}

pub fn set_progress(conn: &rusqlite::Connection, id: &str, progress: f64, message: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE jobs SET progress = ?2, message = ?3 WHERE id = ?1",
        params![id, progress, message],
    )?;
    Ok(())
}

/// 终态落库。status: completed | failed | cancelled
pub fn finish(
    conn: &rusqlite::Connection,
    id: &str,
    status: &str,
    error_code: Option<&str>,
    error_message: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE jobs SET status = ?2, error_code = ?3, error_message = ?4, finished_at = ?5,
         progress = CASE WHEN ?2 = 'completed' THEN 1.0 ELSE progress END
         WHERE id = ?1",
        params![id, status, error_code, error_message, now_iso()],
    )?;
    Ok(())
}

/// 比对聚合结果落库（五个 JSON 列，总览/矩阵/围标/共有词/章节热力）。
pub fn set_compare_results(
    conn: &rusqlite::Connection,
    id: &str,
    summary_json: &str,
    matrix_json: &str,
    collusion_json: &str,
    shared_terms_json: &str,
    sections_json: &str,
) -> AppResult<()> {
    conn.execute(
        "UPDATE jobs SET summary_json = ?2, matrix_json = ?3, collusion_json = ?4,
         shared_terms_json = ?5, sections_json = ?6 WHERE id = ?1",
        params![id, summary_json, matrix_json, collusion_json, shared_terms_json, sections_json],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobResultJsons {
    pub summary_json: Option<String>,
    pub matrix_json: Option<String>,
    pub collusion_json: Option<String>,
    pub shared_terms_json: Option<String>,
    pub sections_json: Option<String>,
}

pub fn get_result_jsons(conn: &rusqlite::Connection, id: &str) -> AppResult<JobResultJsons> {
    conn.query_row(
        "SELECT summary_json, matrix_json, collusion_json, shared_terms_json, sections_json
         FROM jobs WHERE id = ?1",
        [id],
        |r| {
            Ok(JobResultJsons {
                summary_json: r.get(0)?,
                matrix_json: r.get(1)?,
                collusion_json: r.get(2)?,
                shared_terms_json: r.get(3)?,
                sections_json: r.get(4)?,
            })
        },
    )
    .optional()?
    .ok_or_else(|| AppError::not_found("任务"))
}

pub fn set_starred(conn: &rusqlite::Connection, id: &str, starred: bool) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE jobs SET starred = ?2 WHERE id = ?1",
        params![id, starred as i64],
    )?;
    if n == 0 {
        return Err(AppError::not_found("任务"));
    }
    Ok(())
}

/// 清理 N 天前已完结（completed/failed/cancelled）的任务，返回清理数。
/// 收藏（starred）的任务不清理；candidate_edges/clusters 级联删除。
pub fn delete_finished_older_than(conn: &rusqlite::Connection, days: u32) -> AppResult<usize> {
    let n = conn.execute(
        "DELETE FROM jobs
         WHERE status IN ('completed', 'failed', 'cancelled')
           AND starred = 0
           AND created_at < datetime('now', ?1)",
        [format!("-{days} days")],
    )?;
    Ok(n)
}

/// 删除任务（候选边/聚类/成员/diff 级联清理）。运行中的任务不允许删。
pub fn delete(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    let job = get(conn, id)?;
    if matches!(job.status.as_str(), "pending" | "running" | "cancelling") {
        return Err(AppError::new(
            crate::error::AppErrorCode::JobConflict,
            "任务正在运行，请先取消再删除",
        ));
    }
    conn.execute("DELETE FROM jobs WHERE id = ?1", [id])?;
    Ok(())
}

/// 启动自检：上次运行残留的未完结任务全部判失败（进程已死，任务不可能还在跑）。
pub fn mark_stale_as_failed(conn: &rusqlite::Connection) -> AppResult<usize> {
    let n = conn.execute(
        "UPDATE jobs SET status = 'failed', error_code = 'unknown',
         error_message = '应用重启，任务中断', finished_at = ?1
         WHERE status IN ('pending', 'running', 'cancelling')",
        [now_iso()],
    )?;
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::db::repo::workspace_repo;

    #[test]
    fn lifecycle_and_stale_cleanup() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = workspace_repo::create(&conn, "w").unwrap();

        let job = create(&conn, &ws.id, "import", Some("导入"), "{}").unwrap();
        assert_eq!(job.status, "pending");
        assert!(has_active(&conn, &ws.id, "import").unwrap());
        assert!(!has_active(&conn, &ws.id, "compare").unwrap());

        set_running(&conn, &job.id).unwrap();
        set_progress(&conn, &job.id, 0.5, "解析中").unwrap();
        let j = get(&conn, &job.id).unwrap();
        assert_eq!(j.status, "running");
        assert!((j.progress - 0.5).abs() < 1e-9);

        finish(&conn, &job.id, "completed", None, None).unwrap();
        let j = get(&conn, &job.id).unwrap();
        assert_eq!(j.status, "completed");
        assert!((j.progress - 1.0).abs() < 1e-9);
        assert!(j.finished_at.is_some());
        assert!(!has_active(&conn, &ws.id, "import").unwrap());

        // 残留任务清理
        let j2 = create(&conn, &ws.id, "compare", None, "{}").unwrap();
        set_running(&conn, &j2.id).unwrap();
        assert_eq!(mark_stale_as_failed(&conn).unwrap(), 1);
        assert_eq!(get(&conn, &j2.id).unwrap().status, "failed");
    }

    #[test]
    fn cancelling_only_from_live_states() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = workspace_repo::create(&conn, "w").unwrap();
        let job = create(&conn, &ws.id, "import", None, "{}").unwrap();
        finish(&conn, &job.id, "completed", None, None).unwrap();
        set_cancelling(&conn, &job.id).unwrap();
        // 已完结任务不应被改回 cancelling
        assert_eq!(get(&conn, &job.id).unwrap().status, "completed");
    }
}
