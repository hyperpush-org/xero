import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { openUrlMock } = vi.hoisted(() => ({
  openUrlMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

afterEach(() => {
  openUrlMock.mockReset()
})

if (!HTMLElement.prototype.hasPointerCapture) {
  HTMLElement.prototype.hasPointerCapture = () => false
}

if (!HTMLElement.prototype.setPointerCapture) {
  HTMLElement.prototype.setPointerCapture = () => {}
}

if (!HTMLElement.prototype.releasePointerCapture) {
  HTMLElement.prototype.releasePointerCapture = () => {}
}

import { AgentRuntime } from '@/components/cadence/agent-runtime'
import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  PlanningLifecycleView,
  ProjectDetailView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'

type CheckpointControlLoopCard = NonNullable<AgentPaneView['checkpointControlLoop']>['items'][number]

function makeLifecycle(overrides: Partial<PlanningLifecycleView> = {}): PlanningLifecycleView {
  return {
    stages: [],
    byStage: {
      discussion: null,
      research: null,
      requirements: null,
      roadmap: null,
    },
    hasStages: false,
    activeStage: null,
    actionRequiredCount: 0,
    blockedCount: 0,
    completedCount: 0,
    percentComplete: 0,
    ...overrides,
  }
}

function makeProject(overrides: Partial<ProjectDetailView> = {}): ProjectDetailView {
  return {
    id: 'project-1',
    name: 'Cadence',
    description: 'Desktop shell',
    milestone: 'M001',
    totalPhases: 0,
    completedPhases: 0,
    activePhase: 0,
    phases: [],
    branch: 'No branch',
    runtime: 'Runtime unavailable',
    branchLabel: 'No branch',
    runtimeLabel: 'Runtime unavailable',
    phaseProgressPercent: 0,
    lifecycle: makeLifecycle(),
    repository: {
      id: 'repo-1',
      projectId: 'project-1',
      rootPath: '/tmp/Cadence',
      displayName: 'Cadence',
      branch: null,
      branchLabel: 'No branch',
      headSha: null,
      headShaLabel: 'No HEAD',
      isGitRepo: true,
    },
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
    phase: 'authenticated',
    phaseLabel: 'Authenticated',
    runtimeLabel: 'Openai Codex · Authenticated',
    accountLabel: 'acct@example.com',
    sessionLabel: 'session-1',
    callbackBound: true,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:00:49Z',
    isAuthenticated: true,
    isLoginInProgress: false,
    needsManualInput: false,
    isSignedOut: false,
    isFailed: false,
    ...overrides,
  }
}

function makeRuntimeRun(overrides: Partial<RuntimeRunView> = {}): RuntimeRunView {
  const runtimeRun: RuntimeRunView = {
    projectId: 'project-1',
    runId: 'run-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    runtimeLabel: 'Openai Codex · Supervisor running',
    supervisorKind: 'detached_pty',
    supervisorLabel: 'Detached Pty',
    status: 'running',
    statusLabel: 'Supervisor running',
    transport: {
      kind: 'tcp',
      endpoint: '127.0.0.1:4455',
      liveness: 'reachable',
      livenessLabel: 'Control reachable',
    },
    startedAt: '2026-04-15T20:00:00Z',
    lastHeartbeatAt: '2026-04-15T20:00:05Z',
    lastCheckpointSequence: 1,
    lastCheckpointAt: '2026-04-15T20:00:06Z',
    stoppedAt: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:00:06Z',
    checkpoints: [
      {
        sequence: 1,
        kind: 'bootstrap',
        kindLabel: 'Bootstrap',
        summary: 'Supervisor boot recorded.',
        createdAt: '2026-04-15T20:00:01Z',
      },
    ],
    latestCheckpoint: {
      sequence: 1,
      kind: 'bootstrap',
      kindLabel: 'Bootstrap',
      summary: 'Supervisor boot recorded.',
      createdAt: '2026-04-15T20:00:01Z',
    },
    checkpointCount: 1,
    hasCheckpoints: true,
    isActive: true,
    isTerminal: false,
    isStale: false,
    isFailed: false,
    ...overrides,
  }

  return runtimeRun
}

function makeAutonomousRun(
  overrides: Partial<NonNullable<ProjectDetailView['autonomousRun']>> = {},
): NonNullable<ProjectDetailView['autonomousRun']> {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    runtimeLabel: 'Openai Codex · Autonomous run active',
    supervisorKind: 'detached_pty',
    supervisorLabel: 'Detached Pty',
    status: 'running' as const,
    statusLabel: 'Autonomous run active',
    recoveryState: 'recovery_required' as const,
    recoveryLabel: 'Recovery required',
    activeUnitId: 'auto-run-1:checkpoint:2',
    activeAttemptId: 'auto-run-1:checkpoint:2:attempt:1',
    duplicateStartDetected: false,
    duplicateStartRunId: null,
    duplicateStartReason: null,
    startedAt: '2026-04-16T20:00:00Z',
    lastHeartbeatAt: '2026-04-16T20:00:05Z',
    lastCheckpointAt: '2026-04-16T20:00:06Z',
    pausedAt: '2026-04-16T20:03:00Z',
    cancelledAt: null,
    completedAt: null,
    crashedAt: null,
    stoppedAt: null,
    pauseReason: {
      code: 'operator_pause',
      message: 'Operator paused the autonomous run for review.',
    },
    cancelReason: null,
    crashReason: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-16T20:03:00Z',
    isActive: true,
    needsRecovery: true,
    isTerminal: false,
    isFailed: false,
    ...overrides,
  }
}

function makeAutonomousUnit(overrides: Partial<NonNullable<ProjectDetailView['autonomousUnit']>> = {}) {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    unitId: 'auto-run-1:checkpoint:2',
    sequence: 2,
    kind: 'state' as const,
    kindLabel: 'State',
    status: 'active' as const,
    statusLabel: 'Active',
    summary: 'Recovered the current autonomous unit boundary.',
    boundaryId: 'checkpoint:2',
    workflowLinkage: null,
    startedAt: '2026-04-16T20:00:01Z',
    finishedAt: null,
    updatedAt: '2026-04-16T20:03:00Z',
    lastErrorCode: null,
    lastError: null,
    isActive: true,
    isTerminal: false,
    isFailed: false,
    ...overrides,
  }
}

function makeAutonomousAttempt(
  overrides: Partial<NonNullable<ProjectDetailView['autonomousAttempt']>> = {},
): NonNullable<ProjectDetailView['autonomousAttempt']> {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    unitId: 'auto-run-1:checkpoint:2',
    attemptId: 'auto-run-1:checkpoint:2:attempt:1',
    attemptNumber: 1,
    childSessionId: 'child-session-1',
    status: 'active' as const,
    statusLabel: 'Active',
    boundaryId: 'checkpoint:2',
    workflowLinkage: null,
    startedAt: '2026-04-16T20:00:02Z',
    finishedAt: null,
    updatedAt: '2026-04-16T20:03:00Z',
    lastErrorCode: null,
    lastError: null,
    isActive: true,
    isTerminal: false,
    isFailed: false,
    ...overrides,
  }
}

