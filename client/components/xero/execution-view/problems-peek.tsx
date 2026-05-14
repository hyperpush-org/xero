"use client"

import { useEffect, useMemo, useRef } from 'react'
import { CheckCircle2, Play, Sparkles, X, XCircle } from 'lucide-react'

import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import type {
  ProjectDiagnosticDto,
  ProjectLintResponseDto,
  ProjectTypecheckResponseDto,
} from '@/src/lib/xero-model'

export type ProblemsPeekTypecheckState =
  | { status: 'idle'; response: null; error: null }
  | { status: 'running'; response: ProjectTypecheckResponseDto | null; error: null }
  | { status: 'ready'; response: ProjectTypecheckResponseDto; error: null }
  | { status: 'error'; response: ProjectTypecheckResponseDto | null; error: string }

export type ProblemsPeekLintState =
  | { status: 'idle'; response: null; error: null }
  | { status: 'running'; response: ProjectLintResponseDto | null; error: null }
  | { status: 'ready'; response: ProjectLintResponseDto; error: null }
  | { status: 'error'; response: ProjectLintResponseDto | null; error: string }

export type ProblemsPeekFormatStatus =
  | { status: 'idle' }
  | { status: 'running' }
  | { status: 'formatted'; message?: string }
  | { status: 'unchanged'; message?: string }
  | { status: 'unavailable'; message?: string }
  | { status: 'failed'; message: string }

export interface ProblemsPeekEditorTaskState {
  status: 'running' | 'passed' | 'failed'
  label: string
  message: string | null
  diagnostics: ProjectDiagnosticDto[]
  truncated: boolean
}

export interface ProblemsPeekProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  typecheckState: ProblemsPeekTypecheckState
  lintState: ProblemsPeekLintState
  editorTaskStates: ProblemsPeekEditorTaskState[]
  formatStatus: ProblemsPeekFormatStatus
  onRunTypecheck?: () => void
  onRunLint?: () => void
  onOpenAtLine: (path: string, line: number, column: number) => void
}

