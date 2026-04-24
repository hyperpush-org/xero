use std::path::Path;

use rand::RngCore;
use rusqlite::{params, Connection, Error as SqlError, Transaction};

use crate::{
    auth::now_timestamp,
    commands::CommandError,
    db::database_path_for_repo,
};

use super::{
    decode_optional_non_empty_text, open_runtime_database, read_project_row, require_non_empty_owned,
    validate_non_empty_text,
};

pub const DEFAULT_AGENT_SESSION_ID: &str = "agent-session-main";
pub const DEFAULT_AGENT_SESSION_TITLE: &str = "Main";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionStatus {
    Active,
    Archived,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub title: String,
    pub summary: String,
    pub status: AgentSessionStatus,
    pub selected: bool,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub last_run_id: Option<String>,
    pub last_runtime_kind: Option<String>,
    pub last_provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionCreateRecord {
    pub project_id: String,
    pub title: String,
    pub summary: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSessionUpdateRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub title: Option<String>,
    pub summary: Option<String>,
    pub selected: Option<bool>,
}

#[derive(Debug)]
struct RawAgentSessionRow {
    project_id: String,
    agent_session_id: String,
    title: String,
    summary: String,
    status: String,
    selected: i64,
    created_at: String,
    updated_at: String,
    archived_at: Option<String>,
    last_run_id: Option<String>,
    last_runtime_kind: Option<String>,
    last_provider_id: Option<String>,
}

pub fn generate_agent_session_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "agent-session-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub fn create_agent_session(
    repo_root: &Path,
    payload: &AgentSessionCreateRecord,
) -> Result<AgentSessionRecord, CommandError> {
    validate_non_empty_text(&payload.project_id, "projectId", "agent_session_request_invalid")?;
    validate_non_empty_text(&payload.title, "title", "agent_session_request_invalid")?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &payload.project_id)?;

    let agent_session_id = generate_agent_session_id();
    let now = now_timestamp();
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "agent_session_transaction_failed",
            format!(
                "Cadence could not start the agent-session transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    if payload.selected {
        clear_selected_agent_session(&transaction, &database_path, &payload.project_id)?;
    }

    transaction
        .execute(
            r#"
            INSERT INTO agent_sessions (
                project_id,
                agent_session_id,
                title,
                summary,
                status,
                selected,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, 'active', ?5, ?6, ?7)
            "#,
            params![
                payload.project_id.as_str(),
                agent_session_id.as_str(),
                payload.title.trim(),
                payload.summary.as_str(),
                if payload.selected { 1 } else { 0 },
                now.as_str(),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Cadence could not persist an agent session in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "agent_session_commit_failed",
            format!(
                "Cadence could not commit the agent-session transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    read_agent_session_row(
        &connection,
        &database_path,
        &payload.project_id,
        agent_session_id.as_str(),
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_session_missing_after_persist",
            format!(
                "Cadence persisted agent session `{agent_session_id}` in {} but could not read it back.",
                database_path.display()
            ),
        )
    })
}

pub fn get_agent_session(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Option<AgentSessionRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    read_agent_session_row(&connection, &database_path, project_id, agent_session_id)
}

