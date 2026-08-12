/**
 * chatStore — AI 聊天全局状态
 *
 * 职责：
 * - 管理 AIProvider 实例生命周期（创建/重建/销毁）
 * - 暴露 provider 供 use-chat-stream composable 使用
 * - 不管理消息列表（消息由 composable 管理，store 只管 provider）
 */

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { AIProvider } from '@nahida-nana/shared'
import { createCloudProvider } from '../services/cloud-provider'
import { hasAPIKey } from '../services/ai-settings'

export const useChatStore = defineStore('chat', () => {
  /** 当前 AI provider 实例 */
  const provider = ref<AIProvider | null>(null)

  /** 是否已初始化 provider */
  const hasProvider = computed(() => provider.value !== null)

  /** 初始化 provider（应用启动时调用一次） */
  function initProvider(): void {
    if (hasAPIKey()) {
      provider.value = createCloudProvider()
    }
  }

  /** 配置变更后重建 provider */
  function rebuildProvider(): void {
    if (hasAPIKey()) {
      provider.value = createCloudProvider()
    } else {
      provider.value = null
    }
  }

  /** 清除 provider（API Key 被清空时） */
  function clearProvider(): void {
    provider.value = null
  }

  return {
    provider,
    hasProvider,
    initProvider,
    rebuildProvider,
    clearProvider,
  }
})
