use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::Connection;

use crate::commands::CommandError;

pub mod importer;
pub mod migrations;

pub use importer::{
    import_legacy_dictation_settings, import_legacy_mcp_registry,
    import_legacy_provider_model_catalog_cache, import_legacy_skill_sources,
};

pub const GLOBAL_DATABASE_FILE_NAME: &str = "cadence.db";

pub fn global_database_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(GLOBAL_DATABASE_FILE_NAME)
}

pub fn open_global_database(database_path: &Path) -> Result<Connection, CommandError> {
    if let Some(parent) = database_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "global_database_dir_unavailable",
                format!(
                    "Cadence could not prepare the app-data directory at {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }

    let mut connection = Connection::open(database_path).map_err(|error| {
        CommandError::retryable(
            "global_database_open_failed",
            format!(
                "Cadence could not open the global database at {}: {error}",
                database_path.display()
            ),
        )
    })?;

    configure_connection(&connection)?;

    migrations::migrations()
        .to_latest(&mut connection)
        .map_err(|error| {
            CommandError::system_fault(
                "global_database_migration_failed",
                format!(
                    "Cadence could not migrate the global database at {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    Ok(connection)
}

pub(crate) fn configure_connection(connection: &Connection) -> Result<(), CommandError> {
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| {
            CommandError::system_fault(
                "global_database_configuration_failed",
                format!("Cadence could not configure SQLite busy timeout: {error}"),
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
                format!("Cadence could not configure SQLite pragmas: {error}"),
            )
        })
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
            "provider_profiles",
            "provider_profiles_metadata",
            "provider_profile_credentials",
            "openai_codex_sessions",
            "notification_credentials",
            "notification_inbound_cursors",
            "runtime_settings",
            "dictation_settings",
            "skill_sources",
            "mcp_registry",
            "provider_model_catalog_cache",
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
            assert_eq!(count, 1, "expected table `{table}` to exist after migration");
        }
    }

    #[test]
    fn open_global_database_creates_file_and_migrates() {
        let tempdir = tempfile::tempdir().expect("create tempdir");
        let database_path = tempdir.path().join("cadence.db");

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
        let database_path = tempdir.path().join("cadence.db");

        {
            let _ = open_global_database(&database_path).expect("first open");
        }
        let _ = open_global_database(&database_path).expect("second open is idempotent");
    }

    #[test]
    fn provider_profile_credentials_cascade_with_profile_delete() {
        let connection = migrate_in_memory();

        connection
            .execute(
                "INSERT INTO provider_profiles (
                    profile_id, provider_id, runtime_kind, label, model_id, updated_at
                ) VALUES ('p1', 'openai', '', 'Profile 1', 'gpt-x', '2025-01-01T00:00:00Z')",
                [],
            )
            .expect("insert profile");

        connection
            .execute(
                "INSERT INTO provider_profile_credentials (
                    profile_id, api_key, updated_at
                ) VALUES ('p1', 'sk-secret', '2025-01-01T00:00:00Z')",
                [],
            )
            .expect("insert credentials");

        connection
            .execute("DELETE FROM provider_profiles WHERE profile_id = 'p1'", [])
            .expect("delete profile");

        let remaining: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM provider_profile_credentials WHERE profile_id = 'p1'",
                [],
                |row| row.get(0),
            )
            .expect("count remaining credentials");
        assert_eq!(remaining, 0, "credentials should cascade-delete with profile");
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
        assert_eq!(remaining, 0, "repositories should cascade with project delete");
    }
}
