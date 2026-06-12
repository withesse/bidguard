// 原本 · 标书查重 —— Tauri 入口与命令注册。
// 业务在 commands/（薄壳）→ services/（编排）→ engine/（算法）+ db/（持久化）。
mod commands;
pub mod config;
pub mod db;
mod engine;
pub mod error;
mod export;
pub mod jobs;
pub mod services;
pub mod state;
#[cfg(test)]
pub(crate) mod test_fixtures;

use tauri::Manager;

/// 解析单个文件，返回页数与字数；用于导入前的早期校验。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DocMeta {
    pages: u32,
    char_count: usize,
}

#[tauri::command]
async fn parse_meta(path: String) -> Result<DocMeta, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let pb = engine::parse::parse_file_blocks(std::path::Path::new(&path), &cancel)?;
        Ok(DocMeta {
            pages: pb.pages,
            char_count: pb.legacy_text.chars().count(),
        })
    })
    .await
    .map_err(|e| format!("解析任务失败：{e}"))?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        // 日志落盘到 app_log_dir（设计文档 §15.2：只记任务 ID/错误码/摘要，永不记录标书正文）
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(log::LevelFilter::Info)
                .targets([
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::LogDir {
                        file_name: Some("bidguard".into()),
                    }),
                    tauri_plugin_log::Target::new(tauri_plugin_log::TargetKind::Stdout),
                ])
                .max_file_size(2_000_000)
                .build(),
        )
        .setup(|app| {
            let base = app.path().app_data_dir()?;
            let pool = db::open(&base)?;
            // 上次运行残留的未完结任务判失败（进程已死，不可能还在跑）。
            // 清理失败只记录，不阻止应用启动。
            match pool.get() {
                Ok(conn) => {
                    if let Err(e) = db::repo::job_repo::mark_stale_as_failed(&conn) {
                        log::error!("启动清理残留任务失败：{e}");
                    }
                }
                Err(e) => log::error!("启动清理取连接失败：{e}"),
            }
            app.manage(state::AppState::new(pool));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            parse_meta,
            commands::workspace::create_workspace,
            commands::workspace::list_workspaces,
            commands::workspace::get_workspace,
            commands::workspace::rename_workspace,
            commands::workspace::set_workspace_settings,
            commands::workspace::delete_workspace,
            commands::document::import_documents,
            commands::document::list_documents,
            commands::document::get_document_preview,
            commands::document::remove_document,
            commands::document::read_document_file,
            commands::document::get_document_ocr_layout,
            commands::annotation::add_annotation,
            commands::annotation::list_annotations,
            commands::annotation::update_annotation,
            commands::annotation::delete_annotation,
            commands::job::get_job,
            commands::job::list_jobs,
            commands::job::cancel_job,
            commands::job::set_job_starred,
            commands::job::delete_job,
            commands::job::cleanup_old_jobs,
            commands::compare::start_compare,
            commands::compare::get_compare_summary,
            commands::compare::list_clusters,
            commands::compare::get_cluster_detail,
            commands::compare::set_cluster_review_status,
            commands::compare::get_pair_detail,
            commands::settings::get_app_settings,
            commands::settings::set_app_settings,
            commands::settings::get_app_info,
            commands::settings::list_source_templates,
            commands::settings::save_source_template,
            commands::settings::delete_source_template,
            commands::export::export_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
