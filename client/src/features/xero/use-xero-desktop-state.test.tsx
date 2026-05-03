import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import {
  createXeroDoctorReport,
  type ImportMcpServersResponseDto,
  type ImportRepositoryResponseDto,
  type ListNotificationDispatchesResponseDto,
  type ListNotificationRoutesResponseDto,
  type ListProjectsResponseDto,
  type McpRegistryDto,
  type ProjectSnapshotResponseDto,
  type ProjectUpdatedPayloadDto,
  type ProviderCredentialDto,
  type ProviderAuthSessionDto,
  type ProviderModelCatalogDto,
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
  type SkillRegistryDto,
} from '@/src/lib/xero-model'
import { type ProviderProfilesDto } from '@/src/test/legacy-provider-profiles'
import { XeroDesktopError, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import { useXeroDesktopState } from '@/src/features/xero/use-xero-desktop-state'
import { REPOSITORY_STATUS_BATCH_WINDOW_MS } from './use-xero-desktop-state/runtime-stream'

type SetSkillEnabledRequest = Parameters<XeroDesktopAdapter['setSkillEnabled']>[0]
type RemoveSkillRequest = Parameters<XeroDesktopAdapter['removeSkill']>[0]
type UpsertSkillLocalRootRequest = Parameters<XeroDesktopAdapter['upsertSkillLocalRoot']>[0]
type RemoveSkillLocalRootRequest = Parameters<XeroDesktopAdapter['removeSkillLocalRoot']>[0]
type UpdateProjectSkillSourceRequest = Parameters<XeroDesktopAdapter['updateProjectSkillSource']>[0]
type UpdateGithubSkillSourceRequest = Parameters<XeroDesktopAdapter['updateGithubSkillSource']>[0]
type UpsertPluginRootRequest = Parameters<XeroDesktopAdapter['upsertPluginRoot']>[0]
type RemovePluginRootRequest = Parameters<XeroDesktopAdapter['removePluginRoot']>[0]
type SetPluginEnabledRequest = Parameters<XeroDesktopAdapter['setPluginEnabled']>[0]
type RemovePluginRequest = Parameters<XeroDesktopAdapter['removePlugin']>[0]

function createDeferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason?: unknown) => void
  const promise = new Promise<T>((promiseResolve, promiseReject) => {
    resolve = promiseResolve
    reject = promiseReject
  })

  return { promise, resolve, reject }
}

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

function makeAgentSession(projectId: string) {
  return {
    projectId,
    agentSessionId: 'agent-session-main',
    title: 'Main session',
    summary: 'Primary project session',
    status: 'active' as const,
    selected: true,
    createdAt: '2026-04-15T17:55:00Z',
    updatedAt: '2026-04-15T17:55:00Z',
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
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    agentSessions: [makeAgentSession(id)],
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
    updatedAt: '2026-04-13T19:33:32Z',
    ...overrides,
  }
}