function makeAutonomousArtifact(
  overrides: Partial<NonNullable<ProjectDetailView['autonomousRecentArtifacts']>[number]> = {},
) {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    unitId: 'auto-run-1:checkpoint:2',
    attemptId: 'auto-run-1:checkpoint:2:attempt:1',
    artifactId: 'auto-run-1:checkpoint:2:attempt:1:tool:readme',
    artifactKind: 'tool_result',
    artifactKindLabel: 'Tool result',
    status: 'recorded' as const,
    statusLabel: 'Recorded',
    summary: 'Read README.md from the imported repository root.',
    contentHash: 'abc123',
    payload: null,
    createdAt: '2026-04-16T20:01:00Z',
    updatedAt: '2026-04-16T20:03:00Z',
    detail: 'Tool `read` succeeded for `README.md`.',
    commandResult: {
      exitCode: 0,
      timedOut: false,
      summary: 'read completed',
    },
    toolName: 'read',
    toolState: 'succeeded' as const,
    toolStateLabel: 'Succeeded',
    evidenceKind: null,
    verificationOutcome: null,
    verificationOutcomeLabel: null,
    diagnosticCode: null,
    actionId: null,
    boundaryId: null,
    isToolResult: true,
    isVerificationEvidence: false,
    isPolicyDenied: false,
    ...overrides,
  }
}

function makeRuntimeStream(overrides: Partial<RuntimeStreamView> = {}): RuntimeStreamView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    runId: 'run-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    status: 'idle',
    items: [],
    transcriptItems: [],
    toolCalls: [],
    skillItems: [],
    activityItems: [],
    actionRequired: [],
    completion: null,
    failure: null,
    lastIssue: null,
    lastItemAt: null,
    lastSequence: null,
    ...overrides,
  }
}

function makeAgentModel(
  overrides: Partial<NonNullable<AgentPaneView['selectedModelOption']>> = {},
): NonNullable<AgentPaneView['selectedModelOption']> {
  return {
    modelId: 'openai_codex',
    label: 'openai_codex',
    displayName: 'openai_codex',
    groupId: 'current_selection',
    groupLabel: 'Current selection',
    availability: 'orphaned',
    availabilityLabel: 'Unavailable',
    thinkingSupported: false,
    thinkingEffortOptions: [],
    defaultThinkingEffort: null,
    ...overrides,
  }
}

function makeProviderModelCatalog(
  overrides: Partial<AgentPaneView['providerModelCatalog']> = {},
): AgentPaneView['providerModelCatalog'] {
  return {
    profileId: null,
    profileLabel: null,
    providerId: 'openai_codex',
    providerLabel: 'OpenAI Codex',
    source: null,
    loadStatus: 'idle',
    state: 'unavailable',
    stateLabel: 'Catalog unavailable',
    detail:
      'Cadence does not have a discovered model catalog for OpenAI Codex yet, so only configured model truth remains visible.',
    fetchedAt: null,
    lastSuccessAt: null,
    lastRefreshError: null,
    models: [makeAgentModel()],
    ...overrides,
  }
}

function makeRecentAutonomousUnits(
  overrides: Partial<NonNullable<AgentPaneView['recentAutonomousUnits']>> = {},
): NonNullable<AgentPaneView['recentAutonomousUnits']> {
  return {
    items: [
      {
        unitId: 'unit-history-2',
        sequence: 2,
        sequenceLabel: '#2',
        kindLabel: 'State',
        status: 'blocked',
        statusLabel: 'Blocked',
        summary: 'Blocked on operator boundary while durable history remains available.',
        boundaryId: 'boundary-2',
        updatedAt: '2026-04-16T20:05:00Z',
        latestAttemptOnlyLabel: 'Only the latest attempt is shown for this unit.',
        latestAttemptLabel: 'Attempt #2',
        latestAttemptStatusLabel: 'Blocked',
        latestAttemptUpdatedAt: '2026-04-16T20:05:00Z',
        latestAttemptSummary: 'Latest durable attempt is blocked for child session child-2.',
        workflowState: 'awaiting_snapshot',
        workflowStateLabel: 'Snapshot lag',
        workflowNodeLabel: 'Research',
        workflowLinkageLabel: 'Attempt linkage',
        workflowDetail:
          'Cadence is keeping lifecycle progression anchored to snapshot truth while the linked node `Research` waits for the active lifecycle stage to catch up.',
        evidenceCount: 1,
        evidenceStateLabel: '1 recent evidence row',
        evidenceSummary: 'Showing the latest durable evidence row linked to this unit.',
        latestEvidenceAt: '2026-04-16T20:05:00Z',
        evidencePreviews: [
          {
            artifactId: 'artifact-1',
            artifactKindLabel: 'Tool result',
            statusLabel: 'Recorded',
            summary: 'Read README.md from the imported repository root.',
            updatedAt: '2026-04-16T20:05:00Z',
          },
        ],
      },
    ],
    totalCount: 1,
    visibleCount: 1,
    hiddenCount: 0,
    isTruncated: false,
    windowLabel: 'Showing 1 durable unit from the recent-history window.',
    latestAttemptOnlyCopy: 'Only the latest durable attempt per unit is shown here.',
    emptyTitle: 'No recent autonomous units recorded',
    emptyBody: 'Cadence has not persisted a bounded autonomous unit history for this project yet.',
    ...overrides,
  }
}

