use std::{
    fs, thread,
    time::{Duration, Instant},
};

use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        cancel_agent_run, export_agent_trace, get_agent_run, list_agent_runs, reject_agent_action,
        resume_agent_run, send_agent_message, start_agent_task, AgentRunStatusDto,
        CancelAgentRunRequestDto, ExportAgentTraceRequestDto, GetAgentRunRequestDto,
        ListAgentRunsRequestDto, RejectAgentActionRequestDto, ResumeAgentRunRequestDto,
        RuntimeAgentIdDto, SendAgentMessageRequestDto, StartAgentTaskRequestDto,
    },
    configure_builder_with_state,
    db::{self, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::AgentProviderConfig,
    state::DesktopState,
};

fn build_mock_app(root: &TempDir) -> tauri::App<tauri::test::MockRuntime> {
    let state = DesktopState::default()
        .with_global_db_path_override(root.path().join("app-data").join("xero.db"))
        .with_owned_agent_provider_config_override(AgentProviderConfig::Fake);
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("build mock app")
}

fn seed_project(
    root: &TempDir,
    app: &tauri::App<tauri::test::MockRuntime>,
) -> (String, std::path::PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(&repo_root).expect("create repository root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repository root");
    let suffix = root
        .path()
        .file_name()
        .expect("fixture suffix")
        .to_string_lossy()
        .replace('.', "");
    let project_id = format!("project-agent-task-{suffix}");
    let repository_id = format!("repo-agent-task-{suffix}");
    let root_path = canonical_root.to_string_lossy().into_owned();
    let repository = CanonicalRepository {
        project_id: project_id.clone(),
        repository_id: repository_id.clone(),
        root_path: canonical_root.clone(),
        root_path_string: root_path.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "agent-task-fixture".into(),
        branch_name: None,
        head_sha: None,
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
        .global_db_path(&app.handle().clone())
        .expect("registry path");
    db::configure_project_database_paths(&registry_path);
    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import fixture project");
    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: project_id.clone(),
            repository_id,
            root_path,
            is_git_repo: false,
        }],
    )
    .expect("persist fixture registry");
    (project_id, canonical_root)
}

fn insert_run(repo_root: &std::path::Path, project_id: &str, run_id: &str) {
    project_store::insert_agent_run(
        repo_root,
        &project_store::NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: project_id.into(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: run_id.into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            prompt: "Exercise the agent task commands.".into(),
            system_prompt: "Fixture system prompt.".into(),
            now: "2026-07-18T20:00:00Z".into(),
        },
    )
    .expect("insert fixture run");
}

