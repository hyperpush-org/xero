use std::path::{Path, PathBuf};

use cadence_desktop_lib::{
    commands::CommandResult,
    db::{self, database_path_for_repo, import_project},
    git::repository::CanonicalRepository,
    global_db::open_global_database,
    provider_profiles::{
        import_legacy_provider_profiles, load_provider_profiles_or_default,
        ProviderProfileCredentialLink, ProviderProfileReadinessProof,
        ProviderProfileReadinessStatus, ProviderProfilesSnapshot, ANTHROPIC_DEFAULT_PROFILE_ID,
        GITHUB_MODELS_DEFAULT_PROFILE_ID, OPENAI_CODEX_DEFAULT_PROFILE_ID,
        OPENROUTER_DEFAULT_PROFILE_ID, OPENROUTER_FALLBACK_MODEL_ID,
    },
    state::ImportFailpoints,
};

/// Test helper preserving the historical `load_or_migrate_provider_profiles_from_paths` shape
/// while routing through the Phase 2 importer + SQLite store. Each call uses an isolated DB so
/// the legacy fixtures fully drive the snapshot returned to the caller.
fn load_or_migrate_provider_profiles_from_paths(
    metadata_path: &Path,
    credentials_path: &Path,
    legacy_settings_path: &Path,
    legacy_openrouter_credentials_path: &Path,
    legacy_openai_auth_path: &Path,
) -> CommandResult<ProviderProfilesSnapshot> {
    let parent = metadata_path
        .parent()
        .expect("metadata path must have a parent dir for the global DB");
    let database_path = parent.join("cadence.db");
    let mut connection = open_global_database(&database_path)?;
    import_legacy_provider_profiles(
        &mut connection,
        metadata_path,
        credentials_path,
        legacy_settings_path,
        legacy_openrouter_credentials_path,
        legacy_openai_auth_path,
    )?;
    load_provider_profiles_or_default(&connection)
}

#[derive(Debug)]
struct TestPaths {
    provider_profiles_path: PathBuf,
    provider_profile_credentials_path: PathBuf,
    legacy_settings_path: PathBuf,
    legacy_openrouter_credentials_path: PathBuf,
    legacy_openai_auth_path: PathBuf,
}

fn create_paths(root: &tempfile::TempDir) -> TestPaths {
    TestPaths {
        provider_profiles_path: root.path().join("app-data").join("provider-profiles.json"),
        provider_profile_credentials_path: root
            .path()
            .join("app-data")
            .join("provider-profile-credentials.json"),
        legacy_settings_path: root.path().join("app-data").join("runtime-settings.json"),
        legacy_openrouter_credentials_path: root
            .path()
            .join("app-data")
            .join("openrouter-credentials.json"),
        legacy_openai_auth_path: root.path().join("app-data").join("openai-auth.json"),
    }
}

fn write_json(path: &Path, value: serde_json::Value) {
    let parent = path.parent().expect("parent");
    std::fs::create_dir_all(parent).expect("create parent");
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&value).expect("serialize json"),
    )
    .expect("write json file");
}

fn persist_openai_session(
    path: &Path,
    session_id: &str,
    account_id: &str,
    updated_at: &str,
    access_token: &str,
    refresh_token: &str,
) {
    // The provider-profiles importer reads legacy `openai-auth.json` directly (pre-Phase 2.2
    // format) so it can build a migration link before the auth importer copies rows into the
    // global SQLite database. Write the legacy JSON shape here to match what real users would
    // have on disk before upgrading.
    write_json(
        path,
        serde_json::json!({
            "openaiCodexSessions": {
                account_id: {
                    "providerId": "openai_codex",
                    "sessionId": session_id,
                    "accountId": account_id,
                    "accessToken": access_token,
                    "refreshToken": refresh_token,
                    "expiresAt": 4_102_444_800i64,
                    "updatedAt": updated_at,
                }
            },
            "updatedAt": updated_at,
        }),
    );
}

