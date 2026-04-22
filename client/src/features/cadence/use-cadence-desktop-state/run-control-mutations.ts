import { useCallback } from 'react'

import { mapAutonomousRunInspection } from '@/src/lib/cadence-model/autonomous'
import {
  mapRuntimeRun,
  type RuntimeRunControlInputDto,
} from '@/src/lib/cadence-model/runtime'

import type {
  CadenceDesktopMutationActions,
  UseCadenceDesktopMutationsArgs,
} from './mutation-support'
import {
  getActiveProjectId,
  getOperatorActionError,
} from './mutation-support'

export function useRunControlMutations({
  adapter,
  refs,
  setters,
  operations,
}: UseCadenceDesktopMutationsArgs): Pick<
  CadenceDesktopMutationActions,
  | 'startAutonomousRun'
  | 'inspectAutonomousRun'
  | 'cancelAutonomousRun'
  | 'startRuntimeRun'
  | 'updateRuntimeRunControls'
  | 'stopRuntimeRun'
> {
  const { activeProjectIdRef, activeProjectRef } = refs
  const {
    setAutonomousRunActionStatus,
    setPendingAutonomousRunAction,
    setAutonomousRunActionError,
    setRuntimeRunActionStatus,
    setPendingRuntimeRunAction,
    setRuntimeRunActionError,
  } = setters
  const {
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

    setAutonomousRunActionStatus('running')
    setPendingAutonomousRunAction('start')
    setAutonomousRunActionError(null)

    try {
      const response = await adapter.startAutonomousRun(projectId)
      return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
        clearGlobalError: false,
        loadError: null,
      })
    } catch (error) {
      setAutonomousRunActionError(
        getOperatorActionError(
          error,
          'Cadence could not start or inspect the autonomous run for this project.',
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
          'Cadence could not inspect the autonomous run truth for this project.',
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

      setAutonomousRunActionStatus('running')
      setPendingAutonomousRunAction('cancel')
      setAutonomousRunActionError(null)

      try {
        const response = await adapter.cancelAutonomousRun(projectId, runId)
        return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setAutonomousRunActionError(
          getOperatorActionError(error, 'Cadence could not cancel the autonomous run for this project.'),
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
      adapter,
      applyAutonomousRunStateUpdate,
      setAutonomousRunActionError,
      setAutonomousRunActionStatus,
      setPendingAutonomousRunAction,
      syncAutonomousRun,
    ],
  )

  const startRuntimeRun = useCallback(async (options?: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null }) => {
    const projectId = getActiveProjectId(
      activeProjectIdRef,
      'Select an imported project before starting a supervised runtime run.',
    )

    setRuntimeRunActionStatus('running')
    setPendingRuntimeRunAction('start')
    setRuntimeRunActionError(null)

    try {
      const response = await adapter.startRuntimeRun(projectId, {
        initialControls: options?.controls ?? null,
        initialPrompt: options?.prompt ?? null,
      })
      return applyRuntimeRunUpdate(projectId, mapRuntimeRun(response), {
        clearGlobalError: false,
        loadError: null,
      })
    } catch (error) {
      setRuntimeRunActionError(
        getOperatorActionError(
          error,
          'Cadence could not start or reconnect the supervised runtime run for this project.',
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
    adapter,
    applyRuntimeRunUpdate,
    setPendingRuntimeRunAction,
    setRuntimeRunActionError,
    setRuntimeRunActionStatus,
    syncRuntimeRun,
  ])

  const updateRuntimeRunControls = useCallback(
    async (request: { controls?: RuntimeRunControlInputDto | null; prompt?: string | null } = {}) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before queueing supervised runtime-run controls.',
      )
      const runId = activeProjectRef.current?.runtimeRun?.runId?.trim()
      if (!runId) {
        throw new Error('Cadence cannot queue runtime-run controls until a supervised runtime run exists for this project.')
      }

      setRuntimeRunActionStatus('running')
      setPendingRuntimeRunAction('update_controls')
      setRuntimeRunActionError(null)

      try {
        const response = await adapter.updateRuntimeRunControls({
          projectId,
          runId,
          controls: request.controls ?? null,
          prompt: request.prompt ?? null,
        })
        return applyRuntimeRunUpdate(projectId, mapRuntimeRun(response), {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setRuntimeRunActionError(
          getOperatorActionError(
            error,
            'Cadence could not queue runtime-run control changes for this project.',
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
        'Select an imported project before stopping the supervised runtime run.',
      )

      setRuntimeRunActionStatus('running')
      setPendingRuntimeRunAction('stop')
      setRuntimeRunActionError(null)

      try {
        const response = await adapter.stopRuntimeRun(projectId, runId)
        return applyRuntimeRunUpdate(projectId, response ? mapRuntimeRun(response) : null, {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setRuntimeRunActionError(
          getOperatorActionError(error, 'Cadence could not stop the supervised runtime run for this project.'),
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
