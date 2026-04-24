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

pub(crate) fn start_runtime_run_reconnects_original_provider_run_after_active_profile_switch_and_fresh_host_reload(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}}]}"#,
        4,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_openai_compatible_profile(
        &app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&models_base_url),
        Some("2025-03-01-preview"),
        "sk-openai-runtime-secret",
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openai runtime session before run start");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "openai_api");

    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start openai runtime run");
    assert_eq!(launched.provider_id, "openai_api");
    assert_eq!(launched.runtime_kind, "openai_compatible");

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, launched.run_id);
    assert_eq!(running.provider_id, "openai_api");
    assert_eq!(running.runtime_kind, "openai_compatible");
    assert_eq!(running.controls.active.model_id, "gpt-4.1-mini");

    seed_anthropic_profile(
        &app,
        "anthropic-work",
        "claude-3-7-sonnet-latest",
        "sk-ant-api03-runtime-secret",
    );

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recovered = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(recovered.run_id, launched.run_id);
    assert_eq!(recovered.provider_id, "openai_api");
    assert_eq!(recovered.runtime_kind, "openai_compatible");
    assert_eq!(recovered.controls.active.model_id, "gpt-4.1-mini");

    let reconnected = start_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("profile switch should reconnect original durable run");
    assert_eq!(reconnected.run_id, launched.run_id);
    assert_eq!(reconnected.provider_id, "openai_api");
    assert_eq!(reconnected.runtime_kind, "openai_compatible");
    assert_eq!(count_runtime_run_rows(&repo_root), 1);

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop recovered openai runtime run")
    .expect("reconnected runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn start_runtime_run_fails_closed_when_active_profile_switches_provider_before_first_launch(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}}]}"#,
        2,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_openai_compatible_profile(
        &app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&models_base_url),
        Some("2025-03-01-preview"),
        "sk-openai-runtime-secret",
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openai runtime session before profile switch");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "openai_api");
    assert_eq!(runtime.runtime_kind, "openai_compatible");

    seed_anthropic_profile(
        &app,
        "anthropic-work",
        "claude-3-7-sonnet-latest",
        "sk-ant-api03-runtime-secret",
    );

    let error = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect_err("cross-provider launch should fail closed");
    assert_eq!(error.code, "runtime_supervisor_provider_mismatch");
    assert!(error.message.contains("anthropic"));
    assert!(error.message.contains("openai_api"));
    assert_eq!(count_runtime_run_rows(&repo_root), 0);

    let stored_runtime = project_store::load_runtime_session(&repo_root, &project_id)
        .expect("load persisted runtime session after mismatch")
        .expect("authenticated runtime session should remain persisted");
    assert_eq!(stored_runtime.provider_id, "openai_api");
    assert_eq!(stored_runtime.runtime_kind, "openai_compatible");
    assert_eq!(stored_runtime.auth_phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(
        stored_runtime.session_id.as_deref(),
        runtime.session_id.as_deref()
    );
}

pub(crate) fn start_runtime_run_fails_closed_for_stale_cross_provider_run_without_smearing_durable_identity(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}}]}"#,
        2,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_openai_compatible_profile(
        &app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&models_base_url),
        Some("2025-03-01-preview"),
        "sk-openai-runtime-secret",
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openai runtime session before stale recovery attempt");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "openai_api");

    seed_unreachable_runtime_run_with_identity(
        &repo_root,
        &project_id,
        "run-openai-stale",
        "openai_compatible",
        "openai_api",
        "gpt-4.1-mini",
        Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
    );

    seed_anthropic_profile(
        &app,
        "anthropic-work",
        "claude-3-7-sonnet-latest",
        "sk-ant-api03-runtime-secret",
    );

    let error = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect_err("stale cross-provider run should fail closed");
    assert_eq!(error.code, "runtime_supervisor_provider_mismatch");
    assert!(error.message.contains("run-openai-stale"));
    assert!(error.message.contains("openai_api"));
    assert!(error.message.contains("anthropic"));
    assert_eq!(count_runtime_run_rows(&repo_root), 1);

    let runtime_run = project_store::load_runtime_run(&repo_root, &project_id)
        .expect("load durable runtime run after mismatch")
        .expect("stale runtime run should remain persisted");
    assert_eq!(runtime_run.run.run_id, "run-openai-stale");
    assert_eq!(runtime_run.run.provider_id, "openai_api");
    assert_eq!(runtime_run.run.runtime_kind, "openai_compatible");
    assert_eq!(
        runtime_run.run.status,
        project_store::RuntimeRunStatus::Stale
    );
    assert_eq!(
        runtime_run.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Unreachable
    );
    assert_eq!(
        runtime_run
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_connect_failed")
    );
    assert_eq!(runtime_run.controls.active.model_id, "gpt-4.1-mini");

    let stored_runtime = project_store::load_runtime_session(&repo_root, &project_id)
        .expect("load persisted runtime session after stale mismatch")
        .expect("authenticated runtime session should remain persisted");
    assert_eq!(stored_runtime.provider_id, "openai_api");
    assert_eq!(stored_runtime.runtime_kind, "openai_compatible");
    assert_eq!(stored_runtime.auth_phase, RuntimeAuthPhase::Authenticated);
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

