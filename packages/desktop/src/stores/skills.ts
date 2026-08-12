/**
 * skillStore — Skills 全局状态
 *
 * 职责：
 * - 管理已加载的 Skill 列表
 * - 启用/禁用切换
 * - 提供匹配用户输入的方法
 * - 持久化启用状态到 localStorage
 */

import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { Skill } from '@nahida-nana/shared'

const STORAGE_KEY = 'nahida-skills-enabled'

/** 从 localStorage 读取已启用的 skill id 集合 */
function loadEnabledIds(): Set<string> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (!raw) return new Set()
    return new Set(JSON.parse(raw) as string[])
  } catch {
    return new Set()
  }
}

/** 持久化已启用的 skill id 集合 */
function saveEnabledIds(ids: string[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(ids))
}

export const useSkillStore = defineStore('skills', () => {
  /** 所有已加载的 Skill */
  const skills = ref<Skill[]>([])

  /** Skills 根目录路径 */
  const rootDir = ref('')

  /** 已启用的 Skill 列表 */
  const enabledSkills = computed(() => skills.value.filter((s) => s.enabled))

  /** 替换整个 Skill 列表（扫描完成后调用），恢复启用状态。
   * 常驻型（always）技能默认启用：目录迁移会使路径 hash 变化导致 id 失效，
   * 人格类技能不应因存储位置变动而丢失启用状态 */
  function setSkills(list: Skill[]): void {
    const enabledIds = loadEnabledIds()
    skills.value = list.map((s) => ({
      ...s,
      enabled: enabledIds.has(s.id) || s.manifest.always === true,
    }))
  }

  /** 切换启用状态 */
  function toggleSkill(id: string): void {
    const s = skills.value.find((sk) => sk.id === id)
    if (!s) return
    s.enabled = !s.enabled
    // 持久化
    saveEnabledIds(skills.value.filter((sk) => sk.enabled).map((sk) => sk.id))
  }

  /** 设置根目录路径 */
  function setRootDir(dir: string): void {
    rootDir.value = dir
  }

  /** 清空所有 Skill */
  function clearSkills(): void {
    skills.value = []
  }

  return {
    skills,
    rootDir,
    enabledSkills,
    setSkills,
    toggleSkill,
    setRootDir,
    clearSkills,
  }
})
