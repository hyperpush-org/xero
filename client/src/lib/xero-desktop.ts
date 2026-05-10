import { Channel, invoke, isTauri } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import { ZodError, z } from 'zod'
import {
  backendRequestKey,
  createBackendRequestCoordinator,
} from '@/src/lib/backend-request-coordinator'
import { recordIpcPayloadSample } from '@/src/lib/ipc-payload-budget'
import { createSafeTauriUnlisten } from '@/src/lib/tauri-events'
import {
  autonomousRunStateSchema,
  cancelAutonomousRunRequestSchema,
  getAutonomousRunRequestSchema,
  startAutonomousRunRequestSchema,
  type AutonomousRunStateDto,
  type CancelAutonomousRunRequestDto,
  type GetAutonomousRunRequestDto,
  type StartAutonomousRunRequestDto,
} from '@/src/lib/xero-model/autonomous'
import {
  agentDefinitionSummarySchema,
  agentDefinitionVersionSummarySchema,
  agentDefinitionWriteResponseSchema,
  archiveAgentDefinitionRequestSchema,
  getAgentDefinitionVersionRequestSchema,
  listAgentDefinitionsRequestSchema,
  listAgentDefinitionsResponseSchema,
  saveAgentDefinitionRequestSchema,
  updateAgentDefinitionRequestSchema,
  type AgentDefinitionSummaryDto,
  type AgentDefinitionVersionSummaryDto,
  type AgentDefinitionWriteResponseDto,
  type ArchiveAgentDefinitionRequestDto,
  type GetAgentDefinitionVersionRequestDto,
  type ListAgentDefinitionsRequestDto,
  type ListAgentDefinitionsResponseDto,
  type SaveAgentDefinitionRequestDto,
  type UpdateAgentDefinitionRequestDto,
} from '@/src/lib/xero-model/agent-definition'
import {
  agentAuthoringCatalogSchema,
  getAgentAuthoringCatalogRequestSchema,
  getWorkflowAgentDetailRequestSchema,
  getWorkflowAgentGraphProjectionRequestSchema,
  listWorkflowAgentsRequestSchema,
  listWorkflowAgentsResponseSchema,
  resolveAgentAuthoringSkillRequestSchema,
  searchAgentAuthoringSkillsRequestSchema,
  searchAgentAuthoringSkillsResponseSchema,
  agentAuthoringAttachableSkillSchema,
  workflowAgentGraphProjectionSchema,
  workflowAgentDetailSchema,
  type AgentAuthoringAttachableSkillDto,
  type AgentAuthoringCatalogDto,
  type GetAgentAuthoringCatalogRequestDto,
  type GetWorkflowAgentDetailRequestDto,
  type GetWorkflowAgentGraphProjectionRequestDto,
  type ListWorkflowAgentsRequestDto,
  type ListWorkflowAgentsResponseDto,
  type ResolveAgentAuthoringSkillRequestDto,
  type SearchAgentAuthoringSkillsRequestDto,
  type SearchAgentAuthoringSkillsResponseDto,
  type WorkflowAgentDetailDto,
  type WorkflowAgentGraphProjectionDto,
} from '@/src/lib/xero-model/workflow-agents'
import {
  agentRunEventSchema,
  agentRunSchema,
  agentTraceExportSchema,
  cancelAgentRunRequestSchema,
  exportAgentTraceRequestSchema,
  getAgentRunRequestSchema,
  listAgentRunsRequestSchema,
  listAgentRunsResponseSchema,
  resumeAgentRunRequestSchema,
  sendAgentMessageRequestSchema,
  startAgentTaskRequestSchema,
  subscribeAgentStreamRequestSchema,
  subscribeAgentStreamResponseSchema,
  type AgentRunDto,
  type AgentRunEventDto,
  type AgentTraceExportDto,
  type CancelAgentRunRequestDto,
  type ExportAgentTraceRequestDto,
  type GetAgentRunRequestDto,
  type ListAgentRunsResponseDto,
  type ResumeAgentRunRequestDto,
  type SendAgentMessageRequestDto,
  type StartAgentTaskRequestDto,
  type SubscribeAgentStreamResponseDto,
} from '@/src/lib/xero-model/agent'
import {
  listNotificationDispatchesRequestSchema,
  listNotificationDispatchesResponseSchema,
  listNotificationRoutesRequestSchema,
  listNotificationRoutesResponseSchema,
  recordNotificationDispatchOutcomeRequestSchema,
  recordNotificationDispatchOutcomeResponseSchema,
  submitNotificationReplyRequestSchema,
  submitNotificationReplyResponseSchema,
  syncNotificationAdaptersRequestSchema,
  syncNotificationAdaptersResponseSchema,
  upsertNotificationRouteCredentialsRequestSchema,
  upsertNotificationRouteCredentialsResponseSchema,
  upsertNotificationRouteRequestSchema,
  upsertNotificationRouteResponseSchema,
  type ListNotificationDispatchesResponseDto,
  type ListNotificationRoutesResponseDto,
  type RecordNotificationDispatchOutcomeRequestDto,
  type RecordNotificationDispatchOutcomeResponseDto,
  type SubmitNotificationReplyRequestDto,
  type SubmitNotificationReplyResponseDto,
  type SyncNotificationAdaptersResponseDto,
  type UpsertNotificationRouteCredentialsRequestDto,
  type UpsertNotificationRouteCredentialsResponseDto,
  type UpsertNotificationRouteRequestDto,
  type UpsertNotificationRouteResponseDto,
} from '@/src/lib/xero-model/notifications'
import {
  importMcpServersRequestSchema,
  importMcpServersResponseSchema,
  mcpRegistrySchema,
  refreshMcpServerStatusesRequestSchema,
  removeMcpServerRequestSchema,
  upsertMcpServerRequestSchema,
  type ImportMcpServersResponseDto,
  type McpRegistryDto,
  type UpsertMcpServerRequestDto,
} from '@/src/lib/xero-model/mcp'
import {
  listSkillRegistryRequestSchema,
  removePluginRequestSchema,
  removePluginRootRequestSchema,
  removeSkillLocalRootRequestSchema,
  removeSkillRequestSchema,
  setPluginEnabledRequestSchema,
  setSkillEnabledRequestSchema,
  skillRegistrySchema,
  updateGithubSkillSourceRequestSchema,
  updateProjectSkillSourceRequestSchema,
  upsertPluginRootRequestSchema,
  upsertSkillLocalRootRequestSchema,
  type ListSkillRegistryRequestDto,
  type RemovePluginRequestDto,
  type RemovePluginRootRequestDto,
  type RemoveSkillLocalRootRequestDto,
  type RemoveSkillRequestDto,
  type SetPluginEnabledRequestDto,
  type SetSkillEnabledRequestDto,
  type SkillRegistryDto,
  type UpdateGithubSkillSourceRequestDto,
  type UpdateProjectSkillSourceRequestDto,
  type UpsertPluginRootRequestDto,
  type UpsertSkillLocalRootRequestDto,
} from '@/src/lib/xero-model/skills'
import {
  resolveOperatorActionRequestSchema,
  resolveOperatorActionResponseSchema,
  resumeOperatorRunRequestSchema,
  resumeOperatorRunResponseSchema,
  type ResolveOperatorActionResponseDto,
  type ResumeOperatorRunResponseDto,
} from '@/src/lib/xero-model/operator-actions'
import {
  returnSessionToHereRequestSchema,
  returnSessionToHereResponseSchema,
  selectiveUndoRequestSchema,
  selectiveUndoResponseSchema,
  type ReturnSessionToHereRequestDto,
  type ReturnSessionToHereResponseDto,
  type SelectiveUndoRequestDto,
  type SelectiveUndoResponseDto,
} from '@/src/lib/xero-model/code-history'
import {
  importRepositoryResponseSchema,
  createProjectEntryRequestSchema,
  createProjectEntryResponseSchema,
  deleteProjectEntryResponseSchema,
  listProjectFilesRequestSchema,
  listProjectFilesResponseSchema,
  listProjectsResponseSchema,
  moveProjectEntryRequestSchema,
  moveProjectEntryResponseSchema,
  appUiStateResponseSchema,
  projectFileRequestSchema,
  projectUiStateResponseSchema,
  projectUpdatedPayloadSchema,
  readAppUiStateRequestSchema,
  readProjectUiStateRequestSchema,
  readProjectFileResponseSchema,
  renameProjectEntryRequestSchema,
  renameProjectEntryResponseSchema,
  replaceInProjectRequestSchema,
  replaceInProjectResponseSchema,
  repositoryDiffResponseSchema,
  repositoryStatusChangedPayloadSchema,
  repositoryStatusResponseSchema,
  revokeProjectAssetTokensRequestSchema,
  searchProjectRequestSchema,
  searchProjectResponseSchema,
  workspaceExplainRequestSchema,
  workspaceExplainResponseSchema,
  workspaceIndexRequestSchema,
  workspaceIndexResponseSchema,
  workspaceIndexStatusSchema,
  workspaceQueryRequestSchema,
  workspaceQueryResponseSchema,
  writeProjectFileRequestSchema,
  writeProjectFileResponseSchema,
  writeAppUiStateRequestSchema,
  writeProjectUiStateRequestSchema,
  gitCommitRequestSchema,
  gitCommitResponseSchema,
  gitGenerateCommitMessageRequestSchema,
  gitGenerateCommitMessageResponseSchema,
  gitFetchResponseSchema,
  gitPathsRequestSchema,
  gitPullResponseSchema,
  gitPushResponseSchema,
  gitRemoteRequestSchema,
  type CreateProjectEntryRequestDto,
  type CreateProjectEntryResponseDto,
  type DeleteProjectEntryResponseDto,
  type GitCommitResponseDto,
  type GitGenerateCommitMessageRequestDto,
  type GitGenerateCommitMessageResponseDto,
  type GitFetchResponseDto,
  type GitPullResponseDto,
  type GitPushResponseDto,
  type ImportRepositoryResponseDto,
  type ListProjectFilesRequestDto,
  type ListProjectFilesResponseDto,
  type ListProjectsResponseDto,
  type MoveProjectEntryRequestDto,
  type MoveProjectEntryResponseDto,
  type ProjectUpdatedPayloadDto,
  type AppUiStateResponseDto,
  type ReadProjectFileResponseDto,
  type ReadAppUiStateRequestDto,
  type ProjectUiStateResponseDto,
  type ReadProjectUiStateRequestDto,
  type RenameProjectEntryRequestDto,
  type RenameProjectEntryResponseDto,
  type ReplaceInProjectRequestDto,
  type ReplaceInProjectResponseDto,
  type RepositoryDiffResponseDto,
  type RepositoryDiffScope,
  type RepositoryStatusChangedPayloadDto,
  type RepositoryStatusResponseDto,
  type RevokeProjectAssetTokensRequestDto,
  type SearchProjectRequestDto,
  type SearchProjectResponseDto,
  type WorkspaceExplainRequestDto,
  type WorkspaceExplainResponseDto,
  type WorkspaceIndexRequestDto,
  type WorkspaceIndexResponseDto,
  type WorkspaceIndexStatusDto,
  type WorkspaceQueryRequestDto,
  type WorkspaceQueryResponseDto,
  type WriteAppUiStateRequestDto,
  type WriteProjectFileResponseDto,
  type WriteProjectUiStateRequestDto,
} from '@/src/lib/xero-model/project'
import {
  runtimeRunSchema,
  runtimeRunUpdatedPayloadSchema,
  runtimeSessionSchema,
  providerAuthSessionSchema,
  runtimeUpdatedPayloadSchema,
  archiveAgentSessionRequestSchema,
  autoNameAgentSessionRequestSchema,
  createAgentSessionRequestSchema,
  deleteAgentSessionRequestSchema,
  getAgentSessionRequestSchema,
  restoreAgentSessionRequestSchema,
  getRuntimeRunRequestSchema,
  agentSessionSchema,
  listAgentSessionsRequestSchema,
  listAgentSessionsResponseSchema,
  stagedAgentAttachmentSchema,
  startRuntimeSessionRequestSchema,
  startRuntimeRunRequestSchema,
  stopRuntimeRunRequestSchema,
  updateRuntimeRunControlsRequestSchema,
  updateAgentSessionRequestSchema,
  type AgentSessionDto,
  type ArchiveAgentSessionRequestDto,
  type AutoNameAgentSessionRequestDto,
  type CreateAgentSessionRequestDto,
  type DeleteAgentSessionRequestDto,
  type GetAgentSessionRequestDto,
  type RestoreAgentSessionRequestDto,
  type GetRuntimeRunRequestDto,
  type ListAgentSessionsRequestDto,
  type ListAgentSessionsResponseDto,
  type RuntimeRunControlInputDto,
  type RuntimeRunDto,
  type RuntimeRunUpdatedPayloadDto,
  type RuntimeSessionDto,
  type ProviderAuthSessionDto,
  type RuntimeUpdatedPayloadDto,
  type StagedAgentAttachmentDto,
  type StartRuntimeRunRequestDto,
  type StartRuntimeSessionRequestDto,
  type StopRuntimeRunRequestDto,
  type UpdateAgentSessionRequestDto,
  type UpdateRuntimeRunControlsRequestDto,
} from '@/src/lib/xero-model/runtime'
import {
  completeOAuthCallbackRequestSchema,
  deleteProviderCredentialRequestSchema,
  providerCredentialsSnapshotSchema,
  startOAuthLoginRequestSchema,
  upsertProviderCredentialRequestSchema,
  type CompleteOAuthCallbackRequestDto,
  type ProviderCredentialsSnapshotDto,
  type StartOAuthLoginRequestDto,
  type UpsertProviderCredentialRequestDto,
} from '@/src/lib/xero-model/provider-credentials'
import {
  createPreflightProviderProfileRequest,
  createProviderModelCatalogRequest,
  providerPreflightSnapshotSchema,
  providerModelCatalogSchema,
  type ProviderPreflightRequiredFeaturesDto,
  type ProviderPreflightSnapshotDto,
  type ProviderModelCatalogDto,
} from '@/src/lib/xero-model/provider-models'
import {
  xeroDoctorReportSchema,
  checkProviderProfileRequestSchema,
  providerProfileDiagnosticsSchema,
  runDoctorReportRequestSchema,
  type XeroDoctorReportDto,
  type ProviderProfileDiagnosticsDto,
  type RunDoctorReportRequestDto,
} from '@/src/lib/xero-model/diagnostics'
import {
  runtimeStreamItemSchema,
  runtimeStreamPatchSchema,
  subscribeRuntimeStreamRequestSchema,
  subscribeRuntimeStreamResponseSchema,
  type RuntimeStreamEventDto,
  type RuntimeStreamItemKindDto,
  type RuntimeStreamItemDto,
  type RuntimeStreamPatchDto,
  type SubscribeRuntimeStreamResponseDto,
} from '@/src/lib/xero-model/runtime-stream'
import {
  dictationEventSchema,
  dictationSettingsSchema,
  dictationStartRequestSchema,
  dictationStartResponseSchema,
  dictationStatusSchema,
  upsertDictationSettingsRequestSchema,
  type DictationEventDto,
  type DictationSettingsDto,
  type DictationStartRequestInputDto,
  type DictationStartResponseDto,
  type DictationStatusDto,
  type UpsertDictationSettingsRequestDto,
} from '@/src/lib/xero-model/dictation'
import {
  browserControlSettingsSchema,
  upsertBrowserControlSettingsRequestSchema,
  type BrowserControlSettingsDto,
  type UpsertBrowserControlSettingsRequestDto,
} from '@/src/lib/xero-model/browser'
import {
  soulSettingsSchema,
  upsertSoulSettingsRequestSchema,
  type SoulSettingsDto,
  type UpsertSoulSettingsRequestDto,
} from '@/src/lib/xero-model/soul'
import {
  compactSessionHistoryRequestSchema,
  compactSessionHistoryResponseSchema,
  agentSessionBranchResponseSchema,
  branchAgentSessionRequestSchema,
  deleteSessionMemoryRequestSchema,
  extractSessionMemoryCandidatesRequestSchema,
  extractSessionMemoryCandidatesResponseSchema,
  exportSessionTranscriptRequestSchema,
  getSessionContextSnapshotRequestSchema,
  getSessionTranscriptRequestSchema,
  listSessionMemoriesRequestSchema,
  listSessionMemoriesResponseSchema,
  rewindAgentSessionRequestSchema,
  saveSessionTranscriptExportRequestSchema,
  searchSessionTranscriptsRequestSchema,
  searchSessionTranscriptsResponseSchema,
  sessionContextSnapshotSchema,
  sessionMemoryRecordSchema,
  sessionTranscriptExportResponseSchema,
  sessionTranscriptSchema,
  type ExportSessionTranscriptRequestDto,
  type AgentSessionBranchResponseDto,
  type BranchAgentSessionRequestDto,
  type CompactSessionHistoryRequestDto,
  type CompactSessionHistoryResponseDto,
  type DeleteSessionMemoryRequestDto,
  type ExtractSessionMemoryCandidatesRequestDto,
  type ExtractSessionMemoryCandidatesResponseDto,
  type GetSessionContextSnapshotRequestDto,
  type GetSessionTranscriptRequestDto,
  type ListSessionMemoriesRequestDto,
  type ListSessionMemoriesResponseDto,
  type RewindAgentSessionRequestDto,
  type SaveSessionTranscriptExportRequestDto,
  type SearchSessionTranscriptsRequestDto,
  type SearchSessionTranscriptsResponseDto,
  type SessionContextSnapshotDto,
  type SessionMemoryRecordDto,
  type SessionTranscriptDto,
  type SessionTranscriptExportResponseDto,
  type UpdateSessionMemoryRequestDto,
  updateSessionMemoryRequestSchema,
} from '@/src/lib/xero-model/session-context'
import {
  projectLoadBundleRequestSchema,
  projectLoadBundleSchema,
  projectSnapshotResponseSchema,
  type ProjectLoadBundleDto,
  type ProjectLoadBundleRequestDto,
  type ProjectSnapshotResponseDto,
} from '@/src/lib/xero-model'
import {
  agentUsageUpdatedPayloadSchema,
  projectUsageSummarySchema,
  type AgentUsageUpdatedPayloadDto,
  type ProjectUsageSummaryDto,
} from '@/src/lib/xero-model/usage'
import {
  environmentDiscoveryStatusSchema,
  environmentProbeReportSchema,
  type EnvironmentDiscoveryStatusDto,
  type EnvironmentProbeReportDto,
  environmentProfileSummarySchema,
  type EnvironmentProfileSummaryDto,
  resolveEnvironmentPermissionRequestsSchema,
  type ResolveEnvironmentPermissionRequestsDto,
  saveUserToolRequestSchema,
  type SaveUserToolRequestDto,
  verifyUserToolRequestSchema,
  verifyUserToolResponseSchema,
  type VerifyUserToolRequestDto,
  type VerifyUserToolResponseDto,
} from '@/src/lib/xero-model/environment'

