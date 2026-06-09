// 任务持久化：app 数据目录下 tasks/index.json（摘要列表）+ tasks/<id>.json（完整报告）。
use crate::engine::report::Report;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub name: String,
    pub created_at: u128, // 毫秒时间戳
    pub doc_count: usize,
    pub pair_count: usize,
    pub cluster_count: usize,
    pub peak: f32,
    #[serde(default)]
    pub collusion_level: String, // high|medium|low|none（旧任务为空，前端回落用 peak）
    pub matrix: Vec<Vec<f32>>, // 供列表迷你矩阵
}

fn tasks_dir(base: &Path) -> PathBuf {
    base.join("tasks")
}
fn index_path(base: &Path) -> PathBuf {
    tasks_dir(base).join("index.json")
}
fn task_path(base: &Path, id: &str) -> PathBuf {
    tasks_dir(base).join(format!("{id}.json"))
}

fn load_index(base: &Path) -> Vec<TaskSummary> {
    std::fs::read_to_string(index_path(base))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_index(base: &Path, idx: &[TaskSummary]) -> Result<(), String> {
    std::fs::create_dir_all(tasks_dir(base)).map_err(|e| e.to_string())?;
    let json = serde_json::to_string(idx).map_err(|e| e.to_string())?;
    std::fs::write(index_path(base), json).map_err(|e| e.to_string())
}

pub fn save(base: &Path, name: &str, report: &Report) -> Result<String, String> {
    std::fs::create_dir_all(tasks_dir(base)).map_err(|e| e.to_string())?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let id = format!("t{now}");

    let json = serde_json::to_string(report).map_err(|e| e.to_string())?;
    std::fs::write(task_path(base, &id), json).map_err(|e| e.to_string())?;

    let mut idx = load_index(base);
    idx.insert(
        0,
        TaskSummary {
            id: id.clone(),
            name: name.to_string(),
            created_at: now,
            doc_count: report.docs.len(),
            pair_count: report.pairs.len(),
            cluster_count: report.clusters.len(),
            peak: report.peak,
            collusion_level: report.collusion.level.clone(),
            matrix: report.matrix.clone(),
        },
    );
    save_index(base, &idx)?;
    Ok(id)
}

pub fn list(base: &Path) -> Vec<TaskSummary> {
    load_index(base)
}

pub fn get(base: &Path, id: &str) -> Result<Report, String> {
    let data = std::fs::read_to_string(task_path(base, id)).map_err(|_| "任务记录不存在".to_string())?;
    serde_json::from_str(&data).map_err(|e| e.to_string())
}

pub fn delete(base: &Path, id: &str) -> Result<(), String> {
    let _ = std::fs::remove_file(task_path(base, id));
    let idx: Vec<TaskSummary> = load_index(base).into_iter().filter(|t| t.id != id).collect();
    save_index(base, &idx)
}
