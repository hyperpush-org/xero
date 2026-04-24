use super::support::*;

pub(crate) fn runtime_action_required_persistence_enqueues_notification_dispatches_once_per_route()
{
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_notification_route(&repo_root, &project_id, "route-discord");
    seed_unreachable_runtime_run(&repo_root, &project_id, "run-dispatch");

    let first = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: "run-dispatch".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T19:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-16T20:40:00Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    assert_eq!(
        first.notification_dispatch_outcome.status,
        project_store::NotificationDispatchEnqueueStatus::Enqueued
    );
    assert_eq!(first.notification_dispatch_outcome.dispatch_count, 1);

    let action_id = first.approval_request.action_id.clone();
    let first_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &action_id);
    assert_eq!(first_dispatches.len(), 1);
    assert_eq!(
        first_dispatches[0].status,
        project_store::NotificationDispatchStatus::Pending
    );

    let second = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: "run-dispatch".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T19:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-16T20:40:01Z".into(),
        },
    )
    .expect("replay runtime action-required approval");

    let second_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &action_id);
    assert_eq!(second_dispatches.len(), 1);
    assert_eq!(second_dispatches[0].id, first_dispatches[0].id);
    assert_eq!(
        second.notification_dispatch_outcome.dispatch_count,
        first.notification_dispatch_outcome.dispatch_count
    );
}

pub(crate) fn submit_notification_reply_first_wins_and_rejects_forged_and_duplicate_replies() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_auto_continuation(&repo_root, &project_id, "approve_execution");
    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let primary = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T20:41:00Z",
    )
    .expect("persist primary pending approval");
    let secondary = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-2",
        Some("flow-2"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T20:41:01Z",
    )
    .expect("persist secondary pending approval");

    let primary_dispatch =
        load_notification_dispatches_for_action(&repo_root, &project_id, &primary.action_id)
            .into_iter()
            .next()
            .expect("primary dispatch row should exist");
    let secondary_dispatch =
        load_notification_dispatches_for_action(&repo_root, &project_id, &secondary.action_id)
            .into_iter()
            .next()
            .expect("secondary dispatch row should exist");

    let first = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: primary.action_id.clone(),
            route_id: primary_dispatch.route_id.clone(),
            correlation_key: primary_dispatch.correlation_key.clone(),
            responder_id: Some("operator-a".into()),
            reply_text: "Execution approved.".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:05Z".into(),
        },
    )
    .expect("first reply should claim, resolve, and resume");

    assert_eq!(
        first.claim.status,
        NotificationReplyClaimStatusDto::Accepted
    );
    assert_eq!(
        first.dispatch.status,
        NotificationDispatchStatusDto::Claimed
    );
    assert_eq!(
        first.resolve_result.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        first
            .resume_result
            .as_ref()
            .map(|resume| resume.resume_entry.status.clone()),
        Some(ResumeHistoryStatus::Started)
    );

    let forged_error = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: secondary.action_id.clone(),
            route_id: secondary_dispatch.route_id,
            correlation_key: primary_dispatch.correlation_key.clone(),
            responder_id: Some("operator-b".into()),
            reply_text: "Forged correlation".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:06Z".into(),
        },
    )
    .expect_err("forged correlation must fail closed");
    assert_eq!(forged_error.code, "notification_reply_correlation_invalid");

    let duplicate_error = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: primary.action_id.clone(),
            route_id: primary_dispatch.route_id,
            correlation_key: first.dispatch.correlation_key.clone(),
            responder_id: Some("operator-c".into()),
            reply_text: "Duplicate answer".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:07Z".into(),
        },
    )
    .expect_err("duplicate reply after winner should fail closed");
    assert_eq!(duplicate_error.code, "notification_reply_already_claimed");

    let claims = project_store::load_notification_reply_claims(
        &repo_root,
        &project_id,
        Some(&primary.action_id),
    )
    .expect("load primary reply claims");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| claim.status == project_store::NotificationReplyClaimStatus::Accepted));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));
}

