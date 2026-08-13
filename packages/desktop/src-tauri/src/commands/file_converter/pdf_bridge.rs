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

/// 超时时间：5 分钟
const PYTHON_TIMEOUT: Duration = Duration::from_secs(300);

/// 轮询间隔
const POLL_INTERVAL: Duration = Duration::from_millis(100);

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
    cmd.arg(script_path.to_string_lossy().as_ref())
        .arg(actual_op)
        .arg(src.to_string_lossy().as_ref())
        .arg(dst.to_string_lossy().as_ref())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = crate::commands::hide_window(&mut cmd)
        .spawn()
        .map_err(|e| format!("启动 Python 失败: {}", e))?;

    // 超时等待循环（try_wait 轮询）
    let start = std::time::Instant::now();
    let exit_status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() > PYTHON_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait(); // 确保进程完全终止
                    return Err(
                        "PDF 转换超时（超过 5 分钟），请检查文件是否过大或 Python 环境是否正常"
                            .to_string(),
                    );
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
            // 无 OK 前缀时回退到读取目标文件大小
            let metadata = std::fs::metadata(dst)
                .map_err(|e| format!("读取输出文件失败: {}", e))?;
            Ok(metadata.len())
        }
    } else {
        let stderr_msg = String::from_utf8_lossy(&stderr_buf);
        let error_msg = stderr_msg
            .lines()
            .find(|l| l.starts_with("ERROR:"))
            .map(|l| l.strip_prefix("ERROR:").unwrap_or(l).trim().to_string())
            .unwrap_or_else(|| "PDF 转换失败".to_string());
        Err(error_msg)
    }
}

