import type { XeroDesktopAdapter } from '@/src/lib/xero-desktop'
import type {
  CreateProjectEntryRequestDto,
  CreateProjectEntryResponseDto,
  XeroDoctorReportDto,
  DeleteProjectEntryResponseDto,
  ImportMcpServersResponseDto,
  ListProjectFilesResponseDto,
  MoveProjectEntryRequestDto,
  MoveProjectEntryResponseDto,
  McpImportDiagnosticDto,
  McpRegistryDto,
  NotificationRouteCredentialReadinessDto,
  NotificationRouteDto,
  NotificationRouteKindDto,
  OperatorApprovalView,
  Phase,
  ProjectDetailView,
  ProjectListItem,
  ProjectUsageSummaryDto,
  ProviderModelCatalogDto,
  ProviderModelCatalogSourceDto,
  ProviderModelThinkingEffortDto,
  ProviderCredentialDto,
  ProviderCredentialsSnapshotDto,
  ProviderAuthSessionView,
  ProviderProfileDiagnosticsDto,
  ReadProjectFileResponseDto,
  RenameProjectEntryRequestDto,
  RenameProjectEntryResponseDto,
  ReplaceInProjectRequestDto,
  ReplaceInProjectResponseDto,
  SearchProjectRequestDto,
  SearchProjectResponseDto,
  RepositoryDiffScope,
  RepositoryDiffView,
  RepositoryStatusEntryView,
  RepositoryStatusView,
  ResumeHistoryEntryView,
  RuntimeAuthPhaseDto,
  RuntimeAutoCompactPreferenceDto,
  RuntimeAgentIdDto,
  RuntimeRunActiveControlSnapshotView,
  RuntimeRunApprovalModeDto,
  RuntimeRunControlInputDto,
  RuntimeRunPendingControlSnapshotView,
  RuntimeRunView,
  RuntimeProviderIdDto,
  RuntimeSessionView,
  RunDoctorReportRequestDto,
  RuntimeStreamActionRequiredItemView,
  RuntimeStreamActivityItemView,
  RuntimeStreamIssueView,
  RuntimeStreamSkillItemView,
  RuntimeStreamStatus,
  RuntimeStreamView,
  RuntimeStreamViewItem,
  StagedAgentAttachmentDto,
  SyncNotificationAdaptersResponseDto,
  ListSkillRegistryRequestDto,
  RemovePluginRequestDto,
  RemovePluginRootRequestDto,
  RemoveSkillLocalRootRequestDto,
  RemoveSkillRequestDto,
  SetPluginEnabledRequestDto,
  SetSkillEnabledRequestDto,
  SkillRegistryDto,
  UpdateGithubSkillSourceRequestDto,
  UpdateProjectSkillSourceRequestDto,
  UpsertPluginRootRequestDto,
  UpsertSkillLocalRootRequestDto,
  UpsertMcpServerRequestDto,
  UpsertNotificationRouteRequestDto,
  UpsertProviderCredentialRequestDto,
  VerificationRecordView,
  WriteProjectFileResponseDto,
} from '@/src/lib/xero-model'
import type {
  ComposerModelOptionView,
  SelectedModelView,
  SelectedRuntimeProviderSource,
} from './runtime-provider'
import type { XeroHighChurnStore } from './high-churn-store'

export type RefreshSource =
  | 'startup'
  | 'selection'
  | 'import'
  | 'remove'
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
export type ProjectRemovalStatus = 'idle' | 'running'
export type AutonomousRunActionKind = 'start' | 'cancel' | 'inspect'
export type AutonomousRunActionStatus = 'idle' | 'running'
export type RuntimeRunActionKind = 'start' | 'update_controls' | 'stop'
export type RuntimeRunActionStatus = 'idle' | 'running'

export interface OperatorActionErrorView {
  code: string
  message: string
  retryable: boolean
}

