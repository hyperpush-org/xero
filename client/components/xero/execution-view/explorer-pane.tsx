import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type ReactNode,
} from 'react'
import { FilePlus, FolderPlus, RotateCcw, Search, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import { useSidebarWidthMotion } from '@/lib/sidebar-motion'
import type { FileSystemNode } from '@/src/lib/file-system-tree'
import { FileTree } from '../file-tree'

const MIN_WIDTH = 220
const DEFAULT_WIDTH = 260
const MAX_WIDTH = 560
const RIGHT_PADDING = 360
const STORAGE_KEY = 'xero.editor.explorer.width'

interface ExplorerPaneProps {
  searchQuery: string
  isTreeLoading: boolean
  workspaceError: string | null
  tree: FileSystemNode
  activePath: string | null
  expandedFolders: Set<string>
  dirtyPaths: Set<string>
  creatingEntry: { parentPath: string; type: 'file' | 'folder' } | null
  onSearchQueryChange: (value: string) => void
  onSelectFile: (path: string) => Promise<void> | void
  onToggleFolder: (path: string) => void
  onRequestRename: (path: string, type: 'file' | 'folder') => void
  onRequestDelete: (path: string, type: 'file' | 'folder') => void
  onRequestNewFile: (parentPath: string) => void
  onRequestNewFolder: (parentPath: string) => void
  onMoveEntry: (path: string, targetParentPath: string) => Promise<void> | void
  onCancelCreate: () => void
  onCreateEntry: (name: string) => Promise<string | null>
  onCopyPath: (path: string) => void
  onOpenFind: () => void
  onReload: () => void
}

function viewportMaxWidth() {
  if (typeof window === 'undefined') return MAX_WIDTH
  return Math.max(MIN_WIDTH, Math.min(MAX_WIDTH, window.innerWidth - RIGHT_PADDING))
}

function clampWidth(width: number, maxWidth = viewportMaxWidth()) {
  return Math.max(MIN_WIDTH, Math.min(maxWidth, width))
}

function readPersistedWidth(): number | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.localStorage?.getItem?.(STORAGE_KEY)
    if (!raw) return null
    const parsed = Number.parseInt(raw, 10)
    if (!Number.isFinite(parsed) || parsed < MIN_WIDTH) return null
    return clampWidth(parsed)
  } catch {
    return null
  }
}

function writePersistedWidth(width: number): void {
  if (typeof window === 'undefined') return
  try {
    window.localStorage?.setItem?.(STORAGE_KEY, String(Math.round(width)))
  } catch {
    /* storage unavailable — default next session */
  }
}