fn wait_for_run_status(
    repo_root: &std::path::Path,
    project_id: &str,
    run_id: &str,
    expected: project_store::AgentRunStatus,
) -> project_store::AgentRunSnapshotRecord {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match project_store::load_agent_run(repo_root, project_id, run_id) {
            Ok(snapshot) if snapshot.run.status == expected => return snapshot,
            Ok(snapshot) => assert!(
                Instant::now() < deadline,
                "run {run_id} did not reach {expected:?}; last status was {:?}; last error was {:?}",
                snapshot.run.status,
                snapshot.run.last_error
            ),
            Err(error) => assert!(
                Instant::now() < deadline,
                "run {run_id} was unavailable while waiting for {expected:?}: {error:?}"
            ),
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn wait_for_run_inactive(state: &DesktopState, run_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while state
        .agent_run_supervisor()
        .is_active(run_id)
        .expect("check run activity")
    {
        assert!(
            Instant::now() < deadline,
            "run {run_id} stayed active after reaching a terminal status"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn agent_task_commands_cover_validation_lookup_trace_rejection_listing_and_cancellation() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(&root);

    for request in [
        StartAgentTaskRequestDto {
            project_id: " ".into(),
            agent_session_id: "session".into(),
            run_id: None,
            prompt: "prompt".into(),
            controls: None,
            attachments: Vec::new(),
        },
        StartAgentTaskRequestDto {
            project_id: "project".into(),
            agent_session_id: "".into(),
            run_id: None,
            prompt: "prompt".into(),
            controls: None,
            attachments: Vec::new(),
        },
        StartAgentTaskRequestDto {
            project_id: "project".into(),
            agent_session_id: "session".into(),
            run_id: None,
            prompt: "\n".into(),
            controls: None,
            attachments: Vec::new(),
        },
        StartAgentTaskRequestDto {
            project_id: "project".into(),
            agent_session_id: "session".into(),
            run_id: Some(" ".into()),
            prompt: "prompt".into(),
            controls: None,
            attachments: Vec::new(),
        },
    ] {
        assert_eq!(
            start_agent_task(app.handle().clone(), app.state::<DesktopState>(), request)
                .expect_err("invalid start request")
                .code,
            "invalid_request"
        );
    }

    for request in [
        SendAgentMessageRequestDto {
            run_id: "".into(),
            continuation_request_id: "request".into(),
            prompt: "prompt".into(),
            attachments: Vec::new(),
            auto_compact: None,
        },
        SendAgentMessageRequestDto {
            run_id: "run".into(),
            continuation_request_id: " ".into(),
            prompt: "prompt".into(),
            attachments: Vec::new(),
            auto_compact: None,
        },
        SendAgentMessageRequestDto {
            run_id: "run".into(),
            continuation_request_id: "request".into(),
            prompt: "".into(),
            attachments: Vec::new(),
            auto_compact: None,
        },
    ] {
        assert_eq!(
            send_agent_message(app.handle().clone(), app.state::<DesktopState>(), request)
                .expect_err("invalid send request")
                .code,
            "invalid_request"
        );
    }

    assert_eq!(
        resume_agent_run(
            app.handle().clone(),
            app.state::<DesktopState>(),
            ResumeAgentRunRequestDto {
                run_id: "run".into(),
                continuation_request_id: "request".into(),
                response: "response".into(),
                action_id: Some(" ".into()),
                auto_compact: None,
            },
        )
        .expect_err("blank action id")
        .code,
        "invalid_request"
    );
    assert_eq!(
        cancel_agent_run(
            app.handle().clone(),
            app.state::<DesktopState>(),
            CancelAgentRunRequestDto { run_id: " ".into() },
        )
        .expect_err("blank cancellation run")
        .code,
        "invalid_request"
    );
    assert_eq!(
        reject_agent_action(
            app.handle().clone(),
            app.state::<DesktopState>(),
            RejectAgentActionRequestDto {
                run_id: "run".into(),
                action_id: "".into(),
                response: None,
            },
        )
        .expect_err("blank rejection action")
        .code,
        "invalid_request"
    );

    let (project_id, repo_root) = seed_project(&root, &app);
    insert_run(&repo_root, &project_id, "run-reject");
    insert_run(&repo_root, &project_id, "run-cancel");

    let fetched = get_agent_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentRunRequestDto {
            run_id: "run-reject".into(),
        },
    )
    .expect("get fixture run");
    assert_eq!(fetched.project_id, project_id);
    assert_eq!(fetched.status, AgentRunStatusDto::Starting);

    let listed = list_agent_runs(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ListAgentRunsRequestDto {
            project_id: project_id.clone(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
        },
    )
    .expect("list fixture runs");
    assert_eq!(listed.runs.len(), 2);

    let trace = export_agent_trace(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ExportAgentTraceRequestDto {
            run_id: "run-reject".into(),
            include_support_bundle: true,
        },
    )
    .expect("export fixture trace");
    assert!(trace.markdown_summary.contains("run-reject"));
    assert!(trace.support_bundle.is_some());

    project_store::append_agent_action_request(
        &repo_root,
        &project_store::NewAgentActionRequestRecord {
            project_id: project_id.clone(),
            run_id: "run-reject".into(),
            action_id: "action-1".into(),
            action_type: "approval".into(),
            title: "Approve fixture".into(),
            detail: "Fixture action".into(),
            created_at: "2026-07-18T20:00:01Z".into(),
        },
    )
    .expect("append fixture action");
    project_store::update_agent_run_status(
        &repo_root,
        &project_id,
        "run-reject",
        project_store::AgentRunStatus::Paused,
        None,
        "2026-07-18T20:00:02Z",
    )
    .expect("pause fixture run");
    let rejected = reject_agent_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RejectAgentActionRequestDto {
            run_id: "run-reject".into(),
            action_id: "action-1".into(),
            response: Some(" rejected by fixture ".into()),
        },
    )
    .expect("reject fixture action");
    assert_eq!(rejected.status, AgentRunStatusDto::Failed);
    assert_eq!(
        rejected.action_requests[0].response.as_deref(),
        Some("rejected by fixture")
    );

    let cancelled = cancel_agent_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CancelAgentRunRequestDto {
            run_id: "run-cancel".into(),
        },
    )
    .expect("cancel fixture run");
    assert_eq!(cancelled.status, AgentRunStatusDto::Cancelled);

    let missing = get_agent_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAgentRunRequestDto {
            run_id: "missing-run".into(),
        },
    )
    .expect_err("missing run lookup");
    assert_eq!(missing.code, "agent_run_not_found");
}

#[test]
fn agent_task_commands_start_send_and_resume_a_fake_provider_run() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(&root);
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "run-command-continuations";

    let started = start_agent_task(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartAgentTaskRequestDto {
            project_id: project_id.clone(),
            agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: Some(run_id.into()),
            prompt: "Start the command continuation fixture.".into(),
            controls: None,
            attachments: Vec::new(),
        },
    )
    .expect("start fake-provider command run");
    assert_eq!(started.run_id, run_id);
    assert_eq!(started.status, AgentRunStatusDto::Running);
    wait_for_run_status(
        &repo_root,
        &project_id,
        run_id,
        project_store::AgentRunStatus::Completed,
    );
    wait_for_run_inactive(app.state::<DesktopState>().inner(), run_id);

    let sent = send_agent_message(
        app.handle().clone(),
        app.state::<DesktopState>(),
        SendAgentMessageRequestDto {
            run_id: run_id.into(),
            continuation_request_id: "continuation-send-1".into(),
            prompt: "Continue through send_agent_message.".into(),
            attachments: Vec::new(),
            auto_compact: Some(xero_desktop_lib::commands::AgentAutoCompactPreferenceDto {
                enabled: true,
                threshold_percent: Some(80),
                raw_tail_message_count: Some(8),
            }),
        },
    )
    .expect("send fake-provider continuation");
    assert_eq!(sent.status, AgentRunStatusDto::Running);
    let after_send = wait_for_run_status(
        &repo_root,
        &project_id,
        run_id,
        project_store::AgentRunStatus::Completed,
    );
    assert!(after_send.messages.iter().any(|message| {
        message.role == project_store::AgentMessageRole::User
            && message.content == "Continue through send_agent_message."
    }));
    wait_for_run_inactive(app.state::<DesktopState>().inner(), run_id);

    let resumed = resume_agent_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeAgentRunRequestDto {
            run_id: run_id.into(),
            continuation_request_id: "continuation-resume-1".into(),
            response: "Continue through resume_agent_run.".into(),
            action_id: None,
            auto_compact: Some(xero_desktop_lib::commands::AgentAutoCompactPreferenceDto {
                enabled: false,
                threshold_percent: None,
                raw_tail_message_count: None,
            }),
        },
    )
    .expect("resume fake-provider command run");
    assert_eq!(resumed.status, AgentRunStatusDto::Running);
    let after_resume = wait_for_run_status(
        &repo_root,
        &project_id,
        run_id,
        project_store::AgentRunStatus::Completed,
    );
    assert!(after_resume.messages.iter().any(|message| {
        message.role == project_store::AgentMessageRole::User
            && message.content == "Continue through resume_agent_run."
    }));
    wait_for_run_inactive(app.state::<DesktopState>().inner(), run_id);
}
