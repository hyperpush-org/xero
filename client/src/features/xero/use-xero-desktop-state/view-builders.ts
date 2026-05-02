import {
  type NotificationRouteDto,
  type SyncNotificationAdaptersResponseDto,
} from '@/src/lib/xero-model/notifications'
import {
  getProviderModelCatalogFetchedAt,
  type ProviderModelCatalogDto,
  type ProviderModelThinkingEffortDto,
} from '@/src/lib/xero-model/provider-models'
import { type RepositoryStatusView } from '@/src/lib/xero-model/project'
import {
  DEFAULT_RUNTIME_AGENT_ID,
  DEFAULT_RUNTIME_RUN_APPROVAL_MODE,
  getRuntimeAgentLabel,
  type RuntimeAgentIdDto,
  type RuntimeRunActiveControlSnapshotView,
  type RuntimeRunApprovalModeDto,
  type RuntimeRunPendingControlSnapshotView,
  type RuntimeRunView,
  type RuntimeSessionView,
} from '@/src/lib/xero-model/runtime'
import {
  getRuntimeStreamStatusLabel,
  type RuntimeStreamView,
} from '@/src/lib/xero-model/runtime-stream'
import {
  type Phase,
  type ProjectDetailView,
  type ProviderCredentialsSnapshotDto,
  type RuntimeProviderIdDto,
} from '@/src/lib/xero-model'
import { getCloudProviderDefaultProfileId } from '@/src/lib/xero-model/provider-presets'
import {
  composeAgentTrustSnapshot,
  createUnavailableTrustSnapshot,
  mapNotificationChannelHealth,
  mapNotificationRouteViews,
  type BlockedNotificationSyncPollTarget,
} from './notification-health'
import {
  buildComposerModelOptions,
  getAgentMessagesUnavailableCredentialReason,
  getAgentRuntimeRunUnavailableCredentialReason,
  getAgentSessionUnavailableCredentialReason,
  getProviderModelCatalogForProvider,
  isAgentRuntimeBlocked,
  resolveSelectedModel,
  type ComposerModelOptionView,
  type SelectedModelView,
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
  selectedProvider: {
    profileId: string | null
    profileLabel: string | null
    providerId: RuntimeProviderIdDto
    providerLabel: string
    modelId: string | null
    source: 'credential_default' | 'fallback' | 'default'
  }
}

interface AgentRunControlProjection {
  source: 'runtime_run' | 'fallback'
  selectedProviderProfileId: string | null
  selectedRuntimeAgentId: RuntimeAgentIdDto
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
  providerCredentials?: ProviderCredentialsSnapshotDto | null
  runtimeSession: RuntimeSessionView | null
}

