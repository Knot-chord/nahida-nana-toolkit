/**
 * 项目展柜视图
 *
 * 职责：展示“用户自己的作品”（只读）
 * 支持代码/视频/文章/设计四种类型，按类型筛选
 * 数据来源：GitHub API + 手动录入，当前用 mock 数据
 * 用户角色：只读，点击卡片跳转到原链接
 */

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { NCard, NTag, NSpace, NEmpty, NSpin, NSelect } from 'naive-ui'
import { fetchProjects, type Project, type ProjectType } from '../../data/projects'
import { openExternal } from '../../utils/open'

const projects = ref<Project[]>([])
const loading = ref(true)
const selectedType = ref<ProjectType | '全部'>('全部')

/** 分类选项 */
const typeOptions = [
  { label: '全部', value: '全部' },
  { label: '💻 代码', value: '代码' },
  { label: '🎬 视频', value: '视频' },
  { label: '✍️ 文章', value: '文章' },
  { label: '🎨 设计', value: '设计' },
]

/** 过滤后的项目列表 */
const filteredProjects = computed(() => {
  if (selectedType.value === '全部') return projects.value
  return projects.value.filter((p) => p.type === selectedType.value)
})

/** 项目类型对应的标签颜色 */
function typeColor(type: ProjectType): 'info' | 'success' | 'warning' | 'error' {
  const map: Record<ProjectType, 'info' | 'success' | 'warning' | 'error'> = {
    '代码': 'info',
    '视频': 'success',
    '文章': 'warning',
    '设计': 'error',
  }
  return map[type]
}

/** 打开外部链接 */
function openProject(url: string) {
  openExternal(url)
}

onMounted(async () => {
  try {
    projects.value = await fetchProjects()
  } catch (err) {
    console.error('[项目展柜] 加载失败:', err)
  } finally {
    loading.value = false
  }
})
</script>

<template>
  <div class="projects-view">
    <div class="projects-header">
      <h2 class="page-title">📁 项目展柜</h2>
      <p class="page-desc">这是作者做过的项目，点击卡片可以跳转到原链接~</p>
    </div>

    <!-- 分类筛选 -->
    <div class="filter-bar">
      <NSelect
        v-model:value="selectedType"
        :options="typeOptions"
        size="small"
        style="width: 120px"
      />
    </div>

    <!-- 加载状态 -->
    <div v-if="loading" class="loading-state">
      <NSpin size="medium" />
      <span>正在整理展柜……</span>
    </div>

    <!-- 空状态 -->
    <NEmpty
      v-else-if="filteredProjects.length === 0"
      description="展柜还是空的，过段时间再来看看吧~"
      class="empty-state"
    />

    <!-- 项目卡片网格 -->
    <div v-else class="projects-grid">
      <NCard
        v-for="project in filteredProjects"
        :key="project.id"
        :hoverable="!!project.url"
        size="small"
        class="project-card"
        :class="{ 'no-link': !project.url }"
        @click="project.url && openProject(project.url)"
      >
        <template #header>
          <span class="card-title">{{ project.title }}</span>
        </template>
        <template #header-extra>
          <NTag :type="typeColor(project.type)" size="small">
            {{ project.type }}
          </NTag>
        </template>
        <p class="card-desc">{{ project.description }}</p>
        <NSpace v-if="project.tags?.length" size="small" class="card-tags">
          <NTag
            v-for="tag in project.tags"
            :key="tag"
            size="tiny"
            :bordered="false"
          >
            {{ tag }}
          </NTag>
        </NSpace>
      </NCard>
    </div>
  </div>
</template>

<style scoped>
.projects-view {
  padding: 1.5rem 2rem;
}

.projects-header {
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

/* ── 加载 / 空状态 ── */
.loading-state {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 0.75rem;
  padding: 8vh 0;
  color: var(--text-secondary);
  font-size: 0.875rem;
}

.empty-state {
  padding: 8vh 0;
}

/* ── 卡片网格 ── */
.projects-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(17.5rem, 1fr));
  gap: 1rem;
}

.project-card {
  cursor: pointer;
  transition: box-shadow 0.2s;
}

.project-card:hover {
  box-shadow: 0 2px 8px var(--active-bg);
}

.project-card.no-link {
  cursor: default;
  opacity: 0.6;
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
  margin-top: 0.25rem;
}

/* ── 响应式 ── */
@media (max-width: 640px) {
  .projects-view {
    padding: 1rem;
  }

  .projects-grid {
    grid-template-columns: 1fr;
  }
}
</style>
