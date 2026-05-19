use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
    time::Duration,
};

use rusqlite::Connection;
use rusqlite_migration::{Error as MigrationError, MigrationDefinitionError};

use crate::commands::CommandError;

pub mod environment_profile;
pub mod migrations;
pub mod permissions;
pub mod user_added_tools;

pub const GLOBAL_DATABASE_FILE_NAME: &str = "xero.db";

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct MigratedGlobalDatabaseKey {
    path: PathBuf,
}

static MIGRATED_GLOBAL_DATABASES: LazyLock<Mutex<HashSet<MigratedGlobalDatabaseKey>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

pub fn global_database_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(GLOBAL_DATABASE_FILE_NAME)
}

pub fn open_global_database(database_path: &Path) -> Result<Connection, CommandError> {
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "global_database_dir_unavailable",
                format!(
                    "Xero could not prepare the app-data directory at {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }

    let database_existed = database_path.exists();
    let mut connection = Connection::open(database_path).map_err(|error| {
        CommandError::retryable(
            "global_database_open_failed",
            format!(
                "Xero could not open the global database at {}: {error}",
                database_path.display()
            ),
        )
    })?;

    configure_connection(&connection)?;

    let migration_key = migrated_global_database_key(database_path);
    let mut migrated_databases = MIGRATED_GLOBAL_DATABASES.lock().map_err(|_| {
        CommandError::system_fault(
            "global_database_migration_cache_failed",
            "Xero could not check the global database migration cache.",
        )
    })?;
    if migrated_databases.contains(&migration_key) {
        return Ok(connection);
    }

    let observed_user_version = read_user_version(&connection);
    if database_existed && observed_user_version > migrations::GLOBAL_DATABASE_SCHEMA_VERSION {
        return Err(global_database_schema_too_new_error(
            database_path,
            observed_user_version,
        ));
    }

    let migration_backup_path =
        if database_existed && observed_user_version < migrations::GLOBAL_DATABASE_SCHEMA_VERSION {
            checkpoint_global_database_for_backup(&connection, database_path)?;
            Some(backup_global_database_before_migration(
                database_path,
                observed_user_version,
                migrations::GLOBAL_DATABASE_SCHEMA_VERSION,
            )?)
        } else {
            None
        };

    match migrations::migrations().to_latest(&mut connection) {
        Ok(()) => {}
        Err(error) if is_database_too_far_ahead(&error) => {
            let observed_user_version = read_user_version(&connection);
            return Err(global_database_schema_too_new_error(
                database_path,
                observed_user_version,
            ));
        }
        Err(error) => {
            return Err(global_database_migration_error(
                database_path,
                migration_backup_path.as_deref(),
                error,
            ));
        }
    }

    migrated_databases.insert(migration_key);

    Ok(connection)
}

pub(crate) fn configure_connection(connection: &Connection) -> Result<(), CommandError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| {
            CommandError::system_fault(
                "global_database_configuration_failed",
                format!("Xero could not configure SQLite busy timeout: {error}"),
            )
        })?;

    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON; \
             PRAGMA journal_mode = WAL; \
             PRAGMA synchronous = NORMAL;",
        )
        .map_err(|error| {
            CommandError::system_fault(
                "global_database_configuration_failed",
                format!("Xero could not configure SQLite pragmas: {error}"),
            )
        })
}

fn global_database_migration_error(
    database_path: &Path,
    backup_path: Option<&Path>,
    error: MigrationError,
) -> CommandError {
    let backup_hint = backup_path
        .map(|path| format!(" A pre-migration backup was saved at {}.", path.display()))
        .unwrap_or_default();
    CommandError::system_fault(
        "global_database_migration_failed",
        format!(
            "Xero could not migrate the global database at {}.{backup_hint} The local app state was not reset: {error}",
            database_path.display(),
        ),
    )
}

fn global_database_schema_too_new_error(
    database_path: &Path,
    observed_user_version: i64,
) -> CommandError {
    CommandError::user_fixable(
        "global_database_schema_too_new",
        format!(
            "The global database at {} was created by a newer Xero build (schema v{}). Install a newer build or move that app-data database aside before launching this one.",
            database_path.display(),
            observed_user_version.max(0),
        ),
    )
}

