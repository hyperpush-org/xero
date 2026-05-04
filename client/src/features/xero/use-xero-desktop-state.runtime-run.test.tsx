import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { XeroDesktopError, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import type {
  ImportRepositoryResponseDto,
  ListNotificationDispatchesResponseDto,
  ListNotificationRoutesResponseDto,
  ListProjectsResponseDto,
  ProjectSnapshotResponseDto,
  ProjectUpdatedPayloadDto,
  ProviderAuthSessionDto,
  ProviderModelCatalogDto,
  RepositoryDiffResponseDto,
  RepositoryStatusResponseDto,
  ResumeOperatorRunResponseDto,
  AutonomousRunStateDto,
  RuntimeRunControlInputDto,
  RuntimeRunDto,
  RuntimeRunUpdatedPayloadDto,
  RuntimeSessionDto,
  RuntimeStreamEventDto,
  SubscribeRuntimeStreamResponseDto,
  SyncNotificationAdaptersResponseDto,
  UpsertNotificationRouteCredentialsRequestDto,
  UpsertNotificationRouteCredentialsResponseDto,
} from '@/src/lib/xero-model'
import type { ProviderProfilesDto } from '@/src/test/legacy-provider-profiles'
import { useXeroDesktopState } from '@/src/features/xero/use-xero-desktop-state'

function makeProjectSummary(id: string, name: string) {
  return {
    id,
    name,
    description: `${name} description`,
    milestone: `M-${id}`,
    totalPhases: 1,
    completedPhases: 0,
    activePhase: 1,
    branch: 'main',
    runtime: null,
  }
}

function makeAgentSession(projectId: string) {
  return {
    projectId,
    agentSessionId: 'agent-session-main',
    title: 'Main session',
    summary: 'Primary project session',
    status: 'active' as const,
    selected: true,
    createdAt: '2026-04-15T20:00:00Z',
    updatedAt: '2026-04-15T20:00:00Z',
    archivedAt: null,
    lastRunId: null,
    lastRuntimeKind: null,
    lastProviderId: null,
  }
}

function makeSnapshot(id: string, name: string): ProjectSnapshotResponseDto {
  return {
    project: makeProjectSummary(id, name),
    repository: {
      id: `repo-${id}`,
      projectId: id,
      rootPath: `/tmp/${name}`,
      displayName: name,
      branch: 'main',
      headSha: 'abc1234',
      isGitRepo: true,
    },
    phases: [
      {
        id: 1,
        name: 'Runtime recovery',
        description: 'Recover durable runtime state',
        status: 'active',
        currentStep: 'execute',
        taskCount: 2,
        completedTasks: 1,
        summary: null,
      },
    ],
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    agentSessions: [makeAgentSession(id)],
  }
}

function makeNotificationDispatch(options: {
  id: number
  projectId: string
  actionId: string
  routeId: string
  status: 'pending' | 'sent' | 'failed' | 'claimed'
  attemptCount?: number
  lastAttemptAt?: string | null
  deliveredAt?: string | null
  claimedAt?: string | null
  lastErrorCode?: string | null
  lastErrorMessage?: string | null
  updatedAt?: string
}) {
  return {
    id: options.id,
    projectId: options.projectId,
    actionId: options.actionId,
    routeId: options.routeId,
    correlationKey: `nfy:${String(options.id).padStart(32, 'a')}`,
    status: options.status,
    attemptCount: options.attemptCount ?? 0,
    lastAttemptAt: options.lastAttemptAt ?? null,
    deliveredAt: options.deliveredAt ?? null,
    claimedAt: options.claimedAt ?? null,
    lastErrorCode: options.lastErrorCode ?? null,
    lastErrorMessage: options.lastErrorMessage ?? null,
    createdAt: '2026-04-16T13:00:00Z',
    updatedAt: options.updatedAt ?? '2026-04-16T13:00:00Z',
  }
}

function makeNotificationRoute(options: {
  projectId: string
  routeId: string
  routeKind: 'telegram' | 'discord'
  routeTarget: string
  enabled?: boolean
  metadataJson?: string | null
  credentialReadiness?: {
    hasBotToken: boolean
    hasChatId: boolean
    hasWebhookUrl: boolean
    ready: boolean
    status: 'ready' | 'missing' | 'malformed' | 'unavailable'
    diagnostic?: {
      code: string
      message: string
      retryable: boolean
    } | null
  } | null
  createdAt?: string
  updatedAt?: string
}) {
  const routeKind = options.routeKind

  return {
    projectId: options.projectId,
    routeId: options.routeId,
    routeKind,
    routeTarget: options.routeTarget,
    enabled: options.enabled ?? true,
    metadataJson: options.metadataJson ?? null,
    credentialReadiness:
      options.credentialReadiness ??
      (routeKind === 'telegram'
        ? {
            hasBotToken: true,
            hasChatId: true,
            hasWebhookUrl: false,
            ready: true,
            status: 'ready',
            diagnostic: null,
          }
        : {
            hasBotToken: true,
            hasChatId: false,
            hasWebhookUrl: true,
            ready: true,
            status: 'ready',
            diagnostic: null,
          }),
    createdAt: options.createdAt ?? '2026-04-16T12:59:00Z',
    updatedAt: options.updatedAt ?? '2026-04-16T12:59:00Z',
  }
}

function makeGateLinkedPendingApproval(actionId = 'scope:auto-dispatch:workflow-research:requires_user_input') {
  return {
    actionId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    actionType: 'review_worktree',
    title: 'Review worktree changes',
    detail: 'Inspect the pending repository diff before continuing.',
    userAnswer: null,
    status: 'pending' as const,
    decisionNote: null,
    createdAt: '2026-04-16T13:00:00Z',
    updatedAt: '2026-04-16T13:00:00Z',
    resolvedAt: null,
  }
}

function makeGateLinkedApprovedApproval(actionId = 'scope:auto-dispatch:workflow-research:requires_user_input') {
  return {
    actionId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    actionType: 'review_worktree',
    title: 'Review worktree changes',
    detail: 'Inspect the pending repository diff before continuing.',
    userAnswer: 'Proceed after validating repo changes.',
    status: 'approved' as const,
    decisionNote: 'Looks good to resume.',
    createdAt: '2026-04-16T13:00:00Z',
    updatedAt: '2026-04-16T13:02:00Z',
    resolvedAt: '2026-04-16T13:02:00Z',
  }
}

function makeResumeHistoryEntry(options: {
  id: number
  actionId: string
  status: 'started' | 'failed'
  summary: string
  createdAt?: string
}) {
  return {
    id: options.id,
    sourceActionId: options.actionId,
    sessionId: 'session-1',
    status: options.status,
    summary: options.summary,
    createdAt: options.createdAt ?? '2026-04-16T13:03:00Z',
  }
}

function makeStatus(id: string): RepositoryStatusResponseDto {
  return {
    repository: {
      id: `repo-${id}`,
      projectId: id,
      rootPath: `/tmp/${id}`,
      displayName: id,
      branch: 'main',
      headSha: 'abc1234',
      isGitRepo: true,
    },
    branch: {
      name: 'main',
      headSha: 'abc1234',
      detached: false,
    },
    entries: [],
    hasStagedChanges: false,
    hasUnstagedChanges: false,
    hasUntrackedChanges: false,
  }
}

function makeDiff(id: string, scope: 'staged' | 'unstaged' | 'worktree'): RepositoryDiffResponseDto {
  return {
    repository: {
      id: `repo-${id}`,
      projectId: id,
      rootPath: `/tmp/${id}`,
      displayName: id,
      branch: 'main',
      headSha: 'abc1234',
      isGitRepo: true,
    },
    scope,
    patch: '',
    truncated: false,
    baseRevision: null,
  }
}

function makeRuntimeSession(projectId: string, overrides: Partial<RuntimeSessionDto> = {}): RuntimeSessionDto {
  return {
    projectId,
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: null,
    accountId: null,
    phase: 'idle',
    callbackBound: null,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: 'auth_session_not_found',
    lastError: {
      code: 'auth_session_not_found',
      message: 'Sign in with OpenAI to create a runtime session for this project.',
      retryable: false,
    },
    updatedAt: '2026-04-15T20:00:10Z',
    ...overrides,
  }
}

function makeProviderAuthSession(overrides: Partial<ProviderAuthSessionDto> = {}): ProviderAuthSessionDto {
  return {
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: 'flow-1',
    sessionId: 'session-1',
    accountId: 'acct-1',
    phase: 'authenticated',
    callbackBound: true,
    authorizationUrl: 'https://auth.openai.com/oauth/authorize',
    redirectUri: 'http://127.0.0.1:1455/auth/callback',
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-15T20:00:10Z',
    ...overrides,
  }
}

function makeRuntimeRun(projectId: string, overrides: Partial<RuntimeRunDto> = {}): RuntimeRunDto {
  return {
    projectId,
    agentSessionId: 'agent-session-main',
    runId: `run-${projectId}`,
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    supervisorKind: 'owned_agent',
    status: 'running',
    transport: {
      kind: 'internal',
      endpoint: 'xero://owned-agent',
      liveness: 'reachable',
    },
    controls: {
      active: {
        providerProfileId: 'openai_codex-default',
        runtimeAgentId: 'ask',
        modelId: 'openai_codex',
        thinkingEffort: 'medium',
        approvalMode: 'suggest',
        planModeRequired: false,
        revision: 1,
        appliedAt: '2026-04-15T20:00:00Z',
      },
      pending: null,
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
        summary: 'Owned agent runtime started.',
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

function makeProviderProfiles(overrides: Partial<ProviderProfilesDto> = {}): ProviderProfilesDto {
  const activeProfileId = overrides.activeProfileId ?? 'openai_codex-default'
  const profiles = overrides.profiles ?? [
    {
      profileId: 'openai_codex-default',
      providerId: 'openai_codex',
      runtimeKind: 'openai_codex',
      label: 'OpenAI Codex',
      modelId: 'openai_codex',
      active: activeProfileId === 'openai_codex-default',
      readiness: {
        ready: false,
        status: 'missing',
        proofUpdatedAt: null,
      },
      migratedFromLegacy: false,
      migratedAt: null,
    },
  ]

  return {
    activeProfileId,
    profiles,
    migration: overrides.migration ?? null,
  }
}

function makeAutonomousRunState(
  projectId: string,
  overrides: Partial<NonNullable<AutonomousRunStateDto['run']>> = {},
): AutonomousRunStateDto {
  const runId = overrides.runId ?? `auto-${projectId}`

  return {
    run: {
      projectId,
      agentSessionId: 'agent-session-main',
      runId,
      runtimeKind: 'openai_codex',
      providerId: 'openai_codex',
      supervisorKind: 'owned_agent',
      status: 'running',
      recoveryState: 'healthy',
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
      ...overrides,
    },
  }
}

function makeBlockedAutonomousRunState(projectId: string): AutonomousRunStateDto {
  const state = makeAutonomousRunState(projectId, {
    runId: `auto-${projectId}`,
    status: 'paused',
    recoveryState: 'recovery_required',
    pausedAt: '2026-04-16T20:03:00Z',
    pauseReason: {
      code: 'operator_pause',
      message: 'Operator paused the autonomous run for review.',
    },
    updatedAt: '2026-04-16T20:03:00Z',
  })

  return state
}

function makeRecoveredAutonomousRunState(projectId: string): AutonomousRunStateDto {
  const state = makeAutonomousRunState(projectId, {
    runId: `auto-${projectId}`,
    status: 'running',
    recoveryState: 'healthy',
    pausedAt: null,
    pauseReason: null,
    updatedAt: '2026-04-16T20:04:00Z',
  })

  return state
}

function makeStreamResponse(
  projectId: string,
  overrides: Partial<SubscribeRuntimeStreamResponseDto> = {},
): SubscribeRuntimeStreamResponseDto {
  return {
    projectId,
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId: `run-${projectId}`,
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    ...overrides,
  }
}

function makeNotificationSyncResponse(
  projectId: string,
  overrides: Partial<SyncNotificationAdaptersResponseDto> = {},
): SyncNotificationAdaptersResponseDto {
  return {
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
    ...overrides,
  }
}

function createMockAdapter(options?: {
  listProjects?: ListProjectsResponseDto
  snapshots?: Record<string, ProjectSnapshotResponseDto>
  statuses?: Record<string, RepositoryStatusResponseDto>
  runtimeSessions?: Record<string, RuntimeSessionDto>
  runtimeRuns?: Record<string, RuntimeRunDto | null>
  autonomousStates?: Record<string, AutonomousRunStateDto | null>
  runtimeSessionErrors?: Record<string, Error>
  runtimeRunErrors?: Record<string, Error>
  providerProfiles?: ProviderProfilesDto
  notificationDispatches?: Record<string, ListNotificationDispatchesResponseDto['dispatches']>
  notificationRoutes?: Record<string, ListNotificationRoutesResponseDto['routes']>
  notificationDispatchErrors?: Record<string, Error>
  notificationRouteErrors?: Record<string, Error>
  notificationSyncResponses?: Record<string, SyncNotificationAdaptersResponseDto>
  notificationSyncErrors?: Record<string, Error>
  upsertRouteErrors?: Record<string, Error>
  startRuntimeRunErrors?: Record<string, Error>
  updateRuntimeRunControlErrors?: Record<string, Error>
  subscribeResponses?: Record<string, SubscribeRuntimeStreamResponseDto>
}) {
  let projectUpdatedHandler: ((payload: ProjectUpdatedPayloadDto) => void) | null = null
  let projectUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null

  const snapshots = options?.snapshots ?? {
    'project-1': makeSnapshot('project-1', 'Xero'),
    'project-2': makeSnapshot('project-2', 'orchestra'),
  }
  const statuses = options?.statuses ?? {
    'project-1': makeStatus('project-1'),
    'project-2': makeStatus('project-2'),
  }
  const runtimeSessions = options?.runtimeSessions ?? {
    'project-1': makeRuntimeSession('project-1'),
    'project-2': makeRuntimeSession('project-2'),
  }
  const runtimeRuns = options?.runtimeRuns ?? {
    'project-1': makeRuntimeRun('project-1'),
    'project-2': makeRuntimeRun('project-2', { runId: 'run-project-2' }),
  }
  const autonomousStates = options?.autonomousStates ?? {
    'project-1': makeAutonomousRunState('project-1'),
    'project-2': makeAutonomousRunState('project-2', { runId: 'auto-project-2' }),
  }
  const runtimeSessionErrors = options?.runtimeSessionErrors ?? {}
  const runtimeRunErrors = options?.runtimeRunErrors ?? {}
  const notificationDispatches = options?.notificationDispatches ?? {
    'project-1': [],
    'project-2': [],
  }
  const notificationRoutes = options?.notificationRoutes ?? {
    'project-1': [],
    'project-2': [],
  }
  const notificationDispatchErrors = options?.notificationDispatchErrors ?? {}
  const notificationRouteErrors = options?.notificationRouteErrors ?? {}
  const notificationSyncResponses = options?.notificationSyncResponses ?? {
    'project-1': makeNotificationSyncResponse('project-1'),
    'project-2': makeNotificationSyncResponse('project-2'),
  }
  const notificationSyncErrors = options?.notificationSyncErrors ?? {}
  const upsertRouteErrors = options?.upsertRouteErrors ?? {}
  const startRuntimeRunErrors = options?.startRuntimeRunErrors ?? {}
  const updateRuntimeRunControlErrors = options?.updateRuntimeRunControlErrors ?? {}
  const streamSubscriptions: Array<{
    projectId: string
    agentSessionId: string
    active: boolean
    handler: (payload: RuntimeStreamEventDto) => void
    onError: ((error: XeroDesktopError) => void) | null
    unsubscribe: ReturnType<typeof vi.fn>
  }> = []

  const getRuntimeSession = vi.fn(async (projectId: string) => {
    const error = runtimeSessionErrors[projectId]
    if (error) {
      throw error
    }

    return runtimeSessions[projectId]
  })

  const getRuntimeRun = vi.fn(async (projectId: string) => {
    const error = runtimeRunErrors[projectId]
    if (error) {
      throw error
    }

    return runtimeRuns[projectId] ?? null
  })

  const getAutonomousRun = vi.fn(async (projectId: string) => autonomousStates[projectId] ?? { run: null })

  const listNotificationDispatches = vi.fn(async (projectId: string) => {
    const error = notificationDispatchErrors[projectId]
    if (error) {
      throw error
    }

    return {
      dispatches: notificationDispatches[projectId] ?? [],
    }
  })

  const listNotificationRoutes = vi.fn(async (projectId: string) => {
    const error = notificationRouteErrors[projectId]
    if (error) {
      throw error
    }

    return {
      routes: notificationRoutes[projectId] ?? [],
    }
  })

  const syncNotificationAdapters = vi.fn(async (projectId: string) => {
    const error = notificationSyncErrors[projectId]
    if (error) {
      throw error
    }

    return notificationSyncResponses[projectId] ?? makeNotificationSyncResponse(projectId)
  })

  const upsertNotificationRoute = vi.fn(async (request: {
    projectId: string
    routeId: string
    routeKind: 'telegram' | 'discord'
    routeTarget: string
    enabled: boolean
    metadataJson?: string | null
    updatedAt: string
  }) => {
    const error = upsertRouteErrors[request.routeId]
    if (error) {
      throw error
    }

    const now = '2026-04-16T15:00:00Z'
    const currentRoutes = notificationRoutes[request.projectId] ?? []
    const existingRoute = currentRoutes.find((route) => route.routeId === request.routeId) ?? null

    const nextRoute = {
      projectId: request.projectId,
      routeId: request.routeId,
      routeKind: request.routeKind,
      routeTarget: request.routeTarget,
      enabled: request.enabled,
      metadataJson: request.metadataJson ?? null,
      credentialReadiness:
        existingRoute?.credentialReadiness ??
        (request.routeKind === 'telegram'
          ? {
              hasBotToken: true,
              hasChatId: true,
              hasWebhookUrl: false,
              ready: true,
              status: 'ready',
              diagnostic: null,
            }
          : {
              hasBotToken: true,
              hasChatId: false,
              hasWebhookUrl: true,
              ready: true,
              status: 'ready',
              diagnostic: null,
            }),
      createdAt: existingRoute?.createdAt ?? now,
      updatedAt: now,
    }

    notificationRoutes[request.projectId] = [
      nextRoute,
      ...currentRoutes.filter((route) => route.routeId !== request.routeId),
    ]

    return {
      route: nextRoute,
    }
  })

  const resolveOperatorAction = vi.fn(async () => {
    throw new Error('not used in runtime-run tests')
  })
  const resumeOperatorRun = vi.fn(
    async (
      _projectId: string,
      _actionId: string,
      _options?: { userAnswer?: string | null },
    ): Promise<ResumeOperatorRunResponseDto> => {
      throw new Error('not used in runtime-run tests')
    },
  )

  const getProjectSnapshot = vi.fn(async (projectId: string) => snapshots[projectId])
  const providerProfiles = options?.providerProfiles ?? makeProviderProfiles()
  const upsertNotificationRouteCredentials = vi.fn(
    async (
      request: UpsertNotificationRouteCredentialsRequestDto,
    ): Promise<UpsertNotificationRouteCredentialsResponseDto> => ({
      projectId: request.projectId,
      routeId: request.routeId,
      routeKind: request.routeKind,
      credentialScope: 'app_local',
      hasBotToken: Boolean(request.credentials.botToken),
      hasChatId: Boolean(request.credentials.chatId),
      hasWebhookUrl: Boolean(request.credentials.webhookUrl),
      updatedAt: request.updatedAt,
    }),
  )
  const autoNameAgentSession = vi.fn(async (request: {
    projectId: string
    agentSessionId: string
    prompt: string
    controls?: RuntimeRunControlInputDto | null
  }) => {
    const snapshot = snapshots[request.projectId]
    const existing = snapshot?.agentSessions.find((session) => session.agentSessionId === request.agentSessionId)
    if (!snapshot || !existing) {
      throw new Error(`Missing agent session ${request.agentSessionId}`)
    }

    const nextSession = {
      ...existing,
      title: existing.title.trim().toLowerCase() === 'new chat' ? 'System Prompt Investigation' : existing.title,
      updatedAt: '2026-04-15T20:00:02Z',
    }
    snapshots[request.projectId] = {
      ...snapshot,
      agentSessions: snapshot.agentSessions.map((session) =>
        session.agentSessionId === request.agentSessionId ? nextSession : session,
      ),
    }
    return nextSession
  })

  const adapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder: vi.fn(async () => null),
    importRepository: vi.fn(async () => {
      throw new Error('not used in runtime-run tests')
    }) as unknown as (path: string) => Promise<ImportRepositoryResponseDto>,
    listProjects: vi.fn(async () =>
      options?.listProjects ?? {
        projects: [makeProjectSummary('project-1', 'Xero')],
      },
    ),
    removeProject: vi.fn(async () => ({
      projects: options?.listProjects?.projects ?? [makeProjectSummary('project-1', 'Xero')],
    })),
    getProjectSnapshot,
    getRepositoryStatus: vi.fn(async (projectId: string) => statuses[projectId]),
    getRepositoryDiff: vi.fn(async (projectId: string, scope: 'staged' | 'unstaged' | 'worktree') =>
      makeDiff(projectId, scope),
    ),
    listProjectFiles: vi.fn(async (projectId: string, path = '/') => ({
      projectId,
      path,
      root: {
        name: 'root',
        path: '/',
        type: 'folder' as const,
        children: [],
      },
    })),
    readProjectFile: vi.fn(async (projectId: string, path: string) => ({
      kind: 'text' as const,
      projectId,
      path,
      byteLength: 0,
      modifiedAt: '2026-01-01T00:00:00Z',
      contentHash: `test-${path}`,
      mimeType: 'text/plain; charset=utf-8',
      rendererKind: 'code' as const,
      text: '',
    })),
    writeProjectFile: vi.fn(async (projectId: string, path: string) => ({ projectId, path })),
    createProjectEntry: vi.fn(async (request) => ({
      projectId: request.projectId,
      path: request.parentPath === '/' ? `/${request.name}` : `${request.parentPath}/${request.name}`,
    })),
    renameProjectEntry: vi.fn(async (request) => ({
      projectId: request.projectId,
      path: request.path.split('/').slice(0, -1).filter(Boolean).length
        ? `/${request.path.split('/').slice(0, -1).filter(Boolean).join('/')}/${request.newName}`
        : `/${request.newName}`,
    })),
    moveProjectEntry: vi.fn(async (request) => ({
      projectId: request.projectId,
      path:
        request.targetParentPath === '/'
          ? `/${request.path.split('/').filter(Boolean).pop() ?? ''}`
          : `${request.targetParentPath}/${request.path.split('/').filter(Boolean).pop() ?? ''}`,
    })),
    deleteProjectEntry: vi.fn(async (projectId: string, path: string) => ({ projectId, path })),
    getAutonomousRun,
    getRuntimeRun,
    getRuntimeSession,
    autoNameAgentSession,
    getProviderModelCatalog: vi.fn(async (profileId: string): Promise<ProviderModelCatalogDto> => {
      const profile = providerProfiles.profiles.find((candidate) => candidate.profileId === profileId)
      if (!profile) {
        throw new Error(`Missing provider profile ${profileId}`)
      }

      return {
        profileId,
        providerId: profile.providerId,
        configuredModelId: profile.modelId,
        source: profile.providerId === 'openrouter' && !profile.readiness.ready ? 'unavailable' : 'live',
        fetchedAt: profile.providerId === 'openrouter' && !profile.readiness.ready ? null : '2026-04-21T12:00:00Z',
        lastSuccessAt: profile.providerId === 'openrouter' && !profile.readiness.ready ? null : '2026-04-21T12:00:00Z',
        lastRefreshError:
          profile.providerId === 'openrouter' && !profile.readiness.ready
            ? {
                code: 'openrouter_credentials_missing',
                message: 'Configure an OpenRouter API key before refreshing provider models.',
                retryable: false,
              }
            : null,
        models:
          profile.providerId === 'openrouter'
            ? profile.readiness.ready
              ? [
                  {
                    modelId: profile.modelId,
                    displayName: 'OpenRouter model',
                    thinking: {
                      supported: true,
                      effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                      defaultEffort: 'medium',
                    },
                  },
                ]
              : []
            : [
                {
                  modelId: 'openai_codex',
                  displayName: 'OpenAI Codex',
                  thinking: {
                    supported: true,
                    effortOptions: ['low', 'medium', 'high'],
                    defaultEffort: 'medium',
                  },
                },
              ],
      }
    }),
    startOpenAiLogin: vi.fn(
      async (_options?: { originator?: string | null }) =>
        makeProviderAuthSession(),
    ),
    submitOpenAiCallback: vi.fn(
      async (
        _flowId: string,
        _options?: { manualInput?: string | null },
      ) => makeProviderAuthSession(),
    ),
    startAutonomousRun: vi.fn(async (projectId: string, _agentSessionId: string) => {
      const nextState = makeAutonomousRunState(projectId, {
        duplicateStartDetected: Boolean(autonomousStates[projectId]?.run),
        duplicateStartRunId: autonomousStates[projectId]?.run?.runId ?? null,
        duplicateStartReason: autonomousStates[projectId]?.run
          ? 'Xero reused the already-active autonomous run for this project instead of launching a duplicate supervisor.'
          : null,
      })
      autonomousStates[projectId] = nextState
      return nextState
    }),
    startRuntimeRun: vi.fn(async (
      projectId: string,
      _agentSessionId: string,
      options?: { initialControls?: RuntimeRunControlInputDto | null; initialPrompt?: string | null },
    ) => {
      const error = startRuntimeRunErrors[projectId]
      if (error) {
        throw error
      }

      const nextRun =
        runtimeRuns[projectId] ??
        makeRuntimeRun(projectId, {
          controls: {
            active: {
              providerProfileId: options?.initialControls?.providerProfileId ?? 'openai_codex-default',
              runtimeAgentId: options?.initialControls?.runtimeAgentId ?? 'ask',
              modelId: options?.initialControls?.modelId ?? 'openai_codex',
              thinkingEffort: options?.initialControls?.thinkingEffort ?? 'medium',
              approvalMode: options?.initialControls?.approvalMode ?? 'suggest',
              planModeRequired: options?.initialControls?.planModeRequired ?? false,
              revision: 1,
              appliedAt: '2026-04-15T20:00:00Z',
            },
            pending: options?.initialPrompt
              ? {
                  providerProfileId: options?.initialControls?.providerProfileId ?? 'openai_codex-default',
                  runtimeAgentId: options?.initialControls?.runtimeAgentId ?? 'ask',
                  modelId: options?.initialControls?.modelId ?? 'openai_codex',
                  thinkingEffort: options?.initialControls?.thinkingEffort ?? 'medium',
                  approvalMode: options?.initialControls?.approvalMode ?? 'suggest',
                  planModeRequired: options?.initialControls?.planModeRequired ?? false,
                  revision: 2,
                  queuedAt: '2026-04-15T20:00:01Z',
                  queuedPrompt: options.initialPrompt,
                  queuedPromptAt: '2026-04-15T20:00:01Z',
                }
              : null,
          },
        })
      runtimeRuns[projectId] = nextRun
      return nextRun
    }),
    updateRuntimeRunControls: vi.fn(async (request: {
      projectId: string
      agentSessionId: string
      runId: string
      controls?: RuntimeRunControlInputDto | null
      prompt?: string | null
    }) => {
      const error = updateRuntimeRunControlErrors[request.projectId]
      if (error) {
        throw error
      }

      const currentRun = runtimeRuns[request.projectId] ?? makeRuntimeRun(request.projectId, { runId: request.runId })
      const basePending = currentRun.controls.pending
      const queuedAt = '2026-04-15T20:00:07Z'
      const nextRun = {
        ...currentRun,
        controls: {
          active: currentRun.controls.active,
          pending: {
            providerProfileId:
              request.controls?.providerProfileId ??
              basePending?.providerProfileId ??
              currentRun.controls.active.providerProfileId ??
              null,
            modelId: request.controls?.modelId ?? basePending?.modelId ?? currentRun.controls.active.modelId,
            runtimeAgentId:
              request.controls?.runtimeAgentId ?? basePending?.runtimeAgentId ?? currentRun.controls.active.runtimeAgentId,
            thinkingEffort:
              request.controls?.thinkingEffort ??
              basePending?.thinkingEffort ??
              currentRun.controls.active.thinkingEffort ??
              null,
            approvalMode:
              request.controls?.approvalMode ?? basePending?.approvalMode ?? currentRun.controls.active.approvalMode,
            planModeRequired:
              request.controls?.planModeRequired ??
              basePending?.planModeRequired ??
              currentRun.controls.active.planModeRequired,
            revision: basePending ? basePending.revision + 1 : currentRun.controls.active.revision + 1,
            queuedAt,
            queuedPrompt: request.prompt ?? basePending?.queuedPrompt ?? null,
            queuedPromptAt: request.prompt ? queuedAt : basePending?.queuedPromptAt ?? null,
          },
        },
        updatedAt: queuedAt,
      }
      runtimeRuns[request.projectId] = nextRun
      return nextRun
    }),
    startRuntimeSession: vi.fn(
      async (projectId: string, _options?: { providerProfileId?: string | null }) => runtimeSessions[projectId],
    ),
    cancelAutonomousRun: vi.fn(async (projectId: string, runId: string) => {
      const nextState = makeAutonomousRunState(projectId, {
        runId,
        status: 'cancelled',
        recoveryState: 'terminal',
        cancelledAt: '2026-04-16T20:10:00Z',
        cancelReason: {
          code: 'operator_cancelled',
          message: 'Operator cancelled the autonomous run from the desktop shell.',
        },
        updatedAt: '2026-04-16T20:10:00Z',
      })
      autonomousStates[projectId] = nextState
      return nextState
    }),
    stopRuntimeRun: vi.fn(async (projectId: string) => runtimeRuns[projectId] ?? null),
    logoutRuntimeSession: vi.fn(async (projectId: string) => makeRuntimeSession(projectId)),
    resolveOperatorAction: resolveOperatorAction as never,
    resumeOperatorRun: resumeOperatorRun as never,
    listNotificationRoutes,
    listNotificationDispatches,
    upsertNotificationRoute: upsertNotificationRoute as never,
    upsertNotificationRouteCredentials,
    recordNotificationDispatchOutcome: vi.fn(async () => {
      throw new Error('not used in runtime-run tests')
    }) as never,
    submitNotificationReply: vi.fn(async () => {
      throw new Error('not used in runtime-run tests')
    }) as never,
    syncNotificationAdapters,
    subscribeRuntimeStream: vi.fn(
      async (
        projectId: string,
        agentSessionId: string,
        _itemKinds,
        handler: (payload: RuntimeStreamEventDto) => void,
        onError?: (error: XeroDesktopError) => void,
      ) => {
        const subscription = {
          projectId,
          agentSessionId,
          active: true,
          handler,
          onError: onError ?? null,
          unsubscribe: vi.fn(() => {
            subscription.active = false
          }),
        }
        streamSubscriptions.push(subscription)

        return {
          response:
            options?.subscribeResponses?.[projectId] ??
            makeStreamResponse(projectId, {
              sessionId: runtimeSessions[projectId]?.sessionId ?? 'session-1',
              flowId: runtimeSessions[projectId]?.flowId ?? 'flow-1',
            }),
          unsubscribe: subscription.unsubscribe,
        }
      },
    ),
    onProjectUpdated: vi.fn(
      async (
        handler: (payload: ProjectUpdatedPayloadDto) => void,
        onError?: (error: XeroDesktopError) => void,
      ) => {
        projectUpdatedHandler = handler
        projectUpdatedErrorHandler = onError ?? null
        return () => undefined
      },
    ),
    onRepositoryStatusChanged: vi.fn(async () => () => undefined),
    onRuntimeUpdated: vi.fn(async () => () => undefined),
    onRuntimeRunUpdated: vi.fn(
      async (
        handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
        onError?: (error: XeroDesktopError) => void,
      ) => {
        runtimeRunUpdatedHandler = handler
        runtimeRunUpdatedErrorHandler = onError ?? null
        return () => undefined
      },
    ),
    onAgentUsageUpdated: vi.fn(async () => () => undefined),
  } satisfies Partial<XeroDesktopAdapter>

  return {
    adapter: adapter as unknown as XeroDesktopAdapter,
    getProjectSnapshot,
    getRuntimeRun,
    getAutonomousRun,
    listNotificationRoutes,
    listNotificationDispatches,
    syncNotificationAdapters,
    upsertNotificationRoute,
    startRuntimeRun: adapter.startRuntimeRun,
    autoNameAgentSession,
    updateRuntimeRunControls: adapter.updateRuntimeRunControls,
    resumeOperatorRun,
    subscribeRuntimeStream: adapter.subscribeRuntimeStream,
    streamSubscriptions,
    emitProjectUpdated(payload: ProjectUpdatedPayloadDto) {
      projectUpdatedHandler?.(payload)
    },
    emitProjectUpdatedError(error: XeroDesktopError) {
      projectUpdatedErrorHandler?.(error)
    },
    emitRuntimeRunUpdated(
      payload: RuntimeRunUpdatedPayloadDto | (Omit<RuntimeRunUpdatedPayloadDto, 'agentSessionId'> & { agentSessionId?: string }),
    ) {
      runtimeRunUpdatedHandler?.({
        agentSessionId: payload.run?.agentSessionId ?? 'agent-session-main',
        ...payload,
      })
    },
    emitRuntimeRunUpdatedError(error: XeroDesktopError) {
      runtimeRunUpdatedErrorHandler?.(error)
    },
    emitRuntimeStream(
      index: number,
      payload: RuntimeStreamEventDto | (Omit<RuntimeStreamEventDto, 'agentSessionId'> & { agentSessionId?: string }),
    ) {
      const requested = streamSubscriptions[index]
      const subscription =
        requested?.active
          ? requested
          : streamSubscriptions
              .slice()
              .reverse()
              .find((candidate) => candidate.active)
      subscription?.handler({
        ...payload,
        agentSessionId: payload.agentSessionId ?? subscription.agentSessionId,
      })
    },
    emitRuntimeStreamError(index: number, error: XeroDesktopError) {
      streamSubscriptions[index]?.onError?.(error)
    },
  }
}

function Harness({ adapter }: { adapter: XeroDesktopAdapter }) {
  const state = useXeroDesktopState({ adapter })
  const approvals = state.agentView?.approvalRequests ?? []
  const resumeHistory = state.agentView?.resumeHistory ?? []
  const notificationRoutes = state.agentView?.notificationRoutes ?? []
  const telegramRoute = notificationRoutes.find((route) => route.routeId === 'telegram-primary') ?? null
  const discordRoute = notificationRoutes.find((route) => route.routeId === 'discord-fallback') ?? null
  const telegramChannel = state.agentView?.notificationChannelHealth.find((channel) => channel.routeKind === 'telegram') ?? null
  const discordChannel = state.agentView?.notificationChannelHealth.find((channel) => channel.routeKind === 'discord') ?? null
  const firstApproval = approvals[0] ?? null
  const latestResumeForFirstApproval = firstApproval
    ? resumeHistory.find((entry) => entry.sourceActionId === firstApproval.actionId) ?? null
    : null
  const firstApprovalBroker = firstApproval
    ? state.activeProject?.notificationBroker.byActionId[firstApproval.actionId] ?? null
    : null
  const firstToolCall = state.agentView?.runtimeStream?.toolCalls?.[0] ?? null
  const firstToolSummary = firstToolCall?.toolSummary ?? null
  const firstToolMcpSummary = firstToolSummary?.kind === 'mcp_capability' ? firstToolSummary : null
  const firstToolBrowserComputerUseSummary = firstToolSummary?.kind === 'browser_computer_use' ? firstToolSummary : null
  const firstApprovalResumeState =
    !firstApproval
      ? 'none'
      : state.agentView?.operatorActionStatus === 'running' && state.agentView?.pendingOperatorActionId === firstApproval.actionId
        ? 'running'
        : latestResumeForFirstApproval?.status ?? 'waiting'
  return (
    <div>
      <div data-testid="active-project-id">{state.activeProjectId ?? 'none'}</div>
      <div data-testid="selected-session-title">{state.activeProject?.selectedAgentSession?.title ?? 'none'}</div>
      <div data-testid="error">{state.errorMessage ?? 'none'}</div>
      <div data-testid="refresh-source">{state.refreshSource ?? 'none'}</div>
      <div data-testid="auth-phase">{state.agentView?.authPhase ?? 'none'}</div>
      <div data-testid="runtime-provider-id">{state.agentView?.runtimeSession?.providerId ?? 'none'}</div>
      <div data-testid="runtime-run-id">{state.agentView?.runtimeRun?.runId ?? 'none'}</div>
      <div data-testid="runtime-run-provider-id">{state.agentView?.runtimeRun?.providerId ?? 'none'}</div>
      <div data-testid="runtime-run-status">{state.agentView?.runtimeRun?.status ?? 'none'}</div>
      <div data-testid="runtime-run-status-label">{state.agentView?.runtimeRun?.statusLabel ?? 'none'}</div>
      <div data-testid="runtime-run-checkpoint-count">{String(state.agentView?.runtimeRun?.checkpointCount ?? 0)}</div>
      <div data-testid="runtime-run-last-checkpoint-summary">
        {state.agentView?.runtimeRun?.latestCheckpoint?.summary ?? 'none'}
      </div>
      <div data-testid="control-truth-source">{state.agentView?.controlTruthSource ?? 'none'}</div>
      <div data-testid="selected-model-id">{state.agentView?.selectedModelId ?? 'none'}</div>
      <div data-testid="selected-thinking-effort">{state.agentView?.selectedThinkingEffort ?? 'none'}</div>
      <div data-testid="selected-approval-mode">{state.agentView?.selectedApprovalMode ?? 'none'}</div>
      <div data-testid="selected-prompt">{state.agentView?.selectedPrompt.text ?? 'none'}</div>
      <div data-testid="selected-prompt-queued-at">{state.agentView?.selectedPrompt.queuedAt ?? 'none'}</div>
      <div data-testid="active-control-model-id">{state.agentView?.runtimeRunActiveControls?.modelId ?? 'none'}</div>
      <div data-testid="active-control-thinking-effort">{state.agentView?.runtimeRunActiveControls?.thinkingEffort ?? 'none'}</div>
      <div data-testid="active-control-approval-mode">{state.agentView?.runtimeRunActiveControls?.approvalMode ?? 'none'}</div>
      <div data-testid="active-control-revision">{String(state.agentView?.runtimeRunActiveControls?.revision ?? 0)}</div>
      <div data-testid="pending-control-model-id">{state.agentView?.runtimeRunPendingControls?.modelId ?? 'none'}</div>
      <div data-testid="pending-control-thinking-effort">{state.agentView?.runtimeRunPendingControls?.thinkingEffort ?? 'none'}</div>
      <div data-testid="pending-control-approval-mode">{state.agentView?.runtimeRunPendingControls?.approvalMode ?? 'none'}</div>
      <div data-testid="pending-control-revision">{String(state.agentView?.runtimeRunPendingControls?.revision ?? 0)}</div>
      <div data-testid="pending-control-prompt">{state.agentView?.runtimeRunPendingControls?.queuedPrompt ?? 'none'}</div>
      <div data-testid="pending-control-prompt-at">{state.agentView?.runtimeRunPendingControls?.queuedPromptAt ?? 'none'}</div>
      <div data-testid="runtime-run-error">{state.agentView?.runtimeRunErrorMessage ?? 'none'}</div>
      <div data-testid="runtime-run-action-error">{state.agentView?.runtimeRunActionError?.message ?? 'none'}</div>
      <div data-testid="runtime-run-reason">{state.agentView?.runtimeRunUnavailableReason ?? 'none'}</div>
      <div data-testid="autonomous-run-id">{state.agentView?.autonomousRun?.runId ?? 'none'}</div>
      <div data-testid="autonomous-run-provider-id">{state.agentView?.autonomousRun?.providerId ?? 'none'}</div>
      <div data-testid="autonomous-run-status">{state.agentView?.autonomousRun?.status ?? 'none'}</div>
      <div data-testid="autonomous-run-recovery">{state.agentView?.autonomousRun?.recoveryState ?? 'none'}</div>
      <div data-testid="autonomous-run-duplicate-start">{String(state.agentView?.autonomousRun?.duplicateStartDetected ?? false)}</div>
      <div data-testid="autonomous-run-error">{state.agentView?.autonomousRunErrorMessage ?? 'none'}</div>
      <div data-testid="messages-reason">{state.agentView?.messagesUnavailableReason ?? 'none'}</div>
      <div data-testid="stream-status">{state.agentView?.runtimeStreamStatus ?? 'idle'}</div>
      <div data-testid="stream-run-id">{state.agentView?.runtimeStream?.runId ?? 'none'}</div>
      <div data-testid="stream-last-sequence">{String(state.agentView?.runtimeStream?.lastSequence ?? 0)}</div>
      <div data-testid="stream-item-count">{String(state.agentView?.runtimeStreamItems?.length ?? 0)}</div>
      <div data-testid="stream-tool-count">{String(state.agentView?.runtimeStream?.toolCalls.length ?? 0)}</div>
      <div data-testid="stream-tool-first-id">{firstToolCall?.toolCallId ?? 'none'}</div>
      <div data-testid="stream-tool-first-state">{firstToolCall?.toolState ?? 'none'}</div>
      <div data-testid="stream-tool-first-summary-kind">{firstToolSummary?.kind ?? 'none'}</div>
      <div data-testid="stream-tool-first-mcp-server-id">{firstToolMcpSummary?.serverId ?? 'none'}</div>
      <div data-testid="stream-tool-first-mcp-capability-kind">{firstToolMcpSummary?.capabilityKind ?? 'none'}</div>
      <div data-testid="stream-tool-first-mcp-capability-id">{firstToolMcpSummary?.capabilityId ?? 'none'}</div>
      <div data-testid="stream-tool-first-mcp-capability-name">{firstToolMcpSummary?.capabilityName ?? 'none'}</div>
      <div data-testid="stream-tool-first-browser-surface">{firstToolBrowserComputerUseSummary?.surface ?? 'none'}</div>
      <div data-testid="stream-tool-first-browser-action">{firstToolBrowserComputerUseSummary?.action ?? 'none'}</div>
      <div data-testid="stream-tool-first-browser-status">{firstToolBrowserComputerUseSummary?.status ?? 'none'}</div>
      <div data-testid="stream-tool-first-browser-target">{firstToolBrowserComputerUseSummary?.target ?? 'none'}</div>
      <div data-testid="stream-tool-first-browser-outcome">{firstToolBrowserComputerUseSummary?.outcome ?? 'none'}</div>
      <div data-testid="stream-skill-count">{String(state.agentView?.skillItems?.length ?? 0)}</div>
      <div data-testid="stream-skill-first-id">{state.agentView?.skillItems?.[0]?.skillId ?? 'none'}</div>
      <div data-testid="stream-skill-first-stage">{state.agentView?.skillItems?.[0]?.stage ?? 'none'}</div>
      <div data-testid="stream-skill-first-result">{state.agentView?.skillItems?.[0]?.result ?? 'none'}</div>
      <div data-testid="stream-action-required-count">{String(state.agentView?.actionRequiredItems?.length ?? 0)}</div>
      <div data-testid="pending-approval-count">{String(state.agentView?.pendingApprovalCount ?? 0)}</div>
      <div data-testid="resume-history-count">{String(resumeHistory.length)}</div>
      <div data-testid="latest-resume-status">{resumeHistory[0]?.status ?? 'none'}</div>
      <div data-testid="latest-resume-source-action-id">{resumeHistory[0]?.sourceActionId ?? 'none'}</div>
      <div data-testid="first-approval-action-id">{firstApproval?.actionId ?? 'none'}</div>
      <div data-testid="first-approval-status">{firstApproval?.status ?? 'none'}</div>
      <div data-testid="first-approval-resume-state">{firstApprovalResumeState}</div>
      <div data-testid="broker-dispatch-count">{String(state.activeProject?.notificationBroker.dispatchCount ?? 0)}</div>
      <div data-testid="broker-failed-count">{String(state.activeProject?.notificationBroker.failedCount ?? 0)}</div>
      <div data-testid="route-count">{String(notificationRoutes.length)}</div>
      <div data-testid="route-load-status">{state.agentView?.notificationRouteLoadStatus ?? 'idle'}</div>
      <div data-testid="route-load-error">{state.agentView?.notificationRouteError?.message ?? 'none'}</div>
      <div data-testid="sync-dispatch-attempted">
        {String(state.agentView?.notificationSyncSummary?.dispatch.attemptedCount ?? 0)}
      </div>
      <div data-testid="sync-reply-accepted">
        {String(state.agentView?.notificationSyncSummary?.replies.acceptedCount ?? 0)}
      </div>
      <div data-testid="sync-reply-rejected">
        {String(state.agentView?.notificationSyncSummary?.replies.rejectedCount ?? 0)}
      </div>
      <div data-testid="sync-error">{state.agentView?.notificationSyncError?.message ?? 'none'}</div>
      <div data-testid="sync-polling-active">{String(state.agentView?.notificationSyncPollingActive ?? false)}</div>
      <div data-testid="sync-polling-action-id">{state.agentView?.notificationSyncPollingActionId ?? 'none'}</div>
      <div data-testid="sync-polling-boundary-id">{state.agentView?.notificationSyncPollingBoundaryId ?? 'none'}</div>
      <div data-testid="trust-state">{state.agentView?.trustSnapshot?.state ?? 'none'}</div>
      <div data-testid="trust-credentials-state">{state.agentView?.trustSnapshot?.credentialsState ?? 'none'}</div>
      <div data-testid="trust-routes-state">{state.agentView?.trustSnapshot?.routesState ?? 'none'}</div>
      <div data-testid="trust-sync-state">{state.agentView?.trustSnapshot?.syncState ?? 'none'}</div>
      <div data-testid="trust-ready-credential-count">
        {String(state.agentView?.trustSnapshot?.readyCredentialRouteCount ?? 0)}
      </div>
      <div data-testid="trust-missing-credential-count">
        {String(state.agentView?.trustSnapshot?.missingCredentialRouteCount ?? 0)}
      </div>
      <div data-testid="trust-malformed-credential-count">
        {String(state.agentView?.trustSnapshot?.malformedCredentialRouteCount ?? 0)}
      </div>
      <div data-testid="trust-projection-error">
        {state.agentView?.trustSnapshot?.projectionError?.message ?? 'none'}
      </div>
      <div data-testid="route-mutation-status">{state.agentView?.notificationRouteMutationStatus ?? 'idle'}</div>
      <div data-testid="route-mutation-error">{state.agentView?.notificationRouteMutationError?.message ?? 'none'}</div>
      <div data-testid="telegram-channel-health">{telegramChannel?.health ?? 'none'}</div>
      <div data-testid="telegram-channel-failed-count">{String(telegramChannel?.failedCount ?? 0)}</div>
      <div data-testid="discord-channel-health">{discordChannel?.health ?? 'none'}</div>
      <div data-testid="route-telegram-enabled">{String(telegramRoute?.enabled ?? false)}</div>
      <div data-testid="route-discord-enabled">{String(discordRoute?.enabled ?? false)}</div>
      <div data-testid="route-telegram-credential-status">{telegramRoute?.credentialReadiness?.status ?? 'none'}</div>
      <div data-testid="route-discord-credential-status">{discordRoute?.credentialReadiness?.status ?? 'none'}</div>
      <div data-testid="route-discord-credential-code">
        {discordRoute?.credentialReadiness?.diagnostic?.code ?? 'none'}
      </div>
      <div data-testid="first-approval-broker-dispatch-count">{String(firstApprovalBroker?.dispatchCount ?? 0)}</div>
      <div data-testid="first-approval-broker-has-failures">{String(firstApprovalBroker?.hasFailures ?? false)}</div>
      <div data-testid="operator-action-status">{state.agentView?.operatorActionStatus ?? 'idle'}</div>
      <div data-testid="pending-operator-action-id">{state.agentView?.pendingOperatorActionId ?? 'none'}</div>
      <div data-testid="operator-action-error">{state.agentView?.operatorActionError?.message ?? 'none'}</div>
      <button onClick={() => void state.retry()} type="button">
        Retry state
      </button>
      <button
        onClick={() =>
          void state
            .startRuntimeRun({
              controls: {
                modelId: 'openai/gpt-5-mini',
                runtimeAgentId: 'engineer',
                thinkingEffort: 'high',
                approvalMode: 'auto_edit',
                planModeRequired: false,
              },
              prompt: 'Review the latest diff before continuing.',
            })
            .catch(() => undefined)
        }
        type="button"
      >
        Start runtime run with controls
      </button>
      <button
        onClick={() =>
          void state
            .updateRuntimeRunControls({
              controls: {
                modelId: 'openai/gpt-5-mini',
                runtimeAgentId: 'engineer',
                thinkingEffort: 'high',
                approvalMode: 'auto_edit',
                planModeRequired: false,
              },
              prompt: 'Review the latest diff before continuing.',
            })
            .catch(() => undefined)
        }
        type="button"
      >
        Queue runtime controls
      </button>
      <button
        onClick={() =>
          void state
            .updateRuntimeRunControls({
              controls: {
                modelId: 'openai/gpt-5-mini',
                runtimeAgentId: 'engineer',
                thinkingEffort: 'high',
                approvalMode: 'auto_edit',
                planModeRequired: false,
              },
            })
            .catch(() => undefined)
        }
        type="button"
      >
        Queue runtime controls only
      </button>
      <button onClick={() => void state.startAutonomousRun().catch(() => undefined)} type="button">
        Start autonomous run
      </button>
      <button onClick={() => void state.inspectAutonomousRun().catch(() => undefined)} type="button">
        Inspect autonomous run
      </button>
      <button
        onClick={() => void state.cancelAutonomousRun(state.agentView?.autonomousRun?.runId ?? 'missing').catch(() => undefined)}
        type="button"
      >
        Cancel autonomous run
      </button>
      <button onClick={() => void state.selectProject('project-2')} type="button">
        Select project 2
      </button>
      <button
        onClick={() =>
          void state
            .resumeOperatorRun('scope:auto-dispatch:workflow-research:requires_user_input', {
              userAnswer: 'Proceed after validating repo changes.',
            })
            .catch(() => undefined)
        }
        type="button"
      >
        Resume gate-linked run
      </button>
      <button
        onClick={() =>
          void state
            .resumeOperatorRun('scope:auto-dispatch:workflow-research:requires_user_input', {
              userAnswer: '   ',
            })
            .catch(() => undefined)
        }
        type="button"
      >
        Resume gate-linked run with invalid answer
      </button>
      <button
        onClick={() =>
          void state
            .refreshNotificationRoutes({ force: true })
            .catch(() => undefined)
        }
        type="button"
      >
        Refresh notification routes
      </button>
      <button
        onClick={() =>
          void state
            .upsertNotificationRoute({
              routeId: 'telegram-primary',
              routeKind: 'telegram',
              routeTarget: '@ops-room',
              enabled: false,
              metadataJson: null,
              updatedAt: '2026-04-16T15:00:00Z',
            })
            .catch(() => undefined)
        }
        type="button"
      >
        Disable telegram route
      </button>
    </div>
  )
}

describe('useXeroDesktopState runtime-run hydration', () => {




  it('auto-names a new default-titled session from the first submitted prompt', async () => {
    const newChatSnapshot = makeSnapshot('project-1', 'Xero')
    newChatSnapshot.agentSessions = [
      {
        ...makeAgentSession('project-1'),
        title: 'New Chat',
        summary: '',
        lastRunId: null,
      },
    ]
    const setup = createMockAdapter({
      snapshots: {
        'project-1': newChatSnapshot,
      },
      runtimeRuns: {
        'project-1': null,
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('none'))
    expect(screen.getByTestId('selected-session-title')).toHaveTextContent('New Chat')
    await act(async () => {
      await Promise.resolve()
    })

    await act(async () => {
      fireEvent.click(screen.getByRole('button', { name: 'Start runtime run with controls' }))
    })

    await waitFor(() => expect(setup.startRuntimeRun).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1'))
    await waitFor(() =>
      expect(screen.getByTestId('selected-session-title')).toHaveTextContent('System Prompt Investigation'),
    )
    expect(setup.autoNameAgentSession).toHaveBeenCalledWith({
      projectId: 'project-1',
      agentSessionId: 'agent-session-main',
      prompt: 'Review the latest diff before continuing.',
      controls: expect.objectContaining({
        modelId: 'openai/gpt-5-mini',
        thinkingEffort: 'high',
      }),
    })
  })

  it('preserves the last truthful runtime-run view when a later run refresh fails', async () => {
    const setup = createMockAdapter({
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1'),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1'))

    setup.getRuntimeRun.mockRejectedValueOnce(new Error('runtime run refresh failed'))
    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('runtime run refresh failed'))
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('runtime-run-last-checkpoint-summary')).toHaveTextContent(
      'Recovered repository context before reconnecting the live feed.',
    )
  })

  it('queues pending runtime-run controls without reloading unrelated project state and projects pending-versus-active truth into agentView', async () => {
    const setup = createMockAdapter({
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1'),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1'))
    expect(screen.getByTestId('control-truth-source')).toHaveTextContent('runtime_run')
    expect(screen.getByTestId('selected-model-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('selected-thinking-effort')).toHaveTextContent('medium')
    expect(screen.getByTestId('selected-approval-mode')).toHaveTextContent('suggest')
    expect(screen.getByTestId('pending-control-model-id')).toHaveTextContent('none')

    const initialSnapshotCalls = setup.getProjectSnapshot.mock.calls.length
    fireEvent.click(screen.getByRole('button', { name: 'Queue runtime controls' }))

    await waitFor(() => expect(screen.getByTestId('pending-control-model-id')).toHaveTextContent('openai/gpt-5-mini'))
    expect(screen.getByTestId('active-control-model-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('selected-model-id')).toHaveTextContent('openai/gpt-5-mini')
    expect(screen.getByTestId('selected-thinking-effort')).toHaveTextContent('high')
    expect(screen.getByTestId('selected-approval-mode')).toHaveTextContent('auto_edit')
    expect(screen.getByTestId('selected-prompt')).toHaveTextContent('Review the latest diff before continuing.')
    expect(screen.getByTestId('pending-control-revision')).toHaveTextContent('2')
    expect(setup.getProjectSnapshot.mock.calls.length).toBe(initialSnapshotCalls)
    expect(setup.updateRuntimeRunControls).toHaveBeenCalledTimes(1)
  })

  it('updates pending runtime-run controls without dropping an already queued prompt', async () => {
    const setup = createMockAdapter({
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', {
          controls: {
            active: {
              providerProfileId: 'openai_codex-default',
              runtimeAgentId: 'ask',
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'suggest',
              planModeRequired: false,
              revision: 1,
              appliedAt: '2026-04-15T20:00:00Z',
            },
            pending: {
              providerProfileId: 'openai_codex-default',
              runtimeAgentId: 'ask',
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'suggest',
              planModeRequired: false,
              revision: 2,
              queuedAt: '2026-04-15T20:00:01Z',
              queuedPrompt: 'First queued prompt.',
              queuedPromptAt: '2026-04-15T20:00:01Z',
            },
          },
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('pending-control-prompt')).toHaveTextContent('First queued prompt.'))
    fireEvent.click(screen.getByRole('button', { name: 'Queue runtime controls only' }))

    await waitFor(() => expect(screen.getByTestId('pending-control-model-id')).toHaveTextContent('openai/gpt-5-mini'))
    expect(screen.getByTestId('pending-control-prompt')).toHaveTextContent('First queued prompt.')
    expect(screen.getByTestId('pending-control-prompt-at')).toHaveTextContent('2026-04-15T20:00:01Z')
    expect(screen.getByTestId('pending-control-revision')).toHaveTextContent('3')
  })

  it('hydrates pending YOLO selection without treating it as already active', async () => {
    const setup = createMockAdapter({
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', {
          controls: {
            active: {
              runtimeAgentId: 'ask',
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'suggest',
              planModeRequired: false,
              revision: 1,
              appliedAt: '2026-04-15T20:00:00Z',
            },
            pending: {
              runtimeAgentId: 'engineer',
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'yolo',
              planModeRequired: false,
              revision: 2,
              queuedAt: '2026-04-15T20:00:07Z',
              queuedPrompt: null,
              queuedPromptAt: null,
            },
          },
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1'))
    expect(screen.getByTestId('control-truth-source')).toHaveTextContent('runtime_run')
    expect(screen.getByTestId('selected-approval-mode')).toHaveTextContent('yolo')
    expect(screen.getByTestId('active-control-approval-mode')).toHaveTextContent('suggest')
    expect(screen.getByTestId('pending-control-approval-mode')).toHaveTextContent('yolo')
  })

  it('preserves the last truthful runtime-run control projection when queueing controls fails and refreshes only runtime-run metadata', async () => {
    const setup = createMockAdapter({
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1'),
      },
      updateRuntimeRunControlErrors: {
        'project-1': new Error('runtime controls queue failed'),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1'))

    const initialSnapshotCalls = setup.getProjectSnapshot.mock.calls.length
    const initialRuntimeRunCalls = setup.getRuntimeRun.mock.calls.length
    fireEvent.click(screen.getByRole('button', { name: 'Queue runtime controls' }))

    await waitFor(() => expect(setup.updateRuntimeRunControls).toHaveBeenCalledTimes(1), { timeout: 3000 })
    await waitFor(
      () => expect(screen.getByTestId('runtime-run-action-error')).toHaveTextContent('runtime controls queue failed'),
      { timeout: 3000 },
    )
    expect(screen.getByTestId('selected-model-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('selected-thinking-effort')).toHaveTextContent('medium')
    expect(screen.getByTestId('selected-approval-mode')).toHaveTextContent('suggest')
    expect(screen.getByTestId('pending-control-model-id')).toHaveTextContent('none')
    expect(setup.getProjectSnapshot.mock.calls.length).toBe(initialSnapshotCalls)
    expect(setup.getRuntimeRun.mock.calls.length).toBeGreaterThan(initialRuntimeRunCalls)
  })

  it('hydrates autonomous run truth independently from the durable ledger', async () => {
    const setup = createMockAdapter({
      autonomousStates: {
        'project-1': makeAutonomousRunState('project-1', {
          runId: 'auto-project-1',
          providerId: 'azure_openai',
          recoveryState: 'recovery_required',
          pausedAt: '2026-04-16T20:03:00Z',
          pauseReason: {
            code: 'operator_pause',
            message: 'Operator paused the autonomous run for review.',
          },
          updatedAt: '2026-04-16T20:03:00Z',
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('autonomous-run-id')).toHaveTextContent('auto-project-1'))
    expect(screen.getByTestId('autonomous-run-provider-id')).toHaveTextContent('azure_openai')
    expect(screen.getByTestId('autonomous-run-status')).toHaveTextContent('running')
    expect(screen.getByTestId('autonomous-run-recovery')).toHaveTextContent('recovery_required')
  })

  it('preserves the last truthful autonomous run state when later autonomous refreshes fail', async () => {
    const setup = createMockAdapter({
      autonomousStates: {
        'project-1': makeAutonomousRunState('project-1', { runId: 'auto-project-1', providerId: 'azure_openai' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('autonomous-run-id')).toHaveTextContent('auto-project-1'))

    setup.getAutonomousRun.mockRejectedValueOnce(new Error('autonomous refresh failed'))
    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('autonomous refresh failed'))
    expect(screen.getByTestId('autonomous-run-id')).toHaveTextContent('auto-project-1')
    expect(screen.getByTestId('autonomous-run-provider-id')).toHaveTextContent('azure_openai')
  })

  it('starts, inspects, and cancels the autonomous run through the hook actions', async () => {
    const setup = createMockAdapter({
      autonomousStates: {
        'project-1': null,
      },
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('autonomous-run-id')).toHaveTextContent('none')

    fireEvent.click(screen.getByRole('button', { name: 'Start autonomous run' }))
    await waitFor(() => expect(screen.getByTestId('autonomous-run-id')).toHaveTextContent('auto-project-1'))
    expect(screen.getByTestId('autonomous-run-duplicate-start')).toHaveTextContent('false')

    fireEvent.click(screen.getByRole('button', { name: 'Inspect autonomous run' }))
    await waitFor(() => expect(setup.getAutonomousRun).toHaveBeenCalled())

    fireEvent.click(screen.getByRole('button', { name: 'Cancel autonomous run' }))
    await waitFor(() => expect(screen.getByTestId('autonomous-run-status')).toHaveTextContent('cancelled'))
    expect(screen.getByTestId('autonomous-run-recovery')).toHaveTextContent('terminal')
  })

  it('keeps recovered run state visible while the live runtime stream is still reconnecting', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1'),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('subscribing'))
    expect(screen.getByTestId('runtime-run-checkpoint-count')).toHaveTextContent('2')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent(
      'Xero is reconnecting the live runtime stream for this selected project.',
    )
  })

  it('resubscribes on runtime_run:updated when the active session receives a new run id', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    let autoDispatched = false

    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'Xero' : 'orchestra')
      if (projectId !== 'project-1') {
        return snapshot
      }

      if (!autoDispatched) {
        return snapshot
      }

      return snapshot
    })

    vi.mocked(setup.getRuntimeRun).mockImplementation(async (projectId: string) => {
      if (projectId === 'project-1') {
        return makeRuntimeRun('project-1', { runId: autoDispatched ? 'run-project-1b' : 'run-project-1' })
      }

      return makeRuntimeRun(projectId, { runId: `run-${projectId}` })
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))
    expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(1)

    vi.mocked(setup.subscribeRuntimeStream).mockResolvedValueOnce({
      response: makeStreamResponse('project-1', {
        runId: 'run-project-1b',
        sessionId: 'session-1',
        flowId: 'flow-1',
      }),
      unsubscribe: vi.fn(),
    })

    autoDispatched = true
    const snapshotCallsBeforeEvent = vi.mocked(setup.getProjectSnapshot).mock.calls.length
    const routeRefreshesBeforeEvent = vi.mocked(setup.listNotificationRoutes).mock.calls.length
    const syncRefreshesBeforeEvent = vi.mocked(setup.syncNotificationAdapters).mock.calls.length

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', { runId: 'run-project-1b' }),
      })
    })

    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1b'))
    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1b'))

    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1b')
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('0')
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('runtime_run:updated')
    expect(vi.mocked(setup.getProjectSnapshot).mock.calls.length).toBe(snapshotCallsBeforeEvent)
    expect(vi.mocked(setup.listNotificationRoutes).mock.calls.length).toBe(routeRefreshesBeforeEvent)
    expect(vi.mocked(setup.syncNotificationAdapters).mock.calls.length).toBe(syncRefreshesBeforeEvent)
  })

  it('hydrates gate-linked pending approvals from durable snapshot truth on project:updated refresh', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    let includeGatePause = false
    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'Xero' : 'orchestra')

      if (projectId === 'project-1' && includeGatePause) {
        return {
          ...snapshot,
          approvalRequests: [makeGateLinkedPendingApproval()],
        }
      }

      return snapshot
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
    expect(screen.getByTestId('stream-action-required-count')).toHaveTextContent('0')
    await waitFor(() => expect(vi.mocked(setup.adapter.onProjectUpdated)).toHaveBeenCalledTimes(1))

    includeGatePause = true
    act(() => {
      setup.emitProjectUpdated({
        project: makeProjectSummary('project-1', 'Xero'),
        reason: 'metadata_changed',
      })
    })

    await waitFor(() => expect(screen.getByTestId('refresh-source')).toHaveTextContent('project:updated'))
    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('stream-action-required-count')).toHaveTextContent('0')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('waiting')
  })

  it('keeps gate pauses visible until resume succeeds, then clears pending state from refreshed snapshot truth', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    let snapshotStage: 'pending' | 'cleared' = 'pending'
    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'Xero' : 'orchestra')

      if (projectId === 'project-1' && snapshotStage === 'pending') {
        return {
          ...snapshot,
          approvalRequests: [makeGateLinkedPendingApproval(actionId)],
        }
      }

      return snapshot
    })

    let resolveResume: (() => void) | null = null
    const resumePromise = new Promise<void>((resolve) => {
      resolveResume = resolve
    })
    setup.resumeOperatorRun.mockImplementation(async (_projectId, _actionId, options) => {
      if (!options?.userAnswer?.trim()) {
        throw new Error('A non-empty operator answer is required to clear gate-linked approvals.')
      }

      await resumePromise
      return undefined as never
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('0')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('waiting')

    fireEvent.click(screen.getByRole('button', { name: 'Resume gate-linked run with invalid answer' }))

    await waitFor(() =>
      expect(setup.resumeOperatorRun).toHaveBeenNthCalledWith(1, 'project-1', actionId, {
        userAnswer: '   ',
      }),
    )
    await waitFor(() =>
      expect(screen.getByTestId('operator-action-error')).toHaveTextContent(
        'A non-empty operator answer is required to clear gate-linked approvals.',
      ),
    )
    expect(screen.getByTestId('operator-action-status')).toHaveTextContent('idle')
    expect(screen.getByTestId('pending-operator-action-id')).toHaveTextContent('none')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('waiting')

    fireEvent.click(screen.getByRole('button', { name: 'Resume gate-linked run' }))

    await waitFor(() =>
      expect(setup.resumeOperatorRun).toHaveBeenNthCalledWith(2, 'project-1', actionId, {
        userAnswer: 'Proceed after validating repo changes.',
      }),
    )
    expect(screen.getByTestId('operator-action-status')).toHaveTextContent('running')
    expect(screen.getByTestId('pending-operator-action-id')).toHaveTextContent(actionId)
    expect(screen.getByTestId('operator-action-error')).toHaveTextContent('none')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('running')

    snapshotStage = 'cleared'
    await act(async () => {
      resolveResume?.()
      await Promise.resolve()
    })

    await waitFor(() => expect(screen.getByTestId('refresh-source')).toHaveTextContent('operator:resume'))
    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0'))
    expect(screen.getByTestId('operator-action-status')).toHaveTextContent('idle')
    expect(screen.getByTestId('pending-operator-action-id')).toHaveTextContent('none')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('none')
  })

  it('projects broker dispatch metadata alongside gate-linked approvals while filtering cross-project rows', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Xero'),
          approvalRequests: [makeGateLinkedPendingApproval(actionId)],
        },
      },
      notificationDispatches: {
        'project-1': [
          makeNotificationDispatch({
            id: 1,
            projectId: 'project-1',
            actionId,
            routeId: 'telegram-primary',
            status: 'pending',
            updatedAt: '2026-04-16T13:00:00Z',
          }),
          makeNotificationDispatch({
            id: 2,
            projectId: 'project-1',
            actionId,
            routeId: 'discord-fallback',
            status: 'failed',
            attemptCount: 2,
            lastAttemptAt: '2026-04-16T13:01:00Z',
            lastErrorCode: 'notification_dispatch_http_500',
            lastErrorMessage: 'Discord webhook returned HTTP 500.',
            updatedAt: '2026-04-16T13:01:00Z',
          }),
          makeNotificationDispatch({
            id: 3,
            projectId: 'project-2',
            actionId,
            routeId: 'cross-project-row',
            status: 'sent',
            attemptCount: 1,
            lastAttemptAt: '2026-04-16T13:02:00Z',
            deliveredAt: '2026-04-16T13:02:00Z',
            updatedAt: '2026-04-16T13:02:00Z',
          }),
        ],
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('broker-dispatch-count')).toHaveTextContent('2')
    expect(screen.getByTestId('broker-failed-count')).toHaveTextContent('1')
    expect(screen.getByTestId('first-approval-broker-dispatch-count')).toHaveTextContent('2')
    expect(screen.getByTestId('first-approval-broker-has-failures')).toHaveTextContent('true')
  })

  it('surfaces broker command failures without clearing pending approvals or resume history', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Xero'),
          approvalRequests: [makeGateLinkedPendingApproval(actionId)],
          resumeHistory: [
            makeResumeHistoryEntry({
              id: 5,
              actionId,
              status: 'failed',
              summary: 'Operator resume failed while waiting for broker diagnostics.',
            }),
          ],
        },
      },
      notificationDispatchErrors: {
        'project-1': new XeroDesktopError({
          code: 'notification_dispatch_query_failed',
          errorClass: 'retryable',
          message: 'Xero could not load notification dispatches for this project.',
          retryable: true,
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('failed')
    expect(screen.getByTestId('latest-resume-status')).toHaveTextContent('failed')
    expect(screen.getByTestId('broker-dispatch-count')).toHaveTextContent('0')
    expect(screen.getByTestId('error')).toHaveTextContent(
      'Xero could not load notification dispatches for this project.',
    )
  })

  it('keeps same-run stream state during retry so replayed items can dedupe instead of clearing the feed', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))
    expect(screen.getByTestId('stream-status')).toHaveTextContent('subscribing')

    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(2))
    expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('0')
  })

  it('keeps last-known-good selected-project truth when a selection refresh fails, then converges on retry', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      listProjects: {
        projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')],
      },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Xero'),
          approvalRequests: [makeGateLinkedPendingApproval(actionId)],
          resumeHistory: [
            makeResumeHistoryEntry({
              id: 10,
              actionId,
              status: 'failed',
              summary: 'Operator resume failed while waiting for selected-project convergence.',
            }),
          ],
        },
        'project-2': makeSnapshot('project-2', 'orchestra'),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
        'project-2': makeRuntimeRun('project-2', { runId: 'run-project-2' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('1')

    setup.getProjectSnapshot.mockRejectedValueOnce(
      new XeroDesktopError({
        code: 'project_snapshot_query_failed',
        errorClass: 'retryable',
        message: 'Xero could not reload project-2 during selected-project refresh.',
        retryable: true,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() =>
      expect(screen.getByTestId('error')).toHaveTextContent(
        'Xero could not reload project-2 during selected-project refresh.',
      ),
    )
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2')
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('none')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('0')

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-2'))
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('0')
    expect(screen.getByTestId('error')).toHaveTextContent('none')
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('selection')
  })

  it('projects replayed skill lifecycle rows into the agent view during runtime reconnect', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
      subscribeResponses: {
        'project-1': makeStreamResponse('project-1', {
          runId: 'run-project-1',
          subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))
    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('subscribing'))

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'skill',
          runId: 'run-project-1',
          sequence: 4,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          toolSummary: null,
          skillId: 'find-skills',
          skillStage: 'invoke',
          skillResult: 'failed',
          skillSource: {
            repo: 'vercel-labs/skills',
            path: 'skills/find-skills',
            reference: 'main',
            treeHash: '0123456789abcdef0123456789abcdef01234567',
          },
          skillCacheStatus: 'hit',
          skillDiagnostic: {
            code: 'autonomous_skill_invoke_failed',
            message: 'Xero could not invoke autonomous skill `find-skills`.',
            retryable: false,
          },
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'Autonomous skill `find-skills` failed during invocation.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('live'))
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-skill-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-skill-first-id')).toHaveTextContent('find-skills')
    expect(screen.getByTestId('stream-skill-first-stage')).toHaveTextContent('invoke')
    expect(screen.getByTestId('stream-skill-first-result')).toHaveTextContent('failed')
  })

  it('projects MCP capability tool summaries into the agent tool lane projection', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'tool',
          runId: 'run-project-1',
          sequence: 1,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'mcp-invoke-1',
          toolName: 'mcp.invoke',
          toolState: 'failed',
          toolSummary: {
            kind: 'mcp_capability',
            serverId: 'linear',
            capabilityKind: 'prompt',
            capabilityId: 'summarize_context',
            capabilityName: 'Summarize Context',
          },
          skillId: null,
          skillStage: null,
          skillResult: null,
          skillSource: null,
          skillCacheStatus: null,
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'MCP prompt invocation failed with upstream timeout.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('live'))
    expect(screen.getByTestId('stream-tool-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-tool-first-id')).toHaveTextContent('mcp-invoke-1')
    expect(screen.getByTestId('stream-tool-first-state')).toHaveTextContent('failed')
    expect(screen.getByTestId('stream-tool-first-summary-kind')).toHaveTextContent('mcp_capability')
    expect(screen.getByTestId('stream-tool-first-mcp-server-id')).toHaveTextContent('linear')
    expect(screen.getByTestId('stream-tool-first-mcp-capability-kind')).toHaveTextContent('prompt')
    expect(screen.getByTestId('stream-tool-first-mcp-capability-id')).toHaveTextContent('summarize_context')
    expect(screen.getByTestId('stream-tool-first-mcp-capability-name')).toHaveTextContent('Summarize Context')
  })

  it('projects browser/computer-use tool summaries into the agent tool lane projection', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'tool',
          runId: 'run-project-1',
          sequence: 1,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'browser-click-1',
          toolName: 'browser.click',
          toolState: 'succeeded',
          toolSummary: {
            kind: 'browser_computer_use',
            surface: 'browser',
            action: 'click',
            status: 'succeeded',
            target: 'button[type=submit]',
            outcome: 'Clicked submit and advanced to confirmation.',
          },
          skillId: null,
          skillStage: null,
          skillResult: null,
          skillSource: null,
          skillCacheStatus: null,
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'Browser click action reached the confirmation banner.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('live'))
    expect(screen.getByTestId('stream-tool-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-tool-first-id')).toHaveTextContent('browser-click-1')
    expect(screen.getByTestId('stream-tool-first-state')).toHaveTextContent('succeeded')
    expect(screen.getByTestId('stream-tool-first-summary-kind')).toHaveTextContent('browser_computer_use')
    expect(screen.getByTestId('stream-tool-first-browser-surface')).toHaveTextContent('browser')
    expect(screen.getByTestId('stream-tool-first-browser-action')).toHaveTextContent('click')
    expect(screen.getByTestId('stream-tool-first-browser-status')).toHaveTextContent('succeeded')
    expect(screen.getByTestId('stream-tool-first-browser-target')).toHaveTextContent('button[type=submit]')
    expect(screen.getByTestId('stream-tool-first-browser-outcome')).toHaveTextContent(
      'Clicked submit and advanced to confirmation.',
    )
  })

  it('fails closed on malformed browser/computer-use tool summaries and preserves the last truthful tool lane', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'tool',
          runId: 'run-project-1',
          sequence: 1,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'browser-click-1',
          toolName: 'browser.click',
          toolState: 'succeeded',
          toolSummary: {
            kind: 'browser_computer_use',
            surface: 'browser',
            action: 'click',
            status: 'succeeded',
            target: 'button[type=submit]',
            outcome: 'Clicked submit and advanced to confirmation.',
          },
          skillId: null,
          skillStage: null,
          skillResult: null,
          skillSource: null,
          skillCacheStatus: null,
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'Browser click action reached the confirmation banner.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-tool-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('stream-tool-first-summary-kind')).toHaveTextContent('browser_computer_use')
    expect(screen.getByTestId('stream-tool-first-browser-status')).toHaveTextContent('succeeded')

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'tool',
          runId: 'run-project-1',
          sequence: 2,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'browser-click-2',
          toolName: 'browser.click',
          toolState: 'failed',
          toolSummary: {
            kind: 'browser_computer_use',
            surface: 'browser',
            action: 'click',
            status: 'done',
            target: 'button[type=submit]',
            outcome: 'Malformed browser summary.',
          },
          skillId: null,
          skillStage: null,
          skillResult: null,
          skillSource: null,
          skillCacheStatus: null,
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'Malformed browser summary.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:01Z',
        },
      } as unknown as RuntimeStreamEventDto)
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('stream-tool-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-tool-first-id')).toHaveTextContent('browser-click-1')
    expect(screen.getByTestId('stream-tool-first-summary-kind')).toHaveTextContent('browser_computer_use')
    expect(screen.getByTestId('stream-tool-first-browser-status')).toHaveTextContent('succeeded')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('malformed toolSummary payload')
  })

  it('fails closed on malformed MCP tool summaries and preserves the last truthful tool lane', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'tool',
          runId: 'run-project-1',
          sequence: 1,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'mcp-invoke-1',
          toolName: 'mcp.invoke',
          toolState: 'succeeded',
          toolSummary: {
            kind: 'mcp_capability',
            serverId: 'linear',
            capabilityKind: 'command',
            capabilityId: 'project_sync',
            capabilityName: 'Project Sync',
          },
          skillId: null,
          skillStage: null,
          skillResult: null,
          skillSource: null,
          skillCacheStatus: null,
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'MCP command completed successfully.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-tool-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('stream-tool-first-summary-kind')).toHaveTextContent('mcp_capability')
    expect(screen.getByTestId('stream-tool-first-mcp-capability-kind')).toHaveTextContent('command')

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'tool',
          runId: 'run-project-1',
          sequence: 2,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'mcp-invoke-2',
          toolName: 'mcp.invoke',
          toolState: 'failed',
          toolSummary: {
            kind: 'mcp_capability',
            serverId: 'linear',
            capabilityKind: 'unsupported_kind',
            capabilityId: 'project_sync',
            capabilityName: 'Project Sync',
          },
          skillId: null,
          skillStage: null,
          skillResult: null,
          skillSource: null,
          skillCacheStatus: null,
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'Malformed MCP summary.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:01Z',
        },
      } as unknown as RuntimeStreamEventDto)
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('stream-tool-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-tool-first-id')).toHaveTextContent('mcp-invoke-1')
    expect(screen.getByTestId('stream-tool-first-summary-kind')).toHaveTextContent('mcp_capability')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('malformed toolSummary payload')
  })

  it('fails closed on malformed skill events and preserves the last truthful skill lane', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'skill',
          runId: 'run-project-1',
          sequence: 1,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          toolSummary: null,
          skillId: 'find-skills',
          skillStage: 'install',
          skillResult: 'succeeded',
          skillSource: {
            repo: 'vercel-labs/skills',
            path: 'skills/find-skills',
            reference: 'main',
            treeHash: '0123456789abcdef0123456789abcdef01234567',
          },
          skillCacheStatus: 'refreshed',
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'Installed autonomous skill `find-skills` from the cached vercel-labs/skills tree.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-skill-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('stream-skill-first-id')).toHaveTextContent('find-skills')

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'skill',
          runId: 'run-project-1',
          sequence: 2,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          toolSummary: null,
          skillId: 'find-skills',
          skillStage: null,
          skillResult: 'failed',
          skillSource: {
            repo: 'vercel-labs/skills',
            path: 'skills/find-skills',
            reference: 'main',
            treeHash: '0123456789abcdef0123456789abcdef01234567',
          },
          skillCacheStatus: 'hit',
          skillDiagnostic: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: 'Malformed skill event missing a lifecycle stage.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:01Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('stream-skill-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-skill-first-id')).toHaveTextContent('find-skills')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent(
      'Xero received a runtime skill item without a skillStage value.',
    )
  })

  it('fails closed on malformed stream items and preserves the last valid projection', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'transcript',
          runId: 'run-project-1',
          sequence: 1,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Recovered transcript event.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-item-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('stream-status')).toHaveTextContent('live')
    expect(screen.getByTestId('stream-last-sequence')).toHaveTextContent('1')

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'transcript',
          runId: 'run-project-1',
          sequence: 0,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Malformed replay event.',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId: null,
          boundaryId: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:30:01Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('1')
    expect(screen.getByTestId('stream-last-sequence')).toHaveTextContent('1')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('non-monotonic runtime stream sequence 0')
  })

  it('starts bounded blocked-checkpoint sync polling, dedupes repeated action-required events, and stops after the boundary clears', async () => {
    const setup = createMockAdapter({
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
      },
    })

    const actionId = 'flow:flow-1:run:run-project-1:boundary:boundary-1:terminal_input_required'
    let snapshotMode: 'base' | 'blocked' | 'resolved' = 'base'
    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'Xero' : 'orchestra')

      if (projectId !== 'project-1') {
        return snapshot
      }

      if (snapshotMode === 'blocked') {
        return {
          ...snapshot,
          approvalRequests: [
            {
              actionId,
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'terminal_input_required',
              title: 'Terminal input required',
              detail: 'Provide terminal input to continue this run.',
              userAnswer: null,
              status: 'pending',
              decisionNote: null,
              createdAt: '2026-04-16T13:00:00Z',
              updatedAt: '2026-04-16T13:00:00Z',
              resolvedAt: null,
            },
          ],
          resumeHistory: [
            makeResumeHistoryEntry({
              id: 4,
              actionId,
              status: 'failed',
              summary: 'Operator resume failed and is waiting for corrected terminal input.',
            }),
          ],
        }
      }

      return snapshot
    })
    vi.mocked(setup.getAutonomousRun).mockImplementation(async (projectId: string) => {
      if (projectId !== 'project-1') {
        return makeRecoveredAutonomousRunState(projectId)
      }

      return snapshotMode === 'blocked'
        ? makeBlockedAutonomousRunState(projectId)
        : makeRecoveredAutonomousRunState(projectId)
    })

    render(<Harness adapter={setup.adapter} />)

    const project1SyncCount = () =>
      vi
        .mocked(setup.syncNotificationAdapters)
        .mock.calls.filter(([projectId]) => projectId === 'project-1').length

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('0')
    expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('false')

    snapshotMode = 'blocked'

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'action_required',
          runId: 'run-project-1',
          sequence: 5,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId,
          boundaryId: 'boundary-1',
          actionType: 'terminal_input_required',
          title: 'Terminal input required',
          detail: 'Provide terminal input to continue this run.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:00:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    await waitFor(() => expect(screen.getByTestId('refresh-source')).toHaveTextContent('runtime_stream:action_required'))
    await waitFor(() => expect(screen.getByTestId('resume-history-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('false')
    expect(screen.getByTestId('sync-polling-action-id')).toHaveTextContent('none')
    expect(screen.getByTestId('sync-polling-boundary-id')).toHaveTextContent('none')
    expect(screen.getByTestId('latest-resume-status')).toHaveTextContent('failed')
    expect(screen.getByTestId('latest-resume-source-action-id')).toHaveTextContent(actionId)
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('failed')

    const syncCallsAfterImmediateRefresh = project1SyncCount()

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'action_required',
          runId: 'run-project-1',
          sequence: 6,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId,
          boundaryId: 'boundary-1',
          actionType: 'terminal_input_required',
          title: 'Terminal input required',
          detail: 'Provide terminal input to continue this run.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:00:01Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('stream-action-required-count')).toHaveTextContent('1'))
    expect(project1SyncCount()).toBe(syncCallsAfterImmediateRefresh)

    snapshotMode = 'resolved'

    await new Promise((resolve) => setTimeout(resolve, 650))
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('false')

    const syncCallsAfterBoundaryClear = project1SyncCount()
    await new Promise((resolve) => setTimeout(resolve, 650))
    expect(project1SyncCount()).toBe(syncCallsAfterBoundaryClear)
  })

  it('does not start blocked-checkpoint notification polling from normal runtime boundaries', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')],
      },
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'authenticated',
          sessionId: 'session-1',
          flowId: 'flow-1',
          accountId: 'acct-1',
          lastErrorCode: null,
          lastError: null,
        }),
        'project-2': makeRuntimeSession('project-2', {
          phase: 'authenticated',
          sessionId: 'session-2',
          flowId: 'flow-2',
          accountId: 'acct-2',
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1', { runId: 'run-project-1' }),
        'project-2': makeRuntimeRun('project-2', { runId: 'run-project-2' }),
      },
    })

    const actionId = 'flow:flow-1:run:run-project-1:boundary:boundary-1:terminal_input_required'
    let project1Blocked = false
    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'Xero' : 'orchestra')

      if (projectId === 'project-1' && project1Blocked) {
        return {
          ...snapshot,
          approvalRequests: [
            {
              actionId,
              sessionId: 'session-1',
              flowId: 'flow-1',
              actionType: 'terminal_input_required',
              title: 'Terminal input required',
              detail: 'Provide terminal input to continue this run.',
              userAnswer: null,
              status: 'pending',
              decisionNote: null,
              createdAt: '2026-04-16T13:00:00Z',
              updatedAt: '2026-04-16T13:00:00Z',
              resolvedAt: null,
            },
          ],
        }
      }

      return snapshot
    })
    vi.mocked(setup.getAutonomousRun).mockImplementation(async (projectId: string) => {
      if (projectId === 'project-1' && project1Blocked) {
        return makeBlockedAutonomousRunState(projectId)
      }

      return makeRecoveredAutonomousRunState(projectId)
    })

    render(<Harness adapter={setup.adapter} />)

    const project1SyncCount = () =>
      vi
        .mocked(setup.syncNotificationAdapters)
        .mock.calls.filter(([projectId]) => projectId === 'project-1').length

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))
    project1Blocked = true

    act(() => {
      setup.emitRuntimeStream(0, {
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        runId: 'run-project-1',
        sessionId: 'session-1',
        flowId: 'flow-1',
        subscribedItemKinds: ['transcript', 'tool', 'activity', 'action_required', 'complete', 'failure'],
        item: {
          kind: 'action_required',
          runId: 'run-project-1',
          sequence: 5,
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionId,
          boundaryId: 'boundary-1',
          actionType: 'terminal_input_required',
          title: 'Terminal input required',
          detail: 'Provide terminal input to continue this run.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-16T13:00:00Z',
        },
      })
    })

    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('false')

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('false')

    const project1SyncCallsAfterSwitch = project1SyncCount()
    await new Promise((resolve) => setTimeout(resolve, 650))
    expect(project1SyncCount()).toBe(project1SyncCallsAfterSwitch)
  })

  it('loads route health, then keeps the last truthful route list when refresh fails', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Xero'),
          approvalRequests: [makeGateLinkedPendingApproval(actionId)],
          resumeHistory: [
            makeResumeHistoryEntry({
              id: 8,
              actionId,
              status: 'failed',
              summary: 'Operator resume failed while waiting for route diagnostics.',
            }),
          ],
        },
      },
      notificationRoutes: {
        'project-1': [
          makeNotificationRoute({
            projectId: 'project-1',
            routeId: 'telegram-primary',
            routeKind: 'telegram',
            routeTarget: '@ops-room',
            enabled: true,
          }),
          makeNotificationRoute({
            projectId: 'project-1',
            routeId: 'discord-fallback',
            routeKind: 'discord',
            routeTarget: '1234567890',
            enabled: true,
          }),
        ],
      },
      notificationDispatches: {
        'project-1': [
          makeNotificationDispatch({
            id: 31,
            projectId: 'project-1',
            actionId,
            routeId: 'telegram-primary',
            status: 'failed',
            attemptCount: 2,
            lastAttemptAt: '2026-04-16T13:01:00Z',
            lastErrorCode: 'notification_adapter_transport_failed',
            lastErrorMessage: 'Telegram returned 502.',
            updatedAt: '2026-04-16T13:01:00Z',
          }),
        ],
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('route-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('telegram-channel-health')).toHaveTextContent('degraded')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(screen.getByTestId('latest-resume-status')).toHaveTextContent('failed')
    expect(screen.getByTestId('trust-state')).toHaveTextContent('degraded')
    expect(screen.getByTestId('trust-credentials-state')).toHaveTextContent('healthy')
    expect(screen.getByTestId('trust-routes-state')).toHaveTextContent('degraded')

    setup.listNotificationRoutes.mockRejectedValueOnce(
      new XeroDesktopError({
        code: 'notification_route_query_failed',
        errorClass: 'retryable',
        message: 'Xero could not load notification routes for this project.',
        retryable: true,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Refresh notification routes' }))

    await waitFor(() =>
      expect(screen.getByTestId('route-load-error')).toHaveTextContent(
        'Xero could not load notification routes for this project.',
      ),
    )
    expect(screen.getByTestId('route-load-status')).not.toHaveTextContent('loading')
    expect(screen.getByTestId('route-count')).toHaveTextContent('2')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(screen.getByTestId('latest-resume-status')).toHaveTextContent('failed')
    expect(screen.getByTestId('trust-state')).toHaveTextContent('degraded')
    expect(screen.getByTestId('trust-routes-state')).toHaveTextContent('degraded')
    expect(screen.getByTestId('trust-projection-error')).toHaveTextContent('none')
  })

  it('composes credential-readiness trust state and preserves the last-known-good snapshot on malformed route payloads', async () => {
    const setup = createMockAdapter({
      notificationRoutes: {
        'project-1': [
          makeNotificationRoute({
            projectId: 'project-1',
            routeId: 'telegram-primary',
            routeKind: 'telegram',
            routeTarget: '@ops-room',
            enabled: true,
            credentialReadiness: {
              hasBotToken: true,
              hasChatId: true,
              hasWebhookUrl: false,
              ready: true,
              status: 'ready',
              diagnostic: null,
            },
          }),
          makeNotificationRoute({
            projectId: 'project-1',
            routeId: 'discord-fallback',
            routeKind: 'discord',
            routeTarget: '1234567890',
            enabled: true,
            credentialReadiness: {
              hasBotToken: false,
              hasChatId: false,
              hasWebhookUrl: true,
              ready: false,
              status: 'missing',
              diagnostic: {
                code: 'notification_adapter_credentials_missing',
                message: 'Xero is missing app-local Discord botToken credentials.',
                retryable: false,
              },
            },
          }),
        ],
      },
      notificationSyncResponses: {
        'project-1': makeNotificationSyncResponse('project-1', {
          dispatch: {
            projectId: 'project-1',
            pendingCount: 0,
            attemptedCount: 1,
            sentCount: 1,
            failedCount: 0,
            attemptLimit: 64,
            attemptsTruncated: false,
            attempts: [],
            errorCodeCounts: [],
          },
          replies: {
            projectId: 'project-1',
            routeCount: 2,
            polledRouteCount: 2,
            messageCount: 1,
            acceptedCount: 1,
            rejectedCount: 0,
            attemptLimit: 256,
            attemptsTruncated: false,
            attempts: [],
            errorCodeCounts: [],
          },
          syncedAt: '2026-04-17T03:30:00Z',
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('route-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('route-discord-credential-status')).toHaveTextContent('missing')
    expect(screen.getByTestId('route-discord-credential-code')).toHaveTextContent(
      'notification_adapter_credentials_missing',
    )
    expect(screen.getByTestId('trust-state')).toHaveTextContent('degraded')
    expect(screen.getByTestId('trust-credentials-state')).toHaveTextContent('degraded')
    expect(screen.getByTestId('trust-ready-credential-count')).toHaveTextContent('1')
    expect(screen.getByTestId('trust-missing-credential-count')).toHaveTextContent('1')

    setup.listNotificationRoutes.mockResolvedValueOnce({
      routes: [
        {
          ...makeNotificationRoute({
            projectId: 'project-1',
            routeId: 'telegram-primary',
            routeKind: 'telegram',
            routeTarget: '@ops-room',
            enabled: true,
          }),
          credentialReadiness: {
            hasBotToken: true,
            hasChatId: true,
            hasWebhookUrl: false,
            ready: false,
            status: 'ready',
            diagnostic: null,
          } as never,
        },
        makeNotificationRoute({
          projectId: 'project-1',
          routeId: 'discord-fallback',
          routeKind: 'discord',
          routeTarget: '1234567890',
          enabled: true,
        }),
      ],
    })

    fireEvent.click(screen.getByRole('button', { name: 'Refresh notification routes' }))

    await waitFor(() => expect(screen.getByTestId('route-load-status')).toHaveTextContent('ready'))
    await waitFor(() => expect(screen.getByTestId('trust-projection-error')).not.toHaveTextContent('none'))
    expect(screen.getByTestId('trust-state')).toHaveTextContent('degraded')
    expect(screen.getByTestId('trust-ready-credential-count')).toHaveTextContent('1')
    expect(screen.getByTestId('trust-missing-credential-count')).toHaveTextContent('1')
  })

  it('does not run notification adapter sync during selected-project refreshes', async () => {
    const setup = createMockAdapter({
      notificationSyncResponses: {
        'project-1': {
          projectId: 'project-1',
          dispatch: {
            projectId: 'project-1',
            pendingCount: 2,
            attemptedCount: 2,
            sentCount: 1,
            failedCount: 1,
            attemptLimit: 64,
            attemptsTruncated: false,
            attempts: [],
            errorCodeCounts: [{ code: 'notification_adapter_transport_timeout', count: 1 }],
          },
          replies: {
            projectId: 'project-1',
            routeCount: 2,
            polledRouteCount: 2,
            messageCount: 2,
            acceptedCount: 1,
            rejectedCount: 1,
            attemptLimit: 256,
            attemptsTruncated: false,
            attempts: [],
            errorCodeCounts: [{ code: 'notification_reply_already_claimed', count: 1 }],
          },
          syncedAt: '2026-04-17T03:00:00Z',
        },
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(setup.syncNotificationAdapters).not.toHaveBeenCalled()
    expect(screen.getByTestId('sync-dispatch-attempted')).toHaveTextContent('0')
    expect(screen.getByTestId('sync-reply-accepted')).toHaveTextContent('0')
    expect(screen.getByTestId('sync-reply-rejected')).toHaveTextContent('0')
    expect(screen.getByTestId('sync-error')).toHaveTextContent('none')
  })

  it('keeps broker rows visible without running notification adapter sync on retry', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      notificationDispatches: {
        'project-1': [
          makeNotificationDispatch({
            id: 41,
            projectId: 'project-1',
            actionId,
            routeId: 'discord-fallback',
            status: 'pending',
            updatedAt: '2026-04-16T13:20:00Z',
          }),
        ],
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('sync-dispatch-attempted')).toHaveTextContent('0')
    expect(screen.getByTestId('broker-dispatch-count')).toHaveTextContent('1')

    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() => expect(screen.getByTestId('sync-error')).toHaveTextContent('none'))
    expect(setup.syncNotificationAdapters).not.toHaveBeenCalled()
    expect(screen.getByTestId('sync-dispatch-attempted')).toHaveTextContent('0')
    expect(screen.getByTestId('broker-dispatch-count')).toHaveTextContent('1')
  })

  it('disables one route without mutating unrelated rows or approval state', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Xero'),
          approvalRequests: [makeGateLinkedPendingApproval(actionId)],
        },
      },
      notificationRoutes: {
        'project-1': [
          makeNotificationRoute({
            projectId: 'project-1',
            routeId: 'telegram-primary',
            routeKind: 'telegram',
            routeTarget: '@ops-room',
            enabled: true,
          }),
          makeNotificationRoute({
            projectId: 'project-1',
            routeId: 'discord-fallback',
            routeKind: 'discord',
            routeTarget: '1234567890',
            enabled: true,
          }),
        ],
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('route-telegram-enabled')).toHaveTextContent('true'))
    expect(screen.getByTestId('route-discord-enabled')).toHaveTextContent('true')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')

    fireEvent.click(screen.getByRole('button', { name: 'Disable telegram route' }))

    await waitFor(() => expect(screen.getByTestId('route-telegram-enabled')).toHaveTextContent('false'))
    expect(screen.getByTestId('route-discord-enabled')).toHaveTextContent('true')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(setup.upsertNotificationRoute).toHaveBeenCalledWith(
      expect.objectContaining({
        routeId: 'telegram-primary',
        routeKind: 'telegram',
        enabled: false,
      }),
    )
  })

  it('ignores wrong-project runtime-run updates and preserves the last truthful view on malformed events', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      listProjects: {
        projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')],
      },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Xero'),
          approvalRequests: [makeGateLinkedApprovedApproval(actionId)],
          resumeHistory: [
            makeResumeHistoryEntry({
              id: 7,
              actionId,
              status: 'failed',
              summary: 'Operator resume failed while waiting for refreshed runtime metadata.',
            }),
          ],
        },
        'project-2': makeSnapshot('project-2', 'orchestra'),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1'),
        'project-2': makeRuntimeRun('project-2', { runId: 'run-project-2' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('1')
    expect(screen.getByTestId('latest-resume-status')).toHaveTextContent('failed')
    expect(screen.getByTestId('first-approval-action-id')).toHaveTextContent(actionId)
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('failed')

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-2',
        run: makeRuntimeRun('project-2', { runId: 'run-project-2' }),
      })
    })

    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('startup')
    expect(screen.getByTestId('latest-resume-status')).toHaveTextContent('failed')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('failed')

    act(() => {
      setup.emitRuntimeRunUpdatedError(
        new XeroDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: 'Event runtime_run:updated returned an unexpected payload shape.',
          retryable: false,
        }),
      )
    })

    await waitFor(() =>
      expect(screen.getByTestId('error')).toHaveTextContent(
        'Event runtime_run:updated returned an unexpected payload shape.',
      ),
    )
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('1')
    expect(screen.getByTestId('latest-resume-status')).toHaveTextContent('failed')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('failed')
  })
})