function makeProviderCredential(overrides: Partial<ProviderCredentialDto> = {}): ProviderCredentialDto {
  return {
    providerId: 'openai_codex',
    kind: 'oauth_session',
    hasApiKey: false,
    oauthAccountId: 'acct-1',
    oauthSessionId: 'session-1',
    hasOauthAccessToken: true,
    oauthExpiresAt: null,
    baseUrl: null,
    apiVersion: null,
    region: null,
    projectId: null,
    defaultModelId: null,
    readinessProof: 'oauth_session',
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

function makeProviderProfilesFromRuntimeSettings(
  runtimeSettings: RuntimeSettingsDto,
  options: { profileId?: string; label?: string } = {},
): ProviderProfilesDto {
  const profileId =
    options.profileId ?? (runtimeSettings.providerId === 'openrouter' ? 'openrouter-default' : 'openai_codex-default')
  const providerLabel = options.label ?? (runtimeSettings.providerId === 'openrouter' ? 'OpenRouter' : 'OpenAI Codex')
  const runtimeKind = runtimeSettings.providerId === 'openrouter' ? 'openrouter' : 'openai_codex'
  const presetId = runtimeSettings.providerId === 'openrouter' ? 'openrouter' : null

  const activeProfile: ProviderProfilesDto['profiles'][number] = {
    profileId,
    providerId: runtimeSettings.providerId,
    runtimeKind,
    label: providerLabel,
    modelId: runtimeSettings.modelId,
    presetId,
    active: true,
    readiness:
      runtimeSettings.providerId === 'openrouter'
        ? {
            ready: runtimeSettings.openrouterApiKeyConfigured,
            status: runtimeSettings.openrouterApiKeyConfigured ? 'ready' : 'missing',
            proof: runtimeSettings.openrouterApiKeyConfigured ? 'stored_secret' : null,
            proofUpdatedAt: runtimeSettings.openrouterApiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
          }
        : {
            ready: false,
            status: 'missing',
            proofUpdatedAt: null,
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
            runtimeKind: 'openrouter',
            label: 'OpenRouter',
            modelId: 'openai/gpt-4.1-mini',
            presetId: 'openrouter',
            active: false,
            readiness: {
              ready: runtimeSettings.openrouterApiKeyConfigured,
              status: runtimeSettings.openrouterApiKeyConfigured ? 'ready' : 'missing',
              proof: runtimeSettings.openrouterApiKeyConfigured ? 'stored_secret' : null,
              proofUpdatedAt: runtimeSettings.openrouterApiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
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

function makeMcpRegistry(overrides: Partial<McpRegistryDto> = {}): McpRegistryDto {
  return {
    updatedAt: '2026-04-24T04:00:00Z',
    servers: [
      {
        id: 'memory',
        name: 'Memory Server',
        transport: {
          kind: 'stdio',
          command: 'npx',
          args: ['@modelcontextprotocol/server-memory'],
        },
        env: [
          {
            key: 'OPENAI_API_KEY',
            fromEnv: 'OPENAI_API_KEY',
          },
        ],
        cwd: null,
        connection: {
          status: 'stale',
          diagnostic: {
            code: 'mcp_status_unchecked',
            message: 'Xero has not checked this MCP server yet.',
            retryable: true,
          },
          lastCheckedAt: null,
          lastHealthyAt: null,
        },
        updatedAt: '2026-04-24T04:00:00Z',
      },
    ],
    ...overrides,
  }
}

function makeSkillRegistry(overrides: Partial<SkillRegistryDto> = {}): SkillRegistryDto {
  return {
    projectId: 'project-1',
    reloadedAt: '2026-04-24T05:00:00Z',
    entries: [
      {
        sourceId: 'project:project-1:reviewer',
        skillId: 'reviewer',
        name: 'Reviewer',
        description: 'Reviews code changes.',
        sourceKind: 'project',
        scope: 'project',
        projectId: 'project-1',
        sourceState: 'enabled',
        trustState: 'user_approved',
        enabled: true,
        installed: true,
        userInvocable: true,
        versionHash: 'abcdef123456',
        lastUsedAt: null,
        lastDiagnostic: null,
        source: {
          label: 'Project skill skills/reviewer',
          repo: null,
          reference: null,
          path: 'skills/reviewer',
          rootId: null,
          rootPath: null,
          relativePath: 'skills/reviewer',
          bundleId: null,
          pluginId: null,
          serverId: null,
        },
      },
    ],
    sources: {
      localRoots: [],
      pluginRoots: [],
      github: {
        repo: 'vercel-labs/skills',
        reference: 'main',
        root: 'skills',
        enabled: true,
        updatedAt: '2026-04-24T05:00:00Z',
      },
      projects: [
        {
          projectId: 'project-1',
          enabled: true,
          updatedAt: '2026-04-24T05:00:00Z',
        },
      ],
      updatedAt: '2026-04-24T05:00:00Z',
    },
    diagnostics: [],
    plugins: [],
    pluginCommands: [],
    ...overrides,
  }
}

function makePluginSkillRegistry(overrides: Partial<SkillRegistryDto> = {}): SkillRegistryDto {
  const base = makeSkillRegistry()
  const pluginCommand: SkillRegistryDto['pluginCommands'][number] = {
    commandId: 'plugin:com.acme.tools:command:open-panel',
    pluginId: 'com.acme.tools',
    contributionId: 'open-panel',
    label: 'Open Panel',
    description: 'Opens the Acme plugin panel.',
    entry: 'commands/open-panel.js',
    availability: 'project_open',
    riskLevel: 'observe',
    approvalPolicy: 'required',
    statePolicy: 'ephemeral',
    redactionRequired: true,
    state: 'enabled',
    trust: 'trusted',
  }
  return makeSkillRegistry({
    sources: {
      ...base.sources,
      pluginRoots: [
        {
          rootId: 'team-plugins',
          path: '/tmp/xero-plugins',
          enabled: true,
          updatedAt: '2026-04-24T05:00:00Z',
        },
      ],
    },
    plugins: [
      {
        pluginId: 'com.acme.tools',
        name: 'Acme Tools',
        version: '1.2.3',
        description: 'Project automation helpers.',
        rootId: 'team-plugins',
        rootPath: '/tmp/xero-plugins',
        pluginRootPath: '/tmp/xero-plugins/acme-tools',
        manifestPath: '/tmp/xero-plugins/acme-tools/xero-plugin.json',
        manifestHash: 'abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd',
        state: 'enabled',
        trust: 'trusted',
        enabled: true,
        skillCount: 1,
        commandCount: 1,
        skills: [
          {
            contributionId: 'review-kit',
            skillId: 'review-kit',
            path: 'skills/review-kit',
            sourceId: 'plugin:project-1:com.acme.tools:review-kit',
          },
        ],
        commands: [pluginCommand],
        lastReloadedAt: '2026-04-24T05:00:00Z',
        lastDiagnostic: null,
      },
    ],
    pluginCommands: [pluginCommand],
    ...overrides,
  })
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
    },
  }
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
    agentSessionId: overrides.agentSessionId ?? 'agent-session-main',
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
  mcpRegistry?: McpRegistryDto
  skillRegistry?: SkillRegistryDto
  providerCredentials?: ProviderCredentialDto[]
  providerProfiles?: ProviderProfilesDto
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogErrors?: Record<string, Error>
  autonomousStates?: Record<string, AutonomousRunStateDto | null>
  notificationDispatches?: Record<string, ListNotificationDispatchesResponseDto['dispatches']>
  notificationRoutes?: Record<string, ListNotificationRoutesResponseDto['routes']>
  notificationDispatchErrors?: Record<string, Error>
  diffs?: Partial<Record<'staged' | 'unstaged' | 'worktree', RepositoryDiffResponseDto>>
  importResponse?: ImportRepositoryResponseDto
  subscribeErrors?: Record<string, XeroDesktopError>
  subscribeResponses?: Record<string, SubscribeRuntimeStreamResponseDto>
}) {
  let projectUpdatedHandler: ((payload: ProjectUpdatedPayloadDto) => void) | null = null
  let repositoryStatusChangedHandler: ((payload: RepositoryStatusChangedPayloadDto) => void) | null = null
  let runtimeUpdatedHandler: ((payload: RuntimeUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null
  let projectUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null
  let repositoryStatusErrorHandler: ((error: XeroDesktopError) => void) | null = null
  let runtimeUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null
  let runtimeRunUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null

  const snapshots = options?.snapshots ?? {
    'project-1': makeSnapshot('project-1', 'Xero'),
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
  const currentMcpRegistry = {
    value: options?.mcpRegistry ?? makeMcpRegistry(),
  }
  const currentSkillRegistry = {
    value: options?.skillRegistry ?? makeSkillRegistry(),
  }
  const currentProviderCredentials = {
    value: options?.providerCredentials ?? [],
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

  let listedProjects = (options?.listProjects?.projects ?? [makeProjectSummary('project-1', 'Xero')]).map((project) => ({
    ...project,
  }))

  const streamSubscriptions: Array<{
    projectId: string
    agentSessionId: string
    handler: (payload: RuntimeStreamEventDto) => void
    onError: ((error: XeroDesktopError) => void) | null
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
  const getProjectUsageSummary = vi.fn(async (projectId: string) => ({
    projectId,
    totals: {
      runCount: 0,
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      cacheReadTokens: 0,
      cacheCreationTokens: 0,
      estimatedCostMicros: 0,
    },
    byModel: [],
  }))
  const listAgentSessions = vi.fn(async (request: { projectId: string }) => ({
    sessions: snapshots[request.projectId]?.agentSessions ?? [makeAgentSession(request.projectId)],
  }))
  const getAgentSession = vi.fn(async (request: { projectId: string; agentSessionId: string }) => {
    return (
      (snapshots[request.projectId]?.agentSessions ?? [makeAgentSession(request.projectId)]).find(
        (session) => session.agentSessionId === request.agentSessionId,
      ) ?? null
    )
  })
  const createAgentSession = vi.fn(async (request: { projectId: string; title?: string | null; summary?: string; selected?: boolean }) => {
    const existingSessions = snapshots[request.projectId]?.agentSessions ?? [makeAgentSession(request.projectId)]
    const selected = request.selected ?? true
    const session = {
      ...makeAgentSession(request.projectId),
      agentSessionId: `agent-session-${existingSessions.length + 1}`,
      title: request.title?.trim() || 'New session',
      summary: request.summary ?? '',
      selected,
      createdAt: '2026-04-15T18:05:00Z',
      updatedAt: '2026-04-15T18:05:00Z',
    }
    if (snapshots[request.projectId]) {
      snapshots[request.projectId] = {
        ...snapshots[request.projectId],
        agentSessions: [
          ...existingSessions.map((existing) =>
            selected ? { ...existing, selected: false } : existing,
          ),
          session,
        ],
      }
    }
    return session
  })
  const listAgentDefinitions = vi.fn(async () => ({ definitions: [] }))
  const archiveAgentDefinition = vi.fn(async () => {
    throw new Error('archiveAgentDefinition not stubbed in test adapter')
  })
  const getAgentDefinitionVersion = vi.fn(async () => null)
  const updateAgentSession = vi.fn(async (request: {
    projectId: string
    agentSessionId: string
    title?: string | null
    summary?: string | null
    selected?: boolean | null
  }) => ({
    ...makeAgentSession(request.projectId),
    agentSessionId: request.agentSessionId,
    title: request.title?.trim() || 'Main session',
    summary: request.summary ?? 'Primary project session',
    selected: request.selected ?? true,
    updatedAt: '2026-04-15T18:00:00Z',
  }))
  const autoNameAgentSession = vi.fn(async (request: {
    projectId: string
    agentSessionId: string
    prompt: string
  }) => ({
    ...makeAgentSession(request.projectId),
    agentSessionId: request.agentSessionId,
    title: 'Generated Session Title',
    updatedAt: '2026-04-15T18:00:01Z',
  }))
  const archiveAgentSession = vi.fn(async (request: { projectId: string; agentSessionId: string }) => {
    const archivedSession = {
      ...makeAgentSession(request.projectId),
      agentSessionId: request.agentSessionId,
      status: 'archived' as const,
      selected: false,
      archivedAt: '2026-04-15T18:00:00Z',
      updatedAt: '2026-04-15T18:00:00Z',
    }
    if (snapshots[request.projectId]) {
      snapshots[request.projectId] = {
        ...snapshots[request.projectId],
        agentSessions: snapshots[request.projectId].agentSessions.map((session) =>
          session.agentSessionId === request.agentSessionId ? archivedSession : session,
        ),
      }
    }
    return archivedSession
  })
  const restoreAgentSession = vi.fn(async (request: { projectId: string; agentSessionId: string }) => {
    const restoredSession = {
      ...makeAgentSession(request.projectId),
      agentSessionId: request.agentSessionId,
      status: 'active' as const,
      archivedAt: null,
      updatedAt: '2026-04-15T18:00:00Z',
    }
    if (snapshots[request.projectId]) {
      snapshots[request.projectId] = {
        ...snapshots[request.projectId],
        agentSessions: snapshots[request.projectId].agentSessions.map((session) =>
          session.agentSessionId === request.agentSessionId ? restoredSession : session,
        ),
      }
    }
    return restoredSession
  })
  const deleteAgentSession = vi.fn(async (_request: { projectId: string; agentSessionId: string }) => {
    if (snapshots[_request.projectId]) {
      snapshots[_request.projectId] = {
        ...snapshots[_request.projectId],
        agentSessions: snapshots[_request.projectId].agentSessions.filter(
          (session) => session.agentSessionId !== _request.agentSessionId,
        ),
      }
    }
    return undefined
  })
  const getRepositoryStatus = vi.fn(async (projectId: string) => statuses[projectId])
  const getRepositoryDiff = vi.fn(async (_projectId: string, scope: 'staged' | 'unstaged' | 'worktree') => {
    const configuredDiff = options?.diffs?.[scope]
    return configuredDiff ?? makeDiff('project-1', scope, scope === 'unstaged' ? 'diff --git a/file b/file\n+change' : '')
  })
  const getRuntimeRun = vi.fn(async (projectId: string, _agentSessionId?: string): Promise<RuntimeRunDto | null> =>
    runtimeRuns[projectId] ?? null,
  )
  const getAutonomousRun = vi.fn(async (projectId: string): Promise<AutonomousRunStateDto> =>
    autonomousStates[projectId] ?? { run: null },
  )
  const getRuntimeSession = vi.fn(async (projectId: string) => runtimeSessions[projectId])
  const listMcpServers = vi.fn(async () => currentMcpRegistry.value)
  const upsertMcpServer = vi.fn(async (request: {
    id: string
    name: string
    transport: McpRegistryDto['servers'][number]['transport']
    env?: McpRegistryDto['servers'][number]['env']
    cwd?: string | null
  }) => {
    const now = '2026-04-24T05:00:00Z'
    const existing = currentMcpRegistry.value.servers.filter((server) => server.id !== request.id)
    const previous = currentMcpRegistry.value.servers.find((server) => server.id === request.id)

    currentMcpRegistry.value = {
      updatedAt: now,
      servers: [
        {
          id: request.id,
          name: request.name,
          transport: request.transport,
          env: request.env ?? [],
          cwd: request.cwd ?? null,
          connection: previous?.connection ?? {
            status: 'stale',
            diagnostic: {
              code: 'mcp_status_unchecked',
              message: 'Xero has not checked this MCP server yet.',
              retryable: true,
            },
            lastCheckedAt: null,
            lastHealthyAt: null,
          },
          updatedAt: now,
        },
        ...existing,
      ],
    }

    return currentMcpRegistry.value
  })
  const removeMcpServer = vi.fn(async (serverId: string) => {
    currentMcpRegistry.value = {
      ...currentMcpRegistry.value,
      updatedAt: '2026-04-24T05:01:00Z',
      servers: currentMcpRegistry.value.servers.filter((server) => server.id !== serverId),
    }

    return currentMcpRegistry.value
  })
  const importMcpServers = vi.fn(async (_path: string): Promise<ImportMcpServersResponseDto> => ({
    registry: currentMcpRegistry.value,
    diagnostics: [],
  }))
  const refreshMcpServerStatuses = vi.fn(async (options?: { serverIds?: string[] }) => {
    const serverIds = options?.serverIds ?? []
    const shouldRefresh = (id: string) => serverIds.length === 0 || serverIds.includes(id)
    currentMcpRegistry.value = {
      ...currentMcpRegistry.value,
      updatedAt: '2026-04-24T05:02:00Z',
      servers: currentMcpRegistry.value.servers.map((server) =>
        shouldRefresh(server.id)
          ? {
              ...server,
              connection: {
                status: 'connected',
                diagnostic: null,
                lastCheckedAt: '2026-04-24T05:02:00Z',
                lastHealthyAt: '2026-04-24T05:02:00Z',
              },
            }
          : server,
      ),
    }

    return currentMcpRegistry.value
  })
  const listSkillRegistry = vi.fn(async () => currentSkillRegistry.value)
  const reloadSkillRegistry = vi.fn(async () => currentSkillRegistry.value)
  const setSkillEnabled = vi.fn(async (request: SetSkillEnabledRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      entries: currentSkillRegistry.value.entries.map((entry) =>
        entry.sourceId === request.sourceId
          ? {
              ...entry,
              enabled: request.enabled,
              sourceState: request.enabled ? 'enabled' : 'disabled',
            }
          : entry,
      ),
      reloadedAt: '2026-04-24T05:03:00Z',
    }
    return currentSkillRegistry.value
  })
  const removeSkill = vi.fn(async (request: RemoveSkillRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      entries: currentSkillRegistry.value.entries.filter((entry) => entry.sourceId !== request.sourceId),
      reloadedAt: '2026-04-24T05:04:00Z',
    }
    return currentSkillRegistry.value
  })
  const upsertSkillLocalRoot = vi.fn(async (request: UpsertSkillLocalRootRequest) => {
    const rootId = request.rootId ?? 'local-test'
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      sources: {
        ...currentSkillRegistry.value.sources,
        localRoots: [
          ...currentSkillRegistry.value.sources.localRoots.filter((root) => root.rootId !== rootId),
          {
            rootId,
            path: request.path,
            enabled: request.enabled,
            updatedAt: '2026-04-24T05:05:00Z',
          },
        ],
      },
      reloadedAt: '2026-04-24T05:05:00Z',
    }
    return currentSkillRegistry.value
  })
  const removeSkillLocalRoot = vi.fn(async (request: RemoveSkillLocalRootRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      sources: {
        ...currentSkillRegistry.value.sources,
        localRoots: currentSkillRegistry.value.sources.localRoots.filter((root) => root.rootId !== request.rootId),
      },
      reloadedAt: '2026-04-24T05:06:00Z',
    }
    return currentSkillRegistry.value
  })
  const updateProjectSkillSource = vi.fn(async (request: UpdateProjectSkillSourceRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      sources: {
        ...currentSkillRegistry.value.sources,
        projects: [
          ...currentSkillRegistry.value.sources.projects.filter((project) => project.projectId !== request.projectId),
          {
            projectId: request.projectId,
            enabled: request.enabled,
            updatedAt: '2026-04-24T05:07:00Z',
          },
        ],
      },
      reloadedAt: '2026-04-24T05:07:00Z',
    }
    return currentSkillRegistry.value
  })
  const updateGithubSkillSource = vi.fn(async (request: UpdateGithubSkillSourceRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      sources: {
        ...currentSkillRegistry.value.sources,
        github: {
          repo: request.repo,
          reference: request.reference,
          root: request.root,
          enabled: request.enabled,
          updatedAt: '2026-04-24T05:08:00Z',
        },
      },
      reloadedAt: '2026-04-24T05:08:00Z',
    }
    return currentSkillRegistry.value
  })
  const upsertPluginRoot = vi.fn(async (request: UpsertPluginRootRequest) => {
    const rootId = request.rootId ?? 'plugin-test'
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      sources: {
        ...currentSkillRegistry.value.sources,
        pluginRoots: [
          ...currentSkillRegistry.value.sources.pluginRoots.filter((root) => root.rootId !== rootId),
          {
            rootId,
            path: request.path,
            enabled: request.enabled,
            updatedAt: '2026-04-24T05:09:00Z',
          },
        ],
      },
      reloadedAt: '2026-04-24T05:09:00Z',
    }
    return currentSkillRegistry.value
  })
  const removePluginRoot = vi.fn(async (request: RemovePluginRootRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      sources: {
        ...currentSkillRegistry.value.sources,
        pluginRoots: currentSkillRegistry.value.sources.pluginRoots.filter((root) => root.rootId !== request.rootId),
      },
      reloadedAt: '2026-04-24T05:10:00Z',
    }
    return currentSkillRegistry.value
  })
  const setPluginEnabled = vi.fn(async (request: SetPluginEnabledRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      plugins: currentSkillRegistry.value.plugins.map((plugin) =>
        plugin.pluginId === request.pluginId
          ? {
              ...plugin,
              enabled: request.enabled,
              state: request.enabled ? 'enabled' : 'disabled',
            }
          : plugin,
      ),
      pluginCommands: currentSkillRegistry.value.pluginCommands.map((command) =>
        command.pluginId === request.pluginId
          ? {
              ...command,
              state: request.enabled ? 'enabled' : 'disabled',
            }
          : command,
      ),
      reloadedAt: '2026-04-24T05:11:00Z',
    }
    return currentSkillRegistry.value
  })
  const removePlugin = vi.fn(async (request: RemovePluginRequest) => {
    currentSkillRegistry.value = {
      ...currentSkillRegistry.value,
      plugins: currentSkillRegistry.value.plugins.map((plugin) =>
        plugin.pluginId === request.pluginId
          ? {
              ...plugin,
              enabled: false,
              state: 'stale',
            }
          : plugin,
      ),
      pluginCommands: currentSkillRegistry.value.pluginCommands.filter((command) => command.pluginId !== request.pluginId),
      reloadedAt: '2026-04-24T05:12:00Z',
    }
    return currentSkillRegistry.value
  })
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
        throw new XeroDesktopError({
          code: 'provider_profile_not_found',
          errorClass: 'user_fixable',
          message: `Xero could not find provider profile \`${profileId}\`.`,
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
  const checkProviderProfile = vi.fn(async (profileId: string) => {
    const currentProfile = currentProviderProfiles.value.profiles.find((profile) => profile.profileId === profileId)
    if (!currentProfile) {
      throw new XeroDesktopError({
        code: 'provider_profile_not_found',
        errorClass: 'user_fixable',
        message: `Xero could not find provider profile \`${profileId}\`.`,
      })
    }

    const modelCatalog =
      currentProviderModelCatalogs.value[profileId] ??
      makeProviderModelCatalog(profileId, {
        providerId: currentProfile.providerId,
        configuredModelId: currentProfile.modelId,
      })
    currentProviderModelCatalogs.value = {
      ...currentProviderModelCatalogs.value,
      [profileId]: modelCatalog,
    }

    return {
      checkedAt: '2026-04-26T12:00:00Z',
      profileId,
      providerId: currentProfile.providerId,
      validationChecks: [],
      reachabilityChecks: [],
      modelCatalog,
    }
  })
  const getProviderProfiles = vi.fn(async () => currentProviderProfiles.value)
  const upsertRuntimeSettings = vi.fn(async (request: {
    providerId: RuntimeSettingsDto['providerId']
    modelId: string
    openrouterApiKey?: string | null
    anthropicApiKey?: string | null
  }) => {
    const nextKeyConfigured =
      request.openrouterApiKey === undefined || request.openrouterApiKey === null
        ? currentRuntimeSettings.value.openrouterApiKeyConfigured
        : request.openrouterApiKey.trim().length > 0
    const nextAnthropicKeyConfigured =
      request.anthropicApiKey === undefined || request.anthropicApiKey === null
        ? currentRuntimeSettings.value.anthropicApiKeyConfigured
        : request.anthropicApiKey.trim().length > 0

    currentRuntimeSettings.value = {
      providerId: request.providerId,
      modelId: request.modelId,
      openrouterApiKeyConfigured: nextKeyConfigured,
      anthropicApiKeyConfigured: nextAnthropicKeyConfigured,
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
      presetId: (request.presetId ?? null) as ProviderProfilesDto['profiles'][number]['presetId'],
      baseUrl: request.baseUrl ?? null,
      apiVersion: request.apiVersion ?? null,
      active: nextActiveProfileId === request.profileId,
      readiness:
        request.providerId === 'openai_codex'
          ? {
              ready: false,
              status: 'missing',
              proofUpdatedAt: null,
            }
          : {
              ready: apiKeyConfigured,
              status: apiKeyConfigured ? 'ready' : 'missing',
              proof: apiKeyConfigured ? 'stored_secret' : null,
              proofUpdatedAt: apiKeyConfigured ? '2026-04-16T14:05:00Z' : null,
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
  const logoutProviderProfile = vi.fn(async (profileId: string) => {
    currentProviderProfiles.value = {
      ...currentProviderProfiles.value,
      profiles: currentProviderProfiles.value.profiles.map((profile) =>
        profile.profileId === profileId && profile.providerId === 'openai_codex'
          ? {
              ...profile,
              readiness: {
                ready: false,
                status: 'missing',
                proofUpdatedAt: null,
              },
            }
          : profile,
      ),
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
      _options?: { originator?: string | null },
    ) =>
      makeProviderAuthSession({
        sessionId: null,
        phase: 'awaiting_browser_callback',
        lastErrorCode: null,
        lastError: null,
      }),
  )
  const submitOpenAiCallback = vi.fn(
    async (
      flowId: string,
      _options?: { manualInput?: string | null },
    ) => makeProviderAuthSession({ flowId, phase: 'authenticated' }),
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
              providerProfileId: request.controls.providerProfileId ?? activeControls.providerProfileId ?? null,
              runtimeAgentId: request.controls.runtimeAgentId,
              modelId: request.controls.modelId,
              thinkingEffort: request.controls.thinkingEffort ?? null,
              approvalMode: request.controls.approvalMode,
              planModeRequired: request.controls.planModeRequired,
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
  const startRuntimeSession = vi.fn(async (projectId: string, _options?: { providerProfileId?: string | null }) =>
    makeRuntimeSession(projectId),
  )
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
      _projectId: string,
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
      agentSessionId: string,
      _itemKinds,
      handler: (payload: RuntimeStreamEventDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      const subscribeError = options?.subscribeErrors?.[projectId]
      if (subscribeError) {
        throw subscribeError
      }

      const subscription = {
        projectId,
        agentSessionId,
        handler,
        onError: onError ?? null,
        unsubscribe: vi.fn(),
      }
      streamSubscriptions.push(subscription)

      return {
        response:
          options?.subscribeResponses?.[projectId] ??
          makeStreamResponse(projectId, {
            agentSessionId,
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
      onError?: (error: XeroDesktopError) => void,
    ) => {
      projectUpdatedHandler = handler
      projectUpdatedErrorHandler = onError ?? null
      return projectUnlisten
    },
  )
  const onRepositoryStatusChanged = vi.fn(
    async (
      handler: (payload: RepositoryStatusChangedPayloadDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      repositoryStatusChangedHandler = handler
      repositoryStatusErrorHandler = onError ?? null
      return repositoryUnlisten
    },
  )
  const onRuntimeUpdated = vi.fn(
    async (
      handler: (payload: RuntimeUpdatedPayloadDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      runtimeUpdatedHandler = handler
      runtimeUpdatedErrorHandler = onError ?? null
      return runtimeUnlisten
    },
  )
  const onRuntimeRunUpdated = vi.fn(
    async (
      handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      runtimeRunUpdatedHandler = handler
      runtimeRunUpdatedErrorHandler = onError ?? null
      return runtimeRunUnlisten
    },
  )

  const listProjectFiles = vi.fn(async (projectId: string, path = '/') => ({
    projectId,
    path,
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
  const moveProjectEntry = vi.fn(async (request) => ({
    projectId: request.projectId,
    path:
      request.targetParentPath === '/'
        ? `/${request.path.split('/').filter(Boolean).pop() ?? ''}`
        : `${request.targetParentPath}/${request.path.split('/').filter(Boolean).pop() ?? ''}`,
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

  const adapter: XeroDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder,
    pickParentFolder: vi.fn(async () => null),
    importRepository,
    createRepository: vi.fn(async () => ({
      project: {
        id: 'project-new',
        name: 'project-new',
        description: '',
        milestone: '',
        totalPhases: 0,
        completedPhases: 0,
        activePhase: 0,
        branch: null,
        runtime: null,
      },
      repository: {
        id: 'repo-new',
        projectId: 'project-new',
        rootPath: '',
        displayName: '',
        branch: null,
        headSha: null,
        isGitRepo: true,
      },
    })),
    listProjects,
    removeProject,
    getProjectSnapshot,
    getProjectUsageSummary,
    getRepositoryStatus,
    getRepositoryDiff,
    gitStagePaths: async () => undefined,
    gitUnstagePaths: async () => undefined,
    gitDiscardChanges: async () => undefined,
    gitCommit: async () => ({ sha: 'abc1234', summary: 'mock commit', signature: { name: 'Mock', email: 'mock@example.com' } }),
    gitGenerateCommitMessage: async () => ({
      message: 'feat: mock generated commit',
      providerId: 'openai_api',
      modelId: 'gpt-5.4',
      diffTruncated: false,
    }),
    gitFetch: async () => ({ remote: 'origin', refspecs: [] }),
    gitPull: async () => ({ remote: 'origin', branch: 'main', updated: false, summary: 'already up to date', newHeadSha: null }),
    gitPush: async () => ({ remote: 'origin', branch: 'main', updates: [] }),
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    moveProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
    createAgentSession,
    listAgentDefinitions,
    archiveAgentDefinition,
    getAgentDefinitionVersion,
    listAgentSessions,
    getAgentSession,
    updateAgentSession,
    autoNameAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    getAutonomousRun,
    getRuntimeRun,
    getRuntimeSession,
    listMcpServers,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    listSkillRegistry,
    reloadSkillRegistry,
    setSkillEnabled,
    removeSkill,
    upsertSkillLocalRoot,
    removeSkillLocalRoot,
    updateProjectSkillSource,
    updateGithubSkillSource,
    upsertPluginRoot,
    removePluginRoot,
    setPluginEnabled,
    removePlugin,
    getProviderModelCatalog,
    checkProviderProfile,
    runDoctorReport: vi.fn(async (request) =>
      createXeroDoctorReport({
        reportId: 'doctor-test',
        generatedAt: '2026-04-26T12:00:00Z',
        mode: request?.mode ?? 'quick_local',
        versions: {
          appVersion: 'test',
          runtimeSupervisorVersion: 'test',
          runtimeProtocolVersion: 'supervisor-v1',
        },
      }),
    ),
    startOpenAiLogin,
    submitOpenAiCallback,
    startAutonomousRun,
    startRuntimeRun,
    stageAgentAttachment: vi.fn(async () => ({
      kind: 'image' as const,
      absolutePath: '/tmp/stage.png',
      mediaType: 'image/png',
      originalName: 'stage.png',
      sizeBytes: 0,
    })),
    discardAgentAttachment: vi.fn(async () => undefined),
    updateRuntimeRunControls,
    startRuntimeSession,
    cancelAutonomousRun,
    stopRuntimeRun,
    logoutRuntimeSession,
    listProviderCredentials: vi.fn(async () => ({ credentials: currentProviderCredentials.value })),
    upsertProviderCredential: vi.fn(async () => ({ credentials: [] })),
    deleteProviderCredential: vi.fn(async () => ({ credentials: [] })),
    startOAuthLogin: vi.fn(async () => makeProviderAuthSession()),
    completeOAuthCallback: vi.fn(async () => makeProviderAuthSession()),
    resolveOperatorAction,
    resumeOperatorRun,
    listNotificationRoutes,
    listNotificationDispatches,
    upsertNotificationRoute,
    upsertNotificationRouteCredentials: vi.fn(async () => {
      throw new Error('not used in use-xero-desktop-state tests')
    }) as never,
    recordNotificationDispatchOutcome: vi.fn(async () => {
      throw new Error('not used in use-xero-desktop-state tests')
    }) as never,
    submitNotificationReply: vi.fn(async () => {
      throw new Error('not used in use-xero-desktop-state tests')
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
      label: 'xero-browser',
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
    onAgentUsageUpdated: vi.fn(async () => () => undefined),
  }

  return {
    adapter,
    pickRepositoryFolder,
    importRepository,
    listProjects,
    removeProject,
    getProjectSnapshot,
    getProjectUsageSummary,
    getRepositoryStatus,
    getRepositoryDiff,
    getRuntimeRun,
    getRuntimeSession,
    listMcpServers,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    listSkillRegistry,
    reloadSkillRegistry,
    setSkillEnabled,
    removeSkill,
    upsertSkillLocalRoot,
    removeSkillLocalRoot,
    updateProjectSkillSource,
    updateGithubSkillSource,
    upsertPluginRoot,
    removePluginRoot,
    setPluginEnabled,
    removePlugin,
    getProviderModelCatalog,
    checkProviderProfile,
    getProviderProfiles,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    moveProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
    createAgentSession,
    updateAgentSession,
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
    logoutProviderProfile,
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
    emitProjectUpdatedError(error: XeroDesktopError) {
      projectUpdatedErrorHandler?.(error)
    },
    emitRepositoryStatusChanged(payload: RepositoryStatusChangedPayloadDto) {
      repositoryStatusChangedHandler?.(payload)
    },
    emitRepositoryStatusError(error: XeroDesktopError) {
      repositoryStatusErrorHandler?.(error)
    },
    emitRuntimeUpdated(payload: RuntimeUpdatedPayloadDto) {
      runtimeUpdatedHandler?.(payload)
    },
    emitRuntimeUpdatedError(error: XeroDesktopError) {
      runtimeUpdatedErrorHandler?.(error)
    },
    emitRuntimeRunUpdated(payload: RuntimeRunUpdatedPayloadDto) {
      runtimeRunUpdatedHandler?.(payload)
    },
    emitRuntimeRunUpdatedError(error: XeroDesktopError) {
      runtimeRunUpdatedErrorHandler?.(error)
    },
    emitRuntimeStream(index: number, payload: RuntimeStreamEventDto) {
      streamSubscriptions[index]?.handler(payload)
    },
    emitRuntimeStreamError(index: number, error: XeroDesktopError) {
      streamSubscriptions[index]?.onError?.(error)
    },
    setProviderProfiles(nextProviderProfiles: ProviderProfilesDto) {
      currentProviderProfiles.value = nextProviderProfiles
    },
  }
}

function Harness({ adapter }: { adapter: XeroDesktopAdapter }) {
  const state = useXeroDesktopState({ adapter })

  return (
    <div>
      <div data-testid="loading">{String(state.isLoading)}</div>
      <div data-testid="project-loading">{String(state.isProjectLoading)}</div>
      <div data-testid="active-project">{state.activeProject?.name ?? 'none'}</div>
      <div data-testid="active-project-id">{state.activeProjectId ?? 'none'}</div>
      <div data-testid="pending-project-selection-id">{state.pendingProjectSelectionId ?? 'none'}</div>
      <div data-testid="selected-agent-session-id">{state.activeProject?.selectedAgentSessionId ?? 'none'}</div>
      <div data-testid="workspace-pane-count">{String(state.agentWorkspaceLayout?.paneSlots.length ?? 0)}</div>
      <div data-testid="workspace-focused-pane-id">{state.agentWorkspaceLayout?.focusedPaneId ?? 'none'}</div>
      <div data-testid="workspace-pane-session-ids">
        {state.agentWorkspaceLayout?.paneSlots.map((slot) => slot.agentSessionId ?? 'empty').join(',') ?? 'none'}
      </div>
      <div data-testid="workspace-pane-view-count">{String(state.agentWorkspacePanes.length)}</div>
      <div data-testid="workspace-splitter-ratios">
        {state.agentWorkspaceLayout?.splitterRatios['1x2']?.join(',') ?? 'none'}
      </div>
      <div data-testid="branch">{state.activeProject?.branch ?? 'none'}</div>
      <div data-testid="runtime-label">{state.agentView?.runtimeLabel ?? 'none'}</div>
      <div data-testid="runtime-provider-id">{state.agentView?.runtimeSession?.providerId ?? 'none'}</div>
      <div data-testid="selected-provider-id">{state.agentView?.selectedProviderId ?? 'none'}</div>
      <div data-testid="selected-provider-label">{state.agentView?.selectedProviderLabel ?? 'none'}</div>
      <div data-testid="selected-provider-source">{state.agentView?.selectedProviderSource ?? 'none'}</div>
      <div data-testid="selected-model-id">{state.agentView?.selectedModelId ?? 'none'}</div>
      <div data-testid="selected-model-selection-key">{state.agentView?.selectedModelSelectionKey ?? 'none'}</div>
      <div data-testid="selected-model-option-profile-id">{state.agentView?.selectedModelOption?.profileId ?? 'none'}</div>
      <div data-testid="selected-model-thinking-options">{state.agentView?.selectedModelThinkingEffortOptions.join(',') ?? 'none'}</div>
      <div data-testid="composer-model-option-count">{String(state.agentView?.composerModelOptions?.length ?? 0)}</div>
      <div data-testid="composer-model-option-profile-id">{state.agentView?.composerModelOptions?.[0]?.profileId ?? 'none'}</div>
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
      <div data-testid="runtime-run-agent-session-id">{state.agentView?.runtimeRun?.agentSessionId ?? 'none'}</div>
      <div data-testid="runtime-run-id">{state.agentView?.runtimeRun?.runId ?? 'none'}</div>
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
      <div data-testid="runtime-settings-provider-id">none</div>
      <div data-testid="runtime-settings-model-id">none</div>
      <div data-testid="runtime-settings-key-configured">false</div>
      <div data-testid="provider-profiles-active-profile-id">none</div>
      <div data-testid="provider-profiles-count">0</div>
      <div data-testid="provider-profiles-selected-profile-id">{state.agentView?.selectedProfileId ?? 'none'}</div>
      <div data-testid="provider-profiles-selected-profile-label">{state.agentView?.selectedProfileLabel ?? 'none'}</div>
      <div data-testid="provider-profiles-selected-readiness-status">{'none'}</div>
      <div data-testid="provider-profiles-load-status">idle</div>
      <div data-testid="provider-profiles-load-error-code">none</div>
      <div data-testid="provider-profiles-load-error-message">none</div>
      <div data-testid="provider-profiles-save-status">idle</div>
      <div data-testid="provider-profiles-save-error-code">none</div>
      <div data-testid="provider-profiles-save-error-message">none</div>
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
      <div data-testid="runtime-settings-load-status">idle</div>
      <div data-testid="runtime-settings-load-error-code">none</div>
      <div data-testid="runtime-settings-load-error-message">none</div>
      <div data-testid="runtime-settings-save-status">idle</div>
      <div data-testid="runtime-settings-save-error-code">none</div>
      <div data-testid="runtime-settings-save-error-message">none</div>
      <div data-testid="mcp-registry-count">{String(state.mcpRegistry?.servers.length ?? 0)}</div>
      <div data-testid="mcp-registry-first-status">{state.mcpRegistry?.servers[0]?.connection.status ?? 'none'}</div>
      <div data-testid="mcp-registry-first-id">{state.mcpRegistry?.servers[0]?.id ?? 'none'}</div>
      <div data-testid="mcp-registry-load-status">{state.mcpRegistryLoadStatus}</div>
      <div data-testid="mcp-registry-load-error-code">{state.mcpRegistryLoadError?.code ?? 'none'}</div>
      <div data-testid="mcp-registry-load-error-message">{state.mcpRegistryLoadError?.message ?? 'none'}</div>
      <div data-testid="mcp-registry-mutation-status">{state.mcpRegistryMutationStatus}</div>
      <div data-testid="mcp-registry-mutation-error-code">{state.mcpRegistryMutationError?.code ?? 'none'}</div>
      <div data-testid="mcp-registry-mutation-error-message">{state.mcpRegistryMutationError?.message ?? 'none'}</div>
      <div data-testid="mcp-pending-server-id">{state.pendingMcpServerId ?? 'none'}</div>
      <div data-testid="mcp-import-diagnostics-count">{String(state.mcpImportDiagnostics.length)}</div>
      <div data-testid="skill-registry-plugin-count">{String(state.skillRegistry?.plugins.length ?? 0)}</div>
      <div data-testid="skill-registry-plugin-command-count">{String(state.skillRegistry?.pluginCommands.length ?? 0)}</div>
      <div data-testid="skill-registry-plugin-root-count">{String(state.skillRegistry?.sources.pluginRoots.length ?? 0)}</div>
      <div data-testid="skill-registry-mutation-status">{state.skillRegistryMutationStatus}</div>
      <div data-testid="skill-pending-source-id">{state.pendingSkillSourceId ?? 'none'}</div>
      <div data-testid="refresh-source">{state.refreshSource ?? 'none'}</div>
      <div data-testid="project-count">{String(state.projects.length)}</div>
      <div data-testid="workflow-has-phases">{String(state.workflowView?.hasPhases ?? false)}</div>
      <div data-testid="workflow-overall-percent">{String(state.workflowView?.overallPercent ?? 0)}</div>
      <div data-testid="workflow-active-phase">{state.workflowView?.activePhase?.name ?? 'none'}</div>
      <div data-testid="execution-status-count">{String(state.executionView?.statusCount ?? 0)}</div>
      <div data-testid="execution-branch">{state.executionView?.branchLabel ?? 'none'}</div>
      <div data-testid="active-diff-scope">{state.activeDiffScope}</div>
      <div data-testid="diff-status">{state.activeRepositoryDiff.status}</div>
      <div data-testid="diff-error">{state.activeRepositoryDiff.errorMessage ?? 'none'}</div>
      <div data-testid="diff-patch">{state.activeRepositoryDiff.diff?.patch ?? 'none'}</div>
      <button onClick={() => void state.selectProject('project-2')} type="button">
        Select project 2
      </button>
      <button onClick={() => void state.selectProject('project-1')} type="button">
        Select project 1
      </button>
      <button onClick={() => void state.selectAgentSession('agent-session-alt')} type="button">
        Select alt session
      </button>
      <button onClick={() => void state.spawnPane().catch(() => undefined)} type="button">
        Spawn pane
      </button>
      <button
        onClick={() => {
          const paneId = state.agentWorkspaceLayout?.paneSlots[0]?.id
          if (paneId) {
            state.focusPane(paneId)
          }
        }}
        type="button"
      >
        Focus first pane
      </button>
      <button
        onClick={() => {
          const paneId = state.agentWorkspaceLayout?.paneSlots[1]?.id
          if (paneId) {
            state.closePane(paneId)
          }
        }}
        type="button"
      >
        Close second pane
      </button>
      <button
        onClick={() => {
          const agentSessionId = state.agentWorkspaceLayout?.paneSlots[1]?.agentSessionId
          if (agentSessionId) {
            void state.archiveAgentSession(agentSessionId)
          }
        }}
        type="button"
      >
        Archive second pane session
      </button>
      <button onClick={() => state.setSplitterRatios('1x2', [2, 1, 1])} type="button">
        Save splitter ratios
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
          void state.writeProjectFile('project-1', '/README.md', '# Xero').catch(() => undefined)
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
          void (state as unknown as { refreshRuntimeSettings: (o?: { force?: boolean }) => Promise<unknown> }).refreshRuntimeSettings({ force: true }).catch(() => undefined)
        }}
        type="button"
      >
        Load runtime settings
      </button>
      <button
        onClick={() => {
          void (state as unknown as { upsertRuntimeSettings: (r: unknown) => Promise<unknown> })
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
          void (state as unknown as { upsertRuntimeSettings: (r: unknown) => Promise<unknown> })
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
          void state.refreshMcpRegistry({ force: true }).catch(() => undefined)
        }}
        type="button"
      >
        Load MCP registry
      </button>
      <button
        onClick={() => {
          void state
            .upsertMcpServer({
              id: 'filesystem',
              name: 'Filesystem Server',
              transport: {
                kind: 'stdio',
                command: 'node',
                args: ['/opt/mcp/server-filesystem.js'],
              },
              env: [
                {
                  key: 'OPENAI_API_KEY',
                  fromEnv: 'OPENAI_API_KEY',
                },
              ],
              cwd: null,
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Save MCP server
      </button>
      <button
        onClick={() => {
          void state.removeMcpServer('memory').catch(() => undefined)
        }}
        type="button"
      >
        Remove MCP server
      </button>
      <button
        onClick={() => {
          void state.importMcpServers('/tmp/mcp-import.json').catch(() => undefined)
        }}
        type="button"
      >
        Import MCP servers
      </button>
      <button
        onClick={() => {
          void state.refreshMcpServerStatuses({ serverIds: ['memory'] }).catch(() => undefined)
        }}
        type="button"
      >
        Refresh MCP statuses
      </button>
      <button
        onClick={() => {
          void state.refreshSkillRegistry({ force: true }).catch(() => undefined)
        }}
        type="button"
      >
        Load skill registry
      </button>
      <button
        onClick={() => {
          void state
            .upsertPluginRoot({
              rootId: 'team-plugins',
              path: '/tmp/xero-plugins',
              enabled: true,
              projectId: 'project-1',
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Save plugin root
      </button>
      <button
        onClick={() => {
          void state
            .setPluginEnabled({
              projectId: 'project-1',
              pluginId: 'com.acme.tools',
              enabled: false,
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Disable Acme plugin
      </button>
      <button
        onClick={() => {
          void state
            .removePlugin({
              projectId: 'project-1',
              pluginId: 'com.acme.tools',
            })
            .catch(() => undefined)
        }}
        type="button"
      >
        Remove Acme plugin
      </button>
      <button
        onClick={() => {
          void (state as unknown as { refreshProviderProfiles: (o?: { force?: boolean }) => Promise<unknown> }).refreshProviderProfiles({ force: true }).catch(() => undefined)
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
          void (state as unknown as { upsertProviderProfile: (r: unknown) => Promise<unknown> })
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
          void (state as unknown as { setActiveProviderProfile: (id: string) => Promise<unknown> }).setActiveProviderProfile('openai_codex-default').catch(() => undefined)
        }}
        type="button"
      >
        Activate OpenAI provider profile
      </button>
      <button
        onClick={() => {
          void (state as unknown as { upsertProviderProfile: (r: unknown) => Promise<unknown> })
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

describe('useXeroDesktopState', () => {
  it('loads the project list, repository truth, and runtime session for the active project', async () => {
    const setup = createMockAdapter()

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project')).toHaveTextContent('Xero'))

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
    expect(setup.syncNotificationAdapters).not.toHaveBeenCalled()
  })

  it('loads model catalogs for credentialed providers and feeds the agent composer', async () => {
    const setup = createMockAdapter({
      providerCredentials: [makeProviderCredential()],
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() =>
      expect(setup.getProviderModelCatalog).toHaveBeenCalledWith('openai_codex-default', {
        forceRefresh: false,
      }),
    )
    await waitFor(() => expect(screen.getByTestId('composer-model-option-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('composer-model-option-profile-id')).toHaveTextContent('openai_codex-default')
    expect(screen.getByTestId('selected-model-option-profile-id')).toHaveTextContent('openai_codex-default')
    expect(screen.getByTestId('selected-model-selection-key')).toHaveTextContent('openai_codex:openai_codex')
    expect(screen.getByTestId('selected-model-thinking-options')).toHaveTextContent('low,medium,high')
  })

  it('projects plugin registry mutations through the skill registry state', async () => {
    const setup = createMockAdapter({
      skillRegistry: makePluginSkillRegistry(),
    })

    render(<Harness adapter={setup.adapter} />)

    fireEvent.click(screen.getByRole('button', { name: 'Load skill registry' }))

    await waitFor(() => expect(screen.getByTestId('skill-registry-plugin-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('skill-registry-plugin-root-count')).toHaveTextContent('1')
    expect(screen.getByTestId('skill-registry-plugin-command-count')).toHaveTextContent('1')

    fireEvent.click(screen.getByRole('button', { name: 'Disable Acme plugin' }))

    await waitFor(() =>
      expect(setup.setPluginEnabled).toHaveBeenCalledWith({
        projectId: 'project-1',
        pluginId: 'com.acme.tools',
        enabled: false,
      }),
    )
    await waitFor(() => expect(screen.getByTestId('skill-registry-mutation-status')).toHaveTextContent('idle'))

    fireEvent.click(screen.getByRole('button', { name: 'Remove Acme plugin' }))

    await waitFor(() =>
      expect(setup.removePlugin).toHaveBeenCalledWith({
        projectId: 'project-1',
        pluginId: 'com.acme.tools',
      }),
    )
    await waitFor(() => expect(screen.getByTestId('skill-registry-plugin-command-count')).toHaveTextContent('0'))

    fireEvent.click(screen.getByRole('button', { name: 'Save plugin root' }))

    await waitFor(() =>
      expect(setup.upsertPluginRoot).toHaveBeenCalledWith({
        rootId: 'team-plugins',
        path: '/tmp/xero-plugins',
        enabled: true,
        projectId: 'project-1',
      }),
    )
  })

  it('reloads the full active snapshot after project:updated so durable operator-loop state stays fresh', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    let refreshed = false
    setup.getProjectSnapshot.mockImplementation(async () =>
      refreshed
        ? {
            ...makeSnapshot('project-1', 'Xero'),
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
        : makeSnapshot('project-1', 'Xero'),
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
          ...makeProjectSummary('project-1', 'Xero'),
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
    expect(syncNotificationAdaptersMock.mock.calls.length).toBe(syncRefreshesBeforeEvent)
  })

  it('ignores wrong-project update callbacks so one project cannot overwrite another project\'s operator history', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
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

  it('keeps the empty workflow view scaffold while execution git truth stays live', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [
          {
            id: 'project-1',
            name: 'Xero',
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
            name: 'Xero',
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
            rootPath: '/tmp/Xero',
            displayName: 'Xero',
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
          approvalRequests: [],
          verificationRecords: [],
          resumeHistory: [],
          agentSessions: [makeAgentSession('project-1')],
        },
      },
      statuses: {
        'project-1': {
          repository: {
            id: 'repo-project-1',
            projectId: 'project-1',
            rootPath: '/tmp/Xero',
            displayName: 'Xero',
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
    expect(screen.getByTestId('workflow-overall-percent')).toHaveTextContent('33')
    expect(screen.getByTestId('workflow-active-phase')).toHaveTextContent('Live projection')
    expect(screen.getByTestId('execution-status-count')).toHaveTextContent('1')
    expect(screen.getByTestId('execution-branch')).toHaveTextContent('feature/workflow-truth')
    expect(screen.getByTestId('branch')).toHaveTextContent('feature/workflow-truth')
    expect(screen.getByTestId('runtime-label')).toHaveTextContent('Openai Codex · Awaiting browser')
  })


  it('keeps the workflow scaffold empty when snapshots have no phases', async () => {
    const setup = createMockAdapter({
      listProjects: {
        projects: [
          {
            id: 'project-1',
            name: 'Xero',
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
          ...makeSnapshot('project-1', 'Xero'),
          project: {
            id: 'project-1',
            name: 'Xero',
            description: 'Desktop shell',
            milestone: 'M001',
            totalPhases: 0,
            completedPhases: 0,
            activePhase: 0,
            branch: null,
            runtime: null,
          },
          phases: [],
        },
      },
      statuses: {
        'project-1': {
          repository: {
            id: 'repo-project-1',
            projectId: 'project-1',
            rootPath: '/tmp/Xero',
            displayName: 'Xero',
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
  })

  it('supports cancelled imports and successful imports without duplicating project rows', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
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
        projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')],
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
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
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
    expect(setup.writeProjectFile).toHaveBeenCalledWith('project-1', '/README.md', '# Xero')
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




  it('keeps the clicked project visible when snapshot loading fails', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
    })

    setup.getProjectSnapshot.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        throw new Error('snapshot failed')
      }

      return makeSnapshot(projectId, 'Xero')
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('snapshot failed'))
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2')
    expect(screen.getByTestId('active-project')).toHaveTextContent('orchestra')
  })

  it('switches project selection after the snapshot without waiting for secondary hydration', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    const statusDeferred = createDeferred<RepositoryStatusResponseDto>()
    const runtimeDeferred = createDeferred<RuntimeSessionDto>()
    const routeDeferred = createDeferred<ListNotificationRoutesResponseDto>()
    const dispatchDeferred = createDeferred<ListNotificationDispatchesResponseDto>()

    setup.getRepositoryStatus.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        return statusDeferred.promise
      }

      return makeStatus(projectId, 'main')
    })
    setup.getRuntimeSession.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        return runtimeDeferred.promise
      }

      return makeRuntimeSession(projectId)
    })
    setup.listNotificationRoutes.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        return routeDeferred.promise
      }

      return { routes: [] }
    })
    setup.listNotificationDispatches.mockImplementation(async (projectId: string) => {
      if (projectId === 'project-2') {
        return dispatchDeferred.promise
      }

      return { dispatches: [] }
    })

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('pending-project-selection-id')).toHaveTextContent('project-2'))
    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    expect(screen.getByTestId('active-project')).toHaveTextContent('orchestra')
    expect(screen.getByTestId('project-loading')).toHaveTextContent('false')

    await act(async () => {
      statusDeferred.resolve(makeStatus('project-2', 'feature/import'))
      runtimeDeferred.resolve(makeRuntimeSession('project-2'))
      routeDeferred.resolve({ routes: [] })
      dispatchDeferred.resolve({ dispatches: [] })
      await Promise.all([
        statusDeferred.promise,
        runtimeDeferred.promise,
        routeDeferred.promise,
        dispatchDeferred.promise,
      ])
    })

    await waitFor(() => expect(screen.getByTestId('pending-project-selection-id')).toHaveTextContent('none'))
  })

  it('switches agent sessions optimistically without reloading the full project', async () => {
    const mainSession = makeAgentSession('project-1')
    const altSession = {
      ...makeAgentSession('project-1'),
      agentSessionId: 'agent-session-alt',
      title: 'Alt session',
      selected: false,
      updatedAt: '2026-04-15T18:01:00Z',
    }
    const updateSelection = createDeferred<typeof altSession>()
    const setup = createMockAdapter({
      snapshots: {
        'project-1': {
          ...makeSnapshot('project-1', 'Xero'),
          agentSessions: [mainSession, altSession],
        },
      },
    })
    setup.getRuntimeRun.mockImplementation(async (projectId: string, agentSessionId = 'agent-session-main') =>
      makeRuntimeRun(projectId, {
        agentSessionId,
        runId: agentSessionId === 'agent-session-alt' ? 'run-alt' : 'run-main',
      }),
    )
    setup.updateAgentSession.mockImplementationOnce(async () => ({
      ...(await updateSelection.promise),
      selected: true,
    }))

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('selected-agent-session-id')).toHaveTextContent('agent-session-main'))
    await waitFor(() => expect(screen.getByTestId('runtime-run-agent-session-id')).toHaveTextContent('agent-session-main'))

    const snapshotCallsBeforeSelection = setup.getProjectSnapshot.mock.calls.length
    fireEvent.click(screen.getByRole('button', { name: 'Select alt session' }))

    expect(screen.getByTestId('selected-agent-session-id')).toHaveTextContent('agent-session-alt')
    expect(screen.getByTestId('runtime-run-agent-session-id')).toHaveTextContent('none')
    expect(setup.getProjectSnapshot).toHaveBeenCalledTimes(snapshotCallsBeforeSelection)

    updateSelection.resolve(altSession)

    await waitFor(() => expect(setup.getRuntimeRun).toHaveBeenCalledWith('project-1', 'agent-session-alt'))
    await waitFor(() => expect(screen.getByTestId('runtime-run-id')).toHaveTextContent('run-alt'))
    await waitFor(() => expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-alt'))
    expect(setup.getProjectSnapshot).toHaveBeenCalledTimes(snapshotCallsBeforeSelection)
  })

  it('persists agent workspace panes per project and hydrates them on reload', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    const firstRender = render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main')

    fireEvent.click(screen.getByRole('button', { name: 'Spawn pane' }))

    await waitFor(() => expect(setup.createAgentSession).toHaveBeenCalledWith({
      projectId: 'project-1',
      title: null,
      summary: '',
      selected: false,
    }))
    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,agent-session-2')

    fireEvent.click(screen.getByRole('button', { name: 'Focus first pane' }))
    fireEvent.click(screen.getByRole('button', { name: 'Save splitter ratios' }))

    await waitFor(() => {
      const raw = window.localStorage.getItem('agentWorkspaceLayout')
      expect(raw).toContain('agent-session-2')
      expect(raw).toContain('"1x2":[2,1,1]')
    })

    firstRender.unmount()
    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,agent-session-2')
    expect(screen.getByTestId('workspace-splitter-ratios')).toHaveTextContent('2,1,1')
  })

  it('renders a spawned pane before the fresh session create call resolves', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })
    const createSession = createDeferred<void>()
    setup.createAgentSession.mockImplementationOnce(async (request) => {
      await createSession.promise
      return {
        ...makeAgentSession(request.projectId),
        agentSessionId: 'agent-session-delayed',
        title: 'New session',
        summary: request.summary ?? '',
        selected: request.selected ?? false,
        createdAt: '2026-04-15T18:05:00Z',
        updatedAt: '2026-04-15T18:05:00Z',
      }
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('1'))

    fireEvent.click(screen.getByRole('button', { name: 'Spawn pane' }))

    expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('2')
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,empty')
    expect(screen.getByTestId('workspace-focused-pane-id')).not.toHaveTextContent('agent-pane-project-1')

    createSession.resolve()

    await waitFor(() =>
      expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent(
        'agent-session-main,agent-session-delayed',
      ),
    )
  })

  it('turns an archived loaded pane into an empty slot and reuses it on the next spawn', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('1'))

    fireEvent.click(screen.getByRole('button', { name: 'Spawn pane' }))
    await waitFor(() => expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,agent-session-2'))

    fireEvent.click(screen.getByRole('button', { name: 'Archive second pane session' }))

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,empty')

    fireEvent.click(screen.getByRole('button', { name: 'Spawn pane' }))

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,agent-session-3')
    expect(setup.createAgentSession).toHaveBeenCalledTimes(2)
  })

  it('restores the target project workspace layout when switching projects', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    fireEvent.click(screen.getByRole('button', { name: 'Spawn pane' }))
    await waitFor(() => expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,agent-session-2'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))
    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main')

    fireEvent.click(screen.getByRole('button', { name: 'Select project 1' }))
    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() =>
      expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-main,agent-session-2'),
    )
  })

  it('keeps stale persisted workspace pane slots as empty reusable panes', async () => {
    window.localStorage.setItem(
      'agentWorkspaceLayout',
      JSON.stringify({
        'project-1': {
          paneSlots: [
            { id: 'stale-pane', agentSessionId: 'missing-session' },
            { id: 'main-pane', agentSessionId: 'agent-session-main' },
          ],
          focusedPaneId: 'stale-pane',
          splitterRatios: {},
          preSpawnSidebarMode: null,
        },
      }),
    )
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('empty,agent-session-main')
    expect(screen.getByTestId('workspace-focused-pane-id')).toHaveTextContent('stale-pane')

    fireEvent.click(screen.getByRole('button', { name: 'Spawn pane' }))

    await waitFor(() => expect(screen.getByTestId('workspace-pane-count')).toHaveTextContent('2'))
    expect(screen.getByTestId('workspace-pane-session-ids')).toHaveTextContent('agent-session-2,agent-session-main')
    expect(screen.getByTestId('workspace-focused-pane-id')).toHaveTextContent('stale-pane')
  })

  it('accepts snapshots without lifecycle projection after workflow model removal', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
    })

    setup.getProjectSnapshot.mockImplementation(async (projectId: string): Promise<ProjectSnapshotResponseDto> => {
      if (projectId === 'project-2') {
        const legacySnapshot = makeSnapshot(projectId, 'orchestra') as unknown as Record<string, unknown>
        delete legacySnapshot.lifecycle
        return legacySnapshot as unknown as ProjectSnapshotResponseDto
      }

      return makeSnapshot(projectId, 'Xero')
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))

    fireEvent.click(screen.getByRole('button', { name: 'Select project 2' }))

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2'))
    expect(screen.getByTestId('error')).toHaveTextContent('none')
    expect(screen.getByTestId('active-project')).toHaveTextContent('orchestra')
    await waitFor(() => expect(screen.getByTestId('workflow-active-phase')).toHaveTextContent('Import'))
  })

  it('keeps the selected project visible when repository status loading fails', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
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
    expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-2')
    expect(screen.getByTestId('active-project')).toHaveTextContent('orchestra')
    expect(screen.getByTestId('branch')).toHaveTextContent('No branch')
  })

  it('preserves the newly selected project when runtime loading fails after project selection', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
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
    await waitFor(() => expect(screen.getByTestId('error')).toHaveTextContent('runtime failed'))
    expect(screen.getByTestId('runtime-label')).toHaveTextContent('Runtime unavailable')
    expect(setup.listNotificationRoutes).toHaveBeenLastCalledWith('project-2')
    expect(setup.syncNotificationAdapters).not.toHaveBeenCalled()
  })

  it('resolves operator actions by invoking the adapter and reloading the active project snapshot', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    let resolved = false
    setup.getProjectSnapshot.mockImplementation(async () =>
      resolved
        ? {
            ...makeSnapshot('project-1', 'Xero'),
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
            ...makeSnapshot('project-1', 'Xero'),
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
    setup.resolveOperatorAction.mockImplementation(async (_projectId: string, actionId: string, decision: 'approve' | 'reject') => {
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
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
    })

    setup.resolveOperatorAction.mockRejectedValueOnce(
      new XeroDesktopError({
        code: 'operator_action_not_found',
        errorClass: 'user_fixable',
        message: 'Xero could not find operator request `flow-1:review_worktree` for the selected project.',
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
    expect(screen.getByTestId('active-project')).toHaveTextContent('Xero')
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

  it('coalesces repeated repository status events without resetting loaded diffs', async () => {
    const setup = createMockAdapter()

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('status-count')).toHaveTextContent('1'))
    fireEvent.click(screen.getByRole('button', { name: 'Load unstaged diff' }))
    await waitFor(() => expect(screen.getByTestId('diff-status')).toHaveTextContent('ready'))
    expect(screen.getByTestId('diff-patch')).toHaveTextContent('+change')

    act(() => {
      setup.emitRepositoryStatusChanged({
        projectId: 'project-1',
        repositoryId: 'repo-project-1',
        status: makeStatus('project-1', 'main'),
      })
    })

    await new Promise((resolve) => setTimeout(resolve, REPOSITORY_STATUS_BATCH_WINDOW_MS + 5))

    expect(screen.getByTestId('refresh-source')).toHaveTextContent('startup')
    expect(screen.getByTestId('diff-status')).toHaveTextContent('ready')
    expect(screen.getByTestId('diff-patch')).toHaveTextContent('+change')

    act(() => {
      setup.emitRepositoryStatusChanged({
        projectId: 'project-1',
        repositoryId: 'repo-project-1',
        status: makeStatus('project-1', 'feature/coalesced-status'),
      })
    })

    await waitFor(() => expect(screen.getByTestId('branch')).toHaveTextContent('feature/coalesced-status'))
    expect(screen.getByTestId('refresh-source')).toHaveTextContent('repository:status_changed')
    expect(screen.getByTestId('diff-status')).toHaveTextContent('idle')
    expect(screen.getByTestId('diff-patch')).toHaveTextContent('none')
  })


  it('clears stale stream cache on project switch and ignores callbacks from the old subscription', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero'), makeProjectSummary('project-2', 'orchestra')] },
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
          text: 'Connected to Xero.',
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
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
      subscribeErrors: {
        'project-1': new XeroDesktopError({
          code: 'runtime_stream_not_ready',
          errorClass: 'retryable',
          message: 'Xero cannot start a runtime stream until the selected project finishes auth.',
          retryable: true,
        }),
      },
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project')).toHaveTextContent('Xero'))
    await waitFor(() => expect(screen.getByTestId('stream-status')).toHaveTextContent('stale'))
    expect(screen.getByTestId('stream-error')).toHaveTextContent(
      'Xero cannot start a runtime stream until the selected project finishes auth.',
    )

    setup.subscribeRuntimeStream.mockImplementationOnce(
      async (projectId: string, agentSessionId: string, _itemKinds, handler, onError) => {
        setup.streamSubscriptions.push({
          projectId,
          agentSessionId,
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
        new XeroDesktopError({
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
    expect(screen.getByTestId('active-project')).toHaveTextContent('Xero')
  })





  it('loads MCP registry truth and applies upsert/remove/import/refresh mutations from desktop state', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
      mcpRegistry: makeMcpRegistry(),
    })

    setup.importMcpServers.mockResolvedValueOnce({
      registry: makeMcpRegistry(),
      diagnostics: [
        {
          index: 1,
          serverId: 'duplicate-memory',
          code: 'mcp_registry_import_invalid',
          message: 'Server id `memory` is duplicated in the import file.',
        },
      ],
    })

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('active-project-id')).toHaveTextContent('project-1'))
    await waitFor(() => expect(screen.getByTestId('mcp-registry-count')).toHaveTextContent('1'))
    expect(screen.getByTestId('mcp-registry-first-id')).toHaveTextContent('memory')
    expect(setup.listMcpServers).toHaveBeenCalledTimes(1)

    fireEvent.click(screen.getByRole('button', { name: 'Save MCP server' }))

    await waitFor(() =>
      expect(setup.upsertMcpServer).toHaveBeenCalledWith({
        id: 'filesystem',
        name: 'Filesystem Server',
        transport: {
          kind: 'stdio',
          command: 'node',
          args: ['/opt/mcp/server-filesystem.js'],
        },
        env: [
          {
            key: 'OPENAI_API_KEY',
            fromEnv: 'OPENAI_API_KEY',
          },
        ],
        cwd: null,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Import MCP servers' }))
    await waitFor(() => expect(setup.importMcpServers).toHaveBeenCalledWith('/tmp/mcp-import.json'))
    expect(screen.getByTestId('mcp-import-diagnostics-count')).toHaveTextContent('1')

    fireEvent.click(screen.getByRole('button', { name: 'Refresh MCP statuses' }))
    await waitFor(() => expect(setup.refreshMcpServerStatuses).toHaveBeenCalledWith({ serverIds: ['memory'] }))

    fireEvent.click(screen.getByRole('button', { name: 'Remove MCP server' }))
    await waitFor(() => expect(setup.removeMcpServer).toHaveBeenCalledWith('memory'))
  })

  it('keeps the last truthful MCP snapshot visible when refresh or contract-parse checks fail', async () => {
    const setup = createMockAdapter({
      listProjects: { projects: [makeProjectSummary('project-1', 'Xero')] },
      mcpRegistry: makeMcpRegistry({
        servers: [
          {
            ...makeMcpRegistry().servers[0],
            connection: {
              status: 'stale',
              diagnostic: {
                code: 'mcp_status_unchecked',
                message: 'Xero has not checked this MCP server yet.',
                retryable: true,
              },
              lastCheckedAt: null,
              lastHealthyAt: null,
            },
          },
        ],
      }),
    })

    setup.refreshMcpServerStatuses.mockRejectedValueOnce(
      new XeroDesktopError({
        code: 'mcp_status_refresh_failed',
        errorClass: 'retryable',
        message: 'Xero could not refresh MCP server statuses.',
        retryable: true,
      }),
    )

    render(<Harness adapter={setup.adapter} />)

    await waitFor(() => expect(screen.getByTestId('mcp-registry-first-status')).toHaveTextContent('stale'))

    fireEvent.click(screen.getByRole('button', { name: 'Refresh MCP statuses' }))

    await waitFor(() =>
      expect(screen.getByTestId('mcp-registry-mutation-error-code')).toHaveTextContent('mcp_status_refresh_failed'),
    )
    expect(screen.getByTestId('mcp-registry-first-status')).toHaveTextContent('stale')

    setup.listMcpServers.mockRejectedValueOnce(
      new XeroDesktopError({
        code: 'adapter_contract_mismatch',
        errorClass: 'adapter_contract_mismatch',
        message: 'Command list_mcp_servers returned an unexpected payload shape.',
        retryable: false,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Load MCP registry' }))

    await waitFor(() =>
      expect(screen.getByTestId('mcp-registry-load-error-code')).toHaveTextContent('adapter_contract_mismatch'),
    )
    expect(screen.getByTestId('mcp-registry-first-status')).toHaveTextContent('stale')
  })














})
