use super::support::*;
pub(crate) fn workflow_handoff_package_persists_with_transition_linkage_and_replays_idempotently() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-1";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-1",
        "repo-graph-handoff",
    );

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
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 2,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 3,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            }],
            gates: vec![],
        },
    )
    .expect("seed handoff graph");

    let transition = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-handoff-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::NotApplicable,
            gate_decision_context: None,
            gate_updates: vec![],
            occurred_at: "2026-04-16T16:00:00Z".into(),
        },
    )
    .expect("persist transition for handoff package");

    let upsert_payload = project_store::WorkflowHandoffPackageUpsertRecord {
        project_id: project_id.into(),
        handoff_transition_id: transition.transition_event.transition_id.clone(),
        causal_transition_id: transition.transition_event.causal_transition_id.clone(),
        from_node_id: transition.transition_event.from_node_id.clone(),
        to_node_id: transition.transition_event.to_node_id.clone(),
        transition_kind: transition.transition_event.transition_kind.clone(),
        package_payload:
            r#"{"context":{"next_node":"execute","stage":"execute"},"summary":"ready"}"#.into(),
        created_at: "2026-04-16T16:00:01Z".into(),
    };

    let first = project_store::upsert_workflow_handoff_package(&repo_root, &upsert_payload)
        .expect("persist handoff package");
    let replayed = project_store::upsert_workflow_handoff_package(&repo_root, &upsert_payload)
        .expect("replay handoff package persist");

    assert_eq!(first, replayed);
    assert_eq!(first.handoff_transition_id, "txn-handoff-001");
    assert_eq!(first.package_hash.len(), 64);

    let loaded =
        project_store::load_workflow_handoff_package(&repo_root, project_id, "txn-handoff-001")
            .expect("load handoff package by transition id")
            .expect("handoff package row should exist");
    assert_eq!(loaded, first);

    let recent = project_store::load_recent_workflow_handoff_packages(&repo_root, project_id, None)
        .expect("load recent handoff packages");
    assert_eq!(recent, vec![first.clone()]);

    let connection = open_state_connection(&repo_root);
    let row_count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_handoff_packages WHERE project_id = ?1 AND handoff_transition_id = ?2",
            [project_id, "txn-handoff-001"],
            |row| row.get(0),
        )
        .expect("count handoff package rows");
    assert_eq!(row_count, 1);
}

pub(crate) fn workflow_handoff_package_rejects_missing_transition_and_malformed_payload() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-2";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-2",
        "repo-graph-handoff",
    );

    let missing_transition = project_store::upsert_workflow_handoff_package(
        &repo_root,
        &project_store::WorkflowHandoffPackageUpsertRecord {
            project_id: project_id.into(),
            handoff_transition_id: "missing-transition".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            package_payload: r#"{"summary":"ready"}"#.into(),
            created_at: "2026-04-16T16:10:00Z".into(),
        },
    )
    .expect_err("missing transition should fail handoff persistence");
    assert_eq!(
        missing_transition.code,
        "workflow_handoff_transition_missing"
    );

    let malformed_payload = project_store::upsert_workflow_handoff_package(
        &repo_root,
        &project_store::WorkflowHandoffPackageUpsertRecord {
            project_id: project_id.into(),
            handoff_transition_id: "missing-transition".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            package_payload: "{not-json}".into(),
            created_at: "2026-04-16T16:10:00Z".into(),
        },
    )
    .expect_err("malformed JSON payload should be rejected before persistence");
    assert_eq!(malformed_payload.code, "workflow_handoff_request_invalid");
}

pub(crate) fn workflow_handoff_package_decode_fails_closed_on_corrupted_hash_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-3";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-3",
        "repo-graph-handoff",
    );

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
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            }],
            gates: vec![],
        },
    )
    .expect("seed handoff corruption graph");

    project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-handoff-corrupt-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::NotApplicable,
            gate_decision_context: None,
            gate_updates: vec![],
            occurred_at: "2026-04-16T16:20:00Z".into(),
        },
    )
    .expect("persist transition for corruption test");

    project_store::upsert_workflow_handoff_package(
        &repo_root,
        &project_store::WorkflowHandoffPackageUpsertRecord {
            project_id: project_id.into(),
            handoff_transition_id: "txn-handoff-corrupt-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            package_payload: r#"{"summary":"ready"}"#.into(),
            created_at: "2026-04-16T16:20:01Z".into(),
        },
    )
    .expect("persist valid handoff package before corruption");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable handoff-package check constraints for corruption test");
    connection
        .execute(
            "UPDATE workflow_handoff_packages SET package_hash = 'abc' WHERE project_id = ?1 AND handoff_transition_id = 'txn-handoff-corrupt-001'",
            [project_id],
        )
        .expect("corrupt handoff package hash");

    let error = project_store::load_recent_workflow_handoff_packages(&repo_root, project_id, None)
        .expect_err("corrupted handoff package hash should fail closed");
    assert_eq!(error.code, "workflow_handoff_decode_failed");
    assert!(error.message.contains("package_hash"));
}

