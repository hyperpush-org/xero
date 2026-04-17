"use client"

import { useMemo, useState } from 'react'
import type {
  ExecutionPaneView,
  RepositoryDiffState,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  RepositoryDiffScope,
  ResumeHistoryEntryView,
  VerificationRecordView,
} from '@/src/lib/cadence-model'
import {
  AlertCircle,
  Check,
  ChevronRight,
  FileCode,
  GitBranch,
  Hash,
  Loader2,
  RefreshCw,
} from 'lucide-react'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'

interface ExecutionViewProps {
  execution: ExecutionPaneView
  activeDiffScope: RepositoryDiffScope
  activeDiff: RepositoryDiffState
  onSelectDiffScope: (scope: RepositoryDiffScope) => void
  onRetryDiff: () => void
}

type ExecutionTab = 'waves' | 'changes' | 'verify'

type DiffLineKind = 'add' | 'del' | 'meta' | 'context'

type BadgeVariant = 'default' | 'secondary' | 'outline' | 'destructive'
type NotificationBrokerRouteView = ExecutionPaneView['notificationBroker']['routes'][number]

interface DiffLineView {
  kind: DiffLineKind
  content: string
}

const TAB_LABELS: Record<ExecutionTab, string> = {
  waves: 'Execution',
  changes: 'Changes',
  verify: 'Verify',
}

function parsePatch(patch: string): DiffLineView[] {
  return patch.split('\n').map((content) => {
    if (content.startsWith('+') && !content.startsWith('+++')) {
      return { kind: 'add', content }
    }

    if (content.startsWith('-') && !content.startsWith('---')) {
      return { kind: 'del', content }
    }

    if (
      content.startsWith('diff ') ||
      content.startsWith('index ') ||
      content.startsWith('@@') ||
      content.startsWith('---') ||
      content.startsWith('+++')
    ) {
      return { kind: 'meta', content }
    }

    return { kind: 'context', content }
  })
}

function displayValue(value: string | null | undefined, fallback: string): string {
  if (typeof value !== 'string') {
    return fallback
  }

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : fallback
}

function getTimestampValue(timestamp: string | null | undefined): number {
  if (typeof timestamp !== 'string' || timestamp.trim().length === 0) {
    return 0
  }

  const parsed = new Date(timestamp)
  return Number.isNaN(parsed.getTime()) ? 0 : parsed.getTime()
}

function sortByNewest<T>(values: T[], getTimestamp: (value: T) => string | null | undefined): T[] {
  return [...values].sort((left, right) => getTimestampValue(getTimestamp(right)) - getTimestampValue(getTimestamp(left)))
}

function formatTimestamp(timestamp: string | null | undefined): string {
  if (typeof timestamp !== 'string' || timestamp.trim().length === 0) {
    return 'Unknown'
  }

  const parsed = new Date(timestamp)
  if (Number.isNaN(parsed.getTime())) {
    return timestamp
  }

  return parsed.toLocaleString()
}

function getVerificationBadgeVariant(status: VerificationRecordView['status']): BadgeVariant {
  switch (status) {
    case 'pending':
      return 'secondary'
    case 'passed':
      return 'default'
    case 'failed':
      return 'destructive'
  }
}

function getResumeBadgeVariant(status: ResumeHistoryEntryView['status']): BadgeVariant {
  switch (status) {
    case 'started':
      return 'default'
    case 'failed':
      return 'destructive'
  }
}

function getRouteDiagnosticBadgeVariant(route: NotificationBrokerRouteView): BadgeVariant {
  if (route.hasFailures) {
    return 'destructive'
  }

  if (route.hasPending) {
    return 'secondary'
  }

  return 'default'
}

function getRouteDiagnosticLabel(route: NotificationBrokerRouteView): string {
  if (route.hasFailures) {
    return 'Needs attention'
  }

  if (route.hasPending) {
    return 'Pending replies'
  }

  return 'Healthy'
}

function StatusBadge({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="rounded-md border border-border bg-card/70 px-3 py-2">
      <p className="text-[10px] uppercase tracking-wide text-muted-foreground">{label}</p>
      <p className="mt-1 font-mono text-[13px] text-foreground/80">{value}</p>
    </div>
  )
}

