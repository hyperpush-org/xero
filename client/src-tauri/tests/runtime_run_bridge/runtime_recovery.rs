use super::support::*;

pub(crate) fn get_runtime_run_returns_none_when_selected_project_has_no_durable_run() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime_run = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto { project_id },
    )
    .expect("get runtime run should succeed");

    assert!(runtime_run.is_none());
}

pub(crate) fn get_runtime_run_fails_closed_for_malformed_durable_rows_without_projection_event_drift(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_unreachable_runtime_run(&repo_root, &project_id, "run-corrupt");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints");
    connection
        .execute(
            "UPDATE runtime_runs SET status = 'bogus_status' WHERE project_id = ?1",
            [&project_id],
        )
        .expect("corrupt runtime run status");

    recorder.clear();
    let error = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect_err("malformed runtime run rows should fail closed");

    assert_eq!(error.code, "runtime_run_decode_failed");
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);
}

pub(crate) fn start_runtime_run_requires_authenticated_runtime_session() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    let error = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id,
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect_err("start runtime run should require auth binding");

    assert_eq!(error.code, "runtime_run_auth_required");
}

pub(crate) fn start_runtime_run_reconnects_existing_run_without_duplicate_launch_or_auth_event_drift(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    recorder.clear();

    let first = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start runtime run");
    assert_eq!(first.project_id, project_id);

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, first.run_id);

    let second = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("second start should reconnect");
    assert_eq!(second.run_id, first.run_id);
    assert_eq!(count_runtime_run_rows(&repo_root), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let auth_runtime = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime session after run start");
    assert_eq!(auth_runtime.phase, RuntimeAuthPhase::Authenticated);

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: first.run_id,
        },
    )
    .expect("stop runtime run should succeed")
    .expect("stopped runtime run should exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn get_runtime_run_recovers_truthful_running_state_after_fresh_host_reload() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start runtime run");

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);

    let recovered = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(recovered.run_id, launched.run_id);

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop recovered runtime run")
    .expect("recovered runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn get_runtime_run_recovers_stale_unreachable_state_once_after_fresh_host_reload() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    seed_unreachable_runtime_run(&repo_root, &project_id, "run-unreachable");

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recorder = attach_event_recorders(&fresh_app);

    let first = get_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after fresh-host restart")
    .expect("runtime run should exist after restart");
    assert_eq!(first.run_id, "run-unreachable");
    assert_eq!(first.status, RuntimeRunStatusDto::Stale);
    assert_eq!(
        first.transport.liveness,
        RuntimeRunTransportLivenessDto::Unreachable
    );
    assert_eq!(
        first.last_error_code.as_deref(),
        Some("runtime_supervisor_connect_failed")
    );
    assert_eq!(first.last_checkpoint_sequence, 1);
    assert_eq!(first.checkpoints.len(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 1);

    let runtime_session = get_runtime_session(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime session after stale recovery");
    assert_eq!(runtime_session.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(recorder.runtime_update_count(), 0);

    let second = get_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reload stale runtime run projection")
    .expect("runtime run should remain durable after stale recovery");

    assert_eq!(second.run_id, first.run_id);
    assert_eq!(second.status, first.status);
    assert_eq!(second.transport.liveness, first.transport.liveness);
    assert_eq!(second.last_error_code, first.last_error_code);
    assert_eq!(
        second.last_checkpoint_sequence,
        first.last_checkpoint_sequence
    );
    assert_eq!(second.checkpoints, first.checkpoints);
    assert_eq!(
        second.updated_at, first.updated_at,
        "unchanged stale projections should not rewrite durable runtime rows"
    );
    assert_eq!(
        recorder.runtime_run_update_count(),
        1,
        "unchanged stale recovery should not emit duplicate runtime-run updates"
    );
}

pub(crate) fn start_runtime_run_replaces_stale_row_with_new_reachable_run() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    seed_unreachable_runtime_run(&repo_root, &project_id, "run-stale");

    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start runtime run after stale row");
    assert_ne!(launched.run_id, "run-stale");

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, launched.run_id);

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop replacement runtime run")
    .expect("replacement runtime run should exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn stop_runtime_run_rejects_mismatched_run_id_and_marks_unreachable_sidecar_stale() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_unreachable_runtime_run(&repo_root, &project_id, "run-1");

    let mismatch = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id: project_id.clone(),
            run_id: "run-2".into(),
        },
    )
    .expect_err("mismatched run id should fail closed");
    assert_eq!(mismatch.code, "runtime_run_mismatch");

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: "run-1".into(),
        },
    )
    .expect("stop against unreachable sidecar should return durable snapshot")
    .expect("durable runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stale);
    assert_eq!(
        stopped.transport.liveness,
        RuntimeRunTransportLivenessDto::Unreachable
    );
    assert_eq!(
        stopped.last_error_code.as_deref(),
        Some("supervisor_stop_failed")
    );
}

pub(crate) fn stop_runtime_run_returns_existing_terminal_snapshot_after_sidecar_exit() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_failed_runtime_run(&repo_root, &project_id, "run-failed");

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: "run-failed".into(),
        },
    )
    .expect("stop after sidecar exit should succeed")
    .expect("terminal runtime run should still be returned");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Failed);
    assert_eq!(
        stopped.last_error_code.as_deref(),
        Some("runtime_supervisor_exit_nonzero")
    );
}
