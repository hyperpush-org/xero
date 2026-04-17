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
import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  PlanningLifecycleView,
  ProjectDetailView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'

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
    name: 'cadence',
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
      rootPath: '/tmp/cadence',
      displayName: 'cadence',
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
  return {
    projectId: 'project-1',
    runId: 'run-1',
    runtimeKind: 'openai_codex',
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
}

function makeRuntimeStream(overrides: Partial<RuntimeStreamView> = {}): RuntimeStreamView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    runId: 'run-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
    status: 'idle',
    items: [],
    transcriptItems: [],
    toolCalls: [],
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

function makeAgent(overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  const project = overrides.project ?? makeProject()
  const runtimeSession = overrides.runtimeSession ?? null
  const runtimeRun = overrides.runtimeRun ?? null
  const runtimeStream = overrides.runtimeStream ?? null
  const runtimeStreamStatus = overrides.runtimeStreamStatus ?? runtimeStream?.status ?? 'idle'

  return {
    project,
    activePhase: null,
    branchLabel: project.branchLabel,
    headShaLabel: project.repository?.headShaLabel ?? 'No HEAD',
    runtimeLabel: runtimeSession?.runtimeLabel ?? project.runtimeLabel,
    repositoryLabel: project.repository?.displayName ?? project.name,
    repositoryPath: project.repository?.rootPath ?? null,
    runtimeSession,
    runtimeRun,
    runtimeErrorMessage: null,
    runtimeRunErrorMessage: null,
    authPhase: runtimeSession?.phase ?? null,
    authPhaseLabel: runtimeSession?.phaseLabel ?? 'Signed out',
    runtimeStream,
    runtimeStreamStatus,
    runtimeStreamStatusLabel: overrides.runtimeStreamStatusLabel ?? 'No live stream',
    runtimeStreamError: overrides.runtimeStreamError ?? null,
    runtimeStreamItems: overrides.runtimeStreamItems ?? [],
    activityItems: overrides.activityItems ?? [],
    actionRequiredItems: overrides.actionRequiredItems ?? [],
    approvalRequests: overrides.approvalRequests ?? project.approvalRequests,
    pendingApprovalCount: overrides.pendingApprovalCount ?? project.pendingApprovalCount,
    latestDecisionOutcome: overrides.latestDecisionOutcome ?? project.latestDecisionOutcome,
    resumeHistory: overrides.resumeHistory ?? project.resumeHistory,
    operatorActionStatus: overrides.operatorActionStatus ?? 'idle',
    pendingOperatorActionId: overrides.pendingOperatorActionId ?? null,
    operatorActionError: overrides.operatorActionError ?? null,
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
    trustSnapshot: null,
    sessionUnavailableReason: overrides.sessionUnavailableReason ?? 'Current session status for this project.',
    runtimeRunUnavailableReason:
      overrides.runtimeRunUnavailableReason ?? 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
    messagesUnavailableReason:
      overrides.messagesUnavailableReason ?? 'Cadence authenticated this project, but the live runtime stream has not started yet.',
    ...overrides,
  }
}

describe('AgentRuntime current UI', () => {
  it('renders an authenticated empty state and starts a run from the footer control', async () => {
    const onStartRuntimeRun = vi.fn(async () => null)

    render(
      <AgentRuntime
        agent={makeAgent({ runtimeSession: makeRuntimeSession() })}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.getByText('No supervised run is attached')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Start run' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Start run' }))
    await waitFor(() => expect(onStartRuntimeRun).toHaveBeenCalledTimes(1))
  })

  it('renders a recovered run snapshot with current checkpoint copy', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession(),
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({ status: 'idle' }),
          runtimeRunUnavailableReason: 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
          messagesUnavailableReason: 'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.',
        })}
      />,
    )

    expect(screen.getAllByRole('heading', { name: 'Recovered run snapshot' }).length).toBeGreaterThanOrEqual(1)
    expect(screen.getByText('Supervisor boot recorded.')).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Waiting for the first run-scoped event' })).toBeVisible()
  })

  it('renders operator approvals and resumes with current labels', async () => {
    const onResolveOperatorAction = vi.fn(async () => undefined)
    const onResumeOperatorRun = vi.fn(async () => undefined)

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession(),
          runtimeRun: makeRuntimeRun(),
          approvalRequests: [
            {
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
              answerShapeKind: 'text',
              answerShapeLabel: 'Required user answer',
              answerShapeHint: 'Describe the operator decision that justifies approval.',
              answerPlaceholder: 'Provide operator input for this action.',
            },
          ],
          pendingApprovalCount: 0,
          resumeHistory: [
            {
              id: 1,
              sourceActionId: 'action-1',
              sessionId: 'session-1',
              status: 'started',
              statusLabel: 'Resume started',
              summary: 'Operator resumed the selected project runtime session.',
              createdAt: '2026-04-13T20:04:00Z',
            },
          ],
        })}
        onResolveOperatorAction={onResolveOperatorAction}
        onResumeOperatorRun={onResumeOperatorRun}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Durable approvals and resume checkpoints' })).toBeVisible()
    expect(screen.getByText('Review worktree changes')).toBeVisible()
    expect(screen.getByText('Latest resume started: Operator resumed the selected project runtime session.')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Resume run' }))
    await waitFor(() =>
      expect(onResumeOperatorRun).toHaveBeenCalledWith('action-1', {
        userAnswer: 'Looks good to resume.',
      }),
    )
  })

  it('keeps the signed-out shell minimal and truthful', () => {
    render(<AgentRuntime agent={makeAgent()} />)

    expect(screen.queryByRole('heading', { name: 'Authenticate to view live agent activity' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute('placeholder', 'Sign in with OpenAI to start.')
    expect(screen.getByText('Signed out')).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
  })
})
