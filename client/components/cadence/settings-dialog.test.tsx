import { fireEvent, render, screen, waitFor } from '@testing-library/react'
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
  ProviderProfileDto,
  ProviderProfilesDto,
  RuntimeSessionView,
  UpsertProviderProfileRequestDto,
} from '@/src/lib/cadence-model'

type NotificationRouteRequest = Parameters<NonNullable<SettingsDialogProps['onUpsertNotificationRoute']>>[0]

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

function makeProviderProfiles(overrides: Partial<ProviderProfilesDto> = {}): ProviderProfilesDto {
  return {
    activeProfileId: overrides.activeProfileId ?? 'openai_codex-default',
    profiles:
      overrides.profiles ?? [makeOpenAiProfile(), makeOpenRouterProfile({ active: false })],
    migration: overrides.migration ?? null,
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
    onRefreshProviderProfiles: vi.fn(async () => makeProviderProfiles()),
    onUpsertProviderProfile: vi.fn(async (_request: UpsertProviderProfileRequestDto) => makeProviderProfiles()),
    onSetActiveProviderProfile: vi.fn(async (_profileId: string) => makeProviderProfiles()),
    onStartLogin: vi.fn(async () => makeRuntimeSession()),
    onLogout: vi.fn(async () => makeRuntimeSession({ sessionId: null, accountId: null })),
    ...overrides,
  }
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
      screen.getByText(
        'Manage app-local provider profiles, readiness, and active selection. Projects are not assigned to a provider here.',
      ),
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

    fireEvent.click(screen.getByRole('button', { name: 'Set up' }))

    const modelInput = screen.getByLabelText('Model ID') as HTMLInputElement
    const keyInput = screen.getByLabelText('API Key') as HTMLInputElement

    expect(modelInput).toHaveValue('openai/gpt-4.1-mini')
    expect(keyInput).toHaveValue('')

    fireEvent.change(modelInput, { target: { value: 'openrouter/anthropic/claude-3.5-sonnet' } })
    fireEvent.change(keyInput, { target: { value: secret } })
    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertProviderProfile).toHaveBeenCalledWith({
        profileId: 'openrouter-default',
        providerId: 'openrouter',
        label: 'OpenRouter',
        modelId: 'openrouter/anthropic/claude-3.5-sonnet',
        openrouterApiKey: secret,
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
    fireEvent.click(screen.getByRole('button', { name: 'Edit setup' }))

    const modelInputAfter = screen.getByLabelText('Model ID') as HTMLInputElement
    const keyInputAfter = screen.getByLabelText('API Key') as HTMLInputElement

    expect(modelInputAfter).toHaveValue('openrouter/anthropic/claude-3.5-sonnet')
    expect(keyInputAfter).toHaveValue('')
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()
    expect(screen.queryByText(secret)).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }))
    fireEvent.click(screen.getByRole('button', { name: 'Use this profile' }))

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

    expect(screen.getByText('Active profile')).toBeVisible()
    expect(screen.getByText('Using this')).toBeVisible()
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

    fireEvent.click(screen.getByRole('button', { name: 'Edit setup' }))

    expect(screen.getByDisplayValue('openrouter/meta-llama/llama-3.1-8b-instruct')).toBeVisible()
    expect(screen.getByText('Ready')).toBeVisible()
  })
})
