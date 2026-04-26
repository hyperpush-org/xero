"use client"

import { useCallback, useEffect, useMemo, useState } from 'react'
import { save } from '@tauri-apps/plugin-dialog'
import {
  AlertCircle,
  Archive,
  Copy,
  Download,
  FileJson,
  GitBranch,
  History,
  Loader2,
  RefreshCw,
  RotateCcw,
} from 'lucide-react'

import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
import type { AgentSessionView } from '@/src/lib/cadence-model'
import type {
  AgentSessionBranchResponseDto,
  BranchAgentSessionRequestDto,
  ExportSessionTranscriptRequestDto,
  GetSessionTranscriptRequestDto,
  RewindAgentSessionRequestDto,
  SessionTranscriptDto,
  SessionTranscriptExportResponseDto,
  SessionTranscriptItemDto,
  SessionTranscriptSearchResultSnippetDto,
} from '@/src/lib/cadence-model/session-context'

import { displayValue } from './shared-helpers'

export interface SessionHistoryTarget {
  agentSessionId: string
  runId?: string | null
  source?: 'search' | 'session' | 'run'
  nonce: number
}

interface SessionHistorySectionProps {
  projectId: string
  selectedSession: AgentSessionView | null
  historyTarget?: SessionHistoryTarget | null
  searchResult?: SessionTranscriptSearchResultSnippetDto | null
  onLoadTranscript?: (request: GetSessionTranscriptRequestDto) => Promise<SessionTranscriptDto>
  onExportTranscript?: (request: ExportSessionTranscriptRequestDto) => Promise<SessionTranscriptExportResponseDto>
  onSaveTranscriptExport?: (request: { path: string; content: string }) => Promise<void>
  onBranchSession?: (
    request: Omit<BranchAgentSessionRequestDto, 'projectId'>,
  ) => Promise<AgentSessionBranchResponseDto>
  onRewindSession?: (
    request: Omit<RewindAgentSessionRequestDto, 'projectId'>,
  ) => Promise<AgentSessionBranchResponseDto>
}

type LoadStatus = 'idle' | 'loading' | 'ready' | 'error'
type ExportAction = 'copy-markdown' | 'save-markdown' | 'save-json'
type HistoryMutationAction =
  | { kind: 'branch'; targetId: string }
  | { kind: 'rewind'; targetId: string }