pub fn list_agent_sessions(
    repo_root: &Path,
    project_id: &str,
    include_archived: bool,
) -> Result<Vec<AgentSessionRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                agent_session_id,
                title,
                summary,
                status,
                selected,
                created_at,
                updated_at,
                archived_at,
                last_run_id,
                last_runtime_kind,
                last_provider_id
            FROM agent_sessions
            WHERE project_id = ?1
              AND (?2 = 1 OR status = 'active')
            ORDER BY selected DESC, updated_at DESC, created_at DESC, agent_session_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_query_failed",
                format!(
                    "Cadence could not prepare the agent-session query against {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let rows = statement
        .query_map(params![project_id, if include_archived { 1 } else { 0 }], |row| {
            Ok(RawAgentSessionRow {
                project_id: row.get(0)?,
                agent_session_id: row.get(1)?,
                title: row.get(2)?,
                summary: row.get(3)?,
                status: row.get(4)?,
                selected: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                archived_at: row.get(8)?,
                last_run_id: row.get(9)?,
                last_runtime_kind: row.get(10)?,
                last_provider_id: row.get(11)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_query_failed",
                format!(
                    "Cadence could not query agent sessions from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut sessions = Vec::new();
    for row in rows {
        sessions.push(decode_agent_session_row(
            row.map_err(|error| {
                CommandError::system_fault(
                    "agent_session_query_failed",
                    format!(
                        "Cadence could not read an agent-session row from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            &database_path,
        )?);
    }
    Ok(sessions)
}

pub fn read_selected_agent_session(
    repo_root: &Path,
    project_id: &str,
) -> Result<Option<AgentSessionRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    read_selected_agent_session_row(&connection, &database_path, project_id)
}

pub fn update_agent_session(
    repo_root: &Path,
    payload: &AgentSessionUpdateRecord,
) -> Result<AgentSessionRecord, CommandError> {
    validate_non_empty_text(&payload.project_id, "projectId", "agent_session_request_invalid")?;
    validate_non_empty_text(
        &payload.agent_session_id,
        "agentSessionId",
        "agent_session_request_invalid",
    )?;
    if let Some(title) = payload.title.as_deref() {
        validate_non_empty_text(title, "title", "agent_session_request_invalid")?;
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &payload.project_id)?;

    let existing = read_agent_session_row(
        &connection,
        &database_path,
        &payload.project_id,
        &payload.agent_session_id,
    )?
    .ok_or_else(|| missing_agent_session_error(&payload.project_id, &payload.agent_session_id))?;

    if existing.status == AgentSessionStatus::Archived {
        return Err(CommandError::user_fixable(
            "agent_session_archived",
            format!(
                "Cadence cannot update archived agent session `{}` for project `{}`.",
                payload.agent_session_id, payload.project_id
            ),
        ));
    }

    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "agent_session_transaction_failed",
            format!(
                "Cadence could not start the agent-session update transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    if payload.selected == Some(true) {
        clear_selected_agent_session(&transaction, &database_path, &payload.project_id)?;
    }

    let now = now_timestamp();
    transaction
        .execute(
            r#"
            UPDATE agent_sessions
            SET title = COALESCE(?3, title),
                summary = COALESCE(?4, summary),
                selected = COALESCE(?5, selected),
                updated_at = ?6
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND status = 'active'
            "#,
            params![
                payload.project_id.as_str(),
                payload.agent_session_id.as_str(),
                payload.title.as_deref().map(str::trim),
                payload.summary.as_deref(),
                payload.selected.map(|selected| if selected { 1 } else { 0 }),
                now.as_str(),
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Cadence could not update agent session `{}` in {}: {error}",
                    payload.agent_session_id,
                    database_path.display()
                ),
            )
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "agent_session_commit_failed",
            format!(
                "Cadence could not commit the agent-session update transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    read_agent_session_row(
        &connection,
        &database_path,
        &payload.project_id,
        &payload.agent_session_id,
    )?
    .ok_or_else(|| missing_agent_session_error(&payload.project_id, &payload.agent_session_id))
}

pub fn archive_agent_session(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<AgentSessionRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId", "agent_session_request_invalid")?;
    validate_non_empty_text(
        agent_session_id,
        "agentSessionId",
        "agent_session_request_invalid",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let existing = read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
        .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))?;
    if existing.status == AgentSessionStatus::Archived {
        return Ok(existing);
    }

    let active_run = connection.query_row(
        r#"
        SELECT run_id, status
        FROM runtime_runs
        WHERE project_id = ?1
          AND agent_session_id = ?2
          AND status IN ('starting', 'running', 'stale')
        "#,
        params![project_id, agent_session_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
    );
    match active_run {
        Ok((run_id, status)) => {
            return Err(CommandError::user_fixable(
                "agent_session_active_run",
                format!(
                    "Cadence cannot archive agent session `{agent_session_id}` for project `{project_id}` while run `{run_id}` is {status}. Stop the run first."
                ),
            ))
        }
        Err(SqlError::QueryReturnedNoRows) => {}
        Err(error) => {
            return Err(CommandError::system_fault(
                "agent_session_query_failed",
                format!(
                    "Cadence could not inspect runtime runs for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            ))
        }
    }

    let now = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE agent_sessions
            SET status = 'archived',
                selected = 0,
                archived_at = ?3,
                updated_at = ?3
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND status = 'active'
            "#,
            params![project_id, agent_session_id, now.as_str()],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Cadence could not archive agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
        .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))
}

pub(crate) fn ensure_agent_session_active(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<AgentSessionRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let session = read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
        .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))?;
    if session.status != AgentSessionStatus::Active {
        return Err(CommandError::user_fixable(
            "agent_session_archived",
            format!(
                "Cadence cannot use archived agent session `{agent_session_id}` for project `{project_id}`."
            ),
        ));
    }
    Ok(session)
}

pub(crate) fn touch_agent_session_runtime_run(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
    runtime_kind: &str,
    provider_id: &str,
    updated_at: &str,
) -> Result<(), CommandError> {
    let affected = transaction
        .execute(
            r#"
            UPDATE agent_sessions
            SET last_run_id = ?3,
                last_runtime_kind = ?4,
                last_provider_id = ?5,
                updated_at = ?6
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND status = 'active'
            "#,
            params![
                project_id,
                agent_session_id,
                run_id,
                runtime_kind,
                provider_id,
                updated_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Cadence could not update agent-session runtime metadata in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    if affected == 0 {
        return Err(missing_agent_session_error(project_id, agent_session_id));
    }

    Ok(())
}

pub(crate) fn read_agent_session_row(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Option<AgentSessionRecord>, CommandError> {
    let row = connection.query_row(
        r#"
        SELECT
            project_id,
            agent_session_id,
            title,
            summary,
            status,
            selected,
            created_at,
            updated_at,
            archived_at,
            last_run_id,
            last_runtime_kind,
            last_provider_id
        FROM agent_sessions
        WHERE project_id = ?1
          AND agent_session_id = ?2
        "#,
        params![project_id, agent_session_id],
        |row| {
            Ok(RawAgentSessionRow {
                project_id: row.get(0)?,
                agent_session_id: row.get(1)?,
                title: row.get(2)?,
                summary: row.get(3)?,
                status: row.get(4)?,
                selected: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                archived_at: row.get(8)?,
                last_run_id: row.get(9)?,
                last_runtime_kind: row.get(10)?,
                last_provider_id: row.get(11)?,
            })
        },
    );

    match row {
        Ok(row) => decode_agent_session_row(row, database_path).map(Some),
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(CommandError::system_fault(
            "agent_session_query_failed",
            format!(
                "Cadence could not read agent session `{agent_session_id}` from {}: {error}",
                database_path.display()
            ),
        )),
    }
}

pub(crate) fn read_selected_agent_session_row(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
) -> Result<Option<AgentSessionRecord>, CommandError> {
    let row = connection.query_row(
        r#"
        SELECT
            project_id,
            agent_session_id,
            title,
            summary,
            status,
            selected,
            created_at,
            updated_at,
            archived_at,
            last_run_id,
            last_runtime_kind,
            last_provider_id
        FROM agent_sessions
        WHERE project_id = ?1
          AND selected = 1
          AND status = 'active'
        "#,
        params![project_id],
        |row| {
            Ok(RawAgentSessionRow {
                project_id: row.get(0)?,
                agent_session_id: row.get(1)?,
                title: row.get(2)?,
                summary: row.get(3)?,
                status: row.get(4)?,
                selected: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
                archived_at: row.get(8)?,
                last_run_id: row.get(9)?,
                last_runtime_kind: row.get(10)?,
                last_provider_id: row.get(11)?,
            })
        },
    );

    match row {
        Ok(row) => decode_agent_session_row(row, database_path).map(Some),
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(CommandError::system_fault(
            "agent_session_query_failed",
            format!(
                "Cadence could not read the selected agent session from {}: {error}",
                database_path.display()
            ),
        )),
    }
}

fn clear_selected_agent_session(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
) -> Result<(), CommandError> {
    transaction
        .execute(
            "UPDATE agent_sessions SET selected = 0 WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Cadence could not clear the selected agent session in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn decode_agent_session_row(
    row: RawAgentSessionRow,
    database_path: &Path,
) -> Result<AgentSessionRecord, CommandError> {
    let status = parse_agent_session_status(&row.status).map_err(|details| {
        CommandError::system_fault(
            "agent_session_decode_failed",
            format!(
                "Cadence found malformed agent-session metadata in {}: {details}",
                database_path.display()
            ),
        )
    })?;
    let selected = match row.selected {
        0 => false,
        1 => true,
        other => {
            return Err(CommandError::system_fault(
                "agent_session_decode_failed",
                format!(
                    "Cadence found malformed agent-session selected flag `{other}` in {}.",
                    database_path.display()
                ),
            ))
        }
    };

    let archived_at =
        decode_optional_non_empty_text(row.archived_at, "archived_at", database_path, "agent_session_decode_failed")?;
    if matches!(status, AgentSessionStatus::Archived) && archived_at.is_none() {
        return Err(CommandError::system_fault(
            "agent_session_decode_failed",
            format!(
                "Cadence found archived agent-session metadata without archived_at in {}.",
                database_path.display()
            ),
        ));
    }
    if matches!(status, AgentSessionStatus::Active) && archived_at.is_some() {
        return Err(CommandError::system_fault(
            "agent_session_decode_failed",
            format!(
                "Cadence found active agent-session metadata with archived_at in {}.",
                database_path.display()
            ),
        ));
    }

    Ok(AgentSessionRecord {
        project_id: require_non_empty_owned(
            row.project_id,
            "project_id",
            database_path,
            "agent_session_decode_failed",
        )?,
        agent_session_id: require_non_empty_owned(
            row.agent_session_id,
            "agent_session_id",
            database_path,
            "agent_session_decode_failed",
        )?,
        title: require_non_empty_owned(
            row.title,
            "title",
            database_path,
            "agent_session_decode_failed",
        )?,
        summary: row.summary,
        status,
        selected,
        created_at: require_non_empty_owned(
            row.created_at,
            "created_at",
            database_path,
            "agent_session_decode_failed",
        )?,
        updated_at: require_non_empty_owned(
            row.updated_at,
            "updated_at",
            database_path,
            "agent_session_decode_failed",
        )?,
        archived_at,
        last_run_id: decode_optional_non_empty_text(
            row.last_run_id,
            "last_run_id",
            database_path,
            "agent_session_decode_failed",
        )?,
        last_runtime_kind: decode_optional_non_empty_text(
            row.last_runtime_kind,
            "last_runtime_kind",
            database_path,
            "agent_session_decode_failed",
        )?,
        last_provider_id: decode_optional_non_empty_text(
            row.last_provider_id,
            "last_provider_id",
            database_path,
            "agent_session_decode_failed",
        )?,
    })
}

fn parse_agent_session_status(value: &str) -> Result<AgentSessionStatus, String> {
    match value {
        "active" => Ok(AgentSessionStatus::Active),
        "archived" => Ok(AgentSessionStatus::Archived),
        other => Err(format!(
            "Field `status` must be a known agent-session status, found `{other}`."
        )),
    }
}

fn missing_agent_session_error(project_id: &str, agent_session_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_session_missing",
        format!(
            "Cadence could not find active agent session `{agent_session_id}` for project `{project_id}`."
        ),
    )
}
