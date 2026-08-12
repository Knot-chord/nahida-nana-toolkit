/**
 * skillRunner — Skills 加载与执行引擎
 *
 * 职责：
 * - 扫描指定目录下的 SKILL.md 文件
 * - 解析 YAML frontmatter + Markdown body
 * - 转换为 Skill 类型并写入 skillStore
 * - 提供 getPrompt() 获取已启用 Skills 的注入文本
 *
 * 当前阶段不实现动态脚本执行 —— Skill 仅作为 AI 行为的提示/约束注入
 */

import type { Skill, SkillManifest } from '@nahida-nana/shared'
import { useSkillStore } from '../stores/skills'
import { readDir, readTextFile } from '@tauri-apps/plugin-fs'

/** 简单 YAML frontmatter 解析器（免依赖） */
function parseFrontmatter(raw: string): { frontmatter: Record<string, string>; body: string } {
  const match = raw.match(/^---\s*\n([\s\S]*?)\n---\s*\n?([\s\S]*)$/)
  if (!match) {
    return { frontmatter: {}, body: raw }
  }

  const yamlBlock = match[1]
  const body = match[2].trim()

  const frontmatter: Record<string, string> = {}
  for (const line of yamlBlock.split('\n')) {
    const colonIdx = line.indexOf(':')
    if (colonIdx === -1) continue
    const key = line.slice(0, colonIdx).trim()
    let value = line.slice(colonIdx + 1).trim()
    // 去除引号
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) {
      value = value.slice(1, -1)
    }
    if (key) frontmatter[key] = value
  }

  return { frontmatter, body }
}

/** 从文件路径生成唯一 ID（简单 hash） */
function hashPath(filePath: string): string {
  let hash = 0
  for (let i = 0; i < filePath.length; i++) {
    const ch = filePath.charCodeAt(i)
    hash = ((hash << 5) - hash + ch) | 0
  }
  return 'skill_' + Math.abs(hash).toString(36)
}

/**
 * 扫描 Skills 根目录，加载所有 SKILL.md。
 * 结果写入 skillStore。
 *
 * @param rootDir - Skills 根目录绝对路径
 */
export async function scanSkills(rootDir: string): Promise<void> {
  const store = useSkillStore()
  store.setRootDir(rootDir)

  if (!rootDir) {
    store.clearSkills()
    return
  }

  try {
    const entries = await readDir(rootDir)
    const skills: Skill[] = []

    for (const entry of entries) {
      if (!entry.isDirectory) continue

      const skillDir = `${rootDir}/${entry.name}`
      const skillMdPath = `${skillDir}/SKILL.md`

      try {
        const raw = await readTextFile(skillMdPath)
        const { frontmatter, body } = parseFrontmatter(raw)

        const manifest: SkillManifest = {
          name: frontmatter.name ?? entry.name,
          description: frontmatter.description ?? '',
          version: frontmatter.version,
          author: frontmatter.author,
          // 常驻注入（人格/角色类技能）：支持 always: true 与 inject: always 两种写法
          always: frontmatter.always === 'true' || frontmatter.inject === 'always',
        }

        skills.push({
          id: hashPath(skillMdPath),
          path: skillDir,
          manifest,
          prompt: body,
          enabled: false, // store.setSkills 会从 localStorage 恢复
        })
      } catch {
        // 跳过无 SKILL.md 或读取失败的目录
        console.warn(`[skillRunner] 跳过：${skillMdPath}`)
      }
    }

    store.setSkills(skills)
  } catch (e) {
    console.error('[skillRunner] 扫描 Skills 目录失败：', e)
    store.clearSkills()
  }
}

/**
 * 获取已启用 Skills 的系统注入文本。
 *
 * 两类注入方式：
 * - 常驻型（frontmatter always: true）：完整 prompt 每轮对话直接注入，
 *   适合人格/角色类技能（人格必须贯穿整段对话，不能按需加载）
 * - 按需型（默认）：渐进式披露 L1，只注入技能名称 + 描述 + ID，
 *   模型判定需要时输出 {{USE_SKILL:id}} 标记，由 use-chat-stream
 *   检测后发起后续请求加载完整内容
 *
 * 预期 Token 消耗：按需型 ~50 tokens/技能（vs 原方案 ~1000+ tokens/技能）
 */
export function getEnabledSkillsPrompt(): string {
  const store = useSkillStore()
  const enabled = store.enabledSkills

  if (enabled.length === 0) return ''

  const sections: string[] = []

  // ── 常驻注入型：完整内容每轮都在 ──
  for (const s of enabled.filter((sk) => sk.manifest.always)) {
    sections.push(`## 常驻技能：${s.manifest.name}\n\n${s.prompt}`)
  }

  // ── 按需加载型：仅注入索引（L1） ──
  const onDemand = enabled.filter((sk) => !sk.manifest.always)
  if (onDemand.length > 0) {
    const indexLines = onDemand
      .map((s) => `- [${s.id}] ${s.manifest.name}: ${s.manifest.description}`)
      .join('\n')

    sections.push([
      '## 可用技能',
      '以下是已启用的技能索引（仅名称和描述）。',
      '如果用户的请求需要某个技能的完整指令，请在回复的**最开头**输出：',
      '{{USE_SKILL:技能ID}}',
      '系统会自动加载完整指令并重新请求。',
      '如果需要多个技能，每行一个标记。',
      '',
      indexLines,
    ].join('\n'))
  }

  return sections.join('\n\n')
}

/**
 * 根据 ID 获取技能的完整 prompt 内容。
 * 供 use-chat-stream 在检测到 {{USE_SKILL:id}} 后调用。
 */
export function getSkillFullContent(id: string): string | null {
  const store = useSkillStore()
  const skill = store.skills.find((s) => s.id === id)
  return skill?.prompt ?? null
}
