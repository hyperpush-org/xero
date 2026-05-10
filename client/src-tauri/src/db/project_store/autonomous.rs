use std::path::Path;

use rusqlite::{params, Connection, Error as SqlError};

use crate::{commands::CommandError, db::database_path_for_repo};

use super::runtime::{
    decode_runtime_run_bool, decode_runtime_run_optional_non_empty_text, decode_runtime_run_reason,
    find_prohibited_runtime_persistence_content, map_runtime_run_commit_error,
    map_runtime_run_decode_error, map_runtime_run_transaction_error, map_runtime_run_write_error,
    read_runtime_run_row, require_runtime_run_non_empty_owned, RuntimeRunDiagnosticRecord,
};
use super::{open_runtime_database, read_project_row, validate_non_empty_text};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutonomousRunStatus {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousSkillLifecycleStageRecord {
    Discovery,
    Install,
    Invoke,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousSkillLifecycleResultRecord {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousRunRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub supervisor_kind: String,
    pub status: AutonomousRunStatus,
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
    pub pause_reason: Option<RuntimeRunDiagnosticRecord>,
    pub cancel_reason: Option<RuntimeRunDiagnosticRecord>,
    pub crash_reason: Option<RuntimeRunDiagnosticRecord>,
    pub last_error: Option<RuntimeRunDiagnosticRecord>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousRunUpsertRecord {
    pub run: AutonomousRunRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousRunSnapshotRecord {
    pub run: AutonomousRunRecord,
}

#[derive(Debug)]
struct RawAutonomousRunRow {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    runtime_kind: String,
    provider_id: String,
    supervisor_kind: String,
    status: String,
    duplicate_start_detected: i64,
    duplicate_start_run_id: Option<String>,
    duplicate_start_reason: Option<String>,
    started_at: String,
    last_heartbeat_at: Option<String>,
    last_checkpoint_at: Option<String>,
    paused_at: Option<String>,
    cancelled_at: Option<String>,
    completed_at: Option<String>,
    crashed_at: Option<String>,
    stopped_at: Option<String>,
    pause_reason_code: Option<String>,
    pause_reason_message: Option<String>,
    cancel_reason_code: Option<String>,
    cancel_reason_message: Option<String>,
    crash_reason_code: Option<String>,
    crash_reason_message: Option<String>,
    last_error_code: Option<String>,
    last_error_message: Option<String>,
    updated_at: String,
}

pub fn load_autonomous_run(
    repo_root: &Path,
    expected_project_id: &str,
    expected_agent_session_id: &str,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, expected_project_id)?;

    read_autonomous_run_snapshot(
        &connection,
        &database_path,
        expected_project_id,
        expected_agent_session_id,
    )
}

pub fn upsert_autonomous_run(
    repo_root: &Path,
    payload: &AutonomousRunUpsertRecord,
) -> Result<AutonomousRunSnapshotRecord, CommandError> {
    validate_autonomous_run_payload(&payload.run)?;

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
            "autonomous_run_transaction_failed",
            &database_path,
            error,
            "Xero could not start the durable autonomous-run transaction.",
        )
    })?;

    let runtime_row = read_runtime_run_row(
        &transaction,
        &database_path,
        &payload.run.project_id,
        &payload.run.agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::retryable(
            "autonomous_run_missing_runtime_row",
            format!(
                "Xero could not persist autonomous-run metadata in {} because the selected project has no durable runtime-run row.",
                database_path.display()
            ),
        )
    })?;

    if runtime_row.run_id != payload.run.run_id {
        return Err(CommandError::retryable(
            "autonomous_run_mismatch",
            format!(
                "Xero refused to persist autonomous-run metadata for run `{}` because the durable runtime-run row currently points at `{}`.",
                payload.run.run_id, runtime_row.run_id
            ),
        ));
    }

    if !runtime_run_identity_matches_autonomous_projection(
        &runtime_row.provider_id,
        &runtime_row.runtime_kind,
        &payload.run.provider_id,
        &payload.run.runtime_kind,
    ) {
        return Err(CommandError::retryable(
            "autonomous_run_mismatch",
            format!(
                "Xero refused to persist autonomous-run metadata for run `{}` because the durable runtime-run identity is `{}`/`{}` instead of `{}`/`{}`.",
                payload.run.run_id,
                runtime_row.provider_id,
                runtime_row.runtime_kind,
                payload.run.provider_id,
                payload.run.runtime_kind
            ),
        ));
    }

    let duplicate_start_detected = i64::from(payload.run.duplicate_start_detected);
    let pause_reason_code = payload
        .run
        .pause_reason
        .as_ref()
        .map(|reason| reason.code.as_str());
    let pause_reason_message = payload
        .run
        .pause_reason
        .as_ref()
        .map(|reason| reason.message.as_str());
    let cancel_reason_code = payload
        .run
        .cancel_reason
        .as_ref()
        .map(|reason| reason.code.as_str());
    let cancel_reason_message = payload
        .run
        .cancel_reason
        .as_ref()
        .map(|reason| reason.message.as_str());
    let crash_reason_code = payload
        .run
        .crash_reason
        .as_ref()
        .map(|reason| reason.code.as_str());
    let crash_reason_message = payload
        .run
        .crash_reason
        .as_ref()
        .map(|reason| reason.message.as_str());
    let last_error_code = payload
        .run
        .last_error
        .as_ref()
        .map(|reason| reason.code.as_str());
    let last_error_message = payload
        .run
        .last_error
        .as_ref()
        .map(|reason| reason.message.as_str());

    transaction
        .execute(
            r#"
            INSERT INTO autonomous_runs (
                project_id,
                agent_session_id,
                run_id,
                runtime_kind,
                provider_id,
                supervisor_kind,
                status,
                active_unit_sequence,
                duplicate_start_detected,
                duplicate_start_run_id,
                duplicate_start_reason,
                started_at,
                last_heartbeat_at,
                last_checkpoint_at,
                paused_at,
                cancelled_at,
                completed_at,
                crashed_at,
                stopped_at,
                pause_reason_code,
                pause_reason_message,
                cancel_reason_code,
                cancel_reason_message,
                crash_reason_code,
                crash_reason_message,
                last_error_code,
                last_error_message,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, NULL, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27)
            ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
                run_id = excluded.run_id,
                runtime_kind = excluded.runtime_kind,
                provider_id = excluded.provider_id,
                supervisor_kind = excluded.supervisor_kind,
                status = excluded.status,
                active_unit_sequence = NULL,
                duplicate_start_detected = excluded.duplicate_start_detected,
                duplicate_start_run_id = excluded.duplicate_start_run_id,
                duplicate_start_reason = excluded.duplicate_start_reason,
                started_at = excluded.started_at,
                last_heartbeat_at = excluded.last_heartbeat_at,
                last_checkpoint_at = excluded.last_checkpoint_at,
                paused_at = excluded.paused_at,
                cancelled_at = excluded.cancelled_at,
                completed_at = excluded.completed_at,
                crashed_at = excluded.crashed_at,
                stopped_at = excluded.stopped_at,
                pause_reason_code = excluded.pause_reason_code,
                pause_reason_message = excluded.pause_reason_message,
                cancel_reason_code = excluded.cancel_reason_code,
                cancel_reason_message = excluded.cancel_reason_message,
                crash_reason_code = excluded.crash_reason_code,
                crash_reason_message = excluded.crash_reason_message,
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
                autonomous_run_status_sql_value(&payload.run.status),
                duplicate_start_detected,
                payload.run.duplicate_start_run_id.as_deref(),
                payload.run.duplicate_start_reason.as_deref(),
                payload.run.started_at.as_str(),
                payload.run.last_heartbeat_at.as_deref(),
                payload.run.last_checkpoint_at.as_deref(),
                payload.run.paused_at.as_deref(),
                payload.run.cancelled_at.as_deref(),
                payload.run.completed_at.as_deref(),
                payload.run.crashed_at.as_deref(),
                payload.run.stopped_at.as_deref(),
                pause_reason_code,
                pause_reason_message,
                cancel_reason_code,
                cancel_reason_message,
                crash_reason_code,
                crash_reason_message,
                last_error_code,
                last_error_message,
                payload.run.updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            map_runtime_run_write_error(
                "autonomous_run_persist_failed",
                &database_path,
                error,
                "Xero could not persist durable autonomous-run metadata.",
            )
        })?;

    transaction.commit().map_err(|error| {
        map_runtime_run_commit_error(
            "autonomous_run_commit_failed",
            &database_path,
            error,
            "Xero could not commit the durable autonomous-run transaction.",
        )
    })?;

    read_autonomous_run_snapshot(
        &connection,
        &database_path,
        &payload.run.project_id,
        &payload.run.agent_session_id,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "autonomous_run_missing_after_persist",
            format!(
                "Xero persisted durable autonomous-run metadata in {} but could not read it back.",
                database_path.display()
            ),
        )
    })
}

