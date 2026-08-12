/**
 * 虚空终端
 *
 * 职责：AI统一入口 —— "我需要AI帮忙"
 * 系统状态作为辅助信息，不喧宾夺主
 *
 * 交互状态：
 * - 沉睡态 (idle)：居中卡片布局，AI能力预览，点击输入框唤醒
 * - 活跃态 (active)：底部输入布局，对谈界面，Esc/▲ 回到沉睡态
 *
 * 关键技术：
 * - 双态切换：沉睡态↔活跃态纯淡入淡出（opacity transition），无位移
 * - 自适应轮询：根据CPU变化剧烈程度动态调整检测频率（1s~5s）
 */

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted, nextTick, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { tempDir } from '@tauri-apps/api/path'
import { writeFile, remove } from '@tauri-apps/plugin-fs'
import { useChatStream } from '../services/use-chat-stream'
import { useChatStore } from '../stores/chat'
import { diag } from '../services/__diagnostic'
import { renderMarkdown } from '../services/markdown'
import { useConversations, WELCOME_TEXT } from '../services/conversations'
import type { ChatMessage } from '@nahida-nana/shared'
import { useSystemMonitor } from '../services/use-system-monitor'

const { info, gpus, isLoading } = useSystemMonitor()

const isActive = ref(false)
const chatInput = ref<HTMLInputElement | null>(null)
const fileInput = ref<HTMLInputElement | null>(null)

// ── 对话管理 ──
const conv = useConversations()

// 确保至少有一个对话
if (!conv.current.value) {
  conv.create()
}

// ── AI 对话 ──
const chatStore = useChatStore()
// ── 诊断面板 ──
const diagOpen = ref(false)
const diagEvents = ref<ReturnType<typeof diag.recent>>([])

function diagRefresh() {
  diagEvents.value = diag.recent(300)
}
function diagCopy() {
  navigator.clipboard.writeText(diag.exportText()).catch(() => {})
}

/** 诊断日志：时间戳 → HH:mm:ss */
function fmtDiagTime(ts: number): string {
  return new Date(ts).toLocaleTimeString('zh-CN', { hour12: false })
}

/** 诊断日志：detail 格式化（耗时优先展示，过长截断） */
function fmtDiagDetail(ev: { detail?: unknown }): string {
  if (ev.detail == null) return ''
  const s = typeof ev.detail === 'string' ? ev.detail : JSON.stringify(ev.detail)
  return s.length > 200 ? s.slice(0, 200) + '…' : s
}

// ── 暴露给模板 ──

const { messages, isStreaming, lastError, send: rawSend, abort: rawAbort, retry } = useChatStream()

/** 欢迎动画是否正在播放（独立于 AI streaming，不阻塞输入） */
const isWelcomePlaying = ref(false)
/** 欢迎动画是否已被用户取消 */
let _welcomeAborted = false
/** 发送中同步锁：防止 async 间隙的并发点击 */
let _sending = false

// 包装 abortChat：同时支持取消欢迎动画和 AI 请求
function abortChat() {
  if (isStreaming.value || isWelcomePlaying.value) {
    _welcomeAborted = true
    rawAbort()
  }
}

// 从对话 store 加载消息（无消息时注入欢迎语）
function loadConversation() {
  const c = conv.current.value
  if (!c) { messages.value = []; return }
  if (c.messages.length > 0) {
    // 排除可能持久化的前端欢迎语（role:assistant 且之前无 user 消息）
    const firstUserIdx = c.messages.findIndex(m => m.role === 'user')
    const filtered = firstUserIdx >= 0
      ? c.messages.slice(firstUserIdx)
      : c.messages
    messages.value = [...filtered]
    console.debug('[TerminalPage]', '加载已有对话', `msgs=${filtered.length}`)
  } else {
    // 新对话：注入欢迎语
    messages.value = [{ role: 'assistant', content: '' }]
    simulateWelcome()
  }
}
loadConversation()

/** 本地模拟流式输出欢迎语（不消耗token） */
async function simulateWelcome() {
  if (isStreaming.value || isWelcomePlaying.value) {
    console.debug('[TerminalPage]', '欢迎动画被并发调用拦截')
    return
  }
  isWelcomePlaying.value = true
  _welcomeAborted = false
  console.info('[TerminalPage]', '欢迎动画开始')
  try {
    let i = 0
    while (i < WELCOME_TEXT.length) {
      // 被用户终止
      if (_welcomeAborted) {
        console.info('[TerminalPage]', '欢迎动画被用户终止')
        break
      }
      const step = 1 + Math.floor(Math.random() * 3)
      const chunk = WELCOME_TEXT.slice(i, i + step)
      i += step
      const msgs = messages.value
      const last = msgs[msgs.length - 1]
      if (last && last.role === 'assistant') {
        last.content += chunk
        messages.value = [...msgs]
      }
      await new Promise(r => setTimeout(r, 15 + Math.random() * 25))
    }
  } finally {
    isWelcomePlaying.value = false
  }
}

// 消息变化 → 同步到对话 store（流式输出时节流至 500ms，减少 localStorage 写入）
let syncTimer: ReturnType<typeof setTimeout> | null = null
watch(messages, (msgs) => {
  // 排除仅含前端欢迎语（无 user 消息）的空对话，不持久化
  const hasRealMessages = msgs.some(m => m.role === 'user')
  if (!hasRealMessages) return

  if (isStreaming.value || isWelcomePlaying.value) {
    // 流式中：500ms 节流，最后一次变更后 500ms 才落盘
    if (syncTimer) clearTimeout(syncTimer)
    syncTimer = setTimeout(() => { conv.updateMessages(msgs); syncTimer = null }, 500)
  } else {
    // 非流式：立即同步
    if (syncTimer) { clearTimeout(syncTimer); syncTimer = null }
    conv.updateMessages(msgs)
  }
})

// 切换对话时加载消息
watch(() => conv.currentId.value, () => {
  loadConversation()
})

// ── 上传文件 ──

/** 统一附件模型 */
interface Attachment {
  id: number
  name: string
  type: 'image' | 'document'
  /** 图片 base64 dataUrl */
  dataUrl?: string
  /** 文档提取文本（展示在气泡卡片中，Markdown 格式） */
  text?: string
  /** 文档富 HTML（发给 AI 用，含表格/标题/格式） */
  richHtml?: string
  /** 文档内嵌图片（base64 编码，用于构造多模态消息） */
  richImages?: { name: string; mime: string; base64: string }[]
  /** 文件扩展名（用于图标映射） */
  ext: string
}

let _attachId = 0
const attachments = ref<Attachment[]>([])
const fileProcessing = ref(false)

/** 消息 ID → 附件元数据（用于在气泡中渲染文件卡片） */
const attachmentsByMsgId = ref<Record<number, { name: string; ext: string }[]>>({})

/** 可直接 readAsText 的文本格式 */
const TEXT_EXTS = new Set(['txt', 'md', 'csv', 'log', 'yml', 'yaml', 'toml', 'ini', 'cfg', 'conf', 'json', 'xml'])

