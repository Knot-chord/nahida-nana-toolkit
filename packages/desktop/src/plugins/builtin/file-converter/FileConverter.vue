/**
 * 文件格式转换组件
 *
 * 三层操作设计：
 * - 简单层：拖拽/点击添加文件，一键转换
 * - 自定义层：可选输出目录
 * - 源码层：新增转换格式在 Rust 后端扩展
 *
 * 支持超大单文件（流式处理）和批量转换（10+ 文件）。
 * 支持格式：md / txt / html / docx / pdf 互转。
 */

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import {
  NButton,
  NCard,
  NTag,
  NProgress,
  NSelect,
  NEmpty,
  NText,
  NModal,
  useMessage,
} from 'naive-ui'

/** 文件项 */
interface FileItem {
  id: string
  name: string
  path: string
  ext: string
  status: 'pending' | 'converting' | 'done' | 'error' | 'skipped'
  message: string
  size: number
  /** 该文件的目标格式（逐文件自定义） */
  targetExt: string
}

/** 格式映射：源扩展名 → 可转换的目标格式 */
const FORMAT_MAP: Record<string, { label: string; targets: { label: string; value: string }[] }> = {
  '.md': {
    label: 'Markdown',
    targets: [
      { label: '纯文本 (.txt)', value: '.txt' },
      { label: 'HTML (.html)', value: '.html' },
      { label: 'Word 文档 (.docx)', value: '.docx' },
      { label: 'PDF (.pdf)', value: '.pdf' },
    ],
  },
  '.markdown': {
    label: 'Markdown',
    targets: [
      { label: '纯文本 (.txt)', value: '.txt' },
      { label: 'HTML (.html)', value: '.html' },
      { label: 'Word 文档 (.docx)', value: '.docx' },
      { label: 'PDF (.pdf)', value: '.pdf' },
    ],
  },
  '.txt': {
    label: '纯文本',
    targets: [
      { label: 'HTML (.html)', value: '.html' },
      { label: 'Markdown (.md)', value: '.md' },
      { label: 'Word 文档 (.docx)', value: '.docx' },
      { label: 'PDF (.pdf)', value: '.pdf' },
    ],
  },
  '.html': {
    label: 'HTML',
    targets: [
      { label: '纯文本 (.txt)', value: '.txt' },
      { label: 'Markdown (.md)', value: '.md' },
      { label: 'Word 文档 (.docx)', value: '.docx' },
      { label: 'PDF (.pdf)', value: '.pdf' },
    ],
  },
  '.htm': {
    label: 'HTML',
    targets: [
      { label: '纯文本 (.txt)', value: '.txt' },
      { label: 'Markdown (.md)', value: '.md' },
      { label: 'Word 文档 (.docx)', value: '.docx' },
      { label: 'PDF (.pdf)', value: '.pdf' },
    ],
  },
  '.docx': {
    label: 'Word 文档',
    targets: [
      { label: '纯文本 (.txt)', value: '.txt' },
      { label: 'HTML (.html)', value: '.html' },
      { label: 'Markdown (.md)', value: '.md' },
      { label: 'PDF (.pdf)', value: '.pdf' },
    ],
  },
  '.pdf': {
    label: 'PDF 文档',
    targets: [
      { label: '纯文本 (.txt)', value: '.txt' },
      { label: 'HTML (.html)', value: '.html' },
      { label: 'Markdown (.md)', value: '.md' },
      { label: 'Word 文档 (.docx)', value: '.docx' },
    ],
  },
}

/** 去重后的格式标签（用于拖拽区展示，避免 Markdown/HTML 重复出现） */
const uniqueFormatLabels = computed(() => {
  const seen = new Set<string>()
  const result: { key: string; label: string }[] = []
  for (const [ext, info] of Object.entries(FORMAT_MAP)) {
    if (!seen.has(info.label)) {
      seen.add(info.label)
      result.push({ key: ext, label: info.label })
    }
  }
  return result
})

const SUPPORTED_EXTS = Object.keys(FORMAT_MAP)

const message = useMessage()
const files = ref<FileItem[]>([])
const outputDir = ref('')
const converting = ref(false)
const cancelRequested = ref(false)
/** 暂停状态：worker 领取下一个任务前检查，已在跑的任务自然跑完 */
const paused = ref(false)
const progress = ref(0)
const logs = ref<string[]>([])
const isDragging = ref(false)
/** 批量设置目标格式（空 = 未设置，各文件独立） */
const batchFormat = ref('')
/** 格式支持列表弹窗 */
const showFormatModal = ref(false)

/** 目录扫描载入状态（超大文件夹批量导入时实时反馈，避免“点了没反应”） */
const scanning = ref(false)
const scanScanned = ref(0)
const scanFound = ref(0)

/** 与后端 COLLECT_FILE_CAP 同源：达到上限时提示已截断 */
const SCAN_FILE_CAP = 500

/** 根据源扩展名获取可选目标格式 */
function getTargetsForExt(ext: string): { label: string; value: string }[] {
  return FORMAT_MAP[ext.toLowerCase()]?.targets || []
}

