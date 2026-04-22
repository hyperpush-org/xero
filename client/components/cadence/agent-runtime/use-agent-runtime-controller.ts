"use client"

import { useEffect, useMemo, useRef, useState } from 'react'

import type { AgentPaneView } from '@/src/features/cadence/use-cadence-desktop-state'
import type { ProviderModelThinkingEffortDto, RuntimeRunView } from '@/src/lib/cadence-model'

import { getComposerModelOption, resolveComposerThinkingSelection } from './composer-helpers'

export type OperatorIntentKind = 'approve' | 'reject' | 'resume'
export type ComposerThinkingLevel = ProviderModelThinkingEffortDto | null

export interface PendingOperatorIntent {
  actionId: string
  kind: OperatorIntentKind
}

interface UseAgentRuntimeControllerOptions {
  projectId: string
  selectedModelId: string | null
  availableModels: AgentPaneView['providerModelCatalog']['models']
  approvalRequests: AgentPaneView['approvalRequests']
  operatorActionStatus: AgentPaneView['operatorActionStatus']
  pendingOperatorActionId: string | null
  renderableRuntimeRun: RuntimeRunView | null
  runtimeStream: AgentPaneView['runtimeStream'] | null
  runtimeStreamItems: NonNullable<AgentPaneView['runtimeStreamItems']>
  runtimeRunActionError: AgentPaneView['runtimeRunActionError']
  canStartRuntimeRun: boolean
  canStopRuntimeRun: boolean
  onStartRuntimeRun?: () => Promise<RuntimeRunView | null>
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
  availableModels,
  approvalRequests,
  operatorActionStatus,
  pendingOperatorActionId,
  renderableRuntimeRun,
  runtimeStream,
  runtimeStreamItems,
  runtimeRunActionError,
  canStartRuntimeRun,
  canStopRuntimeRun,
  onStartRuntimeRun,
  onStopRuntimeRun,
  onResolveOperatorAction,
  onResumeOperatorRun,
}: UseAgentRuntimeControllerOptions) {
  const normalizedSelectedModelId = selectedModelId?.trim() || null
  const externalSelectionKey = `${projectId}::${normalizedSelectedModelId ?? ''}`
  const externalSelectedModel = useMemo(
    () => getComposerModelOption(availableModels, normalizedSelectedModelId),
    [availableModels, normalizedSelectedModelId],
  )

  const [composerModelId, setComposerModelId] = useState<string | null>(normalizedSelectedModelId)
  const [composerThinkingLevel, setComposerThinkingLevel] = useState<ComposerThinkingLevel>(() =>
    resolveComposerThinkingSelection(externalSelectedModel, null),
  )
  const [runtimeRunActionMessage, setRuntimeRunActionMessage] = useState<string | null>(null)
  const [operatorAnswers, setOperatorAnswers] = useState<Record<string, string>>({})
  const [pendingOperatorIntent, setPendingOperatorIntent] = useState<PendingOperatorIntent | null>(null)
  const [recentRunReplacement, setRecentRunReplacement] = useState<{
    previousRunId: string
    nextRunId: string
  } | null>(null)

  const lastSeenProjectIdRef = useRef(projectId)
  const lastSeenRuntimeRunIdRef = useRef<string | null>(renderableRuntimeRun?.runId ?? null)
  const lastExternalSelectionKeyRef = useRef<string | null>(null)

  const selectedComposerModel = useMemo(
    () => getComposerModelOption(availableModels, composerModelId),
    [availableModels, composerModelId],
  )

  useEffect(() => {
    if (lastExternalSelectionKeyRef.current === externalSelectionKey) {
      return
    }

    lastExternalSelectionKeyRef.current = externalSelectionKey
    setComposerModelId(normalizedSelectedModelId)
    setComposerThinkingLevel(resolveComposerThinkingSelection(externalSelectedModel, null))
  }, [externalSelectionKey, externalSelectedModel, normalizedSelectedModelId])

  useEffect(() => {
    setComposerThinkingLevel((currentThinkingLevel) =>
      resolveComposerThinkingSelection(selectedComposerModel, currentThinkingLevel),
    )
  }, [selectedComposerModel])

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

  async function handleStartRuntimeRun() {
    if (!canStartRuntimeRun || !onStartRuntimeRun) {
      return
    }

    setRuntimeRunActionMessage(null)

    try {
      await onStartRuntimeRun()
    } catch (error) {
      setRuntimeRunActionMessage(getErrorMessage(error, 'Cadence could not start the supervised run.'))
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
    composerModelId,
    composerThinkingLevel,
    setComposerModelId,
    setComposerThinkingLevel,
    operatorAnswers,
    pendingOperatorIntent,
    recentRunReplacement,
    runtimeRunActionError: resolvedRuntimeRunActionError,
    runtimeRunActionErrorTitle,
    handleOperatorAnswerChange,
    handleStartRuntimeRun,
    handleStopRuntimeRun,
    handleResolveOperatorAction,
    handleResumeOperatorRun,
  }
}