/** 需后端解析的二进制办公文档 */
const OFFICE_EXTS = new Set(['docx', 'doc', 'pdf'])

function fileExt(f: File): string {
  return f.name.split('.').pop()?.toLowerCase() ?? ''
}

function fileIcon(ext: string): string {
  const map: Record<string, string> = {
    docx: '📄', doc: '📄', pdf: '📕',
    txt: '📝', md: '📝', csv: '📊',
    json: '📋', xml: '📋', log: '📋',
    yml: '⚙️', yaml: '⚙️', toml: '⚙️',
    ini: '⚙️', cfg: '⚙️', conf: '⚙️',
  }
  return map[ext] ?? '📎'
}

function handleFilePick() {
  fileInput.value?.click()
}

/** 小文件 IPC 上限：≤5MB 直接传 Buffer */
const MAX_IPC_FILE = 5 * 1024 * 1024
/** 大文件磁盘路径上限：5-50MB 写盘传路径 */
const MAX_DISK_FILE = 50 * 1024 * 1024

function onFilesSelected(e: Event) {
  const files = (e.target as HTMLInputElement).files
  if (!files) return
  for (const f of files) {
    // 大文件保护：超过 50MB 直接拒绝
    if (f.size > MAX_DISK_FILE) {
      alert(`文件 "${f.name}" 过大（${(f.size / 1024 / 1024).toFixed(1)} MB），上限 50 MB`)
      continue
    }
    const ext = fileExt(f)
    if (f.type.startsWith('image/')) {
      const reader = new FileReader()
      reader.onload = () => {
        attachments.value = [...attachments.value, {
          id: ++_attachId,
          name: f.name,
          type: 'image',
          dataUrl: reader.result as string,
          ext,
        }]
      }
      reader.readAsDataURL(f)
    } else if (OFFICE_EXTS.has(ext)) {
      handleOfficeFile(f, ext)
    } else if (TEXT_EXTS.has(ext) || f.type.startsWith('text/')) {
      const reader = new FileReader()
      reader.onload = () => {
        const text = (reader.result as string).slice(0, 8000)
        attachments.value = [...attachments.value, {
          id: ++_attachId,
          name: f.name,
          type: 'document',
          text,
          ext,
        }]
      }
      reader.readAsText(f)
    }
  }
  ;(e.target as HTMLInputElement).value = ''
}

/** 文档完整提取结果类型 */
interface DocExtractResult {
  html: string
  images: { name: string; mime: string; base64: string }[]
}

async function handleOfficeFile(f: File, ext: string) {
  fileProcessing.value = true
  let tempPath: string | null = null
  try {
    const buffer = await f.arrayBuffer()
    let html: string
    let images: { name: string; mime: string; base64: string }[]
    let plainText = ''

    if (f.size <= MAX_IPC_FILE) {
      // 小文件：走 IPC 直传
      const data = Array.from(new Uint8Array(buffer))
      const result = await invoke<DocExtractResult>('extract_document_full', { data, filename: f.name })
      html = result.html
      images = result.images
      // PDF 不再并行调第二个 Python 进程（避免双进程文件冲突导致闪退），
      // 改为从 HTML 输出中剥离纯文本
      if (ext === 'pdf') {
        plainText = html.replace(/<[^>]+>/g, '').trim().slice(0, 8000)
      } else {
        plainText = await invoke<string>('extract_office_text', { data, filename: f.name })
          .catch(() => html.replace(/<[^>]+>/g, '').trim().slice(0, 8000))
      }
    } else {
      // 大文件：写临时磁盘文件，传路径
      const tmpDir = await tempDir()
      tempPath = `${tmpDir}nahida-upload-${Date.now()}-${f.name}`
      await writeFile(tempPath, new Uint8Array(buffer))
      const result = await invoke<DocExtractResult>('extract_document_full_by_path', { path: tempPath, filename: f.name })
      html = result.html
      images = result.images
    }

    attachments.value = [...attachments.value, {
      id: ++_attachId,
      name: f.name,
      type: 'document',
      text: plainText,
      richHtml: html,
      richImages: images,
      ext,
    }]
  } catch (e) {
    attachments.value = [...attachments.value, {
      id: ++_attachId,
      name: f.name,
      type: 'document',
      text: `❌ 无法解析：${String(e)}`,
      ext,
    }]
  } finally {
    fileProcessing.value = false
    if (tempPath) {
      remove(tempPath).catch(() => {})
    }
  }
}

function removeAttachment(id: number) {
  attachments.value = attachments.value.filter(a => a.id !== id)
}

/** 发送消息（支持多模态 + 文件 + 搜索） */
async function sendMessage() {
  // 同步锁：在第一个 await 之前拦截并发点击，避免 async 间隙的竞争条件
  if (_sending) return
  _sending = true

  const input = chatInput.value
  if (!input) {
    _sending = false
    console.warn('[TerminalPage]', '发送失败：chatInput 模板引用未绑定')
    return
  }
  const text = input.value.trim()
  const hasAttachments = attachments.value.length > 0
  if (!text && !hasAttachments) {
    _sending = false
    return
  }

  if (isStreaming.value) {
    _sending = false
    console.warn('[TerminalPage]', '发送被拦截：isStreaming 为 true')
    return
  }

  try {
  // 终止欢迎动画（防止并发修改 messages 竞争）
  if (isWelcomePlaying.value) {
    _welcomeAborted = true
    isWelcomePlaying.value = false
  }

  if (!chatStore.provider) {
    console.warn('[TerminalPage]', 'provider 为空，无法发送')
    // 让 rawSend 内部捕获并显示 ❌ 错误消息（带 proper error handling）
    // 不 return——rawSend 会添加 user msg + assistant error 到消息列表
  }

  // ── 构建消息文本 ──
  // 用户气泡中只显示用户自己的文本，文件以卡片形式渲染在气泡上方
  const userBubbleText = text || ''

  // AI 收完整文本（含文件提取内容）
  let aiText = text
  const docAttachments = attachments.value.filter(a => a.type === 'document' && a.text)
  if (docAttachments.length > 0) {
    const fileSections = docAttachments.map(a => {
      // 有富 HTML 时使用 HTML 文本（表格、格式更完整）
      const content = a.richHtml || a.text!
      return `\n--- 文件: ${a.name} ---\n${content}`
    }).join('\n')
    aiText = (text ? text + '\n' : '') + fileSections
  }

  const imageAttachments = attachments.value.filter(a => a.type === 'image')

  // ── 收集文档内嵌图片（用于构造多模态消息）──
  const docImages: { name: string; mime: string; base64: string }[] = []
  for (const a of docAttachments) {
    if (a.richImages && a.richImages.length > 0) {
      docImages.push(...a.richImages)
    }
  }

  // 构建多模态消息（发给 AI）
  const contentParts: { type: 'text' | 'image_url'; text?: string; image_url?: { url: string } }[] = []
  if (aiText) contentParts.push({ type: 'text', text: aiText })
  // 用户上传的图片
  for (const img of imageAttachments) {
    contentParts.push({ type: 'image_url', image_url: { url: img.dataUrl! } })
  }
  // 文档内嵌图片（紧随文本后面，AI 可交叉理解）
  for (const img of docImages) {
    contentParts.push({ type: 'image_url', image_url: { url: `data:${img.mime};base64,${img.base64}` } })
  }

  const apiContent = contentParts.length === 1 && contentParts[0].type === 'text'
    ? aiText
    : contentParts as any

  // 发送前保存附件元数据，用于消息气泡中渲染文件卡片
  const attachMeta = attachments.value.map(a => ({ name: a.name, ext: a.ext }))

  // 预分配 msgId（在 rawSend 之前），确保附件卡片在消息渲染时立即可见
  const msgId = Date.now()
  if (attachMeta.length > 0) {
    attachmentsByMsgId.value = { ...attachmentsByMsgId.value, [msgId]: attachMeta }
  }

  console.info('[TerminalPage]', '发送消息')

  // 添加用户消息到 history（显示用 userBubbleText，AI 用 apiContent）
  try {
    await rawSend(apiContent, { displayContent: userBubbleText, msgId })
    console.info('[TerminalPage]', '发送成功')
  } catch (e) {
    const errStr = e instanceof Error ? e.message : String(e)
    console.error('[TerminalPage]', '发送消息异常', errStr)
  }

  } finally {
    _sending = false
  }
  input.value = ''
  attachments.value = []
}

