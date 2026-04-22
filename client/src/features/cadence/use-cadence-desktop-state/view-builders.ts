import {
  createEmptyPlanningLifecycle,
  type PlanningLifecycleView,
} from '@/src/lib/cadence-model/workflow'
import { deriveAutonomousWorkflowContext } from '@/src/lib/cadence-model/autonomous'
import {
  type NotificationRouteDto,
  type SyncNotificationAdaptersResponseDto,
} from '@/src/lib/cadence-model/notifications'
import {
  getProviderModelCatalogFetchedAt,
  type ProviderModelCatalogDto,
  type ProviderModelThinkingEffortDto,
  type ProviderModelDto,
} from '@/src/lib/cadence-model/provider-models'
import { type RepositoryStatusView } from '@/src/lib/cadence-model/project'
import {
  DEFAULT_RUNTIME_RUN_APPROVAL_MODE,
  type RuntimeRunActiveControlSnapshotView,
  type RuntimeRunApprovalModeDto,
  type RuntimeRunPendingControlSnapshotView,
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
  getProviderMismatchCopy,
  getRuntimeProviderLabel,
  hasProviderMismatch,
  resolveSelectedRuntimeProvider,
} from './runtime-provider'
import type {
  AgentPaneView,
  AgentProviderModelCatalogView,
  AgentProviderModelView,
  AgentTrustSnapshotView,
  AutonomousRunActionKind,
  AutonomousRunActionStatus,
  DiffScopeSummary,
  ExecutionPaneView,
  NotificationRouteMutationStatus,
  NotificationRoutesLoadStatus,
  OperatorActionErrorView,
  OperatorActionStatus,
  ProviderModelCatalogLoadStatus,
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
  providerMismatchCopy: ReturnType<typeof getProviderMismatchCopy>
}

interface AgentRunControlProjection {
  source: 'runtime_run' | 'fallback'
  selectedModelId: string | null
  selectedThinkingEffort: ProviderModelThinkingEffortDto | null
  selectedApprovalMode: RuntimeRunApprovalModeDto
  selectedPrompt: {
    text: string | null
    queuedAt: string | null
    hasQueuedPrompt: boolean
  }
  activeControls: RuntimeRunActiveControlSnapshotView | null
  pendingControls: RuntimeRunPendingControlSnapshotView | null
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
  activeProviderModelCatalog: ProviderModelCatalogDto | null
  activeProviderModelCatalogLoadStatus: ProviderModelCatalogLoadStatus
  activeProviderModelCatalogLoadError: OperatorActionErrorView | null
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
  const providerMismatchCopy = getProviderMismatchCopy(selectedProvider, runtimeSession)

  return {
    selectedProvider,
    providerMismatch: hasProviderMismatch(selectedProvider, runtimeSession),
    providerMismatchCopy,
  }
}

