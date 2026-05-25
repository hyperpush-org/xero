"use client"

import { useEffect, useMemo, useRef, useState } from 'react'

import type { AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type {
  AgentDefinitionSummaryDto,
  AgentDefaultModelDto,
  ProviderModelThinkingEffortDto,
  RuntimeAgentIdDto,
  RuntimeRunApprovalModeDto,
  RuntimeRunControlInputDto,
  RuntimeRunView,
  RuntimeSessionView,
  StagedAgentAttachmentDto,
} from '@/src/lib/xero-model'
import { BUILTIN_RUNTIME_AGENT_IDS } from '@/src/lib/xero-model'

import {
  buildComposerAgentSelectionKey,
  getComposerControlInput,
  getComposerModelOption,
  parseComposerAgentSelectionKey,
  resolveRuntimeAgentApprovalMode,
  resolveComposerThinkingSelection,
} from './composer-helpers'
import {
  useSpeechDictation,
  type SpeechDictationAdapter,
} from './use-speech-dictation'

export type OperatorIntentKind = 'approve' | 'reject' | 'resume'
export type ComposerThinkingLevel = ProviderModelThinkingEffortDto | null

export interface PendingOperatorIntent {
  actionId: string
  kind: OperatorIntentKind
}

interface UseAgentRuntimeControllerOptions {
  projectId: string
  selectedModelSelectionKey: string | null
  selectedRuntimeAgentId: RuntimeAgentIdDto
  selectedAgentDefinitionId?: string | null
  customAgentDefinitions?: readonly AgentDefinitionSummaryDto[]
  agentDefaultModels?: Readonly<Record<string, AgentDefaultModelDto | null | undefined>>
  selectedThinkingEffort: ComposerThinkingLevel
  selectedApprovalMode: RuntimeRunApprovalModeDto
  selectedAutoCompactEnabled: boolean
  selectedPrompt: AgentPaneView['selectedPrompt']
  availableModels: AgentPaneView['providerModelCatalog']['models']
  approvalRequests: AgentPaneView['approvalRequests']
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  renderableRuntimeRun: RuntimeRunView | null
  runtimeStream: AgentPaneView['runtimeStream'] | null
  runtimeStreamItems: NonNullable<AgentPaneView['runtimeStreamItems']>
  runtimeRunActionStatus: AgentPaneView['runtimeRunActionStatus']
  runtimeRunActionError: AgentPaneView['runtimeRunActionError']
  canStartRuntimeRun: boolean
  canStartRuntimeSession: boolean
  canStopRuntimeRun: boolean
  actionRequiredItems: NonNullable<AgentPaneView['actionRequiredItems']>
  dictationAdapter?: SpeechDictationAdapter
  dictationEnabled?: boolean
  dictationScopeKey: string
  reportComposerControls?: boolean
  /** Force the composer/run controls to a single runtime agent for session-scoped modes. */
  lockedRuntimeAgentId?: RuntimeAgentIdDto | null
  /**
   * One-shot runtime agent to apply to the composer when the controller mounts
   * (or when this value changes to a new non-null id). Used to open a session
   * with a specific agent pre-selected, e.g. "Create agent" entry points
   * landing the user on `agent_create`.
   */
  pendingInitialRuntimeAgentId?: RuntimeAgentIdDto | null
  pendingInitialAgentDefinitionId?: string | null
  /** Called once the pending initial runtime agent has been applied. */
  onPendingInitialRuntimeAgentIdConsumed?: () => void
  onStartRuntimeRun?: (options?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
    attachments?: StagedAgentAttachmentDto[]
  }) => Promise<RuntimeRunView | null>
  onStartRuntimeSession?: (options?: { providerProfileId?: string | null }) => Promise<RuntimeSessionView | null>
  onUpdateRuntimeRunControls?: (request?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
    attachments?: StagedAgentAttachmentDto[]
  }) => Promise<RuntimeRunView | null>
  /** Returns the staged attachments that should be sent with the next prompt. */
  getPendingAttachments?: () => StagedAgentAttachmentDto[]
  /** Called after a prompt+attachments submission succeeds, so the caller can clear chips. */
  onSubmitAttachmentsSettled?: () => void
  onComposerControlsChange?: (controls: RuntimeRunControlInputDto | null) => void
  onStopRuntimeRun?: (runId: string) => Promise<RuntimeRunView | null>
  onResolveOperatorAction?: (
    actionId: string,
    decision: 'approve' | 'reject',
    options?: { userAnswer?: string | null },
  ) => Promise<unknown>
  onResumeOperatorRun?: (actionId: string, options?: { userAnswer?: string | null }) => Promise<unknown>
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error && error.message.trim().length > 0) {
    return error.message
  }

  if (typeof error === 'string' && error.trim().length > 0) {
    return error
  }

  return fallback
}

const AUTO_COMPACT_STORAGE_KEY = 'xero.agent.autoCompact.enabled'
export const COMPOSER_SETTINGS_STORAGE_KEY = 'xero.agent.composer.settings.v1'
export const COMPOSER_SETTINGS_APP_STATE_KEY = COMPOSER_SETTINGS_STORAGE_KEY
const COMPOSER_SETTINGS_VERSION = 1
const COMPOSER_THINKING_LEVELS = ['none', 'minimal', 'low', 'medium', 'high', 'x_high'] as const
const COMPOSER_APPROVAL_MODES = ['suggest', 'auto_edit', 'yolo'] as const

interface StoredComposerSettings {
  modelSelectionKey?: string | null
  modelId?: string | null
  providerProfileId?: string | null
  runtimeAgentId?: RuntimeAgentIdDto
  agentDefinitionId?: string | null
  thinkingEffort?: ComposerThinkingLevel
  approvalMode?: RuntimeRunApprovalModeDto
  autoCompactEnabled?: boolean
}

