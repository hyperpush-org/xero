import type { ReactNode } from 'react'
import { ChevronRight, FilePlus, FolderPlus, RotateCcw, Search, X } from 'lucide-react'
import { Input } from '@/components/ui/input'
import { cn } from '@/lib/utils'
import type { FileSystemNode } from '@/src/lib/file-system-tree'
import { FileTree } from '../file-tree'

interface ExplorerPaneProps {
  projectLabel: string
  subtitle: string | null
  searchQuery: string
  isTreeLoading: boolean
  workspaceError: string | null
  tree: FileSystemNode
  activePath: string | null
  expandedFolders: Set<string>
  dirtyPaths: Set<string>
  onSearchQueryChange: (value: string) => void
  onSelectFile: (path: string) => Promise<void> | void
  onToggleFolder: (path: string) => void
  onRequestRename: (path: string, type: 'file' | 'folder') => void
  onRequestDelete: (path: string, type: 'file' | 'folder') => void
  onRequestNewFile: (parentPath: string) => void
  onRequestNewFolder: (parentPath: string) => void
  onCopyPath: (path: string) => void
  onCollapseAll: () => void
  onReload: () => void
}

export function ExplorerPane({
  projectLabel,
  subtitle,
  searchQuery,
  isTreeLoading,
  workspaceError,
  tree,
  activePath,
  expandedFolders,
  dirtyPaths,
  onSearchQueryChange,
  onSelectFile,
  onToggleFolder,
  onRequestRename,
  onRequestDelete,
  onRequestNewFile,
  onRequestNewFolder,
  onCopyPath,
  onCollapseAll,
  onReload,
}: ExplorerPaneProps) {
  return (
    <aside className="motion-layout-island flex w-[260px] shrink-0 flex-col border-r border-border bg-sidebar">
      <div className="flex shrink-0 items-start justify-between gap-2 px-3 pt-2.5 pb-2">
        <div className="min-w-0">
          <span className="text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Explorer
          </span>
          <p className="truncate text-[11px] text-foreground/85">{projectLabel}</p>
          {subtitle ? <p className="truncate text-[10px] text-muted-foreground">{subtitle}</p> : null}
        </div>
        <div className="flex items-center gap-0.5">
          <IconButton label="New file" onClick={() => onRequestNewFile('/')}>
            <FilePlus className="h-3.5 w-3.5" />
          </IconButton>
          <IconButton label="New folder" onClick={() => onRequestNewFolder('/')}>
            <FolderPlus className="h-3.5 w-3.5" />
          </IconButton>
          <IconButton label="Collapse all" onClick={onCollapseAll}>
            <ChevronRight className="h-3.5 w-3.5 rotate-90" />
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
          onSelectFile={(path) => {
            void onSelectFile(path)
          }}
          onToggleFolder={onToggleFolder}
          onRequestRename={onRequestRename}
          onRequestDelete={onRequestDelete}
          onRequestNewFile={onRequestNewFile}
          onRequestNewFolder={onRequestNewFolder}
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
    <button
      aria-label={label}
      className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
      onClick={onClick}
      title={label}
      type="button"
    >
      {children}
    </button>
  )
}
