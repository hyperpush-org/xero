import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import {
  CadenceDesktopError,
  cadenceDesktopAdapter,
  getDesktopErrorMessage,
  type CadenceDesktopAdapter,
} from '@/src/lib/cadence-desktop'
import {
  applyRepositoryStatus,
  applyRuntimeRun,
  applyRuntimeSession,
  applyRuntimeStreamIssue,
  createEmptyPlanningLifecycle,
  createRuntimeStreamFromSubscription,
  createRuntimeStreamView,
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
  type RepositoryDiffScope,
  type RepositoryDiffView,
  type RepositoryStatusEntryView,
  type RepositoryStatusView,
  type ResumeHistoryEntryView,
  type RuntimeAuthPhaseDto,
  type RuntimeRunView,
  type RuntimeSessionView,
  type RuntimeStreamActionRequiredItemView,
  type RuntimeStreamActivityItemView,
  type RuntimeStreamIssueView,
  type RuntimeStreamItemKindDto,
  type RuntimeStreamStatus,
  type RuntimeStreamView,
  type RuntimeStreamViewItem,
  type SyncNotificationAdaptersResponseDto,
  type UpsertNotificationRouteRequestDto,
  type VerificationRecordView,
} from '@/src/lib/cadence-model'

export type RefreshSource =
  | 'startup'
  | 'selection'
  | 'import'
  | 'project:updated'
  | 'repository:status_changed'
  | 'runtime:updated'
  | 'runtime_run:updated'
  | 'runtime_stream:action_required'
  | 'operator:resolve'
  | 'operator:resume'
  | null

export type OperatorActionDecision = 'approve' | 'reject'
export type OperatorActionStatus = 'idle' | 'running'
export type RuntimeRunActionKind = 'start' | 'stop'
export type RuntimeRunActionStatus = 'idle' | 'running'

export interface OperatorActionErrorView {
  code: string
  message: string
  retryable: boolean
}

export type RepositoryDiffLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type NotificationRoutesLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type NotificationRouteMutationStatus = 'idle' | 'running'
export type NotificationRouteHealthState = 'disabled' | 'idle' | 'pending' | 'healthy' | 'degraded'
export type AgentTrustSignalState = 'healthy' | 'degraded' | 'unavailable'

export interface AgentTrustSnapshotView {
  state: AgentTrustSignalState
  stateLabel: string
  runtimeState: AgentTrustSignalState
  runtimeReason: string
  streamState: AgentTrustSignalState
  streamReason: string
  approvalsState: AgentTrustSignalState
  approvalsReason: string
  routesState: AgentTrustSignalState
  routesReason: string
  credentialsState: AgentTrustSignalState
  credentialsReason: string
  syncState: AgentTrustSignalState
  syncReason: string
  routeCount: number
  enabledRouteCount: number
  degradedRouteCount: number
  readyCredentialRouteCount: number
  missingCredentialRouteCount: number
  malformedCredentialRouteCount: number
  unavailableCredentialRouteCount: number
  pendingApprovalCount: number
  syncDispatchFailedCount: number
  syncReplyRejectedCount: number
  routeError: OperatorActionErrorView | null
  syncError: OperatorActionErrorView | null
  projectionError: OperatorActionErrorView | null
}

export interface NotificationRouteHealthView {
  projectId: string
  routeId: string
  routeKind: NotificationRouteKindDto
  routeKindLabel: string
  routeTarget: string
  enabled: boolean
  metadataJson: string | null
  credentialReadiness?: NotificationRouteCredentialReadinessDto | null
  credentialDiagnosticCode?: string | null
  createdAt: string
  updatedAt: string
  dispatchCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestDispatchAt: string | null
  latestFailureCode: string | null
  latestFailureMessage: string | null
  health: NotificationRouteHealthState
  healthLabel: string
}

export interface NotificationChannelHealthView {
  routeKind: NotificationRouteKindDto
  routeKindLabel: string
  routeCount: number
  enabledCount: number
  disabledCount: number
  dispatchCount: number
  pendingCount: number
  sentCount: number
  failedCount: number
  claimedCount: number
  latestDispatchAt: string | null
  health: NotificationRouteHealthState
  healthLabel: string
}

export interface UseCadenceDesktopStateOptions {
  adapter?: CadenceDesktopAdapter
}

export interface RepositoryDiffState {
  status: RepositoryDiffLoadStatus
  diff: RepositoryDiffView | null
  errorMessage: string | null
  projectId: string | null
}

export interface DiffScopeSummary {
  scope: RepositoryDiffScope
  label: string
  count: number
}

export interface WorkflowPaneView {
  project: ProjectDetailView
  activePhase: Phase | null
  lifecycle: PlanningLifecycleView
  activeLifecycleStage: PlanningLifecycleStageView | null
  lifecyclePercent: number
  hasLifecycle: boolean
  actionRequiredLifecycleCount: number
  overallPercent: number
  hasPhases: boolean
}

