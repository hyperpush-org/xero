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
      screen.getByText('Configure AI model providers for Cadence'),
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
        onRefreshRuntimeSettings={vi.fn(async () => makeRuntimeSettings())}
        onUpsertRuntimeSettings={onUpsertRuntimeSettings}
      />,
    )

    // Click Configure button on OpenRouter card
    const configureButton = screen.getAllByRole('button', { name: 'Configure' })[0]
    fireEvent.click(configureButton)

    const modelInput = screen.getByLabelText('Model ID') as HTMLInputElement
    const keyInput = screen.getByLabelText('API Key') as HTMLInputElement

    expect(modelInput).toHaveValue('')
    expect(keyInput).toHaveValue('')

    fireEvent.change(modelInput, { target: { value: 'openrouter/anthropic/claude-3.5-sonnet' } })
    fireEvent.change(keyInput, { target: { value: secret } })

    expect(modelInput).toHaveValue('openrouter/anthropic/claude-3.5-sonnet')
    expect(keyInput).toHaveValue(secret)

    fireEvent.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() =>
      expect(onUpsertRuntimeSettings).toHaveBeenCalledWith({
        providerId: 'openrouter',
        modelId: 'openrouter/anthropic/claude-3.5-sonnet',
        openrouterApiKey: secret,
      }),
    )

    // After save, the configuration form closes - key input is no longer in DOM
    expect(screen.queryByLabelText('API Key')).not.toBeInTheDocument()
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()

    // Rerender with updated settings
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
        onRefreshRuntimeSettings={vi.fn(async () => makeRuntimeSettings())}
        onUpsertRuntimeSettings={onUpsertRuntimeSettings}
      />,
    )

    // OpenRouter should now show as Configured
    expect(screen.getByText('Configured')).toBeVisible()
    expect(screen.getByText('Active')).toBeVisible()

    // Open Configure form again to verify model ID is saved and secret is not echoed
    const configureButtonAfter = screen.getAllByRole('button', { name: 'Configure' })[0]
    fireEvent.click(configureButtonAfter)

    const modelInputAfter = screen.getByLabelText('Model ID') as HTMLInputElement
    const keyInputAfter = screen.getByLabelText('API Key') as HTMLInputElement

    await waitFor(() => expect(modelInputAfter).toHaveValue('openrouter/anthropic/claude-3.5-sonnet'))
    expect(keyInputAfter).toHaveValue('')
    expect(screen.queryByDisplayValue(secret)).not.toBeInTheDocument()
    expect(screen.queryByText(secret)).not.toBeInTheDocument()
  })

  it('keeps last truthful provider snapshot visible when a typed load error is present', () => {
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

    expect(screen.getByText('Cadence timed out while loading app-global runtime settings.')).toBeVisible()

    // Provider cards should still be visible showing the snapshot
    expect(screen.getByText('OpenRouter')).toBeVisible()
    expect(screen.getByText('OpenAI Codex')).toBeVisible()

    // Click Configure to verify the model ID is preserved
    const configureButton = screen.getByRole('button', { name: 'Configure' })
    fireEvent.click(configureButton)

    expect(screen.getByDisplayValue('openrouter/meta-llama/llama-3.1-8b-instruct')).toBeVisible()
    expect(screen.getByText('Key saved')).toBeVisible()
  })
})