export type RepositoryDiffLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type NotificationRoutesLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type NotificationRouteMutationStatus = 'idle' | 'running'
export type ProviderCredentialsLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type ProviderCredentialsSaveStatus = 'idle' | 'running'
export type ProviderModelCatalogLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type DoctorReportRunStatus = 'idle' | 'running' | 'ready' | 'error'
export type McpRegistryLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type McpRegistryMutationStatus = 'idle' | 'running'
export type SkillRegistryLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type SkillRegistryMutationStatus = 'idle' | 'running'
export type NotificationRouteHealthState = 'disabled' | 'idle' | 'pending' | 'healthy' | 'degraded'
export type AgentTrustSignalState = 'healthy' | 'degraded' | 'unavailable'
export type AgentRunControlTruthSource = 'runtime_run' | 'fallback'

export interface RuntimeRunControlMutationRequest {
  controls?: RuntimeRunControlInputDto | null
  prompt?: string | null
  attachments?: StagedAgentAttachmentDto[]
  autoCompact?: RuntimeAutoCompactPreferenceDto | null
}

export interface AgentRunControlSelectionView {
  source: AgentRunControlTruthSource
  runtimeAgentId: RuntimeAgentIdDto
  modelId: string | null
  thinkingEffort: ProviderModelThinkingEffortDto | null
  approvalMode: RuntimeRunApprovalModeDto
}

export interface AgentRunPromptView {
  text: string | null
  queuedAt: string | null
  hasQueuedPrompt: boolean
}

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

export type AgentProviderModelCatalogState = 'live' | 'stale' | 'unavailable'
export type AgentProviderModelAvailability = 'available' | 'orphaned'

export interface AgentProviderModelView {
  selectionKey: string
  profileId: string | null
  profileLabel: string | null
  providerId: ProviderModelCatalogDto['providerId']
  providerLabel: string
  modelId: string
  label: string
  displayName: string
  groupId: string
  groupLabel: string
  availability: AgentProviderModelAvailability
  availabilityLabel: string
  thinkingSupported: boolean
  thinkingEffortOptions: ProviderModelThinkingEffortDto[]
  defaultThinkingEffort: ProviderModelThinkingEffortDto | null
}

