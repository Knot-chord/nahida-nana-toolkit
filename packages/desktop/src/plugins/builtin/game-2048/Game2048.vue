/**
 * 2048 小游戏
 *
 * 方向键控制，分数累计，输赢判断。
 * 游戏状态持久化到 localStorage，退出后重新进入继续上一局。
 * 点击"新一局"清除存档重新开始。
 */

<script setup lang="ts">
import { ref, watch, onMounted, onUnmounted, nextTick } from 'vue'
import { NButton, NTag } from 'naive-ui'

// ─── 常量 ───────────────────────────────────────────
const SIZE = 4
const STORAGE_KEY = 'game-2048-state'
const BEST_KEY = 'game-2048-best'

type Grid = number[][]
type Direction = 'left' | 'right' | 'up' | 'down'
type GameState = 'playing' | 'won' | 'lost'

/** 持久化的存档结构 */
interface SaveData {
  grid: Grid
  score: number
  state: GameState
  keepPlaying: boolean
}

// ─── 棋盘工具函数 ─────────────────────────────────────

function emptyGrid(): Grid {
  return Array.from({ length: SIZE }, () => Array<number>(SIZE).fill(0))
}

function clone(g: Grid): Grid {
  return g.map(row => [...row])
}

/** 逆时针旋转 90°：(r,c) → (c, SIZE-1-r) */
function rotateCCW(g: Grid): Grid {
  const n = emptyGrid()
  for (let r = 0; r < SIZE; r++)
    for (let c = 0; c < SIZE; c++)
      n[c][SIZE - 1 - r] = g[r][c]
  return n
}

/** 向左滑动一行，返回 { 结果行, 本次得分 } */
function slideRow(row: number[]): { result: number[]; gained: number } {
  const filtered = row.filter(v => v !== 0)
  const result: number[] = []
  let gained = 0
  let i = 0
  while (i < filtered.length) {
    if (i + 1 < filtered.length && filtered[i] === filtered[i + 1]) {
      const merged = filtered[i] * 2
      result.push(merged)
      gained += merged
      i += 2
    } else {
      result.push(filtered[i])
      i++
    }
  }
  while (result.length < SIZE) result.push(0)
  return { result, gained }
}

/** 在随机空格子生成 2（90%）或 4（10%） */
function spawnTile(g: Grid): void {
  const cells: [number, number][] = []
  for (let r = 0; r < SIZE; r++)
    for (let c = 0; c < SIZE; c++)
      if (g[r][c] === 0) cells.push([r, c])
  if (cells.length === 0) return
  const [r, c] = cells[Math.floor(Math.random() * cells.length)]
  g[r][c] = Math.random() < 0.9 ? 2 : 4
}

/** 创建新游戏棋盘（随机放 2 个方块） */
function freshGrid(): Grid {
  const g = emptyGrid()
  spawnTile(g)
  spawnTile(g)
  return g
}

// ─── 状态 ─────────────────────────────────────────────

const grid = ref<Grid>(emptyGrid())
const score = ref(0)
const bestScore = ref(Number(localStorage.getItem(BEST_KEY) || '0'))
const gameState = ref<GameState>('playing')
const keepPlaying = ref(false)

// ─── 存档 / 读档 ──────────────────────────────────────

function save(): void {
  const data: SaveData = {
    grid: grid.value,
    score: score.value,
    state: gameState.value,
    keepPlaying: keepPlaying.value,
  }
  localStorage.setItem(STORAGE_KEY, JSON.stringify(data))
}

function load(): boolean {
  const raw = localStorage.getItem(STORAGE_KEY)
  if (!raw) return false
  try {
    const data: SaveData = JSON.parse(raw)
    grid.value = data.grid
    score.value = data.score
    gameState.value = data.state
    keepPlaying.value = data.keepPlaying
    return true
  } catch {
    return false
  }
}

function clearSave(): void {
  localStorage.removeItem(STORAGE_KEY)
}

// ─── 游戏逻辑 ─────────────────────────────────────────

/** 开始新一局（清除存档） */
function newGame(): void {
  grid.value = freshGrid()
  score.value = 0
  gameState.value = 'playing'
  keepPlaying.value = false
  clearSave()
}

