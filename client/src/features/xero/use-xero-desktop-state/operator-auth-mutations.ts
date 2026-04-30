import { useCallback, useEffect, useRef } from 'react'

import {
  XeroDesktopError,
} from '@/src/lib/xero-desktop'
import { mapProviderAuthSession, mapRuntimeSession } from '@/src/lib/xero-model/runtime'

import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'
import {
  getActiveProjectId,
  getOperatorActionError,
} from './mutation-support'

const OPENAI_CALLBACK_INITIAL_POLL_DELAY_MS = 1_500
const OPENAI_CALLBACK_POLL_INTERVAL_MS = 750
const OPENAI_CALLBACK_POLL_TIMEOUT_MS = 120_000

type OpenAiCallbackPollTask = {
  cancelled: boolean
  timer: ReturnType<typeof setTimeout> | null
}

function isOpenAiCallbackPending(error: unknown): boolean {
  return error instanceof XeroDesktopError && error.code === 'authorization_code_pending'
}

export function useOperatorAuthMutations({
  adapter,
  refs,
  setters,
  operations,
}: UseXeroDesktopMutationsArgs): Pick<
  XeroDesktopMutationActions,
  | 'startOpenAiLogin'
  | 'submitOpenAiCallback'
  | 'startRuntimeSession'
  | 'logoutRuntimeSession'
  | 'resolveOperatorAction'
  | 'resumeOperatorRun'
