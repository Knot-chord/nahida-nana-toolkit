/**
 * @nahida-nana/shared — 跨包共享类型与工具函数
 *
 * 存放插件接口定义等
 * 所有子包都可能依赖的公共代码。
 */

export type {
  PluginManifest,
  PluginModule,
  PluginState,
  PluginCategory,
} from './types/plugin'

export type {
  ChatMessage,
  ChatContentPart,
  AIProvider,
  CancelSignal,
  AIProviderConfig,
  SkillManifest,
  Skill,
} from './types/ai'

export { DEFAULT_AI_CONFIG } from './types/ai'
