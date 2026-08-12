/**
 * 打开外部链接
 *
 * Tauri 环境：使用系统浏览器打开
 * Web 环境：使用 window.open
 */
export async function openExternal(url: string): Promise<void> {
  if (!url) return
  try {
    // Tauri 环境
    if ('__TAURI_INTERNALS__' in window) {
      const { openUrl } = await import('@tauri-apps/plugin-opener')
      await openUrl(url)
    } else {
      window.open(url, '_blank', 'noopener,noreferrer')
    }
  } catch {
    // 降级
    window.open(url, '_blank', 'noopener,noreferrer')
  }
}
