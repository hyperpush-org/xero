import {
  createEmptyPlanningLifecycle,
  type PlanningLifecycleView,
} from '@/src/lib/cadence-model/workflow'
import { deriveAutonomousWorkflowContext } from '@/src/lib/cadence-model/autonomous'
import {
  type NotificationRouteDto,
  type SyncNotificationAdaptersResponseDto,
} from '@/src/lib/cadence-model/notifications'
import { type RepositoryStatusView } from '@/src/lib/cadence-model/project'
import {
  type RuntimeRunView,
  type RuntimeSessionView,
  type RuntimeSettingsDto,
} from '@/src/lib/cadence-model/runtime'
import {
  getRuntimeStreamStatusLabel,
  type RuntimeStreamView,
} from '@/src/lib/cadence-model/runtime-stream'
import {
  type Phase,
  type ProjectDetailView,
  type ProviderProfilesDto,
} from '@/src/lib/cadence-model'
import { projectCheckpointControlLoops } from '../agent-runtime-projections/checkpoint-control-loops'
import { projectRecentAutonomousUnits } from '../agent-runtime-projections/recent-autonomous-units'
import {
  composeAgentTrustSnapshot,
  createUnavailableTrustSnapshot,
  mapNotificationChannelHealth,
  mapNotificationRouteViews,
  type BlockedNotificationSyncPollTarget,
} from './notification-health'
import {
  getAgentMessagesUnavailableReason,
  getAgentRuntimeRunUnavailableReason,
  getAgentSessionUnavailableReason,
  hasProviderMismatch,
  resolveSelectedRuntimeProvider,
} from './runtime-provider'
import type {
  AgentPaneView,
  AgentTrustSnapshotView,
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  DiffScopeSummary,
  ExecutionPaneView,
  NotificationRouteMutationStatus,
  NotificationRoutesLoadStatus,
  OperatorActionErrorView,
  OperatorActionStatus,
  RuntimeRunActionKind,
  RuntimeRunActionStatus,
  WorkflowPaneView,
} from './types'

const REPOSITORY_DIFF_SCOPE_LABELS = {
  staged: 'Staged',
  unstaged: 'Unstaged',
  worktree: 'Worktree',
} as const

interface SelectedProviderProjection {
  providerMismatch: boolean
  selectedProvider: ReturnType<typeof resolveSelectedRuntimeProvider>
}

export interface BuildWorkflowViewDependencies {
  project: ProjectDetailView | null
  activePhase: Phase | null
  providerProfiles: ProviderProfilesDto | null
  runtimeSession: RuntimeSessionView | null
  runtimeSettings: RuntimeSettingsDto | null
}

export interface BuildAgentViewDependencies {
  project: ProjectDetailView | null
  activePhase: Phase | null
  repositoryStatus: RepositoryStatusView | null
  providerProfiles: ProviderProfilesDto | null
  runtimeSession: RuntimeSessionView | null
  runtimeSettings: RuntimeSettingsDto | null
  runtimeRun: RuntimeRunView | null
  autonomousRun: ProjectDetailView['autonomousRun']
  autonomousUnit: ProjectDetailView['autonomousUnit']
  autonomousAttempt: ProjectDetailView['autonomousAttempt']
  autonomousHistory: ProjectDetailView['autonomousHistory']
  autonomousRecentArtifacts: ProjectDetailView['autonomousRecentArtifacts']
  runtimeErrorMessage: string | null
  runtimeRunErrorMessage: string | null
  autonomousRunErrorMessage: string | null
  runtimeStream: RuntimeStreamView | null
  notificationRoutes: NotificationRouteDto[]
  notificationRouteLoadStatus: NotificationRoutesLoadStatus
  notificationRouteError: OperatorActionErrorView | null
  notificationSyncSummary: SyncNotificationAdaptersResponseDto | null
  notificationSyncError: OperatorActionErrorView | null
  blockedNotificationSyncPollTarget: BlockedNotificationSyncPollTarget | null
  notificationRouteMutationStatus: NotificationRouteMutationStatus
  pendingNotificationRouteId: string | null
  notificationRouteMutationError: OperatorActionErrorView | null
  previousTrustSnapshot: AgentTrustSnapshotView | null
  operatorActionStatus: OperatorActionStatus
  pendingOperatorActionId: string | null
  operatorActionError: OperatorActionErrorView | null
  autonomousRunActionStatus: AutonomousRunActionStatus
  pendingAutonomousRunAction: AutonomousRunActionKind | null
  autonomousRunActionError: OperatorActionErrorView | null
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
}

