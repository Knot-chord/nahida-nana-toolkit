<script setup lang="ts">
import { NLayout, NLayoutSider, NLayoutContent, NMessageProvider, NDialogProvider, NConfigProvider, type GlobalThemeOverrides } from 'naive-ui'
import PluginNav from './components/PluginNav.vue'
import SidebarFooter from './components/SidebarFooter.vue'
import { startSystemMonitor } from './services/use-system-monitor'
import { useSettingsStore } from './stores/settings'
import { scanSkills } from './services/skill-runner'

// 应用启动即开始系统监控后台预采集：
// 用户进入虚空终端时数据已在流动，状态栏零等待、无割裂感
startSystemMonitor()

// 应用启动即解析并扫描 Skills 目录（默认位于应用资源目录，随安装包分发，相对解析无硬编码）：
// 无需进设置页，对话链路随时可用已启用技能；含历史绝对路径自动迁移
const settingsStore = useSettingsStore()
settingsStore.resolveDefaultSkillsDir().then(() => {
  const dir = settingsStore.skillsRootDir
  if (dir) scanSkills(dir)
})

// 全站主题覆写：NCard 默认圆角 3px 偏硬朗，统一调到 10px，
// 与纸面底色 + 微影的整体柔和感同源（各模块卡片均为 NCard，一处生效）
const themeOverrides: GlobalThemeOverrides = {
  Card: { borderRadius: '10px' },
}
</script>

<template>
  <NConfigProvider :theme-overrides="themeOverrides">
  <NMessageProvider>
    <NDialogProvider>
      <NLayout has-sider class="app-layout">
    <!-- 侧边栏 -->
    <NLayoutSider
      bordered
      :width="220"
      :native-scrollbar="false"
      class="sidebar"
    >
      <!-- 顶部标题 -->
      <div class="sidebar-header">
        <span class="sidebar-icon">🌿</span>
        <span class="sidebar-title">Nahida-nana</span>
      </div>

      <!-- 模块导航菜单 -->
      <div class="sidebar-nav">
        <PluginNav />
      </div>

      <!-- 底部：版本号 + 设置/关于图标（固定在窗口最底部） -->
      <SidebarFooter />
    </NLayoutSider>

    <!-- 右侧内容区（路由切换带柔和过渡，避免页面突然出现/消失） -->
    <NLayoutContent
      :native-scrollbar="true"
      class="content-area"
    >
      <router-view v-slot="{ Component }">
        <transition name="page" mode="out-in">
          <!-- 包一层单元素 div：部分页面是多根节点组件（如虚空终端含诊断按钮），
               Transition 无法直接动画多根组件，会导致 out-in 卡死不切换 -->
          <div :key="$route.fullPath" class="page-slot">
            <component :is="Component" />
          </div>
        </transition>
      </router-view>
    </NLayoutContent>
      </NLayout>
    </NDialogProvider>
  </NMessageProvider>
  </NConfigProvider>
</template>

<style>
/* ========================================
 * Nahida-nana 工具箱 — 全局基础样式
 * 当前阶段：使用 Naive UI 默认主题，配色留到 UI 打磨层
 * ======================================== */

/* ── 设计变量（后续深色模式时集中替换；中性色全部带极淡暖调，与纸面底色同源） ── */
:root {
  --bg-color: #ffffff;
  --border-color: #e9e8e2;
  --text-muted: #9a998f;
  --text-secondary: #666;
  --text-body: #555;
  --text-primary: #333;
  --hover-bg: rgba(0, 0, 0, 0.05);
  --active-bg: rgba(0, 0, 0, 0.08);
}

*,
*::before,
*::after {
  box-sizing: border-box;
}

html {
  /* 兼容 Windows 缩放和浏览器缩放 */
  font-size: 100%;
  -webkit-text-size-adjust: 100%;
}

body {
  margin: 0;
  padding: 0;
  font-family: 'Segoe UI', -apple-system, BlinkMacSystemFont, 'PingFang SC',
    'Microsoft YaHei', 'Helvetica Neue', Arial, sans-serif;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

/* ========================================
 * 应用布局 — 响应式视口
 * ======================================== */

.app-layout {
  height: 100vh;
  height: 100dvh; /* 动态视口，兼容移动端和部分缩放场景 */
}

/* ========================================
 * 侧边栏
 * ======================================== */

.sidebar {
  display: flex;
  flex-direction: column;
  overflow: hidden; /* 防止缩放时溢出 */
  position: relative; /* 为 absolute footer 提供定位上下文 */
}

.sidebar-header {
  padding: 1.125rem 1rem 0.875rem;
  display: flex;
  align-items: center;
  /* 左对齐：与下方导航项同一视觉纵列，整条侧边栏读起来更协调 */
  justify-content: flex-start;
  gap: 0.5rem;
  border-bottom: 1px solid var(--border-color);
  flex-shrink: 0;
}

.sidebar-icon {
  font-size: 1.25rem;
  line-height: 1;
}

.sidebar-title {
  font-weight: 700;
  font-size: 0.9375rem;
  letter-spacing: 0.03em;
}

.sidebar-nav {
  flex: 1;
  min-height: 0; /* flex 子元素防止溢出 */
  padding: 0.25rem 0;
  padding-bottom: 3.5rem; /* 为底部固定的 footer 预留空间 */
  overflow-y: auto;
}

/* ========================================
 * 内容区
 * ======================================== */

.content-area {
  padding: 0;
  /* 暖白纸面底色（纳西妲配色规范底色 #fafaf8，呼应白色花苞裙）：
     纯色而非渐变——高级感来自克制，纸面质感交给内容与留白。
     与侧边栏纯白形成极微弱的层次分离，无装饰图形，性能零开销 */
  background: #fafaf8;
}

/* 路由过渡容器：继承内容区高度，保证 height:100% 页面（如虚空终端）布局不被破坏 */
.page-slot {
  height: 100%;
}

/* ========================================
 * 路由切换过渡 — 参考 View Transitions 社区方案：
 * 短时长（200ms 内）+ 淡入淡出 + 轻微位移，
 * 只动 opacity/transform（合成层），不触发重排
 * ======================================== */
.page-enter-active {
  transition: opacity 0.22s ease-out, transform 0.22s ease-out;
}

.page-leave-active {
  transition: opacity 0.15s ease-in, transform 0.15s ease-in;
}

.page-enter-from {
  opacity: 0;
  transform: translateY(8px);
}

.page-leave-to {
  opacity: 0;
  transform: translateY(-6px);
}

/* ========================================
 * 响应式适配 — 小窗口 / 高缩放比
 * ======================================== */

@media (max-width: 640px) {
  .sidebar-header {
    padding: 0.75rem 0.75rem 0.625rem;
  }

  .sidebar-title {
    font-size: 0.8125rem;
  }
}
</style>