export interface AgentPaneView {
  project: ProjectDetailView
  activePhase: Phase | null
  branchLabel: string
  headShaLabel: string
  runtimeLabel: string
  repositoryLabel: string
  repositoryPath: string | null
  runtimeSession?: RuntimeSessionView | null
  runtimeRun?: RuntimeRunView | null
  runtimeErrorMessage?: string | null
  runtimeRunErrorMessage?: string | null
  authPhase?: RuntimeAuthPhaseDto | null
  authPhaseLabel?: string
  runtimeStream?: RuntimeStreamView | null
  runtimeStreamStatus?: RuntimeStreamStatus
  runtimeStreamStatusLabel?: string
  runtimeStreamError?: RuntimeStreamIssueView | null
  runtimeStreamItems?: RuntimeStreamViewItem[]
  activityItems?: RuntimeStreamActivityItemView[]
  actionRequiredItems?: RuntimeStreamActionRequiredItemView[]
  notificationBroker: ProjectDetailView['notificationBroker']
  notificationRoutes: NotificationRouteHealthView[]
  notificationChannelHealth: NotificationChannelHealthView[]
  notificationRouteLoadStatus: NotificationRoutesLoadStatus
  notificationRouteIsRefreshing: boolean
  notificationRouteError: OperatorActionErrorView | null
  notificationSyncSummary: SyncNotificationAdaptersResponseDto | null
  notificationSyncError: OperatorActionErrorView | null
  notificationRouteMutationStatus: NotificationRouteMutationStatus
  pendingNotificationRouteId: string | null
  notificationRouteMutationError: OperatorActionErrorView | null
  trustSnapshot?: AgentTrustSnapshotView
  approvalRequests: OperatorApprovalView[]
  pendingApprovalCount: number
  latestDecisionOutcome: ProjectDetailView['latestDecisionOutcome']
  resumeHistory: ResumeHistoryEntryView[]
  operatorActionStatus: OperatorActionStatus
  pendingOperatorActionId: string | null
  operatorActionError: OperatorActionErrorView | null
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
  sessionUnavailableReason: string
  runtimeRunUnavailableReason: string
  messagesUnavailableReason: string
}

export interface ExecutionPaneView {
  project: ProjectDetailView
  activePhase: Phase | null
  branchLabel: string
  headShaLabel: string
  statusEntries: RepositoryStatusEntryView[]
  statusCount: number
  hasChanges: boolean
  diffScopes: DiffScopeSummary[]
  verificationRecords: VerificationRecordView[]
  resumeHistory: ResumeHistoryEntryView[]
  latestDecisionOutcome: ProjectDetailView['latestDecisionOutcome']
  notificationBroker: ProjectDetailView['notificationBroker']
  operatorActionError: OperatorActionErrorView | null
  verificationUnavailableReason: string
}

export interface UseCadenceDesktopStateResult {
  projects: ProjectListItem[]
  activeProject: ProjectDetailView | null
  activeProjectId: string | null
  repositoryStatus: RepositoryStatusView | null
  workflowView: WorkflowPaneView | null
  agentView: AgentPaneView | null
  executionView: ExecutionPaneView | null
  repositoryDiffs: Record<RepositoryDiffScope, RepositoryDiffState>
  activeDiffScope: RepositoryDiffScope
  activeRepositoryDiff: RepositoryDiffState
  isLoading: boolean
  isProjectLoading: boolean
  isImporting: boolean
  errorMessage: string | null
  refreshSource: RefreshSource
  isDesktopRuntime: boolean
  operatorActionStatus: OperatorActionStatus
  pendingOperatorActionId: string | null
  operatorActionError: OperatorActionErrorView | null
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
  selectProject: (projectId: string) => Promise<void>
  importProject: () => Promise<void>
  retry: () => Promise<void>
  showRepositoryDiff: (scope: RepositoryDiffScope, options?: { force?: boolean }) => Promise<void>
  retryActiveRepositoryDiff: () => Promise<void>
  startOpenAiLogin: () => Promise<RuntimeSessionView | null>
  submitOpenAiCallback: (flowId: string, options?: { manualInput?: string | null }) => Promise<RuntimeSessionView | null>
  startRuntimeRun: () => Promise<RuntimeRunView | null>
  startRuntimeSession: () => Promise<RuntimeSessionView | null>
  stopRuntimeRun: (runId: string) => Promise<RuntimeRunView | null>
  logoutRuntimeSession: () => Promise<RuntimeSessionView | null>
  resolveOperatorAction: (
    actionId: string,
    decision: OperatorActionDecision,
    options?: { userAnswer?: string | null },
  ) => Promise<ProjectDetailView | null>
  resumeOperatorRun: (
    actionId: string,
    options?: { userAnswer?: string | null },
  ) => Promise<ProjectDetailView | null>
  refreshNotificationRoutes: (options?: { force?: boolean }) => Promise<NotificationRouteDto[]>
  upsertNotificationRoute: (
    request: Omit<UpsertNotificationRouteRequestDto, 'projectId'>,
  ) => Promise<NotificationRouteDto | null>
}

const REPOSITORY_DIFF_SCOPE_LABELS: Record<RepositoryDiffScope, string> = {
  staged: 'Staged',
  unstaged: 'Unstaged',
  worktree: 'Worktree',
}

const ACTIVE_RUNTIME_STREAM_ITEM_KINDS: RuntimeStreamItemKindDto[] = [
  'transcript',
  'tool',
  'activity',
  'action_required',
  'complete',
  'failure',
]

const NOTIFICATION_ROUTE_KINDS: NotificationRouteKindDto[] = ['telegram', 'discord']

const NOTIFICATION_ROUTE_KIND_LABELS: Record<NotificationRouteKindDto, string> = {
  telegram: 'Telegram',
  discord: 'Discord',
}

function getNotificationRouteKindLabel(routeKind: NotificationRouteKindDto): string {
  return NOTIFICATION_ROUTE_KIND_LABELS[routeKind]
}

