import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { openUrlMock, saveDialogMock } = vi.hoisted(() => ({
  openUrlMock: vi.fn(),
  saveDialogMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

vi.mock('@tauri-apps/plugin-dialog', () => ({
  save: saveDialogMock,
}))

afterEach(() => {
  openUrlMock.mockReset()
  saveDialogMock.mockReset()
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
  AgentSessionView,
  ProjectDetailView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamView,
  SessionContextSnapshotDto,
  SessionTranscriptDto,
  SessionTranscriptExportResponseDto,
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

function makeAgentSession(overrides: Partial<AgentSessionView> = {}): AgentSessionView {
  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    title: 'History session',
    summary: 'Session with durable run history',
    status: 'active',
    statusLabel: 'Active',
    selected: true,
    createdAt: '2026-04-26T10:00:00Z',
    updatedAt: '2026-04-26T11:00:00Z',
    archivedAt: null,
    lastRunId: 'run-history-2',
    lastRuntimeKind: 'owned_agent',
    lastProviderId: 'openrouter',
    isActive: true,
    isArchived: false,
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

function makeSessionTranscript(): SessionTranscriptDto {
  const redaction = {
    redactionClass: 'public' as const,
    redacted: false,
    reason: null,
  }
  const baseItem = {
    contractVersion: 1 as const,
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    providerId: 'openrouter',
    modelId: 'openai/gpt-5.4',
    sourceKind: 'owned_agent' as const,
    sourceTable: 'agent_runs',
    kind: 'message' as const,
    actor: 'user' as const,
    summary: null,
    toolCallId: null,
    toolName: null,
    toolState: null,
    filePath: null,
    checkpointKind: null,
    actionId: null,
    redaction,
  }

  return {
    contractVersion: 1,
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    title: 'History session',
    summary: 'Session with durable run history',
    status: 'active',
    archived: false,
    archivedAt: null,
    runs: [
      {
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-history-1',
        providerId: 'openrouter',
        modelId: 'openai/gpt-5.4',
        status: 'completed',
        startedAt: '2026-04-26T10:00:00Z',
        completedAt: '2026-04-26T10:05:00Z',
        itemCount: 1,
        usageTotals: null,
      },
      {
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-history-2',
        providerId: 'openrouter',
        modelId: 'openai/gpt-5.4',
        status: 'completed',
        startedAt: '2026-04-26T11:00:00Z',
        completedAt: '2026-04-26T11:05:00Z',
        itemCount: 1,
        usageTotals: null,
      },
    ],
    items: [
      {
        ...baseItem,
        itemId: 'run_prompt:run-history-1',
        runId: 'run-history-1',
        sourceId: 'run-history-1',
        sequence: 1,
        createdAt: '2026-04-26T10:00:00Z',
        title: 'Run prompt',
        text: 'First run prompt',
      },
      {
        ...baseItem,
        itemId: 'run_prompt:run-history-2',
        runId: 'run-history-2',
        sourceId: 'run-history-2',
        sequence: 2,
        createdAt: '2026-04-26T11:00:00Z',
        title: 'Run prompt',
        text: 'Second run prompt',
      },
    ],
    usageTotals: null,
    redaction,
  }
}

function makeContextSnapshot(overrides: Partial<SessionContextSnapshotDto> = {}): SessionContextSnapshotDto {
  const redaction = {
    redactionClass: 'public' as const,
    redacted: false,
    reason: null,
  }
  const contributors: SessionContextSnapshotDto['contributors'] = [
    {
      contributorId: 'system_prompt:run-1',
      kind: 'system_prompt',
      label: 'System prompt',
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-1',
      sourceId: 'owned_agent_system_prompt',
      sequence: 1,
      estimatedTokens: 180,
      estimatedChars: 720,
      included: true,
      modelVisible: true,
      text: 'Owned-agent system prompt.',
      redaction,
    },
    {
      contributorId: 'instruction:AGENTS.md',
      kind: 'instruction_file',
      label: 'Project instructions',
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: null,
      sourceId: 'AGENTS.md',
      sequence: 2,
      estimatedTokens: 120,
      estimatedChars: 480,
      included: true,
      modelVisible: true,
      text: 'Use ShadCN for all UI where possible.',
      redaction,
    },
    {
      contributorId: 'tool_descriptor:read',
      kind: 'tool_descriptor',
      label: 'Tool descriptor: read',
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-1',
      sourceId: 'read',
      sequence: 3,
      estimatedTokens: 220,
      estimatedChars: 880,
      included: true,
      modelVisible: true,
      text: 'Read a file from the imported repository.',
      redaction,
    },
    {
      contributorId: 'message:run-1:2',
      kind: 'conversation_tail',
      label: 'User message',
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-1',
      sourceId: '2',
      sequence: 4,
      estimatedTokens: 360,
      estimatedChars: 1440,
      included: true,
      modelVisible: true,
      text: 'Implement the context panel.',
      redaction,
    },
    {
      contributorId: 'provider_usage:run-1',
      kind: 'provider_usage',
      label: 'Provider usage',
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-1',
      sourceId: 'run-1',
      sequence: 5,
      estimatedTokens: 0,
      estimatedChars: 48,
      included: false,
      modelVisible: false,
      text: '1200 input + 400 output = 1600 total tokens.',
      redaction,
    },
  ]

  return {
    contractVersion: 1,
    snapshotId: 'context:project-1:agent-session-main:run-1',
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runId: 'run-1',
    providerId: 'openrouter',
    modelId: 'openai/gpt-5.4',
    generatedAt: '2026-04-26T12:00:00Z',
    budget: {
      budgetTokens: 1000,
      estimatedTokens: 880,
      estimationSource: 'mixed',
      pressure: 'high',
      knownProviderBudget: true,
    },
    contributors,
    policyDecisions: [
      {
        contractVersion: 1,
        decisionId: 'compaction:auto:disabled',
        kind: 'compaction',
        action: 'skipped',
        trigger: 'auto',
        reasonCode: 'auto_compact_disabled',
        message: 'Auto-compact is disabled for this session.',
        rawTranscriptPreserved: true,
        modelVisible: false,
        redaction,
      },
    ],
    usageTotals: {
      projectId: 'project-1',
      runId: 'run-1',
      providerId: 'openrouter',
      modelId: 'openai/gpt-5.4',
      inputTokens: 1200,
      outputTokens: 400,
      totalTokens: 1600,
      estimatedCostMicros: 42,
      source: 'provider',
      updatedAt: '2026-04-26T12:00:00Z',
    },
    redaction,
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

describe('AgentRuntime current UI', () => {
  it('does not expose the removed pipeline mock inside the Agent tab', () => {
    render(
      <AgentRuntime
        agent={makeAgent({ runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }) })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.queryByRole('tab', { name: 'Runtime' })).not.toBeInTheDocument()
    expect(screen.queryByRole('tab', { name: 'Pipeline' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Cadence Desktop' })).not.toBeInTheDocument()
    expect(screen.queryByLabelText('Pipeline contract overview')).not.toBeInTheDocument()
    expect(screen.queryByText('Distributed evidence-pack assembly')).not.toBeInTheDocument()
  })

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

  it('keeps the recovered runtime snapshot visible without rendering removed debug panels', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          autonomousRun: makeAutonomousRun({ duplicateStartDetected: true, duplicateStartRunId: 'auto-run-1' }),
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({ status: 'idle' }),
          runtimeRunUnavailableReason: 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
          messagesUnavailableReason: 'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.',
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Waiting for the first run-scoped event' })).toBeVisible()
    expect(screen.queryByRole('heading', { name: 'Recovered run snapshot' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByText('Recovered the current autonomous unit boundary.')).not.toBeInTheDocument()
    expect(screen.queryByText('Duplicate start prevented')).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
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
            'Cadence recovered durable supervised-run state for this project, but live streaming still requires repaired Ollama local-endpoint metadata for the selected provider.',
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
    expect(
      screen.getByText('Open Settings to choose a provider and model before using the agent tab for this imported project.'),
    ).toBeVisible()
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
            'Settings now select provider profile GitHub Models Work (github-models-work), but the persisted runtime session still reflects OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind the selected profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Settings now select provider profile GitHub Models Work (github-models-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Settings now select provider profile GitHub Models Work (github-models-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile before trusting new stream activity.',
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
            'Settings now select provider profile Anthropic Work (anthropic-work), but the persisted runtime session still reflects OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind the selected profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Settings now select provider profile Anthropic Work (anthropic-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Settings now select provider profile Anthropic Work (anthropic-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile before trusting new stream activity.',
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
            'Settings now select provider profile Ollama Work (ollama-work), but the persisted runtime session still reflects OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind the selected profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Settings now select provider profile Ollama Work (ollama-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Settings now select provider profile Ollama Work (ollama-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile before trusting new stream activity.',
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
            'Settings now select provider profile Amazon Bedrock Work (bedrock-work), but the persisted runtime session still reflects OpenAI Codex.',
          providerMismatchRecoveryCopy:
            'Rebind the selected profile so durable runtime truth matches Settings.',
          sessionUnavailableReason:
            'Settings now select provider profile Amazon Bedrock Work (bedrock-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile so durable runtime truth matches Settings.',
          messagesUnavailableReason:
            'Live runtime streaming is paused because Settings now select provider profile Amazon Bedrock Work (bedrock-work), but the persisted runtime session still reflects OpenAI Codex. Rebind the selected profile before trusting new stream activity.',
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

  it('renders browser/computer-use and MCP summary context in the tool lane without regressing standard tool details', () => {
    const toolCalls: RuntimeStreamView['toolCalls'] = [
      {
        id: 'tool-read',
        kind: 'tool',
        runId: 'run-1',
        sequence: 5,
        createdAt: '2026-04-18T14:10:00Z',
        toolCallId: 'tool-read-1',
        toolName: 'read',
        toolState: 'succeeded',
        detail: 'Read README.md from the repository root.',
        toolSummary: {
          kind: 'command',
          exitCode: 0,
          timedOut: false,
          stdoutTruncated: false,
          stderrTruncated: false,
          stdoutRedacted: false,
          stderrRedacted: false,
        },
      },
      {
        id: 'tool-browser',
        kind: 'tool',
        runId: 'run-1',
        sequence: 6,
        createdAt: '2026-04-18T14:10:01Z',
        toolCallId: 'browser-click-1',
        toolName: 'browser.click',
        toolState: 'succeeded',
        detail: 'Browser click action reached the confirmation banner.',
        toolSummary: {
          kind: 'browser_computer_use',
          surface: 'browser',
          action: 'click',
          status: 'succeeded',
          target: 'button[type=submit]',
          outcome: 'Clicked submit and advanced to confirmation.',
        },
      },
      {
        id: 'tool-computer-use',
        kind: 'tool',
        runId: 'run-1',
        sequence: 7,
        createdAt: '2026-04-18T14:10:02Z',
        toolCallId: 'computer-key-1',
        toolName: 'computer_use.key_press',
        toolState: 'failed',
        detail: 'Computer-use key press is waiting for operator retry.',
        toolSummary: {
          kind: 'browser_computer_use',
          surface: 'computer_use',
          action: 'press_key',
          status: 'blocked',
          target: null,
          outcome: null,
        },
      },
      {
        id: 'tool-mcp',
        kind: 'tool',
        runId: 'run-1',
        sequence: 8,
        createdAt: '2026-04-18T14:10:03Z',
        toolCallId: 'mcp-invoke-1',
        toolName: 'mcp.invoke',
        toolState: 'failed',
        detail: 'MCP prompt invocation failed with upstream timeout.',
        toolSummary: {
          kind: 'mcp_capability',
          serverId: 'linear',
          capabilityKind: 'prompt',
          capabilityId: 'summarize_context',
          capabilityName: 'Summarize Context',
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
            items: toolCalls,
            toolCalls,
            lastSequence: 8,
          }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Tool lane' })).toBeVisible()
    expect(screen.getByText('read')).toBeVisible()
    expect(screen.getByText('browser.click')).toBeVisible()
    expect(screen.getByText('computer_use.key_press')).toBeVisible()
    expect(screen.getByText('mcp.invoke')).toBeVisible()
    expect(screen.getByText('Read README.md from the repository root.')).toBeVisible()
    expect(screen.getByText('Browser click action reached the confirmation banner.')).toBeVisible()
    expect(screen.getByText('Computer-use key press is waiting for operator retry.')).toBeVisible()
    expect(screen.getByText('MCP prompt invocation failed with upstream timeout.')).toBeVisible()
    expect(
      screen.getByText(
        'Browser action click · status Succeeded · target button[type=submit] · outcome Clicked submit and advanced to confirmation.',
      ),
    ).toBeVisible()
    expect(
      screen.getByText(
        'Computer use action press_key · status Blocked · target Target unavailable · outcome Outcome unavailable',
      ),
    ).toBeVisible()
    expect(screen.getByText('MCP Prompt · Summarize Context · server linear · outcome Failed')).toBeVisible()
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
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toHaveTextContent('Approval · yolo')
  })

  it('loads session history, navigates prior runs, and exports the selected run', async () => {
    const transcript = makeSessionTranscript()
    const redaction = {
      redactionClass: 'public' as const,
      redacted: false,
      reason: null,
    }
    const onLoadSessionTranscript = vi.fn(async () => transcript)
    const onExportSessionTranscript = vi.fn(
      async (request: { runId?: string | null; format: 'markdown' | 'json' }) =>
        ({
          payload: {
            contractVersion: 1,
            exportId: `export-${request.format}`,
            generatedAt: '2026-04-26T12:00:00Z',
            scope: request.runId ? 'run' : 'session',
            format: request.format,
            transcript,
            contextSnapshot: null,
            redaction,
          },
          content: `${request.format} export for ${request.runId ?? 'session'}`,
          mimeType: request.format === 'json' ? 'application/json' : 'text/markdown',
          suggestedFileName: request.format === 'json' ? 'history.json' : 'history.md',
        }) satisfies SessionTranscriptExportResponseDto,
    )
    const onSaveSessionTranscriptExport = vi.fn(async () => undefined)
    const clipboardWrite = vi.fn(async () => undefined)
    Object.defineProperty(navigator, 'clipboard', {
      configurable: true,
      value: { writeText: clipboardWrite },
    })
    saveDialogMock.mockResolvedValue('/tmp/history.json')

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          project: makeProject({
            agentSessions: [makeAgentSession()],
            selectedAgentSession: makeAgentSession(),
            selectedAgentSessionId: 'agent-session-main',
          }),
        })}
        historyTarget={{
          agentSessionId: 'agent-session-main',
          runId: 'run-history-1',
          source: 'search',
          nonce: 1,
        }}
        historySearchResult={{
          contractVersion: 1,
          resultId: 'item:run-history-1:run_prompt',
          projectId: 'project-1',
          agentSessionId: 'agent-session-main',
          runId: 'run-history-1',
          itemId: 'run_prompt:run-history-1',
          archived: false,
          rank: 0,
          matchedFields: ['text'],
          snippet: 'First run prompt',
          redaction,
        }}
        onLoadSessionTranscript={onLoadSessionTranscript}
        onExportSessionTranscript={onExportSessionTranscript}
        onSaveSessionTranscriptExport={onSaveSessionTranscriptExport}
      />,
    )

    expect(await screen.findByText('History session')).toBeVisible()
    expect(onLoadSessionTranscript).toHaveBeenCalledWith({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: null,
    })
    expect(screen.getAllByText('First run prompt').length).toBeGreaterThanOrEqual(1)

    fireEvent.click(screen.getByRole('button', { name: /run-history-2/i }))
    expect(await screen.findByText('Second run prompt')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Copy' }))
    await waitFor(() => expect(clipboardWrite).toHaveBeenCalledWith('markdown export for run-history-2'))
    expect(onExportSessionTranscript).toHaveBeenLastCalledWith({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      runId: 'run-history-2',
      format: 'markdown',
    })

    fireEvent.click(screen.getByRole('button', { name: 'JSON' }))
    await waitFor(() =>
      expect(onSaveSessionTranscriptExport).toHaveBeenCalledWith({
        path: '/tmp/history.json',
        content: 'json export for run-history-2',
      }),
    )
  })

  it('loads context visualization with budget pressure and contributors', async () => {
    const onLoadSessionContextSnapshot = vi.fn(async () => makeContextSnapshot())

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          project: makeProject({
            agentSessions: [makeAgentSession({ lastRunId: 'run-1' })],
            selectedAgentSession: makeAgentSession({ lastRunId: 'run-1' }),
            selectedAgentSessionId: 'agent-session-main',
          }),
        })}
        onLoadSessionContextSnapshot={onLoadSessionContextSnapshot}
        onUpdateRuntimeRunControls={vi.fn(async () => makeRuntimeRun({ runId: 'run-1' }))}
      />,
    )

    expect(await screen.findByText('Context')).toBeVisible()
    await waitFor(() =>
      expect(onLoadSessionContextSnapshot).toHaveBeenCalledWith({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-1',
        providerId: 'openai_codex',
        modelId: 'openai_codex',
        pendingPrompt: null,
      }),
    )
    expect(screen.getByText('High pressure')).toBeVisible()
    expect(screen.getByText('AGENTS.md included')).toBeVisible()
    expect(screen.getByText('Tool descriptor: read')).toBeVisible()
    expect(screen.getByText('Provider usage: 1.6K tokens recorded.')).toBeVisible()
  })

  it('includes the draft prompt in context preflight and shows over-budget pressure', async () => {
    const onLoadSessionContextSnapshot = vi.fn(async () =>
      makeContextSnapshot({
        budget: {
          budgetTokens: 1000,
          estimatedTokens: 1200,
          estimationSource: 'mixed',
          pressure: 'over',
          knownProviderBudget: true,
        },
      }),
    )

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun({ runId: 'run-1' }),
          project: makeProject({
            agentSessions: [makeAgentSession({ lastRunId: 'run-1' })],
            selectedAgentSession: makeAgentSession({ lastRunId: 'run-1' }),
            selectedAgentSessionId: 'agent-session-main',
          }),
        })}
        onLoadSessionContextSnapshot={onLoadSessionContextSnapshot}
        onUpdateRuntimeRunControls={vi.fn(async () => makeRuntimeRun({ runId: 'run-1' }))}
      />,
    )

    expect(await screen.findByText('Likely over context budget')).toBeVisible()
    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Continue with the largest remaining task.' },
    })

    await waitFor(() =>
      expect(onLoadSessionContextSnapshot).toHaveBeenLastCalledWith({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-1',
        providerId: 'openai_codex',
        modelId: 'openai_codex',
        pendingPrompt: 'Continue with the largest remaining task.',
      }),
    )
  })
})
