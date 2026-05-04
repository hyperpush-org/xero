"use client"

import { useEffect, useMemo, useRef, useState } from 'react'

import type { AgentPaneView } from '@/src/features/xero/use-xero-desktop-state'
import type {
  AgentDefinitionSummaryDto,
  ProviderModelThinkingEffortDto,
  RuntimeAgentIdDto,
  RuntimeAutoCompactPreferenceDto,
  RuntimeRunApprovalModeDto,
  RuntimeRunControlInputDto,
  RuntimeRunView,
  RuntimeSessionView,
  StagedAgentAttachmentDto,
} from '@/src/lib/xero-model'

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
  selectedThinkingEffort: ComposerThinkingLevel
  selectedApprovalMode: RuntimeRunApprovalModeDto
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
    autoCompact?: RuntimeAutoCompactPreferenceDto | null
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
const AUTO_COMPACT_DEFAULT_PREFERENCE: RuntimeAutoCompactPreferenceDto = {
  enabled: true,
  thresholdPercent: 85,
  rawTailMessageCount: 8,
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
    Boolean(left.planModeRequired) === Boolean(right.planModeRequired)
  )
}

function readStoredAutoCompactEnabled(): boolean {
  if (typeof window === 'undefined') return false

  try {
    return window.localStorage.getItem(AUTO_COMPACT_STORAGE_KEY) === '1'
  } catch {
    return false
  }
}