function getTimestampMs(value: string | null | undefined): number {
  if (typeof value !== 'string' || value.trim().length === 0) {
    return 0
  }

  const parsed = Date.parse(value)
  return Number.isFinite(parsed) ? parsed : 0
}

function getNotificationHealthLabel(state: NotificationRouteHealthState): string {
  switch (state) {
    case 'disabled':
      return 'Disabled'
    case 'idle':
      return 'Idle'
    case 'pending':
      return 'Pending delivery'
    case 'healthy':
      return 'Healthy'
    case 'degraded':
      return 'Needs attention'
  }
}

function getNotificationHealthState(options: {
  enabled: boolean
  failedCount: number
  pendingCount: number
  sentCount: number
  claimedCount: number
}): NotificationRouteHealthState {
  if (!options.enabled) {
    return 'disabled'
  }

  if (options.failedCount > 0) {
    return 'degraded'
  }

  if (options.pendingCount > 0) {
    return 'pending'
  }

  if (options.sentCount > 0 || options.claimedCount > 0) {
    return 'healthy'
  }

  return 'idle'
}

function summarizeRouteDispatches(dispatches: NotificationDispatchView[]) {
  let pendingCount = 0
  let sentCount = 0
  let failedCount = 0
  let claimedCount = 0
  let latestDispatchAt: string | null = null
  let latestFailureCode: string | null = null
  let latestFailureMessage: string | null = null
  let latestFailureAt = 0

  for (const dispatch of dispatches) {
    if (dispatch.isPending) {
      pendingCount += 1
    } else if (dispatch.isSent) {
      sentCount += 1
    } else if (dispatch.isFailed) {
      failedCount += 1
    } else if (dispatch.isClaimed) {
      claimedCount += 1
    }

    const dispatchUpdatedAt = dispatch.updatedAt ?? dispatch.lastAttemptAt ?? dispatch.createdAt
    if (getTimestampMs(dispatchUpdatedAt) >= getTimestampMs(latestDispatchAt)) {
      latestDispatchAt = dispatchUpdatedAt
    }

    if (!dispatch.isFailed) {
      continue
    }

    const failureTimestamp = getTimestampMs(dispatch.updatedAt ?? dispatch.lastAttemptAt ?? dispatch.createdAt)
    if (failureTimestamp < latestFailureAt) {
      continue
    }

    latestFailureAt = failureTimestamp
    latestFailureCode = dispatch.lastErrorCode
    latestFailureMessage = dispatch.lastErrorMessage
  }

  return {
    dispatchCount: dispatches.length,
    pendingCount,
    sentCount,
    failedCount,
    claimedCount,
    latestDispatchAt,
    latestFailureCode,
    latestFailureMessage,
  }
}

function mapNotificationRouteViews(
  projectId: string,
  routes: NotificationRouteDto[],
  dispatches: NotificationDispatchView[],
): NotificationRouteHealthView[] {
  const dispatchesByRouteId = new Map<string, NotificationDispatchView[]>()

  for (const dispatch of dispatches) {
    const routeId = dispatch.routeId?.trim()
    if (!routeId) {
      continue
    }

    const existingDispatches = dispatchesByRouteId.get(routeId)
    if (existingDispatches) {
      existingDispatches.push(dispatch)
      continue
    }

    dispatchesByRouteId.set(routeId, [dispatch])
  }

  const sortedRoutes = [...routes]
    .filter((route) => route.projectId === projectId && route.routeId.trim().length > 0)
    .sort((left, right) => {
      if (left.routeKind !== right.routeKind) {
        return left.routeKind.localeCompare(right.routeKind)
      }

      return left.routeId.localeCompare(right.routeId)
    })

  return sortedRoutes.map((route) => {
    const routeDispatches = dispatchesByRouteId.get(route.routeId) ?? []
    const summary = summarizeRouteDispatches(routeDispatches)
    const health = getNotificationHealthState({
      enabled: route.enabled,
      failedCount: summary.failedCount,
      pendingCount: summary.pendingCount,
      sentCount: summary.sentCount,
      claimedCount: summary.claimedCount,
    })

    return {
      projectId: route.projectId,
      routeId: route.routeId,
      routeKind: route.routeKind,
      routeKindLabel: getNotificationRouteKindLabel(route.routeKind),
      routeTarget: route.routeTarget,
      enabled: route.enabled,
      metadataJson: route.metadataJson ?? null,
      credentialReadiness: route.credentialReadiness ?? null,
      credentialDiagnosticCode: route.credentialReadiness?.diagnostic?.code ?? null,
      createdAt: route.createdAt,
      updatedAt: route.updatedAt,
      dispatchCount: summary.dispatchCount,
      pendingCount: summary.pendingCount,
      sentCount: summary.sentCount,
      failedCount: summary.failedCount,
      claimedCount: summary.claimedCount,
      latestDispatchAt: summary.latestDispatchAt,
      latestFailureCode: summary.latestFailureCode,
      latestFailureMessage: summary.latestFailureMessage,
      health,
      healthLabel: getNotificationHealthLabel(health),
    }
  })
}