/** 从 ChatMessage 中提取纯文本（用于渲染 Markdown） */
function msgContent(msg: ChatMessage): string {
  if (typeof msg.content === 'string') return msg.content
  return msg.content
    .filter((p) => p.type === 'text')
    .map((p) => p.text ?? '')
    .join('')
}

/** 滚动到底部（仅在用户未上翻时自动滚，流式输出不锁死浏览） */
function scrollToBottom(force = false) {
  nextTick(() => {
    const el = document.querySelector('.chat-messages')
    if (!el) return
    // 仅当用户在底部 80px 以内（或强制）时才自动滚
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 80
    if (force || atBottom) {
      el.scrollTop = el.scrollHeight
    }
  })
}

// 消息变化时自动滚底（仅当用户在底部附近）
watch(messages, () => scrollToBottom())

/** 键盘快捷键（仅处理全局快捷键，Enter 发送由 input 的 @keydown.enter 处理） */
function onKeydown(e: KeyboardEvent) {
  // Esc: 退出活跃态（流式中先中断）
  if (e.key === 'Escape') {
    if (isActive.value) {
      if (isStreaming.value) abortChat()
      deactivate()
    }
  }
}

async function activate() {
  if (isActive.value) return
  console.info('[TerminalPage]', '激活（睡眠→活跃）')
  // 纯淡入淡出过渡（opacity transition），无位移；聚焦输入框
  isActive.value = true
  await nextTick()
  chatInput.value?.focus()
}

function deactivate() {
  if (!isActive.value) return
  isActive.value = false
}

onMounted(() => {
  chatStore.initProvider()
  document.addEventListener('keydown', onKeydown)
})

onUnmounted(() => {
  document.removeEventListener('keydown', onKeydown)
})

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const k = 1024
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), units.length - 1)
  return (bytes / Math.pow(k, i)).toFixed(1) + ' ' + units[i]
}

const memoryPercent = computed(() => {
  if (!info.value || info.value.memory_total === 0) return 0
  return Number((info.value.memory_used / info.value.memory_total * 100).toFixed(1))
})

const vramPercent = computed(() => {
  if (!gpus.value.length || gpus.value[0].vram_total === 0) return 0
  return Number((gpus.value[0].vram_used / gpus.value[0].vram_total * 100).toFixed(1))
})

// 是否有真实显存占用数据（NVIDIA NVML 才有；Intel/AMD 回退方案只有总量）
const hasVramData = computed(() =>
  gpus.value.length > 0 && gpus.value[0].vram_total > 0 && gpus.value[0].vram_used > 0
)

// GPU 进度条百分比：优先显存使用率，否则用 GPU 使用率
const gpuPercent = computed(() => {
  if (!gpus.value.length) return 0
  return hasVramData.value ? vramPercent.value : gpus.value[0].usage_percent
})
</script>

