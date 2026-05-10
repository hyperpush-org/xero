import { formatBytes } from '@/lib/agent-attachments'

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
  dimensions?: { width: number; height: number } | null
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

export function PreviewStatusBar({ rendererKind, mimeType, byteLength, dimensions }: PreviewStatusBarProps) {
  return (
    <div
      className="flex shrink-0 items-center justify-between border-t border-border bg-secondary/20 px-3 py-1 text-[10px] text-muted-foreground"
      data-testid="preview-status-bar"
    >
      <div className="flex items-center gap-3">
        <span className="capitalize">{rendererKind}</span>
        {dimensions ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <span
              className="tabular-nums"
              data-testid="preview-status-bar-dimensions"
            >
              {dimensions.width} × {dimensions.height}
            </span>
          </>
        ) : null}
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
