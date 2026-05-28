import {
  Bug,
  Compass,
  ListChecks,
  MessageCircle,
  Monitor,
  Package,
  Search,
  Sparkles,
  Wrench,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, type ReactNode, type RefObject } from 'react'

import type {
  OperatorActionErrorView,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from '@/src/features/xero/use-xero-desktop-state/types'
import {
  getRuntimeAgentDescriptor,
  RUNTIME_AGENT_DESCRIPTORS,
  type AgentDefinitionSummaryDto,
  type ProviderModelThinkingEffortDto,
  type RuntimeAgentIdDto,
  type RuntimeRunApprovalModeDto,
} from '@/src/lib/xero-model'

import {
  buildComposerAgentSelectionKey,
  runtimeAgentIdForCustomBaseCapability,
} from './composer-helpers'
import { cn } from '@/lib/utils'
import {
  Composer,
  type ComposerSelectGroup,
  type ComposerSelectOption,
} from '@xero/ui/components/composer'
import { useShortcuts } from '@/src/features/shortcuts/shortcuts-provider'

import type {
  ComposerApprovalOption,
  ComposerModelGroup,
  ComposerThinkingOption,
} from './composer-helpers'
import type { SpeechDictationPhase } from './use-speech-dictation'

export type { ComposerPendingAttachment } from '@xero/ui/components/composer'
import type { ComposerPendingAttachment } from '@xero/ui/components/composer'
export type ComposerAttachmentKind = ComposerPendingAttachment['kind']

interface ComposerDictationControl {
  audioLevel?: number
  isVisible: boolean
  phase: SpeechDictationPhase
  isListening: boolean
  isToggleDisabled: boolean
  ariaLabel: string
  tooltip: string
  toggle: () => Promise<void>
}

interface ComposerDockProps {
  density?: 'comfortable' | 'compact'
  /** Tighten paddings, gaps, and control sizes for the narrower sidebar surface. */
  inSidebar?: boolean
  placeholder: string
  draftPrompt: string
  promptInputRef: RefObject<HTMLTextAreaElement | null>
  promptInputLabel: string
  sendButtonLabel: string
  isPromptDisabled: boolean
  isSendDisabled: boolean
  isStopVisible?: boolean
  isStopDisabled?: boolean
  composerRuntimeAgentId: RuntimeAgentIdDto
  composerRuntimeAgentLabel: string
  availableRuntimeAgentIds?: readonly RuntimeAgentIdDto[]
  runtimeAgentLockReason?: string
  hideAgentSelector?: boolean
  composerAgentDefinitionId?: string | null
  composerAgentSelectionKey?: string
  customAgentDefinitions?: readonly AgentDefinitionSummaryDto[]
  composerModelId: string | null
  composerModelGroups: ComposerModelGroup[]
  composerThinkingLevel: ProviderModelThinkingEffortDto | null
  composerThinkingOptions: ComposerThinkingOption[]
  composerThinkingPlaceholder: string
  composerApprovalMode: RuntimeRunApprovalModeDto
  composerApprovalOptions: ComposerApprovalOption[]
  autoCompactEnabled: boolean
  hideAutoCompact?: boolean
  controlsDisabled: boolean
  runtimeAgentSwitchDisabled: boolean
  runtimeSessionBindInFlight: boolean
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
  runtimeRunActionErrorTitle: string
  dictation: ComposerDictationControl
  hideDictation?: boolean
  contextMeter?: ReactNode
  hideContextMeter?: boolean
  pendingAttachments?: ComposerPendingAttachment[]
  onAddFiles?: (files: File[]) => void
  onRemoveAttachment?: (id: string) => void
  onOpenDiagnostics?: () => void
  onDraftPromptChange: (value: string) => void
  onSubmitDraftPrompt: () => void
  onStopRuntimeRun?: () => void
  onAutoCompactEnabledChange: (value: boolean) => void
  onComposerRuntimeAgentChange: (value: RuntimeAgentIdDto) => void
  onComposerAgentSelectionChange?: (selectionKey: string) => void
  onComposerModelChange: (value: string) => void
  onComposerThinkingLevelChange: (value: ProviderModelThinkingEffortDto) => void
  onComposerApprovalModeChange: (value: RuntimeRunApprovalModeDto) => void
}

function getBuiltinAgentIcon(agentId: RuntimeAgentIdDto) {
  switch (agentId) {
    case 'generalist':
      return Compass
    case 'ask':
      return MessageCircle
    case 'plan':
      return ListChecks
    case 'debug':
      return Bug
    case 'crawl':
      return Search
    case 'computer_use':
      return Monitor
    case 'agent_create':
      return Sparkles
    case 'engineer':
      return Wrench
  }
}

function useStableCallback<TArgs extends unknown[], TResult>(
  callback: (...args: TArgs) => TResult,
): (...args: TArgs) => TResult {
  const callbackRef = useRef(callback)

  useEffect(() => {
    callbackRef.current = callback
  }, [callback])

  return useCallback((...args: TArgs) => callbackRef.current(...args), [])
}

/**
 * Thin adapter that maps the desktop runtime's DTO-typed composer state onto the
 * shared, presentation-only `Composer` from `@xero/ui`. Keep all desktop domain
 * knowledge (runtime agents, custom agent grouping, approval modes, runtime run
 * errors) here so the shared component stays framework/DTO agnostic.
 */
export function ComposerDock({
  density = 'comfortable',
  inSidebar = false,
  placeholder,
  draftPrompt,
  promptInputRef,
  promptInputLabel,
  sendButtonLabel,
  isPromptDisabled,
  isSendDisabled,
  isStopVisible = false,
  isStopDisabled = false,
  composerRuntimeAgentId,
  composerRuntimeAgentLabel,
  availableRuntimeAgentIds,
  runtimeAgentLockReason,
  hideAgentSelector = false,
  composerAgentDefinitionId = null,
  composerAgentSelectionKey,
  customAgentDefinitions = [],
  composerModelId,
  composerModelGroups,
  composerThinkingLevel,
  composerThinkingOptions,
  composerThinkingPlaceholder,
  composerApprovalMode,
  composerApprovalOptions,
  autoCompactEnabled,
  hideAutoCompact = false,
  controlsDisabled,
  runtimeAgentSwitchDisabled,
  runtimeSessionBindInFlight,
  runtimeRunActionStatus,
  pendingRuntimeRunAction,
  runtimeRunActionError,
  runtimeRunActionErrorTitle,
  dictation,
  hideDictation = false,
  contextMeter,
  hideContextMeter = false,
  pendingAttachments,
  onAddFiles,
  onRemoveAttachment,
  onOpenDiagnostics,
  onDraftPromptChange,
  onSubmitDraftPrompt,
  onStopRuntimeRun,
  onAutoCompactEnabledChange,
  onComposerRuntimeAgentChange,
  onComposerAgentSelectionChange,
  onComposerModelChange,
  onComposerThinkingLevelChange,
  onComposerApprovalModeChange,
}: ComposerDockProps) {
  const { bindings } = useShortcuts()
  const handleComposerModelChange = useStableCallback(onComposerModelChange)
  const composerRuntimeAgentDescriptor = getRuntimeAgentDescriptor(composerRuntimeAgentId)
  const showApprovalSelector = composerRuntimeAgentDescriptor.allowedApprovalModes.length > 1
  const isAgentSelectorDisabled = runtimeAgentSwitchDisabled || controlsDisabled
  const isUpdatingControls = runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'update_controls'
  const isStartingRun =
    runtimeSessionBindInFlight || (runtimeRunActionStatus === 'running' && pendingRuntimeRunAction === 'start')

  const activeCustomAgent = useMemo(() => {
    if (!composerAgentDefinitionId) return null
    return customAgentDefinitions.find((agent) => agent.definitionId === composerAgentDefinitionId) ?? null
  }, [composerAgentDefinitionId, customAgentDefinitions])
  const isCustomAgent = Boolean(activeCustomAgent)
  const agentTriggerLabel = activeCustomAgent?.displayName ?? composerRuntimeAgentLabel
  const agentSelectorValue =
    composerAgentSelectionKey ??
    buildComposerAgentSelectionKey(composerRuntimeAgentId, composerAgentDefinitionId)

  const availableRuntimeAgentIdSet = useMemo(
    () => (availableRuntimeAgentIds ? new Set(availableRuntimeAgentIds) : null),
    [availableRuntimeAgentIds],
  )
  const selectableRuntimeAgents = useMemo(() => {
    if (!availableRuntimeAgentIdSet) return RUNTIME_AGENT_DESCRIPTORS
    return RUNTIME_AGENT_DESCRIPTORS.filter((agent) => availableRuntimeAgentIdSet.has(agent.id))
  }, [availableRuntimeAgentIdSet])
  const visibleCustomAgents = useMemo(
    () =>
      customAgentDefinitions.filter(
        (agent) =>
          (agent.lifecycleState === 'active' || agent.definitionId === composerAgentDefinitionId) &&
          (!availableRuntimeAgentIdSet ||
            availableRuntimeAgentIdSet.has(runtimeAgentIdForCustomBaseCapability(agent.baseCapabilityProfile))),
      ),
    [availableRuntimeAgentIdSet, composerAgentDefinitionId, customAgentDefinitions],
  )
  const AgentTriggerIcon = isCustomAgent ? Package : getBuiltinAgentIcon(composerRuntimeAgentId)

  const agentGroups = useMemo<ComposerSelectGroup[]>(() => {
    const groups: ComposerSelectGroup[] = []
    if (selectableRuntimeAgents.length > 0) {
      groups.push({
        id: 'builtin',
        options: selectableRuntimeAgents.map((agent) => {
          const BuiltinAgentIcon = getBuiltinAgentIcon(agent.id)
          return {
            id: buildComposerAgentSelectionKey(agent.id, null),
            label: agent.label,
            icon: <BuiltinAgentIcon aria-hidden="true" className="size-3 text-muted-foreground" />,
          }
        }),
      })
    }
    if (visibleCustomAgents.length > 0) {
      groups.push({
        id: 'custom',
        label: 'User agents',
        options: visibleCustomAgents.map((agent) => ({
          id: buildComposerAgentSelectionKey(
            runtimeAgentIdForCustomBaseCapability(agent.baseCapabilityProfile),
            agent.definitionId,
          ),
          label: agent.displayName,
          icon: <Package aria-hidden="true" className="size-3 text-primary" />,
          sublabel: agent.lifecycleState !== 'active' ? agent.lifecycleState : undefined,
        })),
      })
    }
    return groups
  }, [selectableRuntimeAgents, visibleCustomAgents])

  const modelGroups = useMemo<ComposerSelectGroup[]>(
    () =>
      composerModelGroups.map((group) => ({
        id: group.id,
        label: group.label,
        options: group.items.map((item) => ({ id: item.value, label: item.label })),
      })),
    [composerModelGroups],
  )

  const thinkingComposerOptions = useMemo<ComposerSelectOption[]>(
    () => composerThinkingOptions.map((option) => ({ id: option.value, label: option.label })),
    [composerThinkingOptions],
  )

  const approvalComposerOptions = useMemo<ComposerSelectOption[]>(
    () =>
      composerApprovalOptions.map((option) => ({
        id: option.value,
        label: option.label,
        sublabel: option.sublabel,
      })),
    [composerApprovalOptions],
  )

  const handleAgentChange = useCallback(
    (value: string) => {
      if (onComposerAgentSelectionChange) {
        onComposerAgentSelectionChange(value)
        return
      }
      if (value.startsWith('builtin:')) {
        const builtinId = value.slice('builtin:'.length) as RuntimeAgentIdDto
        if (selectableRuntimeAgents.some((agent) => agent.id === builtinId)) {
          onComposerRuntimeAgentChange(builtinId)
        }
      }
    },
    [onComposerAgentSelectionChange, onComposerRuntimeAgentChange, selectableRuntimeAgents],
  )

  const agentTooltip = runtimeAgentSwitchDisabled
    ? (runtimeAgentLockReason ?? 'Selected agent is fixed for the current run.')
    : isCustomAgent
      ? `${agentTriggerLabel} (user custom agent)`
      : `${agentTriggerLabel} agent`

  const composerError = runtimeRunActionError
    ? {
        title: runtimeRunActionErrorTitle,
        message: runtimeRunActionError.message,
        code: runtimeRunActionError.code,
      }
    : null

  const composer = (
    <Composer
      density={density}
      inSidebar={inSidebar}
      placeholder={placeholder}
      promptInputRef={promptInputRef}
      promptInputLabel={promptInputLabel}
      draftPrompt={draftPrompt}
      onDraftPromptChange={onDraftPromptChange}
      onSubmit={() => onSubmitDraftPrompt()}
      isPromptDisabled={isPromptDisabled}
      isSendDisabled={isSendDisabled}
      sendButtonLabel={sendButtonLabel}
      isSendLoading={isUpdatingControls || isStartingRun}
      isStopVisible={isStopVisible}
      isStopDisabled={isStopDisabled}
      onStop={onStopRuntimeRun}
      agentGroups={hideAgentSelector ? [] : agentGroups}
      selectedAgentId={agentSelectorValue}
      onAgentChange={handleAgentChange}
      agentDisabled={isAgentSelectorDisabled}
      agentTooltip={agentTooltip}
      agentTriggerIcon={AgentTriggerIcon ? <AgentTriggerIcon aria-hidden="true" className="size-3" /> : undefined}
      agentTriggerLabel={agentTriggerLabel}
      modelGroups={modelGroups}
      selectedModelId={composerModelId}
      onModelChange={handleComposerModelChange}
      modelDisabled={modelGroups.length === 0 || controlsDisabled}
      thinkingOptions={thinkingComposerOptions}
      selectedThinkingId={composerThinkingLevel}
      onThinkingChange={(value) => onComposerThinkingLevelChange(value as ProviderModelThinkingEffortDto)}
      thinkingDisabled={thinkingComposerOptions.length === 0 || controlsDisabled}
      thinkingPlaceholder={composerThinkingPlaceholder}
      approvalOptions={showApprovalSelector ? approvalComposerOptions : undefined}
      selectedApprovalId={composerApprovalMode}
      onApprovalChange={(value) => onComposerApprovalModeChange(value as RuntimeRunApprovalModeDto)}
      approvalDisabled={controlsDisabled}
      autoCompactEnabled={hideAutoCompact ? undefined : autoCompactEnabled}
      onAutoCompactEnabledChange={hideAutoCompact ? undefined : onAutoCompactEnabledChange}
      autoCompactDisabled={runtimeRunActionStatus === 'running'}
      pendingAttachments={pendingAttachments}
      onAddFiles={onAddFiles}
      onRemoveAttachment={onRemoveAttachment}
      dictation={hideDictation ? { ...dictation, isVisible: false } : dictation}
      dictationShortcut={bindings['composer.dictation']}
      contextMeter={hideContextMeter ? undefined : contextMeter}
      error={composerError}
      onOpenDiagnostics={onOpenDiagnostics}
    />
  )

  const dense = inSidebar || density === 'compact'

  return (
    <div className={cn('relative shrink-0 pt-0', dense ? 'px-0 pb-0' : 'px-4 pb-3')}>
      <div className={cn('mx-auto w-full', dense ? 'max-w-full' : 'max-w-[720px]')}>
        {composer}
      </div>
    </div>
  )
}
