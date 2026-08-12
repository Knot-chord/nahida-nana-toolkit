/**
 * load-skill — Skills 按需加载工具（渐进式披露 L2）
 *
 * 职责：
 * - 检测 AI 回复中的 {{USE_SKILL:id}} 标记
 * - 从 skillStore 获取完整技能内容
 * - 构建后续请求的注入上下文
 *
 * 渐进式披露流程：
 * L1：System Prompt 只包含技能索引（名称+描述+ID）→ ~50 tokens/技能
 * L2：模型输出 {{USE_SKILL:id}} → 系统检测并加载完整内容 → 发起后续请求
 * L3：资源文件 → 保持现状（Skill prompt 中引用的外部文件）
 */

import { getSkillFullContent } from './skill-runner'

/** 匹配 {{USE_SKILL:xxx}} 标记的正则 */
const SKILL_REQUEST_RE = /\{\{USE_SKILL:([^}]+)\}\}/g

/** 检测到的技能请求 */
export interface SkillRequest {
  /** 技能 ID */
  id: string
  /** 技能完整内容（未找到时为 null） */
  content: string | null
}

/**
 * 检测文本中的技能加载请求。
 * 返回所有匹配的技能 ID 和对应内容。
 */
export function detectSkillRequests(text: string): SkillRequest[] {
  const requests: SkillRequest[] = []
  const seen = new Set<string>()

  let match: RegExpExecArray | null
  while ((match = SKILL_REQUEST_RE.exec(text)) !== null) {
    const id = match[1].trim()
    if (!id || seen.has(id)) continue
    seen.add(id)
    requests.push({
      id,
      content: getSkillFullContent(id),
    })
  }

  return requests
}

/** 判断文本中是否包含技能加载请求 */
export function hasSkillRequest(text: string): boolean {
  return SKILL_REQUEST_RE.test(text)
}

/**
 * 从文本中移除所有 {{USE_SKILL:xxx}} 标记。
 * 同时清理因此产生的空行。
 */
export function stripSkillMarkers(text: string): string {
  return text
    .replace(SKILL_REQUEST_RE, '')
    .replace(/\n{3,}/g, '\n\n')
    .trim()
}

/**
 * 将加载到的技能完整内容格式化为 system 注入文本。
 * 用于后续请求中注入 AI 上下文。
 */
export function formatSkillContext(skills: SkillRequest[]): string {
  const loaded = skills.filter((s) => s.content !== null)
  if (loaded.length === 0) return ''

  const parts = loaded.map(
    (s) => `## 技能指令：${s.id}\n\n${s.content}`,
  )

  return [
    '以下是你请求加载的完整技能指令。',
    '请严格按照这些指令执行。',
    '',
    ...parts,
  ].join('\n')
}
