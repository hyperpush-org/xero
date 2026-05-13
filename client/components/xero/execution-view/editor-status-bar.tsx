import { formatBytes } from '@/lib/agent-attachments'
import {
  describeIndentMode,
  describeLineEnding,
  type DocumentSettings,
} from '@/lib/document-settings'
import { cn } from '@/lib/utils'
import type { EditorGitFileStatus } from './git-aware-editing'

interface EditorStatusBarProps {
  cursor: {
    line: number
    column: number
  }
  lang: string
  lineCount: number
  isDirty: boolean
  isSaving: boolean
  staleState?: { kind: 'changed' | 'deleted'; detectedAt: string } | null
  diagnosticCount?: number
  gitStatus?: EditorGitFileStatus | null
  documentSettings?: DocumentSettings | null
  byteLength?: number
  encoding?: string
  readOnly?: boolean
}

interface PreviewStatusBarProps {
  rendererKind: string
  mimeType: string | null
  byteLength: number
  dimensions?: { width: number; height: number } | null
}

export function EditorStatusBar({
  cursor,
  lang,
  lineCount,
  isDirty,
  isSaving,
  staleState = null,
  diagnosticCount = 0,
  gitStatus = null,
  documentSettings = null,
  byteLength,
  encoding = 'UTF-8',
  readOnly = false,
}: EditorStatusBarProps) {
  const indentLabel = documentSettings
    ? describeIndentMode(documentSettings)
    : 'Spaces (2)'
  const eolLabel = documentSettings ? describeLineEnding(documentSettings.eol) : 'LF'

  return (
    <div className="flex shrink-0 items-center justify-between border-t border-border bg-secondary/20 px-3 py-1 text-[10px] text-muted-foreground">
      <div className="flex items-center gap-3">
        <span className="tabular-nums">Ln {cursor.line}, Col {cursor.column}</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="tabular-nums">{lineCount} lines</span>
        <span className="text-muted-foreground/40">·</span>
        <span>{indentLabel}</span>
        {typeof byteLength === 'number' ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <span className="tabular-nums">{formatBytes(byteLength)}</span>
          </>
        ) : null}
        {readOnly ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <span className="text-warning">Read-only</span>
          </>
        ) : null}
      </div>
      <div className="flex items-center gap-3">
        {isSaving ? <span className="text-primary">Saving…</span> : isDirty ? <span className="text-primary">● Unsaved</span> : <span>Saved</span>}
        {staleState ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <span className="text-warning">{staleState.kind === 'deleted' ? 'Deleted on disk' : 'Changed on disk'}</span>
          </>
        ) : null}
        {diagnosticCount > 0 ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <span className="text-destructive">{diagnosticCount} problem{diagnosticCount === 1 ? '' : 's'}</span>
          </>
        ) : null}
        {gitStatus ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <span
              className={cn(
                'font-mono',
                gitStatus.tone === 'added' && 'text-success',
                gitStatus.tone === 'modified' && 'text-primary',
                gitStatus.tone === 'deleted' && 'text-destructive',
                gitStatus.tone === 'warning' && 'text-warning',
                gitStatus.tone === 'conflicted' && 'text-destructive',
              )}
              title={`Git: ${gitStatus.description}`}
            >
              Git {gitStatus.label}
            </span>
          </>
        ) : null}
        <span className="text-muted-foreground/40">·</span>
        <span>{encoding}</span>
        <span className="text-muted-foreground/40">·</span>
        <span>{eolLabel}</span>
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
