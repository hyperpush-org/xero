import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { openUrlMock } = vi.hoisted(() => ({
  openUrlMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

vi.mock('@/components/xero/workflow-canvas/agent-visualization', () => ({
  AgentVisualization: () => null,
}))

afterEach(() => {
  openUrlMock.mockReset()
})

import { AgentRuntime } from '@/components/xero/agent-runtime'
import { ExecutionView } from '@/components/xero/execution-view'
import { PhaseView } from '@/components/xero/phase-view'
import type {
  AgentPaneView,
  ExecutionPaneView,
  WorkflowPaneView,
} from '@/src/features/xero/use-xero-desktop-state'
import type { AgentProviderModelCatalogView } from '@/src/features/xero/use-xero-desktop-state/types'
import {
  getRuntimeAgentLabel,
  type ProjectDetailView,
  type ProviderModelThinkingEffortDto,
  type RuntimeRunView,
  type RuntimeSessionView,
  type RuntimeStreamView,
} from '@/src/lib/xero-model'
import type { WorkflowAgentDetailDto } from '@/src/lib/xero-model/workflow-agents'

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

function makeWorkflowAgentDetail(
  overrides: Partial<WorkflowAgentDetailDto> = {},
): WorkflowAgentDetailDto {
  return {
    ref: { kind: 'built_in', runtimeAgentId: 'plan', version: 1 },
    header: {
      displayName: 'Plan',
      shortLabel: 'Planning',
      description: 'Turns ambiguous work into an accepted implementation plan.',
      taskPurpose: 'Draft a durable plan before repository mutation.',
      scope: 'built_in',
      lifecycleState: 'active',
      baseCapabilityProfile: 'planning',
      defaultApprovalMode: 'suggest',
      allowedApprovalModes: ['suggest'],
      allowPlanGate: true,
      allowVerificationGate: false,
      allowAutoCompact: true,
    },
    promptPolicy: 'plan',
    toolPolicy: 'planning',
    prompts: [],
    tools: [],
    dbTouchpoints: {
      reads: [],
      writes: [],
      encouraged: [],
    },
    output: {
      contract: 'plan_pack',
      label: 'Plan Pack',
      description: 'Implementation plan output.',
      sections: [],
    },
    consumes: [],
    attachedSkills: [],
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
        summary: 'Owned agent runtime started.',
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
    plan: null,
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

function makeAgent(project = makeProject(), overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  const runtimeSession = overrides.runtimeSession ?? null
  const runtimeRun = overrides.runtimeRun ?? project.runtimeRun ?? null
  const runtimeStream = overrides.runtimeStream ?? null
  const runtimeStreamStatus = overrides.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'
  const runtimeRunControls = runtimeRun?.controls ?? null
  const selectedControls = runtimeRunControls?.selected ?? null
  const providerModelCatalog = overrides.providerModelCatalog ?? makeProviderModelCatalog()
  const selectedModelOption = overrides.selectedModelOption ?? providerModelCatalog.models[0] ?? null
  const selectedRuntimeAgentId = overrides.selectedRuntimeAgentId ?? selectedControls?.runtimeAgentId ?? 'ask'

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
    selectedRuntimeAgentId,
    selectedRuntimeAgentLabel: overrides.selectedRuntimeAgentLabel ?? getRuntimeAgentLabel(selectedRuntimeAgentId),
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
        ? 'Xero recovered a Xero-owned agent run before the live runtime feed resumed.'
        : 'Authenticate and launch a Xero-owned agent run to populate durable app-data run state for this project.'),
    messagesUnavailableReason:
      overrides.messagesUnavailableReason ??
      (runtimeSession?.isAuthenticated
        ? 'Xero authenticated this project, but the live runtime stream has not started yet.'
        : 'Sign in with OpenAI to establish a runtime session for this imported project.'),
    ...overrides,
  }
}

describe('live views', () => {
  it('renders the workflow tab as an interactive canvas', () => {
    render(<PhaseView workflow={makeWorkflow()} />)

    expect(screen.getByLabelText('Workflow canvas')).toBeInTheDocument()
  })

  it('shows the selected agent name in the workflow header', () => {
    render(
      <PhaseView
        workflow={makeWorkflow()}
        agentDetail={makeWorkflowAgentDetail()}
        agentDetailStatus="ready"
        onClearAgentSelection={vi.fn()}
        onCreateAgent={vi.fn()}
      />,
    )

    const selectedAgent = screen.getByLabelText('Selected agent')
    expect(within(selectedAgent).getByRole('img', { name: 'Agent' })).toBeVisible()
    expect(within(selectedAgent).getByText('Plan')).toBeVisible()
    expect(within(selectedAgent).getByText('system')).toBeVisible()
    expect(within(selectedAgent).getByText('Planning')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Close agent inspector' })).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Create workflow' })).not.toBeInTheDocument()
  })

  it('does not render the mock pipeline controls on the workflow tab', () => {
    render(<PhaseView workflow={makeWorkflow()} />)

    expect(screen.queryByText('Xero Desktop')).not.toBeInTheDocument()
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
    expect(screen.getByRole('heading', { name: /What can we build together in Xero/ })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Explore the codebase' })).toBeVisible()
    expect(screen.getByLabelText('Agent input')).toHaveAttribute('placeholder', 'Ask anything to get started with OpenAI Codex.')
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
      target: { value: 'Start the agent run.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))
    await waitFor(() => expect(onStartRuntimeRun).toHaveBeenCalledTimes(1))
  })

  it('renders the editor against the selected project tree', async () => {
    const readProjectFile = vi.fn(async (projectId: string, path: string) => ({
      kind: 'text' as const,
      projectId,
      path,
      byteLength: 7,
      modifiedAt: '2026-01-01T00:00:00Z',
      contentHash: 'test-readme',
      mimeType: 'text/markdown; charset=utf-8',
      rendererKind: 'markdown' as const,
      text: '# Xero\n',
    }))

    render(
      <ExecutionView
        execution={makeExecution()}
        listProjectFiles={async () => ({
          projectId: 'project-1',
          path: '/',
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
          view: {
            rootPath: '/',
            nodesByPath: {
              '/': {
                id: '/',
                name: 'root',
                path: '/',
                type: 'folder',
                childrenLoaded: true,
                truncated: false,
                omittedEntryCount: 0,
              },
              '/README.md': {
                id: '/README.md',
                name: 'README.md',
                path: '/README.md',
                type: 'file',
                childrenLoaded: true,
                truncated: false,
                omittedEntryCount: 0,
              },
              '/src': {
                id: '/src',
                name: 'src',
                path: '/src',
                type: 'folder',
                childrenLoaded: true,
                truncated: false,
                omittedEntryCount: 0,
              },
              '/src/App.tsx': {
                id: '/src/App.tsx',
                name: 'App.tsx',
                path: '/src/App.tsx',
                type: 'file',
                childrenLoaded: true,
                truncated: false,
                omittedEntryCount: 0,
              },
            },
            childPathsByPath: {
              '/': ['/README.md', '/src'],
              '/src': ['/src/App.tsx'],
            },
            loadedPaths: ['/', '/src'],
            stats: {
              byteSize: 1,
              childListCount: 2,
              nodeCount: 4,
              unloadedFolderCount: 0,
            },
            truncated: false,
            omittedEntryCount: 0,
          },
          truncated: false,
          omittedEntryCount: 0,
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
        moveProjectEntry={async (request) => ({
          projectId: request.projectId,
          path:
            request.targetParentPath === '/'
              ? `/${request.path.split('/').filter(Boolean).pop() ?? ''}`
              : `${request.targetParentPath}/${request.path.split('/').filter(Boolean).pop() ?? ''}`,
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
