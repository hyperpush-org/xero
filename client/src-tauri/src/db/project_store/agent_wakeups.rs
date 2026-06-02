use std::path::Path;

use rusqlite::{params, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    commands::{validate_non_empty, CommandError},
    db::database_path_for_repo,
};

use super::{open_runtime_database, AgentRunDiagnosticRecord};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunWakeupKind {
    Sleep,
    ProcessExit,
    ProcessReady,
    ProcessOutput,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunWakeupStatus {
    Pending,
    Fired,
    Cancelled,
    Expired,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentRunWakeupRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub wake_id: String,
    pub kind: AgentRunWakeupKind,
    pub due_at: String,
    pub deadline_at: Option<String>,
    pub poll_interval_ms: Option<u64>,
    pub payload_json: String,
    pub status: AgentRunWakeupStatus,
    pub attempt_count: u64,
    pub last_error: Option<AgentRunDiagnosticRecord>,
    pub fired_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl AgentRunWakeupRecord {
    pub fn payload(&self) -> Result<JsonValue, CommandError> {
        serde_json::from_str(&self.payload_json).map_err(|error| {
            CommandError::retryable(
                "agent_run_wakeup_payload_decode_failed",
                format!(
                    "Xero could not decode scheduled wakeup `{}` payload: {error}",
                    self.wake_id
                ),
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentRunWakeupRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub wake_id: String,
    pub kind: AgentRunWakeupKind,
    pub due_at: String,
    pub deadline_at: Option<String>,
    pub poll_interval_ms: Option<u64>,
    pub payload_json: String,
    pub created_at: String,
}

pub fn insert_agent_run_wakeup(
    repo_root: &Path,
    record: &NewAgentRunWakeupRecord,
) -> Result<AgentRunWakeupRecord, CommandError> {
    validate_new_wakeup(record)?;
    let connection = open_wakeup_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_run_wakeups (
                project_id,
                agent_session_id,
                run_id,
                wake_id,
                kind,
                due_at,
                deadline_at,
                poll_interval_ms,
                payload_json,
                status,
                attempt_count,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', 0, ?10, ?10)
            "#,
            params![
                record.project_id,
                record.agent_session_id,
                record.run_id,
                record.wake_id,
                agent_run_wakeup_kind_sql_value(record.kind),
                record.due_at,
                record.deadline_at,
                optional_u64_to_i64(record.poll_interval_ms)?,
                record.payload_json,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_wakeup_store_write_error(repo_root, "agent_run_wakeup_insert_failed", error)
        })?;
    load_agent_run_wakeup(
        repo_root,
        &record.project_id,
        &record.run_id,
        &record.wake_id,
    )
}

pub fn load_agent_run_wakeup(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    wake_id: &str,
) -> Result<AgentRunWakeupRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(wake_id, "wakeId")?;
    let connection = open_wakeup_database(repo_root)?;
    connection
        .query_row(
            agent_run_wakeup_select_sql("WHERE project_id = ?1 AND run_id = ?2 AND wake_id = ?3")
                .as_str(),
            params![project_id, run_id, wake_id],
            read_agent_run_wakeup_row,
        )
        .map_err(|error| {
            map_wakeup_store_query_error(repo_root, "agent_run_wakeup_read_failed", error)
        })
}

pub fn list_pending_agent_run_wakeups(
    repo_root: &Path,
) -> Result<Vec<AgentRunWakeupRecord>, CommandError> {
    let connection = open_wakeup_database(repo_root)?;
    let mut statement = connection
        .prepare(
            agent_run_wakeup_select_sql(
                "WHERE status = 'pending' ORDER BY due_at ASC, created_at ASC",
            )
            .as_str(),
        )
        .map_err(|error| {
            map_wakeup_store_query_error(repo_root, "agent_run_wakeups_prepare_failed", error)
        })?;
    let rows = statement
        .query_map([], read_agent_run_wakeup_row)
        .map_err(|error| {
            map_wakeup_store_query_error(repo_root, "agent_run_wakeups_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_wakeup_store_query_error(repo_root, "agent_run_wakeups_decode_failed", error)
    })
}

pub fn list_pending_agent_run_wakeups_for_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AgentRunWakeupRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    let connection = open_wakeup_database(repo_root)?;
    let mut statement = connection
        .prepare(
            agent_run_wakeup_select_sql(
                "WHERE project_id = ?1 AND run_id = ?2 AND status = 'pending' ORDER BY due_at ASC, created_at ASC",
            )
            .as_str(),
        )
        .map_err(|error| {
            map_wakeup_store_query_error(repo_root, "agent_run_wakeups_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, run_id], read_agent_run_wakeup_row)
        .map_err(|error| {
            map_wakeup_store_query_error(repo_root, "agent_run_wakeups_query_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_wakeup_store_query_error(repo_root, "agent_run_wakeups_decode_failed", error)
    })
}

pub fn maybe_load_pending_agent_run_wakeup(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    wake_id: &str,
) -> Result<Option<AgentRunWakeupRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(wake_id, "wakeId")?;
    let connection = open_wakeup_database(repo_root)?;
    connection
        .query_row(
            agent_run_wakeup_select_sql(
                "WHERE project_id = ?1 AND run_id = ?2 AND wake_id = ?3 AND status = 'pending'",
            )
            .as_str(),
            params![project_id, run_id, wake_id],
            read_agent_run_wakeup_row,
        )
        .optional()
        .map_err(|error| {
            map_wakeup_store_query_error(repo_root, "agent_run_wakeup_read_failed", error)
        })
}

pub fn mark_agent_run_wakeup_fired(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    wake_id: &str,
    fired_at: &str,
) -> Result<bool, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(wake_id, "wakeId")?;
    validate_non_empty_text(fired_at, "firedAt")?;
    let connection = open_wakeup_database(repo_root)?;
    let changed = connection
        .execute(
            r#"
            UPDATE agent_run_wakeups
            SET status = 'fired',
                attempt_count = attempt_count + 1,
                fired_at = ?4,
                updated_at = ?4,
                last_error_code = NULL,
                last_error_message = NULL
            WHERE project_id = ?1
              AND run_id = ?2
              AND wake_id = ?3
              AND status = 'pending'
            "#,
            params![project_id, run_id, wake_id, fired_at],
        )
        .map_err(|error| {
            map_wakeup_store_write_error(repo_root, "agent_run_wakeup_fire_failed", error)
        })?;
    Ok(changed > 0)
}

pub fn reschedule_agent_run_wakeup(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    wake_id: &str,
    due_at: &str,
    payload_json: &str,
    updated_at: &str,
) -> Result<AgentRunWakeupRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(wake_id, "wakeId")?;
    validate_non_empty_text(due_at, "dueAt")?;
    validate_json_payload(payload_json, "payloadJson")?;
    validate_non_empty_text(updated_at, "updatedAt")?;
    let connection = open_wakeup_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_run_wakeups
            SET status = 'pending',
                due_at = ?4,
                payload_json = ?5,
                attempt_count = attempt_count + 1,
                fired_at = NULL,
                last_error_code = NULL,
                last_error_message = NULL,
                updated_at = ?6
            WHERE project_id = ?1
              AND run_id = ?2
              AND wake_id = ?3
              AND status = 'pending'
            "#,
            params![
                project_id,
                run_id,
                wake_id,
                due_at,
                payload_json,
                updated_at
            ],
        )
        .map_err(|error| {
            map_wakeup_store_write_error(repo_root, "agent_run_wakeup_reschedule_failed", error)
        })?;
    load_agent_run_wakeup(repo_root, project_id, run_id, wake_id)
}

pub fn mark_agent_run_wakeup_status(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    wake_id: &str,
    status: AgentRunWakeupStatus,
    diagnostic: Option<AgentRunDiagnosticRecord>,
    updated_at: &str,
) -> Result<AgentRunWakeupRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(run_id, "runId")?;
    validate_non_empty_text(wake_id, "wakeId")?;
    validate_non_empty_text(updated_at, "updatedAt")?;
    let connection = open_wakeup_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_run_wakeups
            SET status = ?4,
                last_error_code = ?5,
                last_error_message = ?6,
                updated_at = ?7
            WHERE project_id = ?1
              AND run_id = ?2
              AND wake_id = ?3
            "#,
            params![
                project_id,
                run_id,
                wake_id,
                agent_run_wakeup_status_sql_value(status),
                diagnostic.as_ref().map(|value| value.code.as_str()),
                diagnostic.as_ref().map(|value| value.message.as_str()),
                updated_at,
            ],
        )
        .map_err(|error| {
            map_wakeup_store_write_error(repo_root, "agent_run_wakeup_status_update_failed", error)
        })?;
    load_agent_run_wakeup(repo_root, project_id, run_id, wake_id)
}

fn validate_new_wakeup(record: &NewAgentRunWakeupRecord) -> Result<(), CommandError> {
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.agent_session_id, "agentSessionId")?;
    validate_non_empty_text(&record.run_id, "runId")?;
    validate_non_empty_text(&record.wake_id, "wakeId")?;
    validate_non_empty_text(&record.due_at, "dueAt")?;
    if let Some(deadline_at) = record.deadline_at.as_deref() {
        validate_non_empty_text(deadline_at, "deadlineAt")?;
    }
    validate_json_payload(&record.payload_json, "payloadJson")?;
    validate_non_empty_text(&record.created_at, "createdAt")?;
    Ok(())
}

fn validate_non_empty_text(value: &str, field: &'static str) -> Result<(), CommandError> {
    validate_non_empty(value, field)
}

fn validate_json_payload(value: &str, field: &'static str) -> Result<(), CommandError> {
    validate_non_empty_text(value, field)?;
    serde_json::from_str::<JsonValue>(value)
        .map(|_| ())
        .map_err(|_| CommandError::invalid_request(field))
}

fn open_wakeup_database(repo_root: &Path) -> Result<rusqlite::Connection, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    open_runtime_database(repo_root, &database_path)
}

fn agent_run_wakeup_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            project_id,
            agent_session_id,
            run_id,
            wake_id,
            kind,
            due_at,
            deadline_at,
            poll_interval_ms,
            payload_json,
            status,
            attempt_count,
            last_error_code,
            last_error_message,
            fired_at,
            created_at,
            updated_at
        FROM agent_run_wakeups
        {where_clause}
        "#
    )
}

