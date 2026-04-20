pub mod apply_workflow_transition;
pub mod cancel_autonomous_run;
pub mod get_autonomous_run;
pub mod get_project_snapshot;
pub mod get_repository_diff;
pub mod get_repository_status;
pub mod get_runtime_run;
pub mod get_runtime_session;
pub mod get_runtime_settings;
pub mod import_repository;
pub mod list_notification_dispatches;
pub mod list_notification_routes;
pub mod list_projects;
pub mod logout_runtime_session;
pub mod record_notification_dispatch_outcome;
pub mod remove_project;
pub mod resolve_operator_action;
pub mod resume_operator_run;
pub(crate) mod runtime_support;
pub mod start_autonomous_run;
pub mod start_openai_login;
pub mod start_runtime_run;
pub mod start_runtime_session;
pub mod stop_runtime_run;
pub mod submit_notification_reply;
pub mod submit_openai_callback;
pub mod subscribe_runtime_stream;
pub mod sync_notification_adapters;
pub mod upsert_notification_route;
pub mod upsert_notification_route_credentials;
pub mod upsert_runtime_settings;
pub mod upsert_workflow_graph;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::project_store;

pub use apply_workflow_transition::apply_workflow_transition;
pub use cancel_autonomous_run::cancel_autonomous_run;
pub use get_autonomous_run::get_autonomous_run;
pub use get_project_snapshot::get_project_snapshot;
pub use get_repository_diff::get_repository_diff;
pub use get_repository_status::get_repository_status;
pub use get_runtime_run::get_runtime_run;
pub use get_runtime_session::get_runtime_session;
pub use get_runtime_settings::get_runtime_settings;
pub use import_repository::import_repository;
pub use list_notification_dispatches::list_notification_dispatches;
pub use list_notification_routes::list_notification_routes;
pub use list_projects::list_projects;
pub use logout_runtime_session::logout_runtime_session;
pub use record_notification_dispatch_outcome::record_notification_dispatch_outcome;
pub use remove_project::remove_project;
pub use resolve_operator_action::resolve_operator_action;
pub use resume_operator_run::resume_operator_run;
pub use start_autonomous_run::start_autonomous_run;
pub use start_openai_login::start_openai_login;
pub use start_runtime_run::start_runtime_run;
pub use start_runtime_session::start_runtime_session;
pub use stop_runtime_run::stop_runtime_run;
pub use submit_notification_reply::submit_notification_reply;
pub use submit_openai_callback::submit_openai_callback;
pub use subscribe_runtime_stream::subscribe_runtime_stream;
pub use sync_notification_adapters::sync_notification_adapters;
pub use upsert_notification_route::upsert_notification_route;
pub use upsert_notification_route_credentials::upsert_notification_route_credentials;
pub use upsert_runtime_settings::upsert_runtime_settings;
pub use upsert_workflow_graph::upsert_workflow_graph;

pub const IMPORT_REPOSITORY_COMMAND: &str = "import_repository";
pub const LIST_PROJECTS_COMMAND: &str = "list_projects";
pub const REMOVE_PROJECT_COMMAND: &str = "remove_project";
pub const GET_AUTONOMOUS_RUN_COMMAND: &str = "get_autonomous_run";
pub const GET_PROJECT_SNAPSHOT_COMMAND: &str = "get_project_snapshot";
pub const GET_REPOSITORY_STATUS_COMMAND: &str = "get_repository_status";
pub const GET_REPOSITORY_DIFF_COMMAND: &str = "get_repository_diff";
pub const GET_RUNTIME_RUN_COMMAND: &str = "get_runtime_run";
pub const GET_RUNTIME_SESSION_COMMAND: &str = "get_runtime_session";
pub const GET_RUNTIME_SETTINGS_COMMAND: &str = "get_runtime_settings";
pub const START_AUTONOMOUS_RUN_COMMAND: &str = "start_autonomous_run";
pub const START_OPENAI_LOGIN_COMMAND: &str = "start_openai_login";
pub const SUBMIT_OPENAI_CALLBACK_COMMAND: &str = "submit_openai_callback";
pub const LOGOUT_RUNTIME_SESSION_COMMAND: &str = "logout_runtime_session";
pub const START_RUNTIME_RUN_COMMAND: &str = "start_runtime_run";
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
pub const UPSERT_RUNTIME_SETTINGS_COMMAND: &str = "upsert_runtime_settings";
pub const UPSERT_WORKFLOW_GRAPH_COMMAND: &str = "upsert_workflow_graph";
pub const APPLY_WORKFLOW_TRANSITION_COMMAND: &str = "apply_workflow_transition";

