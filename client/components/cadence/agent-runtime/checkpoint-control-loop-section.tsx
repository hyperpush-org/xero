import { AlertCircle, LoaderCircle, ShieldCheck } from 'lucide-react'

import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import { Alert, AlertDescription, AlertTitle } from '@/components/ui/alert'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'

import {
  getCheckpointControlLoopBrokerBadgeVariant,
  getCheckpointControlLoopCoverageAlertMeta,
  getCheckpointControlLoopDurableBadgeVariant,
  getCheckpointControlLoopEvidenceBadgeVariant,
  getCheckpointControlLoopFailureBadgeVariant,
  getCheckpointControlLoopRecoveryAlertMeta,
  getCheckpointControlLoopRecoveryBadgeVariant,
  getCheckpointControlLoopResumabilityBadgeVariant,
  getCheckpointControlLoopTruthBadgeVariant,
  getPerActionResumeStateMeta,
} from './checkpoint-control-loop-helpers'
import { displayValue, formatTimestamp } from './shared-helpers'
import type { PendingOperatorIntent } from './use-agent-runtime-controller'

type CheckpointControlLoop = NonNullable<AgentPaneView['checkpointControlLoop']>

type CheckpointControlLoopCard = CheckpointControlLoop['items'][number]

interface CheckpointControlLoopSectionProps {
  checkpointControlLoop: CheckpointControlLoop
  pendingApprovalCount: number
  operatorActionError: AgentPaneView['operatorActionError']
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  pendingOperatorIntent: PendingOperatorIntent | null
  operatorAnswers: Record<string, string>
  checkpointControlLoopRecoveryAlert: ReturnType<typeof getCheckpointControlLoopRecoveryAlertMeta>
  checkpointControlLoopCoverageAlert: ReturnType<typeof getCheckpointControlLoopCoverageAlertMeta>
  onOperatorAnswerChange: (actionId: string, value: string) => void
  onResolveOperatorAction: (
    actionId: string,
    decision: 'approve' | 'reject',
    options?: { userAnswer?: string | null },
  ) => Promise<void>
  onResumeOperatorRun: (actionId: string, options?: { userAnswer?: string | null }) => Promise<void>
}

function normalizeAnswerInput(value: string): string {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : ''
}

function isOperatorActionPending(options: {
  actionId: string
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  pendingOperatorIntent: PendingOperatorIntent | null
}): boolean {
  return (
    options.pendingOperatorIntent?.actionId === options.actionId ||
    (options.operatorActionStatus === 'running' && options.pendingOperatorActionId === options.actionId)
  )
}

