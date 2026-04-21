import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import {
  type ImportRepositoryResponseDto,
  type ListNotificationDispatchesResponseDto,
  type ListNotificationRoutesResponseDto,
  type ListProjectsResponseDto,
  type ProjectSnapshotResponseDto,
  type ProjectUpdatedPayloadDto,
  type RepositoryDiffResponseDto,
  type RepositoryStatusChangedPayloadDto,
  type RepositoryStatusResponseDto,
  type ResolveOperatorActionResponseDto,
  type ResumeOperatorRunResponseDto,
  type AutonomousRunStateDto,
  type RuntimeRunDto,
  type RuntimeRunUpdatedPayloadDto,
  type RuntimeSessionDto,
  type RuntimeSettingsDto,
  type RuntimeStreamEventDto,
  type SubscribeRuntimeStreamResponseDto,
  type RuntimeUpdatedPayloadDto,
} from '@/src/lib/cadence-model'
import { CadenceDesktopError, type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import { useCadenceDesktopState } from '@/src/features/cadence/use-cadence-desktop-state'

function makeProjectSummary(id: string, name: string) {
  return {
    id,
    name,
    description: `${name} description`,
    milestone: `M-${id}`,
    totalPhases: 2,
    completedPhases: id === 'project-1' ? 1 : 0,
    activePhase: id === 'project-1' ? 2 : 1,
    branch: id === 'project-1' ? 'main' : null,
    runtime: id === 'project-1' ? 'codex' : null,
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
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    phases: [
      {
        id: 1,
        name: 'Import',
        description: 'Import repo',
        status: id === 'project-1' ? 'complete' : 'active',
        currentStep: id === 'project-1' ? null : 'execute',
        taskCount: 2,
        completedTasks: id === 'project-1' ? 2 : 1,
        summary: id === 'project-1' ? 'Done' : null,
      },
    ],
    lifecycle: {
      stages: [
        {
          stage: 'discussion',
          nodeId: 'workflow-discussion',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-15T17:59:00Z',
        },
        {
          stage: 'research',
          nodeId: 'workflow-research',
          status: id === 'project-1' ? 'active' : 'pending',
          actionRequired: id === 'project-1',
          lastTransitionAt: '2026-04-15T18:00:00Z',
        },
      ],
    },
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
  }
}

function makeStatus(id: string, branchName: string): RepositoryStatusResponseDto {
  return {
    repository: {
      id: `repo-${id}`,
      projectId: id,
      rootPath: `/tmp/${id}`,
      displayName: id,
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    branch: {
      name: branchName,
      headSha: null,
      detached: false,
    },
    entries: [
      {
        path: 'client/src/App.tsx',
        staged: 'modified',
        unstaged: null,
        untracked: false,
      },
    ],
    hasStagedChanges: true,
    hasUnstagedChanges: false,
    hasUntrackedChanges: false,
  }
}

function makeDiff(id: string, scope: 'staged' | 'unstaged' | 'worktree', patch = ''): RepositoryDiffResponseDto {
  return {
    repository: {
      id: `repo-${id}`,
      projectId: id,
      rootPath: `/tmp/${id}`,
      displayName: id,
      branch: null,
      headSha: null,
      isGitRepo: true,
    },
    scope,
    patch,
    truncated: false,
    baseRevision: null,
  }
}

function makeRuntimeSession(
  projectId: string,
  overrides: Partial<RuntimeSessionDto> = {},
): RuntimeSessionDto {
  return {
    projectId,
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
    updatedAt: '2026-04-13T19:33:32Z',
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

function makeRuntimeRun(projectId: string, overrides: Partial<RuntimeRunDto> = {}): RuntimeRunDto {
  return {
    projectId,
    runId: `run-${projectId}`,
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
        kind: 'state',
        summary: 'Recovered repository context before reconnecting the live feed.',
        createdAt: '2026-04-15T20:00:06Z',
      },
    ],
    ...overrides,
  }
}

function makeAutonomousRunState(projectId: string, runId = `auto-${projectId}`): AutonomousRunStateDto {
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
      startedAt: '2026-04-16T20:00:01Z',
      finishedAt: null,
      updatedAt: '2026-04-16T20:00:06Z',
      lastErrorCode: null,
      lastError: null,
    },
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
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
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
      overrides.subscribedItemKinds ?? ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      runId,
      sequence,
      ...item,
    },
  }
}

