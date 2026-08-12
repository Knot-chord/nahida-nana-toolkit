/**
 * 项目展柜 — 数据层
 *
 * 展示“用户自己的作品”——不限于代码，也包括视频、文章、设计等
 * 数据来源：GitHub API 自动拉取 + 手动录入
 * 当前阶段：使用 mock 数据占位，后续替换为真实请求
 *
 * 用户角色：只读，点击卡片跳转到原链接
 */

/** 项目类型 */
export type ProjectType = '代码' | '视频' | '文章' | '设计'

/** 项目卡片数据 */
export interface Project {
  id: string
  /** 标题 */
  title: string
  /** 简短描述 */
  description: string
  /** 项目类型 */
  type: ProjectType
  /** 外部链接（GitHub / B站 / 等） */
  url: string
  /** 标签（可选，如技术栈） */
  tags?: string[]
  /** 封面图（可选，B站视频等可自动获取） */
  cover?: string
}

/**
 * 获取项目列表
 *
 * 当前返回 mock 数据。
 * 后续替换为：
 * 1. 代码项目：GitHub API 自动拉取（配置用户名即可）
 * 2. 非GitHub作品：工具箱内表单手动录入
 */
export async function fetchProjects(): Promise<Project[]> {
  // TODO: 替换为真实数据源
  return mockProjects
}

/** mock 项目数据（占位，后续替换为真实内容） */
const mockProjects: Project[] = [
  {
    id: 'nahida-toolkit',
    title: 'Nahida-nana 工具箱',
    description: '一个用户可编程的本地工具箱平台，基于 Tauri v2 + Vue 3 构建。',
    type: '代码',
    url: 'https://github.com/Knot-chord/nahida-nana-toolkit',
    tags: ['Tauri', 'Vue 3', 'TypeScript'],
  },
  {
    id: 'bilibili-video',
    title: '雨过的午后，寂静的思绪',
    description: 'B站视频作品 · BV1qQLf6iEzu',
    type: '视频',
    url: 'https://www.bilibili.com/video/BV1qQLf6iEzu',
    tags: ['B站'],
    cover: 'https://i2.hdslb.com/bfs/archive/da57e7906880f0296acbfcec0f10d393457d1568.jpg',
  },
  {
    id: 'placeholder-article',
    title: '示例文章',
    description: '占位文章项目，后续替换为真实作品。',
    type: '文章',
    url: '',
    tags: ['示例'],
  },
]
