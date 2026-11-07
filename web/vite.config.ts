import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  worker: { format: 'es' },
  build: {
    rolldownOptions: {
      input: {
        chooser: resolve(import.meta.dirname, 'index.html'),
        poe1: resolve(import.meta.dirname, 'poe1/index.html'),
        simple: resolve(import.meta.dirname, 'simple/index.html'),
        poe2: resolve(import.meta.dirname, 'poe2/index.html'),
      },
    },
  },
})
