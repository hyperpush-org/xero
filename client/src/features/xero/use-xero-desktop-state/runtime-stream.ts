import { startTransition, type Dispatch, type MutableRefObject, type SetStateAction } from 'react'
import { XeroDesktopError, type XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import { applyRuntimeRun, applyRuntimeSession, type ProjectDetailView } from '@/src/lib/xero-model'
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
  mergeRuntimeUpdated,
  type RuntimeRunUpdatedPayloadDto,
  type RuntimeRunView,
  type RuntimeSessionView,
} from '@/src/lib/xero-model/runtime'
import {
  applyRuntimeStreamIssue,
  createRuntimeStreamFromSubscription,
  createRuntimeStreamViewFromSnapshot,
  createRuntimeStreamView,
  mergeRuntimeStreamEvent,
  type RuntimeStreamEventDto,
  type RuntimeStreamItemKindDto,
  type RuntimeStreamPatchDto,
  type RuntimeStreamView,
} from '@/src/lib/xero-model/runtime-stream'

import {
  BLOCKED_NOTIFICATION_SYNC_POLL_MS,
  getBlockedNotificationSyncPollKey,
  type BlockedNotificationSyncPollTarget,
} from './notification-health'
import {
  applyRuntimeToProjectList,
  removeProjectRecord,
  type ProjectLoadSource,
} from './project-loaders'
import { removeRuntimeStreamsForProject } from './high-churn-store'
import { createRepositoryStatusSyncKey } from './repository-status'
import type { RefreshSource } from './types'

export const RUNTIME_STREAM_BATCH_WINDOW_MS = 6
export const REPOSITORY_STATUS_BATCH_WINDOW_MS = 6
export const RUNTIME_RUN_UPDATE_BATCH_WINDOW_MS = 24
const INCREMENTAL_RUNTIME_STREAM_REPLAY_LIMIT = 200

export const ACTIVE_RUNTIME_STREAM_ITEM_KINDS: RuntimeStreamItemKindDto[] = [
  'transcript',
  'tool',
  'skill',
  'activity',
  'action_required',
  'complete',
  'failure',
]

type SetState<T> = Dispatch<SetStateAction<T>>
type RuntimeStreamUpdater = (current: RuntimeStreamView | null) => RuntimeStreamView | null

type RuntimeSessionRecords = Record<string, RuntimeSessionView>
type RuntimeLoadErrorRecords = Record<string, string | null>
type RuntimeStreamRecords = Record<string, RuntimeStreamView>
type RuntimeStreamChannelPayload = RuntimeStreamEventDto | RuntimeStreamPatchDto

type UpdateRuntimeStream = (
  projectId: string,
  agentSessionId: string,
  updater: RuntimeStreamUpdater,
) => void

export type RuntimeMetadataRefreshSource = Extract<
  RefreshSource,
  'runtime_run:updated' | 'runtime_stream:action_required'
>

interface RuntimeMetadataRefreshRefs {
  activeProjectIdRef: MutableRefObject<string | null>
  pendingRuntimeRefreshRef: MutableRefObject<{
    projectId: string
    source: RuntimeMetadataRefreshSource
  } | null>
  runtimeRefreshTimeoutRef: MutableRefObject<ReturnType<typeof setTimeout> | null>
}

interface BlockedNotificationSyncPollRefs {
  activeProjectIdRef: MutableRefObject<string | null>
  blockedNotificationSyncPollTimeoutRef: MutableRefObject<ReturnType<typeof setTimeout> | null>
  blockedNotificationSyncPollTargetRef: MutableRefObject<BlockedNotificationSyncPollTarget | null>
  blockedNotificationSyncPollInFlightRef: MutableRefObject<boolean>
}

interface AttachDesktopRuntimeListenersRefs {
  activeProjectIdRef: MutableRefObject<string | null>
  runtimeSessionsRef: MutableRefObject<RuntimeSessionRecords>
  repositoryStatusSyncKeyRef: MutableRefObject<string>
}

interface AttachDesktopRuntimeListenersSetters {
  setProjects: SetState<ProjectListItem[]>
  setRefreshSource: SetState<RefreshSource>
  setRepositoryStatus: SetState<RepositoryStatusView | null>
  setActiveProject: SetState<ProjectDetailView | null>
  setRuntimeSessions: SetState<RuntimeSessionRecords>
  setRuntimeLoadErrors: SetState<RuntimeLoadErrorRecords>
  setRuntimeStreams: SetState<RuntimeStreamRecords>
  setErrorMessage: SetState<string | null>
}

