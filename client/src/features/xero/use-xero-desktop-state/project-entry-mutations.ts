import { useCallback } from 'react'

import { getDesktopErrorMessage } from '@/src/lib/xero-desktop'
import {
  mapProjectSummary,
  upsertProjectListItem,
} from '@/src/lib/xero-model/project'

import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'

export function useProjectEntryMutations({
  adapter,
  setters,
  operations,
}: UseXeroDesktopMutationsArgs): Pick<
  XeroDesktopMutationActions,
  | 'importProject'
  | 'createProject'
  | 'removeProject'
  | 'listProjectFiles'
  | 'readProjectFile'
  | 'writeProjectFile'
  | 'revokeProjectAssetTokens'
  | 'openProjectFileExternal'
  | 'createProjectEntry'
  | 'renameProjectEntry'
  | 'moveProjectEntry'
  | 'deleteProjectEntry'
  | 'searchProject'
  | 'replaceInProject'
> {
  const {
    setProjects,
    setIsImporting,
    setProjectRemovalStatus,
    setPendingProjectRemovalId,
    setRefreshSource,
    setErrorMessage,
  } = setters
  const { bootstrap, loadProject } = operations

  const importProject = useCallback(
    async (path?: string) => {
      setIsImporting(true)
      setRefreshSource('import')
      setErrorMessage(null)

      try {
        const selectedPath = path ?? (await adapter.pickRepositoryFolder())
        if (!selectedPath) {
          return false
        }

        const response = await adapter.importRepository(selectedPath)
        const summary = mapProjectSummary(response.project)
        setProjects((currentProjects) => upsertProjectListItem(currentProjects, summary))
        await loadProject(summary.id, 'import')
        return true
      } catch (error) {
        setErrorMessage(getDesktopErrorMessage(error))
        return false
      } finally {
        setIsImporting(false)
      }
    },
    [adapter, loadProject, setErrorMessage, setIsImporting, setProjects, setRefreshSource],
  )

  const createProject = useCallback(
    async (parentPath: string, name: string) => {
      setIsImporting(true)
      setRefreshSource('import')
      setErrorMessage(null)

      try {
        const response = await adapter.createRepository(parentPath, name)
        const summary = mapProjectSummary(response.project)
        setProjects((currentProjects) => upsertProjectListItem(currentProjects, summary))
        await loadProject(summary.id, 'import')
        return true
      } catch (error) {
        setErrorMessage(getDesktopErrorMessage(error))
        return false
      } finally {
        setIsImporting(false)
      }
    },
    [adapter, loadProject, setErrorMessage, setIsImporting, setProjects, setRefreshSource],
  )

  const removeProject = useCallback(
    async (projectId: string) => {
      if (!projectId.trim()) {
        return
      }

      setProjectRemovalStatus('running')
      setPendingProjectRemovalId(projectId)
      setRefreshSource('remove')
      setErrorMessage(null)

      try {
        await adapter.removeProject(projectId)
        await bootstrap('remove')
      } catch (error) {
        setErrorMessage(getDesktopErrorMessage(error))
      } finally {
        setPendingProjectRemovalId(null)
        setProjectRemovalStatus('idle')
      }
    },
    [adapter, bootstrap, setErrorMessage, setPendingProjectRemovalId, setProjectRemovalStatus, setRefreshSource],
  )

  const listProjectFiles = useCallback(
    async (projectId: string, path?: string) => {
      return path === undefined
        ? await adapter.listProjectFiles(projectId)
        : await adapter.listProjectFiles(projectId, path)
    },
    [adapter],
  )

  const readProjectFile = useCallback(
    async (projectId: string, path: string) => {
      return await adapter.readProjectFile(projectId, path)
    },
    [adapter],
  )

  const writeProjectFile = useCallback(
    async (projectId: string, path: string, content: string) => {
      return await adapter.writeProjectFile(projectId, path, content)
    },
    [adapter],
  )

  const revokeProjectAssetTokens = useCallback(
    async (projectId: string, paths?: string[]) => {
      await adapter.revokeProjectAssetTokens?.(projectId, paths)
    },
    [adapter],
  )

  const openProjectFileExternal = useCallback(
    async (projectId: string, path: string) => {
      await adapter.openProjectFileExternal?.(projectId, path)
    },
    [adapter],
  )

  const createProjectEntry = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['createProjectEntry']>[0]) => {
      return await adapter.createProjectEntry(request)
    },
    [adapter],
  )

  const renameProjectEntry = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['renameProjectEntry']>[0]) => {
      return await adapter.renameProjectEntry(request)
    },
    [adapter],
  )

  const moveProjectEntry = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['moveProjectEntry']>[0]) => {
      return await adapter.moveProjectEntry(request)
    },
    [adapter],
  )

  const deleteProjectEntry = useCallback(
    async (projectId: string, path: string) => {
      return await adapter.deleteProjectEntry(projectId, path)
    },
    [adapter],
  )

  const searchProject = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['searchProject']>[0]) => {
      return await adapter.searchProject(request)
    },
    [adapter],
  )

  const replaceInProject = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['replaceInProject']>[0]) => {
      return await adapter.replaceInProject(request)
    },
    [adapter],
  )

  return {
    importProject,
    createProject,
    removeProject,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    revokeProjectAssetTokens,
    openProjectFileExternal,
    createProjectEntry,
    renameProjectEntry,
    moveProjectEntry,
    deleteProjectEntry,
    searchProject,
    replaceInProject,
  }
}
