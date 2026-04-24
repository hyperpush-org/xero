use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, Connection};
use rusqlite_migration::{Error as MigrationError, MigrationDefinitionError};

use crate::{
    commands::{CommandError, ProjectSummaryDto, RepositorySummaryDto},
    db::migrations::migrations,
    git::repository::CanonicalRepository,
    state::ImportFailpoints,
};

pub mod migrations;
pub mod project_store;

const CADENCE_DIRECTORY: &str = ".cadence";
const STATE_DATABASE_FILE: &str = "state.db";

#[derive(Debug, Clone)]
pub struct ImportedProjectRecord {
    pub project: ProjectSummaryDto,
    pub repository: RepositorySummaryDto,
    pub database_path: PathBuf,
}

pub fn database_path_for_repo(repo_root: &Path) -> PathBuf {
    repo_root.join(CADENCE_DIRECTORY).join(STATE_DATABASE_FILE)
}

pub fn import_project(
    repository: &CanonicalRepository,
    failpoints: &ImportFailpoints,
) -> Result<ImportedProjectRecord, CommandError> {
    let cadence_directory = repository.root_path.join(CADENCE_DIRECTORY);
    let database_path = cadence_directory.join(STATE_DATABASE_FILE);
    let cadence_directory_existed = cadence_directory.exists();
    let database_existed = database_path.exists();

    fs::create_dir_all(&cadence_directory).map_err(|error| {
        CommandError::retryable(
            "cadence_state_dir_unavailable",
            format!(
                "Cadence could not prepare the repo-local state directory at {}: {error}",
                cadence_directory.display()
            ),
        )
    })?;

    let import_result = (|| -> Result<ImportedProjectRecord, CommandError> {
        let mut connection = open_database_connection(&database_path)?;
        configure_connection(&connection)?;

        if failpoints.fail_migration {
            return Err(CommandError::system_fault(
                "state_database_migration_failed",
                "Test failpoint forced the repo-local migration to fail before any rows were written.",
            ));
        }

        let connection = match migrations().to_latest(&mut connection) {
            Ok(()) => connection,
            Err(error) if database_existed && is_database_too_far_ahead(&error) => {
                let observed_user_version = read_user_version(&connection);
                let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
                drop(connection);

                quarantine_incompatible_database(&database_path, observed_user_version)?;

                let mut reset_connection = open_database_connection(&database_path)?;
                configure_connection(&reset_connection)?;
                migrations()
                    .to_latest(&mut reset_connection)
                    .map_err(|error| state_database_migration_error(&database_path, error))?;
                reset_connection
            }
            Err(error) => return Err(state_database_migration_error(&database_path, error)),
        };

        persist_import_rows(&connection, repository)?;

        Ok(ImportedProjectRecord {
            project: ProjectSummaryDto {
                id: repository.project_id.clone(),
                name: repository.display_name.clone(),
                description: String::new(),
                milestone: String::new(),
                total_phases: 0,
                completed_phases: 0,
                active_phase: 0,
                branch: repository.branch_name.clone(),
                runtime: None,
            },
            repository: RepositorySummaryDto {
                id: repository.repository_id.clone(),
                project_id: repository.project_id.clone(),
                root_path: repository.root_path_string.clone(),
                display_name: repository.display_name.clone(),
                branch: repository.branch_name.clone(),
                head_sha: repository.head_sha.clone(),
                is_git_repo: true,
            },
            database_path: database_path.clone(),
        })
    })();

    if import_result.is_err() {
        cleanup_partial_state(
            &cadence_directory,
            &database_path,
            cadence_directory_existed,
            database_existed,
        );
    }

    import_result
}

pub(crate) fn configure_connection(connection: &Connection) -> Result<(), CommandError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| {
            CommandError::system_fault(
                "state_database_configuration_failed",
                format!("Cadence could not configure SQLite busy timeout: {error}"),
            )
        })?;

    connection
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|error| {
            CommandError::system_fault(
                "state_database_configuration_failed",
                format!("Cadence could not configure SQLite pragmas: {error}"),
            )
        })
}

