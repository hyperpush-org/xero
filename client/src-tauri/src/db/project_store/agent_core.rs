use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    commands::{CommandError, RuntimeAgentIdDto},
    db::database_path_for_repo,
};

use super::{open_runtime_database, resolve_agent_definition_for_run};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunStatus {
    Starting,
    Running,
    Paused,
    Cancelling,
    Cancelled,
    HandedOff,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunEventKind {
    RunStarted,
    AssistantCandidate,
    MessageDelta,
    ReasoningSummary,
    ToolStarted,
    ToolDelta,
    ToolCompleted,
    FileChanged,
    CommandOutput,
    ValidationStarted,
    ValidationCompleted,
    ToolRegistrySnapshot,
    PolicyDecision,
    StateTransition,
    PlanUpdated,
    RouteRequested,
    VerificationGate,
    ContextManifestRecorded,
    RetrievalPerformed,
    MemoryCandidateCaptured,
    EnvironmentLifecycleUpdate,
    SandboxLifecycleUpdate,
    ActionRequired,
    ApprovalRequired,
    ToolPermissionGrant,
    ProviderModelChanged,
    RuntimeSettingsChanged,
    RunPaused,
    RunCompleted,
    RunFailed,
    SubagentLifecycle,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentToolCallState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentRunDiagnosticRecord {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunRecord {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub lineage_kind: String,
    pub parent_run_id: Option<String>,
    pub parent_trace_id: Option<String>,
    pub parent_subagent_id: Option<String>,
    pub subagent_role: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub status: AgentRunStatus,
    pub prompt: String,
    pub system_prompt: String,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub last_error: Option<AgentRunDiagnosticRecord>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunDriveLeaseRecord {
    pub project_id: String,
    pub run_id: String,
    pub owner_instance_id: String,
    pub owner_process_id: u32,
    pub owner_process_birth_identity: String,
    pub drive_token: String,
    pub acquired_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunDriveLeaseClaimResult {
    Acquired,
    Held(AgentRunDriveLeaseRecord),
    RunNotDrivable(AgentRunStatus),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRunStartRequestState {
    Preparing,
    Ready,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunStartRequestRecord {
    pub project_id: String,
    pub run_id: String,
    pub payload_hash: String,
    pub recovery_payload_json: String,
    pub state: AgentRunStartRequestState,
    pub owner_process_id: u32,
    pub owner_process_birth_identity: String,
    pub created_at: String,
    pub ready_at: Option<String>,
    pub failed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunStartRegistrationResult {
    Registered(AgentRunSnapshotRecord),
    Replayed {
        snapshot: AgentRunSnapshotRecord,
        request: AgentRunStartRequestRecord,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRunCancellationCasResult {
    Applied {
        snapshot: AgentRunSnapshotRecord,
        transitioned: bool,
        event: Option<AgentEventRecord>,
    },
    LeaseChanged(Option<AgentRunDriveLeaseRecord>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActionRejectionCommitResult {
    pub snapshot: AgentRunSnapshotRecord,
    pub action: AgentActionRequestRecord,
    pub inserted_events: Vec<AgentEventRecord>,
    pub replayed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentContinuationRequestState {
    Prepared,
    Driving,
    Consumed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContinuationDriveStartResult {
    Started(AgentContinuationRequestRecord),
    AlreadyDriving(AgentContinuationRequestRecord),
    Consumed(AgentContinuationRequestRecord),
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContinuationRequestRecord {
    pub project_id: String,
    pub request_id: String,
    pub run_id: String,
    pub payload_hash: String,
    pub recovery_payload_json: String,
    pub state: AgentContinuationRequestState,
    pub message_id: i64,
    pub linked_path_grant_event_id: Option<i64>,
    pub message_event_id: i64,
    pub prepared_at: String,
    pub drive_started_at: Option<String>,
    pub consumed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentContinuationPreparationRecord {
    pub project_id: String,
    pub request_id: String,
    pub run_id: String,
    pub payload_hash: String,
    pub recovery_payload_json: String,
    pub role: AgentMessageRole,
    pub content: String,
    pub attachments: Vec<NewMessageAttachmentInput>,
    pub linked_path_grant_payload_json: Option<String>,
    pub message_payload_json: String,
    pub action_answer: Option<AgentContinuationActionAnswerRecord>,
    pub prepared_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContinuationActionAnswerRecord {
    pub action_id: Option<String>,
    pub response: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingAgentContinuationPreparationRecord {
    pub project_id: String,
    pub request_id: String,
    pub run_id: String,
    pub payload_hash: String,
    pub recovery_payload_json: String,
    pub message_id: i64,
    pub message_event_id: i64,
    pub prepared_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContinuationPreparationResult {
    pub snapshot: AgentRunSnapshotRecord,
    pub request: AgentContinuationRequestRecord,
    pub inserted_events: Vec<AgentEventRecord>,
    pub inserted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessageRecord {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub role: AgentMessageRole,
    pub content: String,
    pub provider_metadata_json: Option<String>,
    pub created_at: String,
    pub attachments: Vec<AgentMessageAttachmentRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageAttachmentKind {
    Image,
    Document,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMessageAttachmentRecord {
    pub id: i64,
    pub message_id: i64,
    pub project_id: String,
    pub run_id: String,
    pub kind: AgentMessageAttachmentKind,
    pub storage_path: String,
    pub media_type: String,
    pub original_name: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentMessageAttachmentRecord {
    pub message_id: i64,
    pub project_id: String,
    pub run_id: String,
    pub kind: AgentMessageAttachmentKind,
    pub storage_path: String,
    pub media_type: String,
    pub original_name: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEventRecord {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub event_kind: AgentRunEventKind,
    pub payload_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentToolCallRecord {
    pub project_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input_json: String,
    pub state: AgentToolCallState,
    pub result_json: Option<String>,
    pub error: Option<AgentRunDiagnosticRecord>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentFileChangeRecord {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub top_level_run_id: String,
    pub subagent_id: Option<String>,
    pub subagent_role: Option<String>,
    pub change_group_id: Option<String>,
    pub path: String,
    pub operation: String,
    pub old_hash: Option<String>,
    pub new_hash: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCheckpointRecord {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub checkpoint_kind: String,
    pub summary: String,
    pub payload_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentActionRequestRecord {
    pub project_id: String,
    pub run_id: String,
    pub action_id: String,
    pub action_type: String,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnvironmentLifecycleSnapshotRecord {
    pub project_id: String,
    pub run_id: String,
    pub environment_id: String,
    pub state: String,
    pub previous_state: Option<String>,
    pub pending_message_count: i64,
    pub health_checks_json: String,
    pub setup_steps_json: String,
    pub diagnostic_json: Option<String>,
    pub snapshot_json: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentEnvironmentLifecycleSnapshotRecord {
    pub project_id: String,
    pub run_id: String,
    pub environment_id: String,
    pub state: String,
    pub previous_state: Option<String>,
    pub pending_message_count: i64,
    pub health_checks_json: String,
    pub setup_steps_json: String,
    pub diagnostic_json: Option<String>,
    pub snapshot_json: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEnvironmentPendingMessageRecord {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub role: AgentMessageRole,
    pub content: String,
    pub submitted_at: String,
    pub delivered_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentUsageRecord {
    pub project_id: String,
    pub run_id: String,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: u64,
    pub billable_input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub estimated_cost_micros: u64,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSubagentTaskRecord {
    pub project_id: String,
    pub parent_run_id: String,
    pub subagent_id: String,
    pub role: String,
    pub role_label: String,
    pub prompt_hash: String,
    pub prompt_preview: String,
    pub model_id: Option<String>,
    pub write_set_json: String,
    pub workflow_structure_json: Option<String>,
    pub verification_contract: String,
    pub depth: u64,
    pub max_tool_calls: u64,
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    pub used_tool_calls: u64,
    pub used_tokens: u64,
    pub used_cost_micros: u64,
    pub budget_status: String,
    pub budget_diagnostic_json: Option<String>,
    pub status: String,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub integrated_at: Option<String>,
    pub child_run_id: Option<String>,
    pub child_trace_id: Option<String>,
    pub parent_trace_id: Option<String>,
    pub input_log_json: String,
    pub result_summary: Option<String>,
    pub result_artifact: Option<String>,
    pub parent_decision: Option<String>,
    pub latest_summary: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunSnapshotRecord {
    pub run: AgentRunRecord,
    pub messages: Vec<AgentMessageRecord>,
    pub events: Vec<AgentEventRecord>,
    pub tool_calls: Vec<AgentToolCallRecord>,
    pub file_changes: Vec<AgentFileChangeRecord>,
    pub checkpoints: Vec<AgentCheckpointRecord>,
    pub action_requests: Vec<AgentActionRequestRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentRunRecord {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: Option<String>,
    pub agent_definition_version: Option<u32>,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub prompt: String,
    pub system_prompt: String,
    pub now: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunLineageUpdateRecord {
    pub project_id: String,
    pub run_id: String,
    pub parent_run_id: String,
    pub parent_trace_id: String,
    pub parent_subagent_id: String,
    pub subagent_role: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentMessageRecord {
    pub project_id: String,
    pub run_id: String,
    pub role: AgentMessageRole,
    pub content: String,
    pub provider_metadata_json: Option<String>,
    pub created_at: String,
    pub attachments: Vec<NewMessageAttachmentInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMessageAttachmentInput {
    pub kind: AgentMessageAttachmentKind,
    pub storage_path: String,
    pub media_type: String,
    pub original_name: String,
    pub size_bytes: i64,
    pub width: Option<i64>,
    pub height: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentEventRecord {
    pub project_id: String,
    pub run_id: String,
    pub event_kind: AgentRunEventKind,
    pub payload_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentToolCallStartRecord {
    pub project_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input_json: String,
    pub started_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentToolCallFinishRecord {
    pub project_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub state: AgentToolCallState,
    pub result_json: Option<String>,
    pub error: Option<AgentRunDiagnosticRecord>,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentFileChangeRecord {
    pub project_id: String,
    pub run_id: String,
    pub change_group_id: Option<String>,
    pub path: String,
    pub operation: String,
    pub old_hash: Option<String>,
    pub new_hash: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentCheckpointRecord {
    pub project_id: String,
    pub run_id: String,
    pub checkpoint_kind: String,
    pub summary: String,
    pub payload_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentActionRequestRecord {
    pub project_id: String,
    pub run_id: String,
    pub action_id: String,
    pub action_type: String,
    pub title: String,
    pub detail: String,
    pub created_at: String,
}

pub fn insert_agent_run(
    repo_root: &Path,
    record: &NewAgentRunRecord,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    validate_agent_run(record)?;
    let selection = match (
        record.agent_definition_id.as_deref(),
        record.agent_definition_version,
    ) {
        (Some(definition_id), Some(version)) => {
            let mut selection = resolve_agent_definition_for_run(
                repo_root,
                Some(definition_id),
                record.runtime_agent_id,
            )?;
            selection.version = version;
            selection
        }
        (Some(definition_id), None) => resolve_agent_definition_for_run(
            repo_root,
            Some(definition_id),
            record.runtime_agent_id,
        )?,
        (None, Some(version)) => {
            let mut selection =
                resolve_agent_definition_for_run(repo_root, None, record.runtime_agent_id)?;
            selection.version = version;
            selection
        }
        (None, None) => resolve_agent_definition_for_run(repo_root, None, record.runtime_agent_id)?,
    };
    let trace_id = xero_agent_core::runtime_trace_id_for_run(&record.project_id, &record.run_id);
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_runs (
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                provider_id,
                model_id,
                status,
                prompt,
                system_prompt,
                started_at,
                last_heartbeat_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'starting', ?10, ?11, ?12, ?12, ?12)
            "#,
            params![
                selection.runtime_agent_id.as_str(),
                selection.definition_id,
                selection.version,
                record.project_id,
                record.agent_session_id,
                record.run_id,
                trace_id,
                record.provider_id,
                record.model_id,
                record.prompt,
                record.system_prompt,
                record.now,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_insert_failed", error)
        })?;

    read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)
}

/// Atomically reserves a caller-supplied run id and its immutable start payload.
///
/// The reservation and the initial `starting` row commit together. Exact retries replay the
/// existing row; reusing the run id for different caller input is rejected before any transcript
/// writes can be duplicated.
pub fn register_agent_run_start(
    repo_root: &Path,
    record: &NewAgentRunRecord,
    payload_hash: &str,
    recovery_payload_json: &str,
    owner_process_id: u32,
    owner_process_birth_identity: &str,
) -> Result<AgentRunStartRegistrationResult, CommandError> {
    validate_agent_run(record)?;
    validate_payload_hash(payload_hash, "payloadHash")?;
    validate_json_payload(recovery_payload_json, "recoveryPayloadJson")?;
    validate_non_empty_text(owner_process_birth_identity, "ownerProcessBirthIdentity")?;
    if owner_process_id == 0 {
        return Err(CommandError::invalid_request("ownerProcessId"));
    }
    let selection = match (
        record.agent_definition_id.as_deref(),
        record.agent_definition_version,
    ) {
        (Some(definition_id), Some(version)) => {
            let mut selection = resolve_agent_definition_for_run(
                repo_root,
                Some(definition_id),
                record.runtime_agent_id,
            )?;
            selection.version = version;
            selection
        }
        (Some(definition_id), None) => resolve_agent_definition_for_run(
            repo_root,
            Some(definition_id),
            record.runtime_agent_id,
        )?,
        (None, Some(version)) => {
            let mut selection =
                resolve_agent_definition_for_run(repo_root, None, record.runtime_agent_id)?;
            selection.version = version;
            selection
        }
        (None, None) => resolve_agent_definition_for_run(repo_root, None, record.runtime_agent_id)?,
    };
    let trace_id = xero_agent_core::runtime_trace_id_for_run(&record.project_id, &record.run_id);
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_transaction_failed", error)
        })?;

    if let Some(existing) =
        read_agent_run_start_request(&transaction, &record.project_id, &record.run_id, repo_root)?
    {
        if existing.payload_hash != payload_hash {
            return Err(CommandError::user_fixable(
                "agent_run_start_conflict",
                format!(
                    "Owned-agent run id `{}` was already used for a different start request.",
                    record.run_id
                ),
            ));
        }
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_commit_failed", error)
        })?;
        let snapshot =
            read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)?;
        return Ok(AgentRunStartRegistrationResult::Replayed {
            snapshot,
            request: existing,
        });
    }

    let run_exists = transaction
        .query_row(
            "SELECT 1 FROM agent_runs WHERE project_id = ?1 AND run_id = ?2",
            params![record.project_id, record.run_id],
            |_| Ok(()),
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_start_existing_read_failed", error)
        })?
        .is_some();
    if run_exists {
        return Err(CommandError::user_fixable(
            "agent_run_start_conflict",
            format!(
                "Owned-agent run id `{}` already exists without the requested start identity.",
                record.run_id
            ),
        ));
    }

    transaction
        .execute(
            r#"
            INSERT INTO agent_runs (
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                provider_id,
                model_id,
                status,
                prompt,
                system_prompt,
                started_at,
                last_heartbeat_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'starting', ?10, ?11, ?12, ?12, ?12)
            "#,
            params![
                selection.runtime_agent_id.as_str(),
                selection.definition_id,
                selection.version,
                record.project_id,
                record.agent_session_id,
                record.run_id,
                trace_id,
                record.provider_id,
                record.model_id,
                record.prompt,
                record.system_prompt,
                record.now,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_insert_failed", error)
        })?;
    transaction
        .execute(
            r#"
            INSERT INTO agent_run_start_requests (
                project_id,
                run_id,
                payload_hash,
                recovery_payload_json,
                state,
                owner_process_id,
                owner_process_birth_identity,
                created_at
            ) VALUES (?1, ?2, ?3, ?4, 'preparing', ?5, ?6, ?7)
            "#,
            params![
                record.project_id,
                record.run_id,
                payload_hash,
                recovery_payload_json,
                i64::from(owner_process_id),
                owner_process_birth_identity,
                record.now,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_identity_insert_failed", error)
        })?;
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_run_start_commit_failed", error)
    })?;
    read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)
        .map(AgentRunStartRegistrationResult::Registered)
}

/// Atomically replaces an interrupted, never-dispatched run start with the same immutable
/// identity. The deterministic run id is never externally absent between the old and new rows.
pub fn replace_replayable_agent_run_start(
    repo_root: &Path,
    expected: &AgentRunStartRequestRecord,
    record: &NewAgentRunRecord,
    payload_hash: &str,
    recovery_payload_json: &str,
    owner_process_id: u32,
    owner_process_birth_identity: &str,
) -> Result<AgentRunStartRegistrationResult, CommandError> {
    validate_agent_run(record)?;
    validate_payload_hash(payload_hash, "payloadHash")?;
    validate_json_payload(recovery_payload_json, "recoveryPayloadJson")?;
    validate_non_empty_text(owner_process_birth_identity, "ownerProcessBirthIdentity")?;
    if owner_process_id == 0 {
        return Err(CommandError::invalid_request("ownerProcessId"));
    }
    if expected.project_id != record.project_id
        || expected.run_id != record.run_id
        || expected.payload_hash != payload_hash
    {
        return Err(CommandError::invalid_request("expectedStartRequest"));
    }
    let selection = match (
        record.agent_definition_id.as_deref(),
        record.agent_definition_version,
    ) {
        (Some(definition_id), Some(version)) => {
            let mut selection = resolve_agent_definition_for_run(
                repo_root,
                Some(definition_id),
                record.runtime_agent_id,
            )?;
            selection.version = version;
            selection
        }
        (Some(definition_id), None) => resolve_agent_definition_for_run(
            repo_root,
            Some(definition_id),
            record.runtime_agent_id,
        )?,
        (None, Some(version)) => {
            let mut selection =
                resolve_agent_definition_for_run(repo_root, None, record.runtime_agent_id)?;
            selection.version = version;
            selection
        }
        (None, None) => resolve_agent_definition_for_run(repo_root, None, record.runtime_agent_id)?,
    };
    let trace_id = xero_agent_core::runtime_trace_id_for_run(&record.project_id, &record.run_id);
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(
                repo_root,
                "agent_run_start_replace_transaction_failed",
                error,
            )
        })?;
    let existing =
        read_agent_run_start_request(&transaction, &record.project_id, &record.run_id, repo_root)?
            .ok_or_else(|| {
                CommandError::retryable(
                    "agent_run_start_replace_raced",
                    format!(
                        "Agent run `{}` changed before its start could be rebuilt.",
                        record.run_id
                    ),
                )
            })?;
    if existing.payload_hash != payload_hash {
        return Err(CommandError::user_fixable(
            "agent_run_start_conflict",
            format!(
                "Owned-agent run id `{}` was already used for a different start request.",
                record.run_id
            ),
        ));
    }
    if existing != *expected {
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_replace_commit_failed", error)
        })?;
        let snapshot =
            read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)?;
        return Ok(AgentRunStartRegistrationResult::Replayed {
            snapshot,
            request: existing,
        });
    }
    let unsafe_continuation_exists = transaction
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM agent_continuation_requests
                WHERE project_id = ?1 AND run_id = ?2 AND state IN ('driving', 'consumed')
            ) OR EXISTS (
                SELECT 1 FROM agent_handoff_lineage
                WHERE project_id = ?1 AND target_run_id = ?2 AND status = 'completed'
            )
            "#,
            params![record.project_id, record.run_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_start_replace_guard_failed", error)
        })?
        == 1;
    if !matches!(
        existing.state,
        AgentRunStartRequestState::Preparing | AgentRunStartRequestState::Failed
    ) || unsafe_continuation_exists
    {
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_replace_commit_failed", error)
        })?;
        let snapshot =
            read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)?;
        return Ok(AgentRunStartRegistrationResult::Replayed {
            snapshot,
            request: existing,
        });
    }
    transaction
        .execute(
            "DELETE FROM agent_runs WHERE project_id = ?1 AND run_id = ?2",
            params![record.project_id, record.run_id],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_replace_delete_failed", error)
        })?;
    transaction
        .execute(
            r#"
            INSERT INTO agent_runs (
                runtime_agent_id, agent_definition_id, agent_definition_version,
                project_id, agent_session_id, run_id, trace_id, provider_id, model_id,
                status, prompt, system_prompt, started_at, last_heartbeat_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'starting', ?10, ?11, ?12, ?12, ?12)
            "#,
            params![
                selection.runtime_agent_id.as_str(),
                selection.definition_id,
                selection.version,
                record.project_id,
                record.agent_session_id,
                record.run_id,
                trace_id,
                record.provider_id,
                record.model_id,
                record.prompt,
                record.system_prompt,
                record.now,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_replace_insert_failed", error)
        })?;
    transaction
        .execute(
            r#"
            INSERT INTO agent_run_start_requests (
                project_id, run_id, payload_hash, recovery_payload_json, state, owner_process_id,
                owner_process_birth_identity, created_at
            ) VALUES (?1, ?2, ?3, ?4, 'preparing', ?5, ?6, ?7)
            "#,
            params![
                record.project_id,
                record.run_id,
                payload_hash,
                recovery_payload_json,
                i64::from(owner_process_id),
                owner_process_birth_identity,
                record.now,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_replace_identity_failed", error)
        })?;
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_run_start_replace_commit_failed", error)
    })?;
    read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)
        .map(AgentRunStartRegistrationResult::Registered)
}

pub fn load_agent_run_start_request(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AgentRunStartRequestRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_run_start_request(&connection, project_id, run_id, repo_root)
}

pub fn list_ready_agent_run_starts(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<AgentRunStartRequestRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT starts.project_id, starts.run_id, starts.payload_hash,
                   starts.recovery_payload_json, starts.state,
                   starts.owner_process_id, starts.owner_process_birth_identity,
                   starts.created_at, starts.ready_at, starts.failed_at
            FROM agent_run_start_requests starts
            JOIN agent_runs runs
              ON runs.project_id = starts.project_id AND runs.run_id = starts.run_id
            WHERE starts.project_id = ?1 AND starts.state = 'ready' AND runs.status = 'running'
            ORDER BY starts.created_at ASC, starts.run_id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_start_list_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(AgentRunStartRequestRecord {
                project_id: row.get(0)?,
                run_id: row.get(1)?,
                payload_hash: row.get(2)?,
                recovery_payload_json: row.get(3)?,
                state: AgentRunStartRequestState::Ready,
                owner_process_id: read_positive_u32(row, 5)?,
                owner_process_birth_identity: row.get(6)?,
                created_at: row.get(7)?,
                ready_at: row.get(8)?,
                failed_at: row.get(9)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_start_list_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_run_start_list_failed", error)
    })
}

pub fn mark_agent_run_start_ready(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    timestamp: &str,
) -> Result<AgentRunStartRequestRecord, CommandError> {
    validate_non_empty_text(timestamp, "timestamp")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_run_start_requests
            SET state = 'ready', ready_at = ?3, failed_at = NULL
            WHERE project_id = ?1 AND run_id = ?2 AND state = 'preparing'
            "#,
            params![project_id, run_id, timestamp],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_ready_failed", error)
        })?;
    read_agent_run_start_request(&connection, project_id, run_id, repo_root)?.ok_or_else(|| {
        CommandError::system_fault(
            "agent_run_start_identity_missing",
            format!("Owned-agent run `{run_id}` has no durable start identity."),
        )
    })
}

pub fn fail_preparing_agent_run_start(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    diagnostic: AgentRunDiagnosticRecord,
    timestamp: &str,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    validate_non_empty_text(timestamp, "timestamp")?;
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_fail_transaction_failed", error)
        })?;
    let changed = transaction
        .execute(
            r#"
            UPDATE agent_run_start_requests
            SET state = 'failed', ready_at = NULL, failed_at = ?3
            WHERE project_id = ?1 AND run_id = ?2 AND state = 'preparing'
            "#,
            params![project_id, run_id, timestamp],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_start_fail_marker_failed", error)
        })?;
    if changed == 1 {
        transaction
            .execute(
                r#"
                UPDATE agent_runs
                SET status = 'failed', last_heartbeat_at = ?3,
                    completed_at = NULL, cancelled_at = NULL,
                    last_error_code = ?4, last_error_message = ?5, updated_at = ?3
                WHERE project_id = ?1 AND run_id = ?2
                  AND status NOT IN ('completed', 'handed_off', 'failed', 'cancelled')
                "#,
                params![
                    project_id,
                    run_id,
                    timestamp,
                    diagnostic.code,
                    diagnostic.message
                ],
            )
            .map_err(|error| {
                map_agent_store_write_error(repo_root, "agent_run_start_fail_status_failed", error)
            })?;
        let payload = serde_json::json!({
            "code": diagnostic.code,
            "message": diagnostic.message,
            "retryable": false,
            "state": "blocked",
            "stopReason": "blocked",
        })
        .to_string();
        insert_agent_event_in_transaction(
            &transaction,
            project_id,
            run_id,
            AgentRunEventKind::RunFailed,
            &payload,
            timestamp,
            repo_root,
        )?;
    }
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_run_start_fail_commit_failed", error)
    })?;
    read_agent_run_snapshot(&connection, repo_root, project_id, run_id)
}

pub fn append_agent_message(
    repo_root: &Path,
    record: &NewAgentMessageRecord,
) -> Result<AgentMessageRecord, CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    if record.content.trim().is_empty()
        && !(matches!(record.role, AgentMessageRole::Assistant)
            && record
                .provider_metadata_json
                .as_deref()
                .is_some_and(|metadata| !metadata.trim().is_empty()))
    {
        validate_non_empty_text(&record.content, "content")?;
    }
    if let Some(metadata_json) = record.provider_metadata_json.as_deref() {
        validate_provider_metadata_json(metadata_json)?;
    }
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection.transaction().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_message_transaction_failed", error)
    })?;
    transaction
        .execute(
            r#"
            INSERT INTO agent_messages (
                project_id,
                run_id,
                role,
                content,
                provider_metadata_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                record.project_id,
                record.run_id,
                agent_message_role_sql_value(&record.role),
                record.content,
                record.provider_metadata_json,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_message_insert_failed", error)
        })?;
    let id = transaction.last_insert_rowid();
    let mut stored_attachments = Vec::with_capacity(record.attachments.len());
    for attachment in &record.attachments {
        validate_non_empty_text(&attachment.storage_path, "attachment.storagePath")?;
        validate_non_empty_text(&attachment.media_type, "attachment.mediaType")?;
        validate_non_empty_text(&attachment.original_name, "attachment.originalName")?;
        if attachment.size_bytes < 0 {
            return Err(CommandError::user_fixable(
                "agent_message_attachment_invalid_size",
                "Xero refused to record an attachment with a negative size.",
            ));
        }
        transaction
            .execute(
                r#"
                INSERT INTO agent_message_attachments (
                    message_id,
                    project_id,
                    run_id,
                    kind,
                    storage_path,
                    media_type,
                    original_name,
                    size_bytes,
                    width,
                    height,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    id,
                    record.project_id,
                    record.run_id,
                    agent_message_attachment_kind_sql_value(&attachment.kind),
                    attachment.storage_path,
                    attachment.media_type,
                    attachment.original_name,
                    attachment.size_bytes,
                    attachment.width,
                    attachment.height,
                    record.created_at,
                ],
            )
            .map_err(|error| {
                map_agent_store_write_error(
                    repo_root,
                    "agent_message_attachment_insert_failed",
                    error,
                )
            })?;
        let attachment_id = transaction.last_insert_rowid();
        stored_attachments.push(AgentMessageAttachmentRecord {
            id: attachment_id,
            message_id: id,
            project_id: record.project_id.clone(),
            run_id: record.run_id.clone(),
            kind: attachment.kind.clone(),
            storage_path: attachment.storage_path.clone(),
            media_type: attachment.media_type.clone(),
            original_name: attachment.original_name.clone(),
            size_bytes: attachment.size_bytes,
            width: attachment.width,
            height: attachment.height,
            created_at: record.created_at.clone(),
        });
    }
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_message_transaction_commit_failed", error)
    })?;
    Ok(AgentMessageRecord {
        id,
        project_id: record.project_id.clone(),
        run_id: record.run_id.clone(),
        role: record.role.clone(),
        content: record.content.clone(),
        provider_metadata_json: record.provider_metadata_json.clone(),
        created_at: record.created_at.clone(),
        attachments: stored_attachments,
    })
}

pub fn update_agent_run_lineage(
    repo_root: &Path,
    record: &AgentRunLineageUpdateRecord,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.parent_run_id, "parentRunId")?;
    validate_non_empty_text(&record.parent_trace_id, "parentTraceId")?;
    validate_non_empty_text(&record.parent_subagent_id, "parentSubagentId")?;
    validate_non_empty_text(&record.subagent_role, "subagentRole")?;
    let connection = open_agent_database(repo_root)?;
    let updated = connection
        .execute(
            r#"
            UPDATE agent_runs
            SET lineage_kind = 'subagent_child',
                parent_run_id = ?3,
                parent_trace_id = ?4,
                parent_subagent_id = ?5,
                subagent_role = ?6,
                updated_at = ?7
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![
                record.project_id,
                record.run_id,
                record.parent_run_id,
                record.parent_trace_id,
                record.parent_subagent_id,
                record.subagent_role,
                record.updated_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_lineage_update_failed", error)
        })?;
    if updated != 1 {
        return Err(CommandError::system_fault(
            "agent_run_lineage_run_missing",
            format!(
                "Xero could not attach subagent lineage to run `{}` in project `{}` because the run was not found.",
                record.run_id, record.project_id
            ),
        ));
    }
    read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)
}

pub fn append_agent_event(
    repo_root: &Path,
    record: &NewAgentEventRecord,
) -> Result<AgentEventRecord, CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_json_payload(&record.payload_json, "payloadJson")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                record.project_id,
                record.run_id,
                agent_event_kind_sql_value(&record.event_kind),
                record.payload_json,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_event_insert_failed", error)
        })?;

    let id = connection.last_insert_rowid();
    Ok(AgentEventRecord {
        id,
        project_id: record.project_id.clone(),
        run_id: record.run_id.clone(),
        event_kind: record.event_kind.clone(),
        payload_json: record.payload_json.clone(),
        created_at: record.created_at.clone(),
    })
}

pub fn read_agent_events_after(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    after_event_id: i64,
    limit: usize,
) -> Result<Vec<AgentEventRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let limit = limit.clamp(1, 1_000) as i64;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, event_kind, payload_json, created_at
            FROM agent_events
            WHERE project_id = ?1
              AND run_id = ?2
              AND id > ?3
            ORDER BY id ASC
            LIMIT ?4
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_events_after_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id, after_event_id, limit], |row| {
            Ok(AgentEventRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                event_kind: parse_agent_event_kind(row.get::<_, String>(3)?.as_str()),
                payload_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_events_after_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_events_after_decode_failed", error)
    })
}