function mapNotificationChannelHealth(routeViews: NotificationRouteHealthView[]): NotificationChannelHealthView[] {
  return NOTIFICATION_ROUTE_KINDS.map((routeKind) => {
    const channelRoutes = routeViews.filter((route) => route.routeKind === routeKind)
    const dispatchCount = channelRoutes.reduce((total, route) => total + route.dispatchCount, 0)
    const pendingCount = channelRoutes.reduce((total, route) => total + route.pendingCount, 0)
    const sentCount = channelRoutes.reduce((total, route) => total + route.sentCount, 0)
    const failedCount = channelRoutes.reduce((total, route) => total + route.failedCount, 0)
    const claimedCount = channelRoutes.reduce((total, route) => total + route.claimedCount, 0)
    const enabledCount = channelRoutes.filter((route) => route.enabled).length
    const disabledCount = channelRoutes.length - enabledCount
    const latestDispatchAt = channelRoutes.reduce<string | null>((latest, route) => {
      if (getTimestampMs(route.latestDispatchAt) >= getTimestampMs(latest)) {
        return route.latestDispatchAt
      }

      return latest
    }, null)

    const health = getNotificationHealthState({
      enabled: enabledCount > 0,
      failedCount,
      pendingCount,
      sentCount,
      claimedCount,
    })

    return {
      routeKind,
      routeKindLabel: getNotificationRouteKindLabel(routeKind),
      routeCount: channelRoutes.length,
      enabledCount,
      disabledCount,
      dispatchCount,
      pendingCount,
      sentCount,
      failedCount,
      claimedCount,
      latestDispatchAt,
      health,
      healthLabel: getNotificationHealthLabel(health),
    }
  })
}

function getTrustSignalLabel(state: AgentTrustSignalState): string {
  switch (state) {
    case 'healthy':
      return 'Healthy'
    case 'degraded':
      return 'Needs attention'
    case 'unavailable':
      return 'Unavailable'
  }
}

function parseCredentialReadiness(route: NotificationRouteHealthView): NotificationRouteCredentialReadinessDto {
  const parsedReadiness = notificationRouteCredentialReadinessSchema.safeParse(route.credentialReadiness)
  if (!parsedReadiness.success) {
    throw new Error(
      `Cadence could not compose trust snapshot because route \`${route.routeId}\` readiness metadata was malformed: ${parsedReadiness.error.message}`,
    )
  }

  return parsedReadiness.data
}

