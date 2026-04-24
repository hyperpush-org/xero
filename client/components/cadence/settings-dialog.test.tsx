import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

const { openUrlMock } = vi.hoisted(() => ({
  openUrlMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

import { SettingsDialog, type SettingsDialogProps } from '@/components/cadence/settings-dialog'
import type { AgentPaneView, OperatorActionErrorView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  McpRegistryDto,
  ProviderModelCatalogDto,
  ProviderProfileDto,
  ProviderProfilesDto,
  RuntimeSessionView,
  UpsertProviderProfileRequestDto,
} from '@/src/lib/cadence-model'

type NotificationRouteRequest = Parameters<NonNullable<SettingsDialogProps['onUpsertNotificationRoute']>>[0]
type McpUpsertRequest = Parameters<NonNullable<SettingsDialogProps['onUpsertMcpServer']>>[0]

function makeOpenAiProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  return {
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
    ...overrides,
  }
}

function makeOpenRouterProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  const ready = overrides.readiness?.ready ?? false

  return {
    profileId: 'openrouter-default',
    providerId: 'openrouter',
    label: 'OpenRouter',
    modelId: 'openai/gpt-4.1-mini',
    active: false,
    readiness: ready
      ? {
          ready: true,
          status: 'ready',
          credentialUpdatedAt: '2026-04-20T00:00:00Z',
        }
      : {
          ready: false,
          status: 'missing',
          credentialUpdatedAt: null,
        },
    migratedFromLegacy: false,
    migratedAt: null,
    ...overrides,
  }
}

function makeAnthropicProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  const ready = overrides.readiness?.ready ?? false

  return {
    profileId: 'anthropic-default',
    providerId: 'anthropic',
    label: 'Anthropic',
    modelId: 'claude-3-7-sonnet-latest',
    active: false,
    readiness: ready
      ? {
          ready: true,
          status: 'ready',
          credentialUpdatedAt: '2026-04-20T00:00:00Z',
        }
      : {
          ready: false,
          status: 'missing',
          credentialUpdatedAt: null,
        },
    migratedFromLegacy: false,
    migratedAt: null,
    ...overrides,
  }
}

function makeGithubProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  const ready = overrides.readiness?.ready ?? false

  return {
    profileId: 'github_models-default',
    providerId: 'github_models',
    label: 'GitHub Models',
    modelId: 'openai/gpt-4.1',
    active: false,
    readiness: ready
      ? {
          ready: true,
          status: 'ready',
          credentialUpdatedAt: '2026-04-20T00:00:00Z',
        }
      : {
          ready: false,
          status: 'missing',
          credentialUpdatedAt: null,
        },
    migratedFromLegacy: false,
    migratedAt: null,
    ...overrides,
  }
}

function makeOpenAiApiProfile(overrides: Partial<ProviderProfileDto> = {}): ProviderProfileDto {
  const ready = overrides.readiness?.ready ?? false

  return {
    profileId: 'openai_api-default',
    providerId: 'openai_api',
    label: 'OpenAI-compatible',
    modelId: 'gpt-4.1-mini',
    active: false,
    baseUrl: null,
    apiVersion: null,
    readiness: ready
      ? {
          ready: true,
          status: 'ready',
          credentialUpdatedAt: '2026-04-20T00:00:00Z',
        }
      : {
          ready: false,
          status: 'missing',
          credentialUpdatedAt: null,
        },
    migratedFromLegacy: false,
    migratedAt: null,
    ...overrides,
  }
}

function makeProviderProfiles(overrides: Partial<ProviderProfilesDto> = {}): ProviderProfilesDto {
  return {
    activeProfileId: overrides.activeProfileId ?? 'openai_codex-default',
    profiles:
      overrides.profiles ?? [makeOpenAiProfile(), makeOpenRouterProfile({ active: false })],
    migration: overrides.migration ?? null,
  }
}

