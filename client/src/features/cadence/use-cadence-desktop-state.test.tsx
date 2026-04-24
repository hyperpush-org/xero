import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import {
  type ImportRepositoryResponseDto,
  type ListNotificationDispatchesResponseDto,
  type ListNotificationRoutesResponseDto,
  type ListProjectsResponseDto,
  type ProjectSnapshotResponseDto,
  type ProjectUpdatedPayloadDto,
  type ProviderModelCatalogDto,
  type ProviderProfilesDto,
  type RepositoryDiffResponseDto,
  type RepositoryStatusChangedPayloadDto,
  type RepositoryStatusResponseDto,
  type ResolveOperatorActionResponseDto,
  type ResumeOperatorRunResponseDto,
  type AutonomousRunStateDto,
  type RuntimeRunControlInputDto,
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
    anthropicApiKeyConfigured: false,
    ...overrides,
  }
}

function makeProviderProfiles(overrides: Partial<ProviderProfilesDto> = {}): ProviderProfilesDto {
  const activeProfileId = overrides.activeProfileId ?? 'openai_codex-default'
  const profiles = overrides.profiles ?? [
    {
      profileId: 'openai_codex-default',
      providerId: 'openai_codex',
      label: 'OpenAI Codex',
      modelId: 'openai_codex',
      active: activeProfileId === 'openai_codex-default',
      readiness: {
        ready: false,
        status: 'missing',
        credentialUpdatedAt: null,
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

function makeProviderProfilesFromRuntimeSettings(
  runtimeSettings: RuntimeSettingsDto,
  options: { profileId?: string; label?: string } = {},
): ProviderProfilesDto {
  const profileId =
    options.profileId ?? (runtimeSettings.providerId === 'openrouter' ? 'openrouter-default' : 'openai_codex-default')
  const providerLabel = options.label ?? (runtimeSettings.providerId === 'openrouter' ? 'OpenRouter' : 'OpenAI Codex')

  const activeProfile: ProviderProfilesDto['profiles'][number] = {
    profileId,
    providerId: runtimeSettings.providerId,
    label: providerLabel,
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
  }

  const inactiveOpenRouterProfile: ProviderProfilesDto['profiles'] =
    runtimeSettings.providerId === 'openrouter'
      ? []
      : [
          {
            profileId: 'openrouter-default',
            providerId: 'openrouter',
            label: 'OpenRouter',
            modelId: 'openai/gpt-4.1-mini',
            active: false,
            readiness: {
              ready: runtimeSettings.openrouterApiKeyConfigured,
              status: runtimeSettings.openrouterApiKeyConfigured ? 'ready' : 'missing',
              credentialUpdatedAt: runtimeSettings.openrouterApiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ]

  return makeProviderProfiles({
    activeProfileId: profileId,
    profiles: [activeProfile, ...inactiveOpenRouterProfile],
  })
}

function makeProviderModelCatalog(
  profileId: string,
  overrides: Partial<ProviderModelCatalogDto> = {},
): ProviderModelCatalogDto {
  const providerId = overrides.providerId ?? (profileId.startsWith('openrouter') ? 'openrouter' : 'openai_codex')
  const configuredModelId =
    overrides.configuredModelId ??
    (providerId === 'openrouter' ? 'openai/gpt-4.1-mini' : 'openai_codex')

  return {
    profileId,
    providerId,
    configuredModelId,
    source: overrides.source ?? 'live',
    fetchedAt: overrides.fetchedAt ?? '2026-04-21T12:00:00Z',
    lastSuccessAt: overrides.lastSuccessAt ?? '2026-04-21T12:00:00Z',
    lastRefreshError: overrides.lastRefreshError ?? null,
    models:
      overrides.models ??
      (providerId === 'openrouter'
        ? [
            {
              modelId: configuredModelId,
              displayName: 'OpenAI GPT-4.1 Mini',
              thinking: {
                supported: true,
                effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                defaultEffort: 'medium',
              },
            },
            {
              modelId: 'anthropic/claude-3.7-sonnet',
              displayName: 'Claude 3.7 Sonnet',
              thinking: {
                supported: false,
                effortOptions: [],
                defaultEffort: null,
              },
            },
          ]
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
          ]),
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
    controls: {
      active: {
        modelId: 'openai_codex',
        thinkingEffort: 'medium',
        approvalMode: 'suggest',
        revision: 1,
        appliedAt: '2026-04-15T20:00:00Z',
      },
      pending: null,
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
  providerProfiles?: ProviderProfilesDto
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogErrors?: Record<string, Error>
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
  const currentProviderProfiles = {
    value: options?.providerProfiles ?? makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings.value),
  }
  const currentProviderModelCatalogs = {
    value:
      options?.providerModelCatalogs ??
      Object.fromEntries(
        currentProviderProfiles.value.profiles.map((profile) => [
          profile.profileId,
          makeProviderModelCatalog(profile.profileId, {
            providerId: profile.providerId,
            configuredModelId: profile.modelId,
            source: profile.providerId === 'openrouter' && !profile.readiness.ready ? 'unavailable' : 'live',
            fetchedAt: profile.providerId === 'openrouter' && !profile.readiness.ready ? null : '2026-04-21T12:00:00Z',
            lastSuccessAt:
              profile.providerId === 'openrouter' && !profile.readiness.ready ? null : '2026-04-21T12:00:00Z',
            models:
              profile.providerId === 'openrouter' && !profile.readiness.ready
                ? []
                : undefined,
          }),
        ]),
      ),
  }
  const providerModelCatalogErrors = options?.providerModelCatalogErrors ?? {}
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
  const getProviderModelCatalog = vi.fn(
    async (
      profileId: string,
      options?: { forceRefresh?: boolean },
    ): Promise<ProviderModelCatalogDto> => {
      const error = providerModelCatalogErrors[profileId]
      if (error) {
        throw error
      }

      const currentProfile = currentProviderProfiles.value.profiles.find((profile) => profile.profileId === profileId)
      if (!currentProfile) {
        throw new CadenceDesktopError({
          code: 'provider_profile_not_found',
          errorClass: 'user_fixable',
          message: `Cadence could not find provider profile \`${profileId}\`.`,
        })
      }

      const existingCatalog = currentProviderModelCatalogs.value[profileId]
      if (!options?.forceRefresh && existingCatalog) {
        return existingCatalog
      }

      const nextCatalog =
        currentProfile.providerId === 'openrouter' && !currentProfile.readiness.ready
          ? makeProviderModelCatalog(profileId, {
              providerId: currentProfile.providerId,
              configuredModelId: currentProfile.modelId,
              source: 'unavailable',
              fetchedAt: null,
              lastSuccessAt: null,
              lastRefreshError: {
                code: 'openrouter_credentials_missing',
                message: 'Configure an OpenRouter API key before refreshing provider models.',
                retryable: false,
              },
              models: [],
            })
          : makeProviderModelCatalog(profileId, {
              providerId: currentProfile.providerId,
              configuredModelId: currentProfile.modelId,
            })

      currentProviderModelCatalogs.value = {
        ...currentProviderModelCatalogs.value,
        [profileId]: nextCatalog,
      }
      return nextCatalog
    },
  )
  const getProviderProfiles = vi.fn(async () => currentProviderProfiles.value)
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
    currentProviderProfiles.value = makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings.value)

    return currentRuntimeSettings.value
  })
  const upsertProviderProfile = vi.fn(async (request: {
    profileId: string
    providerId: RuntimeSettingsDto['providerId']
    runtimeKind: string
    label: string
    modelId: string
    presetId?: string | null
    baseUrl?: string | null
    apiVersion?: string | null
    apiKey?: string | null
    activate?: boolean
  }) => {
    const existingProfiles = currentProviderProfiles.value.profiles.filter(
      (profile) => profile.profileId !== request.profileId,
    )
    const apiKeyConfigured =
      request.providerId === 'openai_codex'
        ? false
        : request.apiKey == null
          ? currentProviderProfiles.value.profiles.find((profile) => profile.profileId === request.profileId)?.readiness.ready ?? false
          : request.apiKey.trim().length > 0

    const nextActiveProfileId = request.activate
      ? request.profileId
      : currentProviderProfiles.value.activeProfileId
    const nextProfile: ProviderProfilesDto['profiles'][number] = {
      profileId: request.profileId,
      providerId: request.providerId,
      runtimeKind: request.runtimeKind as ProviderProfilesDto['profiles'][number]['runtimeKind'],
      label: request.label,
      modelId: request.modelId,
      presetId: request.presetId ?? null,
      baseUrl: request.baseUrl ?? null,
      apiVersion: request.apiVersion ?? null,
      active: nextActiveProfileId === request.profileId,
      readiness:
        request.providerId === 'openai_codex'
          ? {
              ready: false,
              status: 'missing',
              credentialUpdatedAt: null,
            }
          : {
              ready: apiKeyConfigured,
              status: apiKeyConfigured ? 'ready' : 'missing',
              credentialUpdatedAt: apiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
            },
      migratedFromLegacy: false,
      migratedAt: null,
    }

    currentProviderProfiles.value = {
      activeProfileId: nextActiveProfileId,
      profiles: [nextProfile, ...existingProfiles].map<ProviderProfilesDto['profiles'][number]>((profile) => ({
        ...profile,
        active: profile.profileId === nextActiveProfileId,
      })),
      migration: currentProviderProfiles.value.migration ?? null,
    }
    currentRuntimeSettings.value = {
      providerId:
        currentProviderProfiles.value.profiles.find((profile) => profile.profileId === nextActiveProfileId)?.providerId ??
        currentRuntimeSettings.value.providerId,
      modelId:
        currentProviderProfiles.value.profiles.find((profile) => profile.profileId === nextActiveProfileId)?.modelId ??
        currentRuntimeSettings.value.modelId,
      openrouterApiKeyConfigured: currentProviderProfiles.value.profiles.some(
        (profile) => profile.providerId === 'openrouter' && profile.readiness.ready,
      ),
      anthropicApiKeyConfigured: currentProviderProfiles.value.profiles.some(
        (profile) => profile.providerId === 'anthropic' && profile.readiness.ready,
      ),
    }

    return currentProviderProfiles.value
  })
  const setActiveProviderProfile = vi.fn(async (profileId: string) => {
    currentProviderProfiles.value = {
      ...currentProviderProfiles.value,
      activeProfileId: profileId,
      profiles: currentProviderProfiles.value.profiles.map((profile) => ({
        ...profile,
        active: profile.profileId === profileId,
      })),
    }
    const activeProfile = currentProviderProfiles.value.profiles.find((profile) => profile.profileId === profileId)
    if (activeProfile) {
      currentRuntimeSettings.value = {
        providerId: activeProfile.providerId,
        modelId: activeProfile.modelId,
        openrouterApiKeyConfigured: currentProviderProfiles.value.profiles.some(
          (profile) => profile.providerId === 'openrouter' && profile.readiness.ready,
        ),
        anthropicApiKeyConfigured: currentProviderProfiles.value.profiles.some(
          (profile) => profile.providerId === 'anthropic' && profile.readiness.ready,
        ),
      }
    }

    return currentProviderProfiles.value
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
  const startOpenAiLogin = vi.fn(
    async (
      projectId: string,
      _options: { selectedProfileId: string; originator?: string | null },
    ) =>
      makeRuntimeSession(projectId, {
        sessionId: null,
        phase: 'awaiting_browser_callback',
        lastErrorCode: null,
        lastError: null,
      }),
  )
  const submitOpenAiCallback = vi.fn(
    async (
      projectId: string,
      flowId: string,
      _options: { selectedProfileId: string; manualInput?: string | null },
    ) => makeRuntimeSession(projectId, { flowId, phase: 'authenticated' }),
  )
  const startAutonomousRun = vi.fn(async (projectId: string) => {
    const nextState = makeAutonomousRunState(projectId)
    autonomousStates[projectId] = nextState
    return nextState
  })
  const startRuntimeRun = vi.fn(async (projectId: string) => runtimeRuns[projectId] ?? makeRuntimeRun(projectId))
  const updateRuntimeRunControls = vi.fn(
    async (request: {
      projectId: string
      runId: string
      controls?: RuntimeRunControlInputDto | null
      prompt?: string | null
    }): Promise<RuntimeRunDto> => {
      const currentRun = runtimeRuns[request.projectId] ?? makeRuntimeRun(request.projectId, { runId: request.runId })
      const queuedAt = '2026-04-15T20:00:07Z'
      const activeControls = currentRun.controls.active
      const pendingControls = request.controls
        ? {
            modelId: request.controls.modelId,
            thinkingEffort: request.controls.thinkingEffort ?? null,
            approvalMode: request.controls.approvalMode,
            revision: activeControls.revision + 1,
            queuedAt,
            queuedPrompt: request.prompt ?? null,
            queuedPromptAt: request.prompt ? queuedAt : null,
          }
        : currentRun.controls.pending

      const nextRun = {
        ...currentRun,
        controls: {
          active: currentRun.controls.active,
          pending: pendingControls,
        },
        updatedAt: queuedAt,
      }
      runtimeRuns[request.projectId] = nextRun
      return nextRun
    },
  )
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

  const listProjectFiles = vi.fn(async (projectId: string) => ({
    projectId,
    root: {
      name: 'root',
      path: '/',
      type: 'folder' as const,
      children: [],
    },
  }))
  const readProjectFile = vi.fn(async (projectId: string, path: string) => ({ projectId, path, content: '' }))
  const writeProjectFile = vi.fn(async (projectId: string, path: string) => ({ projectId, path }))
  const createProjectEntry = vi.fn(async (request) => ({
    projectId: request.projectId,
    path: request.parentPath === '/' ? `/${request.name}` : `${request.parentPath}/${request.name}`,
  }))
  const renameProjectEntry = vi.fn(async (request) => ({
    projectId: request.projectId,
    path: request.path.split('/').slice(0, -1).filter(Boolean).length
      ? `/${request.path.split('/').slice(0, -1).filter(Boolean).join('/')}/${request.newName}`
      : `/${request.newName}`,
  }))
  const deleteProjectEntry = vi.fn(async (projectId: string, path: string) => ({ projectId, path }))
  const searchProject = vi.fn(async (request: { projectId: string }) => ({
    projectId: request.projectId,
    totalMatches: 0,
    totalFiles: 0,
    truncated: false,
    files: [],
  }))
  const replaceInProject = vi.fn(async (request: { projectId: string }) => ({
    projectId: request.projectId,
    filesChanged: 0,
    totalReplacements: 0,
  }))

  const adapter: CadenceDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder,
    importRepository,
    listProjects,
    removeProject,
    getProjectSnapshot,
    getRepositoryStatus,
    getRepositoryDiff,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
    getAutonomousRun,
    getRuntimeRun,
    getRuntimeSession,
    getRuntimeSettings,
    getProviderModelCatalog,
    getProviderProfiles,
    startOpenAiLogin,
    submitOpenAiCallback,
    startAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    startRuntimeSession,
    cancelAutonomousRun,
    stopRuntimeRun,
    logoutRuntimeSession,
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
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
    browserEval: vi.fn(async () => undefined),
    browserCurrentUrl: vi.fn(async () => null),
    browserScreenshot: vi.fn(async () => ''),
    browserNavigate: vi.fn(async () => undefined),
    browserBack: vi.fn(async () => undefined),
    browserForward: vi.fn(async () => undefined),
    browserReload: vi.fn(async () => undefined),
    browserStop: vi.fn(async () => undefined),
    browserClick: vi.fn(async () => undefined),
    browserType: vi.fn(async () => undefined),
    browserScroll: vi.fn(async () => undefined),
    browserPressKey: vi.fn(async () => undefined),
    browserReadText: vi.fn(async () => undefined),
    browserQuery: vi.fn(async () => undefined),
    browserWaitForSelector: vi.fn(async () => undefined),
    browserWaitForLoad: vi.fn(async () => undefined),
    browserHistoryState: vi.fn(async () => undefined),
    browserCookiesGet: vi.fn(async () => undefined),
    browserCookiesSet: vi.fn(async () => undefined),
    browserStorageRead: vi.fn(async () => undefined),
    browserStorageWrite: vi.fn(async () => undefined),
    browserStorageClear: vi.fn(async () => undefined),
    browserTabList: vi.fn(async () => []),
    browserTabFocus: vi.fn(async () => ({
      id: 'tab-1',
      label: 'cadence-browser',
      title: null,
      url: null,
      loading: false,
      canGoBack: false,
      canGoForward: false,
      active: true,
    })),
    browserTabClose: vi.fn(async () => []),
    onBrowserUrlChanged: vi.fn(async () => () => undefined),
    onBrowserLoadState: vi.fn(async () => () => undefined),
    onBrowserConsole: vi.fn(async () => () => undefined),
    onBrowserTabUpdated: vi.fn(async () => () => undefined),
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
    getProviderModelCatalog,
    getProviderProfiles,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
    listNotificationRoutes,
    listNotificationDispatches,
    syncNotificationAdapters: adapter.syncNotificationAdapters,
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
    setProviderProfiles(nextProviderProfiles: ProviderProfilesDto) {
      currentProviderProfiles.value = nextProviderProfiles
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
      <div data-testid="selected-provider-source">{state.agentView?.selectedProviderSource ?? 'none'}</div>
      <div data-testid="selected-model-id">{state.agentView?.selectedModelId ?? 'none'}</div>
      <div data-testid="provider-mismatch">{String(state.agentView?.providerMismatch ?? false)}</div>
      <div data-testid="provider-mismatch-reason">{state.agentView?.providerMismatchReason ?? 'none'}</div>
      <div data-testid="provider-mismatch-recovery-copy">{state.agentView?.providerMismatchRecoveryCopy ?? 'none'}</div>
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
      <div data-testid="provider-profiles-active-profile-id">{state.providerProfiles?.activeProfileId ?? 'none'}</div>
      <div data-testid="provider-profiles-count">{String(state.providerProfiles?.profiles.length ?? 0)}</div>
      <div data-testid="provider-profiles-selected-profile-id">{state.agentView?.selectedProfileId ?? 'none'}</div>
      <div data-testid="provider-profiles-selected-profile-label">{state.agentView?.selectedProfileLabel ?? 'none'}</div>
      <div data-testid="provider-profiles-selected-readiness-status">{state.agentView?.selectedProfileReadiness?.status ?? 'none'}</div>
      <div data-testid="provider-profiles-load-status">{state.providerProfilesLoadStatus}</div>
      <div data-testid="provider-profiles-load-error-code">{state.providerProfilesLoadError?.code ?? 'none'}</div>
      <div data-testid="provider-profiles-load-error-message">{state.providerProfilesLoadError?.message ?? 'none'}</div>
      <div data-testid="provider-profiles-save-status">{state.providerProfilesSaveStatus}</div>
      <div data-testid="provider-profiles-save-error-code">{state.providerProfilesSaveError?.code ?? 'none'}</div>
      <div data-testid="provider-profiles-save-error-message">{state.providerProfilesSaveError?.message ?? 'none'}</div>
      <div data-testid="provider-model-catalog-count">{String(Object.keys(state.providerModelCatalogs).length)}</div>
      <div data-testid="provider-model-catalog-active-source">{state.activeProviderModelCatalog?.source ?? 'none'}</div>
      <div data-testid="provider-model-catalog-active-profile-id">{state.activeProviderModelCatalog?.profileId ?? 'none'}</div>
      <div data-testid="provider-model-catalog-active-provider-id">{state.activeProviderModelCatalog?.providerId ?? 'none'}</div>
      <div data-testid="provider-model-catalog-active-configured-model-id">{state.activeProviderModelCatalog?.configuredModelId ?? 'none'}</div>
      <div data-testid="provider-model-catalog-active-model-count">{String(state.activeProviderModelCatalog?.models.length ?? 0)}</div>
      <div data-testid="provider-model-catalog-active-model-ids">{state.activeProviderModelCatalog?.models.map((model) => model.modelId).join(',') ?? 'none'}</div>
      <div data-testid="provider-model-catalog-active-load-status">{state.activeProviderModelCatalogLoadStatus}</div>
      <div data-testid="provider-model-catalog-active-load-error-code">{state.activeProviderModelCatalogLoadError?.code ?? 'none'}</div>
      <div data-testid="provider-model-catalog-openrouter-source">{state.providerModelCatalogs['openrouter-default']?.source ?? 'none'}</div>
      <div data-testid="provider-model-catalog-openrouter-configured-model-id">{state.providerModelCatalogs['openrouter-default']?.configuredModelId ?? 'none'}</div>
      <div data-testid="provider-model-catalog-openrouter-load-status">{state.providerModelCatalogLoadStatuses['openrouter-default'] ?? 'idle'}</div>
      <div data-testid="provider-model-catalog-openrouter-load-error-code">{state.providerModelCatalogLoadErrors['openrouter-default']?.code ?? 'none'}</div>
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
          void state.listProjectFiles('project-1').catch(() => undefined)
        }}
        type="button"
      >
        List project files
      </button>
      <button
        onClick={() => {
          void state.readProjectFile('project-1', '/README.md').catch(() => undefined)
        }}
        type="button"
      >
        Read README
      </button>
      <button
        onClick={() => {
          void state.writeProjectFile('project-1', '/README.md', '# Cadence').catch(() => undefined)
        }}
        type="button"
      >
        Write README
      </button>
      <button
        onClick={() => {
          void state
            .createProjectEntry({
              projectId: 'project-1',
              parentPath: '/',
              name: 'notes.md',
              entryType: 'file',
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Create notes file
      </button>
      <button
        onClick={() => {
          void state
            .renameProjectEntry({
              projectId: 'project-1',
              path: '/notes.md',
              newName: 'notes-2.md',
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Rename notes file
      </button>
      <button
        onClick={() => {
          void state.deleteProjectEntry('project-1', '/notes-2.md').catch(() => undefined)
        }}
        type="button"
      >
        Delete notes file
      </button>
      <button
        onClick={() => {
          void state.startOpenAiLogin().catch(() => undefined)
        }}
        type="button"
      >
        Start OpenAI login
      </button>
      <button
        onClick={() => {
          void state
            .submitOpenAiCallback('flow-1', { manualInput: 'browser-callback-token' })
            .catch(() => undefined)
        }}
        type="button"
      >
        Submit OpenAI callback
      </button>
      <button
        onClick={() => {
          void state.startRuntimeSession().catch(() => undefined)
        }}
        type="button"
      >
        Start runtime session
      </button>
      <button
        onClick={() => {
          void state.logoutRuntimeSession().catch(() => undefined)
        }}
        type="button"
      >
        Logout runtime session
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
          void state.refreshProviderProfiles({ force: true }).catch(() => undefined)
        }}
        type="button"
      >
        Load provider profiles
      </button>
      <button
        onClick={() => {
          void state.refreshProviderModelCatalog('openrouter-default', { force: true }).catch(() => undefined)
        }}
        type="button"
      >
        Refresh OpenRouter provider-model catalog
      </button>
      <button
        onClick={() => {
          void state.refreshProviderModelCatalog('openai_codex-default', { force: true }).catch(() => undefined)
        }}
        type="button"
      >
        Refresh OpenAI provider-model catalog
      </button>
      <button
        onClick={() => {
          void state
            .upsertProviderProfile({
              profileId: 'openrouter-default',
              providerId: 'openrouter',
              runtimeKind: 'openrouter',
              label: 'OpenRouter',
              modelId: 'openai/gpt-4.1-mini',
              presetId: 'openrouter',
              baseUrl: null,
              apiVersion: null,
              apiKey: 'sk-or-v1-test-secret',
              activate: true,
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Save OpenRouter provider profile
      </button>
      <button
        onClick={() => {
          void state.setActiveProviderProfile('openai_codex-default').catch(() => undefined)
        }}
        type="button"
      >
        Activate OpenAI provider profile
      </button>
      <button
        onClick={() => {
          void state
            .upsertProviderProfile({
              profileId: 'github_models-default',
              providerId: 'github_models',
              runtimeKind: 'openai_compatible',
              label: 'GitHub Models',
              modelId: 'openai/gpt-4.1',
              presetId: 'github_models',
              baseUrl: null,
              apiVersion: null,
              apiKey: 'ghp_test_secret',
              activate: true,
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Save GitHub provider profile
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
    expect(setup.listNotificationRoutes).toHaveBeenCalledWith('project-1')
    expect(setup.syncNotificationAdapters).toHaveBeenCalledWith('project-1')
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
    const routeRefreshesBeforeEvent = setup.listNotificationRoutes.mock.calls.length
    const syncNotificationAdaptersMock = setup.syncNotificationAdapters as ReturnType<typeof vi.fn>
    const syncRefreshesBeforeEvent = syncNotificationAdaptersMock.mock.calls.length

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
    expect(setup.listNotificationRoutes.mock.calls.length).toBeGreaterThan(routeRefreshesBeforeEvent)
    expect(syncNotificationAdaptersMock.mock.calls.length).toBeGreaterThan(syncRefreshesBeforeEvent)
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

  it('forwards execution file actions to the adapter with the requested project payloads', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'List project files' }))
    fireEvent.click(screen.getByRole('button', { name: 'Read README' }))
    fireEvent.click(screen.getByRole('button', { name: 'Write README' }))
    fireEvent.click(screen.getByRole('button', { name: 'Create notes file' }))
    fireEvent.click(screen.getByRole('button', { name: 'Rename notes file' }))
    fireEvent.click(screen.getByRole('button', { name: 'Delete notes file' }))

    await waitFor(() => expect(setup.listProjectFiles).toHaveBeenCalledWith('project-1'))
    expect(setup.readProjectFile).toHaveBeenCalledWith('project-1', '/README.md')
    expect(setup.writeProjectFile).toHaveBeenCalledWith('project-1', '/README.md', '# Cadence')
    expect(setup.createProjectEntry).toHaveBeenCalledWith({
      projectId: 'project-1',
      parentPath: '/',
      name: 'notes.md',
      entryType: 'file',
    })
    expect(setup.renameProjectEntry).toHaveBeenCalledWith({
      projectId: 'project-1',
      path: '/notes.md',
      newName: 'notes-2.md',
    })
    expect(setup.deleteProjectEntry).toHaveBeenCalledWith('project-1', '/notes-2.md')
  })

  it('transitions runtime auth state through login, callback, logout, and session bind actions', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
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
      runtimeRuns: {
        'project-1': null,
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('idle')

    await act(async () => {
      await Promise.resolve()
    })

    fireEvent.click(screen.getByRole('button', { name: 'Start OpenAI login' }))

    await waitFor(() =>
      expect(setup.startOpenAiLogin).toHaveBeenCalledWith('project-1', {
        selectedProfileId: 'openai_codex-default',
        originator: 'agent-pane',
      }),
    )
    await waitFor(() => expect(screen.getByTestId('auth-phase')).toHaveTextContent('awaiting_browser_callback'))

    fireEvent.click(screen.getByRole('button', { name: 'Submit OpenAI callback' }))

    await waitFor(() =>
      expect(setup.submitOpenAiCallback).toHaveBeenCalledWith('project-1', 'flow-1', {
        selectedProfileId: 'openai_codex-default',
        manualInput: 'browser-callback-token',
      }),
    )
    await waitFor(() => expect(screen.getByTestId('auth-phase')).toHaveTextContent('authenticated'))
    expect(screen.getByTestId('session-label')).toHaveTextContent('session-1')

    fireEvent.click(screen.getByRole('button', { name: 'Logout runtime session' }))

    await waitFor(() => expect(setup.logoutRuntimeSession).toHaveBeenCalledWith('project-1'))
    await waitFor(() => expect(screen.getByTestId('auth-phase')).toHaveTextContent('idle'))

    fireEvent.click(screen.getByRole('button', { name: 'Start runtime session' }))

    await waitFor(() => expect(setup.startRuntimeSession).toHaveBeenCalledWith('project-1'))
    await waitFor(() => expect(screen.getByTestId('auth-phase')).toHaveTextContent('authenticated'))
    expect(screen.getByTestId('session-label')).toHaveTextContent('session-1')
  })

  it('preserves the selected provider-profile snapshot when OpenAI callback completion refreshes a typed profile mismatch error', async () => {
    const initialRuntimeSession = makeRuntimeSession('project-1', {
      providerId: 'openai_codex',
      runtimeKind: 'openai_codex',
      flowId: 'flow-1',
      sessionId: null,
      accountId: null,
      phase: 'awaiting_browser_callback',
      callbackBound: true,
      lastErrorCode: null,
      lastError: null,
    })
    const refreshedRuntimeSession = makeRuntimeSession('project-1', {
      providerId: 'openai_codex',
      runtimeKind: 'openai_codex',
      flowId: 'flow-1',
      sessionId: null,
      accountId: null,
      phase: 'awaiting_browser_callback',
      callbackBound: true,
      lastErrorCode: 'auth_flow_profile_mismatch',
      lastError: {
        code: 'auth_flow_profile_mismatch',
        message:
          'Cadence rejected auth flow `flow-1` because it was started for provider profile `openai_codex-default` instead of the selected profile `zz-openai-alt`. Retry login for the currently selected profile.',
        retryable: false,
      },
    })
    const providerProfiles = makeProviderProfiles({
      activeProfileId: 'zz-openai-alt',
      profiles: [
        {
          profileId: 'openai_codex-default',
          providerId: 'openai_codex',
          label: 'OpenAI Codex',
          modelId: 'openai_codex',
          active: false,
          readiness: {
            ready: false,
            status: 'missing',
            credentialUpdatedAt: null,
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
        {
          profileId: 'zz-openai-alt',
          providerId: 'openai_codex',
          label: 'OpenAI Alt',
          modelId: 'openai_codex',
          active: true,
          readiness: {
            ready: false,
            status: 'missing',
            credentialUpdatedAt: null,
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
      ],
    })
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openai_codex',
        modelId: 'openai_codex',
        openrouterApiKeyConfigured: false,
      }),
      providerProfiles,
      runtimeSessions: {
        'project-1': initialRuntimeSession,
      },
      runtimeRuns: {
        'project-1': null,
      },
    })

    setup.getRuntimeSession
      .mockResolvedValueOnce(initialRuntimeSession)
      .mockResolvedValueOnce(refreshedRuntimeSession)
    setup.submitOpenAiCallback.mockRejectedValueOnce(
      new CadenceDesktopError({
        code: 'auth_flow_profile_mismatch',
        errorClass: 'user_fixable',
        message:
          'Cadence rejected auth flow `flow-1` because it was started for provider profile `openai_codex-default` instead of the selected profile `zz-openai-alt`. Retry login for the currently selected profile.',
        retryable: false,
      }),
    )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('zz-openai-alt'))
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('OpenAI Alt')
    expect(screen.getByTestId('selected-provider-source')).toHaveTextContent('provider_profiles')
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('awaiting_browser_callback')

    fireEvent.click(screen.getByRole('button', { name: 'Submit OpenAI callback' }))

    await waitFor(() =>
      expect(setup.submitOpenAiCallback).toHaveBeenCalledWith('project-1', 'flow-1', {
        selectedProfileId: 'zz-openai-alt',
        manualInput: 'browser-callback-token',
      }),
    )
    await waitFor(() =>
      expect(screen.getByTestId('session-reason')).toHaveTextContent('selected profile `zz-openai-alt`'),
    )
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('awaiting_browser_callback')
    expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('zz-openai-alt')
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('OpenAI Alt')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openai_codex')
    expect(setup.getRuntimeSession).toHaveBeenCalledTimes(2)
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
    expect(setup.listNotificationRoutes).toHaveBeenLastCalledWith('project-2')
    expect(setup.syncNotificationAdapters).toHaveBeenLastCalledWith('project-2')
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
    expect(screen.getByTestId('selected-model-id')).toHaveTextContent('openai_codex')
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

  it('projects active provider-profile identity without mutating repo-local runtime truth', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openai_codex',
        modelId: 'openai_codex',
        openrouterApiKeyConfigured: false,
      }),
      providerProfiles: makeProviderProfilesFromRuntimeSettings(
        makeRuntimeSettings({
          providerId: 'openai_codex',
          modelId: 'openai_codex',
          openrouterApiKeyConfigured: false,
        }),
      ),
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          providerId: 'openai_codex',
          runtimeKind: 'openai_codex',
          phase: 'authenticated',
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openai_codex-default'))
    expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('openai_codex-default')
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('OpenAI Codex')
    expect(screen.getByTestId('selected-provider-source')).toHaveTextContent('provider_profiles')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('openai_codex')

    fireEvent.click(screen.getByRole('button', { name: 'Save OpenRouter provider profile' }))

    await waitFor(() =>
      expect(setup.upsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'openrouter-default',
        providerId: 'openrouter',
        runtimeKind: 'openrouter',
        label: 'OpenRouter',
        modelId: 'openai/gpt-4.1-mini',
        presetId: 'openrouter',
        baseUrl: null,
        apiVersion: null,
        apiKey: 'sk-or-v1-test-secret',
        activate: true,
      }),
    )
    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openrouter-default'))
    expect(screen.getByTestId('provider-profiles-count')).toHaveTextContent('2')
    expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('openrouter-default')
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('OpenRouter')
    expect(screen.getByTestId('provider-profiles-selected-readiness-status')).toHaveTextContent('ready')
    expect(screen.getByTestId('selected-provider-source')).toHaveTextContent('provider_profiles')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openrouter')
    expect(screen.getByTestId('selected-model-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('runtime-settings-provider-id')).toHaveTextContent('openrouter')
    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('auth-phase')).toHaveTextContent('authenticated')

    fireEvent.click(screen.getByRole('button', { name: 'Activate OpenAI provider profile' }))

    await waitFor(() => expect(setup.setActiveProviderProfile).toHaveBeenCalledWith('openai_codex-default'))
    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openai_codex-default'))
    expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('openai_codex-default')
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('OpenAI Codex')
    expect(screen.getByTestId('selected-provider-source')).toHaveTextContent('provider_profiles')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openai_codex')
    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('openai_codex')
  })


  it('forwards generic GitHub provider-profile payloads and projects GitHub as selected provider truth', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openai_codex',
        modelId: 'openai_codex',
        openrouterApiKeyConfigured: false,
        anthropicApiKeyConfigured: false,
      }),
      providerProfiles: makeProviderProfilesFromRuntimeSettings(
        makeRuntimeSettings({
          providerId: 'openai_codex',
          modelId: 'openai_codex',
          openrouterApiKeyConfigured: false,
          anthropicApiKeyConfigured: false,
        }),
      ),
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          providerId: 'openai_codex',
          runtimeKind: 'openai_codex',
          phase: 'authenticated',
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openai_codex-default'))

    fireEvent.click(screen.getByRole('button', { name: 'Save GitHub provider profile' }))

    await waitFor(() =>
      expect(setup.upsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'github_models-default',
        providerId: 'github_models',
        runtimeKind: 'openai_compatible',
        label: 'GitHub Models',
        modelId: 'openai/gpt-4.1',
        presetId: 'github_models',
        baseUrl: null,
        apiVersion: null,
        apiKey: 'ghp_test_secret',
        activate: true,
      }),
    )
    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('github_models-default'))
    expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('github_models-default')
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('GitHub Models')
    expect(screen.getByTestId('provider-profiles-selected-readiness-status')).toHaveTextContent('ready')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('github_models')
    expect(screen.getByTestId('runtime-settings-provider-id')).toHaveTextContent('github_models')
    expect(screen.getByTestId('runtime-provider-id')).toHaveTextContent('openai_codex')
  })

  it('preserves the last truthful provider-profile snapshot when refresh or save fails', async () => {
    const initialRuntimeSettings = makeRuntimeSettings({
      providerId: 'openrouter',
      modelId: 'openai/gpt-4.1-mini',
      openrouterApiKeyConfigured: true,
    })
    const initialProviderProfiles = makeProviderProfilesFromRuntimeSettings(initialRuntimeSettings)
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: initialRuntimeSettings,
      providerProfiles: initialProviderProfiles,
    })

    setup.getProviderProfiles
      .mockResolvedValueOnce(initialProviderProfiles)
      .mockRejectedValueOnce(
        new CadenceDesktopError({
          code: 'provider_profiles_timeout',
          errorClass: 'retryable',
          message: 'Cadence timed out while loading app-local provider profiles.',
          retryable: true,
        }),
      )

    setup.upsertProviderProfile.mockRejectedValueOnce(
      new CadenceDesktopError({
        code: 'provider_profiles_write_failed',
        errorClass: 'retryable',
        message: 'Cadence could not save the selected provider profile.',
        retryable: true,
      }),
    )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openrouter-default'))
    expect(screen.getByTestId('provider-profiles-selected-readiness-status')).toHaveTextContent('ready')
    expect(screen.getByTestId('provider-profiles-count')).toHaveTextContent('1')

    fireEvent.click(screen.getByRole('button', { name: 'Load provider profiles' }))

    await waitFor(() => expect(screen.getByTestId('provider-profiles-load-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('provider-profiles-load-error-code')).toHaveTextContent('provider_profiles_timeout')
    expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openrouter-default')
    expect(screen.getByTestId('provider-profiles-selected-readiness-status')).toHaveTextContent('ready')

    fireEvent.click(screen.getByRole('button', { name: 'Save OpenRouter provider profile' }))

    await waitFor(() => expect(screen.getByTestId('provider-profiles-save-error-code')).toHaveTextContent('provider_profiles_write_failed'))
    expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openrouter-default')
    expect(screen.getByTestId('provider-profiles-count')).toHaveTextContent('1')
    expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('openrouter')
  })

  it('loads active provider-model truth and supports explicit inactive-profile refreshes', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openrouter',
        modelId: 'openai/gpt-4.1-mini',
        openrouterApiKeyConfigured: true,
      }),
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'openrouter-default',
        profiles: [
          {
            profileId: 'openrouter-default',
            providerId: 'openrouter',
            label: 'OpenRouter',
            modelId: 'openai/gpt-4.1-mini',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-16T14:05:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
          {
            profileId: 'openai_codex-default',
            providerId: 'openai_codex',
            label: 'OpenAI Codex',
            modelId: 'openai_codex',
            active: false,
            readiness: {
              ready: false,
              status: 'missing',
              credentialUpdatedAt: null,
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openrouter-default'))
    expect(screen.getByTestId('provider-model-catalog-active-source')).toHaveTextContent('live')
    expect(screen.getByTestId('provider-model-catalog-active-configured-model-id')).toHaveTextContent('openai/gpt-4.1-mini')
    expect(screen.getByTestId('provider-model-catalog-active-model-count')).toHaveTextContent('2')
    expect(setup.getProviderModelCatalog).toHaveBeenCalledWith('openrouter-default', { forceRefresh: false })

    fireEvent.click(screen.getByRole('button', { name: 'Refresh OpenAI provider-model catalog' }))

    await waitFor(() => expect(setup.getProviderModelCatalog).toHaveBeenCalledWith('openai_codex-default', { forceRefresh: true }))
    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('provider-model-catalog-openrouter-source')).toHaveTextContent('live')
  })

  it('preserves the last-known-good provider-model catalog snapshot when refresh fails', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'openrouter',
        modelId: 'openai/gpt-4.1-mini',
        openrouterApiKeyConfigured: true,
      }),
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'openrouter-default',
        profiles: [
          {
            profileId: 'openrouter-default',
            providerId: 'openrouter',
            label: 'OpenRouter',
            modelId: 'openai/gpt-4.1-mini',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-16T14:05:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
    })

    setup.getProviderModelCatalog
      .mockResolvedValueOnce(
        makeProviderModelCatalog('openrouter-default', {
          providerId: 'openrouter',
          configuredModelId: 'openai/gpt-4.1-mini',
        }),
      )
      .mockRejectedValueOnce(
        new CadenceDesktopError({
          code: 'openrouter_provider_unavailable',
          errorClass: 'retryable',
          message: 'Cadence timed out while refreshing provider models.',
          retryable: true,
        }),
      )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openrouter-default'))
    expect(screen.getByTestId('provider-model-catalog-active-model-ids')).toHaveTextContent(
      'openai/gpt-4.1-mini,anthropic/claude-3.7-sonnet',
    )

    fireEvent.click(screen.getByRole('button', { name: 'Refresh OpenRouter provider-model catalog' }))

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-load-status')).toHaveTextContent('error'))
    expect(screen.getByTestId('provider-model-catalog-active-load-error-code')).toHaveTextContent(
      'openrouter_provider_unavailable',
    )
    expect(screen.getByTestId('provider-model-catalog-active-source')).toHaveTextContent('live')
    expect(screen.getByTestId('provider-model-catalog-active-configured-model-id')).toHaveTextContent(
      'openai/gpt-4.1-mini',
    )
    expect(screen.getByTestId('provider-model-catalog-active-model-ids')).toHaveTextContent(
      'openai/gpt-4.1-mini,anthropic/claude-3.7-sonnet',
    )
  })

  it('force-refreshes only the affected provider-model catalog after provider-profile changes', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'openai_codex-default',
        profiles: [
          {
            profileId: 'openai_codex-default',
            providerId: 'openai_codex',
            label: 'OpenAI Codex',
            modelId: 'openai_codex',
            active: true,
            readiness: {
              ready: false,
              status: 'missing',
              credentialUpdatedAt: null,
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
          {
            profileId: 'openrouter-default',
            providerId: 'openrouter',
            label: 'OpenRouter',
            modelId: 'old/openrouter-model',
            active: false,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-16T14:05:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openai_codex-default'))

    fireEvent.click(screen.getByRole('button', { name: 'Refresh OpenRouter provider-model catalog' }))
    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('provider-model-catalog-openrouter-configured-model-id')).toHaveTextContent(
      'old/openrouter-model',
    )

    fireEvent.click(screen.getByRole('button', { name: 'Save OpenRouter provider profile' }))

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openrouter-default'))
    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openrouter-default'))
    await waitFor(() =>
      expect(screen.getByTestId('provider-model-catalog-active-configured-model-id')).toHaveTextContent(
        'openai/gpt-4.1-mini',
      ),
    )
    expect(screen.getByTestId('provider-model-catalog-count')).toHaveTextContent('2')
    expect(screen.getByTestId('provider-model-catalog-active-source')).toHaveTextContent('live')
    expect(setup.getProviderModelCatalog).toHaveBeenLastCalledWith('openrouter-default', { forceRefresh: true })
  })

  it('force-refreshes the active provider-model catalog when endpoint metadata changes', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'openai-custom',
        profiles: [
          {
            profileId: 'openai-custom',
            providerId: 'openai_api',
            label: 'OpenAI-compatible Custom',
            modelId: 'gpt-4.1-mini',
            presetId: null,
            baseUrl: 'https://example-one.invalid/v1',
            apiVersion: '2025-02-01',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-16T14:05:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
      providerModelCatalogs: {
        'openai-custom': makeProviderModelCatalog('openai-custom', {
          providerId: 'openai_api',
          configuredModelId: 'gpt-4.1-mini',
          models: [
            {
              modelId: 'gpt-4.1-mini',
              displayName: 'GPT-4.1 Mini',
              thinking: {
                supported: true,
                effortOptions: ['low', 'medium', 'high'],
                defaultEffort: 'medium',
              },
            },
          ],
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openai-custom'))
    expect(setup.getProviderModelCatalog).toHaveBeenCalledWith('openai-custom', { forceRefresh: false })

    act(() => {
      setup.setProviderProfiles(
        makeProviderProfiles({
          activeProfileId: 'openai-custom',
          profiles: [
            {
              profileId: 'openai-custom',
              providerId: 'openai_api',
              label: 'OpenAI-compatible Custom',
              modelId: 'gpt-4.1-mini',
              presetId: null,
              baseUrl: 'https://example-two.invalid/v1',
              apiVersion: '2026-03-01',
              active: true,
              readiness: {
                ready: true,
                status: 'ready',
                credentialUpdatedAt: '2026-04-16T14:05:00Z',
              },
              migratedFromLegacy: false,
              migratedAt: null,
            },
          ],
        }),
      )
    })

    fireEvent.click(screen.getByRole('button', { name: 'Load provider profiles' }))

    await waitFor(() =>
      expect(setup.getProviderModelCatalog).toHaveBeenLastCalledWith('openai-custom', { forceRefresh: true }),
    )
    expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openai-custom')
  })

  it('keeps the newly active provider-model catalog truthful when an older refresh resolves later', async () => {
    let resolveOpenRouterCatalog: ((value: ProviderModelCatalogDto) => void) | null = null
    let resolveOpenAiCatalog: ((value: ProviderModelCatalogDto) => void) | null = null

    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'openrouter-default',
        profiles: [
          {
            profileId: 'openrouter-default',
            providerId: 'openrouter',
            label: 'OpenRouter',
            modelId: 'openai/gpt-4.1-mini',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-16T14:05:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
          {
            profileId: 'openai_codex-default',
            providerId: 'openai_codex',
            label: 'OpenAI Codex',
            modelId: 'openai_codex',
            active: false,
            readiness: {
              ready: false,
              status: 'missing',
              credentialUpdatedAt: null,
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
    })

    setup.getProviderModelCatalog.mockImplementation(
      async (profileId: string, options?: { forceRefresh?: boolean }) =>
        new Promise<ProviderModelCatalogDto>((resolve) => {
          if (profileId === 'openrouter-default' && options?.forceRefresh === false) {
            resolveOpenRouterCatalog = resolve
            return
          }

          if (profileId === 'openai_codex-default') {
            resolveOpenAiCatalog = resolve
            return
          }

          resolve(
            makeProviderModelCatalog(profileId, {
              providerId: profileId.startsWith('openrouter') ? 'openrouter' : 'openai_codex',
              configuredModelId: profileId.startsWith('openrouter') ? 'openai/gpt-4.1-mini' : 'openai_codex',
            }),
          )
        }),
    )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openrouter-default'))
    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-load-status')).toHaveTextContent('loading'))

    fireEvent.click(screen.getByRole('button', { name: 'Activate OpenAI provider profile' }))

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('openai_codex-default'))

    act(() => {
      resolveOpenAiCatalog?.(
        makeProviderModelCatalog('openai_codex-default', {
          providerId: 'openai_codex',
          configuredModelId: 'openai_codex',
        }),
      )
    })

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openai_codex-default'))
    expect(screen.getByTestId('provider-model-catalog-active-provider-id')).toHaveTextContent('openai_codex')

    act(() => {
      resolveOpenRouterCatalog?.(
        makeProviderModelCatalog('openrouter-default', {
          providerId: 'openrouter',
          configuredModelId: 'openai/gpt-4.1-mini',
        }),
      )
    })

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('openai_codex-default')
    expect(screen.getByTestId('provider-model-catalog-active-provider-id')).toHaveTextContent('openai_codex')
  })

  it('keeps the newly active provider-model catalog truthful when a local-provider refresh resolves after switching to an ambient profile', async () => {
    let resolveOllamaCatalog: ((value: ProviderModelCatalogDto) => void) | null = null
    let resolveBedrockCatalog: ((value: ProviderModelCatalogDto) => void) | null = null

    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      providerProfiles: makeProviderProfiles({
        activeProfileId: 'ollama-work',
        profiles: [
          {
            profileId: 'ollama-work',
            providerId: 'ollama',
            runtimeKind: 'openai_compatible',
            label: 'Ollama Work',
            modelId: 'llama3.2',
            presetId: 'ollama',
            baseUrl: 'http://127.0.0.1:11434/v1',
            apiVersion: null,
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'local',
              proofUpdatedAt: '2026-04-16T14:05:00Z',
              credentialUpdatedAt: '2026-04-16T14:05:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
          {
            profileId: 'bedrock-work',
            providerId: 'bedrock',
            runtimeKind: 'anthropic',
            label: 'Amazon Bedrock Work',
            modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
            presetId: 'bedrock',
            baseUrl: null,
            apiVersion: null,
            region: 'us-east-1',
            active: false,
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'ambient',
              proofUpdatedAt: '2026-04-16T14:05:00Z',
              credentialUpdatedAt: '2026-04-16T14:05:00Z',
            },
            migratedFromLegacy: false,
            migratedAt: null,
          },
        ],
      }),
    })

    setup.getProviderModelCatalog.mockImplementation(
      async (profileId: string, options?: { forceRefresh?: boolean }) =>
        new Promise<ProviderModelCatalogDto>((resolve) => {
          if (profileId === 'ollama-work' && options?.forceRefresh === false) {
            resolveOllamaCatalog = resolve
            return
          }

          if (profileId === 'bedrock-work') {
            resolveBedrockCatalog = resolve
            return
          }

          resolve(
            makeProviderModelCatalog(profileId, {
              providerId: profileId === 'ollama-work' ? 'ollama' : 'bedrock',
              configuredModelId:
                profileId === 'ollama-work'
                  ? 'llama3.2'
                  : 'anthropic.claude-3-7-sonnet-20250219-v1:0',
            }),
          )
        }),
    )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('ollama-work'))
    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-load-status')).toHaveTextContent('loading'))

    act(() => {
      setup.setProviderProfiles(
        makeProviderProfiles({
          activeProfileId: 'bedrock-work',
          profiles: [
            {
              profileId: 'ollama-work',
              providerId: 'ollama',
              runtimeKind: 'openai_compatible',
              label: 'Ollama Work',
              modelId: 'llama3.2',
              presetId: 'ollama',
              baseUrl: 'http://127.0.0.1:11434/v1',
              apiVersion: null,
              active: false,
              readiness: {
                ready: true,
                status: 'ready',
                proof: 'local',
                proofUpdatedAt: '2026-04-16T14:05:00Z',
                credentialUpdatedAt: '2026-04-16T14:05:00Z',
              },
              migratedFromLegacy: false,
              migratedAt: null,
            },
            {
              profileId: 'bedrock-work',
              providerId: 'bedrock',
              runtimeKind: 'anthropic',
              label: 'Amazon Bedrock Work',
              modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
              presetId: 'bedrock',
              baseUrl: null,
              apiVersion: null,
              region: 'us-east-1',
              active: true,
              readiness: {
                ready: true,
                status: 'ready',
                proof: 'ambient',
                proofUpdatedAt: '2026-04-16T14:05:00Z',
                credentialUpdatedAt: '2026-04-16T14:05:00Z',
              },
              migratedFromLegacy: false,
              migratedAt: null,
            },
          ],
        }),
      )
    })

    fireEvent.click(screen.getByRole('button', { name: 'Load provider profiles' }))

    await waitFor(() => expect(screen.getByTestId('provider-profiles-active-profile-id')).toHaveTextContent('bedrock-work'))

    act(() => {
      resolveBedrockCatalog?.(
        makeProviderModelCatalog('bedrock-work', {
          providerId: 'bedrock',
          configuredModelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
        }),
      )
    })

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('bedrock-work'))
    expect(screen.getByTestId('provider-model-catalog-active-provider-id')).toHaveTextContent('bedrock')

    act(() => {
      resolveOllamaCatalog?.(
        makeProviderModelCatalog('ollama-work', {
          providerId: 'ollama',
          configuredModelId: 'llama3.2',
        }),
      )
    })

    await waitFor(() => expect(screen.getByTestId('provider-model-catalog-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('provider-model-catalog-active-profile-id')).toHaveTextContent('bedrock-work')
    expect(screen.getByTestId('provider-model-catalog-active-provider-id')).toHaveTextContent('bedrock')
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
    expect(screen.getByTestId('selected-provider-source')).toHaveTextContent('provider_profiles')
    expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('openrouter-default')
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('OpenRouter')
    expect(screen.getByTestId('provider-mismatch')).toHaveTextContent('true')
    expect(screen.getByTestId('provider-mismatch-reason')).toHaveTextContent(
      'Settings now select provider profile OpenRouter (openrouter-default)',
    )
    expect(screen.getByTestId('provider-mismatch-recovery-copy')).toHaveTextContent(
      'Rebind the selected profile so durable runtime truth matches Settings.',
    )
    expect(screen.getByTestId('session-reason')).toHaveTextContent(
      'Settings now select provider profile OpenRouter (openrouter-default)',
    )
    expect(screen.getByTestId('messages-reason')).toHaveTextContent(
      'Rebind the selected profile before trusting new stream activity.',
    )
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

  it('derives Anthropic-first guidance and mismatch recovery without rewriting persisted runtime truth', async () => {
    const anthropicProfiles = makeProviderProfiles({
      activeProfileId: 'anthropic-work',
      profiles: [
        {
          profileId: 'anthropic-work',
          providerId: 'anthropic',
          label: 'Anthropic Work',
          modelId: 'claude-3-7-sonnet-latest',
          active: true,
          readiness: {
            ready: true,
            status: 'ready',
            credentialUpdatedAt: '2026-04-16T14:05:00Z',
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
      ],
    })

    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        openrouterApiKeyConfigured: false,
        anthropicApiKeyConfigured: true,
      }),
      providerProfiles: anthropicProfiles,
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          providerId: 'openrouter',
          runtimeKind: 'openrouter',
          phase: 'authenticated',
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('anthropic'))
    expect(screen.getByTestId('selected-provider-label')).toHaveTextContent('Anthropic')
    expect(screen.getByTestId('selected-provider-source')).toHaveTextContent('provider_profiles')
    expect(screen.getByTestId('provider-profiles-selected-profile-id')).toHaveTextContent('anthropic-work')
    expect(screen.getByTestId('provider-profiles-selected-profile-label')).toHaveTextContent('Anthropic Work')
    expect(screen.getByTestId('provider-mismatch')).toHaveTextContent('true')
    expect(screen.getByTestId('provider-mismatch-reason')).toHaveTextContent(
      'Settings now select provider profile Anthropic Work (anthropic-work)',
    )
    expect(screen.getByTestId('provider-mismatch-recovery-copy')).toHaveTextContent(
      'Rebind the selected profile so durable runtime truth matches Settings.',
    )
    expect(screen.getByTestId('session-reason')).toHaveTextContent(
      'Settings now select provider profile Anthropic Work (anthropic-work)',
    )
    expect(screen.getByTestId('messages-reason')).toHaveTextContent(
      'Rebind the selected profile before trusting new stream activity.',
    )
    expect(screen.getByTestId('session-reason')).not.toHaveTextContent('OpenAI')
  })

  it('derives missing-key Anthropic guidance without leaking OpenRouter or OpenAI copy', async () => {
    const anthropicProfiles = makeProviderProfiles({
      activeProfileId: 'anthropic-work',
      profiles: [
        {
          profileId: 'anthropic-work',
          providerId: 'anthropic',
          label: 'Anthropic Work',
          modelId: 'claude-3-5-haiku-latest',
          active: true,
          readiness: {
            ready: false,
            status: 'missing',
            credentialUpdatedAt: null,
          },
          migratedFromLegacy: false,
          migratedAt: null,
        },
      ],
    })

    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Cadence')] },
      runtimeSettings: makeRuntimeSettings({
        providerId: 'anthropic',
        modelId: 'claude-3-5-haiku-latest',
        openrouterApiKeyConfigured: false,
        anthropicApiKeyConfigured: false,
      }),
      providerProfiles: anthropicProfiles,
      runtimeSessions: {
        'project-1': makeRuntimeSession('project-1', {
          providerId: 'anthropic',
          runtimeKind: 'anthropic',
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

    await waitFor(() => expect(screen.getByTestId('selected-provider-id')).toHaveTextContent('anthropic'))
    expect(screen.getByTestId('session-reason')).toHaveTextContent('Configure an Anthropic API key in Settings')
    expect(screen.getByTestId('messages-reason')).toHaveTextContent('Configure an Anthropic API key in Settings')
    expect(screen.getByTestId('session-reason')).not.toHaveTextContent('OpenRouter')
    expect(screen.getByTestId('messages-reason')).not.toHaveTextContent('OpenAI')
  })
})
