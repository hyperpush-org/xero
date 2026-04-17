use std::{
    fs,
    path::{Path, PathBuf},
};

use cadence_desktop_lib::{
    commands::{PhaseStatus, PhaseStep, PlanningLifecycleStageKindDto},
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    state::ImportFailpoints,
};
use rusqlite::Connection;
use tempfile::TempDir;

fn seed_project(root: &TempDir, project_id: &str, repository_id: &str, repo_name: &str) -> PathBuf {
    let repo_root = root.path().join(repo_name);
    fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");

    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: repository_id.into(),
        root_path: canonical_root.clone(),
        root_path_string: canonical_root.to_string_lossy().into_owned(),
        common_git_dir: canonical_root.join(".git"),
        display_name: repo_name.into(),
        branch_name: Some("main".into()),
        head_sha: Some("abc123".into()),
        branch: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

    db::import_project(&repository, &ImportFailpoints::default()).expect("import project");
    canonical_root
}

fn open_state_connection(repo_root: &Path) -> Connection {
    Connection::open(database_path_for_repo(repo_root)).expect("open state db")
}

fn seed_auto_dispatch_workflow_graph(repo_root: &Path, project_id: &str) {
    project_store::upsert_workflow_graph(
        repo_root,
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
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
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
                    gate_requirement: Some("execution_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
            ],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "execute".into(),
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Pending,
                action_type: Some("approve_execution".into()),
                title: Some("Approve execution".into()),
                detail: Some("Operator approval required.".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed graph with automatic continuation");
}

#[test]
fn workflow_graph_upsert_projects_phase_projection_in_stable_order() {
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

#[test]
fn workflow_transition_is_transactional_and_event_is_durable() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-2";
    let repo_root = seed_project(&root, project_id, "repo-graph-2", "repo-graph");

    let graph = project_store::WorkflowGraphUpsertRecord {
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
                task_count: 4,
                completed_tasks: 0,
                summary: None,
            },
        ],
        edges: vec![project_store::WorkflowGraphEdgeRecord {
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_requirement: Some("execution_gate".into()),
        }],
        gates: vec![project_store::WorkflowGateMetadataRecord {
            node_id: "execute".into(),
            gate_key: "execution_gate".into(),
            gate_state: project_store::WorkflowGateState::Pending,
            action_type: Some("approve_execution".into()),
            title: Some("Approve execution".into()),
            detail: Some("Operator approval required.".into()),
            decision_context: None,
        }],
    };

    project_store::upsert_workflow_graph(&repo_root, project_id, &graph).expect("seed graph");

    let blocked = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Blocked,
            gate_decision_context: Some("approval missing".into()),
            gate_updates: vec![],
            occurred_at: "2026-04-15T18:00:00Z".into(),
        },
    )
    .expect_err("transition should fail when gate remains pending");
    assert_eq!(blocked.code, "workflow_transition_gate_unmet");

    let after_block = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("graph after blocked transition");
    assert_eq!(after_block.nodes[0].status, PhaseStatus::Active);
    assert_eq!(after_block.nodes[1].status, PhaseStatus::Pending);

    let blocked_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after blocked transition");
    assert!(
        blocked_events.is_empty(),
        "blocked transition should not emit durable event"
    );

    let applied = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-002".into(),
            causal_transition_id: Some("txn-001".into()),
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("operator-approved".into()),
            gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Satisfied,
                decision_context: Some("approved by operator".into()),
            }],
            occurred_at: "2026-04-15T18:01:00Z".into(),
        },
    )
    .expect("transition should succeed once gate is satisfied");

    assert_eq!(applied.transition_event.transition_id, "txn-002");
    assert_eq!(
        applied.transition_event.causal_transition_id.as_deref(),
        Some("txn-001")
    );
    assert_eq!(applied.transition_event.from_node_id, "plan");
    assert_eq!(applied.transition_event.to_node_id, "execute");

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].transition_id, "txn-002");
    assert_eq!(events[0].causal_transition_id.as_deref(), Some("txn-001"));
    assert_eq!(
        events[0].gate_decision,
        project_store::WorkflowTransitionGateDecision::Approved
    );
    assert_eq!(
        events[0].gate_decision_context.as_deref(),
        Some("operator-approved")
    );

    let reopened_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("reload transition events after reopen");
    assert_eq!(events, reopened_events);

    let graph_after_apply =
        project_store::load_workflow_graph(&repo_root, project_id).expect("graph after apply");
    assert_eq!(graph_after_apply.nodes[0].status, PhaseStatus::Complete);
    assert_eq!(graph_after_apply.nodes[1].status, PhaseStatus::Active);
}