/** 批量设置：将所有文件的目标格式统一为选定值，同格式文件标记忽略，返回实际更新的数目 */
function onBatchFormatChange(val: string | null) {
  if (!val) return
  let updated = 0
  let ignored = 0
  for (const f of files.value) {
    // 同格式：转换无意义，直接标记忽略（不能保留旧目标，否则会被误转成其他格式）
    if (f.ext === val) {
      f.targetExt = f.ext
      f.status = 'skipped'
      f.message = '同格式，将忽略'
      ignored++
      continue
    }
    const targets = getTargetsForExt(f.ext)
    if (targets.some(t => t.value === val)) {
      f.targetExt = val
      if (f.status === 'skipped') {
        f.status = 'pending'
        f.message = ''
      }
      updated++
    }
  }
  const untouched = files.value.length - updated - ignored
  const parts = [`已更新 ${updated}/${files.value.length} 个文件`]
  if (ignored > 0) parts.push(`${ignored} 个同格式将忽略`)
  if (untouched > 0) parts.push(`${untouched} 个不支持该格式已跳过`)
  if (ignored > 0 || untouched > 0) {
    message.info(parts.join('，'))
  }
}

/** 是否有待转换文件 */
const hasFiles = computed(() => files.value.length > 0)

/** 「开始转换」禁用原因（空串 = 可点击），悬停按钮可见，避免用户不知为何点不了 */
const startDisabledReason = computed(() => {
  if (!hasFiles.value) return '先添加待转换文件'
  if (files.value.some(f => !f.targetExt)) return '还有文件未选择目标格式'
  return ''
})

/** 批量格式选项：所有文件源格式的并集目标（不兼容的文件在 onBatchFormatChange 中自动跳过） */
const batchOptions = computed(() => {
  if (files.value.length === 0) return []
  const srcExts = new Set(files.value.map(f => f.ext.toLowerCase()))
  const seen = new Set<string>()
  const result: { label: string; value: string }[] = []
  for (const ext of srcExts) {
    const info = FORMAT_MAP[ext]
    if (!info) continue
    for (const t of info.targets) {
      if (!seen.has(t.value)) {
        seen.add(t.value)
        result.push(t)
      }
    }
  }
  return result
})

/** 支持格式弹窗：五大格式（矩阵行列同源，code 作保真度键） */
const matrixFormats = [
  { code: 'md', name: 'Markdown', emoji: '📝', exts: '.md / .markdown' },
  { code: 'txt', name: '纯文本', emoji: '📄', exts: '.txt' },
  { code: 'html', name: 'HTML', emoji: '🌐', exts: '.html / .htm' },
  { code: 'docx', name: 'Word', emoji: '📘', exts: '.docx' },
  { code: 'pdf', name: 'PDF', emoji: '📕', exts: '.pdf' },
]

/**
 * 无损路径白名单（按转换器实现实测）：
 * - 纯文本为源：纯文本没有任何可丢失的内容，目标格式只是包装它
 * - Markdown → HTML：comrak GFM 完整渲染，内容全量保留
 * 其余路径均为有损：内容保留，排版/图片等细节随目标格式能力降级
 */
const LOSSLESS_PATHS = new Set([
  'txt->md', 'txt->html', 'txt->docx', 'txt->pdf',
  'md->html',
])

/** 查转换保真度（对角线同格式由模板单独处理） */
function fidelityOf(src: string, dst: string): 'lossless' | 'lossy' {
  return LOSSLESS_PATHS.has(`${src}->${dst}`) ? 'lossless' : 'lossy'
}

/** 转换须知：格式特性决定的技术限制，如实标注 */
const convertNotes = [
  '有损是格式特性决定的正常现象：排版、字体、复杂表格等细节随目标格式能力变化',
  '图片：Markdown / HTML → Word、PDF 时本地路径可解析即嵌入，否则以【图片】占位；Word、PDF → 其余格式以【图片】标记位置',
  'PDF 相关路径需本机 Python 环境（PyMuPDF / pdf2docx 等，首次使用按提示安装）',
  '扫描件 PDF 无 OCR 能力，转换结果可能为空',
]

/** 生成唯一 ID */
function uid(): string {
  return `f-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`
}

// ── Tauri 原生拖拽处理 ──
// 使用 onDragDropEvent 直接获取文件路径，避免 HTML5 File 读取导致的卡顿

const dropZoneRef = ref<HTMLElement | null>(null)
let unlistenDrag: (() => void) | null = null
let dragLeaveTimer: ReturnType<typeof setTimeout> | null = null
let _windowLeaveHandler: (() => void) | null = null

/** 拖拽遮罩层：记录拖拽区位置（CSS 像素） */
const dragOverlayBounds = ref({ top: 0, left: 0, width: 0, height: 0 })

function updateDropZoneBounds() {
  if (!dropZoneRef.value) return
  const rect = dropZoneRef.value.getBoundingClientRect()
  dragOverlayBounds.value = { top: rect.top, left: rect.left, width: rect.width, height: rect.height }
}

/** 判断 Tauri 事件坐标是否落在拖拽区内 */
function isInDropZone(physicalX: number, physicalY: number): boolean {
  if (!dropZoneRef.value) return false
  const dpr = window.devicePixelRatio || 1
  const cssX = physicalX / dpr
  const cssY = physicalY / dpr
  const rect = dropZoneRef.value.getBoundingClientRect()
  return cssX >= rect.left && cssX <= rect.right && cssY >= rect.top && cssY <= rect.bottom
}

