/**
 * 游戏小馆视图
 *
 * 卡片网格展示所有游戏插件，点击卡片进入游戏
 * 支持按分类筛选（当前只有游戏类，预留扩展）
 */

<script setup lang="ts">
import { ref, computed } from 'vue'
import { NCard, NTag, NEmpty, NSelect } from 'naive-ui'
import { useRouter } from 'vue-router'
import { getPlugins } from '../../plugins/manager'

/** 游戏卡片数据 */
interface GameItem {
  id: string
  name: string
  icon: string
  description: string
  tags?: string[]
}

const router = useRouter()
const selectedTag = ref<string>('全部')

/** 游戏列表（当前只有一个2048，后续自动从插件注册表读取） */
const games = ref<GameItem[]>([
  {
    id: 'game-2048',
    name: '2048',
    icon: '🎮',
    description: '经典数字合并游戏，方向键控制，挑战最高分~',
    tags: ['益智', '经典'],
  },
])

/** 所有标签（去重） */
const allTags = computed(() => {
  const tags = new Set<string>()
  games.value.forEach(g => g.tags?.forEach(t => tags.add(t)))
  return ['全部', ...Array.from(tags)]
})

/** 筛选选项 */
const tagOptions = computed(() =>
  allTags.value.map(t => ({ label: t, value: t }))
)

/** 已启用的游戏插件 ID 集合（响应式） */
const enabledGameIds = computed(() => {
  const ids = new Set<string>()
  for (const entry of Array.from(getPlugins().values())) {
    if (entry.state.enabled && entry.module?.manifest.category === '游戏') {
      ids.add(entry.module.manifest.id)
    }
  }
  return ids
})

/** 过滤后的游戏（按标签 + 插件启用状态） */
const filteredGames = computed(() => {
  const list = selectedTag.value === '全部'
    ? games.value
    : games.value.filter(g => g.tags?.includes(selectedTag.value))
  return list.filter(g => enabledGameIds.value.has(g.id))
})

/** 进入游戏 */
function enterGame(gameId: string) {
  router.push(`/module/games/${gameId}`)
}
</script>

<template>
  <div class="games-view">
    <div class="games-header">
      <h2 class="page-title">🎮 游戏小馆</h2>
      <p class="page-desc">想放松的时候来这里玩玩~</p>
    </div>

    <!-- 分类筛选 -->
    <div class="filter-bar">
      <NSelect
        v-model:value="selectedTag"
        :options="tagOptions"
        size="small"
        style="width: 120px"
      />
    </div>

    <!-- 空状态 -->
    <NEmpty
      v-if="filteredGames.length === 0"
      description="还没有游戏哦，过段时间再来看看吧~"
      class="empty-state"
    />

    <!-- 游戏卡片网格 -->
    <div v-else class="games-grid">
      <NCard
        v-for="game in filteredGames"
        :key="game.id"
        hoverable
        size="small"
        class="game-card"
        @click="enterGame(game.id)"
      >
        <template #header>
          <span class="card-title">{{ game.icon }} {{ game.name }}</span>
        </template>
        <p class="card-desc">{{ game.description }}</p>
        <div v-if="game.tags?.length" class="card-tags">
          <NTag
            v-for="tag in game.tags"
            :key="tag"
            size="tiny"
            :bordered="false"
          >
            {{ tag }}
          </NTag>
        </div>
      </NCard>
    </div>
  </div>
</template>

<style scoped>
.games-view {
  padding: 1.5rem 2rem;
}

.games-header {
  margin-bottom: 1rem;
}

.page-title {
  font-size: 1.25rem;
  font-weight: 600;
  margin: 0 0 0.25rem;
}

.page-desc {
  font-size: 0.8125rem;
  color: var(--text-secondary);
  margin: 0;
}

/* ── 筛选栏 ── */
.filter-bar {
  margin-bottom: 1rem;
}

/* ── 空状态 ── */
.empty-state {
  padding: 8vh 0;
}

/* ── 卡片网格 ── */
.games-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(17.5rem, 1fr));
  gap: 1rem;
}

.game-card {
  cursor: pointer;
  transition: box-shadow 0.2s;
}

.game-card:hover {
  box-shadow: 0 2px 8px var(--active-bg);
}

.card-title {
  font-size: 0.9375rem;
  font-weight: 600;
}

.card-desc {
  font-size: 0.8125rem;
  color: var(--text-body);
  line-height: 1.6;
  margin: 0 0 0.5rem;
}

.card-tags {
  display: flex;
  gap: 0.25rem;
  flex-wrap: wrap;
}

/* ── 响应式 ── */
@media (max-width: 640px) {
  .games-view {
    padding: 1rem;
  }

  .games-grid {
    grid-template-columns: 1fr;
  }
}
</style>
