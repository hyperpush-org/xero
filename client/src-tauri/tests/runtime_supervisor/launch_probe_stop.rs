use super::support::*;

fn anthropic_environment_report_script() -> String {
    if cfg!(windows) {
        [
            "if not \"%ANTHROPIC_API_KEY%\"==\"\" (echo key-present) else (echo key-missing)"
                .to_string(),
            "echo provider:%CADENCE_RUNTIME_PROVIDER_ID%".to_string(),
            "echo session:%CADENCE_RUNTIME_SESSION_ID%".to_string(),
            "echo model:%CADENCE_RUNTIME_MODEL_ID%".to_string(),
            "echo thinking:%CADENCE_RUNTIME_THINKING_EFFORT%".to_string(),
            "timeout /T 5 /NOBREAK > NUL".to_string(),
        ]
        .join(" & ")
    } else {
        [
            "if [ -n \"$ANTHROPIC_API_KEY\" ]; then printf '%s\\n' 'key-present'; else printf '%s\\n' 'key-missing'; fi"
                .to_string(),
            "printf '%s\\n' \"provider:$CADENCE_RUNTIME_PROVIDER_ID\"".to_string(),
            "printf '%s\\n' \"session:$CADENCE_RUNTIME_SESSION_ID\"".to_string(),
            "printf '%s\\n' \"model:$CADENCE_RUNTIME_MODEL_ID\"".to_string(),
            "printf '%s\\n' \"thinking:$CADENCE_RUNTIME_THINKING_EFFORT\"".to_string(),
            "sleep 5".to_string(),
        ]
        .join("; ")
    }
}

fn openai_compatible_environment_report_script() -> String {
    if cfg!(windows) {
        [
            "if not \"%OPENAI_API_KEY%\"==\"\" (echo key-present) else (echo key-missing)"
                .to_string(),
            "echo base:%OPENAI_BASE_URL%".to_string(),
            "echo version:%OPENAI_API_VERSION%".to_string(),
            "echo provider:%CADENCE_RUNTIME_PROVIDER_ID%".to_string(),
            "echo session:%CADENCE_RUNTIME_SESSION_ID%".to_string(),
            "echo model:%CADENCE_RUNTIME_MODEL_ID%".to_string(),
            "echo thinking:%CADENCE_RUNTIME_THINKING_EFFORT%".to_string(),
            "timeout /T 5 /NOBREAK > NUL".to_string(),
        ]
        .join(" & ")
    } else {
        [
            "if [ -n \"$OPENAI_API_KEY\" ]; then printf '%s\\n' 'key-present'; else printf '%s\\n' 'key-missing'; fi"
                .to_string(),
            "printf '%s\\n' \"base:$OPENAI_BASE_URL\"".to_string(),
            "printf '%s\\n' \"version:$OPENAI_API_VERSION\"".to_string(),
            "printf '%s\\n' \"provider:$CADENCE_RUNTIME_PROVIDER_ID\"".to_string(),
            "printf '%s\\n' \"session:$CADENCE_RUNTIME_SESSION_ID\"".to_string(),
            "printf '%s\\n' \"model:$CADENCE_RUNTIME_MODEL_ID\"".to_string(),
            "printf '%s\\n' \"thinking:$CADENCE_RUNTIME_THINKING_EFFORT\"".to_string(),
            "sleep 5".to_string(),
        ]
        .join("; ")
    }
}

