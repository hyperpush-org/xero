use super::support::*;

pub(crate) fn gate_linked_resume_applies_transition_and_records_causal_event() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-1", "repo-gate");
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:00:00Z",
    )
    .expect("persist gate-linked approval");

    assert_eq!(pending.gate_node_id.as_deref(), Some("execute"));
    assert_eq!(pending.gate_key.as_deref(), Some("execution_gate"));
    assert_eq!(pending.transition_from_node_id.as_deref(), Some("plan"));
    assert_eq!(pending.transition_to_node_id.as_deref(), Some("execute"));
    assert_eq!(pending.transition_kind.as_deref(), Some("advance"));
    assert!(pending.action_id.contains(":gate:execute:execution_gate:"));

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect("resume should apply gate-linked transition");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after resume transition");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Complete
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].from_node_id, "plan");
    assert_eq!(events[0].to_node_id, "execute");
    assert_eq!(events[0].transition_kind, "advance");
    assert_eq!(
        events[0].gate_decision,
        project_store::WorkflowTransitionGateDecision::Approved
    );
    assert_eq!(
        events[0].gate_decision_context.as_deref(),
        Some("Execution gate approved by operator.")
    );
}

pub(crate) fn gate_linked_resume_auto_dispatches_next_legal_edge_and_replays_idempotently() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-auto-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-auto-1",
        "repo-gate-auto",
    );
    seed_gate_linked_workflow_with_auto_continuation(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:00:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    let first_resume = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect("first resume should apply transition and auto-dispatch continuation");
    assert_eq!(
        first_resume.resume_entry.status,
        ResumeHistoryStatus::Started
    );

    let graph_after_first = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after first resume");
    assert_eq!(
        graph_after_first
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Complete
    );
    assert_eq!(
        graph_after_first
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Complete
    );
    assert_eq!(
        graph_after_first
            .nodes
            .iter()
            .find(|node| node.node_id == "verify")
            .expect("verify node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );

    let first_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after first resume");
    assert_eq!(first_events.len(), 2);

    let primary_event = first_events
        .iter()
        .find(|event| event.from_node_id == "plan" && event.to_node_id == "execute")
        .expect("primary gate-linked transition event");
    let auto_event = first_events
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
    assert_eq!(
        auto_event.gate_decision,
        project_store::WorkflowTransitionGateDecision::NotApplicable
    );

    let persisted_auto_package = project_store::load_workflow_handoff_package(
        &repo_root,
        project_id,
        &auto_event.transition_id,
    )
    .expect("load persisted handoff package for auto transition")
    .expect("handoff package row should exist for auto transition");
    let persisted_payload: serde_json::Value =
        serde_json::from_str(&persisted_auto_package.package_payload)
            .expect("decode persisted auto handoff payload");
    assert_eq!(
        persisted_payload["triggerTransition"]["transitionId"],
        auto_event.transition_id
    );
    assert_eq!(
        persisted_payload["triggerTransition"]["causalTransitionId"],
        primary_event.transition_id
    );

    let first_events_reloaded =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("reload transition events after first resume");
    assert_eq!(first_events, first_events_reloaded);

    let replay_resume = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("replayed resume should be idempotent for transition persistence");
    assert_eq!(
        replay_resume.resume_entry.status,
        ResumeHistoryStatus::Started
    );

    let replay_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after replayed resume");
    assert_eq!(replay_events.len(), 2);
    assert_eq!(first_events, replay_events);

    let replayed_auto_package = project_store::load_workflow_handoff_package(
        &repo_root,
        project_id,
        &auto_event.transition_id,
    )
    .expect("load replayed handoff package for auto transition")
    .expect("handoff package row should remain persisted across replay");
    assert_eq!(
        persisted_auto_package.package_hash,
        replayed_auto_package.package_hash
    );
}

pub(crate) fn command_and_gate_linked_resume_persist_equivalent_transition_shapes() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);

    let command_project_id = "project-gate-parity-command";
    let command_repo_root = seed_project(
        &root,
        &app,
        command_project_id,
        "repo-gate-parity-command",
        "repo-gate-parity-command",
    );
    seed_gate_linked_workflow(&command_repo_root, command_project_id, "approve_execution");

    let command_transition = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: command_project_id.into(),
            transition_id: "transition-parity-command-1".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: "approved".into(),
            gate_decision_context: Some("approved by operator".into()),
            gate_updates: vec![WorkflowTransitionGateUpdateRequestDto {
                gate_key: "execution_gate".into(),
                gate_state: "satisfied".into(),
                decision_context: Some("approved by operator".into()),
            }],
            occurred_at: "2026-04-15T19:50:00Z".into(),
        },
    )
    .expect("command transition should persist");

    assert_eq!(
        command_transition.transition_event.gate_decision,
        cadence_desktop_lib::commands::WorkflowTransitionGateDecisionDto::Approved
    );

    let resume_project_id = "project-gate-parity-resume";
    let resume_repo_root = seed_project(
        &root,
        &app,
        resume_project_id,
        "repo-gate-parity-resume",
        "repo-gate-parity-resume",
    );
    seed_gate_linked_workflow(&resume_repo_root, resume_project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &resume_repo_root,
        resume_project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:50:00Z",
    )
    .expect("persist gate-linked approval for parity test");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: resume_project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved by operator".into()),
        },
    )
    .expect("approve gate-linked action for parity test");

    resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: resume_project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("resume transition should persist");

    let resume_events = project_store::load_recent_workflow_transition_events(
        &resume_repo_root,
        resume_project_id,
        None,
    )
    .expect("load resume transition events");
    assert_eq!(resume_events.len(), 1);
    let resume_event = &resume_events[0];

    assert_eq!(
        command_transition.transition_event.from_node_id,
        resume_event.from_node_id
    );
    assert_eq!(
        command_transition.transition_event.to_node_id,
        resume_event.to_node_id
    );
    assert_eq!(
        command_transition.transition_event.transition_kind,
        resume_event.transition_kind
    );
    assert_eq!(
        command_transition.transition_event.gate_decision_context,
        resume_event.gate_decision_context
    );
    assert_eq!(
        resume_event.gate_decision,
        project_store::WorkflowTransitionGateDecision::Approved
    );
}

