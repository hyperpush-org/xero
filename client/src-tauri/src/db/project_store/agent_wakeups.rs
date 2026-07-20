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
        kind: parse_agent_run_wakeup_kind(&row.get::<_, String>(4)?, 4)?,
        due_at: row.get(5)?,
        deadline_at: row.get(6)?,
        poll_interval_ms,
        payload_json: row.get(8)?,
        status: parse_agent_run_wakeup_status(&row.get::<_, String>(9)?, 9)?,
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

fn parse_agent_run_wakeup_kind(value: &str, index: usize) -> rusqlite::Result<AgentRunWakeupKind> {
    match value {
        "sleep" => Ok(AgentRunWakeupKind::Sleep),
        "process_exit" => Ok(AgentRunWakeupKind::ProcessExit),
        "process_ready" => Ok(AgentRunWakeupKind::ProcessReady),
        "process_output" => Ok(AgentRunWakeupKind::ProcessOutput),
        _ => Err(invalid_wakeup_enum_value(index, "kind", value)),
    }
}

fn parse_agent_run_wakeup_status(
    value: &str,
    index: usize,
) -> rusqlite::Result<AgentRunWakeupStatus> {
    match value {
        "pending" => Ok(AgentRunWakeupStatus::Pending),
        "fired" => Ok(AgentRunWakeupStatus::Fired),
        "cancelled" => Ok(AgentRunWakeupStatus::Cancelled),
        "expired" => Ok(AgentRunWakeupStatus::Expired),
        "failed" => Ok(AgentRunWakeupStatus::Failed),
        _ => Err(invalid_wakeup_enum_value(index, "status", value)),
    }
}

