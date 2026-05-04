import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ComponentProps } from 'react'

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

import {
  AgentRuntime,
  isRuntimeConversationNearBottom,
} from '@/components/xero/agent-runtime'
import type { SpeechDictationAdapter } from '@/components/xero/agent-runtime/use-speech-dictation'
import type { AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type { DictationEventDto, DictationStatusDto } from '@/src/lib/xero-model/dictation'
import type {
  ProjectDetailView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamToolItemView,
} from '@/src/lib/xero-model'

function makeProject(overrides: Partial<ProjectDetailView> = {}): ProjectDetailView {
  return {
    id: 'project-1',
    name: 'Xero',
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
      rootPath: '/tmp/Xero',
      displayName: 'Xero',
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
    runtimeLabel: 'Openai Codex · Agent running',
    supervisorKind: 'owned_agent',
    supervisorLabel: 'Owned Agent',
    status: 'running',
    statusLabel: 'Agent running',
    transport: {
      kind: 'internal',
      endpoint: 'xero://owned-agent',
      liveness: 'reachable',
      livenessLabel: 'Runtime reachable',
    },
    controls: {
      active: {
        providerProfileId: null,
        agentDefinitionId: null,
        agentDefinitionVersion: null,
        runtimeAgentId: 'ask',
        runtimeAgentLabel: 'Ask',
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
        agentDefinitionId: null,
        agentDefinitionVersion: null,
        runtimeAgentId: 'ask',
        runtimeAgentLabel: 'Ask',
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
        summary: 'Owned agent runtime started.',
        createdAt: '2026-04-15T20:00:01Z',
      },
    ],
    latestCheckpoint: {
      sequence: 1,
      kind: 'bootstrap',
      kindLabel: 'Bootstrap',
      summary: 'Owned agent runtime started.',
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
      'Xero does not have a discovered model catalog for OpenAI Codex yet, so only configured model truth remains visible.',
    fetchedAt: null,
    lastSuccessAt: null,
    lastRefreshError: null,
    models: [makeAgentModel()],
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
    selectedProviderSource: overrides.selectedProviderSource ?? 'credential_default',
    controlTruthSource: overrides.controlTruthSource ?? (runtimeRun ? 'runtime_run' : 'fallback'),
    selectedRuntimeAgentId: overrides.selectedRuntimeAgentId ?? 'ask',
    selectedRuntimeAgentLabel: overrides.selectedRuntimeAgentLabel ?? 'Ask',
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
      overrides.runtimeRunUnavailableReason ?? 'Xero recovered a Xero-owned agent run before the live runtime feed resumed.',
    messagesUnavailableReason:
      overrides.messagesUnavailableReason ?? 'Xero authenticated this project, but the live runtime stream has not started yet.',
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

function makeTranscriptItem(options: {
  sequence: number
  role?: 'user' | 'assistant'
  text: string
}) {
  return {
    id: `transcript:run-1:${options.sequence}`,
    kind: 'transcript' as const,
    runId: 'run-1',
    sequence: options.sequence,
    createdAt: `2026-04-29T00:48:${String(options.sequence).padStart(2, '0')}Z`,
    role: options.role ?? 'assistant',
    text: options.text,
  }
}

function makeToolItem(
  options: Partial<RuntimeStreamToolItemView> & {
    sequence: number
    toolCallId: string
    toolName: string
    toolState: RuntimeStreamToolItemView['toolState']
  },
): RuntimeStreamToolItemView {
  const { sequence, ...rest } = options

  return {
    id: `tool:run-1:${sequence}`,
    kind: 'tool',
    runId: 'run-1',
    sequence,
    createdAt: `2026-04-29T00:48:${String(sequence).padStart(2, '0')}Z`,
    detail: null,
    toolSummary: null,
    ...rest,
  }
}

function makeReasoningItem(options: {
  sequence: number
  text: string
}) {
  const detail = options.text.trim() || 'Owned agent reasoning summary updated.'
  return {
    id: `activity:run-1:${options.sequence}`,
    kind: 'activity' as const,
    runId: 'run-1',
    sequence: options.sequence,
    createdAt: `2026-04-29T00:48:${String(options.sequence).padStart(2, '0')}Z`,
    code: 'owned_agent_reasoning',
    title: 'Reasoning',
    text: options.text,
    detail,
  }
}

function setScrollMetrics(
  element: HTMLElement,
  metrics: { scrollTop: number; scrollHeight: number; clientHeight: number },
) {
  Object.defineProperty(element, 'scrollTop', {
    configurable: true,
    writable: true,
    value: metrics.scrollTop,
  })
  Object.defineProperty(element, 'scrollHeight', {
    configurable: true,
    value: metrics.scrollHeight,
  })
  Object.defineProperty(element, 'clientHeight', {
    configurable: true,
    value: metrics.clientHeight,
  })
}

function renderRuntimeStreamItems(runtimeStreamItems: NonNullable<AgentPaneView['runtimeStreamItems']>) {
  return render(
    <AgentRuntime
      agent={makeAgent({
        runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
        runtimeRun: makeRuntimeRun(),
        runtimeStreamStatus: 'live',
        runtimeStreamStatusLabel: 'Live stream',
        runtimeStreamItems,
      })}
    />,
  )
}

describe('AgentRuntime current UI', () => {
  it('classifies conversation scroll positions near the bottom', () => {
    expect(
      isRuntimeConversationNearBottom({
        scrollTop: 500,
        scrollHeight: 1_000,
        clientHeight: 420,
      }),
    ).toBe(true)
    expect(
      isRuntimeConversationNearBottom({
        scrollTop: 300,
        scrollHeight: 1_000,
        clientHeight: 420,
      }),
    ).toBe(false)
    expect(
      isRuntimeConversationNearBottom({
        scrollTop: 0,
        scrollHeight: 320,
        clientHeight: 420,
      }),
    ).toBe(true)
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
      screen.queryByText('Xero has not persisted a bounded autonomous unit history for this project yet.'),
    ).not.toBeInTheDocument()
  })

  it('keeps memory management hidden for the selected agent session', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          project: makeProject({
            selectedAgentSession: {
              projectId: 'project-1',
              agentSessionId: 'agent-session-main',
              title: 'Main chat',
              summary: '',
              status: 'active',
              statusLabel: 'Active',
              selected: true,
              createdAt: '2026-05-01T11:00:00Z',
              updatedAt: '2026-05-01T11:00:00Z',
              archivedAt: null,
              lastRunId: 'run-1',
              lastRuntimeKind: null,
              lastProviderId: null,
              lineage: null,
              isActive: true,
              isArchived: false,
            },
          }),
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          runtimeRun: makeRuntimeRun({ status: 'stopped', isActive: false, isTerminal: true }),
        })}
      />,
    )

    expect(screen.queryByText('Memory')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Approve' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Extract' })).not.toBeInTheDocument()
  })

  it('surfaces failed run diagnostics and starts a replacement run from the composer', async () => {
    const onStartRuntimeRun = vi.fn(async () => makeRuntimeRun({ runId: 'run-2' }))

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun({
            status: 'failed',
            statusLabel: 'Agent failed',
            runtimeLabel: 'OpenAI Codex · Agent failed',
            isActive: false,
            isTerminal: true,
            isFailed: true,
            stoppedAt: '2026-04-29T00:48:02Z',
            lastErrorCode: 'openai_codex_auth_failed',
            lastError: {
              code: 'openai_codex_auth_failed',
              message:
                "Provider 'openai_codex' returned HTTP 401: provider error body redacted because it may contain credential material.",
            },
          }),
          composerModelOptions: [
            {
              selectionKey: 'unscoped::openai_codex',
              profileId: 'unscoped',
              providerId: 'openai_codex' as const,
              providerLabel: 'OpenAI Codex',
              modelId: 'openai_codex',
              displayName: 'openai_codex',
              thinking: { supported: false, effortOptions: [], defaultEffort: null },
              thinkingEffortOptions: [],
              defaultThinkingEffort: null,
            },
          ],
        })}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    expect(screen.getAllByText('Latest saved run failed').length).toBeGreaterThan(0)
    expect(screen.getByText(/Provider 'openai_codex' returned HTTP 401/)).toBeVisible()

    const input = screen.getByLabelText('Agent input')
    expect(input).toBeEnabled()
    expect(input).toHaveAttribute('placeholder', 'Last run failed — send a message to start a fresh one.')

    fireEvent.change(input, { target: { value: '1+1' } })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(onStartRuntimeRun).toHaveBeenCalledWith({
        controls: {
          providerProfileId: 'unscoped',
          runtimeAgentId: 'ask',
          agentDefinitionId: null,
          modelId: 'openai_codex',
          thinkingEffort: null,
          approvalMode: 'suggest',
          planModeRequired: false,
        },
        prompt: '1+1',
      }),
    )
    await waitFor(() => expect(input).toHaveValue(''))
  })

  it('shows a handoff notice when the runtime stream completion reports a same-type handoff', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun({
            status: 'stopped',
            statusLabel: 'Run stopped',
            runtimeLabel: 'Openai Codex · Run stopped',
            isActive: false,
            isTerminal: true,
            stoppedAt: '2026-04-29T00:48:09Z',
          }),
          runtimeStreamStatus: 'complete',
          runtimeStreamStatusLabel: 'Stream complete',
          runtimeStream: {
            projectId: 'project-1',
            agentSessionId: 'agent-session-main',
            runtimeKind: 'openai_codex',
            runId: 'run-1',
            sessionId: 'session-1',
            flowId: null,
            subscribedItemKinds: ['transcript', 'complete'],
            status: 'complete',
            items: [],
            transcriptItems: [],
            toolCalls: [],
            skillItems: [],
            activityItems: [],
            actionRequired: [],
            completion: {
              id: 'complete:run-1:9',
              kind: 'complete',
              runId: 'run-1',
              sequence: 9,
              createdAt: '2026-04-29T00:48:10Z',
              detail: 'Owned agent run handed off to a same-type target run.',
            },
            failure: null,
            lastIssue: null,
            lastItemAt: '2026-04-29T00:48:10Z',
            lastSequence: 9,
          },
          runtimeStreamItems: [
            {
              id: 'transcript:run-1:8',
              kind: 'transcript',
              runId: 'run-1',
              sequence: 8,
              createdAt: '2026-04-29T00:48:09Z',
              role: 'assistant',
              text: 'Saved progress so far.',
            },
          ],
        })}
      />,
    )

    expect(screen.getByText('Run continued in a fresh session')).toBeVisible()
    expect(
      screen.getByText(/handed this conversation off to a new same-type run/i),
    ).toBeVisible()
    expect(screen.queryByText('Latest saved run failed')).not.toBeInTheDocument()
  })

  it('renders runtime stream messages as chronological conversation turns', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Live stream',
          runtimeStreamItems: [
            {
              id: 'transcript:run-1:2',
              kind: 'transcript',
              runId: 'run-1',
              sequence: 2,
              createdAt: '2026-04-29T00:48:03Z',
              role: 'user',
              text: 'What is 1+1?',
            },
            {
              id: 'activity:run-1:3',
              kind: 'activity',
              runId: 'run-1',
              sequence: 3,
              createdAt: '2026-04-29T00:48:04Z',
              code: 'owned_agent_validation_started',
              title: 'Validation started',
              detail: 'Validation started: repo_preflight.',
            },
            {
              id: 'transcript:run-1:4',
              kind: 'transcript',
              runId: 'run-1',
              sequence: 4,
              createdAt: '2026-04-29T00:48:05Z',
              role: 'assistant',
              text: '2',
            },
          ],
        })}
      />,
    )

    expect(screen.getByRole('list', { name: 'Agent conversation turns' })).toBeVisible()
    expect(screen.getByText('What is 1+1?')).toBeVisible()
    expect(screen.getByText('2')).toBeVisible()
    expect(screen.queryByText('Agent feed')).not.toBeInTheDocument()
    expect(screen.queryByText('Validation started')).not.toBeInTheDocument()
  })

  it('shows an agent thinking row immediately while a submitted prompt is starting', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: null,
          runtimeRunActionStatus: 'running',
          pendingRuntimeRunAction: 'start',
        })}
      />,
    )

    expect(screen.getByRole('status', { name: 'Agent is thinking' })).toBeVisible()
    expect(screen.getByText('Thinking')).toBeVisible()
    expect(screen.queryByText(/What can we build together/i)).not.toBeInTheDocument()
  })

  it('pauses auto-follow when the user scrolls away and resumes from the latest button', () => {
    const scrollIntoView = vi.mocked(HTMLElement.prototype.scrollIntoView)
    const initialItems: NonNullable<AgentPaneView['runtimeStreamItems']> = [
      makeTranscriptItem({ sequence: 2, role: 'user', text: 'Walk me through the runtime.' }),
      makeTranscriptItem({ sequence: 3, text: 'Working' }),
    ]

    const { rerender } = render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Live stream',
          runtimeStreamItems: initialItems,
        })}
      />,
    )

    const viewport = screen.getByLabelText('Agent conversation viewport')
    setScrollMetrics(viewport, {
      scrollTop: 0,
      scrollHeight: 1_000,
      clientHeight: 360,
    })
    fireEvent.scroll(viewport)

    expect(screen.getByRole('button', { name: 'Jump to latest' })).toBeVisible()

    scrollIntoView.mockClear()
    rerender(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Live stream',
          runtimeStreamItems: [
            ...initialItems,
            makeTranscriptItem({ sequence: 4, text: ' through the runtime.' }),
          ],
        })}
      />,
    )

    expect(screen.getByText('Working through the runtime.')).toBeVisible()
    expect(scrollIntoView).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Jump to latest' }))

    expect(scrollIntoView).toHaveBeenCalledWith({
      block: 'end',
      inline: 'nearest',
      behavior: 'smooth',
    })
    expect(screen.queryByRole('button', { name: 'Jump to latest' })).not.toBeInTheDocument()
  })

  it('pauses auto-follow immediately when the user wheels upward during streaming', () => {
    const scrollIntoView = vi.mocked(HTMLElement.prototype.scrollIntoView)
    const initialItems: NonNullable<AgentPaneView['runtimeStreamItems']> = [
      makeTranscriptItem({ sequence: 2, role: 'user', text: 'Walk me through the runtime.' }),
      makeTranscriptItem({ sequence: 3, text: 'Working' }),
    ]

    const { rerender } = render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Live stream',
          runtimeStreamItems: initialItems,
        })}
      />,
    )

    const viewport = screen.getByLabelText('Agent conversation viewport')
    setScrollMetrics(viewport, {
      scrollTop: 560,
      scrollHeight: 1_000,
      clientHeight: 360,
    })
    fireEvent.wheel(viewport, { deltaY: -24 })

    expect(screen.getByRole('button', { name: 'Jump to latest' })).toBeVisible()

    scrollIntoView.mockClear()
    rerender(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Live stream',
          runtimeStreamItems: [
            ...initialItems,
            makeTranscriptItem({ sequence: 4, text: ' through the runtime.' }),
          ],
        })}
      />,
    )

    expect(screen.getByText('Working through the runtime.')).toBeVisible()
    expect(scrollIntoView).not.toHaveBeenCalled()
  })

  it('preserves subword streamed assistant transcript deltas exactly', () => {
    renderRuntimeStreamItems([
      makeTranscriptItem({ sequence: 2, text: 'mon' }),
      makeTranscriptItem({ sequence: 3, text: 'om' }),
      makeTranscriptItem({ sequence: 4, text: 'orph' }),
      makeTranscriptItem({ sequence: 5, text: 'ization' }),
    ])

    expect(screen.getByText('monomorphization')).toBeVisible()
    expect(screen.getAllByText('Agent')).toHaveLength(1)
  })

  it('preserves markdown delimiters split across streamed assistant deltas', () => {
    renderRuntimeStreamItems([
      makeTranscriptItem({ sequence: 2, text: '**Native binary' }),
      makeTranscriptItem({ sequence: 3, text: '** Main modules' }),
    ])

    const boldText = screen.getByText('Native binary')

    expect(boldText.tagName).toBe('STRONG')
    expect(boldText.textContent).toBe('Native binary')
    expect(boldText.closest('p')).toHaveTextContent('Native binary Main modules')
  })

  it('preserves inline code split across streamed assistant deltas', () => {
    renderRuntimeStreamItems([
      makeTranscriptItem({ sequence: 2, text: '`mesh' }),
      makeTranscriptItem({ sequence: 3, text: 'c' }),
      makeTranscriptItem({ sequence: 4, text: ' build`' }),
    ])

    const codeText = screen.getByText('meshc build')

    expect(codeText.tagName).toBe('CODE')
    expect(screen.queryByText('mesh c build')).not.toBeInTheDocument()
  })

  it('renders streamed markdown structure after split transcript deltas are reassembled', () => {
    renderRuntimeStreamItems([
      makeTranscriptItem({ sequence: 2, text: '# Pl' }),
      makeTranscriptItem({ sequence: 3, text: 'an\n\n- Keep **bo' }),
      makeTranscriptItem({ sequence: 4, text: 'ld** text\n- Run `pn' }),
      makeTranscriptItem({ sequence: 5, text: 'pm test`\n\n```ts\nconst me' }),
      makeTranscriptItem({ sequence: 6, text: 'ssage = "ok"\n```' }),
    ])

    const conversation = screen.getByRole('list', { name: 'Agent conversation turns' })
    const heading = within(conversation).getByText('Plan')
    const boldText = within(conversation).getByText('bold')
    const inlineCode = within(conversation).getByText('pnpm test')
    const codeBlock = within(conversation).getByText('const message = "ok"')

    expect(heading).toHaveClass('font-semibold')
    expect(boldText.tagName).toBe('STRONG')
    expect(inlineCode.tagName).toBe('CODE')
    expect(codeBlock.tagName).toBe('CODE')
    expect(boldText.closest('li')).toHaveTextContent('Keep bold text')
    expect(inlineCode.closest('li')).toHaveTextContent('Run pnpm test')
  })

  it('renders streamed reasoning activity as an inline thoughts block', () => {
    renderRuntimeStreamItems([
      makeTranscriptItem({ sequence: 2, role: 'user', text: 'Why is the build failing?' }),
      makeReasoningItem({ sequence: 3, text: 'I should inspect the latest build output' }),
      makeReasoningItem({ sequence: 4, text: ' before suggesting a fix.' }),
      makeToolItem({
        sequence: 5,
        toolCallId: 'call-read-build-log',
        toolName: 'read',
        toolState: 'succeeded',
        detail: 'Read build log.',
      }),
      makeTranscriptItem({ sequence: 6, text: 'The build is failing because the generated type is stale.' }),
    ])

    expect(screen.getByText('Thoughts')).toBeVisible()
    expect(screen.getByText('I should inspect the latest build output before suggesting a fix.')).toBeVisible()
    expect(screen.getByText('read')).toBeVisible()
    expect(screen.getByText('The build is failing because the generated type is stale.')).toBeVisible()
  })

  it('keeps consecutive full user transcript items as separate prompts', () => {
    renderRuntimeStreamItems([
      makeTranscriptItem({ sequence: 2, role: 'user', text: 'First prompt.' }),
      makeTranscriptItem({ sequence: 3, role: 'user', text: 'Second prompt.' }),
    ])

    expect(screen.getByText('First prompt.')).toBeVisible()
    expect(screen.getByText('Second prompt.')).toBeVisible()
    expect(screen.getAllByText('You')).toHaveLength(2)
  })

  it('collapses tool state transitions into one compact card with details', () => {
    renderRuntimeStreamItems([
      makeToolItem({
        sequence: 2,
        toolCallId: 'call-read',
        toolName: 'read',
        toolState: 'running',
        detail: 'path: client/components/xero/agent-runtime.tsx, startLine: 1, lineCount: 80',
      }),
      makeToolItem({
        sequence: 3,
        toolCallId: 'call-read',
        toolName: 'read',
        toolState: 'succeeded',
        detail: 'Read 80 line(s) from `client/components/xero/agent-runtime.tsx`.',
        toolSummary: {
          kind: 'file',
          path: 'client/components/xero/agent-runtime.tsx',
          scope: null,
          lineCount: 80,
          matchCount: null,
          truncated: false,
        },
      }),
    ])

    expect(screen.getAllByText('read agent-runtime.tsx')).toHaveLength(1)
    expect(screen.getByText('Succeeded')).toBeVisible()
    expect(screen.queryByText('Running')).not.toBeInTheDocument()
    expect(screen.getByText('Read 80 line(s) from `client/components/xero/agent-runtime.tsx`.')).toBeVisible()
    expect(screen.queryByText('Tool activity recorded.')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show tool details for read agent-runtime\.tsx/i }))

    expect(screen.getByText('Input')).toBeVisible()
    expect(screen.getByText('path: client/components/xero/agent-runtime.tsx, startLine: 1, lineCount: 80')).toBeVisible()
    expect(screen.getByText('Result')).toBeVisible()
    expect(screen.getByText('File result · path client/components/xero/agent-runtime.tsx · 80 lines')).toBeVisible()
  })

  it('uses action plus target labels for search-oriented tool cards', () => {
    renderRuntimeStreamItems([
      makeToolItem({
        sequence: 2,
        toolCallId: 'call-find',
        toolName: 'find',
        toolState: 'running',
        detail: 'pattern: appendTranscriptDelta, path: client/components/xero',
      }),
      makeToolItem({
        sequence: 3,
        toolCallId: 'call-list',
        toolName: 'list',
        toolState: 'running',
        detail: 'path: client/components/xero, maxDepth: 2',
      }),
    ])

    expect(screen.getByText('find appendTranscriptDelta')).toBeVisible()
    expect(screen.getByText('list client/components/xero')).toBeVisible()
  })

  it('groups long tool bursts without evicting the surrounding transcript turns', () => {
    const toolBurst = Array.from({ length: 30 }, (_, index) =>
      makeToolItem({
        sequence: index + 3,
        toolCallId: `call-read-${index}`,
        toolName: 'read',
        toolState: 'succeeded',
        detail: `Read tool ${index}.`,
        toolSummary: {
          kind: 'file',
          path: `client/src/tool-${index}.ts`,
          scope: null,
          lineCount: 12,
          matchCount: null,
          truncated: false,
        },
      }),
    )

    renderRuntimeStreamItems([
      makeTranscriptItem({ sequence: 2, role: 'user', text: 'Please inspect the codebase.' }),
      ...toolBurst,
      makeTranscriptItem({ sequence: 40, role: 'assistant', text: 'Inspection complete.' }),
    ])

    expect(screen.getByText('Please inspect the codebase.')).toBeVisible()
    expect(screen.getByText('Inspection complete.')).toBeVisible()
    expect(screen.getByText('30 tool calls')).toBeVisible()
    expect(screen.queryByText('read tool-0.ts')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /show grouped tool details for 30 tool calls/i }))

    expect(screen.getByText('read tool-0.ts')).toBeVisible()
    expect(screen.getByText('read tool-29.ts')).toBeVisible()
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

  it('renders Debug as an approval-capable composer agent', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedRuntimeAgentId: 'debug',
          selectedRuntimeAgentLabel: 'Debug',
          selectedApprovalMode: 'auto_edit',
        })}
      />,
    )

    expect(screen.getByRole('combobox', { name: 'Agent selector' })).toHaveTextContent('Debug')
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toHaveTextContent('Auto edit')
  })

  it('renders Agent Create as a built-in suggest-only composer agent', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          selectedRuntimeAgentId: 'agent_create',
          selectedRuntimeAgentLabel: 'Agent Create',
          selectedApprovalMode: 'suggest',
        })}
      />,
    )

    expect(screen.getByRole('combobox', { name: 'Agent selector' })).toHaveTextContent('Agent Create')
    expect(screen.queryByRole('combobox', { name: 'Approval mode selector' })).not.toBeInTheDocument()
  })

  it('keeps model selectors available while a prompt is pending on an active run', async () => {
    const onUpdateRuntimeRunControls = vi.fn(async () => makeRuntimeRun())

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1', isSignedOut: false }),
          runtimeRun: makeRuntimeRun(),
          controlTruthSource: 'runtime_run',
          selectedRuntimeAgentId: 'engineer',
          selectedRuntimeAgentLabel: 'Engineer',
          selectedModelSelectionKey: 'anthropic:anthropic/claude-3.5-haiku',
          selectedModelId: 'anthropic/claude-3.5-haiku',
          selectedThinkingEffort: 'low',
          selectedApprovalMode: 'yolo',
          composerModelOptions: [
            {
              selectionKey: 'openai_codex:openai_codex',
              profileId: 'openai_codex-default',
              providerId: 'openai_codex',
              providerLabel: 'OpenAI Codex',
              modelId: 'openai_codex',
              displayName: 'openai_codex',
              thinking: { supported: true, effortOptions: ['medium'], defaultEffort: 'medium' },
              thinkingEffortOptions: ['medium'],
              defaultThinkingEffort: 'medium',
            },
            {
              selectionKey: 'anthropic:anthropic/claude-3.5-haiku',
              profileId: 'anthropic-default',
              providerId: 'anthropic',
              providerLabel: 'Anthropic',
              modelId: 'anthropic/claude-3.5-haiku',
              displayName: 'anthropic/claude-3.5-haiku',
              thinking: { supported: true, effortOptions: ['low'], defaultEffort: 'low' },
              thinkingEffortOptions: ['low'],
              defaultThinkingEffort: 'low',
            },
          ],
          selectedPrompt: {
            text: 'Review the diff before continuing.',
            queuedAt: '2026-04-20T12:05:00Z',
            hasQueuedPrompt: true,
          },
          runtimeRunActiveControls: {
            providerProfileId: null,
            agentDefinitionId: null,
            agentDefinitionVersion: null,
            runtimeAgentId: 'engineer',
            runtimeAgentLabel: 'Engineer',
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
            agentDefinitionId: null,
            agentDefinitionVersion: null,
            runtimeAgentId: 'engineer',
            runtimeAgentLabel: 'Engineer',
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
        onUpdateRuntimeRunControls={onUpdateRuntimeRunControls}
      />,
    )

    expect(screen.getByRole('combobox', { name: 'Model selector' })).toBeEnabled()
    expect(screen.getByRole('combobox', { name: 'Thinking level selector' })).toBeEnabled()
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled()
    expect(screen.queryByText('Queued prompt pending the next model-call boundary.')).not.toBeInTheDocument()
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()

    expect(onUpdateRuntimeRunControls).not.toHaveBeenCalled()
  })

  it.skip('starts a run with the draft prompt and current projected controls, then clears the draft after acknowledgement', async () => {
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
          selectedRuntimeAgentId: 'engineer',
          selectedRuntimeAgentLabel: 'Engineer',
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
            agentDefinitionId: null,
            agentDefinitionVersion: null,
            runtimeAgentId: 'ask',
            runtimeAgentLabel: 'Ask',
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
            agentDefinitionId: null,
            agentDefinitionVersion: null,
            runtimeAgentId: 'ask',
            runtimeAgentLabel: 'Ask',
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

  it.skip('binds a ready provider profile and starts the first run from the send button', async () => {
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
        runtimeLabel: 'OpenRouter · Agent running',
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
      'Ask anything to get started with OpenRouter.',
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
          selectedRuntimeAgentId: 'engineer',
          selectedRuntimeAgentLabel: 'Engineer',
          selectedModelId: 'openai_codex',
          selectedThinkingEffort: 'medium',
          selectedApprovalMode: 'yolo',
          runtimeRunActiveControls: {
            providerProfileId: null,
            agentDefinitionId: null,
            agentDefinitionVersion: null,
            runtimeAgentId: 'engineer',
            runtimeAgentLabel: 'Engineer',
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
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toHaveValue(''))
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

  it('uses the credentials-driven union catalog for the composer model picker when populated', () => {
    const composerModelOptions = [
      {
        selectionKey: 'openrouter:openai/gpt-4.1-mini',
        profileId: 'openrouter-default',
        providerId: 'openrouter' as const,
        providerLabel: 'OpenRouter',
        modelId: 'openai/gpt-4.1-mini',
        displayName: 'GPT-4.1 mini',
        thinking: { supported: false, effortOptions: [], defaultEffort: null },
        thinkingEffortOptions: [],
        defaultThinkingEffort: null,
      },
      {
        selectionKey: 'anthropic:claude-3-7-sonnet-latest',
        profileId: 'anthropic-default',
        providerId: 'anthropic' as const,
        providerLabel: 'Anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        displayName: 'Claude 3.7 Sonnet',
        thinking: { supported: false, effortOptions: [], defaultEffort: null },
        thinkingEffortOptions: [],
        defaultThinkingEffort: null,
      },
    ]
    render(
      <AgentRuntime
        agent={makeAgent({
          composerModelOptions,
          agentRuntimeBlocked: false,
          selectedModel: {
            providerId: 'openrouter',
            providerLabel: 'OpenRouter',
            modelId: 'openai/gpt-4.1-mini',
            hasCredential: true,
            credentialKind: 'api_key',
            source: 'credential_default',
          },
          runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
        })}
      />,
    )

    // The picker should expose the union of credentialed providers.
    expect(screen.queryByText('Configure agent runtime')).not.toBeInTheDocument()
  })

  it('shows the setup empty state when credentials are configured but the chosen model has no credential', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          composerModelOptions: [
            {
              selectionKey: 'openrouter:openai/gpt-4.1-mini',
              profileId: 'openrouter-default',
              providerId: 'openrouter' as const,
              providerLabel: 'OpenRouter',
              modelId: 'openai/gpt-4.1-mini',
              displayName: 'GPT-4.1 mini',
              thinking: { supported: false, effortOptions: [], defaultEffort: null },
              thinkingEffortOptions: [],
              defaultThinkingEffort: null,
            },
          ],
          agentRuntimeBlocked: true,
          selectedModel: {
            providerId: 'anthropic',
            providerLabel: 'Anthropic',
            modelId: 'claude-3-7-sonnet-latest',
            hasCredential: false,
            credentialKind: null,
            source: 'runtime_run',
          },
          runtimeSession: null,
        })}
      />,
    )

    expect(screen.getByText('Configure agent runtime')).toBeVisible()
  })

  describe('multi-pane density and pane controls', () => {
    it('hides the close button when only one pane is open', () => {
      render(
        <AgentRuntime
          agent={makeAgent({ runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }) })}
          paneCount={1}
          paneNumber={1}
        />,
      )

      expect(screen.queryByRole('button', { name: 'Close pane' })).not.toBeInTheDocument()
    })

    it('shows the close button and pane number chip when multiple panes are open', () => {
      const onClose = vi.fn()
      render(
        <AgentRuntime
          agent={makeAgent({ runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }) })}
          paneCount={2}
          paneNumber={2}
          onClosePane={onClose}
        />,
      )

      const closeButton = screen.getByRole('button', { name: 'Close pane' })
      expect(closeButton).toBeVisible()
      expect(screen.getByLabelText('Pane 2')).toHaveTextContent('P2')

      fireEvent.click(closeButton)
      expect(onClose).toHaveBeenCalledTimes(1)
    })

    it('reports close guard state for running runs and unsent composer text', async () => {
      const onPaneCloseStateChange = vi.fn()
      render(
        <AgentRuntime
          agent={makeAgent({
            runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
            runtimeRun: makeRuntimeRun(),
          })}
          paneCount={2}
          paneNumber={1}
          onUpdateRuntimeRunControls={vi.fn()}
          onPaneCloseStateChange={onPaneCloseStateChange}
        />,
      )

      await waitFor(() =>
        expect(onPaneCloseStateChange).toHaveBeenLastCalledWith(
          expect.objectContaining({
            hasRunningRun: true,
            hasUnsavedComposerText: false,
          }),
        ),
      )

      fireEvent.change(screen.getByLabelText('Agent input'), {
        target: { value: 'Please inspect the failing build.' },
      })

      await waitFor(() =>
        expect(onPaneCloseStateChange).toHaveBeenLastCalledWith(
          expect.objectContaining({
            hasRunningRun: true,
            hasUnsavedComposerText: true,
          }),
        ),
      )
    })

    it('keeps non-focused multi-pane runtimes off focused-pane polling paths', async () => {
      const dictation = createDictationAdapter()
      const getSessionContextSnapshot = vi.fn(async () => ({} as never))
      const onComposerControlsChange = vi.fn()
      const agent = makeAgent({
        runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
        runtimeRun: makeRuntimeRun(),
      })
      const desktopAdapter: ComponentProps<typeof AgentRuntime>['desktopAdapter'] = {
        ...dictation.adapter,
        getSessionContextSnapshot,
      }

      const { rerender } = render(
        <AgentRuntime
          agent={agent}
          desktopAdapter={desktopAdapter}
          paneCount={3}
          paneNumber={2}
          isPaneFocused={false}
          onComposerControlsChange={onComposerControlsChange}
        />,
      )

      await act(async () => {
        await Promise.resolve()
        await Promise.resolve()
      })

      expect(dictation.adapter.speechDictationStatus).not.toHaveBeenCalled()
      expect(getSessionContextSnapshot).not.toHaveBeenCalled()
      expect(onComposerControlsChange).not.toHaveBeenCalled()

      rerender(
        <AgentRuntime
          agent={agent}
          desktopAdapter={desktopAdapter}
          paneCount={3}
          paneNumber={2}
          isPaneFocused
          onComposerControlsChange={onComposerControlsChange}
        />,
      )

      await waitFor(() => expect(onComposerControlsChange).toHaveBeenCalledTimes(1))
    })

    it('disables the spawn-pane button when the workspace is at capacity', () => {
      const onSpawn = vi.fn()
      render(
        <AgentRuntime
          agent={makeAgent({ runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }) })}
          paneCount={6}
          paneNumber={6}
          onSpawnPane={onSpawn}
          spawnPaneDisabled
        />,
      )

      const spawnBtn = screen.getByRole('button', { name: 'Pane limit reached' })
      expect(spawnBtn).toBeDisabled()
    })

    it('renders the compact composer variant with a gear popover when density is compact', () => {
      render(
        <AgentRuntime
          agent={makeAgent({
            runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          })}
          density="compact"
          paneCount={3}
          paneNumber={1}
        />,
      )

      // Compact composer: gear popover trigger is visible.
      expect(screen.getByRole('button', { name: 'Composer settings' })).toBeVisible()
      // Comfortable-mode inline thinking selector is hidden in compact mode (lives inside gear popover).
      expect(screen.queryByLabelText('Thinking level selector')).not.toBeInTheDocument()
    })

    it('keeps a focused empty session condensed when exactly three panes are open', () => {
      render(
        <AgentRuntime
          agent={makeAgent({
            runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          })}
          density="compact"
          paneCount={3}
          paneNumber={2}
          isPaneFocused
        />,
      )

      const viewport = screen.getByLabelText('Agent conversation viewport')
      expect(
        screen.queryByRole('heading', { name: /What can we build together/i }),
      ).not.toBeInTheDocument()
      expect(within(viewport).getByRole('heading', { name: 'Xero' })).toBeVisible()
      expect(within(viewport).getByRole('button', { name: 'Explore the codebase' })).toBeVisible()
    })

    it('renders the comfortable composer variant with inline thinking selector when density is comfortable', () => {
      render(
        <AgentRuntime
          agent={makeAgent({
            runtimeSession: makeRuntimeSession({ sessionId: 'session-1' }),
          })}
          density="comfortable"
          paneCount={1}
          paneNumber={1}
        />,
      )

      expect(screen.queryByRole('button', { name: 'Composer settings' })).not.toBeInTheDocument()
      expect(screen.getByLabelText('Thinking level selector')).toBeVisible()
    })
  })
})
