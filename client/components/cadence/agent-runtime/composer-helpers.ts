import type {
  AgentRunControlTruthSource,
  AgentRunPromptView,
  OperatorActionErrorView,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from '@/src/features/cadence/use-cadence-desktop-state/types'
import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  ProviderModelThinkingEffortDto,
  ProviderProfileReadinessDto,
  RuntimeRunApprovalModeDto,
  RuntimeRunControlInputDto,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamStatus,
} from '@/src/lib/cadence-model'
import {
  getProviderModelThinkingEffortLabel,
  getRuntimeRunApprovalModeLabel,
} from '@/src/lib/cadence-model'

import { displayValue } from './shared-helpers'
import { hasUsableRuntimeRunId } from './runtime-stream-helpers'
import { getCloudProviderLabel, isApiKeyCloudProvider } from '@/src/lib/cadence-model/provider-presets'

export interface ComposerModelOption {
  value: string
  label: string
}

export interface ComposerModelGroup {
  id: string
  label: string
  items: ComposerModelOption[]
}

export interface ComposerThinkingOption {
  value: ProviderModelThinkingEffortDto
  label: string
}

export interface ComposerApprovalOption {
  value: RuntimeRunApprovalModeDto
  label: string
}

export interface ComposerCatalogStatusCopy {
  catalogLabel: string
  catalogDetail: string
  thinkingDetail: string
}

export interface ComposerStatusCopy {
  tone: 'start' | 'ready' | 'active' | 'pending'
  badgeLabel: string
  summary: string
  detail: string
}

const composerApprovalModes: RuntimeRunApprovalModeDto[] = ['suggest', 'auto_edit', 'yolo']

function formatComposerTimestamp(value: string | null | undefined, fallback: string): string {
  const trimmedValue = value?.trim() ?? ''
  return trimmedValue.length > 0 ? trimmedValue : fallback
}

function formatComposerRevision(value: number | null | undefined): string {
  return Number.isFinite(value) && typeof value === 'number' && value > 0 ? `revision ${value}` : 'revision unavailable'
}

export function getComposerModelGroups(
  models: AgentPaneView['providerModelCatalog']['models'],
  currentModelId: string | null | undefined = null,
): ComposerModelGroup[] {
  const currentModel = getComposerModelOption(models, currentModelId)
  const visibleModels =
    currentModel && !models.some((model) => model.modelId === currentModel.modelId)
      ? [currentModel, ...models]
      : models
  const groups = new Map<string, ComposerModelGroup>()

  for (const model of visibleModels) {
    const existingGroup = groups.get(model.groupId)
    const nextItem: ComposerModelOption = {
      value: model.modelId,
      label: model.availability === 'orphaned' ? `${model.label} · unavailable` : model.label,
    }

    if (existingGroup) {
      existingGroup.items.push(nextItem)
      continue
    }

    groups.set(model.groupId, {
      id: model.groupId,
      label: model.groupLabel,
      items: [nextItem],
    })
  }

  return Array.from(groups.values())
}

export function getComposerModelOption(
  models: AgentPaneView['providerModelCatalog']['models'],
  modelId: string | null | undefined,
): AgentPaneView['selectedModelOption'] {
  const trimmedModelId = modelId?.trim() ?? ''
  if (trimmedModelId.length === 0) {
    return null
  }

  return (
    models.find((model) => model.modelId === trimmedModelId) ?? {
      modelId: trimmedModelId,
      label: trimmedModelId,
      displayName: trimmedModelId,
      groupId: 'current_selection',
      groupLabel: 'Current selection',
      availability: 'orphaned',
      availabilityLabel: 'Unavailable',
      thinkingSupported: false,
      thinkingEffortOptions: [],
      defaultThinkingEffort: null,
    }
  )
}

export function getComposerThinkingOptions(
  model: AgentPaneView['selectedModelOption'],
): ComposerThinkingOption[] {
  if (!model?.thinkingSupported) {
    return []
  }

  return model.thinkingEffortOptions.map((effort) => ({
    value: effort,
    label: `Thinking · ${getProviderModelThinkingEffortLabel(effort).toLowerCase()}`,
  }))
}

export function getComposerApprovalOptions(): ComposerApprovalOption[] {
  return composerApprovalModes.map((mode) => ({
    value: mode,
    label: `Approval · ${getRuntimeRunApprovalModeLabel(mode).toLowerCase()}`,
  }))
}

export function resolveComposerThinkingSelection(
  model: AgentPaneView['selectedModelOption'],
  currentThinkingEffort: ProviderModelThinkingEffortDto | null | undefined,
): ProviderModelThinkingEffortDto | null {
  if (!model?.thinkingSupported || model.thinkingEffortOptions.length === 0) {
    return null
  }

  if (currentThinkingEffort && model.thinkingEffortOptions.includes(currentThinkingEffort)) {
    return currentThinkingEffort
  }

  if (model.defaultThinkingEffort && model.thinkingEffortOptions.includes(model.defaultThinkingEffort)) {
    return model.defaultThinkingEffort
  }

  return model.thinkingEffortOptions[0] ?? null
}