onMounted(async () => {
  unlistenDrag = await getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === 'over') {
      // 实时悬停：坐标判定（物理像素→CSS像素）
      const inZone = isInDropZone(payload.position.x, payload.position.y)
      if (inZone) {
        if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null }
        if (!isDragging.value) updateDropZoneBounds()
        isDragging.value = true
      } else {
        // 100ms 防抖：避免快速移动时闪烁
        if (!dragLeaveTimer) {
          dragLeaveTimer = setTimeout(() => {
            isDragging.value = false
            dragLeaveTimer = null
          }, 100)
        }
      }
    } else if (payload.type === 'drop') {
      if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null }
      isDragging.value = false
      if (isInDropZone(payload.position.x, payload.position.y)) {
        // 直接使用文件路径，零延迟！
        addDroppedFiles(payload.paths)
      }
    } else {
      // cancel / leave
      if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null }
      isDragging.value = false
    }
  })
  // 窗口级 mouseleave：拖拽离开窗口时 Tauri 不发送 cancel，需手动重置
  const handleWindowLeave = () => {
    if (dragLeaveTimer) { clearTimeout(dragLeaveTimer); dragLeaveTimer = null }
    isDragging.value = false
  }
  document.addEventListener('mouseleave', handleWindowLeave)
  _windowLeaveHandler = handleWindowLeave
})

onUnmounted(() => {
  unlistenDrag?.()
  if (dragLeaveTimer) clearTimeout(dragLeaveTimer)
  if (_windowLeaveHandler) document.removeEventListener('mouseleave', _windowLeaveHandler)
})

/** 处理拖拽添加的内容（文件直接导入；文件夹经后端展开收集支持文档） */
async function addDroppedFiles(paths: string[]) {
  const filePaths: string[] = []
  const unsupported: string[] = []
  for (const p of paths) {
    const name = p.split(/[\\/]/).pop() || p
    const ext = '.' + (name.split('.').pop() || '').toLowerCase()
    if (SUPPORTED_EXTS.includes(ext)) {
      filePaths.push(p)
      continue
    }
    // 非支持扩展：可能是文件夹，交给后端展开收集（内部有深度/数量上限，带进度事件）
    try {
      const collected = await collectWithProgress(p)
      if (collected.length > 0) {
        filePaths.push(...collected)
        if (collected.length >= SCAN_FILE_CAP) {
          message.warning(`目录过大，已按 ${SCAN_FILE_CAP} 个文件上限截断导入`)
        }
      } else {
        unsupported.push(name)
      }
    } catch {
      unsupported.push(name)
    }
  }

  const added = await ingestPaths(filePaths)
  if (added > 0) {
    message.success(`导入成功：${added} 个文件，确认目标格式后点「开始转换」~`)
  }
  if (unsupported.length > 0) {
    message.warning(`以下内容不支持：${unsupported.slice(0, 3).join(', ')}${unsupported.length > 3 ? ` 等 ${unsupported.length} 个` : ''}`)
  }
}

/** 带进度反馈的目录扫描：监听后端节流推送的扫描进度事件 */
async function collectWithProgress(path: string): Promise<string[]> {
  scanning.value = true
  scanScanned.value = 0
  scanFound.value = 0
  const unlisten = await listen<{ scanned: number; found: number }>(
    'file-collect-progress',
    (ev) => {
      scanScanned.value = ev.payload.scanned
      scanFound.value = ev.payload.found
    }
  )
  try {
    return await invoke<string[]>('collect_supported_files', { path })
  } finally {
    unlisten()
    scanning.value = false
  }
}

/** 统一导入口：同路径去重（完成/失败条目允许重转），返回实际新增数 */
async function ingestPaths(paths: string[]): Promise<number> {
  // 先做纯内存筛选：扩展名过滤 + 同路径去重，不发起任何 IPC
  const fresh: string[] = []
  for (const filePath of paths) {
    const name = filePath.split(/[\\/]/).pop() || filePath
    const ext = '.' + (name.split('.').pop() || '').toLowerCase()
    if (!SUPPORTED_EXTS.includes(ext)) continue

    // 如果同路径文件已完成/失败/被忽略，替换为新条目（允许重新转换）
    const existIdx = files.value.findIndex(f => f.path === filePath)
    if (existIdx !== -1) {
      const st = files.value[existIdx].status
      if (st === 'done' || st === 'error' || st === 'skipped') {
        files.value.splice(existIdx, 1)
      } else {
        continue
      }
    }
    fresh.push(filePath)
  }
  if (fresh.length === 0) return 0

  // 批量取大小：一次 IPC 替代逐文件 N 次往返（超大文件夹导入性能关键）
  let sizeMap = new Map<string, number>()
  try {
    const stats = await invoke<{ path: string; size: number }[]>('get_files_info', { paths: fresh })
    sizeMap = new Map(stats.map(s => [s.path, s.size]))
  } catch {
    // 失败回退为大小 0，不阻断导入
  }

  // 先构建全部条目，再分片 push：避免数百张卡片一次性渲染冻住界面
  const items: FileItem[] = fresh.map(filePath => {
    const name = filePath.split(/[\\/]/).pop() || filePath
    const ext = '.' + (name.split('.').pop() || '').toLowerCase()
    return {
      id: uid(),
      name,
      path: filePath,
      ext,
      status: 'pending',
      message: '',
      size: sizeMap.get(filePath) || 0,
      targetExt: getTargetsForExt(ext)[0]?.value || '',
    }
  })
  const CHUNK = 40
  for (let i = 0; i < items.length; i += CHUNK) {
    files.value.push(...items.slice(i, i + CHUNK))
    if (i + CHUNK < items.length) await nextTick()
  }
  return items.length
}

