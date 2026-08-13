/**
 * 工具箱管理
 *
 * 左侧菜单 + 右侧内容区
 * - 通用设置（占位）
 * - 模块管理（插件启用/禁用开关）
 */

<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { NMenu, NResult, NSwitch } from 'naive-ui'
import { getPlugins, togglePlugin } from '../plugins/manager'
import { getAIConfig, saveAIConfig, resetAIConfig, getTokenUsage, resetTokenUsage } from '../services/ai-settings'
import { DEFAULT_AI_CONFIG } from '@nahida-nana/shared'
import { API_PLATFORMS, matchPlatform, getPlatformModels, type BalanceResult } from '../services/api-platforms'
import { checkBalance, formatBalance } from '../services/api-balance'
import { useSettingsStore } from '../stores/settings'
import { useSkillStore } from '../stores/skills'
import { scanSkills } from '../services/skill-runner'

const settingsStore = useSettingsStore()
const skillStore = useSkillStore()

const activeSection = ref('general')

/** AI 配置（响应式，直接绑定表单） */
const aiConfig = getAIConfig()

/** 当前选中平台 ID（下拉框绑定） */
const selectedPlatformId = ref<string>(
  matchPlatform(aiConfig.baseUrl)?.id ?? 'custom',
)

/** 是否使用自定义地址 */
const isCustomUrl = computed(() => selectedPlatformId.value === 'custom')

/** 是否为自定义模型（不在平台推荐列表中则显示输入框） */
const isCustomModel = ref(false)

/** 当前平台推荐模型（未匹配时给通用兜底） */
const platformModels = computed(() => {
  const models = getPlatformModels(aiConfig.baseUrl)
  if (models.length > 0) return models
  // 通用兜底：常见性价比模型
  return ['gpt-4o-mini', 'gpt-4o', 'deepseek-chat', 'qwen-plus', 'glm-4-flash', 'claude-3.5-sonnet']
})

/** 检测当前模型是否在平台推荐列表中 */
const modelInList = computed(() => {
  return platformModels.value.includes(aiConfig.model)
})

/** 平台切换：自动填入 Base URL + 推荐模型 */
function onPlatformChange(id: string) {
  selectedPlatformId.value = id
  const plat = API_PLATFORMS.find((p) => p.id === id)
  if (plat && plat.baseUrl) {
    aiConfig.baseUrl = plat.baseUrl
    // 有推荐模型时自动填入第一个
    if (plat.models.length > 0) {
      aiConfig.model = plat.models[0]
      isCustomModel.value = false
    }
  }
  onFieldChange()
}

/** Base URL 手动输入时反查平台，并同步模型 */
function onBaseUrlChange() {
  const matched = matchPlatform(aiConfig.baseUrl)
  const prevId = selectedPlatformId.value
  selectedPlatformId.value = matched?.id ?? 'custom'
  // 平台变化时自动填入推荐模型
  if (matched && matched.id !== prevId && matched.models.length > 0) {
    aiConfig.model = matched.models[0]
    isCustomModel.value = false
  }
  onFieldChange()
}

/** 模型选择（select 或自定义） */
function onModelSelect(value: string) {
  if (value === '__custom__') {
    isCustomModel.value = true
  } else if (value === '__back__') {
    // 从自定义输入回到平台推荐列表（默认选第一个）
    isCustomModel.value = false
    if (platformModels.value.length > 0) {
      aiConfig.model = platformModels.value[0]
      onFieldChange()
    }
  } else {
    isCustomModel.value = false
    aiConfig.model = value
    onFieldChange()
  }
}

/** 最大输出 Token 净化：清空/非法输入回退默认值，负数收敛到 0（0 = 不限制） */
function onMaxTokensChange() {
  const v = aiConfig.maxTokens
  if (!Number.isFinite(v)) {
    aiConfig.maxTokens = DEFAULT_AI_CONFIG.maxTokens
  } else if (v < 0) {
    aiConfig.maxTokens = 0
  }
  onFieldChange()
}

/** 是否有未保存的更改 */
const isDirty = ref(false)
const saveMsg = ref('')
let saveTimer: ReturnType<typeof setTimeout> | null = null

/** 任一字段变更时自动保存（防抖 600ms） */
function onFieldChange() {
  isDirty.value = true
  if (saveTimer) clearTimeout(saveTimer)
  saveTimer = setTimeout(() => {
    saveAIConfig()
    isDirty.value = false
    saveMsg.value = '✓ 已自动保存'
    setTimeout(() => { saveMsg.value = '' }, 2500)
  }, 600)
}

