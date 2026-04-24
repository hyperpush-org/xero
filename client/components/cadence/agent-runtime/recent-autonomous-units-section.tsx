import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'

import { displayValue, formatTimestamp, type BadgeVariant } from './shared-helpers'

type RecentAutonomousUnits = NonNullable<AgentPaneView['recentAutonomousUnits']>
type RecentAutonomousUnitCard = RecentAutonomousUnits['items'][number]

interface RecentAutonomousUnitsSectionProps {
  recentAutonomousUnits: RecentAutonomousUnits
}

function getWorkflowBadgeVariant(workflowState: RecentAutonomousUnitCard['workflowState']): BadgeVariant {
  switch (workflowState) {
    case 'ready':
      return 'secondary'
    case 'awaiting_snapshot':
      return 'destructive'
    case 'awaiting_handoff':
      return 'outline'
    case 'unlinked':
      return 'outline'
  }
}

function getLifecycleBadgeVariant(status: RecentAutonomousUnitCard['status']): BadgeVariant {
  switch (status) {
    case 'active':
      return 'secondary'
    case 'completed':
      return 'outline'
    case 'blocked':
    case 'failed':
    case 'cancelled':
      return 'destructive'
    default:
      return 'outline'
  }
}

function formatHashPreview(value: string | null): string {
  if (!value) {
    return 'Pending durable linkage'
  }

  return value.length <= 16 ? value : `${value.slice(0, 16)}…`
}

export function RecentAutonomousUnitsSection({ recentAutonomousUnits }: RecentAutonomousUnitsSectionProps) {
  return (
    <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">Worker lifecycle</p>
            <h2 className="mt-2 text-lg font-semibold text-foreground">Recent autonomous workers</h2>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant="outline">{recentAutonomousUnits.windowLabel}</Badge>
            {recentAutonomousUnits.isTruncated ? (
              <Badge variant="secondary">
                +{recentAutonomousUnits.hiddenCount} older unit{recentAutonomousUnits.hiddenCount === 1 ? '' : 's'}
              </Badge>
            ) : null}
          </div>
        </div>

        <p className="text-sm leading-6 text-muted-foreground">
          Cadence renders durable worker purpose, lifecycle, attempt identity, and workflow-handoff linkage exactly as
          projected so refresh and replay drift remains visible instead of inferred.
        </p>

        <p className="text-xs text-muted-foreground">{recentAutonomousUnits.latestAttemptOnlyCopy}</p>

        {recentAutonomousUnits.items.length > 0 ? (
          <div className="space-y-3">
            {recentAutonomousUnits.items.map((unit) => (
              <RecentAutonomousUnitCardView key={unit.unitId} unit={unit} />
            ))}
          </div>
        ) : (
          <FeedEmptyState title={recentAutonomousUnits.emptyTitle} body={recentAutonomousUnits.emptyBody} />
        )}
      </div>
    </section>
  )
}

function RecentAutonomousUnitCardView({ unit }: { unit: RecentAutonomousUnitCard }) {
  return (
    <Card className="gap-4 border-border/70 bg-card/70 py-4 shadow-none">
      <CardHeader className="gap-3 px-4">
        <div className="flex flex-wrap items-center gap-2">
          <Badge variant="outline">{unit.sequenceLabel}</Badge>
          <Badge variant="outline">{unit.kindLabel}</Badge>
          <Badge variant={getLifecycleBadgeVariant(unit.status)}>{unit.statusLabel}</Badge>
          <Badge variant={getWorkflowBadgeVariant(unit.workflowState)}>{unit.workflowStateLabel}</Badge>
        </div>
        <CardTitle className="text-sm">{unit.summary}</CardTitle>
        <CardDescription>{unit.workflowDetail}</CardDescription>
      </CardHeader>

      <CardContent className="grid gap-3 px-4 lg:grid-cols-3">
        <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Attempt identity</p>
          <InfoRow label="Attempt" value={unit.latestAttemptLabel} />
          <InfoRow label="Attempt ID" value={displayValue(unit.latestAttemptId, 'Pending durable linkage')} mono />
          <InfoRow label="Child session" value={displayValue(unit.latestAttemptChildSessionId, 'Pending durable linkage')} mono />
          <InfoRow label="Updated" value={formatTimestamp(unit.latestAttemptUpdatedAt)} />
          <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{unit.latestAttemptSummary}</p>
        </div>

        <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Workflow linkage</p>
          <InfoRow label="Linkage" value={unit.workflowLinkageLabel} />
          <InfoRow label="Node" value={displayValue(unit.workflowNodeId, unit.workflowNodeLabel)} mono />
          <InfoRow
            label="Transition"
            value={displayValue(unit.workflowTransitionId, 'Pending durable linkage')}
            mono
          />
          <InfoRow
            label="Handoff"
            value={displayValue(unit.workflowHandoffTransitionId, 'Pending durable linkage')}
            mono
          />
          <InfoRow label="Hash" value={formatHashPreview(unit.workflowHandoffPackageHash)} mono />
          <InfoRow label="Boundary" value={displayValue(unit.boundaryId, 'Not linked')} mono />
          <InfoRow label="Updated" value={formatTimestamp(unit.updatedAt)} />
        </div>

        <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Evidence</p>
            <Badge variant={unit.evidenceCount > 0 ? 'secondary' : 'outline'}>{unit.evidenceStateLabel}</Badge>
          </div>
          <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{unit.evidenceSummary}</p>
          <p className="mt-2 text-[11px] text-muted-foreground">Latest evidence {formatTimestamp(unit.latestEvidenceAt)}</p>
          {unit.evidencePreviews.length > 0 ? (
            <ul className="mt-3 space-y-2">
              {unit.evidencePreviews.map((artifact) => (
                <li key={artifact.artifactId} className="rounded-md border border-border/70 px-2 py-2 text-[11px]">
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="outline">{artifact.artifactKindLabel}</Badge>
                    <Badge variant="outline">{artifact.statusLabel}</Badge>
                  </div>
                  <p className="mt-2 text-foreground/85">{artifact.summary}</p>
                </li>
              ))}
            </ul>
          ) : null}
        </div>
      </CardContent>
    </Card>
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

function InfoRow({
  label,
  value,
  mono = false,
}: {
  label: string
  value: string
  mono?: boolean
}) {
  return (
    <div className="mt-2 flex items-start justify-between gap-3 text-[11px] text-muted-foreground first:mt-0">
      <span>{label}</span>
      <span
        className={
          mono
            ? 'max-w-[65%] break-all text-right font-mono text-foreground/75'
            : 'max-w-[65%] text-right text-foreground/75'
        }
      >
        {value}
      </span>
    </div>
  )
}
