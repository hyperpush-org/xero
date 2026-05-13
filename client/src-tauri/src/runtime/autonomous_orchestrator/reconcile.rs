use crate::{
    auth::now_timestamp,
    db::project_store::{
        AutonomousRunRecord, AutonomousRunSnapshotRecord, AutonomousRunStatus,
        AutonomousRunUpsertRecord, RuntimeRunDiagnosticRecord, RuntimeRunSnapshotRecord,
        RuntimeRunStatus,
    },
};

use super::AutonomousRuntimeReconcileIntent;

const AUTONOMOUS_DUPLICATE_START_REASON: &str = "Xero reused the already-active autonomous run for this project instead of launching a duplicate supervisor.";
const AUTONOMOUS_CANCEL_REASON_CODE: &str = "autonomous_run_cancelled";
const AUTONOMOUS_CANCEL_REASON_MESSAGE: &str =
    "Operator cancelled the autonomous run from the desktop shell.";

pub fn reconcile_runtime_snapshot(
    existing: Option<&AutonomousRunSnapshotRecord>,
    runtime_snapshot: &RuntimeRunSnapshotRecord,
    intent: AutonomousRuntimeReconcileIntent,
) -> AutonomousRunUpsertRecord {
    let is_same_run =
        existing.is_some_and(|existing| existing.run.run_id == runtime_snapshot.run.run_id);
    let existing_run = is_same_run.then(|| existing.expect("checked same-run autonomous snapshot"));

    let duplicate_start_detected =
        matches!(intent, AutonomousRuntimeReconcileIntent::DuplicateStart)
            || existing_run
                .map(|snapshot| snapshot.run.duplicate_start_detected)
                .unwrap_or(false);
    let duplicate_start_run_id =
        duplicate_start_detected.then(|| runtime_snapshot.run.run_id.clone());
    let duplicate_start_reason =
        duplicate_start_detected.then_some(AUTONOMOUS_DUPLICATE_START_REASON.to_string());

    let base_updated_at = if matches!(intent, AutonomousRuntimeReconcileIntent::DuplicateStart) {
        now_timestamp()
    } else {
        runtime_snapshot.run.updated_at.clone()
    };
    let last_error = runtime_snapshot.run.last_error.clone();

    let (status, cancelled_at, cancel_reason) = match runtime_snapshot.run.status {
        RuntimeRunStatus::Stopped
            if matches!(intent, AutonomousRuntimeReconcileIntent::CancelRequested) =>
        {
            (
                AutonomousRunStatus::Cancelled,
                runtime_snapshot
                    .run
                    .stopped_at
                    .clone()
                    .or_else(|| Some(now_timestamp())),
                Some(RuntimeRunDiagnosticRecord {
                    code: AUTONOMOUS_CANCEL_REASON_CODE.into(),
                    message: AUTONOMOUS_CANCEL_REASON_MESSAGE.into(),
                }),
            )
        }
        RuntimeRunStatus::Stopped => (
            existing_run
                .map(|snapshot| snapshot.run.status.clone())
                .filter(|status| {
                    matches!(
                        status,
                        AutonomousRunStatus::Cancelled | AutonomousRunStatus::Completed
                    )
                })
                .unwrap_or(AutonomousRunStatus::Stopped),
            existing_run.and_then(|snapshot| snapshot.run.cancelled_at.clone()),
            existing_run.and_then(|snapshot| snapshot.run.cancel_reason.clone()),
        ),
        RuntimeRunStatus::Starting => (AutonomousRunStatus::Starting, None, None),
        RuntimeRunStatus::Running => (AutonomousRunStatus::Running, None, None),
        RuntimeRunStatus::Stale => (AutonomousRunStatus::Stale, None, None),
        RuntimeRunStatus::Failed => (AutonomousRunStatus::Failed, None, None),
    };

    let (crashed_at, crash_reason) = match runtime_snapshot.run.status {
        RuntimeRunStatus::Stale | RuntimeRunStatus::Failed => (
            existing_run
                .and_then(|snapshot| snapshot.run.crashed_at.clone())
                .or_else(|| Some(runtime_snapshot.run.updated_at.clone())),
            last_error.clone(),
        ),
        _ => (None, None),
    };

    let completed_at = existing_run
        .and_then(|snapshot| snapshot.run.completed_at.clone())
        .filter(|_| matches!(status, AutonomousRunStatus::Completed));

    AutonomousRunUpsertRecord {
        run: AutonomousRunRecord {
            project_id: runtime_snapshot.run.project_id.clone(),
            agent_session_id: runtime_snapshot.run.agent_session_id.clone(),
            run_id: runtime_snapshot.run.run_id.clone(),
            runtime_kind: runtime_snapshot.run.runtime_kind.clone(),
            provider_id: runtime_snapshot.run.provider_id.clone(),
            supervisor_kind: runtime_snapshot.run.supervisor_kind.clone(),
            status,
            duplicate_start_detected,
            duplicate_start_run_id,
            duplicate_start_reason,
            started_at: runtime_snapshot.run.started_at.clone(),
            last_heartbeat_at: runtime_snapshot.run.last_heartbeat_at.clone(),
            last_checkpoint_at: runtime_snapshot.last_checkpoint_at.clone(),
            paused_at: None,
            cancelled_at,
            completed_at,
            crashed_at,
            stopped_at: runtime_snapshot.run.stopped_at.clone(),
            pause_reason: None,
            cancel_reason,
            crash_reason,
            last_error,
            updated_at: base_updated_at,
        },
    }
}
