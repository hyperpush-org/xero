import {
  notificationRouteCredentialReadinessSchema,
  type NotificationDispatchView,
  type NotificationRouteCredentialReadinessDto,
  type NotificationRouteDto,
  type NotificationRouteKindDto,
  type SyncNotificationAdaptersResponseDto,
} from '@/src/lib/xero-model/notifications'
import { type OperatorApprovalView } from '@/src/lib/xero-model/operator-actions'
import { type RuntimeRunView, type RuntimeSessionView } from '@/src/lib/xero-model/runtime'
import { type RuntimeStreamView } from '@/src/lib/xero-model/runtime-stream'
import { type ProjectDetailView } from '@/src/lib/xero-model'
import type {
  AgentTrustSignalState,
  AgentTrustSnapshotView,
  NotificationChannelHealthView,
  NotificationRouteHealthState,
  NotificationRouteHealthView,
  OperatorActionErrorView,
} from './types'

export const BLOCKED_NOTIFICATION_SYNC_POLL_MS = 400

const NOTIFICATION_ROUTE_KINDS: NotificationRouteKindDto[] = ['telegram', 'discord']

const NOTIFICATION_ROUTE_KIND_LABELS: Record<NotificationRouteKindDto, string> = {
  telegram: 'Telegram',
  discord: 'Discord',
}

export interface BlockedNotificationSyncPollTarget {
  projectId: string
  actionId: string
  boundaryId: string
}

function getNotificationRouteKindLabel(routeKind: NotificationRouteKindDto): string {
  return NOTIFICATION_ROUTE_KIND_LABELS[routeKind]
}

function extractRuntimeBoundaryIdFromActionId(actionId: string, actionType: string): string | null {
  const normalizedActionId = actionId.trim()
  const normalizedActionType = actionType.trim()
  if (
    normalizedActionId.length === 0 ||
    normalizedActionType.length === 0 ||
    !normalizedActionId.includes(':run:') ||
    !normalizedActionId.includes(':boundary:')
  ) {
    return null
  }

  const boundaryMarker = ':boundary:'
  const boundaryMarkerIndex = normalizedActionId.indexOf(boundaryMarker)
  if (boundaryMarkerIndex < 0) {
    return null
  }

  const boundaryAndAction = normalizedActionId.slice(boundaryMarkerIndex + boundaryMarker.length)
  const actionSuffix = `:${normalizedActionType}`
  if (!boundaryAndAction.endsWith(actionSuffix)) {
    return null
  }

  const boundaryId = boundaryAndAction.slice(0, -actionSuffix.length).trim()
  return boundaryId.length > 0 ? boundaryId : null
}

export function getBlockedNotificationSyncPollTarget(options: {
  project: ProjectDetailView | null
  runtimeStream: RuntimeStreamView | null
}): BlockedNotificationSyncPollTarget | null {
  const { project, runtimeStream } = options
  if (!project) {
    return null
  }

  const pendingApprovals = project.approvalRequests.filter((approval) => approval.isPending)
  if (pendingApprovals.length === 0) {
    return null
  }

  const pendingActionIds = new Set(pendingApprovals.map((approval) => approval.actionId))
  const matchingRuntimeAction = [...(runtimeStream?.actionRequired ?? [])]
    .filter((item) => pendingActionIds.has(item.actionId) && item.boundaryId?.trim())
    .sort((left, right) => getTimestampMs(right.createdAt) - getTimestampMs(left.createdAt))[0]

  if (matchingRuntimeAction) {
    return {
      projectId: project.id,
      actionId: matchingRuntimeAction.actionId,
      boundaryId: matchingRuntimeAction.boundaryId?.trim() ?? '',
    }
  }

  const matchingApproval = pendingApprovals.find((approval) =>
    Boolean(extractRuntimeBoundaryIdFromActionId(approval.actionId, approval.actionType)),
  )

  if (!matchingApproval) {
    return null
  }

  const boundaryId = extractRuntimeBoundaryIdFromActionId(matchingApproval.actionId, matchingApproval.actionType)
  if (!boundaryId) {
    return null
  }

  return {
    projectId: project.id,
    actionId: matchingApproval.actionId,
    boundaryId,
  }
}

export function getBlockedNotificationSyncPollKey(
  target: BlockedNotificationSyncPollTarget | null,
): string | null {
  if (!target) {
    return null
  }

  return `${target.projectId}:${target.actionId}:${target.boundaryId}`
}

function getTimestampMs(value: string | null | undefined): number {
  if (typeof value !== 'string' || value.trim().length === 0) {
    return 0
  }

  const parsed = Date.parse(value)
  return Number.isFinite(parsed) ? parsed : 0
}

export function getNotificationHealthLabel(state: NotificationRouteHealthState): string {
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

export function getNotificationHealthState(options: {
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

export function summarizeRouteDispatches(dispatches: NotificationDispatchView[]) {
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

export function mapNotificationRouteViews(
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

export function mapNotificationChannelHealth(
  routeViews: NotificationRouteHealthView[],
): NotificationChannelHealthView[] {
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
      `Xero could not compose trust snapshot because route \`${route.routeId}\` readiness metadata was malformed: ${parsedReadiness.error.message}`,
    )
  }

  return parsedReadiness.data
}

export function composeAgentTrustSnapshot(options: {
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
      ? 'The durable runtime-run record indicates the owned agent run failed.'
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

export function createUnavailableTrustSnapshot(options: {
  routeCount: number
  enabledRouteCount: number
  pendingApprovalCount: number
  notificationRouteError: OperatorActionErrorView | null
  notificationSyncError: OperatorActionErrorView | null
  projectionError: OperatorActionErrorView | null
}): AgentTrustSnapshotView {
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