pub(crate) fn start_runtime_run_launches_anthropic_with_truthful_provider_identity_and_secret_free_persistence(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"claude-3-7-sonnet-latest","display_name":"Claude 3.7 Sonnet","capabilities":{"effort":{"supported":true,"medium":{"supported":true},"high":{"supported":true}}}}]}"#,
        3,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_anthropic_auth_config_override(anthropic_auth_config(format!(
        "{models_base_url}/v1/models"
    )));
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let secret = "sk-ant-api03-runtime-secret";

    seed_anthropic_profile(&app, "anthropic-work", "claude-3-7-sonnet-latest", secret);

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind anthropic runtime session before run start");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "anthropic");
    assert_eq!(runtime.runtime_kind, "anthropic");

    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start anthropic runtime run");
    assert_eq!(launched.provider_id, "anthropic");
    assert_eq!(launched.runtime_kind, "anthropic");

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, launched.run_id);
    assert_eq!(running.provider_id, "anthropic");
    assert_eq!(running.runtime_kind, "anthropic");
    assert_eq!(running.controls.active.model_id, "claude-3-7-sonnet-latest");
    assert_eq!(
        running.controls.active.thinking_effort,
        Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium)
    );

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recovered = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(recovered.run_id, launched.run_id);
    assert_eq!(recovered.provider_id, "anthropic");
    assert_eq!(recovered.runtime_kind, "anthropic");

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop recovered anthropic runtime run")
    .expect("anthropic runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn start_runtime_run_launches_bedrock_with_truthful_provider_identity_and_ambient_launch_metadata(
) {
    with_scoped_env(
        &[
            ("AWS_ACCESS_KEY_ID", Some("test-bedrock-access-key")),
            ("AWS_SECRET_ACCESS_KEY", Some("test-bedrock-secret-key")),
            ("AWS_SESSION_TOKEN", None),
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, repo_root) = seed_project(&root, &app);

            seed_ambient_anthropic_family_profile(
                &app,
                "bedrock-work",
                "bedrock",
                "anthropic.claude-3-7-sonnet-20250219-v1:0",
                "us-east-1",
                None,
            );

            let runtime = start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("bind bedrock runtime session before run start");
            assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
            assert_eq!(runtime.provider_id, "bedrock");
            assert_eq!(runtime.runtime_kind, "anthropic");

            let launched = start_runtime_run(
                app.handle().clone(),
                app.state::<DesktopState>(),
                StartRuntimeRunRequestDto {
                    project_id: project_id.clone(),
                    initial_controls: None,
                    initial_prompt: None,
                },
            )
            .expect("start bedrock runtime run");
            assert_eq!(launched.provider_id, "bedrock");
            assert_eq!(launched.runtime_kind, "anthropic");

            let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
                runtime_run.status == RuntimeRunStatusDto::Running
                    && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            });
            assert_eq!(running.run_id, launched.run_id);
            assert_eq!(running.provider_id, "bedrock");
            assert_eq!(running.runtime_kind, "anthropic");
            assert_eq!(
                running.controls.active.model_id,
                "anthropic.claude-3-7-sonnet-20250219-v1:0"
            );

            let database_bytes =
                std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
            let database_text = String::from_utf8_lossy(&database_bytes);
            assert!(!database_text.contains("test-bedrock-secret-key"));

            let stopped = stop_runtime_run(
                app.handle().clone(),
                app.state::<DesktopState>(),
                StopRuntimeRunRequestDto {
                    project_id,
                    run_id: launched.run_id,
                },
            )
            .expect("stop bedrock runtime run")
            .expect("bedrock runtime run should still exist");
            assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
        },
    );
}

