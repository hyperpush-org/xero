use serde_json::json;
use std::path::PathBuf;
use xero_desktop_lib::{
    commands::RuntimeAgentIdDto,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    state::DesktopState,
};

#[test]
fn agent_run_wakeups_persist_reschedule_and_fire_in_app_data_project_db() {
    let root = tempfile::tempdir().expect("temp dir");
    let app_data_dir = root.path().join("app-data");
    let project_id = "project-1";
    let run_id = "run-wakeup-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let database_path = database_path_for_repo(&repo_root);

    assert!(database_path.starts_with(&app_data_dir));
    assert!(!repo_root.join(".xero").exists());

    project_store::insert_agent_run(
        &repo_root,
        &project_store::NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: Some("engineer".into()),
            agent_definition_version: Some(1),
            project_id: project_id.into(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: run_id.into(),
            provider_id: "test-provider".into(),
            model_id: "test-model".into(),
            prompt: "Wait briefly, then continue.".into(),
            system_prompt: "xero-owned-agent-v1".into(),
            now: "2026-04-24T12:00:00Z".into(),
        },
    )
    .expect("insert agent run");

    let payload = json!({
        "schema": "xero.agent_run_wakeup.payload.v1",
        "kind": "process_output",
        "reason": "Poll build output.",
        "processId": "proc-1",
        "outputPattern": "Finished",
    });
    let inserted = project_store::insert_agent_run_wakeup(
        &repo_root,
        &project_store::NewAgentRunWakeupRecord {
            project_id: project_id.into(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: run_id.into(),
            wake_id: "wake-1".into(),
            kind: project_store::AgentRunWakeupKind::ProcessOutput,
            due_at: "2026-04-24T12:00:10Z".into(),
            deadline_at: Some("2026-04-24T12:05:00Z".into()),
            poll_interval_ms: Some(5_000),
            payload_json: payload.to_string(),
            created_at: "2026-04-24T12:00:00Z".into(),
        },
    )
    .expect("insert wakeup");

    assert_eq!(
        inserted.kind,
        project_store::AgentRunWakeupKind::ProcessOutput
    );
    assert_eq!(
        inserted.status,
        project_store::AgentRunWakeupStatus::Pending
    );
    assert_eq!(
        inserted.payload().expect("decode payload")["reason"],
        "Poll build output."
    );
    assert_eq!(
        project_store::list_pending_agent_run_wakeups_for_run(&repo_root, project_id, run_id)
            .expect("list pending wakeups for run")
            .len(),
        1
    );
    assert!(project_store::maybe_load_pending_agent_run_wakeup(
        &repo_root, project_id, run_id, "wake-1",
    )
    .expect("load pending wakeup")
    .is_some());

    let rescheduled_payload = json!({
        "schema": "xero.agent_run_wakeup.payload.v1",
        "kind": "process_output",
        "reason": "Poll build output.",
        "processId": "proc-1",
        "outputPattern": "Finished",
        "afterCursor": 42,
    });
    let rescheduled = project_store::reschedule_agent_run_wakeup(
        &repo_root,
        project_id,
        run_id,
        "wake-1",
        "2026-04-24T12:00:15Z",
        &rescheduled_payload.to_string(),
        "2026-04-24T12:00:10Z",
    )
    .expect("reschedule wakeup");

    assert_eq!(rescheduled.attempt_count, 1);
    assert_eq!(rescheduled.due_at, "2026-04-24T12:00:15Z");
    assert_eq!(
        rescheduled.payload().expect("decode rescheduled payload")["afterCursor"],
        42
    );

    assert!(project_store::mark_agent_run_wakeup_fired(
        &repo_root,
        project_id,
        run_id,
        "wake-1",
        "2026-04-24T12:00:15Z",
    )
    .expect("fire wakeup"));
    let fired = project_store::load_agent_run_wakeup(&repo_root, project_id, run_id, "wake-1")
        .expect("load fired wakeup");

    assert_eq!(fired.status, project_store::AgentRunWakeupStatus::Fired);
    assert_eq!(fired.attempt_count, 2);
    assert_eq!(fired.fired_at.as_deref(), Some("2026-04-24T12:00:15Z"));
    assert!(project_store::list_pending_agent_run_wakeups(&repo_root)
        .expect("list pending wakeups")
        .is_empty());
    assert!(!project_store::mark_agent_run_wakeup_fired(
        &repo_root,
        project_id,
        run_id,
        "wake-1",
        "2026-04-24T12:00:20Z",
    )
    .expect("second fire is ignored"));
}

fn seed_project(
    root: &tempfile::TempDir,
    project_id: &str,
    repository_id: &str,
    repo_name: &str,
) -> PathBuf {
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

    db::configure_project_database_paths(&root.path().join("app-data").join("xero.db"));
    let state = DesktopState::default();
    db::import_project(&repository, state.import_failpoints()).expect("import project");

    canonical_root
}
