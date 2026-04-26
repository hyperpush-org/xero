"use client"

import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  AlertTriangle,
  BarChart3,
  DatabaseZap,
  Gauge,
  Loader2,
  RefreshCw,
} from 'lucide-react'

import { cn } from '@/lib/utils'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import type { AgentSessionView } from '@/src/lib/cadence-model'
import type {
  CompactSessionHistoryRequestDto,
  CompactSessionHistoryResponseDto,
  DeleteSessionMemoryRequestDto,
  ExtractSessionMemoryCandidatesRequestDto,
  ExtractSessionMemoryCandidatesResponseDto,
  SessionCompactionRecordDto,
  GetSessionContextSnapshotRequestDto,
  ListSessionMemoriesRequestDto,
  ListSessionMemoriesResponseDto,
  SessionContextContributorDto,
  SessionContextSnapshotDto,
  SessionMemoryRecordDto,
  UpdateSessionMemoryRequestDto,
} from '@/src/lib/cadence-model/session-context'

import { MemoryReviewSection } from './memory-review-section'

interface ContextVisualizationSectionProps {
  projectId: string
  selectedSession: AgentSessionView | null
  runId?: string | null
  providerId?: string | null
  modelId?: string | null
  pendingPrompt?: string | null
  onLoadContextSnapshot?: (
    request: GetSessionContextSnapshotRequestDto,
  ) => Promise<SessionContextSnapshotDto>
  onCompactSessionHistory?: (
    request: CompactSessionHistoryRequestDto,
  ) => Promise<CompactSessionHistoryResponseDto>
  onListSessionMemories?: (
    request: ListSessionMemoriesRequestDto,
  ) => Promise<ListSessionMemoriesResponseDto>
  onExtractSessionMemoryCandidates?: (
    request: ExtractSessionMemoryCandidatesRequestDto,
  ) => Promise<ExtractSessionMemoryCandidatesResponseDto>
  onUpdateSessionMemory?: (request: UpdateSessionMemoryRequestDto) => Promise<SessionMemoryRecordDto>
  onDeleteSessionMemory?: (request: DeleteSessionMemoryRequestDto) => Promise<void>
}

type LoadStatus = 'idle' | 'loading' | 'ready' | 'error'
type CompactStatus = 'idle' | 'running' | 'success' | 'error'

const PRESSURE_META = {
  unknown: {
    label: 'Unknown budget',
    badgeVariant: 'outline' as const,
    className: 'text-muted-foreground',
  },
  low: {
    label: 'Low pressure',
    badgeVariant: 'secondary' as const,
    className: 'text-emerald-700 dark:text-emerald-300',
  },
  medium: {
    label: 'Medium pressure',
    badgeVariant: 'outline' as const,
    className: 'text-amber-700 dark:text-amber-300',
  },
  high: {
    label: 'High pressure',
    badgeVariant: 'outline' as const,
    className: 'text-orange-700 dark:text-orange-300',
  },
  over: {
    label: 'Over budget',
    badgeVariant: 'destructive' as const,
    className: 'text-destructive',
  },
}

