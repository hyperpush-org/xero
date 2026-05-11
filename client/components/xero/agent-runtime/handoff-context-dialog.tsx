import {
  AlertTriangle,
  ArrowRight,
  Check,
  ClipboardList,
  GitMerge,
  History,
  Loader2,
  RefreshCcw,
  ShieldAlert,
  Target,
  X,
} from 'lucide-react'

import { cn } from '@/lib/utils'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import type {
  AgentHandoffContextSummaryDto,
  AgentHandoffOmittedContextDto,
  AgentHandoffSafetyRationaleDto,
} from '@/src/lib/xero-model/agent-reports'

export type HandoffContextDialogStatus = 'idle' | 'loading' | 'ready' | 'error'

interface HandoffContextDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  status: HandoffContextDialogStatus
  errorMessage: string | null
  summary: AgentHandoffContextSummaryDto | null
  onRefresh: () => void
}

export function HandoffContextDialog({
  open,
  onOpenChange,
  status,
  errorMessage,
  summary,
  onRefresh,
}: HandoffContextDialogProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[85vh] w-full max-w-3xl overflow-hidden p-0 sm:max-w-3xl"
        aria-label="Agent handoff context summary"
        showCloseButton={false}
      >
        <DialogHeader className="border-b border-border/40 px-5 py-3.5">
          <div className="flex items-start justify-between gap-3">
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-2">
                <GitMerge className="h-4 w-4 text-foreground/80" aria-hidden="true" />
                <DialogTitle className="text-[14px]">
                  What carried over in this handoff
                </DialogTitle>
              </div>
              <DialogDescription className="text-[12px] text-muted-foreground">
                Lineage, definition pin, carried context, and what was redacted or omitted when
                Xero handed this conversation off to a fresh same-type run.
              </DialogDescription>
            </div>
            <div className="flex shrink-0 items-center gap-1.5">
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="h-7 gap-1.5 px-2 text-[12px]"
                onClick={onRefresh}
                disabled={status === 'loading'}
                aria-label="Refresh handoff context summary"
              >
                <RefreshCcw
                  className={cn('h-3.5 w-3.5', status === 'loading' && 'animate-spin')}
                  aria-hidden="true"
                />
                Refresh
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="icon"
                className="h-7 w-7"
                onClick={() => onOpenChange(false)}
                aria-label="Close handoff context summary"
              >
                <X className="h-3.5 w-3.5" aria-hidden="true" />
              </Button>
            </div>
          </div>
        </DialogHeader>

        <div className="max-h-[calc(85vh-9rem)] overflow-y-auto px-5 py-4">
          {status === 'loading' && !summary ? (
            <LoadingState />
          ) : status === 'error' ? (
            <ErrorState message={errorMessage} />
          ) : !summary ? (
            <IdleState />
          ) : (
            <SummaryBody summary={summary} />
          )}
        </div>

        <DialogFooter className="border-t border-border/40 bg-muted/20 px-5 py-2.5">
          <p className="text-[11px] text-muted-foreground">
            Raw bundle payloads are never shown here — only redaction-safe previews. Source-cited
            text is preserved; secret-shaped values are hidden.
          </p>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}

function LoadingState() {
  return (
    <div
      className="flex items-center gap-2 text-[12px] text-muted-foreground"
      role="status"
      aria-live="polite"
    >
      <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />
      Loading handoff context summary…
    </div>
  )
}

function ErrorState({ message }: { message: string | null }) {
  return (
    <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.04] px-3 py-2.5 text-[12px] text-destructive">
      <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      <p>{message ?? 'Xero could not load the handoff context summary.'}</p>
    </div>
  )
}

function IdleState() {
  return (
    <p className="text-[12px] text-muted-foreground">
      No handoff context summary is loaded yet.
    </p>
  )
}

