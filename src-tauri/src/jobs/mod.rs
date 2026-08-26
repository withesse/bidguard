// 任务系统：状态机 pending→running→{completed|failed|cancelling→cancelled}，
// 协作式取消（AtomicBool），进度节流（事件与 DB 共用一套阈值）。
// execute() 是同步核心（可测试），spawn() 仅是 spawn_blocking 薄壳。
pub mod progress;

use crate::db::repo::job_repo::{self, JobRow};
use crate::db::DbPool;
use crate::error::{AppError, AppErrorCode, AppResult};
use progress::{JobProgress, JobTerminal, ProgressSink};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// 进度节流：距上次 ≥100ms 或百分比变化 ≥1%，stage 变化与首末帧必发。
const THROTTLE_MS: u128 = 100;
const THROTTLE_PCT: f32 = 0.01;

#[derive(Clone, Default)]
pub struct JobManager {
    running: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    /// 进程级 DB 写串行锁：所有写事务（导入 persist / 比对 persist）共用一把，
    /// 避免跨任务/跨工作区并发写在 SQLite 单写者下互撞 SQLITE_BUSY 造成任务假失败。
    db_write: Arc<Mutex<()>>,
}

/// 交给任务体的执行上下文：取消检查 + 进度上报（含落库与事件）。
pub struct JobCtx {
    pub job_id: String,
    pub job_type: String,
    pub db: DbPool,
    cancel: Arc<AtomicBool>,
    sink: Arc<dyn ProgressSink>,
    last_emit: Mutex<(Instant, f32, String)>, // (时间, 百分比, stage)
    db_write: Arc<Mutex<()>>,                 // 进程级 DB 写串行锁（见 JobManager::db_write）
}

impl JobCtx {
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// 旗标引用：交给需要在内部循环里自查取消的引擎代码（如 OCR 逐页）。
    pub fn cancel_flag(&self) -> &AtomicBool {
        &self.cancel
    }

    /// 进程级 DB 写串行锁：所有写事务（导入 persist / 比对 persist）持此锁串行，
    /// 避免 SQLite 单写者下并发写互撞 SQLITE_BUSY。毒化后恢复，避免一次 panic 后永久卡死。
    pub fn write_lock(&self) -> std::sync::MutexGuard<'_, ()> {
        self.db_write.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 测试用构造：绕过 JobManager 直接组装上下文。
    #[cfg(test)]
    pub fn for_test(
        job_id: String,
        job_type: String,
        db: DbPool,
        cancel: Arc<AtomicBool>,
        sink: Arc<dyn ProgressSink>,
    ) -> Self {
        Self {
            job_id,
            job_type,
            db,
            cancel,
            sink,
            last_emit: Mutex::new((Instant::now(), -1.0, String::new())),
            db_write: Arc::new(Mutex::new(())),
        }
    }

    /// 在循环/阶段边界调用；已请求取消则返回 JobCancelled 让任务体即刻收尾。
    pub fn check(&self) -> AppResult<()> {
        if self.cancelled() {
            return Err(AppError::new(AppErrorCode::JobCancelled, "任务已取消"));
        }
        Ok(())
    }

    /// 上报进度：节流后写 jobs 表并广播事件。
    pub fn progress(&self, stage: &str, current: usize, total: usize, message: impl Into<String>) {
        let percent = if total == 0 {
            0.0
        } else {
            (current as f32 / total as f32).clamp(0.0, 1.0)
        };
        {
            // 毒化恢复：节流状态即使被 panic 线程污染也只是时间戳/百分比，取内值继续
            let mut last = self.last_emit.lock().unwrap_or_else(|e| e.into_inner());
            let stage_changed = last.2 != stage;
            let edge = current == 0 || current >= total;
            let due = last.0.elapsed().as_millis() >= THROTTLE_MS
                || (percent - last.1).abs() >= THROTTLE_PCT;
            if !(stage_changed || edge || due) {
                return;
            }
            *last = (Instant::now(), percent, stage.to_string());
        }
        let message = message.into();
        // 进度落库是尽力而为：短超时拿连接，拿不到（池忙）就只发事件，绝不阻塞任务体
        if let Ok(conn) = self.db.get_timeout(std::time::Duration::from_millis(200)) {
            let _ = job_repo::set_progress(&conn, &self.job_id, percent as f64, &message);
        }
        self.sink.emit_progress(&JobProgress {
            job_id: self.job_id.clone(),
            job_type: self.job_type.clone(),
            stage: stage.to_string(),
            message,
            current,
            total,
            percent,
        });
    }
}

