use super::support::*;

pub(crate) fn apply_workflow_transition_gate_pause_returns_skipped_diagnostics_and_truthful_project_update(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_blocked_auto_continuation(
        &repo_root,
        &project_id,
        "approve_execution",
        "approve_verify",
    );
    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let request = ApplyWorkflowTransitionRequestDto {
        project_id: project_id.clone(),
        transition_id: "transition-gate-pause-1".into(),
        causal_transition_id: None,
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: "approved".into(),
        gate_decision_context: Some("approved by operator".into()),
        gate_updates: vec![
            cadence_desktop_lib::commands::WorkflowTransitionGateUpdateRequestDto {
                gate_key: "execution_gate".into(),
                gate_state: "satisfied".into(),
                decision_context: Some("approved by operator".into()),
            },
        ],
        occurred_at: "2026-04-16T15:00:00Z".into(),
    };

    recorder.clear();
    let applied = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        request.clone(),
    )
    .expect("apply transition should persist and return skipped automatic-dispatch diagnostics");

    assert_eq!(
        applied.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        applied.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    let applied_message = applied
        .automatic_dispatch
        .message
        .as_deref()
        .expect("skipped automatic-dispatch outcome should include diagnostics");
    assert!(
        applied_message.contains("Persisted pending operator approval"),
        "expected persisted-approval diagnostics, got {applied_message}"
    );
    assert!(
        applied.automatic_dispatch.transition_event.is_none(),
        "skipped automatic-dispatch outcome must not fabricate a transition event"
    );
    assert!(
        applied.automatic_dispatch.handoff_package.is_none(),
        "skipped automatic-dispatch outcome must not fabricate a handoff package"
    );

    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after gate pause");
    let pending_verify_approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| {
            approval.gate_key.as_deref() == Some("verify_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .expect("expected pending verify-gate approval persisted from skipped automatic dispatch");
    assert_eq!(
        pending_verify_approval.gate_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_from_node_id.as_deref(),
        Some("execute")
    );
    assert_eq!(
        pending_verify_approval.transition_to_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_kind.as_deref(),
        Some("advance")
    );
    assert_eq!(pending_verify_approval.action_type, "approve_verify");

    let persisted_action_id = pending_verify_approval.action_id.clone();
    assert!(
        applied_message.contains(persisted_action_id.as_str()),
        "expected skipped diagnostics to include deterministic persisted action id, got {applied_message}"
    );

    let initial_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &persisted_action_id);
    assert_eq!(initial_dispatches.len(), 1);
    assert_eq!(
        initial_dispatches[0].status,
        project_store::NotificationDispatchStatus::Pending
    );

    let initial_events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after apply gate pause");
    assert_eq!(initial_events.len(), 1);
    assert_eq!(initial_events[0].transition_id, request.transition_id);
    assert_eq!(initial_events[0].from_node_id, "plan");
    assert_eq!(initial_events[0].to_node_id, "execute");

    recorder.clear();
    let replayed =
        apply_workflow_transition(app.handle().clone(), app.state::<DesktopState>(), request)
            .expect("replayed apply transition should remain idempotent with skipped diagnostics");

    assert_eq!(
        replayed.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        replayed.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    assert!(replayed.automatic_dispatch.transition_event.is_none());
    assert!(replayed.automatic_dispatch.handoff_package.is_none());

    let replayed_message = replayed
        .automatic_dispatch
        .message
        .as_deref()
        .expect("replayed skipped diagnostics should include message");
    assert!(
        replayed_message.contains(persisted_action_id.as_str()),
        "expected replayed skipped diagnostics to keep deterministic action id, got {replayed_message}"
    );

    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let replay_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after replayed apply gate pause");
    let replay_pending_verify: Vec<_> = replay_snapshot
        .approval_requests
        .iter()
        .filter(|approval| {
            approval.gate_key.as_deref() == Some("verify_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .collect();
    assert_eq!(replay_pending_verify.len(), 1);
    assert_eq!(replay_pending_verify[0].action_id, persisted_action_id);

    let replay_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &persisted_action_id);
    assert_eq!(replay_dispatches.len(), 1);
    assert_eq!(replay_dispatches[0].id, initial_dispatches[0].id);

    let replay_events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after replayed apply gate pause");
    assert_eq!(replay_events, initial_events);
}

pub(crate) fn gate_linked_resume_gate_pause_returns_skipped_diagnostics_without_runtime_event_drift(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_blocked_auto_continuation(
        &repo_root,
        &project_id,
        "approve_execution",
        "approve_verify",
    );

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T15:10:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    recorder.clear();

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("resume should apply gate transition and persist skipped auto-dispatch diagnostics");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    let auto_dispatch = resumed
        .automatic_dispatch
        .expect("gate-linked resume should include automatic-dispatch diagnostics");
    assert_eq!(
        auto_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        auto_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    let auto_dispatch_message = auto_dispatch
        .message
        .as_deref()
        .expect("skipped diagnostics should include message");
    assert!(
        auto_dispatch_message.contains("Persisted pending operator approval"),
        "expected persisted-approval diagnostics, got {auto_dispatch_message}"
    );
    assert!(
        auto_dispatch.transition_event.is_none(),
        "skipped auto-dispatch diagnostics must not fabricate transition payloads"
    );
    assert!(
        auto_dispatch.handoff_package.is_none(),
        "skipped auto-dispatch diagnostics must not fabricate handoff payloads"
    );

    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let project_update = recorder
        .latest_project_update()
        .expect("expected project:updated payload after resume gate pause");
    assert_eq!(project_update.project.id, project_id);
    assert_eq!(project_update.reason, ProjectUpdateReason::MetadataChanged);
    assert_eq!(project_update.project.active_phase, 2);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after resume gate pause");

    let pending_verify_approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| {
            approval.gate_key.as_deref() == Some("verify_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .expect("expected pending verify-gate approval persisted from resume auto-dispatch skip");
    assert_eq!(
        pending_verify_approval.gate_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_from_node_id.as_deref(),
        Some("execute")
    );
    assert_eq!(
        pending_verify_approval.transition_to_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_kind.as_deref(),
        Some("advance")
    );

    assert!(
        auto_dispatch_message.contains(pending_verify_approval.action_id.as_str()),
        "expected skipped diagnostics to include persisted action id, got {auto_dispatch_message}"
    );

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after resume gate pause");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].from_node_id, "plan");
    assert_eq!(events[0].to_node_id, "execute");
    assert!(
        events[0].transition_id.starts_with("resume:"),
        "expected deterministic resume transition id, got {}",
        events[0].transition_id
    );
}

pub(crate) fn gate_linked_resume_auto_dispatch_emits_project_update_without_runtime_event_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_auto_continuation(&repo_root, &project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T15:00:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    recorder.clear();

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("resume should auto-dispatch continuation without manual apply command");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let project_update = recorder
        .latest_project_update()
        .expect("expected project:updated payload after resume auto-dispatch");
    assert_eq!(project_update.project.id, project_id);
    assert_eq!(project_update.reason, ProjectUpdateReason::MetadataChanged);
    assert_eq!(project_update.project.active_phase, 3);
    assert_eq!(project_update.project.completed_phases, 2);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after gate-linked auto-dispatch");
    assert_eq!(events.len(), 2);

    let primary_event = events
        .iter()
        .find(|event| event.from_node_id == "plan" && event.to_node_id == "execute")
        .expect("primary gate-linked transition event");
    let auto_event = events
        .iter()
        .find(|event| event.from_node_id == "execute" && event.to_node_id == "verify")
        .expect("automatic continuation transition event");

    assert!(
        primary_event.transition_id.starts_with("resume:"),
        "expected deterministic resume transition id, got {}",
        primary_event.transition_id
    );
    assert!(
        auto_event.transition_id.starts_with("auto:"),
        "expected deterministic auto transition id, got {}",
        auto_event.transition_id
    );
    assert_eq!(
        auto_event.causal_transition_id.as_deref(),
        Some(primary_event.transition_id.as_str())
    );

    let persisted_auto_package = project_store::load_workflow_handoff_package(
        &repo_root,
        &project_id,
        &auto_event.transition_id,
    )
    .expect("load persisted handoff package for runtime bridge auto transition")
    .expect("handoff package row should exist for runtime bridge auto transition");
    let persisted_payload: serde_json::Value =
        serde_json::from_str(&persisted_auto_package.package_payload)
            .expect("decode runtime bridge auto handoff payload");
    assert_eq!(
        persisted_payload["triggerTransition"]["transitionId"],
        auto_event.transition_id
    );
    assert_eq!(
        persisted_payload["triggerTransition"]["causalTransitionId"],
        primary_event.transition_id
    );

    let reloaded_events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("reload transition events after gate-linked auto-dispatch");
    assert_eq!(events, reloaded_events);
}

pub(crate) fn planning_lifecycle_completion_branch_auto_dispatches_to_roadmap_without_duplicate_rows(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id, false);

    let discussion_to_research = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: project_id.clone(),
            transition_id: "lifecycle-discussion-research-1".into(),
            causal_transition_id: None,
            from_node_id: "discussion".into(),
            to_node_id: "research".into(),
            transition_kind: "advance".into(),
            gate_decision: "approved".into(),
            gate_decision_context: Some("discussion complete".into()),
            gate_updates: Vec::new(),
            occurred_at: "2026-04-16T16:00:00Z".into(),
        },
    )
    .expect("discussion -> research transition should persist and auto-dispatch to requirements");

    assert_eq!(
        discussion_to_research.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Applied
    );

    let research_to_requirements = discussion_to_research
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("discussion -> research should auto-dispatch research -> requirements");
    assert_eq!(research_to_requirements.from_node_id, "research");
    assert_eq!(research_to_requirements.to_node_id, "requirements");
    assert_eq!(
        research_to_requirements.causal_transition_id.as_deref(),
        Some("lifecycle-discussion-research-1")
    );

    let research_handoff = discussion_to_research
        .automatic_dispatch
        .handoff_package
        .clone()
        .and_then(|outcome| outcome.package)
        .expect("research -> requirements auto-dispatch should persist a handoff package");
    assert_eq!(
        research_handoff.handoff_transition_id,
        research_to_requirements.transition_id
    );
    assert_eq!(
        research_handoff.causal_transition_id.as_deref(),
        Some("lifecycle-discussion-research-1")
    );

    let requirements_trigger = replay_transition_request(&project_id, &research_to_requirements);

    let requirements_to_roadmap = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger.clone(),
    )
    .expect("replayed requirements trigger should auto-dispatch into roadmap");

    assert_eq!(
        requirements_to_roadmap.transition_event.transition_id,
        research_to_requirements.transition_id
    );
    assert_eq!(
        requirements_to_roadmap.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Applied
    );

    let roadmap_transition = requirements_to_roadmap
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("requirements replay should auto-dispatch requirements -> roadmap");
    assert_eq!(roadmap_transition.from_node_id, "requirements");
    assert_eq!(roadmap_transition.to_node_id, "roadmap");
    assert_eq!(
        roadmap_transition.causal_transition_id.as_deref(),
        Some(research_to_requirements.transition_id.as_str())
    );

    let roadmap_handoff = requirements_to_roadmap
        .automatic_dispatch
        .handoff_package
        .clone()
        .and_then(|outcome| outcome.package)
        .expect("requirements -> roadmap auto-dispatch should persist a handoff package");
    assert_eq!(
        roadmap_handoff.handoff_transition_id,
        roadmap_transition.transition_id
    );
    assert_eq!(
        roadmap_handoff.causal_transition_id.as_deref(),
        Some(research_to_requirements.transition_id.as_str())
    );

    let replayed_requirements_trigger = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger,
    )
    .expect("replaying the same requirements trigger should remain idempotent");

    assert_eq!(
        replayed_requirements_trigger.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Replayed
    );
    let replayed_roadmap_transition = replayed_requirements_trigger
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("replayed requirements trigger should return persisted roadmap transition");
    assert_eq!(
        replayed_roadmap_transition.transition_id,
        roadmap_transition.transition_id
    );

    let replayed_roadmap_handoff = replayed_requirements_trigger
        .automatic_dispatch
        .handoff_package
        .clone()
        .and_then(|outcome| outcome.package)
        .expect("replayed requirements trigger should replay persisted roadmap handoff package");
    assert_eq!(
        replayed_roadmap_handoff.package_hash,
        roadmap_handoff.package_hash
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 2);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        0
    );

    let persisted_roadmap_handoff = project_store::load_workflow_handoff_package(
        &repo_root,
        &project_id,
        &roadmap_transition.transition_id,
    )
    .expect("load persisted roadmap handoff package")
    .expect("roadmap transition should have a persisted handoff package");
    assert_eq!(
        persisted_roadmap_handoff.package_hash,
        roadmap_handoff.package_hash
    );

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after lifecycle completion branch");

    assert!(
        snapshot
            .approval_requests
            .iter()
            .all(|approval| approval.status != OperatorApprovalStatus::Pending),
        "completion branch should not leave pending operator approvals"
    );

    let roadmap_stage = snapshot
        .lifecycle
        .stages
        .iter()
        .find(|stage| stage.node_id == "roadmap")
        .expect("roadmap lifecycle stage should exist");
    assert_eq!(roadmap_stage.status, PhaseStatus::Active);
    assert!(
        !roadmap_stage.action_required,
        "roadmap stage should be actionable-free in completion branch"
    );
}

pub(crate) fn planning_lifecycle_gate_pause_branch_requires_explicit_resume_without_duplicate_rows()
{
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id, true);

    let discussion_to_research = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: project_id.clone(),
            transition_id: "lifecycle-discussion-research-pause-1".into(),
            causal_transition_id: None,
            from_node_id: "discussion".into(),
            to_node_id: "research".into(),
            transition_kind: "advance".into(),
            gate_decision: "approved".into(),
            gate_decision_context: Some("discussion complete".into()),
            gate_updates: Vec::new(),
            occurred_at: "2026-04-16T16:10:00Z".into(),
        },
    )
    .expect("discussion -> research transition should persist and auto-dispatch to requirements");

    let research_to_requirements = discussion_to_research
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("discussion -> research should auto-dispatch research -> requirements");

    let requirements_trigger = replay_transition_request(&project_id, &research_to_requirements);

    let paused = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger.clone(),
    )
    .expect("requirements replay should pause at unresolved roadmap gate");

    assert_eq!(
        paused.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        paused.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    assert!(paused.automatic_dispatch.transition_event.is_none());
    assert!(paused.automatic_dispatch.handoff_package.is_none());

    let pause_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after lifecycle gate pause");

    let pending_roadmap_approvals: Vec<_> = pause_snapshot
        .approval_requests
        .iter()
        .filter(|approval| {
            approval.gate_key.as_deref() == Some("roadmap_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .collect();
    assert_eq!(pending_roadmap_approvals.len(), 1);

    let pending_roadmap_approval = pending_roadmap_approvals[0];
    assert_eq!(
        pending_roadmap_approval.gate_node_id.as_deref(),
        Some("roadmap")
    );
    assert_eq!(
        pending_roadmap_approval.transition_from_node_id.as_deref(),
        Some("requirements")
    );
    assert_eq!(
        pending_roadmap_approval.transition_to_node_id.as_deref(),
        Some("roadmap")
    );
    assert_eq!(
        pending_roadmap_approval.transition_kind.as_deref(),
        Some("advance")
    );
    assert_eq!(pending_roadmap_approval.action_type, "approve_roadmap");

    let pending_action_id = pending_roadmap_approval.action_id.clone();
    let paused_message = paused
        .automatic_dispatch
        .message
        .as_deref()
        .expect("gate pause diagnostics should include a message");
    assert!(
        paused_message.contains(pending_action_id.as_str()),
        "expected gate-pause diagnostics to include deterministic pending action id, got {paused_message}"
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 2);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 1);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        1
    );
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let replayed_pause = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger,
    )
    .expect("replayed requirements trigger should keep gate pause idempotent");

    assert_eq!(
        replayed_pause.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        replayed_pause.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    let replayed_pause_message = replayed_pause
        .automatic_dispatch
        .message
        .as_deref()
        .expect("replayed gate pause diagnostics should include message");
    assert!(
        replayed_pause_message.contains(pending_action_id.as_str()),
        "expected replayed gate-pause diagnostics to keep deterministic action id, got {replayed_pause_message}"
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 2);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 1);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        1
    );
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let missing_approval_error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("resume must fail while gate-linked approval is still unresolved");
    assert_eq!(
        missing_approval_error.code,
        "operator_resume_requires_approved_action"
    );

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Roadmap gate approved by operator.".into()),
        },
    )
    .expect("resolve pending roadmap gate approval");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            user_answer: None,
        },
    )
    .expect("resume should continue requirements -> roadmap once gate approval is provided");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after gate pause resume");
    assert_eq!(events.len(), 3);

    let resumed_transition = events
        .iter()
        .find(|event| event.from_node_id == "requirements" && event.to_node_id == "roadmap")
        .expect("resumed transition should persist requirements -> roadmap event");
    assert!(
        resumed_transition.transition_id.starts_with("resume:"),
        "expected deterministic resume transition id, got {}",
        resumed_transition.transition_id
    );
    assert_eq!(
        resumed_transition.causal_transition_id.as_deref(),
        Some(research_to_requirements.transition_id.as_str())
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 1);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        0
    );
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let resumed_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto { project_id },
    )
    .expect("load project snapshot after gate pause resume");
    assert!(
        resumed_snapshot.approval_requests.iter().all(|approval| {
            !(approval.gate_key.as_deref() == Some("roadmap_gate")
                && approval.status == OperatorApprovalStatus::Pending)
        }),
        "roadmap gate pause should clear after explicit resolve + resume"
    );
}

