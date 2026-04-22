use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use cadence_desktop_lib::{
    commands::{
        submit_notification_reply::submit_notification_reply,
        sync_notification_adapters::sync_notification_adapters_with_service,
        OperatorApprovalStatus, ResumeHistoryStatus,
    },
    configure_builder_with_state,
    db::{self, project_store},
    git::repository::CanonicalRepository,
    notifications::{
        service::NotificationDispatchService, DiscordRouteCredentials, DiscordTransport,
        NotificationAdapterError, NotificationCredentialResolver, NotificationInboundBatch,
        NotificationInboundMessage, NotificationRouteKind, RouteCredentials,
        TelegramRouteCredentials, TelegramTransport, REPLY_RECEIVED_DIAGNOSTIC,
        REPLY_REJECTED_DIAGNOSTIC,
    },
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};
use tauri::Manager;
use tempfile::TempDir;

type RouteKey = (String, String);
type CursorKey = (String, String, String);

#[derive(Clone, Default)]
struct MockCredentialStore {
    route_credentials: HashMap<RouteKey, RouteCredentials>,
    inbound_cursors: Arc<Mutex<HashMap<CursorKey, String>>>,
    fail_cursor_persist: Arc<Mutex<HashSet<CursorKey>>>,
}

impl MockCredentialStore {
    fn with_route_credentials(
        mut self,
        project_id: &str,
        route_id: &str,
        credentials: RouteCredentials,
    ) -> Self {
        self.route_credentials
            .insert((project_id.to_string(), route_id.to_string()), credentials);
        self
    }

    fn with_inbound_cursor(
        self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
        cursor: &str,
    ) -> Self {
        self.inbound_cursors.lock().expect("cursor lock").insert(
            (
                project_id.to_string(),
                route_id.to_string(),
                route_kind.as_str().to_string(),
            ),
            cursor.to_string(),
        );
        self
    }

    fn fail_cursor_persist_for_route(
        self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
    ) -> Self {
        self.fail_cursor_persist
            .lock()
            .expect("cursor persist lock")
            .insert((
                project_id.to_string(),
                route_id.to_string(),
                route_kind.as_str().to_string(),
            ));
        self
    }

    fn cursor_for(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
    ) -> Option<String> {
        self.inbound_cursors
            .lock()
            .expect("cursor lock")
            .get(&(
                project_id.to_string(),
                route_id.to_string(),
                route_kind.as_str().to_string(),
            ))
            .cloned()
    }
}

impl NotificationCredentialResolver for MockCredentialStore {
    fn resolve_route_credentials(
        &self,
        project_id: &str,
        route_id: &str,
        _route_kind: NotificationRouteKind,
    ) -> Result<RouteCredentials, NotificationAdapterError> {
        self.route_credentials
            .get(&(project_id.to_string(), route_id.to_string()))
            .cloned()
            .ok_or_else(|| {
                NotificationAdapterError::credentials_missing(format!(
                    "No mock credentials for route `{route_id}` in project `{project_id}`."
                ))
            })
    }

    fn load_inbound_cursor(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
    ) -> Result<Option<String>, NotificationAdapterError> {
        Ok(self.cursor_for(project_id, route_id, route_kind))
    }

    fn persist_inbound_cursor(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
        cursor: &str,
    ) -> Result<(), NotificationAdapterError> {
        let key = (
            project_id.to_string(),
            route_id.to_string(),
            route_kind.as_str().to_string(),
        );

        if self
            .fail_cursor_persist
            .lock()
            .expect("cursor persist lock")
            .contains(&key)
        {
            return Err(NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                "Simulated cursor persistence failure.",
                true,
            ));
        }

        self.inbound_cursors
            .lock()
            .expect("cursor lock")
            .insert(key, cursor.to_string());
        Ok(())
    }
}

#[derive(Clone, Default)]
struct MockTelegramTransport {
    reply_batches:
        Arc<Mutex<HashMap<String, Result<NotificationInboundBatch, NotificationAdapterError>>>>,
    requested_cursors: Arc<Mutex<Vec<Option<String>>>>,
}

