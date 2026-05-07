use std::path::Path;

use rand::RngCore;
use rusqlite::{params, Connection, Error as SqlError, Transaction};

use crate::{auth::now_timestamp, commands::CommandError, db::database_path_for_repo};

use super::{
    agent_lineage::{read_agent_session_lineage_for_child, AgentSessionLineageRecord},
    clear_memory_runs_for_deletion, decode_optional_non_empty_text, open_runtime_database,
    read_project_row, require_non_empty_owned, validate_non_empty_text,
};

pub const DEFAULT_AGENT_SESSION_ID: &str = "agent-session-main";
pub const DEFAULT_AGENT_SESSION_TITLE: &str = "New Chat";

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
    pub lineage: Option<AgentSessionLineageRecord>,
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
    validate_non_empty_text(
        &payload.project_id,
        "projectId",
        "agent_session_request_invalid",
    )?;
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
                "Xero could not start the agent-session transaction in {}: {error}",
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
                    "Xero could not persist an agent session in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "agent_session_commit_failed",
            format!(
                "Xero could not commit the agent-session transaction in {}: {error}",
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
                "Xero persisted agent session `{agent_session_id}` in {} but could not read it back.",
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
            ORDER BY updated_at DESC, created_at DESC, agent_session_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_query_failed",
                format!(
                    "Xero could not prepare the agent-session query against {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let rows = statement
        .query_map(
            params![project_id, if include_archived { 1 } else { 0 }],
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
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_query_failed",
                format!(
                    "Xero could not query agent sessions from {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let mut sessions = Vec::new();
    for row in rows {
        let mut session = decode_agent_session_row(
            row.map_err(|error| {
                CommandError::system_fault(
                    "agent_session_query_failed",
                    format!(
                        "Xero could not read an agent-session row from {}: {error}",
                        database_path.display()
                    ),
                )
            })?,
            &database_path,
        )?;
        session.lineage = read_agent_session_lineage_for_child(
            &connection,
            &database_path,
            &session.project_id,
            &session.agent_session_id,
        )?;
        sessions.push(session);
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
    validate_non_empty_text(
        &payload.project_id,
        "projectId",
        "agent_session_request_invalid",
    )?;
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
                "Xero cannot update archived agent session `{}` for project `{}`.",
                payload.agent_session_id, payload.project_id
            ),
        ));
    }

    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "agent_session_transaction_failed",
            format!(
                "Xero could not start the agent-session update transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    if payload.selected == Some(true) {
        clear_selected_agent_session(&transaction, &database_path, &payload.project_id)?;
    }

    let updated_at = if payload.title.is_some() || payload.summary.is_some() {
        now_timestamp()
    } else {
        existing.updated_at.clone()
    };
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
                payload
                    .selected
                    .map(|selected| if selected { 1 } else { 0 }),
                updated_at.as_str(),
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not update agent session `{}` in {}: {error}",
                    payload.agent_session_id,
                    database_path.display()
                ),
            )
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "agent_session_commit_failed",
            format!(
                "Xero could not commit the agent-session update transaction in {}: {error}",
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

    let existing =
        read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
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
                    "Xero cannot archive agent session `{agent_session_id}` for project `{project_id}` while run `{run_id}` is {status}. Stop the run first."
                ),
            ))
        }
        Err(SqlError::QueryReturnedNoRows) => {}
        Err(error) => {
            return Err(CommandError::system_fault(
                "agent_session_query_failed",
                format!(
                    "Xero could not inspect runtime runs for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            ))
        }
    }

    let now = now_timestamp();
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "agent_session_transaction_failed",
            format!(
                "Xero could not start the agent-session archive transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    transaction
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
                    "Xero could not archive agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    ensure_active_agent_session_after_removal(
        &transaction,
        &database_path,
        project_id,
        now.as_str(),
    )?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "agent_session_commit_failed",
            format!(
                "Xero could not commit the agent-session archive transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
        .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))
}

