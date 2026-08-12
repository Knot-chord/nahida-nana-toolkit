use serde::Serialize;

#[derive(Serialize, Default, Clone)]
pub struct GpuInfo {
    /// GPU 名称（如 "NVIDIA GeForce RTX 4090"）
    pub name: String,
    /// VRAM 总量（字节）
    pub vram_total: u64,
    /// VRAM 已用（字节）
    pub vram_used: u64,
    /// VRAM 空闲（字节）
    pub vram_free: u64,
    /// GPU 使用率（0-100）
    pub usage_percent: u32,
    /// GPU 温度（摄氏度），不可用时为 0
    pub temperature: u32,
}

/// 通过注册表获取 GPU 名称（备用方案，适用于非 NVIDIA GPU）
/// 名称在运行期间不变，用 OnceLock 缓存，避免每次轮询都拉起 PowerShell
#[cfg(target_os = "windows")]
fn get_gpu_names_from_registry() -> Vec<String> {
    use std::process::Command;
    use std::sync::OnceLock;
    static CACHE: OnceLock<Vec<String>> = OnceLock::new();

    CACHE.get_or_init(|| {
        let output = Command::new("powershell")
            .args(&[
                "-NoProfile",
                "-Command",
                "Get-CimInstance -ClassName Win32_VideoController | Select-Object -ExpandProperty Name"
            ])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                stdout.lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            Err(_) => vec![],
        }
    }).clone()
}

#[cfg(not(target_os = "windows"))]
fn get_gpu_names_from_registry() -> Vec<String> {
    vec![]
}

/// 通过性能计数器获取 GPU 综合使用率（3D 引擎总和，与任务管理器口径一致，适用于 Intel/AMD）
#[cfg(target_os = "windows")]
fn get_gpu_usage_counter() -> u32 {
    use std::process::Command;
    // 只统计 3D 引擎（engtype_3D）：全引擎平均会被大量空闲的 Copy/Video 引擎拉低到接近 0%
    let output = Command::new("powershell")
        .args(&[
            "-NoProfile",
            "-Command",
            "$s = (Get-Counter '\\GPU Engine(*)\\Utilization Percentage' -SampleInterval 1 -MaxSamples 1).CounterSamples | Where-Object { $_.InstanceName -like '*engtype_3D*' }; \
             $v = ($s | Measure-Object -Property CookedValue -Sum).Sum; \
             [math]::Round([math]::Min($v, 100))"
        ])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout.trim().parse::<u32>().unwrap_or(0).min(100)
        }
        Err(_) => 0,
    }
}

#[cfg(not(target_os = "windows"))]
fn get_gpu_usage_counter() -> u32 {
    0
}

/// 获取 GPU 信息（优先 NVIDIA NVML，失败时回退到注册表获取名称）
#[tauri::command]
pub async fn get_gpu_info() -> Result<Vec<GpuInfo>, String> {
    // 异步命令：回退方案会拉起 PowerShell 采样 1s+，
    // 必须在工作线程执行，否则每 10s 轮询都会冻结 UI 主线程
    tauri::async_runtime::spawn_blocking(collect_gpu_info)
        .await
        .map_err(|e| e.to_string())?
}

fn collect_gpu_info() -> Result<Vec<GpuInfo>, String> {
    // 尝试 NVML（NVIDIA 专用）
    let nvml = match nvml_wrapper::Nvml::init() {
        Ok(n) => n,
        Err(_) => {
            // NVML 失败，回退到注册表获取名称 + 性能计数器获取使用率
            // （WMI 的 AdapterRAM 受 32 位限制无法准确报告 >4GB 显存，故不填 vram_total）
            let names = get_gpu_names_from_registry();
            let usage = get_gpu_usage_counter();
            let gpus: Vec<GpuInfo> = names.into_iter().map(|name| GpuInfo {
                name,
                vram_total: 0,
                vram_used: 0,
                vram_free: 0,
                usage_percent: usage,
                temperature: 0,
            }).collect();
            return Ok(gpus);
        }
    };

    let device_count = nvml
        .device_count()
        .map_err(|e| format!("获取 GPU 数量失败：{e}"))?;

    let mut gpus = Vec::with_capacity(device_count as usize);

    for i in 0..device_count {
        let device = match nvml.device_by_index(i) {
            Ok(d) => d,
            Err(_) => continue,
        };

        let name = device.name().unwrap_or_default();

        let vram_info = match device.memory_info() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let usage = device.utilization_rates().ok();
        let temp = device.temperature(
            nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu,
        ).ok();

        gpus.push(GpuInfo {
            name,
            vram_total: vram_info.total,
            vram_used: vram_info.used,
            vram_free: vram_info.free,
            usage_percent: usage.map(|u| u.gpu).unwrap_or(0),
            temperature: temp.unwrap_or(0),
        });
    }

    Ok(gpus)
}
