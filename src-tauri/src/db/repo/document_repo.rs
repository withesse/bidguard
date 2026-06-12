// 文档仓储：documents 表 + 按 file_hash 的去重查询。
use crate::db::now_iso;
use crate::error::{AppError, AppResult};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentRow {
    pub id: String,
    pub workspace_id: String,
    pub file_name: String,
    pub file_path: String,
    pub file_hash: String,
    pub file_type: String,
    pub status: String, // pending | parsing | parsed | failed
    pub parse_error: Option<String>,
    pub parse_method: Option<String>, // docx | text | pdfium | pdf-extract | ocr | cache
    pub page_count: Option<i64>,
    pub char_count: Option<i64>,
    pub fingerprint_json: Option<String>,
    pub chunk_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

const SELECT: &str = "SELECT d.id, d.workspace_id, d.file_name, d.file_path, d.file_hash,
  d.file_type, d.status, d.parse_error, d.parse_method, d.page_count, d.char_count,
  d.fingerprint_json,
  (SELECT COUNT(*) FROM chunks c WHERE c.document_id = d.id AND c.chunk_level = 'paragraph'),
  d.created_at, d.updated_at FROM documents d";

fn map_row(r: &rusqlite::Row) -> rusqlite::Result<DocumentRow> {
    Ok(DocumentRow {
        id: r.get(0)?,
        workspace_id: r.get(1)?,
        file_name: r.get(2)?,
        file_path: r.get(3)?,
        file_hash: r.get(4)?,
        file_type: r.get(5)?,
        status: r.get(6)?,
        parse_error: r.get(7)?,
        parse_method: r.get(8)?,
        page_count: r.get(9)?,
        char_count: r.get(10)?,
        fingerprint_json: r.get(11)?,
        chunk_count: r.get(12)?,
        created_at: r.get(13)?,
        updated_at: r.get(14)?,
    })
}

pub fn create_parsing(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    file_name: &str,
    file_path: &str,
    file_hash: &str,
    file_type: &str,
    parse_options_hash: &str,
) -> AppResult<DocumentRow> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_iso();
    conn.execute(
        "INSERT INTO documents (id, workspace_id, file_name, file_path, file_hash, file_type, status, parse_options_hash, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'parsing', ?7, ?8, ?8)",
        params![id, workspace_id, file_name, file_path, file_hash, file_type, parse_options_hash, now],
    )?;
    get(conn, &id)
}

pub fn get(conn: &rusqlite::Connection, id: &str) -> AppResult<DocumentRow> {
    conn.query_row(&format!("{SELECT} WHERE d.id = ?1"), [id], map_row)
        .optional()?
        .ok_or_else(|| AppError::not_found("文档"))
}

pub fn list(conn: &rusqlite::Connection, workspace_id: &str) -> AppResult<Vec<DocumentRow>> {
    let mut stmt =
        conn.prepare(&format!("{SELECT} WHERE d.workspace_id = ?1 ORDER BY d.created_at"))?;
    let rows = stmt
        .query_map([workspace_id], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// 同工作区按 hash 找已有文档（避免重复导入同一文件）。
/// 解析失败的行不算「已存在」——否则失败文档会永远挡住同一文件的重试导入。
pub fn find_by_hash(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    file_hash: &str,
) -> AppResult<Option<DocumentRow>> {
    Ok(conn
        .query_row(
            &format!(
                "{SELECT} WHERE d.workspace_id = ?1 AND d.file_hash = ?2 AND d.status != 'failed' LIMIT 1"
            ),
            params![workspace_id, file_hash],
            map_row,
        )
        .optional()?)
}

/// 清理同工作区同 hash 的失败残留行（重试导入成功路径的前置）。
pub fn remove_failed_by_hash(
    conn: &rusqlite::Connection,
    workspace_id: &str,
    file_hash: &str,
) -> AppResult<usize> {
    Ok(conn.execute(
        "DELETE FROM documents WHERE workspace_id = ?1 AND file_hash = ?2 AND status = 'failed'",
        params![workspace_id, file_hash],
    )?)
}

/// 跨工作区找同 hash 且已解析成功的文档（复用其 chunks/特征，跳过解析）。
/// 必须同时匹配解析配置指纹：不同归一/分块配置产出的分块不可互换（V3 前的旧行
/// parse_options_hash 为 NULL，永不匹配 → 重新解析）。
pub fn find_parsed_by_hash(
    conn: &rusqlite::Connection,
    file_hash: &str,
    parse_options_hash: &str,
) -> AppResult<Option<DocumentRow>> {
    Ok(conn
        .query_row(
            &format!(
                "{SELECT} WHERE d.file_hash = ?1 AND d.status = 'parsed' AND d.parse_options_hash = ?2 LIMIT 1"
            ),
            params![file_hash, parse_options_hash],
            map_row,
        )
        .optional()?)
}

pub fn mark_parsed(
    conn: &rusqlite::Connection,
    id: &str,
    parse_method: &str,
    page_count: u32,
    char_count: usize,
    fingerprint_json: &str,
    ocr_layout_json: Option<&str>,
) -> AppResult<()> {
    conn.execute(
        "UPDATE documents SET status = 'parsed', parse_method = ?2, page_count = ?3,
         char_count = ?4, fingerprint_json = ?5, ocr_layout_json = ?6, parse_error = NULL,
         updated_at = ?7 WHERE id = ?1",
        params![
            id,
            parse_method,
            page_count,
            char_count as i64,
            fingerprint_json,
            ocr_layout_json,
            now_iso()
        ],
    )?;
    Ok(())
}

/// 扫描件 OCR 行级版面 JSON（原文版式预览的文本层数据源）；非扫描件为 None。
pub fn get_ocr_layout(conn: &rusqlite::Connection, id: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT ocr_layout_json FROM documents WHERE id = ?1", [id], |r| {
            r.get::<_, Option<String>>(0)
        })
        .optional()?
        .flatten())
}

pub fn mark_failed(conn: &rusqlite::Connection, id: &str, error: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE documents SET status = 'failed', parse_error = ?2, updated_at = ?3 WHERE id = ?1",
        params![id, error, now_iso()],
    )?;
    Ok(())
}

/// 删除文档（chunks/特征/facts 级联清理）。
pub fn remove(conn: &rusqlite::Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM documents WHERE id = ?1", [id])?;
    if n == 0 {
        return Err(AppError::not_found("文档"));
    }
    Ok(())
}