<template>
  <div class="terminal-page" :class="{ idle: !isActive, active: isActive }">
    <!-- 头部 -->
    <div class="terminal-header">
      <div class="header-left">
        <h2 class="page-title">💭 虚空终端</h2>
      </div>
      <div class="header-right">
        <span class="hostname-tag">{{ info.hostname }}</span>
      </div>
    </div>

    <!-- 主内容（非阻塞：界面立即呈现，数据就绪后平滑淡入，避免“梦境编织”加载屏） -->
    <div class="terminal-body">
        <!-- ============ AI 主区域（双态切换） ============ -->
        <div class="ai-section">
          <!-- 沉睡态 -->
          <div class="idle-layout" :class="{ 'state-hidden': isActive }">
            <div class="ai-icon-area">
              <span class="ai-icon">💭</span>
              <div class="ai-icon-ring"></div>
            </div>
            <h3 class="ai-title">智慧对谈</h3>
            <p class="ai-desc">
              点击输入框开始对话
            </p>
            <div class="ai-input-placeholder" @click="activate">
              <span class="placeholder-text">输入消息……</span>
            </div>
          </div>

          <!-- 活跃态 -->
          <div class="active-layout" :class="{ 'state-visible': isActive }">
            <!-- 对话侧边栏 -->
            <div class="conv-panel">
              <div class="conv-panel-header">
                <span class="conv-panel-title">对话</span>
                <button class="chat-header-btn" @click="conv.create()" title="新建对话">+</button>
              </div>
              <div class="conv-list">
                <div
                  v-for="c in conv.sorted()"
                  :key="c.id"
                  class="conv-item"
                  :class="{ 'conv-item--active': c.id === conv.currentId.value }"
                  @click="conv.switchTo(c.id)"
                >
                  <span class="conv-item-title">{{ c.title }}</span>
                  <button
                    class="conv-item-del"
                    @click.stop="conv.remove(c.id)"
                    title="删除对话"
                  >×</button>
                </div>
              </div>
            </div>

            <!-- 聊天主体 — 无多余标题栏，更像真实对话 -->
            <div class="chat-area">
              <div class="chat-messages" ref="chatMessagesRef">
                <!-- 回睡眠态按钮（轻量，置于聊天区右上角） -->
                <button class="chat-collapse" @click="deactivate" title="返回睡眠态 (Esc)">
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                    <polyline points="18 15 12 9 6 15"/>
                  </svg>
                </button>
              <div
                v-for="(msg, i) in messages"
                :key="((msg as any)._msgId ?? i)"
                class="msg"
                :class="{
                  'msg-system': i === 0 && msg.role === 'assistant',
                  'msg-assistant': i > 0 && msg.role === 'assistant',
                  'msg-user': msg.role === 'user',
                  'msg-streaming': isStreaming && i === messages.length - 1 && msg.role === 'assistant',
                }"
                :style="{ '--msg-delay': `${Math.min(Math.max(0, messages.length - 1 - i) * 0.06, 0.8)}s` }"
              >
                <span v-if="msg.role === 'assistant'" class="msg-sender">🌿 纳西妲</span>
                <span v-else-if="msg.role === 'user'" class="msg-sender msg-sender-user">你</span>
                <!-- 文件卡片（DeepSeek 风格：附在消息文本上方） -->
                <div
                  v-if="msg.role === 'user' && attachmentsByMsgId[(msg as any)._msgId]?.length"
                  class="msg-attach-chips"
                >
                  <div
                    v-for="att in attachmentsByMsgId[(msg as any)._msgId]"
                    :key="att.name"
                    class="msg-attach-chip"
                  >
                    <span class="msg-attach-chip-icon">{{ fileIcon(att.ext) }}</span>
                    <span class="msg-attach-chip-name">{{ att.name }}</span>
                  </div>
                </div>
                <div class="msg-text" v-html="renderMarkdown(msgContent(msg))"></div>
                <button
                  v-if="i === messages.length - 1 && lastError && msg.role === 'assistant'"
                  class="msg-retry"
                  @click="retry()"
                >
                  🔄 重试
                </button>
              </div>
            </div>
            <div class="chat-input-bar">
              <!-- 文件解析中提示 -->
              <div v-if="fileProcessing" class="file-processing-hint">⏳ 正在解析文档…</div>
              <!-- 附件预览区 -->
              <div v-if="attachments.length" class="attach-previews">
                <div
                  v-for="a in attachments"
                  :key="a.id"
                  :class="a.type === 'image' ? 'attach-thumb' : 'attach-chip'"
                >
                  <!-- 图片缩略图 -->
                  <template v-if="a.type === 'image'">
                    <img :src="a.dataUrl" :alt="a.name" />
                    <button class="attach-remove" @click="removeAttachment(a.id)" title="移除">×</button>
                  </template>
                  <!-- 文档 Chip -->
                  <template v-else>
                    <span class="attach-chip-icon">{{ fileIcon(a.ext) }}</span>
                    <span class="attach-chip-name">{{ a.name }}</span>
                    <button class="attach-remove attach-remove--chip" @click="removeAttachment(a.id)" title="移除">×</button>
                  </template>
                </div>
              </div>

                <div class="chat-input-row">
                  <!-- 附件上传 -->
                  <button class="input-tool-btn" @click="handleFilePick" title="上传文件或图片">
                    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                      <path d="M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48"/>
                    </svg>
                  </button>
                  <input
                    ref="fileInput"
                    type="file"
                    accept="image/*,.pdf,.txt,.md,.json,.xml,.csv,.doc,.docx"
                    multiple
                    class="file-input-hidden"
                    @change="onFilesSelected"
                  />

                  <div class="chat-input-wrap">
                    <input
                      ref="chatInput"
                      class="chat-input"
                      placeholder="输入消息……"
                      :disabled="isStreaming"
                      @keydown.enter="sendMessage"
                    />
                  </div>

                  <button
                    class="chat-send"
                    :class="{ 'chat-send--stop': isStreaming }"
                    :disabled="!isStreaming && !chatInput?.value?.trim() && !attachments.length"
                    :title="isStreaming ? '停止生成' : '发送'"
                    @click="isStreaming ? abortChat() : sendMessage()"
                  >
                    {{ isStreaming ? '■' : '➤' }}
                  </button>
                </div>
              </div>
            </div>
          </div>
        </div>

        <!-- ============ 系统状态栏（结构立即完整呈现，GPU 数据到达后只有数值平滑变化） ============ -->
        <div class="sys-section" :class="{ 'sys-ready': !isLoading }">
          <div class="sys-bar">
            <div class="sys-item sys-info-item" title="操作系统">
              <span class="sys-icon">🖥️</span>
              <span class="sys-label">{{ info.os_name }} {{ info.os_version }}</span>
            </div>
            <span class="sys-divider"></span>

            <div class="sys-item" title="CPU 使用率">
              <span class="sys-icon">🧠</span>
              <div class="sys-bar-inner">
                <div class="sys-bar-track">
                  <div
                    class="sys-bar-fill cpu-fill"
                    :style="{ width: info.cpu_usage.toFixed(0) + '%' }"
                  ></div>
                </div>
                <span class="sys-pct">{{ info.cpu_usage.toFixed(0) }}%</span>
              </div>
            </div>
            <span class="sys-divider"></span>

            <div class="sys-item" title="内存使用率">
              <span class="sys-icon">🫧</span>
              <div class="sys-bar-inner">
                <div class="sys-bar-track">
                  <div
                    class="sys-bar-fill mem-fill"
                    :style="{ width: memoryPercent + '%' }"
                  ></div>
                </div>
                <span class="sys-pct">{{ memoryPercent }}%</span>
              </div>
            </div>
            <span class="sys-divider"></span>

            <!-- GPU 条常驻占位：采样未完成时先以 0% 占位，数据到达后条宽平滑过渡，不后补插入 -->
            <div class="sys-item" :title="hasVramData ? '显存使用率' : 'GPU 使用率'">
              <span class="sys-icon">🎮</span>
              <span class="sys-label gpu-name">{{ gpus.length ? gpus[0].name : 'GPU 采样中…' }}</span>
              <div class="sys-bar-inner">
                <div class="sys-bar-track">
                  <div
                    class="sys-bar-fill"
                    :class="hasVramData ? 'vram-fill' : 'gpu-fill'"
                    :style="{ width: gpuPercent + '%' }"
                  ></div>
                </div>
                <span class="sys-pct">{{ gpuPercent }}%</span>
              </div>
            </div>
          </div>

          <div class="sys-detail-hint">
            <span class="hint-label">CPU</span>
            <span class="hint-val">{{ info.cpu_name }}</span>
            <span class="hint-sep">·</span>
            <span class="hint-label">内存</span>
            <span class="hint-val">{{ formatBytes(info.memory_used) }} / {{ formatBytes(info.memory_total) }}</span>
            <span class="hint-sep">·</span>
            <span class="hint-label">GPU</span>
            <span class="hint-val">
              <template v-if="gpus.length">
                {{ gpus[0].name }}
                <template v-if="gpus[0].vram_total > 0">
                  {{ formatBytes(gpus[0].vram_used) }} / {{ formatBytes(gpus[0].vram_total) }}
                </template>
                <template v-if="gpus[0].usage_percent > 0">
                  {{ gpus[0].usage_percent }}%
                </template>
              </template>
              <template v-else>采样中…</template>
            </span>
          </div>
        </div>

        <!-- 虚空之眼 -->
        <div class="eye-bar">
          <span class="eye-text" :class="{ 'eye-awake': isActive }">
            {{ isActive ? '👁️ 虚空之眼已苏醒' : '👁️ 虚空之眼 · 自适应检测' }}
          </span>
        </div>
    </div>
  </div>

  <!-- ═══ 诊断面板 ═══ -->
  <button class="diag-toggle" @click="diagOpen = !diagOpen; if(diagOpen) diagRefresh()"
    :title="diagOpen ? '关闭诊断' : '打开诊断'">
    {{ diagOpen ? '✕' : '⚡' }}
  </button>

  <div v-if="diagOpen" class="diag-overlay" @click.self="diagOpen = false">
    <div class="diag-panel">
      <div class="diag-header">
        <strong>诊断日志</strong>
        <div class="diag-actions">
          <button @click="diagCopy">📋 复制</button>
          <button @click="(diag.clear(), diagRefresh())">🗑 清空</button>
          <button @click="diagRefresh">🔄 刷新</button>
        </div>
      </div>
      <div class="diag-body">
        <div v-if="!diagEvents.length" class="diag-empty">（无日志，请先发送一条消息）</div>
        <div v-for="ev in diagEvents" :key="ev.id" class="diag-line" :class="'cat-' + ev.cat.toLowerCase()">
          <span class="diag-time">{{ fmtDiagTime(ev.ts) }}</span>
          <span class="diag-cat">{{ ev.cat }}</span>
          <span class="diag-label">{{ ev.label }}</span>
          <span v-if="fmtDiagDetail(ev)" class="diag-detail">{{ fmtDiagDetail(ev) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.terminal-page {
  padding: 1.5rem 2rem;
  max-width: 60rem;
  margin: 0 auto;
  height: 100%;
  display: flex;
  flex-direction: column;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  text-rendering: optimizeLegibility;
  animation: fade-in 0.35s ease-out both;
}

/* ── 头部 ── */
.terminal-header {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  margin-bottom: 1.25rem;
  flex-shrink: 0;
}

.header-left {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.page-title {
  font-size: 1.25rem;
  font-weight: 700;
  margin: 0;
}



.header-right {
  flex-shrink: 0;
}

.hostname-tag {
  font-size: 0.75rem;
  color: var(--text-muted);
  background: var(--hover-bg);
  padding: 0.125rem 0.5rem;
  border-radius: 4px;
}

/* ── 主体 ── */
.terminal-body {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 1rem;
  min-height: 0;
}

/* ============================================
   AI 区域（双态容器）
   ============================================ */
.ai-section {
  flex: 1;
  position: relative;
  min-height: 0;
  overflow: hidden;
}

/* ── 沉睡态 ── */
.idle-layout {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  text-align: center;
  /* 退场时平滑淡出，避免元素突然消失 */
  transition: opacity 0.4s ease;
}

.idle-layout.state-hidden {
  opacity: 0;
  pointer-events: none;
}

.ai-icon-area {
  position: relative;
  display: inline-flex;
  margin-bottom: 1rem;
}

.ai-icon {
  font-size: 3rem;
  position: relative;
  z-index: 1;
  animation: idle-breathe 4s ease-in-out infinite;
}

@keyframes idle-breathe {
  0%, 100% { transform: scale(1); }
  50% { transform: scale(1.06); }
}

.ai-icon-ring {
  position: absolute;
  inset: -6px;
  border-radius: 50%;
  border: 2px solid rgba(120, 184, 104, 0.25);
  animation: ring-pulse 3s ease-in-out infinite;
}

@keyframes ring-pulse {
  0%, 100% { transform: scale(1); opacity: 0.25; }
  50% { transform: scale(1.15); opacity: 0.08; }
}

.ai-title {
  font-size: 1.25rem;
  font-weight: 700;
  margin: 0 0 0.5rem;
  color: var(--text-primary);
}

.ai-desc {
  font-size: 0.8125rem;
  color: var(--text-muted);
  line-height: 1.7;
  margin: 0 0 1.25rem;
  max-width: 20rem;
}

.ai-input-placeholder {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  height: 2.25rem;
  box-sizing: border-box;
  padding: 0.5rem 1rem;
  border: 1px solid rgba(128, 128, 128, 0.15);
  border-radius: 8px;
  background: var(--hover-bg);
  margin-bottom: 1rem;
  cursor: pointer;
  transition: border-color 0.3s ease, box-shadow 0.3s ease;
}

.ai-input-placeholder:hover {
  border-color: #78b868;
  box-shadow: 0 0 0 2px rgba(120, 184, 104, 0.15);
}

.placeholder-text {
  font-size: 0.8125rem;
  color: var(--text-muted);
  user-select: none;
}

/* ── 活跃态 ── */
.active-layout {
  position: absolute;
  inset: 0;
  display: flex;
  gap: 0;
  opacity: 0;
  pointer-events: none;
  /* 双态切换的淡入淡出过渡 */
  transition: opacity 0.4s ease;
}

/* 确保非活跃态时所有子元素都不拦截点击 */
.active-layout > * {
  pointer-events: none;
}

.active-layout.state-visible {
  opacity: 1;
  pointer-events: auto;
}

.active-layout.state-visible > * {
  pointer-events: auto;
}

/* 对话侧边栏 */
.conv-panel {
  width: 11rem;
  flex-shrink: 0;
  border-right: 1px solid rgba(128, 128, 128, 0.12);
  display: flex;
  flex-direction: column;
  overflow: hidden;
  opacity: 0;
  padding-right: 0.25rem;
}

.active-layout.state-visible .conv-panel {
  animation: fade-in 0.3s ease-out 0.2s both;
}

/* 聊天消息区延迟淡入，与共享元素位移形成层次感 */
.chat-messages {
  opacity: 0;
}

.active-layout.state-visible .chat-messages {
  animation: fade-in 0.35s ease-out 0.15s both;
}

.conv-panel-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.625rem 0.75rem;
  border-bottom: 1px solid rgba(128, 128, 128, 0.1);
  flex-shrink: 0;
}

.conv-panel-title {
  font-size: 0.75rem;
  font-weight: 600;
  color: var(--text-secondary);
}

.conv-list {
  flex: 1;
  overflow-y: auto;
  padding: 0.375rem 0;
}

/* ── 统一滚动条样式 ── */
.conv-list::-webkit-scrollbar,
.chat-messages::-webkit-scrollbar {
  width: 4px;
}

.conv-list::-webkit-scrollbar-track,
.chat-messages::-webkit-scrollbar-track {
  background: transparent;
}

.conv-list::-webkit-scrollbar-thumb,
.chat-messages::-webkit-scrollbar-thumb {
  background: rgba(128, 128, 128, 0.18);
  border-radius: 2px;
}

.conv-list::-webkit-scrollbar-thumb:hover,
.chat-messages::-webkit-scrollbar-thumb:hover {
  background: rgba(128, 128, 128, 0.3);
}

.conv-item {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.5rem 0.75rem;
  cursor: pointer;
  transition: background 0.15s;
  border-left: 2px solid transparent;
}

.conv-item:hover {
  background: var(--hover-bg);
}

.conv-item--active {
  background: rgba(120, 184, 104, 0.08);
  border-left-color: #78b868;
}

.conv-item-title {
  flex: 1;
  font-size: 0.6875rem;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  line-height: 1.3;
}

.conv-item-del {
  flex-shrink: 0;
  width: 1rem;
  height: 1rem;
  border: none;
  border-radius: 3px;
  background: transparent;
  color: var(--text-muted);
  font-size: 0.75rem;
  line-height: 1;
  cursor: pointer;
  display: none;
  align-items: center;
  justify-content: center;
  padding: 0;
}

.conv-item:hover .conv-item-del {
  display: flex;
}

.conv-item-del:hover {
  background: rgba(224, 72, 72, 0.12);
  color: #e04848;
}

/* 聊天主体区 */
.chat-area {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  position: relative;
}

/* 活跃态独占元素 — 在沉睡页中无对应物 */
.chat-send {
  opacity: 0;
}

.active-layout.state-visible .chat-send {
  animation: fade-in-up 0.3s ease-out 0.40s both;
}

@keyframes fade-in-up {
  from { opacity: 0; transform: translateY(6px); }
  to   { opacity: 1; transform: translateY(0); }
}

/* 对话区顶部/底部分隔线从透明过渡到可见 */
.chat-input-bar {
  border-top: 1px solid transparent;
  transition: border-top-color 0.3s ease-out 0.40s;
}
.active-layout.state-visible .chat-input-bar {
  border-top-color: rgba(128, 128, 128, 0.1);
}

@keyframes fade-in {
  from { opacity: 0; }
  to { opacity: 1; }
}

/* ── 折叠按钮（聊天区右上角，浮动轻量图标）── */
.chat-collapse {
  position: absolute;
  top: 0.375rem;
  right: 0.375rem;
  z-index: 5;
  background: rgba(128, 128, 128, 0.08);
  border: 1px solid transparent;
  cursor: pointer;
  width: 1.75rem;
  height: 1.75rem;
  padding: 0;
  border-radius: 50%;
  display: flex;
  align-items: center;
  justify-content: center;
  color: var(--text-muted);
  opacity: 0;
  transition: opacity 0.25s ease-out, background 0.2s, border-color 0.2s, color 0.2s;
  font-family: inherit;
  flex-shrink: 0;
}

.chat-collapse:hover {
  background: rgba(120, 184, 104, 0.15);
  border-color: rgba(120, 184, 104, 0.3);
  color: #78b868;
}

.active-layout.state-visible .chat-collapse {
  opacity: 1;
  animation: fade-in 0.3s ease-out 0.60s both;
}

/* 对话下拉（已废弃，由侧边栏替代） */

/* 头部小按钮 */
.chat-header-btn {
  flex-shrink: 0;
  width: 1.5rem;
  height: 1.5rem;
  border: 1px solid rgba(128, 128, 128, 0.12);
  border-radius: 4px;
  background: var(--bg-color);
  color: var(--text-muted);
  font-size: 1rem;
  line-height: 1;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.15s, border-color 0.15s;
  padding: 0;
  font-family: inherit;
}

.chat-header-btn:hover {
  color: #78b868;
  border-color: #78b868;
}

.chat-messages {
  flex: 1;
  overflow-y: auto;
  padding: 2rem 0.875rem 0.75rem;
  display: flex;
  flex-direction: column;
  gap: 0.875rem;
  scroll-behavior: smooth;
}

.msg {
  max-width: min(85%, 720px);
  word-break: break-word;
  overflow-wrap: break-word;
}

/* ── 所有消息初始隐藏，由 .state-visible 触发逐条生长动画 ── */
.msg-system,
.msg-assistant,
.msg-user {
  opacity: 0;
  transform: translateY(16px) scale(0.94);
}

/* 动画触发：仅当活跃态容器可见时，逐条 cascade 生长 */
.active-layout.state-visible .msg-system,
.active-layout.state-visible .msg-assistant,
.active-layout.state-visible .msg-user {
  animation: msg-sprout 0.50s cubic-bezier(0.16, 1, 0.3, 1) both;
  animation-delay: var(--msg-delay, 0s);
}

/* 布局 */
.msg-system { align-self: flex-start; }
.msg-assistant { align-self: flex-start; }
.msg-user { align-self: flex-end; }

.msg-user .msg-text {
  background: rgba(120, 184, 104, 0.12);
  border-radius: 8px;
  border-top-right-radius: 2px;
}

.msg-sender-user {
  text-align: right;
  color: var(--text-muted);
}

/* ── 消息气泡内文件卡片（DeepSeek 风格）── */
.msg-attach-chips {
  display: flex;
  flex-wrap: wrap;
  gap: 0.375rem;
  margin-bottom: 0.375rem;
  justify-content: flex-end;
}

.msg-attach-chip {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.25rem 0.5rem;
  background: rgba(128, 128, 128, 0.05);
  border: 1px solid rgba(128, 128, 128, 0.12);
  border-radius: 6px;
  font-size: 0.75rem;
  max-width: 14rem;
}

.msg-attach-chip-icon {
  font-size: 0.875rem;
  flex-shrink: 0;
}

.msg-attach-chip-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-secondary);
}

