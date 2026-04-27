use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    thread,
    time::Duration,
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use cadence_desktop_lib::{
    auth::{
        load_openai_codex_session, now_timestamp, persist_openai_codex_session,
        remove_openai_codex_session, sync_openai_profile_link, AnthropicAuthConfig,
        OpenAiCodexAuthConfig, OpenRouterAuthConfig, StoredOpenAiCodexSession,
    },
    commands::{
        get_runtime_session::get_runtime_session,
        logout_runtime_session::logout_runtime_session,
        provider_profiles::{
            list_provider_profiles, logout_provider_profile, set_active_provider_profile,
            upsert_provider_profile,
        },
        start_openai_login::start_openai_login,
        start_runtime_session::start_runtime_session as start_runtime_session_command,
        upsert_runtime_settings::upsert_runtime_settings,
        CommandResult, LogoutProviderProfileRequestDto, ProjectIdRequestDto, RuntimeAuthPhase,
        RuntimeSessionDto, RuntimeUpdatedPayloadDto, SetActiveProviderProfileRequestDto,
        StartOpenAiLoginRequestDto, StartRuntimeSessionRequestDto, UpsertProviderProfileRequestDto,
        UpsertRuntimeSettingsRequestDto, RUNTIME_UPDATED_EVENT,
    },
    configure_builder_with_state,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::openai_codex_provider,
    state::DesktopState,
};
use serde_json::json;
use tauri::{Listener, Manager};
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn start_runtime_session<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: tauri::State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<RuntimeSessionDto> {
    start_runtime_session_command(
        app,
        state,
        StartRuntimeSessionRequestDto {
            project_id: request.project_id,
            provider_profile_id: None,
        },
    )
}

fn create_state(root: &TempDir) -> (DesktopState, PathBuf, PathBuf) {
    let app_data = root.path().join("app-data");
    std::fs::create_dir_all(&app_data).expect("create app-data dir");
    // Phase 2.7: every per-file override now funnels into a single global SQLite database. The
    // legacy registry/auth-store/provider-profile/runtime-settings paths share `cadence.db`
    // so writes through the legacy helpers stay visible to the runtime session reads.
    let global_db_path = app_data.join("cadence.db");
    (
        DesktopState::default()
            .with_global_db_path_override(global_db_path.clone()),
        global_db_path.clone(),
        global_db_path,
    )
}

#[derive(Clone, Default)]
struct EventRecorder {
    runtime_updates: Arc<Mutex<Vec<RuntimeUpdatedPayloadDto>>>,
}

impl EventRecorder {
    fn latest_runtime_update(&self) -> Option<RuntimeUpdatedPayloadDto> {
        self.runtime_updates
            .lock()
            .expect("runtime updates lock")
            .last()
            .cloned()
    }
}

fn attach_event_recorder(app: &tauri::App<tauri::test::MockRuntime>) -> EventRecorder {
    let recorder = EventRecorder::default();
    let runtime_updates = Arc::clone(&recorder.runtime_updates);
    app.listen(RUNTIME_UPDATED_EVENT, move |event| {
        let payload: RuntimeUpdatedPayloadDto = serde_json::from_str(event.payload())
            .expect("runtime updated payload should deserialize");
        runtime_updates
            .lock()
            .expect("runtime updates lock")
            .push(payload);
    });
    recorder
}

fn ambient_env_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn with_scoped_env<T>(entries: &[(&str, Option<&str>)], operation: impl FnOnce() -> T) -> T {
    let _guard = ambient_env_guard();
    let previous = entries
        .iter()
        .map(|(key, _)| ((*key).to_string(), std::env::var(key).ok()))
        .collect::<Vec<_>>();

    for (key, value) in entries {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }

    let result = operation();

    for (key, value) in previous {
        match value {
            Some(value) => std::env::set_var(&key, value),
            None => std::env::remove_var(&key),
        }
    }

    result
}

fn jwt_with_account_id(account_id: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(
        json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": account_id,
            }
        })
        .to_string(),
    );
    format!("{header}.{payload}.")
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();

    let repository = CanonicalRepository {
        project_id: "project-1".into(),
        repository_id: "repo-1".into(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
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

    let registry_path = app
        .state::<DesktopState>()
        .registry_file(&app.handle().clone())
        .expect("registry path");
    db::configure_project_database_paths(&registry_path);
    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import project into repo-local db");

    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: repository.project_id.clone(),
            repository_id: repository.repository_id.clone(),
            root_path: root_path_string,
        }],
    )
    .expect("persist registry entry");

    (repository.project_id, canonical_root)
}

fn persist_auth_session(
    auth_store_path: &Path,
    session_id: &str,
    account_id: &str,
    expires_at: i64,
    updated_at: &str,
) {
    persist_openai_codex_session(
        auth_store_path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: session_id.into(),
            account_id: account_id.into(),
            access_token: jwt_with_account_id(account_id),
            refresh_token: format!("refresh-{account_id}"),
            expires_at,
            updated_at: updated_at.into(),
        },
    )
    .expect("persist auth session");
}

fn seed_runtime_session_record(
    repo_root: &Path,
    project_id: &str,
    account_id: Option<&str>,
    session_id: Option<&str>,
    phase: RuntimeAuthPhase,
) {
    let provider = openai_codex_provider();
    project_store::upsert_runtime_session(
        repo_root,
        &project_store::RuntimeSessionRecord {
            project_id: project_id.into(),
            runtime_kind: provider.runtime_kind.into(),
            provider_id: provider.provider_id.into(),
            flow_id: None,
            session_id: session_id.map(str::to_owned),
            account_id: account_id.map(str::to_owned),
            auth_phase: phase,
            last_error: None,
            updated_at: now_timestamp(),
        },
    )
    .expect("seed runtime session record");
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

fn auth_config_with_token_url(token_url: String) -> OpenAiCodexAuthConfig {
    OpenAiCodexAuthConfig {
        token_url,
        callback_port: 0,
        originator: "Cadence-tests".into(),
        timeout: Duration::from_secs(5),
        ..OpenAiCodexAuthConfig::default()
    }
}

fn openrouter_auth_config(models_url: String) -> OpenRouterAuthConfig {
    OpenRouterAuthConfig {
        models_url,
        timeout: Duration::from_secs(5),
    }
}

fn anthropic_auth_config(models_url: String) -> AnthropicAuthConfig {
    AnthropicAuthConfig {
        models_url,
        anthropic_version: "2023-06-01".into(),
        timeout: Duration::from_secs(5),
    }
}

fn spawn_static_http_server(status: u16, body: &str) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test http server");
    let address = listener.local_addr().expect("test http server addr");
    let body = body.to_owned();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test http request");
        let mut reader = BufReader::new(stream.try_clone().expect("clone tcp stream"));
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line).expect("read request line");
            if bytes == 0 || line == "\r\n" {
                break;
            }
        }

        write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write test http response");
    });

    format!("http://{address}")
}

