/**
 * 对话管理
 *
 * 职责：
 * - 管理多个对话会话（创建、切换、删除）
 * - localStorage 持久化
 * - 提供响应式当前对话状态
 */

import { ref, shallowRef } from 'vue'
import type { ChatMessage } from '@nahida-nana/shared'

export interface Conversation {
  id: string
  title: string
  messages: ChatMessage[]
  createdAt: number
  updatedAt: number
}

const STORAGE_KEY = 'nahida-chat-conversations'

/** 欢迎语（新对话预设，本地流式不消耗 token；人格不内置于此，由 Skills 常驻注入实现） */
export const WELCOME_TEXT = '虚空终端已连接。\n\n想聊点什么，或者有什么要我帮忙的？随时开口就好。'

/** 生成短 ID */
function genId(): string {
  return Date.now().toString(36) + Math.random().toString(36).slice(2, 6)
}

/** 从第一条用户消息自动生成标题 */
function autoTitle(messages: ChatMessage[]): string {
  const firstUser = messages.find((m) => m.role === 'user')
  if (!firstUser) return '新对话'
  const text = typeof firstUser.content === 'string'
    ? firstUser.content
    : firstUser.content.find((p) => p.type === 'text')?.text ?? ''
  return text.slice(0, 30) + (text.length > 30 ? '…' : '')
}

/** 加载所有对话 */
function loadAll(): Conversation[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return []
    return JSON.parse(raw) as Conversation[]
  } catch {
    return []
  }
}

/** 持久化 */
function saveAll(list: Conversation[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list))
}

// ─── 响应式状态 ───

const list = ref<Conversation[]>(loadAll())
const currentId = ref<string | null>(list.value[0]?.id ?? null)

/** 当前对话 */
const current = shallowRef<Conversation | null>(list.value[0] ?? null)

/**
 * 对话列表（自然序：新对话在最前，切换对话不改变顺序）。
 * 不按 updatedAt 排序，避免切换对话时视觉上"动了但好像没动"——
 * 参考 DeepSeek 网页端：列表保持创建顺序，只更新活跃态高亮。
 */
const sorted = () => list.value

// ─── 操作 ───

/** 当前对话是否有实质内容（有用户消息才算） */
function hasContent(conv: Conversation | null): boolean {
  if (!conv) return false
  return conv.messages.some((m) => m.role === 'user')
}

/** 创建新对话（若当前为空则不重复创建） */
function create(): Conversation {
  // 若当前对话无任何用户消息，直接复用
  if (current.value && !hasContent(current.value)) {
    return current.value
  }
  const conv: Conversation = {
    id: genId(),
    title: '新对话',
    messages: [],
    createdAt: Date.now(),
    updatedAt: Date.now(),
  }
  list.value = [conv, ...list.value]
  currentId.value = conv.id
  current.value = conv
  saveAll(list.value)
  return conv
}

/** 切换到指定对话 */
function switchTo(id: string): void {
  const conv = list.value.find((c) => c.id === id)
  if (!conv) return
  currentId.value = id
  current.value = conv
}

/** 删除对话 */
function remove(id: string): void {
  list.value = list.value.filter((c) => c.id !== id)
  if (currentId.value === id) {
    const next = list.value[0] ?? null
    currentId.value = next?.id ?? null
    current.value = next
  }
  saveAll(list.value)
}

/** 更新当前对话消息（同时更新 title 和 updatedAt） */
function updateMessages(messages: ChatMessage[]): void {
  if (!current.value) return
  current.value.messages = messages
  current.value.title = autoTitle(messages)
  current.value.updatedAt = Date.now()
  // 同步到 list
  const idx = list.value.findIndex((c) => c.id === current.value!.id)
  if (idx >= 0) {
    list.value[idx] = { ...current.value }
  }
  saveAll(list.value)
}

/** 清空所有对话 */
function clearAll(): void {
  list.value = []
  currentId.value = null
  current.value = null
  saveAll([])
}

export function useConversations() {
  return {
    list,
    currentId,
    current,
    sorted,
    create,
    switchTo,
    remove,
    updateMessages,
    clearAll,
  }
}
