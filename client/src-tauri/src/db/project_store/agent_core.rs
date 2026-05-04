use std::path::Path;

use rusqlite::{params, OptionalExtension};
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
    VerificationGate,
    ActionRequired,
    RunPaused,
    RunCompleted,
    RunFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentToolCallState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
pub struct AgentMessageRecord {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub role: AgentMessageRole,
    pub content: String,
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
pub struct AgentUsageRecord {
    pub project_id: String,
    pub run_id: String,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
    pub estimated_cost_micros: u64,
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
pub struct NewAgentMessageRecord {
    pub project_id: String,
    pub run_id: String,
    pub role: AgentMessageRole,
    pub content: String,
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
                provider_id,
                model_id,
                status,
                prompt,
                system_prompt,
                started_at,
                last_heartbeat_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'starting', ?9, ?10, ?11, ?11, ?11)
            "#,
            params![
                selection.runtime_agent_id.as_str(),
                selection.definition_id,
                selection.version,
                record.project_id,
                record.agent_session_id,
                record.run_id,
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

    load_agent_run(repo_root, &record.project_id, &record.run_id)
}

pub fn append_agent_message(
    repo_root: &Path,
    record: &NewAgentMessageRecord,
) -> Result<AgentMessageRecord, CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.content, "content")?;
    let mut connection = open_agent_database(repo_root)?;
    let transaction = connection.transaction().map_err(|error| {
        map_agent_store_write_error(repo_root, "agent_message_transaction_failed", error)
    })?;
    transaction
        .execute(
            r#"
            INSERT INTO agent_messages (project_id, run_id, role, content, created_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                record.project_id,
                record.run_id,
                agent_message_role_sql_value(&record.role),
                record.content,
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
        created_at: record.created_at.clone(),
        attachments: stored_attachments,
    })
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
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.path, "path")?;
    validate_non_empty_text(&record.operation, "operation")?;
    validate_optional_sha256(record.old_hash.as_deref(), "oldHash")?;
    validate_optional_sha256(record.new_hash.as_deref(), "newHash")?;

    let connection = open_agent_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_file_changes (
                project_id,
                run_id,
                path,
                operation,
                old_hash,
                new_hash,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                record.project_id,
                record.run_id,
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

    let id = connection.last_insert_rowid();
    Ok(AgentFileChangeRecord {
        id,
        project_id: record.project_id.clone(),
        run_id: record.run_id.clone(),
        path: record.path.clone(),
        operation: record.operation.clone(),
        old_hash: record.old_hash.clone(),
        new_hash: record.new_hash.clone(),
        created_at: record.created_at.clone(),
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
                output_tokens,
                total_tokens,
                cache_read_tokens,
                cache_creation_tokens,
                estimated_cost_micros,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(project_id, run_id) DO UPDATE SET
                agent_definition_id = excluded.agent_definition_id,
                agent_definition_version = excluded.agent_definition_version,
                provider_id = excluded.provider_id,
                model_id = excluded.model_id,
                input_tokens = excluded.input_tokens,
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
    connection
        .execute(
            r#"
            UPDATE agent_runs
            SET status = ?3,
                last_heartbeat_at = ?4,
                completed_at = CASE WHEN ?3 IN ('completed', 'handed_off') THEN ?4 ELSE NULL END,
                cancelled_at = CASE WHEN ?3 = 'cancelled' THEN ?4 ELSE NULL END,
                last_error_code = ?5,
                last_error_message = ?6,
                updated_at = ?4
            WHERE project_id = ?1
              AND run_id = ?2
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
    load_agent_run(repo_root, project_id, run_id)
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

pub fn load_agent_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<AgentRunSnapshotRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_agent_database(repo_root)?;
    let run = connection
        .query_row(
            r#"
            SELECT
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                project_id,
                agent_session_id,
                run_id,
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
        })?;

    let messages = read_agent_messages(&connection, project_id, run_id, repo_root)?;
    let events = read_agent_events(&connection, project_id, run_id, repo_root)?;
    let tool_calls = read_agent_tool_calls(&connection, project_id, run_id, repo_root)?;
    let file_changes = read_agent_file_changes(&connection, project_id, run_id, repo_root)?;
    let checkpoints = read_agent_checkpoints(&connection, project_id, run_id, repo_root)?;
    let action_requests = read_agent_action_requests(&connection, project_id, run_id, repo_root)?;

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

/// Aggregate token + cost totals across every agent run for one project.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectUsageTotalsRecord {
    pub run_count: u64,
    pub input_tokens: u64,
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
    connection
        .query_row(
            r#"
            SELECT
                COUNT(*) AS run_count,
                COALESCE(SUM(input_tokens), 0) AS input_tokens,
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
                    output_tokens: read_nonnegative_u64(row, 2)?,
                    total_tokens: read_nonnegative_u64(row, 3)?,
                    cache_read_tokens: read_nonnegative_u64(row, 4)?,
                    cache_creation_tokens: read_nonnegative_u64(row, 5)?,
                    estimated_cost_micros: read_nonnegative_u64(row, 6)?,
                    last_updated_at: row.get::<_, Option<String>>(7)?,
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
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

/// List rows that need a cost recompute — rows priced at 0 but with non-zero
/// token activity. Existed pre-Phase-3 (or were written by ollama / unknown
/// models that legitimately price at 0; those will still resolve to 0 and
/// won't trigger a write).
pub fn list_unpriced_agent_usage_rows(
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
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cache_creation_tokens
            FROM agent_usage
            WHERE estimated_cost_micros = 0
              AND total_tokens > 0
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
                input_tokens: read_nonnegative_u64(row, 4)?,
                output_tokens: read_nonnegative_u64(row, 5)?,
                cache_read_tokens: read_nonnegative_u64(row, 6)?,
                cache_creation_tokens: read_nonnegative_u64(row, 7)?,
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
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                provider_id,
                model_id,
                COUNT(*) AS run_count,
                COALESCE(SUM(input_tokens), 0) AS input_tokens,
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
                output_tokens: read_nonnegative_u64(row, 4)?,
                total_tokens: read_nonnegative_u64(row, 5)?,
                cache_read_tokens: read_nonnegative_u64(row, 6)?,
                cache_creation_tokens: read_nonnegative_u64(row, 7)?,
                estimated_cost_micros: read_nonnegative_u64(row, 8)?,
                last_updated_at: row.get::<_, Option<String>>(9)?,
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
    let runs = list_agent_runs_for_session(repo_root, project_id, agent_session_id)?;
    let mut snapshots = Vec::with_capacity(runs.len());
    for run in runs {
        let usage = load_agent_usage(repo_root, project_id, &run.run_id)?;
        let snapshot = load_agent_run(repo_root, project_id, &run.run_id)?;
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

fn list_agent_runs_for_session(
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

fn validate_optional_sha256(value: Option<&str>, field: &'static str) -> Result<(), CommandError> {
    match value {
        Some(value) if is_lowercase_sha256(value) => Ok(()),
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

fn read_agent_messages(
    connection: &rusqlite::Connection,
    project_id: &str,
    run_id: &str,
    repo_root: &Path,
) -> Result<Vec<AgentMessageRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT id, project_id, run_id, role, content, created_at
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
                created_at: row.get(5)?,
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
            SELECT id, project_id, run_id, path, operation, old_hash, new_hash, created_at
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
                path: row.get(3)?,
                operation: row.get(4)?,
                old_hash: row.get(5)?,
                new_hash: row.get(6)?,
                created_at: row.get(7)?,
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

fn read_agent_run_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AgentRunRecord> {
    let last_error_code: Option<String> = row.get(15)?;
    let last_error_message: Option<String> = row.get(16)?;
    Ok(AgentRunRecord {
        runtime_agent_id: parse_runtime_agent_id(row.get::<_, String>(0)?.as_str()),
        agent_definition_id: row.get(1)?,
        agent_definition_version: read_positive_u32(row, 2)?,
        project_id: row.get(3)?,
        agent_session_id: row.get(4)?,
        run_id: row.get(5)?,
        provider_id: row.get(6)?,
        model_id: row.get(7)?,
        status: parse_agent_run_status(row.get::<_, String>(8)?.as_str()),
        prompt: row.get(9)?,
        system_prompt: row.get(10)?,
        started_at: row.get(11)?,
        last_heartbeat_at: row.get(12)?,
        completed_at: row.get(13)?,
        cancelled_at: row.get(14)?,
        last_error: match (last_error_code, last_error_message) {
            (Some(code), Some(message)) => Some(AgentRunDiagnosticRecord { code, message }),
            _ => None,
        },
        updated_at: row.get(17)?,
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
        output_tokens: read_nonnegative_u64(row, 7)?,
        total_tokens: read_nonnegative_u64(row, 8)?,
        cache_read_tokens: read_nonnegative_u64(row, 9)?,
        cache_creation_tokens: read_nonnegative_u64(row, 10)?,
        estimated_cost_micros: read_nonnegative_u64(row, 11)?,
        updated_at: row.get(12)?,
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
        AgentRunEventKind::VerificationGate => "verification_gate",
        AgentRunEventKind::ActionRequired => "action_required",
        AgentRunEventKind::RunPaused => "run_paused",
        AgentRunEventKind::RunCompleted => "run_completed",
        AgentRunEventKind::RunFailed => "run_failed",
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

fn parse_runtime_agent_id(value: &str) -> RuntimeAgentIdDto {
    match value {
        "engineer" => RuntimeAgentIdDto::Engineer,
        "debug" => RuntimeAgentIdDto::Debug,
        "agent_create" => RuntimeAgentIdDto::AgentCreate,
        _ => RuntimeAgentIdDto::Ask,
    }
}

fn parse_agent_event_kind(value: &str) -> AgentRunEventKind {
    match value {
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
        "verification_gate" => AgentRunEventKind::VerificationGate,
        "action_required" => AgentRunEventKind::ActionRequired,
        "run_paused" => AgentRunEventKind::RunPaused,
        "run_completed" => AgentRunEventKind::RunCompleted,
        "run_failed" => AgentRunEventKind::RunFailed,
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
