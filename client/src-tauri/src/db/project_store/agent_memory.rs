use std::path::Path;

use rand::RngCore;
use rusqlite::{params, OptionalExtension, Row};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{auth::now_timestamp, commands::CommandError, db::database_path_for_repo};

use super::{agent_core::AgentRunDiagnosticRecord, open_runtime_database, read_project_row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMemoryScope {
    Project,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMemoryKind {
    ProjectFact,
    UserPreference,
    Decision,
    SessionSummary,
    Troubleshooting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMemoryReviewState {
    Candidate,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMemoryRecord {
    pub id: i64,
    pub memory_id: String,
    pub project_id: String,
    pub agent_session_id: Option<String>,
    pub scope: AgentMemoryScope,
    pub kind: AgentMemoryKind,
    pub text: String,
    pub text_hash: String,
    pub review_state: AgentMemoryReviewState,
    pub enabled: bool,
    pub confidence: Option<u8>,
    pub source_run_id: Option<String>,
    pub source_item_ids: Vec<String>,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentMemoryRecord {
    pub memory_id: String,
    pub project_id: String,
    pub agent_session_id: Option<String>,
    pub scope: AgentMemoryScope,
    pub kind: AgentMemoryKind,
    pub text: String,
    pub review_state: AgentMemoryReviewState,
    pub enabled: bool,
    pub confidence: Option<u8>,
    pub source_run_id: Option<String>,
    pub source_item_ids: Vec<String>,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMemoryUpdateRecord {
    pub project_id: String,
    pub memory_id: String,
    pub review_state: Option<AgentMemoryReviewState>,
    pub enabled: Option<bool>,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgentMemoryListFilter<'a> {
    pub agent_session_id: Option<&'a str>,
    pub include_disabled: bool,
    pub include_rejected: bool,
}

pub fn generate_agent_memory_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "memory-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub fn agent_memory_text_hash(text: &str) -> String {
    let normalized = normalize_memory_text(text);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn insert_agent_memory(
    repo_root: &Path,
    record: &NewAgentMemoryRecord,
) -> Result<AgentMemoryRecord, CommandError> {
    validate_new_agent_memory(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    let text_hash = agent_memory_text_hash(&record.text);
    connection
        .execute(
            r#"
            INSERT INTO agent_memories (
                memory_id,
                project_id,
                agent_session_id,
                scope_kind,
                memory_kind,
                text,
                text_hash,
                review_state,
                enabled,
                confidence,
                source_run_id,
                source_item_ids_json,
                diagnostic_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?14)
            "#,
            params![
                record.memory_id,
                record.project_id,
                record.agent_session_id,
                agent_memory_scope_sql_value(&record.scope),
                agent_memory_kind_sql_value(&record.kind),
                record.text.trim(),
                text_hash,
                agent_memory_review_state_sql_value(&record.review_state),
                if record.enabled { 1 } else { 0 },
                record.confidence,
                record.source_run_id,
                serde_json::to_string(&record.source_item_ids).map_err(|error| {
                    CommandError::system_fault(
                        "agent_memory_source_items_serialize_failed",
                        format!("Cadence could not serialize memory source item ids: {error}"),
                    )
                })?,
                diagnostic_json(&record.diagnostic)?,
                record.created_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_memory_write_failed",
                format!(
                    "Cadence could not persist reviewed session memory in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    read_agent_memory_by_id(
        repo_root,
        &record.project_id,
        connection.last_insert_rowid(),
    )
}

pub fn list_agent_memories(
    repo_root: &Path,
    project_id: &str,
    filter: AgentMemoryListFilter<'_>,
) -> Result<Vec<AgentMemoryRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    if let Some(agent_session_id) = filter.agent_session_id {
        validate_non_empty_text(agent_session_id, "agentSessionId")?;
    }
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let mut statement = connection
        .prepare(
            agent_memory_select_sql(
                r#"
                WHERE project_id = ?1
                  AND (
                    scope_kind = 'project'
                    OR (?2 IS NOT NULL AND agent_session_id = ?2)
                  )
                  AND (?3 = 1 OR enabled = 1 OR review_state = 'candidate')
                  AND (?4 = 1 OR review_state <> 'rejected')
                ORDER BY
                    CASE scope_kind WHEN 'project' THEN 0 ELSE 1 END ASC,
                    CASE memory_kind
                        WHEN 'project_fact' THEN 0
                        WHEN 'decision' THEN 1
                        WHEN 'user_preference' THEN 2
                        WHEN 'troubleshooting' THEN 3
                        ELSE 4
                    END ASC,
                    updated_at DESC,
                    id DESC
                "#,
            )
            .as_str(),
        )
        .map_err(map_agent_memory_read_error)?;
    let rows = statement
        .query_map(
            params![
                project_id,
                filter.agent_session_id,
                if filter.include_disabled { 1 } else { 0 },
                if filter.include_rejected { 1 } else { 0 },
            ],
            read_agent_memory_row,
        )
        .map_err(map_agent_memory_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_agent_memory_read_error)?
        .into_iter()
        .collect()
}

pub fn list_approved_agent_memories(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: Option<&str>,
) -> Result<Vec<AgentMemoryRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    if let Some(agent_session_id) = agent_session_id {
        validate_non_empty_text(agent_session_id, "agentSessionId")?;
    }
    let connection = open_agent_memory_database(repo_root)?;
    let mut statement = connection
        .prepare(
            agent_memory_select_sql(
                r#"
                WHERE project_id = ?1
                  AND review_state = 'approved'
                  AND enabled = 1
                  AND (
                    scope_kind = 'project'
                    OR (?2 IS NOT NULL AND agent_session_id = ?2)
                  )
                ORDER BY
                    CASE scope_kind WHEN 'project' THEN 0 ELSE 1 END ASC,
                    CASE memory_kind
                        WHEN 'project_fact' THEN 0
                        WHEN 'decision' THEN 1
                        WHEN 'user_preference' THEN 2
                        WHEN 'troubleshooting' THEN 3
                        ELSE 4
                    END ASC,
                    created_at ASC,
                    memory_id ASC
                "#,
            )
            .as_str(),
        )
        .map_err(map_agent_memory_read_error)?;
    let rows = statement
        .query_map(params![project_id, agent_session_id], read_agent_memory_row)
        .map_err(map_agent_memory_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_agent_memory_read_error)?
        .into_iter()
        .collect()
}

pub fn find_active_agent_memory_by_hash(
    repo_root: &Path,
    project_id: &str,
    scope: &AgentMemoryScope,
    agent_session_id: Option<&str>,
    kind: &AgentMemoryKind,
    text_hash: &str,
) -> Result<Option<AgentMemoryRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_sha256(text_hash, "textHash")?;
    let connection = open_agent_memory_database(repo_root)?;
    connection
        .query_row(
            agent_memory_select_sql(
                r#"
                WHERE project_id = ?1
                  AND scope_kind = ?2
                  AND COALESCE(agent_session_id, '') = COALESCE(?3, '')
                  AND memory_kind = ?4
                  AND text_hash = ?5
                  AND review_state IN ('candidate', 'approved')
                ORDER BY updated_at DESC, id DESC
                LIMIT 1
                "#,
            )
            .as_str(),
            params![
                project_id,
                agent_memory_scope_sql_value(scope),
                agent_session_id,
                agent_memory_kind_sql_value(kind),
                text_hash,
            ],
            read_agent_memory_row,
        )
        .optional()
        .map_err(map_agent_memory_read_error)?
        .transpose()
}

pub fn update_agent_memory(
    repo_root: &Path,
    update: &AgentMemoryUpdateRecord,
) -> Result<AgentMemoryRecord, CommandError> {
    validate_non_empty_text(&update.project_id, "projectId")?;
    validate_non_empty_text(&update.memory_id, "memoryId")?;
    if update.review_state.is_none() && update.enabled.is_none() && update.diagnostic.is_none() {
        return Err(CommandError::invalid_request("memoryUpdate"));
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let existing =
        read_agent_memory_by_memory_id(repo_root, &update.project_id, &update.memory_id)?;
    let next_review_state = update
        .review_state
        .clone()
        .unwrap_or(existing.review_state.clone());
    let mut next_enabled = update.enabled.unwrap_or(existing.enabled);
    if next_review_state != AgentMemoryReviewState::Approved {
        next_enabled = false;
    }
    let now = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE agent_memories
            SET review_state = ?3,
                enabled = ?4,
                diagnostic_json = COALESCE(?5, diagnostic_json),
                updated_at = ?6
            WHERE project_id = ?1
              AND memory_id = ?2
            "#,
            params![
                update.project_id,
                update.memory_id,
                agent_memory_review_state_sql_value(&next_review_state),
                if next_enabled { 1 } else { 0 },
                diagnostic_json(&update.diagnostic)?,
                now,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_memory_update_failed",
                format!(
                    "Cadence could not update reviewed session memory in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    read_agent_memory_by_memory_id(repo_root, &update.project_id, &update.memory_id)
}

pub fn get_agent_memory(
    repo_root: &Path,
    project_id: &str,
    memory_id: &str,
) -> Result<AgentMemoryRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(memory_id, "memoryId")?;
    read_agent_memory_by_memory_id(repo_root, project_id, memory_id)
}

pub fn delete_agent_memory(
    repo_root: &Path,
    project_id: &str,
    memory_id: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(memory_id, "memoryId")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let affected = connection
        .execute(
            r#"
            DELETE FROM agent_memories
            WHERE project_id = ?1
              AND memory_id = ?2
            "#,
            params![project_id, memory_id],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_memory_delete_failed",
                format!(
                    "Cadence could not delete reviewed session memory from {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    if affected == 0 {
        return Err(missing_agent_memory_error(project_id, memory_id));
    }
    Ok(())
}

fn read_agent_memory_by_id(
    repo_root: &Path,
    project_id: &str,
    id: i64,
) -> Result<AgentMemoryRecord, CommandError> {
    let connection = open_agent_memory_database(repo_root)?;
    connection
        .query_row(
            agent_memory_select_sql("WHERE project_id = ?1 AND id = ?2").as_str(),
            params![project_id, id],
            read_agent_memory_row,
        )
        .optional()
        .map_err(map_agent_memory_read_error)?
        .transpose()?
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_memory_insert_missing",
                "Cadence persisted a memory record but could not load it back.",
            )
        })
}

fn read_agent_memory_by_memory_id(
    repo_root: &Path,
    project_id: &str,
    memory_id: &str,
) -> Result<AgentMemoryRecord, CommandError> {
    let connection = open_agent_memory_database(repo_root)?;
    connection
        .query_row(
            agent_memory_select_sql("WHERE project_id = ?1 AND memory_id = ?2").as_str(),
            params![project_id, memory_id],
            read_agent_memory_row,
        )
        .optional()
        .map_err(map_agent_memory_read_error)?
        .transpose()?
        .ok_or_else(|| missing_agent_memory_error(project_id, memory_id))
}

fn agent_memory_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            memory_id,
            project_id,
            agent_session_id,
            scope_kind,
            memory_kind,
            text,
            text_hash,
            review_state,
            enabled,
            confidence,
            source_run_id,
            source_item_ids_json,
            diagnostic_json,
            created_at,
            updated_at
        FROM agent_memories
        {where_clause}
        "#
    )
}

fn read_agent_memory_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentMemoryRecord, CommandError>> {
    let source_item_ids_json: String = row.get(12)?;
    let source_item_ids =
        serde_json::from_str::<Vec<String>>(&source_item_ids_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    let diagnostic_json: Option<String> = row.get(13)?;
    let diagnostic = diagnostic_json
        .as_deref()
        .map(parse_diagnostic_json)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                13,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(Ok(AgentMemoryRecord {
        id: row.get(0)?,
        memory_id: row.get(1)?,
        project_id: row.get(2)?,
        agent_session_id: row.get(3)?,
        scope: parse_agent_memory_scope(row.get::<_, String>(4)?.as_str()),
        kind: parse_agent_memory_kind(row.get::<_, String>(5)?.as_str()),
        text: row.get(6)?,
        text_hash: row.get(7)?,
        review_state: parse_agent_memory_review_state(row.get::<_, String>(8)?.as_str()),
        enabled: row.get::<_, i64>(9)? == 1,
        confidence: row.get(10)?,
        source_run_id: row.get(11)?,
        source_item_ids,
        diagnostic,
        created_at: row.get(14)?,
        updated_at: row.get(15)?,
    }))
}

fn validate_new_agent_memory(record: &NewAgentMemoryRecord) -> Result<(), CommandError> {
    validate_non_empty_text(&record.memory_id, "memoryId")?;
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.text, "text")?;
    validate_non_empty_text(&record.created_at, "createdAt")?;
    match record.scope {
        AgentMemoryScope::Project if record.agent_session_id.is_some() => {
            return Err(CommandError::invalid_request("agentSessionId"));
        }
        AgentMemoryScope::Session => {
            validate_non_empty_text(
                record.agent_session_id.as_deref().unwrap_or_default(),
                "agentSessionId",
            )?;
        }
        AgentMemoryScope::Project => {}
    }
    if record.enabled && record.review_state != AgentMemoryReviewState::Approved {
        return Err(CommandError::invalid_request("enabled"));
    }
    if record
        .source_item_ids
        .iter()
        .any(|item_id| item_id.trim().is_empty())
    {
        return Err(CommandError::invalid_request("sourceItemIds"));
    }
    if let Some(source_run_id) = record.source_run_id.as_deref() {
        validate_non_empty_text(source_run_id, "sourceRunId")?;
    }
    Ok(())
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
                    "agent_memory_diagnostic_serialize_failed",
                    format!("Cadence could not serialize memory diagnostic metadata: {error}"),
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
            .unwrap_or("agent_memory_diagnostic")
            .to_string(),
        message: value
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("Cadence could not decode memory diagnostic details.")
            .to_string(),
    })
}

fn normalize_memory_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
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

fn validate_non_empty_text(value: &str, field: &'static str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn open_agent_memory_database(repo_root: &Path) -> Result<rusqlite::Connection, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    open_runtime_database(repo_root, &database_path)
}

fn agent_memory_scope_sql_value(scope: &AgentMemoryScope) -> &'static str {
    match scope {
        AgentMemoryScope::Project => "project",
        AgentMemoryScope::Session => "session",
    }
}

fn parse_agent_memory_scope(value: &str) -> AgentMemoryScope {
    match value {
        "session" => AgentMemoryScope::Session,
        _ => AgentMemoryScope::Project,
    }
}

fn agent_memory_kind_sql_value(kind: &AgentMemoryKind) -> &'static str {
    match kind {
        AgentMemoryKind::ProjectFact => "project_fact",
        AgentMemoryKind::UserPreference => "user_preference",
        AgentMemoryKind::Decision => "decision",
        AgentMemoryKind::SessionSummary => "session_summary",
        AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn parse_agent_memory_kind(value: &str) -> AgentMemoryKind {
    match value {
        "user_preference" => AgentMemoryKind::UserPreference,
        "decision" => AgentMemoryKind::Decision,
        "session_summary" => AgentMemoryKind::SessionSummary,
        "troubleshooting" => AgentMemoryKind::Troubleshooting,
        _ => AgentMemoryKind::ProjectFact,
    }
}

fn agent_memory_review_state_sql_value(review_state: &AgentMemoryReviewState) -> &'static str {
    match review_state {
        AgentMemoryReviewState::Candidate => "candidate",
        AgentMemoryReviewState::Approved => "approved",
        AgentMemoryReviewState::Rejected => "rejected",
    }
}

fn parse_agent_memory_review_state(value: &str) -> AgentMemoryReviewState {
    match value {
        "approved" => AgentMemoryReviewState::Approved,
        "rejected" => AgentMemoryReviewState::Rejected,
        _ => AgentMemoryReviewState::Candidate,
    }
}

fn missing_agent_memory_error(project_id: &str, memory_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_memory_not_found",
        format!("Cadence could not find memory `{memory_id}` for project `{project_id}`."),
    )
}

fn map_agent_memory_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "agent_memory_read_failed",
        format!("Cadence could not read reviewed session memory: {error}"),
    )
}
