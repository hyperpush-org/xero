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
import type {
  AgentPaneView,
  AgentTrustSnapshotView,
} from '@/src/features/cadence/use-cadence-desktop-state'
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
    updatedAt: '2026-04-15T20:00:49Z',
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

function makeRuntimeStream(overrides: Partial<RuntimeStreamView> = {}): RuntimeStreamView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    runId: 'run-1',
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
    status: 'live',
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

function makeTrustSnapshot(overrides: Partial<AgentTrustSnapshotView> = {}): AgentTrustSnapshotView {
  return {
    state: 'healthy',
    stateLabel: 'Healthy',
    runtimeState: 'healthy',
    runtimeReason: 'Durable runtime run + authenticated session are both healthy.',
    streamState: 'healthy',
    streamReason: 'Runtime event stream is connected and delivering operator-visible activity.',
    approvalsState: 'healthy',
    approvalsReason: 'No pending operator approvals are blocking autonomous continuation.',
    routesState: 'healthy',
    routesReason: 'Notification route health is stable for configured channels.',
    credentialsState: 'healthy',
    credentialsReason: 'All enabled routes report fully configured app-local credentials.',
    syncState: 'healthy',
    syncReason: 'Latest notification adapter sync cycle completed without failed dispatches or rejected replies.',
    routeCount: 2,
    enabledRouteCount: 2,
    degradedRouteCount: 0,
    readyCredentialRouteCount: 2,
    missingCredentialRouteCount: 0,
    malformedCredentialRouteCount: 0,
    unavailableCredentialRouteCount: 0,
    pendingApprovalCount: 0,
    syncDispatchFailedCount: 0,
    syncReplyRejectedCount: 0,
    routeError: null,
    syncError: null,
    projectionError: null,
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
    runtimeStreamStatusLabel: 'No live stream',
    runtimeStreamError: overrides.runtimeStreamError ?? runtimeStream?.lastIssue ?? null,
    runtimeStreamItems: overrides.runtimeStreamItems ?? runtimeStream?.items ?? [],
    activityItems: overrides.activityItems ?? runtimeStream?.activityItems ?? [],
    actionRequiredItems: overrides.actionRequiredItems ?? runtimeStream?.actionRequired ?? [],
    approvalRequests: [],
    pendingApprovalCount: 0,
    latestDecisionOutcome: null,
    resumeHistory: [],
    operatorActionStatus: 'idle',
    pendingOperatorActionId: null,
    operatorActionError: null,
    runtimeRunActionStatus: 'idle',
    pendingRuntimeRunAction: null,
    runtimeRunActionError: null,
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

describe('AgentRuntime run controls', () => {
  it('renders a startable empty state for an authenticated project with no durable run yet', async () => {
    const onStartRuntimeRun = vi.fn(async () => makeRuntimeRun())

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            accountId: 'acct-1',
            accountLabel: 'acct-1',
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
          runtimeRun: null,
          runtimeRunUnavailableReason: 'No durable supervised runtime run is recorded for this project yet.',
        })}
        onStartRuntimeRun={onStartRuntimeRun}
      />,
    )

    expect(screen.getByRole('heading', { name: 'No durable runtime run yet' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.getByText('No supervised run is attached')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Start supervised run' })).toBeEnabled()
    expect(screen.queryByRole('button', { name: 'Stop run' })).not.toBeInTheDocument()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Start or reconnect a supervised run to create the run-scoped live feed for this imported project.',
    )

    fireEvent.click(screen.getByRole('button', { name: 'Start supervised run' }))

    await waitFor(() => expect(onStartRuntimeRun).toHaveBeenCalledTimes(1))
  })

  it('renders a healthy operator trust snapshot with explicit permission/storage boundaries', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            accountId: 'acct-1',
            accountLabel: 'acct-1',
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
          runtimeStream: makeRuntimeStream({ status: 'live' }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
          trustSnapshot: makeTrustSnapshot(),
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Consolidated trust surface for unattended operation' })).toBeVisible()
    expect(screen.getByText('Permission scope')).toBeVisible()
    expect(screen.getByText('Storage boundaries')).toBeVisible()
    expect(
      screen.getByText(
        'Durable runtime + operator state stays bound to the selected repository, while notification credentials remain app-local and never render raw secret values here.',
      ),
    ).toBeVisible()
    expect(screen.getByText('Runtime liveness')).toBeVisible()
    expect(screen.getByText('Approval backlog')).toBeVisible()
    expect(screen.getByText('Credential readiness')).toBeVisible()
    expect(screen.getByText('Latest sync diagnostics')).toBeVisible()
    expect(screen.getByText('Trust snapshot is green')).toBeVisible()
    expect(screen.queryByRole('heading', { name: 'Operator recovery actions' })).not.toBeInTheDocument()
  })

  it('renders degraded trust states with concrete recovery actions and inspectable error codes', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession(),
          runtimeRun: makeRuntimeRun({
            status: 'stale',
            statusLabel: 'Supervisor stale',
            isActive: false,
            isTerminal: false,
            isStale: true,
            isFailed: false,
            lastErrorCode: 'runtime_supervisor_connect_failed',
          }),
          runtimeStream: makeRuntimeStream({ status: 'stale' }),
          runtimeStreamStatus: 'stale',
          runtimeStreamStatusLabel: 'Stream stale',
          trustSnapshot: makeTrustSnapshot({
            state: 'degraded',
            stateLabel: 'Needs attention',
            runtimeState: 'degraded',
            runtimeReason: 'The durable runtime-run record is stale and needs operator review.',
            approvalsState: 'degraded',
            approvalsReason: 'There are 2 pending operator approval gate(s) waiting for action.',
            pendingApprovalCount: 2,
            routesState: 'degraded',
            routesReason: '1 route(s) show degraded or pending dispatch health.',
            routeCount: 2,
            degradedRouteCount: 1,
            credentialsState: 'degraded',
            credentialsReason: '1 enabled route(s) are missing required app-local credentials.',
            readyCredentialRouteCount: 1,
            missingCredentialRouteCount: 1,
            syncState: 'degraded',
            syncReason: 'Latest sync cycle reported 1 failed dispatch(es) and 0 rejected replies.',
            syncDispatchFailedCount: 1,
            routeError: {
              code: 'notification_route_refresh_failed',
              message: 'Cadence could not refresh notification route health for this project.',
              retryable: true,
            },
            projectionError: {
              code: 'trust_projection_contract_mismatch',
              message: 'Cadence ignored malformed trust metadata and kept the last truthful snapshot visible.',
              retryable: true,
            },
          }),
        })}
      />,
    )

    expect(screen.getByText('Operator recovery actions')).toBeVisible()
    const recoveryList = screen.getByLabelText('Operator trust recovery actions')
    expect(within(recoveryList).getByText('Sign in with OpenAI from Runtime setup before trusting autonomous execution.')).toBeVisible()
    expect(within(recoveryList).getByText('Start or reconnect the supervised run so runtime liveness is durable and current.')).toBeVisible()
    expect(within(recoveryList).getByText('Resolve pending operator approvals so autonomous continuation is no longer blocked.')).toBeVisible()
    expect(
      within(recoveryList).getByText('Configure missing or malformed app-local route credentials in Route editor before dispatch.'),
    ).toBeVisible()
    expect(within(recoveryList).getByText('Refresh route health from Notification channels & routes after credential updates.')).toBeVisible()
    expect(within(recoveryList).getByText('Inspect error code trust_projection_contract_mismatch before considering this project trust state healthy.')).toBeVisible()
    expect(screen.getByText('Trust projection fallback is active')).toBeVisible()
    expect(screen.getByText('code: trust_projection_contract_mismatch')).toBeVisible()
  })

  it('renders replay-aware transcript, activity, and tool lanes with run-scoped diagnostics', () => {
    const runtimeSession = makeRuntimeSession({
      phase: 'authenticated',
      phaseLabel: 'Authenticated',
      runtimeLabel: 'Openai Codex · Authenticated',
      accountId: 'acct-1',
      accountLabel: 'acct-1',
      sessionId: 'session-1',
      sessionLabel: 'session-1',
      lastErrorCode: null,
      lastError: null,
      isAuthenticated: true,
      isLoginInProgress: false,
      needsManualInput: false,
      isSignedOut: false,
      isFailed: false,
    })
    const runtimeStream = makeRuntimeStream({
      status: 'replaying',
      lastSequence: 7,
      lastItemAt: '2026-04-15T20:00:07Z',
      items: [
        {
          id: 'transcript:run-1:5',
          kind: 'transcript',
          runId: 'run-1',
          sequence: 5,
          createdAt: '2026-04-15T20:00:05Z',
          text: 'Recovered transcript line.',
        },
        {
          id: 'activity:run-1:6',
          kind: 'activity',
          runId: 'run-1',
          sequence: 6,
          createdAt: '2026-04-15T20:00:06Z',
          code: 'stream.replay',
          title: 'Replay backlog attached',
          detail: '',
        },
        {
          id: 'tool:run-1:7',
          kind: 'tool',
          runId: 'run-1',
          sequence: 7,
          createdAt: '2026-04-15T20:00:07Z',
          toolCallId: 'inspect_repository_context',
          toolName: 'inspect_repository_context',
          toolState: 'running',
          detail: 'Rehydrating repository context.',
        },
      ],
      transcriptItems: [
        {
          id: 'transcript:run-1:5',
          kind: 'transcript',
          runId: 'run-1',
          sequence: 5,
          createdAt: '2026-04-15T20:00:05Z',
          text: 'Recovered transcript line.',
        },
      ],
      activityItems: [
        {
          id: 'activity:run-1:6',
          kind: 'activity',
          runId: 'run-1',
          sequence: 6,
          createdAt: '2026-04-15T20:00:06Z',
          code: 'stream.replay',
          title: 'Replay backlog attached',
          detail: null,
        },
      ],
      toolCalls: [
        {
          id: 'tool:run-1:7',
          kind: 'tool',
          runId: 'run-1',
          sequence: 7,
          createdAt: '2026-04-15T20:00:07Z',
          toolCallId: 'inspect_repository_context',
          toolName: 'inspect_repository_context',
          toolState: 'running',
          detail: 'Rehydrating repository context.',
        },
      ],
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession,
          runtimeRun: makeRuntimeRun(),
          runtimeStream,
          runtimeStreamStatus: 'replaying',
          runtimeStreamStatusLabel: '   ',
          messagesUnavailableReason:
            'Cadence is replaying recent run-scoped activity while the live runtime stream catches up for this selected project.',
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Replaying recent run-scoped activity' })).toBeVisible()
    expect(screen.getByText('Replaying recent run-scoped backlog')).toBeVisible()
    expect(screen.getByText('Runtime activity')).toBeVisible()
    expect(screen.getByText('Replay backlog attached')).toBeVisible()
    expect(screen.getByText('Cadence recorded this activity without additional detail.')).toBeVisible()
    expect(screen.getByText('Recovered transcript line.')).toBeVisible()
    expect(screen.getByText('inspect_repository_context')).toBeVisible()
    expect(screen.getAllByText('#7').length).toBeGreaterThan(0)
    expect(screen.getAllByText('run-1').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Replaying recent activity').length).toBeGreaterThan(0)
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Cadence is replaying recent run-scoped activity for run-1 while the live feed catches up.',
    )
  })

  it('surfaces same-session run replacement diagnostics until the new run produces fresh items', () => {
    const runtimeSession = makeRuntimeSession({
      phase: 'authenticated',
      phaseLabel: 'Authenticated',
      runtimeLabel: 'Openai Codex · Authenticated',
      accountId: 'acct-1',
      accountLabel: 'acct-1',
      sessionId: 'session-1',
      sessionLabel: 'session-1',
      lastErrorCode: null,
      lastError: null,
      isAuthenticated: true,
      isLoginInProgress: false,
      needsManualInput: false,
      isSignedOut: false,
      isFailed: false,
    })

    const { rerender } = render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession,
          runtimeRun: makeRuntimeRun({ runId: 'run-1' }),
          runtimeStream: makeRuntimeStream({
            runId: 'run-1',
            status: 'live',
            lastSequence: 3,
            items: [
              {
                id: 'transcript:run-1:3',
                kind: 'transcript',
                runId: 'run-1',
                sequence: 3,
                createdAt: '2026-04-15T20:00:03Z',
                text: 'First run transcript.',
              },
            ],
            transcriptItems: [
              {
                id: 'transcript:run-1:3',
                kind: 'transcript',
                runId: 'run-1',
                sequence: 3,
                createdAt: '2026-04-15T20:00:03Z',
                text: 'First run transcript.',
              },
            ],
          }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
        })}
      />,
    )

    expect(screen.getByText('First run transcript.')).toBeVisible()

    rerender(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession,
          runtimeRun: makeRuntimeRun({ runId: 'run-2', updatedAt: '2026-04-15T20:01:00Z' }),
          runtimeStream: makeRuntimeStream({
            runId: 'run-2',
            status: 'subscribing',
            items: [],
            transcriptItems: [],
            activityItems: [],
            toolCalls: [],
            lastSequence: null,
          }),
          runtimeStreamStatus: 'subscribing',
          runtimeStreamStatusLabel: 'Connecting stream',
          messagesUnavailableReason:
            'Cadence is connecting the live runtime stream for this selected project.',
        })}
      />,
    )

    expect(screen.getByText('Switched to a new supervised run')).toBeVisible()
    expect(screen.getAllByText(/run-1/).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/run-2/).length).toBeGreaterThan(0)
    expect(screen.getByText('Connecting to the live transcript')).toBeVisible()

    rerender(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession,
          runtimeRun: makeRuntimeRun({ runId: 'run-2', updatedAt: '2026-04-15T20:01:02Z' }),
          runtimeStream: makeRuntimeStream({
            runId: 'run-2',
            status: 'live',
            lastSequence: 1,
            items: [
              {
                id: 'transcript:run-2:1',
                kind: 'transcript',
                runId: 'run-2',
                sequence: 1,
                createdAt: '2026-04-15T20:01:02Z',
                text: 'Second run transcript.',
              },
            ],
            transcriptItems: [
              {
                id: 'transcript:run-2:1',
                kind: 'transcript',
                runId: 'run-2',
                sequence: 1,
                createdAt: '2026-04-15T20:01:02Z',
                text: 'Second run transcript.',
              },
            ],
          }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
        })}
      />,
    )

    expect(screen.queryByText('Switched to a new supervised run')).not.toBeInTheDocument()
    expect(screen.getByText('Second run transcript.')).toBeVisible()
  })

  it('renders stale recovered diagnostics with reconnect and stop controls even when auth is signed out', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession(),
          runtimeRun: makeRuntimeRun({
            status: 'stale',
            statusLabel: '',
            transport: {
              kind: 'tcp',
              endpoint: '127.0.0.1:4455',
              liveness: 'unreachable',
              livenessLabel: 'Control unreachable',
            },
            checkpoints: [
              {
                sequence: 3,
                kind: 'state',
                kindLabel: 'State',
                summary: '',
                createdAt: '2026-04-15T20:08:00Z',
              },
            ],
            latestCheckpoint: {
              sequence: 3,
              kind: 'state',
              kindLabel: 'State',
              summary: '',
              createdAt: '2026-04-15T20:08:00Z',
            },
            checkpointCount: 1,
            hasCheckpoints: true,
            isActive: false,
            isTerminal: false,
            isStale: true,
            isFailed: false,
          }),
          runtimeRunUnavailableReason:
            'Cadence recovered a stale supervised harness run. The durable checkpoint trail is still available even though the control endpoint is no longer reachable.',
        })}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
        onStopRuntimeRun={vi.fn(async () => makeRuntimeRun({ status: 'stopped', statusLabel: 'Run stopped' }))}
      />,
    )

    expect(screen.getByText('Supervisor heartbeat is stale')).toBeVisible()
    expect(screen.getAllByText('Supervisor stale').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Durable checkpoint recorded.').length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'Reconnect supervisor' })).toBeEnabled()
    expect(screen.getByRole('button', { name: 'Stop run' })).toBeVisible()
  })

  it('renders stopped runs as terminal history with a restart action instead of stop controls', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            lastErrorCode: null,
            lastError: null,
            isAuthenticated: true,
            isLoginInProgress: false,
            needsManualInput: false,
            isSignedOut: false,
            isFailed: false,
          }),
          runtimeRun: makeRuntimeRun({
            status: 'stopped',
            statusLabel: 'Run stopped',
            stoppedAt: '2026-04-15T20:10:00Z',
            updatedAt: '2026-04-15T20:10:00Z',
            isActive: false,
            isTerminal: true,
            isStale: false,
            isFailed: false,
          }),
          runtimeRunUnavailableReason:
            'Cadence recovered a stopped supervised harness run. Final checkpoints remain available for inspection after reload.',
        })}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
      />,
    )

    expect(screen.getByText('Supervisor stopped cleanly')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Start new supervised run' })).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Stop run' })).not.toBeInTheDocument()
  })

  it('refuses to render an incomplete durable run payload as a healthy snapshot', () => {
    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            lastErrorCode: null,
            lastError: null,
            isAuthenticated: true,
            isLoginInProgress: false,
            needsManualInput: false,
            isSignedOut: false,
            isFailed: false,
          }),
          runtimeRun: makeRuntimeRun({
            runId: '   ',
            status: 'running',
            statusLabel: 'Supervisor running',
          }),
        })}
        onStartRuntimeRun={vi.fn(async () => makeRuntimeRun())}
        onStopRuntimeRun={vi.fn(async () => makeRuntimeRun())}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Durable run snapshot unavailable' })).toBeVisible()
    expect(screen.getByText('Durable run snapshot is incomplete')).toBeVisible()
    expect(screen.queryByText('Recovered run snapshot')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Stop run' })).not.toBeInTheDocument()
  })

  it('surfaces run-control failures without hiding the last truthful checkpoint history', async () => {
    const onStopRuntimeRun = vi.fn(async () => {
      throw new Error('runtime stop timed out')
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          runtimeSession: makeRuntimeSession({
            phase: 'authenticated',
            phaseLabel: 'Authenticated',
            runtimeLabel: 'Openai Codex · Authenticated',
            lastErrorCode: null,
            lastError: null,
            isAuthenticated: true,
            isLoginInProgress: false,
            needsManualInput: false,
            isSignedOut: false,
            isFailed: false,
          }),
          runtimeRun: makeRuntimeRun(),
        })}
        onStopRuntimeRun={onStopRuntimeRun}
      />,
    )

    expect(screen.getAllByText('Recovered repository context before reconnecting the live feed.').length).toBeGreaterThan(0)

    fireEvent.click(screen.getByRole('button', { name: 'Stop run' }))

    await waitFor(() => expect(onStopRuntimeRun).toHaveBeenCalledWith('run-1'))
    expect(screen.getByText('Run control failed')).toBeVisible()
    expect(screen.getByText('runtime stop timed out')).toBeVisible()
    expect(screen.getAllByText('Recovered repository context before reconnecting the live feed.').length).toBeGreaterThan(0)
  })

  it('enforces required-answer parity for gate-linked/runtime-resumable approvals and keeps optional approvals permissive', async () => {
    const runtimeSession = makeRuntimeSession({
      phase: 'authenticated',
      phaseLabel: 'Authenticated',
      runtimeLabel: 'Openai Codex · Authenticated',
      accountId: 'acct-1',
      accountLabel: 'acct-1',
      sessionId: 'session-1',
      sessionLabel: 'session-1',
      lastErrorCode: null,
      lastError: null,
      isAuthenticated: true,
      isLoginInProgress: false,
      needsManualInput: false,
      isSignedOut: false,
      isFailed: false,
    })

    const gatePendingApproval = {
      actionId: 'gate-pending',
      sessionId: 'session-1',
      flowId: 'flow-1',
      actionType: 'review_worktree',
      title: 'Review pending gate answer',
      detail: 'Inspect the repository diff before continuing.',
      gateNodeId: 'workflow-research',
      gateKey: 'requires_user_input',
      transitionFromNodeId: 'workflow-discussion',
      transitionToNodeId: 'workflow-research',
      transitionKind: 'advance',
      userAnswer: null,
      status: 'pending' as const,
      statusLabel: 'Pending approval',
      decisionNote: null,
      createdAt: '2026-04-16T13:00:00Z',
      updatedAt: '2026-04-16T13:00:00Z',
      resolvedAt: null,
      isPending: true,
      isResolved: false,
      canResume: false,
      isGateLinked: true,
      isRuntimeResumable: false,
      requiresUserAnswer: true,
      answerRequirementReason: 'gate_linked' as const,
      answerRequirementLabel: 'Required — gate-linked approvals need a non-empty user answer before approval.',
      answerShapeKind: 'plain_text' as const,
      answerShapeLabel: 'Worktree review rationale',
      answerShapeHint: 'Summarize why the repository diff is safe to proceed.',
      answerPlaceholder: 'Summarize the worktree review rationale that justifies approval.',
    }

    const runtimePendingApproval = {
      actionId: 'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required',
      sessionId: 'session-1',
      flowId: 'flow-1',
      actionType: 'terminal_input_required',
      title: 'Provide terminal input for runtime resume',
      detail: 'Supply terminal input text so the runtime can resume this boundary.',
      gateNodeId: null,
      gateKey: null,
      transitionFromNodeId: null,
      transitionToNodeId: null,
      transitionKind: null,
      userAnswer: null,
      status: 'pending' as const,
      statusLabel: 'Pending approval',
      decisionNote: null,
      createdAt: '2026-04-16T13:00:30Z',
      updatedAt: '2026-04-16T13:00:30Z',
      resolvedAt: null,
      isPending: true,
      isResolved: false,
      canResume: false,
      isGateLinked: false,
      isRuntimeResumable: true,
      requiresUserAnswer: true,
      answerRequirementReason: 'runtime_resumable' as const,
      answerRequirementLabel: 'Required — runtime-resumable approvals need a non-empty user answer before approval.',
      answerShapeKind: 'terminal_input' as const,
      answerShapeLabel: 'Terminal input text',
      answerShapeHint: 'Provide the exact non-empty terminal input text that should be submitted on resume.',
      answerPlaceholder: 'Type the exact terminal input response to submit on resume.',
    }

    const optionalPendingApproval = {
      actionId: 'optional-pending',
      sessionId: 'session-1',
      flowId: 'flow-1',
      actionType: 'confirm_resume',
      title: 'Optional follow-up context',
      detail: 'This action can be approved without additional answer text.',
      gateNodeId: null,
      gateKey: null,
      transitionFromNodeId: null,
      transitionToNodeId: null,
      transitionKind: null,
      userAnswer: null,
      status: 'pending' as const,
      statusLabel: 'Pending approval',
      decisionNote: null,
      createdAt: '2026-04-16T13:01:00Z',
      updatedAt: '2026-04-16T13:01:00Z',
      resolvedAt: null,
      isPending: true,
      isResolved: false,
      canResume: false,
      isGateLinked: false,
      isRuntimeResumable: false,
      requiresUserAnswer: false,
      answerRequirementReason: 'optional' as const,
      answerRequirementLabel: 'Optional — this action can be approved or rejected without a user answer.',
      answerShapeKind: 'plain_text' as const,
      answerShapeLabel: 'Resume confirmation note',
      answerShapeHint: 'Optional plain-text context for this decision.',
      answerPlaceholder: 'Optional plain-text context for this resume confirmation.',
    }

    const approvedApproval = {
      actionId: 'gate-approved',
      sessionId: 'session-1',
      flowId: 'flow-1',
      actionType: 'review_worktree',
      title: 'Resume approved gate action',
      detail: 'Resume after the gate answer was approved.',
      gateNodeId: 'workflow-research',
      gateKey: 'requires_user_input',
      transitionFromNodeId: 'workflow-discussion',
      transitionToNodeId: 'workflow-research',
      transitionKind: 'advance',
      userAnswer: 'Proceed after validating repo changes.',
      status: 'approved' as const,
      statusLabel: 'Approved',
      decisionNote: 'Looks good to resume.',
      createdAt: '2026-04-16T13:01:00Z',
      updatedAt: '2026-04-16T13:02:00Z',
      resolvedAt: '2026-04-16T13:02:00Z',
      isPending: false,
      isResolved: true,
      canResume: true,
      isGateLinked: true,
      isRuntimeResumable: false,
      requiresUserAnswer: true,
      answerRequirementReason: 'gate_linked' as const,
      answerRequirementLabel: 'Required — gate-linked approvals need a non-empty user answer before approval.',
      answerShapeKind: 'plain_text' as const,
      answerShapeLabel: 'Worktree review rationale',
      answerShapeHint: 'Summarize why the repository diff is safe to proceed.',
      answerPlaceholder: 'Summarize the worktree review rationale that justifies approval.',
    }

    const projectWithPending = makeProject({
      approvalRequests: [gatePendingApproval, runtimePendingApproval, optionalPendingApproval, approvedApproval],
      pendingApprovalCount: 3,
      latestDecisionOutcome: {
        actionId: approvedApproval.actionId,
        title: approvedApproval.title,
        status: approvedApproval.status,
        statusLabel: approvedApproval.statusLabel,
        gateNodeId: approvedApproval.gateNodeId,
        gateKey: approvedApproval.gateKey,
        userAnswer: approvedApproval.userAnswer,
        decisionNote: approvedApproval.decisionNote,
        resolvedAt: approvedApproval.resolvedAt,
      },
      resumeHistory: [],
    })

    const resolveOperatorAction = vi.fn(async () => undefined)
    const resumeOperatorRun = vi.fn(async () => undefined)

    render(
      <AgentRuntime
        agent={makeAgent({
          project: projectWithPending,
          approvalRequests: projectWithPending.approvalRequests,
          pendingApprovalCount: projectWithPending.pendingApprovalCount,
          latestDecisionOutcome: projectWithPending.latestDecisionOutcome,
          resumeHistory: projectWithPending.resumeHistory,
          runtimeSession,
          runtimeRun: makeRuntimeRun(),
          runtimeStream: makeRuntimeStream({ status: 'live' }),
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
          authPhase: 'authenticated',
          authPhaseLabel: 'Authenticated',
        })}
        onResolveOperatorAction={resolveOperatorAction}
        onResumeOperatorRun={resumeOperatorRun}
      />,
    )

    const gateAnswer = screen.getByLabelText('Operator answer for gate-pending')
    const runtimeAnswer = screen.getByLabelText('Operator answer for flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required')
    const optionalAnswer = screen.getByLabelText('Operator answer for optional-pending')

    const gateCard = gateAnswer.closest('.rounded-xl')
    const runtimeCard = runtimeAnswer.closest('.rounded-xl')
    const optionalCard = optionalAnswer.closest('.rounded-xl')

    if (!gateCard || !runtimeCard || !optionalCard) {
      throw new Error('Expected pending approval cards to render with textarea controls.')
    }

    const gateApproveButton = within(gateCard).getByRole('button', { name: 'Approve' })
    const runtimeApproveButton = within(runtimeCard).getByRole('button', { name: 'Approve' })
    const optionalApproveButton = within(optionalCard).getByRole('button', { name: 'Approve' })

    expect(within(gateCard).getByText('Required answer contract')).toBeVisible()
    expect(within(gateCard).getByText('Answer shape:')).toBeVisible()
    expect(within(gateCard).getByText('Worktree review rationale')).toBeVisible()
    expect(within(runtimeCard).getByText('Required answer contract')).toBeVisible()
    expect(within(runtimeCard).getByText('Terminal input text')).toBeVisible()
    expect(within(optionalCard).getByText('Optional answer contract')).toBeVisible()

    expect(gateApproveButton).toBeDisabled()
    expect(runtimeApproveButton).toBeDisabled()
    expect(optionalApproveButton).toBeEnabled()

    fireEvent.click(optionalApproveButton)
    await waitFor(() =>
      expect(resolveOperatorAction).toHaveBeenCalledWith('optional-pending', 'approve', {
        userAnswer: null,
      }),
    )

    fireEvent.change(runtimeAnswer, {
      target: { value: '   ' },
    })
    expect(runtimeApproveButton).toBeDisabled()
    expect(within(runtimeCard).getByText('A non-empty user answer is required before approving this runtime-resumable request.')).toBeVisible()

    fireEvent.change(runtimeAnswer, {
      target: { value: 'Enter maintenance mode' },
    })
    expect(runtimeApproveButton).toBeEnabled()

    fireEvent.click(runtimeApproveButton)
    await waitFor(() =>
      expect(resolveOperatorAction).toHaveBeenCalledWith(
        'flow:flow-1:run:run-1:boundary:boundary-1:terminal_input_required',
        'approve',
        {
          userAnswer: 'Enter maintenance mode',
        },
      ),
    )

    fireEvent.change(gateAnswer, {
      target: { value: 'Proceed after validating repo changes.' },
    })
    expect(gateApproveButton).toBeEnabled()

    fireEvent.click(gateApproveButton)
    await waitFor(() =>
      expect(resolveOperatorAction).toHaveBeenCalledWith('gate-pending', 'approve', {
        userAnswer: 'Proceed after validating repo changes.',
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Resume run' }))
    await waitFor(() =>
      expect(resumeOperatorRun).toHaveBeenCalledWith('gate-approved', {
        userAnswer: 'Proceed after validating repo changes.',
      }),
    )
  })

  it('validates route form inputs fail-closed before invoking route upserts', async () => {
    const onUpsertNotificationRoute = vi.fn(async () => undefined)

    render(
      <AgentRuntime
        agent={makeAgent({
          notificationRoutes: [
            {
              projectId: 'project-1',
              routeId: 'telegram-primary',
              routeKind: 'telegram',
              routeKindLabel: 'Telegram',
              routeTarget: 'telegram:@ops-room',
              enabled: true,
              metadataJson: null,
              createdAt: '2026-04-16T12:59:00Z',
              updatedAt: '2026-04-16T12:59:00Z',
              dispatchCount: 1,
              pendingCount: 0,
              sentCount: 1,
              failedCount: 0,
              claimedCount: 0,
              latestDispatchAt: '2026-04-16T13:00:00Z',
              latestFailureCode: null,
              latestFailureMessage: null,
              health: 'healthy',
              healthLabel: 'Healthy',
            },
          ],
          notificationChannelHealth: [
            {
              routeKind: 'telegram',
              routeKindLabel: 'Telegram',
              routeCount: 1,
              enabledCount: 1,
              disabledCount: 0,
              dispatchCount: 1,
              pendingCount: 0,
              sentCount: 1,
              failedCount: 0,
              claimedCount: 0,
              latestDispatchAt: '2026-04-16T13:00:00Z',
              health: 'healthy',
              healthLabel: 'Healthy',
            },
            {
              routeKind: 'discord',
              routeKindLabel: 'Discord',
              routeCount: 0,
              enabledCount: 0,
              disabledCount: 0,
              dispatchCount: 0,
              pendingCount: 0,
              sentCount: 0,
              failedCount: 0,
              claimedCount: 0,
              latestDispatchAt: null,
              health: 'disabled',
              healthLabel: 'Disabled',
            },
          ],
          notificationRouteLoadStatus: 'ready',
          notificationRouteIsRefreshing: false,
          notificationRouteError: null,
          notificationRouteMutationStatus: 'idle',
          pendingNotificationRouteId: null,
          notificationRouteMutationError: null,
        })}
        onUpsertNotificationRoute={onUpsertNotificationRoute}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Save route' }))

    expect(onUpsertNotificationRoute).not.toHaveBeenCalled()
    expect(screen.getByText('Route ID is required.')).toBeVisible()
    expect(screen.getByText('Route target is required.')).toBeVisible()

    fireEvent.change(screen.getByLabelText('Notification route id'), { target: { value: 'ops-route' } })
    fireEvent.change(screen.getByLabelText('Notification route target'), { target: { value: '@ops-room' } })
    fireEvent.change(screen.getByLabelText('Notification route metadata'), { target: { value: '{bad-json' } })

    fireEvent.click(screen.getByRole('button', { name: 'Save route' }))

    expect(onUpsertNotificationRoute).not.toHaveBeenCalled()
    expect(screen.getByText('Metadata must be valid JSON.')).toBeVisible()

    fireEvent.change(screen.getByLabelText('Notification route kind'), { target: { value: 'slack' } })
    expect(screen.getByText('Route kind must be Telegram or Discord.')).toBeVisible()

    fireEvent.change(screen.getByLabelText('Notification route metadata'), {
      target: { value: '{"threadId":"ops-urgent"}' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Save route' }))

    await waitFor(() =>
      expect(onUpsertNotificationRoute).toHaveBeenCalledWith({
        routeId: 'ops-route',
        routeKind: 'telegram',
        routeTarget: 'telegram:@ops-room',
        enabled: true,
        metadataJson: '{"threadId":"ops-urgent"}',
      }),
    )
  })

  it('surfaces route upsert failures and keeps existing route rows visible', async () => {
    const onUpsertNotificationRoute = vi.fn(async () => {
      throw new Error('backend route save failed')
    })

    render(
      <AgentRuntime
        agent={makeAgent({
          notificationRoutes: [
            {
              projectId: 'project-1',
              routeId: 'telegram-primary',
              routeKind: 'telegram',
              routeKindLabel: 'Telegram',
              routeTarget: 'telegram:@ops-room',
              enabled: true,
              metadataJson: null,
              createdAt: '2026-04-16T12:59:00Z',
              updatedAt: '2026-04-16T12:59:00Z',
              dispatchCount: 2,
              pendingCount: 0,
              sentCount: 1,
              failedCount: 1,
              claimedCount: 0,
              latestDispatchAt: '2026-04-16T13:00:00Z',
              latestFailureCode: 'notification_adapter_transport_failed',
              latestFailureMessage: 'Telegram returned 502.',
              health: 'degraded',
              healthLabel: 'Needs attention',
            },
          ],
          notificationChannelHealth: [
            {
              routeKind: 'telegram',
              routeKindLabel: 'Telegram',
              routeCount: 1,
              enabledCount: 1,
              disabledCount: 0,
              dispatchCount: 2,
              pendingCount: 0,
              sentCount: 1,
              failedCount: 1,
              claimedCount: 0,
              latestDispatchAt: '2026-04-16T13:00:00Z',
              health: 'degraded',
              healthLabel: 'Needs attention',
            },
            {
              routeKind: 'discord',
              routeKindLabel: 'Discord',
              routeCount: 0,
              enabledCount: 0,
              disabledCount: 0,
              dispatchCount: 0,
              pendingCount: 0,
              sentCount: 0,
              failedCount: 0,
              claimedCount: 0,
              latestDispatchAt: null,
              health: 'disabled',
              healthLabel: 'Disabled',
            },
          ],
          notificationRouteLoadStatus: 'ready',
          notificationRouteIsRefreshing: false,
          notificationRouteError: null,
          notificationRouteMutationStatus: 'idle',
          pendingNotificationRouteId: null,
          notificationRouteMutationError: null,
        })}
        onUpsertNotificationRoute={onUpsertNotificationRoute}
      />,
    )

    expect(screen.getByText('telegram-primary')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Disable route telegram-primary' }))

    await waitFor(() => expect(onUpsertNotificationRoute).toHaveBeenCalledTimes(1))
    expect(screen.getByText('backend route save failed')).toBeVisible()
    expect(screen.getByText('telegram-primary')).toBeVisible()
  })
})
