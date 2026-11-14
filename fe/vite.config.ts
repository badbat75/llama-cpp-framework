/// <reference types="vitest/config" />
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// https://vite.dev/config/ ; the `test` block is Vitest's
// (https://vitest.dev/config/): the components render in jsdom.
export default defineConfig({
  plugins: [react()],
  test: {
    environment: 'jsdom',
  },
})
