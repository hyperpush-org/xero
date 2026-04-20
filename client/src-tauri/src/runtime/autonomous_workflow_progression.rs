use std::path::Path;

use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, PhaseStatus, PhaseStep, PlanningLifecycleStageKindDto},
    db::project_store::{
        self, ApplyWorkflowTransitionRecord, AutonomousRunSnapshotRecord, AutonomousRunStatus,
        AutonomousRunUpsertRecord, AutonomousUnitArtifactRecord, AutonomousUnitAttemptRecord,
        AutonomousUnitKind, AutonomousUnitRecord, AutonomousUnitStatus,
        AutonomousWorkflowLinkageRecord, WorkflowAutomaticDispatchOutcome,
        WorkflowAutomaticDispatchPackageOutcome, WorkflowGraphEdgeRecord, WorkflowGraphNodeRecord,
        WorkflowHandoffPackageRecord, WorkflowTransitionEventRecord,
        WorkflowTransitionGateDecision,
    },
};

const MAX_PROGRESSION_STEPS: usize = 16;

pub fn persist_autonomous_workflow_progression(
    repo_root: &Path,
    project_id: &str,
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: AutonomousRunUpsertRecord,
) -> Result<AutonomousRunSnapshotRecord, CommandError> {
    if !matches!(
        payload.run.status,
        AutonomousRunStatus::Starting | AutonomousRunStatus::Running
    ) {
        return persist_autonomous_run_if_changed(repo_root, existing, &payload);
    }

    let graph = project_store::load_workflow_graph(repo_root, project_id)?;
    if graph.nodes.is_empty() {
        return persist_autonomous_run_if_changed(repo_root, existing, &payload);
    }

    let existing_linkage = existing
        .and_then(|snapshot| snapshot.unit.as_ref())
        .and_then(|unit| unit.workflow_linkage.as_ref());

    let active_node = resolve_active_node(&graph.nodes)?;
    if let Some(linkage) = existing_linkage {
        if linkage.workflow_node_id != active_node.node_id {
            return Err(CommandError::user_fixable(
                "autonomous_workflow_linkage_stage_conflict",
                format!(
                    "Cadence refused to advance autonomous workflow progression because the durable autonomous linkage points at workflow node `{}` while the active workflow node is `{}`.",
                    linkage.workflow_node_id, active_node.node_id
                ),
            ));
        }
    }

    let progression_states =
        collect_progression_states(repo_root, project_id, &payload.run.run_id, existing_linkage)?;

    if progression_states.is_empty() {
        return persist_autonomous_run_if_changed(repo_root, existing, &payload);
    }

    let mut persisted = existing.cloned();
    let mut working_payload = payload;
    for progression in progression_states {
        working_payload = reconcile_payload_with_progression_stage(
            persisted.as_ref(),
            working_payload,
            &progression,
        );
        persisted = Some(project_store::upsert_autonomous_run(
            repo_root,
            &working_payload,
        )?);
    }

    persisted.ok_or_else(|| {
        CommandError::system_fault(
            "autonomous_workflow_progression_missing",
            format!(
                "Cadence progressed autonomous workflow state for project `{project_id}` but could not read back the durable autonomous snapshot."
            ),
        )
    })
}

fn persist_autonomous_run_if_changed(
    repo_root: &Path,
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: &AutonomousRunUpsertRecord,
) -> Result<AutonomousRunSnapshotRecord, CommandError> {
    if autonomous_run_payload_matches_existing(existing, payload) {
        return Ok(existing
            .expect("matching autonomous payload requires an existing snapshot")
            .clone());
    }

    project_store::upsert_autonomous_run(repo_root, payload)
}

fn autonomous_run_payload_matches_existing(
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: &AutonomousRunUpsertRecord,
) -> bool {
    let Some(existing) = existing else {
        return false;
    };

    existing.run == payload.run
        && existing.unit == payload.unit
        && existing.attempt == payload.attempt
        && current_attempt_artifacts(existing) == payload.artifacts.as_slice()
}