fn seed_repo_database(root: &tempfile::TempDir) -> PathBuf {
    let repo_root = root.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");
    let repository = CanonicalRepository {
        project_id: "project-1".into(),
        repository_id: "repo-1".into(),
        root_path: canonical_root.clone(),
        root_path_string: canonical_root.to_string_lossy().into_owned(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "repo".into(),
        branch_name: Some("main".into()),
        head_sha: Some("abc123".into()),
        branch: None,
        last_commit: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
        additions: 0,
        deletions: 0,
    };

    db::configure_project_database_paths(&root.path().join("app-data").join("cadence.db"));
    import_project(&repository, &ImportFailpoints::default()).expect("import repo into db");
    database_path_for_repo(&canonical_root)
}

fn read_sqlite_bytes(database_path: &Path) -> String {
    let mut bytes = Vec::new();
    for path in [
        database_path.to_path_buf(),
        database_path.with_extension("db-wal"),
        database_path.with_extension("db-shm"),
    ] {
        if path.exists() {
            bytes.extend(std::fs::read(path).expect("read sqlite sidecar bytes"));
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[test]
fn fresh_install_returns_in_memory_default_without_writing_files() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    let snapshot = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("load default provider profiles");

    assert_eq!(
        snapshot.metadata.active_profile_id,
        OPENAI_CODEX_DEFAULT_PROFILE_ID
    );
    assert_eq!(snapshot.metadata.profiles.len(), 1);
    assert_eq!(
        snapshot.metadata.profiles[0]
            .readiness(&snapshot.credentials)
            .status,
        ProviderProfileReadinessStatus::Missing
    );
    assert!(!paths.provider_profiles_path.exists());
    assert!(!paths.provider_profile_credentials_path.exists());
}

#[test]
fn openai_only_legacy_state_migrates_to_redacted_profile_store() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);
    persist_openai_session(
        &paths.legacy_openai_auth_path,
        "session-1",
        "acct-1",
        "2026-04-21T01:00:00Z",
        "access-token-openai",
        "refresh-token-openai",
    );

    let snapshot = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("migrate openai-only legacy state");

    assert_eq!(
        snapshot.metadata.active_profile_id,
        OPENAI_CODEX_DEFAULT_PROFILE_ID
    );
    let openai_profile = snapshot
        .profile(OPENAI_CODEX_DEFAULT_PROFILE_ID)
        .expect("openai profile");
    assert!(matches!(
        openai_profile.credential_link,
        Some(ProviderProfileCredentialLink::OpenAiCodex { .. })
    ));
    assert_eq!(
        openai_profile.readiness(&snapshot.credentials).status,
        ProviderProfileReadinessStatus::Ready
    );
    assert!(!paths.provider_profiles_path.exists());
    assert!(!paths.provider_profile_credentials_path.exists());
    assert!(paths.legacy_openai_auth_path.exists());
}

#[test]
fn openrouter_only_legacy_state_migrates_and_removes_legacy_files() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    write_json(
        &paths.legacy_settings_path,
        serde_json::json!({
            "providerId": "openrouter",
            "modelId": "openai/gpt-4.1-mini",
            "openrouterApiKeyConfigured": true,
            "updatedAt": "2026-04-21T01:00:00Z"
        }),
    );
    write_json(
        &paths.legacy_openrouter_credentials_path,
        serde_json::json!({
            "apiKey": "sk-or-v1-legacy-secret",
            "updatedAt": "2026-04-21T01:00:05Z"
        }),
    );

    let snapshot = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("migrate openrouter-only legacy state");

    assert_eq!(
        snapshot.metadata.active_profile_id,
        OPENROUTER_DEFAULT_PROFILE_ID
    );
    let openrouter_profile = snapshot
        .profile(OPENROUTER_DEFAULT_PROFILE_ID)
        .expect("openrouter profile");
    assert_eq!(openrouter_profile.model_id, "openai/gpt-4.1-mini");
    assert_eq!(
        openrouter_profile.readiness(&snapshot.credentials).status,
        ProviderProfileReadinessStatus::Ready
    );
    assert!(!paths.provider_profiles_path.exists());
    assert!(!paths.provider_profile_credentials_path.exists());
    assert!(!paths.legacy_settings_path.exists());
    assert!(!paths.legacy_openrouter_credentials_path.exists());
}

