use super::support::*;
use serde_json::json;

pub(crate) fn detached_supervisor_live_event_redacts_secret_bearing_output_in_replay_and_checkpoint(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-redaction";
    let repo_root = seed_project(&root, project_id, "repo-redaction", "repo");
    let state = DesktopState::default();

    let live_lines = vec![
        "access_token=shh-secret-value".to_string(),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"activity\",\"code\":\"diag\",\"title\":\"Auth\",\"detail\":\"Bearer hidden-token\"}}"
        ),
    ];

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-redaction",
            &runtime_shell::script_print_lines_and_sleep(&live_lines, 5),
        ),
    )
    .expect("launch redaction runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.last_checkpoint_sequence >= 2
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-redaction", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert_eq!(attached.replayed_count, 2);

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-redaction");
    assert!(frames.iter().all(|frame| matches!(
        frame,
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Activity { code, .. },
            ..
        } if code == "runtime_supervisor_live_event_redacted"
    )));

    let replay_dump = response_dump(&frames);
    assert!(!replay_dump.contains("access_token"));
    assert!(!replay_dump.contains("Bearer"));
    assert!(!replay_dump.contains("sk-"));

    let stored = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load stored runtime run")
        .expect("stored runtime run should exist");
    let checkpoint_dump = stored
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.summary.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!checkpoint_dump.contains("access_token"));
    assert!(!checkpoint_dump.contains("Bearer"));
    assert!(checkpoint_dump.contains("runtime_supervisor_live_event_redacted"));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop redaction runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_live_event_drops_unsupported_structured_payload_kind() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-invalid-payload";
    let repo_root = seed_project(&root, project_id, "repo-invalid-payload", "repo");
    let state = DesktopState::default();

    let live_lines = vec![format!(
        "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"mystery\",\"detail\":\"unexpected\"}}"
    )];

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-invalid-payload",
            &runtime_shell::script_print_lines_and_sleep(&live_lines, 5),
        ),
    )
    .expect("launch invalid payload runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.last_checkpoint_sequence >= 1
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-invalid-payload", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert_eq!(attached.replayed_count, 1);

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert!(matches!(
        &frames[0],
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Activity { code, title, .. },
            ..
        } if code == "runtime_supervisor_live_event_unsupported"
            && title == "Live output fragment dropped"
    ));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop invalid payload runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_persists_redacted_interactive_boundary_and_replays_same_action_identity(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-interactive";
    let repo_root = seed_project(&root, project_id, "repo-interactive", "repo");
    let state = DesktopState::default();

    let _launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-interactive",
            &runtime_shell::script_prompt_and_sleep("Paste access_token now: ", 5),
        ),
    )
    .expect("launch interactive runtime supervisor");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let interactive_checkpoint = running
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
        })
        .expect("action required checkpoint");
    assert!(!interactive_checkpoint.summary.contains("access_token"));
    assert!(!interactive_checkpoint
        .summary
        .contains("Paste access_token now"));
    assert_eq!(
        interactive_checkpoint.summary,
        "Detached runtime blocked on terminal input and is awaiting operator approval."
    );

    let project_snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load project snapshot")
        .snapshot;
    assert_eq!(project_snapshot.approval_requests.len(), 1);
    let approval = &project_snapshot.approval_requests[0];
    assert_eq!(
        approval.status,
        cadence_desktop_lib::commands::OperatorApprovalStatus::Pending
    );
    assert_eq!(approval.session_id.as_deref(), Some("session-1"));
    assert_eq!(approval.flow_id.as_deref(), Some("flow-1"));
    assert_eq!(approval.action_type, "terminal_input_required");
    assert_eq!(approval.title, "Terminal input required");
    assert_eq!(approval.detail, "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.");
    assert!(!approval.detail.contains("access_token"));
    assert!(approval
        .action_id
        .contains(":run:run-interactive:boundary:"));

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe with fresh host state")
        .expect("runtime run should still exist");

    let mut reader = attach_reader(
        &recovered.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-interactive", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert!(
        attached.replayed_count >= 1,
        "expected at least one replay frame for interactive boundary"
    );

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    let action_required_frame = frames
        .iter()
        .find(|frame| {
            matches!(
                frame,
                SupervisorControlResponse::Event {
                    item: SupervisorLiveEventPayload::ActionRequired { .. },
                    ..
                }
            )
        })
        .expect("expected action-required replay frame");
    let action_required_count = frames
        .iter()
        .filter(|frame| {
            matches!(
                frame,
                SupervisorControlResponse::Event {
                    item: SupervisorLiveEventPayload::ActionRequired { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(action_required_count, 1);
    assert!(matches!(
        action_required_frame,
        SupervisorControlResponse::Event {
            item:
                SupervisorLiveEventPayload::ActionRequired {
                    action_id,
                    action_type,
                    title,
                    detail,
                    ..
                },
            ..
        } if action_id == &approval.action_id
            && action_type == "terminal_input_required"
            && title == "Terminal input required"
            && detail == "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run."
    ));

    let replay_dump = response_dump(&frames);
    assert!(!replay_dump.contains("access_token"));
    assert!(!replay_dump.contains("Paste access_token now"));

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop interactive runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_persists_matching_autonomous_boundary_once_before_reload() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-interactive-autonomous";
    let repo_root = seed_project(&root, project_id, "repo-interactive-autonomous", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-interactive-autonomous",
            &runtime_shell::script_join_steps(&[
                runtime_shell::script_sleep(1),
                runtime_shell::script_prompt_read_echo_and_sleep(
                    "Enter deployment code: ",
                    "value",
                    "value=",
                    5,
                ),
            ]),
        ),
    )
    .expect("launch interactive runtime supervisor for autonomous persistence");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
    });
    seed_active_autonomous_run(&repo_root, project_id, &launched.run.run_id);

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    let (approval_action_id, boundary_id) = frames
        .iter()
        .find_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item:
                    SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        boundary_id,
                        ..
                    },
                ..
            } => Some((action_id.clone(), boundary_id.clone())),
            _ => None,
        })
        .expect("expected action-required replay frame for autonomous persistence test");

    persist_supervisor_event(
        &repo_root,
        project_id,
        &SupervisorLiveEventPayload::ActionRequired {
            action_id: approval_action_id.clone(),
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
        },
    )
    .expect("persist autonomous boundary from supervisor event")
    .expect("autonomous boundary persistence should return a snapshot");

    let boundary_snapshot = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("load autonomous run after boundary persistence")
        .expect("autonomous run should exist after boundary persistence");
    assert_eq!(
        boundary_snapshot.run.status,
        project_store::AutonomousRunStatus::Paused
    );
    assert_eq!(
        boundary_snapshot
            .unit
            .as_ref()
            .map(|unit| unit.status.clone()),
        Some(project_store::AutonomousUnitStatus::Blocked)
    );
    assert_eq!(
        boundary_snapshot
            .attempt
            .as_ref()
            .map(|attempt| attempt.status.clone()),
        Some(project_store::AutonomousUnitStatus::Blocked)
    );

    let boundary_evidence = boundary_snapshot
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                        && payload.action_id.as_deref() == Some(approval_action_id.as_str())
                        && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(boundary_evidence.len(), 1);

    let approval_action_id = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load project snapshot after autonomous boundary persist")
        .snapshot
        .approval_requests[0]
        .action_id
        .clone();
    assert!(approval_action_id.contains(boundary_id.as_str()));

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe runtime run with fresh host state")
        .expect("runtime run should still exist after fresh probe");
    assert_eq!(recovered.run.run_id, launched.run.run_id);

    let replayed_snapshot = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run after fresh probe")
        .expect("autonomous run should still exist after fresh probe");
    let replayed_boundary_evidence = replayed_snapshot
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                        && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
            )
        })
        .count();
    assert_eq!(replayed_boundary_evidence, 1);

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop interactive runtime supervisor after autonomous persistence test")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_coalesces_repeated_prompt_churn_into_one_boundary() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-interactive-repeat";
    let repo_root = seed_project(&root, project_id, "repo-interactive-repeat", "repo");
    let state = DesktopState::default();

    launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-interactive-repeat",
            &runtime_shell::script_repeat_prompt_and_sleep("Enter deployment code: ", 2, 5),
        ),
    )
    .expect("launch repeated interactive runtime supervisor");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let action_required_count = running
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
        })
        .count();
    assert_eq!(action_required_count, 1);

    let project_snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load project snapshot")
        .snapshot;
    assert_eq!(project_snapshot.approval_requests.len(), 1);

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop repeated interactive runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_submit_input_routes_bytes_through_owned_writer() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-submit-success";
    let repo_root = seed_project(&root, project_id, "repo-submit-success", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-submit-success",
            &runtime_shell::script_prompt_read_echo_and_sleep(
                "Enter value: ",
                "value",
                "value=",
                5,
            ),
        ),
    )
    .expect("launch interactive runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-submit-success", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    let (action_id, boundary_id) = match &frames[0] {
        SupervisorControlResponse::Event {
            item:
                SupervisorLiveEventPayload::ActionRequired {
                    action_id,
                    boundary_id,
                    ..
                },
            ..
        } => (action_id.clone(), boundary_id.clone()),
        other => panic!("expected action-required replay frame, got {other:?}"),
    };

    let submit = send_control_request(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::submit_input(
            project_id,
            "run-submit-success",
            "session-1",
            Some("flow-1".into()),
            action_id,
            boundary_id,
            "approved",
        ),
    );
    assert!(matches!(
        submit,
        SupervisorControlResponse::SubmitInputAccepted { .. }
    ));

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw_delivery = false;
    let mut saw_transcript = false;
    while Instant::now() < deadline && !(saw_delivery && saw_transcript) {
        match read_supervisor_response(&mut reader) {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Activity { code, .. },
                ..
            } if code == "runtime_supervisor_input_delivered" => saw_delivery = true,
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            } if text == "value=approved" => saw_transcript = true,
            _ => {}
        }
    }

    assert!(saw_delivery, "expected input-delivered activity frame");
    assert!(saw_transcript, "expected resumed transcript output");

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop submit-success runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn submit_runtime_run_input_rejects_mismatched_ack_and_preserves_running_projection() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-submit-ack-mismatch";
    let repo_root = seed_project(&root, project_id, "repo-submit-ack-mismatch", "repo");
    let state = DesktopState::default();

    let mismatched_ack = SupervisorControlResponse::SubmitInputAccepted {
        protocol_version: SUPERVISOR_PROTOCOL_VERSION,
        project_id: project_id.into(),
        run_id: "run-submit-ack-mismatch".into(),
        action_id: "forged-action".into(),
        boundary_id: "boundary-1".into(),
        delivered_at: "2026-04-16T00:00:02Z".into(),
    };
    let (endpoint, server) = spawn_single_response_control_server(
        serde_json::to_string(&mismatched_ack).expect("serialize mismatched ack"),
    );
    seed_running_runtime_run(&repo_root, project_id, "run-submit-ack-mismatch", &endpoint);

    let error = submit_runtime_run_input(
        &state,
        RuntimeSupervisorSubmitInputRequest {
            project_id: project_id.into(),
            repo_root: repo_root.clone(),
            run_id: "run-submit-ack-mismatch".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            action_id: "expected-action".into(),
            boundary_id: "boundary-1".into(),
            input: "approved".into(),
            control_timeout: Duration::from_millis(750),
        },
    )
    .expect_err("submit should reject mismatched acknowledgement identity");
    assert_eq!(error.code, "runtime_supervisor_submit_ack_mismatch");

    server
        .join()
        .expect("mock mismatched-ack control server thread should complete");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load runtime run after mismatched ack")
        .expect("runtime run should still exist after mismatched ack");
    assert_eq!(
        snapshot.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert_eq!(
        snapshot.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Reachable
    );
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_submit_ack_mismatch")
    );
}

