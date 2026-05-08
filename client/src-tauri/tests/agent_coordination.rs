use std::{fs, path::PathBuf};

use tempfile::TempDir;
use xero_desktop_lib::{
    auth::now_timestamp,
    commands::RuntimeAgentIdDto,
    db::{self, project_store},
    git::repository::CanonicalRepository,
    runtime::{
        AutonomousAgentCoordinationAction, AutonomousAgentCoordinationRequest,
        AutonomousToolOutput, AutonomousToolRuntime,
    },
    state::DesktopState,
};

fn seed_project(root: &TempDir) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("create repo root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let project_id = "project-coordination".to_string();
    let repository = CanonicalRepository {
        project_id: project_id.clone(),
        repository_id: "repo-coordination".into(),
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

    db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
    db::import_project(&repository, DesktopState::default().import_failpoints())
        .expect("import project");
    (project_id, canonical_root)
}

fn create_session(repo_root: &std::path::Path, project_id: &str, title: &str) -> String {
    project_store::create_agent_session(
        repo_root,
        &project_store::AgentSessionCreateRecord {
            project_id: project_id.into(),
            title: title.into(),
            summary: "Parallel active run".into(),
            selected: false,
        },
    )
    .expect("create agent session")
    .agent_session_id
}

fn seed_run(
    repo_root: &std::path::Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
) -> project_store::AgentRunSnapshotRecord {
    project_store::insert_agent_run(
        repo_root,
        &project_store::NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            prompt: "Coordinate active work.".into(),
            system_prompt: "system".into(),
            now: "2026-05-03T00:00:00Z".into(),
        },
    )
    .expect("seed agent run")
}

#[expect(
    clippy::too_many_arguments,
    reason = "test helper mirrors code-change capture inputs"
)]
fn capture_text_modify(
    repo_root: &std::path::Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
    change_group_id: &str,
    path: &str,
    before: &str,
    after: &str,
) -> project_store::CompletedCodeChangeGroup {
    let absolute_path = repo_root.join(path);
    fs::create_dir_all(absolute_path.parent().expect("parent directory"))
        .expect("create parent directory");
    fs::write(&absolute_path, before).expect("write before content");

    let handle = project_store::begin_exact_path_capture(
        repo_root,
        project_store::CodeChangeGroupInput {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
            change_group_id: Some(change_group_id.into()),
            parent_change_group_id: None,
            tool_call_id: None,
            runtime_event_id: None,
            conversation_sequence: None,
            change_kind: project_store::CodeChangeKind::FileTool,
            summary_label: format!("Modify {path}"),
            restore_state: project_store::CodeChangeRestoreState::SnapshotAvailable,
        },
        vec![project_store::CodeRollbackCaptureTarget::modify(path)],
    )
    .expect("begin code capture");
    fs::write(&absolute_path, after).expect("write after content");
    project_store::complete_exact_path_capture(repo_root, handle).expect("complete code capture")
}

fn coordination_request(
    action: AutonomousAgentCoordinationAction,
) -> AutonomousAgentCoordinationRequest {
    AutonomousAgentCoordinationRequest {
        action,
        path: None,
        paths: Vec::new(),
        operation: None,
        note: None,
        override_reason: None,
        reservation_id: None,
        release_reason: None,
        item_type: None,
        item_id: None,
        target_agent_session_id: None,
        target_run_id: None,
        target_role: None,
        title: None,
        body: None,
        priority: None,
        ttl_seconds: None,
        summary: None,
        limit: None,
    }
}