#[test]
fn workflow_transition_auto_dispatches_single_legal_next_edge() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-1";
    let repo_root = seed_project(&root, project_id, "repo-graph-auto-1", "repo-graph-auto");

    let graph = project_store::WorkflowGraphUpsertRecord {
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
            project_store::WorkflowGraphNodeRecord {
                node_id: "verify".into(),
                phase_id: 3,
                sort_order: 3,
                name: "Verify".into(),
                description: "Verify phase".into(),
                status: PhaseStatus::Pending,
                current_step: Some(PhaseStep::Verify),
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
                gate_requirement: Some("execution_gate".into()),
            },
            project_store::WorkflowGraphEdgeRecord {
                from_node_id: "execute".into(),
                to_node_id: "verify".into(),
                transition_kind: "advance".into(),
                gate_requirement: None,
            },
        ],
        gates: vec![project_store::WorkflowGateMetadataRecord {
            node_id: "execute".into(),
            gate_key: "execution_gate".into(),
            gate_state: project_store::WorkflowGateState::Pending,
            action_type: Some("approve_execution".into()),
            title: Some("Approve execution".into()),
            detail: Some("Operator approval required.".into()),
            decision_context: None,
        }],
    };

    project_store::upsert_workflow_graph(&repo_root, project_id, &graph).expect("seed graph");

    let applied = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-auto-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("operator-approved".into()),
            gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Satisfied,
                decision_context: Some("approved by operator".into()),
            }],
            occurred_at: "2026-04-16T12:00:00Z".into(),
        },
    )
    .expect("transition should trigger automatic continuation");

    let (auto_event, auto_package) = match &applied.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            transition_event,
            handoff_package,
        } => {
            let package = match handoff_package {
                project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { package }
                | project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { package } => {
                    package
                }
                project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped {
                    code,
                    message,
                } => {
                    panic!(
                        "expected persisted handoff package outcome, got skipped code={code} message={message}"
                    )
                }
            };
            (transition_event, package)
        }
        other => panic!("expected applied automatic dispatch outcome, got {other:?}"),
    };

    assert_eq!(auto_event.from_node_id, "execute");
    assert_eq!(auto_event.to_node_id, "verify");
    assert_eq!(
        auto_event.causal_transition_id.as_deref(),
        Some(applied.transition_event.transition_id.as_str())
    );
    assert_eq!(
        auto_event.gate_decision,
        project_store::WorkflowTransitionGateDecision::NotApplicable
    );
    assert_eq!(auto_package.handoff_transition_id, auto_event.transition_id);
    assert_eq!(
        auto_package.causal_transition_id.as_deref(),
        Some(applied.transition_event.transition_id.as_str())
    );

    let graph_after =
        project_store::load_workflow_graph(&repo_root, project_id).expect("load graph after auto");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        PhaseStatus::Complete
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        PhaseStatus::Complete
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "verify")
            .expect("verify node")
            .status,
        PhaseStatus::Active
    );

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after auto dispatch");
    assert_eq!(events.len(), 2);

    let persisted_primary = events
        .iter()
        .find(|event| event.from_node_id == "plan" && event.to_node_id == "execute")
        .expect("persisted primary transition event");
    let persisted_auto = events
        .iter()
        .find(|event| event.from_node_id == "execute" && event.to_node_id == "verify")
        .expect("persisted automatic transition event");

    assert_eq!(persisted_primary.transition_id, "txn-auto-001");
    assert!(persisted_auto.transition_id.starts_with("auto:"));
    assert_eq!(
        persisted_auto.causal_transition_id.as_deref(),
        Some("txn-auto-001")
    );

    let persisted_package = project_store::load_workflow_handoff_package(
        &repo_root,
        project_id,
        &persisted_auto.transition_id,
    )
    .expect("load persisted automatic handoff package")
    .expect("automatic handoff package should exist");
    assert_eq!(persisted_package.id, auto_package.id);
    assert_eq!(persisted_package.package_hash, auto_package.package_hash);

    let reloaded_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("reload transition events after auto dispatch");
    assert_eq!(events, reloaded_events);
}