export function getComposerControlInput(options: {
  models: AgentPaneView['providerModelCatalog']['models']
  modelId: string | null | undefined
  thinkingEffort: ProviderModelThinkingEffortDto | null | undefined
  approvalMode: RuntimeRunApprovalModeDto
}): RuntimeRunControlInputDto | null {
  const model = getComposerModelOption(options.models, options.modelId)
  if (!model) {
    return null
  }

  return {
    modelId: model.modelId,
    thinkingEffort: resolveComposerThinkingSelection(model, options.thinkingEffort),
    approvalMode: options.approvalMode,
  }
}

export function getComposerCatalogStatusCopy(
  catalog: AgentPaneView['providerModelCatalog'],
  selectedModel: AgentPaneView['selectedModelOption'],
): ComposerCatalogStatusCopy {
  if (!selectedModel) {
    return {
      catalogLabel: catalog.stateLabel,
      catalogDetail: catalog.detail,
      thinkingDetail: 'Choose a model to inspect supported thinking efforts.',
    }
  }

  if (selectedModel.availability === 'orphaned') {
    return {
      catalogLabel: catalog.stateLabel,
      catalogDetail: catalog.detail,
      thinkingDetail: `${selectedModel.label} is not present in the latest ${catalog.providerLabel} catalog, so thinking options stay unavailable until discovery confirms it.`,
    }
  }

  if (!selectedModel.thinkingSupported) {
    return {
      catalogLabel: catalog.stateLabel,
      catalogDetail: catalog.detail,
      thinkingDetail: `${selectedModel.label} does not expose configurable thinking for this provider catalog.`,
    }
  }

  const supportedEfforts = selectedModel.thinkingEffortOptions.map((effort) => getProviderModelThinkingEffortLabel(effort))
  const defaultEffort = selectedModel.defaultThinkingEffort
    ? getProviderModelThinkingEffortLabel(selectedModel.defaultThinkingEffort)
    : null

  return {
    catalogLabel: catalog.stateLabel,
    catalogDetail: catalog.detail,
    thinkingDetail: defaultEffort
      ? `Thinking supports ${supportedEfforts.join(', ')}. Default: ${defaultEffort}.`
      : `Thinking supports ${supportedEfforts.join(', ')}.`,
  }
}

export function getComposerControlStatusCopy(options: {
  label: string
  selectedLabel: string
  truthSource: AgentRunControlTruthSource
  activeLabel: string | null
  activeRevision: number | null
  activeAt: string | null
  pendingLabel: string | null
  pendingRevision: number | null
  pendingAt: string | null
}): ComposerStatusCopy {
  if (options.pendingLabel && options.pendingRevision && options.pendingAt) {
    const activeDetail = `${displayValue(options.activeLabel, 'Unavailable')} (${formatComposerRevision(options.activeRevision)} at ${formatComposerTimestamp(options.activeAt, 'an unknown time')})`

    if (options.label === 'Approval' && options.pendingLabel === 'YOLO') {
      return {
        tone: 'pending',
        badgeLabel: 'Pending',
        summary: `${options.label} pending · ${options.pendingLabel}`,
        detail: `Queued ${formatComposerRevision(options.pendingRevision)} at ${formatComposerTimestamp(options.pendingAt, 'an unknown time')}. Pending YOLO does not apply until the next model-call boundary. Active approval remains ${activeDetail}.`,
      }
    }

    return {
      tone: 'pending',
      badgeLabel: 'Pending',
      summary: `${options.label} pending · ${options.pendingLabel}`,
      detail: `Queued ${formatComposerRevision(options.pendingRevision)} at ${formatComposerTimestamp(options.pendingAt, 'an unknown time')}. Active ${options.label.toLowerCase()}: ${activeDetail}.`,
    }
  }

  if (options.truthSource === 'runtime_run' && options.activeLabel) {
    return {
      tone: 'active',
      badgeLabel: 'Active',
      summary: `${options.label} active · ${options.activeLabel}`,
      detail: `Using ${formatComposerRevision(options.activeRevision)} applied at ${formatComposerTimestamp(options.activeAt, 'an unknown time')}.`,
    }
  }

  return {
    tone: 'start',
    badgeLabel: 'Start',
    summary: `Next run ${options.label.toLowerCase()} · ${options.selectedLabel}`,
    detail: 'This value will seed the next supervised run until run-scoped control truth exists.',
  }
}

