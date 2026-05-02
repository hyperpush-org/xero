import { fileURLToPath, URL } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': fileURLToPath(new URL('.', import.meta.url)),
    },
  },
  server: {
    host: '0.0.0.0',
    port: 3000,
    strictPort: true,
  },
  preview: {
    host: '0.0.0.0',
    port: 3000,
    strictPort: true,
  },
  build: {
    chunkSizeWarningLimit: 1200,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (/[\\/]src[\\/]features[\\/]solana[\\/]/.test(id)) {
            return 'solana-workbench'
          }

          if (/[\\/]src[\\/]features[\\/]xero[\\/]use-xero-desktop-state(?:[\\/]|\.ts$)/.test(id)) {
            return 'xero-state'
          }

          if (/[\\/]src[\\/]lib[\\/]xero-model(?:[\\/]|\.ts$)/.test(id)) {
            return 'xero-model'
          }

          if (/[\\/]src[\\/]lib[\\/]xero-desktop\.ts$/.test(id)) {
            return 'xero-desktop-adapter'
          }

          if (!/[\\/]node_modules[\\/]/.test(id)) {
            return undefined
          }

          if (/[\\/]node_modules[\\/](?:react|react-dom|scheduler)[\\/]/.test(id)) {
            return 'react-vendor'
          }

          if (/[\\/]node_modules[\\/]@codemirror[\\/](?:lang-|legacy-modes)/.test(id)) {
            return 'codemirror-languages'
          }

          if (/[\\/]node_modules[\\/]@codemirror[\\/]view[\\/]/.test(id)) {
            return 'codemirror-view'
          }

          if (/[\\/]node_modules[\\/]@codemirror[\\/]state[\\/]/.test(id)) {
            return 'codemirror-state'
          }

          if (/[\\/]node_modules[\\/]@lezer[\\/]/.test(id)) {
            return 'codemirror-parser'
          }

          if (/[\\/]node_modules[\\/](?:@codemirror|codemirror)[\\/]/.test(id)) {
            return 'codemirror-core'
          }

          if (/[\\/]node_modules[\\/](?:@radix-ui|cmdk|vaul|sonner|react-day-picker|embla-carousel-react|input-otp|react-resizable-panels)[\\/]/.test(id)) {
            return 'ui-vendor'
          }

          if (/[\\/]node_modules[\\/](?:lucide-react|motion|recharts|date-fns)[\\/]/.test(id)) {
            return 'visual-vendor'
          }

          if (/[\\/]node_modules[\\/](?:zod|react-hook-form|@hookform)[\\/]/.test(id)) {
            return 'form-schema-vendor'
          }

          if (/[\\/]node_modules[\\/](?:shiki|@shikijs|vscode-textmate|vscode-oniguruma)[\\/]/.test(id)) {
            return undefined
          }

          return 'vendor'
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    fileParallelism: false,
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
  },
})
