import { startTransition } from 'react'
import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import { getDesktopErrorMessage, type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import { applyRuntimeRun, applyRuntimeSession, mapProjectSnapshot, type ProjectDetailView } from '@/src/lib/cadence-model'
import { mapAutonomousRunInspection } from '@/src/lib/cadence-model/autonomous'
import {
  type NotificationDispatchDto,
  type NotificationRouteDto,
  type SyncNotificationAdaptersResponseDto,
} from '@/src/lib/cadence-model/notifications'
import {
  applyRepositoryStatus,
  mapProjectSummary,
  mapRepositoryStatus,
  upsertProjectListItem,
  type ProjectListItem,
  type RepositoryStatusView,
} from '@/src/lib/cadence-model/project'
import {
  mapRuntimeRun,
  mapRuntimeSession,
  type RuntimeRunView,
  type RuntimeSessionView,
} from '@/src/lib/cadence-model/runtime'
import type {
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  NotificationRoutesLoadResult,
  NotificationRoutesLoadStatus,
  OperatorActionErrorView,
  OperatorActionStatus,
  RefreshSource,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
} from './types'

export type ProjectLoadSource = Exclude<RefreshSource, 'repository:status_changed' | 'runtime:updated' | null>

type SetState<T> = Dispatch<SetStateAction<T>>
type RuntimeSessionRecords = Record<string, RuntimeSessionView>
type RuntimeRunRecords = Record<string, RuntimeRunView>
type AutonomousRunRecords = Record<string, NonNullable<ProjectDetailView['autonomousRun']>>
type NotificationSyncSummaryRecords = Record<string, SyncNotificationAdaptersResponseDto | null>
type NotificationRouteRecords = Record<string, NotificationRouteDto[]>
type NotificationRouteErrorRecords = Record<string, OperatorActionErrorView | null>
type NotificationRouteStatusRecords = Record<string, NotificationRoutesLoadStatus>
type RuntimeLoadErrorRecords = Record<string, string | null>
type RuntimeLoadResult = {
  ok: true
  runtime: RuntimeSessionView
  error: null
} | {
  ok: false
  runtime: RuntimeSessionView | null
  error: string
}
type RuntimeRunLoadResult = {
  ok: true
  runtimeRun: RuntimeRunView | null
  error: null
} | {
  ok: false
  runtimeRun: RuntimeRunView | null
  error: string
}
type AutonomousInspection = ReturnType<typeof mapAutonomousRunInspection>
type AutonomousRunLoadResult = {
  ok: true
  inspection: AutonomousInspection
  error: null
} | {
  ok: false
  inspection: AutonomousInspection
  error: string
}

export function applyRuntimeToProjectList(project: ProjectListItem, runtimeSession: RuntimeSessionView): ProjectListItem {
  return {
    ...project,
    runtime: runtimeSession.runtimeLabel,
    runtimeLabel: runtimeSession.runtimeLabel,
  }
}

export function applyAutonomousRunState(
  project: ProjectDetailView,
  autonomousRun: ProjectDetailView['autonomousRun'],
): ProjectDetailView {
  return {
    ...project,
    autonomousRun: autonomousRun ?? null,
  }
}

export function removeProjectRecord<T>(records: Record<string, T>, projectId: string): Record<string, T> {
  if (!(projectId in records)) {
    return records
  }

  const nextRecords = { ...records }
  delete nextRecords[projectId]
  return nextRecords
}

function combineLoadErrors(...errors: Array<string | null | undefined>): string | null {
  const messages = Array.from(
    new Set(
      errors
        .map((error) => (typeof error === 'string' ? error.trim() : ''))
        .filter((error) => error.length > 0),
    ),
  )

  if (messages.length === 0) {
    return null
  }

  return messages.join(' ')
}

interface NotificationRouteLoaderArgs {
  adapter: CadenceDesktopAdapter
  projectId: string
  force?: boolean
  notificationRoutesRef: MutableRefObject<NotificationRouteRecords>
  notificationRouteLoadErrorsRef: MutableRefObject<NotificationRouteErrorRecords>
  notificationRouteLoadRequestRef: MutableRefObject<Record<string, number>>
  notificationRouteLoadInFlightRef: MutableRefObject<Record<string, Promise<NotificationRoutesLoadResult>>>
  setNotificationRoutes: SetState<NotificationRouteRecords>
  setNotificationRouteLoadStatuses: SetState<NotificationRouteStatusRecords>
  setNotificationRouteLoadErrors: SetState<NotificationRouteErrorRecords>
  getOperatorActionError: (error: unknown, fallback: string) => OperatorActionErrorView
}

export function loadNotificationRoutesForProject({
  adapter,
  projectId,
  force = false,
  notificationRoutesRef,
  notificationRouteLoadErrorsRef,
  notificationRouteLoadRequestRef,
  notificationRouteLoadInFlightRef,
  setNotificationRoutes,
  setNotificationRouteLoadStatuses,
  setNotificationRouteLoadErrors,
  getOperatorActionError,
}: NotificationRouteLoaderArgs): Promise<NotificationRoutesLoadResult> {
  const inFlightRequest = notificationRouteLoadInFlightRef.current[projectId]
  if (!force && inFlightRequest) {
    return inFlightRequest
  }

  const cachedRoutes = notificationRoutesRef.current[projectId] ?? []
  const cachedLoadError = notificationRouteLoadErrorsRef.current[projectId] ?? null
  const nextRequestId = (notificationRouteLoadRequestRef.current[projectId] ?? 0) + 1
  notificationRouteLoadRequestRef.current[projectId] = nextRequestId

  setNotificationRouteLoadStatuses((currentStatuses) => ({
    ...currentStatuses,
    [projectId]: 'loading',
  }))
  setNotificationRouteLoadErrors((currentErrors) => ({
    ...currentErrors,
    [projectId]: null,
  }))

  const requestPromise: Promise<NotificationRoutesLoadResult> = adapter
    .listNotificationRoutes(projectId)
    .then((response) => {
      if (notificationRouteLoadRequestRef.current[projectId] !== nextRequestId) {
        return {
          routes: notificationRoutesRef.current[projectId] ?? cachedRoutes,
          loadError: notificationRouteLoadErrorsRef.current[projectId] ?? cachedLoadError,
        }
      }

      const inScopeRoutes = response.routes.filter(
        (route) => route.projectId === projectId && route.routeId.trim().length > 0,
      )

      setNotificationRoutes((currentRoutes) => ({
        ...currentRoutes,
        [projectId]: inScopeRoutes,
      }))
      setNotificationRouteLoadStatuses((currentStatuses) => ({
        ...currentStatuses,
        [projectId]: 'ready',
      }))
      setNotificationRouteLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: null,
      }))

      return {
        routes: inScopeRoutes,
        loadError: null,
      }
    })
    .catch((error) => {
      if (notificationRouteLoadRequestRef.current[projectId] !== nextRequestId) {
        return {
          routes: notificationRoutesRef.current[projectId] ?? cachedRoutes,
          loadError: notificationRouteLoadErrorsRef.current[projectId] ?? cachedLoadError,
        }
      }

      const loadError = getOperatorActionError(error, 'Cadence could not load notification routes for this project.')
      setNotificationRouteLoadStatuses((currentStatuses) => ({
        ...currentStatuses,
        [projectId]: 'error',
      }))
      setNotificationRouteLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: loadError,
      }))

      return {
        routes: notificationRoutesRef.current[projectId] ?? cachedRoutes,
        loadError,
      }
    })
    .finally(() => {
      if (notificationRouteLoadInFlightRef.current[projectId] === requestPromise) {
        delete notificationRouteLoadInFlightRef.current[projectId]
      }
    })

  notificationRouteLoadInFlightRef.current[projectId] = requestPromise
  return requestPromise
}