pub(crate) fn submit_runtime_run_input_preserves_running_projection_on_malformed_control_response()
{
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-submit-malformed-response";
    let repo_root = seed_project(&root, project_id, "repo-submit-malformed-response", "repo");
    let state = DesktopState::default();

    let (endpoint, server) = spawn_single_response_control_server("not-json".into());
    seed_running_runtime_run(
        &repo_root,
        project_id,
        "run-submit-malformed-response",
        &endpoint,
    );

    let error = submit_runtime_run_input(
        &state,
        RuntimeSupervisorSubmitInputRequest {
            project_id: project_id.into(),
            repo_root: repo_root.clone(),
            run_id: "run-submit-malformed-response".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            action_id: "expected-action".into(),
            boundary_id: "boundary-1".into(),
            input: "approved".into(),
            control_timeout: Duration::from_millis(750),
        },
    )
    .expect_err("submit should fail closed when control response is malformed");
    assert_eq!(error.code, "runtime_supervisor_control_invalid");

    server
        .join()
        .expect("mock malformed-response control server thread should complete");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load runtime run after malformed control response")
        .expect("runtime run should still exist after malformed control response");
    assert_eq!(
        snapshot.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert_eq!(
        snapshot.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Reachable
    );
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_control_invalid")
    );
}