#[test]
fn workflow_transition_auto_dispatch_fails_closed_on_ambiguous_next_edge() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-ambiguous-1";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-auto-ambiguous-1",
        "repo-graph-auto-ambiguous",
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
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "rollback".into(),
                    phase_id: 4,
                    sort_order: 4,
                    name: "Rollback".into(),
                    description: "Rollback phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
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
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "rollback".into(),
                    transition_kind: "rollback".into(),
                    gate_requirement: None,
                },
            ],
            gates: vec![],
        },
    )
    .expect("seed graph with ambiguous continuation");

    let applied = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-auto-ambiguous-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("complete plan".into()),
            gate_updates: vec![],
            occurred_at: "2026-04-16T12:10:00Z".into(),
        },
    )
    .expect("primary transition should succeed while auto-dispatch fails closed");

    match &applied.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Skipped { code, .. } => {
            assert_eq!(code, "workflow_transition_ambiguous_next_step")
        }
        other => panic!("expected skipped automatic dispatch outcome, got {other:?}"),
    }

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load graph after ambiguous auto-dispatch");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        PhaseStatus::Complete
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        PhaseStatus::Active
    );

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after ambiguous dispatch");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].from_node_id, "plan");
    assert_eq!(events[0].to_node_id, "execute");
}

#[test]
fn workflow_transition_auto_dispatch_skips_when_next_edge_has_unresolved_gates() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-gated-1";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-auto-gated-1",
        "repo-graph-auto-gated",
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
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
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
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("verify_gate".into()),
                },
            ],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "verify".into(),
                gate_key: "verify_gate".into(),
                gate_state: project_store::WorkflowGateState::Pending,
                action_type: Some("approve_verify".into()),
                title: Some("Approve verify".into()),
                detail: Some("Operator approval required.".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed graph with gated continuation");

    let applied = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-auto-gated-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("complete plan".into()),
            gate_updates: vec![],
            occurred_at: "2026-04-16T12:15:00Z".into(),
        },
    )
    .expect("primary transition should succeed while gated auto-dispatch fails closed");

    match &applied.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Skipped { code, message } => {
            assert_eq!(code, "workflow_transition_gate_unmet");
            assert!(
                message.contains("unresolved gates"),
                "expected unresolved gate message, got {message}"
            );
        }
        other => panic!("expected skipped automatic dispatch outcome, got {other:?}"),
    }

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load graph after gated auto-dispatch");
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "plan")
            .expect("plan node")
            .status,
        PhaseStatus::Complete
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "execute")
            .expect("execute node")
            .status,
        PhaseStatus::Active
    );
    assert_eq!(
        graph_after
            .nodes
            .iter()
            .find(|node| node.node_id == "verify")
            .expect("verify node")
            .status,
        PhaseStatus::Pending
    );

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after gated auto-dispatch");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].transition_id, "txn-auto-gated-001");
    assert_eq!(events[0].from_node_id, "plan");
    assert_eq!(events[0].to_node_id, "execute");

    let reloaded_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("reload transition events after gated auto-dispatch");
    assert_eq!(events, reloaded_events);
}

