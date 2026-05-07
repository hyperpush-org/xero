use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

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

static PROJECT_DB_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn multi_agent_code_history_harness_preserves_conflicts_notifies_and_refreshes_stale_runs() {
    let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
    let root = tempfile::tempdir().expect("temp dir");
    let project = seed_project(&root);

    fs::write(
        project.repo_root.join("src/boundary.rs"),
        "boundary before\n",
    )
    .expect("boundary baseline");
    fs::write(project.repo_root.join("src/overlap.rs"), "base\n").expect("overlap baseline");
    fs::write(
        project.repo_root.join("src/adjacent.rs"),
        "left before\nright before\n",
    )
    .expect("adjacent baseline");

    activate_run(
        &project,
        &project.run_a,
        "Session A is editing code history paths.",
    );
    activate_run(
        &project,
        &project.run_b,
        "Session B is active on adjacent and overlap paths.",
    );
    claim_reservations(
        &project,
        &project.run_b,
        &["src/overlap.rs", "src/adjacent.rs"],
    );

    let boundary = capture_modify(
        &project,
        &project.session_a,
        &project.run_a,
        "code-change-s36-boundary",
        "boundary edit",
        "src/boundary.rs",
        "boundary after\n",
    );
    let a_overlap = capture_modify(
        &project,
        &project.session_a,
        &project.run_a,
        "code-change-s36-a-overlap",
        "session A overlap edit",
        "src/overlap.rs",
        "A owns\n",
    );
    capture_modify(
        &project,
        &project.session_b,
        &project.run_b,
        "code-change-s36-b-overlap",
        "session B overlap edit",
        "src/overlap.rs",
        "B owns\n",
    );

    let head_before_rollback =
        project_store::read_code_workspace_head(&project.repo_root, &project.project_id)
            .expect("read workspace head before rollback")
            .expect("workspace head before rollback");
    let rollback = project_store::apply_code_session_rollback(
        &project.repo_root,
        project_store::ApplyCodeSessionRollbackRequest {
            boundary: project_store::ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.session_a.clone(),
                target_kind: project_store::CodeSessionBoundaryTargetKind::SessionBoundary,
                target_id: format!("change_group:{}", boundary.change_group_id),
                boundary_id: format!("change_group:{}", boundary.change_group_id),
                run_id: None,
                change_group_id: Some(boundary.change_group_id.clone()),
            },
            operation_id: Some("history-op-s36-rollback-conflict".into()),
            explicitly_selected_change_group_ids: Vec::new(),
            expected_workspace_epoch: Some(head_before_rollback.workspace_epoch),
        },
    )
    .expect("session rollback conflict result");

    assert_eq!(
        rollback.status,
        project_store::CodeFileUndoApplyStatus::Conflicted
    );
    assert_eq!(
        rollback.target_change_group_ids,
        vec![a_overlap.change_group_id.clone()]
    );
    assert_eq!(rollback.result_commit_id, None);
    assert_eq!(rollback.conflicts.len(), 1);
    assert_eq!(rollback.conflicts[0].path, "src/overlap.rs");
    assert_eq!(
        rollback.conflicts[0].kind,
        project_store::CodeFileUndoConflictKind::TextOverlap
    );
    assert_eq!(
        fs::read_to_string(project.repo_root.join("src/overlap.rs")).expect("read overlap"),
        "B owns\n"
    );
    let head_after_rollback =
        project_store::read_code_workspace_head(&project.repo_root, &project.project_id)
            .expect("read workspace head after conflict")
            .expect("workspace head after conflict");
    assert_eq!(
        head_after_rollback.workspace_epoch,
        head_before_rollback.workspace_epoch
    );

    let conflict_notice = inbox_item_for_path(
        &project,
        &project.run_b,
        "src/overlap.rs",
        project_store::AgentMailboxItemType::UndoConflictNotice,
    );
    assert_eq!(
        conflict_notice.priority,
        project_store::AgentMailboxPriority::High
    );

    let a_adjacent = capture_modify(
        &project,
        &project.session_a,
        &project.run_a,
        "code-change-s36-a-adjacent",
        "session A adjacent edit",
        "src/adjacent.rs",
        "left A\nright before\n",
    );
    capture_modify(
        &project,
        &project.session_b,
        &project.run_b,
        "code-change-s36-b-adjacent",
        "session B adjacent edit",
        "src/adjacent.rs",
        "left A\nright B\n",
    );

    let observed_epoch_before_undo =
        project_store::read_code_workspace_head(&project.repo_root, &project.project_id)
            .expect("read workspace head before undo")
            .expect("workspace head before undo")
            .workspace_epoch;
    let undo = project_store::apply_code_file_undo(
        &project.repo_root,
        project_store::ApplyCodeFileUndoRequest {
            project_id: project.project_id.clone(),
            operation_id: Some("history-op-s36-adjacent-undo".into()),
            target_change_group_id: a_adjacent.change_group_id.clone(),
            target_patch_file_id: None,
            target_file_path: Some("src/adjacent.rs".into()),
            target_hunk_ids: Vec::new(),
            expected_workspace_epoch: Some(observed_epoch_before_undo),
        },
    )
    .expect("apply adjacent selective undo");

    assert_eq!(
        undo.status,
        project_store::CodeFileUndoApplyStatus::Completed
    );
    assert_eq!(undo.affected_paths, vec!["src/adjacent.rs"]);
    assert_eq!(
        fs::read_to_string(project.repo_root.join("src/adjacent.rs")).expect("read adjacent"),
        "left before\nright B\n"
    );

    let stale_write = project_store::validate_code_workspace_epoch_for_paths(
        &project.repo_root,
        &project.project_id,
        observed_epoch_before_undo,
        &["src/adjacent.rs".into()],
    )
    .expect_err("stale run should be blocked before refresh");
    assert_eq!(stale_write.code, "agent_workspace_epoch_stale");
    assert!(stale_write.message.contains("history-op-s36-adjacent-undo"));

    let adjacent_notice = inbox_item_for_path(
        &project,
        &project.run_b,
        "src/adjacent.rs",
        project_store::AgentMailboxItemType::ReservationInvalidated,
    );
    assert_eq!(
        adjacent_notice.priority,
        project_store::AgentMailboxPriority::High
    );

    let runtime = AutonomousToolRuntime::new(&project.repo_root)
        .expect("build autonomous tool runtime")
        .with_agent_run_context(
            project.project_id.clone(),
            project.session_b.clone(),
            project.run_b.clone(),
        );
    let mut acknowledge = coordination_request(AutonomousAgentCoordinationAction::Acknowledge);
    acknowledge.item_id = Some(adjacent_notice.item_id.clone());
    let ack_result = runtime
        .agent_coordination(acknowledge)
        .expect("acknowledge adjacent history notice");
    let AutonomousToolOutput::AgentCoordination(ack_output) = ack_result.output else {
        panic!("expected agent coordination output");
    };
    let refreshed_epoch = ack_output
        .code_workspace_epoch
        .expect("ack refreshes code workspace epoch");
    assert_eq!(ack_output.refreshed_paths, vec!["src/adjacent.rs"]);
    project_store::validate_code_workspace_epoch_for_paths(
        &project.repo_root,
        &project.project_id,
        refreshed_epoch,
        &["src/adjacent.rs".into()],
    )
    .expect("acknowledged epoch allows continued adjacent work");

    let mut renew = coordination_request(AutonomousAgentCoordinationAction::ClaimReservation);
    renew.paths = vec!["src/adjacent.rs".into()];
    renew.operation = Some(project_store::AgentCoordinationReservationOperation::Editing);
    renew.note = Some("Renewed after reading current file and acknowledging undo.".into());
    let renew_result = runtime
        .agent_coordination(renew)
        .expect("renew adjacent reservation");
    let AutonomousToolOutput::AgentCoordination(renew_output) = renew_result.output else {
        panic!("expected agent coordination output");
    };
    assert!(renew_output.conflicts.is_empty());
    assert_eq!(renew_output.reservations.len(), 1);
    assert_eq!(renew_output.reservations[0].path, "src/adjacent.rs");
    assert!(renew_output.reservations[0].invalidated_at.is_none());

    capture_modify(
        &project,
        &project.session_b,
        &project.run_b,
        "code-change-s36-b-continued",
        "session B continued after ack",
        "src/adjacent.rs",
        "left before\nright B continued\n",
    );
    assert_eq!(
        fs::read_to_string(project.repo_root.join("src/adjacent.rs")).expect("read continued"),
        "left before\nright B continued\n"
    );
}

