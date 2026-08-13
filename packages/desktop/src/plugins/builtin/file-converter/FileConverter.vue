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
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'
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
  status: 'pending' | 'converting' | 'done' | 'error'
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
const progress = ref(0)
const logs = ref<string[]>([])
const isDragging = ref(false)
/** 批量设置目标格式（空 = 未设置，各文件独立） */
const batchFormat = ref('')
/** 格式支持列表弹窗 */
const showFormatModal = ref(false)

/** 根据源扩展名获取可选目标格式 */
function getTargetsForExt(ext: string): { label: string; value: string }[] {
  return FORMAT_MAP[ext.toLowerCase()]?.targets || []
}

/** 批量设置：将所有文件的目标格式统一为选定值，返回实际更新的数目 */
function onBatchFormatChange(val: string | null) {
  if (!val) return
  let updated = 0
  for (const f of files.value) {
    const targets = getTargetsForExt(f.ext)
    if (targets.some(t => t.value === val)) {
      f.targetExt = val
      updated++
    }
  }
  if (updated < files.value.length) {
    message.info(`已更新 ${updated}/${files.value.length} 个文件，${files.value.length - updated} 个不支持该格式已跳过`)
  }
}

/** 是否有待转换文件 */
const hasFiles = computed(() => files.value.length > 0)

/** 「开始转换」禁用原因（空串 = 可点击），悬停按钮可见，避免用户不知为何点不了 */
const startDisabledReason = computed(() => {
  if (!hasFiles.value) return '先添加待转换文件'
  if (!outputDir.value) return '先选择输出目录'
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

/** 总转换路径数（动态计算） */
const totalConversionPaths = computed(() => {
  return Object.values(FORMAT_MAP).reduce((sum, info) => sum + info.targets.length, 0)
})

/** 格式支持详情列表（用于弹窗展示） */
const formatSupportList = computed(() => {
  // 源格式能提取图片的（TXT 是纯文本，没有图片）
  const sourceHasImage: Record<string, boolean> = {
    'Markdown': true,
    'HTML': true,
    'Word 文档': true,
    'PDF 文档': true,
    '纯文本': false,
  }
  // 目标格式能嵌入图片的
  const targetCanEmbed = ['.docx', '.pdf']

  const seen = new Set<string>()
  const result: { label: string; exts: string; targets: { name: string; ext: string; image: boolean }[]; note: string }[] = []
  for (const [, info] of Object.entries(FORMAT_MAP)) {
    if (seen.has(info.label)) continue
    seen.add(info.label)
    const srcImg = sourceHasImage[info.label] ?? false
    const targets = info.targets.map(t => ({
      name: t.label.replace(/ \(.*\)/, ''),
      ext: t.value,
      // 源能提取 且 目标能嵌入 = 支持图片
      image: srcImg && targetCanEmbed.includes(t.value),
    }))
    const exts = Object.entries(FORMAT_MAP)
      .filter(([, v]) => v.label === info.label)
      .map(([k]) => k)
      .join(' / ')
    let note = ''
    if (info.label === '纯文本') note = '纯文本无图片'
    else if (info.label === 'PDF 文档') note = '图片按文档流位置提取'
    else if (info.label === 'Word 文档') note = '图片按段落位置嵌入'
    else if (info.label === 'Markdown') note = '图片引用保留原始路径'
    result.push({ label: info.label, exts, targets, note })
  }
  return result
})

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
    // 非支持扩展：可能是文件夹，交给后端展开收集（内部有深度/数量上限）
    try {
      const collected = await invoke<string[]>('collect_supported_files', { path: p })
      if (collected.length > 0) filePaths.push(...collected)
      else unsupported.push(name)
    } catch {
      unsupported.push(name)
    }
  }

  const added = await ingestPaths(filePaths)
  if (added > 0) {
    message.success(`已导入 ${added} 个文件~`)
  }
  if (unsupported.length > 0) {
    message.warning(`以下内容不支持：${unsupported.slice(0, 3).join(', ')}${unsupported.length > 3 ? ` 等 ${unsupported.length} 个` : ''}`)
  }
}

