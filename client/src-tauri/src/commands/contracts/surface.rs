use serde::{Deserialize, Serialize};

use super::{
    autonomous::AutonomousRunDto,
    dictation::{
        SPEECH_DICTATION_CANCEL_COMMAND, SPEECH_DICTATION_SETTINGS_COMMAND,
        SPEECH_DICTATION_START_COMMAND, SPEECH_DICTATION_STOP_COMMAND,
        SPEECH_DICTATION_UPDATE_SETTINGS_COMMAND,
    },
    runtime::AgentSessionDto,
    workflow::{
        OperatorApprovalDto, PhaseSummaryDto, ResumeHistoryEntryDto, VerificationRecordDto,
    },
};

pub const IMPORT_REPOSITORY_COMMAND: &str = "import_repository";
pub const CREATE_REPOSITORY_COMMAND: &str = "create_repository";
pub const LIST_PROJECTS_COMMAND: &str = "list_projects";
pub const REMOVE_PROJECT_COMMAND: &str = "remove_project";
pub const CREATE_AGENT_SESSION_COMMAND: &str = "create_agent_session";
pub const LIST_AGENT_SESSIONS_COMMAND: &str = "list_agent_sessions";
pub const GET_AGENT_SESSION_COMMAND: &str = "get_agent_session";
pub const UPDATE_AGENT_SESSION_COMMAND: &str = "update_agent_session";
pub const AUTO_NAME_AGENT_SESSION_COMMAND: &str = "auto_name_agent_session";
pub const ARCHIVE_AGENT_SESSION_COMMAND: &str = "archive_agent_session";
pub const RESTORE_AGENT_SESSION_COMMAND: &str = "restore_agent_session";
pub const DELETE_AGENT_SESSION_COMMAND: &str = "delete_agent_session";
pub const BRANCH_AGENT_SESSION_COMMAND: &str = "branch_agent_session";
pub const REWIND_AGENT_SESSION_COMMAND: &str = "rewind_agent_session";
pub const GET_AUTONOMOUS_RUN_COMMAND: &str = "get_autonomous_run";
pub const GET_PROJECT_SNAPSHOT_COMMAND: &str = "get_project_snapshot";
pub const GET_REPOSITORY_STATUS_COMMAND: &str = "get_repository_status";
pub const GET_REPOSITORY_DIFF_COMMAND: &str = "get_repository_diff";
pub const GIT_GENERATE_COMMIT_MESSAGE_COMMAND: &str = "git_generate_commit_message";
pub const LIST_PROJECT_FILES_COMMAND: &str = "list_project_files";
pub const READ_PROJECT_FILE_COMMAND: &str = "read_project_file";
pub const WRITE_PROJECT_FILE_COMMAND: &str = "write_project_file";
pub const CREATE_PROJECT_ENTRY_COMMAND: &str = "create_project_entry";
pub const RENAME_PROJECT_ENTRY_COMMAND: &str = "rename_project_entry";
pub const MOVE_PROJECT_ENTRY_COMMAND: &str = "move_project_entry";
pub const DELETE_PROJECT_ENTRY_COMMAND: &str = "delete_project_entry";
pub const GET_RUNTIME_RUN_COMMAND: &str = "get_runtime_run";
pub const GET_RUNTIME_SESSION_COMMAND: &str = "get_runtime_session";
pub const LIST_MCP_SERVERS_COMMAND: &str = "list_mcp_servers";
pub const UPSERT_MCP_SERVER_COMMAND: &str = "upsert_mcp_server";
pub const REMOVE_MCP_SERVER_COMMAND: &str = "remove_mcp_server";
pub const IMPORT_MCP_SERVERS_COMMAND: &str = "import_mcp_servers";
pub const REFRESH_MCP_SERVER_STATUSES_COMMAND: &str = "refresh_mcp_server_statuses";
pub const LIST_SKILL_REGISTRY_COMMAND: &str = "list_skill_registry";
pub const RELOAD_SKILL_REGISTRY_COMMAND: &str = "reload_skill_registry";
pub const SET_SKILL_ENABLED_COMMAND: &str = "set_skill_enabled";
pub const REMOVE_SKILL_COMMAND: &str = "remove_skill";
pub const UPSERT_SKILL_LOCAL_ROOT_COMMAND: &str = "upsert_skill_local_root";
pub const REMOVE_SKILL_LOCAL_ROOT_COMMAND: &str = "remove_skill_local_root";
pub const UPDATE_PROJECT_SKILL_SOURCE_COMMAND: &str = "update_project_skill_source";
pub const UPDATE_GITHUB_SKILL_SOURCE_COMMAND: &str = "update_github_skill_source";
pub const UPSERT_PLUGIN_ROOT_COMMAND: &str = "upsert_plugin_root";
pub const REMOVE_PLUGIN_ROOT_COMMAND: &str = "remove_plugin_root";
pub const SET_PLUGIN_ENABLED_COMMAND: &str = "set_plugin_enabled";
pub const REMOVE_PLUGIN_COMMAND: &str = "remove_plugin";
pub const GET_PROVIDER_MODEL_CATALOG_COMMAND: &str = "get_provider_model_catalog";
pub const RUN_DOCTOR_REPORT_COMMAND: &str = "run_doctor_report";
pub const START_AUTONOMOUS_RUN_COMMAND: &str = "start_autonomous_run";
pub const START_OPENAI_LOGIN_COMMAND: &str = "start_openai_login";
pub const SUBMIT_OPENAI_CALLBACK_COMMAND: &str = "submit_openai_callback";
pub const START_OAUTH_LOGIN_COMMAND: &str = "start_oauth_login";
pub const COMPLETE_OAUTH_CALLBACK_COMMAND: &str = "complete_oauth_callback";
pub const LIST_PROVIDER_CREDENTIALS_COMMAND: &str = "list_provider_credentials";
pub const UPSERT_PROVIDER_CREDENTIAL_COMMAND: &str = "upsert_provider_credential";
pub const DELETE_PROVIDER_CREDENTIAL_COMMAND: &str = "delete_provider_credential";
pub const LOGOUT_RUNTIME_SESSION_COMMAND: &str = "logout_runtime_session";
pub const START_RUNTIME_RUN_COMMAND: &str = "start_runtime_run";
pub const UPDATE_RUNTIME_RUN_CONTROLS_COMMAND: &str = "update_runtime_run_controls";
pub const CANCEL_AUTONOMOUS_RUN_COMMAND: &str = "cancel_autonomous_run";
pub const START_RUNTIME_SESSION_COMMAND: &str = "start_runtime_session";
pub const STOP_RUNTIME_RUN_COMMAND: &str = "stop_runtime_run";
pub const SUBSCRIBE_RUNTIME_STREAM_COMMAND: &str = "subscribe_runtime_stream";
pub const RESOLVE_OPERATOR_ACTION_COMMAND: &str = "resolve_operator_action";
pub const RESUME_OPERATOR_RUN_COMMAND: &str = "resume_operator_run";
pub const LIST_NOTIFICATION_ROUTES_COMMAND: &str = "list_notification_routes";
pub const LIST_NOTIFICATION_DISPATCHES_COMMAND: &str = "list_notification_dispatches";
pub const UPSERT_NOTIFICATION_ROUTE_COMMAND: &str = "upsert_notification_route";
pub const UPSERT_NOTIFICATION_ROUTE_CREDENTIALS_COMMAND: &str =
    "upsert_notification_route_credentials";
