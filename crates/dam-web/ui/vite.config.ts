import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  build: {
    outDir: '../assets',
    emptyOutDir: false,
    sourcemap: false,
    target: 'es2020',
    rollupOptions: {
      input: resolve(__dirname, 'src/main.tsx'),
      output: {
        entryFileNames: 'dam-web-ui.js',
        chunkFileNames: 'dam-web-ui-[name].js',
        assetFileNames: 'dam-web-ui.[ext]',
      },
    },
  },
})