/** 批量添加文件（通过对话框） */
async function addFilesDialog() {
  try {
    const selected = await openDialog({
      multiple: true,
      filters: [
        {
          name: '文档文件',
          extensions: ['md', 'markdown', 'txt', 'html', 'htm', 'docx', 'pdf'],
        },
      ],
    })
    if (!selected || (Array.isArray(selected) && selected.length === 0)) return
    const paths = Array.isArray(selected) ? selected : [selected]
    const added = await ingestPaths(paths)
    if (added > 0) {
      message.success(`导入成功：${added} 个文件，确认目标格式后点「开始转换」~`)
    }
  } catch (e) {
    message.error(`选择文件失败: ${e}`)
  }
}

/** 选择文件夹批量导入（后端递归收集受支持文档，深度/数量有上限） */
async function addFolderDialog() {
  try {
    const selected = await openDialog({ directory: true })
    if (!selected || typeof selected !== 'string') return
    await addDroppedFiles([selected])
  } catch (e) {
    message.error(`选择文件夹失败: ${e}`)
  }
}

/** 选择输出目录 */
async function selectOutputDir() {
  try {
    const selected = await openDialog({ directory: true })
    if (selected && typeof selected === 'string') {
      outputDir.value = selected
    }
  } catch (e) {
    message.error(`选择目录失败: ${e}`)
  }
}

/** 移除文件 */
function removeFile(id: string) {
  files.value = files.value.filter(f => f.id !== id)
}

/** 清空列表 */
function clearFiles() {
  files.value = []
  progress.value = 0
  logs.value = []
  batchFormat.value = ''
}

/** 添加日志 */
function addLog(msg: string) {
  const time = new Date().toLocaleTimeString()
  logs.value.push(`[${time}] ${msg}`)
}

/**
 * 转换并发数：按本机 CPU 核数自动检测，充分利用多核优势
 * 与后端转换引擎线程池同策略：取核数 3/4，留 1/4 给系统/界面，限幅 [2, 16]
 */
function computeConcurrency(): number {
  const cpus = navigator.hardwareConcurrency || 4
  return Math.min(16, Math.max(2, Math.floor((cpus * 3) / 4)))
}
const CONVERT_CONCURRENCY = computeConcurrency()

/** 开始转换（唯一启动入口：仅按钮触发，导入/扫描完成后绝不自动开始） */
async function startConvert() {
  if (!hasFiles.value) {
    message.warning('先添加文件再转换哦~')
    return
  }
  if (!outputDir.value) {
    // 兜底：未选输出目录时直接拉起选择，选完继续转换，避免“点了没反应”
    await selectOutputDir()
    if (!outputDir.value) return
  }

  // 全部同格式时无需转换，直接告知
  if (!files.value.some(f => f.targetExt && f.targetExt !== f.ext)) {
    message.info('全部文件都是同格式，无需转换~')
    return
  }

  converting.value = true
  cancelRequested.value = false
  paused.value = false
  progress.value = 0
  logs.value = []

  const total = files.value.length
  let successCount = 0
  let failCount = 0
  let skipCount = 0
  let nextIdx = 0
  let completed = 0

  addLog(`开始转换，共 ${total} 个文件（并发 ${Math.min(CONVERT_CONCURRENCY, total)} 路）`)

  /** 转换单个文件（worker 循环领取队列） */
  const convertOne = async (file: FileItem, order: number) => {
    file.status = 'converting'
    file.message = '转换中...'
    addLog(`[${order}/${total}] ${file.name}`)

    const targetExt = file.targetExt
    if (!targetExt) {
      file.status = 'error'
      file.message = '未选择目标格式'
      failCount++
      addLog(`  ✗ 未选择目标格式`)
      return
    }

    // 同格式：转换无意义，中性忽略（不计入成败）
    if (targetExt === file.ext) {
      file.status = 'skipped'
      file.message = '同格式，已忽略'
      skipCount++
      addLog(`  ⏭ ${file.name} 同格式转换无意义，已忽略`)
      return
    }

    const baseName = file.name.replace(/\.[^.]+$/, '')
    const dstPath = outputDir.value + '/' + baseName + targetExt

    try {
      const result = await invoke<{ success: boolean; message: string; size: number }>(
        'convert_file',
        { srcPath: file.path, dstPath }
      )

      if (result.success) {
        file.status = 'done'
        file.message = `完成 (${formatSize(result.size)})`
        successCount++
        addLog(`  ✓ ${file.name} → ${baseName}${targetExt}`)
      } else {
        file.status = 'error'
        file.message = result.message
        failCount++
        addLog(`  ✗ ${file.name}: ${result.message}`)
      }
    } catch (e) {
      file.status = 'error'
      file.message = String(e)
      failCount++
      addLog(`  ✗ ${file.name} 转换失败: ${e}`)
    }
  }

  /** 工作协程：不断从队列领取下一个文件，直到取完、被取消或暂停中 */
  const worker = async () => {
    for (;;) {
      if (cancelRequested.value) return
      await waitWhilePaused()
      if (cancelRequested.value) return
      const i = nextIdx++
      if (i >= total) return
      await convertOne(files.value[i], i + 1)
      completed++
      progress.value = (completed / total) * 100
    }
  }

  await Promise.all(
    Array.from({ length: Math.min(CONVERT_CONCURRENCY, total) }, () => worker())
  )

  if (cancelRequested.value) {
    addLog('用户取消转换')
  }
  converting.value = false
  paused.value = false
  addLog(`转换完成：成功 ${successCount}，失败 ${failCount}${skipCount > 0 ? `，忽略 ${skipCount}` : ''}`)

  if (failCount === 0 && successCount > 0) {
    message.success(`全部转换成功！共 ${successCount} 个文件 ✨${skipCount > 0 ? `（另忽略 ${skipCount} 个同格式）` : ''}`)
  } else if (successCount + failCount > 0) {
    message.warning(`转换完成，成功 ${successCount}，失败 ${failCount}${skipCount > 0 ? `，忽略 ${skipCount}` : ''}`)
  }
}

