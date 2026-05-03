import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const {
  githubLoginMock,
  githubLogoutMock,
  githubRefreshMock,
  openUrlMock,
} = vi.hoisted(() => ({
  githubLoginMock: vi.fn(async () => undefined),
  githubLogoutMock: vi.fn(async () => undefined),
  githubRefreshMock: vi.fn(async () => undefined),
  openUrlMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

vi.mock('@/src/lib/github-auth', () => ({
  useGitHubAuth: () => ({
    session: null,
    status: 'idle',
    error: null,
    login: githubLoginMock,
    logout: githubLogoutMock,
    refresh: githubRefreshMock,
  }),
}))

vi.mock('@/components/ui/tooltip', () => ({
  Tooltip: ({ children }: any) => <>{children}</>,
  TooltipContent: () => null,
  TooltipProvider: ({ children }: any) => <>{children}</>,
  TooltipTrigger: ({ children }: any) => <>{children}</>,
}))

vi.mock('../components/xero/code-editor', async () => {
  const React = await import('react')

  function MockCodeEditor({
    documentVersion,
    filePath,
    onDirtyChange,
    onDocumentStatsChange,
    onSave,
    onSnapshotChange,
    onViewReady,
    savedValue = '',
    value,
  }: any) {
    const [draft, setDraft] = React.useState(value)
    const draftRef = React.useRef(value)

    React.useEffect(() => {
      setDraft(value)
      draftRef.current = value
    }, [documentVersion, filePath, value])

    React.useEffect(() => {
      const view = {
        state: {
          doc: {
            toString: () => draftRef.current,
          },
        },
      }
      onViewReady?.(view)
      return () => onViewReady?.(null)
    }, [onViewReady])

    return (
      <div>
        <label>
          <span className="sr-only">Editor for {filePath}</span>
          <textarea
            aria-label={`Editor for ${filePath}`}
            onChange={(event) => {
              const next = event.target.value
              draftRef.current = next
              setDraft(next)
              onDirtyChange?.(next !== savedValue)
              onDocumentStatsChange?.({ lineCount: next.length === 0 ? 1 : next.split('\n').length })
            }}
            onBlur={() => onSnapshotChange?.(draftRef.current)}
            value={draft}
          />
        </label>
        <button onClick={() => onSave?.(draftRef.current)} type="button">
          Trigger save
        </button>
      </div>
    )
  }

  return { CodeEditor: MockCodeEditor }
})

afterEach(() => {
  githubLoginMock.mockClear()
  githubLogoutMock.mockClear()
  githubRefreshMock.mockClear()
  openUrlMock.mockReset()
  if (typeof window.localStorage?.clear === 'function') {
    window.localStorage.clear()
  }
  if (typeof window.sessionStorage?.clear === 'function') {
    window.sessionStorage.clear()
  }
})

import { XeroApp, useActivatedSurface } from './App'
import { XeroDesktopError, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import {
  createXeroDoctorReport,
  providerModelCatalogSchema,
} from '@/src/lib/xero-model'
import {
  providerProfilesSchema,
  type ProviderProfileDto,
  type ProviderProfileReadinessDto,
  type ProviderProfilesDto,
} from '@/src/test/legacy-provider-profiles'
import type {
  AutonomousRunStateDto,
  EnvironmentDiscoveryStatusDto,
  ImportMcpServersResponseDto,
  ImportRepositoryResponseDto,
  ListNotificationDispatchesResponseDto,
  ListNotificationRoutesResponseDto,
  ListProjectFilesResponseDto,
  ListProjectsResponseDto,
  McpImportDiagnosticDto,
  McpRegistryDto,
  ProjectSnapshotResponseDto,
  ProjectUpdatedPayloadDto,
  ProjectUsageSummaryDto,
  ProviderAuthSessionDto,
  ProviderModelCatalogDto,
  RepositoryDiffResponseDto,
  RepositoryStatusChangedPayloadDto,
  RepositoryStatusResponseDto,
  RuntimeRunControlInputDto,
  RuntimeRunDto,
  RuntimeRunUpdatedPayloadDto,
  RuntimeSessionDto,
  RuntimeSettingsDto,
  RuntimeStreamEventDto,
  RuntimeUpdatedPayloadDto,
  SubscribeRuntimeStreamResponseDto,
  SkillRegistryDto,
  SyncNotificationAdaptersResponseDto,
  UpsertMcpServerRequestDto,
  UpsertNotificationRouteRequestDto,
} from '@/src/lib/xero-model'
import {
  getCloudProviderPreset,
  type CloudProviderPreset,
} from '@/src/lib/xero-model/provider-presets'

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

function makeAgentSession(projectId = 'project-1') {
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

function makeSnapshot(projectId = 'project-1', name = 'Xero'): ProjectSnapshotResponseDto {
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
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    agentSessions: [makeAgentSession(projectId)],
    notificationDispatches: [],
    notificationReplyClaims: [],
  }
}

function makeStatus(projectId = 'project-1', name = 'Xero'): RepositoryStatusResponseDto {
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
      rootPath: '/tmp/Xero',
      displayName: 'Xero',
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
    path: '/',
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

function makeProviderAuthSession(overrides: Partial<ProviderAuthSessionDto> = {}): ProviderAuthSessionDto {
  return {
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
    anthropicApiKeyConfigured: false,
    ...overrides,
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
            code: 'runtime_mcp_projection_unchecked',
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
    entries: [],
    plugins: [],
    pluginCommands: [],
    sources: {
      localRoots: [],
      pluginRoots: [],
      github: {
        repo: 'owner/skills',
        reference: 'main',
        root: 'skills',
        enabled: true,
        updatedAt: '2026-04-24T04:00:00Z',
      },
      projects: [],
      updatedAt: '2026-04-24T04:00:00Z',
    },
    diagnostics: [],
    reloadedAt: '2026-04-24T04:00:00Z',
    ...overrides,
  }
}

function makeEnvironmentDiscoveryStatus(
  overrides: Partial<EnvironmentDiscoveryStatusDto> = {},
): EnvironmentDiscoveryStatusDto {
  return {
    hasProfile: false,
    status: 'pending',
    stale: true,
    shouldStart: true,
    refreshedAt: null,
    probeStartedAt: null,
    probeCompletedAt: null,
    permissionRequests: [],
    diagnostics: [],
    ...overrides,
  }
}

function getRequiredCloudProviderPreset(providerId: ProviderProfileDto['providerId'], context: string): CloudProviderPreset {
  const preset = getCloudProviderPreset(providerId)
  if (!preset) {
    throw new Error(`${context} could not resolve provider preset for \`${providerId}\`.`)
  }

  return preset
}

function makeMissingProviderReadiness(
  status: ProviderProfileReadinessDto['status'] = 'missing',
  proofUpdatedAt: string | null = null,
): ProviderProfileReadinessDto {
  return {
    ready: false,
    status,
    proof: null,
    proofUpdatedAt: status === 'malformed' ? proofUpdatedAt ?? '2026-04-16T14:05:00Z' : null,
  }
}

function makeReadyProviderReadiness(preset: CloudProviderPreset): ProviderProfileReadinessDto {
  return {
    ready: true,
    status: 'ready',
    proof:
      preset.authMode === 'oauth'
        ? 'oauth_session'
        : preset.authMode === 'api_key'
          ? 'stored_secret'
          : preset.authMode,
    proofUpdatedAt: '2026-04-16T14:05:00Z',
  }
}

function getLegacyProviderReadiness(runtimeSettings: RuntimeSettingsDto, preset: CloudProviderPreset): ProviderProfileReadinessDto {
  switch (preset.providerId) {
    case 'openrouter':
      return runtimeSettings.openrouterApiKeyConfigured
        ? makeReadyProviderReadiness(preset)
        : makeMissingProviderReadiness()
    case 'anthropic':
      return runtimeSettings.anthropicApiKeyConfigured
        ? makeReadyProviderReadiness(preset)
        : makeMissingProviderReadiness()
    default:
      return makeMissingProviderReadiness()
  }
}

function makeProviderProfilesFromRuntimeSettings(runtimeSettings: RuntimeSettingsDto): ProviderProfilesDto {
  const preset = getRequiredCloudProviderPreset(
    runtimeSettings.providerId,
    'makeProviderProfilesFromRuntimeSettings',
  )

  return providerProfilesSchema.parse({
    activeProfileId: preset.defaultProfileId,
    profiles: [
      {
        profileId: preset.defaultProfileId,
        providerId: preset.providerId,
        runtimeKind: preset.runtimeKind,
        label: preset.defaultProfileLabel,
        modelId: runtimeSettings.modelId,
        presetId: preset.presetId ?? null,
        baseUrl: null,
        apiVersion: null,
        region: null,
        projectId: null,
        active: true,
        readiness: getLegacyProviderReadiness(runtimeSettings, preset),
        migratedFromLegacy: false,
        migratedAt: null,
      },
    ],
    migration: null,
  })
}

function applyOpenAiRuntimeReadinessToProfiles(
  providerProfiles: ProviderProfilesDto,
  runtimeSession: RuntimeSessionDto,
): ProviderProfilesDto {
  if (runtimeSession.providerId !== 'openai_codex' || runtimeSession.phase !== 'authenticated') {
    return providerProfiles
  }

  return providerProfilesSchema.parse({
    ...providerProfiles,
    profiles: providerProfiles.profiles.map((profile) =>
      profile.profileId === providerProfiles.activeProfileId && profile.providerId === 'openai_codex'
        ? {
            ...profile,
            readiness: makeReadyProviderReadiness(
              getRequiredCloudProviderPreset('openai_codex', 'applyOpenAiRuntimeReadinessToProfiles'),
            ),
          }
        : profile,
    ),
  })
}

function buildProviderModelCatalog(profile: ProviderProfileDto): ProviderModelCatalogDto {
  const preset = getRequiredCloudProviderPreset(profile.providerId, 'buildProviderModelCatalog')
  const isReady = preset.authMode === 'oauth' ? true : profile.readiness.ready

  const lastRefreshError = (() => {
    if (isReady) {
      return null
    }

    switch (preset.providerId) {
      case 'openrouter':
        return {
          code: 'openrouter_api_key_missing',
          message: `Xero cannot discover OpenRouter models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'anthropic':
        return {
          code: 'anthropic_api_key_missing',
          message: `Xero cannot discover Anthropic models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'github_models':
        return {
          code: 'github_models_token_missing',
          message: `Xero cannot discover GitHub Models for provider profile \`${profile.profileId}\` because no app-local GitHub token is configured for that profile.`,
          retryable: false,
        }
      case 'openai_api':
        return {
          code: 'openai_api_key_missing',
          message: `Xero cannot discover OpenAI-compatible models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'ollama':
        return {
          code: 'local_provider_unreachable',
          message: `Xero cannot discover Ollama models for provider profile \`${profile.profileId}\` because the local endpoint is not ready yet.`,
          retryable: false,
        }
      case 'azure_openai':
        return {
          code: 'azure_openai_api_key_missing',
          message: `Xero cannot discover Azure OpenAI models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'gemini_ai_studio':
        return {
          code: 'gemini_ai_studio_api_key_missing',
          message: `Xero cannot discover Gemini AI Studio models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'bedrock':
        return {
          code: 'bedrock_ambient_proof_missing',
          message: `Xero cannot validate Amazon Bedrock model availability for provider profile \`${profile.profileId}\` because the profile is missing its ambient readiness proof link. Save the profile again so Xero records ambient-auth intent.`,
          retryable: false,
        }
      case 'vertex':
        return {
          code: 'vertex_ambient_proof_missing',
          message: `Xero cannot validate Google Vertex AI model availability for provider profile \`${profile.profileId}\` because the profile is missing its ambient readiness proof link. Save the profile again so Xero records ambient-auth intent.`,
          retryable: false,
        }
      case 'openai_codex':
        return null
    }
  })()

  const models = isReady
    ? (() => {
        switch (preset.providerId) {
          case 'openrouter':
            return [
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
          case 'anthropic':
            return [
              {
                modelId: 'claude-3-7-sonnet-latest',
                displayName: 'Claude 3.7 Sonnet',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high', 'x_high'],
                  defaultEffort: 'medium',
                },
              },
              {
                modelId: 'claude-3-5-haiku-latest',
                displayName: 'Claude 3.5 Haiku',
                thinking: {
                  supported: false,
                  effortOptions: [],
                  defaultEffort: null,
                },
              },
            ]
          case 'github_models':
            return [
              {
                modelId: 'openai/gpt-4.1',
                displayName: 'OpenAI GPT-4.1',
                thinking: {
                  supported: true,
                  effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                  defaultEffort: 'medium',
                },
              },
              {
                modelId: 'meta/Llama-4-Scout-17B-16E-Instruct',
                displayName: 'Llama 4 Scout 17B',
                thinking: {
                  supported: false,
                  effortOptions: [],
                  defaultEffort: null,
                },
              },
            ]
          case 'openai_api':
            return [
              {
                modelId: profile.modelId,
                displayName: 'OpenAI-compatible model',
                thinking: {
                  supported: true,
                  effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                  defaultEffort: 'medium',
                },
              },
            ]
          case 'ollama':
            return [
              {
                modelId: 'llama3.2',
                displayName: 'Llama 3.2',
                thinking: {
                  supported: false,
                  effortOptions: [],
                  defaultEffort: null,
                },
              },
            ]
          case 'azure_openai':
            return [
              {
                modelId: profile.modelId,
                displayName: 'Azure OpenAI deployment',
                thinking: {
                  supported: true,
                  effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                  defaultEffort: 'medium',
                },
              },
            ]
          case 'gemini_ai_studio':
            return [
              {
                modelId: 'gemini-2.5-flash',
                displayName: 'Gemini 2.5 Flash',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high'],
                  defaultEffort: 'medium',
                },
              },
            ]
          case 'bedrock':
            return [
              {
                modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
                displayName: 'Claude 3.7 Sonnet (Bedrock)',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high'],
                  defaultEffort: 'medium',
                },
              },
            ]
          case 'vertex':
            return [
              {
                modelId: 'claude-3-7-sonnet@20250219',
                displayName: 'Claude 3.7 Sonnet (Vertex)',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high'],
                  defaultEffort: 'medium',
                },
              },
            ]
          case 'openai_codex':
            return [
              {
                modelId: 'openai_codex',
                displayName: 'OpenAI Codex',
                thinking: {
                  supported: true,
                  effortOptions: ['low', 'medium', 'high'],
                  defaultEffort: 'medium',
                },
              },
            ]
        }
      })()
    : []

  return providerModelCatalogSchema.parse({
    profileId: profile.profileId,
    providerId: profile.providerId,
    configuredModelId: profile.modelId,
    source: isReady ? 'live' : 'unavailable',
    fetchedAt: isReady ? '2026-04-21T12:00:00Z' : null,
    lastSuccessAt: isReady ? '2026-04-21T12:00:00Z' : null,
    lastRefreshError,
    models,
  })
}

function makeRuntimeRun(projectId = 'project-1', overrides: Partial<RuntimeRunDto> = {}): RuntimeRunDto {
  const runtimeRun: RuntimeRunDto = {
    projectId,
    agentSessionId: 'agent-session-main',
    runId: 'run-1',
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

function ensureKnownProjectId(projectId: string, knownProjectIds: string[], context: string) {
  if (knownProjectIds.includes(projectId)) {
    return
  }

  throw new Error(
    `${context} expected one of [${knownProjectIds.join(', ')}] but received projectId \`${projectId}\`.`,
  )
}

function ensureCompatibleRuntimeRun(
  projectId: string,
  currentRuntimeRun: RuntimeRunDto | null,
  nextRuntimeRun: RuntimeRunDto | null,
  context: string,
) {
  if (!nextRuntimeRun) {
    return
  }

  if (nextRuntimeRun.projectId !== projectId) {
    throw new Error(
      `${context} expected run.projectId \`${projectId}\` but received \`${nextRuntimeRun.projectId}\`.`,
    )
  }

  if (!currentRuntimeRun || currentRuntimeRun.runId === nextRuntimeRun.runId) {
    return
  }

  throw new Error(
    `${context} expected active runId \`${currentRuntimeRun.runId}\` for project \`${projectId}\`; clear the active run before attaching \`${nextRuntimeRun.runId}\`.`,
  )
}

function cloneRuntimeRun(runtimeRun: RuntimeRunDto): RuntimeRunDto {
  return {
    ...runtimeRun,
    transport: { ...runtimeRun.transport },
    controls: {
      active: { ...runtimeRun.controls.active },
      pending: runtimeRun.controls.pending ? { ...runtimeRun.controls.pending } : null,
    },
    lastError: runtimeRun.lastError ? { ...runtimeRun.lastError } : null,
    checkpoints: runtimeRun.checkpoints.map((checkpoint) => ({ ...checkpoint })),
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
  mcpRegistry?: McpRegistryDto
  skillRegistry?: SkillRegistryDto
  runtimeRun?: RuntimeRunDto | null
  autonomousState?: AutonomousRunStateDto | null
  notificationRoutes?: ListNotificationRoutesResponseDto['routes']
  environmentDiscoveryStatus?: EnvironmentDiscoveryStatusDto
  projectFiles?: ListProjectFilesResponseDto
  pickedRepositoryPath?: string | null
  usageSummary?: ProjectUsageSummaryDto
}) {
  let currentSnapshot = options?.snapshot ?? makeSnapshot()
  let currentStatus = options?.status ?? makeStatus()
  let currentDiff = options?.diff ?? makeDiff()
  let currentRuntimeSession = options?.runtimeSession ?? makeRuntimeSession()
  let currentProviderProfiles = providerProfilesSchema.parse(
    options?.providerProfiles ?? makeProviderProfilesFromRuntimeSettings(options?.runtimeSettings ?? makeRuntimeSettings()),
  )
  if (!options?.providerProfiles) {
    currentProviderProfiles = applyOpenAiRuntimeReadinessToProfiles(
      currentProviderProfiles,
      currentRuntimeSession,
    )
  }
  let currentProviderModelCatalogs: Record<string, ProviderModelCatalogDto> = Object.fromEntries(
    currentProviderProfiles.profiles.map((profile) => [profile.profileId, buildProviderModelCatalog(profile)]),
  )
  let currentMcpRegistry = options?.mcpRegistry ?? makeMcpRegistry()
  let currentSkillRegistry = options?.skillRegistry ?? makeSkillRegistry()
  let currentMcpImportDiagnostics: McpImportDiagnosticDto[] = []
  let currentRuntimeRun = options?.runtimeRun ?? null
  let currentAutonomousState = options?.autonomousState ?? null
  let currentNotificationRoutes = options?.notificationRoutes ?? []
  let currentEnvironmentDiscoveryStatus =
    options?.environmentDiscoveryStatus ?? makeEnvironmentDiscoveryStatus()
  let currentProjects = options?.projects ?? [makeProjectSummary('project-1', 'Xero')]
  let currentProjectFiles = options?.projectFiles ?? makeProjectFiles()
  const currentUsageSummary =
    options?.usageSummary ??
    ({
      projectId: 'project-1',
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
    } satisfies ProjectUsageSummaryDto)
  const updateAgentSessions = (agentSessions: ProjectSnapshotResponseDto['agentSessions']) => {
    currentSnapshot = {
      ...currentSnapshot,
      agentSessions,
    }
  }
  const pickedRepositoryPath = options?.pickedRepositoryPath ?? null
  const currentFileContents: Record<string, string> = {
    '/README.md': '# Xero\n',
    '/src/App.tsx': 'export default function App() {\n  return <main>Xero</main>\n}\n',
  }
  let projectUpdatedHandler: ((payload: ProjectUpdatedPayloadDto) => void) | null = null
  let projectUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null
  let runtimeUpdatedHandler: ((payload: RuntimeUpdatedPayloadDto) => void) | null = null
  let runtimeUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedErrorHandler: ((error: XeroDesktopError) => void) | null = null
  const streamSubscriptions: Array<{
    projectId: string
    handler: (payload: RuntimeStreamEventDto) => void
    onError: ((error: XeroDesktopError) => void) | null
    unsubscribe: () => void
  }> = []

  const rebuildProviderModelCatalogs = () => {
    currentProviderModelCatalogs = Object.fromEntries(
      currentProviderProfiles.profiles.map((profile) => [profile.profileId, buildProviderModelCatalog(profile)]),
    )
  }

  const getActiveProviderProfileSnapshot = (): ProviderProfileDto => {
    const activeProfile =
      currentProviderProfiles.profiles.find((profile) => profile.profileId === currentProviderProfiles.activeProfileId) ??
      null

    if (!activeProfile) {
      throw new Error(
        `createAdapter expected activeProfileId \`${currentProviderProfiles.activeProfileId}\` to match a stored provider profile.`,
      )
    }

    return activeProfile
  }

  const cloneRuntimeRunActiveControls = (runtimeRun: RuntimeRunDto) => ({
    ...runtimeRun.controls.active,
  })

  const cloneRuntimeRunPendingControls = (runtimeRun: RuntimeRunDto) =>
    runtimeRun.controls.pending ? { ...runtimeRun.controls.pending } : null

  const getRuntimeKindForProvider = (providerId: RuntimeSettingsDto['providerId']) =>
    getRequiredCloudProviderPreset(providerId, 'createAdapter.getRuntimeKindForProvider').runtimeKind

  const buildRuntimeRunControls = (options: {
    base: RuntimeRunDto['controls']['active']
    nextControls?: RuntimeRunControlInputDto | null
    revision: number
    appliedAt?: string
    queuedAt?: string
    queuedPrompt?: string | null
    queuedPromptAt?: string | null
  }): RuntimeRunDto['controls'] => {
    const providerProfileId = options.nextControls?.providerProfileId ?? options.base.providerProfileId ?? null
    const modelId = options.nextControls?.modelId ?? options.base.modelId
    const runtimeAgentId = options.nextControls?.runtimeAgentId ?? options.base.runtimeAgentId
    const thinkingEffort = options.nextControls?.thinkingEffort ?? options.base.thinkingEffort
    const approvalMode = options.nextControls?.approvalMode ?? options.base.approvalMode
    const planModeRequired = options.nextControls?.planModeRequired ?? options.base.planModeRequired

    return {
      active: {
        providerProfileId,
        runtimeAgentId,
        modelId,
        thinkingEffort,
        approvalMode,
        planModeRequired,
        revision: options.revision,
        appliedAt: options.appliedAt ?? options.base.appliedAt,
      },
      pending:
        options.queuedAt || options.queuedPrompt != null
          ? {
              providerProfileId,
              runtimeAgentId,
              modelId,
              thinkingEffort,
              approvalMode,
              planModeRequired,
              revision: options.revision,
              queuedAt: options.queuedAt ?? options.appliedAt ?? options.base.appliedAt,
              queuedPrompt: options.queuedPrompt ?? null,
              queuedPromptAt: options.queuedPromptAt ?? null,
            }
          : null,
    }
  }

  const mergePendingRuntimeRunControls = (runtimeRun: RuntimeRunDto, request?: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null }) => {
    const activeControls = cloneRuntimeRunActiveControls(runtimeRun)
    const existingPending = cloneRuntimeRunPendingControls(runtimeRun)
    const nextRevision = existingPending?.revision ?? activeControls.revision + 1
    const nextQueuedAt = existingPending?.queuedAt ?? '2026-04-22T12:05:00Z'
    const nextQueuedPrompt = request?.prompt ?? existingPending?.queuedPrompt ?? null
    const nextQueuedPromptAt = request?.prompt ? '2026-04-22T12:05:30Z' : existingPending?.queuedPromptAt ?? null

    return buildRuntimeRunControls({
      base: activeControls,
      nextControls: request?.controls ?? existingPending,
      revision: nextRevision,
      queuedAt: nextQueuedAt,
      queuedPrompt: nextQueuedPrompt,
      queuedPromptAt: nextQueuedPromptAt,
    })
  }

  const queuePendingRuntimeRunSnapshot = (request?: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null }) => {
    const activeProfile = getActiveProviderProfileSnapshot()

    currentRuntimeRun = currentRuntimeRun
      ? makeRuntimeRun('project-1', {
          ...currentRuntimeRun,
          runtimeKind: getRuntimeKindForProvider(activeProfile.providerId),
          providerId: activeProfile.providerId,
          controls: mergePendingRuntimeRunControls(currentRuntimeRun, request),
          lastHeartbeatAt: '2026-04-22T12:05:30Z',
          updatedAt: '2026-04-22T12:05:30Z',
        })
      : makeRuntimeRun('project-1', {
          runtimeKind: getRuntimeKindForProvider(activeProfile.providerId),
          providerId: activeProfile.providerId,
          controls: buildRuntimeRunControls({
            base: makeRuntimeRun('project-1').controls.active,
            nextControls: {
              providerProfileId: request?.controls?.providerProfileId ?? activeProfile.profileId,
              runtimeAgentId: request?.controls?.runtimeAgentId ?? 'ask',
              modelId: request?.controls?.modelId ?? activeProfile.modelId,
              thinkingEffort: request?.controls?.thinkingEffort ?? 'medium',
              approvalMode: request?.controls?.approvalMode ?? 'suggest',
              planModeRequired: request?.controls?.planModeRequired ?? false,
            },
            revision: 1,
            appliedAt: '2026-04-22T12:05:30Z',
            queuedAt: request?.prompt ? '2026-04-22T12:05:30Z' : undefined,
            queuedPrompt: request?.prompt ?? null,
            queuedPromptAt: request?.prompt ? '2026-04-22T12:05:30Z' : null,
          }),
          lastHeartbeatAt: '2026-04-22T12:05:30Z',
          updatedAt: '2026-04-22T12:05:30Z',
        })

    return currentRuntimeRun
  }

  const startRuntimeRunSnapshot = (options?: { initialControls?: RuntimeRunControlInputDto | null; initialPrompt?: string | null }) => {
    const activeProfile = getActiveProviderProfileSnapshot()
    const activeControls = buildRuntimeRunControls({
      base: makeRuntimeRun('project-1').controls.active,
      nextControls: {
        providerProfileId: options?.initialControls?.providerProfileId ?? activeProfile.profileId,
        runtimeAgentId: options?.initialControls?.runtimeAgentId ?? 'ask',
        modelId: options?.initialControls?.modelId ?? activeProfile.modelId,
        thinkingEffort: options?.initialControls?.thinkingEffort ?? 'medium',
        approvalMode: options?.initialControls?.approvalMode ?? 'suggest',
        planModeRequired: options?.initialControls?.planModeRequired ?? false,
      },
      revision: 1,
      appliedAt: '2026-04-22T12:00:00Z',
      queuedAt: options?.initialPrompt ? '2026-04-22T12:00:30Z' : undefined,
      queuedPrompt: options?.initialPrompt ?? null,
      queuedPromptAt: options?.initialPrompt ? '2026-04-22T12:00:30Z' : null,
    })

    currentRuntimeRun = makeRuntimeRun('project-1', {
      runtimeKind: getRuntimeKindForProvider(activeProfile.providerId),
      providerId: activeProfile.providerId,
      controls: activeControls,
      startedAt: '2026-04-22T12:00:00Z',
      lastHeartbeatAt: '2026-04-22T12:00:05Z',
      lastCheckpointAt: '2026-04-22T12:00:06Z',
      updatedAt: '2026-04-22T12:00:06Z',
    })

    return currentRuntimeRun
  }

  const getKnownProjectIds = () => {
    const projectIds = new Set<string>(currentProjects.map((project) => project.id))
    projectIds.add(currentSnapshot.project.id)
    projectIds.add(currentRuntimeSession.projectId)
    if (currentRuntimeRun) {
      projectIds.add(currentRuntimeRun.projectId)
    }
    if (currentAutonomousState?.run) {
      projectIds.add(currentAutonomousState.run.projectId)
    }
    return Array.from(projectIds)
  }

  const applyProjectUpdatedPayload = (payload: ProjectUpdatedPayloadDto) => {
    ensureKnownProjectId(payload.project.id, getKnownProjectIds(), 'emitProjectUpdated')
    currentProjects = currentProjects.map((project) =>
      project.id === payload.project.id ? payload.project : project,
    )

    if (currentSnapshot.project.id === payload.project.id) {
      const currentRepository =
        currentSnapshot.repository ?? {
          id: `repo-${payload.project.id}`,
          projectId: payload.project.id,
          rootPath: `/tmp/${payload.project.name}`,
          displayName: payload.project.name,
          branch: null,
          headSha: null,
          isGitRepo: true,
        }

      currentSnapshot = {
        ...currentSnapshot,
        project: payload.project,
        repository: {
          ...currentRepository,
          projectId: payload.project.id,
          displayName: payload.project.name,
        },
      }
    }
  }

  const applyRuntimeUpdatedPayload = (payload: RuntimeUpdatedPayloadDto) => {
    ensureKnownProjectId(payload.projectId, getKnownProjectIds(), 'emitRuntimeUpdated')
    currentRuntimeSession = makeRuntimeSession(payload.projectId, {
      runtimeKind: payload.runtimeKind,
      providerId: payload.providerId,
      flowId: payload.flowId,
      sessionId: payload.sessionId,
      accountId: payload.accountId,
      phase: payload.authPhase,
      callbackBound: currentRuntimeSession.callbackBound,
      authorizationUrl: currentRuntimeSession.authorizationUrl,
      redirectUri: currentRuntimeSession.redirectUri,
      lastErrorCode: payload.lastErrorCode,
      lastError: payload.lastError,
      updatedAt: payload.updatedAt,
    })
    currentProviderProfiles = applyOpenAiRuntimeReadinessToProfiles(
      currentProviderProfiles,
      currentRuntimeSession,
    )
    rebuildProviderModelCatalogs()
  }

  const applyRuntimeRunUpdatedPayload = (payload: RuntimeRunUpdatedPayloadDto) => {
    ensureKnownProjectId(payload.projectId, getKnownProjectIds(), 'emitRuntimeRunUpdated')
    ensureCompatibleRuntimeRun(payload.projectId, currentRuntimeRun, payload.run, 'emitRuntimeRunUpdated')
    currentRuntimeRun = payload.run ? cloneRuntimeRun(payload.run) : null
  }

  let currentProviderCredentials: { credentials: import('@/src/lib/xero-model').ProviderCredentialDto[] } = {
    credentials: [],
  }
  const listProviderCredentials = vi.fn(async () => currentProviderCredentials)
  const upsertProviderCredential = vi.fn(async (request: import('@/src/lib/xero-model').UpsertProviderCredentialRequestDto) => {
    const next = currentProviderCredentials.credentials.filter((c) => c.providerId !== request.providerId)
    next.push({
      providerId: request.providerId,
      kind: request.kind,
      hasApiKey: typeof request.apiKey === 'string' && request.apiKey.length > 0,
      oauthAccountId: null,
      oauthSessionId: null,
      hasOauthAccessToken: false,
      oauthExpiresAt: null,
      baseUrl: request.baseUrl ?? null,
      apiVersion: request.apiVersion ?? null,
      region: request.region ?? null,
      projectId: request.projectId ?? null,
      defaultModelId: request.defaultModelId ?? null,
      readinessProof:
        request.kind === 'api_key'
          ? 'stored_secret'
          : request.kind === 'local'
            ? 'local'
            : 'ambient',
      updatedAt: '2026-04-15T20:00:00.000Z',
    })
    currentProviderCredentials = { credentials: next }
    return currentProviderCredentials
  })
  const deleteProviderCredential = vi.fn(async (providerId: string) => {
    currentProviderCredentials = {
      credentials: currentProviderCredentials.credentials.filter((c) => c.providerId !== providerId),
    }
    return currentProviderCredentials
  })

  const listMcpServers = vi.fn(async () => currentMcpRegistry)

  const upsertMcpServer = vi.fn(async (request: UpsertMcpServerRequestDto) => {
    const now = '2026-04-24T05:00:00Z'
    const existing = currentMcpRegistry.servers.filter((server) => server.id !== request.id)
    const previous = currentMcpRegistry.servers.find((server) => server.id === request.id)

    currentMcpRegistry = {
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
              code: 'runtime_mcp_projection_unchecked',
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

    return currentMcpRegistry
  })

  const removeMcpServer = vi.fn(async (serverId: string) => {
    currentMcpRegistry = {
      ...currentMcpRegistry,
      updatedAt: '2026-04-24T05:01:00Z',
      servers: currentMcpRegistry.servers.filter((server) => server.id !== serverId),
    }

    return currentMcpRegistry
  })

  const importMcpServers = vi.fn(async (_path: string): Promise<ImportMcpServersResponseDto> => {
    const now = '2026-04-24T05:02:00Z'
    const importedServer = {
      id: 'linear',
      name: 'Linear MCP',
      transport: {
        kind: 'http' as const,
        url: 'https://mcp.linear.app/http',
      },
      env: [
        {
          key: 'LINEAR_API_KEY',
          fromEnv: 'LINEAR_API_KEY',
        },
      ],
      cwd: null,
      connection: {
        status: 'failed' as const,
        diagnostic: {
          code: 'runtime_mcp_projection_decode_failed',
          message: 'Xero kept the last truthful MCP projection because the imported status payload was malformed.',
          retryable: true,
        },
        lastCheckedAt: now,
        lastHealthyAt: null,
      },
      updatedAt: now,
    }

    currentMcpRegistry = {
      updatedAt: now,
      servers: [
        importedServer,
        ...currentMcpRegistry.servers.filter((server) => server.id !== importedServer.id),
      ],
    }
    currentMcpImportDiagnostics = [
      {
        index: 0,
        serverId: 'linear',
        code: 'runtime_mcp_projection_decode_failed',
        message: 'Xero preserved the last truthful MCP projection while parsing imported rows.',
      },
    ]

    return {
      registry: currentMcpRegistry,
      diagnostics: currentMcpImportDiagnostics,
    }
  })

  const refreshMcpServerStatuses = vi.fn(async (options?: { serverIds?: string[] }) => {
    const serverIds = options?.serverIds ?? []
    const shouldRefresh = (id: string) => serverIds.length === 0 || serverIds.includes(id)

    currentMcpRegistry = {
      ...currentMcpRegistry,
      updatedAt: '2026-04-24T05:03:00Z',
      servers: currentMcpRegistry.servers.map((server) =>
        shouldRefresh(server.id)
          ? {
              ...server,
              connection: {
                status: 'connected',
                diagnostic: null,
                lastCheckedAt: '2026-04-24T05:03:00Z',
                lastHealthyAt: '2026-04-24T05:03:00Z',
              },
            }
          : server,
      ),
    }

    return currentMcpRegistry
  })

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

  const startRuntimeRun = vi.fn(async (_projectId: string, _agentSessionId: string, options?: { initialControls?: RuntimeRunControlInputDto | null; initialPrompt?: string | null }) =>
    startRuntimeRunSnapshot(options),
  )

  const updateRuntimeRunControls = vi.fn(async (request?: {
    projectId: string
    agentSessionId: string
    runId: string
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
  }) => queuePendingRuntimeRunSnapshot(request))

  const startAutonomousRun = vi.fn(async (_projectId: string, _agentSessionId: string, _options?: { initialControls?: RuntimeRunControlInputDto | null; initialPrompt?: string | null }) => {
    currentAutonomousState = makeAutonomousRunState('project-1')
    return currentAutonomousState
  })
  const getEnvironmentDiscoveryStatus = vi.fn(async () => currentEnvironmentDiscoveryStatus)
  const startEnvironmentDiscovery = vi.fn(async () => {
    currentEnvironmentDiscoveryStatus = {
      ...currentEnvironmentDiscoveryStatus,
      hasProfile: true,
      status: 'probing',
      shouldStart: false,
      probeStartedAt: '2026-04-30T18:00:00Z',
      refreshedAt: '2026-04-30T18:00:00Z',
    }
    return currentEnvironmentDiscoveryStatus
  })
  const resolveEnvironmentPermissionRequests = vi.fn(async (request: {
    decisions: Array<{ id: string; status: 'granted' | 'denied' | 'skipped' }>
  }) => {
    const decisions = new Map(request.decisions.map((decision) => [decision.id, decision.status]))
    currentEnvironmentDiscoveryStatus = {
      ...currentEnvironmentDiscoveryStatus,
      permissionRequests: currentEnvironmentDiscoveryStatus.permissionRequests
        .map((permission) => ({
          ...permission,
          status: decisions.get(permission.id) ?? permission.status,
        }))
        .filter((permission) => permission.status === 'pending'),
    }
    return currentEnvironmentDiscoveryStatus
  })

  const pickRepositoryFolder = vi.fn(async () => pickedRepositoryPath)
  const importRepository = vi.fn(async (_path: string): Promise<ImportRepositoryResponseDto> => {
    const project = makeProjectSummary('project-1', 'Xero')
    currentProjects = [project]
    return {
      project,
      repository: makeStatus().repository,
    }
  })
  const onProjectUpdated = vi.fn(
    async (
      handler: (payload: ProjectUpdatedPayloadDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      projectUpdatedHandler = handler
      projectUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )
  const onRuntimeUpdated = vi.fn(
    async (
      handler: (payload: RuntimeUpdatedPayloadDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      runtimeUpdatedHandler = handler
      runtimeUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )
  const onRuntimeRunUpdated = vi.fn(
    async (
      handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      runtimeRunUpdatedHandler = handler
      runtimeRunUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )

  const adapter: XeroDesktopAdapter = {
    isDesktopRuntime: () => true,
    pickRepositoryFolder,
    pickParentFolder: async () => null,
    importRepository,
    createRepository: async () => ({
      project: currentSnapshot.project,
      repository: currentSnapshot.repository ?? {
        id: 'repo-stub',
        projectId: currentSnapshot.project.id,
        rootPath: '',
        displayName: '',
        branch: null,
        headSha: null,
        isGitRepo: true,
      },
    }),
    listProjects: async () => ({ projects: currentProjects }),
    removeProject: async (projectId) => {
      currentProjects = currentProjects.filter((project) => project.id !== projectId)
      return { projects: currentProjects }
    },
    getProjectSnapshot: async () => currentSnapshot,
    getProjectUsageSummary: async (projectId: string) => ({
      ...currentUsageSummary,
      projectId,
      totals: { ...currentUsageSummary.totals },
      byModel: currentUsageSummary.byModel.map((row) => ({ ...row })),
    }),
    getRepositoryStatus: async () => currentStatus,
    getRepositoryDiff: async (_projectId, scope) => ({ ...currentDiff, scope }),
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
    moveProjectEntry: async (request) => ({
      projectId: request.projectId,
      path:
        request.targetParentPath === '/'
          ? `/${request.path.split('/').filter(Boolean).pop() ?? ''}`
          : `${request.targetParentPath}/${request.path.split('/').filter(Boolean).pop() ?? ''}`,
    }),
    deleteProjectEntry: async (projectId, path) => ({ projectId, path }),
    searchProject: async (request) => ({
      projectId: request.projectId,
      totalMatches: 0,
      totalFiles: 0,
      truncated: false,
      files: [],
    }),
    replaceInProject: async (request) => ({
      projectId: request.projectId,
      filesChanged: 0,
      totalReplacements: 0,
    }),
    listAgentDefinitions: async () => ({ definitions: [] }),
    archiveAgentDefinition: async () => {
      throw new Error('archiveAgentDefinition not stubbed in test adapter')
    },
    getAgentDefinitionVersion: async () => null,
    createAgentSession: async (request) => {
      const now = '2026-04-23T12:00:00Z'
      const selected = request.selected ?? true
      const agentSession = {
        ...makeAgentSession(request.projectId),
        agentSessionId: `agent-session-${currentSnapshot.agentSessions.length + 1}`,
        title: request.title ?? `Session ${currentSnapshot.agentSessions.length + 1}`,
        summary: request.summary ?? '',
        selected,
        createdAt: now,
        updatedAt: now,
      }
      updateAgentSessions([
        ...currentSnapshot.agentSessions.map((session) =>
          selected && session.projectId === request.projectId ? { ...session, selected: false } : session,
        ),
        agentSession,
      ])
      return agentSession
    },
    listAgentSessions: async (request) => ({
      sessions: currentSnapshot.agentSessions.filter(
        (session) => session.projectId === request.projectId && (request.includeArchived || session.status !== 'archived'),
      ),
    }),
    getAgentSession: async (request) =>
      currentSnapshot.agentSessions.find(
        (session) => session.projectId === request.projectId && session.agentSessionId === request.agentSessionId,
      ) ?? null,
    updateAgentSession: async (request) => {
      const existing = currentSnapshot.agentSessions.find(
        (session) => session.projectId === request.projectId && session.agentSessionId === request.agentSessionId,
      )
      if (!existing) {
        throw new Error(`Missing agent session ${request.agentSessionId}`)
      }

      const selected = request.selected ?? existing.selected
      const nextSession = {
        ...existing,
        title: request.title ?? existing.title,
        summary: request.summary ?? existing.summary,
        selected,
        updatedAt: '2026-04-23T12:05:00Z',
      }
      updateAgentSessions(
        currentSnapshot.agentSessions.map((session) => {
          if (session.projectId !== request.projectId) {
            return session
          }
          if (session.agentSessionId === request.agentSessionId) {
            return nextSession
          }
          return selected ? { ...session, selected: false } : session
        }),
      )
      return nextSession
    },
    autoNameAgentSession: async (request) => {
      const existing = currentSnapshot.agentSessions.find(
        (session) => session.projectId === request.projectId && session.agentSessionId === request.agentSessionId,
      )
      if (!existing) {
        throw new Error(`Missing agent session ${request.agentSessionId}`)
      }

      const nextSession = {
        ...existing,
        title: existing.title.trim().toLowerCase() === 'new chat' ? 'Generated Session Title' : existing.title,
        updatedAt: '2026-04-23T12:06:00Z',
      }
      updateAgentSessions(
        currentSnapshot.agentSessions.map((session) =>
          session.projectId === request.projectId && session.agentSessionId === request.agentSessionId
            ? nextSession
            : session,
        ),
      )
      return nextSession
    },
    archiveAgentSession: async (request) => {
      const existing = currentSnapshot.agentSessions.find(
        (session) => session.projectId === request.projectId && session.agentSessionId === request.agentSessionId,
      )
      if (!existing) {
        throw new Error(`Missing agent session ${request.agentSessionId}`)
      }

      const archivedSession = {
        ...existing,
        status: 'archived' as const,
        selected: false,
        archivedAt: '2026-04-23T12:10:00Z',
        updatedAt: '2026-04-23T12:10:00Z',
      }
      updateAgentSessions(
        currentSnapshot.agentSessions.map((session) =>
          session.projectId === request.projectId && session.agentSessionId === request.agentSessionId
            ? archivedSession
            : session,
        ),
      )
      return archivedSession
    },
    restoreAgentSession: async (request) => {
      const existing = currentSnapshot.agentSessions.find(
        (session) => session.projectId === request.projectId && session.agentSessionId === request.agentSessionId,
      )
      if (!existing) {
        throw new Error(`Missing agent session ${request.agentSessionId}`)
      }

      const restoredSession = {
        ...existing,
        status: 'active' as const,
        archivedAt: null,
        updatedAt: '2026-04-23T12:10:00Z',
      }
      updateAgentSessions(
        currentSnapshot.agentSessions.map((session) =>
          session.projectId === request.projectId && session.agentSessionId === request.agentSessionId
            ? restoredSession
            : session,
        ),
      )
      return restoredSession
    },
    deleteAgentSession: async (request) => {
      updateAgentSessions(
        currentSnapshot.agentSessions.filter(
          (session) =>
            !(session.projectId === request.projectId && session.agentSessionId === request.agentSessionId),
        ),
      )
    },
    getAutonomousRun: async () => currentAutonomousState ?? { run: null },
    getRuntimeRun: async () => currentRuntimeRun,
    listMcpServers,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    listSkillRegistry: async () => currentSkillRegistry,
    reloadSkillRegistry: async () => {
      currentSkillRegistry = {
        ...currentSkillRegistry,
        reloadedAt: '2026-04-24T05:04:00Z',
      }
      return currentSkillRegistry
    },
    setSkillEnabled: async () => currentSkillRegistry,
    removeSkill: async () => currentSkillRegistry,
    upsertSkillLocalRoot: async () => currentSkillRegistry,
    removeSkillLocalRoot: async () => currentSkillRegistry,
    updateProjectSkillSource: async () => currentSkillRegistry,
    updateGithubSkillSource: async () => currentSkillRegistry,
    upsertPluginRoot: async () => currentSkillRegistry,
    removePluginRoot: async () => currentSkillRegistry,
    setPluginEnabled: async () => currentSkillRegistry,
    removePlugin: async () => currentSkillRegistry,
    getProviderModelCatalog: async (profileId, options) => {
      const currentProfile = currentProviderProfiles.profiles.find((profile) => profile.profileId === profileId)
      if (!currentProfile) {
        throw new Error(`Missing provider profile ${profileId}`)
      }

      const currentCatalog = currentProviderModelCatalogs[profileId]
      if (!options?.forceRefresh && currentCatalog) {
        return currentCatalog
      }

      const nextCatalog = buildProviderModelCatalog(currentProfile)
      currentProviderModelCatalogs[profileId] = nextCatalog
      return nextCatalog
    },
    checkProviderProfile: async (profileId) => {
      const currentProfile = currentProviderProfiles.profiles.find((profile) => profile.profileId === profileId)
      if (!currentProfile) {
        throw new Error(`Missing provider profile ${profileId}`)
      }

      const modelCatalog = currentProviderModelCatalogs[profileId] ?? buildProviderModelCatalog(currentProfile)
      currentProviderModelCatalogs[profileId] = modelCatalog
      return {
        checkedAt: '2026-04-26T12:00:00Z',
        profileId,
        providerId: currentProfile.providerId,
        validationChecks: [],
        reachabilityChecks: [],
        modelCatalog,
      }
    },
    runDoctorReport: async (request) =>
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
    getRuntimeSession: async () => currentRuntimeSession,
    startOpenAiLogin: async (_options) => {
      return makeProviderAuthSession({
        phase: 'awaiting_browser_callback',
        flowId: 'flow-1',
      })
    },
    submitOpenAiCallback: async (_flowId, _options) => {
      return makeProviderAuthSession()
    },
    startAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    listProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin: async () => {
      return makeProviderAuthSession()
    },
    completeOAuthCallback: async () => {
      return makeProviderAuthSession()
    },
    startRuntimeSession: async () => {
      currentRuntimeSession = makeRuntimeSession('project-1')
      return currentRuntimeSession
    },
    stopRuntimeRun: async (_projectId, _agentSessionId, runId) => {
      currentRuntimeRun = makeRuntimeRun('project-1', {
        runId,
        status: 'stopped',
        stoppedAt: '2026-04-15T20:10:00Z',
      })
      return currentRuntimeRun
    },
    stageAgentAttachment: async () => ({
      kind: 'image' as const,
      absolutePath: '/tmp/stage.png',
      mediaType: 'image/png',
      originalName: 'stage.png',
      sizeBytes: 0,
    }),
    discardAgentAttachment: async () => undefined,
    cancelAutonomousRun: async (_projectId, _agentSessionId, runId) => {
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
    getEnvironmentDiscoveryStatus,
    resolveEnvironmentPermissionRequests,
    startEnvironmentDiscovery,
    browserEval: async () => undefined,
    browserCurrentUrl: async () => null,
    browserScreenshot: async () => '',
    browserNavigate: async () => undefined,
    browserBack: async () => undefined,
    browserForward: async () => undefined,
    browserReload: async () => undefined,
    browserStop: async () => undefined,
    browserClick: async () => undefined,
    browserType: async () => undefined,
    browserScroll: async () => undefined,
    browserPressKey: async () => undefined,
    browserReadText: async () => undefined,
    browserQuery: async () => undefined,
    browserWaitForSelector: async () => undefined,
    browserWaitForLoad: async () => undefined,
    browserHistoryState: async () => undefined,
    browserCookiesGet: async () => undefined,
    browserCookiesSet: async () => undefined,
    browserStorageRead: async () => undefined,
    browserStorageWrite: async () => undefined,
    browserStorageClear: async () => undefined,
    browserTabList: async () => [],
    browserTabFocus: async () => ({
      id: 'tab-1',
      label: 'xero-browser',
      title: null,
      url: null,
      loading: false,
      canGoBack: false,
      canGoForward: false,
      active: true,
    }),
    browserTabClose: async () => [],
    onBrowserUrlChanged: async () => () => undefined,
    onBrowserLoadState: async () => () => undefined,
    onBrowserConsole: async () => () => undefined,
    onBrowserTabUpdated: async () => () => undefined,
    subscribeRuntimeStream: async (
      projectId: string,
      agentSessionId: string,
      itemKinds: RuntimeStreamEventDto['subscribedItemKinds'],
      handler: (payload: RuntimeStreamEventDto) => void,
      onError?: (error: XeroDesktopError) => void,
    ) => {
      const subscription = {
        projectId,
        handler,
        onError: onError ?? null,
        unsubscribe: () => {
          const index = streamSubscriptions.indexOf(subscription)
          if (index >= 0) {
            streamSubscriptions.splice(index, 1)
          }
        },
      }
      streamSubscriptions.push(subscription)

      return {
        response: {
          projectId,
          agentSessionId,
          runtimeKind: 'openai_codex',
          runId: currentRuntimeRun?.runId ?? 'run-1',
          sessionId: currentRuntimeSession.sessionId ?? 'session-1',
          flowId: currentRuntimeSession.flowId ?? null,
          subscribedItemKinds: itemKinds,
        } satisfies SubscribeRuntimeStreamResponseDto,
        unsubscribe: subscription.unsubscribe,
      }
    },
    onProjectUpdated,
    onRepositoryStatusChanged: async (_handler: (payload: RepositoryStatusChangedPayloadDto) => void) => () => {},
    onRuntimeUpdated,
    onRuntimeRunUpdated,
    onAgentUsageUpdated: async () => () => {},
  }

  return {
    adapter,
    streamSubscriptions,
    upsertNotificationRoute,
    listProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    listMcpServers,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    importRepository,
    pickRepositoryFolder,
    startRuntimeRun,
    updateRuntimeRunControls,
    startAutonomousRun,
    getEnvironmentDiscoveryStatus,
    resolveEnvironmentPermissionRequests,
    startEnvironmentDiscovery,
    onProjectUpdated,
    onRuntimeUpdated,
    onRuntimeRunUpdated,
    setSnapshot(snapshot: ProjectSnapshotResponseDto) {
      currentSnapshot = snapshot
    },
    setAutonomousState(state: AutonomousRunStateDto | null) {
      currentAutonomousState = state
    },
    emitProjectUpdated(payload: ProjectUpdatedPayloadDto) {
      applyProjectUpdatedPayload(payload)
      projectUpdatedHandler?.(payload)
    },
    emitProjectUpdatedError(error: XeroDesktopError) {
      projectUpdatedErrorHandler?.(error)
    },
    emitRuntimeUpdated(payload: RuntimeUpdatedPayloadDto) {
      applyRuntimeUpdatedPayload(payload)
      runtimeUpdatedHandler?.(payload)
    },
    emitRuntimeUpdatedError(error: XeroDesktopError) {
      runtimeUpdatedErrorHandler?.(error)
    },
    emitRuntimeRunUpdated(payload: RuntimeRunUpdatedPayloadDto) {
      applyRuntimeRunUpdatedPayload(payload)
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
  }
}

function getProviderCard(label: string): HTMLElement {
  const card = screen
    .getAllByText(label)
    .map((node) => node.closest('.rounded-lg'))
    .find((value): value is HTMLElement => value instanceof HTMLElement)

  if (!card) {
    throw new Error(`Could not find provider card for ${label}`)
  }

  return card
}

function ActivatedSurfaceProbe({
  active,
  prewarm,
}: {
  active: boolean
  prewarm?: boolean
}) {
  const mounted = useActivatedSurface(active, prewarm)
  return <div data-mounted={mounted ? 'true' : 'false'}>surface</div>
}

describe('useActivatedSurface', () => {
  it('keeps a prewarmed surface mounted after the warmup window ends', () => {
    const { rerender } = render(<ActivatedSurfaceProbe active={false} prewarm />)

    expect(screen.getByText('surface')).toHaveAttribute('data-mounted', 'true')

    rerender(<ActivatedSurfaceProbe active={false} prewarm={false} />)

    expect(screen.getByText('surface')).toHaveAttribute('data-mounted', 'true')
  })

  it('keeps a user-opened surface mounted after it closes', () => {
    const { rerender } = render(<ActivatedSurfaceProbe active />)

    expect(screen.getByText('surface')).toHaveAttribute('data-mounted', 'true')

    rerender(<ActivatedSurfaceProbe active={false} />)

    expect(screen.getByText('surface')).toHaveAttribute('data-mounted', 'true')
  })
})

describe('XeroApp current UI', () => {
  it('shows the onboarding flow on a cold-start empty state', async () => {
    const { adapter, getEnvironmentDiscoveryStatus, startEnvironmentDiscovery } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<XeroApp adapter={adapter} />)

    expect(await screen.findByRole('heading', { name: /Welcome to Xero/i })).toBeVisible()
    expect(screen.getByText('OpenAI, Anthropic, Ollama, Bedrock, Vertex, and more')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Get started' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Skip setup' })).toBeVisible()
    expect(screen.queryByLabelText('Status bar')).not.toBeInTheDocument()
    await waitFor(() => expect(getEnvironmentDiscoveryStatus).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(startEnvironmentDiscovery).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole('heading', { name: /Review environment access/i })).not.toBeInTheDocument()
  })

  it('falls through to the legacy empty state when onboarding is dismissed', async () => {
    const { adapter, startEnvironmentDiscovery } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<XeroApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Skip setup' }))

    expect(await screen.findByRole('heading', { name: 'Add your first project' })).toBeVisible()
    expect(screen.getAllByRole('button', { name: /Import repository/ }).length).toBeGreaterThanOrEqual(1)
    await waitFor(() => expect(startEnvironmentDiscovery).toHaveBeenCalledTimes(1))
  })

  it('persists environment access decisions before confirmation', async () => {
    const { adapter, resolveEnvironmentPermissionRequests, startEnvironmentDiscovery } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
      environmentDiscoveryStatus: makeEnvironmentDiscoveryStatus({
        hasProfile: true,
        status: 'partial',
        stale: false,
        shouldStart: false,
        refreshedAt: '2026-04-30T18:00:00Z',
        permissionRequests: [
          {
            id: 'developer-folder-access',
            kind: 'protected_path',
            status: 'pending',
            title: 'Developer folder access',
            reason: 'Allow Xero to inspect a protected toolchain directory during onboarding.',
            optional: true,
          },
        ],
      }),
    })

    render(<XeroApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByRole('heading', { name: 'Add a project' })).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByRole('heading', { name: 'Add notification routes' })).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))

    expect(await screen.findByRole('heading', { name: 'Review environment access' })).toBeVisible()
    expect(screen.getByText('Developer folder access')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Skip optional access' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Allow Developer folder access' })).toBeVisible()
    expect(startEnvironmentDiscovery).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    await waitFor(() =>
      expect(resolveEnvironmentPermissionRequests).toHaveBeenCalledWith({
        decisions: [{ id: 'developer-folder-access', status: 'skipped' }],
      }),
    )
    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
  })

  it('requires mandatory environment access approval before continuing', async () => {
    const { adapter, resolveEnvironmentPermissionRequests } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
      environmentDiscoveryStatus: makeEnvironmentDiscoveryStatus({
        hasProfile: true,
        status: 'partial',
        stale: false,
        shouldStart: false,
        refreshedAt: '2026-04-30T18:00:00Z',
        permissionRequests: [
          {
            id: 'required-toolchain-access',
            kind: 'protected_path',
            status: 'pending',
            title: 'Required toolchain access',
            reason: 'Allow Xero to inspect the selected toolchain directory.',
            optional: false,
          },
        ],
      }),
    })

    render(<XeroApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Continue' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Continue' }))

    expect(await screen.findByRole('heading', { name: 'Review environment access' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Continue' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Skip' })).toBeDisabled()

    fireEvent.click(screen.getByRole('button', { name: 'Allow Required toolchain access' }))
    expect(screen.getByRole('button', { name: 'Continue' })).toBeEnabled()
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))

    await waitFor(() =>
      expect(resolveEnvironmentPermissionRequests).toHaveBeenCalledWith({
        decisions: [{ id: 'required-toolchain-access', status: 'granted' }],
      }),
    )
    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
  })

  it('goes straight from notifications to confirmation when no environment permission is needed', async () => {
    const { adapter, getEnvironmentDiscoveryStatus, startEnvironmentDiscovery } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
      environmentDiscoveryStatus: makeEnvironmentDiscoveryStatus({
        hasProfile: true,
        status: 'ready',
        stale: false,
        shouldStart: false,
        refreshedAt: '2026-04-30T18:00:00Z',
      }),
    })

    render(<XeroApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Continue' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Continue' }))

    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
    expect(screen.queryByRole('heading', { name: 'Review environment access' })).not.toBeInTheDocument()
    expect(getEnvironmentDiscoveryStatus).toHaveBeenCalledTimes(1)
    expect(startEnvironmentDiscovery).not.toHaveBeenCalled()
  })

  it('refreshes stale environment discovery on startup even with existing projects', async () => {
    const { adapter, getEnvironmentDiscoveryStatus, startEnvironmentDiscovery } = createAdapter({
      projects: [makeProjectSummary('project-1', 'Xero')],
      environmentDiscoveryStatus: makeEnvironmentDiscoveryStatus({
        hasProfile: true,
        status: 'ready',
        stale: true,
        shouldStart: true,
        refreshedAt: '2026-04-20T18:00:00Z',
      }),
    })

    render(<XeroApp adapter={adapter} />)

    await waitFor(() => expect(getEnvironmentDiscoveryStatus).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(startEnvironmentDiscovery).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole('heading', { name: /Welcome to Xero/i })).not.toBeInTheDocument()
  })

  it('keeps the app shell hidden behind one boot loader while project state is loading', async () => {
    let resolveListProjects!: (value: ListProjectsResponseDto) => void
    const listProjectsPromise = new Promise<ListProjectsResponseDto>((resolve) => {
      resolveListProjects = resolve
    })
    const { adapter } = createAdapter({
      projects: [makeProjectSummary('project-1', 'Xero')],
    })
    adapter.listProjects = vi.fn(async () => listProjectsPromise)

    render(<XeroApp adapter={adapter} />)

    expect(screen.getByRole('status', { name: 'Loading' })).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Workflow' })).not.toBeInTheDocument()

    await act(async () => {
      resolveListProjects({ projects: [makeProjectSummary('project-1', 'Xero')] })
      await listProjectsPromise
    })

    expect(await screen.findByRole('button', { name: 'Workflow' })).toBeVisible()
  })









  it('saves an OpenRouter API-key credential from onboarding', async () => {
    const { adapter, upsertProviderCredential } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<XeroApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    const card = getProviderCard('OpenRouter')
    fireEvent.click(within(card).getByRole('button', { name: /configure/i }))
    fireEvent.change(within(card).getByLabelText(/API key/i), {
      target: { value: 'sk-or-v1-test-secret' },
    })
    fireEvent.click(within(card).getByRole('button', { name: /save/i }))

    await waitFor(() => expect(upsertProviderCredential).toHaveBeenCalledTimes(1))
    expect(upsertProviderCredential).toHaveBeenCalledWith(
      expect.objectContaining({
        providerId: 'openrouter',
        kind: 'api_key',
        apiKey: 'sk-or-v1-test-secret',
      }),
    )
  })

  it('saves an Anthropic API-key credential from onboarding', async () => {
    const { adapter, upsertProviderCredential } = createAdapter({
      projects: [],
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<XeroApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    const card = getProviderCard('Anthropic')
    fireEvent.click(within(card).getByRole('button', { name: /configure/i }))
    fireEvent.change(within(card).getByLabelText(/API key/i), {
      target: { value: 'sk-ant-test-secret' },
    })
    fireEvent.click(within(card).getByRole('button', { name: /save/i }))

    await waitFor(() => expect(upsertProviderCredential).toHaveBeenCalledTimes(1))
    expect(upsertProviderCredential).toHaveBeenCalledWith(
      expect.objectContaining({
        providerId: 'anthropic',
        kind: 'api_key',
        apiKey: 'sk-ant-test-secret',
      }),
    )
  })


  it('imports a project and creates a notification route from onboarding', async () => {
    const { adapter, pickRepositoryFolder, importRepository, upsertNotificationRoute } = createAdapter({
      projects: [],
      pickedRepositoryPath: '/tmp/Xero',
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<XeroApp adapter={adapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    expect(await screen.findByRole('heading', { name: 'Add a project' })).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: /Choose a folder/i }))

    await waitFor(() => expect(pickRepositoryFolder).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(importRepository).toHaveBeenCalledTimes(1))
    await waitFor(() => expect(screen.getByText('/tmp/Xero')).toBeVisible())

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

  it('renders the workflow tab as a blank slate for an imported project', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.queryByText('No milestone assigned')).not.toBeInTheDocument()
    expect(screen.queryByText('Xero Desktop')).not.toBeInTheDocument()
  })

  it('lazy-activates the agent pane only after the Agent view is opened', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.queryByLabelText('Agent conversation viewport')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(await screen.findByLabelText('Agent conversation viewport')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Workflow' }))

    await waitFor(() =>
      expect(screen.getByLabelText('Agent conversation viewport')).not.toBeVisible(),
    )
  })

  it('renders the existing single-pane agent runtime through the workspace shell', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(await screen.findByLabelText('Agent conversation viewport')).toBeVisible()
    expect(screen.getByText('New Session')).toBeVisible()
    expect(screen.getAllByText('Main session').length).toBeGreaterThan(0)
  })

  it('confirms before closing a pane with a running agent run', async () => {
    const { adapter } = createAdapter({
      runtimeRun: makeRuntimeRun('project-1'),
    })

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByLabelText('Agent conversation viewport')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Spawn agent pane' }))
    await waitFor(() => expect(screen.getAllByRole('button', { name: 'Close pane' })).toHaveLength(2))

    const firstPane = screen.getByRole('region', { name: 'Agent pane 1 - Session "Main session"' })
    fireEvent.click(within(firstPane).getByRole('button', { name: 'Close pane' }))

    expect(await screen.findByRole('alertdialog')).toBeVisible()
    expect(screen.getByRole('heading', { name: 'Close agent pane?' })).toBeVisible()
    expect(screen.getByText(/The agent is still running/)).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    await waitFor(() => expect(screen.queryByRole('alertdialog')).not.toBeInTheDocument())
    expect(screen.getAllByRole('button', { name: 'Close pane' })).toHaveLength(2)
  })

  it('hides the collapsed sessions strip outside the Agent view', async () => {
    window.localStorage.setItem('xero.explorer.collapsed', 'collapsed')
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.queryByRole('button', { name: 'Show sessions sidebar' })).not.toBeInTheDocument()

    fireEvent.pointerEnter(screen.getByRole('complementary'))
    expect(screen.queryByRole('button', { name: 'Show sessions sidebar' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('button', { name: 'Show sessions sidebar' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Workflow' }))
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Show sessions sidebar' })).not.toBeInTheDocument(),
    )
  })

  it('opens the sessions peek from the project rail in compact and expanded states', async () => {
    window.localStorage.setItem('xero.explorer.collapsed', 'collapsed')
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('button', { name: 'Show sessions sidebar' })).toBeVisible()

    const getProjectRail = () => document.querySelector('aside[data-collapsed]') as HTMLElement

    await waitFor(() => expect(getProjectRail()).toHaveAttribute('data-collapsed', 'true'))
    fireEvent.pointerEnter(getProjectRail())
    expect(await screen.findByRole('button', { name: 'Main session' })).toBeVisible()
    fireEvent.pointerLeave(getProjectRail())
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Main session' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Expand project sidebar' }))
    await waitFor(() => expect(getProjectRail()).toHaveAttribute('data-collapsed', 'false'))
    fireEvent.pointerEnter(getProjectRail())
    expect(await screen.findByRole('button', { name: 'Main session' })).toBeVisible()
  })

  it('lazy-mounts the workflows sidebar and preserves its local search state while hidden', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(document.querySelector('aside[aria-label="Workflows"]')).toBeNull()

    fireEvent.click(screen.getAllByRole('button', { name: 'Open workflows' })[0])

    const workflowsPanel = await screen.findByLabelText('Workflows')
    fireEvent.click(within(workflowsPanel).getByRole('button', { name: 'Search workflows' }))
    fireEvent.change(within(workflowsPanel).getByRole('searchbox', { name: 'Search workflows' }), {
      target: { value: 'review' },
    })

    fireEvent.click(screen.getAllByRole('button', { name: 'Close workflows' })[0])

    await waitFor(() =>
      expect(screen.queryByRole('complementary', { name: 'Workflows' })).not.toBeInTheDocument(),
    )
    expect(document.querySelector('aside[aria-label="Workflows"]')).toHaveAttribute('aria-hidden', 'true')

    fireEvent.click(screen.getAllByRole('button', { name: 'Open workflows' })[0])

    expect(await screen.findByRole('searchbox', { name: 'Search workflows' })).toHaveValue('review')
  })

  it('renders live git footer data from desktop state while leaving mock-only fields untouched', async () => {
    const runtimeSettings = makeRuntimeSettings({
      providerId: 'openrouter',
      modelId: 'openai/gpt-4.1-mini',
      openrouterApiKeyConfigured: true,
    })
    const providerProfiles = makeProviderProfilesFromRuntimeSettings(runtimeSettings)
    const { adapter } = createAdapter({
      status: {
        repository: {
          id: 'repo-project-1',
          projectId: 'project-1',
          rootPath: '/tmp/Xero',
          displayName: 'Xero',
          branch: 'feature/footer-live-data',
          headSha: '1234567890abcdef1234567890abcdef12345678',
          isGitRepo: true,
        },
        branch: {
          name: 'feature/footer-live-data',
          headSha: '1234567890abcdef1234567890abcdef12345678',
          detached: false,
          upstream: {
            name: 'origin/feature/footer-live-data',
            ahead: 4,
            behind: 1,
          },
        },
        lastCommit: {
          sha: '1234567890abcdef1234567890abcdef12345678',
          summary: 'fix: use live head commit metadata',
          committedAt: '2026-04-22T17:55:00Z',
        },
        entries: [
          {
            path: 'src/App.tsx',
            staged: 'modified',
            unstaged: null,
            untracked: false,
          },
          {
            path: 'src-tauri/src/main.rs',
            staged: null,
            unstaged: 'modified',
            untracked: false,
          },
        ],
        hasStagedChanges: true,
        hasUnstagedChanges: true,
        hasUntrackedChanges: false,
      },
      runtimeSettings,
      providerProfiles,
      runtimeSession: makeRuntimeSession('project-1', {
        runtimeKind: 'openrouter',
        providerId: 'openrouter',
      }),
      runtimeRun: makeRuntimeRun('project-1', {
        runtimeKind: 'openrouter',
        providerId: 'openrouter',
        status: 'running',
      }),
    })

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    const statusBar = screen.getByRole('contentinfo', { name: 'Status bar' })
    expect(statusBar).toBeVisible()
    expect(within(statusBar).getByText('feature/footer-live-data')).toBeVisible()
    expect(within(statusBar).getByText('↑4 ↓1')).toBeVisible()
    expect(within(statusBar).getByText('2 changes')).toBeVisible()
    expect(within(statusBar).getByText('1234567')).toBeVisible()
    expect(within(statusBar).getByText('fix: use live head commit metadata')).toBeVisible()
  })

  it('renders project-global footer spend by summing every model breakdown row', async () => {
    const { adapter } = createAdapter({
      usageSummary: {
        projectId: 'project-1',
        totals: {
          runCount: 2,
          inputTokens: 1,
          outputTokens: 1,
          totalTokens: 2,
          cacheReadTokens: 0,
          cacheCreationTokens: 0,
          estimatedCostMicros: 2,
        },
        byModel: [
          {
            providerId: 'anthropic',
            modelId: 'claude-sonnet-4-6',
            runCount: 1,
            inputTokens: 600_000,
            outputTokens: 300_000,
            totalTokens: 900_000,
            cacheReadTokens: 0,
            cacheCreationTokens: 0,
            estimatedCostMicros: 1_250_000,
          },
          {
            providerId: 'openai_codex',
            modelId: 'gpt-5.1',
            runCount: 1,
            inputTokens: 100_000,
            outputTokens: 50_500,
            totalTokens: 150_500,
            cacheReadTokens: 0,
            cacheCreationTokens: 0,
            estimatedCostMicros: 250_000,
          },
        ],
      },
    })

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    const statusBar = screen.getByRole('contentinfo', { name: 'Status bar' })
    await waitFor(() => {
      expect(within(statusBar).getByText('1.05M tok')).toBeVisible()
      expect(within(statusBar).getByText('$1.50')).toBeVisible()
    })
  })

  it('collapses the project rail into a compact icon strip from the titlebar toggle', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    const collapseButton = screen.getByRole('button', { name: 'Collapse project sidebar' })
    fireEvent.click(collapseButton)

    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()
    expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull()
    expect(screen.queryByRole('button', { name: 'Project actions for xero' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: /xero/i })).toBeVisible()
  })

  it('opens the Solana workbench from the titlebar in the normal app shell', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Tools' }), { button: 0, ctrlKey: false })
    fireEvent.click(screen.getByRole('menuitem', { name: 'Open Solana workbench' }))

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Tools' })).toHaveAttribute('aria-pressed', 'true'),
    )
    const breadcrumb = await screen.findByRole('navigation', {
      name: 'Solana Workbench breadcrumb',
    })

    expect(within(breadcrumb).getByText('Solana Workbench')).toBeVisible()
    expect(within(breadcrumb).getByText('Cluster')).toBeVisible()
  })

  it('starts GitHub auth from the titlebar without opening Account settings', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Sign in with GitHub' }))

    await waitFor(() => expect(githubLoginMock).toHaveBeenCalledTimes(1))
    expect(screen.queryByRole('heading', { name: 'Account' })).not.toBeInTheDocument()
    expect(screen.queryByText('Connect your GitHub account to identify this install.')).not.toBeInTheDocument()
  })

  it('auto-collapses the project rail in Editor and restores it when leaving if it started expanded', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

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

    render(<XeroApp adapter={adapter} />)

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


  it('rehydrates the recovered runtime snapshot after reload without rendering the removed debug panels', async () => {
    const recoveredAutonomousState = makeAutonomousRunState('project-1', 'auto-run-1')
    recoveredAutonomousState.run = {
      ...recoveredAutonomousState.run!,
      recoveryState: 'recovery_required',
      duplicateStartDetected: true,
      duplicateStartRunId: 'auto-run-1',
      duplicateStartReason:
        'Xero reused the already-active autonomous run for this project instead of launching a duplicate supervisor.',
      crashedAt: '2026-04-16T20:03:00Z',
      crashReason: {
        code: 'runtime_supervisor_connect_failed',
        message: 'Xero restored the same autonomous run after reload without launching a duplicate continuation.',
      },
      lastErrorCode: 'runtime_supervisor_connect_failed',
      lastError: {
        code: 'runtime_supervisor_connect_failed',
        message: 'Xero restored the same autonomous run after reload without launching a duplicate continuation.',
      },
      updatedAt: '2026-04-16T20:03:00Z',
    }

    const { adapter } = createAdapter({
      snapshot: {
        ...makeSnapshot(),
        autonomousRun: recoveredAutonomousState.run,
      },
      runtimeRun: makeRuntimeRun('project-1', {
        status: 'stale',
        transport: {
          kind: 'internal',
          endpoint: 'xero://owned-agent',
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
          message: 'Xero restored the same autonomous run after reload without launching a duplicate continuation.',
        },
      }),
      autonomousState: recoveredAutonomousState,
    })

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(screen.queryByRole('heading', { name: 'Recovered run snapshot' })).not.toBeInTheDocument()
    expect(
      screen.queryByText('Recovered the current autonomous unit boundary after reload without launching a duplicate continuation.'),
    ).not.toBeInTheDocument()
    expect(screen.queryByText('Duplicate start prevented')).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start autonomous run' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Inspect truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
  })

  it('refreshes active project metadata from project:updated events without rerendering the app root', async () => {
    const setup = createAdapter()

    render(<XeroApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )
    await act(async () => {})
    await waitFor(() => expect(setup.onProjectUpdated).toHaveBeenCalledTimes(1))
    expect(screen.getByRole('button', { name: 'Xero' })).toBeVisible()

    act(() => {
      setup.emitProjectUpdated({
        project: makeProjectSummary('project-1', 'Xero Prime'),
        reason: 'metadata_changed',
      })
    })

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Xero Prime' })).toBeVisible(),
    )
    expect(screen.queryByRole('button', { name: 'Xero' })).not.toBeInTheDocument()
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

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByLabelText('Settings'))
    expect(await screen.findByRole('heading', { name: 'Providers' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }))

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

    render(<XeroApp adapter={adapter} />)

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

  it('keeps open editor tabs and unsaved edits when switching away from Editor and back', async () => {
    const { adapter } = createAdapter()

    render(<XeroApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))
    fireEvent.click(await screen.findByRole('treeitem', { name: 'README.md' }))

    const editor = await screen.findByLabelText('Editor for /README.md')
    const executionPane = editor.closest('[aria-hidden]')
    expect(executionPane).toHaveAttribute('aria-hidden', 'false')
    fireEvent.change(editor, { target: { value: '# Draft changes\n' } })

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    await waitFor(() => expect(executionPane).toHaveAttribute('aria-hidden', 'true'))
    expect(screen.queryByText('No milestone assigned')).not.toBeInTheDocument()
    expect(screen.queryByText('Xero Desktop')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))

    const restoredEditor = await screen.findByLabelText('Editor for /README.md')
    expect(restoredEditor).toBeVisible()
    expect(restoredEditor).toHaveValue('# Draft changes\n')
    expect(screen.getByRole('button', { name: 'Close README.md' })).toBeVisible()
  })
})