pub(crate) fn plan_mode_required_false_keeps_implementation_continuation_auto_dispatch_behavior() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_plan_mode_continuation_workflow(&repo_root, &project_id, None, None);
    seed_unreachable_runtime_run_with_identity_and_plan_mode(
        &repo_root,
        &project_id,
        "run-plan-mode-false",
        "openai_codex",
        "openai_codex",
        "openai_codex",
        None,
        false,
    );

    let applied = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: project_id.clone(),
            transition_id: "requirements-roadmap-plan-mode-false-1".into(),
            causal_transition_id: None,
            from_node_id: "requirements".into(),
            to_node_id: "roadmap".into(),
            transition_kind: "advance".into(),
            gate_decision: "not_applicable".into(),
            gate_decision_context: None,
            gate_updates: Vec::new(),
            occurred_at: "2026-04-24T07:40:00Z".into(),
        },
    )
    .expect("planModeRequired=false should keep automatic continuation behavior");

    assert_eq!(
        applied.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Applied
    );
    let continuation = applied
        .automatic_dispatch
        .transition_event
        .as_ref()
        .expect("automatic continuation transition should persist when plan mode is disabled");
    assert_eq!(continuation.from_node_id, "roadmap");
    assert_eq!(continuation.to_node_id, "implementation");
    assert!(continuation.transition_id.starts_with("auto:"));
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 2);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load snapshot after planModeRequired=false continuation");
    assert!(snapshot.approval_requests.iter().all(|approval| {
        !(approval.gate_key.as_deref() == Some("plan_mode_required")
            && approval.status == OperatorApprovalStatus::Pending)
    }));
}

