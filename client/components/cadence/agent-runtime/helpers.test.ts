import { describe, expect, it } from 'vitest'

import {
  createEmptyCheckpointControlLoop,
  displayValue,
  formatSequence,
  getCheckpointControlLoopCoverageAlertMeta,
  getCheckpointControlLoopRecoveryAlertMeta,
  getComposerPlaceholder,
  getPerActionResumeStateMeta,
  getStreamStatusMeta,
} from '@/components/cadence/agent-runtime/helpers'
import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type { RuntimeSessionView } from '@/src/lib/cadence-model'

function makeAgent(overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  return {
    project: {
      id: 'project-1',
      name: 'Cadence',
      description: 'Desktop shell',
      milestone: 'M001',
      totalPhases: 0,
      completedPhases: 0,
      activePhase: 0,
      phases: [],
      branch: 'main',
      runtime: 'Runtime unavailable',
      branchLabel: 'main',
      runtimeLabel: 'Runtime unavailable',
      phaseProgressPercent: 0,
      lifecycle: null,
      repository: null,
      repositoryStatus: null,
      approvalRequests: [],
      pendingApprovalCount: 0,
      latestDecisionOutcome: null,
      verificationRecords: [],
      resumeHistory: [],
      handoffPackages: [],
      notificationBroker: {
        dispatches: [],
        actions: [],
        routes: [],
        byActionId: {},
        byRouteId: {},
        dispatchCount: 0,
        routeCount: 0,
        pendingCount: 0,
        sentCount: 0,
        failedCount: 0,
        claimedCount: 0,
        latestUpdatedAt: null,
        isTruncated: false,
        totalBeforeTruncation: 0,
      },
      runtimeSession: null,
      runtimeRun: null,
      autonomousRun: null,
      autonomousUnit: null,
      autonomousAttempt: null,
      autonomousHistory: [],
      autonomousRecentArtifacts: [],
    },
    activePhase: null,
    branchLabel: 'main',
    headShaLabel: 'No HEAD',
    runtimeLabel: 'Runtime unavailable',
    repositoryLabel: 'Cadence',
    repositoryPath: '/tmp/Cadence',
    notificationBroker: {
      dispatches: [],
      actions: [],
      routes: [],
      byActionId: {},
      byRouteId: {},
      dispatchCount: 0,
      routeCount: 0,
      pendingCount: 0,
      sentCount: 0,
      failedCount: 0,
      claimedCount: 0,
      latestUpdatedAt: null,
      isTruncated: false,
      totalBeforeTruncation: 0,
    },
    notificationRoutes: [],
    notificationChannelHealth: [],
    notificationRouteLoadStatus: 'idle',
    notificationRouteIsRefreshing: false,
    notificationRouteError: null,
    notificationSyncSummary: null,
    notificationSyncError: null,
    notificationSyncPollingActive: false,
    notificationSyncPollingActionId: null,
    notificationSyncPollingBoundaryId: null,
    notificationRouteMutationStatus: 'idle',
    pendingNotificationRouteId: null,
    notificationRouteMutationError: null,
    approvalRequests: [],
    pendingApprovalCount: 0,
    latestDecisionOutcome: null,
    resumeHistory: [],
    operatorActionStatus: 'idle',
    pendingOperatorActionId: null,
    operatorActionError: null,
    autonomousRunActionStatus: 'idle',
    pendingAutonomousRunAction: null,
    autonomousRunActionError: null,
    runtimeRunActionStatus: 'idle',
    pendingRuntimeRunAction: null,
    runtimeRunActionError: null,
    sessionUnavailableReason: 'Current session status for this project.',
    runtimeRunUnavailableReason:
      'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
    messagesUnavailableReason: 'Cadence authenticated this project, but the live runtime stream has not started yet.',
    ...overrides,
  }
}