export interface BuildAgentViewDependencies {
  project: ProjectDetailView | null
  activePhase: Phase | null
  repositoryStatus: RepositoryStatusView | null
  providerCredentials?: ProviderCredentialsSnapshotDto | null
  runtimeSession: RuntimeSessionView | null
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses?: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors?: Record<string, OperatorActionErrorView | null>
  // Phase 4 legacy: the agent composer's active-profile catalog projection
  // is gone; the union list (composerModelOptions) drives the picker. These
  // fields are kept optional only so the orchestrator's existing wiring keeps
  // compiling while consumers migrate away.
  activeProviderModelCatalog?: ProviderModelCatalogDto | null
  activeProviderModelCatalogLoadStatus?: ProviderModelCatalogLoadStatus
  activeProviderModelCatalogLoadError?: OperatorActionErrorView | null
  runtimeRun: RuntimeRunView | null
  autonomousRun: ProjectDetailView['autonomousRun']
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

function getSelectedProviderProjection(
  selectedModel: SelectedModelView,
): SelectedProviderProjection {
  const providerId = (selectedModel.providerId ?? 'openai_codex') as RuntimeProviderIdDto
  return {
    selectedProvider: {
      profileId: null,
      profileLabel: null,
      providerId,
      providerLabel: selectedModel.providerLabel,
      modelId: selectedModel.modelId,
      source: selectedModel.source === 'runtime_run' ? 'credential_default' : selectedModel.source,
    },
  }
}

function getAgentRunControlProjection(runtimeRun: RuntimeRunView | null): AgentRunControlProjection {
  const activeControls = runtimeRun?.controls?.active ?? null
  const pendingControls = runtimeRun?.controls?.pending ?? null
  const selectedControls = runtimeRun?.controls?.selected ?? null
  const useRuntimeRunTruth = Boolean(selectedControls && !runtimeRun?.isTerminal)

  return {
    source: useRuntimeRunTruth ? 'runtime_run' : 'fallback',
    selectedProviderProfileId: useRuntimeRunTruth ? selectedControls?.providerProfileId ?? null : null,
    selectedRuntimeAgentId: useRuntimeRunTruth
      ? selectedControls?.runtimeAgentId ?? DEFAULT_RUNTIME_AGENT_ID
      : DEFAULT_RUNTIME_AGENT_ID,
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
    'Xero could not compose trust snapshot details from notification/runtime projection data.',
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


function buildOrphanedAgentProviderModel(
  modelId: string,
  providerId: SelectedProviderProjection['selectedProvider']['providerId'],
  providerLabel: string,
  profileId: string | null,
  profileLabel: string | null,
  groupLabel: string,
): AgentProviderModelView | null {
  const trimmedModelId = modelId.trim()
  if (trimmedModelId.length === 0) {
    return null
  }

  return {
    selectionKey: `${providerId}:${trimmedModelId}`,
    profileId,
    profileLabel,
    providerId,
    providerLabel,
    modelId: trimmedModelId,
    label: trimmedModelId,
    displayName: trimmedModelId,
    groupId: profileId ? `${profileId}:current_selection` : providerId === 'github_models' ? 'github_models_current_selection' : 'current_selection',
    groupLabel,
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

function getProviderCatalogStateKeys(
  providerId: string | null | undefined,
  catalog: ProviderModelCatalogDto | null,
): string[] {
  if (!providerId) {
    return []
  }

  const keys = [providerId]
  if (catalog?.profileId && !keys.includes(catalog.profileId)) {
    keys.push(catalog.profileId)
  }

  const defaultProfileId = getCloudProviderDefaultProfileId(providerId)
  if (defaultProfileId && !keys.includes(defaultProfileId)) {
    keys.push(defaultProfileId)
  }

  return keys
}

function firstCatalogStateValue<T>(
  records: Record<string, T> | null | undefined,
  keys: string[],
): T | undefined {
  if (!records) {
    return undefined
  }

  for (const key of keys) {
    if (key in records) {
      return records[key]
    }
  }

  return undefined
}

function getCatalogStateCopy(options: {
  providerId: string
  providerLabel: string
  catalog: ProviderModelCatalogDto | null
  loadStatus: ProviderModelCatalogLoadStatus
  refreshError: OperatorActionErrorView | null
  discoveredModelCount: number
}): Pick<AgentProviderModelCatalogView, 'state' | 'stateLabel' | 'detail'> {
  const ownerLabel = options.providerLabel

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

  if (options.catalog?.source === 'manual' && options.discoveredModelCount > 0) {
    return {
      state: 'live',
      stateLabel: 'Manual catalog',
      detail: `Xero is showing configured model truth for ${ownerLabel} because this provider does not expose a compatible live models listing endpoint.`,
    }
  }

  if (options.discoveredModelCount > 0) {
    return {
      state: 'stale',
      stateLabel: options.catalog?.source === 'cache' ? 'Cached catalog' : 'Stale catalog',
      detail: options.refreshError?.message?.trim()
        ? `${options.refreshError.message} Xero is keeping the last successful model catalog for ${ownerLabel} visible.`
        : `Xero is keeping the last successful model catalog for ${ownerLabel} visible.`,
    }
  }

  if (options.loadStatus === 'loading') {
    return {
      state: 'unavailable',
      stateLabel: 'Catalog unavailable',
      detail: `Loading the active provider-model catalog for ${ownerLabel}. Xero is keeping the configured model visible without sample lists.`,
    }
  }

  return {
    state: 'unavailable',
    stateLabel: 'Catalog unavailable',
    detail: options.refreshError?.message?.trim()
      ? `${options.refreshError.message} Xero is keeping the configured model visible without discovered-model confirmation.`
      : `Xero does not have a discovered model catalog for ${ownerLabel} yet, so only configured model truth remains visible.`,
  }
}

/**
 * Phase 4: credentials-driven catalog projection. Replaces the legacy
 * provider-profile catalog walk. The agent composer's primary picker now
 * reads the union list from `buildComposerModelOptions`; this function
 * only produces the AgentPaneView's `providerModelCatalog` field plus the
 * selected-model details for the run-controls projection.
 */
function buildAgentProviderModelCatalog(options: {
  selectedModel: SelectedModelView
  composerModelOptions: ComposerModelOptionView[]
  providerCredentials: ProviderCredentialsSnapshotDto | null
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses?: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors?: Record<string, OperatorActionErrorView | null>
  selectedModelIdOverride?: string | null
}): {
  providerModelCatalog: AgentProviderModelCatalogView
  selectedModelOption: AgentProviderModelView | null
  selectedModelThinkingEffortOptions: ProviderModelThinkingEffortDto[]
  selectedModelDefaultThinkingEffort: ProviderModelThinkingEffortDto | null
  selectedModelId: string | null
} {
  const providerId = options.selectedModel.providerId
  const providerLabel = options.selectedModel.providerLabel
  const catalog = getProviderModelCatalogForProvider(options.providerModelCatalogs, providerId)
  const catalogStateKeys = getProviderCatalogStateKeys(providerId, catalog)
  const loadStatus =
    providerId
      ? firstCatalogStateValue(options.providerModelCatalogLoadStatuses, catalogStateKeys) ?? 'idle'
      : ('idle' as ProviderModelCatalogLoadStatus)
  const loadError = providerId
    ? firstCatalogStateValue(options.providerModelCatalogLoadErrors, catalogStateKeys) ?? null
    : null
  const refreshError = providerId
    ? getCatalogRefreshError(catalog, loadError)
    : null

  const credentialedProviders = new Set(
    (options.providerCredentials?.credentials ?? []).map((c) => c.providerId),
  )
  // The agent composer's union list (composerModelOptions) is the source of
  // truth for `models`; we adapt it to the legacy AgentProviderModelView shape.
  const models: AgentProviderModelView[] = options.composerModelOptions.map((option) => ({
    selectionKey: option.selectionKey,
    profileId: option.profileId,
    profileLabel: null,
    providerId: option.providerId,
    providerLabel: option.providerLabel,
    modelId: option.modelId,
    label: option.modelId,
    displayName: option.displayName,
    groupId: option.providerId,
    groupLabel: option.providerLabel,
    availability: 'available',
    availabilityLabel: 'Available',
    thinkingSupported: option.thinking.supported,
    thinkingEffortOptions: option.thinkingEffortOptions,
    defaultThinkingEffort: option.defaultThinkingEffort,
  }))

  const configuredModelId =
    options.selectedModelIdOverride?.trim() ||
    options.selectedModel.modelId?.trim() ||
    catalog?.configuredModelId.trim() ||
    null
  const selectedModelId = configuredModelId && configuredModelId.length > 0 ? configuredModelId : null
  const selectionKey =
    providerId && selectedModelId ? `${providerId}:${selectedModelId}` : null
  const discoveredSelected = selectionKey
    ? models.find((model) => model.selectionKey === selectionKey)
    : null
  const selectedModelOption: AgentProviderModelView | null =
    discoveredSelected ??
    (selectedModelId
      ? buildOrphanedAgentProviderModel(
          selectedModelId,
          providerId ?? 'openai_codex',
          providerLabel,
          null,
          null,
          'Current selection',
        )
      : null)

  const finalModels =
    selectedModelOption && selectedModelOption.availability === 'orphaned'
      ? [selectedModelOption, ...models]
      : models

  const aggregateProviderLabel =
    models.length > 0 && credentialedProviders.size > 1 ? 'Configured providers' : providerLabel

  // State copy: empty when no credentials, otherwise live/cache/manual based on
  // the selected provider's catalog.
  const stateCopy = options.selectedModel.hasCredential
    ? getCatalogStateCopy({
        providerId: providerId ?? 'openai_codex',
        providerLabel,
        catalog,
        loadStatus,
        refreshError,
        discoveredModelCount: models.length,
      })
    : {
        state: 'unavailable' as const,
        stateLabel: 'Catalog unavailable',
        detail: `Add a ${providerLabel} credential in Settings to discover models for this provider.`,
      }

  return {
    providerModelCatalog: {
      profileId: null,
      profileLabel: null,
      providerId: providerId ?? 'openai_codex',
      providerLabel: aggregateProviderLabel,
      source: catalog?.source ?? null,
      loadStatus,
      state: stateCopy.state,
      stateLabel: stateCopy.stateLabel,
      detail: stateCopy.detail,
      fetchedAt: getProviderModelCatalogFetchedAt(catalog),
      lastSuccessAt: catalog?.lastSuccessAt ?? null,
      lastRefreshError: refreshError,
      models: finalModels,
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
  providerCredentials = null,
  runtimeSession,
}: BuildWorkflowViewDependencies): WorkflowPaneView | null {
  if (!project) {
    return null
  }

  const selectedModel = resolveSelectedModel(providerCredentials, null)
  const { selectedProvider } = getSelectedProviderProjection(selectedModel)
  const hasAnyReadyProvider = (providerCredentials?.credentials.length ?? 0) > 0

  return {
    project,
    activePhase,
    overallPercent: project.phaseProgressPercent,
    hasPhases: project.phases.length > 0,
    runtimeSession,
    selectedProfileId: selectedProvider.profileId,
    selectedProfileLabel: selectedProvider.profileLabel,
    selectedProviderId: selectedProvider.providerId,
    selectedProviderLabel: selectedProvider.providerLabel,
    selectedProviderSource: selectedProvider.source,
    selectedModelId: selectedProvider.modelId,
    hasAnyReadyProvider,
    providerMismatch: false,
    providerMismatchReason: null,
    providerMismatchRecoveryCopy: null,
  }
}

export function buildAgentView({
  project,
  activePhase,
  repositoryStatus,
  providerCredentials = null,
  runtimeSession,
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  providerModelCatalogLoadErrors,
  runtimeRun,
  autonomousRun,
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

  // Selected model + composer options + blocked flag (credentials-driven).
  const selectedRunControls = runtimeRun?.controls.selected ?? null
  const selectedModel: SelectedModelView = resolveSelectedModel(
    providerCredentials,
    selectedRunControls,
    { runtimeRun },
  )
  const { selectedProvider } = getSelectedProviderProjection(selectedModel)
  const composerModelOptions: ComposerModelOptionView[] = buildComposerModelOptions(
    providerCredentials,
    providerModelCatalogs,
  )
  const agentRuntimeBlocked =
    providerCredentials !== null
      ? isAgentRuntimeBlocked(providerCredentials, selectedModel)
      : undefined
  const controlProjection = getAgentRunControlProjection(runtimeRun)
  const providerModelCatalogProjection = buildAgentProviderModelCatalog({
    selectedModel,
    composerModelOptions,
    providerCredentials,
    providerModelCatalogs,
    providerModelCatalogLoadStatuses,
    providerModelCatalogLoadErrors,
    selectedModelIdOverride: controlProjection.selectedModelId,
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
  const selectedRuntimeAgentId = controlProjection.selectedRuntimeAgentId

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
      selectedRuntimeAgentId,
      selectedRuntimeAgentLabel: getRuntimeAgentLabel(selectedRuntimeAgentId),
      selectedModelId,
      selectedModelSelectionKey: providerModelCatalogProjection.selectedModelOption?.selectionKey ?? null,
      selectedThinkingEffort,
      selectedApprovalMode,
      selectedPrompt: controlProjection.selectedPrompt,
      runtimeRunActiveControls: controlProjection.activeControls,
      runtimeRunPendingControls: controlProjection.pendingControls,
      providerModelCatalog: providerModelCatalogProjection.providerModelCatalog,
      selectedModelOption: providerModelCatalogProjection.selectedModelOption,
      selectedModelThinkingEffortOptions: providerModelCatalogProjection.selectedModelThinkingEffortOptions,
      selectedModelDefaultThinkingEffort: providerModelCatalogProjection.selectedModelDefaultThinkingEffort,
      hasAnyReadyProvider: (providerCredentials?.credentials.length ?? 0) > 0,
      providerMismatch: false,
      providerMismatchReason: null,
      providerMismatchRecoveryCopy: null,
      selectedModel,
      agentRuntimeBlocked,
      composerModelOptions,
      runtimeRun,
      autonomousRun,
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
      sessionUnavailableReason: getAgentSessionUnavailableCredentialReason(
        runtimeSession,
        runtimeErrorMessage,
        selectedModel,
        agentRuntimeBlocked ?? false,
      ),
      runtimeRunUnavailableReason: getAgentRuntimeRunUnavailableCredentialReason(
        runtimeRun,
        runtimeRunErrorMessage,
        runtimeSession,
        agentRuntimeBlocked ?? false,
      ),
      messagesUnavailableReason: getAgentMessagesUnavailableCredentialReason(
        runtimeSession,
        runtimeStream,
        runtimeRun,
        agentRuntimeBlocked ?? false,
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
