use std::{path::PathBuf, time::Duration};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use cadence_desktop_lib::{
    auth::{persist_openai_codex_session, StoredOpenAiCodexSession},
    commands::{
        get_runtime_session::get_runtime_session, start_runtime_session::start_runtime_session,
        ProjectIdRequestDto, RuntimeAuthPhase,
    },
    configure_builder_with_state,
    db::{self, database_path_for_repo},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> (DesktopState, PathBuf, PathBuf) {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");
    (
        DesktopState::default()
            .with_registry_file_override(registry_path.clone())
            .with_auth_store_file_override(auth_store_path.clone()),
        registry_path,
        auth_store_path,
    )
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
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import project into repo-local db");

    let registry_path = app
        .state::<DesktopState>()
        .registry_file(&app.handle().clone())
        .expect("registry path");
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

#[test]
fn start_runtime_session_binds_latest_app_local_auth_without_tokens_in_repo_db() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_openai_codex_session(
        &auth_store_path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: "session-auth".into(),
            account_id: "acct-1".into(),
            access_token: jwt_with_account_id("acct-1"),
            refresh_token: "refresh-1".into(),
            expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
            updated_at: "2026-04-13T14:11:59Z".into(),
        },
    )
    .expect("persist auth session");

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
    assert!(!database_text.contains("refresh-1"));
    assert!(!database_text.contains("chatgpt_account_id"));
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
fn corrupted_runtime_rows_fail_with_typed_decode_errors() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_openai_codex_session(
        &auth_store_path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: "session-auth".into(),
            account_id: "acct-1".into(),
            access_token: jwt_with_account_id("acct-1"),
            refresh_token: "refresh-1".into(),
            expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
            updated_at: "2026-04-13T14:11:59Z".into(),
        },
    )
    .expect("persist auth session");

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

    let contents = std::fs::read_to_string(&registry_path).expect("read pruned registry");
    assert!(contents.contains("\"projects\": []"));
}

#[test]
fn start_runtime_session_does_not_create_durable_runtime_run_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    persist_openai_codex_session(
        &auth_store_path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: "session-auth".into(),
            account_id: "acct-1".into(),
            access_token: jwt_with_account_id("acct-1"),
            refresh_token: "refresh-1".into(),
            expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
            updated_at: "2026-04-13T14:11:59Z".into(),
        },
    )
    .expect("persist auth session");

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

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}
