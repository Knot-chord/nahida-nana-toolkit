/**
 * 控制台页面
 *
 * 职责：状态总览 —— "发生了什么"
 * 不执行功能、不做深度设置、不展示详细列表
 *
 * 结构：
 * 1. 欢迎语（Nahida 语感）
 * 2. 动态信息区（版本更新 / 开发预告 / GitHub 动态）
 * 3. 模块快捷入口（四卡片 + 查看全部）
 */

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useRouter } from 'vue-router'
import { NCard, NTag, NTimeline, NTimelineItem, NButton, NSwitch } from 'naive-ui'
import { getPlugins } from '../plugins/manager'
import { getModuleVisibility, toggleModuleVisibility, type ModuleId } from '../utils/modules'
import type { PluginCategory } from '@nahida-nana/shared'

const router = useRouter()

/** 动态信息折叠状态 */
const feedExpanded = ref(false)

/** 折叠时只显示前 2 条 */
const COLLAPSED_COUNT = 2

const visibleFeed = computed(() => {
  if (feedExpanded.value) return feedItems
  return feedItems.slice(0, COLLAPSED_COUNT)
})

/** 时间线颜色渐变：从冷到暖 */
const feedColors = ['#2080f0', '#18a058', '#f0a020', '#e88030', '#d03050', '#8030d0']

/** 模块可见性（响应式） */
const moduleVisibility = getModuleVisibility()

/** 按分类统计插件数量（响应式读取注册表） */
const moduleStats = computed(() => {
  const registry = getPlugins()
  const categories: { id: string; name: string; icon: string; category: PluginCategory }[] = [
    { id: 'projects', name: '项目展柜', icon: '📁', category: '项目' },
    { id: 'tools', name: '工具工坊', icon: '🔧', category: '工具' },
    { id: 'interests', name: '兴趣收藏', icon: '💡', category: '兴趣' },
    { id: 'games', name: '游戏小馆', icon: '🎮', category: '游戏' },
  ]
  return categories.map((mod) => {
    let total = 0
    let enabled = 0
    for (const entry of Array.from(registry.values())) {
      if (entry.module?.manifest.category === mod.category) {
        total++
        if (entry.state.enabled) enabled++
      }
    }
    return { ...mod, total, enabled }
  })
})

/** 跳转到模块页面 */
function goToModule(moduleId: string) {
  router.push({ name: 'module', params: { id: moduleId } })
}

/**
 * 最新动态：只反映开发进度，不反映运行时状态
 *
 * 三种时态：【已完成】完成时、【开发中】进行时、【待开发】将来时。
 * 新条目在前，每条带具体日期（待开发条目日期为规划提出时间）。
 */
const feedItems: { type: 'success' | 'warning' | 'default'; title: string; content: string; date: string }[] = [
  // ── 已完成（完成时）──
  { type: 'success', title: '【已完成】🎭 角色人格去内置化', content: '对话剥离全部纳西妲风格，人格经 Skills 常驻注入实现', date: '08-12' },
  { type: 'success', title: '【已完成】⚡ 性能全面优化', content: '监控启动即预采集、状态栏零等待、采集线程分离、模块平滑过渡', date: '08-12' },
  { type: 'success', title: '【已完成】📊 系统监控全量就位', content: 'CPU/内存/GPU 动态进度条，支持 Intel/AMD 显卡', date: '08-10' },
  { type: 'success', title: '【已完成】🧩 Skills 机制去内置化', content: '从文件系统加载，支持 JSON 导入与三级启用', date: '07-20' },
  { type: 'success', title: '【已完成】📄 文件转换全格式覆盖', content: 'Markdown/HTML/Word/纯文本互转 + 原生拖拽', date: '07-10' },
  { type: 'success', title: '【已完成】💭 虚空终端加入侧边栏', content: 'AI 统一入口就位，等待第 3 层唤醒', date: '06-25' },
  { type: 'success', title: '【已完成】🏗️ v0.1 地基搭建', content: '插件系统、界面骨架已就位', date: '06-20' },

  // ── 开发中（进行时）──
  { type: 'warning', title: '【开发中】☁️ 第 3 层 · 云端 AI', content: '虚空终端 AI 对话、流式输出、风格转换、Skills 三层唤醒', date: '08-12' },

  // ── 待开发（将来时）──
  { type: 'default', title: '【待开发】📴 第 4 层 · 本地 AI', content: '离线对话、模型下载校验、本地风格转换', date: '08-12' },
  { type: 'default', title: '【待开发】🎨 LUT 处理工具', content: '图形渲染能力已就绪，进入工具工坊即可安装', date: '08-12' },
  { type: 'default', title: '【待开发】🕹️ 游戏扩充', content: '更多小游戏 + 游戏 AI 助手，复用游戏插件模板', date: '08-12' },
  { type: 'default', title: '【待开发】🔌 插件 SDK', content: 'v1.0 稳定后对外开放；官网待桌面端稳定后重启', date: '08-12' },
]