export function SessionHistorySection({
  projectId,
  selectedSession,
  historyTarget,
  searchResult,
  onLoadTranscript,
  onExportTranscript,
  onSaveTranscriptExport,
  onBranchSession,
  onRewindSession,
}: SessionHistorySectionProps) {
  const targetSessionId = historyTarget?.agentSessionId ?? selectedSession?.agentSessionId ?? null
  const targetRunId = historyTarget?.runId ?? null
  const [transcript, setTranscript] = useState<SessionTranscriptDto | null>(null)
  const [selectedRunId, setSelectedRunId] = useState<string | null>(targetRunId)
  const [status, setStatus] = useState<LoadStatus>('idle')
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [exportAction, setExportAction] = useState<ExportAction | null>(null)
  const [exportMessage, setExportMessage] = useState<string | null>(null)
  const [historyAction, setHistoryAction] = useState<HistoryMutationAction | null>(null)
  const [historyActionMessage, setHistoryActionMessage] = useState<string | null>(null)

  const canLoad = Boolean(projectId && targetSessionId && onLoadTranscript)
  const loadTranscript = useCallback(async () => {
    if (!projectId || !targetSessionId || !onLoadTranscript) return
    setStatus('loading')
    setErrorMessage(null)
    try {
      const loaded = await onLoadTranscript({
        projectId,
        agentSessionId: targetSessionId,
        runId: null,
      })
      setTranscript(loaded)
      setStatus('ready')
      setSelectedRunId((current) => {
        if (targetRunId && loaded.runs.some((run) => run.runId === targetRunId)) {
          return targetRunId
        }
        if (current && loaded.runs.some((run) => run.runId === current)) {
          return current
        }
        return loaded.runs[loaded.runs.length - 1]?.runId ?? null
      })
    } catch (error) {
      setStatus('error')
      setErrorMessage(error instanceof Error ? error.message : 'Cadence could not load this session history.')
    }
  }, [onLoadTranscript, projectId, targetRunId, targetSessionId])

  useEffect(() => {
    setTranscript(null)
    setSelectedRunId(targetRunId)
    setExportMessage(null)
    setHistoryActionMessage(null)
    if (canLoad) {
      void loadTranscript()
    }
  }, [canLoad, historyTarget?.nonce, loadTranscript, targetRunId, targetSessionId])

  const selectedItems = useMemo(() => {
    if (!transcript || !selectedRunId) return []
    return transcript.items.filter((item) => item.runId === selectedRunId)
  }, [selectedRunId, transcript])

  const selectedRun = useMemo(
    () => transcript?.runs.find((run) => run.runId === selectedRunId) ?? null,
    [selectedRunId, transcript],
  )

  const handleCopyMarkdown = useCallback(async () => {
    if (!targetSessionId || !onExportTranscript) return
    setExportAction('copy-markdown')
    setExportMessage(null)
    try {
      const exported = await onExportTranscript({
        projectId,
        agentSessionId: targetSessionId,
        runId: selectedRunId,
        format: 'markdown',
      })
      if (typeof navigator === 'undefined' || !navigator.clipboard?.writeText) {
        throw new Error('Clipboard is unavailable in this webview.')
      }
      await navigator.clipboard.writeText(exported.content)
      setExportMessage('Markdown copied.')
    } catch (error) {
      setExportMessage(error instanceof Error ? error.message : 'Cadence could not copy the transcript.')
    } finally {
      setExportAction(null)
    }
  }, [onExportTranscript, projectId, selectedRunId, targetSessionId])

  const handleSave = useCallback(
    async (format: 'markdown' | 'json') => {
      if (!targetSessionId || !onExportTranscript || !onSaveTranscriptExport) return
      const action: ExportAction = format === 'markdown' ? 'save-markdown' : 'save-json'
      setExportAction(action)
      setExportMessage(null)
      try {
        const exported = await onExportTranscript({
          projectId,
          agentSessionId: targetSessionId,
          runId: selectedRunId,
          format,
        })
        const path = await save({
          defaultPath: exported.suggestedFileName,
          filters: [
            {
              name: format === 'markdown' ? 'Markdown' : 'JSON',
              extensions: [format === 'markdown' ? 'md' : 'json'],
            },
          ],
        })
        if (!path) {
          setExportMessage('Save cancelled.')
          return
        }
        await onSaveTranscriptExport({ path, content: exported.content })
        setExportMessage(`Saved ${exported.suggestedFileName}.`)
      } catch (error) {
        setExportMessage(error instanceof Error ? error.message : 'Cadence could not save the transcript.')
      } finally {
        setExportAction(null)
      }
    },
    [onExportTranscript, onSaveTranscriptExport, projectId, selectedRunId, targetSessionId],
  )

  const handleBranchRun = useCallback(
    async (runId: string) => {
      if (!targetSessionId || !onBranchSession) return
      const targetId = `branch:${runId}`
      setHistoryAction({ kind: 'branch', targetId })
      setHistoryActionMessage(null)
      try {
        const response = await onBranchSession({
          sourceAgentSessionId: targetSessionId,
          sourceRunId: runId,
          selected: true,
        })
        setHistoryActionMessage(`Branched to ${response.session.title}.`)
      } catch (error) {
        setHistoryActionMessage(error instanceof Error ? error.message : 'Cadence could not branch this session.')
      } finally {
        setHistoryAction(null)
      }
    },
    [onBranchSession, targetSessionId],
  )

  const handleRewindItem = useCallback(
    async (item: SessionTranscriptItemDto) => {
      if (!targetSessionId || !onRewindSession) return
      const boundary = getRewindBoundary(item)
      if (!boundary) return
      const targetId = `rewind:${item.runId}:${item.itemId}`
      setHistoryAction({ kind: 'rewind', targetId })
      setHistoryActionMessage(null)
      try {
        const response = await onRewindSession({
          sourceAgentSessionId: targetSessionId,
          sourceRunId: item.runId,
          boundaryKind: boundary.kind,
          sourceMessageId: boundary.kind === 'message' ? boundary.id : null,
          sourceCheckpointId: boundary.kind === 'checkpoint' ? boundary.id : null,
          selected: true,
        })
        setHistoryActionMessage(`Rewound to ${response.session.title}.`)
      } catch (error) {
        setHistoryActionMessage(error instanceof Error ? error.message : 'Cadence could not rewind this session.')
      } finally {
        setHistoryAction(null)
      }
    },
    [onRewindSession, targetSessionId],
  )

  if (!onLoadTranscript || !targetSessionId) {
    return null
  }

  const title = transcript?.title ?? selectedSession?.title ?? 'Session history'
  const archived = transcript?.archived ?? selectedSession?.isArchived ?? false

  return (
    <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <History className="h-4 w-4 text-muted-foreground" />
              <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
                Session history
              </p>
              {archived ? (
                <Badge variant="secondary" className="gap-1">
                  <Archive className="h-3 w-3" />
                  Archived
                </Badge>
              ) : null}
            </div>
            <h2 className="mt-2 truncate text-lg font-semibold text-foreground">{title}</h2>
            <p className="mt-1 truncate text-xs text-muted-foreground">{targetSessionId}</p>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={status === 'loading'}
              onClick={() => void loadTranscript()}
            >
              {status === 'loading' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <RefreshCw className="h-3.5 w-3.5" />
              )}
              Refresh
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!onExportTranscript || exportAction !== null || !transcript}
              onClick={() => void handleCopyMarkdown()}
            >
              {exportAction === 'copy-markdown' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Copy className="h-3.5 w-3.5" />
              )}
              Copy
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!onExportTranscript || !onSaveTranscriptExport || exportAction !== null || !transcript}
              onClick={() => void handleSave('markdown')}
            >
              {exportAction === 'save-markdown' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <Download className="h-3.5 w-3.5" />
              )}
              Markdown
            </Button>
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!onExportTranscript || !onSaveTranscriptExport || exportAction !== null || !transcript}
              onClick={() => void handleSave('json')}
            >
              {exportAction === 'save-json' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <FileJson className="h-3.5 w-3.5" />
              )}
              JSON
            </Button>
          </div>
        </div>

        {searchResult ? (
          <div className="rounded-lg border border-primary/20 bg-primary/5 px-3 py-2 text-sm">
            <p className="font-medium text-foreground">Search match</p>
            <p className="mt-1 leading-6 text-muted-foreground">{searchResult.snippet}</p>
          </div>
        ) : null}

        <LineageSummary session={selectedSession} />

        {status === 'error' ? (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Could not load history</AlertTitle>
            <AlertDescription>{errorMessage}</AlertDescription>
          </Alert>
        ) : null}

        {exportMessage ? (
          <div className="rounded-lg border border-border/70 bg-secondary/30 px-3 py-2 text-xs text-muted-foreground">
            {exportMessage}
          </div>
        ) : null}

        {historyActionMessage ? (
          <div className="rounded-lg border border-border/70 bg-secondary/30 px-3 py-2 text-xs text-muted-foreground">
            {historyActionMessage}
          </div>
        ) : null}

        {status === 'loading' && !transcript ? (
          <div className="flex items-center gap-2 rounded-xl border border-dashed border-border/70 bg-secondary/20 px-4 py-5 text-sm text-muted-foreground">
            <Loader2 className="h-4 w-4 animate-spin" />
            Loading session history…
          </div>
        ) : transcript ? (
          <div className="grid gap-4 lg:grid-cols-[minmax(180px,240px)_1fr]">
            <div className="min-w-0 rounded-xl border border-border/70 bg-card/70 p-3">
              <div className="flex items-center justify-between gap-2">
                <h3 className="text-sm font-semibold text-foreground">Runs</h3>
                <Badge variant="outline">
                  {transcript.runs.length}
                </Badge>
              </div>
              <div className="mt-3 flex max-h-[320px] flex-col gap-1 overflow-y-auto pr-1 scrollbar-thin">
                {transcript.runs.length > 0 ? (
                  transcript.runs.map((run) => {
                    const branchTargetId = `branch:${run.runId}`
                    const isBranching = historyAction?.kind === 'branch' && historyAction.targetId === branchTargetId
                    return (
                      <div
                        key={run.runId}
                        className={[
                          'flex items-center gap-1 rounded-md text-xs transition-colors',
                          run.runId === selectedRunId
                            ? 'bg-primary/10 text-foreground'
                            : 'text-muted-foreground hover:bg-secondary/60 hover:text-foreground',
                        ].join(' ')}
                      >
                        <button
                          type="button"
                          className="min-w-0 flex-1 px-2 py-2 text-left"
                          onClick={() => setSelectedRunId(run.runId)}
                        >
                          <span className="block truncate font-medium">{run.runId}</span>
                          <span className="mt-1 block truncate">{formatHistoryDate(run.startedAt)}</span>
                          <span className="mt-1 block truncate">{run.status}</span>
                        </button>
                        {onBranchSession ? (
                          <Tooltip>
                            <TooltipTrigger asChild>
                              <Button
                                type="button"
                                size="icon-sm"
                                variant="ghost"
                                aria-label={`Branch run ${run.runId}`}
                                className="mr-1 h-7 w-7 shrink-0"
                                disabled={historyAction !== null}
                                onClick={() => void handleBranchRun(run.runId)}
                              >
                                {isBranching ? (
                                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                                ) : (
                                  <GitBranch className="h-3.5 w-3.5" />
                                )}
                              </Button>
                            </TooltipTrigger>
                            <TooltipContent>Branch run</TooltipContent>
                          </Tooltip>
                        ) : null}
                      </div>
                    )
                  })
                ) : (
                  <HistoryEmptyState title="No runs" body="This session does not have persisted runs yet." />
                )}
              </div>
            </div>

            <div className="min-w-0 rounded-xl border border-border/70 bg-card/70 p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="min-w-0">
                  <h3 className="truncate text-sm font-semibold text-foreground">
                    {selectedRun ? selectedRun.runId : 'Transcript'}
                  </h3>
                  <p className="mt-1 truncate text-xs text-muted-foreground">
                    {selectedRun
                      ? `${selectedRun.providerId} · ${selectedRun.modelId}`
                      : 'Select a run to inspect its transcript.'}
                  </p>
                </div>
                {selectedRun?.usageTotals ? (
                  <Badge variant="secondary">{selectedRun.usageTotals.totalTokens.toLocaleString()} tokens</Badge>
                ) : null}
              </div>
              <div className="mt-3 max-h-[420px] space-y-3 overflow-y-auto pr-1 scrollbar-thin">
                {selectedItems.length > 0 ? (
                  selectedItems.map((item) => (
                    <TranscriptItem
                      key={`${item.runId}:${item.itemId}`}
                      item={item}
                      onRewind={onRewindSession ? handleRewindItem : undefined}
                      rewinding={
                        historyAction?.kind === 'rewind' &&
                        historyAction.targetId === `rewind:${item.runId}:${item.itemId}`
                      }
                      rewindDisabled={historyAction !== null}
                    />
                  ))
                ) : (
                  <HistoryEmptyState title="No transcript items" body="This run has no persisted transcript rows yet." />
                )}
              </div>
            </div>
          </div>
        ) : (
          <HistoryEmptyState
            title="History unavailable"
            body={displayValue(errorMessage, 'Cadence has not loaded this session history yet.')}
          />
        )}
      </div>
    </section>
  )
}