impl MockTelegramTransport {
    fn with_reply_batch(
        self,
        chat_id: &str,
        batch: Result<NotificationInboundBatch, NotificationAdapterError>,
    ) -> Self {
        self.reply_batches
            .lock()
            .expect("telegram batch lock")
            .insert(chat_id.to_string(), batch);
        self
    }
}

impl TelegramTransport for MockTelegramTransport {
    fn send_message(
        &self,
        _credentials: &TelegramRouteCredentials,
        _message: &str,
    ) -> Result<(), NotificationAdapterError> {
        Ok(())
    }

    fn fetch_replies(
        &self,
        credentials: &TelegramRouteCredentials,
        cursor: Option<&str>,
    ) -> Result<NotificationInboundBatch, NotificationAdapterError> {
        self.requested_cursors
            .lock()
            .expect("telegram requested cursor lock")
            .push(cursor.map(ToString::to_string));

        self.reply_batches
            .lock()
            .expect("telegram batch lock")
            .get(&credentials.chat_id)
            .cloned()
            .unwrap_or_else(|| Ok(NotificationInboundBatch::default()))
    }
}

#[derive(Clone, Default)]
struct MockDiscordTransport {
    reply_batches:
        Arc<Mutex<HashMap<String, Result<NotificationInboundBatch, NotificationAdapterError>>>>,
    requested_cursors: Arc<Mutex<Vec<Option<String>>>>,
}

impl MockDiscordTransport {
    fn with_reply_batch(
        self,
        channel_target: &str,
        batch: Result<NotificationInboundBatch, NotificationAdapterError>,
    ) -> Self {
        self.reply_batches
            .lock()
            .expect("discord batch lock")
            .insert(channel_target.to_string(), batch);
        self
    }

    fn requested_cursors(&self) -> Vec<Option<String>> {
        self.requested_cursors
            .lock()
            .expect("discord requested cursor lock")
            .clone()
    }
}

impl DiscordTransport for MockDiscordTransport {
    fn send_message(
        &self,
        _credentials: &DiscordRouteCredentials,
        _message: &str,
    ) -> Result<(), NotificationAdapterError> {
        Ok(())
    }

    fn fetch_replies(
        &self,
        _credentials: &DiscordRouteCredentials,
        channel_target: &str,
        cursor: Option<&str>,
    ) -> Result<NotificationInboundBatch, NotificationAdapterError> {
        self.requested_cursors
            .lock()
            .expect("discord requested cursor lock")
            .push(cursor.map(ToString::to_string));

        self.reply_batches
            .lock()
            .expect("discord batch lock")
            .get(channel_target)
            .cloned()
            .unwrap_or_else(|| Ok(NotificationInboundBatch::default()))
    }
}

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    DesktopState::default().with_registry_file_override(registry_path)
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

fn upsert_route(
    repo_root: &std::path::Path,
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
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-17T03:00:00Z".into(),
        },
    )
    .expect("upsert notification route");
}

fn upsert_pending_approval(
    repo_root: &std::path::Path,
    project_id: &str,
    session_id: &str,
    created_at: &str,
) -> String {
    project_store::upsert_pending_operator_approval(
        repo_root,
        project_id,
        session_id,
        Some("flow-1"),
        "terminal_input_required",
        "Terminal input required",
        "Runtime paused and requires operator input.",
        created_at,
    )
    .expect("upsert pending approval")
    .action_id
}

