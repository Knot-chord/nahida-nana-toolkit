/**
 * 兴趣收藏 — 数据层
 *
 * 书签管理器模式：手动添加链接，按时间倒序排列
 * 仅存储 标题 + 链接 + 来源，不存储内容本身
 * 数据持久化到 localStorage
 */

const STORAGE_KEY = 'interests-bookmarks'

/** 收藏条目 */
export interface Bookmark {
  id: string
  /** 标题 */
  title: string
  /** 链接 */
  url: string
  /** 来源（如 "B站"、"Twitter"） */
  source: string
  /** 创建时间戳 */
  createdAt: number
}

/** 读取所有收藏（按时间倒序） */
export function loadBookmarks(): Bookmark[] {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) return []
  try {
    const list: Bookmark[] = JSON.parse(raw)
    return list.sort((a, b) => b.createdAt - a.createdAt)
  } catch {
    return []
  }
}

/** 保存收藏列表 */
function save(list: Bookmark[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(list))
}

/** 添加收藏 */
export function addBookmark(item: Omit<Bookmark, 'id' | 'createdAt'>): Bookmark {
  const list = loadBookmarks()
  const bookmark: Bookmark = {
    ...item,
    id: `bm-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`,
    createdAt: Date.now(),
  }
  list.push(bookmark)
  save(list)
  return bookmark
}

/** 删除收藏 */
export function removeBookmark(id: string): void {
  const list = loadBookmarks().filter((b) => b.id !== id)
  save(list)
}
