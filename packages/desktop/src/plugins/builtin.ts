/**
 * 内置插件注册入口
 *
 * 使用 Vite import.meta.glob 在构建时自动导入 plugins/builtin/ 下的所有插件。
 * 每个插件目录下需有 index.ts 导出 PluginModule。
 */

const builtinPlugins = import.meta.glob<{ default: unknown }>('./builtin/*/index.ts')

export default builtinPlugins
