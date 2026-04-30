import { describe, expect, it } from 'vitest'
import type {
  NotificationBrokerActionView,
  NotificationBrokerView,
  NotificationDispatchView,
  OperatorApprovalView,
  ResumeHistoryEntryView,
  RuntimeStreamActionRequiredItemView,
} from '@/src/lib/xero-model'
import { projectCheckpointControlLoops } from '@/src/features/xero/agent-runtime-projections'

function makeActionRequired(overrides: Partial<RuntimeStreamActionRequiredItemView> = {}): RuntimeStreamActionRequiredItemView {
  return {
    id: 'action_required:run-1:1',
    kind: 'action_required',
    runId: 'run-1',
    sequence: 1,
    actionId: 'scope:run:run-1:boundary:boundary-1:review_worktree',
    boundaryId: 'boundary-1',
    actionType: 'review_worktree',
    title: 'Review worktree changes',
    detail: 'Inspect the repo diff before continuing.',
    createdAt: '2026-04-16T12:00:00Z',
    ...overrides,
  }
}

function makeApproval(overrides: Partial<OperatorApprovalView> = {}): OperatorApprovalView {
  return {
    actionId: 'scope:run:run-1:boundary:boundary-1:review_worktree',
    sessionId: 'session-1',
    flowId: 'flow-1',
    actionType: 'review_worktree',
    title: 'Review worktree changes',
    detail: 'Inspect the repo diff before continuing.',
    userAnswer: null,
    status: 'pending',
    statusLabel: 'Pending',
    decisionNote: null,
    createdAt: '2026-04-16T12:00:01Z',
    updatedAt: '2026-04-16T12:00:01Z',
    resolvedAt: null,
    isPending: true,
    isResolved: false,
    canResume: false,
    isRuntimeResumable: true,
    requiresUserAnswer: true,
    answerRequirementReason: 'runtime_resumable',
    answerRequirementLabel: 'Required',
    answerShapeKind: 'plain_text',
    answerShapeLabel: 'Plain-text response',
    answerShapeHint: 'Provide plain-text operator context.',
    answerPlaceholder: 'Provide operator input for this action.',
    ...overrides,
  }
}

function makeResume(overrides: Partial<ResumeHistoryEntryView> = {}): ResumeHistoryEntryView {
  return {
    id: 1,
    sourceActionId: 'scope:run:run-1:boundary:boundary-1:review_worktree',
    sessionId: 'session-1',
    status: 'started',
    statusLabel: 'Resume started',
    summary: 'Operator resumed the selected project runtime session.',
    createdAt: '2026-04-16T12:02:00Z',
    ...overrides,
  }
}

function makeDispatch(overrides: Partial<NotificationDispatchView> = {}): NotificationDispatchView {
  return {
    id: 1,
    projectId: 'project-1',
    actionId: 'scope:run:run-1:boundary:boundary-1:review_worktree',
    routeId: 'telegram-primary',
    correlationKey: 'nfy:1',
    status: 'claimed',
    statusLabel: 'Reply claimed',
    attemptCount: 1,
    lastAttemptAt: '2026-04-16T12:00:30Z',
    deliveredAt: '2026-04-16T12:00:35Z',
    claimedAt: '2026-04-16T12:01:00Z',
    lastErrorCode: null,
    lastErrorMessage: null,
    createdAt: '2026-04-16T12:00:30Z',
    updatedAt: '2026-04-16T12:01:00Z',
    isPending: false,
    isSent: false,
    isFailed: false,
    isClaimed: true,
    hasFailureDiagnostics: false,
    ...overrides,
  }
}