const COMMANDS = {
  importRepository: 'import_repository',
  createRepository: 'create_repository',
  listProjects: 'list_projects',
  removeProject: 'remove_project',
  readAppUiState: 'read_app_ui_state',
  writeAppUiState: 'write_app_ui_state',
  readProjectUiState: 'read_project_ui_state',
  writeProjectUiState: 'write_project_ui_state',
  getProjectLoadBundle: 'get_project_load_bundle',
  getProjectSnapshot: 'get_project_snapshot',
  getProjectUsageSummary: 'get_project_usage_summary',
  getRepositoryStatus: 'get_repository_status',
  getRepositoryDiff: 'get_repository_diff',
  applySelectiveUndo: 'apply_selective_undo',
  returnSessionToHere: 'apply_session_rollback',
  gitStagePaths: 'git_stage_paths',
  gitUnstagePaths: 'git_unstage_paths',
  gitDiscardChanges: 'git_discard_changes',
  gitCommit: 'git_commit',
  gitGenerateCommitMessage: 'git_generate_commit_message',
  gitFetch: 'git_fetch',
  gitPull: 'git_pull',
  gitPush: 'git_push',
  listProjectFiles: 'list_project_files',
  readProjectFile: 'read_project_file',
  writeProjectFile: 'write_project_file',
  revokeProjectAssetTokens: 'revoke_project_asset_tokens',
  openProjectFileExternal: 'open_project_file_external',
  createProjectEntry: 'create_project_entry',
  renameProjectEntry: 'rename_project_entry',
  moveProjectEntry: 'move_project_entry',
  deleteProjectEntry: 'delete_project_entry',
  searchProject: 'search_project',
  replaceInProject: 'replace_in_project',
  workspaceIndex: 'workspace_index',
  workspaceStatus: 'workspace_status',
  workspaceQuery: 'workspace_query',
  workspaceExplain: 'workspace_explain',
  workspaceReset: 'workspace_reset',
  createAgentSession: 'create_agent_session',
  listAgentDefinitions: 'list_agent_definitions',
  archiveAgentDefinition: 'archive_agent_definition',
  getAgentDefinitionVersion: 'get_agent_definition_version',
  saveAgentDefinition: 'save_agent_definition',
  updateAgentDefinition: 'update_agent_definition',
  listWorkflowAgents: 'list_workflow_agents',
  getWorkflowAgentDetail: 'get_workflow_agent_detail',
  getWorkflowAgentGraphProjection: 'get_workflow_agent_graph_projection',
  getAgentAuthoringCatalog: 'get_agent_authoring_catalog',
  searchAgentAuthoringSkills: 'search_agent_authoring_skills',
  resolveAgentAuthoringSkill: 'resolve_agent_authoring_skill',
  listAgentSessions: 'list_agent_sessions',
  getAgentSession: 'get_agent_session',
  updateAgentSession: 'update_agent_session',
  autoNameAgentSession: 'auto_name_agent_session',
  archiveAgentSession: 'archive_agent_session',
  restoreAgentSession: 'restore_agent_session',
  deleteAgentSession: 'delete_agent_session',
  getAutonomousRun: 'get_autonomous_run',
  startAgentTask: 'start_agent_task',
  sendAgentMessage: 'send_agent_message',
  cancelAgentRun: 'cancel_agent_run',
  resumeAgentRun: 'resume_agent_run',
  getAgentRun: 'get_agent_run',
  exportAgentTrace: 'export_agent_trace',
  listAgentRuns: 'list_agent_runs',
  subscribeAgentStream: 'subscribe_agent_stream',
  getSessionTranscript: 'get_session_transcript',
  exportSessionTranscript: 'export_session_transcript',
  saveSessionTranscriptExport: 'save_session_transcript_export',
  searchSessionTranscripts: 'search_session_transcripts',
  getSessionContextSnapshot: 'get_session_context_snapshot',
  compactSessionHistory: 'compact_session_history',
  branchAgentSession: 'branch_agent_session',
  rewindAgentSession: 'rewind_agent_session',
  listSessionMemories: 'list_session_memories',
  extractSessionMemoryCandidates: 'extract_session_memory_candidates',
  updateSessionMemory: 'update_session_memory',
  deleteSessionMemory: 'delete_session_memory',
  getRuntimeRun: 'get_runtime_run',
  getRuntimeSession: 'get_runtime_session',
  listMcpServers: 'list_mcp_servers',
  upsertMcpServer: 'upsert_mcp_server',
  removeMcpServer: 'remove_mcp_server',
  importMcpServers: 'import_mcp_servers',
  refreshMcpServerStatuses: 'refresh_mcp_server_statuses',
  listSkillRegistry: 'list_skill_registry',
  reloadSkillRegistry: 'reload_skill_registry',
  setSkillEnabled: 'set_skill_enabled',
  removeSkill: 'remove_skill',
  upsertSkillLocalRoot: 'upsert_skill_local_root',
  removeSkillLocalRoot: 'remove_skill_local_root',
  updateProjectSkillSource: 'update_project_skill_source',
  updateGithubSkillSource: 'update_github_skill_source',
  upsertPluginRoot: 'upsert_plugin_root',
  removePluginRoot: 'remove_plugin_root',
  setPluginEnabled: 'set_plugin_enabled',
  removePlugin: 'remove_plugin',
  getProviderModelCatalog: 'get_provider_model_catalog',
  preflightProviderProfile: 'preflight_provider_profile',
  runDoctorReport: 'run_doctor_report',
  checkProviderProfile: 'check_provider_profile',
  listProviderCredentials: 'list_provider_credentials',
  upsertProviderCredential: 'upsert_provider_credential',
  deleteProviderCredential: 'delete_provider_credential',
  startOAuthLogin: 'start_oauth_login',
  completeOAuthCallback: 'complete_oauth_callback',
  startOpenAiLogin: 'start_openai_login',
  submitOpenAiCallback: 'submit_openai_callback',
  startAutonomousRun: 'start_autonomous_run',
  stageAgentAttachment: 'stage_agent_attachment',
  discardAgentAttachment: 'discard_agent_attachment',
  startRuntimeRun: 'start_runtime_run',
  updateRuntimeRunControls: 'update_runtime_run_controls',
  startRuntimeSession: 'start_runtime_session',
  cancelAutonomousRun: 'cancel_autonomous_run',
  stopRuntimeRun: 'stop_runtime_run',
  logoutRuntimeSession: 'logout_runtime_session',
  resolveOperatorAction: 'resolve_operator_action',
  resumeOperatorRun: 'resume_operator_run',
  listNotificationRoutes: 'list_notification_routes',
  listNotificationDispatches: 'list_notification_dispatches',
  upsertNotificationRoute: 'upsert_notification_route',
  upsertNotificationRouteCredentials: 'upsert_notification_route_credentials',
  recordNotificationDispatchOutcome: 'record_notification_dispatch_outcome',
  submitNotificationReply: 'submit_notification_reply',
  syncNotificationAdapters: 'sync_notification_adapters',
  speechDictationStatus: 'speech_dictation_status',
  speechDictationSettings: 'speech_dictation_settings',
  speechDictationUpdateSettings: 'speech_dictation_update_settings',
  speechDictationStart: 'speech_dictation_start',
  speechDictationStop: 'speech_dictation_stop',
  speechDictationCancel: 'speech_dictation_cancel',
  subscribeRuntimeStream: 'subscribe_runtime_stream',
  browserControlSettings: 'browser_control_settings',
  browserControlUpdateSettings: 'browser_control_update_settings',
  soulSettings: 'soul_settings',
  soulUpdateSettings: 'soul_update_settings',
  browserShow: 'browser_show',
  browserResize: 'browser_resize',
  browserHide: 'browser_hide',
  browserEval: 'browser_eval',
  browserCurrentUrl: 'browser_current_url',
  browserScreenshot: 'browser_screenshot',
  browserNavigate: 'browser_navigate',
  browserBack: 'browser_back',
  browserForward: 'browser_forward',
  browserReload: 'browser_reload',
  browserStop: 'browser_stop',
  browserClick: 'browser_click',
  browserType: 'browser_type',
  browserScroll: 'browser_scroll',
  browserPressKey: 'browser_press_key',
  browserReadText: 'browser_read_text',
  browserQuery: 'browser_query',
  browserWaitForSelector: 'browser_wait_for_selector',
  browserWaitForLoad: 'browser_wait_for_load',
  browserHistoryState: 'browser_history_state',
  browserCookiesGet: 'browser_cookies_get',
  browserCookiesSet: 'browser_cookies_set',
  browserStorageRead: 'browser_storage_read',
  browserStorageWrite: 'browser_storage_write',
  browserStorageClear: 'browser_storage_clear',
  browserTabList: 'browser_tab_list',
  browserTabFocus: 'browser_tab_focus',
  browserTabClose: 'browser_tab_close',
  getEnvironmentDiscoveryStatus: 'get_environment_discovery_status',
  getEnvironmentProfileSummary: 'get_environment_profile_summary',
  refreshEnvironmentDiscovery: 'refresh_environment_discovery',
  environmentVerifyUserTool: 'environment_verify_user_tool',
  environmentSaveUserTool: 'environment_save_user_tool',
  environmentRemoveUserTool: 'environment_remove_user_tool',
  resolveEnvironmentPermissionRequests: 'resolve_environment_permission_requests',
  startEnvironmentDiscovery: 'start_environment_discovery',
} as const