fn read_autonomous_run_snapshot(
    connection: &Connection,
    database_path: &Path,
    expected_project_id: &str,
    expected_agent_session_id: &str,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
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
                duplicate_start_detected,
                duplicate_start_run_id,
                duplicate_start_reason,
                started_at,
                last_heartbeat_at,
                last_checkpoint_at,
                paused_at,
                cancelled_at,
                completed_at,
                crashed_at,
                stopped_at,
                pause_reason_code,
                pause_reason_message,
                cancel_reason_code,
                cancel_reason_message,
                crash_reason_code,
                crash_reason_message,
                last_error_code,
                last_error_message,
                updated_at
            FROM autonomous_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
            "#,
        params![expected_project_id, expected_agent_session_id],
        |row| {
            Ok(RawAutonomousRunRow {
                project_id: row.get(0)?,
                agent_session_id: row.get(1)?,
                run_id: row.get(2)?,
                runtime_kind: row.get(3)?,
                provider_id: row.get(4)?,
                supervisor_kind: row.get(5)?,
                status: row.get(6)?,
                duplicate_start_detected: row.get(7)?,
                duplicate_start_run_id: row.get(8)?,
                duplicate_start_reason: row.get(9)?,
                started_at: row.get(10)?,
                last_heartbeat_at: row.get(11)?,
                last_checkpoint_at: row.get(12)?,
                paused_at: row.get(13)?,
                cancelled_at: row.get(14)?,
                completed_at: row.get(15)?,
                crashed_at: row.get(16)?,
                stopped_at: row.get(17)?,
                pause_reason_code: row.get(18)?,
                pause_reason_message: row.get(19)?,
                cancel_reason_code: row.get(20)?,
                cancel_reason_message: row.get(21)?,
                crash_reason_code: row.get(22)?,
                crash_reason_message: row.get(23)?,
                last_error_code: row.get(24)?,
                last_error_message: row.get(25)?,
                updated_at: row.get(26)?,
            })
        },
    );

    let raw_row = match row {
        Ok(row) => row,
        Err(SqlError::QueryReturnedNoRows) => return Ok(None),
        Err(other) => {
            return Err(CommandError::system_fault(
                "autonomous_run_query_failed",
                format!(
                    "Xero could not read durable autonomous-run metadata from {}: {other}",
                    database_path.display()
                ),
            ));
        }
    };

    decode_autonomous_run_row(raw_row, database_path)
        .map(|run| Some(AutonomousRunSnapshotRecord { run }))
}

