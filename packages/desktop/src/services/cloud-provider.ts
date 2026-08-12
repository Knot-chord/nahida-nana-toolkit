/**
 * CloudProvider — OpenAI 兼容云端 AI
 *
 * 职责：
 * - 实现 AIProvider 接口
 * - 调用 OpenAI 兼容 Chat Completions API（POST /chat/completions）
 * - 解析 SSE (Server-Sent Events) 流式响应
 * - 统一错误码映射为用户友好提示
 * - 请求超时保护（30s）
 *
 * 支持所有 OpenAI 兼容接口：
 * - OpenAI 官方
 * - DeepSeek
 * - 通义千问 (Qwen)
 * - 其他兼容代理/网关
 */

import type { AIProvider, ChatMessage, CancelSignal, AIProviderConfig } from '@nahida-nana/shared'
import { getAIConfig, addTokenUsage, calcTokenCost } from './ai-settings'

/** 请求总超时（连接 + 响应首字节 + 流式传输），默认 30s */
const REQUEST_TIMEOUT_MS = 30_000

/** 根据 HTTP 状态码和响应体映射用户友好错误消息 */
function mapError(status: number, body: string): string {
  if (status === 401 || status === 403) {
    return 'API Key 无效或已过期，请在设置中检查'
  }
  if (status === 429) {
    return '请求过于频繁，请稍后重试'
  }
  if (status >= 500) {
    return 'AI 服务暂时不可用，请稍后重试'
  }
  // 尝试从响应体中提取错误信息
  try {
    const err = JSON.parse(body)
    const msg = err.error?.message ?? ''
    if (msg) {
      // 模型不支持 / 不存在
      if (/model.*not.*found|does not exist|invalid.*model|no.*model/i.test(msg)) {
        return `模型不可用：${msg}\n请在「工具箱管理 → AI 配置」中检查模型名称是否正确，或切换其他模型`
      }
      // 余额不足
      if (/insufficient.*(balance|quota|fund)|billing|payment/i.test(msg)) {
        return `账户余额不足或欠费：${msg}`
      }
      return msg
    }
  } catch { /* ignore */ }
  return `请求失败 (HTTP ${status})`
}

/**
 * 创建融合了外部 cancel signal 和内部超时的 AbortSignal。
 * 当外部 signal 触发或超时时间到达时，返回的 signal 都会被 abort。
 * 返回 controller 用于在请求完成后清理超时 timer。
 */
function createTimeoutSignal(
  externalSignal?: CancelSignal,
  timeoutMs: number = REQUEST_TIMEOUT_MS,
): { signal: AbortSignal; controller: AbortController; timer: ReturnType<typeof setTimeout> } {
  const controller = new AbortController()

  // 监听外部取消
  if (externalSignal) {
    if (externalSignal.aborted) {
      controller.abort()
    } else {
      externalSignal.addEventListener('abort', () => controller.abort(), { once: true })
    }
  }

  // 超时
  const timer = setTimeout(() => controller.abort(), timeoutMs)

  // 请求完成后清理 timer（无论成功/失败）
  const cleanup = () => clearTimeout(timer)
  controller.signal.addEventListener('abort', cleanup, { once: true })

  return { signal: controller.signal, controller, timer }
}

