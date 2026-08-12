/**
 * 侧边栏导航
 *
 * 纯导航，不展示任何状态信息。
 * 结构：控制台 → 虚空终端 → 分隔线 → 四大模块
 * 设置/关于通过底部 footer 图标访问，不在菜单中。
 */

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { NMenu } from 'naive-ui'
import type { MenuOption } from 'naive-ui'
import { getModuleVisibility, type ModuleId } from '../utils/modules'

const route = useRoute()
const router = useRouter()

/** 模块可见性（响应式） */
const moduleVisibility = getModuleVisibility()

/** 所有模块菜单项（按顺序） */
const allModuleItems: { label: string; key: string; id: ModuleId }[] = [
  { label: '📁 项目展柜', key: 'module:projects', id: 'projects' },
  { label: '🔧 工具工坊', key: 'module:tools', id: 'tools' },
  { label: '💡 兴趣收藏', key: 'module:interests', id: 'interests' },
  { label: '🎮 游戏小馆', key: 'module:games', id: 'games' },
]

/**
 * 侧边栏菜单
 *
 * 顺序：控制台 → 虚空终端 → 分隔线 → 可见模块
 * 根据模块可见性动态过滤
 */
const menuOptions = computed<MenuOption[]>(() => {
  const visibleModules = allModuleItems
    .filter((item) => moduleVisibility[item.id])
    .map(({ label, key }) => ({ label, key }))

  return [
    { label: '🏠 控制台', key: 'console' },
    { label: '💭 虚空终端', key: 'terminal' },
    { type: 'divider', key: 'div1' },
    ...visibleModules,
  ]
})

/** 菜单选中项（game-detail 路由归属游戏小馆） */
const activeKey = computed(() => {
  if (route.name === 'console') return 'console'
  if (route.name === 'terminal') return 'terminal'
  if (route.name === 'game-detail') return 'module:games'
  if (route.name === 'tool-detail') return 'module:tools'
  if (route.name === 'module') return `module:${route.params.id as string}`
  return null
})

function handleMenuSelect(key: string) {
  if (key === 'console') {
    router.push({ name: 'console' })
  } else if (key === 'terminal') {
    router.push({ name: 'terminal' })
  } else if (key.startsWith('module:')) {
    const moduleId = key.slice(7)
    router.push({ name: 'module', params: { id: moduleId } })
  }
}
</script>

<template>
  <NMenu
    :options="menuOptions"
    :value="activeKey"
    @update:value="handleMenuSelect"
  />
</template>
