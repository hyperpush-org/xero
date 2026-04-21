"use client"

import { useMemo } from 'react'
import {
  ChevronDown,
  ChevronRight,
  File as FileIcon,
  FileCode,
  FileJson,
  FileText,
  Folder,
  FolderOpen,
  Image as ImageIcon,
  Settings2,
} from 'lucide-react'
import { cn } from '@/lib/utils'
import type { FileSystemNode } from '@/src/lib/file-system-tree'
import { FileContextMenu } from './file-context-menu'

interface FileTreeProps {
  root: FileSystemNode
  selectedPath: string | null
  expandedFolders: Set<string>
  dirtyPaths?: Set<string>
  searchQuery?: string
  onSelectFile: (path: string) => void
  onToggleFolder: (path: string) => void
  onRequestRename: (path: string, type: 'file' | 'folder') => void
  onRequestDelete: (path: string, type: 'file' | 'folder') => void
  onRequestNewFile: (parentPath: string) => void
  onRequestNewFolder: (parentPath: string) => void
  onCopyPath: (path: string) => void
}

interface MatchInfo {
  matchedPaths: Set<string>
  ancestorPaths: Set<string>
}

function computeSearchMatches(root: FileSystemNode, query: string): MatchInfo | null {
  const q = query.trim().toLowerCase()
  if (!q) return null
  const matched = new Set<string>()
  const ancestors = new Set<string>()

  function walk(node: FileSystemNode, trail: string[]): boolean {
    let any = false
    const nameHit = node.name.toLowerCase().includes(q)
    if (nameHit) {
      matched.add(node.path)
      for (const p of trail) ancestors.add(p)
      any = true
    }
    if (node.children) {
      for (const child of node.children) {
        if (walk(child, [...trail, node.path])) {
          ancestors.add(node.path)
          any = true
        }
      }
    }
    return any
  }

  walk(root, [])
  return { matchedPaths: matched, ancestorPaths: ancestors }
}

export function FileTree({
  root,
  selectedPath,
  expandedFolders,
  dirtyPaths,
  searchQuery = '',
  onSelectFile,
  onToggleFolder,
  onRequestRename,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onCopyPath,
}: FileTreeProps) {
  const search = useMemo(() => computeSearchMatches(root, searchQuery), [root, searchQuery])

  if (!root.children || root.children.length === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
        <Folder className="h-8 w-8 text-muted-foreground/40" />
        <p className="text-xs text-muted-foreground">Workspace is empty</p>
      </div>
    )
  }

  if (search && search.matchedPaths.size === 0) {
    return (
      <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
        <p className="text-[11px] text-muted-foreground">No files match “{searchQuery}”</p>
      </div>
    )
  }

  return (
    <div className="flex-1 overflow-y-auto py-1 scrollbar-thin">
      {root.children.map((child) => (
        <TreeNode
          key={child.id}
          node={child}
          level={0}
          selectedPath={selectedPath}
          expandedFolders={expandedFolders}
          dirtyPaths={dirtyPaths}
          search={search}
          onSelectFile={onSelectFile}
          onToggleFolder={onToggleFolder}
          onRequestRename={onRequestRename}
          onRequestDelete={onRequestDelete}
          onRequestNewFile={onRequestNewFile}
          onRequestNewFolder={onRequestNewFolder}
          onCopyPath={onCopyPath}
        />
      ))}
    </div>
  )
}

interface TreeNodeProps {
  node: FileSystemNode
  level: number
  selectedPath: string | null
  expandedFolders: Set<string>
  dirtyPaths?: Set<string>
  search: MatchInfo | null
  onSelectFile: (path: string) => void
  onToggleFolder: (path: string) => void
  onRequestRename: (path: string, type: 'file' | 'folder') => void
  onRequestDelete: (path: string, type: 'file' | 'folder') => void
  onRequestNewFile: (parentPath: string) => void
  onRequestNewFolder: (parentPath: string) => void
  onCopyPath: (path: string) => void
}

function TreeNode(props: TreeNodeProps) {
  const { node, search } = props
  if (search) {
    const visible = search.matchedPaths.has(node.path) || search.ancestorPaths.has(node.path)
    if (!visible) return null
  }
  return node.type === 'folder' ? <FolderRow {...props} /> : <FileRow {...props} />
}