fn current_attempt_artifacts(
    existing: &AutonomousRunSnapshotRecord,
) -> &[AutonomousUnitArtifactRecord] {
    let Some(attempt_id) = existing
        .attempt
        .as_ref()
        .map(|attempt| attempt.attempt_id.as_str())
    else {
        return &[];
    };

    existing
        .history
        .iter()
        .find(|entry| {
            entry
                .latest_attempt
                .as_ref()
                .is_some_and(|attempt| attempt.attempt_id == attempt_id)
        })
        .map(|entry| entry.artifacts.as_slice())
        .unwrap_or(&[])
}

#[derive(Debug, Clone)]
struct StableProgressionState {
    unit_kind: AutonomousUnitKind,
    workflow_linkage: Option<AutonomousWorkflowLinkageRecord>,
    unit_summary: String,
}

fn collect_progression_states(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    existing_linkage: Option<&AutonomousWorkflowLinkageRecord>,
) -> Result<Vec<StableProgressionState>, CommandError> {
    let mut graph = project_store::load_workflow_graph(repo_root, project_id)?;
    let mut active_node = resolve_active_node(&graph.nodes)?;
    let mut linkage = match existing_linkage.cloned() {
        Some(linkage) => Some(linkage),
        None => resolve_linkage_for_active_node(repo_root, project_id, &active_node)?,
    };

    let mut states = Vec::new();
    let mut steps = 0_usize;
    loop {
        if steps >= MAX_PROGRESSION_STEPS {
            return Err(CommandError::system_fault(
                "autonomous_workflow_progression_loop",
                format!(
                    "Cadence aborted autonomous workflow progression after {MAX_PROGRESSION_STEPS} replay steps for project `{project_id}` to avoid an infinite loop."
                ),
            ));
        }

        if let Some(current_linkage) = linkage.clone() {
            let replay_event = project_store::load_workflow_transition_event(
                repo_root,
                project_id,
                &current_linkage.transition_id,
            )?
            .ok_or_else(|| {
                CommandError::retryable(
                    "autonomous_workflow_transition_missing",
                    format!(
                        "Cadence cannot replay autonomous workflow transition `{}` because the durable transition row is missing for project `{project_id}`.",
                        current_linkage.transition_id
                    ),
                )
            })?;

            let replayed = project_store::apply_workflow_transition(
                repo_root,
                project_id,
                &ApplyWorkflowTransitionRecord {
                    transition_id: replay_event.transition_id.clone(),
                    causal_transition_id: replay_event.causal_transition_id.clone(),
                    from_node_id: replay_event.from_node_id.clone(),
                    to_node_id: replay_event.to_node_id.clone(),
                    transition_kind: replay_event.transition_kind.clone(),
                    gate_decision: WorkflowTransitionGateDecision::NotApplicable,
                    gate_decision_context: None,
                    gate_updates: Vec::new(),
                    occurred_at: replay_event.created_at.clone(),
                },
            )?;

            match replayed.automatic_dispatch {
                WorkflowAutomaticDispatchOutcome::Applied {
                    transition_event,
                    handoff_package,
                }
                | WorkflowAutomaticDispatchOutcome::Replayed {
                    transition_event,
                    handoff_package,
                } => {
                    let package = ensure_progression_handoff(
                        repo_root,
                        project_id,
                        &transition_event,
                        Some(handoff_package),
                    )?;
                    graph = project_store::load_workflow_graph(repo_root, project_id)?;
                    active_node = resolve_active_node(&graph.nodes)?;
                    let state =
                        stage_state_from_transition(&graph.nodes, &transition_event, &package)?;
                    linkage = state.workflow_linkage.clone();
                    states.push(state);
                    steps += 1;
                    continue;
                }
                WorkflowAutomaticDispatchOutcome::NoContinuation
                | WorkflowAutomaticDispatchOutcome::Skipped { .. } => {
                    let package = ensure_transition_handoff(repo_root, project_id, &replay_event)?;
                    graph = project_store::load_workflow_graph(repo_root, project_id)?;
                    states.push(stage_state_from_transition(
                        &graph.nodes,
                        &replay_event,
                        &package,
                    )?);
                    return Ok(states);
                }
            }
        }

        if !is_seedable_initial_node(&active_node) {
            let _ = map_node_to_unit_kind(&active_node)?;
            return Err(CommandError::retryable(
                "autonomous_workflow_linkage_missing",
                format!(
                    "Cadence cannot continue autonomous workflow progression from active workflow node `{}` because no durable handoff linkage is available to replay.",
                    active_node.node_id
                ),
            ));
        }

        let seed_edge = resolve_unique_outgoing_edge(&graph.edges, &active_node)?;
        let transition_id =
            derive_seed_transition_id(run_id, &active_node.node_id, &seed_edge.to_node_id);
        let occurred_at = now_timestamp();
        let seeded = project_store::apply_workflow_transition(
            repo_root,
            project_id,
            &ApplyWorkflowTransitionRecord {
                transition_id,
                causal_transition_id: None,
                from_node_id: seed_edge.from_node_id.clone(),
                to_node_id: seed_edge.to_node_id.clone(),
                transition_kind: seed_edge.transition_kind.clone(),
                gate_decision: gate_decision_for_seed_edge(seed_edge),
                gate_decision_context: Some(
                    "autonomous progression seeded the initial workflow stage".into(),
                ),
                gate_updates: Vec::new(),
                occurred_at,
            },
        )?;

        graph = project_store::load_workflow_graph(repo_root, project_id)?;
        active_node = resolve_active_node(&graph.nodes)?;

        let seeded_package =
            ensure_transition_handoff(repo_root, project_id, &seeded.transition_event)?;
        states.push(stage_state_from_transition(
            &graph.nodes,
            &seeded.transition_event,
            &seeded_package,
        )?);
        steps += 1;

        match seeded.automatic_dispatch {
            WorkflowAutomaticDispatchOutcome::Applied {
                transition_event,
                handoff_package,
            }
            | WorkflowAutomaticDispatchOutcome::Replayed {
                transition_event,
                handoff_package,
            } => {
                let package = ensure_progression_handoff(
                    repo_root,
                    project_id,
                    &transition_event,
                    Some(handoff_package),
                )?;
                let state = stage_state_from_transition(&graph.nodes, &transition_event, &package)?;
                linkage = state.workflow_linkage.clone();
                states.push(state);
                steps += 1;
                continue;
            }
            WorkflowAutomaticDispatchOutcome::NoContinuation
            | WorkflowAutomaticDispatchOutcome::Skipped { .. } => return Ok(states),
        }
    }
}

