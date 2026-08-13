pub mod file_converter;
pub mod conversion_engine;
pub mod system_info;
pub mod gpu_info;
pub mod open_folder;
pub mod skills_fs;

/// 隐藏子进程控制台窗口（Windows）：拉起 powershell/python 等控制台进程时，
/// 不加 CREATE_NO_WINDOW 每次都会弹黑窗，周期轮询（如 GPU 采集）下尤其明显
pub(crate) fn hide_window(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        return cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        cmd
    }
}