pub const REGISTERED_COMMAND_NAMES: &[&str] = &[
    IMPORT_REPOSITORY_COMMAND,
    LIST_PROJECTS_COMMAND,
    REMOVE_PROJECT_COMMAND,
    GET_AUTONOMOUS_RUN_COMMAND,
    GET_PROJECT_SNAPSHOT_COMMAND,
    GET_REPOSITORY_STATUS_COMMAND,
    GET_REPOSITORY_DIFF_COMMAND,
    GET_RUNTIME_RUN_COMMAND,
    GET_RUNTIME_SESSION_COMMAND,
    GET_RUNTIME_SETTINGS_COMMAND,
    START_AUTONOMOUS_RUN_COMMAND,
    START_OPENAI_LOGIN_COMMAND,
    SUBMIT_OPENAI_CALLBACK_COMMAND,
    LOGOUT_RUNTIME_SESSION_COMMAND,
    START_RUNTIME_RUN_COMMAND,
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
    UPSERT_RUNTIME_SETTINGS_COMMAND,
    UPSERT_WORKFLOW_GRAPH_COMMAND,
    APPLY_WORKFLOW_TRANSITION_COMMAND,
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

pub type CommandResult<T> = Result<T, CommandError>;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    Complete,
    Active,
    Pending,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStep {
    Discuss,
    Plan,
    Execute,
    Verify,
    Ship,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowGateStateDto {
    Pending,
    Satisfied,
    Blocked,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowTransitionGateDecisionDto {
    Approved,
    Rejected,
    Blocked,
    NotApplicable,
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
#[serde(rename_all = "snake_case")]
pub enum CommandErrorClass {
    UserFixable,
    Retryable,
    SystemFault,
    PolicyDenied,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Error)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[error("{message}")]
pub struct CommandError {
    pub code: String,
    pub class: CommandErrorClass,
    pub message: String,
    pub retryable: bool,
}

impl CommandError {
    pub fn new(
        code: impl Into<String>,
        class: CommandErrorClass,
        message: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            code: code.into(),
            class,
            message: message.into(),
            retryable,
        }
    }

    pub fn invalid_request(field: &'static str) -> Self {
        Self::new(
            "invalid_request",
            CommandErrorClass::UserFixable,
            format!("Field `{field}` must be a non-empty string."),
            false,
        )
    }

    pub fn user_fixable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, CommandErrorClass::UserFixable, message, false)
    }

    pub fn policy_denied(message: impl Into<String>) -> Self {
        Self::new(
            "policy_denied",
            CommandErrorClass::PolicyDenied,
            message,
            false,
        )
    }

    pub fn retryable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, CommandErrorClass::Retryable, message, true)
    }

    pub fn system_fault(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(code, CommandErrorClass::SystemFault, message, false)
    }

    pub fn backend_not_ready(command: &'static str) -> Self {
        Self::system_fault(
            "desktop_backend_not_ready",
            format!("Command {command} is not available from the desktop backend yet."),
        )
    }

    pub fn project_not_found() -> Self {
        Self::user_fixable(
            "project_not_found",
            "Project was not found in the local desktop registry.",
        )
    }
}