interface ProjectLoadRefs {
  latestLoadRequestRef: MutableRefObject<number>
  runtimeSessionsRef: MutableRefObject<RuntimeSessionRecords>
  runtimeRunsRef: MutableRefObject<RuntimeRunRecords>
  autonomousRunsRef: MutableRefObject<AutonomousRunRecords>
  notificationSyncSummariesRef: MutableRefObject<NotificationSyncSummaryRecords>
  notificationDispatchesRef: MutableRefObject<Record<string, NotificationDispatchDto[]>>
  notificationRoutesRef: MutableRefObject<NotificationRouteRecords>
}

interface ProjectLoadSetters {
  setProjects: SetState<ProjectListItem[]>
  setActiveProject: SetState<ProjectDetailView | null>
  setActiveProjectId: SetState<string | null>
  setRepositoryStatus: SetState<RepositoryStatusView | null>
  setRuntimeSessions: SetState<RuntimeSessionRecords>
  setRuntimeRuns: SetState<RuntimeRunRecords>
  setAutonomousRuns: SetState<AutonomousRunRecords>
  setNotificationSyncSummaries: SetState<NotificationSyncSummaryRecords>
  setNotificationSyncErrors: SetState<NotificationRouteErrorRecords>
  setRuntimeLoadErrors: SetState<RuntimeLoadErrorRecords>
  setRuntimeRunLoadErrors: SetState<RuntimeLoadErrorRecords>
  setAutonomousRunLoadErrors: SetState<RuntimeLoadErrorRecords>
  setIsProjectLoading: SetState<boolean>
  setRefreshSource: SetState<RefreshSource>
  setErrorMessage: SetState<string | null>
  setOperatorActionError: SetState<OperatorActionErrorView | null>
  setPendingOperatorActionId: SetState<string | null>
  setOperatorActionStatus: SetState<OperatorActionStatus>
  setRuntimeRunActionError: SetState<OperatorActionErrorView | null>
  setPendingRuntimeRunAction: SetState<RuntimeRunActionKind | null>
  setRuntimeRunActionStatus: SetState<RuntimeRunActionStatus>
  setAutonomousRunActionError: SetState<OperatorActionErrorView | null>
  setPendingAutonomousRunAction: SetState<AutonomousRunActionKind | null>
  setAutonomousRunActionStatus: SetState<AutonomousRunActionStatus>
  setNotificationRouteMutationError: SetState<OperatorActionErrorView | null>
}