pub fn read_all_agent_events(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AgentEventRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_events(&connection, project_id, run_id, repo_root)
}

pub fn read_latest_agent_events(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    limit: usize,
) -> Result<Vec<AgentEventRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let limit = limit.clamp(1, 1_000) as i64;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, event_kind, payload_json, created_at
            FROM agent_events
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY id DESC
            LIMIT ?3
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_events_latest_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id, limit], |row| {
            Ok(AgentEventRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                event_kind: parse_agent_event_kind(row.get::<_, String>(3)?.as_str()),
                payload_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_events_latest_query_failed", error)
        })?;
    let mut events = rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_events_latest_decode_failed", error)
    })?;
    events.reverse();
    Ok(events)
}

pub fn read_agent_file_change_paths_for_tool_call(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call_id: &str,
) -> Result<Vec<String>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(tool_call_id, "toolCallId")?;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT json_extract(payload_json, '$.path'),
                   json_extract(payload_json, '$.toPath')
            FROM agent_events
            WHERE project_id = ?1
              AND run_id = ?2
              AND event_kind = ?3
              AND json_extract(payload_json, '$.toolCallId') = ?4
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_file_change_paths_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(
            params![
                project_id,
                run_id,
                agent_event_kind_sql_value(&AgentRunEventKind::FileChanged),
                tool_call_id,
            ],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                ))
            },
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_file_change_paths_query_failed", error)
        })?;
    let mut paths = Vec::new();
    for row in rows {
        let (path, to_path) = row.map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_file_change_paths_decode_failed", error)
        })?;
        for path in [path, to_path].into_iter().flatten() {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

pub fn read_latest_agent_event_by_payload_kind(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    event_kind: AgentRunEventKind,
    payload_kind: &str,
) -> Result<Option<AgentEventRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(payload_kind, "payloadKind")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .query_row(
            r#"
            SELECT id, project_id, run_id, event_kind, payload_json, created_at
            FROM agent_events
            WHERE project_id = ?1
              AND run_id = ?2
              AND event_kind = ?3
              AND json_extract(payload_json, '$.kind') = ?4
            ORDER BY id DESC
            LIMIT 1
            "#,
            params![
                project_id,
                run_id,
                agent_event_kind_sql_value(&event_kind),
                payload_kind,
            ],
            |row| {
                Ok(AgentEventRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    run_id: row.get(2)?,
                    event_kind: parse_agent_event_kind(row.get::<_, String>(3)?.as_str()),
                    payload_json: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_event_payload_kind_read_failed", error)
        })
}

pub fn upsert_agent_environment_lifecycle_snapshot(
    repo_root: &Path,
    record: &NewAgentEnvironmentLifecycleSnapshotRecord,
) -> Result<AgentEnvironmentLifecycleSnapshotRecord, CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.environment_id, "environmentId")?;
    validate_non_empty_text(&record.state, "state")?;
    if let Some(previous_state) = record.previous_state.as_ref() {
        validate_non_empty_text(previous_state, "previousState")?;
    }
    validate_json_payload(&record.health_checks_json, "healthChecksJson")?;
    validate_json_payload(&record.setup_steps_json, "setupStepsJson")?;
    if let Some(diagnostic_json) = record.diagnostic_json.as_ref() {
        validate_json_payload(diagnostic_json, "diagnosticJson")?;
    }
    validate_json_payload(&record.snapshot_json, "snapshotJson")?;
    validate_non_empty_text(&record.updated_at, "updatedAt")?;
    if record.pending_message_count < 0 {
        return Err(CommandError::user_fixable(
            "agent_environment_lifecycle_pending_count_invalid",
            "Environment lifecycle pending-message count must be zero or greater.",
        ));
    }

    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_environment_lifecycle_snapshots (
                project_id,
                run_id,
                environment_id,
                state,
                previous_state,
                pending_message_count,
                health_checks_json,
                setup_steps_json,
                diagnostic_json,
                snapshot_json,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(project_id, run_id) DO UPDATE SET
                environment_id = excluded.environment_id,
                state = excluded.state,
                previous_state = excluded.previous_state,
                pending_message_count = excluded.pending_message_count,
                health_checks_json = excluded.health_checks_json,
                setup_steps_json = excluded.setup_steps_json,
                diagnostic_json = excluded.diagnostic_json,
                snapshot_json = excluded.snapshot_json,
                updated_at = excluded.updated_at
            "#,
            params![
                record.project_id,
                record.run_id,
                record.environment_id,
                record.state,
                record.previous_state,
                record.pending_message_count,
                record.health_checks_json,
                record.setup_steps_json,
                record.diagnostic_json,
                record.snapshot_json,
                record.updated_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(
                repo_root,
                "agent_environment_lifecycle_snapshot_upsert_failed",
                error,
            )
        })?;

    read_agent_environment_lifecycle_snapshot(
        &connection,
        repo_root,
        &record.project_id,
        &record.run_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_environment_lifecycle_snapshot_missing",
            format!(
                "Xero could not reload the lifecycle snapshot for run `{}` after saving it.",
                record.run_id
            ),
        )
    })
}

pub fn load_agent_environment_lifecycle_snapshot(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AgentEnvironmentLifecycleSnapshotRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_environment_lifecycle_snapshot(&connection, repo_root, project_id, run_id)
}

fn read_agent_environment_lifecycle_snapshot(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AgentEnvironmentLifecycleSnapshotRecord>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                run_id,
                environment_id,
                state,
                previous_state,
                pending_message_count,
                health_checks_json,
                setup_steps_json,
                diagnostic_json,
                snapshot_json,
                updated_at
            FROM agent_environment_lifecycle_snapshots
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id],
            read_agent_environment_lifecycle_snapshot_row,
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(
                repo_root,
                "agent_environment_lifecycle_snapshot_read_failed",
                error,
            )
        })
}

pub fn insert_agent_environment_pending_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    role: AgentMessageRole,
    content: &str,
    submitted_at: &str,
) -> Result<AgentEnvironmentPendingMessageRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(content, "content")?;
    validate_non_empty_text(submitted_at, "submittedAt")?;
    if role != AgentMessageRole::User {
        return Err(CommandError::user_fixable(
            "agent_environment_pending_message_role_invalid",
            "Environment pending messages currently accept only user messages.",
        ));
    }

    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_environment_pending_messages (
                project_id,
                run_id,
                role,
                content,
                submitted_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                project_id,
                run_id,
                agent_message_role_sql_value(&role),
                content,
                submitted_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(
                repo_root,
                "agent_environment_pending_message_insert_failed",
                error,
            )
        })?;
    let id = connection.last_insert_rowid();
    Ok(AgentEnvironmentPendingMessageRecord {
        id,
        project_id: project_id.into(),
        run_id: run_id.into(),
        role,
        content: content.into(),
        submitted_at: submitted_at.into(),
        delivered_at: None,
    })
}

pub fn list_undelivered_agent_environment_pending_messages(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AgentEnvironmentPendingMessageRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, role, content, submitted_at, delivered_at
            FROM agent_environment_pending_messages
            WHERE project_id = ?1
              AND run_id = ?2
              AND delivered_at IS NULL
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(
                repo_root,
                "agent_environment_pending_messages_prepare_failed",
                error,
            )
        })?;
    let rows = statement
        .query_map(
            params![project_id, run_id],
            read_agent_environment_pending_message_row,
        )
        .map_err(|error| {
            map_agent_store_query_error(
                repo_root,
                "agent_environment_pending_messages_query_failed",
                error,
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(
            repo_root,
            "agent_environment_pending_messages_decode_failed",
            error,
        )
    })
}

pub fn count_undelivered_agent_environment_pending_messages(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<i64, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM agent_environment_pending_messages
            WHERE project_id = ?1
              AND run_id = ?2
              AND delivered_at IS NULL
            "#,
            params![project_id, run_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            map_agent_store_query_error(
                repo_root,
                "agent_environment_pending_messages_count_failed",
                error,
            )
        })
}

pub fn mark_agent_environment_pending_messages_delivered(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    message_ids: &[i64],
    delivered_at: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(delivered_at, "deliveredAt")?;
    if message_ids.is_empty() {
        return Ok(());
    }

    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection.transaction().map_err(|error| {
        map_agent_store_write_error(
            repo_root,
            "agent_environment_pending_messages_transaction_failed",
            error,
        )
    })?;
    for message_id in message_ids {
        transaction
            .execute(
                r#"
                UPDATE agent_environment_pending_messages
                SET delivered_at = ?4
                WHERE project_id = ?1
                  AND run_id = ?2
                  AND id = ?3
                  AND delivered_at IS NULL
                "#,
                params![project_id, run_id, message_id, delivered_at],
            )
            .map_err(|error| {
                map_agent_store_write_error(
                    repo_root,
                    "agent_environment_pending_message_deliver_failed",
                    error,
                )
            })?;
    }
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(
            repo_root,
            "agent_environment_pending_messages_commit_failed",
            error,
        )
    })
}

pub fn start_agent_tool_call(
    repo_root: &Path,
    record: &AgentToolCallStartRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.tool_call_id, "toolCallId")?;
    validate_non_empty_text(&record.tool_name, "toolName")?;
    validate_json_payload(&record.input_json, "inputJson")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_tool_calls (
                project_id,
                run_id,
                tool_call_id,
                tool_name,
                input_json,
                state,
                started_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'running', ?6)
            ON CONFLICT(project_id, run_id, tool_call_id) DO UPDATE SET
                tool_name = excluded.tool_name,
                input_json = excluded.input_json,
                state = 'running',
                result_json = NULL,
                error_code = NULL,
                error_message = NULL,
                completed_at = NULL
            "#,
            params![
                record.project_id,
                record.run_id,
                record.tool_call_id,
                record.tool_name,
                record.input_json,
                record.started_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_tool_call_start_failed", error)
        })?;
    Ok(())
}

pub fn finish_agent_tool_call(
    repo_root: &Path,
    record: &AgentToolCallFinishRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.tool_call_id, "toolCallId")?;
    if let Some(result_json) = &record.result_json {
        validate_json_payload(result_json, "resultJson")?;
    }
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_tool_calls
            SET state = ?4,
                result_json = ?5,
                error_code = ?6,
                error_message = ?7,
                completed_at = ?8
            WHERE project_id = ?1
              AND run_id = ?2
              AND tool_call_id = ?3
            "#,
            params![
                record.project_id,
                record.run_id,
                record.tool_call_id,
                agent_tool_call_state_sql_value(&record.state),
                record.result_json,
                record.error.as_ref().map(|error| error.code.as_str()),
                record.error.as_ref().map(|error| error.message.as_str()),
                record.completed_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_tool_call_finish_failed", error)
        })?;
    Ok(())
}

pub fn append_agent_file_change(
    repo_root: &Path,
    record: &NewAgentFileChangeRecord,
) -> Result<AgentFileChangeRecord, CommandError> {
    validate_new_agent_file_change(record)?;
    let connection = open_agent_database(repo_root)?;
    insert_agent_file_change_with_connection(&connection, repo_root, record)
}

/// Persist a file-change row and its durable event in one transaction.
///
/// `prepare_event_payload` runs after the row is inserted but before the event
/// is written or the transaction commits, so required freshness work can fail
/// the operation without leaving either relational record behind.
pub fn append_agent_file_change_with_event<F>(
    repo_root: &Path,
    record: &NewAgentFileChangeRecord,
    prepare_event_payload: F,
) -> Result<(AgentFileChangeRecord, AgentEventRecord), CommandError>
where
    F: FnOnce(&AgentFileChangeRecord) -> Result<JsonValue, CommandError>,
{
    validate_new_agent_file_change(record)?;
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_file_change_transaction_failed", error)
        })?;
    let change = insert_agent_file_change_with_connection(&transaction, repo_root, record)?;
    let payload_json =
        serde_json::to_string(&prepare_event_payload(&change)?).map_err(|error| {
            CommandError::system_fault(
                "agent_file_change_event_serialize_failed",
                format!("Xero could not serialize a file-change event payload: {error}"),
            )
        })?;
    let event = insert_agent_event_in_transaction(
        &transaction,
        &record.project_id,
        &record.run_id,
        AgentRunEventKind::FileChanged,
        &payload_json,
        &record.created_at,
        repo_root,
    )?;
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_file_change_commit_failed", error)
    })?;
    Ok((change, event))
}

fn validate_new_agent_file_change(record: &NewAgentFileChangeRecord) -> Result<(), CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    if let Some(change_group_id) = record.change_group_id.as_deref() {
        validate_non_empty_text(change_group_id, "changeGroupId")?;
    }
    validate_non_empty_text(&record.path, "path")?;
    validate_non_empty_text(&record.operation, "operation")?;
    validate_optional_sha256(record.old_hash.as_deref(), "oldHash")?;
    validate_optional_sha256(record.new_hash.as_deref(), "newHash")?;
    validate_non_empty_text(&record.created_at, "createdAt")
}

fn insert_agent_file_change_with_connection(
    connection: &Connection,
    repo_root: &Path,
    record: &NewAgentFileChangeRecord,
) -> Result<AgentFileChangeRecord, CommandError> {
    let inserted = connection
        .execute(
            r#"
            INSERT INTO agent_file_changes (
                project_id,
                run_id,
                trace_id,
                top_level_run_id,
                subagent_id,
                subagent_role,
                change_group_id,
                path,
                operation,
                old_hash,
                new_hash,
                created_at
            )
            SELECT
                ?1,
                ?2,
                agent_runs.trace_id,
                COALESCE(agent_runs.parent_run_id, agent_runs.run_id),
                agent_runs.parent_subagent_id,
                agent_runs.subagent_role,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7,
                ?8
            FROM agent_runs
            WHERE agent_runs.project_id = ?1
              AND agent_runs.run_id = ?2
            "#,
            params![
                record.project_id,
                record.run_id,
                record.change_group_id,
                record.path,
                record.operation,
                record.old_hash,
                record.new_hash,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_file_change_insert_failed", error)
        })?;
    if inserted != 1 {
        return Err(CommandError::system_fault(
            "agent_file_change_run_missing",
            format!(
                "Xero could not attribute a file change for run `{}` in project `{}` because the run was not found.",
                record.run_id, record.project_id
            ),
        ));
    }

    let id = connection.last_insert_rowid();
    connection
        .query_row(
            r#"
            SELECT
                id,
                project_id,
                run_id,
                trace_id,
                top_level_run_id,
                subagent_id,
                subagent_role,
                change_group_id,
                path,
                operation,
                old_hash,
                new_hash,
                created_at
            FROM agent_file_changes
            WHERE id = ?1
            "#,
            params![id],
            |row| {
                Ok(AgentFileChangeRecord {
                    id: row.get(0)?,
                    project_id: row.get(1)?,
                    run_id: row.get(2)?,
                    trace_id: row.get(3)?,
                    top_level_run_id: row.get(4)?,
                    subagent_id: row.get(5)?,
                    subagent_role: row.get(6)?,
                    change_group_id: row.get(7)?,
                    path: row.get(8)?,
                    operation: row.get(9)?,
                    old_hash: row.get(10)?,
                    new_hash: row.get(11)?,
                    created_at: row.get(12)?,
                })
            },
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_file_change_query_failed", error)
        })
}