#[test]
fn start_runtime_session_binds_latest_app_local_auth_without_tokens_in_repo_db() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-auth",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime session");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.account_id.as_deref(), Some("acct-1"));
    assert_eq!(runtime.session_id.as_deref(), Some("session-auth"));
    assert!(runtime.last_error.is_none());

    let status = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime session");
    assert_eq!(status.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(status.account_id.as_deref(), Some("acct-1"));

    let database_path = database_path_for_repo(&repo_root);
    let database_bytes = std::fs::read(&database_path).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains("refresh-acct-1"));
    assert!(!database_text.contains("chatgpt_account_id"));
}

#[test]
fn runtime_session_bridge_rebinds_the_active_profile_link_over_stale_repo_account() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-explicit",
        "acct-explicit",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:58Z",
    );
    persist_auth_session(
        &auth_store_path,
        "session-latest",
        "acct-latest",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    seed_runtime_session_record(
        &repo_root,
        &project_id,
        Some("acct-explicit"),
        None,
        RuntimeAuthPhase::Idle,
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime session with migrated active profile linkage");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.account_id.as_deref(), Some("acct-latest"));
    assert_eq!(runtime.session_id.as_deref(), Some("session-latest"));
}

#[test]
fn start_runtime_session_returns_signed_out_state_when_no_auth_store_entry_exists() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime session should return signed-out state");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("auth_session_not_found")
    );
    assert!(runtime.session_id.is_none());
}

#[test]
fn runtime_session_bridge_profile_commands_expose_redacted_profile_state() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);

    let listed = list_provider_profiles(app.handle().clone(), app.state::<DesktopState>())
        .expect("list default provider profiles");
    assert_eq!(listed.active_profile_id, "openai_codex-default");
    assert_eq!(listed.profiles.len(), 1);
    assert_eq!(listed.profiles[0].profile_id, "openai_codex-default");
    assert!(!listed.profiles[0].readiness.ready);

    // The app now enforces one profile per provider, so add a second profile under a different
    // provider (openrouter) to exercise listing + activation across multiple profiles.
    let upserted = upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "zz-openrouter-alt".into(),
            provider_id: "openrouter".into(),
            runtime_kind: "openrouter".into(),
            label: "OpenRouter Alt".into(),
            model_id: "openai/gpt-4.1-mini".into(),
            preset_id: Some("openrouter".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("sk-or-secret".into()),
            activate: false,
        },
    )
    .expect("upsert redacted openrouter profile");
    assert_eq!(upserted.active_profile_id, "openai_codex-default");
    assert_eq!(upserted.profiles.len(), 2);
    assert!(upserted
        .profiles
        .iter()
        .all(|profile| profile.profile_id != "access-token-openai"));

    let switched = set_active_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SetActiveProviderProfileRequestDto {
            profile_id: "zz-openrouter-alt".into(),
        },
    )
    .expect("switch active provider profile");
    assert_eq!(switched.active_profile_id, "zz-openrouter-alt");
    assert!(
        switched
            .profiles
            .iter()
            .find(|profile| profile.profile_id == "zz-openrouter-alt")
            .expect("switched profile")
            .active
    );
}

#[test]
fn runtime_session_bridge_logout_provider_profile_clears_openai_oauth_link() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let stored_session = StoredOpenAiCodexSession {
        provider_id: "openai_codex".into(),
        session_id: "session-1".into(),
        account_id: "acct-1".into(),
        access_token: jwt_with_account_id("acct-1"),
        refresh_token: "refresh-1".into(),
        expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        updated_at: "2026-04-26T12:00:00Z".into(),
    };

    persist_openai_codex_session(&auth_store_path, stored_session.clone())
        .expect("persist OpenAI auth session");
    sync_openai_profile_link(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        Some("openai_codex-default"),
        Some(&stored_session),
    )
    .expect("sync provider profile link");

    let listed = list_provider_profiles(app.handle().clone(), app.state::<DesktopState>())
        .expect("list linked profile");
    assert!(listed.profiles[0].readiness.ready);

    let signed_out = logout_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        LogoutProviderProfileRequestDto {
            profile_id: "openai_codex-default".into(),
        },
    )
    .expect("sign out OpenAI provider profile");

    let profile = signed_out
        .profiles
        .iter()
        .find(|profile| profile.profile_id == "openai_codex-default")
        .expect("default OpenAI profile");
    assert!(!profile.readiness.ready);
    assert_eq!(
        profile.readiness.status,
        cadence_desktop_lib::commands::ProviderProfileReadinessStatusDto::Missing
    );
    assert!(profile.readiness.proof.is_none());
    assert!(profile.readiness.proof_updated_at.is_none());
    assert!(load_openai_codex_session(&auth_store_path, "acct-1")
        .expect("load auth session")
        .is_none());
}

// Removed: `runtime_session_bridge_reuses_global_openai_auth_when_active_openai_profile_changes`
// previously created two `openai_codex` provider profiles to assert auth reuse across active
// profile switches. The app now enforces one profile per provider, so the multi-profile premise
// no longer exists.