fn bedrock_environment_report_script() -> String {
    if cfg!(windows) {
        [
            "if not \"%AWS_ACCESS_KEY_ID%\"==\"\" (echo aws-auth-present) else (echo aws-auth-missing)".to_string(),
            "echo mode:%CLAUDE_CODE_USE_BEDROCK%".to_string(),
            "echo region:%AWS_REGION%".to_string(),
            "echo default-region:%AWS_DEFAULT_REGION%".to_string(),
            "echo provider:%CADENCE_RUNTIME_PROVIDER_ID%".to_string(),
            "echo session:%CADENCE_RUNTIME_SESSION_ID%".to_string(),
            "echo model:%CADENCE_RUNTIME_MODEL_ID%".to_string(),
            "echo thinking:%CADENCE_RUNTIME_THINKING_EFFORT%".to_string(),
            "timeout /T 5 /NOBREAK > NUL".to_string(),
        ]
        .join(" & ")
    } else {
        [
            "if [ -n \"$AWS_ACCESS_KEY_ID\" ] || [ -n \"$AWS_PROFILE\" ]; then printf '%s\\n' 'aws-auth-present'; else printf '%s\\n' 'aws-auth-missing'; fi".to_string(),
            "printf '%s\\n' \"mode:$CLAUDE_CODE_USE_BEDROCK\"".to_string(),
            "printf '%s\\n' \"region:$AWS_REGION\"".to_string(),
            "printf '%s\\n' \"default-region:$AWS_DEFAULT_REGION\"".to_string(),
            "printf '%s\\n' \"provider:$CADENCE_RUNTIME_PROVIDER_ID\"".to_string(),
            "printf '%s\\n' \"session:$CADENCE_RUNTIME_SESSION_ID\"".to_string(),
            "printf '%s\\n' \"model:$CADENCE_RUNTIME_MODEL_ID\"".to_string(),
            "printf '%s\\n' \"thinking:$CADENCE_RUNTIME_THINKING_EFFORT\"".to_string(),
            "sleep 5".to_string(),
        ]
        .join("; ")
    }
}

fn vertex_environment_report_script() -> String {
    if cfg!(windows) {
        [
            "if not \"%GOOGLE_APPLICATION_CREDENTIALS%\"==\"\" (echo adc-present) else (echo adc-missing)".to_string(),
            "echo mode:%CLAUDE_CODE_USE_VERTEX%".to_string(),
            "echo region:%CLOUD_ML_REGION%".to_string(),
            "echo project:%ANTHROPIC_VERTEX_PROJECT_ID%".to_string(),
            "echo provider:%CADENCE_RUNTIME_PROVIDER_ID%".to_string(),
            "echo session:%CADENCE_RUNTIME_SESSION_ID%".to_string(),
            "echo model:%CADENCE_RUNTIME_MODEL_ID%".to_string(),
            "echo thinking:%CADENCE_RUNTIME_THINKING_EFFORT%".to_string(),
            "timeout /T 5 /NOBREAK > NUL".to_string(),
        ]
        .join(" & ")
    } else {
        [
            "if [ -n \"$GOOGLE_APPLICATION_CREDENTIALS\" ]; then printf '%s\\n' 'adc-present'; else printf '%s\\n' 'adc-missing'; fi".to_string(),
            "printf '%s\\n' \"mode:$CLAUDE_CODE_USE_VERTEX\"".to_string(),
            "printf '%s\\n' \"region:$CLOUD_ML_REGION\"".to_string(),
            "printf '%s\\n' \"project:$ANTHROPIC_VERTEX_PROJECT_ID\"".to_string(),
            "printf '%s\\n' \"provider:$CADENCE_RUNTIME_PROVIDER_ID\"".to_string(),
            "printf '%s\\n' \"session:$CADENCE_RUNTIME_SESSION_ID\"".to_string(),
            "printf '%s\\n' \"model:$CADENCE_RUNTIME_MODEL_ID\"".to_string(),
            "printf '%s\\n' \"thinking:$CADENCE_RUNTIME_THINKING_EFFORT\"".to_string(),
            "sleep 5".to_string(),
        ]
        .join("; ")
    }
}

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
                provider_id: "openai_codex".into(),
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

