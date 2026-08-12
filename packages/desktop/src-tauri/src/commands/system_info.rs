use serde::Serialize;
use std::sync::{Mutex, OnceLock};
use sysinfo::System;

fn get_system() -> &'static Mutex<System> {
    static SYS: OnceLock<Mutex<System>> = OnceLock::new();
    SYS.get_or_init(|| Mutex::new(System::new()))
}

#[derive(Serialize)]
pub struct SystemInfo {
    pub os_name: String,
    pub os_version: String,
    pub kernel_version: String,
    pub hostname: String,
    pub cpu_name: String,
    pub cpu_cores: usize,
    pub cpu_usage: f32,
    pub memory_total: u64,
    pub memory_used: u64,
    pub memory_available: u64,
}

#[tauri::command]
pub async fn get_system_info() -> Result<SystemInfo, String> {
    // 异步命令：在工作线程执行，避免阻塞 UI 主线程导致界面卡顿
    tauri::async_runtime::spawn_blocking(collect_system_info)
        .await
        .map_err(|e| e.to_string())?
}

fn collect_system_info() -> Result<SystemInfo, String> {
    let sys_lock = get_system();
    let mut sys = sys_lock.lock().map_err(|e| e.to_string())?;
    // 只刷新 CPU 与内存（refresh_all 会枚举全部进程，首次调用耗时数秒，导致“梦境编织”加载慢）
    sys.refresh_cpu_usage();
    sys.refresh_memory();

    let cpu_cores = sys.cpus().len();
    // 手动计算所有核心的平均 CPU 使用率（Windows 上 global_cpu_usage() 可能不准确）
    let cpu_usage = if cpu_cores > 0 {
        sys.cpus().iter().map(|c| c.cpu_usage()).sum::<f32>() / cpu_cores as f32
    } else {
        0.0
    };
    // 取第一个核心的品牌名作为 CPU 型号（如 "Intel Core i7-12700H"）
    let cpu_name = sys
        .cpus()
        .first()
        .map(|c| c.brand().to_string())
        .unwrap_or_default();

    Ok(SystemInfo {
        os_name: System::name().unwrap_or_default(),
        os_version: System::os_version().unwrap_or_default(),
        kernel_version: System::kernel_version().unwrap_or_default(),
        hostname: System::host_name().unwrap_or_default(),
        cpu_name,
        cpu_cores,
        cpu_usage,
        memory_total: sys.total_memory(),
        memory_used: sys.used_memory(),
        memory_available: sys.available_memory(),
    })
}
