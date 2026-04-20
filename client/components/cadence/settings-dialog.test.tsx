import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

const { openUrlMock } = vi.hoisted(() => ({
  openUrlMock: vi.fn(),
}))

vi.mock('@tauri-apps/plugin-opener', () => ({
  openUrl: openUrlMock,
}))

import { SettingsDialog } from '@/components/cadence/settings-dialog'
import type { AgentPaneView, OperatorActionErrorView } from '@/src/features/cadence/use-cadence-desktop-state'
import type { RuntimeSessionView, RuntimeSettingsDto, UpsertRuntimeSettingsRequestDto } from '@/src/lib/cadence-model'

function makeRuntimeSettings(overrides: Partial<RuntimeSettingsDto> = {}): RuntimeSettingsDto {
  return {
    providerId: 'openrouter',
    modelId: 'openai/gpt-4.1-mini',
    openrouterApiKeyConfigured: false,
    ...overrides,
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

function makeAgent(overrides: Partial<AgentPaneView> = {}): AgentPaneView {
  return {
    project: {
      id: 'project-1',
      name: 'cadence',
      repository: {
        rootPath: '/tmp/cadence',
      },
    } as AgentPaneView['project'],
    activePhase: null,
    branchLabel: 'main',
    headShaLabel: 'abc123',
    runtimeLabel: 'Openai Codex · Signed out',
    repositoryLabel: 'cadence',
    repositoryPath: '/tmp/cadence',
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
    code: 'runtime_settings_failed',
    message: 'Cadence could not load app-global runtime settings.',
    retryable: true,
    ...overrides,
  }
}

describe('SettingsDialog', () => {
  it('refreshes app-global settings on open and keeps notifications project-bound when no project is selected', async () => {
    const onRefreshRuntimeSettings = vi.fn(async () => makeRuntimeSettings())

    render(
      <SettingsDialog
        open
        onOpenChange={vi.fn()}
        agent={null}
        runtimeSettings={makeRuntimeSettings()}
        runtimeSettingsLoadStatus="ready"
        runtimeSettingsLoadError={null}
        runtimeSettingsSaveStatus="idle"
        runtimeSettingsSaveError={null}
        onRefreshRuntimeSettings={onRefreshRuntimeSettings}
      />,
    )

    await waitFor(() => expect(onRefreshRuntimeSettings).toHaveBeenCalledWith({ force: true }))
    expect(
      screen.getByText('Configure the app-global runtime provider, model, and OpenRouter key without requiring a selected project.'),
    ).toBeVisible()

    fireEvent.click(screen.getByRole('button', { name: 'Notifications' }))

    expect(screen.getByText('Notifications require a selected project')).toBeVisible()
    expect(
      screen.getByText(
        'Provider settings are app-global, but notification routes stay project-bound so Cadence never writes cross-project delivery state into the wrong repository view.',
      ),
    ).toBeVisible()
  })

  it('keeps an OpenRouter-specific model draft and never echoes a saved API key back into the dialog state', async () => {
    const secret = 'sk-or-v1-test-secret'
    const onRefreshRuntimeSettings = vi.fn(async () => makeRuntimeSettings({ providerId: 'openai_codex', modelId: 'openai_codex' }))

    let nextRuntimeSettings = makeRuntimeSettings({
      providerId: 'openai_codex',
      modelId: 'openai_codex',
      openrouterApiKeyConfigured: false,
    })

    const onUpsertRuntimeSettings = vi.fn(async (request: UpsertRuntimeSettingsRequestDto) => {
      nextRuntimeSettings = {
        providerId: request.providerId,
        modelId: request.modelId,
        openrouterApiKeyConfigured: Boolean(request.openrouterApiKey?.trim()),
      }

      return nextRuntimeSettings
    })

    const { rerender } = render(
      <SettingsDialog
        open
        onOpenChange={vi.fn()}
        agent={makeAgent()}
        runtimeSettings={nextRuntimeSettings}
        runtimeSettingsLoadStatus="ready"
        runtimeSettingsLoadError={null}
        runtimeSettingsSaveStatus="idle"
        runtimeSettingsSaveError={null}
        onRefreshRuntimeSettings={onRefreshRuntimeSettings}
        onUpsertRuntimeSettings={onUpsertRuntimeSettings}
      />,
    )

    await waitFor(() => expect(onRefreshRuntimeSettings).toHaveBeenCalledWith({ force: true }))

    fireEvent.click(screen.getByRole('button', { name: 'Use provider' }))

    const modelInput = screen.getByLabelText('Model ID') as HTMLInputElement
    const keyInput = screen.getByLabelText('OpenRouter API key') as HTMLInputElement

    expect(modelInput).toHaveValue('')
    expect(screen.getByText('OpenRouter needs a saved key')).toBeVisible()

    fireEvent.change(modelInput, { target: { value: 'openrouter/anthropic/claude-3.5-sonnet' } })
    fireEvent.change(keyInput, { target: { value: secret } })

    await waitFor(() => expect(screen.queryByText('OpenRouter needs a saved key')).not.toBeInTheDocument())

    fireEvent.click(screen.getByRole('button', { name: 'Save provider settings' }))

    await waitFor(() =>
      expect(onUpsertRuntimeSettings).toHaveBeenCalledWith({
        providerId: 'openrouter',
        modelId: 'openrouter/anthropic/claude-3.5-sonnet',
        openrouterApiKey: secret,
      }),
    )
    await waitFor(() => expect(keyInput).toHaveValue(''))
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()

    rerender(
      <SettingsDialog
        open
        onOpenChange={vi.fn()}
        agent={makeAgent()}
        runtimeSettings={nextRuntimeSettings}
        runtimeSettingsLoadStatus="ready"
        runtimeSettingsLoadError={null}
        runtimeSettingsSaveStatus="idle"
        runtimeSettingsSaveError={null}
        onRefreshRuntimeSettings={onRefreshRuntimeSettings}
        onUpsertRuntimeSettings={onUpsertRuntimeSettings}
      />,
    )

    await waitFor(() => expect(screen.getByDisplayValue('openrouter/anthropic/claude-3.5-sonnet')).toBeVisible())
    expect(screen.getByText('Configured')).toBeVisible()
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()
    expect(screen.queryByText(secret)).not.toBeInTheDocument()
  })

  it('keeps the last truthful provider snapshot visible when a typed load error is present', () => {
    render(
      <SettingsDialog
        open
        onOpenChange={vi.fn()}
        agent={makeAgent()}
        runtimeSettings={makeRuntimeSettings({
          providerId: 'openrouter',
          modelId: 'openrouter/meta-llama/llama-3.1-8b-instruct',
          openrouterApiKeyConfigured: true,
        })}
        runtimeSettingsLoadStatus="error"
        runtimeSettingsLoadError={makeError({
          code: 'runtime_settings_timeout',
          message: 'Cadence timed out while loading app-global runtime settings.',
        })}
        runtimeSettingsSaveStatus="idle"
        runtimeSettingsSaveError={null}
        onRefreshRuntimeSettings={vi.fn(async () => makeRuntimeSettings())}
      />,
    )

    expect(screen.getByText('Settings load failed')).toBeVisible()
    expect(screen.getByText('Cadence timed out while loading app-global runtime settings.')).toBeVisible()
    expect(screen.getByDisplayValue('openrouter/meta-llama/llama-3.1-8b-instruct')).toBeVisible()
    expect(screen.getByText('Configured')).toBeVisible()
  })
})
