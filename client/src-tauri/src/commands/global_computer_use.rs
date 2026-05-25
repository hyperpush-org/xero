use std::{fs, path::PathBuf};

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{agent_session::agent_session_dto, AgentSessionDto, CommandError, CommandResult},
    db::{
        self,
        migrations::{migrations, PROJECT_DATABASE_SCHEMA_VERSION},
        project_store::{self, AgentSessionRecord, COMPUTER_USE_AGENT_SESSION_TITLE},
    },
    state::DesktopState,
};

pub const GLOBAL_COMPUTER_USE_PROJECT_ID: &str = "global-computer-use";
pub const GLOBAL_COMPUTER_USE_PROJECT_NAME: &str = "Computer Use";
pub const GLOBAL_COMPUTER_USE_AGENT_SESSION_ID: &str = "agent-session-global-computer-use";
pub const REMOTE_COMPUTER_USE_SESSION_ID: &str = "__computer_use__";

const GLOBAL_COMPUTER_USE_DIR: &str = "computer-use";
const GLOBAL_COMPUTER_USE_STATE_DB: &str = "state.db";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GlobalComputerUseSessionDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub session: AgentSessionDto,
}

#[derive(Debug, Clone)]
pub(crate) struct GlobalComputerUseSessionRecord {
    pub project_id: String,
    pub repo_root: PathBuf,
    pub session: AgentSessionRecord,
}

#[tauri::command]
pub fn ensure_global_computer_use_session<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<GlobalComputerUseSessionDto> {
    let record = ensure_global_computer_use_session_record(&app, state.inner())?;
    Ok(GlobalComputerUseSessionDto {
        project_id: record.project_id.clone(),
        agent_session_id: record.session.agent_session_id.clone(),
        session: agent_session_dto(&record.session),
    })
}

pub(crate) fn ensure_global_computer_use_session_record<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<GlobalComputerUseSessionRecord> {
    let (repo_root, database_path) = global_computer_use_paths(app, state)?;
    fs::create_dir_all(&repo_root).map_err(|error| {
        CommandError::retryable(
            "computer_use_state_dir_unavailable",
            format!(
                "Xero could not prepare the Computer Use app-data directory at {}: {error}",
                repo_root.display()
            ),
        )
    })?;
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "computer_use_state_dir_unavailable",
                format!(
                    "Xero could not prepare the Computer Use state directory at {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }

    db::register_project_database_path(&repo_root, &database_path);
    ensure_global_computer_use_database(&repo_root, &database_path)?;
    let session = project_store::get_agent_session(
        &repo_root,
        GLOBAL_COMPUTER_USE_PROJECT_ID,
        GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "computer_use_session_missing_after_prepare",
            "Xero prepared Computer Use state but could not read the backing session.",
        )
    })?;

    Ok(GlobalComputerUseSessionRecord {
        project_id: GLOBAL_COMPUTER_USE_PROJECT_ID.into(),
        repo_root,
        session,
    })
}

pub(crate) fn global_computer_use_project_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<PathBuf> {
    let record = ensure_global_computer_use_session_record(app, state)?;
    Ok(record.repo_root)
}

fn global_computer_use_paths<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
) -> CommandResult<(PathBuf, PathBuf)> {
    let repo_root = state.app_data_dir(app)?.join(GLOBAL_COMPUTER_USE_DIR);
    let database_path = repo_root.join(GLOBAL_COMPUTER_USE_STATE_DB);
    Ok((repo_root, database_path))
}

fn ensure_global_computer_use_database(
    repo_root: &PathBuf,
    database_path: &PathBuf,
) -> CommandResult<()> {
    let database_existed = database_path.exists();
    let mut connection = open_global_computer_use_database(database_path)?;
    let observed_user_version = db::read_user_version(&connection);
    if database_existed && observed_user_version != PROJECT_DATABASE_SCHEMA_VERSION {
        drop(connection);
        remove_database_sidecars(database_path);
        fs::remove_file(database_path).map_err(|error| {
            CommandError::retryable(
                "computer_use_state_reset_failed",
                format!(
                    "Xero found incompatible Computer Use state at {} but could not reset it: {error}",
                    database_path.display()
                ),
            )
        })?;
        connection = open_global_computer_use_database(database_path)?;
    }

    migrations().to_latest(&mut connection).map_err(|error| {
        CommandError::retryable(
            "computer_use_state_migration_failed",
            format!(
                "Xero could not initialize Computer Use state at {}: {error}",
                database_path.display()
            ),
        )
    })?;
    upsert_global_computer_use_rows(&connection, repo_root, database_path)
}