/* ── 流式消息（无光标，逐字出现即是进度）── */

/* ── 重试按钮 ── */
.msg-retry {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  margin-top: 0.375rem;
  padding: 0.25rem 0.625rem;
  border: 1px solid rgba(128, 128, 128, 0.12);
  border-radius: 6px;
  background: var(--hover-bg);
  color: var(--text-secondary);
  font-size: 0.75rem;
  cursor: pointer;
  transition: background 0.2s ease, border-color 0.2s ease;
  font-family: inherit;
}

.msg-retry:hover {
  background: rgba(120, 184, 104, 0.12);
  border-color: #78b868;
  color: #78b868;
}

/* ── 消息入场动画：草芽萌发式生长 ── */
@keyframes msg-sprout {
  0% {
    opacity: 0;
    transform: translateY(16px) scale(0.94);
  }
  60% {
    opacity: 0.85;
    transform: translateY(-2px) scale(1.01);
  }
  100% {
    opacity: 1;
    transform: translateY(0) scale(1);
  }
}

.msg-sender {
  font-size: 0.8125rem;
  color: #78b868;
  font-weight: 600;
  display: block;
  margin-bottom: 0.25rem;
}

.msg-text {
  font-size: 0.9375rem;
  color: var(--text-primary);
  line-height: 1.75;
  margin: 0;
  padding: 0.625rem 0.875rem;
  background: var(--hover-bg);
  border-radius: 8px;
  border-top-left-radius: 2px;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  text-rendering: optimizeLegibility;
  word-break: normal;
  overflow-wrap: break-word;
}