pub(crate) fn gate_linked_resume_rejects_illegal_edge_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-illegal-edge";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-illegal-edge",
        "repo-gate-illegal-edge",
    );
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T20:00:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            r#"
            DELETE FROM workflow_graph_edges
            WHERE project_id = ?1
              AND from_node_id = 'plan'
              AND to_node_id = 'execute'
              AND transition_kind = 'advance'
              AND gate_requirement = 'execution_gate'
            "#,
            [project_id],
        )
        .expect("remove legal edge to force illegal-edge failure");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail when gate-linked edge is missing");
    assert_eq!(error.code, "workflow_transition_illegal_edge");

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after illegal-edge failure");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Pending
    );

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed illegal-edge resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after illegal-edge resume failure");
    assert!(events.is_empty());
}

pub(crate) fn gate_linked_resume_rejects_unresolved_target_gates_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-gate-unmet";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-gate-unmet",
        "repo-gate-gate-unmet",
    );
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            r#"
            INSERT INTO workflow_gate_metadata (
                project_id,
                node_id,
                gate_key,
                gate_state,
                action_type,
                title,
                detail,
                decision_context,
                updated_at
            )
            VALUES (?1, 'execute', 'safety_gate', 'pending', 'approve_safety', 'Approve safety', 'Safety review required.', NULL, '2026-04-15T20:10:00Z')
            "#,
            [project_id],
        )
        .expect("insert additional unresolved gate");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T20:10:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail while other target gates remain unresolved");
    assert_eq!(error.code, "workflow_transition_gate_unmet");

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load workflow graph after gate-unmet failure");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Active
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        cadence_desktop_lib::commands::PhaseStatus::Pending
    );

    let execute_gate = graph_after
        .gates
        .iter()
        .find(|gate| gate.gate_key == "execution_gate")
        .expect("execution gate metadata");
    assert_eq!(
        execute_gate.gate_state,
        project_store::WorkflowGateState::Pending
    );

    let safety_gate = graph_after
        .gates
        .iter()
        .find(|gate| gate.gate_key == "safety_gate")
        .expect("safety gate metadata");
    assert_eq!(
        safety_gate.gate_state,
        project_store::WorkflowGateState::Pending
    );

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed gate-unmet resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after gate-unmet resume failure");
    assert!(events.is_empty());
}