/// 同步执行核心：状态跃迁落库 + 终态事件。worker 返回 JobCancelled 视为取消而非失败。
pub fn execute<F>(
    db: DbPool,
    sink: Arc<dyn ProgressSink>,
    cancel: Arc<AtomicBool>,
    job: &JobRow,
    db_write: Arc<Mutex<()>>,
    worker: F,
) where
    F: FnOnce(&JobCtx) -> AppResult<()>,
{
    let mark = |status: &str, code: Option<&str>, msg: Option<&str>| {
        // 终态落库失败不能静默：DB 与已广播的终态事件会永久不一致（前端显示完成但表停在
        // running，重启后被误判为「中断」）。至少记日志（仅 job_id + 错误码，不含正文）。
        match db.get() {
            Ok(conn) => {
                if let Err(e) = job_repo::finish(&conn, &job.id, status, code, msg) {
                    log::error!("任务终态落库失败 job_id={} status={status} code={:?}", job.id, e.code);
                }
            }
            Err(e) => log::error!("任务终态落库取连接失败 job_id={} status={status}: {e}", job.id),
        }
        sink.emit_terminal(&JobTerminal {
            job_id: job.id.clone(),
            job_type: job.job_type.clone(),
            status: status.to_string(),
            error_code: code.map(str::to_string),
            error_message: msg.map(str::to_string),
        });
    };

    // 取连接失败必须显式处理：否则会跳过 set_running 让任务带 pending 状态照常运行。
    match db.get() {
        Ok(conn) => match job_repo::set_running(&conn, &job.id) {
            Ok(true) => {} // 正常进入 running
            // pending→running 未命中：竞态窗口内任务已被取消/终态，直接收尾为 cancelled 不再执行
            Ok(false) => {
                mark("cancelled", None, None);
                return;
            }
            Err(e) => {
                log::error!("set_running 失败 job_id={} code={:?}", job.id, e.code);
                mark("failed", Some("databaseError"), Some("任务启动失败"));
                return;
            }
        },
        Err(e) => {
            log::error!("任务启动取连接失败 job_id={}: {e}", job.id);
            mark("failed", Some("databaseError"), Some("任务启动失败"));
            return;
        }
    }

    let ctx = JobCtx {
        job_id: job.id.clone(),
        job_type: job.job_type.clone(),
        db: db.clone(),
        cancel: cancel.clone(),
        sink: sink.clone(),
        last_emit: Mutex::new((Instant::now(), -1.0, String::new())),
        db_write,
    };

    // worker panic 不能拖垮线程池，也不能让任务永远停在 running
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker(&ctx)));
    match result {
        // 返回 Ok 说明工作确实做完了：即使取消旗标已置位也如实记 completed，
        // 想响应取消的任务体应在检查点返回 JobCancelled
        Ok(Ok(())) => mark("completed", None, None),
        Ok(Err(e)) if e.code == AppErrorCode::JobCancelled => mark("cancelled", None, None),
        Ok(Err(e)) => {
            let code = serde_json::to_value(e.code)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string));
            mark("failed", code.as_deref(), Some(&e.message));
        }
        Err(_) => mark("failed", Some("unknown"), Some("任务内部异常终止")),
    }
}