/* ── 消息内 Markdown 子元素 ── */
.msg-text :deep(p) {
  margin: 0 0 0.5rem;
}

.msg-text :deep(p:last-child) {
  margin-bottom: 0;
}

.msg-text :deep(pre) {
  margin: 0.5rem 0;
  padding: 0.625rem 0.75rem;
  background: rgba(0, 0, 0, 0.15);
  border-radius: 6px;
  overflow-x: auto;
  font-size: 0.8125rem;
  line-height: 1.5;
}

.msg-text :deep(pre code) {
  background: none;
  padding: 0;
  font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', 'Consolas', monospace;
}

.msg-text :deep(code) {
  font-family: 'Cascadia Code', 'Fira Code', 'JetBrains Mono', 'Consolas', monospace;
  font-size: 0.8125rem;
  background: rgba(0, 0, 0, 0.1);
  padding: 0.125rem 0.325rem;
  border-radius: 4px;
}

.msg-text :deep(strong) {
  font-weight: 700;
  color: var(--text-primary);
}

.msg-text :deep(em) {
  font-style: italic;
}

/* ── 标题 ── */
.msg-text :deep(h1) { font-size: 1.25rem; font-weight: 700; margin: 0.75rem 0 0.375rem; color: var(--text-primary); }
.msg-text :deep(h2) { font-size: 1.125rem; font-weight: 700; margin: 0.625rem 0 0.375rem; color: var(--text-primary); }
.msg-text :deep(h3) { font-size: 1.0625rem; font-weight: 600; margin: 0.5rem 0 0.25rem; color: var(--text-primary); }
.msg-text :deep(h4),
.msg-text :deep(h5),
.msg-text :deep(h6) { font-size: 0.9375rem; font-weight: 600; margin: 0.375rem 0 0.125rem; color: var(--text-primary); }

