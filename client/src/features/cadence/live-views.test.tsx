import { dirname, resolve } from 'node:path'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  AgentTrustSnapshotView,
  ExecutionPaneView,
  RepositoryDiffState,
  WorkflowPaneView,
} from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  ProjectDetailView,
  RepositoryStatusEntryView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'

function makeLifecycle(
  overrides: Partial<ProjectDetailView['lifecycle']> = {},
): ProjectDetailView['lifecycle'] {
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
    runtimeRun: null,
    ...overrides,
  }
}

function makeHandoffPackage(
  overrides: Partial<ProjectDetailView['handoffPackages'][number]> = {},
): ProjectDetailView['handoffPackages'][number] {
  return {
    id: 1,
    projectId: 'project-1',
    handoffTransitionId: 'auto:txn-001',
    causalTransitionId: 'txn-000',
    fromNodeId: 'workflow-discussion',
    toNodeId: 'workflow-research',
    transitionKind: 'advance',
    packagePayload: '{"schemaVersion":1,"note":"redacted"}',
    packageHash: 'hash-001',
    createdAt: '2026-04-15T18:00:00Z',
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

function getStreamStatusLabel(status: RuntimeStreamView['status']): string {
  switch (status) {
    case 'idle':
      return 'No live stream'
    case 'subscribing':
      return 'Connecting stream'
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
    routeCount: 1,
    enabledRouteCount: 1,
    degradedRouteCount: 0,
    readyCredentialRouteCount: 1,
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
    operatorActionStatus: overrides.operatorActionStatus ?? 'idle',
    pendingOperatorActionId: overrides.pendingOperatorActionId ?? null,
    operatorActionError: overrides.operatorActionError ?? null,
    runtimeRunActionStatus: overrides.runtimeRunActionStatus ?? 'idle',
    pendingRuntimeRunAction: overrides.pendingRuntimeRunAction ?? null,
    runtimeRunActionError: overrides.runtimeRunActionError ?? null,
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

function makeExecution(
  project = makeProject(),
  statusEntries: RepositoryStatusEntryView[] = [],
): ExecutionPaneView {
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
    executionUnavailableReason: 'Execution waves and task-by-task progress are not available from the backend yet.',
    verificationUnavailableReason:
      project.verificationRecords.length > 0 || project.resumeHistory.length > 0
        ? 'Durable operator verification and resume history are loaded from the selected project snapshot.'
        : 'Verification details will appear here once the backend exposes run and wave results.',
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
  it('renders a truthful lifecycle-unavailable state when no persisted planning stages exist', () => {
    render(<PhaseView workflow={makeWorkflow()} />)

    expect(screen.getByText('Planning lifecycle')).toBeVisible()
    expect(screen.getByText('Lifecycle projection unavailable')).toBeVisible()
    expect(screen.getByRole('heading', { name: 'No phases available yet' })).toBeVisible()
    expect(screen.getByText('0/0 phases (legacy)')).toBeVisible()
    expect(screen.getByText('No active phase yet')).toBeVisible()
    expect(screen.getByText('Workflow handoff truth')).toBeVisible()
    expect(screen.getByText('0 persisted packages')).toBeVisible()
    expect(screen.getByText('Autonomous loop active')).toBeVisible()
    expect(screen.getAllByText('No persisted handoff packages yet').length).toBeGreaterThan(0)
    expect(screen.getByText('Payload bodies stay redacted in this workflow view; only transition linkage metadata is shown.')).toBeVisible()
  }, 10_000)

  it('renders lifecycle-first cards with mixed statuses and keeps phase cards as secondary context', () => {
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
            milestone: 'M001',
            totalPhases: 3,
            completedPhases: 1,
            activePhase: 2,
            phaseProgressPercent: 33,
            handoffPackages: [
              makeHandoffPackage({
                id: 1,
                handoffTransitionId: 'auto:txn-001',
                causalTransitionId: 'txn-000',
                fromNodeId: 'workflow-discussion',
                toNodeId: 'workflow-research',
                transitionKind: 'advance',
                packagePayload: '{"payload":"should-not-render"}',
                packageHash: 'hash-001',
                createdAt: '2026-04-15T18:00:00Z',
              }),
              makeHandoffPackage({
                id: 2,
                handoffTransitionId: 'auto:txn-002',
                causalTransitionId: 'txn-001',
                fromNodeId: 'workflow-research',
                toNodeId: 'workflow-requirements',
                transitionKind: 'advance',
                packagePayload: '{"payload":"still-redacted"}',
                packageHash: 'hash-002',
                createdAt: '2026-04-15T18:05:00Z',
              }),
            ],
            pendingApprovalCount: 1,
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
            phases: [
              {
                id: 1,
                name: 'Import',
                description: 'Import a tracked repository',
                status: 'complete',
                currentStep: null,
                stepStatuses: {
                  discuss: 'complete',
                  plan: 'complete',
                  execute: 'complete',
                  verify: 'complete',
                  ship: 'complete',
                },
                taskCount: 2,
                completedTasks: 2,
                summary: 'Imported successfully',
              },
              {
                id: 2,
                name: 'Workflow truth',
                description: 'Project persisted phases into the shell',
                status: 'active',
                currentStep: 'verify',
                stepStatuses: {
                  discuss: 'complete',
                  plan: 'complete',
                  execute: 'complete',
                  verify: 'active',
                  ship: 'pending',
                },
                taskCount: 3,
                completedTasks: 2,
              },
              {
                id: 3,
                name: 'Ship proof',
                description: 'Close the slice with a debug build',
                status: 'pending',
                currentStep: null,
                stepStatuses: {
                  discuss: 'pending',
                  plan: 'pending',
                  execute: 'pending',
                  verify: 'pending',
                  ship: 'pending',
                },
                taskCount: 1,
                completedTasks: 0,
              },
            ],
          }),
        )}
      />,
    )

    expect(screen.getByText('Discussion')).toBeVisible()
    expect(screen.getByText('Research')).toBeVisible()
    expect(screen.getByText('Requirements')).toBeVisible()
    expect(screen.getByText('Roadmap')).toBeVisible()
    expect(screen.getByText('25%')).toBeVisible()
    expect(screen.getByText('1/4 lifecycle stages complete')).toBeVisible()
    expect(screen.getByText('1 stages need action')).toBeVisible()
    expect(screen.getByText('Action required before this stage can close.')).toBeVisible()
    expect(screen.getByText('Workflow handoff truth')).toBeVisible()
    expect(screen.getByText('Latest persisted handoff package')).toBeVisible()
    expect(screen.getByText('2 persisted packages')).toBeVisible()
    expect(screen.getByText('Waiting on operator input')).toBeVisible()
    expect(screen.getByText('auto:txn-002')).toBeVisible()
    expect(screen.getByText('txn-001')).toBeVisible()
    expect(screen.getByText('workflow-research → workflow-requirements (advance)')).toBeVisible()
    expect(screen.getByText('hash-002')).toBeVisible()
    expect(screen.getByText('2026-04-15T18:05:00Z')).toBeVisible()
    expect(screen.queryByText('should-not-render')).not.toBeInTheDocument()
    expect(screen.queryByText('still-redacted')).not.toBeInTheDocument()
    expect(screen.queryByText('Workflow truth')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Show legacy phase details/i }))

    expect(screen.getByText('Workflow truth')).toBeVisible()
    expect(screen.getByText('2 · Workflow truth')).toBeVisible()
    expect(screen.getByText('2/3')).toBeVisible()
  })

  it('renders post-dispatch lifecycle progression truthfully when the active stage advances', () => {
    const discussionStage = {
      stage: 'discussion' as const,
      stageLabel: 'Discussion',
      nodeId: 'workflow-discussion',
      nodeLabel: 'Workflow Discussion',
      status: 'complete' as const,
      statusLabel: 'Complete',
      actionRequired: false,
      lastTransitionAt: '2026-04-16T14:00:00Z',
    }
    const researchStage = {
      stage: 'research' as const,
      stageLabel: 'Research',
      nodeId: 'workflow-research',
      nodeLabel: 'Workflow Research',
      status: 'complete' as const,
      statusLabel: 'Complete',
      actionRequired: false,
      lastTransitionAt: '2026-04-16T14:01:00Z',
    }
    const requirementsStage = {
      stage: 'requirements' as const,
      stageLabel: 'Requirements',
      nodeId: 'workflow-requirements',
      nodeLabel: 'Workflow Requirements',
      status: 'active' as const,
      statusLabel: 'Active',
      actionRequired: false,
      lastTransitionAt: '2026-04-16T14:02:00Z',
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
              activeStage: requirementsStage,
              actionRequiredCount: 0,
              blockedCount: 0,
              completedCount: 2,
              percentComplete: 50,
            }),
          }),
        )}
      />,
    )

    expect(screen.getByText('Planning lifecycle')).toBeVisible()
    expect(screen.queryByText('Lifecycle projection unavailable')).not.toBeInTheDocument()
    expect(screen.getByText('2/4 lifecycle stages complete')).toBeVisible()
    expect(screen.getByText('50%')).toBeVisible()
    expect(screen.getByText('Requirements')).toBeVisible()
    expect(screen.queryByText('Action required before this stage can close.')).not.toBeInTheDocument()
  })

  it('renders handoff metadata fallbacks for malformed rows and keeps payload redacted', () => {
    render(
      <PhaseView
        workflow={makeWorkflow(
          makeProject({
            handoffPackages: [
              makeHandoffPackage({
                handoffTransitionId: '   ',
                causalTransitionId: '   ',
                fromNodeId: '   ',
                toNodeId: ' ',
                transitionKind: '   ',
                packageHash: '   ',
                createdAt: 'not-a-timestamp',
                packagePayload: '{"token":"top-secret-token"}',
              }),
            ],
          }),
        )}
      />,
    )

    expect(screen.getByText('Workflow handoff truth')).toBeVisible()
    expect(screen.getByText('1 persisted package')).toBeVisible()
    expect(screen.getByText('Transition ID unavailable')).toBeVisible()
    expect(screen.getByText('No causal transition linked')).toBeVisible()
    expect(screen.getByText('From node unavailable → To node unavailable (kind unavailable)')).toBeVisible()
    expect(screen.getByText('Hash unavailable')).toBeVisible()
    expect(screen.getByText('Timestamp unavailable')).toBeVisible()
    expect(screen.queryByText('top-secret-token')).not.toBeInTheDocument()
  })

  it('renders explicit unavailable lifecycle cards when persisted stage entries are partial', () => {
    const discussionStage = {
      stage: 'discussion' as const,
      stageLabel: 'Discussion',
      nodeId: 'workflow-discussion',
      nodeLabel: 'Workflow Discussion',
      status: 'active' as const,
      statusLabel: 'Active',
      actionRequired: false,
      lastTransitionAt: '2026-04-15T17:59:00Z',
    }

    render(
      <PhaseView
        workflow={makeWorkflow(
          makeProject({
            lifecycle: makeLifecycle({
              stages: [discussionStage],
              byStage: {
                discussion: discussionStage,
                research: null,
                requirements: null,
                roadmap: null,
              },
              hasStages: true,
              activeStage: discussionStage,
              actionRequiredCount: 0,
              blockedCount: 0,
              completedCount: 0,
              percentComplete: 0,
            }),
          }),
        )}
      />,
    )

    expect(screen.getByText(/lifecycle stages missing from persisted data/i)).toBeVisible()
    expect(screen.getAllByText('Unavailable').length).toBeGreaterThanOrEqual(3)
  })

  it('surfaces duplicate lifecycle stage data and keeps only the first persisted entry visible', () => {
    const firstDiscussionStage = {
      stage: 'discussion' as const,
      stageLabel: 'Discussion',
      nodeId: 'workflow-discussion',
      nodeLabel: 'Workflow Discussion',
      status: 'active' as const,
      statusLabel: 'Active',
      actionRequired: false,
      lastTransitionAt: '2026-04-15T17:59:00Z',
    }
    const duplicateDiscussionStage = {
      ...firstDiscussionStage,
      nodeId: 'workflow-discussion-duplicate',
      nodeLabel: 'Workflow Discussion Duplicate',
      status: 'blocked' as const,
      statusLabel: 'Blocked',
      actionRequired: true,
      lastTransitionAt: '2026-04-15T18:00:00Z',
    }

    render(
      <PhaseView
        workflow={makeWorkflow(
          makeProject({
            lifecycle: makeLifecycle({
              stages: [firstDiscussionStage, duplicateDiscussionStage],
              byStage: {
                discussion: firstDiscussionStage,
                research: null,
                requirements: null,
                roadmap: null,
              },
              hasStages: true,
              activeStage: firstDiscussionStage,
              actionRequiredCount: 0,
              blockedCount: 0,
              completedCount: 0,
              percentComplete: 0,
            }),
          }),
        )}
      />,
    )

    expect(screen.getByText(/Duplicate lifecycle stage entries were received/i)).toBeVisible()
    expect(screen.getByText(/Showing the first persisted entry only/i)).toBeVisible()
    expect(screen.queryByText('Workflow Discussion Duplicate')).not.toBeInTheDocument()
  })

  it('renders signed-out runtime state honestly instead of a placeholder card', () => {
    render(<AgentRuntime agent={makeAgent()} />)

    expect(screen.getByRole('heading', { name: 'Sign in to OpenAI for this project' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Authenticate to view live agent activity' })).toBeVisible()
    expect(screen.getByText('Runtime setup')).toBeVisible()
    expect(screen.getByText('Project binding')).toBeVisible()
    expect(screen.getByText('Account + session')).toBeVisible()
    expect(screen.getByText('/tmp/cadence')).toBeVisible()
    expect(screen.getAllByText('No branch').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Runtime unavailable').length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'Sign in with OpenAI' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Reuse app-local session' })).toBeDisabled()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Sign in with OpenAI to unlock the live agent feed for this imported project.',
    )
  })

  it('renders trust snapshot recovery guidance without leaking credential payloads', () => {
    render(
      <AgentRuntime
        agent={makeAgent(makeProject(), {
          runtimeSession: makeRuntimeSession(),
          runtimeRun: makeRuntimeRun({
            status: 'stale',
            statusLabel: 'Supervisor stale',
            isActive: false,
            isTerminal: false,
            isStale: true,
            isFailed: false,
          }),
          runtimeStream: makeRuntimeStream({ status: 'stale' }),
          runtimeStreamStatus: 'stale',
          runtimeStreamStatusLabel: 'Stream stale',
          trustSnapshot: makeTrustSnapshot({
            state: 'degraded',
            stateLabel: 'Needs attention',
            runtimeState: 'degraded',
            approvalsState: 'degraded',
            approvalsReason: 'There are 1 pending operator approval gate(s) waiting for action.',
            pendingApprovalCount: 1,
            routesState: 'degraded',
            routesReason: '1 route(s) show degraded or pending dispatch health.',
            degradedRouteCount: 1,
            credentialsState: 'degraded',
            credentialsReason: '1 enabled route(s) are missing required app-local credentials.',
            routeError: {
              code: 'notification_route_refresh_failed',
              message: 'Cadence could not refresh notification route health for this project.',
              retryable: true,
            },
          }),
        })}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Consolidated trust surface for unattended operation' })).toBeVisible()
    expect(screen.getByText('Operator recovery actions')).toBeVisible()
    expect(screen.getByText('Permission scope')).toBeVisible()
    expect(screen.getByText('Storage boundaries')).toBeVisible()
    expect(screen.getByText(/never render raw secret values here/i)).toBeVisible()
    expect(screen.getByText('Resolve pending operator approvals so autonomous continuation is no longer blocked.')).toBeVisible()
    expect(screen.getByText('Refresh route health from Notification channels & routes after credential updates.')).toBeVisible()
    expect(screen.queryByText('discord-bot-token')).not.toBeInTheDocument()
  })

  it(
    'renders durable approval controls, latest decision context, and resume history in the real agent pane',
    async () => {
    const runtimeSession = makeRuntimeSession({
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
    })
    const runtimeStream = makeRuntimeStream({
      status: 'live',
      lastItemAt: '2026-04-13T20:01:03Z',
      lastSequence: 3,
      items: [
        {
          id: 'transcript-1',
          kind: 'transcript',
          runId: 'run-1',
          sequence: 1,
          createdAt: '2026-04-13T20:01:00Z',
          text: 'Connected to cadence.',
        },
        {
          id: 'activity-1',
          kind: 'activity',
          runId: 'run-1',
          sequence: 2,
          createdAt: '2026-04-13T20:01:02Z',
          code: 'stream.attach',
          title: 'Recovered replay attached',
          detail: 'Cadence resumed the active run-scoped stream after reload.',
        },
        {
          id: 'tool-1',
          kind: 'tool',
          runId: 'run-1',
          sequence: 3,
          createdAt: '2026-04-13T20:01:03Z',
          toolCallId: 'bootstrap-repository-context',
          toolName: 'inspect_repository_context',
          toolState: 'running',
          detail: 'Collecting repository status.',
        },
      ],
      transcriptItems: [
        {
          id: 'transcript-1',
          kind: 'transcript',
          runId: 'run-1',
          sequence: 1,
          createdAt: '2026-04-13T20:01:00Z',
          text: 'Connected to cadence.',
        },
      ],
      activityItems: [
        {
          id: 'activity-1',
          kind: 'activity',
          runId: 'run-1',
          sequence: 2,
          createdAt: '2026-04-13T20:01:02Z',
          code: 'stream.attach',
          title: 'Recovered replay attached',
          detail: 'Cadence resumed the active run-scoped stream after reload.',
        },
      ],
      toolCalls: [
        {
          id: 'tool-1',
          kind: 'tool',
          runId: 'run-1',
          sequence: 3,
          createdAt: '2026-04-13T20:01:03Z',
          toolCallId: 'bootstrap-repository-context',
          toolName: 'inspect_repository_context',
          toolState: 'running',
          detail: 'Collecting repository status.',
        },
      ],
    })
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
        },
        {
          actionId: 'action-approved',
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_plan',
          title: 'Resume after plan review',
          detail: 'The operator approved continuing after reviewing the current plan state.',
          gateNodeId: 'workflow-research',
          gateKey: 'requires_user_input',
          transitionFromNodeId: 'workflow-discussion',
          transitionToNodeId: 'workflow-research',
          transitionKind: 'advance',
          userAnswer: 'Proceed after validating repo changes.',
          status: 'approved',
          statusLabel: 'Approved',
          decisionNote: 'Looks good to resume.',
          createdAt: '2026-04-13T20:01:00Z',
          updatedAt: '2026-04-13T20:02:30Z',
          resolvedAt: '2026-04-13T20:02:30Z',
          isPending: false,
          isResolved: true,
          canResume: true,
          isGateLinked: true,
        },
      ],
      pendingApprovalCount: 1,
      latestDecisionOutcome: {
        actionId: 'action-approved',
        title: 'Resume after plan review',
        status: 'approved',
        statusLabel: 'Approved',
        gateNodeId: 'workflow-research',
        gateKey: 'requires_user_input',
        userAnswer: 'Proceed after validating repo changes.',
        decisionNote: 'Looks good to resume.',
        resolvedAt: '2026-04-13T20:02:30Z',
      },
      resumeHistory: [
        {
          id: 9,
          sourceActionId: 'action-approved',
          sessionId: 'session-1',
          status: 'started',
          statusLabel: 'Resume started',
          summary: 'Operator resumed the selected project runtime session.',
          createdAt: '2026-04-13T20:03:30Z',
        },
      ],
    })
    const resolveOperatorAction = vi.fn(async () => undefined)
    let resolveResumeIntent: (() => void) | null = null
    const resumeIntentPromise = new Promise<void>((resolve) => {
      resolveResumeIntent = resolve
    })
    const resumeOperatorRun = vi.fn(async () => {
      await resumeIntentPromise
    })

    render(
      <AgentRuntime
        agent={makeAgent(project, {
          runtimeSession,
          runtimeRun: makeRuntimeRun(),
          authPhase: 'authenticated',
          authPhaseLabel: 'Authenticated',
          runtimeStream,
          runtimeStreamStatus: 'live',
          runtimeStreamStatusLabel: 'Streaming live activity',
          sessionUnavailableReason:
            'Cadence is authenticated as acct@example.com and bound to session session-1.',
          messagesUnavailableReason:
            'Live runtime activity is streaming for this project (2 items captured).',
        })}
        onLogout={vi.fn(async () => runtimeSession)}
        onResolveOperatorAction={resolveOperatorAction}
        onResumeOperatorRun={resumeOperatorRun}
        onRetryStream={vi.fn(async () => undefined)}
      />,
    )

    expect(screen.getByRole('heading', { name: 'OpenAI runtime session connected' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Streaming run-scoped live activity' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Durable operator loop truth for the selected repo' })).toBeVisible()
    expect(screen.getByText('Review worktree changes')).toBeVisible()
    expect(
      screen.getAllByText('workflow-research · requires_user_input · workflow-discussion → workflow-research (advance)').length,
    ).toBeGreaterThan(0)
    expect(screen.getAllByText('Persisted user answer').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Proceed after validating repo changes.').length).toBeGreaterThan(1)
    expect(screen.getAllByText('Resume after plan review').length).toBeGreaterThan(1)
    expect(screen.getAllByText('Looks good to resume.').length).toBeGreaterThan(1)
    expect(screen.getAllByText('Resume state').length).toBeGreaterThan(1)
    expect(screen.getByText('Waiting for operator input before this action can resume the run.')).toBeVisible()
    expect(
      screen.getByText('Latest resume started: Operator resumed the selected project runtime session.'),
    ).toBeVisible()
    expect(screen.getByText('Operator resumed the selected project runtime session.')).toBeVisible()
    expect(screen.getByText('Connected to cadence.')).toBeVisible()
    expect(screen.getByText('Recovered replay attached')).toBeVisible()
    expect(screen.getByText('Cadence resumed the active run-scoped stream after reload.')).toBeVisible()
    expect(screen.getByText('inspect_repository_context')).toBeVisible()
    expect(screen.getByText('Runtime activity')).toBeVisible()
    expect(screen.getAllByText('run-1').length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'Retry live feed' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Sign out' })).toBeVisible()

    const approveButton = screen.getByRole('button', { name: 'Approve' })
    expect(approveButton).toBeDisabled()
    expect(
      screen.getByText('A non-empty user answer is required before approving this gate-linked request.'),
    ).toBeVisible()

    fireEvent.change(screen.getByLabelText('Operator answer for action-pending'), {
      target: { value: '   ' },
    })
    expect(approveButton).toBeDisabled()
    fireEvent.click(approveButton)
    expect(resolveOperatorAction).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText('Operator answer for action-pending'), {
      target: { value: 'Proceed after validating repo changes.' },
    })
    expect(approveButton).toBeEnabled()
    fireEvent.click(approveButton)
    await waitFor(() =>
      expect(resolveOperatorAction).toHaveBeenCalledWith('action-pending', 'approve', {
        userAnswer: 'Proceed after validating repo changes.',
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Resume run' }))
    await waitFor(() =>
      expect(resumeOperatorRun).toHaveBeenCalledWith('action-approved', {
        userAnswer: 'Proceed after validating repo changes.',
      }),
    )
    expect(screen.getByText('Resume request is in flight for this action. Cadence will refresh durable state before updating this card.')).toBeVisible()
    expect(screen.getAllByText('Running').length).toBeGreaterThan(0)

    await act(async () => {
      resolveResumeIntent?.()
      await Promise.resolve()
    })

    await waitFor(() =>
      expect(
        screen.getByText('Latest resume started: Operator resumed the selected project runtime session.'),
      ).toBeVisible(),
    )

    expect(screen.getAllByText('acct@example.com').length).toBeGreaterThan(0)
    expect(screen.getAllByText('session-1').length).toBeGreaterThan(0)
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Live runtime activity is streaming for the active supervised run. Composer remains read-only in this shell.',
    )
  }, 10_000)

  it('renders explicit empty feed state for an authenticated runtime before the first event arrives', () => {
    const runtimeSession = makeRuntimeSession({
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
    })

    render(
      <AgentRuntime
        agent={makeAgent(undefined, {
          runtimeSession,
          authPhase: 'authenticated',
          authPhaseLabel: 'Authenticated',
          messagesUnavailableReason: 'Cadence authenticated this project, but the live runtime stream has not started yet.',
        })}
        onRetryStream={vi.fn(async () => undefined)}
      />,
    )

    expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.getByText('No supervised run is attached')).toBeVisible()
    expect(screen.getByText('No supervised run transcript yet')).toBeVisible()
    expect(screen.getByText('No run-scoped activity yet')).toBeVisible()
    expect(screen.getByText('No tool lifecycle yet')).toBeVisible()
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Start or reconnect a supervised run to create the run-scoped live feed for this imported project.',
    )
  })

  it('renders recovered supervised run status and durable checkpoints before the live feed resumes', () => {
    const runtimeSession = makeRuntimeSession({
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
    })
    const runtimeRun = makeRuntimeRun()

    render(
      <AgentRuntime
        agent={makeAgent(undefined, {
          runtimeSession,
          runtimeRun,
          authPhase: 'authenticated',
          authPhaseLabel: 'Authenticated',
          runtimeStream: null,
          runtimeRunUnavailableReason:
            'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
          messagesUnavailableReason:
            'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.',
        })}
        onRetryStream={vi.fn(async () => undefined)}
      />,
    )

    expect(screen.getByRole('heading', { name: 'Recovered durable runtime run' })).toBeVisible()
    expect(screen.getAllByText('Supervisor running').length).toBeGreaterThan(0)
    expect(screen.getAllByText('Recovered repository context before reconnecting the live feed.').length).toBeGreaterThan(0)
    expect(screen.getByText('Supervisor boot recorded.')).toBeVisible()
    expect(screen.getByText('Control reachable')).toBeVisible()
    expect(screen.getAllByText('run-1').length).toBeGreaterThan(0)
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Cadence recovered run-1, but the live runtime stream has not delivered its first event yet.',
    )
  })

  it(
    'keeps manual redirect fallback usable when browser open or manual submit fails',
    async () => {
      openUrlMock.mockRejectedValueOnce(new Error('browser unavailable'))
      const submitManualCallback = vi.fn(async () => {
        throw new Error('manual redirect failed')
      })
      const runtimeSession = makeRuntimeSession({
        flowId: 'flow-1',
        phase: 'awaiting_manual_input',
        phaseLabel: 'Awaiting manual input',
        runtimeLabel: 'Openai Codex · Awaiting manual input',
        callbackBound: false,
        authorizationUrl: 'https://auth.openai.com/oauth/authorize?client_id=test',
        redirectUri: 'http://127.0.0.1:1455/auth/callback',
        lastErrorCode: 'callback_listener_bind_failed',
        lastError: {
          code: 'callback_listener_bind_failed',
          message: 'Paste the redirect URL to finish login.',
          retryable: false,
        },
        isAuthenticated: false,
        isLoginInProgress: true,
        needsManualInput: true,
        isSignedOut: false,
        isFailed: false,
      })

      render(
        <AgentRuntime
          agent={makeAgent(undefined, {
            runtimeSession,
            authPhase: 'awaiting_manual_input',
            authPhaseLabel: 'Awaiting manual input',
            sessionUnavailableReason: 'Paste the redirect URL to finish login.',
          })}
          onSubmitManualCallback={submitManualCallback}
        />,
      )

      fireEvent.click(screen.getByRole('button', { name: 'Open sign-in again' }))

      await waitFor(() =>
        expect(screen.getByText('Browser launch needs manual fallback')).toBeVisible(),
      )

      expect(screen.getByRole('heading', { name: 'Keep the login flow moving even if the browser callback is brittle' })).toBeVisible()
      expect(screen.getByDisplayValue('https://auth.openai.com/oauth/authorize?client_id=test')).toBeVisible()

      fireEvent.change(screen.getByLabelText('OpenAI redirect URL'), {
        target: { value: 'http://127.0.0.1:1455/auth/callback?code=test&state=flow-1' },
      })
      fireEvent.click(screen.getByRole('button', { name: 'Complete pasted redirect' }))

      await waitFor(() => expect(submitManualCallback).toHaveBeenCalledWith('flow-1', expect.any(String)))
      await waitFor(() => expect(screen.getByText('Runtime action failed')).toBeVisible())
      expect(screen.getByText('manual redirect failed')).toBeVisible()
      expect(screen.getAllByText('project-1').length).toBeGreaterThan(0)
      expect(screen.getByRole('button', { name: 'Complete pasted redirect' })).toBeEnabled()
    },
    10_000,
  )

  it('keeps durable approvals and resume history visible when the live feed turns stale', () => {
    const runtimeSession = makeRuntimeSession({
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
    })
    const runtimeStream = makeRuntimeStream({
      status: 'stale',
      lastItemAt: '2026-04-13T20:01:03Z',
      lastSequence: 2,
      lastIssue: {
        code: 'runtime_stream_bootstrap_failed',
        message: 'Cadence lost the runtime bootstrap stream while collecting repository context.',
        retryable: true,
        observedAt: '2026-04-13T20:01:03Z',
      },
      items: [
        {
          id: 'transcript-1',
          kind: 'transcript',
          runId: 'run-1',
          sequence: 1,
          createdAt: '2026-04-13T20:01:00Z',
          text: 'Connected to cadence.',
        },
        {
          id: 'failure-1',
          kind: 'failure',
          runId: 'run-1',
          sequence: 2,
          createdAt: '2026-04-13T20:01:03Z',
          code: 'runtime_stream_bootstrap_failed',
          message: 'Cadence lost the runtime bootstrap stream while collecting repository context.',
          retryable: true,
        },
      ],
      transcriptItems: [
        {
          id: 'transcript-1',
          kind: 'transcript',
          runId: 'run-1',
          sequence: 1,
          createdAt: '2026-04-13T20:01:00Z',
          text: 'Connected to cadence.',
        },
      ],
      failure: {
        id: 'failure-1',
        kind: 'failure',
        runId: 'run-1',
        sequence: 2,
        createdAt: '2026-04-13T20:01:03Z',
        code: 'runtime_stream_bootstrap_failed',
        message: 'Cadence lost the runtime bootstrap stream while collecting repository context.',
        retryable: true,
      },
    })
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
        },
        {
          actionId: 'action-approved',
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_plan',
          title: 'Resume after stale stream recovery',
          detail: 'Retry resume once stale stream diagnostics are reviewed.',
          gateNodeId: 'workflow-research',
          gateKey: 'requires_user_input',
          transitionFromNodeId: 'workflow-discussion',
          transitionToNodeId: 'workflow-research',
          transitionKind: 'advance',
          userAnswer: 'Proceed after reviewing stale diagnostics.',
          status: 'approved',
          statusLabel: 'Approved',
          decisionNote: 'Ready to retry resume once diagnostics are reviewed.',
          createdAt: '2026-04-13T20:01:00Z',
          updatedAt: '2026-04-13T20:03:30Z',
          resolvedAt: '2026-04-13T20:03:30Z',
          isPending: false,
          isResolved: true,
          canResume: true,
          isGateLinked: true,
        },
      ],
      pendingApprovalCount: 1,
      resumeHistory: [
        {
          id: 2,
          sourceActionId: 'action-approved',
          sessionId: 'session-1',
          status: 'failed',
          statusLabel: 'Resume failed',
          summary: 'Operator resume failed after the stale stream was detected.',
          createdAt: '2026-04-13T20:04:00Z',
        },
      ],
    })

    render(
      <AgentRuntime
        agent={makeAgent(project, {
          runtimeSession,
          authPhase: 'authenticated',
          authPhaseLabel: 'Authenticated',
          runtimeStream,
          runtimeStreamStatus: 'stale',
          runtimeStreamStatusLabel: 'Stream stale',
          operatorActionError: {
            code: 'operator_action_conflict',
            message: 'Cadence could not persist the operator decision because the approval was already resolved elsewhere.',
            retryable: false,
          },
          messagesUnavailableReason: 'Cadence lost the runtime bootstrap stream while collecting repository context.',
        })}
        onRetryStream={vi.fn(async () => undefined)}
      />,
    )

    expect(screen.getByText('Connected to cadence.')).toBeVisible()
    expect(screen.getByText('Review worktree changes')).toBeVisible()
    expect(screen.getByText('Waiting for operator input before this action can resume the run.')).toBeVisible()
    expect(
      screen.getByText('Latest resume failed: Operator resume failed after the stale stream was detected.'),
    ).toBeVisible()
    expect(screen.getByText('Operator resume failed after the stale stream was detected.')).toBeVisible()
    expect(screen.getByText('Operator action failed')).toBeVisible()
    expect(screen.getByText('Cadence could not persist the operator decision because the approval was already resolved elsewhere.')).toBeVisible()
    expect(
      screen.getAllByText('Cadence lost the runtime bootstrap stream while collecting repository context.').length,
    ).toBeGreaterThan(0)
    expect(screen.getAllByText('Run-scoped stream failed').length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'Retry live feed' })).toBeVisible()
    expect(screen.getAllByText(/runtime_stream_bootstrap_failed/).length).toBeGreaterThan(0)
    expect(screen.getByRole('button', { name: 'Approve' })).toBeVisible()
  })

  it('renders empty diff, clean status, and unavailable verification details without fabricating data', () => {
    const onSelectDiffScope = vi.fn()
    const onRetryDiff = vi.fn()

    render(
      <ExecutionView
        activeDiff={makeDiffState()}
        activeDiffScope="unstaged"
        execution={makeExecution()}
        onRetryDiff={onRetryDiff}
        onSelectDiffScope={onSelectDiffScope}
      />,
    )

    expect(screen.getByText('No live waves yet')).toBeVisible()
    expect(screen.getByText('The repository is currently clean, so there are no live execution-side file changes to show here.')).toBeVisible()
    expect(screen.getByText('Channel dispatch diagnostics')).toBeVisible()
    expect(
      screen.getByText('Cadence has not recorded any notification dispatch rows for this project yet, so channel health stays empty instead of fabricated.'),
    ).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Changes' }))
    expect(onSelectDiffScope).toHaveBeenCalledWith('unstaged')
    expect(screen.getByRole('heading', { name: 'No unstaged diff available' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Verify' }))
    expect(screen.getByText('No verification records yet')).toBeVisible()
    expect(screen.getByText('Verification details will appear here once the backend exposes run and wave results.')).toBeVisible()
  })

  it('renders durable verification rows, resume history, and malformed fallbacks on the Verify tab', () => {
    render(
      <ExecutionView
        activeDiff={makeDiffState()}
        activeDiffScope="unstaged"
        execution={makeExecution(
          makeProject({
            verificationRecords: [
              {
                id: 7,
                sourceActionId: 'action-7',
                status: 'failed',
                statusLabel: ' ',
                summary: 'Verification failed after the operator rejected the request.',
                detail: 'Cadence kept the failed verification row visible for inspection.',
                recordedAt: '',
              },
            ],
            resumeHistory: [
              {
                id: 4,
                sourceActionId: null,
                sessionId: null,
                status: 'failed',
                statusLabel: ' ',
                summary: 'Resume could not continue after the previous failure.',
                createdAt: '',
              },
            ],
            latestDecisionOutcome: {
              actionId: 'action-7',
              title: 'Reject unsafe worktree review',
              status: 'rejected',
              statusLabel: 'Rejected',
              gateNodeId: null,
              gateKey: null,
              userAnswer: null,
              decisionNote: null,
              resolvedAt: '2026-04-13T20:05:00Z',
            },
          }),
        )}
        onRetryDiff={vi.fn()}
        onSelectDiffScope={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Verify' }))

    expect(screen.getByText('Repo-local operator verification truth')).toBeVisible()
    expect(screen.getByText('Verification failed after the operator rejected the request.')).toBeVisible()
    expect(screen.getByText('Cadence kept the failed verification row visible for inspection.')).toBeVisible()
    expect(screen.getByText('Resume could not continue after the previous failure.')).toBeVisible()
    expect(screen.getByText('Reject unsafe worktree review')).toBeVisible()
    expect(screen.getAllByText('Unknown').length).toBeGreaterThan(0)
    expect(screen.getAllByText('failed').length).toBeGreaterThan(0)
  })

  it('renders explicit diff failures for the selected scope', () => {
    render(
      <ExecutionView
        activeDiff={makeDiffState({ status: 'error', diff: null, errorMessage: 'diff failed', projectId: 'project-1' })}
        activeDiffScope="worktree"
        execution={makeExecution(
          makeProject(),
          [{ path: 'client/src/App.tsx', staged: null, unstaged: 'modified', untracked: false }],
        )}
        onRetryDiff={vi.fn()}
        onSelectDiffScope={vi.fn()}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Changes' }))
    expect(screen.getByText('Failed to load the worktree diff.')).toBeVisible()
    expect(screen.getByText('diff failed')).toBeVisible()
    expect(screen.getByText('Active diff')).toBeVisible()
  })

  it('keeps MOCK fixtures out of the primary render path', () => {
    const here = dirname(fileURLToPath(import.meta.url))
    const sourcePaths = [
      resolve(here, '../../App.tsx'),
      resolve(here, '../../../components/cadence/phase-view.tsx'),
      resolve(here, '../../../components/cadence/agent-runtime.tsx'),
      resolve(here, '../../../components/cadence/execution-view.tsx'),
    ]

    for (const sourcePath of sourcePaths) {
      const source = readFileSync(sourcePath, 'utf8')
      expect(source).not.toMatch(/MOCK_/)
    }
  })
})