fn runtime_run_identity_matches_autonomous_projection(
    runtime_provider_id: &str,
    runtime_kind: &str,
    autonomous_provider_id: &str,
    autonomous_runtime_kind: &str,
) -> bool {
    runtime_provider_id == autonomous_provider_id
        && autonomous_projection_runtime_kind(runtime_provider_id, runtime_kind)
            == autonomous_runtime_kind
}

fn autonomous_projection_runtime_kind(provider_id: &str, runtime_kind: &str) -> String {
    if runtime_kind == crate::runtime::OWNED_AGENT_RUNTIME_KIND {
        return crate::runtime::resolve_runtime_provider_identity(Some(provider_id), None)
            .map(|provider| provider.runtime_kind.to_string())
            .unwrap_or_else(|_| runtime_kind.to_string());
    }

    runtime_kind.to_string()
}

fn validate_autonomous_run_payload(payload: &AutonomousRunRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &payload.project_id,
        "project_id",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.agent_session_id,
        "agent_session_id",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(&payload.run_id, "run_id", "autonomous_run_request_invalid")?;
    validate_non_empty_text(
        &payload.runtime_kind,
        "runtime_kind",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.provider_id,
        "provider_id",
        "autonomous_run_request_invalid",
    )?;
    crate::runtime::resolve_runtime_provider_identity(
        Some(payload.provider_id.as_str()),
        Some(payload.runtime_kind.as_str()),
    )
    .map_err(|diagnostic| {
        CommandError::user_fixable(
            "autonomous_run_request_invalid",
            format!(
                "Xero rejected the durable autonomous-run identity because {}",
                diagnostic.message
            ),
        )
    })?;
    validate_non_empty_text(
        &payload.supervisor_kind,
        "supervisor_kind",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.started_at,
        "started_at",
        "autonomous_run_request_invalid",
    )?;
    validate_non_empty_text(
        &payload.updated_at,
        "updated_at",
        "autonomous_run_request_invalid",
    )?;

    for (value, field) in [
        (payload.last_heartbeat_at.as_deref(), "last_heartbeat_at"),
        (payload.last_checkpoint_at.as_deref(), "last_checkpoint_at"),
        (payload.paused_at.as_deref(), "paused_at"),
        (payload.cancelled_at.as_deref(), "cancelled_at"),
        (payload.completed_at.as_deref(), "completed_at"),
        (payload.crashed_at.as_deref(), "crashed_at"),
        (payload.stopped_at.as_deref(), "stopped_at"),
        (
            payload.duplicate_start_run_id.as_deref(),
            "duplicate_start_run_id",
        ),
        (
            payload.duplicate_start_reason.as_deref(),
            "duplicate_start_reason",
        ),
    ] {
        if let Some(value) = value {
            validate_non_empty_text(value, field, "autonomous_run_request_invalid")?;
        }
    }

    for (reason, label) in [
        (payload.pause_reason.as_ref(), "pause_reason"),
        (payload.cancel_reason.as_ref(), "cancel_reason"),
        (payload.crash_reason.as_ref(), "crash_reason"),
        (payload.last_error.as_ref(), "last_error"),
    ] {
        if let Some(reason) = reason {
            validate_non_empty_text(
                &reason.code,
                &format!("{label}_code"),
                "autonomous_run_request_invalid",
            )?;
            validate_non_empty_text(
                &reason.message,
                &format!("{label}_message"),
                "autonomous_run_request_invalid",
            )?;
            if let Some(secret_hint) = find_prohibited_runtime_persistence_content(&reason.message)
            {
                return Err(CommandError::user_fixable(
                    "autonomous_run_request_invalid",
                    format!(
                        "Autonomous run {label} must not include {secret_hint}. Remove secret-bearing content before retrying."
                    ),
                ));
            }
        }
    }

    Ok(())
}