struct HarnessProject {
    project_id: String,
    repo_root: PathBuf,
    session_a: String,
    run_a: String,
    session_b: String,
    run_b: String,
}

fn seed_project(root: &TempDir) -> HarnessProject {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("create repo src");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let project_id = "project-s36-multi-agent".to_string();

    db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
    db::import_project(
        &CanonicalRepository {
            project_id: project_id.clone(),
            repository_id: "repo-s36-multi-agent".into(),
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
        },
        DesktopState::default().import_failpoints(),
    )
    .expect("import project");

    let session_a = create_session(&canonical_root, &project_id, "Session A");
    let session_b = create_session(&canonical_root, &project_id, "Session B");
    let project = HarnessProject {
        project_id,
        repo_root: canonical_root,
        session_a,
        run_a: "run-s36-session-a".into(),
        session_b,
        run_b: "run-s36-session-b".into(),
    };
    seed_run(&project, &project.session_a, &project.run_a);
    seed_run(&project, &project.session_b, &project.run_b);
    project
}

fn create_session(repo_root: &Path, project_id: &str, title: &str) -> String {
    project_store::create_agent_session(
        repo_root,
        &project_store::AgentSessionCreateRecord {
            project_id: project_id.into(),
            title: title.into(),
            summary: "S36 multi-agent history harness".into(),
            selected: false,
        },
    )
    .expect("create agent session")
    .agent_session_id
}

