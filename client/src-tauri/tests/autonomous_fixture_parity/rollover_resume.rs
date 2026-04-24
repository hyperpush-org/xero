use super::support::*;

pub(crate) fn autonomous_fixture_repo_parity_proves_stage_rollover_boundary_resume_and_reload_identity(
) {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id);
    upsert_notification_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:ops-room",
    );
    upsert_notification_route(
        &repo_root,
        &project_id,
        "route-discord",
        "discord",
        "discord:ops-room",
    );

    let browser_story_script = runtime_shell::script_join_steps(&[
        runtime_shell::script_sleep(2),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-inspect-1",
                "tool_name": "inspect_repository",
                "tool_state": "running",
                "detail": "Collecting deterministic fixture proof context"
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-inspect-1",
                "tool_name": "inspect_repository",
                "tool_state": "succeeded",
                "detail": "Collected deterministic fixture proof context"
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-browser-open",
                "tool_name": "browser.open",
                "tool_state": "succeeded",
                "detail": "Opened in-app browser context",
                "tool_summary": {
                    "kind": "browser_computer_use",
                    "surface": "browser",
                    "action": "open",
                    "status": "succeeded",
                    "target": "https://example.com",
                    "outcome": "Opened browser context"
                }
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-browser-tab-open",
                "tool_name": "browser.tab_open",
                "tool_state": "succeeded",
                "detail": "Opened docs tab",
                "tool_summary": {
                    "kind": "browser_computer_use",
                    "surface": "browser",
                    "action": "tab_open",
                    "status": "succeeded",
                    "target": "https://example.com/docs",
                    "outcome": "Opened tab tab-2"
                }
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-browser-tab-focus",
                "tool_name": "browser.tab_focus",
                "tool_state": "failed",
                "detail": "policy_denied_browser_permissions",
                "tool_summary": {
                    "kind": "browser_computer_use",
                    "surface": "browser",
                    "action": "tab_focus",
                    "status": "blocked",
                    "target": "tab-2",
                    "outcome": "Permission denied"
                }
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-browser-current-url",
                "tool_name": "browser.current_url",
                "tool_state": "succeeded",
                "detail": "Read active tab URL",
                "tool_summary": {
                    "kind": "browser_computer_use",
                    "surface": "browser",
                    "action": "current_url",
                    "status": "succeeded",
                    "target": "tab-2",
                    "outcome": "https://example.com/docs"
                }
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "activity",
                "code": "policy_denied_write_access",
                "title": "Policy denied write access",
                "detail": "Cadence blocked repository writes until operator approval resumes the active boundary"
            })
        )),
        runtime_shell::script_prompt_read_echo_and_sleep(
            "Enter approval code: ",
            "value",
            "value=",
            5,
        ),
    ]);

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-autonomous-fixture-parity",
        "session-1",
        Some("flow-1"),
        &browser_story_script,
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });

    let progressed = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous_state.attempt.as_ref() else {
            return false;
        };
        let Some(linkage) = unit.workflow_linkage.as_ref() else {
            return false;
        };

        run.run_id == launched.run.run_id
            && autonomous_state.history.len() == 3
            && unit.sequence == 3
            && attempt.attempt_number == 3
            && unit.kind == AutonomousUnitKindDto::Planner
            && linkage.workflow_node_id == "roadmap"
            && attempt.workflow_linkage.as_ref() == Some(linkage)
    });

    let progressed_run = progressed
        .run
        .as_ref()
        .expect("progressed autonomous run should exist");
    let progressed_unit = progressed
        .unit
        .as_ref()
        .expect("progressed autonomous unit should exist");
    let progressed_attempt = progressed
        .attempt
        .as_ref()
        .expect("progressed autonomous attempt should exist");
    let progressed_linkage = progressed_unit
        .workflow_linkage
        .as_ref()
        .expect("progressed unit should expose workflow linkage");
    let progressed_shape = history_shape(&progressed);

    assert_eq!(progressed_run.run_id, launched.run.run_id);
    assert_eq!(progressed_unit.run_id, launched.run.run_id);
    assert_eq!(progressed_attempt.run_id, launched.run.run_id);
    assert_eq!(
        progressed_run.active_unit_id.as_deref(),
        Some(progressed_unit.unit_id.as_str())
    );
    assert_eq!(
        progressed_run.active_attempt_id.as_deref(),
        Some(progressed_attempt.attempt_id.as_str())
    );
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 3);
    assert_eq!(
        progressed
            .history
            .iter()
            .map(|entry| entry.unit.sequence)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert_eq!(
        progressed
            .history
            .iter()
            .map(|entry| {
                entry
                    .unit
                    .workflow_linkage
                    .as_ref()
                    .expect("workflow linkage")
                    .workflow_node_id
                    .clone()
            })
            .collect::<Vec<_>>(),
        vec!["roadmap", "requirements", "research"]
    );

    let durable_progressed = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable progressed autonomous run")
        .expect("durable progressed autonomous run should exist");
    let roadmap_transition =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load lifecycle transition events")
            .into_iter()
            .find(|event| event.to_node_id == "roadmap")
            .expect("roadmap transition should exist");
    let roadmap_handoff = project_store::load_workflow_handoff_package(
        &repo_root,
        &project_id,
        &roadmap_transition.transition_id,
    )
    .expect("load roadmap handoff package")
    .expect("roadmap handoff package should exist");
    assert_eq!(progressed_linkage.workflow_node_id, "roadmap");
    assert_eq!(
        progressed_linkage.transition_id,
        roadmap_transition.transition_id
    );
    assert_eq!(
        progressed_linkage.causal_transition_id.as_deref(),
        roadmap_transition.causal_transition_id.as_deref()
    );
    assert_eq!(
        progressed_linkage.handoff_transition_id,
        roadmap_handoff.handoff_transition_id
    );
    assert_eq!(
        progressed_linkage.handoff_package_hash,
        roadmap_handoff.package_hash
    );
    assert_eq!(durable_progressed.history.len(), 3);

    thread::sleep(Duration::from_secs(3));
    let boundary_id = "boundary-1".to_string();
    let persisted_boundary = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: launched.run.run_id.clone(),
            runtime_kind: launched.run.runtime_kind.clone(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: launched.run.transport.endpoint.clone(),
            started_at: launched.run.started_at.clone(),
            last_heartbeat_at: launched.run.last_heartbeat_at.clone(),
            last_error: None,
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-18T19:00:00Z".into(),
        },
    )
    .expect("persist runtime action-required boundary for fixture parity proof");
    let action_id = persisted_boundary.approval_request.action_id.clone();
    persist_supervisor_event(
        &repo_root,
        &project_id,
        &SupervisorLiveEventPayload::ActionRequired {
            action_id: action_id.clone(),
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
        },
    )
    .expect("persist autonomous action-required event for fixture parity proof")
    .expect("autonomous action-required persistence should return a snapshot");

    let paused = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous_state.attempt.as_ref() else {
            return false;
        };

        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Paused
            && unit.status == AutonomousUnitStatusDto::Blocked
            && attempt.status == AutonomousUnitStatusDto::Blocked
            && unit.boundary_id == attempt.boundary_id
            && autonomous_state.history.len() == 3
            && autonomous_state
                .history
                .first()
                .is_some_and(|entry| entry.artifacts.len() >= 4)
    });

    let paused_run = paused
        .run
        .as_ref()
        .expect("paused autonomous run should exist");
    let paused_unit = paused
        .unit
        .as_ref()
        .expect("paused autonomous unit should exist");
    let paused_attempt = paused
        .attempt
        .as_ref()
        .expect("paused autonomous attempt should exist");
    let paused_shape = history_shape(&paused);

    assert_eq!(paused_shape, progressed_shape);
    assert_eq!(paused_run.run_id, launched.run.run_id);
    assert_eq!(paused_unit.sequence, 3);
    assert_eq!(paused_attempt.attempt_number, 3);
    assert_eq!(
        paused_unit.workflow_linkage.as_ref(),
        Some(progressed_linkage)
    );
    assert_eq!(
        paused_attempt.workflow_linkage.as_ref(),
        Some(progressed_linkage)
    );
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &launched.run.run_id),
        3
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
    assert!(pending_snapshot.resume_history.is_empty());
    let approval = &pending_snapshot.approval_requests[0];
    assert_eq!(approval.action_id, action_id);
    assert_eq!(approval.status, OperatorApprovalStatus::Pending);
    assert!(action_id.contains(boundary_id.as_str()));

    let dispatches =
        wait_for_notification_dispatches_for_action(&repo_root, &project_id, &action_id, 2);
    let telegram_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-telegram")
        .expect("telegram dispatch row should exist");
    let discord_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-discord")
        .expect("discord dispatch row should exist");
    assert!(dispatches
        .iter()
        .all(|dispatch| { dispatch.status == project_store::NotificationDispatchStatus::Pending }));

    let paused_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after boundary pause")
        .expect("durable autonomous run should exist after boundary pause");
    let paused_artifacts = paused_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.attempt_id == paused_attempt.attempt_id)
        .collect::<Vec<_>>();
    assert_eq!(paused_artifacts.len(), 9);
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "tool_result")
            .count(),
        6
    );
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "policy_denied")
            .count(),
        1
    );
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "verification_evidence")
            .count(),
        2
    );
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-inspect-1"
                    && payload.tool_name == "inspect_repository"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Running
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-inspect-1"
                    && payload.tool_name == "inspect_repository"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Succeeded
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-browser-open"
                    && payload.tool_name == "browser.open"
                    && payload.tool_state
                        == project_store::AutonomousToolCallStateRecord::Succeeded
                    && matches!(
                        payload.tool_summary.as_ref(),
                        Some(
                            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                                summary,
                            ),
                        ) if summary.action == "open"
                    )
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-browser-tab-open"
                    && payload.tool_name == "browser.tab_open"
                    && payload.tool_state
                        == project_store::AutonomousToolCallStateRecord::Succeeded
                    && matches!(
                        payload.tool_summary.as_ref(),
                        Some(
                            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                                summary,
                            ),
                        ) if summary.action == "tab_open"
                    )
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-browser-tab-focus"
                    && payload.tool_name == "browser.tab_focus"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Failed
                    && matches!(
                        payload.tool_summary.as_ref(),
                        Some(
                            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                                summary,
                            ),
                        ) if summary.action == "tab_focus"
                            && summary.outcome.as_deref() == Some(
                                "advanced_browser_failure_policy_permission: Browser/computer-use action was blocked by policy or permissions. Grant the required access or approve the boundary before retrying."
                            )
                    )
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-browser-current-url"
                    && payload.tool_name == "browser.current_url"
                    && payload.tool_state
                        == project_store::AutonomousToolCallStateRecord::Succeeded
                    && matches!(
                        payload.tool_summary.as_ref(),
                        Some(
                            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                                summary,
                            ),
                        ) if summary.action == "current_url"
                            && summary.outcome.as_deref() == Some("https://example.com/docs")
                    )
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::PolicyDenied(payload))
                if payload.diagnostic_code == "policy_denied_write_access"
                    && payload.message == "Cadence blocked repository writes until operator approval resumes the active boundary"
                    && payload.action_id.as_deref() == Some(action_id.as_str())
                    && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                if payload.evidence_kind == "policy_denied_write_access"
                    && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Failed
                    && payload.action_id.as_deref() == Some(action_id.as_str())
                    && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                if payload.action_id.as_deref() == Some(action_id.as_str())
                    && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                    && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
                    && payload.evidence_kind == "terminal_input_required"
        )
    }));

    let fresh_paused_app = build_mock_app(create_state(&root));
    let recovered_paused = wait_for_autonomous_run(&fresh_paused_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Paused
            && attempt.boundary_id.as_deref() == Some(boundary_id.as_str())
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&recovered_paused), paused_shape);

    let reloaded_pending_snapshot = get_project_snapshot(
        fresh_paused_app.handle().clone(),
        fresh_paused_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load paused project snapshot after fresh host reload");
    assert_eq!(reloaded_pending_snapshot.approval_requests.len(), 1);
    assert_eq!(
        reloaded_pending_snapshot.approval_requests[0].action_id,
        action_id
    );
    assert!(reloaded_pending_snapshot.resume_history.is_empty());

    let blank_action_error = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: "   ".into(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "approved".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:05Z".into(),
        },
    )
    .expect_err("blank action id should fail closed");
    assert_eq!(blank_action_error.code, "invalid_request");

    let forged_error = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: discord_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "forged correlation".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:06Z".into(),
        },
    )
    .expect_err("forged correlation should fail closed");
    assert_eq!(forged_error.code, "notification_reply_correlation_invalid");
    let malformed_claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load notification reply claims after malformed replies");
    assert_eq!(malformed_claims.len(), 1);
    assert!(malformed_claims.iter().all(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_correlation_invalid")
    }));
    let still_paused_snapshot = get_project_snapshot(
        fresh_paused_app.handle().clone(),
        fresh_paused_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after malformed replies");
    assert!(still_paused_snapshot.resume_history.is_empty());
    assert!(still_paused_snapshot
        .approval_requests
        .iter()
        .any(|approval_request| {
            approval_request.action_id == action_id
                && approval_request.status == OperatorApprovalStatus::Pending
        }));

    let first = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "approved".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:07Z".into(),
        },
    )
    .expect("first remote reply should claim, resolve, and resume exactly once");
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
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: discord_dispatch.route_id.clone(),
            correlation_key: discord_dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "duplicate after resume".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:08Z".into(),
        },
    )
    .expect_err("duplicate reply after the winner should fail closed");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let resumed = wait_for_autonomous_run(&fresh_paused_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Running
            && attempt.boundary_id.is_none()
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&resumed), paused_shape);

    let resumed_runtime = wait_for_runtime_run(&fresh_paused_app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });
    assert_eq!(resumed_runtime.run_id, launched.run.run_id);

    let claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load reply claims after accepted and duplicate replies");
    assert_eq!(claims.len(), 3);
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Accepted
            && claim.route_id == telegram_dispatch.route_id
    }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.route_id == discord_dispatch.route_id
            && claim.rejection_code.as_deref() == Some("notification_reply_correlation_invalid")
    }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.route_id == discord_dispatch.route_id
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let dispatches_after =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load notification dispatches after remote replies");
    assert_eq!(
        dispatches_after
            .iter()
            .filter(|dispatch| dispatch.status == project_store::NotificationDispatchStatus::Claimed)
            .count(),
        1
    );
    assert!(dispatches_after.iter().any(|dispatch| {
        dispatch.route_id == telegram_dispatch.route_id
            && dispatch.status == project_store::NotificationDispatchStatus::Claimed
    }));

    let resumed_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after resume")
        .expect("durable autonomous run should still exist after resume");
    let resumed_artifacts = resumed_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.attempt_id == paused_attempt.attempt_id)
        .collect::<Vec<_>>();
    assert_eq!(resumed_artifacts.len(), 10);
    assert_eq!(
        resumed_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "tool_result")
            .count(),
        6
    );
    assert!(resumed_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-browser-tab-focus"
                    && payload.tool_name == "browser.tab_focus"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Failed
                    && matches!(
                        payload.tool_summary.as_ref(),
                        Some(
                            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                                summary,
                            ),
                        ) if summary.action == "tab_focus"
                            && summary.outcome.as_deref() == Some(
                                "advanced_browser_failure_policy_permission: Browser/computer-use action was blocked by policy or permissions. Grant the required access or approve the boundary before retrying."
                            )
                    )
        )
    }));
    assert!(resumed_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-browser-current-url"
                    && payload.tool_name == "browser.current_url"
                    && payload.tool_state
                        == project_store::AutonomousToolCallStateRecord::Succeeded
                    && matches!(
                        payload.tool_summary.as_ref(),
                        Some(
                            cadence_desktop_lib::runtime::protocol::ToolResultSummary::BrowserComputerUse(
                                summary,
                            ),
                        ) if summary.action == "current_url"
                            && summary.outcome.as_deref() == Some("https://example.com/docs")
                    )
        )
    }));
    assert_eq!(
        resumed_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "policy_denied")
            .count(),
        1
    );
    let boundary_evidence = resumed_artifacts
        .iter()
        .filter(|artifact| artifact.artifact_kind == "verification_evidence")
        .collect::<Vec<_>>();
    assert_eq!(boundary_evidence.len(), 3);
    assert_eq!(
        boundary_evidence
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.payload.as_ref(),
                    Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                        if payload.evidence_kind == "policy_denied_write_access"
                            && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Failed
                            && payload.action_id.as_deref() == Some(action_id.as_str())
                            && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
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
                        if payload.action_id.as_deref() == Some(action_id.as_str())
                            && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                            && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
                            && payload.evidence_kind == "terminal_input_required"
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
                        if payload.action_id.as_deref() == Some(action_id.as_str())
                            && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                            && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Passed
                            && payload.evidence_kind == "operator_resume"
                )
            })
            .count(),
        1
    );

    let resumed_snapshot = get_project_snapshot(
        fresh_paused_app.handle().clone(),
        fresh_paused_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after remote resume");
    assert_eq!(resumed_snapshot.resume_history.len(), 1);
    assert_eq!(
        resumed_snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
    assert_eq!(
        resumed_snapshot.resume_history[0]
            .source_action_id
            .as_deref(),
        Some(action_id.as_str())
    );
    assert!(resumed_snapshot
        .approval_requests
        .iter()
        .any(|approval_request| {
            approval_request.action_id == action_id
                && approval_request.status == OperatorApprovalStatus::Approved
        }));

    let fresh_resumed_app = build_mock_app(create_state(&root));
    let replayed = wait_for_autonomous_run(&fresh_resumed_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && attempt.boundary_id.is_none()
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&replayed), paused_shape);
    assert_eq!(
        replayed
            .run
            .as_ref()
            .map(|run| run.active_unit_id.as_deref()),
        resumed
            .run
            .as_ref()
            .map(|run| run.active_unit_id.as_deref())
    );
    assert_eq!(
        replayed
            .run
            .as_ref()
            .map(|run| run.active_attempt_id.as_deref()),
        resumed
            .run
            .as_ref()
            .map(|run| run.active_attempt_id.as_deref())
    );
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 3);
    assert_eq!(
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("reload notification reply claims after resume replay")
            .len(),
        3
    );
    assert_eq!(
        get_project_snapshot(
            fresh_resumed_app.handle().clone(),
            fresh_resumed_app.state::<DesktopState>(),
            ProjectIdRequestDto {
                project_id: project_id.clone(),
            },
        )
        .expect("reload project snapshot after resume replay")
        .resume_history
        .len(),
        1
    );

    let stopped = stop_runtime_run(
        fresh_resumed_app.handle().clone(),
        fresh_resumed_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop runtime run after fixture parity proof")
    .expect("runtime run should still exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}
