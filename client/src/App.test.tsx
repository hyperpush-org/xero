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
  ListProjectFilesResponseDto,
  ListProjectsResponseDto,
  ProjectSnapshotResponseDto,
  ProjectUpdatedPayloadDto,
  ProviderProfilesDto,
  RepositoryDiffResponseDto,
  RepositoryStatusChangedPayloadDto,
  RepositoryStatusResponseDto,
  RuntimeRunDto,
  AutonomousRunStateDto,
  RuntimeRunUpdatedPayloadDto,
  RuntimeSessionDto,
  RuntimeSettingsDto,
  RuntimeStreamEventDto,
  RuntimeUpdatedPayloadDto,
  SubscribeRuntimeStreamResponseDto,
  SyncNotificationAdaptersResponseDto,
  UpsertNotificationRouteRequestDto,
  UpsertRuntimeSettingsRequestDto,
  UpsertWorkflowGraphResponseDto,
  ApplyWorkflowTransitionResponseDto,
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

function makeSnapshot(projectId = 'project-1', name = 'Cadence'): ProjectSnapshotResponseDto {
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

function makeStatus(projectId = 'project-1', name = 'Cadence'): RepositoryStatusResponseDto {
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
      rootPath: '/tmp/Cadence',
      displayName: 'Cadence',
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

function makeProjectFiles(projectId = 'project-1'): ListProjectFilesResponseDto {
  return {
    projectId,
    root: {
      name: 'root',
      path: '/',
      type: 'folder',
      children: [
        {
          name: 'README.md',
          path: '/README.md',
          type: 'file',
        },
        {
          name: 'src',
          path: '/src',
          type: 'folder',
          children: [
            {
              name: 'App.tsx',
              path: '/src/App.tsx',
              type: 'file',
            },
          ],
        },
      ],
    },
  }
}

function makeRuntimeSession(projectId = 'project-1', overrides: Partial<RuntimeSessionDto> = {}): RuntimeSessionDto {
  return {
    projectId,
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: 'session-1',
    accountId: 'acct-1',
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

function makeRuntimeSettings(overrides: Partial<RuntimeSettingsDto> = {}): RuntimeSettingsDto {
  return {
    providerId: 'openai_codex',
    modelId: 'openai_codex',
    openrouterApiKeyConfigured: false,
    ...overrides,
  }
}

function makeProviderProfilesFromRuntimeSettings(runtimeSettings: RuntimeSettingsDto): ProviderProfilesDto {
  const activeProfileId = runtimeSettings.providerId === 'openrouter' ? 'openrouter-default' : 'openai_codex-default'

  return {
    activeProfileId,
    profiles: [
      {
        profileId: activeProfileId,
        providerId: runtimeSettings.providerId,
        label: runtimeSettings.providerId === 'openrouter' ? 'OpenRouter' : 'OpenAI Codex',
        modelId: runtimeSettings.modelId,
        active: true,
        readiness:
          runtimeSettings.providerId === 'openrouter'
            ? {
                ready: runtimeSettings.openrouterApiKeyConfigured,
                status: runtimeSettings.openrouterApiKeyConfigured ? 'ready' : 'missing',
                credentialUpdatedAt: runtimeSettings.openrouterApiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
              }
            : {
                ready: false,
                status: 'missing',
                credentialUpdatedAt: null,
              },
        migratedFromLegacy: false,
        migratedAt: null,
      },
    ],
    migration: null,
  }
}

function makeRuntimeRun(projectId = 'project-1', overrides: Partial<RuntimeRunDto> = {}): RuntimeRunDto {
  const runtimeRun: RuntimeRunDto = {
    projectId,
    runId: 'run-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
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

  return runtimeRun
}

function makeAutonomousRunState(projectId = 'project-1', runId = 'auto-run-1'): AutonomousRunStateDto {
  return {
    run: {
      projectId,
      runId,
      runtimeKind: 'openai_codex',
      providerId: 'openai_codex',
      supervisorKind: 'detached_pty',
      status: 'running',
      recoveryState: 'healthy',
      activeUnitId: `${runId}:checkpoint:1`,
      duplicateStartDetected: false,
      duplicateStartRunId: null,
      duplicateStartReason: null,
      startedAt: '2026-04-16T20:00:00Z',
      lastHeartbeatAt: '2026-04-16T20:00:05Z',
      lastCheckpointAt: '2026-04-16T20:00:06Z',
      pausedAt: null,
      cancelledAt: null,
      completedAt: null,
      crashedAt: null,
      stoppedAt: null,
      pauseReason: null,
      cancelReason: null,
      crashReason: null,
      lastErrorCode: null,
      lastError: null,
      updatedAt: '2026-04-16T20:00:06Z',
    },
    unit: {
      projectId,
      runId,
      unitId: `${runId}:checkpoint:1`,
      sequence: 1,
      kind: 'state',
      status: 'active',
      summary: 'Recovered the current autonomous unit boundary.',
      boundaryId: 'checkpoint:1',
      workflowLinkage: null,
      startedAt: '2026-04-16T20:00:01Z',
      finishedAt: null,
      updatedAt: '2026-04-16T20:00:06Z',
      lastErrorCode: null,
      lastError: null,
    },
  }
}

function createAdapter(options?: {
  projects?: ListProjectsResponseDto['projects']
  snapshot?: ProjectSnapshotResponseDto
  status?: RepositoryStatusResponseDto
  diff?: RepositoryDiffResponseDto
  runtimeSession?: RuntimeSessionDto
  runtimeSettings?: RuntimeSettingsDto
  providerProfiles?: ProviderProfilesDto
  runtimeRun?: RuntimeRunDto | null
  autonomousState?: AutonomousRunStateDto | null
  notificationRoutes?: ListNotificationRoutesResponseDto['routes']
  projectFiles?: ListProjectFilesResponseDto
  pickedRepositoryPath?: string | null
}) {
  let currentSnapshot = options?.snapshot ?? makeSnapshot()
  let currentStatus = options?.status ?? makeStatus()
  let currentDiff = options?.diff ?? makeDiff()
  let currentRuntimeSession = options?.runtimeSession ?? makeRuntimeSession()
  let currentRuntimeSettings = options?.runtimeSettings ?? makeRuntimeSettings()
  let currentProviderProfiles = options?.providerProfiles ?? makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings)
  let currentRuntimeRun = options?.runtimeRun ?? null
  let currentAutonomousState = options?.autonomousState ?? null
  let currentNotificationRoutes = options?.notificationRoutes ?? []
  let currentProjects = options?.projects ?? [makeProjectSummary('project-1', 'Cadence')]
  let currentProjectFiles = options?.projectFiles ?? makeProjectFiles()
  const pickedRepositoryPath = options?.pickedRepositoryPath ?? null
  const currentFileContents: Record<string, string> = {
    '/README.md': '# Cadence\n',
    '/src/App.tsx': 'export default function App() {\n  return <main>Cadence</main>\n}\n',
  }

  const upsertRuntimeSettings = vi.fn(async (request: UpsertRuntimeSettingsRequestDto) => {
    currentRuntimeSettings = {
      providerId: request.providerId,
      modelId: request.modelId,
      openrouterApiKeyConfigured:
        request.providerId === 'openrouter'
          ? request.openrouterApiKey == null
            ? currentRuntimeSettings.openrouterApiKeyConfigured
            : request.openrouterApiKey.trim().length > 0
          : false,
    }
    currentProviderProfiles = makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings)
    return currentRuntimeSettings
  })

  const upsertProviderProfile = vi.fn(async (request: {
    profileId: string
    providerId: 'openai_codex' | 'openrouter'
    label: string
    modelId: string
    openrouterApiKey?: string | null
    activate?: boolean
  }) => {
    currentRuntimeSettings = {
      providerId: request.providerId,
      modelId: request.modelId,
      openrouterApiKeyConfigured:
        request.providerId === 'openrouter'
          ? request.openrouterApiKey == null
            ? currentRuntimeSettings.openrouterApiKeyConfigured
            : request.openrouterApiKey.trim().length > 0
          : currentRuntimeSettings.openrouterApiKeyConfigured,
    }
    currentProviderProfiles = makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings)
    return currentProviderProfiles
  })

  const setActiveProviderProfile = vi.fn(async (_profileId: string) => currentProviderProfiles)

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

  const startAutonomousRun = vi.fn(async () => {
    currentAutonomousState = makeAutonomousRunState('project-1')
    return currentAutonomousState
  })

  const pickRepositoryFolder = vi.fn(async () => pickedRepositoryPath)
  const importRepository = vi.fn(async (_path: string): Promise<ImportRepositoryResponseDto> => {
    const project = makeProjectSummary('project-1', 'Cadence')
    currentProjects = [project]
    return {
      project,
      repository: makeStatus().repository,
    }
  })

  const adapter: CadenceDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder,
    importRepository,
    listProjects: async () => ({ projects: currentProjects }),
    removeProject: async (projectId) => {
      currentProjects = currentProjects.filter((project) => project.id !== projectId)
      return { projects: currentProjects }
    },
    getProjectSnapshot: async () => currentSnapshot,
    getRepositoryStatus: async () => currentStatus,
    getRepositoryDiff: async (_projectId, scope) => ({ ...currentDiff, scope }),
    listProjectFiles: async () => currentProjectFiles,
    readProjectFile: async (projectId, path) => ({
      projectId,
      path,
      content: currentFileContents[path] ?? '',
    }),
    writeProjectFile: async (projectId, path, content) => {
      currentFileContents[path] = content
      return { projectId, path }
    },
    createProjectEntry: async (request) => {
      currentFileContents[request.parentPath === '/' ? `/${request.name}` : `${request.parentPath}/${request.name}`] = ''
      return {
        projectId: request.projectId,
        path: request.parentPath === '/' ? `/${request.name}` : `${request.parentPath}/${request.name}`,
      }
    },
    renameProjectEntry: async (request) => ({
      projectId: request.projectId,
      path: request.path.split('/').slice(0, -1).filter(Boolean).length
        ? `/${request.path.split('/').slice(0, -1).filter(Boolean).join('/')}/${request.newName}`
        : `/${request.newName}`,
    }),
    deleteProjectEntry: async (projectId, path) => ({ projectId, path }),
    getAutonomousRun: async () => currentAutonomousState ?? { run: null, unit: null },
    getRuntimeRun: async () => currentRuntimeRun,
    getRuntimeSettings: async () => currentRuntimeSettings,
    getProviderProfiles: async () => currentProviderProfiles,
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
    startAutonomousRun,
    startRuntimeRun,
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
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
    cancelAutonomousRun: async (_projectId, runId) => {
      currentAutonomousState = {
        run: {
          ...makeAutonomousRunState('project-1', runId).run!,
          status: 'cancelled',
          recoveryState: 'terminal',
          cancelledAt: '2026-04-16T20:10:00Z',
          cancelReason: {
            code: 'operator_cancelled',
            message: 'Operator cancelled the autonomous run from the desktop shell.',
          },
          updatedAt: '2026-04-16T20:10:00Z',
        },
        unit: {
          ...makeAutonomousRunState('project-1', runId).unit!,
          status: 'cancelled',
          finishedAt: '2026-04-16T20:10:00Z',
          updatedAt: '2026-04-16T20:10:00Z',
        },
      }
      return currentAutonomousState
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
    submitNotificationReply: async (request) => ({
      claim: {
        id: 1,
        projectId: request.projectId,
        actionId: request.actionId,
        routeId: request.routeId,
        correlationKey: request.correlationKey,
        responderId: request.responderId ?? null,
        status: request.decision === 'approve' ? 'accepted' : 'rejected',
        rejectionCode: request.decision === 'approve' ? null : 'notification_reply_rejected',
        rejectionMessage: request.decision === 'approve' ? null : 'Operator rejected the notification reply.',
        createdAt: request.receivedAt,
      },
      dispatch: {
        id: 1,
        projectId: request.projectId,
        actionId: request.actionId,
        routeId: request.routeId,
        correlationKey: request.correlationKey,
        status: request.decision === 'approve' ? 'claimed' : 'failed',
        attemptCount: 1,
        lastAttemptAt: request.receivedAt,
        deliveredAt: request.decision === 'approve' ? request.receivedAt : null,
        claimedAt: request.decision === 'approve' ? request.receivedAt : null,
        lastErrorCode: request.decision === 'approve' ? null : 'notification_reply_rejected',
        lastErrorMessage: request.decision === 'approve' ? null : 'Operator rejected the notification reply.',
        createdAt: request.receivedAt,
        updatedAt: request.receivedAt,
      },
      resolveResult: {
        approvalRequest: {
          actionId: request.actionId,
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_worktree',
          title: 'Review worktree changes',
          detail: 'Inspect the pending repository diff before continuing.',
          status: request.decision === 'approve' ? 'approved' : 'rejected',
          decisionNote: null,
          createdAt: request.receivedAt,
          updatedAt: request.receivedAt,
          resolvedAt: request.receivedAt,
        },
        verificationRecord: {
          id: 1,
          sourceActionId: request.actionId,
          status: request.decision === 'approve' ? 'passed' : 'failed',
          summary: request.decision === 'approve' ? 'Approved operator action.' : 'Rejected operator action.',
          detail: null,
          recordedAt: request.receivedAt,
        },
      },
      resumeResult: null,
    }),
    syncNotificationAdapters: async (_projectId): Promise<SyncNotificationAdaptersResponseDto> => ({
      projectId: 'project-1',
      dispatch: {
        projectId: 'project-1',
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
        projectId: 'project-1',
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
      syncedAt: '2026-04-16T13:00:00Z',
    }),
    upsertWorkflowGraph: async (request): Promise<UpsertWorkflowGraphResponseDto> => ({
      nodes: request.nodes,
      edges: request.edges,
      gates: request.gates,
      phases: [],
    }),
    applyWorkflowTransition: async (request): Promise<ApplyWorkflowTransitionResponseDto> => ({
      transitionEvent: {
        id: 1,
        transitionId: request.transitionId,
        causalTransitionId: request.causalTransitionId ?? null,
        fromNodeId: request.fromNodeId,
        toNodeId: request.toNodeId,
        transitionKind: request.transitionKind,
        gateDecision: request.gateDecision,
        gateDecisionContext: request.gateDecisionContext ?? null,
        createdAt: request.occurredAt,
      },
      automaticDispatch: undefined,
      phases: [],
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

  return { adapter, upsertNotificationRoute, upsertRuntimeSettings, upsertProviderProfile, importRepository, pickRepositoryFolder, startRuntimeRun, startAutonomousRun }
}

describe('CadenceApp current UI', () => {
  it('shows the onboarding flow on a cold-start empty state', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    expect(await screen.findByRole('heading', { name: /Welcome to Cadence/i })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Get started' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Skip setup' })).toBeVisible()
  })

  it('falls through to the legacy empty state when onboarding is dismissed', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Skip setup' }))

    expect(await screen.findByRole('heading', { name: 'Add your first project' })).toBeVisible()
    expect(screen.getAllByRole('button', { name: /Import repository/ }).length).toBeGreaterThanOrEqual(1)
  })

  it('reflects real provider settings in onboarding and disables unsupported providers', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))

    expect(await screen.findByRole('heading', { name: 'Configure providers' })).toBeVisible()
    expect(screen.getByText('Provider setup is app-wide. Choose the active profile for new runtime binds without rewriting project runtime history.')).toBeVisible()
    expect(screen.getByText('Active profile')).toBeVisible()
    expect(screen.getByText('Using this')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Set up' })).toBeVisible()
    expect(screen.getAllByText('Unavailable')).toHaveLength(2)
  })

  it('keeps onboarding provider review truthful before OpenAI is connected', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))

    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
    expect(screen.getByText('OpenAI Codex · active profile')).toBeVisible()
  })

  it('saves OpenRouter provider settings from onboarding', async () => {
    const { adapter, upsertProviderProfile } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Set up' }))
    fireEvent.change(screen.getByLabelText('Model ID'), { target: { value: 'openai/gpt-4.1-mini' } })
    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-or-v1-test-secret' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(upsertProviderProfile).toHaveBeenCalledTimes(1))
    expect(upsertProviderProfile).toHaveBeenCalledWith({
      profileId: 'openrouter-default',
      providerId: 'openrouter',
      label: 'OpenRouter',
      modelId: 'openai/gpt-4.1-mini',
      openrouterApiKey: 'sk-or-v1-test-secret',
      activate: true,
    })
  })

  it('imports a project and creates a notification route from onboarding', async () => {
    const { adapter, pickRepositoryFolder, importRepository, upsertNotificationRoute } = createAdapter({
      projects: [],
      pickedRepositoryPath: '/tmp/Cadence',
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByRole('heading', { name: 'Add a project' })).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: /Choose a folder/i }))

    await waitFor(() => expect(pickRepositoryFolder).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(importRepository).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(screen.getByText('/tmp/Cadence')).toBeVisible())

    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click((await screen.findAllByRole('button', { name: 'Add route' }))[0])
    fireEvent.change(screen.getByPlaceholderText('Chat ID or @channel'), { target: { value: '@ops-room' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save route' }))

    await waitFor(() => expect(upsertNotificationRoute).toHaveBeenCalledTimes(1))
    expect(upsertNotificationRoute).toHaveBeenCalledWith(
      expect.objectContaining({
        routeId: 'telegram-primary',
        routeKind: 'telegram',
        routeTarget: 'telegram:@ops-room',
        enabled: true,
      }),
    )
  })

  it('renders the current workflow empty state for an imported project', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.getByText('No milestone assigned')).toBeVisible()
  })

  it('collapses the project rail into a compact icon strip from the titlebar toggle', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    const collapseButton = screen.getByRole('button', { name: 'Collapse project sidebar' })
    fireEvent.click(collapseButton)

    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()
    expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull()
    expect(screen.queryByRole('button', { name: 'Project actions for cadence' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /cadence/i })).toBeVisible()
  })

  it('auto-collapses the project rail in Editor and restores it when leaving if it started expanded', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(document.querySelector('aside[data-collapsed="false"]')).not.toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))

    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())
    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Workflow' }))

    await waitFor(() => expect(document.querySelector('aside[data-collapsed="false"]')).not.toBeNull())
    expect(screen.getByRole('button', { name: 'Collapse project sidebar' })).toBeVisible()
  })

  it('keeps the project rail collapsed after leaving Editor when it was already collapsed', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Collapse project sidebar' }))
    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))
    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())
    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()
  })

  it('switches to Agent without rendering the removed debug panels', async () => {
    const { adapter } = createAdapter({ runtimeRun: null, autonomousState: null })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(await screen.findByRole('heading', { name: 'No supervised run attached yet' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Start run' })).toBeVisible()
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start autonomous run' })).not.toBeInTheDocument()
  })

  it('rehydrates the recovered runtime snapshot after reload without rendering the removed debug panels', async () => {
    const recoveredAutonomousState = makeAutonomousRunState('project-1', 'auto-run-1')
    recoveredAutonomousState.run = {
      ...recoveredAutonomousState.run!,
      recoveryState: 'recovery_required',
      activeUnitId: 'auto-run-1:checkpoint:2',
      duplicateStartDetected: true,
      duplicateStartRunId: 'auto-run-1',
      duplicateStartReason:
        'Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor.',
      crashedAt: '2026-04-16T20:03:00Z',
      crashReason: {
        code: 'runtime_supervisor_connect_failed',
        message: 'Cadence restored the same autonomous run after reload without launching a duplicate continuation.',
      },
      lastErrorCode: 'runtime_supervisor_connect_failed',
      lastError: {
        code: 'runtime_supervisor_connect_failed',
        message: 'Cadence restored the same autonomous run after reload without launching a duplicate continuation.',
      },
      updatedAt: '2026-04-16T20:03:00Z',
    }
    recoveredAutonomousState.unit = {
      ...recoveredAutonomousState.unit!,
      unitId: 'auto-run-1:checkpoint:2',
      sequence: 2,
      summary: 'Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.',
      boundaryId: 'checkpoint:2',
      updatedAt: '2026-04-16T20:03:00Z',
    }

    const { adapter } = createAdapter({
      snapshot: {
        ...makeSnapshot(),
        autonomousRun: recoveredAutonomousState.run,
        autonomousUnit: recoveredAutonomousState.unit,
      },
      runtimeRun: makeRuntimeRun('project-1', {
        status: 'stale',
        transport: {
          kind: 'tcp',
          endpoint: '127.0.0.1:4455',
          liveness: 'unreachable',
        },
        lastCheckpointSequence: 2,
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
        lastErrorCode: 'runtime_supervisor_connect_failed',
        lastError: {
          code: 'runtime_supervisor_connect_failed',
          message: 'Cadence restored the same autonomous run after reload without launching a duplicate continuation.',
        },
      }),
      autonomousState: recoveredAutonomousState,
    })

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(await screen.findByRole('heading', { name: 'Recovered run snapshot' })).toBeVisible()
    expect(
      screen.queryByText('Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.'),
    ).not.toBeInTheDocument()
    expect(screen.queryByText('Duplicate start prevented')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start autonomous run' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Inspect truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
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
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }))
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

  it('switches to Editor and loads the selected project files', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))
    expect(await screen.findByText('README.md')).toBeVisible()
    expect(screen.getByText('Explorer')).toBeVisible()
    expect(screen.getByLabelText('Search files')).toBeVisible()
    expect(screen.getByText('Select a file to start editing')).toBeVisible()
    expect(screen.queryByText('No execution activity yet')).not.toBeInTheDocument()
  })
})
