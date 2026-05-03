import { useCallback } from 'react'

import { mapAutonomousRunInspection } from '@/src/lib/xero-model/autonomous'
import {
  mapAgentSession,
  mapRuntimeRun,
  selectAgentSessionId,
  type RuntimeAutoCompactPreferenceDto,
  type RuntimeRunControlInputDto,
  type StagedAgentAttachmentDto,
  type UpdateRuntimeRunControlsRequestDto,
} from '@/src/lib/xero-model/runtime'

import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'
import {
  getActiveProjectId,
  getOperatorActionError,
} from './mutation-support'

const DEFAULT_AGENT_SESSION_TITLE = 'New Chat'

export function useRunControlMutations({
  adapter,
  refs,
  setters,
  operations,
}: UseXeroDesktopMutationsArgs): Pick<
  XeroDesktopMutationActions,
  | 'startAutonomousRun'
  | 'inspectAutonomousRun'
  | 'cancelAutonomousRun'
  | 'startRuntimeRun'
  | 'updateRuntimeRunControls'
  | 'stopRuntimeRun'
> {
  const { activeProjectIdRef, activeProjectRef, runtimeRunsRef } = refs
  const {
    setAutonomousRunActionStatus,
    setPendingAutonomousRunAction,
    setAutonomousRunActionError,
    setRuntimeRunActionStatus,
    setPendingRuntimeRunAction,
    setRuntimeRunActionError,
  } = setters
  const {
    applyAgentSessionSelection,
    syncRuntimeRun,
    syncAutonomousRun,
    applyRuntimeRunUpdate,
    applyAutonomousRunStateUpdate,
  } = operations

  const startAutonomousRun = useCallback(async () => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before starting an autonomous run.',
    )
    const agentSessionId = selectAgentSessionId(
      activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
    )

    setAutonomousRunActionStatus('running')
    setPendingAutonomousRunAction('start')
    setAutonomousRunActionError(null)

    try {
      const response = await adapter.startAutonomousRun(projectId, agentSessionId)
      return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
        clearGlobalError: false,
        loadError: null,
      })
    } catch (error) {
      setAutonomousRunActionError(
        getOperatorActionError(
          error,
          'Xero could not start or inspect the autonomous run for this project.',
        ),
      )

      try {
        await syncAutonomousRun(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    } finally {
      setAutonomousRunActionStatus('idle')
      setPendingAutonomousRunAction(null)
    }
  }, [
    activeProjectIdRef,
    activeProjectRef,
    adapter,
    applyAutonomousRunStateUpdate,
    setAutonomousRunActionError,
    setAutonomousRunActionStatus,
    setPendingAutonomousRunAction,
    syncAutonomousRun,
  ])

  const inspectAutonomousRun = useCallback(async () => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before inspecting autonomous run truth.',
    )

    setAutonomousRunActionStatus('running')
    setPendingAutonomousRunAction('inspect')
    setAutonomousRunActionError(null)

    try {
      return await syncAutonomousRun(projectId)
    } catch (error) {
      setAutonomousRunActionError(
        getOperatorActionError(
          error,
          'Xero could not inspect the autonomous run truth for this project.',
        ),
      )
      throw error
    } finally {
      setAutonomousRunActionStatus('idle')
      setPendingAutonomousRunAction(null)
    }
  }, [
    activeProjectIdRef,
    setAutonomousRunActionError,
    setAutonomousRunActionStatus,
    setPendingAutonomousRunAction,
    syncAutonomousRun,
  ])

  const cancelAutonomousRun = useCallback(
    async (runId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before cancelling the autonomous run.',
      )
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )

      setAutonomousRunActionStatus('running')
      setPendingAutonomousRunAction('cancel')
      setAutonomousRunActionError(null)

      try {
        const response = await adapter.cancelAutonomousRun(projectId, agentSessionId, runId)
        return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setAutonomousRunActionError(
          getOperatorActionError(error, 'Xero could not cancel the autonomous run for this project.'),
        )

        try {
          await syncAutonomousRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        setAutonomousRunActionStatus('idle')
        setPendingAutonomousRunAction(null)
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      adapter,
      applyAutonomousRunStateUpdate,
      setAutonomousRunActionError,
      setAutonomousRunActionStatus,
      setPendingAutonomousRunAction,
      syncAutonomousRun,
    ],
  )

  const startRuntimeRun = useCallback(async (options?: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null; attachments?: StagedAgentAttachmentDto[] }) => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before starting a Xero-owned agent run.',
    )
    const agentSessionId = selectAgentSessionId(
      activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
    )
    const selectedSession =
      activeProjectRef.current?.id === projectId
        ? activeProjectRef.current.agentSessions.find((session) => session.agentSessionId === agentSessionId) ?? null
        : null
    const promptForAutoName = options?.prompt?.trim() ?? ''
    const shouldAutoNameSession = Boolean(
      promptForAutoName.length > 0 &&
        selectedSession &&
        selectedSession.lastRunId === null &&
        selectedSession.title.trim().toLowerCase() === DEFAULT_AGENT_SESSION_TITLE.toLowerCase(),
    )

    setRuntimeRunActionStatus('running')
    setPendingRuntimeRunAction('start')
    setRuntimeRunActionError(null)

    try {
      const response = await adapter.startRuntimeRun(projectId, agentSessionId, {
        initialControls: options?.controls ?? null,
        initialPrompt: options?.prompt ?? null,
        initialAttachments: options?.attachments ?? [],
      })
      const runtimeRun = applyRuntimeRunUpdate(projectId, mapRuntimeRun(response), {
        clearGlobalError: false,
        loadError: null,
      })

      if (shouldAutoNameSession) {
        void adapter
          .autoNameAgentSession({
            projectId,
            agentSessionId,
            prompt: promptForAutoName,
            controls: options?.controls ?? null,
          })
          .then((session) => {
            if (activeProjectIdRef.current === projectId) {
              applyAgentSessionSelection(mapAgentSession(session))
            }
          })
          .catch(() => {
            // Auto-naming should never interrupt the user's first prompt.
          })
      }

      return runtimeRun
    } catch (error) {
      setRuntimeRunActionError(
        getOperatorActionError(
          error,
          'Xero could not start or reconnect the Xero-owned agent run for this project.',
        ),
      )

      try {
        await syncRuntimeRun(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    } finally {
      setRuntimeRunActionStatus('idle')
      setPendingRuntimeRunAction(null)
    }
  }, [
    activeProjectIdRef,
    activeProjectRef,
    adapter,
    applyAgentSessionSelection,
    applyRuntimeRunUpdate,
    setPendingRuntimeRunAction,
    setRuntimeRunActionError,
    setRuntimeRunActionStatus,
    syncRuntimeRun,
  ])

  const updateRuntimeRunControls = useCallback(
    async (request: {
      controls?: RuntimeRunControlInputDto | null
      prompt?: string | null
      attachments?: StagedAgentAttachmentDto[]
      autoCompact?: RuntimeAutoCompactPreferenceDto | null
    } = {}) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before queueing agent-run controls.',
      )
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )
      let runId: string | null =
        runtimeRunsRef.current[projectId]?.runId?.trim() ??
        activeProjectRef.current?.runtimeRun?.runId?.trim() ??
        null

      if (!runId) {
        try {
          const hydratedRun = await syncRuntimeRun(projectId)
          runId = hydratedRun?.runId?.trim() ?? null
        } catch {
          // Ignore refresh failure here; the queue attempt should still fail with the explicit missing-run copy below.
        }
      }

      if (!runId) {
        throw new Error('Xero cannot queue runtime-run controls until a Xero-owned agent run exists for this project.')
      }
      const resolvedRunId = runId

      setRuntimeRunActionStatus('running')
      setPendingRuntimeRunAction('update_controls')
      setRuntimeRunActionError(null)

      try {
        const updateRequest: UpdateRuntimeRunControlsRequestDto = {
          projectId,
          agentSessionId,
          runId: resolvedRunId,
          controls: request.controls ?? null,
          prompt: request.prompt ?? null,
          attachments: request.attachments ?? [],
        }
        if (request.autoCompact !== undefined) {
          updateRequest.autoCompact = request.autoCompact
        }
        const response = await adapter.updateRuntimeRunControls(updateRequest)
        return applyRuntimeRunUpdate(projectId, mapRuntimeRun(response), {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setRuntimeRunActionError(
          getOperatorActionError(
            error,
            'Xero could not queue runtime-run control changes for this project.',
          ),
        )

        try {
          await syncRuntimeRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        setRuntimeRunActionStatus('idle')
        setPendingRuntimeRunAction(null)
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      runtimeRunsRef,
      adapter,
      applyRuntimeRunUpdate,
      setPendingRuntimeRunAction,
      setRuntimeRunActionError,
      setRuntimeRunActionStatus,
      syncRuntimeRun,
    ],
  )

  const stopRuntimeRun = useCallback(
    async (runId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before stopping the Xero-owned agent run.',
      )
      const agentSessionId = selectAgentSessionId(
        activeProjectRef.current?.id === projectId ? activeProjectRef.current.agentSessions : null,
      )

      setRuntimeRunActionStatus('running')
      setPendingRuntimeRunAction('stop')
      setRuntimeRunActionError(null)

      try {
        const response = await adapter.stopRuntimeRun(projectId, agentSessionId, runId)
        return applyRuntimeRunUpdate(projectId, response ? mapRuntimeRun(response) : null, {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setRuntimeRunActionError(
          getOperatorActionError(error, 'Xero could not stop the Xero-owned agent run for this project.'),
        )

        try {
          await syncRuntimeRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        setRuntimeRunActionStatus('idle')
        setPendingRuntimeRunAction(null)
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      adapter,
      applyRuntimeRunUpdate,
      setPendingRuntimeRunAction,
      setRuntimeRunActionError,
      setRuntimeRunActionStatus,
      syncRuntimeRun,
    ],
  )

  return {
    startAutonomousRun,
    inspectAutonomousRun,
    cancelAutonomousRun,
    startRuntimeRun,
    updateRuntimeRunControls,
    stopRuntimeRun,
  }
}
