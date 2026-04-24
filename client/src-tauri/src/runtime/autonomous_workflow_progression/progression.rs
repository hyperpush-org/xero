use std::path::Path;

use crate::{
    auth::now_timestamp,
    commands::CommandError,
    db::project_store::{
        self, ApplyWorkflowTransitionRecord, AutonomousWorkflowLinkageRecord,
        WorkflowAutomaticDispatchOutcome, WorkflowAutomaticDispatchPackageOutcome,
        WorkflowGraphEdgeRecord, WorkflowGraphNodeRecord, WorkflowHandoffPackageRecord,
        WorkflowTransitionEventRecord, WorkflowTransitionGateDecision,
    },
};

use super::{linkage, MAX_PROGRESSION_STEPS};

const WORKFLOW_TRANSITION_GATE_UNMET_CODE: &str = "workflow_transition_gate_unmet";

pub(super) fn collect_progression_states(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    existing_linkage: Option<&AutonomousWorkflowLinkageRecord>,
) -> Result<Vec<linkage::StableProgressionState>, CommandError> {
    let mut graph = project_store::load_workflow_graph(repo_root, project_id)?;
    let mut active_node = linkage::resolve_active_node(&graph.nodes)?;
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
                    active_node = linkage::resolve_active_node(&graph.nodes)?;
                    let state = linkage::stage_state_from_transition(
                        &graph.nodes,
                        &transition_event,
                        &package,
                    )?;
                    linkage = state.workflow_linkage.clone();
                    states.push(state);
                    steps += 1;
                    continue;
                }
                WorkflowAutomaticDispatchOutcome::Skipped { code, .. }
                    if code == WORKFLOW_TRANSITION_GATE_UNMET_CODE =>
                {
                    let package = ensure_transition_handoff(repo_root, project_id, &replay_event)?;
                    graph = project_store::load_workflow_graph(repo_root, project_id)?;
                    states.push(linkage::stage_state_from_transition(
                        &graph.nodes,
                        &replay_event,
                        &package,
                    )?);
                    return Ok(states);
                }
                WorkflowAutomaticDispatchOutcome::NoContinuation
                | WorkflowAutomaticDispatchOutcome::Skipped { .. } => {
                    let package = ensure_transition_handoff(repo_root, project_id, &replay_event)?;
                    graph = project_store::load_workflow_graph(repo_root, project_id)?;
                    states.push(linkage::stage_state_from_transition(
                        &graph.nodes,
                        &replay_event,
                        &package,
                    )?);
                    return Ok(states);
                }
            }
        }

        if !is_seedable_initial_node(&active_node) {
            let _ = linkage::map_node_to_unit_kind(&active_node)?;
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
        active_node = linkage::resolve_active_node(&graph.nodes)?;

        let seeded_package =
            ensure_transition_handoff(repo_root, project_id, &seeded.transition_event)?;
        states.push(linkage::stage_state_from_transition(
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
                let state = linkage::stage_state_from_transition(
                    &graph.nodes,
                    &transition_event,
                    &package,
                )?;
                linkage = state.workflow_linkage.clone();
                states.push(state);
                steps += 1;
                continue;
            }
            WorkflowAutomaticDispatchOutcome::Skipped { code, .. }
                if code == WORKFLOW_TRANSITION_GATE_UNMET_CODE =>
            {
                return Ok(states);
            }
            WorkflowAutomaticDispatchOutcome::NoContinuation
            | WorkflowAutomaticDispatchOutcome::Skipped { .. } => return Ok(states),
        }
    }
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

pub(super) fn resolve_linkage_for_active_node(
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
    Ok(Some(linkage::linkage_from_transition_and_package(
        &transition_event,
        &package,
    )))
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

fn is_seedable_initial_node(node: &WorkflowGraphNodeRecord) -> bool {
    matches!(
        linkage::classify_stage(node),
        Some(crate::commands::PlanningLifecycleStageKindDto::Discussion)
    ) || matches!(node.current_step, Some(crate::commands::PhaseStep::Discuss))
}

fn gate_decision_for_seed_edge(edge: &WorkflowGraphEdgeRecord) -> WorkflowTransitionGateDecision {
    if edge.gate_requirement.is_some() {
        WorkflowTransitionGateDecision::Approved
    } else {
        WorkflowTransitionGateDecision::NotApplicable
    }
}

fn derive_seed_transition_id(run_id: &str, from_node_id: &str, to_node_id: &str) -> String {
    use sha2::{Digest, Sha256};

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