pub fn append_agent_checkpoint(
    repo_root: &Path,
    record: &NewAgentCheckpointRecord,
) -> Result<AgentCheckpointRecord, CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.checkpoint_kind, "checkpointKind")?;
    validate_non_empty_text(&record.summary, "summary")?;
    if let Some(payload_json) = &record.payload_json {
        validate_json_payload(payload_json, "payloadJson")?;
    }

    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_checkpoints (
                project_id,
                run_id,
                checkpoint_kind,
                summary,
                payload_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
            params![
                record.project_id,
                record.run_id,
                record.checkpoint_kind,
                record.summary,
                record.payload_json,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_checkpoint_insert_failed", error)
        })?;

    let id = connection.last_insert_rowid();
    Ok(AgentCheckpointRecord {
        id,
        project_id: record.project_id.clone(),
        run_id: record.run_id.clone(),
        checkpoint_kind: record.checkpoint_kind.clone(),
        summary: record.summary.clone(),
        payload_json: record.payload_json.clone(),
        created_at: record.created_at.clone(),
    })
}

pub fn append_agent_action_request(
    repo_root: &Path,
    record: &NewAgentActionRequestRecord,
) -> Result<AgentActionRequestRecord, CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.action_id, "actionId")?;
    validate_non_empty_text(&record.action_type, "actionType")?;
    validate_non_empty_text(&record.title, "title")?;
    validate_non_empty_text(&record.detail, "detail")?;

    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_action_requests (
                project_id,
                run_id,
                action_id,
                action_type,
                title,
                detail,
                status,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7)
            ON CONFLICT(project_id, run_id, action_id) DO UPDATE SET
                action_type = excluded.action_type,
                title = excluded.title,
                detail = excluded.detail,
                status = 'pending',
                created_at = excluded.created_at,
                resolved_at = NULL,
                response = NULL
            "#,
            params![
                record.project_id,
                record.run_id,
                record.action_id,
                record.action_type,
                record.title,
                record.detail,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_request_insert_failed", error)
        })?;

    Ok(AgentActionRequestRecord {
        project_id: record.project_id.clone(),
        run_id: record.run_id.clone(),
        action_id: record.action_id.clone(),
        action_type: record.action_type.clone(),
        title: record.title.clone(),
        detail: record.detail.clone(),
        status: "pending".into(),
        created_at: record.created_at.clone(),
        resolved_at: None,
        response: None,
    })
}

pub fn answer_pending_agent_action_requests(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    response: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(response, "response")?;
    let now = crate::auth::now_timestamp();
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_action_requests
            SET status = 'answered',
                resolved_at = ?3,
                response = ?4
            WHERE project_id = ?1
              AND run_id = ?2
              AND status = 'pending'
            "#,
            params![project_id, run_id, now, response],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_request_answer_failed", error)
        })?;
    Ok(())
}

pub fn answer_pending_agent_action_request(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    response: &str,
) -> Result<AgentActionRequestRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(action_id, "actionId")?;
    validate_non_empty_text(response, "response")?;

    let connection = open_agent_database(repo_root)?;
    let existing = read_agent_action_requests(&connection, project_id, run_id, repo_root)?
        .into_iter()
        .find(|action| action.action_id == action_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_action_request_not_found",
                format!(
                    "Xero could not find pending owned-agent action `{action_id}` for run `{run_id}`."
                ),
            )
        })?;
    if existing.status != "pending" {
        return Err(CommandError::user_fixable(
            "agent_action_request_already_resolved",
            format!(
                "Xero cannot answer owned-agent action `{action_id}` because it is already {}.",
                existing.status
            ),
        ));
    }

    let now = crate::auth::now_timestamp();
    connection
        .execute(
            r#"
            UPDATE agent_action_requests
            SET status = 'answered',
                resolved_at = ?4,
                response = ?5
            WHERE project_id = ?1
              AND run_id = ?2
              AND action_id = ?3
              AND status = 'pending'
            "#,
            params![project_id, run_id, action_id, now, response],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_request_answer_failed", error)
        })?;

    Ok(AgentActionRequestRecord {
        status: "answered".into(),
        resolved_at: Some(now),
        response: Some(response.to_owned()),
        ..existing
    })
}

pub fn reject_pending_agent_action_request(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    response: Option<&str>,
) -> Result<AgentActionRequestRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(action_id, "actionId")?;
    if let Some(response) = response {
        validate_non_empty_text(response, "response")?;
    }

    let connection = open_agent_database(repo_root)?;
    let existing = read_agent_action_requests(&connection, project_id, run_id, repo_root)?
        .into_iter()
        .find(|action| action.action_id == action_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_action_request_not_found",
                format!(
                    "Xero could not find pending owned-agent action `{action_id}` for run `{run_id}`."
                ),
            )
        })?;
    if existing.status != "pending" {
        return Err(CommandError::user_fixable(
            "agent_action_request_already_resolved",
            format!(
                "Xero cannot reject owned-agent action `{action_id}` because it is already {}.",
                existing.status
            ),
        ));
    }

    let now = crate::auth::now_timestamp();
    connection
        .execute(
            r#"
            UPDATE agent_action_requests
            SET status = 'rejected',
                resolved_at = ?4,
                response = ?5
            WHERE project_id = ?1
              AND run_id = ?2
              AND action_id = ?3
              AND status = 'pending'
            "#,
            params![project_id, run_id, action_id, now, response],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_request_reject_failed", error)
        })?;

    Ok(AgentActionRequestRecord {
        status: "rejected".into(),
        resolved_at: Some(now),
        response: response.map(ToOwned::to_owned),
        ..existing
    })
}

/// Atomically rejects one pending action, records its deterministic terminal events, and fails
/// the run. Exact retries replay the committed snapshot; a different response for the same
/// action id is a conflict.
#[allow(clippy::too_many_arguments)]
pub fn reject_agent_action_and_fail_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    response: Option<&str>,
    resolved_at: &str,
) -> Result<AgentActionRejectionCommitResult, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(action_id, "actionId")?;
    validate_non_empty_text(resolved_at, "resolvedAt")?;
    if let Some(response) = response {
        validate_non_empty_text(response, "response")?;
    }
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_reject_transaction_failed", error)
        })?;
    let existing = read_agent_action_requests(&transaction, project_id, run_id, repo_root)?
        .into_iter()
        .find(|action| action.action_id == action_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_action_request_not_found",
                format!(
                    "Xero could not find pending owned-agent action `{action_id}` for run `{run_id}`."
                ),
            )
        })?;

    if existing.status == "rejected" {
        if existing.response.as_deref() != response {
            return Err(CommandError::user_fixable(
                "agent_action_rejection_conflict",
                format!(
                    "Owned-agent action `{action_id}` was already rejected with a different response."
                ),
            ));
        }
        let snapshot = read_agent_run_snapshot(&transaction, repo_root, project_id, run_id)?;
        if snapshot.run.status != AgentRunStatus::Failed
            || snapshot
                .run
                .last_error
                .as_ref()
                .map(|error| error.code.as_str())
                != Some("agent_action_rejected")
        {
            return Err(CommandError::system_fault(
                "agent_action_rejection_incomplete",
                format!(
                    "Owned-agent action `{action_id}` is rejected but run `{run_id}` is missing its atomic terminal state."
                ),
            ));
        }
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_reject_commit_failed", error)
        })?;
        return Ok(AgentActionRejectionCommitResult {
            snapshot,
            action: existing,
            inserted_events: Vec::new(),
            replayed: true,
        });
    }
    if existing.status != "pending" {
        return Err(CommandError::user_fixable(
            "agent_action_request_already_resolved",
            format!(
                "Xero cannot reject owned-agent action `{action_id}` because it is already {}.",
                existing.status
            ),
        ));
    }

    let action_changed = transaction
        .execute(
            r#"
            UPDATE agent_action_requests
            SET status = 'rejected', resolved_at = ?4, response = ?5
            WHERE project_id = ?1 AND run_id = ?2 AND action_id = ?3 AND status = 'pending'
            "#,
            params![project_id, run_id, action_id, resolved_at, response],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_request_reject_failed", error)
        })?;
    if action_changed != 1 {
        return Err(CommandError::retryable(
            "agent_action_rejection_raced",
            format!("Owned-agent action `{action_id}` changed while it was being rejected."),
        ));
    }

    let event_payloads = [
        (
            AgentRunEventKind::PolicyDecision,
            serde_json::json!({
                "kind": "approval_decision",
                "actionId": existing.action_id,
                "actionType": existing.action_type,
                "decision": "rejected",
                "response": response,
                "status": "rejected",
            }),
        ),
        (
            AgentRunEventKind::StateTransition,
            serde_json::json!({
                "kind": "agent_state_transition",
                "from": "approval_wait",
                "to": "blocked",
                "reason": "Operator rejected a pending owned-agent action.",
                "stopReason": "blocked",
                "actionId": action_id,
                "decision": "rejected",
            }),
        ),
        (
            AgentRunEventKind::RunFailed,
            serde_json::json!({
                "code": "agent_action_rejected",
                "message": format!("Operator rejected action `{action_id}`."),
                "retryable": false,
                "state": "blocked",
                "stopReason": "blocked",
            }),
        ),
    ];
    let mut inserted_events = Vec::with_capacity(event_payloads.len());
    for (kind, payload) in event_payloads {
        let payload = serde_json::to_string(&payload).map_err(|error| {
            CommandError::system_fault(
                "agent_action_reject_event_serialize_failed",
                format!("Xero could not encode an action-rejection event: {error}"),
            )
        })?;
        inserted_events.push(insert_agent_event_in_transaction(
            &transaction,
            project_id,
            run_id,
            kind,
            &payload,
            resolved_at,
            repo_root,
        )?);
    }

    let run_changed = transaction
        .execute(
            r#"
            UPDATE agent_runs
            SET status = 'failed', last_heartbeat_at = ?3,
                completed_at = NULL, cancelled_at = NULL,
                last_error_code = 'agent_action_rejected',
                last_error_message = ?4, updated_at = ?3
            WHERE project_id = ?1 AND run_id = ?2
              AND status IN ('starting', 'running', 'paused')
            "#,
            params![
                project_id,
                run_id,
                resolved_at,
                format!("Operator rejected action `{action_id}`."),
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_action_reject_run_failed", error)
        })?;
    if run_changed != 1 {
        let snapshot = read_agent_run_snapshot(&transaction, repo_root, project_id, run_id)?;
        return Err(CommandError::user_fixable(
            "agent_run_terminal",
            format!(
                "Xero cannot reject action `{action_id}` because owned-agent run `{run_id}` is {:?}.",
                snapshot.run.status
            ),
        ));
    }

    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_action_reject_commit_failed", error)
    })?;
    let snapshot = read_agent_run_snapshot(&connection, repo_root, project_id, run_id)?;
    Ok(AgentActionRejectionCommitResult {
        snapshot,
        action: AgentActionRequestRecord {
            status: "rejected".into(),
            resolved_at: Some(resolved_at.to_owned()),
            response: response.map(ToOwned::to_owned),
            ..existing
        },
        inserted_events,
        replayed: false,
    })
}

pub fn upsert_agent_usage(repo_root: &Path, record: &AgentUsageRecord) -> Result<(), CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.agent_definition_id, "agentDefinitionId")?;
    validate_non_empty_text(&record.provider_id, "providerId")?;
    validate_non_empty_text(&record.model_id, "modelId")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_usage (
                project_id,
                run_id,
                agent_definition_id,
                agent_definition_version,
                provider_id,
                model_id,
                input_tokens,
                billable_input_tokens,
                output_tokens,
                total_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                estimated_cost_micros,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(project_id, run_id) DO UPDATE SET
                agent_definition_id = excluded.agent_definition_id,
                agent_definition_version = excluded.agent_definition_version,
                provider_id = excluded.provider_id,
                model_id = excluded.model_id,
                input_tokens = excluded.input_tokens,
                billable_input_tokens = excluded.billable_input_tokens,
                output_tokens = excluded.output_tokens,
                total_tokens = excluded.total_tokens,
                cache_read_tokens = excluded.cache_read_tokens,
                cache_creation_tokens = excluded.cache_creation_tokens,
                estimated_cost_micros = excluded.estimated_cost_micros,
                updated_at = excluded.updated_at
            "#,
            params![
                record.project_id,
                record.run_id,
                record.agent_definition_id,
                record.agent_definition_version,
                record.provider_id,
                record.model_id,
                record.input_tokens,
                record.billable_input_tokens,
                record.output_tokens,
                record.total_tokens,
                record.cache_read_tokens,
                record.cache_creation_tokens,
                record.estimated_cost_micros,
                record.updated_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_usage_upsert_failed", error)
        })?;
    Ok(())
}

pub fn claim_agent_run_drive_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    owner_instance_id: &str,
    owner_process_id: u32,
    owner_process_birth_identity: &str,
    drive_token: &str,
    acquired_at: &str,
) -> Result<AgentRunDriveLeaseClaimResult, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(owner_instance_id, "ownerInstanceId")?;
    validate_non_empty_text(owner_process_birth_identity, "ownerProcessBirthIdentity")?;
    validate_non_empty_text(drive_token, "driveToken")?;
    validate_non_empty_text(acquired_at, "acquiredAt")?;
    if owner_process_id == 0 {
        return Err(CommandError::invalid_request("ownerProcessId"));
    }

    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_drive_lease_claim_failed", error)
        })?;
    let run_status = transaction
        .query_row(
            "SELECT status FROM agent_runs WHERE project_id = ?1 AND run_id = ?2",
            params![project_id, run_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_drive_lease_status_failed", error)
        })?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_run_not_found",
                format!("Owned-agent run `{run_id}` was not found."),
            )
        })?;
    let run_status = parse_agent_run_status(&run_status);
    if matches!(
        run_status,
        AgentRunStatus::Cancelling | AgentRunStatus::Cancelled | AgentRunStatus::HandedOff
    ) {
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_drive_lease_claim_failed", error)
        })?;
        return Ok(AgentRunDriveLeaseClaimResult::RunNotDrivable(run_status));
    }
    let inserted = transaction
        .execute(
            r#"
            INSERT OR IGNORE INTO agent_run_drive_leases (
                project_id,
                run_id,
                owner_instance_id,
                owner_process_id,
                owner_process_birth_identity,
                drive_token,
                acquired_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                project_id,
                run_id,
                owner_instance_id,
                i64::from(owner_process_id),
                owner_process_birth_identity,
                drive_token,
                acquired_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_drive_lease_claim_failed", error)
        })?;

    let outcome = if inserted == 1 {
        AgentRunDriveLeaseClaimResult::Acquired
    } else {
        let held = transaction
            .query_row(
                r#"
                SELECT
                    project_id,
                    run_id,
                    owner_instance_id,
                    owner_process_id,
                    owner_process_birth_identity,
                    drive_token,
                    acquired_at
                FROM agent_run_drive_leases
                WHERE project_id = ?1
                  AND run_id = ?2
                "#,
                params![project_id, run_id],
                read_agent_run_drive_lease,
            )
            .map_err(|error| {
                map_agent_store_query_error(repo_root, "agent_run_drive_lease_read_failed", error)
            })?;
        AgentRunDriveLeaseClaimResult::Held(held)
    };
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_run_drive_lease_claim_failed", error)
    })?;
    Ok(outcome)
}

pub fn replace_agent_run_drive_lease(
    repo_root: &Path,
    replacement: &AgentRunDriveLeaseRecord,
    expected: &AgentRunDriveLeaseRecord,
) -> Result<bool, CommandError> {
    validate_non_empty_text(&replacement.project_id, "projectId")?;
    validate_non_empty_text(&replacement.run_id, "runId")?;
    validate_non_empty_text(&replacement.owner_instance_id, "ownerInstanceId")?;
    validate_non_empty_text(
        &replacement.owner_process_birth_identity,
        "ownerProcessBirthIdentity",
    )?;
    validate_non_empty_text(&replacement.drive_token, "driveToken")?;
    validate_non_empty_text(&replacement.acquired_at, "acquiredAt")?;
    if replacement.owner_process_id == 0 {
        return Err(CommandError::invalid_request("ownerProcessId"));
    }
    if replacement.project_id != expected.project_id || replacement.run_id != expected.run_id {
        return Err(CommandError::invalid_request("expectedLease"));
    }

    let connection = open_agent_database(repo_root)?;
    let rows = connection
        .execute(
            r#"
            UPDATE agent_run_drive_leases
            SET owner_instance_id = ?3,
                owner_process_id = ?4,
                owner_process_birth_identity = ?5,
                drive_token = ?6,
                acquired_at = ?7
            WHERE project_id = ?1
              AND run_id = ?2
              AND owner_instance_id = ?8
              AND owner_process_id = ?9
              AND owner_process_birth_identity = ?10
              AND drive_token = ?11
            "#,
            params![
                replacement.project_id,
                replacement.run_id,
                replacement.owner_instance_id,
                i64::from(replacement.owner_process_id),
                replacement.owner_process_birth_identity,
                replacement.drive_token,
                replacement.acquired_at,
                expected.owner_instance_id,
                i64::from(expected.owner_process_id),
                expected.owner_process_birth_identity,
                expected.drive_token,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_drive_lease_replace_failed", error)
        })?;
    Ok(rows == 1)
}

pub fn load_agent_run_drive_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AgentRunDriveLeaseRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                run_id,
                owner_instance_id,
                owner_process_id,
                owner_process_birth_identity,
                drive_token,
                acquired_at
            FROM agent_run_drive_leases
            WHERE project_id = ?1 AND run_id = ?2
            "#,
            params![project_id, run_id],
            read_agent_run_drive_lease,
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_drive_lease_read_failed", error)
        })
}

pub fn renew_agent_run_drive_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    owner_instance_id: &str,
    drive_token: &str,
    heartbeat_at: &str,
) -> Result<bool, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(owner_instance_id, "ownerInstanceId")?;
    validate_non_empty_text(drive_token, "driveToken")?;
    validate_non_empty_text(heartbeat_at, "heartbeatAt")?;
    let connection = open_agent_database(repo_root)?;
    let rows = connection
        .execute(
            r#"
            UPDATE agent_run_drive_leases
            SET acquired_at = ?5
            WHERE project_id = ?1
              AND run_id = ?2
              AND owner_instance_id = ?3
              AND drive_token = ?4
              AND acquired_at <= ?5
              AND EXISTS (
                  SELECT 1
                  FROM agent_runs
                  WHERE agent_runs.project_id = agent_run_drive_leases.project_id
                    AND agent_runs.run_id = agent_run_drive_leases.run_id
                    AND agent_runs.status NOT IN ('cancelling', 'cancelled')
              )
            "#,
            params![
                project_id,
                run_id,
                owner_instance_id,
                drive_token,
                heartbeat_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_drive_lease_renew_failed", error)
        })?;
    Ok(rows == 1)
}

pub fn release_agent_run_drive_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    owner_instance_id: &str,
    drive_token: &str,
) -> Result<bool, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(owner_instance_id, "ownerInstanceId")?;
    validate_non_empty_text(drive_token, "driveToken")?;

    let connection = open_agent_database(repo_root)?;
    let rows = connection
        .execute(
            r#"
            DELETE FROM agent_run_drive_leases
            WHERE project_id = ?1
              AND run_id = ?2
              AND owner_instance_id = ?3
              AND drive_token = ?4
            "#,
            params![project_id, run_id, owner_instance_id, drive_token],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_drive_lease_release_failed", error)
        })?;
    Ok(rows == 1)
}