function makeRuntimeSession(overrides: Partial<RuntimeSessionView> = {}): RuntimeSessionView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: null,
    accountId: null,
    phase: 'idle',
    phaseLabel: 'Signed out',
    runtimeLabel: 'Runtime unavailable',
    accountLabel: 'Not signed in',
    sessionLabel: 'No session',
    callbackBound: null,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: 'auth_session_not_found',
    lastError: {
      code: 'auth_session_not_found',
      message: 'Sign in with OpenAI to create a runtime session for this project.',
      retryable: false,
    },
    updatedAt: '2026-04-13T20:00:49Z',
    isAuthenticated: false,
    isLoginInProgress: false,
    needsManualInput: false,
    isSignedOut: true,
    isFailed: false,
    ...overrides,
  }
}

describe('agent-runtime helpers', () => {
  it('keeps blank labels and missing sequences on the existing fallback copy', () => {
    expect(displayValue('   ', 'Unavailable')).toBe('Unavailable')
    expect(formatSequence(null)).toBe('Not observed')
  })

  it('keeps the bounded checkpoint empty state and coverage copy stable', () => {
    expect(createEmptyCheckpointControlLoop()).toMatchObject({
      windowLabel: 'No checkpoint actions are visible in the bounded control-loop window.',
      emptyTitle: 'No checkpoint control loops recorded',
    })

    const coverage = getCheckpointControlLoopCoverageAlertMeta({
      ...createEmptyCheckpointControlLoop(),
      items: [
        {
          key: 'action-1::boundary-1',
          actionId: 'action-1',
          boundaryId: 'boundary-1',
          title: 'Review worktree changes',
          detail: 'Inspect the repository diff before trusting the next operator step.',
          truthSource: 'live_hint_only',
          truthSourceLabel: 'Live hint only',
          truthSourceDetail: 'Waiting for durable approval rows.',
          liveActionRequired: null,
          liveStateLabel: 'Live action required',
          liveStateDetail: 'Live row only.',
          liveUpdatedAt: '2026-04-16T20:05:00Z',
          approval: null,
          durableStateLabel: 'Durable approval pending refresh',
          durableStateDetail: 'Pending durable refresh.',
          durableUpdatedAt: null,
          gateLinkageLabel: null,
          brokerAction: null,
          brokerStateLabel: 'No broker fan-out observed',
          brokerStateDetail: 'No broker fan-out rows retained.',
          brokerLatestUpdatedAt: null,
          brokerRoutePreviews: [],
          evidenceCount: 0,
          evidenceStateLabel: 'No durable evidence in bounded window',
          evidenceSummary: 'No evidence retained.',
          latestEvidenceAt: null,
          evidencePreviews: [],
          latestResume: null,
          resumeStateLabel: 'Waiting on approval',
          resumeDetail: 'Waiting for operator input before this action can resume the run.',
          resumeUpdatedAt: '2026-04-16T20:05:00Z',
        },
      ],
      totalCount: 3,
      visibleCount: 1,
      hiddenCount: 2,
      isTruncated: true,
      missingEvidenceCount: 1,
      liveHintOnlyCount: 1,
      recoveredCount: 1,
    })

    expect(coverage?.title).toBe('Bounded checkpoint coverage')
    expect(coverage?.body).toContain('2 older checkpoint actions are outside this bounded window.')
    expect(coverage?.body).toContain('1 card still lacks durable evidence inside the bounded artifact window.')
  })

  it('keeps provider mismatch and signed-out placeholder copy stable', () => {
    expect(
      getComposerPlaceholder(null, 'idle', null, undefined, {
        selectedProviderId: 'openrouter',
        openrouterApiKeyConfigured: false,
        providerMismatch: false,
      }),
    ).toBe('Configure an OpenRouter API key in Settings to start.')

    expect(
      getComposerPlaceholder(makeRuntimeSession({ isAuthenticated: true, isSignedOut: false }), 'idle', null, undefined, {
        selectedProviderId: 'openrouter',
        openrouterApiKeyConfigured: true,
        providerMismatch: true,
      }),
    ).toBe('Rebind OpenRouter before trusting new live activity.')
  })

  it('keeps the stream meta and degraded checkpoint alert copy stable', () => {
    const meta = getStreamStatusMeta(
      makeAgent({
        runtimeRun: { runId: 'run-unavailable' } as never,
      }),
      makeRuntimeSession({
        isAuthenticated: true,
        isSignedOut: false,
        lastError: null,
        lastErrorCode: null,
      }),
    )

    expect(meta.title).toBe('No supervised run attached yet')

    const alert = getCheckpointControlLoopRecoveryAlertMeta({
      controlLoop: {
        ...createEmptyCheckpointControlLoop(),
        items: [{ key: 'action-1::boundary-1' } as never],
      },
      trustSnapshot: {
        syncState: 'degraded',
        syncReason: 'Cadence could not sync notification adapters for this project.',
      },
      autonomousRunErrorMessage: null,
      notificationSyncPollingActive: true,
      notificationSyncPollingActionId: 'action-live-only',
      notificationSyncPollingBoundaryId: 'boundary-live-only',
    })

    expect(alert?.title).toBe('Showing last truthful checkpoint loop')
    expect(alert?.body).toContain('boundary-live-only')
    expect(alert?.body).toContain('action-live-only')
  })

  it('keeps per-action resume state fail-closed when no resume exists yet', () => {
    const resumeMeta = getPerActionResumeStateMeta({
      card: {
        key: 'action-1::boundary-1',
        actionId: 'action-1',
        boundaryId: 'boundary-1',
        title: 'Review worktree changes',
        detail: 'Inspect the repository diff before trusting the next operator step.',
        truthSource: 'durable_only',
        truthSourceLabel: 'Durable only',
        truthSourceDetail: 'Durable approval persisted.',
        liveActionRequired: null,
        liveStateLabel: 'No live action required',
        liveStateDetail: 'Waiting for durable resume.',
        liveUpdatedAt: null,
        approval: {
          actionId: 'action-1',
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_worktree',
          title: 'Review worktree changes',
          detail: 'Inspect the repository diff before trusting the next operator step.',
          gateNodeId: 'workflow-research',
          gateKey: 'requires_user_input',
          transitionFromNodeId: 'workflow-discussion',
          transitionToNodeId: 'workflow-research',
          transitionKind: 'advance',
          userAnswer: null,
          status: 'approved',
          statusLabel: 'Approved',
          decisionNote: null,
          createdAt: '2026-04-13T20:01:00Z',
          updatedAt: '2026-04-13T20:03:30Z',
          resolvedAt: '2026-04-13T20:03:30Z',
          isPending: false,
          isResolved: true,
          canResume: true,
          isGateLinked: true,
          isRuntimeResumable: false,
          requiresUserAnswer: true,
          answerRequirementReason: 'gate_linked',
          answerRequirementLabel: 'Required',
          answerShapeKind: 'plain_text',
          answerShapeLabel: 'Required user answer',
          answerShapeHint: 'Describe the operator decision that justifies approval.',
          answerPlaceholder: 'Provide operator input for this action.',
        },
        durableStateLabel: 'Approved',
        durableStateDetail: 'Approved by operator.',
        durableUpdatedAt: '2026-04-13T20:03:30Z',
        gateLinkageLabel: 'workflow-research · requires_user_input',
        brokerAction: null,
        brokerStateLabel: 'No broker fan-out observed',
        brokerStateDetail: 'No broker rows retained.',
        brokerLatestUpdatedAt: null,
        brokerRoutePreviews: [],
        evidenceCount: 0,
        evidenceStateLabel: 'No durable evidence in bounded window',
        evidenceSummary: 'No evidence retained.',
        latestEvidenceAt: null,
        evidencePreviews: [],
        latestResume: null,
        resumeStateLabel: 'Waiting',
        resumeDetail: 'No resume recorded yet for this action.',
        resumeUpdatedAt: '2026-04-13T20:03:30Z',
      },
      operatorActionStatus: 'idle',
      pendingOperatorActionId: null,
      pendingOperatorIntent: null,
    })

    expect(resumeMeta).toMatchObject({
      label: 'Waiting',
      detail: 'No resume recorded yet for this action.',
      badgeVariant: 'outline',
    })
  })
})
