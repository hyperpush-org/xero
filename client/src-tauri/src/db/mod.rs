use std::{
    cell::RefCell,
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, RwLock},
    time::Duration,
};

use rusqlite::{params, Connection};
use rusqlite_migration::{Error as MigrationError, MigrationDefinitionError};
use sha2::{Digest, Sha256};

use crate::{
    commands::{CommandError, ProjectSummaryDto, RepositorySummaryDto},
    db::migrations::migrations,
    git::repository::CanonicalRepository,
    state::ImportFailpoints,
};

pub mod migrations;
pub mod project_store;

const STATE_DATABASE_FILE: &str = "state.db";
const PROJECTS_DIRECTORY: &str = "projects";
const APP_DATA_DIRECTORY_NAME: &str = "dev.sn0w.cadence";

#[derive(Debug, Clone, Default)]
struct ProjectDatabasePathConfig {
    project_root: Option<PathBuf>,
    registry_path: Option<PathBuf>,
    repo_root_to_database_path: HashMap<PathBuf, PathBuf>,
}

static PROJECT_DATABASE_PATH_CONFIG: LazyLock<RwLock<ProjectDatabasePathConfig>> =
    LazyLock::new(|| RwLock::new(ProjectDatabasePathConfig::default()));
thread_local! {
    static THREAD_PROJECT_DATABASE_PATH_CONFIG: RefCell<ProjectDatabasePathConfig> =
        RefCell::new(ProjectDatabasePathConfig::default());
}

#[derive(Debug, Clone)]
pub struct ImportedProjectRecord {
    pub project: ProjectSummaryDto,
    pub repository: RepositorySummaryDto,
    pub database_path: PathBuf,
}

pub fn configure_project_database_paths(global_db_path: &Path) {
    let project_root = project_database_root_for_global_db(global_db_path);
    let registry_path = global_db_path.to_path_buf();

    let mut config = PROJECT_DATABASE_PATH_CONFIG
        .write()
        .expect("project database path config lock poisoned");
    config.project_root = Some(project_root.clone());
    config.registry_path = Some(registry_path.clone());
    drop(config);

    THREAD_PROJECT_DATABASE_PATH_CONFIG.with(|thread_config| {
        let mut config = thread_config.borrow_mut();
        config.project_root = Some(project_root);
        config.registry_path = Some(registry_path);
        config.repo_root_to_database_path.clear();
    });

    // Propagate the configured global DB path to child processes (notably the detached
    // runtime supervisor sidecar) via the shared env var so they resolve the same DB.
    std::env::set_var(
        crate::runtime::supervisor::CADENCE_GLOBAL_DB_PATH_ENV,
        global_db_path,
    );
}

pub fn database_path_for_project(project_id: &str) -> PathBuf {
    configured_database_path_for_project(project_id)
        .unwrap_or_else(|| default_database_path_for_project(project_id))
}

pub fn database_path_for_project_in_app_data(app_data_dir: &Path, project_id: &str) -> PathBuf {
    app_data_dir
        .join(PROJECTS_DIRECTORY)
        .join(project_id)
        .join(STATE_DATABASE_FILE)
}

pub fn database_path_for_repo(repo_root: &Path) -> PathBuf {
    database_path_for_registered_repo(repo_root)
        .unwrap_or_else(|| database_path_for_project(&stable_project_id_for_repo_root(repo_root)))
}