const EVENTS = {
  projectUpdated: 'project:updated',
  repositoryStatusChanged: 'repository:status_changed',
  runtimeUpdated: 'runtime:updated',
  runtimeRunUpdated: 'runtime_run:updated',
  browserUrlChanged: 'browser:url_changed',
  browserLoadState: 'browser:load_state',
  browserConsole: 'browser:console',
  browserTabUpdated: 'browser:tab_updated',
  agentUsageUpdated: 'agent_usage_updated',
} as const

const commandErrorSchema = z.object({
  code: z.string(),
  class: z.enum(['user_fixable', 'retryable', 'system_fault', 'policy_denied']),
  message: z.string(),
  retryable: z.boolean(),
})

const browserEvalResponseSchema = z.unknown()
const browserCurrentUrlResponseSchema = z.string().nullable()
const browserScreenshotResponseSchema = z.string()
const browserVoidSchema = z.null().optional().transform(() => undefined)
const browserJsonSchema = z.unknown()
const backendRequestCoordinator = createBackendRequestCoordinator()

export const browserTabMetadataSchema = z
  .object({
    id: z.string(),
    label: z.string(),
    title: z.string().nullable(),
    url: z.string().nullable(),
    loading: z.boolean(),
    canGoBack: z.boolean(),
    canGoForward: z.boolean(),
    active: z.boolean(),
  })
  .strict()
export type BrowserTabMetadataDto = z.infer<typeof browserTabMetadataSchema>

const browserTabListSchema = z.array(browserTabMetadataSchema)

export const browserUrlChangedPayloadSchema = z
  .object({
    tabId: z.string(),
    url: z.string(),
    title: z.string().nullable(),
    canGoBack: z.boolean(),
    canGoForward: z.boolean(),
  })
  .strict()
export type BrowserUrlChangedPayload = z.infer<typeof browserUrlChangedPayloadSchema>

export const browserLoadStatePayloadSchema = z
  .object({
    tabId: z.string(),
    loading: z.boolean(),
    url: z.string().nullable(),
    error: z.string().nullable(),
  })
  .strict()
export type BrowserLoadStatePayload = z.infer<typeof browserLoadStatePayloadSchema>

export const browserConsolePayloadSchema = z
  .object({
    tabId: z.string(),
    level: z.string(),
    message: z.string(),
  })
  .strict()
export type BrowserConsolePayload = z.infer<typeof browserConsolePayloadSchema>

export const browserTabUpdatedPayloadSchema = z
  .object({
    tabs: browserTabListSchema,
  })
  .strict()
export type BrowserTabUpdatedPayload = z.infer<typeof browserTabUpdatedPayloadSchema>