export function ExplorerPane({
  searchQuery,
  isTreeLoading,
  workspaceError,
  tree,
  activePath,
  expandedFolders,
  dirtyPaths,
  creatingEntry,
  onSearchQueryChange,
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
  onOpenFind,
  onReload,
}: ExplorerPaneProps) {
  const [width, setWidth] = useState(() => readPersistedWidth() ?? DEFAULT_WIDTH)
  const [maxWidth, setMaxWidth] = useState(viewportMaxWidth)
  const [isResizing, setIsResizing] = useState(false)
  const widthMotion = useSidebarWidthMotion(width, { isResizing })
  const widthRef = useRef(width)
  widthRef.current = width

  useEffect(() => {
    if (typeof window === 'undefined') return
    const handleResize = () => {
      const nextMax = viewportMaxWidth()
      setMaxWidth(nextMax)
      setWidth((current) => clampWidth(current, nextMax))
    }
    window.addEventListener('resize', handleResize)
    return () => window.removeEventListener('resize', handleResize)
  }, [])

  useEffect(() => {
    writePersistedWidth(width)
  }, [width])

  const handleResizeStart = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.preventDefault()
    const startX = event.clientX
    const startWidth = widthRef.current
    const ceiling = viewportMaxWidth()
    setMaxWidth(ceiling)
    setIsResizing(true)

    const previousCursor = document.body.style.cursor
    const previousSelect = document.body.style.userSelect
    document.body.style.cursor = 'col-resize'
    document.body.style.userSelect = 'none'

    const handleMove = (ev: globalThis.PointerEvent) => {
      const delta = ev.clientX - startX
      setWidth(clampWidth(startWidth + delta, ceiling))
    }
    const handleUp = () => {
      window.removeEventListener('pointermove', handleMove)
      window.removeEventListener('pointerup', handleUp)
      window.removeEventListener('pointercancel', handleUp)
      document.body.style.cursor = previousCursor
      document.body.style.userSelect = previousSelect
      setIsResizing(false)
    }

    window.addEventListener('pointermove', handleMove)
    window.addEventListener('pointerup', handleUp)
    window.addEventListener('pointercancel', handleUp)
  }, [])

  const handleResizeKey = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
    event.preventDefault()
    const step = event.shiftKey ? 32 : 8
    const ceiling = viewportMaxWidth()
    setMaxWidth(ceiling)
    setWidth((current) => {
      const delta = event.key === 'ArrowRight' ? step : -step
      return clampWidth(current + delta, ceiling)
    })
  }, [])

  return (
    <aside
      className={cn(
        widthMotion.islandClassName,
        'relative flex shrink-0 flex-col border-r border-border bg-sidebar',
      )}
      style={widthMotion.style}
    >
      <div
        aria-label="Resize explorer sidebar"
        aria-orientation="vertical"
        aria-valuemax={maxWidth}
        aria-valuemin={MIN_WIDTH}
        aria-valuenow={width}
        className={cn(
          'absolute inset-y-0 -right-[3px] z-10 w-[6px] cursor-col-resize bg-transparent transition-colors',
          'hover:bg-primary/30',
          isResizing && 'bg-primary/40',
        )}
        onKeyDown={handleResizeKey}
        onPointerDown={handleResizeStart}
        role="separator"
        tabIndex={0}
      />

      <div className="flex shrink-0 items-start justify-between gap-2 px-3 pt-2.5 pb-2">
        <div className="min-w-0">
          <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Explorer
          </span>
        </div>
        <div className="flex items-center gap-0.5">
          <IconButton label="New file" onClick={() => onRequestNewFile('/')}>
            <FilePlus className="h-3.5 w-3.5" />
          </IconButton>
          <IconButton label="New folder" onClick={() => onRequestNewFolder('/')}>
            <FolderPlus className="h-3.5 w-3.5" />
          </IconButton>
          <IconButton label="Open find and replace" onClick={onOpenFind}>
            <Search className="h-3.5 w-3.5" />
          </IconButton>
          <IconButton label="Reload project" onClick={onReload}>
            <RotateCcw className={cn('h-3.5 w-3.5', isTreeLoading && 'animate-spin')} />
          </IconButton>
        </div>
      </div>

      <div className="shrink-0 px-2 pb-2">
        <div className="relative">
          <Search className="pointer-events-none absolute left-2 top-1/2 h-3 w-3 -translate-y-1/2 text-muted-foreground/70" />
          <Input
            aria-label="Search files"
            className="h-7 pl-6 pr-6 text-[11px]"
            onChange={(event) => onSearchQueryChange(event.target.value)}
            placeholder="Search files…"
            value={searchQuery}
          />
          {searchQuery ? (
            <button
              aria-label="Clear search"
              className="absolute right-1.5 top-1/2 -translate-y-1/2 rounded p-0.5 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
              onClick={() => onSearchQueryChange('')}
              type="button"
            >
              <X className="h-3 w-3" />
            </button>
          ) : null}
        </div>
      </div>

      {workspaceError ? (
        <div className="mx-2 mb-2 rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-2 text-[11px] text-destructive">
          {workspaceError}
        </div>
      ) : null}

      {isTreeLoading && !tree.children?.length ? (
        <div className="flex flex-1 items-center justify-center px-6 text-center text-[11px] text-muted-foreground">
          Loading selected project files…
        </div>
      ) : (
        <FileTree
          root={tree}
          selectedPath={activePath}
          expandedFolders={expandedFolders}
          dirtyPaths={dirtyPaths}
          searchQuery={searchQuery}
          creatingEntry={creatingEntry}
          onSelectFile={(path) => {
            void onSelectFile(path)
          }}
          onToggleFolder={onToggleFolder}
          onRequestRename={onRequestRename}
          onRequestDelete={onRequestDelete}
          onRequestNewFile={onRequestNewFile}
          onRequestNewFolder={onRequestNewFolder}
          onMoveEntry={(path, targetParentPath) => {
            void onMoveEntry(path, targetParentPath)
          }}
          onCancelCreate={onCancelCreate}
          onCreateEntry={onCreateEntry}
          onCopyPath={onCopyPath}
        />
      )}
    </aside>
  )
}

function IconButton({
  label,
  onClick,
  children,
}: {
  label: string
  onClick: () => void
  children: ReactNode
}) {
  return (
    <Button
      aria-label={label}
      className="size-6 rounded text-muted-foreground hover:bg-muted hover:text-foreground"
      onClick={onClick}
      size="icon"
      title={label}
      type="button"
      variant="ghost"
    >
      {children}
    </Button>
  )
}