pub(crate) fn submit_notification_reply_cross_channel_race_accepts_single_winner_and_preserves_resume_truth(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_auto_continuation(&repo_root, &project_id, "approve_execution");

    project_store::upsert_notification_route(
        &repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.clone(),
            route_id: "route-telegram".into(),
            route_kind: "telegram".into(),
            route_target: "telegram:ops-room".into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T14:59:58Z".into(),
        },
    )
    .expect("upsert telegram route");
    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let approval = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T20:50:00Z",
    )
    .expect("persist pending approval");

    let dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &approval.action_id);
    assert_eq!(dispatches.len(), 2);

    let telegram_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-telegram")
        .expect("telegram dispatch row");
    let discord_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-discord")
        .expect("discord dispatch row");

    let first = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: approval.action_id.clone(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "Approve from telegram".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:50:05Z".into(),
        },
    )
    .expect("first channel reply should claim and resume");

    assert_eq!(
        first.claim.status,
        NotificationReplyClaimStatusDto::Accepted
    );
    assert_eq!(
        first.dispatch.status,
        NotificationDispatchStatusDto::Claimed
    );
    assert_eq!(
        first.resolve_result.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        first
            .resume_result
            .as_ref()
            .map(|resume| resume.resume_entry.status.clone()),
        Some(ResumeHistoryStatus::Started)
    );

    let duplicate = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: approval.action_id.clone(),
            route_id: discord_dispatch.route_id.clone(),
            correlation_key: discord_dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "Duplicate from discord".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:50:06Z".into(),
        },
    )
    .expect_err("late cross-channel reply should be rejected");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let claims = project_store::load_notification_reply_claims(
        &repo_root,
        &project_id,
        Some(&approval.action_id),
    )
    .expect("load cross-channel reply claims");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| { claim.status == project_store::NotificationReplyClaimStatus::Accepted }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let snapshot = project_store::load_project_snapshot(&repo_root, &project_id)
        .expect("load project snapshot")
        .snapshot;
    let approval_after = snapshot
        .approval_requests
        .iter()
        .find(|pending| pending.action_id == approval.action_id)
        .expect("approval after reply race");
    assert_eq!(approval_after.status, OperatorApprovalStatus::Approved);
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].source_action_id.as_deref(),
        Some(approval.action_id.as_str())
    );
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
}

