/**
 * useChatStream — 流式聊天状态机（替代 use-chat.ts）
 *
 * 职责：
 * - 管理消息列表、流式状态、错误状态
 * - 封装 provider.chat() 调用（从 chatStore 获取 provider）
 * - 提供 send / abort / retry 操作
 * - 支持文件内容注入（独立消息字段，不污染对话显示）
 * - 支持 Skills 渐进式披露（L1 索引注入 → L2 按需加载）
 *
 * 设计原则：
 * - Provider 单例由 chatStore 管理，不再通过回调注入
 * - 流式输出期间 isStreaming=true，UI 据此显示打字动画
 * - 错误统一由 cloud-provider 映射为用户友好文案
 * - 流式期间不锁滚动（由 TerminalPage 自行处理 scroll 行为）
 */

import { ref, shallowRef } from 'vue'
import type { ChatMessage, ChatContentPart } from '@nahida-nana/shared'
import { useChatStore } from '../stores/chat'
import { getEnabledSkillsPrompt } from './skill-runner'
import { detectSkillRequests, stripSkillMarkers, formatSkillContext } from './load-skill'
import { diag } from './__diagnostic'

export interface ChatError {
  message: string
  retryable: boolean
}

export function useChatStream() {
  const chatStore = useChatStore()

  // ===== 状态 =====

  let _msgIdCounter = 0

  const messages = shallowRef<ChatMessage[]>([])
  const isStreaming = ref(false)
  const lastError = ref<ChatError | null>(null)
  let abortCtrl: AbortController | null = null
  /** 标记本次中止是否由用户主动触发，用于区分「用户取消」与「意外中止」 */
  let _userAborted = false
  /** onChunk 计数器（用于减少诊断日志频率） */
  let _onChunkCount = 0

  // ===== onChunk 批量合并（减少渲染次数）=====
  let pendingBatch = ''
  let batchRaf: ReturnType<typeof requestAnimationFrame> | null = null

  function flushChunk(): void {
    batchRaf = null
    if (!pendingBatch) return
    const msgs = messages.value
    const last = msgs[msgs.length - 1]
    if (last && last.role === 'assistant') {
      last.content += pendingBatch
      messages.value = [...msgs]
    }
    pendingBatch = ''
  }

  // ===== 操作 =====

  /**
   * 发送消息
   *
   * @param content - 发往 AI 的完整内容（含文件文本等）
   * @param opts.displayContent - 可选，消息历史中存储的显示用内容（不含文件全文）
   * @param opts.fileContent - 可选，文档解析后的全文（注入 AI 上下文但不显示）
   * @param opts.searchContext - 可选，搜索结果文本（注入 AI 上下文）
   */
  async function send(
    content: string | ChatContentPart[],
    opts?: {
      displayContent?: string | ChatContentPart[]
      fileContent?: string
      searchContext?: string
      msgId?: number
    },
  ): Promise<number> {
    const sendKey = diag.sendStart('send()', {
      hasOpts: !!opts,
      contentLen: typeof content === 'string' ? content.length : `parts:${content.length}`,
      isStreaming: isStreaming.value,
    })

    const display = opts?.displayContent ?? content
    const textContent = typeof display === 'string' ? (display as string).trim() : ''
    if (!textContent && typeof display !== 'string') {
      // 纯多模态（无文本）允许发送
    } else if (!textContent && typeof content === 'string' && !(content as string).trim()) {
      return 0
    }
    if (isStreaming.value) return 0

    lastError.value = null

    // 消息 ID
    const msgId = opts?.msgId ?? ++_msgIdCounter
    if (opts?.msgId != null && opts.msgId > _msgIdCounter) {
      _msgIdCounter = opts.msgId
    }

    const userMsg: ChatMessage & { _msgId: number } = {
      role: 'user',
      content: typeof display === 'string' ? (display as string).trim() : display,
      _msgId: msgId,
    }
    // 显示历史：始终用干净内容
    const displayHistory = [...messages.value, userMsg]
    messages.value = displayHistory

    // 构建发往 AI 的历史：将最后一条 user 消息的 content 替换为完整 AI 内容
    // 注意：排除前端欢迎语（新对话中 role:assistant 的本地欢迎词），不发送给 AI
    const originalMessages = messages.value.slice(0, -1) // 去掉刚追加的 userMsg
    const firstRealUserIdx = originalMessages.findIndex(m => m.role === 'user')
    const relevantHistory = firstRealUserIdx >= 0
      ? originalMessages.slice(firstRealUserIdx)
      : [] // 新对话：仅含前端欢迎语，全部排除

    let aiHistory: ChatMessage[]
    if (content !== display) {
      const aiUserMsg: ChatMessage = {
        role: 'user',
        content: typeof content === 'string' ? content.trim() : content,
      }
      aiHistory = [...relevantHistory, aiUserMsg]
    } else {
      aiHistory = [...relevantHistory, userMsg]
    }

    // 注入已启用的 Skills 系统提示
    const skillsPrompt = getEnabledSkillsPrompt()
    if (skillsPrompt) {
      const systemMsg: ChatMessage = { role: 'system', content: skillsPrompt }
      aiHistory = [systemMsg, ...aiHistory]
    }

    // 追加占位 assistant 消息
    const assistantMsg: ChatMessage & { _msgId: number } = {
      role: 'assistant',
      content: '',
      _msgId: ++_msgIdCounter,
    }
    messages.value = [...displayHistory, assistantMsg]

    isStreaming.value = true
    abortCtrl = new AbortController()
    _userAborted = false

    diag.log('STATE', 'abortCtrl init', {
      aborted: abortCtrl.signal.aborted,
      signalRef: 'abortCtrl',
    })

    try {
      const provider = chatStore.provider
      diag.log('STATE', 'provider check', {
        hasProvider: !!provider,
        providerName: (provider as any)?.name,
      })

      // 诊断：确认 abortCtrl 在调用 provider.chat 前状态正常
      if (abortCtrl?.signal.aborted) {
        diag.log('STATE', 'abortCtrl 异常 — 创建后已被中止', {
          _userAborted,
        })
      }
      if (!provider) {
        diag.error('provider is null', '请先配置 API Key')
        throw new Error('请先配置 API Key（工具箱管理 → AI 配置）')
      }

      const fullText = await provider.chat(
        aiHistory,
        (delta) => {
          // 每 8 块记录一次诊断，减少缓冲区溢出
          _onChunkCount++
          if (_onChunkCount % 8 === 1) {
            diag.log('RENDER', 'onChunk', { len: delta.length, idx: _onChunkCount })
          }
          pendingBatch += delta
          if (batchRaf === null) {
            batchRaf = requestAnimationFrame(flushChunk)
          }
        },
        abortCtrl.signal,
      )

      // 流结束后汇总日志
      diag.log('RENDER', 'onChunk end', { totalChunks: _onChunkCount })
      _onChunkCount = 0

      // 流结束后同步刷新 batch，防止 rAF 异步延迟导致下一个消息收到残留内容
      if (batchRaf !== null) {
        cancelAnimationFrame(batchRaf)
        batchRaf = null
      }
      flushChunk()

      // 确保最终内容完整
      let finalText = fullText
      const msgs = messages.value
      const last = msgs[msgs.length - 1]
      if (last && last.role === 'assistant') {
        last.content = fullText
        messages.value = [...msgs]
      }

      // ── Skills 渐进式披露：检测 {{USE_SKILL:id}} 标记 ──
      const skillRequests = detectSkillRequests(fullText)
      const loadedSkills = skillRequests.filter(s => s.content !== null)

      if (loadedSkills.length > 0 && !_userAborted && abortCtrl?.signal.aborted === false) {
        diag.log('STATE', 'skill_load_request', {
          count: loadedSkills.length,
          ids: loadedSkills.map(s => s.id),
        })

        // 清除标记后的文本（移除 {{USE_SKILL:xxx}} 标记）
        const cleanedText = stripSkillMarkers(fullText)

        // 构建后续请求：注入完整技能内容
        const skillContext = formatSkillContext(loadedSkills)
        const followUpHistory: ChatMessage[] = [
          ...aiHistory,
          { role: 'assistant', content: fullText },
          {
            role: 'system',
            content: `${skillContext}\n\n请基于以上技能指令，重新生成完整回复。`,
          },
        ]

        // 更新 UI：显示清理后的文本 + 加载提示
        const currentMsgs = messages.value
        const currentLast = currentMsgs[currentMsgs.length - 1]
        if (currentLast && currentLast.role === 'assistant') {
          currentLast.content = cleanedText + '\n\n*🌱 正在加载技能指令...*'
          messages.value = [...currentMsgs]
        }

        try {
          const followUpText = await provider.chat(
            followUpHistory,
            (delta) => {
              pendingBatch += delta
              if (batchRaf === null) {
                batchRaf = requestAnimationFrame(flushChunk)
              }
            },
            abortCtrl.signal,
          )

          // 同步刷新 batch
          if (batchRaf !== null) {
            cancelAnimationFrame(batchRaf)
            batchRaf = null
          }
          flushChunk()

          // 用后续请求的完整回复替换 assistant 消息
          finalText = followUpText
          const finalMsgs = messages.value
          const finalLast = finalMsgs[finalMsgs.length - 1]
          if (finalLast && finalLast.role === 'assistant') {
            finalLast.content = followUpText
            messages.value = [...finalMsgs]
          }

          diag.log('STATE', 'skill_load_complete', {
            followUpLen: followUpText.length,
          })
        } catch (followUpErr: unknown) {
          // 后续请求失败：保留清理后的文本，附加错误提示
          if (batchRaf !== null) {
            cancelAnimationFrame(batchRaf)
            batchRaf = null
          }
          pendingBatch = ''
          const errMsg = followUpErr instanceof Error ? followUpErr.message : String(followUpErr)
          const failMsgs = messages.value
          const failLast = failMsgs[failMsgs.length - 1]
          if (failLast && failLast.role === 'assistant') {
            failLast.content = cleanedText + `\n\n❌ 技能加载失败：${errMsg}`
            messages.value = [...failMsgs]
          }
          diag.error('skill follow-up failed', followUpErr)
        }
      }

      diag.sendEnd(sendKey, 'chat() ok', { fullTextLen: finalText.length })
    } catch (e: unknown) {
      diag.error('chat() catch', e)
      // 清除 batch，防止残留内容写到下一条消息
      if (batchRaf !== null) {
        cancelAnimationFrame(batchRaf)
        batchRaf = null
      }
      pendingBatch = ''
      const errMsg = e instanceof Error ? e.message : String(e)
      const isAbort = errMsg === '已取消' || (e as DOMException)?.name === 'AbortError'

      if (isAbort && _userAborted) {
        // 用户主动取消 → 回滚消息
        messages.value = displayHistory
      } else if (isAbort) {
        // 意外中止（非用户触发）→ 显示错误而非静默回滚
        lastError.value = { message: errMsg, retryable: true }
        const msgs = messages.value
        const last = msgs[msgs.length - 1]
        if (last && last.role === 'assistant') {
          last.content = `❌ 连接中断：${errMsg}`
          messages.value = [...msgs]
        }
      } else {
        lastError.value = { message: errMsg, retryable: true }
        const msgs = messages.value
        const last = msgs[msgs.length - 1]
        if (last && last.role === 'assistant') {
          last.content = `❌ ${errMsg}`
          messages.value = [...msgs]
        }
      }
    } finally {
      isStreaming.value = false
      abortCtrl = null
    }

    return msgId
  }

  /** 取消当前请求 */
  function abort(): void {
    _userAborted = true
    abortCtrl?.abort()
  }

  /** 重试最后一条消息 */
  async function retry(): Promise<void> {
    if (isStreaming.value) return

    const msgs = messages.value
    const last = msgs[msgs.length - 1]
    if (!last) return

    if (last.role === 'assistant') {
      messages.value = msgs.slice(0, -1)
    }

    const updatedMsgs = messages.value
    const lastUser = updatedMsgs[updatedMsgs.length - 1]
    if (lastUser && lastUser.role === 'user') {
      await send(lastUser.content)
    }
  }

  /** 清空对话 */
  function clear(): void {
    abort()
    messages.value = []
    lastError.value = null
  }

  return {
    messages,
    isStreaming,
    lastError,
    send,
    abort,
    retry,
    clear,
  }
}
