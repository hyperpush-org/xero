import {
  Bug,
  Compass,
  ListChecks,
  MessageCircle,
  Monitor,
  Package,
  Search,
  Sparkles,
  Workflow,
  Wrench,
} from 'lucide-react'
import { openUrl } from '@tauri-apps/plugin-opener'
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode, type RefObject } from 'react'

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
import type { AgentAttachmentCompatibilityProfile } from '@/lib/agent-attachments'
import {
  Composer,
  CreditLimitNotice,
  type ComposerContextMentionOption,
  type ComposerContextMentionStatus,
  type ComposerPendingContext,
  type ComposerSelectGroup,
  type ComposerSelectOption,
} from '@xero/ui/components/composer'
import type { CreditLimitNoticeView } from '@xero/ui/model/credit-limit'
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
export type {
  ComposerContextMentionOption,
  ComposerPendingContext,
} from '@xero/ui/components/composer'

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

export interface ComposerWorkflowOption {
  id: string
  label: string
  sublabel?: string
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
  workflowOptions?: readonly ComposerWorkflowOption[]
  selectedWorkflowOptionId?: string | null
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
  /**
   * When the latest run failed because the provider is out of credits / hit a
   * spending limit, this holds the classified billing card to dock above the
   * composer (instead of a generic error). `null` when there is no such failure.
   */
  creditLimitNotice?: CreditLimitNoticeView | null
  dictation: ComposerDictationControl
  hideDictation?: boolean
  contextMeter?: ReactNode
  hideContextMeter?: boolean
  pendingAttachments?: ComposerPendingAttachment[]
  pendingContexts?: ComposerPendingContext[]
  attachmentCompatibility?: AgentAttachmentCompatibilityProfile | null
  onAddFiles?: (files: File[]) => void
  onAddFolders?: () => void
  onRemoveAttachment?: (id: string) => void
  onRemoveContext?: (id: string) => void
  contextMentionOptions?: readonly ComposerContextMentionOption[]
  contextMentionStatus?: ComposerContextMentionStatus
  contextMentionError?: string | null
  onContextMentionQueryChange?: (query: string | null) => void
  onSelectContextMention?: (option: ComposerContextMentionOption) => void
  onOpenDiagnostics?: () => void
  onDraftPromptChange: (value: string) => void
  onSubmitDraftPrompt: () => void
  onStopRuntimeRun?: () => void
  onAutoCompactEnabledChange: (value: boolean) => void
  onComposerRuntimeAgentChange: (value: RuntimeAgentIdDto) => void
  onComposerAgentSelectionChange?: (selectionKey: string) => void
  onComposerWorkflowSelectionChange?: (workflowId: string | null) => void
  onComposerModelChange: (value: string) => void
  onComposerThinkingLevelChange: (value: ProviderModelThinkingEffortDto) => void
  onComposerApprovalModeChange: (value: RuntimeRunApprovalModeDto) => void
}

const WORKFLOW_SELECTION_PREFIX = 'workflow:'
const EMPTY_WORKFLOW_OPTIONS: readonly ComposerWorkflowOption[] = []

