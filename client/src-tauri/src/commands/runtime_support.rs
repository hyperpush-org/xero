use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use rand::RngCore;
use tauri::{AppHandle, Emitter, Runtime};

use crate::{
    auth::{AuthDiagnostic, AuthFlowError, OPENAI_CODEX_PROVIDER_ID},
    commands::{
        CommandError, CommandErrorClass, CommandResult, ProjectUpdateReason,
        ProjectUpdatedPayloadDto, RuntimeDiagnosticDto, RuntimeRunCheckpointDto,
        RuntimeRunCheckpointKindDto, RuntimeRunDiagnosticDto, RuntimeRunDto, RuntimeRunStatusDto,
        RuntimeRunTransportDto, RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto,
        RuntimeSessionDto, RuntimeUpdatedPayloadDto, PROJECT_UPDATED_EVENT,
        RUNTIME_RUN_UPDATED_EVENT, RUNTIME_UPDATED_EVENT,
    },
    db::project_store::{
        self, RuntimeRunCheckpointKind, RuntimeRunSnapshotRecord, RuntimeRunStatus,
        RuntimeRunTransportLiveness, RuntimeSessionDiagnosticRecord, RuntimeSessionRecord,
    },
    registry::{self, RegistryProjectRecord},
    runtime::{probe_runtime_run, RuntimeSupervisorProbeRequest},
    state::DesktopState,
};

pub(crate) const OPENAI_RUNTIME_KIND: &str = OPENAI_CODEX_PROVIDER_ID;
pub(crate) const DEFAULT_RUNTIME_RUN_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
pub(crate) const DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT: Duration = Duration::from_millis(750);
pub(crate) const DEFAULT_RUNTIME_RUN_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(4);