> {
  const { activeProjectIdRef, activeProjectRef } = refs
  const {
    setErrorMessage,
    setOperatorActionStatus,
    setPendingOperatorActionId,
    setOperatorActionError,
  } = setters
  const {
    loadProject,
    syncRuntimeSession,
    applyRuntimeSessionUpdate,
  } = operations
  const openAiCallbackPollsRef = useRef<Map<string, OpenAiCallbackPollTask>>(new Map())

  useEffect(() => {
    return () => {
      for (const task of openAiCallbackPollsRef.current.values()) {
        task.cancelled = true
        if (task.timer !== null) {
          clearTimeout(task.timer)
        }
      }
      openAiCallbackPollsRef.current.clear()
    }
  }, [])

  const cancelOpenAiCallbackPoll = useCallback((flowId: string) => {
    const pollKey = flowId
    const task = openAiCallbackPollsRef.current.get(pollKey)
    if (!task) {
      return
    }

    task.cancelled = true
    if (task.timer !== null) {
      clearTimeout(task.timer)
    }
    openAiCallbackPollsRef.current.delete(pollKey)
  }, [])

  const cancelOpenAiCallbackPolls = useCallback(() => {
    for (const [pollKey, task] of openAiCallbackPollsRef.current.entries()) {
      task.cancelled = true
      if (task.timer !== null) {
        clearTimeout(task.timer)
      }
      openAiCallbackPollsRef.current.delete(pollKey)
    }
  }, [])

  const scheduleOpenAiCallbackCompletion = useCallback(
    (flowId: string) => {
      const pollKey = flowId
      if (openAiCallbackPollsRef.current.has(pollKey)) {
        return
      }

      const deadline = Date.now() + OPENAI_CALLBACK_POLL_TIMEOUT_MS
      const task: OpenAiCallbackPollTask = {
        cancelled: false,
        timer: null,
      }

      const finish = () => {
        task.cancelled = true
        if (task.timer !== null) {
          clearTimeout(task.timer)
          task.timer = null
        }
        openAiCallbackPollsRef.current.delete(pollKey)
      }

      const poll = async () => {
        task.timer = null
        if (task.cancelled) {
          return
        }

        try {
          await adapter.submitOpenAiCallback(flowId, {
            manualInput: null,
          })
          finish()
        } catch (error) {
          if (task.cancelled) {
            return
          }

          if (isOpenAiCallbackPending(error) && Date.now() < deadline) {
            task.timer = setTimeout(poll, OPENAI_CALLBACK_POLL_INTERVAL_MS)
            return
          }

          finish()
        }
      }

      openAiCallbackPollsRef.current.set(pollKey, task)
      task.timer = setTimeout(poll, OPENAI_CALLBACK_INITIAL_POLL_DELAY_MS)
    },
    [adapter],
  )

  const resolveOperatorAction = useCallback(
    async (
      actionId: string,
      decision: Parameters<XeroDesktopMutationActions['resolveOperatorAction']>[1],
      options: { userAnswer?: string | null } = {},
    ) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before resolving an operator action.',
      )

      setOperatorActionStatus('running')
      setPendingOperatorActionId(actionId)
      setOperatorActionError(null)
      setErrorMessage(null)

      try {
        await adapter.resolveOperatorAction(projectId, actionId, decision, {
          userAnswer: options.userAnswer ?? null,
        })
        await loadProject(projectId, 'operator:resolve')
        return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
      } catch (error) {
        setOperatorActionError(
          getOperatorActionError(
            error,
            'Xero could not persist the operator decision for this project.',
          ),
        )
        throw error
      } finally {
        setOperatorActionStatus('idle')
        setPendingOperatorActionId(null)
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      adapter,
      loadProject,
      setErrorMessage,
      setOperatorActionError,
      setOperatorActionStatus,
      setPendingOperatorActionId,
    ],
  )

  const resumeOperatorRun = useCallback(
    async (actionId: string, options: { userAnswer?: string | null } = {}) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before resuming the runtime session.',
      )

      setOperatorActionStatus('running')
      setPendingOperatorActionId(actionId)
      setOperatorActionError(null)
      setErrorMessage(null)

      try {
        await adapter.resumeOperatorRun(projectId, actionId, {
          userAnswer: options.userAnswer ?? null,
        })
        await loadProject(projectId, 'operator:resume')
        return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
      } catch (error) {
        setOperatorActionError(
          getOperatorActionError(
            error,
            'Xero could not record the operator resume request for this project.',
          ),
        )
        throw error
      } finally {
        setOperatorActionStatus('idle')
        setPendingOperatorActionId(null)
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      adapter,
      loadProject,
      setErrorMessage,
      setOperatorActionError,
      setOperatorActionStatus,
      setPendingOperatorActionId,
    ],
  )

  const startOpenAiLogin = useCallback(async (options: { originator?: string | null } = {}) => {
    const response = await adapter.startOpenAiLogin({
      originator: options.originator ?? 'pi',
    })
    const runtime = mapProviderAuthSession(response)
    if (
      runtime.providerId === 'openai_codex' &&
      runtime.flowId &&
      runtime.isLoginInProgress
    ) {
      scheduleOpenAiCallbackCompletion(runtime.flowId)
    }
    return runtime
  }, [
    adapter,
    scheduleOpenAiCallbackCompletion,
  ])

  const submitOpenAiCallback = useCallback(
    async (flowId: string, options: { manualInput?: string | null } = {}) => {
      cancelOpenAiCallbackPoll(flowId)
      const response = await adapter.submitOpenAiCallback(flowId, {
        manualInput: options.manualInput ?? null,
      })
      return mapProviderAuthSession(response)
    },
    [
      adapter,
      cancelOpenAiCallbackPoll,
    ],
  )

  const startRuntimeSession = useCallback(async (options: { providerProfileId?: string | null } = {}) => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before binding a runtime session.',
    )

    try {
      const response = await adapter.startRuntimeSession(projectId, {
        providerProfileId: options.providerProfileId ?? null,
      })
      return applyRuntimeSessionUpdate(mapRuntimeSession(response))
    } catch (error) {
      try {
        await syncRuntimeSession(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    }
  }, [activeProjectIdRef, adapter, applyRuntimeSessionUpdate, syncRuntimeSession])

  const logoutRuntimeSession = useCallback(async () => {
    const projectId = getActiveProjectId(activeProjectIdRef, 'Select an imported project before signing out.')
    cancelOpenAiCallbackPolls()

    try {
      const response = await adapter.logoutRuntimeSession(projectId)
      return applyRuntimeSessionUpdate(mapRuntimeSession(response))
    } catch (error) {
      try {
        await syncRuntimeSession(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    }
  }, [
    activeProjectIdRef,
    adapter,
    applyRuntimeSessionUpdate,
    cancelOpenAiCallbackPolls,
    syncRuntimeSession,
  ])

  return {
    startOpenAiLogin,
    submitOpenAiCallback,
    startRuntimeSession,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
  }
}
