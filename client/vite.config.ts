import { fileURLToPath, URL } from 'node:url'
import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: [
      // Specific aliases must come before the catch-all `@`. Vite picks the first match.
      { find: '@/components/ui', replacement: fileURLToPath(new URL('../packages/ui/src/components/ui', import.meta.url)) },
      { find: '@/lib/utils', replacement: fileURLToPath(new URL('../packages/ui/src/lib/utils.ts', import.meta.url)) },
      { find: '@/lib/byte-budget-cache', replacement: fileURLToPath(new URL('../packages/ui/src/lib/byte-budget-cache.ts', import.meta.url)) },
      { find: '@/lib/shiki', replacement: fileURLToPath(new URL('../packages/ui/src/lib/shiki.ts', import.meta.url)) },
      { find: '@/lib/language-detection', replacement: fileURLToPath(new URL('../packages/ui/src/lib/language-detection.ts', import.meta.url)) },
      // Sub-modules of xero-model that moved into the shared package. Listed
      // individually so the remaining client-local files in xero-model/ still
      // resolve via the catch-all `@` alias below.
      { find: '@/src/lib/xero-model/runtime-stream', replacement: fileURLToPath(new URL('../packages/ui/src/model/runtime-stream.ts', import.meta.url)) },
      { find: '@/src/lib/xero-model/runtime', replacement: fileURLToPath(new URL('../packages/ui/src/model/runtime.ts', import.meta.url)) },
      { find: '@/src/lib/xero-model/shared', replacement: fileURLToPath(new URL('../packages/ui/src/model/shared.ts', import.meta.url)) },
      { find: '@/src/lib/xero-model/code-history', replacement: fileURLToPath(new URL('../packages/ui/src/model/code-history.ts', import.meta.url)) },
      { find: '@xero/ui', replacement: fileURLToPath(new URL('../packages/ui/src', import.meta.url)) },
      { find: '@', replacement: fileURLToPath(new URL('.', import.meta.url)) },
    ],
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
    chunkSizeWarningLimit: 3500,
    rollupOptions: {
      output: {
        manualChunks(id) {
          const normalizedId = id.replace(/\\/g, '/')

          if (
            normalizedId.includes('/node_modules/') &&
            (
              normalizedId.includes('/@tauri-apps/') ||
              normalizedId.includes('/@tauri-apps+') ||
              normalizedId.includes('@tauri-apps_')
            )
          ) {
            return 'tauri-api'
          }

          if (/[/]src[/]features[/]xero[/]use-xero-desktop-state(?:[/]|\.ts$)/.test(normalizedId)) {
            return 'xero-state'
          }

          if (/[/]src[/]lib[/]xero-model(?:[/]|\.ts$)/.test(normalizedId)) {
            return 'xero-model'
          }

          if (/[/]src[/]lib[/]xero-desktop\.ts$/.test(normalizedId)) {
            return 'xero-desktop-adapter'
          }

          if (normalizedId.includes('/packages/ui/src/')) {
            return 'xero-ui'
          }

          if (!normalizedId.includes('/node_modules/')) {
            return undefined
          }

          if (/[/]node_modules[/](?:react|react-dom|scheduler)[/]/.test(normalizedId)) {
            return 'react-vendor'
          }

          const codeMirrorLanguageMatch = normalizedId.match(/[/]node_modules[/]@codemirror[/](lang-[^/]+|legacy-modes)(?:[/]|$)/)
          if (codeMirrorLanguageMatch) {
            const packageName = codeMirrorLanguageMatch[1]
            if (packageName === 'legacy-modes') {
              const legacyModeMatch = normalizedId.match(/[/]legacy-modes[/]mode[/]([^/]+)(?:\.|$)/)
              return legacyModeMatch ? `codemirror-legacy-${legacyModeMatch[1]}` : 'codemirror-legacy-modes'
            }

            return `codemirror-${packageName}`
          }

          if (/[/]node_modules[/]@codemirror[/]view[/]/.test(normalizedId)) {
            return 'codemirror-view'
          }

          if (/[/]node_modules[/]@codemirror[/]state[/]/.test(normalizedId)) {
            return 'codemirror-state'
          }

          if (/[/]node_modules[/]@lezer[/]/.test(normalizedId)) {
            return 'codemirror-parser'
          }

          if (/[/]node_modules[/](?:@codemirror|codemirror)[/]/.test(normalizedId)) {
            return 'codemirror-core'
          }

          if (/[/]node_modules[/](?:@radix-ui|cmdk|vaul|sonner|react-day-picker|embla-carousel-react|input-otp|react-resizable-panels)[/]/.test(normalizedId)) {
            return 'ui-vendor'
          }

          if (/[/]node_modules[/](?:lucide-react|motion|recharts|date-fns)[/]/.test(normalizedId)) {
            return 'visual-vendor'
          }

          if (/[/]node_modules[/](?:zod|react-hook-form|@hookform)[/]/.test(normalizedId)) {
            return 'form-schema-vendor'
          }

          if (/[/]node_modules[/](?:shiki|@shikijs|vscode-textmate|vscode-oniguruma)[/]/.test(normalizedId)) {
            return undefined
          }

          // Mermaid (~1MB) is dynamically imported on first diagram render — keep it
          // (and its transitive d3 / dagre / cytoscape graph libs) in a dedicated chunk
          // so cold start does not pay for it.
          if (
            /[/]node_modules[/](?:mermaid|@mermaid-js|d3|d3-[^/]+|dagre|dagre-d3-es|cytoscape|cytoscape-[^/]+|elkjs|katex|khroma|@braintree|dompurify|@iconify|roughjs|robust-predicates|delaunator|internmap|robust-orientation|robust-product|robust-sum|robust-scale|robust-compress|robust-add)[/]/.test(normalizedId)
          ) {
            return 'mermaid'
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