fn is_database_too_far_ahead(error: &MigrationError) -> bool {
    matches!(
        error,
        MigrationError::MigrationDefinition(MigrationDefinitionError::DatabaseTooFarAhead)
    )
}

fn migrated_global_database_key(database_path: &Path) -> MigratedGlobalDatabaseKey {
    let path = fs::canonicalize(database_path).unwrap_or_else(|_| database_path.to_path_buf());

    MigratedGlobalDatabaseKey { path }
}

fn read_user_version(connection: &Connection) -> i64 {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap_or(0)
}

fn checkpoint_global_database_for_backup(
    connection: &Connection,
    database_path: &Path,
) -> Result<(), CommandError> {
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| {
            CommandError::retryable(
                "global_database_checkpoint_failed",
                format!(
                    "Xero needs to migrate the global database at {} but could not checkpoint pending SQLite WAL data before backing it up: {error}",
                    database_path.display(),
                ),
            )
        })
}

fn backup_global_database_before_migration(
    database_path: &Path,
    from_version: i64,
    to_version: i64,
) -> Result<PathBuf, CommandError> {
    let backup_path = next_schema_migration_backup_path(database_path, from_version, to_version);

    fs::copy(database_path, &backup_path).map_err(|error| {
        CommandError::retryable(
            "global_database_backup_failed",
            format!(
                "Xero needs to migrate the global database at {} but could not create a pre-migration backup at {}: {error}",
                database_path.display(),
                backup_path.display(),
            ),
        )
    })?;

    Ok(backup_path)
}