#[test]
fn both_providers_migrate_once_and_repo_local_sqlite_stays_secret_free() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);
    let database_path = seed_repo_database(&root);
    let database_before = read_sqlite_bytes(&database_path);

    write_json(
        &paths.legacy_settings_path,
        serde_json::json!({
            "providerId": "openai_codex",
            "modelId": "openai_codex",
            "openrouterApiKeyConfigured": true,
            "updatedAt": "2026-04-21T02:00:00Z"
        }),
    );
    write_json(
        &paths.legacy_openrouter_credentials_path,
        serde_json::json!({
            "apiKey": "sk-or-v1-both-secret",
            "updatedAt": "2026-04-21T02:00:05Z"
        }),
    );
    persist_openai_session(
        &paths.legacy_openai_auth_path,
        "session-both",
        "acct-both",
        "2026-04-21T02:00:10Z",
        "access-token-both",
        "refresh-token-both",
    );

    let first = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("migrate dual-provider legacy state");

    let second = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("reload migrated provider profiles");

    assert_eq!(first, second);
    assert_eq!(
        first.metadata.active_profile_id,
        OPENAI_CODEX_DEFAULT_PROFILE_ID
    );
    assert!(first.profile(OPENROUTER_DEFAULT_PROFILE_ID).is_some());
    assert_eq!(
        first
            .metadata
            .migration
            .as_ref()
            .and_then(|migration| migration.openrouter_model_inferred),
        Some(true)
    );
    assert_eq!(
        first
            .profile(OPENROUTER_DEFAULT_PROFILE_ID)
            .expect("openrouter profile")
            .model_id,
        OPENROUTER_FALLBACK_MODEL_ID
    );
    assert!(!paths.legacy_settings_path.exists());
    assert!(!paths.legacy_openrouter_credentials_path.exists());
    assert!(paths.legacy_openai_auth_path.exists());

    let database_after = read_sqlite_bytes(&database_path);
    assert_eq!(database_before, database_after);
    assert!(!database_after.contains("sk-or-v1-both-secret"));
    assert!(!database_after.contains("access-token-both"));
    assert!(!database_after.contains("refresh-token-both"));
}

#[test]
fn migration_keeps_legacy_files_when_global_database_open_fails() {
    let root = tempfile::tempdir().expect("temp dir");
    let blocked_parent = root.path().join("blocked-parent");
    std::fs::write(&blocked_parent, "not-a-directory").expect("create blocking file");

    // Point the metadata path at a directory that cannot be created, which forces the global
    // database open (and therefore the importer) to fail before any legacy file is touched.
    let provider_profiles_path = blocked_parent.join("provider-profiles.json");
    let provider_profile_credentials_path =
        blocked_parent.join("provider-profile-credentials.json");
    let legacy_settings_path = root.path().join("app-data").join("runtime-settings.json");
    let legacy_openrouter_credentials_path = root
        .path()
        .join("app-data")
        .join("openrouter-credentials.json");
    let legacy_openai_auth_path = root.path().join("app-data").join("openai-auth.json");

    write_json(
        &legacy_settings_path,
        serde_json::json!({
            "providerId": "openrouter",
            "modelId": "openai/gpt-4.1-mini",
            "openrouterApiKeyConfigured": true,
            "updatedAt": "2026-04-21T03:00:00Z"
        }),
    );
    write_json(
        &legacy_openrouter_credentials_path,
        serde_json::json!({
            "apiKey": "sk-or-v1-rollback-secret",
            "updatedAt": "2026-04-21T03:00:05Z"
        }),
    );

    let error = load_or_migrate_provider_profiles_from_paths(
        &provider_profiles_path,
        &provider_profile_credentials_path,
        &legacy_settings_path,
        &legacy_openrouter_credentials_path,
        &legacy_openai_auth_path,
    )
    .expect_err("global database open failure should abort the importer");

    assert_eq!(error.code, "global_database_dir_unavailable");
    assert!(!provider_profiles_path.exists());
    assert!(legacy_settings_path.exists());
    assert!(legacy_openrouter_credentials_path.exists());
}

#[test]
fn migration_rejects_blank_openai_account_or_session_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);
    write_json(
        &paths.legacy_openai_auth_path,
        serde_json::json!({
            "openaiCodexSessions": {
                "acct-1": {
                    "providerId": "openai_codex",
                    "sessionId": "   ",
                    "accountId": "acct-1",
                    "accessToken": "access-token-invalid",
                    "refreshToken": "refresh-token-invalid",
                    "expiresAt": 4102444800i64,
                    "updatedAt": "2026-04-21T04:00:00Z"
                }
            },
            "updatedAt": "2026-04-21T04:00:00Z"
        }),
    );

    let error = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect_err("blank migrated session id should fail closed");

    assert_eq!(
        error.code,
        "provider_profiles_migration_openai_link_invalid"
    );
    assert!(!paths.provider_profiles_path.exists());
    assert!(!paths.provider_profile_credentials_path.exists());
}

