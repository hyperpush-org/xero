import { useCallback } from 'react'

import { type McpRegistryDto } from '@/src/lib/xero-model/mcp'
import { type SkillRegistryDto } from '@/src/lib/xero-model/skills'
import type {
  XeroDesktopMutationActions,
  UseXeroDesktopMutationsArgs,
} from './mutation-support'
import {
  getActiveProjectId,
  getOperatorActionError,
} from './mutation-support'

function createMcpRegistrySyncKey(registry: McpRegistryDto | null): string {
  if (!registry) {
    return 'none'
  }

  return JSON.stringify(registry)
}

function createSkillRegistrySyncKey(registry: SkillRegistryDto | null): string {
  if (!registry) {
    return 'none'
  }

  return JSON.stringify(registry)
}

export function useRuntimeSettingsNotificationMutations({
  adapter,
  refs,
  setters,
  operations,
  mcpRegistryLoadStatus,
  skillRegistryLoadStatus,
}: UseXeroDesktopMutationsArgs): Pick<
  XeroDesktopMutationActions,
  | 'refreshMcpRegistry'
  | 'upsertMcpServer'
  | 'removeMcpServer'
  | 'importMcpServers'
  | 'refreshMcpServerStatuses'
  | 'refreshSkillRegistry'
  | 'reloadSkillRegistry'
  | 'setSkillEnabled'
  | 'removeSkill'
  | 'upsertSkillLocalRoot'
  | 'removeSkillLocalRoot'
  | 'updateProjectSkillSource'
  | 'updateGithubSkillSource'
  | 'upsertPluginRoot'
  | 'removePluginRoot'
  | 'setPluginEnabled'
  | 'removePlugin'
  | 'refreshNotificationRoutes'
  | 'upsertNotificationRoute'
