import { createApp } from 'vue'
import { createPinia } from 'pinia'
import App from './App.vue'
import router from './router'
import { registerPlugins } from './plugins/manager'
import builtinPlugins from './plugins/builtin'

const app = createApp(App)

// Pinia 状态管理
app.use(createPinia())

// 注册路由
app.use(router)

// 加载内置插件
registerPlugins(builtinPlugins)

app.mount('#app')
