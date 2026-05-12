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
    commands::{CommandError, ProjectOriginDto, ProjectSummaryDto, RepositorySummaryDto},
    db::migrations::migrations,
    git::repository::CanonicalRepository,
    state::ImportFailpoints,
};

pub mod migrations;
pub mod project_store;

const STATE_DATABASE_FILE: &str = "state.db";
const PROJECTS_DIRECTORY: &str = "projects";
const APP_DATA_DIRECTORY_NAME: &str = "dev.sn0w.xero";

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectOrigin {
    Brownfield,
    Greenfield,
    Unknown,
}

impl ProjectOrigin {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Brownfield => "brownfield",
            Self::Greenfield => "greenfield",
            Self::Unknown => "unknown",
        }
    }

    pub fn to_dto(self) -> ProjectOriginDto {
        match self {
            Self::Brownfield => ProjectOriginDto::Brownfield,
            Self::Greenfield => ProjectOriginDto::Greenfield,
            Self::Unknown => ProjectOriginDto::Unknown,
        }
    }
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

    let _ = global_db_path;
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

pub fn project_app_data_dir_for_project(project_id: &str) -> PathBuf {
    database_path_for_project(project_id)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_project_app_data_dir(project_id))
}

pub fn project_app_data_dir_for_repo(repo_root: &Path) -> PathBuf {
    database_path_for_repo(repo_root)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            project_app_data_dir_for_project(&stable_project_id_for_repo_root(repo_root))
        })
}

pub fn database_path_for_repo(repo_root: &Path) -> PathBuf {
    database_path_for_registered_repo(repo_root)
        .unwrap_or_else(|| database_path_for_project(&stable_project_id_for_repo_root(repo_root)))
}

pub fn import_project(
    repository: &CanonicalRepository,
    failpoints: &ImportFailpoints,
) -> Result<ImportedProjectRecord, CommandError> {
    import_project_with_origin(repository, ProjectOrigin::Brownfield, failpoints)
}

pub fn import_project_with_origin(
    repository: &CanonicalRepository,
    project_origin: ProjectOrigin,
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
                "Xero could not prepare the project state directory at {}: {error}",
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

        persist_import_rows(&connection, repository, project_origin)?;

        Ok(ImportedProjectRecord {
            project: ProjectSummaryDto {
                id: repository.project_id.clone(),
                name: repository.display_name.clone(),
                description: String::new(),
                milestone: String::new(),
                project_origin: project_origin.to_dto(),
                total_phases: 0,
                completed_phases: 0,
                active_phase: 0,
                branch: repository.branch_name.clone(),
                runtime: None,
                start_targets: Vec::new(),
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
                format!("Xero could not configure SQLite busy timeout: {error}"),
            )
        })?;

    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA wal_autocheckpoint = 1000;",
        )
        .map_err(|error| {
            CommandError::system_fault(
                "state_database_configuration_failed",
                format!("Xero could not configure SQLite pragmas: {error}"),
            )
        })
}

fn open_database_connection(database_path: &Path) -> Result<Connection, CommandError> {
    Connection::open(database_path).map_err(|error| {
        CommandError::retryable(
            "state_database_open_failed",
            format!(
                "Xero could not open the project state database at {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn state_database_migration_error(database_path: &Path, error: MigrationError) -> CommandError {
    CommandError::system_fault(
        "state_database_migration_failed",
        format!(
            "Xero could not migrate the project state database at {}: {error}",
            database_path.display()
        ),
    )
}

pub(crate) fn is_database_too_far_ahead(error: &MigrationError) -> bool {
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
                "Xero found project state from a newer build at {} but could not move it aside to {}: {error}",
                database_path.display(),
                backup_path.display(),
            ),
        )
    })?;

    remove_database_sidecars(database_path);
    Ok(())
}