#[test]
fn inbound_reply_ingestion_preserves_cross_channel_first_wins_resume_invariants() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:tg-room",
    );
    upsert_route(
        &repo_root,
        &project_id,
        "route-discord",
        "discord",
        "discord:998877665544332211",
    );

    let action_id =
        upsert_pending_approval(&repo_root, &project_id, "session-1", "2026-04-17T03:01:00Z");

    let dispatches =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load dispatches for action");
    assert_eq!(dispatches.len(), 2);

    let telegram_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-telegram")
        .expect("telegram dispatch row");
    let discord_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-discord")
        .expect("discord dispatch row");

    let credential_store = MockCredentialStore::default()
        .with_route_credentials(
            &project_id,
            "route-telegram",
            RouteCredentials::Telegram(TelegramRouteCredentials {
                bot_token: "telegram-bot-token".into(),
                chat_id: "tg-room".into(),
            }),
        )
        .with_route_credentials(
            &project_id,
            "route-discord",
            RouteCredentials::Discord(DiscordRouteCredentials {
                webhook_url: "https://discord.com/api/webhooks/1/2".into(),
                bot_token: Some("discord-bot-token".into()),
            }),
        );

    let telegram_transport = MockTelegramTransport::default().with_reply_batch(
        "tg-room",
        Ok(NotificationInboundBatch {
            messages: vec![NotificationInboundMessage {
                message_id: "1001".into(),
                responder_id: Some("telegram-user".into()),
                received_at: "2026-04-17T03:01:05Z".into(),
                body: format!(
                    "approve {} Telegram answer wins if first.",
                    telegram_dispatch.correlation_key
                ),
                context_action_id: Some(action_id.clone()),
            }],
            next_cursor: Some("1002".into()),
        }),
    );

    let discord_transport = MockDiscordTransport::default().with_reply_batch(
        "998877665544332211",
        Ok(NotificationInboundBatch {
            messages: vec![NotificationInboundMessage {
                message_id: "2001".into(),
                responder_id: Some("discord-user".into()),
                received_at: "2026-04-17T03:01:06Z".into(),
                body: format!(
                    "approve {} Discord answer arrives in same cycle.",
                    discord_dispatch.correlation_key
                ),
                context_action_id: Some(action_id.clone()),
            }],
            next_cursor: Some("2002".into()),
        }),
    );

    let service = NotificationDispatchService::new(
        credential_store.clone(),
        telegram_transport,
        discord_transport,
    );

    let result = service
        .ingest_replies_for_project(app.handle().clone(), &repo_root, &project_id)
        .expect("ingest inbound replies");

    assert_eq!(result.route_count, 2);
    assert_eq!(result.polled_route_count, 2);
    assert_eq!(result.message_count, 2);
    assert_eq!(result.accepted_count, 1);
    assert_eq!(result.rejected_count, 1);
    assert!(result
        .attempts
        .iter()
        .any(|attempt| attempt.accepted && attempt.diagnostic_code == REPLY_RECEIVED_DIAGNOSTIC));
    assert!(result.attempts.iter().any(|attempt| {
        !attempt.accepted
            && attempt.diagnostic_code == REPLY_REJECTED_DIAGNOSTIC
            && attempt.reply_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load reply claims");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| { claim.status == project_store::NotificationReplyClaimStatus::Accepted }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let snapshot = project_store::load_project_snapshot(&repo_root, &project_id)
        .expect("load project snapshot")
        .snapshot;
    let approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| approval.action_id == action_id)
        .expect("approval request for action");
    assert_eq!(approval.status, OperatorApprovalStatus::Approved);
    let started_resume_entries = snapshot
        .resume_history
        .iter()
        .filter(|entry| {
            entry.source_action_id.as_deref() == Some(action_id.as_str())
                && entry.status == ResumeHistoryStatus::Started
        })
        .count();
    assert_eq!(started_resume_entries, 1);

    assert_eq!(
        credential_store
            .cursor_for(
                &project_id,
                "route-telegram",
                NotificationRouteKind::Telegram
            )
            .as_deref(),
        Some("1002")
    );
    assert_eq!(
        credential_store
            .cursor_for(&project_id, "route-discord", NotificationRouteKind::Discord)
            .as_deref(),
        Some("2002")
    );
}

#[test]
fn inbound_reply_ingestion_rejects_forged_and_malformed_payloads_with_typed_codes() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:tg-room",
    );

    let action_id =
        upsert_pending_approval(&repo_root, &project_id, "session-2", "2026-04-17T03:10:00Z");
    let dispatch =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load dispatches")
            .into_iter()
            .next()
            .expect("dispatch row");

    let forged_correlation = if dispatch.correlation_key == "nfy:ffffffffffffffffffffffffffffffff" {
        "nfy:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string()
    } else {
        "nfy:ffffffffffffffffffffffffffffffff".to_string()
    };

    let credential_store = MockCredentialStore::default().with_route_credentials(
        &project_id,
        "route-telegram",
        RouteCredentials::Telegram(TelegramRouteCredentials {
            bot_token: "telegram-bot-token".into(),
            chat_id: "tg-room".into(),
        }),
    );

    let telegram_transport = MockTelegramTransport::default().with_reply_batch(
        "tg-room",
        Ok(NotificationInboundBatch {
            messages: vec![
                NotificationInboundMessage {
                    message_id: "3001".into(),
                    responder_id: Some("forger".into()),
                    received_at: "2026-04-17T03:10:05Z".into(),
                    body: format!(
                        "approve {} forged correlation should fail.",
                        forged_correlation
                    ),
                    context_action_id: Some(action_id.clone()),
                },
                NotificationInboundMessage {
                    message_id: "3002".into(),
                    responder_id: Some("operator".into()),
                    received_at: "2026-04-17T03:10:06Z".into(),
                    body: "approve only-two-tokens".into(),
                    context_action_id: Some(action_id.clone()),
                },
                NotificationInboundMessage {
                    message_id: "3003".into(),
                    responder_id: Some("operator".into()),
                    received_at: "2026-04-17T03:10:07Z".into(),
                    body: format!("maybe {} unsupported decision", dispatch.correlation_key),
                    context_action_id: Some(action_id.clone()),
                },
                NotificationInboundMessage {
                    message_id: "3004".into(),
                    responder_id: Some("operator".into()),
                    received_at: "2026-04-17T03:10:08Z".into(),
                    body: format!("approve {}   ", dispatch.correlation_key),
                    context_action_id: Some(action_id.clone()),
                },
            ],
            next_cursor: Some("3005".into()),
        }),
    );

    let service = NotificationDispatchService::new(
        credential_store,
        telegram_transport,
        MockDiscordTransport::default(),
    );

    let result = service
        .ingest_replies_for_project(app.handle().clone(), &repo_root, &project_id)
        .expect("ingest malformed replies");

    assert_eq!(result.message_count, 4);
    assert_eq!(result.accepted_count, 0);
    assert_eq!(result.rejected_count, 4);
    assert!(result.attempts.iter().any(|attempt| {
        attempt.reply_code.as_deref() == Some("notification_reply_correlation_invalid")
    }));
    assert!(result.attempts.iter().any(|attempt| {
        attempt.reply_code.as_deref() == Some("notification_reply_request_invalid")
    }));
    assert!(result.attempts.iter().any(|attempt| {
        attempt.reply_code.as_deref() == Some("notification_reply_decision_unsupported")
    }));

    let claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load reply claims after malformed ingestion");
    assert_eq!(claims.len(), 1);
    assert_eq!(
        claims[0].rejection_code.as_deref(),
        Some("notification_reply_correlation_invalid")
    );

    let snapshot = project_store::load_project_snapshot(&repo_root, &project_id)
        .expect("load project snapshot")
        .snapshot;
    let approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| approval.action_id == action_id)
        .expect("approval request for action");
    assert_eq!(approval.status, OperatorApprovalStatus::Pending);
}