pub(crate) fn start_runtime_run_launches_vertex_with_truthful_provider_identity_and_ambient_launch_metadata(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let adc_path = root.path().join("vertex-adc.json");
    std::fs::write(&adc_path, "{}\n").expect("write fake adc file");
    let adc_env = adc_path.to_string_lossy().into_owned();

    with_scoped_env(
        &[
            ("GOOGLE_APPLICATION_CREDENTIALS", Some(adc_env.as_str())),
            ("AWS_ACCESS_KEY_ID", None),
            ("AWS_SECRET_ACCESS_KEY", None),
        ],
        || {
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, _repo_root) = seed_project(&root, &app);

            seed_ambient_anthropic_family_profile(
                &app,
                "vertex-work",
                "vertex",
                "claude-3-7-sonnet@20250219",
                "us-central1",
                Some("vertex-project"),
            );

            let runtime = start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("bind vertex runtime session before run start");
            assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
            assert_eq!(runtime.provider_id, "vertex");
            assert_eq!(runtime.runtime_kind, "anthropic");

            let launched = start_runtime_run(
                app.handle().clone(),
                app.state::<DesktopState>(),
                StartRuntimeRunRequestDto {
                    project_id: project_id.clone(),
                    initial_controls: None,
                    initial_prompt: None,
                },
            )
            .expect("start vertex runtime run");
            assert_eq!(launched.provider_id, "vertex");
            assert_eq!(launched.runtime_kind, "anthropic");

            let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
                runtime_run.status == RuntimeRunStatusDto::Running
                    && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            });
            assert_eq!(running.run_id, launched.run_id);
            assert_eq!(running.provider_id, "vertex");
            assert_eq!(running.runtime_kind, "anthropic");
            assert_eq!(
                running.controls.active.model_id,
                "claude-3-7-sonnet@20250219"
            );

            let stopped = stop_runtime_run(
                app.handle().clone(),
                app.state::<DesktopState>(),
                StopRuntimeRunRequestDto {
                    project_id,
                    run_id: launched.run_id,
                },
            )
            .expect("stop vertex runtime run")
            .expect("vertex runtime run should still exist");
            assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
        },
    );
}

pub(crate) fn start_runtime_run_surfaces_typed_vertex_adc_missing_diagnostic_before_launch() {
    with_scoped_env(
        &[
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
            ("AWS_ACCESS_KEY_ID", None),
            ("AWS_SECRET_ACCESS_KEY", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, _repo_root) = seed_project(&root, &app);

            seed_ambient_anthropic_family_profile(
                &app,
                "vertex-work",
                "vertex",
                "claude-3-7-sonnet@20250219",
                "us-central1",
                Some("vertex-project"),
            );

            let runtime = start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("surface vertex adc diagnostic before run start");
            assert_eq!(runtime.phase, RuntimeAuthPhase::Idle);
            assert_eq!(
                runtime.last_error_code.as_deref(),
                Some("vertex_adc_missing")
            );

            let error = start_runtime_run(
                app.handle().clone(),
                app.state::<DesktopState>(),
                StartRuntimeRunRequestDto {
                    project_id,
                    initial_controls: None,
                    initial_prompt: None,
                },
            )
            .expect_err("vertex run start should fail before detached launch without adc");
            assert_eq!(error.code, "runtime_run_auth_required");
        },
    );
}

