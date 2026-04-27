import { useCallback, useEffect, useRef } from 'react'

import {
  CadenceDesktopError,
} from '@/src/lib/cadence-desktop'
import { mapRuntimeSession } from '@/src/lib/cadence-model/runtime'

import type {
  CadenceDesktopMutationActions,
  UseCadenceDesktopMutationsArgs,
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
  return error instanceof CadenceDesktopError && error.code === 'authorization_code_pending'
}

function getSelectedProfileId(selectedProfileId: string | null | undefined, action: string): string {
  const profileId = selectedProfileId?.trim() ?? ''
  if (profileId.length > 0) {
    return profileId
  }

  throw new CadenceDesktopError({
    code: 'provider_profiles_missing',
    errorClass: 'retryable',
    message: `Cadence could not ${action} because the provider profile is unavailable. Refresh Settings and retry.`,
    retryable: true,
  })
}

export function useOperatorAuthMutations({
  adapter,
  refs,
  setters,
  operations,
}: UseCadenceDesktopMutationsArgs): Pick<
  CadenceDesktopMutationActions,
  | 'startOpenAiLogin'
  | 'submitOpenAiCallback'
  | 'startRuntimeSession'
  | 'logoutRuntimeSession'
  | 'resolveOperatorAction'
  | 'resumeOperatorRun'
> {
  const { activeProjectIdRef, activeProjectRef, providerProfilesRef } = refs
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

  const cancelOpenAiCallbackPoll = useCallback((projectId: string, flowId: string) => {
    const pollKey = `${projectId}:${flowId}`
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

  const cancelOpenAiCallbackPollsForProject = useCallback((projectId: string) => {
    for (const pollKey of Array.from(openAiCallbackPollsRef.current.keys())) {
      if (pollKey.startsWith(`${projectId}:`)) {
        const task = openAiCallbackPollsRef.current.get(pollKey)
        if (task) {
          task.cancelled = true
          if (task.timer !== null) {
            clearTimeout(task.timer)
          }
        }
        openAiCallbackPollsRef.current.delete(pollKey)
      }
    }
  }, [])

  const scheduleOpenAiCallbackCompletion = useCallback(
    (projectId: string, flowId: string, selectedProfileId: string) => {
      const pollKey = `${projectId}:${flowId}`
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
          const response = await adapter.submitOpenAiCallback(projectId, flowId, {
            selectedProfileId,
            manualInput: null,
          })
          if (!task.cancelled) {
            applyRuntimeSessionUpdate(mapRuntimeSession(response))
          }
          finish()
        } catch (error) {
          if (task.cancelled) {
            return
          }

          if (isOpenAiCallbackPending(error) && Date.now() < deadline) {
            task.timer = setTimeout(poll, OPENAI_CALLBACK_POLL_INTERVAL_MS)
            return
          }

          try {
            await syncRuntimeSession(projectId)
          } catch {
            // Ignore follow-up refresh failures and preserve the last truthful state.
          } finally {
            finish()
          }
        }
      }

      openAiCallbackPollsRef.current.set(pollKey, task)
      task.timer = setTimeout(poll, OPENAI_CALLBACK_INITIAL_POLL_DELAY_MS)
    },
    [adapter, applyRuntimeSessionUpdate, syncRuntimeSession],
  )

  const resolveOperatorAction = useCallback(
    async (
      actionId: string,
      decision: Parameters<CadenceDesktopMutationActions['resolveOperatorAction']>[1],
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
            'Cadence could not persist the operator decision for this project.',
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
            'Cadence could not record the operator resume request for this project.',
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

  const startOpenAiLogin = useCallback(async (options: { profileId?: string | null } = {}) => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before starting OpenAI login.',
    )
    const selectedProfileId = getSelectedProfileId(
      options.profileId ?? providerProfilesRef.current?.activeProfileId,
      'start OpenAI login',
    )

    try {
      const response = await adapter.startOpenAiLogin(projectId, {
        selectedProfileId,
        originator: 'pi',
      })
      const runtime = applyRuntimeSessionUpdate(mapRuntimeSession(response))
      if (
        runtime.providerId === 'openai_codex' &&
        runtime.flowId &&
        runtime.isLoginInProgress
      ) {
        scheduleOpenAiCallbackCompletion(projectId, runtime.flowId, selectedProfileId)
      }
      return runtime
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
    providerProfilesRef,
    scheduleOpenAiCallbackCompletion,
    syncRuntimeSession,
  ])

  const submitOpenAiCallback = useCallback(
    async (flowId: string, options: { manualInput?: string | null } = {}) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before completing OpenAI login.',
      )
      const selectedProfileId = getSelectedProfileId(
        providerProfilesRef.current?.activeProfileId,
        'complete OpenAI login',
      )
      cancelOpenAiCallbackPoll(projectId, flowId)

      try {
        const response = await adapter.submitOpenAiCallback(projectId, flowId, {
          selectedProfileId,
          manualInput: options.manualInput ?? null,
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
    },
    [
      activeProjectIdRef,
      adapter,
      applyRuntimeSessionUpdate,
      cancelOpenAiCallbackPoll,
      providerProfilesRef,
      syncRuntimeSession,
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
    cancelOpenAiCallbackPollsForProject(projectId)

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
    cancelOpenAiCallbackPollsForProject,
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