function LineageSummary({ session }: { session: AgentSessionView | null }) {
  const lineage = session?.lineage ?? null
  if (!lineage) return null

  const sourceLabel = lineage.sourceAgentSessionId
    ? `${lineage.sourceTitle} · ${lineage.sourceRunId ?? 'source run unavailable'}`
    : `${lineage.sourceTitle} · source deleted`
  const boundaryLabel = getLineageBoundaryLabel(lineage)

  return (
    <div className="rounded-lg border border-border/70 bg-secondary/20 px-3 py-2 text-sm">
      <div className="flex flex-wrap items-center gap-2">
        <GitBranch className="h-3.5 w-3.5 text-muted-foreground" />
        <p className="font-medium text-foreground">Branch lineage</p>
        <Badge variant="outline">{boundaryLabel}</Badge>
        {lineage.sourceDeletedAt ? <Badge variant="secondary">Source deleted</Badge> : null}
      </div>
      <p className="mt-1 truncate text-xs text-muted-foreground">{sourceLabel}</p>
      <p className="mt-2 text-xs leading-5 text-muted-foreground">{lineage.fileChangeSummary}</p>
      {lineage.diagnostic ? (
        <p className="mt-2 text-xs leading-5 text-muted-foreground">{lineage.diagnostic.message}</p>
      ) : null}
    </div>
  )
}