interface StoredComposerSettingsReadResult {
  settings: StoredComposerSettings
  hasControlSettings: boolean
}

interface InitialComposerSettings {
  modelSelectionKey: string | null
  runtimeAgentId: RuntimeAgentIdDto
  agentDefinitionId: string | null
  thinkingEffort: ComposerThinkingLevel
  approvalMode: RuntimeRunApprovalModeDto
  autoCompactEnabled: boolean
  fromStoredControls: boolean
}

function sameRuntimeControlInput(
  left: RuntimeRunControlInputDto | null,
  right: RuntimeRunControlInputDto | null,
): boolean {
  if (left === right) return true
  if (!left || !right) return left === right

  return (
    (left.providerProfileId ?? null) === (right.providerProfileId ?? null) &&
    left.runtimeAgentId === right.runtimeAgentId &&
    (left.agentDefinitionId ?? null) === (right.agentDefinitionId ?? null) &&
    left.modelId === right.modelId &&
    (left.thinkingEffort ?? null) === (right.thinkingEffort ?? null) &&
    left.approvalMode === right.approvalMode &&
    Boolean(left.planModeRequired) === Boolean(right.planModeRequired) &&
    left.autoCompactEnabled === right.autoCompactEnabled
  )
}

function readStoredAutoCompactEnabled(): boolean {
  if (typeof window === 'undefined') return true

  try {
    const raw = window.localStorage.getItem(AUTO_COMPACT_STORAGE_KEY)
    if (raw === null) return true
    return raw === '1'
  } catch {
    return true
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function normalizeStoredText(value: unknown): string | null {
  if (typeof value !== 'string') return null

  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

function isRuntimeAgentId(value: unknown): value is RuntimeAgentIdDto {
  return (
    typeof value === 'string' &&
    BUILTIN_RUNTIME_AGENT_IDS.includes(value as RuntimeAgentIdDto)
  )
}

function isComposerThinkingLevel(value: unknown): value is ProviderModelThinkingEffortDto {
  return (
    typeof value === 'string' &&
    COMPOSER_THINKING_LEVELS.includes(value as ProviderModelThinkingEffortDto)
  )
}

function isComposerApprovalMode(value: unknown): value is RuntimeRunApprovalModeDto {
  return (
    typeof value === 'string' &&
    COMPOSER_APPROVAL_MODES.includes(value as RuntimeRunApprovalModeDto)
  )
}

function readStoredComposerSettings(): StoredComposerSettingsReadResult | null {
  if (typeof window === 'undefined') return null

  try {
    const raw = window.localStorage.getItem(COMPOSER_SETTINGS_STORAGE_KEY)
    if (!raw) return null

    const parsed: unknown = JSON.parse(raw)
    if (!isRecord(parsed) || parsed.version !== COMPOSER_SETTINGS_VERSION) {
      return null
    }

    const settings: StoredComposerSettings = {}
    const modelSelectionKey = normalizeStoredText(parsed.modelSelectionKey)
    const agentDefinitionId = normalizeStoredText(parsed.agentDefinitionId)
    let hasControlSettings = false

    if (modelSelectionKey) {
      settings.modelSelectionKey = modelSelectionKey
      hasControlSettings = true
    }
    const modelId = normalizeStoredText(parsed.modelId)
    if (modelId) {
      settings.modelId = modelId
      hasControlSettings = true
    }
    const providerProfileId = normalizeStoredText(parsed.providerProfileId)
    if (providerProfileId) {
      settings.providerProfileId = providerProfileId
    }
    if (isRuntimeAgentId(parsed.runtimeAgentId)) {
      settings.runtimeAgentId = parsed.runtimeAgentId
      hasControlSettings = true
    }
    if (agentDefinitionId) {
      settings.agentDefinitionId = agentDefinitionId
      hasControlSettings = true
    }
    if (isComposerThinkingLevel(parsed.thinkingEffort)) {
      settings.thinkingEffort = parsed.thinkingEffort
      hasControlSettings = true
    } else if (parsed.thinkingEffort === null) {
      settings.thinkingEffort = null
    }
    if (isComposerApprovalMode(parsed.approvalMode)) {
      settings.approvalMode = parsed.approvalMode
      hasControlSettings = true
    }
    if (typeof parsed.autoCompactEnabled === 'boolean') {
      settings.autoCompactEnabled = parsed.autoCompactEnabled
    }

    return { settings, hasControlSettings }
  } catch {
    return null
  }
}

function writeStoredComposerSettings(settings: Required<StoredComposerSettings>): void {
  if (typeof window === 'undefined') return

  try {
    window.localStorage.setItem(
      COMPOSER_SETTINGS_STORAGE_KEY,
      JSON.stringify({
        version: COMPOSER_SETTINGS_VERSION,
        modelSelectionKey: settings.modelSelectionKey,
        modelId: settings.modelId,
        providerProfileId: settings.providerProfileId,
        runtimeAgentId: settings.runtimeAgentId,
        agentDefinitionId: settings.agentDefinitionId,
        thinkingEffort: settings.thinkingEffort,
        approvalMode: settings.approvalMode,
        autoCompactEnabled: settings.autoCompactEnabled,
      }),
    )
    window.localStorage.setItem(AUTO_COMPACT_STORAGE_KEY, settings.autoCompactEnabled ? '1' : '0')
  } catch {
    /* storage unavailable, keep the in-memory preference */
  }
}

function storedModelSelectionKey(
  settings: StoredComposerSettings | undefined,
  fallbackSelectionKey: string | null,
  availableModels: AgentPaneView['providerModelCatalog']['models'],
): string | null {
  const storedSelectionKey = normalizeStoredText(settings?.modelSelectionKey)
  if (storedSelectionKey) return storedSelectionKey

  const storedModelId = normalizeStoredText(settings?.modelId)
  if (!storedModelId) return fallbackSelectionKey

  const storedProviderProfileId = normalizeStoredText(settings?.providerProfileId)
  const matchingModel =
    availableModels.find(
      (model) =>
        model.modelId === storedModelId &&
        (!storedProviderProfileId || model.profileId === storedProviderProfileId),
    ) ??
    availableModels.find((model) => model.modelId === storedModelId) ??
    null
  return matchingModel?.selectionKey ?? storedModelId
}

function defaultModelSelectionForAgent(
  runtimeAgentId: RuntimeAgentIdDto,
  agentDefinitionId: string | null,
  customAgentDefinitions: readonly AgentDefinitionSummaryDto[],
  agentDefaultModels: Readonly<Record<string, AgentDefaultModelDto | null | undefined>>,
  availableModels: AgentPaneView['providerModelCatalog']['models'],
): {
  selectionKey: string
  thinkingEffort: ComposerThinkingLevel
} | null {
  const trimmedDefinitionId = agentDefinitionId?.trim() ?? ''
  const selectionKey = buildComposerAgentSelectionKey(
    runtimeAgentId,
    trimmedDefinitionId.length > 0 ? trimmedDefinitionId : null,
  )
  const defaultModel =
    agentDefaultModels[selectionKey] ??
    (trimmedDefinitionId
      ? customAgentDefinitions.find((definition) => definition.definitionId === trimmedDefinitionId)
          ?.defaultModel
      : null)
  if (!defaultModel?.modelId?.trim()) return null

  const configuredKey =
    defaultModel.selectionKey?.trim() ||
    `${defaultModel.providerId}:${defaultModel.modelId.trim()}`
  const matchingModel =
    availableModels.find(
      (model) =>
        model.selectionKey === configuredKey ||
        (
          model.providerId === defaultModel.providerId &&
          model.modelId === defaultModel.modelId &&
          (
            !defaultModel.providerProfileId ||
            !model.profileId ||
            model.profileId === defaultModel.providerProfileId
          )
        ),
    ) ?? getComposerModelOption(availableModels, configuredKey)
  const thinkingEffort =
    defaultModel.thinkingEffort &&
    matchingModel?.thinkingSupported &&
    matchingModel.thinkingEffortOptions.includes(defaultModel.thinkingEffort)
      ? defaultModel.thinkingEffort
      : matchingModel?.defaultThinkingEffort ?? null

  return {
    selectionKey: matchingModel?.selectionKey ?? configuredKey,
    thinkingEffort,
  }
}

export function useAgentRuntimeController({
  projectId,
  selectedModelSelectionKey,
  selectedRuntimeAgentId,
  selectedAgentDefinitionId = null,
  customAgentDefinitions = [],
  agentDefaultModels = {},
  selectedThinkingEffort,
  selectedApprovalMode,
  selectedAutoCompactEnabled,
  selectedPrompt,
  availableModels,
  approvalRequests,
  operatorActionStatus,
  pendingOperatorActionId,
  renderableRuntimeRun,
  runtimeStream,
  runtimeStreamItems,
  runtimeRunActionStatus,
  runtimeRunActionError,
  canStartRuntimeRun,
  canStartRuntimeSession,
  canStopRuntimeRun,
  actionRequiredItems,
  dictationAdapter,
  dictationEnabled = true,
  dictationScopeKey,
  reportComposerControls = true,
  lockedRuntimeAgentId = null,
  pendingInitialRuntimeAgentId = null,
  pendingInitialAgentDefinitionId = null,
  onPendingInitialRuntimeAgentIdConsumed,
  onStartRuntimeRun,
  onStartRuntimeSession,
  onUpdateRuntimeRunControls,
  onComposerControlsChange,
  onStopRuntimeRun,
  onResolveOperatorAction,
  onResumeOperatorRun,
  getPendingAttachments,
  onSubmitAttachmentsSettled,
}: UseAgentRuntimeControllerOptions) {
  const initialComposerSettingsRef = useRef<InitialComposerSettings | null>(null)
  const getInitialComposerSettings = () => {
    if (!initialComposerSettingsRef.current) {
      const stored = readStoredComposerSettings()
      const storedRuntimeAgentId =
        lockedRuntimeAgentId ??
        (
          stored?.settings.runtimeAgentId && stored.settings.runtimeAgentId !== 'computer_use'
            ? stored.settings.runtimeAgentId
            : selectedRuntimeAgentId
        )
      initialComposerSettingsRef.current = {
        modelSelectionKey: storedModelSelectionKey(
          stored?.settings,
          selectedModelSelectionKey,
          availableModels,
        ),
        runtimeAgentId: storedRuntimeAgentId,
        agentDefinitionId: lockedRuntimeAgentId
          ? null
          : stored?.settings.agentDefinitionId ?? selectedAgentDefinitionId ?? null,
        thinkingEffort:
          stored && 'thinkingEffort' in stored.settings
            ? stored.settings.thinkingEffort ?? null
            : selectedThinkingEffort,
        approvalMode: resolveRuntimeAgentApprovalMode(
          storedRuntimeAgentId,
          stored?.settings.approvalMode ?? selectedApprovalMode,
        ),
        autoCompactEnabled: stored?.settings.autoCompactEnabled ?? readStoredAutoCompactEnabled(),
        fromStoredControls: lockedRuntimeAgentId ? false : stored?.hasControlSettings ?? false,
      }
    }

    return initialComposerSettingsRef.current
  }
  const [draftPrompt, setDraftPrompt] = useState('')
  const [draftModelSelectionKey, setDraftModelSelectionKey] = useState<string | null>(
    () => getInitialComposerSettings().modelSelectionKey,
  )
  const [draftRuntimeAgentId, setDraftRuntimeAgentId] = useState<RuntimeAgentIdDto>(
    () => getInitialComposerSettings().runtimeAgentId,
  )
  const [draftAgentDefinitionId, setDraftAgentDefinitionId] = useState<string | null>(
    () => getInitialComposerSettings().agentDefinitionId,
  )
  const [draftThinkingEffort, setDraftThinkingEffort] = useState<ComposerThinkingLevel>(
    () => getInitialComposerSettings().thinkingEffort,
  )
  const [draftApprovalMode, setDraftApprovalMode] = useState<RuntimeRunApprovalModeDto>(
    () => getInitialComposerSettings().approvalMode,
  )
  const [runtimeSessionBindInFlight, setRuntimeSessionBindInFlight] = useState(false)
  const [queuedDraftAcknowledgement, setQueuedDraftAcknowledgement] = useState<string | null>(null)
  const [runtimeRunActionMessage, setRuntimeRunActionMessage] = useState<string | null>(null)
  const [autoCompactEnabled, setAutoCompactEnabled] = useState(
    () => getInitialComposerSettings().autoCompactEnabled,
  )
  const [operatorAnswers, setOperatorAnswers] = useState<Record<string, string>>({})
  const [pendingOperatorIntent, setPendingOperatorIntent] = useState<PendingOperatorIntent | null>(null)
  const [recentRunReplacement, setRecentRunReplacement] = useState<{
    previousRunId: string
    nextRunId: string
  } | null>(null)
  const promptInputRef = useRef<HTMLTextAreaElement | null>(null)

  const lastSeenProjectIdRef = useRef(projectId)
  const lastSeenRuntimeRunIdRef = useRef<string | null>(renderableRuntimeRun?.runId ?? null)
  const draftPromptRef = useRef(draftPrompt)
  const lastReportedComposerControlsRef = useRef<RuntimeRunControlInputDto | null | undefined>(undefined)
  const hasUserComposerSettingsRef = useRef(getInitialComposerSettings().fromStoredControls)

  const activeRuntimeRun = renderableRuntimeRun && !renderableRuntimeRun.isTerminal ? renderableRuntimeRun : null
  const effectiveRuntimeAgentId =
    lockedRuntimeAgentId ?? (activeRuntimeRun ? selectedRuntimeAgentId : draftRuntimeAgentId)
  const effectiveAgentDefinitionId = lockedRuntimeAgentId
    ? null
    : activeRuntimeRun
      ? selectedAgentDefinitionId ?? null
      : draftAgentDefinitionId
  const effectiveModelSelectionKey = activeRuntimeRun ? selectedModelSelectionKey : draftModelSelectionKey
  const effectiveThinkingEffort = activeRuntimeRun ? selectedThinkingEffort : draftThinkingEffort
  const effectiveApprovalMode = resolveRuntimeAgentApprovalMode(
    effectiveRuntimeAgentId,
    activeRuntimeRun ? selectedApprovalMode : draftApprovalMode,
  )
  const effectiveAutoCompactEnabled = activeRuntimeRun ? selectedAutoCompactEnabled : autoCompactEnabled
  const composerAgentSelectionKey = buildComposerAgentSelectionKey(
    effectiveRuntimeAgentId,
    effectiveAgentDefinitionId,
  )
  const selectedControlInput = useMemo(
    () =>
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        agentDefinitionId: effectiveAgentDefinitionId,
        models: availableModels,
        selectionKey: effectiveModelSelectionKey,
        thinkingEffort: effectiveThinkingEffort,
        approvalMode: effectiveApprovalMode,
        autoCompactEnabled: effectiveAutoCompactEnabled,
      }),
    [
      availableModels,
      effectiveAgentDefinitionId,
      effectiveApprovalMode,
      effectiveAutoCompactEnabled,
      effectiveModelSelectionKey,
      effectiveRuntimeAgentId,
      effectiveThinkingEffort,
    ],
  )

  const trimmedDraftPrompt = draftPrompt.trim()
  const hasQueuedPrompt = selectedPrompt.hasQueuedPrompt
  const canPrepareFirstRun = Boolean(
    !renderableRuntimeRun &&
      (canStartRuntimeRun || canStartRuntimeSession) &&
      onStartRuntimeRun,
  )
  const canStartReplacementRun = Boolean(
    renderableRuntimeRun?.isTerminal &&
      (canStartRuntimeRun || canStartRuntimeSession) &&
      onStartRuntimeRun,
  )
  const canStartNewRuntimeRun = canPrepareFirstRun || canStartReplacementRun
  const runtimeMutationInFlight = runtimeRunActionStatus === 'running' || runtimeSessionBindInFlight
  const promptInputAvailable = Boolean(
    canStartNewRuntimeRun ||
      (activeRuntimeRun && onUpdateRuntimeRunControls),
  )
  const isPromptDisabled = !promptInputAvailable || runtimeMutationInFlight
  const areControlsDisabled = Boolean(
    runtimeMutationInFlight ||
      (activeRuntimeRun ? !onUpdateRuntimeRunControls : !canStartNewRuntimeRun),
  )
  const isRuntimeAgentSwitchDisabled = Boolean(
    lockedRuntimeAgentId ||
      runtimeMutationInFlight ||
      (activeRuntimeRun ? !onUpdateRuntimeRunControls : !canStartNewRuntimeRun),
  )
  const canSubmitPrompt = Boolean(
    !runtimeMutationInFlight &&
      !hasQueuedPrompt &&
      trimmedDraftPrompt.length > 0 &&
      (canStartNewRuntimeRun ||
        (activeRuntimeRun &&
          onUpdateRuntimeRunControls)),
  )
  const dictation = useSpeechDictation({
    adapter: dictationAdapter,
    enabled: dictationEnabled,
    scopeKey: dictationScopeKey,
    draftPrompt,
    setDraftPrompt,
    promptInputDisabled: isPromptDisabled,
    promptInputRef,
  })

  useEffect(() => {
    if (!reportComposerControls) {
      lastReportedComposerControlsRef.current = undefined
      return
    }

    if (
      lastReportedComposerControlsRef.current !== undefined &&
      sameRuntimeControlInput(lastReportedComposerControlsRef.current, selectedControlInput)
    ) {
      return
    }

    lastReportedComposerControlsRef.current = selectedControlInput
    onComposerControlsChange?.(selectedControlInput)
  }, [onComposerControlsChange, reportComposerControls, selectedControlInput])

  useEffect(() => {
    draftPromptRef.current = draftPrompt
  }, [draftPrompt])

  useEffect(() => {
    if (lockedRuntimeAgentId) {
      setDraftRuntimeAgentId(lockedRuntimeAgentId)
      setDraftAgentDefinitionId(null)
      setDraftApprovalMode((current) =>
        resolveRuntimeAgentApprovalMode(lockedRuntimeAgentId, current),
      )
      return
    }

    if (activeRuntimeRun || !hasUserComposerSettingsRef.current) {
      setDraftModelSelectionKey(selectedModelSelectionKey)
      setDraftRuntimeAgentId(selectedRuntimeAgentId)
      setDraftAgentDefinitionId(selectedAgentDefinitionId ?? null)
      setDraftThinkingEffort(selectedThinkingEffort)
      setDraftApprovalMode(selectedApprovalMode)
    }
  }, [
    activeRuntimeRun,
    lockedRuntimeAgentId,
    projectId,
    selectedAgentDefinitionId,
    selectedApprovalMode,
    selectedModelSelectionKey,
    selectedRuntimeAgentId,
    selectedThinkingEffort,
  ])

  useEffect(() => {
    if (lockedRuntimeAgentId) {
      return
    }

    const modelSelectionKey = normalizeStoredText(effectiveModelSelectionKey)
    const shouldPersistComposerControls =
      hasUserComposerSettingsRef.current ||
      Boolean(activeRuntimeRun && modelSelectionKey)
    if (!shouldPersistComposerControls) {
      return
    }
    if (activeRuntimeRun && modelSelectionKey) {
      hasUserComposerSettingsRef.current = true
    }

    writeStoredComposerSettings({
      modelSelectionKey,
      modelId: normalizeStoredText(selectedControlInput?.modelId),
      providerProfileId: normalizeStoredText(selectedControlInput?.providerProfileId),
      runtimeAgentId: effectiveRuntimeAgentId,
      agentDefinitionId: normalizeStoredText(effectiveAgentDefinitionId),
      thinkingEffort: effectiveThinkingEffort,
      approvalMode: effectiveApprovalMode,
      autoCompactEnabled: effectiveAutoCompactEnabled,
    })
  }, [
    activeRuntimeRun,
    effectiveAutoCompactEnabled,
    effectiveAgentDefinitionId,
    effectiveApprovalMode,
    effectiveModelSelectionKey,
    effectiveRuntimeAgentId,
    effectiveThinkingEffort,
    lockedRuntimeAgentId,
    selectedControlInput?.modelId,
    selectedControlInput?.providerProfileId,
  ])

  useEffect(() => {
    if (lockedRuntimeAgentId) {
      if (pendingInitialRuntimeAgentId) {
        onPendingInitialRuntimeAgentIdConsumed?.()
      }
      return
    }
    if (!pendingInitialRuntimeAgentId) return
    if (activeRuntimeRun) return
    if (runtimeMutationInFlight) return

    const trimmedAgentDefinitionId = pendingInitialAgentDefinitionId?.trim() ?? ''
    const defaultModelSelection = defaultModelSelectionForAgent(
      pendingInitialRuntimeAgentId,
      trimmedAgentDefinitionId.length > 0 ? trimmedAgentDefinitionId : null,
      customAgentDefinitions,
      agentDefaultModels,
      availableModels,
    )
    setDraftRuntimeAgentId(pendingInitialRuntimeAgentId)
    setDraftAgentDefinitionId(
      trimmedAgentDefinitionId.length > 0 ? trimmedAgentDefinitionId : null,
    )
    if (defaultModelSelection) {
      setDraftModelSelectionKey(defaultModelSelection.selectionKey)
      setDraftThinkingEffort(defaultModelSelection.thinkingEffort)
    }
    setDraftApprovalMode((current) =>
      resolveRuntimeAgentApprovalMode(pendingInitialRuntimeAgentId, current),
    )
    onPendingInitialRuntimeAgentIdConsumed?.()
  }, [
    activeRuntimeRun,
    availableModels,
    agentDefaultModels,
    customAgentDefinitions,
    onPendingInitialRuntimeAgentIdConsumed,
    lockedRuntimeAgentId,
    pendingInitialAgentDefinitionId,
    pendingInitialRuntimeAgentId,
    runtimeMutationInFlight,
  ])

  useEffect(() => {
    if (runtimeRunActionError) {
      setRuntimeRunActionMessage(null)
      return
    }

    if (renderableRuntimeRun?.updatedAt) {
      setRuntimeRunActionMessage(null)
    }
  }, [renderableRuntimeRun?.runId, renderableRuntimeRun?.updatedAt, runtimeRunActionError])

  useEffect(() => {
    if (!queuedDraftAcknowledgement) {
      return
    }

    if (selectedPrompt.hasQueuedPrompt && selectedPrompt.text === queuedDraftAcknowledgement) {
      setDraftPrompt((currentDraft) => (currentDraft === queuedDraftAcknowledgement ? '' : currentDraft))
      setQueuedDraftAcknowledgement(null)
    }
  }, [queuedDraftAcknowledgement, selectedPrompt])

  useEffect(() => {
    if (operatorActionStatus === 'idle' && !pendingOperatorActionId) {
      setPendingOperatorIntent(null)
    }
  }, [operatorActionStatus, pendingOperatorActionId])

  useEffect(() => {
    setOperatorAnswers((currentAnswers) => {
      const nextAnswers: Record<string, string> = {}

      for (const approval of approvalRequests) {
        const existingAnswer = currentAnswers[approval.actionId]
        if (typeof existingAnswer === 'string') {
          nextAnswers[approval.actionId] = existingAnswer
          continue
        }

        if (approval.userAnswer) {
          nextAnswers[approval.actionId] = approval.userAnswer
        }
      }

      for (const actionRequired of actionRequiredItems) {
        const existingAnswer = currentAnswers[actionRequired.actionId]
        if (typeof existingAnswer === 'string') {
          nextAnswers[actionRequired.actionId] = existingAnswer
        }
      }

      const currentKeys = Object.keys(currentAnswers)
      const nextKeys = Object.keys(nextAnswers)
      if (
        currentKeys.length === nextKeys.length &&
        nextKeys.every((actionId) => nextAnswers[actionId] === currentAnswers[actionId])
      ) {
        return currentAnswers
      }

      return nextAnswers
    })
  }, [actionRequiredItems, approvalRequests])

  useEffect(() => {
    if (lastSeenProjectIdRef.current !== projectId) {
      lastSeenProjectIdRef.current = projectId
      lastSeenRuntimeRunIdRef.current = renderableRuntimeRun?.runId ?? null
      setDraftPrompt('')
      setQueuedDraftAcknowledgement(null)
      setRecentRunReplacement(null)
      return
    }

    const previousRunId = lastSeenRuntimeRunIdRef.current
    const nextRunId = renderableRuntimeRun?.runId ?? null

    if (previousRunId && nextRunId && previousRunId !== nextRunId) {
      setRecentRunReplacement({ previousRunId, nextRunId })
    }

    lastSeenRuntimeRunIdRef.current = nextRunId
  }, [projectId, renderableRuntimeRun?.runId])

  useEffect(() => {
    if (!recentRunReplacement) {
      return
    }

    const currentRunId = renderableRuntimeRun?.runId ?? null
    const hasFreshItemsForReplacementRun =
      currentRunId === recentRunReplacement.nextRunId &&
      runtimeStream?.runId === recentRunReplacement.nextRunId &&
      runtimeStreamItems.some((item) => item.runId === recentRunReplacement.nextRunId)

    if (!currentRunId || currentRunId !== recentRunReplacement.nextRunId || hasFreshItemsForReplacementRun) {
      setRecentRunReplacement(null)
    }
  }, [recentRunReplacement, renderableRuntimeRun?.runId, runtimeStream?.runId, runtimeStreamItems])

  const resolvedRuntimeRunActionError =
    runtimeRunActionError ??
    (runtimeRunActionMessage
      ? {
          code: 'runtime_run_action_failed',
          message: runtimeRunActionMessage,
          retryable: false,
        }
      : null)

  const composerActionError = resolvedRuntimeRunActionError ?? dictation.error

  const runtimeRunActionErrorTitle =
    resolvedRuntimeRunActionError?.retryable || resolvedRuntimeRunActionError?.code.includes('timeout')
      ? 'Run control needs retry'
      : 'Run control failed'
  const composerActionErrorTitle = resolvedRuntimeRunActionError
    ? runtimeRunActionErrorTitle
    : 'Dictation unavailable'

  async function queueRuntimeRunControls(nextControls: RuntimeRunControlInputDto | null) {
    if (!renderableRuntimeRun || renderableRuntimeRun.isTerminal || !onUpdateRuntimeRunControls || !nextControls) {
      return
    }

    if (runtimeRunActionStatus === 'running') {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onUpdateRuntimeRunControls({ controls: nextControls })
    } catch (error) {
      setRuntimeRunActionMessage(getErrorMessage(error, 'Xero could not queue the requested run control change.'))
    }
  }

  function clearSubmittedDraft() {
    draftPromptRef.current = ''
    setDraftPrompt('')
  }

  async function handleStartRuntimeRun(): Promise<boolean> {
    if (!onStartRuntimeRun || (!canStartRuntimeRun && !canStartRuntimeSession)) {
      return false
    }

    setRuntimeRunActionMessage(null)

    try {
      if (!canStartRuntimeRun && canStartRuntimeSession) {
        if (!onStartRuntimeSession) {
          return false
        }

        setRuntimeSessionBindInFlight(true)
        const boundRuntimeSession = await onStartRuntimeSession({
          providerProfileId: selectedControlInput?.providerProfileId ?? null,
        })
        setRuntimeSessionBindInFlight(false)

        if (!boundRuntimeSession?.isAuthenticated) {
          const message = boundRuntimeSession?.isLoginInProgress
            ? 'Finish provider sign-in, then send again.'
            : boundRuntimeSession?.lastError?.message?.trim() ||
              'Xero could not authenticate the configured provider. Check the provider setup and try again.'
          setRuntimeRunActionMessage(message)
          return false
        }
      }

      if (!(await dictation.stopBeforeSubmit())) {
        return false
      }

      const promptToSubmit = draftPromptRef.current.trim()
      const attachmentsToSubmit = (getPendingAttachments?.() ?? []).filter(
        (attachment) => attachment.absolutePath != null,
      )
      await onStartRuntimeRun({
        controls: selectedControlInput,
        prompt: promptToSubmit.length > 0 ? promptToSubmit : null,
        attachments: attachmentsToSubmit.length > 0 ? attachmentsToSubmit : undefined,
      })
      if (promptToSubmit.length > 0 || attachmentsToSubmit.length > 0) {
        clearSubmittedDraft()
        if (promptToSubmit.length > 0) {
          setQueuedDraftAcknowledgement(promptToSubmit)
        }
        onSubmitAttachmentsSettled?.()
      }
      return true
    } catch (error) {
      setQueuedDraftAcknowledgement(null)
      setRuntimeRunActionMessage(getErrorMessage(error, 'Xero could not start the agent run.'))
      setRuntimeSessionBindInFlight(false)
      return false
    }
  }

  async function handleSubmitDraftPrompt(): Promise<boolean> {
    if (!activeRuntimeRun) {
      return handleStartRuntimeRun()
    }

    if (!onUpdateRuntimeRunControls) {
      return false
    }

    if (trimmedDraftPrompt.length === 0 || hasQueuedPrompt || runtimeRunActionStatus === 'running') {
      return false
    }

    setRuntimeRunActionMessage(null)

    try {
      if (!(await dictation.stopBeforeSubmit())) {
        return false
      }

      const promptToSubmit = draftPromptRef.current.trim()
      if (promptToSubmit.length === 0) {
        return false
      }

      const attachmentsToSubmit = (getPendingAttachments?.() ?? []).filter(
        (attachment) => attachment.absolutePath != null,
      )
      await onUpdateRuntimeRunControls({
        prompt: promptToSubmit,
        ...(attachmentsToSubmit.length > 0 ? { attachments: attachmentsToSubmit } : {}),
      })
      clearSubmittedDraft()
      setQueuedDraftAcknowledgement(promptToSubmit)
      onSubmitAttachmentsSettled?.()
      return true
    } catch (error) {
      setQueuedDraftAcknowledgement(null)
      setRuntimeRunActionMessage(getErrorMessage(error, 'Xero could not queue the next prompt for this agent run.'))
      return false
    }
  }

  async function handleStopRuntimeRun() {
    if (!canStopRuntimeRun || !onStopRuntimeRun || !renderableRuntimeRun) {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onStopRuntimeRun(renderableRuntimeRun.runId)
    } catch (error) {
      setRuntimeRunActionMessage(getErrorMessage(error, 'Xero could not stop the agent run.'))
    }
  }

  function handleDraftPromptChange(value: string) {
    draftPromptRef.current = value
    setDraftPrompt(value)
  }

  function handleAutoCompactEnabledChange(value: boolean) {
    hasUserComposerSettingsRef.current = true
    setAutoCompactEnabled(value)
    if (!activeRuntimeRun) {
      return
    }
    void queueRuntimeRunControls(
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        agentDefinitionId: effectiveAgentDefinitionId,
        models: availableModels,
        selectionKey: effectiveModelSelectionKey,
        thinkingEffort: effectiveThinkingEffort,
        approvalMode: effectiveApprovalMode,
        autoCompactEnabled: value,
      }),
    )
  }

  function handleComposerModelChange(value: string) {
    if (!activeRuntimeRun) {
      const selectedModel = getComposerModelOption(availableModels, value)
      hasUserComposerSettingsRef.current = true
      setDraftModelSelectionKey(value)
      setDraftThinkingEffort(resolveComposerThinkingSelection(selectedModel, draftThinkingEffort))
      return
    }

    void queueRuntimeRunControls(
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        agentDefinitionId: effectiveAgentDefinitionId,
        models: availableModels,
        selectionKey: value,
        thinkingEffort: selectedThinkingEffort,
        approvalMode: selectedApprovalMode,
        autoCompactEnabled: effectiveAutoCompactEnabled,
      }),
    )
  }

  function handleComposerThinkingLevelChange(value: ProviderModelThinkingEffortDto) {
    const selectedModel = getComposerModelOption(availableModels, effectiveModelSelectionKey)
    if (!selectedModel?.thinkingSupported || !selectedModel.thinkingEffortOptions.includes(value)) {
      return
    }

    if (!activeRuntimeRun) {
      hasUserComposerSettingsRef.current = true
      setDraftThinkingEffort(value)
      return
    }

    void queueRuntimeRunControls(
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        agentDefinitionId: effectiveAgentDefinitionId,
        models: availableModels,
        selectionKey: effectiveModelSelectionKey,
        thinkingEffort: value,
        approvalMode: effectiveApprovalMode,
        autoCompactEnabled: effectiveAutoCompactEnabled,
      }),
    )
  }

  function handleComposerApprovalModeChange(value: RuntimeRunApprovalModeDto) {
    if (resolveRuntimeAgentApprovalMode(effectiveRuntimeAgentId, value) !== value) {
      return
    }

    if (!activeRuntimeRun) {
      hasUserComposerSettingsRef.current = true
      setDraftApprovalMode(value)
      return
    }

    void queueRuntimeRunControls(
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        agentDefinitionId: effectiveAgentDefinitionId,
        models: availableModels,
        selectionKey: effectiveModelSelectionKey,
        thinkingEffort: effectiveThinkingEffort,
        approvalMode: value,
        autoCompactEnabled: effectiveAutoCompactEnabled,
      }),
    )
  }

  function handleComposerRuntimeAgentChange(value: RuntimeAgentIdDto) {
    if (lockedRuntimeAgentId || isRuntimeAgentSwitchDisabled) {
      return
    }

    if (activeRuntimeRun) {
      void queueRuntimeRunControls(
        getComposerControlInput({
          runtimeAgentId: value,
          agentDefinitionId: null,
          models: availableModels,
          selectionKey: effectiveModelSelectionKey,
          thinkingEffort: effectiveThinkingEffort,
          approvalMode: resolveRuntimeAgentApprovalMode(value, effectiveApprovalMode),
          autoCompactEnabled: effectiveAutoCompactEnabled,
        }),
      )
      return
    }

    hasUserComposerSettingsRef.current = true
    setDraftRuntimeAgentId(value)
    setDraftAgentDefinitionId(null)
    setDraftApprovalMode((current) => resolveRuntimeAgentApprovalMode(value, current))
  }

  function handleComposerAgentSelectionChange(selectionKey: string) {
    if (lockedRuntimeAgentId || isRuntimeAgentSwitchDisabled) {
      return
    }

    const selection = parseComposerAgentSelectionKey(selectionKey, customAgentDefinitions)
    if (!selection) {
      return
    }

    hasUserComposerSettingsRef.current = true
    const defaultModelSelection = defaultModelSelectionForAgent(
      selection.runtimeAgentId,
      selection.agentDefinitionId,
      customAgentDefinitions,
      agentDefaultModels,
      availableModels,
    )
    if (activeRuntimeRun) {
      void queueRuntimeRunControls(
        getComposerControlInput({
          runtimeAgentId: selection.runtimeAgentId,
          agentDefinitionId: selection.agentDefinitionId,
          models: availableModels,
          selectionKey: effectiveModelSelectionKey,
          thinkingEffort: effectiveThinkingEffort,
          approvalMode: resolveRuntimeAgentApprovalMode(
            selection.runtimeAgentId,
            effectiveApprovalMode,
          ),
          autoCompactEnabled: effectiveAutoCompactEnabled,
        }),
      )
      return
    }

    setDraftRuntimeAgentId(selection.runtimeAgentId)
    setDraftAgentDefinitionId(selection.agentDefinitionId)
    if (defaultModelSelection) {
      setDraftModelSelectionKey(defaultModelSelection.selectionKey)
      setDraftThinkingEffort(defaultModelSelection.thinkingEffort)
    }
    setDraftApprovalMode((current) =>
      resolveRuntimeAgentApprovalMode(selection.runtimeAgentId, current),
    )
  }

  async function handleResolveOperatorAction(
    actionId: string,
    decision: 'approve' | 'reject',
    options: { userAnswer?: string | null } = {},
  ) {
    if (!onResolveOperatorAction) {
      return
    }

    setPendingOperatorIntent({ actionId, kind: decision })

    try {
      await onResolveOperatorAction(actionId, decision, {
        userAnswer: options.userAnswer ?? null,
      })
    } catch {
      // Preserve the last truthful UI state. Hook-backed callers surface operatorActionError.
    } finally {
      setPendingOperatorIntent((currentIntent) =>
        currentIntent?.actionId === actionId && currentIntent.kind === decision ? null : currentIntent,
      )
    }
  }

  async function handleResumeOperatorRun(actionId: string, options: { userAnswer?: string | null } = {}) {
    if (!onResumeOperatorRun) {
      return
    }

    setPendingOperatorIntent({ actionId, kind: 'resume' })

    try {
      await onResumeOperatorRun(actionId, {
        userAnswer: options.userAnswer ?? null,
      })
    } catch {
      // Preserve the last truthful UI state. Hook-backed callers surface operatorActionError.
    } finally {
      setPendingOperatorIntent((currentIntent) =>
        currentIntent?.actionId === actionId && currentIntent.kind === 'resume' ? null : currentIntent,
      )
    }
  }

  async function handleResumeLiveActionRequired(actionId: string, options: { userAnswer?: string | null } = {}) {
    if (!renderableRuntimeRun || renderableRuntimeRun.isTerminal || !onUpdateRuntimeRunControls) {
      return
    }

    if (runtimeRunActionStatus === 'running') {
      return
    }

    const userAnswer = options.userAnswer?.trim() ?? ''
    if (userAnswer.length === 0) {
      return
    }

    setRuntimeRunActionMessage(null)
    setPendingOperatorIntent({ actionId, kind: 'resume' })

    try {
      await onUpdateRuntimeRunControls({ prompt: userAnswer })
    } catch (error) {
      setRuntimeRunActionMessage(getErrorMessage(error, 'Xero could not send the owned-agent response.'))
    } finally {
      setPendingOperatorIntent((currentIntent) =>
        currentIntent?.actionId === actionId && currentIntent.kind === 'resume' ? null : currentIntent,
      )
    }
  }

  function handleOperatorAnswerChange(actionId: string, value: string) {
    setOperatorAnswers((currentAnswers) => ({
      ...currentAnswers,
      [actionId]: value,
    }))
  }

  return {
    draftPrompt,
    autoCompactEnabled: effectiveAutoCompactEnabled,
    composerModelId: effectiveModelSelectionKey,
    composerRuntimeAgentId: effectiveRuntimeAgentId,
    composerAgentDefinitionId: effectiveAgentDefinitionId,
    composerAgentSelectionKey,
    composerThinkingEffort: effectiveThinkingEffort,
    composerApprovalMode: effectiveApprovalMode,
    promptInputAvailable,
    isPromptDisabled,
    areControlsDisabled,
    isRuntimeAgentSwitchDisabled,
    canSubmitPrompt,
    canStopRuntimeRun,
    runtimeSessionBindInFlight,
    operatorAnswers,
    pendingOperatorIntent,
    recentRunReplacement,
    runtimeRunActionError: composerActionError,
    runtimeRunActionErrorTitle: composerActionErrorTitle,
    dictation,
    promptInputRef,
    handleDraftPromptChange,
    handleAutoCompactEnabledChange,
    handleSubmitDraftPrompt,
    handleComposerModelChange,
    handleComposerRuntimeAgentChange,
    handleComposerAgentSelectionChange,
    handleComposerThinkingLevelChange,
    handleComposerApprovalModeChange,
    handleOperatorAnswerChange,
    handleStartRuntimeRun,
    handleStopRuntimeRun,
    handleResolveOperatorAction,
    handleResumeOperatorRun,
    handleResumeLiveActionRequired,
  }
}