pub fn restore_agent_session(
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

    let existing =
        read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
            .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))?;
    if existing.status == AgentSessionStatus::Active {
        return Ok(existing);
    }

    let now = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE agent_sessions
            SET status = 'active',
                archived_at = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND status = 'archived'
            "#,
            params![project_id, agent_session_id, now.as_str()],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not restore agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
        .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))
}

pub fn delete_agent_session(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId", "agent_session_request_invalid")?;
    validate_non_empty_text(
        agent_session_id,
        "agentSessionId",
        "agent_session_request_invalid",
    )?;

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let existing =
        read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
            .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))?;
    if existing.status != AgentSessionStatus::Archived {
        return Err(CommandError::user_fixable(
            "agent_session_not_archived",
            format!(
                "Xero cannot permanently delete agent session `{agent_session_id}` for project `{project_id}` because it is not archived. Archive it first."
            ),
        ));
    }

    // Snapshot the run_ids that the relational cascade is about to delete so
    // we can clear matching `source_run_id` references in the Lance dataset.
    let cascade_run_ids =
        read_run_ids_for_session(&connection, &database_path, project_id, agent_session_id)?;

    let now = now_timestamp();
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "agent_session_transaction_failed",
            format!(
                "Xero could not start the agent-session delete transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    prepare_code_history_for_agent_session_delete(
        &transaction,
        &database_path,
        project_id,
        agent_session_id,
        now.as_str(),
    )?;

    transaction
        .execute(
            r#"
            DELETE FROM agent_sessions
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND status = 'archived'
            "#,
            params![project_id, agent_session_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not delete agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    ensure_active_agent_session_after_removal(
        &transaction,
        &database_path,
        project_id,
        now.as_str(),
    )?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "agent_session_commit_failed",
            format!(
                "Xero could not commit the agent-session delete transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    drop(connection);
    if !cascade_run_ids.is_empty() {
        clear_memory_runs_for_deletion(repo_root, project_id, &cascade_run_ids)?;
    }

    Ok(())
}

fn prepare_code_history_for_agent_session_delete(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
    updated_at: &str,
) -> Result<(), CommandError> {
    transaction
        .execute(
            r#"
            UPDATE code_commits
            SET parent_commit_id = NULL
            WHERE project_id = ?1
              AND parent_commit_id IN (
                SELECT commit_id
                FROM code_commits
                WHERE project_id = ?1
                  AND agent_session_id = ?2
              )
            "#,
            params![project_id, agent_session_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not detach code commit parent links for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction
        .execute(
            r#"
            UPDATE code_workspace_heads
            SET head_id = NULL,
                tree_id = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND head_id IN (
                SELECT commit_id
                FROM code_commits
                WHERE project_id = ?1
                  AND agent_session_id = ?2
              )
            "#,
            params![project_id, agent_session_id, updated_at],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not clear code workspace head for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction
        .execute(
            r#"
            UPDATE code_path_epochs
            SET commit_id = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND commit_id IN (
                SELECT commit_id
                FROM code_commits
                WHERE project_id = ?1
                  AND agent_session_id = ?2
              )
            "#,
            params![project_id, agent_session_id, updated_at],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not clear code path epochs for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction
        .execute(
            r#"
            UPDATE code_history_operations
            SET result_commit_id = NULL,
                updated_at = ?3
            WHERE project_id = ?1
              AND result_commit_id IN (
                SELECT commit_id
                FROM code_commits
                WHERE project_id = ?1
                  AND agent_session_id = ?2
              )
            "#,
            params![project_id, agent_session_id, updated_at],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not clear code history commit references for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction
        .execute(
            r#"
            UPDATE code_history_operations
            SET target_change_group_id = CASE
                    WHEN target_change_group_id IN (
                        SELECT change_group_id
                        FROM code_change_groups
                        WHERE project_id = ?1
                          AND agent_session_id = ?2
                    )
                    THEN NULL
                    ELSE target_change_group_id
                END,
                result_change_group_id = CASE
                    WHEN result_change_group_id IN (
                        SELECT change_group_id
                        FROM code_change_groups
                        WHERE project_id = ?1
                          AND agent_session_id = ?2
                    )
                    THEN NULL
                    ELSE result_change_group_id
                END,
                updated_at = ?3
            WHERE project_id = ?1
              AND (
                target_change_group_id IN (
                    SELECT change_group_id
                    FROM code_change_groups
                    WHERE project_id = ?1
                      AND agent_session_id = ?2
                )
                OR result_change_group_id IN (
                    SELECT change_group_id
                    FROM code_change_groups
                    WHERE project_id = ?1
                      AND agent_session_id = ?2
                )
              )
            "#,
            params![project_id, agent_session_id, updated_at],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not clear code history change-group references for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    transaction
        .execute(
            r#"
            DELETE FROM code_commits
            WHERE project_id = ?1
              AND agent_session_id = ?2
            "#,
            params![project_id, agent_session_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not delete code commits for agent session `{agent_session_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    Ok(())
}

fn ensure_active_agent_session_after_removal(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    timestamp: &str,
) -> Result<(), CommandError> {
    let active_count: i64 = transaction
        .query_row(
            "SELECT COUNT(*) FROM agent_sessions WHERE project_id = ?1 AND status = 'active'",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_query_failed",
                format!(
                    "Xero could not count active agent sessions in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    if active_count > 0 {
        return Ok(());
    }

    clear_selected_agent_session(transaction, database_path, project_id)?;

    let agent_session_id = generate_agent_session_id();
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
            VALUES (?1, ?2, ?3, '', 'active', 1, ?4, ?4)
            "#,
            params![
                project_id,
                agent_session_id.as_str(),
                DEFAULT_AGENT_SESSION_TITLE,
                timestamp,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not create a replacement agent session in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    Ok(())
}

fn read_run_ids_for_session(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<Vec<String>, CommandError> {
    let mut statement = connection
        .prepare("SELECT run_id FROM agent_runs WHERE project_id = ?1 AND agent_session_id = ?2")
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not enumerate agent runs for cascade clearing in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let rows = statement
        .query_map(params![project_id, agent_session_id], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!(
                    "Xero could not read agent run cascade list in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CommandError::system_fault(
            "agent_session_persist_failed",
            format!(
                "Xero could not collect agent run cascade list in {}: {error}",
                database_path.display()
            ),
        )
    })
}

pub(crate) fn ensure_agent_session_active(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> Result<AgentSessionRecord, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let session =
        read_agent_session_row(&connection, &database_path, project_id, agent_session_id)?
            .ok_or_else(|| missing_agent_session_error(project_id, agent_session_id))?;
    if session.status != AgentSessionStatus::Active {
        return Err(CommandError::user_fixable(
            "agent_session_archived",
            format!(
                "Xero cannot use archived agent session `{agent_session_id}` for project `{project_id}`."
            ),
        ));
    }
    Ok(session)
}

#[allow(clippy::too_many_arguments)]
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
                    "Xero could not update agent-session runtime metadata in {}: {error}",
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
        Ok(row) => {
            let mut session = decode_agent_session_row(row, database_path)?;
            session.lineage = read_agent_session_lineage_for_child(
                connection,
                database_path,
                &session.project_id,
                &session.agent_session_id,
            )?;
            Ok(Some(session))
        }
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(CommandError::system_fault(
            "agent_session_query_failed",
            format!(
                "Xero could not read agent session `{agent_session_id}` from {}: {error}",
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
        Ok(row) => {
            let mut session = decode_agent_session_row(row, database_path)?;
            session.lineage = read_agent_session_lineage_for_child(
                connection,
                database_path,
                &session.project_id,
                &session.agent_session_id,
            )?;
            Ok(Some(session))
        }
        Err(SqlError::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(CommandError::system_fault(
            "agent_session_query_failed",
            format!(
                "Xero could not read the selected agent session from {}: {error}",
                database_path.display()
            ),
        )),
    }
}

pub(crate) fn clear_selected_agent_session(
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
                    "Xero could not clear the selected agent session in {}: {error}",
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
                "Xero found malformed agent-session metadata in {}: {details}",
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
                    "Xero found malformed agent-session selected flag `{other}` in {}.",
                    database_path.display()
                ),
            ))
        }
    };

    let archived_at = decode_optional_non_empty_text(
        row.archived_at,
        "archived_at",
        database_path,
        "agent_session_decode_failed",
    )?;
    if matches!(status, AgentSessionStatus::Archived) && archived_at.is_none() {
        return Err(CommandError::system_fault(
            "agent_session_decode_failed",
            format!(
                "Xero found archived agent-session metadata without archived_at in {}.",
                database_path.display()
            ),
        ));
    }
    if matches!(status, AgentSessionStatus::Active) && archived_at.is_some() {
        return Err(CommandError::system_fault(
            "agent_session_decode_failed",
            format!(
                "Xero found active agent-session metadata with archived_at in {}.",
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
        lineage: None,
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
            "Xero could not find active agent session `{agent_session_id}` for project `{project_id}`."
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{fs, path::PathBuf};

    use crate::{
        db::import_project, git::repository::CanonicalRepository, state::ImportFailpoints,
    };
    use rusqlite::{params, Connection};

    struct TestProject {
        _repo_dir: tempfile::TempDir,
        project_id: String,
        repo_root: PathBuf,
        database_path: PathBuf,
    }

    impl Drop for TestProject {
        fn drop(&mut self) {
            if let Some(project_dir) = self.database_path.parent() {
                let _ = fs::remove_dir_all(project_dir);
            }
        }
    }

    fn import_test_project() -> TestProject {
        let repo_dir = tempfile::tempdir().expect("temp repo");
        let repo_root = repo_dir.path().to_path_buf();
        let project_id = format!("project-{}", generate_agent_session_id());
        let root_path_string = repo_root.to_string_lossy().into_owned();
        let repository = CanonicalRepository {
            project_id: project_id.clone(),
            repository_id: format!("repo-{project_id}"),
            root_path: repo_root.clone(),
            root_path_string,
            common_git_dir: repo_root.join(".git"),
            display_name: "Session Order Test".into(),
            branch_name: Some("main".into()),
            head_sha: None,
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        };
        let imported =
            import_project(&repository, &ImportFailpoints::default()).expect("import test project");

        TestProject {
            _repo_dir: repo_dir,
            project_id,
            repo_root,
            database_path: imported.database_path,
        }
    }

    fn set_session_time(
        database_path: &Path,
        project_id: &str,
        agent_session_id: &str,
        timestamp: &str,
    ) {
        let connection = Connection::open(database_path).expect("open project database");
        connection
            .execute(
                r#"
                UPDATE agent_sessions
                SET created_at = ?3,
                    updated_at = ?3
                WHERE project_id = ?1
                  AND agent_session_id = ?2
                "#,
                params![project_id, agent_session_id, timestamp],
            )
            .expect("set session timestamp");
    }

    fn force_archive_session(
        database_path: &Path,
        project_id: &str,
        agent_session_id: &str,
        timestamp: &str,
    ) {
        let connection = Connection::open(database_path).expect("open project database");
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
                "#,
                params![project_id, agent_session_id, timestamp],
            )
            .expect("force archive session");
    }

    fn session_ids(sessions: &[AgentSessionRecord]) -> Vec<String> {
        sessions
            .iter()
            .map(|session| session.agent_session_id.clone())
            .collect()
    }

    #[test]
    fn archiving_last_active_session_creates_selected_replacement() {
        let project = import_test_project();

        let archived = archive_agent_session(
            &project.repo_root,
            &project.project_id,
            DEFAULT_AGENT_SESSION_ID,
        )
        .expect("archive default session");

        assert_eq!(archived.status, AgentSessionStatus::Archived);
        assert!(!archived.selected);

        let active = list_agent_sessions(&project.repo_root, &project.project_id, false)
            .expect("list active sessions");
        assert_eq!(active.len(), 1);
        let replacement = &active[0];
        assert_ne!(replacement.agent_session_id, DEFAULT_AGENT_SESSION_ID);
        assert_eq!(replacement.title, DEFAULT_AGENT_SESSION_TITLE);
        assert_eq!(replacement.status, AgentSessionStatus::Active);
        assert!(replacement.selected);
        assert!(replacement.archived_at.is_none());

        let all = list_agent_sessions(&project.repo_root, &project.project_id, true)
            .expect("list all sessions");
        assert_eq!(
            all.iter()
                .filter(|session| session.status == AgentSessionStatus::Active)
                .count(),
            1,
        );
        assert!(all.iter().any(|session| {
            session.agent_session_id == DEFAULT_AGENT_SESSION_ID
                && session.status == AgentSessionStatus::Archived
        }));
    }

    #[test]
    fn deleting_last_session_creates_selected_replacement() {
        let project = import_test_project();
        force_archive_session(
            &project.database_path,
            &project.project_id,
            DEFAULT_AGENT_SESSION_ID,
            "2026-04-15T21:00:00Z",
        );

        delete_agent_session(
            &project.repo_root,
            &project.project_id,
            DEFAULT_AGENT_SESSION_ID,
        )
        .expect("delete archived default session");

        let active = list_agent_sessions(&project.repo_root, &project.project_id, false)
            .expect("list active sessions");
        assert_eq!(active.len(), 1);
        let replacement = &active[0];
        assert_ne!(replacement.agent_session_id, DEFAULT_AGENT_SESSION_ID);
        assert_eq!(replacement.title, DEFAULT_AGENT_SESSION_TITLE);
        assert_eq!(replacement.status, AgentSessionStatus::Active);
        assert!(replacement.selected);

        let deleted = get_agent_session(
            &project.repo_root,
            &project.project_id,
            DEFAULT_AGENT_SESSION_ID,
        )
        .expect("read deleted session");
        assert!(deleted.is_none());
    }

    #[test]
    fn selecting_session_preserves_recency_order_and_timestamp() {
        let project = import_test_project();
        let middle = create_agent_session(
            &project.repo_root,
            &AgentSessionCreateRecord {
                project_id: project.project_id.clone(),
                title: "Middle".into(),
                summary: String::new(),
                selected: false,
            },
        )
        .expect("create middle session");
        let top = create_agent_session(
            &project.repo_root,
            &AgentSessionCreateRecord {
                project_id: project.project_id.clone(),
                title: "Top".into(),
                summary: String::new(),
                selected: false,
            },
        )
        .expect("create top session");

        set_session_time(
            &project.database_path,
            &project.project_id,
            DEFAULT_AGENT_SESSION_ID,
            "2026-04-15T20:00:00Z",
        );
        set_session_time(
            &project.database_path,
            &project.project_id,
            &middle.agent_session_id,
            "2026-04-15T20:01:00Z",
        );
        set_session_time(
            &project.database_path,
            &project.project_id,
            &top.agent_session_id,
            "2026-04-15T20:02:00Z",
        );

        let expected_order = vec![
            top.agent_session_id.clone(),
            middle.agent_session_id.clone(),
            DEFAULT_AGENT_SESSION_ID.into(),
        ];
        let before = list_agent_sessions(&project.repo_root, &project.project_id, false)
            .expect("list sessions before selection");
        assert_eq!(session_ids(&before), expected_order);

        let middle_before = get_agent_session(
            &project.repo_root,
            &project.project_id,
            &middle.agent_session_id,
        )
        .expect("read middle session")
        .expect("middle session exists");

        let selected_middle = update_agent_session(
            &project.repo_root,
            &AgentSessionUpdateRecord {
                project_id: project.project_id.clone(),
                agent_session_id: middle.agent_session_id.clone(),
                title: None,
                summary: None,
                selected: Some(true),
            },
        )
        .expect("select middle session");

        assert!(selected_middle.selected);
        assert_eq!(selected_middle.updated_at, middle_before.updated_at);

        let after = list_agent_sessions(&project.repo_root, &project.project_id, false)
            .expect("list sessions after selection");
        assert_eq!(session_ids(&after), expected_order);
        assert_eq!(
            after
                .iter()
                .filter(|session| session.selected)
                .map(|session| session.agent_session_id.as_str())
                .collect::<Vec<_>>(),
            vec![middle.agent_session_id.as_str()],
        );
    }
}