function handleReset() {
  if (saveTimer) clearTimeout(saveTimer)
  resetAIConfig()
  selectedPlatformId.value = matchPlatform(aiConfig.baseUrl)?.id ?? 'custom'
  isDirty.value = false
  saveMsg.value = '已重置为默认值'
  setTimeout(() => { saveMsg.value = '' }, 2500)
}

/** API Key 可见性切换 */
const showKey = ref(false)

// ─── 余额查询 ───
const balanceResult = ref<BalanceResult | null>(null)
const balanceLoading = ref(false)
const balanceError = ref('')

async function handleTestConnection() {
  if (!aiConfig.apiKey.trim()) {
    balanceError.value = '请先填写 API Key'
    return
  }
  balanceLoading.value = true
  balanceError.value = ''
  balanceResult.value = null
  try {
    const result = await checkBalance(aiConfig)
    balanceResult.value = result
  } catch (e) {
    balanceError.value = `检测失败：${String(e)}`
  } finally {
    balanceLoading.value = false
  }
}

const balanceText = computed(() => formatBalance(balanceResult.value))

// Token 总用量
const tokenTotal = computed(() => tokenUsage.value.inputTokens + tokenUsage.value.outputTokens)

const balanceCurrencySymbol = computed(() => {
  return balanceResult.value?.currency === 'USD' ? '$' : '¥'
})

// ─── 会话 Token 用量 ───
const tokenUsage = ref(getTokenUsage())
const tokenUsageTimer = ref<ReturnType<typeof setInterval> | null>(null)

function refreshTokenUsage() {
  tokenUsage.value = getTokenUsage()
}

function handleResetTokens() {
  resetTokenUsage()
  tokenUsage.value = { inputTokens: 0, outputTokens: 0, estimatedCost: 0 }
}

/** 在文件资源管理器中打开 Skills 目录（直接管理 SKILL.md 文件） */
async function handleOpenSkillsDir() {
  try {
    // 未设置时先解析默认目录
    if (!settingsStore.skillsRootDir) {
      await settingsStore.resolveDefaultSkillsDir()
    }
    const dir = settingsStore.skillsRootDir
    if (!dir) return
    const { openPath } = await import('@tauri-apps/plugin-opener')
    await openPath(dir)
  } catch (e) {
    console.warn('[Skills] 打开目录失败：', e)
  }
}

/** 修改 Skills 根目录 */
async function handleChangeSkillsDir() {
  try {
    const { open } = await import('@tauri-apps/plugin-dialog')
    const selected = await open({ directory: true, multiple: false, title: '选择 Skills 根目录' })
    if (!selected) return
    settingsStore.setSkillsRootDir(selected as string)
    await scanSkills(selected as string)
  } catch (e) {
    console.warn('[Skills] 更改目录失败：', e)
  }
}

/** 扫描 Skills 目录 */
async function handleScanSkills() {
  const dir = settingsStore.skillsRootDir
  if (dir) {
    await scanSkills(dir)
  }
}

/** 重置 Skills 目录为默认（清除自定义覆盖，回退到运行时解析的默认目录） */
async function handleResetSkillsDir() {
  settingsStore.setSkillsRootDir('')
  await settingsStore.resolveDefaultSkillsDir()
  const dir = settingsStore.skillsRootDir
  if (dir) await scanSkills(dir)
}

// 定期刷新（每 5s 轮询，因为 cloud-provider 异步更新 localStorage）
onMounted(() => {
  tokenUsageTimer.value = setInterval(refreshTokenUsage, 5000)
  // 扫描 Skills 目录（含旧版默认值残留清理；App 启动时已扫一次，此处保证页面展示最新）
  settingsStore.resolveDefaultSkillsDir().then(() => {
    const dir = settingsStore.skillsRootDir
    if (dir) scanSkills(dir)
  })
  // 有 API Key 时自动查询余额
  if (aiConfig.apiKey.trim()) {
    handleTestConnection()
  }
})
onUnmounted(() => {
  if (tokenUsageTimer.value) clearInterval(tokenUsageTimer.value)
})

/** 所有已注册插件（含详细信息） */
const pluginList = computed(() => {
  const result: { id: string; name: string; description: string; category: string; version: string; enabled: boolean }[] = []
  for (const entry of getPlugins().values()) {
    if (entry.module?.manifest) {
      result.push({
        id: entry.module.manifest.id,
        name: entry.module.manifest.name,
        description: entry.module.manifest.description,
        category: entry.module.manifest.category,
        version: entry.module.manifest.version,
        enabled: entry.state.enabled,
      })
    }
  }
  return result
})

/** 模块分类配置 */
const categoryConfig: { name: string; icon: string }[] = [
  { name: '项目', icon: '📁' },
  { name: '工具', icon: '🔧' },
  { name: '兴趣', icon: '💡' },
  { name: '游戏', icon: '🎮' },
]

