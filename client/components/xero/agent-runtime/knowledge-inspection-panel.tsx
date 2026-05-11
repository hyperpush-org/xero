import { useMemo } from 'react'
import {
  AlertTriangle,
  Brain,
  Database,
  Eye,
  GitMerge,
  Info,
  Loader2,
  RefreshCcw,
  ShieldAlert,
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
  AgentKnowledgeApprovedMemoryDto,
  AgentKnowledgeHandoffRecordDto,
  AgentKnowledgeInspectionDto,
  AgentKnowledgeProjectRecordDto,
} from '@/src/lib/xero-model/agent-reports'

export type KnowledgeInspectionStatus = 'idle' | 'loading' | 'ready' | 'error'

interface KnowledgeInspectionPanelProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  status: KnowledgeInspectionStatus
  errorMessage: string | null
  inspection: AgentKnowledgeInspectionDto | null
  onRefresh: () => void
}

export function KnowledgeInspectionPanel({
  open,
  onOpenChange,
  status,
  errorMessage,
  inspection,
  onRefresh,
}: KnowledgeInspectionPanelProps) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent
        className="max-h-[85vh] w-full max-w-3xl overflow-hidden p-0 sm:max-w-3xl"
        aria-label="Agent knowledge inspection"
        showCloseButton={false}
      >
        <DialogHeader className="border-b border-border/40 px-5 py-3.5">
          <div className="flex items-start justify-between gap-3">
            <div className="flex flex-col gap-1">
              <div className="flex items-center gap-2">
                <Brain className="h-4 w-4 text-foreground/80" aria-hidden="true" />
                <DialogTitle className="text-[14px]">
                  What this agent can see right now
                </DialogTitle>
              </div>
              <DialogDescription className="text-[12px] text-muted-foreground">
                Project records, approved memory, handoff records, and continuity entries the
                next turn could retrieve under the current retrieval policy.
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
                aria-label="Refresh knowledge inspection"
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
                aria-label="Close knowledge inspection"
              >
                <X className="h-3.5 w-3.5" aria-hidden="true" />
              </Button>
            </div>
          </div>
        </DialogHeader>

        <div className="max-h-[calc(85vh-9rem)] overflow-y-auto px-5 py-4">
          {status === 'loading' && !inspection ? (
            <LoadingState />
          ) : status === 'error' ? (
            <ErrorState message={errorMessage} />
          ) : !inspection ? (
            <IdleState />
          ) : (
            <InspectionBody inspection={inspection} />
          )}
        </div>

        <DialogFooter className="border-t border-border/40 bg-muted/20 px-5 py-2.5">
          <p className="text-[11px] text-muted-foreground">
            Limits and redactions are applied by the retrieval policy before content is shown
            here. Raw blocked records are excluded; redacted record text is hidden.
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
      Inspecting the agent&apos;s current knowledge…
    </div>
  )
}

function ErrorState({ message }: { message: string | null }) {
  return (
    <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/[0.04] px-3 py-2.5 text-[12px] text-destructive">
      <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
      <p>{message ?? 'Xero could not load the knowledge inspection.'}</p>
    </div>
  )
}

function IdleState() {
  return (
    <p className="text-[12px] text-muted-foreground">
      Start an agent run to see what records, memories, and handoff context the next turn
      could retrieve.
    </p>
  )
}

interface InspectionBodyProps {
  inspection: AgentKnowledgeInspectionDto
}

