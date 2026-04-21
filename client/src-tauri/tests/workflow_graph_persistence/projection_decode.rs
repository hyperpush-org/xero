use super::support::*;
pub(crate) fn workflow_graph_upsert_projects_phase_projection_in_stable_order() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-1";
    let repo_root = seed_project(&root, project_id, "repo-graph-1", "repo-graph");

    let graph = project_store::WorkflowGraphUpsertRecord {
        nodes: vec![
            project_store::WorkflowGraphNodeRecord {
                node_id: "plan".into(),
                phase_id: 1,
                sort_order: 1,
                name: "Plan workflow".into(),
                description: "Capture durable workflow graph state.".into(),
                status: PhaseStatus::Complete,
                current_step: Some(PhaseStep::Ship),
                task_count: 3,
                completed_tasks: 3,
                summary: Some("Plan complete".into()),
            },
            project_store::WorkflowGraphNodeRecord {
                node_id: "execute".into(),
                phase_id: 2,
                sort_order: 2,
                name: "Execute workflow".into(),
                description: "Run durable transition path.".into(),
                status: PhaseStatus::Active,
                current_step: Some(PhaseStep::Execute),
                task_count: 4,
                completed_tasks: 1,
                summary: None,
            },
            project_store::WorkflowGraphNodeRecord {
                node_id: "verify".into(),
                phase_id: 3,
                sort_order: 3,
                name: "Verify workflow".into(),
                description: "Check gate state before ship.".into(),
                status: PhaseStatus::Blocked,
                current_step: Some(PhaseStep::Verify),
                task_count: 2,
                completed_tasks: 0,
                summary: Some("Awaiting operator approval".into()),
            },
        ],
        edges: vec![
            project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            },
            project_store::WorkflowGraphEdgeRecord {
                from_node_id: "execute".into(),
                to_node_id: "verify".into(),
                transition_kind: "advance".into(),
                gate_requirement: Some("verification_gate".into()),
            },
        ],
        gates: vec![project_store::WorkflowGateMetadataRecord {
            node_id: "verify".into(),
            gate_key: "verification_gate".into(),
            gate_state: project_store::WorkflowGateState::Pending,
            action_type: Some("confirm_verification".into()),
            title: Some("Verification required".into()),
            detail: Some("Review verification artifacts before ship.".into()),
            decision_context: None,
        }],
    };

    let persisted =
        project_store::upsert_workflow_graph(&repo_root, project_id, &graph).expect("upsert graph");
    assert_eq!(persisted.nodes.len(), 3);
    assert_eq!(persisted.edges.len(), 2);
    assert_eq!(persisted.gates.len(), 1);

    let snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load snapshot")
        .snapshot;

    assert_eq!(snapshot.phases.len(), 3);
    assert_eq!(snapshot.project.total_phases, 3);
    assert_eq!(snapshot.project.completed_phases, 1);
    assert_eq!(snapshot.project.active_phase, 2);
    assert_eq!(snapshot.phases[0].id, 1);
    assert_eq!(snapshot.phases[0].status, PhaseStatus::Complete);
    assert_eq!(snapshot.phases[1].id, 2);
    assert_eq!(snapshot.phases[1].status, PhaseStatus::Active);
    assert_eq!(snapshot.phases[2].id, 3);
    assert_eq!(snapshot.phases[2].status, PhaseStatus::Blocked);

    let reopened =
        project_store::load_workflow_graph(&repo_root, project_id).expect("reload graph");
    assert_eq!(persisted, reopened);
}

