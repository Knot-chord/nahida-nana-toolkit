/**
 * 2048 小游戏 — 插件入口
 *
 * 纯前端游戏插件，方向键控制，分数累计，输赢判断。
 */

import type { PluginModule } from '@nahida-nana/shared'
import Game2048 from './Game2048.vue'

const plugin: PluginModule = {
  manifest: {
    id: 'game-2048',
    name: '2048',
    version: '0.1.0',
    description: '经典 2048 小游戏，方向键合并数字，挑战最高分~',
    icon: '🎮',
    category: '游戏',
    enabledByDefault: true,
  },
  component: Game2048,
}

export default plugin
