use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rusqlite::Connection;

use crate::commands::CommandError;

pub mod importer;
pub(crate) mod legacy_runtime_settings;
pub mod migrations;
pub mod permissions;

pub use importer::{
    import_legacy_dictation_settings, import_legacy_mcp_registry,
    import_legacy_provider_model_catalog_cache, import_legacy_skill_sources,
};

// Legacy JSON filenames. Phase 6 collapses every per-module `*_FILE_NAME` constant into
// this single source of truth so production modules no longer carry import-time strings.
pub(crate) const LEGACY_PROVIDER_PROFILES_FILE_NAME: &str = "provider-profiles.json";
pub(crate) const LEGACY_PROVIDER_PROFILE_CREDENTIALS_FILE_NAME: &str =
    "provider-profile-credentials.json";
pub(crate) const LEGACY_RUNTIME_SETTINGS_FILE_NAME: &str = "runtime-settings.json";
pub(crate) const LEGACY_OPENROUTER_CREDENTIAL_FILE_NAME: &str = "openrouter-credentials.json";
pub(crate) const LEGACY_OPENAI_CODEX_AUTH_STORE_FILE_NAME: &str = "openai-auth.json";
pub(crate) const LEGACY_NOTIFICATION_CREDENTIAL_STORE_FILE_NAME: &str =
    "notification-credentials.json";
pub(crate) const LEGACY_DICTATION_SETTINGS_FILE_NAME: &str = "dictation-settings.json";
pub(crate) const LEGACY_SKILL_SOURCE_SETTINGS_FILE_NAME: &str = "skill-sources.json";
pub(crate) const LEGACY_MCP_REGISTRY_FILE_NAME: &str = "mcp-registry.json";
pub(crate) const LEGACY_PROVIDER_MODEL_CATALOG_CACHE_FILE_NAME: &str =
    "provider-model-catalogs.json";
pub(crate) const LEGACY_PROJECT_REGISTRY_FILE_NAME: &str = "project-registry.json";

/// Locations of the legacy JSON files this orchestrator may consume on first boot.
/// Every field is required; missing files are skipped silently.
pub struct LegacyJsonImportPaths {
    pub global_db: PathBuf,
    pub provider_profiles: PathBuf,
    pub provider_profile_credentials: PathBuf,
    pub legacy_runtime_settings: PathBuf,
    pub legacy_openrouter_credentials: PathBuf,
    pub openai_codex_auth: PathBuf,
    pub notification_credentials: PathBuf,
    pub dictation_settings: PathBuf,
    pub skill_sources: PathBuf,
    pub mcp_registry: PathBuf,
    pub provider_model_catalog_cache: PathBuf,
    pub project_registry: PathBuf,
}

impl LegacyJsonImportPaths {
    /// Build the legacy import paths from a single app-data directory. This is the
    /// only construction site outside of tests; production callers in `lib.rs`
    /// invoke this so the legacy filename strings live in exactly one module.
    pub fn resolve(app_data_dir: &Path) -> Self {
        Self {
            global_db: global_database_path(app_data_dir),
            provider_profiles: app_data_dir.join(LEGACY_PROVIDER_PROFILES_FILE_NAME),
            provider_profile_credentials: app_data_dir
                .join(LEGACY_PROVIDER_PROFILE_CREDENTIALS_FILE_NAME),
            legacy_runtime_settings: app_data_dir.join(LEGACY_RUNTIME_SETTINGS_FILE_NAME),
            legacy_openrouter_credentials: app_data_dir
                .join(LEGACY_OPENROUTER_CREDENTIAL_FILE_NAME),
            openai_codex_auth: app_data_dir.join(LEGACY_OPENAI_CODEX_AUTH_STORE_FILE_NAME),
            notification_credentials: app_data_dir
                .join(LEGACY_NOTIFICATION_CREDENTIAL_STORE_FILE_NAME),
            dictation_settings: app_data_dir.join(LEGACY_DICTATION_SETTINGS_FILE_NAME),
            skill_sources: app_data_dir.join(LEGACY_SKILL_SOURCE_SETTINGS_FILE_NAME),
            mcp_registry: app_data_dir.join(LEGACY_MCP_REGISTRY_FILE_NAME),
            provider_model_catalog_cache: app_data_dir
                .join(LEGACY_PROVIDER_MODEL_CATALOG_CACHE_FILE_NAME),
            project_registry: app_data_dir.join(LEGACY_PROJECT_REGISTRY_FILE_NAME),
        }
    }
}

