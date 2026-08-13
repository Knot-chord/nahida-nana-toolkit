/**
 * Python PDF 桥接
 *
 * 所有涉及 PDF 的转换一律通过此模块调用 Python 脚本。
 * 增强功能：
 * - Python 路径自动检测（python → python3 → py）
 * - 超时控制（5 分钟）
 * - 大文件自动降级（>100MB 切换 _light 操作）
 * - 解析 OK:{size} / ERROR:{msg} 协议
 */

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

/// 基础超时时间：5 分钟（小文件）；大文件按体积动态扩容，见 timeout_for()
const PYTHON_BASE_TIMEOUT: Duration = Duration::from_secs(300);

/// 超时上限：30 分钟（防止异常文件无限挂起）
const PYTHON_MAX_TIMEOUT: Duration = Duration::from_secs(1800);

/// 按文件大小计算超时：基础 5 分钟 + 每 100MB 追加 3 分钟，封顶 30 分钟
/// （百 MB 级 PDF 逐页提取合理耗时远超固定 5 分钟，避免大文件被误杀）
fn timeout_for(file_size: u64) -> Duration {
    let extra_secs = (file_size / (100 * 1024 * 1024)) * 180;
    let total = PYTHON_BASE_TIMEOUT + Duration::from_secs(extra_secs);
    if total > PYTHON_MAX_TIMEOUT { PYTHON_MAX_TIMEOUT } else { total }
}

/// 轮询间隔
const POLL_INTERVAL: Duration = Duration::from_millis(100);

/// 单个 Python 子进程的内存预算下限（MB）：保证小内存机器也能转小文件
const MIN_MEM_BUDGET_MB: u64 = 512;

/// 单个 Python 子进程的内存预算（MB），与“用 3/4 留 1/4”理念一致：
/// 物理内存 × 3/4 ÷ 引擎并发工作线程数（与 conversion_engine 同公式）。
/// 所有 PDF 子进程都跑在引擎工作线程上，并发数不会超过池大小，
/// 因此即使全部 worker 同时在跑 PDF，总占用也不超过物理内存 3/4
pub(super) fn per_process_mem_budget_mb() -> u64 {
    static BUDGET: OnceLock<u64> = OnceLock::new();
    *BUDGET.get_or_init(|| {
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let total_mb = sys.total_memory() / 1024 / 1024;
        if total_mb == 0 {
            return MIN_MEM_BUDGET_MB; // 探测失败时给保守下限，由 Python 侧 psutil 兜底
        }
        let pool = super::super::conversion_engine::compute_pool_size() as u64;
        (total_mb * 3 / 4 / pool).max(MIN_MEM_BUDGET_MB)
    })
}

/// 缓存的 Python 命令名（避免每次 PDF 操作都起子进程探测）
static PYTHON_CMD: OnceLock<Option<&'static str>> = OnceLock::new();

/// 缓存的脚本路径（避免每次跑 current_exe/current_dir syscall）
static SCRIPT_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

/// 尝试查找可用的 Python 命令（结果缓存在 OnceLock 中）
fn find_python() -> Result<&'static str, String> {
    match PYTHON_CMD.get_or_init(|| {
        let candidates = ["python", "python3", "py"];
        for cmd in &candidates {
            let mut probe = Command::new(cmd);
            probe.arg("--version");
            if crate::commands::hide_window(&mut probe)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
            {
                return Some(cmd);
            }
        }
        None
    }) {
        Some(cmd) => Ok(cmd),
        None => Err("未找到 Python 运行时，请安装 Python 并确保其已添加到 PATH 环境变量中".to_string()),
    }
}

/// 查找 pdf_handler.py 脚本路径（结果缓存在 OnceLock 中）
fn find_script_path() -> Result<&'static Path, String> {
    match SCRIPT_PATH.get_or_init(|| {
        let dev_base = option_env!("CARGO_MANIFEST_DIR").map(PathBuf::from);
        let exe_base = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(PathBuf::from));
        let cwd_base = std::env::current_dir().ok();

        [dev_base, exe_base, cwd_base]
            .into_iter()
            .flatten()
            .map(|b| b.join("resources").join("scripts").join("pdf_handler.py"))
            .find(|p| p.exists())
    }) {
        Some(path) => Ok(path.as_path()),
        None => Err("找不到 pdf_handler.py 脚本，请确认 Python 脚本已正确部署".to_string()),
    }
}

