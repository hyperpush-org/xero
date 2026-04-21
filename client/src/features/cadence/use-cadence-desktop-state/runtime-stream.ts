import type { Dispatch, MutableRefObject, SetStateAction } from 'react'
import { CadenceDesktopError, type CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import {
  applyRepositoryStatus,
  applyRuntimeRun,
  applyRuntimeSession,
  applyRuntimeStreamIssue,
  createRuntimeStreamFromSubscription,
  createRuntimeStreamView,
  mapProjectSummary,
  mapRepositoryStatus,
  mapRuntimeRun,
  mergeRuntimeStreamEvent,
  mergeRuntimeUpdated,
  upsertProjectListItem,
  type ProjectDetailView,
  type ProjectListItem,
  type RepositoryStatusView,
  type RuntimeRunView,
  type RuntimeSessionView,
  type RuntimeStreamItemKindDto,
  type RuntimeStreamView,
} from '@/src/lib/cadence-model'

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
import type { RefreshSource } from './types'

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

type UpdateRuntimeStream = (projectId: string, updater: RuntimeStreamUpdater) => void

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
  adapter: CadenceDesktopAdapter
  refs: AttachDesktopRuntimeListenersRefs
  setters: AttachDesktopRuntimeListenersSetters
  handleAdapterEventError: (error: CadenceDesktopError) => void
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
  runtimeSession: RuntimeSessionView | null
  runId: string | null
  adapter: CadenceDesktopAdapter
  runtimeActionRefreshKeysRef: MutableRefObject<Record<string, Set<string>>>
  updateRuntimeStream: UpdateRuntimeStream
  scheduleRuntimeMetadataRefresh: (projectId: string, source: RuntimeMetadataRefreshSource) => void
}

