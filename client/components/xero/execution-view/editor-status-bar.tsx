import { CheckCircle2, Settings2, XCircle } from 'lucide-react'
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover'
import { Switch } from '@/components/ui/switch'
import { Label } from '@/components/ui/label'
import { Slider } from '@/components/ui/slider'
import { formatBytes } from '@/lib/agent-attachments'
import {
  describeIndentMode,
  describeLineEnding,
  type DocumentSettings,
} from '@/lib/document-settings'
import { cn } from '@/lib/utils'
import type { EditorRenderPreferences } from '../code-editor'
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
  problemsBusy?: boolean
  problemsPeekOpen?: boolean
  onToggleProblemsPeek?: () => void
  gitStatus?: EditorGitFileStatus | null
  documentSettings?: DocumentSettings | null
  byteLength?: number
  encoding?: string
  readOnly?: boolean
  preferences?: EditorRenderPreferences
  onPreferencesChange?: (next: EditorRenderPreferences) => void
  preferencesOpen?: boolean
  onPreferencesOpenChange?: (open: boolean) => void
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
  problemsBusy = false,
  problemsPeekOpen = false,
  onToggleProblemsPeek,
  gitStatus = null,
  documentSettings = null,
  byteLength,
  encoding = 'UTF-8',
  readOnly = false,
  preferences,
  onPreferencesChange,
  preferencesOpen,
  onPreferencesOpenChange,
}: EditorStatusBarProps) {
  const indentLabel = documentSettings
    ? describeIndentMode(documentSettings)
    : 'Spaces (2)'
  const eolLabel = documentSettings ? describeLineEnding(documentSettings.eol) : 'LF'
  const hasProblems = diagnosticCount > 0
  const showProblemsSegment = !!onToggleProblemsPeek
  const showPreferences = !!(preferences && onPreferencesChange)

  return (
    <div className="flex shrink-0 items-center justify-between border-t border-border bg-secondary/20 px-3 py-1 text-[10px] text-muted-foreground">
      <div className="flex items-center gap-3">
        <span className="tabular-nums">
          Ln {cursor.line}, Col {cursor.column}
        </span>
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
        {staleState ? (
          <span className="text-warning">
            {staleState.kind === 'deleted' ? 'Deleted on disk' : 'Changed on disk'}
          </span>
        ) : null}
        {gitStatus ? (
          <>
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
            <span className="text-muted-foreground/40">·</span>
          </>
        ) : null}
        <span>{encoding}</span>
        <span className="text-muted-foreground/40">·</span>
        <span>{eolLabel}</span>
        <span className="text-muted-foreground/40">·</span>
        <span className="capitalize">{lang}</span>
        {showProblemsSegment ? (
          <>
            <span className="text-muted-foreground/40">·</span>
            <button
              type="button"
              data-problems-peek-trigger="true"
              data-testid="status-bar-problems"
              aria-pressed={problemsPeekOpen}
              aria-label={
                hasProblems
                  ? `${diagnosticCount} problem${diagnosticCount === 1 ? '' : 's'}`
                  : 'No problems'
              }
              onClick={onToggleProblemsPeek}
              className={cn(
                'inline-flex items-center gap-1 rounded px-1.5 py-0.5 text-[10px] transition-colors',
                'hover:bg-secondary/50 hover:text-foreground',
                problemsPeekOpen && 'bg-secondary/60 text-foreground',
                hasProblems ? 'text-destructive' : 'text-muted-foreground',
                problemsBusy && 'animate-pulse',
              )}
            >
              {hasProblems ? (
                <XCircle className="h-3 w-3" aria-hidden="true" />
              ) : (
                <CheckCircle2 className="h-3 w-3 text-success" aria-hidden="true" />
              )}
              <span className="tabular-nums">
                {hasProblems
                  ? `${diagnosticCount} problem${diagnosticCount === 1 ? '' : 's'}`
                  : '0 problems'}
              </span>
            </button>
          </>
        ) : null}
        <span className="text-muted-foreground/40">·</span>
        {isSaving ? (
          <span className="text-primary">Saving…</span>
        ) : isDirty ? (
          <span className="text-primary">● Unsaved</span>
        ) : (
          <span>Saved</span>
        )}
        {showPreferences ? (
          <EditorPreferencesPopover
            preferences={preferences!}
            onPreferencesChange={onPreferencesChange!}
            open={preferencesOpen}
            onOpenChange={onPreferencesOpenChange}
          />
        ) : null}
      </div>
    </div>
  )
}

export function PreviewStatusBar({
  rendererKind,
  mimeType,
  byteLength,
  dimensions,
}: PreviewStatusBarProps) {
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

function EditorPreferencesPopover({
  preferences,
  onPreferencesChange,
  open,
  onOpenChange,
}: {
  preferences: EditorRenderPreferences
  onPreferencesChange: (next: EditorRenderPreferences) => void
  open?: boolean
  onOpenChange?: (open: boolean) => void
}) {
  const clampFontSize = (value: number) => Math.max(10, Math.min(22, Math.round(value)))
  const clampTabSize = (value: number) => Math.max(1, Math.min(8, Math.round(value)))

  return (
    <Popover open={open} onOpenChange={onOpenChange}>
      <PopoverTrigger asChild>
        <button
          type="button"
          aria-label="Editor preferences"
          className={cn(
            'inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground transition-colors',
            'hover:bg-secondary/50 hover:text-foreground',
          )}
        >
          <Settings2 className="h-3 w-3" aria-hidden="true" />
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-72 space-y-3 text-[12px]">
        <div className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          Editor preferences
        </div>
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-[12px]" htmlFor="editor-pref-font-size">
              Font size
            </Label>
            <span className="text-[11px] tabular-nums text-muted-foreground">
              {preferences.fontSize}px
            </span>
          </div>
          <Slider
            id="editor-pref-font-size"
            min={10}
            max={22}
            step={1}
            value={[preferences.fontSize]}
            onValueChange={(values) =>
              onPreferencesChange({
                ...preferences,
                fontSize: clampFontSize(values[0] ?? preferences.fontSize),
              })
            }
          />
        </div>
        <div className="space-y-2">
          <div className="flex items-center justify-between gap-3">
            <Label className="text-[12px]" htmlFor="editor-pref-tab-size">
              Tab size
            </Label>
            <span className="text-[11px] tabular-nums text-muted-foreground">
              {preferences.tabSize}
            </span>
          </div>
          <Slider
            id="editor-pref-tab-size"
            min={1}
            max={8}
            step={1}
            value={[preferences.tabSize]}
            onValueChange={(values) =>
              onPreferencesChange({
                ...preferences,
                tabSize: clampTabSize(values[0] ?? preferences.tabSize),
              })
            }
          />
        </div>
        <div className="flex items-center justify-between gap-3">
          <Label className="text-[12px]" htmlFor="editor-pref-insert-spaces">
            Insert spaces
          </Label>
          <Switch
            id="editor-pref-insert-spaces"
            checked={preferences.insertSpaces}
            onCheckedChange={(checked) =>
              onPreferencesChange({ ...preferences, insertSpaces: checked })
            }
          />
        </div>
        <div className="flex items-center justify-between gap-3">
          <Label className="text-[12px]" htmlFor="editor-pref-line-wrapping">
            Line wrapping
          </Label>
          <Switch
            id="editor-pref-line-wrapping"
            checked={preferences.lineWrapping}
            onCheckedChange={(checked) =>
              onPreferencesChange({ ...preferences, lineWrapping: checked })
            }
          />
        </div>
      </PopoverContent>
    </Popover>
  )
}