pub(crate) fn detached_supervisor_probe_preserves_provider_identity_after_stale_recovery() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-gemini-stale";
    let repo_root = seed_project(&root, project_id, "repo-gemini-stale", "repo");

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-gemini-stale".into(),
                runtime_kind: "gemini".into(),
                provider_id: "gemini_ai_studio".into(),
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
            checkpoint: Some(project_store::RuntimeRunCheckpointRecord {
                project_id: project_id.into(),
                run_id: "run-gemini-stale".into(),
                sequence: 1,
                kind: project_store::RuntimeRunCheckpointKind::Bootstrap,
                summary: "Supervisor boot recorded.".into(),
                created_at: "2026-04-15T19:00:10Z".into(),
            }),
            control_state: Some(sample_runtime_run_controls_for_model(
                "2026-04-15T19:00:10Z",
                "gemini-2.5-flash",
                Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
            )),
        },
    )
    .expect("seed gemini stale runtime run");

    let state = DesktopState::default();
    let recovered = probe_runtime_run(&state, probe_request(project_id, &repo_root))
        .expect("probe stale gemini runtime run")
        .expect("gemini runtime run should exist after stale probe");

    assert_eq!(recovered.run.run_id, "run-gemini-stale");
    assert_eq!(recovered.run.provider_id, "gemini_ai_studio");
    assert_eq!(recovered.run.runtime_kind, "gemini");
    assert_eq!(recovered.controls.active.model_id, "gemini-2.5-flash");
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
            launch_context: sample_launch_context(
                "openai_codex",
                "session-1",
                Some("flow-1"),
                "openai_codex",
                Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
            ),
            launch_env: RuntimeSupervisorLaunchEnv::default(),
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

