/**
 * 模块可见性管理
 *
 * 控制侧边栏和控制台中哪些模块显示/隐藏。
 * 数据持久化到 localStorage，所有组件共享同一份状态。
 */

import { reactive } from 'vue'

const STORAGE_KEY = 'module-visibility'

/** 模块 ID 列表（与侧边栏顺序一致） */
export const MODULE_IDS = ['projects', 'tools', 'interests', 'games'] as const
export type ModuleId = (typeof MODULE_IDS)[number]

/** 响应式可见性状态 */
const state = reactive<Record<ModuleId, boolean>>(loadVisibility())

function loadVisibility(): Record<ModuleId, boolean> {
  const raw = localStorage.getItem(STORAGE_KEY)
  const hidden: string[] = raw ? JSON.parse(raw) : []
  return {
    projects: !hidden.includes('projects'),
    tools: !hidden.includes('tools'),
    interests: !hidden.includes('interests'),
    games: !hidden.includes('games'),
  }
}

function saveVisibility(): void {
  const hidden = MODULE_IDS.filter((id) => !state[id])
  localStorage.setItem(STORAGE_KEY, JSON.stringify(hidden))
}

/** 获取响应式可见性状态（直接引用，修改即生效） */
export function getModuleVisibility() {
  return state
}

/** 切换模块可见性 */
export function toggleModuleVisibility(id: ModuleId, visible: boolean) {
  state[id] = visible
  saveVisibility()
}