#[test]
fn runtime_session_bridge_profile_commands_reject_invalid_requests_and_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);

    let blank_profile_id = upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "   ".into(),
            provider_id: "openai_codex".into(),
            runtime_kind: "openai_codex".into(),
            label: "OpenAI".into(),
            model_id: "openai_codex".into(),
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: false,
        },
    )
    .expect_err("blank profile id should fail");
    assert_eq!(blank_profile_id.code, "invalid_request");

    let blank_label = upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "openai-alt".into(),
            provider_id: "openai_codex".into(),
            runtime_kind: "openai_codex".into(),
            label: "   ".into(),
            model_id: "openai_codex".into(),
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: false,
        },
    )
    .expect_err("blank label should fail");
    assert_eq!(blank_label.code, "invalid_request");

    let unknown_provider = upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "bogus".into(),
            provider_id: "azure_openai".into(),
            runtime_kind: "openai_compatible".into(),
            label: "Bogus".into(),
            model_id: "claude".into(),
            preset_id: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: false,
        },
    )
    .expect_err("unknown provider should fail");
    assert_eq!(unknown_provider.code, "provider_profiles_invalid");

    // Phase 2.7: provider profiles live in the global SQLite DB. Inject an invalid row directly
    // (blank credential link session id) so the loader validation rejects it the same way it used
    // to reject malformed JSON metadata.
    let global_db_path = app
        .state::<DesktopState>()
        .global_db_path(&app.handle().clone())
        .expect("global db path");
    let connection = cadence_desktop_lib::global_db::open_global_database(&global_db_path)
        .expect("open global database for invalid provider profile injection");
    connection
        .execute(
            "DELETE FROM provider_profiles_metadata WHERE id = 1",
            [],
        )
        .expect("clear provider profile metadata before reseeding");
    connection
        .execute("DELETE FROM provider_profiles", [])
        .expect("clear provider profiles before reseeding");
    connection
        .execute(
            "INSERT INTO provider_profiles (
                profile_id, provider_id, runtime_kind, label, model_id,
                credential_link_kind, credential_link_account_id,
                credential_link_session_id, credential_link_updated_at,
                migrated_from_legacy, migrated_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            rusqlite::params![
                "openai_codex-default",
                "openai_codex",
                "openai_codex",
                "OpenAI Codex",
                "openai_codex",
                "openai_codex",
                "acct-1",
                "   ",
                "2026-04-21T02:00:00Z",
                1,
                "2026-04-21T02:00:00Z",
                "2026-04-21T02:00:00Z",
            ],
        )
        .expect("insert invalid provider profile row");
    connection
        .execute(
            "INSERT INTO provider_profiles_metadata (
                id, active_profile_id, updated_at
            ) VALUES (1, ?1, ?2)",
            rusqlite::params!["openai_codex-default", "2026-04-21T02:00:00Z"],
        )
        .expect("insert provider profile metadata pointing to invalid row");

    let error = list_provider_profiles(app.handle().clone(), app.state::<DesktopState>())
        .expect_err("invalid provider metadata should fail closed");
    assert_eq!(error.code, "provider_profiles_invalid");
}

#[test]
fn start_runtime_session_returns_idle_diagnostic_when_auth_store_is_unreadable() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    // Phase 2.7: the auth store now lives inside the global SQLite database. Replace the
    // existing file with a directory so SQLite open fails the same way an unreadable JSON store
    // used to fail before the storage refactor.
    std::fs::remove_file(&auth_store_path).expect("remove existing global db file");
    if let Some(sidecar) = auth_store_path.parent() {
        for ext in ["db-wal", "db-shm"] {
            let _ = std::fs::remove_file(sidecar.join(format!(
                "{}.{ext}",
                auth_store_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("cadence")
            )));
        }
    }
    std::fs::create_dir_all(&auth_store_path).expect("create unreadable auth-store directory");

    let error = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect_err("start runtime session should fail closed when global DB is unreadable");

    assert_eq!(error.code, "global_database_open_failed");
}

#[test]
fn start_runtime_session_preserves_retryable_refresh_state_when_refresh_fails() {
    let token_base_url = spawn_static_http_server(500, "boom");
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let state = state.with_openai_auth_config_override(auth_config_with_token_url(format!(
        "{token_base_url}/oauth/token"
    )));
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-refresh",
        "acct-refresh",
        current_unix_timestamp() - Duration::from_secs(60).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );
    let before = load_openai_codex_session(&auth_store_path, "acct-refresh")
        .expect("seed session present")
        .expect("seed session row");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime session should surface retryable refresh failure");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Refreshing);
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("token_refresh_server_error")
    );
    assert_eq!(runtime.account_id.as_deref(), Some("acct-refresh"));
    assert!(runtime.session_id.is_none());

    let after = load_openai_codex_session(&auth_store_path, "acct-refresh")
        .expect("post-refresh session present")
        .expect("post-refresh session row");
    assert_eq!(
        before, after,
        "failed refresh should not rewrite stored tokens"
    );
}

#[test]
fn get_runtime_session_returns_idle_when_bound_auth_row_disappears() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-auth",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("seed runtime session");

    remove_openai_codex_session(&auth_store_path, "acct-1").expect("remove auth session");

    let runtime = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile missing auth row");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("auth_session_not_found")
    );
    assert!(runtime.session_id.is_none());
}

#[test]
fn get_runtime_session_repairs_authenticated_row_missing_account_id_from_global_auth() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-auth",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("seed runtime session");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute(
            "UPDATE runtime_sessions SET account_id = NULL WHERE project_id = ?1",
            [&project_id],
        )
        .expect("clear runtime account id");

    let runtime = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile missing runtime account id");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.account_id.as_deref(), Some("acct-1"));
    assert_eq!(runtime.session_id.as_deref(), Some("session-auth"));
    assert!(runtime.last_error.is_none());
}

#[test]
fn get_runtime_session_rebinds_authenticated_binding_to_new_global_session() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-auth",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("seed runtime session");

    persist_auth_session(
        &auth_store_path,
        "session-new",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:12:59Z",
    );

    let runtime = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale runtime binding");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.account_id.as_deref(), Some("acct-1"));
    assert_eq!(runtime.session_id.as_deref(), Some("session-new"));
    assert!(runtime.last_error.is_none());
}

