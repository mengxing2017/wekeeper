import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

const apiOrigin = process.env.WEKEEPER_API_ORIGIN ?? 'http://127.0.0.1:8080'

export default defineConfig({
  plugins: [react()],
  optimizeDeps: { exclude: ['@wekeeper/api-contract'] },
  server: {
    host: true, port: 5173, strictPort: true,
    proxy: Object.fromEntries(['/api', '/healthz', '/readyz'].map(path => [path, {
      target: apiOrigin, changeOrigin: true,
    }])),
  },
  test: {
    // Ant Design form interactions can exceed the default 5s under parallel test load.
    testTimeout: 20000,
    // Bound JSDOM/Ant Design workers to avoid CPU contention in CI.
    maxWorkers: 2,
    setupFiles: ['./src/test/setup.ts'],
    environment: 'jsdom',
    clearMocks: true,
  },
})