function createMockAdapter(options?: {
  listProjects?: ListProjectsResponseDto
  snapshots?: Record<string, ProjectSnapshotResponseDto>
  statuses?: Record<string, RepositoryStatusResponseDto>
  runtimeSessions?: Record<string, RuntimeSessionDto>
  runtimeRuns?: Record<string, RuntimeRunDto | null>
  runtimeSettings?: RuntimeSettingsDto
  autonomousStates?: Record<string, AutonomousRunStateDto | null>
  notificationDispatches?: Record<string, ListNotificationDispatchesResponseDto['dispatches']>
  notificationRoutes?: Record<string, ListNotificationRoutesResponseDto['routes']>
  notificationDispatchErrors?: Record<string, Error>
  diffs?: Partial<Record<'staged' | 'unstaged' | 'worktree', RepositoryDiffResponseDto>>
  importResponse?: ImportRepositoryResponseDto
  subscribeErrors?: Record<string, CadenceDesktopError>
  subscribeResponses?: Record<string, SubscribeRuntimeStreamResponseDto>
}) {
  let projectUpdatedHandler: ((payload: ProjectUpdatedPayloadDto) => void) | null = null
  let repositoryStatusChangedHandler: ((payload: RepositoryStatusChangedPayloadDto) => void) | null = null
  let runtimeUpdatedHandler: ((payload: RuntimeUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null
  let projectUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let repositoryStatusErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let runtimeUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let runtimeRunUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null

  const snapshots = options?.snapshots ?? {
    'project-1': makeSnapshot('project-1', 'Cadence'),
    'project-2': makeSnapshot('project-2', 'orchestra'),
  }
  const statuses = options?.statuses ?? {
    'project-1': makeStatus('project-1', 'main'),
    'project-2': makeStatus('project-2', 'feature/import'),
  }
  const runtimeSessions = options?.runtimeSessions ?? {
    'project-1': makeRuntimeSession('project-1'),
    'project-2': makeRuntimeSession('project-2', {
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
    }),
  }
  const runtimeRuns = options?.runtimeRuns ?? {
    'project-1': makeRuntimeRun('project-1'),
    'project-2': null,
  }
  const autonomousStates = options?.autonomousStates ?? {
    'project-1': makeAutonomousRunState('project-1'),
    'project-2': null,
  }
  const notificationDispatches = options?.notificationDispatches ?? {
    'project-1': [],
    'project-2': [],
  }
  const notificationRoutes = options?.notificationRoutes ?? {
    'project-1': [],
    'project-2': [],
  }
  const currentRuntimeSettings = {
    value: options?.runtimeSettings ?? makeRuntimeSettings(),
  }
  const notificationDispatchErrors = options?.notificationDispatchErrors ?? {}

  let listedProjects = (options?.listProjects?.projects ?? [makeProjectSummary('project-1', 'Cadence')]).map((project) => ({
    ...project,
  }))

  const streamSubscriptions: Array<{
    projectId: string
    handler: (payload: RuntimeStreamEventDto) => void
    onError: ((error: CadenceDesktopError) => void) | null
    unsubscribe: ReturnType<typeof vi.fn>
  }> = []

  const projectUnlisten = vi.fn()
  const repositoryUnlisten = vi.fn()
  const runtimeUnlisten = vi.fn()
  const runtimeRunUnlisten = vi.fn()
  const pickRepositoryFolder = vi.fn(async (): Promise<string | null> => '/tmp/imported')
  const importRepository = vi.fn(async () => {
    const response =
      options?.importResponse ?? {
        project: makeProjectSummary('project-2', 'orchestra'),
        repository: {
          id: 'repo-project-2',
          projectId: 'project-2',
          rootPath: '/tmp/orchestra',
          displayName: 'orchestra',
          branch: 'feature/import',
          headSha: null,
          isGitRepo: true,
        },
      }

    listedProjects = [...listedProjects.filter((project) => project.id !== response.project.id), response.project]
    return response
  })
  const listProjects = vi.fn(async () => ({ projects: listedProjects }))
  const removeProject = vi.fn(async (projectId: string) => {
    listedProjects = listedProjects.filter((project) => project.id !== projectId)
    return { projects: listedProjects }
  })
  const getProjectSnapshot = vi.fn(async (projectId: string) => snapshots[projectId])
  const getRepositoryStatus = vi.fn(async (projectId: string) => statuses[projectId])
  const getRepositoryDiff = vi.fn(async (_projectId: string, scope: 'staged' | 'unstaged' | 'worktree') => {
    const configuredDiff = options?.diffs?.[scope]
    return configuredDiff ?? makeDiff('project-1', scope, scope === 'unstaged' ? 'diff --git a/file b/file\n+change' : '')
  })
  const getRuntimeRun = vi.fn(async (projectId: string): Promise<RuntimeRunDto | null> => runtimeRuns[projectId] ?? null)
  const getAutonomousRun = vi.fn(async (projectId: string): Promise<AutonomousRunStateDto> =>
    autonomousStates[projectId] ?? { run: null, unit: null },
  )
  const getRuntimeSession = vi.fn(async (projectId: string) => runtimeSessions[projectId])
  const getRuntimeSettings = vi.fn(async () => currentRuntimeSettings.value)
  const upsertRuntimeSettings = vi.fn(async (request: {
    providerId: RuntimeSettingsDto['providerId']
    modelId: string
    openrouterApiKey?: string | null
  }) => {
    const nextKeyConfigured =
      request.openrouterApiKey === undefined || request.openrouterApiKey === null
        ? currentRuntimeSettings.value.openrouterApiKeyConfigured
        : request.openrouterApiKey.trim().length > 0

    currentRuntimeSettings.value = {
      providerId: request.providerId,
      modelId: request.modelId,
      openrouterApiKeyConfigured: nextKeyConfigured,
    }

    return currentRuntimeSettings.value
  })
  const listNotificationDispatches = vi.fn(async (projectId: string) => {
    const error = notificationDispatchErrors[projectId]
    if (error) {
      throw error
    }

    return {
      dispatches: notificationDispatches[projectId] ?? [],
    }
  })
  const listNotificationRoutes = vi.fn(async (projectId: string) => ({
    routes: notificationRoutes[projectId] ?? [],
  }))
  const upsertNotificationRoute = vi.fn(async (request: {
    projectId: string
    routeId: string
    routeKind: 'telegram' | 'discord'
    routeTarget: string
    enabled: boolean
    metadataJson?: string | null
  }) => {
    const now = '2026-04-16T14:00:00Z'
    const currentRoutes = notificationRoutes[request.projectId] ?? []
    const nextRoute = {
      projectId: request.projectId,
      routeId: request.routeId,
      routeKind: request.routeKind,
      routeTarget: request.routeTarget,
      enabled: request.enabled,
      metadataJson: request.metadataJson ?? null,
      createdAt:
        currentRoutes.find((route) => route.routeId === request.routeId)?.createdAt ?? now,
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
  const startOpenAiLogin = vi.fn(async (projectId: string) =>
    makeRuntimeSession(projectId, {
      sessionId: null,
      phase: 'awaiting_browser_callback',
      lastErrorCode: null,
      lastError: null,
    }),
  )
  const submitOpenAiCallback = vi.fn(async (projectId: string, flowId: string) =>
    makeRuntimeSession(projectId, { flowId, phase: 'authenticated' }),
  )
  const startAutonomousRun = vi.fn(async (projectId: string) => {
    const nextState = makeAutonomousRunState(projectId)
    autonomousStates[projectId] = nextState
    return nextState
  })
  const startRuntimeRun = vi.fn(async (projectId: string) => runtimeRuns[projectId] ?? makeRuntimeRun(projectId))
  const startRuntimeSession = vi.fn(async (projectId: string) => makeRuntimeSession(projectId))
  const cancelAutonomousRun = vi.fn(async (projectId: string, runId: string) => {
    const nextState = makeAutonomousRunState(projectId, runId)
    nextState.run = {
      ...nextState.run!,
      status: 'cancelled',
      recoveryState: 'terminal',
      cancelledAt: '2026-04-16T20:10:00Z',
      cancelReason: {
        code: 'operator_cancelled',
        message: 'Operator cancelled the autonomous run from the desktop shell.',
      },
      updatedAt: '2026-04-16T20:10:00Z',
    }
    nextState.unit = {
      ...nextState.unit!,
      status: 'cancelled',
      finishedAt: '2026-04-16T20:10:00Z',
      updatedAt: '2026-04-16T20:10:00Z',
    }
    autonomousStates[projectId] = nextState
    return nextState
  })
  const stopRuntimeRun = vi.fn(async (projectId: string, runId: string): Promise<RuntimeRunDto | null> => {
    const currentRun = runtimeRuns[projectId]
    if (!currentRun) {
      return null
    }

    return {
      ...currentRun,
      runId,
      status: 'stopped',
      stoppedAt: '2026-04-15T20:05:00Z',
      updatedAt: '2026-04-15T20:05:00Z',
    }
  })
  const logoutRuntimeSession = vi.fn(async (projectId: string) =>
    makeRuntimeSession(projectId, {
      flowId: null,
      sessionId: null,
      accountId: null,
      phase: 'idle',
      callbackBound: null,
      authorizationUrl: null,
      redirectUri: null,
      lastErrorCode: null,
      lastError: null,
    }),
  )
  const resolveOperatorAction = vi.fn(
    async (
      projectId: string,
      actionId: string,
      decision: 'approve' | 'reject',
    ): Promise<ResolveOperatorActionResponseDto> => ({
      approvalRequest: {
        actionId,
        sessionId: 'session-1',
        flowId: 'flow-1',
        actionType: 'review_worktree',
        title: 'Review worktree changes',
        detail: 'Inspect the pending repository diff before continuing.',
        status: decision === 'approve' ? 'approved' : 'rejected',
        decisionNote: null,
        createdAt: '2026-04-13T20:01:00Z',
        updatedAt: '2026-04-13T20:02:00Z',
        resolvedAt: '2026-04-13T20:02:00Z',
      },
      verificationRecord: {
        id: 1,
        sourceActionId: actionId,
        status: decision === 'approve' ? 'passed' : 'failed',
        summary: decision === 'approve' ? 'Approved operator action.' : 'Rejected operator action.',
        detail: null,
        recordedAt: '2026-04-13T20:02:01Z',
      },
    }),
  )
  const resumeOperatorRun = vi.fn(
    async (_projectId: string, actionId: string): Promise<ResumeOperatorRunResponseDto> => ({
      approvalRequest: {
        actionId,
        sessionId: 'session-1',
        flowId: 'flow-1',
        actionType: 'review_worktree',
        title: 'Review worktree changes',
        detail: 'Inspect the pending repository diff before continuing.',
        status: 'approved',
        decisionNote: null,
        createdAt: '2026-04-13T20:01:00Z',
        updatedAt: '2026-04-13T20:03:00Z',
        resolvedAt: '2026-04-13T20:02:00Z',
      },
      resumeEntry: {
        id: 1,
        sourceActionId: actionId,
        sessionId: 'session-1',
        status: 'started',
        summary: 'Operator resumed the selected project runtime session.',
        createdAt: '2026-04-13T20:03:30Z',
      },
    }),
  )
  const subscribeRuntimeStream = vi.fn(
    async (
      projectId: string,
      _itemKinds,
      handler: (payload: RuntimeStreamEventDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      const subscribeError = options?.subscribeErrors?.[projectId]
      if (subscribeError) {
        throw subscribeError
      }

      const subscription = {
        projectId,
        handler,
        onError: onError ?? null,
        unsubscribe: vi.fn(),
      }
      streamSubscriptions.push(subscription)

      return {
        response:
          options?.subscribeResponses?.[projectId] ??
          makeStreamResponse(projectId, {
            sessionId: runtimeSessions[projectId]?.sessionId ?? 'session-1',
            flowId: runtimeSessions[projectId]?.flowId ?? null,
            runtimeKind: runtimeSessions[projectId]?.runtimeKind ?? 'openai_codex',
          }),
        unsubscribe: subscription.unsubscribe,
      }
    },
  )
  const onProjectUpdated = vi.fn(
    async (
      handler: (payload: ProjectUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      projectUpdatedHandler = handler
      projectUpdatedErrorHandler = onError ?? null
      return projectUnlisten
    },
  )
  const onRepositoryStatusChanged = vi.fn(
    async (
      handler: (payload: RepositoryStatusChangedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      repositoryStatusChangedHandler = handler
      repositoryStatusErrorHandler = onError ?? null
      return repositoryUnlisten
    },
  )
  const onRuntimeUpdated = vi.fn(
    async (
      handler: (payload: RuntimeUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      runtimeUpdatedHandler = handler
      runtimeUpdatedErrorHandler = onError ?? null
      return runtimeUnlisten
    },
  )
  const onRuntimeRunUpdated = vi.fn(
    async (
      handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      runtimeRunUpdatedHandler = handler
      runtimeRunUpdatedErrorHandler = onError ?? null
      return runtimeRunUnlisten
    },
  )

  const adapter: CadenceDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder,
    importRepository,
    listProjects,
    removeProject,
    getProjectSnapshot,
    getRepositoryStatus,
    getRepositoryDiff,
    listProjectFiles: vi.fn(async (projectId: string) => ({
      projectId,
      root: {
        name: 'root',
        path: '/',
        type: 'folder' as const,
        children: [],
      },
    })),
    readProjectFile: vi.fn(async (projectId: string, path: string) => ({ projectId, path, content: '' })),
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
    deleteProjectEntry: vi.fn(async (projectId: string, path: string) => ({ projectId, path })),
    getAutonomousRun,
    getRuntimeRun,
    getRuntimeSession,
    getRuntimeSettings,
    startOpenAiLogin,
    submitOpenAiCallback,
    startAutonomousRun,
    startRuntimeRun,
    startRuntimeSession,
    cancelAutonomousRun,
    stopRuntimeRun,
    logoutRuntimeSession,
    upsertRuntimeSettings,
    resolveOperatorAction,
    resumeOperatorRun,
    listNotificationRoutes,
    listNotificationDispatches,
    upsertNotificationRoute,
    upsertNotificationRouteCredentials: vi.fn(async () => {
      throw new Error('not used in use-cadence-desktop-state tests')
    }) as never,
    recordNotificationDispatchOutcome: vi.fn(async () => {
      throw new Error('not used in use-cadence-desktop-state tests')
    }) as never,
    submitNotificationReply: vi.fn(async () => {
      throw new Error('not used in use-cadence-desktop-state tests')
    }) as never,
    syncNotificationAdapters: vi.fn(async (projectId: string) => ({
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
    })),
    upsertWorkflowGraph: vi.fn(async () => {
      throw new Error('not used in use-cadence-desktop-state tests')
    }) as never,
    applyWorkflowTransition: vi.fn(async () => {
      throw new Error('not used in use-cadence-desktop-state tests')
    }) as never,
    subscribeRuntimeStream,
    onProjectUpdated,
    onRepositoryStatusChanged,
    onRuntimeUpdated,
    onRuntimeRunUpdated,
  }

  return {
    adapter,
    pickRepositoryFolder,
    importRepository,
    listProjects,
    removeProject,
    getProjectSnapshot,
    getRepositoryStatus,
    getRepositoryDiff,
    getRuntimeRun,
    getRuntimeSession,
    getRuntimeSettings,
    upsertRuntimeSettings,
    listNotificationRoutes,
    listNotificationDispatches,
    upsertNotificationRoute,
    startOpenAiLogin,
    submitOpenAiCallback,
    startRuntimeRun,
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    subscribeRuntimeStream,
    onProjectUpdated,
    onRepositoryStatusChanged,
    onRuntimeUpdated,
    onRuntimeRunUpdated,
    projectUnlisten,
    repositoryUnlisten,
    runtimeUnlisten,
    runtimeRunUnlisten,
    streamSubscriptions,
    emitProjectUpdated(payload: ProjectUpdatedPayloadDto) {
      projectUpdatedHandler?.(payload)
    },
    emitProjectUpdatedError(error: CadenceDesktopError) {
      projectUpdatedErrorHandler?.(error)
    },
    emitRepositoryStatusChanged(payload: RepositoryStatusChangedPayloadDto) {
      repositoryStatusChangedHandler?.(payload)
    },
    emitRepositoryStatusError(error: CadenceDesktopError) {
      repositoryStatusErrorHandler?.(error)
    },
    emitRuntimeUpdated(payload: RuntimeUpdatedPayloadDto) {
      runtimeUpdatedHandler?.(payload)
    },
    emitRuntimeUpdatedError(error: CadenceDesktopError) {
      runtimeUpdatedErrorHandler?.(error)
    },
    emitRuntimeRunUpdated(payload: RuntimeRunUpdatedPayloadDto) {
      runtimeRunUpdatedHandler?.(payload)
    },
    emitRuntimeRunUpdatedError(error: CadenceDesktopError) {
      runtimeRunUpdatedErrorHandler?.(error)
    },
    emitRuntimeStream(index: number, payload: RuntimeStreamEventDto) {
      streamSubscriptions[index]?.handler(payload)
    },
    emitRuntimeStreamError(index: number, error: CadenceDesktopError) {
      streamSubscriptions[index]?.onError?.(error)
    },
  }
}

function Harness({ adapter }: { adapter: CadenceDesktopAdapter }) {
  const state = useCadenceDesktopState({ adapter })

  return (
    <div>
      <div data-testid="loading">{String(state.isLoading)}</div>
      <div data-testid="project-loading">{String(state.isProjectLoading)}</div>
      <div data-testid="active-project">{state.activeProject?.name ?? 'none'}</div>
      <div data-testid="active-project-id">{state.activeProjectId ?? 'none'}</div>
      <div data-testid="branch">{state.activeProject?.branch ?? 'none'}</div>
      <div data-testid="runtime-label">{state.agentView?.runtimeLabel ?? 'none'}</div>
      <div data-testid="runtime-provider-id">{state.agentView?.runtimeSession?.providerId ?? 'none'}</div>
      <div data-testid="selected-provider-id">{state.agentView?.selectedProviderId ?? 'none'}</div>
      <div data-testid="selected-provider-label">{state.agentView?.selectedProviderLabel ?? 'none'}</div>
      <div data-testid="selected-model-id">{state.agentView?.selectedModelId ?? 'none'}</div>
      <div data-testid="provider-mismatch">{String(state.agentView?.providerMismatch ?? false)}</div>
      <div data-testid="auth-phase">{state.agentView?.authPhase ?? 'none'}</div>
      <div data-testid="auth-phase-label">{state.agentView?.authPhaseLabel ?? 'none'}</div>
      <div data-testid="session-label">{state.agentView?.runtimeSession?.sessionLabel ?? 'none'}</div>
      <div data-testid="account-label">{state.agentView?.runtimeSession?.accountLabel ?? 'none'}</div>
      <div data-testid="session-reason">{state.agentView?.sessionUnavailableReason ?? 'none'}</div>
      <div data-testid="messages-reason">{state.agentView?.messagesUnavailableReason ?? 'none'}</div>
      <div data-testid="stream-status">{state.agentView?.runtimeStreamStatus ?? 'idle'}</div>
      <div data-testid="stream-status-label">{state.agentView?.runtimeStreamStatusLabel ?? 'No live stream'}</div>
      <div data-testid="stream-run-id">{state.agentView?.runtimeStream?.runId ?? 'none'}</div>
      <div data-testid="stream-last-sequence">{String(state.agentView?.runtimeStream?.lastSequence ?? 0)}</div>
      <div data-testid="stream-error">{state.agentView?.runtimeStreamError?.message ?? 'none'}</div>
      <div data-testid="stream-item-count">{String(state.agentView?.runtimeStreamItems?.length ?? 0)}</div>
      <div data-testid="stream-skill-count">{String(state.agentView?.skillItems?.length ?? 0)}</div>
      <div data-testid="stream-skill-first-id">{state.agentView?.skillItems?.[0]?.skillId ?? 'none'}</div>
      <div data-testid="stream-skill-first-stage">{state.agentView?.skillItems?.[0]?.stage ?? 'none'}</div>
      <div data-testid="stream-skill-first-result">{state.agentView?.skillItems?.[0]?.result ?? 'none'}</div>
      <div data-testid="activity-count">{String(state.agentView?.activityItems?.length ?? 0)}</div>
      <div data-testid="action-required-count">{String(state.agentView?.actionRequiredItems?.length ?? 0)}</div>
      <div data-testid="action-required-title">{state.agentView?.actionRequiredItems?.[0]?.title ?? 'none'}</div>
      <div data-testid="approval-count">{String(state.agentView?.approvalRequests.length ?? 0)}</div>
      <div data-testid="pending-approval-count">{String(state.agentView?.pendingApprovalCount ?? 0)}</div>
      <div data-testid="latest-decision-status">{state.agentView?.latestDecisionOutcome?.status ?? 'none'}</div>
      <div data-testid="verification-count">{String(state.executionView?.verificationRecords.length ?? 0)}</div>
      <div data-testid="resume-history-count">{String(state.executionView?.resumeHistory.length ?? 0)}</div>
      <div data-testid="operator-action-status">{state.operatorActionStatus}</div>
      <div data-testid="pending-operator-action-id">{state.pendingOperatorActionId ?? 'none'}</div>
      <div data-testid="operator-action-error-code">{state.operatorActionError?.code ?? 'none'}</div>
      <div data-testid="operator-action-error-message">{state.operatorActionError?.message ?? 'none'}</div>
      <div data-testid="status-count">{String(state.repositoryStatus?.statusCount ?? 0)}</div>
      <div data-testid="error">{state.errorMessage ?? 'none'}</div>
      <div data-testid="runtime-settings-provider-id">{state.runtimeSettings?.providerId ?? 'none'}</div>
      <div data-testid="runtime-settings-model-id">{state.runtimeSettings?.modelId ?? 'none'}</div>
      <div data-testid="runtime-settings-key-configured">{String(state.runtimeSettings?.openrouterApiKeyConfigured ?? false)}</div>
      <div data-testid="runtime-settings-load-status">{state.runtimeSettingsLoadStatus}</div>
      <div data-testid="runtime-settings-load-error-code">{state.runtimeSettingsLoadError?.code ?? 'none'}</div>
      <div data-testid="runtime-settings-load-error-message">{state.runtimeSettingsLoadError?.message ?? 'none'}</div>
      <div data-testid="runtime-settings-save-status">{state.runtimeSettingsSaveStatus}</div>
      <div data-testid="runtime-settings-save-error-code">{state.runtimeSettingsSaveError?.code ?? 'none'}</div>
      <div data-testid="runtime-settings-save-error-message">{state.runtimeSettingsSaveError?.message ?? 'none'}</div>
      <div data-testid="refresh-source">{state.refreshSource ?? 'none'}</div>
      <div data-testid="project-count">{String(state.projects.length)}</div>
      <div data-testid="workflow-has-phases">{String(state.workflowView?.hasPhases ?? false)}</div>
      <div data-testid="workflow-overall-percent">{String(state.workflowView?.overallPercent ?? 0)}</div>
      <div data-testid="workflow-active-phase">{state.workflowView?.activePhase?.name ?? 'none'}</div>
      <div data-testid="workflow-has-lifecycle">{String(state.workflowView?.hasLifecycle ?? false)}</div>
      <div data-testid="workflow-lifecycle-percent">{String(state.workflowView?.lifecyclePercent ?? 0)}</div>
      <div data-testid="workflow-active-lifecycle-stage">{state.workflowView?.activeLifecycleStage?.stage ?? 'none'}</div>
      <div data-testid="workflow-lifecycle-action-required">{String(state.workflowView?.actionRequiredLifecycleCount ?? 0)}</div>
      <div data-testid="execution-status-count">{String(state.executionView?.statusCount ?? 0)}</div>
      <div data-testid="execution-branch">{state.executionView?.branchLabel ?? 'none'}</div>
      <div data-testid="active-diff-scope">{state.activeDiffScope}</div>
      <div data-testid="diff-status">{state.activeRepositoryDiff.status}</div>
      <div data-testid="diff-error">{state.activeRepositoryDiff.errorMessage ?? 'none'}</div>
      <div data-testid="diff-patch">{state.activeRepositoryDiff.diff?.patch ?? 'none'}</div>
      <button onClick={() => void state.selectProject('project-2')} type="button">
        Select project 2
      </button>
      <button onClick={() => void state.importProject()} type="button">
        Import project
      </button>
      <button onClick={() => void state.removeProject('project-1')} type="button">
        Remove project 1
      </button>
      <button onClick={() => void state.showRepositoryDiff('unstaged')} type="button">
        Load unstaged diff
      </button>
      <button onClick={() => void state.retry()} type="button">
        Retry state
      </button>
      <button
        onClick={() => {
          void state.refreshRuntimeSettings({ force: true }).catch(() => undefined)
        }}
        type="button"
      >
        Load runtime settings
      </button>
      <button
        onClick={() => {
          void state
            .upsertRuntimeSettings({
              providerId: 'openrouter',
              modelId: 'openai/gpt-4.1-mini',
              openrouterApiKey: 'sk-or-v1-test-secret',
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Save OpenRouter runtime settings
      </button>
      <button
        onClick={() => {
          void state
            .upsertRuntimeSettings({
              providerId: 'openrouter',
              modelId: 'openai/gpt-4.1-mini',
              openrouterApiKey: '   ',
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Clear OpenRouter runtime key
      </button>
      <button
        onClick={() => {
          void state.resolveOperatorAction('flow-1:review_worktree', 'approve').catch(() => undefined)
        }}
        type="button"
      >
        Approve operator action
      </button>
      <button
        onClick={() => {
          void state.resumeOperatorRun('flow-1:review_worktree').catch(() => undefined)
        }}
        type="button"
      >
        Resume operator run
      </button>
    </div>
  )
}

describe('useCadenceDesktopState', () => {
  it('loads the project list, repository truth, and runtime session for the active project', async () => {
    const setup = createMockAdapter()

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project')).toHaveTextContent('Cadence'))

    expect(screen.getByTestId('status-count')).toHaveTextContent('1')
    expect(screen.getByTestId('branch')).toHaveTextContent('main')
    expect(screen.getByTestId('runtime-label')).toHaveTextContent('Openai Codex · Authenticated')
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('authenticated')
    expect(screen.getByTestId('session-label')).toHaveTextContent('session-1')
    expect(screen.getByTestId('account-label')).toHaveTextContent('acct-1')
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('startup')
    expect(setup.listProjects).toHaveBeenCalledTimes(1)
    expect(setup.getProjectSnapshot).toHaveBeenCalledWith('project-1')
    expect(setup.getRepositoryStatus).toHaveBeenCalledWith('project-1')
    expect(setup.getRuntimeSession).toHaveBeenCalledWith('project-1')
  })

  it('reloads the full active snapshot after project:updated so durable operator-loop state stays fresh', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
    })

    let refreshed = false
    setup.getProjectSnapshot.mockImplementation(async () =>
      refreshed
        ? {
            ...makeSnapshot('project-1', 'Cadence'),
            approvalRequests: [
              {
                actionId: 'flow-1:review_worktree',
                sessionId: 'session-1',
                flowId: 'flow-1',
                actionType: 'review_worktree',
                title: 'Review worktree changes',
                detail: 'Inspect the pending repository diff before continuing.',
                status: 'approved',
                decisionNote: 'Looks safe to continue.',
                createdAt: '2026-04-13T20:01:00Z',
                updatedAt: '2026-04-13T20:02:00Z',
                resolvedAt: '2026-04-13T20:02:00Z',
              },
            ],
            verificationRecords: [
              {
                id: 1,
                sourceActionId: 'flow-1:review_worktree',
                status: 'passed',
                summary: 'Approved operator action.',
                detail: null,
                recordedAt: '2026-04-13T20:02:01Z',
              },
            ],
            resumeHistory: [
              {
                id: 1,
                sourceActionId: 'flow-1:review_worktree',
                sessionId: 'session-1',
                status: 'started',
                summary: 'Operator resumed the selected project runtime session.',
                createdAt: '2026-04-13T20:03:30Z',
              },
            ],
          }
        : makeSnapshot('project-1', 'Cadence'),
    )
    setup.getRepositoryStatus.mockImplementation(async () => (refreshed ? makeStatus('project-1', 'release/verified') : makeStatus('project-1', 'main')))

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('approval-count')).toHaveTextContent('0')
    await waitFor(() => expect(setup.onProjectUpdated).toHaveBeenCalledTimes(1))

    act(() => {
      refreshed = true
      setup.emitProjectUpdated({
        project: {
          ...makeProjectSummary('project-1', 'Cadence'),
          branch: 'release/verified',
          runtime: 'openai_codex',
        },
        reason: 'metadata_changed',
      })
    })

    await waitFor(() => expect(screen.getByTestId('refresh-source')).toHaveTextContent('project:updated'))
    await waitFor(() => expect(screen.getByTestId('approval-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
    expect(screen.getByTestId('latest-decision-status')).toHaveTextContent('approved')
    expect(screen.getByTestId('verification-count')).toHaveTextContent('1')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('1')
    expect(screen.getByTestId('branch')).toHaveTextContent('release/verified')
  })

  it('ignores wrong-project update callbacks so one project cannot overwrite another project\'s operator history', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(setup.onProjectUpdated).toHaveBeenCalledTimes(1))
    const initialSnapshotCalls = setup.getProjectSnapshot.mock.calls.length

    act(() => {
      setup.emitProjectUpdated({
        project: makeProjectSummary('project-2', 'orchestra'),
        reason: 'metadata_changed',
      })
    })

    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')
    expect(screen.getByTestId('approval-count')).toHaveTextContent('0')
    expect(setup.getProjectSnapshot.mock.calls.length).toBe(initialSnapshotCalls)
  })

  it('projects persisted workflow phases into the workflow view while keeping execution git truth live', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [
          {
            id: 'project-1',
            name: 'Cadence',
            description: 'Desktop shell',
            milestone: 'M001',
            totalPhases: 3,
            completedPhases: 1,
            activePhase: 2,
            branch: null,
            runtime: null,
          },
        ],
      },
      snapshots: {
        'project-1': {
          project: {
            id: 'project-1',
            name: 'Cadence',
            description: 'Desktop shell',
            milestone: 'M001',
            totalPhases: 3,
            completedPhases: 1,
            activePhase: 2,
            branch: null,
            runtime: null,
          },
          repository: {
            id: 'repo-project-1',
            projectId: 'project-1',
            rootPath: '/tmp/Cadence',
            displayName: 'Cadence',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
          phases: [
            {
              id: 1,
              name: 'Import',
              description: 'Imported from the local registry',
              status: 'complete',
              currentStep: null,
              taskCount: 2,
              completedTasks: 2,
              summary: 'Imported successfully',
            },
            {
              id: 2,
              name: 'Live projection',
              description: 'Project workflow truth into the shell',
              status: 'active',
              currentStep: 'verify',
              taskCount: 3,
              completedTasks: 2,
              summary: null,
            },
            {
              id: 3,
              name: 'Ship shell proof',
              description: 'Verify the live shell contract',
              status: 'pending',
              currentStep: null,
              taskCount: 1,
              completedTasks: 0,
              summary: '   ',
            },
          ],
          lifecycle: {
            stages: [
              {
                stage: 'discussion',
                nodeId: 'workflow-discussion',
                status: 'complete',
                actionRequired: false,
                lastTransitionAt: '2026-04-15T17:59:00Z',
              },
              {
                stage: 'research',
                nodeId: 'workflow-research',
                status: 'active',
                actionRequired: true,
                lastTransitionAt: '2026-04-15T18:00:00Z',
              },
              {
                stage: 'requirements',
                nodeId: 'workflow-requirements',
                status: 'pending',
                actionRequired: false,
                lastTransitionAt: null,
              },
            ],
          },
          approvalRequests: [],
          verificationRecords: [],
          resumeHistory: [],
        },
      },
      statuses: {
        'project-1': {
          repository: {
            id: 'repo-project-1',
            projectId: 'project-1',
            rootPath: '/tmp/Cadence',
            displayName: 'Cadence',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
          branch: {
            name: 'feature/workflow-truth',
            headSha: 'abc1234',
            detached: false,
          },
          entries: [
            {
              path: 'client/src/App.tsx',
              staged: null,
              unstaged: 'modified',
              untracked: false,
            },
          ],
          hasStagedChanges: false,
          hasUnstagedChanges: true,
          hasUntrackedChanges: false,
        },
      },
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          phase: 'awaiting_browser_callback',
          sessionId: null,
          lastErrorCode: null,
          lastError: null,
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    expect(screen.getByTestId('workflow-has-phases')).toHaveTextContent('true')
    expect(screen.getByTestId('workflow-has-lifecycle')).toHaveTextContent('true')
    expect(screen.getByTestId('workflow-lifecycle-percent')).toHaveTextContent('33')
    expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent('research')
    expect(screen.getByTestId('workflow-lifecycle-action-required')).toHaveTextContent('1')
    expect(screen.getByTestId('workflow-overall-percent')).toHaveTextContent('33')
    expect(screen.getByTestId('workflow-active-phase')).toHaveTextContent('Live projection')
    expect(screen.getByTestId('execution-status-count')).toHaveTextContent('1')
    expect(screen.getByTestId('execution-branch')).toHaveTextContent('feature/workflow-truth')
    expect(screen.getByTestId('branch')).toHaveTextContent('feature/workflow-truth')
    expect(screen.getByTestId('runtime-label')).toHaveTextContent('Openai Codex · Awaiting browser')
  })

  it('keeps zero-phase snapshots and signed-out runtime metadata honest on startup reopen', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [
          {
            id: 'project-1',
            name: 'Cadence',
            description: '',
            milestone: '',
            totalPhases: 0,
            completedPhases: 0,
            activePhase: 0,
            branch: null,
            runtime: null,
          },
        ],
      },
      snapshots: {
        'project-1': {
          project: {
            id: 'project-1',
            name: 'Cadence',
            description: '',
            milestone: '',
            totalPhases: 0,
            completedPhases: 0,
            activePhase: 0,
            branch: null,
            runtime: null,
          },
          repository: {
            id: 'repo-project-1',
            projectId: 'project-1',
            rootPath: '/tmp/Cadence',
            displayName: 'Cadence',
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
        },
      },
      statuses: {
        'project-1': {
          repository: {
            id: 'repo-project-1',
            projectId: 'project-1',
            rootPath: '/tmp/Cadence',
            displayName: 'Cadence',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
          branch: null,
          entries: [],
          hasStagedChanges: false,
          hasUnstagedChanges: false,
          hasUntrackedChanges: false,
        },
      },
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
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
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('workflow-has-phases')).toHaveTextContent('false')
    expect(screen.getByTestId('workflow-has-lifecycle')).toHaveTextContent('false')
    expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent('none')
    expect(screen.getByTestId('workflow-lifecycle-action-required')).toHaveTextContent('0')
    expect(screen.getByTestId('branch')).toHaveTextContent('No branch')
    expect(screen.getByTestId('status-count')).toHaveTextContent('0')
    expect(screen.getByTestId('runtime-label')).toHaveTextContent('Runtime unavailable')
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('idle')
    expect(screen.getByTestId('session-reason')).toHaveTextContent(
      'Sign in with OpenAI to create a runtime session for this project.',
    )
    expect(screen.getByTestId('error')).toHaveTextContent('none')
  })

  it('exposes lifecycle-first workflow data when phases are empty but lifecycle stages are present', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [
          {
            id: 'project-1',
            name: 'Cadence',
            description: 'Desktop shell',
            milestone: 'M001',
            totalPhases: 0,
            completedPhases: 0,
            activePhase: 0,
            branch: null,
            runtime: null,
          },
        ],
      },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Cadence'),
          project: {
            id: 'project-1',
            name: 'Cadence',
            description: 'Desktop shell',
            milestone: 'M001',
            totalPhases: 0,
            completedPhases: 0,
            activePhase: 0,
            branch: null,
            runtime: null,
          },
          phases: [],
          lifecycle: {
            stages: [
              {
                stage: 'discussion',
                nodeId: 'workflow-discussion',
                status: 'complete',
                actionRequired: false,
                lastTransitionAt: '2026-04-15T17:59:00Z',
              },
              {
                stage: 'research',
                nodeId: 'workflow-research',
                status: 'active',
                actionRequired: true,
                lastTransitionAt: '2026-04-15T18:00:00Z',
              },
            ],
          },
        },
      },
      statuses: {
        'project-1': {
          repository: {
            id: 'repo-project-1',
            projectId: 'project-1',
            rootPath: '/tmp/Cadence',
            displayName: 'Cadence',
            branch: null,
            headSha: null,
            isGitRepo: true,
          },
          branch: null,
          entries: [],
          hasStagedChanges: false,
          hasUnstagedChanges: false,
          hasUntrackedChanges: false,
        },
      },
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          flowId: null,
          sessionId: null,
          accountId: null,
          phase: 'idle',
          callbackBound: null,
          authorizationUrl: null,
          redirectUri: null,
          lastErrorCode: null,
          lastError: null,
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('workflow-has-phases')).toHaveTextContent('false')
    expect(screen.getByTestId('workflow-has-lifecycle')).toHaveTextContent('true')
    expect(screen.getByTestId('workflow-lifecycle-percent')).toHaveTextContent('50')
    expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent('research')
    expect(screen.getByTestId('workflow-lifecycle-action-required')).toHaveTextContent('1')
  })

  it('supports cancelled imports and successful imports without duplicating project rows', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    setup.pickRepositoryFolder.mockResolvedValueOnce(null)
    fireEvent.click(screen.getByRole('button', { name: 'Import project' }))
    expect(setup.adapter.importRepository).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Import project' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    expect(screen.getByTestId('project-count')).toHaveTextContent('2')
  })

  it('removes the active project from the sidebar list and loads the next available project', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')],
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'Remove project 1' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    expect(screen.getByTestId('active-project')).toHaveTextContent('orchestra')
    expect(screen.getByTestId('project-count')).toHaveTextContent('1')
    expect(setup.removeProject).toHaveBeenCalledWith('project-1')
    expect(setup.listProjects).toHaveBeenCalledTimes(2)
  })

  it('keeps the current selection intact when snapshot loading fails', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')] },
    })

    setup.getProjectSnapshot.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        throw new Error('snapshot failed')
      }

      return makeSnapshot(projectId, 'Cadence')
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('snapshot failed'))
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')
    expect(screen.getByTestId('active-project')).toHaveTextContent('Cadence')
  })

  it('keeps the prior snapshot when the adapter rejects a mixed-version snapshot missing lifecycle', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')] },
    })

    setup.getProjectSnapshot.mockImplementation(async (projectId: string): Promise<ProjectSnapshotResponseDto> => {
      if (projectId === 'project-2') {
        const legacySnapshot = makeSnapshot(projectId, 'orchestra') as unknown as Record<string, unknown>
        delete legacySnapshot.lifecycle
        return legacySnapshot as unknown as ProjectSnapshotResponseDto
      }

      return makeSnapshot(projectId, 'Cadence')
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() =>
      expect(screen.getByTestId('error')).toHaveTextContent('without the required lifecycle projection'),
    )
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')
    expect(screen.getByTestId('active-project')).toHaveTextContent('Cadence')
    expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent('research')
  })

  it('keeps the current selection intact when repository status loading fails', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')] },
    })

    setup.getRepositoryStatus.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        throw new Error('status failed')
      }

      return makeStatus(projectId, projectId === 'project-1' ? 'main' : 'feature/import')
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('status failed'))
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')
    expect(screen.getByTestId('active-project')).toHaveTextContent('Cadence')
    expect(screen.getByTestId('branch')).toHaveTextContent('main')
  })

  it('preserves the newly selected project when runtime loading fails after project selection', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')] },
    })

    setup.getRuntimeSession.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        throw new Error('runtime failed')
      }

      return makeRuntimeSession(projectId)
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    expect(screen.getByTestId('active-project')).toHaveTextContent('orchestra')
    expect(screen.getByTestId('error')).toHaveTextContent('runtime failed')
    expect(screen.getByTestId('runtime-label')).toHaveTextContent('Runtime unavailable')
  })

  it('resolves operator actions by invoking the adapter and reloading the active project snapshot', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
    })

    let resolved = false
    setup.getProjectSnapshot.mockImplementation(async () =>
      resolved
        ? {
            ...makeSnapshot('project-1', 'Cadence'),
            approvalRequests: [
              {
                actionId: 'flow-1:review_worktree',
                sessionId: 'session-1',
                flowId: 'flow-1',
                actionType: 'review_worktree',
                title: 'Review worktree changes',
                detail: 'Inspect the pending repository diff before continuing.',
                status: 'approved',
                decisionNote: null,
                createdAt: '2026-04-13T20:01:00Z',
                updatedAt: '2026-04-13T20:02:00Z',
                resolvedAt: '2026-04-13T20:02:00Z',
              },
            ],
            verificationRecords: [
              {
                id: 1,
                sourceActionId: 'flow-1:review_worktree',
                status: 'passed',
                summary: 'Approved operator action.',
                detail: null,
                recordedAt: '2026-04-13T20:02:01Z',
              },
            ],
            resumeHistory: [],
          }
        : {
            ...makeSnapshot('project-1', 'Cadence'),
            approvalRequests: [
              {
                actionId: 'flow-1:review_worktree',
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
            verificationRecords: [],
            resumeHistory: [],
          },
    )
    setup.resolveOperatorAction.mockImplementation(async (projectId: string, actionId: string, decision: 'approve' | 'reject') => {
      resolved = true
      return {
        approvalRequest: {
          actionId,
          sessionId: 'session-1',
          flowId: 'flow-1',
          actionType: 'review_worktree',
          title: 'Review worktree changes',
          detail: 'Inspect the pending repository diff before continuing.',
          status: decision === 'approve' ? 'approved' : 'rejected',
          decisionNote: null,
          createdAt: '2026-04-13T20:01:00Z',
          updatedAt: '2026-04-13T20:02:00Z',
          resolvedAt: '2026-04-13T20:02:00Z',
        },
        verificationRecord: {
          id: 1,
          sourceActionId: actionId,
          status: decision === 'approve' ? 'passed' : 'failed',
          summary: 'Approved operator action.',
          detail: null,
          recordedAt: '2026-04-13T20:02:01Z',
        },
      }
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    await waitFor(() => expect(screen.getByTestId('project-loading')).toHaveTextContent('false'))

    await act(async () => {
      await Promise.resolve()
    })

    fireEvent.click(screen.getByRole('button', { name: 'Approve operator action' }))

    await waitFor(() => expect(setup.resolveOperatorAction).toHaveBeenCalledWith('project-1', 'flow-1:review_worktree', 'approve', { userAnswer: null }))
    await waitFor(() => expect(screen.getByTestId('refresh-source')).toHaveTextContent('operator:resolve'))
    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0'))
    expect(screen.getByTestId('verification-count')).toHaveTextContent('1')
    expect(screen.getByTestId('operator-action-error-code')).toHaveTextContent('none')
  })

  it('surfaces operator mutation failures and keeps the last truthful project view', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
    })

    setup.resolveOperatorAction.mockRejectedValueOnce(
      new CadenceDesktopError({
        code: 'operator_action_not_found',
        errorClass: 'user_fixable',
        message: 'Cadence could not find operator request `flow-1:review_worktree` for the selected project.',
        retryable: false,
      }),
    )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('project-loading')).toHaveTextContent('false'))

    await act(async () => {
      await Promise.resolve()
    })

    fireEvent.click(screen.getByRole('button', { name: 'Approve operator action' }))

    await waitFor(() => expect(screen.getByTestId('operator-action-error-code')).toHaveTextContent('operator_action_not_found'))
    expect(screen.getByTestId('active-project')).toHaveTextContent('Cadence')
    expect(screen.getByTestId('pending-operator-action-id')).toHaveTextContent('none')
  })

  it('loads repository diffs lazily and surfaces diff failures without clearing the active project', async () => {
    const setup = createMockAdapter()

    setup.getRepositoryDiff.mockRejectedValueOnce(new Error('diff failed'))

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('project-loading')).toHaveTextContent('false'))

    await act(async () => {
      await Promise.resolve()
    })

    fireEvent.click(screen.getByRole('button', { name: 'Load unstaged diff' }))

    await waitFor(() => expect(screen.getByTestId('diff-error')).toHaveTextContent('diff failed'))
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')
    expect(screen.getByTestId('diff-status')).toHaveTextContent('error')

    setup.getRepositoryDiff.mockResolvedValueOnce(makeDiff('project-1', 'unstaged', 'diff --git a/file b/file\n+change'))
    fireEvent.click(screen.getByRole('button', { name: 'Load unstaged diff' }))

    await waitFor(() => expect(screen.getByTestId('diff-status')).toHaveTextContent('ready'))
    expect(screen.getByTestId('diff-patch')).toHaveTextContent('+change')
  })

  it('subscribes to authenticated runtime streams and exposes normalized stream state in the agent view', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(1))
    expect(setup.subscribeRuntimeStream).toHaveBeenCalledWith(
      'project-1',
      ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
      expect.any(Function),
      expect.any(Function),
    )
    expect(screen.getByTestId('stream-status')).toHaveTextContent('subscribing')

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to Cadence.',
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

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('live'))
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('1')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('1 item captured')

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'skill',
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
          createdAt: '2026-04-13T20:01:02Z',
        }),
      )
    })

    await waitFor(() => expect(screen.getByTestId('stream-skill-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('stream-skill-first-id')).toHaveTextContent('find-skills')
    expect(screen.getByTestId('stream-skill-first-stage')).toHaveTextContent('install')
    expect(screen.getByTestId('stream-skill-first-result')).toHaveTextContent('succeeded')

    act(() => {
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
          actionId: 'run-project-1:review_worktree',
          boundaryId: 'boundary-1',
          actionType: 'review_worktree',
          title: 'Repository has local changes',
          detail: 'Review the worktree before trusting agent actions.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:03Z',
        }),
      )
    })

    await waitFor(() => expect(screen.getByTestId('action-required-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('action-required-title')).toHaveTextContent('Repository has local changes')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('Review the worktree before trusting agent actions.')
  })

  it('clears stale stream cache on project switch and ignores callbacks from the old subscription', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')] },
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1'),
        'project-2': makeRuntimeSession('project-2', {
          flowId: 'flow-2',
          sessionId: 'session-2',
          accountId: 'acct-2',
        }),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1'),
        'project-2': makeRuntimeRun('project-2', { runId: 'run-project-2' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(1))

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'Connected to Cadence.',
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

    await waitFor(() => expect(screen.getByTestId('stream-item-count')).toHaveTextContent('1'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(2))
    expect(setup.streamSubscriptions[0]?.unsubscribe).toHaveBeenCalledTimes(1)
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('0')

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-1', {
          kind: 'transcript',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: 'stale callback',
          toolCallId: null,
          toolName: null,
          toolState: null,
          actionType: null,
          title: null,
          detail: null,
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:02:00Z',
        }),
      )
    })

    expect(screen.getByTestId('active-project')).toHaveTextContent('orchestra')
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('0')
  })

  it('surfaces subscribe failures and malformed stream payloads without clearing the selected project', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      subscribeErrors: {
        'project-1': new CadenceDesktopError({
          code: 'runtime_stream_not_ready',
          errorClass: 'retryable',
          message: 'Cadence cannot start a runtime stream until the selected project finishes auth.',
          retryable: true,
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project')).toHaveTextContent('Cadence'))
    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('stale'))
    expect(screen.getByTestId('stream-error')).toHaveTextContent(
      'Cadence cannot start a runtime stream until the selected project finishes auth.',
    )

    setup.subscribeRuntimeStream.mockImplementationOnce(
      async (projectId: string, _itemKinds, handler, onError) => {
        setup.streamSubscriptions.push({
          projectId,
          handler,
          onError: onError ?? null,
          unsubscribe: vi.fn(),
        })

        return {
          response: makeStreamResponse(projectId),
          unsubscribe: vi.fn(),
        }
      },
    )

    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('subscribing'))

    act(() => {
      setup.emitRuntimeStreamError(
        0,
        new CadenceDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: 'Command subscribe_runtime_stream channel returned an unexpected payload shape.',
        }),
      )
    })

    await waitFor(() =>
      expect(screen.getByTestId('stream-error')).toHaveTextContent(
        'Command subscribe_runtime_stream channel returned an unexpected payload shape.',
      ),
    )
    expect(screen.getByTestId('active-project')).toHaveTextContent('Cadence')
  })

  it('rejects wrong-project stream items and clears stream state when runtime auth logs out', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(setup.onRuntimeUpdated).toHaveBeenCalledTimes(1))

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeStreamEvent('project-2', {
          kind: 'tool',
          sessionId: 'session-1',
          flowId: 'flow-1',
          text: null,
          toolCallId: 'bootstrap-repository-context',
          toolName: 'inspect_repository_context',
          toolState: 'running',
          actionType: null,
          title: null,
          detail: 'Collecting repository context.',
          code: null,
          message: null,
          retryable: null,
          createdAt: '2026-04-13T20:01:00Z',
        }),
      )
    })

    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('stream-error')).toHaveTextContent('project-2')
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('0')

    act(() => {
      setup.emitRuntimeUpdated({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        flowId: null,
        sessionId: null,
        accountId: null,
        authPhase: 'idle',
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-13T20:02:00Z',
      })
    })

    await waitFor(() => expect(screen.getByTestId('auth-phase')).toHaveTextContent('idle'))
    expect(screen.getByTestId('stream-status')).toHaveTextContent('idle')
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('0')
  })

  it('applies runtime update events and surfaces contract mismatches without clearing selection', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence'), makeProjectSummary('project-2', 'orchestra')] },
    })

    const { unmount } = render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(setup.onRuntimeUpdated).toHaveBeenCalledTimes(1))

    act(() => {
      setup.emitRuntimeUpdated({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'azure_openai',
        flowId: 'flow-1',
        sessionId: null,
        accountId: 'acct-1',
        authPhase: 'awaiting_manual_input',
        lastErrorCode: 'callback_listener_bind_failed',
        lastError: {
          code: 'callback_listener_bind_failed',
          message: 'Paste the redirect URL to finish login.',
          retryable: false,
        },
        updatedAt: '2026-04-13T20:01:00Z',
      })
    })

    await waitFor(() => expect(screen.getByTestId('auth-phase')).toHaveTextContent('awaiting_manual_input'))
    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('azure_openai')
    expect(screen.getByTestId('runtime-label')).toHaveTextContent('Openai Codex · Awaiting manual input')
    expect(screen.getByTestId('session-reason')).toHaveTextContent('Paste the redirect URL to finish login.')
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('runtime:updated')

    act(() => {
      setup.emitRuntimeUpdated({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'stale_provider',
        flowId: 'flow-1',
        sessionId: 'session-stale',
        accountId: 'acct-stale',
        authPhase: 'idle',
        lastErrorCode: 'auth_session_not_found',
        lastError: {
          code: 'auth_session_not_found',
          message: 'Stale payload should not win.',
          retryable: false,
        },
        updatedAt: '2026-04-13T20:00:00Z',
      })
    })

    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('azure_openai')
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('awaiting_manual_input')

    act(() => {
      setup.emitRuntimeUpdatedError(
        new CadenceDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: 'Event runtime:updated returned an unexpected payload shape.',
        }),
      )
    })

    await waitFor(() =>
      expect(screen.getByTestId('error')).toHaveTextContent(
        'Event runtime:updated returned an unexpected payload shape.',
      ),
    )
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')

    unmount()
    expect(setup.projectUnlisten).toHaveBeenCalledTimes(1)
    expect(setup.repositoryUnlisten).toHaveBeenCalledTimes(1)
    expect(setup.runtimeUnlisten).toHaveBeenCalledTimes(1)
  })

  it('eager-loads app-global runtime settings and keeps selected-project runtime truth separate after save', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openai_codex',
        modelId: 'openai_codex',
        openrouterApiKeyConfigured: false,
      }),
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('runtime-settings-provider-id')).toHaveTextContent('openai_codex'))
    expect(screen.getByTestId('runtime-settings-model-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('selected-provider-label')).toHaveTextContent('OpenAI Codex')
    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('openai_codex')

    fireEvent.click(screen.getByRole('button', { name: 'Save OpenRouter runtime settings' }))

    await waitFor(() =>
      expect(setup.upsertRuntimeSettings).toHaveBeenCalledWith({
        providerId: 'openrouter',
        modelId: 'openai/gpt-4.1-mini',
        openrouterApiKey: 'sk-or-v1-test-secret',
      }),
    )
    await waitFor(() => expect(screen.getByTestId('runtime-settings-provider-id')).toHaveTextContent('openrouter'))

    expect(screen.getByTestId('runtime-settings-model-id')).toHaveTextContent('openai/gpt-4.1-mini')
    expect(screen.getByTestId('runtime-settings-key-configured')).toHaveTextContent('true')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openrouter')
    expect(screen.getByTestId('selected-provider-label')).toHaveTextContent('OpenRouter')
    expect(screen.getByTestId('selected-model-id')).toHaveTextContent('openai/gpt-4.1-mini')
    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('authenticated')
    expect(screen.getByTestId('runtime-settings-load-error-code')).toHaveTextContent('none')
    expect(screen.getByTestId('runtime-settings-save-error-code')).toHaveTextContent('none')
  })

  it('preserves the last-known-good runtime settings snapshot when refresh or save fails', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openrouter',
        modelId: 'meta-llama/llama-3.1-8b-instruct',
        openrouterApiKeyConfigured: true,
      }),
    })

    setup.getRuntimeSettings
      .mockResolvedValueOnce(
        makeRuntimeSettings({
          providerId: 'openrouter',
          modelId: 'meta-llama/llama-3.1-8b-instruct',
          openrouterApiKeyConfigured: true,
        }),
      )
      .mockRejectedValueOnce(
        new CadenceDesktopError({
          code: 'runtime_settings_timeout',
          errorClass: 'retryable',
          message: 'Cadence timed out while loading app-global runtime settings.',
          retryable: true,
        }),
      )

    setup.upsertRuntimeSettings.mockRejectedValueOnce(
      new CadenceDesktopError({
        code: 'runtime_settings_write_failed',
        errorClass: 'retryable',
        message: 'Cadence could not save app-global runtime settings.',
        retryable: true,
      }),
    )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('runtime-settings-provider-id')).toHaveTextContent('openrouter'))
    expect(screen.getByTestId('runtime-settings-model-id')).toHaveTextContent('meta-llama/llama-3.1-8b-instruct')
    expect(screen.getByTestId('runtime-settings-key-configured')).toHaveTextContent('true')

    fireEvent.click(screen.getByRole('button', { name: 'Load runtime settings' }))

    await waitFor(() => expect(screen.getByTestId('runtime-settings-load-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('runtime-settings-load-error-code')).toHaveTextContent('runtime_settings_timeout')
    expect(screen.getByTestId('runtime-settings-provider-id')).toHaveTextContent('openrouter')
    expect(screen.getByTestId('runtime-settings-model-id')).toHaveTextContent('meta-llama/llama-3.1-8b-instruct')
    expect(screen.getByTestId('runtime-settings-key-configured')).toHaveTextContent('true')

    fireEvent.click(screen.getByRole('button', { name: 'Save OpenRouter runtime settings' }))

    await waitFor(() => expect(screen.getByTestId('runtime-settings-save-error-code')).toHaveTextContent('runtime_settings_write_failed'))
    expect(screen.getByTestId('runtime-settings-provider-id')).toHaveTextContent('openrouter')
    expect(screen.getByTestId('runtime-settings-model-id')).toHaveTextContent('meta-llama/llama-3.1-8b-instruct')
    expect(screen.getByTestId('runtime-settings-key-configured')).toHaveTextContent('true')
  })

  it('derives OpenRouter-first guidance and mismatch recovery from app-global settings', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openrouter',
        modelId: 'openai/gpt-4.1-mini',
        openrouterApiKeyConfigured: true,
      }),
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          providerId: 'openai_codex',
          runtimeKind: 'openai_codex',
          phase: 'authenticated',
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openrouter'))
    expect(screen.getByTestId('selected-provider-label')).toHaveTextContent('OpenRouter')
    expect(screen.getByTestId('provider-mismatch')).toHaveTextContent('true')
    expect(screen.getByTestId('session-reason')).toHaveTextContent('Selected provider is OpenRouter')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('Rebind the selected provider before trusting new stream activity.')
  })

  it('derives missing-key OpenRouter guidance without OpenAI login copy', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openrouter',
        modelId: 'openai/gpt-4.1-mini',
        openrouterApiKeyConfigured: false,
      }),
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          providerId: 'openrouter',
          runtimeKind: 'openrouter',
          flowId: null,
          sessionId: null,
          accountId: null,
          phase: 'idle',
          callbackBound: null,
          authorizationUrl: null,
          redirectUri: null,
          lastErrorCode: null,
          lastError: null,
        }),
      },
      runtimeRuns: {
        'project-1': null,
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openrouter'))
    expect(screen.getByTestId('session-reason')).toHaveTextContent('Configure an OpenRouter API key in Settings')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('Configure an OpenRouter API key in Settings')
    expect(screen.getByTestId('session-reason')).not.toHaveTextContent('OpenAI')
    expect(screen.getByTestId('messages-reason')).not.toHaveTextContent('OpenAI')
  })
})