function SummaryBody({ summary }: { summary: AgentHandoffContextSummaryDto }) {
  const {
    source,
    target,
    provider,
    status,
    handoffId,
    carriedContext,
    omittedContext,
    redaction,
    safetyRationale,
    createdAt,
    completedAt,
  } = summary

  const continuingPathway = target.runId
    ? `${source.runId}  →  ${target.runId}`
    : `${source.runId}  →  (target run pending)`

  return (
    <div className="flex flex-col gap-4">
      <LineageStrip
        handoffId={handoffId}
        status={status}
        pathway={continuingPathway}
        providerLabel={`${provider.providerId} · ${provider.modelId}`}
        createdAt={createdAt}
        completedAt={completedAt}
      />

      <DefinitionPinStrip source={source} target={target} />

      <RedactionStrip redaction={redaction} />

      <CarriedContextSection
        userGoal={carriedContext.userGoal}
        currentTask={carriedContext.currentTask}
        currentStatus={carriedContext.currentStatus}
        constraints={carriedContext.constraints}
      />

      <WorkingSetSection
        activeTodoCount={carriedContext.workingSetSummary.activeTodoCount}
        recentFileChangeCount={carriedContext.workingSetSummary.recentFileChangeCount}
        latestChangedPaths={carriedContext.workingSetSummary.latestChangedPaths}
        completedWork={carriedContext.completedWork.length}
        pendingWork={carriedContext.pendingWork.length}
        activeTodoItems={carriedContext.activeTodoItems.length}
        importantDecisions={carriedContext.importantDecisions.length}
        sourceCitedContinuityRecords={carriedContext.sourceCitedContinuityRecords.length}
        recentFileChanges={carriedContext.recentFileChanges.length}
        toolAndCommandEvidence={carriedContext.toolAndCommandEvidence.length}
        approvedMemories={carriedContext.approvedMemories.length}
        relevantProjectRecords={carriedContext.relevantProjectRecords.length}
      />

      <OmittedContextSection omitted={omittedContext} />

      <SafetyRationaleSection rationale={safetyRationale} />
    </div>
  )
}

interface LineageStripProps {
  handoffId: string
  status: string
  pathway: string
  providerLabel: string
  createdAt: string
  completedAt: string | null | undefined
}

function LineageStrip({
  handoffId,
  status,
  pathway,
  providerLabel,
  createdAt,
  completedAt,
}: LineageStripProps) {
  const fields: { label: string; value: string }[] = [
    { label: 'Handoff', value: handoffId },
    { label: 'Status', value: status },
    { label: 'Runs', value: pathway },
    { label: 'Provider', value: providerLabel },
    { label: 'Created', value: createdAt },
  ]
  if (completedAt) {
    fields.push({ label: 'Completed', value: completedAt })
  }
  return (
    <ul
      aria-label="Handoff lineage"
      className="flex flex-wrap items-center gap-1.5 rounded-md border border-border/40 bg-muted/20 px-2.5 py-2 text-[11px]"
    >
      {fields.map((field) => (
        <li
          key={field.label}
          className="flex items-center gap-1.5 rounded-sm border border-border/40 bg-background/60 px-2 py-0.5"
        >
          <span className="font-mono text-[10px] uppercase tracking-wide text-muted-foreground">
            {field.label}
          </span>
          <span className="max-w-[36ch] truncate font-mono text-[11px] text-foreground/85">
            {field.value}
          </span>
        </li>
      ))}
    </ul>
  )
}

interface DefinitionPinStripProps {
  source: AgentHandoffContextSummaryDto['source']
  target: AgentHandoffContextSummaryDto['target']
}