pub(crate) fn detached_supervisor_rejects_provider_runtime_kind_mismatch_launch_context() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-provider-mismatch";
    let repo_root = seed_project(&root, project_id, "repo-provider-mismatch", "repo");
    let state = DesktopState::default();

    let error = launch_detached_runtime_supervisor(
        &state,
        launch_request_with_context(
            project_id,
            &repo_root,
            "run-provider-mismatch",
            "openai_compatible",
            sample_launch_context(
                "bedrock",
                "session-1",
                Some("flow-1"),
                "gpt-4.1-mini",
                Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
            ),
            RuntimeSupervisorLaunchEnv::default(),
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect_err("mismatched provider/runtime kind should fail closed");

    assert_eq!(error.code, "runtime_supervisor_provider_mismatch");
    assert!(error.message.contains("bedrock"));
    assert!(error.message.contains("openai_compatible"));
    assert!(
        project_store::load_runtime_run(&repo_root, project_id)
            .expect("load runtime run after launch validation failure")
            .is_none(),
        "malformed launch context should be rejected before any durable run row is written"
    );
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

pub(crate) fn detached_supervisor_launches_anthropic_child_with_context_env_and_secret_free_persistence(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-anthropic";
    let repo_root = seed_project(&root, project_id, "repo-anthropic", "repo");
    let state = DesktopState::default();
    let secret = "sk-ant-api03-sidecar-secret";

    let launched = launch_detached_runtime_supervisor(
        &state,
        anthropic_launch_request(
            project_id,
            &repo_root,
            "run-anthropic",
            "claude-3-7-sonnet-latest",
            Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::High),
            Some(secret),
            &anthropic_environment_report_script(),
        ),
    )
    .expect("launch anthropic detached runtime supervisor");

    assert_eq!(launched.run.runtime_kind, "anthropic");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.run.transport.liveness
                == project_store::RuntimeRunTransportLiveness::Reachable
            && snapshot.last_checkpoint_sequence >= 1
    });
    assert_eq!(running.run.runtime_kind, "anthropic");

    let mut reader = attach_reader(
        &running.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-anthropic", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert!(
        attached.latest_sequence.unwrap_or_default() >= 5,
        "attach ack missing expected transcript sequence coverage: {attached:?}"
    );
    assert!(attached.replayed_count >= 5, "attach ack: {attached:?}");

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-anthropic");
    let transcripts = frames
        .iter()
        .filter_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        transcripts.iter().any(|text| *text == "key-present"),
        "missing key-present transcript in replay: {}",
        response_dump(&frames)
    );
    assert!(
        transcripts.iter().any(|text| *text == "provider:anthropic"),
        "missing provider transcript in replay: {}",
        response_dump(&frames)
    );
    assert!(
        transcripts
            .iter()
            .any(|text| *text == "session:anthropic-session-1"),
        "missing session transcript in replay: {}",
        response_dump(&frames)
    );
    assert!(
        transcripts
            .iter()
            .any(|text| *text == "model:claude-3-7-sonnet-latest"),
        "missing model transcript in replay: {}",
        response_dump(&frames)
    );
    assert!(
        transcripts.iter().any(|text| *text == "thinking:high"),
        "missing thinking transcript in replay: {}",
        response_dump(&frames)
    );

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop anthropic detached runtime supervisor")
        .expect("anthropic runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_rejects_anthropic_launch_without_api_key_env() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-anthropic-missing";
    let repo_root = seed_project(&root, project_id, "repo-anthropic-missing", "repo");
    let state = DesktopState::default();

    let error = launch_detached_runtime_supervisor(
        &state,
        anthropic_launch_request(
            project_id,
            &repo_root,
            "run-anthropic-missing",
            "claude-3-7-sonnet-latest",
            Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
            None,
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect_err("anthropic detached launch should require api key env");
    assert_eq!(error.code, "anthropic_api_key_missing");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load failed anthropic runtime run")
        .expect("failed anthropic runtime run should persist");
    assert_eq!(snapshot.run.status, project_store::RuntimeRunStatus::Failed);
    assert_eq!(snapshot.run.transport.endpoint, "launch-pending");
    assert!(snapshot.run.last_heartbeat_at.is_none());
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("anthropic_api_key_missing")
    );
}

pub(crate) fn detached_supervisor_launches_bedrock_child_with_context_env_and_ambient_auth() {
    with_scoped_env(
        &[
            ("AWS_ACCESS_KEY_ID", Some("test-bedrock-access-key")),
            ("AWS_SECRET_ACCESS_KEY", Some("test-bedrock-secret-key")),
            ("AWS_SESSION_TOKEN", None),
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let project_id = "project-bedrock";
            let repo_root = seed_project(&root, project_id, "repo-bedrock", "repo");
            let state = DesktopState::default();

            let launched = launch_detached_runtime_supervisor(
                &state,
                bedrock_launch_request(
                    project_id,
                    &repo_root,
                    "run-bedrock",
                    "anthropic.claude-3-7-sonnet-20250219-v1:0",
                    "us-east-1",
                    Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::High),
                    &bedrock_environment_report_script(),
                ),
            )
            .expect("launch bedrock detached runtime supervisor");

            assert_eq!(launched.run.provider_id, "bedrock");
            assert_eq!(launched.run.runtime_kind, "anthropic");

            let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
                snapshot.run.status == project_store::RuntimeRunStatus::Running
                    && snapshot.run.transport.liveness
                        == project_store::RuntimeRunTransportLiveness::Reachable
                    && snapshot.last_checkpoint_sequence >= 1
            });

            let mut reader = attach_reader(
                &running.run.transport.endpoint,
                SupervisorControlRequest::attach(project_id, "run-bedrock", None),
            );
            let attached = expect_attach_ack(read_supervisor_response(&mut reader));
            let frames = read_event_frames(&mut reader, attached.replayed_count);
            assert_monotonic_sequences(&frames, "run-bedrock");
            let transcripts = frames
                .iter()
                .filter_map(|frame| match frame {
                    SupervisorControlResponse::Event {
                        item: SupervisorLiveEventPayload::Transcript { text },
                        ..
                    } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert!(transcripts.iter().any(|text| *text == "aws-auth-present"));
            assert!(transcripts.iter().any(|text| *text == "mode:1"));
            assert!(transcripts.iter().any(|text| *text == "region:us-east-1"));
            assert!(transcripts
                .iter()
                .any(|text| *text == "default-region:us-east-1"));
            assert!(transcripts.iter().any(|text| *text == "provider:bedrock"));
            assert!(transcripts
                .iter()
                .any(|text| *text == "model:anthropic.claude-3-7-sonnet-20250219-v1:0"));
            assert!(transcripts.iter().any(|text| *text == "thinking:high"));

            let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
                .expect("stop bedrock detached runtime supervisor")
                .expect("bedrock runtime run should exist after stop");
            assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
        },
    );
}