#[test]
fn get_runtime_session_marks_transient_flow_failed_when_snapshot_missing_after_reload() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, registry_path, auth_store_path) = create_state(&root);
    let state = state.with_openai_auth_config_override(auth_config_with_token_url(
        "http://127.0.0.1:9/oauth/token".into(),
    ));
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    let started = start_openai_login(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartOpenAiLoginRequestDto {
            project_id: project_id.clone(),
            profile_id: "openai_codex-default".into(),
            originator: Some("Cadence-tests".into()),
        },
    )
    .expect("start login flow");
    assert!(started.flow_id.is_some());

    let reloaded = build_mock_app(
        DesktopState::default()
            .with_global_db_path_override(auth_store_path.clone())
            .with_registry_file_override(registry_path)
            .with_auth_store_file_override(auth_store_path)
            .with_openai_auth_config_override(auth_config_with_token_url(
                "http://127.0.0.1:9/oauth/token".into(),
            )),
    );

    let runtime = get_runtime_session(
        reloaded.handle().clone(),
        reloaded.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile missing in-memory flow");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Failed);
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("auth_flow_unavailable")
    );
    assert!(runtime.flow_id.is_none());
}

#[test]
fn logout_runtime_session_succeeds_when_backing_auth_row_is_already_gone() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-auth",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("seed runtime session");

    remove_openai_codex_session(&auth_store_path, "acct-1").expect("remove auth session");

    let runtime = logout_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("logout runtime session");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(runtime.account_id.as_deref(), Some("acct-1"));
    assert!(runtime.session_id.is_none());
    assert!(runtime.last_error.is_none());
}

#[test]
fn logout_runtime_session_projects_selected_openrouter_provider_from_settings() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorder(&app);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("sk-or-v1-openrouter-secret".into()),
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter runtime settings");

    let runtime = logout_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("logout runtime session with selected openrouter provider");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(runtime.runtime_kind, "openrouter");
    assert!(runtime.session_id.is_none());
    assert!(runtime.last_error.is_none());

    let event = recorder
        .latest_runtime_update()
        .expect("runtime update event should be emitted");
    assert_eq!(event.project_id, project_id);
    assert_eq!(event.provider_id, "openrouter");
    assert_eq!(event.runtime_kind, "openrouter");
    assert_eq!(event.auth_phase, RuntimeAuthPhase::Idle);
    assert!(event.session_id.is_none());
}

#[test]
fn start_runtime_session_binds_openrouter_from_global_settings_without_secret_leakage() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/gpt-4o-mini","supported_parameters":[]}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_openrouter_auth_config_override(openrouter_auth_config(format!(
        "{models_base_url}/api/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let secret = "sk-or-v1-openrouter-secret";

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some(secret.into()),
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter runtime settings");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openrouter runtime session");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(runtime.runtime_kind, "openrouter");
    assert!(runtime.session_id.is_some());
    assert!(runtime.account_id.is_some());
    assert!(runtime.last_error.is_none());

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));
}

#[test]
fn start_runtime_session_accepts_documented_openrouter_models_payload_shape() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"architecture":{"input_modalities":["text"],"modality":"text->text","output_modalities":["text"],"instruct_type":"chatml","tokenizer":"GPT"},"canonical_slug":"openai/gpt-4o-mini","context_length":128000,"created":1692901234,"id":"openai/gpt-4o-mini","links":{"details":"/api/v1/models/openai/gpt-4o-mini/endpoints"},"name":"GPT-4o mini","pricing":{"completion":"0.00006","prompt":"0.00003","image":"0","request":"0"},"supported_parameters":["temperature","top_p","max_tokens"],"top_provider":{"is_moderated":true,"context_length":128000,"max_completion_tokens":16384},"description":"Test payload matching the documented models response shape."}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_openrouter_auth_config_override(openrouter_auth_config(format!(
        "{models_base_url}/api/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("sk-or-v1-openrouter-secret".into()),
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter runtime settings");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openrouter runtime session against documented models payload");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(runtime.runtime_kind, "openrouter");
    assert!(runtime.session_id.is_some());
    assert!(runtime.account_id.is_some());
    assert!(runtime.last_error.is_none());
}

#[test]
fn start_runtime_session_returns_idle_when_openrouter_selected_without_api_key() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: None,
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter runtime settings without api key");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("surface missing openrouter key diagnostic");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(runtime.runtime_kind, "openrouter");
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("openrouter_api_key_missing")
    );
    assert!(runtime.session_id.is_none());
}

#[test]
fn start_runtime_session_maps_openrouter_network_failures_to_retryable_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_openrouter_auth_config_override(openrouter_auth_config(
        "http://127.0.0.1:9/api/v1/models".into(),
    ));
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("sk-or-v1-openrouter-secret".into()),
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter runtime settings");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("surface openrouter network diagnostic");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("openrouter_provider_unavailable")
    );
    assert_eq!(
        runtime
            .last_error
            .as_ref()
            .map(|diagnostic| diagnostic.retryable),
        Some(true)
    );
    assert!(runtime.session_id.is_none());
}

