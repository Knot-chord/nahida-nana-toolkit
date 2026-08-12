/**
 * 插件管理器
 *
 * v1.0 仅支持内置插件，通过 Vite import.meta.glob 构建时加载。
 * 负责扫描插件目录、加载插件模块、管理插件状态。
 */

import { reactive } from 'vue'
import type { PluginModule, PluginState } from '@nahida-nana/shared'

/** 插件注册表（响应式） */
const registry = reactive<Map<string, { module: PluginModule; state: PluginState }>>(new Map())

/**
 * 批量注册插件模块
 * 由 main.ts 调用，传入 import.meta.glob 的结果
 */
export function registerPlugins(modules: Record<string, () => Promise<unknown>>) {
  for (const [path, loader] of Object.entries(modules)) {
    loader().then((mod) => {
      const pluginModule = (mod as { default: PluginModule }).default
      if (!pluginModule?.manifest?.id) {
        console.warn(`[PluginManager] 插件缺少 manifest: ${path}`)
        return
      }
      registry.set(pluginModule.manifest.id, {
        module: pluginModule,
        state: { loaded: true, enabled: true },
      })
      console.log(`[PluginManager] 已加载插件: ${pluginModule.manifest.name} (${pluginModule.manifest.id})`)
    }).catch((err) => {
      console.error(`[PluginManager] 加载插件失败: ${path}`, err)
      // 尝试从路径推断 id
      const id = path.split('/').pop()?.replace('.ts', '') ?? 'unknown'
      registry.set(id, {
        module: null as unknown as PluginModule,
        state: { loaded: false, enabled: false, error: String(err) },
      })
    })
  }
}

/** 获取所有已注册插件 */
export function getPlugins() {
  return registry
}

/** 获取已启用的插件列表 */
export function getEnabledPlugins(): PluginModule[] {
  const result: PluginModule[] = []
  for (const entry of registry.values()) {
    if (entry.state.enabled && entry.module) {
      result.push(entry.module)
    }
  }
  return result
}

/** 获取已加载插件数量 */
export function getPluginCount(): number {
  let count = 0
  for (const entry of registry.values()) {
    if (entry.state.loaded) count++
  }
  return count
}

/** 启用/禁用插件 */
export function togglePlugin(id: string, enabled: boolean) {
  const entry = registry.get(id)
  if (!entry) return
  entry.state.enabled = enabled
  if (enabled) {
    entry.module.onActivated?.()
  } else {
    entry.module.onDeactivated?.()
  }
}