fn reconcile_payload_with_progression_stage(
    existing: Option<&AutonomousRunSnapshotRecord>,
    mut payload: AutonomousRunUpsertRecord,
    progression: &StableProgressionState,
) -> AutonomousRunUpsertRecord {
    let Some(current_unit) = payload.unit.as_ref() else {
        return payload;
    };
    let Some(current_attempt) = payload.attempt.as_ref() else {
        return payload;
    };

    if should_reuse_current_identity(current_unit, current_attempt, progression) {
        payload.run.active_unit_sequence = Some(current_unit.sequence);

        if let Some(unit) = payload.unit.as_mut() {
            unit.kind = progression.unit_kind.clone();
            unit.workflow_linkage = progression.workflow_linkage.clone();
            if unit.boundary_id.is_none()
                && !matches!(
                    unit.status,
                    AutonomousUnitStatus::Blocked | AutonomousUnitStatus::Paused
                )
            {
                unit.summary = progression.unit_summary.clone();
            }
        }

        if let Some(attempt) = payload.attempt.as_mut() {
            attempt.workflow_linkage = progression.workflow_linkage.clone();
        }

        return payload;
    }

    let next_sequence = next_unit_sequence(existing, &payload);
    let next_attempt_number = next_attempt_number(existing, &payload);
    let timestamp = payload.run.updated_at.clone();
    let unit_status = autonomous_unit_status_for_run(&payload.run.status);
    let unit_id = format!("{}:unit:{}", payload.run.run_id, next_sequence);
    let attempt_id = format!("{unit_id}:attempt:{next_attempt_number}");

    payload.run.active_unit_sequence = Some(next_sequence);
    payload.unit = Some(AutonomousUnitRecord {
        project_id: payload.run.project_id.clone(),
        run_id: payload.run.run_id.clone(),
        unit_id: unit_id.clone(),
        sequence: next_sequence,
        kind: progression.unit_kind.clone(),
        status: unit_status.clone(),
        summary: progression.unit_summary.clone(),
        boundary_id: None,
        workflow_linkage: progression.workflow_linkage.clone(),
        started_at: timestamp.clone(),
        finished_at: None,
        updated_at: timestamp.clone(),
        last_error: payload.run.last_error.clone(),
    });
    payload.attempt = Some(AutonomousUnitAttemptRecord {
        project_id: payload.run.project_id.clone(),
        run_id: payload.run.run_id.clone(),
        unit_id,
        attempt_id,
        attempt_number: next_attempt_number,
        child_session_id: generate_autonomous_child_session_id(),
        status: unit_status,
        boundary_id: None,
        workflow_linkage: progression.workflow_linkage.clone(),
        started_at: timestamp.clone(),
        finished_at: None,
        updated_at: timestamp,
        last_error: payload.run.last_error.clone(),
    });
    payload.artifacts = Vec::new();

    payload
}