fn decode_autonomous_run_row(
    raw_row: RawAutonomousRunRow,
    database_path: &Path,
) -> Result<AutonomousRunRecord, CommandError> {
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
    crate::runtime::resolve_runtime_provider_identity(
        Some(provider_id.as_str()),
        Some(runtime_kind.as_str()),
    )
    .map_err(|diagnostic| {
        map_runtime_run_decode_error(
            database_path,
            format!(
                "Autonomous run provider identity is invalid because {}",
                diagnostic.message
            ),
        )
    })?;
    let supervisor_kind = require_runtime_run_non_empty_owned(
        raw_row.supervisor_kind,
        "supervisor_kind",
        database_path,
    )?;
    let status = parse_autonomous_run_status(&raw_row.status).map_err(|details| {
        map_runtime_run_decode_error(database_path, format!("Field `status` {details}"))
    })?;
    let duplicate_start_detected = decode_runtime_run_bool(
        raw_row.duplicate_start_detected,
        "duplicate_start_detected",
        database_path,
    )?;
    let duplicate_start_run_id = decode_runtime_run_optional_non_empty_text(
        raw_row.duplicate_start_run_id,
        "duplicate_start_run_id",
        database_path,
    )?;
    let duplicate_start_reason = decode_runtime_run_optional_non_empty_text(
        raw_row.duplicate_start_reason,
        "duplicate_start_reason",
        database_path,
    )?;
    let started_at =
        require_runtime_run_non_empty_owned(raw_row.started_at, "started_at", database_path)?;
    let last_heartbeat_at = decode_runtime_run_optional_non_empty_text(
        raw_row.last_heartbeat_at,
        "last_heartbeat_at",
        database_path,
    )?;
    let last_checkpoint_at = decode_runtime_run_optional_non_empty_text(
        raw_row.last_checkpoint_at,
        "last_checkpoint_at",
        database_path,
    )?;
    let paused_at =
        decode_runtime_run_optional_non_empty_text(raw_row.paused_at, "paused_at", database_path)?;
    let cancelled_at = decode_runtime_run_optional_non_empty_text(
        raw_row.cancelled_at,
        "cancelled_at",
        database_path,
    )?;
    let completed_at = decode_runtime_run_optional_non_empty_text(
        raw_row.completed_at,
        "completed_at",
        database_path,
    )?;
    let crashed_at = decode_runtime_run_optional_non_empty_text(
        raw_row.crashed_at,
        "crashed_at",
        database_path,
    )?;
    let stopped_at = decode_runtime_run_optional_non_empty_text(
        raw_row.stopped_at,
        "stopped_at",
        database_path,
    )?;
    let pause_reason = decode_runtime_run_reason(
        raw_row.pause_reason_code,
        raw_row.pause_reason_message,
        "pause_reason",
        database_path,
    )?;
    let cancel_reason = decode_runtime_run_reason(
        raw_row.cancel_reason_code,
        raw_row.cancel_reason_message,
        "cancel_reason",
        database_path,
    )?;
    let crash_reason = decode_runtime_run_reason(
        raw_row.crash_reason_code,
        raw_row.crash_reason_message,
        "crash_reason",
        database_path,
    )?;
    let last_error = decode_runtime_run_reason(
        raw_row.last_error_code,
        raw_row.last_error_message,
        "last_error",
        database_path,
    )?;
    let updated_at =
        require_runtime_run_non_empty_owned(raw_row.updated_at, "updated_at", database_path)?;

    if duplicate_start_detected
        && (duplicate_start_run_id.is_none() || duplicate_start_reason.is_none())
    {
        return Err(map_runtime_run_decode_error(
            database_path,
            "Autonomous run duplicate-start fields must be fully populated when duplicate_start_detected is true.".into(),
        ));
    }

    if !duplicate_start_detected
        && (duplicate_start_run_id.is_some() || duplicate_start_reason.is_some())
    {
        return Err(map_runtime_run_decode_error(
            database_path,
            "Autonomous run duplicate-start fields must be null when duplicate_start_detected is false.".into(),
        ));
    }

    Ok(AutonomousRunRecord {
        project_id,
        agent_session_id,
        run_id,
        runtime_kind,
        provider_id,
        supervisor_kind,
        status,
        duplicate_start_detected,
        duplicate_start_run_id,
        duplicate_start_reason,
        started_at,
        last_heartbeat_at,
        last_checkpoint_at,
        paused_at,
        cancelled_at,
        completed_at,
        crashed_at,
        stopped_at,
        pause_reason,
        cancel_reason,
        crash_reason,
        last_error,
        updated_at,
    })
}

