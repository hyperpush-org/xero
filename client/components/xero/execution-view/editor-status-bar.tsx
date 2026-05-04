import { ChevronRight } from 'lucide-react'
import { formatBytes } from '@/lib/agent-attachments'
import { cn } from '@/lib/utils'

interface EditorToolbarProps {
  activePath: string
  isDirty: boolean
  isSaving: boolean
  showSaveControls: boolean
  onRevert: () => void
  onSave: () => void
}

interface EditorStatusBarProps {
  cursor: {
    line: number
    column: number
  }
  lang: string
  lineCount: number
  isDirty: boolean
  isSaving: boolean
}

interface PreviewStatusBarProps {
  rendererKind: string
  mimeType: string | null
  byteLength: number
}

export function EditorToolbar({
  activePath,
  isDirty,
  isSaving,
  showSaveControls,
  onRevert,
  onSave,
}: EditorToolbarProps) {
  return (
    <div className="flex shrink-0 items-center justify-between border-b border-border bg-background px-3 py-1.5">
      <Breadcrumb path={activePath} />
      {showSaveControls ? (
        <div className="flex items-center gap-2 text-[11px] text-muted-foreground">
          {isDirty ? (
            <button
              className="rounded px-1.5 py-0.5 transition-colors hover:bg-secondary/50 hover:text-foreground"
              onClick={onRevert}
              type="button"
            >
              Revert
            </button>
          ) : null}
          <button
            className={cn(
              'rounded px-2 py-0.5 font-medium transition-colors',
              isDirty && !isSaving
                ? 'bg-primary text-primary-foreground hover:bg-primary/90'
                : 'text-muted-foreground',
            )}
            disabled={!isDirty || isSaving}
            onClick={onSave}
            type="button"
            title="Save (⌘S)"
          >
            {isSaving ? 'Saving…' : 'Save'}
          </button>
        </div>
      ) : null}
    </div>
  )
}

export function EditorStatusBar({ cursor, lang, lineCount, isDirty, isSaving }: EditorStatusBarProps) {
  return (
    <div className="flex shrink-0 items-center justify-between border-t border-border bg-secondary/20 px-3 py-1 text-[10px] text-muted-foreground">
      <div className="flex items-center gap-3">
        <span className="tabular-nums">Ln {cursor.line}, Col {cursor.column}</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="tabular-nums">{lineCount} lines</span>
        <span className="text-muted-foreground/40">·</span>
        <span>Spaces: 2</span>
      </div>
      <div className="flex items-center gap-3">
        {isSaving ? <span className="text-primary">Saving…</span> : isDirty ? <span className="text-primary">● Unsaved</span> : <span>Saved</span>}
        <span className="text-muted-foreground/40">·</span>
        <span>UTF-8</span>
        <span className="text-muted-foreground/40">·</span>
        <span>LF</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="capitalize">{lang}</span>
      </div>
    </div>
  )
}

export function PreviewStatusBar({ rendererKind, mimeType, byteLength }: PreviewStatusBarProps) {
  return (
    <div
      className="flex shrink-0 items-center justify-between border-t border-border bg-secondary/20 px-3 py-1 text-[10px] text-muted-foreground"
      data-testid="preview-status-bar"
    >
      <div className="flex items-center gap-3">
        <span className="capitalize">{rendererKind}</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="tabular-nums">{formatBytes(byteLength)}</span>
      </div>
      <div className="flex items-center gap-3">
        <span>Read-only</span>
        {mimeType ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <span className="font-mono">{mimeType}</span>
          </>
        ) : null}
      </div>
    </div>
  )
}

function Breadcrumb({ path }: { path: string }) {
  const segments = path.split('/').filter(Boolean)

  return (
    <div className="flex min-w-0 items-center gap-1 truncate font-mono text-[11px] text-muted-foreground">
      {segments.map((segment, index) => (
        <span key={`${segment}-${index}`} className="flex items-center gap-1">
          {index > 0 ? <ChevronRight className="h-3 w-3 text-muted-foreground/40" /> : null}
          <span className={cn(index === segments.length - 1 && 'text-foreground/85')}>{segment}</span>
        </span>
      ))}
    </div>
  )
}
