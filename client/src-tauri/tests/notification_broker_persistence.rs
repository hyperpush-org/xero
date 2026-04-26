use std::path::{Path, PathBuf};

use cadence_desktop_lib::{
    commands::OperatorApprovalStatus,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    state::DesktopState,
};
use rusqlite::{params, Connection};
use tempfile::TempDir;

fn seed_project(root: &TempDir, project_id: &str, repository_id: &str, repo_name: &str) -> PathBuf {
    let repo_root = root.path().join(repo_name);
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();

    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: repository_id.into(),
        root_path: canonical_root.clone(),
        root_path_string,
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

    let state = DesktopState::default();
    db::import_project(&repository, state.import_failpoints()).expect("import project");

    canonical_root
}

fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open repo-local database")
}

fn insert_project_row(repo_root: &Path, project_id: &str, name: &str) {
    let connection = open_state_connection(repo_root);
    connection
        .execute(
            r#"
            INSERT INTO projects (
                id,
                name,
                description,
                milestone,
                total_phases,
                completed_phases,
                active_phase,
                branch,
                runtime,
                updated_at
            )
            VALUES (?1, ?2, '', '', 0, 0, 0, 'main', NULL, '2026-04-16T10:00:00Z')
            "#,
            params![project_id, name],
        )
        .expect("insert project row");
}

fn upsert_route(
    repo_root: &Path,
    project_id: &str,
    route_id: &str,
    route_kind: &str,
    route_target: &str,
    updated_at: &str,
) -> project_store::NotificationRouteRecord {
    project_store::upsert_notification_route(
        repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: route_id.into(),
            route_kind: route_kind.into(),
            route_target: route_target.into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: updated_at.into(),
        },
    )
    .expect("upsert notification route")
}

fn upsert_pending_approval(repo_root: &Path, project_id: &str, created_at: &str) -> String {
    project_store::upsert_pending_operator_approval(
        repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "terminal_input_required",
        "Terminal input required",
        "Runtime paused and requires a coarse operator answer.",
        created_at,
    )
    .expect("upsert pending operator approval")
    .action_id
}

fn enqueue_dispatches(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    enqueued_at: &str,
) -> Vec<project_store::NotificationDispatchRecord> {
    project_store::enqueue_notification_dispatches(
        repo_root,
        &project_store::NotificationDispatchEnqueueRecord {
            project_id: project_id.into(),
            action_id: action_id.into(),
            enqueued_at: enqueued_at.into(),
        },
    )
    .expect("enqueue notification dispatches")
}

fn count_dispatch_rows(repo_root: &Path, project_id: &str, action_id: &str) -> i64 {
    let connection = open_state_connection(repo_root);
    connection
        .query_row(
            "SELECT COUNT(*) FROM notification_dispatches WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, action_id],
            |row| row.get(0),
        )
        .expect("count notification dispatch rows")
}

#[test]
fn notification_dispatch_enqueue_is_idempotent_per_route_and_action() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");

    upsert_route(
        &repo_root,
        project_id,
        "route-discord",
        "discord",
        "discord:ops-room",
        "2026-04-16T10:00:01Z",
    );
    upsert_route(
        &repo_root,
        project_id,
        "route-telegram",
        "telegram",
        "telegram:ops-channel",
        "2026-04-16T10:00:02Z",
    );

    let action_id = upsert_pending_approval(&repo_root, project_id, "2026-04-16T10:01:00Z");

    let first = enqueue_dispatches(&repo_root, project_id, &action_id, "2026-04-16T10:01:05Z");
    let second = enqueue_dispatches(&repo_root, project_id, &action_id, "2026-04-16T10:01:06Z");

    assert_eq!(first.len(), 2);
    assert_eq!(second.len(), 2);

    let first_ids: std::collections::BTreeMap<_, _> = first
        .iter()
        .map(|dispatch| (dispatch.route_id.clone(), dispatch.id))
        .collect();
    let second_ids: std::collections::BTreeMap<_, _> = second
        .iter()
        .map(|dispatch| (dispatch.route_id.clone(), dispatch.id))
        .collect();

    assert_eq!(first_ids, second_ids);
    assert_eq!(count_dispatch_rows(&repo_root, project_id, &action_id), 2);
    assert!(second.iter().all(|dispatch| {
        dispatch.status == project_store::NotificationDispatchStatus::Pending
            && dispatch.attempt_count == 0
    }));
}

