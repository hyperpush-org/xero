"use client"

import { useEffect, useMemo, useRef, useState } from 'react'

import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type {
  ProviderModelThinkingEffortDto,
  RuntimeRunApprovalModeDto,
  RuntimeRunControlInputDto,
  RuntimeRunView,
} from '@/src/lib/cadence-model'

import { getComposerControlInput, getComposerModelOption } from './composer-helpers'

export type OperatorIntentKind = 'approve' | 'reject' | 'resume'
export type ComposerThinkingLevel = ProviderModelThinkingEffortDto | null

export interface PendingOperatorIntent {
  actionId: string
  kind: OperatorIntentKind
}

interface UseAgentRuntimeControllerOptions {
  projectId: string
  selectedModelId: string | null
  selectedThinkingEffort: ComposerThinkingLevel
  selectedApprovalMode: RuntimeRunApprovalModeDto
  selectedPrompt: AgentPaneView['selectedPrompt']
  availableModels: AgentPaneView['providerModelCatalog']['models']
  approvalRequests: AgentPaneView['approvalRequests']
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  pendingRuntimeRunAction: AgentPaneView['pendingRuntimeRunAction']
  renderableRuntimeRun: RuntimeRunView | null
  runtimeRunPendingControls: AgentPaneView['runtimeRunPendingControls']
  runtimeStream: AgentPaneView['runtimeStream'] | null
  runtimeStreamItems: NonNullable<AgentPaneView['runtimeStreamItems']>
  runtimeRunActionStatus: AgentPaneView['runtimeRunActionStatus']
  runtimeRunActionError: AgentPaneView['runtimeRunActionError']
  canStartRuntimeRun: boolean
  canStopRuntimeRun: boolean
  onStartRuntimeRun?: (options?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
  }) => Promise<RuntimeRunView | null>
  onUpdateRuntimeRunControls?: (request?: {
    controls?: RuntimeRunControlInputDto | null
    prompt?: string | null
  }) => Promise<RuntimeRunView | null>
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