function composeAgentTrustSnapshot(options: {
  runtimeSession: RuntimeSessionView | null
  runtimeRun: RuntimeRunView | null
  runtimeStream: RuntimeStreamView | null
  approvalRequests: OperatorApprovalView[]
  routeViews: NotificationRouteHealthView[]
  notificationRouteError: OperatorActionErrorView | null
  notificationSyncSummary: SyncNotificationAdaptersResponseDto | null
  notificationSyncError: OperatorActionErrorView | null
}): AgentTrustSnapshotView {
  const pendingApprovalCount = options.approvalRequests.filter((approval) => approval.isPending).length

  const runtimeState: AgentTrustSignalState = !options.runtimeRun
    ? 'unavailable'
    : options.runtimeRun.isFailed || options.runtimeRun.isStale
      ? 'degraded'
      : options.runtimeRun.isActive
        ? options.runtimeSession?.isAuthenticated
          ? 'healthy'
          : 'degraded'
        : 'unavailable'
  const runtimeReason = !options.runtimeRun
    ? 'No durable runtime-run record is available for the selected project.'
    : options.runtimeRun.isFailed
      ? 'The durable runtime-run record indicates the supervisor failed.'
      : options.runtimeRun.isStale
        ? 'The durable runtime-run record is stale and needs operator review.'
        : options.runtimeRun.isActive && options.runtimeSession?.isAuthenticated
          ? 'Durable runtime run + authenticated session are both healthy.'
          : options.runtimeRun.isActive
            ? 'Durable runtime run exists, but desktop runtime authentication is not currently healthy.'
            : 'The durable runtime-run record is terminal and no active run is available.'

  const streamState: AgentTrustSignalState = !options.runtimeStream
    ? 'unavailable'
    : options.runtimeStream.status === 'error' || options.runtimeStream.status === 'stale'
      ? 'degraded'
      : options.runtimeStream.status === 'live' || options.runtimeStream.status === 'complete'
        ? 'healthy'
        : 'unavailable'
  const streamReason = !options.runtimeStream
    ? 'No runtime event stream is attached to the selected project.'
    : options.runtimeStream.status === 'error' || options.runtimeStream.status === 'stale'
      ? options.runtimeStream.lastIssue?.message ?? 'Runtime event stream reported a degraded state.'
      : options.runtimeStream.status === 'live' || options.runtimeStream.status === 'complete'
        ? 'Runtime event stream is connected and delivering operator-visible activity.'
        : 'Runtime event stream is not yet live.'

  const approvalsState: AgentTrustSignalState = pendingApprovalCount > 0 ? 'degraded' : 'healthy'
  const approvalsReason = pendingApprovalCount > 0
    ? `There are ${pendingApprovalCount} pending operator approval gate(s) waiting for action.`
    : 'No pending operator approvals are blocking autonomous continuation.'

  const enabledRoutes = options.routeViews.filter((route) => route.enabled)
  const degradedRouteCount = options.routeViews.filter(
    (route) => route.health === 'degraded' || route.health === 'pending',
  ).length

  const routesState: AgentTrustSignalState = options.routeViews.length === 0
    ? options.notificationRouteError
      ? 'degraded'
      : 'unavailable'
    : degradedRouteCount > 0 || options.notificationRouteError
      ? 'degraded'
      : 'healthy'
  const routesReason = options.notificationRouteError
    ? options.notificationRouteError.message
    : options.routeViews.length === 0
      ? 'No notification routes are configured for the selected project.'
      : degradedRouteCount > 0
        ? `${degradedRouteCount} route(s) show degraded or pending dispatch health.`
        : 'Notification route health is stable for configured channels.'

  let readyCredentialRouteCount = 0
  let missingCredentialRouteCount = 0
  let malformedCredentialRouteCount = 0
  let unavailableCredentialRouteCount = 0

  for (const route of enabledRoutes) {
    const readiness = parseCredentialReadiness(route)
    switch (readiness.status) {
      case 'ready':
        readyCredentialRouteCount += 1
        break
      case 'missing':
        missingCredentialRouteCount += 1
        break
      case 'malformed':
        malformedCredentialRouteCount += 1
        break
      case 'unavailable':
        unavailableCredentialRouteCount += 1
        break
    }
  }

  const credentialsState: AgentTrustSignalState = enabledRoutes.length === 0
    ? 'unavailable'
    : malformedCredentialRouteCount > 0 || unavailableCredentialRouteCount > 0 || missingCredentialRouteCount > 0
      ? 'degraded'
      : 'healthy'
  const credentialsReason = enabledRoutes.length === 0
    ? 'No enabled routes require app-local credential readiness checks.'
    : malformedCredentialRouteCount > 0
      ? `${malformedCredentialRouteCount} enabled route(s) have malformed app-local credential state.`
      : unavailableCredentialRouteCount > 0
        ? `${unavailableCredentialRouteCount} enabled route(s) could not read app-local credential state.`
        : missingCredentialRouteCount > 0
          ? `${missingCredentialRouteCount} enabled route(s) are missing required app-local credentials.`
          : 'All enabled routes report fully configured app-local credentials.'

  const syncDispatchFailedCount = options.notificationSyncSummary?.dispatch.failedCount ?? 0
  const syncReplyRejectedCount = options.notificationSyncSummary?.replies.rejectedCount ?? 0

  const syncState: AgentTrustSignalState = options.notificationSyncError
    ? 'degraded'
    : !options.notificationSyncSummary
      ? 'unavailable'
      : syncDispatchFailedCount > 0 || syncReplyRejectedCount > 0
        ? 'degraded'
        : 'healthy'
  const syncReason = options.notificationSyncError
    ? options.notificationSyncError.message
    : !options.notificationSyncSummary
      ? 'No notification adapter sync summary is available yet.'
      : syncDispatchFailedCount > 0 || syncReplyRejectedCount > 0
        ? `Latest sync cycle reported ${syncDispatchFailedCount} failed dispatch(es) and ${syncReplyRejectedCount} rejected repl${syncReplyRejectedCount === 1 ? 'y' : 'ies'}.`
        : 'Latest notification adapter sync cycle completed without failed dispatches or rejected replies.'

  const signalStates = [runtimeState, streamState, approvalsState, routesState, credentialsState, syncState]
  const state: AgentTrustSignalState = signalStates.includes('degraded')
    ? 'degraded'
    : signalStates.every((value) => value === 'healthy')
      ? 'healthy'
      : 'unavailable'

  return {
    state,
    stateLabel: getTrustSignalLabel(state),
    runtimeState,
    runtimeReason,
    streamState,
    streamReason,
    approvalsState,
    approvalsReason,
    routesState,
    routesReason,
    credentialsState,
    credentialsReason,
    syncState,
    syncReason,
    routeCount: options.routeViews.length,
    enabledRouteCount: enabledRoutes.length,
    degradedRouteCount,
    readyCredentialRouteCount,
    missingCredentialRouteCount,
    malformedCredentialRouteCount,
    unavailableCredentialRouteCount,
    pendingApprovalCount,
    syncDispatchFailedCount,
    syncReplyRejectedCount,
    routeError: options.notificationRouteError,
    syncError: options.notificationSyncError,
    projectionError: null,
  }
}

function createUnavailableTrustSnapshot(
  options: {
    routeCount: number
    enabledRouteCount: number
    pendingApprovalCount: number
    notificationRouteError: OperatorActionErrorView | null
    notificationSyncError: OperatorActionErrorView | null
    projectionError: OperatorActionErrorView | null
  },
): AgentTrustSnapshotView {
  return {
    state: 'unavailable',
    stateLabel: getTrustSignalLabel('unavailable'),
    runtimeState: 'unavailable',
    runtimeReason: 'Trust projection is unavailable because prerequisite runtime metadata is missing.',
    streamState: 'unavailable',
    streamReason: 'Trust projection is unavailable because runtime-stream metadata is missing.',
    approvalsState: options.pendingApprovalCount > 0 ? 'degraded' : 'healthy',
    approvalsReason:
      options.pendingApprovalCount > 0
        ? `There are ${options.pendingApprovalCount} pending operator approval gate(s) waiting for action.`
        : 'No pending operator approvals are blocking autonomous continuation.',
    routesState: options.routeCount > 0 ? 'degraded' : 'unavailable',
    routesReason:
      options.routeCount > 0
        ? 'Trust projection kept the last-known-good route state after a transient projection failure.'
        : 'No notification routes are configured for the selected project.',
    credentialsState: options.enabledRouteCount > 0 ? 'degraded' : 'unavailable',
    credentialsReason:
      options.enabledRouteCount > 0
        ? 'Trust projection kept the last-known-good credential-readiness state after a transient projection failure.'
        : 'No enabled routes require app-local credential readiness checks.',
    syncState: options.notificationSyncError ? 'degraded' : 'unavailable',
    syncReason:
      options.notificationSyncError?.message ??
      'No notification adapter sync summary is available while trust projection is unavailable.',
    routeCount: options.routeCount,
    enabledRouteCount: options.enabledRouteCount,
    degradedRouteCount: options.routeCount,
    readyCredentialRouteCount: 0,
    missingCredentialRouteCount: 0,
    malformedCredentialRouteCount: 0,
    unavailableCredentialRouteCount: options.enabledRouteCount,
    pendingApprovalCount: options.pendingApprovalCount,
    syncDispatchFailedCount: 0,
    syncReplyRejectedCount: 0,
    routeError: options.notificationRouteError,
    syncError: options.notificationSyncError,
    projectionError: options.projectionError,
  }
}

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