pub fn cancel_agent_run_with_expected_drive_lease(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    expected_lease: Option<&AgentRunDriveLeaseRecord>,
    defer_to_drive_owner: bool,
    cancellation_event_payload_json: &str,
    timestamp: &str,
) -> Result<AgentRunCancellationCasResult, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(timestamp, "timestamp")?;
    validate_json_payload(
        cancellation_event_payload_json,
        "cancellationEventPayloadJson",
    )?;
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_cancel_transaction_failed", error)
        })?;
    let current_lease = transaction
        .query_row(
            r#"
            SELECT project_id, run_id, owner_instance_id, owner_process_id,
                   owner_process_birth_identity, drive_token, acquired_at
            FROM agent_run_drive_leases
            WHERE project_id = ?1 AND run_id = ?2
            "#,
            params![project_id, run_id],
            read_agent_run_drive_lease,
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_cancel_lease_read_failed", error)
        })?;
    if current_lease.as_ref() != expected_lease {
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_cancel_commit_failed", error)
        })?;
        return Ok(AgentRunCancellationCasResult::LeaseChanged(current_lease));
    }

    let current_status = transaction
        .query_row(
            "SELECT status FROM agent_runs WHERE project_id = ?1 AND run_id = ?2",
            params![project_id, run_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_cancel_status_read_failed", error)
        })?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_run_not_found",
                format!("Owned-agent run `{run_id}` was not found."),
            )
        })?;
    let status = parse_agent_run_status(&current_status);
    let terminal = matches!(
        status,
        AgentRunStatus::Cancelled
            | AgentRunStatus::Completed
            | AgentRunStatus::HandedOff
            | AgentRunStatus::Failed
    );
    let defer_terminal = defer_to_drive_owner && expected_lease.is_some() && !terminal;
    let intent_transitioned = !terminal && status != AgentRunStatus::Cancelling;
    let transitioned = if defer_terminal {
        intent_transitioned
    } else {
        !terminal
    };
    let event = if intent_transitioned {
        transaction
            .execute(
                r#"
                INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
                VALUES (?1, ?2, 'run_failed', ?3, ?4)
                "#,
                params![
                    project_id,
                    run_id,
                    cancellation_event_payload_json,
                    timestamp
                ],
            )
            .map_err(|error| {
                map_agent_store_write_error(repo_root, "agent_run_cancel_event_failed", error)
            })?;
        let event = AgentEventRecord {
            id: transaction.last_insert_rowid(),
            project_id: project_id.to_owned(),
            run_id: run_id.to_owned(),
            event_kind: AgentRunEventKind::RunFailed,
            payload_json: cancellation_event_payload_json.to_owned(),
            created_at: timestamp.to_owned(),
        };
        Some(event)
    } else {
        None
    };
    if defer_terminal {
        transaction
            .execute(
                r#"
                UPDATE agent_runs
                SET status = 'cancelling',
                    last_heartbeat_at = ?3,
                    completed_at = NULL,
                    cancelled_at = NULL,
                    last_error_code = NULL,
                    last_error_message = NULL,
                    updated_at = ?3
                WHERE project_id = ?1
                  AND run_id = ?2
                  AND status NOT IN ('completed', 'handed_off', 'failed', 'cancelled')
                "#,
                params![project_id, run_id, timestamp],
            )
            .map_err(|error| {
                map_agent_store_write_error(repo_root, "agent_run_cancel_status_failed", error)
            })?;
    } else if !terminal {
        transaction
            .execute(
                r#"
                UPDATE agent_runs
                SET status = 'cancelled',
                    last_heartbeat_at = ?3,
                    completed_at = NULL,
                    cancelled_at = ?3,
                    last_error_code = NULL,
                    last_error_message = NULL,
                    updated_at = ?3
                WHERE project_id = ?1
                  AND run_id = ?2
                  AND status NOT IN ('completed', 'handed_off', 'failed', 'cancelled')
                "#,
                params![project_id, run_id, timestamp],
            )
            .map_err(|error| {
                map_agent_store_write_error(repo_root, "agent_run_cancel_status_failed", error)
            })?;
    }
    if !defer_terminal {
        if let Some(lease) = expected_lease {
            transaction
                .execute(
                    r#"
                DELETE FROM agent_run_drive_leases
                WHERE project_id = ?1
                  AND run_id = ?2
                  AND owner_instance_id = ?3
                  AND owner_process_id = ?4
                  AND owner_process_birth_identity = ?5
                  AND drive_token = ?6
                "#,
                    params![
                        project_id,
                        run_id,
                        lease.owner_instance_id,
                        i64::from(lease.owner_process_id),
                        lease.owner_process_birth_identity,
                        lease.drive_token,
                    ],
                )
                .map_err(|error| {
                    map_agent_store_write_error(
                        repo_root,
                        "agent_run_cancel_lease_release_failed",
                        error,
                    )
                })?;
        }
    }
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_run_cancel_commit_failed", error)
    })?;
    let snapshot = read_agent_run_snapshot(&connection, repo_root, project_id, run_id)?;
    Ok(AgentRunCancellationCasResult::Applied {
        snapshot,
        transitioned,
        event,
    })
}

pub fn update_agent_run_status(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    status: AgentRunStatus,
    diagnostic: Option<AgentRunDiagnosticRecord>,
    timestamp: &str,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let status_value = agent_run_status_sql_value(&status);
    let connection = open_agent_database(repo_root)?;
    // Never overwrite a terminal run, except for the monotonic Completed -> HandedOff
    // refinement recorded after an accepted routing switch creates its target run. The first
    // terminal writer otherwise wins, closing cancel/complete races in both directions.
    let rows = connection
        .execute(
            r#"
            UPDATE agent_runs
            SET status = ?3,
                last_heartbeat_at = ?4,
                completed_at = CASE
                    WHEN ?3 IN ('completed', 'handed_off') THEN COALESCE(completed_at, ?4)
                    ELSE NULL
                END,
                cancelled_at = CASE WHEN ?3 = 'cancelled' THEN ?4 ELSE NULL END,
                last_error_code = ?5,
                last_error_message = ?6,
                updated_at = ?4
            WHERE project_id = ?1
              AND run_id = ?2
              AND NOT (status = 'cancelling' AND ?3 <> 'cancelled')
              AND (
                    status NOT IN ('completed', 'handed_off', 'failed', 'cancelled')
                    OR (status = 'completed' AND ?3 = 'handed_off')
              )
            "#,
            params![
                project_id,
                run_id,
                status_value,
                timestamp,
                diagnostic.as_ref().map(|error| error.code.as_str()),
                diagnostic.as_ref().map(|error| error.message.as_str()),
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_status_update_failed", error)
        })?;
    // Zero rows means the requested transition lost to an immutable terminal state (or the run
    // is absent). Return the current snapshot so callers observe the winning terminal state;
    // `read_agent_run_snapshot` surfaces a not-found error if the run truly does not exist.
    let _ = rows;
    read_agent_run_snapshot(&connection, repo_root, project_id, run_id)
}

/// Re-open a previously finished run for an explicit user continuation.
///
/// Generic status updates intentionally keep terminal rows immutable so a late
/// provider/cancellation writer cannot resurrect a run. Continuation is the
/// one deliberate exception: its caller already owns the persisted drive
/// lease, so this compare-and-set transition can safely clear the old terminal
/// metadata before the next provider turn starts.
pub fn reopen_agent_run_for_continuation(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    timestamp: &str,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    let rows = connection
        .execute(
            r#"
            UPDATE agent_runs
            SET status = 'running',
                last_heartbeat_at = ?3,
                completed_at = NULL,
                cancelled_at = NULL,
                last_error_code = NULL,
                last_error_message = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND run_id = ?2
              AND status IN ('running', 'paused', 'completed', 'failed')
            "#,
            params![project_id, run_id, timestamp],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_continuation_reopen_failed", error)
        })?;
    if rows == 0 {
        let snapshot = read_agent_run_snapshot(&connection, repo_root, project_id, run_id)?;
        return Err(CommandError::user_fixable(
            "agent_run_not_resumable",
            format!(
                "Xero cannot continue owned agent run `{run_id}` because it is {:?}.",
                snapshot.run.status
            ),
        ));
    }
    read_agent_run_snapshot(&connection, repo_root, project_id, run_id)
}

pub fn load_agent_continuation_preparation(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    request_id: &str,
    payload_hash: &str,
) -> Result<Option<AgentContinuationRequestRecord>, CommandError> {
    validate_agent_continuation_identity(project_id, run_id, request_id, payload_hash)?;
    let connection = open_agent_database(repo_root)?;
    let existing = read_agent_continuation_request(&connection, project_id, request_id, repo_root)?;
    existing
        .map(|record| {
            ensure_matching_agent_continuation_request(&record, run_id, payload_hash)?;
            Ok(record)
        })
        .transpose()
}

pub fn load_agent_continuation_request_by_id(
    repo_root: &Path,
    project_id: &str,
    request_id: &str,
) -> Result<Option<AgentContinuationRequestRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_continuation_request_id(request_id)?;
    let connection = open_agent_database(repo_root)?;
    read_agent_continuation_request(&connection, project_id, request_id, repo_root)
}

pub fn list_prepared_agent_continuation_requests(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<AgentContinuationRequestRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT project_id, request_id, run_id, payload_hash, recovery_payload_json, state, message_id,
                   linked_path_grant_event_id, message_event_id, prepared_at,
                   drive_started_at, consumed_at
            FROM agent_continuation_requests
            WHERE project_id = ?1 AND state = 'prepared'
            ORDER BY prepared_at ASC, request_id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_continuation_list_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(AgentContinuationRequestRecord {
                project_id: row.get(0)?,
                request_id: row.get(1)?,
                run_id: row.get(2)?,
                payload_hash: row.get(3)?,
                recovery_payload_json: row.get(4)?,
                state: parse_agent_continuation_request_state(row.get::<_, String>(5)?.as_str()),
                message_id: row.get(6)?,
                linked_path_grant_event_id: row.get(7)?,
                message_event_id: row.get(8)?,
                prepared_at: row.get(9)?,
                drive_started_at: row.get(10)?,
                consumed_at: row.get(11)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_continuation_list_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_continuation_list_failed", error)
    })
}

pub fn list_agent_continuation_requests_for_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AgentContinuationRequestRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT project_id, request_id, run_id, payload_hash, recovery_payload_json, state, message_id,
                   linked_path_grant_event_id, message_event_id, prepared_at,
                   drive_started_at, consumed_at
            FROM agent_continuation_requests
            WHERE project_id = ?1 AND run_id = ?2
            ORDER BY prepared_at ASC, request_id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_continuation_run_list_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            Ok(AgentContinuationRequestRecord {
                project_id: row.get(0)?,
                request_id: row.get(1)?,
                run_id: row.get(2)?,
                payload_hash: row.get(3)?,
                recovery_payload_json: row.get(4)?,
                state: parse_agent_continuation_request_state(row.get::<_, String>(5)?.as_str()),
                message_id: row.get(6)?,
                linked_path_grant_event_id: row.get(7)?,
                message_event_id: row.get(8)?,
                prepared_at: row.get(9)?,
                drive_started_at: row.get(10)?,
                consumed_at: row.get(11)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_continuation_run_list_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_continuation_run_list_failed", error)
    })
}

/// Binds an already-persisted initial user turn to a continuation identity.
///
/// Runtime-run startup persists the initial message before provider dispatch. Registering that
/// existing message prevents crash recovery from appending the same initial prompt as a second
/// continuation.
pub fn register_existing_agent_continuation(
    repo_root: &Path,
    record: &ExistingAgentContinuationPreparationRecord,
) -> Result<AgentContinuationPreparationResult, CommandError> {
    validate_agent_continuation_identity(
        &record.project_id,
        &record.run_id,
        &record.request_id,
        &record.payload_hash,
    )?;
    validate_non_empty_text(&record.prepared_at, "preparedAt")?;
    validate_json_payload(&record.recovery_payload_json, "recoveryPayloadJson")?;
    if record.message_id <= 0 || record.message_event_id <= 0 {
        return Err(CommandError::invalid_request("existingContinuationTurn"));
    }
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_transaction_failed", error)
        })?;
    if let Some(existing) = read_agent_continuation_request(
        &transaction,
        &record.project_id,
        &record.request_id,
        repo_root,
    )? {
        ensure_matching_agent_continuation_request(
            &existing,
            &record.run_id,
            &record.payload_hash,
        )?;
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_commit_failed", error)
        })?;
        let snapshot =
            read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)?;
        return Ok(AgentContinuationPreparationResult {
            snapshot,
            request: existing,
            inserted_events: Vec::new(),
            inserted: false,
        });
    }
    let message_exists = transaction
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM agent_messages
                WHERE id = ?1 AND project_id = ?2 AND run_id = ?3 AND role = 'user'
            )
            "#,
            params![record.message_id, record.project_id, record.run_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_continuation_message_read_failed", error)
        })?
        == 1;
    let event_exists = transaction
        .query_row(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM agent_events
                WHERE id = ?1 AND project_id = ?2 AND run_id = ?3 AND event_kind = 'message_delta'
            )
            "#,
            params![record.message_event_id, record.project_id, record.run_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_continuation_event_read_failed", error)
        })?
        == 1;
    if !message_exists || !event_exists {
        return Err(CommandError::system_fault(
            "agent_continuation_existing_turn_missing",
            "Xero could not bind the runtime-start continuation because its initial user turn is incomplete.",
        ));
    }
    transaction
        .execute(
            r#"
            INSERT INTO agent_continuation_requests (
                project_id, request_id, run_id, payload_hash, recovery_payload_json, state, message_id,
                linked_path_grant_event_id, message_event_id, prepared_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'prepared', ?6, NULL, ?7, ?8)
            "#,
            params![
                record.project_id,
                record.request_id,
                record.run_id,
                record.payload_hash,
                record.recovery_payload_json,
                record.message_id,
                record.message_event_id,
                record.prepared_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(
                repo_root,
                "agent_continuation_request_insert_failed",
                error,
            )
        })?;
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_continuation_commit_failed", error)
    })?;
    let snapshot =
        read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)?;
    let request =
        load_agent_continuation_request_by_id(repo_root, &record.project_id, &record.request_id)?
            .ok_or_else(|| {
            CommandError::system_fault(
                "agent_continuation_request_missing",
                "The registered runtime-start continuation could not be read back.",
            )
        })?;
    Ok(AgentContinuationPreparationResult {
        snapshot,
        request,
        inserted_events: Vec::new(),
        inserted: true,
    })
}

/// Atomically records a continuation prompt and re-opens its run.
///
/// The request id is project-wide. Reusing it with the same run and payload is a no-op; reusing
/// it for different content is rejected. The message, attachments, linked-path grant, message
/// event, request marker, and terminal-to-running transition commit together.
pub fn prepare_agent_continuation(
    repo_root: &Path,
    record: &NewAgentContinuationPreparationRecord,
) -> Result<AgentContinuationPreparationResult, CommandError> {
    validate_agent_continuation_identity(
        &record.project_id,
        &record.run_id,
        &record.request_id,
        &record.payload_hash,
    )?;
    validate_non_empty_text(&record.content, "content")?;
    validate_non_empty_text(&record.prepared_at, "preparedAt")?;
    validate_json_payload(&record.recovery_payload_json, "recoveryPayloadJson")?;
    validate_json_payload(&record.message_payload_json, "messagePayloadJson")?;
    if let Some(payload_json) = record.linked_path_grant_payload_json.as_deref() {
        validate_json_payload(payload_json, "linkedPathGrantPayloadJson")?;
    }
    if let Some(answer) = record.action_answer.as_ref() {
        validate_non_empty_text(&answer.response, "actionAnswer.response")?;
        if let Some(action_id) = answer.action_id.as_deref() {
            validate_non_empty_text(action_id, "actionAnswer.actionId")?;
        }
    }
    if !matches!(
        record.role,
        AgentMessageRole::User | AgentMessageRole::Developer
    ) {
        return Err(CommandError::invalid_request("role"));
    }

    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_transaction_failed", error)
        })?;

    if let Some(existing) = read_agent_continuation_request(
        &transaction,
        &record.project_id,
        &record.request_id,
        repo_root,
    )? {
        ensure_matching_agent_continuation_request(
            &existing,
            &record.run_id,
            &record.payload_hash,
        )?;
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_commit_failed", error)
        })?;
        let snapshot =
            read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)?;
        return Ok(AgentContinuationPreparationResult {
            snapshot,
            request: existing,
            inserted_events: Vec::new(),
            inserted: false,
        });
    }

    let mut inserted_events = Vec::new();
    if let Some(answer) = record.action_answer.as_ref() {
        let actions = read_agent_action_requests(
            &transaction,
            &record.project_id,
            &record.run_id,
            repo_root,
        )?;
        let pending = actions
            .into_iter()
            .filter(|action| {
                action.status == "pending"
                    && answer
                        .action_id
                        .as_deref()
                        .map(|action_id| action.action_id == action_id)
                        .unwrap_or(true)
            })
            .collect::<Vec<_>>();
        if let Some(action_id) = answer.action_id.as_deref() {
            if pending.is_empty() {
                return Err(CommandError::user_fixable(
                    "agent_action_request_not_found",
                    format!(
                        "Xero could not find pending owned-agent action `{action_id}` for run `{}`.",
                        record.run_id
                    ),
                ));
            }
        }
        for action in pending {
            let changed = transaction
                .execute(
                    r#"
                    UPDATE agent_action_requests
                    SET status = 'answered', resolved_at = ?4, response = ?5
                    WHERE project_id = ?1 AND run_id = ?2 AND action_id = ?3 AND status = 'pending'
                    "#,
                    params![
                        record.project_id,
                        record.run_id,
                        action.action_id,
                        record.prepared_at,
                        answer.response,
                    ],
                )
                .map_err(|error| {
                    map_agent_store_write_error(
                        repo_root,
                        "agent_continuation_action_answer_failed",
                        error,
                    )
                })?;
            if changed != 1 {
                return Err(CommandError::retryable(
                    "agent_continuation_action_answer_raced",
                    format!(
                        "Owned-agent action `{}` changed while its continuation was being prepared.",
                        action.action_id
                    ),
                ));
            }
            let payload = serde_json::to_string(&serde_json::json!({
                "kind": "approval_decision",
                "actionId": action.action_id,
                "actionType": action.action_type,
                "decision": "approved",
                "response": answer.response,
                "status": "answered",
            }))
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_continuation_action_event_serialize_failed",
                    format!("Xero could not encode an action approval event: {error}"),
                )
            })?;
            inserted_events.push(insert_agent_event_in_transaction(
                &transaction,
                &record.project_id,
                &record.run_id,
                AgentRunEventKind::PolicyDecision,
                &payload,
                &record.prepared_at,
                repo_root,
            )?);
        }
    }

    transaction
        .execute(
            r#"
            INSERT INTO agent_messages (
                project_id,
                run_id,
                role,
                content,
                provider_metadata_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, NULL, ?5)
            "#,
            params![
                record.project_id,
                record.run_id,
                agent_message_role_sql_value(&record.role),
                record.content,
                record.prepared_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(
                repo_root,
                "agent_continuation_message_insert_failed",
                error,
            )
        })?;
    let message_id = transaction.last_insert_rowid();

    for attachment in &record.attachments {
        validate_non_empty_text(&attachment.storage_path, "attachment.storagePath")?;
        validate_non_empty_text(&attachment.media_type, "attachment.mediaType")?;
        validate_non_empty_text(&attachment.original_name, "attachment.originalName")?;
        if attachment.size_bytes < 0 {
            return Err(CommandError::user_fixable(
                "agent_message_attachment_invalid_size",
                "Xero refused to record an attachment with a negative size.",
            ));
        }
        transaction
            .execute(
                r#"
                INSERT INTO agent_message_attachments (
                    message_id,
                    project_id,
                    run_id,
                    kind,
                    storage_path,
                    media_type,
                    original_name,
                    size_bytes,
                    width,
                    height,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                "#,
                params![
                    message_id,
                    record.project_id,
                    record.run_id,
                    agent_message_attachment_kind_sql_value(&attachment.kind),
                    attachment.storage_path,
                    attachment.media_type,
                    attachment.original_name,
                    attachment.size_bytes,
                    attachment.width,
                    attachment.height,
                    record.prepared_at,
                ],
            )
            .map_err(|error| {
                map_agent_store_write_error(
                    repo_root,
                    "agent_continuation_attachment_insert_failed",
                    error,
                )
            })?;
    }

    let linked_path_grant_event_id =
        if let Some(payload_json) = record.linked_path_grant_payload_json.as_deref() {
            let event = insert_agent_event_in_transaction(
                &transaction,
                &record.project_id,
                &record.run_id,
                AgentRunEventKind::ToolPermissionGrant,
                payload_json,
                &record.prepared_at,
                repo_root,
            )?;
            let event_id = event.id;
            inserted_events.push(event);
            Some(event_id)
        } else {
            None
        };
    let message_event = insert_agent_event_in_transaction(
        &transaction,
        &record.project_id,
        &record.run_id,
        AgentRunEventKind::MessageDelta,
        &record.message_payload_json,
        &record.prepared_at,
        repo_root,
    )?;
    let message_event_id = message_event.id;
    inserted_events.push(message_event);

    let reopened = transaction
        .execute(
            r#"
            UPDATE agent_runs
            SET status = 'running',
                last_heartbeat_at = ?3,
                completed_at = NULL,
                cancelled_at = NULL,
                last_error_code = NULL,
                last_error_message = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND run_id = ?2
              AND status IN ('running', 'paused', 'completed', 'failed')
            "#,
            params![record.project_id, record.run_id, record.prepared_at],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_reopen_failed", error)
        })?;
    if reopened != 1 {
        let snapshot =
            read_agent_run_snapshot(&transaction, repo_root, &record.project_id, &record.run_id)?;
        return Err(CommandError::user_fixable(
            "agent_run_not_resumable",
            format!(
                "Xero cannot continue owned agent run `{}` because it is {:?}.",
                record.run_id, snapshot.run.status
            ),
        ));
    }

    transaction
        .execute(
            r#"
            INSERT INTO agent_continuation_requests (
                project_id,
                request_id,
                run_id,
                payload_hash,
                recovery_payload_json,
                state,
                message_id,
                linked_path_grant_event_id,
                message_event_id,
                prepared_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'prepared', ?6, ?7, ?8, ?9)
            "#,
            params![
                record.project_id,
                record.request_id,
                record.run_id,
                record.payload_hash,
                record.recovery_payload_json,
                message_id,
                linked_path_grant_event_id,
                message_event_id,
                record.prepared_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(
                repo_root,
                "agent_continuation_request_insert_failed",
                error,
            )
        })?;

    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_continuation_commit_failed", error)
    })?;
    let snapshot =
        read_agent_run_snapshot(&connection, repo_root, &record.project_id, &record.run_id)?;
    Ok(AgentContinuationPreparationResult {
        snapshot,
        request: AgentContinuationRequestRecord {
            project_id: record.project_id.clone(),
            request_id: record.request_id.clone(),
            run_id: record.run_id.clone(),
            payload_hash: record.payload_hash.clone(),
            recovery_payload_json: record.recovery_payload_json.clone(),
            state: AgentContinuationRequestState::Prepared,
            message_id,
            linked_path_grant_event_id,
            message_event_id,
            prepared_at: record.prepared_at.clone(),
            drive_started_at: None,
            consumed_at: None,
        },
        inserted_events,
        inserted: true,
    })
}

pub fn mark_agent_continuation_drive_started(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    request_id: &str,
    timestamp: &str,
) -> Result<AgentContinuationDriveStartResult, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_continuation_request_id(request_id)?;
    validate_non_empty_text(timestamp, "timestamp")?;
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_drive_start_failed", error)
        })?;
    let existing =
        read_agent_continuation_request(&transaction, project_id, request_id, repo_root)?;
    let outcome = match existing {
        None => AgentContinuationDriveStartResult::Missing,
        Some(existing) => {
            ensure_matching_agent_continuation_request(&existing, run_id, &existing.payload_hash)?;
            match existing.state {
                AgentContinuationRequestState::Prepared => {
                    let updated = transaction
                        .execute(
                            r#"
                            UPDATE agent_continuation_requests
                            SET state = 'driving',
                                drive_started_at = ?3,
                                consumed_at = NULL
                            WHERE project_id = ?1
                              AND request_id = ?2
                              AND state = 'prepared'
                            "#,
                            params![project_id, request_id, timestamp],
                        )
                        .map_err(|error| {
                            map_agent_store_write_error(
                                repo_root,
                                "agent_continuation_drive_start_failed",
                                error,
                            )
                        })?;
                    if updated != 1 {
                        return Err(CommandError::retryable(
                            "agent_continuation_drive_start_raced",
                            "The continuation drive state changed while Xero was claiming provider dispatch.",
                        ));
                    }
                    let started = read_agent_continuation_request(
                        &transaction,
                        project_id,
                        request_id,
                        repo_root,
                    )?
                    .ok_or_else(|| {
                        CommandError::system_fault(
                            "agent_continuation_drive_start_missing",
                            "The continuation request disappeared while Xero was claiming provider dispatch.",
                        )
                    })?;
                    AgentContinuationDriveStartResult::Started(started)
                }
                AgentContinuationRequestState::Driving => {
                    AgentContinuationDriveStartResult::AlreadyDriving(existing)
                }
                AgentContinuationRequestState::Consumed => {
                    AgentContinuationDriveStartResult::Consumed(existing)
                }
            }
        }
    };
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_continuation_drive_start_failed", error)
    })?;
    Ok(outcome)
}

