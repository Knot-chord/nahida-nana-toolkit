/**
 * 文件格式转换 — 插件入口
 *
 * 工具工坊第一个工具。支持 md/txt/html 互转，
 * 大文件流式处理，批量转换 10+ 文件。
 */

import type { PluginModule } from '@nahida-nana/shared'
import FileConverter from './FileConverter.vue'

const plugin: PluginModule = {
  manifest: {
    id: 'file-converter',
    name: '文件格式转换',
    version: '0.1.0',
    description: '文档格式互转，支持 Markdown、纯文本、HTML，批量转换~',
    icon: '📄',
    category: '工具',
    enabledByDefault: true,
  },
  component: FileConverter,
}

export default plugin
