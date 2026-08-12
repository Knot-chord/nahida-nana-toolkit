/**
 * 插件接口定义（跨包共享）
 *
 * 每个插件必须实现 PluginModule 接口，
 * 并通过 manifest 声明元信息。
 */

import type { Component } from 'vue'

/** 插件清单 */
export interface PluginManifest {
  /** 插件唯一标识，如 "todo"、"2048" */
  id: string
  /** 显示名称 */
  name: string
  /** 版本号 */
  version: string
  /** 简短描述 */
  description: string
  /** 图标（Naive UI 图标名或图片路径） */
  icon?: string
  /** 插件分类 */
  category: PluginCategory
  /** 是否默认启用（默认 true） */
  enabledByDefault?: boolean
  /** 所需权限（预留，v1.0 暂不启用） */
  permissions?: string[]
}

/** 插件分类（用户视角命名） */
export type PluginCategory = '工具' | '项目' | '兴趣' | '游戏'

/** 插件模块 —— 每个插件入口文件导出的对象 */
export interface PluginModule {
  /** 插件清单 */
  manifest: PluginManifest
  /** 插件根组件 */
  component: Component
  /** 插件激活时调用（可选） */
  onActivated?: () => void
  /** 插件停用时调用（可选） */
  onDeactivated?: () => void
}

/** 插件运行时状态 */
export interface PluginState {
  /** 是否已加载 */
  loaded: boolean
  /** 是否已启用 */
  enabled: boolean
  /** 加载错误信息（如有） */
  error?: string
}
