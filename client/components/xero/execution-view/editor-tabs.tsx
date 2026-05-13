import { useRef, type KeyboardEvent } from 'react'
import { AlertTriangle, X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { getFileIcon as getFileIconForName } from '../file-tree'
import type { EditorGitFileStatus } from './git-aware-editing'

interface EditorTabsProps {
  openTabs: string[]
  activePath: string | null
  dirtyPaths: Set<string>
  stalePaths?: Record<string, { kind: 'changed' | 'deleted'; detectedAt: string }>
  diagnosticCountsByPath?: Record<string, number>
  gitStatusByPath?: Record<string, EditorGitFileStatus>
  pendingFilePath: string | null
  onSelectTab: (path: string) => void
  onCloseTab: (path: string) => void
}

export function EditorTabs({
  openTabs,
  activePath,
  dirtyPaths,
  stalePaths = {},
  diagnosticCountsByPath = {},
  gitStatusByPath = {},
  pendingFilePath,
  onSelectTab,
  onCloseTab,
}: EditorTabsProps) {
  const tabRefs = useRef<Map<string, HTMLButtonElement>>(new Map())

  if (openTabs.length === 0) {
    return (
      <div className="flex h-9 min-w-0 flex-1 items-center px-3 text-[11px] text-muted-foreground/70">
        {pendingFilePath ? `Opening ${pendingFilePath.split('/').pop() ?? pendingFilePath}…` : 'No files open'}
      </div>
    )
  }

  const focusTab = (path: string | undefined | null) => {
    if (!path) return
    const node = tabRefs.current.get(path)
    if (!node) return
    node.focus()
  }

  const handleKeyDown = (event: KeyboardEvent<HTMLButtonElement>, tabPath: string) => {
    const currentIndex = openTabs.indexOf(tabPath)
    if (currentIndex < 0) return

    if (event.key === 'ArrowRight') {
      event.preventDefault()
      const nextPath = openTabs[(currentIndex + 1) % openTabs.length]
      onSelectTab(nextPath)
      focusTab(nextPath)
      return
    }
    if (event.key === 'ArrowLeft') {
      event.preventDefault()
      const nextPath = openTabs[(currentIndex - 1 + openTabs.length) % openTabs.length]
      onSelectTab(nextPath)
      focusTab(nextPath)
      return
    }
    if (event.key === 'Home') {
      event.preventDefault()
      onSelectTab(openTabs[0])
      focusTab(openTabs[0])
      return
    }
    if (event.key === 'End') {
      event.preventDefault()
      const last = openTabs[openTabs.length - 1]
      onSelectTab(last)
      focusTab(last)
      return
    }
    if (event.key === 'Delete' || (event.key === 'w' && (event.metaKey || event.ctrlKey))) {
      event.preventDefault()
      onCloseTab(tabPath)
    }
  }

  return (
    <div
      aria-label="Open editor tabs"
      className="flex min-w-0 flex-1 items-stretch overflow-x-auto overflow-y-hidden scrollbar-thin"
      role="tablist"
    >
      {openTabs.map((tabPath) => {
        const isActive = activePath === tabPath
        const isDirty = dirtyPaths.has(tabPath)
        const isStale = !!stalePaths[tabPath]
        const diagnosticCount = diagnosticCountsByPath[tabPath] ?? 0
        const gitStatus = gitStatusByPath[tabPath] ?? null
        const name = tabPath.split('/').pop() ?? tabPath
        const labelParts: string[] = [name]
        if (isDirty) labelParts.push('unsaved')
        if (isStale) labelParts.push('changed on disk')
        if (diagnosticCount > 0) {
          labelParts.push(`${diagnosticCount} problem${diagnosticCount === 1 ? '' : 's'}`)
        }

        return (
          <div
            key={tabPath}
            className={cn(
              'group relative flex shrink-0 items-center gap-1.5 border-r border-border pl-3 pr-2 text-[12px] transition-colors',
              isActive
                ? 'bg-background text-foreground'
                : 'bg-secondary/10 text-muted-foreground hover:bg-secondary/30 hover:text-foreground',
            )}
          >
            <button
              ref={(node) => {
                if (node) tabRefs.current.set(tabPath, node)
                else tabRefs.current.delete(tabPath)
              }}
              aria-label={labelParts.join(', ')}
              aria-selected={isActive}
              data-active={isActive ? 'true' : undefined}
              role="tab"
              tabIndex={isActive ? 0 : -1}
              type="button"
              onClick={() => onSelectTab(tabPath)}
              onKeyDown={(event) => handleKeyDown(event, tabPath)}
              className="flex items-center gap-1.5 py-1.5"
              title={tabPath}
            >
              {getFileIconForName(name)}
              <span className="font-mono">{name}</span>
            </button>
            {isStale ? (
              <AlertTriangle className="h-3 w-3 shrink-0 text-warning" aria-label="Changed on disk" />
            ) : null}
            {gitStatus ? <GitStatusBadge status={gitStatus} /> : null}
            {diagnosticCount > 0 ? (
              <span
                className="rounded bg-destructive/15 px-1 text-[10px] leading-4 text-destructive"
                aria-label={`${diagnosticCount} problems`}
              >
                {diagnosticCount}
              </span>
            ) : null}
            <button
              aria-label={`Close ${name}`}
              className={cn(
                'ml-0.5 flex h-4 w-4 items-center justify-center rounded-sm transition-colors',
                isDirty
                  ? 'text-primary hover:bg-muted hover:text-foreground'
                  : 'text-muted-foreground opacity-0 hover:bg-muted hover:text-foreground group-hover:opacity-100',
                isActive && 'opacity-100',
              )}
              onClick={(event) => {
                event.stopPropagation()
                onCloseTab(tabPath)
              }}
              type="button"
            >
              {isDirty ? <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden /> : <X className="h-3 w-3" />}
            </button>
            {isActive ? <span className="absolute inset-x-0 bottom-0 h-px bg-primary" aria-hidden /> : null}
          </div>
        )
      })}
    </div>
  )
}

function GitStatusBadge({ status }: { status: EditorGitFileStatus }) {
  return (
    <span
      aria-label={`Git ${status.description}`}
      className={cn(
        'rounded border px-1 font-mono text-[9px] leading-4',
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