pub fn finish_agent_continuation_drive(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    request_id: &str,
    consumed: bool,
    timestamp: &str,
) -> Result<Option<AgentContinuationRequestRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_continuation_request_id(request_id)?;
    validate_non_empty_text(timestamp, "timestamp")?;
    let connection = open_agent_database(repo_root)?;
    let Some(existing) =
        read_agent_continuation_request(&connection, project_id, request_id, repo_root)?
    else {
        return Ok(None);
    };
    ensure_matching_agent_continuation_request(&existing, run_id, &existing.payload_hash)?;
    if consumed {
        connection
            .execute(
                r#"
                UPDATE agent_continuation_requests
                SET state = 'consumed',
                    consumed_at = COALESCE(consumed_at, ?3)
                WHERE project_id = ?1
                  AND request_id = ?2
                "#,
                params![project_id, request_id, timestamp],
            )
            .map_err(|error| {
                map_agent_store_write_error(repo_root, "agent_continuation_consume_failed", error)
            })?;
    }
    read_agent_continuation_request(&connection, project_id, request_id, repo_root)
}

/// Reconciles the narrow crash window after a continuation completed durably but before its
/// request marker was advanced from `driving` to `consumed`.
///
/// Consumption is considered unambiguous when a provider-originated event was committed after
/// this request's user-message event. This covers terminal completion as well as paused and
/// action-required turns. Failed and cancelled runs remain `driving` even if an earlier provider
/// event exists: a terminal failure is not evidence that this continuation reached a successful
/// turn boundary, and replay must stay fenced for explicit recovery.
pub fn reconcile_completed_agent_continuation(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    request_id: &str,
    timestamp: &str,
) -> Result<Option<AgentContinuationRequestRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_continuation_request_id(request_id)?;
    validate_non_empty_text(timestamp, "timestamp")?;

    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| {
            map_agent_store_write_error(
                repo_root,
                "agent_continuation_reconcile_transaction_failed",
                error,
            )
        })?;
    let Some(existing) =
        read_agent_continuation_request(&transaction, project_id, request_id, repo_root)?
    else {
        transaction.commit().map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_reconcile_failed", error)
        })?;
        return Ok(None);
    };
    ensure_matching_agent_continuation_request(&existing, run_id, &existing.payload_hash)?;

    if existing.state == AgentContinuationRequestState::Driving {
        let provider_outcome_after_request = transaction
            .query_row(
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM agent_events AS event
                    WHERE event.project_id = ?1
                      AND event.run_id = ?2
                      AND event.id > ?3
                      AND (
                            event.event_kind IN (
                                'assistant_candidate',
                                'reasoning_summary',
                                'tool_started',
                                'tool_delta',
                                'tool_completed',
                                'action_required',
                                'approval_required',
                                'route_requested',
                                'run_paused',
                                'run_completed'
                            )
                            OR (
                                event.event_kind = 'message_delta'
                                AND json_extract(event.payload_json, '$.role') = 'assistant'
                            )
                      )
                ) AND EXISTS (
                    SELECT 1
                    FROM agent_runs AS run
                    WHERE run.project_id = ?1
                      AND run.run_id = ?2
                      AND run.status NOT IN ('failed', 'cancelled')
                )
                "#,
                params![project_id, run_id, existing.message_event_id],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| {
                map_agent_store_query_error(
                    repo_root,
                    "agent_continuation_completion_evidence_read_failed",
                    error,
                )
            })?
            == 1;
        if provider_outcome_after_request {
            transaction
                .execute(
                    r#"
                    UPDATE agent_continuation_requests
                    SET state = 'consumed',
                        consumed_at = COALESCE(consumed_at, ?4)
                    WHERE project_id = ?1
                      AND run_id = ?2
                      AND request_id = ?3
                      AND state = 'driving'
                    "#,
                    params![project_id, run_id, request_id, timestamp],
                )
                .map_err(|error| {
                    map_agent_store_write_error(
                        repo_root,
                        "agent_continuation_reconcile_failed",
                        error,
                    )
                })?;
        }
    }

    let reconciled =
        read_agent_continuation_request(&transaction, project_id, request_id, repo_root)?;
    transaction.commit().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_continuation_reconcile_failed", error)
    })?;
    Ok(reconciled)
}

pub fn touch_agent_run_heartbeat(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    timestamp: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_runs
            SET last_heartbeat_at = ?3,
                updated_at = ?3
            WHERE project_id = ?1
              AND run_id = ?2
              AND status IN ('starting', 'running', 'cancelling')
            "#,
            params![project_id, run_id, timestamp],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_run_heartbeat_update_failed", error)
        })?;
    Ok(())
}

pub fn load_agent_run_record(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<AgentRunRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_run_record(&connection, repo_root, project_id, run_id)
}

/// Count messages in `run_id` whose id is greater than `after_message_id`. Used by the
/// context policy to decide whether an active compaction still protects the turn: once the
/// uncovered raw tail regrows past the intended window, the compaction is stale.
pub fn count_agent_run_messages_after_id(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    after_message_id: i64,
) -> Result<u64, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    let count: i64 = connection
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM agent_messages
            WHERE project_id = ?1
              AND run_id = ?2
              AND id > ?3
            "#,
            params![project_id, run_id, after_message_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_message_count_failed", error)
        })?;
    Ok(count.max(0) as u64)
}

/// The highest child event id already forwarded into `parent_run_id` for `subagent_id`.
/// Forwarded events embed their source child event id in `payload_json.sourceChildEventId`;
/// this lets `forward_child_events_to_parent` skip events it already mirrored instead of
/// re-appending the child's entire event history on every interaction.
pub fn max_forwarded_child_event_id(
    repo_root: &Path,
    project_id: &str,
    parent_run_id: &str,
    subagent_id: &str,
) -> Result<Option<i64>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(parent_run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    let watermark: Option<i64> = connection
        .query_row(
            r#"
            SELECT MAX(CAST(json_extract(payload_json, '$.sourceChildEventId') AS INTEGER))
            FROM agent_events
            WHERE project_id = ?1
              AND run_id = ?2
              AND json_extract(payload_json, '$.subagentId') = ?3
              AND json_extract(payload_json, '$.sourceChildEventId') IS NOT NULL
            "#,
            params![project_id, parent_run_id, subagent_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_forwarded_event_watermark_failed", error)
        })?;
    Ok(watermark)
}

/// Whether an active compaction still protects the current turn. A compaction is "current"
/// only while the raw tail beyond its coverage boundary stays within the intended
/// `raw_tail_message_count`; once new turns push the tail past that window (or the compaction
/// recorded no coverage boundary), it is stale and the context policy should recompact.
pub fn agent_compaction_is_current(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    compaction: &super::AgentCompactionRecord,
) -> Result<bool, CommandError> {
    match compaction.covered_message_end_id {
        Some(end_id) => {
            let tail_beyond_coverage =
                count_agent_run_messages_after_id(repo_root, project_id, run_id, end_id)?;
            Ok(tail_beyond_coverage <= u64::from(compaction.raw_tail_message_count))
        }
        None => Ok(false),
    }
}

fn read_agent_run_record(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<AgentRunRecord, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                lineage_kind,
                parent_run_id,
                parent_trace_id,
                parent_subagent_id,
                subagent_role,
                provider_id,
                model_id,
                status,
                prompt,
                system_prompt,
                started_at,
                last_heartbeat_at,
                completed_at,
                cancelled_at,
                last_error_code,
                last_error_message,
                updated_at
            FROM agent_runs
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id],
            read_agent_run_row,
        )
        .optional()
        .map_err(|error| map_agent_store_query_error(repo_root, "agent_run_read_failed", error))?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_run_not_found",
                format!("Xero could not find owned agent run `{run_id}`."),
            )
        })
}

pub fn load_agent_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_run_snapshot(&connection, repo_root, project_id, run_id)
}

pub fn load_agent_run_with_usage(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<(AgentRunSnapshotRecord, Option<AgentUsageRecord>), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_run_snapshot_with_usage(&connection, repo_root, project_id, run_id)
}

pub(crate) fn read_agent_run_snapshot_with_usage(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<(AgentRunSnapshotRecord, Option<AgentUsageRecord>), CommandError> {
    let snapshot = read_agent_run_snapshot(connection, repo_root, project_id, run_id)?;
    let usage = read_agent_usage(connection, repo_root, project_id, run_id)?;
    Ok((snapshot, usage))
}

pub(crate) fn read_agent_run_snapshot(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    let run = read_agent_run_record(connection, repo_root, project_id, run_id)?;
    let messages = read_agent_messages(connection, project_id, run_id, repo_root)?;
    let events = read_agent_events(connection, project_id, run_id, repo_root)?;
    let tool_calls = read_agent_tool_calls(connection, project_id, run_id, repo_root)?;
    let file_changes = read_agent_file_changes(connection, project_id, run_id, repo_root)?;
    let checkpoints = read_agent_checkpoints(connection, project_id, run_id, repo_root)?;
    let action_requests = read_agent_action_requests(connection, project_id, run_id, repo_root)?;

    Ok(AgentRunSnapshotRecord {
        run,
        messages,
        events,
        tool_calls,
        file_changes,
        checkpoints,
        action_requests,
    })
}

fn read_agent_continuation_request(
    connection: &Connection,
    project_id: &str,
    request_id: &str,
    repo_root: &Path,
) -> Result<Option<AgentContinuationRequestRecord>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                request_id,
                run_id,
                payload_hash,
                recovery_payload_json,
                state,
                message_id,
                linked_path_grant_event_id,
                message_event_id,
                prepared_at,
                drive_started_at,
                consumed_at
            FROM agent_continuation_requests
            WHERE project_id = ?1
              AND request_id = ?2
            "#,
            params![project_id, request_id],
            |row| {
                Ok(AgentContinuationRequestRecord {
                    project_id: row.get(0)?,
                    request_id: row.get(1)?,
                    run_id: row.get(2)?,
                    payload_hash: row.get(3)?,
                    recovery_payload_json: row.get(4)?,
                    state: parse_agent_continuation_request_state(
                        row.get::<_, String>(5)?.as_str(),
                    ),
                    message_id: row.get(6)?,
                    linked_path_grant_event_id: row.get(7)?,
                    message_event_id: row.get(8)?,
                    prepared_at: row.get(9)?,
                    drive_started_at: row.get(10)?,
                    consumed_at: row.get(11)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_continuation_request_read_failed", error)
        })
}

fn read_agent_run_start_request(
    connection: &Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Option<AgentRunStartRequestRecord>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT project_id, run_id, payload_hash, recovery_payload_json, state, owner_process_id,
                   owner_process_birth_identity, created_at, ready_at, failed_at
            FROM agent_run_start_requests
            WHERE project_id = ?1 AND run_id = ?2
            "#,
            params![project_id, run_id],
            |row| {
                let state = match row.get::<_, String>(4)?.as_str() {
                    "preparing" => AgentRunStartRequestState::Preparing,
                    "ready" => AgentRunStartRequestState::Ready,
                    _ => AgentRunStartRequestState::Failed,
                };
                Ok(AgentRunStartRequestRecord {
                    project_id: row.get(0)?,
                    run_id: row.get(1)?,
                    payload_hash: row.get(2)?,
                    recovery_payload_json: row.get(3)?,
                    state,
                    owner_process_id: read_positive_u32(row, 5)?,
                    owner_process_birth_identity: row.get(6)?,
                    created_at: row.get(7)?,
                    ready_at: row.get(8)?,
                    failed_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_run_start_identity_read_failed", error)
        })
}

fn ensure_matching_agent_continuation_request(
    existing: &AgentContinuationRequestRecord,
    run_id: &str,
    payload_hash: &str,
) -> Result<(), CommandError> {
    if existing.run_id == run_id && existing.payload_hash == payload_hash {
        return Ok(());
    }
    Err(CommandError::user_fixable(
        "agent_continuation_request_conflict",
        format!(
            "Continuation request `{}` was already used with different run input. Retry with its original payload or submit a new request id.",
            existing.request_id
        ),
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "transactional event insertion requires the complete durable event identity"
)]
fn insert_agent_event_in_transaction(
    transaction: &rusqlite::Transaction<'_>,
    project_id: &str,
    run_id: &str,
    event_kind: AgentRunEventKind,
    payload_json: &str,
    created_at: &str,
    repo_root: &Path,
) -> Result<AgentEventRecord, CommandError> {
    transaction
        .execute(
            r#"
            INSERT INTO agent_events (project_id, run_id, event_kind, payload_json, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                project_id,
                run_id,
                agent_event_kind_sql_value(&event_kind),
                payload_json,
                created_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_continuation_event_insert_failed", error)
        })?;
    Ok(AgentEventRecord {
        id: transaction.last_insert_rowid(),
        project_id: project_id.into(),
        run_id: run_id.into(),
        event_kind,
        payload_json: payload_json.into(),
        created_at: created_at.into(),
    })
}

pub(crate) fn load_agent_file_changes(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AgentFileChangeRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_file_changes(&connection, project_id, run_id, repo_root)
}

pub fn load_agent_usage(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AgentUsageRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    read_agent_usage(&connection, repo_root, project_id, run_id)
}

fn read_agent_usage(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Option<AgentUsageRecord>, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                run_id,
                agent_definition_id,
                agent_definition_version,
                provider_id,
                model_id,
                input_tokens,
                billable_input_tokens,
                output_tokens,
                total_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                estimated_cost_micros,
                updated_at
            FROM agent_usage
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id],
            read_agent_usage_row,
        )
        .optional()
        .map_err(|error| map_agent_store_query_error(repo_root, "agent_usage_read_failed", error))
}

pub fn upsert_agent_subagent_task(
    repo_root: &Path,
    record: &AgentSubagentTaskRecord,
) -> Result<(), CommandError> {
    validate_agent_subagent_task(record)?;
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_subagent_tasks (
                project_id,
                parent_run_id,
                subagent_id,
                role,
                role_label,
                prompt_hash,
                prompt_preview,
                model_id,
                write_set_json,
                workflow_structure_json,
                verification_contract,
                depth,
                max_tool_calls,
                max_tokens,
                max_cost_micros,
                used_tool_calls,
                used_tokens,
                used_cost_micros,
                budget_status,
                budget_diagnostic_json,
                status,
                created_at,
                started_at,
                completed_at,
                cancelled_at,
                integrated_at,
                child_run_id,
                child_trace_id,
                parent_trace_id,
                input_log_json,
                result_summary,
                result_artifact,
                parent_decision,
                latest_summary,
                updated_at
            )
            VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20,
                ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30,
                ?31, ?32, ?33, ?34, ?35
            )
            ON CONFLICT(project_id, parent_run_id, subagent_id) DO UPDATE SET
                role = excluded.role,
                role_label = excluded.role_label,
                prompt_hash = excluded.prompt_hash,
                prompt_preview = excluded.prompt_preview,
                model_id = excluded.model_id,
                write_set_json = excluded.write_set_json,
                workflow_structure_json = excluded.workflow_structure_json,
                verification_contract = excluded.verification_contract,
                depth = excluded.depth,
                max_tool_calls = excluded.max_tool_calls,
                max_tokens = excluded.max_tokens,
                max_cost_micros = excluded.max_cost_micros,
                used_tool_calls = excluded.used_tool_calls,
                used_tokens = excluded.used_tokens,
                used_cost_micros = excluded.used_cost_micros,
                budget_status = excluded.budget_status,
                budget_diagnostic_json = excluded.budget_diagnostic_json,
                status = excluded.status,
                started_at = excluded.started_at,
                completed_at = excluded.completed_at,
                cancelled_at = excluded.cancelled_at,
                integrated_at = excluded.integrated_at,
                child_run_id = excluded.child_run_id,
                child_trace_id = excluded.child_trace_id,
                parent_trace_id = excluded.parent_trace_id,
                input_log_json = excluded.input_log_json,
                result_summary = excluded.result_summary,
                result_artifact = excluded.result_artifact,
                parent_decision = excluded.parent_decision,
                latest_summary = excluded.latest_summary,
                updated_at = excluded.updated_at
            "#,
            params![
                record.project_id,
                record.parent_run_id,
                record.subagent_id,
                record.role,
                record.role_label,
                record.prompt_hash,
                record.prompt_preview,
                record.model_id,
                record.write_set_json,
                record.workflow_structure_json,
                record.verification_contract,
                record.depth,
                record.max_tool_calls,
                record.max_tokens,
                record.max_cost_micros,
                record.used_tool_calls,
                record.used_tokens,
                record.used_cost_micros,
                record.budget_status,
                record.budget_diagnostic_json,
                record.status,
                record.created_at,
                record.started_at,
                record.completed_at,
                record.cancelled_at,
                record.integrated_at,
                record.child_run_id,
                record.child_trace_id,
                record.parent_trace_id,
                record.input_log_json,
                record.result_summary,
                record.result_artifact,
                record.parent_decision,
                record.latest_summary,
                record.updated_at,
            ],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_subagent_task_upsert_failed", error)
        })?;
    Ok(())
}

pub fn load_agent_subagent_task(
    repo_root: &Path,
    project_id: &str,
    parent_run_id: &str,
    subagent_id: &str,
) -> Result<Option<AgentSubagentTaskRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(parent_run_id, "parentRunId")?;
    validate_non_empty_text(subagent_id, "subagentId")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .query_row(
            agent_subagent_task_select_sql(
                "WHERE project_id = ?1 AND parent_run_id = ?2 AND subagent_id = ?3",
            )
            .as_str(),
            params![project_id, parent_run_id, subagent_id],
            read_agent_subagent_task_row,
        )
        .optional()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_subagent_task_read_failed", error)
        })
}

pub fn list_agent_subagent_tasks_for_parent(
    repo_root: &Path,
    project_id: &str,
    parent_run_id: &str,
) -> Result<Vec<AgentSubagentTaskRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(parent_run_id, "parentRunId")?;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            agent_subagent_task_select_sql(
                "WHERE project_id = ?1 AND parent_run_id = ?2 ORDER BY created_at ASC, subagent_id ASC",
            )
            .as_str(),
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_subagent_tasks_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(
            params![project_id, parent_run_id],
            read_agent_subagent_task_row,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_subagent_tasks_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_subagent_tasks_decode_failed", error)
    })
}

/// Aggregate token + cost totals across every agent run for one project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectUsageTotalsRecord {
    pub run_count: u64,
    pub input_tokens: u64,
    pub billable_input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub estimated_cost_micros: u64,
    pub last_updated_at: Option<String>,
}

/// One row of the per-(provider, model) breakdown for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectUsageModelBreakdownRecord {
    pub provider_id: String,
    pub model_id: String,
    pub run_count: u64,
    pub input_tokens: u64,
    pub billable_input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub estimated_cost_micros: u64,
    pub last_updated_at: Option<String>,
}

/// Sum every run for a project into a single totals row. Returns zeroed
/// totals when no runs exist yet.
pub fn project_usage_totals(
    repo_root: &Path,
    project_id: &str,
) -> Result<ProjectUsageTotalsRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    let connection = open_agent_database(repo_root)?;
    read_project_usage_totals(&connection, repo_root, project_id)
}

fn read_project_usage_totals(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
) -> Result<ProjectUsageTotalsRecord, CommandError> {
    connection
        .query_row(
            r#"
            SELECT
                COUNT(*) AS run_count,
                COALESCE(SUM(input_tokens), 0) AS input_tokens,
                COALESCE(SUM(billable_input_tokens), 0) AS billable_input_tokens,
                COALESCE(SUM(output_tokens), 0) AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                COALESCE(SUM(cache_read_tokens), 0) AS cache_read_tokens,
                COALESCE(SUM(cache_creation_tokens), 0) AS cache_creation_tokens,
                COALESCE(SUM(estimated_cost_micros), 0) AS estimated_cost_micros,
                MAX(updated_at) AS last_updated_at
            FROM agent_usage
            WHERE project_id = ?1
            "#,
            params![project_id],
            |row| {
                Ok(ProjectUsageTotalsRecord {
                    run_count: read_nonnegative_u64(row, 0)?,
                    input_tokens: read_nonnegative_u64(row, 1)?,
                    billable_input_tokens: read_nonnegative_u64(row, 2)?,
                    output_tokens: read_nonnegative_u64(row, 3)?,
                    total_tokens: read_nonnegative_u64(row, 4)?,
                    cache_read_tokens: read_nonnegative_u64(row, 5)?,
                    cache_creation_tokens: read_nonnegative_u64(row, 6)?,
                    estimated_cost_micros: read_nonnegative_u64(row, 7)?,
                    last_updated_at: row.get::<_, Option<String>>(8)?,
                })
            },
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_usage_totals_read_failed", error)
        })
}

/// One row for cost backfill: identity + token counts so a caller (the
/// runtime pricing module) can compute and write back `estimated_cost_micros`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentUsageCostBackfillRow {
    pub project_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub billable_input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub estimated_cost_micros: u64,
}

/// List token-usage rows that can have `estimated_cost_micros` recomputed from
/// the current pricing catalog.
pub fn list_agent_usage_cost_rows(
    repo_root: &Path,
) -> Result<Vec<AgentUsageCostBackfillRow>, CommandError> {
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                run_id,
                provider_id,
                model_id,
                billable_input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                estimated_cost_micros
            FROM agent_usage
            WHERE total_tokens > 0
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_usage_backfill_prepare_failed", error)
        })?;
    let rows = statement
        .query_map([], |row| {
            Ok(AgentUsageCostBackfillRow {
                project_id: row.get(0)?,
                run_id: row.get(1)?,
                provider_id: row.get(2)?,
                model_id: row.get(3)?,
                billable_input_tokens: read_nonnegative_u64(row, 4)?,
                output_tokens: read_nonnegative_u64(row, 5)?,
                cache_read_tokens: read_nonnegative_u64(row, 6)?,
                cache_creation_tokens: read_nonnegative_u64(row, 7)?,
                estimated_cost_micros: read_nonnegative_u64(row, 8)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_usage_backfill_query_failed", error)
        })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_usage_backfill_decode_failed", error)
        })?);
    }
    Ok(out)
}