function makeCheckpointControlLoopCard(
  overrides: Partial<CheckpointControlLoopCard> = {},
): CheckpointControlLoopCard {
  const approval = overrides.approval ?? {
    actionId: 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required',
    sessionId: 'session-1',
    flowId: 'flow-1',
    actionType: 'terminal_input_required',
    title: 'Terminal input required',
    detail: 'Provide terminal input before the run can continue.',
    gateNodeId: 'workflow-research',
    gateKey: 'requires_user_input',
    transitionFromNodeId: 'workflow-discussion',
    transitionToNodeId: 'workflow-research',
    transitionKind: 'advance',
    userAnswer: 'Looks good to resume.',
    status: 'approved' as const,
    statusLabel: 'Approved',
    decisionNote: 'Ready to resume.',
    createdAt: '2026-04-16T20:03:00Z',
    updatedAt: '2026-04-16T20:03:30Z',
    resolvedAt: '2026-04-16T20:03:30Z',
    isPending: false,
    isResolved: true,
    canResume: true,
    isGateLinked: true,
    isRuntimeResumable: false,
    requiresUserAnswer: true,
    answerRequirementReason: 'gate_linked' as const,
    answerRequirementLabel: 'Required',
    answerShapeKind: 'plain_text' as const,
    answerShapeLabel: 'Required user answer',
    answerShapeHint: 'Describe the operator decision that justifies approval.',
    answerPlaceholder: 'Provide operator input for this action.',
  }

  return {
    key: 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required::boundary-1',
    actionId: approval.actionId,
    boundaryId: 'boundary-1',
    title: approval.title,
    detail: approval.detail,
    gateLinkageLabel: 'workflow-research · requires_user_input · workflow-discussion → workflow-research (advance)',
    truthSource: 'durable_only',
    truthSourceLabel: 'Durable only',
    truthSourceDetail: 'The live row has cleared or is unavailable, so this card is anchored to durable approval and resume truth.',
    liveActionRequired: null,
    liveStateLabel: 'Live row unavailable',
    liveStateDetail:
      'The selected project snapshot still shows this checkpoint as pending even though the live stream no longer has a matching row.',
    liveUpdatedAt: '2026-04-16T20:03:30Z',
    approval,
    durableStateLabel: approval.statusLabel,
    durableStateDetail: approval.detail,
    durableUpdatedAt: approval.updatedAt,
    latestResume: {
      id: 1,
      sourceActionId: approval.actionId,
      sessionId: 'session-1',
      status: 'started',
      statusLabel: 'Resume started',
      summary: 'Operator resumed the selected project runtime session.',
      createdAt: '2026-04-16T20:04:00Z',
    },
    resumeStateLabel: 'Resume started',
    resumeDetail: 'Operator resumed the selected project runtime session.',
    resumeUpdatedAt: '2026-04-16T20:04:00Z',
    brokerAction: {
      actionId: approval.actionId,
      dispatches: [],
      dispatchCount: 0,
      pendingCount: 0,
      sentCount: 0,
      failedCount: 0,
      claimedCount: 0,
      latestUpdatedAt: null,
      hasFailures: false,
      hasPending: false,
      hasClaimed: false,
    },
    brokerStateLabel: 'Broker diagnostics unavailable',
    brokerStateDetail: 'No notification broker fan-out rows were retained for this action in the bounded dispatch window.',
    brokerLatestUpdatedAt: null,
    brokerRoutePreviews: [],
    evidenceCount: 1,
    evidenceStateLabel: '1 durable evidence row',
    evidenceSummary: 'Showing the latest durable evidence row linked to this action.',
    latestEvidenceAt: '2026-04-16T20:04:10Z',
    evidencePreviews: [
      {
        artifactId: 'artifact-checkpoint-1',
        artifactKindLabel: 'Verification evidence',
        statusLabel: 'Recorded',
        summary: 'Captured resume verification evidence for this action.',
        updatedAt: '2026-04-16T20:04:10Z',
      },
    ],
    sortTimestamp: '2026-04-16T20:04:10Z',
    ...overrides,
  }
}

function makeCheckpointControlLoop(
  overrides: Partial<NonNullable<AgentPaneView['checkpointControlLoop']>> = {},
): NonNullable<AgentPaneView['checkpointControlLoop']> {
  return {
    items: [makeCheckpointControlLoopCard()],
    totalCount: 1,
    visibleCount: 1,
    hiddenCount: 0,
    isTruncated: false,
    windowLabel: 'Showing 1 checkpoint action from the bounded control-loop window.',
    emptyTitle: 'No checkpoint control loops recorded',
    emptyBody:
      'Cadence has not observed a live or durable checkpoint boundary for this project yet. Waiting boundaries, resume outcomes, and broker fan-out will appear here once recorded.',
    missingEvidenceCount: 0,
    liveHintOnlyCount: 0,
    durableOnlyCount: 1,
    recoveredCount: 0,
    ...overrides,
  }
}