/* ── 列表 ── */
.msg-text :deep(ul),
.msg-text :deep(ol) {
  margin: 0.25rem 0;
  padding-left: 1.5rem;
}
.msg-text :deep(li) {
  margin-bottom: 0.125rem;
  line-height: 1.65;
}
.msg-text :deep(li:last-child) { margin-bottom: 0; }

/* ── 引用 ── */
.msg-text :deep(blockquote) {
  margin: 0.375rem 0;
  padding: 0.375rem 0.75rem;
  border-left: 3px solid rgba(120, 184, 104, 0.5);
  background: rgba(120, 184, 104, 0.06);
  border-radius: 0 6px 6px 0;
}
.msg-text :deep(blockquote p) {
  margin: 0;
  color: var(--text-secondary);
}

/* ── 链接 ── */
.msg-text :deep(a) {
  color: #78b868;
  text-decoration: underline;
  text-underline-offset: 2px;
}
.msg-text :deep(a:hover) {
  color: #98cc88;
}

/* ── 图片 ── */
.msg-text :deep(img) {
  max-width: 100%;
  border-radius: 6px;
  margin: 0.25rem 0;
}

/* ── 分割线 ── */
.msg-text :deep(hr) {
  border: none;
  border-top: 1px solid rgba(128, 128, 128, 0.15);
  margin: 0.5rem 0;
}

/* ── 表格 ── */
.msg-text :deep(table) {
  margin: 0.5rem 0;
  border-collapse: collapse;
  width: 100%;
  font-size: 0.875rem;
}
.msg-text :deep(th),
.msg-text :deep(td) {
  padding: 0.375rem 0.625rem;
  border: 1px solid rgba(128, 128, 128, 0.2);
  text-align: left;
}
.msg-text :deep(th) {
  background: rgba(128, 128, 128, 0.08);
  font-weight: 600;
}
.msg-text :deep(tr:nth-child(even) td) {
  background: rgba(128, 128, 128, 0.03);
}

/* ── 删除线 ── */
.msg-text :deep(s),
.msg-text :deep(del) {
  text-decoration: line-through;
  opacity: 0.7;
}

.chat-input-bar {
  flex-shrink: 0;
  display: flex;
  flex-direction: column;
  gap: 0.625rem;
  padding: 1rem 0.25rem 0.25rem;
  border-top: 1px solid transparent;
}

/* 输入行（工具按钮 + 输入框 + 发送） */
.chat-input-row {
  display: flex;
  align-items: center;
  gap: 0.625rem;
}

/* 工具按钮 */
.input-tool-btn {
  flex-shrink: 0;
  width: 2.25rem;
  height: 2.25rem;
  border: 1px solid rgba(128, 128, 128, 0.12);
  border-radius: 8px;
  background: rgba(128, 128, 128, 0.04);
  color: var(--text-muted);
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: color 0.2s ease, border-color 0.2s ease, background 0.2s ease, transform 0.15s ease;
  padding: 0;
  box-sizing: border-box;
}

.input-tool-btn:hover {
  color: #78b868;
  border-color: rgba(120, 184, 104, 0.35);
  background: rgba(120, 184, 104, 0.08);
  transform: scale(1.05);
}

.input-tool-btn:active {
  transform: scale(0.95);
}

.input-tool-btn--active {
  color: #78b868;
  border-color: rgba(120, 184, 104, 0.45);
  background: rgba(120, 184, 104, 0.12);
}

.file-input-hidden {
  display: none;
}

/* 文件解析提示 */
.file-processing-hint {
  font-size: 0.75rem;
  color: #78b868;
  padding: 0.25rem 0;
  animation: fade-in 0.2s ease-out, hint-pulse 1.2s ease-in-out infinite 0.2s;
}

@keyframes hint-pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.5; }
}

/* ── 附件预览区 ── */
.attach-previews {
  display: flex;
  gap: 0.5rem;
  flex-wrap: wrap;
  align-items: flex-start;
}

.attach-previews > * {
  animation: fade-in-up 0.25s ease-out both;
}

/* 图片缩略图 */
.attach-thumb {
  position: relative;
  width: 3rem;
  height: 3rem;
  border-radius: 6px;
  overflow: hidden;
  border: 1px solid rgba(128, 128, 128, 0.12);
  flex-shrink: 0;
}

.attach-thumb img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

/* 文档 Chip 卡片 */
.attach-chip {
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 0.375rem;
  padding: 0.375rem 1.5rem 0.375rem 0.625rem;
  background: rgba(128, 128, 128, 0.06);
  border: 1px solid rgba(128, 128, 128, 0.15);
  border-radius: 8px;
  height: 2.5rem;
  box-sizing: border-box;
  flex-shrink: 0;
  max-width: 14rem;
}

.attach-chip-icon {
  font-size: 1rem;
  line-height: 1;
  flex-shrink: 0;
}

