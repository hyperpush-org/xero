import type {
  AgentRunControlTruthSource,
  AgentRunPromptView,
  OperatorActionErrorView,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from '@/src/features/xero/use-xero-desktop-state/types'
import type { AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type {
  AgentDefinitionBaseCapabilityProfileDto,
  AgentDefinitionSummaryDto,
  ProviderModelThinkingEffortDto,
  RuntimeAgentIdDto,
  RuntimeRunApprovalModeDto,
  RuntimeRunControlInputDto,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeStreamStatus,
} from '@/src/lib/xero-model'
import {
  getRuntimeAgentDescriptor,
  getProviderModelThinkingEffortLabel,
  getRuntimeRunApprovalModeLabel,
} from '@/src/lib/xero-model'

import { displayValue } from './shared-helpers'
import { hasUsableRuntimeRunId } from './runtime-stream-helpers'
import { getCloudProviderLabel } from '@/src/lib/xero-model/provider-presets'

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
  currentSelectionKey: string | null | undefined = null,
): ComposerModelGroup[] {
  const currentModel = getComposerModelOption(models, currentSelectionKey)
  const visibleModels =
    currentModel && !models.some((model) => model.selectionKey === currentModel.selectionKey)
      ? [currentModel, ...models]
      : models
  const groups = new Map<string, ComposerModelGroup>()

  for (const model of visibleModels) {
    const existingGroup = groups.get(model.groupId)
    const nextItem: ComposerModelOption = {
      value: model.selectionKey,
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
  selectionKey: string | null | undefined,
): AgentPaneView['selectedModelOption'] {
  const trimmedSelectionKey = selectionKey?.trim() ?? ''
  if (trimmedSelectionKey.length === 0) {
    return null
  }

  return (
    models.find((model) => model.selectionKey === trimmedSelectionKey || model.modelId === trimmedSelectionKey) ?? {
      selectionKey: trimmedSelectionKey,
      profileId: null,
      profileLabel: null,
      providerId: 'openai_codex',
      providerLabel: 'Runtime provider',
      modelId: trimmedSelectionKey,
      label: trimmedSelectionKey,
      displayName: trimmedSelectionKey,
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
    label: getProviderModelThinkingEffortLabel(effort),
  }))
}

export function getComposerApprovalOptions(runtimeAgentId: RuntimeAgentIdDto): ComposerApprovalOption[] {
  const allowedModes = getRuntimeAgentDescriptor(runtimeAgentId).allowedApprovalModes
  return composerApprovalModes.filter((mode) => allowedModes.includes(mode)).map((mode) => ({
    value: mode,
    label: getRuntimeRunApprovalModeLabel(mode),
  }))
}

export function runtimeAgentIdForCustomBaseCapability(
  profile: AgentDefinitionBaseCapabilityProfileDto,
): RuntimeAgentIdDto {
  switch (profile) {
    case 'engineering':
      return 'engineer'
    case 'debugging':
      return 'debug'
    case 'agent_builder':
      return 'agent_create'
    case 'observe_only':
      return 'ask'
  }
}

export interface ComposerAgentSelection {
  selectionKey: string
  runtimeAgentId: RuntimeAgentIdDto
  agentDefinitionId: string | null
  label: string
  isCustom: boolean
  scope: AgentDefinitionSummaryDto['scope'] | null
}

const BUILTIN_SELECTION_PREFIX = 'builtin:'
const CUSTOM_SELECTION_PREFIX = 'custom:'

export function buildComposerAgentSelectionKey(
  runtimeAgentId: RuntimeAgentIdDto,
  agentDefinitionId: string | null | undefined,
): string {
  const trimmed = agentDefinitionId?.trim() ?? ''
  if (trimmed.length === 0) {
    return `${BUILTIN_SELECTION_PREFIX}${runtimeAgentId}`
  }
  return `${CUSTOM_SELECTION_PREFIX}${trimmed}`
}

export function parseComposerAgentSelectionKey(
  key: string,
  customAgents: readonly AgentDefinitionSummaryDto[],
): ComposerAgentSelection | null {
  if (key.startsWith(BUILTIN_SELECTION_PREFIX)) {
    const runtimeAgentId = key.slice(BUILTIN_SELECTION_PREFIX.length) as RuntimeAgentIdDto
    const descriptor = getRuntimeAgentDescriptor(runtimeAgentId)
    return {
      selectionKey: key,
      runtimeAgentId,
      agentDefinitionId: null,
      label: descriptor.label,
      isCustom: false,
      scope: null,
    }
  }
  if (key.startsWith(CUSTOM_SELECTION_PREFIX)) {
    const definitionId = key.slice(CUSTOM_SELECTION_PREFIX.length)
    const definition = customAgents.find((agent) => agent.definitionId === definitionId)
    if (!definition) return null
    return {
      selectionKey: key,
      runtimeAgentId: runtimeAgentIdForCustomBaseCapability(definition.baseCapabilityProfile),
      agentDefinitionId: definition.definitionId,
      label: definition.displayName,
      isCustom: true,
      scope: definition.scope,
    }
  }
  return null
}

export function resolveRuntimeAgentApprovalMode(
  runtimeAgentId: RuntimeAgentIdDto,
  currentApprovalMode: RuntimeRunApprovalModeDto,
): RuntimeRunApprovalModeDto {
  const descriptor = getRuntimeAgentDescriptor(runtimeAgentId)
  return descriptor.allowedApprovalModes.includes(currentApprovalMode)
    ? currentApprovalMode
    : descriptor.defaultApprovalMode
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
  runtimeAgentId: RuntimeAgentIdDto
  agentDefinitionId?: string | null
  models: AgentPaneView['providerModelCatalog']['models']
  selectionKey: string | null | undefined
  thinkingEffort: ProviderModelThinkingEffortDto | null | undefined
  approvalMode: RuntimeRunApprovalModeDto
}): RuntimeRunControlInputDto | null {
  const model = getComposerModelOption(options.models, options.selectionKey)
  if (!model) {
    return null
  }

  const trimmedDefinitionId = options.agentDefinitionId?.trim() ?? ''
  return {
    runtimeAgentId: options.runtimeAgentId,
    agentDefinitionId: trimmedDefinitionId.length > 0 ? trimmedDefinitionId : null,
    providerProfileId: model.profileId,
    modelId: model.modelId,
    thinkingEffort: resolveComposerThinkingSelection(model, options.thinkingEffort),
    approvalMode: resolveRuntimeAgentApprovalMode(options.runtimeAgentId, options.approvalMode),
    planModeRequired: false,
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
    detail: 'This value will seed the next Xero-owned agent run until run-scoped control truth exists.',
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
      detail: 'Xero is waiting for the owned-agent run snapshot to confirm the latest prompt or control queue request.',
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
      summary: 'Draft the first prompt before starting the agent run.',
      detail: 'Xero will pass this draft as the initial queued run input once the run starts.',
    }
  }

  return null
}