pub(crate) fn start_runtime_run_launches_openai_compatible_with_truthful_provider_identity_and_secret_free_persistence(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}}]}"#,
        3,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let secret = "sk-openai-runtime-secret";

    seed_openai_compatible_profile(
        &app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&models_base_url),
        Some("2025-03-01-preview"),
        secret,
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openai-compatible runtime session before run start");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "openai_api");
    assert_eq!(runtime.runtime_kind, "openai_compatible");

    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start openai-compatible runtime run");
    assert_eq!(launched.provider_id, "openai_api");
    assert_eq!(launched.runtime_kind, "openai_compatible");

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, launched.run_id);
    assert_eq!(running.provider_id, "openai_api");
    assert_eq!(running.runtime_kind, "openai_compatible");
    assert_eq!(running.controls.active.model_id, "gpt-4.1-mini");
    assert_eq!(
        running.controls.active.thinking_effort,
        Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium)
    );

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recovered = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(recovered.run_id, launched.run_id);
    assert_eq!(recovered.provider_id, "openai_api");
    assert_eq!(recovered.runtime_kind, "openai_compatible");

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop recovered openai-compatible runtime run")
    .expect("openai-compatible runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn start_runtime_run_launches_ollama_with_truthful_provider_identity_and_secret_free_persistence(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"llama3.2","display_name":"Llama 3.2"}]}"#,
        3,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: "ollama-work".into(),
            provider_id: "ollama".into(),
            runtime_kind: "openai_compatible".into(),
            label: "Ollama".into(),
            model_id: "llama3.2".into(),
            preset_id: Some("ollama".into()),
            base_url: Some(format!("{models_base_url}/v1")),
            api_version: None,
            region: None,
            project_id: None,
            api_key: None,
            activate: true,
        },
    )
    .expect("save ollama provider profile");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind ollama runtime session before run start");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "ollama");
    assert_eq!(runtime.runtime_kind, "openai_compatible");

    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start ollama runtime run");
    assert_eq!(launched.provider_id, "ollama");
    assert_eq!(launched.runtime_kind, "openai_compatible");

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, launched.run_id);
    assert_eq!(running.provider_id, "ollama");
    assert_eq!(running.runtime_kind, "openai_compatible");
    assert_eq!(running.controls.active.model_id, "llama3.2");
    assert_eq!(running.controls.active.thinking_effort, None);

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains("OPENAI_API_KEY"));

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recovered = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(recovered.run_id, launched.run_id);
    assert_eq!(recovered.provider_id, "ollama");
    assert_eq!(recovered.runtime_kind, "openai_compatible");

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop recovered ollama runtime run")
    .expect("ollama runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn start_runtime_run_launches_github_models_with_truthful_provider_identity_and_secret_free_persistence(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let catalog_base_url = spawn_static_http_server_for_requests(
        200,
        r#"[{"id":"openai/gpt-4.1","name":"OpenAI GPT-4.1","capabilities":["streaming","tool-calling"]}]"#,
        3,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let state = state.with_openai_compatible_auth_config_override(OpenAiCompatibleAuthConfig {
        github_models_catalog_url: format!("{catalog_base_url}/catalog/models"),
        ..OpenAiCompatibleAuthConfig::default()
    });
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let secret = "github_pat_runtime_secret";

    seed_openai_compatible_profile(
        &app,
        "github-models-work",
        "github_models",
        "openai_compatible",
        "openai/gpt-4.1",
        Some("github_models"),
        None,
        None,
        secret,
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind github models runtime session before run start");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "github_models");
    assert_eq!(runtime.runtime_kind, "openai_compatible");

    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start github models runtime run");
    assert_eq!(launched.provider_id, "github_models");
    assert_eq!(launched.runtime_kind, "openai_compatible");

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, launched.run_id);
    assert_eq!(running.provider_id, "github_models");
    assert_eq!(running.runtime_kind, "openai_compatible");
    assert_eq!(running.controls.active.model_id, "openai/gpt-4.1");
    assert_eq!(running.controls.active.thinking_effort, None);

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recovered = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(recovered.run_id, launched.run_id);
    assert_eq!(recovered.provider_id, "github_models");
    assert_eq!(recovered.runtime_kind, "openai_compatible");

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop recovered github models runtime run")
    .expect("github models runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn start_runtime_run_projects_connected_mcp_servers_into_run_scoped_projection_contract()
{
    with_scoped_env(
        &[("MCP_CONNECTED_TOKEN", Some("runtime-mcp-secret-value"))],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let models_base_url = spawn_static_http_server_for_requests(
                200,
                r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}}]}"#,
                3,
            );
            let (state, _registry_path, _auth_store_path) = create_state(&root);
            let app = build_mock_app(state);
            let (project_id, repo_root) = seed_project(&root, &app);

            let mut mcp_registry = cadence_desktop_lib::mcp::default_mcp_registry();
            mcp_registry.servers = vec![
                cadence_desktop_lib::mcp::McpServerRecord {
                    id: "mcp-connected".into(),
                    name: "Connected MCP".into(),
                    transport: cadence_desktop_lib::mcp::McpTransport::Stdio {
                        command: "npx".into(),
                        args: vec!["-y".into(), "@acme/connected-mcp".into()],
                    },
                    env: vec![cadence_desktop_lib::mcp::McpEnvironmentReference {
                        key: "TOKEN".into(),
                        from_env: "MCP_CONNECTED_TOKEN".into(),
                    }],
                    cwd: None,
                    connection: cadence_desktop_lib::mcp::McpConnectionState {
                        status: cadence_desktop_lib::mcp::McpConnectionStatus::Connected,
                        diagnostic: None,
                        last_checked_at: Some("2026-04-20T10:00:00Z".into()),
                        last_healthy_at: Some("2026-04-20T10:00:00Z".into()),
                    },
                    updated_at: "2026-04-20T10:00:00Z".into(),
                },
                cadence_desktop_lib::mcp::McpServerRecord {
                    id: "mcp-stale".into(),
                    name: "Stale MCP".into(),
                    transport: cadence_desktop_lib::mcp::McpTransport::Http {
                        url: "https://stale.example.com/mcp".into(),
                    },
                    env: Vec::new(),
                    cwd: None,
                    connection: cadence_desktop_lib::mcp::McpConnectionState {
                        status: cadence_desktop_lib::mcp::McpConnectionStatus::Stale,
                        diagnostic: Some(cadence_desktop_lib::mcp::McpConnectionDiagnostic {
                            code: "mcp_status_unchecked".into(),
                            message: "Server has not been checked yet.".into(),
                            retryable: true,
                        }),
                        last_checked_at: None,
                        last_healthy_at: None,
                    },
                    updated_at: "2026-04-20T10:00:01Z".into(),
                },
                cadence_desktop_lib::mcp::McpServerRecord {
                    id: "mcp-failed".into(),
                    name: "Failed MCP".into(),
                    transport: cadence_desktop_lib::mcp::McpTransport::Sse {
                        url: "https://failed.example.com/sse".into(),
                    },
                    env: Vec::new(),
                    cwd: None,
                    connection: cadence_desktop_lib::mcp::McpConnectionState {
                        status: cadence_desktop_lib::mcp::McpConnectionStatus::Failed,
                        diagnostic: Some(cadence_desktop_lib::mcp::McpConnectionDiagnostic {
                            code: "mcp_connect_failed".into(),
                            message: "Probe failed".into(),
                            retryable: true,
                        }),
                        last_checked_at: Some("2026-04-20T10:00:02Z".into()),
                        last_healthy_at: None,
                    },
                    updated_at: "2026-04-20T10:00:02Z".into(),
                },
            ];
            persist_mcp_registry_snapshot(&app, &mcp_registry);

            seed_openai_compatible_profile(
                &app,
                "openai-compatible-work",
                "openai_api",
                "openai_compatible",
                "gpt-4.1-mini",
                Some("openai_api"),
                Some(&models_base_url),
                Some("2025-03-01-preview"),
                "sk-openai-runtime-secret",
            );

            let runtime = start_runtime_session(
                app.handle().clone(),
                app.state::<DesktopState>(),
                ProjectIdRequestDto {
                    project_id: project_id.clone(),
                },
            )
            .expect("bind openai-compatible runtime session before run start");
            assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
            assert_eq!(runtime.provider_id, "openai_api");

            let launched = start_runtime_run(
                app.handle().clone(),
                app.state::<DesktopState>(),
                StartRuntimeRunRequestDto {
                    project_id: project_id.clone(),
                    initial_controls: None,
                    initial_prompt: None,
                },
            )
            .expect("start runtime run with mcp projection");

            let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
                runtime_run.status == RuntimeRunStatusDto::Running
                    && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            });
            assert_eq!(running.run_id, launched.run_id);

            let projection_path = runtime_mcp_projection_path(&app, &launched.run_id);
            assert!(projection_path.is_file());

            let projection = load_runtime_mcp_projection_snapshot(&app, &launched.run_id);
            assert_eq!(projection.servers.len(), 1);
            assert_eq!(projection.servers[0].id, "mcp-connected");
            assert!(projection
                .servers
                .iter()
                .all(|server| server.connection.status
                    == cadence_desktop_lib::mcp::McpConnectionStatus::Connected));

            let projection_text = std::fs::read_to_string(&projection_path)
                .expect("read runtime mcp projection file");
            assert!(!projection_text.contains("runtime-mcp-secret-value"));
            assert!(projection_text.contains("MCP_CONNECTED_TOKEN"));

            let database_bytes =
                std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db bytes");
            let database_text = String::from_utf8_lossy(&database_bytes);
            assert!(!database_text.contains("runtime-mcp-secret-value"));

            let stopped = stop_runtime_run(
                app.handle().clone(),
                app.state::<DesktopState>(),
                StopRuntimeRunRequestDto {
                    project_id,
                    run_id: launched.run_id,
                },
            )
            .expect("stop runtime run with mcp projection")
            .expect("runtime run with mcp projection should still exist");
            assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
        },
    );
}

