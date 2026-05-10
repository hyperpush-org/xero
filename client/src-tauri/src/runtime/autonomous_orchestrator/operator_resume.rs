use std::path::Path;

use crate::{
    auth::now_timestamp,
    commands::CommandError,
    db::project_store::{self, AutonomousRunSnapshotRecord, AutonomousRunStatus, RuntimeRunStatus},
};

use super::{
    persist_autonomous_run_scaffold, reconcile_runtime_snapshot, AutonomousRuntimeReconcileIntent,
};

pub fn validate_operator_resume_target(
    _repo_root: &Path,
    _project_id: &str,
    _agent_session_id: &str,
    _action_id: &str,
    _boundary_id: &str,
) -> Result<(), CommandError> {
    Ok(())
}

pub fn persist_operator_resume(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    _action_id: &str,
    _boundary_id: &str,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let runtime_snapshot =
        match project_store::load_runtime_run(repo_root, project_id, agent_session_id)? {
            Some(snapshot) => snapshot,
            None => return Ok(None),
        };
    let existing = project_store::load_autonomous_run(repo_root, project_id, agent_session_id)?;
    let mut payload = reconcile_runtime_snapshot(
        existing.as_ref(),
        &runtime_snapshot,
        AutonomousRuntimeReconcileIntent::Observe,
    );
    let timestamp = now_timestamp();
    payload.run.status = match runtime_snapshot.run.status {
        RuntimeRunStatus::Starting | RuntimeRunStatus::Running => AutonomousRunStatus::Running,
        RuntimeRunStatus::Stale => AutonomousRunStatus::Stale,
        RuntimeRunStatus::Failed => AutonomousRunStatus::Failed,
        RuntimeRunStatus::Stopped => AutonomousRunStatus::Stopped,
    };
    payload.run.paused_at = None;
    payload.run.pause_reason = None;
    payload.run.updated_at = timestamp;

    persist_autonomous_run_scaffold(repo_root, existing.as_ref(), payload).map(Some)
}