fn open_database_connection(database_path: &Path) -> Result<Connection, CommandError> {
    Connection::open(database_path).map_err(|error| {
        CommandError::retryable(
            "state_database_open_failed",
            format!(
                "Cadence could not open the repo-local database at {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn state_database_migration_error(database_path: &Path, error: MigrationError) -> CommandError {
    CommandError::system_fault(
        "state_database_migration_failed",
        format!(
            "Cadence could not migrate the repo-local database at {}: {error}",
            database_path.display()
        ),
    )
}

fn is_database_too_far_ahead(error: &MigrationError) -> bool {
    matches!(
        error,
        MigrationError::MigrationDefinition(MigrationDefinitionError::DatabaseTooFarAhead)
    )
}

fn read_user_version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap_or(0)
}

fn quarantine_incompatible_database(
    database_path: &Path,
    observed_user_version: i64,
) -> Result<(), CommandError> {
    let backup_path = next_incompatible_backup_path(database_path, observed_user_version);

    fs::rename(database_path, &backup_path).map_err(|error| {
        CommandError::retryable(
            "state_database_backup_failed",
            format!(
                "Cadence found repo-local state from a newer build at {} but could not move it aside to {}: {error}",
                database_path.display(),
                backup_path.display(),
            ),
        )
    })?;

    remove_database_sidecars(database_path);
    Ok(())
}

fn next_incompatible_backup_path(database_path: &Path, observed_user_version: i64) -> PathBuf {
    let file_name = database_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(STATE_DATABASE_FILE);
    let version_label = observed_user_version.max(0);
    let parent = database_path.parent().unwrap_or_else(|| Path::new("."));

    let mut candidate = parent.join(format!("{file_name}.incompatible-v{version_label}.bak"));
    let mut attempt = 1;
    while candidate.exists() {
        candidate = parent.join(format!(
            "{file_name}.incompatible-v{version_label}.{attempt}.bak"
        ));
        attempt += 1;
    }

    candidate
}

fn remove_database_sidecars(database_path: &Path) {
    let wal_path = database_path.with_extension("db-wal");
    let shm_path = database_path.with_extension("db-shm");
    let _ = fs::remove_file(wal_path);
    let _ = fs::remove_file(shm_path);
}

fn persist_import_rows(
    connection: &Connection,
    repository: &CanonicalRepository,
) -> Result<(), CommandError> {
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "state_database_transaction_failed",
            format!("Cadence could not start the import transaction: {error}"),
        )
    })?;

    transaction
        .execute(
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
            VALUES (?1, ?2, '', '', 0, 0, 0, ?3, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                branch = excluded.branch,
                updated_at = excluded.updated_at
            "#,
            params![
                repository.project_id,
                repository.display_name,
                repository.branch_name,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "project_row_persist_failed",
                format!("Cadence could not persist the imported project row: {error}"),
            )
        })?;

    transaction
        .execute(
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
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                root_path = excluded.root_path,
                display_name = excluded.display_name,
                branch = excluded.branch,
                head_sha = excluded.head_sha,
                is_git_repo = excluded.is_git_repo,
                updated_at = excluded.updated_at
            "#,
            params![
                repository.repository_id,
                repository.project_id,
                repository.root_path_string,
                repository.display_name,
                repository.branch_name,
                repository.head_sha,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "repository_row_persist_failed",
                format!("Cadence could not persist the imported repository row: {error}"),
            )
        })?;

    transaction
        .execute(
            "UPDATE agent_sessions SET selected = 0 WHERE project_id = ?1",
            params![repository.project_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!("Cadence could not clear selected agent sessions before import: {error}"),
            )
        })?;

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
                updated_at
            )
            VALUES (?1, 'agent-session-main', 'Main', '', 'active', 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(project_id, agent_session_id) DO UPDATE SET
                selected = CASE
                    WHEN agent_sessions.status = 'active' THEN 1
                    ELSE agent_sessions.selected
                END,
                updated_at = CASE
                    WHEN agent_sessions.status = 'active' THEN excluded.updated_at
                    ELSE agent_sessions.updated_at
                END
            "#,
            params![repository.project_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "agent_session_persist_failed",
                format!("Cadence could not persist the default agent session row: {error}"),
            )
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "state_database_commit_failed",
            format!("Cadence could not commit the import transaction: {error}"),
        )
    })
}

fn cleanup_partial_state(
    cadence_directory: &Path,
    database_path: &Path,
    cadence_directory_existed: bool,
    database_existed: bool,
) {
    if !database_existed {
        let _ = fs::remove_file(database_path);
        let wal_path = database_path.with_extension("db-wal");
        let shm_path = database_path.with_extension("db-shm");
        let _ = fs::remove_file(wal_path);
        let _ = fs::remove_file(shm_path);
    }

    if !cadence_directory_existed {
        let _ = fs::remove_dir(cadence_directory);
    }
}
