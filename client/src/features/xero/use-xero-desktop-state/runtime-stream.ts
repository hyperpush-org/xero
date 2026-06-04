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
  type RuntimeStreamStatus,
  type RuntimeStreamView,
} from '@/src/lib/xero-model/runtime-stream'

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
export const RUNTIME_STREAM_SUBSCRIBE_TIMEOUT_MS = 15_000
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
const COMPLETION_NOTIFICATION_STREAM_ITEM_KINDS: RuntimeStreamItemKindDto[] = [
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
  recordRuntimeSessionCompletion?: RuntimeStreamEventBufferArgs['onRuntimeSessionCompleted']
  loadProject: (projectId: string, source: ProjectLoadSource) => Promise<ProjectDetailView | null>
  resetRepositoryDiffs: (status: RepositoryStatusView | null) => void
}

interface AttachRuntimeStreamSubscriptionArgs {
  projectId: string | null
  agentSessionId: string | null
  runtimeSession: RuntimeSessionView | null
  runId: string | null
  forceFullReplay?: boolean
  adapter: XeroDesktopAdapter
  runtimeActionRefreshKeysRef: MutableRefObject<Record<string, Set<string>>>
  updateRuntimeStream: UpdateRuntimeStream
  scheduleRuntimeMetadataRefresh: (projectId: string, source: RuntimeMetadataRefreshSource) => void
  recordRuntimeSessionCompletion?: RuntimeStreamEventBufferArgs['onRuntimeSessionCompleted']
  subscribeTimeoutMs?: number
}

interface AttachRuntimeCompletionNotificationSubscriptionArgs {
  projectId: string
  agentSessionId: string
  runtimeSession: RuntimeSessionView
  runId: string
  adapter: XeroDesktopAdapter
  recordRuntimeSessionCompletion?: RuntimeStreamEventBufferArgs['onRuntimeSessionCompleted']
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
  onRuntimeSessionCompleted?: (completion: {
    projectId: string
    agentSessionId: string
    runId: string
    completedAt: string
  }) => void
  scheduleFlush?: FlushScheduler
}