pub(crate) fn start_runtime_run_fails_closed_when_mcp_registry_snapshot_is_unreadable() {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}}]}"#,
        2,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let unreadable_registry_path = root.path().join("app-data").join("mcp-registry-unreadable");
    std::fs::create_dir_all(&unreadable_registry_path).expect("create unreadable mcp registry dir");
    let state = state.with_mcp_registry_file_override(unreadable_registry_path);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_openai_compatible_profile(
        &app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&models_base_url),
        Some("2025-03-01-preview"),
        "sk-openai-runtime-secret",
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openai-compatible runtime session before fail-closed launch");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);

    let error = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect_err("unreadable mcp registry should fail closed");
    assert_eq!(error.code, "runtime_mcp_projection_failed");
    assert!(error.message.contains("mcp_registry_read_failed"));
    assert_eq!(count_runtime_run_rows(&repo_root), 0);
}

pub(crate) fn start_runtime_run_fails_closed_when_mcp_registry_snapshot_is_malformed() {
    let root = tempfile::tempdir().expect("temp dir");
    let models_base_url = spawn_static_http_server_for_requests(
        200,
        r#"{"data":[{"id":"gpt-4.1-mini","display_name":"GPT-4.1 Mini","capabilities":{"reasoning":{"supported":true,"effortOptions":["low","medium","high"],"defaultEffort":"medium"}}}]}"#,
        2,
    );
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let malformed_registry_path = root
        .path()
        .join("app-data")
        .join("mcp-registry-malformed.json");
    std::fs::create_dir_all(
        malformed_registry_path
            .parent()
            .expect("malformed registry parent path"),
    )
    .expect("create malformed mcp registry parent");
    std::fs::write(
        &malformed_registry_path,
        r#"{
  "version": 1,
  "servers": [
    {
      "id": "bad id",
      "name": "Bad Server",
      "transport": { "kind": "stdio", "command": "npx", "args": ["-y", "bad"] },
      "connection": { "status": "connected" },
      "updatedAt": "2026-04-20T10:00:00Z"
    }
  ],
  "updatedAt": "2026-04-20T10:00:00Z"
}
"#,
    )
    .expect("write malformed mcp registry snapshot");
    let state = state.with_mcp_registry_file_override(malformed_registry_path);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_openai_compatible_profile(
        &app,
        "openai-compatible-work",
        "openai_api",
        "openai_compatible",
        "gpt-4.1-mini",
        Some("openai_api"),
        Some(&models_base_url),
        Some("2025-03-01-preview"),
        "sk-openai-runtime-secret",
    );

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("bind openai-compatible runtime session before malformed projection launch");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);

    let error = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id,
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect_err("malformed mcp registry should fail closed");
    assert_eq!(error.code, "runtime_mcp_projection_failed");
    assert!(error.message.contains("mcp_registry_invalid"));
    assert_eq!(count_runtime_run_rows(&repo_root), 0);
}
