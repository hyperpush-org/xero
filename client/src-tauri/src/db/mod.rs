use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::{params, Connection};

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
        let mut connection = Connection::open(&database_path).map_err(|error| {
            CommandError::retryable(
                "state_database_open_failed",
                format!(
                    "Cadence could not open the repo-local database at {}: {error}",
                    database_path.display()
                ),
            )
        })?;

        configure_connection(&connection)?;

        if failpoints.fail_migration {
            return Err(CommandError::system_fault(
                "state_database_migration_failed",
                "Test failpoint forced the repo-local migration to fail before any rows were written.",
            ));
        }

        migrations().to_latest(&mut connection).map_err(|error| {
            CommandError::system_fault(
                "state_database_migration_failed",
                format!(
                    "Cadence could not migrate the repo-local database at {}: {error}",
                    database_path.display()
                ),
            )
        })?;

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