/** 按分类分组的插件列表 */
const groupedPlugins = computed(() => {
  return categoryConfig
    .map((cat) => ({
      ...cat,
      plugins: pluginList.value.filter((p) => p.category === cat.name),
    }))
    .filter((group) => group.plugins.length > 0)
})

/** 切换插件启用状态 */
function handleToggle(id: string, enabled: boolean) {
  togglePlugin(id, enabled)
}


</script>

<template>
  <div class="settings-page">
    <h2 class="settings-title">⚙️ 工具箱管理</h2>

    <div class="settings-body">
      <!-- 左侧菜单 -->
      <div class="settings-sidebar">
        <NMenu
          :value="activeSection"
          :options="[
            { label: '通用设置', key: 'general' },
            { label: 'AI 配置', key: 'ai' },
            { label: '模块管理', key: 'plugins' },
          ]"
          @update:value="(k: string) => activeSection = k"
        />
      </div>

      <!-- 右侧内容 -->
      <div class="settings-content">
        <!-- 通用设置 -->
        <div v-if="activeSection === 'general'" class="settings-panel">
          <NResult
            status="info"
            title="🌱 通用设置开发中"
            description="主题、语言等通用选项正在慢慢成长中~"
          />
        </div>

        <!-- AI 配置 -->
        <div v-else-if="activeSection === 'ai'" class="settings-panel ai-panel">
          <h3 class="panel-title">🤖 AI 配置</h3>
          <p class="panel-desc">配置云端大模型 API，支持所有 OpenAI 兼容接口（如 DeepSeek、通义千问等）。</p>

          <!-- 可滚动内容区 -->
          <div class="ai-panel-body">
          <!-- 连接凭证卡片 -->
          <div class="ai-card">
            <div class="ai-card-header">🔑 连接凭证</div>

            <!-- 平台选择 -->
            <label class="ai-field">
              <span class="ai-label">API 平台</span>
              <select
                :value="selectedPlatformId"
                class="ai-select"
                @change="onPlatformChange(($event.target as HTMLSelectElement).value)"
              >
                <option
                  v-for="p in API_PLATFORMS"
                  :key="p.id"
                  :value="p.id"
                >
                  {{ p.name }}
                </option>
              </select>
            </label>

            <!-- API 地址（自定义时显示输入框） -->
            <label class="ai-field">
              <span class="ai-label">API 地址</span>
              <input
                v-if="isCustomUrl"
                v-model="aiConfig.baseUrl"
                type="text"
                class="ai-input"
                placeholder="https://api.example.com/v1"
                @change="onBaseUrlChange"
              />
              <span v-else class="ai-input ai-input--readonly">{{ aiConfig.baseUrl }}</span>
              <span class="ai-hint">
                {{ isCustomUrl ? '输入任意 OpenAI 兼容端点地址' : '选择平台后自动填入，切换「自定义」可手动输入' }}
              </span>
            </label>

            <!-- API Key -->
            <label class="ai-field">
              <span class="ai-label">API Key</span>
              <div class="ai-input-wrap">
                <input
                  v-model="aiConfig.apiKey"
                  :type="showKey ? 'text' : 'password'"
                  class="ai-input"
                  placeholder="sk-..."
                  @change="onFieldChange"
                />
                <button
                  class="ai-input-btn"
                  type="button"
                  @click="showKey = !showKey"
                  :title="showKey ? '隐藏' : '显示'"
                >
                  <!-- 极简眼睛图标 -->
                  <svg v-if="!showKey" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/>
                    <circle cx="12" cy="12" r="3"/>
                  </svg>
                  <svg v-else width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
                    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94"/>
                    <path d="M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19"/>
                    <line x1="1" y1="1" x2="23" y2="23"/>
                    <path d="M14.12 14.12a3 3 0 1 1-4.24-4.24"/>
                  </svg>
                </button>
              </div>
              <span class="ai-hint">密钥仅保存在本地，不会上传到任何服务器</span>
            </label>

            <!-- 连通性测试 -->
            <div class="ai-balance-row">
              <button
                class="ai-test-btn"
                :disabled="balanceLoading || !aiConfig.apiKey.trim()"
                @click="handleTestConnection"
              >
                {{ balanceLoading ? '⏳ 检测中…' : '🔍 测试连接' }}
              </button>
              <span v-if="balanceText" class="ai-balance-text">{{ balanceText }}</span>
              <span v-else-if="balanceError" class="ai-balance-error">{{ balanceError }}</span>
            </div>
          </div>

          <!-- 模型参数卡片 -->
          <div class="ai-card">
            <div class="ai-card-header">⚙️ 模型参数</div>

            <label class="ai-field">
              <span class="ai-label">模型</span>
              <select
                v-if="platformModels.length"
                :value="isCustomModel ? '__custom__' : modelInList ? aiConfig.model : '__back__'"
                class="ai-select"
                @change="onModelSelect(($event.target as HTMLSelectElement).value)"
              >
                <option v-for="m in platformModels" :key="m" :value="m">{{ m }}</option>
                <option v-if="!modelInList" value="__back__" disabled>↩ 当前为自定义模型，选择上方任一推荐模型可切回</option>
                <option value="__custom__">✎ 自定义输入…</option>
              </select>
              <input
                v-if="isCustomModel || !platformModels.length || !modelInList"
                v-model="aiConfig.model"
                type="text"
                class="ai-input"
                placeholder="gpt-4o-mini"
                @change="onFieldChange"
              />
              <span v-if="isCustomModel" class="ai-hint">从上方下拉框选择任一推荐模型即可退出自定义输入</span>
            </label>

            <!-- Temperature -->
            <label class="ai-field">
              <span class="ai-label">
                温度 (Temperature)
                <span class="ai-val-tag">{{ aiConfig.temperature.toFixed(1) }}</span>
              </span>
              <div class="ai-range-row">
                <span class="ai-range-end">0</span>
                <input
                  v-model.number="aiConfig.temperature"
                  type="range"
                  class="ai-range"
                  min="0"
                  max="2"
                  step="0.1"
                  @change="onFieldChange"
                />
                <span class="ai-range-end">2</span>
              </div>
              <span class="ai-hint">越低越确定，越高越有创意</span>
            </label>

            <!-- Top P -->
            <label class="ai-field">
              <span class="ai-label">
                Top-P
                <span class="ai-val-tag">{{ aiConfig.topP.toFixed(2) }}</span>
              </span>
              <div class="ai-range-row">
                <span class="ai-range-end">0</span>
                <input
                  v-model.number="aiConfig.topP"
                  type="range"
                  class="ai-range"
                  min="0"
                  max="1"
                  step="0.05"
                  @change="onFieldChange"
                />
                <span class="ai-range-end">1</span>
              </div>
              <span class="ai-hint">核采样概率阈值（1 = 全部，0.1 = 前 10%）</span>
            </label>

            <!-- Max Tokens -->
            <label class="ai-field">
              <span class="ai-label">最大输出 Token</span>
              <input
                v-model.number="aiConfig.maxTokens"
                type="number"
                class="ai-input ai-input--short"
                min="0"
                max="128000"
                step="256"
                @change="onMaxTokensChange"
              />
              <span class="ai-hint">设为 0 表示不限制输出长度</span>
            </label>
          </div>

          <!-- 平台余额 -->
          <div class="ai-card">
            <div class="ai-card-header">💰 平台余额</div>

            <!-- 连接检测 + 余额显示 -->
            <div class="balance-status">
              <div v-if="balanceLoading" class="balance-status-text balance-status--loading">⏳ 查询账户余额…</div>
              <div v-else-if="balanceText" class="balance-status-text balance-status--ok">{{ balanceText }}</div>
              <div v-else-if="balanceError" class="balance-status-text balance-status--error">{{ balanceError }}</div>
              <div v-else class="balance-status-text balance-status--idle">点击下方按钮查询账户余额</div>
            </div>

            <!-- 余额 & Token 用量双卡 -->
            <div class="balance-stats">
              <div class="balance-stat-card">
                <span class="balance-stat-label">账户余额</span>
                <span class="balance-stat-value">{{ balanceCurrencySymbol }}{{ balanceResult?.balance ?? '--' }}</span>
              </div>
              <div class="balance-stat-card balance-stat-card--total">
                <span class="balance-stat-label">Token 使用量</span>
                <span class="balance-stat-value">{{ tokenTotal.toLocaleString() }}</span>
                <span class="balance-stat-sub">输入 {{ tokenUsage.inputTokens.toLocaleString() }} · 输出 {{ tokenUsage.outputTokens.toLocaleString() }}</span>
              </div>
            </div>

            <div class="balance-actions">
              <button
                class="ai-test-btn"
                :disabled="balanceLoading || !aiConfig.apiKey.trim()"
                @click="handleTestConnection"
              >
                {{ balanceLoading ? '⏳ 查询中…' : '🔍 查询余额' }}
              </button>
              <button
                class="ai-test-btn"
                :disabled="tokenUsage.inputTokens === 0 && tokenUsage.outputTokens === 0"
                @click="handleResetTokens"
              >🗑 重置用量</button>
            </div>
          </div>

          <!-- ── Skills 技能 ── -->
          <div class="ai-card skills-card">
            <div class="ai-card-header">
              <span>🧰 Skills 技能</span>
              <span v-if="skillStore.skills.length" class="skills-count">{{ skillStore.skills.length }} 个</span>
            </div>

            <!-- 存储路径 -->
            <div class="skills-path-row">
              <span class="skills-path-label">存储位置</span>
              <code class="skills-path-value">{{ settingsStore.skillsRootDir || '未设置' }}</code>
              <button class="skills-path-btn" @click="handleOpenSkillsDir" title="在文件资源管理器中打开，直接管理技能文件">打开文件夹</button>
              <button class="skills-path-btn" @click="handleChangeSkillsDir" title="更换 Skills 根目录">更改</button>
              <button v-if="settingsStore.hasCustomSkillsDir" class="skills-path-btn" @click="handleResetSkillsDir" title="清除自定义目录，回退到项目内置默认位置">重置为默认</button>
            </div>

            <div v-if="skillStore.skills.length === 0" class="skills-empty">
              <p>暂无 Skills。</p>
              <p class="skills-empty-hint">在 Skills 目录下创建 SKILL.md 文件即可自动识别。</p>
              <p class="skills-empty-hint">格式：YAML frontmatter（name、description）+ Markdown 正文。</p>
            </div>

            <div v-else class="skills-list">
              <div v-for="s in skillStore.skills" :key="s.id" class="skills-item">
                <div class="skills-item-icon">📋</div>
                <div class="skills-item-info">
                  <span class="skills-item-name">{{ s.manifest.name }}</span>
                  <span v-if="s.manifest.description" class="skills-item-desc">{{ s.manifest.description }}</span>
                </div>
                <div class="skills-item-actions">
                  <NSwitch
                    :value="s.enabled"
                    @update:value="() => skillStore.toggleSkill(s.id)"
                    size="small"
                  />
                </div>
              </div>
            </div>

            <div class="skills-actions">
              <button class="skills-action-btn" @click="handleScanSkills">🔄 刷新扫描</button>
            </div>
          </div>

          </div><!-- /ai-panel-body -->

          <!-- 固定底栏 -->
          <div class="ai-footer">
            <div class="ai-footer-left">
              <span v-if="saveMsg" class="ai-save-msg">{{ saveMsg }}</span>
              <span v-else-if="isDirty" class="ai-save-msg ai-save-msg--dirty">● 未保存</span>
            </div>
            <div class="ai-footer-right">
              <button class="ai-footer-link" @click="handleReset">重置为默认</button>
            </div>
          </div>
        </div>

        <!-- 模块管理 -->
        <div v-else-if="activeSection === 'plugins'" class="settings-panel">
          <h3 class="panel-title">插件管理</h3>
          <p class="panel-desc">单独控制每个插件的启用/禁用。模块整体开关请在控制台操作。</p>

          <div v-if="pluginList.length === 0" class="plugins-empty">
            还没有注册任何模块哦~
          </div>

          <div v-else class="plugin-groups">
            <div v-for="group in groupedPlugins" :key="group.name" class="plugin-group">
              <h4 class="group-title">{{ group.icon }} {{ group.name }}</h4>
              <div class="plugin-list">
                <div
                  v-for="plugin in group.plugins"
                  :key="plugin.id"
                  class="plugin-item"
                >
                  <div class="plugin-info">
                    <span class="plugin-name">{{ plugin.name }}</span>
                    <span class="plugin-version">v{{ plugin.version }}</span>
                  </div>
                  <p class="plugin-desc">{{ plugin.description }}</p>
                  <div class="plugin-action">
                    <NSwitch
                      :value="plugin.enabled"
                      @update:value="(v: boolean) => handleToggle(plugin.id, v)"
                      size="small"
                    />
                    <span class="plugin-status-text">{{ plugin.enabled ? '已启用' : '已禁用' }}</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.settings-page {
  padding: 1.5rem 2rem;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.settings-title {
  font-size: 1.25rem;
  font-weight: 600;
  margin: 0 0 1rem;
}

/* ── 左右布局 ── */
.settings-body {
  display: flex;
  gap: 1.5rem;
  flex: 1;
  min-height: 0;
}

.settings-sidebar {
  width: 10rem;
  flex-shrink: 0;
}

.settings-content {
  flex: 1;
  min-width: 0;
  overflow-y: auto;
  padding-bottom: 0.5rem;
}

.settings-panel {
  max-width: 36rem;
}

/* AI 配置面板：独立三层结构（标题+滚动体+底栏） */
.ai-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  max-width: 36rem;
  width: 100%;
}

