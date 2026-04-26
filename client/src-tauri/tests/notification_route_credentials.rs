use std::{
    fs,
    path::{Path, PathBuf},
};

use cadence_desktop_lib::{
    commands::{
        list_notification_routes::list_notification_routes, ListNotificationRoutesRequestDto,
        NotificationRouteCredentialReadinessStatusDto,
    },
    configure_builder_with_state,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    notifications::{NotificationCredentialStoreEntry, NotificationCredentialStoreFile},
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};
use rusqlite::params;
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(root: &TempDir) -> tauri::App<tauri::test::MockRuntime> {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");
    let credential_store_path = root
        .path()
        .join("app-data")
        .join("notification-credentials.json");

    let state = DesktopState::default()
        .with_registry_file_override(registry_path)
        .with_auth_store_file_override(auth_store_path)
        .with_notification_credential_store_file_override(credential_store_path);

    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock tauri app")
}

fn seed_project(
    root: &TempDir,
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    repo_name: &str,
) -> PathBuf {
    let repo_root = root.path().join(repo_name);
    fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonicalize repo root");

    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: format!("repo-{project_id}"),
        root_path: canonical_root.clone(),
        root_path_string: canonical_root.to_string_lossy().into_owned(),
        common_git_dir: canonical_root.join(".git"),
        display_name: repo_name.into(),
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

    db::import_project(&repository, DesktopState::default().import_failpoints())
        .expect("import seeded project");

    let registry_path = app
        .state::<DesktopState>()
        .registry_file(app.handle())
        .expect("registry path");
    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: project_id.to_string(),
            repository_id: format!("repo-{project_id}"),
            root_path: canonical_root.to_string_lossy().into_owned(),
        }],
    )
    .expect("write registry record");

    canonical_root
}

fn upsert_route(
    repo_root: &Path,
    project_id: &str,
    route_id: &str,
    route_kind: &str,
    route_target: &str,
) {
    project_store::upsert_notification_route(
        repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: route_id.into(),
            route_kind: route_kind.into(),
            route_target: route_target.into(),
            enabled: true,
            metadata_json: Some("{\"channel\":\"ops\"}".into()),
            updated_at: "2026-04-17T09:00:00Z".into(),
        },
    )
    .expect("upsert notification route");
}

fn insert_raw_route(
    repo_root: &Path,
    project_id: &str,
    route_id: &str,
    route_kind: &str,
    route_target: &str,
) {
    let connection = rusqlite::Connection::open(database_path_for_repo(repo_root))
        .expect("open repo-local sqlite connection");

    connection
        .execute(
            r#"
            INSERT INTO notification_routes (
                project_id,
                route_id,
                route_kind,
                route_target,
                enabled,
                metadata_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
            "#,
            params![
                project_id,
                route_id,
                route_kind,
                route_target,
                1_i64,
                Some("{\"channel\":\"ops\"}"),
                "2026-04-17T09:00:00Z",
            ],
        )
        .expect("insert raw route row");
}

fn write_store_file(path: &Path, store: &NotificationCredentialStoreFile) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create app-data directory");
    }

    let json = serde_json::to_string_pretty(store).expect("serialize store");
    fs::write(path, json).expect("write store file");
}

#[test]
fn list_notification_routes_projects_redacted_readiness_for_present_and_partial_credentials() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(&root);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "Cadence");

    upsert_route(
        &repo_root,
        project_id,
        "telegram-primary",
        "telegram",
        "telegram:@ops-room",
    );
    upsert_route(
        &repo_root,
        project_id,
        "discord-fallback",
        "discord",
        "discord:1234567890",
    );

    let credential_store_path = app
        .state::<DesktopState>()
        .notification_credential_store_file(app.handle())
        .expect("credential store path");
    write_store_file(
        &credential_store_path,
        &NotificationCredentialStoreFile {
            routes: vec![
                NotificationCredentialStoreEntry {
                    project_id: project_id.into(),
                    route_id: "telegram-primary".into(),
                    route_kind: "telegram".into(),
                    bot_token: Some("telegram-secret-bot".into()),
                    chat_id: Some("123456789".into()),
                    webhook_url: None,
                },
                NotificationCredentialStoreEntry {
                    project_id: project_id.into(),
                    route_id: "discord-fallback".into(),
                    route_kind: "discord".into(),
                    bot_token: None,
                    chat_id: None,
                    webhook_url: Some("https://discord.com/api/webhooks/1/2".into()),
                },
            ],
            inbound_cursors: Vec::new(),
        },
    );

    let response = list_notification_routes(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListNotificationRoutesRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("list notification routes");

    assert_eq!(response.routes.len(), 2);

    let telegram = response
        .routes
        .iter()
        .find(|route| route.route_id == "telegram-primary")
        .expect("telegram route should be present");
    let telegram_readiness = telegram
        .credential_readiness
        .as_ref()
        .expect("telegram readiness should be projected");
    assert!(telegram_readiness.has_bot_token);
    assert!(telegram_readiness.has_chat_id);
    assert!(!telegram_readiness.has_webhook_url);
    assert!(telegram_readiness.ready);
    assert_eq!(
        telegram_readiness.status,
        NotificationRouteCredentialReadinessStatusDto::Ready
    );
    assert!(telegram_readiness.diagnostic.is_none());

    let discord = response
        .routes
        .iter()
        .find(|route| route.route_id == "discord-fallback")
        .expect("discord route should be present");
    let discord_readiness = discord
        .credential_readiness
        .as_ref()
        .expect("discord readiness should be projected");
    assert!(!discord_readiness.has_bot_token);
    assert!(!discord_readiness.has_chat_id);
    assert!(discord_readiness.has_webhook_url);
    assert!(!discord_readiness.ready);
    assert_eq!(
        discord_readiness.status,
        NotificationRouteCredentialReadinessStatusDto::Missing
    );
    assert_eq!(
        discord_readiness
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("notification_adapter_credentials_missing")
    );

    let serialized = serde_json::to_string(&response).expect("serialize response");
    assert!(!serialized.contains("telegram-secret-bot"));
    assert!(!serialized.contains("https://discord.com/api/webhooks/1/2"));
}

