use std::path::Path;

use rusqlite::{params, Connection, Error as SqlError};
use serde::{Deserialize, Serialize};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    commands::{
        CommandError, OperatorApprovalDto, ProviderModelThinkingEffortDto, RuntimeAuthPhase,
        RuntimeRunApprovalModeDto,
    },
    db::database_path_for_repo,
};

use super::{open_runtime_database, read_project_row, validate_non_empty_text};

const MAX_RUNTIME_RUN_CHECKPOINT_ROWS: i64 = 32;
const MAX_RUNTIME_RUN_CHECKPOINT_SUMMARY_CHARS: usize = 280;
const RUNTIME_RUN_STALE_AFTER_SECONDS: i64 = 45;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionDiagnosticRecord {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeSessionRecord {
    pub project_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub flow_id: Option<String>,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub auth_phase: RuntimeAuthPhase,
    pub last_error: Option<RuntimeSessionDiagnosticRecord>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeRunStatus {
    Starting,
    Running,
    Stale,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeRunTransportLiveness {
    Unknown,
    Reachable,
    Unreachable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeRunCheckpointKind {
    Bootstrap,
    State,
    Tool,
    ActionRequired,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunTransportRecord {
    pub kind: String,
    pub endpoint: String,
    pub liveness: RuntimeRunTransportLiveness,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunDiagnosticRecord {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunActiveControlSnapshotRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
    pub approval_mode: RuntimeRunApprovalModeDto,
    #[serde(default)]
    pub plan_mode_required: bool,
    pub revision: u32,
    pub applied_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunPendingControlSnapshotRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
    pub approval_mode: RuntimeRunApprovalModeDto,
    #[serde(default)]
    pub plan_mode_required: bool,
    pub revision: u32,
    pub queued_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_prompt_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunControlStateRecord {
    pub active: RuntimeRunActiveControlSnapshotRecord,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<RuntimeRunPendingControlSnapshotRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub supervisor_kind: String,
    pub status: RuntimeRunStatus,
    pub transport: RuntimeRunTransportRecord,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub stopped_at: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticRecord>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunCheckpointRecord {
    pub project_id: String,
    pub run_id: String,
    pub sequence: u32,
    pub kind: RuntimeRunCheckpointKind,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunUpsertRecord {
    pub run: RuntimeRunRecord,
    pub checkpoint: Option<RuntimeRunCheckpointRecord>,
    pub control_state: Option<RuntimeRunControlStateRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRouteUpsertRecord {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub route_target: String,
    pub enabled: bool,
    pub metadata_json: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationRouteRecord {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub route_target: String,
    pub enabled: bool,
    pub metadata_json: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationDispatchStatus {
    Pending,
    Sent,
    Failed,
    Claimed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDispatchEnqueueRecord {
    pub project_id: String,
    pub action_id: String,
    pub enqueued_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDispatchOutcomeUpdateRecord {
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub status: NotificationDispatchStatus,
    pub attempted_at: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationDispatchRecord {
    pub id: i64,
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub status: NotificationDispatchStatus,
    pub attempt_count: u32,
    pub last_attempt_at: Option<String>,
    pub delivered_at: Option<String>,
    pub claimed_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationReplyClaimStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationReplyClaimRequestRecord {
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub responder_id: Option<String>,
    pub reply_text: String,
    pub received_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationReplyClaimRecord {
    pub id: i64,
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub responder_id: Option<String>,
    pub reply_text: String,
    pub status: NotificationReplyClaimStatus,
    pub rejection_code: Option<String>,
    pub rejection_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationReplyClaimResultRecord {
    pub claim: NotificationReplyClaimRecord,
    pub dispatch: NotificationDispatchRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActionRequiredUpsertRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub transport_endpoint: String,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticRecord>,
    pub boundary_id: String,
    pub action_type: String,
    pub title: String,
    pub detail: String,
    pub checkpoint_summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeActionRequiredPersistedRecord {
    pub approval_request: OperatorApprovalDto,
    pub runtime_run: RuntimeRunSnapshotRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeRunSnapshotRecord {
    pub run: RuntimeRunRecord,
    pub controls: RuntimeRunControlStateRecord,
    pub checkpoints: Vec<RuntimeRunCheckpointRecord>,
    pub last_checkpoint_sequence: u32,
    pub last_checkpoint_at: Option<String>,
}

#[derive(Debug)]
pub(crate) struct StoredRuntimeRunRow {
    pub(crate) run_id: String,
    pub(crate) runtime_kind: String,
    pub(crate) provider_id: String,
    pub(crate) last_checkpoint_sequence: u32,
    pub(crate) last_checkpoint_at: Option<String>,
    pub(crate) control_state_json: Option<String>,
}

#[derive(Debug)]
struct RawRuntimeRunRow {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    runtime_kind: String,
    provider_id: String,
    supervisor_kind: String,
    status: String,
    transport_kind: String,
    transport_endpoint: String,
    transport_liveness: String,
    control_state_json: Option<String>,
    last_checkpoint_sequence: i64,
    started_at: String,
    last_heartbeat_at: Option<String>,
    last_checkpoint_at: Option<String>,
    stopped_at: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    updated_at: String,
}

#[derive(Debug)]
struct RawRuntimeRunCheckpointRow {
    project_id: String,
    run_id: String,
    sequence: i64,
    kind: String,
    summary: String,
    created_at: String,
}

pub fn load_runtime_session(
    repo_root: &Path,
    expected_project_id: &str,
) -> Result<Option<RuntimeSessionRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;
    read_runtime_session_row(&connection, &database_path, expected_project_id)
}

pub fn upsert_runtime_session(
    repo_root: &Path,
    session: &RuntimeSessionRecord,
) -> Result<RuntimeSessionRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &session.project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "runtime_session_transaction_failed",
            format!(
                "Cadence could not start the runtime-session transaction for {}: {error}",
                database_path.display()
            ),
        )
    })?;

    transaction
        .execute(
            r#"
            UPDATE projects
            SET runtime = ?2,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?1
            "#,
            params![session.project_id, session.runtime_kind],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "runtime_project_update_failed",
                format!(
                    "Cadence could not persist runtime kind for project `{}` in {}: {error}",
                    session.project_id,
                    database_path.display()
                ),
            )
        })?;

    let (last_error_code, last_error_message, last_error_retryable) = session
        .last_error
        .as_ref()
        .map(|error| {
            (
                Some(error.code.as_str()),
                Some(error.message.as_str()),
                Some(if error.retryable { 1_i64 } else { 0_i64 }),
            )
        })
        .unwrap_or((None, None, None));

    transaction
        .execute(
            r#"
            INSERT INTO runtime_sessions (
                project_id,
                runtime_kind,
                provider_id,
                flow_id,
                session_id,
                account_id,
                auth_phase,
                last_error_code,
                last_error_message,
                last_error_retryable,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            ON CONFLICT(project_id) DO UPDATE SET
                runtime_kind = excluded.runtime_kind,
                provider_id = excluded.provider_id,
                flow_id = excluded.flow_id,
                session_id = excluded.session_id,
                account_id = excluded.account_id,
                auth_phase = excluded.auth_phase,
                last_error_code = excluded.last_error_code,
                last_error_message = excluded.last_error_message,
                last_error_retryable = excluded.last_error_retryable,
                updated_at = excluded.updated_at
            "#,
            params![
                session.project_id,
                session.runtime_kind,
                session.provider_id,
                session.flow_id,
                session.session_id,
                session.account_id,
                runtime_auth_phase_sql_value(&session.auth_phase),
                last_error_code,
                last_error_message,
                last_error_retryable,
                session.updated_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "runtime_session_persist_failed",
                format!(
                    "Cadence could not persist runtime-session metadata in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "runtime_session_commit_failed",
            format!(
                "Cadence could not commit runtime-session metadata in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    read_runtime_session_row(&connection, &database_path, &session.project_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "runtime_session_missing_after_persist",
            format!(
                "Cadence persisted runtime-session metadata in {} but could not read it back.",
                database_path.display()
            ),
        )
    })
}

pub fn load_runtime_run(
    repo_root: &Path,
    expected_project_id: &str,
    expected_agent_session_id: &str,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_runtime_run_transaction_error(
            "runtime_run_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the durable runtime-run read transaction.",
        )
    })?;

    let snapshot = read_runtime_run_snapshot(
        &transaction,
        &database_path,
        expected_project_id,
        expected_agent_session_id,
    )?;
    transaction.rollback().map_err(|error| {
        map_runtime_run_commit_error(
            "runtime_run_commit_failed",
            &database_path,
            error,
            "Cadence could not close the durable runtime-run read transaction.",
        )
    })?;

    Ok(snapshot)
}

pub fn upsert_runtime_run(
    repo_root: &Path,
    payload: &RuntimeRunUpsertRecord,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    validate_runtime_run_upsert_payload(payload)?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(
        &connection,
        &database_path,
        repo_root,
        &payload.run.project_id,
    )?;

    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_runtime_run_transaction_error(
            "runtime_run_transaction_failed",
            &database_path,
            error,
            "Cadence could not start the durable runtime-run transaction.",
        )
    })?;

    super::touch_agent_session_runtime_run(
        &transaction,
        &database_path,
        &payload.run.project_id,
        &payload.run.agent_session_id,
        &payload.run.run_id,
        &payload.run.runtime_kind,
        &payload.run.provider_id,
        &payload.run.updated_at,
    )?;

    let existing = read_runtime_run_row(
        &transaction,
        &database_path,
        &payload.run.project_id,
        &payload.run.agent_session_id,
    )?;
    let existing_run_id = existing.as_ref().map(|row| row.run_id.as_str());
    let existing_last_checkpoint_sequence = existing
        .as_ref()
        .map_or(0_u32, |row| row.last_checkpoint_sequence);
    let existing_last_checkpoint_at = existing
        .as_ref()
        .and_then(|row| row.last_checkpoint_at.clone());
    let existing_control_state_json = existing
        .as_ref()
        .and_then(|row| row.control_state_json.clone());

    if let Some(run_id) = existing_run_id.filter(|run_id| *run_id != payload.run.run_id.as_str()) {
        transaction
            .execute(
                "DELETE FROM runtime_run_checkpoints WHERE project_id = ?1 AND run_id = ?2",
                params![payload.run.project_id.as_str(), run_id],
            )
            .map_err(|error| {
                map_runtime_run_write_error(
                    "runtime_run_checkpoint_reset_failed",
                    &database_path,
                    error,
                    "Cadence could not clear the prior runtime-run checkpoints before rotating the run id.",
                )
            })?;
    }

    if let Some(checkpoint) = payload.checkpoint.as_ref() {
        if existing_run_id.is_some_and(|run_id| run_id == payload.run.run_id.as_str())
            && checkpoint.sequence <= existing_last_checkpoint_sequence
        {
            return Err(CommandError::system_fault(
                "runtime_run_checkpoint_sequence_invalid",
                format!(
                    "Cadence refused to persist runtime-run checkpoint sequence {} for run `{}` because the prior durable sequence is {} in {}.",
                    checkpoint.sequence,
                    payload.run.run_id,
                    existing_last_checkpoint_sequence,
                    database_path.display()
                ),
            ));
        }
    }

    let (last_checkpoint_sequence, last_checkpoint_at) = match payload.checkpoint.as_ref() {
        Some(checkpoint) => (checkpoint.sequence, Some(checkpoint.created_at.as_str())),
        None if existing_run_id.is_some_and(|run_id| run_id == payload.run.run_id.as_str()) => (
            existing_last_checkpoint_sequence,
            existing_last_checkpoint_at.as_deref(),
        ),
        None => (0_u32, None),
    };

    let control_state_json = match payload.control_state.as_ref() {
        Some(control_state) => Some(serialize_runtime_run_control_state(control_state)?),
        None if existing_run_id.is_some_and(|run_id| run_id == payload.run.run_id.as_str()) => {
            Some(existing_control_state_json.clone().ok_or_else(|| {
                CommandError::system_fault(
                    "runtime_run_control_state_missing",
                    format!(
                        "Cadence refused to rewrite runtime-run `{}` in {} because the durable control snapshot was missing.",
                        payload.run.run_id,
                        database_path.display()
                    ),
                )
            })?)
        }
        None => {
            return Err(CommandError::system_fault(
                "runtime_run_control_state_missing",
                format!(
                    "Cadence requires a durable control snapshot before it can persist runtime-run `{}` in {}.",
                    payload.run.run_id,
                    database_path.display()
                ),
            ))
        }
    };

    let (last_error_code, last_error_message) = payload
        .run
        .last_error
        .as_ref()
        .map(|error| (Some(error.code.as_str()), Some(error.message.as_str())))
        .unwrap_or((None, None));

    transaction
        .execute(
            r#"
            INSERT INTO runtime_runs (
                project_id,
                agent_session_id,
                run_id,
                runtime_kind,
                provider_id,
                supervisor_kind,
                status,
                transport_kind,
                transport_endpoint,
                transport_liveness,
                control_state_json,
                last_checkpoint_sequence,
                started_at,
                last_heartbeat_at,
                last_checkpoint_at,
                stopped_at,
                last_error_code,
                last_error_message,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
                run_id = excluded.run_id,
                runtime_kind = excluded.runtime_kind,
                provider_id = excluded.provider_id,
                supervisor_kind = excluded.supervisor_kind,
                status = excluded.status,
                transport_kind = excluded.transport_kind,
                transport_endpoint = excluded.transport_endpoint,
                transport_liveness = excluded.transport_liveness,
                control_state_json = excluded.control_state_json,
                last_checkpoint_sequence = excluded.last_checkpoint_sequence,
                started_at = excluded.started_at,
                last_heartbeat_at = excluded.last_heartbeat_at,
                last_checkpoint_at = excluded.last_checkpoint_at,
                stopped_at = excluded.stopped_at,
                last_error_code = excluded.last_error_code,
                last_error_message = excluded.last_error_message,
                updated_at = excluded.updated_at
            "#,
            params![
                payload.run.project_id.as_str(),
                payload.run.agent_session_id.as_str(),
                payload.run.run_id.as_str(),
                payload.run.runtime_kind.as_str(),
                payload.run.provider_id.as_str(),
                payload.run.supervisor_kind.as_str(),
                runtime_run_status_sql_value(&payload.run.status),
                payload.run.transport.kind.as_str(),
                payload.run.transport.endpoint.as_str(),
                runtime_run_transport_liveness_sql_value(&payload.run.transport.liveness),
                control_state_json.as_deref(),
                i64::from(last_checkpoint_sequence),
                payload.run.started_at.as_str(),
                payload.run.last_heartbeat_at.as_deref(),
                last_checkpoint_at,
                payload.run.stopped_at.as_deref(),
                last_error_code,
                last_error_message,
                payload.run.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_runtime_run_write_error(
                "runtime_run_persist_failed",
                &database_path,
                error,
                "Cadence could not persist the durable runtime-run row.",
            )
        })?;

    if let Some(checkpoint) = payload.checkpoint.as_ref() {
        transaction
            .execute(
                r#"
                INSERT INTO runtime_run_checkpoints (
                    project_id,
                    run_id,
                    sequence,
                    kind,
                    summary,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                "#,
                params![
                    checkpoint.project_id.as_str(),
                    checkpoint.run_id.as_str(),
                    i64::from(checkpoint.sequence),
                    runtime_run_checkpoint_kind_sql_value(&checkpoint.kind),
                    normalize_runtime_checkpoint_summary(&checkpoint.summary),
                    checkpoint.created_at.as_str(),
                ],
            )
            .map_err(|error| {
                map_runtime_run_write_error(
                    "runtime_run_checkpoint_persist_failed",
                    &database_path,
                    error,
                    "Cadence could not persist the durable runtime-run checkpoint.",
                )
            })?;
    }

    transaction.commit().map_err(|error| {
        map_runtime_run_commit_error(
            "runtime_run_commit_failed",
            &database_path,
            error,
            "Cadence could not commit the durable runtime-run transaction.",
        )
    })?;

    read_runtime_run_snapshot(
        &connection,
        &database_path,
        &payload.run.project_id,
        &payload.run.agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "runtime_run_missing_after_persist",
            format!(
                "Cadence persisted durable runtime-run metadata in {} but could not read it back.",
                database_path.display()
            ),
        )
    })
}

pub(crate) fn read_runtime_session_row(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
) -> Result<Option<RuntimeSessionRecord>, CommandError> {
    let row = connection.query_row(
        r#"
            SELECT
                project_id,
                runtime_kind,
                provider_id,
                flow_id,
                session_id,
                account_id,
                auth_phase,
                last_error_code,
                last_error_message,
                last_error_retryable,
                updated_at
            FROM runtime_sessions
            WHERE project_id = ?1
            "#,
        [expected_project_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, Option<String>>(7)?,
                row.get::<_, Option<String>>(8)?,
                row.get::<_, Option<i64>>(9)?,
                row.get::<_, String>(10)?,
            ))
        },
    );

    match row {
        Ok(row) => decode_runtime_session_row(row, database_path).map(Some),
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(other) => Err(CommandError::system_fault(
            "runtime_session_query_failed",
            format!(
                "Cadence could not read runtime-session metadata from {}: {other}",
                database_path.display()
            ),
        )),
    }
}

type RuntimeSessionRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<i64>,
    String,
);

fn decode_runtime_session_row(
    row: RuntimeSessionRow,
    database_path: &Path,
) -> Result<RuntimeSessionRecord, CommandError> {
    let (
        project_id,
        runtime_kind,
        provider_id,
        flow_id,
        session_id,
        account_id,
        auth_phase,
        last_error_code,
        last_error_message,
        last_error_retryable,
        updated_at,
    ) = row;

    let auth_phase = parse_runtime_auth_phase(&auth_phase).map_err(|message| {
        map_runtime_decode_error(database_path, format!("Field `auth_phase` {message}"))
    })?;

    let last_error = match (last_error_code, last_error_message, last_error_retryable) {
        (None, None, None) => None,
        (Some(code), Some(message), Some(retryable)) => Some(RuntimeSessionDiagnosticRecord {
            code,
            message,
            retryable: match retryable {
                0 => false,
                1 => true,
                other => {
                    return Err(map_runtime_decode_error(
                        database_path,
                        format!("Field `last_error_retryable` must be 0 or 1, found {other}."),
                    ))
                }
            },
        }),
        _ => {
            return Err(map_runtime_decode_error(
                database_path,
                "last_error fields must be all null or all populated.".into(),
            ))
        }
    };

    Ok(RuntimeSessionRecord {
        project_id,
        runtime_kind,
        provider_id,
        flow_id,
        session_id,
        account_id,
        auth_phase,
        last_error,
        updated_at,
    })
}

pub(crate) fn read_runtime_run_snapshot(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
    expected_agent_session_id: &str,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    let row = connection.query_row(
        r#"
            SELECT
                project_id,
                agent_session_id,
                run_id,
                runtime_kind,
                provider_id,
                supervisor_kind,
                status,
                transport_kind,
                transport_endpoint,
                transport_liveness,
                control_state_json,
                last_checkpoint_sequence,
                started_at,
                last_heartbeat_at,
                last_checkpoint_at,
                stopped_at,
                last_error_code,
                last_error_message,
                updated_at
            FROM runtime_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
            "#,
        params![expected_project_id, expected_agent_session_id],
        |row| {
            Ok(RawRuntimeRunRow {
                project_id: row.get(0)?,
                agent_session_id: row.get(1)?,
                run_id: row.get(2)?,
                runtime_kind: row.get(3)?,
                provider_id: row.get(4)?,
                supervisor_kind: row.get(5)?,
                status: row.get(6)?,
                transport_kind: row.get(7)?,
                transport_endpoint: row.get(8)?,
                transport_liveness: row.get(9)?,
                control_state_json: row.get(10)?,
                last_checkpoint_sequence: row.get(11)?,
                started_at: row.get(12)?,
                last_heartbeat_at: row.get(13)?,
                last_checkpoint_at: row.get(14)?,
                stopped_at: row.get(15)?,
                last_error_code: row.get(16)?,
                last_error_message: row.get(17)?,
                updated_at: row.get(18)?,
            })
        },
    );

    let raw_row = match row {
        Ok(row) => row,
        Err(SqlError::QueryReturnedNoRows) => return Ok(None),
        Err(other) => {
            return Err(CommandError::system_fault(
                "runtime_run_query_failed",
                format!(
                    "Cadence could not read durable runtime-run metadata from {}: {other}",
                    database_path.display()
                ),
            ))
        }
    };

    let checkpoints = read_runtime_run_checkpoints(
        connection,
        database_path,
        expected_project_id,
        raw_row.run_id.as_str(),
    )?;
    let controls =
        decode_runtime_run_control_state(raw_row.control_state_json.clone(), database_path)?;
    let last_checkpoint_sequence = decode_runtime_run_checkpoint_sequence(
        raw_row.last_checkpoint_sequence,
        "last_checkpoint_sequence",
        database_path,
    )?;

    if checkpoints.is_empty() {
        if last_checkpoint_sequence != 0 || raw_row.last_checkpoint_at.is_some() {
            return Err(map_runtime_run_decode_error(
                database_path,
                "Runtime run reported checkpoint metadata but no durable checkpoint rows exist."
                    .into(),
            ));
        }
    } else {
        let latest_checkpoint = checkpoints
            .last()
            .expect("checked non-empty runtime run checkpoints");
        if latest_checkpoint.sequence != last_checkpoint_sequence {
            return Err(map_runtime_run_decode_error(
                database_path,
                format!(
                    "Runtime run reported last checkpoint sequence {} but durable checkpoint rows end at {}.",
                    last_checkpoint_sequence, latest_checkpoint.sequence
                ),
            ));
        }

        if raw_row.last_checkpoint_at.as_deref() != Some(latest_checkpoint.created_at.as_str()) {
            return Err(map_runtime_run_decode_error(
                database_path,
                "Runtime run reported a last checkpoint timestamp that does not match the latest durable checkpoint row.".into(),
            ));
        }
    }

    let snapshot_last_checkpoint_at = raw_row.last_checkpoint_at.clone();

    Ok(Some(RuntimeRunSnapshotRecord {
        run: decode_runtime_run_row(raw_row, database_path)?,
        controls,
        checkpoints,
        last_checkpoint_sequence,
        last_checkpoint_at: snapshot_last_checkpoint_at,
    }))
}

pub(crate) fn read_runtime_run_row(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
    expected_agent_session_id: &str,
) -> Result<Option<StoredRuntimeRunRow>, CommandError> {
    let row = connection.query_row(
        r#"
            SELECT
                run_id,
                runtime_kind,
                provider_id,
                last_checkpoint_sequence,
                last_checkpoint_at,
                control_state_json
            FROM runtime_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
            "#,
        params![expected_project_id, expected_agent_session_id],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
            ))
        },
    );

    match row {
        Ok((
            run_id,
            runtime_kind,
            provider_id,
            last_checkpoint_sequence,
            last_checkpoint_at,
            control_state_json,
        )) => {
            let run_id = require_runtime_run_non_empty_owned(run_id, "run_id", database_path)?;
            let runtime_kind =
                require_runtime_run_non_empty_owned(runtime_kind, "runtime_kind", database_path)?;
            let provider_id =
                require_runtime_run_non_empty_owned(provider_id, "provider_id", database_path)?;
            resolve_runtime_run_provider_identity(provider_id.as_str(), runtime_kind.as_str())
                .map_err(|diagnostic| {
                    map_runtime_run_decode_error(
                        database_path,
                        format!(
                            "Runtime run identity is invalid because {}",
                            diagnostic.message
                        ),
                    )
                })?;
            Ok(Some(StoredRuntimeRunRow {
                run_id,
                runtime_kind,
                provider_id,
                last_checkpoint_sequence: decode_runtime_run_checkpoint_sequence(
                    last_checkpoint_sequence,
                    "last_checkpoint_sequence",
                    database_path,
                )?,
                last_checkpoint_at: decode_runtime_run_optional_non_empty_text(
                    last_checkpoint_at,
                    "last_checkpoint_at",
                    database_path,
                )?,
                control_state_json,
            }))
        }
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(other) => Err(CommandError::system_fault(
            "runtime_run_query_failed",
            format!(
                "Cadence could not read durable runtime-run metadata from {}: {other}",
                database_path.display()
            ),
        )),
    }
}

fn read_runtime_run_checkpoints(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<RuntimeRunCheckpointRecord>, CommandError> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                run_id,
                sequence,
                kind,
                summary,
                created_at
            FROM runtime_run_checkpoints
            WHERE project_id = ?1
              AND run_id = ?2
            ORDER BY sequence ASC, created_at ASC, id ASC
            LIMIT ?3
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "runtime_run_checkpoint_query_failed",
                format!(
                    "Cadence could not prepare the durable runtime-run checkpoint query against {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let rows = statement
        .query_map(
            params![project_id, run_id, MAX_RUNTIME_RUN_CHECKPOINT_ROWS],
            |row| {
                Ok(RawRuntimeRunCheckpointRow {
                    project_id: row.get(0)?,
                    run_id: row.get(1)?,
                    sequence: row.get(2)?,
                    kind: row.get(3)?,
                    summary: row.get(4)?,
                    created_at: row.get(5)?,
                })
            },
        )
        .map_err(|error| {
            CommandError::system_fault(
                "runtime_run_checkpoint_query_failed",
                format!(
                    "Cadence could not query durable runtime-run checkpoints from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut checkpoints = Vec::new();
    let mut previous_sequence = 0_u32;
    for row in rows {
        let checkpoint = decode_runtime_run_checkpoint_row(
            row.map_err(|error| {
                CommandError::system_fault(
                    "runtime_run_checkpoint_query_failed",
                    format!(
                    "Cadence could not read a durable runtime-run checkpoint row from {}: {error}",
                    database_path.display()
                ),
                )
            })?,
            database_path,
        )?;

        if checkpoint.sequence <= previous_sequence {
            return Err(map_runtime_run_checkpoint_decode_error(
                database_path,
                format!(
                    "Runtime run checkpoints must increase monotonically, but sequence {} followed {}.",
                    checkpoint.sequence, previous_sequence
                ),
            ));
        }

        previous_sequence = checkpoint.sequence;
        checkpoints.push(checkpoint);
    }

    Ok(checkpoints)
}

fn decode_runtime_run_row(
    raw_row: RawRuntimeRunRow,
    database_path: &Path,
) -> Result<RuntimeRunRecord, CommandError> {
    let project_id =
        require_runtime_run_non_empty_owned(raw_row.project_id, "project_id", database_path)?;
    let agent_session_id = require_runtime_run_non_empty_owned(
        raw_row.agent_session_id,
        "agent_session_id",
        database_path,
    )?;
    let run_id = require_runtime_run_non_empty_owned(raw_row.run_id, "run_id", database_path)?;
    let runtime_kind =
        require_runtime_run_non_empty_owned(raw_row.runtime_kind, "runtime_kind", database_path)?;
    let provider_id =
        require_runtime_run_non_empty_owned(raw_row.provider_id, "provider_id", database_path)?;
    resolve_runtime_run_provider_identity(provider_id.as_str(), runtime_kind.as_str()).map_err(
        |diagnostic| {
            map_runtime_run_decode_error(
                database_path,
                format!(
                    "Runtime run provider identity is invalid because {}",
                    diagnostic.message
                ),
            )
        },
    )?;
    let supervisor_kind = require_runtime_run_non_empty_owned(
        raw_row.supervisor_kind,
        "supervisor_kind",
        database_path,
    )?;
    let transport_kind = require_runtime_run_non_empty_owned(
        raw_row.transport_kind,
        "transport_kind",
        database_path,
    )?;
    let transport_endpoint = require_runtime_run_non_empty_owned(
        raw_row.transport_endpoint,
        "transport_endpoint",
        database_path,
    )?;
    let started_at =
        require_runtime_run_non_empty_owned(raw_row.started_at, "started_at", database_path)?;
    let last_heartbeat_at = decode_runtime_run_optional_non_empty_text(
        raw_row.last_heartbeat_at,
        "last_heartbeat_at",
        database_path,
    )?;
    let stopped_at = decode_runtime_run_optional_non_empty_text(
        raw_row.stopped_at,
        "stopped_at",
        database_path,
    )?;
    let updated_at =
        require_runtime_run_non_empty_owned(raw_row.updated_at, "updated_at", database_path)?;

    let status = parse_runtime_run_status(&raw_row.status).map_err(|details| {
        map_runtime_run_decode_error(database_path, format!("Field `status` {details}"))
    })?;
    let transport_liveness = parse_runtime_run_transport_liveness(&raw_row.transport_liveness)
        .map_err(|details| {
            map_runtime_run_decode_error(
                database_path,
                format!("Field `transport_liveness` {details}"),
            )
        })?;

    let last_error = match (raw_row.last_error_code, raw_row.last_error_message) {
        (None, None) => None,
        (Some(code), Some(message)) => Some(RuntimeRunDiagnosticRecord {
            code: require_runtime_run_non_empty_owned(code, "last_error_code", database_path)?,
            message: require_runtime_run_non_empty_owned(
                message,
                "last_error_message",
                database_path,
            )?,
        }),
        _ => {
            return Err(map_runtime_run_decode_error(
                database_path,
                "Runtime run last_error fields must be all null or all populated.".into(),
            ))
        }
    };

    let status = derive_runtime_run_status(
        status,
        last_heartbeat_at.as_deref(),
        updated_at.as_str(),
        database_path,
    )?;

    Ok(RuntimeRunRecord {
        project_id,
        agent_session_id,
        run_id,
        runtime_kind,
        provider_id,
        supervisor_kind,
        status,
        transport: RuntimeRunTransportRecord {
            kind: transport_kind,
            endpoint: transport_endpoint,
            liveness: transport_liveness,
        },
        started_at,
        last_heartbeat_at,
        stopped_at,
        last_error,
        updated_at,
    })
}

fn decode_runtime_run_checkpoint_row(
    raw_row: RawRuntimeRunCheckpointRow,
    database_path: &Path,
) -> Result<RuntimeRunCheckpointRecord, CommandError> {
    Ok(RuntimeRunCheckpointRecord {
        project_id: require_runtime_run_checkpoint_non_empty_owned(
            raw_row.project_id,
            "project_id",
            database_path,
        )?,
        run_id: require_runtime_run_checkpoint_non_empty_owned(
            raw_row.run_id,
            "run_id",
            database_path,
        )?,
        sequence: decode_runtime_run_checkpoint_sequence(
            raw_row.sequence,
            "sequence",
            database_path,
        )?,
        kind: parse_runtime_run_checkpoint_kind(&raw_row.kind)
            .map_err(|details| map_runtime_run_checkpoint_decode_error(database_path, details))?,
        summary: require_runtime_run_checkpoint_non_empty_owned(
            raw_row.summary,
            "summary",
            database_path,
        )?,
        created_at: require_runtime_run_checkpoint_non_empty_owned(
            raw_row.created_at,
            "created_at",
            database_path,
        )?,
    })
}

pub(crate) fn validate_runtime_action_required_payload(
    payload: &RuntimeActionRequiredUpsertRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &payload.project_id,
        "project_id",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.agent_session_id,
        "agent_session_id",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(&payload.run_id, "run_id", "runtime_action_request_invalid")?;
    validate_non_empty_text(
        &payload.runtime_kind,
        "runtime_kind",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.session_id,
        "session_id",
        "runtime_action_request_invalid",
    )?;
    if let Some(flow_id) = payload.flow_id.as_deref() {
        validate_non_empty_text(flow_id, "flow_id", "runtime_action_request_invalid")?;
    }
    validate_non_empty_text(
        &payload.transport_endpoint,
        "transport_endpoint",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.started_at,
        "started_at",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.boundary_id,
        "boundary_id",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.action_type,
        "action_type",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(&payload.title, "title", "runtime_action_request_invalid")?;
    validate_non_empty_text(&payload.detail, "detail", "runtime_action_request_invalid")?;
    validate_non_empty_text(
        &payload.checkpoint_summary,
        "checkpoint_summary",
        "runtime_action_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.created_at,
        "created_at",
        "runtime_action_request_invalid",
    )?;

    for value in [
        payload.title.as_str(),
        payload.detail.as_str(),
        payload.checkpoint_summary.as_str(),
    ] {
        if let Some(secret_hint) = find_prohibited_runtime_persistence_content(value) {
            return Err(CommandError::user_fixable(
                "runtime_action_request_invalid",
                format!(
                    "Runtime action-required persistence must not include {secret_hint}. Remove secret-bearing content before retrying."
                ),
            ));
        }
    }

    if let Some(last_heartbeat_at) = payload.last_heartbeat_at.as_deref() {
        validate_non_empty_text(
            last_heartbeat_at,
            "last_heartbeat_at",
            "runtime_action_request_invalid",
        )?;
    }

    if let Some(last_error) = payload.last_error.as_ref() {
        validate_non_empty_text(
            &last_error.code,
            "last_error_code",
            "runtime_action_request_invalid",
        )?;
        validate_non_empty_text(
            &last_error.message,
            "last_error_message",
            "runtime_action_request_invalid",
        )?;
        if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&last_error.message)
        {
            return Err(CommandError::user_fixable(
                "runtime_action_request_invalid",
                format!(
                    "Runtime action-required diagnostics must not include {secret_hint}. Remove secret-bearing content before retrying."
                ),
            ));
        }
    }

    Ok(())
}

fn validate_runtime_run_upsert_payload(
    payload: &RuntimeRunUpsertRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &payload.run.project_id,
        "project_id",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.run.agent_session_id,
        "agent_session_id",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(&payload.run.run_id, "run_id", "runtime_run_request_invalid")?;
    validate_non_empty_text(
        &payload.run.runtime_kind,
        "runtime_kind",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.run.provider_id,
        "provider_id",
        "runtime_run_request_invalid",
    )?;
    resolve_runtime_run_provider_identity(
        payload.run.provider_id.as_str(),
        payload.run.runtime_kind.as_str(),
    )
    .map_err(|diagnostic| {
        CommandError::user_fixable(
            "runtime_run_request_invalid",
            format!(
                "Cadence rejected the durable runtime-run identity because {}",
                diagnostic.message
            ),
        )
    })?;
    validate_non_empty_text(
        &payload.run.supervisor_kind,
        "supervisor_kind",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.run.transport.kind,
        "transport_kind",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.run.transport.endpoint,
        "transport_endpoint",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.run.started_at,
        "started_at",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.run.updated_at,
        "updated_at",
        "runtime_run_request_invalid",
    )?;

    if let Some(last_heartbeat_at) = payload.run.last_heartbeat_at.as_deref() {
        validate_non_empty_text(
            last_heartbeat_at,
            "last_heartbeat_at",
            "runtime_run_request_invalid",
        )?;
    }

    if let Some(stopped_at) = payload.run.stopped_at.as_deref() {
        validate_non_empty_text(stopped_at, "stopped_at", "runtime_run_request_invalid")?;
    }

    if let Some(last_error) = payload.run.last_error.as_ref() {
        validate_non_empty_text(
            &last_error.code,
            "last_error_code",
            "runtime_run_request_invalid",
        )?;
        validate_non_empty_text(
            &last_error.message,
            "last_error_message",
            "runtime_run_request_invalid",
        )?;

        if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&last_error.message)
        {
            return Err(CommandError::user_fixable(
                "runtime_run_request_invalid",
                format!(
                    "Runtime-run diagnostics must not include {secret_hint}. Remove secret-bearing content before retrying."
                ),
            ));
        }
    }

    if let Some(control_state) = payload.control_state.as_ref() {
        validate_runtime_run_control_state(control_state)?;
    }

    if let Some(checkpoint) = payload.checkpoint.as_ref() {
        if checkpoint.project_id != payload.run.project_id {
            return Err(CommandError::system_fault(
                "runtime_run_checkpoint_invalid",
                "Cadence could not persist a runtime-run checkpoint whose project id does not match the parent run.",
            ));
        }

        if checkpoint.run_id != payload.run.run_id {
            return Err(CommandError::system_fault(
                "runtime_run_checkpoint_invalid",
                "Cadence could not persist a runtime-run checkpoint whose run id does not match the parent run.",
            ));
        }

        if checkpoint.sequence == 0 {
            return Err(CommandError::system_fault(
                "runtime_run_checkpoint_invalid",
                "Cadence requires runtime-run checkpoint sequences to start at 1.",
            ));
        }

        validate_non_empty_text(
            &checkpoint.summary,
            "summary",
            "runtime_run_checkpoint_invalid",
        )?;
        validate_non_empty_text(
            &checkpoint.created_at,
            "created_at",
            "runtime_run_checkpoint_invalid",
        )?;

        if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&checkpoint.summary)
        {
            return Err(CommandError::user_fixable(
                "runtime_run_checkpoint_invalid",
                format!(
                    "Runtime-run checkpoint summaries must not include {secret_hint}. Remove secret-bearing content before retrying."
                ),
            ));
        }
    }

    Ok(())
}

fn resolve_runtime_run_provider_identity(
    provider_id: &str,
    runtime_kind: &str,
) -> Result<crate::runtime::ResolvedRuntimeProvider, crate::auth::AuthDiagnostic> {
    if runtime_kind == crate::runtime::OWNED_AGENT_RUNTIME_KIND {
        return crate::runtime::resolve_runtime_provider_identity(Some(provider_id), None);
    }

    crate::runtime::resolve_runtime_provider_identity(Some(provider_id), Some(runtime_kind))
}

fn validate_runtime_run_control_state(
    control_state: &RuntimeRunControlStateRecord,
) -> Result<(), CommandError> {
    validate_runtime_run_active_control_snapshot(&control_state.active)?;
    if let Some(pending) = control_state.pending.as_ref() {
        validate_runtime_run_pending_control_snapshot(pending, control_state.active.revision)?;
    }
    Ok(())
}

fn validate_runtime_run_active_control_snapshot(
    active: &RuntimeRunActiveControlSnapshotRecord,
) -> Result<(), CommandError> {
    if let Some(provider_profile_id) = active.provider_profile_id.as_ref() {
        validate_non_empty_text(
            provider_profile_id,
            "control_state.active.provider_profile_id",
            "runtime_run_request_invalid",
        )?;
    }
    validate_non_empty_text(
        &active.model_id,
        "control_state.active.model_id",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &active.applied_at,
        "control_state.active.applied_at",
        "runtime_run_request_invalid",
    )?;
    validate_runtime_run_control_timestamp(&active.applied_at, "control_state.active.applied_at")?;
    if active.revision == 0 {
        return Err(CommandError::system_fault(
            "runtime_run_request_invalid",
            "Cadence requires runtime-run active control revisions to start at 1.",
        ));
    }
    Ok(())
}

fn validate_runtime_run_pending_control_snapshot(
    pending: &RuntimeRunPendingControlSnapshotRecord,
    active_revision: u32,
) -> Result<(), CommandError> {
    if let Some(provider_profile_id) = pending.provider_profile_id.as_ref() {
        validate_non_empty_text(
            provider_profile_id,
            "control_state.pending.provider_profile_id",
            "runtime_run_request_invalid",
        )?;
    }
    validate_non_empty_text(
        &pending.model_id,
        "control_state.pending.model_id",
        "runtime_run_request_invalid",
    )?;
    validate_non_empty_text(
        &pending.queued_at,
        "control_state.pending.queued_at",
        "runtime_run_request_invalid",
    )?;
    validate_runtime_run_control_timestamp(&pending.queued_at, "control_state.pending.queued_at")?;
    if pending.revision <= active_revision {
        return Err(CommandError::system_fault(
            "runtime_run_request_invalid",
            "Cadence requires pending runtime-run control revisions to advance beyond the active revision.",
        ));
    }

    match (&pending.queued_prompt, &pending.queued_prompt_at) {
        (None, None) => {}
        (Some(prompt), Some(queued_prompt_at)) => {
            if prompt.trim().is_empty() {
                return Err(CommandError::invalid_request("initialPrompt"));
            }
            validate_non_empty_text(
                queued_prompt_at,
                "control_state.pending.queued_prompt_at",
                "runtime_run_request_invalid",
            )?;
            validate_runtime_run_control_timestamp(
                queued_prompt_at,
                "control_state.pending.queued_prompt_at",
            )?;
            if let Some(secret_hint) = find_prohibited_runtime_control_prompt_content(prompt) {
                return Err(CommandError::user_fixable(
                    "runtime_run_request_invalid",
                    format!(
                        "Runtime-run queued prompts must not include {secret_hint}. Remove secret-bearing content before retrying."
                    ),
                ));
            }
        }
        _ => {
            return Err(CommandError::system_fault(
                "runtime_run_request_invalid",
                "Cadence requires queuedPrompt and queuedPromptAt to be populated together.",
            ))
        }
    }

    Ok(())
}

fn validate_runtime_run_control_timestamp(value: &str, field: &str) -> Result<(), CommandError> {
    OffsetDateTime::parse(value, &Rfc3339).map_err(|error| {
        CommandError::system_fault(
            "runtime_run_request_invalid",
            format!("Cadence requires {field} to be valid RFC3339 text: {error}"),
        )
    })?;
    Ok(())
}

pub fn build_runtime_run_control_state(
    model_id: &str,
    thinking_effort: Option<ProviderModelThinkingEffortDto>,
    approval_mode: RuntimeRunApprovalModeDto,
    timestamp: &str,
    initial_prompt: Option<&str>,
) -> Result<RuntimeRunControlStateRecord, CommandError> {
    build_runtime_run_control_state_with_plan_mode(
        model_id,
        thinking_effort,
        approval_mode,
        false,
        timestamp,
        initial_prompt,
    )
}

pub fn build_runtime_run_control_state_with_plan_mode(
    model_id: &str,
    thinking_effort: Option<ProviderModelThinkingEffortDto>,
    approval_mode: RuntimeRunApprovalModeDto,
    plan_mode_required: bool,
    timestamp: &str,
    initial_prompt: Option<&str>,
) -> Result<RuntimeRunControlStateRecord, CommandError> {
    build_runtime_run_control_state_with_profile(
        None,
        model_id,
        thinking_effort,
        approval_mode,
        plan_mode_required,
        timestamp,
        initial_prompt,
    )
}

pub fn build_runtime_run_control_state_with_profile(
    provider_profile_id: Option<&str>,
    model_id: &str,
    thinking_effort: Option<ProviderModelThinkingEffortDto>,
    approval_mode: RuntimeRunApprovalModeDto,
    plan_mode_required: bool,
    timestamp: &str,
    initial_prompt: Option<&str>,
) -> Result<RuntimeRunControlStateRecord, CommandError> {
    let provider_profile_id = provider_profile_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let model_id = model_id.trim();
    if model_id.is_empty() {
        return Err(CommandError::invalid_request("initialControls.modelId"));
    }

    let active = RuntimeRunActiveControlSnapshotRecord {
        provider_profile_id: provider_profile_id.clone(),
        model_id: model_id.to_owned(),
        thinking_effort,
        approval_mode: approval_mode.clone(),
        plan_mode_required,
        revision: 1,
        applied_at: timestamp.to_owned(),
    };
    let pending = match initial_prompt {
        Some(prompt) if !prompt.trim().is_empty() => Some(RuntimeRunPendingControlSnapshotRecord {
            provider_profile_id,
            model_id: active.model_id.clone(),
            thinking_effort: active.thinking_effort.clone(),
            approval_mode,
            plan_mode_required: active.plan_mode_required,
            revision: active.revision.saturating_add(1),
            queued_at: timestamp.to_owned(),
            queued_prompt: Some(prompt.to_owned()),
            queued_prompt_at: Some(timestamp.to_owned()),
        }),
        Some(_) => return Err(CommandError::invalid_request("initialPrompt")),
        None => None,
    };

    let control_state = RuntimeRunControlStateRecord { active, pending };
    validate_runtime_run_control_state(&control_state)?;
    Ok(control_state)
}

fn serialize_runtime_run_control_state(
    control_state: &RuntimeRunControlStateRecord,
) -> Result<String, CommandError> {
    serde_json::to_string(control_state).map_err(|error| {
        CommandError::system_fault(
            "runtime_run_control_state_serialize_failed",
            format!(
                "Cadence could not serialize the durable runtime-run control snapshot: {error}"
            ),
        )
    })
}

fn decode_runtime_run_control_state(
    value: Option<String>,
    database_path: &Path,
) -> Result<RuntimeRunControlStateRecord, CommandError> {
    let Some(value) = value else {
        return Err(map_runtime_run_decode_error(
            database_path,
            "Field `control_state_json` must be populated with a durable control snapshot.".into(),
        ));
    };
    if value.trim().is_empty() {
        return Err(map_runtime_run_decode_error(
            database_path,
            "Field `control_state_json` must be non-empty JSON text.".into(),
        ));
    }

    let control_state =
        serde_json::from_str::<RuntimeRunControlStateRecord>(&value).map_err(|error| {
            map_runtime_run_decode_error(
                database_path,
                format!(
                    "Field `control_state_json` is not valid runtime-run control JSON: {error}"
                ),
            )
        })?;

    validate_runtime_run_control_state(&control_state)
        .map_err(|error| map_runtime_run_decode_error(database_path, error.message))?;
    Ok(control_state)
}

pub(crate) fn find_prohibited_runtime_control_prompt_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();
    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("bearer ")
        || normalized.contains("sk-")
        || normalized.contains("authorization_url")
        || normalized.contains("redirect_uri")
        || normalized.contains("localhost:")
        || normalized.contains("127.0.0.1:")
    {
        return Some("OAuth or API credential material");
    }
    None
}

pub(crate) fn normalize_runtime_checkpoint_summary(summary: &str) -> String {
    let trimmed = summary.trim();
    let normalized = if trimmed.chars().count() > MAX_RUNTIME_RUN_CHECKPOINT_SUMMARY_CHARS {
        let truncated: String = trimmed
            .chars()
            .take(MAX_RUNTIME_RUN_CHECKPOINT_SUMMARY_CHARS.saturating_sub(1))
            .collect();
        format!("{truncated}…")
    } else {
        trimmed.to_string()
    };

    normalized
}

pub(crate) fn find_prohibited_runtime_persistence_content(value: &str) -> Option<&'static str> {
    if let Some(secret_hint) = find_prohibited_transition_diagnostic_content(value) {
        return Some(secret_hint);
    }

    let normalized = value.to_ascii_lowercase();
    if normalized.contains("redirect_uri")
        || normalized.contains("authorization_url")
        || normalized.contains("/auth/callback")
        || normalized.contains("127.0.0.1:")
        || normalized.contains("localhost:")
    {
        return Some("OAuth redirect URL data");
    }

    if normalized.contains("chatgpt_account_id")
        || normalized.contains("session_id") && normalized.contains("provider_id")
    {
        return Some("auth-store contents");
    }

    if value.contains('\u{1b}')
        || value.contains('\0')
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Some("raw PTY byte sequences");
    }

    None
}

pub(crate) fn find_prohibited_transition_diagnostic_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();

    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("bearer ")
        || normalized.contains("oauth")
        || normalized.contains("sk-")
    {
        return Some("OAuth or API token material");
    }

    if normalized.contains("transcript") {
        return Some("runtime transcript text");
    }

    if normalized.contains("tool_payload")
        || normalized.contains("tool payload")
        || normalized.contains("raw payload")
    {
        return Some("tool raw payload data");
    }

    None
}

fn parse_runtime_run_status(value: &str) -> Result<RuntimeRunStatus, String> {
    match value {
        "starting" => Ok(RuntimeRunStatus::Starting),
        "running" => Ok(RuntimeRunStatus::Running),
        "stale" => Ok(RuntimeRunStatus::Stale),
        "stopped" => Ok(RuntimeRunStatus::Stopped),
        "failed" => Ok(RuntimeRunStatus::Failed),
        other => Err(format!(
            "must be a known runtime-run status, found `{other}`."
        )),
    }
}

fn runtime_run_status_sql_value(value: &RuntimeRunStatus) -> &'static str {
    match value {
        RuntimeRunStatus::Starting => "starting",
        RuntimeRunStatus::Running => "running",
        RuntimeRunStatus::Stale => "stale",
        RuntimeRunStatus::Stopped => "stopped",
        RuntimeRunStatus::Failed => "failed",
    }
}

fn parse_runtime_run_transport_liveness(
    value: &str,
) -> Result<RuntimeRunTransportLiveness, String> {
    match value {
        "unknown" => Ok(RuntimeRunTransportLiveness::Unknown),
        "reachable" => Ok(RuntimeRunTransportLiveness::Reachable),
        "unreachable" => Ok(RuntimeRunTransportLiveness::Unreachable),
        other => Err(format!(
            "must be a known transport liveness value, found `{other}`."
        )),
    }
}

fn runtime_run_transport_liveness_sql_value(value: &RuntimeRunTransportLiveness) -> &'static str {
    match value {
        RuntimeRunTransportLiveness::Unknown => "unknown",
        RuntimeRunTransportLiveness::Reachable => "reachable",
        RuntimeRunTransportLiveness::Unreachable => "unreachable",
    }
}

fn parse_runtime_run_checkpoint_kind(value: &str) -> Result<RuntimeRunCheckpointKind, String> {
    match value {
        "bootstrap" => Ok(RuntimeRunCheckpointKind::Bootstrap),
        "state" => Ok(RuntimeRunCheckpointKind::State),
        "tool" => Ok(RuntimeRunCheckpointKind::Tool),
        "action_required" => Ok(RuntimeRunCheckpointKind::ActionRequired),
        "diagnostic" => Ok(RuntimeRunCheckpointKind::Diagnostic),
        other => Err(format!(
            "Field `kind` must be a known runtime-run checkpoint kind, found `{other}`."
        )),
    }
}

pub(crate) fn runtime_run_checkpoint_kind_sql_value(
    value: &RuntimeRunCheckpointKind,
) -> &'static str {
    match value {
        RuntimeRunCheckpointKind::Bootstrap => "bootstrap",
        RuntimeRunCheckpointKind::State => "state",
        RuntimeRunCheckpointKind::Tool => "tool",
        RuntimeRunCheckpointKind::ActionRequired => "action_required",
        RuntimeRunCheckpointKind::Diagnostic => "diagnostic",
    }
}

fn derive_runtime_run_status(
    status: RuntimeRunStatus,
    last_heartbeat_at: Option<&str>,
    updated_at: &str,
    database_path: &Path,
) -> Result<RuntimeRunStatus, CommandError> {
    if !matches!(
        status,
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running
    ) {
        return Ok(status);
    }

    let reference_timestamp = last_heartbeat_at.unwrap_or(updated_at);
    let reference_time = OffsetDateTime::parse(reference_timestamp, &Rfc3339).map_err(|error| {
        map_runtime_run_decode_error(
            database_path,
            format!(
                "Runtime run timestamp `{reference_timestamp}` is not valid RFC3339 text: {error}"
            ),
        )
    })?;

    let stale_cutoff =
        OffsetDateTime::now_utc() - time::Duration::seconds(RUNTIME_RUN_STALE_AFTER_SECONDS);
    if reference_time <= stale_cutoff {
        Ok(RuntimeRunStatus::Stale)
    } else {
        Ok(status)
    }
}

fn parse_runtime_auth_phase(value: &str) -> Result<RuntimeAuthPhase, String> {
    match value {
        "idle" => Ok(RuntimeAuthPhase::Idle),
        "starting" => Ok(RuntimeAuthPhase::Starting),
        "awaiting_browser_callback" => Ok(RuntimeAuthPhase::AwaitingBrowserCallback),
        "awaiting_manual_input" => Ok(RuntimeAuthPhase::AwaitingManualInput),
        "exchanging_code" => Ok(RuntimeAuthPhase::ExchangingCode),
        "authenticated" => Ok(RuntimeAuthPhase::Authenticated),
        "refreshing" => Ok(RuntimeAuthPhase::Refreshing),
        "cancelled" => Ok(RuntimeAuthPhase::Cancelled),
        "failed" => Ok(RuntimeAuthPhase::Failed),
        other => Err(format!(
            "must be a known runtime auth phase, found `{other}`."
        )),
    }
}

fn runtime_auth_phase_sql_value(value: &RuntimeAuthPhase) -> &'static str {
    match value {
        RuntimeAuthPhase::Idle => "idle",
        RuntimeAuthPhase::Starting => "starting",
        RuntimeAuthPhase::AwaitingBrowserCallback => "awaiting_browser_callback",
        RuntimeAuthPhase::AwaitingManualInput => "awaiting_manual_input",
        RuntimeAuthPhase::ExchangingCode => "exchanging_code",
        RuntimeAuthPhase::Authenticated => "authenticated",
        RuntimeAuthPhase::Refreshing => "refreshing",
        RuntimeAuthPhase::Cancelled => "cancelled",
        RuntimeAuthPhase::Failed => "failed",
    }
}