</script>

<template>
  <div class="console-page">
    <!-- 欢迎语 -->
    <div class="welcome-section">
      <h2 class="welcome-title">🌿 欢迎来到 Nahida-nana</h2>
      <p class="welcome-sub">今天的工具箱也好好地成长着呢 🌱</p>
    </div>

    <!-- 动态信息区 -->
    <section class="console-section">
      <h3 class="section-title">最新动态</h3>
      <NTimeline>
        <NTimelineItem
          v-for="(item, index) in visibleFeed"
          :key="index"
          :type="item.type"
          :title="item.title"
          :content="item.content"
          :time="item.date"
          :color="feedColors[index % feedColors.length]"
        />
      </NTimeline>
      <NButton
        v-if="feedItems.length > COLLAPSED_COUNT"
        text
        size="small"
        class="feed-toggle"
        @click="feedExpanded = !feedExpanded"
      >
        {{ feedExpanded ? '收起 ▲' : `展开更多（${feedItems.length - COLLAPSED_COUNT} 条）▼` }}
      </NButton>
    </section>

    <!-- 模块一览（含模块级开关） -->
    <section class="console-section">
      <h3 class="section-title">模块一览</h3>
      <div class="module-grid">
        <NCard
          v-for="mod in moduleStats"
          :key="mod.id"
          hoverable
          size="small"
          class="module-card"
          :class="{ 'module-disabled': !moduleVisibility[mod.id as ModuleId] }"
          @click="goToModule(mod.id)"
        >
          <div class="module-card-inner">
            <span class="module-icon">{{ mod.icon }}</span>
            <div class="module-info">
              <span class="module-name">{{ mod.name }}</span>
              <NTag size="tiny" :bordered="false">
                {{ mod.total === 0 ? '空空的~' : `${mod.enabled} / ${mod.total}` }}
              </NTag>
            </div>
            <NSwitch
              :value="moduleVisibility[mod.id as ModuleId]"
              @update:value="(v: boolean) => toggleModuleVisibility(mod.id as ModuleId, v)"
              @click.stop
              size="small"
            />
          </div>
        </NCard>
      </div>
    </section>


  </div>
</template>

<style scoped>
.console-page {
  padding: 1.5rem 2rem;
}

/* ── 欢迎语 ── */
.welcome-section {
  margin-bottom: 1.5rem;
}

.welcome-title {
  font-size: 1.25rem;
  font-weight: 600;
  margin: 0 0 0.25rem;
}

.welcome-sub {
  font-size: 0.875rem;
  color: var(--text-secondary);
  margin: 0;
}

/* ── 通用段落 ── */
.console-section {
  margin-bottom: 1.5rem;
}

.section-title {
  font-size: 0.9375rem;
  font-weight: 600;
  margin: 0 0 0.75rem;
  color: var(--text-primary);
}

.feed-toggle {
  margin-top: 0.25rem;
  color: var(--text-muted);
  font-size: 0.75rem;
}

/* ── 模块快捷入口 ── */
.module-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 0.75rem;
}

.module-card {
  cursor: pointer;
  transition: box-shadow 0.2s;
  width: 16rem;
  flex-shrink: 0;
}

.module-card:hover {
  box-shadow: 0 2px 8px var(--active-bg);
}

.module-disabled {
  opacity: 0.55;
}

.module-card-inner {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.module-icon {
  font-size: 1.5rem;
  flex-shrink: 0;
}

.module-info {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.module-name {
  font-size: 0.875rem;
  font-weight: 600;
}

.module-arrow {
  color: var(--text-muted);
  font-size: 0.875rem;
  flex-shrink: 0;
}


/* ── 响应式 ── */
@media (max-width: 640px) {
  .console-page {
    padding: 1rem;
  }
}
</style>
