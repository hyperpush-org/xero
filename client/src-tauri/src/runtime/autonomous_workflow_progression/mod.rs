use std::path::Path;

use crate::{
    commands::CommandError,
    db::project_store::{
        self, AutonomousRunSnapshotRecord, AutonomousRunStatus, AutonomousRunUpsertRecord,
    },
};

mod linkage;
mod payload;
mod progression;

const MAX_PROGRESSION_STEPS: usize = 16;

pub fn persist_autonomous_workflow_progression(
    repo_root: &Path,
    project_id: &str,
    existing: Option<&AutonomousRunSnapshotRecord>,
    payload: AutonomousRunUpsertRecord,
) -> Result<AutonomousRunSnapshotRecord, CommandError> {
    if let Some(existing) = existing {
        let existing_status = &existing.run.status;
        if existing.run.run_id != payload.run.run_id
            && !matches!(
                existing_status,
                AutonomousRunStatus::Cancelled
                    | AutonomousRunStatus::Completed
                    | AutonomousRunStatus::Stopped
            )
        {
            return Err(CommandError::retryable(
                "autonomous_workflow_run_mismatch",
                format!(
                    "Cadence refused to advance autonomous workflow progression because active runtime run `{}` does not match durable autonomous run `{}`.",
                    payload.run.run_id, existing.run.run_id
                ),
            ));
        }
    }

    if !matches!(
        payload.run.status,
        AutonomousRunStatus::Starting | AutonomousRunStatus::Running
    ) {
        return payload::persist_autonomous_run_if_changed(repo_root, existing, &payload);
    }

    let graph = project_store::load_workflow_graph(repo_root, project_id)?;
    if graph.nodes.is_empty() {
        return payload::persist_autonomous_run_if_changed(repo_root, existing, &payload);
    }

    let existing_linkage = existing
        .and_then(|snapshot| snapshot.unit.as_ref())
        .and_then(|unit| unit.workflow_linkage.as_ref());

    let active_node = linkage::resolve_active_node(&graph.nodes)?;
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

        let active_linkage = progression::resolve_linkage_for_active_node(
            repo_root,
            project_id,
            &active_node,
        )?
        .ok_or_else(|| {
            CommandError::retryable(
                "autonomous_workflow_linkage_identity_conflict",
                format!(
                    "Cadence refused to advance autonomous workflow progression because active workflow node `{}` does not expose a durable transition/handoff linkage.",
                    active_node.node_id
                ),
            )
        })?;

        if *linkage != active_linkage {
            return Err(CommandError::user_fixable(
                "autonomous_workflow_linkage_identity_conflict",
                format!(
                    "Cadence refused to advance autonomous workflow progression because durable linkage identity for workflow node `{}` does not match the latest transition/handoff identity (durable transition `{}` / handoff hash `{}` vs active transition `{}` / handoff hash `{}`).",
                    active_node.node_id,
                    linkage.transition_id,
                    linkage.handoff_package_hash,
                    active_linkage.transition_id,
                    active_linkage.handoff_package_hash,
                ),
            ));
        }
    }

    let progression_states = progression::collect_progression_states(
        repo_root,
        project_id,
        &payload.run.run_id,
        existing_linkage,
    )?;

    if progression_states.is_empty() {
        return payload::persist_autonomous_run_if_changed(repo_root, existing, &payload);
    }

    let mut persisted = existing.cloned();
    let mut working_payload = payload;
    for progression in progression_states {
        working_payload = linkage::reconcile_payload_with_progression_stage(
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
