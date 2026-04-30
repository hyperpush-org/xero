use std::path::Path;

use crate::{
    commands::{
        AutonomousRunDto, AutonomousRunRecoveryStateDto, AutonomousRunStateDto,
        AutonomousRunStatusDto, CommandError, CommandErrorClass, CommandResult,
    },
    db::project_store::{
        self, AutonomousRunRecord, AutonomousRunSnapshotRecord, AutonomousRunStatus,
        AutonomousRunUpsertRecord, RuntimeRunDiagnosticRecord, RuntimeRunSnapshotRecord,
        RuntimeRunStatus,
    },
};

use super::run::{runtime_reason_dto, runtime_run_diagnostic_dto};

pub(crate) fn load_persisted_autonomous_run(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<Option<AutonomousRunSnapshotRecord>> {
    project_store::load_autonomous_run(repo_root, project_id, agent_session_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AutonomousSyncIntent {
    Observe,
    DuplicateStart,
    CancelRequested,
}

pub(crate) fn sync_autonomous_run_state(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    runtime_snapshot: Option<&RuntimeRunSnapshotRecord>,
    intent: AutonomousSyncIntent,
) -> CommandResult<AutonomousRunStateDto> {
    let existing = load_persisted_autonomous_run(repo_root, project_id, agent_session_id)?;

    let persisted = match runtime_snapshot {
        Some(snapshot) => {
            let payload = durable_run_payload(existing.as_ref(), snapshot, intent);
            match project_store::upsert_autonomous_run(repo_root, &payload) {
                Ok(persisted) => persisted,
                Err(error) if should_fallback_autonomous_sync(&error, intent) => {
                    return Ok(autonomous_run_state_from_transient_payload(&payload));
                }
                Err(error) => return Err(error),
            }
        }
        None => {
            if let Some(existing) = existing {
                existing
            } else {
                return Ok(AutonomousRunStateDto { run: None });
            }
        }
    };

    Ok(autonomous_run_state_from_snapshot(Some(&persisted)))
}

pub(crate) fn autonomous_run_state_from_snapshot(
    snapshot: Option<&AutonomousRunSnapshotRecord>,
) -> AutonomousRunStateDto {
    AutonomousRunStateDto {
        run: snapshot.map(autonomous_run_dto_from_snapshot),
    }
}

fn should_fallback_autonomous_sync(error: &CommandError, intent: AutonomousSyncIntent) -> bool {
    matches!(
        intent,
        AutonomousSyncIntent::Observe | AutonomousSyncIntent::DuplicateStart
    ) && matches!(error.class, CommandErrorClass::Retryable)
        && matches!(
            error.code.as_str(),
            "autonomous_run_transaction_failed"
                | "autonomous_run_persist_failed"
                | "autonomous_run_commit_failed"
        )
}

fn autonomous_run_state_from_transient_payload(
    payload: &AutonomousRunUpsertRecord,
) -> AutonomousRunStateDto {
    let snapshot = AutonomousRunSnapshotRecord {
        run: payload.run.clone(),
    };
    autonomous_run_state_from_snapshot(Some(&snapshot))
}

fn durable_run_payload(
    existing: Option<&AutonomousRunSnapshotRecord>,
    runtime_snapshot: &RuntimeRunSnapshotRecord,
    intent: AutonomousSyncIntent,
) -> AutonomousRunUpsertRecord {
    let same_run =
        existing.is_some_and(|existing| existing.run.run_id == runtime_snapshot.run.run_id);
    let existing_run = same_run.then(|| existing.expect("checked same-run autonomous snapshot"));

    let duplicate_start_detected = matches!(intent, AutonomousSyncIntent::DuplicateStart)
        || existing_run
            .map(|snapshot| snapshot.run.duplicate_start_detected)
            .unwrap_or(false);
    let duplicate_start_run_id =
        duplicate_start_detected.then(|| runtime_snapshot.run.run_id.clone());
    let duplicate_start_reason = duplicate_start_detected.then_some(
        "Xero reused the already-active autonomous run for this project instead of launching a duplicate supervisor."
            .to_string(),
    );

    let base_updated_at = if matches!(intent, AutonomousSyncIntent::DuplicateStart) {
        crate::auth::now_timestamp()
    } else {
        runtime_snapshot.run.updated_at.clone()
    };
    let last_error = runtime_snapshot.run.last_error.clone();

    let (status, cancelled_at, cancel_reason) = match runtime_snapshot.run.status {
        RuntimeRunStatus::Stopped if matches!(intent, AutonomousSyncIntent::CancelRequested) => (
            AutonomousRunStatus::Cancelled,
            runtime_snapshot
                .run
                .stopped_at
                .clone()
                .or_else(|| Some(crate::auth::now_timestamp())),
            Some(RuntimeRunDiagnosticRecord {
                code: "autonomous_run_cancelled".into(),
                message: "Operator cancelled the autonomous run from the desktop shell.".into(),
            }),
        ),
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
            runtime_kind: autonomous_runtime_kind(runtime_snapshot),
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

fn autonomous_runtime_kind(runtime_snapshot: &RuntimeRunSnapshotRecord) -> String {
    if runtime_snapshot.run.runtime_kind == crate::runtime::OWNED_AGENT_RUNTIME_KIND {
        return crate::runtime::resolve_runtime_provider_identity(
            Some(runtime_snapshot.run.provider_id.as_str()),
            None,
        )
        .map(|provider| provider.runtime_kind.to_string())
        .unwrap_or_else(|_| runtime_snapshot.run.runtime_kind.clone());
    }

    runtime_snapshot.run.runtime_kind.clone()
}

fn autonomous_run_dto_from_snapshot(snapshot: &AutonomousRunSnapshotRecord) -> AutonomousRunDto {
    AutonomousRunDto {
        project_id: snapshot.run.project_id.clone(),
        agent_session_id: snapshot.run.agent_session_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        runtime_kind: snapshot.run.runtime_kind.clone(),
        provider_id: snapshot.run.provider_id.clone(),
        supervisor_kind: snapshot.run.supervisor_kind.clone(),
        status: autonomous_run_status_dto(&snapshot.run.status),
        recovery_state: autonomous_run_recovery_state_dto(&snapshot.run.status),
        duplicate_start_detected: snapshot.run.duplicate_start_detected,
        duplicate_start_run_id: snapshot.run.duplicate_start_run_id.clone(),
        duplicate_start_reason: snapshot.run.duplicate_start_reason.clone(),
        started_at: snapshot.run.started_at.clone(),
        last_heartbeat_at: snapshot.run.last_heartbeat_at.clone(),
        last_checkpoint_at: snapshot.run.last_checkpoint_at.clone(),
        paused_at: snapshot.run.paused_at.clone(),
        cancelled_at: snapshot.run.cancelled_at.clone(),
        completed_at: snapshot.run.completed_at.clone(),
        crashed_at: snapshot.run.crashed_at.clone(),
        stopped_at: snapshot.run.stopped_at.clone(),
        pause_reason: snapshot.run.pause_reason.as_ref().map(runtime_reason_dto),
        cancel_reason: snapshot.run.cancel_reason.as_ref().map(runtime_reason_dto),
        crash_reason: snapshot.run.crash_reason.as_ref().map(runtime_reason_dto),
        last_error_code: snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: snapshot
            .run
            .last_error
            .as_ref()
            .map(runtime_run_diagnostic_dto),
        updated_at: snapshot.run.updated_at.clone(),
    }
}

fn autonomous_run_status_dto(status: &AutonomousRunStatus) -> AutonomousRunStatusDto {
    match status {
        AutonomousRunStatus::Starting => AutonomousRunStatusDto::Starting,
        AutonomousRunStatus::Running => AutonomousRunStatusDto::Running,
        AutonomousRunStatus::Paused => AutonomousRunStatusDto::Paused,
        AutonomousRunStatus::Cancelling => AutonomousRunStatusDto::Cancelling,
        AutonomousRunStatus::Cancelled => AutonomousRunStatusDto::Cancelled,
        AutonomousRunStatus::Stale => AutonomousRunStatusDto::Stale,
        AutonomousRunStatus::Failed => AutonomousRunStatusDto::Failed,
        AutonomousRunStatus::Stopped => AutonomousRunStatusDto::Stopped,
        AutonomousRunStatus::Crashed => AutonomousRunStatusDto::Crashed,
        AutonomousRunStatus::Completed => AutonomousRunStatusDto::Completed,
    }
}

fn autonomous_run_recovery_state_dto(
    status: &AutonomousRunStatus,
) -> AutonomousRunRecoveryStateDto {
    match status {
        AutonomousRunStatus::Starting
        | AutonomousRunStatus::Running
        | AutonomousRunStatus::Paused => AutonomousRunRecoveryStateDto::Healthy,
        AutonomousRunStatus::Cancelling | AutonomousRunStatus::Stale => {
            AutonomousRunRecoveryStateDto::RecoveryRequired
        }
        AutonomousRunStatus::Cancelled
        | AutonomousRunStatus::Stopped
        | AutonomousRunStatus::Completed => AutonomousRunRecoveryStateDto::Terminal,
        AutonomousRunStatus::Failed | AutonomousRunStatus::Crashed => {
            AutonomousRunRecoveryStateDto::Failed
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::RuntimeRunApprovalModeDto;

    fn owned_agent_runtime_snapshot() -> RuntimeRunSnapshotRecord {
        RuntimeRunSnapshotRecord {
            run: project_store::RuntimeRunRecord {
                project_id: "project-1".into(),
                agent_session_id: project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: "run-1".into(),
                runtime_kind: crate::runtime::OWNED_AGENT_RUNTIME_KIND.into(),
                provider_id: crate::runtime::OPENAI_CODEX_PROVIDER_ID.into(),
                supervisor_kind: crate::runtime::OWNED_AGENT_SUPERVISOR_KIND.into(),
                status: RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "internal".into(),
                    endpoint: "xero://owned-agent".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: "2026-04-29T00:00:00Z".into(),
                last_heartbeat_at: Some("2026-04-29T00:00:01Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-29T00:00:01Z".into(),
            },
            controls: project_store::build_runtime_run_control_state(
                "gpt-5.4",
                None,
                RuntimeRunApprovalModeDto::Suggest,
                "2026-04-29T00:00:00Z",
                None,
            )
            .expect("build runtime controls"),
            checkpoints: Vec::new(),
            last_checkpoint_sequence: 0,
            last_checkpoint_at: None,
        }
    }

    #[test]
    fn autonomous_projection_uses_provider_runtime_kind_for_owned_agent_rows() {
        let snapshot = owned_agent_runtime_snapshot();
        let payload = durable_run_payload(None, &snapshot, AutonomousSyncIntent::Observe);

        assert_eq!(
            payload.run.runtime_kind,
            crate::runtime::OPENAI_CODEX_PROVIDER_ID
        );
        assert_eq!(
            payload.run.provider_id,
            crate::runtime::OPENAI_CODEX_PROVIDER_ID
        );
        assert_eq!(
            payload.run.supervisor_kind,
            crate::runtime::OWNED_AGENT_SUPERVISOR_KIND
        );
    }
}
