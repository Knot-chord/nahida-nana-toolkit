/**
 * 路由配置
 *
 * 固定页面：控制台(/)、工具箱管理(/settings)、关于(/about)
 * 模块页面：/module/:id（项目展柜/工具工坊/兴趣收藏/游戏小馆）
 *
 * 首次启动逻辑：检查 localStorage.hasLaunchedBefore，
 * 首次启动重定向到 /about，后续启动进入 /。
 */

import { createRouter, createWebHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'

const routes: RouteRecordRaw[] = [
  {
    path: '/',
    name: 'console',
    component: () => import('./views/ConsolePage.vue'),
    meta: { title: '控制台' },
  },
  {
    path: '/terminal',
    name: 'terminal',
    component: () => import('./views/TerminalPage.vue'),
    meta: { title: '虚空终端' },
  },
  {
    path: '/about',
    name: 'about',
    component: () => import('./views/AboutPage.vue'),
    meta: { title: '关于' },
  },
  {
    path: '/settings',
    name: 'settings',
    component: () => import('./views/SettingsPage.vue'),
    meta: { title: '工具箱管理' },
  },
  {
    path: '/module/games/:gameId',
    name: 'game-detail',
    component: () => import('./views/ModulePage.vue'),
    meta: { title: '游戏' },
  },
  {
    path: '/module/tools/:toolId',
    name: 'tool-detail',
    component: () => import('./views/ModulePage.vue'),
    meta: { title: '工具' },
  },
  {
    path: '/module/:id',
    name: 'module',
    component: () => import('./views/ModulePage.vue'),
    meta: { title: '模块' },
  },
  // 兼容旧路由
  {
    path: '/plugin/:id',
    redirect: (to) => ({ name: 'module', params: { id: to.params.id } }),
  },
]

const router = createRouter({
  history: createWebHistory(),
  routes,
})

/** 动态更新页面标题 */
router.afterEach((to: import('vue-router').RouteLocationNormalized) => {
  const title = to.meta.title as string | undefined
  document.title = title ? `${title} - Nahida-nana 工具箱` : 'Nahida-nana 工具箱'
})

/**
 * 首次启动检测
 *
 * 首次启动时重定向到 /about，后续启动进入 /。
 * 通过 localStorage 中的 hasLaunchedBefore 标记判断。
 */
router.beforeEach((to, _from, next) => {
  if (to.path === '/' && !localStorage.getItem('hasLaunchedBefore')) {
    localStorage.setItem('hasLaunchedBefore', 'true')
    next({ name: 'about' })
  } else {
    next()
  }
})

export default router