function InspectionBody({ inspection }: InspectionBodyProps) {
  const {
    projectRecords,
    continuityRecords,
    approvedMemory,
    handoffRecords,
    retrievalPolicy,
    redaction,
    limit,
    runId,
    agentSessionId,
  } = inspection

  return (
    <div className="flex flex-col gap-4">
      <ScopeStrip
        runId={runId}
        agentSessionId={agentSessionId}
        limit={limit}
        policySource={retrievalPolicy.source}
      />

      <RetrievalPolicyStrip
        recordKindFilter={retrievalPolicy.recordKindFilter}
        memoryKindFilter={retrievalPolicy.memoryKindFilter}
        filtersApplied={retrievalPolicy.filtersApplied}
      />

      {(redaction.rawBlockedRecordsExcluded ||
        redaction.redactedProjectRecordTextHidden ||
        redaction.handoffBundleRawPayloadHidden) && (
        <RedactionStrip redaction={redaction} />
      )}

      <ProjectRecordsSection
        title="Project records"
        icon={<Database className="h-3.5 w-3.5" aria-hidden="true" />}
        records={projectRecords}
        limit={limit}
        emptyHint="No project records would be retrieved on the next turn."
      />

      <ProjectRecordsSection
        title="Continuity records"
        icon={<GitMerge className="h-3.5 w-3.5" aria-hidden="true" />}
        records={continuityRecords}
        limit={limit}
        emptyHint="No continuity records are visible to this run."
      />

      <ApprovedMemorySection memories={approvedMemory} limit={limit} />

      <HandoffRecordsSection handoffRecords={handoffRecords} limit={limit} />
    </div>
  )
}

interface ScopeStripProps {
  runId: string | null | undefined
  agentSessionId: string | null | undefined
  limit: number
  policySource: 'runtime_audit_export' | 'not_requested'
}

function ScopeStrip({ runId, agentSessionId, limit, policySource }: ScopeStripProps) {
  const fields: { label: string; value: string }[] = []
  if (runId) fields.push({ label: 'Run', value: runId })
  if (agentSessionId) fields.push({ label: 'Session', value: agentSessionId })
  fields.push({ label: 'Limit', value: `${limit} per section` })
  fields.push({
    label: 'Retrieval policy',
    value: policySource === 'runtime_audit_export' ? 'From last run audit' : 'Not yet recorded',
  })

  return (
    <ul
      aria-label="Inspection scope"
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
          <span className="max-w-[16ch] truncate font-mono text-[11px] text-foreground/85">
            {field.value}
          </span>
        </li>
      ))}
    </ul>
  )
}

interface RetrievalPolicyStripProps {
  recordKindFilter: string[]
  memoryKindFilter: string[]
  filtersApplied: boolean
}

function RetrievalPolicyStrip({
  recordKindFilter,
  memoryKindFilter,
  filtersApplied,
}: RetrievalPolicyStripProps) {
  if (!filtersApplied) {
    return (
      <div className="flex items-start gap-2 rounded-md border border-border/40 bg-background/40 px-3 py-2 text-[11.5px] text-muted-foreground">
        <Info className="mt-0.5 h-3.5 w-3.5 shrink-0" aria-hidden="true" />
        <p>
          No record or memory kind filters are pinned for this run, so retrieval considers every
          kind below.
        </p>
      </div>
    )
  }

  return (
    <div className="rounded-md border border-border/40 bg-background/40 px-3 py-2 text-[11.5px]">
      <div className="mb-1 flex items-center gap-1.5 text-muted-foreground">
        <Eye className="h-3.5 w-3.5" aria-hidden="true" />
        <span className="font-medium text-foreground/80">Retrieval policy filters</span>
      </div>
      <div className="grid gap-1.5 sm:grid-cols-2">
        <FilterList label="Record kinds" values={recordKindFilter} />
        <FilterList label="Memory kinds" values={memoryKindFilter} />
      </div>
    </div>
  )
}

function FilterList({ label, values }: { label: string; values: string[] }) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="font-mono text-[10px] uppercase tracking-wide text-muted-foreground">
        {label}
      </span>
      {values.length === 0 ? (
        <span className="text-[11px] text-muted-foreground">No filter</span>
      ) : (
        <div className="flex flex-wrap gap-1">
          {values.map((value) => (
            <Badge key={value} variant="outline" className="h-5 px-1.5 text-[10.5px] font-normal">
              {value}
            </Badge>
          ))}
        </div>
      )}
    </div>
  )
}

interface RedactionStripProps {
  redaction: AgentKnowledgeInspectionDto['redaction']
}

