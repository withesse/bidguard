// 查重源模板仓储：source_templates 表（导入时标记命中样板的分块）。
use crate::db::now_iso;
use crate::error::{AppError, AppResult};
use rusqlite::params;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRow {
    pub id: String,
    pub name: String,
    pub text: String,
    pub enabled: bool,
    pub created_at: String,
}

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<TemplateRow> {
    Ok(TemplateRow {
        id: r.get(0)?,
        name: r.get(1)?,
        text: r.get(2)?,
        enabled: r.get::<_, i64>(3)? != 0,
        created_at: r.get(4)?,
    })
}

pub fn list(conn: &rusqlite::Connection) -> AppResult<Vec<TemplateRow>> {
    let mut stmt = conn
        .prepare("SELECT id, name, text, enabled, created_at FROM source_templates ORDER BY created_at")?;
    let rows = stmt.query_map([], map_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn list_enabled_texts(conn: &rusqlite::Connection) -> AppResult<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT text FROM source_templates WHERE enabled = 1 ORDER BY created_at")?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn save(conn: &rusqlite::Connection, id: Option<&str>, name: &str, text: &str) -> AppResult<TemplateRow> {
    let id = id.map(str::to_string).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    conn.execute(
        "INSERT INTO source_templates (id, name, text, enabled, created_at) VALUES (?1, ?2, ?3, 1, ?4)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, text = excluded.text",
        params![id, name, text, now_iso()],
    )?;
    conn.query_row(
        "SELECT id, name, text, enabled, created_at FROM source_templates WHERE id = ?1",
        [&id],
        map_row,
    )
    .map_err(AppError::from)
}

pub fn delete(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM source_templates WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(AppError::not_found("模板"));
    }
    Ok(())
}
