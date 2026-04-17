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

import { CadenceApp } from './App'
import { type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import type {
  ImportRepositoryResponseDto,
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
  SyncNotificationAdaptersResponseDto,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/cadence-model'

function makeProjectSummary(id: string, name: string): ListProjectsResponseDto['projects'][number] {
  return {
    id,
    name,
    description: `${name} description`,
    milestone: 'M001',
    totalPhases: 0,
    completedPhases: 0,
    activePhase: 0,
    branch: null,
    runtime: null,
  }
}

function makeSnapshot(projectId = 'project-1', name = 'cadence'): ProjectSnapshotResponseDto {
  return {
    project: makeProjectSummary(projectId, name),
    repository: {
      id: `repo-${projectId}`,
      projectId,
      rootPath: `/tmp/${name}`,
      displayName: name,
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    phases: [],
    lifecycle: { stages: [] },
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    handoffPackages: [],
    notificationDispatches: [],
    notificationReplyClaims: [],
  }
}

function makeStatus(projectId = 'project-1', name = 'cadence'): RepositoryStatusResponseDto {
  return {
    repository: {
      id: `repo-${projectId}`,
      projectId,
      rootPath: `/tmp/${name}`,
      displayName: name,
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    branch: null,
    entries: [],
    hasStagedChanges: false,
    hasUnstagedChanges: false,
    hasUntrackedChanges: false,
  }
}

function makeDiff(projectId = 'project-1', scope: RepositoryDiffResponseDto['scope'] = 'unstaged'): RepositoryDiffResponseDto {
  return {
    repository: {
      id: `repo-${projectId}`,
      projectId,
      rootPath: '/tmp/cadence',
      displayName: 'cadence',
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    scope,
    patch: '',
    truncated: false,
    baseRevision: null,
  }
}

function makeRuntimeSession(projectId = 'project-1', overrides: Partial<RuntimeSessionDto> = {}): RuntimeSessionDto {
  return {
    projectId,
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: null,
    accountId: null,
    phase: 'authenticated',
    callbackBound: true,
    authorizationUrl: 'https://auth.openai.com/oauth/authorize?client_id=test',
    redirectUri: 'http://127.0.0.1:1455/auth/callback',
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:00:00Z',
    ...overrides,
  }
}

function makeRuntimeRun(projectId = 'project-1', overrides: Partial<RuntimeRunDto> = {}): RuntimeRunDto {
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
        summary: 'Supervisor boot recorded.',
        createdAt: '2026-04-15T20:00:01Z',
      },
    ],
    ...overrides,
  }
}

function createAdapter(options?: {
  projects?: ListProjectsResponseDto['projects']
  snapshot?: ProjectSnapshotResponseDto
  status?: RepositoryStatusResponseDto
  diff?: RepositoryDiffResponseDto
  runtimeSession?: RuntimeSessionDto
  runtimeRun?: RuntimeRunDto | null
  notificationRoutes?: ListNotificationRoutesResponseDto['routes']
}) {
  let currentSnapshot = options?.snapshot ?? makeSnapshot()
  let currentStatus = options?.status ?? makeStatus()
  let currentDiff = options?.diff ?? makeDiff()
  let currentRuntimeSession = options?.runtimeSession ?? makeRuntimeSession()
  let currentRuntimeRun = options?.runtimeRun ?? null
  let currentNotificationRoutes = options?.notificationRoutes ?? []

  const upsertNotificationRoute = vi.fn(async (request: UpsertNotificationRouteRequestDto) => {
    const route = {
      projectId: request.projectId,
      routeId: request.routeId,
      routeKind: request.routeKind,
      routeTarget: request.routeTarget,
      enabled: request.enabled,
      metadataJson: request.metadataJson ?? null,
      credentialReadiness: null,
      createdAt: '2026-04-16T12:59:00Z',
      updatedAt: request.updatedAt,
    }

    currentNotificationRoutes = [
      ...currentNotificationRoutes.filter((item) => item.routeId !== route.routeId),
      route,
    ]

    return { route }
  })

  const startRuntimeRun = vi.fn(async () => {
    currentRuntimeRun = makeRuntimeRun('project-1')
    return currentRuntimeRun
  })

  const adapter: CadenceDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder: async () => null,
    importRepository: async (_path: string): Promise<ImportRepositoryResponseDto> => ({
      project: makeProjectSummary('project-1', 'cadence'),
      repository: makeStatus().repository,
    }),
    listProjects: async () => ({ projects: options?.projects ?? [makeProjectSummary('project-1', 'cadence')] }),
    getProjectSnapshot: async () => currentSnapshot,
    getRepositoryStatus: async () => currentStatus,
    getRepositoryDiff: async (_projectId, scope) => ({ ...currentDiff, scope }),
    getRuntimeRun: async () => currentRuntimeRun,
    getRuntimeSession: async () => currentRuntimeSession,
    startOpenAiLogin: async () => {
      currentRuntimeSession = makeRuntimeSession('project-1', {
        phase: 'awaiting_browser_callback',
        flowId: 'flow-1',
      })
      return currentRuntimeSession
    },
    submitOpenAiCallback: async () => {
      currentRuntimeSession = makeRuntimeSession('project-1')
      return currentRuntimeSession
    },
    startRuntimeRun,
    startRuntimeSession: async () => {
      currentRuntimeSession = makeRuntimeSession('project-1')
      return currentRuntimeSession
    },
    stopRuntimeRun: async (_projectId, runId) => {
      currentRuntimeRun = makeRuntimeRun('project-1', {
        runId,
        status: 'stopped',
        stoppedAt: '2026-04-15T20:10:00Z',
      })
      return currentRuntimeRun
    },
    logoutRuntimeSession: async () => {
      currentRuntimeSession = makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      })
      return currentRuntimeSession
    },
    resolveOperatorAction: async () => {
      throw new Error('not used')
    },
    resumeOperatorRun: async () => {
      throw new Error('not used')
    },
    listNotificationRoutes: async () => ({ routes: currentNotificationRoutes }),
    listNotificationDispatches: async (): Promise<ListNotificationDispatchesResponseDto> => ({ dispatches: [] }),
    upsertNotificationRoute,
    upsertNotificationRouteCredentials: async (request) => ({
      projectId: request.projectId,
      routeId: request.routeId,
      routeKind: request.routeKind,
      credentialScope: 'app_local',
      hasBotToken: Boolean(request.credentials.botToken),
      hasChatId: Boolean(request.credentials.chatId),
      hasWebhookUrl: Boolean(request.credentials.webhookUrl),
      updatedAt: request.updatedAt,
    }),
    recordNotificationDispatchOutcome: async (request) => ({ dispatch: request as never }),
    submitNotificationReply: async (request) => ({ claim: request as never }),
    syncNotificationAdapters: async (_projectId): Promise<SyncNotificationAdaptersResponseDto> => ({
      projectId: 'project-1',
      dispatch: {
        attemptedCount: 0,
        sentCount: 0,
        failedCount: 0,
        cycleCompletedAt: '2026-04-16T13:00:00Z',
        errorCounts: [],
        attempts: [],
      },
      replies: {
        attemptedCount: 0,
        acceptedCount: 0,
        rejectedCount: 0,
        cycleCompletedAt: '2026-04-16T13:00:00Z',
        errorCounts: [],
        attempts: [],
      },
    }),
    upsertWorkflowGraph: async (request) => ({ graph: request.graph, updatedAt: request.updatedAt }),
    applyWorkflowTransition: async (request) => ({
      event: {
        id: request.transitionId,
        projectId: request.projectId,
        fromNodeId: request.fromNodeId,
        toNodeId: request.toNodeId,
        transitionKind: request.transitionKind,
        trigger: request.trigger,
        summary: null,
        createdAt: request.createdAt,
      },
      graph: request.graph,
      automaticDispatch: null,
    }),
    subscribeRuntimeStream: async (
      projectId: string,
      itemKinds: RuntimeStreamEventDto['subscribedItemKinds'],
      _handler: (payload: RuntimeStreamEventDto) => void,
    ) => ({
      response: {
        projectId,
        runtimeKind: 'openai_codex',
        runId: currentRuntimeRun?.runId ?? 'run-1',
        sessionId: currentRuntimeSession.sessionId ?? 'session-1',
        flowId: currentRuntimeSession.flowId ?? null,
        subscribedItemKinds: itemKinds,
      } satisfies SubscribeRuntimeStreamResponseDto,
      unsubscribe: () => {},
    }),
    onProjectUpdated: async (_handler: (payload: ProjectUpdatedPayloadDto) => void) => () => {},
    onRepositoryStatusChanged: async (_handler: (payload: RepositoryStatusChangedPayloadDto) => void) => () => {},
    onRuntimeUpdated: async (_handler: (payload: RuntimeUpdatedPayloadDto) => void) => () => {},
    onRuntimeRunUpdated: async (_handler: (payload: RuntimeRunUpdatedPayloadDto) => void) => () => {},
  }

  return { adapter, upsertNotificationRoute, startRuntimeRun }
}