export interface AgentProviderModelCatalogView {
  profileId: string | null
  profileLabel: string | null
  providerId: ProviderModelCatalogDto['providerId']
  providerLabel: string
  source: ProviderModelCatalogSourceDto | null
  loadStatus: ProviderModelCatalogLoadStatus
  state: AgentProviderModelCatalogState
  stateLabel: string
  detail: string
  fetchedAt: string | null
  lastSuccessAt: string | null
  lastRefreshError: OperatorActionErrorView | null
  models: AgentProviderModelView[]
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

export interface UseXeroDesktopStateOptions {
  adapter?: XeroDesktopAdapter
  /**
   * Runtime stream items are high-frequency UI data. The full subscription
   * stays enabled by default for direct hook consumers and tests; app shells
   * can opt out and subscribe from the visible runtime pane instead.
   */
  subscribeRuntimeStreams?: boolean
  /**
   * Repository status can be consumed through selectors when only badges need
   * updates. Defaulting to the legacy full subscription keeps direct callers
   * straightforward while the app migrates leaf-by-leaf.
   */
  subscribeRepositoryStatus?: boolean
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
  overallPercent: number
  hasPhases: boolean
  runtimeSession?: RuntimeSessionView | null
  selectedProfileId?: string | null
  selectedProfileLabel?: string | null
  selectedProviderId?: RuntimeProviderIdDto | null
  selectedProviderLabel?: string
  selectedProviderSource?: SelectedRuntimeProviderSource | null
  selectedModelId?: string | null
  hasAnyReadyProvider?: boolean
  providerMismatch?: boolean
  providerMismatchReason?: string | null
  providerMismatchRecoveryCopy?: string | null
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
  selectedProfileId?: string | null
  selectedProfileLabel?: string | null
  selectedProviderId?: RuntimeProviderIdDto | null
  selectedProviderLabel?: string
  selectedProviderSource?: SelectedRuntimeProviderSource | null
  controlTruthSource: AgentRunControlTruthSource
  selectedRuntimeAgentId: RuntimeAgentIdDto
  selectedRuntimeAgentLabel: string
  selectedModelId?: string | null
  selectedModelSelectionKey?: string | null
  selectedThinkingEffort: ProviderModelThinkingEffortDto | null
  selectedApprovalMode: RuntimeRunApprovalModeDto
  selectedPrompt: AgentRunPromptView
  runtimeRunActiveControls: RuntimeRunActiveControlSnapshotView | null
  runtimeRunPendingControls: RuntimeRunPendingControlSnapshotView | null
  providerModelCatalog: AgentProviderModelCatalogView
  selectedModelOption: AgentProviderModelView | null
  selectedModelThinkingEffortOptions: ProviderModelThinkingEffortDto[]
  selectedModelDefaultThinkingEffort: ProviderModelThinkingEffortDto | null
  hasAnyReadyProvider?: boolean
  providerMismatch?: boolean
  providerMismatchReason?: string | null
  providerMismatchRecoveryCopy?: string | null
  selectedModel?: SelectedModelView
  agentRuntimeBlocked?: boolean
  composerModelOptions?: ComposerModelOptionView[]
  runtimeRun?: RuntimeRunView | null
  autonomousRun?: ProjectDetailView['autonomousRun']
  runtimeErrorMessage?: string | null
  runtimeRunErrorMessage?: string | null
  autonomousRunErrorMessage?: string | null
  authPhase?: RuntimeAuthPhaseDto | null
  authPhaseLabel?: string
  runtimeStream?: RuntimeStreamView | null
  runtimeStreamStatus?: RuntimeStreamStatus
  runtimeStreamStatusLabel?: string
  runtimeStreamError?: RuntimeStreamIssueView | null
  runtimeStreamItems?: RuntimeStreamViewItem[]
  skillItems?: RuntimeStreamSkillItemView[]
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
  notificationSyncPollingActive: boolean
  notificationSyncPollingActionId: string | null
  notificationSyncPollingBoundaryId: string | null
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
  autonomousRunActionStatus: AutonomousRunActionStatus
  pendingAutonomousRunAction: AutonomousRunActionKind | null
  autonomousRunActionError: OperatorActionErrorView | null
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
  sessionUnavailableReason: string
  runtimeRunUnavailableReason: string
  messagesUnavailableReason: string
}

export interface AgentWorkspacePaneSlot {
  id: string
  agentSessionId: string | null
}

export type AgentWorkspaceSidebarMode = 'pinned' | 'collapsed'

export interface AgentWorkspaceLayoutState {
  paneSlots: AgentWorkspacePaneSlot[]
  focusedPaneId: string
  splitterRatios: Record<string, number[]>
  preSpawnSidebarMode: AgentWorkspaceSidebarMode | null
}

export interface AgentWorkspacePaneView {
  paneId: string
  agentSessionId: string | null
  agent: AgentPaneView
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

export interface NotificationRoutesLoadResult {
  routes: NotificationRouteDto[]
  loadError: OperatorActionErrorView | null
}

export interface UseXeroDesktopStateResult {
  highChurnStore: XeroHighChurnStore
  projects: ProjectListItem[]
  activeProject: ProjectDetailView | null
  activeProjectId: string | null
  pendingProjectSelectionId: string | null
  repositoryStatus: RepositoryStatusView | null
  workflowView: WorkflowPaneView | null
  agentView: AgentPaneView | null
  agentWorkspaceLayout: AgentWorkspaceLayoutState | null
  agentWorkspacePanes: AgentWorkspacePaneView[]
  executionView: ExecutionPaneView | null
  repositoryDiffs: Record<RepositoryDiffScope, RepositoryDiffState>
  activeDiffScope: RepositoryDiffScope
  activeRepositoryDiff: RepositoryDiffState
  isLoading: boolean
  isProjectLoading: boolean
  isImporting: boolean
  projectRemovalStatus: ProjectRemovalStatus
  pendingProjectRemovalId: string | null
  errorMessage: string | null
  providerCredentials: ProviderCredentialsSnapshotDto | null
  providerCredentialsLoadStatus: ProviderCredentialsLoadStatus
  providerCredentialsLoadError: OperatorActionErrorView | null
  providerCredentialsSaveStatus: ProviderCredentialsSaveStatus
  providerCredentialsSaveError: OperatorActionErrorView | null
  providerModelCatalogs: Record<string, ProviderModelCatalogDto>
  providerModelCatalogLoadStatuses: Record<string, ProviderModelCatalogLoadStatus>
  providerModelCatalogLoadErrors: Record<string, OperatorActionErrorView | null>
  activeProviderModelCatalog: ProviderModelCatalogDto | null
  activeProviderModelCatalogLoadStatus: ProviderModelCatalogLoadStatus
  activeProviderModelCatalogLoadError: OperatorActionErrorView | null
  doctorReport: XeroDoctorReportDto | null
  doctorReportStatus: DoctorReportRunStatus
  doctorReportError: OperatorActionErrorView | null
  mcpRegistry: McpRegistryDto | null
  mcpImportDiagnostics: McpImportDiagnosticDto[]
  mcpRegistryLoadStatus: McpRegistryLoadStatus
  mcpRegistryLoadError: OperatorActionErrorView | null
  mcpRegistryMutationStatus: McpRegistryMutationStatus
  pendingMcpServerId: string | null
  mcpRegistryMutationError: OperatorActionErrorView | null
  skillRegistry: SkillRegistryDto | null
  skillRegistryLoadStatus: SkillRegistryLoadStatus
  skillRegistryLoadError: OperatorActionErrorView | null
  skillRegistryMutationStatus: SkillRegistryMutationStatus
  pendingSkillSourceId: string | null
  skillRegistryMutationError: OperatorActionErrorView | null
  refreshSource: RefreshSource
  isDesktopRuntime: boolean
  operatorActionStatus: OperatorActionStatus
  pendingOperatorActionId: string | null
  operatorActionError: OperatorActionErrorView | null
  autonomousRunActionStatus: AutonomousRunActionStatus
  pendingAutonomousRunAction: AutonomousRunActionKind | null
  autonomousRunActionError: OperatorActionErrorView | null
  runtimeRunActionStatus: RuntimeRunActionStatus
  pendingRuntimeRunAction: RuntimeRunActionKind | null
  runtimeRunActionError: OperatorActionErrorView | null
  selectProject: (projectId: string) => Promise<void>
  prefetchProject: (projectId: string) => void
  importProject: (path?: string) => Promise<boolean>
  createProject: (parentPath: string, name: string) => Promise<boolean>
  removeProject: (projectId: string) => Promise<void>
  retry: () => Promise<void>
  showRepositoryDiff: (scope: RepositoryDiffScope, options?: { force?: boolean }) => Promise<void>
  retryActiveRepositoryDiff: () => Promise<void>
  listProjectFiles: (projectId: string, path?: string) => Promise<ListProjectFilesResponseDto>
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  writeProjectFile: (projectId: string, path: string, content: string) => Promise<WriteProjectFileResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  moveProjectEntry: (request: MoveProjectEntryRequestDto) => Promise<MoveProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
  searchProject: (request: SearchProjectRequestDto) => Promise<SearchProjectResponseDto>
  replaceInProject: (request: ReplaceInProjectRequestDto) => Promise<ReplaceInProjectResponseDto>
  startOpenAiLogin: (options?: { originator?: string | null }) => Promise<ProviderAuthSessionView | null>
  submitOpenAiCallback: (flowId: string, options?: { manualInput?: string | null }) => Promise<ProviderAuthSessionView | null>
  startAutonomousRun: () => Promise<ProjectDetailView['autonomousRun'] | null>
  inspectAutonomousRun: () => Promise<ProjectDetailView['autonomousRun'] | null>
  cancelAutonomousRun: (runId: string) => Promise<ProjectDetailView['autonomousRun'] | null>
  startRuntimeRun: (options?: RuntimeRunControlMutationRequest) => Promise<RuntimeRunView | null>
  updateRuntimeRunControls: (request?: RuntimeRunControlMutationRequest) => Promise<RuntimeRunView | null>
  startRuntimeSession: (options?: { providerProfileId?: string | null }) => Promise<RuntimeSessionView | null>
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
  refreshProviderCredentials: (options?: { force?: boolean }) => Promise<ProviderCredentialsSnapshotDto>
  upsertProviderCredential: (
    request: UpsertProviderCredentialRequestDto,
  ) => Promise<ProviderCredentialsSnapshotDto>
  deleteProviderCredential: (
    providerId: ProviderCredentialDto['providerId'],
  ) => Promise<ProviderCredentialsSnapshotDto>
  startOAuthLogin: (
    request: {
      providerId: ProviderCredentialDto['providerId']
      originator?: string | null
    },
  ) => Promise<ProviderAuthSessionView | null>
  completeOAuthCallback: (
    request: {
      providerId: ProviderCredentialDto['providerId']
      flowId: string
      manualInput?: string | null
    },
  ) => Promise<ProviderAuthSessionView | null>
  refreshProviderModelCatalog: (
    profileId: string,
    options?: { force?: boolean },
  ) => Promise<ProviderModelCatalogDto>
  checkProviderProfile: (
    profileId: string,
    options?: { includeNetwork?: boolean },
  ) => Promise<ProviderProfileDiagnosticsDto>
  runDoctorReport: (request?: Partial<RunDoctorReportRequestDto>) => Promise<XeroDoctorReportDto>
  refreshMcpRegistry: (options?: { force?: boolean }) => Promise<McpRegistryDto>
  upsertMcpServer: (request: UpsertMcpServerRequestDto) => Promise<McpRegistryDto>
  removeMcpServer: (serverId: string) => Promise<McpRegistryDto>
  importMcpServers: (path: string) => Promise<ImportMcpServersResponseDto>
  refreshMcpServerStatuses: (options?: { serverIds?: string[] }) => Promise<McpRegistryDto>
  refreshSkillRegistry: (options?: Partial<ListSkillRegistryRequestDto> & { force?: boolean }) => Promise<SkillRegistryDto>
  reloadSkillRegistry: (options?: Partial<ListSkillRegistryRequestDto>) => Promise<SkillRegistryDto>
  setSkillEnabled: (request: SetSkillEnabledRequestDto) => Promise<SkillRegistryDto>
  removeSkill: (request: RemoveSkillRequestDto) => Promise<SkillRegistryDto>
  upsertSkillLocalRoot: (request: UpsertSkillLocalRootRequestDto) => Promise<SkillRegistryDto>
  removeSkillLocalRoot: (request: RemoveSkillLocalRootRequestDto) => Promise<SkillRegistryDto>
  updateProjectSkillSource: (request: UpdateProjectSkillSourceRequestDto) => Promise<SkillRegistryDto>
  updateGithubSkillSource: (request: UpdateGithubSkillSourceRequestDto) => Promise<SkillRegistryDto>
  upsertPluginRoot: (request: UpsertPluginRootRequestDto) => Promise<SkillRegistryDto>
  removePluginRoot: (request: RemovePluginRootRequestDto) => Promise<SkillRegistryDto>
  setPluginEnabled: (request: SetPluginEnabledRequestDto) => Promise<SkillRegistryDto>
  removePlugin: (request: RemovePluginRequestDto) => Promise<SkillRegistryDto>
  refreshNotificationRoutes: (options?: { force?: boolean }) => Promise<NotificationRouteDto[]>
  upsertNotificationRoute: (
    request: Omit<UpsertNotificationRouteRequestDto, 'projectId'>,
  ) => Promise<NotificationRouteDto | null>
  createAgentSession: (options?: {
    title?: string | null
    summary?: string | null
  }) => Promise<ProjectDetailView | null>
  selectAgentSession: (agentSessionId: string) => Promise<ProjectDetailView | null>
  archiveAgentSession: (agentSessionId: string) => Promise<ProjectDetailView | null>
  restoreAgentSession: (agentSessionId: string) => Promise<ProjectDetailView | null>
  deleteAgentSession: (agentSessionId: string) => Promise<ProjectDetailView | null>
  renameAgentSession: (agentSessionId: string, title: string) => Promise<ProjectDetailView | null>
  spawnPane: () => Promise<AgentWorkspaceLayoutState | null>
  closePane: (paneId: string) => void
  focusPane: (paneId: string) => void
  setSplitterRatios: (arrangementKey: string, ratios: number[]) => void
  usageSummaries: Record<string, ProjectUsageSummaryDto>
  activeUsageSummary: ProjectUsageSummaryDto | null
  activeUsageSummaryLoadError: string | null
  refreshUsageSummary: (projectId: string) => Promise<ProjectUsageSummaryDto | null>
}
