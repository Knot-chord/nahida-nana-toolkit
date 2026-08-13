/**
 * 文件格式转换 — 插件入口
 *
 * 工具工坊第一个工具。md/txt/html/docx/pdf 五格式互转（20 条路径），
 * Rust 原生 + Python 桥接双通道，目录批量导入、暂停/继续、并发自适应。
 */

import type { PluginModule } from '@nahida-nana/shared'
import FileConverter from './FileConverter.vue'

const plugin: PluginModule = {
  manifest: {
    id: 'file-converter',
    name: '文件格式转换',
    version: '0.1.2',
    description: 'md/txt/html/docx/pdf 五格式互转，目录批量导入、暂停/继续、大文件自适应~',
    icon: '📄',
    category: '工具',
    enabledByDefault: true,
  },
  component: FileConverter,
}

export default plugin
