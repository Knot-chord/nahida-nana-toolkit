/**
 * settingsStore — 工具箱管理全局状态
 *
 * 职责：
 * - 包装 ai-settings 的配置读写
 * - Skills 根目录路径持久化
 * - 系统级配置（主题、启动行为等，当前占位）
 */

import { defineStore } from 'pinia'
import { computed, ref } from 'vue'
import {
  getAIConfig,
  saveAIConfig,
  resetAIConfig,
} from '../services/ai-settings'
import type { AIProviderConfig } from '@nahida-nana/shared'

const SKILLS_DIR_KEY = 'nahida-skills-root-dir'

/** 历史遗留路径（第一版硬编码默认值，仅作迁移识别用） */
const LEGACY_PROJECT_SKILLS_DIR = 'D:/study/兴趣项目/nahida-nana工具箱/代码/skills'

/** 路径归一化：统一分隔符、去尾斜杠，用于比较与 startsWith 判定 */
const norm = (p: string) => p.replace(/\\/g, '/').replace(/\/$/, '')

/**
 * 默认 Skills 根目录（统一规则，不硬编码任何绝对路径）：
 * - 开发模式（vite dev server）：项目内 src-tauri/skills/（仓库源目录）。
 *   Windows 上 resourceDir 恒为 exe 所在目录（tauri-utils platform.rs），
 *   dev 时 exe 在 src-tauri/target/debug，需上溯两级才到 src-tauri
 * - 生产模式（安装包）：资源目录下的 skills/（随安装包分发，与 exe 同级）
 *
 * 注意：tauri resolve 是 push 语义（plugin.rs），绝对路径会替换已累积路径，
 * 因此绝对基准必须放第一个参数：resolve(base, 相对路径)。
 */
async function defaultSkillsDir(): Promise<string> {
  const { resourceDir, resolve } = await import('@tauri-apps/api/path')
  const base = await resourceDir()
  return import.meta.env.DEV ? resolve(base, '../../skills') : resolve(base, 'skills')
}

/** 同步读取用户自定义目录（仅此值会持久化，默认值不落盘） */
function readCustomSkillsDir(): string {
  try {
    return localStorage.getItem(SKILLS_DIR_KEY) ?? ''
  } catch {
    return ''
  }
}

/**
 * 运行时将目录加入 fs 插件 scope。
 * capabilities 里的静态模式含 ../ 时无法可靠匹配（scope 不做 .. 规范化），
 * 故对生效目录一律动态放行，覆盖 dev/prod 默认与用户自定义。
 */
async function allowSkillsDirInScope(dir: string): Promise<void> {
  if (!dir) return
  try {
    const { invoke } = await import('@tauri-apps/api/core')
    await invoke('allow_custom_skills_dir', { path: dir })
  } catch {
    // 非 Tauri 环境忽略
  }
}

export const useSettingsStore = defineStore('settings', () => {
  // ═══ AI 配置（委托给 ai-settings 模块） ═══

  /** AI 配置（reactive，直接修改后调用 saveAIConfig 持久化） */
  const aiConfig: AIProviderConfig = getAIConfig()

  /** 持久化 AI 配置 */
  function saveAi(): void {
    saveAIConfig()
  }

  /** 重置 AI 配置为默认值 */
  function resetAi(): void {
    resetAIConfig()
  }

  // ═══ Skills 根目录配置 ═══
  // 根本方案：localStorage 只持久化“用户自定义覆盖”，默认值每次启动实时解析，
  // 永不落盘 → 默认规则演进时自动生效，天然免疫任何历史遗留路径。

  /** 用户自定义目录（唯一持久化值；空 = 用默认） */
  const customSkillsDir = ref(readCustomSkillsDir())
  /** 运行时解析的默认目录（不落盘） */
  const defaultSkillsDirResolved = ref('')

  /** 当前生效的 Skills 根目录：自定义 > 默认 */
  const skillsRootDir = computed(() => customSkillsDir.value || defaultSkillsDirResolved.value)
  /** 是否处于自定义状态（控制“重置为默认”按钮显示） */
  const hasCustomSkillsDir = computed(() => !!customSkillsDir.value)

  /**
   * 启动/进入设置页时调用：
   * 1. 清理旧版本写入的“默认值”残留（落盘于 localStorage 的 appDataDir/资源目录/旧硬编码形态）
   * 2. 实时解析默认目录
   * 幂等，可多次调用。
   */
  async function resolveDefaultSkillsDir(): Promise<void> {
    try {
      const { appDataDir, resourceDir } = await import('@tauri-apps/api/path')
      // 旧版默认值均落在这些根之下（appDataDir\skills、target\…\skills、安装目录 skills 等），
      // 新版默认不落盘，故命中即清除；用户真正自选的目录不会在这些位置
      const legacyRoots = [
        norm(LEGACY_PROJECT_SKILLS_DIR),
        norm(await appDataDir()),
        norm(await resourceDir()),
      ]
      if (customSkillsDir.value) {
        const s = norm(customSkillsDir.value)
        if (legacyRoots.some((r) => s === r || s.startsWith(r + '/'))) {
          customSkillsDir.value = ''
          localStorage.removeItem(SKILLS_DIR_KEY)
        }
      }
      defaultSkillsDirResolved.value = await defaultSkillsDir()
      // 生效目录（自定义 > 默认）动态加入 fs scope
      await allowSkillsDirInScope(customSkillsDir.value || defaultSkillsDirResolved.value)
    } catch {
      // 非 Tauri 环境或 path 不可用时忽略
    }
  }

  /** 设置自定义 Skills 目录（传空串 = 重置为默认） */
  function setSkillsRootDir(dir: string): void {
    customSkillsDir.value = dir
    if (dir) {
      localStorage.setItem(SKILLS_DIR_KEY, dir)
      void allowSkillsDirInScope(dir)
    } else {
      localStorage.removeItem(SKILLS_DIR_KEY)
    }
  }

  // ═══ 系统配置（占位） ═══

  /** 外观主题（后续实现） */
  const theme = ref<'light' | 'dark' | 'auto'>('auto')

  return {
    aiConfig,
    saveAi,
    resetAi,
    skillsRootDir,
    hasCustomSkillsDir,
    setSkillsRootDir,
    resolveDefaultSkillsDir,
    theme,
  }
})