export function ContextVisualizationSection({
  projectId,
  selectedSession,
  runId,
  providerId,
  modelId,
  pendingPrompt,
  onLoadContextSnapshot,
  onCompactSessionHistory,
  onListSessionMemories,
  onExtractSessionMemoryCandidates,
  onUpdateSessionMemory,
  onDeleteSessionMemory,
}: ContextVisualizationSectionProps) {
  const targetSessionId = selectedSession?.agentSessionId ?? null
  const [snapshot, setSnapshot] = useState<SessionContextSnapshotDto | null>(null)
  const [status, setStatus] = useState<LoadStatus>('idle')
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [compactStatus, setCompactStatus] = useState<CompactStatus>('idle')
  const [compactMessage, setCompactMessage] = useState<string | null>(null)

  const canLoad = Boolean(projectId && targetSessionId && onLoadContextSnapshot)
  const loadSnapshot = useCallback(async () => {
    if (!projectId || !targetSessionId || !onLoadContextSnapshot) return
    setStatus('loading')
    setErrorMessage(null)
    try {
      const loaded = await onLoadContextSnapshot({
        projectId,
        agentSessionId: targetSessionId,
        runId: runId ?? null,
        providerId: providerId ?? null,
        modelId: modelId ?? null,
        pendingPrompt: pendingPrompt?.trim() ? pendingPrompt : null,
      })
      setSnapshot(loaded)
      setStatus('ready')
    } catch (error) {
      setStatus('error')
      setErrorMessage(error instanceof Error ? error.message : 'Cadence could not load context usage.')
    }
  }, [
    modelId,
    onLoadContextSnapshot,
    pendingPrompt,
    projectId,
    providerId,
    runId,
    targetSessionId,
  ])

  const canCompact = Boolean(projectId && targetSessionId && runId && onCompactSessionHistory)
  const handleCompact = useCallback(async () => {
    if (!projectId || !targetSessionId || !runId || !onCompactSessionHistory) return
    setCompactStatus('running')
    setCompactMessage(null)
    try {
      const response = await onCompactSessionHistory({
        projectId,
        agentSessionId: targetSessionId,
        runId,
        rawTailMessageCount: 8,
      })
      setSnapshot(response.contextSnapshot)
      setStatus('ready')
      setCompactStatus('success')
      setCompactMessage(formatCompactionSuccess(response.compaction))
    } catch (error) {
      setCompactStatus('error')
      setCompactMessage(error instanceof Error ? error.message : 'Cadence could not compact this session.')
    }
  }, [onCompactSessionHistory, projectId, runId, targetSessionId])

  useEffect(() => {
    if (!canLoad) {
      setSnapshot(null)
      setStatus('idle')
      return
    }
    const timeout = window.setTimeout(() => {
      void loadSnapshot()
    }, pendingPrompt?.trim() ? 220 : 0)
    return () => window.clearTimeout(timeout)
  }, [canLoad, loadSnapshot, pendingPrompt])

  const visibleContributors = useMemo(
    () => snapshot?.contributors.filter((contributor) => contributor.included && contributor.modelVisible) ?? [],
    [snapshot],
  )
  const topContributors = useMemo(
    () =>
      visibleContributors
        .slice()
        .sort((left, right) => {
          const tokenDelta = right.estimatedTokens - left.estimatedTokens
          return tokenDelta !== 0 ? tokenDelta : left.sequence - right.sequence
        })
        .slice(0, 7),
    [visibleContributors],
  )
  const usageContributorCount = snapshot?.contributors.filter((contributor) => contributor.kind === 'provider_usage').length ?? 0
  const approvedMemoryCount = snapshot?.contributors.filter((contributor) => contributor.kind === 'approved_memory').length ?? 0
  const hasCompactionSummary = Boolean(
    snapshot?.contributors.some((contributor) => contributor.kind === 'compaction_summary'),
  )
  const hasInstructionFile = Boolean(
    snapshot?.contributors.some((contributor) => contributor.kind === 'instruction_file'),
  )
  const toolDescriptorCount = snapshot?.contributors.filter((contributor) => contributor.kind === 'tool_descriptor').length ?? 0
  const budgetPercent =
    snapshot?.budget.budgetTokens && snapshot.budget.budgetTokens > 0
      ? Math.min(100, Math.round((snapshot.budget.estimatedTokens / snapshot.budget.budgetTokens) * 100))
      : null
  const pressureMeta = snapshot ? PRESSURE_META[snapshot.budget.pressure] : PRESSURE_META.unknown
  const budgetLabel = snapshot
    ? snapshot.budget.budgetTokens
      ? `${formatTokens(snapshot.budget.estimatedTokens)} / ${formatTokens(snapshot.budget.budgetTokens)}`
      : `${formatTokens(snapshot.budget.estimatedTokens)} estimated`
    : 'Context'
  const shouldWarn = snapshot?.budget.pressure === 'high' || snapshot?.budget.pressure === 'over'
  const warningTitle = snapshot?.budget.pressure === 'over' ? 'Likely over context budget' : 'Context pressure is high'
  const noReplayYet = Boolean(snapshot && !snapshot.runId && visibleContributors.length === (hasInstructionFile ? 1 : 0))

  if (!onLoadContextSnapshot || !targetSessionId) {
    return null
  }

  return (
    <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div className="min-w-0">
            <div className="flex items-center gap-2">
              <Gauge className="h-4 w-4 text-muted-foreground" />
              <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
                Context
              </p>
              {snapshot ? (
                <Badge variant={pressureMeta.badgeVariant} className={cn('gap-1', pressureMeta.className)}>
                  <BarChart3 className="h-3 w-3" />
                  {pressureMeta.label}
                </Badge>
              ) : null}
            </div>
            <h2 className="mt-2 truncate text-lg font-semibold text-foreground">{budgetLabel}</h2>
            <p className="mt-1 truncate text-xs text-muted-foreground">
              {snapshot ? `${snapshot.providerId} / ${snapshot.modelId}` : selectedSession?.title ?? targetSessionId}
            </p>
          </div>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={status === 'loading'}
            onClick={() => void loadSnapshot()}
          >
            {status === 'loading' ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <RefreshCw className="h-3.5 w-3.5" />
            )}
            Refresh
          </Button>
          {onCompactSessionHistory ? (
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={!canCompact || compactStatus === 'running' || status === 'loading'}
              onClick={() => void handleCompact()}
            >
              {compactStatus === 'running' ? (
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
              ) : (
                <DatabaseZap className="h-3.5 w-3.5" />
              )}
              Compact
            </Button>
          ) : null}
        </div>

        {status === 'error' ? (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Context unavailable</AlertTitle>
            <AlertDescription>{errorMessage}</AlertDescription>
          </Alert>
        ) : null}

        {compactStatus === 'success' && compactMessage ? (
          <Alert>
            <DatabaseZap className="h-4 w-4" />
            <AlertTitle>Session compacted</AlertTitle>
            <AlertDescription>{compactMessage}</AlertDescription>
          </Alert>
        ) : null}

        {compactStatus === 'error' && compactMessage ? (
          <Alert variant="destructive">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>Compact failed</AlertTitle>
            <AlertDescription>{compactMessage}</AlertDescription>
          </Alert>
        ) : null}

        {shouldWarn ? (
          <Alert variant={snapshot?.budget.pressure === 'over' ? 'destructive' : 'default'} className="border-orange-500/40">
            <AlertTriangle className="h-4 w-4" />
            <AlertTitle>{warningTitle}</AlertTitle>
            <AlertDescription>
              {snapshot?.budget.pressure === 'over'
                ? 'This continuation is expected to exceed the active model budget.'
                : 'This session is close to the active model budget.'}
            </AlertDescription>
          </Alert>
        ) : null}

        <div className="grid gap-3 sm:grid-cols-3">
          <ContextMetric label="Visible pieces" value={visibleContributors.length.toLocaleString()} />
          <ContextMetric label="Tool schemas" value={toolDescriptorCount.toLocaleString()} />
          <ContextMetric
            label="Usage source"
            value={formatUsageSource(snapshot?.budget.estimationSource ?? 'unavailable')}
          />
        </div>

        {snapshot ? (
          <div className="flex flex-col gap-3">
            {budgetPercent !== null ? (
              <div className="space-y-2">
                <div className="flex items-center justify-between gap-3 text-xs text-muted-foreground">
                  <span>{budgetPercent}% of known budget</span>
                  <span>{snapshot.budget.knownProviderBudget ? 'Provider budget known' : 'Provider budget unknown'}</span>
                </div>
                <Progress value={budgetPercent} className="h-2" />
              </div>
            ) : (
              <div className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
                Provider budget is unknown for this model.
              </div>
            )}

            <div className="flex flex-wrap gap-2">
              <Badge variant="secondary" className="gap-1">
                <DatabaseZap className="h-3 w-3" />
                {hasCompactionSummary ? 'Compacted replay' : 'Raw history replay'}
              </Badge>
              {hasInstructionFile ? <Badge variant="outline">AGENTS.md included</Badge> : null}
              {usageContributorCount > 0 ? (
                <Badge variant="outline">{usageContributorCount.toLocaleString()} usage rollup</Badge>
              ) : null}
              {approvedMemoryCount > 0 ? (
                <Badge variant="outline">{approvedMemoryCount.toLocaleString()} approved memory</Badge>
              ) : null}
            </div>

            {noReplayYet ? (
              <p className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2 text-xs text-muted-foreground">
                No provider replay has been recorded for this session yet.
              </p>
            ) : null}

            <div className="divide-y divide-border/60 overflow-hidden rounded-lg border border-border/60">
              {topContributors.length > 0 ? (
                topContributors.map((contributor) => (
                  <ContributorRow key={contributor.contributorId} contributor={contributor} />
                ))
              ) : (
                <p className="px-3 py-3 text-sm text-muted-foreground">No model-visible context yet.</p>
              )}
            </div>

            {snapshot.usageTotals ? (
              <p className="text-xs text-muted-foreground">
                Provider usage: {formatTokens(snapshot.usageTotals.totalTokens)} recorded.
              </p>
            ) : null}

            <MemoryReviewSection
              projectId={projectId}
              selectedSession={selectedSession}
              runId={runId}
              onListSessionMemories={onListSessionMemories}
              onExtractSessionMemoryCandidates={onExtractSessionMemoryCandidates}
              onUpdateSessionMemory={onUpdateSessionMemory}
              onDeleteSessionMemory={onDeleteSessionMemory}
              onContextRefresh={loadSnapshot}
            />
          </div>
        ) : status === 'loading' ? (
          <p className="text-sm text-muted-foreground">Loading context usage...</p>
        ) : null}
      </div>
    </section>
  )
}

