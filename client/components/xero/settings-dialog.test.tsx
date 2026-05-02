import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'

const { invokeMock, isTauriMock, openUrlMock } = vi.hoisted(() => ({
  invokeMock: vi.fn(),
  isTauriMock: vi.fn(() => true),
  openUrlMock: vi.fn(),
}))

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
  isTauri: isTauriMock,
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

import { SettingsDialog, type SettingsDialogProps } from '@/components/xero/settings-dialog'
import { createXeroDiagnosticCheck, createXeroDoctorReport } from '@/src/lib/xero-model'
import type { AgentPaneView, OperatorActionErrorView } from '@/src/features/xero/use-xero-desktop-state'
import type {
  McpRegistryDto,
  XeroDoctorReportDto,
  DictationSettingsDto,
  DictationStatusDto,
  RuntimeSessionView,
  SkillRegistryDto,
  SoulSettingsDto,
} from '@/src/lib/xero-model'

type NotificationRouteRequest = Parameters<NonNullable<SettingsDialogProps['onUpsertNotificationRoute']>>[0]
type McpUpsertRequest = Parameters<NonNullable<SettingsDialogProps['onUpsertMcpServer']>>[0]
type SetSkillEnabledRequest = Parameters<NonNullable<SettingsDialogProps['onSetSkillEnabled']>>[0]
type RemoveSkillRequest = Parameters<NonNullable<SettingsDialogProps['onRemoveSkill']>>[0]
type UpsertSkillLocalRootRequest = Parameters<NonNullable<SettingsDialogProps['onUpsertSkillLocalRoot']>>[0]
type RemoveSkillLocalRootRequest = Parameters<NonNullable<SettingsDialogProps['onRemoveSkillLocalRoot']>>[0]
type UpdateProjectSkillSourceRequest = Parameters<NonNullable<SettingsDialogProps['onUpdateProjectSkillSource']>>[0]
type UpdateGithubSkillSourceRequest = Parameters<NonNullable<SettingsDialogProps['onUpdateGithubSkillSource']>>[0]
type UpsertPluginRootRequest = Parameters<NonNullable<SettingsDialogProps['onUpsertPluginRoot']>>[0]
type RemovePluginRootRequest = Parameters<NonNullable<SettingsDialogProps['onRemovePluginRoot']>>[0]
type SetPluginEnabledRequest = Parameters<NonNullable<SettingsDialogProps['onSetPluginEnabled']>>[0]
type RemovePluginRequest = Parameters<NonNullable<SettingsDialogProps['onRemovePlugin']>>[0]

function makeDictationStatus(overrides: Partial<DictationStatusDto> = {}): DictationStatusDto {
  return {
    platform: 'macos',
    osVersion: '26.0.0',
    defaultLocale: 'en_US',
    supportedLocales: ['en_US', 'es_US'],
    modern: {
      available: false,
      compiled: false,
      runtimeSupported: false,
      reason: 'modern_sdk_unavailable',
    },
    legacy: {
      available: true,
      compiled: true,
      runtimeSupported: true,
      reason: null,
    },
    modernAssets: {
      status: 'unavailable',
      locale: null,
      reason: 'modern_sdk_unavailable',
    },
    microphonePermission: 'denied',
    speechPermission: 'authorized',
    activeSession: null,
    ...overrides,
  }
}

function makeDictationSettings(overrides: Partial<DictationSettingsDto> = {}): DictationSettingsDto {
  return {
    enginePreference: 'automatic',
    privacyMode: 'on_device_preferred',
    locale: null,
    updatedAt: null,
    ...overrides,
  }
}

function makeDictationAdapter(
  overrides: {
    status?: DictationStatusDto
    settings?: DictationSettingsDto
  } = {},
): NonNullable<SettingsDialogProps['dictationAdapter']> {
  return {
    isDesktopRuntime: vi.fn(() => true),
    speechDictationStatus: vi.fn(async () => overrides.status ?? makeDictationStatus()),
    speechDictationSettings: vi.fn(async () => overrides.settings ?? makeDictationSettings()),
    speechDictationUpdateSettings: vi.fn(async (request) => ({
      ...request,
      updatedAt: '2026-04-26T12:30:00Z',
    })),
  }
}

function makeSoulSettings(overrides: Partial<SoulSettingsDto> = {}): SoulSettingsDto {
  const presets = [
    {
      id: 'steward' as const,
      name: 'Steady steward',
      summary: 'Calm, grounded, and quietly thorough.',
      prompt:
        'Be calm, grounded, and quietly thorough. Help the user feel oriented. Prefer evidence, plain language, scoped action, and measured next steps.',
    },
    {
      id: 'pair' as const,
      name: 'Warm pair',
      summary: 'Collaborative, teaching-aware, and conversational.',
      prompt:
        'Act like a generous pair programmer. Think with the user, name tradeoffs, teach briefly when useful, and keep collaboration warm without slowing momentum.',
    },
    {
      id: 'builder' as const,
      name: 'Sharp builder',
      summary: 'Decisive, pragmatic, and momentum-oriented.',
      prompt:
        'Be decisive and momentum-oriented. Minimize ceremony, choose sensible defaults, make progress in small verified steps, and keep summaries crisp.',
    },
    {
      id: 'sentinel' as const,
      name: 'Careful sentinel',
      summary: 'Skeptical, risk-aware, and verification-minded.',
      prompt:
        'Be constructively skeptical. Look for hidden risks, edge cases, missing tests, security hazards, and data-loss hazards. Call out uncertainty before it becomes damage.',
    },
  ]
  const selectedSoulId = overrides.selectedSoulId ?? 'steward'
  const selectedSoul =
    overrides.selectedSoul ??
    presets.find((preset) => preset.id === selectedSoulId) ??
    presets[0]

  return {
    selectedSoulId,
    selectedSoul,
    presets,
    updatedAt: null,
    ...overrides,
  }
}