/// 调用 Python PDF 处理脚本（含超时控制 + Python 路径自动检测）
pub fn call_pdf_python(operation: &str, src: &Path, dst: &Path) -> Result<u64, String> {
    // 缓存的脚本路径 + Python 命令（首次调用后零开销）
    let script_path = find_script_path()?;
    let python_cmd = find_python()?;

    // 大文件降级：>100MB 自动切换为仅文本操作
    let file_size = std::fs::metadata(src)
        .map(|m| m.len())
        .unwrap_or(0);
    let actual_op = if file_size > 100 * 1024 * 1024 {
        match operation {
            "pdf_to_html" => "pdf_to_html_light",
            "pdf_to_md" => "pdf_to_md_light",
            _ => operation,
        }
    } else {
        operation
    };

    // 启动子进程（隐藏控制台窗口，避免转换时弹黑窗）
    let mut cmd = Command::new(python_cmd);
    // 强制 UTF-8 输出：Windows 中文系统默认 ANSI 代码页（GBK），
    // 本侧按 UTF-8 解码 stdout/stderr，不强制则错误信息必然乱码
    cmd.env("PYTHONIOENCODING", "utf-8");
    // 下发每进程内存预算（按本机物理内存与并发数自适应，见 per_process_mem_budget_mb）
    cmd.env("CONVERT_MEM_LIMIT_MB", per_process_mem_budget_mb().to_string());
    cmd.arg(script_path.to_string_lossy().as_ref())
        .arg(actual_op)
        .arg(src.to_string_lossy().as_ref())
        .arg(dst.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = crate::commands::hide_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("启动 Python 失败: {}", e))?;

    // 超时等待循环（try_wait 轮询；时长按文件体积动态扩容）
    let timeout = timeout_for(file_size);
    let start = std::time::Instant::now();
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait(); // 确保进程完全终止
                    return Err(format!(
                        "PDF 转换超时（超过 {} 分钟），请检查文件是否过大或 Python 环境是否正常",
                        timeout.as_secs() / 60
                    ));
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => return Err(format!("等待 Python 子进程失败: {}", e)),
        }
    };

    // 读取输出管道中的剩余数据
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    if let Some(ref mut s) = child.stdout {
        let _ = s.read_to_end(&mut stdout_buf);
    }
    if let Some(ref mut s) = child.stderr {
        let _ = s.read_to_end(&mut stderr_buf);
    }

    // 解析 OK:{size} / ERROR:{msg} 协议
    if exit_status.success() {
        let stdout = String::from_utf8_lossy(&stdout_buf);
        if let Some(size_str) = stdout.trim().strip_prefix("OK:") {
            let size: u64 = size_str.trim().parse().unwrap_or(0);
            Ok(size)
        } else {
            // 无 OK 前缀时回退到读取目标文件大小（0 字节同样判为失败，不假成功）
            let metadata = std::fs::metadata(dst)
                .map_err(|e| format!("读取输出文件失败: {}", e))?;
            if metadata.len() == 0 {
                return Err("转换结果为空：源文件可能没有可提取的文本内容（如扫描版/纯图片 PDF）".to_string());
            }
            Ok(metadata.len())
        }
    } else {
        let stderr_msg = String::from_utf8_lossy(&stderr_buf);
        // 优先取 ERROR: 协议行；否则取 stderr 末尾实质行（如 Python 异常堆栈）；
        // 再否则报退出码，绝不给无信息的“转换失败”
        let error_msg = stderr_msg
            .lines()
            .find(|l| l.starts_with("ERROR:"))
            .map(|l| l.strip_prefix("ERROR:").unwrap_or(l).trim().to_string())
            .or_else(|| {
                stderr_msg
                    .lines()
                    .rev()
                    .map(|l| l.trim())
                    .find(|l| !l.is_empty())
                    .map(|l| l.to_string())
            })
            .unwrap_or_else(|| {
                format!(
                    "PDF 转换异常退出（退出码 {:?}，无错误输出）",
                    exit_status.code()
                )
            });
        Err(error_msg)
    }
}