pub(crate) fn resume_operator_run_delivers_approved_terminal_input_without_auth_event_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-resume-success",
        "session-1",
        Some("flow-1"),
        &runtime_shell::script_prompt_read_echo_and_sleep("Enter value: ", "value", "value=", 5),
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(&project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let replayed_count = match attached {
        SupervisorControlResponse::Attached { replayed_count, .. } => replayed_count,
        other => panic!("expected attach ack, got {other:?}"),
    };
    let frames = read_action_required_frames_after_attach(&mut reader, replayed_count);
    let action_id = frames
        .iter()
        .find_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item:
                    SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        action_type,
                        ..
                    },
                ..
            } => {
                assert_eq!(action_type, "terminal_input_required");
                Some(action_id.clone())
            }
            _ => None,
        })
        .expect("expected action-required replay frame");

    let missing_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: None,
        },
    )
    .expect_err("runtime-scoped approvals should fail closed when answer is missing");
    assert_eq!(missing_answer.code, "operator_action_answer_required");

    let pending_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after missing-answer resolve attempt");
    let pending_approval = pending_snapshot
        .approval_requests
        .iter()
        .find(|approval| approval.action_id == action_id)
        .expect("runtime approval should remain pending after missing-answer failure");
    assert_eq!(pending_approval.status, OperatorApprovalStatus::Pending);
    assert!(pending_snapshot.verification_records.is_empty());
    assert!(pending_snapshot.resume_history.is_empty());

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve interactive operator action");

    recorder.clear();
    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id,
            user_answer: None,
        },
    )
    .expect("resume runtime run with approved terminal input");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw_delivery = false;
    let mut saw_transcript = false;
    while Instant::now() < deadline && !(saw_delivery && saw_transcript) {
        match read_supervisor_response(&mut reader) {
            Some(SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Activity { code, .. },
                ..
            }) if code == "runtime_supervisor_input_delivered" => saw_delivery = true,
            Some(SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            }) if text == "value=approved" => saw_transcript = true,
            Some(_) => {}
            None => thread::sleep(Duration::from_millis(25)),
        }
    }

    assert!(saw_delivery, "expected input-delivered activity frame");
    assert!(saw_transcript, "expected resumed transcript output");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after runtime resume");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );

    let running = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after resume")
    .expect("runtime run should still exist");
    assert_eq!(running.status, RuntimeRunStatusDto::Running);
    assert_eq!(
        running.transport.liveness,
        RuntimeRunTransportLivenessDto::Reachable
    );
    assert!(running.last_error_code.is_none());

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop resumed runtime run")
    .expect("runtime run should exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn submit_notification_reply_persists_autonomous_boundary_and_resume_evidence_exactly_once(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    project_store::upsert_notification_route(
        &repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.clone(),
            route_id: "route-telegram".into(),
            route_kind: "telegram".into(),
            route_target: "telegram:ops-room".into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T14:59:58Z".into(),
        },
    )
    .expect("upsert telegram route for autonomous reply test");
    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-autonomous-resume-evidence",
        "session-1",
        Some("flow-1"),
        &runtime_shell::script_prompt_read_echo_and_sleep("Enter value: ", "value", "value=", 5),
    );
    seed_active_autonomous_run(&repo_root, &project_id, &launched.run.run_id);

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(&project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let replayed_count = match attached {
        SupervisorControlResponse::Attached { replayed_count, .. } => replayed_count,
        other => panic!("expected attach ack, got {other:?}"),
    };
    let frames = read_action_required_frames_after_attach(&mut reader, replayed_count);
    let (action_id, blocked_boundary_id) = frames
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
        .expect("expected action-required replay frame for autonomous resume evidence test");

    persist_supervisor_event(
        &repo_root,
        &project_id,
        &SupervisorLiveEventPayload::ActionRequired {
            action_id: action_id.clone(),
            boundary_id: blocked_boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail:
                "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run."
                    .into(),
        },
    )
    .expect("persist autonomous action-required event")
    .expect("autonomous action-required persistence should return a snapshot");

    let blocked = get_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load autonomous run after action-required persistence");
    assert_eq!(
        blocked.run.as_ref().map(|run| run.status.clone()),
        Some(AutonomousRunStatusDto::Paused)
    );
    assert_eq!(
        blocked.unit.as_ref().map(|unit| unit.status.clone()),
        Some(AutonomousUnitStatusDto::Blocked)
    );
    assert_eq!(
        blocked
            .attempt
            .as_ref()
            .map(|attempt| attempt.status.clone()),
        Some(AutonomousUnitStatusDto::Blocked)
    );

    let pending_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after autonomous boundary pause");
    assert_eq!(pending_snapshot.approval_requests.len(), 1);
    assert_eq!(pending_snapshot.approval_requests[0].action_id, action_id);
    assert!(
        action_id.contains(blocked_boundary_id.as_str()),
        "expected runtime action id to encode the blocked boundary, got {action_id}"
    );

    let dispatches =
        wait_for_notification_dispatches_for_action(&repo_root, &project_id, &action_id, 2);
    assert_eq!(dispatches.len(), 2);
    let telegram_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-telegram")
        .expect("telegram dispatch row for autonomous reply test");
    let discord_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-discord")
        .expect("discord dispatch row for autonomous reply test");

    let blocked_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after boundary pause")
        .expect("durable autonomous run should exist after boundary pause");
    let blocked_evidence = blocked_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.action_id.as_deref() == Some(action_id.as_str())
                        && payload.boundary_id.as_deref() == Some(blocked_boundary_id.as_str())
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(blocked_evidence.len(), 1);
    assert!(matches!(
        blocked_evidence[0].payload.as_ref(),
        Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
            if payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
    ));

    let first = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "approved".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:05Z".into(),
        },
    )
    .expect("first autonomous remote reply should claim, resolve, and resume");
    assert_eq!(
        first.claim.status,
        NotificationReplyClaimStatusDto::Accepted
    );
    assert_eq!(
        first.resolve_result.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        first
            .resume_result
            .as_ref()
            .map(|resume| resume.resume_entry.status.clone()),
        Some(ResumeHistoryStatus::Started)
    );

    let duplicate = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: discord_dispatch.route_id.clone(),
            correlation_key: discord_dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "duplicate after resume".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:06Z".into(),
        },
    )
    .expect_err("duplicate autonomous remote reply should fail closed after the first winner");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load autonomous remote reply claims");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| claim.status == project_store::NotificationReplyClaimStatus::Accepted));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let resumed_autonomous = get_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load autonomous run after resume");
    assert_eq!(
        resumed_autonomous
            .run
            .as_ref()
            .map(|run| run.status.clone()),
        Some(AutonomousRunStatusDto::Running)
    );
    assert_eq!(
        resumed_autonomous
            .unit
            .as_ref()
            .and_then(|unit| unit.boundary_id.as_deref()),
        None
    );
    assert_eq!(
        resumed_autonomous
            .attempt
            .as_ref()
            .and_then(|attempt| attempt.boundary_id.as_deref()),
        None
    );

    let resumed_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after resume")
        .expect("durable autonomous run should still exist after resume");
    let boundary_evidence = resumed_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.action_id.as_deref() == Some(action_id.as_str())
                        && payload.boundary_id.as_deref() == Some(blocked_boundary_id.as_str())
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(boundary_evidence.len(), 2);
    assert_eq!(
        boundary_evidence
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.payload.as_ref(),
                    Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                        if payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
                )
            })
            .count(),
        1
    );
    assert_eq!(
        boundary_evidence
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.payload.as_ref(),
                    Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                        if payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Passed
                            && payload.evidence_kind == "operator_resume"
                )
            })
            .count(),
        1
    );

    let resumed_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after autonomous remote resume");
    assert_eq!(resumed_snapshot.resume_history.len(), 1);
    assert_eq!(
        resumed_snapshot.resume_history[0]
            .source_action_id
            .as_deref(),
        Some(action_id.as_str())
    );
    assert_eq!(
        resumed_snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );

    let replayed = get_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reload autonomous run after resume");
    assert_eq!(
        replayed.run.as_ref().map(|run| run.run_id.as_str()),
        Some(launched.run.run_id.as_str())
    );
    let replayed_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("reload durable autonomous run after resume replay")
        .expect("durable autonomous run should still exist after replay");
    let replayed_boundary_evidence = replayed_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.action_id.as_deref() == Some(action_id.as_str())
                        && payload.boundary_id.as_deref() == Some(blocked_boundary_id.as_str())
            )
        })
        .count();
    assert_eq!(replayed_boundary_evidence, 2);
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &action_id),
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
    .expect("stop runtime run after autonomous resume evidence test")
    .expect("runtime run should still exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn resume_operator_run_records_failed_history_when_runtime_identity_session_is_stale() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-resume-session-mismatch",
        "session-1",
        Some("flow-1"),
        &runtime_shell::script_prompt_read_echo_and_sleep("Enter value: ", "value", "value=", 5),
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(&project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let replayed_count = match attached {
        SupervisorControlResponse::Attached { replayed_count, .. } => replayed_count,
        other => panic!("expected attach ack, got {other:?}"),
    };
    let frames = read_action_required_frames_after_attach(&mut reader, replayed_count);
    let action_id = frames
        .iter()
        .find_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::ActionRequired { action_id, .. },
                ..
            } => Some(action_id.clone()),
            _ => None,
        })
        .expect("expected action-required replay frame");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve interactive operator action");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute(
            "UPDATE operator_approvals SET session_id = 'session-stale' WHERE project_id = ?1 AND action_id = ?2",
            [project_id.as_str(), action_id.as_str()],
        )
        .expect("corrupt runtime approval session identity");

    recorder.clear();
    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("resume should fail when approved runtime session identity is stale");
    assert_eq!(error.code, "runtime_supervisor_session_mismatch");
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after stale-session resume failure");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Failed
    );
    assert_eq!(
        snapshot.resume_history[0].source_action_id.as_deref(),
        Some(action_id.as_str())
    );

    let durable_runtime_run = project_store::load_runtime_run(&repo_root, &project_id)
        .expect("load durable runtime run after stale-session resume failure")
        .expect("durable runtime run should still exist after stale-session resume failure");
    assert_eq!(
        durable_runtime_run
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_session_mismatch")
    );

    let runtime_run = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after stale-session resume failure")
    .expect("runtime run should still exist after stale-session resume failure");
    assert_eq!(runtime_run.status, RuntimeRunStatusDto::Running);
    assert_eq!(
        runtime_run.transport.liveness,
        RuntimeRunTransportLivenessDto::Reachable
    );

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop runtime run after stale-session resume failure")
    .expect("runtime run should exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn resume_operator_run_records_failed_history_when_detached_control_channel_is_unreachable(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_unreachable_runtime_run(&repo_root, &project_id, "run-submit-failed");
    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: "run-submit-failed".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T19:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary: "Detached runtime blocked on terminal input and is awaiting operator approval.".into(),
            created_at: "2026-04-15T19:00:12Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: persisted.approval_request.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve unreachable runtime action");

    recorder.clear();
    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: persisted.approval_request.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail when detached control channel is unreachable");
    assert_eq!(error.code, "runtime_supervisor_connect_failed");
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after failed runtime resume");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Failed
    );
    assert_eq!(
        snapshot.resume_history[0].source_action_id.as_deref(),
        Some(snapshot.approval_requests[0].action_id.as_str())
    );

    let durable_runtime_run = project_store::load_runtime_run(&repo_root, &project_id)
        .expect("load durable runtime run after failed resume")
        .expect("durable runtime run should still exist after failed resume");
    assert_eq!(
        durable_runtime_run
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_connect_failed")
    );

    let runtime_run = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after failed resume")
    .expect("runtime run should still exist after failed resume");
    assert_eq!(runtime_run.status, RuntimeRunStatusDto::Stale);
    assert_eq!(
        runtime_run.transport.liveness,
        RuntimeRunTransportLivenessDto::Unreachable
    );
}

