use std::{fs, path::PathBuf};

use tempfile::TempDir;
use xero_desktop_lib::{
    commands::RuntimeAgentIdDto,
    db::{self, project_store},
    git::repository::CanonicalRepository,
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