pub(crate) fn plan_mode_required_true_pauses_and_requires_explicit_resolve_resume() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_plan_mode_continuation_workflow(&repo_root, &project_id, None, None);
    seed_unreachable_runtime_run_with_identity_and_plan_mode(
        &repo_root,
        &project_id,
        "run-plan-mode-true",
        "openai_codex",
        "openai_codex",
        "openai_codex",
        None,
        true,
    );

    let request = ApplyWorkflowTransitionRequestDto {
        project_id: project_id.clone(),
        transition_id: "requirements-roadmap-plan-mode-true-1".into(),
        causal_transition_id: None,
        from_node_id: "requirements".into(),
        to_node_id: "roadmap".into(),
        transition_kind: "advance".into(),
        gate_decision: "not_applicable".into(),
        gate_decision_context: None,
        gate_updates: Vec::new(),
        occurred_at: "2026-04-24T07:41:00Z".into(),
    };

    let paused = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        request.clone(),
    )
    .expect("planModeRequired=true should pause implementation continuation");

    assert_eq!(
        paused.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        paused.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    let paused_message = paused
        .automatic_dispatch
        .message
        .as_deref()
        .expect("plan-mode pause should include diagnostics");
    assert!(paused_message.contains("Persisted pending operator approval"));
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 1);

    let pause_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load snapshot after plan-mode pause");

    let pending_plan_mode = pause_snapshot
        .approval_requests
        .iter()
        .find(|approval| {
            approval.gate_key.as_deref() == Some("plan_mode_required")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .expect("plan-mode pause should persist deterministic pending approval");
    assert_eq!(pending_plan_mode.transition_from_node_id.as_deref(), Some("roadmap"));
    assert_eq!(
        pending_plan_mode.transition_to_node_id.as_deref(),
        Some("implementation")
    );
    assert_eq!(pending_plan_mode.transition_kind.as_deref(), Some("advance"));

    let pending_action_id = pending_plan_mode.action_id.clone();
    assert!(paused_message.contains(pending_action_id.as_str()));
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let replayed_pause = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        request,
    )
    .expect("replaying trigger should keep plan-mode pause idempotent");
    assert_eq!(
        replayed_pause.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    let replayed_message = replayed_pause
        .automatic_dispatch
        .message
        .as_deref()
        .expect("replayed pause diagnostics should include message");
    assert!(replayed_message.contains(pending_action_id.as_str()));
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 1);
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let missing_approval_error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("resume should remain blocked until pending plan-mode approval is resolved");
    assert_eq!(
        missing_approval_error.code,
        "operator_resume_requires_approved_action"
    );

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Planning outputs are accepted for implementation.".into()),
        },
    )
    .expect("resolve plan-mode pending approval");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            user_answer: None,
        },
    )
    .expect("resolved plan-mode approval should unblock continuation through resume");
    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after plan-mode resume");
    assert_eq!(events.len(), 2);
    let resumed_transition = events
        .iter()
        .find(|event| event.from_node_id == "roadmap" && event.to_node_id == "implementation")
        .expect("resume should persist roadmap -> implementation transition");
    assert!(resumed_transition.transition_id.starts_with("resume:"));
    assert_eq!(
        resumed_transition.causal_transition_id.as_deref(),
        Some("requirements-roadmap-plan-mode-true-1")
    );

    let resumed_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto { project_id },
    )
    .expect("load snapshot after plan-mode resume");
    assert!(resumed_snapshot.approval_requests.iter().all(|approval| {
        !(approval.gate_key.as_deref() == Some("plan_mode_required")
            && approval.status == OperatorApprovalStatus::Pending)
    }));
}

