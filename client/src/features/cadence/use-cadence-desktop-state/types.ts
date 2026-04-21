import type { CadenceDesktopAdapter } from '@/src/lib/cadence-desktop'
import type {
  AutonomousWorkflowContextView,
  CreateProjectEntryRequestDto,
  CreateProjectEntryResponseDto,
  DeleteProjectEntryResponseDto,
  ListProjectFilesResponseDto,
  NotificationRouteCredentialReadinessDto,
  NotificationRouteDto,
  NotificationRouteKindDto,
  OperatorApprovalView,
  Phase,
  PlanningLifecycleStageView,
  PlanningLifecycleView,
  ProjectDetailView,
  ProjectListItem,
  ReadProjectFileResponseDto,
  RenameProjectEntryRequestDto,
  RenameProjectEntryResponseDto,
  RepositoryDiffScope,
  RepositoryDiffView,
  RepositoryStatusEntryView,
  RepositoryStatusView,
  ResumeHistoryEntryView,
  RuntimeAuthPhaseDto,
  RuntimeRunView,
  RuntimeSessionView,
  RuntimeSettingsDto,
  RuntimeStreamActionRequiredItemView,
  RuntimeStreamActivityItemView,
  RuntimeStreamIssueView,
  RuntimeStreamSkillItemView,
  RuntimeStreamStatus,
  RuntimeStreamView,
  RuntimeStreamViewItem,
  SyncNotificationAdaptersResponseDto,
  UpsertNotificationRouteRequestDto,
  UpsertRuntimeSettingsRequestDto,
  VerificationRecordView,
  WriteProjectFileResponseDto,
} from '@/src/lib/cadence-model'
import type {
  CheckpointControlLoopProjectionView,
  RecentAutonomousUnitsProjectionView,
} from '../agent-runtime-projections'

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
export type RuntimeSettingsLoadStatus = 'idle' | 'loading' | 'ready' | 'error'
export type RuntimeSettingsSaveStatus = 'idle' | 'running'
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
  runtimeSession?: RuntimeSessionView | null
  selectedProviderId?: RuntimeSettingsDto['providerId'] | null
  selectedProviderLabel?: string
  selectedModelId?: string | null
  openrouterApiKeyConfigured?: boolean
  providerMismatch?: boolean
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
  selectedProviderId?: RuntimeSettingsDto['providerId'] | null
  selectedProviderLabel?: string
  selectedModelId?: string | null
  openrouterApiKeyConfigured?: boolean
  providerMismatch?: boolean
  runtimeRun?: RuntimeRunView | null
  autonomousRun?: ProjectDetailView['autonomousRun']
  autonomousUnit?: ProjectDetailView['autonomousUnit']
  autonomousAttempt?: ProjectDetailView['autonomousAttempt']
  autonomousWorkflowContext?: AutonomousWorkflowContextView | null
  autonomousHistory: ProjectDetailView['autonomousHistory']
  autonomousRecentArtifacts: ProjectDetailView['autonomousRecentArtifacts']
  recentAutonomousUnits?: RecentAutonomousUnitsProjectionView
  checkpointControlLoop?: CheckpointControlLoopProjectionView
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
  projectRemovalStatus: ProjectRemovalStatus
  pendingProjectRemovalId: string | null
  errorMessage: string | null
  runtimeSettings: RuntimeSettingsDto | null
  runtimeSettingsLoadStatus: RuntimeSettingsLoadStatus
  runtimeSettingsLoadError: OperatorActionErrorView | null
  runtimeSettingsSaveStatus: RuntimeSettingsSaveStatus
  runtimeSettingsSaveError: OperatorActionErrorView | null
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
  importProject: () => Promise<void>
  removeProject: (projectId: string) => Promise<void>
  retry: () => Promise<void>
  showRepositoryDiff: (scope: RepositoryDiffScope, options?: { force?: boolean }) => Promise<void>
  retryActiveRepositoryDiff: () => Promise<void>
  listProjectFiles: (projectId: string) => Promise<ListProjectFilesResponseDto>
  readProjectFile: (projectId: string, path: string) => Promise<ReadProjectFileResponseDto>
  writeProjectFile: (projectId: string, path: string, content: string) => Promise<WriteProjectFileResponseDto>
  createProjectEntry: (request: CreateProjectEntryRequestDto) => Promise<CreateProjectEntryResponseDto>
  renameProjectEntry: (request: RenameProjectEntryRequestDto) => Promise<RenameProjectEntryResponseDto>
  deleteProjectEntry: (projectId: string, path: string) => Promise<DeleteProjectEntryResponseDto>
  startOpenAiLogin: () => Promise<RuntimeSessionView | null>
  submitOpenAiCallback: (flowId: string, options?: { manualInput?: string | null }) => Promise<RuntimeSessionView | null>
  startAutonomousRun: () => Promise<ProjectDetailView['autonomousRun'] | null>
  inspectAutonomousRun: () => Promise<ProjectDetailView['autonomousRun'] | null>
  cancelAutonomousRun: (runId: string) => Promise<ProjectDetailView['autonomousRun'] | null>
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
  refreshRuntimeSettings: (options?: { force?: boolean }) => Promise<RuntimeSettingsDto>
  upsertRuntimeSettings: (request: UpsertRuntimeSettingsRequestDto) => Promise<RuntimeSettingsDto>
  refreshNotificationRoutes: (options?: { force?: boolean }) => Promise<NotificationRouteDto[]>
  upsertNotificationRoute: (
    request: Omit<UpsertNotificationRouteRequestDto, 'projectId'>,
  ) => Promise<NotificationRouteDto | null>
}
