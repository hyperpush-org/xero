import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  CadenceDesktopError,
  CadenceDesktopAdapter,
  getDesktopErrorMessage,
} from '@/src/lib/cadence-desktop'
import {
  projectCheckpointControlLoops,
  projectRecentAutonomousUnits,
  type CheckpointControlLoopProjectionView,
  type RecentAutonomousUnitsProjectionView,
} from './agent-runtime-projections'
import {
  applyRepositoryStatus,
  applyRuntimeRun,
  applyRuntimeSession,
  mapAutonomousRunInspection,
  applyRuntimeStreamIssue,
  createEmptyPlanningLifecycle,
  createRuntimeStreamFromSubscription,
  createRuntimeStreamView,
  deriveAutonomousWorkflowContext,
  getRuntimeStreamStatusLabel,
  mapProjectSnapshot,
  mapProjectSummary,
  mapRepositoryDiff,
  mapRepositoryStatus,
  mapRuntimeRun,
  mapRuntimeSession,
  mergeRuntimeStreamEvent,
  mergeRuntimeUpdated,
  upsertProjectListItem,
  notificationRouteCredentialReadinessSchema,
  type AutonomousUnitAttemptView,
  type AutonomousUnitArtifactView,
  type AutonomousUnitHistoryEntryView,
  type AutonomousWorkflowContextView,
  type CreateProjectEntryRequestDto,
  type CreateProjectEntryResponseDto,
  type DeleteProjectEntryResponseDto,
  type ListProjectFilesResponseDto,
  type NotificationDispatchDto,
  type NotificationDispatchView,
  type NotificationRouteCredentialReadinessDto,
  type NotificationRouteDto,
  type NotificationRouteKindDto,
  type OperatorApprovalView,
  type Phase,
  type PlanningLifecycleStageView,
  type PlanningLifecycleView,
  type ProjectDetailView,
  type ProjectListItem,
  type ReadProjectFileResponseDto,
  type RenameProjectEntryRequestDto,
  type RenameProjectEntryResponseDto,
  type RepositoryDiffScope,
  type RepositoryDiffView,
  type RepositoryStatusEntryView,
  type RepositoryStatusView,
  type ResumeHistoryEntryView,
  type RuntimeAuthPhaseDto,
  type RuntimeRunView,
  type RuntimeSessionView,
  type RuntimeSettingsDto,
  type RuntimeStreamActionRequiredItemView,
  type RuntimeStreamActivityItemView,
  type RuntimeStreamIssueView,
  type RuntimeStreamSkillItemView,
  type RuntimeStreamItemKindDto,
  type RuntimeStreamStatus,
  type RuntimeStreamView,
  type RuntimeStreamViewItem,
  type SyncNotificationAdaptersResponseDto,
  type UpsertNotificationRouteRequestDto,
  type UpsertRuntimeSettingsRequestDto,
  type VerificationRecordView,
  type WriteProjectFileResponseDto,
} from '@/src/lib/cadence-model'

import {
  BLOCKED_NOTIFICATION_SYNC_POLL_MS,
  composeAgentTrustSnapshot,
  createUnavailableTrustSnapshot,
  getBlockedNotificationSyncPollKey,
  getBlockedNotificationSyncPollTarget,
  mapNotificationChannelHealth,
  mapNotificationRouteViews,
} from './use-cadence-desktop-state/notification-health'
import {
  getAgentMessagesUnavailableReason,
  getAgentRuntimeRunUnavailableReason,
  getAgentSessionUnavailableReason,
  hasProviderMismatch,
  resolveSelectedRuntimeProvider,
} from './use-cadence-desktop-state/runtime-provider'
import type {
  AgentPaneView,
  AgentTrustSnapshotView,
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  DiffScopeSummary,
  ExecutionPaneView,
  NotificationRouteMutationStatus,
  NotificationRoutesLoadResult,
  NotificationRoutesLoadStatus,
  OperatorActionDecision,
  OperatorActionErrorView,
  OperatorActionStatus,
  ProjectRemovalStatus,
  RefreshSource,
  RepositoryDiffState,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
  RuntimeSettingsLoadStatus,
  RuntimeSettingsSaveStatus,
  UseCadenceDesktopStateOptions,
  UseCadenceDesktopStateResult,
  WorkflowPaneView,
} from './use-cadence-desktop-state/types'

export type {
  AgentPaneView,
  AgentTrustSignalState,
  AgentTrustSnapshotView,
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  DiffScopeSummary,
  ExecutionPaneView,
  NotificationChannelHealthView,
  NotificationRouteHealthState,
  NotificationRouteHealthView,
  NotificationRouteMutationStatus,
  NotificationRoutesLoadStatus,
  OperatorActionDecision,
  OperatorActionErrorView,
  OperatorActionStatus,
  ProjectRemovalStatus,
  RefreshSource,
  RepositoryDiffLoadStatus,
  RepositoryDiffState,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
  RuntimeSettingsLoadStatus,
  RuntimeSettingsSaveStatus,
  UseCadenceDesktopStateOptions,
  UseCadenceDesktopStateResult,
  WorkflowPaneView,
} from './use-cadence-desktop-state/types'
export { BLOCKED_NOTIFICATION_SYNC_POLL_MS } from './use-cadence-desktop-state/notification-health'

const REPOSITORY_DIFF_SCOPE_LABELS: Record<RepositoryDiffScope, string> = {
  staged: 'Staged',
  unstaged: 'Unstaged',
  worktree: 'Worktree',
}

const ACTIVE_RUNTIME_STREAM_ITEM_KINDS: RuntimeStreamItemKindDto[] = [
  'transcript',
  'tool',
  'skill',
  'activity',
  'action_required',
  'complete',
  'failure',
]

function createEmptyRepositoryDiffState(): RepositoryDiffState {
  return {
    status: 'idle',
    diff: null,
    errorMessage: null,
    projectId: null,
  }
}

function createInitialRepositoryDiffs(): Record<RepositoryDiffScope, RepositoryDiffState> {
  return {
    staged: createEmptyRepositoryDiffState(),
    unstaged: createEmptyRepositoryDiffState(),
    worktree: createEmptyRepositoryDiffState(),
  }
}

function getDefaultDiffScope(status: RepositoryStatusView | null): RepositoryDiffScope {
  if (!status) {
    return 'unstaged'
  }

  if (status.unstagedCount > 0) {
    return 'unstaged'
  }

  if (status.stagedCount > 0) {
    return 'staged'
  }

  return 'worktree'
}

function getActivePhase(project: ProjectDetailView | null): Phase | null {
  if (!project) {
    return null
  }

  return (
    project.phases.find((phase) => phase.status === 'active') ??
    project.phases.find((phase) => phase.id === project.activePhase) ??
    project.phases[0] ??
    null
  )
}

function getPlanningLifecycleView(project: ProjectDetailView | null): PlanningLifecycleView {
  return project?.lifecycle ?? createEmptyPlanningLifecycle()
}

function applyRuntimeToProjectList(project: ProjectListItem, runtimeSession: RuntimeSessionView): ProjectListItem {
  return {
    ...project,
    runtime: runtimeSession.runtimeLabel,
    runtimeLabel: runtimeSession.runtimeLabel,
  }
}

function applyAutonomousRunState(
  project: ProjectDetailView,
  autonomousRun: ProjectDetailView['autonomousRun'],
  autonomousUnit: ProjectDetailView['autonomousUnit'],
  autonomousAttempt: ProjectDetailView['autonomousAttempt'],
  autonomousHistory: ProjectDetailView['autonomousHistory'],
  autonomousRecentArtifacts: ProjectDetailView['autonomousRecentArtifacts'],
): ProjectDetailView {
  return {
    ...project,
    autonomousRun: autonomousRun ?? null,
    autonomousUnit: autonomousUnit ?? null,
    autonomousAttempt: autonomousAttempt ?? null,
    autonomousHistory,
    autonomousRecentArtifacts,
  }
}

function removeProjectRecord<T>(records: Record<string, T>, projectId: string): Record<string, T> {
  if (!(projectId in records)) {
    return records
  }

  const nextRecords = { ...records }
  delete nextRecords[projectId]
  return nextRecords
}