interface AttachDesktopRuntimeListenersArgs {
  adapter: XeroDesktopAdapter
  refs: AttachDesktopRuntimeListenersRefs
  setters: AttachDesktopRuntimeListenersSetters
  handleAdapterEventError: (error: XeroDesktopError) => void
  applyRuntimeRunUpdate: (
    projectId: string,
    runtimeRun: RuntimeRunView | null,
    options?: { clearGlobalError?: boolean; loadError?: string | null },
  ) => RuntimeRunView | null
  loadProject: (projectId: string, source: ProjectLoadSource) => Promise<ProjectDetailView | null>
  resetRepositoryDiffs: (status: RepositoryStatusView | null) => void
}

interface AttachRuntimeStreamSubscriptionArgs {
  projectId: string | null
  agentSessionId: string | null
  runtimeSession: RuntimeSessionView | null
  runId: string | null
  adapter: XeroDesktopAdapter
  runtimeActionRefreshKeysRef: MutableRefObject<Record<string, Set<string>>>
  updateRuntimeStream: UpdateRuntimeStream
  scheduleRuntimeMetadataRefresh: (projectId: string, source: RuntimeMetadataRefreshSource) => void
}

type ScheduledFlushCancel = () => void
type FlushScheduler = (callback: () => void) => ScheduledFlushCancel

interface RuntimeStreamEventBufferArgs {
  projectId: string
  agentSessionId: string
  runtimeKind: string
  runId: string
  sessionId: string | null
  flowId: string | null
  subscribedItemKinds: RuntimeStreamItemKindDto[]
  runtimeActionRefreshKeysRef: MutableRefObject<Record<string, Set<string>>>
  updateRuntimeStream: UpdateRuntimeStream
  scheduleRuntimeMetadataRefresh: (projectId: string, source: RuntimeMetadataRefreshSource) => void
  scheduleFlush?: FlushScheduler
}

interface RuntimeRunUpdateBufferArgs {
  activeProjectIdRef: MutableRefObject<string | null>
  applyRuntimeRunUpdate: (
    projectId: string,
    runtimeRun: RuntimeRunView | null,
    options?: { clearGlobalError?: boolean; loadError?: string | null },
  ) => RuntimeRunView | null
  setRefreshSource: SetState<RefreshSource>
  setErrorMessage: SetState<string | null>
  scheduleFlush?: FlushScheduler
}

export interface RuntimeRunUpdateBuffer {
  enqueue: (payload: RuntimeRunUpdatedPayloadDto) => void
  flush: () => void
  dispose: () => void
}

export interface RuntimeStreamEventBuffer {
  enqueue: (payload: RuntimeStreamChannelPayload) => void
  reportIssue: (issue: { code: string; message: string; retryable: boolean }) => void
  flush: () => void
  dispose: () => void
}

function getRuntimeStreamIssue(
  error: unknown,
  fallback: { code: string; message: string; retryable: boolean },
) {
  if (error instanceof XeroDesktopError) {
    return {
      code: error.code,
      message: error.message,
      retryable: error.retryable,
    }
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return {
      code: fallback.code,
      message: error.message,
      retryable: fallback.retryable,
    }
  }

  return fallback
}

function scheduleFrameOrTimeout(callback: () => void, windowMs: number): ScheduledFlushCancel {
  let didRun = false
  let frameId: number | null = null
  let timeoutId: ReturnType<typeof setTimeout> | null = null

  const run = () => {
    if (didRun) {
      return
    }

    didRun = true
    if (frameId !== null && typeof window !== 'undefined' && typeof window.cancelAnimationFrame === 'function') {
      window.cancelAnimationFrame(frameId)
    }
    if (timeoutId !== null) {
      clearTimeout(timeoutId)
    }
    frameId = null
    timeoutId = null
    callback()
  }

  if (typeof window !== 'undefined' && typeof window.requestAnimationFrame === 'function') {
    frameId = window.requestAnimationFrame(run)
  }
  timeoutId = setTimeout(run, windowMs)

  return () => {
    if (didRun) {
      return
    }

    didRun = true
    if (frameId !== null && typeof window !== 'undefined' && typeof window.cancelAnimationFrame === 'function') {
      window.cancelAnimationFrame(frameId)
    }
    if (timeoutId !== null) {
      clearTimeout(timeoutId)
    }
    frameId = null
    timeoutId = null
  }
}