#[test]
fn workflow_transition_auto_dispatch_replays_idempotently() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-replay-1";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-auto-replay-1",
        "repo-graph-auto-replay",
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
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
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
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
            ],
            gates: vec![],
        },
    )
    .expect("seed replay graph");

    let request = project_store::ApplyWorkflowTransitionRecord {
        transition_id: "txn-auto-replay-001".into(),
        causal_transition_id: None,
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
        gate_decision_context: Some("complete plan".into()),
        gate_updates: vec![],
        occurred_at: "2026-04-16T12:20:00Z".into(),
    };

    let first = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("first apply should succeed");
    let second = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("replayed apply should be idempotent");

    assert_eq!(first.transition_event.transition_id, request.transition_id);
    assert_eq!(second.transition_event.transition_id, request.transition_id);

    let first_package = match &first.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            handoff_package, ..
        } => match handoff_package {
            project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { package }
            | project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { package } => {
                package
            }
            project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped { code, message } => {
                panic!(
                    "expected first apply handoff package success outcome, got skipped code={code} message={message}"
                )
            }
        },
        other => panic!("expected applied automatic dispatch outcome, got {other:?}"),
    };

    let replayed_package = match &second.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Replayed {
            transition_event,
            handoff_package,
        } => {
            assert_eq!(transition_event.from_node_id, "execute");
            assert_eq!(transition_event.to_node_id, "verify");
            assert_eq!(
                transition_event.causal_transition_id.as_deref(),
                Some(request.transition_id.as_str())
            );

            match handoff_package {
                project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { package }
                | project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { package } => {
                    package
                }
                project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped {
                    code,
                    message,
                } => {
                    panic!(
                        "expected replayed handoff package outcome, got skipped code={code} message={message}"
                    )
                }
            }
        }
        other => panic!("expected replayed automatic dispatch outcome, got {other:?}"),
    };
    assert_eq!(first_package.package_hash, replayed_package.package_hash);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load replay transition events");
    assert_eq!(events.len(), 2);

    let persisted_primary = events
        .iter()
        .find(|event| event.from_node_id == "plan" && event.to_node_id == "execute")
        .expect("persisted primary transition event");
    let persisted_auto = events
        .iter()
        .find(|event| event.from_node_id == "execute" && event.to_node_id == "verify")
        .expect("persisted automatic transition event");

    assert_eq!(persisted_primary.transition_id, "txn-auto-replay-001");
    assert!(persisted_auto.transition_id.starts_with("auto:"));
    assert_eq!(
        persisted_auto.causal_transition_id.as_deref(),
        Some("txn-auto-replay-001")
    );

    let replayed_package_row = project_store::load_workflow_handoff_package(
        &repo_root,
        project_id,
        &persisted_auto.transition_id,
    )
    .expect("load replayed automatic handoff package")
    .expect("automatic handoff package should remain persisted");
    assert_eq!(
        first_package.package_hash,
        replayed_package_row.package_hash
    );

    let reloaded_events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("reload replay transition events");
    assert_eq!(events, reloaded_events);
}

#[test]
fn workflow_transition_auto_dispatch_preserves_transition_when_handoff_package_build_fails() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-package-skip-redaction";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-auto-package-skip-redaction",
        "repo-graph-auto-package-skip-redaction",
    );

    seed_auto_dispatch_workflow_graph(&repo_root, project_id);

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE workflow_graph_nodes SET description = 'oauth access_token=sk-live-secret' WHERE project_id = ?1 AND node_id = 'verify'",
            [project_id],
        )
        .expect("inject secret-bearing destination description for auto-dispatch package");

    let applied = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-auto-package-skip-redaction-001".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("operator-approved".into()),
            gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Satisfied,
                decision_context: Some("approved by operator".into()),
            }],
            occurred_at: "2026-04-16T18:10:00Z".into(),
        },
    )
    .expect("transition should persist even when package assembly fails");

    let auto_transition_id = match &applied.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            transition_event,
            handoff_package,
        } => {
            match handoff_package {
                project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped { code, .. } => {
                    assert_eq!(code, "workflow_handoff_redaction_failed");
                }
                other => panic!(
                    "expected skipped handoff package outcome after redaction failure, got {other:?}"
                ),
            }
            transition_event.transition_id.clone()
        }
        other => panic!("expected applied automatic dispatch outcome, got {other:?}"),
    };

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after package redaction failure");
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .any(|event| event.transition_id == auto_transition_id),
        "automatic transition should remain durable when package assembly fails"
    );

    let package =
        project_store::load_workflow_handoff_package(&repo_root, project_id, &auto_transition_id)
            .expect("load handoff package after redaction failure");
    assert!(
        package.is_none(),
        "package persist failure should not create a handoff row"
    );
}

