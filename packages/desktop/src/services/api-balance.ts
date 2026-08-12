/**
 * API 余额查询服务
 *
 * 职责：
 * - 测试 API Key 是否有效（调用 /models）
 * - 查询各平台账户余额 / 订阅状态
 * - 区分按量付费 vs 订阅制，订阅制不显示余额数字
 */

import type { AIProviderConfig } from '@nahida-nana/shared'
import {
  matchPlatform,
  type ApiPlatform,
  type BalanceResult,
} from './api-platforms'

/** API 响应共用 headers */
function authHeaders(apiKey: string): HeadersInit {
  return {
    Authorization: `Bearer ${apiKey}`,
    'Content-Type': 'application/json',
  }
}

/** 安全 fetch，超时 10s，非 2xx 抛异常 */
async function safeFetch(url: string, init: RequestInit): Promise<Response> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), 10_000)
  try {
    const res = await fetch(url, { ...init, signal: controller.signal })
    if (!res.ok) {
      const text = await res.text().catch(() => '')
      throw new Error(`HTTP ${res.status}: ${text.slice(0, 200)}`)
    }
    return res
  } finally {
    clearTimeout(timer)
  }
}

// ═══════════════════════════════════════════════════════════
//  连接测试
// ═══════════════════════════════════════════════════════════

/** 测试 API Key 是否有效（调用 /models） */
async function testConnection(config: AIProviderConfig): Promise<{ ok: boolean; models?: string[] }> {
  const base = config.baseUrl.replace(/\/+$/, '')
  try {
    const res = await safeFetch(`${base}/models`, {
      method: 'GET',
      headers: authHeaders(config.apiKey),
    })
    const data = await res.json() as { data?: { id: string }[] }
    const models = data.data?.map((m) => m.id) ?? []
    return { ok: true, models }
  } catch (e) {
    return { ok: false }
  }
}

// ═══════════════════════════════════════════════════════════
//  各平台余额解析
// ═══════════════════════════════════════════════════════════

async function queryDeepSeek(config: AIProviderConfig, platform: ApiPlatform): Promise<BalanceResult> {
  const base = platform.baseUrl.replace(/\/+$/, '')
  const res = await safeFetch(`${base}/user/balance`, {
    method: 'GET',
    headers: authHeaders(config.apiKey),
  })
  const data = await res.json() as {
    is_available: boolean
    balance_infos?: { currency: string; total_balance: string; granted_balance: string; topped_up_balance: string }[]
  }
  const info = data.balance_infos?.[0]
  return {
    platformName: platform.name,
    billingType: 'prepaid',
    balance: info?.total_balance,
    grantedBalance: info?.granted_balance,
    toppedUpBalance: info?.topped_up_balance,
    currency: info?.currency ?? 'CNY',
    connected: true,
  }
}

async function querySiliconFlow(config: AIProviderConfig, platform: ApiPlatform): Promise<BalanceResult> {
  const base = platform.baseUrl.replace(/\/+$/, '')
  const res = await safeFetch(`${base}/user/info`, {
    method: 'GET',
    headers: authHeaders(config.apiKey),
  })
  const data = await res.json() as {
    status: boolean
    data?: { balance: string; chargeBalance: string; totalBalance: string; status: string }
  }
  return {
    platformName: platform.name,
    billingType: 'prepaid',
    balance: data.data?.totalBalance ?? data.data?.balance,
    toppedUpBalance: data.data?.chargeBalance,
    status: data.data?.status,
    currency: 'CNY',
    connected: true,
  }
}

async function queryMoonshot(config: AIProviderConfig, platform: ApiPlatform): Promise<BalanceResult> {
  const base = platform.baseUrl.replace(/\/+$/, '')
  const res = await safeFetch(`${base}/users/me/balance`, {
    method: 'GET',
    headers: authHeaders(config.apiKey),
  })
  const data = await res.json() as {
    data?: { total_balance?: number; granted_balance?: number; topped_up_balance?: number }
  }
  return {
    platformName: platform.name,
    billingType: 'prepaid',
    balance: data.data?.total_balance?.toString(),
    grantedBalance: data.data?.granted_balance?.toString(),
    toppedUpBalance: data.data?.topped_up_balance?.toString(),
    currency: 'CNY',
    connected: true,
  }
}

async function queryOpenRouter(config: AIProviderConfig, platform: ApiPlatform): Promise<BalanceResult> {
  const base = platform.baseUrl.replace(/\/+$/, '')
  const res = await safeFetch(`${base}/auth/key`, {
    method: 'GET',
    headers: authHeaders(config.apiKey),
  })
  const data = await res.json() as {
    data?: { credits?: number; usage?: number; is_free_tier?: boolean; rate_limit?: { requests: number } }
  }
  return {
    platformName: platform.name,
    billingType: 'prepaid',
    balance: data.data?.credits?.toString(),
    currency: 'USD',
    connected: true,
  }
}