#[test]
fn start_runtime_session_rejects_malformed_openrouter_models_payload() {
    let models_base_url =
        spawn_static_http_server(200, r#"{"models":[{"name":"missing-data-and-id"}]}"#);
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_openrouter_auth_config_override(openrouter_auth_config(format!(
        "{models_base_url}/api/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("sk-or-v1-openrouter-secret".into()),
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter runtime settings");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("surface malformed openrouter payload diagnostic");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("openrouter_models_decode_failed")
    );
    assert_eq!(
        runtime
            .last_error
            .as_ref()
            .map(|diagnostic| diagnostic.retryable),
        Some(false)
    );
    assert!(runtime.session_id.is_none());
}

#[test]
fn start_runtime_session_maps_openrouter_validation_failures_to_typed_diagnostics() {
    let cases = [
        (401_u16, "openrouter_invalid_api_key", false),
        (402_u16, "openrouter_insufficient_credits", false),
        (429_u16, "openrouter_rate_limited", true),
        (503_u16, "openrouter_provider_unavailable", true),
    ];

    for (status, expected_code, expected_retryable) in cases {
        let models_base_url = spawn_static_http_server(status, "denied");
        let root = tempfile::tempdir().expect("temp dir");
        let (state, _registry_path, _auth_store_path) = create_state(&root);
        let state = state.with_openrouter_auth_config_override(openrouter_auth_config(format!(
            "{models_base_url}/api/v1/models"
        )));
        let app = build_mock_app(state);
        let (project_id, _repo_root) = seed_project(&root, &app);

        upsert_runtime_settings(
            app.handle().clone(),
            app.state::<DesktopState>(),
            UpsertRuntimeSettingsRequestDto {
                provider_id: "openrouter".into(),
                model_id: "openai/gpt-4o-mini".into(),
                openrouter_api_key: Some("sk-or-v1-openrouter-secret".into()),
                anthropic_api_key: None,
            },
        )
        .expect("save openrouter runtime settings");

        let runtime = start_runtime_session(
            app.handle().clone(),
            app.state::<DesktopState>(),
            ProjectIdRequestDto {
                project_id: project_id.clone(),
            },
        )
        .expect("surface openrouter diagnostic");

        assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
        assert_eq!(runtime.provider_id, "openrouter");
        assert_eq!(runtime.last_error_code.as_deref(), Some(expected_code));
        assert_eq!(
            runtime
                .last_error
                .as_ref()
                .map(|diagnostic| diagnostic.retryable),
            Some(expected_retryable)
        );
        assert!(runtime.session_id.is_none());
    }
}

#[test]
fn get_runtime_session_rejects_stale_openrouter_binding_after_key_rotation() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/gpt-4o-mini","supported_parameters":[]}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_openrouter_auth_config_override(openrouter_auth_config(format!(
        "{models_base_url}/api/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("sk-or-v1-first".into()),
            anthropic_api_key: None,
        },
    )
    .expect("save initial openrouter settings");

    let first = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind first openrouter runtime session");

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("sk-or-v1-second".into()),
            anthropic_api_key: None,
        },
    )
    .expect("rotate openrouter key");

    let reconciled = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale openrouter binding");

    assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
    assert_eq!(
        reconciled.last_error_code.as_deref(),
        Some("openrouter_binding_stale")
    );
    assert!(reconciled.session_id.is_none());

    let rebound_models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/gpt-4o-mini","supported_parameters":[]}]}"#,
    );
    let (rebound_state, _registry_path, _auth_store_path) = create_state(&root);
    let rebound_state = rebound_state.with_openrouter_auth_config_override(openrouter_auth_config(
        format!("{rebound_models_base_url}/api/v1/models"),
    ));
    let rebound_app = build_mock_app(rebound_state);
    let rebound = start_runtime_session(
        rebound_app.handle().clone(),
        rebound_app.state::<DesktopState>(),
        ProjectIdRequestDto { project_id },
    )
    .expect("rebind rotated openrouter key");

    assert_eq!(rebound.phase, RuntimeAuthPhase::Authenticated);
    assert_ne!(rebound.session_id, first.session_id);
    assert_ne!(rebound.account_id, first.account_id);
}

#[test]
fn get_runtime_session_rejects_blank_openrouter_session_id_as_stale_binding() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"openai/gpt-4o-mini","supported_parameters":[]}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_openrouter_auth_config_override(openrouter_auth_config(format!(
        "{models_base_url}/api/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some("sk-or-v1-first".into()),
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter settings");

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openrouter runtime session");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute(
            "UPDATE runtime_sessions SET session_id = '   ' WHERE project_id = ?1",
            [&project_id],
        )
        .expect("blank openrouter session id");

    let runtime = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile blank openrouter session id");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("openrouter_binding_stale")
    );
    assert!(runtime.session_id.is_none());
}

#[test]
fn start_runtime_session_binds_anthropic_from_provider_profiles_without_secret_leakage() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"claude-3-5-sonnet-latest","display_name":"Claude 3.5 Sonnet"}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_anthropic_auth_config_override(anthropic_auth_config(format!(
        "{models_base_url}/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let secret = "sk-ant-api03-test-secret";

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "anthropic-default".into(),
            provider_id: "anthropic".into(),
            runtime_kind: "anthropic".into(),
            label: "Anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            preset_id: Some("anthropic".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some(secret.into()),
            activate: true,
        },
    )
    .expect("save anthropic provider profile");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind anthropic runtime session");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "anthropic");
    assert_eq!(runtime.runtime_kind, "anthropic");
    assert!(runtime.session_id.is_some());
    assert!(runtime.account_id.is_some());
    assert!(runtime.last_error.is_none());

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));
}

#[test]
fn start_runtime_session_returns_idle_when_anthropic_selected_without_api_key() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "anthropic-default".into(),
            provider_id: "anthropic".into(),
            runtime_kind: "anthropic".into(),
            label: "Anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            preset_id: Some("anthropic".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: true,
        },
    )
    .expect("save anthropic provider profile without api key");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("surface missing anthropic key diagnostic");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
    assert_eq!(runtime.provider_id, "anthropic");
    assert_eq!(runtime.runtime_kind, "anthropic");
    assert_eq!(
        runtime.last_error_code.as_deref(),
        Some("anthropic_api_key_missing")
    );
    assert!(runtime.session_id.is_none());
}

#[test]
fn get_runtime_session_rejects_stale_anthropic_binding_after_key_rotation() {
    let models_base_url = spawn_static_http_server(
        200,
        r#"{"data":[{"id":"claude-3-5-sonnet-latest","display_name":"Claude 3.5 Sonnet"}]}"#,
    );
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_anthropic_auth_config_override(anthropic_auth_config(format!(
        "{models_base_url}/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "anthropic-default".into(),
            provider_id: "anthropic".into(),
            runtime_kind: "anthropic".into(),
            label: "Anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            preset_id: Some("anthropic".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("sk-ant-api03-first".into()),
            activate: true,
        },
    )
    .expect("save first anthropic provider profile");

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind first anthropic runtime session");

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "anthropic-default".into(),
            provider_id: "anthropic".into(),
            runtime_kind: "anthropic".into(),
            label: "Anthropic".into(),
            model_id: "claude-3-5-sonnet-latest".into(),
            preset_id: Some("anthropic".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("sk-ant-api03-second".into()),
            activate: true,
        },
    )
    .expect("rotate anthropic key");

    let reconciled = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale anthropic binding");

    assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
    assert_eq!(reconciled.provider_id, "anthropic");
    assert_eq!(
        reconciled.last_error_code.as_deref(),
        Some("anthropic_binding_stale")
    );
    assert!(reconciled.session_id.is_none());
}