fn parse_autonomous_run_status(value: &str) -> Result<AutonomousRunStatus, String> {
    match value {
        "starting" => Ok(AutonomousRunStatus::Starting),
        "running" => Ok(AutonomousRunStatus::Running),
        "paused" => Ok(AutonomousRunStatus::Paused),
        "cancelling" => Ok(AutonomousRunStatus::Cancelling),
        "cancelled" => Ok(AutonomousRunStatus::Cancelled),
        "stale" => Ok(AutonomousRunStatus::Stale),
        "failed" => Ok(AutonomousRunStatus::Failed),
        "stopped" => Ok(AutonomousRunStatus::Stopped),
        "crashed" => Ok(AutonomousRunStatus::Crashed),
        "completed" => Ok(AutonomousRunStatus::Completed),
        other => Err(format!(
            "must be a known autonomous-run status, found `{other}`."
        )),
    }
}

fn autonomous_run_status_sql_value(value: &AutonomousRunStatus) -> &'static str {
    match value {
        AutonomousRunStatus::Starting => "starting",
        AutonomousRunStatus::Running => "running",
        AutonomousRunStatus::Paused => "paused",
        AutonomousRunStatus::Cancelling => "cancelling",
        AutonomousRunStatus::Cancelled => "cancelled",
        AutonomousRunStatus::Stale => "stale",
        AutonomousRunStatus::Failed => "failed",
        AutonomousRunStatus::Stopped => "stopped",
        AutonomousRunStatus::Crashed => "crashed",
        AutonomousRunStatus::Completed => "completed",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autonomous_identity_match_accepts_owned_agent_runtime_rows() {
        assert!(runtime_run_identity_matches_autonomous_projection(
            crate::runtime::OPENAI_CODEX_PROVIDER_ID,
            crate::runtime::OWNED_AGENT_RUNTIME_KIND,
            crate::runtime::OPENAI_CODEX_PROVIDER_ID,
            crate::runtime::OPENAI_CODEX_PROVIDER_ID,
        ));
    }

    #[test]
    fn autonomous_identity_match_rejects_provider_mismatch() {
        assert!(!runtime_run_identity_matches_autonomous_projection(
            crate::runtime::OPENAI_CODEX_PROVIDER_ID,
            crate::runtime::OWNED_AGENT_RUNTIME_KIND,
            crate::runtime::OPENROUTER_PROVIDER_ID,
            crate::runtime::OPENROUTER_PROVIDER_ID,
        ));
    }
}