/** OpenAI 余额查询（Billing API，需要组织级 Key，不可靠） */
async function queryOpenAI(config: AIProviderConfig, platform: ApiPlatform): Promise<BalanceResult> {
  const base = platform.baseUrl.replace(/\/+$/, '')
  // 1) 查订阅
  let hardLimit = 0
  try {
    const subRes = await safeFetch(`${base}/dashboard/billing/subscription`, {
      method: 'GET',
      headers: authHeaders(config.apiKey),
    })
    const subData = await subRes.json() as { hard_limit_usd?: number }
    hardLimit = subData.hard_limit_usd ?? 0
  } catch {
    // 非组织 Key 可能 403
    hardLimit = 0
  }

  // 2) 查当月用量
  let totalUsed = 0
  try {
    const now = new Date()
    const start = new Date(now.getFullYear(), now.getMonth(), 1).toISOString().split('T')[0]
    const end = now.toISOString().split('T')[0]
    const useRes = await safeFetch(
      `${base}/dashboard/billing/usage?start_date=${start}&end_date=${end}`,
      { method: 'GET', headers: authHeaders(config.apiKey) },
    )
    const useData = await useRes.json() as { total_usage?: number }
    totalUsed = (useData.total_usage ?? 0) / 100 // 美分转美元
  } catch {
    totalUsed = 0
  }

  return {
    platformName: platform.name,
    billingType: 'prepaid',
    hardLimit: hardLimit > 0 ? hardLimit : undefined,
    totalUsed: totalUsed > 0 ? totalUsed : undefined,
    currency: 'USD',
    connected: hardLimit > 0 || totalUsed > 0,
  }
}

// ═══════════════════════════════════════════════════════════
//  公开 API
// ═══════════════════════════════════════════════════════════

/** 连通性 + 余额综合查询 */
export async function checkBalance(config: AIProviderConfig): Promise<BalanceResult> {
  const platform = matchPlatform(config.baseUrl)

  // 无平台匹配 → 仅测通
  if (!platform) {
    const { ok } = await testConnection(config)
    return {
      platformName: '自定义',
      billingType: 'prepaid',
      connected: ok,
      error: ok ? undefined : '连接失败，请检查 API 地址和 Key',
    }
  }

  // 先测通
  const { ok: connected } = await testConnection(config)
  if (!connected) {
    return {
      platformName: platform.name,
      billingType: platform.billingType,
      connected: false,
      error: '连接失败，请检查 API Key',
    }
  }

  // 订阅制 → 只显示状态
  if (platform.billingType === 'subscription') {
    return {
      platformName: platform.name,
      billingType: 'subscription',
      connected: true,
    }
  }

  // 按平台查询余额
  try {
    switch (platform.id) {
      case 'deepseek':
        return await queryDeepSeek(config, platform)
      case 'siliconflow':
        return await querySiliconFlow(config, platform)
      case 'moonshot':
        return await queryMoonshot(config, platform)
      case 'openrouter':
        return await queryOpenRouter(config, platform)
      case 'openai':
        return await queryOpenAI(config, platform)
      default:
        // 无余额接口 → 仅返回连接正常
        return {
          platformName: platform.name,
          billingType: platform.billingType,
          connected: true,
        }
    }
  } catch (e) {
    return {
      platformName: platform.name,
      billingType: platform.billingType,
      connected: true,
      error: `余额查询失败：${String(e)}`,
    }
  }
}

/**
 * 格式化余额为可读字符串
 * 订阅制返回 "订阅有效"，余额未知返回 "连接正常"
 */
export function formatBalance(result: BalanceResult | null): string {
  if (!result) return ''
  if (result.error && !result.connected) return `❌ ${result.error}`
  if (result.billingType === 'subscription') return '📦 订阅有效（不限用量）'

  const parts: string[] = []
  if (result.balance) {
    const symbol = result.currency === 'USD' ? '$' : '¥'
    parts.push(`余额 ${symbol}${result.balance}`)
  }
  if (result.grantedBalance) parts.push(`赠送 ¥${result.grantedBalance}`)
  if (result.toppedUpBalance && result.toppedUpBalance !== result.balance) {
    parts.push(`充值 ¥${result.toppedUpBalance}`)
  }
  if (parts.length === 0 && result.connected) return '✅ 连接正常'
  return parts.join(' · ')
}
