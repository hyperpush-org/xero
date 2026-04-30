"use client"

import { useEffect, useMemo, useRef, useState, type FormEvent } from 'react'
import {
  Check,
  ChevronDown,
  ChevronRight,
  File as FileIcon,
  FileCode,
  FileJson,
  FilePlus,
  FileText,
  Folder,
  FolderPlus,
  FolderOpen,
  Image as ImageIcon,
  Settings2,
  X,
} from 'lucide-react'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { FileSystemNode } from '@/src/lib/file-system-tree'
import { FileContextMenu } from './file-context-menu'

const FILE_TREE_DRAG_TYPE = 'application/x-xero-project-entry'

interface FileTreeProps {
  root: FileSystemNode
  selectedPath: string | null
  expandedFolders: Set<string>
  dirtyPaths?: Set<string>
  searchQuery?: string
  creatingEntry?: { parentPath: string; type: 'file' | 'folder' } | null
  onSelectFile: (path: string) => void
  onToggleFolder: (path: string) => void
  onRequestRename: (path: string, type: 'file' | 'folder') => void
  onRequestDelete: (path: string, type: 'file' | 'folder') => void
  onRequestNewFile: (parentPath: string) => void
  onRequestNewFolder: (parentPath: string) => void
  onMoveEntry: (path: string, targetParentPath: string) => void
  onCancelCreate: () => void
  onCreateEntry: (name: string) => Promise<string | null>
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
  creatingEntry = null,
  onSelectFile,
  onToggleFolder,
  onRequestRename,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onMoveEntry,
  onCancelCreate,
  onCreateEntry,
  onCopyPath,
}: FileTreeProps) {
  const search = useMemo(() => computeSearchMatches(root, searchQuery), [root, searchQuery])
  const [draggingPath, setDraggingPath] = useState<string | null>(null)
  const [dropTargetPath, setDropTargetPath] = useState<string | null>(null)

  const handleDropOnFolder = (event: React.DragEvent, targetParentPath: string) => {
    event.preventDefault()
    event.stopPropagation()
    const path = event.dataTransfer.getData(FILE_TREE_DRAG_TYPE)
    setDropTargetPath(null)
    setDraggingPath(null)
    if (!path || path === targetParentPath || targetParentPath.startsWith(`${path}/`)) {
      return
    }
    onMoveEntry(path, targetParentPath)
  }

  const rootCreateRow = creatingEntry?.parentPath === '/' ? (
    <InlineCreateRow
      key="create:/"
      level={0}
      type={creatingEntry.type}
      onCancel={onCancelCreate}
      onCreate={onCreateEntry}
    />
  ) : null

  if (!root.children || root.children.length === 0) {
    return (
      <FileContextMenu
        type="folder"
        onNewFile={() => onRequestNewFile('/')}
        onNewFolder={() => onRequestNewFolder('/')}
        onCopyPath={() => onCopyPath('/')}
      >
        <div
          className="flex flex-1 flex-col"
          onDragOver={(event) => {
            event.preventDefault()
            setDropTargetPath('/')
          }}
          onDrop={(event) => handleDropOnFolder(event, '/')}
        >
          {rootCreateRow}
          <div className="flex flex-1 flex-col items-center justify-center gap-2 p-6 text-center">
            <Folder className="h-8 w-8 text-muted-foreground/40" />
            <p className="text-xs text-muted-foreground">Workspace is empty</p>
          </div>
        </div>
      </FileContextMenu>
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
    <FileContextMenu
      type="folder"
      onNewFile={() => onRequestNewFile('/')}
      onNewFolder={() => onRequestNewFolder('/')}
      onCopyPath={() => onCopyPath('/')}
    >
      <div
        className={cn('flex-1 overflow-y-auto py-1 scrollbar-thin', dropTargetPath === '/' && 'bg-primary/5')}
        onDragLeave={(event) => {
          if (event.currentTarget === event.target) setDropTargetPath(null)
        }}
        onDragOver={(event) => {
          event.preventDefault()
          setDropTargetPath('/')
        }}
        onDrop={(event) => handleDropOnFolder(event, '/')}
      >
        {rootCreateRow}
        {root.children.map((child) => (
          <TreeNode
            key={child.id}
            node={child}
            level={0}
            selectedPath={selectedPath}
            expandedFolders={expandedFolders}
            dirtyPaths={dirtyPaths}
            search={search}
            creatingEntry={creatingEntry}
            draggingPath={draggingPath}
            dropTargetPath={dropTargetPath}
            onDragStart={setDraggingPath}
            onDragEnd={() => {
              setDraggingPath(null)
              setDropTargetPath(null)
            }}
            onDropTargetChange={setDropTargetPath}
            onDropOnFolder={handleDropOnFolder}
            onSelectFile={onSelectFile}
            onToggleFolder={onToggleFolder}
            onRequestRename={onRequestRename}
            onRequestDelete={onRequestDelete}
            onRequestNewFile={onRequestNewFile}
            onRequestNewFolder={onRequestNewFolder}
            onCancelCreate={onCancelCreate}
            onCreateEntry={onCreateEntry}
            onCopyPath={onCopyPath}
          />
        ))}
      </div>
    </FileContextMenu>
  )
}

interface TreeNodeProps {
  node: FileSystemNode
  level: number
  selectedPath: string | null
  expandedFolders: Set<string>
  dirtyPaths?: Set<string>
  search: MatchInfo | null
  creatingEntry: { parentPath: string; type: 'file' | 'folder' } | null
  draggingPath: string | null
  dropTargetPath: string | null
  onDragStart: (path: string) => void
  onDragEnd: () => void
  onDropTargetChange: (path: string | null) => void
  onDropOnFolder: (event: React.DragEvent, targetParentPath: string) => void
  onSelectFile: (path: string) => void
  onToggleFolder: (path: string) => void
  onRequestRename: (path: string, type: 'file' | 'folder') => void
  onRequestDelete: (path: string, type: 'file' | 'folder') => void
  onRequestNewFile: (parentPath: string) => void
  onRequestNewFolder: (parentPath: string) => void
  onCancelCreate: () => void
  onCreateEntry: (name: string) => Promise<string | null>
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
  creatingEntry,
  draggingPath,
  dropTargetPath,
  onDragStart,
  onDragEnd,
  onDropTargetChange,
  onDropOnFolder,
  onSelectFile,
  onToggleFolder,
  onRequestRename,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onCancelCreate,
  onCreateEntry,
  onCopyPath,
}: TreeNodeProps) {
  const isExpanded = search ? search.ancestorPaths.has(node.path) || expandedFolders.has(node.path) : expandedFolders.has(node.path)
  const isDropTarget =
    dropTargetPath === node.path &&
    draggingPath !== node.path &&
    !node.path.startsWith(`${draggingPath ?? ''}/`)

  return (
    <div>
      <FileContextMenu
        type="folder"
        onNewFile={() => onRequestNewFile(node.path)}
        onNewFolder={() => onRequestNewFolder(node.path)}
        onRename={() => onRequestRename(node.path, 'folder')}
        onDelete={() => onRequestDelete(node.path, 'folder')}
        onCopyPath={() => onCopyPath(node.path)}
      >
        <button
          type="button"
          draggable
          onClick={() => onToggleFolder(node.path)}
          onDragEnd={onDragEnd}
          onDragStart={(event) => {
            event.dataTransfer.effectAllowed = 'move'
            event.dataTransfer.setData(FILE_TREE_DRAG_TYPE, node.path)
            onDragStart(node.path)
          }}
          onDragLeave={() => onDropTargetChange(null)}
          onDragOver={(event) => {
            if (draggingPath && draggingPath !== node.path && !node.path.startsWith(`${draggingPath}/`)) {
              event.preventDefault()
              event.stopPropagation()
              event.dataTransfer.dropEffect = 'move'
              onDropTargetChange(node.path)
            }
          }}
          onDrop={(event) => onDropOnFolder(event, node.path)}
          className={cn(
            'group flex w-full items-center gap-1 py-[3px] pr-2 text-left text-[12px] leading-5 transition-colors',
            'hover:bg-muted/40 text-foreground/80',
            draggingPath === node.path && 'opacity-50',
            isDropTarget && 'bg-primary/12 text-foreground',
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
          {creatingEntry?.parentPath === node.path ? (
            <InlineCreateRow
              level={level + 1}
              type={creatingEntry.type}
              onCancel={onCancelCreate}
              onCreate={onCreateEntry}
            />
          ) : null}
          {node.children.map((child) => (
            <TreeNode
              key={child.id}
              node={child}
              level={level + 1}
              selectedPath={selectedPath}
              expandedFolders={expandedFolders}
              dirtyPaths={dirtyPaths}
              search={search}
              creatingEntry={creatingEntry}
              draggingPath={draggingPath}
              dropTargetPath={dropTargetPath}
              onDragStart={onDragStart}
              onDragEnd={onDragEnd}
              onDropTargetChange={onDropTargetChange}
              onDropOnFolder={onDropOnFolder}
              onSelectFile={onSelectFile}
              onToggleFolder={onToggleFolder}
              onRequestRename={onRequestRename}
              onRequestDelete={onRequestDelete}
              onRequestNewFile={onRequestNewFile}
              onRequestNewFolder={onRequestNewFolder}
              onCancelCreate={onCancelCreate}
              onCreateEntry={onCreateEntry}
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
  draggingPath,
  onDragStart,
  onDragEnd,
  onSelectFile,
  onRequestRename,
  onRequestDelete,
  onCopyPath,
}: TreeNodeProps) {
  const isSelected = selectedPath === node.path
  const isDirty = dirtyPaths?.has(node.path) ?? false

  return (
    <FileContextMenu
      type="file"
      onRename={() => onRequestRename(node.path, 'file')}
      onDelete={() => onRequestDelete(node.path, 'file')}
      onCopyPath={() => onCopyPath(node.path)}
    >
      <button
        type="button"
        draggable
        onClick={() => onSelectFile(node.path)}
        onDragEnd={onDragEnd}
        onDragStart={(event) => {
          event.dataTransfer.effectAllowed = 'move'
          event.dataTransfer.setData(FILE_TREE_DRAG_TYPE, node.path)
          onDragStart(node.path)
        }}
        className={cn(
          'group flex w-full items-center gap-1 py-[3px] pr-2 text-left text-[12px] leading-5 transition-colors',
          isSelected
            ? 'bg-primary/15 text-foreground'
            : 'text-foreground/75 hover:bg-muted/40 hover:text-foreground',
          draggingPath === node.path && 'opacity-50',
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

function InlineCreateRow({
  level,
  type,
  onCancel,
  onCreate,
}: {
  level: number
  type: 'file' | 'folder'
  onCancel: () => void
  onCreate: (name: string) => Promise<string | null>
}) {
  const [value, setValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [isSubmitting, setIsSubmitting] = useState(false)
  const inputRef = useRef<HTMLInputElement | null>(null)
  const Icon = type === 'folder' ? FolderPlus : FilePlus

  useEffect(() => {
    inputRef.current?.focus()
  }, [])

  const submit = async () => {
    const trimmed = value.trim()
    if (!trimmed) {
      setError('Name cannot be empty')
      return
    }

    setIsSubmitting(true)
    try {
      const result = await onCreate(trimmed)
      if (result) {
        setError(result)
        return
      }
      setValue('')
      setError(null)
    } finally {
      setIsSubmitting(false)
    }
  }

  const handleSubmit = (event: FormEvent) => {
    event.preventDefault()
    void submit()
  }

  return (
    <form
      className="py-0.5 pr-1.5"
      onSubmit={handleSubmit}
      style={{ paddingLeft: `${6 + level * 12 + 16}px` }}
    >
      <div className="flex items-center gap-1">
        <Icon className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
        <Input
          ref={inputRef}
          aria-label={`New ${type} path`}
          className={cn(
            'h-6 min-w-0 flex-1 rounded-sm px-1.5 text-[12px]',
            error && 'border-destructive focus-visible:ring-destructive/30',
          )}
          disabled={isSubmitting}
          onChange={(event) => {
            setValue(event.target.value)
            setError(null)
          }}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              onCancel()
            }
          }}
          placeholder={type === 'folder' ? 'folder/subfolder' : 'folder/file.ext'}
          value={value}
        />
        <button
          aria-label={`Create ${type}`}
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
          disabled={isSubmitting || !value.trim()}
          type="submit"
        >
          <Check className="h-3.5 w-3.5" />
        </button>
        <button
          aria-label="Cancel create"
          className="flex h-6 w-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted hover:text-foreground disabled:opacity-50"
          disabled={isSubmitting}
          onClick={onCancel}
          type="button"
        >
          <X className="h-3.5 w-3.5" />
        </button>
      </div>
      {error ? <p className="mt-0.5 pl-5 text-[11px] text-destructive">{error}</p> : null}
    </form>
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
