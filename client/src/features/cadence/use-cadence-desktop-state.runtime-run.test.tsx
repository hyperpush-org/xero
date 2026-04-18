import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { CadenceDesktopError, type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
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
  ResumeOperatorRunResponseDto,
  AutonomousRunStateDto,
  RuntimeRunDto,
  RuntimeRunUpdatedPayloadDto,
  RuntimeSessionDto,
  RuntimeStreamEventDto,
  RuntimeUpdatedPayloadDto,
  SubscribeRuntimeStreamResponseDto,
  SyncNotificationAdaptersResponseDto,
  UpsertNotificationRouteCredentialsRequestDto,
  UpsertNotificationRouteCredentialsResponseDto,
} from '@/src/lib/cadence-model'
import { useCadenceDesktopState, BLOCKED_NOTIFICATION_SYNC_POLL_MS } from '@/src/features/cadence/use-cadence-desktop-state'

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
    lifecycle: {
      stages: [
        {
          stage: 'discussion',
          nodeId: 'workflow-discussion',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-15T20:00:00Z',
        },
      ],
    },
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
  }
}

function makeHandoffPackage(projectId: string, transitionId = 'auto:transition-1') {
  return {
    id: 11,
    projectId,
    handoffTransitionId: transitionId,
    causalTransitionId: 'txn-001',
    fromNodeId: 'workflow-discussion',
    toNodeId: 'workflow-research',
    transitionKind: 'advance',
    packagePayload: '{"schemaVersion":1}',
    packageHash: 'd41d8cd98f00b204e9800998ecf8427e',
    createdAt: '2026-04-16T14:00:01Z',
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
    gateNodeId: 'workflow-research',
    gateKey: 'requires_user_input',
    transitionFromNodeId: 'workflow-discussion',
    transitionToNodeId: 'workflow-research',
    transitionKind: 'advance',
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
    gateNodeId: 'workflow-research',
    gateKey: 'requires_user_input',
    transitionFromNodeId: 'workflow-discussion',
    transitionToNodeId: 'workflow-research',
    transitionKind: 'advance',
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

function makeRuntimeRun(projectId: string, overrides: Partial<RuntimeRunDto> = {}): RuntimeRunDto {
  return {
    projectId,
    runId: `run-${projectId}`,
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

function makeAutonomousRunState(
  projectId: string,
  overrides: Partial<NonNullable<AutonomousRunStateDto['run']>> = {},
): AutonomousRunStateDto {
  const runId = overrides.runId ?? `auto-${projectId}`

  return {
    run: {
      projectId,
      runId,
      runtimeKind: 'openai_codex',
      supervisorKind: 'detached_pty',
      status: 'running',
      recoveryState: 'healthy',
      activeUnitId: `${runId}:checkpoint:2`,
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
    unit: {
      projectId,
      runId,
      unitId: `${runId}:checkpoint:2`,
      sequence: 2,
      kind: 'state',
      status: 'active',
      summary: 'Recovered the current autonomous unit boundary.',
      boundaryId: 'checkpoint:2',
      startedAt: '2026-04-16T20:00:01Z',
      finishedAt: null,
      updatedAt: '2026-04-16T20:00:06Z',
      lastErrorCode: null,
      lastError: null,
    },
  }
}

function makeBlockedAutonomousRunState(projectId: string, boundaryId: string): AutonomousRunStateDto {
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

  state.unit = {
    ...state.unit!,
    status: 'blocked',
    summary: 'Blocked on operator boundary `Terminal input required`.',
    boundaryId,
    updatedAt: '2026-04-16T20:03:00Z',
  }

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

  state.unit = {
    ...state.unit!,
    status: 'active',
    summary: 'Recovered the current autonomous unit boundary.',
    boundaryId: null,
    updatedAt: '2026-04-16T20:04:00Z',
  }

  return state
}

function makeAutonomousHistoryEntry(options: {
  projectId: string
  unitId: string
  sequence: number
  unitUpdatedAt: string
  latestAttempt?: boolean
  workflowNodeId?: string
  handoffTransitionId?: string
  handoffPackageHash?: string
  artifactSummary?: string
}) {
  const runId = `auto-${options.projectId}`
  const attemptId = `${options.unitId}:attempt:1`
  const workflowLinkage = options.workflowNodeId
    ? {
        workflowNodeId: options.workflowNodeId,
        transitionId: `${options.unitId}:transition:1`,
        causalTransitionId: `${options.unitId}:causal:1`,
        handoffTransitionId: options.handoffTransitionId ?? `${options.unitId}:handoff:1`,
        handoffPackageHash: options.handoffPackageHash ?? `${options.unitId}:hash:1`,
      }
    : null

  return {
    unit: {
      projectId: options.projectId,
      runId,
      unitId: options.unitId,
      sequence: options.sequence,
      kind: 'state',
      status: 'completed',
      summary: `Recovered durable unit ${options.sequence}.`,
      boundaryId: `boundary:${options.sequence}`,
      workflowLinkage,
      startedAt: '2026-04-16T20:00:00Z',
      finishedAt: options.unitUpdatedAt,
      updatedAt: options.unitUpdatedAt,
      lastErrorCode: null,
      lastError: null,
    },
    latestAttempt:
      options.latestAttempt === false
        ? null
        : {
            projectId: options.projectId,
            runId,
            unitId: options.unitId,
            attemptId,
            attemptNumber: 1,
            childSessionId: `child-${options.sequence}`,
            status: 'completed',
            boundaryId: `boundary:${options.sequence}`,
            workflowLinkage,
            startedAt: '2026-04-16T20:00:01Z',
            finishedAt: options.unitUpdatedAt,
            updatedAt: options.unitUpdatedAt,
            lastErrorCode: null,
            lastError: null,
          },
    artifacts: options.artifactSummary
      ? [
          {
            projectId: options.projectId,
            runId,
            unitId: options.unitId,
            attemptId,
            artifactId: `${attemptId}:artifact:1`,
            artifactKind: 'tool_result',
            status: 'recorded',
            summary: options.artifactSummary,
            contentHash: 'hash',
            payload: null,
            createdAt: options.unitUpdatedAt,
            updatedAt: options.unitUpdatedAt,
            detail: 'Tool output recorded.',
            commandResult: {
              exitCode: 0,
              timedOut: false,
              summary: 'read completed',
            },
            toolName: 'read',
            toolState: 'succeeded',
            evidenceKind: null,
            verificationOutcome: null,
            diagnosticCode: null,
            actionId: null,
            boundaryId: null,
          },
        ]
      : [],
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
  notificationDispatches?: Record<string, ListNotificationDispatchesResponseDto['dispatches']>
  notificationRoutes?: Record<string, ListNotificationRoutesResponseDto['routes']>
  notificationDispatchErrors?: Record<string, Error>
  notificationRouteErrors?: Record<string, Error>
  notificationSyncResponses?: Record<string, SyncNotificationAdaptersResponseDto>
  notificationSyncErrors?: Record<string, Error>
  upsertRouteErrors?: Record<string, Error>
  subscribeResponses?: Record<string, SubscribeRuntimeStreamResponseDto>
}) {
  let projectUpdatedHandler: ((payload: ProjectUpdatedPayloadDto) => void) | null = null
  let projectUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null

  const snapshots = options?.snapshots ?? {
    'project-1': makeSnapshot('project-1', 'cadence'),
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
  const streamSubscriptions: Array<{
    projectId: string
    handler: (payload: RuntimeStreamEventDto) => void
    onError: ((error: CadenceDesktopError) => void) | null
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

  const getAutonomousRun = vi.fn(async (projectId: string) => autonomousStates[projectId] ?? { run: null, unit: null })

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

  const adapter: CadenceDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder: vi.fn(async () => null),
    importRepository: vi.fn(async () => {
      throw new Error('not used in runtime-run tests')
    }) as unknown as (path: string) => Promise<ImportRepositoryResponseDto>,
    listProjects: vi.fn(async () =>
      options?.listProjects ?? {
        projects: [makeProjectSummary('project-1', 'cadence')],
      },
    ),
    getProjectSnapshot,
    getRepositoryStatus: vi.fn(async (projectId: string) => statuses[projectId]),
    getRepositoryDiff: vi.fn(async (projectId: string, scope: 'staged' | 'unstaged' | 'worktree') =>
      makeDiff(projectId, scope),
    ),
    getAutonomousRun,
    getRuntimeRun,
    getRuntimeSession,
    startOpenAiLogin: vi.fn(async (projectId: string) => makeRuntimeSession(projectId)),
    submitOpenAiCallback: vi.fn(async (projectId: string) => makeRuntimeSession(projectId)),
    startAutonomousRun: vi.fn(async (projectId: string) => {
      const nextState = makeAutonomousRunState(projectId, {
        duplicateStartDetected: Boolean(autonomousStates[projectId]?.run),
        duplicateStartRunId: autonomousStates[projectId]?.run?.runId ?? null,
        duplicateStartReason: autonomousStates[projectId]?.run
          ? 'Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor.'
          : null,
      })
      autonomousStates[projectId] = nextState
      return nextState
    }),
    startRuntimeRun: vi.fn(async (projectId: string) => runtimeRuns[projectId] ?? makeRuntimeRun(projectId)),
    startRuntimeSession: vi.fn(async (projectId: string) => runtimeSessions[projectId]),
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
      nextState.unit = {
        ...nextState.unit!,
        status: 'cancelled',
        finishedAt: '2026-04-16T20:10:00Z',
        updatedAt: '2026-04-16T20:10:00Z',
      }
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
    upsertWorkflowGraph: vi.fn(async () => {
      throw new Error('not used in runtime-run tests')
    }) as never,
    applyWorkflowTransition: vi.fn(async () => {
      throw new Error('not used in runtime-run tests')
    }) as never,
    subscribeRuntimeStream: vi.fn(
      async (
        projectId: string,
        _itemKinds,
        handler: (payload: RuntimeStreamEventDto) => void,
        onError?: (error: CadenceDesktopError) => void,
      ) => {
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
              flowId: runtimeSessions[projectId]?.flowId ?? 'flow-1',
            }),
          unsubscribe: subscription.unsubscribe,
        }
      },
    ),
    onProjectUpdated: vi.fn(
      async (
        handler: (payload: ProjectUpdatedPayloadDto) => void,
        onError?: (error: CadenceDesktopError) => void,
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
        onError?: (error: CadenceDesktopError) => void,
      ) => {
        runtimeRunUpdatedHandler = handler
        runtimeRunUpdatedErrorHandler = onError ?? null
        return () => undefined
      },
    ),
  }

  return {
    adapter,
    getProjectSnapshot,
    getRuntimeRun,
    getAutonomousRun,
    listNotificationRoutes,
    listNotificationDispatches,
    syncNotificationAdapters,
    upsertNotificationRoute,
    resumeOperatorRun,
    subscribeRuntimeStream: adapter.subscribeRuntimeStream,
    streamSubscriptions,
    emitProjectUpdated(payload: ProjectUpdatedPayloadDto) {
      projectUpdatedHandler?.(payload)
    },
    emitProjectUpdatedError(error: CadenceDesktopError) {
      projectUpdatedErrorHandler?.(error)
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
  const checkpointControlLoop = state.agentView?.checkpointControlLoop ?? null
  const firstCheckpointCard = checkpointControlLoop?.items[0] ?? null
  const firstApprovalResumeState =
    !firstApproval
      ? 'none'
      : state.agentView?.operatorActionStatus === 'running' && state.agentView?.pendingOperatorActionId === firstApproval.actionId
        ? 'running'
        : latestResumeForFirstApproval?.status ?? 'waiting'

  return (
    <div>
      <div data-testid="active-project-id">{state.activeProjectId ?? 'none'}</div>
      <div data-testid="error">{state.errorMessage ?? 'none'}</div>
      <div data-testid="refresh-source">{state.refreshSource ?? 'none'}</div>
      <div data-testid="auth-phase">{state.agentView?.authPhase ?? 'none'}</div>
      <div data-testid="runtime-run-id">{state.agentView?.runtimeRun?.runId ?? 'none'}</div>
      <div data-testid="runtime-run-status">{state.agentView?.runtimeRun?.status ?? 'none'}</div>
      <div data-testid="runtime-run-status-label">{state.agentView?.runtimeRun?.statusLabel ?? 'none'}</div>
      <div data-testid="runtime-run-checkpoint-count">{String(state.agentView?.runtimeRun?.checkpointCount ?? 0)}</div>
      <div data-testid="runtime-run-last-checkpoint-summary">
        {state.agentView?.runtimeRun?.latestCheckpoint?.summary ?? 'none'}
      </div>
      <div data-testid="runtime-run-error">{state.agentView?.runtimeRunErrorMessage ?? 'none'}</div>
      <div data-testid="runtime-run-reason">{state.agentView?.runtimeRunUnavailableReason ?? 'none'}</div>
      <div data-testid="autonomous-run-id">{state.agentView?.autonomousRun?.runId ?? 'none'}</div>
      <div data-testid="autonomous-run-status">{state.agentView?.autonomousRun?.status ?? 'none'}</div>
      <div data-testid="autonomous-run-recovery">{state.agentView?.autonomousRun?.recoveryState ?? 'none'}</div>
      <div data-testid="autonomous-run-duplicate-start">{String(state.agentView?.autonomousRun?.duplicateStartDetected ?? false)}</div>
      <div data-testid="autonomous-run-error">{state.agentView?.autonomousRunErrorMessage ?? 'none'}</div>
      <div data-testid="autonomous-unit-id">{state.agentView?.autonomousUnit?.unitId ?? 'none'}</div>
      <div data-testid="autonomous-unit-status">{state.agentView?.autonomousUnit?.status ?? 'none'}</div>
      <div data-testid="autonomous-unit-summary">{state.agentView?.autonomousUnit?.summary ?? 'none'}</div>
      <div data-testid="recent-unit-count">{String(state.agentView?.recentAutonomousUnits?.items.length ?? 0)}</div>
      <div data-testid="recent-unit-window-label">{state.agentView?.recentAutonomousUnits?.windowLabel ?? 'none'}</div>
      <div data-testid="recent-unit-first-id">{state.agentView?.recentAutonomousUnits?.items[0]?.unitId ?? 'none'}</div>
      <div data-testid="recent-unit-first-workflow-state">
        {state.agentView?.recentAutonomousUnits?.items[0]?.workflowStateLabel ?? 'none'}
      </div>
      <div data-testid="recent-unit-first-evidence-state">
        {state.agentView?.recentAutonomousUnits?.items[0]?.evidenceStateLabel ?? 'none'}
      </div>
      <div data-testid="checkpoint-loop-count">{String(checkpointControlLoop?.items.length ?? 0)}</div>
      <div data-testid="checkpoint-loop-window-label">{checkpointControlLoop?.windowLabel ?? 'none'}</div>
      <div data-testid="checkpoint-loop-first-action-id">{firstCheckpointCard?.actionId ?? 'none'}</div>
      <div data-testid="checkpoint-loop-first-truth-source">{firstCheckpointCard?.truthSource ?? 'none'}</div>
      <div data-testid="checkpoint-loop-first-live-state">{firstCheckpointCard?.liveStateLabel ?? 'none'}</div>
      <div data-testid="checkpoint-loop-first-durable-state">{firstCheckpointCard?.durableStateLabel ?? 'none'}</div>
      <div data-testid="checkpoint-loop-first-broker-state">{firstCheckpointCard?.brokerStateLabel ?? 'none'}</div>
      <div data-testid="checkpoint-loop-first-resume-state">{firstCheckpointCard?.resumeStateLabel ?? 'none'}</div>
      <div data-testid="checkpoint-loop-first-evidence-state">{firstCheckpointCard?.evidenceStateLabel ?? 'none'}</div>
      <div data-testid="messages-reason">{state.agentView?.messagesUnavailableReason ?? 'none'}</div>
      <div data-testid="stream-status">{state.agentView?.runtimeStreamStatus ?? 'idle'}</div>
      <div data-testid="stream-run-id">{state.agentView?.runtimeStream?.runId ?? 'none'}</div>
      <div data-testid="stream-last-sequence">{String(state.agentView?.runtimeStream?.lastSequence ?? 0)}</div>
      <div data-testid="stream-item-count">{String(state.agentView?.runtimeStreamItems?.length ?? 0)}</div>
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
      <div data-testid="handoff-package-count">{String(state.activeProject?.handoffPackages.length ?? 0)}</div>
      <div data-testid="latest-handoff-transition-id">
        {state.activeProject?.handoffPackages[state.activeProject.handoffPackages.length - 1]?.handoffTransitionId ?? 'none'}
      </div>
      <div data-testid="workflow-has-lifecycle">{String(state.workflowView?.hasLifecycle ?? false)}</div>
      <div data-testid="workflow-lifecycle-percent">{String(state.workflowView?.lifecyclePercent ?? 0)}</div>
      <div data-testid="workflow-active-lifecycle-stage">{state.workflowView?.activeLifecycleStage?.stage ?? 'none'}</div>
      <div data-testid="workflow-lifecycle-action-required">{String(state.workflowView?.actionRequiredLifecycleCount ?? 0)}</div>
      <button onClick={() => void state.retry()} type="button">
        Retry state
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

describe('useCadenceDesktopState runtime-run hydration', () => {
  it('hydrates durable runtime-run state independently when auth-session loading fails on startup', async () => {
    const setup = createMockAdapter({
      runtimeSessionErrors: {
        'project-1': new Error('runtime auth failed'),
      },
      runtimeRuns: {
        'project-1': makeRuntimeRun('project-1'),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    expect(screen.getByTestId('auth-phase')).toHaveTextContent('none')
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('runtime-run-status')).toHaveTextContent('running')
    expect(screen.getByTestId('runtime-run-checkpoint-count')).toHaveTextContent('2')
    expect(screen.getByTestId('runtime-run-last-checkpoint-summary')).toHaveTextContent(
      'Recovered repository context before reconnecting the live feed.',
    )
    expect(screen.getByTestId('runtime-run-reason')).toHaveTextContent(
      'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.',
    )
    expect(screen.getByTestId('messages-reason')).toHaveTextContent(
      'Cadence recovered durable supervised-run state for this project, but live streaming still requires a desktop-authenticated runtime session.',
    )
    expect(screen.getByTestId('error')).toHaveTextContent('runtime auth failed')
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

  it('hydrates autonomous run and unit truth independently from the durable ledger', async () => {
    const setup = createMockAdapter({
      autonomousStates: {
        'project-1': makeAutonomousRunState('project-1', {
          runId: 'auto-project-1',
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
    expect(screen.getByTestId('autonomous-run-status')).toHaveTextContent('running')
    expect(screen.getByTestId('autonomous-run-recovery')).toHaveTextContent('recovery_required')
    expect(screen.getByTestId('autonomous-unit-id')).toHaveTextContent('auto-project-1:checkpoint:2')
    expect(screen.getByTestId('autonomous-unit-status')).toHaveTextContent('active')
  })

  it('preserves the last truthful autonomous run state when later autonomous refreshes fail', async () => {
    const setup = createMockAdapter({
      autonomousStates: {
        'project-1': makeAutonomousRunState('project-1', { runId: 'auto-project-1' }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('autonomous-run-id')).toHaveTextContent('auto-project-1'))

    setup.getAutonomousRun.mockRejectedValueOnce(new Error('autonomous refresh failed'))
    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('autonomous refresh failed'))
    expect(screen.getByTestId('autonomous-run-id')).toHaveTextContent('auto-project-1')
    expect(screen.getByTestId('autonomous-unit-id')).toHaveTextContent('auto-project-1:checkpoint:2')
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
    expect(screen.getByTestId('autonomous-unit-status')).toHaveTextContent('cancelled')
  })

  it('keeps recovered checkpoints visible while the live runtime stream is still reconnecting', async () => {
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
      'Cadence is reconnecting the live runtime stream while keeping durable checkpoints visible for this selected project.',
    )
  })

  it('projects bounded recent autonomous units from durable history while live runtime state recovers', async () => {
    const recoveredState = makeRecoveredAutonomousRunState('project-1')
    recoveredState.history = [
      makeAutonomousHistoryEntry({
        projectId: 'project-1',
        unitId: 'unit-history-2',
        sequence: 2,
        workflowNodeId: 'workflow-research',
        handoffTransitionId: 'handoff-history-2',
        handoffPackageHash: 'hash-history-2',
        unitUpdatedAt: '2026-04-16T20:05:00Z',
        artifactSummary: 'Read README.md from the imported repository root.',
      }),
      makeAutonomousHistoryEntry({
        projectId: 'project-1',
        unitId: 'unit-history-1',
        sequence: 1,
        latestAttempt: false,
        unitUpdatedAt: '2026-04-16T20:04:00Z',
      }),
    ]

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
      autonomousStates: {
        'project-1': recoveredState,
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('recent-unit-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('recent-unit-first-id')).toHaveTextContent('unit-history-2')
    expect(screen.getByTestId('recent-unit-first-workflow-state')).toHaveTextContent('Snapshot lag')
    expect(screen.getByTestId('recent-unit-first-evidence-state')).toHaveTextContent('1 recent evidence row')
    expect(screen.getByTestId('recent-unit-window-label')).toHaveTextContent('Showing 2 durable units')
  })

  it('preserves the last truthful recent-unit projection when later autonomous refreshes fail', async () => {
    const recoveredState = makeRecoveredAutonomousRunState('project-1')
    recoveredState.history = [
      makeAutonomousHistoryEntry({
        projectId: 'project-1',
        unitId: 'unit-history-2',
        sequence: 2,
        workflowNodeId: 'workflow-research',
        handoffTransitionId: 'handoff-history-2',
        handoffPackageHash: 'hash-history-2',
        unitUpdatedAt: '2026-04-16T20:05:00Z',
        artifactSummary: 'Read README.md from the imported repository root.',
      }),
    ]

    const setup = createMockAdapter({
      autonomousStates: {
        'project-1': recoveredState,
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('recent-unit-count')).toHaveTextContent('1'))

    setup.getAutonomousRun.mockRejectedValueOnce(new Error('autonomous refresh failed'))
    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('autonomous refresh failed'))
    expect(screen.getByTestId('recent-unit-count')).toHaveTextContent('1')
    expect(screen.getByTestId('recent-unit-first-id')).toHaveTextContent('unit-history-2')
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
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'cadence' : 'orchestra')
      if (projectId !== 'project-1') {
        return snapshot
      }

      if (!autoDispatched) {
        return {
          ...snapshot,
          lifecycle: {
            stages: [
              {
                stage: 'discussion',
                nodeId: 'workflow-discussion',
                status: 'active',
                actionRequired: false,
                lastTransitionAt: '2026-04-16T14:00:00Z',
              },
            ],
          },
          handoffPackages: [makeHandoffPackage('project-1', 'auto:txn-001'), makeHandoffPackage('project-2', 'auto:txn-ghost')],
        }
      }

      return {
        ...snapshot,
        lifecycle: {
          stages: [
            {
              stage: 'discussion',
              nodeId: 'workflow-discussion',
              status: 'complete',
              actionRequired: false,
              lastTransitionAt: '2026-04-16T14:00:00Z',
            },
            {
              stage: 'research',
              nodeId: 'workflow-research',
              status: 'active',
              actionRequired: true,
              lastTransitionAt: '2026-04-16T14:01:00Z',
            },
          ],
        },
        handoffPackages: [makeHandoffPackage('project-1', 'auto:txn-002'), makeHandoffPackage('project-2', 'auto:txn-ghost')],
      }
    })

    vi.mocked(setup.getRuntimeRun).mockImplementation(async (projectId: string) => {
      if (projectId === 'project-1') {
        return makeRuntimeRun('project-1', { runId: autoDispatched ? 'run-project-1b' : 'run-project-1' })
      }

      return makeRuntimeRun(projectId, { runId: `run-${projectId}` })
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1'))
    expect(screen.getByTestId('workflow-has-lifecycle')).toHaveTextContent('true')
    expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent('discussion')
    expect(screen.getByTestId('workflow-lifecycle-percent')).toHaveTextContent('0')
    expect(screen.getByTestId('handoff-package-count')).toHaveTextContent('1')
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

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        run: makeRuntimeRun('project-1', { runId: 'run-project-1b' }),
      })
    })

    await waitFor(() => expect(setup.subscribeRuntimeStream).toHaveBeenCalledTimes(2))
    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent('run-project-1b'))
    await waitFor(() => expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent('research'))

    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1b')
    expect(screen.getByTestId('stream-item-count')).toHaveTextContent('0')
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('runtime_run:updated')
    expect(screen.getByTestId('workflow-lifecycle-percent')).toHaveTextContent('50')
    expect(screen.getByTestId('workflow-lifecycle-action-required')).toHaveTextContent('1')
    expect(screen.getByTestId('handoff-package-count')).toHaveTextContent('1')
    expect(vi.mocked(setup.getProjectSnapshot).mock.calls.length).toBeGreaterThan(snapshotCallsBeforeEvent)
  })

  it('advances lifecycle and handoff continuity to terminal roadmap state from runtime_run:updated refreshes', async () => {
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

    const runIds = ['run-project-1', 'run-project-1b', 'run-project-1c', 'run-project-1d'] as const
    const lifecycleSnapshots: ProjectSnapshotResponseDto['lifecycle']['stages'][] = [
      [
        {
          stage: 'discussion',
          nodeId: 'workflow-discussion',
          status: 'active',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:00:00Z',
        },
        {
          stage: 'research',
          nodeId: 'workflow-research',
          status: 'pending',
          actionRequired: false,
          lastTransitionAt: null,
        },
        {
          stage: 'requirements',
          nodeId: 'workflow-requirements',
          status: 'pending',
          actionRequired: false,
          lastTransitionAt: null,
        },
        {
          stage: 'roadmap',
          nodeId: 'workflow-roadmap',
          status: 'pending',
          actionRequired: false,
          lastTransitionAt: null,
        },
      ],
      [
        {
          stage: 'discussion',
          nodeId: 'workflow-discussion',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:00:00Z',
        },
        {
          stage: 'research',
          nodeId: 'workflow-research',
          status: 'active',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:01:00Z',
        },
        {
          stage: 'requirements',
          nodeId: 'workflow-requirements',
          status: 'pending',
          actionRequired: false,
          lastTransitionAt: null,
        },
        {
          stage: 'roadmap',
          nodeId: 'workflow-roadmap',
          status: 'pending',
          actionRequired: false,
          lastTransitionAt: null,
        },
      ],
      [
        {
          stage: 'discussion',
          nodeId: 'workflow-discussion',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:00:00Z',
        },
        {
          stage: 'research',
          nodeId: 'workflow-research',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:01:00Z',
        },
        {
          stage: 'requirements',
          nodeId: 'workflow-requirements',
          status: 'active',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:02:00Z',
        },
        {
          stage: 'roadmap',
          nodeId: 'workflow-roadmap',
          status: 'pending',
          actionRequired: false,
          lastTransitionAt: null,
        },
      ],
      [
        {
          stage: 'discussion',
          nodeId: 'workflow-discussion',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:00:00Z',
        },
        {
          stage: 'research',
          nodeId: 'workflow-research',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:01:00Z',
        },
        {
          stage: 'requirements',
          nodeId: 'workflow-requirements',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:02:00Z',
        },
        {
          stage: 'roadmap',
          nodeId: 'workflow-roadmap',
          status: 'complete',
          actionRequired: false,
          lastTransitionAt: '2026-04-16T14:03:00Z',
        },
      ],
    ]

    let snapshotIndex = 0
    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'cadence' : 'orchestra')
      if (projectId !== 'project-1') {
        return snapshot
      }

      const handoffPackages = [
        ...runIds.slice(0, snapshotIndex + 1).map((_runId, index) => ({
          ...makeHandoffPackage('project-1', `auto:txn-00${index + 1}`),
          id: index + 1,
        })),
        {
          ...makeHandoffPackage('project-2', `auto:txn-ghost-${snapshotIndex + 1}`),
          id: 100 + snapshotIndex,
        },
      ]

      return {
        ...snapshot,
        lifecycle: {
          stages: lifecycleSnapshots[snapshotIndex],
        },
        handoffPackages,
        approvalRequests: [],
      }
    })

    vi.mocked(setup.getRuntimeRun).mockImplementation(async (projectId: string) => {
      if (projectId === 'project-1') {
        return makeRuntimeRun('project-1', {
          runId: runIds[snapshotIndex],
          lastCheckpointSequence: snapshotIndex + 2,
          updatedAt: `2026-04-16T14:0${snapshotIndex}:10Z`,
        })
      }

      return makeRuntimeRun(projectId, { runId: `run-${projectId}` })
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent(runIds[0]))
    expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent('discussion')
    expect(screen.getByTestId('workflow-lifecycle-percent')).toHaveTextContent('0')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
    expect(screen.getByTestId('handoff-package-count')).toHaveTextContent('1')
    expect(screen.getByTestId('latest-handoff-transition-id')).toHaveTextContent('auto:txn-001')

    const expectedActiveStages = ['research', 'requirements', 'none'] as const
    const expectedLifecyclePercents = ['25', '50', '100'] as const

    for (let nextIndex = 1; nextIndex < runIds.length; nextIndex += 1) {
      const nextRunId = runIds[nextIndex]

      vi.mocked(setup.subscribeRuntimeStream).mockResolvedValueOnce({
        response: makeStreamResponse('project-1', {
          runId: nextRunId,
          sessionId: 'session-1',
          flowId: 'flow-1',
        }),
        unsubscribe: vi.fn(),
      })

      snapshotIndex = nextIndex
      act(() => {
        setup.emitRuntimeRunUpdated({
          projectId: 'project-1',
          run: makeRuntimeRun('project-1', {
            runId: nextRunId,
            lastCheckpointSequence: nextIndex + 2,
            updatedAt: `2026-04-16T14:1${nextIndex}:00Z`,
          }),
        })
      })

      await waitFor(() => expect(screen.getByTestId('stream-run-id')).toHaveTextContent(nextRunId))
      await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent(nextRunId))
      await waitFor(() =>
        expect(screen.getByTestId('workflow-active-lifecycle-stage')).toHaveTextContent(expectedActiveStages[nextIndex - 1]),
      )

      expect(screen.getByTestId('refresh-source')).toHaveTextContent('runtime_run:updated')
      expect(screen.getByTestId('workflow-lifecycle-percent')).toHaveTextContent(expectedLifecyclePercents[nextIndex - 1])
      expect(screen.getByTestId('workflow-lifecycle-action-required')).toHaveTextContent('0')
      expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
      expect(screen.getByTestId('handoff-package-count')).toHaveTextContent(String(nextIndex + 1))
      expect(screen.getByTestId('latest-handoff-transition-id')).toHaveTextContent(`auto:txn-00${nextIndex + 1}`)
    }

    expect(vi.mocked(setup.subscribeRuntimeStream).mock.calls.length).toBeGreaterThanOrEqual(4)
  })

  it('hydrates gate-linked pending approvals from durable snapshot truth on project:updated refresh', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'cadence')] },
    })

    let includeGatePause = false
    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'cadence' : 'orchestra')

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
        project: makeProjectSummary('project-1', 'cadence'),
        reason: 'metadata_changed',
      })
    })

    await waitFor(() => expect(screen.getByTestId('refresh-source')).toHaveTextContent('project:updated'))
    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('stream-action-required-count')).toHaveTextContent('0')
    expect(screen.getByTestId('checkpoint-loop-count')).toHaveTextContent('1')
    expect(screen.getByTestId('checkpoint-loop-first-action-id')).toHaveTextContent(
      'scope:auto-dispatch:workflow-research:requires_user_input',
    )
    expect(screen.getByTestId('checkpoint-loop-first-truth-source')).toHaveTextContent('durable_only')
    expect(screen.getByTestId('checkpoint-loop-first-durable-state')).toHaveTextContent('Pending approval')
    expect(screen.getByTestId('first-approval-resume-state')).toHaveTextContent('waiting')
  })

  it('keeps gate pauses visible until resume succeeds, then clears pending state from refreshed snapshot truth', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'cadence')] },
    })

    let snapshotStage: 'pending' | 'cleared' = 'pending'
    vi.mocked(setup.getProjectSnapshot).mockImplementation(async (projectId: string) => {
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'cadence' : 'orchestra')

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
      listProjects: { projects: [makeProjectSummary('project-1', 'cadence')] },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'cadence'),
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
    expect(screen.getByTestId('checkpoint-loop-count')).toHaveTextContent('1')
    expect(screen.getByTestId('checkpoint-loop-first-truth-source')).toHaveTextContent('durable_only')
    expect(screen.getByTestId('checkpoint-loop-first-broker-state')).toHaveTextContent('1 broker failure')
  })

  it('surfaces broker command failures without clearing pending approvals or resume history', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'cadence')] },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'cadence'),
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
        'project-1': new CadenceDesktopError({
          code: 'notification_dispatch_query_failed',
          errorClass: 'retryable',
          message: 'Cadence could not load notification dispatches for this project.',
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
      'Cadence could not load notification dispatches for this project.',
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
        projects: [makeProjectSummary('project-1', 'cadence'), makeProjectSummary('project-2', 'orchestra')],
      },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'cadence'),
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
      new CadenceDesktopError({
        code: 'project_snapshot_query_failed',
        errorClass: 'retryable',
        message: 'Cadence could not reload project-2 during selected-project refresh.',
        retryable: true,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() =>
      expect(screen.getByTestId('error')).toHaveTextContent(
        'Cadence could not reload project-2 during selected-project refresh.',
      ),
    )
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1')
    expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-1')
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('1')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('1')

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-project-2'))
    expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0')
    expect(screen.getByTestId('resume-history-count')).toHaveTextContent('0')
    expect(screen.getByTestId('error')).toHaveTextContent('none')
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('selection')
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
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'cadence' : 'orchestra')

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
              gateNodeId: null,
              gateKey: null,
              transitionFromNodeId: null,
              transitionToNodeId: null,
              transitionKind: null,
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
        ? makeBlockedAutonomousRunState(projectId, 'boundary-1')
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
    await waitFor(() => expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('true'))
    expect(screen.getByTestId('checkpoint-loop-count')).toHaveTextContent('1')
    expect(screen.getByTestId('checkpoint-loop-first-action-id')).toHaveTextContent(actionId)
    expect(screen.getByTestId('checkpoint-loop-first-truth-source')).toHaveTextContent('live_and_durable')
    expect(screen.getByTestId('checkpoint-loop-first-live-state')).toHaveTextContent('Live action required')
    expect(screen.getByTestId('checkpoint-loop-first-resume-state')).toHaveTextContent('Resume failed')
    expect(screen.getByTestId('sync-polling-action-id')).toHaveTextContent(actionId)
    expect(screen.getByTestId('sync-polling-boundary-id')).toHaveTextContent('boundary-1')
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
    await new Promise((resolve) => setTimeout(resolve, 200))
    expect(project1SyncCount()).toBe(syncCallsAfterImmediateRefresh)

    await waitFor(() => expect(project1SyncCount()).toBeGreaterThan(syncCallsAfterImmediateRefresh), {
      timeout: BLOCKED_NOTIFICATION_SYNC_POLL_MS + 800,
    })

    snapshotMode = 'resolved'

    await waitFor(() => expect(screen.getByTestId('pending-approval-count')).toHaveTextContent('0'), {
      timeout: BLOCKED_NOTIFICATION_SYNC_POLL_MS + 800,
    })
    await waitFor(() => expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('false'))

    const syncCallsAfterBoundaryClear = project1SyncCount()
    await new Promise((resolve) => setTimeout(resolve, BLOCKED_NOTIFICATION_SYNC_POLL_MS + 250))
    expect(project1SyncCount()).toBe(syncCallsAfterBoundaryClear)
  })

  it('stops blocked-checkpoint sync polling when the active project changes', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [makeProjectSummary('project-1', 'cadence'), makeProjectSummary('project-2', 'orchestra')],
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
      const snapshot = makeSnapshot(projectId, projectId === 'project-1' ? 'cadence' : 'orchestra')

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
              gateNodeId: null,
              gateKey: null,
              transitionFromNodeId: null,
              transitionToNodeId: null,
              transitionKind: null,
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
        return makeBlockedAutonomousRunState(projectId, 'boundary-1')
      }

      return makeRecoveredAutonomousRunState(projectId)
    })

    render(<Harness adapter={setup.adapter} />)

    const project1SyncCount = () =>
      vi
        .mocked(setup.syncNotificationAdapters)
        .mock.calls.filter(([projectId]) => projectId === 'project-1').length

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
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

    await waitFor(() => expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('true'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    await waitFor(() => expect(screen.getByTestId('sync-polling-active')).toHaveTextContent('false'))

    const project1SyncCallsAfterSwitch = project1SyncCount()
    await new Promise((resolve) => setTimeout(resolve, BLOCKED_NOTIFICATION_SYNC_POLL_MS + 250))
    expect(project1SyncCount()).toBe(project1SyncCallsAfterSwitch)
  })

  it('loads route health, then keeps the last truthful route list when refresh fails', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'cadence'),
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
      new CadenceDesktopError({
        code: 'notification_route_query_failed',
        errorClass: 'retryable',
        message: 'Cadence could not load notification routes for this project.',
        retryable: true,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Refresh notification routes' }))

    await waitFor(() =>
      expect(screen.getByTestId('route-load-error')).toHaveTextContent(
        'Cadence could not load notification routes for this project.',
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
                message: 'Cadence is missing app-local Discord botToken credentials.',
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
          credentialReadiness: null,
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
    expect(screen.getByTestId('trust-state')).toHaveTextContent('degraded')
    expect(screen.getByTestId('trust-ready-credential-count')).toHaveTextContent('1')
    expect(screen.getByTestId('trust-missing-credential-count')).toHaveTextContent('1')
    expect(screen.getByTestId('trust-projection-error')).not.toHaveTextContent('none')
  })

  it('runs notification adapter sync during selected-project refreshes and exposes one-reply-wins cycle counts', async () => {
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

    await waitFor(() => expect(setup.syncNotificationAdapters).toHaveBeenCalledWith('project-1'))
    expect(screen.getByTestId('sync-dispatch-attempted')).toHaveTextContent('2')
    expect(screen.getByTestId('sync-reply-accepted')).toHaveTextContent('1')
    expect(screen.getByTestId('sync-reply-rejected')).toHaveTextContent('1')
    expect(screen.getByTestId('sync-error')).toHaveTextContent('none')
  })

  it('keeps the last truthful broker + sync summary when a later sync cycle fails', async () => {
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
      notificationSyncResponses: {
        'project-1': makeNotificationSyncResponse('project-1', {
          dispatch: {
            projectId: 'project-1',
            pendingCount: 1,
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
            routeCount: 1,
            polledRouteCount: 1,
            messageCount: 1,
            acceptedCount: 1,
            rejectedCount: 0,
            attemptLimit: 256,
            attemptsTruncated: false,
            attempts: [],
            errorCodeCounts: [],
          },
          syncedAt: '2026-04-17T03:10:00Z',
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('sync-dispatch-attempted')).toHaveTextContent('1'))
    expect(screen.getByTestId('broker-dispatch-count')).toHaveTextContent('1')

    setup.syncNotificationAdapters.mockRejectedValueOnce(
      new CadenceDesktopError({
        code: 'notification_adapter_sync_failed',
        errorClass: 'retryable',
        message: 'Cadence could not sync notification adapters for this project.',
        retryable: true,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Retry state' }))

    await waitFor(() =>
      expect(screen.getByTestId('sync-error')).toHaveTextContent(
        'Cadence could not sync notification adapters for this project.',
      ),
    )
    expect(screen.getByTestId('sync-dispatch-attempted')).toHaveTextContent('1')
    expect(screen.getByTestId('broker-dispatch-count')).toHaveTextContent('1')
  })

  it('disables one route without mutating unrelated rows or approval state', async () => {
    const actionId = 'scope:auto-dispatch:workflow-research:requires_user_input'
    const setup = createMockAdapter({
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'cadence'),
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
        projects: [makeProjectSummary('project-1', 'cadence'), makeProjectSummary('project-2', 'orchestra')],
      },
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'cadence'),
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
        new CadenceDesktopError({
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
