import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

afterEach(() => {
  window.localStorage.clear()
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
import type { SpeechDictationAdapter } from '@/components/cadence/agent-runtime/use-speech-dictation'
import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type { DictationEventDto, DictationStatusDto } from '@/src/lib/cadence-model/dictation'
import type {
  ProjectDetailView,
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
    isTerminal: false,
    isStale: false,
    ...overrides,
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

function makeAgentModel(
  overrides: Partial<NonNullable<AgentPaneView['selectedModelOption']>> = {},
): NonNullable<AgentPaneView['selectedModelOption']> {
  return {
    selectionKey: 'unscoped::openai_codex',
    profileId: null,
    profileLabel: null,
    providerId: 'openai_codex',
    providerLabel: 'OpenAI Codex',
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
    resumabilityDetail:
      'Cadence has a durable resume path for this action using the existing approve/reject/resume controls.',
    isResumable: true,
    advancedFailureClass: null,
    advancedFailureClassLabel: null,
    advancedFailureDiagnosticCode: null,
    recoveryRecommendation: 'approve_resume',
    recoveryRecommendationLabel: 'Approve / resume',
    recoveryRecommendationDetail:
      'Use the existing approve/reject/resume controls for this action. Cadence will refresh durable truth after the decision is persisted.',
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
    controlTruthSource: overrides.controlTruthSource ?? (runtimeRun ? 'runtime_run' : 'fallback'),
    selectedModelId,
    selectedThinkingEffort: overrides.selectedThinkingEffort ?? selectedModelOption?.defaultThinkingEffort ?? null,
    selectedApprovalMode: overrides.selectedApprovalMode ?? 'suggest',
    selectedPrompt:
      overrides.selectedPrompt ??
      ({
        text: null,
        queuedAt: null,
        hasQueuedPrompt: false,
      } as AgentPaneView['selectedPrompt']),
    runtimeRunActiveControls: overrides.runtimeRunActiveControls ?? null,
    runtimeRunPendingControls: overrides.runtimeRunPendingControls ?? null,
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

function makeDictationStatus(overrides: Partial<DictationStatusDto> = {}): DictationStatusDto {
  return {
    platform: 'macos',
    osVersion: '26.0.0',
    defaultLocale: 'en_US',
    supportedLocales: ['en_US'],
    modern: {
      available: false,
      compiled: false,
      runtimeSupported: false,
      reason: 'modern_sdk_unavailable',
    },
    legacy: {
      available: true,
      compiled: true,
      runtimeSupported: true,
      reason: null,
    },
    modernAssets: {
      status: 'unavailable',
      locale: null,
      reason: 'modern_sdk_unavailable',
    },
    microphonePermission: 'authorized',
    speechPermission: 'authorized',
    activeSession: null,
    ...overrides,
  }
}

function createDictationAdapter(options: {
  status?: DictationStatusDto
  stop?: () => Promise<void>
  cancel?: () => Promise<void>
} = {}) {
  let eventHandler: ((event: DictationEventDto) => void) | null = null
  const session = {
    response: {
      sessionId: 'dictation-session-1',
      engine: 'legacy' as const,
      locale: 'en_US',
    },
    unsubscribe: vi.fn(),
    stop: vi.fn(options.stop ?? (async () => undefined)),
    cancel: vi.fn(options.cancel ?? (async () => undefined)),
  }
  const adapter: SpeechDictationAdapter = {
    isDesktopRuntime: () => true,
    speechDictationStatus: vi.fn(async () => options.status ?? makeDictationStatus()),
    speechDictationStart: vi.fn(async (_request, handler) => {
      eventHandler = handler
      handler({
        kind: 'started',
        sessionId: session.response.sessionId,
        engine: 'legacy',
        locale: 'en_US',
      })
      return session
    }),
    speechDictationStop: vi.fn(async () => undefined),
    speechDictationCancel: vi.fn(async () => undefined),
  }

  return {
    adapter,
    session,
    emit(event: DictationEventDto) {
      if (!eventHandler) {
        throw new Error('Dictation session has not started.')
      }

      act(() => {
        eventHandler?.(event)
      })
    },
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
    expect(screen.queryByRole('heading', { name: 'Recent autonomous workers' })).not.toBeInTheDocument()
  })

  it('renders recovered local-provider repair guidance without collapsing to generic credential copy', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProfileId: 'ollama-work',
          selectedProfileLabel: 'Ollama Work',
          selectedProviderId: 'ollama',
          selectedProviderLabel: 'Ollama',
          selectedProfileReadiness: {
            ready: false,
            status: 'malformed',
            proof: 'local',
            proofUpdatedAt: '2026-04-20T12:00:00Z',
          },
          runtimeSession: null,
          runtimeRun: makeRuntimeRun({
            providerId: 'ollama',
            runtimeKind: 'openai_compatible',
            runtimeLabel: 'Ollama · Supervisor running',
          }),
          runtimeStream: makeRuntimeStream({ status: 'idle' }),
          runtimeRunUnavailableReason:
            'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
          messagesUnavailableReason:
            'Cadence recovered durable supervised-run state for this project, but live streaming still requires repaired Ollama local-endpoint metadata for the configured provider.',
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Repair the Ollama local endpoint metadata in Settings to start.',
    )
    expect(screen.queryByText(/profile credentials/i)).not.toBeInTheDocument()
  })

  it('renders checkpoint control-loop cards and resume controls on the Agent tab', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession(),
          runtimeRun: makeRuntimeRun(),
          checkpointControlLoop: makeCheckpointControlLoop({
            items: [
              makeCheckpointControlLoopCard({
                actionId: 'action-1',
                key: 'action-1::boundary-1',
                boundaryId: 'boundary-1',
                title: 'Review worktree changes',
                detail: 'Inspect the repository diff before trusting the next operator step.',
                approval: {
                  actionId: 'action-1',
                  sessionId: 'session-1',
                  flowId: 'flow-1',
                  actionType: 'review_worktree',
                  title: 'Review worktree changes',
                  detail: 'Inspect the repository diff before trusting the next operator step.',
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
              }),
            ],
          }),
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible()
    expect(screen.getByText('Review worktree changes')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Resume run' })).toBeVisible()
  })

  it('does not render worker lifecycle cards on the Agent tab', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
      />,
    )

    expect(screen.queryByRole('heading', { name: 'Recent autonomous workers' })).not.toBeInTheDocument()
    expect(screen.queryByText('Snapshot lag')).not.toBeInTheDocument()
    expect(screen.queryByText('Handoff pending')).not.toBeInTheDocument()
    expect(screen.queryByText('Only the latest durable attempt per unit is shown here.')).not.toBeInTheDocument()
    expect(screen.queryByText('Showing 2 of 4 durable units in the bounded recent-history window.')).not.toBeInTheDocument()
    expect(screen.queryByText('+2 older units')).not.toBeInTheDocument()
  })

  it('does not render the empty worker lifecycle state', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
      />,
    )

    expect(screen.queryByRole('heading', { name: 'Recent autonomous workers' })).not.toBeInTheDocument()
    expect(screen.queryByText('No recent autonomous units recorded')).not.toBeInTheDocument()
    expect(
      screen.queryByText('Cadence has not persisted a bounded autonomous unit history for this project yet.'),
    ).not.toBeInTheDocument()
  })

  it('renders recovered durable denial cards on the Agent tab', () => {
    const deniedActionId = 'flow:flow-1:run:run-1:boundary:boundary-denied-1:review_command'
    const deniedCard = makeCheckpointControlLoopCard({
      actionId: deniedActionId,
      key: `${deniedActionId}::boundary-denied-1`,
      boundaryId: 'boundary-denied-1',
      title: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
      detail: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
      truthSource: 'recovered_durable',
      truthSourceLabel: 'Recovered durable denial',
      truthSourceDetail:
        'No resumable live review row remains, so this card is anchored to the durable shell-policy denial that Cadence persisted for the command.',
      liveActionRequired: null,
      liveStateLabel: 'No live review row',
      liveStateDetail:
        'Hard-denied shell-policy outcomes do not create a resumable live action-required row, so Cadence is anchoring this card to durable denial evidence.',
      liveUpdatedAt: null,
      approval: null,
      durableStateLabel: 'Policy denied',
      durableStateDetail: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
      durableUpdatedAt: '2026-04-16T20:04:10Z',
      latestResume: null,
      resumeStateLabel: 'Not resumable',
      resumeDetail: 'Hard-denied shell-policy outcomes do not create an operator approval or resume path.',
      resumeUpdatedAt: '2026-04-16T20:04:10Z',
      resumability: 'not_resumable',
      resumabilityLabel: 'Not resumable',
      resumabilityDetail:
        'Cadence recorded a hard denial for this action, so no operator resume path is available for this boundary.',
      isResumable: false,
      advancedFailureClass: null,
      advancedFailureClassLabel: null,
      advancedFailureDiagnosticCode: null,
      recoveryRecommendation: 'fix_permissions_policy',
      recoveryRecommendationLabel: 'Fix permissions / policy',
      recoveryRecommendationDetail:
        'Browser/computer-use action was blocked by policy or permissions. Fix access or policy before retrying.',
      evidenceCount: 2,
      evidenceStateLabel: '2 durable evidence rows',
      evidenceSummary: 'Showing the latest durable evidence rows linked to this action.',
      latestEvidenceAt: '2026-04-16T20:04:10Z',
      evidencePreviews: [
        {
          artifactId: 'artifact-policy-denied',
          artifactKindLabel: 'Policy denied',
          statusLabel: 'Recorded',
          summary: 'Cadence denied the autonomous shell command because its cwd escapes the imported repository root.',
          updatedAt: '2026-04-16T20:04:10Z',
        },
      ],
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
          checkpointControlLoop: makeCheckpointControlLoop({
            items: [deniedCard],
            durableOnlyCount: 0,
            recoveredCount: 1,
          }),
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible()
    expect(screen.getByText('Recovered durable denial')).toBeVisible()
    expect(screen.getAllByText('Policy denied').length).toBeGreaterThan(0)
    expect(screen.getByText(/Recovery guidance Fix permissions \/ policy/i)).toBeVisible()
  })

  it('surfaces operator-answer controls and operator-action failures on the Agent tab', () => {
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
      />,
    )

    expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible()
    expect(screen.getByLabelText('Operator answer for action-pending')).toBeVisible()
    expect(screen.getByText('Cadence could not approve action action-pending for boundary boundary-1.')).toBeVisible()
  })

  it('renders checkpoint recovery banners and bounded coverage copy on the Agent tab', () => {
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

    expect(screen.getByText('Remote escalation is actively polling this checkpoint')).toBeVisible()
    expect(screen.getByText('Bounded checkpoint coverage')).toBeVisible()
    expect(screen.getByText('Live hint only')).toBeVisible()
  })

  it('sends owned-agent live checkpoint responses through runtime run controls', async () => {
    const onUpdateRuntimeRunControls = vi.fn(async () => makeRuntimeRun())
    const actionId = 'plan-mode-before-tools'

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'owned-agent:run-1', runtimeKind: 'owned_agent' }),
          runtimeRun: makeRuntimeRun({
            runtimeKind: 'owned_agent',
            runtimeLabel: 'Owned agent · Running',
            supervisorKind: 'owned_agent',
            supervisorLabel: 'Owned agent',
          }),
          actionRequiredItems: [
            {
              id: 'owned-action-required-1',
              kind: 'action_required',
              runId: 'run-1',
              sequence: 9,
              createdAt: '2026-04-16T20:05:00Z',
              actionId,
              boundaryId: 'owned_agent',
              actionType: 'plan_mode',
              title: 'Plan required',
              detail: 'Plan mode paused before tool execution.',
            },
          ],
          checkpointControlLoop: makeCheckpointControlLoop({
            items: [
              makeCheckpointControlLoopCard({
                actionId,
                key: `${actionId}::owned_agent`,
                boundaryId: 'owned_agent',
                title: 'Plan required',
                detail: 'Plan mode paused before tool execution.',
                approval: null,
                liveActionRequired: {
                  id: 'owned-action-required-1',
                  kind: 'action_required',
                  runId: 'run-1',
                  sequence: 9,
                  createdAt: '2026-04-16T20:05:00Z',
                  actionId,
                  boundaryId: 'owned_agent',
                  actionType: 'plan_mode',
                  title: 'Plan required',
                  detail: 'Plan mode paused before tool execution.',
                },
                liveStateLabel: 'Live action required',
                durableStateLabel: 'Durable approval pending refresh',
                truthSource: 'live_hint_only',
                truthSourceLabel: 'Live hint only',
                truthSourceDetail:
                  'Cadence is showing the live action-required row while waiting for durable approval or evidence rows to persist.',
              }),
            ],
            liveHintOnlyCount: 1,
            durableOnlyCount: 0,
          }),
        })}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    const responseInput = screen.getByLabelText(`Owned agent response for ${actionId}`)
    const sendResponse = screen.getByRole('button', { name: 'Send response' })
    expect(sendResponse).toBeDisabled()

    fireEvent.change(responseInput, { target: { value: 'Proceed with the approved plan.' } })
    fireEvent.click(screen.getByRole('button', { name: 'Send response' }))

    await waitFor(() =>
      expect(onUpdateRuntimeRunControls).toHaveBeenCalledWith({
        prompt: 'Proceed with the approved plan.',
      }),
    )
    expect(screen.queryByText('Durable approval row not available')).not.toBeInTheDocument()
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
            'Configured provider profile OpenRouter Work (openrouter-work) no longer matches the persisted runtime session for OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind this profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Configured provider profile OpenRouter Work (openrouter-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Configured provider profile OpenRouter Work (openrouter-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile before trusting new stream activity.',
          runtimeSession: makeRuntimeSession({
            providerId: 'openai_codex',
            runtimeKind: 'openai_codex',
            phase: 'authenticated',
          }),
        })}
        onStartLogin={vi.fn(async () => null)}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
        onStartRuntimeSession={vi.fn(async () => null)}
      />,
    )

    expect(screen.queryByRole('heading', { name: 'OpenRouter is selected in Settings' })).not.toBeInTheDocument()
    expect(screen.queryByText('Provider mismatch')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Rebind OpenRouter runtime' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start OpenAI login' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
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
    expect(screen.getByText('Connect a provider in Settings to start chatting with the agent.')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Configure' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Configure an OpenRouter API key in Settings to start.',
    )
  })

  it('keeps GitHub Models provider mismatch truthful without rendering fallback provider UI', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProfileId: 'github-models-work',
          selectedProfileLabel: 'GitHub Models Work',
          selectedProviderId: 'github_models',
          selectedProviderLabel: 'GitHub Models',
          selectedModelId: 'openai/gpt-4.1',
          selectedProfileReadiness: {
            ready: true,
            status: 'ready',
            proofUpdatedAt: '2026-04-20T12:00:00Z',
          },
          providerMismatch: true,
          providerMismatchReason:
            'Configured provider profile GitHub Models Work (github-models-work) no longer matches the persisted runtime session for OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind this profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Configured provider profile GitHub Models Work (github-models-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Configured provider profile GitHub Models Work (github-models-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile before trusting new stream activity.',
          runtimeSession: makeRuntimeSession({
            providerId: 'openai_codex',
            runtimeKind: 'openai_codex',
            phase: 'authenticated',
          }),
        })}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
        onStartRuntimeSession={vi.fn(async () => null)}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Start OpenAI login' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Rebind GitHub Models before trusting new live activity.',
    )
  })

  it('renders GitHub Models setup guidance in the centered agent empty state', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'github_models',
          selectedProviderLabel: 'GitHub Models',
          selectedModelId: 'openai/gpt-4.1',
          selectedProfileReadiness: {
            ready: false,
            status: 'missing',
            proofUpdatedAt: null,
          },
          runtimeSession: null,
          sessionUnavailableReason:
            'Configure a GitHub Models API key in Settings before Cadence can bind a project runtime session.',
          messagesUnavailableReason:
            'Configure a GitHub Models API key in Settings before Cadence can establish a runtime session for this imported project.',
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Configure a GitHub Models API key in Settings to start.',
    )
  })

  it('keeps Anthropic provider mismatch truthful without rendering provider-specific fallback UI', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProfileId: 'anthropic-work',
          selectedProfileLabel: 'Anthropic Work',
          selectedProviderId: 'anthropic',
          selectedProviderLabel: 'Anthropic',
          selectedModelId: 'claude-3-7-sonnet-latest',
          selectedProfileReadiness: {
            ready: true,
            status: 'ready',
            proofUpdatedAt: '2026-04-20T12:00:00Z',
          },
          providerMismatch: true,
          providerMismatchReason:
            'Configured provider profile Anthropic Work (anthropic-work) no longer matches the persisted runtime session for OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind this profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Configured provider profile Anthropic Work (anthropic-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Configured provider profile Anthropic Work (anthropic-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile before trusting new stream activity.',
          runtimeSession: makeRuntimeSession({
            providerId: 'openai_codex',
            runtimeKind: 'openai_codex',
            phase: 'authenticated',
          }),
        })}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
        onStartRuntimeSession={vi.fn(async () => null)}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Start OpenAI login' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Rebind Anthropic before trusting new live activity.',
    )
  })

  it('renders Anthropic setup guidance in the centered agent empty state', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'anthropic',
          selectedProviderLabel: 'Anthropic',
          selectedModelId: 'claude-3-7-sonnet-latest',
          selectedProfileReadiness: {
            ready: false,
            status: 'missing',
            proofUpdatedAt: null,
          },
          runtimeSession: null,
          sessionUnavailableReason:
            'Configure an Anthropic API key in Settings before Cadence can bind a project runtime session.',
          messagesUnavailableReason:
            'Configure an Anthropic API key in Settings before Cadence can establish a runtime session for this imported project.',
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Configure an Anthropic API key in Settings to start.',
    )
  })

  it('keeps Ollama provider mismatch truthful without rendering provider-specific fallback UI', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProfileId: 'ollama-work',
          selectedProfileLabel: 'Ollama Work',
          selectedProviderId: 'ollama',
          selectedProviderLabel: 'Ollama',
          selectedModelId: 'llama3.2',
          selectedProfileReadiness: {
            ready: true,
            status: 'ready',
            proof: 'local',
            proofUpdatedAt: '2026-04-20T12:00:00Z',
          },
          providerMismatch: true,
          providerMismatchReason:
            'Configured provider profile Ollama Work (ollama-work) no longer matches the persisted runtime session for OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind this profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Configured provider profile Ollama Work (ollama-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Configured provider profile Ollama Work (ollama-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile before trusting new stream activity.',
          runtimeSession: makeRuntimeSession({
            providerId: 'openai_codex',
            runtimeKind: 'openai_codex',
            phase: 'authenticated',
          }),
        })}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
        onStartRuntimeSession={vi.fn(async () => null)}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Start OpenAI login' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Rebind Ollama before trusting new live activity.',
    )
  })

  it('renders Ollama setup guidance in the centered agent empty state', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'ollama',
          selectedProviderLabel: 'Ollama',
          selectedModelId: 'llama3.2',
          selectedProfileReadiness: {
            ready: false,
            status: 'missing',
            proofUpdatedAt: null,
          },
          runtimeSession: null,
          sessionUnavailableReason:
            'Save the selected Ollama local endpoint profile in Settings before Cadence can bind a project runtime session.',
          messagesUnavailableReason:
            'Save the selected Ollama local endpoint profile in Settings before Cadence can establish a runtime session for this imported project.',
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Save the selected Ollama local endpoint profile in Settings to start.',
    )
  })

  it('keeps Bedrock provider mismatch truthful without rendering provider-specific fallback UI', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProfileId: 'bedrock-work',
          selectedProfileLabel: 'Amazon Bedrock Work',
          selectedProviderId: 'bedrock',
          selectedProviderLabel: 'Amazon Bedrock',
          selectedModelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
          selectedProfileReadiness: {
            ready: true,
            status: 'ready',
            proof: 'ambient',
            proofUpdatedAt: '2026-04-20T12:00:00Z',
          },
          providerMismatch: true,
          providerMismatchReason:
            'Configured provider profile Amazon Bedrock Work (bedrock-work) no longer matches the persisted runtime session for OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind this profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Configured provider profile Amazon Bedrock Work (bedrock-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Configured provider profile Amazon Bedrock Work (bedrock-work) no longer matches the persisted runtime session for OpenAI Codex. Rebind this profile before trusting new stream activity.',
          runtimeSession: makeRuntimeSession({
            providerId: 'openai_codex',
            runtimeKind: 'openai_codex',
            phase: 'authenticated',
          }),
        })}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
        onStartRuntimeSession={vi.fn(async () => null)}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Start OpenAI login' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Rebind Amazon Bedrock before trusting new live activity.',
    )
  })

  it('renders Bedrock ambient-auth setup guidance in the centered agent empty state', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'bedrock',
          selectedProviderLabel: 'Amazon Bedrock',
          selectedModelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
          selectedProfileReadiness: {
            ready: false,
            status: 'missing',
            proofUpdatedAt: null,
          },
          runtimeSession: null,
          sessionUnavailableReason:
            'Save the selected Amazon Bedrock ambient-auth profile with region in Settings before Cadence can bind a project runtime session.',
          messagesUnavailableReason:
            'Save the selected Amazon Bedrock ambient-auth profile with region in Settings before Cadence can establish a runtime session for this imported project.',
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Save the selected Amazon Bedrock ambient-auth profile with region in Settings to start.',
    )
  })

  it('renders Vertex ambient-auth setup guidance in the centered agent empty state', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedProviderId: 'vertex',
          selectedProviderLabel: 'Google Vertex AI',
          selectedModelId: 'claude-3-7-sonnet@20250219',
          selectedProfileReadiness: {
            ready: false,
            status: 'missing',
            proofUpdatedAt: null,
          },
          runtimeSession: null,
          sessionUnavailableReason:
            'Save the selected Google Vertex AI ambient-auth profile with region and project ID in Settings before Cadence can bind a project runtime session.',
          messagesUnavailableReason:
            'Save the selected Google Vertex AI ambient-auth profile with region and project ID in Settings before Cadence can establish a runtime session for this imported project.',
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Save the selected Google Vertex AI ambient-auth profile with region and project ID in Settings to start.',
    )
  })

  it('renders a centered agent runtime setup state and opens settings', () => {
    const onOpenSettings = vi.fn()

    render(<AgentRuntime agent={makeAgent()} onOpenSettings={onOpenSettings} />)

    const composer = screen.getByLabelText('Agent input unavailable')
    const modelSelector = screen.getByRole('combobox', { name: 'Model selector' })
    const thinkingLevelSelector = screen.getByRole('combobox', { name: 'Thinking level selector' })

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
    expect(screen.getByText('Connect a provider in Settings to start chatting with the agent.')).toBeVisible()
    expect(composer).toHaveAttribute('placeholder', 'Connect a provider to start.')
    expect(composer).toHaveAttribute('rows', '3')
    expect(modelSelector).toHaveTextContent('openai_codex')
    expect(thinkingLevelSelector).toHaveTextContent('Thinking unavailable')
    expect(screen.getByRole('button', { name: 'Configure' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Send message unavailable' })).toBeDisabled()
    expect(screen.queryByText('Context')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Configure' }))
    expect(onOpenSettings).toHaveBeenCalledTimes(1)
  })

  it('offers diagnostics from runtime startup failures', () => {
    const onOpenDiagnostics = vi.fn()

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeRunActionError: {
            code: 'provider_profile_credentials_missing',
            message: 'Runtime startup failed because provider credentials are missing.',
            retryable: false,
          },
        })}
        onOpenDiagnostics={onOpenDiagnostics}
      />,
    )

    expect(screen.getByText('Runtime startup failed because provider credentials are missing.')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Diagnostics' }))

    expect(onOpenDiagnostics).toHaveBeenCalledTimes(1)
  })

  it('renders the current model selectors and disables compose actions while a run update is pending', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          controlTruthSource: 'runtime_run',
          selectedModelId: 'anthropic/claude-3.5-haiku',
          selectedThinkingEffort: 'low',
          selectedApprovalMode: 'yolo',
          selectedPrompt: {
            text: 'Review the diff before continuing.',
            queuedAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
          runtimeRunActiveControls: {
            providerProfileId: null,
            modelId: 'openai_codex',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            planModeRequired: false,
            revision: 1,
            appliedAt: '2026-04-20T12:00:00Z',
          },
          runtimeRunPendingControls: {
            providerProfileId: null,
            modelId: 'anthropic/claude-3.5-haiku',
            thinkingEffort: 'low',
            thinkingEffortLabel: 'Low',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            planModeRequired: false,
            revision: 2,
            queuedAt: '2026-04-20T12:05:00Z',
            queuedPrompt: 'Review the diff before continuing.',
            queuedPromptAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
          providerModelCatalog: makeProviderModelCatalog({
            models: [
              makeAgentModel({
                modelId: 'openai_codex',
                label: 'openai_codex',
                displayName: 'openai_codex',
                groupId: 'openai',
                groupLabel: 'OpenAI',
                availability: 'available',
                availabilityLabel: 'Available',
                thinkingSupported: true,
                thinkingEffortOptions: ['medium'],
                defaultThinkingEffort: 'medium',
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
            ],
          }),
        })}
      />,
    )

    expect(screen.getByRole('combobox', { name: 'Model selector' })).toBeDisabled()
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Send message unavailable' })).toBeDisabled()
    expect(screen.queryByText('Queued prompt pending the next model-call boundary.')).not.toBeInTheDocument()
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()
  })

  it('starts a run with the draft prompt and current projected controls, then clears the draft after acknowledgement', async () => {
    const onStartRuntimeRun = vi.fn(async () => makeRuntimeRun())
    const { rerender } = render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: null,
          controlTruthSource: 'fallback',
          selectedModelId: 'openai_codex',
          selectedThinkingEffort: 'medium',
          selectedApprovalMode: 'suggest',
        })}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Kick off the first run.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(onStartRuntimeRun).toHaveBeenCalledWith({
        controls: {
          providerProfileId: null,
          modelId: 'openai_codex',
          thinkingEffort: null,
          approvalMode: 'suggest',
          planModeRequired: false,
        },
        prompt: 'Kick off the first run.',
      }),
    )

    rerender(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          controlTruthSource: 'runtime_run',
          selectedModelId: 'openai_codex',
          selectedThinkingEffort: 'medium',
          selectedApprovalMode: 'suggest',
          selectedPrompt: {
            text: 'Kick off the first run.',
            queuedAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
          runtimeRunActiveControls: {
            providerProfileId: null,
            modelId: 'openai_codex',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            planModeRequired: false,
            revision: 1,
            appliedAt: '2026-04-20T12:00:00Z',
          },
          runtimeRunPendingControls: {
            providerProfileId: null,
            modelId: 'openai_codex',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'suggest',
            approvalModeLabel: 'Suggest',
            planModeRequired: false,
            revision: 2,
            queuedAt: '2026-04-20T12:05:00Z',
            queuedPrompt: 'Kick off the first run.',
            queuedPromptAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
        })}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    await waitFor(() => expect(screen.getByLabelText('Agent input unavailable')).toHaveValue(''))
  })

  it('binds a ready provider profile and starts the first run from the send button', async () => {
    const onStartRuntimeSession = vi.fn(async () =>
      makeRuntimeSession({
        runtimeKind: 'openrouter',
        providerId: 'openrouter',
        sessionId: 'session-openrouter',
      }),
    )
    const onStartRuntimeRun = vi.fn(async () =>
      makeRuntimeRun({
        runtimeKind: 'openrouter',
        providerId: 'openrouter',
        runtimeLabel: 'OpenRouter · Supervisor running',
      }),
    )

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: null,
          runtimeRun: null,
          selectedProfileId: 'openrouter-default',
          selectedProfileLabel: 'OpenRouter',
          selectedProviderId: 'openrouter',
          selectedProviderLabel: 'OpenRouter',
          selectedModelId: 'openai/gpt-4.1-mini',
          selectedThinkingEffort: 'medium',
          selectedProfileReadiness: {
            ready: true,
            status: 'ready',
            proof: 'stored_secret',
            proofUpdatedAt: '2026-04-20T12:00:00Z',
          },
          providerModelCatalog: makeProviderModelCatalog({
            profileId: 'openrouter-default',
            profileLabel: 'OpenRouter',
            providerId: 'openrouter',
            providerLabel: 'OpenRouter',
            models: [
              makeAgentModel({
                modelId: 'openai/gpt-4.1-mini',
                label: 'openai/gpt-4.1-mini',
                displayName: 'OpenAI GPT-4.1 Mini',
                groupId: 'openai',
                groupLabel: 'OpenAI',
                availability: 'available',
                availabilityLabel: 'Available',
                thinkingSupported: true,
                thinkingEffortOptions: ['low', 'medium', 'high'],
                defaultThinkingEffort: 'medium',
              }),
            ],
          }),
        })}
        onStartRuntimeSession={onStartRuntimeSession}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    expect(screen.queryByText('Configure agent runtime')).not.toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Model selector' })).toBeEnabled()
    expect(screen.getByRole('combobox', { name: 'Thinking level selector' })).toBeEnabled()
    expect(screen.getByLabelText('Agent input')).toHaveAttribute(
      'placeholder',
      'Send a message to start with OpenRouter.',
    )

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Build the provider path.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() => expect(onStartRuntimeSession).toHaveBeenCalledTimes(1))
    await waitFor(() =>
      expect(onStartRuntimeRun).toHaveBeenCalledWith({
        controls: {
          providerProfileId: null,
          modelId: 'openai/gpt-4.1-mini',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          planModeRequired: false,
        },
        prompt: 'Build the provider path.',
      }),
    )
  })

  it('keeps the first prompt queued when provider binding does not authenticate', async () => {
    const onStartRuntimeSession = vi.fn(async () =>
      makeRuntimeSession({
        runtimeKind: 'openrouter',
        providerId: 'openrouter',
        phase: 'idle',
        phaseLabel: 'Idle',
        sessionId: null,
        isAuthenticated: false,
        isSignedOut: true,
        lastError: {
          code: 'provider_validation_failed',
          message: 'OpenRouter rejected the saved API key.',
          retryable: false,
        },
      }),
    )
    const onStartRuntimeRun = vi.fn(async () => makeRuntimeRun())

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: null,
          runtimeRun: null,
          selectedProfileId: 'openrouter-default',
          selectedProfileLabel: 'OpenRouter',
          selectedProviderId: 'openrouter',
          selectedProviderLabel: 'OpenRouter',
          selectedModelId: 'openai/gpt-4.1-mini',
          selectedThinkingEffort: 'medium',
          selectedProfileReadiness: {
            ready: true,
            status: 'ready',
            proof: 'stored_secret',
            proofUpdatedAt: '2026-04-20T12:00:00Z',
          },
          providerModelCatalog: makeProviderModelCatalog({
            profileId: 'openrouter-default',
            profileLabel: 'OpenRouter',
            providerId: 'openrouter',
            providerLabel: 'OpenRouter',
            models: [
              makeAgentModel({
                modelId: 'openai/gpt-4.1-mini',
                label: 'openai/gpt-4.1-mini',
                displayName: 'OpenAI GPT-4.1 Mini',
                groupId: 'openai',
                groupLabel: 'OpenAI',
                availability: 'available',
                availabilityLabel: 'Available',
                thinkingSupported: true,
                thinkingEffortOptions: ['medium'],
                defaultThinkingEffort: 'medium',
              }),
            ],
          }),
        })}
        onStartRuntimeSession={onStartRuntimeSession}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Do not lose this.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() => expect(onStartRuntimeSession).toHaveBeenCalledTimes(1))
    expect(onStartRuntimeRun).not.toHaveBeenCalled()
    expect(screen.getByLabelText('Agent input')).toHaveValue('Do not lose this.')
    expect(await screen.findByText('OpenRouter rejected the saved API key.')).toBeVisible()
  })

  it('queues the next prompt against the active run while preserving truthful selected controls', async () => {
    const onUpdateRuntimeRunControls = vi.fn(async () => makeRuntimeRun())

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          controlTruthSource: 'runtime_run',
          selectedModelId: 'openai_codex',
          selectedThinkingEffort: 'medium',
          selectedApprovalMode: 'yolo',
          runtimeRunActiveControls: {
            providerProfileId: null,
            modelId: 'openai_codex',
            thinkingEffort: 'medium',
            thinkingEffortLabel: 'Medium',
            approvalMode: 'yolo',
            approvalModeLabel: 'YOLO',
            planModeRequired: false,
            revision: 3,
            appliedAt: '2026-04-20T12:00:00Z',
          },
        })}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Queue the next prompt.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(onUpdateRuntimeRunControls).toHaveBeenCalledWith({
        prompt: 'Queue the next prompt.',
      }),
    )
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toHaveTextContent('YOLO')
  })

  it('opts owned-agent continuations into auto-compact from the composer', async () => {
    const onUpdateRuntimeRunControls = vi.fn(async () => makeRuntimeRun())

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun({
            runtimeKind: 'owned_agent',
            runtimeLabel: 'Owned agent · Running',
            supervisorKind: 'owned_agent',
            supervisorLabel: 'Owned agent',
          }),
        })}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Auto-compact before sending' }))
    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Continue after compacting old context.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(onUpdateRuntimeRunControls).toHaveBeenCalledWith({
        prompt: 'Continue after compacting old context.',
        autoCompact: {
          enabled: true,
          thresholdPercent: 85,
          rawTailMessageCount: 8,
        },
      }),
    )
  })

  it('keeps the dictation mic hidden without native macOS support', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
        onUpdateRuntimeRunControls={vi.fn(async () => makeRuntimeRun())}
      />,
    )

    expect(screen.queryByRole('button', { name: 'Start dictation' })).not.toBeInTheDocument()
  })

  it('disables the dictation mic while the composer input is disabled', async () => {
    const dictation = createDictationAdapter()

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
          runtimeRunActionStatus: 'running',
          pendingRuntimeRunAction: 'update_controls',
        })}
        desktopAdapter={dictation.adapter}
        onUpdateRuntimeRunControls={vi.fn(async () => makeRuntimeRun())}
      />,
    )

    const micButton = await screen.findByRole('button', { name: 'Start dictation' })
    expect(micButton).toBeDisabled()

    fireEvent.click(micButton)
    expect(dictation.adapter.speechDictationStart).not.toHaveBeenCalled()
  })

  it('keeps Enter-to-send behavior unchanged when dictation support is available', async () => {
    const dictation = createDictationAdapter()
    const onUpdateRuntimeRunControls = vi.fn(async () => makeRuntimeRun())

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
        desktopAdapter={dictation.adapter}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    const input = screen.getByLabelText('Agent input')
    fireEvent.change(input, { target: { value: 'Send from keyboard.' } })
    fireEvent.keyDown(input, { key: 'Enter', shiftKey: true })
    expect(onUpdateRuntimeRunControls).not.toHaveBeenCalled()

    fireEvent.keyDown(input, { key: 'Enter' })

    await waitFor(() =>
      expect(onUpdateRuntimeRunControls).toHaveBeenCalledWith({
        prompt: 'Send from keyboard.',
      }),
    )
  })

  it('streams dictated partials into the composer without duplicating revised fragments', async () => {
    const dictation = createDictationAdapter()

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
        desktopAdapter={dictation.adapter}
        onUpdateRuntimeRunControls={vi.fn(async () => makeRuntimeRun())}
      />,
    )

    const input = screen.getByLabelText('Agent input')
    fireEvent.change(input, { target: { value: 'Review' } })
    fireEvent.click(await screen.findByRole('button', { name: 'Start dictation' }))

    await waitFor(() => expect(dictation.adapter.speechDictationStart).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop dictation' })).toHaveAttribute('aria-pressed', 'true'))
    await waitFor(() => expect(input).toHaveFocus())

    dictation.emit({
      kind: 'partial',
      sessionId: 'dictation-session-1',
      text: 'the logs',
      sequence: 1,
    })
    expect(input).toHaveValue('Review the logs')

    dictation.emit({
      kind: 'partial',
      sessionId: 'dictation-session-1',
      text: 'the logs carefully',
      sequence: 2,
    })
    expect(input).toHaveValue('Review the logs carefully')

    dictation.emit({
      kind: 'final',
      sessionId: 'dictation-session-1',
      text: 'the logs carefully before sending',
      sequence: 3,
    })
    expect(input).toHaveValue('Review the logs carefully before sending')
  })

  it('appends new dictated partials after manual edits during dictation', async () => {
    const dictation = createDictationAdapter()

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
        desktopAdapter={dictation.adapter}
        onUpdateRuntimeRunControls={vi.fn(async () => makeRuntimeRun())}
      />,
    )

    const input = screen.getByLabelText('Agent input')
    fireEvent.change(input, { target: { value: 'Draft' } })
    fireEvent.click(await screen.findByRole('button', { name: 'Start dictation' }))
    await waitFor(() => expect(dictation.adapter.speechDictationStart).toHaveBeenCalledTimes(1))

    dictation.emit({
      kind: 'partial',
      sessionId: 'dictation-session-1',
      text: 'first phrase',
      sequence: 1,
    })
    expect(input).toHaveValue('Draft first phrase')

    fireEvent.change(input, { target: { value: 'Draft with manual edit' } })

    dictation.emit({
      kind: 'partial',
      sessionId: 'dictation-session-1',
      text: 'second phrase',
      sequence: 2,
    })
    expect(input).toHaveValue('Draft with manual edit second phrase')
  })

  it('stops active dictation before submitting the composer draft', async () => {
    const calls: string[] = []
    const dictation = createDictationAdapter({
      stop: async () => {
        calls.push('stop')
      },
    })
    const onUpdateRuntimeRunControls = vi.fn(async () => {
      calls.push('submit')
      return makeRuntimeRun()
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
        desktopAdapter={dictation.adapter}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Send after stopping dictation.' },
    })
    fireEvent.click(await screen.findByRole('button', { name: 'Start dictation' }))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop dictation' })).toBeVisible())

    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() => expect(onUpdateRuntimeRunControls).toHaveBeenCalledTimes(1))
    expect(dictation.session.stop).toHaveBeenCalledTimes(1)
    expect(calls).toEqual(['stop', 'submit'])
  })

  it('cancels active dictation when the selected agent session changes', async () => {
    const dictation = createDictationAdapter()
    const onUpdateRuntimeRunControls = vi.fn(async () => makeRuntimeRun())
    const { rerender } = render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
        desktopAdapter={dictation.adapter}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    fireEvent.click(await screen.findByRole('button', { name: 'Start dictation' }))
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop dictation' })).toBeVisible())

    rerender(
      <AgentRuntime
        agent={makeAgent({
          project: makeProject({ selectedAgentSessionId: 'agent-session-next' }),
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun(),
        })}
        desktopAdapter={dictation.adapter}
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    await waitFor(() => expect(dictation.session.cancel).toHaveBeenCalledTimes(1))
  })

})