function TranscriptItem({
  item,
  onRewind,
  rewinding = false,
  rewindDisabled = false,
}: {
  item: SessionTranscriptItemDto
  onRewind?: (item: SessionTranscriptItemDto) => void
  rewinding?: boolean
  rewindDisabled?: boolean
}) {
  const boundary = getRewindBoundary(item)
  return (
    <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-2">
      <div className="flex items-start justify-between gap-2">
        <div className="flex flex-wrap items-center gap-2 text-[11px] text-muted-foreground">
          <Badge variant="outline">{item.sequence}</Badge>
          <span>{item.actor}</span>
          <span>{item.kind}</span>
          {item.toolName ? <Badge variant="secondary">{item.toolName}</Badge> : null}
        </div>
        {boundary && onRewind ? (
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                type="button"
                size="icon-sm"
                variant="ghost"
                aria-label={`Rewind session to ${boundary.kind} ${item.sequence}`}
                className="h-7 w-7 shrink-0"
                disabled={rewindDisabled}
                onClick={() => onRewind(item)}
              >
                {rewinding ? (
                  <Loader2 className="h-3.5 w-3.5 animate-spin" />
                ) : (
                  <RotateCcw className="h-3.5 w-3.5" />
                )}
              </Button>
            </TooltipTrigger>
            <TooltipContent>Rewind from here</TooltipContent>
          </Tooltip>
        ) : null}
      </div>
      <p className="mt-2 text-sm font-medium text-foreground">
        {displayValue(item.title, itemKindTitle(item.kind))}
      </p>
      {item.text ? <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-foreground/90">{item.text}</p> : null}
      {item.summary ? (
        <p className="mt-1 whitespace-pre-wrap text-sm leading-6 text-muted-foreground">{item.summary}</p>
      ) : null}
      {item.filePath ? <p className="mt-2 truncate font-mono text-[11px] text-muted-foreground">{item.filePath}</p> : null}
    </div>
  )
}

function getRewindBoundary(
  item: SessionTranscriptItemDto,
): { kind: 'message' | 'checkpoint'; id: number } | null {
  if (item.kind === 'message' && item.sourceTable === 'agent_messages') {
    const messageId = parsePositiveInteger(item.sourceId)
    return messageId ? { kind: 'message', id: messageId } : null
  }

  if (item.kind === 'checkpoint' && item.sourceTable === 'agent_checkpoints') {
    const checkpointId = parsePositiveInteger(item.sourceId)
    return checkpointId ? { kind: 'checkpoint', id: checkpointId } : null
  }

  return null
}

function parsePositiveInteger(value: string): number | null {
  if (!/^[1-9]\d*$/.test(value)) return null
  const parsed = Number.parseInt(value, 10)
  return Number.isSafeInteger(parsed) ? parsed : null
}

function getLineageBoundaryLabel(lineage: NonNullable<AgentSessionView['lineage']>): string {
  switch (lineage.sourceBoundaryKind) {
    case 'run':
      return 'Run branch'
    case 'message':
      return lineage.sourceMessageId ? `Message ${lineage.sourceMessageId}` : 'Message branch'
    case 'checkpoint':
      return lineage.sourceCheckpointId ? `Checkpoint ${lineage.sourceCheckpointId}` : 'Checkpoint branch'
  }
}

function HistoryEmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="rounded-xl border border-dashed border-border/70 bg-secondary/20 px-4 py-5 text-sm text-muted-foreground">
      <p className="font-medium text-foreground/85">{title}</p>
      <p className="mt-1 leading-6">{body}</p>
    </div>
  )
}

function itemKindTitle(kind: SessionTranscriptItemDto['kind']): string {
  switch (kind) {
    case 'message':
      return 'Message'
    case 'reasoning':
      return 'Reasoning'
    case 'tool_call':
      return 'Tool call'
    case 'tool_result':
      return 'Tool result'
    case 'file_change':
      return 'File change'
    case 'checkpoint':
      return 'Checkpoint'
    case 'action_request':
      return 'Action request'
    case 'activity':
      return 'Activity'
    case 'complete':
      return 'Run completed'
    case 'failure':
      return 'Run failed'
    case 'usage':
      return 'Usage'
  }
}

function formatHistoryDate(value: string): string {
  const parsed = Date.parse(value)
  if (!Number.isFinite(parsed)) return value
  return new Date(parsed).toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}