function DefinitionPinStrip({ source, target }: DefinitionPinStripProps) {
  const samePin =
    source.agentDefinitionId === target.agentDefinitionId &&
    source.agentDefinitionVersion === target.agentDefinitionVersion &&
    source.runtimeAgentId === target.runtimeAgentId
  return (
    <section
      aria-label="Definition pin"
      className="rounded-md border border-border/40 bg-background/30"
    >
      <header className="flex items-center justify-between border-b border-border/40 px-3 py-1.5">
        <div className="flex items-center gap-1.5 text-[12px] text-foreground/80">
          <History className="h-3.5 w-3.5" aria-hidden="true" />
          <span className="font-medium">Definition pin</span>
        </div>
        <Badge variant="outline" className="h-5 px-1.5 text-[10.5px] font-normal">
          {samePin ? 'pinned' : 'changed'}
        </Badge>
      </header>
      <div className="grid gap-2 px-3 py-2.5 sm:grid-cols-2">
        <PinColumn
          title="Source"
          icon={<ArrowRight className="h-3.5 w-3.5 text-muted-foreground/80" aria-hidden="true" />}
          runtimeAgentId={source.runtimeAgentId}
          agentDefinitionId={source.agentDefinitionId}
          agentDefinitionVersion={source.agentDefinitionVersion}
          runId={source.runId}
          sessionId={source.agentSessionId}
          contextHash={source.contextHash}
        />
        <PinColumn
          title="Target"
          icon={<Target className="h-3.5 w-3.5 text-muted-foreground/80" aria-hidden="true" />}
          runtimeAgentId={target.runtimeAgentId}
          agentDefinitionId={target.agentDefinitionId}
          agentDefinitionVersion={target.agentDefinitionVersion}
          runId={target.runId ?? null}
          sessionId={target.agentSessionId ?? null}
          contextHash={null}
        />
      </div>
    </section>
  )
}

interface PinColumnProps {
  title: string
  icon: React.ReactNode
  runtimeAgentId: string
  agentDefinitionId: string
  agentDefinitionVersion: number
  runId: string | null
  sessionId: string | null
  contextHash: string | null
}

function PinColumn({
  title,
  icon,
  runtimeAgentId,
  agentDefinitionId,
  agentDefinitionVersion,
  runId,
  sessionId,
  contextHash,
}: PinColumnProps) {
  const rows: { label: string; value: string }[] = [
    { label: 'Runtime', value: runtimeAgentId },
    { label: 'Definition', value: `${agentDefinitionId} v${agentDefinitionVersion}` },
  ]
  if (runId) rows.push({ label: 'Run', value: runId })
  if (sessionId) rows.push({ label: 'Session', value: sessionId })
  if (contextHash) rows.push({ label: 'Context hash', value: contextHash })
  return (
    <div className="rounded-md border border-border/40 bg-background/60 px-3 py-2">
      <div className="mb-1 flex items-center gap-1.5 text-[11.5px] text-muted-foreground">
        {icon}
        <span className="font-medium text-foreground/80">{title}</span>
      </div>
      <dl className="flex flex-col gap-0.5">
        {rows.map((row) => (
          <div key={row.label} className="flex items-baseline gap-1.5 text-[11px]">
            <dt className="font-mono uppercase tracking-wide text-muted-foreground">
              {row.label}
            </dt>
            <dd className="min-w-0 flex-1 truncate font-mono text-foreground/85">{row.value}</dd>
          </div>
        ))}
      </dl>
    </div>
  )
}

interface RedactionStripProps {
  redaction: AgentHandoffContextSummaryDto['redaction']
}

function RedactionStrip({ redaction }: RedactionStripProps) {
  const notes: string[] = []
  notes.push(`State: ${redaction.state}.`)
  if (redaction.bundleRedactionCount !== null && redaction.bundleRedactionCount > 0) {
    notes.push(`${redaction.bundleRedactionCount} bundle field(s) redacted.`)
  }
  if (redaction.summaryRedactionApplied) {
    notes.push('Summary redaction applied.')
  }
  notes.push('Raw bundle payload hidden.')
  return (
    <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/[0.05] px-3 py-2 text-[11.5px]">
      <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning" aria-hidden="true" />
      <p className="text-foreground/80">{notes.join(' ')}</p>
    </div>
  )
}

interface CarriedContextSectionProps {
  userGoal: string | null
  currentTask: string | null
  currentStatus: string | null
  constraints: string[]
}

