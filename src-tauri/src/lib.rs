// 原本 · 标书查重 —— Tauri 入口与命令
mod engine;
mod export;
mod store;

use engine::report::Report;
use store::TaskSummary;
use tauri::Manager;

fn data_base(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("你好，{name}！来自「原本」引擎。")
}

/// 对 2-5 份标书做交叉比对，返回相似度矩阵 + 段落对齐 + 雷同条款聚合 + 元数据指纹。
/// `on_progress` 通过 Tauri Channel 实时回传检测进度。解析在本地完成，不上传任何文件。
#[tauri::command]
fn analyze_paths(
    paths: Vec<String>,
    templates: Vec<String>,
    semantic: bool,
    threshold: f32,
    scope: String,
    on_progress: tauri::ipc::Channel<engine::report::Progress>,
) -> Result<Report, String> {
    engine::analyze(paths, templates, semantic, threshold, scope, &move |p| {
        let _ = on_progress.send(p);
    })
}

/// 保存一次查重结果为历史任务，返回任务 id。
#[tauri::command]
fn save_task(app: tauri::AppHandle, name: String, report: Report) -> Result<String, String> {
    store::save(&data_base(&app)?, &name, &report)
}

/// 列出全部历史任务摘要（按时间倒序）。
#[tauri::command]
fn list_tasks(app: tauri::AppHandle) -> Result<Vec<TaskSummary>, String> {
    Ok(store::list(&data_base(&app)?))
}

/// 读取某历史任务的完整报告。
#[tauri::command]
fn get_task(app: tauri::AppHandle, id: String) -> Result<Report, String> {
    store::get(&data_base(&app)?, &id)
}

/// 删除某历史任务。
#[tauri::command]
fn delete_task(app: tauri::AppHandle, id: String) -> Result<(), String> {
    store::delete(&data_base(&app)?, &id)
}

/// 导出 Excel 报告到指定路径。
#[tauri::command]
fn export_excel(report: Report, path: String) -> Result<(), String> {
    export::to_xlsx(&report, &path)
}

/// 导出 Word(.docx) 报告。
#[tauri::command]
fn export_docx(report: Report, path: String) -> Result<(), String> {
    export::to_docx(&report, &path)
}

/// 导出 HTML 报告（可在浏览器打印为 PDF）。
#[tauri::command]
fn export_html(report: Report, path: String) -> Result<(), String> {
    export::to_html(&report, &path)
}

/// 解析单个文件，返回页数与字数；用于候选槽位的即时状态与早期校验。
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DocMeta {
    pages: u32,
    char_count: usize,
}

#[tauri::command]
fn parse_meta(path: String) -> Result<DocMeta, String> {
    let pd = engine::parse::parse_file(std::path::Path::new(&path))?;
    Ok(DocMeta {
        pages: pd.pages,
        char_count: pd.text.chars().count(),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            greet,
            analyze_paths,
            save_task,
            list_tasks,
            get_task,
            delete_task,
            export_excel,
            export_docx,
            export_html,
            parse_meta
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
