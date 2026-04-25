import { useCallback } from 'react'

import type {
  CadenceDesktopMutationActions,
  UseCadenceDesktopMutationsArgs,
} from './mutation-support'
import { getActiveProjectId } from './mutation-support'

export function useAgentSessionMutations({
  adapter,
  refs,
  operations,
}: UseCadenceDesktopMutationsArgs): Pick<
  CadenceDesktopMutationActions,
  'createAgentSession' | 'selectAgentSession' | 'archiveAgentSession' | 'renameAgentSession'
> {
  const { activeProjectIdRef, activeProjectRef } = refs
  const { loadProject } = operations

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

      await adapter.updateAgentSession({
        projectId,
        agentSessionId,
        selected: true,
      })
      await loadProject(projectId, 'selection')
      return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
    },
    [activeProjectIdRef, activeProjectRef, adapter, loadProject],
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
    renameAgentSession,
  }
}