interface ProjectLoadArgs {
  adapter: CadenceDesktopAdapter
  projectId: string
  source: ProjectLoadSource
  refs: ProjectLoadRefs
  setters: ProjectLoadSetters
  resetRepositoryDiffs: (status: RepositoryStatusView | null) => void
  loadNotificationRoutes: (projectId: string, options?: { force?: boolean }) => Promise<NotificationRoutesLoadResult>
  getOperatorActionError: (error: unknown, fallback: string) => OperatorActionErrorView
}

function createAutonomousFallbackInspection(projectId: string, refs: ProjectLoadRefs): AutonomousInspection {
  return {
    autonomousRun: refs.autonomousRunsRef.current[projectId] ?? null,
  }
}

function applyAutonomousInspectionRecords(
  projectId: string,
  inspection: AutonomousInspection,
  setters: Pick<ProjectLoadSetters, 'setAutonomousRuns'>,
  options: { allowRemovals: boolean },
) {
  const { allowRemovals } = options

  if (allowRemovals) {
    setters.setAutonomousRuns((currentRuns) => {
      const nextRun = inspection.autonomousRun
      if (!nextRun) {
        return removeProjectRecord(currentRuns, projectId)
      }

      return {
        ...currentRuns,
        [projectId]: nextRun,
      }
    })
  } else {
    const nextRun = inspection.autonomousRun
    if (nextRun) {
      setters.setAutonomousRuns((currentRuns) => ({
        ...currentRuns,
        [projectId]: nextRun,
      }))
    }
  }
}

