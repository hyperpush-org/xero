use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

/// Migrations for the global SQLite database (`xero.db`).
///
/// This is the single source of truth for non-project-scoped state:
/// credentials, sessions, settings, registries, and the model catalog cache.
/// The app is still pre-release, so the global database starts from a fresh
/// baseline instead of carrying compatibility migrations for removed schemas.
pub fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
        Migrations::new(vec![
            M::up(INITIAL_SCHEMA_SQL),
            M::up(BROWSER_CONTROL_SETTINGS_SQL),
            M::up(ENVIRONMENT_PROFILE_SQL),
            M::up(SOUL_SETTINGS_SQL),
            M::up(USER_ADDED_ENVIRONMENT_TOOLS_SQL),
            M::up(PROVIDER_PREFLIGHT_RESULTS_SQL),
            M::up(AGENT_TOOLING_SETTINGS_SQL),
            M::up(DEVELOPER_TOOL_SEQUENCES_SQL),
        ])
    });
    &MIGRATIONS
}

const DEVELOPER_TOOL_SEQUENCES_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS developer_tool_sequences (
        id TEXT PRIMARY KEY CHECK (id <> ''),
        name TEXT NOT NULL CHECK (name <> ''),
        payload TEXT NOT NULL CHECK (payload <> '' AND json_valid(payload)),
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    ) STRICT;

    CREATE INDEX IF NOT EXISTS idx_developer_tool_sequences_name
        ON developer_tool_sequences(name);
"#;

const AGENT_TOOLING_SETTINGS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS agent_tooling_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload TEXT NOT NULL CHECK (payload <> '' AND json_valid(payload)),
        updated_at TEXT NOT NULL
    ) STRICT;
"#;

const PROVIDER_PREFLIGHT_RESULTS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS provider_preflight_results (
        profile_id              TEXT NOT NULL CHECK (profile_id <> ''),
        provider_id             TEXT NOT NULL CHECK (provider_id <> ''),
        model_id                TEXT NOT NULL CHECK (model_id <> ''),
        source                  TEXT NOT NULL CHECK (source IN ('live_probe', 'live_catalog', 'cached_probe', 'static_manual', 'unavailable')),
        status                  TEXT NOT NULL CHECK (status IN ('passed', 'warning', 'failed', 'skipped')),
        checked_at              TEXT NOT NULL CHECK (checked_at <> ''),
        required_features_json  TEXT NOT NULL CHECK (required_features_json <> '' AND json_valid(required_features_json)),
        payload                 TEXT NOT NULL CHECK (payload <> '' AND json_valid(payload)),
        PRIMARY KEY (profile_id, provider_id, model_id)
    ) STRICT;

    CREATE INDEX IF NOT EXISTS idx_provider_preflight_results_checked
        ON provider_preflight_results(provider_id, model_id, checked_at DESC);
"#;

const USER_ADDED_ENVIRONMENT_TOOLS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS user_added_environment_tools (
        id              TEXT PRIMARY KEY CHECK (id <> ''),
        category        TEXT NOT NULL CHECK (category <> ''),
        command         TEXT NOT NULL CHECK (command <> ''),
        args_json       TEXT NOT NULL CHECK (args_json <> '' AND json_valid(args_json)),
        created_at      TEXT NOT NULL,
        updated_at      TEXT NOT NULL
    ) STRICT;
"#;

const SOUL_SETTINGS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS soul_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
"#;

const ENVIRONMENT_PROFILE_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS environment_profile (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        schema_version INTEGER NOT NULL CHECK (schema_version > 0),
        status TEXT NOT NULL CHECK (status IN ('pending', 'probing', 'ready', 'partial', 'failed')),
        os_kind TEXT NOT NULL CHECK (os_kind <> ''),
        os_version TEXT,
        arch TEXT NOT NULL CHECK (arch <> ''),
        default_shell TEXT,
        path_fingerprint TEXT,
        payload_json TEXT NOT NULL CHECK (payload_json <> '' AND json_valid(payload_json)),
        summary_json TEXT NOT NULL CHECK (summary_json <> '' AND json_valid(summary_json)),
        permission_requests_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(permission_requests_json)),
        diagnostics_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(diagnostics_json)),
        probe_started_at TEXT,
        probe_completed_at TEXT,
        refreshed_at TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
        updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
    );

    CREATE INDEX IF NOT EXISTS idx_environment_profile_refreshed_at
        ON environment_profile(refreshed_at);
"#;

const BROWSER_CONTROL_SETTINGS_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS browser_control_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );
"#;

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

    CREATE TABLE IF NOT EXISTS browser_control_settings (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS soul_settings (
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
