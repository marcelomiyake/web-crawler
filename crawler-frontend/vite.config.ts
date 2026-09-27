import { defineConfig } from 'vitest/config'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  plugins: [vue()],
  test: {
    include: ['src/**/*.spec.ts'],
    environment: 'jsdom',
    clearMocks: true,
    coverage: {
      include: ['src/App.vue'],
      reportsDirectory: './coverage',
      reporter: ['text', 'lcov'],
    },
  },
  server: {
    proxy: { '/api': 'http://127.0.0.1:8081' },
  },
})