pub(crate) fn gate_linked_resume_rejects_secret_user_answer_input_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-secret-input";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-resume-secret-input",
        "repo-gate-secret-input",
    );
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T20:20:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: Some("oauth access_token=sk-live-secret".into()),
        },
    )
    .expect_err("secret-bearing userAnswer should be rejected at resume boundary");
    assert_eq!(error.code, "operator_resume_decision_payload_invalid");

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed secret-input resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after failed secret-input resume");
    assert!(events.is_empty());
}

pub(crate) fn gate_linked_resume_rejects_missing_transition_context_without_side_effects() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-2";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-2", "repo-gate");
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:10:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked action");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable constraints for corruption test");
    connection
        .execute(
            "UPDATE operator_approvals SET transition_kind = NULL WHERE project_id = ?1 AND action_id = ?2",
            params![project_id, pending.action_id.as_str()],
        )
        .expect("corrupt continuation metadata");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail closed when continuation metadata is corrupted");
    assert_eq!(error.code, "operator_approval_decode_failed");

    let connection = open_state_connection(&repo_root);
    let resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resume rows after failed resume");
    assert_eq!(resume_count, 0);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after failed resume");
    assert!(events.is_empty());
}

pub(crate) fn gate_linked_approval_requires_non_secret_user_answer() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-resume-3";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-3", "repo-gate");
    seed_gate_linked_workflow(&repo_root, project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-15T19:20:00Z",
    )
    .expect("persist gate-linked approval");

    let missing_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: None,
        },
    )
    .expect_err("gate-linked approvals should require a recorded answer");
    assert_eq!(missing_answer.code, "operator_action_answer_required");

    let secret_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id,
            decision: "approve".into(),
            user_answer: Some("oauth access_token=sk-live-secret".into()),
        },
    )
    .expect_err("secret-bearing answer payload should fail closed");
    assert_eq!(
        secret_answer.code,
        "operator_action_decision_payload_invalid"
    );
}

pub(crate) fn gate_linked_upsert_rejects_ambiguous_gate_context() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-ambiguous-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-gate-ambiguous-1",
        "repo-gate",
    );

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "a".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "A".into(),
                    description: "A".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "b".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "B".into(),
                    description: "B".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "a".into(),
                    to_node_id: "b".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("shared_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "b".into(),
                    to_node_id: "a".into(),
                    transition_kind: "rollback".into(),
                    gate_requirement: Some("shared_gate".into()),
                },
            ],
            gates: vec![
                project_store::WorkflowGateMetadataRecord {
                    node_id: "a".into(),
                    gate_key: "shared_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_shared".into()),
                    title: Some("Approve shared gate".into()),
                    detail: Some("Multiple unresolved gates share this action.".into()),
                    decision_context: None,
                },
                project_store::WorkflowGateMetadataRecord {
                    node_id: "b".into(),
                    gate_key: "shared_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_shared".into()),
                    title: Some("Approve shared gate".into()),
                    detail: Some("Multiple unresolved gates share this action.".into()),
                    decision_context: None,
                },
            ],
        },
    )
    .expect("seed ambiguous workflow graph");

    let error = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_shared",
        "Approve shared gate",
        "Multiple unresolved gates share this action.",
        "2026-04-15T19:30:00Z",
    )
    .expect_err("ambiguous unresolved gates should fail closed");

    assert_eq!(error.code, "operator_approval_gate_ambiguous");
}