#[test]
fn inbound_reply_ingestion_resets_malformed_cursor_and_survives_cursor_persist_failures() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_route(
        &repo_root,
        &project_id,
        "route-discord",
        "discord",
        "discord:998877665544332211",
    );

    let credential_store = MockCredentialStore::default()
        .with_route_credentials(
            &project_id,
            "route-discord",
            RouteCredentials::Discord(DiscordRouteCredentials {
                webhook_url: "https://discord.com/api/webhooks/1/2".into(),
                bot_token: Some("discord-bot-token".into()),
            }),
        )
        .with_inbound_cursor(
            &project_id,
            "route-discord",
            NotificationRouteKind::Discord,
            "malformed-cursor",
        )
        .fail_cursor_persist_for_route(
            &project_id,
            "route-discord",
            NotificationRouteKind::Discord,
        );

    let discord_transport = MockDiscordTransport::default().with_reply_batch(
        "998877665544332211",
        Ok(NotificationInboundBatch {
            messages: Vec::new(),
            next_cursor: Some("998877665544332299".into()),
        }),
    );

    let service = NotificationDispatchService::new(
        credential_store.clone(),
        MockTelegramTransport::default(),
        discord_transport.clone(),
    );

    let result = service
        .ingest_replies_for_project(app.handle().clone(), &repo_root, &project_id)
        .expect("ingest with malformed cursor state");

    assert_eq!(result.message_count, 0);
    assert!(result.rejected_count >= 2);
    assert!(result.attempts.iter().any(|attempt| {
        attempt.reply_code.as_deref() == Some("notification_adapter_cursor_malformed")
    }));
    assert!(result.attempts.iter().any(|attempt| {
        attempt.reply_code.as_deref() == Some("notification_adapter_credentials_write_failed")
    }));

    let requested_cursors = discord_transport.requested_cursors();
    assert_eq!(requested_cursors.len(), 1);
    assert_eq!(requested_cursors[0], None);

    assert_eq!(
        credential_store
            .cursor_for(&project_id, "route-discord", NotificationRouteKind::Discord)
            .as_deref(),
        Some("malformed-cursor")
    );
}