function buildComposerWorkflowSelectionKey(workflowId: string): string {
  return `${WORKFLOW_SELECTION_PREFIX}${workflowId}`
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
  workflowOptions,
  selectedWorkflowOptionId = null,
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
  creditLimitNotice = null,
  dictation,
  hideDictation = false,
  contextMeter,
  hideContextMeter = false,
  pendingAttachments,
  pendingContexts,
  attachmentCompatibility,
  onAddFiles,
  onAddFolders,
  onRemoveAttachment,
  onRemoveContext,
  contextMentionOptions,
  contextMentionStatus,
  contextMentionError,
  onContextMentionQueryChange,
  onSelectContextMention,
  onOpenDiagnostics,
  onDraftPromptChange,
  onSubmitDraftPrompt,
  onStopRuntimeRun,
  onAutoCompactEnabledChange,
  onComposerRuntimeAgentChange,
  onComposerAgentSelectionChange,
  onComposerWorkflowSelectionChange,
  onComposerModelChange,
  onComposerThinkingLevelChange,
  onComposerApprovalModeChange,
}: ComposerDockProps) {
  const { bindings } = useShortcuts()
  // Controlled open-state for the model picker so the credit-limit card's
  // "Switch model" button can open it externally.
  const [modelSelectOpen, setModelSelectOpen] = useState(false)
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
  const visibleWorkflowOptions = workflowOptions ?? EMPTY_WORKFLOW_OPTIONS
  const activeWorkflow = useMemo(
    () =>
      selectedWorkflowOptionId
        ? visibleWorkflowOptions.find((workflow) => workflow.id === selectedWorkflowOptionId) ?? null
        : null,
    [selectedWorkflowOptionId, visibleWorkflowOptions],
  )
  const workflowSelectionEnabled = Boolean(
    workflowOptions || selectedWorkflowOptionId || onComposerWorkflowSelectionChange,
  )
  const isCustomAgent = Boolean(activeCustomAgent)
  const isWorkflowSelected = Boolean(activeWorkflow)
  const agentTriggerLabel =
    activeWorkflow?.label ?? activeCustomAgent?.displayName ?? composerRuntimeAgentLabel
  const agentSelectorValue = activeWorkflow
    ? buildComposerWorkflowSelectionKey(activeWorkflow.id)
    : composerAgentSelectionKey ??
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
  const AgentTriggerIcon = isWorkflowSelected
    ? Workflow
    : isCustomAgent
      ? Package
      : getBuiltinAgentIcon(composerRuntimeAgentId)

  const agentGroups = useMemo<ComposerSelectGroup[]>(() => {
    const builtinOptions = selectableRuntimeAgents.map((agent) => {
      const BuiltinAgentIcon = getBuiltinAgentIcon(agent.id)
      return {
        id: buildComposerAgentSelectionKey(agent.id, null),
        label: agent.label,
        icon: (
          <BuiltinAgentIcon aria-hidden="true" className="size-3 text-muted-foreground" />
        ),
      }
    })
    const customOptions = visibleCustomAgents.map((agent) => ({
      id: buildComposerAgentSelectionKey(
        runtimeAgentIdForCustomBaseCapability(agent.baseCapabilityProfile),
        agent.definitionId,
      ),
      label: agent.displayName,
      icon: <Package aria-hidden="true" className="size-3 text-primary" />,
      sublabel: 'User',
    }))

    if (workflowSelectionEnabled) {
      return [
        {
          id: 'agents',
          label: 'Agents',
          options: [...builtinOptions, ...customOptions],
        },
        {
          id: 'workflows',
          label: 'Workflows',
          options: visibleWorkflowOptions.map((workflow) => ({
            id: buildComposerWorkflowSelectionKey(workflow.id),
            label: workflow.label,
            icon: <Workflow aria-hidden="true" className="size-3 text-primary" />,
            sublabel: workflow.sublabel,
          })),
        },
      ]
    }

    const groups: ComposerSelectGroup[] = []
    if (builtinOptions.length > 0) groups.push({ id: 'builtin', options: builtinOptions })
    if (customOptions.length > 0) {
      groups.push({ id: 'custom', label: 'User agents', options: customOptions })
    }
    return groups
  }, [
    selectableRuntimeAgents,
    visibleCustomAgents,
    visibleWorkflowOptions,
    workflowSelectionEnabled,
  ])

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
      if (value.startsWith(WORKFLOW_SELECTION_PREFIX)) {
        const workflowId = value.slice(WORKFLOW_SELECTION_PREFIX.length)
        if (visibleWorkflowOptions.some((workflow) => workflow.id === workflowId)) {
          onComposerWorkflowSelectionChange?.(workflowId)
        }
        return
      }
      onComposerWorkflowSelectionChange?.(null)
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
    [
      onComposerAgentSelectionChange,
      onComposerRuntimeAgentChange,
      onComposerWorkflowSelectionChange,
      selectableRuntimeAgents,
      visibleWorkflowOptions,
    ],
  )

  const agentTooltip = runtimeAgentSwitchDisabled
    ? (runtimeAgentLockReason ?? 'Selected agent is fixed for the current run.')
    : activeWorkflow
      ? `${activeWorkflow.label} Workflow`
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
      allowEmptySubmit={isWorkflowSelected}
      sendButtonLabel={sendButtonLabel}
      isSendLoading={isUpdatingControls || isStartingRun}
      isStopVisible={isStopVisible}
      isStopDisabled={isStopDisabled}
      onStop={onStopRuntimeRun}
      agentGroups={hideAgentSelector ? [] : agentGroups}
      selectedAgentId={agentSelectorValue}
      onAgentChange={handleAgentChange}
      agentSelectorAriaLabel={
        workflowSelectionEnabled ? 'Agent or Workflow selector' : 'Agent selector'
      }
      agentDisabled={isAgentSelectorDisabled}
      agentTooltip={agentTooltip}
      agentTriggerIcon={AgentTriggerIcon ? <AgentTriggerIcon aria-hidden="true" className="size-3" /> : undefined}
      agentTriggerLabel={agentTriggerLabel}
      modelGroups={modelGroups}
      selectedModelId={composerModelId}
      onModelChange={handleComposerModelChange}
      modelDisabled={modelGroups.length === 0 || controlsDisabled}
      modelSelectOpen={modelSelectOpen}
      onModelSelectOpenChange={setModelSelectOpen}
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
      pendingContexts={pendingContexts}
      attachmentCompatibility={attachmentCompatibility}
      onAddFiles={onAddFiles}
      onAddFolders={onAddFolders}
      onRemoveAttachment={onRemoveAttachment}
      onRemoveContext={onRemoveContext}
      contextMentionOptions={contextMentionOptions}
      contextMentionStatus={contextMentionStatus}
      contextMentionError={contextMentionError}
      onContextMentionQueryChange={onContextMentionQueryChange}
      onSelectContextMention={onSelectContextMention}
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
      <div className={cn('mx-auto flex w-full flex-col gap-2', dense ? 'max-w-full' : 'max-w-[720px]')}>
        {creditLimitNotice ? (
          <CreditLimitNotice
            notice={creditLimitNotice}
            onOpenLink={(url) => {
              void openUrl(url).catch(() => {
                // The opener plugin surfaces its own errors; swallow to keep the
                // click handler synchronous.
              })
            }}
            onSwitchModel={
              modelGroups.length === 0 || controlsDisabled
                ? undefined
                : () => setModelSelectOpen(true)
            }
          />
        ) : null}
        {composer}
      </div>
    </div>
  )
}