function makeAgent(overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  const project = overrides.project ?? makeProject()
  const runtimeSession = overrides.runtimeSession ?? null
  const runtimeRun = overrides.runtimeRun ?? null
  const runtimeStream = overrides.runtimeStream ?? null
  const runtimeStreamStatus = overrides.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const selectedProviderId = overrides.selectedProviderId ?? 'openai_codex'
  const selectedProviderLabel = overrides.selectedProviderLabel ?? 'OpenAI Codex'
  const selectedModelId = overrides.selectedModelId ?? 'openai_codex'
  const fallbackSelectedModelOption = selectedModelId
    ? makeAgentModel({
        modelId: selectedModelId,
        label: selectedModelId,
        displayName: selectedModelId,
      })
    : null
  const providerModelCatalog =
    overrides.providerModelCatalog ??
    makeProviderModelCatalog({
      providerId: selectedProviderId,
      providerLabel: selectedProviderLabel,
      models: fallbackSelectedModelOption ? [fallbackSelectedModelOption] : [],
    })
  const selectedModelOption =
    overrides.selectedModelOption ??
    (selectedModelId
      ? providerModelCatalog.models.find((model) => model.modelId === selectedModelId) ?? fallbackSelectedModelOption
      : null)

  return {
    project,
    activePhase: null,
    branchLabel: project.branchLabel,
    headShaLabel: project.repository?.headShaLabel ?? 'No HEAD',
    runtimeLabel: runtimeSession?.runtimeLabel ?? project.runtimeLabel,
    repositoryLabel: project.repository?.displayName ?? project.name,
    repositoryPath: project.repository?.rootPath ?? null,
    runtimeSession,
    selectedProfileId: overrides.selectedProfileId ?? null,
    selectedProfileLabel: overrides.selectedProfileLabel ?? null,
    selectedProviderId,
    selectedProviderLabel,
    selectedProviderSource: overrides.selectedProviderSource ?? 'provider_profiles',
    selectedModelId,
    providerModelCatalog,
    selectedModelOption,
    selectedModelThinkingEffortOptions:
      overrides.selectedModelThinkingEffortOptions ?? selectedModelOption?.thinkingEffortOptions ?? [],
    selectedModelDefaultThinkingEffort:
      overrides.selectedModelDefaultThinkingEffort ?? selectedModelOption?.defaultThinkingEffort ?? null,
    openrouterApiKeyConfigured: overrides.openrouterApiKeyConfigured ?? false,
    providerMismatch: overrides.providerMismatch ?? false,
    runtimeRun,
    autonomousRun: overrides.autonomousRun ?? project.autonomousRun ?? null,
    autonomousUnit: overrides.autonomousUnit ?? project.autonomousUnit ?? null,
    autonomousAttempt: overrides.autonomousAttempt ?? project.autonomousAttempt ?? null,
    autonomousHistory: overrides.autonomousHistory ?? project.autonomousHistory,
    autonomousRecentArtifacts: overrides.autonomousRecentArtifacts ?? project.autonomousRecentArtifacts,
    runtimeErrorMessage: null,
    runtimeRunErrorMessage: null,
    autonomousRunErrorMessage: null,
    authPhase: runtimeSession?.phase ?? null,
    authPhaseLabel: runtimeSession?.phaseLabel ?? 'Signed out',
    runtimeStream,
    runtimeStreamStatus,
    runtimeStreamStatusLabel: overrides.runtimeStreamStatusLabel ?? 'No live stream',
    runtimeStreamError: overrides.runtimeStreamError ?? null,
    runtimeStreamItems: overrides.runtimeStreamItems ?? [],
    skillItems: overrides.skillItems ?? runtimeStream?.skillItems ?? [],
    activityItems: overrides.activityItems ?? [],
    actionRequiredItems: overrides.actionRequiredItems ?? [],
    approvalRequests: overrides.approvalRequests ?? project.approvalRequests,
    pendingApprovalCount: overrides.pendingApprovalCount ?? project.pendingApprovalCount,
    latestDecisionOutcome: overrides.latestDecisionOutcome ?? project.latestDecisionOutcome,
    resumeHistory: overrides.resumeHistory ?? project.resumeHistory,
    notificationBroker: overrides.notificationBroker ?? project.notificationBroker,
    operatorActionStatus: overrides.operatorActionStatus ?? 'idle',
    pendingOperatorActionId: overrides.pendingOperatorActionId ?? null,
    operatorActionError: overrides.operatorActionError ?? null,
    autonomousRunActionStatus: overrides.autonomousRunActionStatus ?? 'idle',
    pendingAutonomousRunAction: overrides.pendingAutonomousRunAction ?? null,
    autonomousRunActionError: overrides.autonomousRunActionError ?? null,
    runtimeRunActionStatus: overrides.runtimeRunActionStatus ?? 'idle',
    pendingRuntimeRunAction: overrides.pendingRuntimeRunAction ?? null,
    runtimeRunActionError: overrides.runtimeRunActionError ?? null,
    notificationRoutes: [],
    notificationRouteLoadStatus: 'idle',
    notificationRouteError: null,
    notificationRouteMutationStatus: 'idle',
    pendingNotificationRouteId: null,
    notificationRouteMutationError: null,
    notificationChannelHealth: [],
    notificationSyncSummary: null,
    notificationSyncError: null,
    notificationRouteIsRefreshing: false,
    notificationSyncPollingActive: false,
    notificationSyncPollingActionId: null,
    notificationSyncPollingBoundaryId: null,
    trustSnapshot: undefined,
    sessionUnavailableReason: overrides.sessionUnavailableReason ?? 'Current session status for this project.',
    runtimeRunUnavailableReason:
      overrides.runtimeRunUnavailableReason ?? 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
    messagesUnavailableReason:
      overrides.messagesUnavailableReason ?? 'Cadence authenticated this project, but the live runtime stream has not started yet.',
    ...overrides,
  }
}