function getAgentSessionUnavailableReason(
  runtimeSession: RuntimeSessionView | null,
  runtimeErrorMessage: string | null,
): string {
  if (runtimeErrorMessage) {
    return runtimeErrorMessage
  }

  if (!runtimeSession) {
    return 'Live runtime session metadata is not available from the desktop backend yet.'
  }

  if (runtimeSession.lastError?.message) {
    return runtimeSession.lastError.message
  }

  switch (runtimeSession.phase) {
    case 'authenticated':
      return runtimeSession.sessionId
        ? `Cadence is authenticated as ${runtimeSession.accountLabel} and bound to session ${runtimeSession.sessionLabel}.`
        : `Cadence is authenticated as ${runtimeSession.accountLabel}.`
    case 'awaiting_browser_callback':
      return 'Cadence started the OpenAI login flow and is waiting for the browser callback to return.'
    case 'awaiting_manual_input':
      return 'Cadence is waiting for the pasted OpenAI redirect URL to finish login for this project.'
    case 'starting':
      return 'Cadence is opening the OpenAI login flow for this project.'
    case 'exchanging_code':
      return 'Cadence is exchanging the OpenAI authorization code for a project-bound session.'
    case 'refreshing':
      return 'Cadence is refreshing the stored OpenAI auth session for this project.'
    case 'idle':
      return 'Sign in with OpenAI to create or reuse a runtime session for this imported project.'
    case 'cancelled':
      return 'The OpenAI login flow was cancelled before Cadence could create a runtime session.'
    case 'failed':
      return 'Cadence could not create a runtime session for this project.'
  }
}

function getAgentRuntimeRunUnavailableReason(
  runtimeRun: RuntimeRunView | null,
  runtimeRunErrorMessage: string | null,
  runtimeSession: RuntimeSessionView | null,
): string {
  if (runtimeRunErrorMessage) {
    return runtimeRunErrorMessage
  }

  if (!runtimeRun) {
    if (runtimeSession?.isAuthenticated) {
      return 'No durable supervised runtime run is recorded for this project yet.'
    }

    return 'Authenticate and launch a supervised harness run to populate durable repo-local run state for this project.'
  }

  if (runtimeRun.lastError?.message) {
    return runtimeRun.lastError.message
  }

  if (runtimeRun.isFailed) {
    return 'Cadence recovered a failed supervised harness run. Inspect the final checkpoint and error details before retrying.'
  }

  if (runtimeRun.isStale) {
    return 'Cadence recovered a stale supervised harness run. The durable checkpoint trail is still available even though the control endpoint is no longer reachable.'
  }

  if (runtimeRun.isTerminal) {
    return 'Cadence recovered a stopped supervised harness run. Final checkpoints remain available for inspection after reload.'
  }

  return 'Cadence recovered a supervised harness run and its durable checkpoints before the live runtime feed resumed.'
}

function getAgentMessagesUnavailableReason(
  runtimeSession: RuntimeSessionView | null,
  runtimeStream: RuntimeStreamView | null,
  runtimeRun: RuntimeRunView | null,
): string {
  if (!runtimeSession) {
    return runtimeRun
      ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires a desktop-authenticated runtime session.'
      : 'Sign in with OpenAI to establish a runtime session for this imported project.'
  }

  if (!runtimeSession.isAuthenticated) {
    if (runtimeSession.isLoginInProgress) {
      return 'Finish the OpenAI login flow to establish the runtime session for this imported project.'
    }

    return runtimeRun
      ? 'Cadence recovered durable supervised-run state for this project, but live streaming still requires an authenticated runtime session.'
      : 'Sign in with OpenAI to establish a runtime session for this imported project.'
  }

  if (!runtimeStream) {
    return runtimeRun?.hasCheckpoints
      ? 'Cadence recovered a supervised harness run, but the live runtime stream has not resumed yet. Durable checkpoints remain visible below.'
      : 'Cadence authenticated this project, but the live runtime stream has not started yet.'
  }

  if (runtimeStream.lastIssue?.message) {
    return runtimeStream.lastIssue.message
  }

  const latestActionRequired = runtimeStream.actionRequired[runtimeStream.actionRequired.length - 1] ?? null
  if (latestActionRequired) {
    return `${latestActionRequired.title}: ${latestActionRequired.detail}`
  }

  if (runtimeStream.status === 'subscribing') {
    return runtimeRun?.hasCheckpoints
      ? 'Cadence is reconnecting the live runtime stream while keeping durable checkpoints visible for this selected project.'
      : 'Cadence is connecting the live runtime stream for this selected project.'
  }

  if (runtimeStream.status === 'replaying') {
    return 'Cadence is replaying recent run-scoped activity while the live runtime stream catches up for this selected project.'
  }

  if (runtimeStream.status === 'complete') {
    return runtimeStream.completion?.detail ?? 'Cadence completed the current runtime bootstrap stream for this project.'
  }

  if (runtimeStream.status === 'stale') {
    return 'Cadence marked the runtime stream as stale. Retry or reselect the project to resubscribe.'
  }

  if (runtimeStream.status === 'error') {
    return runtimeStream.failure?.message ?? 'Cadence could not keep the runtime stream connected for this project.'
  }

  return `Live runtime activity is streaming for this project (${runtimeStream.items.length} item${runtimeStream.items.length === 1 ? '' : 's'} captured).`
}