/// Runs every legacy-JSON importer once at app startup. Each importer is idempotent: it short-
/// circuits when its destination table already has rows, so re-running this function across
/// boots is safe.
///
/// Returns the first error encountered; importers run in the listed order. Phase 2.6 wires this
/// in `lib.rs::configure_builder_with_state` after the global DB has been opened.
pub fn run_legacy_json_imports(paths: &LegacyJsonImportPaths) -> Result<(), CommandError> {
    let mut connection = open_global_database(&paths.global_db)?;

    crate::provider_profiles::import_legacy_provider_profiles(
        &mut connection,
        &paths.provider_profiles,
        &paths.provider_profile_credentials,
        &paths.legacy_runtime_settings,
        &paths.legacy_openrouter_credentials,
        &paths.openai_codex_auth,
    )?;

    crate::auth::import_legacy_openai_codex_sessions(&connection, &paths.openai_codex_auth)?;

    crate::notifications::import_legacy_notification_credentials(
        &mut connection,
        &paths.notification_credentials,
    )?;

    import_legacy_dictation_settings(&connection, &paths.dictation_settings)?;
    import_legacy_skill_sources(&connection, &paths.skill_sources)?;
    import_legacy_mcp_registry(&connection, &paths.mcp_registry)?;
    import_legacy_provider_model_catalog_cache(&connection, &paths.provider_model_catalog_cache)?;

    drop(connection);

    crate::registry::import_legacy_project_registry(&paths.global_db, &paths.project_registry)?;

    Ok(())
}

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
            assert_eq!(
                count, 1,
                "expected table `{table}` to exist after migration"
            );
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
        assert_eq!(
            remaining, 0,
            "credentials should cascade-delete with profile"
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
    fn legacy_json_import_paths_resolve_uses_canonical_filenames() {
        let app_data_dir = Path::new("/var/lib/cadence");
        let paths = LegacyJsonImportPaths::resolve(app_data_dir);

        assert_eq!(paths.global_db, app_data_dir.join("cadence.db"));
        assert_eq!(
            paths.provider_profiles,
            app_data_dir.join("provider-profiles.json")
        );
        assert_eq!(
            paths.provider_profile_credentials,
            app_data_dir.join("provider-profile-credentials.json")
        );
        assert_eq!(
            paths.legacy_runtime_settings,
            app_data_dir.join("runtime-settings.json")
        );
        assert_eq!(
            paths.legacy_openrouter_credentials,
            app_data_dir.join("openrouter-credentials.json")
        );
        assert_eq!(
            paths.openai_codex_auth,
            app_data_dir.join("openai-auth.json")
        );
        assert_eq!(
            paths.notification_credentials,
            app_data_dir.join("notification-credentials.json")
        );
        assert_eq!(
            paths.dictation_settings,
            app_data_dir.join("dictation-settings.json")
        );
        assert_eq!(
            paths.skill_sources,
            app_data_dir.join("skill-sources.json")
        );
        assert_eq!(paths.mcp_registry, app_data_dir.join("mcp-registry.json"));
        assert_eq!(
            paths.provider_model_catalog_cache,
            app_data_dir.join("provider-model-catalogs.json")
        );
        assert_eq!(
            paths.project_registry,
            app_data_dir.join("project-registry.json")
        );
    }
}
