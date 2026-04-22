import { useCallback } from 'react'

import { projectRuntimeSettingsFromProviderProfiles } from '@/src/lib/cadence-model/provider-profiles'
import type {
  CadenceDesktopMutationActions,
  UseCadenceDesktopMutationsArgs,
} from './mutation-support'
import {
  getActiveProjectId,
  getOperatorActionError,
} from './mutation-support'

export function useRuntimeSettingsNotificationMutations({
  adapter,
  refs,
  setters,
  operations,
  providerProfilesLoadStatus,
  runtimeSettingsLoadStatus,
}: UseCadenceDesktopMutationsArgs): Pick<
  CadenceDesktopMutationActions,
  | 'refreshProviderProfiles'
  | 'upsertProviderProfile'
  | 'setActiveProviderProfile'
  | 'refreshRuntimeSettings'
  | 'upsertRuntimeSettings'
  | 'refreshNotificationRoutes'
  | 'upsertNotificationRoute'
> {
  const {
    activeProjectIdRef,
    providerProfilesRef,
    providerProfilesLoadInFlightRef,
    runtimeSettingsRef,
    runtimeSettingsLoadInFlightRef,
  } = refs
  const {
    setNotificationRoutes,
    setNotificationRouteLoadStatuses,
    setNotificationRouteLoadErrors,
    setNotificationRouteMutationStatus,
    setPendingNotificationRouteId,
    setNotificationRouteMutationError,
    setProviderProfiles,
    setProviderProfilesLoadStatus,
    setProviderProfilesLoadError,
    setProviderProfilesSaveStatus,
    setProviderProfilesSaveError,
    setRuntimeSettings,
    setRuntimeSettingsLoadStatus,
    setRuntimeSettingsLoadError,
    setRuntimeSettingsSaveStatus,
    setRuntimeSettingsSaveError,
  } = setters
  const { loadNotificationRoutes } = operations

  const applyProviderProfilesSnapshot = useCallback(
    (response: Awaited<ReturnType<typeof adapter.getProviderProfiles>>) => {
      setProviderProfiles(response)
      setProviderProfilesLoadStatus('ready')
      setProviderProfilesLoadError(null)

      const projectedRuntimeSettings = projectRuntimeSettingsFromProviderProfiles(response)
      if (projectedRuntimeSettings) {
        setRuntimeSettings(projectedRuntimeSettings)
        setRuntimeSettingsLoadStatus('ready')
        setRuntimeSettingsLoadError(null)
      }

      return response
    },
    [
      adapter,
      setProviderProfiles,
      setProviderProfilesLoadError,
      setProviderProfilesLoadStatus,
      setRuntimeSettings,
      setRuntimeSettingsLoadError,
      setRuntimeSettingsLoadStatus,
    ],
  )

  const refreshProviderProfiles = useCallback(
    async (options: { force?: boolean } = {}) => {
      if (providerProfilesLoadInFlightRef.current) {
        return providerProfilesLoadInFlightRef.current
      }

      const cachedProviderProfiles = providerProfilesRef.current
      if (!options.force && cachedProviderProfiles && providerProfilesLoadStatus === 'ready') {
        return cachedProviderProfiles
      }

      setProviderProfilesLoadStatus('loading')
      setProviderProfilesLoadError(null)

      const loadPromise = (async () => {
        try {
          const response = await adapter.getProviderProfiles()
          return applyProviderProfilesSnapshot(response)
        } catch (error) {
          setProviderProfilesLoadStatus('error')
          setProviderProfilesLoadError(
            getOperatorActionError(error, 'Cadence could not load app-local provider profiles.'),
          )
          throw error
        } finally {
          providerProfilesLoadInFlightRef.current = null
        }
      })()

      providerProfilesLoadInFlightRef.current = loadPromise
      return loadPromise
    },
    [
      adapter,
      applyProviderProfilesSnapshot,
      providerProfilesLoadInFlightRef,
      providerProfilesLoadStatus,
      providerProfilesRef,
      setProviderProfilesLoadError,
      setProviderProfilesLoadStatus,
    ],
  )

  const upsertProviderProfile = useCallback(
    async (request: Parameters<CadenceDesktopMutationActions['upsertProviderProfile']>[0]) => {
      setProviderProfilesSaveStatus('running')
      setProviderProfilesSaveError(null)

      try {
        const response = await adapter.upsertProviderProfile(request)
        applyProviderProfilesSnapshot(response)
        setProviderProfilesSaveError(null)
        return response
      } catch (error) {
        setProviderProfilesSaveError(
          getOperatorActionError(error, 'Cadence could not save the selected provider profile.'),
        )

        try {
          await refreshProviderProfiles({ force: true })
        } catch {
          // Preserve the last truthful profile snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setProviderProfilesSaveStatus('idle')
      }
    },
    [
      adapter,
      applyProviderProfilesSnapshot,
      refreshProviderProfiles,
      setProviderProfilesSaveError,
      setProviderProfilesSaveStatus,
    ],
  )

  const setActiveProviderProfile = useCallback(
    async (profileId: string) => {
      setProviderProfilesSaveStatus('running')
      setProviderProfilesSaveError(null)

      try {
        const response = await adapter.setActiveProviderProfile(profileId)
        applyProviderProfilesSnapshot(response)
        setProviderProfilesSaveError(null)
        return response
      } catch (error) {
        setProviderProfilesSaveError(
          getOperatorActionError(error, 'Cadence could not switch the active provider profile.'),
        )

        try {
          await refreshProviderProfiles({ force: true })
        } catch {
          // Preserve the last truthful profile snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setProviderProfilesSaveStatus('idle')
      }
    },
    [
      adapter,
      applyProviderProfilesSnapshot,
      refreshProviderProfiles,
      setProviderProfilesSaveError,
      setProviderProfilesSaveStatus,
    ],
  )

  const refreshRuntimeSettings = useCallback(
    async (options: { force?: boolean } = {}) => {
      if (runtimeSettingsLoadInFlightRef.current) {
        return runtimeSettingsLoadInFlightRef.current
      }

      const cachedRuntimeSettings = runtimeSettingsRef.current
      if (!options.force && cachedRuntimeSettings && runtimeSettingsLoadStatus === 'ready') {
        return cachedRuntimeSettings
      }

      setRuntimeSettingsLoadStatus('loading')
      setRuntimeSettingsLoadError(null)

      const loadPromise = (async () => {
        try {
          const response = await adapter.getRuntimeSettings()
          setRuntimeSettings(response)
          setRuntimeSettingsLoadStatus('ready')
          setRuntimeSettingsLoadError(null)
          return response
        } catch (error) {
          setRuntimeSettingsLoadStatus('error')
          setRuntimeSettingsLoadError(
            getOperatorActionError(error, 'Cadence could not load app-global runtime settings.'),
          )
          throw error
        } finally {
          runtimeSettingsLoadInFlightRef.current = null
        }
      })()

      runtimeSettingsLoadInFlightRef.current = loadPromise
      return loadPromise
    },
    [
      adapter,
      runtimeSettingsLoadInFlightRef,
      runtimeSettingsLoadStatus,
      runtimeSettingsRef,
      setRuntimeSettings,
      setRuntimeSettingsLoadError,
      setRuntimeSettingsLoadStatus,
    ],
  )

  const upsertRuntimeSettings = useCallback(
    async (request: Parameters<CadenceDesktopMutationActions['upsertRuntimeSettings']>[0]) => {
      setRuntimeSettingsSaveStatus('running')
      setRuntimeSettingsSaveError(null)

      try {
        const response = await adapter.upsertRuntimeSettings(request)
        setRuntimeSettings(response)
        setRuntimeSettingsLoadStatus('ready')
        setRuntimeSettingsLoadError(null)
        setRuntimeSettingsSaveError(null)

        try {
          await refreshProviderProfiles({ force: true })
        } catch {
          // Keep the truthful compatibility snapshot even if the profile refresh fails.
        }

        return response
      } catch (error) {
        setRuntimeSettingsSaveError(
          getOperatorActionError(error, 'Cadence could not save app-global runtime settings.'),
        )

        try {
          await refreshProviderProfiles({ force: true })
        } catch {
          // Preserve the last truthful provider-profile snapshot when refresh-after-failure also fails.
        }

        throw error
      } finally {
        setRuntimeSettingsSaveStatus('idle')
      }
    },
    [
      adapter,
      refreshProviderProfiles,
      setRuntimeSettings,
      setRuntimeSettingsLoadError,
      setRuntimeSettingsLoadStatus,
      setRuntimeSettingsSaveError,
      setRuntimeSettingsSaveStatus,
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
    async (request: Parameters<CadenceDesktopMutationActions['upsertNotificationRoute']>[0]) => {
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
          getOperatorActionError(error, 'Cadence could not save the notification route for this project.'),
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
    refreshProviderProfiles,
    upsertProviderProfile,
    setActiveProviderProfile,
    refreshRuntimeSettings,
    upsertRuntimeSettings,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  }
}