describe('CadenceApp current UI', () => {
  it('renders the no-projects empty state', async () => {
    const { adapter } = createAdapter({ projects: [] })

    render(<CadenceApp adapter={adapter} />)

    expect(await screen.findByText('No projects imported')).toBeVisible()
    expect(screen.getAllByRole('button', { name: 'Import repository' }).length).toBeGreaterThanOrEqual(1)
  })

  it('renders the current workflow empty state for an imported project', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.getByText('No milestone assigned')).toBeVisible()
  })

  it('switches to Agent and starts a supervised run with the current shell controls', async () => {
    const { adapter, startRuntimeRun } = createAdapter({ runtimeRun: null })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Start run' }))

    await waitFor(() => expect(startRuntimeRun).toHaveBeenCalledTimes(1))
  })

  it('opens Settings and runs the current provider and notification flows', async () => {
    const { adapter, upsertNotificationRoute } = createAdapter({
      runtimeRun: null,
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
        lastErrorCode: 'auth_session_not_found',
        lastError: {
          code: 'auth_session_not_found',
          message: 'Sign in with OpenAI to create a runtime session for this project.',
          retryable: false,
        },
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByLabelText('Settings'))
    expect(await screen.findByRole('heading', { name: 'Providers' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Connect' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Connect' }))
    await waitFor(() => expect(openUrlMock).toHaveBeenCalledTimes(1))

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))
    expect(await screen.findByText('Telegram')).toBeVisible()

    fireEvent.click(screen.getAllByRole('button', { name: 'Add route' })[0])
    fireEvent.change(screen.getByLabelText('Route name'), { target: { value: 'ops-alerts' } })
    fireEvent.change(screen.getByLabelText('Target'), { target: { value: '@ops-room' } })
    fireEvent.click(screen.getByRole('button', { name: 'Create route' }))

    await waitFor(() => expect(upsertNotificationRoute).toHaveBeenCalledTimes(1))
    expect(upsertNotificationRoute.mock.calls[0][0]).toMatchObject({
      routeId: 'ops-alerts',
      routeKind: 'telegram',
      routeTarget: 'telegram:@ops-room',
      enabled: true,
    })
  })

  it('switches to Execution and shows the current empty state', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Execution' }))
    expect(await screen.findByText('No execution activity yet')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Changes' }))
    expect(await screen.findByRole('button', { name: 'Unstaged' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Verify' }))
    expect(screen.getByText('No verification activity yet')).toBeVisible()
  })
})