function makeSoulAdapter(
  overrides: {
    settings?: SoulSettingsDto
  } = {},
): NonNullable<SettingsDialogProps['soulAdapter']> {
  const settings = overrides.settings ?? makeSoulSettings()
  return {
    isDesktopRuntime: vi.fn(() => true),
    soulSettings: vi.fn(async () => settings),
    soulUpdateSettings: vi.fn(async (request) =>
      makeSoulSettings({
        selectedSoulId: request.selectedSoulId,
        selectedSoul:
          settings.presets.find((preset) => preset.id === request.selectedSoulId) ??
          settings.selectedSoul,
        presets: settings.presets,
        updatedAt: '2026-05-01T12:00:00Z',
      }),
    ),
  }
}

function makeDoctorReport(): XeroDoctorReportDto {
  return createXeroDoctorReport({
    reportId: 'doctor-20260426-120000',
    generatedAt: '2026-04-26T12:00:00Z',
    mode: 'quick_local',
    versions: {
      appVersion: 'test',
      runtimeSupervisorVersion: 'test',
      runtimeProtocolVersion: 'supervisor-v1',
    },
    profileChecks: [
      createXeroDiagnosticCheck({
        subject: 'provider_credential',
        status: 'passed',
        severity: 'info',
        retryable: false,
        code: 'provider_credential_ready',
        message: 'Provider credential `openrouter-work` is ready.',
        affectedProfileId: 'openrouter-work',
        affectedProviderId: 'openrouter',
      }),
    ],
    runtimeSupervisorChecks: [
      createXeroDiagnosticCheck({
        subject: 'runtime_binding',
        status: 'failed',
        severity: 'error',
        retryable: false,
        code: 'provider_credential_missing',
        message: 'Runtime startup failed because provider credentials are missing.',
        affectedProviderId: 'openrouter',
        remediation: 'Open Providers settings, repair credentials, then restart the runtime session.',
      }),
    ],
    settingsDependencyChecks: [
      createXeroDiagnosticCheck({
        subject: 'settings_dependency',
        status: 'skipped',
        severity: 'info',
        retryable: false,
        code: 'notification_routes_not_configured',
        message: 'No notification routes are configured.',
        remediation: 'Add a notification route before checking notification credential readiness.',
      }),
    ],
  })
}

function makeRuntimeSession(overrides: Partial<RuntimeSessionView> = {}): RuntimeSessionView {
  return {
    projectId: 'project-1',
    runtimeKind: 'openai_codex',
    providerId: 'openai_codex',
    flowId: null,
    sessionId: null,
    accountId: null,
    phase: 'idle',
    phaseLabel: 'Idle',
    runtimeLabel: 'Openai Codex · Signed out',
    accountLabel: 'No account',
    sessionLabel: 'No session',
    callbackBound: null,
    authorizationUrl: null,
    redirectUri: null,
    lastErrorCode: null,
    lastError: null,
    updatedAt: '2026-04-20T00:00:00Z',
    isAuthenticated: false,
    isLoginInProgress: false,
    needsManualInput: false,
    isSignedOut: true,
    isFailed: false,
    ...overrides,
  }
}

