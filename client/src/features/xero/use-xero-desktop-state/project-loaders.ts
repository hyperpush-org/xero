import { startTransition } from 'react'
import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import { getDesktopErrorMessage, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import {
  applyRuntimeRun,
  applyRuntimeSession,
  mapProjectSnapshot,
  type ProjectDetailView,
  type ProjectLoadBundleDiagnosticDto,
  type ProjectLoadBundleDto,
} from '@/src/lib/xero-model'
import { mapAutonomousRunInspection } from '@/src/lib/xero-model/autonomous'
import {
  applyRepositoryStatus,
  mapProjectSummary,
  mapRepositoryStatus,
  upsertProjectListItem,
  type ProjectListItem,
  type RepositoryStatusView,
} from '@/src/lib/xero-model/project'
import {
  mapRuntimeRun,
  mapRuntimeSession,
  type RuntimeRunView,
  type RuntimeSessionView,
} from '@/src/lib/xero-model/runtime'
import type {
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
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
type RepositoryStatusLoadResult = {
  ok: true
  status: RepositoryStatusView
  error: null
} | {
  ok: false
  status: RepositoryStatusView | null
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

function isSupersededProjectLoadError(error: unknown): boolean {
  const code = (error as { code?: unknown } | null)?.code
  return code === 'project_load_bundle_superseded' || code === 'backend_job_stale_result'
}

interface ProjectLoadRefs {
  activeProjectIdRef: MutableRefObject<string | null>
  latestLoadRequestRef: MutableRefObject<number>
  projectDetailsRef: MutableRefObject<Record<string, ProjectDetailView>>
  runtimeSessionsRef: MutableRefObject<RuntimeSessionRecords>
  runtimeRunsRef: MutableRefObject<RuntimeRunRecords>
  autonomousRunsRef: MutableRefObject<AutonomousRunRecords>
}

interface ProjectLoadSetters {
  setProjects: SetState<ProjectListItem[]>
  setActiveProject: SetState<ProjectDetailView | null>
  setActiveProjectId: SetState<string | null>
  setRepositoryStatus: SetState<RepositoryStatusView | null>
  setRuntimeSessions: SetState<RuntimeSessionRecords>
  setRuntimeRuns: SetState<RuntimeRunRecords>
  setAutonomousRuns: SetState<AutonomousRunRecords>
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
}

interface ProjectLoadArgs {
  adapter: XeroDesktopAdapter
  projectId: string
  source: ProjectLoadSource
  applyCachedProject?: boolean
  refs: ProjectLoadRefs
  setters: ProjectLoadSetters
  resetRepositoryDiffs: (status: RepositoryStatusView | null) => void
  getOperatorActionError: (error: unknown, fallback: string) => OperatorActionErrorView
}

function createAutonomousFallbackInspection(projectId: string, refs: ProjectLoadRefs): AutonomousInspection {
  return {
    autonomousRun: refs.autonomousRunsRef.current[projectId] ?? null,
  }
}

function snapshotHasAutonomousRunProjection(
  snapshot: { autonomousRun?: unknown },
): boolean {
  return Object.prototype.hasOwnProperty.call(snapshot, 'autonomousRun')
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

function bundleDiagnostic(
  bundle: ProjectLoadBundleDto,
  section: string,
): ProjectLoadBundleDiagnosticDto | null {
  return bundle.diagnostics.find((diagnostic) => diagnostic.section === section) ?? null
}

function diagnosticMessage(
  bundle: ProjectLoadBundleDto,
  section: string,
): string | null {
  return bundleDiagnostic(bundle, section)?.message ?? null
}

async function loadProjectStateFromBundle({
  adapter,
  projectId,
  requestId,
  bundleLoader,
  cachedRepositoryStatus,
  refs,
  setters,
  resetRepositoryDiffs,
}: ProjectLoadArgs & {
  requestId: number
  bundleLoader: NonNullable<XeroDesktopAdapter['getProjectLoadBundle']>
  cachedRepositoryStatus: RepositoryStatusView | null
}): Promise<ProjectDetailView | null> {
  const bundle = await bundleLoader({
    projectId,
  })

  if (refs.latestLoadRequestRef.current !== requestId) {
    return null
  }

  const snapshotProject = mapProjectSnapshot(bundle.projectSnapshot)
  const selectedAgentSessionId = snapshotProject.selectedAgentSession?.agentSessionId ?? null
  const runtimeRunDiagnostic = bundleDiagnostic(bundle, 'runtimeRun')
  const status = bundle.repositoryStatus ? mapRepositoryStatus(bundle.repositoryStatus) : cachedRepositoryStatus
  const runtime = bundle.runtimeSession ? mapRuntimeSession(bundle.runtimeSession) : refs.runtimeSessionsRef.current[projectId] ?? null
  const cachedRuntimeRun = refs.runtimeRunsRef.current[projectId] ?? null
  let runtimeRun = bundle.runtimeRun
    ? mapRuntimeRun(bundle.runtimeRun)
    : runtimeRunDiagnostic
      ? cachedRuntimeRun
      : null
  if (runtimeRun?.agentSessionId !== selectedAgentSessionId) {
    runtimeRun = null
  }
  let runtimeRunLoadError = runtimeRunDiagnostic?.message ?? null
  if (!bundle.runtimeRun && !runtimeRunDiagnostic && selectedAgentSessionId) {
    try {
      const response = await adapter.getRuntimeRun(projectId, selectedAgentSessionId)
      const loadedRuntimeRun = response ? mapRuntimeRun(response) : null
      runtimeRun = loadedRuntimeRun?.agentSessionId === selectedAgentSessionId ? loadedRuntimeRun : null
      runtimeRunLoadError = null
    } catch (error) {
      runtimeRun = cachedRuntimeRun?.agentSessionId === selectedAgentSessionId ? cachedRuntimeRun : null
      runtimeRunLoadError = getDesktopErrorMessage(error)
    }
  }
  const autonomousInspection = bundle.autonomousRun
    ? mapAutonomousRunInspection(bundle.autonomousRun)
    : {
        autonomousRun:
          refs.autonomousRunsRef.current[projectId] ?? snapshotProject.autonomousRun ?? null,
      }
  const autonomousRun = autonomousInspection.autonomousRun
  const nextSummary = mapProjectSummary(bundle.projectSnapshot.project)
  const finalizedProject = applyAutonomousRunState(
    applyRuntimeRun(
      runtime
        ? applyRuntimeSession(
            status ? applyRepositoryStatus(snapshotProject, status) : snapshotProject,
            runtime,
          )
        : status ? applyRepositoryStatus(snapshotProject, status) : snapshotProject,
      runtimeRun,
    ),
    autonomousRun,
  )
  setters.setProjects((currentProjects) =>
    upsertProjectListItem(
      currentProjects,
      runtime ? applyRuntimeToProjectList(nextSummary, runtime) : nextSummary,
    ),
  )

  startTransition(() => {
    if (runtime) {
      setters.setRuntimeSessions((currentRuntimeSessions) => ({
        ...currentRuntimeSessions,
        [projectId]: runtime,
      }))
    }
    if (runtimeRun) {
      setters.setRuntimeRuns((currentRuntimeRuns) => ({
        ...currentRuntimeRuns,
        [projectId]: runtimeRun,
      }))
    } else if (runtimeRunLoadError === null) {
      setters.setRuntimeRuns((currentRuntimeRuns) => removeProjectRecord(currentRuntimeRuns, projectId))
    }
    applyAutonomousInspectionRecords(projectId, autonomousInspection, setters, {
      allowRemovals: bundleDiagnostic(bundle, 'autonomousRun') === null,
    })
    setters.setRuntimeLoadErrors((currentErrors) => ({
      ...currentErrors,
      [projectId]: diagnosticMessage(bundle, 'runtimeSession'),
    }))
    setters.setRuntimeRunLoadErrors((currentErrors) => ({
      ...currentErrors,
      [projectId]: runtimeRunLoadError,
    }))
    setters.setAutonomousRunLoadErrors((currentErrors) => ({
      ...currentErrors,
      [projectId]: diagnosticMessage(bundle, 'autonomousRun'),
    }))
  })

  setters.setRepositoryStatus(status)
  resetRepositoryDiffs(status)
  setters.setActiveProjectId(projectId)
  setters.setActiveProject(finalizedProject)
  setters.setErrorMessage(
    combineLoadErrors(
      diagnosticMessage(bundle, 'repositoryStatus'),
      diagnosticMessage(bundle, 'runtimeSession'),
      runtimeRunLoadError,
      diagnosticMessage(bundle, 'autonomousRun'),
    ),
  )

  return finalizedProject
}

export async function loadProjectState({
  adapter,
  projectId,
  source,
  applyCachedProject = true,
  refs,
  setters,
  resetRepositoryDiffs,
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

  const cachedProject = refs.projectDetailsRef.current[projectId] ?? null
  const cachedRepositoryStatus = cachedProject?.repositoryStatus ?? null
  if (cachedProject && applyCachedProject) {
    setters.setRepositoryStatus(cachedRepositoryStatus)
    setters.setActiveProjectId(projectId)
    setters.setActiveProject(cachedProject)
    resetRepositoryDiffs(cachedRepositoryStatus)
  }

  // Project rail clicks must land after the lightweight snapshot; the bundle
  // hydrates secondary state and can include expensive git/runtime work.
  const bundleLoader = source === 'selection' ? undefined : adapter.getProjectLoadBundle
  if (bundleLoader) {
    try {
      const result = await loadProjectStateFromBundle({
        adapter,
        projectId,
        source,
        applyCachedProject,
        refs,
        setters,
        resetRepositoryDiffs,
        getOperatorActionError,
        requestId,
        bundleLoader,
        cachedRepositoryStatus,
      })
      if (refs.latestLoadRequestRef.current === requestId) {
        setters.setIsProjectLoading(false)
      }
      return result
    } catch (error) {
      if (
        refs.latestLoadRequestRef.current !== requestId ||
        isSupersededProjectLoadError(error)
      ) {
        return null
      }
      // Fall through to the legacy fan-out loader when running against an
      // older backend or if the bundle command itself fails before diagnostics.
    }
  }

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

  const snapshotPromise = adapter.getProjectSnapshot(projectId)
  const repositoryStatusPromise: Promise<RepositoryStatusLoadResult> = adapter
    .getRepositoryStatus(projectId)
    .then((response) => ({
      ok: true as const,
      status: mapRepositoryStatus(response),
      error: null,
    }))
    .catch((error) => ({
      ok: false as const,
      status: cachedRepositoryStatus,
      error: getDesktopErrorMessage(error),
    }))

  try {
    const snapshotResponse = await snapshotPromise

    if (refs.latestLoadRequestRef.current !== requestId) {
      return null
    }

    const snapshotProject = mapProjectSnapshot(snapshotResponse)
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

    const autonomousRunPromise: Promise<AutonomousRunLoadResult> = snapshotHasAutonomousRunProjection(snapshotResponse)
      ? Promise.resolve({
          ok: true as const,
          inspection: {
            autonomousRun: snapshotProject.autonomousRun ?? null,
          },
          error: null,
        })
      : adapter
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
    const cachedRuntime = refs.runtimeSessionsRef.current[projectId] ?? null
    const cachedRuntimeRun = refs.runtimeRunsRef.current[projectId] ?? null
    const cachedAutonomousRun = refs.autonomousRunsRef.current[projectId] ?? snapshotProject.autonomousRun ?? null
    const nextProject = applyAutonomousRunState(
      applyRuntimeRun(
        applyRuntimeSession(
          cachedRepositoryStatus ? applyRepositoryStatus(snapshotProject, cachedRepositoryStatus) : snapshotProject,
          cachedRuntime,
        ),
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
    setters.setRepositoryStatus(cachedRepositoryStatus)
    setters.setActiveProjectId(projectId)
    setters.setActiveProject(nextProject)
    resetRepositoryDiffs(cachedRepositoryStatus)
    if (source === 'selection') {
      setters.setIsProjectLoading(false)
    }

    const [
      statusResult,
      runtimeResult,
      runtimeRunResult,
      autonomousRunResult,
    ] = await Promise.all([
      repositoryStatusPromise,
      runtimePromise,
      runtimeRunPromise,
      autonomousRunPromise,
    ])
    const canApplySelectionResult =
      source === 'selection' && refs.activeProjectIdRef.current === projectId
    if (refs.latestLoadRequestRef.current !== requestId && !canApplySelectionResult) {
      return nextProject
    }

    const finalStatus = statusResult.status
    const finalRuntime = runtimeResult.runtime ?? cachedRuntime
    const finalRuntimeRun = runtimeRunResult.ok ? runtimeRunResult.runtimeRun : runtimeRunResult.runtimeRun ?? cachedRuntimeRun
    const finalAutonomousRun = autonomousRunResult.ok
      ? autonomousRunResult.inspection.autonomousRun
      : autonomousRunResult.inspection.autonomousRun ?? cachedAutonomousRun
    const finalizedProject = applyAutonomousRunState(
      applyRuntimeRun(
        finalRuntime
          ? applyRuntimeSession(
              finalStatus ? applyRepositoryStatus(nextProject, finalStatus) : nextProject,
              finalRuntime,
            )
          : finalStatus ? applyRepositoryStatus(nextProject, finalStatus) : nextProject,
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

    setters.setRepositoryStatus(finalStatus)
    resetRepositoryDiffs(finalStatus)
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
        statusResult.error,
        runtimeResult.error,
        runtimeRunResult.error,
        autonomousRunResult.error,
      ),
    )

    return finalizedProject
  } catch (error) {
    const canApplySelectionError =
      source === 'selection' && refs.activeProjectIdRef.current === projectId
    if (refs.latestLoadRequestRef.current === requestId || canApplySelectionError) {
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
