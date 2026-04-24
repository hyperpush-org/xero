import type {
  AgentPaneView,
  AgentTrustSnapshotView,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  OperatorApprovalView,
  ResumeHistoryEntryView,
} from '@/src/lib/cadence-model'

import { type BadgeVariant, displayValue } from './shared-helpers'

type CheckpointControlLoopCard = NonNullable<AgentPaneView['checkpointControlLoop']>['items'][number]
type OperatorIntentKind = 'approve' | 'reject' | 'resume'
type PerActionResumeState = 'waiting' | 'running' | 'started' | 'failed'

export interface PerActionResumeStateMeta {
  state: PerActionResumeState
  label: string
  detail: string
  badgeVariant: BadgeVariant
  timestamp: string | null
}

export function createEmptyCheckpointControlLoop(): NonNullable<AgentPaneView['checkpointControlLoop']> {
  return {
    items: [],
    totalCount: 0,
    visibleCount: 0,
    hiddenCount: 0,
    isTruncated: false,
    windowLabel: 'No checkpoint actions are visible in the bounded control-loop window.',
    emptyTitle: 'No checkpoint control loops recorded',
    emptyBody:
      'Cadence has not observed a live or durable checkpoint boundary for this project yet. Waiting boundaries, resume outcomes, and broker fan-out will appear here once recorded.',
    missingEvidenceCount: 0,
    liveHintOnlyCount: 0,
    durableOnlyCount: 0,
    recoveredCount: 0,
  }
}

export function getCheckpointControlLoopTruthBadgeVariant(
  truthSource: CheckpointControlLoopCard['truthSource'],
): BadgeVariant {
  switch (truthSource) {
    case 'live_and_durable':
      return 'default'
    case 'live_hint_only':
      return 'secondary'
    case 'durable_only':
    case 'recovered_durable':
      return 'outline'
  }
}

export function getApprovalBadgeVariant(status: OperatorApprovalView['status']): BadgeVariant {
  switch (status) {
    case 'pending':
      return 'secondary'
    case 'approved':
      return 'default'
    case 'rejected':
      return 'destructive'
  }
}

export function getCheckpointControlLoopDurableBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  if (card.approval) {
    return getApprovalBadgeVariant(card.approval.status)
  }

  if (card.liveActionRequired) {
    return 'secondary'
  }

  return 'outline'
}

export function getCheckpointControlLoopBrokerBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  if (card.brokerAction?.hasFailures) {
    return 'destructive'
  }

  if (card.brokerAction?.hasPending) {
    return 'secondary'
  }

  return card.brokerAction ? 'default' : 'outline'
}

export function getCheckpointControlLoopEvidenceBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  return card.evidenceCount > 0 ? 'outline' : 'secondary'
}

export function getCheckpointControlLoopFailureBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  switch (card.advancedFailureClass) {
    case 'timeout':
      return 'secondary'
    case 'policy_permission':
      return 'destructive'
    case 'validation_runtime':
      return 'outline'
    default:
      return 'outline'
  }
}

export function getCheckpointControlLoopResumabilityBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  switch (card.resumability) {
    case 'resumable':
      return 'default'
    case 'awaiting_approval':
      return 'secondary'
    case 'not_resumable':
      return 'destructive'
    case 'unknown':
      return 'outline'
    default:
      return 'outline'
  }
}

export function getCheckpointControlLoopRecoveryBadgeVariant(card: CheckpointControlLoopCard): BadgeVariant {
  switch (card.recoveryRecommendation) {
    case 'approve_resume':
      return 'default'
    case 'retry':
      return 'secondary'
    case 'fix_permissions_policy':
      return 'destructive'
    case 'observe':
      return 'outline'
    default:
      return 'outline'
  }
}

export function getCheckpointControlLoopRecoveryAlertMeta(options: {
  controlLoop: NonNullable<AgentPaneView['checkpointControlLoop']>
  trustSnapshot: Pick<AgentTrustSnapshotView, 'syncState' | 'syncReason'>
  autonomousRunErrorMessage: string | null | undefined
  notificationSyncPollingActive: boolean
  notificationSyncPollingActionId: string | null
  notificationSyncPollingBoundaryId: string | null
}) {
  if (options.controlLoop.items.length === 0) {
    return null
  }

  if (options.notificationSyncPollingActive && options.trustSnapshot.syncState === 'degraded') {
    return {
      title: 'Showing last truthful checkpoint loop',
      body: `Cadence is still polling remote routes for blocked boundary ${displayValue(options.notificationSyncPollingBoundaryId, 'unknown')} and action ${displayValue(options.notificationSyncPollingActionId, 'unknown')} while preserving the last truthful sync summary. ${options.trustSnapshot.syncReason}`,
      variant: 'destructive' as const,
    }
  }

  if (options.trustSnapshot.syncState === 'degraded') {
    return {
      title: 'Showing last truthful checkpoint loop',
      body: options.trustSnapshot.syncReason,
      variant: 'destructive' as const,
    }
  }

  if (options.autonomousRunErrorMessage) {
    return {
      title: 'Recovered checkpoint state remains visible',
      body: options.autonomousRunErrorMessage,
      variant: 'default' as const,
    }
  }

  if (options.notificationSyncPollingActive) {
    return {
      title: 'Remote escalation is actively polling this checkpoint',
      body: `Cadence is polling remote routes for blocked boundary ${displayValue(options.notificationSyncPollingBoundaryId, 'unknown')} and action ${displayValue(options.notificationSyncPollingActionId, 'unknown')} while durable approval, broker, and resume truth remain visible here.`,
      variant: 'default' as const,
    }
  }

  return null
}

