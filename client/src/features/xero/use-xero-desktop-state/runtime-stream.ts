import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
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
  type RuntimeRunView,
  type RuntimeSessionView,
} from '@/src/lib/xero-model/runtime'
import {
  applyRuntimeStreamIssue,
  createRuntimeStreamFromSubscription,
  createRuntimeStreamView,
  mergeRuntimeStreamEvent,
  type RuntimeStreamEventDto,
  type RuntimeStreamItemKindDto,
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
  runtimeRunRefreshKeyRef: MutableRefObject<Record<string, string>>
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
  scheduleRuntimeMetadataRefresh: (projectId: string, source: RuntimeMetadataRefreshSource) => void
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

export interface RuntimeStreamEventBuffer {
  enqueue: (payload: RuntimeStreamEventDto) => void
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

export function isUrgentRuntimeStreamEvent(event: RuntimeStreamEventDto): boolean {
  return (
    event.item.kind === 'action_required' ||
    event.item.kind === 'complete' ||
    event.item.kind === 'failure'
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
  events: RuntimeStreamEventDto[],
): RuntimeStreamView | null {
  let nextStream = currentStream
  let sequenceGapIssue: {
    event: RuntimeStreamEventDto
    expectedSequence: number
    observedSequence: number
  } | null = null

  for (const event of events) {
    const previousSequence = nextStream?.lastSequence ?? null
    if (
      previousSequence !== null &&
      event.item.sequence > previousSequence + 1 &&
      !sequenceGapIssue
    ) {
      sequenceGapIssue = {
        event,
        expectedSequence: previousSequence + 1,
        observedSequence: event.item.sequence,
      }
    }

    try {
      nextStream = mergeRuntimeStreamEvent(nextStream, event)
    } catch (error) {
      const issue = getRuntimeStreamIssue(error, {
        code: 'runtime_stream_contract_mismatch',
        message: 'Xero ignored a malformed runtime stream item to preserve the last truthful stream state.',
        retryable: false,
      })

      nextStream = applyRuntimeStreamEventIssue(nextStream, event, issue)
    }
  }

  if (sequenceGapIssue && nextStream && !nextStream.failure && !nextStream.completion) {
    nextStream = applyRuntimeStreamEventIssue(nextStream, sequenceGapIssue.event, {
      code: 'runtime_stream_sequence_gap',
      message: `Xero detected a runtime stream sequence gap for run ${sequenceGapIssue.event.runId}: expected ${sequenceGapIssue.expectedSequence}, received ${sequenceGapIssue.observedSequence}.`,
      retryable: true,
    })
  }

  return nextStream
}

function scheduleRuntimeActionRefreshes(
  projectId: string,
  events: RuntimeStreamEventDto[],
  runtimeActionRefreshKeysRef: MutableRefObject<Record<string, Set<string>>>,
  scheduleRuntimeMetadataRefresh: (projectId: string, source: RuntimeMetadataRefreshSource) => void,
) {
  for (const event of events) {
    if (event.item.kind !== 'action_required') {
      continue
    }

    const actionId = event.item.actionId?.trim()
    if (!actionId) {
      continue
    }

    const refreshKey = `${event.agentSessionId}:${event.runId}:${actionId}`
    const knownKeys = runtimeActionRefreshKeysRef.current[projectId] ?? new Set<string>()
    runtimeActionRefreshKeysRef.current[projectId] = knownKeys

    if (!knownKeys.has(refreshKey)) {
      knownKeys.add(refreshKey)
      scheduleRuntimeMetadataRefresh(projectId, 'runtime_stream:action_required')
    }
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
  const pendingEvents: RuntimeStreamEventDto[] = []
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

      if (payload.projectId !== projectId) {
        reportIssue({
          code: 'runtime_stream_project_mismatch',
          message: `Xero received a runtime stream item for ${payload.projectId} while ${projectId} is active.`,
          retryable: false,
        })
        return
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
  scheduleRuntimeMetadataRefresh,
}: AttachDesktopRuntimeListenersArgs): Promise<() => void> {
  let projectUnlisten: (() => void) | null = null
  let repositoryUnlisten: (() => void) | null = null
  let runtimeUnlisten: (() => void) | null = null
  let runtimeRunUnlisten: (() => void) | null = null
  const pendingRepositoryStatuses = new Map<string, RepositoryStatusView>()
  const pendingRepositoryStatusKeys = new Map<string, string>()
  let cancelRepositoryStatusFlush: ScheduledFlushCancel | null = null
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

      const nextRuntimeRun = payload.run ? mapRuntimeRun(payload.run) : null
      applyRuntimeRunUpdate(payload.projectId, nextRuntimeRun)

      if (refs.activeProjectIdRef.current !== payload.projectId) {
        return
      }

      const refreshKey = payload.run
        ? `${payload.run.runId}:${payload.run.lastCheckpointSequence}:${payload.run.updatedAt}:${payload.run.status}`
        : 'none'
      if (refs.runtimeRunRefreshKeyRef.current[payload.projectId] !== refreshKey) {
        refs.runtimeRunRefreshKeyRef.current[payload.projectId] = refreshKey
        scheduleRuntimeMetadataRefresh(payload.projectId, 'runtime_run:updated')
      }

      setters.setRefreshSource('runtime_run:updated')
      setters.setErrorMessage(null)
    },
    handleAdapterEventError,
  )

  return () => {
    disposed = true
    pendingRepositoryStatuses.clear()
    pendingRepositoryStatusKeys.clear()
    cancelRepositoryStatusFlush?.()
    cancelRepositoryStatusFlush = null
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
