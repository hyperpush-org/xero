use super::support::*;

pub(crate) fn detached_supervisor_launches_and_recovers_after_fresh_host_probe() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-1",
            &runtime_shell::script_print_line_and_sleep("hello from detached supervisor", 5),
        ),
    )
    .expect("launch detached runtime supervisor");

    assert_eq!(
        launched.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert!(!launched.run.transport.endpoint.is_empty());
    assert_eq!(
        state
            .runtime_supervisor_controller()
            .snapshot(project_id)
            .as_ref()
            .map(|snapshot| snapshot.run_id.as_str()),
        Some("run-1")
    );

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.run.transport.liveness
                == project_store::RuntimeRunTransportLiveness::Reachable
            && snapshot.last_checkpoint_sequence >= 1
    });
    assert!(running.run.last_heartbeat_at.is_some());

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe with fresh host state")
        .expect("runtime run should still exist");
    assert_eq!(recovered.run.run_id, "run-1");
    assert_eq!(
        recovered.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert_eq!(
        recovered.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Reachable
    );
    assert!(recovered.run.last_heartbeat_at.is_some());
    assert!(recovered.last_checkpoint_sequence >= 1);

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop detached runtime supervisor")
        .expect("stopped runtime run should exist");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
    assert!(stopped.run.stopped_at.is_some());
}

pub(crate) fn detached_supervisor_probe_marks_unreachable_run_stale() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-stale".into(),
                runtime_kind: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Unknown,
                },
                started_at: "2026-04-15T19:00:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T19:00:10Z".into(),
            },
            checkpoint: None,
            control_state: Some(sample_runtime_run_controls("2026-04-15T19:00:10Z")),
        },
    )
    .expect("seed unreachable runtime run");

    let state = DesktopState::default();
    let recovered = probe_runtime_run(&state, probe_request(project_id, &repo_root))
        .expect("probe stale runtime run")
        .expect("runtime run should exist after stale probe");

    assert_eq!(recovered.run.status, project_store::RuntimeRunStatus::Stale);
    assert_eq!(
        recovered.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Unreachable
    );
    assert_eq!(
        recovered
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_connect_failed")
    );
}

pub(crate) fn detached_supervisor_rejects_missing_shell_program() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    let error = launch_detached_runtime_supervisor(
        &state,
        RuntimeSupervisorLaunchRequest {
            project_id: project_id.into(),
            repo_root,
            runtime_kind: "openai_codex".into(),
            run_id: "run-invalid".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            program: String::new(),
            args: Vec::new(),
            startup_timeout: Duration::from_secs(5),
            control_timeout: Duration::from_millis(750),
            supervisor_binary: Some(supervisor_binary_path()),
            run_controls: sample_runtime_run_controls("2026-04-15T19:00:00Z"),
        },
    )
    .expect_err("missing shell program should fail");

    assert_eq!(error.code, "invalid_request");
}

pub(crate) fn detached_supervisor_rejects_duplicate_running_project_launches() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-1",
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect("launch first detached runtime supervisor");
    assert_eq!(
        launched.run.status,
        project_store::RuntimeRunStatus::Running
    );

    let error = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-2",
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect_err("duplicate launch should fail");
    assert_eq!(error.code, "runtime_run_already_active");

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop first detached runtime supervisor")
        .expect("runtime run should exist after stop");
    assert!(matches!(
        stopped.run.status,
        project_store::RuntimeRunStatus::Stopped | project_store::RuntimeRunStatus::Stale
    ));
}

pub(crate) fn detached_supervisor_marks_fast_nonzero_exit_as_failed_without_live_attach() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-fast-exit",
            &runtime_shell::script_exit(17),
        ),
    )
    .expect("launch fast-exit detached runtime supervisor");

    let terminal = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Failed
    });
    assert_eq!(terminal.run.run_id, "run-fast-exit");
    assert_eq!(
        terminal
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_exit_nonzero")
    );
    assert!(
        terminal.run.transport.liveness == project_store::RuntimeRunTransportLiveness::Reachable
    );
}