pub(crate) fn plan_mode_required_missing_required_gate_metadata_keeps_dispatch_blocked() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_plan_mode_continuation_workflow(
        &repo_root,
        &project_id,
        Some("implementation_gate"),
        None,
    );
    seed_unreachable_runtime_run_with_identity_and_plan_mode(
        &repo_root,
        &project_id,
        "run-plan-mode-malformed",
        "openai_codex",
        "openai_codex",
        "openai_codex",
        None,
        true,
    );

    let blocked = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: project_id.clone(),
            transition_id: "requirements-roadmap-plan-mode-malformed-1".into(),
            causal_transition_id: None,
            from_node_id: "requirements".into(),
            to_node_id: "roadmap".into(),
            transition_kind: "advance".into(),
            gate_decision: "not_applicable".into(),
            gate_decision_context: None,
            gate_updates: Vec::new(),
            occurred_at: "2026-04-24T07:42:00Z".into(),
        },
    )
    .expect("missing implementation gate metadata should block continuation fail-closed");

    assert_eq!(
        blocked.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        blocked.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    assert!(
        blocked
            .automatic_dispatch
            .message
            .as_deref()
            .is_some_and(|message| message.contains("required gate linkage could not be resolved")),
        "expected malformed gate-linkage diagnostics, got {:?}",
        blocked.automatic_dispatch.message
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 1);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load snapshot after malformed plan-mode gate mapping");
    assert!(snapshot.approval_requests.iter().all(|approval| {
        !(approval.transition_from_node_id.as_deref() == Some("roadmap")
            && approval.transition_to_node_id.as_deref() == Some("implementation")
            && approval.status == OperatorApprovalStatus::Pending)
    }));
}

