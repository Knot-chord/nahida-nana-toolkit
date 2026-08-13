mod commands;
use commands::file_converter;
use commands::system_info;
use commands::gpu_info;
use commands::open_folder;
use commands::skills_fs;
// Manager/FsExt 仅开发模式授权 skills 目录时使用
#[cfg(debug_assertions)]
use tauri::Manager;
#[cfg(debug_assertions)]
use tauri_plugin_fs::FsExt;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|_app| {
            // 开发模式：运行时允许访问项目根下的便携 skills 目录
            #[cfg(debug_assertions)]
            {
                let app = _app;
                let scope = app.fs_scope();
                if let Ok(res_dir) = app.path().resource_dir() {
                    if let Some(proj_root) = res_dir.parent() {
                        let skills_dir = proj_root.join("skills");
                        let _ = scope.allow_directory(skills_dir, true);
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            file_converter::convert_file,
            file_converter::convert_content,
            file_converter::get_file_size,
            file_converter::get_files_info,
            file_converter::collect_supported_files,
            file_converter::save_dropped_file,
            file_converter::extract_office_text,
            file_converter::extract_office_text_by_path,
            file_converter::extract_document_full,
            file_converter::extract_document_full_by_path,
            system_info::get_system_info,
            gpu_info::get_gpu_info,
            open_folder::open_folder,
            skills_fs::allow_custom_skills_dir,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