#[test]
fn anthropic_profile_store_keeps_api_keys_out_of_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);
    let secret = "sk-ant-api03-provider-profile";

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 2,
            "activeProfileId": ANTHROPIC_DEFAULT_PROFILE_ID,
            "profiles": [{
                "profileId": ANTHROPIC_DEFAULT_PROFILE_ID,
                "providerId": "anthropic",
                "runtimeKind": "anthropic",
                "label": "Anthropic",
                "modelId": "claude-3-5-sonnet-latest",
                "presetId": "anthropic",
                "credentialLink": {
                    "kind": "api_key",
                    "updated_at": "2026-04-21T05:00:00Z"
                },
                "updatedAt": "2026-04-21T05:00:00Z"
            }],
            "updatedAt": "2026-04-21T05:00:00Z"
        }),
    );
    write_json(
        &paths.provider_profile_credentials_path,
        serde_json::json!({
            "apiKeys": [{
                "profileId": ANTHROPIC_DEFAULT_PROFILE_ID,
                "apiKey": secret,
                "updatedAt": "2026-04-21T05:00:00Z"
            }]
        }),
    );

    let metadata_file =
        std::fs::read_to_string(&paths.provider_profiles_path).expect("read metadata file");
    assert!(!metadata_file.contains(secret));
    let credentials_file = std::fs::read_to_string(&paths.provider_profile_credentials_path)
        .expect("read credentials file");
    assert!(credentials_file.contains(secret));

    let snapshot = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("load anthropic provider profiles");

    let anthropic_profile = snapshot
        .profile(ANTHROPIC_DEFAULT_PROFILE_ID)
        .expect("anthropic profile");
    assert!(matches!(
        anthropic_profile.credential_link,
        Some(ProviderProfileCredentialLink::ApiKey { .. })
    ));
    assert_eq!(
        anthropic_profile.readiness(&snapshot.credentials).status,
        ProviderProfileReadinessStatus::Ready
    );
}

#[test]
fn github_models_profile_store_round_trips_fixed_openai_compatible_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);
    let secret = "github_pat_provider_profile_secret";

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 2,
            "activeProfileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
            "profiles": [{
                "profileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
                "providerId": "github_models",
                "runtimeKind": "openai_compatible",
                "label": "GitHub Models",
                "modelId": "openai/gpt-4.1",
                "presetId": "github_models",
                "credentialLink": {
                    "kind": "api_key",
                    "updated_at": "2026-04-21T05:20:00Z"
                },
                "updatedAt": "2026-04-21T05:20:00Z"
            }],
            "updatedAt": "2026-04-21T05:20:00Z"
        }),
    );
    write_json(
        &paths.provider_profile_credentials_path,
        serde_json::json!({
            "apiKeys": [{
                "profileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
                "apiKey": secret,
                "updatedAt": "2026-04-21T05:20:00Z"
            }]
        }),
    );

    let snapshot = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("load github models provider profiles");

    let github_profile = snapshot
        .profile(GITHUB_MODELS_DEFAULT_PROFILE_ID)
        .expect("github models profile");
    assert_eq!(github_profile.provider_id, "github_models");
    assert_eq!(github_profile.runtime_kind, "openai_compatible");
    assert_eq!(github_profile.preset_id.as_deref(), Some("github_models"));
    assert_eq!(github_profile.base_url, None);
    assert_eq!(github_profile.api_version, None);
    assert!(matches!(
        github_profile.credential_link,
        Some(ProviderProfileCredentialLink::ApiKey { .. })
    ));
    assert_eq!(
        github_profile.readiness(&snapshot.credentials).status,
        ProviderProfileReadinessStatus::Ready
    );
}

#[test]
fn github_models_profile_store_rejects_wrong_runtime_kind() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 2,
            "activeProfileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
            "profiles": [{
                "profileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
                "providerId": "github_models",
                "runtimeKind": "github_models",
                "label": "GitHub Models",
                "modelId": "openai/gpt-4.1",
                "presetId": "github_models",
                "updatedAt": "2026-04-21T05:21:00Z"
            }],
            "updatedAt": "2026-04-21T05:21:00Z"
        }),
    );

    let error = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect_err("github models runtime kind should fail closed");

    assert_eq!(error.code, "provider_profiles_invalid");
    assert!(error.message.contains("github_models"));
}

#[test]
fn github_models_profile_store_rejects_illegal_endpoint_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 2,
            "activeProfileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
            "profiles": [{
                "profileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
                "providerId": "github_models",
                "runtimeKind": "openai_compatible",
                "label": "GitHub Models",
                "modelId": "openai/gpt-4.1",
                "presetId": "github_models",
                "baseUrl": "https://example.invalid/v1",
                "updatedAt": "2026-04-21T05:22:00Z"
            }],
            "updatedAt": "2026-04-21T05:22:00Z"
        }),
    );

    let error = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect_err("github models baseUrl override should fail closed");

    assert_eq!(error.code, "provider_profiles_invalid");
    assert!(error.message.contains("field `baseUrl` is not allowed"));
}