export function CheckpointControlLoopSection({
  checkpointControlLoop,
  pendingApprovalCount,
  operatorActionError,
  operatorActionStatus,
  pendingOperatorActionId,
  pendingOperatorIntent,
  operatorAnswers,
  checkpointControlLoopRecoveryAlert,
  checkpointControlLoopCoverageAlert,
  onOperatorAnswerChange,
  onResolveOperatorAction,
  onResumeOperatorRun,
}: CheckpointControlLoopSectionProps) {
  return (
    <section className="rounded-2xl border border-border/70 bg-card/80 p-5 shadow-sm">
      <div className="flex flex-col gap-4">
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <p className="text-[11px] font-medium uppercase tracking-[0.18em] text-muted-foreground">
              Operator checkpoints
            </p>
            <h2 className="mt-2 text-lg font-semibold text-foreground">Checkpoint control loop</h2>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Badge variant={pendingApprovalCount > 0 ? 'secondary' : 'outline'}>{pendingApprovalCount} pending</Badge>
            <Badge variant="outline">{checkpointControlLoop.windowLabel}</Badge>
            {checkpointControlLoop.isTruncated ? (
              <Badge variant="secondary">
                +{checkpointControlLoop.hiddenCount} older action{checkpointControlLoop.hiddenCount === 1 ? '' : 's'}
              </Badge>
            ) : null}
          </div>
        </div>

        <p className="text-sm leading-6 text-muted-foreground">
          Cadence correlates live action-required hints with durable approvals, broker fan-out, resume history, and
          bounded evidence so the same action and boundary stay traceable from pause to recovery.
        </p>

        {operatorActionError ? (
          <Alert variant="destructive">
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>Operator action failed</AlertTitle>
            <AlertDescription>
              <p>{operatorActionError.message}</p>
              <p className="font-mono text-[11px] text-destructive/80">code: {operatorActionError.code}</p>
            </AlertDescription>
          </Alert>
        ) : null}

        {checkpointControlLoopRecoveryAlert ? (
          <Alert variant={checkpointControlLoopRecoveryAlert.variant}>
            {checkpointControlLoopRecoveryAlert.variant === 'destructive' ? (
              <AlertCircle className="h-4 w-4" />
            ) : (
              <ShieldCheck className="h-4 w-4" />
            )}
            <AlertTitle>{checkpointControlLoopRecoveryAlert.title}</AlertTitle>
            <AlertDescription>{checkpointControlLoopRecoveryAlert.body}</AlertDescription>
          </Alert>
        ) : null}

        {checkpointControlLoopCoverageAlert ? (
          <Alert>
            <AlertCircle className="h-4 w-4" />
            <AlertTitle>{checkpointControlLoopCoverageAlert.title}</AlertTitle>
            <AlertDescription>{checkpointControlLoopCoverageAlert.body}</AlertDescription>
          </Alert>
        ) : null}

        {checkpointControlLoop.items.length > 0 ? (
          <div className="space-y-3">
            {checkpointControlLoop.items.map((card) => (
              <CheckpointControlLoopCardView
                key={card.key}
                card={card}
                operatorActionStatus={operatorActionStatus}
                pendingOperatorActionId={pendingOperatorActionId}
                pendingOperatorIntent={pendingOperatorIntent}
                answerValue={operatorAnswers[card.actionId] ?? card.approval?.userAnswer ?? ''}
                onOperatorAnswerChange={onOperatorAnswerChange}
                onResolveOperatorAction={onResolveOperatorAction}
                onResumeOperatorRun={onResumeOperatorRun}
              />
            ))}
          </div>
        ) : (
          <FeedEmptyState title={checkpointControlLoop.emptyTitle} body={checkpointControlLoop.emptyBody} />
        )}
      </div>
    </section>
  )
}