pub(crate) fn map_runtime_run_transaction_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if super::is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", super::sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!(
                "{message} {}: {error}",
                super::sqlite_path_suffix(database_path)
            ),
        )
    }
}

pub(crate) fn map_runtime_run_write_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if super::is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", super::sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!(
                "{message} {}: {error}",
                super::sqlite_path_suffix(database_path)
            ),
        )
    }
}

pub(crate) fn map_runtime_run_commit_error(
    code: &str,
    database_path: &Path,
    error: SqlError,
    message: &str,
) -> CommandError {
    if super::is_retryable_sql_error(&error) {
        CommandError::retryable(
            code,
            format!("{message} {}", super::sqlite_path_suffix(database_path)),
        )
    } else {
        CommandError::system_fault(
            code,
            format!(
                "{message} {}: {error}",
                super::sqlite_path_suffix(database_path)
            ),
        )
    }
}

pub(crate) fn decode_runtime_run_checkpoint_sequence(
    value: i64,
    field: &str,
    database_path: &Path,
) -> Result<u32, CommandError> {
    u32::try_from(value).map_err(|_| {
        map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be a non-negative 32-bit integer, found {value}."),
        )
    })
}

pub(crate) fn require_runtime_run_non_empty_owned(
    value: String,
    field: &str,
    database_path: &Path,
) -> Result<String, CommandError> {
    if value.trim().is_empty() {
        Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be a non-empty string."),
        ))
    } else {
        Ok(value)
    }
}