interface RuntimeRunUpdateBufferArgs {
  activeProjectIdRef: MutableRefObject<string | null>
  applyRuntimeRunUpdate: (
    projectId: string,
    runtimeRun: RuntimeRunView | null,
    options?: { clearGlobalError?: boolean; loadError?: string | null },
  ) => RuntimeRunView | null
  recordRuntimeSessionCompletion?: RuntimeStreamEventBufferArgs['onRuntimeSessionCompleted']
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
  enableCompletionNotifications: () => void
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

function getRuntimeStreamConnectedStatus(stream: RuntimeStreamView): RuntimeStreamStatus {
  if (stream.completion) {
    return 'complete'
  }

  if (stream.failure) {
    return stream.failure.retryable ? 'stale' : 'error'
  }

  return 'live'
}

function isRuntimeStreamPatch(payload: RuntimeStreamChannelPayload): payload is RuntimeStreamPatchDto {
  return 'schema' in payload && payload.schema === 'xero.runtime_stream_patch.v1'
}

function getRuntimeStreamPayloadItem(payload: RuntimeStreamChannelPayload) {
  return payload.item
}

function runtimeStreamPayloadUpdateSequence(payload: RuntimeStreamChannelPayload): number {
  const item = getRuntimeStreamPayloadItem(payload)
  return typeof item.updatedSequence === 'number'
    ? Math.max(item.sequence, item.updatedSequence)
    : item.sequence
}

function canAggregateRuntimeTranscriptDelta(
  previous: RuntimeStreamEventDto,
  next: RuntimeStreamEventDto,
): boolean {
  const previousItem = previous.item
  const nextItem = next.item
  return previous.projectId === next.projectId &&
    previous.agentSessionId === next.agentSessionId &&
    previous.runtimeKind === next.runtimeKind &&
    previous.runId === next.runId &&
    previous.sessionId === next.sessionId &&
    previous.flowId === next.flowId &&
    previous.subscribedItemKinds.join('\u0000') === next.subscribedItemKinds.join('\u0000') &&
    previousItem.kind === 'transcript' &&
    nextItem.kind === 'transcript' &&
    (previousItem.transcriptRole ?? 'assistant') === 'assistant' &&
    (nextItem.transcriptRole ?? 'assistant') === 'assistant' &&
    typeof previousItem.text === 'string' &&
    typeof nextItem.text === 'string' &&
    previousItem.text.length > 0 &&
    nextItem.text.length > 0 &&
    !previousItem.mediaAttachments?.length &&
    !nextItem.mediaAttachments?.length &&
    !previousItem.codeChangeGroupId &&
    !nextItem.codeChangeGroupId &&
    !previousItem.codeCommitId &&
    !nextItem.codeCommitId &&
    runtimeStreamPayloadUpdateSequence(previous) + 1 === nextItem.sequence
}

function aggregateRuntimeTranscriptDeltas(
  events: RuntimeStreamChannelPayload[],
): RuntimeStreamChannelPayload[] {
  if (events.length < 2) {
    return events
  }

  const aggregated: RuntimeStreamChannelPayload[] = []
  for (const event of events) {
    const previous = aggregated.at(-1)
    if (
      previous &&
      !isRuntimeStreamPatch(previous) &&
      !isRuntimeStreamPatch(event) &&
      canAggregateRuntimeTranscriptDelta(previous, event)
    ) {
      previous.item = {
        ...previous.item,
        text: `${previous.item.text ?? ''}${event.item.text ?? ''}`,
        updatedSequence: runtimeStreamPayloadUpdateSequence(event),
        createdAt: event.item.createdAt,
      }
      continue
    }

    aggregated.push(
      !isRuntimeStreamPatch(event) && event.item.kind === 'transcript'
        ? cloneRuntimeStreamEventForAggregation(event)
        : event,
    )
  }
  return aggregated
}

function cloneRuntimeStreamEventForAggregation(event: RuntimeStreamEventDto): RuntimeStreamEventDto {
  return {
    ...event,
    subscribedItemKinds: [...event.subscribedItemKinds],
    item: {
      ...event.item,
      mediaAttachments: event.item.mediaAttachments
        ? [...event.item.mediaAttachments]
        : event.item.mediaAttachments,
    },
  }
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

  for (const event of aggregateRuntimeTranscriptDeltas(events)) {
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

function notifyRuntimeStreamCompletions(
  events: RuntimeStreamChannelPayload[],
  onRuntimeSessionCompleted?: RuntimeStreamEventBufferArgs['onRuntimeSessionCompleted'],
) {
  if (!onRuntimeSessionCompleted) {
    return
  }

  const notifiedRunIds = new Set<string>()
  for (const event of events) {
    const item = getRuntimeStreamPayloadItem(event)
    const completionItem = isRuntimeStreamPatch(event)
      ? item.kind === 'complete'
        ? item
        : event.snapshot.completion
      : item.kind === 'complete'
        ? item
        : null
    if (!completionItem) {
      continue
    }

    const projectId = isRuntimeStreamPatch(event) ? event.snapshot.projectId : event.projectId
    const agentSessionId = isRuntimeStreamPatch(event) ? event.snapshot.agentSessionId : event.agentSessionId
    const runId = completionItem.runId?.trim()
    const completedAt = completionItem.createdAt?.trim()
    if (!projectId || !agentSessionId || !runId || !completedAt || notifiedRunIds.has(runId)) {
      continue
    }

    notifiedRunIds.add(runId)
    onRuntimeSessionCompleted({
      projectId,
      agentSessionId,
      runId,
      completedAt,
    })
  }
}

export function createRuntimeRunUpdateBuffer({
  activeProjectIdRef,
  applyRuntimeRunUpdate,
  recordRuntimeSessionCompletion,
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
        if (update.runtimeRun?.status === 'stopped') {
          recordRuntimeSessionCompletion?.({
            projectId: update.projectId,
            agentSessionId: update.agentSessionId,
            runId: update.runtimeRun.runId,
            completedAt: update.runtimeRun.stoppedAt ?? update.runtimeRun.updatedAt,
          })
        }
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
  onRuntimeSessionCompleted,
  scheduleFlush = scheduleRuntimeStreamFlush,
}: RuntimeStreamEventBufferArgs): RuntimeStreamEventBuffer {
  const pendingEvents: RuntimeStreamChannelPayload[] = []
  let cancelScheduledFlush: ScheduledFlushCancel | null = null
  let disposed = false
  let completionNotificationsEnabled = false

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
    if (completionNotificationsEnabled) {
      notifyRuntimeStreamCompletions(events, onRuntimeSessionCompleted)
    }
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
    enableCompletionNotifications: () => {
      completionNotificationsEnabled = true
    },
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

export async function attachDesktopRuntimeListeners({
  adapter,
  refs,
  setters,
  handleAdapterEventError,
  applyRuntimeRunUpdate,
  recordRuntimeSessionCompletion,
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
    recordRuntimeSessionCompletion,
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

export function attachRuntimeCompletionNotificationSubscription({
  projectId,
  agentSessionId,
  runtimeSession,
  runId,
  adapter,
  recordRuntimeSessionCompletion,
}: AttachRuntimeCompletionNotificationSubscriptionArgs): () => void {
  if (
    !recordRuntimeSessionCompletion ||
    !runtimeSession.isAuthenticated ||
    !runtimeSession.sessionId ||
    typeof adapter.subscribeRuntimeStream !== 'function'
  ) {
    return () => undefined
  }

  let disposed = false
  let unsubscribe: () => void = () => {}

  const dispose = () => {
    if (disposed) {
      return
    }

    disposed = true
    unsubscribe()
  }

  const handleTerminalPayload = (payload: RuntimeStreamChannelPayload) => {
    if (disposed) {
      return
    }

    const item = getRuntimeStreamPayloadItem(payload)
    const payloadProjectId = isRuntimeStreamPatch(payload)
      ? payload.snapshot.projectId
      : payload.projectId
    const payloadAgentSessionId = isRuntimeStreamPatch(payload)
      ? payload.snapshot.agentSessionId
      : payload.agentSessionId
    const payloadRunId = isRuntimeStreamPatch(payload) ? payload.snapshot.runId : payload.runId
    if (
      payloadProjectId !== projectId ||
      payloadAgentSessionId !== agentSessionId ||
      payloadRunId !== runId
    ) {
      return
    }

    notifyRuntimeStreamCompletions([payload], recordRuntimeSessionCompletion)
    if (
      item.kind === 'complete' ||
      item.kind === 'failure' ||
      (isRuntimeStreamPatch(payload) && (payload.snapshot.completion || payload.snapshot.failure))
    ) {
      dispose()
    }
  }

  void adapter
    .subscribeRuntimeStream(
      projectId,
      agentSessionId,
      COMPLETION_NOTIFICATION_STREAM_ITEM_KINDS,
      handleTerminalPayload,
      () => {
        dispose()
      },
      {
        afterSequence: null,
        replayLimit: null,
      },
    )
    .then((subscription) => {
      if (disposed) {
        subscription.unsubscribe()
        return
      }

      unsubscribe = subscription.unsubscribe
    })
    .catch(() => {
      disposed = true
    })

  return dispose
}

export function attachRuntimeStreamSubscription({
  projectId,
  agentSessionId,
  runtimeSession,
  runId,
  forceFullReplay = false,
  adapter,
  runtimeActionRefreshKeysRef,
  updateRuntimeStream,
  scheduleRuntimeMetadataRefresh,
  recordRuntimeSessionCompletion,
  subscribeTimeoutMs = RUNTIME_STREAM_SUBSCRIBE_TIMEOUT_MS,
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
  let subscriptionSettled = false
  let subscribeTimeoutId: ReturnType<typeof setTimeout> | null = null

  const clearSubscribeTimeout = () => {
    if (subscribeTimeoutId === null) {
      return
    }

    clearTimeout(subscribeTimeoutId)
    subscribeTimeoutId = null
  }

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
    if (
      !forceFullReplay &&
      currentStream?.runId === runId &&
      currentStream.agentSessionId === agentSessionId
    ) {
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
    onRuntimeSessionCompleted: recordRuntimeSessionCompletion,
  })

  subscribeTimeoutId = setTimeout(() => {
    if (disposed || subscriptionSettled) {
      return
    }

    streamEventBuffer.reportIssue({
      code: 'runtime_stream_subscribe_timeout',
      message: 'Xero could not connect the live runtime stream in time. Retry the stream to reconnect.',
      retryable: true,
    })
    scheduleRuntimeMetadataRefresh(projectId, 'runtime_run:updated')
  }, subscribeTimeoutMs)

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
      subscriptionSettled = true
      clearSubscribeTimeout()

      if (disposed) {
        subscription.unsubscribe()
        return
      }

      streamEventBuffer.enableCompletionNotifications()
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
            status: getRuntimeStreamConnectedStatus(currentStream),
            lastIssue: currentStream.failure ? currentStream.lastIssue : null,
          }
        }

        return createRuntimeStreamFromSubscription(subscription.response, 'live')
      })
    })
    .catch((error) => {
      subscriptionSettled = true
      clearSubscribeTimeout()

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
    clearSubscribeTimeout()
    streamEventBuffer?.dispose()
    unsubscribe()
  }
}

export function shouldForceFullRuntimeStreamReplay(
  previousProjectId: string | null,
  activeProjectId: string | null,
): boolean {
  return Boolean(activeProjectId && previousProjectId !== activeProjectId)
}