function RedactionStrip({ redaction }: RedactionStripProps) {
  const notes: string[] = []
  if (redaction.rawBlockedRecordsExcluded) {
    notes.push('Blocked records excluded.')
  }
  if (redaction.redactedProjectRecordTextHidden) {
    notes.push('Redacted record text hidden.')
  }
  if (redaction.handoffBundleRawPayloadHidden) {
    notes.push('Handoff bundle payloads hidden.')
  }
  return (
    <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/[0.05] px-3 py-2 text-[11.5px]">
      <ShieldAlert className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning" aria-hidden="true" />
      <p className="text-foreground/80">{notes.join(' ')}</p>
    </div>
  )
}

interface ProjectRecordsSectionProps {
  title: string
  icon: React.ReactNode
  records: AgentKnowledgeProjectRecordDto[]
  limit: number
  emptyHint: string
}

function ProjectRecordsSection({
  title,
  icon,
  records,
  limit,
  emptyHint,
}: ProjectRecordsSectionProps) {
  return (
    <SectionShell
      title={title}
      icon={icon}
      count={records.length}
      limit={limit}
      aria-label={title}
    >
      {records.length === 0 ? (
        <EmptyHint message={emptyHint} />
      ) : (
        <ul className="flex flex-col gap-2">
          {records.map((record) => (
            <li
              key={record.recordId}
              className="rounded-md border border-border/40 bg-background/40 px-3 py-2"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12.5px] font-medium text-foreground">
                    {record.title}
                  </p>
                  <p className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
                    {record.recordKind}
                    {record.schemaName ? ` · ${record.schemaName}` : null}
                  </p>
                </div>
                <RedactionBadge state={record.redactionState} />
              </div>
              {record.textPreview ? (
                <p className="mt-1 line-clamp-3 text-[11.5px] text-foreground/80">
                  {record.textPreview}
                </p>
              ) : record.redactionState === 'redacted' ? (
                <p className="mt-1 text-[11.5px] italic text-muted-foreground">
                  Text hidden — record is redacted.
                </p>
              ) : record.summary ? (
                <p className="mt-1 line-clamp-3 text-[11.5px] text-foreground/80">
                  {record.summary}
                </p>
              ) : null}
              <MetaRow
                items={[
                  record.importance ? { label: 'Importance', value: record.importance } : null,
                  typeof record.confidence === 'number'
                    ? { label: 'Confidence', value: record.confidence.toFixed(2) }
                    : null,
                  record.freshnessState
                    ? { label: 'Freshness', value: record.freshnessState }
                    : null,
                  record.tags.length > 0
                    ? { label: 'Tags', value: record.tags.join(', ') }
                    : null,
                  record.relatedPaths.length > 0
                    ? { label: 'Paths', value: record.relatedPaths.join(', ') }
                    : null,
                ]}
              />
            </li>
          ))}
        </ul>
      )}
    </SectionShell>
  )
}

interface ApprovedMemorySectionProps {
  memories: AgentKnowledgeApprovedMemoryDto[]
  limit: number
}

function ApprovedMemorySection({ memories, limit }: ApprovedMemorySectionProps) {
  return (
    <SectionShell
      title="Approved memory"
      icon={<Brain className="h-3.5 w-3.5" aria-hidden="true" />}
      count={memories.length}
      limit={limit}
      aria-label="Approved memory"
    >
      {memories.length === 0 ? (
        <EmptyHint message="No approved memories are in scope for this run." />
      ) : (
        <ul className="flex flex-col gap-2">
          {memories.map((memory) => (
            <li
              key={memory.memoryId}
              className="rounded-md border border-border/40 bg-background/40 px-3 py-2"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0 flex-1">
                  <p className="line-clamp-3 text-[12px] text-foreground/85">
                    {memory.textPreview}
                  </p>
                  <p className="mt-0.5 font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
                    {memory.scope} · {memory.kind}
                  </p>
                </div>
                <Badge variant="outline" className="h-5 px-1.5 text-[10.5px] font-normal">
                  {memory.freshnessState}
                </Badge>
              </div>
              <MetaRow
                items={[
                  typeof memory.confidence === 'number'
                    ? { label: 'Confidence', value: String(memory.confidence) }
                    : null,
                  memory.sourceRunId ? { label: 'Source run', value: memory.sourceRunId } : null,
                  memory.sourceItemIds.length > 0
                    ? { label: 'Sources', value: memory.sourceItemIds.join(', ') }
                    : null,
                ]}
              />
            </li>
          ))}
        </ul>
      )}
    </SectionShell>
  )
}