#[test]
fn workflow_transition_auto_dispatch_replay_surfaces_missing_handoff_package_row() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-package-replay-missing";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-auto-package-replay-missing",
        "repo-graph-auto-package-replay-missing",
    );

    seed_auto_dispatch_workflow_graph(&repo_root, project_id);

    let request = project_store::ApplyWorkflowTransitionRecord {
        transition_id: "txn-auto-package-replay-missing-001".into(),
        causal_transition_id: None,
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
        gate_decision_context: Some("operator-approved".into()),
        gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
            gate_key: "execution_gate".into(),
            gate_state: project_store::WorkflowGateState::Satisfied,
            decision_context: Some("approved by operator".into()),
        }],
        occurred_at: "2026-04-16T18:20:00Z".into(),
    };

    let first = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("first apply should persist transition and package");

    let auto_transition_id = match &first.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            transition_event,
            handoff_package,
        } => {
            assert!(matches!(
                handoff_package,
                project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { .. }
                    | project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { .. }
            ));
            transition_event.transition_id.clone()
        }
        other => panic!("expected applied automatic dispatch outcome, got {other:?}"),
    };

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "DELETE FROM workflow_handoff_packages WHERE project_id = ?1 AND handoff_transition_id = ?2",
            [project_id, auto_transition_id.as_str()],
        )
        .expect("remove persisted handoff package row to force replay lookup miss");

    let replayed = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("replayed apply should preserve transition and surface package lookup skip");

    match &replayed.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Replayed {
            handoff_package, ..
        } => match handoff_package {
            project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped { code, .. } => {
                assert_eq!(code, "workflow_handoff_replay_not_found");
            }
            other => panic!(
                "expected replayed handoff package skip outcome for missing row, got {other:?}"
            ),
        },
        other => panic!("expected replayed automatic dispatch outcome, got {other:?}"),
    }

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after replay lookup miss");
    assert_eq!(events.len(), 2);
}

#[test]
fn workflow_transition_auto_dispatch_replay_surfaces_decode_failure_as_skipped_package() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-package-replay-decode";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-auto-package-replay-decode",
        "repo-graph-auto-package-replay-decode",
    );

    seed_auto_dispatch_workflow_graph(&repo_root, project_id);

    let request = project_store::ApplyWorkflowTransitionRecord {
        transition_id: "txn-auto-package-replay-decode-001".into(),
        causal_transition_id: None,
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
        gate_decision_context: Some("operator-approved".into()),
        gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
            gate_key: "execution_gate".into(),
            gate_state: project_store::WorkflowGateState::Satisfied,
            decision_context: Some("approved by operator".into()),
        }],
        occurred_at: "2026-04-16T18:25:00Z".into(),
    };

    let first = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("first apply should persist transition and package");

    let auto_transition_id = match &first.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            transition_event,
            handoff_package,
        } => {
            assert!(matches!(
                handoff_package,
                project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { .. }
                    | project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { .. }
            ));
            transition_event.transition_id.clone()
        }
        other => panic!("expected applied automatic dispatch outcome, got {other:?}"),
    };

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable handoff-package check constraints for replay decode corruption");
    connection
        .execute(
            "UPDATE workflow_handoff_packages SET package_hash = 'abc' WHERE project_id = ?1 AND handoff_transition_id = ?2",
            [project_id, auto_transition_id.as_str()],
        )
        .expect("corrupt persisted handoff package hash for replay decode failure");

    let replayed = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("replayed apply should preserve transition and surface package decode skip");

    match &replayed.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Replayed {
            handoff_package, ..
        } => match handoff_package {
            project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped { code, .. } => {
                assert_eq!(code, "workflow_handoff_decode_failed");
            }
            other => panic!(
                "expected replayed handoff package skip outcome for decode failure, got {other:?}"
            ),
        },
        other => panic!("expected replayed automatic dispatch outcome, got {other:?}"),
    }

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after replay decode failure");
    assert_eq!(events.len(), 2);
}