function VerifyEmptyState({ title, body }: { title: string; body: string }) {
  return (
    <div className="rounded-xl border border-dashed border-border/70 bg-secondary/20 px-4 py-5 text-sm text-muted-foreground">
      <p className="font-medium text-foreground/85">{title}</p>
      <p className="mt-1 leading-6">{body}</p>
    </div>
  )
}

export function ExecutionView({
  execution,
  activeDiffScope,
  activeDiff,
  onSelectDiffScope,
  onRetryDiff,
}: ExecutionViewProps) {
  const [activeTab, setActiveTab] = useState<ExecutionTab>('waves')
  const diffLines = useMemo(() => parsePatch(activeDiff.diff?.patch ?? ''), [activeDiff.diff?.patch])
  const verificationRecords = useMemo(
    () => sortByNewest(execution.verificationRecords, (record) => record.recordedAt).slice(0, 8),
    [execution.verificationRecords],
  )
  const resumeHistory = useMemo(
    () => sortByNewest(execution.resumeHistory, (entry) => entry.createdAt).slice(0, 8),
    [execution.resumeHistory],
  )
  const routeDiagnostics = useMemo(
    () => execution.notificationBroker.routes.slice(0, 8),
    [execution.notificationBroker.routes],
  )
  const failedRouteCount = useMemo(
    () => routeDiagnostics.filter((route) => route.hasFailures).length,
    [routeDiagnostics],
  )
  const pendingRouteCount = useMemo(
    () => routeDiagnostics.filter((route) => route.hasPending).length,
    [routeDiagnostics],
  )
  const hasDurableVerificationState = verificationRecords.length > 0 || resumeHistory.length > 0

  const handleSelectTab = (tab: ExecutionTab) => {
    setActiveTab(tab)

    if (tab === 'changes') {
      onSelectDiffScope(activeDiffScope)
    }
  }

  return (
    <div className="flex min-h-0 min-w-0 flex-1 flex-col">
      <div className="flex items-center border-b border-border shrink-0">
        <div className="border-r border-border px-5 py-3">
          <p className="mb-0.5 text-[10px] text-muted-foreground">Phase {execution.activePhase?.id ?? '—'}</p>
          <h2 className="text-[13px] font-medium text-foreground">{execution.activePhase?.name ?? 'No active phase yet'}</h2>
        </div>

        <nav className="flex h-full items-center">
          {(['waves', 'changes', 'verify'] as const).map((tab) => (
            <button
              className={`-mb-px border-b px-4 py-3 text-[12px] font-medium capitalize transition-colors ${
                activeTab === tab
                  ? 'border-foreground text-foreground'
                  : 'border-transparent text-muted-foreground hover:text-foreground/70'
              }`}
              key={tab}
              onClick={() => handleSelectTab(tab)}
              type="button"
            >
              {TAB_LABELS[tab]}
            </button>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-3 px-5 text-[11px] text-muted-foreground">
          <div className="flex items-center gap-1">
            <GitBranch className="h-3.5 w-3.5" />
            <span className="font-mono">{execution.branchLabel}</span>
          </div>
          <div className="flex items-center gap-1">
            <Hash className="h-3.5 w-3.5" />
            <span className="font-mono">{execution.headShaLabel}</span>
          </div>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto scrollbar-thin">
        {activeTab === 'waves' ? (
          <div className="max-w-4xl space-y-4 p-5">
            <div className="grid gap-3 sm:grid-cols-4">
              <StatusBadge label="Selected project" value={execution.project.id} />
              <StatusBadge label="Tracked paths" value={execution.statusCount} />
              <StatusBadge label="Branch" value={execution.branchLabel} />
              <StatusBadge label="HEAD" value={execution.headShaLabel} />
            </div>

            <div className="rounded-lg border border-border bg-card p-4">
              <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">Execution availability</p>
              <h3 className="mt-2 text-[15px] font-semibold text-foreground">No live waves yet</h3>
              <p className="mt-2 text-[13px] leading-6 text-muted-foreground">{execution.executionUnavailableReason}</p>
            </div>

            <div className="rounded-lg border border-border bg-card/70 p-4">
              <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">Current repository truth</p>
              {execution.statusEntries.length > 0 ? (
                <div className="mt-3 space-y-2">
                  {execution.statusEntries.map((entry) => (
                    <div key={entry.path} className="flex items-center gap-3 rounded-md border border-border/70 px-3 py-2 text-[12px]">
                      <FileCode className="h-3.5 w-3.5 text-muted-foreground" />
                      <span className="min-w-0 flex-1 truncate font-mono text-foreground/80">{entry.path}</span>
                      <div className="flex items-center gap-2 text-[10px] font-medium uppercase tracking-wide">
                        {entry.staged ? <span className="rounded bg-success/10 px-1.5 py-0.5 text-success">staged</span> : null}
                        {entry.unstaged ? <span className="rounded bg-foreground/10 px-1.5 py-0.5 text-foreground/70">unstaged</span> : null}
                        {entry.untracked ? <span className="rounded bg-secondary px-1.5 py-0.5 text-muted-foreground">untracked</span> : null}
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <p className="mt-3 text-[13px] leading-6 text-muted-foreground">The repository is currently clean, so there are no live execution-side file changes to show here.</p>
              )}
            </div>

            <div className="rounded-lg border border-border bg-card/70 p-4">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">Channel dispatch diagnostics</p>
                <Badge variant={execution.notificationBroker.failedCount > 0 ? 'destructive' : execution.notificationBroker.pendingCount > 0 ? 'secondary' : 'default'}>
                  {execution.notificationBroker.dispatchCount} dispatch rows
                </Badge>
              </div>

              <div className="mt-3 grid gap-2 sm:grid-cols-4">
                <StatusBadge label="Routes" value={execution.notificationBroker.routeCount} />
                <StatusBadge label="Failed routes" value={failedRouteCount} />
                <StatusBadge label="Pending routes" value={pendingRouteCount} />
                <StatusBadge
                  label="Latest dispatch"
                  value={displayValue(formatTimestamp(execution.notificationBroker.latestUpdatedAt), 'Unknown')}
                />
              </div>

              {execution.notificationBroker.isTruncated ? (
                <p className="mt-3 text-[11px] leading-5 text-muted-foreground">
                  Showing the newest {execution.notificationBroker.dispatchCount} dispatch rows out of{' '}
                  {execution.notificationBroker.totalBeforeTruncation} total rows.
                </p>
              ) : null}

              {routeDiagnostics.length > 0 ? (
                <div className="mt-4 space-y-3">
                  {routeDiagnostics.map((route) => (
                    <div key={route.routeId} className="rounded-xl border border-border/70 bg-background/70 px-4 py-3">
                      <div className="flex flex-wrap items-center gap-2">
                        <p className="font-mono text-sm font-semibold text-foreground">{route.routeId}</p>
                        <Badge variant={getRouteDiagnosticBadgeVariant(route)}>{getRouteDiagnosticLabel(route)}</Badge>
                      </div>

                      <div className="mt-3 grid gap-2 text-[11px] text-muted-foreground sm:grid-cols-2 xl:grid-cols-4">
                        <p>Pending: <span className="font-mono text-foreground/75">{route.pendingCount}</span></p>
                        <p>Sent: <span className="font-mono text-foreground/75">{route.sentCount}</span></p>
                        <p>Failed: <span className="font-mono text-foreground/75">{route.failedCount}</span></p>
                        <p>Claimed: <span className="font-mono text-foreground/75">{route.claimedCount}</span></p>
                      </div>

                      {route.latestFailureCode && route.latestFailureMessage ? (
                        <div className="mt-3 rounded-md border border-destructive/20 bg-destructive/5 px-3 py-2 text-[11px] text-destructive">
                          <p className="font-mono">{route.latestFailureCode}</p>
                          <p className="mt-1 leading-5 text-destructive/90">{route.latestFailureMessage}</p>
                        </div>
                      ) : (
                        <p className="mt-3 text-[11px] text-muted-foreground">No failure diagnostics recorded for this route.</p>
                      )}
                    </div>
                  ))}
                </div>
              ) : (
                <p className="mt-3 text-[13px] leading-6 text-muted-foreground">
                  Cadence has not recorded any notification dispatch rows for this project yet, so channel health stays empty instead of fabricated.
                </p>
              )}
            </div>
          </div>
        ) : null}

        {activeTab === 'changes' ? (
          <div className="max-w-4xl p-5">
            <div className="mb-4 flex flex-wrap items-center gap-2">
              {execution.diffScopes.map((diffScope) => (
                <button
                  className={`rounded-md border px-3 py-1.5 text-[11px] font-medium transition-colors ${
                    activeDiffScope === diffScope.scope
                      ? 'border-foreground bg-secondary text-foreground'
                      : 'border-border bg-card/70 text-muted-foreground hover:text-foreground'
                  }`}
                  key={diffScope.scope}
                  onClick={() => onSelectDiffScope(diffScope.scope)}
                  type="button"
                >
                  {diffScope.label} · {diffScope.count}
                </button>
              ))}
            </div>

            <div className="mb-4 grid gap-3 sm:grid-cols-4">
              <StatusBadge label="Selected project" value={execution.project.id} />
              <StatusBadge label="Active diff" value={activeDiffScope} />
              <StatusBadge label="Status paths" value={execution.statusCount} />
              <StatusBadge label="Repository state" value={execution.hasChanges ? 'Dirty' : 'Clean'} />
            </div>

            {activeDiff.status === 'loading' ? (
              <div className="mb-4 flex items-center gap-2 rounded-md border border-border bg-secondary/30 px-3.5 py-2.5 text-[12px] text-muted-foreground">
                <Loader2 className="h-3.5 w-3.5 animate-spin" />
                <span>Loading {activeDiffScope} diff…</span>
              </div>
            ) : null}

            {activeDiff.status === 'error' ? (
              <div className="mb-4 rounded-md border border-destructive/20 bg-destructive/5 px-3.5 py-3 text-[12px] text-destructive">
                <div className="flex items-start gap-2">
                  <AlertCircle className="mt-0.5 h-4 w-4 shrink-0" />
                  <div className="flex-1">
                    <p className="font-medium">Failed to load the {activeDiffScope} diff.</p>
                    <p className="mt-1 leading-5">{activeDiff.errorMessage ?? 'Unknown diff load error.'}</p>
                  </div>
                  <button
                    className="rounded border border-destructive/30 px-2 py-1 text-[11px] font-medium text-destructive transition-colors hover:bg-destructive/10"
                    onClick={onRetryDiff}
                    type="button"
                  >
                    Retry
                  </button>
                </div>
              </div>
            ) : null}

            {activeDiff.status === 'ready' && activeDiff.diff?.isEmpty ? (
              <div className="rounded-lg border border-border bg-card p-5">
                <div className="flex items-center gap-2 text-foreground/80">
                  <Check className="h-4 w-4 text-success" />
                  <h3 className="text-[14px] font-semibold">No {activeDiffScope} diff available</h3>
                </div>
                <p className="mt-2 text-[13px] leading-6 text-muted-foreground">
                  The backend returned an empty patch for this scope, so the current repository truth is a clean or non-diffable state for {activeDiffScope} changes.
                </p>
              </div>
            ) : null}

            {activeDiff.diff && !activeDiff.diff.isEmpty ? (
              <div className="rounded-md border border-border overflow-hidden">
                <div className="flex items-center justify-between border-b border-border bg-secondary/30 px-3.5 py-2">
                  <span className="text-[11px] font-mono text-muted-foreground">
                    {activeDiffScope} diff · base {activeDiff.diff.baseRevisionLabel}
                  </span>
                  <div className="flex items-center gap-2 text-[10px] font-mono text-muted-foreground">
                    {activeDiff.diff.truncated ? (
                      <span className="rounded bg-secondary px-1.5 py-0.5">truncated</span>
                    ) : null}
                    <button
                      className="inline-flex items-center gap-1 rounded border border-border px-2 py-1 transition-colors hover:bg-secondary/50"
                      onClick={onRetryDiff}
                      type="button"
                    >
                      <RefreshCw className="h-3 w-3" />
                      Refresh
                    </button>
                  </div>
                </div>
                <div className="overflow-x-auto font-mono text-[11px] leading-5">
                  {diffLines.map((line, index) => (
                    <div
                      className={`flex px-3.5 ${line.kind === 'add' ? 'border-l-2 border-success/30 bg-success/5' : ''} ${
                        line.kind === 'del' ? 'border-l-2 border-destructive/30 bg-destructive/5' : ''
                      } ${line.kind === 'meta' ? 'border-l-2 border-border bg-secondary/20 text-foreground/60' : ''} ${
                        line.kind === 'context' ? 'border-l-2 border-transparent' : ''
                      }`}
                      key={`${line.kind}-${index}`}
                    >
                      <span
                        className={`w-4 shrink-0 select-none text-center ${line.kind === 'add' ? 'text-success' : ''} ${
                          line.kind === 'del' ? 'text-destructive' : ''
                        } ${line.kind === 'meta' ? 'text-muted-foreground' : ''}`}
                      >
                        {line.kind === 'add' ? '+' : line.kind === 'del' ? '-' : line.kind === 'meta' ? '›' : ' '}
                      </span>
                      <span
                        className={`flex-1 whitespace-pre-wrap ${line.kind === 'add' ? 'text-success/80' : ''} ${
                          line.kind === 'del' ? 'text-destructive/80' : ''
                        } ${line.kind === 'context' ? 'text-foreground/60' : ''}`}
                      >
                        {line.content || ' '}
                      </span>
                    </div>
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        ) : null}

        {activeTab === 'verify' ? (
          <div className="max-w-4xl space-y-4 p-5">
            <div className="rounded-lg border border-border bg-card p-4">
              <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">Verification availability</p>
              <div className="mt-2 flex flex-wrap items-center gap-2">
                <h3 className="text-[15px] font-semibold text-foreground">
                  {hasDurableVerificationState ? 'Repo-local operator verification truth' : 'No verification records yet'}
                </h3>
                <Badge variant={hasDurableVerificationState ? 'default' : 'outline'}>
                  {verificationRecords.length + resumeHistory.length} durable rows
                </Badge>
              </div>
              <p className="mt-2 text-[13px] leading-6 text-muted-foreground">{execution.verificationUnavailableReason}</p>
            </div>

            {execution.operatorActionError ? (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertTitle>Operator loop error remains visible</AlertTitle>
                <AlertDescription>
                  <p>{execution.operatorActionError.message}</p>
                  <p className="font-mono text-[11px] text-destructive/80">code: {execution.operatorActionError.code}</p>
                </AlertDescription>
              </Alert>
            ) : null}

            <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
              <StatusBadge label="Selected project" value={execution.project.id} />
              <StatusBadge label="Verification rows" value={verificationRecords.length} />
              <StatusBadge label="Resume rows" value={resumeHistory.length} />
              <StatusBadge
                label="Latest decision"
                value={execution.latestDecisionOutcome ? displayValue(execution.latestDecisionOutcome.statusLabel, execution.latestDecisionOutcome.status) : 'None'}
              />
            </div>

            {!hasDurableVerificationState ? (
              <VerifyEmptyState
                body="Cadence will keep this view empty until the selected project snapshot contains real verification or resume rows. No placeholder pass state is fabricated here."
                title="No durable verification or resume rows yet"
              />
            ) : (
              <div className="grid gap-4 xl:grid-cols-[minmax(0,1.4fr)_minmax(300px,1fr)]">
                <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">Verification records</p>
                      <p className="mt-1 text-sm text-muted-foreground">Durable rows written when approval decisions are persisted or verification outcomes are recorded.</p>
                    </div>
                    <Badge variant={verificationRecords.length > 0 ? 'default' : 'outline'}>{verificationRecords.length} rows</Badge>
                  </div>

                  <div className="mt-4 space-y-3">
                    {verificationRecords.length > 0 ? (
                      verificationRecords.map((record) => (
                        <div key={record.id} className="rounded-xl border border-border/70 bg-background/70 px-4 py-3">
                          <div className="flex flex-wrap items-center gap-2">
                            <p className="text-sm font-semibold text-foreground">{displayValue(record.summary, 'Verification record available.')}</p>
                            <Badge variant={getVerificationBadgeVariant(record.status)}>
                              {displayValue(record.statusLabel, record.status)}
                            </Badge>
                          </div>
                          <div className="mt-3 grid gap-2 text-[11px] text-muted-foreground sm:grid-cols-2">
                            <p>Action id: <span className="font-mono text-foreground/75">{displayValue(record.sourceActionId, 'Unknown')}</span></p>
                            <p>Recorded: <span className="text-foreground/75">{formatTimestamp(record.recordedAt)}</span></p>
                          </div>
                          {record.detail ? <p className="mt-3 text-sm leading-6 text-muted-foreground">{record.detail}</p> : null}
                        </div>
                      ))
                    ) : (
                      <VerifyEmptyState
                        body="Cadence has resume history for this project, but no separate durable verification rows were recorded yet."
                        title="No verification rows recorded"
                      />
                    )}
                  </div>
                </div>

                <div className="space-y-4">
                  <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">Latest operator decision</p>
                        <p className="mt-1 text-sm text-muted-foreground">Most recent durable approval outcome linked to this repo-local verification history.</p>
                      </div>
                      <Badge
                        variant={
                          execution.latestDecisionOutcome
                            ? execution.latestDecisionOutcome.status === 'approved'
                              ? 'default'
                              : 'destructive'
                            : 'outline'
                        }
                      >
                        {execution.latestDecisionOutcome ? displayValue(execution.latestDecisionOutcome.statusLabel, execution.latestDecisionOutcome.status) : 'None'}
                      </Badge>
                    </div>

                    {execution.latestDecisionOutcome ? (
                      <div className="mt-4 rounded-xl border border-border/70 bg-background/70 px-4 py-3">
                        <p className="text-sm font-semibold text-foreground">{displayValue(execution.latestDecisionOutcome.title, 'Operator decision')}</p>
                        <p className="mt-3 text-[11px] text-muted-foreground">
                          Action id: <span className="font-mono text-foreground/75">{displayValue(execution.latestDecisionOutcome.actionId, 'Unknown')}</span>
                        </p>
                        <p className="mt-1 text-[11px] text-muted-foreground">
                          Resolved: <span className="text-foreground/75">{formatTimestamp(execution.latestDecisionOutcome.resolvedAt)}</span>
                        </p>
                        {execution.latestDecisionOutcome.decisionNote ? (
                          <p className="mt-3 text-sm leading-6 text-muted-foreground">{execution.latestDecisionOutcome.decisionNote}</p>
                        ) : null}
                      </div>
                    ) : (
                      <VerifyEmptyState
                        body="Once an operator decision is resolved, Cadence keeps the latest durable outcome visible alongside verification history."
                        title="No resolved operator decision yet"
                      />
                    )}
                  </div>

                  <div className="rounded-xl border border-border/70 bg-card/70 p-4">
                    <div className="flex items-center justify-between gap-3">
                      <div>
                        <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">Resume history</p>
                        <p className="mt-1 text-sm text-muted-foreground">Durable restarts that were recorded after an approved operator action reopened the runtime loop.</p>
                      </div>
                      <Badge variant={resumeHistory.length > 0 ? 'default' : 'outline'}>{resumeHistory.length} rows</Badge>
                    </div>

                    <div className="mt-4 space-y-3">
                      {resumeHistory.length > 0 ? (
                        resumeHistory.map((entry) => (
                          <div key={entry.id} className="rounded-xl border border-border/70 bg-background/70 px-4 py-3">
                            <div className="flex flex-wrap items-center gap-2">
                              <p className="text-sm font-semibold text-foreground">{displayValue(entry.summary, 'Resume history recorded.')}</p>
                              <Badge variant={getResumeBadgeVariant(entry.status)}>{displayValue(entry.statusLabel, entry.status)}</Badge>
                            </div>
                            <div className="mt-3 grid gap-2 text-[11px] text-muted-foreground">
                              <p>Action id: <span className="font-mono text-foreground/75">{displayValue(entry.sourceActionId, 'Unknown')}</span></p>
                              <p>Session: <span className="font-mono text-foreground/75">{displayValue(entry.sessionId, 'Unknown')}</span></p>
                              <p>Recorded: <span className="text-foreground/75">{formatTimestamp(entry.createdAt)}</span></p>
                            </div>
                          </div>
                        ))
                      ) : (
                        <VerifyEmptyState
                          body="Verification rows exist for this project, but no durable resume entry has been recorded yet."
                          title="No resume history recorded"
                        />
                      )}
                    </div>
                  </div>
                </div>
              </div>
            )}
          </div>
        ) : null}
      </div>
    </div>
  )
}
