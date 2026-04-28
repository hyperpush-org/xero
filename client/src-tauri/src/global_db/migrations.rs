use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

/// Migrations for the global SQLite database (`cadence.db`).
///
/// This is the single source of truth for non-project-scoped state:
/// credentials, sessions, settings, registries, and the model catalog cache.
/// The app is still pre-release, so the global database starts from a fresh
/// baseline instead of carrying compatibility migrations for removed schemas.
pub fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: LazyLock<Migrations<'static>> =
        LazyLock::new(|| Migrations::new(vec![M::up(INITIAL_SCHEMA_SQL)]));
    &MIGRATIONS
}

const INITIAL_SCHEMA_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS provider_credentials (
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
        updated_at               TEXT    NOT NULL,
        CHECK (
            (kind = 'api_key' AND api_key IS NOT NULL) OR
            (kind = 'oauth_session' AND oauth_account_id IS NOT NULL AND oauth_session_id IS NOT NULL) OR
            (kind IN ('local', 'ambient'))
        )
    );

    CREATE INDEX IF NOT EXISTS idx_provider_credentials_kind
        ON provider_credentials(kind);

    CREATE TABLE IF NOT EXISTS openai_codex_sessions (
        account_id TEXT PRIMARY KEY,
        provider_id TEXT NOT NULL,
        session_id TEXT NOT NULL,
        access_token TEXT NOT NULL,
        refresh_token TEXT NOT NULL,
        expires_at INTEGER NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_openai_codex_sessions_session_id
        ON openai_codex_sessions(session_id);

    CREATE TABLE IF NOT EXISTS notification_credentials (
        project_id TEXT NOT NULL,
        route_id TEXT NOT NULL,
        route_kind TEXT NOT NULL,
        bot_token TEXT,
        chat_id TEXT,
        webhook_url TEXT,
        updated_at TEXT NOT NULL
            DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        PRIMARY KEY (project_id, route_id)
    );

    CREATE INDEX IF NOT EXISTS idx_notification_credentials_project_id
        ON notification_credentials(project_id);

    CREATE TABLE IF NOT EXISTS notification_inbound_cursors (
        project_id TEXT NOT NULL,
        route_id TEXT NOT NULL,
        route_kind TEXT NOT NULL,
        cursor TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY (project_id, route_id)
    );

    CREATE TABLE IF NOT EXISTS runtime_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS dictation_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS skill_sources (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS mcp_registry (
        server_id TEXT PRIMARY KEY,
        payload TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS provider_model_catalog_cache (
        profile_id TEXT PRIMARY KEY,
        payload TEXT NOT NULL,
        fetched_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS projects (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        description TEXT NOT NULL DEFAULT '',
        milestone TEXT NOT NULL DEFAULT '',
        total_phases INTEGER NOT NULL DEFAULT 0 CHECK (total_phases >= 0),
        completed_phases INTEGER NOT NULL DEFAULT 0 CHECK (completed_phases >= 0),
        active_phase INTEGER NOT NULL DEFAULT 0 CHECK (active_phase >= 0),
        branch TEXT,
        runtime TEXT,
        created_at TEXT NOT NULL
            DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL
            DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE TABLE IF NOT EXISTS repositories (
        id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL
            REFERENCES projects(id) ON DELETE CASCADE,
        root_path TEXT NOT NULL UNIQUE,
        display_name TEXT NOT NULL,
        branch TEXT,
        head_sha TEXT,
        is_git_repo INTEGER NOT NULL DEFAULT 1 CHECK (is_git_repo IN (0, 1)),
        created_at TEXT NOT NULL
            DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL
            DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE INDEX IF NOT EXISTS idx_repositories_project_id
        ON repositories(project_id);
    CREATE INDEX IF NOT EXISTS idx_repositories_root_path
        ON repositories(root_path);
"#;
