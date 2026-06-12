// 任务进度事件：ProgressSink 抽象出事件后端（Tauri emit / 测试收集 / 静默），
// 节流统一在 JobCtx 做（事件与 DB 进度共用一套节流，避免事件风暴与写放大）。
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

/// 进度 payload（camelCase 直达前端）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobProgress {
    pub job_id: String,
    pub job_type: String, // import | compare | export
    pub stage: String,
    pub message: String,
    pub current: usize,
    pub total: usize,
    pub percent: f32,
}

/// 终态 payload。status: completed | failed | cancelled
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobTerminal {
    pub job_id: String,
    pub job_type: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// 事件名前缀按任务类型映射（Tauri v2 事件名不允许 `.`，统一用 `:`）。
pub fn event_prefix(job_type: &str) -> &'static str {
    match job_type {
        "import" => "document:import",
        "compare" => "compare",
        "export" => "export",
        _ => "job",
    }
}

pub trait ProgressSink: Send + Sync {
    fn emit_progress(&self, p: &JobProgress);
    fn emit_terminal(&self, t: &JobTerminal);
}

/// 生产实现：通过 Tauri 全局事件广播，任何路由/窗口都能订阅。
pub struct TauriEventSink {
    app: tauri::AppHandle,
}

impl TauriEventSink {
    pub fn new(app: tauri::AppHandle) -> Self {
        Self { app }
    }
}

impl ProgressSink for TauriEventSink {
    fn emit_progress(&self, p: &JobProgress) {
        use tauri::Emitter;
        let name = format!("{}:progress", event_prefix(&p.job_type));
        if let Err(e) = self.app.emit(&name, p) {
            log::warn!("事件发送失败 {name}: {e}");
        }
    }

    fn emit_terminal(&self, t: &JobTerminal) {
        use tauri::Emitter;
        let name = format!("{}:{}", event_prefix(&t.job_type), t.status);
        if let Err(e) = self.app.emit(&name, t) {
            log::warn!("事件发送失败 {name}: {e}");
        }
    }
}

/// 测试实现：什么都不发。
pub struct NoopSink;

impl ProgressSink for NoopSink {
    fn emit_progress(&self, _: &JobProgress) {}
    fn emit_terminal(&self, _: &JobTerminal) {}
}

/// 测试实现：收集全部事件供断言。
#[derive(Default)]
pub struct CollectSink {
    pub progress: Mutex<Vec<JobProgress>>,
    pub terminal: Mutex<Vec<JobTerminal>>,
}

impl ProgressSink for CollectSink {
    fn emit_progress(&self, p: &JobProgress) {
        self.progress.lock().unwrap().push(p.clone());
    }
    fn emit_terminal(&self, t: &JobTerminal) {
        self.terminal.lock().unwrap().push(t.clone());
    }
}
