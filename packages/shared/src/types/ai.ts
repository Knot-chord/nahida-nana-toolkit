/**
 * AI 对话类型定义（跨包共享）
 *
 * 设计原则：
 * - AIProvider 是最小接口，只定义"发送消息"和"中断"两个能力
 * - 流式输出通过 onChunk 回调逐 token 推送，而非一次性返回
 * - CancelSignal 是 AbortSignal 的最小兼容接口
 * - AIProviderConfig 存储所有可调参数，用户可在设置页自定义
 * - Skill 仅作为 AI 行为的"提示/约束"注入，不执行脚本
 */

/** 取消信号（最小接口，兼容 AbortSignal） */
export interface CancelSignal {
  readonly aborted: boolean
  addEventListener(type: 'abort', listener: () => void, options?: { once?: boolean }): void
}

/** 单条消息 */
export interface ChatMessage {
  /** 角色 */
  role: 'user' | 'assistant' | 'system'
  /**
   * 内容：普通文本使用 string；多模态（含图片）使用 ContentPart[]
   * 向后兼容：所有现有代码的 string content 不受影响
   */
  content: string | ChatContentPart[]
}

/** 多模态内容片段 */
export interface ChatContentPart {
  type: 'text' | 'image_url'
  text?: string
  image_url?: { url: string; detail?: 'auto' | 'low' | 'high' }
}

/** AI 服务商配置（用户可在设置页修改） */
export interface AIProviderConfig {
  /** API 密钥 */
  apiKey: string
  /** API 基础地址（如 https://api.openai.com/v1） */
  baseUrl: string
  /** 模型名称 */
  model: string
  /** 温度 (0-2)，越高越随机 */
  temperature: number
  /** 最大输出 token 数 */
  maxTokens: number
  /** Top-P 采样 (0-1) */
  topP: number
}

/** 默认配置 */
export const DEFAULT_AI_CONFIG: AIProviderConfig = {
  apiKey: '',
  baseUrl: 'https://api.openai.com/v1',
  model: 'gpt-4o-mini',
  temperature: 0.8,
  maxTokens: 2048,
  topP: 0.95,
}

/** AI 服务商接口 */
export interface AIProvider {
  /** 服务商名称（显示用） */
  readonly name: string

  /**
   * 发送消息并流式接收回复
   *
   * @param messages - 完整对话历史（含最新用户消息）
   * @param onChunk  - 每收到一个文本片段时回调（传入新增部分）
   * @param signal   - 取消信号，用于中断正在进行的请求
   * @returns 完整的 assistant 回复文本
   * @throws 网络错误/认证错误/限流错误等
   */
  chat(
    messages: ChatMessage[],
    onChunk: (delta: string) => void,
    signal?: CancelSignal,
  ): Promise<string>
}

// ═══════════════════════════════════════════════════
//  Skills 类型
// ═══════════════════════════════════════════════════

/** SKILL.md 解析出的 Skill 元数据 */
export interface SkillManifest {
  /** Skill 名称（唯一标识） */
  name: string
  /** 一句话描述，用于匹配用户意图 */
  description: string
  /** 版本号 */
  version?: string
  /** 作者 */
  author?: string
  /** 常驻注入：启用后完整内容每轮对话都注入（适合人格/角色类技能，不走 L2 按需加载） */
  always?: boolean
}

/** 已加载的 Skill（运行时） */
export interface Skill {
  /** 唯一 ID（由文件路径 hash 生成） */
  id: string
  /** 磁盘路径 */
  path: string
  /** SKILL.md frontmatter 解析结果 */
  manifest: SkillManifest
  /** SKILL.md body 内容（注入 AI 上下文的提示/约束文本） */
  prompt: string
  /** 是否启用 */
  enabled: boolean
}