export function ProblemsPeek({
  open,
  onOpenChange,
  typecheckState,
  lintState,
  editorTaskStates,
  formatStatus,
  onRunTypecheck,
  onRunLint,
  onOpenAtLine,
}: ProblemsPeekProps) {
  const panelRef = useRef<HTMLDivElement | null>(null)

  const diagnostics = useMemo(() => {
    const typecheckDiagnostics = typecheckState.response?.diagnostics ?? []
    const lintDiagnostics = lintState.response?.diagnostics ?? []
    const taskDiagnostics = editorTaskStates.flatMap((task) => task.diagnostics)
    return [...typecheckDiagnostics, ...lintDiagnostics, ...taskDiagnostics]
  }, [editorTaskStates, lintState, typecheckState])

  const { summary, hasFailure, truncated } = useMemo(() => {
    const errorCount = diagnostics.filter((diagnostic) => diagnostic.severity === 'error').length
    const warningCount = diagnostics.filter(
      (diagnostic) => diagnostic.severity === 'warning',
    ).length

    const parts: string[] = []
    if (typecheckState.status === 'running') parts.push('Typecheck running')
    if (lintState.status === 'running') parts.push('Lint running')
    for (const task of editorTaskStates) {
      if (task.status === 'running') parts.push(`${task.label} running`)
      if (task.status === 'failed' && task.message) parts.push(task.message)
      if (task.status === 'passed' && task.message && diagnostics.length === 0) {
        parts.push(task.message)
      }
    }
    if (formatStatus.status === 'running') parts.push('Formatting')
    if (formatStatus.status === 'failed') parts.push(formatStatus.message)
    if (formatStatus.status === 'formatted' && formatStatus.message) {
      parts.push(formatStatus.message)
    }
    if (typecheckState.status === 'error') parts.push(typecheckState.error)
    if (lintState.status === 'error') parts.push(lintState.error)
    if (
      typecheckState.response?.status === 'unavailable' &&
      typecheckState.response.message
    ) {
      parts.push(typecheckState.response.message)
    }
    if (lintState.response?.status === 'unavailable' && lintState.response.message) {
      parts.push(lintState.response.message)
    }
    if (diagnostics.length > 0) {
      parts.push(
        `${errorCount} error${errorCount === 1 ? '' : 's'} · ${warningCount} warning${warningCount === 1 ? '' : 's'}`,
      )
    }
    if (parts.length === 0) parts.push('No problems')
    const truncated = !!(
      typecheckState.response?.truncated ||
      lintState.response?.truncated ||
      editorTaskStates.some((task) => task.truncated)
    )
    const hasFailure =
      diagnostics.length > 0 ||
      typecheckState.status === 'error' ||
      lintState.status === 'error' ||
      editorTaskStates.some((task) => task.status === 'failed') ||
      formatStatus.status === 'failed'
    return { summary: parts.join(' · '), hasFailure, truncated }
  }, [diagnostics, editorTaskStates, formatStatus, lintState, typecheckState])

  useEffect(() => {
    if (!open) return
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onOpenChange(false)
      }
    }
    const handlePointerDown = (event: MouseEvent) => {
      const target = event.target as Node | null
      if (!target) return
      if (panelRef.current && !panelRef.current.contains(target)) {
        // Allow clicks on the status-bar trigger to toggle without
        // immediately re-closing the panel.
        const interactiveTrigger = (target as HTMLElement).closest?.(
          '[data-problems-peek-trigger="true"]',
        )
        if (interactiveTrigger) return
        onOpenChange(false)
      }
    }
    document.addEventListener('keydown', handleKey)
    document.addEventListener('mousedown', handlePointerDown)
    return () => {
      document.removeEventListener('keydown', handleKey)
      document.removeEventListener('mousedown', handlePointerDown)
    }
  }, [onOpenChange, open])

  if (!open) return null

  const lspServers = typecheckState.response?.lspServers ?? []
  const lspAvailable = lspServers.filter((server) => server.available).length

  return (
    <div
      ref={panelRef}
      aria-label="Problems"
      className={cn(
        'shrink-0 max-h-72 shadow-lg',
        'border-t border-border bg-popover',
      )}
      data-testid="problems-panel"
      role="dialog"
    >
      <div className="flex h-8 items-center justify-between gap-3 border-b border-border/70 px-3">
        <div className="flex min-w-0 items-center gap-2">
          {hasFailure ? (
            <XCircle className="h-3.5 w-3.5 text-destructive" aria-hidden="true" />
          ) : (
            <CheckCircle2 className="h-3.5 w-3.5 text-success" aria-hidden="true" />
          )}
          <span className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
            Problems
          </span>
          <span className="truncate text-[11px] text-muted-foreground">{summary}</span>
          {truncated ? (
            <span className="rounded bg-warning/15 px-1.5 py-0.5 text-[10px] text-warning">
              Truncated
            </span>
          ) : null}
        </div>
        <div className="flex items-center gap-1">
          {onRunLint ? (
            <Button
              className="h-6 gap-1 rounded px-2 text-[11px]"
              disabled={lintState.status === 'running'}
              onClick={onRunLint}
              size="sm"
              type="button"
              variant="ghost"
            >
              <Sparkles className="h-3 w-3" aria-hidden="true" />
              {lintState.status === 'running' ? 'Linting' : 'Lint'}
            </Button>
          ) : null}
          {onRunTypecheck ? (
            <Button
              className="h-6 gap-1 rounded px-2 text-[11px]"
              disabled={typecheckState.status === 'running'}
              onClick={onRunTypecheck}
              size="sm"
              type="button"
              variant="ghost"
            >
              <Play className="h-3 w-3" aria-hidden="true" />
              {typecheckState.status === 'running' ? 'Running' : 'Typecheck'}
            </Button>
          ) : null}
          <button
            type="button"
            aria-label="Close problems panel"
            className="inline-flex h-6 w-6 items-center justify-center rounded text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
            onClick={() => onOpenChange(false)}
          >
            <X className="h-3 w-3" aria-hidden="true" />
          </button>
        </div>
      </div>
      {diagnostics.length > 0 ? (
        <div className="max-h-56 overflow-auto py-1 scrollbar-thin">
          {diagnostics.map((diagnostic, index) => (
            <ProblemRow
              diagnostic={diagnostic}
              index={index}
              key={`${diagnostic.source}:${diagnostic.path ?? 'project'}:${diagnostic.line ?? 0}:${diagnostic.column ?? 0}:${diagnostic.code ?? index}`}
              onOpenAtLine={onOpenAtLine}
            />
          ))}
        </div>
      ) : (
        <div className="px-3 py-3 text-[11px] text-muted-foreground">
          {typecheckState.status === 'idle' && lintState.status === 'idle'
            ? 'Run typecheck or lint to populate project diagnostics.'
            : summary}
        </div>
      )}
      {lspServers.length ? (
        <div className="border-t border-border/70 px-3 py-1.5 text-[10px] text-muted-foreground">
          LSP servers: {lspAvailable}/{lspServers.length} available
        </div>
      ) : null}
    </div>
  )
}

function ProblemRow({
  diagnostic,
  index,
  onOpenAtLine,
}: {
  diagnostic: ProjectDiagnosticDto
  index: number
  onOpenAtLine: (path: string, line: number, column: number) => void
}) {
  const canOpen = !!diagnostic.path && !!diagnostic.line
  const location = diagnostic.path
    ? `${diagnostic.path}${diagnostic.line ? `:${diagnostic.line}:${diagnostic.column ?? 1}` : ''}`
    : 'Project'

  return (
    <button
      className="grid w-full grid-cols-[72px_minmax(120px,220px)_1fr] items-start gap-2 px-3 py-1.5 text-left text-[11px] hover:bg-muted/40 disabled:cursor-default disabled:hover:bg-transparent"
      disabled={!canOpen}
      onClick={() => {
        if (diagnostic.path && diagnostic.line) {
          onOpenAtLine(diagnostic.path, diagnostic.line, diagnostic.column ?? 1)
        }
      }}
      type="button"
    >
      <span className={diagnostic.severity === 'error' ? 'text-destructive' : 'text-warning'}>
        {diagnostic.severity}
      </span>
      <span className="truncate font-mono text-muted-foreground" title={location}>
        {location}
      </span>
      <span className="min-w-0 text-foreground/85">
        {diagnostic.code ? (
          <span className="mr-1 font-mono text-muted-foreground">{diagnostic.code}</span>
        ) : null}
        {diagnostic.message || `Diagnostic ${index + 1}`}
      </span>
    </button>
  )
}