export function useCadenceDesktopState(
  options: UseCadenceDesktopStateOptions = {},
): UseCadenceDesktopStateResult {
  const adapter = options.adapter ?? cadenceDesktopAdapter
  const [projects, setProjects] = useState<ProjectListItem[]>([])
  const [activeProject, setActiveProject] = useState<ProjectDetailView | null>(null)
  const [activeProjectId, setActiveProjectId] = useState<string | null>(null)
  const [repositoryStatus, setRepositoryStatus] = useState<RepositoryStatusView | null>(null)
  const [repositoryDiffs, setRepositoryDiffs] = useState<Record<RepositoryDiffScope, RepositoryDiffState>>(
    createInitialRepositoryDiffs,
  )
  const [runtimeSessions, setRuntimeSessions] = useState<Record<string, RuntimeSessionView>>({})
  const [runtimeRuns, setRuntimeRuns] = useState<Record<string, RuntimeRunView>>({})
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
  const [activeDiffScope, setActiveDiffScope] = useState<RepositoryDiffScope>('unstaged')
  const [isLoading, setIsLoading] = useState(true)
  const [isProjectLoading, setIsProjectLoading] = useState(false)
  const [isImporting, setIsImporting] = useState(false)
  const [operatorActionStatus, setOperatorActionStatus] = useState<OperatorActionStatus>('idle')
  const [pendingOperatorActionId, setPendingOperatorActionId] = useState<string | null>(null)
  const [operatorActionError, setOperatorActionError] = useState<OperatorActionErrorView | null>(null)
  const [runtimeRunActionStatus, setRuntimeRunActionStatus] = useState<RuntimeRunActionStatus>('idle')
  const [pendingRuntimeRunAction, setPendingRuntimeRunAction] = useState<RuntimeRunActionKind | null>(null)
  const [runtimeRunActionError, setRuntimeRunActionError] = useState<OperatorActionErrorView | null>(null)
  const [notificationRouteMutationStatus, setNotificationRouteMutationStatus] =
    useState<NotificationRouteMutationStatus>('idle')
  const [pendingNotificationRouteId, setPendingNotificationRouteId] = useState<string | null>(null)
  const [notificationRouteMutationError, setNotificationRouteMutationError] =
    useState<OperatorActionErrorView | null>(null)
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
  const notificationRoutesRef = useRef<Record<string, NotificationRouteDto[]>>({})
  const notificationRouteLoadStatusesRef = useRef<Record<string, NotificationRoutesLoadStatus>>({})
  const notificationRouteLoadRequestRef = useRef<Record<string, number>>({})
  const notificationRouteLoadInFlightRef = useRef<Record<string, Promise<NotificationRouteDto[]>>>({})
  const notificationSyncSummariesRef = useRef<Record<string, SyncNotificationAdaptersResponseDto | null>>({})
  const notificationDispatchesRef = useRef<Record<string, NotificationDispatchDto[]>>({})
  const trustSnapshotRef = useRef<Record<string, AgentTrustSnapshotView>>({})
  const runtimeRefreshTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const pendingRuntimeRefreshRef = useRef<{ projectId: string; source: RefreshSource } | null>(null)
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
    notificationRoutesRef.current = notificationRoutes
  }, [notificationRoutes])

  useEffect(() => {
    notificationRouteLoadStatusesRef.current = notificationRouteLoadStatuses
  }, [notificationRouteLoadStatuses])

  useEffect(() => {
    notificationSyncSummariesRef.current = notificationSyncSummaries
  }, [notificationSyncSummaries])

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

  const loadNotificationRoutes = useCallback(
    async (projectId: string, options: { force?: boolean } = {}): Promise<NotificationRouteDto[]> => {
      const force = options.force ?? false
      const inFlightRequest = notificationRouteLoadInFlightRef.current[projectId]
      if (!force && inFlightRequest) {
        return inFlightRequest
      }

      const cachedRoutes = notificationRoutesRef.current[projectId] ?? []
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

      const requestPromise = adapter
        .listNotificationRoutes(projectId)
        .then((response) => {
          if (notificationRouteLoadRequestRef.current[projectId] !== nextRequestId) {
            return notificationRoutesRef.current[projectId] ?? cachedRoutes
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

          return inScopeRoutes
        })
        .catch((error) => {
          if (notificationRouteLoadRequestRef.current[projectId] !== nextRequestId) {
            return notificationRoutesRef.current[projectId] ?? cachedRoutes
          }

          setNotificationRouteLoadStatuses((currentStatuses) => ({
            ...currentStatuses,
            [projectId]: 'error',
          }))
          setNotificationRouteLoadErrors((currentErrors) => ({
            ...currentErrors,
            [projectId]: getOperatorActionError(error, 'Cadence could not load notification routes for this project.'),
          }))

          return notificationRoutesRef.current[projectId] ?? cachedRoutes
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
          })
            .then((routes) => ({
              ok: true as const,
              routes,
              error: null as string | null,
            }))
            .catch((error) => ({
              ok: false as const,
              routes: notificationRoutesRef.current[projectId] ?? [],
              error: getDesktopErrorMessage(error),
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
        const nextProject = applyRuntimeRun(
          applyRuntimeSession(applyRepositoryStatus(snapshotProject, status), cachedRuntime),
          cachedRuntimeRun,
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

        const [runtimeResult, runtimeRunResult] = await Promise.all([runtimePromise, runtimeRunPromise])
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

        setRuntimeLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: runtimeResult.error,
        }))
        setRuntimeRunLoadErrors((currentErrors) => ({
          ...currentErrors,
          [projectId]: runtimeRunResult.error,
        }))

        const finalRuntime = runtimeResult.runtime ?? cachedRuntime
        const finalRuntimeRun = runtimeRunResult.ok ? runtimeRunResult.runtimeRun : runtimeRunResult.runtimeRun ?? cachedRuntimeRun
        const finalizedProject = applyRuntimeRun(
          finalRuntime ? applyRuntimeSession(nextProject, finalRuntime) : nextProject,
          finalRuntimeRun,
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

  useEffect(() => {
    return () => {
      if (runtimeRefreshTimeoutRef.current) {
        clearTimeout(runtimeRefreshTimeoutRef.current)
        runtimeRefreshTimeoutRef.current = null
      }
      pendingRuntimeRefreshRef.current = null
    }
  }, [])

  const bootstrap = useCallback(async () => {
    setIsLoading(true)
    setRefreshSource('startup')
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
        trustSnapshotRef.current = {}
        resetRepositoryDiffs(null)
        return
      }

      const preferredProjectId = activeProjectIdRef.current
      const nextProjectId =
        preferredProjectId && nextProjects.some((project) => project.id === preferredProjectId)
          ? preferredProjectId
          : nextProjects[0].id

      await loadProject(nextProjectId, 'startup')
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

  const refreshNotificationRoutes = useCallback(
    async (options: { force?: boolean } = {}) => {
      const projectId = activeProjectIdRef.current
      if (!projectId) {
        throw new Error('Select an imported project before loading notification routes.')
      }

      return loadNotificationRoutes(projectId, {
        force: options.force ?? false,
      })
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
    let unsubscribe = () => undefined

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

  const workflowView = useMemo<WorkflowPaneView | null>(() => {
    if (!activeProject) {
      return null
    }

    const lifecycle = getPlanningLifecycleView(activeProject)

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
    }
  }, [activePhase, activeProject])

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

    return {
      project: activeProject,
      activePhase,
      branchLabel: repositoryStatus?.branchLabel ?? activeProject.branchLabel,
      headShaLabel: repositoryStatus?.headShaLabel ?? activeProject.repository?.headShaLabel ?? 'No HEAD',
      runtimeLabel: activeRuntimeSession?.runtimeLabel ?? activeProject.runtimeLabel,
      repositoryLabel: activeProject.repository?.displayName ?? activeProject.name,
      repositoryPath: activeProject.repository?.rootPath ?? null,
      runtimeSession: activeRuntimeSession,
      runtimeRun: activeRuntimeRun,
      runtimeErrorMessage: activeRuntimeErrorMessage,
      runtimeRunErrorMessage: activeRuntimeRunErrorMessage,
      authPhase: activeRuntimeSession?.phase ?? null,
      authPhaseLabel: activeRuntimeSession?.phaseLabel ?? 'Runtime unavailable',
      runtimeStream: activeRuntimeStream,
      runtimeStreamStatus: activeRuntimeStream?.status ?? 'idle',
      runtimeStreamStatusLabel: getRuntimeStreamStatusLabel(activeRuntimeStream?.status ?? 'idle'),
      runtimeStreamError: activeRuntimeStream?.lastIssue ?? null,
      runtimeStreamItems: activeRuntimeStream?.items ?? [],
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
      runtimeRunActionStatus,
      pendingRuntimeRunAction,
      runtimeRunActionError,
      sessionUnavailableReason: getAgentSessionUnavailableReason(activeRuntimeSession, activeRuntimeErrorMessage),
      runtimeRunUnavailableReason: getAgentRuntimeRunUnavailableReason(
        activeRuntimeRun,
        activeRuntimeRunErrorMessage,
        activeRuntimeSession,
      ),
      messagesUnavailableReason: getAgentMessagesUnavailableReason(
        activeRuntimeSession,
        activeRuntimeStream,
        activeRuntimeRun,
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
    activeRuntimeErrorMessage,
    activeRuntimeRun,
    activeRuntimeRunErrorMessage,
    activeRuntimeSession,
    activeRuntimeStream,
    notificationRouteMutationError,
    notificationRouteMutationStatus,
    operatorActionError,
    operatorActionStatus,
    pendingNotificationRouteId,
    pendingOperatorActionId,
    pendingRuntimeRunAction,
    repositoryStatus,
    runtimeRunActionError,
    runtimeRunActionStatus,
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
    errorMessage,
    refreshSource,
    isDesktopRuntime: adapter.isDesktopRuntime(),
    operatorActionStatus,
    pendingOperatorActionId,
    operatorActionError,
    runtimeRunActionStatus,
    pendingRuntimeRunAction,
    runtimeRunActionError,
    selectProject,
    importProject,
    retry,
    showRepositoryDiff,
    retryActiveRepositoryDiff,
    startOpenAiLogin,
    submitOpenAiCallback,
    startRuntimeRun,
    startRuntimeSession,
    stopRuntimeRun,
    logoutRuntimeSession,
    resolveOperatorAction,
    resumeOperatorRun,
    refreshNotificationRoutes,
    upsertNotificationRoute,
  }
}