function getRuntimeStreamIssue(error: unknown, fallback: { code: string; message: string; retryable: boolean }) {
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

function getOperatorActionError(error: unknown, fallback: string): OperatorActionErrorView {
  if (error instanceof CadenceDesktopError) {
    return {
      code: error.code,
      message: error.message,
      retryable: error.retryable,
    }
  }

  if (error instanceof Error && error.message.trim().length > 0) {
    return {
      code: 'operator_action_failed',
      message: error.message,
      retryable: false,
    }
  }

  return {
    code: 'operator_action_failed',
    message: fallback,
    retryable: false,
  }
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

export function useCadenceDesktopState(
  options: UseCadenceDesktopStateOptions = {},
): UseCadenceDesktopStateResult {
  const adapter = options.adapter ?? CadenceDesktopAdapter
  const [projects, setProjects] = useState<ProjectListItem[]>([])
  const [activeProject, setActiveProject] = useState<ProjectDetailView | null>(null)
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null)
  const [repositoryStatus, setRepositoryStatus] = useState<RepositoryStatusView | null>(null)
  const [repositoryDiffs, setRepositoryDiffs] = useState<Record<RepositoryDiffScope, RepositoryDiffState>>(
    createInitialRepositoryDiffs,
  )
  const [runtimeSessions, setRuntimeSessions] = useState<Record<string, RuntimeSessionView>>({})
  const [runtimeRuns, setRuntimeRuns] = useState<Record<string, RuntimeRunView>>({})
  const [autonomousRuns, setAutonomousRuns] = useState<Record<string, NonNullable<ProjectDetailView['autonomousRun']>>>({})
  const [autonomousUnits, setAutonomousUnits] = useState<Record<string, NonNullable<ProjectDetailView['autonomousUnit']>>>({})
  const [autonomousAttempts, setAutonomousAttempts] = useState<Record<string, NonNullable<ProjectDetailView['autonomousAttempt']>>>({})
  const [autonomousHistories, setAutonomousHistories] = useState<Record<string, AutonomousUnitHistoryEntryView[]>>({})
  const [autonomousRecentArtifacts, setAutonomousRecentArtifacts] = useState<Record<string, AutonomousUnitArtifactView[]>>({})
  const [notificationRoutes, setNotificationRoutes] = useState<Record<string, NotificationRouteDto[]>>({})
  const [notificationRouteLoadStatuses, setNotificationRouteLoadStatuses] = useState<
    Record<string, NotificationRoutesLoadStatus>
  >({})
  const [notificationRouteLoadErrors, setNotificationRouteLoadErrors] = useState<
    Record<string, OperatorActionErrorView | null>
  >({})
  const [notificationSyncSummaries, setNotificationSyncSummaries] = useState<
    Record<string, SyncNotificationAdaptersResponseDto | null>
  >({})
  const [notificationSyncErrors, setNotificationSyncErrors] = useState<
    Record<string, OperatorActionErrorView | null>
  >({})
  const [runtimeStreams, setRuntimeStreams] = useState<Record<string, RuntimeStreamView>>({})
  const [runtimeLoadErrors, setRuntimeLoadErrors] = useState<Record<string, string | null>>({})
  const [runtimeRunLoadErrors, setRuntimeRunLoadErrors] = useState<Record<string, string | null>>({})
  const [autonomousRunLoadErrors, setAutonomousRunLoadErrors] = useState<Record<string, string | null>>({})
  const [activeDiffScope, setActiveDiffScope] = useState<RepositoryDiffScope>('unstaged')
  const [isLoading, setIsLoading] = useState(true)
  const [isProjectLoading, setIsProjectLoading] = useState(false)
  const [isImporting, setIsImporting] = useState(false)
  const [projectRemovalStatus, setProjectRemovalStatus] = useState<ProjectRemovalStatus>('idle')
  const [pendingProjectRemovalId, setPendingProjectRemovalId] = useState<string | null>(null)
  const [operatorActionStatus, setOperatorActionStatus] = useState<OperatorActionStatus>('idle')
  const [pendingOperatorActionId, setPendingOperatorActionId] = useState<string | null>(null)
  const [operatorActionError, setOperatorActionError] = useState<OperatorActionErrorView | null>(null)
  const [autonomousRunActionStatus, setAutonomousRunActionStatus] = useState<AutonomousRunActionStatus>('idle')
  const [pendingAutonomousRunAction, setPendingAutonomousRunAction] = useState<AutonomousRunActionKind | null>(null)
  const [autonomousRunActionError, setAutonomousRunActionError] = useState<OperatorActionErrorView | null>(null)
  const [runtimeRunActionStatus, setRuntimeRunActionStatus] = useState<RuntimeRunActionStatus>('idle')
  const [pendingRuntimeRunAction, setPendingRuntimeRunAction] = useState<RuntimeRunActionKind | null>(null)
  const [runtimeRunActionError, setRuntimeRunActionError] = useState<OperatorActionErrorView | null>(null)
  const [notificationRouteMutationStatus, setNotificationRouteMutationStatus] =
    useState<NotificationRouteMutationStatus>('idle')
  const [pendingNotificationRouteId, setPendingNotificationRouteId] = useState<string | null>(null)
  const [notificationRouteMutationError, setNotificationRouteMutationError] =
    useState<OperatorActionErrorView | null>(null)
  const [runtimeSettings, setRuntimeSettings] = useState<RuntimeSettingsDto | null>(null)
  const [runtimeSettingsLoadStatus, setRuntimeSettingsLoadStatus] = useState<RuntimeSettingsLoadStatus>('idle')
  const [runtimeSettingsLoadError, setRuntimeSettingsLoadError] = useState<OperatorActionErrorView | null>(null)
  const [runtimeSettingsSaveStatus, setRuntimeSettingsSaveStatus] = useState<RuntimeSettingsSaveStatus>('idle')
  const [runtimeSettingsSaveError, setRuntimeSettingsSaveError] = useState<OperatorActionErrorView | null>(null)
  const [errorMessage, setErrorMessage] = useState<string | null>(null)
  const [refreshSource, setRefreshSource] = useState<RefreshSource>(null)
  const [runtimeStreamRetryToken, setRuntimeStreamRetryToken] = useState(0)
  const activeProjectRef = useRef<ProjectDetailView | null>(null)
  const activeProjectIdRef = useRef<string | null>(null)
  const latestLoadRequestRef = useRef(0)
  const latestDiffRequestRef = useRef<Record<RepositoryDiffScope, number>>({
    staged: 0,
    unstaged: 0,
    worktree: 0,
  })
  const repositoryDiffsRef = useRef<Record<RepositoryDiffScope, RepositoryDiffState>>(createInitialRepositoryDiffs())
  const runtimeSessionsRef = useRef<Record<string, RuntimeSessionView>>({})
  const runtimeRunsRef = useRef<Record<string, RuntimeRunView>>({})
  const autonomousRunsRef = useRef<Record<string, NonNullable<ProjectDetailView['autonomousRun']>>>({})
  const autonomousUnitsRef = useRef<Record<string, NonNullable<ProjectDetailView['autonomousUnit']>>>({})
  const autonomousAttemptsRef = useRef<Record<string, NonNullable<ProjectDetailView['autonomousAttempt']>>>({})
  const autonomousHistoriesRef = useRef<Record<string, AutonomousUnitHistoryEntryView[]>>({})
  const autonomousRecentArtifactsRef = useRef<Record<string, AutonomousUnitArtifactView[]>>({})
  const notificationRoutesRef = useRef<Record<string, NotificationRouteDto[]>>({})
  const notificationRouteLoadStatusesRef = useRef<Record<string, NotificationRoutesLoadStatus>>({})
  const notificationRouteLoadErrorsRef = useRef<Record<string, OperatorActionErrorView | null>>({})
  const notificationRouteLoadRequestRef = useRef<Record<string, number>>({})
  const notificationRouteLoadInFlightRef = useRef<Record<string, Promise<NotificationRoutesLoadResult>>>({})
  const notificationSyncSummariesRef = useRef<Record<string, SyncNotificationAdaptersResponseDto | null>>({})
  const notificationDispatchesRef = useRef<Record<string, NotificationDispatchDto[]>>({})
  const trustSnapshotRef = useRef<Record<string, AgentTrustSnapshotView>>({})
  const runtimeSettingsRef = useRef<RuntimeSettingsDto | null>(null)
  const runtimeSettingsLoadInFlightRef = useRef<Promise<RuntimeSettingsDto> | null>(null)
  const runtimeRefreshTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const blockedNotificationSyncPollTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const blockedNotificationSyncPollTargetRef = useRef<BlockedNotificationSyncPollTarget | null>(null)
  const blockedNotificationSyncPollInFlightRef = useRef(false)
  const pendingRuntimeRefreshRef = useRef<{
    projectId: string
    source: Extract<RefreshSource, 'runtime_run:updated' | 'runtime_stream:action_required'>
  } | null>(null)
  const runtimeActionRefreshKeysRef = useRef<Record<string, Set<string>>>({})
  const runtimeRunRefreshKeyRef = useRef<Record<string, string>>({})

  useEffect(() => {
    activeProjectRef.current = activeProject
  }, [activeProject])

  useEffect(() => {
    activeProjectIdRef.current = activeProjectId
  }, [activeProjectId])

  useEffect(() => {
    repositoryDiffsRef.current = repositoryDiffs
  }, [repositoryDiffs])

  useEffect(() => {
    runtimeSessionsRef.current = runtimeSessions
  }, [runtimeSessions])

  useEffect(() => {
    runtimeRunsRef.current = runtimeRuns
  }, [runtimeRuns])

  useEffect(() => {
    autonomousRunsRef.current = autonomousRuns
  }, [autonomousRuns])

  useEffect(() => {
    autonomousUnitsRef.current = autonomousUnits
  }, [autonomousUnits])

  useEffect(() => {
    autonomousAttemptsRef.current = autonomousAttempts
  }, [autonomousAttempts])

  useEffect(() => {
    autonomousHistoriesRef.current = autonomousHistories
  }, [autonomousHistories])

  useEffect(() => {
    autonomousRecentArtifactsRef.current = autonomousRecentArtifacts
  }, [autonomousRecentArtifacts])

  useEffect(() => {
    notificationRoutesRef.current = notificationRoutes
  }, [notificationRoutes])

  useEffect(() => {
    notificationRouteLoadStatusesRef.current = notificationRouteLoadStatuses
  }, [notificationRouteLoadStatuses])

  useEffect(() => {
    notificationRouteLoadErrorsRef.current = notificationRouteLoadErrors
  }, [notificationRouteLoadErrors])

  useEffect(() => {
    notificationSyncSummariesRef.current = notificationSyncSummaries
  }, [notificationSyncSummaries])

  useEffect(() => {
    runtimeSettingsRef.current = runtimeSettings
  }, [runtimeSettings])

  const updateRuntimeStream = useCallback(
    (projectId: string, updater: (current: RuntimeStreamView | null) => RuntimeStreamView | null) => {
      setRuntimeStreams((currentStreams) => {
        const nextStream = updater(currentStreams[projectId] ?? null)
        if (!nextStream) {
          return removeProjectRecord(currentStreams, projectId)
        }

        return {
          ...currentStreams,
          [projectId]: nextStream,
        }
      })
    },
    [],
  )

  const resetRepositoryDiffs = useCallback((status: RepositoryStatusView | null) => {
    setRepositoryDiffs(createInitialRepositoryDiffs())
    setActiveDiffScope(getDefaultDiffScope(status))
  }, [])

  const handleAdapterEventError = useCallback((error: CadenceDesktopError) => {
    setErrorMessage(getDesktopErrorMessage(error))
  }, [])

  const applyRuntimeSessionUpdate = useCallback(
    (runtimeSession: RuntimeSessionView, options: { clearGlobalError?: boolean } = {}) => {
      setRuntimeSessions((currentRuntimeSessions) => ({
        ...currentRuntimeSessions,
        [runtimeSession.projectId]: runtimeSession,
      }))
      setRuntimeLoadErrors((currentErrors) => ({
        ...currentErrors,
        [runtimeSession.projectId]: null,
      }))
      setProjects((currentProjects) =>
        currentProjects.map((project) =>
          project.id === runtimeSession.projectId ? applyRuntimeToProjectList(project, runtimeSession) : project,
        ),
      )
      setActiveProject((currentProject) =>
        currentProject && currentProject.id === runtimeSession.projectId
          ? applyRuntimeSession(currentProject, runtimeSession)
          : currentProject,
      )

      if ((runtimeSession.isSignedOut || runtimeSession.isFailed) && runtimeSession.projectId) {
        setRuntimeStreams((currentStreams) => removeProjectRecord(currentStreams, runtimeSession.projectId))
      }

      if (options.clearGlobalError ?? true) {
        setErrorMessage(null)
      }

      return runtimeSession
    },
    [],
  )

  const applyRuntimeRunUpdate = useCallback(
    (
      projectId: string,
      runtimeRun: RuntimeRunView | null,
      options: { clearGlobalError?: boolean; loadError?: string | null } = {},
    ) => {
      setRuntimeRuns((currentRuntimeRuns) => {
        if (!runtimeRun) {
          return removeProjectRecord(currentRuntimeRuns, projectId)
        }

        return {
          ...currentRuntimeRuns,
          [projectId]: runtimeRun,
        }
      })
      setRuntimeRunLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: options.loadError ?? null,
      }))
      setActiveProject((currentProject) =>
        currentProject && currentProject.id === projectId ? applyRuntimeRun(currentProject, runtimeRun) : currentProject,
      )

      if (options.clearGlobalError ?? false) {
        setErrorMessage(options.loadError ?? null)
      }

      return runtimeRun
    },
    [],
  )

  const applyAutonomousRunStateUpdate = useCallback(
    (
      projectId: string,
      inspection: {
        autonomousRun: ProjectDetailView['autonomousRun']
        autonomousUnit: ProjectDetailView['autonomousUnit']
        autonomousAttempt: ProjectDetailView['autonomousAttempt']
        autonomousHistory: ProjectDetailView['autonomousHistory']
        autonomousRecentArtifacts: ProjectDetailView['autonomousRecentArtifacts']
      },
      options: { clearGlobalError?: boolean; loadError?: string | null } = {},
    ) => {
      setAutonomousRuns((currentRuns) => {
        if (!inspection.autonomousRun) {
          return removeProjectRecord(currentRuns, projectId)
        }

        return {
          ...currentRuns,
          [projectId]: inspection.autonomousRun,
        }
      })
      setAutonomousUnits((currentUnits) => {
        if (!inspection.autonomousUnit) {
          return removeProjectRecord(currentUnits, projectId)
        }

        return {
          ...currentUnits,
          [projectId]: inspection.autonomousUnit,
        }
      })
      setAutonomousAttempts((currentAttempts) => {
        if (!inspection.autonomousAttempt) {
          return removeProjectRecord(currentAttempts, projectId)
        }

        return {
          ...currentAttempts,
          [projectId]: inspection.autonomousAttempt,
        }
      })
      setAutonomousHistories((currentHistories) => ({
        ...currentHistories,
        [projectId]: inspection.autonomousHistory,
      }))
      setAutonomousRecentArtifacts((currentArtifacts) => ({
        ...currentArtifacts,
        [projectId]: inspection.autonomousRecentArtifacts,
      }))
      setAutonomousRunLoadErrors((currentErrors) => ({
        ...currentErrors,
        [projectId]: options.loadError ?? null,
      }))
      setActiveProject((currentProject) =>
        currentProject && currentProject.id === projectId
          ? applyAutonomousRunState(
              currentProject,
              inspection.autonomousRun,
              inspection.autonomousUnit,
              inspection.autonomousAttempt,
              inspection.autonomousHistory,
              inspection.autonomousRecentArtifacts,
            )
          : currentProject,
      )

      if (options.clearGlobalError ?? false) {
        setErrorMessage(options.loadError ?? null)
      }

      return inspection.autonomousRun
    },
    [],
  )

  const syncRuntimeSession = useCallback(
    async (projectId: string) => {
      const response = await adapter.getRuntimeSession(projectId)
      return applyRuntimeSessionUpdate(mapRuntimeSession(response), { clearGlobalError: false })
    },
    [adapter, applyRuntimeSessionUpdate],
  )

  const syncRuntimeRun = useCallback(
    async (projectId: string) => {
      const response = await adapter.getRuntimeRun(projectId)
      return applyRuntimeRunUpdate(projectId, response ? mapRuntimeRun(response) : null, {
        clearGlobalError: false,
        loadError: null,
      })
    },
    [adapter, applyRuntimeRunUpdate],
  )

  const syncAutonomousRun = useCallback(
    async (projectId: string) => {
      const response = await adapter.getAutonomousRun(projectId)
      const inspection = mapAutonomousRunInspection(response)
      applyAutonomousRunStateUpdate(projectId, inspection, {
        clearGlobalError: false,
        loadError: null,
      })
      return inspection.autonomousRun
    },
    [adapter, applyAutonomousRunStateUpdate],
  )

  const loadNotificationRoutes = useCallback(
    async (projectId: string, options: { force?: boolean } = {}): Promise<NotificationRoutesLoadResult> => {
      const force = options.force ?? false
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
    },
    [adapter],
  )

  const loadProject = useCallback(
    async (
      projectId: string,
      source: Exclude<RefreshSource, 'repository:status_changed' | 'runtime:updated' | null>,
    ) => {
      const requestId = latestLoadRequestRef.current + 1
      latestLoadRequestRef.current = requestId
      setIsProjectLoading(true)
      setRefreshSource(source)
      setErrorMessage(null)

      if (source !== 'operator:resolve' && source !== 'operator:resume') {
        setOperatorActionError(null)
        setPendingOperatorActionId(null)
        setOperatorActionStatus('idle')
      }

      setRuntimeRunActionError(null)
      setPendingRuntimeRunAction(null)
      setRuntimeRunActionStatus('idle')
      setAutonomousRunActionError(null)
      setPendingAutonomousRunAction(null)
      setAutonomousRunActionStatus('idle')
      setNotificationRouteMutationError(null)

      const runtimePromise = adapter
        .getRuntimeSession(projectId)
        .then((response) => ({
          ok: true as const,
          runtime: mapRuntimeSession(response),
          error: null as string | null,
        }))
        .catch((error) => ({
          ok: false as const,
          runtime: runtimeSessionsRef.current[projectId] ?? null,
          error: getDesktopErrorMessage(error),
        }))

      const runtimeRunPromise = adapter
        .getRuntimeRun(projectId)
        .then((response) => ({
          ok: true as const,
          runtimeRun: response ? mapRuntimeRun(response) : null,
          error: null as string | null,
        }))
        .catch((error) => ({
          ok: false as const,
          runtimeRun: runtimeRunsRef.current[projectId] ?? null,
          error: getDesktopErrorMessage(error),
        }))

      const autonomousRunPromise = adapter
        .getAutonomousRun(projectId)
        .then((response) => ({
          ok: true as const,
          inspection: mapAutonomousRunInspection(response),
          error: null as string | null,
        }))
        .catch((error) => ({
          ok: false as const,
          inspection: {
            autonomousRun: autonomousRunsRef.current[projectId] ?? null,
            autonomousUnit: autonomousUnitsRef.current[projectId] ?? null,
            autonomousAttempt: autonomousAttemptsRef.current[projectId] ?? null,
            autonomousHistory: autonomousHistoriesRef.current[projectId] ?? [],
            autonomousRecentArtifacts: autonomousRecentArtifactsRef.current[projectId] ?? [],
          },
          error: getDesktopErrorMessage(error),
        }))

      const shouldSyncNotificationAdapters = source !== 'runtime_run:updated'
      const syncResult = shouldSyncNotificationAdapters
        ? await adapter
            .syncNotificationAdapters(projectId)
            .then((summary) => ({
              attempted: true as const,
              summary,
              error: null as OperatorActionErrorView | null,
              errorMessage: null as string | null,
            }))
            .catch((error) => {
              const metadata = getOperatorActionError(
                error,
                'Cadence could not sync notification adapters for this project.',
              )
              return {
                attempted: true as const,
                summary: notificationSyncSummariesRef.current[projectId] ?? null,
                error: metadata,
                errorMessage: metadata.message,
              }
            })
        : {
            attempted: false as const,
            summary: notificationSyncSummariesRef.current[projectId] ?? null,
            error: null as OperatorActionErrorView | null,
            errorMessage: null as string | null,
          }

      const brokerPromise = adapter
        .listNotificationDispatches(projectId)
        .then((response) => ({
          ok: true as const,
          dispatches: response.dispatches,
          error: null as string | null,
        }))
        .catch((error) => ({
          ok: false as const,
          dispatches: notificationDispatchesRef.current[projectId] ?? [],
          error: getDesktopErrorMessage(error),
        }))

      const shouldRefreshRoutes = source !== 'runtime_run:updated' && source !== 'runtime_stream:action_required'
      const routePromise = shouldRefreshRoutes
        ? loadNotificationRoutes(projectId, {
            force: source === 'startup' || source === 'selection' || source === 'import',
          }).then((result) => ({
            ok: result.loadError === null,
            routes: result.routes,
            error: result.loadError?.message ?? null,
          }))
        : Promise.resolve({
            ok: true as const,
            routes: notificationRoutesRef.current[projectId] ?? [],
            error: null as string | null,
          })

      try {
        const [snapshotResponse, statusResponse, brokerResult, routeResult] = await Promise.all([
          adapter.getProjectSnapshot(projectId),
          adapter.getRepositoryStatus(projectId),
          brokerPromise,
          routePromise,
        ])

        if (latestLoadRequestRef.current !== requestId) {
          return null
        }

        if (syncResult.attempted) {
          if (syncResult.summary) {
            setNotificationSyncSummaries((currentSummaries) => ({
              ...currentSummaries,
              [projectId]: syncResult.summary,
            }))
          }

          setNotificationSyncErrors((currentErrors) => ({
            ...currentErrors,
            [projectId]: syncResult.error,
          }))
        }

        notificationDispatchesRef.current[projectId] = brokerResult.dispatches
        const snapshotProject = mapProjectSnapshot(snapshotResponse, {
          notificationDispatches: brokerResult.dispatches,
        })
        const status = mapRepositoryStatus(statusResponse)
        const cachedRuntime = runtimeSessionsRef.current[projectId] ?? null
        const cachedRuntimeRun = runtimeRunsRef.current[projectId] ?? null
        const cachedAutonomousRun = autonomousRunsRef.current[projectId] ?? snapshotProject.autonomousRun ?? null
        const cachedAutonomousUnit = autonomousUnitsRef.current[projectId] ?? snapshotProject.autonomousUnit ?? null
        const cachedAutonomousAttempt = autonomousAttemptsRef.current[projectId] ?? snapshotProject.autonomousAttempt ?? null
        const cachedAutonomousHistory = autonomousHistoriesRef.current[projectId] ?? snapshotProject.autonomousHistory
        const cachedAutonomousRecentArtifacts =
          autonomousRecentArtifactsRef.current[projectId] ?? snapshotProject.autonomousRecentArtifacts
        const nextProject = applyAutonomousRunState(
          applyRuntimeRun(
            applyRuntimeSession(applyRepositoryStatus(snapshotProject, status), cachedRuntime),
            cachedRuntimeRun,
          ),
          cachedAutonomousRun,
          cachedAutonomousUnit,
          cachedAutonomousAttempt,
          cachedAutonomousHistory,
          cachedAutonomousRecentArtifacts,
        )
        const nextSummary = mapProjectSummary(snapshotResponse.project)

        setProjects((currentProjects) =>
          upsertProjectListItem(
            currentProjects,
            cachedRuntime ? applyRuntimeToProjectList(nextSummary, cachedRuntime) : nextSummary,
          ),
        )
        setRepositoryStatus(status)
        setActiveProjectId(projectId)
        setActiveProject(nextProject)
        resetRepositoryDiffs(status)

        const [runtimeResult, runtimeRunResult, autonomousRunResult] = await Promise.all([
          runtimePromise,
          runtimeRunPromise,
          autonomousRunPromise,
        ])
        if (latestLoadRequestRef.current !== requestId) {
          return nextProject
        }

        if (runtimeResult.runtime) {
          setRuntimeSessions((currentRuntimeSessions) => ({
            ...currentRuntimeSessions,
            [projectId]: runtimeResult.runtime,
          }))
          setProjects((currentProjects) =>
            currentProjects.map((project) =>
              project.id === projectId ? applyRuntimeToProjectList(project, runtimeResult.runtime as RuntimeSessionView) : project,
            ),
          )
        }

        if (runtimeRunResult.ok) {
          setRuntimeRuns((currentRuntimeRuns) => {
            if (!runtimeRunResult.runtimeRun) {
              return removeProjectRecord(currentRuntimeRuns, projectId)
            }

            return {
              ...currentRuntimeRuns,
              [projectId]: runtimeRunResult.runtimeRun,
            }
          })
        } else if (runtimeRunResult.runtimeRun) {
          setRuntimeRuns((currentRuntimeRuns) => ({
            ...currentRuntimeRuns,
            [projectId]: runtimeRunResult.runtimeRun,
          }))
        }

        if (autonomousRunResult.ok) {
          setAutonomousRuns((currentRuns) => {
            if (!autonomousRunResult.inspection.autonomousRun) {
              return removeProjectRecord(currentRuns, projectId)
            }

            return {
              ...currentRuns,
              [projectId]: autonomousRunResult.inspection.autonomousRun,
            }
          })
          setAutonomousUnits((currentUnits) => {
            if (!autonomousRunResult.inspection.autonomousUnit) {
              return removeProjectRecord(currentUnits, projectId)
            }

            return {
              ...currentUnits,
              [projectId]: autonomousRunResult.inspection.autonomousUnit,
            }
          })
          setAutonomousAttempts((currentAttempts) => {
            if (!autonomousRunResult.inspection.autonomousAttempt) {
              return removeProjectRecord(currentAttempts, projectId)
            }

            return {
              ...currentAttempts,
              [projectId]: autonomousRunResult.inspection.autonomousAttempt,
            }
          })
          setAutonomousHistories((currentHistories) => ({
            ...currentHistories,
            [projectId]: autonomousRunResult.inspection.autonomousHistory,
          }))
          setAutonomousRecentArtifacts((currentArtifacts) => ({
            ...currentArtifacts,
            [projectId]: autonomousRunResult.inspection.autonomousRecentArtifacts,
          }))
        } else {
          if (autonomousRunResult.inspection.autonomousRun) {
            setAutonomousRuns((currentRuns) => ({
              ...currentRuns,
              [projectId]: autonomousRunResult.inspection.autonomousRun,
            }))
          }

          if (autonomousRunResult.inspection.autonomousUnit) {
            setAutonomousUnits((currentUnits) => ({
              ...currentUnits,
              [projectId]: autonomousRunResult.inspection.autonomousUnit,
            }))
          }

          if (autonomousRunResult.inspection.autonomousAttempt) {
            setAutonomousAttempts((currentAttempts) => ({
              ...currentAttempts,
              [projectId]: autonomousRunResult.inspection.autonomousAttempt,
            }))
          }

          setAutonomousHistories((currentHistories) => ({
            ...currentHistories,
            [projectId]: autonomousRunResult.inspection.autonomousHistory,
          }))
          setAutonomousRecentArtifacts((currentArtifacts) => ({
            ...currentArtifacts,
            [projectId]: autonomousRunResult.inspection.autonomousRecentArtifacts,
          }))
        }

        setRuntimeLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: runtimeResult.error,
        }))
        setRuntimeRunLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: runtimeRunResult.error,
        }))
        setAutonomousRunLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: autonomousRunResult.error,
        }))

        const finalRuntime = runtimeResult.runtime ?? cachedRuntime
        const finalRuntimeRun = runtimeRunResult.ok ? runtimeRunResult.runtimeRun : runtimeRunResult.runtimeRun ?? cachedRuntimeRun
        const finalAutonomousRun = autonomousRunResult.ok
          ? autonomousRunResult.inspection.autonomousRun
          : autonomousRunResult.inspection.autonomousRun ?? cachedAutonomousRun
        const finalAutonomousUnit = autonomousRunResult.ok
          ? autonomousRunResult.inspection.autonomousUnit
          : autonomousRunResult.inspection.autonomousUnit ?? cachedAutonomousUnit
        const finalAutonomousAttempt = autonomousRunResult.ok
          ? autonomousRunResult.inspection.autonomousAttempt
          : autonomousRunResult.inspection.autonomousAttempt ?? cachedAutonomousAttempt
        const finalAutonomousHistory = autonomousRunResult.ok
          ? autonomousRunResult.inspection.autonomousHistory
          : autonomousRunResult.inspection.autonomousHistory.length > 0
            ? autonomousRunResult.inspection.autonomousHistory
            : cachedAutonomousHistory
        const finalAutonomousRecentArtifacts = autonomousRunResult.ok
          ? autonomousRunResult.inspection.autonomousRecentArtifacts
          : autonomousRunResult.inspection.autonomousRecentArtifacts.length > 0
            ? autonomousRunResult.inspection.autonomousRecentArtifacts
            : cachedAutonomousRecentArtifacts
        const finalizedProject = applyAutonomousRunState(
          applyRuntimeRun(
            finalRuntime ? applyRuntimeSession(nextProject, finalRuntime) : nextProject,
            finalRuntimeRun,
          ),
          finalAutonomousRun,
          finalAutonomousUnit,
          finalAutonomousAttempt,
          finalAutonomousHistory,
          finalAutonomousRecentArtifacts,
        )
        setActiveProject((currentProject) => {
          if (!currentProject || currentProject.id !== projectId) {
            return currentProject
          }

          return finalizedProject
        })
        setErrorMessage(
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
        if (latestLoadRequestRef.current === requestId) {
          const nextMessage = getDesktopErrorMessage(error)
          setErrorMessage(nextMessage)

          if (source === 'operator:resolve' || source === 'operator:resume') {
            setOperatorActionError(getOperatorActionError(error, nextMessage))
          }
        }

        return null
      } finally {
        if (latestLoadRequestRef.current === requestId) {
          setIsProjectLoading(false)
        }
      }
    },
    [adapter, loadNotificationRoutes, resetRepositoryDiffs],
  )

  const scheduleRuntimeMetadataRefresh = useCallback(
    (projectId: string, source: Extract<RefreshSource, 'runtime_run:updated' | 'runtime_stream:action_required'>) => {
      if (activeProjectIdRef.current !== projectId) {
        return
      }

      pendingRuntimeRefreshRef.current = { projectId, source }
      if (runtimeRefreshTimeoutRef.current) {
        return
      }

      runtimeRefreshTimeoutRef.current = setTimeout(() => {
        runtimeRefreshTimeoutRef.current = null
        const pendingRefresh = pendingRuntimeRefreshRef.current
        pendingRuntimeRefreshRef.current = null
        if (!pendingRefresh) {
          return
        }

        if (activeProjectIdRef.current !== pendingRefresh.projectId) {
          return
        }

        void loadProject(pendingRefresh.projectId, pendingRefresh.source)
      }, 120)
    },
    [loadProject],
  )

  const clearBlockedNotificationSyncPoll = useCallback(() => {
    if (blockedNotificationSyncPollTimeoutRef.current) {
      clearTimeout(blockedNotificationSyncPollTimeoutRef.current)
      blockedNotificationSyncPollTimeoutRef.current = null
    }
  }, [])

  const scheduleBlockedNotificationSyncPoll = useCallback(
    (expectedPollKey: string) => {
      if (blockedNotificationSyncPollTimeoutRef.current) {
        return
      }

      blockedNotificationSyncPollTimeoutRef.current = setTimeout(() => {
        blockedNotificationSyncPollTimeoutRef.current = null

        const pollTarget = blockedNotificationSyncPollTargetRef.current
        if (!pollTarget || getBlockedNotificationSyncPollKey(pollTarget) !== expectedPollKey) {
          return
        }

        if (activeProjectIdRef.current !== pollTarget.projectId) {
          return
        }

        if (blockedNotificationSyncPollInFlightRef.current) {
          scheduleBlockedNotificationSyncPoll(expectedPollKey)
          return
        }

        blockedNotificationSyncPollInFlightRef.current = true
        void loadProject(pollTarget.projectId, 'runtime_stream:action_required').finally(() => {
          blockedNotificationSyncPollInFlightRef.current = false
          const nextTarget = blockedNotificationSyncPollTargetRef.current
          if (!nextTarget || getBlockedNotificationSyncPollKey(nextTarget) !== expectedPollKey) {
            return
          }

          if (activeProjectIdRef.current !== nextTarget.projectId) {
            return
          }

          scheduleBlockedNotificationSyncPoll(expectedPollKey)
        })
      }, BLOCKED_NOTIFICATION_SYNC_POLL_MS)
    },
    [loadProject],
  )

  useEffect(() => {
    return () => {
      if (runtimeRefreshTimeoutRef.current) {
        clearTimeout(runtimeRefreshTimeoutRef.current)
        runtimeRefreshTimeoutRef.current = null
      }
      pendingRuntimeRefreshRef.current = null
      clearBlockedNotificationSyncPoll()
      blockedNotificationSyncPollTargetRef.current = null
      blockedNotificationSyncPollInFlightRef.current = false
    }
  }, [clearBlockedNotificationSyncPoll])

  const bootstrap = useCallback(async (source: 'startup' | 'remove' = 'startup') => {
    setIsLoading(true)
    setRefreshSource(source)
    setErrorMessage(null)

    try {
      const response = await adapter.listProjects()
      const nextProjects = response.projects.map(mapProjectSummary)
      setProjects(nextProjects)

      if (nextProjects.length === 0) {
        setActiveProjectId(null)
        setActiveProject(null)
        setRepositoryStatus(null)
        setRuntimeRuns({})
        setAutonomousRuns({})
        setAutonomousUnits({})
        setAutonomousAttempts({})
        setAutonomousHistories({})
        setAutonomousRecentArtifacts({})
        setNotificationRoutes({})
        setNotificationRouteLoadStatuses({})
        setNotificationRouteLoadErrors({})
        setNotificationSyncSummaries({})
        setNotificationSyncErrors({})
        setNotificationRouteMutationStatus('idle')
        setPendingNotificationRouteId(null)
        setNotificationRouteMutationError(null)
        setRuntimeStreams({})
        setRuntimeLoadErrors({})
        setRuntimeRunLoadErrors({})
        setAutonomousRunLoadErrors({})
        trustSnapshotRef.current = {}
        resetRepositoryDiffs(null)
        return
      }

      const preferredProjectId = activeProjectIdRef.current
      const nextProjectId =
        preferredProjectId && nextProjects.some((project) => project.id === preferredProjectId)
          ? preferredProjectId
          : nextProjects[0].id

      await loadProject(nextProjectId, source)
    } catch (error) {
      setErrorMessage(getDesktopErrorMessage(error))
    } finally {
      setIsLoading(false)
    }
  }, [adapter, loadProject, resetRepositoryDiffs])

  useEffect(() => {
    let projectUnlisten: (() => void) | null = null
    let repositoryUnlisten: (() => void) | null = null
    let runtimeUnlisten: (() => void) | null = null
    let runtimeRunUnlisten: (() => void) | null = null
    let disposed = false

    void bootstrap()

    const attachListeners = async () => {
      projectUnlisten = await adapter.onProjectUpdated(
        (payload) => {
          if (disposed) {
            return
          }

          const summary = mapProjectSummary(payload.project)
          const cachedRuntime = runtimeSessionsRef.current[summary.id] ?? null
          setProjects((currentProjects) =>
            upsertProjectListItem(currentProjects, cachedRuntime ? applyRuntimeToProjectList(summary, cachedRuntime) : summary),
          )

          if (activeProjectIdRef.current !== summary.id) {
            return
          }

          void loadProject(summary.id, 'project:updated')
        },
        handleAdapterEventError,
      )

      repositoryUnlisten = await adapter.onRepositoryStatusChanged(
        (payload) => {
          if (disposed || activeProjectIdRef.current !== payload.projectId) {
            return
          }

          const nextStatus = mapRepositoryStatus(payload.status)
          setRefreshSource('repository:status_changed')
          setRepositoryStatus(nextStatus)
          setActiveProject((currentProject) => {
            if (!currentProject) {
              return currentProject
            }

            const nextProject = applyRepositoryStatus(currentProject, nextStatus)
            const withRuntime = currentProject.runtimeSession ? applyRuntimeSession(nextProject, currentProject.runtimeSession) : nextProject
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

          const currentRuntime = runtimeSessionsRef.current[payload.projectId] ?? null
          const nextRuntime = mergeRuntimeUpdated(currentRuntime, payload)

          setRuntimeSessions((currentRuntimeSessions) => ({
            ...currentRuntimeSessions,
            [payload.projectId]: nextRuntime,
          }))
          setRuntimeLoadErrors((currentErrors) => ({
            ...currentErrors,
            [payload.projectId]: null,
          }))
          setProjects((currentProjects) =>
            currentProjects.map((project) =>
              project.id === payload.projectId ? applyRuntimeToProjectList(project, nextRuntime) : project,
            ),
          )

          if (!nextRuntime.isAuthenticated) {
            setRuntimeStreams((currentStreams) => removeProjectRecord(currentStreams, payload.projectId))
          }

          if (activeProjectIdRef.current !== payload.projectId) {
            return
          }

          setRefreshSource('runtime:updated')
          setErrorMessage(null)
          setActiveProject((currentProject) =>
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

          if (activeProjectIdRef.current !== payload.projectId) {
            return
          }

          const refreshKey = payload.run
            ? `${payload.run.runId}:${payload.run.lastCheckpointSequence}:${payload.run.updatedAt}:${payload.run.status}`
            : 'none'
          if (runtimeRunRefreshKeyRef.current[payload.projectId] !== refreshKey) {
            runtimeRunRefreshKeyRef.current[payload.projectId] = refreshKey
            scheduleRuntimeMetadataRefresh(payload.projectId, 'runtime_run:updated')
          }

          setRefreshSource('runtime_run:updated')
          setErrorMessage(null)
        },
        handleAdapterEventError,
      )
    }

    void attachListeners()

    return () => {
      disposed = true
      projectUnlisten?.()
      repositoryUnlisten?.()
      runtimeUnlisten?.()
      runtimeRunUnlisten?.()
    }
  }, [adapter, applyRuntimeRunUpdate, bootstrap, handleAdapterEventError, scheduleRuntimeMetadataRefresh])

  const showRepositoryDiff = useCallback(
    async (scope: RepositoryDiffScope, options: { force?: boolean } = {}) => {
      setActiveDiffScope(scope)

      const projectId = activeProjectIdRef.current
      if (!projectId) {
        return
      }

      const currentDiffState = repositoryDiffsRef.current[scope]
      if (
        !options.force &&
        currentDiffState.projectId === projectId &&
        (currentDiffState.status === 'ready' || currentDiffState.status === 'loading')
      ) {
        return
      }

      const requestId = latestDiffRequestRef.current[scope] + 1
      latestDiffRequestRef.current[scope] = requestId

      setRepositoryDiffs((currentDiffs) => ({
        ...currentDiffs,
        [scope]: {
          ...currentDiffs[scope],
          status: 'loading',
          errorMessage: null,
          projectId,
        },
      }))

      try {
        const response = await adapter.getRepositoryDiff(projectId, scope)
        if (activeProjectIdRef.current !== projectId || latestDiffRequestRef.current[scope] !== requestId) {
          return
        }

        const nextDiff = mapRepositoryDiff(response)
        setRepositoryDiffs((currentDiffs) => ({
          ...currentDiffs,
          [scope]: {
            status: 'ready',
            diff: nextDiff,
            errorMessage: null,
            projectId,
          },
        }))
      } catch (error) {
        if (activeProjectIdRef.current !== projectId || latestDiffRequestRef.current[scope] !== requestId) {
          return
        }

        const nextMessage = getDesktopErrorMessage(error)
        setRepositoryDiffs((currentDiffs) => ({
          ...currentDiffs,
          [scope]: {
            ...currentDiffs[scope],
            status: 'error',
            errorMessage: nextMessage,
            projectId,
          },
        }))
      }
    },
    [adapter],
  )

  const selectProject = useCallback(
    async (projectId: string) => {
      if (projectId === activeProjectIdRef.current) {
        return
      }

      await loadProject(projectId, 'selection')
    },
    [loadProject],
  )

  const importProject = useCallback(async () => {
    setIsImporting(true)
    setRefreshSource('import')
    setErrorMessage(null)

    try {
      const selectedPath = await adapter.pickRepositoryFolder()
      if (!selectedPath) {
        return
      }

      const response = await adapter.importRepository(selectedPath)
      const summary = mapProjectSummary(response.project)
      setProjects((currentProjects) => upsertProjectListItem(currentProjects, summary))
      await loadProject(summary.id, 'import')
    } catch (error) {
      setErrorMessage(getDesktopErrorMessage(error))
    } finally {
      setIsImporting(false)
    }
  }, [adapter, loadProject])

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
    [adapter, bootstrap],
  )

  const retry = useCallback(async () => {
    if (activeProjectIdRef.current) {
      const projectId = activeProjectIdRef.current
      delete runtimeActionRefreshKeysRef.current[projectId]
      delete runtimeRunRefreshKeyRef.current[projectId]
      await loadProject(projectId, 'selection')
      setRuntimeStreamRetryToken((current) => current + 1)
      return
    }

    await bootstrap()
  }, [bootstrap, loadProject])

  const retryActiveRepositoryDiff = useCallback(async () => {
    await showRepositoryDiff(activeDiffScope, { force: true })
  }, [activeDiffScope, showRepositoryDiff])

  const listProjectFiles = useCallback(
    async (projectId: string) => {
      return await adapter.listProjectFiles(projectId)
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

  const createProjectEntry = useCallback(
    async (request: CreateProjectEntryRequestDto) => {
      return await adapter.createProjectEntry(request)
    },
    [adapter],
  )

  const renameProjectEntry = useCallback(
    async (request: RenameProjectEntryRequestDto) => {
      return await adapter.renameProjectEntry(request)
    },
    [adapter],
  )

  const deleteProjectEntry = useCallback(
    async (projectId: string, path: string) => {
      return await adapter.deleteProjectEntry(projectId, path)
    },
    [adapter],
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
          setRuntimeSettingsLoadError(getOperatorActionError(error, 'Cadence could not load app-global runtime settings.'))
          throw error
        } finally {
          runtimeSettingsLoadInFlightRef.current = null
        }
      })()

      runtimeSettingsLoadInFlightRef.current = loadPromise
      return loadPromise
    },
    [adapter, runtimeSettingsLoadStatus],
  )

  const upsertRuntimeSettings = useCallback(
    async (request: UpsertRuntimeSettingsRequestDto) => {
      setRuntimeSettingsSaveStatus('running')
      setRuntimeSettingsSaveError(null)

      try {
        const response = await adapter.upsertRuntimeSettings(request)
        setRuntimeSettings(response)
        setRuntimeSettingsLoadStatus('ready')
        setRuntimeSettingsLoadError(null)
        setRuntimeSettingsSaveError(null)
        return response
      } catch (error) {
        setRuntimeSettingsSaveError(getOperatorActionError(error, 'Cadence could not save app-global runtime settings.'))
        throw error
      } finally {
        setRuntimeSettingsSaveStatus('idle')
      }
    },
    [adapter],
  )

  useEffect(() => {
    if (runtimeSettings || runtimeSettingsLoadStatus !== 'idle') {
      return
    }

    void refreshRuntimeSettings().catch(() => undefined)
  }, [refreshRuntimeSettings, runtimeSettings, runtimeSettingsLoadStatus])

  const refreshNotificationRoutes = useCallback(
    async (options: { force?: boolean } = {}) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before loading notification routes.')
      }

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
    [loadNotificationRoutes],
  )

  const upsertNotificationRoute = useCallback(
    async (request: Omit<UpsertNotificationRouteRequestDto, 'projectId'>) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before saving a notification route.')
      }

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
          const nextRoutes = [response.route, ...existingRoutes.filter((route) => route.routeId !== response.route.routeId)]

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
    [adapter, loadNotificationRoutes],
  )

  const resolveOperatorAction = useCallback(
    async (actionId: string, decision: OperatorActionDecision, options: { userAnswer?: string | null } = {}) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before resolving an operator action.')
      }

      setOperatorActionStatus('running')
      setPendingOperatorActionId(actionId)
      setOperatorActionError(null)
      setErrorMessage(null)

      try {
        await adapter.resolveOperatorAction(projectId, actionId, decision, {
          userAnswer: options.userAnswer ?? null,
        })
        await loadProject(projectId, 'operator:resolve')
        return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
      } catch (error) {
        setOperatorActionError(
          getOperatorActionError(error, 'Cadence could not persist the operator decision for this project.'),
        )
        throw error
      } finally {
        setOperatorActionStatus('idle')
        setPendingOperatorActionId(null)
      }
    },
    [adapter, loadProject],
  )

  const resumeOperatorRun = useCallback(
    async (actionId: string, options: { userAnswer?: string | null } = {}) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before resuming the runtime session.')
      }

      setOperatorActionStatus('running')
      setPendingOperatorActionId(actionId)
      setOperatorActionError(null)
      setErrorMessage(null)

      try {
        await adapter.resumeOperatorRun(projectId, actionId, {
          userAnswer: options.userAnswer ?? null,
        })
        await loadProject(projectId, 'operator:resume')
        return activeProjectIdRef.current === projectId ? activeProjectRef.current : null
      } catch (error) {
        setOperatorActionError(
          getOperatorActionError(error, 'Cadence could not record the operator resume request for this project.'),
        )
        throw error
      } finally {
        setOperatorActionStatus('idle')
        setPendingOperatorActionId(null)
      }
    },
    [adapter, loadProject],
  )

  const startOpenAiLogin = useCallback(async () => {
    const projectId = activeProjectIdRef.current
    if (!projectId) {
      throw new Error('Select an imported project before starting OpenAI login.')
    }

    try {
      const response = await adapter.startOpenAiLogin(projectId, { originator: 'agent-pane' })
      return applyRuntimeSessionUpdate(mapRuntimeSession(response))
    } catch (error) {
      try {
        await syncRuntimeSession(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    }
  }, [adapter, applyRuntimeSessionUpdate, syncRuntimeSession])

  const submitOpenAiCallback = useCallback(
    async (flowId: string, options: { manualInput?: string | null } = {}) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before completing OpenAI login.')
      }

      try {
        const response = await adapter.submitOpenAiCallback(projectId, flowId, {
          manualInput: options.manualInput ?? null,
        })
        return applyRuntimeSessionUpdate(mapRuntimeSession(response))
      } catch (error) {
        try {
          await syncRuntimeSession(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      }
    },
    [adapter, applyRuntimeSessionUpdate, syncRuntimeSession],
  )

  const startAutonomousRun = useCallback(async () => {
    const projectId = activeProjectIdRef.current
    if (!projectId) {
      throw new Error('Select an imported project before starting an autonomous run.')
    }

    setAutonomousRunActionStatus('running')
    setPendingAutonomousRunAction('start')
    setAutonomousRunActionError(null)

    try {
      const response = await adapter.startAutonomousRun(projectId)
      return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
        clearGlobalError: false,
        loadError: null,
      })
    } catch (error) {
      setAutonomousRunActionError(
        getOperatorActionError(error, 'Cadence could not start or inspect the autonomous run for this project.'),
      )

      try {
        await syncAutonomousRun(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    } finally {
      setAutonomousRunActionStatus('idle')
      setPendingAutonomousRunAction(null)
    }
  }, [adapter, applyAutonomousRunStateUpdate, syncAutonomousRun])

  const inspectAutonomousRun = useCallback(async () => {
    const projectId = activeProjectIdRef.current
    if (!projectId) {
      throw new Error('Select an imported project before inspecting autonomous run truth.')
    }

    setAutonomousRunActionStatus('running')
    setPendingAutonomousRunAction('inspect')
    setAutonomousRunActionError(null)

    try {
      return await syncAutonomousRun(projectId)
    } catch (error) {
      setAutonomousRunActionError(
        getOperatorActionError(error, 'Cadence could not inspect the autonomous run truth for this project.'),
      )
      throw error
    } finally {
      setAutonomousRunActionStatus('idle')
      setPendingAutonomousRunAction(null)
    }
  }, [syncAutonomousRun])

  const cancelAutonomousRun = useCallback(
    async (runId: string) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before cancelling the autonomous run.')
      }

      setAutonomousRunActionStatus('running')
      setPendingAutonomousRunAction('cancel')
      setAutonomousRunActionError(null)

      try {
        const response = await adapter.cancelAutonomousRun(projectId, runId)
        return applyAutonomousRunStateUpdate(projectId, mapAutonomousRunInspection(response), {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setAutonomousRunActionError(
          getOperatorActionError(error, 'Cadence could not cancel the autonomous run for this project.'),
        )

        try {
          await syncAutonomousRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        setAutonomousRunActionStatus('idle')
        setPendingAutonomousRunAction(null)
      }
    },
    [adapter, applyAutonomousRunStateUpdate, syncAutonomousRun],
  )

  const startRuntimeRun = useCallback(async () => {
    const projectId = activeProjectIdRef.current
    if (!projectId) {
      throw new Error('Select an imported project before starting a supervised runtime run.')
    }

    setRuntimeRunActionStatus('running')
    setPendingRuntimeRunAction('start')
    setRuntimeRunActionError(null)

    try {
      const response = await adapter.startRuntimeRun(projectId)
      return applyRuntimeRunUpdate(projectId, mapRuntimeRun(response), {
        clearGlobalError: false,
        loadError: null,
      })
    } catch (error) {
      setRuntimeRunActionError(
        getOperatorActionError(error, 'Cadence could not start or reconnect the supervised runtime run for this project.'),
      )

      try {
        await syncRuntimeRun(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    } finally {
      setRuntimeRunActionStatus('idle')
      setPendingRuntimeRunAction(null)
    }
  }, [adapter, applyRuntimeRunUpdate, syncRuntimeRun])

  const startRuntimeSession = useCallback(async () => {
    const projectId = activeProjectIdRef.current
    if (!projectId) {
      throw new Error('Select an imported project before binding a runtime session.')
    }

    try {
      const response = await adapter.startRuntimeSession(projectId)
      return applyRuntimeSessionUpdate(mapRuntimeSession(response))
    } catch (error) {
      try {
        await syncRuntimeSession(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    }
  }, [adapter, applyRuntimeSessionUpdate, syncRuntimeSession])

  const stopRuntimeRun = useCallback(
    async (runId: string) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before stopping the supervised runtime run.')
      }

      setRuntimeRunActionStatus('running')
      setPendingRuntimeRunAction('stop')
      setRuntimeRunActionError(null)

      try {
        const response = await adapter.stopRuntimeRun(projectId, runId)
        return applyRuntimeRunUpdate(projectId, response ? mapRuntimeRun(response) : null, {
          clearGlobalError: false,
          loadError: null,
        })
      } catch (error) {
        setRuntimeRunActionError(
          getOperatorActionError(error, 'Cadence could not stop the supervised runtime run for this project.'),
        )

        try {
          await syncRuntimeRun(projectId)
        } catch {
          // Ignore follow-up refresh failures and preserve the last truthful state.
        }

        throw error
      } finally {
        setRuntimeRunActionStatus('idle')
        setPendingRuntimeRunAction(null)
      }
    },
    [adapter, applyRuntimeRunUpdate, syncRuntimeRun],
  )

  const logoutRuntimeSession = useCallback(async () => {
    const projectId = activeProjectIdRef.current
    if (!projectId) {
      throw new Error('Select an imported project before signing out.')
    }

    try {
      const response = await adapter.logoutRuntimeSession(projectId)
      return applyRuntimeSessionUpdate(mapRuntimeSession(response))
    } catch (error) {
      try {
        await syncRuntimeSession(projectId)
      } catch {
        // Ignore follow-up refresh failures and preserve the last truthful state.
      }

      throw error
    }
  }, [adapter, applyRuntimeSessionUpdate, syncRuntimeSession])

  const activeRuntimeSession = activeProjectId
    ? runtimeSessions[activeProjectId] ?? activeProject?.runtimeSession ?? null
    : null
  const activeRuntimeRun = activeProjectId ? runtimeRuns[activeProjectId] ?? activeProject?.runtimeRun ?? null : null
  const activeAutonomousRun = activeProjectId
    ? autonomousRuns[activeProjectId] ?? activeProject?.autonomousRun ?? null
    : null
  const activeAutonomousUnit = activeProjectId
    ? autonomousUnits[activeProjectId] ?? activeProject?.autonomousUnit ?? null
    : null
  const activeAutonomousAttempt = activeProjectId
    ? autonomousAttempts[activeProjectId] ?? activeProject?.autonomousAttempt ?? null
    : null
  const activeAutonomousHistory = activeProjectId
    ? autonomousHistories[activeProjectId] ?? activeProject?.autonomousHistory ?? []
    : []
  const activeAutonomousRecentArtifacts = activeProjectId
    ? autonomousRecentArtifacts[activeProjectId] ?? activeProject?.autonomousRecentArtifacts ?? []
    : []
  const activeAutonomousRunErrorMessage = activeProjectId ? autonomousRunLoadErrors[activeProjectId] ?? null : null
  const activeRuntimeRunId = activeRuntimeRun?.runId ?? null
  const activeRuntimeSubscriptionKey =
    activeProjectId && activeRuntimeSession?.isAuthenticated && activeRuntimeSession.sessionId && activeRuntimeRunId
      ? `${activeProjectId}:${activeRuntimeSession.sessionId}:${activeRuntimeRunId}:${runtimeStreamRetryToken}`
      : null

  useEffect(() => {
    const projectId = activeProjectId
    const runtimeSession = activeRuntimeSession
    const runId = activeRuntimeRunId

    if (!projectId) {
      return
    }

    if (!runtimeSession?.isAuthenticated || !runtimeSession.sessionId) {
      updateRuntimeStream(projectId, () => null)
      return
    }

    if (!runId) {
      updateRuntimeStream(projectId, () => null)
      return
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

      return
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
  }, [
    activeProjectId,
    activeRuntimeRunId,
    activeRuntimeSession,
    activeRuntimeSubscriptionKey,
    adapter,
    scheduleRuntimeMetadataRefresh,
    updateRuntimeStream,
  ])

  const activePhase = useMemo(() => getActivePhase(activeProject), [activeProject])
  const activeRuntimeErrorMessage = activeProject ? runtimeLoadErrors[activeProject.id] ?? null : null
  const activeRuntimeRunErrorMessage = activeProject ? runtimeRunLoadErrors[activeProject.id] ?? null : null
  const activeRuntimeStream = activeProject ? runtimeStreams[activeProject.id] ?? null : null
  const activeNotificationRoutes = activeProject
    ? (notificationRoutes[activeProject.id] ?? []).filter(
        (route) => route.projectId === activeProject.id && route.routeId.trim().length > 0,
      )
    : []
  const activeNotificationRouteLoadStatus: NotificationRoutesLoadStatus = activeProject
    ? notificationRouteLoadStatuses[activeProject.id] ?? 'idle'
    : 'idle'
  const activeNotificationRouteLoadError = activeProject
    ? notificationRouteLoadErrors[activeProject.id] ?? null
    : null
  const activeNotificationSyncSummary = activeProject
    ? notificationSyncSummaries[activeProject.id] ?? null
    : null
  const activeNotificationSyncError = activeProject
    ? notificationSyncErrors[activeProject.id] ?? null
    : null
  const activeBlockedNotificationSyncPollTarget = useMemo(
    () =>
      getBlockedNotificationSyncPollTarget({
        project: activeProject,
        autonomousUnit: activeAutonomousUnit,
        runtimeStream: activeRuntimeStream,
      }),
    [activeAutonomousUnit, activeProject, activeRuntimeStream],
  )
  const activeBlockedNotificationSyncPollKey = getBlockedNotificationSyncPollKey(
    activeBlockedNotificationSyncPollTarget,
  )

  useEffect(() => {
    blockedNotificationSyncPollTargetRef.current = activeBlockedNotificationSyncPollTarget
  }, [activeBlockedNotificationSyncPollTarget])

  useEffect(() => {
    clearBlockedNotificationSyncPoll()
    blockedNotificationSyncPollInFlightRef.current = false

    if (!activeBlockedNotificationSyncPollKey) {
      return
    }

    scheduleBlockedNotificationSyncPoll(activeBlockedNotificationSyncPollKey)

    return () => {
      clearBlockedNotificationSyncPoll()
    }
  }, [
    activeBlockedNotificationSyncPollKey,
    clearBlockedNotificationSyncPoll,
    scheduleBlockedNotificationSyncPoll,
  ])

  const workflowView = useMemo<WorkflowPaneView | null>(() => {
    if (!activeProject) {
      return null
    }

    const lifecycle = getPlanningLifecycleView(activeProject)
    const selectedProvider = resolveSelectedRuntimeProvider(runtimeSettings, activeRuntimeSession)
    const providerMismatch = hasProviderMismatch(selectedProvider, activeRuntimeSession)

    return {
      project: activeProject,
      activePhase,
      lifecycle,
      activeLifecycleStage: lifecycle.activeStage,
      lifecyclePercent: lifecycle.percentComplete,
      hasLifecycle: lifecycle.hasStages,
      actionRequiredLifecycleCount: lifecycle.actionRequiredCount,
      overallPercent: activeProject.phaseProgressPercent,
      hasPhases: activeProject.phases.length > 0,
      runtimeSession: activeRuntimeSession,
      selectedProviderId: selectedProvider.providerId,
      selectedProviderLabel: selectedProvider.providerLabel,
      selectedModelId: selectedProvider.modelId,
      openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
      providerMismatch,
    }
  }, [activePhase, activeProject, activeRuntimeSession, runtimeSettings])

  const agentView = useMemo<AgentPaneView | null>(() => {
    if (!activeProject) {
      return null
    }

    const notificationRouteViews = mapNotificationRouteViews(
      activeProject.id,
      activeNotificationRoutes,
      activeProject.notificationBroker.dispatches,
    )
    const notificationChannelHealth = mapNotificationChannelHealth(notificationRouteViews)
    const previousTrustSnapshot = trustSnapshotRef.current[activeProject.id] ?? null

    let trustSnapshot: AgentTrustSnapshotView
    try {
      trustSnapshot = composeAgentTrustSnapshot({
        runtimeSession: activeRuntimeSession,
        runtimeRun: activeRuntimeRun,
        runtimeStream: activeRuntimeStream,
        approvalRequests: activeProject.approvalRequests,
        routeViews: notificationRouteViews,
        notificationRouteError: activeNotificationRouteLoadError,
        notificationSyncSummary: activeNotificationSyncSummary,
        notificationSyncError: activeNotificationSyncError,
      })
      trustSnapshotRef.current[activeProject.id] = trustSnapshot
    } catch (error) {
      const projectionError = getOperatorActionError(
        error,
        'Cadence could not compose trust snapshot details from notification/runtime projection data.',
      )
      trustSnapshot = previousTrustSnapshot
        ? {
            ...previousTrustSnapshot,
            routeError: activeNotificationRouteLoadError,
            syncError: activeNotificationSyncError,
            projectionError,
          }
        : createUnavailableTrustSnapshot({
            routeCount: notificationRouteViews.length,
            enabledRouteCount: notificationRouteViews.filter((route) => route.enabled).length,
            pendingApprovalCount: activeProject.pendingApprovalCount,
            notificationRouteError: activeNotificationRouteLoadError,
            notificationSyncError: activeNotificationSyncError,
            projectionError,
          })
      trustSnapshotRef.current[activeProject.id] = trustSnapshot
    }

    const autonomousWorkflowContext = deriveAutonomousWorkflowContext({
      lifecycle: activeProject.lifecycle,
      handoffPackages: activeProject.handoffPackages,
      approvalRequests: activeProject.approvalRequests,
      autonomousUnit: activeAutonomousUnit,
      autonomousAttempt: activeAutonomousAttempt,
    })
    const recentAutonomousUnits = projectRecentAutonomousUnits({
      autonomousHistory: activeAutonomousHistory,
      autonomousRecentArtifacts: activeAutonomousRecentArtifacts,
      lifecycle: activeProject.lifecycle,
      handoffPackages: activeProject.handoffPackages,
      approvalRequests: activeProject.approvalRequests,
    })
    const checkpointControlLoop = projectCheckpointControlLoops({
      actionRequiredItems: activeRuntimeStream?.actionRequired ?? [],
      approvalRequests: activeProject.approvalRequests,
      resumeHistory: activeProject.resumeHistory,
      notificationBroker: activeProject.notificationBroker,
      autonomousHistory: activeAutonomousHistory,
      autonomousRecentArtifacts: activeAutonomousRecentArtifacts,
    })

    const selectedProvider = resolveSelectedRuntimeProvider(runtimeSettings, activeRuntimeSession)
    const providerMismatch = hasProviderMismatch(selectedProvider, activeRuntimeSession)

    return {
      project: activeProject,
      activePhase,
      branchLabel: repositoryStatus?.branchLabel ?? activeProject.branchLabel,
      headShaLabel: repositoryStatus?.headShaLabel ?? activeProject.repository?.headShaLabel ?? 'No HEAD',
      runtimeLabel: activeRuntimeSession?.runtimeLabel ?? activeProject.runtimeLabel,
      repositoryLabel: activeProject.repository?.displayName ?? activeProject.name,
      repositoryPath: activeProject.repository?.rootPath ?? null,
      runtimeSession: activeRuntimeSession,
      selectedProviderId: selectedProvider.providerId,
      selectedProviderLabel: selectedProvider.providerLabel,
      selectedModelId: selectedProvider.modelId,
      openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
      providerMismatch,
      runtimeRun: activeRuntimeRun,
      autonomousRun: activeAutonomousRun,
      autonomousUnit: activeAutonomousUnit,
      autonomousAttempt: activeAutonomousAttempt,
      autonomousWorkflowContext,
      autonomousHistory: activeAutonomousHistory,
      autonomousRecentArtifacts: activeAutonomousRecentArtifacts,
      recentAutonomousUnits,
      checkpointControlLoop,
      runtimeErrorMessage: activeRuntimeErrorMessage,
      runtimeRunErrorMessage: activeRuntimeRunErrorMessage,
      autonomousRunErrorMessage: activeAutonomousRunErrorMessage,
      authPhase: activeRuntimeSession?.phase ?? null,
      authPhaseLabel: activeRuntimeSession?.phaseLabel ?? 'Runtime unavailable',
      runtimeStream: activeRuntimeStream,
      runtimeStreamStatus: activeRuntimeStream?.status ?? 'idle',
      runtimeStreamStatusLabel: getRuntimeStreamStatusLabel(activeRuntimeStream?.status ?? 'idle'),
      runtimeStreamError: activeRuntimeStream?.lastIssue ?? null,
      runtimeStreamItems: activeRuntimeStream?.items ?? [],
      skillItems: activeRuntimeStream?.skillItems ?? [],
      activityItems: activeRuntimeStream?.activityItems ?? [],
      actionRequiredItems: activeRuntimeStream?.actionRequired ?? [],
      notificationBroker: activeProject.notificationBroker,
      notificationRoutes: notificationRouteViews,
      notificationChannelHealth,
      notificationRouteLoadStatus: activeNotificationRouteLoadStatus,
      notificationRouteIsRefreshing:
        activeNotificationRouteLoadStatus === 'loading' && notificationRouteViews.length > 0,
      notificationRouteError: activeNotificationRouteLoadError,
      notificationSyncSummary: activeNotificationSyncSummary,
      notificationSyncError: activeNotificationSyncError,
      notificationSyncPollingActive: Boolean(activeBlockedNotificationSyncPollTarget),
      notificationSyncPollingActionId: activeBlockedNotificationSyncPollTarget?.actionId ?? null,
      notificationSyncPollingBoundaryId: activeBlockedNotificationSyncPollTarget?.boundaryId ?? null,
      notificationRouteMutationStatus,
      pendingNotificationRouteId,
      notificationRouteMutationError,
      trustSnapshot,
      approvalRequests: activeProject.approvalRequests,
      pendingApprovalCount: activeProject.pendingApprovalCount,
      latestDecisionOutcome: activeProject.latestDecisionOutcome,
      resumeHistory: activeProject.resumeHistory,
      operatorActionStatus,
      pendingOperatorActionId,
      operatorActionError,
      autonomousRunActionStatus,
      pendingAutonomousRunAction,
      autonomousRunActionError,
      runtimeRunActionStatus,
      pendingRuntimeRunAction,
      runtimeRunActionError,
      sessionUnavailableReason: getAgentSessionUnavailableReason(
        activeRuntimeSession,
        activeRuntimeErrorMessage,
        selectedProvider,
      ),
      runtimeRunUnavailableReason: getAgentRuntimeRunUnavailableReason(
        activeRuntimeRun,
        activeRuntimeRunErrorMessage,
        activeRuntimeSession,
        selectedProvider,
      ),
      messagesUnavailableReason: getAgentMessagesUnavailableReason(
        activeRuntimeSession,
        activeRuntimeStream,
        activeRuntimeRun,
        selectedProvider,
      ),
    }
  }, [
    activeNotificationRouteLoadError,
    activeNotificationRouteLoadStatus,
    activeNotificationRoutes,
    activeNotificationSyncError,
    activeNotificationSyncSummary,
    activePhase,
    activeProject,
    activeAutonomousAttempt,
    activeAutonomousHistory,
    activeAutonomousRecentArtifacts,
    activeAutonomousRun,
    activeAutonomousRunErrorMessage,
    activeAutonomousUnit,
    activeRuntimeErrorMessage,
    activeRuntimeRun,
    activeRuntimeRunErrorMessage,
    activeRuntimeSession,
    activeRuntimeStream,
    notificationRouteMutationError,
    notificationRouteMutationStatus,
    operatorActionError,
    operatorActionStatus,
    pendingAutonomousRunAction,
    pendingNotificationRouteId,
    pendingOperatorActionId,
    pendingRuntimeRunAction,
    repositoryStatus,
    autonomousRunActionError,
    autonomousRunActionStatus,
    runtimeRunActionError,
    runtimeRunActionStatus,
    runtimeSettings,
  ])

  const executionView = useMemo<ExecutionPaneView | null>(() => {
    if (!activeProject) {
      return null
    }

    const statusEntries = repositoryStatus?.entries ?? []
    const diffScopes: DiffScopeSummary[] = [
      {
        scope: 'staged',
        label: REPOSITORY_DIFF_SCOPE_LABELS.staged,
        count: repositoryStatus?.stagedCount ?? 0,
      },
      {
        scope: 'unstaged',
        label: REPOSITORY_DIFF_SCOPE_LABELS.unstaged,
        count: repositoryStatus?.unstagedCount ?? 0,
      },
      {
        scope: 'worktree',
        label: REPOSITORY_DIFF_SCOPE_LABELS.worktree,
        count: repositoryStatus?.statusCount ?? 0,
      },
    ]

    return {
      project: activeProject,
      activePhase,
      branchLabel: repositoryStatus?.branchLabel ?? activeProject.branchLabel,
      headShaLabel: repositoryStatus?.headShaLabel ?? activeProject.repository?.headShaLabel ?? 'No HEAD',
      statusEntries,
      statusCount: repositoryStatus?.statusCount ?? 0,
      hasChanges: repositoryStatus?.hasChanges ?? false,
      diffScopes,
      verificationRecords: activeProject.verificationRecords,
      resumeHistory: activeProject.resumeHistory,
      latestDecisionOutcome: activeProject.latestDecisionOutcome,
      notificationBroker: activeProject.notificationBroker,
      operatorActionError,
      verificationUnavailableReason:
        activeProject.verificationRecords.length > 0 || activeProject.resumeHistory.length > 0
          ? 'Durable operator verification and resume history are loaded from the selected project snapshot.'
          : 'Verification details will appear here once the backend exposes run and wave results.',
    }
  }, [activePhase, activeProject, operatorActionError, repositoryStatus])

  return {
    projects,
    activeProject,
    activeProjectId,
    repositoryStatus,
    workflowView,
    agentView,
    executionView,
    repositoryDiffs,
    activeDiffScope,
    activeRepositoryDiff: repositoryDiffs[activeDiffScope],
    isLoading,
    isProjectLoading,
    isImporting,
    projectRemovalStatus,
    pendingProjectRemovalId,
    errorMessage,
    runtimeSettings,
    runtimeSettingsLoadStatus,
    runtimeSettingsLoadError,
    runtimeSettingsSaveStatus,
    runtimeSettingsSaveError,
    refreshSource,
    isDesktopRuntime: adapter.isDesktopRuntime(),
    operatorActionStatus,
    pendingOperatorActionId,
    operatorActionError,
    autonomousRunActionStatus,
    pendingAutonomousRunAction,
    autonomousRunActionError,
    runtimeRunActionStatus,
    pendingRuntimeRunAction,
    runtimeRunActionError,
    selectProject,
    importProject,
    removeProject,
    retry,
    showRepositoryDiff,
    retryActiveRepositoryDiff,
    listProjectFiles,
    readProjectFile,
    writeProjectFile,
    createProjectEntry,
    renameProjectEntry,
    deleteProjectEntry,
    startOpenAiLogin,
    submitOpenAiCallback,
    startAutonomousRun,
    inspectAutonomousRun,
    cancelAutonomousRun,
    startRuntimeRun,
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    refreshRuntimeSettings,
    upsertRuntimeSettings,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  }
}