pub(crate) fn decode_runtime_run_optional_non_empty_text(
    value: Option<String>,
    field: &str,
    database_path: &Path,
) -> Result<Option<String>, CommandError> {
    match value {
        Some(value) if value.trim().is_empty() => Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be null or a non-empty string."),
        )),
        other => Ok(other),
    }
}

pub(crate) fn decode_runtime_run_bool(
    value: i64,
    field: &str,
    database_path: &Path,
) -> Result<bool, CommandError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        other => Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must be 0 or 1, found {other}."),
        )),
    }
}

pub(crate) fn decode_runtime_run_reason(
    code: Option<String>,
    message: Option<String>,
    field: &str,
    database_path: &Path,
) -> Result<Option<RuntimeRunDiagnosticRecord>, CommandError> {
    match (code, message) {
        (None, None) => Ok(None),
        (Some(code), Some(message)) => Ok(Some(RuntimeRunDiagnosticRecord {
            code: require_runtime_run_non_empty_owned(
                code,
                &format!("{field}_code"),
                database_path,
            )?,
            message: require_runtime_run_non_empty_owned(
                message,
                &format!("{field}_message"),
                database_path,
            )?,
        })),
        _ => Err(map_runtime_run_decode_error(
            database_path,
            format!("Field `{field}` must have both code and message populated together."),
        )),
    }
}