export function useAgentRuntimeController({
  projectId,
  selectedModelId,
  selectedThinkingEffort,
  selectedApprovalMode,
  selectedPrompt,
  availableModels,
  approvalRequests,
  operatorActionStatus,
  pendingOperatorActionId,
  pendingRuntimeRunAction,
  renderableRuntimeRun,
  runtimeRunPendingControls,
  runtimeStream,
  runtimeStreamItems,
  runtimeRunActionStatus,
  runtimeRunActionError,
  canStartRuntimeRun,
  canStopRuntimeRun,
  onStartRuntimeRun,
  onUpdateRuntimeRunControls,
  onStopRuntimeRun,
  onResolveOperatorAction,
  onResumeOperatorRun,
}: UseAgentRuntimeControllerOptions) {
  const [draftPrompt, setDraftPrompt] = useState('')
  const [queuedDraftAcknowledgement, setQueuedDraftAcknowledgement] = useState<string | null>(null)
  const [runtimeRunActionMessage, setRuntimeRunActionMessage] = useState<string | null>(null)
  const [operatorAnswers, setOperatorAnswers] = useState<Record<string, string>>({})
  const [pendingOperatorIntent, setPendingOperatorIntent] = useState<PendingOperatorIntent | null>(null)
  const [recentRunReplacement, setRecentRunReplacement] = useState<{
    previousRunId: string
    nextRunId: string
  } | null>(null)

  const lastSeenProjectIdRef = useRef(projectId)
  const lastSeenRuntimeRunIdRef = useRef<string | null>(renderableRuntimeRun?.runId ?? null)

  const selectedControlInput = useMemo(
    () =>
      getComposerControlInput({
        models: availableModels,
        modelId: selectedModelId,
        thinkingEffort: selectedThinkingEffort,
        approvalMode: selectedApprovalMode,
      }),
    [availableModels, selectedApprovalMode, selectedModelId, selectedThinkingEffort],
  )

  const trimmedDraftPrompt = draftPrompt.trim()
  const hasQueuedPrompt = selectedPrompt.hasQueuedPrompt
  const promptInputAvailable = Boolean(
    (!renderableRuntimeRun && canStartRuntimeRun) || (renderableRuntimeRun && !renderableRuntimeRun.isTerminal),
  )
  const isPromptDisabled = !promptInputAvailable || runtimeRunActionStatus === 'running'
  const areControlsDisabled = Boolean(
    !renderableRuntimeRun ||
      renderableRuntimeRun.isTerminal ||
      !onUpdateRuntimeRunControls ||
      runtimeRunActionStatus === 'running' ||
      runtimeRunPendingControls,
  )
  const canSubmitPrompt = Boolean(
    renderableRuntimeRun &&
      !renderableRuntimeRun.isTerminal &&
      onUpdateRuntimeRunControls &&
      runtimeRunActionStatus !== 'running' &&
      !hasQueuedPrompt &&
      trimmedDraftPrompt.length > 0,
  )

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
  }, [approvalRequests])

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

  const runtimeRunActionErrorTitle =
    resolvedRuntimeRunActionError?.retryable || resolvedRuntimeRunActionError?.code.includes('timeout')
      ? 'Run control needs retry'
      : 'Run control failed'

  async function queueRuntimeRunControls(nextControls: RuntimeRunControlInputDto | null) {
    if (!renderableRuntimeRun || renderableRuntimeRun.isTerminal || !onUpdateRuntimeRunControls || !nextControls) {
      return
    }

    if (runtimeRunActionStatus === 'running' || runtimeRunPendingControls) {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onUpdateRuntimeRunControls({ controls: nextControls })
    } catch (error) {
      setRuntimeRunActionMessage(getErrorMessage(error, 'Cadence could not queue the requested run control change.'))
    }
  }

  async function handleStartRuntimeRun() {
    if (!canStartRuntimeRun || !onStartRuntimeRun) {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onStartRuntimeRun({
        controls: selectedControlInput,
        prompt: trimmedDraftPrompt.length > 0 ? trimmedDraftPrompt : null,
      })
      if (trimmedDraftPrompt.length > 0) {
        setQueuedDraftAcknowledgement(trimmedDraftPrompt)
      }
    } catch (error) {
      setQueuedDraftAcknowledgement(null)
      setRuntimeRunActionMessage(getErrorMessage(error, 'Cadence could not start the supervised run.'))
    }
  }

  async function handleSubmitDraftPrompt() {
    if (!renderableRuntimeRun || renderableRuntimeRun.isTerminal || !onUpdateRuntimeRunControls) {
      return
    }

    if (trimmedDraftPrompt.length === 0 || hasQueuedPrompt || runtimeRunActionStatus === 'running') {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onUpdateRuntimeRunControls({ prompt: trimmedDraftPrompt })
      setQueuedDraftAcknowledgement(trimmedDraftPrompt)
    } catch (error) {
      setQueuedDraftAcknowledgement(null)
      setRuntimeRunActionMessage(getErrorMessage(error, 'Cadence could not queue the next prompt for this supervised run.'))
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
      setRuntimeRunActionMessage(getErrorMessage(error, 'Cadence could not stop the supervised run.'))
    }
  }

  function handleDraftPromptChange(value: string) {
    setDraftPrompt(value)
  }

  function handleComposerModelChange(value: string) {
    void queueRuntimeRunControls(
      getComposerControlInput({
        models: availableModels,
        modelId: value,
        thinkingEffort: selectedThinkingEffort,
        approvalMode: selectedApprovalMode,
      }),
    )
  }

  function handleComposerThinkingLevelChange(value: ProviderModelThinkingEffortDto) {
    const selectedModel = getComposerModelOption(availableModels, selectedModelId)
    if (!selectedModel?.thinkingSupported || !selectedModel.thinkingEffortOptions.includes(value)) {
      return
    }

    void queueRuntimeRunControls(
      getComposerControlInput({
        models: availableModels,
        modelId: selectedModelId,
        thinkingEffort: value,
        approvalMode: selectedApprovalMode,
      }),
    )
  }

  function handleComposerApprovalModeChange(value: RuntimeRunApprovalModeDto) {
    void queueRuntimeRunControls(
      getComposerControlInput({
        models: availableModels,
        modelId: selectedModelId,
        thinkingEffort: selectedThinkingEffort,
        approvalMode: value,
      }),
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

  function handleOperatorAnswerChange(actionId: string, value: string) {
    setOperatorAnswers((currentAnswers) => ({
      ...currentAnswers,
      [actionId]: value,
    }))
  }

  return {
    draftPrompt,
    promptInputAvailable,
    isPromptDisabled,
    areControlsDisabled,
    canSubmitPrompt,
    operatorAnswers,
    pendingOperatorIntent,
    recentRunReplacement,
    runtimeRunActionError: resolvedRuntimeRunActionError,
    runtimeRunActionErrorTitle,
    handleDraftPromptChange,
    handleSubmitDraftPrompt,
    handleComposerModelChange,
    handleComposerThinkingLevelChange,
    handleComposerApprovalModeChange,
    handleOperatorAnswerChange,
    handleStartRuntimeRun,
    handleStopRuntimeRun,
    handleResolveOperatorAction,
    handleResumeOperatorRun,
  }
}
