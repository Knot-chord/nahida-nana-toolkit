/**
 * 模块页面
 *
 * 根据路由参数 :id 渲染对应模块内容。
 * 游戏小馆支持两级导航：列表(/module/games) → 具体游戏(/module/games/:gameId)
 * 工具工坊支持两级导航：列表(/module/tools) → 具体工具(/module/tools/:toolId)
 * 未实现的模块显示占位提示。
 */

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { NResult, NCard, NSpace } from 'naive-ui'

// 同步导入各模块视图：进入模块时内容与页面同帧呈现（单一过渡动画），
// 避免异步 chunk 加载导致“页面先出、内容后出”的两段式观感；
// 组件总体积很小，桌面端解析开销可忽略
import ProjectsView from './projects/ProjectsView.vue'
import InterestsView from './interests/InterestsView.vue'
import GamesView from './games/GamesView.vue'
import Game2048 from '../plugins/builtin/game-2048/Game2048.vue'
import FileConverter from '../plugins/builtin/file-converter/FileConverter.vue'

const route = useRoute()

/** 当前模块 ID */
const moduleId = computed(() => {
  if (route.params.toolId) return 'tools'
  if (route.params.gameId) return 'games'
  return (route.params.id as string) || ''
})

/** 当前游戏 ID */
const gameId = computed(() => route.params.gameId as string | undefined)

/** 当前工具 ID */
const toolId = computed(() => route.params.toolId as string | undefined)

/** 游戏名称映射 */
const gameNames: Record<string, string> = {
  'game-2048': '2048',
}

/** 工具名称映射 */
const toolNames: Record<string, string> = {
  'file-converter': '文件格式转换',
}

/** 模块信息映射 */
const moduleInfo = computed(() => {
  const map: Record<string, { name: string; icon: string; desc: string }> = {
    projects: { name: '项目展柜', icon: '📁', desc: '作者的作品集' },
    tools: { name: '工具工坊', icon: '🔧', desc: '你的工具，你来决定装什么' },
    interests: { name: '兴趣收藏', icon: '💡', desc: '收藏有趣的内容' },
    games: { name: '游戏小馆', icon: '🎮', desc: '休闲小游戏合集' },
  }
  return map[moduleId.value] ?? { name: '未知模块', icon: '❓', desc: '该模块不存在' }
})
</script>

<template>
  <div class="module-page">
    <!-- 项目展柜 -->
    <ProjectsView v-if="moduleId === 'projects'" />

    <!-- 兴趣收藏 -->
    <InterestsView v-else-if="moduleId === 'interests'" />

    <!-- 游戏小馆：游戏列表 -->
    <GamesView v-else-if="moduleId === 'games' && !gameId" />

    <!-- 游戏小馆：具体游戏 -->
    <div v-else-if="moduleId === 'games' && gameId" class="game-detail">
      <div class="detail-header">
        <span class="detail-name">{{ gameNames[gameId] ?? '未知游戏' }}</span>
        <button class="back-btn" title="返回游戏列表" @click="$router.push('/module/games')">←</button>
      </div>
      <Game2048 v-if="gameId === 'game-2048'" />
      <NResult v-else status="info" title="游戏不存在" description="这个游戏还没开发哦~" />
    </div>

    <!-- 工具工坊：工具列表（头部范式与展柜/收藏等模块对齐：标题 + 介绍） -->
    <div v-else-if="moduleId === 'tools' && !toolId" class="tools-list">
      <div class="tools-header">
        <h2 class="page-title">🔧 工具工坊</h2>
        <p class="page-desc">你的工具你来决定——想装什么就装什么，每个工具都是独立插件~</p>
      </div>
      <NSpace :size="12" style="flex-wrap: wrap">
        <NCard
          class="tool-card"
          hoverable
          @click="$router.push('/module/tools/file-converter')"
        >
          <div class="tool-card-content">
            <span class="tool-icon">📄</span>
            <div>
              <div class="tool-name">文件格式转换</div>
              <div class="tool-desc">Markdown / 纯文本 / HTML / Word / PDF 五格式互转，支持拖拽导入~</div>
            </div>
          </div>
        </NCard>
      </NSpace>
    </div>

    <!-- 工具工坊：具体工具 -->
    <div v-else-if="moduleId === 'tools' && toolId" class="tool-detail">
      <div class="detail-header">
        <span class="detail-name">{{ toolNames[toolId] ?? '未知工具' }}</span>
        <button class="back-btn" title="返回工具列表" @click="$router.push('/module/tools')">←</button>
      </div>
      <FileConverter v-if="toolId === 'file-converter'" />
      <NResult v-else status="info" title="工具不存在" description="这个工具还没开发哦~" />
    </div>

    <!-- 其他模块（占位） -->
    <div v-else class="module-placeholder">
      <h2 class="module-title">{{ moduleInfo.icon }} {{ moduleInfo.name }}</h2>
      <NResult
        status="info"
        :title="`${moduleInfo.name}开发中`"
        :description="moduleInfo.desc"
      />
    </div>
  </div>
</template>

<style scoped>
.module-page {
  min-height: 100%;
}

.detail-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  /* 底部留白对齐全站 header 标准（与其他模块 margin-bottom: 1rem 一致） */
  padding: 0 2rem 1rem;
}

/* 返回按钮：纯图标、靠右上角，悬停淡底色反馈 */
.back-btn {
  width: 2.25rem;
  height: 2.25rem;
  display: flex;
  align-items: center;
  justify-content: center;
  border: none;
  background: transparent;
  border-radius: 8px;
  color: var(--text-secondary);
  font-size: 1.125rem;
  line-height: 1;
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease;
}

.back-btn:hover {
  background-color: rgba(0, 0, 0, 0.05);
  color: var(--text-primary);
}

/* 详情页标题与其他模块 page-title 同规格（1.25rem/600），
   返回行即标题行，不再形成两层头部 */
.detail-name {
  font-size: 1.25rem;
  font-weight: 600;
}

/* 顶部留白统一为全站标准 1.5rem（与终端/控制台/展柜等模块对齐），
   内部组件不再叠加额外顶部边距，避免“整体靠下” */
.game-detail,
.tool-detail {
  padding: 1.5rem 0 0;
}

/* 工具列表：顶部/横向留白与标题、介绍规格全部对齐全站标准
   （与项目展柜/兴趣收藏等模块同一范式） */
.tools-list {
  padding: 1.5rem 2rem;
}

.tools-header {
  margin-bottom: 1rem;
}

.tools-list .page-title {
  font-size: 1.25rem;
  font-weight: 600;
  margin: 0 0 0.25rem;
}

.tools-list .page-desc {
  font-size: 0.8125rem;
  color: var(--text-secondary);
  margin: 0;
}

.tool-card {
  cursor: pointer;
  width: 18rem;
  transition: box-shadow 0.2s;
}

.tool-card:hover {
  box-shadow: 0 2px 12px rgba(0, 0, 0, 0.08);
}

.tool-card-content {
  display: flex;
  align-items: flex-start;
  gap: 0.75rem;
}

.tool-icon {
  font-size: 1.5rem;
}

.tool-name {
  font-weight: 600;
  font-size: 0.95rem;
}

.tool-desc {
  font-size: 0.8rem;
  color: rgba(0, 0, 0, 0.45);
  margin-top: 0.25rem;
}

.module-placeholder {
  padding: 1.5rem 2rem;
}

.module-placeholder .module-title {
  margin-bottom: 1.5rem;
  font-size: 1.25rem;
  font-weight: 600;
  text-align: center;
  padding-top: 6vh;
}
</style>