/** 执行移动 */
function move(dir: Direction): boolean {
  if (gameState.value === 'lost') return false
  if (gameState.value === 'won' && !keepPlaying.value) return false

  let g = clone(grid.value)
  let gained = 0

  // rotateCCW 是逆时针，所以：
  // UP → 顺时针 = 逆时针×3 → slideLeft → 逆时针×1 转回
  // DOWN → 逆时针×1 → slideLeft → 逆时针×3 转回
  const rotMap: Record<Direction, number> = { left: 0, up: 3, right: 2, down: 1 }
  const rotations = rotMap[dir]

  for (let i = 0; i < rotations; i++) g = rotateCCW(g)
  for (let r = 0; r < SIZE; r++) {
    const { result, gained: rowGained } = slideRow(g[r])
    g[r] = result
    gained += rowGained
  }
  for (let i = 0; i < (4 - rotations) % 4; i++) g = rotateCCW(g)

  // 无变化则忽略
  if (g.every((row, r) => row.every((v, c) => v === grid.value[r][c]))) return false

  grid.value = g
  score.value += gained
  if (score.value > bestScore.value) {
    bestScore.value = score.value
    localStorage.setItem(BEST_KEY, String(bestScore.value))
  }

  spawnTile(grid.value)

  // 状态判断
  if (!keepPlaying.value && grid.value.some(row => row.some(v => v >= 2048))) {
    gameState.value = 'won'
  } else if (isGameOver()) {
    gameState.value = 'lost'
  }

  return true
}

/** 棋盘已满且无可合并 → 游戏结束 */
function isGameOver(): boolean {
  const g = grid.value
  for (let r = 0; r < SIZE; r++) {
    for (let c = 0; c < SIZE; c++) {
      if (g[r][c] === 0) return false
      if (c + 1 < SIZE && g[r][c] === g[r][c + 1]) return false
      if (r + 1 < SIZE && g[r][c] === g[r + 1][c]) return false
    }
  }
  return true
}

// ─── 输入 ──────────────────────────────────────────────

const gameRef = ref<HTMLElement | null>(null)

const KEY_MAP: Record<string, Direction> = {
  ArrowLeft: 'left', ArrowRight: 'right',
  ArrowUp: 'up', ArrowDown: 'down',
}

function onKeyDown(e: KeyboardEvent) {
  const dir = KEY_MAP[e.key]
  if (dir) {
    e.preventDefault()
    move(dir)
  }
}

function focusGame() {
  gameRef.value?.focus()
}

// ─── 自动存档：每次移动后 ──────────────────────────────

watch([grid, score, gameState, keepPlaying], () => { save() }, { deep: true })

// ─── 生命周期 ──────────────────────────────────────────

onMounted(() => {
  // 优先读档，无存档则开新局
  if (!load()) newGame()
  window.addEventListener('keydown', onKeyDown)
  nextTick(() => gameRef.value?.focus())
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKeyDown)
})

// ─── 模板辅助 ──────────────────────────────────────────

function tileClass(v: number): string {
  return v === 0 ? 'tile-empty' : `tile-${v}`
}
function tileText(v: number): string {
  return v > 0 ? String(v) : ''
}
</script>

<template>
  <div ref="gameRef" class="game-2048" tabindex="0" @click="focusGame">
    <!-- 头部：标题 + 分数 -->
    <div class="game-header">
      <div>
        <h2 class="game-title">🎮 2048</h2>
        <p class="game-desc">方向键合并数字，达到 2048 就赢啦~</p>
      </div>
      <div class="score-board">
        <div class="score-item">
          <span class="score-label">分数</span>
          <span class="score-value">{{ score }}</span>
        </div>
        <div class="score-item">
          <span class="score-label">最高</span>
          <span class="score-value">{{ bestScore }}</span>
        </div>
      </div>
    </div>

    <!-- 胜利提示 -->
    <div v-if="gameState === 'won'" class="overlay won">
      <p class="overlay-title">✨ 达到 2048 了！</p>
      <p class="overlay-desc">好厉害~要继续挑战更高分数吗？</p>
      <div class="overlay-buttons">
        <NButton size="small" @click="keepPlaying = true; gameState = 'playing'">继续玩</NButton>
        <NButton size="small" type="primary" @click="newGame()">新一局</NButton>
      </div>
    </div>

    <!-- 失败提示 -->
    <div v-if="gameState === 'lost'" class="overlay lost">
      <p class="overlay-title">游戏结束了~</p>
      <p class="overlay-desc">没关系的，再来一次吧 🌱</p>
      <NButton size="small" type="primary" @click="newGame()">新一局</NButton>
    </div>

    <!-- 棋盘 -->
    <div class="grid">
      <div v-for="(row, r) in grid" :key="r" class="grid-row">
        <div
          v-for="(cell, c) in row"
          :key="`${r}-${c}`"
          class="tile"
          :class="tileClass(cell)"
        >
          <span class="tile-value">{{ tileText(cell) }}</span>
        </div>
      </div>
    </div>

    <!-- 底部 -->
    <div class="game-footer">
      <NButton size="small" @click="newGame()">🌱 新一局</NButton>
      <NTag size="tiny" :bordered="false">方向键控制</NTag>
    </div>
  </div>
