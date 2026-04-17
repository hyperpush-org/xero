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

import { CadenceApp } from './App'
import { CadenceDesktopError, type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import type {
  ListNotificationDispatchesResponseDto,
  ListNotificationRoutesResponseDto,
  ListProjectsResponseDto,
  ProjectSnapshotResponseDto,
  ProjectUpdatedPayloadDto,
  RepositoryDiffResponseDto,
  RepositoryStatusChangedPayloadDto,
  RepositoryStatusResponseDto,
  RuntimeRunDto,
  RuntimeRunUpdatedPayloadDto,
  RuntimeSessionDto,
  RuntimeStreamEventDto,
  RuntimeUpdatedPayloadDto,
  SubscribeRuntimeStreamResponseDto,
} from '@/src/lib/cadence-model'

function makeProjectSummary(id: string, name: string, overrides: Partial<ProjectSnapshotResponseDto['project']> = {}) {
  return {
    id,
    name,
    description: '',
    milestone: '',
    totalPhases: 0,
    completedPhases: 0,
    activePhase: 0,
    branch: null,
    runtime: null,
    ...overrides,
  }
}

function makeSnapshot(overrides: Partial<ProjectSnapshotResponseDto> = {}): ProjectSnapshotResponseDto {
  return {
    project: makeProjectSummary('project-1', 'api-gateway'),
    repository: {
      id: 'repo-1',
      projectId: 'project-1',
      rootPath: '/tmp/api-gateway',
      displayName: 'api-gateway',
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    phases: [],
    lifecycle: {
      stages: [],
    },
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    ...overrides,
  }
}

function makeLifecycleStage(
  stage: ProjectSnapshotResponseDto['lifecycle']['stages'][number]['stage'],
  overrides: Partial<ProjectSnapshotResponseDto['lifecycle']['stages'][number]> = {},
): ProjectSnapshotResponseDto['lifecycle']['stages'][number] {
  return {
    stage,
    nodeId: `workflow-${stage}`,
    status: 'pending',
    actionRequired: false,
    lastTransitionAt: null,
    ...overrides,
  }
}

function makeHandoffPackage(
  projectId: string,
  overrides: Partial<NonNullable<ProjectSnapshotResponseDto['handoffPackages']>[number]> = {},
): NonNullable<ProjectSnapshotResponseDto['handoffPackages']>[number] {
  return {
    id: 1,
    projectId,
    handoffTransitionId: 'auto:txn-001',
    causalTransitionId: 'txn-000',
    fromNodeId: 'workflow-discussion',
    toNodeId: 'workflow-research',
    transitionKind: 'advance',
    packagePayload: '{"payload":"redacted"}',
    packageHash: 'hash-001',
    createdAt: '2026-04-16T12:00:00Z',
    ...overrides,
  }
}

function makeStatus(overrides: Partial<RepositoryStatusResponseDto> = {}): RepositoryStatusResponseDto {
  return {
    repository: {
      id: 'repo-1',
      projectId: 'project-1',
      rootPath: '/tmp/api-gateway',
      displayName: 'api-gateway',
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    branch: null,
    entries: [],
    hasStagedChanges: false,
    hasUnstagedChanges: false,
    hasUntrackedChanges: false,
    ...overrides,
  }
}

function makeDiff(overrides: Partial<RepositoryDiffResponseDto> = {}): RepositoryDiffResponseDto {
  return {
    repository: {
      id: 'repo-1',
      projectId: 'project-1',
      rootPath: '/tmp/api-gateway',
      displayName: 'api-gateway',
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    scope: 'unstaged',
    patch: '',
    truncated: false,
    baseRevision: null,
    ...overrides,
  }
}

function createRuntimeSession(
  projectId: string,
  overrides: Partial<RuntimeSessionDto> = {},
): RuntimeSessionDto {
  return {
    projectId,
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: null,
    accountId: null,
    phase: 'idle',
    callbackBound: null,
    authorizationUrl: 'https://auth.openai.com/oauth/authorize?client_id=test',
    redirectUri: 'http://127.0.0.1:1455/auth/callback',
    lastErrorCode: 'auth_session_not_found',
    lastError: {
      code: 'auth_session_not_found',
      message: 'Sign in with OpenAI to create a runtime session for this project.',
      retryable: false,
    },
    updatedAt: '2026-04-13T19:33:32Z',
    ...overrides,
  }
}

function createRuntimeRun(
  projectId: string,
  overrides: Partial<RuntimeRunDto> = {},
): RuntimeRunDto {
  return {
    projectId,
    runId: 'run-1',
    runtimeKind: 'openai_codex',
    supervisorKind: 'detached_pty',
    status: 'running',
    transport: {
      kind: 'tcp',
      endpoint: '127.0.0.1:4455',
      liveness: 'reachable',
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
        summary: 'Supervisor boot recorded.',
        createdAt: '2026-04-15T20:00:01Z',
      },
      {
        sequence: 2,
        kind: 'state',
        summary: 'Recovered repository context before reconnecting the live feed.',
        createdAt: '2026-04-15T20:00:06Z',
      },
    ],
    ...overrides,
  }
}

function makeStreamResponse(
  projectId: string,
  overrides: Partial<SubscribeRuntimeStreamResponseDto> = {},
): SubscribeRuntimeStreamResponseDto {
  return {
    projectId,
    runtimeKind: 'openai_codex',
    runId: `run-${projectId}`,
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
    ...overrides,
  }
}

function makeStreamEvent(
  projectId: string,
  item: Omit<RuntimeStreamEventDto['item'], 'runId' | 'sequence'>,
  overrides: Partial<Omit<RuntimeStreamEventDto, 'projectId' | 'item'>> & { sequence?: number } = {},
): RuntimeStreamEventDto {
  const runId = overrides.runId ?? `run-${projectId}`
  const sequence =
    overrides.sequence ??
    Math.max(1, Number.isFinite(Date.parse(String(item.createdAt))) ? Math.floor(Date.parse(String(item.createdAt)) / 1000) : 1)

  return {
    projectId,
    runtimeKind: overrides.runtimeKind ?? 'openai_codex',
    runId,
    sessionId: overrides.sessionId ?? 'session-1',
    flowId: overrides.flowId ?? 'flow-1',
    subscribedItemKinds:
      overrides.subscribedItemKinds ?? ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      runId,
      sequence,
      ...item,
    },
  }
}

function createAdapter(
  listProjects: ListProjectsResponseDto,
  options?: {
    snapshot?: ProjectSnapshotResponseDto
    status?: RepositoryStatusResponseDto
    diff?: RepositoryDiffResponseDto
    runtimeSession?: RuntimeSessionDto
    runtimeRun?: RuntimeRunDto | null
    startLoginResult?: RuntimeSessionDto
    submitCallbackResult?: RuntimeSessionDto
    startRuntimeResult?: RuntimeSessionDto
    startRuntimeRunError?: CadenceDesktopError
    logoutResult?: RuntimeSessionDto
    notificationDispatches?: ListNotificationDispatchesResponseDto['dispatches']
    notificationRoutes?: ListNotificationRoutesResponseDto['routes']
    notificationDispatchError?: CadenceDesktopError
    upsertRouteError?: CadenceDesktopError
    subscribeError?: CadenceDesktopError
    subscribeResponse?: SubscribeRuntimeStreamResponseDto
  },
) {
  const subscriptions: Array<{
    projectId: string
    handler: (payload: RuntimeStreamEventDto) => void
    onError?: (error: CadenceDesktopError) => void
    unsubscribe: ReturnType<typeof vi.fn>
  }> = []
  let currentSnapshot = options?.snapshot ?? makeSnapshot()
  let currentStatus = options?.status ?? makeStatus()
  let currentDiff = options?.diff ?? makeDiff()
  let currentRuntimeRun = options?.runtimeRun ?? null
  let currentNotificationDispatches = options?.notificationDispatches ?? []
  let currentNotificationRoutes = options?.notificationRoutes ?? []
  let runtimeUpdatedHandler: ((payload: RuntimeUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null

  const adapter: CadenceDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder: async () => null,
    importRepository: async () => {
      throw new Error('not used in App tests')
    },
    listProjects: async () => listProjects,
    getProjectSnapshot: async () => currentSnapshot,
    getRepositoryStatus: async () => currentStatus,
    getRepositoryDiff: async () => currentDiff,
    getRuntimeRun: async () => currentRuntimeRun,
    getRuntimeSession: async () => options?.runtimeSession ?? createRuntimeSession('project-1'),
    startOpenAiLogin: async () =>
      options?.startLoginResult ??
      createRuntimeSession('project-1', {
        phase: 'awaiting_browser_callback',
        flowId: 'flow-1',
        lastErrorCode: null,
        lastError: null,
      }),
    submitOpenAiCallback: async () =>
      options?.submitCallbackResult ??
      createRuntimeSession('project-1', {
        phase: 'authenticated',
        flowId: null,
        sessionId: 'session-1',
        accountId: 'acct-1',
        lastErrorCode: null,
        lastError: null,
      }),
    startRuntimeRun: async () => {
      if (options?.startRuntimeRunError) {
        throw options.startRuntimeRunError
      }

      currentRuntimeRun = currentRuntimeRun
        ? {
            ...currentRuntimeRun,
            status: 'running',
            transport: {
              ...currentRuntimeRun.transport,
              liveness: 'reachable',
            },
            lastErrorCode: null,
            lastError: null,
            stoppedAt: null,
            updatedAt: '2026-04-15T20:06:00Z',
          }
        : createRuntimeRun('project-1', {
            status: 'running',
            stoppedAt: null,
            lastErrorCode: null,
            lastError: null,
          })

      return currentRuntimeRun
    },
    startRuntimeSession: async () =>
      options?.startRuntimeResult ??
      createRuntimeSession('project-1', {
        phase: 'authenticated',
        sessionId: 'session-1',
        accountId: 'acct-1',
        lastErrorCode: null,
        lastError: null,
      }),
    stopRuntimeRun: async (_projectId, runId) => {
      currentRuntimeRun = currentRuntimeRun
        ? {
            ...currentRuntimeRun,
            runId,
            status: 'stopped',
            stoppedAt: '2026-04-15T20:05:00Z',
            updatedAt: '2026-04-15T20:05:00Z',
          }
        : null

      return currentRuntimeRun
    },
    logoutRuntimeSession: async () => options?.logoutResult ?? createRuntimeSession('project-1'),
    resolveOperatorAction: async (_projectId, actionId, decision) => {
      const existingApproval =
        currentSnapshot.approvalRequests.find((approval) => approval.actionId === actionId) ??
        currentSnapshot.approvalRequests[0] ??
        {
          actionId,
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_worktree',
          title: 'Review worktree changes',
          detail: 'Inspect the pending repository diff before continuing.',
          status: 'pending' as const,
          decisionNote: null,
          createdAt: '2026-04-13T20:01:00Z',
          updatedAt: '2026-04-13T20:01:00Z',
          resolvedAt: null,
        }

      const approvalRequest = {
        ...existingApproval,
        actionId,
        status: decision === 'approve' ? 'approved' : 'rejected',
        updatedAt: '2026-04-13T20:02:00Z',
        resolvedAt: '2026-04-13T20:02:00Z',
      }
      const verificationRecord = {
        id: currentSnapshot.verificationRecords.length + 1,
        sourceActionId: actionId,
        status: decision === 'approve' ? 'passed' : 'failed',
        summary: decision === 'approve' ? 'Approved operator action.' : 'Rejected operator action.',
        detail: null,
        recordedAt: '2026-04-13T20:02:01Z',
      }

      currentSnapshot = {
        ...currentSnapshot,
        approvalRequests: [
          approvalRequest,
          ...currentSnapshot.approvalRequests.filter((approval) => approval.actionId !== actionId),
        ],
        verificationRecords: [verificationRecord, ...currentSnapshot.verificationRecords],
      }

      return {
        approvalRequest,
        verificationRecord,
      }
    },
    resumeOperatorRun: async (_projectId, actionId) => {
      const existingApproval =
        currentSnapshot.approvalRequests.find((approval) => approval.actionId === actionId) ??
        currentSnapshot.approvalRequests[0] ??
        {
          actionId,
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_worktree',
          title: 'Review worktree changes',
          detail: 'Inspect the pending repository diff before continuing.',
          status: 'approved' as const,
          decisionNote: null,
          createdAt: '2026-04-13T20:01:00Z',
          updatedAt: '2026-04-13T20:02:00Z',
          resolvedAt: '2026-04-13T20:02:00Z',
        }

      const approvalRequest = {
        ...existingApproval,
        actionId,
        status: 'approved' as const,
        updatedAt: '2026-04-13T20:03:00Z',
        resolvedAt: existingApproval.resolvedAt ?? '2026-04-13T20:02:00Z',
      }
      const resumeEntry = {
        id: currentSnapshot.resumeHistory.length + 1,
        sourceActionId: actionId,
        sessionId: approvalRequest.sessionId ?? 'session-1',
        status: 'started' as const,
        summary: 'Operator resumed the selected project runtime session.',
        createdAt: '2026-04-13T20:03:30Z',
      }

      currentSnapshot = {
        ...currentSnapshot,
        approvalRequests: [
          approvalRequest,
          ...currentSnapshot.approvalRequests.filter((approval) => approval.actionId !== actionId),
        ],
        resumeHistory: [resumeEntry, ...currentSnapshot.resumeHistory],
      }

      return {
        approvalRequest,
        resumeEntry,
      }
    },
    listNotificationRoutes: async () => ({
      routes: currentNotificationRoutes,
    }),
    listNotificationDispatches: async () => {
      if (options?.notificationDispatchError) {
        throw options.notificationDispatchError
      }

      return {
        dispatches: currentNotificationDispatches,
      }
    },
    upsertNotificationRoute: async (request) => {
      if (options?.upsertRouteError) {
        throw options.upsertRouteError
      }

      const now = '2026-04-16T15:10:00Z'
      const existingRoute =
        currentNotificationRoutes.find((route) => route.routeId === request.routeId) ?? null
      const nextRoute = {
        projectId: request.projectId,
        routeId: request.routeId,
        routeKind: request.routeKind,
        routeTarget: request.routeTarget,
        enabled: request.enabled,
        metadataJson: request.metadataJson ?? null,
        createdAt: existingRoute?.createdAt ?? now,
        updatedAt: now,
      }

      currentNotificationRoutes = [
        nextRoute,
        ...currentNotificationRoutes.filter((route) => route.routeId !== request.routeId),
      ]

      return {
        route: nextRoute,
      }
    },
    recordNotificationDispatchOutcome: async () => {
      throw new Error('not used in App tests')
    },
    submitNotificationReply: async () => {
      throw new Error('not used in App tests')
    },
    syncNotificationAdapters: async (projectId: string) => ({
      projectId,
      dispatch: {
        projectId,
        pendingCount: 0,
        attemptedCount: 0,
        sentCount: 0,
        failedCount: 0,
        attemptLimit: 64,
        attemptsTruncated: false,
        attempts: [],
        errorCodeCounts: [],
      },
      replies: {
        projectId,
        routeCount: 0,
        polledRouteCount: 0,
        messageCount: 0,
        acceptedCount: 0,
        rejectedCount: 0,
        attemptLimit: 256,
        attemptsTruncated: false,
        attempts: [],
        errorCodeCounts: [],
      },
      syncedAt: '2026-04-17T03:00:00Z',
    }),
    subscribeRuntimeStream: async (projectId, _itemKinds, handler, onError) => {
      if (options?.subscribeError) {
        throw options.subscribeError
      }

      const subscription = {
        projectId,
        handler,
        onError,
        unsubscribe: vi.fn(),
      }
      subscriptions.push(subscription)

      return {
        response:
          options?.subscribeResponse ??
          makeStreamResponse(projectId, {
            runId: currentRuntimeRun?.runId ?? `run-${projectId}`,
            sessionId: options?.runtimeSession?.sessionId ?? 'session-1',
            flowId: options?.runtimeSession?.flowId ?? 'flow-1',
          }),
        unsubscribe: subscription.unsubscribe,
      }
    },
    onProjectUpdated: async (_handler: (payload: ProjectUpdatedPayloadDto) => void) => () => undefined,
    onRepositoryStatusChanged: async (
      _handler: (payload: RepositoryStatusChangedPayloadDto) => void,
    ) => () => undefined,
    onRuntimeUpdated: async (handler: (payload: RuntimeUpdatedPayloadDto) => void) => {
      runtimeUpdatedHandler = handler
      return () => {
        runtimeUpdatedHandler = null
      }
    },
    onRuntimeRunUpdated: async (handler: (payload: RuntimeRunUpdatedPayloadDto) => void) => {
      runtimeRunUpdatedHandler = handler
      return () => {
        runtimeRunUpdatedHandler = null
      }
    },
  }

  return {
    adapter,
    subscriptions,
    emitRuntimeStream(index: number, payload: RuntimeStreamEventDto) {
      subscriptions[index]?.handler(payload)
    },
    emitRuntimeStreamError(index: number, error: CadenceDesktopError) {
      subscriptions[index]?.onError?.(error)
    },
    emitRuntimeUpdated(payload: RuntimeUpdatedPayloadDto) {
      runtimeUpdatedHandler?.(payload)
    },
    emitRuntimeRunUpdated(payload: RuntimeRunUpdatedPayloadDto) {
      currentRuntimeRun = payload.run
      runtimeRunUpdatedHandler?.(payload)
    },
  }
}

describe('Cadence desktop shell', () => {
  it(
    'renders truthful zero-phase desktop state from the adapter',
    async () => {
      render(
        <CadenceApp
          adapter={createAdapter({
            projects: [makeProjectSummary('project-1', 'api-gateway')],
          }).adapter}
        />,
      )

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      expect(screen.getByText('Planning lifecycle')).toBeVisible()
      expect(screen.getByText('Lifecycle projection unavailable')).toBeVisible()
      expect(screen.getByRole('heading', { name: 'No phases available yet' })).toBeVisible()
      expect(screen.getByRole('button', { name: 'Workflow' })).toBeVisible()
      expect(screen.getByRole('button', { name: 'Agent' })).toBeVisible()
      expect(screen.getByRole('button', { name: 'Execution' })).toBeVisible()
      expect(screen.getAllByText('No milestone assigned').length).toBeGreaterThan(0)
      expect(screen.getByText('0/0 phases (legacy)')).toBeVisible()
      expect(screen.getByText('0 paths')).toBeVisible()
      expect(screen.getAllByText('No branch').length).toBeGreaterThan(0)
      expect(screen.getAllByText('Runtime unavailable').length).toBeGreaterThan(0)
      expect(screen.getAllByText('project-1').length).toBeGreaterThan(0)
    },
    10_000,
  )

  it('renders lifecycle-first cards while keeping legacy phase details available on demand', async () => {
    render(
      <CadenceApp
        adapter={createAdapter(
          {
            projects: [
              makeProjectSummary('project-1', 'api-gateway', {
                description: 'Desktop shell',
                milestone: 'M001',
                totalPhases: 3,
                completedPhases: 1,
                activePhase: 2,
                branch: null,
                runtime: null,
              }),
            ],
          },
          {
            snapshot: makeSnapshot({
              project: makeProjectSummary('project-1', 'api-gateway', {
                description: 'Desktop shell',
                milestone: 'M001',
                totalPhases: 3,
                completedPhases: 1,
                activePhase: 2,
                branch: null,
                runtime: null,
              }),
              lifecycle: {
                stages: [
                  makeLifecycleStage('discussion', {
                    status: 'complete',
                    lastTransitionAt: '2026-04-15T17:59:00Z',
                  }),
                  makeLifecycleStage('research', {
                    status: 'active',
                    lastTransitionAt: '2026-04-15T18:00:00Z',
                  }),
                  makeLifecycleStage('requirements', {
                    status: 'blocked',
                    actionRequired: true,
                    lastTransitionAt: '2026-04-15T18:01:00Z',
                  }),
                  makeLifecycleStage('roadmap', {
                    status: 'pending',
                  }),
                ],
              },
              phases: [
                {
                  id: 1,
                  name: 'Import',
                  description: 'Import the repository into Cadence',
                  status: 'complete',
                  currentStep: null,
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
                  taskCount: 3,
                  completedTasks: 2,
                  summary: null,
                },
                {
                  id: 3,
                  name: 'Ship proof',
                  description: 'Close the slice with a real build',
                  status: 'pending',
                  currentStep: null,
                  taskCount: 1,
                  completedTasks: 0,
                  summary: '   ',
                },
              ],
              approvalRequests: [
                {
                  actionId: 'action-pending',
                  sessionId: 'session-1',
                  flowId: 'flow-1',
                  actionType: 'review_plan',
                  title: 'Confirm roadmap handoff',
                  detail: 'Review the latest handoff package before resuming autonomous progression.',
                  gateNodeId: 'workflow-requirements',
                  gateKey: 'requires_user_input',
                  transitionFromNodeId: 'workflow-research',
                  transitionToNodeId: 'workflow-requirements',
                  transitionKind: 'advance',
                  userAnswer: null,
                  status: 'pending',
                  decisionNote: null,
                  createdAt: '2026-04-15T18:06:00Z',
                  updatedAt: '2026-04-15T18:06:00Z',
                  resolvedAt: null,
                },
              ],
              handoffPackages: [
                makeHandoffPackage('project-1', {
                  id: 1,
                  handoffTransitionId: 'auto:txn-001',
                  causalTransitionId: 'txn-000',
                  fromNodeId: 'workflow-discussion',
                  toNodeId: 'workflow-research',
                  transitionKind: 'advance',
                  packagePayload: '{"handoff":"first-redacted"}',
                  packageHash: 'hash-001',
                  createdAt: '2026-04-15T18:00:00Z',
                }),
                makeHandoffPackage('project-1', {
                  id: 2,
                  handoffTransitionId: 'auto:txn-002',
                  causalTransitionId: 'txn-001',
                  fromNodeId: 'workflow-research',
                  toNodeId: 'workflow-requirements',
                  transitionKind: 'advance',
                  packagePayload: '{"handoff":"latest-redacted"}',
                  packageHash: 'hash-002',
                  createdAt: '2026-04-15T18:05:00Z',
                }),
              ],
              verificationRecords: [],
              resumeHistory: [],
            }),
            status: makeStatus({
              branch: {
                name: 'feature/workflow-shell',
                headSha: 'abc1234',
                detached: false,
              },
              entries: [
                {
                  path: 'client/components/cadence/project-rail.tsx',
                  staged: null,
                  unstaged: 'modified',
                  untracked: false,
                },
              ],
              hasStagedChanges: false,
              hasUnstagedChanges: true,
              hasUntrackedChanges: false,
            }),
          },
        ).adapter}
      />,
    )

    expect(await screen.findByText('Discussion')).toBeVisible()
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
    expect(screen.queryByText('first-redacted')).not.toBeInTheDocument()
    expect(screen.queryByText('latest-redacted')).not.toBeInTheDocument()
    expect(screen.getByText('Active · P2')).toBeVisible()
    expect(screen.getAllByText('feature/workflow-shell').length).toBeGreaterThan(0)
    expect(screen.queryByText('Workflow truth')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Show legacy phase details/i }))

    expect(screen.getByText('Workflow truth')).toBeVisible()
    expect(screen.getByText('2 · Workflow truth')).toBeVisible()
    expect(screen.getByText('2/3')).toBeVisible()
    expect(screen.queryByText('No active phase yet')).not.toBeInTheDocument()
  })

  it('renders terminal completion handoff truth without pending operator input', async () => {
    render(
      <CadenceApp
        adapter={createAdapter(
          {
            projects: [
              makeProjectSummary('project-1', 'api-gateway', {
                description: 'Desktop shell',
                milestone: 'M001',
                totalPhases: 4,
                completedPhases: 4,
                activePhase: 0,
                branch: null,
                runtime: null,
              }),
            ],
          },
          {
            snapshot: makeSnapshot({
              project: makeProjectSummary('project-1', 'api-gateway', {
                description: 'Desktop shell',
                milestone: 'M001',
                totalPhases: 4,
                completedPhases: 4,
                activePhase: 0,
                branch: null,
                runtime: null,
              }),
              lifecycle: {
                stages: [
                  makeLifecycleStage('discussion', {
                    status: 'complete',
                    lastTransitionAt: '2026-04-16T10:00:00Z',
                  }),
                  makeLifecycleStage('research', {
                    status: 'complete',
                    lastTransitionAt: '2026-04-16T10:01:00Z',
                  }),
                  makeLifecycleStage('requirements', {
                    status: 'complete',
                    lastTransitionAt: '2026-04-16T10:02:00Z',
                  }),
                  makeLifecycleStage('roadmap', {
                    status: 'complete',
                    lastTransitionAt: '2026-04-16T10:03:00Z',
                  }),
                ],
              },
              phases: [
                {
                  id: 1,
                  name: 'Import',
                  description: 'Import the repository into Cadence',
                  status: 'complete',
                  currentStep: null,
                  taskCount: 2,
                  completedTasks: 2,
                  summary: 'Imported successfully',
                },
                {
                  id: 2,
                  name: 'Workflow truth',
                  description: 'Project persisted phases into the shell',
                  status: 'complete',
                  currentStep: null,
                  taskCount: 3,
                  completedTasks: 3,
                  summary: 'Lifecycle completed with no pending gates.',
                },
                {
                  id: 3,
                  name: 'Ship proof',
                  description: 'Close the slice with a real build',
                  status: 'complete',
                  currentStep: null,
                  taskCount: 1,
                  completedTasks: 1,
                  summary: 'Debug build passed.',
                },
                {
                  id: 4,
                  name: 'Archive',
                  description: 'Mark the run complete',
                  status: 'complete',
                  currentStep: null,
                  taskCount: 1,
                  completedTasks: 1,
                  summary: 'Milestone archived.',
                },
              ],
              approvalRequests: [],
              handoffPackages: [
                makeHandoffPackage('project-1', {
                  id: 4,
                  handoffTransitionId: 'auto:txn-004',
                  causalTransitionId: 'txn-003',
                  fromNodeId: 'workflow-requirements',
                  toNodeId: 'workflow-roadmap',
                  transitionKind: 'advance',
                  packagePayload: '{"handoff":"completion-redacted"}',
                  packageHash: 'hash-004',
                  createdAt: '2026-04-16T10:03:30Z',
                }),
              ],
              verificationRecords: [],
              resumeHistory: [],
            }),
          },
        ).adapter}
      />,
    )

    expect(await screen.findByText('Discussion')).toBeVisible()
    expect(screen.getAllByText('100%').length).toBeGreaterThan(0)
    expect(screen.getByText('4/4 lifecycle stages complete')).toBeVisible()
    expect(screen.getByText('0 stages need action')).toBeVisible()
    expect(screen.getByText('Workflow handoff truth')).toBeVisible()
    expect(screen.getByText('1 persisted package')).toBeVisible()
    expect(screen.getByText('Autonomous loop complete')).toBeVisible()
    expect(screen.getByText('auto:txn-004')).toBeVisible()
    expect(screen.getByText('txn-003')).toBeVisible()
    expect(screen.getByText('workflow-requirements → workflow-roadmap (advance)')).toBeVisible()
    expect(screen.getByText('hash-004')).toBeVisible()
    expect(screen.getByText('2026-04-16T10:03:30Z')).toBeVisible()
    expect(screen.queryByText('Waiting on operator input')).not.toBeInTheDocument()
    expect(screen.queryByText('completion-redacted')).not.toBeInTheDocument()
  })

  it('renders the subscribed live agent feed alongside durable operator approvals in the real shell path', async () => {
    const setup = createAdapter(
      {
        projects: [makeProjectSummary('project-1', 'api-gateway')],
      },
      {
        snapshot: makeSnapshot({
          approvalRequests: [
            {
              actionId: 'action-1',
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'review_worktree',
              title: 'Review worktree changes',
              detail: 'Inspect the pending repository diff before continuing.',
              status: 'pending',
              decisionNote: null,
              createdAt: '2026-04-13T20:01:02Z',
              updatedAt: '2026-04-13T20:01:02Z',
              resolvedAt: null,
            },
          ],
        }),
        runtimeSession: createRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          accountId: 'acct-1',
          flowId: 'flow-1',
          lastErrorCode: null,
          lastError: null,
        }),
        runtimeRun: createRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    )

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(
      () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
      { timeout: 10_000 },
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(await screen.findByRole('heading', { name: 'OpenAI runtime session connected' })).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Consolidated trust surface for unattended operation' })).toBeVisible()
    expect(screen.getByText('Permission scope')).toBeVisible()
    expect(screen.getByText('Storage boundaries')).toBeVisible()
    await waitFor(() => expect(setup.subscriptions).toHaveLength(1))
    expect(screen.getByRole('heading', { name: 'Connecting the run-scoped live feed' })).toBeVisible()

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to cadence.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        }, { sequence: 1, runId: 'run-project-1' }),
      )
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'activity',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: 'Recovered replay attached',
          detail: 'Cadence resumed the active run-scoped stream after reload.',
          code: 'stream.attach',
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:01Z',
        }, { sequence: 2, runId: 'run-project-1' }),
      )
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'tool',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'bootstrap-repository-context',
          toolName: 'inspect_repository_context',
          toolState: 'running',
          actionType: null,
          title: null,
          detail: 'Collecting repository status.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:02Z',
        }, { sequence: 3, runId: 'run-project-1' }),
      )
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'action_required',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId: 'action-1',
          boundaryId: 'boundary-1',
          actionType: 'review_worktree',
          title: 'Repository has local changes',
          detail: 'Review the worktree before trusting agent actions.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:03Z',
        }, { sequence: 4, runId: 'run-project-1' }),
      )
    })

    expect(
      await screen.findByRole('heading', {
        name: /Streaming run-scoped live activity|Replaying recent run-scoped activity/,
      }),
    ).toBeVisible()
    expect(screen.getByText('Connected to cadence.')).toBeVisible()
    expect(screen.getByText('Recovered replay attached')).toBeVisible()
    expect(screen.getByText('Cadence resumed the active run-scoped stream after reload.')).toBeVisible()
    expect(screen.getByText('inspect_repository_context')).toBeVisible()
    expect(screen.getByText('Collecting repository status.')).toBeVisible()
    expect(screen.getByText('Runtime activity')).toBeVisible()
    expect(screen.getAllByText('run-project-1').length).toBeGreaterThan(0)
    expect(screen.getByRole('heading', { name: 'Durable operator loop truth for the selected repo' })).toBeVisible()
    expect(screen.getByText('Review worktree changes')).toBeVisible()
    expect(screen.getByText('Inspect the pending repository diff before continuing.')).toBeVisible()
    expect(screen.getByText('Waiting for operator input before this action can resume the run.')).toBeVisible()
    expect(screen.getByText('Resolve pending operator approvals so autonomous continuation is no longer blocked.')).toBeVisible()
    expect(screen.getByText(/never render raw secret values here/i)).toBeVisible()
    expect(screen.getByRole('button', { name: 'Approve' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Reject' })).toBeVisible()
    expect([
      'Live runtime activity is streaming for the active supervised run. Composer remains read-only in this shell.',
      'Cadence is replaying recent run-scoped activity for run-project-1 while the live feed catches up.',
    ]).toContain(screen.getByLabelText('Agent input unavailable').getAttribute('placeholder'))
  }, 10_000)

  it('keeps transcript history visible when the subscribed stream fails and allows retry', async () => {
    const setup = createAdapter(
      {
        projects: [makeProjectSummary('project-1', 'api-gateway')],
      },
      {
        runtimeSession: createRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          accountId: 'acct-1',
          flowId: 'flow-1',
          lastErrorCode: null,
          lastError: null,
        }),
        runtimeRun: createRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    )

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(
      () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
      { timeout: 10_000 },
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    await waitFor(() => expect(setup.subscriptions).toHaveLength(1))

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to cadence.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        }),
      )
    })

    expect(await screen.findByText('Connected to cadence.')).toBeVisible()

    act(() => {
      setup.emitRuntimeStreamError(
        0,
        new CadenceDesktopError({
          code: 'runtime_stream_bootstrap_failed',
          errorClass: 'retryable',
          message: 'Cadence lost the runtime bootstrap stream while collecting repository context.',
          retryable: true,
        }),
      )
    })

    await waitFor(() =>
      expect(
        screen.getAllByText('Cadence lost the runtime bootstrap stream while collecting repository context.').length,
      ).toBeGreaterThan(0),
    )
    expect(screen.getByText('Connected to cadence.')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Retry live feed' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Retry live feed' }))

    await waitFor(() => expect(setup.subscriptions).toHaveLength(2))
    expect(screen.getByRole('heading', { name: 'Replaying recent run-scoped activity' })).toBeVisible()
    expect(screen.getByText('Replaying recent run-scoped backlog')).toBeVisible()
    expect(screen.getByText('Connected to cadence.')).toBeVisible()
  }, 10_000)

  it('surfaces same-session run replacement diagnostics on the real shell path', async () => {
    const setup = createAdapter(
      {
        projects: [makeProjectSummary('project-1', 'api-gateway')],
      },
      {
        runtimeSession: createRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          accountId: 'acct-1',
          flowId: 'flow-1',
          lastErrorCode: null,
          lastError: null,
        }),
        runtimeRun: createRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    )

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(
      () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
      { timeout: 10_000 },
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    await waitFor(() => expect(setup.subscriptions).toHaveLength(1))

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'First run transcript.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        }, { sequence: 1, runId: 'run-project-1' }),
      )
    })

    expect(await screen.findByText('First run transcript.')).toBeVisible()

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: createRuntimeRun('project-1', {
          runId: 'run-project-2',
          updatedAt: '2026-04-15T20:06:00Z',
        }),
      })
    })

    await waitFor(() => expect(setup.subscriptions).toHaveLength(2))
    expect(screen.getByText('Switched to a new supervised run')).toBeVisible()
    expect(screen.getAllByText(/run-project-1/).length).toBeGreaterThan(0)
    expect(screen.getAllByText(/run-project-2/).length).toBeGreaterThan(0)

    act(() => {
      setup.emitRuntimeStream(
        1,
        makeStreamEvent('project-1', {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Second run transcript.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:05Z',
        }, { sequence: 1, runId: 'run-project-2' }),
      )
    })

    expect(await screen.findByText('Second run transcript.')).toBeVisible()
    await waitFor(() => expect(screen.queryByText('Switched to a new supervised run')).not.toBeInTheDocument())
  }, 10_000)

  it(
    'runs the real Agent pane login flow with browser open and manual redirect completion',
    async () => {
      openUrlMock.mockResolvedValueOnce(undefined)

      render(
        <CadenceApp
          adapter={createAdapter(
            {
              projects: [makeProjectSummary('project-1', 'api-gateway')],
            },
            {
              runtimeSession: createRuntimeSession('project-1'),
              startLoginResult: createRuntimeSession('project-1', {
                phase: 'awaiting_manual_input',
                flowId: 'flow-1',
                callbackBound: false,
                lastErrorCode: 'callback_listener_bind_failed',
                lastError: {
                  code: 'callback_listener_bind_failed',
                  message: 'Paste the redirect URL to finish login.',
                  retryable: false,
                },
              }),
              submitCallbackResult: createRuntimeSession('project-1', {
                phase: 'authenticated',
                flowId: null,
                sessionId: 'session-1',
                accountId: 'acct-1',
                lastErrorCode: null,
                lastError: null,
              }),
            },
          ).adapter}
        />,
      )

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

      expect(await screen.findByRole('heading', { name: 'Sign in to OpenAI for this project' })).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Sign in with OpenAI' }))

      await waitFor(() =>
        expect(openUrlMock).toHaveBeenCalledWith('https://auth.openai.com/oauth/authorize?client_id=test'),
      )
      expect(
        await screen.findByRole('heading', { name: 'Keep the login flow moving even if the browser callback is brittle' }),
      ).toBeVisible()

      fireEvent.change(screen.getByLabelText('OpenAI redirect URL'), {
        target: { value: 'http://127.0.0.1:1455/auth/callback?code=test&state=flow-1' },
      })
      fireEvent.click(screen.getByRole('button', { name: 'Complete pasted redirect' }))

      expect(await screen.findByRole('heading', { name: 'OpenAI runtime session connected' })).toBeVisible()
      expect(screen.getAllByText('acct-1').length).toBeGreaterThan(0)
      expect(screen.getAllByText('session-1').length).toBeGreaterThan(0)
      expect(screen.getByRole('button', { name: 'Sign out' })).toBeVisible()
      expect([
        'Start or reconnect a supervised run to create the run-scoped live feed for this imported project.',
        'Cadence is connecting the run-scoped live stream for run-1.',
      ]).toContain(screen.getByLabelText('Agent input unavailable').getAttribute('placeholder'))
    },
    10_000,
  )

  it(
    'starts and stops a supervised runtime run from the real Agent pane shell path',
    async () => {
      render(
        <CadenceApp
          adapter={createAdapter(
            {
              projects: [makeProjectSummary('project-1', 'api-gateway')],
            },
            {
              runtimeSession: createRuntimeSession('project-1', {
                phase: 'authenticated',
                sessionId: 'session-1',
                accountId: 'acct-1',
                flowId: 'flow-1',
                lastErrorCode: null,
                lastError: null,
              }),
              runtimeRun: null,
            },
          ).adapter}
        />,
      )

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

      expect(await screen.findByRole('heading', { name: 'No durable runtime run yet' })).toBeVisible()
      expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
      expect(screen.getByText('No supervised run is attached')).toBeVisible()
      fireEvent.click(screen.getByRole('button', { name: 'Start supervised run' }))

      expect(await screen.findByRole('heading', { name: 'Recovered durable runtime run' })).toBeVisible()
      expect(screen.getAllByText('run-1').length).toBeGreaterThan(0)
      expect(screen.getByRole('button', { name: 'Stop run' })).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Stop run' }))

      await waitFor(() => expect(screen.getAllByText('Run stopped').length).toBeGreaterThan(0))
      expect(screen.getByText('Supervisor stopped cleanly')).toBeVisible()
      expect(screen.getByRole('button', { name: 'Start new supervised run' })).toBeVisible()
      expect(screen.queryByRole('button', { name: 'Stop run' })).not.toBeInTheDocument()
    },
    10_000,
  )

  it(
    'preserves the last truthful no-run state when supervised-run start times out',
    async () => {
      render(
        <CadenceApp
          adapter={createAdapter(
            {
              projects: [makeProjectSummary('project-1', 'api-gateway')],
            },
            {
              runtimeSession: createRuntimeSession('project-1', {
                phase: 'authenticated',
                sessionId: 'session-1',
                accountId: 'acct-1',
                flowId: 'flow-1',
                lastErrorCode: null,
                lastError: null,
              }),
              runtimeRun: null,
              startRuntimeRunError: new CadenceDesktopError({
                code: 'runtime_run_start_timeout',
                errorClass: 'retryable',
                message: 'Timed out while reconnecting the detached supervisor.',
                retryable: true,
              }),
            },
          ).adapter}
        />,
      )

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

      expect(await screen.findByRole('heading', { name: 'OpenAI runtime session connected' })).toBeVisible()
      expect(screen.getByRole('heading', { name: 'No durable runtime run yet' })).toBeVisible()
      expect(screen.getByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
      expect(screen.queryByRole('button', { name: 'Stop run' })).not.toBeInTheDocument()

      fireEvent.click(screen.getByRole('button', { name: 'Start supervised run' }))

      await waitFor(() => expect(screen.getByText('Run control needs retry')).toBeVisible())
      expect(screen.getByText('Timed out while reconnecting the detached supervisor.')).toBeVisible()
      expect(screen.getByRole('heading', { name: 'OpenAI runtime session connected' })).toBeVisible()
      expect(screen.getByRole('heading', { name: 'No durable runtime run yet' })).toBeVisible()
      expect(screen.getByRole('button', { name: 'Start supervised run' })).toBeEnabled()
      expect(screen.queryByRole('button', { name: 'Stop run' })).not.toBeInTheDocument()
    },
    10_000,
  )

  it(
    'keeps stale recovered run controls separate from auth login state after reload',
    async () => {
      render(
        <CadenceApp
          adapter={createAdapter(
            {
              projects: [makeProjectSummary('project-1', 'api-gateway')],
            },
            {
              runtimeSession: createRuntimeSession('project-1'),
              runtimeRun: createRuntimeRun('project-1', {
                status: 'stale',
                transport: {
                  kind: 'tcp',
                  endpoint: '127.0.0.1:4455',
                  liveness: 'unreachable',
                },
                lastHeartbeatAt: '2026-04-15T19:58:00Z',
              }),
            },
          ).adapter}
        />,
      )

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

      expect(await screen.findByRole('heading', { name: 'Sign in to OpenAI for this project' })).toBeVisible()
      expect(screen.getByText('Supervisor heartbeat is stale')).toBeVisible()
      expect(screen.getByRole('button', { name: 'Reconnect supervisor' })).toBeEnabled()
      expect(screen.getByRole('button', { name: 'Stop run' })).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Reconnect supervisor' }))

      await waitFor(() => expect(screen.getAllByText('Supervisor running').length).toBeGreaterThan(0))
      expect(screen.queryByText('Supervisor heartbeat is stale')).not.toBeInTheDocument()
    },
    10_000,
  )

  it(
    'keeps last-known-good selected-project UI state when project-2 reload fails, then converges on retry',
    async () => {
      const projectOne = makeProjectSummary('project-1', 'api-gateway', {
        description: 'Gateway shell',
        milestone: 'M001',
      })
      const projectTwo = makeProjectSummary('project-2', 'orchestra', {
        description: 'Orchestration shell',
        milestone: 'M002',
      })

      const setup = createAdapter(
        {
          projects: [projectOne, projectTwo],
        },
        {
          snapshot: makeSnapshot({
            project: projectOne,
            repository: {
              id: 'repo-1',
              projectId: 'project-1',
              rootPath: '/tmp/api-gateway',
              displayName: 'api-gateway',
              branch: null,
              headSha: null,
              isGitRepo: true,
            },
          }),
          status: makeStatus({
            repository: {
              id: 'repo-1',
              projectId: 'project-1',
              rootPath: '/tmp/api-gateway',
              displayName: 'api-gateway',
              branch: null,
              headSha: null,
              isGitRepo: true,
            },
          }),
          runtimeSession: createRuntimeSession('project-1'),
          runtimeRun: createRuntimeRun('project-1', { runId: 'run-project-1' }),
        },
      )

      const snapshots: Record<string, ProjectSnapshotResponseDto> = {
        'project-1': makeSnapshot({
          project: projectOne,
          repository: {
            id: 'repo-1',
            projectId: 'project-1',
            rootPath: '/tmp/api-gateway',
            displayName: 'api-gateway',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
        }),
        'project-2': makeSnapshot({
          project: projectTwo,
          repository: {
            id: 'repo-2',
            projectId: 'project-2',
            rootPath: '/tmp/orchestra',
            displayName: 'orchestra',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
        }),
      }
      const statuses: Record<string, RepositoryStatusResponseDto> = {
        'project-1': makeStatus({
          repository: {
            id: 'repo-1',
            projectId: 'project-1',
            rootPath: '/tmp/api-gateway',
            displayName: 'api-gateway',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
        }),
        'project-2': makeStatus({
          repository: {
            id: 'repo-2',
            projectId: 'project-2',
            rootPath: '/tmp/orchestra',
            displayName: 'orchestra',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
        }),
      }
      const runtimeSessions: Record<string, RuntimeSessionDto> = {
        'project-1': createRuntimeSession('project-1'),
        'project-2': createRuntimeSession('project-2'),
      }
      const runtimeRuns: Record<string, RuntimeRunDto | null> = {
        'project-1': createRuntimeRun('project-1', { runId: 'run-project-1' }),
        'project-2': createRuntimeRun('project-2', { runId: 'run-project-2' }),
      }

      let failProjectTwoSnapshot = true
      setup.adapter.getProjectSnapshot = vi.fn(async (projectId: string) => {
        if (projectId === 'project-2' && failProjectTwoSnapshot) {
          failProjectTwoSnapshot = false
          throw new CadenceDesktopError({
            code: 'project_snapshot_query_failed',
            errorClass: 'retryable',
            message: 'Cadence could not reload project-2 from durable snapshot state.',
            retryable: true,
          })
        }

        return snapshots[projectId]
      })
      setup.adapter.getRepositoryStatus = vi.fn(async (projectId: string) => statuses[projectId])
      setup.adapter.getRuntimeSession = vi.fn(async (projectId: string) => runtimeSessions[projectId])
      setup.adapter.getRuntimeRun = vi.fn(async (projectId: string) => runtimeRuns[projectId] ?? null)
      setup.adapter.listNotificationDispatches = vi.fn(async () => ({ dispatches: [] }))
      setup.adapter.listNotificationRoutes = vi.fn(async () => ({ routes: [] }))
      setup.adapter.syncNotificationAdapters = vi.fn(async (projectId: string) => ({
        projectId,
        dispatch: {
          projectId,
          pendingCount: 0,
          attemptedCount: 0,
          sentCount: 0,
          failedCount: 0,
          attemptLimit: 64,
          attemptsTruncated: false,
          attempts: [],
          errorCodeCounts: [],
        },
        replies: {
          projectId,
          routeCount: 0,
          polledRouteCount: 0,
          messageCount: 0,
          acceptedCount: 0,
          rejectedCount: 0,
          attemptLimit: 256,
          attemptsTruncated: false,
          attempts: [],
          errorCodeCounts: [],
        },
        syncedAt: '2026-04-17T03:00:00Z',
      }))

      const getSelectedRow = () => {
        const row = screen.getByText('Selected').closest('div')
        if (!row) {
          throw new Error('Selected-project row is missing from the project rail.')
        }

        return row
      }

      render(<CadenceApp adapter={setup.adapter} />)

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      expect(getSelectedRow()).toHaveTextContent('project-1')
      expect(screen.getByText('/tmp/api-gateway')).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: /orchestra/i }))

      await waitFor(() =>
        expect(screen.getByText('Cadence could not reload project-2 from durable snapshot state.')).toBeVisible(),
      )
      expect(getSelectedRow()).toHaveTextContent('project-1')
      expect(screen.getByText('/tmp/api-gateway')).toBeVisible()
      expect(screen.queryByText('/tmp/orchestra')).not.toBeInTheDocument()

      fireEvent.click(screen.getByRole('button', { name: /orchestra/i }))

      await waitFor(() => expect(getSelectedRow()).toHaveTextContent('project-2'))
      await waitFor(() => expect(screen.getByText('/tmp/orchestra')).toBeVisible())
      expect(screen.queryByText('Cadence could not reload project-2 from durable snapshot state.')).not.toBeInTheDocument()
    },
    10_000,
  )

  it(
    'persists operator approvals and resume history through the agent and execution panes',
    async () => {
      render(
        <CadenceApp
          adapter={createAdapter(
            {
              projects: [makeProjectSummary('project-1', 'api-gateway')],
            },
            {
              snapshot: makeSnapshot({
                approvalRequests: [
                  {
                    actionId: 'action-1',
                    sessionId: 'session-1',
                    flowId: 'flow-1',
                    actionType: 'review_worktree',
                    title: 'Review worktree changes',
                    detail: 'Inspect the pending repository diff before continuing.',
                    status: 'pending',
                    decisionNote: null,
                    createdAt: '2026-04-13T20:01:00Z',
                    updatedAt: '2026-04-13T20:01:00Z',
                    resolvedAt: null,
                  },
                ],
              }),
              runtimeSession: createRuntimeSession('project-1', {
                phase: 'authenticated',
                sessionId: 'session-1',
                accountId: 'acct-1',
                flowId: 'flow-1',
                lastErrorCode: null,
                lastError: null,
              }),
            },
          ).adapter}
        />,
      )

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
      expect(await screen.findByText('Review worktree changes')).toBeVisible()
      expect(screen.getByText('Waiting for operator input before this action can resume the run.')).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Approve' }))

      await waitFor(() => expect(screen.getAllByText('Approved').length).toBeGreaterThan(0))
      expect(screen.getByRole('button', { name: 'Resume run' })).toBeVisible()
      expect(screen.getByText('No resume recorded yet for this action.')).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Resume run' }))

      await waitFor(() =>
        expect(screen.getByText('Operator resumed the selected project runtime session.')).toBeVisible(),
      )
      expect(
        screen.getByText('Latest resume started: Operator resumed the selected project runtime session.'),
      ).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Execution' }))
      fireEvent.click(screen.getByRole('button', { name: 'Verify' }))

      expect(await screen.findByText('Repo-local operator verification truth')).toBeVisible()
      expect(screen.getByText('Approved operator action.')).toBeVisible()
      expect(screen.getByText('Operator resumed the selected project runtime session.')).toBeVisible()
      expect(screen.getByText('Durable operator verification and resume history are loaded from the selected project snapshot.')).toBeVisible()
    },
    10_000,
  )

  it(
    'projects real repository status counts and diff truth into the execution surface',
    async () => {
      render(
        <CadenceApp
          adapter={createAdapter(
            {
              projects: [makeProjectSummary('project-1', 'api-gateway')],
            },
            {
              status: makeStatus({
                branch: {
                  name: 'feature/live-git',
                  headSha: 'abc1234',
                  detached: false,
                },
                entries: [
                  {
                    path: 'client/src/App.tsx',
                    staged: 'modified',
                    unstaged: null,
                    untracked: false,
                  },
                  {
                    path: 'client/src-tauri/src/lib.rs',
                    staged: null,
                    unstaged: 'modified',
                    untracked: false,
                  },
                ],
                hasStagedChanges: true,
                hasUnstagedChanges: true,
                hasUntrackedChanges: false,
              }),
              diff: makeDiff({
                patch: 'diff --git a/client/src-tauri/src/lib.rs b/client/src-tauri/src/lib.rs\n+real change\n',
                truncated: true,
                baseRevision: null,
              }),
            },
          ).adapter}
        />,
      )

      expect(await screen.findByText('2 paths')).toBeVisible()
      expect(screen.getAllByText('feature/live-git').length).toBeGreaterThan(0)

      fireEvent.click(screen.getByRole('button', { name: 'Execution' }))
      expect(await screen.findByText('Channel dispatch diagnostics')).toBeVisible()
      expect(
        screen.getByText('Cadence has not recorded any notification dispatch rows for this project yet, so channel health stays empty instead of fabricated.'),
      ).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Changes' }))

      expect(await screen.findByText('Active diff')).toBeVisible()
      expect(screen.getByText('abc1234')).toBeVisible()
      expect(screen.getByText('Dirty')).toBeVisible()
      expect(await screen.findByText('truncated')).toBeVisible()
      expect(screen.getByRole('button', { name: 'Refresh' })).toBeVisible()
      expect(screen.getByText('Staged · 1')).toBeVisible()
      expect(screen.getByText('Unstaged · 1')).toBeVisible()
      expect(screen.getByText('Worktree · 2')).toBeVisible()
    },
    10_000,
  )

  it(
    'wires route upsert controls through App without regressing operator approval state',
    async () => {
      const setup = createAdapter(
        {
          projects: [makeProjectSummary('project-1', 'api-gateway')],
        },
        {
          snapshot: makeSnapshot({
            approvalRequests: [
              {
                actionId: 'action-1',
                sessionId: 'session-1',
                flowId: 'flow-1',
                actionType: 'review_worktree',
                title: 'Review worktree changes',
                detail: 'Inspect the pending repository diff before continuing.',
                status: 'pending',
                decisionNote: null,
                createdAt: '2026-04-13T20:01:00Z',
                updatedAt: '2026-04-13T20:01:00Z',
                resolvedAt: null,
              },
            ],
          }),
          runtimeSession: createRuntimeSession('project-1', {
            phase: 'authenticated',
            sessionId: 'session-1',
            accountId: 'acct-1',
            flowId: 'flow-1',
            lastErrorCode: null,
            lastError: null,
          }),
          runtimeRun: createRuntimeRun('project-1', { runId: 'run-project-1' }),
          notificationRoutes: [
            {
              projectId: 'project-1',
              routeId: 'telegram-primary',
              routeKind: 'telegram',
              routeTarget: '@ops-room',
              enabled: true,
              metadataJson: null,
              createdAt: '2026-04-16T12:59:00Z',
              updatedAt: '2026-04-16T12:59:00Z',
            },
            {
              projectId: 'project-1',
              routeId: 'discord-fallback',
              routeKind: 'discord',
              routeTarget: '1234567890',
              enabled: true,
              metadataJson: null,
              createdAt: '2026-04-16T12:59:00Z',
              updatedAt: '2026-04-16T12:59:00Z',
            },
          ],
        },
      )

      render(<CadenceApp adapter={setup.adapter} />)

      await waitFor(
        () => expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
        { timeout: 10_000 },
      )

      fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

      expect(await screen.findByRole('heading', { name: 'Configure Telegram and Discord delivery paths' })).toBeVisible()
      expect(screen.getByText('Waiting for operator input before this action can resume the run.')).toBeVisible()

      fireEvent.click(screen.getByRole('button', { name: 'Disable route telegram-primary' }))

      expect(await screen.findByRole('button', { name: 'Enable route telegram-primary' })).toBeVisible()
      expect(screen.getByText('Waiting for operator input before this action can resume the run.')).toBeVisible()
    },
    10_000,
  )

  it('renders an honest empty state when no backend projects are loaded', async () => {
    render(<CadenceApp adapter={createAdapter({ projects: [] }).adapter} />)

    expect(await screen.findByRole('heading', { name: 'No projects imported' })).toBeVisible()
    expect(
      screen.getByText('The Vite/Tauri shell is running, but there is no backend project state loaded yet.'),
    ).toBeVisible()
  })
})
