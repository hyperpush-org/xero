use std::path::Path;

use crate::{
    auth::now_timestamp,
    commands::CommandError,
    db::project_store::{
        self, AutonomousRunSnapshotRecord, AutonomousRunStatus, RuntimeRunDiagnosticRecord,
    },
    runtime::protocol::SupervisorLiveEventPayload,
};

use super::{
    persist_autonomous_run_scaffold, reconcile_runtime_snapshot, AutonomousRuntimeReconcileIntent,
};

pub fn persist_supervisor_event(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    event: &SupervisorLiveEventPayload,
) -> Result<Option<AutonomousRunSnapshotRecord>, CommandError> {
    let runtime_snapshot =
        match project_store::load_runtime_run(repo_root, project_id, agent_session_id)? {
            Some(snapshot) => snapshot,
            None => return Ok(None),
        };
    let existing = project_store::load_autonomous_run(repo_root, project_id, agent_session_id)?;
    if let Some(snapshot) = existing.as_ref() {
        if snapshot.run.run_id != runtime_snapshot.run.run_id {
            return Err(CommandError::retryable(
                "autonomous_live_event_run_mismatch",
                format!(
                    "Xero refused to persist live supervisor event state because the durable autonomous run `{}` does not match active runtime run `{}` for project `{project_id}`.",
                    snapshot.run.run_id, runtime_snapshot.run.run_id,
                ),
            ));
        }
    }

    let mut payload = reconcile_runtime_snapshot(
        existing.as_ref(),
        &runtime_snapshot,
        AutonomousRuntimeReconcileIntent::Observe,
    );

    if let SupervisorLiveEventPayload::ActionRequired {
        boundary_id,
        title,
        detail,
        ..
    } = event
    {
        let timestamp = now_timestamp();
        payload.run.status = AutonomousRunStatus::Paused;
        payload.run.paused_at = Some(timestamp.clone());
        payload.run.pause_reason = Some(RuntimeRunDiagnosticRecord {
            code: "autonomous_operator_action_required".into(),
            message: format!("{title}: {detail}"),
        });
        payload.run.updated_at = timestamp;
        payload.run.last_checkpoint_at = Some(boundary_id.clone());
    }

    persist_autonomous_run_scaffold(repo_root, payload).map(Some)
}