pub(crate) fn require_runtime_run_checkpoint_non_empty_owned(
    value: String,
    field: &str,
    database_path: &Path,
) -> Result<String, CommandError> {
    if value.trim().is_empty() {
        Err(map_runtime_run_checkpoint_decode_error(
            database_path,
            format!("Field `{field}` must be a non-empty string."),
        ))
    } else {
        Ok(value)
    }
}

pub(crate) fn map_runtime_decode_error(database_path: &Path, details: String) -> CommandError {
    CommandError::system_fault(
        "runtime_session_decode_failed",
        format!(
            "Cadence could not decode runtime-session metadata from {}: {details}",
            database_path.display()
        ),
    )
}

pub(crate) fn map_runtime_run_decode_error(database_path: &Path, details: String) -> CommandError {
    CommandError::system_fault(
        "runtime_run_decode_failed",
        format!(
            "Cadence could not decode durable runtime-run metadata from {}: {details}",
            database_path.display()
        ),
    )
}

pub(crate) fn map_runtime_run_checkpoint_decode_error(
    database_path: &Path,
    details: String,
) -> CommandError {
    CommandError::system_fault(
        "runtime_run_checkpoint_decode_failed",
        format!(
            "Cadence could not decode durable runtime-run checkpoints from {}: {details}",
            database_path.display()
        ),
    )
}