</template>

<style scoped>
.game-2048 {
  max-width: 28rem;
  margin: 0 auto;
  /* 顶部边距由外层 game-detail 统一控制（全站标准 1.5rem） */
  padding: 0 1.5rem 1.5rem;
  user-select: none;
  outline: none;
}

/* ── 头部 ── */
.game-header {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  margin-bottom: 1rem;
}
.game-title {
  font-size: 1.5rem;
  font-weight: 700;
  margin: 0 0 0.25rem;
}
.game-desc {
  font-size: 0.75rem;
  color: var(--text-muted);
  margin: 0;
}
.score-board {
  display: flex;
  gap: 0.5rem;
}
.score-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  background: #f5f5f5;
  border-radius: 0.5rem;
  padding: 0.375rem 0.75rem;
  min-width: 3.5rem;
}
.score-label {
  font-size: 0.625rem;
  color: var(--text-muted);
  text-transform: uppercase;
  font-weight: 600;
}
.score-value {
  font-size: 1.125rem;
  font-weight: 700;
  color: var(--text-primary);
}

/* ── 覆盖层 ── */
.overlay {
  position: relative;
  margin-bottom: 0.5rem;
  padding: 1rem;
  border-radius: 0.5rem;
  text-align: center;
}
.overlay.won { background: rgba(237, 194, 46, 0.15); }
.overlay.lost { background: rgba(187, 173, 160, 0.15); }
.overlay-title {
  font-size: 1.125rem;
  font-weight: 600;
  margin: 0 0 0.25rem;
}
.overlay-desc {
  font-size: 0.8125rem;
  color: var(--text-secondary);
  margin: 0 0 0.75rem;
}
.overlay-buttons {
  display: flex;
  gap: 0.5rem;
  justify-content: center;
}

/* ── 棋盘 ── */
.grid {
  background: #bbada0;
  border-radius: 0.5rem;
  padding: 0.5rem;
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}
.grid-row {
  display: flex;
  gap: 0.5rem;
}
.tile {
  width: 5rem;
  height: 5rem;
  border-radius: 0.375rem;
  display: flex;
  align-items: center;
  justify-content: center;
  flex: 1;
  transition: background-color 0.1s;
}
.tile-empty { background: rgba(238, 228, 218, 0.35); }
.tile-value { font-weight: 700; font-size: 1.5rem; }

/* ── 数字颜色 ── */
.tile-2    { background: #eee4da; color: #776e65; }
.tile-4    { background: #ede0c8; color: #776e65; }
.tile-8    { background: #f2b179; color: #f9f6f2; }
.tile-16   { background: #f59563; color: #f9f6f2; }
.tile-32   { background: #f67c5f; color: #f9f6f2; }
.tile-64   { background: #f65e3b; color: #f9f6f2; }
.tile-128  { background: #edcf72; color: #f9f6f2; }
.tile-256  { background: #edcc61; color: #f9f6f2; }
.tile-512  { background: #edc850; color: #f9f6f2; }
.tile-1024 { background: #edc53f; color: #f9f6f2; }
.tile-2048 { background: #edc22e; color: #f9f6f2; }
.tile-4096 { background: #3c3a32; color: #f9f6f2; }

/* 大数字缩小字体 */
.tile-128 .tile-value,
.tile-256 .tile-value,
.tile-512 .tile-value { font-size: 1.25rem; }
.tile-1024 .tile-value,
.tile-2048 .tile-value,
.tile-4096 .tile-value { font-size: 1rem; }

/* ── 底部 ── */
.game-footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-top: 0.75rem;
}

/* ── 响应式 ── */
@media (max-width: 480px) {
  .tile { width: 4rem; height: 4rem; }
  .tile-value { font-size: 1.25rem; }
  .tile-128 .tile-value,
  .tile-256 .tile-value,
  .tile-512 .tile-value { font-size: 1rem; }
  .tile-1024 .tile-value,
  .tile-2048 .tile-value,
  .tile-4096 .tile-value { font-size: 0.875rem; }
}
</style>
