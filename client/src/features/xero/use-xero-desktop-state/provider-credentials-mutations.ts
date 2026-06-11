import { useCallback, useEffect, useRef } from 'react'

import { XeroDesktopError } from '@/src/lib/xero-desktop'
import type { ProviderCredentialsSnapshotDto } from '@/src/lib/xero-model/provider-credentials'
import type { ProviderAuthSessionView } from '@/src/lib/xero-model/runtime'
import { mapProviderAuthSession } from '@/src/lib/xero-model/runtime'

import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'
import {
  getOperatorActionError,
} from './mutation-support'

const OAUTH_CALLBACK_INITIAL_POLL_DELAY_MS = 1_500
const OAUTH_CALLBACK_POLL_INTERVAL_MS = 750
const OAUTH_CALLBACK_POLL_TIMEOUT_MS = 120_000

type OAuthCallbackPollTask = {
  cancelled: boolean
  timer: ReturnType<typeof setTimeout> | null
}

function isOAuthCallbackPending(error: unknown): boolean {
  return error instanceof XeroDesktopError && error.code === 'authorization_code_pending'
}

export function useProviderCredentialsMutations({
  adapter,
  refs,
  setters,
  providerCredentialsLoadStatus,
}: UseXeroDesktopMutationsArgs): Pick<
  XeroDesktopMutationActions,
  | 'refreshProviderCredentials'
  | 'upsertProviderCredential'
  | 'deleteProviderCredential'
  | 'startOAuthLogin'
  | 'completeOAuthCallback'
