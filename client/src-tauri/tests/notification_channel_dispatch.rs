use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use rusqlite::{params, Connection};
use tempfile::TempDir;
use xero_desktop_lib::{
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    notifications::{
        service::NotificationDispatchService, DiscordRouteCredentials, DiscordTransport,
        NotificationAdapterError, NotificationCredentialResolver, NotificationRouteKind,
        RouteCredentials, TelegramRouteCredentials, TelegramTransport,
        DISPATCH_ATTEMPTED_DIAGNOSTIC, DISPATCH_FAILED_DIAGNOSTIC,
    },
    state::DesktopState,
};

#[derive(Debug, Clone, Default)]
struct MockCredentialStore {
    entries: HashMap<(String, String), Result<RouteCredentials, NotificationAdapterError>>,
}

impl MockCredentialStore {
    fn with_route_credentials(
        mut self,
        project_id: &str,
        route_id: &str,
        credentials: RouteCredentials,
    ) -> Self {
        self.entries.insert(
            (project_id.to_string(), route_id.to_string()),
            Ok(credentials),
        );
        self
    }
}

impl NotificationCredentialResolver for MockCredentialStore {
    fn resolve_route_credentials(
        &self,
        project_id: &str,
        route_id: &str,
        _route_kind: NotificationRouteKind,
    ) -> Result<RouteCredentials, NotificationAdapterError> {
        self.entries
            .get(&(project_id.to_string(), route_id.to_string()))
            .cloned()
            .unwrap_or_else(|| {
                Err(NotificationAdapterError::credentials_missing(format!(
                    "No credentials for route `{route_id}` in project `{project_id}`."
                )))
            })
    }
}

#[derive(Debug, Clone, Default)]
struct MockTelegramTransport {
    responses: Arc<Mutex<HashMap<String, Result<(), NotificationAdapterError>>>>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl MockTelegramTransport {
    fn send_count(&self) -> usize {
        self.sent.lock().expect("telegram sent lock").len()
    }
}

impl TelegramTransport for MockTelegramTransport {
    fn send_message(
        &self,
        credentials: &TelegramRouteCredentials,
        message: &str,
    ) -> Result<(), NotificationAdapterError> {
        self.sent
            .lock()
            .expect("telegram sent lock")
            .push(message.to_string());

        self.responses
            .lock()
            .expect("telegram responses lock")
            .get(&credentials.chat_id)
            .cloned()
            .unwrap_or(Ok(()))
    }
}

#[derive(Debug, Clone, Default)]
struct MockDiscordTransport {
    responses: Arc<Mutex<HashMap<String, Result<(), NotificationAdapterError>>>>,
    sent: Arc<Mutex<Vec<String>>>,
}

impl MockDiscordTransport {
    fn with_response(
        self,
        webhook_url: &str,
        response: Result<(), NotificationAdapterError>,
    ) -> Self {
        self.responses
            .lock()
            .expect("discord responses lock")
            .insert(webhook_url.to_string(), response);
        self
    }

    fn send_count(&self) -> usize {
        self.sent.lock().expect("discord sent lock").len()
    }
}

impl DiscordTransport for MockDiscordTransport {
    fn send_message(
        &self,
        credentials: &DiscordRouteCredentials,
        message: &str,
    ) -> Result<(), NotificationAdapterError> {
        self.sent
            .lock()
            .expect("discord sent lock")
            .push(message.to_string());

        self.responses
            .lock()
            .expect("discord responses lock")
            .get(&credentials.webhook_url)
            .cloned()
            .unwrap_or(Ok(()))
    }
}

fn seed_project(root: &TempDir, project_id: &str, repo_name: &str) -> PathBuf {
    let repo_root = root.path().join(repo_name);
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");

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

    db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
    db::import_project(&repository, DesktopState::default().import_failpoints())
        .expect("import seeded project");

    canonical_root
}

fn upsert_route(
    repo_root: &Path,
    project_id: &str,
    route_id: &str,
    route_kind: &str,
    route_target: &str,
    enabled: bool,
) {
    project_store::upsert_notification_route(
        repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: route_id.into(),
            route_kind: route_kind.into(),
            route_target: route_target.into(),
            enabled,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T10:00:00Z".into(),
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
    enabled: bool,
) {
    let connection = open_connection(repo_root);
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
                if enabled { 1_i64 } else { 0_i64 },
                Some("{\"label\":\"ops\"}"),
                "2026-04-16T10:00:00Z",
            ],
        )
        .expect("insert raw notification route");
}

