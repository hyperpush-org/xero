import { fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  ProjectDetailView,
  ProviderModelThinkingEffortDto,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'

type CheckpointControlLoopCard = NonNullable<AgentPaneView['checkpointControlLoop']>['items'][number]

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
    ...overrides,
  }
}

function makeWorkflow(project = makeProject(), overrides: Partial<WorkflowPaneView> = {}): WorkflowPaneView {
  return {
    project,
    activePhase: project.phases.find((phase) => phase.status === 'active') ?? null,
    overallPercent: project.phaseProgressPercent,
    hasPhases: project.phases.length > 0,
    runtimeSession: overrides.runtimeSession ?? project.runtimeSession ?? null,
    selectedProviderId: overrides.selectedProviderId ?? 'openai_codex',
    selectedProviderLabel: overrides.selectedProviderLabel ?? 'OpenAI Codex',
    selectedModelId: overrides.selectedModelId ?? 'openai_codex',
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
    agentSessionId: 'agent-session-main',
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
        providerProfileId: null,
        modelId: 'openai_codex',
        thinkingEffort: 'medium',
        thinkingEffortLabel: 'Medium',
        approvalMode: 'suggest',
        approvalModeLabel: 'Suggest',
        planModeRequired: false,
        revision: 1,
        appliedAt: '2026-04-15T20:00:00Z',
      },
      pending: null,
      selected: {
        source: 'active',
        providerProfileId: null,
        modelId: 'openai_codex',
        thinkingEffort: 'medium',
        thinkingEffortLabel: 'Medium',
        approvalMode: 'suggest',
        approvalModeLabel: 'Suggest',
        planModeRequired: false,
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
    agentSessionId: 'agent-session-main',
    runId: 'auto-run-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    runtimeLabel: 'Openai Codex · Autonomous run active',
    supervisorKind: 'detached_pty',
    status: 'running' as const,
    statusLabel: 'Autonomous run active',
    recoveryState: 'recovery_required' as const,
    recoveryLabel: 'Recovery required',
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
    isTerminal: false,
    isStale: false,
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
    agentSessionId: 'agent-session-main',
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
        selectionKey: 'openai_codex-default::openai_codex',
        profileId: 'openai_codex-default',
        profileLabel: 'OpenAI Codex',
        providerId: 'openai_codex',
        providerLabel: 'OpenAI Codex',
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
    isRuntimeResumable: true,
    requiresUserAnswer: true,
    answerRequirementReason: 'runtime_resumable' as const,
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
    resumability: 'resumable',
    resumabilityLabel: 'Resumable',
    resumabilityDetail: 'Cadence can resume this checkpoint from durable action truth.',
    isResumable: true,
    advancedFailureClass: null,
    advancedFailureClassLabel: null,
    advancedFailureDiagnosticCode: null,
    recoveryRecommendation: 'approve_resume',
    recoveryRecommendationLabel: 'Approve and resume',
    recoveryRecommendationDetail: 'Resume the run after operator approval.',
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
    selectedProviderSource: overrides.selectedProviderSource ?? 'credential_default',
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
        : 'Authenticate and launch a supervised harness run to populate durable app-data run state for this project.'),
    messagesUnavailableReason:
      overrides.messagesUnavailableReason ??
      (runtimeSession?.isAuthenticated
        ? 'Cadence authenticated this project, but the live runtime stream has not started yet.'
        : 'Sign in with OpenAI to establish a runtime session for this imported project.'),
    ...overrides,
  }
}

describe('live views', () => {
  it('renders the workflow tab as a blank slate', () => {
    const { container } = render(<PhaseView workflow={makeWorkflow()} />)

    expect(container).toBeEmptyDOMElement()
  })

  it('does not render the mock pipeline controls on the workflow tab', () => {
    render(<PhaseView workflow={makeWorkflow()} />)

    expect(screen.queryByText('Cadence Desktop')).not.toBeInTheDocument()
    expect(screen.queryByTitle(/^P11 —/)).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Open agent runtime/ })).not.toBeInTheDocument()
  })


  it('renders the promptable empty-session state when the selected provider is ready', () => {
    const onStartRuntimeSession = vi.fn(async () =>
      makeRuntimeSession({
        phase: 'authenticated',
        phaseLabel: 'Authenticated',
        sessionId: 'session-1',
        isAuthenticated: true,
        isSignedOut: false,
      }),
    )
    const onStartRuntimeRun = vi.fn(async () => makeRuntimeRun())

    render(
      <AgentRuntime
        agent={makeAgent(makeProject(), {
          runtimeSession: null,
          runtimeRun: null,
        })}
        onStartRuntimeSession={onStartRuntimeSession}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    expect(screen.queryByText('Configure agent runtime')).not.toBeInTheDocument()
    expect(screen.getByRole('heading', { name: /What can we build together in Cadence/ })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Explore the codebase' })).toBeVisible()
    expect(screen.getByLabelText('Agent input')).toHaveAttribute('placeholder', 'Send a message to start with OpenAI Codex.')
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

    expect(screen.queryByLabelText('Agent input unavailable')).not.toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Start the supervised run.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))
    await waitFor(() => expect(onStartRuntimeRun).toHaveBeenCalledTimes(1))
  })

  it('keeps the live feed visible while rendering checkpoint control-loop cards', () => {
    const pendingApproval = {
      actionId: 'action-pending',
      sessionId: 'session-1',
      flowId: 'flow-1',
      actionType: 'review_worktree',
      title: 'Review worktree changes',
      detail: 'Inspect the repository diff before trusting the next operator step.',
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
      isRuntimeResumable: true,
      requiresUserAnswer: true,
      answerRequirementReason: 'runtime_resumable' as const,
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
        isRuntimeResumable: true,
        requiresUserAnswer: true,
        answerRequirementReason: 'runtime_resumable',
        answerRequirementLabel: 'Required',
        answerShapeKind: 'plain_text',
        answerShapeLabel: 'Required user answer',
        answerShapeHint: 'Describe the operator decision that justifies approval.',
        answerPlaceholder: 'Provide operator input for this action.',
      },
    })

    const project = makeProject({
      approvalRequests: [pendingApproval, approvedCard.approval!],
      pendingApprovalCount: 1,
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
      />,
    )

    expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible()
    expect(screen.getByText('Review worktree changes')).toBeVisible()
    expect(screen.getByText('Resume after plan review')).toBeVisible()
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
        searchProject={async (request) => ({
          projectId: request.projectId,
          totalMatches: 0,
          totalFiles: 0,
          truncated: false,
          files: [],
        })}
        replaceInProject={async (request) => ({
          projectId: request.projectId,
          filesChanged: 0,
          totalReplacements: 0,
        })}
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