pub(crate) fn repeated_action_type_uses_gate_scoped_action_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-gate-repeat-1";
    let repo_root = seed_project(&root, &app, project_id, "repo-gate-repeat-1", "repo-gate");

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "plan".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Plan".into(),
                    description: "Plan".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Complete,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Active,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Verify),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "plan".into(),
                    to_node_id: "execute".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("execute_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("verify_gate".into()),
                },
            ],
            gates: vec![
                project_store::WorkflowGateMetadataRecord {
                    node_id: "execute".into(),
                    gate_key: "execute_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_stage".into()),
                    title: Some("Approve execute stage".into()),
                    detail: Some("Approve execute gate".into()),
                    decision_context: None,
                },
                project_store::WorkflowGateMetadataRecord {
                    node_id: "verify".into(),
                    gate_key: "verify_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_stage".into()),
                    title: Some("Approve verify stage".into()),
                    detail: Some("Approve verify gate".into()),
                    decision_context: None,
                },
            ],
        },
    )
    .expect("seed repeated-action graph");

    let first = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_stage",
        "Approve execute stage",
        "Approve execute gate",
        "2026-04-15T19:40:00Z",
    )
    .expect("persist first gate-scoped approval");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'complete' WHERE project_id = ?1 AND node_id = 'execute'",
            [project_id],
        )
        .expect("complete execute node");
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'active' WHERE project_id = ?1 AND node_id = 'verify'",
            [project_id],
        )
        .expect("activate verify node");
    connection
        .execute(
            "UPDATE workflow_gate_metadata SET gate_state = 'satisfied' WHERE project_id = ?1 AND node_id = 'execute' AND gate_key = 'execute_gate'",
            [project_id],
        )
        .expect("satisfy execute gate");

    let second = project_store::upsert_pending_operator_approval(
        &repo_root,
        project_id,
        "session-1",
        Some("flow-1"),
        "approve_stage",
        "Approve verify stage",
        "Approve verify gate",
        "2026-04-15T19:41:00Z",
    )
    .expect("persist second gate-scoped approval");

    assert_ne!(first.action_id, second.action_id);
    assert_eq!(first.gate_key.as_deref(), Some("execute_gate"));
    assert_eq!(second.gate_key.as_deref(), Some("verify_gate"));
}

pub(crate) fn plan_mode_required_resume_unblocks_implementation_continuation_without_duplicate_rows(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-plan-mode-gate-linked-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-plan-mode-gate-linked-1",
        "repo-plan-mode-gate-linked",
    );

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "requirements".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Requirements".into(),
                    description: "Lock requirement deltas.".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Active,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "roadmap".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Roadmap".into(),
                    description: "Plan downstream slices.".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "implementation".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Implementation".into(),
                    description: "Implement approved changes.".into(),
                    status: cadence_desktop_lib::commands::PhaseStatus::Pending,
                    current_step: Some(cadence_desktop_lib::commands::PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "requirements".into(),
                    to_node_id: "roadmap".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "roadmap".into(),
                    to_node_id: "implementation".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
            ],
            gates: vec![],
        },
    )
    .expect("seed plan-mode continuation workflow for operator loop persistence test");

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-plan-mode-gate-linked-1".into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Unknown,
                },
                started_at: "2026-04-24T07:50:00Z".into(),
                last_heartbeat_at: Some("2026-04-24T07:50:01Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-24T07:50:01Z".into(),
            },
            checkpoint: Some(project_store::RuntimeRunCheckpointRecord {
                project_id: project_id.into(),
                run_id: "run-plan-mode-gate-linked-1".into(),
                sequence: 1,
                kind: project_store::RuntimeRunCheckpointKind::Bootstrap,
                summary: "Seeded runtime run.".into(),
                created_at: "2026-04-24T07:50:01Z".into(),
            }),
            control_state: Some(
                project_store::build_runtime_run_control_state_with_plan_mode(
                    "openai_codex",
                    None,
                    cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
                    true,
                    "2026-04-24T07:50:00Z",
                    None,
                )
                .expect("build seeded plan-mode runtime controls"),
            ),
        },
    )
    .expect("seed plan-mode runtime run");

    let paused = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: project_id.into(),
            transition_id: "requirements-roadmap-plan-mode-gate-linked-1".into(),
            causal_transition_id: None,
            from_node_id: "requirements".into(),
            to_node_id: "roadmap".into(),
            transition_kind: "advance".into(),
            gate_decision: "not_applicable".into(),
            gate_decision_context: None,
            gate_updates: vec![],
            occurred_at: "2026-04-24T07:50:10Z".into(),
        },
    )
    .expect("plan mode should pause implementation continuation");
    assert_eq!(
        paused.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("load project snapshot after plan-mode gate pause");

    let pending = snapshot
        .approval_requests
        .iter()
        .find(|approval| {
            approval.gate_key.as_deref() == Some("plan_mode_required")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .expect("plan mode should persist pending continuation approval")
        .clone();

    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, project_id, &pending.action_id),
        1
    );

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Proceed with implementation.".into()),
        },
    )
    .expect("resolve pending plan-mode approval");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: pending.action_id.clone(),
            user_answer: None,
        },
    )
    .expect("resume should unblock plan-mode implementation continuation");
    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transitions after plan-mode resume");
    assert_eq!(events.len(), 2);
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, project_id, &pending.action_id),
        1
    );
}

