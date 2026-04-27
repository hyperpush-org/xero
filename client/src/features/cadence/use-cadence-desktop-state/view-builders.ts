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
  type ProviderProfileDto,
  type ProviderProfilesDto,
} from '@/src/lib/cadence-model'
import { projectCheckpointControlLoops } from '../agent-runtime-projections/checkpoint-control-loops'
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
  isKnownRuntimeProviderId,
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
  selectedProviderProfileId: string | null
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

type ComposerProviderProfile = Pick<ProviderProfileDto, 'profileId' | 'providerId' | 'label' | 'modelId'>

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
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses?: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors?: Record<string, OperatorActionErrorView | null>
  activeProviderModelCatalog: ProviderModelCatalogDto | null
  activeProviderModelCatalogLoadStatus: ProviderModelCatalogLoadStatus
  activeProviderModelCatalogLoadError: OperatorActionErrorView | null
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
    selectedProviderProfileId: useRuntimeRunTruth ? selectedControls?.providerProfileId ?? null : null,
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

function createProviderModelSelectionKey(profileId: string | null | undefined, modelId: string): string {
  const normalizedProfileId = profileId?.trim() || 'unscoped'
  return `${encodeURIComponent(normalizedProfileId)}::${encodeURIComponent(modelId.trim())}`
}

function getProfileDisplayLabel(profile: ComposerProviderProfile): string {
  const label = profile.label.trim()
  return label.length > 0 ? label : profile.profileId
}

function getProviderProfileGroupLabel(profile: ComposerProviderProfile): string {
  const providerLabel = getRuntimeProviderLabel(profile.providerId)
  const profileLabel = getProfileDisplayLabel(profile)
  return profileLabel === providerLabel ? providerLabel : `${profileLabel} · ${providerLabel}`
}

function getCatalogProjectionProvider(options: {
  selectedProvider: SelectedProviderProjection['selectedProvider']
  runtimeRun: RuntimeRunView | null
  controlTruthSource: AgentRunControlProjection['source']
}): SelectedProviderProjection['selectedProvider'] {
  const runtimeRunProviderId = options.runtimeRun?.providerId?.trim() ?? ''
  if (
    options.controlTruthSource !== 'runtime_run' ||
    runtimeRunProviderId.length === 0 ||
    !isKnownRuntimeProviderId(runtimeRunProviderId) ||
    runtimeRunProviderId === options.selectedProvider.providerId
  ) {
    return options.selectedProvider
  }

  return {
    ...options.selectedProvider,
    profileId: null,
    profileLabel: null,
    providerId: runtimeRunProviderId,
    providerLabel: getRuntimeProviderLabel(runtimeRunProviderId),
  }
}

function getGitHubScopedGroupLabel(providerLabel: string, groupLabel: string): string {
  return providerLabel === groupLabel ? providerLabel : `${providerLabel} · ${groupLabel}`
}

