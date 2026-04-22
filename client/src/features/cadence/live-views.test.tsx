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

import { AgentRuntime } from '@/components/cadence/agent-runtime'
import { ExecutionView } from '@/components/cadence/execution-view'
import { PhaseView } from '@/components/cadence/phase-view'
import type {
  AgentPaneView,
  ExecutionPaneView,
  WorkflowPaneView,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type { AgentProviderModelCatalogView } from '@/src/features/cadence/use-cadence-desktop-state/types'
import type {
  PlanningLifecycleView,
  ProjectDetailView,
  ProviderModelThinkingEffortDto,
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

function makeWorkflow(project = makeProject(), overrides: Partial<WorkflowPaneView> = {}): WorkflowPaneView {
  const lifecycle = project.lifecycle ?? makeLifecycle()

  return {
    project,
    activePhase: project.phases.find((phase) => phase.status === 'active') ?? null,
    lifecycle,
    activeLifecycleStage: lifecycle.activeStage,
    lifecyclePercent: lifecycle.percentComplete,
    hasLifecycle: lifecycle.hasStages,
    actionRequiredLifecycleCount: lifecycle.actionRequiredCount,
    overallPercent: project.phaseProgressPercent,
    hasPhases: project.phases.length > 0,
    runtimeSession: overrides.runtimeSession ?? project.runtimeSession ?? null,
    selectedProviderId: overrides.selectedProviderId ?? 'openai_codex',
    selectedProviderLabel: overrides.selectedProviderLabel ?? 'OpenAI Codex',
    selectedModelId: overrides.selectedModelId ?? 'openai_codex',
    openrouterApiKeyConfigured: overrides.openrouterApiKeyConfigured ?? false,
    providerMismatch: overrides.providerMismatch ?? false,
    ...overrides,
  }
}

function makeExecution(project = makeProject(), overrides: Partial<ExecutionPaneView> = {}): ExecutionPaneView {
  return {
    project,
    activePhase: project.phases.find((phase) => phase.status === 'active') ?? null,
    branchLabel: project.branchLabel,
    headShaLabel: project.repository?.headShaLabel ?? 'No HEAD',
    statusEntries: [],
    statusCount: 0,
    hasChanges: false,
    diffScopes: [],
    verificationRecords: project.verificationRecords,
    resumeHistory: project.resumeHistory,
    latestDecisionOutcome: project.latestDecisionOutcome,
    notificationBroker: project.notificationBroker,
    operatorActionError: null,
    verificationUnavailableReason: 'Verification details will appear here once the backend exposes run and wave results.',
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

function makeRuntimeRun(overrides: Partial<RuntimeRunView> = {}): RuntimeRunView {
  return {
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
    controls: {
      active: {
        modelId: 'openai_codex',
        thinkingEffort: 'medium',
        thinkingEffortLabel: 'Medium',
        approvalMode: 'suggest',
        approvalModeLabel: 'Suggest',
        revision: 1,
        appliedAt: '2026-04-15T20:00:00Z',
      },
      pending: null,
      selected: {
        source: 'active',
        modelId: 'openai_codex',
        thinkingEffort: 'medium',
        thinkingEffortLabel: 'Medium',
        approvalMode: 'suggest',
        approvalModeLabel: 'Suggest',
        revision: 1,
        effectiveAt: '2026-04-15T20:00:00Z',
        queuedPrompt: null,
        queuedPromptAt: null,
        hasQueuedPrompt: false,
      },
      hasPendingControls: false,
    },
    startedAt: '2026-04-15T20:00:00Z',
    lastHeartbeatAt: '2026-04-15T20:00:05Z',
    lastCheckpointSequence: 2,
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
      {
        sequence: 2,
        kind: 'state',
        kindLabel: 'State',
        summary: 'Recovered repository context before reconnecting the live feed.',
        createdAt: '2026-04-15T20:00:06Z',
      },
    ],
    latestCheckpoint: {
      sequence: 2,
      kind: 'state',
      kindLabel: 'State',
      summary: 'Recovered repository context before reconnecting the live feed.',
      createdAt: '2026-04-15T20:00:06Z',
    },
    checkpointCount: 2,
    hasCheckpoints: true,
    isActive: true,
    isTerminal: false,
    isStale: false,
    isFailed: false,
    ...overrides,
  }
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
    pausedAt: null,
    cancelledAt: null,
    completedAt: null,
    crashedAt: '2026-04-16T20:03:00Z',
    stoppedAt: null,
    pauseReason: null,
    cancelReason: null,
    crashReason: {
      code: 'runtime_supervisor_connect_failed',
      message: 'Cadence restored the same autonomous run after reload without starting a duplicate continuation.',
    },
    lastErrorCode: 'runtime_supervisor_connect_failed',
    lastError: {
      code: 'runtime_supervisor_connect_failed',
      message: 'Cadence restored the same autonomous run after reload without starting a duplicate continuation.',
    },
    updatedAt: '2026-04-16T20:03:00Z',
    isActive: true,
    needsRecovery: true,
    isTerminal: false,
    isFailed: true,
    ...overrides,
  }
}

function makeAutonomousUnit(
  overrides: Partial<NonNullable<ProjectDetailView['autonomousUnit']>> = {},
): NonNullable<ProjectDetailView['autonomousUnit']> {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    unitId: 'auto-run-1:checkpoint:2',
    sequence: 2,
    kind: 'state' as const,
    kindLabel: 'State',
    status: 'active' as const,
    statusLabel: 'Active',
    summary: 'Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.',
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

function getStreamStatusLabel(status: RuntimeStreamView['status']): string {
  switch (status) {
    case 'idle':
      return 'No live stream'
    case 'subscribing':
      return 'Connecting stream'
    case 'replaying':
      return 'Replaying recent activity'
    case 'live':
      return 'Streaming live activity'
    case 'complete':
      return 'Stream complete'
    case 'stale':
      return 'Stream stale'
    case 'error':
      return 'Stream failed'
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

function makeProviderModelCatalog(): AgentProviderModelCatalogView {
  const thinkingEffortOptions: ProviderModelThinkingEffortDto[] = ['low', 'medium', 'high']

  return {
    profileId: 'openai_codex-default',
    profileLabel: 'OpenAI Codex',
    providerId: 'openai_codex',
    providerLabel: 'OpenAI Codex',
    source: 'live',
    loadStatus: 'ready',
    state: 'live',
    stateLabel: 'Live catalog',
    detail: 'OpenAI Codex is ready for this imported project.',
    fetchedAt: '2026-04-15T20:00:00Z',
    lastSuccessAt: '2026-04-15T20:00:00Z',
    lastRefreshError: null,
    models: [
      {
        modelId: 'openai_codex',
        label: 'OpenAI Codex',
        displayName: 'OpenAI Codex',
        groupId: 'openai',
        groupLabel: 'OpenAI',
        availability: 'available',
        availabilityLabel: 'Available',
        thinkingSupported: true,
        thinkingEffortOptions,
        defaultThinkingEffort: 'medium',
      },
    ],
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

function makeAgent(project = makeProject(), overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  const runtimeSession = overrides.runtimeSession ?? null
  const runtimeRun = overrides.runtimeRun ?? project.runtimeRun ?? null
  const runtimeStream = overrides.runtimeStream ?? null
  const runtimeStreamStatus = overrides.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const runtimeRunControls = runtimeRun?.controls ?? null
  const selectedControls = runtimeRunControls?.selected ?? null
  const providerModelCatalog = overrides.providerModelCatalog ?? makeProviderModelCatalog()
  const selectedModelOption = overrides.selectedModelOption ?? providerModelCatalog.models[0] ?? null

  return {
    project,
    activePhase: null,
    branchLabel: project.branchLabel,
    headShaLabel: project.repository?.headShaLabel ?? 'No HEAD',
    runtimeLabel: runtimeSession?.runtimeLabel ?? project.runtimeLabel,
    repositoryLabel: project.repository?.displayName ?? project.name,
    repositoryPath: project.repository?.rootPath ?? null,
    runtimeSession,
    selectedProfileId: overrides.selectedProfileId ?? providerModelCatalog.profileId,
    selectedProfileLabel: overrides.selectedProfileLabel ?? providerModelCatalog.profileLabel,
    selectedProviderId: overrides.selectedProviderId ?? providerModelCatalog.providerId,
    selectedProviderLabel: overrides.selectedProviderLabel ?? providerModelCatalog.providerLabel,
    selectedProviderSource: overrides.selectedProviderSource ?? 'provider_profiles',
    controlTruthSource:
      overrides.controlTruthSource ?? (selectedControls && !runtimeRun?.isTerminal ? 'runtime_run' : 'fallback'),
    selectedModelId: overrides.selectedModelId ?? selectedControls?.modelId ?? selectedModelOption?.modelId ?? null,
    selectedThinkingEffort:
      overrides.selectedThinkingEffort ??
      selectedControls?.thinkingEffort ??
      selectedModelOption?.defaultThinkingEffort ??
      null,
    selectedApprovalMode: overrides.selectedApprovalMode ?? selectedControls?.approvalMode ?? 'suggest',
    selectedPrompt: overrides.selectedPrompt ?? {
      text: selectedControls?.queuedPrompt ?? null,
      queuedAt: selectedControls?.queuedPromptAt ?? null,
      hasQueuedPrompt: selectedControls?.hasQueuedPrompt ?? false,
    },
    runtimeRunActiveControls: overrides.runtimeRunActiveControls ?? runtimeRunControls?.active ?? null,
    runtimeRunPendingControls: overrides.runtimeRunPendingControls ?? runtimeRunControls?.pending ?? null,
    providerModelCatalog,
    selectedModelOption,
    selectedModelThinkingEffortOptions:
      overrides.selectedModelThinkingEffortOptions ?? selectedModelOption?.thinkingEffortOptions ?? [],
    selectedModelDefaultThinkingEffort:
      overrides.selectedModelDefaultThinkingEffort ?? selectedModelOption?.defaultThinkingEffort ?? null,
    runtimeRun,
    runtimeErrorMessage: null,
    runtimeRunErrorMessage: null,
    autonomousRunErrorMessage: null,
    autonomousRun: overrides.autonomousRun ?? project.autonomousRun ?? null,
    autonomousUnit: overrides.autonomousUnit ?? project.autonomousUnit ?? null,
    autonomousAttempt: overrides.autonomousAttempt ?? project.autonomousAttempt ?? null,
    autonomousHistory: overrides.autonomousHistory ?? project.autonomousHistory,
    autonomousRecentArtifacts: overrides.autonomousRecentArtifacts ?? project.autonomousRecentArtifacts,
    authPhase: runtimeSession?.phase ?? null,
    authPhaseLabel: runtimeSession?.phaseLabel ?? 'Signed out',
    runtimeStream,
    runtimeStreamStatus,
    runtimeStreamStatusLabel: overrides.runtimeStreamStatusLabel ?? getStreamStatusLabel(runtimeStreamStatus),
    runtimeStreamError: overrides.runtimeStreamError ?? runtimeStream?.lastIssue ?? null,
    runtimeStreamItems: overrides.runtimeStreamItems ?? runtimeStream?.items ?? [],
    skillItems: overrides.skillItems ?? runtimeStream?.skillItems ?? [],
    activityItems: overrides.activityItems ?? runtimeStream?.activityItems ?? [],
    actionRequiredItems: overrides.actionRequiredItems ?? runtimeStream?.actionRequired ?? [],
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
    notificationRoutes: overrides.notificationRoutes ?? [],
    notificationRouteLoadStatus: overrides.notificationRouteLoadStatus ?? 'idle',
    notificationRouteError: overrides.notificationRouteError ?? null,
    notificationRouteMutationStatus: overrides.notificationRouteMutationStatus ?? 'idle',
    pendingNotificationRouteId: overrides.pendingNotificationRouteId ?? null,
    notificationRouteMutationError: overrides.notificationRouteMutationError ?? null,
    notificationChannelHealth: overrides.notificationChannelHealth ?? [],
    notificationSyncSummary: overrides.notificationSyncSummary ?? null,
    notificationSyncError: overrides.notificationSyncError ?? null,
    notificationRouteIsRefreshing: overrides.notificationRouteIsRefreshing ?? false,
    notificationSyncPollingActive: overrides.notificationSyncPollingActive ?? false,
    notificationSyncPollingActionId: overrides.notificationSyncPollingActionId ?? null,
    notificationSyncPollingBoundaryId: overrides.notificationSyncPollingBoundaryId ?? null,
    trustSnapshot: overrides.trustSnapshot ?? undefined,
    sessionUnavailableReason:
      runtimeSession?.lastError?.message ??
      'Sign in with OpenAI to create or reuse a runtime session for this imported project.',
    runtimeRunUnavailableReason:
      overrides.runtimeRunUnavailableReason ??
      (runtimeRun
        ? 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.'
        : 'Authenticate and launch a supervised harness run to populate durable repo-local run state for this project.'),
    messagesUnavailableReason:
      overrides.messagesUnavailableReason ??
      (runtimeSession?.isAuthenticated
        ? 'Cadence authenticated this project, but the live runtime stream has not started yet.'
        : 'Sign in with OpenAI to establish a runtime session for this imported project.'),
    ...overrides,
  }
}

describe('live views', () => {
  it('renders workflow runtime setup state when runtime is not configured', () => {
    const onOpenSettings = vi.fn()

    render(<PhaseView onOpenSettings={onOpenSettings} workflow={makeWorkflow()} />)

    expect(screen.getByText('Milestone')).toBeVisible()
    expect(screen.getByText('M001')).toBeVisible()
    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(
      screen.getByText('Open Settings to choose a provider and model before using the workflow tab for this imported project.'),
    ).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Configure' }))
    expect(onOpenSettings).toHaveBeenCalledTimes(1)
  })

  it('renders the milestone empty state once runtime is configured', () => {
    render(
      <PhaseView
        workflow={makeWorkflow(makeProject(), {
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            accountLabel: 'acct@example.com',
            sessionLabel: 'session-1',
            lastErrorCode: null,
            lastError: null,
            isAuthenticated: true,
            isSignedOut: false,
          }),
        })}
      />,
    )

    expect(screen.getByText('No milestone assigned')).toBeVisible()
    expect(
      screen.getByText('Assign a milestone to this project to start tracking planning lifecycle stages.'),
    ).toBeVisible()
  })

  it('renders lifecycle cards for the current workflow UI', () => {
    const discussionStage = {
      stage: 'discussion' as const,
      stageLabel: 'Discussion',
      nodeId: 'workflow-discussion',
      nodeLabel: 'Workflow Discussion',
      status: 'complete' as const,
      statusLabel: 'Complete',
      actionRequired: false,
      lastTransitionAt: '2026-04-15T17:59:00Z',
    }
    const researchStage = {
      stage: 'research' as const,
      stageLabel: 'Research',
      nodeId: 'workflow-research',
      nodeLabel: 'Workflow Research',
      status: 'active' as const,
      statusLabel: 'Active',
      actionRequired: false,
      lastTransitionAt: '2026-04-15T18:00:00Z',
    }
    const requirementsStage = {
      stage: 'requirements' as const,
      stageLabel: 'Requirements',
      nodeId: 'workflow-requirements',
      nodeLabel: 'Workflow Requirements',
      status: 'blocked' as const,
      statusLabel: 'Blocked',
      actionRequired: true,
      lastTransitionAt: '2026-04-15T18:01:00Z',
    }
    const roadmapStage = {
      stage: 'roadmap' as const,
      stageLabel: 'Roadmap',
      nodeId: 'workflow-roadmap',
      nodeLabel: 'Workflow Roadmap',
      status: 'pending' as const,
      statusLabel: 'Pending',
      actionRequired: false,
      lastTransitionAt: null,
    }

    render(
      <PhaseView
        workflow={makeWorkflow(
          makeProject({
            lifecycle: makeLifecycle({
              stages: [discussionStage, researchStage, requirementsStage, roadmapStage],
              byStage: {
                discussion: discussionStage,
                research: researchStage,
                requirements: requirementsStage,
                roadmap: roadmapStage,
              },
              hasStages: true,
              activeStage: researchStage,
              actionRequiredCount: 1,
              blockedCount: 1,
              completedCount: 1,
              percentComplete: 25,
            }),
          }),
        )}
      />,
    )

    expect(screen.getByText('Planning lifecycle')).toBeVisible()
    expect(screen.getByText('Research active')).toBeVisible()
    expect(screen.getByText('25%')).toBeVisible()
    expect(screen.getByText('1/4 stages')).toBeVisible()
    expect(screen.getByText('Discussion')).toBeVisible()
    expect(screen.getByText('Research')).toBeVisible()
    expect(screen.getByText('Requirements')).toBeVisible()
    expect(screen.getByText('Roadmap')).toBeVisible()
    expect(screen.getByText('Action required')).toBeVisible()
  })

  it('renders the current signed-out agent shell truthfully', () => {
    render(<AgentRuntime agent={makeAgent()} />)

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(
      screen.getByText('Open Settings to choose a provider and model before using the agent tab for this imported project.'),
    ).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute('placeholder', 'Connect a provider to start.')
    expect(screen.queryByText('Context')).not.toBeInTheDocument()
    expect(screen.queryByText('Signed out')).not.toBeInTheDocument()
  })

  it('renders the authenticated no-run agent state and can start a run', async () => {
    const onStartRuntimeRun = vi.fn(async () => null)

    render(
      <AgentRuntime
        agent={makeAgent(makeProject(), {
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            accountId: 'acct@example.com',
            accountLabel: 'acct@example.com',
            sessionId: 'session-1',
            sessionLabel: 'session-1',
            lastErrorCode: null,
            lastError: null,
            isAuthenticated: true,
            isLoginInProgress: false,
            needsManualInput: false,
            isSignedOut: false,
            isFailed: false,
          }),
        })}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.getByText('No supervised run is attached')).toBeVisible()
    expect(screen.getByText('No transcript yet')).toBeVisible()
    expect(screen.getByText('No runtime activity yet')).toBeVisible()
    expect(screen.getByText('No tool calls yet')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Start run' })).toBeVisible()
    expect(screen.queryByLabelText('Agent input unavailable')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Start run' }))
    await waitFor(() => expect(onStartRuntimeRun).toHaveBeenCalledTimes(1))
  })

  it('renders projected skill lifecycle rows in the live agent feed', () => {
    const skillItems: RuntimeStreamView['skillItems'] = [
      {
        id: 'skill-discovery-1',
        kind: 'skill',
        runId: 'run-1',
        sequence: 3,
        createdAt: '2026-04-18T15:00:00Z',
        skillId: 'find-skills',
        stage: 'discovery',
        result: 'succeeded',
        detail: 'Discovery completed for the requested skill set.',
        source: {
          repo: 'vercel-labs/skills',
          path: 'skills/find-skills',
          reference: 'main',
          treeHash: '0123456789abcdef0123456789abcdef01234567',
        },
        cacheStatus: 'miss',
        diagnostic: null,
      },
    ]

    render(
      <AgentRuntime
        agent={makeAgent(makeProject(), {
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            accountId: 'acct@example.com',
            accountLabel: 'acct@example.com',
            sessionId: 'session-1',
            sessionLabel: 'session-1',
            lastErrorCode: null,
            lastError: null,
            isAuthenticated: true,
            isLoginInProgress: false,
            needsManualInput: false,
            isSignedOut: false,
            isFailed: false,
          }),
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({
            status: 'live',
            items: skillItems,
            skillItems,
            lastSequence: 3,
          }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: getStreamStatusLabel('live'),
          skillItems,
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Skill lane' })).toBeVisible()
    expect(screen.getByText('find-skills')).toBeVisible()
    expect(screen.getByText('Discovery')).toBeVisible()
    expect(screen.getByText('Cache miss')).toBeVisible()
  })

  it('renders recovered runtime without the removed autonomous and remote debug panels', () => {
    const autonomousRun = makeAutonomousRun({
      duplicateStartDetected: true,
      duplicateStartRunId: 'auto-run-1',
      duplicateStartReason:
        'Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor.',
    })
    const autonomousUnit = makeAutonomousUnit()

    render(
      <AgentRuntime
        agent={makeAgent(
          makeProject({
            autonomousRun,
            autonomousUnit,
            runtimeRun: makeRuntimeRun({
              status: 'stale',
              statusLabel: 'Supervisor stale',
              transport: {
                kind: 'tcp',
                endpoint: '127.0.0.1:4455',
                liveness: 'unreachable',
                livenessLabel: 'Control unreachable',
              },
              lastErrorCode: 'runtime_supervisor_connect_failed',
              lastError: {
                code: 'runtime_supervisor_connect_failed',
                message: 'Cadence restored the same autonomous run after reload without starting a duplicate continuation.',
              },
              isActive: false,
              isStale: true,
            }),
          }),
          {
            runtimeSession: makeRuntimeSession({
              phase: 'authenticated',
              phaseLabel: 'Authenticated',
              runtimeLabel: 'Openai Codex · Authenticated',
              accountId: 'acct@example.com',
              accountLabel: 'acct@example.com',
              sessionId: 'session-1',
              sessionLabel: 'session-1',
              lastErrorCode: null,
              lastError: null,
              isAuthenticated: true,
              isLoginInProgress: false,
              needsManualInput: false,
              isSignedOut: false,
              isFailed: false,
            }),
            autonomousRun,
            autonomousUnit,
            runtimeRun: makeRuntimeRun({
              status: 'stale',
              statusLabel: 'Supervisor stale',
              transport: {
                kind: 'tcp',
                endpoint: '127.0.0.1:4455',
                liveness: 'unreachable',
                livenessLabel: 'Control unreachable',
              },
              lastErrorCode: 'runtime_supervisor_connect_failed',
              lastError: {
                code: 'runtime_supervisor_connect_failed',
                message: 'Cadence restored the same autonomous run after reload without starting a duplicate continuation.',
              },
              isActive: false,
              isStale: true,
            }),
            runtimeStream: makeRuntimeStream({ status: 'idle' }),
            runtimeRunUnavailableReason:
              'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
            messagesUnavailableReason:
              'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.',
          },
        )}
      />,
    )

    expect(screen.getAllByRole('heading', { name: 'Recovered run snapshot' }).length).toBeGreaterThanOrEqual(1)
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByText('Current autonomous boundary')).not.toBeInTheDocument()
    expect(
      screen.queryByText('Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.'),
    ).not.toBeInTheDocument()
    expect(screen.queryByText('Duplicate start prevented')).not.toBeInTheDocument()
    expect(screen.queryByText('run: auto-run-1')).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
  })

  it('renders recovered runtime and checkpoint control-loop actions with the current headings', async () => {
    const resolveOperatorAction = vi.fn(async () => undefined)
    const resumeOperatorRun = vi.fn(async () => undefined)

    const pendingApproval = {
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
      status: 'pending' as const,
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
      answerRequirementReason: 'gate_linked' as const,
      answerRequirementLabel: 'Required',
      answerShapeKind: 'plain_text' as const,
      answerShapeLabel: 'Required user answer',
      answerShapeHint: 'Describe the operator decision that justifies approval.',
      answerPlaceholder: 'Provide operator input for this action.',
    }
    const approvedCard = makeCheckpointControlLoopCard({
      actionId: 'action-approved',
      key: 'action-approved::boundary-2',
      boundaryId: 'boundary-2',
      title: 'Resume after plan review',
      detail: 'Retry resume after the operator confirms the plan is safe.',
      approval: {
        actionId: 'action-approved',
        sessionId: 'session-1',
        flowId: 'flow-1',
        actionType: 'review_plan',
        title: 'Resume after plan review',
        detail: 'Retry resume after the operator confirms the plan is safe.',
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
      latestResume: {
        id: 2,
        sourceActionId: 'action-approved',
        sessionId: 'session-1',
        status: 'started',
        statusLabel: 'Resume started',
        summary: 'Operator resumed the selected project runtime session.',
        createdAt: '2026-04-13T20:04:00Z',
      },
      resumeStateLabel: 'Resume started',
      resumeDetail: 'Operator resumed the selected project runtime session.',
      resumeUpdatedAt: '2026-04-13T20:04:00Z',
    })

    const project = makeProject({
      approvalRequests: [pendingApproval, approvedCard.approval!],
      pendingApprovalCount: 1,
      resumeHistory: [approvedCard.latestResume!],
    })

    render(
      <AgentRuntime
        agent={makeAgent(project, {
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            accountId: 'acct@example.com',
            accountLabel: 'acct@example.com',
            sessionId: 'session-1',
            sessionLabel: 'session-1',
            lastErrorCode: null,
            lastError: null,
            isAuthenticated: true,
            isLoginInProgress: false,
            needsManualInput: false,
            isSignedOut: false,
            isFailed: false,
          }),
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({ status: 'idle' }),
          checkpointControlLoop: makeCheckpointControlLoop({
            items: [
              makeCheckpointControlLoopCard({
                actionId: 'action-pending',
                key: 'action-pending::boundary-1',
                boundaryId: 'boundary-1',
                title: pendingApproval.title,
                detail: pendingApproval.detail,
                approval: pendingApproval,
                durableStateLabel: pendingApproval.statusLabel,
                durableStateDetail: pendingApproval.detail,
                durableUpdatedAt: pendingApproval.updatedAt,
                latestResume: null,
                resumeStateLabel: 'Waiting on approval',
                resumeDetail: 'Cadence is waiting for operator input before this action can resume the run.',
                resumeUpdatedAt: pendingApproval.updatedAt,
                evidenceCount: 0,
                evidenceStateLabel: 'No durable evidence in bounded window',
                evidenceSummary:
                  'Cadence did not retain a matching tool result, verification row, or policy denial for this action in the bounded evidence window.',
                latestEvidenceAt: null,
                evidencePreviews: [],
              }),
              approvedCard,
            ],
            totalCount: 2,
            visibleCount: 2,
            hiddenCount: 0,
            windowLabel: 'Showing 2 checkpoint actions from the bounded control-loop window.',
            durableOnlyCount: 2,
          }),
          runtimeRunUnavailableReason: 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
          messagesUnavailableReason: 'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.',
        })}
        onResolveOperatorAction={resolveOperatorAction}
        onResumeOperatorRun={resumeOperatorRun}
      />,
    )

    expect(screen.getAllByRole('heading', { name: 'Recovered run snapshot' }).length).toBeGreaterThanOrEqual(1)
    expect(screen.getByRole('heading', { name: 'Waiting for the first run-scoped event' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible()
    const checkpointSection = screen.getByRole('heading', { name: 'Checkpoint control loop' }).closest('section')
    expect(checkpointSection).not.toBeNull()
    const checkpointQueries = within(checkpointSection as HTMLElement)
    expect(screen.getByText('Supervisor boot recorded.')).toBeVisible()
    expect(checkpointQueries.getByText('Review worktree changes')).toBeVisible()
    expect(checkpointQueries.getByText('Resume after plan review')).toBeVisible()
    expect(
      checkpointQueries.getAllByText('Latest resume started: Operator resumed the selected project runtime session.').length,
    ).toBeGreaterThan(0)

    fireEvent.change(checkpointQueries.getByLabelText('Operator answer for action-pending'), {
      target: { value: 'Proceed after validating repo changes.' },
    })
    fireEvent.click(checkpointQueries.getByRole('button', { name: 'Approve' }))

    await waitFor(() =>
      expect(resolveOperatorAction).toHaveBeenCalledWith('action-pending', 'approve', {
        userAnswer: 'Proceed after validating repo changes.',
      }),
    )

    fireEvent.click(checkpointQueries.getByRole('button', { name: 'Resume run' }))
    await waitFor(() =>
      expect(resumeOperatorRun).toHaveBeenCalledWith('action-approved', {
        userAnswer: 'Looks good to resume.',
      }),
    )
  })

  it('renders the editor against the selected project tree', async () => {
    const readProjectFile = vi.fn(async (projectId: string, path: string) => ({
      projectId,
      path,
      content: '# Cadence\n',
    }))

    render(
      <ExecutionView
        execution={makeExecution()}
        listProjectFiles={async () => ({
          projectId: 'project-1',
          root: {
            name: 'root',
            path: '/',
            type: 'folder',
            children: [
              { name: 'README.md', path: '/README.md', type: 'file' },
              {
                name: 'src',
                path: '/src',
                type: 'folder',
                children: [{ name: 'App.tsx', path: '/src/App.tsx', type: 'file' }],
              },
            ],
          },
        })}
        readProjectFile={readProjectFile}
        writeProjectFile={async (projectId: string, path: string) => ({ projectId, path })}
        createProjectEntry={async (request) => ({
          projectId: request.projectId,
          path: request.parentPath === '/' ? `/${request.name}` : `${request.parentPath}/${request.name}`,
        })}
        renameProjectEntry={async (request) => ({
          projectId: request.projectId,
          path: request.path.split('/').slice(0, -1).filter(Boolean).length
            ? `/${request.path.split('/').slice(0, -1).filter(Boolean).join('/')}/${request.newName}`
            : `/${request.newName}`,
        })}
        deleteProjectEntry={async (projectId: string, path: string) => ({ projectId, path })}
      />,
    )

    expect(await screen.findByText('README.md')).toBeVisible()
    expect(screen.getByText('Select a file to start editing')).toBeVisible()
    fireEvent.click(screen.getByText('README.md'))
    await waitFor(() => expect(readProjectFile).toHaveBeenCalledWith('project-1', '/README.md'))
    expect(screen.queryByText('Changes')).not.toBeInTheDocument()
    expect(screen.queryByText('No unstaged changes')).not.toBeInTheDocument()
  })
})