#[test]
fn file_reservations_detect_overlap_expire_and_allow_explicit_override() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_run(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-one",
    );
    let second_session = create_session(&repo_root, &project_id, "Parallel");
    seed_run(&repo_root, &project_id, &second_session, "run-two");

    let first_claim = project_store::claim_agent_file_reservations(
        &repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project_id.clone(),
            owner_run_id: "run-one".into(),
            paths: vec!["src/lib.rs".into()],
            operation: project_store::AgentCoordinationReservationOperation::Editing,
            note: Some("Editing runtime bus".into()),
            override_reason: None,
            claimed_at: "2026-05-03T00:00:00Z".into(),
            lease_seconds: Some(1),
        },
    )
    .expect("claim first reservation");
    assert_eq!(first_claim.claimed.len(), 1);

    let conflicts = project_store::check_agent_file_reservation_conflicts(
        &repo_root,
        &project_id,
        "run-two",
        &["src".into()],
        "2026-05-03T00:00:00Z",
    )
    .expect("check conflicts");
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].reservation.owner_run_id, "run-one");

    let refused = project_store::claim_agent_file_reservations(
        &repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project_id.clone(),
            owner_run_id: "run-two".into(),
            paths: vec!["src/lib.rs".into()],
            operation: project_store::AgentCoordinationReservationOperation::Editing,
            note: None,
            override_reason: None,
            claimed_at: "2026-05-03T00:00:00Z".into(),
            lease_seconds: Some(300),
        },
    )
    .expect("refuse conflicting claim");
    assert!(refused.claimed.is_empty());
    assert_eq!(refused.conflicts.len(), 1);

    let override_claim = project_store::claim_agent_file_reservations(
        &repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project_id.clone(),
            owner_run_id: "run-two".into(),
            paths: vec!["src/lib.rs".into()],
            operation: project_store::AgentCoordinationReservationOperation::Refactoring,
            note: Some("Coordinated override".into()),
            override_reason: Some("Pairing with run-one; same pane owner approved.".into()),
            claimed_at: "2026-05-03T00:00:00Z".into(),
            lease_seconds: Some(300),
        },
    )
    .expect("override conflicting claim");
    assert_eq!(override_claim.claimed.len(), 1);
    assert_eq!(override_claim.conflicts.len(), 1);
    assert!(override_claim.override_recorded);

    project_store::release_agent_file_reservations(
        &repo_root,
        &project_store::ReleaseAgentFileReservationRequest {
            project_id: project_id.clone(),
            owner_run_id: "run-two".into(),
            reservation_id: None,
            paths: vec!["src/lib.rs".into()],
            release_reason: "test_release".into(),
            released_at: "2026-05-03T00:00:01Z".into(),
        },
    )
    .expect("release override claim");

    let after_expiry = project_store::check_agent_file_reservation_conflicts(
        &repo_root,
        &project_id,
        "run-two",
        &["src/lib.rs".into()],
        "2026-05-03T00:00:02Z",
    )
    .expect("check after expiry");
    assert!(after_expiry.is_empty());
}

#[test]
fn active_presence_and_reservations_are_cleaned_up_for_completed_runs() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_run(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-one",
    );
    let second_session = create_session(&repo_root, &project_id, "Parallel");
    seed_run(&repo_root, &project_id, &second_session, "run-two");

    for run_id in ["run-one", "run-two"] {
        project_store::upsert_agent_coordination_presence(
            &repo_root,
            &project_store::UpsertAgentCoordinationPresenceRecord {
                project_id: project_id.clone(),
                run_id: run_id.into(),
                pane_id: None,
                status: "running".into(),
                current_phase: "editing".into(),
                activity_summary: format!("{run_id} is editing."),
                last_event_id: None,
                last_event_kind: None,
                updated_at: "2026-05-03T00:00:00Z".into(),
                lease_seconds: Some(120),
            },
        )
        .expect("upsert presence");
    }

    let siblings = project_store::list_active_agent_coordination_presence(
        &repo_root,
        &project_id,
        Some("run-one"),
        "2026-05-03T00:00:01Z",
        10,
    )
    .expect("list active presence");
    assert_eq!(siblings.len(), 1);
    assert_eq!(siblings[0].run_id, "run-two");

    project_store::claim_agent_file_reservations(
        &repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project_id.clone(),
            owner_run_id: "run-one".into(),
            paths: vec!["src/cleanup.rs".into()],
            operation: project_store::AgentCoordinationReservationOperation::Writing,
            note: None,
            override_reason: None,
            claimed_at: "2026-05-03T00:00:00Z".into(),
            lease_seconds: Some(300),
        },
    )
    .expect("claim cleanup reservation");

    project_store::cleanup_agent_coordination_for_run(
        &repo_root,
        &project_id,
        "run-one",
        "run_completed",
        "2026-05-03T00:00:02Z",
    )
    .expect("cleanup completed run");

    let remaining_presence = project_store::list_active_agent_coordination_presence(
        &repo_root,
        &project_id,
        Some("run-two"),
        "2026-05-03T00:00:03Z",
        10,
    )
    .expect("list remaining presence");
    assert!(remaining_presence.is_empty());

    let conflicts = project_store::check_agent_file_reservation_conflicts(
        &repo_root,
        &project_id,
        "run-two",
        &["src/cleanup.rs".into()],
        "2026-05-03T00:00:03Z",
    )
    .expect("check released reservation");
    assert!(conflicts.is_empty());
}