function getRuntimeStreamIssue(
  error: unknown,
  fallback: { code: string; message: string; retryable: boolean },
) {
  if (error instanceof CadenceDesktopError) {
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
  let disposed = false

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
      setters.setRefreshSource('repository:status_changed')
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
      setters.setProjects((currentProjects) =>
        currentProjects.map((project) =>
          project.id === payload.projectId ? applyRuntimeToProjectList(project, nextRuntime) : project,
        ),
      )

      if (!nextRuntime.isAuthenticated) {
        setters.setRuntimeStreams((currentStreams) => removeProjectRecord(currentStreams, payload.projectId))
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
    projectUnlisten?.()
    repositoryUnlisten?.()
    runtimeUnlisten?.()
    runtimeRunUnlisten?.()
  }
}

export function attachRuntimeStreamSubscription({
  projectId,
  runtimeSession,
  runId,
  adapter,
  runtimeActionRefreshKeysRef,
  updateRuntimeStream,
  scheduleRuntimeMetadataRefresh,
}: AttachRuntimeStreamSubscriptionArgs): () => void {
  if (!projectId) {
    return () => undefined
  }

  if (!runtimeSession?.isAuthenticated || !runtimeSession.sessionId) {
    updateRuntimeStream(projectId, () => null)
    return () => undefined
  }

  if (!runId) {
    updateRuntimeStream(projectId, () => null)
    return () => undefined
  }

  const seenActionKeys = runtimeActionRefreshKeysRef.current[projectId] ?? new Set<string>()
  runtimeActionRefreshKeysRef.current[projectId] = seenActionKeys
  for (const key of Array.from(seenActionKeys)) {
    if (!key.startsWith(`${runId}:`)) {
      seenActionKeys.delete(key)
    }
  }

  let disposed = false
  let unsubscribe: () => void = () => {}

  if (typeof adapter.subscribeRuntimeStream !== 'function') {
    updateRuntimeStream(projectId, (currentStream) =>
      applyRuntimeStreamIssue(currentStream, {
        projectId,
        runtimeKind: runtimeSession.runtimeKind,
        runId,
        sessionId: runtimeSession.sessionId,
        flowId: runtimeSession.flowId,
        subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
        code: 'runtime_stream_adapter_missing',
        message: 'Cadence desktop adapter does not expose runtime stream subscriptions for this environment.',
        retryable: false,
      }),
    )

    return () => undefined
  }

  updateRuntimeStream(projectId, (currentStream) => {
    if (currentStream?.runId === runId) {
      return {
        ...currentStream,
        runtimeKind: runtimeSession.runtimeKind,
        sessionId: runtimeSession.sessionId,
        flowId: runtimeSession.flowId,
        subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
        status: currentStream.items.length > 0 ? 'replaying' : 'subscribing',
      }
    }

    return createRuntimeStreamView({
      projectId,
      runtimeKind: runtimeSession.runtimeKind,
      runId,
      sessionId: runtimeSession.sessionId,
      flowId: runtimeSession.flowId,
      subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
      status: 'subscribing',
    })
  })

  void adapter
    .subscribeRuntimeStream(
      projectId,
      ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
      (payload) => {
        if (disposed) {
          return
        }

        if (payload.projectId !== projectId) {
          updateRuntimeStream(projectId, (currentStream) =>
            applyRuntimeStreamIssue(currentStream, {
              projectId,
              runtimeKind: runtimeSession.runtimeKind,
              runId,
              sessionId: runtimeSession.sessionId,
              flowId: runtimeSession.flowId,
              subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
              code: 'runtime_stream_project_mismatch',
              message: `Cadence received a runtime stream item for ${payload.projectId} while ${projectId} is active.`,
              retryable: false,
            }),
          )
          return
        }

        updateRuntimeStream(projectId, (currentStream) => {
          try {
            return mergeRuntimeStreamEvent(currentStream, payload)
          } catch (error) {
            const issue = getRuntimeStreamIssue(error, {
              code: 'runtime_stream_contract_mismatch',
              message: 'Cadence ignored a malformed runtime stream item to preserve the last truthful stream state.',
              retryable: false,
            })

            return applyRuntimeStreamIssue(currentStream, {
              projectId,
              runtimeKind: payload.runtimeKind,
              runId: payload.runId,
              sessionId: payload.sessionId,
              flowId: payload.flowId,
              subscribedItemKinds: payload.subscribedItemKinds,
              code: issue.code,
              message: issue.message,
              retryable: issue.retryable,
            })
          }
        })

        if (payload.item.kind === 'action_required') {
          const actionId = payload.item.actionId?.trim()
          if (actionId) {
            const refreshKey = `${payload.runId}:${actionId}`
            const knownKeys = runtimeActionRefreshKeysRef.current[projectId] ?? new Set<string>()
            runtimeActionRefreshKeysRef.current[projectId] = knownKeys

            if (!knownKeys.has(refreshKey)) {
              knownKeys.add(refreshKey)
              scheduleRuntimeMetadataRefresh(projectId, 'runtime_stream:action_required')
            }
          }
        }
      },
      (error) => {
        if (disposed) {
          return
        }

        updateRuntimeStream(projectId, (currentStream) =>
          applyRuntimeStreamIssue(currentStream, {
            projectId,
            runtimeKind: runtimeSession.runtimeKind,
            runId,
            sessionId: runtimeSession.sessionId,
            flowId: runtimeSession.flowId,
            subscribedItemKinds: ACTIVE_RUNTIME_STREAM_ITEM_KINDS,
            code: error.code,
            message: error.message,
            retryable: error.retryable,
          }),
        )
      },
    )
    .then((subscription) => {
      if (disposed) {
        subscription.unsubscribe()
        return
      }

      unsubscribe = subscription.unsubscribe
      updateRuntimeStream(projectId, (currentStream) => {
        if (currentStream?.runId === subscription.response.runId) {
          return {
            ...currentStream,
            runtimeKind: subscription.response.runtimeKind,
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
        message: 'Cadence could not subscribe to the selected project runtime stream.',
        retryable: true,
      })

      updateRuntimeStream(projectId, (currentStream) =>
        applyRuntimeStreamIssue(currentStream, {
          projectId,
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
    unsubscribe()
  }
}
