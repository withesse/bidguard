// 服务层：编排任务管线（仓储 + 引擎 + 任务上下文），不直接依赖 Tauri 句柄，便于测试。
pub mod compare_service;
pub mod export_service;
pub mod import_service;