#[test]
fn notification_reply_claim_is_first_wins_and_preserves_pending_operator_truth() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");

    upsert_route(
        &repo_root,
        project_id,
        "route-discord",
        "discord",
        "discord:ops-room",
        "2026-04-16T11:00:01Z",
    );

    let action_id = upsert_pending_approval(&repo_root, project_id, "2026-04-16T11:01:00Z");
    let dispatches = enqueue_dispatches(&repo_root, project_id, &action_id, "2026-04-16T11:01:05Z");
    let dispatch = dispatches
        .into_iter()
        .next()
        .expect("dispatch row should exist");

    let first_claim = project_store::claim_notification_reply(
        &repo_root,
        &project_store::NotificationReplyClaimRequestRecord {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            route_id: dispatch.route_id.clone(),
            correlation_key: dispatch.correlation_key.clone(),
            responder_id: Some("operator-a".into()),
            reply_text: "approved".into(),
            received_at: "2026-04-16T11:01:10Z".into(),
        },
    )
    .expect("first claim should succeed");

    assert_eq!(
        first_claim.claim.status,
        project_store::NotificationReplyClaimStatus::Accepted
    );
    assert_eq!(
        first_claim.dispatch.status,
        project_store::NotificationDispatchStatus::Claimed
    );

    let duplicate = project_store::claim_notification_reply(
        &repo_root,
        &project_store::NotificationReplyClaimRequestRecord {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            route_id: dispatch.route_id.clone(),
            correlation_key: dispatch.correlation_key,
            responder_id: Some("operator-b".into()),
            reply_text: "late answer".into(),
            received_at: "2026-04-16T11:01:11Z".into(),
        },
    )
    .expect_err("second claim should be rejected");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let claims =
        project_store::load_notification_reply_claims(&repo_root, project_id, Some(&action_id))
            .expect("load reply claim history");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| claim.status == project_store::NotificationReplyClaimStatus::Accepted));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load snapshot after claims")
        .snapshot;
    let approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| approval.action_id == action_id)
        .expect("approval row for action should exist");
    assert_eq!(approval.status, OperatorApprovalStatus::Pending);
}

#[test]
fn notification_reply_claim_rejects_malformed_and_cross_project_correlation_attempts() {
    let root = tempfile::tempdir().expect("temp dir");
    let repo_root = seed_project(&root, "project-1", "repo-1", "repo");
    insert_project_row(&repo_root, "project-2", "repo-two");

    upsert_route(
        &repo_root,
        "project-1",
        "route-discord",
        "discord",
        "discord:ops-room",
        "2026-04-16T12:00:01Z",
    );
    upsert_route(
        &repo_root,
        "project-2",
        "route-discord",
        "discord",
        "discord:ops-room-two",
        "2026-04-16T12:00:02Z",
    );

    let action_one = upsert_pending_approval(&repo_root, "project-1", "2026-04-16T12:01:00Z");
    let action_two = project_store::upsert_pending_operator_approval(
        &repo_root,
        "project-2",
        "session-2",
        Some("flow-2"),
        "terminal_input_required",
        "Terminal input required",
        "Runtime paused and requires a coarse operator answer.",
        "2026-04-16T12:01:01Z",
    )
    .expect("upsert project-2 pending operator approval")
    .action_id;

    let dispatch_one =
        enqueue_dispatches(&repo_root, "project-1", &action_one, "2026-04-16T12:01:05Z")
            .into_iter()
            .next()
            .expect("project-1 dispatch should exist");

    let malformed = project_store::claim_notification_reply(
        &repo_root,
        &project_store::NotificationReplyClaimRequestRecord {
            project_id: "project-1".into(),
            action_id: action_one.clone(),
            route_id: dispatch_one.route_id.clone(),
            correlation_key: "malformed-key".into(),
            responder_id: Some("operator-a".into()),
            reply_text: "approved".into(),
            received_at: "2026-04-16T12:01:10Z".into(),
        },
    )
    .expect_err("malformed correlation keys should fail closed");
    assert_eq!(malformed.code, "notification_reply_request_invalid");

    assert!(project_store::load_notification_reply_claims(
        &repo_root,
        "project-1",
        Some(&action_one)
    )
    .expect("load claims after malformed attempt")
    .is_empty());

    let forged = project_store::claim_notification_reply(
        &repo_root,
        &project_store::NotificationReplyClaimRequestRecord {
            project_id: "project-1".into(),
            action_id: action_two,
            route_id: dispatch_one.route_id,
            correlation_key: dispatch_one.correlation_key,
            responder_id: Some("operator-b".into()),
            reply_text: "approved".into(),
            received_at: "2026-04-16T12:01:11Z".into(),
        },
    )
    .expect_err("cross-project action ids should fail closed");
    assert_eq!(forged.code, "notification_reply_correlation_invalid");

    let claims = project_store::load_notification_reply_claims(&repo_root, "project-1", None)
        .expect("load claims after forged attempt");
    assert_eq!(claims.len(), 1);
    assert_eq!(
        claims[0].status,
        project_store::NotificationReplyClaimStatus::Rejected
    );
    assert_eq!(
        claims[0].rejection_code.as_deref(),
        Some("notification_reply_correlation_invalid")
    );
}

#[test]
fn notification_reply_claim_rejects_secret_bearing_reply_payloads() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");

    upsert_route(
        &repo_root,
        project_id,
        "route-discord",
        "discord",
        "discord:ops-room",
        "2026-04-16T13:00:01Z",
    );

    let action_id = upsert_pending_approval(&repo_root, project_id, "2026-04-16T13:01:00Z");
    let dispatch = enqueue_dispatches(&repo_root, project_id, &action_id, "2026-04-16T13:01:05Z")
        .into_iter()
        .next()
        .expect("dispatch row should exist");

    let error = project_store::claim_notification_reply(
        &repo_root,
        &project_store::NotificationReplyClaimRequestRecord {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            route_id: dispatch.route_id,
            correlation_key: dispatch.correlation_key,
            responder_id: Some("operator-a".into()),
            reply_text: "oauth access_token=sk-live-secret".into(),
            received_at: "2026-04-16T13:01:10Z".into(),
        },
    )
    .expect_err("secret-bearing payloads should fail closed");

    assert_eq!(error.code, "notification_reply_request_invalid");
    assert!(project_store::load_notification_reply_claims(
        &repo_root,
        project_id,
        Some(&action_id)
    )
    .expect("load claims after rejected secret payload")
    .is_empty());
}