.ai-panel-body {
  flex: 1;
  overflow-y: auto;
  min-height: 0;
  padding-bottom: 0.25rem;
}

.panel-title {
  font-size: 1rem;
  font-weight: 600;
  margin: 0 0 0.25rem;
}

.panel-desc {
  font-size: 0.8125rem;
  color: var(--text-secondary);
  margin: 0 0 1rem;
}

/* ── 插件列表 ── */
.plugins-empty {
  font-size: 0.875rem;
  color: var(--text-muted);
  padding: 2rem 0;
  text-align: center;
}

.plugin-groups {
  display: flex;
  flex-direction: column;
  gap: 1.25rem;
}

.plugin-group {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.group-title {
  font-size: 0.8125rem;
  font-weight: 600;
  margin: 0;
  color: var(--text-primary);
}

.plugin-list {
  display: flex;
  flex-direction: column;
  gap: 0.5rem;
}

.plugin-item {
  padding: 0.75rem 1rem;
  border: 1px solid rgba(128, 128, 128, 0.12);
  border-radius: 0.5rem;
}

.plugin-info {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-bottom: 0.25rem;
}

.plugin-name {
  font-size: 0.875rem;
  font-weight: 600;
}

.plugin-version {
  font-size: 0.6875rem;
  color: var(--text-muted);
}

.plugin-desc {
  font-size: 0.75rem;
  color: var(--text-body);
  margin: 0 0 0.25rem;
  line-height: 1.5;
}

.plugin-action {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin-top: 0.25rem;
}

.plugin-status-text {
  font-size: 0.6875rem;
  color: var(--text-muted);
}

/* ── AI 配置 ── */
/* 卡片分组 */
.ai-card {
  border: 1px solid rgba(128, 128, 128, 0.12);
  border-radius: 10px;
  padding: 1rem 1.125rem;
  display: flex;
  flex-direction: column;
  gap: 0.75rem;
  background: rgba(128, 128, 128, 0.03);
}

.ai-card + .ai-card {
  margin-top: 0.75rem;
}

.ai-card-header {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-primary);
  opacity: 0.7;
  letter-spacing: 0.02em;
}