#[test]
fn child_run_reservations_record_parent_and_child_lineage() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    let parent = seed_run(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-parent",
    );
    seed_run(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-child",
    );
    project_store::update_agent_run_lineage(
        &repo_root,
        &project_store::AgentRunLineageUpdateRecord {
            project_id: project_id.clone(),
            run_id: "run-child".into(),
            parent_run_id: "run-parent".into(),
            parent_trace_id: parent.run.trace_id.clone(),
            parent_subagent_id: "subagent-1".into(),
            subagent_role: "engineer".into(),
            updated_at: "2026-05-03T00:00:01Z".into(),
        },
    )
    .expect("attach child lineage");

    let claim = project_store::claim_agent_file_reservations(
        &repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id,
            owner_run_id: "run-child".into(),
            paths: vec!["src/child.rs".into()],
            operation: project_store::AgentCoordinationReservationOperation::Refactoring,
            note: Some("Child agent writeSet".into()),
            override_reason: None,
            claimed_at: "2026-05-03T00:00:02Z".into(),
            lease_seconds: Some(300),
        },
    )
    .expect("claim child reservation");

    assert_eq!(claim.claimed.len(), 1);
    let reservation = &claim.claimed[0];
    assert_eq!(reservation.owner_run_id, "run-parent");
    assert_eq!(reservation.owner_child_run_id.as_deref(), Some("run-child"));
    assert_eq!(reservation.owner_role.as_deref(), Some("engineer"));
}