pub(crate) fn detached_supervisor_rejects_bedrock_launch_without_aws_credentials() {
    with_scoped_env(
        &[
            ("AWS_ACCESS_KEY_ID", None),
            ("AWS_SECRET_ACCESS_KEY", None),
            ("AWS_PROFILE", None),
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let project_id = "project-bedrock-missing";
            let repo_root = seed_project(&root, project_id, "repo-bedrock-missing", "repo");
            let state = DesktopState::default();

            let error = launch_detached_runtime_supervisor(
                &state,
                bedrock_launch_request(
                    project_id,
                    &repo_root,
                    "run-bedrock-missing",
                    "anthropic.claude-3-7-sonnet-20250219-v1:0",
                    "us-east-1",
                    Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
                    &runtime_shell::script_sleep(5),
                ),
            )
            .expect_err("bedrock detached launch should require ambient aws credentials");
            assert_eq!(error.code, "bedrock_aws_credentials_missing");
        },
    );
}

pub(crate) fn detached_supervisor_launches_vertex_child_with_context_env_and_ambient_auth() {
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
            let project_id = "project-vertex";
            let repo_root = seed_project(&root, project_id, "repo-vertex", "repo");
            let state = DesktopState::default();

            let launched = launch_detached_runtime_supervisor(
                &state,
                vertex_launch_request(
                    project_id,
                    &repo_root,
                    "run-vertex",
                    "claude-3-7-sonnet@20250219",
                    "us-central1",
                    "vertex-project",
                    Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
                    &vertex_environment_report_script(),
                ),
            )
            .expect("launch vertex detached runtime supervisor");

            assert_eq!(launched.run.provider_id, "vertex");
            assert_eq!(launched.run.runtime_kind, "anthropic");

            let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
                snapshot.run.status == project_store::RuntimeRunStatus::Running
                    && snapshot.run.transport.liveness
                        == project_store::RuntimeRunTransportLiveness::Reachable
                    && snapshot.last_checkpoint_sequence >= 1
            });

            let mut reader = attach_reader(
                &running.run.transport.endpoint,
                SupervisorControlRequest::attach(project_id, "run-vertex", None),
            );
            let attached = expect_attach_ack(read_supervisor_response(&mut reader));
            let frames = read_event_frames(&mut reader, attached.replayed_count);
            assert_monotonic_sequences(&frames, "run-vertex");
            let transcripts = frames
                .iter()
                .filter_map(|frame| match frame {
                    SupervisorControlResponse::Event {
                        item: SupervisorLiveEventPayload::Transcript { text },
                        ..
                    } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>();

            assert!(transcripts.iter().any(|text| *text == "adc-present"));
            assert!(transcripts.iter().any(|text| *text == "mode:1"));
            assert!(transcripts.iter().any(|text| *text == "region:us-central1"));
            assert!(transcripts
                .iter()
                .any(|text| *text == "project:vertex-project"));
            assert!(transcripts.iter().any(|text| *text == "provider:vertex"));
            assert!(transcripts
                .iter()
                .any(|text| *text == "model:claude-3-7-sonnet@20250219"));
            assert!(transcripts.iter().any(|text| *text == "thinking:medium"));

            let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
                .expect("stop vertex detached runtime supervisor")
                .expect("vertex runtime run should exist after stop");
            assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
        },
    );
}

pub(crate) fn detached_supervisor_rejects_vertex_launch_without_adc() {
    with_scoped_env(
        &[
            ("GOOGLE_APPLICATION_CREDENTIALS", None),
            ("AWS_ACCESS_KEY_ID", None),
            ("AWS_SECRET_ACCESS_KEY", None),
        ],
        || {
            let root = tempfile::tempdir().expect("temp dir");
            let project_id = "project-vertex-missing";
            let repo_root = seed_project(&root, project_id, "repo-vertex-missing", "repo");
            let state = DesktopState::default();

            let error = launch_detached_runtime_supervisor(
                &state,
                vertex_launch_request(
                    project_id,
                    &repo_root,
                    "run-vertex-missing",
                    "claude-3-7-sonnet@20250219",
                    "us-central1",
                    "vertex-project",
                    Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
                    &runtime_shell::script_sleep(5),
                ),
            )
            .expect_err("vertex detached launch should require adc");
            assert_eq!(error.code, "vertex_adc_missing");
        },
    );
}

