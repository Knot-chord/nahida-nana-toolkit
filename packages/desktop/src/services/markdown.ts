/**
 * Markdown → HTML 渲染器
 *
 * 基于 markdown-it（100% CommonMark + GFM 表格/删除线）
 * VS Code 同款解析引擎，正确处理表格内加粗、引用嵌套等复杂场景。
 *
 * 安全策略：html: false 禁止源中原始 HTML，markdown-it 内部自动转义
 */

import MarkdownIt from 'markdown-it'

const md = new MarkdownIt({
  html: false,         // 安全：禁止源中原始 HTML 标签
  breaks: true,        // \n 自动转 <br>
  linkify: true,       // 自动识别裸 URL 为链接
  typographer: false,  // 不自动替换引号/符号（避免干扰中文语境）
})

// 启用 GFM 扩展
md.enable(['table', 'strikethrough'])

// ── 自定义链接渲染：所有链接新窗口打开 ──
const defaultLinkOpen = md.renderer.rules.link_open
  || function (tokens, idx, options, _env, self) {
    return self.renderToken(tokens, idx, options)
  }

md.renderer.rules.link_open = function (tokens, idx, options, env, self) {
  const token = tokens[idx]
  if (token.attrIndex('target') < 0) {
    token.attrPush(['target', '_blank'])
  }
  if (token.attrIndex('rel') < 0) {
    token.attrPush(['rel', 'noopener noreferrer'])
  }
  return defaultLinkOpen(tokens, idx, options, env, self)
}

/** 将消息文本渲染为安全 HTML（含 [citation:N] 引用徽章） */
export function renderMarkdown(text: string): string {
  if (!text) return ''

  // 先渲染 Markdown
  let html = md.render(text)

  // ── 将 [citation:N] 转换为可点击的引用徽章 ──
  // 点击事件通过父容器 .msg-text 的 @click 代理处理
  html = html.replace(
    /\[citation:(\d+)\]/g,
    '<sup class="citation-badge" data-cite="$1">[$1]</sup>'
  )

  return html
}