function scheduleRuntimeStreamFlush(callback: () => void): ScheduledFlushCancel {
  return scheduleFrameOrTimeout(callback, RUNTIME_STREAM_BATCH_WINDOW_MS)
}

function scheduleRepositoryStatusFlush(callback: () => void): ScheduledFlushCancel {
  return scheduleFrameOrTimeout(callback, REPOSITORY_STATUS_BATCH_WINDOW_MS)
}

function scheduleRuntimeRunUpdateFlush(callback: () => void): ScheduledFlushCancel {
  const timeoutId = setTimeout(callback, RUNTIME_RUN_UPDATE_BATCH_WINDOW_MS)
  return () => clearTimeout(timeoutId)
}

function isRuntimeStreamPatch(payload: RuntimeStreamChannelPayload): payload is RuntimeStreamPatchDto {
  return 'schema' in payload && payload.schema === 'xero.runtime_stream_patch.v1'
}

function getRuntimeStreamPayloadItem(payload: RuntimeStreamChannelPayload) {
  return payload.item
}

export function isUrgentRuntimeStreamEvent(event: RuntimeStreamChannelPayload): boolean {
  const item = getRuntimeStreamPayloadItem(event)
  return (
    item.kind === 'action_required' ||
    item.kind === 'complete' ||
    item.kind === 'failure'
  )
}

function applyRuntimeStreamEventIssue(
  currentStream: RuntimeStreamView | null,
  event: RuntimeStreamEventDto,
  issue: { code: string; message: string; retryable: boolean },
): RuntimeStreamView {
  return applyRuntimeStreamIssue(currentStream, {
    projectId: event.projectId,
    agentSessionId: event.agentSessionId,
    runtimeKind: event.runtimeKind,
    runId: event.runId,
    sessionId: event.sessionId,
    flowId: event.flowId,
    subscribedItemKinds: event.subscribedItemKinds,
    code: issue.code,
    message: issue.message,
    retryable: issue.retryable,
  })
}

export function mergeRuntimeStreamEvents(
  currentStream: RuntimeStreamView | null,
  events: RuntimeStreamChannelPayload[],
): RuntimeStreamView | null {
  let nextStream = currentStream

  for (const event of events) {
    try {
      nextStream = isRuntimeStreamPatch(event)
        ? createRuntimeStreamViewFromSnapshot(event.snapshot)
        : mergeRuntimeStreamEvent(nextStream, event)
    } catch (error) {
      const issue = getRuntimeStreamIssue(error, {
        code: 'runtime_stream_contract_mismatch',
        message: 'Xero ignored a malformed runtime stream item to preserve the last truthful stream state.',
        retryable: false,
      })

      nextStream = isRuntimeStreamPatch(event)
        ? applyRuntimeStreamIssue(nextStream, {
            projectId: event.snapshot.projectId,
            agentSessionId: event.snapshot.agentSessionId,
            runtimeKind: event.snapshot.runtimeKind,
            runId: event.snapshot.runId,
            sessionId: event.snapshot.sessionId,
            flowId: event.snapshot.flowId,
            subscribedItemKinds: event.snapshot.subscribedItemKinds,
            code: issue.code,
            message: issue.message,
            retryable: issue.retryable,
          })
        : applyRuntimeStreamEventIssue(nextStream, event, issue)
    }
  }

  return nextStream
}

function scheduleRuntimeActionRefreshes(
  projectId: string,
  events: RuntimeStreamChannelPayload[],
  runtimeActionRefreshKeysRef: MutableRefObject<Record<string, Set<string>>>,
  scheduleRuntimeMetadataRefresh: (projectId: string, source: RuntimeMetadataRefreshSource) => void,
) {
  for (const event of events) {
    const item = getRuntimeStreamPayloadItem(event)
    if (item.kind !== 'action_required') {
      continue
    }

    const actionId = item.actionId?.trim()
    if (!actionId) {
      continue
    }

    const agentSessionId = isRuntimeStreamPatch(event)
      ? event.snapshot.agentSessionId
      : event.agentSessionId
    const runId = isRuntimeStreamPatch(event) ? event.snapshot.runId : event.runId
    const refreshKey = `${agentSessionId}:${runId}:${actionId}`
    const knownKeys = runtimeActionRefreshKeysRef.current[projectId] ?? new Set<string>()
    runtimeActionRefreshKeysRef.current[projectId] = knownKeys

    if (!knownKeys.has(refreshKey)) {
      knownKeys.add(refreshKey)
      scheduleRuntimeMetadataRefresh(projectId, 'runtime_stream:action_required')
    }
  }
}