pub(crate) fn workflow_snapshot_projects_planning_lifecycle_with_gate_and_transition_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-lifecycle-1";
    let repo_root = seed_project(&root, project_id, "repo-graph-lifecycle-1", "repo-graph");

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "discussion".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Discussion".into(),
                    description: "Clarify goals".into(),
                    status: PhaseStatus::Complete,
                    current_step: Some(PhaseStep::Ship),
                    task_count: 3,
                    completed_tasks: 3,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "research".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Research".into(),
                    description: "Collect implementation constraints".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 4,
                    completed_tasks: 2,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "requirements".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Requirements".into(),
                    description: "Lock requirement deltas".into(),
                    status: PhaseStatus::Blocked,
                    current_step: Some(PhaseStep::Verify),
                    task_count: 2,
                    completed_tasks: 1,
                    summary: Some("Awaiting gate".into()),
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "roadmap".into(),
                    phase_id: 4,
                    sort_order: 4,
                    name: "Roadmap".into(),
                    description: "Plan upcoming slices".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 5,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "discussion".into(),
                    to_node_id: "research".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "research".into(),
                    to_node_id: "requirements".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "requirements".into(),
                    to_node_id: "roadmap".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("roadmap_gate".into()),
                },
            ],
            gates: vec![
                project_store::WorkflowGateMetadataRecord {
                    node_id: "requirements".into(),
                    gate_key: "requirements_gate".into(),
                    gate_state: project_store::WorkflowGateState::Blocked,
                    action_type: Some("resolve_requirements".into()),
                    title: Some("Requirements blocked".into()),
                    detail: Some("Need sign-off before roadmap planning.".into()),
                    decision_context: None,
                },
                project_store::WorkflowGateMetadataRecord {
                    node_id: "roadmap".into(),
                    gate_key: "roadmap_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some("approve_roadmap".into()),
                    title: Some("Roadmap approval required".into()),
                    detail: Some("Review roadmap draft before scheduling.".into()),
                    decision_context: None,
                },
            ],
        },
    )
    .expect("seed lifecycle graph");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            r#"
            INSERT INTO workflow_transition_events (
                project_id,
                transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                gate_decision,
                gate_decision_context,
                created_at
            )
            VALUES (?1, ?2, NULL, ?3, ?4, 'advance', 'approved', NULL, ?5)
            "#,
            [
                project_id,
                "lifecycle-evt-1",
                "discussion",
                "research",
                "2026-04-15T10:00:00Z",
            ],
        )
        .expect("insert lifecycle transition event for research");
    connection
        .execute(
            r#"
            INSERT INTO workflow_transition_events (
                project_id,
                transition_id,
                causal_transition_id,
                from_node_id,
                to_node_id,
                transition_kind,
                gate_decision,
                gate_decision_context,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, 'advance', 'approved', 'operator-approved', ?6)
            "#,
            [
                project_id,
                "lifecycle-evt-2",
                "lifecycle-evt-1",
                "requirements",
                "roadmap",
                "2026-04-15T12:00:00Z",
            ],
        )
        .expect("insert lifecycle transition event for roadmap");

    let snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load snapshot with lifecycle")
        .snapshot;

    assert_eq!(snapshot.lifecycle.stages.len(), 4);

    let discussion = &snapshot.lifecycle.stages[0];
    assert_eq!(discussion.stage, PlanningLifecycleStageKindDto::Discussion);
    assert_eq!(discussion.node_id, "discussion");
    assert_eq!(discussion.status, PhaseStatus::Complete);
    assert!(!discussion.action_required);
    assert_eq!(
        discussion.last_transition_at.as_deref(),
        Some("2026-04-15T10:00:00Z")
    );

    let research = &snapshot.lifecycle.stages[1];
    assert_eq!(research.stage, PlanningLifecycleStageKindDto::Research);
    assert_eq!(research.node_id, "research");
    assert_eq!(research.status, PhaseStatus::Active);
    assert!(!research.action_required);
    assert_eq!(
        research.last_transition_at.as_deref(),
        Some("2026-04-15T10:00:00Z")
    );

    let requirements = &snapshot.lifecycle.stages[2];
    assert_eq!(
        requirements.stage,
        PlanningLifecycleStageKindDto::Requirements
    );
    assert_eq!(requirements.node_id, "requirements");
    assert_eq!(requirements.status, PhaseStatus::Blocked);
    assert!(requirements.action_required);
    assert_eq!(
        requirements.last_transition_at.as_deref(),
        Some("2026-04-15T12:00:00Z")
    );

    let roadmap = &snapshot.lifecycle.stages[3];
    assert_eq!(roadmap.stage, PlanningLifecycleStageKindDto::Roadmap);
    assert_eq!(roadmap.node_id, "roadmap");
    assert_eq!(roadmap.status, PhaseStatus::Pending);
    assert!(roadmap.action_required);
    assert_eq!(
        roadmap.last_transition_at.as_deref(),
        Some("2026-04-15T12:00:00Z")
    );
}