pub(crate) fn workflow_handoff_package_assembly_is_deterministic_for_replay_inputs() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-assembly-deterministic";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-assembly-deterministic",
        "repo-graph-handoff-assembly-deterministic",
    );

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
                    description: "Discuss scope".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Discuss),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "research".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Research".into(),
                    description: "Research context".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "discussion".into(),
                to_node_id: "research".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            }],
            gates: vec![],
        },
    )
    .expect("seed deterministic handoff graph");

    project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-handoff-assembly-001".into(),
            causal_transition_id: Some("txn-seed-001".into()),
            from_node_id: "discussion".into(),
            to_node_id: "research".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("approved by planner".into()),
            gate_updates: vec![],
            occurred_at: "2026-04-16T17:05:00Z".into(),
        },
    )
    .expect("persist deterministic transition");

    let assembled_first = project_store::assemble_workflow_handoff_package(
        &repo_root,
        project_id,
        "txn-handoff-assembly-001",
    )
    .expect("assemble deterministic package first pass");
    let assembled_second = project_store::assemble_workflow_handoff_package(
        &repo_root,
        project_id,
        "txn-handoff-assembly-001",
    )
    .expect("assemble deterministic package second pass");

    assert_eq!(assembled_first, assembled_second);

    let parsed_payload: serde_json::Value =
        serde_json::from_str(&assembled_first.package_payload).expect("decode assembled payload");
    assert_eq!(parsed_payload["schemaVersion"], 1);
    assert_eq!(
        parsed_payload["triggerTransition"]["transitionId"],
        "txn-handoff-assembly-001"
    );

    let persisted = project_store::upsert_workflow_handoff_package(&repo_root, &assembled_first)
        .expect("persist assembled package");
    let replayed = project_store::upsert_workflow_handoff_package(&repo_root, &assembled_second)
        .expect("replay assembled package");

    assert_eq!(persisted.package_payload, replayed.package_payload);
    assert_eq!(persisted.package_hash, replayed.package_hash);
}

pub(crate) fn workflow_handoff_package_assembly_fails_closed_on_secret_bearing_destination_content() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-assembly-redaction";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-assembly-redaction",
        "repo-graph-handoff-assembly-redaction",
    );

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
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            }],
            gates: vec![],
        },
    )
    .expect("seed redaction graph");

    project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-handoff-redaction-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::NotApplicable,
            gate_decision_context: None,
            gate_updates: vec![],
            occurred_at: "2026-04-16T17:10:00Z".into(),
        },
    )
    .expect("persist transition before redaction validation");

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET description = 'oauth access_token=sk-live-secret' WHERE project_id = ?1 AND node_id = 'execute'",
            [project_id],
        )
        .expect("inject secret-bearing destination description");

    let error = project_store::assemble_and_persist_workflow_handoff_package(
        &repo_root,
        project_id,
        "txn-handoff-redaction-001",
    )
    .expect_err("secret-bearing destination metadata should fail assembly");
    assert_eq!(error.code, "workflow_handoff_redaction_failed");

    let packages =
        project_store::load_recent_workflow_handoff_packages(&repo_root, project_id, None)
            .expect("load handoff packages after redaction failure");
    assert!(
        packages.is_empty(),
        "redaction failure should persist no package rows"
    );
}

pub(crate) fn workflow_handoff_package_assembly_rejects_missing_target_node_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-assembly-missing-target";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-assembly-missing-target",
        "repo-graph-handoff-assembly-missing-target",
    );

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
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            }],
            gates: vec![],
        },
    )
    .expect("seed missing-target graph");

    project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-handoff-missing-target-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::NotApplicable,
            gate_decision_context: None,
            gate_updates: vec![],
            occurred_at: "2026-04-16T17:15:00Z".into(),
        },
    )
    .expect("persist transition before missing-target corruption");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA foreign_keys = OFF;")
        .expect("disable foreign keys for missing-target corruption");
    connection
        .execute(
            "DELETE FROM workflow_graph_nodes WHERE project_id = ?1 AND node_id = 'execute'",
            [project_id],
        )
        .expect("delete destination node metadata");

    let error = project_store::assemble_workflow_handoff_package(
        &repo_root,
        project_id,
        "txn-handoff-missing-target-001",
    )
    .expect_err("missing destination node should fail assembly");
    assert_eq!(error.code, "workflow_handoff_build_target_missing");
}

