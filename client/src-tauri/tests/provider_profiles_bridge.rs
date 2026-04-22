use std::path::{Path, PathBuf};

use cadence_desktop_lib::{
    auth::{persist_openai_codex_session, StoredOpenAiCodexSession},
    db::{database_path_for_repo, import_project},
    git::repository::CanonicalRepository,
    provider_profiles::{
        load_or_migrate_provider_profiles_from_paths, ProviderProfileCredentialLink,
        ProviderProfileReadinessStatus, OPENAI_CODEX_DEFAULT_PROFILE_ID,
        OPENROUTER_DEFAULT_PROFILE_ID, OPENROUTER_FALLBACK_MODEL_ID,
    },
    state::ImportFailpoints,
};

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
    persist_openai_codex_session(
        path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: session_id.into(),
            account_id: account_id.into(),
            access_token: access_token.into(),
            refresh_token: refresh_token.into(),
            expires_at: 4_102_444_800,
            updated_at: updated_at.into(),
        },
    )
    .expect("persist openai session");
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
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

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
    assert!(paths.provider_profiles_path.exists());
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
    assert!(paths.provider_profiles_path.exists());
    assert!(paths.provider_profile_credentials_path.exists());
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
    let metadata_after_first =
        std::fs::read(&paths.provider_profiles_path).expect("read provider profile metadata bytes");

    let second = load_or_migrate_provider_profiles_from_paths(
        &paths.provider_profiles_path,
        &paths.provider_profile_credentials_path,
        &paths.legacy_settings_path,
        &paths.legacy_openrouter_credentials_path,
        &paths.legacy_openai_auth_path,
    )
    .expect("reload migrated provider profiles");
    let metadata_after_second =
        std::fs::read(&paths.provider_profiles_path).expect("read provider profile metadata bytes");

    assert_eq!(first, second);
    assert_eq!(metadata_after_first, metadata_after_second);
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
fn migration_rolls_back_new_store_and_keeps_legacy_files_when_credential_write_fails() {
    let root = tempfile::tempdir().expect("temp dir");
    let blocked_parent = root.path().join("blocked-parent");
    std::fs::write(&blocked_parent, "not-a-directory").expect("create blocking file");

    let provider_profiles_path = root.path().join("app-data").join("provider-profiles.json");
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
    .expect_err("credential write failure should roll back migration");

    assert_eq!(
        error.code,
        "provider_profile_credentials_directory_unavailable"
    );
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