#[test]
fn list_notification_routes_marks_missing_store_as_fail_closed_with_typed_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(&root);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "Cadence");

    upsert_route(
        &repo_root,
        project_id,
        "telegram-primary",
        "telegram",
        "telegram:@ops-room",
    );

    let response = list_notification_routes(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListNotificationRoutesRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("list notification routes");

    assert_eq!(response.routes.len(), 1);

    let readiness = response.routes[0]
        .credential_readiness
        .as_ref()
        .expect("readiness should be projected");
    assert!(!readiness.has_bot_token);
    assert!(!readiness.has_chat_id);
    assert!(!readiness.has_webhook_url);
    assert!(!readiness.ready);
    assert_eq!(
        readiness.status,
        NotificationRouteCredentialReadinessStatusDto::Missing
    );
    assert_eq!(
        readiness
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("notification_adapter_credentials_missing")
    );
}

#[test]
fn list_notification_routes_marks_unreadable_store_as_unavailable_with_typed_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(&root);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "Cadence");

    upsert_route(
        &repo_root,
        project_id,
        "telegram-primary",
        "telegram",
        "telegram:@ops-room",
    );

    let credential_store_path = app
        .state::<DesktopState>()
        .notification_credential_store_file(app.handle())
        .expect("credential store path");
    fs::create_dir_all(&credential_store_path).expect("create unreadable credential store path");

    let response = list_notification_routes(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListNotificationRoutesRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("list notification routes");

    assert_eq!(response.routes.len(), 1);

    let readiness = response.routes[0]
        .credential_readiness
        .as_ref()
        .expect("readiness should be projected");
    assert!(!readiness.ready);
    assert_eq!(
        readiness.status,
        NotificationRouteCredentialReadinessStatusDto::Unavailable
    );
    assert_eq!(
        readiness
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("notification_adapter_credentials_read_failed")
    );
}

#[test]
fn list_notification_routes_marks_malformed_store_as_fail_closed_with_typed_diagnostics() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(&root);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "Cadence");

    upsert_route(
        &repo_root,
        project_id,
        "telegram-primary",
        "telegram",
        "telegram:@ops-room",
    );

    let credential_store_path = app
        .state::<DesktopState>()
        .notification_credential_store_file(app.handle())
        .expect("credential store path");
    if let Some(parent) = credential_store_path.parent() {
        fs::create_dir_all(parent).expect("create app-data directory");
    }
    fs::write(&credential_store_path, "{ malformed json")
        .expect("write malformed credential store");

    let response = list_notification_routes(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListNotificationRoutesRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("list notification routes");

    assert_eq!(response.routes.len(), 1);

    let readiness = response.routes[0]
        .credential_readiness
        .as_ref()
        .expect("readiness should be projected");
    assert!(!readiness.ready);
    assert_eq!(
        readiness.status,
        NotificationRouteCredentialReadinessStatusDto::Malformed
    );
    assert_eq!(
        readiness
            .diagnostic
            .as_ref()
            .map(|diagnostic| diagnostic.code.as_str()),
        Some("notification_adapter_credentials_malformed")
    );
}

#[test]
fn list_notification_routes_rejects_unsupported_persisted_route_kinds() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(&root);
    let project_id = "project-1";
    let repo_root = seed_project(&root, &app, project_id, "Cadence");

    insert_raw_route(
        &repo_root,
        project_id,
        "email-primary",
        "email",
        "email:ops@example.com",
    );

    let error = list_notification_routes(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListNotificationRoutesRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect_err("unsupported route kinds should fail closed");

    assert_eq!(error.code, "notification_route_decode_failed");
}