function FolderRow({
  node,
  level,
  selectedPath,
  expandedFolders,
  dirtyPaths,
  search,
  onSelectFile,
  onToggleFolder,
  onRequestRename,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onCopyPath,
}: TreeNodeProps) {
  const isExpanded = search ? search.ancestorPaths.has(node.path) || expandedFolders.has(node.path) : expandedFolders.has(node.path)

  return (
    <div>
      <FileContextMenu
        path={node.path}
        type="folder"
        onNewFile={() => onRequestNewFile(node.path)}
        onNewFolder={() => onRequestNewFolder(node.path)}
        onRename={() => onRequestRename(node.path, 'folder')}
        onDelete={() => onRequestDelete(node.path, 'folder')}
        onCopyPath={() => onCopyPath(node.path)}
      >
        <button
          type="button"
          onClick={() => onToggleFolder(node.path)}
          className={cn(
            'group flex w-full items-center gap-1 py-[3px] pr-2 text-left text-[12px] leading-5 transition-colors',
            'hover:bg-muted/40 text-foreground/80',
          )}
          style={{ paddingLeft: `${6 + level * 12}px` }}
        >
          <span className="flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground/70">
            {isExpanded ? <ChevronDown className="h-3.5 w-3.5" /> : <ChevronRight className="h-3.5 w-3.5" />}
          </span>
          <span className="flex h-4 w-4 shrink-0 items-center justify-center text-chart-1">
            {isExpanded ? <FolderOpen className="h-3.5 w-3.5" /> : <Folder className="h-3.5 w-3.5" />}
          </span>
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
        </button>
      </FileContextMenu>

      {isExpanded && node.children && (
        <div>
          {node.children.map((child) => (
            <TreeNode
              key={child.id}
              node={child}
              level={level + 1}
              selectedPath={selectedPath}
              expandedFolders={expandedFolders}
              dirtyPaths={dirtyPaths}
              search={search}
              onSelectFile={onSelectFile}
              onToggleFolder={onToggleFolder}
              onRequestRename={onRequestRename}
              onRequestDelete={onRequestDelete}
              onRequestNewFile={onRequestNewFile}
              onRequestNewFolder={onRequestNewFolder}
              onCopyPath={onCopyPath}
            />
          ))}
        </div>
      )}
    </div>
  )
}

function FileRow({
  node,
  level,
  selectedPath,
  dirtyPaths,
  onSelectFile,
  onRequestRename,
  onRequestDelete,
  onCopyPath,
}: TreeNodeProps) {
  const isSelected = selectedPath === node.path
  const isDirty = dirtyPaths?.has(node.path) ?? false

  return (
    <FileContextMenu
      path={node.path}
      type="file"
      onRename={() => onRequestRename(node.path, 'file')}
      onDelete={() => onRequestDelete(node.path, 'file')}
      onCopyPath={() => onCopyPath(node.path)}
    >
      <button
        type="button"
        onClick={() => onSelectFile(node.path)}
        className={cn(
          'group flex w-full items-center gap-1 py-[3px] pr-2 text-left text-[12px] leading-5 transition-colors',
          isSelected
            ? 'bg-primary/15 text-foreground'
            : 'text-foreground/75 hover:bg-muted/40 hover:text-foreground',
        )}
        style={{ paddingLeft: `${6 + level * 12 + 16}px` }}
      >
        <span className="flex h-4 w-4 shrink-0 items-center justify-center">{getFileIcon(node.name)}</span>
        <span className="min-w-0 flex-1 truncate">{node.name}</span>
        {isDirty ? (
          <span
            className="ml-1 h-1.5 w-1.5 shrink-0 rounded-full bg-primary"
            aria-label="Unsaved changes"
          />
        ) : null}
      </button>
    </FileContextMenu>
  )
}

export function getFileIcon(filename: string, className = 'h-3.5 w-3.5'): React.ReactNode {
  const lower = filename.toLowerCase()
  const ext = lower.includes('.') ? lower.slice(lower.lastIndexOf('.') + 1) : ''

  if (!ext && !lower.startsWith('.')) {
    return <FileIcon className={cn(className, 'text-muted-foreground')} />
  }

  if (lower === 'package.json' || lower === 'package-lock.json' || lower === 'pnpm-lock.yaml') {
    return <FileJson className={cn(className, 'text-rose-400')} />
  }

  if (ext === 'tsx' || ext === 'jsx') {
    return <FileCode className={cn(className, 'text-sky-400')} />
  }
  if (ext === 'ts') return <FileCode className={cn(className, 'text-blue-400')} />
  if (ext === 'js' || ext === 'mjs' || ext === 'cjs') {
    return <FileCode className={cn(className, 'text-yellow-400')} />
  }
  if (ext === 'py') return <FileCode className={cn(className, 'text-emerald-400')} />
  if (ext === 'rs') return <FileCode className={cn(className, 'text-orange-400')} />
  if (ext === 'go') return <FileCode className={cn(className, 'text-cyan-400')} />
  if (['java', 'c', 'cpp', 'h', 'hpp'].includes(ext)) {
    return <FileCode className={cn(className, 'text-indigo-400')} />
  }

  if (ext === 'css' || ext === 'scss') {
    return <FileCode className={cn(className, 'text-fuchsia-400')} />
  }
  if (ext === 'html' || ext === 'htm' || ext === 'vue' || ext === 'svelte') {
    return <FileCode className={cn(className, 'text-orange-300')} />
  }

  if (ext === 'json' || ext === 'jsonc') {
    return <FileJson className={cn(className, 'text-amber-300')} />
  }

  if (['md', 'mdx', 'txt'].includes(ext)) {
    return <FileText className={cn(className, 'text-slate-300')} />
  }
  if (['yaml', 'yml', 'toml'].includes(ext)) {
    return <FileText className={cn(className, 'text-amber-400')} />
  }

  if (['png', 'jpg', 'jpeg', 'gif', 'webp', 'ico'].includes(ext)) {
    return <ImageIcon className={cn(className, 'text-violet-400')} />
  }
  if (ext === 'svg') return <ImageIcon className={cn(className, 'text-purple-400')} />

  if (
    ['lock', 'config', 'env', 'gitignore', 'editorconfig', 'dockerignore'].includes(ext) ||
    lower.startsWith('.')
  ) {
    return <Settings2 className={cn(className, 'text-muted-foreground')} />
  }

  return <FileIcon className={cn(className, 'text-muted-foreground')} />
}