const startOpenAiLoginRequestSchema = z
  .object({
    originator: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

const submitOpenAiCallbackRequestSchema = z
  .object({
    flowId: z.string().trim().min(1),
    manualInput: z.string().trim().min(1).nullable().optional(),
  })
  .strict()

export interface StartOpenAiLoginOptions {
  originator?: string | null
}

export interface SubmitOpenAiCallbackOptions {
  manualInput?: string | null
}

export interface StartRuntimeRunOptions {
  initialControls?: RuntimeRunControlInputDto | null
  initialPrompt?: string | null
  initialAttachments?: StagedAgentAttachmentDto[]
}

export interface StageAgentAttachmentInput {
  projectId: string
  runId: string
  originalName: string
  mediaType: string
  bytes: Uint8Array
}

export interface StartRuntimeSessionOptions {
  providerProfileId?: string | null
}

export class XeroDesktopError extends Error {
  code: string
  errorClass: z.infer<typeof commandErrorSchema>['class'] | 'adapter_contract_mismatch' | 'desktop_runtime_unavailable'
  retryable: boolean

  constructor(options: {
    message: string
    code?: string
    errorClass?: XeroDesktopError['errorClass']
    retryable?: boolean
    cause?: unknown
  }) {
    super(options.message)
    this.name = 'XeroDesktopError'
    this.code = options.code ?? 'desktop_error'
    this.errorClass = options.errorClass ?? 'system_fault'
    this.retryable = options.retryable ?? false
    if (options.cause !== undefined) {
      ;(this as Error & { cause?: unknown }).cause = options.cause
    }
  }
}

export interface XeroRuntimeStreamSubscription {
  response: SubscribeRuntimeStreamResponseDto
  unsubscribe: () => void
}

export interface XeroAgentStreamSubscription {
  response: SubscribeAgentStreamResponseDto
  unsubscribe: () => void
}

export interface XeroDictationSession {
  response: DictationStartResponseDto
  unsubscribe: () => void
  stop: () => Promise<void>
  cancel: () => Promise<void>
}

export interface XeroDesktopAdapter {
  isDesktopRuntime(): boolean
  pickRepositoryFolder(): Promise<string | null>
  pickParentFolder(): Promise<string | null>
  importRepository(path: string): Promise<ImportRepositoryResponseDto>
  createRepository(parentPath: string, name: string): Promise<ImportRepositoryResponseDto>
  listProjects(): Promise<ListProjectsResponseDto>
  removeProject(projectId: string): Promise<ListProjectsResponseDto>
  readAppUiState?(request: ReadAppUiStateRequestDto): Promise<AppUiStateResponseDto>
  writeAppUiState?(request: WriteAppUiStateRequestDto): Promise<AppUiStateResponseDto>
  readProjectUiState?(request: ReadProjectUiStateRequestDto): Promise<ProjectUiStateResponseDto>
  writeProjectUiState?(request: WriteProjectUiStateRequestDto): Promise<ProjectUiStateResponseDto>
  getProjectSnapshot(projectId: string): Promise<ProjectSnapshotResponseDto>
  getProjectLoadBundle?(
    request: ProjectLoadBundleRequestDto,
  ): Promise<ProjectLoadBundleDto>
  getProjectUsageSummary(projectId: string): Promise<ProjectUsageSummaryDto>
  getRepositoryStatus(projectId: string): Promise<RepositoryStatusResponseDto>
  getRepositoryDiff(projectId: string, scope: RepositoryDiffScope): Promise<RepositoryDiffResponseDto>
  applySelectiveUndo(request: SelectiveUndoRequestDto): Promise<SelectiveUndoResponseDto>
  returnSessionToHere(request: ReturnSessionToHereRequestDto): Promise<ReturnSessionToHereResponseDto>
  gitStagePaths(projectId: string, paths: string[]): Promise<void>
  gitUnstagePaths(projectId: string, paths: string[]): Promise<void>
  gitDiscardChanges(projectId: string, paths: string[]): Promise<void>
  gitCommit(projectId: string, message: string): Promise<GitCommitResponseDto>
  gitGenerateCommitMessage(
    request: GitGenerateCommitMessageRequestDto,
  ): Promise<GitGenerateCommitMessageResponseDto>
  gitFetch(projectId: string, remote?: string | null): Promise<GitFetchResponseDto>
  gitPull(projectId: string, remote?: string | null): Promise<GitPullResponseDto>
  gitPush(projectId: string, remote?: string | null): Promise<GitPushResponseDto>
  listProjectFiles(projectId: string, path?: string): Promise<ListProjectFilesResponseDto>
  readProjectFile(projectId: string, path: string): Promise<ReadProjectFileResponseDto>
  writeProjectFile(projectId: string, path: string, content: string): Promise<WriteProjectFileResponseDto>
  revokeProjectAssetTokens?(projectId: string, paths?: string[]): Promise<void>
  openProjectFileExternal?(projectId: string, path: string): Promise<void>
  createProjectEntry(request: CreateProjectEntryRequestDto): Promise<CreateProjectEntryResponseDto>
  renameProjectEntry(request: RenameProjectEntryRequestDto): Promise<RenameProjectEntryResponseDto>
  moveProjectEntry(request: MoveProjectEntryRequestDto): Promise<MoveProjectEntryResponseDto>
  deleteProjectEntry(projectId: string, path: string): Promise<DeleteProjectEntryResponseDto>
  searchProject(request: SearchProjectRequestDto): Promise<SearchProjectResponseDto>
  replaceInProject(request: ReplaceInProjectRequestDto): Promise<ReplaceInProjectResponseDto>
  workspaceIndex(request: WorkspaceIndexRequestDto): Promise<WorkspaceIndexResponseDto>
  workspaceStatus(projectId: string): Promise<WorkspaceIndexStatusDto>
  workspaceQuery(request: WorkspaceQueryRequestDto): Promise<WorkspaceQueryResponseDto>
  workspaceExplain(request: WorkspaceExplainRequestDto): Promise<WorkspaceExplainResponseDto>
  workspaceReset(projectId: string): Promise<WorkspaceIndexStatusDto>
  createAgentSession(request: CreateAgentSessionRequestDto): Promise<AgentSessionDto>
  listAgentDefinitions(
    request: ListAgentDefinitionsRequestDto,
  ): Promise<ListAgentDefinitionsResponseDto>
  archiveAgentDefinition(
    request: ArchiveAgentDefinitionRequestDto,
  ): Promise<AgentDefinitionSummaryDto>
  getAgentDefinitionVersion(
    request: GetAgentDefinitionVersionRequestDto,
  ): Promise<AgentDefinitionVersionSummaryDto | null>
  saveAgentDefinition(
    request: SaveAgentDefinitionRequestDto,
  ): Promise<AgentDefinitionWriteResponseDto>
  updateAgentDefinition(
    request: UpdateAgentDefinitionRequestDto,
  ): Promise<AgentDefinitionWriteResponseDto>
  listWorkflowAgents(
    request: ListWorkflowAgentsRequestDto,
  ): Promise<ListWorkflowAgentsResponseDto>
  getWorkflowAgentDetail(
    request: GetWorkflowAgentDetailRequestDto,
  ): Promise<WorkflowAgentDetailDto>
  getWorkflowAgentGraphProjection?(
    request: GetWorkflowAgentGraphProjectionRequestDto,
  ): Promise<WorkflowAgentGraphProjectionDto>
  getAgentAuthoringCatalog(
    request: GetAgentAuthoringCatalogRequestDto,
  ): Promise<AgentAuthoringCatalogDto>
  searchAgentAuthoringSkills?(
    request: SearchAgentAuthoringSkillsRequestDto,
  ): Promise<SearchAgentAuthoringSkillsResponseDto>
  resolveAgentAuthoringSkill?(
    request: ResolveAgentAuthoringSkillRequestDto,
  ): Promise<AgentAuthoringAttachableSkillDto>
  listAgentSessions(request: ListAgentSessionsRequestDto): Promise<ListAgentSessionsResponseDto>
  getAgentSession(request: GetAgentSessionRequestDto): Promise<AgentSessionDto | null>
  updateAgentSession(request: UpdateAgentSessionRequestDto): Promise<AgentSessionDto>
  autoNameAgentSession(request: AutoNameAgentSessionRequestDto): Promise<AgentSessionDto>
  archiveAgentSession(request: ArchiveAgentSessionRequestDto): Promise<AgentSessionDto>
  restoreAgentSession(request: RestoreAgentSessionRequestDto): Promise<AgentSessionDto>
  deleteAgentSession(request: DeleteAgentSessionRequestDto): Promise<void>
  getAutonomousRun(projectId: string, agentSessionId: string): Promise<AutonomousRunStateDto>
  startAgentTask?(
    projectId: string,
    agentSessionId: string,
    prompt: string,
    options?: { controls?: RuntimeRunControlInputDto | null },
  ): Promise<AgentRunDto>
  sendAgentMessage?(
    runId: string,
    prompt: string,
    options?: { autoCompact?: SendAgentMessageRequestDto['autoCompact'] },
  ): Promise<AgentRunDto>
  cancelAgentRun?(runId: string): Promise<AgentRunDto>
  resumeAgentRun?(
    runId: string,
    response: string,
    options?: { autoCompact?: ResumeAgentRunRequestDto['autoCompact'] },
  ): Promise<AgentRunDto>
  getAgentRun?(runId: string): Promise<AgentRunDto>
  exportAgentTrace?(runId: string, options?: { includeSupportBundle?: boolean }): Promise<AgentTraceExportDto>
  listAgentRuns?(projectId: string, agentSessionId: string): Promise<ListAgentRunsResponseDto>
  getSessionTranscript?(request: GetSessionTranscriptRequestDto): Promise<SessionTranscriptDto>
  exportSessionTranscript?(
    request: ExportSessionTranscriptRequestDto,
  ): Promise<SessionTranscriptExportResponseDto>
  saveSessionTranscriptExport?(request: SaveSessionTranscriptExportRequestDto): Promise<void>
  searchSessionTranscripts?(
    request: SearchSessionTranscriptsRequestDto,
  ): Promise<SearchSessionTranscriptsResponseDto>
  getSessionContextSnapshot?(
    request: GetSessionContextSnapshotRequestDto,
  ): Promise<SessionContextSnapshotDto>
  compactSessionHistory?(
    request: CompactSessionHistoryRequestDto,
  ): Promise<CompactSessionHistoryResponseDto>
  branchAgentSession?(request: BranchAgentSessionRequestDto): Promise<AgentSessionBranchResponseDto>
  rewindAgentSession?(request: RewindAgentSessionRequestDto): Promise<AgentSessionBranchResponseDto>
  listSessionMemories?(
    request: ListSessionMemoriesRequestDto,
  ): Promise<ListSessionMemoriesResponseDto>
  extractSessionMemoryCandidates?(
    request: ExtractSessionMemoryCandidatesRequestDto,
  ): Promise<ExtractSessionMemoryCandidatesResponseDto>
  updateSessionMemory?(request: UpdateSessionMemoryRequestDto): Promise<SessionMemoryRecordDto>
  deleteSessionMemory?(request: DeleteSessionMemoryRequestDto): Promise<void>
  getRuntimeRun(projectId: string, agentSessionId: string): Promise<RuntimeRunDto | null>
  getRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  listMcpServers(): Promise<McpRegistryDto>
  upsertMcpServer(request: UpsertMcpServerRequestDto): Promise<McpRegistryDto>
  removeMcpServer(serverId: string): Promise<McpRegistryDto>
  importMcpServers(path: string): Promise<ImportMcpServersResponseDto>
  refreshMcpServerStatuses(options?: { serverIds?: string[] }): Promise<McpRegistryDto>
  listSkillRegistry(request?: Partial<ListSkillRegistryRequestDto>): Promise<SkillRegistryDto>
  reloadSkillRegistry(request?: Partial<ListSkillRegistryRequestDto>): Promise<SkillRegistryDto>
  setSkillEnabled(request: SetSkillEnabledRequestDto): Promise<SkillRegistryDto>
  removeSkill(request: RemoveSkillRequestDto): Promise<SkillRegistryDto>
  upsertSkillLocalRoot(request: UpsertSkillLocalRootRequestDto): Promise<SkillRegistryDto>
  removeSkillLocalRoot(request: RemoveSkillLocalRootRequestDto): Promise<SkillRegistryDto>
  updateProjectSkillSource(request: UpdateProjectSkillSourceRequestDto): Promise<SkillRegistryDto>
  updateGithubSkillSource(request: UpdateGithubSkillSourceRequestDto): Promise<SkillRegistryDto>
  upsertPluginRoot(request: UpsertPluginRootRequestDto): Promise<SkillRegistryDto>
  removePluginRoot(request: RemovePluginRootRequestDto): Promise<SkillRegistryDto>
  setPluginEnabled(request: SetPluginEnabledRequestDto): Promise<SkillRegistryDto>
  removePlugin(request: RemovePluginRequestDto): Promise<SkillRegistryDto>
  getProviderModelCatalog(
    profileId: string,
    options?: { forceRefresh?: boolean },
  ): Promise<ProviderModelCatalogDto>
  preflightProviderProfile(
    profileId: string,
    options?: {
      forceRefresh?: boolean
      modelId?: string | null
      requiredFeatures?: Partial<ProviderPreflightRequiredFeaturesDto>
    },
  ): Promise<ProviderPreflightSnapshotDto>
  runDoctorReport(request?: Partial<RunDoctorReportRequestDto>): Promise<XeroDoctorReportDto>
  checkProviderProfile(
    profileId: string,
    options?: { includeNetwork?: boolean; modelId?: string | null },
  ): Promise<ProviderProfileDiagnosticsDto>
  startOpenAiLogin(options?: StartOpenAiLoginOptions): Promise<ProviderAuthSessionDto>
  submitOpenAiCallback(
    flowId: string,
    options?: SubmitOpenAiCallbackOptions,
  ): Promise<ProviderAuthSessionDto>
  startAutonomousRun(
    projectId: string,
    agentSessionId: string,
    options?: StartRuntimeRunOptions,
  ): Promise<AutonomousRunStateDto>
  startRuntimeRun(
    projectId: string,
    agentSessionId: string,
    options?: StartRuntimeRunOptions,
  ): Promise<RuntimeRunDto>
  stageAgentAttachment(input: StageAgentAttachmentInput): Promise<StagedAgentAttachmentDto>
  discardAgentAttachment(projectId: string, absolutePath: string): Promise<void>
  updateRuntimeRunControls(request: UpdateRuntimeRunControlsRequestDto): Promise<RuntimeRunDto>
  startRuntimeSession(projectId: string, options?: StartRuntimeSessionOptions): Promise<RuntimeSessionDto>
  cancelAutonomousRun(projectId: string, agentSessionId: string, runId: string): Promise<AutonomousRunStateDto>
  stopRuntimeRun(projectId: string, agentSessionId: string, runId: string): Promise<RuntimeRunDto | null>
  logoutRuntimeSession(projectId: string): Promise<RuntimeSessionDto>
  listProviderCredentials(): Promise<ProviderCredentialsSnapshotDto>
  upsertProviderCredential(
    request: UpsertProviderCredentialRequestDto,
  ): Promise<ProviderCredentialsSnapshotDto>
  deleteProviderCredential(providerId: string): Promise<ProviderCredentialsSnapshotDto>
  startOAuthLogin(request: StartOAuthLoginRequestDto): Promise<ProviderAuthSessionDto>
  completeOAuthCallback(request: CompleteOAuthCallbackRequestDto): Promise<ProviderAuthSessionDto>
  resolveOperatorAction(
    projectId: string,
    actionId: string,
    decision: 'approve' | 'reject',
    options?: { userAnswer?: string | null },
  ): Promise<ResolveOperatorActionResponseDto>
  resumeOperatorRun(
    projectId: string,
    actionId: string,
    options?: { userAnswer?: string | null },
  ): Promise<ResumeOperatorRunResponseDto>
  listNotificationRoutes(projectId: string): Promise<ListNotificationRoutesResponseDto>
  listNotificationDispatches(
    projectId: string,
    options?: { actionId?: string | null },
  ): Promise<ListNotificationDispatchesResponseDto>
  upsertNotificationRoute(
    request: UpsertNotificationRouteRequestDto,
  ): Promise<UpsertNotificationRouteResponseDto>
  upsertNotificationRouteCredentials(
    request: UpsertNotificationRouteCredentialsRequestDto,
  ): Promise<UpsertNotificationRouteCredentialsResponseDto>
  recordNotificationDispatchOutcome(
    request: RecordNotificationDispatchOutcomeRequestDto,
  ): Promise<RecordNotificationDispatchOutcomeResponseDto>
  submitNotificationReply(request: SubmitNotificationReplyRequestDto): Promise<SubmitNotificationReplyResponseDto>
  syncNotificationAdapters(projectId: string): Promise<SyncNotificationAdaptersResponseDto>
  getEnvironmentDiscoveryStatus?(): Promise<EnvironmentDiscoveryStatusDto>
  getEnvironmentProfileSummary?(): Promise<EnvironmentProfileSummaryDto>
  refreshEnvironmentDiscovery?(): Promise<EnvironmentDiscoveryStatusDto>
  verifyUserEnvironmentTool?(
    request: VerifyUserToolRequestDto,
  ): Promise<VerifyUserToolResponseDto>
  saveUserEnvironmentTool?(
    request: SaveUserToolRequestDto,
  ): Promise<EnvironmentProbeReportDto>
  removeUserEnvironmentTool?(id: string): Promise<EnvironmentProbeReportDto>
  resolveEnvironmentPermissionRequests?(
    request: ResolveEnvironmentPermissionRequestsDto,
  ): Promise<EnvironmentDiscoveryStatusDto>
  startEnvironmentDiscovery?(): Promise<EnvironmentDiscoveryStatusDto>
  speechDictationStatus?(): Promise<DictationStatusDto>
  speechDictationSettings?(): Promise<DictationSettingsDto>
  speechDictationUpdateSettings?(
    request: UpsertDictationSettingsRequestDto,
  ): Promise<DictationSettingsDto>
  speechDictationStart?(
    request: DictationStartRequestInputDto,
    handler: (event: DictationEventDto) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<XeroDictationSession>
  speechDictationStop?(): Promise<void>
  speechDictationCancel?(): Promise<void>
  browserControlSettings?(): Promise<BrowserControlSettingsDto>
  browserControlUpdateSettings?(
    request: UpsertBrowserControlSettingsRequestDto,
  ): Promise<BrowserControlSettingsDto>
  soulSettings?(): Promise<SoulSettingsDto>
  soulUpdateSettings?(request: UpsertSoulSettingsRequestDto): Promise<SoulSettingsDto>
  browserEval(js: string, options?: { timeoutMs?: number }): Promise<unknown>
  browserCurrentUrl(): Promise<string | null>
  browserScreenshot(): Promise<string>
  browserNavigate(url: string, options?: { tabId?: string }): Promise<void>
  browserBack(): Promise<unknown>
  browserForward(): Promise<unknown>
  browserReload(options?: { tabId?: string }): Promise<void>
  browserStop(): Promise<unknown>
  browserClick(selector: string, options?: { timeoutMs?: number }): Promise<unknown>
  browserType(
    selector: string,
    text: string,
    options?: { append?: boolean; timeoutMs?: number },
  ): Promise<unknown>
  browserScroll(options?: {
    selector?: string
    x?: number
    y?: number
    timeoutMs?: number
  }): Promise<unknown>
  browserPressKey(
    key: string,
    options?: { selector?: string; timeoutMs?: number },
  ): Promise<unknown>
  browserReadText(options?: { selector?: string; timeoutMs?: number }): Promise<unknown>
  browserQuery(
    selector: string,
    options?: { limit?: number; timeoutMs?: number },
  ): Promise<unknown>
  browserWaitForSelector(
    selector: string,
    options?: { timeoutMs?: number; visible?: boolean },
  ): Promise<unknown>
  browserWaitForLoad(options?: { timeoutMs?: number }): Promise<unknown>
  browserHistoryState(): Promise<unknown>
  browserCookiesGet(): Promise<unknown>
  browserCookiesSet(cookie: string): Promise<unknown>
  browserStorageRead(area: 'local' | 'session', key?: string): Promise<unknown>
  browserStorageWrite(area: 'local' | 'session', key: string, value: string | null): Promise<unknown>
  browserStorageClear(area: 'local' | 'session'): Promise<unknown>
  browserTabList(): Promise<BrowserTabMetadataDto[]>
  browserTabFocus(tabId: string): Promise<BrowserTabMetadataDto>
  browserTabClose(tabId: string): Promise<BrowserTabMetadataDto[]>
  onBrowserUrlChanged(
    handler: (payload: BrowserUrlChangedPayload) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  onBrowserLoadState(
    handler: (payload: BrowserLoadStatePayload) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  onBrowserConsole(
    handler: (payload: BrowserConsolePayload) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  onBrowserTabUpdated(
    handler: (payload: BrowserTabUpdatedPayload) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  subscribeRuntimeStream(
    projectId: string,
    agentSessionId: string,
    itemKinds: RuntimeStreamItemKindDto[],
    handler: (payload: RuntimeStreamChannelPayloadDto) => void,
    onError?: (error: XeroDesktopError) => void,
    options?: { afterSequence?: number | null; replayLimit?: number | null },
  ): Promise<XeroRuntimeStreamSubscription>
  subscribeAgentStream?(
    runId: string,
    handler: (payload: AgentRunEventDto) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<XeroAgentStreamSubscription>
  onProjectUpdated(
    handler: (payload: ProjectUpdatedPayloadDto) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  onRepositoryStatusChanged(
    handler: (payload: RepositoryStatusChangedPayloadDto) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  onRuntimeUpdated(
    handler: (payload: RuntimeUpdatedPayloadDto) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  onRuntimeRunUpdated(
    handler: (payload: RuntimeRunUpdatedPayloadDto) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
  onAgentUsageUpdated(
    handler: (payload: AgentUsageUpdatedPayloadDto) => void,
    onError?: (error: XeroDesktopError) => void,
  ): Promise<UnlistenFn>
}

function ensureDesktopRuntime(context: string): void {
  if (!isTauri()) {
    throw new XeroDesktopError({
      code: 'desktop_runtime_unavailable',
      errorClass: 'desktop_runtime_unavailable',
      message: `${context} requires the Tauri desktop runtime.`,
    })
  }
}

function normalizeError(error: unknown, context: string): XeroDesktopError {
  const commandError = commandErrorSchema.safeParse(error)
  if (commandError.success) {
    return new XeroDesktopError({
      code: commandError.data.code,
      errorClass: commandError.data.class,
      message: commandError.data.message,
      retryable: commandError.data.retryable,
      cause: error,
    })
  }

  if (error instanceof ZodError) {
    return new XeroDesktopError({
      code: 'adapter_contract_mismatch',
      errorClass: 'adapter_contract_mismatch',
      message: `${context} returned an unexpected payload shape.`,
      cause: error,
    })
  }

  if (error instanceof XeroDesktopError) {
    return error
  }

  if (error instanceof Error) {
    return new XeroDesktopError({
      message: error.message,
      cause: error,
    })
  }

  return new XeroDesktopError({
    message: `${context} failed for an unknown reason.`,
    cause: error,
  })
}

const LARGE_RESPONSE_CONTRACT_COMMANDS = new Set<string>([
  'get_agent_authoring_catalog',
  'get_provider_model_catalog',
  'get_project_load_bundle',
  'get_workflow_agent_detail',
  'get_workflow_agent_graph_projection',
  'list_agent_definitions',
  'list_skill_registry',
  'list_workflow_agents',
  'reload_skill_registry',
])

export function shouldValidateCommandResponse(
  command: string,
  mode: string | undefined = import.meta.env?.MODE,
): boolean {
  const normalizedMode = mode ?? 'production'
  if (normalizedMode === 'development' || normalizedMode === 'test') {
    return true
  }
  return !LARGE_RESPONSE_CONTRACT_COMMANDS.has(command)
}

async function invokeTyped<TResponse>(
  command: string,
  schema: z.ZodType<TResponse, z.ZodTypeDef, unknown>,
  args?: Record<string, unknown>,
): Promise<TResponse> {
  ensureDesktopRuntime(`Command ${command}`)

  try {
    const response = await invoke(command, args)
    recordIpcPayloadSample({ boundary: 'command', name: command, payload: response })
    if (!shouldValidateCommandResponse(command)) {
      return response as TResponse
    }
    return schema.parse(response)
  } catch (error) {
    throw normalizeError(error, `Command ${command}`)
  }
}

async function invokeTypedDeduped<TResponse>(
  command: string,
  schema: z.ZodType<TResponse, z.ZodTypeDef, unknown>,
  args?: Record<string, unknown>,
): Promise<TResponse> {
  const requestKey = backendRequestKey(command, args)
  return backendRequestCoordinator.runDeduped(requestKey, () => invokeTyped(command, schema, args))
}

async function invokeRaw(command: string, args?: Record<string, unknown>): Promise<void> {
  ensureDesktopRuntime(`Command ${command}`)

  try {
    await invoke(command, args)
  } catch (error) {
    throw normalizeError(error, `Command ${command}`)
  }
}

async function listenTyped<TPayload>(
  eventName: string,
  schema: z.ZodType<TPayload, z.ZodTypeDef, unknown>,
  handler: (payload: TPayload) => void,
  onError?: (error: XeroDesktopError) => void,
): Promise<UnlistenFn> {
  if (!isTauri()) {
    return () => undefined
  }

  const unlisten = await listen(eventName, (event) => {
    try {
      recordIpcPayloadSample({ boundary: 'event', name: eventName, payload: event.payload })
      handler(schema.parse(event.payload))
    } catch (error) {
      onError?.(normalizeError(error, `Event ${eventName}`))
    }
  })

  return createSafeTauriUnlisten(unlisten)
}

const RUNTIME_STREAM_ITEM_KINDS = new Set<string>([
  'transcript',
  'tool',
  'skill',
  'activity',
  'action_required',
  'plan',
  'complete',
  'failure',
  'subagent_lifecycle',
])

type RuntimeStreamChannelPayloadDto = RuntimeStreamEventDto | RuntimeStreamPatchDto

const RUNTIME_STREAM_DELIVERY_BATCH_SIZE = 24
const RUNTIME_STREAM_DELIVERY_TIME_BUDGET_MS = 4

function scheduleRuntimeStreamDelivery(callback: () => void): ReturnType<typeof setTimeout> {
  return setTimeout(callback, 0)
}

function nowForDeliveryBudget(): number {
  return typeof performance !== 'undefined' && typeof performance.now === 'function'
    ? performance.now()
    : Date.now()
}

function hasCheapRuntimeStreamItemShape(payload: unknown): payload is RuntimeStreamItemDto {
  if (!payload || typeof payload !== 'object') {
    return false
  }

  const item = payload as Record<string, unknown>
  return (
    typeof item.kind === 'string' &&
    RUNTIME_STREAM_ITEM_KINDS.has(item.kind) &&
    typeof item.runId === 'string' &&
    item.runId.trim().length > 0 &&
    typeof item.sequence === 'number' &&
    Number.isInteger(item.sequence) &&
    item.sequence > 0 &&
    typeof item.createdAt === 'string' &&
    item.createdAt.trim().length > 0
  )
}

function hasRuntimeStreamPatchEnvelope(payload: unknown): payload is { schema: string } {
  return (
    Boolean(payload)
    && typeof payload === 'object'
    && (payload as { schema?: unknown }).schema === 'xero.runtime_stream_patch.v1'
  )
}

function hasCheapRuntimeStreamPatchShape(payload: unknown): payload is RuntimeStreamPatchDto {
  if (!hasRuntimeStreamPatchEnvelope(payload)) {
    return false
  }

  const patch = payload as {
    item?: unknown
    snapshot?: {
      schema?: unknown
      projectId?: unknown
      agentSessionId?: unknown
      runId?: unknown
      sessionId?: unknown
      lastSequence?: unknown
    }
  }
  const snapshot = patch.snapshot
  return (
    hasCheapRuntimeStreamItemShape(patch.item)
    && Boolean(snapshot)
    && snapshot?.schema === 'xero.runtime_stream_view_snapshot.v1'
    && typeof snapshot.projectId === 'string'
    && snapshot.projectId.trim().length > 0
    && typeof snapshot.agentSessionId === 'string'
    && snapshot.agentSessionId.trim().length > 0
    && typeof snapshot.runId === 'string'
    && snapshot.runId.trim().length > 0
    && typeof snapshot.sessionId === 'string'
    && snapshot.sessionId.trim().length > 0
    && (
      snapshot.lastSequence == null
      || (typeof snapshot.lastSequence === 'number' && Number.isInteger(snapshot.lastSequence))
    )
  )
}

function shouldUseFullRuntimeStreamChannelValidation(): boolean {
  return import.meta.env.MODE === 'test' || import.meta.env.VITE_XERO_RUNTIME_STREAM_ZOD === '1'
}

function parseRuntimeStreamChannelPayload(payload: unknown): RuntimeStreamItemDto | RuntimeStreamPatchDto {
  const isPatchEnvelope = hasRuntimeStreamPatchEnvelope(payload)
  const budgetSample = recordIpcPayloadSample({
    boundary: 'channel',
    budgetKey: 'runtimeStreamItem',
    name: isPatchEnvelope ? 'subscribe_runtime_stream:patch' : 'subscribe_runtime_stream:item',
    payload,
  })
  if (budgetSample?.overMaxBudget) {
    throw new XeroDesktopError({
      code: 'ipc_payload_budget_exceeded',
      errorClass: 'adapter_contract_mismatch',
      message: `Xero dropped an oversized runtime stream item (${budgetSample.observedBytes} bytes; budget ${budgetSample.budget.maxBytes} bytes).`,
      retryable: true,
    })
  }

  if (shouldUseFullRuntimeStreamChannelValidation()) {
    if (isPatchEnvelope) {
      return runtimeStreamPatchSchema.parse(payload)
    }
    return runtimeStreamItemSchema.parse(payload)
  }

  if (hasCheapRuntimeStreamPatchShape(payload)) {
    return payload
  }

  if (hasCheapRuntimeStreamItemShape(payload)) {
    return payload
  }

  throw new XeroDesktopError({
    code: 'adapter_contract_mismatch',
    errorClass: 'adapter_contract_mismatch',
    message: `Command ${COMMANDS.subscribeRuntimeStream} channel returned a malformed stream item envelope.`,
  })
}

function isRuntimeStreamPatchPayload(
  payload: RuntimeStreamItemDto | RuntimeStreamPatchDto,
): payload is RuntimeStreamPatchDto {
  return 'schema' in payload && payload.schema === 'xero.runtime_stream_patch.v1'
}

async function createRuntimeStreamSubscription(
  projectId: string,
  agentSessionId: string,
  itemKinds: RuntimeStreamItemKindDto[],
  handler: (payload: RuntimeStreamChannelPayloadDto) => void,
  onError?: (error: XeroDesktopError) => void,
  options: { afterSequence?: number | null; replayLimit?: number | null } = {},
): Promise<XeroRuntimeStreamSubscription> {
  ensureDesktopRuntime(`Command ${COMMANDS.subscribeRuntimeStream}`)

  let disposed = false
  let response: SubscribeRuntimeStreamResponseDto | null = null
  let lastDeliveredSequence: number | null = null
  const deliveryQueue: unknown[] = []
  let deliveryQueueCursor = 0
  let scheduledDelivery: ReturnType<typeof setTimeout> | null = null
  const channel = new Channel<unknown>()

  const unsubscribe = () => {
    disposed = true
    lastDeliveredSequence = null
    deliveryQueue.length = 0
    deliveryQueueCursor = 0
    if (scheduledDelivery) {
      clearTimeout(scheduledDelivery)
      scheduledDelivery = null
    }
    channel.onmessage = () => undefined
  }

  const deliver = (payload: unknown, activeResponse: SubscribeRuntimeStreamResponseDto) => {
    if (disposed) {
      return
    }

    try {
      const channelPayload = parseRuntimeStreamChannelPayload(payload)
      const isPatch = isRuntimeStreamPatchPayload(channelPayload)
      const item = isPatch ? channelPayload.item : channelPayload
      if (item.runId !== activeResponse.runId) {
        throw new XeroDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: `Command ${COMMANDS.subscribeRuntimeStream} channel returned a stream item for run ${item.runId} while ${activeResponse.runId} is subscribed.`,
        })
      }
      if (isPatch) {
        if (channelPayload.snapshot.runId !== activeResponse.runId) {
          throw new XeroDesktopError({
            code: 'adapter_contract_mismatch',
            errorClass: 'adapter_contract_mismatch',
            message: `Command ${COMMANDS.subscribeRuntimeStream} channel returned a stream snapshot for run ${channelPayload.snapshot.runId} while ${activeResponse.runId} is subscribed.`,
          })
        }
        if (
          channelPayload.snapshot.projectId !== activeResponse.projectId
          || channelPayload.snapshot.agentSessionId !== activeResponse.agentSessionId
        ) {
          throw new XeroDesktopError({
            code: 'adapter_contract_mismatch',
            errorClass: 'adapter_contract_mismatch',
            message: `Command ${COMMANDS.subscribeRuntimeStream} channel returned a stream snapshot for a different project or agent session.`,
          })
        }
      }

      const deliveredSequence = isPatch
        ? channelPayload.snapshot.lastSequence ?? item.sequence
        : item.sequence
      if (lastDeliveredSequence !== null) {
        if (deliveredSequence < lastDeliveredSequence) {
          return
        }

        if (deliveredSequence === lastDeliveredSequence) {
          return
        }
      }

      lastDeliveredSequence = deliveredSequence

      if (isPatch) {
        handler(channelPayload)
        return
      }

      handler({
        projectId: activeResponse.projectId,
        agentSessionId: activeResponse.agentSessionId,
        runtimeKind: activeResponse.runtimeKind,
        runId: activeResponse.runId,
        sessionId: activeResponse.sessionId,
        flowId: activeResponse.flowId ?? null,
        subscribedItemKinds: activeResponse.subscribedItemKinds,
        item,
      })
    } catch (error) {
      onError?.(normalizeError(error, `Command ${COMMANDS.subscribeRuntimeStream} channel`))
    }
  }

  const hasQueuedPayloads = () => deliveryQueueCursor < deliveryQueue.length

  const compactDeliveryQueue = () => {
    if (deliveryQueueCursor === 0) {
      return
    }

    if (deliveryQueueCursor >= deliveryQueue.length) {
      deliveryQueue.length = 0
      deliveryQueueCursor = 0
      return
    }

    if (deliveryQueueCursor >= 256) {
      deliveryQueue.splice(0, deliveryQueueCursor)
      deliveryQueueCursor = 0
    }
  }

  const scheduleDelivery = () => {
    if (disposed || !response || scheduledDelivery || !hasQueuedPayloads()) {
      return
    }

    scheduledDelivery = scheduleRuntimeStreamDelivery(() => {
      scheduledDelivery = null
      const activeResponse = response
      if (disposed || !activeResponse) {
        return
      }

      const startedAt = nowForDeliveryBudget()
      let deliveredCount = 0
      while (deliveryQueueCursor < deliveryQueue.length) {
        const payload = deliveryQueue[deliveryQueueCursor]
        deliveryQueueCursor += 1
        deliver(payload, activeResponse)
        deliveredCount += 1

        if (
          deliveredCount >= RUNTIME_STREAM_DELIVERY_BATCH_SIZE ||
          nowForDeliveryBudget() - startedAt >= RUNTIME_STREAM_DELIVERY_TIME_BUDGET_MS
        ) {
          break
        }
      }

      compactDeliveryQueue()
      if (hasQueuedPayloads()) {
        scheduleDelivery()
      }
    })
  }

  const enqueueDelivery = (payload: unknown) => {
    if (disposed) {
      return
    }

    deliveryQueue.push(payload)
    scheduleDelivery()
  }

  channel.onmessage = enqueueDelivery

  try {
    const request = subscribeRuntimeStreamRequestSchema.parse({
      projectId,
      agentSessionId,
      itemKinds,
      afterSequence: options.afterSequence ?? null,
      replayLimit: options.replayLimit ?? null,
    })
    response = await invokeTyped(COMMANDS.subscribeRuntimeStream, subscribeRuntimeStreamResponseSchema, {
      request: {
        projectId: request.projectId,
        agentSessionId: request.agentSessionId,
        itemKinds: request.itemKinds,
        afterSequence: request.afterSequence ?? null,
        replayLimit: request.replayLimit ?? null,
        channel,
      },
    })

    scheduleDelivery()

    return {
      response,
      unsubscribe,
    }
  } catch (error) {
    unsubscribe()
    throw normalizeError(error, `Command ${COMMANDS.subscribeRuntimeStream}`)
  }
}

async function createAgentStreamSubscription(
  runId: string,
  handler: (payload: AgentRunEventDto) => void,
  onError?: (error: XeroDesktopError) => void,
): Promise<XeroAgentStreamSubscription> {
  ensureDesktopRuntime(`Command ${COMMANDS.subscribeAgentStream}`)

  let disposed = false
  let response: SubscribeAgentStreamResponseDto | null = null
  let lastDeliveredId: number | null = null
  const pendingPayloads: unknown[] = []
  const channel = new Channel<unknown>()

  const unsubscribe = () => {
    disposed = true
    lastDeliveredId = null
    pendingPayloads.length = 0
    channel.onmessage = () => undefined
  }

  const deliver = (payload: unknown, activeResponse: SubscribeAgentStreamResponseDto) => {
    if (disposed) {
      return
    }

    try {
      const event = agentRunEventSchema.parse(payload)
      if (event.runId !== activeResponse.runId) {
        throw new XeroDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: `Command ${COMMANDS.subscribeAgentStream} channel returned an event for run ${event.runId} while ${activeResponse.runId} is subscribed.`,
        })
      }

      if (lastDeliveredId !== null) {
        if (event.id < lastDeliveredId) {
          throw new XeroDesktopError({
            code: 'adapter_contract_mismatch',
            errorClass: 'adapter_contract_mismatch',
            message: `Command ${COMMANDS.subscribeAgentStream} channel returned non-monotonic event id ${event.id} after ${lastDeliveredId} for run ${event.runId}.`,
          })
        }

        if (event.id === lastDeliveredId) {
          return
        }
      }

      lastDeliveredId = event.id
      handler(event)
    } catch (error) {
      onError?.(normalizeError(error, `Command ${COMMANDS.subscribeAgentStream} channel`))
    }
  }

  channel.onmessage = (payload) => {
    if (disposed) {
      return
    }

    if (!response) {
      pendingPayloads.push(payload)
      return
    }

    deliver(payload, response)
  }

  try {
    const request = subscribeAgentStreamRequestSchema.parse({ runId })
    response = await invokeTyped(COMMANDS.subscribeAgentStream, subscribeAgentStreamResponseSchema, {
      request: {
        runId: request.runId,
        channel,
      },
    })

    for (const pendingPayload of pendingPayloads.splice(0, pendingPayloads.length)) {
      deliver(pendingPayload, response)
    }

    return {
      response,
      unsubscribe,
    }
  } catch (error) {
    unsubscribe()
    throw normalizeError(error, `Command ${COMMANDS.subscribeAgentStream}`)
  }
}

async function createDictationSession(
  request: DictationStartRequestInputDto,
  handler: (event: DictationEventDto) => void,
  onError?: (error: XeroDesktopError) => void,
): Promise<XeroDictationSession> {
  ensureDesktopRuntime(`Command ${COMMANDS.speechDictationStart}`)

  let disposed = false
  let response: DictationStartResponseDto | null = null
  const pendingPayloads: unknown[] = []
  const channel = new Channel<unknown>()

  const unsubscribe = () => {
    disposed = true
    pendingPayloads.length = 0
    channel.onmessage = () => undefined
  }

  const deliver = (payload: unknown, activeResponse: DictationStartResponseDto) => {
    if (disposed) {
      return
    }

    try {
      const event = dictationEventSchema.parse(payload)
      if ('sessionId' in event && event.sessionId !== activeResponse.sessionId) {
        throw new XeroDesktopError({
          code: 'adapter_contract_mismatch',
          errorClass: 'adapter_contract_mismatch',
          message: `Command ${COMMANDS.speechDictationStart} channel returned a dictation event for session ${event.sessionId} while ${activeResponse.sessionId} is active.`,
        })
      }
      handler(event)
    } catch (error) {
      onError?.(normalizeError(error, `Command ${COMMANDS.speechDictationStart} channel`))
    }
  }

  channel.onmessage = (payload) => {
    if (disposed) {
      return
    }

    if (response === null) {
      pendingPayloads.push(payload)
      return
    }

    deliver(payload, response)
  }

  try {
    const parsedRequest = dictationStartRequestSchema.parse(request)
    response = await invokeTyped(COMMANDS.speechDictationStart, dictationStartResponseSchema, {
      request: {
        ...parsedRequest,
        channel,
      },
    })

    for (const pendingPayload of pendingPayloads.splice(0, pendingPayloads.length)) {
      deliver(pendingPayload, response)
    }

    return {
      response,
      unsubscribe,
      stop: () => invokeRaw(COMMANDS.speechDictationStop),
      cancel: () => invokeRaw(COMMANDS.speechDictationCancel),
    }
  } catch (error) {
    unsubscribe()
    throw normalizeError(error, `Command ${COMMANDS.speechDictationStart}`)
  }
}

export const XeroDesktopAdapter: XeroDesktopAdapter = {
  isDesktopRuntime() {
    return isTauri()
  },

  async pickRepositoryFolder() {
    ensureDesktopRuntime('Repository import')

    try {
      const selected = await open({
        directory: true,
        multiple: false,
      })

      if (selected === null) {
        return null
      }

      const path = Array.isArray(selected) ? selected[0] : selected
      return typeof path === 'string' && path.trim().length > 0 ? path : null
    } catch (error) {
      throw normalizeError(error, 'Repository import')
    }
  },

  async pickParentFolder() {
    ensureDesktopRuntime('Project create')

    try {
      const selected = await open({
        directory: true,
        multiple: false,
      })

      if (selected === null) {
        return null
      }

      const path = Array.isArray(selected) ? selected[0] : selected
      return typeof path === 'string' && path.trim().length > 0 ? path : null
    } catch (error) {
      throw normalizeError(error, 'Project create')
    }
  },

  importRepository(path) {
    return invokeTyped(COMMANDS.importRepository, importRepositoryResponseSchema, {
      request: { path },
    })
  },

  createRepository(parentPath, name) {
    return invokeTyped(COMMANDS.createRepository, importRepositoryResponseSchema, {
      request: { parentPath, name },
    })
  },

  listProjects() {
    return invokeTyped(COMMANDS.listProjects, listProjectsResponseSchema)
  },

  removeProject(projectId) {
    return invokeTyped(COMMANDS.removeProject, listProjectsResponseSchema, {
      request: { projectId },
    })
  },

  readAppUiState(request) {
    const parsed = readAppUiStateRequestSchema.parse(request)
    return invokeTyped(COMMANDS.readAppUiState, appUiStateResponseSchema, {
      request: parsed,
    })
  },

  writeAppUiState(request) {
    const parsed = writeAppUiStateRequestSchema.parse(request)
    return invokeTyped(COMMANDS.writeAppUiState, appUiStateResponseSchema, {
      request: parsed,
    })
  },

  readProjectUiState(request) {
    const parsed = readProjectUiStateRequestSchema.parse(request)
    return invokeTyped(COMMANDS.readProjectUiState, projectUiStateResponseSchema, {
      request: parsed,
    })
  },

  writeProjectUiState(request) {
    const parsed = writeProjectUiStateRequestSchema.parse(request)
    return invokeTyped(COMMANDS.writeProjectUiState, projectUiStateResponseSchema, {
      request: parsed,
    })
  },

  getProjectSnapshot(projectId) {
    return invokeTyped(COMMANDS.getProjectSnapshot, projectSnapshotResponseSchema, {
      request: { projectId },
    })
  },

  getProjectLoadBundle(request) {
    const parsed = projectLoadBundleRequestSchema.parse(request)
    return invokeTyped(COMMANDS.getProjectLoadBundle, projectLoadBundleSchema, {
      request: parsed,
    })
  },

  getProjectUsageSummary(projectId) {
    return invokeTyped(COMMANDS.getProjectUsageSummary, projectUsageSummarySchema, {
      request: { projectId },
    })
  },

  getRepositoryStatus(projectId) {
    return invokeTypedDeduped(COMMANDS.getRepositoryStatus, repositoryStatusResponseSchema, {
      request: { projectId },
    })
  },

  getRepositoryDiff(projectId, scope) {
    return invokeTypedDeduped(COMMANDS.getRepositoryDiff, repositoryDiffResponseSchema, {
      request: { projectId, scope },
    })
  },

  applySelectiveUndo(request) {
    const parsedRequest = selectiveUndoRequestSchema.parse(request)
    return invokeTyped(COMMANDS.applySelectiveUndo, selectiveUndoResponseSchema, { request: parsedRequest })
  },

  returnSessionToHere(request) {
    const parsedRequest = returnSessionToHereRequestSchema.parse(request)
    return invokeTyped(COMMANDS.returnSessionToHere, returnSessionToHereResponseSchema, { request: parsedRequest })
  },

  async gitStagePaths(projectId, paths) {
    const request = gitPathsRequestSchema.parse({ projectId, paths })
    await invokeRaw(COMMANDS.gitStagePaths, { request })
  },

  async gitUnstagePaths(projectId, paths) {
    const request = gitPathsRequestSchema.parse({ projectId, paths })
    await invokeRaw(COMMANDS.gitUnstagePaths, { request })
  },

  async gitDiscardChanges(projectId, paths) {
    const request = gitPathsRequestSchema.parse({ projectId, paths })
    await invokeRaw(COMMANDS.gitDiscardChanges, { request })
  },

  gitCommit(projectId, message) {
    const request = gitCommitRequestSchema.parse({ projectId, message })
    return invokeTyped(COMMANDS.gitCommit, gitCommitResponseSchema, { request })
  },

  gitGenerateCommitMessage(request) {
    const parsedRequest = gitGenerateCommitMessageRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.gitGenerateCommitMessage,
      gitGenerateCommitMessageResponseSchema,
      { request: parsedRequest },
    )
  },

  gitFetch(projectId, remote) {
    const request = gitRemoteRequestSchema.parse({ projectId, remote: remote ?? null })
    return invokeTyped(COMMANDS.gitFetch, gitFetchResponseSchema, { request })
  },

  gitPull(projectId, remote) {
    const request = gitRemoteRequestSchema.parse({ projectId, remote: remote ?? null })
    return invokeTyped(COMMANDS.gitPull, gitPullResponseSchema, { request })
  },

  gitPush(projectId, remote) {
    const request = gitRemoteRequestSchema.parse({ projectId, remote: remote ?? null })
    return invokeTyped(COMMANDS.gitPush, gitPushResponseSchema, { request })
  },

  listProjectFiles(projectId, path = '/') {
    const request: ListProjectFilesRequestDto = listProjectFilesRequestSchema.parse({ projectId, path })
    return invokeTypedDeduped(COMMANDS.listProjectFiles, listProjectFilesResponseSchema, {
      request,
    })
  },

  readProjectFile(projectId, path) {
    const request = projectFileRequestSchema.parse({ projectId, path })
    return invokeTypedDeduped(COMMANDS.readProjectFile, readProjectFileResponseSchema, {
      request,
    })
  },

  writeProjectFile(projectId, path, content) {
    const request = writeProjectFileRequestSchema.parse({ projectId, path, content })
    return invokeTyped(COMMANDS.writeProjectFile, writeProjectFileResponseSchema, {
      request,
    })
  },

  async revokeProjectAssetTokens(projectId, paths = []) {
    const request: RevokeProjectAssetTokensRequestDto = revokeProjectAssetTokensRequestSchema.parse({
      projectId,
      paths,
    })
    await invokeRaw(COMMANDS.revokeProjectAssetTokens, { request })
  },

  async openProjectFileExternal(projectId, path) {
    const request = projectFileRequestSchema.parse({ projectId, path })
    await invokeRaw(COMMANDS.openProjectFileExternal, { request })
  },

  createProjectEntry(request) {
    const parsedRequest = createProjectEntryRequestSchema.parse(request)
    return invokeTyped(COMMANDS.createProjectEntry, createProjectEntryResponseSchema, {
      request: parsedRequest,
    })
  },

  renameProjectEntry(request) {
    const parsedRequest = renameProjectEntryRequestSchema.parse(request)
    return invokeTyped(COMMANDS.renameProjectEntry, renameProjectEntryResponseSchema, {
      request: parsedRequest,
    })
  },

  moveProjectEntry(request) {
    const parsedRequest = moveProjectEntryRequestSchema.parse(request)
    return invokeTyped(COMMANDS.moveProjectEntry, moveProjectEntryResponseSchema, {
      request: parsedRequest,
    })
  },

  deleteProjectEntry(projectId, path) {
    const request = projectFileRequestSchema.parse({ projectId, path })
    return invokeTyped(COMMANDS.deleteProjectEntry, deleteProjectEntryResponseSchema, {
      request,
    })
  },

  searchProject(request) {
    const parsed = searchProjectRequestSchema.parse(request)
    return invokeTypedDeduped(COMMANDS.searchProject, searchProjectResponseSchema, {
      request: parsed,
    })
  },

  replaceInProject(request) {
    const parsed = replaceInProjectRequestSchema.parse(request)
    return invokeTyped(COMMANDS.replaceInProject, replaceInProjectResponseSchema, {
      request: parsed,
    })
  },

  workspaceIndex(request) {
    const parsed = workspaceIndexRequestSchema.parse(request)
    return invokeTyped(COMMANDS.workspaceIndex, workspaceIndexResponseSchema, {
      request: parsed,
    })
  },

  workspaceStatus(projectId) {
    const request = z.object({ projectId: z.string().trim().min(1) }).parse({ projectId })
    return invokeTypedDeduped(COMMANDS.workspaceStatus, workspaceIndexStatusSchema, {
      request,
    })
  },

  workspaceQuery(request) {
    const parsed = workspaceQueryRequestSchema.parse(request)
    return invokeTypedDeduped(COMMANDS.workspaceQuery, workspaceQueryResponseSchema, {
      request: parsed,
    })
  },

  workspaceExplain(request) {
    const parsed = workspaceExplainRequestSchema.parse(request)
    return invokeTypedDeduped(COMMANDS.workspaceExplain, workspaceExplainResponseSchema, {
      request: parsed,
    })
  },

  workspaceReset(projectId) {
    const request = z.object({ projectId: z.string().trim().min(1) }).parse({ projectId })
    return invokeTyped(COMMANDS.workspaceReset, workspaceIndexStatusSchema, {
      request,
    })
  },

  createAgentSession(request) {
    const parsed = createAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.createAgentSession, agentSessionSchema, {
      request: {
        projectId: parsed.projectId,
        title: parsed.title ?? null,
        summary: parsed.summary ?? '',
        selected: parsed.selected ?? false,
      },
    })
  },

  listAgentDefinitions(request) {
    const parsed = listAgentDefinitionsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.listAgentDefinitions, listAgentDefinitionsResponseSchema, {
      request: {
        projectId: parsed.projectId,
        includeArchived: parsed.includeArchived,
      },
    })
  },

  archiveAgentDefinition(request) {
    const parsed = archiveAgentDefinitionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.archiveAgentDefinition, agentDefinitionSummarySchema, {
      request: parsed,
    })
  },

  getAgentDefinitionVersion(request) {
    const parsed = getAgentDefinitionVersionRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.getAgentDefinitionVersion,
      agentDefinitionVersionSummarySchema.nullable(),
      { request: parsed },
    )
  },

  saveAgentDefinition(request) {
    const parsed = saveAgentDefinitionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.saveAgentDefinition, agentDefinitionWriteResponseSchema, {
      request: {
        projectId: parsed.projectId,
        definition: parsed.definition,
        definitionId: parsed.definitionId ?? null,
      },
    })
  },

  updateAgentDefinition(request) {
    const parsed = updateAgentDefinitionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.updateAgentDefinition, agentDefinitionWriteResponseSchema, {
      request: {
        projectId: parsed.projectId,
        definitionId: parsed.definitionId,
        definition: parsed.definition,
      },
    })
  },

  listWorkflowAgents(request) {
    const parsed = listWorkflowAgentsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.listWorkflowAgents, listWorkflowAgentsResponseSchema, {
      request: {
        projectId: parsed.projectId,
        includeArchived: parsed.includeArchived,
      },
    })
  },

  getWorkflowAgentDetail(request) {
    const parsed = getWorkflowAgentDetailRequestSchema.parse(request)
    return invokeTyped(COMMANDS.getWorkflowAgentDetail, workflowAgentDetailSchema, {
      request: parsed,
    })
  },

  getWorkflowAgentGraphProjection(request) {
    const parsed = getWorkflowAgentGraphProjectionRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.getWorkflowAgentGraphProjection,
      workflowAgentGraphProjectionSchema,
      {
        request: parsed,
      },
    )
  },

  getAgentAuthoringCatalog(request) {
    const parsed = getAgentAuthoringCatalogRequestSchema.parse(request)
    return invokeTyped(COMMANDS.getAgentAuthoringCatalog, agentAuthoringCatalogSchema, {
      request: parsed,
    })
  },

  searchAgentAuthoringSkills(request) {
    const parsed = searchAgentAuthoringSkillsRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.searchAgentAuthoringSkills,
      searchAgentAuthoringSkillsResponseSchema,
      {
        request: parsed,
      },
    )
  },

  resolveAgentAuthoringSkill(request) {
    const parsed = resolveAgentAuthoringSkillRequestSchema.parse(request)
    return invokeTyped(COMMANDS.resolveAgentAuthoringSkill, agentAuthoringAttachableSkillSchema, {
      request: parsed,
    })
  },

  listAgentSessions(request) {
    const parsed = listAgentSessionsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.listAgentSessions, listAgentSessionsResponseSchema, {
      request: {
        projectId: parsed.projectId,
        includeArchived: parsed.includeArchived ?? false,
      },
    })
  },

  getAgentSession(request) {
    const parsed = getAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.getAgentSession, agentSessionSchema.nullable(), {
      request: parsed,
    })
  },

  updateAgentSession(request) {
    const parsed = updateAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.updateAgentSession, agentSessionSchema, {
      request: parsed,
    })
  },

  autoNameAgentSession(request) {
    const parsed = autoNameAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.autoNameAgentSession, agentSessionSchema, {
      request: parsed,
    })
  },

  archiveAgentSession(request) {
    const parsed = archiveAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.archiveAgentSession, agentSessionSchema, {
      request: parsed,
    })
  },

  restoreAgentSession(request) {
    const parsed = restoreAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.restoreAgentSession, agentSessionSchema, {
      request: parsed,
    })
  },

  async deleteAgentSession(request) {
    const parsed = deleteAgentSessionRequestSchema.parse(request)
    await invokeRaw(COMMANDS.deleteAgentSession, { request: parsed })
  },

  getAutonomousRun(projectId, agentSessionId) {
    const request: GetAutonomousRunRequestDto = getAutonomousRunRequestSchema.parse({
      projectId,
      agentSessionId,
    })
    return invokeTyped(COMMANDS.getAutonomousRun, autonomousRunStateSchema, {
      request,
    })
  },

  startAgentTask(projectId, agentSessionId, prompt, options) {
    const request: StartAgentTaskRequestDto = startAgentTaskRequestSchema.parse({
      projectId,
      agentSessionId,
      prompt,
      controls: options?.controls ?? null,
    })
    return invokeTyped(COMMANDS.startAgentTask, agentRunSchema, {
      request,
    })
  },

  sendAgentMessage(runId, prompt, options) {
    const request: SendAgentMessageRequestDto = sendAgentMessageRequestSchema.parse({
      runId,
      prompt,
      autoCompact: options?.autoCompact ?? null,
    })
    return invokeTyped(COMMANDS.sendAgentMessage, agentRunSchema, {
      request,
    })
  },

  cancelAgentRun(runId) {
    const request: CancelAgentRunRequestDto = cancelAgentRunRequestSchema.parse({
      runId,
    })
    return invokeTyped(COMMANDS.cancelAgentRun, agentRunSchema, {
      request,
    })
  },

  resumeAgentRun(runId, response, options) {
    const request: ResumeAgentRunRequestDto = resumeAgentRunRequestSchema.parse({
      runId,
      response,
      autoCompact: options?.autoCompact ?? null,
    })
    return invokeTyped(COMMANDS.resumeAgentRun, agentRunSchema, {
      request,
    })
  },

  getAgentRun(runId) {
    const request: GetAgentRunRequestDto = getAgentRunRequestSchema.parse({
      runId,
    })
    return invokeTyped(COMMANDS.getAgentRun, agentRunSchema, {
      request,
    })
  },

  exportAgentTrace(runId, options) {
    const request: ExportAgentTraceRequestDto = exportAgentTraceRequestSchema.parse({
      runId,
      includeSupportBundle: options?.includeSupportBundle ?? false,
    })
    return invokeTyped(COMMANDS.exportAgentTrace, agentTraceExportSchema, {
      request,
    })
  },

  listAgentRuns(projectId, agentSessionId) {
    const request = listAgentRunsRequestSchema.parse({
      projectId,
      agentSessionId,
    })
    return invokeTyped(COMMANDS.listAgentRuns, listAgentRunsResponseSchema, {
      request,
    })
  },

  getSessionTranscript(request) {
    const parsed = getSessionTranscriptRequestSchema.parse(request)
    return invokeTyped(COMMANDS.getSessionTranscript, sessionTranscriptSchema, {
      request: parsed,
    })
  },

  exportSessionTranscript(request) {
    const parsed = exportSessionTranscriptRequestSchema.parse(request)
    return invokeTyped(COMMANDS.exportSessionTranscript, sessionTranscriptExportResponseSchema, {
      request: parsed,
    })
  },

  async saveSessionTranscriptExport(request) {
    const parsed = saveSessionTranscriptExportRequestSchema.parse(request)
    await invokeRaw(COMMANDS.saveSessionTranscriptExport, { request: parsed })
  },

  searchSessionTranscripts(request) {
    const parsed = searchSessionTranscriptsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.searchSessionTranscripts, searchSessionTranscriptsResponseSchema, {
      request: parsed,
    })
  },

  getSessionContextSnapshot(request) {
    const parsed = getSessionContextSnapshotRequestSchema.parse(request)
    return invokeTyped(COMMANDS.getSessionContextSnapshot, sessionContextSnapshotSchema, {
      request: parsed,
    })
  },

  compactSessionHistory(request) {
    const parsed = compactSessionHistoryRequestSchema.parse(request)
    return invokeTyped(COMMANDS.compactSessionHistory, compactSessionHistoryResponseSchema, {
      request: parsed,
    })
  },

  branchAgentSession(request) {
    const parsed = branchAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.branchAgentSession, agentSessionBranchResponseSchema, {
      request: {
        ...parsed,
        selected: parsed.selected ?? true,
      },
    })
  },

  rewindAgentSession(request) {
    const parsed = rewindAgentSessionRequestSchema.parse(request)
    return invokeTyped(COMMANDS.rewindAgentSession, agentSessionBranchResponseSchema, {
      request: {
        ...parsed,
        selected: parsed.selected ?? true,
      },
    })
  },

  listSessionMemories(request) {
    const parsed = listSessionMemoriesRequestSchema.parse(request)
    return invokeTyped(COMMANDS.listSessionMemories, listSessionMemoriesResponseSchema, {
      request: parsed,
    })
  },

  extractSessionMemoryCandidates(request) {
    const parsed = extractSessionMemoryCandidatesRequestSchema.parse(request)
    return invokeTyped(COMMANDS.extractSessionMemoryCandidates, extractSessionMemoryCandidatesResponseSchema, {
      request: parsed,
    })
  },

  updateSessionMemory(request) {
    const parsed = updateSessionMemoryRequestSchema.parse(request)
    return invokeTyped(COMMANDS.updateSessionMemory, sessionMemoryRecordSchema, {
      request: parsed,
    })
  },

  async deleteSessionMemory(request) {
    const parsed = deleteSessionMemoryRequestSchema.parse(request)
    await invokeRaw(COMMANDS.deleteSessionMemory, { request: parsed })
  },

  getRuntimeRun(projectId, agentSessionId) {
    const request: GetRuntimeRunRequestDto = getRuntimeRunRequestSchema.parse({
      projectId,
      agentSessionId,
    })
    return invokeTyped(COMMANDS.getRuntimeRun, runtimeRunSchema.nullable(), {
      request,
    })
  },

  getRuntimeSession(projectId) {
    return invokeTyped(COMMANDS.getRuntimeSession, runtimeSessionSchema, {
      request: { projectId },
    })
  },


  listMcpServers() {
    return invokeTyped(COMMANDS.listMcpServers, mcpRegistrySchema)
  },

  upsertMcpServer(request) {
    const parsedRequest = upsertMcpServerRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertMcpServer, mcpRegistrySchema, {
      request: parsedRequest,
    })
  },

  removeMcpServer(serverId) {
    const request = removeMcpServerRequestSchema.parse({ serverId })
    return invokeTyped(COMMANDS.removeMcpServer, mcpRegistrySchema, {
      request,
    })
  },

  importMcpServers(path) {
    const request = importMcpServersRequestSchema.parse({ path })
    return invokeTyped(COMMANDS.importMcpServers, importMcpServersResponseSchema, {
      request,
    })
  },

  refreshMcpServerStatuses(options) {
    const request = refreshMcpServerStatusesRequestSchema.parse({
      serverIds: options?.serverIds ?? [],
    })

    return invokeTyped(COMMANDS.refreshMcpServerStatuses, mcpRegistrySchema, {
      request,
    })
  },

  listSkillRegistry(request = {}) {
    const parsedRequest = listSkillRegistryRequestSchema.parse({
      projectId: request.projectId ?? null,
      query: request.query ?? null,
      includeUnavailable: request.includeUnavailable ?? true,
    })
    return invokeTyped(COMMANDS.listSkillRegistry, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  reloadSkillRegistry(request = {}) {
    const parsedRequest = listSkillRegistryRequestSchema.parse({
      projectId: request.projectId ?? null,
      query: request.query ?? null,
      includeUnavailable: request.includeUnavailable ?? true,
    })
    return invokeTyped(COMMANDS.reloadSkillRegistry, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  setSkillEnabled(request) {
    const parsedRequest = setSkillEnabledRequestSchema.parse(request)
    return invokeTyped(COMMANDS.setSkillEnabled, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  removeSkill(request) {
    const parsedRequest = removeSkillRequestSchema.parse(request)
    return invokeTyped(COMMANDS.removeSkill, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  upsertSkillLocalRoot(request) {
    const parsedRequest = upsertSkillLocalRootRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertSkillLocalRoot, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  removeSkillLocalRoot(request) {
    const parsedRequest = removeSkillLocalRootRequestSchema.parse(request)
    return invokeTyped(COMMANDS.removeSkillLocalRoot, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  updateProjectSkillSource(request) {
    const parsedRequest = updateProjectSkillSourceRequestSchema.parse(request)
    return invokeTyped(COMMANDS.updateProjectSkillSource, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  updateGithubSkillSource(request) {
    const parsedRequest = updateGithubSkillSourceRequestSchema.parse(request)
    return invokeTyped(COMMANDS.updateGithubSkillSource, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  upsertPluginRoot(request) {
    const parsedRequest = upsertPluginRootRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertPluginRoot, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  removePluginRoot(request) {
    const parsedRequest = removePluginRootRequestSchema.parse(request)
    return invokeTyped(COMMANDS.removePluginRoot, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  setPluginEnabled(request) {
    const parsedRequest = setPluginEnabledRequestSchema.parse(request)
    return invokeTyped(COMMANDS.setPluginEnabled, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  removePlugin(request) {
    const parsedRequest = removePluginRequestSchema.parse(request)
    return invokeTyped(COMMANDS.removePlugin, skillRegistrySchema, {
      request: parsedRequest,
    })
  },

  getProviderModelCatalog(profileId, options) {
    const request = createProviderModelCatalogRequest(profileId, {
      forceRefresh: options?.forceRefresh ?? false,
    })
    return invokeTypedDeduped(COMMANDS.getProviderModelCatalog, providerModelCatalogSchema, {
      request,
    })
  },

  preflightProviderProfile(profileId, options) {
    const request = createPreflightProviderProfileRequest(profileId, {
      forceRefresh: options?.forceRefresh ?? false,
      modelId: options?.modelId ?? null,
      requiredFeatures: options?.requiredFeatures,
    })
    return invokeTyped(COMMANDS.preflightProviderProfile, providerPreflightSnapshotSchema, {
      request,
    })
  },

  runDoctorReport(request = {}) {
    const parsedRequest = runDoctorReportRequestSchema.parse({
      mode: request.mode ?? 'quick_local',
    })
    return invokeTyped(COMMANDS.runDoctorReport, xeroDoctorReportSchema, {
      request: parsedRequest,
    })
  },

  checkProviderProfile(profileId, options) {
    const request = checkProviderProfileRequestSchema.parse({
      profileId,
      includeNetwork: options?.includeNetwork ?? false,
      modelId: options?.modelId ?? null,
    })
    return invokeTyped(COMMANDS.checkProviderProfile, providerProfileDiagnosticsSchema, {
      request,
    })
  },


  startOpenAiLogin(options = {}) {
    const request = startOpenAiLoginRequestSchema.parse({
      originator: options.originator ?? null,
    })

    return invokeTyped(COMMANDS.startOpenAiLogin, providerAuthSessionSchema, {
      request: {
        originator: request.originator ?? null,
      },
    })
  },

  submitOpenAiCallback(flowId, options = {}) {
    const request = submitOpenAiCallbackRequestSchema.parse({
      flowId,
      manualInput: options.manualInput ?? null,
    })

    return invokeTyped(COMMANDS.submitOpenAiCallback, providerAuthSessionSchema, {
      request: {
        flowId: request.flowId,
        manualInput: request.manualInput ?? null,
      },
    })
  },

  startAutonomousRun(projectId, agentSessionId, options) {
    const request: StartAutonomousRunRequestDto = startAutonomousRunRequestSchema.parse({
      projectId,
      agentSessionId,
      initialControls: options?.initialControls ?? null,
      initialPrompt: options?.initialPrompt ?? null,
    })
    return invokeTyped(COMMANDS.startAutonomousRun, autonomousRunStateSchema, {
      request,
    })
  },

  startRuntimeRun(projectId, agentSessionId, options) {
    const request: StartRuntimeRunRequestDto = startRuntimeRunRequestSchema.parse({
      projectId,
      agentSessionId,
      initialControls: options?.initialControls ?? null,
      initialPrompt: options?.initialPrompt ?? null,
      initialAttachments: options?.initialAttachments ?? [],
    })

    return invokeTyped(COMMANDS.startRuntimeRun, runtimeRunSchema, {
      request,
    })
  },

  stageAgentAttachment(input) {
    const request = {
      projectId: input.projectId,
      runId: input.runId,
      originalName: input.originalName,
      mediaType: input.mediaType,
      bytes: Array.from(input.bytes),
    }
    return invokeTyped(COMMANDS.stageAgentAttachment, stagedAgentAttachmentSchema, {
      request,
    })
  },

  discardAgentAttachment(projectId, absolutePath) {
    return invoke<void>(COMMANDS.discardAgentAttachment, {
      request: { projectId, absolutePath },
    })
  },

  updateRuntimeRunControls(request) {
    const parsedRequest = updateRuntimeRunControlsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.updateRuntimeRunControls, runtimeRunSchema, {
      request: parsedRequest,
    })
  },

  startRuntimeSession(projectId, options) {
    const request: StartRuntimeSessionRequestDto = startRuntimeSessionRequestSchema.parse({
      projectId,
      providerProfileId: options?.providerProfileId ?? null,
    })

    return invokeTyped(COMMANDS.startRuntimeSession, runtimeSessionSchema, {
      request,
    })
  },

  cancelAutonomousRun(projectId, agentSessionId, runId) {
    const request: CancelAutonomousRunRequestDto = cancelAutonomousRunRequestSchema.parse({
      projectId,
      agentSessionId,
      runId,
    })
    return invokeTyped(COMMANDS.cancelAutonomousRun, autonomousRunStateSchema, {
      request,
    })
  },

  stopRuntimeRun(projectId, agentSessionId, runId) {
    const request: StopRuntimeRunRequestDto = stopRuntimeRunRequestSchema.parse({
      projectId,
      agentSessionId,
      runId,
    })
    return invokeTyped(COMMANDS.stopRuntimeRun, runtimeRunSchema.nullable(), {
      request,
    })
  },

  logoutRuntimeSession(projectId) {
    return invokeTyped(COMMANDS.logoutRuntimeSession, runtimeSessionSchema, {
      request: { projectId },
    })
  },

  listProviderCredentials() {
    return invokeTyped(COMMANDS.listProviderCredentials, providerCredentialsSnapshotSchema)
  },

  upsertProviderCredential(request) {
    const parsedRequest = upsertProviderCredentialRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertProviderCredential, providerCredentialsSnapshotSchema, {
      request: parsedRequest,
    })
  },

  deleteProviderCredential(providerId) {
    const request = deleteProviderCredentialRequestSchema.parse({ providerId })
    return invokeTyped(COMMANDS.deleteProviderCredential, providerCredentialsSnapshotSchema, {
      request,
    })
  },

  startOAuthLogin(request) {
    const parsed = startOAuthLoginRequestSchema.parse(request)
    return invokeTyped(COMMANDS.startOAuthLogin, providerAuthSessionSchema, {
      request: parsed,
    })
  },

  completeOAuthCallback(request) {
    const parsed = completeOAuthCallbackRequestSchema.parse(request)
    return invokeTyped(COMMANDS.completeOAuthCallback, providerAuthSessionSchema, {
      request: parsed,
    })
  },

  resolveOperatorAction(projectId, actionId, decision, options) {
    const request = resolveOperatorActionRequestSchema.parse({
      projectId,
      actionId,
      decision,
      userAnswer: options?.userAnswer ?? null,
    })

    return invokeTyped(COMMANDS.resolveOperatorAction, resolveOperatorActionResponseSchema, {
      request,
    })
  },

  resumeOperatorRun(projectId, actionId, options) {
    const request = resumeOperatorRunRequestSchema.parse({
      projectId,
      actionId,
      userAnswer: options?.userAnswer ?? null,
    })

    return invokeTyped(COMMANDS.resumeOperatorRun, resumeOperatorRunResponseSchema, {
      request,
    })
  },

  listNotificationRoutes(projectId) {
    const request = listNotificationRoutesRequestSchema.parse({ projectId })
    return invokeTyped(COMMANDS.listNotificationRoutes, listNotificationRoutesResponseSchema, {
      request,
    })
  },

  listNotificationDispatches(projectId, options) {
    const request = listNotificationDispatchesRequestSchema.parse({
      projectId,
      actionId: options?.actionId ?? null,
    })

    return invokeTyped(COMMANDS.listNotificationDispatches, listNotificationDispatchesResponseSchema, {
      request,
    })
  },

  upsertNotificationRoute(request) {
    const parsedRequest = upsertNotificationRouteRequestSchema.parse(request)
    return invokeTyped(COMMANDS.upsertNotificationRoute, upsertNotificationRouteResponseSchema, {
      request: parsedRequest,
    })
  },

  upsertNotificationRouteCredentials(request) {
    const parsedRequest = upsertNotificationRouteCredentialsRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.upsertNotificationRouteCredentials,
      upsertNotificationRouteCredentialsResponseSchema,
      {
        request: parsedRequest,
      },
    )
  },

  recordNotificationDispatchOutcome(request) {
    const parsedRequest = recordNotificationDispatchOutcomeRequestSchema.parse(request)
    return invokeTyped(
      COMMANDS.recordNotificationDispatchOutcome,
      recordNotificationDispatchOutcomeResponseSchema,
      {
        request: parsedRequest,
      },
    )
  },

  submitNotificationReply(request) {
    const parsedRequest = submitNotificationReplyRequestSchema.parse(request)
    return invokeTyped(COMMANDS.submitNotificationReply, submitNotificationReplyResponseSchema, {
      request: parsedRequest,
    })
  },

  syncNotificationAdapters(projectId) {
    const request = syncNotificationAdaptersRequestSchema.parse({ projectId })
    return invokeTyped(COMMANDS.syncNotificationAdapters, syncNotificationAdaptersResponseSchema, {
      request,
    })
  },

  getEnvironmentDiscoveryStatus() {
    return invokeTyped(COMMANDS.getEnvironmentDiscoveryStatus, environmentDiscoveryStatusSchema)
  },

  getEnvironmentProfileSummary() {
    return invokeTyped(COMMANDS.getEnvironmentProfileSummary, environmentProfileSummarySchema)
  },

  refreshEnvironmentDiscovery() {
    return invokeTyped(COMMANDS.refreshEnvironmentDiscovery, environmentDiscoveryStatusSchema)
  },

  verifyUserEnvironmentTool(request) {
    const parsedRequest = verifyUserToolRequestSchema.parse(request)
    return invokeTyped(COMMANDS.environmentVerifyUserTool, verifyUserToolResponseSchema, {
      request: parsedRequest,
    })
  },

  saveUserEnvironmentTool(request) {
    const parsedRequest = saveUserToolRequestSchema.parse(request)
    return invokeTyped(COMMANDS.environmentSaveUserTool, environmentProbeReportSchema, {
      request: parsedRequest,
    })
  },

  removeUserEnvironmentTool(id) {
    const parsedId = z.string().trim().min(1).max(32).regex(/^[a-z0-9][a-z0-9_-]*$/).parse(id)
    return invokeTyped(COMMANDS.environmentRemoveUserTool, environmentProbeReportSchema, {
      id: parsedId,
    })
  },

  resolveEnvironmentPermissionRequests(request) {
    const parsedRequest = resolveEnvironmentPermissionRequestsSchema.parse(request)
    return invokeTyped(
      COMMANDS.resolveEnvironmentPermissionRequests,
      environmentDiscoveryStatusSchema,
      { request: parsedRequest },
    )
  },

  startEnvironmentDiscovery() {
    return invokeTyped(COMMANDS.startEnvironmentDiscovery, environmentDiscoveryStatusSchema)
  },

  speechDictationStatus() {
    return invokeTyped(COMMANDS.speechDictationStatus, dictationStatusSchema)
  },

  speechDictationSettings() {
    return invokeTyped(COMMANDS.speechDictationSettings, dictationSettingsSchema)
  },

  speechDictationUpdateSettings(request) {
    const parsedRequest = upsertDictationSettingsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.speechDictationUpdateSettings, dictationSettingsSchema, {
      request: parsedRequest,
    })
  },

  speechDictationStart(request, handler, onError) {
    return createDictationSession(request, handler, onError)
  },

  speechDictationStop() {
    return invokeRaw(COMMANDS.speechDictationStop)
  },

  speechDictationCancel() {
    return invokeRaw(COMMANDS.speechDictationCancel)
  },

  browserControlSettings() {
    return invokeTyped(COMMANDS.browserControlSettings, browserControlSettingsSchema)
  },

  browserControlUpdateSettings(request) {
    const parsedRequest = upsertBrowserControlSettingsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.browserControlUpdateSettings, browserControlSettingsSchema, {
      request: parsedRequest,
    })
  },

  soulSettings() {
    return invokeTyped(COMMANDS.soulSettings, soulSettingsSchema)
  },

  soulUpdateSettings(request) {
    const parsedRequest = upsertSoulSettingsRequestSchema.parse(request)
    return invokeTyped(COMMANDS.soulUpdateSettings, soulSettingsSchema, {
      request: parsedRequest,
    })
  },

  async browserEval(js, options) {
    if (typeof js !== 'string' || js.trim().length === 0) {
      throw new XeroDesktopError({
        code: 'invalid_request',
        errorClass: 'user_fixable',
        message: 'browserEval requires a non-empty `js` string.',
      })
    }
    return invokeTyped(COMMANDS.browserEval, browserEvalResponseSchema, {
      js,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserCurrentUrl() {
    return invokeTyped(COMMANDS.browserCurrentUrl, browserCurrentUrlResponseSchema)
  },

  browserScreenshot() {
    return invokeTyped(COMMANDS.browserScreenshot, browserScreenshotResponseSchema)
  },

  async browserNavigate(url, options) {
    await invokeTyped(COMMANDS.browserNavigate, browserVoidSchema, {
      url,
      tab_id: options?.tabId ?? null,
    })
  },

  browserBack() {
    return invokeTyped(COMMANDS.browserBack, browserJsonSchema)
  },

  browserForward() {
    return invokeTyped(COMMANDS.browserForward, browserJsonSchema)
  },

  async browserReload(options) {
    await invokeTyped(COMMANDS.browserReload, browserVoidSchema, {
      tab_id: options?.tabId ?? null,
    })
  },

  browserStop() {
    return invokeTyped(COMMANDS.browserStop, browserJsonSchema)
  },

  browserClick(selector, options) {
    return invokeTyped(COMMANDS.browserClick, browserJsonSchema, {
      selector,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserType(selector, text, options) {
    return invokeTyped(COMMANDS.browserType, browserJsonSchema, {
      selector,
      text,
      append: options?.append ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserScroll(options) {
    return invokeTyped(COMMANDS.browserScroll, browserJsonSchema, {
      selector: options?.selector ?? null,
      x: options?.x ?? null,
      y: options?.y ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserPressKey(key, options) {
    return invokeTyped(COMMANDS.browserPressKey, browserJsonSchema, {
      key,
      selector: options?.selector ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserReadText(options) {
    return invokeTyped(COMMANDS.browserReadText, browserJsonSchema, {
      selector: options?.selector ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserQuery(selector, options) {
    return invokeTyped(COMMANDS.browserQuery, browserJsonSchema, {
      selector,
      limit: options?.limit ?? null,
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserWaitForSelector(selector, options) {
    return invokeTyped(COMMANDS.browserWaitForSelector, browserJsonSchema, {
      selector,
      timeout_ms: options?.timeoutMs ?? null,
      visible: options?.visible ?? null,
    })
  },

  browserWaitForLoad(options) {
    return invokeTyped(COMMANDS.browserWaitForLoad, browserJsonSchema, {
      timeout_ms: options?.timeoutMs ?? null,
    })
  },

  browserHistoryState() {
    return invokeTyped(COMMANDS.browserHistoryState, browserJsonSchema)
  },

  browserCookiesGet() {
    return invokeTyped(COMMANDS.browserCookiesGet, browserJsonSchema)
  },

  browserCookiesSet(cookie) {
    return invokeTyped(COMMANDS.browserCookiesSet, browserJsonSchema, { cookie })
  },

  browserStorageRead(area, key) {
    return invokeTyped(COMMANDS.browserStorageRead, browserJsonSchema, {
      area,
      key: key ?? null,
    })
  },

  browserStorageWrite(area, key, value) {
    return invokeTyped(COMMANDS.browserStorageWrite, browserJsonSchema, {
      area,
      key,
      value,
    })
  },

  browserStorageClear(area) {
    return invokeTyped(COMMANDS.browserStorageClear, browserJsonSchema, { area })
  },

  browserTabList() {
    return invokeTyped(COMMANDS.browserTabList, browserTabListSchema)
  },

  browserTabFocus(tabId) {
    return invokeTyped(COMMANDS.browserTabFocus, browserTabMetadataSchema, { tab_id: tabId })
  },

  browserTabClose(tabId) {
    return invokeTyped(COMMANDS.browserTabClose, browserTabListSchema, { tab_id: tabId })
  },

  onBrowserUrlChanged(handler, onError) {
    return listenTyped(EVENTS.browserUrlChanged, browserUrlChangedPayloadSchema, handler, onError)
  },

  onBrowserLoadState(handler, onError) {
    return listenTyped(EVENTS.browserLoadState, browserLoadStatePayloadSchema, handler, onError)
  },

  onBrowserConsole(handler, onError) {
    return listenTyped(EVENTS.browserConsole, browserConsolePayloadSchema, handler, onError)
  },

  onBrowserTabUpdated(handler, onError) {
    return listenTyped(EVENTS.browserTabUpdated, browserTabUpdatedPayloadSchema, handler, onError)
  },

  subscribeRuntimeStream(projectId, agentSessionId, itemKinds, handler, onError, options) {
    return createRuntimeStreamSubscription(projectId, agentSessionId, itemKinds, handler, onError, options)
  },

  subscribeAgentStream(runId, handler, onError) {
    return createAgentStreamSubscription(runId, handler, onError)
  },

  onProjectUpdated(handler, onError) {
    return listenTyped(EVENTS.projectUpdated, projectUpdatedPayloadSchema, handler, onError)
  },

  onRepositoryStatusChanged(handler, onError) {
    return listenTyped(EVENTS.repositoryStatusChanged, repositoryStatusChangedPayloadSchema, handler, onError)
  },

  onRuntimeUpdated(handler, onError) {
    return listenTyped(EVENTS.runtimeUpdated, runtimeUpdatedPayloadSchema, handler, onError)
  },

  onRuntimeRunUpdated(handler, onError) {
    return listenTyped(EVENTS.runtimeRunUpdated, runtimeRunUpdatedPayloadSchema, handler, onError)
  },

  onAgentUsageUpdated(handler, onError) {
    return listenTyped(EVENTS.agentUsageUpdated, agentUsageUpdatedPayloadSchema, handler, onError)
  },
}

export function getDesktopErrorMessage(error: unknown): string {
  return normalizeError(error, 'Xero desktop state').message
}