export function getCheckpointControlLoopCoverageAlertMeta(
  controlLoop: NonNullable<AgentPaneView['checkpointControlLoop']>,
) {
  if (controlLoop.items.length === 0) {
    return null
  }

  const coverageNotes: string[] = []
  if (controlLoop.isTruncated) {
    coverageNotes.push(`${controlLoop.hiddenCount} older checkpoint action${controlLoop.hiddenCount === 1 ? '' : 's'} are outside this bounded window.`)
  }
  if (controlLoop.liveHintOnlyCount > 0) {
    coverageNotes.push(
      controlLoop.liveHintOnlyCount === 1
        ? '1 card is still anchored to live hints while durable rows persist.'
        : `${controlLoop.liveHintOnlyCount} cards are still anchored to live hints while durable rows persist.`,
    )
  }
  if (controlLoop.missingEvidenceCount > 0) {
    coverageNotes.push(
      controlLoop.missingEvidenceCount === 1
        ? '1 card still lacks durable evidence inside the bounded artifact window.'
        : `${controlLoop.missingEvidenceCount} cards still lack durable evidence inside the bounded artifact window.`,
    )
  }
  if (controlLoop.recoveredCount > 0) {
    coverageNotes.push(
      controlLoop.recoveredCount === 1
        ? '1 card is being shown from recovered durable history after the live row cleared.'
        : `${controlLoop.recoveredCount} cards are being shown from recovered durable history after the live row cleared.`,
    )
  }

  if (coverageNotes.length === 0) {
    return null
  }

  return {
    title: 'Bounded checkpoint coverage',
    body: coverageNotes.join(' '),
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

export function getPerActionResumeStateMeta(options: {
  card: CheckpointControlLoopCard
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  pendingOperatorIntent: { actionId: string; kind: OperatorIntentKind } | null
}): PerActionResumeStateMeta {
  const { card, operatorActionStatus, pendingOperatorActionId, pendingOperatorIntent } = options
  const approval = card.approval
  const latestResumeForAction = card.latestResume
  const isActionInFlight =
    (operatorActionStatus === 'running' && pendingOperatorActionId === card.actionId) ||
    pendingOperatorIntent?.actionId === card.actionId

  if (isActionInFlight) {
    return {
      state: 'running',
      label: 'Running',
      detail:
        pendingOperatorIntent?.kind === 'resume'
          ? 'Resume request is in flight for this action. Cadence will refresh durable state before updating this card.'
          : 'Decision persistence is in flight for this action. Cadence keeps the last durable resume state visible until refresh completes.',
      badgeVariant: 'secondary',
      timestamp: approval?.updatedAt ?? card.resumeUpdatedAt,
    }
  }

  if (latestResumeForAction?.status === 'failed') {
    return {
      state: 'failed',
      label: 'Failed',
      detail: `Latest resume failed: ${displayValue(latestResumeForAction.summary, 'Resume failed for this action.')}`,
      badgeVariant: 'destructive',
      timestamp: latestResumeForAction.createdAt,
    }
  }

  if (latestResumeForAction?.status === 'started') {
    return {
      state: 'started',
      label: 'Started',
      detail: `Latest resume started: ${displayValue(latestResumeForAction.summary, 'Resume started for this action.')}`,
      badgeVariant: getResumeBadgeVariant(latestResumeForAction.status),
      timestamp: latestResumeForAction.createdAt,
    }
  }

  if (approval?.isPending) {
    return {
      state: 'waiting',
      label: 'Waiting',
      detail: 'Waiting for operator input before this action can resume the run.',
      badgeVariant: 'outline',
      timestamp: approval.updatedAt,
    }
  }

  if (approval?.canResume) {
    return {
      state: 'waiting',
      label: 'Waiting',
      detail: 'No resume recorded yet for this action.',
      badgeVariant: 'outline',
      timestamp: approval.updatedAt,
    }
  }

  return {
    state: 'waiting',
    label: card.resumeStateLabel,
    detail: card.resumeDetail,
    badgeVariant: 'outline',
    timestamp: card.resumeUpdatedAt,
  }
}