#[test]
fn workflow_transition_auto_dispatch_replay_surfaces_transition_linkage_mismatch() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-auto-package-replay-linkage";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-auto-package-replay-linkage",
        "repo-graph-auto-package-replay-linkage",
    );

    seed_auto_dispatch_workflow_graph(&repo_root, project_id);

    let request = project_store::ApplyWorkflowTransitionRecord {
        transition_id: "txn-auto-package-replay-linkage-001".into(),
        causal_transition_id: None,
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
        gate_decision_context: Some("operator-approved".into()),
        gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
            gate_key: "execution_gate".into(),
            gate_state: project_store::WorkflowGateState::Satisfied,
            decision_context: Some("approved by operator".into()),
        }],
        occurred_at: "2026-04-16T18:30:00Z".into(),
    };

    let first = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("first apply should persist transition and package");

    let auto_transition_id = match &first.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Applied {
            transition_event,
            handoff_package,
        } => {
            assert!(matches!(
                handoff_package,
                project_store::WorkflowAutomaticDispatchPackageOutcome::Persisted { .. }
                    | project_store::WorkflowAutomaticDispatchPackageOutcome::Replayed { .. }
            ));
            transition_event.transition_id.clone()
        }
        other => panic!("expected applied automatic dispatch outcome, got {other:?}"),
    };

    let connection = open_state_connection(&repo_root);
    connection
        .execute(
            "UPDATE workflow_handoff_packages SET from_node_id = 'plan' WHERE project_id = ?1 AND handoff_transition_id = ?2",
            [project_id, auto_transition_id.as_str()],
        )
        .expect("corrupt persisted handoff package linkage for replay");

    let replayed = project_store::apply_workflow_transition(&repo_root, project_id, &request)
        .expect("replayed apply should preserve transition and surface package linkage skip");

    match &replayed.automatic_dispatch {
        project_store::WorkflowAutomaticDispatchOutcome::Replayed { handoff_package, .. } => {
            match handoff_package {
                project_store::WorkflowAutomaticDispatchPackageOutcome::Skipped { code, .. } => {
                    assert_eq!(code, "workflow_handoff_linkage_mismatch");
                }
                other => panic!(
                    "expected replayed handoff package skip outcome for linkage mismatch, got {other:?}"
                ),
            }
        }
        other => panic!("expected replayed automatic dispatch outcome, got {other:?}"),
    }
}

#[test]
fn workflow_transition_rejects_secret_bearing_diagnostics_and_preserves_state() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-4";
    let repo_root = seed_project(&root, project_id, "repo-graph-4", "repo-graph");

    let graph = project_store::WorkflowGraphUpsertRecord {
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
                task_count: 4,
                completed_tasks: 0,
                summary: None,
            },
        ],
        edges: vec![project_store::WorkflowGraphEdgeRecord {
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_requirement: Some("execution_gate".into()),
        }],
        gates: vec![project_store::WorkflowGateMetadataRecord {
            node_id: "execute".into(),
            gate_key: "execution_gate".into(),
            gate_state: project_store::WorkflowGateState::Pending,
            action_type: Some("approve_execution".into()),
            title: Some("Approve execution".into()),
            detail: Some("Operator approval required.".into()),
            decision_context: None,
        }],
    };

    project_store::upsert_workflow_graph(&repo_root, project_id, &graph).expect("seed graph");

    let error = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-secret".into(),
            causal_transition_id: Some("txn-prev".into()),
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("oauth access_token=sk-live-secret".into()),
            gate_updates: vec![project_store::WorkflowGateDecisionUpdate {
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Satisfied,
                decision_context: Some("approved by operator".into()),
            }],
            occurred_at: "2026-04-15T18:02:00Z".into(),
        },
    )
    .expect_err("secret-bearing diagnostics should be rejected");

    assert_eq!(error.code, "workflow_transition_request_invalid");

    let graph_after = project_store::load_workflow_graph(&repo_root, project_id)
        .expect("load graph after rejection");
    assert_eq!(graph_after.nodes[0].status, PhaseStatus::Active);
    assert_eq!(graph_after.nodes[1].status, PhaseStatus::Pending);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after rejection");
    assert!(events.is_empty());
}