/// Update only `estimated_cost_micros` for one row.
pub fn update_agent_usage_cost(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    estimated_cost_micros: u64,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_usage
            SET estimated_cost_micros = ?3
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id, estimated_cost_micros],
        )
        .map_err(|error| {
            map_agent_store_write_error(repo_root, "agent_usage_cost_update_failed", error)
        })?;
    Ok(())
}

/// Per-(provider, model) breakdown for a project, sorted by spend descending.
pub fn project_usage_breakdown(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<ProjectUsageModelBreakdownRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    let connection = open_agent_database(repo_root)?;
    read_project_usage_breakdown(&connection, repo_root, project_id)
}

pub fn project_usage_summary(
    repo_root: &Path,
    project_id: &str,
) -> Result<
    (
        ProjectUsageTotalsRecord,
        Vec<ProjectUsageModelBreakdownRecord>,
    ),
    CommandError,
> {
    validate_non_empty_text(project_id, "projectId")?;
    let connection = open_agent_database(repo_root)?;
    let totals = read_project_usage_totals(&connection, repo_root, project_id)?;
    let breakdown = read_project_usage_breakdown(&connection, repo_root, project_id)?;
    Ok((totals, breakdown))
}

fn read_project_usage_breakdown(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<ProjectUsageModelBreakdownRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                provider_id,
                model_id,
                COUNT(*) AS run_count,
                COALESCE(SUM(input_tokens), 0) AS input_tokens,
                COALESCE(SUM(billable_input_tokens), 0) AS billable_input_tokens,
                COALESCE(SUM(output_tokens), 0) AS output_tokens,
                COALESCE(SUM(total_tokens), 0) AS total_tokens,
                COALESCE(SUM(cache_read_tokens), 0) AS cache_read_tokens,
                COALESCE(SUM(cache_creation_tokens), 0) AS cache_creation_tokens,
                COALESCE(SUM(estimated_cost_micros), 0) AS estimated_cost_micros,
                MAX(updated_at) AS last_updated_at
            FROM agent_usage
            WHERE project_id = ?1
            GROUP BY provider_id, model_id
            ORDER BY estimated_cost_micros DESC, total_tokens DESC, provider_id ASC, model_id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_usage_breakdown_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(ProjectUsageModelBreakdownRecord {
                provider_id: row.get(0)?,
                model_id: row.get(1)?,
                run_count: read_nonnegative_u64(row, 2)?,
                input_tokens: read_nonnegative_u64(row, 3)?,
                billable_input_tokens: read_nonnegative_u64(row, 4)?,
                output_tokens: read_nonnegative_u64(row, 5)?,
                total_tokens: read_nonnegative_u64(row, 6)?,
                cache_read_tokens: read_nonnegative_u64(row, 7)?,
                cache_creation_tokens: read_nonnegative_u64(row, 8)?,
                estimated_cost_micros: read_nonnegative_u64(row, 9)?,
                last_updated_at: row.get::<_, Option<String>>(10)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_usage_breakdown_query_failed", error)
        })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row.map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_usage_breakdown_decode_failed", error)
        })?);
    }
    Ok(out)
}

pub fn load_agent_session_run_snapshots(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Vec<(AgentRunSnapshotRecord, Option<AgentUsageRecord>)>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(agent_session_id, "agentSessionId")?;
    let connection = open_agent_database(repo_root)?;
    let runs = list_agent_runs_for_session_with_connection(
        &connection,
        repo_root,
        project_id,
        agent_session_id,
    )?;
    let mut snapshots = Vec::with_capacity(runs.len());
    for run in runs {
        let usage = read_agent_usage(&connection, repo_root, project_id, &run.run_id)?;
        let snapshot = read_agent_run_snapshot(&connection, repo_root, project_id, &run.run_id)?;
        snapshots.push((snapshot, usage));
    }
    Ok(snapshots)
}

pub fn list_agent_runs(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Vec<AgentRunRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(agent_session_id, "agentSessionId")?;
    let connection = open_agent_database(repo_root)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                lineage_kind,
                parent_run_id,
                parent_trace_id,
                parent_subagent_id,
                subagent_role,
                provider_id,
                model_id,
                status,
                prompt,
                system_prompt,
                started_at,
                last_heartbeat_at,
                completed_at,
                cancelled_at,
                last_error_code,
                last_error_message,
                updated_at
            FROM agent_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
            ORDER BY updated_at DESC, started_at DESC, run_id ASC
            LIMIT 50
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_runs_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, agent_session_id], read_agent_run_row)
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_runs_query_failed", error)
        })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| map_agent_store_query_error(repo_root, "agent_runs_decode_failed", error))
}

fn list_agent_runs_for_session_with_connection(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Vec<AgentRunRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                project_id,
                agent_session_id,
                run_id,
                trace_id,
                lineage_kind,
                parent_run_id,
                parent_trace_id,
                parent_subagent_id,
                subagent_role,
                provider_id,
                model_id,
                status,
                prompt,
                system_prompt,
                started_at,
                last_heartbeat_at,
                completed_at,
                cancelled_at,
                last_error_code,
                last_error_message,
                updated_at
            FROM agent_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
            ORDER BY started_at ASC, updated_at ASC, run_id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_runs_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, agent_session_id], read_agent_run_row)
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_runs_query_failed", error)
        })?;

    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| map_agent_store_query_error(repo_root, "agent_runs_decode_failed", error))
}

fn validate_agent_run(record: &NewAgentRunRecord) -> Result<(), CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.agent_session_id, "agentSessionId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.provider_id, "providerId")?;
    validate_non_empty_text(&record.model_id, "modelId")?;
    if let Some(definition_id) = record.agent_definition_id.as_ref() {
        validate_non_empty_text(definition_id, "agentDefinitionId")?;
    }
    if record.agent_definition_version == Some(0) {
        return Err(CommandError::invalid_request("agentDefinitionVersion"));
    }
    validate_non_empty_text(&record.prompt, "prompt")?;
    validate_non_empty_text(&record.system_prompt, "systemPrompt")
}

fn validate_payload_hash(payload_hash: &str, field: &'static str) -> Result<(), CommandError> {
    if !is_lowercase_sha256(payload_hash) {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn validate_agent_continuation_identity(
    project_id: &str,
    run_id: &str,
    request_id: &str,
    payload_hash: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_continuation_request_id(request_id)?;
    if !is_lowercase_sha256(payload_hash) {
        return Err(CommandError::invalid_request("payloadHash"));
    }
    Ok(())
}

fn validate_continuation_request_id(request_id: &str) -> Result<(), CommandError> {
    validate_non_empty_text(request_id, "continuationRequestId")?;
    if request_id.len() > 200 {
        return Err(CommandError::invalid_request("continuationRequestId"));
    }
    Ok(())
}

fn validate_agent_subagent_task(record: &AgentSubagentTaskRecord) -> Result<(), CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.parent_run_id, "parentRunId")?;
    validate_non_empty_text(&record.subagent_id, "subagentId")?;
    validate_non_empty_text(&record.role, "role")?;
    validate_non_empty_text(&record.role_label, "roleLabel")?;
    if !is_lowercase_sha256(&record.prompt_hash) {
        return Err(CommandError::invalid_request("promptHash"));
    }
    if let Some(model_id) = record.model_id.as_ref() {
        validate_non_empty_text(model_id, "modelId")?;
    }
    validate_json_payload(&record.write_set_json, "writeSetJson")?;
    if let Some(workflow_structure_json) = record.workflow_structure_json.as_ref() {
        validate_json_payload(workflow_structure_json, "workflowStructureJson")?;
    }
    validate_non_empty_text(&record.verification_contract, "verificationContract")?;
    validate_non_empty_text(&record.budget_status, "budgetStatus")?;
    if let Some(diagnostic_json) = record.budget_diagnostic_json.as_ref() {
        validate_json_payload(diagnostic_json, "budgetDiagnosticJson")?;
    }
    validate_non_empty_text(&record.status, "status")?;
    validate_non_empty_text(&record.created_at, "createdAt")?;
    if let Some(child_run_id) = record.child_run_id.as_ref() {
        validate_non_empty_text(child_run_id, "childRunId")?;
    }
    validate_optional_trace_id(record.child_trace_id.as_deref(), "childTraceId")?;
    validate_optional_trace_id(record.parent_trace_id.as_deref(), "parentTraceId")?;
    validate_json_payload(&record.input_log_json, "inputLogJson")?;
    if let Some(result_artifact) = record.result_artifact.as_ref() {
        validate_non_empty_text(result_artifact, "resultArtifact")?;
    }
    if let Some(parent_decision) = record.parent_decision.as_ref() {
        validate_non_empty_text(parent_decision, "parentDecision")?;
    }
    validate_non_empty_text(&record.updated_at, "updatedAt")
}

fn open_agent_database(repo_root: &Path) -> Result<rusqlite::Connection, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    open_runtime_database(repo_root, &database_path)
}

fn validate_non_empty_text(value: &str, field: &'static str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn validate_json_payload(value: &str, field: &'static str) -> Result<(), CommandError> {
    validate_non_empty_text(value, field)?;
    serde_json::from_str::<JsonValue>(value)
        .map(|_| ())
        .map_err(|_| CommandError::invalid_request(field))
}

fn validate_provider_metadata_json(value: &str) -> Result<(), CommandError> {
    validate_non_empty_text(value, "providerMetadata")?;
    let parsed = serde_json::from_str::<JsonValue>(value)
        .map_err(|_| CommandError::invalid_request("providerMetadata"))?;
    if !parsed.is_object() {
        return Err(CommandError::invalid_request("providerMetadata"));
    }
    Ok(())
}

fn validate_optional_sha256(value: Option<&str>, field: &'static str) -> Result<(), CommandError> {
    match value {
        Some(value) if is_lowercase_sha256(value) => Ok(()),
        Some(_) => Err(CommandError::invalid_request(field)),
        None => Ok(()),
    }
}

fn validate_optional_trace_id(
    value: Option<&str>,
    field: &'static str,
) -> Result<(), CommandError> {
    match value {
        Some(value) if is_lowercase_trace_id(value) => Ok(()),
        Some(_) => Err(CommandError::invalid_request(field)),
        None => Ok(()),
    }
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_lowercase_trace_id(value: &str) -> bool {
    value.len() == 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn read_agent_messages(
    connection: &rusqlite::Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Vec<AgentMessageRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, role, content, provider_metadata_json, created_at
            FROM agent_messages
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_messages_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            Ok(AgentMessageRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                role: parse_agent_message_role(row.get::<_, String>(3)?.as_str()),
                content: row.get(4)?,
                provider_metadata_json: row.get(5)?,
                created_at: row.get(6)?,
                attachments: Vec::new(),
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_messages_query_failed", error)
        })?;
    let mut messages: Vec<AgentMessageRecord> =
        rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_messages_decode_failed", error)
        })?;
    if messages.is_empty() {
        return Ok(messages);
    }
    let mut attachment_statement = connection
        .prepare(
            r#"
            SELECT id, message_id, project_id, run_id, kind, storage_path, media_type,
                   original_name, size_bytes, width, height, created_at
            FROM agent_message_attachments
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY message_id ASC, id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(
                repo_root,
                "agent_message_attachments_prepare_failed",
                error,
            )
        })?;
    let attachment_rows = attachment_statement
        .query_map(params![project_id, run_id], |row| {
            Ok(AgentMessageAttachmentRecord {
                id: row.get(0)?,
                message_id: row.get(1)?,
                project_id: row.get(2)?,
                run_id: row.get(3)?,
                kind: parse_agent_message_attachment_kind(row.get::<_, String>(4)?.as_str()),
                storage_path: row.get(5)?,
                media_type: row.get(6)?,
                original_name: row.get(7)?,
                size_bytes: row.get(8)?,
                width: row.get(9)?,
                height: row.get(10)?,
                created_at: row.get(11)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_message_attachments_query_failed", error)
        })?;
    let attachments: Vec<AgentMessageAttachmentRecord> = attachment_rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_message_attachments_decode_failed", error)
        })?;
    let mut by_id: std::collections::HashMap<i64, Vec<AgentMessageAttachmentRecord>> =
        std::collections::HashMap::new();
    for attachment in attachments {
        by_id
            .entry(attachment.message_id)
            .or_default()
            .push(attachment);
    }
    for message in &mut messages {
        if let Some(list) = by_id.remove(&message.id) {
            message.attachments = list;
        }
    }
    Ok(messages)
}

fn read_agent_events(
    connection: &rusqlite::Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Vec<AgentEventRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, event_kind, payload_json, created_at
            FROM agent_events
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_events_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            Ok(AgentEventRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                event_kind: parse_agent_event_kind(row.get::<_, String>(3)?.as_str()),
                payload_json: row.get(4)?,
                created_at: row.get(5)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_events_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_events_decode_failed", error)
    })
}

fn read_agent_tool_calls(
    connection: &rusqlite::Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Vec<AgentToolCallRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                run_id,
                tool_call_id,
                tool_name,
                input_json,
                state,
                result_json,
                error_code,
                error_message,
                started_at,
                completed_at
            FROM agent_tool_calls
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY started_at ASC, tool_call_id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_tool_calls_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            let error_code: Option<String> = row.get(7)?;
            let error_message: Option<String> = row.get(8)?;
            Ok(AgentToolCallRecord {
                project_id: row.get(0)?,
                run_id: row.get(1)?,
                tool_call_id: row.get(2)?,
                tool_name: row.get(3)?,
                input_json: row.get(4)?,
                state: parse_agent_tool_call_state(row.get::<_, String>(5)?.as_str()),
                result_json: row.get(6)?,
                error: match (error_code, error_message) {
                    (Some(code), Some(message)) => Some(AgentRunDiagnosticRecord { code, message }),
                    _ => None,
                },
                started_at: row.get(9)?,
                completed_at: row.get(10)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_tool_calls_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_tool_calls_decode_failed", error)
    })
}

fn read_agent_file_changes(
    connection: &rusqlite::Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Vec<AgentFileChangeRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                id,
                project_id,
                run_id,
                trace_id,
                top_level_run_id,
                subagent_id,
                subagent_role,
                change_group_id,
                path,
                operation,
                old_hash,
                new_hash,
                created_at
            FROM agent_file_changes
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_file_changes_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            Ok(AgentFileChangeRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                trace_id: row.get(3)?,
                top_level_run_id: row.get(4)?,
                subagent_id: row.get(5)?,
                subagent_role: row.get(6)?,
                change_group_id: row.get(7)?,
                path: row.get(8)?,
                operation: row.get(9)?,
                old_hash: row.get(10)?,
                new_hash: row.get(11)?,
                created_at: row.get(12)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_file_changes_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_file_changes_decode_failed", error)
    })
}

fn read_agent_checkpoints(
    connection: &rusqlite::Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Vec<AgentCheckpointRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, checkpoint_kind, summary, payload_json, created_at
            FROM agent_checkpoints
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_checkpoints_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            Ok(AgentCheckpointRecord {
                id: row.get(0)?,
                project_id: row.get(1)?,
                run_id: row.get(2)?,
                checkpoint_kind: row.get(3)?,
                summary: row.get(4)?,
                payload_json: row.get(5)?,
                created_at: row.get(6)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_checkpoints_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_checkpoints_decode_failed", error)
    })
}

fn read_agent_environment_lifecycle_snapshot_row(
    row: &Row<'_>,
) -> rusqlite::Result<AgentEnvironmentLifecycleSnapshotRecord> {
    Ok(AgentEnvironmentLifecycleSnapshotRecord {
        project_id: row.get(0)?,
        run_id: row.get(1)?,
        environment_id: row.get(2)?,
        state: row.get(3)?,
        previous_state: row.get(4)?,
        pending_message_count: row.get(5)?,
        health_checks_json: row.get(6)?,
        setup_steps_json: row.get(7)?,
        diagnostic_json: row.get(8)?,
        snapshot_json: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn read_agent_environment_pending_message_row(
    row: &Row<'_>,
) -> rusqlite::Result<AgentEnvironmentPendingMessageRecord> {
    Ok(AgentEnvironmentPendingMessageRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        run_id: row.get(2)?,
        role: parse_agent_message_role(row.get::<_, String>(3)?.as_str()),
        content: row.get(4)?,
        submitted_at: row.get(5)?,
        delivered_at: row.get(6)?,
    })
}

fn read_agent_action_requests(
    connection: &rusqlite::Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Vec<AgentActionRequestRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                run_id,
                action_id,
                action_type,
                title,
                detail,
                status,
                created_at,
                resolved_at,
                response
            FROM agent_action_requests
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY created_at ASC, action_id ASC
            "#,
        )
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_action_requests_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], |row| {
            Ok(AgentActionRequestRecord {
                project_id: row.get(0)?,
                run_id: row.get(1)?,
                action_id: row.get(2)?,
                action_type: row.get(3)?,
                title: row.get(4)?,
                detail: row.get(5)?,
                status: row.get(6)?,
                created_at: row.get(7)?,
                resolved_at: row.get(8)?,
                response: row.get(9)?,
            })
        })
        .map_err(|error| {
            map_agent_store_query_error(repo_root, "agent_action_requests_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_agent_store_query_error(repo_root, "agent_action_requests_decode_failed", error)
    })
}

fn agent_subagent_task_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            project_id,
            parent_run_id,
            subagent_id,
            role,
            role_label,
            prompt_hash,
            prompt_preview,
            model_id,
            write_set_json,
            workflow_structure_json,
            verification_contract,
            depth,
            max_tool_calls,
            max_tokens,
            max_cost_micros,
            used_tool_calls,
            used_tokens,
            used_cost_micros,
            budget_status,
            budget_diagnostic_json,
            status,
            created_at,
            started_at,
            completed_at,
            cancelled_at,
            integrated_at,
            child_run_id,
            child_trace_id,
            parent_trace_id,
            input_log_json,
            result_summary,
            result_artifact,
            parent_decision,
            latest_summary,
            updated_at
        FROM agent_subagent_tasks
        {where_clause}
        "#
    )
}

fn read_agent_subagent_task_row(row: &Row<'_>) -> rusqlite::Result<AgentSubagentTaskRecord> {
    Ok(AgentSubagentTaskRecord {
        project_id: row.get(0)?,
        parent_run_id: row.get(1)?,
        subagent_id: row.get(2)?,
        role: row.get(3)?,
        role_label: row.get(4)?,
        prompt_hash: row.get(5)?,
        prompt_preview: row.get(6)?,
        model_id: row.get(7)?,
        write_set_json: row.get(8)?,
        workflow_structure_json: row.get(9)?,
        verification_contract: row.get(10)?,
        depth: read_nonnegative_u64(row, 11)?,
        max_tool_calls: read_nonnegative_u64(row, 12)?,
        max_tokens: read_nonnegative_u64(row, 13)?,
        max_cost_micros: read_nonnegative_u64(row, 14)?,
        used_tool_calls: read_nonnegative_u64(row, 15)?,
        used_tokens: read_nonnegative_u64(row, 16)?,
        used_cost_micros: read_nonnegative_u64(row, 17)?,
        budget_status: row.get(18)?,
        budget_diagnostic_json: row.get(19)?,
        status: row.get(20)?,
        created_at: row.get(21)?,
        started_at: row.get(22)?,
        completed_at: row.get(23)?,
        cancelled_at: row.get(24)?,
        integrated_at: row.get(25)?,
        child_run_id: row.get(26)?,
        child_trace_id: row.get(27)?,
        parent_trace_id: row.get(28)?,
        input_log_json: row.get(29)?,
        result_summary: row.get(30)?,
        result_artifact: row.get(31)?,
        parent_decision: row.get(32)?,
        latest_summary: row.get(33)?,
        updated_at: row.get(34)?,
    })
}

fn read_agent_run_drive_lease(row: &Row<'_>) -> rusqlite::Result<AgentRunDriveLeaseRecord> {
    Ok(AgentRunDriveLeaseRecord {
        project_id: row.get(0)?,
        run_id: row.get(1)?,
        owner_instance_id: row.get(2)?,
        owner_process_id: read_positive_u32(row, 3)?,
        owner_process_birth_identity: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
        drive_token: row.get(5)?,
        acquired_at: row.get(6)?,
    })
}

fn read_agent_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRunRecord> {
    let last_error_code: Option<String> = row.get(21)?;
    let last_error_message: Option<String> = row.get(22)?;
    Ok(AgentRunRecord {
        runtime_agent_id: parse_runtime_agent_id(row.get::<_, String>(0)?.as_str()),
        agent_definition_id: row.get(1)?,
        agent_definition_version: read_positive_u32(row, 2)?,
        project_id: row.get(3)?,
        agent_session_id: row.get(4)?,
        run_id: row.get(5)?,
        trace_id: row.get(6)?,
        lineage_kind: row.get(7)?,
        parent_run_id: row.get(8)?,
        parent_trace_id: row.get(9)?,
        parent_subagent_id: row.get(10)?,
        subagent_role: row.get(11)?,
        provider_id: row.get(12)?,
        model_id: row.get(13)?,
        status: parse_agent_run_status(row.get::<_, String>(14)?.as_str()),
        prompt: row.get(15)?,
        system_prompt: row.get(16)?,
        started_at: row.get(17)?,
        last_heartbeat_at: row.get(18)?,
        completed_at: row.get(19)?,
        cancelled_at: row.get(20)?,
        last_error: match (last_error_code, last_error_message) {
            (Some(code), Some(message)) => Some(AgentRunDiagnosticRecord { code, message }),
            _ => None,
        },
        updated_at: row.get(23)?,
    })
}

fn read_agent_usage_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentUsageRecord> {
    Ok(AgentUsageRecord {
        project_id: row.get(0)?,
        run_id: row.get(1)?,
        agent_definition_id: row.get(2)?,
        agent_definition_version: read_positive_u32(row, 3)?,
        provider_id: row.get(4)?,
        model_id: row.get(5)?,
        input_tokens: read_nonnegative_u64(row, 6)?,
        billable_input_tokens: read_nonnegative_u64(row, 7)?,
        output_tokens: read_nonnegative_u64(row, 8)?,
        total_tokens: read_nonnegative_u64(row, 9)?,
        cache_read_tokens: read_nonnegative_u64(row, 10)?,
        cache_creation_tokens: read_nonnegative_u64(row, 11)?,
        estimated_cost_micros: read_nonnegative_u64(row, 12)?,
        updated_at: row.get(13)?,
    })
}

fn read_positive_u32(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u32> {
    let value: i64 = row.get(index)?;
    u32::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn read_nonnegative_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

pub fn agent_run_status_sql_value(status: &AgentRunStatus) -> &'static str {
    match status {
        AgentRunStatus::Starting => "starting",
        AgentRunStatus::Running => "running",
        AgentRunStatus::Paused => "paused",
        AgentRunStatus::Cancelling => "cancelling",
        AgentRunStatus::Cancelled => "cancelled",
        AgentRunStatus::HandedOff => "handed_off",
        AgentRunStatus::Completed => "completed",
        AgentRunStatus::Failed => "failed",
    }
}

pub fn runtime_agent_id_sql_value(runtime_agent_id: &RuntimeAgentIdDto) -> &'static str {
    runtime_agent_id.as_str()
}