fn seed_runtime_scoped_resume_fixture(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    repo_root: &Path,
    runtime_run_id: &str,
    autonomous_run_id: &str,
    attempt_boundary_id: Option<&str>,
) -> String {
    project_store::upsert_runtime_run(
        repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: runtime_run_id.into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-25T00:00:00Z".into(),
                last_heartbeat_at: Some("2026-04-25T00:00:05Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-25T00:00:05Z".into(),
            },
            checkpoint: None,
            control_state: Some(
                project_store::build_runtime_run_control_state(
                    "openai_codex",
                    None,
                    cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
                    "2026-04-25T00:00:05Z",
                    None,
                )
                .expect("build runtime control state for runtime-scoped fixture"),
            ),
        },
    )
    .expect("persist runtime run for runtime-scoped resume fixture");

    let persisted = project_store::upsert_runtime_action_required(
        repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            run_id: runtime_run_id.into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-runtime-fixture-1".into(),
            flow_id: Some("flow-runtime-fixture-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-25T00:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-25T00:00:05Z".into()),
            last_error: None,
            boundary_id: "boundary-runtime-fixture-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input and requires operator approval."
                .into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-25T00:00:06Z".into(),
        },
    )
    .expect("persist runtime-scoped operator action");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.into(),
            action_id: persisted.approval_request.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve runtime-scoped operator action");

    let unit_id = format!("{autonomous_run_id}:unit:1");
    let attempt_id = format!("{unit_id}:attempt:1");
    let timestamp = "2026-04-25T00:00:07Z";

    project_store::upsert_autonomous_run(
        repo_root,
        &project_store::AutonomousRunUpsertRecord {
            run: project_store::AutonomousRunRecord {
                project_id: project_id.into(),
                run_id: autonomous_run_id.into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::AutonomousRunStatus::Running,
                active_unit_sequence: Some(1),
                duplicate_start_detected: false,
                duplicate_start_run_id: None,
                duplicate_start_reason: None,
                started_at: timestamp.into(),
                last_heartbeat_at: Some(timestamp.into()),
                last_checkpoint_at: Some(timestamp.into()),
                paused_at: None,
                cancelled_at: None,
                completed_at: None,
                crashed_at: None,
                stopped_at: None,
                pause_reason: None,
                cancel_reason: None,
                crash_reason: None,
                last_error: None,
                updated_at: timestamp.into(),
            },
            unit: Some(project_store::AutonomousUnitRecord {
                project_id: project_id.into(),
                run_id: autonomous_run_id.into(),
                unit_id: unit_id.clone(),
                sequence: 1,
                kind: project_store::AutonomousUnitKind::Researcher,
                status: project_store::AutonomousUnitStatus::Blocked,
                summary: "Autonomous attempt blocked on operator boundary.".into(),
                boundary_id: attempt_boundary_id.map(str::to_string),
                workflow_linkage: None,
                started_at: timestamp.into(),
                finished_at: None,
                updated_at: timestamp.into(),
                last_error: None,
            }),
            attempt: Some(project_store::AutonomousUnitAttemptRecord {
                project_id: project_id.into(),
                run_id: autonomous_run_id.into(),
                unit_id,
                attempt_id,
                attempt_number: 1,
                child_session_id: "child-session-runtime-fixture-1".into(),
                status: project_store::AutonomousUnitStatus::Blocked,
                boundary_id: attempt_boundary_id.map(str::to_string),
                workflow_linkage: None,
                started_at: timestamp.into(),
                finished_at: None,
                updated_at: timestamp.into(),
                last_error: None,
            }),
            artifacts: Vec::new(),
        },
    )
    .expect("seed autonomous run for runtime-scoped resume fixture");

    persisted.approval_request.action_id
}