export function getComposerPromptStatusCopy(options: {
  selectedPrompt: AgentRunPromptView
  runtimeRun: RuntimeRunView | null
  canStartRuntimeRun: boolean
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
}): ComposerStatusCopy | null {
  if (options.selectedPrompt.hasQueuedPrompt) {
    return {
      tone: 'pending',
      badgeLabel: 'Queued',
      summary: 'Queued prompt pending the next model-call boundary.',
      detail: `Queued at ${formatComposerTimestamp(options.selectedPrompt.queuedAt, 'an unknown time')}. ${displayValue(options.selectedPrompt.text, 'Prompt text unavailable.')}`,
    }
  }

  if (
    options.runtimeRunActionStatus === 'running' &&
    (options.pendingRuntimeRunAction === 'start' || options.pendingRuntimeRunAction === 'update_controls')
  ) {
    return {
      tone: 'pending',
      badgeLabel: 'Applying',
      summary: 'Waiting for runtime control acknowledgement.',
      detail: 'Cadence is waiting for the supervised run snapshot to confirm the latest prompt or control queue request.',
    }
  }

  if (options.runtimeRun) {
    return {
      tone: 'ready',
      badgeLabel: 'Ready',
      summary: 'No queued prompt is waiting at the boundary.',
      detail: options.runtimeRunActionError
        ? 'The last mutation failed, but the previous truthful run state is still shown below.'
        : 'Type the next prompt, then send when the current boundary is clear.',
    }
  }

  if (options.canStartRuntimeRun) {
    return {
      tone: 'start',
      badgeLabel: 'Start',
      summary: 'Draft the first prompt before starting the supervised run.',
      detail: 'Cadence will pass this draft as the initial queued run input once the run starts.',
    }
  }

  return null
}

export function getSelectedProviderId(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderId ?? runtimeSession?.providerId ?? 'openai_codex'
}

function getProviderDisplayLabel(providerId: string): string {
  return getCloudProviderLabel(providerId)
}

function isApiKeyProvider(providerId: string): boolean {
  return isApiKeyCloudProvider(providerId)
}

function hasConfiguredApiKey(options: {
  selectedProviderId: string
  selectedProfileReadiness?: ProviderProfileReadinessDto | null
  openrouterApiKeyConfigured: boolean
}): boolean {
  if (!isApiKeyProvider(options.selectedProviderId)) {
    return false
  }

  if (options.selectedProfileReadiness) {
    return options.selectedProfileReadiness.status !== 'missing'
  }

  if (options.selectedProviderId === 'openrouter') {
    return options.openrouterApiKeyConfigured
  }

  return false
}

function getApiKeySetupPlaceholder(options: {
  selectedProviderId: string
  selectedProfileReadiness?: ProviderProfileReadinessDto | null
  openrouterApiKeyConfigured: boolean
}): string {
  const providerLabel = getProviderDisplayLabel(options.selectedProviderId)

  if (options.selectedProfileReadiness?.status === 'malformed') {
    return `Repair the ${providerLabel} API key in Settings to start.`
  }

  return hasConfiguredApiKey(options)
    ? `Bind ${providerLabel} from the Agent tab to start.`
    : `Configure an ${providerLabel} API key in Settings to start.`
}

export function getSelectedProviderLabel(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderLabel ?? getProviderDisplayLabel(getSelectedProviderId(agent, runtimeSession))
}

export function getComposerPlaceholder(
  runtimeSession: RuntimeSessionView | null,
  streamStatus: RuntimeStreamStatus,
  runtimeRun: RuntimeRunView | null,
  streamRunId: string | undefined,
  options: {
    selectedProviderId: string
    selectedProfileReadiness?: ProviderProfileReadinessDto | null
    openrouterApiKeyConfigured: boolean
    providerMismatch: boolean
  },
): string {
  const selectedProviderLabel = getProviderDisplayLabel(options.selectedProviderId)
  const selectedProviderUsesApiKey = isApiKeyProvider(options.selectedProviderId)

  if (!runtimeSession) {
    if (selectedProviderUsesApiKey) {
      return getApiKeySetupPlaceholder(options)
    }

    return 'Connect a provider to start.'
  }

  if (options.providerMismatch) {
    return `Rebind ${selectedProviderUsesApiKey ? selectedProviderLabel : 'the selected provider'} before trusting new live activity.`
  }

  if (!runtimeSession.isAuthenticated) {
    if (runtimeSession.isLoginInProgress) {
      return selectedProviderUsesApiKey
        ? `Finish the ${selectedProviderLabel} bind to continue.`
        : 'Finish the login flow to continue.'
    }

    return selectedProviderUsesApiKey ? getApiKeySetupPlaceholder(options) : 'Connect a provider to start.'
  }

  if (!hasUsableRuntimeRunId(runtimeRun)) {
    return 'Draft the first prompt, then start the supervised run for this imported project.'
  }

  if (runtimeRun.isTerminal) {
    return 'This supervised run is terminal. Start or reconnect a run to queue the next prompt.'
  }

  switch (streamStatus) {
    case 'stale':
      return 'Live activity is stale, but run-scoped control truth is still durable here.'
    case 'error':
      return 'Live activity failed to refresh, but you can still inspect durable run control truth here.'
    case 'subscribing':
      return 'Connecting to the live transcript while supervised run controls stay available.'
    case 'replaying':
      return `Cadence is replaying recent run-scoped activity for ${displayValue(streamRunId, runtimeRun.runId)} while the live feed catches up.`
    case 'complete':
    case 'idle':
    case 'live':
      return 'Queue the next prompt for this supervised run.'
  }
}
