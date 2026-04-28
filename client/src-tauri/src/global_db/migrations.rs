use std::sync::LazyLock;

use rusqlite_migration::{Migrations, M};

/// Migrations for the global SQLite database (`cadence.db`).
///
/// This is the single source of truth for non-project-scoped state:
/// credentials, sessions, settings, registries, and the model catalog cache.
/// Phase 2 of the storage refactor ports each store onto these tables; until
/// then the schema is created but no code reads or writes it.
pub fn migrations() -> &'static Migrations<'static> {
    static MIGRATIONS: LazyLock<Migrations<'static>> = LazyLock::new(|| {
        Migrations::new(vec![
            M::up(INITIAL_SCHEMA_SQL),
            M::up(PROVIDER_CREDENTIALS_SCHEMA_SQL),
            M::up(DROP_LEGACY_PROVIDER_PROFILES_SQL),
        ])
    });
    &MIGRATIONS
}

const INITIAL_SCHEMA_SQL: &str = r#"
    CREATE TABLE IF NOT EXISTS provider_profiles (
        profile_id TEXT PRIMARY KEY,
        provider_id TEXT NOT NULL,
        runtime_kind TEXT NOT NULL DEFAULT '',
        label TEXT NOT NULL,
        model_id TEXT NOT NULL,
        preset_id TEXT,
        base_url TEXT,
        api_version TEXT,
        region TEXT,
        scope_project_id TEXT,
        credential_link_kind TEXT,
        credential_link_account_id TEXT,
        credential_link_session_id TEXT,
        credential_link_updated_at TEXT,
        migrated_from_legacy INTEGER NOT NULL DEFAULT 0
            CHECK (migrated_from_legacy IN (0, 1)),
        migrated_at TEXT,
        updated_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_provider_profiles_provider_id
        ON provider_profiles(provider_id);

    CREATE TABLE IF NOT EXISTS provider_profiles_metadata (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        active_profile_id TEXT NOT NULL
            REFERENCES provider_profiles(profile_id) ON DELETE RESTRICT,
        updated_at TEXT NOT NULL,
        migration_source TEXT,
        migration_migrated_at TEXT,
        migration_runtime_settings_updated_at TEXT,
        migration_openrouter_credentials_updated_at TEXT,
        migration_openai_auth_updated_at TEXT,
        migration_openrouter_model_inferred INTEGER
            CHECK (migration_openrouter_model_inferred IN (0, 1))
    );

    CREATE TABLE IF NOT EXISTS provider_profile_credentials (
        profile_id TEXT PRIMARY KEY
            REFERENCES provider_profiles(profile_id) ON DELETE CASCADE,
        api_key TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

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

/// Provider-credentials refactor (Phase 1): replace the `provider_profiles` /
/// `provider_profile_credentials` / `openai_codex_sessions` triplet with a flat
/// per-provider credential row. Old tables are left in place during this
/// transition so legacy readers (still wired to the profile concept) keep
/// working until Phase 3 ships the frontend rewrite. Phase 2.5 will drop them.
const PROVIDER_CREDENTIALS_SCHEMA_SQL: &str = r#"
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

    -- Backfill: pull every legacy provider_profiles row that has a credential
    -- linkage into the unified credential table. The legacy tables are kept
    -- intact so older readers continue to function during the transition.
    INSERT INTO provider_credentials (
        provider_id, kind, api_key,
        oauth_account_id, oauth_session_id,
        oauth_access_token, oauth_refresh_token, oauth_expires_at,
        base_url, api_version, region, scope_project_id,
        default_model_id, updated_at
    )
    SELECT
        pp.provider_id,
        CASE pp.credential_link_kind
            WHEN 'openai_codex' THEN 'oauth_session'
            WHEN 'api_key' THEN 'api_key'
            WHEN 'local' THEN 'local'
            WHEN 'ambient' THEN 'ambient'
            ELSE NULL
        END AS kind,
        CASE pp.credential_link_kind
            WHEN 'api_key' THEN
                (SELECT api_key FROM provider_profile_credentials
                  WHERE profile_id = pp.profile_id)
            ELSE NULL
        END AS api_key,
        pp.credential_link_account_id,
        pp.credential_link_session_id,
        NULL, NULL, NULL,
        pp.base_url, pp.api_version, pp.region, pp.scope_project_id,
        pp.model_id,
        COALESCE(pp.credential_link_updated_at, pp.updated_at)
    FROM provider_profiles AS pp
    WHERE pp.credential_link_kind IS NOT NULL
      AND (
            pp.credential_link_kind != 'api_key'
         OR EXISTS (
              SELECT 1 FROM provider_profile_credentials AS ppc
               WHERE ppc.profile_id = pp.profile_id
            )
      )
    ON CONFLICT(provider_id) DO NOTHING;

    -- Pull OAuth tokens out of openai_codex_sessions onto the unified row
    -- so that signing in keeps working without a separate sessions table read.
    UPDATE provider_credentials
       SET oauth_access_token = (
                SELECT access_token FROM openai_codex_sessions
                 WHERE account_id = provider_credentials.oauth_account_id
            ),
           oauth_refresh_token = (
                SELECT refresh_token FROM openai_codex_sessions
                 WHERE account_id = provider_credentials.oauth_account_id
            ),
           oauth_expires_at = (
                SELECT expires_at FROM openai_codex_sessions
                 WHERE account_id = provider_credentials.oauth_account_id
            )
     WHERE kind = 'oauth_session'
       AND oauth_account_id IS NOT NULL;
"#;

/// Provider-credentials cleanup: the flat `provider_credentials` table is now
/// the only source of truth, so the legacy profile tables can be removed after
/// the backfill migration has had a chance to run.
const DROP_LEGACY_PROVIDER_PROFILES_SQL: &str = r#"
    DROP TABLE IF EXISTS provider_profile_credentials;
    DROP TABLE IF EXISTS provider_profiles_metadata;
    DROP INDEX IF EXISTS idx_provider_profiles_provider_id;
    DROP TABLE IF EXISTS provider_profiles;
"#;