.attach-chip-name {
  font-size: 0.75rem;
  color: var(--text-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 移除按钮（通用） */
.attach-remove {
  position: absolute;
  top: -3px;
  right: -3px;
  width: 1.125rem;
  height: 1.125rem;
  border: none;
  border-radius: 50%;
  background: rgba(0,0,0,0.55);
  color: #fff;
  font-size: 0.625rem;
  line-height: 1;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 0;
  transition: background 0.15s;
}

.attach-remove:hover {
  background: rgba(220, 40, 40, 0.8);
}

/* Chip 上的移除按钮微调 */
.attach-remove--chip {
  top: 50%;
  right: 0.25rem;
  transform: translateY(-50%);
}

.chat-input-wrap {
  position: relative;
  flex: 1;
  height: 2.25rem;
}

.chat-input {
  width: 100%;
  height: 100%;
  padding: 0.5rem 0.75rem;
  border: 1px solid rgba(128, 128, 128, 0.15);
  border-radius: 8px;
  background: var(--bg-color);
  color: var(--text-primary);
  font-size: 0.875rem;
  font-weight: 400;
  font-family: inherit;
  outline: none;
  caret-color: #78b868;
  transition: border-color 0.3s ease, box-shadow 0.3s ease;
  box-sizing: border-box;
}

.chat-input:focus {
  border-color: #78b868;
  box-shadow: 0 0 0 2px rgba(120, 184, 104, 0.15);
}

.chat-input::placeholder {
  color: var(--text-muted);
}

.chat-send {
  width: 2.25rem;
  height: 2.25rem;
  background: #78b868;
  border: none;
  border-radius: 8px;
  color: #fff;
  font-size: 1rem;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  transition: background 0.2s ease, transform 0.2s ease, opacity 0.25s ease-out;
  flex-shrink: 0;
  padding: 0;
  line-height: 1;
  box-sizing: border-box;
}

.chat-send:hover {
  background: #62a050;
  transform: scale(1.05);
}

.chat-send:active {
  transform: scale(0.95);
}

/* 停止按钮样式 */
.chat-send--stop {
  background: #e04848;
  animation: stop-pulse 1.5s ease-in-out infinite;
}

.chat-send--stop:hover {
  background: #c03030;
  animation: none;
}

@keyframes stop-pulse {
  0%, 100% { box-shadow: 0 0 0 0 rgba(224, 72, 72, 0.4); }
  50% { box-shadow: 0 0 0 5px rgba(224, 72, 72, 0); }
}

/* 禁用状态 */
.chat-send:disabled {
  opacity: 0.4;
  cursor: not-allowed;
  transform: none;
}

/* ============================================
   系统状态栏
   ============================================ */
.sys-section {
  flex-shrink: 0;
  /* 数据就绪前隐藏，就绪后平滑淡入（避免突然弹出） */
  opacity: 0;
  transform: translateY(4px);
  transition: opacity 0.45s ease, transform 0.45s ease;
}

.sys-section.sys-ready {
  opacity: 1;
  transform: translateY(0);
}

.sys-bar {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.625rem 0.875rem;
  background: var(--hover-bg);
  border-radius: 8px;
  flex-wrap: wrap;
}

.sys-item {
  display: flex;
  align-items: center;
  gap: 0.375rem;
  flex-shrink: 0;
}

.sys-info-item {
  max-width: 10rem;
}

.sys-icon {
  font-size: 0.875rem;
  flex-shrink: 0;
}

.sys-label {
  font-size: 0.8125rem;
  color: var(--text-secondary);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.sys-divider {
  width: 1px;
  height: 1rem;
  background: rgba(128, 128, 128, 0.18);
  flex-shrink: 0;
}

/* ── 状态条内进度条 ── */
.sys-bar-inner {
  display: flex;
  align-items: center;
  gap: 0.375rem;
}

.sys-bar-track {
  width: 5rem;
  height: 6px;
  background: rgba(128, 128, 128, 0.15);
  border-radius: 3px;
  overflow: hidden;
  flex-shrink: 0;
}

.gpu-name {
  max-width: 15rem;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.sys-bar-fill {
  height: 100%;
  border-radius: 3px;
  transition: width 0.5s ease;
}

.cpu-fill { background: #78b868; }
.mem-fill { background: #98cc88; }
.vram-fill { background: #62a050; }
.gpu-fill { background: #88c478; }

.sys-pct {
  font-size: 0.75rem;
  color: var(--text-secondary);
  min-width: 2.2em;
  text-align: right;
  font-variant-numeric: tabular-nums;
}

/* ── 详细数据提示 ── */
.sys-detail-hint {
  margin-top: 0.375rem;
  padding: 0 0.875rem;
  display: flex;
  align-items: center;
  gap: 0.25rem;
  flex-wrap: wrap;
  font-size: 0.75rem;
}

.hint-label {
  color: var(--text-muted);
}

.hint-val {
  color: var(--text-secondary);
}

.hint-sep {
  color: rgba(128, 128, 128, 0.2);
  margin: 0 0.125rem;
}

/* ── 虚空之眼 ── */
.eye-bar {
  flex-shrink: 0;
  text-align: center;
  padding-top: 0.25rem;
}

.eye-text {
  font-size: 0.75rem;
  color: var(--text-muted);
  transition: color 0.5s ease;
}

.eye-awake {
  color: #78b868;
}

/* ═══ 诊断面板 ═══ */
.diag-toggle {
  position: fixed;
  bottom: 1rem;
  right: 1rem;
  z-index: 9999;
  width: 2rem;
  height: 2rem;
  border-radius: 50%;
  border: 1px solid var(--border-color, #555);
  background: var(--bg-secondary, #2a2a2a);
  color: var(--text-muted, #999);
  font-size: 0.875rem;
  cursor: pointer;
  opacity: 0.5;
  transition: opacity 0.2s;
  line-height: 1;
}
.diag-toggle:hover {
  opacity: 1;
}
.diag-overlay {
  position: fixed;
  inset: 0;
  z-index: 9998;
  background: rgba(0,0,0,0.5);
  display: flex;
  align-items: center;
  justify-content: center;
}
.diag-panel {
  background: var(--bg-primary, #1e1e1e);
  border: 1px solid var(--border-color, #444);
  border-radius: 0.5rem;
  width: min(90vw, 700px);
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.diag-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.5rem 0.75rem;
  border-bottom: 1px solid var(--border-color, #444);
}
.diag-header button {
  background: var(--bg-secondary, #333);
  border: 1px solid var(--border-color, #555);
  border-radius: 0.25rem;
  padding: 0.2rem 0.5rem;
  color: var(--text-primary, #ddd);
  cursor: pointer;
  font-size: 0.75rem;
}
.diag-header button:hover {
  background: var(--bg-tertiary, #444);
}
.diag-actions {
  display: flex;
  gap: 0.375rem;
}
.diag-body {
  padding: 0.75rem;
  margin: 0;
  overflow: auto;
  font-family: 'Cascadia Code', 'Fira Code', monospace;
  font-size: 0.7rem;
  line-height: 1.5;
  color: var(--text-primary, #ccc);
}
.diag-empty {
  color: var(--text-muted, #888);
}
.diag-line {
  display: flex;
  gap: 0.5rem;
  padding: 0.125rem 0;
  border-bottom: 1px dashed var(--border-color, #333);
}
.diag-line:last-child {
  border-bottom: none;
}
.diag-time {
  flex: none;
  color: var(--text-muted, #888);
}
.diag-cat {
  flex: none;
  width: 3.25rem;
  text-align: center;
  border-radius: 3px;
  background: var(--hover-bg, #333);
  color: var(--text-muted, #999);
}
/* 分类着色：错误红 / 发送绿 / API 蓝 / 其余默认 */
.cat-error .diag-cat {
  background: rgba(224, 92, 92, 0.15);
  color: #e05c5c;
}
.cat-send .diag-cat {
  background: rgba(120, 184, 104, 0.15);
  color: #78b868;
}
.cat-api .diag-cat {
  background: rgba(92, 148, 224, 0.15);
  color: #5c94e0;
}
.cat-error .diag-label {
  color: #e05c5c;
}
.diag-label {
  flex: none;
  max-width: 45%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.diag-detail {
  color: var(--text-muted, #999);
  word-break: break-all;
}
</style>