fn upsert_pending_approval(
    repo_root: &Path,
    project_id: &str,
    session_id: &str,
    created_at: &str,
) -> String {
    project_store::upsert_pending_operator_approval(
        repo_root,
        project_id,
        session_id,
        None,
        "terminal_input_required",
        "Terminal input required",
        "Runtime paused and requires a coarse operator answer.",
        created_at,
    )
    .expect("upsert pending operator approval")
    .action_id
}

fn enqueue_dispatches(repo_root: &Path, project_id: &str, action_id: &str, enqueued_at: &str) {
    project_store::enqueue_notification_dispatches(
        repo_root,
        &project_store::NotificationDispatchEnqueueRecord {
            project_id: project_id.into(),
            action_id: action_id.into(),
            enqueued_at: enqueued_at.into(),
        },
    )
    .expect("enqueue notification dispatches");
}

fn open_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open app-data sqlite connection")
}

#[test]
fn dispatch_worker_marks_sent_and_failed_outcomes_with_typed_diagnostics() {
    let root = tempfile::tempdir().expect("tempdir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-dispatch");

    upsert_route(
        &repo_root,
        project_id,
        "route-telegram",
        "telegram",
        "telegram:ops-room",
        true,
    );
    upsert_route(
        &repo_root,
        project_id,
        "route-discord",
        "discord",
        "discord:ops-room",
        true,
    );

    let action_id =
        upsert_pending_approval(&repo_root, project_id, "session-1", "2026-04-16T10:01:00Z");
    enqueue_dispatches(&repo_root, project_id, &action_id, "2026-04-16T10:01:05Z");

    let credentials = MockCredentialStore::default()
        .with_route_credentials(
            project_id,
            "route-telegram",
            RouteCredentials::Telegram(TelegramRouteCredentials {
                bot_token: "bot-token".into(),
                chat_id: "chat-1".into(),
            }),
        )
        .with_route_credentials(
            project_id,
            "route-discord",
            RouteCredentials::Discord(DiscordRouteCredentials {
                webhook_url: "https://discord.com/api/webhooks/1/2".into(),
                bot_token: None,
            }),
        );

    let telegram = MockTelegramTransport::default();
    let discord = MockDiscordTransport::default().with_response(
        "https://discord.com/api/webhooks/1/2",
        Err(NotificationAdapterError::transport_failed(
            "Discord fixture returned HTTP 502.",
        )),
    );

    let service = NotificationDispatchService::new(credentials, telegram, discord);
    let result = service
        .dispatch_pending_for_project(&repo_root, project_id)
        .expect("run dispatch worker once");

    assert_eq!(result.pending_count, 2);
    assert_eq!(result.attempted_count, 2);
    assert_eq!(result.sent_count, 1);
    assert_eq!(result.failed_count, 1);
    assert!(result
        .attempts
        .iter()
        .any(|attempt| attempt.diagnostic_code == DISPATCH_ATTEMPTED_DIAGNOSTIC));
    assert!(result
        .attempts
        .iter()
        .any(|attempt| attempt.diagnostic_code == DISPATCH_FAILED_DIAGNOSTIC));

    let dispatches =
        project_store::load_notification_dispatches(&repo_root, project_id, Some(&action_id))
            .expect("load dispatch rows after worker run");

    let by_route: HashMap<_, _> = dispatches
        .into_iter()
        .map(|dispatch| (dispatch.route_id.clone(), dispatch))
        .collect();

    let telegram_dispatch = by_route
        .get("route-telegram")
        .expect("telegram dispatch row");
    assert_eq!(
        telegram_dispatch.status,
        project_store::NotificationDispatchStatus::Sent
    );
    assert_eq!(telegram_dispatch.attempt_count, 1);
    assert!(telegram_dispatch.last_error_code.is_none());

    let discord_dispatch = by_route.get("route-discord").expect("discord dispatch row");
    assert_eq!(
        discord_dispatch.status,
        project_store::NotificationDispatchStatus::Failed
    );
    assert_eq!(discord_dispatch.attempt_count, 1);
    assert_eq!(
        discord_dispatch.last_error_code.as_deref(),
        Some("notification_adapter_transport_failed")
    );
}

