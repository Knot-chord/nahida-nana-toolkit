/**
 * AI 设置持久化服务
 *
 * 职责：
 * - 读写 localStorage 中的 AI 配置
 * - 提供响应式配置对象（reactive），修改后自动持久化
 * - 与 SettingsPage 的 AI 配置面板对接
 */

import { reactive, ref } from 'vue'
import { DEFAULT_AI_CONFIG } from '@nahida-nana/shared'
import type { AIProviderConfig } from '@nahida-nana/shared'

const STORAGE_KEY = 'nahida-ai-config'

/** 数值字段净化：非有限数（NaN/历史脏数据）回退默认值，避免配置页 toFixed 崩溃 */
function numOr(v: unknown, fallback: number): number {
  return typeof v === 'number' && Number.isFinite(v) ? v : fallback
}

/** 从 localStorage 加载配置，缺失/非法字段用默认值填补 */
function loadConfig(): AIProviderConfig {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return { ...DEFAULT_AI_CONFIG }
    const parsed = JSON.parse(raw) as Partial<AIProviderConfig>
    return {
      apiKey: parsed.apiKey ?? DEFAULT_AI_CONFIG.apiKey,
      baseUrl: parsed.baseUrl ?? DEFAULT_AI_CONFIG.baseUrl,
      model: parsed.model ?? DEFAULT_AI_CONFIG.model,
      temperature: numOr(parsed.temperature, DEFAULT_AI_CONFIG.temperature),
      maxTokens: Math.max(0, Math.round(numOr(parsed.maxTokens, DEFAULT_AI_CONFIG.maxTokens))),
      topP: numOr(parsed.topP, DEFAULT_AI_CONFIG.topP),
    }
  } catch {
    return { ...DEFAULT_AI_CONFIG }
  }
}

/** 全局唯一的响应式配置对象 */
const config = reactive<AIProviderConfig>(loadConfig())

/** 持久化当前配置到 localStorage */
function saveConfig(): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(config))
}

/** 获取响应式 AI 配置（修改后调用 saveConfig 持久化） */
export function getAIConfig(): AIProviderConfig {
  return config
}

/** 保存并持久化配置 */
export function saveAIConfig(): void {
  saveConfig()
}

/** 重置为默认配置 */
export function resetAIConfig(): void {
  Object.assign(config, { ...DEFAULT_AI_CONFIG })
  saveConfig()
}

/** 是否已配置 API Key（可切换 Cloud/Mock Provider） */
export function hasAPIKey(): boolean {
  return config.apiKey.trim().length > 0
}

// ═══════════════════════════════════════════════════
//  会话 Token 用量追踪
// ═══════════════════════════════════════════════════

const TOKEN_STORAGE_KEY = 'nahida-token-usage'

interface TokenUsage {
  inputTokens: number
  outputTokens: number
  /** 本次会话预估费用（美元） */
  estimatedCost: number
}

/** 本次会话累计 Token 用量 */
const tokenUsage = ref<TokenUsage>(loadTokenUsage())

function loadTokenUsage(): TokenUsage {
  try {
    const raw = localStorage.getItem(TOKEN_STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<TokenUsage>
      return {
        inputTokens: parsed.inputTokens ?? 0,
        outputTokens: parsed.outputTokens ?? 0,
        estimatedCost: parsed.estimatedCost ?? 0,
      }
    }
  } catch { /* ignore */ }
  return { inputTokens: 0, outputTokens: 0, estimatedCost: 0 }
}

function saveTokenUsage(): void {
  localStorage.setItem(TOKEN_STORAGE_KEY, JSON.stringify(tokenUsage.value))
}

/** 获取当前会话 Token 用量 */
export function getTokenUsage(): Readonly<TokenUsage> {
  return tokenUsage.value
}