pub(crate) fn detached_supervisor_persists_structured_shell_review_boundary_and_replays_same_action_identity(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-shell-review";
    let repo_root = seed_project(&root, project_id, "repo-shell-review", "repo");
    let state = DesktopState::default();
    let run_id = "run-shell-review";
    let boundary_id = "boundary-shell-review";
    let action_type = "shell_policy_review";
    let action_id = project_store::derive_runtime_action_id(
        "session-1",
        Some("flow-1"),
        run_id,
        boundary_id,
        action_type,
    )
    .expect("derive canonical shell-review action id");

    let shell_review_script = runtime_shell::script_join_steps(&[
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "action_required",
                "action_id": action_id,
                "boundary_id": boundary_id,
                "action_type": action_type,
                "title": "Review shell command",
                "detail": "Cadence requires operator approval before running the blocked shell command."
            })
        )),
        runtime_shell::script_sleep(5),
    ]);

    let _launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(project_id, &repo_root, run_id, &shell_review_script),
    )
    .expect("launch shell-review runtime supervisor");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let checkpoint = running
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
        })
        .expect("shell-review action-required checkpoint");
    assert_eq!(
        checkpoint.summary,
        "Detached runtime blocked on `Review shell command` and is awaiting operator approval."
    );

    let project_snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load shell-review project snapshot")
        .snapshot;
    assert_eq!(project_snapshot.approval_requests.len(), 1);
    let approval = &project_snapshot.approval_requests[0];
    assert_eq!(approval.action_id, action_id);
    assert_eq!(approval.action_type, action_type);
    assert_eq!(approval.title, "Review shell command");
    assert_eq!(
        approval.detail,
        "Cadence requires operator approval before running the blocked shell command."
    );

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe shell-review runtime run")
        .expect("shell-review runtime run should still exist");
    let mut reader = attach_reader(
        &recovered.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    let action_required = frames
        .iter()
        .find(|frame| {
            matches!(
                frame,
                SupervisorControlResponse::Event {
                    item: SupervisorLiveEventPayload::ActionRequired { .. },
                    ..
                }
            )
        })
        .expect("shell-review replay should include action-required event");
    assert!(matches!(
        action_required,
        SupervisorControlResponse::Event {
            item:
                SupervisorLiveEventPayload::ActionRequired {
                    action_id,
                    boundary_id,
                    action_type,
                    title,
                    detail,
                },
            ..
        } if action_id == approval.action_id.as_str()
            && boundary_id == "boundary-shell-review"
            && action_type == "shell_policy_review"
            && title == "Review shell command"
            && detail == "Cadence requires operator approval before running the blocked shell command."
    ));

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop shell-review runtime supervisor")
        .expect("shell-review runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

pub(crate) fn detached_supervisor_rejects_structured_shell_review_boundary_with_malformed_action_identity(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-shell-review-invalid";
    let repo_root = seed_project(&root, project_id, "repo-shell-review-invalid", "repo");
    let state = DesktopState::default();

    let invalid_script = runtime_shell::script_join_steps(&[
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "action_required",
                "action_id": "forged-action-id",
                "boundary_id": "boundary-shell-review-invalid",
                "action_type": "shell_policy_review",
                "title": "Review shell command",
                "detail": "Cadence requires operator approval before running the blocked shell command."
            })
        )),
        runtime_shell::script_sleep(5),
    ]);

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-shell-review-invalid",
            &invalid_script,
        ),
    )
    .expect("launch malformed shell-review runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot
                .run
                .last_error
                .as_ref()
                .is_some_and(|error| error.code == "runtime_action_identity_invalid")
    });

    let snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load malformed shell-review project snapshot")
        .snapshot;
    assert!(snapshot.approval_requests.is_empty());
    assert!(snapshot.resume_history.is_empty());

    let runtime_run = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load malformed shell-review runtime run")
        .expect("runtime run should exist after malformed shell-review event");
    assert_eq!(
        runtime_run
            .checkpoints
            .iter()
            .filter(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
            .count(),
        0
    );
    assert_eq!(
        runtime_run
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_action_identity_invalid")
    );

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-shell-review-invalid", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert!(frames.iter().all(|frame| matches!(
        frame,
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Activity { code, .. },
            ..
        } if code == "runtime_action_identity_invalid"
    )));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop malformed shell-review runtime supervisor")
        .expect("malformed shell-review runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}