export function useAgentRuntimeController({
  projectId,
  selectedModelSelectionKey,
  selectedRuntimeAgentId,
  selectedAgentDefinitionId = null,
  customAgentDefinitions = [],
  selectedThinkingEffort,
  selectedApprovalMode,
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
  const [draftPrompt, setDraftPrompt] = useState('')
  const [draftModelSelectionKey, setDraftModelSelectionKey] = useState<string | null>(selectedModelSelectionKey)
  const [draftRuntimeAgentId, setDraftRuntimeAgentId] = useState<RuntimeAgentIdDto>(selectedRuntimeAgentId)
  const [draftAgentDefinitionId, setDraftAgentDefinitionId] = useState<string | null>(
    selectedAgentDefinitionId ?? null,
  )
  const [draftThinkingEffort, setDraftThinkingEffort] = useState<ComposerThinkingLevel>(selectedThinkingEffort)
  const [draftApprovalMode, setDraftApprovalMode] = useState<RuntimeRunApprovalModeDto>(selectedApprovalMode)
  const [runtimeSessionBindInFlight, setRuntimeSessionBindInFlight] = useState(false)
  const [queuedDraftAcknowledgement, setQueuedDraftAcknowledgement] = useState<string | null>(null)
  const [runtimeRunActionMessage, setRuntimeRunActionMessage] = useState<string | null>(null)
  const [autoCompactEnabled, setAutoCompactEnabled] = useState(readStoredAutoCompactEnabled)
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

  const activeRuntimeRun = renderableRuntimeRun && !renderableRuntimeRun.isTerminal ? renderableRuntimeRun : null
  const effectiveRuntimeAgentId = activeRuntimeRun ? selectedRuntimeAgentId : draftRuntimeAgentId
  const effectiveAgentDefinitionId = activeRuntimeRun
    ? selectedAgentDefinitionId ?? null
    : draftAgentDefinitionId
  const effectiveModelSelectionKey = activeRuntimeRun ? selectedModelSelectionKey : draftModelSelectionKey
  const effectiveThinkingEffort = activeRuntimeRun ? selectedThinkingEffort : draftThinkingEffort
  const effectiveApprovalMode = resolveRuntimeAgentApprovalMode(
    effectiveRuntimeAgentId,
    activeRuntimeRun ? selectedApprovalMode : draftApprovalMode,
  )
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
      }),
    [
      availableModels,
      effectiveAgentDefinitionId,
      effectiveApprovalMode,
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
  const isRuntimeAgentSwitchDisabled = Boolean(activeRuntimeRun || runtimeMutationInFlight)
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
    if (typeof window === 'undefined') return

    try {
      window.localStorage.setItem(AUTO_COMPACT_STORAGE_KEY, autoCompactEnabled ? '1' : '0')
    } catch {
      /* storage unavailable, keep the in-memory preference */
    }
  }, [autoCompactEnabled])

  useEffect(() => {
    if (renderableRuntimeRun) {
      setDraftModelSelectionKey(selectedModelSelectionKey)
      setDraftRuntimeAgentId(selectedRuntimeAgentId)
      setDraftAgentDefinitionId(selectedAgentDefinitionId ?? null)
      setDraftThinkingEffort(selectedThinkingEffort)
      setDraftApprovalMode(selectedApprovalMode)
      return
    }

    setDraftModelSelectionKey(selectedModelSelectionKey)
    setDraftRuntimeAgentId(selectedRuntimeAgentId)
    setDraftAgentDefinitionId(selectedAgentDefinitionId ?? null)
    setDraftThinkingEffort(selectedThinkingEffort)
    setDraftApprovalMode(selectedApprovalMode)
  }, [
    projectId,
    renderableRuntimeRun?.runId,
    selectedAgentDefinitionId,
    selectedApprovalMode,
    selectedModelSelectionKey,
    selectedRuntimeAgentId,
    selectedThinkingEffort,
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

  async function handleStartRuntimeRun() {
    if (!onStartRuntimeRun || (!canStartRuntimeRun && !canStartRuntimeSession)) {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      if (!canStartRuntimeRun && canStartRuntimeSession) {
        if (!onStartRuntimeSession) {
          return
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
          return
        }
      }

      if (!(await dictation.stopBeforeSubmit())) {
        return
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
    } catch (error) {
      setQueuedDraftAcknowledgement(null)
      setRuntimeRunActionMessage(getErrorMessage(error, 'Xero could not start the agent run.'))
      setRuntimeSessionBindInFlight(false)
    }
  }

  async function handleSubmitDraftPrompt() {
    if (!activeRuntimeRun) {
      await handleStartRuntimeRun()
      return
    }

    if (!onUpdateRuntimeRunControls) {
      return
    }

    if (trimmedDraftPrompt.length === 0 || hasQueuedPrompt || runtimeRunActionStatus === 'running') {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      if (!(await dictation.stopBeforeSubmit())) {
        return
      }

      const promptToSubmit = draftPromptRef.current.trim()
      if (promptToSubmit.length === 0) {
        return
      }

      const attachmentsToSubmit = (getPendingAttachments?.() ?? []).filter(
        (attachment) => attachment.absolutePath != null,
      )
      await onUpdateRuntimeRunControls({
        prompt: promptToSubmit,
        ...(attachmentsToSubmit.length > 0 ? { attachments: attachmentsToSubmit } : {}),
        ...(autoCompactEnabled ? { autoCompact: AUTO_COMPACT_DEFAULT_PREFERENCE } : {}),
      })
      clearSubmittedDraft()
      setQueuedDraftAcknowledgement(promptToSubmit)
      onSubmitAttachmentsSettled?.()
    } catch (error) {
      setQueuedDraftAcknowledgement(null)
      setRuntimeRunActionMessage(getErrorMessage(error, 'Xero could not queue the next prompt for this agent run.'))
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
    setAutoCompactEnabled(value)
  }

  function handleComposerModelChange(value: string) {
    if (!activeRuntimeRun) {
      const selectedModel = getComposerModelOption(availableModels, value)
      setDraftModelSelectionKey(value)
      setDraftThinkingEffort(resolveComposerThinkingSelection(selectedModel, draftThinkingEffort))
      return
    }

    void queueRuntimeRunControls(
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        models: availableModels,
        selectionKey: value,
        thinkingEffort: selectedThinkingEffort,
        approvalMode: selectedApprovalMode,
      }),
    )
  }

  function handleComposerThinkingLevelChange(value: ProviderModelThinkingEffortDto) {
    const selectedModel = getComposerModelOption(availableModels, effectiveModelSelectionKey)
    if (!selectedModel?.thinkingSupported || !selectedModel.thinkingEffortOptions.includes(value)) {
      return
    }

    if (!activeRuntimeRun) {
      setDraftThinkingEffort(value)
      return
    }

    void queueRuntimeRunControls(
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        models: availableModels,
        selectionKey: effectiveModelSelectionKey,
        thinkingEffort: value,
        approvalMode: effectiveApprovalMode,
      }),
    )
  }

  function handleComposerApprovalModeChange(value: RuntimeRunApprovalModeDto) {
    if (resolveRuntimeAgentApprovalMode(effectiveRuntimeAgentId, value) !== value) {
      return
    }

    if (!activeRuntimeRun) {
      setDraftApprovalMode(value)
      return
    }

    void queueRuntimeRunControls(
      getComposerControlInput({
        runtimeAgentId: effectiveRuntimeAgentId,
        models: availableModels,
        selectionKey: effectiveModelSelectionKey,
        thinkingEffort: effectiveThinkingEffort,
        approvalMode: value,
      }),
    )
  }

  function handleComposerRuntimeAgentChange(value: RuntimeAgentIdDto) {
    if (isRuntimeAgentSwitchDisabled) {
      return
    }

    setDraftRuntimeAgentId(value)
    setDraftAgentDefinitionId(null)
    setDraftApprovalMode((current) => resolveRuntimeAgentApprovalMode(value, current))
  }

  function handleComposerAgentSelectionChange(selectionKey: string) {
    if (isRuntimeAgentSwitchDisabled) {
      return
    }

    const selection = parseComposerAgentSelectionKey(selectionKey, customAgentDefinitions)
    if (!selection) {
      return
    }

    setDraftRuntimeAgentId(selection.runtimeAgentId)
    setDraftAgentDefinitionId(selection.agentDefinitionId)
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
    autoCompactEnabled,
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