pub fn agent_event_kind_sql_value(kind: &AgentRunEventKind) -> &'static str {
    match kind {
        AgentRunEventKind::RunStarted => "run_started",
        AgentRunEventKind::AssistantCandidate => "assistant_candidate",
        AgentRunEventKind::MessageDelta => "message_delta",
        AgentRunEventKind::ReasoningSummary => "reasoning_summary",
        AgentRunEventKind::ToolStarted => "tool_started",
        AgentRunEventKind::ToolDelta => "tool_delta",
        AgentRunEventKind::ToolCompleted => "tool_completed",
        AgentRunEventKind::FileChanged => "file_changed",
        AgentRunEventKind::CommandOutput => "command_output",
        AgentRunEventKind::ValidationStarted => "validation_started",
        AgentRunEventKind::ValidationCompleted => "validation_completed",
        AgentRunEventKind::ToolRegistrySnapshot => "tool_registry_snapshot",
        AgentRunEventKind::PolicyDecision => "policy_decision",
        AgentRunEventKind::StateTransition => "state_transition",
        AgentRunEventKind::PlanUpdated => "plan_updated",
        AgentRunEventKind::RouteRequested => "route_requested",
        AgentRunEventKind::VerificationGate => "verification_gate",
        AgentRunEventKind::ContextManifestRecorded => "context_manifest_recorded",
        AgentRunEventKind::RetrievalPerformed => "retrieval_performed",
        AgentRunEventKind::MemoryCandidateCaptured => "memory_candidate_captured",
        AgentRunEventKind::EnvironmentLifecycleUpdate => "environment_lifecycle_update",
        AgentRunEventKind::SandboxLifecycleUpdate => "sandbox_lifecycle_update",
        AgentRunEventKind::ActionRequired => "action_required",
        AgentRunEventKind::ApprovalRequired => "approval_required",
        AgentRunEventKind::ToolPermissionGrant => "tool_permission_grant",
        AgentRunEventKind::ProviderModelChanged => "provider_model_changed",
        AgentRunEventKind::RuntimeSettingsChanged => "runtime_settings_changed",
        AgentRunEventKind::RunPaused => "run_paused",
        AgentRunEventKind::RunCompleted => "run_completed",
        AgentRunEventKind::RunFailed => "run_failed",
        AgentRunEventKind::SubagentLifecycle => "subagent_lifecycle",
    }
}

pub fn agent_message_role_sql_value(role: &AgentMessageRole) -> &'static str {
    match role {
        AgentMessageRole::System => "system",
        AgentMessageRole::Developer => "developer",
        AgentMessageRole::User => "user",
        AgentMessageRole::Assistant => "assistant",
        AgentMessageRole::Tool => "tool",
    }
}

pub fn agent_message_attachment_kind_sql_value(kind: &AgentMessageAttachmentKind) -> &'static str {
    match kind {
        AgentMessageAttachmentKind::Image => "image",
        AgentMessageAttachmentKind::Document => "document",
        AgentMessageAttachmentKind::Text => "text",
    }
}

fn parse_agent_message_attachment_kind(value: &str) -> AgentMessageAttachmentKind {
    match value {
        "image" => AgentMessageAttachmentKind::Image,
        "document" => AgentMessageAttachmentKind::Document,
        _ => AgentMessageAttachmentKind::Text,
    }
}

pub fn agent_tool_call_state_sql_value(state: &AgentToolCallState) -> &'static str {
    match state {
        AgentToolCallState::Pending => "pending",
        AgentToolCallState::Running => "running",
        AgentToolCallState::Succeeded => "succeeded",
        AgentToolCallState::Failed => "failed",
    }
}

fn parse_agent_run_status(value: &str) -> AgentRunStatus {
    match value {
        "starting" => AgentRunStatus::Starting,
        "running" => AgentRunStatus::Running,
        "paused" => AgentRunStatus::Paused,
        "cancelling" => AgentRunStatus::Cancelling,
        "cancelled" => AgentRunStatus::Cancelled,
        "handed_off" => AgentRunStatus::HandedOff,
        "completed" => AgentRunStatus::Completed,
        "failed" => AgentRunStatus::Failed,
        _ => AgentRunStatus::Failed,
    }
}

fn parse_agent_continuation_request_state(value: &str) -> AgentContinuationRequestState {
    match value {
        "prepared" => AgentContinuationRequestState::Prepared,
        "driving" => AgentContinuationRequestState::Driving,
        _ => AgentContinuationRequestState::Consumed,
    }
}

fn parse_runtime_agent_id(value: &str) -> RuntimeAgentIdDto {
    match value {
        "plan" => RuntimeAgentIdDto::Plan,
        "computer_use" => RuntimeAgentIdDto::ComputerUse,
        "engineer" => RuntimeAgentIdDto::Engineer,
        "debug" => RuntimeAgentIdDto::Debug,
        "crawl" => RuntimeAgentIdDto::Crawl,
        "agent_create" => RuntimeAgentIdDto::AgentCreate,
        "generalist" => RuntimeAgentIdDto::Generalist,
        _ => RuntimeAgentIdDto::Ask,
    }
}

fn parse_agent_event_kind(value: &str) -> AgentRunEventKind {
    match value {
        "run_started" => AgentRunEventKind::RunStarted,
        "assistant_candidate" => AgentRunEventKind::AssistantCandidate,
        "message_delta" => AgentRunEventKind::MessageDelta,
        "reasoning_summary" => AgentRunEventKind::ReasoningSummary,
        "tool_started" => AgentRunEventKind::ToolStarted,
        "tool_delta" => AgentRunEventKind::ToolDelta,
        "tool_completed" => AgentRunEventKind::ToolCompleted,
        "file_changed" => AgentRunEventKind::FileChanged,
        "command_output" => AgentRunEventKind::CommandOutput,
        "validation_started" => AgentRunEventKind::ValidationStarted,
        "validation_completed" => AgentRunEventKind::ValidationCompleted,
        "tool_registry_snapshot" => AgentRunEventKind::ToolRegistrySnapshot,
        "policy_decision" => AgentRunEventKind::PolicyDecision,
        "state_transition" => AgentRunEventKind::StateTransition,
        "plan_updated" => AgentRunEventKind::PlanUpdated,
        "route_requested" => AgentRunEventKind::RouteRequested,
        "verification_gate" => AgentRunEventKind::VerificationGate,
        "context_manifest_recorded" => AgentRunEventKind::ContextManifestRecorded,
        "retrieval_performed" => AgentRunEventKind::RetrievalPerformed,
        "memory_candidate_captured" => AgentRunEventKind::MemoryCandidateCaptured,
        "environment_lifecycle_update" => AgentRunEventKind::EnvironmentLifecycleUpdate,
        "sandbox_lifecycle_update" => AgentRunEventKind::SandboxLifecycleUpdate,
        "action_required" => AgentRunEventKind::ActionRequired,
        "approval_required" => AgentRunEventKind::ApprovalRequired,
        "tool_permission_grant" => AgentRunEventKind::ToolPermissionGrant,
        "provider_model_changed" => AgentRunEventKind::ProviderModelChanged,
        "runtime_settings_changed" => AgentRunEventKind::RuntimeSettingsChanged,
        "run_paused" => AgentRunEventKind::RunPaused,
        "run_completed" => AgentRunEventKind::RunCompleted,
        "run_failed" => AgentRunEventKind::RunFailed,
        "subagent_lifecycle" => AgentRunEventKind::SubagentLifecycle,
        _ => AgentRunEventKind::RunFailed,
    }
}

fn parse_agent_message_role(value: &str) -> AgentMessageRole {
    match value {
        "system" => AgentMessageRole::System,
        "developer" => AgentMessageRole::Developer,
        "user" => AgentMessageRole::User,
        "assistant" => AgentMessageRole::Assistant,
        "tool" => AgentMessageRole::Tool,
        _ => AgentMessageRole::Assistant,
    }
}

fn parse_agent_tool_call_state(value: &str) -> AgentToolCallState {
    match value {
        "pending" => AgentToolCallState::Pending,
        "running" => AgentToolCallState::Running,
        "succeeded" => AgentToolCallState::Succeeded,
        "failed" => AgentToolCallState::Failed,
        _ => AgentToolCallState::Failed,
    }
}

fn map_agent_store_query_error(
    repo_root: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not read owned-agent state from {}: {error}",
            database_path_for_repo(repo_root).display()
        ),
    )
}