pub(crate) fn validate_non_empty(value: &str, field: &'static str) -> CommandResult<()> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }

    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ImportRepositoryRequestDto {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectIdRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveOperatorActionRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub decision: String,
    pub user_answer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeOperatorRunRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub user_answer: Option<String>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationRouteKindDto {
    Telegram,
    Discord,
}

impl NotificationRouteKindDto {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationRouteCredentialReadinessStatusDto {
    Ready,
    Missing,
    Malformed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteCredentialReadinessDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteCredentialReadinessDto {
    pub has_bot_token: bool,
    pub has_chat_id: bool,
    pub has_webhook_url: bool,
    pub ready: bool,
    pub status: NotificationRouteCredentialReadinessStatusDto,
    pub diagnostic: Option<NotificationRouteCredentialReadinessDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: NotificationRouteKindDto,
    pub route_target: String,
    pub enabled: bool,
    pub metadata_json: Option<String>,
    pub credential_readiness: Option<NotificationRouteCredentialReadinessDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationRoutesRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationRoutesResponseDto {
    pub routes: Vec<NotificationRouteDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteRequestDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub route_target: String,
    pub enabled: bool,
    pub metadata_json: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteResponseDto {
    pub route: NotificationRouteDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteCredentialPayloadDto {
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteCredentialsRequestDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub credentials: NotificationRouteCredentialPayloadDto,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteCredentialsResponseDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: NotificationRouteKindDto,
    pub credential_scope: String,
    pub has_bot_token: bool,
    pub has_chat_id: bool,
    pub has_webhook_url: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationDispatchesRequestDto {
    pub project_id: String,
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationDispatchOutcomeStatusDto {
    Sent,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordNotificationDispatchOutcomeRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub status: NotificationDispatchOutcomeStatusDto,
    pub attempted_at: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitNotificationReplyRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub responder_id: Option<String>,
    pub reply_text: String,
    pub decision: String,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryDiffRequestDto {
    pub project_id: String,
    pub scope: RepositoryDiffScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGraphNodeDto {
    pub node_id: String,
    pub phase_id: u32,
    pub sort_order: u32,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    pub current_step: Option<PhaseStep>,
    pub task_count: u32,
    pub completed_tasks: u32,
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGraphEdgeDto {
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_requirement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGraphGateRequestDto {
    pub node_id: String,
    pub gate_key: String,
    pub gate_state: String,
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowGateMetadataDto {
    pub node_id: String,
    pub gate_key: String,
    pub gate_state: WorkflowGateStateDto,
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertWorkflowGraphRequestDto {
    pub project_id: String,
    pub nodes: Vec<WorkflowGraphNodeDto>,
    pub edges: Vec<WorkflowGraphEdgeDto>,
    pub gates: Vec<WorkflowGraphGateRequestDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertWorkflowGraphResponseDto {
    pub nodes: Vec<WorkflowGraphNodeDto>,
    pub edges: Vec<WorkflowGraphEdgeDto>,
    pub gates: Vec<WorkflowGateMetadataDto>,
    pub phases: Vec<PhaseSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowTransitionGateUpdateRequestDto {
    pub gate_key: String,
    pub gate_state: String,
    pub decision_context: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyWorkflowTransitionRequestDto {
    pub project_id: String,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_decision: String,
    pub gate_decision_context: Option<String>,
    pub gate_updates: Vec<WorkflowTransitionGateUpdateRequestDto>,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowTransitionEventDto {
    pub id: i64,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub gate_decision: WorkflowTransitionGateDecisionDto,
    pub gate_decision_context: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowHandoffPackageDto {
    pub id: i64,
    pub project_id: String,
    pub handoff_transition_id: String,
    pub causal_transition_id: Option<String>,
    pub from_node_id: String,
    pub to_node_id: String,
    pub transition_kind: String,
    pub package_payload: String,
    pub package_hash: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAutomaticDispatchStatusDto {
    NoContinuation,
    Applied,
    Replayed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAutomaticDispatchPackageStatusDto {
    Persisted,
    Replayed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAutomaticDispatchPackageOutcomeDto {
    pub status: WorkflowAutomaticDispatchPackageStatusDto,
    pub package: Option<WorkflowHandoffPackageDto>,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAutomaticDispatchOutcomeDto {
    pub status: WorkflowAutomaticDispatchStatusDto,
    pub transition_event: Option<WorkflowTransitionEventDto>,
    pub handoff_package: Option<WorkflowAutomaticDispatchPackageOutcomeDto>,
    pub code: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApplyWorkflowTransitionResponseDto {
    pub transition_event: WorkflowTransitionEventDto,
    pub automatic_dispatch: WorkflowAutomaticDispatchOutcomeDto,
    pub phases: Vec<PhaseSummaryDto>,
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
pub struct PhaseSummaryDto {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub status: PhaseStatus,
    pub current_step: Option<PhaseStep>,
    pub task_count: u32,
    pub completed_tasks: u32,
    pub summary: Option<String>,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanningLifecycleStageKindDto {
    Discussion,
    Research,
    Requirements,
    Roadmap,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningLifecycleStageDto {
    pub stage: PlanningLifecycleStageKindDto,
    pub node_id: String,
    pub status: PhaseStatus,
    pub action_required: bool,
    pub last_transition_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PlanningLifecycleProjectionDto {
    pub stages: Vec<PlanningLifecycleStageDto>,
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
pub struct BranchSummaryDto {
    pub name: String,
    pub head_sha: Option<String>,
    pub detached: bool,
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
#[serde(rename_all = "snake_case")]
pub enum NotificationDispatchStatusDto {
    Pending,
    Sent,
    Failed,
    Claimed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationReplyClaimStatusDto {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationDispatchDto {
    pub id: i64,
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub status: NotificationDispatchStatusDto,
    pub attempt_count: u32,
    pub last_attempt_at: Option<String>,
    pub delivered_at: Option<String>,
    pub claimed_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationReplyClaimDto {
    pub id: i64,
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub responder_id: Option<String>,
    pub status: NotificationReplyClaimStatusDto,
    pub rejection_code: Option<String>,
    pub rejection_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationDispatchesResponseDto {
    pub dispatches: Vec<NotificationDispatchDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordNotificationDispatchOutcomeResponseDto {
    pub dispatch: NotificationDispatchDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitNotificationReplyResponseDto {
    pub claim: NotificationReplyClaimDto,
    pub dispatch: NotificationDispatchDto,
    pub resolve_result: ResolveOperatorActionResponseDto,
    pub resume_result: Option<ResumeOperatorRunResponseDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncNotificationAdaptersRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAdapterErrorCountDto {
    pub code: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAdapterDispatchAttemptDto {
    pub dispatch_id: i64,
    pub action_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub outcome_status: NotificationDispatchStatusDto,
    pub diagnostic_code: String,
    pub diagnostic_message: String,
    pub durable_error_code: Option<String>,
    pub durable_error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationDispatchCycleSummaryDto {
    pub project_id: String,
    pub pending_count: u32,
    pub attempted_count: u32,
    pub sent_count: u32,
    pub failed_count: u32,
    pub attempt_limit: u32,
    pub attempts_truncated: bool,
    pub attempts: Vec<NotificationAdapterDispatchAttemptDto>,
    pub error_code_counts: Vec<NotificationAdapterErrorCountDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAdapterReplyAttemptDto {
    pub route_id: String,
    pub route_kind: String,
    pub action_id: Option<String>,
    pub message_id: Option<String>,
    pub accepted: bool,
    pub diagnostic_code: String,
    pub diagnostic_message: String,
    pub reply_code: Option<String>,
    pub reply_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationReplyCycleSummaryDto {
    pub project_id: String,
    pub route_count: u32,
    pub polled_route_count: u32,
    pub message_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub attempt_limit: u32,
    pub attempts_truncated: bool,
    pub attempts: Vec<NotificationAdapterReplyAttemptDto>,
    pub error_code_counts: Vec<NotificationAdapterErrorCountDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncNotificationAdaptersResponseDto {
    pub project_id: String,
    pub dispatch: NotificationDispatchCycleSummaryDto,
    pub replies: NotificationReplyCycleSummaryDto,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OperatorApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VerificationRecordStatus {
    Pending,
    Passed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResumeHistoryStatus {
    Started,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperatorApprovalDto {
    pub action_id: String,
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub action_type: String,
    pub title: String,
    pub detail: String,
    pub gate_node_id: Option<String>,
    pub gate_key: Option<String>,
    pub transition_from_node_id: Option<String>,
    pub transition_to_node_id: Option<String>,
    pub transition_kind: Option<String>,
    pub user_answer: Option<String>,
    pub status: OperatorApprovalStatus,
    pub decision_note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub resolved_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerificationRecordDto {
    pub id: u32,
    pub source_action_id: Option<String>,
    pub status: VerificationRecordStatus,
    pub summary: String,
    pub detail: Option<String>,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeHistoryEntryDto {
    pub id: u32,
    pub source_action_id: Option<String>,
    pub session_id: Option<String>,
    pub status: ResumeHistoryStatus,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveOperatorActionResponseDto {
    pub approval_request: OperatorApprovalDto,
    pub verification_record: VerificationRecordDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeOperatorRunResponseDto {
    pub approval_request: OperatorApprovalDto,
    pub resume_entry: ResumeHistoryEntryDto,
    pub automatic_dispatch: Option<WorkflowAutomaticDispatchOutcomeDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectSnapshotResponseDto {
    pub project: ProjectSummaryDto,
    pub repository: Option<RepositorySummaryDto>,
    pub phases: Vec<PhaseSummaryDto>,
    pub lifecycle: PlanningLifecycleProjectionDto,
    pub approval_requests: Vec<OperatorApprovalDto>,
    pub verification_records: Vec<VerificationRecordDto>,
    pub resume_history: Vec<ResumeHistoryEntryDto>,
    #[serde(default)]
    pub handoff_packages: Vec<WorkflowHandoffPackageDto>,
    #[serde(default)]
    pub autonomous_run: Option<AutonomousRunDto>,
    #[serde(default)]
    pub autonomous_unit: Option<AutonomousUnitDto>,
}

pub(crate) fn map_workflow_transition_event_record(
    event: project_store::WorkflowTransitionEventRecord,
) -> WorkflowTransitionEventDto {
    WorkflowTransitionEventDto {
        id: event.id,
        transition_id: event.transition_id,
        causal_transition_id: event.causal_transition_id,
        from_node_id: event.from_node_id,
        to_node_id: event.to_node_id,
        transition_kind: event.transition_kind,
        gate_decision: map_transition_gate_decision(event.gate_decision),
        gate_decision_context: event.gate_decision_context,
        created_at: event.created_at,
    }
}

pub(crate) fn map_workflow_handoff_package_record(
    package: project_store::WorkflowHandoffPackageRecord,
) -> WorkflowHandoffPackageDto {
    WorkflowHandoffPackageDto {
        id: package.id,
        project_id: package.project_id,
        handoff_transition_id: package.handoff_transition_id,
        causal_transition_id: package.causal_transition_id,
        from_node_id: package.from_node_id,
        to_node_id: package.to_node_id,
        transition_kind: package.transition_kind,
        package_payload: package.package_payload,
        package_hash: package.package_hash,
        created_at: package.created_at,
    }
}

pub(crate) fn map_workflow_automatic_dispatch_outcome(
    outcome: project_store::WorkflowAutomaticDispatchOutcome,
) -> WorkflowAutomaticDispatchOutcomeDto {
    match outcome {
        project_store::WorkflowAutomaticDispatchOutcome::NoContinuation => {
            WorkflowAutomaticDispatchOutcomeDto {
                status: WorkflowAutomaticDispatchStatusDto::NoContinuation,
                transition_event: None,
                handoff_package: None,
                code: None,
                message: None,
            }
        }
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            transition_event,
            handoff_package,
        } => WorkflowAutomaticDispatchOutcomeDto {
            status: WorkflowAutomaticDispatchStatusDto::Applied,
            transition_event: Some(map_workflow_transition_event_record(transition_event)),
            handoff_package: Some(map_workflow_automatic_dispatch_package_outcome(
                handoff_package,
            )),
            code: None,
            message: None,
        },
        project_store::WorkflowAutomaticDispatchOutcome::Replayed {
            transition_event,
            handoff_package,
        } => WorkflowAutomaticDispatchOutcomeDto {
            status: WorkflowAutomaticDispatchStatusDto::Replayed,
            transition_event: Some(map_workflow_transition_event_record(transition_event)),
            handoff_package: Some(map_workflow_automatic_dispatch_package_outcome(
                handoff_package,
            )),
            code: None,
            message: None,
        },
        project_store::WorkflowAutomaticDispatchOutcome::Skipped { code, message } => {
            WorkflowAutomaticDispatchOutcomeDto {
                status: WorkflowAutomaticDispatchStatusDto::Skipped,
                transition_event: None,
                handoff_package: None,
                code: Some(code),
                message: Some(message),
            }
        }
    }
}

pub(crate) fn map_notification_route_record(
    route: project_store::NotificationRouteRecord,
    credential_readiness: Option<NotificationRouteCredentialReadinessDto>,
) -> CommandResult<NotificationRouteDto> {
    Ok(NotificationRouteDto {
        project_id: route.project_id,
        route_id: route.route_id,
        route_kind: parse_notification_route_kind(
            &route.route_kind,
            "notification_route_decode_failed",
        )?,
        route_target: route.route_target,
        enabled: route.enabled,
        metadata_json: route.metadata_json,
        credential_readiness,
        created_at: route.created_at,
        updated_at: route.updated_at,
    })
}

pub(crate) fn map_notification_dispatch_record(
    dispatch: project_store::NotificationDispatchRecord,
) -> NotificationDispatchDto {
    NotificationDispatchDto {
        id: dispatch.id,
        project_id: dispatch.project_id,
        action_id: dispatch.action_id,
        route_id: dispatch.route_id,
        correlation_key: dispatch.correlation_key,
        status: map_notification_dispatch_status(dispatch.status),
        attempt_count: dispatch.attempt_count,
        last_attempt_at: dispatch.last_attempt_at,
        delivered_at: dispatch.delivered_at,
        claimed_at: dispatch.claimed_at,
        last_error_code: dispatch.last_error_code,
        last_error_message: dispatch.last_error_message,
        created_at: dispatch.created_at,
        updated_at: dispatch.updated_at,
    }
}

pub(crate) fn map_notification_reply_claim_record(
    claim: project_store::NotificationReplyClaimRecord,
) -> NotificationReplyClaimDto {
    NotificationReplyClaimDto {
        id: claim.id,
        project_id: claim.project_id,
        action_id: claim.action_id,
        route_id: claim.route_id,
        correlation_key: claim.correlation_key,
        responder_id: claim.responder_id,
        status: map_notification_reply_claim_status(claim.status),
        rejection_code: claim.rejection_code,
        rejection_message: claim.rejection_message,
        created_at: claim.created_at,
    }
}

pub(crate) fn parse_notification_route_kind(
    value: &str,
    code: &'static str,
) -> CommandResult<NotificationRouteKindDto> {
    match value.trim() {
        "telegram" => Ok(NotificationRouteKindDto::Telegram),
        "discord" => Ok(NotificationRouteKindDto::Discord),
        other => Err(CommandError::user_fixable(
            code,
            format!(
                "Cadence does not support notification route kind `{other}`. Allowed kinds: telegram, discord."
            ),
        )),
    }
}

pub(crate) fn map_notification_route_credential_readiness(
    projection: crate::notifications::NotificationCredentialReadinessProjection,
) -> NotificationRouteCredentialReadinessDto {
    NotificationRouteCredentialReadinessDto {
        has_bot_token: projection.has_bot_token,
        has_chat_id: projection.has_chat_id,
        has_webhook_url: projection.has_webhook_url,
        ready: projection.ready,
        status: map_notification_route_credential_readiness_status(projection.status),
        diagnostic: projection.diagnostic.map(|diagnostic| {
            NotificationRouteCredentialReadinessDiagnosticDto {
                code: diagnostic.code,
                message: diagnostic.message,
                retryable: diagnostic.retryable,
            }
        }),
    }
}

fn map_notification_route_credential_readiness_status(
    status: crate::notifications::NotificationCredentialReadinessStatus,
) -> NotificationRouteCredentialReadinessStatusDto {
    match status {
        crate::notifications::NotificationCredentialReadinessStatus::Ready => {
            NotificationRouteCredentialReadinessStatusDto::Ready
        }
        crate::notifications::NotificationCredentialReadinessStatus::Missing => {
            NotificationRouteCredentialReadinessStatusDto::Missing
        }
        crate::notifications::NotificationCredentialReadinessStatus::Malformed => {
            NotificationRouteCredentialReadinessStatusDto::Malformed
        }
        crate::notifications::NotificationCredentialReadinessStatus::Unavailable => {
            NotificationRouteCredentialReadinessStatusDto::Unavailable
        }
    }
}

fn map_notification_dispatch_status(
    status: project_store::NotificationDispatchStatus,
) -> NotificationDispatchStatusDto {
    match status {
        project_store::NotificationDispatchStatus::Pending => {
            NotificationDispatchStatusDto::Pending
        }
        project_store::NotificationDispatchStatus::Sent => NotificationDispatchStatusDto::Sent,
        project_store::NotificationDispatchStatus::Failed => NotificationDispatchStatusDto::Failed,
        project_store::NotificationDispatchStatus::Claimed => {
            NotificationDispatchStatusDto::Claimed
        }
    }
}

fn map_notification_reply_claim_status(
    status: project_store::NotificationReplyClaimStatus,
) -> NotificationReplyClaimStatusDto {
    match status {
        project_store::NotificationReplyClaimStatus::Accepted => {
            NotificationReplyClaimStatusDto::Accepted
        }
        project_store::NotificationReplyClaimStatus::Rejected => {
            NotificationReplyClaimStatusDto::Rejected
        }
    }
}

fn map_workflow_automatic_dispatch_package_outcome(
    outcome: project_store::WorkflowAutomaticDispatchPackageOutcome,
) -> WorkflowAutomaticDispatchPackageOutcomeDto {
    match outcome {
        project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { package } => {
            WorkflowAutomaticDispatchPackageOutcomeDto {
                status: WorkflowAutomaticDispatchPackageStatusDto::Persisted,
                package: Some(map_workflow_handoff_package_record(package)),
                code: None,
                message: None,
            }
        }
        project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { package } => {
            WorkflowAutomaticDispatchPackageOutcomeDto {
                status: WorkflowAutomaticDispatchPackageStatusDto::Replayed,
                package: Some(map_workflow_handoff_package_record(package)),
                code: None,
                message: None,
            }
        }
        project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped { code, message } => {
            WorkflowAutomaticDispatchPackageOutcomeDto {
                status: WorkflowAutomaticDispatchPackageStatusDto::Skipped,
                package: None,
                code: Some(code),
                message: Some(message),
            }
        }
    }
}

fn map_transition_gate_decision(
    value: project_store::WorkflowTransitionGateDecision,
) -> WorkflowTransitionGateDecisionDto {
    match value {
        project_store::WorkflowTransitionGateDecision::Approved => {
            WorkflowTransitionGateDecisionDto::Approved
        }
        project_store::WorkflowTransitionGateDecision::Rejected => {
            WorkflowTransitionGateDecisionDto::Rejected
        }
        project_store::WorkflowTransitionGateDecision::Blocked => {
            WorkflowTransitionGateDecisionDto::Blocked
        }
        project_store::WorkflowTransitionGateDecision::NotApplicable => {
            WorkflowTransitionGateDecisionDto::NotApplicable
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryStatusResponseDto {
    pub repository: RepositorySummaryDto,
    pub branch: Option<BranchSummaryDto>,
    pub entries: Vec<RepositoryStatusEntryDto>,
    pub has_staged_changes: bool,
    pub has_unstaged_changes: bool,
    pub has_untracked_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryDiffResponseDto {
    pub repository: RepositorySummaryDto,
    pub scope: RepositoryDiffScope,
    pub patch: String,
    pub truncated: bool,
    pub base_revision: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAuthPhase {
    Idle,
    Starting,
    AwaitingBrowserCallback,
    AwaitingManualInput,
    ExchangingCode,
    Authenticated,
    Refreshing,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRunStatusDto {
    Starting,
    Running,
    Stale,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRunTransportLivenessDto {
    Unknown,
    Reachable,
    Unreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRunCheckpointKindDto {
    Bootstrap,
    State,
    Tool,
    ActionRequired,
    Diagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunDiagnosticDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunTransportDto {
    pub kind: String,
    pub endpoint: String,
    pub liveness: RuntimeRunTransportLivenessDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunCheckpointDto {
    pub sequence: u32,
    pub kind: RuntimeRunCheckpointKindDto,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunDto {
    pub project_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub supervisor_kind: String,
    pub status: RuntimeRunStatusDto,
    pub transport: RuntimeRunTransportDto,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub last_checkpoint_sequence: u32,
    pub last_checkpoint_at: Option<String>,
    pub stopped_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
    pub updated_at: String,
    pub checkpoints: Vec<RuntimeRunCheckpointDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSessionDto {
    pub project_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub flow_id: Option<String>,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub phase: RuntimeAuthPhase,
    pub callback_bound: Option<bool>,
    pub authorization_url: Option<String>,
    pub redirect_uri: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeDiagnosticDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSettingsDto {
    pub provider_id: String,
    pub model_id: String,
    pub openrouter_api_key_configured: bool,
}

pub type RuntimeAuthStatusDto = RuntimeSessionDto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeUpdatedPayloadDto {
    pub project_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub flow_id: Option<String>,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub auth_phase: RuntimeAuthPhase,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeDiagnosticDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunUpdatedPayloadDto {
    pub project_id: String,
    pub run: Option<RuntimeRunDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousRunStatusDto {
    Starting,
    Running,
    Paused,
    Cancelling,
    Cancelled,
    Stale,
    Failed,
    Stopped,
    Crashed,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousRunRecoveryStateDto {
    Healthy,
    RecoveryRequired,
    Terminal,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousUnitKindDto {
    Researcher,
    Planner,
    Executor,
    Verifier,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousUnitStatusDto {
    Pending,
    Active,
    Blocked,
    Paused,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousUnitArtifactStatusDto {
    Pending,
    Recorded,
    Rejected,
    Redacted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousToolCallStateDto {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousVerificationOutcomeDto {
    Passed,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandResultDto {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub summary: String,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GitToolResultScopeDto {
    Staged,
    Unstaged,
    Worktree,
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WebToolResultContentKindDto {
    Html,
    PlainText,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CommandToolResultSummaryDto {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileToolResultSummaryDto {
    pub path: Option<String>,
    pub scope: Option<String>,
    pub line_count: Option<usize>,
    pub match_count: Option<usize>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GitToolResultSummaryDto {
    pub scope: Option<GitToolResultScopeDto>,
    pub changed_files: usize,
    pub truncated: bool,
    pub base_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WebToolResultSummaryDto {
    pub target: String,
    pub result_count: Option<usize>,
    pub final_url: Option<String>,
    pub content_kind: Option<WebToolResultContentKindDto>,
    pub content_type: Option<String>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum ToolResultSummaryDto {
    Command(CommandToolResultSummaryDto),
    File(FileToolResultSummaryDto),
    Git(GitToolResultSummaryDto),
    Web(WebToolResultSummaryDto),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolResultPayloadDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub tool_state: AutonomousToolCallStateDto,
    pub command_result: Option<AutonomousCommandResultDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummaryDto>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousVerificationEvidencePayloadDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub evidence_kind: String,
    pub label: String,
    pub outcome: AutonomousVerificationOutcomeDto,
    pub command_result: Option<AutonomousCommandResultDto>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPolicyDeniedPayloadDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub diagnostic_code: String,
    pub message: String,
    pub tool_name: Option<String>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutonomousArtifactPayloadDto {
    ToolResult(AutonomousToolResultPayloadDto),
    VerificationEvidence(AutonomousVerificationEvidencePayloadDto),
    PolicyDenied(AutonomousPolicyDeniedPayloadDto),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousLifecycleReasonDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRunDto {
    pub project_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub supervisor_kind: String,
    pub status: AutonomousRunStatusDto,
    pub recovery_state: AutonomousRunRecoveryStateDto,
    pub active_unit_id: Option<String>,
    pub active_attempt_id: Option<String>,
    pub duplicate_start_detected: bool,
    pub duplicate_start_run_id: Option<String>,
    pub duplicate_start_reason: Option<String>,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub last_checkpoint_at: Option<String>,
    pub paused_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub completed_at: Option<String>,
    pub crashed_at: Option<String>,
    pub stopped_at: Option<String>,
    pub pause_reason: Option<AutonomousLifecycleReasonDto>,
    pub cancel_reason: Option<AutonomousLifecycleReasonDto>,
    pub crash_reason: Option<AutonomousLifecycleReasonDto>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWorkflowLinkageDto {
    pub workflow_node_id: String,
    pub transition_id: String,
    pub causal_transition_id: Option<String>,
    pub handoff_transition_id: String,
    pub handoff_package_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub sequence: u32,
    pub kind: AutonomousUnitKindDto,
    pub status: AutonomousUnitStatusDto,
    pub summary: String,
    pub boundary_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_linkage: Option<AutonomousWorkflowLinkageDto>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitAttemptDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub attempt_number: u32,
    pub child_session_id: String,
    pub status: AutonomousUnitStatusDto,
    pub boundary_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_linkage: Option<AutonomousWorkflowLinkageDto>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub updated_at: String,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitArtifactDto {
    pub project_id: String,
    pub run_id: String,
    pub unit_id: String,
    pub attempt_id: String,
    pub artifact_id: String,
    pub artifact_kind: String,
    pub status: AutonomousUnitArtifactStatusDto,
    pub summary: String,
    pub content_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<AutonomousArtifactPayloadDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousUnitHistoryEntryDto {
    pub unit: AutonomousUnitDto,
    pub latest_attempt: Option<AutonomousUnitAttemptDto>,
    #[serde(default)]
    pub artifacts: Vec<AutonomousUnitArtifactDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRunStateDto {
    pub run: Option<AutonomousRunDto>,
    pub unit: Option<AutonomousUnitDto>,
    pub attempt: Option<AutonomousUnitAttemptDto>,
    #[serde(default)]
    pub history: Vec<AutonomousUnitHistoryEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAutonomousRunRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartAutonomousRunRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelAutonomousRunRequestDto {
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartOpenAiLoginRequestDto {
    pub project_id: String,
    pub originator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitOpenAiCallbackRequestDto {
    pub project_id: String,
    pub flow_id: String,
    pub manual_input: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetRuntimeRunRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertRuntimeSettingsRequestDto {
    pub provider_id: String,
    pub model_id: String,
    #[serde(default)]
    pub openrouter_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRuntimeRunRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StopRuntimeRunRequestDto {
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStreamItemKind {
    Transcript,
    Tool,
    Activity,
    ActionRequired,
    Complete,
    Failure,
}

impl RuntimeStreamItemKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::Tool => "tool",
            Self::Activity => "activity",
            Self::ActionRequired => "action_required",
            Self::Complete => "complete",
            Self::Failure => "failure",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeToolCallState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeStreamItemDto {
    pub kind: RuntimeStreamItemKind,
    pub run_id: String,
    pub sequence: u64,
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub text: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_state: Option<RuntimeToolCallState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummaryDto>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub retryable: Option<bool>,
    pub created_at: String,
}

impl RuntimeStreamItemDto {
    pub const ALLOWED_KIND_NAMES: [&'static str; 6] = [
        "transcript",
        "tool",
        "activity",
        "action_required",
        "complete",
        "failure",
    ];

    pub fn allowed_kind_names() -> &'static [&'static str] {
        &Self::ALLOWED_KIND_NAMES
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscribeRuntimeStreamRequestDto {
    pub project_id: String,
    pub channel: Option<String>,
    pub item_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscribeRuntimeStreamResponseDto {
    pub project_id: String,
    pub runtime_kind: String,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub subscribed_item_kinds: Vec<RuntimeStreamItemKind>,
}
