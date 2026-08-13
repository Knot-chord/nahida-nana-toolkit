/**
 * 专用转换引擎（线程池版）
 *
 * 所有文件格式转换操作都通过此模块调度执行。
 * 核心设计：
 * - 线程池：根据 CPU 核心数自动创建工作线程，充分利用多核性能
 * - 每个工作线程拥有 16MB 栈，彻底避免 Tauri 默认 1MB 线程栈溢出
 * - 通道（channel）调度，非阻塞，Tauri 命令线程不被占用
 * - 单任务崩溃隔离：catch_unwind 包裹每个任务，panic 只影响当前任务
 * - 双重恢复：worker 线程内 panic → 继续工作；worker 意外死亡 → 单独重建
 * - 负载均衡：多消费者竞争同一通道，天然均匀分配
 */

use std::sync::mpsc;
use std::sync::{Arc, Mutex};

/// 引擎内部状态
struct EngineInner {
    sender: mpsc::Sender<Job>,
    receiver: Arc<Mutex<mpsc::Receiver<Job>>>,
    workers: Vec<WorkerEntry>,
}

/// 一个工作线程
struct WorkerEntry {
    id: usize,
    handle: std::thread::JoinHandle<()>,
}

/// 一个调度任务
type Job = Box<dyn FnOnce() + Send + 'static>;

/// 全局转换引擎实例
static ENGINE: Mutex<Option<EngineInner>> = Mutex::new(None);

/// 每个工作线程的栈大小：16MB（Tauri 默认仅 1MB）
const WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

/// 计算线程池大小：CPU 核心数的 3/4，限制在 [2, 16]
/// 留 1/4 核心给 OS、Tauri 主线程和前端渲染
pub(crate) fn compute_pool_size() -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let target = cpus * 3 / 4;
    target.clamp(2, 16)
}

/// 创建一个共享接收端
fn make_receiver() -> (mpsc::Sender<Job>, Arc<Mutex<mpsc::Receiver<Job>>>) {
    let (tx, rx) = mpsc::channel::<Job>();
    (tx, Arc::new(Mutex::new(rx)))
}

/// 启动一个工作线程
fn spawn_worker(
    id: usize,
    receiver: Arc<Mutex<mpsc::Receiver<Job>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .stack_size(WORKER_STACK_SIZE)
        .name(format!("conversion-engine-{}", id))
        .spawn(move || {
            loop {
                // 1. 获取任务：持锁时间极短（仅 recv 调用期间）
                let job = {
                    let lock = match receiver.lock() {
                        Ok(lock) => lock,
                        Err(_poisoned) => {
                            // 互斥锁被毒化（极罕见），尝试恢复
                            eprintln!("[转换引擎-worker-{}] 任务通道锁被毒化，尝试恢复...", id);
                            // 尝试使用 into_inner 恢复（Rust Mutex 支持）
                            match receiver.lock() {
                                Ok(lock) => lock,
                                Err(_) => {
                                    eprintln!("[转换引擎-worker-{}] 无法恢复，退出", id);
                                    break;
                                }
                            }
                        }
                    };
                    match lock.recv() {
                        Ok(job) => job,
                        Err(_) => break, // 发送端全部断开，正常退出
                    }
                };
                // 2. 锁已释放，执行任务（可能耗时较长）
                //    catch_unwind 确保 panic 不会杀死线程
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    job();
                }));
                // panic 被捕获，线程继续工作
            }
        })
        .unwrap_or_else(|e| panic!("无法创建转换引擎工作线程 {}: {}", id, e))
}

/// 初始化引擎（首次调用时）
fn init_engine() -> EngineInner {
    let pool_size = compute_pool_size();
    let (tx, rx) = make_receiver();

    let mut workers = Vec::with_capacity(pool_size);
    for id in 0..pool_size {
        let handle = spawn_worker(id, Arc::clone(&rx));
        workers.push(WorkerEntry { id, handle });
    }

    eprintln!(
        "[转换引擎] 线程池已启动：{} 个工作线程（每线程 {}MB 栈，总计 {}MB）",
        pool_size,
        WORKER_STACK_SIZE / 1024 / 1024,
        pool_size * WORKER_STACK_SIZE / 1024 / 1024
    );

    EngineInner {
        sender: tx,
        receiver: rx,
        workers,
    }
}

/// 确保引擎已启动，检查并修复死亡的工作线程，返回发送端
fn ensure_engine() -> mpsc::Sender<Job> {
    let mut guard = ENGINE.lock().unwrap();

    // 首次使用：初始化
    if guard.is_none() {
        *guard = Some(init_engine());
    }

    let inner = guard.as_mut().unwrap();

    // 检查每个工作线程，单独重建死亡的
    let total = inner.workers.len();
    let mut dead_indices: Vec<usize> = Vec::new();

    for (i, w) in inner.workers.iter().enumerate() {
        if w.handle.is_finished() {
            dead_indices.push(i);
        }
    }

    if !dead_indices.is_empty() {
        eprintln!(
            "[转换引擎] {}/{} 个工作线程已退出，正在单独重建...",
            dead_indices.len(),
            total
        );
        for i in dead_indices {
            let id = inner.workers[i].id;
            let handle = spawn_worker(id, Arc::clone(&inner.receiver));
            inner.workers[i] = WorkerEntry { id, handle };
            eprintln!("[转换引擎] worker-{} 已重建", id);
        }
    }

    inner.sender.clone()
}

/// 在转换引擎的线程池上执行闭包并获取结果。
///
/// 所有文件格式转换操作都应通过此函数调度，确保：
/// - 在线程池的 16MB 大栈工作线程上执行（不会栈溢出）
/// - 多任务自动并行（多文件同时转换）
/// - 不阻塞 Tauri 命令线程池
/// - 单个任务 panic 只影响该任务，不影响其他任务
///
/// # 示例
/// ```ignore
/// let result = run_on_worker(|| {
///     convert_md_to_txt(src, dst)
/// });
/// ```
pub fn run_on_worker<F, R>(f: F) -> Result<R, String>
where
    F: FnOnce() -> Result<R, String> + Send + 'static,
    R: Send + 'static,
{
    let sender = ensure_engine();
    let (result_tx, result_rx) = mpsc::sync_channel(1);

    let job = Box::new(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let flattened: Result<R, String> = match result {
            Ok(inner) => inner,
            Err(panic_info) => {
                let msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    format!("转换过程发生内部错误: {}", s)
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    format!("转换过程发生内部错误: {}", s)
                } else {
                    "转换过程发生未知内部错误".to_string()
                };
                Err(msg)
            }
        };
        let _ = result_tx.send(flattened);
    });

    sender
        .send(job)
        .map_err(|_| "转换引擎通道已断开，请重试".to_string())?;

    result_rx
        .recv()
        .map_err(|_| "转换引擎返回结果失败，请重试".to_string())?
}
