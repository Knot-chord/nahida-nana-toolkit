/**
 * API 平台预设
 *
 * 职责：
 * - 定义主流 API 平台的 Base URL、默认模型列表
 * - 定义各平台的余额查询方式（Endpoint + 解析器）
 * - 根据 Base URL 反查平台、获取推荐模型
 */

/** 计费模式 */
export type BillingType = 'prepaid' | 'subscription'

/** 余额查询结果 */
export interface BalanceResult {
  /** 平台名称 */
  platformName: string
  /** 计费模式 */
  billingType: BillingType
  /** 可用余额（字符串，保留原始精度） */
  balance?: string
  /** 赠送余额 */
  grantedBalance?: string
  /** 充值余额 */
  toppedUpBalance?: string
  /** 货币单位（CNY / USD） */
  currency?: string
  /** 本月已用额度（USD，仅 OpenAI） */
  totalUsed?: number
  /** 硬限额（USD，仅 OpenAI） */
  hardLimit?: number
  /** 账户状态 */
  status?: string
  /** 连接是否正常（仅测通） */
  connected?: boolean
  /** 原始错误信息 */
  error?: string
}

/** 平台定义 */
export interface ApiPlatform {
  id: string
  name: string
  baseUrl: string
  /** 推荐模型列表（仅预设平台有） */
  models: string[]
  /** 计费模式 */
  billingType: BillingType

  // ── 余额查询配置 ──
  /** 余额查询方式 */
  balanceMethod: 'none' | 'endpoint' | 'openai_billing'
  /** 余额查询端点（相对 baseUrl） */
  balanceEndpoint?: string
  /** 是否需要版本路径（如 /v1/user/info → balanceEndpoint 写 /user/info，versionPrefix 为 /v1） */
  versionPrefix?: string
}

/** 所有预设平台 */
export const API_PLATFORMS: ApiPlatform[] = [
  {
    id: 'openai',
    name: 'OpenAI',
    baseUrl: 'https://api.openai.com/v1',
    models: ['gpt-4o-mini', 'gpt-4o', 'gpt-4.1-mini', 'gpt-4.1', 'o4-mini', 'o3-mini', 'gpt-4-turbo'],
    billingType: 'prepaid',
    balanceMethod: 'openai_billing',
  },
  {
    id: 'deepseek',
    name: 'DeepSeek',
    baseUrl: 'https://api.deepseek.com',
    models: ['deepseek-chat', 'deepseek-reasoner'],
    billingType: 'prepaid',
    balanceMethod: 'endpoint',
    balanceEndpoint: '/user/balance',
  },
  {
    id: 'siliconflow',
    name: '硅基流动 (SiliconFlow)',
    baseUrl: 'https://api.siliconflow.cn/v1',
    models: [
      'deepseek-ai/DeepSeek-V3',
      'deepseek-ai/DeepSeek-R1',
      'deepseek-ai/DeepSeek-V2.5',
      'Qwen/Qwen2.5-72B-Instruct',
      'Qwen/Qwen3-235B-A22B',
      'Pro/Llama-4-Maverick-17B-128E',
      'Pro/DeepSeek-R1',
      'Pro/Qwen3-235B-A22B',
    ],
    billingType: 'prepaid',
    balanceMethod: 'endpoint',
    balanceEndpoint: '/user/info',
    versionPrefix: '/v1',
  },
  {
    id: 'dashscope',
    name: '通义千问 (DashScope)',
    baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1',
    models: ['qwen-turbo', 'qwen-plus', 'qwen-max', 'qwen-flash', 'qwen3-235b-a22b'],
    billingType: 'prepaid',
    balanceMethod: 'none',
  },
  {
    id: 'zhipu',
    name: '智谱AI (GLM)',
    baseUrl: 'https://open.bigmodel.cn/api/paas/v4',
    models: ['glm-4-flash', 'glm-4', 'glm-4-plus', 'glm-4-air', 'glm-4-airx'],
    billingType: 'prepaid',
    balanceMethod: 'none',
  },
  {
    id: 'moonshot',
    name: '月之暗面 (Kimi)',
    baseUrl: 'https://api.moonshot.cn/v1',
    models: ['moonshot-v1-8k', 'moonshot-v1-32k', 'moonshot-v1-128k', 'kimi-latest'],
    billingType: 'prepaid',
    balanceMethod: 'endpoint',
    balanceEndpoint: '/users/me/balance',
  },
  {
    id: 'volcano',
    name: '火山引擎 (豆包)',
    baseUrl: 'https://ark.cn-beijing.volces.com/api/v3',
    models: ['doubao-lite-128k', 'doubao-pro-128k', 'doubao-pro-256k'],
    billingType: 'subscription',
    balanceMethod: 'none',
  },
  {
    id: 'openrouter',
    name: 'OpenRouter',
    baseUrl: 'https://openrouter.ai/api/v1',
    models: [
      'openai/gpt-4o-mini',
      'openai/gpt-4o',
      'anthropic/claude-3.5-sonnet',
      'anthropic/claude-4-sonnet',
      'google/gemini-2.0-flash',
      'google/gemini-2.5-pro',
    ],
    billingType: 'prepaid',
    balanceMethod: 'endpoint',
    balanceEndpoint: '/auth/key',
  },
  {
    id: 'custom',
    name: '自定义',
    baseUrl: '',
    models: [],
    billingType: 'prepaid',
    balanceMethod: 'none',
  },
]

/** 根据 Base URL 匹配平台（模糊匹配，用于已保存的配置反查） */
export function matchPlatform(baseUrl: string): ApiPlatform | undefined {
  if (!baseUrl) return undefined
  // 统一规范化：去斜杠、补协议
  let normalized = baseUrl.trim().toLowerCase().replace(/\/+$/, '')
  if (!normalized.startsWith('http')) {
    normalized = 'https://' + normalized
  }
  // 精确匹配优先
  const exact = API_PLATFORMS.find(
    (p) => p.baseUrl && p.baseUrl.toLowerCase().replace(/\/+$/, '') === normalized,
  )
  if (exact) return exact
  // 按域名匹配：提取 URL 中的域名部分进行匹配
  // 避免路径差异导致误匹配（如 /v1 vs /compatible-mode/v1）
  let domainMatch: ApiPlatform | undefined
  for (const p of API_PLATFORMS) {
    if (!p.baseUrl) continue
    const pDomain = p.baseUrl.toLowerCase().replace(/\/+$/, '')
    // 如果用户 URL 以平台 baseUrl 开头，且平台不是 custom
    if (p.id !== 'custom' && normalized.startsWith(pDomain)) {
      // 优先选择最长匹配（更精确）
      if (!domainMatch || pDomain.length > (domainMatch.baseUrl?.length ?? 0)) {
        domainMatch = p
      }
    }
  }
  return domainMatch
}

/** 根据平台获取推荐模型 */
export function getPlatformModels(baseUrl: string): string[] {
  const platform = matchPlatform(baseUrl)
  return platform?.models ?? []
}
