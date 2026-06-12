// SQLite 连接池：WAL + 外键 + busy_timeout；打开时自动跑迁移。
pub mod migrations;
pub mod repo;

use crate::error::{AppError, AppErrorCode, AppResult};
use r2d2_sqlite::SqliteConnectionManager;
use std::path::Path;
use std::time::Duration;

pub type DbPool = r2d2::Pool<SqliteConnectionManager>;
pub type DbConn = r2d2::PooledConnection<SqliteConnectionManager>;

/// 统一时间戳格式（ISO8601 UTC，毫秒精度），全部表的 *_at 字段使用。
pub fn now_iso() -> String {
    chrono::Utc::now()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn init_conn(conn: &mut rusqlite::Connection) -> Result<(), rusqlite::Error> {
    // journal_mode 会返回结果行，必须用查询方式设置（内存库返回 "memory" 亦属正常）
    let _mode: String = conn.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    conn.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=NORMAL;")?;
    conn.busy_timeout(Duration::from_millis(5000))
}

/// 打开（或创建）`<base>/bidguard.db` 并迁移到最新 schema。`base` 为 app 数据目录。
pub fn open(base: &Path) -> AppResult<DbPool> {
    std::fs::create_dir_all(base)?;
    let manager = SqliteConnectionManager::file(base.join("bidguard.db")).with_init(init_conn);
    build_pool(manager, 8)
}

/// 测试用内存库。max_size 必须为 1：内存库按连接隔离，多连接会各见各的空库。
pub fn open_in_memory() -> AppResult<DbPool> {
    let manager = SqliteConnectionManager::memory().with_init(init_conn);
    build_pool(manager, 1)
}

fn build_pool(manager: SqliteConnectionManager, max_size: u32) -> AppResult<DbPool> {
    let pool = r2d2::Pool::builder()
        .max_size(max_size)
        .build(manager)
        .map_err(|e| {
            AppError::new(AppErrorCode::DatabaseError, "数据库初始化失败").with_detail(e.to_string())
        })?;
    let mut conn = pool.get()?;
    migrations::run(&mut conn)?;
    drop(conn);
    Ok(pool)
}
