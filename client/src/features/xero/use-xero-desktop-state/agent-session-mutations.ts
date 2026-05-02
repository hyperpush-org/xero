import { useCallback, useRef } from 'react'

import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'
import { getActiveProjectId } from './mutation-support'
import { mapAgentSession } from '@/src/lib/xero-model'

function waitForAgentSessionSelectionPaint(): Promise<void> {
  if (typeof window === 'undefined') {
    return Promise.resolve()
  }

  return new Promise((resolve) => {
    const finishAfterFrame = () => window.setTimeout(resolve, 0)
    if (typeof window.requestAnimationFrame === 'function') {
      window.requestAnimationFrame(finishAfterFrame)
      return
    }

    finishAfterFrame()
  })
}

export function useAgentSessionMutations({
  adapter,
  refs,
  operations,
}: UseXeroDesktopMutationsArgs): Pick<
  XeroDesktopMutationActions,
  | 'createAgentSession'
  | 'selectAgentSession'
  | 'archiveAgentSession'
  | 'restoreAgentSession'
  | 'deleteAgentSession'
  | 'renameAgentSession'
> {
  const { activeProjectIdRef, activeProjectRef } = refs
  const {
    loadProject,
    optimisticallySelectAgentSession,
    applyAgentSessionSelection,
    rollbackAgentSessionSelection,
    hydrateAgentSessionRuntimeState,
  } = operations
  const selectionRequestRef = useRef(0)

  const createAgentSession = useCallback(
    async (options: { title?: string | null; summary?: string | null } = {}) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before creating an agent session.',
      )

      await adapter.createAgentSession({
        projectId,
        title: options.title ?? null,
        summary: options.summary ?? undefined,
        selected: true,
      })
      await loadProject(projectId, 'selection')
      return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
    },
    [activeProjectIdRef, activeProjectRef, adapter, loadProject],
  )

  const selectAgentSession = useCallback(
    async (agentSessionId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before switching agent sessions.',
      )
      const requestId = selectionRequestRef.current + 1
      selectionRequestRef.current = requestId
      const optimisticSelection = optimisticallySelectAgentSession(agentSessionId)

      try {
        const response = await adapter.updateAgentSession({
          projectId,
          agentSessionId,
          selected: true,
        })

        if (selectionRequestRef.current !== requestId || activeProjectIdRef.current !== projectId) {
          return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
        }

        applyAgentSessionSelection(mapAgentSession(response))
        await waitForAgentSessionSelectionPaint()

        if (selectionRequestRef.current === requestId && activeProjectIdRef.current === projectId) {
          void hydrateAgentSessionRuntimeState(projectId, agentSessionId, { force: true })
        }

        return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
      } catch (error) {
        if (selectionRequestRef.current === requestId && activeProjectIdRef.current === projectId) {
          rollbackAgentSessionSelection(optimisticSelection?.previousProject ?? null)
        }
        throw error
      }
    },
    [
      activeProjectIdRef,
      activeProjectRef,
      adapter,
      applyAgentSessionSelection,
      hydrateAgentSessionRuntimeState,
      optimisticallySelectAgentSession,
      rollbackAgentSessionSelection,
    ],
  )

  const archiveAgentSession = useCallback(
    async (agentSessionId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before archiving an agent session.',
      )

      await adapter.archiveAgentSession({ projectId, agentSessionId })
      await loadProject(projectId, 'selection')
      return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
    },
    [activeProjectIdRef, activeProjectRef, adapter, loadProject],
  )

  const restoreAgentSession = useCallback(
    async (agentSessionId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before restoring an agent session.',
      )

      await adapter.restoreAgentSession({ projectId, agentSessionId })
      await loadProject(projectId, 'selection')
      return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
    },
    [activeProjectIdRef, activeProjectRef, adapter, loadProject],
  )

  const deleteAgentSession = useCallback(
    async (agentSessionId: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before deleting an agent session.',
      )

      await adapter.deleteAgentSession({ projectId, agentSessionId })
      await loadProject(projectId, 'selection')
      return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
    },
    [activeProjectIdRef, activeProjectRef, adapter, loadProject],
  )

  const renameAgentSession = useCallback(
    async (agentSessionId: string, title: string) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before renaming an agent session.',
      )

      await adapter.updateAgentSession({
        projectId,
        agentSessionId,
        title,
      })
      await loadProject(projectId, 'selection')
      return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
    },
    [activeProjectIdRef, activeProjectRef, adapter, loadProject],
  )

  return {
    createAgentSession,
    selectAgentSession,
    archiveAgentSession,
    restoreAgentSession,
    deleteAgentSession,
    renameAgentSession,
  }
}