fn seed_run(project: &HarnessProject, agent_session_id: &str, run_id: &str) {
    project_store::insert_agent_run(
        &project.repo_root,
        &project_store::NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: Some("engineer".into()),
            agent_definition_version: Some(1),
            project_id: project.project_id.clone(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
            provider_id: "fake-provider".into(),
            model_id: "fake-model".into(),
            prompt: "Coordinate concurrent code history work.".into(),
            system_prompt: "system".into(),
            now: "2026-05-07T12:00:00Z".into(),
        },
    )
    .expect("seed agent run");
}

fn activate_run(project: &HarnessProject, run_id: &str, summary: &str) {
    project_store::upsert_agent_coordination_presence(
        &project.repo_root,
        &project_store::UpsertAgentCoordinationPresenceRecord {
            project_id: project.project_id.clone(),
            run_id: run_id.into(),
            pane_id: None,
            status: "running".into(),
            current_phase: "s36".into(),
            activity_summary: summary.into(),
            last_event_id: None,
            last_event_kind: None,
            updated_at: now_timestamp(),
            lease_seconds: Some(3_600),
        },
    )
    .expect("activate run presence");
}

fn claim_reservations(project: &HarnessProject, run_id: &str, paths: &[&str]) {
    let result = project_store::claim_agent_file_reservations(
        &project.repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project.project_id.clone(),
            owner_run_id: run_id.into(),
            paths: paths.iter().map(|path| (*path).to_string()).collect(),
            operation: project_store::AgentCoordinationReservationOperation::Editing,
            note: Some("S36 concurrent history harness reservation".into()),
            override_reason: None,
            claimed_at: now_timestamp(),
            lease_seconds: Some(3_600),
        },
    )
    .expect("claim reservations");
    assert_eq!(result.claimed.len(), paths.len());
    assert!(result.conflicts.is_empty());
}

fn capture_modify(
    project: &HarnessProject,
    agent_session_id: &str,
    run_id: &str,
    change_group_id: &str,
    summary_label: &str,
    path: &str,
    after: &str,
) -> project_store::CompletedCodeChangeGroup {
    let handle = project_store::begin_exact_path_capture(
        &project.repo_root,
        project_store::CodeChangeGroupInput {
            project_id: project.project_id.clone(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
            change_group_id: Some(change_group_id.into()),
            parent_change_group_id: None,
            tool_call_id: Some(format!("tool-{change_group_id}")),
            runtime_event_id: None,
            conversation_sequence: None,
            change_kind: project_store::CodeChangeKind::FileTool,
            summary_label: summary_label.into(),
            restore_state: project_store::CodeChangeRestoreState::SnapshotAvailable,
        },
        vec![project_store::CodeRollbackCaptureTarget::modify(path)],
    )
    .expect("begin exact path capture");
    fs::write(project.repo_root.join(path), after).expect("write captured after content");
    project_store::complete_exact_path_capture(&project.repo_root, handle)
        .expect("complete exact path capture")
}

fn inbox_item_for_path(
    project: &HarnessProject,
    run_id: &str,
    path: &str,
    item_type: project_store::AgentMailboxItemType,
) -> project_store::AgentMailboxItemRecord {
    project_store::list_agent_mailbox_inbox(
        &project.repo_root,
        &project.project_id,
        run_id,
        &now_timestamp(),
        20,
    )
    .expect("list inbox")
    .into_iter()
    .find(|delivery| {
        delivery.item.item_type == item_type
            && delivery
                .item
                .related_paths
                .iter()
                .any(|related_path| related_path == path)
    })
    .unwrap_or_else(|| panic!("expected {item_type:?} inbox item for {path}"))
    .item
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