pub(crate) fn detached_supervisor_launches_openai_compatible_child_with_context_env_and_secret_free_persistence(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-gemini";
    let repo_root = seed_project(&root, project_id, "repo-gemini", "repo");
    let state = DesktopState::default();
    let secret = "sk-gemini-sidecar-secret";

    let launched = launch_detached_runtime_supervisor(
        &state,
        openai_compatible_launch_request(
            project_id,
            &repo_root,
            "run-gemini",
            "gemini_ai_studio",
            "gemini",
            "gemini-2.5-flash",
            Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
            Some(secret),
            Some("https://generativelanguage.googleapis.com/v1beta/openai"),
            None,
            &openai_compatible_environment_report_script(),
        ),
    )
    .expect("launch openai-compatible detached runtime supervisor");

    assert_eq!(launched.run.provider_id, "gemini_ai_studio");
    assert_eq!(launched.run.runtime_kind, "gemini");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.run.transport.liveness
                == project_store::RuntimeRunTransportLiveness::Reachable
            && snapshot.last_checkpoint_sequence >= 1
    });

    let mut reader = attach_reader(
        &running.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-gemini", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-gemini");
    let transcripts = frames
        .iter()
        .filter_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(transcripts.iter().any(|text| *text == "key-present"));
    assert!(transcripts
        .iter()
        .any(|text| { *text == "base:https://generativelanguage.googleapis.com/v1beta/openai" }));
    assert!(transcripts
        .iter()
        .any(|text| *text == "provider:gemini_ai_studio"));
    assert!(transcripts
        .iter()
        .any(|text| *text == "model:gemini-2.5-flash"));
    assert!(transcripts.iter().any(|text| *text == "thinking:medium"));

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop openai-compatible detached runtime supervisor")
        .expect("openai-compatible runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_launches_github_models_child_with_context_env_and_secret_free_persistence(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-github-models";
    let repo_root = seed_project(&root, project_id, "repo-github-models", "repo");
    let state = DesktopState::default();
    let secret = "github_pat_sidecar_secret";

    let launched = launch_detached_runtime_supervisor(
        &state,
        openai_compatible_launch_request(
            project_id,
            &repo_root,
            "run-github-models",
            "github_models",
            "openai_compatible",
            "openai/gpt-4.1",
            None,
            Some(secret),
            Some("https://models.inference.ai.azure.com"),
            None,
            &openai_compatible_environment_report_script(),
        ),
    )
    .expect("launch github models detached runtime supervisor");

    assert_eq!(launched.run.provider_id, "github_models");
    assert_eq!(launched.run.runtime_kind, "openai_compatible");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.run.transport.liveness
                == project_store::RuntimeRunTransportLiveness::Reachable
            && snapshot.last_checkpoint_sequence >= 1
    });

    let mut reader = attach_reader(
        &running.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-github-models", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-github-models");
    let transcripts = frames
        .iter()
        .filter_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(transcripts.iter().any(|text| *text == "key-present"));
    assert!(transcripts
        .iter()
        .any(|text| *text == "base:https://models.inference.ai.azure.com"));
    assert!(transcripts
        .iter()
        .any(|text| *text == "provider:github_models"));
    assert!(transcripts
        .iter()
        .any(|text| *text == "model:openai/gpt-4.1"));

    let database_bytes =
        std::fs::read(database_path_for_repo(&repo_root)).expect("read runtime db");
    let database_text = String::from_utf8_lossy(&database_bytes);
    assert!(!database_text.contains(secret));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop github models detached runtime supervisor")
        .expect("github models runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_launches_ollama_child_without_api_key_env() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-ollama";
    let repo_root = seed_project(&root, project_id, "repo-ollama", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        openai_compatible_launch_request(
            project_id,
            &repo_root,
            "run-ollama",
            "ollama",
            "openai_compatible",
            "llama3.2",
            None,
            None,
            Some("http://127.0.0.1:11434/v1"),
            None,
            &openai_compatible_environment_report_script(),
        ),
    )
    .expect("launch ollama detached runtime supervisor");

    assert_eq!(launched.run.provider_id, "ollama");
    assert_eq!(launched.run.runtime_kind, "openai_compatible");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.run.transport.liveness
                == project_store::RuntimeRunTransportLiveness::Reachable
            && snapshot.last_checkpoint_sequence >= 1
    });

    let mut reader = attach_reader(
        &running.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-ollama", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-ollama");
    let transcripts = frames
        .iter()
        .filter_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(transcripts.iter().any(|text| *text == "key-missing"));
    assert!(transcripts.iter().any(|text| *text == "provider:ollama"));
    assert!(transcripts.iter().any(|text| *text == "model:llama3.2"));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop ollama detached runtime supervisor")
        .expect("ollama runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_rejects_openai_compatible_launch_without_api_key_env() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-openai-missing-key";
    let repo_root = seed_project(&root, project_id, "repo-openai-missing-key", "repo");
    let state = DesktopState::default();

    let error = launch_detached_runtime_supervisor(
        &state,
        openai_compatible_launch_request(
            project_id,
            &repo_root,
            "run-openai-missing-key",
            "openai_api",
            "openai_compatible",
            "gpt-4.1-mini",
            Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
            None,
            Some("https://api.openai.com/v1"),
            None,
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect_err("openai-compatible detached launch should require api key env");
    assert_eq!(error.code, "openai_api_key_missing");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load failed openai-compatible runtime run")
        .expect("failed openai-compatible runtime run should persist");
    assert_eq!(snapshot.run.status, project_store::RuntimeRunStatus::Failed);
    assert_eq!(snapshot.run.transport.endpoint, "launch-pending");
    assert!(snapshot.run.last_heartbeat_at.is_none());
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("openai_api_key_missing")
    );
}

pub(crate) fn detached_supervisor_rejects_github_models_launch_without_token_env() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-github-missing-token";
    let repo_root = seed_project(&root, project_id, "repo-github-missing-token", "repo");
    let state = DesktopState::default();

    let error = launch_detached_runtime_supervisor(
        &state,
        openai_compatible_launch_request(
            project_id,
            &repo_root,
            "run-github-missing-token",
            "github_models",
            "openai_compatible",
            "openai/gpt-4.1",
            None,
            None,
            Some("https://models.inference.ai.azure.com"),
            None,
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect_err("github models detached launch should require token env");
    assert_eq!(error.code, "github_models_token_missing");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load failed github models runtime run")
        .expect("failed github models runtime run should persist");
    assert_eq!(snapshot.run.status, project_store::RuntimeRunStatus::Failed);
    assert_eq!(snapshot.run.transport.endpoint, "launch-pending");
    assert!(snapshot.run.last_heartbeat_at.is_none());
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("github_models_token_missing")
    );
}

pub(crate) fn detached_supervisor_rejects_openai_compatible_launch_without_base_url_env() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-gemini-missing";
    let repo_root = seed_project(&root, project_id, "repo-gemini-missing", "repo");
    let state = DesktopState::default();

    let error = launch_detached_runtime_supervisor(
        &state,
        openai_compatible_launch_request(
            project_id,
            &repo_root,
            "run-gemini-missing",
            "gemini_ai_studio",
            "gemini",
            "gemini-2.5-flash",
            Some(cadence_desktop_lib::commands::ProviderModelThinkingEffortDto::Medium),
            Some("sk-gemini-present"),
            None,
            None,
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect_err("openai-compatible detached launch should require base url env");
    assert_eq!(error.code, "openai_compatible_base_url_missing");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load failed openai-compatible runtime run")
        .expect("failed openai-compatible runtime run should persist");
    assert_eq!(snapshot.run.status, project_store::RuntimeRunStatus::Failed);
    assert_eq!(snapshot.run.transport.endpoint, "launch-pending");
    assert!(snapshot.run.last_heartbeat_at.is_none());
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("openai_compatible_base_url_missing")
    );
}