/* 表单字段 */
.ai-field {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
}

.ai-label {
  font-size: 0.8125rem;
  font-weight: 600;
  color: var(--text-primary);
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.ai-val-tag {
  font-size: 0.6875rem;
  font-weight: 500;
  color: #78b868;
  background: rgba(120, 184, 104, 0.12);
  padding: 0.0625rem 0.375rem;
  border-radius: 4px;
}

.ai-input {
  width: 100%;
  padding: 0.5rem 0.75rem;
  border: 1px solid rgba(128, 128, 128, 0.15);
  border-radius: 6px;
  background: var(--bg-color);
  color: var(--text-primary);
  font-size: 0.8125rem;
  outline: none;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  box-sizing: border-box;
  font-family: inherit;
}

.ai-input:focus {
  border-color: #78b868;
  box-shadow: 0 0 0 2px rgba(120, 184, 104, 0.15);
}

.ai-input--short {
  width: 10rem;
}

/* 输入框内联按钮（API Key 可见性） */
.ai-input-wrap {
  position: relative;
  display: flex;
  align-items: center;
}

.ai-input-wrap .ai-input {
  padding-right: 2.25rem;
}

.ai-input-btn {
  position: absolute;
  right: 0.375rem;
  top: 50%;
  transform: translateY(-50%);
  background: none;
  border: none;
  cursor: pointer;
  font-size: 0.875rem;
  line-height: 1;
  padding: 0.25rem;
  border-radius: 4px;
  color: var(--text-muted);
  transition: color 0.15s;
}

.ai-input-btn:hover {
  color: var(--text-primary);
}

/* 滑块 */
.ai-range-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.ai-range-end {
  font-size: 0.6875rem;
  color: var(--text-muted);
  min-width: 0.75rem;
  text-align: center;
  font-variant-numeric: tabular-nums;
}

.ai-range {
  flex: 1;
  height: 4px;
  -webkit-appearance: none;
  appearance: none;
  background: rgba(128, 128, 128, 0.15);
  border-radius: 2px;
  outline: none;
  cursor: pointer;
}

.ai-range::-webkit-slider-thumb {
  -webkit-appearance: none;
  appearance: none;
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #78b868;
  cursor: pointer;
  border: 2px solid #fff;
  box-shadow: 0 1px 3px rgba(0,0,0,0.2);
}

.ai-range::-moz-range-thumb {
  width: 14px;
  height: 14px;
  border-radius: 50%;
  background: #78b868;
  cursor: pointer;
  border: 2px solid #fff;
}

.ai-hint {
  font-size: 0.6875rem;
  color: var(--text-muted);
  line-height: 1.4;
}

/* 平台选择器 */
.ai-select {
  width: 100%;
  padding: 0.5rem 2rem 0.5rem 0.75rem;
  border: 1px solid rgba(128, 128, 128, 0.15);
  border-radius: 6px;
  background: var(--bg-color);
  color: var(--text-primary);
  font-size: 0.8125rem;
  font-family: inherit;
  outline: none;
  cursor: pointer;
  box-sizing: border-box;
  transition: border-color 0.2s ease, box-shadow 0.2s ease;
  -webkit-appearance: none;
  appearance: none;
  background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'%3E%3Cpath d='M0 0l5 6 5-6' fill='%23999'/%3E%3C/svg%3E");
  background-repeat: no-repeat;
  background-position: right 0.75rem center;
}

.ai-select:hover {
  border-color: #aaa;
}

.ai-select:focus {
  border-color: #78b868;
  box-shadow: 0 0 0 2px rgba(120, 184, 104, 0.15);
}

.ai-select option {
  color: var(--text-primary);
  background: var(--bg-color);
  padding: 0.5rem;
}

/* 只读态 URL 展示 */
.ai-input--readonly {
  display: flex;
  align-items: center;
  color: var(--text-muted);
  font-size: 0.75rem;
  user-select: all;
  cursor: default;
}

/* 测试连接 & 余额 */
.ai-balance-row {
  display: flex;
  align-items: center;
  gap: 0.75rem;
  padding-top: 0.125rem;
  flex-wrap: wrap;
}

.ai-test-btn {
  flex-shrink: 0;
  padding: 0.375rem 0.75rem;
  border: 1px solid rgba(128, 128, 128, 0.15);
  border-radius: 6px;
  background: var(--bg-color);
  color: var(--text-primary);
  font-size: 0.75rem;
  cursor: pointer;
  font-family: inherit;
  transition: border-color 0.2s, background 0.2s;
}

.ai-test-btn:hover:not(:disabled) {
  border-color: #78b868;
  background: rgba(120, 184, 104, 0.06);
}

.ai-test-btn:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.ai-test-btn--danger:hover:not(:disabled) {
  border-color: #e04848;
  background: rgba(224, 72, 72, 0.08);
  color: #e04848;
}

/* ── 平台余额卡片 ── */
.balance-status {
  display: flex;
  align-items: center;
  min-height: 2rem;
}

.balance-status-text {
  font-size: 0.8125rem;
  font-weight: 500;
  line-height: 1.5;
}

.balance-status--loading {
  color: var(--text-muted);
}

.balance-status--ok {
  color: #78b868;
}

.balance-status--error {
  color: #e88b6e;
  font-size: 0.75rem;
}

.balance-status--idle {
  color: var(--text-muted);
  font-size: 0.75rem;
  font-weight: 400;
}

/* ── 余额 & Token 统计卡片 ── */
.balance-stats {
  display: flex;
  gap: 0.5rem;
}

.balance-stat-card {
  flex: 1;
  display: flex;
  flex-direction: column;
  gap: 0.125rem;
  padding: 0.625rem 0.75rem;
  border: 1px solid rgba(128, 128, 128, 0.08);
  border-radius: 8px;
  background: rgba(128, 128, 128, 0.03);
}

.balance-stat-card--total {
  background: rgba(120, 184, 104, 0.05);
  border-color: rgba(120, 184, 104, 0.12);
}

.balance-stat-label {
  font-size: 0.6875rem;
  font-weight: 500;
  color: var(--text-muted);
  letter-spacing: 0.02em;
}

.balance-stat-value {
  font-size: 1rem;
  font-weight: 700;
  color: var(--text-primary);
  font-variant-numeric: tabular-nums;
}

.balance-stat-card--total .balance-stat-value {
  color: #78b868;
}

.balance-stat-sub {
  font-size: 0.625rem;
  color: var(--text-muted);
  margin-top: 0.0625rem;
}

.balance-actions {
  display: flex;
  gap: 0.5rem;
  padding-top: 0.25rem;
}

.ai-balance-text {
  font-size: 0.75rem;
  color: #78b868;
  font-weight: 500;
  line-height: 1.4;
}

.ai-balance-error {
  font-size: 0.6875rem;
  color: #e88b6e;
  line-height: 1.4;
}

/* 底栏 — 固定于面板底部，始终可见 */
.ai-footer {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 0.75rem;
  border-top: 1px solid rgba(128, 128, 128, 0.1);
  background: var(--bg-color);
}

.ai-footer-left {
  display: flex;
  align-items: center;
}

.ai-footer-right {
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.ai-save-msg {
  font-size: 0.75rem;
  color: #78b868;
  font-weight: 500;
}

.ai-save-msg--dirty {
  color: var(--text-muted);
}

.ai-footer-link {
  background: none;
  border: none;
  font-size: 0.75rem;
  color: var(--text-muted);
  cursor: pointer;
  padding: 0.25rem 0.5rem;
  border-radius: 4px;
  transition: color 0.15s, background 0.15s;
  font-family: inherit;
}

.ai-footer-link:hover {
  color: var(--text-primary);
  background: rgba(128, 128, 128, 0.08);
}

/* ── Skills 技能卡片 ── */
.skills-card {
  margin-top: 0.75rem;
}

.skills-card:last-child {
  margin-bottom: 0;
}

.skills-card .ai-card-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.skills-count {
  font-size: 0.6875rem;
  font-weight: 500;
  color: var(--text-muted);
  background: rgba(128, 128, 128, 0.08);
  padding: 0.0625rem 0.5rem;
  border-radius: 8px;
}

.skills-path-row {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.625rem;
  margin-bottom: 0.5rem;
  background: rgba(128, 128, 128, 0.04);
  border-radius: 6px;
  font-size: 0.75rem;
}

.skills-path-label {
  color: var(--text-muted);
  flex-shrink: 0;
}

.skills-path-value {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--text-primary);
  font-family: var(--font-mono);
  font-size: 0.6875rem;
  background: rgba(128, 128, 128, 0.06);
  padding: 0.125rem 0.375rem;
  border-radius: 3px;
}

.skills-path-btn {
  flex-shrink: 0;
  font-size: 0.6875rem;
  padding: 0.125rem 0.5rem;
  border: none;
  border-radius: 4px;
  background: rgba(128, 128, 128, 0.1);
  color: var(--text-secondary);
  cursor: pointer;
  transition: background 0.15s;
}

.skills-path-btn:hover {
  background: rgba(128, 128, 128, 0.18);
}

.skills-path-btn--reset {
  padding: 0.125rem 0.375rem;
  font-size: 0.75rem;
}

.skills-empty {
  text-align: center;
  color: var(--text-muted);
  padding: 2rem 0;
  font-size: 0.8125rem;
}

.skills-list {
  display: flex;
  flex-direction: column;
  gap: 0.25rem;
  padding-bottom: 0.75rem;
}

.skills-item {
  display: flex;
  align-items: center;
  gap: 0.625rem;
  padding: 0.375rem 0.5rem;
  border-radius: 6px;
  transition: background 0.15s;
}

.skills-item:hover {
  background: rgba(128, 128, 128, 0.03);
}

.skills-item-icon {
  flex-shrink: 0;
  font-size: 1rem;
  width: 1.5rem;
  text-align: center;
}

.skills-item-info {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.skills-item-name {
  font-size: 0.8125rem;
  font-weight: 500;
  color: var(--text-primary);
}

.skills-item-tag {
  font-size: 0.625rem;
  padding: 0.0625rem 0.375rem;
  border-radius: 3px;
  font-weight: 500;
  line-height: 1.4;
}

.skills-tag--builtin {
  background: rgba(120, 184, 104, 0.1);
  color: #78b868;
}

.skills-tag--custom {
  background: rgba(100, 150, 255, 0.1);
  color: #6496ff;
}

.skills-item-actions {
  flex-shrink: 0;
  display: flex;
  align-items: center;
  gap: 0.375rem;
}

.skills-item-del {
  background: none;
  border: none;
  cursor: pointer;
  font-size: 0.75rem;
  color: var(--text-muted);
  padding: 0.125rem 0.25rem;
  border-radius: 3px;
  transition: color 0.15s, background 0.15s;
  line-height: 1;
  font-family: inherit;
}

.skills-item-del:hover {
  color: #e88080;
  background: rgba(232, 128, 128, 0.08);
}

.skills-actions {
  padding: 0.5rem 0 0;
  border-top: 1px solid rgba(128, 128, 128, 0.06);
  display: flex;
  align-items: center;
  gap: 0.75rem;
}

.skills-action-btn {
  flex-shrink: 0;
  padding: 0.25rem 0.625rem;
  border: 1px solid rgba(128, 128, 128, 0.12);
  border-radius: 6px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 0.75rem;
  cursor: pointer;
  font-family: inherit;
  transition: border-color 0.2s, color 0.2s, background 0.15s;
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
}

.skills-action-btn:hover {
  border-color: rgba(128, 128, 128, 0.3);
  color: var(--text-primary);
  background: rgba(128, 128, 128, 0.04);
}

/* ── 响应式 ── */
@media (max-width: 640px) {
  .settings-page {
    padding: 1rem;
  }

  .settings-body {
    flex-direction: column;
  }

  .settings-sidebar {
    width: 100%;
  }
}
</style>