#[test]
fn start_runtime_session_binds_bedrock_from_ambient_profile_without_secret_leakage() {
    with_scoped_env(
        &[
            ("AWS_ACCESS_KEY_ID", Some("test-bedrock-access-key")),
            ("AWS_SECRET_ACCESS_KEY", Some("test-bedrock-secret-key")),
            ("AWS_SESSION_TOKEN", None),
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, repo_root) = seed_project(&root, &app);

            upsert_provider_profile(
                app.handle().clone(),
                app.state::<DesktopState>(),
                UpsertProviderProfileRequestDto {
                    profile_id: "bedrock-default".into(),
                    provider_id: "bedrock".into(),
                    runtime_kind: "anthropic".into(),
                    label: "Bedrock".into(),
                    model_id: "anthropic.claude-3-7-sonnet-20250219-v1:0".into(),
                    preset_id: Some("bedrock".into()),
                    base_url: None,
                    api_version: None,
                    region: Some("us-east-1".into()),
                    project_id: None,
                    api_key: None,
                    activate: true,
                },
            )
            .expect("save bedrock provider profile");

            let runtime = start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("bind bedrock runtime session");

            assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
            assert_eq!(runtime.provider_id, "bedrock");
            assert_eq!(runtime.runtime_kind, "anthropic");
            assert!(runtime.session_id.is_some());
            assert!(runtime.account_id.is_some());
            assert!(runtime.last_error.is_none());

            let database_bytes =
                std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
            let database_text = String::from_utf8_lossy(&database_bytes);
            assert!(!database_text.contains("test-bedrock-secret-key"));
        },
    );
}

#[test]
fn start_runtime_session_binds_vertex_from_adc_profile_without_secret_flags() {
    let root = tempfile::tempdir().expect("temp dir");
    let adc_path = root.path().join("vertex-adc.json");
    std::fs::write(&adc_path, "{}\n").expect("write fake adc file");
    let adc_env = adc_path.to_string_lossy().into_owned();

    with_scoped_env(
        &[
            ("GOOGLE_APPLICATION_CREDENTIALS", Some(adc_env.as_str())),
            ("AWS_ACCESS_KEY_ID", None),
            ("AWS_SECRET_ACCESS_KEY", None),
        ],
        || {
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, _repo_root) = seed_project(&root, &app);

            upsert_provider_profile(
                app.handle().clone(),
                app.state::<DesktopState>(),
                UpsertProviderProfileRequestDto {
                    profile_id: "vertex-default".into(),
                    provider_id: "vertex".into(),
                    runtime_kind: "anthropic".into(),
                    label: "Vertex".into(),
                    model_id: "claude-3-7-sonnet@20250219".into(),
                    preset_id: Some("vertex".into()),
                    base_url: None,
                    api_version: None,
                    region: Some("us-central1".into()),
                    project_id: Some("vertex-project".into()),
                    api_key: None,
                    activate: true,
                },
            )
            .expect("save vertex provider profile");

            let runtime = start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("bind vertex runtime session");

            assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
            assert_eq!(runtime.provider_id, "vertex");
            assert_eq!(runtime.runtime_kind, "anthropic");
            assert!(runtime.session_id.is_some());
            assert!(runtime.account_id.is_some());
            assert!(runtime.last_error.is_none());
        },
    );
}

#[test]
fn start_runtime_session_surfaces_typed_vertex_adc_missing_diagnostic() {
    with_scoped_env(
        &[
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
            ("AWS_ACCESS_KEY_ID", None),
            ("AWS_SECRET_ACCESS_KEY", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, _repo_root) = seed_project(&root, &app);

            upsert_provider_profile(
                app.handle().clone(),
                app.state::<DesktopState>(),
                UpsertProviderProfileRequestDto {
                    profile_id: "vertex-default".into(),
                    provider_id: "vertex".into(),
                    runtime_kind: "anthropic".into(),
                    label: "Vertex".into(),
                    model_id: "claude-3-7-sonnet@20250219".into(),
                    preset_id: Some("vertex".into()),
                    base_url: None,
                    api_version: None,
                    region: Some("us-central1".into()),
                    project_id: Some("vertex-project".into()),
                    api_key: None,
                    activate: true,
                },
            )
            .expect("save vertex provider profile without adc");

            let runtime = start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("surface vertex adc diagnostic");

            assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
            assert_eq!(runtime.provider_id, "vertex");
            assert_eq!(runtime.runtime_kind, "anthropic");
            assert_eq!(
                runtime.last_error_code.as_deref(),
                Some("vertex_adc_missing")
            );
            assert!(runtime.session_id.is_none());
        },
    );
}

#[test]
fn get_runtime_session_rejects_stale_bedrock_binding_after_region_change() {
    with_scoped_env(
        &[
            ("AWS_ACCESS_KEY_ID", Some("test-bedrock-access-key")),
            ("AWS_SECRET_ACCESS_KEY", Some("test-bedrock-secret-key")),
            ("AWS_SESSION_TOKEN", None),
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, _repo_root) = seed_project(&root, &app);

            upsert_provider_profile(
                app.handle().clone(),
                app.state::<DesktopState>(),
                UpsertProviderProfileRequestDto {
                    profile_id: "bedrock-default".into(),
                    provider_id: "bedrock".into(),
                    runtime_kind: "anthropic".into(),
                    label: "Bedrock".into(),
                    model_id: "anthropic.claude-3-7-sonnet-20250219-v1:0".into(),
                    preset_id: Some("bedrock".into()),
                    base_url: None,
                    api_version: None,
                    region: Some("us-east-1".into()),
                    project_id: None,
                    api_key: None,
                    activate: true,
                },
            )
            .expect("save first bedrock provider profile");

            start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("bind first bedrock runtime session");

            upsert_provider_profile(
                app.handle().clone(),
                app.state::<DesktopState>(),
                UpsertProviderProfileRequestDto {
                    profile_id: "bedrock-default".into(),
                    provider_id: "bedrock".into(),
                    runtime_kind: "anthropic".into(),
                    label: "Bedrock".into(),
                    model_id: "anthropic.claude-3-7-sonnet-20250219-v1:0".into(),
                    preset_id: Some("bedrock".into()),
                    base_url: None,
                    api_version: None,
                    region: Some("us-west-2".into()),
                    project_id: None,
                    api_key: None,
                    activate: true,
                },
            )
            .expect("update bedrock region");

            let reconciled = get_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("reconcile stale bedrock binding");

            assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
            assert_eq!(reconciled.provider_id, "bedrock");
            assert_eq!(reconciled.runtime_kind, "anthropic");
            assert_eq!(
                reconciled.last_error_code.as_deref(),
                Some("bedrock_binding_stale")
            );
            assert!(reconciled.session_id.is_none());
        },
    );
}

