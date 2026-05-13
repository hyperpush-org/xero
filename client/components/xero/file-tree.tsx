"use client"

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent,
} from 'react'
import {
  AlertTriangle,
  Bot,
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
  Loader2,
  Settings2,
  X,
} from 'lucide-react'
import { Input } from '@/components/ui/input'
import { useFixedVirtualizer } from '@/hooks/use-fixed-virtualizer'
import { shouldVirtualizeRows } from '@/lib/virtual-list'
import { cn } from '@/lib/utils'
import type { FileSystemNode } from '@/src/lib/file-system-tree'
import type { EditorGitFileStatus } from './execution-view/git-aware-editing'
import { FileContextMenu } from './file-context-menu'

const FILE_TREE_DRAG_TYPE = 'application/x-xero-project-entry'
const FILE_TREE_ROW_HEIGHT = 26
const FILE_TREE_VIRTUALIZATION_THRESHOLD = 240

interface FileTreeProps {
  root: FileSystemNode
  selectedPath: string | null
  expandedFolders: Set<string>
  loadingFolders?: Set<string>
  dirtyPaths?: Set<string>
  stalePaths?: Record<string, { kind: 'changed' | 'deleted'; detectedAt: string }>
  diagnosticCountsByPath?: Record<string, number>
  gitStatusByPath?: Record<string, EditorGitFileStatus>
  agentActivityCountsByPath?: Record<string, number>
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

export interface MatchInfo {
  matchedPaths: Set<string>
  ancestorPaths: Set<string>
}

export type FileTreeRowModel =
  | {
      kind: 'node'
      node: FileSystemNode
      level: number
    }
  | {
      kind: 'create'
      parentPath: string
      type: 'file' | 'folder'
      level: number
    }
  | {
      kind: 'continuation'
      path: string
      omittedEntryCount: number
      level: number
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

function isVisibleForSearch(node: FileSystemNode, search: MatchInfo | null): boolean {
  if (!search) return true
  return search.matchedPaths.has(node.path) || search.ancestorPaths.has(node.path)
}

function parentOf(path: string): string | null {
  if (!path || path === '/') return null
  const idx = path.lastIndexOf('/')
  if (idx <= 0) return '/'
  return path.slice(0, idx)
}

function isFolderExpanded(
  node: FileSystemNode,
  expandedFolders: Set<string>,
  search: MatchInfo | null,
): boolean {
  return search
    ? search.ancestorPaths.has(node.path) || expandedFolders.has(node.path)
    : expandedFolders.has(node.path)
}

export function flattenFileTreeRows({
  root,
  expandedFolders,
  search,
  creatingEntry,
}: {
  root: FileSystemNode
  expandedFolders: Set<string>
  search: MatchInfo | null
  creatingEntry: { parentPath: string; type: 'file' | 'folder' } | null
}): FileTreeRowModel[] {
  const rows: FileTreeRowModel[] = []

  if (creatingEntry?.parentPath === '/') {
    rows.push({
      kind: 'create',
      parentPath: '/',
      type: creatingEntry.type,
      level: 0,
    })
  }

  const walk = (node: FileSystemNode, level: number) => {
    if (!isVisibleForSearch(node, search)) return

    rows.push({ kind: 'node', node, level })
    if (node.type !== 'folder' || !node.children || !isFolderExpanded(node, expandedFolders, search)) {
      return
    }

    if (creatingEntry?.parentPath === node.path) {
      rows.push({
        kind: 'create',
        parentPath: node.path,
        type: creatingEntry.type,
        level: level + 1,
      })
    }

    for (const child of node.children) {
      walk(child, level + 1)
    }

    if (node.truncated && (node.omittedEntryCount ?? 0) > 0) {
      rows.push({
        kind: 'continuation',
        path: node.path,
        omittedEntryCount: node.omittedEntryCount ?? 0,
        level: level + 1,
      })
    }
  }

  for (const child of root.children ?? []) {
    walk(child, 0)
  }
  if (root.truncated && (root.omittedEntryCount ?? 0) > 0) {
    rows.push({
      kind: 'continuation',
      path: root.path,
      omittedEntryCount: root.omittedEntryCount ?? 0,
      level: 0,
    })
  }

  return rows
}

export function FileTree({
  root,
  selectedPath,
  expandedFolders,
  loadingFolders = new Set(),
  dirtyPaths,
  stalePaths,
  diagnosticCountsByPath,
  gitStatusByPath,
  agentActivityCountsByPath,
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
  const [focusedPath, setFocusedPath] = useState<string | null>(selectedPath)
  const rowRefs = useRef<Map<string, HTMLButtonElement>>(new Map())
  const rows = useMemo(
    () => flattenFileTreeRows({ root, expandedFolders, search, creatingEntry }),
    [creatingEntry, expandedFolders, root, search],
  )
  const orderedNodePaths = useMemo(
    () =>
      rows.flatMap((row) => (row.kind === 'node' ? [row.node.path] : [])),
    [rows],
  )
  const nodesByPath = useMemo(() => {
    const map = new Map<string, FileSystemNode>()
    for (const row of rows) {
      if (row.kind === 'node') map.set(row.node.path, row.node)
    }
    return map
  }, [rows])
  const effectiveFocusedPath =
    focusedPath && orderedNodePaths.includes(focusedPath)
      ? focusedPath
      : selectedPath && orderedNodePaths.includes(selectedPath)
        ? selectedPath
        : orderedNodePaths[0] ?? null
  const selectedRowIndex = useMemo(
    () => rows.findIndex((row) => row.kind === 'node' && row.node.path === selectedPath),
    [rows, selectedPath],
  )

  useEffect(() => {
    if (selectedPath) {
      setFocusedPath(selectedPath)
    }
  }, [selectedPath])

  const registerRowRef = useCallback((path: string, node: HTMLButtonElement | null) => {
    if (node) rowRefs.current.set(path, node)
    else rowRefs.current.delete(path)
  }, [])

  const focusRowByPath = useCallback((path: string | null) => {
    if (!path) return
    rowRefs.current.get(path)?.focus()
  }, [])

  const handleRowKeyDown = useCallback(
    (event: KeyboardEvent<HTMLButtonElement>, rowPath: string) => {
      const idx = orderedNodePaths.indexOf(rowPath)
      if (idx < 0) return

      if (event.key === 'ArrowDown') {
        event.preventDefault()
        const nextIdx = Math.min(idx + 1, orderedNodePaths.length - 1)
        const nextPath = orderedNodePaths[nextIdx]
        setFocusedPath(nextPath)
        focusRowByPath(nextPath)
        return
      }
      if (event.key === 'ArrowUp') {
        event.preventDefault()
        const nextIdx = Math.max(idx - 1, 0)
        const nextPath = orderedNodePaths[nextIdx]
        setFocusedPath(nextPath)
        focusRowByPath(nextPath)
        return
      }
      if (event.key === 'Home') {
        event.preventDefault()
        const nextPath = orderedNodePaths[0]
        setFocusedPath(nextPath)
        focusRowByPath(nextPath)
        return
      }
      if (event.key === 'End') {
        event.preventDefault()
        const nextPath = orderedNodePaths[orderedNodePaths.length - 1]
        setFocusedPath(nextPath)
        focusRowByPath(nextPath)
        return
      }

      const node = nodesByPath.get(rowPath)
      if (!node) return

      if (event.key === 'ArrowRight') {
        if (node.type === 'folder') {
          const expanded = isFolderExpanded(node, expandedFolders, search)
          if (!expanded) {
            event.preventDefault()
            onToggleFolder(node.path)
            return
          }
          const nextPath = orderedNodePaths[idx + 1]
          if (nextPath) {
            event.preventDefault()
            setFocusedPath(nextPath)
            focusRowByPath(nextPath)
          }
        }
        return
      }
      if (event.key === 'ArrowLeft') {
        if (node.type === 'folder' && isFolderExpanded(node, expandedFolders, search)) {
          event.preventDefault()
          onToggleFolder(node.path)
          return
        }
        const parent = parentOf(rowPath)
        if (parent && parent !== '/' && orderedNodePaths.includes(parent)) {
          event.preventDefault()
          setFocusedPath(parent)
          focusRowByPath(parent)
        }
        return
      }
      if (event.key === 'Enter' || event.key === ' ') {
        event.preventDefault()
        if (node.type === 'folder') onToggleFolder(node.path)
        else onSelectFile(node.path)
      }
    },
    [
      expandedFolders,
      focusRowByPath,
      nodesByPath,
      onSelectFile,
      onToggleFolder,
      orderedNodePaths,
      search,
    ],
  )
  const shouldVirtualize = shouldVirtualizeRows(rows.length, FILE_TREE_VIRTUALIZATION_THRESHOLD) && !creatingEntry
  const virtualizer = useFixedVirtualizer({
    enabled: shouldVirtualize,
    itemCount: rows.length,
    itemSize: FILE_TREE_ROW_HEIGHT,
    overscan: 12,
    scrollToIndex: selectedRowIndex >= 0 ? selectedRowIndex : null,
  })
  const renderedRowIndexes = shouldVirtualize
    ? virtualizer.indexes
    : rows.map((_, index) => index)

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
        onScroll={virtualizer.onScroll}
        onDragLeave={(event) => {
          if (event.currentTarget === event.target) setDropTargetPath(null)
        }}
        onDragOver={(event) => {
          event.preventDefault()
          setDropTargetPath('/')
        }}
        onDrop={(event) => handleDropOnFolder(event, '/')}
        ref={virtualizer.scrollRef}
        role="tree"
        aria-label="Project files"
      >
        {shouldVirtualize ? <div aria-hidden="true" style={{ height: virtualizer.range.beforeSize }} /> : null}
        {renderedRowIndexes.map((rowIndex) => {
          const row = rows[rowIndex]
          return (
            <FileTreeRow
              dirtyPaths={dirtyPaths}
              stalePaths={stalePaths}
              diagnosticCountsByPath={diagnosticCountsByPath}
              gitStatusByPath={gitStatusByPath}
              agentActivityCountsByPath={agentActivityCountsByPath}
              draggingPath={draggingPath}
              dropTargetPath={dropTargetPath}
              expandedFolders={expandedFolders}
              loadingFolders={loadingFolders}
              focusedPath={effectiveFocusedPath}
              registerRowRef={registerRowRef}
              onRowFocus={setFocusedPath}
              onRowKeyDown={handleRowKeyDown}
              key={
                row.kind === 'node'
                  ? row.node.id
                  : row.kind === 'create'
                    ? `create:${row.parentPath}`
                    : `continuation:${row.path}`
              }
              onCancelCreate={onCancelCreate}
              onCopyPath={onCopyPath}
              onCreateEntry={onCreateEntry}
              onDragEnd={() => {
                setDraggingPath(null)
                setDropTargetPath(null)
              }}
              onDragStart={setDraggingPath}
              onDropOnFolder={handleDropOnFolder}
              onDropTargetChange={setDropTargetPath}
              onRequestDelete={onRequestDelete}
              onRequestNewFile={onRequestNewFile}
              onRequestNewFolder={onRequestNewFolder}
              onRequestRename={onRequestRename}
              onSelectFile={onSelectFile}
              onToggleFolder={onToggleFolder}
              row={row}
              search={search}
              selectedPath={selectedPath}
            />
          )
        })}
        {shouldVirtualize ? <div aria-hidden="true" style={{ height: virtualizer.range.afterSize }} /> : null}
      </div>
    </FileContextMenu>
  )
}

interface FileTreeRowProps {
  row: FileTreeRowModel
  selectedPath: string | null
  expandedFolders: Set<string>
  loadingFolders: Set<string>
  dirtyPaths?: Set<string>
  stalePaths?: Record<string, { kind: 'changed' | 'deleted'; detectedAt: string }>
  diagnosticCountsByPath?: Record<string, number>
  gitStatusByPath?: Record<string, EditorGitFileStatus>
  agentActivityCountsByPath?: Record<string, number>
  search: MatchInfo | null
  draggingPath: string | null
  dropTargetPath: string | null
  focusedPath: string | null
  registerRowRef: (path: string, node: HTMLButtonElement | null) => void
  onRowFocus: (path: string) => void
  onRowKeyDown: (event: KeyboardEvent<HTMLButtonElement>, path: string) => void
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

interface TreeNodeProps {
  node: FileSystemNode
  level: number
  selectedPath: string | null
  expandedFolders: Set<string>
  loadingFolders: Set<string>
  dirtyPaths?: Set<string>
  stalePaths?: Record<string, { kind: 'changed' | 'deleted'; detectedAt: string }>
  diagnosticCountsByPath?: Record<string, number>
  gitStatusByPath?: Record<string, EditorGitFileStatus>
  agentActivityCountsByPath?: Record<string, number>
  search: MatchInfo | null
  draggingPath: string | null
  dropTargetPath: string | null
  focusedPath: string | null
  registerRowRef: (path: string, node: HTMLButtonElement | null) => void
  onRowFocus: (path: string) => void
  onRowKeyDown: (event: KeyboardEvent<HTMLButtonElement>, path: string) => void
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

function FileTreeRow({ row, ...props }: FileTreeRowProps) {
  if (row.kind === 'create') {
    return (
      <InlineCreateRow
        level={row.level}
        type={row.type}
        onCancel={props.onCancelCreate}
        onCreate={props.onCreateEntry}
      />
    )
  }

  if (row.kind === 'continuation') {
    return <ContinuationRow level={row.level} omittedEntryCount={row.omittedEntryCount} />
  }

  const nodeProps = {
    ...props,
    node: row.node,
    level: row.level,
  }

  return row.node.type === 'folder' ? <FolderRow {...nodeProps} /> : <FileRow {...nodeProps} />
}

function FolderRow({
  node,
  level,
  expandedFolders,
  loadingFolders,
  search,
  draggingPath,
  dropTargetPath,
  focusedPath,
  registerRowRef,
  onRowFocus,
  onRowKeyDown,
  onDragStart,
  onDragEnd,
  onDropTargetChange,
  onDropOnFolder,
  onToggleFolder,
  onRequestRename,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onCopyPath,
}: TreeNodeProps) {
  const isExpanded = isFolderExpanded(node, expandedFolders, search)
  const isLoading = loadingFolders.has(node.path)
  const isDropTarget =
    dropTargetPath === node.path &&
    draggingPath !== node.path &&
    !node.path.startsWith(`${draggingPath ?? ''}/`)
  const isFocusable = focusedPath === node.path

  return (
    <FileContextMenu
      type="folder"
      onNewFile={() => onRequestNewFile(node.path)}
      onNewFolder={() => onRequestNewFolder(node.path)}
      onRename={() => onRequestRename(node.path, 'folder')}
      onDelete={() => onRequestDelete(node.path, 'folder')}
      onCopyPath={() => onCopyPath(node.path)}
    >
      <button
        ref={(element) => registerRowRef(node.path, element)}
        aria-expanded={isExpanded}
        aria-level={level + 1}
        aria-label={`${node.name} folder${isExpanded ? ', expanded' : ', collapsed'}`}
        tabIndex={isFocusable ? 0 : -1}
        type="button"
        draggable
        onClick={() => onToggleFolder(node.path)}
        onFocus={() => onRowFocus(node.path)}
        onKeyDown={(event) => onRowKeyDown(event, node.path)}
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
        role="treeitem"
        className={cn(
          'group flex h-[26px] w-full items-center gap-1 py-0 pr-2 text-left text-[12px] leading-5 transition-colors',
          'hover:bg-muted/40 text-foreground/80',
          draggingPath === node.path && 'opacity-50',
          isDropTarget && 'bg-primary/12 text-foreground',
        )}
        style={{ paddingLeft: `${6 + level * 12}px` }}
      >
        <span className="flex h-4 w-4 shrink-0 items-center justify-center text-muted-foreground/70">
          {isLoading ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : isExpanded ? (
            <ChevronDown className="h-3.5 w-3.5" />
          ) : (
            <ChevronRight className="h-3.5 w-3.5" />
          )}
        </span>
        <span className="flex h-4 w-4 shrink-0 items-center justify-center text-chart-1">
          {isExpanded ? <FolderOpen className="h-3.5 w-3.5" /> : <Folder className="h-3.5 w-3.5" />}
        </span>
        <span className="min-w-0 flex-1 truncate">{node.name}</span>
      </button>
    </FileContextMenu>
  )
}

function FileRow({
  node,
  level,
  selectedPath,
  dirtyPaths,
  stalePaths,
  diagnosticCountsByPath,
  gitStatusByPath,
  agentActivityCountsByPath,
  draggingPath,
  focusedPath,
  registerRowRef,
  onRowFocus,
  onRowKeyDown,
  onDragStart,
  onDragEnd,
  onSelectFile,
  onRequestRename,
  onRequestDelete,
  onCopyPath,
}: TreeNodeProps) {
  const isSelected = selectedPath === node.path
  const isDirty = dirtyPaths?.has(node.path) ?? false
  const isStale = !!stalePaths?.[node.path]
  const diagnosticCount = diagnosticCountsByPath?.[node.path] ?? 0
  const gitStatus = gitStatusByPath?.[node.path] ?? null
  const agentActivityCount = agentActivityCountsByPath?.[node.path] ?? 0
  const isFocusable = focusedPath === node.path

  return (
    <FileContextMenu
      type="file"
      onRename={() => onRequestRename(node.path, 'file')}
      onDelete={() => onRequestDelete(node.path, 'file')}
      onCopyPath={() => onCopyPath(node.path)}
    >
      <button
        ref={(element) => registerRowRef(node.path, element)}
        aria-level={level + 1}
        aria-selected={isSelected}
        tabIndex={isFocusable ? 0 : -1}
        type="button"
        draggable
        onClick={() => onSelectFile(node.path)}
        onFocus={() => onRowFocus(node.path)}
        onKeyDown={(event) => onRowKeyDown(event, node.path)}
        onDragEnd={onDragEnd}
        onDragStart={(event) => {
          event.dataTransfer.effectAllowed = 'move'
          event.dataTransfer.setData(FILE_TREE_DRAG_TYPE, node.path)
          onDragStart(node.path)
        }}
        role="treeitem"
        className={cn(
          'group flex h-[26px] w-full items-center gap-1 py-0 pr-2 text-left text-[12px] leading-5 transition-colors',
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
        {isStale ? (
          <AlertTriangle className="ml-0.5 h-3.5 w-3.5 shrink-0 text-warning" aria-label="Changed on disk" />
        ) : null}
        {gitStatus ? <GitStatusBadge status={gitStatus} /> : null}
        {agentActivityCount > 0 ? <AgentActivityBadge count={agentActivityCount} /> : null}
        {diagnosticCount > 0 ? (
          <span
            className="ml-0.5 rounded bg-destructive/15 px-1 text-[10px] leading-4 text-destructive"
            aria-label={`${diagnosticCount} problems`}
          >
            {diagnosticCount}
          </span>
        ) : null}
      </button>
    </FileContextMenu>
  )
}

function AgentActivityBadge({ count }: { count: number }) {
  return (
    <span
      aria-label={`${count} agent file ${count === 1 ? 'activity' : 'activities'}`}
      className="ml-0.5 inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded border border-info/35 bg-info/10 px-1 text-[9px] leading-none text-info"
      title={`${count} agent file ${count === 1 ? 'activity' : 'activities'}`}
    >
      <Bot className="h-2.5 w-2.5" aria-hidden="true" />
      {count > 1 ? <span className="ml-0.5">{count}</span> : null}
    </span>
  )
}

function GitStatusBadge({ status }: { status: EditorGitFileStatus }) {
  return (
    <span
      aria-label={`Git ${status.description}`}
      className={cn(
        'ml-0.5 inline-flex h-4 min-w-4 shrink-0 items-center justify-center rounded border px-1 font-mono text-[9px] leading-none',
        status.tone === 'added' && 'border-success/40 bg-success/10 text-success',
        status.tone === 'modified' && 'border-primary/35 bg-primary/10 text-primary',
        status.tone === 'deleted' && 'border-destructive/40 bg-destructive/10 text-destructive',
        status.tone === 'warning' && 'border-warning/45 bg-warning/10 text-warning',
        status.tone === 'conflicted' && 'border-destructive/60 bg-destructive/15 text-destructive',
      )}
      title={`Git: ${status.description}`}
    >
      {status.label}
    </span>
  )
}

function ContinuationRow({
  level,
  omittedEntryCount,
}: {
  level: number
  omittedEntryCount: number
}) {
  return (
    <div
      className="flex h-[26px] w-full items-center gap-1 py-0 pr-2 text-[11px] leading-5 text-warning"
      role="status"
      style={{ paddingLeft: `${6 + level * 12}px` }}
    >
      <AlertTriangle className="h-3.5 w-3.5 shrink-0" />
      <span className="min-w-0 flex-1 truncate">
        {omittedEntryCount.toLocaleString()} more entries omitted
      </span>
    </div>
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
    return <FileJson className={cn(className, 'text-destructive')} />
  }

  if (ext === 'tsx' || ext === 'jsx') {
    return <FileCode className={cn(className, 'text-info')} />
  }
  if (ext === 'ts') return <FileCode className={cn(className, 'text-blue-400')} />
  if (ext === 'js' || ext === 'mjs' || ext === 'cjs') {
    return <FileCode className={cn(className, 'text-warning')} />
  }
  if (ext === 'py') return <FileCode className={cn(className, 'text-success')} />
  if (ext === 'rs') return <FileCode className={cn(className, 'text-warning')} />
  if (ext === 'go') return <FileCode className={cn(className, 'text-info')} />
  if (['java', 'c', 'cpp', 'h', 'hpp'].includes(ext)) {
    return <FileCode className={cn(className, 'text-indigo-400')} />
  }

  if (ext === 'css' || ext === 'scss') {
    return <FileCode className={cn(className, 'text-fuchsia-400')} />
  }
  if (ext === 'html' || ext === 'htm' || ext === 'vue' || ext === 'svelte') {
    return <FileCode className={cn(className, 'text-warning')} />
  }

  if (ext === 'json' || ext === 'jsonc') {
    return <FileJson className={cn(className, 'text-warning')} />
  }

  if (['md', 'mdx', 'txt'].includes(ext)) {
    return <FileText className={cn(className, 'text-slate-300')} />
  }
  if (['yaml', 'yml', 'toml'].includes(ext)) {
    return <FileText className={cn(className, 'text-warning')} />
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