fn should_reuse_current_identity(
    unit: &AutonomousUnitRecord,
    attempt: &AutonomousUnitAttemptRecord,
    progression: &StableProgressionState,
) -> bool {
    let target_linkage = progression.workflow_linkage.as_ref();
    unit.workflow_linkage.as_ref() == target_linkage
        || attempt.workflow_linkage.as_ref() == target_linkage
        || (unit.workflow_linkage.is_none() && attempt.workflow_linkage.is_none())
}

fn next_unit_sequence(
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: &AutonomousRunUpsertRecord,
) -> u32 {
    existing
        .map(|snapshot| {
            snapshot
                .history
                .iter()
                .map(|entry| entry.unit.sequence)
                .max()
                .unwrap_or(0)
        })
        .unwrap_or_else(|| payload.unit.as_ref().map(|unit| unit.sequence).unwrap_or(0))
        + 1
}

fn next_attempt_number(
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: &AutonomousRunUpsertRecord,
) -> u32 {
    existing
        .map(|snapshot| {
            snapshot
                .history
                .iter()
                .filter_map(|entry| {
                    entry
                        .latest_attempt
                        .as_ref()
                        .map(|attempt| attempt.attempt_number)
                })
                .max()
                .unwrap_or(0)
        })
        .unwrap_or_else(|| {
            payload
                .attempt
                .as_ref()
                .map(|attempt| attempt.attempt_number)
                .unwrap_or(0)
        })
        + 1
}

fn stage_state_from_transition(
    nodes: &[WorkflowGraphNodeRecord],
    transition_event: &WorkflowTransitionEventRecord,
    package: &WorkflowHandoffPackageRecord,
) -> Result<StableProgressionState, CommandError> {
    let node = resolve_target_node(nodes, &transition_event.to_node_id)?;
    let unit_kind = map_node_to_unit_kind(&node)?;
    Ok(StableProgressionState {
        unit_summary: linked_unit_summary(&unit_kind, &node.name, package),
        unit_kind,
        workflow_linkage: Some(linkage_from_transition_and_package(
            transition_event,
            package,
        )),
    })
}

fn resolve_target_node(
    nodes: &[WorkflowGraphNodeRecord],
    node_id: &str,
) -> Result<WorkflowGraphNodeRecord, CommandError> {
    nodes.iter()
        .find(|node| node.node_id == node_id)
        .cloned()
        .ok_or_else(|| {
            CommandError::retryable(
                "autonomous_workflow_target_node_missing",
                format!(
                    "Cadence could not resolve workflow node `{node_id}` while reconciling autonomous workflow progression."
                ),
            )
        })
}

fn ensure_progression_handoff(
    repo_root: &Path,
    project_id: &str,
    transition_event: &WorkflowTransitionEventRecord,
    observed: Option<WorkflowAutomaticDispatchPackageOutcome>,
) -> Result<WorkflowHandoffPackageRecord, CommandError> {
    match observed {
        Some(WorkflowAutomaticDispatchPackageOutcome::Persisted { package })
        | Some(WorkflowAutomaticDispatchPackageOutcome::Replayed { package }) => Ok(package),
        Some(WorkflowAutomaticDispatchPackageOutcome::Skipped { .. }) | None => {
            ensure_transition_handoff(repo_root, project_id, transition_event)
        }
    }
}

fn ensure_transition_handoff(
    repo_root: &Path,
    project_id: &str,
    transition_event: &WorkflowTransitionEventRecord,
) -> Result<WorkflowHandoffPackageRecord, CommandError> {
    match project_store::load_workflow_handoff_package(
        repo_root,
        project_id,
        &transition_event.transition_id,
    )? {
        Some(package) => Ok(package),
        None => project_store::assemble_and_persist_workflow_handoff_package(
            repo_root,
            project_id,
            &transition_event.transition_id,
        ),
    }
}