export function createRuntimeRunUpdateBuffer({
  activeProjectIdRef,
  applyRuntimeRunUpdate,
  setRefreshSource,
  setErrorMessage,
  scheduleFlush = scheduleRuntimeRunUpdateFlush,
}: RuntimeRunUpdateBufferArgs): RuntimeRunUpdateBuffer {
  const pendingUpdates = new Map<string, {
    projectId: string
    agentSessionId: string
    runtimeRun: RuntimeRunView | null
  }>()
  let cancelScheduledFlush: ScheduledFlushCancel | null = null
  let disposed = false

  const cancelFlush = () => {
    if (!cancelScheduledFlush) {
      return
    }

    const cancel = cancelScheduledFlush
    cancelScheduledFlush = null
    cancel()
  }

  const flush = () => {
    if (disposed) {
      return
    }

    cancelFlush()
    if (pendingUpdates.size === 0) {
      return
    }

    const updates = Array.from(pendingUpdates.values())
    pendingUpdates.clear()
    const activeProjectId = activeProjectIdRef.current
    const touchesActiveProject = updates.some((update) => update.projectId === activeProjectId)

    startTransition(() => {
      for (const update of updates) {
        applyRuntimeRunUpdate(update.projectId, update.runtimeRun)
      }

      if (touchesActiveProject) {
        setRefreshSource('runtime_run:updated')
        setErrorMessage(null)
      }
    })
  }

  return {
    enqueue(payload) {
      if (disposed) {
        return
      }

      pendingUpdates.set(`${payload.projectId}:${payload.agentSessionId}`, {
        projectId: payload.projectId,
        agentSessionId: payload.agentSessionId,
        runtimeRun: payload.run ? mapRuntimeRun(payload.run) : null,
      })

      if (!cancelScheduledFlush) {
        cancelScheduledFlush = scheduleFlush(flush)
      }
    },
    flush,
    dispose() {
      disposed = true
      pendingUpdates.clear()
      cancelFlush()
    },
  }
}

export function createRuntimeStreamEventBuffer({
  projectId,
  agentSessionId,
  runtimeKind,
  runId,
  sessionId,
  flowId,
  subscribedItemKinds,
  runtimeActionRefreshKeysRef,
  updateRuntimeStream,
  scheduleRuntimeMetadataRefresh,
  scheduleFlush = scheduleRuntimeStreamFlush,
}: RuntimeStreamEventBufferArgs): RuntimeStreamEventBuffer {
  const pendingEvents: RuntimeStreamChannelPayload[] = []
  let cancelScheduledFlush: ScheduledFlushCancel | null = null
  let disposed = false

  const cancelFlush = () => {
    if (!cancelScheduledFlush) {
      return
    }

    const cancel = cancelScheduledFlush
    cancelScheduledFlush = null
    cancel()
  }

  const flush = () => {
    if (disposed) {
      return
    }

    cancelFlush()
    if (pendingEvents.length === 0) {
      return
    }

    const events = pendingEvents.splice(0, pendingEvents.length)
    updateRuntimeStream(projectId, agentSessionId, (currentStream) => mergeRuntimeStreamEvents(currentStream, events))
    scheduleRuntimeActionRefreshes(
      projectId,
      events,
      runtimeActionRefreshKeysRef,
      scheduleRuntimeMetadataRefresh,
    )
  }

  const schedule = () => {
    if (cancelScheduledFlush) {
      return
    }

    cancelScheduledFlush = scheduleFlush(flush)
  }

  const reportIssue = (issue: { code: string; message: string; retryable: boolean }) => {
    flush()
    updateRuntimeStream(projectId, agentSessionId, (currentStream) =>
      applyRuntimeStreamIssue(currentStream, {
        projectId,
        agentSessionId,
        runtimeKind,
        runId,
        sessionId,
        flowId,
        subscribedItemKinds,
        code: issue.code,
        message: issue.message,
        retryable: issue.retryable,
      }),
    )
  }

  return {
    enqueue: (payload) => {
      if (disposed) {
        return
      }

      const payloadProjectId = isRuntimeStreamPatch(payload)
        ? payload.snapshot.projectId
        : payload.projectId
      if (payloadProjectId !== projectId) {
        reportIssue({
          code: 'runtime_stream_project_mismatch',
          message: `Xero received a runtime stream item for ${payloadProjectId} while ${projectId} is active.`,
          retryable: false,
        })
        return
      }

      if (isRuntimeStreamPatch(payload)) {
        pendingEvents.length = 0
      }
      pendingEvents.push(payload)
      if (isUrgentRuntimeStreamEvent(payload)) {
        flush()
        return
      }

      schedule()
    },
    reportIssue,
    flush,
    dispose: () => {
      disposed = true
      cancelFlush()
      pendingEvents.length = 0
    },
  }
}


