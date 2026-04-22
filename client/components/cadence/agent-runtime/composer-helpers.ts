import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  ProviderModelThinkingEffortDto,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamStatus,
} from '@/src/lib/cadence-model'
import { getProviderModelThinkingEffortLabel } from '@/src/lib/cadence-model'

import { displayValue } from './shared-helpers'
import { hasUsableRuntimeRunId } from './runtime-stream-helpers'

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

export interface ComposerCatalogStatusCopy {
  catalogLabel: string
  catalogDetail: string
  thinkingDetail: string
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

export function getSelectedProviderId(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderId ?? runtimeSession?.providerId ?? 'openai_codex'
}

export function getSelectedProviderLabel(agent: AgentPaneView, runtimeSession: RuntimeSessionView | null): string {
  return agent.selectedProviderLabel ??
    (getSelectedProviderId(agent, runtimeSession) === 'openrouter' ? 'OpenRouter' : 'OpenAI Codex')
}

export function getComposerPlaceholder(
  runtimeSession: RuntimeSessionView | null,
  streamStatus: RuntimeStreamStatus,
  runtimeRun: RuntimeRunView | null,
  streamRunId: string | undefined,
  options: { selectedProviderId: string; openrouterApiKeyConfigured: boolean; providerMismatch: boolean },
): string {
  if (!runtimeSession) {
    if (options.selectedProviderId === 'openrouter') {
      return options.openrouterApiKeyConfigured
        ? 'Bind OpenRouter from the Agent tab to start.'
        : 'Configure an OpenRouter API key in Settings to start.'
    }

    return 'Connect a provider to start.'
  }

  if (options.providerMismatch) {
    return `Rebind ${options.selectedProviderId === 'openrouter' ? 'OpenRouter' : 'the selected provider'} before trusting new live activity.`
  }

  if (!runtimeSession.isAuthenticated) {
    if (runtimeSession.isLoginInProgress) {
      return options.selectedProviderId === 'openrouter'
        ? 'Finish the OpenRouter bind to continue.'
        : 'Finish the login flow to continue.'
    }

    return options.selectedProviderId === 'openrouter'
      ? options.openrouterApiKeyConfigured
        ? 'Bind OpenRouter from the Agent tab to start.'
        : 'Configure an OpenRouter API key in Settings to start.'
      : 'Connect a provider to start.'
  }

  if (!hasUsableRuntimeRunId(runtimeRun)) {
    return 'Start or reconnect a supervised run to create the run-scoped live feed for this imported project.'
  }

  switch (streamStatus) {
    case 'live':
      return 'Live activity streaming. Composer is read-only.'
    case 'complete':
      return 'Run completed.'
    case 'stale':
      return 'Stream went stale — retry to refresh.'
    case 'error':
      return 'Stream failed — retry to restore.'
    case 'subscribing':
      return 'Connecting to the live transcript.'
    case 'replaying':
      return `Cadence is replaying recent run-scoped activity for ${displayValue(streamRunId, runtimeRun.runId)} while the live feed catches up.`
    case 'idle':
      return 'Waiting for first event…'
  }
}