#[test]
fn get_runtime_session_rejects_stale_vertex_binding_after_project_change() {
    let root = tempfile::tempdir().expect("temp dir");
    let adc_path = root.path().join("vertex-adc.json");
    std::fs::write(&adc_path, "{}\n").expect("write fake adc file");
    let adc_env = adc_path.to_string_lossy().into_owned();

    with_scoped_env(
        &[
            ("GOOGLE_APPLICATION_CREDENTIALS", Some(adc_env.as_str())),
            ("AWS_ACCESS_KEY_ID", None),
            ("AWS_SECRET_ACCESS_KEY", None),
        ],
        || {
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, _repo_root) = seed_project(&root, &app);

            upsert_provider_profile(
                app.handle().clone(),
                app.state::<DesktopState>(),
                UpsertProviderProfileRequestDto {
                    profile_id: "vertex-default".into(),
                    provider_id: "vertex".into(),
                    runtime_kind: "anthropic".into(),
                    label: "Vertex".into(),
                    model_id: "claude-3-7-sonnet@20250219".into(),
                    preset_id: Some("vertex".into()),
                    base_url: None,
                    api_version: None,
                    region: Some("us-central1".into()),
                    project_id: Some("vertex-project-a".into()),
                    api_key: None,
                    activate: true,
                },
            )
            .expect("save first vertex provider profile");

            start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("bind first vertex runtime session");

            upsert_provider_profile(
                app.handle().clone(),
                app.state::<DesktopState>(),
                UpsertProviderProfileRequestDto {
                    profile_id: "vertex-default".into(),
                    provider_id: "vertex".into(),
                    runtime_kind: "anthropic".into(),
                    label: "Vertex".into(),
                    model_id: "claude-3-7-sonnet@20250219".into(),
                    preset_id: Some("vertex".into()),
                    base_url: None,
                    api_version: None,
                    region: Some("us-central1".into()),
                    project_id: Some("vertex-project-b".into()),
                    api_key: None,
                    activate: true,
                },
            )
            .expect("update vertex project id");

            let reconciled = get_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("reconcile stale vertex binding");

            assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
            assert_eq!(reconciled.provider_id, "vertex");
            assert_eq!(reconciled.runtime_kind, "anthropic");
            assert_eq!(
                reconciled.last_error_code.as_deref(),
                Some("vertex_binding_stale")
            );
            assert!(reconciled.session_id.is_none());
        },
    );
}

#[test]
fn get_runtime_session_rejects_stale_openai_api_binding_after_key_rotation() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "openai-api-default".into(),
            provider_id: "openai_api".into(),
            runtime_kind: "openai_compatible".into(),
            label: "OpenAI API".into(),
            model_id: "gpt-4.1".into(),
            preset_id: Some("openai_api".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("sk-openai-first".into()),
            activate: true,
        },
    )
    .expect("save first openai api provider profile");

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind first openai api runtime session");

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "openai-api-default".into(),
            provider_id: "openai_api".into(),
            runtime_kind: "openai_compatible".into(),
            label: "OpenAI API".into(),
            model_id: "gpt-4.1".into(),
            preset_id: Some("openai_api".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("sk-openai-second".into()),
            activate: true,
        },
    )
    .expect("rotate openai api key");

    let reconciled = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale openai api binding");

    assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
    assert_eq!(reconciled.provider_id, "openai_api");
    assert_eq!(reconciled.runtime_kind, "openai_compatible");
    assert_eq!(
        reconciled.last_error_code.as_deref(),
        Some("openai_binding_stale")
    );
    assert!(reconciled.session_id.is_none());
}

#[test]
fn get_runtime_session_rejects_stale_azure_openai_binding_after_api_version_change() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "azure-openai-default".into(),
            provider_id: "azure_openai".into(),
            runtime_kind: "openai_compatible".into(),
            label: "Azure OpenAI".into(),
            model_id: "gpt-4o".into(),
            preset_id: Some("azure_openai".into()),
            base_url: Some("https://example.openai.azure.com/openai/deployments/gpt-4o".into()),
            api_version: Some("2024-10-21".into()),
            region: None,
            project_id: None,
            api_key: Some("azure-key-first".into()),
            activate: true,
        },
    )
    .expect("save first azure openai provider profile");

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind first azure openai runtime session");

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "azure-openai-default".into(),
            provider_id: "azure_openai".into(),
            runtime_kind: "openai_compatible".into(),
            label: "Azure OpenAI".into(),
            model_id: "gpt-4o".into(),
            preset_id: Some("azure_openai".into()),
            base_url: Some("https://example.openai.azure.com/openai/deployments/gpt-4o".into()),
            api_version: Some("2025-03-01-preview".into()),
            region: None,
            project_id: None,
            api_key: Some("azure-key-first".into()),
            activate: true,
        },
    )
    .expect("update azure openai api version");

    let reconciled = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale azure openai binding");

    assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
    assert_eq!(reconciled.provider_id, "azure_openai");
    assert_eq!(reconciled.runtime_kind, "openai_compatible");
    assert_eq!(
        reconciled.last_error_code.as_deref(),
        Some("azure_openai_binding_stale")
    );
    assert!(reconciled.session_id.is_none());
}

#[test]
fn get_runtime_session_rejects_stale_gemini_ai_studio_binding_after_model_change() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "gemini-default".into(),
            provider_id: "gemini_ai_studio".into(),
            runtime_kind: "gemini".into(),
            label: "Gemini AI Studio".into(),
            model_id: "gemini-2.5-flash".into(),
            preset_id: Some("gemini_ai_studio".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("gemini-key-first".into()),
            activate: true,
        },
    )
    .expect("save first gemini ai studio provider profile");

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind first gemini ai studio runtime session");

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "gemini-default".into(),
            provider_id: "gemini_ai_studio".into(),
            runtime_kind: "gemini".into(),
            label: "Gemini AI Studio".into(),
            model_id: "gemini-2.5-pro".into(),
            preset_id: Some("gemini_ai_studio".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("gemini-key-first".into()),
            activate: true,
        },
    )
    .expect("update gemini ai studio model");

    let reconciled = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale gemini ai studio binding");

    assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
    assert_eq!(reconciled.provider_id, "gemini_ai_studio");
    assert_eq!(reconciled.runtime_kind, "gemini");
    assert_eq!(
        reconciled.last_error_code.as_deref(),
        Some("gemini_ai_studio_binding_stale")
    );
    assert!(reconciled.session_id.is_none());
}

