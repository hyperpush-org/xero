import type {
  ProviderCredentialDto,
  ProviderCredentialsSnapshotDto,
  ProviderModelCatalogDto,
  ProviderModelDto,
  ProviderModelThinkingEffortDto,
  ProviderProfileReadinessDto,
  ProviderProfilesDto,
  RuntimeRunControlSelectionView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeSettingsDto,
  RuntimeStreamView,
} from '@/src/lib/cadence-model'
import { getActiveProviderProfile } from '@/src/lib/cadence-model'
import { findProviderCredential } from '@/src/lib/cadence-model/provider-credentials'
import {
  getCloudProviderAuthMode,
  getCloudProviderDefaultModelId,
  getCloudProviderLabel,
  getCloudProviderPreset,
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

type SelectedConfiguredProviderAuthMode = Exclude<
  NonNullable<ReturnType<typeof getCloudProviderAuthMode>>,
  'oauth'
>

interface SelectedConfiguredProviderState {
  providerLabel: string
  authMode: SelectedConfiguredProviderAuthMode
  ready: boolean
  readinessStatus: ProviderProfileReadinessDto['status'] | null
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

function getSelectedConfiguredProviderState(
  selectedProvider: SelectedRuntimeProviderView,
): SelectedConfiguredProviderState | null {
  const authMode = getCloudProviderAuthMode(selectedProvider.providerId)
  if (!authMode || authMode === 'oauth') {
    return null
  }

  if (authMode === 'api_key') {
    const readinessStatus =
      selectedProvider.readiness?.status ??
      (selectedProvider.source === 'runtime_session'
        ? 'ready'
        : hasSelectedApiKeyProviderConfigured(selectedProvider)
          ? 'ready'
          : 'missing')

    return {
      providerLabel: selectedProvider.providerLabel,
      authMode,
      ready: selectedProvider.readiness?.ready ?? readinessStatus === 'ready',
      readinessStatus,
    }
  }

  if (selectedProvider.readiness) {
    return {
      providerLabel: selectedProvider.providerLabel,
      authMode,
      ready: selectedProvider.readiness.ready,
      readinessStatus: selectedProvider.readiness.status,
    }
  }

  return {
    providerLabel: selectedProvider.providerLabel,
    authMode,
    ready: selectedProvider.source === 'runtime_session',
    readinessStatus: selectedProvider.source === 'runtime_session' ? 'ready' : 'missing',
  }
}

function getRecoveredRuntimeRunConfiguredProviderLabel(
  runtimeRun: RuntimeRunView | null,
  selectedProvider: SelectedRuntimeProviderView,
): string | null {
  const runtimeRunProviderId = runtimeRun?.providerId?.trim() ?? ''
  if (runtimeRunProviderId.length === 0 || !isKnownRuntimeProviderId(runtimeRunProviderId)) {
    return null
  }

  const authMode = getCloudProviderAuthMode(runtimeRunProviderId)
  if (!authMode || authMode === 'oauth') {
    return null
  }

  return runtimeRunProviderId === selectedProvider.providerId
    ? selectedProvider.providerLabel
    : getRuntimeProviderLabel(runtimeRunProviderId)
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

function getRequiredMetadataSuffix(providerId: RuntimeSettingsDto['providerId']): string {
  const preset = getCloudProviderPreset(providerId)
  if (!preset) {
    return ''
  }

  const parts: string[] = []
  if (preset.regionMode === 'required') {
    parts.push('region')
  }
  if (preset.projectIdMode === 'required') {
    parts.push('project ID')
  }

  if (parts.length === 0) {
    return ''
  }

  return ` with ${parts.join(' and ')}`
}

function getRepairConfiguredProviderCopy(
  state: SelectedConfiguredProviderState,
  suffix: string,
): string {
  switch (state.authMode) {
    case 'api_key':
      return `Repair the selected ${state.providerLabel} profile credentials in Settings${suffix}`
    case 'local':
      return `Repair the selected ${state.providerLabel} local-endpoint metadata in Settings${suffix}`
    case 'ambient':
      return `Repair the selected ${state.providerLabel} ambient-auth metadata in Settings${suffix}`
  }
}

function getRecoveredRepairConfiguredProviderRequirement(
  state: SelectedConfiguredProviderState,
): string {
  switch (state.authMode) {
    case 'api_key':
      return `repaired ${state.providerLabel} profile credentials`
    case 'local':
      return `repaired ${state.providerLabel} local-endpoint metadata`
    case 'ambient':
      return `repaired ${state.providerLabel} ambient-auth metadata`
  }
}

function getSetupConfiguredProviderCopy(
  state: SelectedConfiguredProviderState,
  providerId: RuntimeSettingsDto['providerId'],
  suffix: string,
): string {
  switch (state.authMode) {
    case 'api_key':
      return getConfigureApiKeyCopy(state.providerLabel, suffix)
    case 'local':
      return `Save the selected ${state.providerLabel} local endpoint profile in Settings${suffix}`
    case 'ambient':
      return `Save the selected ${state.providerLabel} ambient-auth profile${getRequiredMetadataSuffix(providerId)} in Settings${suffix}`
  }
}

function getBindConfiguredProviderCopy(
  state: SelectedConfiguredProviderState,
  suffix: string,
): string {
  switch (state.authMode) {
    case 'api_key':
      return `Bind ${state.providerLabel} with the selected app-local provider profile${suffix}`
    case 'local':
      return `Bind ${state.providerLabel} with the selected local provider profile${suffix}`
    case 'ambient':
      return `Bind ${state.providerLabel} with the selected ambient-auth provider profile${suffix}`
  }
}

function getValidatedConfiguredProviderBindingLabel(
  state: SelectedConfiguredProviderState,
): string {
  switch (state.authMode) {
    case 'api_key':
      return 'profile'
    case 'local':
      return 'local provider profile'
    case 'ambient':
      return 'ambient-auth provider profile'
  }
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
    reason: `Configured ${selectionNoun} ${getSelectedRuntimeIdentityLabel(selectedProvider)} no longer matches the persisted runtime session for ${getRuntimeProviderLabel(runtimeSession.providerId)}.`,
    sessionRecoveryCopy: `Rebind this ${selectionScope} so durable runtime truth matches Settings.`,
    streamRecoveryCopy: `Rebind this ${selectionScope} before trusting new stream activity.`,
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
  const selectedConfiguredProvider = getSelectedConfiguredProviderState(selectedProvider)

  if (!runtimeSession) {
    if (selectedConfiguredProvider) {
      if (selectedConfiguredProvider.readinessStatus === 'malformed') {
        return getRepairConfiguredProviderCopy(
          selectedConfiguredProvider,
          ' before Cadence can bind a project runtime session.',
        )
      }

      return selectedConfiguredProvider.ready
        ? getBindConfiguredProviderCopy(
            selectedConfiguredProvider,
            ' to create a project runtime session.',
          )
        : getSetupConfiguredProviderCopy(
            selectedConfiguredProvider,
            selectedProvider.providerId,
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
      if (selectedConfiguredProvider) {
        return runtimeSession.sessionId
          ? `Cadence validated the selected ${selectedConfiguredProvider.providerLabel} ${getValidatedConfiguredProviderBindingLabel(selectedConfiguredProvider)} and bound session ${runtimeSession.sessionLabel} for model ${selectedProvider.modelId || 'the selected model'}.`
          : `Cadence validated the selected ${selectedConfiguredProvider.providerLabel} ${getValidatedConfiguredProviderBindingLabel(selectedConfiguredProvider)} for model ${selectedProvider.modelId || 'the selected model'}.`
      }

      return runtimeSession.sessionId
        ? `Cadence is authenticated as ${runtimeSession.accountLabel} and bound to session ${runtimeSession.sessionLabel}.`
        : `Cadence is authenticated as ${runtimeSession.accountLabel}.`
    case 'awaiting_browser_callback':
      return selectedConfiguredProvider
        ? `Cadence surfaced a browser-auth phase while ${selectedConfiguredProvider.providerLabel} is configured. Rebind the runtime from the Agent tab or choose another model in the Agent composer.`
        : 'Cadence started the OpenAI login flow and is waiting for the browser callback to return.'
    case 'awaiting_manual_input':
      return selectedConfiguredProvider
        ? `Cadence surfaced a manual-auth phase while ${selectedConfiguredProvider.providerLabel} is configured. Rebind the runtime from the Agent tab or choose another model in the Agent composer.`
        : 'Cadence is waiting for the pasted OpenAI redirect URL to finish login for this project.'
    case 'starting':
      return selectedConfiguredProvider
        ? `Cadence is validating the configured ${selectedConfiguredProvider.providerLabel} ${getValidatedConfiguredProviderBindingLabel(selectedConfiguredProvider)} and binding a runtime session for this project.`
        : 'Cadence is opening the OpenAI login flow for this project.'
    case 'exchanging_code':
      return selectedConfiguredProvider
        ? `Cadence is completing the configured ${selectedConfiguredProvider.providerLabel} runtime bind for this project.`
        : 'Cadence is exchanging the OpenAI authorization code for a project-bound session.'
    case 'refreshing':
      return selectedConfiguredProvider
        ? `Cadence is revalidating the configured ${selectedConfiguredProvider.providerLabel} binding for this project.`
        : 'Cadence is refreshing the stored OpenAI auth session for this project.'
    case 'idle':
      if (selectedConfiguredProvider) {
        if (selectedConfiguredProvider.readinessStatus === 'malformed') {
          return getRepairConfiguredProviderCopy(
            selectedConfiguredProvider,
            ' before Cadence can bind the configured provider for this imported project.',
          )
        }

        return selectedConfiguredProvider.ready
          ? getBindConfiguredProviderCopy(
              selectedConfiguredProvider,
              ' to create or refresh the runtime session for this imported project.',
            )
          : getSetupConfiguredProviderCopy(
              selectedConfiguredProvider,
              selectedProvider.providerId,
              ' before Cadence can bind the configured provider for this imported project.',
            )
      }

      return 'Sign in with OpenAI to create or reuse a runtime session for this imported project.'
    case 'cancelled':
      return selectedConfiguredProvider
        ? `The ${selectedConfiguredProvider.providerLabel} bind flow was cancelled before Cadence could refresh the project runtime session.`
        : 'The OpenAI login flow was cancelled before Cadence could create a runtime session.'
    case 'failed':
      return selectedConfiguredProvider
        ? `Cadence could not bind the ${selectedConfiguredProvider.providerLabel} runtime for this project.`
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
  const selectedConfiguredProvider = getSelectedConfiguredProviderState(selectedProvider)

  if (!runtimeRun) {
    if (runtimeSession?.isAuthenticated && !providerMismatchCopy) {
      return 'No durable supervised runtime run is recorded for this project yet.'
    }

    if (providerMismatchCopy) {
      return `${providerMismatchCopy.reason} Rebind this ${selectedProvider.profileId ? 'profile' : 'provider'} before launching a supervised harness run for this project.`
    }

    if (selectedConfiguredProvider) {
      if (selectedConfiguredProvider.readinessStatus === 'malformed') {
        return getRepairConfiguredProviderCopy(
          selectedConfiguredProvider,
          ' and bind the provider before launching a supervised harness run for this project.',
        )
      }

      return selectedConfiguredProvider.ready
        ? getBindConfiguredProviderCopy(
            selectedConfiguredProvider,
            ' first, then launch a supervised harness run to populate durable repo-local run state for this project.',
          )
        : getSetupConfiguredProviderCopy(
            selectedConfiguredProvider,
            selectedProvider.providerId,
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
  const selectedConfiguredProvider = getSelectedConfiguredProviderState(selectedProvider)
  const recoveredRuntimeRunConfiguredProviderLabel = getRecoveredRuntimeRunConfiguredProviderLabel(
    runtimeRun,
    selectedProvider,
  )

  if (!runtimeSession) {
    if (selectedConfiguredProvider) {
      if (selectedConfiguredProvider.readinessStatus === 'malformed') {
        return runtimeRun
          ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires ${getRecoveredRepairConfiguredProviderRequirement(selectedConfiguredProvider)} for the configured provider.`
          : getRepairConfiguredProviderCopy(
              selectedConfiguredProvider,
              ' before Cadence can establish a runtime session for this imported project.',
            )
      }

      return runtimeRun
        ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires a ${selectedConfiguredProvider.providerLabel} runtime bind for the configured provider.`
        : selectedConfiguredProvider.ready
          ? getBindConfiguredProviderCopy(
              selectedConfiguredProvider,
              ' to establish the runtime session for this imported project.',
            )
          : getSetupConfiguredProviderCopy(
              selectedConfiguredProvider,
              selectedProvider.providerId,
              ' before Cadence can establish a runtime session for this imported project.',
            )
    }

    if (runtimeRun && recoveredRuntimeRunConfiguredProviderLabel) {
      return `Cadence recovered durable supervised-run state for this project, but live streaming still requires a ${recoveredRuntimeRunConfiguredProviderLabel} runtime bind for the recovered provider.`
    }

    return runtimeRun
      ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires a desktop-authenticated runtime session.'
      : 'Sign in with OpenAI to establish a runtime session for this imported project.'
  }

  if (providerMismatchCopy) {
    return `Live runtime streaming is paused because ${providerMismatchCopy.reason} ${providerMismatchCopy.streamRecoveryCopy}`
  }

  if (!runtimeSession.isAuthenticated) {
    if (selectedConfiguredProvider) {
      if (runtimeSession.isLoginInProgress) {
        return `Cadence is binding the selected ${selectedConfiguredProvider.providerLabel} provider. Wait for the saved-key validation to finish before expecting live stream activity.`
      }

      if (selectedConfiguredProvider.readinessStatus === 'malformed') {
        return runtimeRun
          ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires ${getRecoveredRepairConfiguredProviderRequirement(selectedConfiguredProvider)}.`
          : getRepairConfiguredProviderCopy(
              selectedConfiguredProvider,
              ' before live streaming can start for this imported project.',
            )
      }

      return runtimeRun
        ? `Cadence recovered durable supervised-run state for this project, but live streaming still requires an authenticated ${selectedConfiguredProvider.providerLabel} runtime binding.`
        : selectedConfiguredProvider.ready
          ? getBindConfiguredProviderCopy(
              selectedConfiguredProvider,
              ' to establish the runtime session for this imported project.',
            )
          : getSetupConfiguredProviderCopy(
              selectedConfiguredProvider,
              selectedProvider.providerId,
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
      : selectedConfiguredProvider
        ? `Cadence authenticated the selected ${selectedConfiguredProvider.providerLabel} provider, but the live runtime stream has not started yet.`
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

// ---------------------------------------------------------------------------
// Phase 3.3 — credentials-driven model selection.
// Below replaces the legacy `resolveSelectedRuntimeProvider` machinery with a
// pure projection from "what model did the user pick" + the credentials
// snapshot + the catalog union. There is no provider-mismatch state because
// the chosen model fully determines the provider.
// ---------------------------------------------------------------------------

export type SelectedModelSource = 'runtime_run' | 'credential_default' | 'fallback'

export interface SelectedModelView {
  providerId: ProviderCredentialDto['providerId'] | null
  providerLabel: string
  modelId: string | null
  hasCredential: boolean
  credentialKind: ProviderCredentialDto['kind'] | null
  source: SelectedModelSource
}

export interface ComposerModelOptionView {
  selectionKey: string
  providerId: ProviderCredentialDto['providerId']
  providerLabel: string
  modelId: string
  displayName: string
  thinking: ProviderModelDto['thinking']
  thinkingEffortOptions: ProviderModelThinkingEffortDto[]
  defaultThinkingEffort: ProviderModelThinkingEffortDto | null
}

export function buildComposerModelSelectionKey(
  providerId: ProviderCredentialDto['providerId'],
  modelId: string,
): string {
  return `${providerId}:${modelId}`
}

export function parseComposerModelSelectionKey(
  key: string,
): { providerId: string; modelId: string } | null {
  const idx = key.indexOf(':')
  if (idx <= 0 || idx === key.length - 1) {
    return null
  }
  return {
    providerId: key.slice(0, idx),
    modelId: key.slice(idx + 1),
  }
}

function thinkingEffortListFor(model: ProviderModelDto): ProviderModelThinkingEffortDto[] {
  if (!model.thinking.supported) {
    return []
  }
  const out: ProviderModelThinkingEffortDto[] = []
  for (const effort of model.thinking.effortOptions) {
    if (!out.includes(effort)) {
      out.push(effort)
    }
  }
  return out
}

function defaultThinkingEffortFor(
  model: ProviderModelDto,
  options: ProviderModelThinkingEffortDto[],
): ProviderModelThinkingEffortDto | null {
  if (!model.thinking.supported) {
    return null
  }
  if (model.thinking.defaultEffort && options.includes(model.thinking.defaultEffort)) {
    return model.thinking.defaultEffort
  }
  return options[0] ?? null
}

/**
 * Builds the composer model picker option list as the union of every
 * credentialed provider's catalog. Models are sorted by provider label, then
 * by display name. Local-only providers (Ollama) only appear when their
 * credential row exists.
 */
export function buildComposerModelOptions(
  credentials: ProviderCredentialsSnapshotDto | null,
  catalogs: Record<string, ProviderModelCatalogDto> | null | undefined,
): ComposerModelOptionView[] {
  const list = credentials?.credentials ?? []
  if (list.length === 0) {
    return []
  }

  const options: ComposerModelOptionView[] = []
  for (const credential of list) {
    const catalog = catalogs?.[credential.providerId]
    if (!catalog) continue
    const providerLabel = getRuntimeProviderLabel(credential.providerId)
    for (const model of catalog.models) {
      const modelId = model.modelId.trim()
      if (modelId.length === 0) continue
      const thinkingEffortOptions = thinkingEffortListFor(model)
      options.push({
        selectionKey: buildComposerModelSelectionKey(credential.providerId, modelId),
        providerId: credential.providerId,
        providerLabel,
        modelId,
        displayName: model.displayName.trim() || modelId,
        thinking: model.thinking,
        thinkingEffortOptions,
        defaultThinkingEffort: defaultThinkingEffortFor(model, thinkingEffortOptions),
      })
    }
  }

  options.sort((left, right) => {
    if (left.providerLabel !== right.providerLabel) {
      return left.providerLabel.localeCompare(right.providerLabel)
    }
    return left.displayName.localeCompare(right.displayName)
  })

  return options
}

/**
 * Resolves which model the agent pane is currently bound to by looking at
 * (1) the active runtime-run's selected controls, (2) the user's last-picked
 * model for that provider (`default_model_id` on the credential row), and
 * finally (3) a fallback chosen by credential-presence order.
 *
 * The chosen model fully determines the provider — there is no separate
 * provider selection state and no mismatch is possible.
 */
export function resolveSelectedModel(
  credentials: ProviderCredentialsSnapshotDto | null,
  selectedRunControls: RuntimeRunControlSelectionView | null,
  options: { runtimeRun?: RuntimeRunView | null } = {},
): SelectedModelView {
  const list = credentials?.credentials ?? []

  // 1. Runtime-run truth: selected controls carry the (provider, model) pair.
  if (selectedRunControls) {
    const runProviderId = options.runtimeRun?.providerId?.trim() ?? ''
    if (runProviderId.length > 0 && isKnownRuntimeProviderId(runProviderId)) {
      const credential = findProviderCredential(credentials, runProviderId)
      return {
        providerId: runProviderId,
        providerLabel: getRuntimeProviderLabel(runProviderId),
        modelId: selectedRunControls.modelId || null,
        hasCredential: credential !== null,
        credentialKind: credential?.kind ?? null,
        source: 'runtime_run',
      }
    }
  }

  // 2. Per-credential `default_model_id`: prefer a credential that has one set.
  const credentialWithDefault = list.find((credential) =>
    typeof credential.defaultModelId === 'string' && credential.defaultModelId.trim().length > 0,
  )
  if (credentialWithDefault) {
    return {
      providerId: credentialWithDefault.providerId,
      providerLabel: getRuntimeProviderLabel(credentialWithDefault.providerId),
      modelId: credentialWithDefault.defaultModelId ?? null,
      hasCredential: true,
      credentialKind: credentialWithDefault.kind,
      source: 'credential_default',
    }
  }

  // 3. Fallback: any credentialed provider, no model bound.
  if (list.length > 0) {
    const first = list[0]
    return {
      providerId: first.providerId,
      providerLabel: getRuntimeProviderLabel(first.providerId),
      modelId: null,
      hasCredential: true,
      credentialKind: first.kind,
      source: 'fallback',
    }
  }

  return {
    providerId: null,
    providerLabel: 'Runtime provider',
    modelId: null,
    hasCredential: false,
    credentialKind: null,
    source: 'fallback',
  }
}

/**
 * Returns true when the agent pane should refuse to launch a run because
 * either no provider is credentialed or the chosen model's provider has no
 * credential. This replaces the legacy `providerMismatch` boolean — there is
 * only one reason to block now: missing credentials.
 */
export function isAgentRuntimeBlocked(
  credentials: ProviderCredentialsSnapshotDto | null,
  selectedModel: SelectedModelView,
): boolean {
  const list = credentials?.credentials ?? []
  if (list.length === 0) {
    return true
  }

  if (!selectedModel.providerId) {
    return true
  }

  return !selectedModel.hasCredential
}