describe('AgentRuntime current UI', () => {
  it('hides the autonomous ledger and remote-escalation debug panels', () => {
    render(
      <AgentRuntime
        agent={makeAgent({ runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }) })}
        onStartAutonomousRun={vi.fn(async () => null)}
        onInspectAutonomousRun={vi.fn(async () => undefined)}
        onCancelAutonomousRun={vi.fn(async () => undefined)}
      />,
    )

    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByText('No autonomous run recorded')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start autonomous run' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Inspect truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Cancel autonomous run' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
  })

  it('keeps the recovered runtime snapshot visible without rendering removed debug panels', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          autonomousRun: makeAutonomousRun({ duplicateStartDetected: true, duplicateStartRunId: 'auto-run-1' }),
          autonomousUnit: makeAutonomousUnit(),
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({ status: 'idle' }),
          runtimeRunUnavailableReason: 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
          messagesUnavailableReason: 'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.',
        })}
      />,
    )

    expect(screen.getAllByRole('heading', { name: 'Recovered run snapshot' }).length).toBeGreaterThanOrEqual(1)
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByText('Recovered the current autonomous unit boundary.')).not.toBeInTheDocument()
    expect(screen.queryByText('Duplicate start prevented')).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
  })

  it('renders checkpoint control-loop cards with resume actions bound to the same action and boundary', async () => {
    const onResumeOperatorRun = vi.fn(async () => undefined)
    const checkpointCard = makeCheckpointControlLoopCard({
      title: 'Review worktree changes',
      detail: 'Inspect the repository diff before trusting the next operator step.',
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
        userAnswer: 'Looks good to resume.',
        status: 'approved',
        statusLabel: 'Approved',
        decisionNote: 'Ready to resume.',
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
      gateLinkageLabel: 'workflow-research · requires_user_input · workflow-discussion → workflow-research (advance)',
      latestResume: {
        id: 1,
        sourceActionId: 'action-1',
        sessionId: 'session-1',
        status: 'started',
        statusLabel: 'Resume started',
        summary: 'Operator resumed the selected project runtime session.',
        createdAt: '2026-04-13T20:04:00Z',
      },
      resumeStateLabel: 'Resume started',
      resumeDetail: 'Operator resumed the selected project runtime session.',
      resumeUpdatedAt: '2026-04-13T20:04:00Z',
      actionId: 'action-1',
      key: 'action-1::boundary-1',
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession(),
          runtimeRun: makeRuntimeRun(),
          approvalRequests: [checkpointCard.approval!],
          pendingApprovalCount: 0,
          resumeHistory: [checkpointCard.latestResume!],
          checkpointControlLoop: makeCheckpointControlLoop({
            items: [checkpointCard],
          }),
        })}
        onResumeOperatorRun={onResumeOperatorRun}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible()
    const checkpointSection = screen.getByRole('heading', { name: 'Checkpoint control loop' }).closest('section')
    expect(checkpointSection).not.toBeNull()
    const checkpointQueries = within(checkpointSection as HTMLElement)
    expect(checkpointQueries.getByText('Review worktree changes')).toBeVisible()
    expect(checkpointQueries.getByText('Durable only')).toBeVisible()
    expect(
      checkpointQueries.getAllByText('Latest resume started: Operator resumed the selected project runtime session.').length,
    ).toBeGreaterThan(0)
    expect(checkpointQueries.getByText('Captured resume verification evidence for this action.')).toBeVisible()

    fireEvent.click(checkpointQueries.getByRole('button', { name: 'Resume run' }))
    await waitFor(() =>
      expect(onResumeOperatorRun).toHaveBeenCalledWith('action-1', {
        userAnswer: 'Looks good to resume.',
      }),
    )
  })

  it('fails closed on whitespace-only answers and keeps action/boundary ids visible during operator-action failures', () => {
    const pendingCard = makeCheckpointControlLoopCard({
      actionId: 'action-pending',
      key: 'action-pending::boundary-1',
      boundaryId: 'boundary-1',
      title: 'Review worktree changes',
      detail: 'Inspect the repository diff before trusting the next operator step.',
      approval: {
        actionId: 'action-pending',
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
        status: 'pending',
        statusLabel: 'Pending approval',
        decisionNote: null,
        createdAt: '2026-04-13T20:02:00Z',
        updatedAt: '2026-04-13T20:02:00Z',
        resolvedAt: null,
        isPending: true,
        isResolved: false,
        canResume: false,
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
      durableStateLabel: 'Pending approval',
      durableStateDetail: 'Inspect the repository diff before trusting the next operator step.',
      durableUpdatedAt: '2026-04-13T20:02:00Z',
      latestResume: null,
      resumeStateLabel: 'Waiting on approval',
      resumeDetail: 'Cadence is waiting for operator input before this action can resume the run.',
      resumeUpdatedAt: '2026-04-13T20:02:00Z',
      evidenceCount: 0,
      evidenceStateLabel: 'No durable evidence in bounded window',
      evidenceSummary:
        'Cadence did not retain a matching tool result, verification row, or policy denial for this action in the bounded evidence window.',
      latestEvidenceAt: null,
      evidencePreviews: [],
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
          approvalRequests: [pendingCard.approval!],
          pendingApprovalCount: 1,
          operatorActionStatus: 'running',
          pendingOperatorActionId: 'action-pending',
          operatorActionError: {
            code: 'operator_action_failed',
            message: 'Cadence could not approve action action-pending for boundary boundary-1.',
            retryable: true,
          },
          checkpointControlLoop: makeCheckpointControlLoop({
            items: [pendingCard],
          }),
        })}
      />
    )

    const checkpointSection = screen.getByRole('heading', { name: 'Checkpoint control loop' }).closest('section')
    expect(checkpointSection).not.toBeNull()
    const checkpointQueries = within(checkpointSection as HTMLElement)

    fireEvent.change(checkpointQueries.getByLabelText('Operator answer for action-pending'), {
      target: { value: '   ' },
    })

    expect(checkpointQueries.getByText('A non-empty user answer is required before approving this action.')).toBeVisible()
    expect(checkpointQueries.getByRole('button', { name: 'Approve' })).toBeDisabled()
    expect(checkpointQueries.getByRole('button', { name: 'Reject' })).toBeDisabled()
    expect(screen.getByText('Cadence could not approve action action-pending for boundary boundary-1.')).toBeVisible()
    expect(checkpointQueries.getByText(/Action action-pending · Boundary boundary-1/)).toBeVisible()
  })

  it('renders degraded checkpoint recovery banners and bounded coverage copy explicitly', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
          checkpointControlLoop: makeCheckpointControlLoop({
            items: [
              makeCheckpointControlLoopCard({
                truthSource: 'live_hint_only',
                truthSourceLabel: 'Live hint only',
                truthSourceDetail:
                  'Cadence is showing the live action-required row while waiting for durable approval or evidence rows to persist.',
                liveActionRequired: {
                  id: 'action-required-1',
                  kind: 'action_required',
                  runId: 'run-1',
                  sequence: 9,
                  createdAt: '2026-04-16T20:05:00Z',
                  actionId: 'action-live-only',
                  boundaryId: 'boundary-live-only',
                  actionType: 'terminal_input_required',
                  title: 'Terminal input required',
                  detail: 'Provide terminal input before the run can continue.',
                },
                approval: null,
                actionId: 'action-live-only',
                key: 'action-live-only::boundary-live-only',
                boundaryId: 'boundary-live-only',
                liveStateLabel: 'Live action required',
                durableStateLabel: 'Durable approval pending refresh',
                durableStateDetail:
                  'The live action-required row arrived before the selected-project snapshot persisted a matching durable approval row.',
                evidenceCount: 0,
                evidenceStateLabel: 'No durable evidence in bounded window',
                evidenceSummary:
                  'Cadence did not retain a matching tool result, verification row, or policy denial for this action in the bounded evidence window.',
              }),
            ],
            totalCount: 3,
            visibleCount: 1,
            hiddenCount: 2,
            isTruncated: true,
            windowLabel: 'Showing 1 of 3 checkpoint actions in the bounded control-loop window.',
            missingEvidenceCount: 1,
            liveHintOnlyCount: 1,
            durableOnlyCount: 0,
            recoveredCount: 1,
          }),
          notificationSyncError: {
            code: 'notification_adapter_sync_failed',
            message: 'Cadence could not sync notification adapters for this project.',
            retryable: true,
          },
          notificationSyncPollingActive: true,
          notificationSyncPollingActionId: 'action-live-only',
          notificationSyncPollingBoundaryId: 'boundary-live-only',
        })}
      />,
    )

    expect(screen.getByText('Showing last truthful checkpoint loop')).toBeVisible()
    expect(
      screen.getByText(
        /Cadence is still polling remote routes for blocked boundary boundary-live-only and action action-live-only while preserving the last truthful sync summary\./,
      ),
    ).toBeVisible()
    expect(screen.getByText('Bounded checkpoint coverage')).toBeVisible()
    expect(screen.getByText(/2 older checkpoint actions are outside this bounded window/)).toBeVisible()
    expect(screen.getByText(/1 card is being shown from recovered durable history/)).toBeVisible()
    expect(screen.getByText('Live hint only')).toBeVisible()
    expect(screen.getByText('Durable approval pending refresh')).toBeVisible()
  })

  it('keeps OpenRouter provider mismatch truthful without rendering runtime setup affordances', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProfileId: 'openrouter-work',
          selectedProfileLabel: 'OpenRouter Work',
          selectedProviderId: 'openrouter',
          selectedProviderLabel: 'OpenRouter',
          selectedModelId: 'openai/gpt-4.1-mini',
          openrouterApiKeyConfigured: true,
          providerMismatch: true,
          providerMismatchReason:
            'Settings now select provider profile OpenRouter Work (openrouter-work), but the persisted runtime session still reflects OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind the selected profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Settings now select provider profile OpenRouter Work (openrouter-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Settings now select provider profile OpenRouter Work (openrouter-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile before trusting new stream activity.',
          runtimeSession: makeRuntimeSession({
            providerId: 'openai_codex',
            runtimeKind: 'openai_codex',
            phase: 'authenticated',
          }),
        })}
        onStartLogin={vi.fn(async () => null)}
        onStartRuntimeSession={vi.fn(async () => null)}
      />,
    )

    expect(screen.queryByRole('heading', { name: 'OpenRouter is selected in Settings' })).not.toBeInTheDocument()
    expect(screen.queryByText('Provider mismatch')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Rebind OpenRouter runtime' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start OpenAI login' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Manual callback fallback' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Rebind OpenRouter before trusting new live activity.',
    )
  })

  it('renders OpenRouter setup guidance in the centered agent empty state', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'openrouter',
          selectedProviderLabel: 'OpenRouter',
          selectedModelId: 'openai/gpt-4.1-mini',
          openrouterApiKeyConfigured: false,
          runtimeSession: null,
          sessionUnavailableReason:
            'Configure an OpenRouter API key in Settings before Cadence can bind a project runtime session.',
          messagesUnavailableReason:
            'Configure an OpenRouter API key in Settings before Cadence can establish a runtime session for this imported project.',
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(
      screen.getByText('Open Settings to choose a provider and model before using the agent tab for this imported project.'),
    ).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Configure' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Configure an OpenRouter API key in Settings to start.',
    )
  })

  it('renders a first-class skill lane with truthful source, cache, and diagnostic detail', () => {
    const skillItems: RuntimeStreamView['skillItems'] = [
      {
        id: 'skill-install',
        kind: 'skill',
        runId: 'run-1',
        sequence: 7,
        createdAt: '2026-04-18T14:00:00Z',
        skillId: 'find-skills',
        stage: 'install',
        result: 'succeeded',
        detail: 'Installed autonomous skill `find-skills` from the cached vercel-labs/skills tree.',
        source: {
          repo: 'vercel-labs/skills',
          path: 'skills/find-skills',
          reference: 'main',
          treeHash: '0123456789abcdef0123456789abcdef01234567',
        },
        cacheStatus: 'refreshed',
        diagnostic: null,
      },
      {
        id: 'skill-invoke',
        kind: 'skill',
        runId: 'run-1',
        sequence: 8,
        createdAt: '2026-04-18T14:00:02Z',
        skillId: 'react-best-practices',
        stage: 'invoke',
        result: 'failed',
        detail: 'Autonomous skill `react-best-practices` failed during invocation.',
        source: {
          repo: 'vercel-labs/skills',
          path: 'skills/react-best-practices',
          reference: 'main',
          treeHash: 'fedcba98765432100123456789abcdef01234567',
        },
        cacheStatus: 'hit',
        diagnostic: {
          code: 'autonomous_skill_invoke_failed',
          message: 'Cadence could not invoke autonomous skill `react-best-practices`.',
          retryable: false,
        },
      },
    ]

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', phase: 'authenticated', phaseLabel: 'Authenticated' }),
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({
            status: 'live',
            items: skillItems,
            skillItems,
            lastSequence: 8,
          }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
          skillItems,
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Skill lane' })).toBeVisible()
    expect(screen.getByText('find-skills')).toBeVisible()
    expect(screen.getByText('react-best-practices')).toBeVisible()
    expect(screen.getByText('Install')).toBeVisible()
    expect(screen.getByText('Invoke')).toBeVisible()
    expect(screen.getByText('Cache refreshed')).toBeVisible()
    expect(screen.getByText('Cache hit')).toBeVisible()
    expect(screen.getByText('vercel-labs/skills · skills/find-skills @ main')).toBeVisible()
    expect(screen.getByText('tree 0123456789ab')).toBeVisible()
    expect(screen.getByText('Cadence could not invoke autonomous skill `react-best-practices`.')).toBeVisible()
    expect(screen.getByText('code: autonomous_skill_invoke_failed · terminal')).toBeVisible()
  })

  it('renders the empty skill lane state when a run has no skill lifecycle rows yet', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', phase: 'authenticated', phaseLabel: 'Authenticated' }),
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({
            status: 'live',
            items: [
              {
                id: 'transcript-1',
                kind: 'transcript',
                runId: 'run-1',
                sequence: 1,
                createdAt: '2026-04-18T14:10:00Z',
                text: 'Connected to Cadence.',
              },
            ],
            transcriptItems: [
              {
                id: 'transcript-1',
                kind: 'transcript',
                runId: 'run-1',
                sequence: 1,
                createdAt: '2026-04-18T14:10:00Z',
                text: 'Connected to Cadence.',
              },
            ],
            skillItems: [],
            lastSequence: 1,
          }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
          skillItems: [],
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Skill lane' })).toBeVisible()
    expect(screen.getByText('No skill activity yet')).toBeVisible()
    expect(
      screen.getByText('Cadence has not observed any skill discovery, install, or invoke lifecycle rows for this run yet.'),
    ).toBeVisible()
  })

  it('keeps recent run replacement and live-feed issue diagnostics visible while the new stream catches up', async () => {
    const replacementIssue = {
      code: 'runtime_stream_subscribe_failed',
      message: 'Cadence could not subscribe to the replacement run stream yet.',
      retryable: true,
      observedAt: '2026-04-16T20:04:10Z',
    }

    const { rerender } = render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', phase: 'authenticated', phaseLabel: 'Authenticated' }),
          runtimeRun: makeRuntimeRun({ runId: 'run-1' }),
          runtimeStream: makeRuntimeStream({
            status: 'error',
            runId: 'run-1',
            lastIssue: replacementIssue,
          }),
          runtimeStreamStatus: 'error',
          runtimeStreamStatusLabel: 'Live feed failed',
          runtimeStreamError: replacementIssue,
          messagesUnavailableReason: 'Cadence is waiting for the replacement stream to recover before new live rows can appear.',
        })}
      />,
    )

    rerender(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', phase: 'authenticated', phaseLabel: 'Authenticated' }),
          runtimeRun: makeRuntimeRun({ runId: 'run-2' }),
          runtimeStream: makeRuntimeStream({
            status: 'error',
            runId: 'run-1',
            lastIssue: replacementIssue,
          }),
          runtimeStreamStatus: 'error',
          runtimeStreamStatusLabel: 'Live feed failed',
          runtimeStreamError: replacementIssue,
          messagesUnavailableReason: 'Cadence is waiting for the replacement stream to recover before new live rows can appear.',
        })}
      />,
    )

    await waitFor(() => expect(screen.getByText('Switched to a new supervised run')).toBeVisible())
    expect(screen.getByText('run-1 → run-2')).toBeVisible()
    expect(screen.getByText('Live feed issue')).toBeVisible()
    expect(screen.getByText('Cadence could not subscribe to the replacement run stream yet.')).toBeVisible()
    expect(screen.getByText('code: runtime_stream_subscribe_failed')).toBeVisible()
  })

  it('renders a centered agent runtime setup state and opens settings', () => {
    const onOpenSettings = vi.fn()

    render(<AgentRuntime agent={makeAgent()} onOpenSettings={onOpenSettings} />)

    const composer = screen.getByLabelText('Agent input unavailable')
    const modelSelector = screen.getByRole('combobox', { name: 'Model selector' })
    const thinkingLevelSelector = screen.getByRole('combobox', { name: 'Thinking level selector' })

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(
      screen.getByText('Open Settings to choose a provider and model before using the agent tab for this imported project.'),
    ).toBeVisible()
    expect(composer).toHaveAttribute('placeholder', 'Connect a provider to start.')
    expect(composer).toHaveAttribute('rows', '3')
    expect(modelSelector).toHaveTextContent('openai_codex')
    expect(thinkingLevelSelector).toHaveTextContent('Thinking unavailable')
    expect(screen.getByText('Catalog unavailable')).toBeVisible()
    expect(screen.getByText(/only configured model truth remains visible/i)).toBeVisible()
    expect(screen.getByRole('button', { name: 'Configure' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Send message unavailable' })).toBeDisabled()
    expect(screen.queryByText('Context')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Configure' }))
    expect(onOpenSettings).toHaveBeenCalledTimes(1)
  })

  it('renders discovered model groups from the active provider catalog instead of sample lists', async () => {
    const liveCatalog = makeProviderModelCatalog({
      profileId: 'openrouter-work',
      profileLabel: 'OpenRouter Work',
      providerId: 'openrouter',
      providerLabel: 'OpenRouter',
      source: 'live',
      loadStatus: 'ready',
      state: 'live',
      stateLabel: 'Live catalog',
      detail: 'Showing 3 discovered models for OpenRouter Work.',
      fetchedAt: '2026-04-20T12:00:00Z',
      lastSuccessAt: '2026-04-20T12:00:00Z',
      models: [
        makeAgentModel({
          modelId: 'openai/gpt-5-mini',
          label: 'openai/gpt-5-mini',
          displayName: 'openai/gpt-5-mini',
          groupId: 'openai',
          groupLabel: 'OpenAI',
          availability: 'available',
          availabilityLabel: 'Available',
          thinkingSupported: true,
          thinkingEffortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
          defaultThinkingEffort: 'high',
        }),
        makeAgentModel({
          modelId: 'anthropic/claude-3.5-haiku',
          label: 'anthropic/claude-3.5-haiku',
          displayName: 'anthropic/claude-3.5-haiku',
          groupId: 'anthropic',
          groupLabel: 'Anthropic',
          availability: 'available',
          availabilityLabel: 'Available',
          thinkingSupported: true,
          thinkingEffortOptions: ['low'],
          defaultThinkingEffort: 'low',
        }),
        makeAgentModel({
          modelId: 'mistral/devstral-medium',
          label: 'mistral/devstral-medium',
          displayName: 'mistral/devstral-medium',
          groupId: 'mistral',
          groupLabel: 'Mistral',
          availability: 'available',
          availabilityLabel: 'Available',
          thinkingSupported: false,
          thinkingEffortOptions: [],
          defaultThinkingEffort: null,
        }),
      ],
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'openrouter',
          selectedProviderLabel: 'OpenRouter',
          selectedProfileId: 'openrouter-work',
          selectedProfileLabel: 'OpenRouter Work',
          selectedModelId: 'openai/gpt-5-mini',
          providerModelCatalog: liveCatalog,
          openrouterApiKeyConfigured: true,
        })}
      />,
    )

    const modelSelector = screen.getByRole('combobox', { name: 'Model selector' })

    expect(modelSelector).toHaveTextContent('openai/gpt-5-mini')
    expect(screen.getByText('Live catalog')).toBeVisible()
    expect(
      screen.getByText((content) => content.includes('Showing 3 discovered models for OpenRouter Work.')),
    ).toBeVisible()

    fireEvent.keyDown(modelSelector, { key: 'ArrowDown' })

    expect(await screen.findByText('OpenAI')).toBeVisible()
    expect(screen.getByText('Anthropic')).toBeVisible()
    expect(screen.getByText('Mistral')).toBeVisible()
    expect(screen.getByRole('option', { name: 'anthropic/claude-3.5-haiku' })).toBeVisible()
    expect(screen.getByRole('option', { name: 'mistral/devstral-medium' })).toBeVisible()
  })

  it('clamps thinking to the selected model capabilities and disables it when a model exposes none', async () => {
    const liveCatalog = makeProviderModelCatalog({
      profileId: 'openrouter-work',
      profileLabel: 'OpenRouter Work',
      providerId: 'openrouter',
      providerLabel: 'OpenRouter',
      source: 'live',
      loadStatus: 'ready',
      state: 'live',
      stateLabel: 'Live catalog',
      detail: 'Showing 3 discovered models for OpenRouter Work.',
      fetchedAt: '2026-04-20T12:00:00Z',
      lastSuccessAt: '2026-04-20T12:00:00Z',
      models: [
        makeAgentModel({
          modelId: 'openai/gpt-5-mini',
          label: 'openai/gpt-5-mini',
          displayName: 'openai/gpt-5-mini',
          groupId: 'openai',
          groupLabel: 'OpenAI',
          availability: 'available',
          availabilityLabel: 'Available',
          thinkingSupported: true,
          thinkingEffortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
          defaultThinkingEffort: 'high',
        }),
        makeAgentModel({
          modelId: 'anthropic/claude-3.5-haiku',
          label: 'anthropic/claude-3.5-haiku',
          displayName: 'anthropic/claude-3.5-haiku',
          groupId: 'anthropic',
          groupLabel: 'Anthropic',
          availability: 'available',
          availabilityLabel: 'Available',
          thinkingSupported: true,
          thinkingEffortOptions: ['low'],
          defaultThinkingEffort: 'low',
        }),
        makeAgentModel({
          modelId: 'mistral/devstral-medium',
          label: 'mistral/devstral-medium',
          displayName: 'mistral/devstral-medium',
          groupId: 'mistral',
          groupLabel: 'Mistral',
          availability: 'available',
          availabilityLabel: 'Available',
          thinkingSupported: false,
          thinkingEffortOptions: [],
          defaultThinkingEffort: null,
        }),
      ],
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'openrouter',
          selectedProviderLabel: 'OpenRouter',
          selectedProfileId: 'openrouter-work',
          selectedProfileLabel: 'OpenRouter Work',
          selectedModelId: 'openai/gpt-5-mini',
          providerModelCatalog: liveCatalog,
          openrouterApiKeyConfigured: true,
        })}
      />,
    )

    const modelSelector = screen.getByRole('combobox', { name: 'Model selector' })
    const thinkingLevelSelector = screen.getByRole('combobox', { name: 'Thinking level selector' })

    expect(thinkingLevelSelector).toHaveTextContent('Thinking · high')

    fireEvent.keyDown(thinkingLevelSelector, { key: 'ArrowDown' })
    expect(await screen.findByRole('option', { name: 'Thinking · very high' })).toBeVisible()
    fireEvent.click(screen.getByRole('option', { name: 'Thinking · very high' }))
    await waitFor(() => expect(thinkingLevelSelector).toHaveTextContent('Thinking · very high'))

    fireEvent.keyDown(modelSelector, { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'anthropic/claude-3.5-haiku' }))
    await waitFor(() => expect(thinkingLevelSelector).toHaveTextContent('Thinking · low'))
    expect(screen.getByText('Thinking supports Low. Default: Low.')).toBeVisible()

    fireEvent.keyDown(modelSelector, { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'mistral/devstral-medium' }))
    await waitFor(() => expect(thinkingLevelSelector).toHaveTextContent('Thinking unavailable'))
    expect(thinkingLevelSelector).toBeDisabled()
    expect(
      screen.getByText('mistral/devstral-medium does not expose configurable thinking for this provider catalog.'),
    ).toBeVisible()
  })
})