pub(crate) fn submit_notification_reply_resumes_shell_review_boundary_without_duplicate_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let run_id = "run-shell-review-approve";
    let boundary_id = "boundary-shell-review-approve";
    let action_type = "shell_policy_review";
    let action_id = project_store::derive_runtime_action_id(
        "session-1",
        Some("flow-1"),
        run_id,
        boundary_id,
        action_type,
    )
    .expect("derive canonical shell-review action id for bridge test");
    let script = runtime_shell::script_join_steps(&[
        runtime_shell::script_print_line(&format!(
            "__Cadence_EVENT__ {}",
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

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        run_id,
        "session-1",
        Some("flow-1"),
        &script,
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load shell-review snapshot before remote approval");
    assert_eq!(snapshot.approval_requests.len(), 1);
    let approval = &snapshot.approval_requests[0];
    assert_eq!(approval.action_type, "shell_policy_review");

    let dispatch = wait_for_notification_dispatches_for_action(
        &repo_root,
        &project_id,
        &approval.action_id,
        1,
    )
    .into_iter()
    .next()
    .expect("shell-review notification dispatch should exist");

    let first = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: approval.action_id.clone(),
            route_id: dispatch.route_id.clone(),
            correlation_key: dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "approve shell review".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T20:50:05Z".into(),
        },
    )
    .expect("shell-review reply should claim, resolve, and resume");
    assert_eq!(
        first.claim.status,
        NotificationReplyClaimStatusDto::Accepted
    );
    assert_eq!(
        first.resolve_result.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        first
            .resume_result
            .as_ref()
            .map(|resume| resume.resume_entry.status.clone()),
        Some(ResumeHistoryStatus::Started)
    );

    let duplicate = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: approval.action_id.clone(),
            route_id: dispatch.route_id,
            correlation_key: first.dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator-2".into()),
            reply_text: "duplicate shell review".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T20:50:06Z".into(),
        },
    )
    .expect_err("duplicate shell-review reply should fail closed");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let resumed_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load shell-review snapshot after remote approval");
    assert_eq!(resumed_snapshot.resume_history.len(), 1);
    assert_eq!(
        resumed_snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &approval.action_id),
        1
    );

    let runtime_run = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get shell-review runtime run after remote approval")
    .expect("shell-review runtime run should still exist after remote approval");
    assert_eq!(runtime_run.status, RuntimeRunStatusDto::Running);
    assert_eq!(
        runtime_run.transport.liveness,
        RuntimeRunTransportLivenessDto::Reachable
    );

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop shell-review runtime run after remote approval")
    .expect("shell-review runtime run should still exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}
