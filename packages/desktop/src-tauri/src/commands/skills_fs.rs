use tauri::{command, AppHandle};
use tauri_plugin_fs::FsExt;

/// 运行时动态允许访问指定目录（用于自定义 Skills 路径）
#[command]
pub fn allow_custom_skills_dir(app: AppHandle, path: String) -> Result<(), String> {
    let scope = app.fs_scope();
    let _ = scope.allow_directory(std::path::Path::new(&path), true);
    Ok(())
}