pub(crate) fn start_autonomous_run_mints_fresh_child_unit_and_attempt_identity_per_stage() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    seed_planning_lifecycle_workflow(&repo_root, &project_id, false);

    let started = start_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartAutonomousRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start autonomous run for lifecycle progression");
    let started_run = started.run.expect("autonomous run should be returned");

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == started_run.run_id
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

        run.run_id == started_run.run_id
            && autonomous_state.history.len() == 3
            && unit.sequence == 3
            && attempt.attempt_number == 3
            && unit.kind == cadence_desktop_lib::commands::AutonomousUnitKindDto::Planner
            && linkage.workflow_node_id == "roadmap"
            && attempt.workflow_linkage.as_ref() == Some(linkage)
    });

    let progressed_run = progressed
        .run
        .as_ref()
        .expect("progressed run should exist");
    let progressed_unit = progressed
        .unit
        .as_ref()
        .expect("progressed unit should exist");
    let progressed_attempt = progressed
        .attempt
        .as_ref()
        .expect("progressed attempt should exist");
    let progressed_linkage = progressed_unit
        .workflow_linkage
        .as_ref()
        .expect("progressed unit should expose workflow linkage");

    assert_eq!(progressed_unit.run_id, started_run.run_id);
    assert_eq!(progressed_attempt.run_id, started_run.run_id);
    assert_eq!(
        progressed_run.active_unit_id.as_deref(),
        Some(progressed_unit.unit_id.as_str())
    );
    assert_eq!(
        progressed_run.active_attempt_id.as_deref(),
        Some(progressed_attempt.attempt_id.as_str())
    );
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &started_run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &started_run.run_id),
        3
    );
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 3);

    let history_sequences = progressed
        .history
        .iter()
        .map(|entry| entry.unit.sequence)
        .collect::<Vec<_>>();
    assert_eq!(history_sequences, vec![3, 2, 1]);

    let history_attempt_numbers = progressed
        .history
        .iter()
        .map(|entry| {
            entry
                .latest_attempt
                .as_ref()
                .expect("each seeded stage should persist a latest attempt")
                .attempt_number
        })
        .collect::<Vec<_>>();
    assert_eq!(history_attempt_numbers, vec![3, 2, 1]);

    let history_stage_ids = progressed
        .history
        .iter()
        .map(|entry| {
            entry
                .unit
                .workflow_linkage
                .as_ref()
                .expect("each seeded stage should persist workflow linkage")
                .workflow_node_id
                .clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        history_stage_ids,
        vec!["roadmap", "requirements", "research"]
    );

    let history_unit_statuses = progressed
        .history
        .iter()
        .map(|entry| entry.unit.status.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        history_unit_statuses,
        vec![
            AutonomousUnitStatusDto::Active,
            AutonomousUnitStatusDto::Completed,
            AutonomousUnitStatusDto::Completed,
        ]
    );

    let history_attempt_statuses = progressed
        .history
        .iter()
        .map(|entry| {
            entry
                .latest_attempt
                .as_ref()
                .expect("each seeded stage should persist a latest attempt")
                .status
                .clone()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        history_attempt_statuses,
        vec![
            AutonomousUnitStatusDto::Active,
            AutonomousUnitStatusDto::Completed,
            AutonomousUnitStatusDto::Completed,
        ]
    );

    assert!(progressed.history[0].unit.finished_at.is_none());
    assert!(progressed.history[1].unit.finished_at.is_some());
    assert!(progressed.history[2].unit.finished_at.is_some());

    let unit_ids = progressed
        .history
        .iter()
        .map(|entry| entry.unit.unit_id.clone())
        .collect::<Vec<_>>();
    let mut unique_unit_ids = unit_ids.clone();
    unique_unit_ids.sort();
    unique_unit_ids.dedup();
    assert_eq!(unique_unit_ids.len(), unit_ids.len());

    let attempt_ids = progressed
        .history
        .iter()
        .map(|entry| {
            entry
                .latest_attempt
                .as_ref()
                .expect("each seeded stage should persist a latest attempt")
                .attempt_id
                .clone()
        })
        .collect::<Vec<_>>();
    let mut unique_attempt_ids = attempt_ids.clone();
    unique_attempt_ids.sort();
    unique_attempt_ids.dedup();
    assert_eq!(unique_attempt_ids.len(), attempt_ids.len());

    let child_session_ids = progressed
        .history
        .iter()
        .map(|entry| {
            entry
                .latest_attempt
                .as_ref()
                .expect("each seeded stage should persist a latest attempt")
                .child_session_id
                .clone()
        })
        .collect::<Vec<_>>();
    let mut unique_child_session_ids = child_session_ids.clone();
    unique_child_session_ids.sort();
    unique_child_session_ids.dedup();
    assert_eq!(unique_child_session_ids.len(), child_session_ids.len());

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load lifecycle transition events after autonomous progression");
    let roadmap_transition = events
        .iter()
        .find(|event| event.to_node_id == "roadmap")
        .expect("roadmap transition should exist after autonomous progression");
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

    let progressed_identity = progressed
        .history
        .iter()
        .map(|entry| {
            let attempt = entry
                .latest_attempt
                .as_ref()
                .expect("each seeded stage should persist a latest attempt");
            let linkage = entry
                .unit
                .workflow_linkage
                .as_ref()
                .expect("each seeded stage should persist workflow linkage");
            (
                entry.unit.sequence,
                entry.unit.unit_id.clone(),
                attempt.attempt_number,
                attempt.attempt_id.clone(),
                attempt.child_session_id.clone(),
                linkage.workflow_node_id.clone(),
                linkage.transition_id.clone(),
                linkage.handoff_package_hash.clone(),
                entry.unit.status.clone(),
                attempt.status.clone(),
            )
        })
        .collect::<Vec<_>>();

    let replayed = get_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("replayed autonomous refresh should succeed");
    let replayed_run = replayed
        .run
        .as_ref()
        .expect("replayed autonomous run should exist");
    let replayed_linkage = replayed
        .unit
        .as_ref()
        .and_then(|unit| unit.workflow_linkage.as_ref())
        .expect("replayed autonomous state should keep workflow linkage");
    let replayed_identity = replayed
        .history
        .iter()
        .map(|entry| {
            let attempt = entry
                .latest_attempt
                .as_ref()
                .expect("each replayed stage should keep a latest attempt");
            let linkage = entry
                .unit
                .workflow_linkage
                .as_ref()
                .expect("each replayed stage should keep workflow linkage");
            (
                entry.unit.sequence,
                entry.unit.unit_id.clone(),
                attempt.attempt_number,
                attempt.attempt_id.clone(),
                attempt.child_session_id.clone(),
                linkage.workflow_node_id.clone(),
                linkage.transition_id.clone(),
                linkage.handoff_package_hash.clone(),
                entry.unit.status.clone(),
                attempt.status.clone(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        replayed_run.active_unit_id.as_deref(),
        progressed_run.active_unit_id.as_deref()
    );
    assert_eq!(
        replayed_run.active_attempt_id.as_deref(),
        progressed_run.active_attempt_id.as_deref()
    );
    assert_eq!(replayed_linkage, progressed_linkage);
    assert_eq!(replayed_identity, progressed_identity);
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &started_run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &started_run.run_id),
        3
    );
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 3);

    let cancelled = cancel_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CancelAutonomousRunRequestDto {
            project_id,
            run_id: started_run.run_id,
        },
    )
    .expect("cancel autonomous lifecycle progression run")
    .run
    .expect("cancelled autonomous run should exist");
    assert_eq!(cancelled.status, AutonomousRunStatusDto::Cancelled);
}