export async function loadProjectState({
  adapter,
  projectId,
  source,
  refs,
  setters,
  resetRepositoryDiffs,
  loadNotificationRoutes,
  getOperatorActionError,
}: ProjectLoadArgs): Promise<ProjectDetailView | null> {
  const requestId = refs.latestLoadRequestRef.current + 1
  refs.latestLoadRequestRef.current = requestId
  setters.setIsProjectLoading(true)
  setters.setRefreshSource(source)
  setters.setErrorMessage(null)

  if (source !== 'operator:resolve' && source !== 'operator:resume') {
    setters.setOperatorActionError(null)
    setters.setPendingOperatorActionId(null)
    setters.setOperatorActionStatus('idle')
  }

  setters.setRuntimeRunActionError(null)
  setters.setPendingRuntimeRunAction(null)
  setters.setRuntimeRunActionStatus('idle')
  setters.setAutonomousRunActionError(null)
  setters.setPendingAutonomousRunAction(null)
  setters.setAutonomousRunActionStatus('idle')
  setters.setNotificationRouteMutationError(null)

  const runtimePromise: Promise<RuntimeLoadResult> = adapter
    .getRuntimeSession(projectId)
    .then((response) => ({
      ok: true as const,
      runtime: mapRuntimeSession(response),
      error: null,
    }))
    .catch((error) => ({
      ok: false as const,
      runtime: refs.runtimeSessionsRef.current[projectId] ?? null,
      error: getDesktopErrorMessage(error),
    }))

  const shouldSyncNotificationAdapters = source !== 'runtime_run:updated'
  const syncPromise: Promise<{
    attempted: boolean
    summary: SyncNotificationAdaptersResponseDto | null
    error: OperatorActionErrorView | null
    errorMessage: string | null
  }> = shouldSyncNotificationAdapters
    ? adapter
        .syncNotificationAdapters(projectId)
        .then((summary) => ({
          attempted: true as const,
          summary,
          error: null,
          errorMessage: null,
        }))
        .catch((error) => {
          const metadata = getOperatorActionError(
            error,
            'Cadence could not sync notification adapters for this project.',
          )
          return {
            attempted: true as const,
            summary: refs.notificationSyncSummariesRef.current[projectId] ?? null,
            error: metadata,
            errorMessage: metadata.message,
          }
        })
    : Promise.resolve({
        attempted: false as const,
        summary: refs.notificationSyncSummariesRef.current[projectId] ?? null,
        error: null,
        errorMessage: null,
      })

  const brokerPromise: Promise<{
    ok: boolean
    dispatches: NotificationDispatchDto[]
    error: string | null
  }> = adapter
    .listNotificationDispatches(projectId)
    .then((response) => ({
      ok: true as const,
      dispatches: response.dispatches,
      error: null,
    }))
    .catch((error) => ({
      ok: false as const,
      dispatches: refs.notificationDispatchesRef.current[projectId] ?? [],
      error: getDesktopErrorMessage(error),
    }))

  const shouldRefreshRoutes = source !== 'runtime_run:updated' && source !== 'runtime_stream:action_required'
  const routePromise: Promise<{
    ok: boolean
    routes: NotificationRouteDto[]
    error: string | null
  }> = shouldRefreshRoutes
    ? loadNotificationRoutes(projectId, {
        force: source === 'startup' || source === 'selection' || source === 'import',
      }).then((result) => ({
        ok: result.loadError === null,
        routes: result.routes,
        error: result.loadError?.message ?? null,
      }))
    : Promise.resolve({
        ok: true as const,
        routes: refs.notificationRoutesRef.current[projectId] ?? [],
        error: null,
      })

  const snapshotPromise = adapter.getProjectSnapshot(projectId)
  const repositoryStatusPromise = adapter.getRepositoryStatus(projectId)

  try {
    const [syncResult, snapshotResponse, statusResponse, brokerResult, routeResult] = await Promise.all([
      syncPromise,
      snapshotPromise,
      repositoryStatusPromise,
      brokerPromise,
      routePromise,
    ])

    if (refs.latestLoadRequestRef.current !== requestId) {
      return null
    }

    if (syncResult.attempted) {
      if (syncResult.summary) {
        setters.setNotificationSyncSummaries((currentSummaries) => ({
          ...currentSummaries,
          [projectId]: syncResult.summary,
        }))
      }

      setters.setNotificationSyncErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: syncResult.error,
      }))
    }

    refs.notificationDispatchesRef.current[projectId] = brokerResult.dispatches
    const snapshotProject = mapProjectSnapshot(snapshotResponse, {
      notificationDispatches: brokerResult.dispatches,
    })
    const agentSessionId = snapshotProject.selectedAgentSessionId
    const runtimeRunPromise: Promise<RuntimeRunLoadResult> = adapter
      .getRuntimeRun(projectId, agentSessionId)
      .then((response) => ({
        ok: true as const,
        runtimeRun: response ? mapRuntimeRun(response) : null,
        error: null,
      }))
      .catch((error) => ({
        ok: false as const,
        runtimeRun: refs.runtimeRunsRef.current[projectId] ?? null,
        error: getDesktopErrorMessage(error),
      }))

    const autonomousRunPromise: Promise<AutonomousRunLoadResult> = adapter
      .getAutonomousRun(projectId, agentSessionId)
      .then((response) => ({
        ok: true as const,
        inspection: mapAutonomousRunInspection(response),
        error: null,
      }))
      .catch((error) => ({
        ok: false as const,
        inspection: createAutonomousFallbackInspection(projectId, refs),
        error: getDesktopErrorMessage(error),
      }))
    const status = mapRepositoryStatus(statusResponse)
    const cachedRuntime = refs.runtimeSessionsRef.current[projectId] ?? null
    const cachedRuntimeRun = refs.runtimeRunsRef.current[projectId] ?? null
    const cachedAutonomousRun = refs.autonomousRunsRef.current[projectId] ?? snapshotProject.autonomousRun ?? null
    const nextProject = applyAutonomousRunState(
      applyRuntimeRun(
        applyRuntimeSession(applyRepositoryStatus(snapshotProject, status), cachedRuntime),
        cachedRuntimeRun,
      ),
      cachedAutonomousRun,
    )
    const nextSummary = mapProjectSummary(snapshotResponse.project)

    setters.setProjects((currentProjects) =>
      upsertProjectListItem(
        currentProjects,
        cachedRuntime ? applyRuntimeToProjectList(nextSummary, cachedRuntime) : nextSummary,
      ),
    )
    setters.setRepositoryStatus(status)
    setters.setActiveProjectId(projectId)
    setters.setActiveProject(nextProject)
    resetRepositoryDiffs(status)

    const [runtimeResult, runtimeRunResult, autonomousRunResult] = await Promise.all([
      runtimePromise,
      runtimeRunPromise,
      autonomousRunPromise,
    ])
    if (refs.latestLoadRequestRef.current !== requestId) {
      return nextProject
    }

    const finalRuntime = runtimeResult.runtime ?? cachedRuntime
    const finalRuntimeRun = runtimeRunResult.ok ? runtimeRunResult.runtimeRun : runtimeRunResult.runtimeRun ?? cachedRuntimeRun
    const finalAutonomousRun = autonomousRunResult.ok
      ? autonomousRunResult.inspection.autonomousRun
      : autonomousRunResult.inspection.autonomousRun ?? cachedAutonomousRun
    const finalizedProject = applyAutonomousRunState(
      applyRuntimeRun(
        finalRuntime ? applyRuntimeSession(nextProject, finalRuntime) : nextProject,
        finalRuntimeRun,
      ),
      finalAutonomousRun,
    )

    // Runtime/run/autonomous records and their load-error flags are secondary
    // data that the import UI (and most other UI) doesn't depend on directly.
    // Wrapping them in startTransition tells React these are non-urgent updates:
    // it batches them at lower priority and won't interrupt a higher-priority
    // paint (e.g. the busy→idle transition on the import screen) to apply them.
    // This is the React equivalent of Zed/GPUI's 4 ms event-coalescing window —
    // defer slow-path work so the visible UI stays stable during async loading.
    startTransition(() => {
      const nextRuntime = runtimeResult.runtime
      if (nextRuntime) {
        setters.setRuntimeSessions((currentRuntimeSessions) => ({
          ...currentRuntimeSessions,
          [projectId]: nextRuntime,
        }))
        setters.setProjects((currentProjects) =>
          currentProjects.map((project) =>
            project.id === projectId ? applyRuntimeToProjectList(project, nextRuntime) : project,
          ),
        )
      }

      if (runtimeRunResult.ok) {
        setters.setRuntimeRuns((currentRuntimeRuns) => {
          const nextRuntimeRun = runtimeRunResult.runtimeRun
          if (!nextRuntimeRun) {
            return removeProjectRecord(currentRuntimeRuns, projectId)
          }

          return {
            ...currentRuntimeRuns,
            [projectId]: nextRuntimeRun,
          }
        })
      } else {
        const nextRuntimeRun = runtimeRunResult.runtimeRun
        if (nextRuntimeRun) {
          setters.setRuntimeRuns((currentRuntimeRuns) => ({
            ...currentRuntimeRuns,
            [projectId]: nextRuntimeRun,
          }))
        }
      }

      applyAutonomousInspectionRecords(projectId, autonomousRunResult.inspection, setters, {
        allowRemovals: autonomousRunResult.ok,
      })

      setters.setRuntimeLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: runtimeResult.error,
      }))
      setters.setRuntimeRunLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: runtimeRunResult.error,
      }))
      setters.setAutonomousRunLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: autonomousRunResult.error,
      }))
    })

    // setActiveProject and setErrorMessage remain urgent — they drive
    // the import-complete transition and any error banner.
    setters.setActiveProject((currentProject) => {
      if (!currentProject || currentProject.id !== projectId) {
        return currentProject
      }

      return finalizedProject
    })
    setters.setErrorMessage(
      combineLoadErrors(
        syncResult.errorMessage,
        brokerResult.error,
        routeResult.error,
        runtimeResult.error,
        runtimeRunResult.error,
        autonomousRunResult.error,
      ),
    )

    return finalizedProject
  } catch (error) {
    if (refs.latestLoadRequestRef.current === requestId) {
      const nextMessage = getDesktopErrorMessage(error)
      setters.setErrorMessage(nextMessage)

      if (source === 'operator:resolve' || source === 'operator:resume') {
        setters.setOperatorActionError(getOperatorActionError(error, nextMessage))
      }
    }

    return null
  } finally {
    if (refs.latestLoadRequestRef.current === requestId) {
      setters.setIsProjectLoading(false)
    }
  }
}
