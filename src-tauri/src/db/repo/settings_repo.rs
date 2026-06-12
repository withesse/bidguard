// 用户全局设置仓储：app_settings 键值表，value 为 JSON 文本。
use crate::db::now_iso;
use crate::error::{AppError, AppErrorCode, AppResult};
use rusqlite::{params, OptionalExtension};
use serde_json::Value;

pub fn get(conn: &rusqlite::Connection, key: &str) -> AppResult<Option<Value>> {
    let raw: Option<String> = conn
        .query_row("SELECT value_json FROM app_settings WHERE key = ?1", [key], |r| r.get(0))
        .optional()?;
    match raw {
        None => Ok(None),
        Some(s) => serde_json::from_str(&s)
            .map(Some)
            .map_err(|e| {
                AppError::new(AppErrorCode::DatabaseError, "设置数据损坏").with_detail(e.to_string())
            }),
    }
}

pub fn set(conn: &rusqlite::Connection, key: &str, value: &Value) -> AppResult<()> {
    conn.execute(
        "INSERT INTO app_settings (key, value_json, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
        params![key, value.to_string(), now_iso()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;

    #[test]
    fn set_get_overwrite() {
        let pool = open_in_memory().unwrap();
        let conn = pool.get().unwrap();
        assert!(get(&conn, "config").unwrap().is_none());

        set(&conn, "config", &serde_json::json!({ "compare": { "scope": "tech" } })).unwrap();
        let v = get(&conn, "config").unwrap().unwrap();
        assert_eq!(v["compare"]["scope"], "tech");

        set(&conn, "config", &serde_json::json!({ "compare": { "scope": "full" } })).unwrap();
        let v = get(&conn, "config").unwrap().unwrap();
        assert_eq!(v["compare"]["scope"], "full");
    }
}