fn read_agent_run_wakeup_row(row: &Row<'_>) -> rusqlite::Result<AgentRunWakeupRecord> {
    let poll_interval_ms = optional_i64_to_u64(row.get(7)?, 7)?;
    let attempt_count = i64_to_u64(row.get(10)?, 10)?;
    let last_error_code: Option<String> = row.get(11)?;
    let last_error_message: Option<String> = row.get(12)?;
    Ok(AgentRunWakeupRecord {
        project_id: row.get(0)?,
        agent_session_id: row.get(1)?,
        run_id: row.get(2)?,
        wake_id: row.get(3)?,
        kind: parse_agent_run_wakeup_kind(&row.get::<_, String>(4)?),
        due_at: row.get(5)?,
        deadline_at: row.get(6)?,
        poll_interval_ms,
        payload_json: row.get(8)?,
        status: parse_agent_run_wakeup_status(&row.get::<_, String>(9)?),
        attempt_count,
        last_error: match (last_error_code, last_error_message) {
            (Some(code), Some(message)) => Some(AgentRunDiagnosticRecord { code, message }),
            _ => None,
        },
        fired_at: row.get(13)?,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    })
}

pub fn agent_run_wakeup_kind_sql_value(kind: AgentRunWakeupKind) -> &'static str {
    match kind {
        AgentRunWakeupKind::Sleep => "sleep",
        AgentRunWakeupKind::ProcessExit => "process_exit",
        AgentRunWakeupKind::ProcessReady => "process_ready",
        AgentRunWakeupKind::ProcessOutput => "process_output",
    }
}

