/**
 * 兴趣收藏视图
 *
 * 职责：展示"用户收藏的有趣内容"（书签管理器模式）
 * 手动添加链接，按时间倒序排列，点击卡片跳转原链接
 * 仅存储标题 + 链接 + 来源，不存储内容本身
 */

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NCard, NTag, NEmpty, NButton, NModal, NInput } from 'naive-ui'
import {
  loadBookmarks,
  addBookmark,
  removeBookmark,
  type Bookmark,
} from '../../data/interests'
import { openExternal } from '../../utils/open'

const bookmarks = ref<Bookmark[]>([])
const showModal = ref(false)

// 添加表单
const formTitle = ref('')
const formUrl = ref('')
const formSource = ref('')

function refresh() {
  bookmarks.value = loadBookmarks()
}

/** 打开添加弹窗 */
function openAdd() {
  formTitle.value = ''
  formUrl.value = ''
  formSource.value = ''
  showModal.value = true
}

/** 确认添加 */
function confirmAdd() {
  if (!formTitle.value.trim() || !formUrl.value.trim()) return
  addBookmark({
    title: formTitle.value.trim(),
    url: formUrl.value.trim(),
    source: formSource.value.trim() || '网页',
  })
  showModal.value = false
  refresh()
}

/** 删除收藏 */
function handleRemove(id: string) {
  removeBookmark(id)
  refresh()
}

/** 打开链接 */
function openLink(url: string) {
  openExternal(url)
}

/** 格式化时间为可读文本 */
function formatDate(ts: number): string {
  const d = new Date(ts)
  const month = d.getMonth() + 1
  const day = d.getDate()
  return `${month}月${day}日`
}

onMounted(refresh)
</script>

<template>
  <div class="interests-view">
    <div class="interests-header">
      <div>
        <h2 class="page-title">💡 兴趣收藏</h2>
        <p class="page-desc">收藏有趣的内容，发现更多好玩的~</p>
      </div>
      <NButton size="small" type="primary" @click="openAdd">+ 添加收藏</NButton>
    </div>

    <!-- 空状态 -->
    <NEmpty
      v-if="bookmarks.length === 0"
      description="还没收藏内容，去发现有趣的东西吧~"
      class="empty-state"
    />

    <!-- 书签卡片网格 -->
    <div v-else class="bookmarks-grid">
      <NCard
        v-for="bm in bookmarks"
        :key="bm.id"
        hoverable
        size="small"
        class="bookmark-card"
        @click="openLink(bm.url)"
      >
        <template #header>
          <span class="card-title">{{ bm.title }}</span>
        </template>
        <template #header-extra>
          <NTag type="success" size="small" :bordered="false">{{ bm.source }}</NTag>
        </template>
        <p class="card-url">{{ bm.url }}</p>
        <div class="card-footer">
          <span class="card-date">{{ formatDate(bm.createdAt) }}</span>
          <NButton
            text
            size="tiny"
            type="error"
            @click.stop="handleRemove(bm.id)"
          >
            移除
          </NButton>
        </div>
      </NCard>
    </div>

    <!-- 添加弹窗 -->
    <NModal
      v-model:show="showModal"
      preset="dialog"
      title="添加收藏"
      positive-text="收藏"
      negative-text="取消"
      :positive-button-props="{ disabled: !formTitle.trim() || !formUrl.trim() }"
      @positive-click="confirmAdd"
    >
      <div class="add-form">
        <div class="form-field">
          <label>标题</label>
          <NInput v-model:value="formTitle" placeholder="这个内容叫什么~" />
        </div>
        <div class="form-field">
          <label>链接</label>
          <NInput v-model:value="formUrl" placeholder="粘贴链接地址" />
        </div>
        <div class="form-field">
          <label>来源</label>
          <NInput v-model:value="formSource" placeholder="如：B站、Twitter（可选）" />
        </div>
      </div>
    </NModal>
  </div>
</template>

<style scoped>
.interests-view {
  padding: 1.5rem 2rem;
}

.interests-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
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

/* ── 空状态 ── */
.empty-state {
  padding: 8vh 0;
}

/* ── 卡片网格 ── */
.bookmarks-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(17.5rem, 1fr));
  gap: 1rem;
}

.bookmark-card {
  cursor: pointer;
  transition: box-shadow 0.2s;
}

.bookmark-card:hover {
  box-shadow: 0 2px 8px var(--active-bg);
}

.card-title {
  font-size: 0.9375rem;
  font-weight: 600;
}

.card-url {
  font-size: 0.75rem;
  color: var(--text-muted);
  margin: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.card-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 0.5rem;
}

.card-date {
  font-size: 0.6875rem;
  color: var(--text-muted);
}

/* ── 添加弹窗 ── */
.add-form {
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
}

.form-field {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.form-field label {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-primary);
}

/* ── 响应式 ── */
@media (max-width: 640px) {
  .interests-view {
    padding: 1rem;
  }

  .bookmarks-grid {
    grid-template-columns: 1fr;
  }
}
</style>
