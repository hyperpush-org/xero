import type {
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeSettingsDto,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'

export const DEFAULT_RUNTIME_PROVIDER_ID: RuntimeSettingsDto['providerId'] = 'openai_codex'

export interface SelectedRuntimeProviderView {
  providerId: RuntimeSettingsDto['providerId']
  providerLabel: string
  modelId: string | null
  openrouterApiKeyConfigured: boolean
}

export function isKnownRuntimeProviderId(
  value: string | null | undefined,
): value is RuntimeSettingsDto['providerId'] {
  return value === 'openrouter' || value === 'openai_codex'
}

export function getRuntimeProviderLabel(providerId: string | null | undefined): string {
  if (providerId === 'openrouter') {
    return 'OpenRouter'
  }

  if (providerId === 'openai_codex') {
    return 'OpenAI Codex'
  }

  if (typeof providerId !== 'string' || providerId.trim().length === 0) {
    return 'Runtime provider'
  }

  return providerId
    .trim()
    .split(/[_\s-]+/)
    .filter((part) => part.length > 0)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ')
}

export function getDefaultRuntimeModelId(providerId: RuntimeSettingsDto['providerId']): string {
  if (providerId === 'openrouter') {
    return ''
  }

  return 'openai_codex'
}

export function resolveSelectedRuntimeProvider(
  runtimeSettings: RuntimeSettingsDto | null,
  runtimeSession: RuntimeSessionView | null,
): SelectedRuntimeProviderView {
  if (runtimeSettings) {
    return {
      providerId: runtimeSettings.providerId,
      providerLabel: getRuntimeProviderLabel(runtimeSettings.providerId),
      modelId: runtimeSettings.modelId,
      openrouterApiKeyConfigured: runtimeSettings.openrouterApiKeyConfigured,
    }
  }

  if (isKnownRuntimeProviderId(runtimeSession?.providerId)) {
    return {
      providerId: runtimeSession.providerId,
      providerLabel: getRuntimeProviderLabel(runtimeSession.providerId),
      modelId: runtimeSession.runtimeKind,
      openrouterApiKeyConfigured: runtimeSession.providerId === 'openrouter',
    }
  }

  return {
    providerId: DEFAULT_RUNTIME_PROVIDER_ID,
    providerLabel: getRuntimeProviderLabel(DEFAULT_RUNTIME_PROVIDER_ID),
    modelId: getDefaultRuntimeModelId(DEFAULT_RUNTIME_PROVIDER_ID),
    openrouterApiKeyConfigured: false,
  }
}

export function hasProviderMismatch(
  selectedProvider: SelectedRuntimeProviderView,
  runtimeSession: RuntimeSessionView | null,
): boolean {
  return Boolean(runtimeSession && runtimeSession.providerId !== selectedProvider.providerId)
}

export function getAgentSessionUnavailableReason(
  runtimeSession: RuntimeSessionView | null,
  runtimeErrorMessage: string | null,
  selectedProvider: SelectedRuntimeProviderView,
): string {
  if (runtimeErrorMessage) {
    return runtimeErrorMessage
  }

  const providerLabel = selectedProvider.providerLabel
  const providerMismatch = hasProviderMismatch(selectedProvider, runtimeSession)

  if (!runtimeSession) {
    if (selectedProvider.providerId === 'openrouter') {
      return selectedProvider.openrouterApiKeyConfigured
        ? 'Bind OpenRouter with the saved app-local API key to create a project runtime session.'
        : 'Configure an OpenRouter API key in Settings before Cadence can bind a project runtime session.'
    }

    return 'Sign in with OpenAI to create or reuse a runtime session for this imported project.'
  }

  if (runtimeSession.lastError?.message) {
    return runtimeSession.lastError.message
  }

  if (providerMismatch) {
    return `Selected provider is ${providerLabel}, but the persisted runtime session still reflects ${getRuntimeProviderLabel(runtimeSession.providerId)}. Rebind the selected provider so durable runtime truth matches Settings.`
  }

  switch (runtimeSession.phase) {
    case 'authenticated':
      if (selectedProvider.providerId === 'openrouter') {
        return runtimeSession.sessionId
          ? `Cadence validated the saved OpenRouter key and bound session ${runtimeSession.sessionLabel} for model ${selectedProvider.modelId || 'the selected model'}.`
          : `Cadence validated the saved OpenRouter key for model ${selectedProvider.modelId || 'the selected model'}.`
      }

      return runtimeSession.sessionId
        ? `Cadence is authenticated as ${runtimeSession.accountLabel} and bound to session ${runtimeSession.sessionLabel}.`
        : `Cadence is authenticated as ${runtimeSession.accountLabel}.`
    case 'awaiting_browser_callback':
      return selectedProvider.providerId === 'openrouter'
        ? 'Cadence surfaced a browser-auth phase while OpenRouter is selected. Rebind the runtime from the Agent tab or switch providers in Settings.'
        : 'Cadence started the OpenAI login flow and is waiting for the browser callback to return.'
    case 'awaiting_manual_input':
      return selectedProvider.providerId === 'openrouter'
        ? 'Cadence surfaced a manual-auth phase while OpenRouter is selected. Rebind the runtime from the Agent tab or switch providers in Settings.'
        : 'Cadence is waiting for the pasted OpenAI redirect URL to finish login for this project.'
    case 'starting':
      return selectedProvider.providerId === 'openrouter'
        ? 'Cadence is validating the saved OpenRouter key and binding a runtime session for this project.'
        : 'Cadence is opening the OpenAI login flow for this project.'
    case 'exchanging_code':
      return selectedProvider.providerId === 'openrouter'
        ? 'Cadence is completing the selected OpenRouter runtime bind for this project.'
        : 'Cadence is exchanging the OpenAI authorization code for a project-bound session.'
    case 'refreshing':
      return selectedProvider.providerId === 'openrouter'
        ? 'Cadence is revalidating the saved OpenRouter binding for this project.'
        : 'Cadence is refreshing the stored OpenAI auth session for this project.'
    case 'idle':
      return selectedProvider.providerId === 'openrouter'
        ? selectedProvider.openrouterApiKeyConfigured
          ? 'Bind OpenRouter from the Agent tab to create or refresh the runtime session for this imported project.'
          : 'Configure an OpenRouter API key in Settings before Cadence can bind the selected provider for this imported project.'
        : 'Sign in with OpenAI to create or reuse a runtime session for this imported project.'
    case 'cancelled':
      return selectedProvider.providerId === 'openrouter'
        ? 'The OpenRouter bind flow was cancelled before Cadence could refresh the project runtime session.'
        : 'The OpenAI login flow was cancelled before Cadence could create a runtime session.'
    case 'failed':
      return selectedProvider.providerId === 'openrouter'
        ? 'Cadence could not bind the OpenRouter runtime for this project.'
        : 'Cadence could not create a runtime session for this project.'
  }
}

export function getAgentRuntimeRunUnavailableReason(
  runtimeRun: RuntimeRunView | null,
  runtimeRunErrorMessage: string | null,
  runtimeSession: RuntimeSessionView | null,
  selectedProvider: SelectedRuntimeProviderView,
): string {
  if (runtimeRunErrorMessage) {
    return runtimeRunErrorMessage
  }

  if (!runtimeRun) {
    if (runtimeSession?.isAuthenticated && !hasProviderMismatch(selectedProvider, runtimeSession)) {
      return 'No durable supervised runtime run is recorded for this project yet.'
    }

    if (selectedProvider.providerId === 'openrouter') {
      return selectedProvider.openrouterApiKeyConfigured
        ? 'Bind OpenRouter first, then launch a supervised harness run to populate durable repo-local run state for this project.'
        : 'Configure an OpenRouter API key in Settings and bind the provider before launching a supervised harness run for this project.'
    }

    return 'Authenticate and launch a supervised harness run to populate durable repo-local run state for this project.'
  }

  if (runtimeRun.lastError?.message) {
    return runtimeRun.lastError.message
  }

  if (runtimeRun.isFailed) {
    return 'Cadence recovered a failed supervised harness run. Inspect the final checkpoint and error details before retrying.'
  }

  if (runtimeRun.isStale) {
    return 'Cadence recovered a stale supervised harness run. The durable checkpoint trail is still available even though the control endpoint is no longer reachable.'
  }

  if (runtimeRun.isTerminal) {
    return 'Cadence recovered a stopped supervised harness run. Final checkpoints remain available for inspection after reload.'
  }

  return 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.'
}

export function getAgentMessagesUnavailableReason(
  runtimeSession: RuntimeSessionView | null,
  runtimeStream: RuntimeStreamView | null,
  runtimeRun: RuntimeRunView | null,
  selectedProvider: SelectedRuntimeProviderView,
): string {
  const providerMismatch = hasProviderMismatch(selectedProvider, runtimeSession)

  if (!runtimeSession) {
    if (selectedProvider.providerId === 'openrouter') {
      return runtimeRun
        ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires an OpenRouter runtime bind for the selected provider.'
        : selectedProvider.openrouterApiKeyConfigured
          ? 'Bind OpenRouter from the Agent tab to establish the runtime session for this imported project.'
          : 'Configure an OpenRouter API key in Settings before Cadence can establish a runtime session for this imported project.'
    }

    return runtimeRun
      ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires a desktop-authenticated runtime session.'
      : 'Sign in with OpenAI to establish a runtime session for this imported project.'
  }

  if (providerMismatch) {
    return `Live runtime streaming is paused because Settings now select ${selectedProvider.providerLabel}, but the recovered runtime session still reflects ${getRuntimeProviderLabel(runtimeSession.providerId)}. Rebind the selected provider before trusting new stream activity.`
  }

  if (!runtimeSession.isAuthenticated) {
    if (selectedProvider.providerId === 'openrouter') {
      if (runtimeSession.isLoginInProgress) {
        return 'Cadence is binding the selected OpenRouter provider. Wait for the saved-key validation to finish before expecting live stream activity.'
      }

      return runtimeRun
        ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires an authenticated OpenRouter runtime binding.'
        : selectedProvider.openrouterApiKeyConfigured
          ? 'Bind OpenRouter from the Agent tab to establish the runtime session for this imported project.'
          : 'Configure an OpenRouter API key in Settings before live streaming can start for this imported project.'
    }

    if (runtimeSession.isLoginInProgress) {
      return 'Finish the OpenAI login flow to establish the runtime session for this imported project.'
    }

    return runtimeRun
      ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires an authenticated runtime session.'
      : 'Sign in with OpenAI to establish a runtime session for this imported project.'
  }

  if (!runtimeStream) {
    return runtimeRun?.hasCheckpoints
      ? 'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.'
      : selectedProvider.providerId === 'openrouter'
        ? 'Cadence authenticated the selected OpenRouter provider, but the live runtime stream has not started yet.'
        : 'Cadence authenticated this project, but the live runtime stream has not started yet.'
  }

  if (runtimeStream.lastIssue?.message) {
    return runtimeStream.lastIssue.message
  }

  const latestActionRequired = runtimeStream.actionRequired[runtimeStream.actionRequired.length - 1] ?? null
  if (latestActionRequired) {
    return `${latestActionRequired.title}: ${latestActionRequired.detail}`
  }

  if (runtimeStream.status === 'subscribing') {
    return runtimeRun?.hasCheckpoints
      ? 'Cadence is reconnecting the live runtime stream while keeping durable checkpoints visible for this selected project.'
      : 'Cadence is connecting the live runtime stream for this selected project.'
  }

  if (runtimeStream.status === 'replaying') {
    return 'Cadence is replaying recent run-scoped activity while the live runtime stream catches up for this selected project.'
  }

  if (runtimeStream.status === 'complete') {
    return runtimeStream.completion?.detail ?? 'Cadence completed the current runtime bootstrap stream for this project.'
  }

  if (runtimeStream.status === 'stale') {
    return 'Cadence marked the runtime stream as stale. Retry or reselect the project to resubscribe.'
  }

  if (runtimeStream.status === 'error') {
    return runtimeStream.failure?.message ?? 'Cadence could not keep the runtime stream connected for this project.'
  }

  return `Live runtime activity is streaming for this project (${runtimeStream.items.length} item${runtimeStream.items.length === 1 ? '' : 's'} captured).`
}
