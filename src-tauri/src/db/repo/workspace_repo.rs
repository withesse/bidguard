// 工作区仓储：CRUD + 列表聚合（文档数 / 最近任务状态）。
use crate::db::now_iso;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings_json: Option<String>,
    pub document_count: i64,
    pub latest_job_status: Option<String>,
}

const SELECT: &str = "SELECT w.id, w.name, w.created_at, w.updated_at, w.settings_json,
  (SELECT COUNT(*) FROM documents d WHERE d.workspace_id = w.id),
  (SELECT j.status FROM jobs j WHERE j.workspace_id = w.id ORDER BY j.created_at DESC LIMIT 1)
 FROM workspaces w";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<WorkspaceRow> {
    Ok(WorkspaceRow {
        id: r.get(0)?,
        name: r.get(1)?,
        created_at: r.get(2)?,
        updated_at: r.get(3)?,
        settings_json: r.get(4)?,
        document_count: r.get(5)?,
        latest_job_status: r.get(6)?,
    })
}

pub fn create(conn: &rusqlite::Connection, name: &str) -> AppResult<WorkspaceRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_iso();
    conn.execute(
        "INSERT INTO workspaces (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)",
        params![id, name, now],
    )?;
    get(conn, &id)
}

pub fn list(conn: &rusqlite::Connection) -> AppResult<Vec<WorkspaceRow>> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY w.updated_at DESC"))?;
    let rows = stmt
        .query_map([], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn get(conn: &rusqlite::Connection, id: &str) -> AppResult<WorkspaceRow> {
    conn.query_row(&format!("{SELECT} WHERE w.id = ?1"), [id], map_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("工作区"))
}

pub fn rename(conn: &rusqlite::Connection, id: &str, name: &str) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE workspaces SET name = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, name, now_iso()],
    )?;
    if n == 0 {
        return Err(AppError::not_found("工作区"));
    }
    Ok(())
}

/// 刷新 updated_at（导入文档、跑任务后调用，让列表按活跃度排序）。
pub fn touch(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE workspaces SET updated_at = ?2 WHERE id = ?1",
        params![id, now_iso()],
    )?;
    Ok(())
}

/// 工作区级配置 patch（JSON，覆盖用户全局配置，见 config::resolve）。
pub fn set_settings(conn: &rusqlite::Connection, id: &str, settings_json: Option<&str>) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE workspaces SET settings_json = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, settings_json, now_iso()],
    )?;
    if n == 0 {
        return Err(AppError::not_found("工作区"));
    }
    Ok(())
}

/// 删除工作区。文档/chunk/特征/任务/结果由外键 ON DELETE CASCADE 级联清理。
pub fn delete(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM workspaces WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(AppError::not_found("工作区"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn crud_roundtrip() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();

        let ws = create(&conn, "测试工作区").unwrap();
        assert_eq!(ws.name, "测试工作区");
        assert_eq!(ws.document_count, 0);
        assert!(ws.latest_job_status.is_none());

        rename(&conn, &ws.id, "改名后").unwrap();
        assert_eq!(get(&conn, &ws.id).unwrap().name, "改名后");

        set_settings(&conn, &ws.id, Some(r#"{"compare":{"similarityThreshold":0.8}}"#)).unwrap();
        assert!(get(&conn, &ws.id).unwrap().settings_json.is_some());

        assert_eq!(list(&conn).unwrap().len(), 1);

        delete(&conn, &ws.id).unwrap();
        assert!(get(&conn, &ws.id).is_err());
        assert!(rename(&conn, &ws.id, "x").is_err());
    }

    #[test]
    fn delete_cascades_to_documents() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = create(&conn, "w").unwrap();
        conn.execute(
            "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type, status, created_at, updated_at)
             VALUES ('d1', ?1, 'a.txt', '/tmp/a.txt', 'h', 'txt', 'parsed', 't', 't')",
            [&ws.id],
        )
        .unwrap();
        delete(&conn, &ws.id).unwrap();
        let left: i64 = conn
            .query_row("SELECT COUNT(*) FROM documents", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0, "外键级联应清掉文档");
    }
}