fn next_schema_migration_backup_path(
    database_path: &Path,
    from_version: i64,
    to_version: i64,
) -> PathBuf {
    let file_name = database_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(GLOBAL_DATABASE_FILE_NAME);
    let from_label = from_version.max(0);
    let to_label = to_version.max(0);
    let parent = database_path.parent().unwrap_or_else(|| Path::new("."));

    let mut candidate = parent.join(format!(
        "{file_name}.pre-migration-v{from_label}-to-v{to_label}.bak"
    ));
    let mut attempt = 1;
    while candidate.exists() {
        candidate = parent.join(format!(
            "{file_name}.pre-migration-v{from_label}-to-v{to_label}.{attempt}.bak"
        ));
        attempt += 1;
    }

    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn migrate_in_memory() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory db");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        migrations::migrations()
            .to_latest(&mut connection)
            .expect("walk migrations to latest");
        connection
    }

    #[test]
    fn migrations_validate() {
        migrations::migrations()
            .validate()
            .expect("global migrations are well-formed");
    }

    #[test]
    fn migrations_apply_to_empty_database() {
        let connection = migrate_in_memory();

        let expected_tables = [
            "openai_codex_sessions",
            "provider_credentials",
            "notification_credentials",
            "notification_inbound_cursors",
            "runtime_settings",
            "dictation_settings",
            "browser_control_settings",
            "adrenaline_mode_settings",
            "closed_lid_mode_settings",
            "soul_settings",
            "skill_sources",
            "mcp_registry",
            "provider_model_catalog_cache",
            "provider_preflight_results",
            "environment_profile",
            "user_added_environment_tools",
            "projects",
            "repositories",
        ];

        for table in expected_tables {
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            assert_eq!(
                count, 1,
                "expected table `{table}` to exist after migration"
            );
        }

        for table in [
            "provider_profiles",
            "provider_profiles_metadata",
            "provider_profile_credentials",
        ] {
            let count: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    [table],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            assert_eq!(
                count, 0,
                "legacy table `{table}` should be absent from the fresh baseline"
            );
        }
    }

    #[test]
    fn schema_version_constant_matches_latest_migration() {
        let connection = migrate_in_memory();
        let user_version = read_user_version(&connection);

        assert_eq!(
            user_version,
            migrations::GLOBAL_DATABASE_SCHEMA_VERSION,
            "GLOBAL_DATABASE_SCHEMA_VERSION must match the user_version written by rusqlite_migration"
        );
    }

    #[test]
    fn open_global_database_keeps_current_schema_after_process_restart() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let database_path = tempdir.path().join("xero.db");

        {
            let mut connection = Connection::open(&database_path).expect("open seeded db");
            configure_connection(&connection).expect("configure seeded db");
            migrations::migrations()
                .to_latest(&mut connection)
                .expect("seed latest schema");
        }

        let connection = open_global_database(&database_path).expect("open current schema");
        assert_eq!(
            read_user_version(&connection),
            migrations::GLOBAL_DATABASE_SCHEMA_VERSION
        );
        let incompatible_backups = fs::read_dir(tempdir.path())
            .expect("read tempdir")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with("xero.db.incompatible-v")
            })
            .count();
        assert_eq!(
            incompatible_backups, 0,
            "a database at the current schema must not be quarantined on cold restart"
        );
    }

    #[test]
    fn environment_profile_migration_enforces_contract() {
        let connection = migrate_in_memory();

        let index_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = 'idx_environment_profile_refreshed_at'",
                [],
                |row| row.get(0),
            )
            .expect("count environment profile index");
        assert_eq!(
            index_count, 1,
            "environment profile refresh index should exist"
        );

        let payload_json = r#"{
            "schemaVersion": 1,
            "platform": {
                "osKind": "macos",
                "osVersion": "15.4",
                "arch": "aarch64",
                "defaultShell": "zsh"
            },
            "path": {
                "entryCount": 2,
                "fingerprint": "sha256-demo",
                "sources": ["tauri-process-path", "common-dev-dirs"]
            },
            "tools": [],
            "capabilities": [],
            "permissions": [],
            "diagnostics": []
        }"#;
        let summary_json = r#"{
            "schemaVersion": 1,
            "status": "ready",
            "platform": {
                "osKind": "macos",
                "osVersion": "15.4",
                "arch": "aarch64",
                "defaultShell": "zsh"
            },
            "refreshedAt": "2026-04-30T12:00:00Z",
            "tools": [],
            "capabilities": [],
            "permissionRequests": [],
            "diagnostics": []
        }"#;

        connection
            .execute(
                "INSERT INTO environment_profile (
                    id, schema_version, status, os_kind, os_version, arch, default_shell,
                    path_fingerprint, payload_json, summary_json, refreshed_at
                ) VALUES (1, 1, 'ready', 'macos', '15.4', 'aarch64', 'zsh',
                    'sha256-demo', ?1, ?2, '2026-04-30T12:00:00Z'
                )",
                rusqlite::params![payload_json, summary_json],
            )
            .expect("valid environment profile row inserts");

        let invalid_status = connection.execute(
            "UPDATE environment_profile SET status = 'complete' WHERE id = 1",
            [],
        );
        assert!(
            invalid_status.is_err(),
            "environment profile status should be constrained"
        );

        let invalid_json = connection.execute(
            "UPDATE environment_profile SET payload_json = 'not json' WHERE id = 1",
            [],
        );
        assert!(
            invalid_json.is_err(),
            "environment profile payload should require valid JSON"
        );
    }

    #[test]
    fn open_global_database_creates_file_and_migrates() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let database_path = tempdir.path().join("xero.db");

        let connection = open_global_database(&database_path).expect("open and migrate");
        assert!(database_path.exists(), "database file should exist on disk");

        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("read foreign_keys pragma");
        assert_eq!(foreign_keys, 1, "foreign keys should be enabled");

        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("read journal_mode pragma");
        assert_eq!(
            journal_mode.to_ascii_lowercase(),
            "wal",
            "journal_mode should be wal"
        );
    }

    #[test]
    fn open_global_database_is_idempotent() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let database_path = tempdir.path().join("xero.db");

        {
            let _ = open_global_database(&database_path).expect("first open");
        }
        let _ = open_global_database(&database_path).expect("second open is idempotent");
    }

    #[test]
    fn open_global_database_migrates_v10_state_without_losing_user_data() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let database_path = tempdir.path().join("xero.db");

        {
            let connection = Connection::open(&database_path).expect("open v10 db");
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE provider_credentials (
                        provider_id              TEXT    PRIMARY KEY,
                        kind                     TEXT    NOT NULL,
                        api_key                  TEXT,
                        oauth_account_id         TEXT,
                        oauth_session_id         TEXT,
                        oauth_access_token       TEXT,
                        oauth_refresh_token      TEXT,
                        oauth_expires_at         INTEGER,
                        base_url                 TEXT,
                        api_version              TEXT,
                        region                   TEXT,
                        scope_project_id         TEXT,
                        default_model_id         TEXT,
                        updated_at               TEXT    NOT NULL
                    );

                    CREATE TABLE openai_codex_sessions (
                        account_id TEXT PRIMARY KEY,
                        provider_id TEXT NOT NULL,
                        session_id TEXT NOT NULL,
                        access_token TEXT NOT NULL,
                        refresh_token TEXT NOT NULL,
                        expires_at INTEGER NOT NULL,
                        updated_at TEXT NOT NULL
                    );

                    CREATE TABLE projects (
                        id TEXT PRIMARY KEY,
                        name TEXT NOT NULL,
                        description TEXT NOT NULL DEFAULT '',
                        milestone TEXT NOT NULL DEFAULT '',
                        total_phases INTEGER NOT NULL DEFAULT 0 CHECK (total_phases >= 0),
                        completed_phases INTEGER NOT NULL DEFAULT 0 CHECK (completed_phases >= 0),
                        active_phase INTEGER NOT NULL DEFAULT 0 CHECK (active_phase >= 0),
                        branch TEXT,
                        runtime TEXT,
                        start_targets TEXT NOT NULL DEFAULT '[]',
                        created_at TEXT NOT NULL,
                        updated_at TEXT NOT NULL
                    );

                    CREATE TABLE repositories (
                        id TEXT PRIMARY KEY,
                        project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                        root_path TEXT NOT NULL UNIQUE,
                        display_name TEXT NOT NULL,
                        branch TEXT,
                        head_sha TEXT,
                        is_git_repo INTEGER NOT NULL DEFAULT 1 CHECK (is_git_repo IN (0, 1)),
                        created_at TEXT NOT NULL,
                        updated_at TEXT NOT NULL
                    );

                    INSERT INTO provider_credentials (
                        provider_id, kind, oauth_account_id, oauth_session_id,
                        oauth_access_token, oauth_refresh_token, oauth_expires_at, updated_at
                    ) VALUES (
                        'openai-codex', 'oauth_session', 'acct-1', 'session-1',
                        'access-token', 'refresh-token', 1893456000, '2026-05-18T12:00:00Z'
                    );
                    INSERT INTO openai_codex_sessions (
                        account_id, provider_id, session_id, access_token,
                        refresh_token, expires_at, updated_at
                    ) VALUES (
                        'acct-1', 'openai-codex', 'session-1', 'access-token',
                        'refresh-token', 1893456000, '2026-05-18T12:00:00Z'
                    );
                    INSERT INTO projects (
                        id, name, description, milestone, total_phases, completed_phases,
                        active_phase, branch, runtime, start_targets, created_at, updated_at
                    ) VALUES (
                        'project-1', 'mesh-lang', '', '', 0, 0, 0,
                        'main', NULL, '[]', '2026-05-18T12:00:00Z', '2026-05-18T12:00:00Z'
                    );
                    INSERT INTO repositories (
                        id, project_id, root_path, display_name, branch, head_sha,
                        is_git_repo, created_at, updated_at
                    ) VALUES (
                        'repo-1', 'project-1', '/Users/sn0w/Documents/dev/mesh-lang',
                        'mesh-lang', 'main', 'abc123', 1,
                        '2026-05-18T12:00:00Z', '2026-05-18T12:00:00Z'
                    );
                    PRAGMA user_version = 10;
                    "#,
                )
                .expect("seed v10 user state");
        }

        let connection = open_global_database(&database_path).expect("migrate v10 db");

        assert_eq!(
            read_user_version(&connection),
            migrations::GLOBAL_DATABASE_SCHEMA_VERSION,
            "the global database should advance to the latest schema"
        );
        assert_eq!(
            table_count(&connection, "adrenaline_mode_settings"),
            1,
            "the v11 migration should add Adrenaline Mode settings storage"
        );
        assert_eq!(
            table_count(&connection, "closed_lid_mode_settings"),
            1,
            "the v12 migration should add Closed-Lid Mode settings storage"
        );
        assert_eq!(
            row_count(&connection, "provider_credentials"),
            1,
            "OAuth credentials must survive schema upgrades"
        );
        assert_eq!(
            row_count(&connection, "openai_codex_sessions"),
            1,
            "OpenAI Codex OAuth sessions must survive schema upgrades"
        );
        assert_eq!(
            row_count(&connection, "projects"),
            1,
            "imported projects must survive schema upgrades"
        );
        assert_eq!(
            row_count(&connection, "repositories"),
            1,
            "imported repositories must survive schema upgrades"
        );
        assert!(
            tempdir
                .path()
                .join("xero.db.pre-migration-v10-to-v12.bak")
                .exists(),
            "existing user state should be copied before an in-place schema upgrade"
        );
    }

    #[test]
    fn open_global_database_rejects_newer_schema_without_rebuilding() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let database_path = tempdir.path().join("xero.db");

        {
            let connection = Connection::open(&database_path).expect("open future db");
            connection
                .execute_batch(
                    "PRAGMA user_version = 99; CREATE TABLE stale_marker (id INTEGER PRIMARY KEY);",
                )
                .expect("seed future db");
        }

        let error = open_global_database(&database_path).expect_err("newer db should be rejected");
        assert_eq!(error.code, "global_database_schema_too_new");

        let connection = Connection::open(&database_path).expect("reopen future db");
        let stale_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'stale_marker'",
                [],
                |row| row.get(0),
            )
            .expect("count stale table");
        assert_eq!(
            stale_table_count, 1,
            "state from a newer app version must not be rebuilt or discarded"
        );
        assert_eq!(
            read_user_version(&connection),
            99,
            "the newer schema version should remain intact"
        );
        assert!(
            !tempdir.path().join("xero.db.incompatible-v99.bak").exists(),
            "newer app-data should not be moved aside and replaced"
        );
    }

    #[test]
    fn repositories_cascade_with_project_delete() {
        let connection = migrate_in_memory();

        connection
            .execute(
                "INSERT INTO projects (id, name) VALUES ('proj-1', 'Demo')",
                [],
            )
            .expect("insert project");
        connection
            .execute(
                "INSERT INTO repositories (id, project_id, root_path, display_name) \
                 VALUES ('repo-1', 'proj-1', '/tmp/demo', 'Demo')",
                [],
            )
            .expect("insert repository");

        connection
            .execute("DELETE FROM projects WHERE id = 'proj-1'", [])
            .expect("delete project");

        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM repositories WHERE id = 'repo-1'",
                [],
                |row| row.get(0),
            )
            .expect("count remaining repositories");
        assert_eq!(
            remaining, 0,
            "repositories should cascade with project delete"
        );
    }

    #[test]
    fn provider_credentials_schema_is_current_baseline() {
        let connection = migrate_in_memory();

        let columns = table_columns(&connection, "provider_credentials");
        assert_eq!(
            columns,
            [
                "provider_id",
                "kind",
                "api_key",
                "oauth_account_id",
                "oauth_session_id",
                "oauth_access_token",
                "oauth_refresh_token",
                "oauth_expires_at",
                "base_url",
                "api_version",
                "region",
                "scope_project_id",
                "default_model_id",
                "updated_at",
            ],
            "provider_credentials should be created directly in the baseline schema"
        );

        let old_profile_columns = table_columns(&connection, "provider_profiles");
        assert!(
            old_profile_columns.is_empty(),
            "legacy provider profile tables should not exist in the fresh baseline"
        );
    }

    #[test]
    fn provider_credentials_migration_is_idempotent() {
        let mut connection = Connection::open_in_memory().expect("open in-memory db");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        migrations::migrations()
            .to_latest(&mut connection)
            .expect("first migration walk");

        // Re-running migrations on an already-migrated db must be a no-op.
        migrations::migrations()
            .to_latest(&mut connection)
            .expect("second migration walk is idempotent");
    }

    fn table_columns(connection: &Connection, table: &str) -> Vec<String> {
        let mut stmt = connection
            .prepare(&format!("PRAGMA table_info({table})"))
            .expect("prepare table_info");
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(1))
            .expect("query table_info");
        rows.map(|row| row.expect("read column name")).collect()
    }

    fn table_count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .expect("count table")
    }

    fn row_count(connection: &Connection, table: &str) -> i64 {
        connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .expect("count rows")
    }
}