#[test]
fn history_undo_operations_publish_coordination_events_and_expire() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_run(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-one",
    );
    let second_session = create_session(&repo_root, &project_id, "Parallel");
    seed_run(&repo_root, &project_id, &second_session, "run-two");

    let clean_target = capture_text_modify(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-one",
        "code-change-history-clean",
        "src/history_clean.rs",
        "before\n",
        "after\n",
    );
    let clean_undo = project_store::apply_code_change_group_undo(
        &repo_root,
        project_store::ApplyCodeChangeGroupUndoRequest {
            project_id: project_id.clone(),
            operation_id: Some("history-op-clean".into()),
            target_change_group_id: clean_target.change_group_id.clone(),
            expected_workspace_epoch: None,
        },
    )
    .expect("apply clean undo");
    assert_eq!(
        clean_undo.status,
        project_store::CodeFileUndoApplyStatus::Completed
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("src/history_clean.rs")).expect("read clean file"),
        "before\n"
    );

    let conflicted_target = capture_text_modify(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-one",
        "code-change-history-conflict",
        "src/history_conflict.rs",
        "base\n",
        "selected\n",
    );
    fs::write(
        repo_root.join("src/history_conflict.rs"),
        "current overlay\n",
    )
    .expect("write conflicting current overlay");
    let conflicted_undo = project_store::apply_code_change_group_undo(
        &repo_root,
        project_store::ApplyCodeChangeGroupUndoRequest {
            project_id: project_id.clone(),
            operation_id: Some("history-op-conflict".into()),
            target_change_group_id: conflicted_target.change_group_id.clone(),
            expected_workspace_epoch: None,
        },
    )
    .expect("apply conflicted undo");
    assert_eq!(
        conflicted_undo.status,
        project_store::CodeFileUndoApplyStatus::Conflicted
    );

    let context = project_store::active_agent_coordination_context(
        &repo_root,
        &project_id,
        "run-two",
        "2026-05-06T00:00:00Z",
    )
    .expect("active coordination context");
    let clean_event = context
        .events
        .iter()
        .find(|event| event.payload["operationId"] == "history-op-clean")
        .expect("clean undo coordination event");
    assert_eq!(clean_event.event_kind, "history_rewrite_notice");
    assert_eq!(clean_event.payload["mode"], "selective_undo");
    assert_eq!(clean_event.payload["status"], "completed");
    assert_eq!(
        clean_event.payload["target"]["id"],
        clean_target.change_group_id
    );
    assert_eq!(
        clean_event.payload["affectedPaths"],
        serde_json::json!(["src/history_clean.rs"])
    );
    assert_eq!(
        clean_event.payload["resultCommitId"],
        serde_json::json!(clean_undo.result_commit_id)
    );
    assert_eq!(clean_event.payload["conflicts"], serde_json::json!([]));

    let conflict_event = context
        .events
        .iter()
        .find(|event| event.payload["operationId"] == "history-op-conflict")
        .expect("conflicted undo coordination event");
    assert_eq!(conflict_event.event_kind, "undo_conflict_notice");
    assert_eq!(conflict_event.payload["status"], "conflicted");
    assert_eq!(
        conflict_event.payload["target"]["id"],
        conflicted_target.change_group_id
    );
    assert_eq!(
        conflict_event.payload["affectedPaths"],
        serde_json::json!(["src/history_conflict.rs"])
    );
    assert_eq!(
        conflict_event.payload["conflicts"][0]["kind"],
        "text_overlap"
    );
    assert_eq!(
        conflict_event.payload["resultCommitId"],
        serde_json::Value::Null
    );

    let expired_context = project_store::active_agent_coordination_context(
        &repo_root,
        &project_id,
        "run-two",
        "9999-01-01T00:00:00Z",
    )
    .expect("expired coordination context");
    assert!(expired_context.events.iter().all(|event| {
        event.payload["operationId"] != "history-op-clean"
            && event.payload["operationId"] != "history-op-conflict"
    }));
}