export function clearRuntimeMetadataRefresh(
  refs: Pick<RuntimeMetadataRefreshRefs, 'pendingRuntimeRefreshRef' | 'runtimeRefreshTimeoutRef'>,
) {
  if (refs.runtimeRefreshTimeoutRef.current) {
    clearTimeout(refs.runtimeRefreshTimeoutRef.current)
    refs.runtimeRefreshTimeoutRef.current = null
  }

  refs.pendingRuntimeRefreshRef.current = null
}

export function scheduleRuntimeMetadataRefresh(args: {
  projectId: string
  source: RuntimeMetadataRefreshSource
  refs: RuntimeMetadataRefreshRefs
  loadProject: (projectId: string, source: ProjectLoadSource) => Promise<ProjectDetailView | null>
}) {
  const { projectId, source, refs, loadProject } = args
  if (refs.activeProjectIdRef.current !== projectId) {
    return
  }

  refs.pendingRuntimeRefreshRef.current = { projectId, source }
  if (refs.runtimeRefreshTimeoutRef.current) {
    return
  }

  refs.runtimeRefreshTimeoutRef.current = setTimeout(() => {
    refs.runtimeRefreshTimeoutRef.current = null
    const pendingRefresh = refs.pendingRuntimeRefreshRef.current
    refs.pendingRuntimeRefreshRef.current = null
    if (!pendingRefresh) {
      return
    }

    if (refs.activeProjectIdRef.current !== pendingRefresh.projectId) {
      return
    }

    void loadProject(pendingRefresh.projectId, pendingRefresh.source)
  }, 120)
}

export function clearBlockedNotificationSyncPoll(
  timeoutRef: MutableRefObject<ReturnType<typeof setTimeout> | null>,
) {
  if (timeoutRef.current) {
    clearTimeout(timeoutRef.current)
    timeoutRef.current = null
  }
}

export function scheduleBlockedNotificationSyncPoll(args: {
  expectedPollKey: string
  refs: BlockedNotificationSyncPollRefs
  loadProject: (projectId: string, source: ProjectLoadSource) => Promise<ProjectDetailView | null>
}) {
  const { expectedPollKey, refs, loadProject } = args
  if (refs.blockedNotificationSyncPollTimeoutRef.current) {
    return
  }

  refs.blockedNotificationSyncPollTimeoutRef.current = setTimeout(() => {
    refs.blockedNotificationSyncPollTimeoutRef.current = null

    const pollTarget = refs.blockedNotificationSyncPollTargetRef.current
    if (!pollTarget || getBlockedNotificationSyncPollKey(pollTarget) !== expectedPollKey) {
      return
    }

    if (refs.activeProjectIdRef.current !== pollTarget.projectId) {
      return
    }

    if (refs.blockedNotificationSyncPollInFlightRef.current) {
      scheduleBlockedNotificationSyncPoll(args)
      return
    }

    refs.blockedNotificationSyncPollInFlightRef.current = true
    void loadProject(pollTarget.projectId, 'runtime_stream:action_required').finally(() => {
      refs.blockedNotificationSyncPollInFlightRef.current = false
      const nextTarget = refs.blockedNotificationSyncPollTargetRef.current
      if (!nextTarget || getBlockedNotificationSyncPollKey(nextTarget) !== expectedPollKey) {
        return
      }

      if (refs.activeProjectIdRef.current !== nextTarget.projectId) {
        return
      }

      scheduleBlockedNotificationSyncPoll(args)
    })
  }, BLOCKED_NOTIFICATION_SYNC_POLL_MS)
}