fn invalid_wakeup_enum_value(index: usize, field: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        index,
        rusqlite::types::Type::Text,
        Box::new(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unknown agent wakeup {field} `{value}`"),
        )),
    )
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::RuntimeAgentIdDto, db, git::repository::CanonicalRepository, state::DesktopState,
    };

    #[test]
    fn corrupted_wakeup_kind_and_status_fail_closed_instead_of_becoming_pending_sleep() {
        let fixture = tempfile::tempdir().expect("create wakeup fixture");
        let repo_root = seed_project(&fixture);
        seed_run_and_wakeup(&repo_root);

        for record in [
            new_wakeup(
                "wake-2",
                AgentRunWakeupKind::ProcessExit,
                "2026-07-18T12:00:00Z",
            ),
            new_wakeup(
                "wake-3",
                AgentRunWakeupKind::ProcessReady,
                "2026-07-18T12:00:02Z",
            ),
            new_wakeup(
                "wake-4",
                AgentRunWakeupKind::ProcessOutput,
                "2026-07-18T12:00:03Z",
            ),
        ] {
            insert_agent_run_wakeup(&repo_root, &record).expect("insert wakeup kind fixture");
        }
        let pending = list_pending_agent_run_wakeups(&repo_root).expect("list fixture wakeups");
        assert_eq!(
            pending
                .iter()
                .map(|record| record.wake_id.as_str())
                .collect::<Vec<_>>(),
            vec!["wake-2", "wake-1", "wake-3", "wake-4"]
        );
        assert_eq!(
            pending.iter().map(|record| record.kind).collect::<Vec<_>>(),
            vec![
                AgentRunWakeupKind::ProcessExit,
                AgentRunWakeupKind::Sleep,
                AgentRunWakeupKind::ProcessReady,
                AgentRunWakeupKind::ProcessOutput,
            ]
        );
        assert_eq!(
            list_pending_agent_run_wakeups_for_run(&repo_root, "project-1", "run-1")
                .expect("list run wakeups")
                .len(),
            4
        );

        let expired = mark_agent_run_wakeup_status(
            &repo_root,
            "project-1",
            "run-1",
            "wake-2",
            AgentRunWakeupStatus::Expired,
            Some(AgentRunDiagnosticRecord {
                code: "deadline".into(),
                message: "Deadline elapsed.".into(),
            }),
            "2026-07-18T12:00:10Z",
        )
        .expect("expire wakeup");
        assert_eq!(expired.status, AgentRunWakeupStatus::Expired);
        assert_eq!(
            expired.last_error.expect("expiry diagnostic").code,
            "deadline"
        );
        assert_eq!(
            mark_agent_run_wakeup_status(
                &repo_root,
                "project-1",
                "run-1",
                "wake-3",
                AgentRunWakeupStatus::Failed,
                None,
                "2026-07-18T12:00:11Z",
            )
            .expect("fail wakeup")
            .status,
            AgentRunWakeupStatus::Failed
        );
        assert_eq!(
            mark_agent_run_wakeup_status(
                &repo_root,
                "project-1",
                "run-1",
                "wake-4",
                AgentRunWakeupStatus::Cancelled,
                None,
                "2026-07-18T12:00:12Z",
            )
            .expect("cancel wakeup")
            .status,
            AgentRunWakeupStatus::Cancelled
        );
        let rescheduled = reschedule_agent_run_wakeup(
            &repo_root,
            "project-1",
            "run-1",
            "wake-1",
            "2026-07-18T12:00:20Z",
            r#"{"reason":"updated"}"#,
            "2026-07-18T12:00:13Z",
        )
        .expect("reschedule pending wakeup");
        assert_eq!(rescheduled.attempt_count, 1);
        assert_eq!(
            rescheduled.payload().expect("rescheduled payload")["reason"],
            "updated"
        );
        assert!(mark_agent_run_wakeup_fired(
            &repo_root,
            "project-1",
            "run-1",
            "wake-1",
            "2026-07-18T12:00:20Z",
        )
        .expect("fire rescheduled wakeup"));
        assert!(
            maybe_load_pending_agent_run_wakeup(&repo_root, "project-1", "run-1", "wake-1",)
                .expect("load terminal pending wakeup")
                .is_none()
        );
        assert!(list_pending_agent_run_wakeups(&repo_root)
            .expect("list after terminal updates")
            .is_empty());
        assert_eq!(
            insert_agent_run_wakeup(
                &repo_root,
                &new_wakeup("wake-1", AgentRunWakeupKind::Sleep, "2026-07-18T12:00:30Z",)
            )
            .expect_err("duplicate wakeup ids must be rejected")
            .code,
            "agent_run_wakeup_insert_failed"
        );

        let database_path = database_path_for_repo(&repo_root);
        let connection = rusqlite::Connection::open(database_path).expect("open fixture database");
        connection
            .execute_batch("PRAGMA ignore_check_constraints = ON;")
            .expect("enable corruption fixture");

        connection
            .execute(
                "UPDATE agent_run_wakeups SET kind = 'future_kind' WHERE wake_id = 'wake-1'",
                [],
            )
            .expect("corrupt fixture kind");
        assert_eq!(
            load_agent_run_wakeup(&repo_root, "project-1", "run-1", "wake-1")
                .expect_err("unknown wakeup kind must fail closed")
                .code,
            "agent_run_wakeup_read_failed"
        );

        connection
            .execute(
                "UPDATE agent_run_wakeups SET kind = 'sleep', status = 'future_status' WHERE wake_id = 'wake-1'",
                [],
            )
            .expect("corrupt fixture status");
        assert_eq!(
            load_agent_run_wakeup(&repo_root, "project-1", "run-1", "wake-1")
                .expect_err("unknown wakeup status must fail closed")
                .code,
            "agent_run_wakeup_read_failed"
        );
    }

    #[test]
    fn wakeup_validation_payload_decoding_and_sql_mappings_cover_all_boundaries() {
        let valid = new_wakeup(
            "wake-validation",
            AgentRunWakeupKind::Sleep,
            "2026-07-18T12:00:00Z",
        );
        validate_new_wakeup(&valid).expect("valid wakeup fixture");
        for invalid in [
            NewAgentRunWakeupRecord {
                project_id: " ".into(),
                ..valid.clone()
            },
            NewAgentRunWakeupRecord {
                agent_session_id: "".into(),
                ..valid.clone()
            },
            NewAgentRunWakeupRecord {
                run_id: "".into(),
                ..valid.clone()
            },
            NewAgentRunWakeupRecord {
                wake_id: "".into(),
                ..valid.clone()
            },
            NewAgentRunWakeupRecord {
                due_at: "".into(),
                ..valid.clone()
            },
            NewAgentRunWakeupRecord {
                deadline_at: Some(" ".into()),
                ..valid.clone()
            },
            NewAgentRunWakeupRecord {
                payload_json: "malformed".into(),
                ..valid.clone()
            },
            NewAgentRunWakeupRecord {
                created_at: "".into(),
                ..valid.clone()
            },
        ] {
            assert_eq!(
                validate_new_wakeup(&invalid)
                    .expect_err("reject invalid wakeup fixture")
                    .code,
                "invalid_request"
            );
        }

        let malformed = AgentRunWakeupRecord {
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            wake_id: "wake-bad".into(),
            kind: AgentRunWakeupKind::Sleep,
            due_at: "2026-07-18T12:00:00Z".into(),
            deadline_at: None,
            poll_interval_ms: None,
            payload_json: "malformed".into(),
            status: AgentRunWakeupStatus::Pending,
            attempt_count: 0,
            last_error: None,
            fired_at: None,
            created_at: "2026-07-18T12:00:00Z".into(),
            updated_at: "2026-07-18T12:00:00Z".into(),
        };
        assert_eq!(
            malformed
                .payload()
                .expect_err("malformed durable payload must be diagnosed")
                .code,
            "agent_run_wakeup_payload_decode_failed"
        );

        assert_eq!(optional_u64_to_i64(None).expect("none interval"), None);
        assert_eq!(
            optional_u64_to_i64(Some(42)).expect("valid interval"),
            Some(42)
        );
        assert_eq!(
            optional_u64_to_i64(Some(u64::MAX))
                .expect_err("oversized interval")
                .code,
            "invalid_request"
        );
        assert_eq!(
            optional_i64_to_u64(None, 1).expect("none database interval"),
            None
        );
        assert!(optional_i64_to_u64(Some(-1), 1).is_err());
        assert_eq!(i64_to_u64(42, 1).expect("valid database integer"), 42);
        assert!(i64_to_u64(-1, 1).is_err());
        assert!(agent_run_wakeup_select_sql("WHERE wake_id = ?1").contains("WHERE wake_id = ?1"));
        assert!(parse_agent_run_wakeup_kind("unknown", 4).is_err());
        assert!(parse_agent_run_wakeup_status("unknown", 9).is_err());

        let unused_root = Path::new("/unused-wakeup-validation-root");
        assert_invalid(load_agent_run_wakeup(unused_root, "", "run-1", "wake-1"));
        assert_invalid(load_agent_run_wakeup(
            unused_root,
            "project-1",
            "",
            "wake-1",
        ));
        assert_invalid(load_agent_run_wakeup(unused_root, "project-1", "run-1", ""));
        assert_invalid(list_pending_agent_run_wakeups_for_run(
            unused_root,
            "",
            "run-1",
        ));
        assert_invalid(list_pending_agent_run_wakeups_for_run(
            unused_root,
            "project-1",
            "",
        ));
        assert_invalid(maybe_load_pending_agent_run_wakeup(
            unused_root,
            "",
            "run-1",
            "wake-1",
        ));
        assert_invalid(maybe_load_pending_agent_run_wakeup(
            unused_root,
            "project-1",
            "",
            "wake-1",
        ));
        assert_invalid(maybe_load_pending_agent_run_wakeup(
            unused_root,
            "project-1",
            "run-1",
            "",
        ));
        assert_invalid(mark_agent_run_wakeup_fired(
            unused_root,
            "",
            "run-1",
            "wake-1",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(mark_agent_run_wakeup_fired(
            unused_root,
            "project-1",
            "",
            "wake-1",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(mark_agent_run_wakeup_fired(
            unused_root,
            "project-1",
            "run-1",
            "",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(mark_agent_run_wakeup_fired(
            unused_root,
            "project-1",
            "run-1",
            "wake-1",
            "",
        ));
        assert_invalid(reschedule_agent_run_wakeup(
            unused_root,
            "",
            "run-1",
            "wake-1",
            "2026-07-18T12:00:00Z",
            "{}",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(reschedule_agent_run_wakeup(
            unused_root,
            "project-1",
            "",
            "wake-1",
            "2026-07-18T12:00:00Z",
            "{}",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(reschedule_agent_run_wakeup(
            unused_root,
            "project-1",
            "run-1",
            "",
            "2026-07-18T12:00:00Z",
            "{}",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(reschedule_agent_run_wakeup(
            unused_root,
            "project-1",
            "run-1",
            "wake-1",
            "",
            "{}",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(reschedule_agent_run_wakeup(
            unused_root,
            "project-1",
            "run-1",
            "wake-1",
            "2026-07-18T12:00:00Z",
            "malformed",
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(reschedule_agent_run_wakeup(
            unused_root,
            "project-1",
            "run-1",
            "wake-1",
            "2026-07-18T12:00:00Z",
            "{}",
            "",
        ));
        assert_invalid(mark_agent_run_wakeup_status(
            unused_root,
            "",
            "run-1",
            "wake-1",
            AgentRunWakeupStatus::Failed,
            None,
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(mark_agent_run_wakeup_status(
            unused_root,
            "project-1",
            "",
            "wake-1",
            AgentRunWakeupStatus::Failed,
            None,
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(mark_agent_run_wakeup_status(
            unused_root,
            "project-1",
            "run-1",
            "",
            AgentRunWakeupStatus::Failed,
            None,
            "2026-07-18T12:00:00Z",
        ));
        assert_invalid(mark_agent_run_wakeup_status(
            unused_root,
            "project-1",
            "run-1",
            "wake-1",
            AgentRunWakeupStatus::Failed,
            None,
            "",
        ));

        assert_eq!(
            map_wakeup_store_query_error(
                unused_root,
                "query_fixture",
                rusqlite::Error::InvalidQuery
            )
            .code,
            "query_fixture"
        );
        assert_eq!(
            map_wakeup_store_write_error(
                unused_root,
                "write_fixture",
                rusqlite::Error::InvalidQuery
            )
            .code,
            "write_fixture"
        );

        for (kind, sql) in [
            (AgentRunWakeupKind::Sleep, "sleep"),
            (AgentRunWakeupKind::ProcessExit, "process_exit"),
            (AgentRunWakeupKind::ProcessReady, "process_ready"),
            (AgentRunWakeupKind::ProcessOutput, "process_output"),
        ] {
            assert_eq!(agent_run_wakeup_kind_sql_value(kind), sql);
            assert_eq!(
                parse_agent_run_wakeup_kind(sql, 0).expect("parse kind"),
                kind
            );
        }
        for (status, sql) in [
            (AgentRunWakeupStatus::Pending, "pending"),
            (AgentRunWakeupStatus::Fired, "fired"),
            (AgentRunWakeupStatus::Cancelled, "cancelled"),
            (AgentRunWakeupStatus::Expired, "expired"),
            (AgentRunWakeupStatus::Failed, "failed"),
        ] {
            assert_eq!(agent_run_wakeup_status_sql_value(status), sql);
            assert_eq!(
                parse_agent_run_wakeup_status(sql, 0).expect("parse status"),
                status
            );
        }
    }

    fn assert_invalid<T: std::fmt::Debug>(result: Result<T, CommandError>) {
        assert_eq!(
            result.expect_err("fixture must be rejected").code,
            "invalid_request"
        );
    }

    fn seed_project(root: &tempfile::TempDir) -> std::path::PathBuf {
        let repo_root = root.path().join("repo");
        std::fs::create_dir_all(&repo_root).expect("create fixture repository");
        let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical fixture root");
        let repository = CanonicalRepository {
            project_id: "project-1".into(),
            repository_id: "repository-1".into(),
            root_path: canonical_root.clone(),
            root_path_string: canonical_root.to_string_lossy().into_owned(),
            common_git_dir: canonical_root.join(".git"),
            display_name: "Wakeup fixture".into(),
            branch_name: Some("main".into()),
            head_sha: Some("abc123".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };
        db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
        db::import_project(&repository, DesktopState::default().import_failpoints())
            .expect("import fixture project");
        canonical_root
    }

    fn seed_run_and_wakeup(repo_root: &Path) {
        super::super::insert_agent_run(
            repo_root,
            &super::super::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: Some("engineer".into()),
                agent_definition_version: Some(1),
                project_id: "project-1".into(),
                agent_session_id: super::super::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-1".into(),
                provider_id: "fixture-provider".into(),
                model_id: "fixture-model".into(),
                prompt: "Wait.".into(),
                system_prompt: "fixture".into(),
                now: "2026-07-18T12:00:00Z".into(),
            },
        )
        .expect("insert fixture run");
        insert_agent_run_wakeup(
            repo_root,
            &NewAgentRunWakeupRecord {
                project_id: "project-1".into(),
                agent_session_id: super::super::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-1".into(),
                wake_id: "wake-1".into(),
                kind: AgentRunWakeupKind::Sleep,
                due_at: "2026-07-18T12:00:01Z".into(),
                deadline_at: None,
                poll_interval_ms: None,
                payload_json: r#"{"reason":"fixture"}"#.into(),
                created_at: "2026-07-18T12:00:00Z".into(),
            },
        )
        .expect("insert fixture wakeup");
    }

    fn new_wakeup(
        wake_id: &str,
        kind: AgentRunWakeupKind,
        due_at: &str,
    ) -> NewAgentRunWakeupRecord {
        NewAgentRunWakeupRecord {
            project_id: "project-1".into(),
            agent_session_id: super::super::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: "run-1".into(),
            wake_id: wake_id.into(),
            kind,
            due_at: due_at.into(),
            deadline_at: Some("2026-07-18T12:05:00Z".into()),
            poll_interval_ms: Some(1_000),
            payload_json: format!(r#"{{"reason":"{wake_id}"}}"#),
            created_at: format!("2026-07-18T12:00:{:02}Z", wake_id.len()),
        }
    }
}