interface HandoffRecordsSectionProps {
  handoffRecords: AgentKnowledgeHandoffRecordDto[]
  limit: number
}

function HandoffRecordsSection({ handoffRecords, limit }: HandoffRecordsSectionProps) {
  return (
    <SectionShell
      title="Handoff records"
      icon={<GitMerge className="h-3.5 w-3.5" aria-hidden="true" />}
      count={handoffRecords.length}
      limit={limit}
      aria-label="Handoff records"
    >
      {handoffRecords.length === 0 ? (
        <EmptyHint message="No handoff records are linked to this run." />
      ) : (
        <ul className="flex flex-col gap-2">
          {handoffRecords.map((handoff) => (
            <li
              key={handoff.handoffId}
              className="rounded-md border border-border/40 bg-background/40 px-3 py-2"
            >
              <div className="flex items-start justify-between gap-2">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12px] font-medium text-foreground">
                    {handoff.handoffId}
                  </p>
                  <p className="font-mono text-[10.5px] uppercase tracking-wide text-muted-foreground">
                    {handoff.runtimeAgentId} · v{handoff.agentDefinitionVersion}
                  </p>
                </div>
                <Badge variant="outline" className="h-5 px-1.5 text-[10.5px] font-normal">
                  {handoff.status}
                </Badge>
              </div>
              <MetaRow
                items={[
                  { label: 'Source run', value: handoff.sourceRunId },
                  handoff.targetRunId
                    ? { label: 'Target run', value: handoff.targetRunId }
                    : null,
                  {
                    label: 'Provider',
                    value: `${handoff.providerId} · ${handoff.modelId}`,
                  },
                  handoff.bundleKeys.length > 0
                    ? { label: 'Bundle keys', value: handoff.bundleKeys.join(', ') }
                    : null,
                ]}
              />
            </li>
          ))}
        </ul>
      )}
    </SectionShell>
  )
}

interface SectionShellProps {
  title: string
  icon: React.ReactNode
  count: number
  limit: number
  children: React.ReactNode
}

function SectionShell({
  title,
  icon,
  count,
  limit,
  children,
  ...rest
}: SectionShellProps & React.HTMLAttributes<HTMLElement>) {
  return (
    <section
      className="rounded-md border border-border/40 bg-background/30"
      {...rest}
    >
      <header className="flex items-center justify-between border-b border-border/40 px-3 py-1.5">
        <div className="flex items-center gap-1.5 text-[12px] text-foreground/80">
          {icon}
          <span className="font-medium">{title}</span>
        </div>
        <span className="font-mono text-[10.5px] text-muted-foreground">
          {count} / {limit}
        </span>
      </header>
      <div className="px-3 py-2.5">{children}</div>
    </section>
  )
}

function EmptyHint({ message }: { message: string }) {
  return <p className="text-[11.5px] text-muted-foreground">{message}</p>
}

function RedactionBadge({
  state,
}: {
  state: AgentKnowledgeProjectRecordDto['redactionState']
}) {
  if (state === 'clean') {
    return null
  }
  return (
    <Badge variant="outline" className="h-5 px-1.5 text-[10.5px] font-normal">
      {state}
    </Badge>
  )
}

interface MetaRowItem {
  label: string
  value: string
}

function MetaRow({ items }: { items: (MetaRowItem | null)[] }) {
  const filtered = useMemo(
    () => items.filter((item): item is MetaRowItem => item !== null),
    [items],
  )
  if (filtered.length === 0) {
    return null
  }
  return (
    <dl className="mt-1.5 flex flex-wrap gap-x-3 gap-y-0.5">
      {filtered.map((item) => (
        <div key={item.label} className="flex items-center gap-1 text-[10.5px]">
          <dt className="font-mono uppercase tracking-wide text-muted-foreground">
            {item.label}
          </dt>
          <dd className="max-w-[36ch] truncate font-mono text-foreground/80">{item.value}</dd>
        </div>
      ))}
    </dl>
  )
}