#[test]
fn inbound_reply_ingestion_continues_other_routes_when_one_fetch_errors() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:tg-room",
    );
    upsert_route(
        &repo_root,
        &project_id,
        "route-discord",
        "discord",
        "discord:998877665544332211",
    );

    let action_id =
        upsert_pending_approval(&repo_root, &project_id, "session-3", "2026-04-17T03:30:00Z");
    let dispatch =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load dispatches")
            .into_iter()
            .find(|dispatch| dispatch.route_id == "route-discord")
            .expect("discord dispatch");

    let credential_store = MockCredentialStore::default()
        .with_route_credentials(
            &project_id,
            "route-telegram",
            RouteCredentials::Telegram(TelegramRouteCredentials {
                bot_token: "telegram-bot-token".into(),
                chat_id: "tg-room".into(),
            }),
        )
        .with_route_credentials(
            &project_id,
            "route-discord",
            RouteCredentials::Discord(DiscordRouteCredentials {
                webhook_url: "https://discord.com/api/webhooks/1/2".into(),
                bot_token: Some("discord-bot-token".into()),
            }),
        );

    let telegram_transport = MockTelegramTransport::default().with_reply_batch(
        "tg-room",
        Err(NotificationAdapterError::transport_timeout(
            "Simulated Telegram timeout.",
        )),
    );

    let discord_transport = MockDiscordTransport::default().with_reply_batch(
        "998877665544332211",
        Ok(NotificationInboundBatch {
            messages: vec![NotificationInboundMessage {
                message_id: "4001".into(),
                responder_id: Some("discord-user".into()),
                received_at: "2026-04-17T03:30:05Z".into(),
                body: format!("reject {} Cannot approve yet.", dispatch.correlation_key),
                context_action_id: Some(action_id.clone()),
            }],
            next_cursor: Some("4002".into()),
        }),
    );

    let service =
        NotificationDispatchService::new(credential_store, telegram_transport, discord_transport);

    let result = service
        .ingest_replies_for_project(app.handle().clone(), &repo_root, &project_id)
        .expect("ingest replies with one failing route");

    assert_eq!(result.message_count, 1);
    assert_eq!(result.accepted_count, 1);
    assert!(result.attempts.iter().any(|attempt| {
        attempt.reply_code.as_deref() == Some("notification_adapter_transport_timeout")
    }));
    assert!(result
        .attempts
        .iter()
        .any(|attempt| attempt.accepted && attempt.diagnostic_code == REPLY_RECEIVED_DIAGNOSTIC));

    let snapshot = project_store::load_project_snapshot(&repo_root, &project_id)
        .expect("load project snapshot")
        .snapshot;
    let approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| approval.action_id == action_id)
        .expect("approval request for action");
    assert_eq!(approval.status, OperatorApprovalStatus::Rejected);
}