export interface BuildAgentViewResult {
  trustSnapshot: AgentTrustSnapshotView | null
  view: AgentPaneView | null
}

export interface BuildExecutionViewDependencies {
  project: ProjectDetailView | null
  activePhase: Phase | null
  repositoryStatus: RepositoryStatusView | null
  operatorActionError: OperatorActionErrorView | null
}

function getPlanningLifecycleView(project: ProjectDetailView): PlanningLifecycleView {
  return project.lifecycle ?? createEmptyPlanningLifecycle()
}

function getSelectedProviderProjection(
  providerProfiles: ProviderProfilesDto | null,
  runtimeSettings: RuntimeSettingsDto | null,
  runtimeSession: RuntimeSessionView | null,
): SelectedProviderProjection {
  const selectedProvider = resolveSelectedRuntimeProvider(providerProfiles, runtimeSettings, runtimeSession)

  return {
    selectedProvider,
    providerMismatch: hasProviderMismatch(selectedProvider, runtimeSession),
  }
}

function getFallbackTrustSnapshot(options: {
  previousTrustSnapshot: AgentTrustSnapshotView | null
  notificationRouteViews: ReturnType<typeof mapNotificationRouteViews>
  project: ProjectDetailView
  notificationRouteError: OperatorActionErrorView | null
  notificationSyncError: OperatorActionErrorView | null
  error: unknown
}): AgentTrustSnapshotView {
  const projectionError = getProjectionError(
    options.error,
    'Cadence could not compose trust snapshot details from notification/runtime projection data.',
  )

  if (options.previousTrustSnapshot) {
    return {
      ...options.previousTrustSnapshot,
      routeError: options.notificationRouteError,
      syncError: options.notificationSyncError,
      projectionError,
    }
  }

  return createUnavailableTrustSnapshot({
    routeCount: options.notificationRouteViews.length,
    enabledRouteCount: options.notificationRouteViews.filter((route) => route.enabled).length,
    pendingApprovalCount: options.project.pendingApprovalCount,
    notificationRouteError: options.notificationRouteError,
    notificationSyncError: options.notificationSyncError,
    projectionError,
  })
}