export async function attachDesktopRuntimeListeners({
  adapter,
  refs,
  setters,
  handleAdapterEventError,
  applyRuntimeRunUpdate,
  loadProject,
  resetRepositoryDiffs,
}: AttachDesktopRuntimeListenersArgs): Promise<() => void> {
  let projectUnlisten: (() => void) | null = null
  let repositoryUnlisten: (() => void) | null = null
  let runtimeUnlisten: (() => void) | null = null
  let runtimeRunUnlisten: (() => void) | null = null
  const pendingRepositoryStatuses = new Map<string, RepositoryStatusView>()
  const pendingRepositoryStatusKeys = new Map<string, string>()
  let cancelRepositoryStatusFlush: ScheduledFlushCancel | null = null
  const runtimeRunUpdateBuffer = createRuntimeRunUpdateBuffer({
    activeProjectIdRef: refs.activeProjectIdRef,
    applyRuntimeRunUpdate,
    setRefreshSource: setters.setRefreshSource,
    setErrorMessage: setters.setErrorMessage,
  })
  let disposed = false

  const applyRepositoryStatusUpdate = (nextStatus: RepositoryStatusView) => {
    const nextStatusKey = createRepositoryStatusSyncKey(nextStatus)
    if (refs.repositoryStatusSyncKeyRef.current === nextStatusKey) {
      return
    }

    refs.repositoryStatusSyncKeyRef.current = nextStatusKey
    setters.setRefreshSource('repository:status_changed')
    setters.setProjects((currentProjects) => {
      const projectIndex = currentProjects.findIndex((project) => project.id === nextStatus.projectId)
      if (projectIndex < 0) {
        return currentProjects
      }

      const project = currentProjects[projectIndex]
      if (project.branch === nextStatus.branchLabel && project.branchLabel === nextStatus.branchLabel) {
        return currentProjects
      }

      const nextProjects = currentProjects.slice()
      nextProjects[projectIndex] = {
        ...project,
        branch: nextStatus.branchLabel,
        branchLabel: nextStatus.branchLabel,
      }
      return nextProjects
    })
    setters.setRepositoryStatus(nextStatus)
    setters.setActiveProject((currentProject) => {
      if (!currentProject) {
        return currentProject
      }

      const nextProject = applyRepositoryStatus(currentProject, nextStatus)
      const withRuntime = currentProject.runtimeSession
        ? applyRuntimeSession(nextProject, currentProject.runtimeSession)
        : nextProject
      return applyRuntimeRun(withRuntime, currentProject.runtimeRun ?? null)
    })
    resetRepositoryDiffs(nextStatus)
  }

  const flushRepositoryStatus = () => {
    if (disposed) {
      return
    }

    if (cancelRepositoryStatusFlush) {
      const cancel = cancelRepositoryStatusFlush
      cancelRepositoryStatusFlush = null
      cancel()
    }

    const activeProjectId = refs.activeProjectIdRef.current
    if (!activeProjectId) {
      pendingRepositoryStatuses.clear()
      pendingRepositoryStatusKeys.clear()
      return
    }

    const nextStatus = pendingRepositoryStatuses.get(activeProjectId) ?? null
    pendingRepositoryStatuses.clear()
    pendingRepositoryStatusKeys.clear()
    if (!nextStatus) {
      return
    }

    applyRepositoryStatusUpdate(nextStatus)
  }

  const scheduleRepositoryStatus = (nextStatus: RepositoryStatusView) => {
    const nextStatusKey = createRepositoryStatusSyncKey(nextStatus)
    if (refs.repositoryStatusSyncKeyRef.current === nextStatusKey) {
      pendingRepositoryStatuses.delete(nextStatus.projectId)
      pendingRepositoryStatusKeys.delete(nextStatus.projectId)
      return
    }

    if (pendingRepositoryStatusKeys.get(nextStatus.projectId) === nextStatusKey) {
      return
    }

    pendingRepositoryStatuses.set(nextStatus.projectId, nextStatus)
    pendingRepositoryStatusKeys.set(nextStatus.projectId, nextStatusKey)
    if (!cancelRepositoryStatusFlush) {
      cancelRepositoryStatusFlush = scheduleRepositoryStatusFlush(flushRepositoryStatus)
    }
  }

  projectUnlisten = await adapter.onProjectUpdated(
    (payload) => {
      if (disposed) {
        return
      }

      const summary = mapProjectSummary(payload.project)
      const cachedRuntime = refs.runtimeSessionsRef.current[summary.id] ?? null
      setters.setProjects((currentProjects) =>
        upsertProjectListItem(
          currentProjects,
          cachedRuntime ? applyRuntimeToProjectList(summary, cachedRuntime) : summary,
        ),
      )

      if (refs.activeProjectIdRef.current !== summary.id) {
        return
      }

      void loadProject(summary.id, 'project:updated')
    },
    handleAdapterEventError,
  )

  repositoryUnlisten = await adapter.onRepositoryStatusChanged(
    (payload) => {
      if (disposed || refs.activeProjectIdRef.current !== payload.projectId) {
        return
      }

      const nextStatus = mapRepositoryStatus(payload.status)
      scheduleRepositoryStatus(nextStatus)
    },
    handleAdapterEventError,
  )

  runtimeUnlisten = await adapter.onRuntimeUpdated(
    (payload) => {
      if (disposed) {
        return
      }

      const currentRuntime = refs.runtimeSessionsRef.current[payload.projectId] ?? null
      const nextRuntime = mergeRuntimeUpdated(currentRuntime, payload)

      setters.setRuntimeSessions((currentRuntimeSessions) => ({
        ...currentRuntimeSessions,
        [payload.projectId]: nextRuntime,
      }))
      setters.setRuntimeLoadErrors((currentErrors) => ({
        ...currentErrors,
        [payload.projectId]: null,
      }))
      setters.setProjects((currentProjects) => {
        const projectIndex = currentProjects.findIndex((project) => project.id === payload.projectId)
        if (projectIndex < 0) {
          return currentProjects
        }

        const project = currentProjects[projectIndex]
        if (project.runtime === nextRuntime.runtimeLabel && project.runtimeLabel === nextRuntime.runtimeLabel) {
          return currentProjects
        }

        const nextProjects = currentProjects.slice()
        nextProjects[projectIndex] = applyRuntimeToProjectList(project, nextRuntime)
        return nextProjects
      })

      if (!nextRuntime.isAuthenticated) {
        setters.setRuntimeStreams((currentStreams) => removeRuntimeStreamsForProject(currentStreams, payload.projectId))
      }

      if (refs.activeProjectIdRef.current !== payload.projectId) {
        return
      }

      setters.setRefreshSource('runtime:updated')
      setters.setErrorMessage(null)
      setters.setActiveProject((currentProject) =>
        currentProject ? applyRuntimeSession(currentProject, nextRuntime) : currentProject,
      )
    },
    handleAdapterEventError,
  )

  runtimeRunUnlisten = await adapter.onRuntimeRunUpdated(
    (payload) => {
      if (disposed) {
        return
      }

      runtimeRunUpdateBuffer.enqueue(payload)
    },
    handleAdapterEventError,
  )

  return () => {
    disposed = true
    pendingRepositoryStatuses.clear()
    pendingRepositoryStatusKeys.clear()
    cancelRepositoryStatusFlush?.()
    cancelRepositoryStatusFlush = null
    runtimeRunUpdateBuffer.dispose()
    projectUnlisten?.()
    repositoryUnlisten?.()
    runtimeUnlisten?.()
    runtimeRunUnlisten?.()
  }
}

