/**
 * 介绍页
 *
 * 职责：品牌认知 —— "这是什么"
 * 不导航、不配置、不操作
 *
 * 内容：Nahida 风格欢迎语 + 项目简介 + 版本信息 + 链接
 */

<script setup lang="ts">
import { NCard, NSpace, NTag, NButton } from 'naive-ui'

const features = [
  { icon: '📁', name: '项目展柜', desc: '展示作者的作品，支持代码/视频/文章/设计' },
  { icon: '🔧', name: '工具工坊', desc: '你的工具，你来决定装什么' },
  { icon: '💡', name: '兴趣收藏', desc: '收藏有趣的内容，书签管理器模式' },
  { icon: '🎮', name: '游戏小馆', desc: '想放松的时候来这里玩玩' },
  { icon: '💭', name: '虚空终端', desc: 'AI 统一入口，智慧与梦境的交汇' },
]
</script>

<template>
  <div class="about-page">
    <div class="about-hero">
      <div class="hero-icon">🌿</div>
      <h1 class="hero-title">Nahida-nana 工具箱</h1>
      <p class="hero-desc">
        一个正在成长中的小工具箱。<br>
        像纳西妲编织梦境一样，一点一点地把有趣的东西收集起来~
      </p>
      <div class="hero-tags">
        <NTag size="small" :bordered="false" type="success">超轻量</NTag>
        <NTag size="small" :bordered="false" type="info">高拓展</NTag>
        <NTag size="small" :bordered="false" type="warning">本地优先</NTag>
        <NTag size="small" :bordered="false">有趣驱动</NTag>
      </div>
    </div>

    <section class="about-section">
      <h3 class="section-title">它能做什么</h3>
      <div class="feature-grid">
        <NCard
          v-for="f in features"
          :key="f.name"
          size="small"
          class="feature-card"
        >
          <div class="feature-inner">
            <span class="feature-icon">{{ f.icon }}</span>
            <div class="feature-text">
              <span class="feature-name">{{ f.name }}</span>
              <span class="feature-desc">{{ f.desc }}</span>
            </div>
          </div>
        </NCard>
      </div>
    </section>

    <section class="about-section">
      <h3 class="section-title">关于开发者</h3>
      <p class="about-text">
        一个喜欢折腾的开发者，<br>
        相信"有趣"是最好的驱动力 🌱
      </p>
      <NSpace>
        <NButton tag="a" href="https://github.com/Knot-chord" target="_blank" text type="primary" size="small">
          GitHub →
        </NButton>
        <NButton tag="a" href="https://space.bilibili.com/" target="_blank" text type="primary" size="small">
          B站 →
        </NButton>
      </NSpace>
    </section>

    <section class="about-section">
      <h3 class="section-title">版本信息</h3>
      <p class="about-text">
        当前版本：v0.1 · 正在生长中 🌱<br>
        基于 Tauri v2 + Vue 3 + TypeScript + Naive UI 构建
      </p>
    </section>
  </div>
</template>

<style scoped>
.about-page {
  padding: 2rem 2.5rem;
  max-width: 48rem;
  margin: 0 auto;
}

/* ── 头部 ── */
.about-hero {
  text-align: center;
  padding: 3rem 0 2rem;
}

.hero-icon {
  font-size: 3rem;
  margin-bottom: 0.75rem;
}

.hero-title {
  font-size: 1.5rem;
  font-weight: 700;
  margin: 0 0 0.5rem;
  letter-spacing: 0.02em;
}

.hero-desc {
  font-size: 0.9375rem;
  color: var(--text-secondary);
  line-height: 1.7;
  margin: 0 0 1rem;
}

.hero-tags {
  display: flex;
  justify-content: center;
  gap: 0.5rem;
  flex-wrap: wrap;
}

/* ── 段落 ── */
.about-section {
  margin-bottom: 2rem;
}

.section-title {
  font-size: 0.9375rem;
  font-weight: 600;
  margin: 0 0 0.75rem;
  color: var(--text-primary);
}

.about-text {
  font-size: 0.875rem;
  color: var(--text-body);
  line-height: 1.7;
  margin: 0 0 0.75rem;
}

/* ── 功能卡片 ── */
.feature-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(13rem, 1fr));
  gap: 0.75rem;
}

.feature-card {
  cursor: default;
}

.feature-inner {
  display: flex;
  align-items: flex-start;
  gap: 0.625rem;
}

.feature-icon {
  font-size: 1.25rem;
  flex-shrink: 0;
  margin-top: 0.125rem;
}

.feature-text {
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
}

.feature-name {
  font-size: 0.875rem;
  font-weight: 600;
}

.feature-desc {
  font-size: 0.75rem;
  color: var(--text-muted);
}
</style>