#[test]
fn inbound_reply_ingestion_route_message_dedupe_skips_duplicate_message_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_route(
        &repo_root,
        &project_id,
        "route-discord",
        "discord",
        "discord:998877665544332211",
    );

    let action_id =
        upsert_pending_approval(&repo_root, &project_id, "session-4", "2026-04-17T03:40:00Z");
    let dispatch =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load dispatches")
            .into_iter()
            .next()
            .expect("dispatch row");

    let credential_store = MockCredentialStore::default().with_route_credentials(
        &project_id,
        "route-discord",
        RouteCredentials::Discord(DiscordRouteCredentials {
            webhook_url: "https://discord.com/api/webhooks/1/2".into(),
            bot_token: Some("discord-bot-token".into()),
        }),
    );

    let duplicate_message = NotificationInboundMessage {
        message_id: "5001".into(),
        responder_id: Some("discord-user".into()),
        received_at: "2026-04-17T03:40:05Z".into(),
        body: format!(
            "reject {} Duplicate event replay.",
            dispatch.correlation_key
        ),
        context_action_id: Some(action_id.clone()),
    };

    let discord_transport = MockDiscordTransport::default().with_reply_batch(
        "998877665544332211",
        Ok(NotificationInboundBatch {
            messages: vec![duplicate_message.clone(), duplicate_message],
            next_cursor: Some("5002".into()),
        }),
    );

    let service = NotificationDispatchService::new(
        credential_store,
        MockTelegramTransport::default(),
        discord_transport,
    );

    let result = service
        .ingest_replies_for_project(app.handle().clone(), &repo_root, &project_id)
        .expect("ingest duplicate message ids");

    assert_eq!(result.message_count, 2);
    assert_eq!(result.accepted_count, 1);
    assert_eq!(result.rejected_count, 1);
    assert!(result.attempts.iter().any(|attempt| {
        attempt.reply_code.as_deref() == Some("notification_adapter_reply_duplicate")
    }));

    let claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load claims after duplicate ingestion");
    assert_eq!(claims.len(), 1);
    assert_eq!(
        claims[0].status,
        project_store::NotificationReplyClaimStatus::Accepted
    );
}

#[test]
fn inbound_reply_ingestion_uses_submit_notification_reply_contract_directly() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:tg-room",
    );

    let action_id =
        upsert_pending_approval(&repo_root, &project_id, "session-5", "2026-04-17T03:50:00Z");
    let dispatch =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load dispatch")
            .into_iter()
            .next()
            .expect("dispatch row");

    let service = NotificationDispatchService::new(
        MockCredentialStore::default().with_route_credentials(
            &project_id,
            "route-telegram",
            RouteCredentials::Telegram(TelegramRouteCredentials {
                bot_token: "telegram-bot-token".into(),
                chat_id: "tg-room".into(),
            }),
        ),
        MockTelegramTransport::default().with_reply_batch(
            "tg-room",
            Ok(NotificationInboundBatch {
                messages: vec![NotificationInboundMessage {
                    message_id: "6001".into(),
                    responder_id: Some("telegram-user".into()),
                    received_at: "2026-04-17T03:50:05Z".into(),
                    body: format!("approve {} Contract path", dispatch.correlation_key),
                    context_action_id: Some(action_id.clone()),
                }],
                next_cursor: Some("6002".into()),
            }),
        ),
        MockDiscordTransport::default(),
    );

    let result = service
        .ingest_replies_for_project_with_submitter(&repo_root, &project_id, |request| {
            submit_notification_reply(app.handle().clone(), request)
        })
        .expect("submit through command contract");

    assert_eq!(result.accepted_count, 1);
    assert_eq!(result.rejected_count, 0);
}

