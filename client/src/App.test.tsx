import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'

const { githubLoginMock, githubLogoutMock, githubRefreshMock, openUrlMock } = vi.hoisted(() => ({
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

vi.mock('../components/cadence/code-editor', () => ({
  CodeEditor: ({ filePath, onChange, onSave, value }: any) => (
    <div>
      <label>
        <span className="sr-only">Editor for {filePath}</span>
        <textarea
          aria-label={`Editor for ${filePath}`}
          onChange={(event) => onChange(event.target.value)}
          value={value}
        />
      </label>
      <button onClick={onSave} type="button">
        Trigger save
      </button>
    </div>
  ),
}))

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

import { CadenceApp } from './App'
import { CadenceDesktopError, type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import {
  createCadenceDoctorReport,
  projectRuntimeSettingsFromProviderProfiles,
  providerModelCatalogSchema,
  providerProfileSchema,
  providerProfilesSchema,
  upsertProviderProfileRequestSchema,
} from '@/src/lib/cadence-model'
import type {
  AutonomousRunStateDto,
  ImportMcpServersResponseDto,
  ImportRepositoryResponseDto,
  ListNotificationDispatchesResponseDto,
  ListNotificationRoutesResponseDto,
  ListProjectFilesResponseDto,
  ListProjectsResponseDto,
  McpImportDiagnosticDto,
  McpRegistryDto,
  OperatorApprovalDto,
  ProjectSnapshotResponseDto,
  ProjectUpdatedPayloadDto,
  ProviderModelCatalogDto,
  ProviderProfileDto,
  ProviderProfileReadinessDto,
  ProviderProfilesDto,
  RepositoryDiffResponseDto,
  RepositoryStatusChangedPayloadDto,
  RepositoryStatusResponseDto,
  ResumeHistoryEntryDto,
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
  UpsertProviderProfileRequestDto,
  UpsertRuntimeSettingsRequestDto,
} from '@/src/lib/cadence-model'
import {
  getCloudProviderPreset,
  type CloudProviderPreset,
} from '@/src/lib/cadence-model/provider-presets'

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
    approvalRequests: [],
    verificationRecords: [],
    resumeHistory: [],
    agentSessions: [makeAgentSession(projectId)],
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
            message: 'Cadence has not checked this MCP server yet.',
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

function makeProviderProfile(
  overrides: Partial<ProviderProfileDto> &
    Pick<ProviderProfileDto, 'profileId' | 'providerId' | 'label' | 'modelId'>,
): ProviderProfileDto {
  const preset = getRequiredCloudProviderPreset(overrides.providerId, 'makeProviderProfile')

  return providerProfileSchema.parse({
    profileId: overrides.profileId,
    providerId: overrides.providerId,
    runtimeKind: overrides.runtimeKind ?? preset.runtimeKind,
    label: overrides.label,
    modelId: overrides.modelId,
    presetId: overrides.presetId ?? preset.presetId ?? null,
    baseUrl: overrides.baseUrl ?? null,
    apiVersion: overrides.apiVersion ?? null,
    region: overrides.region ?? null,
    projectId: overrides.projectId ?? null,
    active: overrides.active ?? true,
    readiness: overrides.readiness ?? makeMissingProviderReadiness(),
    migratedFromLegacy: overrides.migratedFromLegacy ?? false,
    migratedAt: overrides.migratedAt ?? null,
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
          message: `Cadence cannot discover OpenRouter models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'anthropic':
        return {
          code: 'anthropic_api_key_missing',
          message: `Cadence cannot discover Anthropic models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'github_models':
        return {
          code: 'github_models_token_missing',
          message: `Cadence cannot discover GitHub Models for provider profile \`${profile.profileId}\` because no app-local GitHub token is configured for that profile.`,
          retryable: false,
        }
      case 'openai_api':
        return {
          code: 'openai_api_key_missing',
          message: `Cadence cannot discover OpenAI-compatible models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'ollama':
        return {
          code: 'local_provider_unreachable',
          message: `Cadence cannot discover Ollama models for provider profile \`${profile.profileId}\` because the local endpoint is not ready yet.`,
          retryable: false,
        }
      case 'azure_openai':
        return {
          code: 'azure_openai_api_key_missing',
          message: `Cadence cannot discover Azure OpenAI models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'gemini_ai_studio':
        return {
          code: 'gemini_ai_studio_api_key_missing',
          message: `Cadence cannot discover Gemini AI Studio models for provider profile \`${profile.profileId}\` because no app-local API key is configured for that profile.`,
          retryable: false,
        }
      case 'bedrock':
        return {
          code: 'bedrock_ambient_proof_missing',
          message: `Cadence cannot validate Amazon Bedrock model availability for provider profile \`${profile.profileId}\` because the profile is missing its ambient readiness proof link. Save the profile again so Cadence records ambient-auth intent.`,
          retryable: false,
        }
      case 'vertex':
        return {
          code: 'vertex_ambient_proof_missing',
          message: `Cadence cannot validate Google Vertex AI model availability for provider profile \`${profile.profileId}\` because the profile is missing its ambient readiness proof link. Save the profile again so Cadence records ambient-auth intent.`,
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
    supervisorKind: 'detached_pty',
    status: 'running',
    transport: {
      kind: 'tcp',
      endpoint: '127.0.0.1:4455',
      liveness: 'reachable',
    },
    controls: {
      active: {
        providerProfileId: 'openai_codex-default',
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

function makeRuntimeApproval(actionId: string, overrides: Partial<OperatorApprovalDto> = {}): OperatorApprovalDto {
  return {
    actionId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    actionType: 'review_command',
    title: 'Review destructive shell command',
    detail: 'Cadence blocked a destructive shell wrapper and needs approval before continuing.',
    userAnswer: null,
    status: 'pending',
    decisionNote: null,
    createdAt: '2026-04-22T12:07:00Z',
    updatedAt: '2026-04-22T12:07:00Z',
    resolvedAt: null,
    ...overrides,
  }
}

function makeResumeHistoryEntry(actionId: string, overrides: Partial<ResumeHistoryEntryDto> = {}): ResumeHistoryEntryDto {
  return {
    id: 1,
    sourceActionId: actionId,
    sessionId: 'session-1',
    status: 'failed',
    summary: 'Operator resume failed and is waiting for corrected shell input.',
    createdAt: '2026-04-22T12:07:05Z',
    ...overrides,
  }
}

function makeRuntimeStreamActionRequiredEvent(options: {
  actionId: string
  boundaryId: string
  detail: string
  runId?: string
  sequence?: number
  title?: string
}): RuntimeStreamEventDto {
  const runId = options.runId ?? 'run-1'

  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      kind: 'action_required',
      runId,
      sequence: options.sequence ?? 5,
      sessionId: 'session-1',
      flowId: 'flow-1',
      text: null,
      toolCallId: null,
      toolName: null,
      toolState: null,
      actionId: options.actionId,
      boundaryId: options.boundaryId,
      actionType: 'review_command',
      title: options.title ?? 'Review destructive shell command',
      detail: options.detail,
      code: null,
      message: null,
      retryable: null,
      createdAt: '2026-04-22T12:07:10Z',
    },
  }
}

function makeRuntimeStreamToolEvent(options: {
  runId?: string
  sequence?: number
  detail: string
}): RuntimeStreamEventDto {
  const runId = options.runId ?? 'run-1'

  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      kind: 'tool',
      runId,
      sequence: options.sequence ?? 4,
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
      detail: options.detail,
      code: null,
      message: null,
      retryable: null,
      createdAt: '2026-04-22T12:07:08Z',
    },
  }
}

function makeRuntimeStreamActivityEvent(options: {
  runId?: string
  sequence?: number
  code: string
  title: string
  detail: string
}): RuntimeStreamEventDto {
  const runId = options.runId ?? 'run-1'

  return {
    projectId: 'project-1',
    agentSessionId: 'agent-session-main',
    runtimeKind: 'openai_codex',
    runId,
    sessionId: 'session-1',
    flowId: 'flow-1',
    subscribedItemKinds: ['transcript', 'tool', 'skill', 'activity', 'action_required', 'complete', 'failure'],
    item: {
      kind: 'activity',
      runId,
      sequence: options.sequence ?? 5,
      sessionId: 'session-1',
      flowId: 'flow-1',
      text: null,
      toolCallId: null,
      toolName: null,
      toolState: null,
      actionId: null,
      boundaryId: null,
      actionType: null,
      title: options.title,
      detail: options.detail,
      code: options.code,
      message: null,
      retryable: null,
      skillId: null,
      skillStage: null,
      skillResult: null,
      skillSource: null,
      skillCacheStatus: null,
      skillDiagnostic: null,
      createdAt: '2026-04-22T12:07:09Z',
    },
  }
}

function makeAutonomousRunState(projectId = 'project-1', runId = 'auto-run-1'): AutonomousRunStateDto {
  return {
    run: {
      projectId,
      agentSessionId: 'agent-session-main',
      runId,
      runtimeKind: 'openai_codex',
      providerId: 'openai_codex',
      supervisorKind: 'detached_pty',
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
  projectFiles?: ListProjectFilesResponseDto
  pickedRepositoryPath?: string | null
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
  let currentRuntimeSettings =
    projectRuntimeSettingsFromProviderProfiles(currentProviderProfiles) ?? options?.runtimeSettings ?? makeRuntimeSettings()
  let currentProviderModelCatalogs: Record<string, ProviderModelCatalogDto> = Object.fromEntries(
    currentProviderProfiles.profiles.map((profile) => [profile.profileId, buildProviderModelCatalog(profile)]),
  )
  let currentMcpRegistry = options?.mcpRegistry ?? makeMcpRegistry()
  let currentSkillRegistry = options?.skillRegistry ?? makeSkillRegistry()
  let currentMcpImportDiagnostics: McpImportDiagnosticDto[] = []
  let currentRuntimeRun = options?.runtimeRun ?? null
  let currentAutonomousState = options?.autonomousState ?? null
  let currentNotificationRoutes = options?.notificationRoutes ?? []
  let currentProjects = options?.projects ?? [makeProjectSummary('project-1', 'Cadence')]
  let currentProjectFiles = options?.projectFiles ?? makeProjectFiles()
  const updateAgentSessions = (agentSessions: ProjectSnapshotResponseDto['agentSessions']) => {
    currentSnapshot = {
      ...currentSnapshot,
      agentSessions,
    }
  }
  const pickedRepositoryPath = options?.pickedRepositoryPath ?? null
  const currentFileContents: Record<string, string> = {
    '/README.md': '# Cadence\n',
    '/src/App.tsx': 'export default function App() {\n  return <main>Cadence</main>\n}\n',
  }
  let projectUpdatedHandler: ((payload: ProjectUpdatedPayloadDto) => void) | null = null
  let projectUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let runtimeUpdatedHandler: ((payload: RuntimeUpdatedPayloadDto) => void) | null = null
  let runtimeUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  let runtimeRunUpdatedHandler: ((payload: RuntimeRunUpdatedPayloadDto) => void) | null = null
  let runtimeRunUpdatedErrorHandler: ((error: CadenceDesktopError) => void) | null = null
  const streamSubscriptions: Array<{
    projectId: string
    handler: (payload: RuntimeStreamEventDto) => void
    onError: ((error: CadenceDesktopError) => void) | null
    unsubscribe: () => void
  }> = []

  const rebuildProviderModelCatalogs = () => {
    currentProviderModelCatalogs = Object.fromEntries(
      currentProviderProfiles.profiles.map((profile) => [profile.profileId, buildProviderModelCatalog(profile)]),
    )
  }

  const syncRuntimeSettingsFromActiveProfile = () => {
    currentRuntimeSettings =
      projectRuntimeSettingsFromProviderProfiles(currentProviderProfiles) ?? currentRuntimeSettings
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
    const thinkingEffort = options.nextControls?.thinkingEffort ?? options.base.thinkingEffort
    const approvalMode = options.nextControls?.approvalMode ?? options.base.approvalMode
    const planModeRequired = options.nextControls?.planModeRequired ?? options.base.planModeRequired

    return {
      active: {
        providerProfileId,
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
  }

  const applyRuntimeRunUpdatedPayload = (payload: RuntimeRunUpdatedPayloadDto) => {
    ensureKnownProjectId(payload.projectId, getKnownProjectIds(), 'emitRuntimeRunUpdated')
    ensureCompatibleRuntimeRun(payload.projectId, currentRuntimeRun, payload.run, 'emitRuntimeRunUpdated')
    currentRuntimeRun = payload.run ? cloneRuntimeRun(payload.run) : null
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
      anthropicApiKeyConfigured:
        request.providerId === 'anthropic'
          ? request.anthropicApiKey == null
            ? currentRuntimeSettings.anthropicApiKeyConfigured
            : request.anthropicApiKey.trim().length > 0
          : false,
    }
    currentProviderProfiles = makeProviderProfilesFromRuntimeSettings(currentRuntimeSettings)
    rebuildProviderModelCatalogs()
    return currentRuntimeSettings
  })

  const upsertProviderProfile = vi.fn(async (request: UpsertProviderProfileRequestDto) => {
    const parsedRequest = upsertProviderProfileRequestSchema.parse(request)
    const preset = getRequiredCloudProviderPreset(parsedRequest.providerId, 'createAdapter.upsertProviderProfile')
    const existingProfile =
      currentProviderProfiles.profiles.find((profile) => profile.profileId === parsedRequest.profileId) ?? null

    const nextReadiness = (() => {
      if (preset.authMode === 'oauth') {
        return existingProfile?.readiness ?? makeMissingProviderReadiness()
      }

      if (preset.authMode === 'api_key') {
        if (parsedRequest.apiKey === '') {
          return makeMissingProviderReadiness()
        }

        if (typeof parsedRequest.apiKey === 'string' && parsedRequest.apiKey.trim().length > 0) {
          return makeReadyProviderReadiness(preset)
        }

        return existingProfile?.readiness ?? makeMissingProviderReadiness()
      }

      if (preset.authMode === 'local') {
        return makeReadyProviderReadiness(preset)
      }

      const hasRequiredRegion = preset.regionMode !== 'required' || Boolean(parsedRequest.region?.trim())
      const hasRequiredProjectId =
        preset.projectIdMode !== 'required' || Boolean(parsedRequest.projectId?.trim())

      return hasRequiredRegion && hasRequiredProjectId
        ? makeReadyProviderReadiness(preset)
        : makeMissingProviderReadiness('malformed')
    })()

    const shouldActivate = parsedRequest.activate ?? currentProviderProfiles.profiles.length === 0
    const nextProfile = providerProfileSchema.parse({
      profileId: parsedRequest.profileId,
      providerId: parsedRequest.providerId,
      runtimeKind: parsedRequest.runtimeKind,
      label: parsedRequest.label,
      modelId: parsedRequest.modelId,
      presetId: parsedRequest.presetId ?? null,
      baseUrl: parsedRequest.baseUrl ?? null,
      apiVersion: parsedRequest.apiVersion ?? null,
      region: parsedRequest.region ?? null,
      projectId: parsedRequest.projectId ?? null,
      active: shouldActivate ? true : parsedRequest.profileId === currentProviderProfiles.activeProfileId,
      readiness: nextReadiness,
      migratedFromLegacy: existingProfile?.migratedFromLegacy ?? false,
      migratedAt: existingProfile?.migratedAt ?? null,
    })

    const nextProfiles = currentProviderProfiles.profiles
      .filter((profile) => profile.profileId !== parsedRequest.profileId)
      .map((profile) => ({
        ...profile,
        active: shouldActivate ? false : profile.active,
      }))

    currentProviderProfiles = providerProfilesSchema.parse({
      activeProfileId: shouldActivate ? parsedRequest.profileId : currentProviderProfiles.activeProfileId,
      profiles: [...nextProfiles, nextProfile],
      migration: currentProviderProfiles.migration ?? null,
    })
    syncRuntimeSettingsFromActiveProfile()
    rebuildProviderModelCatalogs()
    return currentProviderProfiles
  })

  const setActiveProviderProfile = vi.fn(async (profileId: string) => {
    const nextProfiles = currentProviderProfiles.profiles.map((profile) => ({
      ...profile,
      active: profile.profileId === profileId,
    }))
    currentProviderProfiles = {
      ...currentProviderProfiles,
      activeProfileId: profileId,
      profiles: nextProfiles,
    }
    syncRuntimeSettingsFromActiveProfile()
    rebuildProviderModelCatalogs()
    return currentProviderProfiles
  })
  const logoutProviderProfile = vi.fn(async (profileId: string) => {
    currentProviderProfiles = {
      ...currentProviderProfiles,
      profiles: currentProviderProfiles.profiles.map((profile) =>
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
    rebuildProviderModelCatalogs()
    return currentProviderProfiles
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
              message: 'Cadence has not checked this MCP server yet.',
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
          message: 'Cadence kept the last truthful MCP projection because the imported status payload was malformed.',
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
        message: 'Cadence preserved the last truthful MCP projection while parsing imported rows.',
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

  const pickRepositoryFolder = vi.fn(async () => pickedRepositoryPath)
  const importRepository = vi.fn(async (_path: string): Promise<ImportRepositoryResponseDto> => {
    const project = makeProjectSummary('project-1', 'Cadence')
    currentProjects = [project]
    return {
      project,
      repository: makeStatus().repository,
    }
  })
  const onProjectUpdated = vi.fn(
    async (
      handler: (payload: ProjectUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      projectUpdatedHandler = handler
      projectUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )
  const onRuntimeUpdated = vi.fn(
    async (
      handler: (payload: RuntimeUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      runtimeUpdatedHandler = handler
      runtimeUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )
  const onRuntimeRunUpdated = vi.fn(
    async (
      handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
      onError?: (error: CadenceDesktopError) => void,
    ) => {
      runtimeRunUpdatedHandler = handler
      runtimeRunUpdatedErrorHandler = onError ?? null
      return () => undefined
    },
  )

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
    getProjectUsageSummary: async (projectId: string) => ({
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
    }),
    getRepositoryStatus: async () => currentStatus,
    getRepositoryDiff: async (_projectId, scope) => ({ ...currentDiff, scope }),
    gitStagePaths: async () => undefined,
    gitUnstagePaths: async () => undefined,
    gitDiscardChanges: async () => undefined,
    gitCommit: async () => ({ sha: 'abc1234', summary: 'mock commit', signature: { name: 'Mock', email: 'mock@example.com' } }),
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
    getRuntimeSettings: async () => currentRuntimeSettings,
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
      createCadenceDoctorReport({
        reportId: 'doctor-test',
        generatedAt: '2026-04-26T12:00:00Z',
        mode: request?.mode ?? 'quick_local',
        versions: {
          appVersion: 'test',
          runtimeSupervisorVersion: 'test',
          runtimeProtocolVersion: 'supervisor-v1',
        },
      }),
    getProviderProfiles: async () => currentProviderProfiles,
    getRuntimeSession: async () => currentRuntimeSession,
    startOpenAiLogin: async (_projectId, _options) => {
      currentRuntimeSession = makeRuntimeSession('project-1', {
        phase: 'awaiting_browser_callback',
        flowId: 'flow-1',
      })
      return currentRuntimeSession
    },
    submitOpenAiCallback: async (_projectId, _flowId, _options) => {
      currentRuntimeSession = makeRuntimeSession('project-1')
      return currentRuntimeSession
    },
    startAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
    logoutProviderProfile,
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
      label: 'cadence-browser',
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
      onError?: (error: CadenceDesktopError) => void,
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
    upsertRuntimeSettings,
    upsertProviderProfile,
    setActiveProviderProfile,
    logoutProviderProfile,
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
    emitProjectUpdatedError(error: CadenceDesktopError) {
      projectUpdatedErrorHandler?.(error)
    },
    emitRuntimeUpdated(payload: RuntimeUpdatedPayloadDto) {
      applyRuntimeUpdatedPayload(payload)
      runtimeUpdatedHandler?.(payload)
    },
    emitRuntimeUpdatedError(error: CadenceDesktopError) {
      runtimeUpdatedErrorHandler?.(error)
    },
    emitRuntimeRunUpdated(payload: RuntimeRunUpdatedPayloadDto) {
      applyRuntimeRunUpdatedPayload(payload)
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
    expect(screen.getByText('OpenAI, Anthropic, Ollama, Bedrock, Vertex, and more')).toBeVisible()
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

  it('reflects real provider settings in onboarding and keeps shipped provider presets available', async () => {
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
    expect(screen.getByText('Provider setup is app-wide. Add credentials or sign in once, then choose models from the agent composer.')).toBeVisible()
    expect(within(getProviderCard('Anthropic')).getByRole('button', { name: 'API key' })).toBeVisible()
    expect(within(getProviderCard('GitHub Models')).queryByRole('button', { name: 'Select' })).not.toBeInTheDocument()
    expect(within(getProviderCard('GitHub Models')).getByRole('button', { name: 'API key' })).toBeVisible()
    expect(within(getProviderCard('Ollama')).getByRole('button', { name: 'Endpoint' })).toBeVisible()
    expect(within(getProviderCard('Amazon Bedrock')).getByRole('button', { name: 'Cloud config' })).toBeVisible()
    expect(within(getProviderCard('Google Vertex AI')).getByRole('button', { name: 'Cloud config' })).toBeVisible()
    const openAiCard = getProviderCard('OpenAI Codex')
    const openAiSignIn = within(openAiCard).getByRole('button', { name: 'Sign in' })
    expect(openAiSignIn).toBeVisible()
    expect(openAiSignIn).not.toBeDisabled()
    expect(within(openAiCard).queryByRole('button', { name: 'Select' })).not.toBeInTheDocument()
    expect(within(openAiCard).queryByRole('button', { name: 'Rename' })).not.toBeInTheDocument()
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
    expect(screen.getByText(/OpenAI Codex .*sign in required/)).toBeVisible()
  })

  it('keeps onboarding provider review truthful for Anthropic API-key readiness', async () => {
    const { adapter: missingKeyAdapter } = createAdapter({
      projects: [],
      runtimeSettings: makeRuntimeSettings({
        providerId: 'anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        anthropicApiKeyConfigured: false,
      }),
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'anthropic',
        runtimeKind: 'anthropic',
        phase: 'idle',
        sessionId: null,
        accountId: null,
      }),
    })

    render(<CadenceApp adapter={missingKeyAdapter} />)

    fireEvent.click(await screen.findByRole('button', { name: 'Get started' }))
    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))
    fireEvent.click(screen.getByRole('button', { name: 'Skip' }))

    expect(await screen.findByRole('heading', { name: 'Review and finish' })).toBeVisible()
    expect(screen.getByText(/Anthropic .*API key required/)).toBeVisible()
  })

  it('keeps onboarding provider review truthful when an Anthropic API key is already saved', async () => {
    const { adapter } = createAdapter({
      projects: [],
      runtimeSettings: makeRuntimeSettings({
        providerId: 'anthropic',
        modelId: 'claude-3-7-sonnet-latest',
        anthropicApiKeyConfigured: true,
      }),
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'anthropic',
        runtimeKind: 'anthropic',
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
    expect(screen.getByText(/Anthropic .*API key saved/)).toBeVisible()
  })

  it('keeps onboarding provider review truthful for missing GitHub Models token readiness', async () => {
    const { adapter } = createAdapter({
      projects: [],
      providerProfiles: {
        activeProfileId: 'github_models-default',
        profiles: [
          makeProviderProfile({
            profileId: 'github_models-default',
            providerId: 'github_models',
            label: 'GitHub Models',
            modelId: 'openai/gpt-4.1',
            active: true,
            readiness: makeMissingProviderReadiness(),
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'github_models',
        runtimeKind: 'openai_compatible',
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
    expect(screen.getByText(/GitHub Models .*API key required/)).toBeVisible()
  })

  it('keeps onboarding provider review truthful for Ollama local endpoint readiness', async () => {
    const { adapter } = createAdapter({
      projects: [],
      providerProfiles: {
        activeProfileId: 'ollama-default',
        profiles: [
          makeProviderProfile({
            profileId: 'ollama-default',
            providerId: 'ollama',
            label: 'Ollama',
            modelId: 'llama3.2',
            baseUrl: 'http://127.0.0.1:11434/v1',
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'local',
              proofUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'ollama',
        runtimeKind: 'openai_compatible',
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
    expect(screen.getByText('Ollama · Custom endpoint · http://127.0.0.1:11434/v1 · local endpoint ready')).toBeVisible()
  })

  it('keeps onboarding provider review truthful for Bedrock ambient-auth readiness', async () => {
    const { adapter } = createAdapter({
      projects: [],
      providerProfiles: {
        activeProfileId: 'bedrock-default',
        profiles: [
          makeProviderProfile({
            profileId: 'bedrock-default',
            providerId: 'bedrock',
            label: 'Amazon Bedrock',
            modelId: 'anthropic.claude-3-7-sonnet-20250219-v1:0',
            region: 'us-east-1',
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'ambient',
              proofUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'bedrock',
        runtimeKind: 'anthropic',
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
    expect(screen.getByText('Amazon Bedrock · Region us-east-1 · ambient auth ready')).toBeVisible()
  })

  it('keeps onboarding provider review truthful for Vertex ambient-auth repair state', async () => {
    const { adapter } = createAdapter({
      projects: [],
      providerProfiles: {
        activeProfileId: 'vertex-default',
        profiles: [
          makeProviderProfile({
            profileId: 'vertex-default',
            providerId: 'vertex',
            label: 'Google Vertex AI',
            modelId: 'claude-3-7-sonnet@20250219',
            region: 'us-central1',
            projectId: 'vertex-project',
            readiness: {
              ready: false,
              status: 'malformed',
              proofUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        providerId: 'vertex',
        runtimeKind: 'anthropic',
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
    expect(screen.getByText('Google Vertex AI · Region us-central1 · Project vertex-project · ambient profile needs repair')).toBeVisible()
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
    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'API key' }))
    expect(screen.queryByLabelText('Model')).not.toBeInTheDocument()
    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-or-v1-test-secret' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(upsertProviderProfile).toHaveBeenCalledTimes(1))
    expect(upsertProviderProfile).toHaveBeenCalledWith({
      profileId: 'openrouter-default',
      providerId: 'openrouter',
      runtimeKind: 'openrouter',
      label: 'OpenRouter',
      modelId: 'openai/gpt-4.1-mini',
      presetId: 'openrouter',
      baseUrl: null,
      apiVersion: null,
      region: null,
      projectId: null,
      apiKey: 'sk-or-v1-test-secret',
      activate: false,
    })
  })

  it('saves Anthropic provider settings from onboarding', async () => {
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
    fireEvent.click(within(getProviderCard('Anthropic')).getByRole('button', { name: 'API key' }))
    fireEvent.change(screen.getByLabelText('API Key'), { target: { value: 'sk-ant-test-secret' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => expect(upsertProviderProfile).toHaveBeenCalledTimes(1))
    expect(upsertProviderProfile).toHaveBeenCalledWith({
      profileId: 'anthropic-default',
      providerId: 'anthropic',
      runtimeKind: 'anthropic',
      label: 'Anthropic',
      modelId: 'claude-3-7-sonnet-latest',
      presetId: 'anthropic',
      baseUrl: null,
      apiVersion: null,
      region: null,
      projectId: null,
      apiKey: 'sk-ant-test-secret',
      activate: false,
    })
  })

  it('rejects legacy provider upsert fields instead of coercing them into generic profile saves', async () => {
    const { adapter } = createAdapter()

    await expect(
      adapter.upsertProviderProfile({
        profileId: 'openai_api-default',
        providerId: 'openai_api',
        label: 'OpenAI-compatible',
        modelId: 'gpt-4.1-mini',
        openrouterApiKey: 'sk-legacy-test-secret',
      } as never),
    ).rejects.toThrow()
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

  it('renders the workflow tab as a blank slate for an imported project', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(screen.queryByText('No milestone assigned')).not.toBeInTheDocument()
    expect(screen.queryByText('Cadence Desktop')).not.toBeInTheDocument()
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
          rootPath: '/tmp/Cadence',
          displayName: 'Cadence',
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

    render(<CadenceApp adapter={adapter} />)

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

  it('opens the Solana workbench from the titlebar in the normal app shell', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.pointerDown(screen.getByRole('button', { name: 'Tools' }), { button: 0, ctrlKey: false })
    fireEvent.click(screen.getByRole('menuitem', { name: 'Open Solana workbench' }))

    expect(screen.getByRole('button', { name: 'Tools' })).toHaveAttribute('aria-pressed', 'true')
    const breadcrumb = screen.getByRole('navigation', {
      name: 'Solana Workbench breadcrumb',
    })

    expect(within(breadcrumb).getByText('Solana Workbench')).toBeVisible()
    expect(within(breadcrumb).getByText('Personas')).toBeVisible()
  })

  it('starts GitHub auth from the titlebar without opening Account settings', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

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

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    expect(document.querySelector('aside[data-collapsed="false"]')).not.toBeNull()

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))

    await waitFor(() => expect(document.querySelector('aside[data-collapsed="true"]')).not.toBeNull())
    expect(screen.getByRole('button', { name: 'Expand project sidebar' })).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Auto' }))

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

    expect(await screen.findByRole('heading', { name: /What can we build together in/ })).toBeVisible()
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled()
    expect(screen.queryByRole('heading', { name: 'Autonomous run truth' })).not.toBeInTheDocument()
    expect(screen.queryByRole('heading', { name: 'Remote escalation trust' })).not.toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Start autonomous run' })).not.toBeInTheDocument()
  })

  it('rehydrates the recovered runtime snapshot after reload without rendering the removed debug panels', async () => {
    const recoveredAutonomousState = makeAutonomousRunState('project-1', 'auto-run-1')
    recoveredAutonomousState.run = {
      ...recoveredAutonomousState.run!,
      recoveryState: 'recovery_required',
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

    const { adapter } = createAdapter({
      snapshot: {
        ...makeSnapshot(),
        autonomousRun: recoveredAutonomousState.run,
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

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )
    await waitFor(() => expect(setup.onProjectUpdated).toHaveBeenCalledTimes(1))
    expect(screen.getByRole('button', { name: 'Project actions for Cadence' })).toBeVisible()

    act(() => {
      setup.emitProjectUpdated({
        project: makeProjectSummary('project-1', 'Cadence Prime'),
        reason: 'metadata_changed',
      })
    })

    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Project actions for Cadence Prime' })).toBeVisible(),
    )
    expect(screen.queryByRole('button', { name: 'Project actions for Cadence' })).not.toBeInTheDocument()
  })

  it('refreshes provider auth UI from runtime:updated events without rerendering', async () => {
    const setup = createAdapter({
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
      runtimeRun: null,
      autonomousState: null,
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByLabelText('Settings'))
    expect(await screen.findByRole('button', { name: 'Sign in' })).toBeVisible()
    await waitFor(() => expect(setup.onRuntimeUpdated).toHaveBeenCalledTimes(1))

    act(() => {
      setup.emitRuntimeUpdated({
        projectId: 'project-1',
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        flowId: 'flow-1',
        sessionId: 'session-1',
        accountId: 'acct-1',
        authPhase: 'authenticated',
        lastErrorCode: null,
        lastError: null,
        updatedAt: '2026-04-22T12:00:00Z',
      })
    })

    await waitFor(() => expect(screen.getByText('Signed in')).toBeVisible())
    expect(screen.getByRole('button', { name: 'Sign out' })).toBeEnabled()

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
      expect(screen.getByText('Event runtime:updated returned an unexpected payload shape.')).toBeVisible(),
    )
  })

  it('refreshes the Agent pane from runtime_run:updated events and rejects mismatched payloads', async () => {
    const setup = createAdapter({
      runtimeRun: null,
      autonomousState: null,
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'authenticated',
        sessionId: 'session-1',
        accountId: 'acct-1',
        lastErrorCode: null,
        lastError: null,
      }),
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: /What can we build together in/ })).toBeVisible()
    await waitFor(() => expect(setup.onRuntimeRunUpdated).toHaveBeenCalledTimes(1))

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', {
          runId: 'run-live-1',
          startedAt: '2026-04-20T12:00:00Z',
          lastHeartbeatAt: '2026-04-20T12:00:05Z',
          lastCheckpointAt: '2026-04-20T12:00:06Z',
          updatedAt: '2026-04-20T12:00:06Z',
          controls: {
            active: {
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'suggest',
              planModeRequired: false,
              revision: 1,
              appliedAt: '2026-04-20T12:00:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    expect(screen.queryByRole('heading', { name: 'Recovered run snapshot' })).not.toBeInTheDocument()

    const pendingRun = makeRuntimeRun('project-1', {
      runId: 'run-live-1',
      startedAt: '2026-04-20T12:00:00Z',
      lastHeartbeatAt: '2026-04-20T12:05:00Z',
      lastCheckpointSequence: 2,
      lastCheckpointAt: '2026-04-20T12:05:00Z',
      updatedAt: '2026-04-20T12:05:00Z',
      controls: {
        active: {
          modelId: 'openai_codex',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          planModeRequired: false,
          revision: 1,
          appliedAt: '2026-04-20T12:00:00Z',
        },
        pending: {
          modelId: 'anthropic/claude-3.5-haiku',
          thinkingEffort: 'low',
          approvalMode: 'yolo',
          planModeRequired: false,
          revision: 2,
          queuedAt: '2026-04-20T12:05:00Z',
          queuedPrompt: 'Review the diff before continuing.',
          queuedPromptAt: '2026-04-20T12:05:00Z',
        },
      },
    })

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: pendingRun,
      })
    })

    await waitFor(() => expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled())
    expect(screen.queryByText('Model pending · anthropic/claude-3.5-haiku')).not.toBeInTheDocument()
    expect(screen.queryByText('Thinking pending · Low')).not.toBeInTheDocument()
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText('Queued prompt pending the next model-call boundary.')).not.toBeInTheDocument()

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: pendingRun,
      })
    })

    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()
    expect(() =>
      setup.emitRuntimeRunUpdated({
        projectId: 'project-2',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-2', { runId: 'run-project-2' }),
      }),
    ).toThrowError(/expected one of \[project-1\]/)
    expect(() =>
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', { runId: 'run-live-2' }),
      }),
    ).toThrowError(/clear the active run before attaching `run-live-2`/)

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', {
          runId: 'run-live-1',
          startedAt: '2026-04-20T12:00:00Z',
          lastHeartbeatAt: '2026-04-20T12:06:00Z',
          lastCheckpointSequence: 3,
          lastCheckpointAt: '2026-04-20T12:06:00Z',
          updatedAt: '2026-04-20T12:06:00Z',
          controls: {
            active: {
              modelId: 'anthropic/claude-3.5-haiku',
              thinkingEffort: 'low',
              approvalMode: 'yolo',
              planModeRequired: false,
              revision: 2,
              appliedAt: '2026-04-20T12:06:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())
    expect(screen.queryByText('Approval active · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText('Queued prompt pending the next model-call boundary.')).not.toBeInTheDocument()

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: null,
      })
    })

    await waitFor(() => expect(screen.getByRole('heading', { name: /What can we build together in/ })).toBeVisible())

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', {
          runId: 'run-live-2',
          startedAt: '2026-04-20T12:07:00Z',
          lastHeartbeatAt: '2026-04-20T12:07:05Z',
          lastCheckpointSequence: 4,
          lastCheckpointAt: '2026-04-20T12:07:06Z',
          updatedAt: '2026-04-20T12:07:06Z',
          controls: {
            active: {
              modelId: 'anthropic/claude-3.5-haiku',
              thinkingEffort: 'low',
              approvalMode: 'yolo',
              planModeRequired: false,
              revision: 2,
              appliedAt: '2026-04-20T12:07:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() => expect(screen.queryByRole('heading', { name: 'Recovered run snapshot' })).not.toBeInTheDocument())
    expect(screen.queryByText('Approval active · YOLO')).not.toBeInTheDocument()

    act(() => {
      setup.emitRuntimeRunUpdatedError(
        new CadenceDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: 'Event runtime_run:updated returned an unexpected payload shape.',
        }),
      )
    })

    await waitFor(() =>
      expect(screen.getByText('Event runtime_run:updated returned an unexpected payload shape.')).toBeVisible(),
    )
  })

  it('starts the shipped Agent path with openai_api provider identity and openai_compatible runtime truth', async () => {
    const setup = createAdapter({
      providerProfiles: {
        activeProfileId: 'openai_api-default',
        profiles: [
          makeProviderProfile({
            profileId: 'openai_api-default',
            providerId: 'openai_api',
            label: 'OpenAI-compatible',
            modelId: 'gpt-4.1-mini',
            baseUrl: 'https://api.openai.example/v1',
            apiVersion: '2024-10-21',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'stored_secret',
              proofUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
          makeProviderProfile({
            profileId: 'openai_codex-default',
            providerId: 'openai_codex',
            label: 'OpenAI Codex',
            modelId: 'openai_codex',
            active: false,
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        runtimeKind: 'openai_compatible',
        providerId: 'openai_api',
        phase: 'authenticated',
        sessionId: 'session-openai-api',
        accountId: 'acct-openai-api',
        lastErrorCode: null,
        lastError: null,
      }),
      runtimeRun: null,
      autonomousState: null,
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: /What can we build together in/ })).toBeVisible()
    expect(screen.getByRole('combobox', { name: 'Model selector' })).toHaveTextContent('gpt-4.1-mini')

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Start the OpenAI-compatible run.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(setup.startRuntimeRun).toHaveBeenCalledWith('project-1', 'agent-session-main', {
        initialControls: {
          providerProfileId: 'openai_api-default',
          modelId: 'gpt-4.1-mini',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          planModeRequired: false,
        },
        initialPrompt: 'Start the OpenAI-compatible run.',
      }),
    )
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())
  })

  it('starts the shipped Agent path with GitHub Models provider identity and shared catalog truth', async () => {
    const setup = createAdapter({
      providerProfiles: {
        activeProfileId: 'github_models-default',
        profiles: [
          makeProviderProfile({
            profileId: 'github_models-default',
            providerId: 'github_models',
            label: 'GitHub Models',
            modelId: 'openai/gpt-4.1',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'stored_secret',
              proofUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
          makeProviderProfile({
            profileId: 'openai_codex-default',
            providerId: 'openai_codex',
            label: 'OpenAI Codex',
            modelId: 'openai_codex',
            active: false,
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        runtimeKind: 'openai_compatible',
        providerId: 'github_models',
        phase: 'authenticated',
        sessionId: 'session-github-models',
        accountId: 'acct-github-models',
        lastErrorCode: null,
        lastError: null,
      }),
      runtimeRun: null,
      autonomousState: null,
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: /What can we build together in/ })).toBeVisible()
    expect(screen.getByRole('combobox', { name: 'Model selector' })).toHaveTextContent('openai/gpt-4.1')

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Start the GitHub Models run.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(setup.startRuntimeRun).toHaveBeenCalledWith('project-1', 'agent-session-main', {
        initialControls: {
          providerProfileId: 'github_models-default',
          modelId: 'openai/gpt-4.1',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          planModeRequired: false,
        },
        initialPrompt: 'Start the GitHub Models run.',
      }),
    )
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())
  })

  it('starts the shipped Agent path with Ollama provider identity and local model truth', async () => {
    const setup = createAdapter({
      providerProfiles: {
        activeProfileId: 'ollama-default',
        profiles: [
          makeProviderProfile({
            profileId: 'ollama-default',
            providerId: 'ollama',
            label: 'Ollama',
            modelId: 'llama3.2',
            baseUrl: 'http://127.0.0.1:11434/v1',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'local',
              proofUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
          makeProviderProfile({
            profileId: 'openai_codex-default',
            providerId: 'openai_codex',
            label: 'OpenAI Codex',
            modelId: 'openai_codex',
            active: false,
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        runtimeKind: 'openai_compatible',
        providerId: 'ollama',
        phase: 'authenticated',
        sessionId: 'session-ollama',
        accountId: 'acct-ollama',
        lastErrorCode: null,
        lastError: null,
      }),
      runtimeRun: null,
      autonomousState: null,
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: /What can we build together in/ })).toBeVisible()
    expect(screen.getByRole('combobox', { name: 'Model selector' })).toHaveTextContent('llama3.2')
    expect(screen.getByRole('combobox', { name: 'Thinking level selector' })).toHaveTextContent('Thinking unavailable')

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Start the local model run.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(setup.startRuntimeRun).toHaveBeenCalledWith('project-1', 'agent-session-main', {
        initialControls: {
          providerProfileId: 'ollama-default',
          modelId: 'llama3.2',
          thinkingEffort: null,
          approvalMode: 'suggest',
          planModeRequired: false,
        },
        initialPrompt: 'Start the local model run.',
      }),
    )
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())
  })

  it('keeps recovered OpenAI Codex run truth visible when the configured profile drifts to Ollama and blocks relaunch', async () => {
    const setup = createAdapter({
      providerProfiles: {
        activeProfileId: 'ollama-default',
        profiles: [
          makeProviderProfile({
            profileId: 'ollama-default',
            providerId: 'ollama',
            label: 'Ollama',
            modelId: 'llama3.2',
            baseUrl: 'http://127.0.0.1:11434/v1',
            active: true,
            readiness: {
              ready: true,
              status: 'ready',
              proof: 'local',
              proofUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
        ],
        migration: null,
      },
      runtimeSession: makeRuntimeSession('project-1', {
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        phase: 'authenticated',
        sessionId: 'session-openai-codex',
        accountId: 'acct-openai-codex',
        lastErrorCode: null,
        lastError: null,
      }),
      runtimeRun: makeRuntimeRun('project-1', {
        runtimeKind: 'openai_codex',
        providerId: 'openai_codex',
        status: 'stale',
        transport: {
          kind: 'tcp',
          endpoint: '127.0.0.1:4455',
          liveness: 'unreachable',
        },
        lastHeartbeatAt: '2026-04-22T12:06:00Z',
        updatedAt: '2026-04-22T12:06:00Z',
      }),
      autonomousState: null,
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))

    expect(screen.queryByRole('heading', { name: 'Recovered run snapshot' })).not.toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Model selector' })).toHaveTextContent('openai_codex')
    expect(screen.getByLabelText('Agent input unavailable')).toHaveAttribute(
      'placeholder',
      'Rebind Ollama before trusting new live activity.',
    )
    expect(screen.queryByRole('button', { name: 'Start run' })).not.toBeInTheDocument()
  })

  it('proves auth, provider-backed model truth, and pending-to-active boundary application through the shipped Agent path', async () => {
    const setup = createAdapter({
      runtimeRun: null,
      autonomousState: null,
      runtimeSession: makeRuntimeSession('project-1'),
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByLabelText('Settings'))
    expect(await screen.findByRole('heading', { name: 'Providers' })).toBeVisible()
    expect(screen.getAllByText('OpenAI Codex').length).toBeGreaterThan(0)
    const openAiCard = getProviderCard('OpenAI Codex')
    expect(within(openAiCard).queryByRole('button', { name: 'Select' })).not.toBeInTheDocument()
    expect(within(openAiCard).getByText('Signed in')).toBeVisible()
    fireEvent.click(screen.getByRole('button', { name: 'Close' }))

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    await waitFor(() =>
      expect(screen.getByRole('combobox', { name: 'Model selector' })).toHaveTextContent('openai_codex'),
    )
    expect(screen.getByRole('combobox', { name: 'Thinking level selector' })).toHaveTextContent('Medium')
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toHaveTextContent('Suggest')

    fireEvent.change(await screen.findByLabelText('Agent input'), {
      target: { value: 'Start the authenticated run.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(setup.startRuntimeRun).toHaveBeenCalledWith('project-1', 'agent-session-main', {
        initialControls: {
          providerProfileId: 'openai_codex-default',
          modelId: 'openai_codex',
          thinkingEffort: 'medium',
          approvalMode: 'suggest',
          planModeRequired: false,
        },
        initialPrompt: 'Start the authenticated run.',
      }),
    )
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', {
          runId: 'run-1',
          startedAt: '2026-04-22T12:00:00Z',
          lastHeartbeatAt: '2026-04-22T12:01:00Z',
          lastCheckpointSequence: 1,
          lastCheckpointAt: '2026-04-22T12:01:00Z',
          updatedAt: '2026-04-22T12:01:00Z',
        }),
      })
    })
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toBeEnabled())

    fireEvent.keyDown(screen.getByRole('combobox', { name: 'Approval mode selector' }), { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'YOLO' }))

    await waitFor(() =>
      expect(setup.updateRuntimeRunControls).toHaveBeenNthCalledWith(1, {
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-1',
        controls: {
          providerProfileId: 'openai_codex-default',
          modelId: 'openai_codex',
          thinkingEffort: 'medium',
          approvalMode: 'yolo',
          planModeRequired: false,
        },
        prompt: null,
      }),
    )
    await waitFor(() => expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled())
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText(/Pending YOLO does not apply until the next model-call boundary\./)).not.toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toBeDisabled()

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Review the diff before continuing.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))

    await waitFor(() =>
      expect(setup.updateRuntimeRunControls).toHaveBeenNthCalledWith(2, {
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        runId: 'run-1',
        controls: null,
        prompt: 'Review the diff before continuing.',
      }),
    )
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toHaveValue(''))

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', {
          runId: 'run-1',
          startedAt: '2026-04-22T12:00:00Z',
          lastHeartbeatAt: '2026-04-22T12:06:00Z',
          lastCheckpointSequence: 2,
          lastCheckpointAt: '2026-04-22T12:06:00Z',
          updatedAt: '2026-04-22T12:06:00Z',
          controls: {
            active: {
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'yolo',
              planModeRequired: false,
              revision: 2,
              appliedAt: '2026-04-22T12:06:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())
    expect(screen.queryByText('Approval active · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText('Queued prompt pending the next model-call boundary.')).not.toBeInTheDocument()
    expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).not.toBeDisabled()
  }, 15_000)

  it('keeps live review-required checkpoint truth visible on the shipped Agent surface even after YOLO becomes active', async () => {
    const reviewActionId = 'flow:flow-1:run:run-1:boundary:boundary-review-1:review_command'
    const setup = createAdapter({
      runtimeRun: null,
      autonomousState: null,
      runtimeSession: makeRuntimeSession('project-1', {
        phase: 'authenticated',
        sessionId: 'session-1',
        accountId: 'acct-1',
        flowId: 'flow-1',
        lastErrorCode: null,
        lastError: null,
      }),
    })

    render(<CadenceApp adapter={setup.adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Agent' }))
    expect(await screen.findByRole('heading', { name: /What can we build together in/ })).toBeVisible()

    fireEvent.change(screen.getByLabelText('Agent input'), {
      target: { value: 'Start before changing approval.' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Send message' }))
    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())
    await waitFor(() => expect(setup.streamSubscriptions.length).toBeGreaterThan(0))

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', {
          runId: 'run-1',
          startedAt: '2026-04-22T12:00:00Z',
          lastHeartbeatAt: '2026-04-22T12:01:00Z',
          lastCheckpointSequence: 1,
          lastCheckpointAt: '2026-04-22T12:01:00Z',
          updatedAt: '2026-04-22T12:01:00Z',
        }),
      })
    })
    await waitFor(() => expect(screen.getByRole('combobox', { name: 'Approval mode selector' })).toBeEnabled())

    fireEvent.keyDown(screen.getByRole('combobox', { name: 'Approval mode selector' }), { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'YOLO' }))

    await waitFor(() => expect(screen.getByRole('button', { name: 'Send message' })).toBeDisabled())
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()

    act(() => {
      setup.emitRuntimeRunUpdated({
        projectId: 'project-1',
        agentSessionId: 'agent-session-main',
        run: makeRuntimeRun('project-1', {
          runId: 'run-1',
          startedAt: '2026-04-22T12:00:00Z',
          lastHeartbeatAt: '2026-04-22T12:07:00Z',
          lastCheckpointSequence: 3,
          lastCheckpointAt: '2026-04-22T12:07:00Z',
          updatedAt: '2026-04-22T12:07:00Z',
          controls: {
            active: {
              modelId: 'openai_codex',
              thinkingEffort: 'medium',
              approvalMode: 'yolo',
              planModeRequired: false,
              revision: 2,
              appliedAt: '2026-04-22T12:07:00Z',
            },
            pending: null,
          },
        }),
      })
    })

    await waitFor(() => expect(screen.getByLabelText('Agent input')).toBeEnabled())
    expect(screen.queryByText('Approval active · YOLO')).not.toBeInTheDocument()
    expect(screen.queryByText('Approval pending · YOLO')).not.toBeInTheDocument()

    setup.setSnapshot({
      ...makeSnapshot(),
      approvalRequests: [makeRuntimeApproval(reviewActionId)],
      resumeHistory: [],
      notificationDispatches: [],
      notificationReplyClaims: [],
    })

    act(() => {
      setup.emitRuntimeStream(
        0,
        makeRuntimeStreamActionRequiredEvent({
          actionId: reviewActionId,
          boundaryId: 'boundary-review-1',
          detail: 'Cadence blocked a destructive shell wrapper and needs operator review before continuing.',
        }),
      )
    })

    await waitFor(() => expect(screen.getByRole('heading', { name: 'Checkpoint control loop' })).toBeVisible())
    expect(screen.getByText('Review destructive shell command')).toBeVisible()
    expect(screen.getByText('Live action required')).toBeVisible()
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

  it('keeps open editor tabs and unsaved edits when switching away from Editor and back', async () => {
    const { adapter } = createAdapter()

    render(<CadenceApp adapter={adapter} />)

    await waitFor(() =>
      expect(screen.queryByRole('heading', { name: 'Loading desktop project state' })).not.toBeInTheDocument(),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))
    fireEvent.click(await screen.findByRole('button', { name: 'README.md' }))

    const editor = await screen.findByLabelText('Editor for /README.md')
    const executionPane = editor.closest('[aria-hidden]')
    expect(executionPane).toHaveAttribute('aria-hidden', 'false')
    fireEvent.change(editor, { target: { value: '# Draft changes\n' } })

    fireEvent.click(screen.getByRole('button', { name: 'Auto' }))
    await waitFor(() => expect(executionPane).toHaveAttribute('aria-hidden', 'true'))
    expect(screen.queryByText('No milestone assigned')).not.toBeInTheDocument()
    expect(screen.queryByText('Cadence Desktop')).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Editor' }))

    const restoredEditor = await screen.findByLabelText('Editor for /README.md')
    expect(restoredEditor).toBeVisible()
    expect(restoredEditor).toHaveValue('# Draft changes\n')
    expect(screen.getByRole('button', { name: 'Close README.md' })).toBeVisible()
  })
})