function makeNotificationRoute(
  overrides: Partial<AgentPaneView['notificationRoutes'][number]> = {},
): AgentPaneView['notificationRoutes'][number] {
  return {
    projectId: 'project-1',
    routeId: 'ops-alerts',
    routeKind: 'telegram',
    routeKindLabel: 'Telegram',
    routeTarget: 'telegram:@ops-room',
    enabled: true,
    metadataJson: null,
    credentialReadiness: null,
    credentialDiagnosticCode: null,
    createdAt: '2026-04-20T00:00:00Z',
    updatedAt: '2026-04-20T00:00:00Z',
    dispatchCount: 0,
    pendingCount: 0,
    sentCount: 0,
    failedCount: 0,
    claimedCount: 0,
    latestDispatchAt: null,
    latestFailureCode: null,
    latestFailureMessage: null,
    health: 'healthy',
    healthLabel: 'Ready',
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
    sources: {
      localRoots: [
        {
          rootId: 'team-skills',
          path: '/tmp/xero-skills',
          enabled: true,
          updatedAt: '2026-04-24T05:00:00Z',
        },
      ],
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
    entries: [
      {
        sourceId: 'project:project-1:reviewer',
        skillId: 'reviewer',
        name: 'Reviewer',
        description: 'Reviews code changes before the agent finishes.',
        sourceKind: 'project',
        scope: 'project',
        projectId: 'project-1',
        sourceState: 'enabled',
        trustState: 'user_approved',
        enabled: true,
        installed: true,
        userInvocable: true,
        versionHash: 'abcdef1234567890',
        lastUsedAt: '2026-04-24T04:30:00Z',
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
      {
        sourceId: 'local:team-skills:release-notes',
        skillId: 'release-notes',
        name: 'Release Notes',
        description: 'Drafts release notes from recent commits.',
        sourceKind: 'local',
        scope: 'global',
        projectId: null,
        sourceState: 'discoverable',
        trustState: 'approval_required',
        enabled: false,
        installed: false,
        userInvocable: true,
        versionHash: '123456abcdef',
        lastUsedAt: null,
        lastDiagnostic: {
          code: 'skill_load_warning',
          message: 'Skill has not been approved for this project.',
          retryable: true,
          recordedAt: '2026-04-24T04:00:00Z',
        },
        source: {
          label: 'Local root team-skills - release-notes',
          repo: null,
          reference: null,
          path: '/tmp/xero-skills',
          rootId: 'team-skills',
          rootPath: '/tmp/xero-skills',
          relativePath: 'release-notes',
          bundleId: null,
          pluginId: null,
          serverId: null,
        },
      },
    ],
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

function makeAgent(overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  return {
    project: {
      id: 'project-1',
      name: 'Xero',
      repository: {
        rootPath: '/tmp/Xero',
      },
    } as AgentPaneView['project'],
    activePhase: null,
    branchLabel: 'main',
    headShaLabel: 'abc123',
    runtimeLabel: 'Openai Codex · Signed out',
    repositoryLabel: 'Xero',
    repositoryPath: '/tmp/Xero',
    runtimeSession: makeRuntimeSession(),
    runtimeRun: null,
    autonomousRun: null,
    autonomousUnit: null,
    autonomousAttempt: null,
    autonomousWorkflowContext: null,
    autonomousHistory: [],
    autonomousRecentArtifacts: [],
    recentAutonomousUnits: undefined,
    runtimeErrorMessage: null,
    runtimeRunErrorMessage: null,
    autonomousRunErrorMessage: null,
    authPhase: 'idle',
    authPhaseLabel: 'Idle',
    runtimeStream: null,
    runtimeStreamStatus: 'idle',
    runtimeStreamStatusLabel: 'No live stream',
    runtimeStreamError: null,
    runtimeStreamItems: [],
    activityItems: [],
    actionRequiredItems: [],
    notificationBroker: {
      dispatches: [],
      actions: [],
      routes: [],
      byActionId: {},
      byRouteId: {},
      dispatchCount: 0,
      routeCount: 0,
      pendingCount: 0,
      sentCount: 0,
      failedCount: 0,
      claimedCount: 0,
      latestUpdatedAt: null,
      isTruncated: false,
      totalBeforeTruncation: 0,
    },
    notificationRoutes: [],
    notificationChannelHealth: [],
    notificationRouteLoadStatus: 'idle',
    notificationRouteIsRefreshing: false,
    notificationRouteError: null,
    notificationSyncSummary: null,
    notificationSyncError: null,
    notificationSyncPollingActive: false,
    notificationSyncPollingActionId: null,
    notificationSyncPollingBoundaryId: null,
    notificationRouteMutationStatus: 'idle',
    pendingNotificationRouteId: null,
    notificationRouteMutationError: null,
    trustSnapshot: undefined,
    approvalRequests: [],
    pendingApprovalCount: 0,
    latestDecisionOutcome: null,
    controlTruthSource: 'fallback',
    selectedRuntimeAgentId: 'ask',
    selectedRuntimeAgentLabel: 'Ask',
    resumeHistory: [],
    operatorActionStatus: 'idle',
    pendingOperatorActionId: null,
    operatorActionError: null,
    autonomousRunActionStatus: 'idle',
    pendingAutonomousRunAction: null,
    autonomousRunActionError: null,
    runtimeRunActionStatus: 'idle',
    pendingRuntimeRunAction: null,
    runtimeRunActionError: null,
    sessionUnavailableReason: 'Signed out.',
    runtimeRunUnavailableReason: 'No runtime run yet.',
    messagesUnavailableReason: 'No messages yet.',
    ...overrides,
  } as AgentPaneView
}

function makeError(overrides: Partial<OperatorActionErrorView> = {}): OperatorActionErrorView {
  return {
    code: 'provider_profiles_failed',
    message: 'Xero could not load app-local provider profiles.',
    retryable: true,
    ...overrides,
  }
}

function makeSettingsDialogProps(overrides: Partial<SettingsDialogProps> & Record<string, unknown> = {}): SettingsDialogProps {
  // Phase 4: legacy provider-profile props are accepted in `overrides` for
  // backwards compatibility with skipped legacy tests, but are ignored.
  const { providerProfiles: _pp, providerProfilesLoadStatus: _ppls, providerProfilesLoadError: _pple,
    providerProfilesSaveStatus: _ppss, providerProfilesSaveError: _ppse,
    onUpsertProviderProfile: _ouop, onStartLogin: _osl, onLogout: _ol,
    onLogoutProviderProfile: _olop, onRefreshProviderProfiles: _orpp, ...rest } = overrides as Record<string, unknown>
  void _pp, _ppls, _pple, _ppss, _ppse, _ouop, _osl, _ol, _olop, _orpp
  return {
    open: true,
    onOpenChange: vi.fn(),
    agent: makeAgent(),
    providerCredentials: { credentials: [] },
    providerCredentialsLoadStatus: 'ready',
    providerCredentialsLoadError: null,
    providerCredentialsSaveStatus: 'idle',
    providerCredentialsSaveError: null,
    onRefreshProviderCredentials: vi.fn(async () => ({ credentials: [] })),
    onUpsertProviderCredential: vi.fn(async () => ({ credentials: [] })),
    onDeleteProviderCredential: vi.fn(async () => ({ credentials: [] })),
    onStartOAuthLogin: vi.fn(async () => makeRuntimeSession()),
    doctorReport: null,
    doctorReportStatus: 'idle',
    doctorReportError: null,
    onRunDoctorReport: vi.fn(async () => makeDoctorReport()),
    soulAdapter: makeSoulAdapter(),
    mcpRegistry: makeMcpRegistry(),
    mcpImportDiagnostics: [],
    mcpRegistryLoadStatus: 'ready',
    mcpRegistryLoadError: null,
    mcpRegistryMutationStatus: 'idle',
    pendingMcpServerId: null,
    mcpRegistryMutationError: null,
    onRefreshMcpRegistry: vi.fn(async () => makeMcpRegistry()),
    onUpsertMcpServer: vi.fn(async (_request: McpUpsertRequest) => makeMcpRegistry()),
    onRemoveMcpServer: vi.fn(async (_serverId: string) => makeMcpRegistry()),
    onImportMcpServers: vi.fn(async (_path: string) => ({ registry: makeMcpRegistry(), diagnostics: [] })),
    onRefreshMcpServerStatuses: vi.fn(async (_options?: { serverIds?: string[] }) => makeMcpRegistry()),
    skillRegistry: makeSkillRegistry(),
    skillRegistryLoadStatus: 'ready',
    skillRegistryLoadError: null,
    skillRegistryMutationStatus: 'idle',
    pendingSkillSourceId: null,
    skillRegistryMutationError: null,
    onRefreshSkillRegistry: vi.fn(async () => makeSkillRegistry()),
    onReloadSkillRegistry: vi.fn(async () => makeSkillRegistry()),
    onSetSkillEnabled: vi.fn(async (_request: SetSkillEnabledRequest) => makeSkillRegistry()),
    onRemoveSkill: vi.fn(async (_request: RemoveSkillRequest) => makeSkillRegistry()),
    onUpsertSkillLocalRoot: vi.fn(async (_request: UpsertSkillLocalRootRequest) => makeSkillRegistry()),
    onRemoveSkillLocalRoot: vi.fn(async (_request: RemoveSkillLocalRootRequest) => makeSkillRegistry()),
    onUpdateProjectSkillSource: vi.fn(async (_request: UpdateProjectSkillSourceRequest) => makeSkillRegistry()),
    onUpdateGithubSkillSource: vi.fn(async (_request: UpdateGithubSkillSourceRequest) => makeSkillRegistry()),
    onUpsertPluginRoot: vi.fn(async (_request: UpsertPluginRootRequest) => makeSkillRegistry()),
    onRemovePluginRoot: vi.fn(async (_request: RemovePluginRootRequest) => makeSkillRegistry()),
    onSetPluginEnabled: vi.fn(async (_request: SetPluginEnabledRequest) => makeSkillRegistry()),
    onRemovePlugin: vi.fn(async (_request: RemovePluginRequest) => makeSkillRegistry()),
    ...(rest as Partial<SettingsDialogProps>),
  }
}

describe('SettingsDialog', () => {
  beforeEach(() => {
    invokeMock.mockReset()
    invokeMock.mockImplementation((command: string, args?: Record<string, unknown>) => {
      if (command === 'developer_storage_overview') {
        return Promise.resolve({
          globalSqlite: {
            path: '/Users/sn0w/Library/Application Support/dev.sn0w.xero/xero.db',
            tables: [
              {
                name: 'provider_credentials',
                columns: [
                  { name: 'provider_id', typeLabel: 'TEXT' },
                  { name: 'api_key', typeLabel: 'TEXT' },
                ],
                rowCount: 1,
              },
            ],
          },
          projectLance: [
            {
              projectId: 'project-1',
              projectName: 'Xero',
              repositoryRoot: '/tmp/Xero',
              stateDatabasePath: '/Users/sn0w/Library/Application Support/dev.sn0w.xero/projects/project-1/state.db',
              lancePath: '/Users/sn0w/Library/Application Support/dev.sn0w.xero/projects/project-1/lance',
              exists: true,
              tables: [
                {
                  name: 'agent_memories',
                  columns: [
                    { name: 'memory_id', typeLabel: 'TEXT' },
                    { name: 'text', typeLabel: 'TEXT' },
                  ],
                  rowCount: 1,
                },
              ],
            },
          ],
        })
      }
      if (command === 'developer_storage_read_table') {
        const request = args?.request as { revealSensitive?: boolean; tableName?: string } | undefined
        return Promise.resolve({
          source: { kind: 'global_sqlite', projectId: null },
          tableName: request?.tableName ?? 'provider_credentials',
          path: '/Users/sn0w/Library/Application Support/dev.sn0w.xero/xero.db',
          columns: [
            { name: 'provider_id', typeLabel: 'TEXT' },
            { name: 'api_key', typeLabel: 'TEXT' },
          ],
          rows: [
            {
              values: {
                provider_id: 'openrouter',
                api_key: request?.revealSensitive ? 'sk-test' : '[redacted]',
              },
            },
          ],
          rowCount: 1,
          limit: 50,
          offset: 0,
          redacted: !request?.revealSensitive,
        })
      }
      if (command === 'browser_control_settings') {
        return Promise.resolve({
          preference: 'default',
          updatedAt: null,
        })
      }
      if (command === 'browser_list_cookie_sources') {
        return Promise.resolve([])
      }
      if (command === 'browser_control_update_settings') {
        return Promise.resolve({
          preference: 'native_browser',
          updatedAt: '2026-04-30T12:00:00Z',
        })
      }
      return Promise.resolve(null)
    })
    isTauriMock.mockReset()
    isTauriMock.mockReturnValue(true)
    openUrlMock.mockReset()
  })

  it('shows local storage data from the development section with sensitive values redacted by default', async () => {
    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          initialSection: 'development',
        })}
      />,
    )

    expect(await screen.findByText('Local storage')).toBeVisible()
    expect(screen.getByText('provider_credentials')).toBeVisible()
    expect(await screen.findByText('[redacted]')).toBeVisible()
    expect(screen.queryByText('sk-test')).not.toBeInTheDocument()

    fireEvent.click(screen.getByLabelText('Reveal sensitive storage values'))

    await waitFor(() =>
      expect(invokeMock).toHaveBeenLastCalledWith('developer_storage_read_table', {
        request: {
          source: { kind: 'global_sqlite', projectId: null },
          tableName: 'provider_credentials',
          limit: 50,
          offset: 0,
          revealSensitive: true,
        },
      }),
    )
    expect(await screen.findByText('sk-test')).toBeVisible()
  })

  it('renders doctor reports from the diagnostics section and runs extended checks explicitly', async () => {
    const report = makeDoctorReport()
    const onRunDoctorReport = vi.fn(async () => report)

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          initialSection: 'diagnostics',
          doctorReport: report,
          onRunDoctorReport,
        })}
      />,
    )

    expect(screen.getByText('Report summary')).toBeVisible()
    expect(screen.getByText('Runtime startup failed because provider credentials are missing.')).toBeVisible()
    expect(screen.getByText('provider_credential_missing')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Extended' }))

    await waitFor(() => expect(onRunDoctorReport).toHaveBeenCalledWith({ mode: 'extended_network' }))
  })

  it('loads macOS dictation settings and saves preference changes', async () => {
    const dictationAdapter = makeDictationAdapter({
      settings: makeDictationSettings({ locale: 'en_US' }),
    })

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          initialSection: 'dictation',
          dictationAdapter,
        })}
      />,
    )

    expect(await screen.findByText('System availability')).toBeVisible()
    expect(screen.getByText('Modern sdk unavailable')).toBeVisible()
    expect(screen.getByText('Open System Settings > Privacy & Security and allow Xero.')).toBeVisible()

    fireEvent.click(screen.getByRole('combobox', { name: 'Engine preference' }))
    fireEvent.click(await screen.findByRole('option', { name: 'Legacy only' }))

    await waitFor(() =>
      expect(dictationAdapter.speechDictationUpdateSettings).toHaveBeenCalledWith({
        enginePreference: 'legacy',
        privacyMode: 'on_device_preferred',
        locale: 'en_US',
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Open Settings' }))
    await waitFor(() =>
      expect(openUrlMock).toHaveBeenCalledWith(
        'x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone',
      ),
    )
  })

  it('loads Soul settings and saves the selected premade soul', async () => {
    const soulAdapter = makeSoulAdapter()

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          initialSection: 'soul',
          soulAdapter,
        })}
      />,
    )

    expect((await screen.findAllByText('Steady steward'))[0]).toBeVisible()
    expect(screen.getByRole('radio', { name: 'Steady steward' })).toBeChecked()
    expect(screen.getByText('Calm, grounded, and quietly thorough.')).toBeVisible()

    fireEvent.click(screen.getByRole('radio', { name: 'Careful sentinel' }))

    await waitFor(() =>
      expect(soulAdapter.soulUpdateSettings).toHaveBeenCalledWith({
        selectedSoulId: 'sentinel',
      }),
    )
    expect((await screen.findAllByText('Careful sentinel'))[0]).toBeVisible()
  })

  it('saves the agent browser control preference from the browser settings section', async () => {
    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          initialSection: 'browser',
        })}
      />,
    )

    expect(await screen.findByText('Agent browser control')).toBeVisible()
    expect(screen.getByRole('radio', { name: 'Default' })).toBeChecked()

    fireEvent.click(screen.getByRole('radio', { name: 'Native browser' }))

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith('browser_control_update_settings', {
        request: {
          preference: 'native_browser',
        },
      })
    })
  })

  it('shows route target validation errors and omits project metadata when creating routes', async () => {
    const onUpsertNotificationRoute = vi.fn(async (_request: NotificationRouteRequest) => ({ route: makeNotificationRoute() }))

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          onUpsertNotificationRoute,
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))
    fireEvent.click(screen.getAllByRole('button', { name: 'Add route' })[0])

    fireEvent.change(screen.getByLabelText('Route name'), { target: { value: 'ops-alerts' } })
    fireEvent.change(screen.getByLabelText('Target'), { target: { value: 'discord:12345' } })
    fireEvent.click(screen.getByRole('button', { name: 'Create route' }))

    expect(
      screen.getByText('Route target prefix `discord` does not match the selected route kind `telegram`.'),
    ).toBeVisible()
    expect(onUpsertNotificationRoute).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText('Target'), { target: { value: '@ops-room' } })
    fireEvent.click(screen.getByRole('button', { name: 'Create route' }))

    await waitFor(() => expect(onUpsertNotificationRoute).toHaveBeenCalledTimes(1))

    expect(onUpsertNotificationRoute.mock.calls[0][0]).toEqual({
      routeId: 'ops-alerts',
      routeKind: 'telegram',
      routeTarget: 'telegram:@ops-room',
      enabled: true,
      metadataJson: null,
    })
  })

  it('keeps truthful stored targets for edit fallback and toggles existing routes', async () => {
    const onUpsertNotificationRoute = vi.fn(async (_request: NotificationRouteRequest) => ({ route: makeNotificationRoute() }))

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          agent: makeAgent({
            notificationRoutes: [
              makeNotificationRoute({
                routeTarget: 'ops-room',
                enabled: false,
              }),
            ],
          }),
          onUpsertNotificationRoute,
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))

    expect(screen.getByText('ops-room')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Edit' }))
    expect(screen.getByLabelText('Target')).toHaveValue('ops-room')

    fireEvent.change(screen.getByLabelText('Target'), { target: { value: '@pager-room' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save changes' }))

    await waitFor(() => expect(onUpsertNotificationRoute).toHaveBeenCalledTimes(1))
    expect(onUpsertNotificationRoute.mock.calls[0][0]).toEqual({
      routeId: 'ops-alerts',
      routeKind: 'telegram',
      routeTarget: 'telegram:@pager-room',
      enabled: false,
      metadataJson: null,
    })

    fireEvent.click(screen.getByLabelText('Off'))

    await waitFor(() => expect(onUpsertNotificationRoute).toHaveBeenCalledTimes(2))
    expect(onUpsertNotificationRoute.mock.calls[1][0]).toEqual({
      routeId: 'ops-alerts',
      routeKind: 'telegram',
      routeTarget: 'ops-room',
      enabled: true,
      metadataJson: null,
    })
  })












  it('manages MCP servers from settings with validation, import, remove, and status refresh actions', async () => {
    const onUpsertMcpServer = vi.fn(async (_request: McpUpsertRequest) => makeMcpRegistry())
    const onRemoveMcpServer = vi.fn(async (_serverId: string) => makeMcpRegistry({ servers: [] }))
    const onImportMcpServers = vi.fn(async (_path: string) => ({
      registry: makeMcpRegistry(),
      diagnostics: [
        {
          index: 1,
          serverId: 'duplicate-memory',
          code: 'mcp_registry_import_invalid',
          message: 'Server id `memory` is duplicated in the import file.',
        },
      ],
    }))
    const onRefreshMcpServerStatuses = vi.fn(async (_options?: { serverIds?: string[] }) => makeMcpRegistry())

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          onUpsertMcpServer,
          onRemoveMcpServer,
          onImportMcpServers,
          onRefreshMcpServerStatuses,
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'MCP' }))

    expect(screen.getByText('Memory Server')).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Add server' }))
    fireEvent.click(screen.getByRole('button', { name: 'Create server' }))

    expect(screen.getByText('Server id is required.')).toBeVisible()
    expect(screen.getByText('Server name is required.')).toBeVisible()
    expect(screen.getByText('stdio transport requires a command.')).toBeVisible()

    fireEvent.change(screen.getByLabelText('Server id'), { target: { value: 'filesystem' } })
    fireEvent.change(screen.getByLabelText('Display name'), { target: { value: 'Filesystem Server' } })
    fireEvent.change(screen.getByLabelText('Command'), { target: { value: 'node' } })
    fireEvent.change(screen.getByLabelText('Args (one per line)'), {
      target: { value: '/opt/mcp/server-filesystem.js' },
    })
    fireEvent.change(screen.getByLabelText('Env mappings (KEY=ENV_VAR)'), {
      target: { value: 'OPENAI_API_KEY=OPENAI_API_KEY' },
    })

    fireEvent.click(screen.getByRole('button', { name: 'Create server' }))

    await waitFor(() =>
      expect(onUpsertMcpServer).toHaveBeenCalledWith({
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

    fireEvent.change(screen.getByLabelText('Import JSON file'), {
      target: { value: '/tmp/mcp-import.json' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Import' }))

    await waitFor(() => expect(onImportMcpServers).toHaveBeenCalledWith('/tmp/mcp-import.json'))

    fireEvent.click(screen.getByRole('button', { name: 'Refresh statuses' }))
    await waitFor(() => expect(onRefreshMcpServerStatuses).toHaveBeenCalledWith({ serverIds: [] }))

    fireEvent.click(screen.getByRole('button', { name: 'Remove Memory Server' }))
    await waitFor(() => expect(onRemoveMcpServer).toHaveBeenCalledWith('memory'))
  })

  it('keeps the last truthful MCP snapshot visible when typed load or mutation errors are projected', () => {
    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          mcpRegistry: makeMcpRegistry({
            servers: [
              {
                ...makeMcpRegistry().servers[0],
                connection: {
                  status: 'failed',
                  diagnostic: {
                    code: 'mcp_probe_failed',
                    message: 'Xero could not connect to this MCP endpoint.',
                    retryable: true,
                  },
                  lastCheckedAt: '2026-04-24T05:00:00Z',
                  lastHealthyAt: '2026-04-24T04:58:00Z',
                },
              },
            ],
          }),
          mcpRegistryLoadStatus: 'error',
          mcpRegistryLoadError: makeError({
            code: 'mcp_registry_timeout',
            message: 'Xero timed out while loading app-local MCP registry.',
          }),
          mcpRegistryMutationError: makeError({
            code: 'mcp_status_refresh_failed',
            message: 'Xero could not refresh MCP server statuses.',
          }),
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'MCP' }))

    expect(screen.getByText('Xero timed out while loading app-local MCP registry.')).toBeVisible()
    expect(screen.getByText('Xero could not refresh MCP server statuses.')).toBeVisible()
    expect(screen.getByText('Memory Server')).toBeVisible()
    expect(screen.getByText('Failed')).toBeVisible()
    expect(screen.getByText('Xero could not connect to this MCP endpoint.')).toBeVisible()
  })

  it('renders the Skills registry with metadata, search, enable toggles, and remove actions', async () => {
    const onRefreshSkillRegistry = vi.fn(async () => makeSkillRegistry())
    const onSetSkillEnabled = vi.fn(async (_request: SetSkillEnabledRequest) => makeSkillRegistry())
    const onRemoveSkill = vi.fn(async (_request: RemoveSkillRequest) => makeSkillRegistry())

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          onRefreshSkillRegistry,
          onSetSkillEnabled,
          onRemoveSkill,
        })}
      />,
    )

    await waitFor(() => expect(onRefreshSkillRegistry).toHaveBeenCalledWith({ force: true }))

    fireEvent.click(screen.getByRole('button', { name: 'Skills' }))

    expect(screen.getByText('Reviewer')).toBeVisible()
    expect(screen.getByText('Release Notes')).toBeVisible()
    expect(screen.getByText('Skill has not been approved for this project.')).toBeVisible()

    fireEvent.click(screen.getAllByText('Source metadata')[0])
    expect(screen.getByText('Project skill skills/reviewer')).toBeVisible()
    expect(screen.getAllByText('skills/reviewer').length).toBeGreaterThan(0)

    fireEvent.change(screen.getByLabelText('Search skills'), { target: { value: 'release' } })

    expect(screen.queryByText('Reviewer')).not.toBeInTheDocument()
    expect(screen.getByText('Release Notes')).toBeVisible()

    fireEvent.change(screen.getByLabelText('Search skills'), { target: { value: '' } })
    fireEvent.click(screen.getByLabelText('Disable Reviewer'))

    await waitFor(() =>
      expect(onSetSkillEnabled).toHaveBeenCalledWith({
        projectId: 'project-1',
        sourceId: 'project:project-1:reviewer',
        enabled: false,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Remove Reviewer' }))
    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))

    await waitFor(() =>
      expect(onRemoveSkill).toHaveBeenCalledWith({
        projectId: 'project-1',
        sourceId: 'project:project-1:reviewer',
      }),
    )
  })

  it('validates and saves skill source management settings', async () => {
    const onUpsertSkillLocalRoot = vi.fn(async (_request: UpsertSkillLocalRootRequest) => makeSkillRegistry())
    const onRemoveSkillLocalRoot = vi.fn(async (_request: RemoveSkillLocalRootRequest) => makeSkillRegistry())
    const onUpdateProjectSkillSource = vi.fn(async (_request: UpdateProjectSkillSourceRequest) => makeSkillRegistry())
    const onUpdateGithubSkillSource = vi.fn(async (_request: UpdateGithubSkillSourceRequest) => makeSkillRegistry())

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          onUpsertSkillLocalRoot,
          onRemoveSkillLocalRoot,
          onUpdateProjectSkillSource,
          onUpdateGithubSkillSource,
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Skills' }))

    fireEvent.change(screen.getByLabelText('Local root path'), {
      target: { value: 'relative/skills' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))

    expect(screen.getByText('Use an absolute directory path.')).toBeVisible()
    expect(onUpsertSkillLocalRoot).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText('Local root path'), {
      target: { value: '/tmp/team-extra-skills' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))

    await waitFor(() =>
      expect(onUpsertSkillLocalRoot).toHaveBeenCalledWith({
        rootId: null,
        path: '/tmp/team-extra-skills',
        enabled: true,
        projectId: 'project-1',
      }),
    )

    fireEvent.click(screen.getByLabelText('Disable local skill root team-skills'))

    await waitFor(() =>
      expect(onUpsertSkillLocalRoot).toHaveBeenCalledWith({
        rootId: 'team-skills',
        path: '/tmp/xero-skills',
        enabled: false,
        projectId: 'project-1',
      }),
    )

    fireEvent.click(screen.getByLabelText('Remove local skill root team-skills'))

    await waitFor(() =>
      expect(onRemoveSkillLocalRoot).toHaveBeenCalledWith({
        rootId: 'team-skills',
        projectId: 'project-1',
      }),
    )

    fireEvent.click(screen.getByLabelText('Enable project skill discovery'))
    await waitFor(() =>
      expect(onUpdateProjectSkillSource).toHaveBeenCalledWith({
        projectId: 'project-1',
        enabled: false,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Edit source' }))

    fireEvent.change(screen.getByLabelText('GitHub skill repository'), {
      target: { value: 'acme/skills' },
    })
    fireEvent.change(screen.getByLabelText('GitHub skill reference'), {
      target: { value: 'stable' },
    })
    fireEvent.change(screen.getByLabelText('GitHub skill root'), {
      target: { value: 'catalog' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpdateGithubSkillSource).toHaveBeenCalledWith({
        repo: 'acme/skills',
        reference: 'stable',
        root: 'catalog',
        enabled: true,
        projectId: 'project-1',
      }),
    )
  })

  it('renders plugin registry metadata, commands, search, enable toggles, and remove actions', async () => {
    const pluginRegistry = makePluginSkillRegistry()
    const onRefreshSkillRegistry = vi.fn(async () => pluginRegistry)
    const onSetPluginEnabled = vi.fn(async (_request: SetPluginEnabledRequest) => pluginRegistry)
    const onRemovePlugin = vi.fn(async (_request: RemovePluginRequest) => pluginRegistry)

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          skillRegistry: pluginRegistry,
          onRefreshSkillRegistry,
          onSetPluginEnabled,
          onRemovePlugin,
        })}
      />,
    )

    await waitFor(() => expect(onRefreshSkillRegistry).toHaveBeenCalledWith({ force: true }))

    fireEvent.click(screen.getByRole('button', { name: 'Plugins' }))

    expect(screen.getByText('Acme Tools')).toBeVisible()
    expect(screen.getAllByText('Open Panel').length).toBeGreaterThan(0)
    expect(screen.getByText('Project automation helpers.')).toBeVisible()
    expect(screen.getByText(/1 .* 1 command/)).toBeVisible()
    expect(screen.getByText(/1 projected/)).toBeVisible()

    fireEvent.click(screen.getByText('Plugin metadata'))
    expect(screen.getAllByText('/tmp/xero-plugins/acme-tools').length).toBeGreaterThan(0)

    fireEvent.click(screen.getByText('Contributions'))
    expect(screen.getByText('skills/review-kit')).toBeVisible()
    expect(screen.getAllByText('commands/open-panel.js').length).toBeGreaterThan(0)

    fireEvent.change(screen.getByLabelText('Search plugins'), { target: { value: 'missing' } })
    expect(screen.queryByText('Acme Tools')).not.toBeInTheDocument()

    fireEvent.change(screen.getByLabelText('Search plugins'), { target: { value: 'acme' } })
    expect(screen.getByText('Acme Tools')).toBeVisible()

    fireEvent.click(screen.getByLabelText('Disable plugin Acme Tools'))

    await waitFor(() =>
      expect(onSetPluginEnabled).toHaveBeenCalledWith({
        projectId: 'project-1',
        pluginId: 'com.acme.tools',
        enabled: false,
      }),
    )

    fireEvent.click(screen.getByRole('button', { name: 'Remove plugin Acme Tools' }))
    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))

    await waitFor(() =>
      expect(onRemovePlugin).toHaveBeenCalledWith({
        projectId: 'project-1',
        pluginId: 'com.acme.tools',
      }),
    )
  })

  it('shows blocked plugin diagnostics and reloads plugin sources explicitly', async () => {
    const pluginRegistry = makePluginSkillRegistry()
    const blockedRegistry = makePluginSkillRegistry({
      plugins: pluginRegistry.plugins.map((plugin) => ({
        ...plugin,
        state: 'blocked',
        trust: 'blocked',
        enabled: false,
        lastDiagnostic: {
          code: 'xero_plugin_manifest_invalid',
          message: 'Xero rejected this plugin because its manifest declared unsupported fields.',
          retryable: false,
          recordedAt: '2026-04-24T05:12:00Z',
        },
      })),
      pluginCommands: pluginRegistry.pluginCommands.map((command) => ({
        ...command,
        state: 'blocked',
        trust: 'blocked',
      })),
    })
    const onReloadSkillRegistry = vi.fn(async () => blockedRegistry)

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          skillRegistry: blockedRegistry,
          onReloadSkillRegistry,
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Plugins' }))

    expect(screen.getByText('Xero rejected this plugin because its manifest declared unsupported fields.')).toBeVisible()
    expect(screen.getByLabelText('Enable plugin Acme Tools')).toBeDisabled()

    fireEvent.click(screen.getByRole('button', { name: 'Reload' }))

    await waitFor(() =>
      expect(onReloadSkillRegistry).toHaveBeenCalledWith({
        projectId: 'project-1',
        includeUnavailable: true,
      }),
    )
  })

  it('validates and saves plugin source roots', async () => {
    const pluginRegistry = makePluginSkillRegistry()
    const onUpsertPluginRoot = vi.fn(async (_request: UpsertPluginRootRequest) => pluginRegistry)
    const onRemovePluginRoot = vi.fn(async (_request: RemovePluginRootRequest) => pluginRegistry)

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          skillRegistry: pluginRegistry,
          onUpsertPluginRoot,
          onRemovePluginRoot,
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Plugins' }))

    fireEvent.change(screen.getByLabelText('Plugin root path'), {
      target: { value: 'relative/plugins' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))

    expect(screen.getByText('Use an absolute directory path.')).toBeVisible()
    expect(onUpsertPluginRoot).not.toHaveBeenCalled()

    fireEvent.change(screen.getByLabelText('Plugin root path'), {
      target: { value: '/tmp/extra-plugins' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Add' }))

    await waitFor(() =>
      expect(onUpsertPluginRoot).toHaveBeenCalledWith({
        rootId: null,
        path: '/tmp/extra-plugins',
        enabled: true,
        projectId: 'project-1',
      }),
    )

    fireEvent.click(screen.getByLabelText('Disable plugin root team-plugins'))

    await waitFor(() =>
      expect(onUpsertPluginRoot).toHaveBeenCalledWith({
        rootId: 'team-plugins',
        path: '/tmp/xero-plugins',
        enabled: false,
        projectId: 'project-1',
      }),
    )

    fireEvent.click(screen.getByLabelText('Remove plugin root team-plugins'))

    await waitFor(() =>
      expect(onRemovePluginRoot).toHaveBeenCalledWith({
        rootId: 'team-plugins',
        projectId: 'project-1',
      }),
    )
  })

  it('keeps the last truthful skill registry visible when typed load or mutation errors are projected', () => {
    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          skillRegistryLoadStatus: 'error',
          skillRegistryLoadError: makeError({
            code: 'skill_registry_failed',
            message: 'Xero could not load app-local skill sources.',
          }),
          skillRegistryMutationError: makeError({
            code: 'skill_source_path_unsafe',
            message: 'Xero requires local skill directories to use absolute paths.',
          }),
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'Skills' }))

    expect(screen.getByText('Xero could not load app-local skill sources.')).toBeVisible()
    expect(screen.getByText('Xero requires local skill directories to use absolute paths.')).toBeVisible()
    expect(screen.getByText('Reviewer')).toBeVisible()
    expect(screen.getByText('Release Notes')).toBeVisible()
  })

})
