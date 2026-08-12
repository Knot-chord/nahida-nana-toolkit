/**
 * 侧边栏底部
 *
 * 单行布局：版本号（左）+ 设置/关于图标（右）
 * 紧贴侧边栏最底部，不换行。
 */

<script setup lang="ts">
import { useRoute, useRouter } from 'vue-router'

const route = useRoute()
const router = useRouter()

function goTo(name: string) {
  router.push({ name })
}
</script>

<template>
  <div class="sidebar-footer">
    <span class="footer-version">v0.1 · 正在生长中 🌱</span>

    <div class="footer-icons">
      <button
        class="footer-icon-btn"
        :class="{ active: route.name === 'settings' }"
        @click="goTo('settings')"
        aria-label="设置"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <circle cx="12" cy="12" r="3"/>
          <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
        </svg>
      </button>

      <button
        class="footer-icon-btn"
        :class="{ active: route.name === 'about' }"
        @click="goTo('about')"
        aria-label="关于"
      >
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">
          <path d="M12 22V10"/>
          <path d="M6 10c0-3.5 2.5-6 6-6 3.5 0 6 2.5 6 6 0 2-1.5 3.5-3 4.5"/>
          <path d="M12 10c-1.5 1-3 2.5-3 4.5"/>
        </svg>
      </button>
    </div>
  </div>
</template>

<style scoped>
.sidebar-footer {
  position: absolute;
  bottom: 0;
  left: 0;
  right: 0;
  border-top: 1px solid var(--border-color);
  padding: 0.5rem 0.75rem;
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.footer-version {
  font-size: 0.6875rem;
  color: var(--text-muted);
  font-weight: 500;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.footer-icons {
  display: flex;
  align-items: center;
  gap: 0.125rem;
  flex-shrink: 0;
}

.footer-icon-btn {
  width: 1.75rem;
  height: 1.75rem;
  border: none;
  border-radius: 0.5rem;
  background: transparent;
  cursor: pointer;
  color: var(--text-muted);
  display: flex;
  align-items: center;
  justify-content: center;
  transition: all 0.2s ease;
}

.footer-icon-btn:hover {
  background-color: var(--hover-bg);
  color: var(--text-primary);
}

.footer-icon-btn.active {
  background-color: var(--active-bg);
  color: var(--text-primary);
}
</style>