pub const RECORD_NOTIFICATION_DISPATCH_OUTCOME_COMMAND: &str =
    "record_notification_dispatch_outcome";
pub const SUBMIT_NOTIFICATION_REPLY_COMMAND: &str = "submit_notification_reply";
pub const SYNC_NOTIFICATION_ADAPTERS_COMMAND: &str = "sync_notification_adapters";
pub const SPEECH_DICTATION_STATUS_COMMAND: &str = "speech_dictation_status";
pub const REGISTERED_COMMAND_NAMES: &[&str] = &[
    IMPORT_REPOSITORY_COMMAND,
    CREATE_REPOSITORY_COMMAND,
    LIST_PROJECTS_COMMAND,
    REMOVE_PROJECT_COMMAND,
    CREATE_AGENT_SESSION_COMMAND,
    LIST_AGENT_SESSIONS_COMMAND,
    GET_AGENT_SESSION_COMMAND,
    UPDATE_AGENT_SESSION_COMMAND,
    AUTO_NAME_AGENT_SESSION_COMMAND,
    ARCHIVE_AGENT_SESSION_COMMAND,
    RESTORE_AGENT_SESSION_COMMAND,
    DELETE_AGENT_SESSION_COMMAND,
    BRANCH_AGENT_SESSION_COMMAND,
    REWIND_AGENT_SESSION_COMMAND,
    GET_AUTONOMOUS_RUN_COMMAND,
    GET_PROJECT_SNAPSHOT_COMMAND,
    GET_REPOSITORY_STATUS_COMMAND,
    GET_REPOSITORY_DIFF_COMMAND,
    LIST_PROJECT_FILES_COMMAND,
    READ_PROJECT_FILE_COMMAND,
    WRITE_PROJECT_FILE_COMMAND,
    CREATE_PROJECT_ENTRY_COMMAND,
    RENAME_PROJECT_ENTRY_COMMAND,
    MOVE_PROJECT_ENTRY_COMMAND,
    DELETE_PROJECT_ENTRY_COMMAND,
    GET_RUNTIME_RUN_COMMAND,
    GET_RUNTIME_SESSION_COMMAND,
    LIST_MCP_SERVERS_COMMAND,
    UPSERT_MCP_SERVER_COMMAND,
    REMOVE_MCP_SERVER_COMMAND,
    IMPORT_MCP_SERVERS_COMMAND,
    REFRESH_MCP_SERVER_STATUSES_COMMAND,
    LIST_SKILL_REGISTRY_COMMAND,
    RELOAD_SKILL_REGISTRY_COMMAND,
    SET_SKILL_ENABLED_COMMAND,
    REMOVE_SKILL_COMMAND,
    UPSERT_SKILL_LOCAL_ROOT_COMMAND,
    REMOVE_SKILL_LOCAL_ROOT_COMMAND,
    UPDATE_PROJECT_SKILL_SOURCE_COMMAND,
    UPDATE_GITHUB_SKILL_SOURCE_COMMAND,
    UPSERT_PLUGIN_ROOT_COMMAND,
    REMOVE_PLUGIN_ROOT_COMMAND,
    SET_PLUGIN_ENABLED_COMMAND,
    REMOVE_PLUGIN_COMMAND,
    GET_PROVIDER_MODEL_CATALOG_COMMAND,
    RUN_DOCTOR_REPORT_COMMAND,
    START_AUTONOMOUS_RUN_COMMAND,
    START_OPENAI_LOGIN_COMMAND,
    SUBMIT_OPENAI_CALLBACK_COMMAND,
    START_OAUTH_LOGIN_COMMAND,
    COMPLETE_OAUTH_CALLBACK_COMMAND,
    LIST_PROVIDER_CREDENTIALS_COMMAND,
    UPSERT_PROVIDER_CREDENTIAL_COMMAND,
    DELETE_PROVIDER_CREDENTIAL_COMMAND,
    LOGOUT_RUNTIME_SESSION_COMMAND,
    START_RUNTIME_RUN_COMMAND,
    UPDATE_RUNTIME_RUN_CONTROLS_COMMAND,
    CANCEL_AUTONOMOUS_RUN_COMMAND,
    START_RUNTIME_SESSION_COMMAND,
    STOP_RUNTIME_RUN_COMMAND,
    SUBSCRIBE_RUNTIME_STREAM_COMMAND,
    RESOLVE_OPERATOR_ACTION_COMMAND,
    RESUME_OPERATOR_RUN_COMMAND,
    LIST_NOTIFICATION_ROUTES_COMMAND,
    LIST_NOTIFICATION_DISPATCHES_COMMAND,
    UPSERT_NOTIFICATION_ROUTE_COMMAND,
    UPSERT_NOTIFICATION_ROUTE_CREDENTIALS_COMMAND,
    RECORD_NOTIFICATION_DISPATCH_OUTCOME_COMMAND,
    SUBMIT_NOTIFICATION_REPLY_COMMAND,
    SYNC_NOTIFICATION_ADAPTERS_COMMAND,
    SPEECH_DICTATION_STATUS_COMMAND,
    SPEECH_DICTATION_SETTINGS_COMMAND,
    SPEECH_DICTATION_UPDATE_SETTINGS_COMMAND,
    SPEECH_DICTATION_START_COMMAND,
    SPEECH_DICTATION_STOP_COMMAND,
    SPEECH_DICTATION_CANCEL_COMMAND,
];