/** 暂停 / 继续转换 */
function togglePause() {
  paused.value = !paused.value
  addLog(paused.value ? '⏸ 已暂停（进行中的任务跑完后停止派新任务）' : '▶ 继续转换')
}

/** 暂停等待：worker 领取任务前调用，每 200ms 轮询，恢复或取消立即退出 */
async function waitWhilePaused() {
  while (paused.value && !cancelRequested.value) {
    await new Promise(r => setTimeout(r, 200))
  }
}

/** 取消转换 */
function cancelConvert() {
  cancelRequested.value = true
  addLog('正在取消...')
}

/** 格式化文件大小 */
function formatSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${units[i]}`
}

/** 状态标签类型 */
function statusType(status: FileItem['status']): 'default' | 'info' | 'success' | 'error' | 'warning' {
  const map = { pending: 'default', converting: 'info', done: 'success', error: 'error', skipped: 'default' } as const
  return map[status]
}



</script>

<template>
  <div class="file-converter">
    <!-- 拖拽遮罩层：拖拽时覆盖全屏，明确指示可放置区域 -->
    <Teleport to="body">
      <div v-if="isDragging" class="drag-overlay">
        <div
          class="drag-overlay-hole"
          :style="{
            top: dragOverlayBounds.top + 'px',
            left: dragOverlayBounds.left + 'px',
            width: dragOverlayBounds.width + 'px',
            height: dragOverlayBounds.height + 'px',
          }"
        >
        </div>
      </div>
    </Teleport>
    <!-- 拖拽区域 + 操作栏 -->
    <div
      ref="dropZoneRef"
      class="drop-zone"
      :class="{ 'drop-zone--active': isDragging }"
      @click="addFilesDialog"
    >
      <div class="drop-zone-icon">
        <!-- 默认态：两页文档 + 转换箭头（纳西妲配色：白+嫩绿+金） -->
        <svg v-if="!isDragging" class="icon-svg" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect x="8" y="12" width="28" height="36" rx="3" stroke="#78b868" stroke-width="2.5" fill="#fafaf8"/>
          <rect x="28" y="16" width="28" height="36" rx="3" stroke="#78b868" stroke-width="2.5" fill="#fafaf8"/>
          <path d="M22 32 L36 32" stroke="#98cc88" stroke-width="2.5" stroke-linecap="round"/>
          <path d="M32 28 L36 32 L32 36" stroke="#98cc88" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"/>
          <line x1="14" y1="22" x2="30" y2="22" stroke="#c9a84c" stroke-width="1.5" stroke-linecap="round" opacity="0.5"/>
          <line x1="14" y1="28" x2="26" y2="28" stroke="#c9a84c" stroke-width="1.5" stroke-linecap="round" opacity="0.5"/>
          <line x1="14" y1="34" x2="28" y2="34" stroke="#c9a84c" stroke-width="1.5" stroke-linecap="round" opacity="0.5"/>
          <line x1="34" y1="26" x2="50" y2="26" stroke="#c9a84c" stroke-width="1.5" stroke-linecap="round" opacity="0.5"/>
          <line x1="34" y1="32" x2="46" y2="32" stroke="#c9a84c" stroke-width="1.5" stroke-linecap="round" opacity="0.5"/>
          <line x1="34" y1="38" x2="48" y2="38" stroke="#c9a84c" stroke-width="1.5" stroke-linecap="round" opacity="0.5"/>
        </svg>
        <!-- 拖拽态：向下箭头 + 文档托盘 -->
        <svg v-else class="icon-svg icon-svg--active" viewBox="0 0 64 64" fill="none" xmlns="http://www.w3.org/2000/svg">
          <rect x="12" y="36" width="40" height="16" rx="3" stroke="#78b868" stroke-width="2.5" fill="#fafaf8"/>
          <path d="M32 12 L32 30" stroke="#78b868" stroke-width="3" stroke-linecap="round"/>
          <path d="M26 24 L32 30 L38 24" stroke="#78b868" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
        </svg>
      </div>
      <div class="drop-zone-text">
        <span v-if="isDragging">松开鼠标即可导入文件~</span>
        <span v-else>拖拽文件或文件夹到此处，或点击选择文件</span>
      </div>
      <div class="drop-zone-formats">
        <NTag v-for="item in uniqueFormatLabels" :key="item.key" size="tiny" round :bordered="false" class="format-tag">
          {{ item.label }}
        </NTag>
      </div>
    </div>

    <!-- 操作栏：左侧导入（输入）/ 右侧输出与转换 -->
    <div class="action-bar">
      <div class="action-left">
        <NButton size="small" @click.stop="addFilesDialog" :disabled="scanning">
          <span class="btn-icon">📄</span>选择文件
        </NButton>
        <NButton size="small" @click.stop="addFolderDialog" :disabled="scanning" title="批量导入文件夹内的受支持文档">
          <span class="btn-icon">📂</span>选择文件夹
        </NButton>
        <NButton size="small" @click.stop="clearFiles" :disabled="!hasFiles || converting">
          🗑 清空
        </NButton>
        <NButton text size="tiny" @click="showFormatModal = true" class="help-link-btn">📋 支持格式</NButton>
      </div>
      <div class="action-right">
        <NButton size="small" @click.stop="selectOutputDir" class="dir-btn" :title="outputDir ? `输出到：${outputDir}` : '选择输出目录（转换结果保存到这里）'">
          <span class="btn-icon">📤</span>
          <span class="dir-btn-label">{{ outputDir ? outputDir.split(/[\\/]/).pop() : '输出到…' }}</span>
        </NButton>
        <NButton
          v-if="converting"
          size="small"
          @click="togglePause"
        >
          {{ paused ? '▶ 继续' : '⏸ 暂停' }}
        </NButton>
        <NButton
          v-if="converting"
          size="small"
          @click="cancelConvert"
        >
          ⏹ 取消
        </NButton>
        <NButton
          type="primary"
          size="small"
          @click="startConvert"
          :disabled="!!startDisabledReason || converting"
          :title="startDisabledReason || '开始批量转换'"
        >
          ▶ 开始转换
        </NButton>
      </div>
    </div>

    <!-- 目录扫描载入条：超大文件夹批量导入时实时反馈扫描进度 -->
    <div v-if="scanning" class="scan-progress">
      <div class="scan-bar">
        <div class="scan-bar-track"></div>
      </div>
      <NText depth="3" class="scan-text">
        正在扫描目录…已发现 {{ scanFound }} 个文档（已扫描 {{ scanScanned }} 项）
      </NText>
    </div>

    <!-- 批量格式设置 -->
    <div v-if="hasFiles && batchOptions.length > 0" class="format-selector">
      <NText depth="3" style="font-size: 0.8rem; white-space: nowrap">批量设为：</NText>
      <NSelect
        v-model:value="batchFormat"
        :options="batchOptions"
        placeholder="统一目标格式"
        size="small"
        style="width: 10rem"
        @update:value="onBatchFormatChange"
      />
    </div>

    <!-- 文件列表 -->
    <NCard v-if="hasFiles" title="待转换文件" size="small" class="file-card">
      <template #header-extra>
        <NTag size="small" round>{{ files.length }} 个文件</NTag>
      </template>

      <div class="file-scroll-area">
        <div class="file-list">
          <div v-for="file in files" :key="file.id" class="file-item">
            <div class="file-info">
              <span class="file-name">{{ file.name }}</span>
              <NTag size="tiny" :type="statusType(file.status)" round>
                {{ file.status === 'pending' ? '等待' : file.status === 'converting' ? '转换中' : file.status === 'done' ? '完成' : file.status === 'skipped' ? '已忽略' : '失败' }}
              </NTag>
            </div>
            <div class="file-meta">
              <div class="file-format-row">
                <NText depth="3" style="font-size: 0.75rem">
                  {{ FORMAT_MAP[file.ext]?.label || file.ext }}
                  <span v-if="file.size"> · {{ formatSize(file.size) }}</span>
                  <span v-if="file.message && file.status !== 'pending' && file.status !== 'converting'"> · {{ file.message }}</span>
                </NText>
                <NSelect
                  v-model:value="file.targetExt"
                  :options="getTargetsForExt(file.ext)"
                  placeholder="选择格式"
                  size="tiny"
                  style="width: 7.5rem"
                  :disabled="converting || file.status === 'done'"
                />
              </div>
              <NButton text size="tiny" @click="removeFile(file.id)" :disabled="converting">
                ✕
              </NButton>
            </div>
            <!-- 单文件进度条 -->
            <NProgress
              v-if="file.status === 'converting'"
              :percentage="100"
              :status="'default'"
              :show-indicator="false"
              style="margin-top: 0.25rem"
              :height="4"
              indeterminate
            />
          </div>
        </div>
      </div>

      <!-- 总进度条 -->
      <div v-if="converting || progress > 0" style="margin-top: 0.5rem">
        <div style="display: flex; justify-content: space-between; align-items: center; margin-bottom: 0.25rem">
          <NText depth="3" style="font-size: 0.75rem">总进度</NText>
          <NText depth="3" style="font-size: 0.75rem">{{ Math.round(progress) }}%</NText>
        </div>
        <NProgress
          :percentage="Math.round(progress)"
          :status="progress >= 100 ? 'success' : 'default'"
          :show-indicator="false"
          :height="6"
        />
      </div>
    </NCard>

    <!-- 空状态 -->
    <div v-else class="empty-state">
      <NEmpty description="还没有文件，拖拽或点击添加~" />
    </div>

    <!-- 转换日志 -->
    <NCard v-if="logs.length > 0" title="转换日志" size="small" style="margin-top: 0.75rem">
      <div class="log-scroll-area">
        <div class="log-list">
          <div v-for="(log, i) in logs" :key="i" class="log-item">
            <NText style="font-size: 0.8rem; font-family: monospace">{{ log }}</NText>
          </div>
        </div>
      </div>
    </NCard>

    <!-- 格式支持列表弹窗 -->
    <NModal
      v-model:show="showFormatModal"
      preset="card"
      title="支持的转换格式"
      style="max-width: 42rem; width: 92%"
      :segmented="{ content: true }"
    >
      <!-- 转换矩阵：行 = 源格式，列 = 目标格式，保真度分档一眼看清 -->
      <table class="matrix-table">
        <thead>
          <tr>
            <th class="matrix-corner">源 ↓ · 目标 →</th>
            <th v-for="t in matrixFormats" :key="'h-' + t.code">
              <div class="matrix-head">{{ t.emoji }} {{ t.name }}</div>
              <div class="matrix-ext">{{ t.exts }}</div>
            </th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="s in matrixFormats" :key="'r-' + s.code">
            <td class="matrix-row-head">
              <div class="matrix-head">{{ s.emoji }} {{ s.name }}</div>
              <div class="matrix-ext">{{ s.exts }}</div>
            </td>
            <td v-for="t in matrixFormats" :key="'c-' + s.code + '-' + t.code" class="matrix-cell">
              <span v-if="s.code === t.code" class="matrix-same">—</span>
              <span
                v-else
                class="matrix-dot"
                :class="fidelityOf(s.code, t.code) === 'lossless' ? 'dot--lossless' : 'dot--lossy'"
                :title="fidelityOf(s.code, t.code) === 'lossless' ? '无损：内容完整保留' : '有损：内容保留，排版/图片等细节降级'"
              ></span>
            </td>
          </tr>
        </tbody>
      </table>

      <!-- 保真度图例 -->
      <div class="matrix-legend">
        <span class="legend-item"><span class="matrix-dot dot--lossless"></span>无损：内容完整保留</span>
        <span class="legend-item"><span class="matrix-dot dot--lossy"></span>有损：内容保留，排版/图片等细节降级</span>
        <span class="legend-item"><span class="matrix-same">—</span>同格式无需转换</span>
      </div>

      <!-- 转换须知：只讲格式特性带来的技术限制 -->
      <div class="convert-notes">
        <div class="convert-notes-title">📌 转换须知</div>
        <ul>
          <li v-for="(note, i) in convertNotes" :key="i">{{ note }}</li>
        </ul>
      </div>

      <div class="format-table-footer">
        <NText depth="3" style="font-size: 0.72rem">
          {{ matrixFormats.length }} 种格式全互转 · {{ matrixFormats.length * (matrixFormats.length - 1) }} 条路径（{{ LOSSLESS_PATHS.size }} 无损 + {{ matrixFormats.length * (matrixFormats.length - 1) - LOSSLESS_PATHS.size }} 有损）
        </NText>
      </div>
    </NModal>
  </div>
</template>

<style>
/* ── 拖拽遮罩层（Teleport 到 body，需全局样式） ── */
.drag-overlay {
  position: fixed;
  inset: 0;
  z-index: 9999;
  pointer-events: none;
  display: flex;
  align-items: center;
  justify-content: center;
}
.drag-overlay-hole {
  position: absolute;
  border: none;
  border-radius: 0.75rem;
  background: rgba(120, 184, 104, 0.08);
  box-shadow: none;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.15s ease;
}
.drag-overlay-hint {
  color: #78b868;
  font-size: 1.1rem;
  font-weight: 600;
  text-shadow: 0 1px 2px rgba(0,0,0,0.1);
  pointer-events: none;
}
</style>
<style scoped>
.file-converter {
  /* 顶部边距由外层 detail-header 统一控制（全站标准 1.5rem），不叠加额外顶部留白 */
  padding: 0 2rem 2rem;
  max-width: 56rem;
  box-sizing: border-box;
}

/* ── 拖拽区域 ── */
.drop-zone {
  border: 2px dashed var(--border-color, #d9d9d9);
  border-radius: 0.75rem;
  /* 内部留白收紧：旧值 2.5rem 使首个可视元素明显深于其他模块内容 */
  padding: 1.75rem 1rem;
  text-align: center;
  cursor: pointer;
  transition: all 0.25s ease;
  background: #fafaf8;
}

.drop-zone:hover {
  border-color: #98cc88;
  background: rgba(120, 184, 104, 0.04);
}

.drop-zone--active {
  border-color: #78b868;
  background: rgba(120, 184, 104, 0.06);
  border-style: solid;
  transform: scale(1.01);
}

.drop-zone-icon {
  margin-bottom: 0.5rem;
  transition: transform 0.2s;
}

.drop-zone-icon .icon-svg {
  width: 64px;
  height: 64px;
}

.drop-zone--active .drop-zone-icon {
  transform: scale(1.1);
}

.drop-zone-text {
  font-size: 0.95rem;
  color: var(--text-primary, #333);
  font-weight: 500;
}

.drop-zone-formats {
  display: flex;
  flex-wrap: wrap;
  justify-content: center;
  gap: 0.25rem;
  margin-top: 0.5rem;
  max-width: 100%;
}

.format-tag {
  font-size: 0.7rem !important;
  padding: 0 0.4rem !important;
  background: rgba(120, 184, 104, 0.08) !important;
  color: #78b868 !important;
}

/* ── 格式选择 ── */
.format-selector {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.75rem;
  padding: 0.5rem 0.75rem;
  background: rgba(0, 0, 0, 0.02);
  border-radius: 0.5rem;
}

/* ── 操作栏 ── */
.action-bar {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 0.75rem;
  gap: 0.5rem;
  flex-wrap: wrap;
}

.action-left {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.action-right {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.dir-btn {
  display: inline-flex;
  align-items: center;
}

.dir-btn-label {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 8rem;
  display: inline-block;
}

.btn-icon {
  opacity: 0.6;
  margin-right: 0.25rem;
}

/* ── 文件列表 ── */
.file-list {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.file-item {
  padding: 0.5rem 0.75rem;
  border-radius: 0.375rem;
  background: rgba(0, 0, 0, 0.02);
}

.file-info {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.file-name {
  font-size: 0.875rem;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 70%;
}

.file-meta {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 0.125rem;
}

.file-format-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  flex: 1;
}

/* ── 空状态 ── */
.empty-state {
  padding: 2rem 0;
  text-align: center;
}

.help-link-btn {
  color: #5a9a48 !important;
  font-size: 0.75rem !important;
  border: 1px solid rgba(120, 184, 104, 0.5) !important;
  border-radius: 0.25rem !important;
  padding: 0 0.4rem !important;
  height: 1.5rem !important;
  line-height: 1.5rem !important;
  white-space: nowrap;
  transition: all 0.2s;
}

.help-link-btn:hover {
  color: #3d7a30 !important;
  border-color: #78b868 !important;
  background: rgba(120, 184, 104, 0.08) !important;
}

/* ── 文件列表卡片 ── */
.file-card {
  margin-top: 1rem;
}

.file-card :deep(.n-card__content) {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 0.5rem !important;
}

.file-scroll-area {
  max-height: 35vh;
  overflow-y: auto;
}

/* ── 目录扫描载入条 ── */
.scan-progress {
  display: flex;
  flex-direction: column;
  gap: 0.3rem;
  margin-bottom: 0.6rem;
}

.scan-bar {
  height: 4px;
  border-radius: 2px;
  background: rgba(120, 184, 104, 0.15);
  overflow: hidden;
}

.scan-bar-track {
  width: 40%;
  height: 100%;
  border-radius: 2px;
  background: #78b868;
  animation: scan-slide 1.2s ease-in-out infinite;
}

@keyframes scan-slide {
  0% { transform: translateX(-100%); }
  100% { transform: translateX(350%); }
}

.scan-text {
  font-size: 0.72rem;
}

/* ── 转换矩阵（支持格式弹窗） ── */
.matrix-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.82rem;
}

.matrix-table th,
.matrix-table td {
  padding: 0.5rem 0.35rem;
  border-bottom: 1px solid rgba(0, 0, 0, 0.05);
  text-align: center;
  vertical-align: middle;
}

.matrix-table thead th {
  border-bottom: 2px solid rgba(120, 184, 104, 0.25);
}

.matrix-corner {
  font-size: 0.7rem;
  color: #999;
  font-weight: 500;
  white-space: nowrap;
}

.matrix-row-head {
  text-align: left !important;
  white-space: nowrap;
}

.matrix-head {
  font-weight: 600;
  color: #3d7a30;
  font-size: 0.8rem;
}

.matrix-table thead .matrix-head {
  color: #555;
}

.matrix-ext {
  font-size: 0.64rem;
  color: #aaa;
  font-weight: normal;
  margin-top: 0.1rem;
}

.matrix-dot {
  display: inline-block;
  width: 9px;
  height: 9px;
  border-radius: 50%;
}

.dot--lossless {
  background: #5b9bd5;
}

.dot--lossy {
  background: #78b868;
}

.matrix-same {
  color: #d5d5d5;
}

.matrix-legend {
  display: flex;
  flex-wrap: wrap;
  gap: 0.4rem 1.25rem;
  margin-top: 0.7rem;
}

.legend-item {
  display: inline-flex;
  align-items: center;
  gap: 0.4rem;
  font-size: 0.72rem;
  color: #777;
}

.convert-notes {
  margin-top: 0.75rem;
  padding: 0.6rem 0.75rem;
  background: rgba(120, 184, 104, 0.06);
  border-radius: 0.5rem;
}

.convert-notes-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: #3d7a30;
  margin-bottom: 0.3rem;
}

.convert-notes ul {
  margin: 0;
  padding-left: 1.1rem;
}

.convert-notes li {
  font-size: 0.72rem;
  color: #777;
  line-height: 1.7;
}

.format-table-footer {
  display: flex;
  align-items: center;
  gap: 0.25rem;
  padding-top: 0.6rem;
  margin-top: 0.25rem;
  border-top: 1px solid rgba(0, 0, 0, 0.06);
}

/* ── 日志 ── */
.log-scroll-area {
  max-height: 140px;
  overflow-y: auto;
}

.log-list {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.log-item {
  padding: 0.125rem 0;
}

/* ── 纳西妲配色：白净 + 嫩绿（偏暖草绿）+ 金色点缀（源自官方：白草净华） ── */
:deep(.n-button--primary-type) {
  --n-color: #78b868;
  --n-color-hover: #8acc7a;
  --n-color-pressed: #62a050;
  --n-border: #78b868;
  --n-border-hover: #8acc7a;
  --n-border-pressed: #62a050;
  --n-text-color: #fff;
  --n-ripple-color: rgba(120, 184, 104, 0.15);
}

:deep(.n-button--primary-type .n-button__icon) {
  color: #fff;
}
</style>
