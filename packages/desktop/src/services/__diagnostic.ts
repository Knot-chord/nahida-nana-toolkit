/**
 * __diagnostic — 临时诊断模块（用完即删）
 *
 * 职责：
 * - 无侵入地记录聊天发送全过程的关键事件与耗时
 * - 所有日志写入 localStorage 环形缓冲区，可随时导出
 * - 提供诊断面板 UI 组件
 *
 * 使用方法：
 * 1. import { diag } from '../services/__diagnostic'
 * 2. diag.sendStart('myKey', { detail })
 * 3. diag.sendEnd('myKey')
 */

const STORAGE_KEY = 'nahida-diagnostic-logs'
const MAX_LOG = 500

export interface DiagEvent {
  id: number
  ts: number
  cat: 'SEND' | 'STATE' | 'PERF' | 'API' | 'ERROR' | 'RENDER'
  label: string
  ms?: number          // 耗时（ms）
  detail?: unknown
}

function load(): DiagEvent[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    return raw ? (JSON.parse(raw) as DiagEvent[]) : []
  } catch { return [] }
}

function save(events: DiagEvent[]): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(events.slice(-MAX_LOG)))
  } catch { /* quota exceeded — clear half */ }
}

let _id = 0
let _timers: Record<string, number> = {}

export const diag = {
  /** 记录一个事件 */
  log(cat: DiagEvent['cat'], label: string, detail?: unknown): number {
    const ev: DiagEvent = { id: ++_id, ts: Date.now(), cat, label, detail }
    const events = load()
    events.push(ev)
    save(events)
    // eslint-disable-next-line no-console
    console.log(`[DIAG] ${label}`, detail ?? '')
    return ev.id
  },

  /** 启动计时器 */
  timerStart(key: string): void {
    _timers[key] = performance.now()
  },

  /** 停止计时器并记录耗时事件 */
  timerEnd(cat: DiagEvent['cat'], key: string, label: string, detail?: unknown): number {
    const start = _timers[key]
    if (start == null) {
      return this.log(cat, `[TIMER MISS] ${label}`, detail)
    }
    delete _timers[key]
    const ms = +(performance.now() - start).toFixed(1)
    return this.log(cat, label, { ...(detail as object ?? {}), ms: `${ms}ms` })
  },

  /** 快速标记发送起始 */
  sendStart(label: string, detail?: unknown): string {
    const key = `send_${label}_${Date.now()}`
    this.log('SEND', `▶ ${label}`, detail)
    this.timerStart(key)
    return key
  },

  /** 快速标记发送结束并记录耗时 */
  sendEnd(key: string, label: string, detail?: unknown): void {
    this.timerEnd('SEND', key, label, detail)
  },

  /** 记录错误 */
  error(label: string, err: unknown): void {
    const detail = err instanceof Error
      ? { message: err.message, name: err.name, stack: err.stack?.slice(0, 300) }
      : String(err)
    this.log('ERROR', `✗ ${label}`, detail)
  },

  /** 导出全部日志文本（用于复制） */
  exportText(): string {
    const events = load()
    return events.map(e => {
      const t = new Date(e.ts).toLocaleTimeString('zh-CN', { hour12: false })
      const ms = e.ms != null ? ` +${e.ms}ms` : ''
      const d = e.detail != null ? ` | ${JSON.stringify(e.detail)}` : ''
      return `[${t}][${e.cat}]${ms} ${e.label}${d}`
    }).join('\n')
  },

  /** 清空日志 */
  clear(): void {
    localStorage.removeItem(STORAGE_KEY)
    _timers = {}
  },

  /** 获取最近事件 */
  recent(n = 100): DiagEvent[] {
    return load().slice(-n)
  },

  /** 汇总统计 */
  summary(): string {
    const events = load()
    const cats = {} as Record<string, number>
    const errs: string[] = []
    for (const e of events) {
      cats[e.cat] = (cats[e.cat] ?? 0) + 1
      if (e.cat === 'ERROR') errs.push(e.label)
    }
    return [
      `事件总数: ${events.length}`,
      ...Object.entries(cats).map(([k, v]) => `  ${k}: ${v}`),
      ...(errs.length ? [`\n错误 (${errs.length}):`, ...errs.map(s => `  ${s}`)] : []),
    ].join('\n')
  },
}
