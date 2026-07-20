'use client'

import { useCallback, useEffect, useMemo, useState } from 'react'
import {
  AlertCircle,
  Ban,
  CheckCircle2,
  ChevronDown,
  ChevronUp,
  Circle,
  CircleDashed,
  Expand,
  Loader2,
  MessageSquare,
  MinusCircle,
  PauseCircle,
  Square,
  Workflow as WorkflowIcon,
  X,
  XCircle,
} from 'lucide-react'
import type { LucideIcon } from 'lucide-react'

import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { cn } from '@/lib/utils'
import {
  buildWorkflowRunProgress,
  isTerminalWorkflowRunStatus,
  pickActiveWorkflowRun,
  type WorkflowRunNodeProgress,
} from '@/src/features/xero/workflow-run-selectors'
import type {
  WorkflowNodeRunStatusDto,
  WorkflowRunStatusDto,
} from '@/src/lib/xero-model/workflow-definition'
import type { WorkflowRunDto } from '@/src/lib/xero-model/workflow-run'

export interface WorkflowRunFloatingPanelProps {
  run: WorkflowRunDto
  actionRunning?: boolean
  onOpenCanvas?: (runId: string) => void
  onCancelRun?: (runId: string) => Promise<unknown> | void
  onResumeCheckpoint?: (
    runId: string,
    nodeRunId: string,
    decision: string,
    payload: unknown,
  ) => Promise<unknown> | void
  onOpenAgentSession?: (agentSessionId: string) => void
  onDismiss?: () => void
  className?: string
}

const RUN_STATUS_TONE: Record<WorkflowRunStatusDto, string> = {
  queued: 'border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300',
  running: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  paused: 'border-amber-500/35 bg-amber-500/10 text-amber-700 dark:text-amber-300',
  cancelling: 'border-muted-foreground/25 bg-muted text-muted-foreground',
  completed: 'border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300',
  failed: 'border-destructive/35 bg-destructive/10 text-destructive',
  cancelled: 'border-muted-foreground/25 bg-muted text-muted-foreground',
}

const NODE_STATUS_ICON: Record<
  WorkflowNodeRunStatusDto,
  { icon: LucideIcon; className: string; spin?: boolean }
> = {
  pending: { icon: Circle, className: 'text-muted-foreground/50' },
  eligible: { icon: CircleDashed, className: 'text-sky-500' },
  starting: { icon: Loader2, className: 'text-sky-500', spin: true },
  running: { icon: Loader2, className: 'text-emerald-500', spin: true },
  waiting_on_gate: { icon: PauseCircle, className: 'text-amber-500' },
  succeeded: { icon: CheckCircle2, className: 'text-emerald-500' },
  failed: { icon: XCircle, className: 'text-destructive' },
  stalled: { icon: AlertCircle, className: 'text-orange-500' },
  skipped: { icon: MinusCircle, className: 'text-muted-foreground/60' },
  cancelled: { icon: Ban, className: 'text-muted-foreground/60' },
}

function humanizeWorkflowToken(value: string): string {
  return value
    .replace(/[_-]+/g, ' ')
    .replace(/\b\w/g, (letter) => letter.toUpperCase())
}

function isNonEmptyRecord(value: unknown): value is Record<string, unknown> {
  return (
    typeof value === 'object' &&
    value !== null &&
    !Array.isArray(value) &&
    Object.keys(value as Record<string, unknown>).length > 0
  )
}

/** Chooses which run the chat surface shows. Follows the most recent active
 * run, keeps showing it after it settles, and stays hidden after the user
 * dismisses it until a different run becomes active. */
export function useChatWorkflowRunPanel(runs: readonly WorkflowRunDto[]): {
  run: WorkflowRunDto | null
  dismiss: () => void
} {
  const activeRun = useMemo(() => pickActiveWorkflowRun(runs), [runs])
  const activeRunId = activeRun?.id ?? null
  const [stickyRunId, setStickyRunId] = useState<string | null>(null)
  const [dismissedRunId, setDismissedRunId] = useState<string | null>(null)

  useEffect(() => {
    if (!activeRunId) return
    setStickyRunId((current) => (current === activeRunId ? current : activeRunId))
    setDismissedRunId((current) => (current === activeRunId ? current : null))
  }, [activeRunId])

  const run = useMemo(() => {
    if (!stickyRunId || stickyRunId === dismissedRunId) return null
    return runs.find((candidate) => candidate.id === stickyRunId) ?? null
  }, [dismissedRunId, runs, stickyRunId])

  const dismiss = useCallback(() => {
    setDismissedRunId(stickyRunId)
  }, [stickyRunId])

  return { run, dismiss }
}