#[test]
fn history_undo_invalidates_overlapping_file_reservations() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_run(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-one",
    );
    let second_session = create_session(&repo_root, &project_id, "Parallel");
    seed_run(&repo_root, &project_id, &second_session, "run-two");

    let claim = project_store::claim_agent_file_reservations(
        &repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project_id.clone(),
            owner_run_id: "run-two".into(),
            paths: vec![
                "src/history_reserved.rs".into(),
                "src/history_unaffected.rs".into(),
            ],
            operation: project_store::AgentCoordinationReservationOperation::Editing,
            note: Some("Editing history reservation paths".into()),
            override_reason: None,
            claimed_at: now_timestamp(),
            lease_seconds: Some(3_600),
        },
    )
    .expect("claim reservations");
    assert_eq!(claim.claimed.len(), 2);

    let target = capture_text_modify(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-one",
        "code-change-history-reservation",
        "src/history_reserved.rs",
        "before\n",
        "after\n",
    );
    let undo = project_store::apply_code_change_group_undo(
        &repo_root,
        project_store::ApplyCodeChangeGroupUndoRequest {
            project_id: project_id.clone(),
            operation_id: Some("history-op-reservation".into()),
            target_change_group_id: target.change_group_id,
            expected_workspace_epoch: None,
        },
    )
    .expect("apply undo");
    assert_eq!(
        undo.status,
        project_store::CodeFileUndoApplyStatus::Completed
    );

    let inbox = project_store::list_agent_mailbox_inbox(
        &repo_root,
        &project_id,
        "run-two",
        &now_timestamp(),
        10,
    )
    .expect("list mailbox inbox");
    let invalidation_notice = inbox
        .iter()
        .find(|delivery| {
            delivery.item.item_type == project_store::AgentMailboxItemType::ReservationInvalidated
                && delivery
                    .item
                    .related_paths
                    .iter()
                    .any(|path| path == "src/history_reserved.rs")
        })
        .expect("reservation invalidation mailbox notice");
    assert_eq!(
        invalidation_notice.item.priority,
        project_store::AgentMailboxPriority::High
    );

    let reservations = project_store::list_agent_file_reservations_for_run(
        &repo_root,
        &project_id,
        "run-two",
        &now_timestamp(),
        10,
    )
    .expect("list run reservations");
    let invalidated = reservations
        .iter()
        .find(|reservation| reservation.path == "src/history_reserved.rs")
        .expect("invalidated reservation");
    assert!(invalidated.invalidated_at.is_some());
    assert_eq!(
        invalidated.invalidating_history_operation_id.as_deref(),
        Some("history-op-reservation")
    );
    assert!(invalidated
        .invalidation_reason
        .as_deref()
        .unwrap_or_default()
        .contains("history-op-reservation"));
    assert!(invalidated.released_at.is_none());

    let unaffected = reservations
        .iter()
        .find(|reservation| reservation.path == "src/history_unaffected.rs")
        .expect("unaffected reservation");
    assert!(unaffected.invalidated_at.is_none());

    let still_active_on_invalidated_path =
        project_store::has_active_agent_file_reservation_for_paths(
            &repo_root,
            &project_id,
            "run-two",
            &["src/history_reserved.rs".into()],
            &now_timestamp(),
        )
        .expect("check invalidated active reservation");
    assert!(!still_active_on_invalidated_path);

    let still_active_on_unaffected_path =
        project_store::has_active_agent_file_reservation_for_paths(
            &repo_root,
            &project_id,
            "run-two",
            &["src/history_unaffected.rs".into()],
            &now_timestamp(),
        )
        .expect("check unaffected active reservation");
    assert!(still_active_on_unaffected_path);

    let active = project_store::list_active_agent_file_reservations(
        &repo_root,
        &project_id,
        None,
        &now_timestamp(),
        10,
    )
    .expect("list active reservations");
    assert!(active
        .iter()
        .all(|reservation| reservation.path != "src/history_reserved.rs"));
    assert!(active
        .iter()
        .any(|reservation| reservation.path == "src/history_unaffected.rs"));
}

