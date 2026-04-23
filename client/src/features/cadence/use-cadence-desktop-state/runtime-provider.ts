import type {
  ProviderProfileReadinessDto,
  ProviderProfilesDto,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeSettingsDto,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'
import { getActiveProviderProfile } from '@/src/lib/cadence-model'
import {
  getCloudProviderDefaultModelId,
  getCloudProviderLabel,
  isApiKeyCloudProvider,
  isKnownCloudProviderId,
} from '@/src/lib/cadence-model/provider-presets'

export const DEFAULT_RUNTIME_PROVIDER_ID: RuntimeSettingsDto['providerId'] = 'openai_codex'

export type SelectedRuntimeProviderSource =
  | 'provider_profiles'
  | 'runtime_settings'
  | 'runtime_session'
  | 'default'

export interface SelectedRuntimeProviderView {
  profileId: string | null
  profileLabel: string | null
  providerId: RuntimeSettingsDto['providerId']
  providerLabel: string
  modelId: string | null
  readiness: ProviderProfileReadinessDto | null
  openrouterApiKeyConfigured: boolean
  anthropicApiKeyConfigured: boolean
  source: SelectedRuntimeProviderSource
}

export interface ProviderMismatchCopyView {
  reason: string
  sessionRecoveryCopy: string
  streamRecoveryCopy: string
}

export function isKnownRuntimeProviderId(
  value: string | null | undefined,
): value is RuntimeSettingsDto['providerId'] {
  return isKnownCloudProviderId(value)
}

export function getRuntimeProviderLabel(providerId: string | null | undefined): string {
  if (typeof providerId !== 'string' || providerId.trim().length === 0) {
    return 'Runtime provider'
  }

  return getCloudProviderLabel(providerId)
}

export function getDefaultRuntimeModelId(providerId: RuntimeSettingsDto['providerId']): string {
  return getCloudProviderDefaultModelId(providerId)
}

function hasAnyReadyProfile(
  providerProfiles: ProviderProfilesDto | null | undefined,
  providerId: RuntimeSettingsDto['providerId'],
): boolean {
  return providerProfiles?.profiles.some((profile) => profile.providerId === providerId && profile.readiness.ready) ?? false
}

function hasSelectedApiKeyProviderConfigured(selectedProvider: SelectedRuntimeProviderView): boolean {
  if (!isApiKeyCloudProvider(selectedProvider.providerId)) {
    return false
  }

  if (selectedProvider.readiness) {
    return selectedProvider.readiness.status !== 'missing'
  }

  if (selectedProvider.providerId === 'openrouter') {
    return selectedProvider.openrouterApiKeyConfigured
  }

  if (selectedProvider.providerId === 'anthropic') {
    return selectedProvider.anthropicApiKeyConfigured
  }

  return false
}

interface SelectedApiKeyProviderState {
  providerLabel: string
  ready: boolean
  readinessStatus: ProviderProfileReadinessDto['status'] | null
}

function getSelectedApiKeyProviderState(
  selectedProvider: SelectedRuntimeProviderView,
): SelectedApiKeyProviderState | null {
  if (!isApiKeyCloudProvider(selectedProvider.providerId)) {
    return null
  }

  return {
    providerLabel: selectedProvider.providerLabel,
    ready: selectedProvider.readiness?.ready ?? false,
    readinessStatus: selectedProvider.readiness?.status ?? (hasSelectedApiKeyProviderConfigured(selectedProvider) ? 'ready' : 'missing'),
  }
}

function getSelectedRuntimeIdentityLabel(selectedProvider: SelectedRuntimeProviderView): string {
  const profileId = selectedProvider.profileId?.trim() ?? ''
  if (profileId.length === 0) {
    return selectedProvider.providerLabel
  }

  const profileLabel = selectedProvider.profileLabel?.trim() ?? ''
  if (profileLabel.length === 0 || profileLabel === profileId) {
    return profileId
  }

  return `${profileLabel} (${profileId})`
}

function getApiKeyArticle(providerLabel: string): 'a' | 'an' {
  const normalizedProviderLabel = providerLabel.trim().toLowerCase()
  return normalizedProviderLabel.startsWith('a') || normalizedProviderLabel.startsWith('e') || normalizedProviderLabel.startsWith('i') || normalizedProviderLabel.startsWith('o') || normalizedProviderLabel.startsWith('u')
    ? 'an'
    : 'a'
}

function getConfigureApiKeyCopy(providerLabel: string, suffix: string): string {
  return `Configure ${getApiKeyArticle(providerLabel)} ${providerLabel} API key in Settings${suffix}`
}

export function resolveSelectedRuntimeProvider(
  providerProfiles: ProviderProfilesDto | null,
  runtimeSettings: RuntimeSettingsDto | null,
  runtimeSession: RuntimeSessionView | null,
): SelectedRuntimeProviderView {
  const activeProfile = getActiveProviderProfile(providerProfiles)
  if (activeProfile && isKnownRuntimeProviderId(activeProfile.providerId)) {
    return {
      profileId: activeProfile.profileId,
      profileLabel: activeProfile.label,
      providerId: activeProfile.providerId,
      providerLabel: getRuntimeProviderLabel(activeProfile.providerId),
      modelId: activeProfile.modelId,
      readiness: activeProfile.readiness,
      openrouterApiKeyConfigured: hasAnyReadyProfile(providerProfiles, 'openrouter'),
      anthropicApiKeyConfigured: hasAnyReadyProfile(providerProfiles, 'anthropic'),
      source: 'provider_profiles',
    }
  }

  if (runtimeSettings) {
    return {
      profileId: null,
      profileLabel: null,
      providerId: runtimeSettings.providerId,
      providerLabel: getRuntimeProviderLabel(runtimeSettings.providerId),
      modelId: runtimeSettings.modelId,
      readiness: null,
      openrouterApiKeyConfigured: runtimeSettings.openrouterApiKeyConfigured,
      anthropicApiKeyConfigured: runtimeSettings.anthropicApiKeyConfigured,
      source: 'runtime_settings',
    }
  }

  if (isKnownRuntimeProviderId(runtimeSession?.providerId)) {
    return {
      profileId: null,
      profileLabel: null,
      providerId: runtimeSession.providerId,
      providerLabel: getRuntimeProviderLabel(runtimeSession.providerId),
      modelId: null,
      readiness: null,
      openrouterApiKeyConfigured: runtimeSession.providerId === 'openrouter',
      anthropicApiKeyConfigured: runtimeSession.providerId === 'anthropic',
      source: 'runtime_session',
    }
  }

  return {
    profileId: null,
    profileLabel: null,
    providerId: DEFAULT_RUNTIME_PROVIDER_ID,
    providerLabel: getRuntimeProviderLabel(DEFAULT_RUNTIME_PROVIDER_ID),
    modelId: getDefaultRuntimeModelId(DEFAULT_RUNTIME_PROVIDER_ID),
    readiness: null,
    openrouterApiKeyConfigured: false,
    anthropicApiKeyConfigured: false,
    source: 'default',
  }
}

export function hasProviderMismatch(
  selectedProvider: SelectedRuntimeProviderView,
  runtimeSession: RuntimeSessionView | null,
): boolean {
  return Boolean(runtimeSession && runtimeSession.providerId !== selectedProvider.providerId)
}

export function getProviderMismatchCopy(
  selectedProvider: SelectedRuntimeProviderView,
  runtimeSession: RuntimeSessionView | null,
): ProviderMismatchCopyView | null {
  if (!runtimeSession || runtimeSession.providerId === selectedProvider.providerId) {
    return null
  }

  const selectionNoun = selectedProvider.profileId ? 'provider profile' : 'provider'
  const selectionScope = selectedProvider.profileId ? 'profile' : 'provider'

  return {
    reason: `Settings now select ${selectionNoun} ${getSelectedRuntimeIdentityLabel(selectedProvider)}, but the persisted runtime session still reflects ${getRuntimeProviderLabel(runtimeSession.providerId)}.`,
    sessionRecoveryCopy: `Rebind the selected ${selectionScope} so durable runtime truth matches Settings.`,
    streamRecoveryCopy: `Rebind the selected ${selectionScope} before trusting new stream activity.`,
  }
}

export function getAgentSessionUnavailableReason(
  runtimeSession: RuntimeSessionView | null,
  runtimeErrorMessage: string | null,
  selectedProvider: SelectedRuntimeProviderView,
): string {
  if (runtimeErrorMessage) {
    return runtimeErrorMessage
  }

  const providerMismatchCopy = getProviderMismatchCopy(selectedProvider, runtimeSession)
  const selectedApiKeyProvider = getSelectedApiKeyProviderState(selectedProvider)

  if (!runtimeSession) {
    if (selectedApiKeyProvider) {
      if (selectedApiKeyProvider.readinessStatus === 'malformed') {
        return `Repair the selected ${selectedApiKeyProvider.providerLabel} profile credentials in Settings before Cadence can bind a project runtime session.`
      }

      return selectedApiKeyProvider.ready
        ? `Bind ${selectedApiKeyProvider.providerLabel} with the selected app-local provider profile to create a project runtime session.`
        : getConfigureApiKeyCopy(
            selectedApiKeyProvider.providerLabel,
            ' before Cadence can bind a project runtime session.',
          )
    }

    return 'Sign in with OpenAI to create or reuse a runtime session for this imported project.'
  }

  if (runtimeSession.lastError?.message) {
    return runtimeSession.lastError.message
  }

  if (providerMismatchCopy) {
    return `${providerMismatchCopy.reason} ${providerMismatchCopy.sessionRecoveryCopy}`
  }

  switch (runtimeSession.phase) {
    case 'authenticated':
      if (selectedApiKeyProvider) {
        return runtimeSession.sessionId
          ? `Cadence validated the selected ${selectedApiKeyProvider.providerLabel} profile and bound session ${runtimeSession.sessionLabel} for model ${selectedProvider.modelId || 'the selected model'}.`
          : `Cadence validated the selected ${selectedApiKeyProvider.providerLabel} profile for model ${selectedProvider.modelId || 'the selected model'}.`
      }

      return runtimeSession.sessionId
        ? `Cadence is authenticated as ${runtimeSession.accountLabel} and bound to session ${runtimeSession.sessionLabel}.`
        : `Cadence is authenticated as ${runtimeSession.accountLabel}.`
    case 'awaiting_browser_callback':
      return selectedApiKeyProvider
        ? `Cadence surfaced a browser-auth phase while ${selectedApiKeyProvider.providerLabel} is selected. Rebind the runtime from the Agent tab or switch providers in Settings.`
        : 'Cadence started the OpenAI login flow and is waiting for the browser callback to return.'
    case 'awaiting_manual_input':
      return selectedApiKeyProvider
        ? `Cadence surfaced a manual-auth phase while ${selectedApiKeyProvider.providerLabel} is selected. Rebind the runtime from the Agent tab or switch providers in Settings.`
        : 'Cadence is waiting for the pasted OpenAI redirect URL to finish login for this project.'
    case 'starting':
      return selectedApiKeyProvider
        ? `Cadence is validating the selected ${selectedApiKeyProvider.providerLabel} profile and binding a runtime session for this project.`
        : 'Cadence is opening the OpenAI login flow for this project.'
    case 'exchanging_code':
      return selectedApiKeyProvider
        ? `Cadence is completing the selected ${selectedApiKeyProvider.providerLabel} runtime bind for this project.`
        : 'Cadence is exchanging the OpenAI authorization code for a project-bound session.'
    case 'refreshing':
      return selectedApiKeyProvider
        ? `Cadence is revalidating the selected ${selectedApiKeyProvider.providerLabel} binding for this project.`
        : 'Cadence is refreshing the stored OpenAI auth session for this project.'
    case 'idle':
      if (selectedApiKeyProvider) {
        if (selectedApiKeyProvider.readinessStatus === 'malformed') {
          return `Repair the selected ${selectedApiKeyProvider.providerLabel} profile credentials in Settings before Cadence can bind the selected provider for this imported project.`
        }

        return selectedApiKeyProvider.ready
          ? `Bind ${selectedApiKeyProvider.providerLabel} from the Agent tab to create or refresh the runtime session for this imported project.`
          : getConfigureApiKeyCopy(
              selectedApiKeyProvider.providerLabel,
              ' before Cadence can bind the selected provider for this imported project.',
            )
      }

      return 'Sign in with OpenAI to create or reuse a runtime session for this imported project.'
    case 'cancelled':
      return selectedApiKeyProvider
        ? `The ${selectedApiKeyProvider.providerLabel} bind flow was cancelled before Cadence could refresh the project runtime session.`
        : 'The OpenAI login flow was cancelled before Cadence could create a runtime session.'
    case 'failed':
      return selectedApiKeyProvider
        ? `Cadence could not bind the ${selectedApiKeyProvider.providerLabel} runtime for this project.`
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

  const providerMismatchCopy = getProviderMismatchCopy(selectedProvider, runtimeSession)
  const selectedApiKeyProvider = getSelectedApiKeyProviderState(selectedProvider)

  if (!runtimeRun) {
    if (runtimeSession?.isAuthenticated && !providerMismatchCopy) {
      return 'No durable supervised runtime run is recorded for this project yet.'
    }

    if (providerMismatchCopy) {
      return `${providerMismatchCopy.reason} Rebind the selected ${selectedProvider.profileId ? 'profile' : 'provider'} before launching a supervised harness run for this project.`
    }

    if (selectedApiKeyProvider) {
      if (selectedApiKeyProvider.readinessStatus === 'malformed') {
        return `Repair the selected ${selectedApiKeyProvider.providerLabel} profile credentials in Settings and bind the provider before launching a supervised harness run for this project.`
      }

      return selectedApiKeyProvider.ready
        ? `Bind ${selectedApiKeyProvider.providerLabel} first, then launch a supervised harness run to populate durable repo-local run state for this project.`
        : getConfigureApiKeyCopy(
            selectedApiKeyProvider.providerLabel,
            ' and bind the provider before launching a supervised harness run for this project.',
          )
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
  const providerMismatchCopy = getProviderMismatchCopy(selectedProvider, runtimeSession)
  const selectedApiKeyProvider = getSelectedApiKeyProviderState(selectedProvider)

  if (!runtimeSession) {
    if (selectedApiKeyProvider) {
      if (selectedApiKeyProvider.readinessStatus === 'malformed') {
        return runtimeRun
          ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires repaired ${selectedApiKeyProvider.providerLabel} profile credentials for the selected provider.`
          : `Repair the selected ${selectedApiKeyProvider.providerLabel} profile credentials in Settings before Cadence can establish a runtime session for this imported project.`
      }

      return runtimeRun
        ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires a ${selectedApiKeyProvider.providerLabel} runtime bind for the selected provider.`
        : selectedApiKeyProvider.ready
          ? `Bind ${selectedApiKeyProvider.providerLabel} from the Agent tab to establish the runtime session for this imported project.`
          : getConfigureApiKeyCopy(
              selectedApiKeyProvider.providerLabel,
              ' before Cadence can establish a runtime session for this imported project.',
            )
    }

    return runtimeRun
      ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires a desktop-authenticated runtime session.'
      : 'Sign in with OpenAI to establish a runtime session for this imported project.'
  }

  if (providerMismatchCopy) {
    return `Live runtime streaming is paused because ${providerMismatchCopy.reason} ${providerMismatchCopy.streamRecoveryCopy}`
  }

  if (!runtimeSession.isAuthenticated) {
    if (selectedApiKeyProvider) {
      if (runtimeSession.isLoginInProgress) {
        return `Cadence is binding the selected ${selectedApiKeyProvider.providerLabel} provider. Wait for the saved-key validation to finish before expecting live stream activity.`
      }

      if (selectedApiKeyProvider.readinessStatus === 'malformed') {
        return runtimeRun
          ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires repaired ${selectedApiKeyProvider.providerLabel} profile credentials.`
          : `Repair the selected ${selectedApiKeyProvider.providerLabel} profile credentials in Settings before live streaming can start for this imported project.`
      }

      return runtimeRun
        ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires an authenticated ${selectedApiKeyProvider.providerLabel} runtime binding.`
        : selectedApiKeyProvider.ready
          ? `Bind ${selectedApiKeyProvider.providerLabel} from the Agent tab to establish the runtime session for this imported project.`
          : getConfigureApiKeyCopy(
              selectedApiKeyProvider.providerLabel,
              ' before live streaming can start for this imported project.',
            )
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
      : selectedApiKeyProvider
        ? `Cadence authenticated the selected ${selectedApiKeyProvider.providerLabel} provider, but the live runtime stream has not started yet.`
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
