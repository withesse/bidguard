// 全局共享状态（tauri .manage()，commands 经 State<AppState> 访问）。
use crate::db::DbPool;
use crate::jobs::JobManager;
use fastembed::TextEmbedding;
use jieba_rs::Jieba;
use std::sync::{Arc, Mutex, OnceLock};

pub struct AppState {
    pub db: DbPool,
    pub jobs: JobManager,
    jieba: OnceLock<Arc<Jieba>>,
    /// 语义模型槽位：首次启用语义比对时加载并常驻；Mutex 同时充当推理互斥
    embedder: Arc<Mutex<Option<TextEmbedding>>>,
}

impl AppState {
    pub fn new(db: DbPool) -> Self {
        Self {
            db,
            jobs: JobManager::new(),
            jieba: OnceLock::new(),
            embedder: Arc::new(Mutex::new(None)),
        }
    }

    /// 惰性构建并常驻的 Jieba 实例（词典加载需数百毫秒，避免每次任务重建）。
    /// Arc 便于交给后台任务线程持有。
    pub fn jieba(&self) -> Arc<Jieba> {
        self.jieba.get_or_init(|| Arc::new(Jieba::new())).clone()
    }

    pub fn embedder(&self) -> Arc<Mutex<Option<TextEmbedding>>> {
        self.embedder.clone()
    }
}
