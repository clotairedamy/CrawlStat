import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    // The utilities are pure functions, so they test as plain Node — no Nuxt
    // runtime, no server to start, and a suite that finishes in under a second.
    environment: 'node',
    include: ['test/**/*.test.ts'],
  },
})