> {
  const {
    providerCredentialsRef,
    providerCredentialsLoadInFlightRef,
  } = refs
  const {
    setProviderCredentials,
    setProviderCredentialsLoadStatus,
    setProviderCredentialsLoadError,
    setProviderCredentialsSaveStatus,
    setProviderCredentialsSaveError,
  } = setters
  const oauthCallbackPollsRef = useRef<Map<string, OAuthCallbackPollTask>>(new Map())

  useEffect(() => {
    return () => {
      for (const task of oauthCallbackPollsRef.current.values()) {
        task.cancelled = true
        if (task.timer !== null) {
          clearTimeout(task.timer)
        }
      }
      oauthCallbackPollsRef.current.clear()
    }
  }, [])

  const applySnapshot = useCallback(
    (snapshot: ProviderCredentialsSnapshotDto) => {
      setProviderCredentials(snapshot)
      setProviderCredentialsLoadStatus('ready')
      setProviderCredentialsLoadError(null)
      return snapshot
    },
    [setProviderCredentials, setProviderCredentialsLoadError, setProviderCredentialsLoadStatus],
  )

  const refreshProviderCredentials = useCallback(
    async (options: { force?: boolean } = {}) => {
      if (providerCredentialsLoadInFlightRef.current) {
        return providerCredentialsLoadInFlightRef.current
      }

      const cached = providerCredentialsRef.current
      if (!options.force && cached && providerCredentialsLoadStatus === 'ready') {
        return cached
      }

      setProviderCredentialsLoadStatus('loading')
      setProviderCredentialsLoadError(null)

      const loadPromise = (async () => {
        try {
          const response = await adapter.listProviderCredentials()
          return applySnapshot(response)
        } catch (error) {
          setProviderCredentialsLoadStatus('error')
          setProviderCredentialsLoadError(
            getOperatorActionError(
              error,
              'Xero could not load app-local provider credentials.',
            ),
          )
          throw error
        } finally {
          providerCredentialsLoadInFlightRef.current = null
        }
      })()

      providerCredentialsLoadInFlightRef.current = loadPromise
      return loadPromise
    },
    [
      adapter,
      applySnapshot,
      providerCredentialsLoadInFlightRef,
      providerCredentialsLoadStatus,
      providerCredentialsRef,
      setProviderCredentialsLoadError,
      setProviderCredentialsLoadStatus,
    ],
  )

  const upsertProviderCredential = useCallback<
    XeroDesktopMutationActions['upsertProviderCredential']
  >(
    async (request) => {
      setProviderCredentialsSaveStatus('running')
      setProviderCredentialsSaveError(null)

      try {
        const response = await adapter.upsertProviderCredential(request)
        applySnapshot(response)
        return response
      } catch (error) {
        setProviderCredentialsSaveError(
          getOperatorActionError(
            error,
            'Xero could not save the provider credential.',
          ),
        )

        try {
          await refreshProviderCredentials({ force: true })
        } catch {
          // Preserve last truthful snapshot if refresh-after-failure also fails.
        }

        throw error
      } finally {
        setProviderCredentialsSaveStatus('idle')
      }
    },
    [
      adapter,
      applySnapshot,
      refreshProviderCredentials,
      setProviderCredentialsSaveError,
      setProviderCredentialsSaveStatus,
    ],
  )

  const deleteProviderCredential = useCallback<
    XeroDesktopMutationActions['deleteProviderCredential']
  >(
    async (providerId) => {
      setProviderCredentialsSaveStatus('running')
      setProviderCredentialsSaveError(null)

      try {
        const response = await adapter.deleteProviderCredential(providerId)
        applySnapshot(response)
        return response
      } catch (error) {
        setProviderCredentialsSaveError(
          getOperatorActionError(
            error,
            'Xero could not remove the provider credential.',
          ),
        )

        try {
          await refreshProviderCredentials({ force: true })
        } catch {
          // Preserve last truthful snapshot if refresh-after-failure also fails.
        }

        throw error
      } finally {
        setProviderCredentialsSaveStatus('idle')
      }
    },
    [
      adapter,
      applySnapshot,
      refreshProviderCredentials,
      setProviderCredentialsSaveError,
      setProviderCredentialsSaveStatus,
    ],
  )

  const scheduleOAuthCallbackCompletion = useCallback(
    (providerId: Parameters<XeroDesktopMutationActions['startOAuthLogin']>[0]['providerId'], flowId: string) => {
      const pollKey = `${providerId}:${flowId}`
      if (oauthCallbackPollsRef.current.has(pollKey)) {
        return
      }

      const deadline = Date.now() + OAUTH_CALLBACK_POLL_TIMEOUT_MS
      const task: OAuthCallbackPollTask = {
        cancelled: false,
        timer: null,
      }

      const finish = () => {
        task.cancelled = true
        if (task.timer !== null) {
          clearTimeout(task.timer)
          task.timer = null
        }
        oauthCallbackPollsRef.current.delete(pollKey)
      }

      const poll = async () => {
        task.timer = null
        if (task.cancelled) {
          return
        }

        try {
          await adapter.completeOAuthCallback({
            providerId,
            flowId,
            manualInput: null,
          })
          if (!task.cancelled) {
            await refreshProviderCredentials({ force: true })
          }
          finish()
        } catch (error) {
          if (task.cancelled) {
            return
          }

          if (isOAuthCallbackPending(error) && Date.now() < deadline) {
            task.timer = setTimeout(poll, OAUTH_CALLBACK_POLL_INTERVAL_MS)
            return
          }

          try {
            await refreshProviderCredentials({ force: true })
          } catch {
            // Preserve the last truthful credentials snapshot if refresh-after-failure also fails.
          } finally {
            finish()
          }
        }
      }

      oauthCallbackPollsRef.current.set(pollKey, task)
      task.timer = setTimeout(poll, OAUTH_CALLBACK_INITIAL_POLL_DELAY_MS)
    },
    [adapter, refreshProviderCredentials],
  )

  const startOAuthLogin = useCallback<
    XeroDesktopMutationActions['startOAuthLogin']
  >(
    async (request): Promise<ProviderAuthSessionView | null> => {
      const session = await adapter.startOAuthLogin({
        providerId: request.providerId,
        originator: request.originator ?? null,
      })
      const runtime = mapProviderAuthSession(session)
      if (runtime.flowId && runtime.isLoginInProgress) {
        scheduleOAuthCallbackCompletion(request.providerId, runtime.flowId)
      }
      return runtime
    },
    [adapter, scheduleOAuthCallbackCompletion],
  )

  const completeOAuthCallback = useCallback<
    XeroDesktopMutationActions['completeOAuthCallback']
  >(
    async (request): Promise<ProviderAuthSessionView | null> => {
      const session = await adapter.completeOAuthCallback({
        providerId: request.providerId,
        flowId: request.flowId,
        manualInput: request.manualInput ?? null,
      })
      const runtime = mapProviderAuthSession(session)
      await refreshProviderCredentials({ force: true })
      return runtime
    },
    [adapter, refreshProviderCredentials],
  )

  return {
    refreshProviderCredentials,
    upsertProviderCredential,
    deleteProviderCredential,
    startOAuthLogin,
    completeOAuthCallback,
  }
}