function CarriedContextSection({
  userGoal,
  currentTask,
  currentStatus,
  constraints,
}: CarriedContextSectionProps) {
  return (
    <section
      aria-label="Carried context"
      className="rounded-md border border-border/40 bg-background/30"
    >
      <header className="flex items-center justify-between border-b border-border/40 px-3 py-1.5">
        <div className="flex items-center gap-1.5 text-[12px] text-foreground/80">
          <ClipboardList className="h-3.5 w-3.5" aria-hidden="true" />
          <span className="font-medium">Carried context</span>
        </div>
      </header>
      <div className="flex flex-col gap-2 px-3 py-2.5">
        <CarriedField label="User goal" value={userGoal} />
        <CarriedField label="Current task" value={currentTask} />
        <CarriedField label="Current status" value={currentStatus} />
        {constraints.length > 0 ? (
          <div className="flex flex-col gap-0.5">
            <span className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
              Constraints
            </span>
            <ul className="flex flex-col gap-0.5">
              {constraints.map((constraint, index) => (
                <li key={`${index}:${constraint}`} className="text-[11.5px] text-foreground/80">
                  · {constraint}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </section>
  )
}

function CarriedField({ label, value }: { label: string; value: string | null }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      {value ? (
        <p className="text-[12px] text-foreground/85">{value}</p>
      ) : (
        <p className="text-[11.5px] italic text-muted-foreground">Not recorded.</p>
      )}
    </div>
  )
}

interface WorkingSetSectionProps {
  activeTodoCount: number
  recentFileChangeCount: number
  latestChangedPaths: string[]
  completedWork: number
  pendingWork: number
  activeTodoItems: number
  importantDecisions: number
  sourceCitedContinuityRecords: number
  recentFileChanges: number
  toolAndCommandEvidence: number
  approvedMemories: number
  relevantProjectRecords: number
}

function WorkingSetSection({
  activeTodoCount,
  recentFileChangeCount,
  latestChangedPaths,
  completedWork,
  pendingWork,
  activeTodoItems,
  importantDecisions,
  sourceCitedContinuityRecords,
  recentFileChanges,
  toolAndCommandEvidence,
  approvedMemories,
  relevantProjectRecords,
}: WorkingSetSectionProps) {
  const counts: { label: string; value: number }[] = [
    { label: 'Completed work', value: completedWork },
    { label: 'Pending work', value: pendingWork },
    { label: 'Active todos', value: activeTodoItems },
    { label: 'Important decisions', value: importantDecisions },
    { label: 'Continuity records', value: sourceCitedContinuityRecords },
    { label: 'Recent file changes', value: recentFileChanges },
    { label: 'Tool evidence', value: toolAndCommandEvidence },
    { label: 'Approved memories', value: approvedMemories },
    { label: 'Project records', value: relevantProjectRecords },
  ]
  return (
    <section
      aria-label="Working set"
      className="rounded-md border border-border/40 bg-background/30"
    >
      <header className="flex items-center justify-between border-b border-border/40 px-3 py-1.5">
        <div className="flex items-center gap-1.5 text-[12px] text-foreground/80">
          <ClipboardList className="h-3.5 w-3.5" aria-hidden="true" />
          <span className="font-medium">Working set</span>
        </div>
        <span className="font-mono text-[10.5px] text-muted-foreground">
          {activeTodoCount} todo{activeTodoCount === 1 ? '' : 's'} ·{' '}
          {recentFileChangeCount} change{recentFileChangeCount === 1 ? '' : 's'}
        </span>
      </header>
      <div className="flex flex-col gap-2 px-3 py-2.5">
        <ul className="grid grid-cols-2 gap-1.5 sm:grid-cols-3">
          {counts.map((entry) => (
            <li
              key={entry.label}
              className="flex items-center justify-between rounded-sm border border-border/40 bg-background/60 px-2 py-1"
            >
              <span className="text-[11px] text-muted-foreground">{entry.label}</span>
              <span className="font-mono text-[11.5px] text-foreground/85">{entry.value}</span>
            </li>
          ))}
        </ul>
        {latestChangedPaths.length > 0 ? (
          <div className="flex flex-col gap-0.5">
            <span className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
              Latest changed paths
            </span>
            <ul className="flex flex-col gap-0.5">
              {latestChangedPaths.map((path) => (
                <li
                  key={path}
                  className="truncate font-mono text-[11px] text-foreground/80"
                  title={path}
                >
                  {path}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </section>
  )
}

function OmittedContextSection({
  omitted,
}: {
  omitted: AgentHandoffOmittedContextDto[]
}) {
  return (
    <section
      aria-label="Omitted context"
      className="rounded-md border border-border/40 bg-background/30"
    >
      <header className="flex items-center justify-between border-b border-border/40 px-3 py-1.5">
        <div className="flex items-center gap-1.5 text-[12px] text-foreground/80">
          <ShieldAlert className="h-3.5 w-3.5" aria-hidden="true" />
          <span className="font-medium">What was omitted</span>
        </div>
        <span className="font-mono text-[10.5px] text-muted-foreground">{omitted.length}</span>
      </header>
      <div className="px-3 py-2.5">
        {omitted.length === 0 ? (
          <p className="text-[11.5px] text-muted-foreground">
            Nothing was omitted from the handoff bundle.
          </p>
        ) : (
          <ul className="flex flex-col gap-1.5">
            {omitted.map((entry, index) => (
              <li
                key={`${entry.kind}:${index}`}
                className="rounded-sm border border-border/40 bg-background/60 px-2.5 py-1.5"
              >
                <div className="flex items-center justify-between gap-2">
                  <span className="font-mono text-[11px] uppercase tracking-wide text-foreground/80">
                    {entry.kind}
                  </span>
                  <Badge variant="outline" className="h-5 px-1.5 text-[10.5px] font-normal">
                    {entry.status}
                  </Badge>
                </div>
                <p className="mt-1 text-[11.5px] text-foreground/80">{entry.reason}</p>
                {typeof entry.referenceCount === 'number' ? (
                  <p className="mt-0.5 font-mono text-[10.5px] text-muted-foreground">
                    {entry.referenceCount} reference{entry.referenceCount === 1 ? '' : 's'}
                  </p>
                ) : null}
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  )
}

function SafetyRationaleSection({
  rationale,
}: {
  rationale: AgentHandoffSafetyRationaleDto
}) {
  const indicators: { label: string; value: boolean }[] = [
    { label: 'Same runtime agent', value: rationale.sameRuntimeAgent },
    { label: 'Same definition version', value: rationale.sameDefinitionVersion },
    { label: 'Source context hash present', value: rationale.sourceContextHashPresent },
    { label: 'Target run created', value: rationale.targetRunCreated },
    { label: 'Handoff record persisted', value: rationale.handoffRecordPersisted },
  ]
  return (
    <section
      aria-label="Safety rationale"
      className="rounded-md border border-border/40 bg-background/30"
    >
      <header className="flex items-center justify-between border-b border-border/40 px-3 py-1.5">
        <div className="flex items-center gap-1.5 text-[12px] text-foreground/80">
          <Check className="h-3.5 w-3.5" aria-hidden="true" />
          <span className="font-medium">Safety rationale</span>
        </div>
      </header>
      <div className="flex flex-col gap-2 px-3 py-2.5">
        <ul className="grid grid-cols-1 gap-1 sm:grid-cols-2">
          {indicators.map((indicator) => (
            <li
              key={indicator.label}
              className="flex items-center justify-between rounded-sm border border-border/40 bg-background/60 px-2 py-1 text-[11px]"
            >
              <span className="text-muted-foreground">{indicator.label}</span>
              <Badge
                variant="outline"
                className={cn(
                  'h-5 px-1.5 text-[10.5px] font-normal',
                  indicator.value
                    ? 'border-success/40 text-success'
                    : 'border-warning/40 text-warning',
                )}
              >
                {indicator.value ? 'yes' : 'no'}
              </Badge>
            </li>
          ))}
        </ul>
        {rationale.reasons.length > 0 ? (
          <div className="flex flex-col gap-0.5">
            <span className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
              Why it was safe to hand off
            </span>
            <ul className="flex flex-col gap-0.5">
              {rationale.reasons.map((reason, index) => (
                <li key={`${index}:${reason}`} className="text-[11.5px] text-foreground/80">
                  · {reason}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </div>
    </section>
  )
}