fn resolve_linkage_for_active_node(
    repo_root: &Path,
    project_id: &str,
    active_node: &WorkflowGraphNodeRecord,
) -> Result<Option<AutonomousWorkflowLinkageRecord>, CommandError> {
    let events =
        project_store::load_recent_workflow_transition_events(repo_root, project_id, None)?;
    let Some(transition_event) = events
        .into_iter()
        .find(|event| event.to_node_id == active_node.node_id)
    else {
        return Ok(None);
    };

    let package = ensure_transition_handoff(repo_root, project_id, &transition_event)?;
    Ok(Some(linkage_from_transition_and_package(
        &transition_event,
        &package,
    )))
}

fn resolve_active_node(
    nodes: &[WorkflowGraphNodeRecord],
) -> Result<WorkflowGraphNodeRecord, CommandError> {
    let mut active_nodes = nodes
        .iter()
        .filter(|node| node.status == PhaseStatus::Active)
        .cloned()
        .collect::<Vec<_>>();

    active_nodes.sort_by(|left, right| {
        left.sort_order
            .cmp(&right.sort_order)
            .then_with(|| left.node_id.cmp(&right.node_id))
    });

    match active_nodes.len() {
        0 => Err(CommandError::retryable(
            "autonomous_workflow_active_node_missing",
            "Cadence cannot advance autonomous workflow progression because the workflow graph has no active node.".to_string(),
        )),
        1 => Ok(active_nodes.remove(0)),
        _ => Err(CommandError::user_fixable(
            "autonomous_workflow_active_node_ambiguous",
            format!(
                "Cadence cannot advance autonomous workflow progression because the workflow graph exposes multiple active nodes ({}).",
                active_nodes
                    .iter()
                    .map(|node| node.node_id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

fn resolve_unique_outgoing_edge<'a>(
    edges: &'a [WorkflowGraphEdgeRecord],
    active_node: &WorkflowGraphNodeRecord,
) -> Result<&'a WorkflowGraphEdgeRecord, CommandError> {
    let mut candidates = edges
        .iter()
        .filter(|edge| edge.from_node_id == active_node.node_id)
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        left.to_node_id
            .cmp(&right.to_node_id)
            .then_with(|| left.transition_kind.cmp(&right.transition_kind))
    });

    match candidates.len() {
        0 => Err(CommandError::retryable(
            "autonomous_workflow_seed_edge_missing",
            format!(
                "Cadence cannot seed autonomous workflow progression from `{}` because the active node has no legal continuation edge.",
                active_node.node_id
            ),
        )),
        1 => Ok(candidates.remove(0)),
        _ => Err(CommandError::user_fixable(
            "autonomous_workflow_seed_edge_ambiguous",
            format!(
                "Cadence cannot seed autonomous workflow progression from `{}` because multiple legal continuation edges exist ({}).",
                active_node.node_id,
                candidates
                    .iter()
                    .map(|edge| format!("{}:{}", edge.to_node_id, edge.transition_kind))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        )),
    }
}

fn map_node_to_unit_kind(
    node: &WorkflowGraphNodeRecord,
) -> Result<AutonomousUnitKind, CommandError> {
    if let Some(stage) = classify_stage(node) {
        return Ok(match stage {
            PlanningLifecycleStageKindDto::Discussion | PlanningLifecycleStageKindDto::Research => {
                AutonomousUnitKind::Researcher
            }
            PlanningLifecycleStageKindDto::Requirements
            | PlanningLifecycleStageKindDto::Roadmap => AutonomousUnitKind::Planner,
        });
    }

    match node.current_step {
        Some(PhaseStep::Discuss) => Ok(AutonomousUnitKind::Researcher),
        Some(PhaseStep::Plan) => Ok(AutonomousUnitKind::Planner),
        Some(PhaseStep::Execute) => Ok(AutonomousUnitKind::Executor),
        Some(PhaseStep::Verify | PhaseStep::Ship) => Ok(AutonomousUnitKind::Verifier),
        None => Err(CommandError::user_fixable(
            "autonomous_workflow_unit_mapping_invalid",
            format!(
                "Cadence cannot map workflow node `{}` onto an autonomous unit kind because the node does not expose a recognized lifecycle stage or step.",
                node.node_id
            ),
        )),
    }
}

fn classify_stage(node: &WorkflowGraphNodeRecord) -> Option<PlanningLifecycleStageKindDto> {
    let normalized = node.node_id.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "discussion"
        | "discuss"
        | "plan-discussion"
        | "planning-discussion"
        | "workflow-discussion"
        | "lifecycle-discussion" => Some(PlanningLifecycleStageKindDto::Discussion),
        "research" | "plan-research" | "planning-research" | "workflow-research"
        | "lifecycle-research" => Some(PlanningLifecycleStageKindDto::Research),
        "requirements"
        | "requirement"
        | "plan-requirements"
        | "planning-requirements"
        | "workflow-requirements"
        | "lifecycle-requirements" => Some(PlanningLifecycleStageKindDto::Requirements),
        "roadmap" | "plan-roadmap" | "planning-roadmap" | "workflow-roadmap"
        | "lifecycle-roadmap" => Some(PlanningLifecycleStageKindDto::Roadmap),
        _ => None,
    }
}

fn is_seedable_initial_node(node: &WorkflowGraphNodeRecord) -> bool {
    matches!(
        classify_stage(node),
        Some(PlanningLifecycleStageKindDto::Discussion)
    ) || matches!(node.current_step, Some(PhaseStep::Discuss))
}

fn gate_decision_for_seed_edge(edge: &WorkflowGraphEdgeRecord) -> WorkflowTransitionGateDecision {
    if edge.gate_requirement.is_some() {
        WorkflowTransitionGateDecision::Approved
    } else {
        WorkflowTransitionGateDecision::NotApplicable
    }
}

fn derive_seed_transition_id(run_id: &str, from_node_id: &str, to_node_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(run_id.as_bytes());
    hasher.update([0]);
    hasher.update(from_node_id.as_bytes());
    hasher.update([0]);
    hasher.update(to_node_id.as_bytes());
    let digest = hasher.finalize();
    let suffix = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("autonomous:{from_node_id}:{to_node_id}:{suffix}")
}

fn linkage_from_transition_and_package(
    transition_event: &WorkflowTransitionEventRecord,
    package: &WorkflowHandoffPackageRecord,
) -> AutonomousWorkflowLinkageRecord {
    AutonomousWorkflowLinkageRecord {
        workflow_node_id: transition_event.to_node_id.clone(),
        transition_id: transition_event.transition_id.clone(),
        causal_transition_id: transition_event.causal_transition_id.clone(),
        handoff_transition_id: package.handoff_transition_id.clone(),
        handoff_package_hash: package.package_hash.clone(),
    }
}

fn linked_unit_summary(
    unit_kind: &AutonomousUnitKind,
    node_name: &str,
    package: &WorkflowHandoffPackageRecord,
) -> String {
    format!(
        "{} child session is using persisted workflow handoff `{}` for stage `{}`.",
        autonomous_unit_kind_label(unit_kind),
        package.handoff_transition_id,
        node_name
    )
}

fn autonomous_unit_kind_label(kind: &AutonomousUnitKind) -> &'static str {
    match kind {
        AutonomousUnitKind::Researcher => "Researcher",
        AutonomousUnitKind::Planner => "Planner",
        AutonomousUnitKind::Executor => "Executor",
        AutonomousUnitKind::Verifier => "Verifier",
    }
}

fn autonomous_unit_status_for_run(status: &AutonomousRunStatus) -> AutonomousUnitStatus {
    match status {
        AutonomousRunStatus::Starting
        | AutonomousRunStatus::Running
        | AutonomousRunStatus::Stale
        | AutonomousRunStatus::Cancelling => AutonomousUnitStatus::Active,
        AutonomousRunStatus::Paused => AutonomousUnitStatus::Paused,
        AutonomousRunStatus::Cancelled => AutonomousUnitStatus::Cancelled,
        AutonomousRunStatus::Stopped | AutonomousRunStatus::Completed => {
            AutonomousUnitStatus::Completed
        }
        AutonomousRunStatus::Failed | AutonomousRunStatus::Crashed => AutonomousUnitStatus::Failed,
    }
}

fn generate_autonomous_child_session_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "child-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
