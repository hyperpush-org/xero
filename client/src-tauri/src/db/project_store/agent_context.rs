use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::Value as JsonValue;

use crate::{auth::now_timestamp, commands::CommandError, db::database_path_for_repo};

use super::{agent_core::AgentRunDiagnosticRecord, open_runtime_database};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCompactionTrigger {
    Manual,
    Auto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentCompactionRecord {
    pub id: i64,
    pub compaction_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub source_run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub summary: String,
    pub covered_run_ids: Vec<String>,
    pub covered_message_start_id: Option<i64>,
    pub covered_message_end_id: Option<i64>,
    pub covered_event_start_id: Option<i64>,
    pub covered_event_end_id: Option<i64>,
    pub source_hash: String,
    pub input_tokens: u64,
    pub summary_tokens: u64,
    pub raw_tail_message_count: u32,
    pub policy_reason: String,
    pub trigger: AgentCompactionTrigger,
    pub active: bool,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
    pub created_at: String,
    pub superseded_at: Option<String>,
}

impl AgentCompactionRecord {
    pub fn covers_run(&self, run_id: &str) -> bool {
        self.covered_run_ids.iter().any(|covered| covered == run_id)
    }

    pub fn covers_message_id(&self, message_id: i64) -> bool {
        matches!(
            (self.covered_message_start_id, self.covered_message_end_id),
            (Some(start), Some(end)) if message_id >= start && message_id <= end
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentCompactionRecord {
    pub compaction_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub source_run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub summary: String,
    pub covered_run_ids: Vec<String>,
    pub covered_message_start_id: Option<i64>,
    pub covered_message_end_id: Option<i64>,
    pub covered_event_start_id: Option<i64>,
    pub covered_event_end_id: Option<i64>,
    pub source_hash: String,
    pub input_tokens: u64,
    pub summary_tokens: u64,
    pub raw_tail_message_count: u32,
    pub policy_reason: String,
    pub trigger: AgentCompactionTrigger,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
    pub created_at: String,
}

pub fn insert_agent_compaction(
    repo_root: &Path,
    record: &NewAgentCompactionRecord,
) -> Result<AgentCompactionRecord, CommandError> {
    validate_new_compaction(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::retryable(
            "agent_compaction_transaction_failed",
            format!(
                "Xero could not start the agent-compaction transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    transaction
        .execute(
            r#"
            UPDATE agent_compactions
            SET active = 0,
                superseded_at = ?3
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND active = 1
            "#,
            params![
                record.project_id,
                record.agent_session_id,
                record.created_at
            ],
        )
        .map_err(map_agent_compaction_write_error)?;

    transaction
        .execute(
            r#"
            INSERT INTO agent_compactions (
                compaction_id,
                project_id,
                agent_session_id,
                source_run_id,
                provider_id,
                model_id,
                summary,
                covered_run_ids_json,
                covered_message_start_id,
                covered_message_end_id,
                covered_event_start_id,
                covered_event_end_id,
                source_hash,
                input_tokens,
                summary_tokens,
                raw_tail_message_count,
                policy_reason,
                trigger_kind,
                active,
                diagnostic_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, 1, ?19, ?20)
            "#,
            params![
                record.compaction_id,
                record.project_id,
                record.agent_session_id,
                record.source_run_id,
                record.provider_id,
                record.model_id,
                record.summary,
                serde_json::to_string(&record.covered_run_ids).map_err(|error| {
                    CommandError::system_fault(
                        "agent_compaction_covered_runs_serialize_failed",
                        format!("Xero could not serialize compaction coverage: {error}"),
                    )
                })?,
                record.covered_message_start_id,
                record.covered_message_end_id,
                record.covered_event_start_id,
                record.covered_event_end_id,
                record.source_hash,
                record.input_tokens,
                record.summary_tokens,
                record.raw_tail_message_count,
                record.policy_reason,
                agent_compaction_trigger_sql_value(&record.trigger),
                diagnostic_json(&record.diagnostic)?,
                record.created_at,
            ],
        )
        .map_err(map_agent_compaction_write_error)?;
    let inserted_id = transaction.last_insert_rowid();
    transaction.commit().map_err(|error| {
        CommandError::retryable(
            "agent_compaction_commit_failed",
            format!(
                "Xero could not commit the agent-compaction transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    read_agent_compaction_by_id_with_connection(&connection, &record.project_id, inserted_id)
}

pub fn load_active_agent_compaction(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Option<AgentCompactionRecord>, CommandError> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(agent_session_id, "agentSessionId")?;
    let connection = open_agent_context_database(repo_root)?;
    read_active_agent_compaction_with_connection(&connection, project_id, agent_session_id)
}

pub(crate) fn read_active_agent_compaction_with_connection(
    connection: &Connection,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Option<AgentCompactionRecord>, CommandError> {
    connection
        .query_row(
            compaction_select_sql(
                r#"
                WHERE project_id = ?1
                  AND agent_session_id = ?2
                  AND active = 1
                ORDER BY created_at DESC, id DESC
                LIMIT 1
                "#,
            )
            .as_str(),
            params![project_id, agent_session_id],
            read_agent_compaction_row,
        )
        .optional()
        .map_err(map_agent_compaction_read_error)?
        .transpose()
}

pub fn list_agent_compactions(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Vec<AgentCompactionRecord>, CommandError> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(agent_session_id, "agentSessionId")?;
    let connection = open_agent_context_database(repo_root)?;
    let sql = compaction_select_sql(
        r#"
        WHERE project_id = ?1
          AND agent_session_id = ?2
        ORDER BY created_at DESC, id DESC
        "#,
    );
    let mut statement = connection
        .prepare(sql.as_str())
        .map_err(map_agent_compaction_read_error)?;
    let rows = statement
        .query_map(
            params![project_id, agent_session_id],
            read_agent_compaction_row,
        )
        .map_err(map_agent_compaction_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_agent_compaction_read_error)?
        .into_iter()
        .collect()
}

pub fn supersede_agent_compaction(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    compaction_id: &str,
) -> Result<(), CommandError> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(agent_session_id, "agentSessionId")?;
    validate_non_empty(compaction_id, "compactionId")?;
    let connection = open_agent_context_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE agent_compactions
            SET active = 0,
                superseded_at = ?4
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND compaction_id = ?3
              AND active = 1
            "#,
            params![project_id, agent_session_id, compaction_id, now_timestamp()],
        )
        .map_err(map_agent_compaction_write_error)?;
    Ok(())
}

fn read_agent_compaction_by_id_with_connection(
    connection: &Connection,
    project_id: &str,
    id: i64,
) -> Result<AgentCompactionRecord, CommandError> {
    connection
        .query_row(
            compaction_select_sql("WHERE project_id = ?1 AND id = ?2").as_str(),
            params![project_id, id],
            read_agent_compaction_row,
        )
        .optional()
        .map_err(map_agent_compaction_read_error)?
        .transpose()?
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_compaction_insert_missing",
                "Xero inserted a compaction record but could not load it back.",
            )
        })
}

fn compaction_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            compaction_id,
            project_id,
            agent_session_id,
            source_run_id,
            provider_id,
            model_id,
            summary,
            covered_run_ids_json,
            covered_message_start_id,
            covered_message_end_id,
            covered_event_start_id,
            covered_event_end_id,
            source_hash,
            input_tokens,
            summary_tokens,
            raw_tail_message_count,
            policy_reason,
            trigger_kind,
            active,
            diagnostic_json,
            created_at,
            superseded_at
        FROM agent_compactions
        {where_clause}
        "#
    )
}

fn read_agent_compaction_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentCompactionRecord, CommandError>> {
    let covered_run_ids_json: String = row.get(8)?;
    let covered_run_ids =
        serde_json::from_str::<Vec<String>>(&covered_run_ids_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let diagnostic_json: Option<String> = row.get(20)?;
    let diagnostic = diagnostic_json
        .as_deref()
        .map(parse_diagnostic_json)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                20,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(Ok(AgentCompactionRecord {
        id: row.get(0)?,
        compaction_id: row.get(1)?,
        project_id: row.get(2)?,
        agent_session_id: row.get(3)?,
        source_run_id: row.get(4)?,
        provider_id: row.get(5)?,
        model_id: row.get(6)?,
        summary: row.get(7)?,
        covered_run_ids,
        covered_message_start_id: row.get(9)?,
        covered_message_end_id: row.get(10)?,
        covered_event_start_id: row.get(11)?,
        covered_event_end_id: row.get(12)?,
        source_hash: row.get(13)?,
        input_tokens: row.get(14)?,
        summary_tokens: row.get(15)?,
        raw_tail_message_count: row.get(16)?,
        policy_reason: row.get(17)?,
        trigger: parse_agent_compaction_trigger(row.get::<_, String>(18)?.as_str()),
        active: row.get::<_, i64>(19)? == 1,
        diagnostic,
        created_at: row.get(21)?,
        superseded_at: row.get(22)?,
    }))
}

fn diagnostic_json(
    diagnostic: &Option<AgentRunDiagnosticRecord>,
) -> Result<Option<String>, CommandError> {
    diagnostic
        .as_ref()
        .map(|diagnostic| {
            serde_json::to_string(&serde_json::json!({
                "code": diagnostic.code,
                "message": diagnostic.message,
            }))
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_compaction_diagnostic_serialize_failed",
                    format!("Xero could not serialize compaction diagnostic metadata: {error}"),
                )
            })
        })
        .transpose()
}

fn parse_diagnostic_json(value: &str) -> Result<AgentRunDiagnosticRecord, serde_json::Error> {
    let value = serde_json::from_str::<JsonValue>(value)?;
    Ok(AgentRunDiagnosticRecord {
        code: value
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("agent_compaction_diagnostic")
            .to_string(),
        message: value
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("Xero could not decode compaction diagnostic details.")
            .to_string(),
    })
}

fn validate_new_compaction(record: &NewAgentCompactionRecord) -> Result<(), CommandError> {
    validate_non_empty(&record.compaction_id, "compactionId")?;
    validate_non_empty(&record.project_id, "projectId")?;
    validate_non_empty(&record.agent_session_id, "agentSessionId")?;
    validate_non_empty(&record.source_run_id, "sourceRunId")?;
    validate_non_empty(&record.provider_id, "providerId")?;
    validate_non_empty(&record.model_id, "modelId")?;
    validate_non_empty(&record.summary, "summary")?;
    validate_non_empty(&record.policy_reason, "policyReason")?;
    validate_sha256(&record.source_hash, "sourceHash")?;
    if record.covered_run_ids.is_empty()
        || record
            .covered_run_ids
            .iter()
            .any(|run_id| run_id.trim().is_empty())
    {
        return Err(CommandError::invalid_request("coveredRunIds"));
    }
    validate_range(
        record.covered_message_start_id,
        record.covered_message_end_id,
        "coveredMessageRange",
    )?;
    validate_range(
        record.covered_event_start_id,
        record.covered_event_end_id,
        "coveredEventRange",
    )?;
    Ok(())
}

fn validate_range(
    start: Option<i64>,
    end: Option<i64>,
    field: &'static str,
) -> Result<(), CommandError> {
    match (start, end) {
        (Some(start), Some(end)) if start > 0 && start <= end => Ok(()),
        (None, None) => Ok(()),
        _ => Err(CommandError::invalid_request(field)),
    }
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &'static str) -> Result<(), CommandError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(CommandError::invalid_request(field))
    }
}

fn open_agent_context_database(repo_root: &Path) -> Result<rusqlite::Connection, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    open_runtime_database(repo_root, &database_path)
}

fn agent_compaction_trigger_sql_value(trigger: &AgentCompactionTrigger) -> &'static str {
    match trigger {
        AgentCompactionTrigger::Manual => "manual",
        AgentCompactionTrigger::Auto => "auto",
    }
}

fn parse_agent_compaction_trigger(value: &str) -> AgentCompactionTrigger {
    match value {
        "auto" => AgentCompactionTrigger::Auto,
        _ => AgentCompactionTrigger::Manual,
    }
}

fn map_agent_compaction_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "agent_compaction_read_failed",
        format!("Xero could not read session compaction records: {error}"),
    )
}

fn map_agent_compaction_write_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "agent_compaction_write_failed",
        format!("Xero could not write session compaction records: {error}"),
    )
}