pub(crate) fn lifecycle_alias_collisions_fail_closed_during_projection_decode() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-lifecycle-2";
    let repo_root = seed_project(&root, project_id, "repo-graph-lifecycle-2", "repo-graph");

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "discussion".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Discussion".into(),
                    description: "Discussion phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Discuss),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "discuss".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Discuss alias".into(),
                    description: "Conflicting alias".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![],
            gates: vec![],
        },
    )
    .expect("seed graph with colliding aliases");

    let error = project_store::load_project_snapshot(&repo_root, project_id)
        .expect_err("alias collisions should fail closed");
    assert_eq!(error.code, "workflow_graph_decode_failed");
    assert!(error
        .message
        .contains("Planning lifecycle stage `discussion` matched multiple workflow nodes"));
}

pub(crate) fn malformed_lifecycle_gate_rows_fail_closed_during_projection_decode() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-lifecycle-3";
    let repo_root = seed_project(&root, project_id, "repo-graph-lifecycle-3", "repo-graph");

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![project_store::WorkflowGraphNodeRecord {
                node_id: "discussion".into(),
                phase_id: 1,
                sort_order: 1,
                name: "Discussion".into(),
                description: "Discussion phase".into(),
                status: PhaseStatus::Active,
                current_step: Some(PhaseStep::Discuss),
                task_count: 1,
                completed_tasks: 0,
                summary: None,
            }],
            edges: vec![],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "discussion".into(),
                gate_key: "discussion_gate".into(),
                gate_state: project_store::WorkflowGateState::Pending,
                action_type: Some("approve_discussion".into()),
                title: Some("Approve discussion".into()),
                detail: Some("Operator review required".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed graph with gate row");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints for corruption test");
    connection
        .execute(
            "UPDATE workflow_gate_metadata SET gate_state = 'mystery' WHERE project_id = ?1 AND gate_key = 'discussion_gate'",
            [project_id],
        )
        .expect("corrupt gate state");

    let error = project_store::load_project_snapshot(&repo_root, project_id)
        .expect_err("malformed gate rows should fail closed");
    assert_eq!(error.code, "workflow_graph_decode_failed");
    assert!(error.message.contains("Field `gate_state`"));
}

pub(crate) fn malformed_graph_rows_fail_closed_during_projection_decode() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-3";
    let repo_root = seed_project(&root, project_id, "repo-graph-3", "repo-graph");

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![project_store::WorkflowGraphNodeRecord {
                node_id: "plan".into(),
                phase_id: 1,
                sort_order: 1,
                name: "Plan".into(),
                description: "Plan phase".into(),
                status: PhaseStatus::Active,
                current_step: Some(PhaseStep::Plan),
                task_count: 1,
                completed_tasks: 0,
                summary: None,
            }],
            edges: vec![],
            gates: vec![],
        },
    )
    .expect("seed graph");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints for corruption test");
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET status = 'mystery' WHERE project_id = ?1 AND node_id = 'plan'",
            [project_id],
        )
        .expect("corrupt graph node status");

    let error = project_store::load_project_snapshot(&repo_root, project_id)
        .expect_err("malformed graph row should fail closed");
    assert_eq!(error.code, "workflow_graph_decode_failed");
}

pub(crate) fn malformed_graph_current_step_fails_closed_during_projection_decode() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-5";
    let repo_root = seed_project(&root, project_id, "repo-graph-5", "repo-graph");

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![project_store::WorkflowGraphNodeRecord {
                node_id: "discussion".into(),
                phase_id: 1,
                sort_order: 1,
                name: "Discussion".into(),
                description: "Discussion phase".into(),
                status: PhaseStatus::Active,
                current_step: Some(PhaseStep::Discuss),
                task_count: 1,
                completed_tasks: 0,
                summary: None,
            }],
            edges: vec![],
            gates: vec![],
        },
    )
    .expect("seed graph");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints for corruption test");
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET current_step = 'invent' WHERE project_id = ?1 AND node_id = 'discussion'",
            [project_id],
        )
        .expect("corrupt graph node current_step");

    let error = project_store::load_project_snapshot(&repo_root, project_id)
        .expect_err("malformed current_step should fail closed");
    assert_eq!(error.code, "workflow_graph_decode_failed");
    assert!(error
        .message
        .contains("Unknown phase current_step `invent`."));
}