fn map_agent_store_write_error(
    repo_root: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not persist owned-agent state to {}: {error}",
            database_path_for_repo(repo_root).display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;
    use std::{
        fs,
        sync::{Arc, Barrier},
        thread,
    };

    use crate::db::{
        configure_connection, migrations::migrations, register_project_database_path_for_tests,
    };

    fn create_project_database(repo_root: &Path, project_id: &str) {
        let database_path = repo_root.join("state.db");
        register_project_database_path_for_tests(repo_root, database_path.clone());
        let mut connection = Connection::open(&database_path).expect("open project database");
        configure_connection(&connection).expect("configure project database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");
        connection
            .execute(
                "INSERT INTO agent_sessions (project_id, agent_session_id, title, status, selected) VALUES (?1, 'session-1', 'Default', 'active', 1)",
                params![project_id],
            )
            .expect("insert agent session");
    }

    fn seed_run(repo_root: &Path, project_id: &str, run_id: &str) {
        insert_agent_run(
            repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: "session-1".into(),
                run_id: run_id.into(),
                provider_id: "provider-1".into(),
                model_id: "model-1".into(),
                prompt: "Do the thing".into(),
                system_prompt: "System prompt".into(),
                now: "2026-06-05T12:00:00Z".into(),
            },
        )
        .expect("insert agent run");
    }

    fn seed_running_run(repo_root: &Path, project_id: &str, run_id: &str) {
        seed_run(repo_root, project_id, run_id);
        update_agent_run_status(
            repo_root,
            project_id,
            run_id,
            AgentRunStatus::Running,
            None,
            "2026-07-15T14:00:00Z",
        )
        .expect("finish agent start");
    }

    fn owned_start_run(project_id: &str, run_id: &str) -> NewAgentRunRecord {
        NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: project_id.into(),
            agent_session_id: "session-1".into(),
            run_id: run_id.into(),
            provider_id: "provider-1".into(),
            model_id: "model-1".into(),
            prompt: "Recover this exact start".into(),
            system_prompt: "System prompt".into(),
            now: "2026-07-15T12:00:00Z".into(),
        }
    }

    #[test]
    fn ready_agent_start_accepts_exact_original_replay_with_recovery_payload_intact() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-ready-start-replay";
        let run_id = "run-ready-start-replay";
        create_project_database(&repo_root, project_id);
        let record = owned_start_run(project_id, run_id);
        let payload_hash = "a".repeat(64);
        let recovery_payload = serde_json::json!({
            "schema": "test.owned_agent_start_recovery.v1",
            "projectId": project_id,
            "runId": run_id,
            "controls": { "approvalMode": "suggest" },
        })
        .to_string();

        assert!(matches!(
            register_agent_run_start(
                &repo_root,
                &record,
                &payload_hash,
                &recovery_payload,
                42,
                "birth-original",
            )
            .expect("register original start"),
            AgentRunStartRegistrationResult::Registered(_)
        ));
        mark_agent_run_start_ready(&repo_root, project_id, run_id, "2026-07-15T12:00:01Z")
            .expect("mark ready");

        let AgentRunStartRegistrationResult::Replayed { request, .. } = register_agent_run_start(
            &repo_root,
            &record,
            &payload_hash,
            &recovery_payload,
            99,
            "birth-replay",
        )
        .expect("accept exact replay") else {
            panic!("exact ready replay should not create another run");
        };
        assert_eq!(request.state, AgentRunStartRequestState::Ready);
        assert_eq!(request.recovery_payload_json, recovery_payload);
    }

    #[test]
    fn interrupted_start_replacement_is_atomic_and_single_winner() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-start-replace-race";
        let run_id = "run-start-replace-race";
        create_project_database(&repo_root, project_id);
        let record = owned_start_run(project_id, run_id);
        let payload_hash = "b".repeat(64);
        let recovery_payload = r#"{"schema":"test.start-recovery.v1"}"#.to_string();
        register_agent_run_start(
            &repo_root,
            &record,
            &payload_hash,
            &recovery_payload,
            42,
            "birth-interrupted",
        )
        .expect("register interrupted start");
        fail_preparing_agent_run_start(
            &repo_root,
            project_id,
            run_id,
            AgentRunDiagnosticRecord {
                code: "interrupted".into(),
                message: "Original owner exited.".into(),
            },
            "2026-07-15T12:00:01Z",
        )
        .expect("mark interrupted start failed");
        let expected = load_agent_run_start_request(&repo_root, project_id, run_id)
            .expect("load failed marker")
            .expect("failed marker exists");

        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for index in 0..2_u32 {
            let barrier = barrier.clone();
            let repo_root = repo_root.clone();
            let record = record.clone();
            let expected = expected.clone();
            let payload_hash = payload_hash.clone();
            let recovery_payload = recovery_payload.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                replace_replayable_agent_run_start(
                    &repo_root,
                    &expected,
                    &record,
                    &payload_hash,
                    &recovery_payload,
                    100 + index,
                    &format!("replacement-birth-{index}"),
                )
                .expect("replace interrupted start")
            }));
        }
        barrier.wait();
        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().expect("join replacement worker"))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, AgentRunStartRegistrationResult::Registered(_)))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(
                    outcome,
                    AgentRunStartRegistrationResult::Replayed { .. }
                ))
                .count(),
            1
        );
        let final_start = load_agent_run_start_request(&repo_root, project_id, run_id)
            .expect("load replacement marker")
            .expect("replacement marker remains present");
        assert_eq!(final_start.state, AgentRunStartRequestState::Preparing);
        assert_eq!(final_start.payload_hash, payload_hash);
        assert_eq!(final_start.recovery_payload_json, recovery_payload);
        load_agent_run(&repo_root, project_id, run_id).expect("replacement run remains present");
    }

    #[test]
    fn file_change_event_transaction_rolls_back_when_payload_preparation_fails() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-file-change-rollback";
        let run_id = "run-file-change-rollback";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        let events_before = read_all_agent_events(&repo_root, project_id, run_id)
            .expect("load events before failed transaction");

        let error = append_agent_file_change_with_event(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                change_group_id: None,
                path: "src/lib.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-07-15T12:00:01Z".into(),
            },
            |_| -> Result<JsonValue, CommandError> {
                Err(CommandError::system_fault(
                    "test_file_change_payload_failed",
                    "Simulated freshness failure.",
                ))
            },
        )
        .expect_err("payload preparation failure should roll back transaction");

        assert_eq!(error.code, "test_file_change_payload_failed");
        assert!(load_agent_file_changes(&repo_root, project_id, run_id)
            .expect("load file changes after rollback")
            .is_empty());
        let events_after = read_all_agent_events(&repo_root, project_id, run_id)
            .expect("load events after failed transaction");
        assert_eq!(events_after.len(), events_before.len());
        assert_eq!(
            events_after
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>(),
            events_before
                .iter()
                .map(|event| event.id)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn file_change_and_event_commit_together_after_prior_events() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-file-change-order";
        let run_id = "run-file-change-order";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        let prior_event = append_agent_event(
            &repo_root,
            &NewAgentEventRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                event_kind: AgentRunEventKind::CommandOutput,
                payload_json: json!({ "toolName": "write" }).to_string(),
                created_at: "2026-07-15T12:00:01Z".into(),
            },
        )
        .expect("append prior event");

        let (change, file_changed_event) = append_agent_file_change_with_event(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                change_group_id: Some("change-group-1".into()),
                path: "src/lib.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-07-15T12:00:02Z".into(),
            },
            |stored_change| {
                Ok(json!({
                    "path": stored_change.path.clone(),
                    "toPath": "src/main.rs",
                    "toolCallId": "tool-call-file-change-order",
                    "traceId": stored_change.trace_id.clone(),
                }))
            },
        )
        .expect("append file change and event");

        assert!(file_changed_event.id > prior_event.id);
        assert_eq!(
            file_changed_event.event_kind,
            AgentRunEventKind::FileChanged
        );
        assert_eq!(file_changed_event.created_at, change.created_at);
        let payload = serde_json::from_str::<JsonValue>(&file_changed_event.payload_json)
            .expect("decode file-changed event payload");
        assert_eq!(payload["path"], json!(change.path));
        assert_eq!(payload["traceId"], json!(change.trace_id));

        let stored_changes = load_agent_file_changes(&repo_root, project_id, run_id)
            .expect("load committed file changes");
        assert_eq!(stored_changes.len(), 1);
        assert_eq!(stored_changes[0].id, change.id);
        let events =
            read_all_agent_events(&repo_root, project_id, run_id).expect("load committed events");
        let prior_index = events
            .iter()
            .position(|event| event.id == prior_event.id)
            .expect("prior event remains stored");
        let file_changed_index = events
            .iter()
            .position(|event| event.id == file_changed_event.id)
            .expect("file-changed event is stored");
        assert!(file_changed_index > prior_index);
        assert_eq!(
            read_agent_file_change_paths_for_tool_call(
                &repo_root,
                project_id,
                run_id,
                "tool-call-file-change-order",
            )
            .expect("read changed paths for tool call"),
            vec!["src/lib.rs".to_string(), "src/main.rs".to_string()]
        );
    }

    fn append_action(repo_root: &Path, project_id: &str, run_id: &str, action_id: &str) {
        append_agent_action_request(
            repo_root,
            &NewAgentActionRequestRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                action_id: action_id.into(),
                action_type: "command_review".into(),
                title: format!("Review {action_id}"),
                detail: "Review before continuing.".into(),
                created_at: "2026-06-05T12:00:01Z".into(),
            },
        )
        .expect("append action request");
    }

    #[test]
    fn concurrent_agent_run_drive_lease_claims_have_one_winner() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-drive-lease-race";
        let run_id = "run-drive-lease-race";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);

        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for index in 0..2_u32 {
            let barrier = barrier.clone();
            let repo_root = repo_root.clone();
            workers.push(thread::spawn(move || {
                barrier.wait();
                claim_agent_run_drive_lease(
                    &repo_root,
                    project_id,
                    run_id,
                    &format!("app-instance-{index}"),
                    10_000 + index,
                    &format!("process-birth-{index}"),
                    &format!("drive-token-{index}"),
                    "2026-07-15T12:00:00Z",
                )
                .expect("claim drive lease")
            }));
        }
        barrier.wait();

        let outcomes = workers
            .into_iter()
            .map(|worker| worker.join().expect("join lease claimant"))
            .collect::<Vec<_>>();
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, AgentRunDriveLeaseClaimResult::Acquired))
                .count(),
            1
        );
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, AgentRunDriveLeaseClaimResult::Held(_)))
                .count(),
            1
        );

        let held = outcomes
            .into_iter()
            .find_map(|outcome| match outcome {
                AgentRunDriveLeaseClaimResult::Held(held) => Some(held),
                AgentRunDriveLeaseClaimResult::Acquired
                | AgentRunDriveLeaseClaimResult::RunNotDrivable(_) => None,
            })
            .expect("held lease describes winner");
        assert!(release_agent_run_drive_lease(
            &repo_root,
            project_id,
            run_id,
            &held.owner_instance_id,
            &held.drive_token,
        )
        .expect("release winning lease"));
        assert_eq!(
            claim_agent_run_drive_lease(
                &repo_root,
                project_id,
                run_id,
                "app-instance-after-release",
                20_000,
                "process-birth-after-release",
                "drive-token-after-release",
                "2026-07-15T12:01:00Z",
            )
            .expect("claim after release"),
            AgentRunDriveLeaseClaimResult::Acquired
        );
    }

    #[test]
    fn agent_run_drive_lease_recovery_is_compare_and_swap_guarded() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-drive-lease-recovery";
        let run_id = "run-drive-lease-recovery";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);

        assert_eq!(
            claim_agent_run_drive_lease(
                &repo_root,
                project_id,
                run_id,
                "prior-app-instance",
                30_000,
                "prior-process-birth",
                "prior-drive-token",
                "2026-07-15T12:00:00Z",
            )
            .expect("claim prior lease"),
            AgentRunDriveLeaseClaimResult::Acquired
        );
        let expected = AgentRunDriveLeaseRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            owner_instance_id: "prior-app-instance".into(),
            owner_process_id: 30_000,
            owner_process_birth_identity: "prior-process-birth".into(),
            drive_token: "prior-drive-token".into(),
            acquired_at: "2026-07-15T12:00:00Z".into(),
        };
        let replacement = AgentRunDriveLeaseRecord {
            owner_instance_id: "replacement-app-instance".into(),
            owner_process_id: 40_000,
            owner_process_birth_identity: "replacement-process-birth".into(),
            drive_token: "replacement-drive-token".into(),
            acquired_at: "2026-07-15T12:01:00Z".into(),
            ..expected.clone()
        };
        let stale_expectation = AgentRunDriveLeaseRecord {
            drive_token: "not-the-current-token".into(),
            ..expected.clone()
        };

        assert!(
            !replace_agent_run_drive_lease(&repo_root, &replacement, &stale_expectation,)
                .expect("reject stale recovery CAS")
        );
        assert!(
            replace_agent_run_drive_lease(&repo_root, &replacement, &expected,)
                .expect("replace abandoned lease")
        );
        assert!(!release_agent_run_drive_lease(
            &repo_root,
            project_id,
            run_id,
            &expected.owner_instance_id,
            &expected.drive_token,
        )
        .expect("old owner cannot release replacement"));
        assert!(release_agent_run_drive_lease(
            &repo_root,
            project_id,
            run_id,
            &replacement.owner_instance_id,
            &replacement.drive_token,
        )
        .expect("replacement owner releases lease"));
    }

    #[test]
    fn agent_run_drive_lease_heartbeat_is_owner_and_token_guarded() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-drive-lease-heartbeat";
        let run_id = "run-drive-lease-heartbeat";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        claim_agent_run_drive_lease(
            &repo_root,
            project_id,
            run_id,
            "lease-owner",
            30_000,
            "lease-owner-process-birth",
            "lease-token",
            "2026-07-15T12:00:00Z",
        )
        .expect("claim lease");

        assert!(!renew_agent_run_drive_lease(
            &repo_root,
            project_id,
            run_id,
            "lease-owner",
            "wrong-token",
            "2026-07-15T12:00:05Z",
        )
        .expect("reject stale heartbeat token"));
        assert!(renew_agent_run_drive_lease(
            &repo_root,
            project_id,
            run_id,
            "lease-owner",
            "lease-token",
            "2026-07-15T12:00:05Z",
        )
        .expect("renew current lease"));
        let renewed = load_agent_run_drive_lease(&repo_root, project_id, run_id)
            .expect("load renewed lease")
            .expect("lease exists");
        assert_eq!(renewed.acquired_at, "2026-07-15T12:00:05Z");
    }

    #[test]
    fn answer_pending_agent_action_request_resolves_only_matching_row() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-1";
        let run_id = "run-1";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        append_action(&repo_root, project_id, run_id, "action-a");
        append_action(&repo_root, project_id, run_id, "action-b");

        let answered = answer_pending_agent_action_request(
            &repo_root,
            project_id,
            run_id,
            "action-a",
            "Approved.",
        )
        .expect("answer action-a");

        assert_eq!(answered.action_id, "action-a");
        assert_eq!(answered.status, "answered");
        assert_eq!(answered.response.as_deref(), Some("Approved."));
        let snapshot = load_agent_run(&repo_root, project_id, run_id).expect("load run");
        let action_a = snapshot
            .action_requests
            .iter()
            .find(|action| action.action_id == "action-a")
            .expect("action-a row");
        let action_b = snapshot
            .action_requests
            .iter()
            .find(|action| action.action_id == "action-b")
            .expect("action-b row");
        assert_eq!(action_a.status, "answered");
        assert_eq!(action_b.status, "pending");
        assert!(action_b.response.is_none());

        let retry_error = answer_pending_agent_action_request(
            &repo_root,
            project_id,
            run_id,
            "action-a",
            "Approved again.",
        )
        .expect_err("resolved action cannot be answered again");
        assert_eq!(retry_error.code, "agent_action_request_already_resolved");
    }

    #[test]
    fn rejecting_an_action_atomically_fails_the_run_and_exact_replay_is_a_noop() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "project-action-rejection";
        let run_id = "run-action-rejection";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        append_action(&repo_root, project_id, run_id, "action-a");

        let committed = reject_agent_action_and_fail_run(
            &repo_root,
            project_id,
            run_id,
            "action-a",
            Some("Do not run this command."),
            "2026-07-15T12:00:00Z",
        )
        .expect("reject action");
        assert!(!committed.replayed);
        assert_eq!(committed.action.status, "rejected");
        assert_eq!(committed.snapshot.run.status, AgentRunStatus::Failed);
        assert_eq!(committed.inserted_events.len(), 3);
        let event_count = committed.snapshot.events.len();

        let replay = reject_agent_action_and_fail_run(
            &repo_root,
            project_id,
            run_id,
            "action-a",
            Some("Do not run this command."),
            "2026-07-15T12:00:01Z",
        )
        .expect("replay exact rejection");
        assert!(replay.replayed);
        assert!(replay.inserted_events.is_empty());
        assert_eq!(replay.snapshot.events.len(), event_count);

        let conflict = reject_agent_action_and_fail_run(
            &repo_root,
            project_id,
            run_id,
            "action-a",
            Some("Use a different response."),
            "2026-07-15T12:00:02Z",
        )
        .expect_err("conflicting replay must fail");
        assert_eq!(conflict.code, "agent_action_rejection_conflict");
    }

    #[test]
    fn terminal_run_status_allows_only_completed_to_handed_off_refinement() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "terminal-status-refinement";
        create_project_database(&repo_root, project_id);

        seed_run(&repo_root, project_id, "completed-to-handoff");
        let completed = update_agent_run_status(
            &repo_root,
            project_id,
            "completed-to-handoff",
            AgentRunStatus::Completed,
            None,
            "2026-06-05T12:01:00Z",
        )
        .expect("complete source run");
        assert_eq!(completed.run.status, AgentRunStatus::Completed);
        assert_eq!(
            completed.run.completed_at.as_deref(),
            Some("2026-06-05T12:01:00Z")
        );

        let handed_off = update_agent_run_status(
            &repo_root,
            project_id,
            "completed-to-handoff",
            AgentRunStatus::HandedOff,
            None,
            "2026-06-05T12:02:00Z",
        )
        .expect("refine completed source to handed off");
        assert_eq!(handed_off.run.status, AgentRunStatus::HandedOff);
        assert_eq!(
            handed_off.run.completed_at.as_deref(),
            Some("2026-06-05T12:01:00Z"),
            "handoff refinement preserves the original completion timestamp"
        );

        seed_run(&repo_root, project_id, "completed-stays-completed");
        update_agent_run_status(
            &repo_root,
            project_id,
            "completed-stays-completed",
            AgentRunStatus::Completed,
            None,
            "2026-06-05T12:03:00Z",
        )
        .expect("complete second run");
        let late_cancel = update_agent_run_status(
            &repo_root,
            project_id,
            "completed-stays-completed",
            AgentRunStatus::Cancelled,
            None,
            "2026-06-05T12:04:00Z",
        )
        .expect("late cancel returns winning terminal state");
        assert_eq!(late_cancel.run.status, AgentRunStatus::Completed);
        assert_eq!(
            late_cancel.run.completed_at.as_deref(),
            Some("2026-06-05T12:03:00Z")
        );

        seed_run(&repo_root, project_id, "failed-stays-failed");
        update_agent_run_status(
            &repo_root,
            project_id,
            "failed-stays-failed",
            AgentRunStatus::Failed,
            None,
            "2026-06-05T12:05:00Z",
        )
        .expect("fail third run");
        let late_handoff = update_agent_run_status(
            &repo_root,
            project_id,
            "failed-stays-failed",
            AgentRunStatus::HandedOff,
            None,
            "2026-06-05T12:06:00Z",
        )
        .expect("late handoff returns failed terminal state");
        assert_eq!(late_handoff.run.status, AgentRunStatus::Failed);
    }

    #[test]
    fn explicit_continuation_reopens_completed_and_failed_runs_only() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-reopen";
        create_project_database(&repo_root, project_id);

        for (run_id, terminal_status) in [
            ("completed-run", AgentRunStatus::Completed),
            ("failed-run", AgentRunStatus::Failed),
        ] {
            seed_run(&repo_root, project_id, run_id);
            let terminal = update_agent_run_status(
                &repo_root,
                project_id,
                run_id,
                terminal_status,
                Some(AgentRunDiagnosticRecord {
                    code: "prior_terminal".into(),
                    message: "Prior turn ended.".into(),
                }),
                "2026-06-05T12:10:00Z",
            )
            .expect("finish prior turn");
            assert_ne!(terminal.run.status, AgentRunStatus::Running);

            let reopened = reopen_agent_run_for_continuation(
                &repo_root,
                project_id,
                run_id,
                "2026-06-05T12:11:00Z",
            )
            .expect("reopen terminal run for continuation");
            assert_eq!(reopened.run.status, AgentRunStatus::Running);
            assert!(reopened.run.completed_at.is_none());
            assert!(reopened.run.cancelled_at.is_none());
            assert!(reopened.run.last_error.is_none());
        }

        seed_run(&repo_root, project_id, "handed-off-run");
        update_agent_run_status(
            &repo_root,
            project_id,
            "handed-off-run",
            AgentRunStatus::Completed,
            None,
            "2026-06-05T12:12:00Z",
        )
        .expect("complete source run");
        update_agent_run_status(
            &repo_root,
            project_id,
            "handed-off-run",
            AgentRunStatus::HandedOff,
            None,
            "2026-06-05T12:13:00Z",
        )
        .expect("hand off source run");
        let error = reopen_agent_run_for_continuation(
            &repo_root,
            project_id,
            "handed-off-run",
            "2026-06-05T12:14:00Z",
        )
        .expect_err("handed-off source must remain terminal");
        assert_eq!(error.code, "agent_run_not_resumable");
    }

    fn continuation_preparation(
        project_id: &str,
        run_id: &str,
        request_id: &str,
        payload_hash: &str,
    ) -> NewAgentContinuationPreparationRecord {
        NewAgentContinuationPreparationRecord {
            project_id: project_id.into(),
            request_id: request_id.into(),
            run_id: run_id.into(),
            payload_hash: payload_hash.into(),
            recovery_payload_json: serde_json::json!({
                "schema": "test.agent_continuation_recovery.v1",
                "projectId": project_id,
                "runId": run_id,
                "requestId": request_id,
            })
            .to_string(),
            role: AgentMessageRole::User,
            content: "Continue safely.".into(),
            attachments: vec![NewMessageAttachmentInput {
                kind: AgentMessageAttachmentKind::Text,
                storage_path: "/tmp/context.txt".into(),
                media_type: "text/plain".into(),
                original_name: "context.txt".into(),
                size_bytes: 12,
                width: None,
                height: None,
            }],
            linked_path_grant_payload_json: Some(
                r#"{"schema":"xero.linked_context_paths.v1","grantKind":"linked_context_paths","paths":[{"kind":"file","absolutePath":"/tmp/context.txt"}]}"#.into(),
            ),
            message_payload_json: r#"{"role":"user","text":"Continue safely."}"#.into(),
            action_answer: None,
            prepared_at: "2026-07-15T14:00:00Z".into(),
        }
    }

    #[test]
    fn continuation_preparation_is_atomic_and_idempotent() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-atomic";
        let run_id = "run-continuation-atomic";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Completed,
            None,
            "2026-07-15T13:59:00Z",
        )
        .expect("complete initial turn");

        let payload_hash = "a".repeat(64);
        let record =
            continuation_preparation(project_id, run_id, "continuation-request-1", &payload_hash);
        let prepared =
            prepare_agent_continuation(&repo_root, &record).expect("prepare continuation");
        assert!(prepared.inserted);
        assert_eq!(prepared.snapshot.run.status, AgentRunStatus::Running);
        assert_eq!(prepared.snapshot.messages.len(), 1);
        assert_eq!(prepared.snapshot.messages[0].attachments.len(), 1);
        assert_eq!(prepared.snapshot.events.len(), 2);

        let retry = prepare_agent_continuation(&repo_root, &record).expect("retry same request");
        assert!(!retry.inserted);
        assert_eq!(retry.request.message_id, prepared.request.message_id);
        assert_eq!(retry.snapshot.messages.len(), 1);
        assert_eq!(retry.snapshot.events.len(), 2);

        let conflict = prepare_agent_continuation(
            &repo_root,
            &NewAgentContinuationPreparationRecord {
                payload_hash: "b".repeat(64),
                ..record
            },
        )
        .expect_err("same request id with different payload must fail");
        assert_eq!(conflict.code, "agent_continuation_request_conflict");
        let snapshot = load_agent_run(&repo_root, project_id, run_id).expect("load after conflict");
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.events.len(), 2);
    }

    #[test]
    fn action_approval_and_continuation_preparation_commit_and_replay_together() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-action-atomic";
        let run_id = "run-continuation-action-atomic";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Paused,
            None,
            "2026-07-15T13:59:00Z",
        )
        .expect("pause run");
        append_action(&repo_root, project_id, run_id, "action-a");

        let mut record = continuation_preparation(
            project_id,
            run_id,
            "continuation-action-request",
            &"c".repeat(64),
        );
        record.action_answer = Some(AgentContinuationActionAnswerRecord {
            action_id: Some("action-a".into()),
            response: "Approved.".into(),
        });
        record.attachments[0].size_bytes = -1;
        prepare_agent_continuation(&repo_root, &record)
            .expect_err("a later preparation failure must roll back the approval");
        let rolled_back = load_agent_run(&repo_root, project_id, run_id).expect("load rollback");
        assert_eq!(rolled_back.run.status, AgentRunStatus::Paused);
        assert_eq!(rolled_back.action_requests[0].status, "pending");
        assert!(rolled_back.messages.is_empty());

        record.attachments[0].size_bytes = 12;
        let prepared = prepare_agent_continuation(&repo_root, &record).expect("prepare approval");
        assert_eq!(prepared.snapshot.run.status, AgentRunStatus::Running);
        assert_eq!(prepared.snapshot.action_requests[0].status, "answered");
        assert_eq!(
            prepared.snapshot.action_requests[0].response.as_deref(),
            Some("Approved.")
        );
        assert_eq!(prepared.snapshot.events.len(), 3);
        let replay = prepare_agent_continuation(&repo_root, &record).expect("replay approval");
        assert!(!replay.inserted);
        assert_eq!(replay.snapshot.events.len(), 3);
        assert_eq!(replay.snapshot.messages.len(), 1);
    }

    #[test]
    fn failed_continuation_preparation_rolls_back_and_can_retry() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-rollback";
        let run_id = "run-continuation-rollback";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Completed,
            None,
            "2026-07-15T13:59:00Z",
        )
        .expect("complete initial turn");

        let payload_hash = "c".repeat(64);
        let mut record = continuation_preparation(
            project_id,
            run_id,
            "continuation-request-rollback",
            &payload_hash,
        );
        record.attachments[0].size_bytes = -1;
        let error = prepare_agent_continuation(&repo_root, &record)
            .expect_err("invalid attachment must abort the transaction");
        assert_eq!(error.code, "agent_message_attachment_invalid_size");

        let after_failure = load_agent_run(&repo_root, project_id, run_id).expect("load rollback");
        assert_eq!(after_failure.run.status, AgentRunStatus::Completed);
        assert!(after_failure.messages.is_empty());
        assert!(after_failure.events.is_empty());
        assert!(load_agent_continuation_preparation(
            &repo_root,
            project_id,
            run_id,
            "continuation-request-rollback",
            &payload_hash,
        )
        .expect("inspect rollback")
        .is_none());

        record.attachments[0].size_bytes = 12;
        let retry = prepare_agent_continuation(&repo_root, &record)
            .expect("retry valid preparation after rollback");
        assert!(retry.inserted);
        assert_eq!(retry.snapshot.run.status, AgentRunStatus::Running);
        assert_eq!(retry.snapshot.messages.len(), 1);
        assert_eq!(retry.snapshot.events.len(), 2);
    }

    #[test]
    fn driving_continuation_is_not_reset_by_a_terminal_failure_without_provider_evidence() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-startup-recovery";
        let run_id = "run-continuation-startup-recovery";
        create_project_database(&repo_root, project_id);
        seed_running_run(&repo_root, project_id, run_id);
        let payload_hash = "d".repeat(64);
        let record = continuation_preparation(
            project_id,
            run_id,
            "continuation-startup-request",
            &payload_hash,
        );
        prepare_agent_continuation(&repo_root, &record).expect("prepare continuation");
        assert!(matches!(
            mark_agent_continuation_drive_started(
                &repo_root,
                project_id,
                run_id,
                &record.request_id,
                "2026-07-15T14:01:00Z",
            )
            .expect("mark drive started"),
            AgentContinuationDriveStartResult::Started(_)
        ));
        assert!(matches!(
            mark_agent_continuation_drive_started(
                &repo_root,
                project_id,
                run_id,
                &record.request_id,
                "2026-07-15T14:01:30Z",
            )
            .expect("observe already-driving continuation"),
            AgentContinuationDriveStartResult::AlreadyDriving(_)
        ));
        update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Failed,
            Some(AgentRunDiagnosticRecord {
                code: "agent_environment_startup_failed".into(),
                message: "Environment startup failed.".into(),
            }),
            "2026-07-15T14:02:00Z",
        )
        .expect("record startup failure");
        finish_agent_continuation_drive(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            false,
            "2026-07-15T14:03:00Z",
        )
        .expect("leave ambiguous drive unchanged");

        let retry = prepare_agent_continuation(&repo_root, &record).expect("recover request");
        assert_eq!(retry.request.state, AgentContinuationRequestState::Driving);
        assert!(retry.request.consumed_at.is_none());
        assert_eq!(retry.snapshot.messages.len(), 1);
    }

    #[test]
    fn driving_continuation_reconciles_only_from_request_scoped_completion_evidence() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-completion-recovery";
        let run_id = "run-continuation-completion-recovery";
        create_project_database(&repo_root, project_id);
        seed_running_run(&repo_root, project_id, run_id);
        let record = continuation_preparation(
            project_id,
            run_id,
            "continuation-completion-request",
            &"e".repeat(64),
        );
        prepare_agent_continuation(&repo_root, &record).expect("prepare continuation");
        mark_agent_continuation_drive_started(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            "2026-07-15T15:00:00Z",
        )
        .expect("mark driving");

        let before_completion = reconcile_completed_agent_continuation(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            "2026-07-15T15:01:00Z",
        )
        .expect("reconcile before completion")
        .expect("request exists");
        assert_eq!(
            before_completion.state,
            AgentContinuationRequestState::Driving
        );

        append_agent_event(
            &repo_root,
            &NewAgentEventRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                event_kind: AgentRunEventKind::RunCompleted,
                payload_json: r#"{"summary":"completed"}"#.into(),
                created_at: "2026-07-15T15:02:00Z".into(),
            },
        )
        .expect("append completion evidence");
        update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Completed,
            None,
            "2026-07-15T15:02:01Z",
        )
        .expect("complete run");

        let reconciled = reconcile_completed_agent_continuation(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            "2026-07-15T15:03:00Z",
        )
        .expect("reconcile completion")
        .expect("request exists");
        assert_eq!(reconciled.state, AgentContinuationRequestState::Consumed);
        assert_eq!(
            reconciled.consumed_at.as_deref(),
            Some("2026-07-15T15:03:00Z")
        );
    }

    #[test]
    fn paused_action_required_turn_reconciles_a_driving_continuation_as_consumed() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-paused-recovery";
        let run_id = "run-continuation-paused-recovery";
        create_project_database(&repo_root, project_id);
        seed_running_run(&repo_root, project_id, run_id);
        let record = continuation_preparation(
            project_id,
            run_id,
            "continuation-paused-request",
            &"9".repeat(64),
        );
        prepare_agent_continuation(&repo_root, &record).expect("prepare continuation");
        mark_agent_continuation_drive_started(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            "2026-07-15T15:10:00Z",
        )
        .expect("mark driving");
        append_agent_event(
            &repo_root,
            &NewAgentEventRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                event_kind: AgentRunEventKind::ActionRequired,
                payload_json: r#"{"actionId":"approval-1"}"#.into(),
                created_at: "2026-07-15T15:11:00Z".into(),
            },
        )
        .expect("append provider outcome");
        update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Paused,
            None,
            "2026-07-15T15:11:01Z",
        )
        .expect("pause run");

        let reconciled = reconcile_completed_agent_continuation(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            "2026-07-15T15:12:00Z",
        )
        .expect("reconcile paused outcome")
        .expect("request exists");
        assert_eq!(reconciled.state, AgentContinuationRequestState::Consumed);
    }

    #[test]
    fn failed_run_never_reconciles_a_driving_continuation_as_consumed() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "continuation-failed-recovery";
        let run_id = "run-continuation-failed-recovery";
        create_project_database(&repo_root, project_id);
        seed_running_run(&repo_root, project_id, run_id);
        let record = continuation_preparation(
            project_id,
            run_id,
            "continuation-failed-request",
            &"f".repeat(64),
        );
        prepare_agent_continuation(&repo_root, &record).expect("prepare continuation");
        mark_agent_continuation_drive_started(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            "2026-07-15T16:00:00Z",
        )
        .expect("mark driving");
        append_agent_event(
            &repo_root,
            &NewAgentEventRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                event_kind: AgentRunEventKind::RunCompleted,
                payload_json: r#"{"summary":"misleading evidence"}"#.into(),
                created_at: "2026-07-15T16:01:00Z".into(),
            },
        )
        .expect("append completion event");
        update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Failed,
            Some(AgentRunDiagnosticRecord {
                code: "provider_failed".into(),
                message: "Provider failed after emitting an event.".into(),
            }),
            "2026-07-15T16:01:01Z",
        )
        .expect("fail run");

        let reconciled = reconcile_completed_agent_continuation(
            &repo_root,
            project_id,
            run_id,
            &record.request_id,
            "2026-07-15T16:02:00Z",
        )
        .expect("reconcile failed run")
        .expect("request exists");
        assert_eq!(reconciled.state, AgentContinuationRequestState::Driving);
        assert!(reconciled.consumed_at.is_none());
    }

    #[test]
    fn cancellation_cas_blocks_a_new_drive_after_success_and_detects_changed_ownership() {
        let temp = tempfile::tempdir().expect("temp dir");
        let repo_root = temp.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo");
        let project_id = "cancel-drive-cas";
        let run_id = "run-cancel-drive-cas";
        create_project_database(&repo_root, project_id);
        seed_run(&repo_root, project_id, run_id);
        assert_eq!(
            claim_agent_run_drive_lease(
                &repo_root,
                project_id,
                run_id,
                "owner-1",
                101,
                "owner-1-process-birth",
                "drive-1",
                "2026-07-15T14:00:00Z",
            )
            .expect("claim drive"),
            AgentRunDriveLeaseClaimResult::Acquired
        );
        let raced = cancel_agent_run_with_expected_drive_lease(
            &repo_root,
            project_id,
            run_id,
            None,
            false,
            r#"{"code":"agent_run_cancelled"}"#,
            "2026-07-15T14:01:00Z",
        )
        .expect("observe changed lease");
        assert!(matches!(
            raced,
            AgentRunCancellationCasResult::LeaseChanged(Some(_))
        ));
        let lease = load_agent_run_drive_lease(&repo_root, project_id, run_id)
            .expect("load lease")
            .expect("held lease");
        let applied = cancel_agent_run_with_expected_drive_lease(
            &repo_root,
            project_id,
            run_id,
            Some(&lease),
            false,
            r#"{"code":"agent_run_cancelled"}"#,
            "2026-07-15T14:02:00Z",
        )
        .expect("cancel with matching lease");
        assert!(matches!(
            applied,
            AgentRunCancellationCasResult::Applied {
                transitioned: true,
                ..
            }
        ));
        let late_accept = update_agent_run_status(
            &repo_root,
            project_id,
            run_id,
            AgentRunStatus::Running,
            None,
            "2026-07-15T14:02:30Z",
        )
        .expect("late queued-prompt acceptance observes cancellation");
        assert_eq!(late_accept.run.status, AgentRunStatus::Cancelled);
        assert!(matches!(
            claim_agent_run_drive_lease(
                &repo_root,
                project_id,
                run_id,
                "owner-2",
                102,
                "owner-2-process-birth",
                "drive-2",
                "2026-07-15T14:03:00Z",
            )
            .expect("cancelled claim result"),
            AgentRunDriveLeaseClaimResult::RunNotDrivable(AgentRunStatus::Cancelled)
        ));
    }
}