#[test]
fn start_runtime_session_binds_ollama_without_api_key_using_local_default_endpoint() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "ollama-default".into(),
            provider_id: "ollama".into(),
            runtime_kind: "openai_compatible".into(),
            label: "Ollama".into(),
            model_id: "llama3.2".into(),
            preset_id: Some("ollama".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: true,
        },
    )
    .expect("save ollama provider profile");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind ollama runtime session");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "ollama");
    assert_eq!(runtime.runtime_kind, "openai_compatible");
    assert!(runtime.session_id.is_some());
    assert!(runtime.account_id.is_some());
    assert!(runtime.last_error.is_none());
}

#[test]
fn get_runtime_session_rejects_stale_ollama_binding_after_endpoint_change() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "ollama-default".into(),
            provider_id: "ollama".into(),
            runtime_kind: "openai_compatible".into(),
            label: "Ollama".into(),
            model_id: "llama3.2".into(),
            preset_id: Some("ollama".into()),
            base_url: Some("http://127.0.0.1:11434/v1".into()),
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: true,
        },
    )
    .expect("save first ollama provider profile");

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind first ollama runtime session");

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "ollama-default".into(),
            provider_id: "ollama".into(),
            runtime_kind: "openai_compatible".into(),
            label: "Ollama".into(),
            model_id: "llama3.2".into(),
            preset_id: Some("ollama".into()),
            base_url: Some("http://127.0.0.1:22434/v1".into()),
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: true,
        },
    )
    .expect("update ollama endpoint");

    let reconciled = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale ollama binding");

    assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
    assert_eq!(reconciled.provider_id, "ollama");
    assert_eq!(
        reconciled.last_error_code.as_deref(),
        Some("ollama_binding_stale")
    );
    assert!(reconciled.session_id.is_none());
}

#[test]
fn start_runtime_session_binds_github_models_from_provider_profiles_without_secret_leakage() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let secret = "github_pat_runtime_session_secret";

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "github_models-default".into(),
            provider_id: "github_models".into(),
            runtime_kind: "openai_compatible".into(),
            label: "GitHub Models".into(),
            model_id: "openai/gpt-4.1".into(),
            preset_id: Some("github_models".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some(secret.into()),
            activate: true,
        },
    )
    .expect("save github models provider profile");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind github models runtime session");

    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "github_models");
    assert_eq!(runtime.runtime_kind, "openai_compatible");
    assert!(runtime.session_id.is_some());
    assert!(runtime.account_id.is_some());
    assert!(runtime.last_error.is_none());

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));
}

#[test]
fn get_runtime_session_rejects_stale_github_models_binding_after_token_rotation() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "github_models-default".into(),
            provider_id: "github_models".into(),
            runtime_kind: "openai_compatible".into(),
            label: "GitHub Models".into(),
            model_id: "openai/gpt-4.1".into(),
            preset_id: Some("github_models".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("github_pat_first".into()),
            activate: true,
        },
    )
    .expect("save first github models provider profile");

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind first github models runtime session");

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "github_models-default".into(),
            provider_id: "github_models".into(),
            runtime_kind: "openai_compatible".into(),
            label: "GitHub Models".into(),
            model_id: "openai/gpt-4.1".into(),
            preset_id: Some("github_models".into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            api_key: Some("github_pat_second".into()),
            activate: true,
        },
    )
    .expect("rotate github token");

    let reconciled = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reconcile stale github binding");

    assert_eq!(reconciled.phase, RuntimeAuthPhase::Idle);
    assert_eq!(reconciled.provider_id, "github_models");
    assert_eq!(
        reconciled.last_error_code.as_deref(),
        Some("github_models_binding_stale")
    );
    assert!(reconciled.session_id.is_none());
}

#[test]
fn start_runtime_session_rejects_empty_project_id() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);

    let error = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: "   ".into(),
        },
    )
    .expect_err("empty project id should be rejected");

    assert_eq!(error.code, "invalid_request");
}

#[test]
fn corrupted_runtime_rows_fail_with_typed_decode_errors() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-auth",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("seed runtime session");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute(
            "UPDATE runtime_sessions SET auth_phase = 'bogus_phase' WHERE project_id = ?1",
            [&project_id],
        )
        .expect("corrupt runtime phase");

    let error = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect_err("corrupted runtime row should fail");
    assert_eq!(error.code, "runtime_session_decode_failed");
}

#[test]
fn stale_registry_roots_are_pruned_before_runtime_lookup() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);

    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: "project-1".into(),
            repository_id: "repo-1".into(),
            root_path: root.path().join("missing-repo").display().to_string(),
        }],
    )
    .expect("write stale registry entry");

    let error = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: "project-1".into(),
        },
    )
    .expect_err("stale registry root should be pruned");
    assert_eq!(error.code, "project_not_found");

    let pruned = registry::read_registry(&registry_path).expect("read pruned registry");
    assert!(
        pruned.projects.is_empty(),
        "expected stale registry roots to be pruned, got {pruned:?}"
    );
}

#[test]
fn start_runtime_session_does_not_create_durable_runtime_run_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_auth_session(
        &auth_store_path,
        "session-auth",
        "acct-1",
        current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        "2026-04-13T14:11:59Z",
    );

    start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime session");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    let run_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runtime_runs", [], |row| row.get(0))
        .expect("count runtime runs");
    let checkpoint_count: i64 = connection
        .query_row("SELECT COUNT(*) FROM runtime_run_checkpoints", [], |row| {
            row.get(0)
        })
        .expect("count runtime checkpoints");

    assert_eq!(run_count, 0);
    assert_eq!(checkpoint_count, 0);
}