function getModelGroupLabel(
  modelId: string,
  providerId: SelectedProviderProjection['selectedProvider']['providerId'],
  providerLabel: string,
): { groupId: string; groupLabel: string } {
  const normalizedProviderLabel = providerLabel.trim().toLowerCase().replace(/[^a-z0-9]+/g, '_') || 'provider_models'
  const trimmedModelId = modelId.trim()
  const namespace = trimmedModelId.includes('/') ? trimmedModelId.split('/')[0]?.trim() ?? '' : ''
  if (namespace.length === 0) {
    return {
      groupId: normalizedProviderLabel,
      groupLabel: providerLabel,
    }
  }

  const normalizedNamespace = namespace.toLowerCase()
  const knownLabel = MODEL_GROUP_LABELS[normalizedNamespace] ?? getRuntimeProviderLabel(namespace)
  const groupId = normalizedNamespace.replace(/[^a-z0-9]+/g, '_')

  if (providerId === 'github_models') {
    return {
      groupId: `github_models_${groupId}`,
      groupLabel: getGitHubScopedGroupLabel(providerLabel, knownLabel),
    }
  }

  return {
    groupId,
    groupLabel: knownLabel,
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
  profile: ComposerProviderProfile,
  groupLabelOverride?: string,
): AgentProviderModelView | null {
  const modelId = model.modelId.trim()
  if (modelId.length === 0) {
    return null
  }

  const providerLabel = getRuntimeProviderLabel(profile.providerId)
  const effortOptions = getThinkingEffortOptions(model)
  const defaultThinkingEffort =
    model.thinking.supported && model.thinking.defaultEffort && effortOptions.includes(model.thinking.defaultEffort)
      ? model.thinking.defaultEffort
      : effortOptions[0] ?? null
  const modelGroup = getModelGroupLabel(modelId, profile.providerId, providerLabel)
  const groupLabel = groupLabelOverride ?? getProviderProfileGroupLabel(profile)

  return {
    selectionKey: createProviderModelSelectionKey(profile.profileId, modelId),
    profileId: profile.profileId,
    profileLabel: getProfileDisplayLabel(profile),
    providerId: profile.providerId,
    providerLabel,
    modelId,
    label: modelId,
    displayName: model.displayName.trim() || modelId,
    groupId: groupLabelOverride ? modelGroup.groupId : profile.profileId,
    groupLabel,
    availability: 'available',
    availabilityLabel: 'Available',
    thinkingSupported: effortOptions.length > 0,
    thinkingEffortOptions: effortOptions,
    defaultThinkingEffort,
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
    selectionKey: createProviderModelSelectionKey(profileId, trimmedModelId),
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

  if (options.catalog?.source === 'manual' && options.discoveredModelCount > 0) {
    return {
      state: 'live',
      stateLabel: 'Manual catalog',
      detail: `Cadence is showing configured model truth for ${ownerLabel} because this provider does not expose a compatible live models listing endpoint.`,
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
  providerProfiles: ProviderProfilesDto | null
  providerModelCatalogs?: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses?: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors?: Record<string, OperatorActionErrorView | null>
  activeProviderModelCatalog: ProviderModelCatalogDto | null
  activeProviderModelCatalogLoadStatus: ProviderModelCatalogLoadStatus
  activeProviderModelCatalogLoadError: OperatorActionErrorView | null
  selectedProviderProfileIdOverride?: string | null
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
  const activeCatalog = allowCatalogTruth ? options.activeProviderModelCatalog : null
  const catalogRecords = {
    ...(options.providerModelCatalogs ?? {}),
  }
  if (activeCatalog) {
    catalogRecords[activeCatalog.profileId] = activeCatalog
  }
  const loadStatusRecords = {
    ...(options.providerModelCatalogLoadStatuses ?? {}),
  }
  if (activeCatalog) {
    loadStatusRecords[activeCatalog.profileId] = options.activeProviderModelCatalogLoadStatus
  }
  const loadErrorRecords = {
    ...(options.providerModelCatalogLoadErrors ?? {}),
  }
  if (activeCatalog) {
    loadErrorRecords[activeCatalog.profileId] = options.activeProviderModelCatalogLoadError
  }
  const discoveredModels: AgentProviderModelView[] = []
  const seenSelectionKeys = new Set<string>()
  const readyProfiles =
    options.providerProfiles?.profiles.filter((profile) => profile.readiness.ready) ?? []
  const activeProfile = options.providerProfiles?.profiles.find((profile) => profile.active) ?? null
  const fallbackProfile: ComposerProviderProfile | null =
    readyProfiles.length === 0
      ? options.selectedProvider.profileId
        ? {
            profileId: options.selectedProvider.profileId,
            providerId: options.selectedProvider.providerId,
            label: options.selectedProvider.profileLabel ?? options.selectedProvider.providerLabel,
            modelId: options.selectedProvider.modelId ?? '',
          }
        : activeCatalog
          ? {
              profileId: activeCatalog.profileId,
              providerId: activeCatalog.providerId,
              label: options.selectedProvider.profileLabel ?? getRuntimeProviderLabel(activeCatalog.providerId),
              modelId: activeCatalog.configuredModelId,
            }
          : null
      : null
  const composerProfiles: ComposerProviderProfile[] =
    readyProfiles.length > 0 ? readyProfiles : fallbackProfile ? [fallbackProfile] : []
  let representativeCatalog: ProviderModelCatalogDto | null = null
  let representativeLoadStatus: ProviderModelCatalogLoadStatus = 'idle'
  let representativeRefreshError: OperatorActionErrorView | null = null

  for (const profile of composerProfiles) {
    const catalog = allowCatalogTruth ? catalogRecords[profile.profileId] ?? null : null
    const loadStatus = loadStatusRecords[profile.profileId] ?? 'idle'
    const refreshError = allowCatalogTruth
      ? getCatalogRefreshError(catalog, loadErrorRecords[profile.profileId] ?? null)
      : null

    if (
      !representativeCatalog &&
      (profile.profileId === options.selectedProviderProfileIdOverride ||
        profile.profileId === options.selectedProvider.profileId ||
        profile.profileId === activeProfile?.profileId ||
        catalog)
    ) {
      representativeCatalog = catalog
      representativeLoadStatus = loadStatus
      representativeRefreshError = refreshError
    }

    for (const model of catalog?.models ?? []) {
      const nextModel = buildAgentProviderModel(model, profile)
      if (!nextModel || seenSelectionKeys.has(nextModel.selectionKey)) {
        continue
      }

      seenSelectionKeys.add(nextModel.selectionKey)
      discoveredModels.push(nextModel)
    }
  }

  representativeCatalog ??= activeCatalog
  representativeLoadStatus =
    representativeCatalog?.profileId && loadStatusRecords[representativeCatalog.profileId]
      ? loadStatusRecords[representativeCatalog.profileId]
      : representativeLoadStatus
  representativeRefreshError ??=
    representativeCatalog?.profileId
      ? getCatalogRefreshError(representativeCatalog, loadErrorRecords[representativeCatalog.profileId] ?? null)
      : null
  const selectedProfileId =
    options.selectedProviderProfileIdOverride?.trim() ||
    options.selectedProvider.profileId?.trim() ||
    (allowCatalogTruth ? activeProfile?.profileId : null) ||
    representativeCatalog?.profileId ||
    null
  const selectedProfile =
    composerProfiles.find((profile) => profile.profileId === selectedProfileId) ??
    (selectedProfileId
      ? ({
          profileId: selectedProfileId,
          providerId: representativeCatalog?.providerId ?? options.selectedProvider.providerId,
          label: options.selectedProvider.profileLabel ?? options.selectedProvider.providerLabel,
          modelId: options.selectedProvider.modelId ?? representativeCatalog?.configuredModelId ?? '',
        } satisfies ComposerProviderProfile)
      : null)
  const selectedProviderLabel = selectedProfile
    ? getRuntimeProviderLabel(selectedProfile.providerId)
    : options.selectedProvider.providerLabel
  const selectedProfileLabel = selectedProfile ? getProfileDisplayLabel(selectedProfile) : options.selectedProvider.profileLabel
  const configuredModelId =
    options.selectedModelIdOverride?.trim() ||
    (selectedProfile?.profileId ? catalogRecords[selectedProfile.profileId]?.configuredModelId.trim() : '') ||
    selectedProfile?.modelId?.trim() ||
    representativeCatalog?.configuredModelId.trim() ||
    options.selectedProvider.modelId?.trim() ||
    null
  const selectedModelId = configuredModelId && configuredModelId.length > 0 ? configuredModelId : null
  const selectedSelectionKey =
    selectedProfile && selectedModelId
      ? createProviderModelSelectionKey(selectedProfile.profileId, selectedModelId)
      : selectedModelId
  const selectedDiscoveredModel =
    selectedSelectionKey
      ? discoveredModels.find((model) => model.selectionKey === selectedSelectionKey) ??
        discoveredModels.find((model) => model.modelId === selectedModelId)
      : null
  const orphanGroupLabel = selectedProfile
    ? getProviderProfileGroupLabel(selectedProfile)
    : options.selectedProvider.providerId === 'github_models'
      ? getGitHubScopedGroupLabel(selectedProviderLabel, 'Current selection')
      : 'Current selection'
  const selectedModelOption =
    selectedDiscoveredModel ??
    (selectedModelId
      ? buildOrphanedAgentProviderModel(
          selectedModelId,
          selectedProfile?.providerId ?? options.selectedProvider.providerId,
          selectedProviderLabel,
          selectedProfile?.profileId ?? options.selectedProvider.profileId,
          selectedProfileLabel ?? null,
          orphanGroupLabel,
        )
      : null)
  const models = selectedModelOption && selectedModelOption.availability === 'orphaned'
    ? [selectedModelOption, ...discoveredModels]
    : discoveredModels
  const stateCopy = allowCatalogTruth
    ? getCatalogStateCopy({
        selectedProvider: options.selectedProvider,
        catalog: representativeCatalog,
        loadStatus: representativeLoadStatus,
        refreshError: representativeRefreshError,
        discoveredModelCount: discoveredModels.length,
      })
    : {
        state: 'unavailable' as const,
        stateLabel: 'Catalog unavailable',
        detail: `Cadence is showing durable ${options.selectedProvider.providerLabel} run-scoped control truth from the active supervised run while keeping the current Settings selection out of the live projection.`,
      }
  const aggregateProviderId =
    representativeCatalog?.providerId ?? selectedProfile?.providerId ?? options.selectedProvider.providerId
  const aggregateProviderLabel =
    discoveredModels.length > 0 && composerProfiles.length > 1
      ? 'Configured providers'
      : getRuntimeProviderLabel(aggregateProviderId)

  return {
    providerModelCatalog: {
      profileId: selectedProfile?.profileId ?? representativeCatalog?.profileId ?? options.selectedProvider.profileId,
      profileLabel: selectedProfileLabel ?? options.selectedProvider.profileLabel,
      providerId: aggregateProviderId,
      providerLabel: aggregateProviderLabel,
      source: representativeCatalog?.source ?? null,
      loadStatus: representativeLoadStatus,
      state: stateCopy.state,
      stateLabel: stateCopy.stateLabel,
      detail: stateCopy.detail,
      fetchedAt: getProviderModelCatalogFetchedAt(representativeCatalog),
      lastSuccessAt: representativeCatalog?.lastSuccessAt ?? null,
      lastRefreshError: representativeRefreshError,
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

  const { selectedProvider, providerMismatch, providerMismatchCopy } = getSelectedProviderProjection(
    providerProfiles,
    runtimeSettings,
    runtimeSession,
  )
  const hasAnyReadyProvider = providerProfiles?.profiles.some((profile) => profile.readiness.ready) ?? false

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
    selectedProfileReadiness: selectedProvider.readiness,
    openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
    hasAnyReadyProvider,
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
  providerModelCatalogs,
  providerModelCatalogLoadStatuses,
  providerModelCatalogLoadErrors,
  activeProviderModelCatalog,
  activeProviderModelCatalogLoadStatus,
  activeProviderModelCatalogLoadError,
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

  const checkpointControlLoop = projectCheckpointControlLoops({
    actionRequiredItems: runtimeStream?.actionRequired ?? [],
    approvalRequests: project.approvalRequests,
    resumeHistory: project.resumeHistory,
    notificationBroker: project.notificationBroker,
  })
  const { selectedProvider, providerMismatch, providerMismatchCopy } = getSelectedProviderProjection(
    providerProfiles,
    runtimeSettings,
    runtimeSession,
  )
  const controlProjection = getAgentRunControlProjection(runtimeRun)
  const catalogProjectionProvider = getCatalogProjectionProvider({
    selectedProvider,
    runtimeRun,
    controlTruthSource: controlProjection.source,
  })
  const allowCatalogTruth =
    controlProjection.source === 'fallback' || runtimeRun?.providerId === selectedProvider.providerId
  const providerModelCatalogProjection = buildAgentProviderModelCatalog({
    selectedProvider: catalogProjectionProvider,
    providerProfiles,
    providerModelCatalogs,
    providerModelCatalogLoadStatuses,
    providerModelCatalogLoadErrors,
    activeProviderModelCatalog,
    activeProviderModelCatalogLoadStatus,
    activeProviderModelCatalogLoadError,
    selectedProviderProfileIdOverride: controlProjection.selectedProviderProfileId,
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
      selectedProfileReadiness: selectedProvider.readiness,
      openrouterApiKeyConfigured: selectedProvider.openrouterApiKeyConfigured,
      hasAnyReadyProvider:
        providerProfiles?.profiles.some((profile) => profile.readiness.ready) ?? false,
      providerMismatch,
      providerMismatchReason: providerMismatchCopy?.reason ?? null,
      providerMismatchRecoveryCopy: providerMismatchCopy?.sessionRecoveryCopy ?? null,
      runtimeRun,
      autonomousRun,
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
