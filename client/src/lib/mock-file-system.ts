export interface FileSystemNode {
  id: string
  name: string
  type: 'file' | 'folder'
  path: string
  children?: FileSystemNode[]
  content?: string
}

// ---------------------------------------------------------------------------
// Seed builders
// ---------------------------------------------------------------------------

let nextId = 1
function genId(): string {
  return `fs-${nextId++}`
}

function file(name: string, content: string): Omit<FileSystemNode, 'path'> {
  return { id: genId(), name, type: 'file', content }
}

function folder(name: string, children: Array<Omit<FileSystemNode, 'path'>>): Omit<FileSystemNode, 'path'> {
  return { id: genId(), name, type: 'folder', children: children as FileSystemNode[] }
}

function assignPaths(node: Omit<FileSystemNode, 'path'>, parentPath: string): FileSystemNode {
  const path = parentPath === '' ? `/${node.name}` : `${parentPath}/${node.name}`
  const result: FileSystemNode = { ...node, path } as FileSystemNode
  if (result.children) {
    result.children = result.children.map((child) => assignPaths(child, path))
  }
  return result
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export function createMockFileSystem(): FileSystemNode {
  const root: Omit<FileSystemNode, 'path'> = folder('', [
    folder('src', [
      file(
        'App.tsx',
        `import { useState } from 'react'
import { Button } from './components/Button'
import { Card } from './components/Card'
import { cn } from './lib/utils'

export default function App() {
  const [count, setCount] = useState(0)

  return (
    <main className={cn('min-h-screen bg-background p-8')}>
      <Card className="mx-auto max-w-md p-6">
        <h1 className="text-2xl font-semibold">Hello, Cadence</h1>
        <p className="mt-2 text-muted-foreground">
          Clicked {count} {count === 1 ? 'time' : 'times'}
        </p>
        <Button className="mt-4" onClick={() => setCount((c) => c + 1)}>
          Increment
        </Button>
      </Card>
    </main>
  )
}
`,
      ),
      file(
        'main.tsx',
        `import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
`,
      ),
      file(
        'index.css',
        `@tailwind base;
@tailwind components;
@tailwind utilities;

:root {
  --background: 0 0% 100%;
  --foreground: 222 47% 11%;
  --muted-foreground: 215 16% 47%;
}

html, body, #root {
  height: 100%;
}

body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, 'Inter', sans-serif;
}
`,
      ),
      folder('components', [
        file(
          'Button.tsx',
          `import { cn } from '@/lib/utils'

export interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: 'default' | 'destructive' | 'outline'
}

export function Button({ className, variant = 'default', ...props }: ButtonProps) {
  return (
    <button
      className={cn(
        'inline-flex items-center justify-center rounded-md px-4 py-2 text-sm font-medium transition-colors',
        variant === 'default' && 'bg-primary text-primary-foreground hover:bg-primary/90',
        variant === 'destructive' && 'bg-destructive text-destructive-foreground hover:bg-destructive/90',
        variant === 'outline' && 'border border-border hover:bg-accent',
        className,
      )}
      {...props}
    />
  )
}
`,
        ),
        file(
          'Card.tsx',
          `import { cn } from '@/lib/utils'

export interface CardProps extends React.HTMLAttributes<HTMLDivElement> {}

export function Card({ className, ...props }: CardProps) {
  return (
    <div
      className={cn('rounded-lg border border-border bg-card text-card-foreground shadow-sm', className)}
      {...props}
    />
  )
}
`,
        ),
        file(
          'Layout.tsx',
          `import { cn } from '@/lib/utils'

interface LayoutProps {
  children: React.ReactNode
  className?: string
}

export function Layout({ children, className }: LayoutProps) {
  return (
    <div className={cn('mx-auto w-full max-w-5xl px-6 py-10', className)}>{children}</div>
  )
}
`,
        ),
      ]),
      folder('lib', [
        file(
          'utils.ts',
          `import { clsx, type ClassValue } from 'clsx'
import { twMerge } from 'tailwind-merge'

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs))
}

export function formatDate(date: Date): string {
  return new Intl.DateTimeFormat('en-US', {
    year: 'numeric',
    month: 'long',
    day: 'numeric',
  }).format(date)
}
`,
        ),
        file(
          'api.ts',
          `const API_BASE = import.meta.env.VITE_API_BASE ?? '/api'

export async function get<T>(url: string): Promise<T> {
  const response = await fetch(\`\${API_BASE}\${url}\`)
  if (!response.ok) {
    throw new Error(\`Request failed: \${response.statusText}\`)
  }
  return response.json()
}

export async function post<T>(url: string, data: unknown): Promise<T> {
  const response = await fetch(\`\${API_BASE}\${url}\`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!response.ok) {
    throw new Error(\`Request failed: \${response.statusText}\`)
  }
  return response.json()
}
`,
        ),
      ]),
      folder('hooks', [
        file(
          'use-theme.ts',
          `import { useEffect, useState } from 'react'

type Theme = 'light' | 'dark'

export function useTheme(): [Theme, (t: Theme) => void] {
  const [theme, setTheme] = useState<Theme>(() =>
    (localStorage.getItem('theme') as Theme) ?? 'dark',
  )

  useEffect(() => {
    document.documentElement.classList.toggle('dark', theme === 'dark')
    localStorage.setItem('theme', theme)
  }, [theme])

  return [theme, setTheme]
}
`,
        ),
      ]),
    ]),
    folder('public', [
      file(
        'favicon.svg',
        `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="currentColor">
  <path d="M4 4h6v6H4V4Zm10 0h6v6h-6V4ZM4 14h6v6H4v-6Zm10 0h6v6h-6v-6Z" />
</svg>
`,
      ),
    ]),
    file(
      'package.json',
      `{
  "name": "cadence-sandbox",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "vite build",
    "preview": "vite preview"
  },
  "dependencies": {
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@types/react": "^19.0.0",
    "@vitejs/plugin-react": "^4.0.0",
    "typescript": "^5.3.0",
    "vite": "^5.0.0"
  }
}
`,
    ),
    file(
      'tsconfig.json',
      `{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "bundler",
    "jsx": "react-jsx",
    "strict": true,
    "skipLibCheck": true,
    "baseUrl": ".",
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["src"]
}
`,
    ),
    file(
      'vite.config.ts',
      `import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
})
`,
    ),
    file(
      'README.md',
      `# Cadence Sandbox

A minimal React + Vite playground used to explore the Cadence editor surface.

## Getting started

\`\`\`bash
pnpm install
pnpm dev
\`\`\`

## Layout

- \`src/components\` — UI primitives
- \`src/lib\` — utilities, API helpers
- \`src/hooks\` — custom React hooks
`,
    ),
    file('.gitignore', `node_modules\ndist\n.env\n.env.local\n*.log\n`),
  ])

  const tree = assignPaths(root, '')
  tree.path = '/'
  tree.name = 'root'
  return tree
}

// ---------------------------------------------------------------------------
// Pure helpers — each returns a new tree
// ---------------------------------------------------------------------------

export function findNode(root: FileSystemNode, path: string): FileSystemNode | null {
  if (root.path === path) return root
  if (!root.children) return null
  for (const child of root.children) {
    const found = findNode(child, path)
    if (found) return found
  }
  return null
}

function cloneTree(node: FileSystemNode): FileSystemNode {
  return {
    ...node,
    children: node.children ? node.children.map(cloneTree) : undefined,
  }
}

function findParentOf(root: FileSystemNode, childPath: string): FileSystemNode | null {
  if (!root.children) return null
  for (const child of root.children) {
    if (child.path === childPath) return root
    const found = findParentOf(child, childPath)
    if (found) return found
  }
  return null
}

function sortChildren(node: FileSystemNode): void {
  if (!node.children) return
  node.children.sort((a, b) => {
    if (a.type !== b.type) return a.type === 'folder' ? -1 : 1
    return a.name.localeCompare(b.name)
  })
}

function reassignDescendantPaths(node: FileSystemNode, newBasePath: string): void {
  node.path = newBasePath
  if (!node.children) return
  for (const child of node.children) {
    reassignDescendantPaths(child, `${newBasePath}/${child.name}`)
  }
}

export function updateFileContent(root: FileSystemNode, path: string, content: string): FileSystemNode {
  const clone = cloneTree(root)
  const node = findNode(clone, path)
  if (node && node.type === 'file') {
    node.content = content
  }
  return clone
}

export function deleteNode(root: FileSystemNode, path: string): FileSystemNode {
  const clone = cloneTree(root)
  const parent = findParentOf(clone, path)
  if (!parent || !parent.children) return clone
  parent.children = parent.children.filter((c) => c.path !== path)
  return clone
}

export function createChild(
  root: FileSystemNode,
  parentPath: string,
  name: string,
  type: 'file' | 'folder',
  content = '',
): { tree: FileSystemNode; createdPath: string | null } {
  const clone = cloneTree(root)
  const parent = findNode(clone, parentPath)
  if (!parent || parent.type !== 'folder') return { tree: clone, createdPath: null }

  const newPath = parentPath === '/' ? `/${name}` : `${parentPath}/${name}`
  if (parent.children?.some((c) => c.name === name)) {
    return { tree: clone, createdPath: null }
  }

  const newNode: FileSystemNode = {
    id: genId(),
    name,
    type,
    path: newPath,
    ...(type === 'file' ? { content } : { children: [] }),
  }

  parent.children = [...(parent.children ?? []), newNode]
  sortChildren(parent)
  return { tree: clone, createdPath: newPath }
}

export function renameNodeByPath(
  root: FileSystemNode,
  oldPath: string,
  newName: string,
): { tree: FileSystemNode; newPath: string | null } {
  const clone = cloneTree(root)
  const parent = findParentOf(clone, oldPath)
  if (!parent) return { tree: clone, newPath: null }

  if (parent.children?.some((c) => c.name === newName && c.path !== oldPath)) {
    return { tree: clone, newPath: null }
  }

  const node = parent.children?.find((c) => c.path === oldPath)
  if (!node) return { tree: clone, newPath: null }

  const parentPath = parent.path === '/' ? '' : parent.path
  const newPath = `${parentPath}/${newName}`
  node.name = newName
  reassignDescendantPaths(node, newPath)
  sortChildren(parent)
  return { tree: clone, newPath }
}

export function listAllFolderPaths(root: FileSystemNode): string[] {
  const paths: string[] = []
  function walk(node: FileSystemNode) {
    if (node.type === 'folder') paths.push(node.path)
    node.children?.forEach(walk)
  }
  walk(root)
  return paths
}

export function listAllFilePaths(root: FileSystemNode): string[] {
  const paths: string[] = []
  function walk(node: FileSystemNode) {
    if (node.type === 'file') paths.push(node.path)
    node.children?.forEach(walk)
  }
  walk(root)
  return paths
}

export function collectFileContents(root: FileSystemNode): Record<string, string> {
  const contents: Record<string, string> = {}
  function walk(node: FileSystemNode) {
    if (node.type === 'file' && node.content !== undefined) {
      contents[node.path] = node.content
    }
    node.children?.forEach(walk)
  }
  walk(root)
  return contents
}
