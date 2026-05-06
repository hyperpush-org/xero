use std::{
    fs,
    path::{Path, PathBuf},
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

    match migrations::migrations().to_latest(&mut connection) {
        Ok(()) => {}
        Err(error) if database_existed && is_database_too_far_ahead(&error) => {
            let observed_user_version = read_user_version(&connection);
            let _ = connection.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
            drop(connection);

            quarantine_incompatible_database(database_path, observed_user_version)?;

            let mut reset_connection = Connection::open(database_path).map_err(|error| {
                CommandError::retryable(
                    "global_database_open_failed",
                    format!(
                        "Xero could not recreate the global database at {}: {error}",
                        database_path.display()
                    ),
                )
            })?;
            configure_connection(&reset_connection)?;
            migrations::migrations()
                .to_latest(&mut reset_connection)
                .map_err(|error| global_database_migration_error(database_path, error))?;
            connection = reset_connection;
        }
        Err(error) => return Err(global_database_migration_error(database_path, error)),
    }

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

fn global_database_migration_error(database_path: &Path, error: MigrationError) -> CommandError {
    CommandError::system_fault(
        "global_database_migration_failed",
        format!(
            "Xero could not initialize the global database at {}. The local app state may need to be reset: {error}",
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
            "global_database_backup_failed",
            format!(
                "Xero found pre-release app state from an incompatible build at {} but could not move it aside to {}: {error}",
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
        .unwrap_or(GLOBAL_DATABASE_FILE_NAME);
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
    fn open_global_database_rebuilds_incompatible_pre_release_state() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let database_path = tempdir.path().join("xero.db");

        {
            let connection = Connection::open(&database_path).expect("open old db");
            connection
                .execute_batch(
                    "PRAGMA user_version = 99; CREATE TABLE stale_marker (id INTEGER PRIMARY KEY);",
                )
                .expect("seed incompatible db");
        }

        let connection = open_global_database(&database_path).expect("rebuild incompatible db");
        let stale_table_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'stale_marker'",
                [],
                |row| row.get(0),
            )
            .expect("count stale table");
        assert_eq!(
            stale_table_count, 0,
            "incompatible pre-release state should be moved aside before rebuilding"
        );
        assert!(
            tempdir.path().join("xero.db.incompatible-v99.bak").exists(),
            "the incompatible database should be quarantined for inspection"
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
}