#[test]
fn github_models_profile_store_rejects_api_version_override() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 2,
            "activeProfileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
            "profiles": [{
                "profileId": GITHUB_MODELS_DEFAULT_PROFILE_ID,
                "providerId": "github_models",
                "runtimeKind": "openai_compatible",
                "label": "GitHub Models",
                "modelId": "openai/gpt-4.1",
                "presetId": "github_models",
                "apiVersion": "2024-10-21",
                "updatedAt": "2026-04-21T05:23:00Z"
            }],
            "updatedAt": "2026-04-21T05:23:00Z"
        }),
    );

    let error = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect_err("github models apiVersion override should fail closed");

    assert_eq!(error.code, "provider_profiles_invalid");
    assert!(error.message.contains("field `apiVersion` is not allowed"));
}

#[test]
fn ollama_profile_store_accepts_local_readiness_without_secret_files() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 3,
            "activeProfileId": "ollama-default",
            "profiles": [{
                "profileId": "ollama-default",
                "providerId": "ollama",
                "runtimeKind": "openai_compatible",
                "label": "Ollama",
                "modelId": "llama3.2",
                "presetId": "ollama",
                "baseUrl": "http://127.0.0.1:11434/v1",
                "credentialLink": {
                    "kind": "local",
                    "updated_at": "2026-04-21T06:00:00Z"
                },
                "updatedAt": "2026-04-21T06:00:00Z"
            }],
            "updatedAt": "2026-04-21T06:00:00Z"
        }),
    );

    let snapshot = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("load ollama provider profiles");

    let profile = snapshot.profile("ollama-default").expect("ollama profile");
    assert!(matches!(
        profile.credential_link,
        Some(ProviderProfileCredentialLink::Local { .. })
    ));
    let readiness = profile.readiness(&snapshot.credentials);
    assert_eq!(readiness.status, ProviderProfileReadinessStatus::Ready);
    assert_eq!(readiness.proof, Some(ProviderProfileReadinessProof::Local));
    assert_eq!(
        readiness.proof_updated_at.as_deref(),
        Some("2026-04-21T06:00:00Z")
    );
    assert!(!paths.provider_profile_credentials_path.exists());
}

#[test]
fn bedrock_profile_store_accepts_ambient_readiness_without_secret_files() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 3,
            "activeProfileId": "bedrock-default",
            "profiles": [{
                "profileId": "bedrock-default",
                "providerId": "bedrock",
                "runtimeKind": "anthropic",
                "label": "Bedrock",
                "modelId": "anthropic.claude-3-7-sonnet-20250219-v1:0",
                "presetId": "bedrock",
                "region": "us-east-1",
                "credentialLink": {
                    "kind": "ambient",
                    "updated_at": "2026-04-21T06:10:00Z"
                },
                "updatedAt": "2026-04-21T06:10:00Z"
            }],
            "updatedAt": "2026-04-21T06:10:00Z"
        }),
    );

    let snapshot = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("load bedrock provider profiles");

    let profile = snapshot
        .profile("bedrock-default")
        .expect("bedrock profile");
    assert_eq!(profile.region.as_deref(), Some("us-east-1"));
    assert!(matches!(
        profile.credential_link,
        Some(ProviderProfileCredentialLink::Ambient { .. })
    ));
    let readiness = profile.readiness(&snapshot.credentials);
    assert_eq!(readiness.status, ProviderProfileReadinessStatus::Ready);
    assert_eq!(
        readiness.proof,
        Some(ProviderProfileReadinessProof::Ambient)
    );
    assert_eq!(
        readiness.proof_updated_at.as_deref(),
        Some("2026-04-21T06:10:00Z")
    );
    assert!(!paths.provider_profile_credentials_path.exists());
}

#[test]
fn vertex_profile_store_rejects_missing_project_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let paths = create_paths(&root);

    write_json(
        &paths.provider_profiles_path,
        serde_json::json!({
            "version": 3,
            "activeProfileId": "vertex-default",
            "profiles": [{
                "profileId": "vertex-default",
                "providerId": "vertex",
                "runtimeKind": "anthropic",
                "label": "Vertex",
                "modelId": "claude-3-7-sonnet@20250219",
                "presetId": "vertex",
                "region": "us-central1",
                "updatedAt": "2026-04-21T06:20:00Z"
            }],
            "updatedAt": "2026-04-21T06:20:00Z"
        }),
    );

    let error = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect_err("vertex project metadata should fail closed");

    assert_eq!(error.code, "provider_profiles_invalid");
    assert!(error.message.contains("projectId"));
}