fn open_global_computer_use_database(database_path: &PathBuf) -> CommandResult<Connection> {
    let connection = Connection::open(database_path).map_err(|error| {
        CommandError::retryable(
            "computer_use_state_open_failed",
            format!(
                "Xero could not open Computer Use state at {}: {error}",
                database_path.display()
            ),
        )
    })?;
    db::configure_connection(&connection)?;
    Ok(connection)
}

fn upsert_global_computer_use_rows(
    connection: &Connection,
    repo_root: &PathBuf,
    database_path: &PathBuf,
) -> CommandResult<()> {
    let now = now_timestamp();
    let tx = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "computer_use_state_transaction_failed",
            format!(
                "Xero could not start the Computer Use state transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    tx.execute(
        r#"
        INSERT INTO projects (
            id,
            name,
            description,
            milestone,
            total_phases,
            completed_phases,
            active_phase,
            branch,
            runtime,
            updated_at
        )
        VALUES (?1, ?2, '', '', 0, 0, 0, NULL, NULL, ?3)
        ON CONFLICT(id) DO UPDATE SET
            name = excluded.name,
            updated_at = excluded.updated_at
        "#,
        params![
            GLOBAL_COMPUTER_USE_PROJECT_ID,
            GLOBAL_COMPUTER_USE_PROJECT_NAME,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        CommandError::system_fault(
            "computer_use_project_persist_failed",
            format!(
                "Xero could not persist the global Computer Use project row in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    tx.execute(
        r#"
        INSERT INTO repositories (
            id,
            project_id,
            root_path,
            display_name,
            branch,
            head_sha,
            is_git_repo,
            updated_at
        )
        VALUES ('global-computer-use-repository', ?1, ?2, ?3, NULL, NULL, 0, ?4)
        ON CONFLICT(id) DO UPDATE SET
            project_id = excluded.project_id,
            root_path = excluded.root_path,
            display_name = excluded.display_name,
            is_git_repo = excluded.is_git_repo,
            updated_at = excluded.updated_at
        "#,
        params![
            GLOBAL_COMPUTER_USE_PROJECT_ID,
            repo_root.to_string_lossy().as_ref(),
            GLOBAL_COMPUTER_USE_PROJECT_NAME,
            now.as_str(),
        ],
    )
    .map_err(|error| {
        CommandError::system_fault(
            "computer_use_repository_persist_failed",
            format!(
                "Xero could not persist the global Computer Use repository row in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    tx.execute(
        r#"
        INSERT INTO agent_sessions (
            project_id,
            agent_session_id,
            session_kind,
            title,
            summary,
            status,
            selected,
            remote_visible,
            created_at,
            updated_at
        )
        VALUES (?1, ?2, 'computer_use', ?3, '', 'active', 1, 0, ?4, ?5)
        ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
            session_kind = 'computer_use',
            title = CASE
                WHEN trim(agent_sessions.title) = '' THEN excluded.title
                ELSE agent_sessions.title
            END,
            status = 'active',
            selected = 1,
            remote_visible = 0,
            archived_at = NULL,
            updated_at = agent_sessions.updated_at
        "#,
        params![
            GLOBAL_COMPUTER_USE_PROJECT_ID,
            GLOBAL_COMPUTER_USE_AGENT_SESSION_ID,
            COMPUTER_USE_AGENT_SESSION_TITLE,
            now.as_str(),
            now.as_str(),
        ],
    )
    .map_err(|error| {
        CommandError::system_fault(
            "computer_use_session_persist_failed",
            format!(
                "Xero could not persist the global Computer Use session row in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    tx.commit().map_err(|error| {
        CommandError::system_fault(
            "computer_use_state_commit_failed",
            format!(
                "Xero could not commit Computer Use state in {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn remove_database_sidecars(database_path: &PathBuf) {
    let wal_path = database_path.with_extension("db-wal");
    let shm_path = database_path.with_extension("db-shm");
    let _ = fs::remove_file(wal_path);
    let _ = fs::remove_file(shm_path);
}