pub(crate) fn get_autonomous_run_fails_closed_when_workflow_graph_has_no_active_node() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id, false);

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open workflow db");
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'pending' WHERE project_id = ?1 AND node_id = 'discussion'",
            [&project_id],
        )
        .expect("clear the only active workflow node");

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-no-active-node",
        "session-1",
        Some("flow-1"),
        &runtime_shell::script_sleep(5),
    );

    let error = get_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect_err("missing active workflow node should fail closed");
    assert_eq!(error.code, "autonomous_workflow_active_node_missing");
    assert_eq!(count_autonomous_run_rows(&repo_root), 0);
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 0);

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop scripted runtime run after missing-active-node failure")
    .expect("stopped runtime run should exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn get_autonomous_run_fails_closed_for_invalid_workflow_node_to_unit_mapping() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    project_store::upsert_workflow_graph(
        &repo_root,
        &project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![project_store::WorkflowGraphNodeRecord {
                node_id: "discovery".into(),
                phase_id: 1,
                sort_order: 1,
                name: "Discovery".into(),
                description: "Unknown autonomous stage mapping.".into(),
                status: PhaseStatus::Active,
                current_step: None,
                task_count: 1,
                completed_tasks: 0,
                summary: None,
            }],
            edges: vec![],
            gates: vec![],
        },
    )
    .expect("seed invalid workflow mapping graph");

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-invalid-unit-mapping",
        "session-1",
        Some("flow-1"),
        &runtime_shell::script_sleep(5),
    );

    let error = get_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect_err("invalid workflow mapping should fail closed");
    assert_eq!(error.code, "autonomous_workflow_unit_mapping_invalid");
    assert_eq!(count_autonomous_run_rows(&repo_root), 0);
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 0);

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop scripted runtime run after invalid-mapping failure")
    .expect("stopped runtime run should exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

pub(crate) fn get_autonomous_run_rejects_stale_workflow_linkage_after_active_stage_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    seed_planning_lifecycle_workflow(&repo_root, &project_id, false);

    let started = start_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartAutonomousRunRequestDto {
            project_id: project_id.clone(),
            initial_controls: None,
            initial_prompt: None,
        },
    )
    .expect("start autonomous run before stage drift");
    let started_run = started
        .run
        .expect("autonomous run should exist before drift");

    wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        autonomous_state
            .unit
            .as_ref()
            .and_then(|unit| unit.workflow_linkage.as_ref())
            .is_some_and(|linkage| linkage.workflow_node_id == "roadmap")
    });

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open workflow db");
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'pending' WHERE project_id = ?1 AND node_id = 'roadmap'",
            [&project_id],
        )
        .expect("demote roadmap stage");
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'active' WHERE project_id = ?1 AND node_id = 'requirements'",
            [&project_id],
        )
        .expect("promote requirements stage to simulate drift");

    let error = get_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect_err("stale workflow linkage should fail closed when active stage drifts");
    assert_eq!(error.code, "autonomous_workflow_linkage_stage_conflict");
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 3);

    let cancelled = cancel_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CancelAutonomousRunRequestDto {
            project_id,
            run_id: started_run.run_id,
        },
    )
    .expect("cancel autonomous run after linkage drift failure")
    .run
    .expect("cancelled autonomous run after linkage drift should exist");
    assert_eq!(cancelled.status, AutonomousRunStatusDto::Cancelled);
}
