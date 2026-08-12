/**
 * useSystemMonitor — 系统监控状态管理（应用级单例）
 *
 * 职责：
 * - 应用启动即开始后台常驻采集，进入虚空终端时数据已在流动，零等待
 * - 定时调用 Rust system_info 获取 CPU/RAM 信息
 * - 同轮并行调用 get_gpu_info 获取 GPU/VRAM 信息（采集在工作线程，不阻塞 UI）
 * - 自适应轮询间隔（2s~4s，CPU/内存/GPU 同一轮刷新，进度条同步跳动）
 * - 监控范围：CPU / 内存 / GPU / 显存
 */

import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface SystemInfo {
  os_name: string
  os_version: string
  kernel_version: string
  hostname: string
  cpu_name: string
  cpu_usage: number
  cpu_cores: number
  memory_total: number
  memory_used: number
  memory_available: number
}

/** GPU 信息 */
export interface GpuInfo {
  name: string
  vram_total: number
  vram_used: number
  vram_free: number
  usage_percent: number
  temperature: number
}

/** 默认系统信息（首次渲染用） */
const DEFAULT_SYSTEM_INFO: SystemInfo = {
  os_name: '—',
  os_version: '—',
  kernel_version: '—',
  hostname: '—',
  cpu_name: '—',
  cpu_usage: 0,
  cpu_cores: 0,
  memory_total: 0,
  memory_used: 0,
  memory_available: 0,
}

/* ── 模块级单例状态：全应用共享同一份数据，多处调用不重复采集 ── */
const info = ref<SystemInfo>({ ...DEFAULT_SYSTEM_INFO })
const gpus = ref<GpuInfo[]>([])
const isLoading = ref(true)

let _timer: ReturnType<typeof setInterval> | null = null
let _started = false
let _lastCpuUsage = 0
let _pollInterval = 2000 // 默认 2s
let _sysBusy = false
let _gpuBusy = false

/** 自适应轮询：CPU 变化剧烈时缩短间隔（下限 2s：GPU 采样本身需 1s+） */
function adaptInterval(cpuUsage: number): void {
  const delta = Math.abs(cpuUsage - _lastCpuUsage)
  _lastCpuUsage = cpuUsage

  if (delta > 20) {
    _pollInterval = 2000 // 剧烈变化 → 2s
  } else if (delta > 10) {
    _pollInterval = 3000
  } else {
    _pollInterval = 4000 // 平稳 → 4s
  }

  // 重启定时器
  if (_timer) {
    clearInterval(_timer)
    _timer = setInterval(fetchAll, _pollInterval)
  }
}

/** 同一轮并行采集 CPU/内存 + GPU，保证所有进度条同步刷新无时差 */
async function fetchAll(): Promise<void> {
  if (!_sysBusy) {
    _sysBusy = true
    try {
      const data = await invoke<SystemInfo>('get_system_info')
      info.value = data
      isLoading.value = false
      adaptInterval(data.cpu_usage)
    } catch (e) {
      console.error('[useSystemMonitor] 获取系统信息失败：', e)
      isLoading.value = false
    } finally {
      _sysBusy = false
    }
  }
  // GPU 采样耗时 1s+，上一轮未完成则跳过，避免 PowerShell 进程堆积
  if (!_gpuBusy) {
    _gpuBusy = true
    try {
      gpus.value = await invoke<GpuInfo[]>('get_gpu_info')
    } catch {
      // GPU 信息获取失败不报错（采集失败是正常降级）
    } finally {
      _gpuBusy = false
    }
  }
}

/** 启动监控（幂等：全局只启动一次，后台常驻不随页面卸载停止） */
export function startSystemMonitor(): void {
  if (_started) return
  _started = true
  fetchAll()
  _timer = setInterval(fetchAll, _pollInterval)
}

export function useSystemMonitor() {
  // 任何调用方挂载时确保采集已启动（幂等）
  onMounted(() => startSystemMonitor())

  return {
    info,
    gpus,
    isLoading,
  }
}
