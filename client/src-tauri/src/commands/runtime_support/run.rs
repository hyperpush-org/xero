use std::{path::Path, time::Duration};

use rand::RngCore;
use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    commands::{
        CommandError, CommandResult, RuntimeRunCheckpointDto, RuntimeRunCheckpointKindDto,
        RuntimeRunDiagnosticDto, RuntimeRunDto, RuntimeRunStatusDto, RuntimeRunTransportDto,
        RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto, RUNTIME_RUN_UPDATED_EVENT,
    },
    db::project_store::{
        self, RuntimeRunCheckpointKind, RuntimeRunDiagnosticRecord, RuntimeRunSnapshotRecord,
        RuntimeRunStatus, RuntimeRunTransportLiveness,
    },
    runtime::{
        probe_runtime_run, resolve_runtime_provider_identity, RuntimeSupervisorProbeRequest,
    },
    state::DesktopState,
};

pub(crate) const DEFAULT_RUNTIME_RUN_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT: Duration = Duration::from_millis(750);
pub(crate) const DEFAULT_RUNTIME_RUN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(4);

pub(crate) fn load_persisted_runtime_run(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    project_store::load_runtime_run(repo_root, project_id)
}

pub(crate) fn load_runtime_run_status(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<RuntimeRunSnapshotRecord>> {
    probe_runtime_run(
        state,
        RuntimeSupervisorProbeRequest {
            project_id: project_id.into(),
            repo_root: repo_root.to_path_buf(),
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
    )
}

pub(crate) fn runtime_run_dto_from_snapshot(snapshot: &RuntimeRunSnapshotRecord) -> RuntimeRunDto {
    RuntimeRunDto {
        project_id: snapshot.run.project_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        runtime_kind: snapshot.run.runtime_kind.clone(),
        provider_id: provider_id_from_runtime_kind(&snapshot.run.runtime_kind),
        supervisor_kind: snapshot.run.supervisor_kind.clone(),
        status: runtime_run_status_dto(snapshot.run.status.clone()),
        transport: RuntimeRunTransportDto {
            kind: snapshot.run.transport.kind.clone(),
            endpoint: snapshot.run.transport.endpoint.clone(),
            liveness: runtime_run_transport_liveness_dto(snapshot.run.transport.liveness.clone()),
        },
        started_at: snapshot.run.started_at.clone(),
        last_heartbeat_at: snapshot.run.last_heartbeat_at.clone(),
        last_checkpoint_sequence: snapshot.last_checkpoint_sequence,
        last_checkpoint_at: snapshot.last_checkpoint_at.clone(),
        stopped_at: snapshot.run.stopped_at.clone(),
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
        checkpoints: snapshot
            .checkpoints
            .iter()
            .map(|checkpoint| RuntimeRunCheckpointDto {
                sequence: checkpoint.sequence,
                kind: runtime_run_checkpoint_kind_dto(checkpoint.kind.clone()),
                summary: checkpoint.summary.clone(),
                created_at: checkpoint.created_at.clone(),
            })
            .collect(),
    }
}

pub(crate) fn emit_runtime_run_updated<R: Runtime>(
    app: &AppHandle<R>,
    runtime_run: Option<&RuntimeRunDto>,
) -> CommandResult<()> {
    let project_id = runtime_run
        .map(|runtime_run| runtime_run.project_id.clone())
        .unwrap_or_default();

    app.emit(
        RUNTIME_RUN_UPDATED_EVENT,
        RuntimeRunUpdatedPayloadDto {
            project_id,
            run: runtime_run.cloned(),
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_run_updated_emit_failed",
            format!(
                "Cadence updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
            ),
        )
    })
}

pub(crate) fn emit_runtime_run_updated_if_changed<R: Runtime>(
    app: &AppHandle<R>,
    project_id: &str,
    before: &Option<RuntimeRunSnapshotRecord>,
    after: &Option<RuntimeRunSnapshotRecord>,
) -> CommandResult<()> {
    if before == after {
        return Ok(());
    }

    let runtime_run = after.as_ref().map(runtime_run_dto_from_snapshot);
    if let Some(runtime_run) = runtime_run.as_ref() {
        return emit_runtime_run_updated(app, Some(runtime_run));
    }

    app.emit(
        RUNTIME_RUN_UPDATED_EVENT,
        RuntimeRunUpdatedPayloadDto {
            project_id: project_id.into(),
            run: None,
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_run_updated_emit_failed",
            format!(
                "Cadence updated durable runtime-run metadata but could not emit the runtime-run update event: {error}"
            ),
        )
    })
}

pub(crate) fn generate_runtime_run_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "run-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub(super) fn provider_id_from_runtime_kind(runtime_kind: &str) -> String {
    resolve_runtime_provider_identity(Some(runtime_kind), Some(runtime_kind))
        .map(|provider| provider.provider_id.to_string())
        .unwrap_or_else(|_| runtime_kind.trim().to_string())
}

pub(super) fn runtime_run_diagnostic_dto(
    reason: &RuntimeRunDiagnosticRecord,
) -> RuntimeRunDiagnosticDto {
    RuntimeRunDiagnosticDto {
        code: reason.code.clone(),
        message: reason.message.clone(),
    }
}

pub(super) fn runtime_reason_dto(
    reason: &RuntimeRunDiagnosticRecord,
) -> crate::commands::AutonomousLifecycleReasonDto {
    crate::commands::AutonomousLifecycleReasonDto {
        code: reason.code.clone(),
        message: reason.message.clone(),
    }
}

fn runtime_run_status_dto(status: RuntimeRunStatus) -> RuntimeRunStatusDto {
    match status {
        RuntimeRunStatus::Starting => RuntimeRunStatusDto::Starting,
        RuntimeRunStatus::Running => RuntimeRunStatusDto::Running,
        RuntimeRunStatus::Stale => RuntimeRunStatusDto::Stale,
        RuntimeRunStatus::Stopped => RuntimeRunStatusDto::Stopped,
        RuntimeRunStatus::Failed => RuntimeRunStatusDto::Failed,
    }
}

fn runtime_run_transport_liveness_dto(
    liveness: RuntimeRunTransportLiveness,
) -> RuntimeRunTransportLivenessDto {
    match liveness {
        RuntimeRunTransportLiveness::Unknown => RuntimeRunTransportLivenessDto::Unknown,
        RuntimeRunTransportLiveness::Reachable => RuntimeRunTransportLivenessDto::Reachable,
        RuntimeRunTransportLiveness::Unreachable => RuntimeRunTransportLivenessDto::Unreachable,
    }
}

fn runtime_run_checkpoint_kind_dto(kind: RuntimeRunCheckpointKind) -> RuntimeRunCheckpointKindDto {
    match kind {
        RuntimeRunCheckpointKind::Bootstrap => RuntimeRunCheckpointKindDto::Bootstrap,
        RuntimeRunCheckpointKind::State => RuntimeRunCheckpointKindDto::State,
        RuntimeRunCheckpointKind::Tool => RuntimeRunCheckpointKindDto::Tool,
        RuntimeRunCheckpointKind::ActionRequired => RuntimeRunCheckpointKindDto::ActionRequired,
        RuntimeRunCheckpointKind::Diagnostic => RuntimeRunCheckpointKindDto::Diagnostic,
    }
}