function getAgentRunControlProjection(runtimeRun: RuntimeRunView | null): AgentRunControlProjection {
  const activeControls = runtimeRun?.controls?.active ?? null
  const pendingControls = runtimeRun?.controls?.pending ?? null
  const selectedControls = runtimeRun?.controls?.selected ?? null
  const useRuntimeRunTruth = Boolean(selectedControls && !runtimeRun?.isTerminal)

  return {
    source: useRuntimeRunTruth ? 'runtime_run' : 'fallback',
    selectedModelId: useRuntimeRunTruth ? selectedControls?.modelId ?? null : null,
    selectedThinkingEffort: useRuntimeRunTruth ? selectedControls?.thinkingEffort ?? null : null,
    selectedApprovalMode: useRuntimeRunTruth
      ? selectedControls?.approvalMode ?? DEFAULT_RUNTIME_RUN_APPROVAL_MODE
      : DEFAULT_RUNTIME_RUN_APPROVAL_MODE,
    selectedPrompt: useRuntimeRunTruth
      ? {
          text: selectedControls?.queuedPrompt ?? null,
          queuedAt: selectedControls?.queuedPromptAt ?? null,
          hasQueuedPrompt: selectedControls?.hasQueuedPrompt ?? false,
        }
      : {
          text: null,
          queuedAt: null,
          hasQueuedPrompt: false,
        },
    activeControls,
    pendingControls,
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

const MODEL_GROUP_LABELS: Record<string, string> = {
  anthropic: 'Anthropic',
  deepseek: 'DeepSeek',
  google: 'Google',
  meta: 'Meta',
  'meta-llama': 'Meta Llama',
  mistral: 'Mistral',
  moonshot: 'Moonshot',
  moonshotai: 'Moonshot',
  openai: 'OpenAI',
  openrouter: 'OpenRouter',
  'x-ai': 'xAI',
  xai: 'xAI',
}

function getCatalogOwnerLabel(selectedProvider: SelectedProviderProjection['selectedProvider']): string {
  const profileLabel = selectedProvider.profileLabel?.trim() ?? ''
  if (profileLabel.length > 0) {
    return profileLabel
  }

  const profileId = selectedProvider.profileId?.trim() ?? ''
  if (profileId.length > 0) {
    return profileId
  }

  return selectedProvider.providerLabel
}

function getModelGroupLabel(modelId: string, providerLabel: string): { groupId: string; groupLabel: string } {
  const trimmedModelId = modelId.trim()
  const namespace = trimmedModelId.includes('/') ? trimmedModelId.split('/')[0]?.trim() ?? '' : ''
  if (namespace.length === 0) {
    return {
      groupId: providerLabel.trim().toLowerCase().replace(/[^a-z0-9]+/g, '_') || 'provider_models',
      groupLabel: providerLabel,
    }
  }

  const normalizedNamespace = namespace.toLowerCase()
  const knownLabel = MODEL_GROUP_LABELS[normalizedNamespace]
  if (knownLabel) {
    return {
      groupId: normalizedNamespace.replace(/[^a-z0-9]+/g, '_'),
      groupLabel: knownLabel,
    }
  }

  return {
    groupId: normalizedNamespace.replace(/[^a-z0-9]+/g, '_'),
    groupLabel: getRuntimeProviderLabel(namespace),
  }
}

function getThinkingEffortOptions(model: ProviderModelDto): ProviderModelThinkingEffortDto[] {
  if (!model.thinking.supported) {
    return []
  }

  const effortOptions: ProviderModelThinkingEffortDto[] = []
  for (const effort of model.thinking.effortOptions) {
    if (!effortOptions.includes(effort)) {
      effortOptions.push(effort)
    }
  }

  return effortOptions
}

function buildAgentProviderModel(
  model: ProviderModelDto,
  providerLabel: string,
): AgentProviderModelView | null {
  const modelId = model.modelId.trim()
  if (modelId.length === 0) {
    return null
  }

  const effortOptions = getThinkingEffortOptions(model)
  const defaultThinkingEffort =
    model.thinking.supported && model.thinking.defaultEffort && effortOptions.includes(model.thinking.defaultEffort)
      ? model.thinking.defaultEffort
      : effortOptions[0] ?? null
  const { groupId, groupLabel } = getModelGroupLabel(modelId, providerLabel)

  return {
    modelId,
    label: modelId,
    displayName: model.displayName.trim() || modelId,
    groupId,
    groupLabel,
    availability: 'available',
    availabilityLabel: 'Available',
    thinkingSupported: effortOptions.length > 0,
    thinkingEffortOptions: effortOptions,
    defaultThinkingEffort,
  }
}

function buildOrphanedAgentProviderModel(modelId: string): AgentProviderModelView | null {
  const trimmedModelId = modelId.trim()
  if (trimmedModelId.length === 0) {
    return null
  }

  return {
    modelId: trimmedModelId,
    label: trimmedModelId,
    displayName: trimmedModelId,
    groupId: 'current_selection',
    groupLabel: 'Current selection',
    availability: 'orphaned',
    availabilityLabel: 'Unavailable',
    thinkingSupported: false,
    thinkingEffortOptions: [],
    defaultThinkingEffort: null,
  }
}

function getCatalogRefreshError(
  catalog: ProviderModelCatalogDto | null,
  loadError: OperatorActionErrorView | null,
): OperatorActionErrorView | null {
  if (catalog?.lastRefreshError) {
    return {
      code: catalog.lastRefreshError.code,
      message: catalog.lastRefreshError.message,
      retryable: catalog.lastRefreshError.retryable,
    }
  }

  return loadError
}

function getCatalogStateCopy(options: {
  selectedProvider: SelectedProviderProjection['selectedProvider']
  catalog: ProviderModelCatalogDto | null
  loadStatus: ProviderModelCatalogLoadStatus
  refreshError: OperatorActionErrorView | null
  discoveredModelCount: number
}): Pick<AgentProviderModelCatalogView, 'state' | 'stateLabel' | 'detail'> {
  const ownerLabel = getCatalogOwnerLabel(options.selectedProvider)

  if (options.catalog?.source === 'live' && options.discoveredModelCount > 0) {
    return {
      state: 'live',
      stateLabel: 'Live catalog',
      detail:
        options.loadStatus === 'loading'
          ? `Refreshing ${ownerLabel} model discovery while keeping ${options.discoveredModelCount} live model${
              options.discoveredModelCount === 1 ? '' : 's'
            } visible.`
          : `Showing ${options.discoveredModelCount} discovered model${
              options.discoveredModelCount === 1 ? '' : 's'
            } for ${ownerLabel}.`,
    }
  }

  if (options.discoveredModelCount > 0) {
    return {
      state: 'stale',
      stateLabel: options.catalog?.source === 'cache' ? 'Cached catalog' : 'Stale catalog',
      detail: options.refreshError?.message?.trim()
        ? `${options.refreshError.message} Cadence is keeping the last successful model catalog for ${ownerLabel} visible.`
        : `Cadence is keeping the last successful model catalog for ${ownerLabel} visible.`,
    }
  }

  if (options.loadStatus === 'loading') {
    return {
      state: 'unavailable',
      stateLabel: 'Catalog unavailable',
      detail: `Loading the active provider-model catalog for ${ownerLabel}. Cadence is keeping the configured model visible without sample lists.`,
    }
  }

  return {
    state: 'unavailable',
    stateLabel: 'Catalog unavailable',
    detail: options.refreshError?.message?.trim()
      ? `${options.refreshError.message} Cadence is keeping the configured model visible without discovered-model confirmation.`
      : `Cadence does not have a discovered model catalog for ${ownerLabel} yet, so only configured model truth remains visible.`,
  }
}

function buildAgentProviderModelCatalog(options: {
  selectedProvider: SelectedProviderProjection['selectedProvider']
  activeProviderModelCatalog: ProviderModelCatalogDto | null
  activeProviderModelCatalogLoadStatus: ProviderModelCatalogLoadStatus
  activeProviderModelCatalogLoadError: OperatorActionErrorView | null
  selectedModelIdOverride?: string | null
  allowCatalogTruth?: boolean
}): {
  providerModelCatalog: AgentProviderModelCatalogView
  selectedModelOption: AgentProviderModelView | null
  selectedModelThinkingEffortOptions: ProviderModelThinkingEffortDto[]
  selectedModelDefaultThinkingEffort: ProviderModelThinkingEffortDto | null
  selectedModelId: string | null
} {
  const allowCatalogTruth = options.allowCatalogTruth ?? true
  const catalog = allowCatalogTruth ? options.activeProviderModelCatalog : null
  const refreshError = allowCatalogTruth
    ? getCatalogRefreshError(catalog, options.activeProviderModelCatalogLoadError)
    : null
  const discoveredModels: AgentProviderModelView[] = []
  const seenModelIds = new Set<string>()

  for (const model of catalog?.models ?? []) {
    const nextModel = buildAgentProviderModel(model, options.selectedProvider.providerLabel)
    if (!nextModel || seenModelIds.has(nextModel.modelId)) {
      continue
    }

    seenModelIds.add(nextModel.modelId)
    discoveredModels.push(nextModel)
  }

  const configuredModelId =
    options.selectedModelIdOverride?.trim() ||
    catalog?.configuredModelId.trim() ||
    options.selectedProvider.modelId?.trim() ||
    null
  const selectedModelId = configuredModelId && configuredModelId.length > 0 ? configuredModelId : null
  const selectedDiscoveredModel =
    selectedModelId ? discoveredModels.find((model) => model.modelId === selectedModelId) ?? null : null
  const selectedModelOption =
    selectedDiscoveredModel ?? (selectedModelId ? buildOrphanedAgentProviderModel(selectedModelId) : null)
  const models = selectedModelOption && selectedModelOption.availability === 'orphaned'
    ? [selectedModelOption, ...discoveredModels]
    : discoveredModels
  const stateCopy = allowCatalogTruth
    ? getCatalogStateCopy({
        selectedProvider: options.selectedProvider,
        catalog,
        loadStatus: options.activeProviderModelCatalogLoadStatus,
        refreshError,
        discoveredModelCount: discoveredModels.length,
      })
    : {
        state: 'unavailable' as const,
        stateLabel: 'Catalog unavailable',
        detail:
          'Cadence is showing durable run-scoped control truth from the active supervised run while keeping provider defaults out of the live projection.',
      }

  return {
    providerModelCatalog: {
      profileId: catalog?.profileId ?? options.selectedProvider.profileId,
      profileLabel: options.selectedProvider.profileLabel,
      providerId: catalog?.providerId ?? options.selectedProvider.providerId,
      providerLabel: options.selectedProvider.providerLabel,
      source: catalog?.source ?? null,
      loadStatus: options.activeProviderModelCatalogLoadStatus,
      state: stateCopy.state,
      stateLabel: stateCopy.stateLabel,
      detail: stateCopy.detail,
      fetchedAt: getProviderModelCatalogFetchedAt(catalog),
      lastSuccessAt: catalog?.lastSuccessAt ?? null,
      lastRefreshError: refreshError,
      models,
    },
    selectedModelOption,
    selectedModelThinkingEffortOptions: selectedModelOption?.thinkingEffortOptions ?? [],
    selectedModelDefaultThinkingEffort: selectedModelOption?.defaultThinkingEffort ?? null,
    selectedModelId,
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
  const { selectedProvider, providerMismatch, providerMismatchCopy } = getSelectedProviderProjection(
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
    selectedProfileLabel: selectedProvider.profileLabel,
    selectedProviderId: selectedProvider.providerId,
    selectedProviderLabel: selectedProvider.providerLabel,
    selectedProviderSource: selectedProvider.source,
    selectedModelId: selectedProvider.modelId,
    selectedProfileReadiness: selectedProvider.readiness,
    openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
    providerMismatch,
    providerMismatchReason: providerMismatchCopy?.reason ?? null,
    providerMismatchRecoveryCopy: providerMismatchCopy?.sessionRecoveryCopy ?? null,
  }
}

export function buildAgentView({
  project,
  activePhase,
  repositoryStatus,
  providerProfiles,
  runtimeSession,
  runtimeSettings,
  activeProviderModelCatalog,
  activeProviderModelCatalogLoadStatus,
  activeProviderModelCatalogLoadError,
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
  const { selectedProvider, providerMismatch, providerMismatchCopy } = getSelectedProviderProjection(
    providerProfiles,
    runtimeSettings,
    runtimeSession,
  )
  const controlProjection = getAgentRunControlProjection(runtimeRun)
  const allowCatalogTruth =
    controlProjection.source === 'fallback' || runtimeRun?.providerId === selectedProvider.providerId
  const providerModelCatalogProjection = buildAgentProviderModelCatalog({
    selectedProvider,
    activeProviderModelCatalog,
    activeProviderModelCatalogLoadStatus,
    activeProviderModelCatalogLoadError,
    selectedModelIdOverride: controlProjection.selectedModelId,
    allowCatalogTruth,
  })
  const selectedModelId =
    controlProjection.source === 'runtime_run'
      ? controlProjection.selectedModelId
      : providerModelCatalogProjection.selectedModelId
  const selectedThinkingEffort =
    controlProjection.source === 'runtime_run'
      ? controlProjection.selectedThinkingEffort
      : providerModelCatalogProjection.selectedModelDefaultThinkingEffort
  const selectedApprovalMode = controlProjection.selectedApprovalMode

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
      selectedProfileLabel: selectedProvider.profileLabel,
      selectedProviderId: selectedProvider.providerId,
      selectedProviderLabel: selectedProvider.providerLabel,
      selectedProviderSource: selectedProvider.source,
      controlTruthSource: controlProjection.source,
      selectedModelId,
      selectedThinkingEffort,
      selectedApprovalMode,
      selectedPrompt: controlProjection.selectedPrompt,
      runtimeRunActiveControls: controlProjection.activeControls,
      runtimeRunPendingControls: controlProjection.pendingControls,
      providerModelCatalog: providerModelCatalogProjection.providerModelCatalog,
      selectedModelOption: providerModelCatalogProjection.selectedModelOption,
      selectedModelThinkingEffortOptions: providerModelCatalogProjection.selectedModelThinkingEffortOptions,
      selectedModelDefaultThinkingEffort: providerModelCatalogProjection.selectedModelDefaultThinkingEffort,
      selectedProfileReadiness: selectedProvider.readiness,
      openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
      providerMismatch,
      providerMismatchReason: providerMismatchCopy?.reason ?? null,
      providerMismatchRecoveryCopy: providerMismatchCopy?.sessionRecoveryCopy ?? null,
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