pub(crate) fn workflow_handoff_package_assembly_rejects_malformed_gate_state_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-assembly-malformed-gates";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-assembly-malformed-gates",
        "repo-graph-handoff-assembly-malformed-gates",
    );

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
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: Some("execute_gate".into()),
            }],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "execute".into(),
                gate_key: "execute_gate".into(),
                gate_state: project_store::WorkflowGateState::Satisfied,
                action_type: Some("approve_execution".into()),
                title: Some("Approve execution".into()),
                detail: Some("Operator approval required".into()),
                decision_context: Some("approved".into()),
            }],
        },
    )
    .expect("seed malformed gate graph");

    project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-handoff-malformed-gates-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("approved by operator".into()),
            gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
                gate_key: "execute_gate".into(),
                gate_state: project_store::WorkflowGateState::Satisfied,
                decision_context: Some("approved".into()),
            }],
            occurred_at: "2026-04-16T17:20:00Z".into(),
        },
    )
    .expect("persist transition before malformed gate corruption");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints for malformed gate corruption");
    connection
        .execute(
            "UPDATE workflow_gate_metadata SET gate_state = 'invent' WHERE project_id = ?1 AND node_id = 'execute' AND gate_key = 'execute_gate'",
            [project_id],
        )
        .expect("corrupt gate_state enum");

    let error = project_store::assemble_workflow_handoff_package(
        &repo_root,
        project_id,
        "txn-handoff-malformed-gates-001",
    )
    .expect_err("malformed gate rows should fail assembly");
    assert_eq!(error.code, "workflow_handoff_build_gate_state_invalid");
}

pub(crate) fn workflow_handoff_package_assembly_rejects_invalid_lifecycle_projection_shape() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-assembly-lifecycle-invalid";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-assembly-lifecycle-invalid",
        "repo-graph-handoff-assembly-lifecycle-invalid",
    );

    project_store::upsert_workflow_graph(
        &repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "discussion".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Discussion A".into(),
                    description: "Discussion stage".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Discuss),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "plan-discussion".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Discussion B".into(),
                    description: "Duplicate lifecycle stage".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Execute".into(),
                    description: "Execute stage".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "discussion".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            }],
            gates: vec![],
        },
    )
    .expect("seed invalid lifecycle graph");

    project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-handoff-lifecycle-invalid-001".into(),
            causal_transition_id: None,
            from_node_id: "discussion".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::NotApplicable,
            gate_decision_context: None,
            gate_updates: vec![],
            occurred_at: "2026-04-16T17:25:00Z".into(),
        },
    )
    .expect("persist transition for invalid lifecycle projection test");

    let error = project_store::assemble_workflow_handoff_package(
        &repo_root,
        project_id,
        "txn-handoff-lifecycle-invalid-001",
    )
    .expect_err("duplicate lifecycle stages should fail assembly");
    assert_eq!(error.code, "workflow_handoff_build_lifecycle_invalid");
}

pub(crate) fn workflow_handoff_package_assembly_rejects_auto_transition_without_causal_linkage() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-handoff-assembly-causal-missing";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-handoff-assembly-causal-missing",
        "repo-graph-handoff-assembly-causal-missing",
    );

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
                    description: "Plan stage".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute stage".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![project_store::WorkflowGraphEdgeRecord {
                from_node_id: "plan".into(),
                to_node_id: "execute".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            }],
            gates: vec![],
        },
    )
    .expect("seed causal-missing graph");

    project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "auto:missing-causal:001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::NotApplicable,
            gate_decision_context: None,
            gate_updates: vec![],
            occurred_at: "2026-04-16T17:30:00Z".into(),
        },
    )
    .expect("persist synthetic auto transition without causal linkage");

    let error = project_store::assemble_workflow_handoff_package(
        &repo_root,
        project_id,
        "auto:missing-causal:001",
    )
    .expect_err("auto transitions without causal linkage should fail assembly");
    assert_eq!(error.code, "workflow_handoff_build_causal_missing");
}