export function getComposerPlaceholder(
  runtimeSession: RuntimeSessionView | null,
  streamStatus: RuntimeStreamStatus,
  runtimeRun: RuntimeRunView | null,
  _streamRunId: string | undefined,
  options: {
    selectedProviderId: string
    agentRuntimeBlocked: boolean
  },
): string {
  const providerLabel = getCloudProviderLabel(options.selectedProviderId)

  if (options.agentRuntimeBlocked) {
    return 'Connect a provider in Settings to start chatting.'
  }

  if (!runtimeSession) {
    return `Ask anything to get started with ${providerLabel}.`
  }

  if (!runtimeSession.isAuthenticated) {
    if (runtimeSession.isLoginInProgress) {
      return `Finish signing in with ${providerLabel} to continue.`
    }
    return `Ask anything to get started with ${providerLabel}.`
  }

  if (!hasUsableRuntimeRunId(runtimeRun)) {
    return 'Type your first message to start the agent.'
  }

  if (runtimeRun.isTerminal) {
    return runtimeRun.isFailed
      ? 'Last run failed — send a message to start a fresh one.'
      : 'Run finished — send a message to keep going.'
  }

  switch (streamStatus) {
    case 'stale':
      return 'Live updates paused — your messages still go through.'
    case 'error':
      return "Live updates dropped — your messages still go through."
    case 'subscribing':
      return 'Connecting to the live feed… type away.'
    case 'replaying':
      return 'Catching up on recent activity… type away.'
    case 'complete':
    case 'idle':
    case 'live':
      return 'Ask the agent anything…'
  }
}
