import { AlertCircle, LoaderCircle, Play, XCircle } from 'lucide-react'

import type {
  OperatorActionErrorView,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  RuntimeRunCheckpointView,
  RuntimeRunView,
} from '@/src/lib/cadence-model'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'

import {
  displayValue,
  formatSequence,
  formatTimestamp,
  getRuntimeRunBadgeVariant,
} from './helpers'

interface RecoveredRuntimeSectionProps {
  hasIncompleteRuntimeRunPayload: boolean
  renderableRuntimeRun: RuntimeRunView | null
  runtimeRunStatusText: string
  runtimeRunUnavailableReason: string
  runtimeRunCheckpoints: RuntimeRunCheckpointView[]
  runtimeRunActionError: OperatorActionErrorView | null
  runtimeRunActionErrorTitle: string
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  primaryRuntimeRunActionLabel: string
  canStartRuntimeRun: boolean
  canStopRuntimeRun: boolean
  onStartRuntimeRun?: () => void
  onStopRuntimeRun?: () => void
}

export function RecoveredRuntimeSection({
  hasIncompleteRuntimeRunPayload,
  renderableRuntimeRun,
  runtimeRunStatusText,
  runtimeRunUnavailableReason,
  runtimeRunCheckpoints,
  runtimeRunActionError,
  runtimeRunActionErrorTitle,
  runtimeRunActionStatus,
  pendingRuntimeRunAction,
  primaryRuntimeRunActionLabel,
  canStartRuntimeRun,
  canStopRuntimeRun,
  onStartRuntimeRun,
  onStopRuntimeRun,
}: RecoveredRuntimeSectionProps) {
  if (!hasIncompleteRuntimeRunPayload && !renderableRuntimeRun) {
    return null
  }

  return (
    <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
      <div className="flex flex-col gap-4">
        {hasIncompleteRuntimeRunPayload ? (
          <>
            <div className="flex flex-wrap items-center gap-2">
              <h2 className="text-lg font-semibold text-foreground">Durable run snapshot unavailable</h2>
              <Badge variant="destructive">Unavailable</Badge>
            </div>
            <p className="text-sm leading-6 text-muted-foreground">Durable run snapshot is incomplete</p>
          </>
        ) : renderableRuntimeRun ? (
          <>
            <div className="flex flex-wrap items-center justify-between gap-3">
              <div>
                <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Durable runtime</p>
                <h2 className="mt-2 text-lg font-semibold text-foreground">Recovered run snapshot</h2>
              </div>
              <div className="flex flex-wrap items-center gap-2">
                <Badge variant={getRuntimeRunBadgeVariant(renderableRuntimeRun)}>{runtimeRunStatusText}</Badge>
                <Badge variant="outline">{displayValue(renderableRuntimeRun.statusLabel, runtimeRunStatusText)}</Badge>
              </div>
            </div>

            <div className="rounded-xl border border-border/70 bg-card/70 p-4">
              <h3 className="text-base font-semibold text-foreground">
                {renderableRuntimeRun.isStale
                  ? 'Supervisor heartbeat is stale'
                  : renderableRuntimeRun.isTerminal
                    ? 'Supervisor stopped cleanly'
                    : 'Recovered run snapshot'}
              </h3>
              <p className="mt-2 text-sm leading-6 text-muted-foreground">{runtimeRunUnavailableReason}</p>
              <div className="mt-4 grid gap-3 sm:grid-cols-2">
                <CountCard label="Run ID" value={renderableRuntimeRun.runId} />
                <CountCard label="Checkpoint count" value={String(renderableRuntimeRun.checkpointCount)} />
              </div>
            </div>

            {runtimeRunActionError ? (
              <Alert variant="destructive">
                <AlertCircle className="h-4 w-4" />
                <AlertTitle>{runtimeRunActionErrorTitle}</AlertTitle>
                <AlertDescription>
                  <p>{runtimeRunActionError.message}</p>
                  {runtimeRunActionError.code ? (
                    <p className="font-mono text-[11px] text-destructive/80">code: {runtimeRunActionError.code}</p>
                  ) : null}
                </AlertDescription>
              </Alert>
            ) : null}

            <div className="flex flex-wrap gap-2">
              {canStartRuntimeRun ? (
                <Button disabled={runtimeRunActionStatus === 'running'} onClick={onStartRuntimeRun} type="button">
                  {runtimeRunActionStatus === 'running' && pendingRuntimeRunAction !== 'stop' ? (
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                  ) : (
                    <Play className="h-4 w-4" />
                  )}
                  {primaryRuntimeRunActionLabel}
                </Button>
              ) : null}

              {canStopRuntimeRun ? (
                <Button
                  disabled={runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'stop'}
                  onClick={onStopRuntimeRun}
                  type="button"
                  variant="outline"
                >
                  {runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'stop' ? (
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                  ) : (
                    <XCircle className="h-4 w-4" />
                  )}
                  Stop run
                </Button>
              ) : null}
            </div>

            <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-4">
              {runtimeRunCheckpoints.length > 0 ? (
                runtimeRunCheckpoints.map((checkpoint) => (
                  <div
                    key={`${checkpoint.kind}-${checkpoint.sequence}-${checkpoint.createdAt}`}
                    className="rounded-xl border border-border/70 bg-card/70 p-4"
                  >
                    <div className="flex flex-wrap items-center gap-2">
                      <Badge variant="outline">{formatSequence(checkpoint.sequence)}</Badge>
                      <Badge variant="outline">{checkpoint.kindLabel}</Badge>
                    </div>
                    <p className="mt-3 text-sm leading-6 text-foreground/90">
                      {displayValue(checkpoint.summary, 'Durable checkpoint recorded.')}
                    </p>
                    <p className="mt-2 text-[11px] text-muted-foreground">{formatTimestamp(checkpoint.createdAt)}</p>
                  </div>
                ))
              ) : (
                <FeedEmptyState
                  body="Cadence has not recorded a durable checkpoint for this run yet."
                  title="No checkpoints recorded"
                />
              )}
            </div>
          </>
        ) : null}
      </div>
    </section>
  )
}

function FeedEmptyState({
  title,
  body,
}: {
  title: string
  body: string
}) {
  return (
    <div className="rounded-xl border border-dashed border-border/70 bg-secondary/20 px-4 py-5 text-sm text-muted-foreground">
      <p className="font-medium text-foreground/85">{title}</p>
      <p className="mt-1 leading-6">{body}</p>
    </div>
  )
}

function CountCard({
  label,
  value,
}: {
  label: string
  value: string
}) {
  return (
    <div className="rounded-xl border border-border/70 bg-card/70 px-3 py-3">
      <p className="text-[10px] font-medium uppercase tracking-[0.16em] text-muted-foreground">{label}</p>
      <p className="mt-2 text-lg font-semibold text-foreground">{value}</p>
    </div>
  )
}
