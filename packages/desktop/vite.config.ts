import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    // 手动代码拆分：分离 naive-ui 和共享工具
    rollupOptions: {
      output: {
        manualChunks: (id) => {
          // naive-ui 统一拆到一个 chunk
          if (id.includes('node_modules/naive-ui')) return 'naive-ui'
          // 页面级组件各自独立
          if (id.includes('/views/')) {
            const name = id.split('/views/')[1].split('.')[0]
            return `page-${name.toLowerCase()}`
          }
        },
      },
    },
    // 目标现代浏览器，减小 polyfill 体积
    target: 'es2021',
    // 体积警告阈值：naive-ui 按需导入后约 510 kB，已独立分包且仅启动时加载一次；
    // 桌面应用资源从本地磁盘读取，500 kB 默认阈值针对网络传输场景，此处不适用
    chunkSizeWarningLimit: 800,
  },
}));