/** 追加 Token 用量（每次 API 调用后调用，含费用估算） */
export function addTokenUsage(input: number, output: number, costUsd?: number): void {
  tokenUsage.value = {
    inputTokens: tokenUsage.value.inputTokens + input,
    outputTokens: tokenUsage.value.outputTokens + output,
    estimatedCost: tokenUsage.value.estimatedCost + (costUsd ?? 0),
  }
  saveTokenUsage()
}

/** 重置 Token 用量 */
export function resetTokenUsage(): void {
  tokenUsage.value = { inputTokens: 0, outputTokens: 0, estimatedCost: 0 }
  saveTokenUsage()
}

// ═══════════════════════════════════════════════════
//  模型定价表（USD / 1M tokens）
// ═══════════════════════════════════════════════════

/** 内置常见模型价格（美元/百万Token），用于费用估算 */
const MODEL_PRICES: Record<string, { input: number; output: number }> = {
  'gpt-4o': { input: 2.50, output: 10.00 },
  'gpt-4o-mini': { input: 0.15, output: 0.60 },
  'gpt-4-turbo': { input: 10.00, output: 30.00 },
  'gpt-4': { input: 30.00, output: 60.00 },
  'gpt-3.5-turbo': { input: 0.50, output: 1.50 },
  'o1': { input: 15.00, output: 60.00 },
  'o1-mini': { input: 3.00, output: 12.00 },
  'o3-mini': { input: 1.10, output: 4.40 },
  'deepseek-chat': { input: 0.27, output: 1.10 },
  'deepseek-reasoner': { input: 0.55, output: 2.19 },
  'qwen-plus': { input: 0.40, output: 1.20 },
  'qwen-max': { input: 2.40, output: 9.60 },
  'qwen-turbo': { input: 0.30, output: 0.60 },
  'glm-4-flash': { input: 0.01, output: 0.01 },
  'glm-4-plus': { input: 0.79, output: 0.79 },
  'claude-3.5-sonnet': { input: 3.00, output: 15.00 },
  'claude-3-haiku': { input: 0.25, output: 1.25 },
  'gemini-2.0-flash': { input: 0.10, output: 0.40 },
  'gemini-1.5-pro': { input: 1.25, output: 5.00 },
}

/** 根据模型名查找价格（返回 input/output 美元/百万Token），未知模型返回 null */
export function getModelPrice(model: string): { input: number; output: number } | null {
  // 精确匹配
  if (MODEL_PRICES[model]) return MODEL_PRICES[model]
  // 模糊匹配（如 gpt-4o-2024-08-06 → gpt-4o）
  for (const [key, price] of Object.entries(MODEL_PRICES)) {
    if (model.startsWith(key)) return price
  }
  return null
}

/** 根据 Token 数和模型价格计算费用（美元） */
export function calcTokenCost(
  inputTokens: number,
  outputTokens: number,
  model: string,
): number {
  const price = getModelPrice(model)
  if (!price) return 0
  return (inputTokens / 1_000_000) * price.input + (outputTokens / 1_000_000) * price.output
}

// ═══════════════════════════════════════════════════
//  预算配置
// ═══════════════════════════════════════════════════

const BUDGET_STORAGE_KEY = 'nahida-budget-config'

export interface BudgetConfig {
  /** 总预算（美元），0 表示不限制 */
  totalBudget: number
  /** 货币符号 */
  currency: string
}

function loadBudgetConfig(): BudgetConfig {
  try {
    const raw = localStorage.getItem(BUDGET_STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw) as Partial<BudgetConfig>
      return {
        totalBudget: parsed.totalBudget ?? 5,
        currency: parsed.currency ?? 'CNY',
      }
    }
  } catch { /* ignore */ }
  return { totalBudget: 5, currency: 'CNY' }
}

const budgetConfig = reactive<BudgetConfig>(loadBudgetConfig())

function saveBudgetConfig(): void {
  localStorage.setItem(BUDGET_STORAGE_KEY, JSON.stringify(budgetConfig))
}

export function getBudgetConfig(): BudgetConfig {
  return budgetConfig
}

export function persistBudgetConfig(): void {
  saveBudgetConfig()
}