pub fn agent_run_wakeup_status_sql_value(status: AgentRunWakeupStatus) -> &'static str {
    match status {
        AgentRunWakeupStatus::Pending => "pending",
        AgentRunWakeupStatus::Fired => "fired",
        AgentRunWakeupStatus::Cancelled => "cancelled",
        AgentRunWakeupStatus::Expired => "expired",
        AgentRunWakeupStatus::Failed => "failed",
    }
}

fn parse_agent_run_wakeup_kind(value: &str) -> AgentRunWakeupKind {
    match value {
        "process_exit" => AgentRunWakeupKind::ProcessExit,
        "process_ready" => AgentRunWakeupKind::ProcessReady,
        "process_output" => AgentRunWakeupKind::ProcessOutput,
        _ => AgentRunWakeupKind::Sleep,
    }
}

fn parse_agent_run_wakeup_status(value: &str) -> AgentRunWakeupStatus {
    match value {
        "fired" => AgentRunWakeupStatus::Fired,
        "cancelled" => AgentRunWakeupStatus::Cancelled,
        "expired" => AgentRunWakeupStatus::Expired,
        "failed" => AgentRunWakeupStatus::Failed,
        _ => AgentRunWakeupStatus::Pending,
    }
}

fn optional_u64_to_i64(value: Option<u64>) -> Result<Option<i64>, CommandError> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| CommandError::invalid_request("pollIntervalMs"))
        })
        .transpose()
}

fn optional_i64_to_u64(value: Option<i64>, index: usize) -> rusqlite::Result<Option<u64>> {
    value.map(|value| i64_to_u64(value, index)).transpose()
}

fn i64_to_u64(value: i64, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn map_wakeup_store_query_error(
    repo_root: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not read scheduled wakeup state from {}: {error}",
            database_path_for_repo(repo_root).display()
        ),
    )
}

fn map_wakeup_store_write_error(
    repo_root: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not persist scheduled wakeup state to {}: {error}",
            database_path_for_repo(repo_root).display()
        ),
    )
}