> {
  const {
    activeProjectIdRef,
    mcpRegistryRef,
    mcpRegistryLoadInFlightRef,
    skillRegistryRef,
    skillRegistryLoadInFlightRef,
  } = refs
  const {
    setNotificationRoutes,
    setNotificationRouteLoadStatuses,
    setNotificationRouteLoadErrors,
    setNotificationRouteMutationStatus,
    setPendingNotificationRouteId,
    setNotificationRouteMutationError,
    setMcpRegistry,
    setMcpImportDiagnostics,
    setMcpRegistryLoadStatus,
    setMcpRegistryLoadError,
    setMcpRegistryMutationStatus,
    setPendingMcpServerId,
    setMcpRegistryMutationError,
    setSkillRegistry,
    setSkillRegistryLoadStatus,
    setSkillRegistryLoadError,
    setSkillRegistryMutationStatus,
    setPendingSkillSourceId,
    setSkillRegistryMutationError,
  } = setters
  const { loadNotificationRoutes } = operations

  const applyMcpRegistrySnapshot = useCallback(
    (response: McpRegistryDto) => {
      const currentRegistry = mcpRegistryRef.current
      const nextSyncKey = createMcpRegistrySyncKey(response)
      const currentSyncKey = createMcpRegistrySyncKey(currentRegistry)

      if (nextSyncKey !== currentSyncKey) {
        setMcpRegistry(response)
      }

      setMcpRegistryLoadStatus('ready')
      setMcpRegistryLoadError(null)

      return nextSyncKey === currentSyncKey && currentRegistry ? currentRegistry : response
    },
    [mcpRegistryRef, setMcpRegistry, setMcpRegistryLoadError, setMcpRegistryLoadStatus],
  )

  const applySkillRegistrySnapshot = useCallback(
    (response: SkillRegistryDto) => {
      const currentRegistry = skillRegistryRef.current
      const nextSyncKey = createSkillRegistrySyncKey(response)
      const currentSyncKey = createSkillRegistrySyncKey(currentRegistry)

      if (nextSyncKey !== currentSyncKey) {
        setSkillRegistry(response)
      }

      setSkillRegistryLoadStatus('ready')
      setSkillRegistryLoadError(null)

      return nextSyncKey === currentSyncKey && currentRegistry ? currentRegistry : response
    },
    [setSkillRegistry, setSkillRegistryLoadError, setSkillRegistryLoadStatus, skillRegistryRef],
  )

  const refreshMcpRegistry = useCallback(
    async (options: { force?: boolean } = {}) => {
      if (mcpRegistryLoadInFlightRef.current) {
        return mcpRegistryLoadInFlightRef.current
      }

      const cachedRegistry = mcpRegistryRef.current
      if (!options.force && cachedRegistry && mcpRegistryLoadStatus === 'ready') {
        return cachedRegistry
      }

      setMcpRegistryLoadStatus('loading')
      setMcpRegistryLoadError(null)

      const loadPromise = (async () => {
        try {
          const response = await adapter.listMcpServers()
          return applyMcpRegistrySnapshot(response)
        } catch (error) {
          setMcpRegistryLoadStatus('error')
          setMcpRegistryLoadError(
            getOperatorActionError(error, 'Xero could not load app-local MCP registry.'),
          )
          throw error
        } finally {
          mcpRegistryLoadInFlightRef.current = null
        }
      })()

      mcpRegistryLoadInFlightRef.current = loadPromise
      return loadPromise
    },
    [
      adapter,
      applyMcpRegistrySnapshot,
      mcpRegistryLoadInFlightRef,
      mcpRegistryLoadStatus,
      mcpRegistryRef,
      setMcpRegistryLoadError,
      setMcpRegistryLoadStatus,
    ],
  )

  const upsertMcpServer = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['upsertMcpServer']>[0]) => {
      const pendingServerId = request.id.trim()
      setMcpRegistryMutationStatus('running')
      setPendingMcpServerId(pendingServerId.length > 0 ? pendingServerId : null)
      setMcpRegistryMutationError(null)

      try {
        const response = await adapter.upsertMcpServer(request)
        const snapshot = applyMcpRegistrySnapshot(response)
        setMcpRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setMcpRegistryMutationError(
          getOperatorActionError(error, 'Xero could not save the MCP server definition.'),
        )

        try {
          await refreshMcpRegistry({ force: true })
        } catch {
          // Preserve the last truthful MCP snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setMcpRegistryMutationStatus('idle')
        setPendingMcpServerId(null)
      }
    },
    [
      adapter,
      applyMcpRegistrySnapshot,
      refreshMcpRegistry,
      setMcpRegistryMutationError,
      setMcpRegistryMutationStatus,
      setPendingMcpServerId,
    ],
  )

  const removeMcpServer = useCallback(
    async (serverId: string) => {
      const pendingServerId = serverId.trim()
      setMcpRegistryMutationStatus('running')
      setPendingMcpServerId(pendingServerId.length > 0 ? pendingServerId : null)
      setMcpRegistryMutationError(null)

      try {
        const response = await adapter.removeMcpServer(serverId)
        const snapshot = applyMcpRegistrySnapshot(response)
        setMcpRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setMcpRegistryMutationError(
          getOperatorActionError(error, 'Xero could not remove the MCP server definition.'),
        )

        try {
          await refreshMcpRegistry({ force: true })
        } catch {
          // Preserve the last truthful MCP snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setMcpRegistryMutationStatus('idle')
        setPendingMcpServerId(null)
      }
    },
    [
      adapter,
      applyMcpRegistrySnapshot,
      refreshMcpRegistry,
      setMcpRegistryMutationError,
      setMcpRegistryMutationStatus,
      setPendingMcpServerId,
    ],
  )

  const importMcpServers = useCallback(
    async (path: string) => {
      setMcpRegistryMutationStatus('running')
      setPendingMcpServerId(null)
      setMcpRegistryMutationError(null)

      try {
        const response = await adapter.importMcpServers(path)
        applyMcpRegistrySnapshot(response.registry)
        setMcpImportDiagnostics(response.diagnostics)
        setMcpRegistryMutationError(null)
        return response
      } catch (error) {
        setMcpRegistryMutationError(
          getOperatorActionError(error, 'Xero could not import MCP servers from that file.'),
        )

        try {
          await refreshMcpRegistry({ force: true })
        } catch {
          // Preserve the last truthful MCP snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setMcpRegistryMutationStatus('idle')
      }
    },
    [
      adapter,
      applyMcpRegistrySnapshot,
      refreshMcpRegistry,
      setMcpImportDiagnostics,
      setMcpRegistryMutationError,
      setMcpRegistryMutationStatus,
      setPendingMcpServerId,
    ],
  )

  const refreshMcpServerStatuses = useCallback(
    async (options: { serverIds?: string[] } = {}) => {
      const serverIds = options.serverIds ?? []
      const pendingServerId = serverIds.length === 1 ? serverIds[0] ?? null : null

      setMcpRegistryMutationStatus('running')
      setPendingMcpServerId(pendingServerId)
      setMcpRegistryMutationError(null)

      try {
        const response = await adapter.refreshMcpServerStatuses({ serverIds })
        const snapshot = applyMcpRegistrySnapshot(response)
        setMcpRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setMcpRegistryMutationError(
          getOperatorActionError(error, 'Xero could not refresh MCP server statuses.'),
        )
        throw error
      } finally {
        setMcpRegistryMutationStatus('idle')
        setPendingMcpServerId(null)
      }
    },
    [
      adapter,
      applyMcpRegistrySnapshot,
      setMcpRegistryMutationError,
      setMcpRegistryMutationStatus,
      setPendingMcpServerId,
    ],
  )

  const refreshSkillRegistry = useCallback(
    async (options: Parameters<XeroDesktopMutationActions['refreshSkillRegistry']>[0] = {}) => {
      if (skillRegistryLoadInFlightRef.current) {
        return skillRegistryLoadInFlightRef.current
      }

      const projectId = options.projectId ?? activeProjectIdRef.current ?? null
      const cachedRegistry = skillRegistryRef.current
      if (
        !options.force &&
        cachedRegistry &&
        skillRegistryLoadStatus === 'ready' &&
        cachedRegistry.projectId === projectId
      ) {
        return cachedRegistry
      }

      setSkillRegistryLoadStatus('loading')
      setSkillRegistryLoadError(null)

      const loadPromise = (async () => {
        try {
          const response = await adapter.listSkillRegistry({
            projectId,
            query: options.query ?? null,
            includeUnavailable: options.includeUnavailable ?? true,
          })
          return applySkillRegistrySnapshot(response)
        } catch (error) {
          setSkillRegistryLoadStatus('error')
          setSkillRegistryLoadError(
            getOperatorActionError(error, 'Xero could not load app-local skill sources.'),
          )
          throw error
        } finally {
          skillRegistryLoadInFlightRef.current = null
        }
      })()

      skillRegistryLoadInFlightRef.current = loadPromise
      return loadPromise
    },
    [
      activeProjectIdRef,
      adapter,
      applySkillRegistrySnapshot,
      setSkillRegistryLoadError,
      setSkillRegistryLoadStatus,
      skillRegistryLoadInFlightRef,
      skillRegistryLoadStatus,
      skillRegistryRef,
    ],
  )

  const reloadSkillRegistry = useCallback(
    async (options: Parameters<XeroDesktopMutationActions['reloadSkillRegistry']>[0] = {}) => {
      setSkillRegistryLoadStatus('loading')
      setSkillRegistryLoadError(null)

      try {
        const response = await adapter.reloadSkillRegistry({
          projectId: options.projectId ?? activeProjectIdRef.current ?? null,
          query: options.query ?? null,
          includeUnavailable: options.includeUnavailable ?? true,
        })
        return applySkillRegistrySnapshot(response)
      } catch (error) {
        setSkillRegistryLoadStatus('error')
        setSkillRegistryLoadError(
          getOperatorActionError(error, 'Xero could not reload skill sources.'),
        )
        throw error
      }
    },
    [activeProjectIdRef, adapter, applySkillRegistrySnapshot, setSkillRegistryLoadError, setSkillRegistryLoadStatus],
  )

  const setSkillEnabled = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['setSkillEnabled']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(request.sourceId)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.setSkillEnabled(request)
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not update the skill state.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful skill registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const removeSkill = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['removeSkill']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(request.sourceId)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.removeSkill(request)
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not remove the installed skill.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful skill registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const upsertSkillLocalRoot = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['upsertSkillLocalRoot']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(request.rootId ?? request.path)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.upsertSkillLocalRoot({
          ...request,
          projectId: request.projectId ?? activeProjectIdRef.current ?? null,
        })
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not save the local skill source.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful skill registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      activeProjectIdRef,
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const removeSkillLocalRoot = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['removeSkillLocalRoot']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(request.rootId)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.removeSkillLocalRoot({
          ...request,
          projectId: request.projectId ?? activeProjectIdRef.current ?? null,
        })
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not remove the local skill source.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful skill registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      activeProjectIdRef,
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const updateProjectSkillSource = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['updateProjectSkillSource']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(`project:${request.projectId}`)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.updateProjectSkillSource(request)
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not update project skill discovery.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful skill registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const updateGithubSkillSource = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['updateGithubSkillSource']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId('github')
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.updateGithubSkillSource({
          ...request,
          projectId: request.projectId ?? activeProjectIdRef.current ?? null,
        })
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not save the GitHub skill source.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful skill registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      activeProjectIdRef,
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const upsertPluginRoot = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['upsertPluginRoot']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(request.rootId ?? request.path)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.upsertPluginRoot({
          ...request,
          projectId: request.projectId ?? activeProjectIdRef.current ?? null,
        })
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not save the plugin root.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful plugin registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      activeProjectIdRef,
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const removePluginRoot = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['removePluginRoot']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(request.rootId)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.removePluginRoot({
          ...request,
          projectId: request.projectId ?? activeProjectIdRef.current ?? null,
        })
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not remove the plugin root.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful plugin registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      activeProjectIdRef,
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const setPluginEnabled = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['setPluginEnabled']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(`plugin:${request.pluginId}`)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.setPluginEnabled(request)
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not update the plugin state.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful plugin registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const removePlugin = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['removePlugin']>[0]) => {
      setSkillRegistryMutationStatus('running')
      setPendingSkillSourceId(`plugin:${request.pluginId}`)
      setSkillRegistryMutationError(null)

      try {
        const response = await adapter.removePlugin(request)
        const snapshot = applySkillRegistrySnapshot(response)
        setSkillRegistryMutationError(null)
        return snapshot
      } catch (error) {
        setSkillRegistryMutationError(
          getOperatorActionError(error, 'Xero could not remove the plugin.'),
        )

        try {
          await refreshSkillRegistry({ force: true })
        } catch {
          // Preserve the last truthful plugin registry snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setSkillRegistryMutationStatus('idle')
        setPendingSkillSourceId(null)
      }
    },
    [
      adapter,
      applySkillRegistrySnapshot,
      refreshSkillRegistry,
      setPendingSkillSourceId,
      setSkillRegistryMutationError,
      setSkillRegistryMutationStatus,
    ],
  )

  const refreshNotificationRoutes = useCallback(
    async (options: { force?: boolean } = {}) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before loading notification routes.',
      )

      const result = await loadNotificationRoutes(projectId, {
        force: options.force ?? false,
      })

      if (result.loadError) {
        setNotificationRouteLoadStatuses((currentStatuses) => ({
          ...currentStatuses,
          [projectId]: 'error',
        }))
        setNotificationRouteLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: result.loadError,
        }))
      }

      return result.routes
    },
    [
      activeProjectIdRef,
      loadNotificationRoutes,
      setNotificationRouteLoadErrors,
      setNotificationRouteLoadStatuses,
    ],
  )

  const upsertNotificationRoute = useCallback(
    async (request: Parameters<XeroDesktopMutationActions['upsertNotificationRoute']>[0]) => {
      const projectId = getActiveProjectId(
        activeProjectIdRef,
        'Select an imported project before saving a notification route.',
      )

      const trimmedRouteId = request.routeId.trim()
      setNotificationRouteMutationStatus('running')
      setPendingNotificationRouteId(trimmedRouteId.length > 0 ? trimmedRouteId : null)
      setNotificationRouteMutationError(null)

      try {
        const response = await adapter.upsertNotificationRoute({
          ...request,
          projectId,
        })

        setNotificationRoutes((currentRoutes) => {
          const existingRoutes = currentRoutes[projectId] ?? []
          const nextRoutes = [
            response.route,
            ...existingRoutes.filter((route) => route.routeId !== response.route.routeId),
          ]

          return {
            ...currentRoutes,
            [projectId]: nextRoutes,
          }
        })
        setNotificationRouteLoadStatuses((currentStatuses) => ({
          ...currentStatuses,
          [projectId]: 'ready',
        }))
        setNotificationRouteLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: null,
        }))

        void loadNotificationRoutes(projectId, { force: true })
        return response.route
      } catch (error) {
        setNotificationRouteMutationError(
          getOperatorActionError(error, 'Xero could not save the notification route for this project.'),
        )

        try {
          await loadNotificationRoutes(projectId, { force: true })
        } catch {
          // Preserve the last truthful route list when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setNotificationRouteMutationStatus('idle')
        setPendingNotificationRouteId(null)
      }
    },
    [
      activeProjectIdRef,
      adapter,
      loadNotificationRoutes,
      setNotificationRouteLoadErrors,
      setNotificationRouteLoadStatuses,
      setNotificationRouteMutationError,
      setNotificationRouteMutationStatus,
      setNotificationRoutes,
      setPendingNotificationRouteId,
    ],
  )

  return {
    refreshMcpRegistry,
    upsertMcpServer,
    removeMcpServer,
    importMcpServers,
    refreshMcpServerStatuses,
    refreshSkillRegistry,
    reloadSkillRegistry,
    setSkillEnabled,
    removeSkill,
    upsertSkillLocalRoot,
    removeSkillLocalRoot,
    updateProjectSkillSource,
    updateGithubSkillSource,
    upsertPluginRoot,
    removePluginRoot,
    setPluginEnabled,
    removePlugin,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  }
}