#[test]
fn history_notice_acknowledgement_reports_epoch_and_reservation_can_be_renewed() {
    let root = tempfile::tempdir().expect("temp dir");
    let (project_id, repo_root) = seed_project(&root);
    seed_run(
        &repo_root,
        &project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
        "run-history-owner",
    );
    let blocked_session = create_session(&repo_root, &project_id, "Blocked");
    seed_run(&repo_root, &project_id, &blocked_session, "run-blocked");

    fs::write(repo_root.join("src/history_notice.rs"), "current\n").expect("seed notice file");
    let claimed = project_store::claim_agent_file_reservations(
        &repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project_id.clone(),
            owner_run_id: "run-blocked".into(),
            paths: vec!["src/history_notice.rs".into()],
            operation: project_store::AgentCoordinationReservationOperation::Editing,
            note: Some("Editing before history operation".into()),
            override_reason: None,
            claimed_at: now_timestamp(),
            lease_seconds: Some(3_600),
        },
    )
    .expect("claim initial reservation");
    assert_eq!(claimed.claimed.len(), 1);

    let advanced = project_store::advance_code_workspace_epoch(
        &repo_root,
        &project_store::AdvanceCodeWorkspaceEpochRequest {
            project_id: project_id.clone(),
            head_id: Some("code-commit-history-notice".into()),
            tree_id: Some("code-tree-history-notice".into()),
            commit_id: Some("code-commit-history-notice".into()),
            latest_history_operation_id: Some("history-op-notice".into()),
            affected_paths: vec!["src/history_notice.rs".into()],
            updated_at: now_timestamp(),
        },
    )
    .expect("advance code workspace epoch");
    assert_eq!(advanced.workspace_head.workspace_epoch, 1);
    project_store::invalidate_overlapping_agent_file_reservations(
        &repo_root,
        &project_store::InvalidateAgentFileReservationsRequest {
            project_id: project_id.clone(),
            history_operation_id: "history-op-notice".into(),
            affected_paths: vec!["src/history_notice.rs".into()],
            invalidated_at: now_timestamp(),
        },
    )
    .expect("invalidate stale reservation");

    let notice = project_store::publish_agent_mailbox_item(
        &repo_root,
        &project_store::NewAgentMailboxItemRecord {
            project_id: project_id.clone(),
            sender_run_id: "run-history-owner".into(),
            item_type: project_store::AgentMailboxItemType::ReservationInvalidated,
            parent_item_id: None,
            target_agent_session_id: Some(blocked_session.clone()),
            target_run_id: Some("run-blocked".into()),
            target_role: None,
            title: "History operation changed your reserved path".into(),
            body: "Code history operation `history-op-notice` changed src/history_notice.rs. Re-read current files before overlapping writes.".into(),
            related_paths: vec!["src/history_notice.rs".into()],
            priority: project_store::AgentMailboxPriority::High,
            created_at: now_timestamp(),
            ttl_seconds: Some(3_600),
        },
    )
    .expect("publish history mailbox notice");

    let runtime = AutonomousToolRuntime::new(&repo_root)
        .expect("build tool runtime")
        .with_agent_run_context(project_id.clone(), blocked_session, "run-blocked");

    let mut read_inbox = coordination_request(AutonomousAgentCoordinationAction::ReadInbox);
    read_inbox.limit = Some(10);
    let inbox_result = runtime.agent_coordination(read_inbox).expect("read inbox");
    let AutonomousToolOutput::AgentCoordination(inbox_output) = inbox_result.output else {
        panic!("expected agent coordination output");
    };
    assert!(inbox_output
        .mailbox
        .iter()
        .any(|delivery| delivery.item.item_id == notice.item_id));
    assert_eq!(inbox_output.code_workspace_epoch, None);

    let mut acknowledge = coordination_request(AutonomousAgentCoordinationAction::Acknowledge);
    acknowledge.item_id = Some(notice.item_id.clone());
    let ack_result = runtime
        .agent_coordination(acknowledge)
        .expect("acknowledge history notice");
    let AutonomousToolOutput::AgentCoordination(ack_output) = ack_result.output else {
        panic!("expected agent coordination output");
    };
    assert_eq!(
        ack_output
            .mailbox_item
            .as_ref()
            .map(|item| item.item_id.as_str()),
        Some(notice.item_id.as_str())
    );
    assert_eq!(ack_output.code_workspace_epoch, Some(1));
    assert_eq!(ack_output.refreshed_paths, vec!["src/history_notice.rs"]);
    assert!(ack_output
        .message
        .contains("refreshed observed code workspace epoch 1"));

    let mut list_reservations =
        coordination_request(AutonomousAgentCoordinationAction::ListReservations);
    list_reservations.limit = Some(10);
    let list_result = runtime
        .agent_coordination(list_reservations)
        .expect("list reservations");
    let AutonomousToolOutput::AgentCoordination(list_output) = list_result.output else {
        panic!("expected agent coordination output");
    };
    assert!(list_output.reservations.iter().any(|reservation| {
        reservation.path == "src/history_notice.rs" && reservation.invalidated_at.is_some()
    }));

    let mut renew = coordination_request(AutonomousAgentCoordinationAction::ClaimReservation);
    renew.paths = vec!["src/history_notice.rs".into()];
    renew.operation = Some(project_store::AgentCoordinationReservationOperation::Editing);
    renew.note = Some("Renewed after reading the current file and acknowledging history.".into());
    let renew_result = runtime
        .agent_coordination(renew)
        .expect("renew reservation");
    let AutonomousToolOutput::AgentCoordination(renew_output) = renew_result.output else {
        panic!("expected agent coordination output");
    };
    assert!(renew_output.conflicts.is_empty());
    assert_eq!(renew_output.reservations.len(), 1);
    assert_eq!(renew_output.reservations[0].path, "src/history_notice.rs");
    assert!(renew_output.reservations[0].invalidated_at.is_none());
}