function makeProviderModelCatalog(
  profileId: string,
  overrides: Partial<ProviderModelCatalogDto> = {},
): ProviderModelCatalogDto {
  const providerId =
    overrides.providerId ??
    (profileId.startsWith('openrouter')
      ? 'openrouter'
      : profileId.startsWith('anthropic')
        ? 'anthropic'
        : profileId.startsWith('github_models')
          ? 'github_models'
          : profileId.startsWith('openai_api')
            ? 'openai_api'
            : 'openai_codex')
  const configuredModelId =
    overrides.configuredModelId ??
    (providerId === 'openrouter'
      ? 'openai/gpt-4.1-mini'
      : providerId === 'anthropic'
        ? 'claude-3-7-sonnet-latest'
        : providerId === 'github_models'
          ? 'openai/gpt-4.1'
          : providerId === 'openai_api'
            ? 'gpt-4.1-mini'
            : 'openai_codex')

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
              modelId: 'openai/gpt-4.1-mini',
              displayName: 'OpenAI GPT-4.1 Mini',
              thinking: {
                supported: true,
                effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                defaultEffort: 'medium',
              },
            },
            {
              modelId: 'openrouter/anthropic/claude-3.5-sonnet',
              displayName: 'Claude 3.5 Sonnet',
              thinking: {
                supported: true,
                effortOptions: ['low', 'medium', 'high'],
                defaultEffort: 'medium',
              },
            },
          ]
        : providerId === 'anthropic'
          ? [
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
          : providerId === 'github_models'
            ? [
                {
                  modelId: 'openai/gpt-4.1',
                  displayName: 'OpenAI GPT-4.1',
                  thinking: {
                    supported: true,
                    effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                    defaultEffort: 'medium',
                  },
                },
              ]
            : providerId === 'openai_api'
              ? [
                  {
                    modelId: 'gpt-4.1-mini',
                    displayName: 'GPT-4.1 Mini',
                    thinking: {
                      supported: true,
                      effortOptions: ['minimal', 'low', 'medium', 'high', 'x_high'],
                      defaultEffort: 'medium',
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

function makeAgent(overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  return {
    project: {
      id: 'project-1',
      name: 'Cadence',
      repository: {
        rootPath: '/tmp/Cadence',
      },
    } as AgentPaneView['project'],
    activePhase: null,
    branchLabel: 'main',
    headShaLabel: 'abc123',
    runtimeLabel: 'Openai Codex · Signed out',
    repositoryLabel: 'Cadence',
    repositoryPath: '/tmp/Cadence',
    runtimeSession: makeRuntimeSession(),
    runtimeRun: null,
    autonomousRun: null,
    autonomousUnit: null,
    autonomousAttempt: null,
    autonomousWorkflowContext: null,
    autonomousHistory: [],
    autonomousRecentArtifacts: [],
    recentAutonomousUnits: undefined,
    checkpointControlLoop: undefined,
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
    message: 'Cadence could not load app-local provider profiles.',
    retryable: true,
    ...overrides,
  }
}

function makeSettingsDialogProps(overrides: Partial<SettingsDialogProps> = {}): SettingsDialogProps {
  return {
    open: true,
    onOpenChange: vi.fn(),
    agent: makeAgent(),
    providerProfiles: makeProviderProfiles(),
    providerProfilesLoadStatus: 'ready',
    providerProfilesLoadError: null,
    providerProfilesSaveStatus: 'idle',
    providerProfilesSaveError: null,
    providerModelCatalogs: {
      'openai_codex-default': makeProviderModelCatalog('openai_codex-default'),
      'openrouter-default': makeProviderModelCatalog('openrouter-default'),
    },
    providerModelCatalogLoadStatuses: {
      'openai_codex-default': 'ready',
      'openrouter-default': 'ready',
    },
    providerModelCatalogLoadErrors: {
      'openai_codex-default': null,
      'openrouter-default': null,
    },
    onRefreshProviderProfiles: vi.fn(async () => makeProviderProfiles()),
    onRefreshProviderModelCatalog: vi.fn(async (profileId: string) => makeProviderModelCatalog(profileId)),
    onUpsertProviderProfile: vi.fn(async (_request: UpsertProviderProfileRequestDto) => makeProviderProfiles()),
    onSetActiveProviderProfile: vi.fn(async (_profileId: string) => makeProviderProfiles()),
    onStartLogin: vi.fn(async () => makeRuntimeSession()),
    onLogout: vi.fn(async () => makeRuntimeSession({ sessionId: null, accountId: null })),
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
    ...overrides,
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

describe('SettingsDialog', () => {
  it('refreshes app-local provider profiles on open and keeps notifications project-bound when no project is selected', async () => {
    const onRefreshProviderProfiles = vi.fn(async () => makeProviderProfiles())

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          agent: null,
          onRefreshProviderProfiles,
        })}
      />,
    )

    await waitFor(() => expect(onRefreshProviderProfiles).toHaveBeenCalledWith({ force: true }))
    expect(
      screen.getByText('Pick a provider, manage its API key, and choose a model.'),
    ).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))

    expect(screen.getByText('Notifications require a selected project')).toBeVisible()
    expect(
      screen.getByText(
        'Provider settings are app-global, but notification routes stay project-bound so Cadence never writes cross-project delivery state into the wrong repository view.',
      ),
    ).toBeVisible()
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

  it('keeps provider profile secrets blank on re-open and switches the active profile explicitly', async () => {
    const secret = 'sk-or-v1-test-secret'

    let nextProviderProfiles = makeProviderProfiles({
      activeProfileId: 'openai_codex-default',
      profiles: [makeOpenAiProfile({ active: true }), makeOpenRouterProfile({ active: false })],
    })

    const onUpsertProviderProfile = vi.fn(async (request: UpsertProviderProfileRequestDto) => {
      nextProviderProfiles = makeProviderProfiles({
        activeProfileId: 'openai_codex-default',
        profiles: [
          makeOpenAiProfile({ active: true }),
          makeOpenRouterProfile({
            active: false,
            modelId: request.modelId,
            label: request.label,
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
        ],
      })

      return nextProviderProfiles
    })

    const onSetActiveProviderProfile = vi.fn(async (_profileId: string) => {
      nextProviderProfiles = makeProviderProfiles({
        activeProfileId: 'openrouter-default',
        profiles: [makeOpenAiProfile({ active: false }), makeOpenRouterProfile({ active: true, readiness: { ready: true, status: 'ready', credentialUpdatedAt: '2026-04-20T12:00:00Z' } })],
      })

      return nextProviderProfiles
    })

    const { rerender } = render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
          onSetActiveProviderProfile,
        })}
      />,
    )

    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'Set up' }))

    const modelSelector = screen.getByLabelText('Model')
    const keyInput = screen.getByLabelText('API Key') as HTMLInputElement

    expect(modelSelector).toHaveTextContent('OpenAI GPT-4.1 Mini · openai/gpt-4.1-mini')
    expect(keyInput).toHaveValue('')

    fireEvent.keyDown(modelSelector, { key: 'ArrowDown' })
    fireEvent.click(await screen.findByRole('option', { name: 'Claude 3.5 Sonnet · openrouter/anthropic/claude-3.5-sonnet' }))
    fireEvent.change(keyInput, { target: { value: secret } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'openrouter-default',
        providerId: 'openrouter',
        runtimeKind: 'openrouter',
        label: 'OpenRouter',
        modelId: 'openrouter/anthropic/claude-3.5-sonnet',
        presetId: 'openrouter',
        baseUrl: null,
        apiVersion: null,
        region: null,
        projectId: null,
        apiKey: secret,
        activate: false,
      }),
    )

    rerender(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
          onSetActiveProviderProfile,
        })}
      />,
    )

    expect(screen.getByText('Ready')).toBeVisible()
    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'Edit' }))

    const modelSelectorAfter = screen.getByLabelText('Model')
    const keyInputAfter = screen.getByLabelText('API Key') as HTMLInputElement

    expect(modelSelectorAfter).toHaveTextContent('Claude 3.5 Sonnet · openrouter/anthropic/claude-3.5-sonnet')
    expect(keyInputAfter).toHaveValue('')
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()
    expect(screen.queryByText(secret)).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'Use this' }))

    await waitFor(() => expect(onSetActiveProviderProfile).toHaveBeenCalledWith('openrouter-default'))

    rerender(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
          onSetActiveProviderProfile,
        })}
      />,
    )

    expect(screen.getAllByText('Active').length).toBeGreaterThan(0)
  })

  it('lets Anthropic use the shared API-key save, edit, and activate flow', async () => {
    const secret = 'sk-ant-test-secret'

    let nextProviderProfiles = makeProviderProfiles({
      activeProfileId: 'openai_codex-default',
      profiles: [makeOpenAiProfile({ active: true }), makeOpenRouterProfile({ active: false })],
    })

    const onUpsertProviderProfile = vi.fn(async (request: UpsertProviderProfileRequestDto) => {
      nextProviderProfiles = makeProviderProfiles({
        activeProfileId: 'openai_codex-default',
        profiles: [
          makeOpenAiProfile({ active: true }),
          makeOpenRouterProfile({ active: false }),
          makeAnthropicProfile({
            active: false,
            label: request.label,
            modelId: request.modelId,
            readiness: request.apiKey === ''
              ? {
                  ready: false,
                  status: 'missing',
                  credentialUpdatedAt: null,
                }
              : {
                  ready: true,
                  status: 'ready',
                  credentialUpdatedAt: '2026-04-20T12:00:00Z',
                },
          }),
        ],
      })

      return nextProviderProfiles
    })

    const onSetActiveProviderProfile = vi.fn(async (_profileId: string) => {
      nextProviderProfiles = makeProviderProfiles({
        activeProfileId: 'anthropic-default',
        profiles: [
          makeOpenAiProfile({ active: false }),
          makeOpenRouterProfile({ active: false }),
          makeAnthropicProfile({
            active: true,
            label: 'Anthropic Work',
            readiness: {
              ready: true,
              status: 'ready',
              credentialUpdatedAt: '2026-04-20T12:00:00Z',
            },
          }),
        ],
      })

      return nextProviderProfiles
    })

    const { rerender } = render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
          onSetActiveProviderProfile,
        })}
      />,
    )

    fireEvent.click(within(getProviderCard('Anthropic')).getByRole('button', { name: 'Set up' }))

    const labelInput = screen.getByLabelText('Profile label') as HTMLInputElement
    const modelSelector = screen.getByLabelText('Model')
    const keyInput = screen.getByLabelText('API Key') as HTMLInputElement

    expect(labelInput).toHaveValue('Anthropic')
    expect(modelSelector).toHaveTextContent('claude-3-7-sonnet-latest')
    expect(keyInput).toHaveValue('')

    fireEvent.change(labelInput, { target: { value: '   ' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByText('Profile label is required.')).toBeVisible()

    fireEvent.change(labelInput, { target: { value: 'Anthropic Work' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByText('Anthropic requires an API key.')).toBeVisible()

    fireEvent.change(keyInput, { target: { value: secret } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'anthropic-default',
        providerId: 'anthropic',
        runtimeKind: 'anthropic',
        label: 'Anthropic Work',
        modelId: 'claude-3-7-sonnet-latest',
        presetId: 'anthropic',
        baseUrl: null,
        apiVersion: null,
        region: null,
        projectId: null,
        apiKey: secret,
        activate: false,
      }),
    )

    rerender(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
          onSetActiveProviderProfile,
        })}
      />,
    )

    expect(screen.getByText('Ready')).toBeVisible()
    fireEvent.click(within(getProviderCard('Anthropic Work')).getByRole('button', { name: 'Edit' }))

    const keyInputAfter = screen.getByLabelText('API Key') as HTMLInputElement

    expect(keyInputAfter).toHaveValue('')
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()
    expect(screen.queryByText(secret)).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    fireEvent.click(within(getProviderCard('Anthropic Work')).getByRole('button', { name: 'Use this' }))

    await waitFor(() => expect(onSetActiveProviderProfile).toHaveBeenCalledWith('anthropic-default'))
  })


  it('lets GitHub Models use the shared generic save flow and keeps tokens redacted on edit', async () => {
    const secret = 'ghp_test_secret'

    let nextProviderProfiles = makeProviderProfiles({
      activeProfileId: 'openai_codex-default',
      profiles: [makeOpenAiProfile({ active: true }), makeOpenRouterProfile({ active: false })],
    })

    const onUpsertProviderProfile = vi.fn(async (request: UpsertProviderProfileRequestDto) => {
      nextProviderProfiles = makeProviderProfiles({
        activeProfileId: 'openai_codex-default',
        profiles: [
          makeOpenAiProfile({ active: true }),
          makeOpenRouterProfile({ active: false }),
          makeGithubProfile({
            active: false,
            label: request.label,
            modelId: request.modelId,
            readiness: request.apiKey === ''
              ? {
                  ready: false,
                  status: 'missing',
                  credentialUpdatedAt: null,
                }
              : {
                  ready: true,
                  status: 'ready',
                  credentialUpdatedAt: '2026-04-20T12:00:00Z',
                },
          }),
        ],
      })

      return nextProviderProfiles
    })

    const { rerender } = render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
        })}
      />,
    )

    fireEvent.click(within(getProviderCard('GitHub Models')).getByRole('button', { name: 'Set up' }))

    const labelInput = screen.getByLabelText('Profile label') as HTMLInputElement
    const modelInput = screen.getByLabelText('Model') as HTMLInputElement
    const keyInput = screen.getByLabelText('API Key') as HTMLInputElement

    expect(labelInput).toHaveValue('GitHub Models')
    expect(keyInput).toHaveValue('')

    fireEvent.change(labelInput, { target: { value: 'GitHub Work' } })
    fireEvent.change(modelInput, { target: { value: 'meta/Llama-4-Scout-17B-16E-Instruct' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByText('GitHub Models requires an API key.')).toBeVisible()

    fireEvent.change(keyInput, { target: { value: secret } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'github_models-default',
        providerId: 'github_models',
        runtimeKind: 'openai_compatible',
        label: 'GitHub Work',
        modelId: 'openai/gpt-4.1',
        presetId: 'github_models',
        baseUrl: null,
        apiVersion: null,
        region: null,
        projectId: null,
        apiKey: secret,
        activate: false,
      }),
    )

    rerender(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
        })}
      />,
    )

    expect(screen.getByText('Ready')).toBeVisible()
    fireEvent.click(within(getProviderCard('GitHub Work')).getByRole('button', { name: 'Edit' }))

    const keyInputAfter = screen.getByLabelText('API Key') as HTMLInputElement
    expect(keyInputAfter).toHaveValue('')
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()
    expect(screen.queryByText(secret)).not.toBeInTheDocument()
  })

  it('lets OpenAI-compatible use the shared generic save flow and preserves connection metadata on edit', async () => {
    const secret = 'sk-openai-api-test-secret'

    let nextProviderProfiles = makeProviderProfiles({
      activeProfileId: 'openai_codex-default',
      profiles: [makeOpenAiProfile({ active: true }), makeOpenRouterProfile({ active: false })],
    })

    const onUpsertProviderProfile = vi.fn(async (request: UpsertProviderProfileRequestDto) => {
      nextProviderProfiles = makeProviderProfiles({
        activeProfileId: 'openai_codex-default',
        profiles: [
          makeOpenAiProfile({ active: true }),
          makeOpenRouterProfile({ active: false }),
          makeOpenAiApiProfile({
            active: false,
            label: request.label,
            modelId: request.modelId,
            baseUrl: request.baseUrl,
            apiVersion: request.apiVersion,
            readiness: request.apiKey === ''
              ? {
                  ready: false,
                  status: 'missing',
                  credentialUpdatedAt: null,
                }
              : {
                  ready: true,
                  status: 'ready',
                  credentialUpdatedAt: '2026-04-20T12:00:00Z',
                },
          }),
        ],
      })

      return nextProviderProfiles
    })

    const { rerender } = render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
        })}
      />,
    )

    fireEvent.click(within(getProviderCard('OpenAI-compatible')).getByRole('button', { name: 'Set up' }))

    const labelInput = screen.getByLabelText('Profile label') as HTMLInputElement
    const modelInput = screen.getByLabelText('Model') as HTMLInputElement
    const baseUrlInput = screen.getByLabelText('Base URL') as HTMLInputElement
    const apiVersionInput = screen.getByLabelText('API version') as HTMLInputElement
    const keyInput = screen.getByLabelText('API Key') as HTMLInputElement

    expect(labelInput).toHaveValue('OpenAI-compatible')
    expect(keyInput).toHaveValue('')

    fireEvent.change(labelInput, { target: { value: 'OpenAI Work' } })
    fireEvent.change(modelInput, { target: { value: 'gpt-4.1-mini' } })
    fireEvent.change(baseUrlInput, { target: { value: 'https://api.openai.example/v1' } })
    fireEvent.change(apiVersionInput, { target: { value: '2024-10-21' } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))
    expect(screen.getByText('OpenAI-compatible requires an API key.')).toBeVisible()

    fireEvent.change(keyInput, { target: { value: secret } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'openai_api-default',
        providerId: 'openai_api',
        runtimeKind: 'openai_compatible',
        label: 'OpenAI Work',
        modelId: 'gpt-4.1-mini',
        presetId: 'openai_api',
        baseUrl: 'https://api.openai.example/v1',
        apiVersion: '2024-10-21',
        region: null,
        projectId: null,
        apiKey: secret,
        activate: false,
      }),
    )

    rerender(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: nextProviderProfiles,
          onUpsertProviderProfile,
        })}
      />,
    )

    expect(screen.getByText('Ready')).toBeVisible()
    fireEvent.click(within(getProviderCard('OpenAI Work')).getByRole('button', { name: 'Edit' }))

    expect((screen.getByLabelText('Base URL') as HTMLInputElement).value).toBe('https://api.openai.example/v1')
    expect((screen.getByLabelText('API version') as HTMLInputElement).value).toBe('2024-10-21')
    expect((screen.getByLabelText('API Key') as HTMLInputElement).value).toBe('')
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()
    expect(screen.queryByText(secret)).not.toBeInTheDocument()
  })

  it('limits OpenAI auth controls to the selected profile and keeps typed auth failures inline', async () => {
    const onStartLogin = vi.fn(async () => {
      throw new Error(
        'Cadence rejected auth flow `flow-1` because it was started for provider profile `openai_codex-default` instead of the selected profile `zz-openai-alt`. Retry login for the currently selected profile.',
      )
    })

    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          agent: makeAgent({
            runtimeSession: makeRuntimeSession(),
          }),
          providerProfiles: makeProviderProfiles({
            activeProfileId: 'zz-openai-alt',
            profiles: [
              makeOpenAiProfile({ active: false }),
              makeOpenAiProfile({
                profileId: 'zz-openai-alt',
                label: 'OpenAI Alt',
                active: true,
              }),
              makeOpenRouterProfile({ active: false, migratedFromLegacy: false, migratedAt: null }),
            ],
          }),
          onStartLogin,
        })}
      />,
    )

    expect(screen.getByRole('button', { name: 'Sign in' })).toBeVisible()
    expect(screen.getAllByRole('button', { name: 'Sign in' })).toHaveLength(1)

    fireEvent.click(screen.getByRole('button', { name: 'Sign in' }))

    await waitFor(() => expect(onStartLogin).toHaveBeenCalledTimes(1))
    expect(
      screen.getByText(
        'Cadence rejected auth flow `flow-1` because it was started for provider profile `openai_codex-default` instead of the selected profile `zz-openai-alt`. Retry login for the currently selected profile.',
      ),
    ).toBeVisible()
    expect(screen.getByText('OpenAI Alt')).toBeVisible()
    expect(screen.getAllByText('Active').length).toBeGreaterThan(0)
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

    fireEvent.click(screen.getByRole('button', { name: 'Remove' }))
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
                    message: 'Cadence could not connect to this MCP endpoint.',
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
            message: 'Cadence timed out while loading app-local MCP registry.',
          }),
          mcpRegistryMutationError: makeError({
            code: 'mcp_status_refresh_failed',
            message: 'Cadence could not refresh MCP server statuses.',
          }),
        })}
      />,
    )

    fireEvent.click(screen.getByRole('button', { name: 'MCP' }))

    expect(screen.getByText('Cadence timed out while loading app-local MCP registry.')).toBeVisible()
    expect(screen.getByText('Cadence could not refresh MCP server statuses.')).toBeVisible()
    expect(screen.getByText('Memory Server')).toBeVisible()
    expect(screen.getByText('Failed')).toBeVisible()
    expect(screen.getByText('Cadence could not connect to this MCP endpoint.')).toBeVisible()
  })

  it('keeps the last truthful provider snapshot visible when a typed load error is present', () => {
    render(
      <SettingsDialog
        {...makeSettingsDialogProps({
          providerProfiles: makeProviderProfiles({
            activeProfileId: 'openrouter-default',
            profiles: [
              makeOpenAiProfile({ active: false }),
              makeOpenRouterProfile({
                active: true,
                modelId: 'openrouter/meta-llama/llama-3.1-8b-instruct',
                readiness: {
                  ready: true,
                  status: 'ready',
                  credentialUpdatedAt: '2026-04-20T00:00:00Z',
                },
              }),
            ],
          }),
          providerProfilesLoadStatus: 'error',
          providerProfilesLoadError: makeError({
            code: 'provider_profiles_timeout',
            message: 'Cadence timed out while loading app-local provider profiles.',
          }),
        })}
      />,
    )

    expect(screen.getByText('Cadence timed out while loading app-local provider profiles.')).toBeVisible()
    expect(screen.getByText('OpenRouter')).toBeVisible()
    expect(screen.getByText('OpenAI Codex')).toBeVisible()

    fireEvent.click(within(getProviderCard('OpenRouter')).getByRole('button', { name: 'Edit' }))

    expect(screen.getByLabelText('Model')).toHaveTextContent('openrouter/meta-llama/llama-3.1-8b-instruct')
    expect(screen.getByText('Ready')).toBeVisible()
  })
})