pub(crate) fn runtime_scoped_resume_rejects_run_identity_mismatch_without_resumed_evidence_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-gate-run-mismatch-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-gate-run-mismatch-1",
        "repo-runtime-gate-run-mismatch",
    );

    let action_id = seed_runtime_scoped_resume_fixture(
        &app,
        project_id,
        &repo_root,
        "run-runtime-gate-1",
        "run-runtime-gate-1",
        Some("boundary-runtime-fixture-1"),
    );

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .expect("disable foreign keys for runtime/autonomous run-id mismatch fixture");
    connection
        .execute(
            "UPDATE autonomous_runs SET run_id = 'run-autonomous-gate-2' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt autonomous run id for mismatch fixture");
    connection
        .execute(
            "UPDATE autonomous_units SET run_id = 'run-autonomous-gate-2' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt autonomous unit run ids for mismatch fixture");
    connection
        .execute(
            "UPDATE autonomous_unit_attempts SET run_id = 'run-autonomous-gate-2' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt autonomous attempt run ids for mismatch fixture");
    connection
        .execute(
            "UPDATE autonomous_unit_artifacts SET run_id = 'run-autonomous-gate-2' WHERE project_id = ?1",
            [project_id],
        )
        .expect("corrupt autonomous artifact run ids for mismatch fixture");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("re-enable foreign keys after runtime/autonomous run-id mismatch fixture");

    let before = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("load autonomous snapshot before runtime run mismatch resume")
        .expect("autonomous snapshot should exist before runtime run mismatch resume");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("runtime-scoped resume should fail closed on run identity mismatch");
    assert_eq!(error.code, "autonomous_resume_run_mismatch");

    let after = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("load autonomous snapshot after runtime run mismatch resume")
        .expect("autonomous snapshot should remain after runtime run mismatch resume");
    assert_eq!(after.run.run_id, before.run.run_id);
    assert_eq!(after.run.status, before.run.status);
    assert_eq!(after.attempt, before.attempt);

    let connection = open_state_connection(&repo_root);
    let resumed_artifact_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM autonomous_unit_artifacts WHERE project_id = ?1 AND artifact_id LIKE '%:resumed'",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resumed evidence artifacts after run mismatch");
    assert_eq!(resumed_artifact_count, 0);

    let failed_resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1 AND source_action_id = ?2 AND status = 'failed'",
            params![project_id, action_id.as_str()],
            |row| row.get(0),
        )
        .expect("count failed resume history rows after run mismatch");
    assert_eq!(failed_resume_count, 1);
}

pub(crate) fn runtime_scoped_resume_rejects_missing_boundary_identity_without_resumed_evidence_drift(
) {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path) = create_state(&root);
    let app = build_mock_app(state);
    let project_id = "project-runtime-gate-boundary-missing-1";
    let repo_root = seed_project(
        &root,
        &app,
        project_id,
        "repo-runtime-gate-boundary-missing-1",
        "repo-runtime-gate-boundary-missing",
    );

    let action_id = seed_runtime_scoped_resume_fixture(
        &app,
        project_id,
        &repo_root,
        "run-runtime-gate-boundary-1",
        "run-runtime-gate-boundary-1",
        None,
    );

    let before = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("load autonomous snapshot before missing-boundary resume")
        .expect("autonomous snapshot should exist before missing-boundary resume");

    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.into(),
            action_id: action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err(
        "runtime-scoped resume should fail closed when attempt boundary linkage is missing",
    );
    assert_eq!(error.code, "autonomous_resume_identity_invalid");

    let after = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("load autonomous snapshot after missing-boundary resume")
        .expect("autonomous snapshot should remain after missing-boundary resume");
    assert_eq!(after.run.run_id, before.run.run_id);
    assert_eq!(after.run.status, before.run.status);
    assert_eq!(
        after
            .attempt
            .as_ref()
            .and_then(|attempt| attempt.boundary_id.as_deref()),
        None
    );

    let connection = open_state_connection(&repo_root);
    let resumed_artifact_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM autonomous_unit_artifacts WHERE project_id = ?1 AND artifact_id LIKE '%:resumed'",
            [project_id],
            |row| row.get(0),
        )
        .expect("count resumed evidence artifacts after missing-boundary resume");
    assert_eq!(resumed_artifact_count, 0);

    let failed_resume_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM operator_resume_history WHERE project_id = ?1 AND source_action_id = ?2 AND status = 'failed'",
            params![project_id, action_id.as_str()],
            |row| row.get(0),
        )
        .expect("count failed resume history rows after missing-boundary resume");
    assert_eq!(failed_resume_count, 1);
}
