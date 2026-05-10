import { X } from 'lucide-react'
import { cn } from '@/lib/utils'
import { getFileIcon as getFileIconForName } from '../file-tree'

interface EditorTabsProps {
  openTabs: string[]
  activePath: string | null
  dirtyPaths: Set<string>
  pendingFilePath: string | null
  onSelectTab: (path: string) => void
  onCloseTab: (path: string) => void
}

export function EditorTabs({
  openTabs,
  activePath,
  dirtyPaths,
  pendingFilePath,
  onSelectTab,
  onCloseTab,
}: EditorTabsProps) {
  if (openTabs.length === 0) {
    return (
      <div className="flex h-9 min-w-0 flex-1 items-center px-3 text-[11px] text-muted-foreground/70">
        {pendingFilePath ? `Opening ${pendingFilePath.split('/').pop() ?? pendingFilePath}…` : 'No files open'}
      </div>
    )
  }

  return (
    <div className="flex min-w-0 flex-1 items-stretch overflow-x-auto overflow-y-hidden scrollbar-thin">
      {openTabs.map((tabPath) => {
        const isActive = activePath === tabPath
        const isDirty = dirtyPaths.has(tabPath)
        const name = tabPath.split('/').pop() ?? tabPath

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
              type="button"
              onClick={() => onSelectTab(tabPath)}
              className="flex items-center gap-1.5 py-1.5"
              title={tabPath}
            >
              {getFileIconForName(name)}
              <span className="font-mono">{name}</span>
            </button>
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