export function attachRuntimeStreamSubscription({
  projectId,
  agentSessionId,
  runtimeSession,
  runId,
  adapter,
  runtimeActionRefreshKeysRef,
  updateRuntimeStream,
  scheduleRuntimeMetadataRefresh,
}: AttachRuntimeStreamSubscriptionArgs): () => void {
  if (!projectId || !agentSessionId) {
    return () => undefined
  }

  if (!runtimeSession?.isAuthenticated || !runtimeSession.sessionId) {
    updateRuntimeStream(projectId, agentSessionId, () => null)
    return () => undefined
  }

  if (!runId) {
    updateRuntimeStream(projectId, agentSessionId, () => null)
    return () => undefined
  }

  const seenActionKeys = runtimeActionRefreshKeysRef.current[projectId] ?? new Set<string>()
  runtimeActionRefreshKeysRef.current[projectId] = seenActionKeys
  for (const key of Array.from(seenActionKeys)) {
    if (!key.startsWith(`${agentSessionId}:${runId}:`)) {
      seenActionKeys.delete(key)
    }
  }

  let disposed = false
  let unsubscribe: () => void = () => {}
  let replayAfterSequence: number | null = null

  if (typeof adapter.subscribeRuntimeStream !== 'function') {
    updateRuntimeStream(projectId, agentSessionId, (currentStream) =>
      applyRuntimeStreamIssue(currentStream, {
        projectId,
        agentSessionId,
        runtimeKind: runtimeSession.runtimeKind,
        runId,
        sessionId: runtimeSession.sessionId,
        flowId: runtimeSession.flowId,
        subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
        code: 'runtime_stream_adapter_missing',
        message: 'Xero desktop adapter does not expose runtime stream subscriptions for this environment.',
        retryable: false,
      }),
    )

    return () => undefined
  }

  updateRuntimeStream(projectId, agentSessionId, (currentStream) => {
    if (currentStream?.runId === runId && currentStream.agentSessionId === agentSessionId) {
      replayAfterSequence = currentStream.lastSequence ?? null
      return {
        ...currentStream,
        agentSessionId,
        runtimeKind: runtimeSession.runtimeKind,
        sessionId: runtimeSession.sessionId,
        flowId: runtimeSession.flowId,
        subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
        status: currentStream.items.length > 0 ? 'replaying' : 'subscribing',
      }
    }

    replayAfterSequence = null
    return createRuntimeStreamView({
      projectId,
      agentSessionId,
      runtimeKind: runtimeSession.runtimeKind,
      runId,
      sessionId: runtimeSession.sessionId,
      flowId: runtimeSession.flowId,
      subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
      status: 'subscribing',
    })
  })

  const streamEventBuffer = createRuntimeStreamEventBuffer({
    projectId,
    agentSessionId,
    runtimeKind: runtimeSession.runtimeKind,
    runId,
    sessionId: runtimeSession.sessionId,
    flowId: runtimeSession.flowId,
    subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
    runtimeActionRefreshKeysRef,
    updateRuntimeStream,
    scheduleRuntimeMetadataRefresh,
  })

  void adapter
    .subscribeRuntimeStream(
      projectId,
      agentSessionId,
      ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
      (payload) => {
        if (disposed) {
          return
        }

        streamEventBuffer?.enqueue(payload)
      },
      (error) => {
        if (disposed) {
          return
        }

        streamEventBuffer?.reportIssue({
          code: error.code,
          message: error.message,
          retryable: error.retryable,
        })
      },
      {
        afterSequence: replayAfterSequence,
        replayLimit:
          replayAfterSequence == null
            ? null
            : INCREMENTAL_RUNTIME_STREAM_REPLAY_LIMIT,
      },
    )
    .then((subscription) => {
      if (disposed) {
        subscription.unsubscribe()
        return
      }

      unsubscribe = subscription.unsubscribe
      updateRuntimeStream(projectId, agentSessionId, (currentStream) => {
        if (
          currentStream?.runId === subscription.response.runId
          && currentStream.agentSessionId === subscription.response.agentSessionId
        ) {
          return {
            ...currentStream,
            runtimeKind: subscription.response.runtimeKind,
            agentSessionId: subscription.response.agentSessionId,
            runId: subscription.response.runId,
            sessionId: subscription.response.sessionId,
            flowId: subscription.response.flowId ?? null,
            subscribedItemKinds: subscription.response.subscribedItemKinds,
          }
        }

        return createRuntimeStreamFromSubscription(subscription.response, 'subscribing')
      })
    })
    .catch((error) => {
      if (disposed) {
        return
      }

      const issue = getRuntimeStreamIssue(error, {
        code: 'runtime_stream_subscribe_failed',
        message: 'Xero could not subscribe to the selected project runtime stream.',
        retryable: true,
      })

      updateRuntimeStream(projectId, agentSessionId, (currentStream) =>
        applyRuntimeStreamIssue(currentStream, {
          projectId,
          agentSessionId,
          runtimeKind: runtimeSession.runtimeKind,
          runId,
          sessionId: runtimeSession.sessionId,
          flowId: runtimeSession.flowId,
          subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
          code: issue.code,
          message: issue.message,
          retryable: issue.retryable,
        }),
      )
    })

  return () => {
    disposed = true
    streamEventBuffer?.dispose()
    unsubscribe()
  }
}