#[test]
fn workflow_transition_rejects_malformed_target_gate_rows_and_preserves_state() {
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-graph-malformed-gates-1";
    let repo_root = seed_project(
        &root,
        project_id,
        "repo-graph-malformed-gates-1",
        "repo-graph",
    );

    let graph = project_store::WorkflowGraphUpsertRecord {
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
                task_count: 4,
                completed_tasks: 0,
                summary: None,
            },
        ],
        edges: vec![project_store::WorkflowGraphEdgeRecord {
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_requirement: Some("execution_gate".into()),
        }],
        gates: vec![project_store::WorkflowGateMetadataRecord {
            node_id: "execute".into(),
            gate_key: "execution_gate".into(),
            gate_state: project_store::WorkflowGateState::Pending,
            action_type: Some("approve_execution".into()),
            title: Some("Approve execution".into()),
            detail: Some("Operator approval required.".into()),
            decision_context: None,
        }],
    };

    project_store::upsert_workflow_graph(&repo_root, project_id, &graph).expect("seed graph");

    let connection = open_state_connection(&repo_root);
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints for corruption test");
    connection
        .execute(
            "UPDATE workflow_gate_metadata SET gate_state = 'mystery' WHERE project_id = ?1 AND gate_key = 'execution_gate'",
            [project_id],
        )
        .expect("corrupt gate metadata row");

    let error = project_store::apply_workflow_transition(
        &repo_root,
        project_id,
        &project_store::ApplyWorkflowTransitionRecord {
            transition_id: "txn-malformed-gate".into(),
            causal_transition_id: None,
            from_node_id: "plan".into(),
            to_node_id: "execute".into(),
            transition_kind: "advance".into(),
            gate_decision: project_store::WorkflowTransitionGateDecision::Approved,
            gate_decision_context: Some("operator-approved".into()),
            gate_updates: vec![],
            occurred_at: "2026-04-15T18:03:00Z".into(),
        },
    )
    .expect_err("malformed gate metadata should fail closed");

    assert_eq!(error.code, "workflow_transition_gate_check_failed");

    let connection = open_state_connection(&repo_root);
    let plan_status: String = connection
        .query_row(
            "SELECT status FROM workflow_graph_nodes WHERE project_id = ?1 AND node_id = 'plan'",
            [project_id],
            |row| row.get(0),
        )
        .expect("read plan status after malformed gate rejection");
    let execute_status: String = connection
        .query_row(
            "SELECT status FROM workflow_graph_nodes WHERE project_id = ?1 AND node_id = 'execute'",
            [project_id],
            |row| row.get(0),
        )
        .expect("read execute status after malformed gate rejection");

    assert_eq!(plan_status, "active");
    assert_eq!(execute_status, "pending");

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, project_id, None)
            .expect("load transition events after malformed gate rejection");
    assert!(events.is_empty());
}

#[test]
fn workflow_snapshot_projects_planning_lifecycle_with_gate_and_transition_metadata() {
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

#[test]
fn lifecycle_alias_collisions_fail_closed_during_projection_decode() {
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

#[test]
fn malformed_lifecycle_gate_rows_fail_closed_during_projection_decode() {
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

#[test]
fn malformed_graph_rows_fail_closed_during_projection_decode() {
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

#[test]
fn malformed_graph_current_step_fails_closed_during_projection_decode() {
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

#[test]
fn workflow_handoff_package_persists_with_transition_linkage_and_replays_idempotently() {
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

#[test]
fn workflow_handoff_package_rejects_missing_transition_and_malformed_payload() {
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

#[test]
fn workflow_handoff_package_decode_fails_closed_on_corrupted_hash_rows() {
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

#[test]
fn workflow_handoff_package_assembly_is_deterministic_for_replay_inputs() {
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

#[test]
fn workflow_handoff_package_assembly_fails_closed_on_secret_bearing_destination_content() {
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

#[test]
fn workflow_handoff_package_assembly_rejects_missing_target_node_metadata() {
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

#[test]
fn workflow_handoff_package_assembly_rejects_malformed_gate_state_rows() {
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

#[test]
fn workflow_handoff_package_assembly_rejects_invalid_lifecycle_projection_shape() {
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

#[test]
fn workflow_handoff_package_assembly_rejects_auto_transition_without_causal_linkage() {
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