#[test]
fn dispatch_worker_is_replay_idempotent_after_initial_outcome_persist() {
    let root = tempfile::tempdir().expect("tempdir");
    let project_id = "project-2";
    let repo_root = seed_project(&root, project_id, "repo-replay");

    upsert_route(
        &repo_root,
        project_id,
        "route-telegram",
        "telegram",
        "telegram:ops-room",
        true,
    );

    let action_id =
        upsert_pending_approval(&repo_root, project_id, "session-1", "2026-04-16T11:01:00Z");
    enqueue_dispatches(&repo_root, project_id, &action_id, "2026-04-16T11:01:05Z");

    let credentials = MockCredentialStore::default().with_route_credentials(
        project_id,
        "route-telegram",
        RouteCredentials::Telegram(TelegramRouteCredentials {
            bot_token: "bot-token".into(),
            chat_id: "chat-1".into(),
        }),
    );

    let telegram = MockTelegramTransport::default();
    let discord = MockDiscordTransport::default();
    let service = NotificationDispatchService::new(credentials, telegram.clone(), discord);

    let first = service
        .dispatch_pending_for_project(&repo_root, project_id)
        .expect("first dispatch cycle");
    let second = service
        .dispatch_pending_for_project(&repo_root, project_id)
        .expect("second dispatch cycle");

    assert_eq!(first.attempted_count, 1);
    assert_eq!(second.pending_count, 0);
    assert_eq!(second.attempted_count, 0);
    assert_eq!(telegram.send_count(), 1);

    let dispatches =
        project_store::load_notification_dispatches(&repo_root, project_id, Some(&action_id))
            .expect("load dispatch rows after replay");
    assert_eq!(dispatches.len(), 1);
    assert_eq!(dispatches[0].attempt_count, 1);
    assert_eq!(
        dispatches[0].status,
        project_store::NotificationDispatchStatus::Sent
    );
}

#[test]
fn pending_dispatch_query_prioritizes_oldest_pending_rows_over_recency_order() {
    let root = tempfile::tempdir().expect("tempdir");
    let project_id = "project-3";
    let repo_root = seed_project(&root, project_id, "repo-ordering");

    upsert_route(
        &repo_root,
        project_id,
        "route-discord",
        "discord",
        "discord:ops-room",
        true,
    );

    let old_action = upsert_pending_approval(
        &repo_root,
        project_id,
        "session-old",
        "2026-04-16T12:00:00Z",
    );
    enqueue_dispatches(&repo_root, project_id, &old_action, "2026-04-16T12:00:01Z");

    let new_action = upsert_pending_approval(
        &repo_root,
        project_id,
        "session-new",
        "2026-04-16T12:00:05Z",
    );
    enqueue_dispatches(&repo_root, project_id, &new_action, "2026-04-16T12:00:06Z");

    let recency_order = project_store::load_notification_dispatches(&repo_root, project_id, None)
        .expect("load recency-ordered dispatch rows");
    assert_eq!(recency_order[0].action_id, new_action);

    let pending_order =
        project_store::load_pending_notification_dispatches(&repo_root, project_id, Some(2))
            .expect("load pending-focused dispatch rows");

    assert_eq!(pending_order.len(), 2);
    assert_eq!(pending_order[0].action_id, old_action);
    assert_eq!(pending_order[1].action_id, new_action);
}