export function WorkflowRunFloatingPanel({
  run,
  actionRunning = false,
  onOpenCanvas,
  onCancelRun,
  onResumeCheckpoint,
  onOpenAgentSession,
  onDismiss,
  className,
}: WorkflowRunFloatingPanelProps) {
  const [collapsed, setCollapsed] = useState(false)
  const [actionError, setActionError] = useState<string | null>(null)
  const progress = useMemo(() => buildWorkflowRunProgress(run), [run])
  const runTerminal = isTerminalWorkflowRunStatus(run.status)

  useEffect(() => {
    setActionError(null)
  }, [run.id])

  const runAction = useCallback(
    async (action: () => Promise<unknown> | void) => {
      setActionError(null)
      try {
        await action()
      } catch (error) {
        setActionError(error instanceof Error ? error.message : String(error))
      }
    },
    [],
  )

  const waitingEntry = progress.waitingEntry
  const waitingCheckpoint =
    waitingEntry?.node.type === 'human_checkpoint' ? waitingEntry.node : null
  const waitingDecisionOptions =
    waitingCheckpoint && waitingCheckpoint.decisionOptions.length > 0
      ? waitingCheckpoint.decisionOptions
      : ['continue']
  const waitingRequiresPayload = isNonEmptyRecord(
    waitingCheckpoint?.resumePayloadSchema ?? null,
  )

  return (
    <aside
      aria-label="Workflow run status"
      className={cn(
        'pointer-events-auto absolute right-4 top-4 z-30 flex w-[19.5rem] max-w-[calc(100%-2rem)] flex-col overflow-hidden rounded-xl border border-border/80 bg-card/95 text-[12px] shadow-xl backdrop-blur-md',
        className,
      )}
      onPointerDown={(event) => event.stopPropagation()}
    >
      <div className="flex items-center gap-2 px-3 py-2.5">
        <WorkflowIcon aria-hidden="true" className="size-3.5 shrink-0 text-foreground/65" />
        <span className="min-w-0 flex-1 truncate text-[12.5px] font-semibold text-foreground/85">
          {run.definitionSnapshot.name}
        </span>
        <Badge
          variant="outline"
          className={cn(
            'h-[18px] shrink-0 rounded px-1.5 py-0 text-[10px] font-semibold',
            RUN_STATUS_TONE[run.status],
          )}
        >
          {humanizeWorkflowToken(run.status)}
        </Badge>
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label={collapsed ? 'Expand workflow run panel' : 'Collapse workflow run panel'}
          aria-expanded={!collapsed}
          onClick={() => setCollapsed((current) => !current)}
          className="size-6 shrink-0 rounded-md text-foreground/60 hover:text-foreground"
        >
          {collapsed ? <ChevronDown className="size-3.5" /> : <ChevronUp className="size-3.5" />}
        </Button>
        {onDismiss ? (
          <Button
            type="button"
            size="icon-sm"
            variant="ghost"
            aria-label="Hide workflow run panel"
            onClick={onDismiss}
            className="size-6 shrink-0 rounded-md text-foreground/60 hover:text-foreground"
          >
            <X className="size-3.5" />
          </Button>
        ) : null}
      </div>

      <div className="flex items-center gap-2 border-t border-border/60 px-3 py-1.5 text-[11px] text-muted-foreground">
        <span className="shrink-0 font-medium tabular-nums">
          Step {Math.min(progress.settledCount + 1, Math.max(progress.totalCount, 1))} / {progress.totalCount}
        </span>
        {progress.activeEntry ? (
          <span className="min-w-0 truncate">{progress.activeEntry.node.title}</span>
        ) : runTerminal ? (
          <span className="min-w-0 truncate">
            {run.terminalStatus ? humanizeWorkflowToken(run.terminalStatus) : 'Finished'}
          </span>
        ) : null}
      </div>

      {!collapsed ? (
        <>
          <ul
            aria-label="Workflow steps"
            className="max-h-60 overflow-y-auto border-t border-border/60 px-1.5 py-1.5"
          >
            {progress.entries.map((entry) => (
              <WorkflowRunNodeRow
                key={entry.node.id}
                entry={entry}
                onOpenAgentSession={onOpenAgentSession}
              />
            ))}
          </ul>

          {waitingEntry?.runNode && onResumeCheckpoint && !runTerminal ? (
            <div className="space-y-2 border-t border-amber-500/25 bg-amber-500/5 px-3 py-2.5">
              <div className="flex items-start gap-2">
                <PauseCircle aria-hidden="true" className="mt-0.5 size-3.5 shrink-0 text-amber-500" />
                <p className="min-w-0 flex-1 text-muted-foreground">
                  {waitingCheckpoint?.prompt ?? 'Workflow is paused at a gate.'}
                </p>
              </div>
              <div className="flex flex-wrap justify-end gap-1.5">
                {waitingRequiresPayload ? (
                  onOpenCanvas ? (
                    <Button
                      type="button"
                      size="sm"
                      className="h-7 text-[11px]"
                      disabled={actionRunning}
                      onClick={() => onOpenCanvas(run.id)}
                    >
                      Resume on canvas
                    </Button>
                  ) : null
                ) : (
                  waitingDecisionOptions.map((option) => (
                    <Button
                      key={option}
                      type="button"
                      size="sm"
                      className="h-7 text-[11px]"
                      disabled={actionRunning}
                      onClick={() =>
                        runAction(() =>
                          onResumeCheckpoint(
                            run.id,
                            waitingEntry.runNode!.id,
                            option,
                            null,
                          ),
                        )
                      }
                    >
                      {actionRunning ? <Loader2 className="size-3 animate-spin" /> : null}
                      {humanizeWorkflowToken(option)}
                    </Button>
                  ))
                )}
              </div>
            </div>
          ) : null}

          {actionError ? (
            <p role="alert" className="border-t border-border/60 px-3 py-2 text-[11px] text-destructive">
              {actionError}
            </p>
          ) : null}

          {onOpenCanvas || (onCancelRun && !runTerminal) ? (
            <div className="flex items-center justify-end gap-1.5 border-t border-border/60 px-2 py-1.5">
              {onOpenCanvas ? (
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  onClick={() => onOpenCanvas(run.id)}
                  className="h-6 gap-1.5 rounded-md px-2 text-[11px] text-foreground/70 hover:text-foreground"
                >
                  <Expand className="size-3" />
                  Open canvas
                </Button>
              ) : null}
              {onCancelRun && !runTerminal ? (
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={actionRunning || run.status === 'cancelling'}
                  onClick={() => runAction(() => onCancelRun(run.id))}
                  className="h-6 gap-1.5 rounded-md px-2 text-[11px] text-foreground/70 hover:text-destructive"
                >
                  <Square className="size-3" />
                  Cancel run
                </Button>
              ) : null}
            </div>
          ) : null}
        </>
      ) : null}
    </aside>
  )
}

