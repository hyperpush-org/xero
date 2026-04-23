use super::support::*;

const STRUCTURED_EVENT_PREFIX: &str = "__Cadence_EVENT__ ";
const CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE: &str = "runtime_run_controls_apply_boundary";
const CONTROL_APPLIED_ACTIVITY_CODE: &str = "runtime_run_controls_applied";

fn control_boundary_event(detail: &str) -> String {
    format!(
        "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"activity\",\"code\":\"{CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE}\",\"title\":\"Model-call boundary reached\",\"detail\":\"{detail}\"}}"
    )
}

fn activity_code(frame: &SupervisorControlResponse) -> Option<&str> {
    match frame {
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Activity { code, .. },
            ..
        } => Some(code.as_str()),
        _ => None,
    }
}

fn count_activity_code(frames: &[SupervisorControlResponse], target: &str) -> usize {
    frames
        .iter()
        .filter(|frame| activity_code(frame) == Some(target))
        .count()
}

fn response_dump(frames: &[SupervisorControlResponse]) -> String {
    frames
        .iter()
        .map(|frame| serde_json::to_string(frame).expect("serialize frame"))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn queued_runtime_controls_apply_on_next_model_boundary_and_recover_reload_truth() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    let runtime_session = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get authenticated runtime session before scripted launch");
    let session_id = runtime_session
        .session_id
        .clone()
        .expect("runtime session should expose a stable session id");

    let script = runtime_shell::script_join_steps(&[
        runtime_shell::script_print_line("boot"),
        runtime_shell::script_sleep(3),
        runtime_shell::script_print_line(&control_boundary_event(
            "Harness reached the next model-call boundary.",
        )),
        runtime_shell::script_sleep(6),
    ]);
    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-controls-apply",
        &session_id,
        runtime_session.flow_id.as_deref(),
        &script,
    );

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });

    let queued_prompt = "Review the latest diff before continuing.";
    let queued = update_runtime_run_controls(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpdateRuntimeRunControlsRequestDto {
            project_id: project_id.clone(),
            run_id: launched.run.run_id.clone(),
            controls: Some(RuntimeRunControlInputDto {
                model_id: running.controls.active.model_id.clone(),
                thinking_effort: running.controls.active.thinking_effort.clone(),
                approval_mode: RuntimeRunApprovalModeDto::AutoEdit,
            }),
            prompt: Some(queued_prompt.into()),
        },
    )
    .expect("queue runtime controls");
    let pending = queued
        .controls
        .pending
        .clone()
        .expect("queued runtime controls should expose pending snapshot");
    assert_eq!(pending.revision, queued.controls.active.revision + 1);
    assert_eq!(pending.approval_mode, RuntimeRunApprovalModeDto::AutoEdit);
    assert_eq!(pending.queued_prompt.as_deref(), Some(queued_prompt));

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let before_apply = get_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run before apply")
    .expect("runtime run should exist before apply");
    assert_eq!(before_apply.run_id, launched.run.run_id);
    assert_eq!(
        before_apply
            .controls
            .pending
            .as_ref()
            .and_then(|pending| pending.queued_prompt.as_deref()),
        Some(queued_prompt)
    );

    let applied = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.controls.pending.is_none()
            && runtime_run.controls.active.revision == pending.revision
            && runtime_run.controls.active.approval_mode == RuntimeRunApprovalModeDto::AutoEdit
    });
    assert_eq!(
        applied.controls.active.model_id,
        running.controls.active.model_id
    );
    assert!(applied.controls.active.applied_at >= pending.queued_at);
    assert_ne!(
        applied.controls.active.applied_at,
        queued.controls.active.applied_at
    );
    assert!(applied.last_error_code.is_none());

    let mut reader = attach_reader(
        &applied.transport.endpoint,
        SupervisorControlRequest::attach(&project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let replayed_count = match attached {
        SupervisorControlResponse::Attached { replayed_count, .. } => replayed_count,
        other => panic!("expected attach ack, got {other:?}"),
    };
    let frames = read_event_frames(&mut reader, replayed_count);
    assert_eq!(
        count_activity_code(&frames, "runtime_run_controls_queued"),
        1
    );
    assert_eq!(
        count_activity_code(&frames, CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE),
        1
    );
    assert_eq!(
        count_activity_code(&frames, CONTROL_APPLIED_ACTIVITY_CODE),
        1
    );

    let replay_dump = response_dump(&frames);
    assert!(!replay_dump.contains(queued_prompt));
    let checkpoint_dump = applied
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.summary.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(checkpoint_dump.contains("runtime_run_controls_queued"));
    assert!(checkpoint_dump.contains(CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE));
    assert!(checkpoint_dump.contains(CONTROL_APPLIED_ACTIVITY_CODE));
    assert!(!checkpoint_dump.contains(queued_prompt));

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop runtime run after control apply")
    .expect("runtime run should still exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn queued_runtime_controls_duplicate_boundary_is_idempotent() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    let runtime_session = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get authenticated runtime session before duplicate boundary launch");
    let session_id = runtime_session
        .session_id
        .clone()
        .expect("runtime session should expose a stable session id");

    let script = runtime_shell::script_join_steps(&[
        runtime_shell::script_print_line("boot"),
        runtime_shell::script_sleep(3),
        runtime_shell::script_print_line(&control_boundary_event(
            "Harness reached the next model-call boundary.",
        )),
        runtime_shell::script_sleep(1),
        runtime_shell::script_print_line(&control_boundary_event(
            "Harness reached the next model-call boundary.",
        )),
        runtime_shell::script_sleep(6),
    ]);
    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-controls-duplicate-boundary",
        &session_id,
        runtime_session.flow_id.as_deref(),
        &script,
    );

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });

    let queued = update_runtime_run_controls(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpdateRuntimeRunControlsRequestDto {
            project_id: project_id.clone(),
            run_id: launched.run.run_id.clone(),
            controls: Some(RuntimeRunControlInputDto {
                model_id: running.controls.active.model_id.clone(),
                thinking_effort: running.controls.active.thinking_effort.clone(),
                approval_mode: RuntimeRunApprovalModeDto::Yolo,
            }),
            prompt: None,
        },
    )
    .expect("queue runtime controls for duplicate boundary proof");
    let pending_revision = queued
        .controls
        .pending
        .as_ref()
        .expect("pending snapshot should exist after queue")
        .revision;

    let _applied = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.controls.pending.is_none()
            && runtime_run.controls.active.revision == pending_revision
            && runtime_run.controls.active.approval_mode == RuntimeRunApprovalModeDto::Yolo
    });
    let replay_ready = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run
                .checkpoints
                .iter()
                .filter(|checkpoint| {
                    checkpoint
                        .summary
                        .contains(CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE)
                })
                .count()
                >= 2
    });

    let mut reader = attach_reader(
        &replay_ready.transport.endpoint,
        SupervisorControlRequest::attach(&project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let replayed_count = match attached {
        SupervisorControlResponse::Attached { replayed_count, .. } => replayed_count,
        other => panic!("expected attach ack, got {other:?}"),
    };
    let frames = read_event_frames(&mut reader, replayed_count);
    assert_eq!(
        count_activity_code(&frames, CONTROL_APPLY_BOUNDARY_ACTIVITY_CODE),
        2,
        "duplicate boundary markers should still replay, but only the first may apply pending controls"
    );
    assert_eq!(
        count_activity_code(&frames, CONTROL_APPLIED_ACTIVITY_CODE),
        1
    );

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop runtime run after duplicate boundary proof")
    .expect("runtime run should still exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn get_runtime_run_fails_closed_for_malformed_control_state_without_fake_apply_transition(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    let control_state = project_store::build_runtime_run_control_state(
        "openai_codex",
        None,
        RuntimeRunApprovalModeDto::Suggest,
        "2026-04-15T19:00:00Z",
        Some("Review the diff before continuing."),
    )
    .expect("build queued control state");
    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.clone(),
                run_id: "run-malformed-control-state".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Failed,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "launch-pending".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Unknown,
                },
                started_at: "2026-04-15T19:00:00Z".into(),
                last_heartbeat_at: None,
                stopped_at: Some("2026-04-15T19:00:01Z".into()),
                last_error: None,
                updated_at: "2026-04-15T19:00:01Z".into(),
            },
            checkpoint: None,
            control_state: Some(control_state),
        },
    )
    .expect("seed runtime run with queued control state");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints");
    connection
        .execute(
            "UPDATE runtime_runs SET control_state_json = '{\"active\":{\"modelId\":\"openai_codex\",\"approvalMode\":\"suggest\",\"revision\":1,\"appliedAt\":\"bad-timestamp\"},\"pending\":{\"modelId\":\"openai_codex\",\"approvalMode\":\"auto_edit\",\"revision\":2,\"queuedAt\":\"still-bad\",\"queuedPrompt\":\"Review the diff before continuing.\",\"queuedPromptAt\":\"also-bad\"}}' WHERE project_id = ?1",
            [&project_id],
        )
        .expect("corrupt control state json");

    recorder.clear();
    let error = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto { project_id },
    )
    .expect_err("malformed control state should fail closed");

    assert_eq!(error.code, "runtime_run_decode_failed");
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);
    assert_eq!(count_runtime_run_rows(&repo_root), 1);
}
