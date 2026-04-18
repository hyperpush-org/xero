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
  RepositoryDiffState,
  WorkflowPaneView,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  PlanningLifecycleView,
  ProjectDetailView,
  RepositoryStatusEntryView,
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

function makeWorkflow(project = makeProject()): WorkflowPaneView {
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

function makeAutonomousRun(overrides: Partial<NonNullable<ProjectDetailView['autonomousRun']>> = {}) {
  return {
    projectId: 'project-1',
    runId: 'auto-run-1',
    runtimeKind: 'openai_codex',
    runtimeLabel: 'Openai Codex · Autonomous run active',
    supervisorKind: 'detached_pty',
    supervisorLabel: 'Detached Pty',
    status: 'running' as const,
    statusLabel: 'Autonomous run active',
    recoveryState: 'recovery_required' as const,
    recoveryLabel: 'Recovery required',
    activeUnitId: 'auto-run-1:checkpoint:2',
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
      retryable: true,
    },
    updatedAt: '2026-04-16T20:03:00Z',
    isActive: true,
    needsRecovery: true,
    isTerminal: false,
    isFailed: true,
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
    summary: 'Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.',
    boundaryId: 'checkpoint:2',
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

function makeAgent(project = makeProject(), overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  const runtimeSession = overrides.runtimeSession ?? null
  const runtimeRun = overrides.runtimeRun ?? project.runtimeRun ?? null
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
    runtimeStreamStatusLabel: overrides.runtimeStreamStatusLabel ?? getStreamStatusLabel(runtimeStreamStatus),
    runtimeStreamError: overrides.runtimeStreamError ?? runtimeStream?.lastIssue ?? null,
    runtimeStreamItems: overrides.runtimeStreamItems ?? runtimeStream?.items ?? [],
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

function makeExecution(project = makeProject(), statusEntries: RepositoryStatusEntryView[] = []): ExecutionPaneView {
  return {
    project,
    activePhase: null,
    branchLabel: project.branchLabel,
    headShaLabel: project.repository?.headShaLabel ?? 'No HEAD',
    statusEntries,
    statusCount: statusEntries.length,
    hasChanges: statusEntries.length > 0,
    diffScopes: [
      { scope: 'staged', label: 'Staged', count: statusEntries.filter((entry) => entry.staged !== null).length },
      { scope: 'unstaged', label: 'Unstaged', count: statusEntries.filter((entry) => entry.unstaged !== null).length },
      { scope: 'worktree', label: 'Worktree', count: statusEntries.length },
    ],
    verificationRecords: project.verificationRecords,
    resumeHistory: project.resumeHistory,
    latestDecisionOutcome: project.latestDecisionOutcome,
    notificationBroker: project.notificationBroker,
    operatorActionError: null,
    verificationUnavailableReason: 'Verification results will appear here once this project records durable verification outcomes or resume history.',
  }
}

function makeDiffState(overrides: Partial<RepositoryDiffState> = {}): RepositoryDiffState {
  return {
    status: 'ready',
    diff: {
      projectId: 'project-1',
      repositoryId: 'repo-1',
      scope: 'unstaged',
      patch: '',
      isEmpty: true,
      truncated: false,
      baseRevisionLabel: 'Working tree',
    },
    errorMessage: null,
    projectId: 'project-1',
    ...overrides,
  }
}

describe('live views', () => {
  it('renders the current empty workflow state', () => {
    render(<PhaseView workflow={makeWorkflow()} />)

    expect(screen.getByText('Milestone')).toBeVisible()
    expect(screen.getByText('M001')).toBeVisible()
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

    expect(screen.queryByRole('heading', { name: 'Authenticate to view live agent activity' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute('placeholder', 'Sign in with OpenAI to start.')
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
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Start or reconnect a supervised run to create the run-scoped live feed for this imported project.',
    )

    fireEvent.click(screen.getByRole('button', { name: 'Start run' }))
    await waitFor(() => expect(onStartRuntimeRun).toHaveBeenCalledTimes(1))
  })

  it('renders the recovered autonomous ledger as a first-class desktop truth surface', () => {
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
                retryable: true,
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
                retryable: true,
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

    expect(screen.getByRole('heading', { name: 'Autonomous run truth' })).toBeVisible()
    expect(
      screen.getByText(
        'Cadence is projecting the durable autonomous run and active unit boundary separately from the live runtime/session feed.',
      ),
    ).toBeVisible()
    expect(screen.getByText('Current autonomous boundary')).toBeVisible()
    expect(
      screen.getByText('Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.'),
    ).toBeVisible()
    expect(screen.getByText('Duplicate start prevented')).toBeVisible()
    expect(screen.getByText('run: auto-run-1')).toBeVisible()
    expect(screen.getAllByRole('heading', { name: 'Recovered run snapshot' }).length).toBeGreaterThanOrEqual(1)
  })

  it('renders recovered runtime and durable operator controls with the current headings', async () => {
    const resolveOperatorAction = vi.fn(async () => undefined)
    const resumeOperatorRun = vi.fn(async () => undefined)

    const project = makeProject({
      approvalRequests: [
        {
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
        {
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
      ],
      pendingApprovalCount: 1,
      resumeHistory: [
        {
          id: 2,
          sourceActionId: 'action-approved',
          sessionId: 'session-1',
          status: 'started',
          statusLabel: 'Resume started',
          summary: 'Operator resumed the selected project runtime session.',
          createdAt: '2026-04-13T20:04:00Z',
        },
      ],
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
          runtimeRunUnavailableReason: 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
          messagesUnavailableReason: 'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.',
        })}
        onResolveOperatorAction={resolveOperatorAction}
        onResumeOperatorRun={resumeOperatorRun}
      />,
    )

    expect(screen.getAllByRole('heading', { name: 'Recovered run snapshot' }).length).toBeGreaterThanOrEqual(1)
    expect(screen.getByRole('heading', { name: 'Waiting for the first run-scoped event' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Durable approvals and resume checkpoints' })).toBeVisible()
    expect(screen.getByText('Supervisor boot recorded.')).toBeVisible()
    expect(screen.getByText('Review worktree changes')).toBeVisible()
    expect(screen.getByText('Latest resume started: Operator resumed the selected project runtime session.')).toBeVisible()

    fireEvent.change(screen.getByLabelText('Operator answer for action-pending'), {
      target: { value: 'Proceed after validating repo changes.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Approve' }))

    await waitFor(() =>
      expect(resolveOperatorAction).toHaveBeenCalledWith('action-pending', 'approve', {
        userAnswer: 'Proceed after validating repo changes.',
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Resume run' }))
    await waitFor(() =>
      expect(resumeOperatorRun).toHaveBeenCalledWith('action-approved', {
        userAnswer: 'Looks good to resume.',
      }),
    )
  })

  it('renders the current execution empty and error states', () => {
    const onSelectDiffScope = vi.fn()
    const onRetryDiff = vi.fn()

    const { rerender } = render(
      <ExecutionView
        activeDiff={makeDiffState()}
        activeDiffScope="unstaged"
        execution={makeExecution()}
        onRetryDiff={onRetryDiff}
        onSelectDiffScope={onSelectDiffScope}
      />,
    )

    expect(screen.getByText('No execution activity yet')).toBeVisible()
    expect(
      screen.getByText('Execution activity will appear here once this project records live run output or backend execution views become available.'),
    ).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Changes' }))
    expect(onSelectDiffScope).toHaveBeenCalledWith('unstaged')
    expect(screen.getByText('No unstaged changes')).toBeVisible()
    expect(screen.getByText('Working tree is clean for this scope.')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Verify' }))
    expect(screen.getByText('No verification activity yet')).toBeVisible()

    rerender(
      <ExecutionView
        activeDiff={makeDiffState({ status: 'error', diff: null, errorMessage: 'diff failed', projectId: 'project-1' })}
        activeDiffScope="worktree"
        execution={makeExecution(makeProject(), [
          { path: 'client/src/App.tsx', staged: null, unstaged: 'modified', untracked: false },
        ])}
        onRetryDiff={onRetryDiff}
        onSelectDiffScope={onSelectDiffScope}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Changes' }))
    expect(screen.getByText('Failed to load diff')).toBeVisible()
    expect(screen.getByText('diff failed')).toBeVisible()
  })
})