/** 创建 CloudProvider 实例 */
export function createCloudProvider(): AIProvider {
  return {
    name: '云端 (OpenAI 兼容)',

    async chat(
      messages: ChatMessage[],
      onChunk: (delta: string) => void,
      signal?: CancelSignal,
    ): Promise<string> {
      const cfg: AIProviderConfig = getAIConfig()

      if (!cfg.apiKey.trim()) {
        throw new Error('请先在设置中配置 API Key')
      }

      const url = `${cfg.baseUrl.replace(/\/+$/, '')}/chat/completions`

      const bodyObj: Record<string, unknown> = {
        model: cfg.model,
        messages,
        temperature: cfg.temperature,
        top_p: cfg.topP,
        stream: true,
        stream_options: { include_usage: true },
      }
      // maxTokens = 0 表示不限制，不发送该参数
      if (cfg.maxTokens > 0) {
        bodyObj.max_tokens = cfg.maxTokens
      }

      const body = JSON.stringify(bodyObj)

      // 创建带超时的 signal
      const { signal: timeoutSignal, timer: timeoutTimer } = createTimeoutSignal(signal)

      let response: Response
      try {
        response = await fetch(url, {
          method: 'POST',
          headers: {
            'Content-Type': 'application/json',
            'Authorization': `Bearer ${cfg.apiKey}`,
          },
          body,
          signal: timeoutSignal,
        })
      } catch (e: unknown) {
        if (e instanceof DOMException && e.name === 'AbortError') {
          // 区分超时和用户取消
          if (signal?.aborted) {
            throw new Error('已取消')
          }
          throw new Error('请求超时，请重试')
        }
        // TypeError 通常是网络错误（fetch 无法到达目标）
        throw new Error('网络连接失败，请检查网络后重试')
      } finally {
        // 只清理超时 timer，不清除 signal（避免后续 response body stream 被意外取消）
        clearTimeout(timeoutTimer)
      }

      // 非 2xx → 读取错误体后抛出
      if (!response.ok) {
        let errBody = ''
        try { errBody = await response.text() } catch { /* ignore */ }
        throw new Error(mapError(response.status, errBody))
      }

      // 读取 SSE 流
      if (!response.body) {
        throw new Error('AI 服务返回了空响应')
      }

      const reader = response.body.getReader()
      const decoder = new TextDecoder()
      let fullText = ''
      let buffer = ''
      let usage: { prompt_tokens?: number; completion_tokens?: number } | null = null

      try {
        while (true) {
          // 支持取消
          if (signal?.aborted) {
            reader.cancel()
            throw new Error('已取消')
          }

          const { done, value } = await reader.read()
          if (done) break

          buffer += decoder.decode(value, { stream: true })

          // 按行解析 SSE
          const lines = buffer.split('\n')
          buffer = lines.pop() ?? ''

          for (const line of lines) {
            const trimmed = line.trim()
            if (!trimmed || trimmed.startsWith(':')) continue

            if (trimmed === 'data: [DONE]') {
              // 流结束
              const inputTokens = usage?.prompt_tokens ?? 0
              const outputTokens = usage?.completion_tokens ?? 0
              const cost = calcTokenCost(inputTokens, outputTokens, cfg.model)
              addTokenUsage(inputTokens, outputTokens, cost)
              return fullText
            }

            if (trimmed.startsWith('data: ')) {
              const jsonStr = trimmed.slice(6)
              try {
                const parsed = JSON.parse(jsonStr)
                const delta = parsed.choices?.[0]?.delta?.content
                if (delta) {
                  fullText += delta
                  onChunk(delta)
                }
                // 捕获 usage 信息（stream_options.include_usage 启用的 provider 会在最后 chunk 返回）
                if (parsed.usage) {
                  usage = parsed.usage
                }
              } catch {
                // 跳过无法解析的行
              }
            }
          }
        }
      } catch (e: unknown) {
        // 流读取阶段的 AbortError：区分用户取消和意外中断
        if ((e as DOMException)?.name === 'AbortError' && signal?.aborted) {
          throw new Error('已取消')
        }
        if ((e as DOMException)?.name === 'AbortError') {
          throw new Error('连接中断，请重试')
        }
        throw e
      } finally {
        reader.releaseLock()
      }

      // 正常结束（无 [DONE] 标记时）
      const inputTokens = usage?.prompt_tokens ?? 0
      const outputTokens = usage?.completion_tokens ?? 0
      const cost = calcTokenCost(inputTokens, outputTokens, cfg.model)
      addTokenUsage(inputTokens, outputTokens, cost)
      return fullText
    },
  }
}

