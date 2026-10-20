import { fileURLToPath, URL } from 'node:url'
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

const apiOrigin = process.env.WEKEEPER_API_ORIGIN ?? 'http://127.0.0.1:8080'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@wekeeper/api-contract/business': fileURLToPath(new URL('../api-contract/src/modules/business/index.ts', import.meta.url)),
      '@wekeeper/api-contract/http': fileURLToPath(new URL('../api-contract/src/http/index.ts', import.meta.url)),
      '@wekeeper/api-contract/system': fileURLToPath(new URL('../api-contract/src/modules/system/index.ts', import.meta.url)),
      '@wekeeper/api-contract/health': fileURLToPath(new URL('../api-contract/src/modules/health/index.ts', import.meta.url)),
    },
  },
  optimizeDeps: { exclude: ['@wekeeper/api-contract'] },
  server: {
    host: true, port: 5173, strictPort: true,
    proxy: Object.fromEntries(['/api', '/healthz', '/readyz'].map(path => [path, {
      target: apiOrigin, changeOrigin: true,
    }])),
  },
  test: { environment: 'jsdom', clearMocks: true },
})