#[test]
fn upsert_notification_route_rejects_noncanonical_targets_before_durable_write() {
    let root = tempfile::tempdir().expect("tempdir");
    let project_id = "project-3b";
    let repo_root = seed_project(&root, project_id, "repo-route-contract");

    upsert_route(
        &repo_root,
        project_id,
        "route-discord",
        "discord",
        "discord:123456789012345678",
        true,
    );

    let expect_invalid = |route_kind: &str, route_target: &str| {
        let error = project_store::upsert_notification_route(
            &repo_root,
            &project_store::NotificationRouteUpsertRecord {
                project_id: project_id.into(),
                route_id: "route-discord".into(),
                route_kind: route_kind.into(),
                route_target: route_target.into(),
                enabled: true,
                metadata_json: Some("{\"label\":\"ops\"}".into()),
                updated_at: "2026-04-16T12:30:00Z".into(),
            },
        )
        .expect_err("non-canonical route target should fail closed");

        assert_eq!(error.code, "notification_route_request_invalid");
    };

    expect_invalid("discord", "123456789012345678");
    expect_invalid("discord", "telegram:123456789012345678");
    expect_invalid("discord", "discord:   ");
    expect_invalid("discord", "   ");

    let persisted_after_errors = project_store::load_notification_routes(&repo_root, project_id)
        .expect("load route rows after invalid upserts");
    assert_eq!(persisted_after_errors.len(), 1);
    assert_eq!(
        persisted_after_errors[0].route_target,
        "discord:123456789012345678"
    );

    let canonicalized = project_store::upsert_notification_route(
        &repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: "route-discord".into(),
            route_kind: " discord ".into(),
            route_target: " discord:123456789012345678 ".into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T12:31:00Z".into(),
        },
    )
    .expect("canonical route target should persist");

    assert_eq!(canonicalized.route_kind, "discord");
    assert_eq!(canonicalized.route_target, "discord:123456789012345678");

    let idempotent = project_store::upsert_notification_route(
        &repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: "route-discord".into(),
            route_kind: "discord".into(),
            route_target: canonicalized.route_target.clone(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T12:32:00Z".into(),
        },
    )
    .expect("canonical target upserts should be idempotent");

    assert_eq!(idempotent.route_target, "discord:123456789012345678");
}

#[test]
fn dispatch_worker_fails_closed_on_missing_credentials_and_malformed_route_contracts() {
    let root = tempfile::tempdir().expect("tempdir");
    let project_id = "project-4";
    let repo_root = seed_project(&root, project_id, "repo-fail-closed");

    upsert_route(
        &repo_root,
        project_id,
        "route-missing-creds",
        "discord",
        "discord:ops-room",
        true,
    );
    insert_raw_route(
        &repo_root,
        project_id,
        "route-malformed-target",
        "telegram",
        "telegram:",
        true,
    );
    insert_raw_route(
        &repo_root,
        project_id,
        "route-unknown-kind",
        "pagerduty",
        "pagerduty:ops-room",
        true,
    );

    let action_id =
        upsert_pending_approval(&repo_root, project_id, "session-1", "2026-04-16T13:00:00Z");
    enqueue_dispatches(&repo_root, project_id, &action_id, "2026-04-16T13:00:01Z");

    let credentials = MockCredentialStore::default().with_route_credentials(
        project_id,
        "route-malformed-target",
        RouteCredentials::Telegram(TelegramRouteCredentials {
            bot_token: "bot-token".into(),
            chat_id: "chat-1".into(),
        }),
    );

    let telegram = MockTelegramTransport::default();
    let discord = MockDiscordTransport::default();
    let service = NotificationDispatchService::new(credentials, telegram.clone(), discord.clone());

    let result = service
        .dispatch_pending_for_project(&repo_root, project_id)
        .expect("dispatch cycle should persist fail-closed outcomes");

    assert_eq!(result.pending_count, 3);
    assert_eq!(result.attempted_count, 3);
    assert_eq!(result.sent_count, 0);
    assert_eq!(result.failed_count, 3);
    assert_eq!(telegram.send_count(), 0);
    assert_eq!(discord.send_count(), 0);

    let dispatches =
        project_store::load_notification_dispatches(&repo_root, project_id, Some(&action_id))
            .expect("load dispatches after fail-closed cycle");
    assert_eq!(dispatches.len(), 3);
    assert!(dispatches.iter().all(|dispatch| {
        dispatch.status == project_store::NotificationDispatchStatus::Failed
            && dispatch.last_error_code.as_deref().is_some()
    }));
    assert!(dispatches.iter().any(|dispatch| {
        dispatch.route_id == "route-missing-creds"
            && dispatch.last_error_code.as_deref()
                == Some("notification_adapter_credentials_missing")
    }));
    assert!(dispatches.iter().any(|dispatch| {
        dispatch.route_id == "route-malformed-target"
            && dispatch.last_error_code.as_deref() == Some("notification_adapter_payload_invalid")
    }));
    assert!(dispatches.iter().any(|dispatch| {
        dispatch.route_id == "route-unknown-kind"
            && dispatch.last_error_code.as_deref() == Some("notification_adapter_payload_invalid")
    }));
}