/** 统一导入口：同路径去重（完成/失败条目允许重转），返回实际新增数 */
async function ingestPaths(paths: string[]): Promise<number> {
  let added = 0
  for (const filePath of paths) {
    const name = filePath.split(/[\\/]/).pop() || filePath
    const ext = '.' + (name.split('.').pop() || '').toLowerCase()
    if (!SUPPORTED_EXTS.includes(ext)) continue

    // 如果同路径文件已完成/失败，替换为新条目（允许重新转换）
    const existIdx = files.value.findIndex(f => f.path === filePath)
    if (existIdx !== -1) {
      const st = files.value[existIdx].status
      if (st === 'done' || st === 'error') {
        files.value.splice(existIdx, 1)
      } else {
        continue
      }
    }

    // 获取文件大小
    let size = 0
    try {
      const stat = await invoke<{ size: number }>('get_file_size', { path: filePath })
      size = stat.size
    } catch {
      // 忽略错误，大小为 0
    }

    files.value.push({
      id: uid(),
      name,
      path: filePath,
      ext,
      status: 'pending',
      message: '',
      size,
      targetExt: getTargetsForExt(ext)[0]?.value || '',
    })
    added++
  }
  return added
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
      message.success(`已添加 ${added} 个文件~`)
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

/** 开始转换 */
async function startConvert() {
  if (!hasFiles.value) {
    message.warning('先添加文件再转换哦~')
    return
  }
  if (!outputDir.value) {
    message.warning('请选择输出目录~')
    return
  }

  converting.value = true
  cancelRequested.value = false
  progress.value = 0
  logs.value = []

  const total = files.value.length
  let successCount = 0
  let failCount = 0

  addLog(`开始转换，共 ${total} 个文件`)

  for (let i = 0; i < files.value.length; i++) {
    if (cancelRequested.value) {
      addLog('用户取消转换')
      break
    }

    const file = files.value[i]
    file.status = 'converting'
    file.message = '转换中...'
    addLog(`[${i + 1}/${total}] ${file.name}`)

    const targetExt = file.targetExt
    if (!targetExt) {
      file.status = 'error'
      file.message = '未选择目标格式'
      failCount++
      addLog(`  ✗ 未选择目标格式`)
      progress.value = ((i + 1) / total) * 100
      continue
    }

    // 同格式跳过
    if (targetExt === file.ext) {
      file.status = 'error'
      file.message = '源格式与目标格式相同'
      failCount++
      addLog(`  ✗ ${file.name} 源格式与目标格式相同，跳过`)
      progress.value = ((i + 1) / total) * 100
      continue
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
        addLog(`  ✗ ${result.message}`)
      }
    } catch (e) {
      file.status = 'error'
      file.message = String(e)
      failCount++
      addLog(`  ✗ 转换失败: ${e}`)
    }

    progress.value = ((i + 1) / total) * 100
  }

  converting.value = false
  addLog(`转换完成：成功 ${successCount}，失败 ${failCount}`)

  if (failCount === 0) {
    message.success(`全部转换成功！共 ${successCount} 个文件 ✨`)
  } else {
    message.warning(`转换完成，成功 ${successCount}，失败 ${failCount}`)
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
  const map = { pending: 'default', converting: 'info', done: 'success', error: 'error' } as const
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
        <span v-else>拖拽文件或文件夹到此处，或点击选择文件 · <span class="import-folder-link" @click.stop="addFolderDialog">导入文件夹</span></span>
      </div>
      <div class="drop-zone-formats">
        <NTag v-for="item in uniqueFormatLabels" :key="item.key" size="tiny" round :bordered="false" class="format-tag">
          {{ item.label }}
        </NTag>
      </div>
    </div>

    <!-- 格式选择 + 操作栏 -->
    <div class="action-bar">
      <div class="action-left">
        <NButton size="small" @click.stop="clearFiles" :disabled="!hasFiles || converting">
          🗑 清空
        </NButton>
        <NButton size="small" @click.stop="selectOutputDir" class="dir-btn" :title="outputDir || '选择输出目录'">
          <span style="opacity: 0.6; margin-right: 0.25rem">📁</span>
          <span style="overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 8rem; display: inline-block">
            {{ outputDir ? outputDir.split(/[\\/]/).pop() : '选择目录' }}
          </span>
        </NButton>
        <NButton text size="tiny" @click="showFormatModal = true" class="help-link-btn">📋 支持格式</NButton>
      </div>
      <div class="action-right">
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
                {{ file.status === 'pending' ? '等待' : file.status === 'converting' ? '转换中' : file.status === 'done' ? '完成' : '失败' }}
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
      <table class="format-table">
        <thead>
          <tr>
            <th>源格式</th>
            <th>可转换为</th>
            <th>说明</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="item in formatSupportList" :key="item.label">
            <td class="col-source">
              <strong>{{ item.label }}</strong>
              <span class="ext-hint">{{ item.exts }}</span>
            </td>
            <td class="col-targets">
              <span
                v-for="t in item.targets"
                :key="t.ext"
                class="target-chip"
                :class="{ 'target-chip--img': t.image }"
              >
                <span class="chip-dot" :class="t.image ? 'dot--yes' : 'dot--no'"></span>
                {{ t.name }}
              </span>
            </td>
            <td class="col-note">{{ item.note || '—' }}</td>
          </tr>
        </tbody>
      </table>
      <div class="format-table-footer">
        <span class="chip-dot dot--yes" style="display:inline-block;vertical-align:middle;margin-right:2px"></span>
        <NText depth="3" style="font-size: 0.72rem">
          = 支持图片嵌入 · 共 {{ formatSupportList.length }} 种源格式、{{ totalConversionPaths }} 条转换路径
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

.import-folder-link {
  color: #78b868;
  cursor: pointer;
}

.import-folder-link:hover {
  text-decoration: underline;
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
  color: #999 !important;
  font-size: 0.75rem !important;
  border: 1px solid #ddd !important;
  border-radius: 0.25rem !important;
  padding: 0 0.4rem !important;
  height: 1.5rem !important;
  line-height: 1.5rem !important;
  white-space: nowrap;
  transition: all 0.2s;
}

.help-link-btn:hover {
  color: #78b868 !important;
  border-color: #78b868 !important;
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

/* ── 格式支持表格 ── */
.format-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 0.82rem;
}

.format-table th {
  text-align: left;
  padding: 0.5rem 0.6rem;
  font-weight: 600;
  font-size: 0.75rem;
  color: #666;
  border-bottom: 2px solid rgba(120, 184, 104, 0.25);
  white-space: nowrap;
}

.format-table td {
  padding: 0.55rem 0.6rem;
  border-bottom: 1px solid rgba(0, 0, 0, 0.05);
  vertical-align: middle;
}

.col-source {
  white-space: nowrap;
}

.col-source strong {
  color: #5a9a48;
}

.ext-hint {
  display: block;
  font-size: 0.68rem;
  color: #999;
  font-weight: normal;
}

.col-targets {
  display: flex;
  flex-wrap: wrap;
  gap: 0.3rem;
}

.target-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.15rem 0.45rem;
  border-radius: 0.25rem;
  font-size: 0.76rem;
  color: #555;
  background: rgba(0, 0, 0, 0.03);
}

.target-chip--img {
  background: rgba(120, 184, 104, 0.1);
  color: #3d7a30;
}

.chip-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  flex-shrink: 0;
}

.dot--yes {
  background: #78b868;
}

.dot--no {
  background: #d0d0d0;
}

.col-note {
  font-size: 0.7rem;
  color: #888;
  font-style: italic;
  max-width: 10rem;
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