pub const PROJECT_UPDATED_EVENT: &str = "project:updated";
pub const REPOSITORY_STATUS_CHANGED_EVENT: &str = "repository:status_changed";
pub const RUNTIME_UPDATED_EVENT: &str = "runtime:updated";
pub const RUNTIME_RUN_UPDATED_EVENT: &str = "runtime_run:updated";

pub const START_OPENAI_CODEX_AUTH_COMMAND: &str = START_OPENAI_LOGIN_COMMAND;
pub const COMPLETE_OPENAI_CODEX_AUTH_COMMAND: &str = SUBMIT_OPENAI_CALLBACK_COMMAND;
pub const CANCEL_OPENAI_CODEX_AUTH_COMMAND: &str = "cancel_openai_codex_auth";
pub const GET_RUNTIME_AUTH_STATUS_COMMAND: &str = GET_RUNTIME_SESSION_COMMAND;
pub const REFRESH_OPENAI_CODEX_AUTH_COMMAND: &str = START_RUNTIME_SESSION_COMMAND;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChange,
    Conflicted,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryDiffScope {
    Staged,
    Unstaged,
    Worktree,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectUpdateReason {
    Imported,
    Refreshed,
    MetadataChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportRepositoryRequestDto {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRepositoryRequestDto {
    pub parent_path: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectIdRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProjectFilesRequestDto {
    pub project_id: String,
    #[serde(default = "default_project_tree_path")]
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryDiffRequestDto {
    pub project_id: String,
    pub scope: RepositoryDiffScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectEntryKindDto {
    File,
    Folder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFileRendererKindDto {
    Code,
    Svg,
    Markdown,
    Csv,
    Html,
    Image,
    Pdf,
    Audio,
    Video,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectFileRequestDto {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RevokeProjectAssetTokensRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteProjectFileRequestDto {
    pub project_id: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProjectEntryRequestDto {
    pub project_id: String,
    pub parent_path: String,
    pub name: String,
    pub entry_type: ProjectEntryKindDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameProjectEntryRequestDto {
    pub project_id: String,
    pub path: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveProjectEntryRequestDto {
    pub project_id: String,
    pub path: String,
    pub target_parent_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectFileNodeDto {
    pub name: String,
    pub path: String,
    pub r#type: ProjectEntryKindDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<ProjectFileNodeDto>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub children_loaded: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub omitted_entry_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PayloadBudgetDiagnosticDto {
    pub key: String,
    pub budget_bytes: u32,
    pub observed_bytes: u32,
    pub truncated: bool,
    pub dropped: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSummaryDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub milestone: String,
    pub total_phases: u32,
    pub completed_phases: u32,
    pub active_phase: u32,
    pub branch: Option<String>,
    pub runtime: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositorySummaryDto {
    pub id: String,
    pub project_id: String,
    pub root_path: String,
    pub display_name: String,
    pub branch: Option<String>,
    pub head_sha: Option<String>,
    pub is_git_repo: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BranchUpstreamSummaryDto {
    pub name: String,
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BranchSummaryDto {
    pub name: String,
    pub head_sha: Option<String>,
    pub detached: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upstream: Option<BranchUpstreamSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryStatusEntryDto {
    pub path: String,
    pub staged: Option<ChangeKind>,
    pub unstaged: Option<ChangeKind>,
    pub untracked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LastCommitSummaryDto {
    pub sha: String,
    pub summary: String,
    pub committed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportRepositoryResponseDto {
    pub project: ProjectSummaryDto,
    pub repository: RepositorySummaryDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProjectsResponseDto {
    pub projects: Vec<ProjectSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSnapshotResponseDto {
    pub project: ProjectSummaryDto,
    pub repository: Option<RepositorySummaryDto>,
    pub phases: Vec<PhaseSummaryDto>,
    pub approval_requests: Vec<OperatorApprovalDto>,
    pub verification_records: Vec<VerificationRecordDto>,
    pub resume_history: Vec<ResumeHistoryEntryDto>,
    #[serde(default)]
    pub agent_sessions: Vec<AgentSessionDto>,
    #[serde(default)]
    pub autonomous_run: Option<AutonomousRunDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryStatusResponseDto {
    pub repository: RepositorySummaryDto,
    pub branch: Option<BranchSummaryDto>,
    #[serde(default)]
    pub last_commit: Option<LastCommitSummaryDto>,
    pub entries: Vec<RepositoryStatusEntryDto>,
    pub has_staged_changes: bool,
    pub has_unstaged_changes: bool,
    pub has_untracked_changes: bool,
    #[serde(default)]
    pub additions: u32,
    #[serde(default)]
    pub deletions: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_budget: Option<PayloadBudgetDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryDiffResponseDto {
    pub repository: RepositorySummaryDto,
    pub scope: RepositoryDiffScope,
    pub patch: String,
    pub truncated: bool,
    pub base_revision: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_budget: Option<PayloadBudgetDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitPathsRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitCommitRequestDto {
    pub project_id: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitGenerateCommitMessageRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<super::runtime::ProviderModelThinkingEffortDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitGenerateCommitMessageResponseDto {
    pub message: String,
    pub provider_id: String,
    pub model_id: String,
    pub diff_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitRemoteRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub remote: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitSignatureDto {
    pub name: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitCommitResponseDto {
    pub sha: String,
    pub summary: String,
    pub signature: GitSignatureDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitFetchResponseDto {
    pub remote: String,
    pub refspecs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitPullResponseDto {
    pub remote: String,
    pub branch: String,
    pub updated: bool,
    pub summary: String,
    #[serde(default)]
    pub new_head_sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitRemoteRefUpdateDto {
    pub ref_name: String,
    pub ok: bool,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitPushResponseDto {
    pub remote: String,
    pub branch: String,
    pub updates: Vec<GitRemoteRefUpdateDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProjectFilesResponseDto {
    pub project_id: String,
    pub path: String,
    pub root: ProjectFileNodeDto,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub omitted_entry_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_budget: Option<PayloadBudgetDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum ReadProjectFileResponseDto {
    Text {
        project_id: String,
        path: String,
        byte_length: u64,
        modified_at: String,
        content_hash: String,
        mime_type: String,
        renderer_kind: ProjectFileRendererKindDto,
        text: String,
    },
    Renderable {
        project_id: String,
        path: String,
        byte_length: u64,
        modified_at: String,
        content_hash: String,
        mime_type: String,
        renderer_kind: ProjectFileRendererKindDto,
        preview_url: String,
    },
    Unsupported {
        project_id: String,
        path: String,
        byte_length: u64,
        modified_at: String,
        content_hash: String,
        mime_type: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        renderer_kind: Option<ProjectFileRendererKindDto>,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteProjectFileResponseDto {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProjectEntryResponseDto {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RenameProjectEntryResponseDto {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MoveProjectEntryResponseDto {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteProjectEntryResponseDto {
    pub project_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchProjectRequestDto {
    pub project_id: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub include_globs: Vec<String>,
    #[serde(default)]
    pub exclude_globs: Vec<String>,
    #[serde(default)]
    pub max_results: Option<u32>,
    #[serde(default)]
    pub max_files: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchMatchDto {
    pub line: u32,
    pub column: u32,
    pub preview_prefix: String,
    pub preview_match: String,
    pub preview_suffix: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchFileResultDto {
    pub path: String,
    pub matches: Vec<SearchMatchDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchProjectResponseDto {
    pub project_id: String,
    pub total_matches: u32,
    pub total_files: u32,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_budget: Option<PayloadBudgetDiagnosticDto>,
    pub files: Vec<SearchFileResultDto>,
}

fn default_project_tree_path() -> String {
    "/".into()
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceInProjectRequestDto {
    pub project_id: String,
    pub query: String,
    pub replacement: String,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub whole_word: bool,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub include_globs: Vec<String>,
    #[serde(default)]
    pub exclude_globs: Vec<String>,
    #[serde(default)]
    pub target_paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplaceInProjectResponseDto {
    pub project_id: String,
    pub files_changed: u32,
    pub total_replacements: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectUpdatedPayloadDto {
    pub project: ProjectSummaryDto,
    pub reason: ProjectUpdateReason,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryStatusChangedPayloadDto {
    pub project_id: String,
    pub repository_id: String,
    pub status: RepositoryStatusResponseDto,
}
