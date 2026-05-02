import type {
  ProviderCredentialDto,
  ProviderCredentialsSnapshotDto,
  ProviderModelCatalogDto,
  ProviderModelDto,
  ProviderModelThinkingEffortDto,
  RuntimeRunControlSelectionView,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamView,
} from '@/src/lib/xero-model'
import { findProviderCredential } from '@/src/lib/xero-model/provider-credentials'
import {
  getCloudProviderDefaultProfileId,
  getCloudProviderLabel,
  isKnownCloudProviderId,
} from '@/src/lib/xero-model/provider-presets'

export type SelectedRuntimeProviderSource =
  | 'runtime_run'
  | 'credential_default'
  | 'fallback'
  | 'default'

export function isKnownRuntimeProviderId(
  value: string | null | undefined,
): value is ProviderCredentialDto['providerId'] {
  return isKnownCloudProviderId(value)
}

export function getRuntimeProviderLabel(providerId: string | null | undefined): string {
  if (typeof providerId !== 'string' || providerId.trim().length === 0) {
    return 'Runtime provider'
  }
  return getCloudProviderLabel(providerId)
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
  profileId: string
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

export function getProviderModelCatalogForProvider(
  catalogs: Record<string, ProviderModelCatalogDto> | null | undefined,
  providerId: ProviderCredentialDto['providerId'] | null | undefined,
): ProviderModelCatalogDto | null {
  if (!catalogs || !providerId) {
    return null
  }

  const providerCatalog = catalogs[providerId]
  if (providerCatalog?.providerId === providerId) {
    return providerCatalog
  }

  const defaultProfileId = getCloudProviderDefaultProfileId(providerId)
  if (defaultProfileId) {
    const defaultProfileCatalog = catalogs[defaultProfileId]
    if (defaultProfileCatalog?.providerId === providerId) {
      return defaultProfileCatalog
    }
  }

  return Object.values(catalogs).find((catalog) => catalog.providerId === providerId) ?? null
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
    const catalog = getProviderModelCatalogForProvider(catalogs, credential.providerId)
    if (!catalog) continue
    const providerLabel = getRuntimeProviderLabel(credential.providerId)
    for (const model of catalog.models) {
      const modelId = model.modelId.trim()
      if (modelId.length === 0) continue
      const thinkingEffortOptions = thinkingEffortListFor(model)
      options.push({
        selectionKey: buildComposerModelSelectionKey(credential.providerId, modelId),
        profileId: catalog.profileId,
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

// ---------------------------------------------------------------------------
// Phase 4 cleanup — credentials-driven unavailable-reason helpers.
// Replace the legacy provider-mismatch / configured-provider chain with
// short, credential-aware copy. The chosen model fully determines the
// provider so the wording does not need to enumerate readiness states.
// ---------------------------------------------------------------------------

export function getAgentSessionUnavailableCredentialReason(
  runtimeSession: RuntimeSessionView | null,
  runtimeErrorMessage: string | null,
  selectedModel: SelectedModelView,
  agentRuntimeBlocked: boolean,
): string {
  if (runtimeErrorMessage) return runtimeErrorMessage

  if (agentRuntimeBlocked) {
    return 'Add a provider credential in Settings to start a runtime session for this project.'
  }

  const providerLabel = selectedModel.providerLabel

  if (!runtimeSession) {
    return `Bind ${providerLabel} to create a runtime session for this project.`
  }

  if (runtimeSession.lastError?.message) return runtimeSession.lastError.message

  switch (runtimeSession.phase) {
    case 'authenticated':
      return runtimeSession.sessionId
        ? `Xero is authenticated as ${runtimeSession.accountLabel} and bound to session ${runtimeSession.sessionLabel}.`
        : `Xero is authenticated as ${runtimeSession.accountLabel}.`
    case 'awaiting_browser_callback':
      return `Xero is waiting for the ${providerLabel} browser callback to return.`
    case 'awaiting_manual_input':
      return `Xero is waiting for the pasted ${providerLabel} redirect URL to finish login.`
    case 'starting':
      return `Xero is opening the ${providerLabel} login flow.`
    case 'exchanging_code':
      return `Xero is completing the ${providerLabel} bind.`
    case 'refreshing':
      return `Xero is refreshing the ${providerLabel} session.`
    case 'idle':
      return `Bind ${providerLabel} to create or refresh the runtime session.`
    case 'cancelled':
      return `The ${providerLabel} bind was cancelled before Xero could create a runtime session.`
    case 'failed':
      return `Xero could not bind ${providerLabel} for this project.`
  }
}

export function getAgentRuntimeRunUnavailableCredentialReason(
  runtimeRun: RuntimeRunView | null,
  runtimeRunErrorMessage: string | null,
  runtimeSession: RuntimeSessionView | null,
  agentRuntimeBlocked: boolean,
): string {
  if (runtimeRunErrorMessage) return runtimeRunErrorMessage

  if (!runtimeRun) {
    if (agentRuntimeBlocked) {
      return 'Add a provider credential in Settings, then launch a Xero-owned agent run to populate durable app-data run state.'
    }

    if (runtimeSession?.isAuthenticated) {
      return 'No durable Xero-owned agent run is recorded for this project yet.'
    }

    return 'Authenticate and launch a Xero-owned agent run to populate durable app-data run state.'
  }

  if (runtimeRun.lastError?.message) return runtimeRun.lastError.message
  if (runtimeRun.isFailed) {
    return 'Xero recovered a failed agent run. Inspect the final error details before retrying.'
  }
  if (runtimeRun.isStale) {
    return 'Xero recovered a stale agent run. The saved run history is still available even though the owned runtime is no longer active.'
  }
  if (runtimeRun.isTerminal) {
    return 'Xero recovered a stopped agent run. The final run history remains available after reload.'
  }
  return 'Xero recovered a Xero-owned agent run before the live runtime feed resumed.'
}

export function getAgentMessagesUnavailableCredentialReason(
  runtimeSession: RuntimeSessionView | null,
  runtimeStream: RuntimeStreamView | null,
  runtimeRun: RuntimeRunView | null,
  agentRuntimeBlocked: boolean,
): string {
  if (agentRuntimeBlocked) {
    return runtimeRun
      ? 'Xero recovered durable agent-run state, but live streaming requires a provider credential. Add one in Settings to resume the stream.'
      : 'Add a provider credential in Settings to start the runtime session and live transcript.'
  }

  if (!runtimeSession) {
    return runtimeRun
      ? 'Xero recovered durable agent-run state, but live streaming requires a desktop-authenticated runtime session.'
      : 'Bind a provider in Settings to establish a runtime session for this project.'
  }

  if (!runtimeSession.isAuthenticated) {
    if (runtimeSession.isLoginInProgress) {
      return 'Xero is binding the selected provider. Wait for the saved-key validation to finish before expecting live stream activity.'
    }
    return runtimeRun
      ? 'Xero recovered durable agent-run state, but live streaming still requires an authenticated runtime session.'
      : 'Bind a provider in Settings to establish a runtime session for this project.'
  }

  if (!runtimeStream) {
    return runtimeRun?.hasCheckpoints
      ? 'Xero recovered an agent run, but the live runtime stream has not resumed yet.'
      : 'Xero authenticated this project, but the live runtime stream has not started yet.'
  }

  if (runtimeStream.lastIssue?.message) return runtimeStream.lastIssue.message

  const latestActionRequired = runtimeStream.actionRequired[runtimeStream.actionRequired.length - 1] ?? null
  if (latestActionRequired) {
    return `${latestActionRequired.title}: ${latestActionRequired.detail}`
  }

  if (runtimeStream.status === 'subscribing') {
    return runtimeRun?.hasCheckpoints
      ? 'Xero is reconnecting the live runtime stream for this selected project.'
      : 'Xero is connecting the live runtime stream for this selected project.'
  }
  if (runtimeStream.status === 'replaying') {
    return 'Xero is replaying recent run-scoped activity while the live runtime stream catches up for this selected project.'
  }
  if (runtimeStream.status === 'complete') {
    return runtimeStream.completion?.detail ?? 'Xero completed the current runtime bootstrap stream for this project.'
  }
  if (runtimeStream.status === 'stale') {
    return 'Xero marked the runtime stream as stale. Retry or reselect the project to resubscribe.'
  }
  if (runtimeStream.status === 'error') {
    return runtimeStream.failure?.message ?? 'Xero could not keep the runtime stream connected for this project.'
  }

  return `Live runtime activity is streaming for this project (${runtimeStream.items.length} item${runtimeStream.items.length === 1 ? '' : 's'} captured).`
}