function WorkflowRunNodeRow({
  entry,
  onOpenAgentSession,
}: {
  entry: WorkflowRunNodeProgress
  onOpenAgentSession?: (agentSessionId: string) => void
}) {
  const statusIcon = NODE_STATUS_ICON[entry.status]
  const StatusIcon = statusIcon.icon
  const agentSessionId = entry.runNode?.agentSessionId ?? null
  const isActive =
    entry.status === 'running' ||
    entry.status === 'starting' ||
    entry.status === 'waiting_on_gate'

  return (
    <li
      className={cn(
        'group flex h-7 items-center gap-2 rounded-md px-1.5',
        isActive && 'bg-muted/50',
      )}
    >
      <StatusIcon
        aria-hidden="true"
        className={cn(
          'size-3.5 shrink-0',
          statusIcon.className,
          statusIcon.spin && 'animate-spin',
        )}
      />
      <span
        className={cn(
          'min-w-0 flex-1 truncate text-[11.5px]',
          isActive ? 'font-medium text-foreground' : 'text-foreground/75',
        )}
        title={entry.node.title}
      >
        {entry.node.title}
      </span>
      <span className="sr-only">{humanizeWorkflowToken(entry.status)}</span>
      {agentSessionId && onOpenAgentSession ? (
        <Button
          type="button"
          size="icon-sm"
          variant="ghost"
          aria-label={`Open chat for ${entry.node.title}`}
          onClick={() => onOpenAgentSession(agentSessionId)}
          className={cn(
            'size-6 shrink-0 rounded-md text-foreground/50 hover:text-primary',
            !isActive && 'opacity-0 focus-visible:opacity-100 group-hover:opacity-100',
          )}
        >
          <MessageSquare className="size-3" />
        </Button>
      ) : null}
    </li>
  )
}
