// 批注仓储：锚定到 文档(+分块/页/引文)，可选关联条款组。
// 原文件永远只读——批注是叠加在预览上的评审记录，不修改文档本身。
use crate::db::now_iso;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnotationRow {
    pub id: String,
    pub workspace_id: String,
    pub document_id: Option<String>,
    pub chunk_id: Option<String>,
    pub cluster_id: Option<String>,
    pub page: Option<i64>,
    pub quote: Option<String>,
    pub note: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewAnnotation<'a> {
    pub workspace_id: &'a str,
    pub document_id: Option<&'a str>,
    pub chunk_id: Option<&'a str>,
    pub cluster_id: Option<&'a str>,
    pub page: Option<i64>,
    pub quote: Option<&'a str>,
    pub note: &'a str,
}

const SELECT: &str = "SELECT id, workspace_id, document_id, chunk_id, cluster_id, page, quote,
  note, created_at, updated_at FROM annotations";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<AnnotationRow> {
    Ok(AnnotationRow {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        document_id: r.get(2)?,
        chunk_id: r.get(3)?,
        cluster_id: r.get(4)?,
        page: r.get(5)?,
        quote: r.get(6)?,
        note: r.get(7)?,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
    })
}

pub fn add(conn: &rusqlite::Connection, a: &NewAnnotation) -> AppResult<AnnotationRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_iso();
    conn.execute(
        "INSERT INTO annotations (id, workspace_id, document_id, chunk_id, cluster_id, page, quote, note, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
        params![id, a.workspace_id, a.document_id, a.chunk_id, a.cluster_id, a.page, a.quote, a.note, now],
    )?;
    conn.query_row(&format!("{SELECT} WHERE id = ?1"), [&id], map_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("批注"))
}

pub fn list_by_workspace(
    conn: &rusqlite::Connection,
    workspace_id: &str,
) -> AppResult<Vec<AnnotationRow>> {
    let mut stmt =
        conn.prepare(&format!("{SELECT} WHERE workspace_id = ?1 ORDER BY created_at"))?;
    let rows = stmt
        .query_map([workspace_id], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

pub fn update_note(conn: &rusqlite::Connection, id: &str, note: &str) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE annotations SET note = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, note, now_iso()],
    )?;
    if n == 0 {
        return Err(AppError::not_found("批注"));
    }
    Ok(())
}

pub fn remove(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM annotations WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(AppError::not_found("批注"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::db::repo::workspace_repo;

    #[test]
    fn annotation_crud_and_cascade() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        let ws = workspace_repo::create(&conn, "w").unwrap();

        let a = add(
            &conn,
            &NewAnnotation {
                workspace_id: &ws.id,
                document_id: None,
                chunk_id: None,
                cluster_id: Some("c-1"),
                page: Some(3),
                quote: Some("报价 1280 万元"),
                note: "与乙份报价仅差 0.8%，重点复核",
            },
        )
        .unwrap();
        assert_eq!(a.page, Some(3));

        let rows = list_by_workspace(&conn, &ws.id).unwrap();
        assert_eq!(rows.len(), 1);

        update_note(&conn, &a.id, "已复核：确认为陪标价").unwrap();
        let rows = list_by_workspace(&conn, &ws.id).unwrap();
        assert!(rows[0].note.contains("陪标价"));
        assert!(rows[0].updated_at >= rows[0].created_at);

        // 删工作区 → 批注级联清理
        workspace_repo::delete(&conn, &ws.id).unwrap();
        assert!(list_by_workspace(&conn, &ws.id).unwrap().is_empty());

        // 删不存在的批注 → NotFound
        assert!(remove(&conn, &a.id).is_err());
    }
}