pub(crate) fn resolve_project_root<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
) -> CommandResult<PathBuf> {
    let registry_path = state.registry_file(app)?;
    let registry = registry::read_registry(&registry_path)?;
    let mut live_root_records = Vec::new();
    let mut candidates = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry.projects {
        if !Path::new(&record.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        if record.project_id == project_id {
            candidates.push(record.clone());
        }
        live_root_records.push(record);
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(&registry_path, live_root_records);
    }

    if candidates.is_empty() {
        return Err(CommandError::project_not_found());
    }

    let mut first_error: Option<CommandError> = None;
    for RegistryProjectRecord {
        project_id,
        root_path,
        ..
    } in candidates
    {
        match project_store::load_project_summary(Path::new(&root_path), &project_id) {
            Ok(_) => return Ok(PathBuf::from(root_path)),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(CommandError::project_not_found))
}

pub(crate) fn default_runtime_session(project_id: &str) -> RuntimeSessionDto {
    RuntimeSessionDto {
        project_id: project_id.into(),
        runtime_kind: OPENAI_RUNTIME_KIND.into(),
        provider_id: OPENAI_CODEX_PROVIDER_ID.into(),
        flow_id: None,
        session_id: None,
        account_id: None,
        phase: crate::commands::RuntimeAuthPhase::Idle,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: None,
        last_error: None,
        updated_at: crate::auth::now_timestamp(),
    }
}

pub(crate) fn load_runtime_session_status(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<RuntimeSessionDto> {
    let stored = project_store::load_runtime_session(repo_root, project_id)?;
    Ok(runtime_session_from_record(
        state,
        project_id,
        stored.as_ref(),
    ))
}

pub(crate) fn persist_runtime_session(
    repo_root: &Path,
    runtime: &RuntimeSessionDto,
) -> CommandResult<RuntimeSessionDto> {
    let record = RuntimeSessionRecord {
        project_id: runtime.project_id.clone(),
        runtime_kind: runtime.runtime_kind.clone(),
        provider_id: runtime.provider_id.clone(),
        flow_id: runtime.flow_id.clone(),
        session_id: runtime.session_id.clone(),
        account_id: runtime.account_id.clone(),
        auth_phase: runtime.phase.clone(),
        last_error: runtime
            .last_error
            .as_ref()
            .map(|error| RuntimeSessionDiagnosticRecord {
                code: error.code.clone(),
                message: error.message.clone(),
                retryable: error.retryable,
            }),
        updated_at: runtime.updated_at.clone(),
    };
    let persisted = project_store::upsert_runtime_session(repo_root, &record)?;
    Ok(RuntimeSessionDto {
        project_id: persisted.project_id,
        runtime_kind: persisted.runtime_kind,
        provider_id: persisted.provider_id,
        flow_id: persisted.flow_id,
        session_id: persisted.session_id,
        account_id: persisted.account_id,
        phase: persisted.auth_phase,
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: persisted
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: persisted.last_error.map(runtime_diagnostic_from_record),
        updated_at: persisted.updated_at,
    })
}

pub(crate) fn runtime_session_from_record(
    state: &DesktopState,
    project_id: &str,
    stored: Option<&RuntimeSessionRecord>,
) -> RuntimeSessionDto {
    let Some(stored) = stored else {
        return default_runtime_session(project_id);
    };

    let active_flow = stored
        .flow_id
        .as_deref()
        .and_then(|flow_id| state.active_auth_flows().snapshot(flow_id));

    if let Some(flow) = active_flow {
        let last_error = flow.last_error.map(runtime_diagnostic_from_auth);
        return RuntimeSessionDto {
            project_id: stored.project_id.clone(),
            runtime_kind: stored.runtime_kind.clone(),
            provider_id: stored.provider_id.clone(),
            flow_id: Some(flow.flow_id),
            session_id: flow.session_id.or_else(|| stored.session_id.clone()),
            account_id: flow.account_id.or_else(|| stored.account_id.clone()),
            phase: flow.phase,
            callback_bound: Some(flow.callback_bound),
            authorization_url: Some(flow.authorization_url),
            redirect_uri: Some(flow.redirect_uri),
            last_error_code: last_error.as_ref().map(|error| error.code.clone()),
            last_error,
            updated_at: flow.updated_at,
        };
    }

    RuntimeSessionDto {
        project_id: stored.project_id.clone(),
        runtime_kind: stored.runtime_kind.clone(),
        provider_id: stored.provider_id.clone(),
        flow_id: stored.flow_id.clone(),
        session_id: stored.session_id.clone(),
        account_id: stored.account_id.clone(),
        phase: stored.auth_phase.clone(),
        callback_bound: None,
        authorization_url: None,
        redirect_uri: None,
        last_error_code: stored.last_error.as_ref().map(|error| error.code.clone()),
        last_error: stored
            .last_error
            .clone()
            .map(runtime_diagnostic_from_record),
        updated_at: stored.updated_at.clone(),
    }
}

pub(crate) fn emit_runtime_updated<R: Runtime>(
    app: &AppHandle<R>,
    runtime: &RuntimeSessionDto,
) -> CommandResult<()> {
    app.emit(
        RUNTIME_UPDATED_EVENT,
        RuntimeUpdatedPayloadDto {
            project_id: runtime.project_id.clone(),
            runtime_kind: runtime.runtime_kind.clone(),
            flow_id: runtime.flow_id.clone(),
            session_id: runtime.session_id.clone(),
            account_id: runtime.account_id.clone(),
            auth_phase: runtime.phase.clone(),
            last_error_code: runtime.last_error_code.clone(),
            last_error: runtime.last_error.clone(),
            updated_at: runtime.updated_at.clone(),
        },
    )
    .map_err(|error| {
        CommandError::retryable(
            "runtime_updated_emit_failed",
            format!(
                "Cadence updated runtime-session metadata but could not emit the runtime update event: {error}"
            ),
        )
    })
}

pub(crate) fn emit_project_updated<R: Runtime>(
    app: &AppHandle<R>,
    repo_root: &Path,
    project_id: &str,
    reason: ProjectUpdateReason,
) -> CommandResult<()> {
    let project = project_store::load_project_summary(repo_root, project_id)?;

    app.emit(
        PROJECT_UPDATED_EVENT,
        ProjectUpdatedPayloadDto { project, reason },
    )
    .map_err(|error| {
        CommandError::retryable(
            "project_updated_emit_failed",
            format!(
                "Cadence updated selected-project metadata but could not emit the project update event: {error}"
            ),
        )
    })
}

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
            .map(|error| RuntimeRunDiagnosticDto {
                code: error.code.clone(),
                message: error.message.clone(),
            }),
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

pub(crate) fn command_error_from_auth(error: AuthFlowError) -> CommandError {
    let class = if error.retryable {
        CommandErrorClass::Retryable
    } else {
        CommandErrorClass::UserFixable
    };

    CommandError::new(error.code, class, error.message, error.retryable)
}

pub(crate) fn runtime_diagnostic_from_auth(error: AuthDiagnostic) -> RuntimeDiagnosticDto {
    RuntimeDiagnosticDto {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}

fn runtime_diagnostic_from_record(error: RuntimeSessionDiagnosticRecord) -> RuntimeDiagnosticDto {
    RuntimeDiagnosticDto {
        code: error.code,
        message: error.message,
        retryable: error.retryable,
    }
}