function makeBroker(actions: NotificationBrokerActionView[] = []): NotificationBrokerView {
  const dispatches = actions.flatMap((action) => action.dispatches)
  return {
    dispatches,
    actions,
    routes: [],
    byActionId: Object.fromEntries(actions.map((action) => [action.actionId, action])),
    byRouteId: {},
    dispatchCount: dispatches.length,
    routeCount: 0,
    pendingCount: dispatches.filter((dispatch) => dispatch.isPending).length,
    sentCount: dispatches.filter((dispatch) => dispatch.isSent).length,
    failedCount: dispatches.filter((dispatch) => dispatch.isFailed).length,
    claimedCount: dispatches.filter((dispatch) => dispatch.isClaimed).length,
    latestUpdatedAt: dispatches[0]?.updatedAt ?? null,
    isTruncated: false,
    totalBeforeTruncation: dispatches.length,
  }
}

function makeBrokerAction(overrides: Partial<NotificationBrokerActionView> = {}): NotificationBrokerActionView {
  const dispatches = overrides.dispatches ?? [makeDispatch()]
  return {
    actionId: 'scope:run:run-1:boundary:boundary-1:review_worktree',
    dispatches,
    dispatchCount: dispatches.length,
    pendingCount: dispatches.filter((dispatch) => dispatch.isPending).length,
    sentCount: dispatches.filter((dispatch) => dispatch.isSent).length,
    failedCount: dispatches.filter((dispatch) => dispatch.isFailed).length,
    claimedCount: dispatches.filter((dispatch) => dispatch.isClaimed).length,
    latestUpdatedAt: dispatches[0]?.updatedAt ?? null,
    hasFailures: dispatches.some((dispatch) => dispatch.isFailed),
    hasPending: dispatches.some((dispatch) => dispatch.isPending),
    hasClaimed: dispatches.some((dispatch) => dispatch.isClaimed),
    ...overrides,
  }
}

describe('projectCheckpointControlLoops', () => {
  it('returns an empty durable-run scaffold when no checkpoint boundary exists', () => {
    const projection = projectCheckpointControlLoops({
      actionRequiredItems: [],
      approvalRequests: [],
      resumeHistory: [],
      notificationBroker: makeBroker(),
    })

    expect(projection).toMatchObject({
      items: [],
      totalCount: 0,
      windowLabel: 'No checkpoint actions are visible in the bounded control-loop window.',
      emptyTitle: 'No checkpoint control loops recorded',
    })
  })

  it('correlates live action, approval, resume, and broker truth without durable unit evidence', () => {
    const projection = projectCheckpointControlLoops({
      actionRequiredItems: [makeActionRequired()],
      approvalRequests: [makeApproval({ status: 'approved', statusLabel: 'Approved', isPending: false, isResolved: true, canResume: true })],
      resumeHistory: [makeResume()],
      notificationBroker: makeBroker([makeBrokerAction()]),
    })

    expect(projection.totalCount).toBe(1)
    expect(projection.items[0]).toMatchObject({
      actionId: 'scope:run:run-1:boundary:boundary-1:review_worktree',
      boundaryId: 'boundary-1',
      truthSource: 'live_and_durable',
      durableStateLabel: 'Approved',
      resumeStateLabel: 'Resume started',
      brokerStateLabel: '1 route dispatch',
      evidenceCount: 0,
      evidenceStateLabel: 'No durable evidence in bounded window',
    })
  })

  it('keeps live-only action-required rows visible while waiting for durable approval refresh', () => {
    const projection = projectCheckpointControlLoops({
      actionRequiredItems: [makeActionRequired({ actionId: 'action-live', boundaryId: 'boundary-live' })],
      approvalRequests: [],
      resumeHistory: [],
      notificationBroker: makeBroker(),
    })

    expect(projection.items[0]).toMatchObject({
      actionId: 'action-live',
      boundaryId: 'boundary-live',
      truthSource: 'live_hint_only',
      durableStateLabel: 'Durable approval pending refresh',
      resumeStateLabel: 'No durable resume',
    })
    expect(projection.liveHintOnlyCount).toBe(1)
    expect(projection.missingEvidenceCount).toBe(1)
  })
})