function getProjectionError(error: unknown, fallback: string): OperatorActionErrorView {
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

export function buildWorkflowView({
  project,
  activePhase,
  providerProfiles,
  runtimeSession,
  runtimeSettings,
}: BuildWorkflowViewDependencies): WorkflowPaneView | null {
  if (!project) {
    return null
  }

  const lifecycle = getPlanningLifecycleView(project)
  const { selectedProvider, providerMismatch } = getSelectedProviderProjection(
    providerProfiles,
    runtimeSettings,
    runtimeSession,
  )

  return {
    project,
    activePhase,
    lifecycle,
    activeLifecycleStage: lifecycle.activeStage,
    lifecyclePercent: lifecycle.percentComplete,
    hasLifecycle: lifecycle.hasStages,
    actionRequiredLifecycleCount: lifecycle.actionRequiredCount,
    overallPercent: project.phaseProgressPercent,
    hasPhases: project.phases.length > 0,
    runtimeSession,
    selectedProfileId: selectedProvider.profileId,
    selectedProviderId: selectedProvider.providerId,
    selectedProviderLabel: selectedProvider.providerLabel,
    selectedModelId: selectedProvider.modelId,
    selectedProfileReadiness: selectedProvider.readiness,
    openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
    providerMismatch,
  }
}

export function buildAgentView({
  project,
  activePhase,
  repositoryStatus,
  providerProfiles,
  runtimeSession,
  runtimeSettings,
  runtimeRun,
  autonomousRun,
  autonomousUnit,
  autonomousAttempt,
  autonomousHistory,
  autonomousRecentArtifacts,
  runtimeErrorMessage,
  runtimeRunErrorMessage,
  autonomousRunErrorMessage,
  runtimeStream,
  notificationRoutes,
  notificationRouteLoadStatus,
  notificationRouteError,
  notificationSyncSummary,
  notificationSyncError,
  blockedNotificationSyncPollTarget,
  notificationRouteMutationStatus,
  pendingNotificationRouteId,
  notificationRouteMutationError,
  previousTrustSnapshot,
  operatorActionStatus,
  pendingOperatorActionId,
  operatorActionError,
  autonomousRunActionStatus,
  pendingAutonomousRunAction,
  autonomousRunActionError,
  runtimeRunActionStatus,
  pendingRuntimeRunAction,
  runtimeRunActionError,
}: BuildAgentViewDependencies): BuildAgentViewResult {
  if (!project) {
    return {
      view: null,
      trustSnapshot: null,
    }
  }

  const notificationRouteViews = mapNotificationRouteViews(
    project.id,
    notificationRoutes,
    project.notificationBroker.dispatches,
  )
  const notificationChannelHealth = mapNotificationChannelHealth(notificationRouteViews)

  let trustSnapshot: AgentTrustSnapshotView
  try {
    trustSnapshot = composeAgentTrustSnapshot({
      runtimeSession,
      runtimeRun,
      runtimeStream,
      approvalRequests: project.approvalRequests,
      routeViews: notificationRouteViews,
      notificationRouteError,
      notificationSyncSummary,
      notificationSyncError,
    })
  } catch (error) {
    trustSnapshot = getFallbackTrustSnapshot({
      previousTrustSnapshot,
      notificationRouteViews,
      project,
      notificationRouteError,
      notificationSyncError,
      error,
    })
  }

  const autonomousWorkflowContext = deriveAutonomousWorkflowContext({
    lifecycle: project.lifecycle,
    handoffPackages: project.handoffPackages,
    approvalRequests: project.approvalRequests,
    autonomousUnit: autonomousUnit ?? null,
    autonomousAttempt: autonomousAttempt ?? null,
  })
  const recentAutonomousUnits = projectRecentAutonomousUnits({
    autonomousHistory,
    autonomousRecentArtifacts,
    lifecycle: project.lifecycle,
    handoffPackages: project.handoffPackages,
    approvalRequests: project.approvalRequests,
  })
  const checkpointControlLoop = projectCheckpointControlLoops({
    actionRequiredItems: runtimeStream?.actionRequired ?? [],
    approvalRequests: project.approvalRequests,
    resumeHistory: project.resumeHistory,
    notificationBroker: project.notificationBroker,
    autonomousHistory,
    autonomousRecentArtifacts,
  })
  const { selectedProvider, providerMismatch } = getSelectedProviderProjection(
    providerProfiles,
    runtimeSettings,
    runtimeSession,
  )

  return {
    trustSnapshot,
    view: {
      project,
      activePhase,
      branchLabel: repositoryStatus?.branchLabel ?? project.branchLabel,
      headShaLabel: repositoryStatus?.headShaLabel ?? project.repository?.headShaLabel ?? 'No HEAD',
      runtimeLabel: runtimeSession?.runtimeLabel ?? project.runtimeLabel,
      repositoryLabel: project.repository?.displayName ?? project.name,
      repositoryPath: project.repository?.rootPath ?? null,
      runtimeSession,
      selectedProfileId: selectedProvider.profileId,
      selectedProviderId: selectedProvider.providerId,
      selectedProviderLabel: selectedProvider.providerLabel,
      selectedModelId: selectedProvider.modelId,
      selectedProfileReadiness: selectedProvider.readiness,
      openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
      providerMismatch,
      runtimeRun,
      autonomousRun,
      autonomousUnit,
      autonomousAttempt,
      autonomousWorkflowContext,
      autonomousHistory,
      autonomousRecentArtifacts,
      recentAutonomousUnits,
      checkpointControlLoop,
      runtimeErrorMessage,
      runtimeRunErrorMessage,
      autonomousRunErrorMessage,
      authPhase: runtimeSession?.phase ?? null,
      authPhaseLabel: runtimeSession?.phaseLabel ?? 'Runtime unavailable',
      runtimeStream,
      runtimeStreamStatus: runtimeStream?.status ?? 'idle',
      runtimeStreamStatusLabel: getRuntimeStreamStatusLabel(runtimeStream?.status ?? 'idle'),
      runtimeStreamError: runtimeStream?.lastIssue ?? null,
      runtimeStreamItems: runtimeStream?.items ?? [],
      skillItems: runtimeStream?.skillItems ?? [],
      activityItems: runtimeStream?.activityItems ?? [],
      actionRequiredItems: runtimeStream?.actionRequired ?? [],
      notificationBroker: project.notificationBroker,
      notificationRoutes: notificationRouteViews,
      notificationChannelHealth,
      notificationRouteLoadStatus,
      notificationRouteIsRefreshing:
        notificationRouteLoadStatus === 'loading' && notificationRouteViews.length > 0,
      notificationRouteError,
      notificationSyncSummary,
      notificationSyncError,
      notificationSyncPollingActive: Boolean(blockedNotificationSyncPollTarget),
      notificationSyncPollingActionId: blockedNotificationSyncPollTarget?.actionId ?? null,
      notificationSyncPollingBoundaryId: blockedNotificationSyncPollTarget?.boundaryId ?? null,
      notificationRouteMutationStatus,
      pendingNotificationRouteId,
      notificationRouteMutationError,
      trustSnapshot,
      approvalRequests: project.approvalRequests,
      pendingApprovalCount: project.pendingApprovalCount,
      latestDecisionOutcome: project.latestDecisionOutcome,
      resumeHistory: project.resumeHistory,
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
        runtimeSession,
        runtimeErrorMessage,
        selectedProvider,
      ),
      runtimeRunUnavailableReason: getAgentRuntimeRunUnavailableReason(
        runtimeRun,
        runtimeRunErrorMessage,
        runtimeSession,
        selectedProvider,
      ),
      messagesUnavailableReason: getAgentMessagesUnavailableReason(
        runtimeSession,
        runtimeStream,
        runtimeRun,
        selectedProvider,
      ),
    },
  }
}

export function buildExecutionView({
  project,
  activePhase,
  repositoryStatus,
  operatorActionError,
}: BuildExecutionViewDependencies): ExecutionPaneView | null {
  if (!project) {
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
    project,
    activePhase,
    branchLabel: repositoryStatus?.branchLabel ?? project.branchLabel,
    headShaLabel: repositoryStatus?.headShaLabel ?? project.repository?.headShaLabel ?? 'No HEAD',
    statusEntries,
    statusCount: repositoryStatus?.statusCount ?? 0,
    hasChanges: repositoryStatus?.hasChanges ?? false,
    diffScopes,
    verificationRecords: project.verificationRecords,
    resumeHistory: project.resumeHistory,
    latestDecisionOutcome: project.latestDecisionOutcome,
    notificationBroker: project.notificationBroker,
    operatorActionError,
    verificationUnavailableReason:
      project.verificationRecords.length > 0 || project.resumeHistory.length > 0
        ? 'Durable operator verification and resume history are loaded from the selected project snapshot.'
        : 'Verification details will appear here once the backend exposes run and wave results.',
  }
}