pub fn import_project(
    repository: &CanonicalRepository,
    failpoints: &ImportFailpoints,
) -> Result<ImportedProjectRecord, CommandError> {
    let database_path = configured_database_path_for_project(&repository.project_id)
        .unwrap_or_else(|| fallback_database_path_for_unconfigured_import(&repository.project_id));
    let database_directory = database_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let database_directory_existed = database_directory.exists();
    let database_existed = database_path.exists();

    fs::create_dir_all(&database_directory).map_err(|error| {
        CommandError::retryable(
            "project_state_dir_unavailable",
            format!(
                "Cadence could not prepare the project state directory at {}: {error}",
                database_directory.display()
            ),
        )
    })?;

    let import_result = (|| -> Result<ImportedProjectRecord, CommandError> {
        let mut connection = open_database_connection(&database_path)?;
        configure_connection(&connection)?;

        if failpoints.fail_migration {
            return Err(CommandError::system_fault(
                "state_database_migration_failed",
                "Test failpoint forced the project-state migration to fail before any rows were written.",
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
            &database_directory,
            &database_path,
            database_directory_existed,
            database_existed,
        );
    } else {
        register_project_database_path(&repository.root_path, &database_path);
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
                "Cadence could not open the project state database at {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn state_database_migration_error(database_path: &Path, error: MigrationError) -> CommandError {
    CommandError::system_fault(
        "state_database_migration_failed",
        format!(
            "Cadence could not migrate the project state database at {}: {error}",
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
                "Cadence found project state from a newer build at {} but could not move it aside to {}: {error}",
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
            r#"
            INSERT INTO meta (id, project_id, updated_at)
            VALUES (1, ?1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(id) DO UPDATE SET
                project_id = excluded.project_id,
                updated_at = excluded.updated_at
            "#,
            params![repository.project_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "project_meta_persist_failed",
                format!("Cadence could not persist the project-state metadata row: {error}"),
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
    database_directory: &Path,
    database_path: &Path,
    database_directory_existed: bool,
    database_existed: bool,
) {
    if !database_existed {
        let _ = fs::remove_file(database_path);
        let wal_path = database_path.with_extension("db-wal");
        let shm_path = database_path.with_extension("db-shm");
        let _ = fs::remove_file(wal_path);
        let _ = fs::remove_file(shm_path);
    }

    if !database_directory_existed {
        let _ = fs::remove_dir(database_directory);
    }
}

fn project_database_root_for_global_db(global_db_path: &Path) -> PathBuf {
    global_db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(PROJECTS_DIRECTORY)
}

fn configured_database_path_for_project(project_id: &str) -> Option<PathBuf> {
    let thread_path = THREAD_PROJECT_DATABASE_PATH_CONFIG.with(|thread_config| {
        thread_config
            .borrow()
            .project_root
            .as_ref()
            .map(|root| root.join(project_id).join(STATE_DATABASE_FILE))
    });
    if thread_path.is_some() {
        return thread_path;
    }

    let config = PROJECT_DATABASE_PATH_CONFIG
        .read()
        .expect("project database path config lock poisoned");
    config
        .project_root
        .as_ref()
        .map(|root| root.join(project_id).join(STATE_DATABASE_FILE))
}

fn default_database_path_for_project(project_id: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DATA_DIRECTORY_NAME)
        .join(PROJECTS_DIRECTORY)
        .join(project_id)
        .join(STATE_DATABASE_FILE)
}

fn database_path_for_registered_repo(repo_root: &Path) -> Option<PathBuf> {
    let normalized_repo_root = normalize_repo_root(repo_root);

    if let Some(database_path) = THREAD_PROJECT_DATABASE_PATH_CONFIG.with(|thread_config| {
        thread_config
            .borrow()
            .repo_root_to_database_path
            .get(&normalized_repo_root)
            .cloned()
    }) {
        return Some(database_path);
    }

    if let Some(project_id) = {
        let config = PROJECT_DATABASE_PATH_CONFIG
            .read()
            .expect("project database path config lock poisoned");
        config
            .repo_root_to_database_path
            .get(&normalized_repo_root)
            .cloned()
    } {
        return Some(project_id);
    }

    let registry_path = THREAD_PROJECT_DATABASE_PATH_CONFIG
        .with(|thread_config| thread_config.borrow().registry_path.clone())
        .or_else(|| {
            let config = PROJECT_DATABASE_PATH_CONFIG
                .read()
                .expect("project database path config lock poisoned");
            config.registry_path.clone()
        })?;

    let registry = crate::registry::read_registry(&registry_path).ok()?;
    let record = registry
        .projects
        .into_iter()
        .find(|record| normalize_repo_root(Path::new(&record.root_path)) == normalized_repo_root)?;

    let database_path = configured_database_path_for_project(&record.project_id)?;
    register_project_database_path(&normalized_repo_root, &database_path);
    Some(database_path)
}

fn register_project_database_path(repo_root: &Path, database_path: &Path) {
    let normalized_repo_root = normalize_repo_root(repo_root);
    THREAD_PROJECT_DATABASE_PATH_CONFIG.with(|thread_config| {
        thread_config
            .borrow_mut()
            .repo_root_to_database_path
            .insert(normalized_repo_root.clone(), database_path.to_path_buf());
    });

    let mut config = PROJECT_DATABASE_PATH_CONFIG
        .write()
        .expect("project database path config lock poisoned");
    config
        .repo_root_to_database_path
        .insert(normalized_repo_root, database_path.to_path_buf());
}

fn normalize_repo_root(repo_root: &Path) -> PathBuf {
    fs::canonicalize(repo_root).unwrap_or_else(|_| repo_root.to_path_buf())
}

fn stable_project_id_for_repo_root(repo_root: &Path) -> String {
    let root_path_string = normalize_repo_root(repo_root)
        .to_string_lossy()
        .into_owned();
    let digest = Sha256::digest(root_path_string.as_bytes());
    let short = digest
        .iter()
        .take(16)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("project_{short}")
}

fn fallback_database_path_for_unconfigured_import(project_id: &str) -> PathBuf {
    std::env::temp_dir()
        .join(APP_DATA_DIRECTORY_NAME)
        .join(PROJECTS_DIRECTORY)
        .join(project_id)
        .join(STATE_DATABASE_FILE)
}