function CheckpointControlLoopCardView({
  card,
  operatorActionStatus,
  pendingOperatorActionId,
  pendingOperatorIntent,
  answerValue,
  onOperatorAnswerChange,
  onResolveOperatorAction,
  onResumeOperatorRun,
}: {
  card: CheckpointControlLoopCard
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  pendingOperatorIntent: PendingOperatorIntent | null
  answerValue: string
  onOperatorAnswerChange: (actionId: string, value: string) => void
  onResolveOperatorAction: (
    actionId: string,
    decision: 'approve' | 'reject',
    options?: { userAnswer?: string | null },
  ) => Promise<void>
  onResumeOperatorRun: (actionId: string, options?: { userAnswer?: string | null }) => Promise<void>
}) {
  const approval = card.approval
  const normalizedAnswer = normalizeAnswerInput(answerValue)
  const requiresAnswer = approval?.requiresUserAnswer ?? false
  const showAnswerError = Boolean(approval) && requiresAnswer && answerValue.length > 0 && normalizedAnswer.length === 0
  const actionPending = isOperatorActionPending({
    actionId: card.actionId,
    operatorActionStatus,
    pendingOperatorActionId,
    pendingOperatorIntent,
  })
  const resumeMeta = getPerActionResumeStateMeta({
    card,
    operatorActionStatus,
    pendingOperatorActionId,
    pendingOperatorIntent,
  })

  return (
    <div className="rounded-xl border border-border/70 bg-card/70 p-4">
      <div className="flex flex-wrap items-start justify-between gap-3">
        <div>
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-sm font-semibold text-foreground">{card.title}</p>
            <Badge variant={getCheckpointControlLoopTruthBadgeVariant(card.truthSource)}>{card.truthSourceLabel}</Badge>
            <Badge variant={getCheckpointControlLoopDurableBadgeVariant(card)}>{card.durableStateLabel}</Badge>
            {card.advancedFailureClassLabel ? (
              <Badge variant={getCheckpointControlLoopFailureBadgeVariant(card)}>
                {card.advancedFailureClassLabel}
              </Badge>
            ) : null}
            <Badge variant={getCheckpointControlLoopResumabilityBadgeVariant(card)}>
              {displayValue(card.resumabilityLabel, 'Resumability unknown')}
            </Badge>
            <Badge variant={getCheckpointControlLoopRecoveryBadgeVariant(card)}>
              {displayValue(card.recoveryRecommendationLabel, 'Observe durable state')}
            </Badge>
            <Badge variant={resumeMeta.badgeVariant}>{resumeMeta.label}</Badge>
          </div>
          <p className="mt-2 text-sm leading-6 text-muted-foreground">{card.detail}</p>
          <p className="mt-2 text-[11px] text-muted-foreground">
            Action {card.actionId} · Boundary {displayValue(card.boundaryId, 'Pending durable linkage')}
          </p>
          {card.gateLinkageLabel ? <p className="mt-2 text-[11px] text-muted-foreground">{card.gateLinkageLabel}</p> : null}
          <p className="mt-2 text-[11px] text-muted-foreground">{card.truthSourceDetail}</p>
          <p className="mt-2 text-[11px] text-muted-foreground">
            Failure class {displayValue(card.advancedFailureClassLabel, 'Not classified')}
            {card.advancedFailureDiagnosticCode ? ` · ${card.advancedFailureDiagnosticCode}` : ''}
          </p>
          <p className="mt-2 text-[11px] text-muted-foreground">
            Resumability {displayValue(card.resumabilityLabel, 'Resumability unknown')} ·{' '}
            {displayValue(card.resumabilityDetail, 'Cadence has not observed enough durable approval or resume evidence yet.')}
          </p>
          <p className="mt-2 text-[11px] text-muted-foreground">
            Recovery guidance {displayValue(card.recoveryRecommendationLabel, 'Observe durable state')} ·{' '}
            {displayValue(
              card.recoveryRecommendationDetail,
              'No typed advanced failure metadata is available yet. Keep the current durable state visible and wait for canonical evidence before retrying.',
            )}
          </p>
        </div>
      </div>

      <div className="mt-4 grid gap-3 xl:grid-cols-4">
        <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Live</p>
            <Badge variant={card.liveActionRequired ? 'secondary' : 'outline'}>{card.liveStateLabel}</Badge>
          </div>
          <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{card.liveStateDetail}</p>
          <p className="mt-2 text-[11px] text-muted-foreground">Updated {formatTimestamp(card.liveUpdatedAt)}</p>
        </div>

        <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Resume</p>
            <Badge variant={resumeMeta.badgeVariant}>{resumeMeta.label}</Badge>
          </div>
          <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{resumeMeta.detail}</p>
          <p className="mt-2 text-[11px] text-muted-foreground">Updated {formatTimestamp(resumeMeta.timestamp)}</p>
        </div>

        <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Broker</p>
            <Badge variant={getCheckpointControlLoopBrokerBadgeVariant(card)}>{card.brokerStateLabel}</Badge>
          </div>
          <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{card.brokerStateDetail}</p>
          <p className="mt-2 text-[11px] text-muted-foreground">Updated {formatTimestamp(card.brokerLatestUpdatedAt)}</p>
          {card.brokerRoutePreviews.length > 0 ? (
            <ul className="mt-3 space-y-2">
              {card.brokerRoutePreviews.map((route) => (
                <li key={`${card.key}:${route.routeId}:${route.updatedAt}`} className="rounded-md border border-border/70 px-2 py-2 text-[11px]">
                  <div className="flex flex-wrap items-center gap-2">
                    <Badge variant="outline">{route.routeId}</Badge>
                    <Badge variant="outline">{route.statusLabel}</Badge>
                  </div>
                  <p className="mt-2 text-muted-foreground">{route.detail}</p>
                </li>
              ))}
            </ul>
          ) : null}
        </div>

        <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <div className="flex flex-wrap items-center gap-2">
            <p className="text-xs font-medium uppercase tracking-[0.16em] text-muted-foreground">Evidence</p>
            <Badge variant={getCheckpointControlLoopEvidenceBadgeVariant(card)}>{card.evidenceStateLabel}</Badge>
          </div>
          <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{card.evidenceSummary}</p>
          <p className="mt-2 text-[11px] text-muted-foreground">Latest evidence {formatTimestamp(card.latestEvidenceAt)}</p>
          {card.evidencePreviews.length > 0 ? (
            <ul className="mt-3 space-y-2">
              {card.evidencePreviews.map((artifact) => (
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
      </div>

      {approval ? (
        <div className="mt-4 grid gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(240px,320px)]">
          <div className="space-y-3">
            <div className="rounded-lg border border-border/70 bg-background/70 px-3 py-3">
              <p className="text-sm font-medium text-foreground">
                {requiresAnswer ? 'Required answer contract' : 'Optional answer contract'}
              </p>
              <p className="mt-2 text-[12px] text-muted-foreground">
                <span className="font-medium text-foreground/80">Answer shape:</span> {approval.answerShapeLabel}
              </p>
              {approval.answerShapeHint ? (
                <p className="mt-2 text-[12px] leading-5 text-muted-foreground">{approval.answerShapeHint}</p>
              ) : null}
            </div>

            {approval.isPending || approval.canResume ? (
              <label className="grid gap-2 text-[12px] text-muted-foreground">
                <span>Operator answer</span>
                <Textarea
                  aria-label={`Operator answer for ${card.actionId}`}
                  className="min-h-24"
                  onChange={(event) => onOperatorAnswerChange(card.actionId, event.target.value)}
                  placeholder={approval.answerPlaceholder ?? 'Provide operator input for this action.'}
                  value={answerValue}
                />
                {showAnswerError ? (
                  <span className="text-destructive">
                    {approval.answerRequirementReason === 'runtime_resumable'
                      ? 'A non-empty user answer is required before approving this runtime-resumable request.'
                      : 'A non-empty user answer is required before approving this action.'}
                  </span>
                ) : null}
              </label>
            ) : null}
          </div>

          <div className="space-y-3 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
            <InfoRow label="Action ID" mono value={card.actionId} />
            <InfoRow label="Boundary" mono value={displayValue(card.boundaryId, 'Pending durable linkage')} />
            <InfoRow label="Updated" value={formatTimestamp(resumeMeta.timestamp)} />
            <p className="text-[12px] leading-5 text-muted-foreground">{resumeMeta.detail}</p>

            <div className="flex flex-wrap gap-2">
              {approval.isPending ? (
                <Button
                  disabled={actionPending || (requiresAnswer && normalizedAnswer.length === 0)}
                  onClick={() =>
                    void onResolveOperatorAction(card.actionId, 'approve', {
                      userAnswer: normalizedAnswer.length > 0 ? normalizedAnswer : null,
                    })
                  }
                  type="button"
                >
                  {actionPending && pendingOperatorIntent?.kind === 'approve' ? (
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                  ) : null}
                  Approve
                </Button>
              ) : null}

              {approval.isPending ? (
                <Button
                  disabled={actionPending}
                  onClick={() =>
                    void onResolveOperatorAction(card.actionId, 'reject', {
                      userAnswer: normalizedAnswer.length > 0 ? normalizedAnswer : null,
                    })
                  }
                  type="button"
                  variant="outline"
                >
                  {actionPending && pendingOperatorIntent?.kind === 'reject' ? (
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                  ) : null}
                  Reject
                </Button>
              ) : null}

              {approval.canResume ? (
                <Button
                  disabled={actionPending}
                  onClick={() =>
                    void onResumeOperatorRun(card.actionId, {
                      userAnswer: normalizedAnswer.length > 0 ? normalizedAnswer : approval.userAnswer ?? null,
                    })
                  }
                  type="button"
                  variant="secondary"
                >
                  {actionPending && pendingOperatorIntent?.kind === 'resume' ? (
                    <LoaderCircle className="h-4 w-4 animate-spin" />
                  ) : null}
                  Resume run
                </Button>
              ) : null}
            </div>
          </div>
        </div>
      ) : (
        <div className="mt-4 rounded-lg border border-border/70 bg-background/70 px-3 py-3">
          <p className="text-sm font-medium text-foreground">Durable approval row not available</p>
          <p className="mt-2 text-[12px] leading-5 text-muted-foreground">
            Cadence is keeping the live, broker, resume, and evidence truth visible for this action even though there is
            no actionable durable approval row in the current snapshot.
          </p>
        </div>
      )}
    </div>
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
    <div className="flex items-start justify-between gap-3 text-[11px] text-muted-foreground">
      <span>{label}</span>
      <span
        className={
          mono
            ? 'max-w-[60%] break-all text-right font-mono text-foreground/75'
            : 'max-w-[60%] text-right text-foreground/75'
        }
      >
        {value}
      </span>
    </div>
  )
}