impl JobManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 创建任务行并在阻塞线程池异步执行；立即返回 pending 状态的 JobRow。
    /// 同工作区已有同类型未完结任务时拒绝（JobConflict）。
    #[allow(clippy::too_many_arguments)] // 任务派发的固有参数（库/事件/范围/类型/名/配置/任务体），拆结构体无收益
    pub fn spawn<F>(
        &self,
        db: &DbPool,
        sink: Arc<dyn ProgressSink>,
        workspace_id: &str,
        job_type: &str,
        name: Option<&str>,
        config_json: &str,
        worker: F,
    ) -> AppResult<JobRow>
    where
        F: FnOnce(&JobCtx) -> AppResult<()> + Send + 'static,
    {
        let job = {
            // IMMEDIATE 事务：并发 spawn 时序列化「查活跃 + 建任务」，杜绝双击建出两个并行任务。
            let mut conn = self.db_get(db)?;
            let tx = conn
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
                .map_err(AppError::from)?;
            if job_repo::has_active(&tx, workspace_id, job_type)? {
                return Err(AppError::new(
                    AppErrorCode::JobConflict,
                    "该工作区已有同类任务在运行，请等待完成或先取消",
                ));
            }
            let job = job_repo::create(&tx, workspace_id, job_type, name, config_json)?;
            tx.commit().map_err(AppError::from)?;
            job
        };

        let cancel = Arc::new(AtomicBool::new(false));
        self.running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(job.id.clone(), cancel.clone());

        let running = self.running.clone();
        let db = db.clone();
        let db_write = self.db_write.clone();
        let job_for_worker = job.clone();
        tauri::async_runtime::spawn_blocking(move || {
            // RAII 守卫：即使 execute 内部 panic，运行表也不残留 zombie 条目
            struct RunningGuard {
                map: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
                id: String,
            }
            impl Drop for RunningGuard {
                fn drop(&mut self) {
                    // 本 Drop 常在 panic 展开路径执行：此处再 unwrap 毒化锁会二次 panic → abort，
                    // 运行表反而永久残留 zombie。恢复内值照常摘除。
                    self.map.lock().unwrap_or_else(|e| e.into_inner()).remove(&self.id);
                }
            }
            let _guard = RunningGuard {
                map: running,
                id: job_for_worker.id.clone(),
            };
            execute(db, sink, cancel, &job_for_worker, db_write, worker);
        });
        Ok(job)
    }

    /// 请求取消：置 cancelling + 翻转取消旗标；任务体在下一个检查点收尾。
    /// 已完结任务幂等返回 Ok；不在运行表中的「孤儿 running」直接判 cancelled。
    pub fn cancel(&self, db: &DbPool, job_id: &str) -> AppResult<()> {
        let conn = self.db_get(db)?;
        let job = job_repo::get(&conn, job_id)?;
        match job.status.as_str() {
            "completed" | "failed" | "cancelled" => return Ok(()),
            _ => {}
        }
        let flag = self.running.lock().unwrap_or_else(|e| e.into_inner()).get(job_id).cloned();
        match flag {
            Some(f) => {
                f.store(true, Ordering::SeqCst);
                job_repo::set_cancelling(&conn, job_id)?;
            }
            None => {
                // 进程内没有对应线程（理论上只在启动清理竞态出现），直接判终态
                job_repo::finish(&conn, job_id, "cancelled", None, None)?;
            }
        }
        Ok(())
    }

    fn db_get(&self, db: &DbPool) -> AppResult<crate::db::DbConn> {
        db.get().map_err(AppError::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::db::repo::workspace_repo;
    use progress::CollectSink;

    fn setup() -> (DbPool, String) {
        let pool = open_in_memory().unwrap();
        let ws = {
            let conn = pool.get().unwrap();
            workspace_repo::create(&conn, "w").unwrap()
        };
        (pool, ws.id)
    }

    #[test]
    fn execute_completes_and_reports() {
        let (pool, ws) = setup();
        let job = {
            let conn = pool.get().unwrap();
            job_repo::create(&conn, &ws, "import", None, "{}").unwrap()
        };
        let sink = Arc::new(CollectSink::default());
        let cancel = Arc::new(AtomicBool::new(false));
        execute(pool.clone(), sink.clone(), cancel, &job, Arc::new(Mutex::new(())), |ctx| {
            ctx.progress("parse", 0, 3, "开始");
            ctx.progress("parse", 3, 3, "完成");
            Ok(())
        });
        let conn = pool.get().unwrap();
        let j = job_repo::get(&conn, &job.id).unwrap();
        assert_eq!(j.status, "completed");
        assert!((j.progress - 1.0).abs() < 1e-9);
        let terms = sink.terminal.lock().unwrap();
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].status, "completed");
        assert!(sink.progress.lock().unwrap().len() >= 2, "首末帧必发");
    }

    #[test]
    fn execute_maps_cancel_and_failure() {
        let (pool, ws) = setup();
        let sink = Arc::new(CollectSink::default());

        // 取消：worker 在检查点发现旗标
        let job = {
            let conn = pool.get().unwrap();
            job_repo::create(&conn, &ws, "import", None, "{}").unwrap()
        };
        let cancel = Arc::new(AtomicBool::new(true));
        execute(pool.clone(), sink.clone(), cancel, &job, Arc::new(Mutex::new(())), |ctx| {
            ctx.check()?;
            unreachable!("已取消不应继续执行");
        });
        let conn = pool.get().unwrap();
        assert_eq!(job_repo::get(&conn, &job.id).unwrap().status, "cancelled");

        // 失败：错误码与信息落库
        let job2 = job_repo::create(&conn, &ws, "import", None, "{}").unwrap();
        drop(conn);
        execute(
            pool.clone(),
            sink.clone(),
            Arc::new(AtomicBool::new(false)),
            &job2,
            Arc::new(Mutex::new(())),
            |_| Err(AppError::new(AppErrorCode::ParseFailed, "解析失败")),
        );
        let conn = pool.get().unwrap();
        let j2 = job_repo::get(&conn, &job2.id).unwrap();
        assert_eq!(j2.status, "failed");
        assert_eq!(j2.error_code.as_deref(), Some("parseFailed"));
        assert_eq!(j2.error_message.as_deref(), Some("解析失败"));
    }

    #[test]
    fn cancel_is_idempotent_on_finished_jobs() {
        let (pool, ws) = setup();
        let mgr = JobManager::new();
        let conn = pool.get().unwrap();
        let job = job_repo::create(&conn, &ws, "import", None, "{}").unwrap();
        job_repo::finish(&conn, &job.id, "completed", None, None).unwrap();
        drop(conn);
        mgr.cancel(&pool, &job.id).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(job_repo::get(&conn, &job.id).unwrap().status, "completed");
    }

    #[test]
    fn cancel_orphan_running_job_marks_cancelled() {
        let (pool, ws) = setup();
        let mgr = JobManager::new();
        let conn = pool.get().unwrap();
        let job = job_repo::create(&conn, &ws, "import", None, "{}").unwrap();
        job_repo::set_running(&conn, &job.id).unwrap();
        drop(conn);
        mgr.cancel(&pool, &job.id).unwrap();
        let conn = pool.get().unwrap();
        assert_eq!(job_repo::get(&conn, &job.id).unwrap().status, "cancelled");
    }
}
