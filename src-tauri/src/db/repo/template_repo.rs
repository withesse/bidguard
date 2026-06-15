// 查重源模板仓储：source_templates 表（导入时标记命中样板的分块）。
use crate::db::now_iso;
use crate::error::{AppError, AppResult};
use rusqlite::params;
use serde::Serialize;
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRow {
    pub id: String,
    pub name: String,
    pub text: String,
    /// 分类（可空；旧行/未填为 NULL，前端归一显示「未分类」）。
    pub category: Option<String>,
    pub enabled: bool,
    pub created_at: String,
    /// 命中过该样板的文档数（COUNT(DISTINCT document_id)）；仅反映重新导入后记录的命中。
    pub hit_count: i64,
}

/// 批量插入的单条来源（粘贴/导入解析后的行）。
#[derive(Debug, Clone)]
pub struct NewTemplate {
    pub category: Option<String>,
    pub name: String,
    pub text: String,
}

/// 批量导入结果：新增与跳过（重复/空）条数。
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub inserted: usize,
    pub skipped: usize,
}

const SELECT_COLS: &str = "SELECT t.id, t.name, t.text, t.category, t.enabled, t.created_at,
    (SELECT COUNT(DISTINCT c.document_id) FROM chunks c WHERE c.template_id = t.id) AS hits
    FROM source_templates t";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<TemplateRow> {
    Ok(TemplateRow {
        id: r.get(0)?,
        name: r.get(1)?,
        text: r.get(2)?,
        category: r.get(3)?,
        enabled: r.get::<_, i64>(4)? != 0,
        created_at: r.get(5)?,
        hit_count: r.get(6)?,
    })
}

pub fn list(conn: &rusqlite::Connection) -> AppResult<Vec<TemplateRow>> {
    let mut stmt = conn.prepare(&format!("{SELECT_COLS} ORDER BY t.created_at"))?;
    let rows = stmt.query_map([], map_row)?.collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 启用中的模板 (id, text)，供导入分块时标记命中样板并记录命中的样板 id。
pub fn list_enabled(conn: &rusqlite::Connection) -> AppResult<Vec<(String, String)>> {
    let mut stmt = conn
        .prepare("SELECT id, text FROM source_templates WHERE enabled = 1 ORDER BY created_at")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 新增或更新一条模板。**不触碰 enabled**：就地编辑正文不应重置启停状态（启停走 set_enabled）。
pub fn save(
    conn: &rusqlite::Connection,
    id: Option<&str>,
    name: &str,
    text: &str,
    category: Option<&str>,
) -> AppResult<TemplateRow> {
    let id = id.map(str::to_string).unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let category = category.map(str::trim).filter(|s| !s.is_empty());
    conn.execute(
        "INSERT INTO source_templates (id, name, text, category, enabled, created_at) VALUES (?1, ?2, ?3, ?4, 1, ?5)
         ON CONFLICT(id) DO UPDATE SET name = excluded.name, text = excluded.text, category = excluded.category",
        params![id, name, text, category, now_iso()],
    )?;
    conn.query_row(&format!("{SELECT_COLS} WHERE t.id = ?1"), [&id], map_row)
        .map_err(AppError::from)
}

/// 启用/停用：与 save 解耦，避免编辑正文时意外重置开关。停用后不参与样板剔除（list_enabled 只取 enabled=1）。
pub fn set_enabled(conn: &rusqlite::Connection, id: &str, enabled: bool) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE source_templates SET enabled = ?2 WHERE id = ?1",
        params![id, enabled as i64],
    )?;
    if n == 0 {
        return Err(AppError::not_found("模板"));
    }
    Ok(())
}

/// 批量导入：单事务原子写入。按正文（trim 后精确匹配，含库内与本批内）去重；空名/空正文跳过。
pub fn batch_save(conn: &mut rusqlite::Connection, rows: &[NewTemplate]) -> AppResult<BatchResult> {
    let tx = conn.transaction()?;
    let mut seen: HashSet<String> = HashSet::new();
    {
        let mut stmt = tx.prepare("SELECT text FROM source_templates")?;
        let mut existing = stmt.query([])?;
        while let Some(r) = existing.next()? {
            seen.insert(r.get::<_, String>(0)?);
        }
    }
    let ts = now_iso();
    let mut inserted = 0usize;
    let mut skipped = 0usize;
    for nt in rows {
        let name = nt.name.trim();
        let text = nt.text.trim();
        if name.is_empty() || text.is_empty() || seen.contains(text) {
            skipped += 1;
            continue;
        }
        let id = uuid::Uuid::new_v4().to_string();
        let category = nt.category.as_deref().map(str::trim).filter(|s| !s.is_empty());
        tx.execute(
            "INSERT INTO source_templates (id, name, text, category, enabled, created_at) VALUES (?1, ?2, ?3, ?4, 1, ?5)",
            params![id, name, text, category, ts],
        )?;
        seen.insert(text.to_string());
        inserted += 1;
    }
    tx.commit()?;
    Ok(BatchResult { inserted, skipped })
}

pub fn delete(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM source_templates WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(AppError::not_found("模板"));
    }
    Ok(())
}