pub(crate) fn rebuild_incompatible_project_database(
    repo_root: &Path,
    database_path: &Path,
    connection: Connection,
) -> Result<Connection, CommandError> {
    let observed_user_version = read_user_version(&connection);
    let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    drop(connection);

    quarantine_incompatible_database(database_path, observed_user_version)?;

    let mut reset_connection = open_database_connection(database_path)?;
    configure_connection(&reset_connection)?;
    migrations()
        .to_latest(&mut reset_connection)
        .map_err(|error| state_database_migration_error(database_path, error))?;

    let repository = crate::git::repository::open_repository_root(repo_root)?;
    persist_import_rows(
        &reset_connection,
        &repository.canonical_repository()?,
        ProjectOrigin::Brownfield,
    )?;
    register_project_database_path(repo_root, database_path);

    Ok(reset_connection)
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
    project_origin: ProjectOrigin,
) -> Result<(), CommandError> {
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "state_database_transaction_failed",
            format!("Xero could not start the import transaction: {error}"),
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
                project_origin,
                total_phases,
                completed_phases,
                active_phase,
                branch,
                runtime,
                updated_at
            )
            VALUES (?1, ?2, '', '', ?3, 0, 0, 0, ?4, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                project_origin = excluded.project_origin,
                branch = excluded.branch,
                updated_at = excluded.updated_at
            "#,
            params![
                repository.project_id,
                repository.display_name,
                project_origin.as_str(),
                repository.branch_name,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "project_row_persist_failed",
                format!("Xero could not persist the imported project row: {error}"),
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
                format!("Xero could not persist the imported repository row: {error}"),
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
                format!("Xero could not persist the project-state metadata row: {error}"),
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
                format!("Xero could not clear selected agent sessions before import: {error}"),
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
                format!("Xero could not persist the default agent session row: {error}"),
            )
        })?;

    transaction.commit().map_err(|error| {
        CommandError::system_fault(
            "state_database_commit_failed",
            format!("Xero could not commit the import transaction: {error}"),
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
    default_project_app_data_dir(project_id).join(STATE_DATABASE_FILE)
}

fn default_project_app_data_dir(project_id: &str) -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(APP_DATA_DIRECTORY_NAME)
        .join(PROJECTS_DIRECTORY)
        .join(project_id)
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

#[cfg(test)]
pub(crate) fn register_project_database_path_for_tests(repo_root: &Path, database_path: PathBuf) {
    register_project_database_path(repo_root, &database_path);
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        commands::{CommandErrorClass, RuntimeAgentIdDto},
        state::ImportFailpoints,
    };
    use tempfile::tempdir;

    fn canonical_repository(root_path: &Path, project_id: &str) -> CanonicalRepository {
        let root_path = fs::canonicalize(root_path).expect("canonical repo root");
        CanonicalRepository {
            project_id: project_id.into(),
            repository_id: format!("repo_{project_id}"),
            root_path: root_path.clone(),
            root_path_string: root_path.to_string_lossy().into_owned(),
            common_git_dir: root_path.join(".git"),
            display_name: project_id.into(),
            branch_name: Some("main".into()),
            head_sha: Some("0123456789abcdef".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        }
    }

    #[test]
    fn s39_configure_connection_applies_project_database_pragmas() {
        let tempdir = tempdir().expect("tempdir");
        let database_path = tempdir.path().join("state.db");
        let connection = Connection::open(database_path).expect("open database");

        configure_connection(&connection).expect("configure connection");

        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign keys pragma");
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode pragma");
        let synchronous: i64 = connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .expect("synchronous pragma");
        let busy_timeout_ms: i64 = connection
            .query_row("PRAGMA busy_timeout", [], |row| row.get(0))
            .expect("busy timeout pragma");
        let wal_autocheckpoint: i64 = connection
            .query_row("PRAGMA wal_autocheckpoint", [], |row| row.get(0))
            .expect("wal autocheckpoint pragma");

        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(synchronous, 1);
        assert_eq!(busy_timeout_ms, 5_000);
        assert_eq!(wal_autocheckpoint, 1_000);
    }

    #[test]
    fn import_project_marks_existing_repository_brownfield_and_allows_crawl() {
        let tempdir = tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("brownfield-repo");
        fs::create_dir_all(&repo_root).expect("repo root");
        configure_project_database_paths(&tempdir.path().join("global").join("state.db"));

        let repository = canonical_repository(&repo_root, "project_brownfield_origin");
        let record =
            import_project(&repository, &ImportFailpoints::default()).expect("import project");

        assert_eq!(record.project.project_origin, ProjectOriginDto::Brownfield);
        assert_eq!(
            project_store::load_project_origin(&repo_root, &repository.project_id)
                .expect("load origin"),
            ProjectOriginDto::Brownfield
        );
        project_store::ensure_runtime_agent_allowed_for_project(
            &repo_root,
            &repository.project_id,
            RuntimeAgentIdDto::Crawl,
        )
        .expect("crawl allowed for brownfield");
    }

    #[test]
    fn greenfield_project_origin_is_persisted_and_rejects_crawl() {
        let tempdir = tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("greenfield-repo");
        fs::create_dir_all(&repo_root).expect("repo root");
        configure_project_database_paths(&tempdir.path().join("global").join("state.db"));

        let repository = canonical_repository(&repo_root, "project_greenfield_origin");
        let record = import_project_with_origin(
            &repository,
            ProjectOrigin::Greenfield,
            &ImportFailpoints::default(),
        )
        .expect("import project");

        assert_eq!(record.project.project_origin, ProjectOriginDto::Greenfield);
        assert_eq!(
            project_store::load_project_origin(&repo_root, &repository.project_id)
                .expect("load origin"),
            ProjectOriginDto::Greenfield
        );

        let error = project_store::ensure_runtime_agent_allowed_for_project(
            &repo_root,
            &repository.project_id,
            RuntimeAgentIdDto::Crawl,
        )
        .expect_err("crawl should be rejected for greenfield");
        assert_eq!(error.code, "runtime_agent_crawl_unavailable_greenfield");
        assert_eq!(error.class, CommandErrorClass::UserFixable);
    }
}