function ContextMetric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-lg border border-border/60 bg-muted/20 px-3 py-2">
      <p className="text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">{label}</p>
      <p className="mt-1 text-sm font-semibold text-foreground">{value}</p>
    </div>
  )
}

function ContributorRow({ contributor }: { contributor: SessionContextContributorDto }) {
  return (
    <div className="grid gap-2 px-3 py-3 sm:grid-cols-[minmax(0,1fr)_auto] sm:items-start">
      <div className="min-w-0">
        <div className="flex min-w-0 flex-wrap items-center gap-2">
          <p className="truncate text-sm font-medium text-foreground">{contributor.label}</p>
          <Badge variant="outline" className="capitalize">
            {contributor.kind.replace(/_/g, ' ')}
          </Badge>
        </div>
        {contributor.text ? (
          <p className="mt-1 line-clamp-2 text-xs leading-relaxed text-muted-foreground">{contributor.text}</p>
        ) : null}
      </div>
      <div className="text-left text-xs font-medium text-muted-foreground sm:text-right">
        {formatTokens(contributor.estimatedTokens)}
      </div>
    </div>
  )
}

function formatTokens(value: number): string {
  if (value >= 1_000_000) {
    return `${(value / 1_000_000).toFixed(value >= 10_000_000 ? 0 : 1)}M tokens`
  }
  if (value >= 1_000) {
    return `${(value / 1_000).toFixed(value >= 10_000 ? 0 : 1)}K tokens`
  }
  return `${value.toLocaleString()} tokens`
}

function formatUsageSource(value: SessionContextSnapshotDto['budget']['estimationSource']): string {
  switch (value) {
    case 'provider':
      return 'Provider'
    case 'mixed':
      return 'Mixed'
    case 'estimated':
      return 'Estimated'
    case 'unavailable':
      return 'Unavailable'
  }
}

function formatCompactionSuccess(compaction: SessionCompactionRecordDto): string {
  const runCount = compaction.coveredRunIds.length
  const runLabel = runCount === 1 ? '1 run' : `${runCount.toLocaleString()} runs`
  const messageRange =
    compaction.coveredMessageStartId && compaction.coveredMessageEndId
      ? `messages ${compaction.coveredMessageStartId}-${compaction.coveredMessageEndId}`
      : 'older messages'

  return `Cadence compacted ${messageRange} across ${runLabel} and preserved the latest ${compaction.rawTailMessageCount.toLocaleString()} raw messages for replay.`
}
