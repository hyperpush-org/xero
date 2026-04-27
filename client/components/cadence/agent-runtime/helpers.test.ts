import { describe, expect, it } from 'vitest'

import {
  createEmptyCheckpointControlLoop,
  getCheckpointControlLoopCoverageAlertMeta,
  getCheckpointControlLoopRecoveryAlertMeta,
  getPerActionResumeStateMeta,
} from '@/components/cadence/agent-runtime/checkpoint-control-loop-helpers'
import {
  getComposerPlaceholder,
  isSelectedProviderReadyForSession,
} from '@/components/cadence/agent-runtime/composer-helpers'
import { getStreamStatusMeta, getToolSummaryContext } from '@/components/cadence/agent-runtime/runtime-stream-helpers'
import { displayValue, formatSequence } from '@/components/cadence/agent-runtime/shared-helpers'
import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type { RuntimeSessionView, RuntimeStreamToolItemView } from '@/src/lib/cadence-model'

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
      repository: null,
      repositoryStatus: null,
      approvalRequests: [],
      pendingApprovalCount: 0,
      latestDecisionOutcome: null,
      verificationRecords: [],
      resumeHistory: [],
      agentSessions: [],
      selectedAgentSession: null,
      selectedAgentSessionId: 'agent-session-main',
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
    controlTruthSource: 'fallback',
    selectedModelId: 'openai_codex',
    selectedThinkingEffort: null,
    selectedApprovalMode: 'suggest',
    selectedPrompt: {
      text: null,
      queuedAt: null,
      hasQueuedPrompt: false,
    },
    runtimeRunActiveControls: null,
    runtimeRunPendingControls: null,
    providerModelCatalog: {
      profileId: null,
      profileLabel: null,
      providerId: 'openai_codex',
      providerLabel: 'OpenAI Codex',
      source: null,
      loadStatus: 'idle',
      state: 'unavailable',
      stateLabel: 'Catalog unavailable',
      detail: 'Cadence does not have a discovered model catalog for OpenAI Codex yet, so only configured model truth remains visible.',
      fetchedAt: null,
      lastSuccessAt: null,
      lastRefreshError: null,
      models: [],
    },
    selectedModelOption: null,
    selectedModelThinkingEffortOptions: [],
    selectedModelDefaultThinkingEffort: null,
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

  it('formats browser/computer-use tool summaries with safe fallback labels for optional metadata', () => {
    const browserItem: RuntimeStreamToolItemView = {
      id: 'tool:run-1:1',
      kind: 'tool',
      runId: 'run-1',
      sequence: 1,
      createdAt: '2026-04-24T17:30:00Z',
      toolCallId: 'browser-click-1',
      toolName: 'browser.click',
      toolState: 'succeeded',
      detail: 'Clicked submit in browser context.',
      toolSummary: {
        kind: 'browser_computer_use',
        surface: 'browser',
        action: 'click',
        status: 'succeeded',
        target: 'button[type=submit]',
        outcome: 'Clicked submit and advanced to confirmation.',
      },
    }

    const computerItem: RuntimeStreamToolItemView = {
      ...browserItem,
      id: 'tool:run-1:2',
      sequence: 2,
      toolCallId: 'computer-key-1',
      toolName: 'computer_use.key_press',
      toolState: 'failed',
      toolSummary: {
        kind: 'browser_computer_use',
        surface: 'computer_use',
        action: 'press_key',
        status: 'blocked',
        target: null,
        outcome: null,
      },
    }

    expect(getToolSummaryContext(browserItem)).toBe(
      'Browser action click · status Succeeded · target button[type=submit] · outcome Clicked submit and advanced to confirmation.',
    )
    expect(getToolSummaryContext(computerItem)).toBe(
      'Computer use action press_key · status Blocked · target Target unavailable · outcome Outcome unavailable',
    )
    expect(getToolSummaryContext({ ...browserItem, toolSummary: null })).toBeNull()
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
          resumability: 'awaiting_approval',
          resumabilityLabel: 'Awaiting approval',
          resumabilityDetail: 'Operator approval is still required before this checkpoint can resume.',
          isResumable: false,
          advancedFailureClass: null,
          advancedFailureClassLabel: null,
          advancedFailureDiagnosticCode: null,
          recoveryRecommendation: 'observe',
          recoveryRecommendationLabel: 'Observe',
          recoveryRecommendationDetail: 'Wait for durable approval or resume evidence.',
          sortTimestamp: '2026-04-16T20:05:00Z',
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

  it('keeps GitHub Models placeholder copy grammatical and provider-specific', () => {
    expect(
      getComposerPlaceholder(null, 'idle', null, undefined, {
        selectedProviderId: 'github_models',
        selectedProfileReadiness: {
          ready: false,
          status: 'missing',
          proofUpdatedAt: null,
        },
        openrouterApiKeyConfigured: false,
        providerMismatch: false,
      }),
    ).toBe('Configure a GitHub Models API key in Settings to start.')

    expect(
      getComposerPlaceholder(makeRuntimeSession({ isAuthenticated: true, isSignedOut: false }), 'idle', null, undefined, {
        selectedProviderId: 'github_models',
        selectedProfileReadiness: {
          ready: true,
          status: 'ready',
          proofUpdatedAt: '2026-04-20T12:00:00Z',
        },
        openrouterApiKeyConfigured: false,
        providerMismatch: true,
      }),
    ).toBe('Rebind GitHub Models before trusting new live activity.')
  })

  it('does not allow malformed API-key readiness to masquerade as session-ready', () => {
    expect(
      isSelectedProviderReadyForSession({
        selectedProviderId: 'openrouter',
        selectedProfileReadiness: {
          ready: false,
          status: 'malformed',
          proof: 'stored_secret',
          proofUpdatedAt: '2026-04-20T12:00:00Z',
        },
        openrouterApiKeyConfigured: true,
      }),
    ).toBe(false)
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
          isRuntimeResumable: true,
          requiresUserAnswer: true,
          answerRequirementReason: 'runtime_resumable',
          answerRequirementLabel: 'Required',
          answerShapeKind: 'plain_text',
          answerShapeLabel: 'Required user answer',
          answerShapeHint: 'Describe the operator decision that justifies approval.',
          answerPlaceholder: 'Provide operator input for this action.',
        },
        durableStateLabel: 'Approved',
        durableStateDetail: 'Approved by operator.',
        durableUpdatedAt: '2026-04-13T20:03:30Z',
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
        resumability: 'resumable',
        resumabilityLabel: 'Resumable',
        resumabilityDetail: 'The durable approval is resolved and can be resumed.',
        isResumable: true,
        advancedFailureClass: null,
        advancedFailureClassLabel: null,
        advancedFailureDiagnosticCode: null,
        recoveryRecommendation: 'observe',
        recoveryRecommendationLabel: 'Observe',
        recoveryRecommendationDetail: 'Resume evidence has not been recorded yet.',
        sortTimestamp: '2026-04-13T20:03:30Z',
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