#[test]
fn sync_notification_adapters_command_seam_preserves_dispatch_and_first_wins_reply_invariants() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:tg-room",
    );
    upsert_route(
        &repo_root,
        &project_id,
        "route-discord",
        "discord",
        "discord:998877665544332211",
    );

    let action_id = upsert_pending_approval(
        &repo_root,
        &project_id,
        "session-sync",
        "2026-04-17T04:00:00Z",
    );

    let dispatches =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load dispatches for sync")
            .into_iter()
            .collect::<Vec<_>>();
    let telegram_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-telegram")
        .expect("telegram dispatch for sync");
    let discord_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-discord")
        .expect("discord dispatch for sync");

    let service = NotificationDispatchService::new(
        MockCredentialStore::default()
            .with_route_credentials(
                &project_id,
                "route-telegram",
                RouteCredentials::Telegram(TelegramRouteCredentials {
                    bot_token: "telegram-bot-token".into(),
                    chat_id: "tg-room".into(),
                }),
            )
            .with_route_credentials(
                &project_id,
                "route-discord",
                RouteCredentials::Discord(DiscordRouteCredentials {
                    webhook_url: "https://discord.com/api/webhooks/1/2".into(),
                    bot_token: Some("discord-bot-token".into()),
                }),
            ),
        MockTelegramTransport::default().with_reply_batch(
            "tg-room",
            Ok(NotificationInboundBatch {
                messages: vec![NotificationInboundMessage {
                    message_id: "7001".into(),
                    responder_id: Some("telegram-user".into()),
                    received_at: "2026-04-17T04:00:05Z".into(),
                    body: format!(
                        "approve {} Telegram reply wins first.",
                        telegram_dispatch.correlation_key
                    ),
                    context_action_id: Some(action_id.clone()),
                }],
                next_cursor: Some("7002".into()),
            }),
        ),
        MockDiscordTransport::default().with_reply_batch(
            "998877665544332211",
            Ok(NotificationInboundBatch {
                messages: vec![NotificationInboundMessage {
                    message_id: "7101".into(),
                    responder_id: Some("discord-user".into()),
                    received_at: "2026-04-17T04:00:06Z".into(),
                    body: format!(
                        "approve {} Discord duplicate arrives second.",
                        discord_dispatch.correlation_key
                    ),
                    context_action_id: Some(action_id.clone()),
                }],
                next_cursor: Some("7102".into()),
            }),
        ),
    );

    let sync_result = sync_notification_adapters_with_service(
        app.handle().clone(),
        &repo_root,
        &project_id,
        &service,
    )
    .expect("sync notification adapters should run dispatch then reply cycles");

    assert_eq!(sync_result.project_id, project_id);
    assert_eq!(sync_result.dispatch.attempted_count, 2);
    assert_eq!(sync_result.dispatch.sent_count, 2);
    assert_eq!(sync_result.dispatch.failed_count, 0);
    assert_eq!(sync_result.replies.accepted_count, 1);
    assert_eq!(sync_result.replies.rejected_count, 1);
    assert!(sync_result
        .replies
        .error_code_counts
        .iter()
        .any(|entry| entry.code == "notification_reply_already_claimed" && entry.count == 1));

    let claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load reply claims after sync cycle");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| { claim.status == project_store::NotificationReplyClaimStatus::Accepted }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let snapshot = project_store::load_project_snapshot(&repo_root, &project_id)
        .expect("load project snapshot after sync cycle")
        .snapshot;
    let approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| approval.action_id == action_id)
        .expect("approval request for synced action");
    assert_eq!(approval.status, OperatorApprovalStatus::Approved);
    assert!(snapshot.resume_history.iter().any(|entry| {
        entry.source_action_id.as_deref() == Some(action_id.as_str())
            && entry.status == ResumeHistoryStatus::Started
    }));
}
