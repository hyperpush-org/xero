use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, RuntimeAuthPhase, RuntimeRunDto,
        StartRuntimeRunRequestDto,
    },
    runtime::{
        launch_detached_runtime_supervisor, resolve_runtime_shell_selection,
        RuntimeSupervisorLaunchRequest,
    },
    state::DesktopState,
};

use super::{
    get_runtime_session::reconcile_runtime_session,
    runtime_support::{
        emit_runtime_run_updated, emit_runtime_run_updated_if_changed, generate_runtime_run_id,
        load_persisted_runtime_run, load_runtime_run_status, load_runtime_session_status,
        resolve_project_root, runtime_run_dto_from_snapshot, DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        DEFAULT_RUNTIME_RUN_STARTUP_TIMEOUT, OPENAI_RUNTIME_KIND,
    },
};

#[tauri::command]
pub fn start_runtime_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartRuntimeRunRequestDto,
) -> CommandResult<RuntimeRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let before = load_persisted_runtime_run(&repo_root, &request.project_id)?;
    let current = load_runtime_run_status(state.inner(), &repo_root, &request.project_id)?;
    emit_runtime_run_updated_if_changed(&app, &request.project_id, &before, &current)?;

    if let Some(existing) = current
        .as_ref()
        .filter(|snapshot| is_reconnectable_runtime_run(snapshot))
    {
        return Ok(runtime_run_dto_from_snapshot(existing));
    }

    let runtime = load_runtime_session_status(state.inner(), &repo_root, &request.project_id)?;
    let runtime = reconcile_runtime_session(&app, state.inner(), &repo_root, runtime)?;
    ensure_runtime_run_auth_ready(&runtime.phase)?;
    let session_id = runtime.session_id.clone().ok_or_else(|| {
        CommandError::retryable(
            "runtime_run_session_missing",
            "Cadence cannot start a supervised runtime run until the selected project's authenticated runtime session exposes a stable session id.",
        )
    })?;
    let flow_id = runtime.flow_id.clone();

    let shell = resolve_runtime_shell_selection();

    let launched = launch_detached_runtime_supervisor(
        state.inner(),
        RuntimeSupervisorLaunchRequest {
            project_id: request.project_id,
            repo_root,
            runtime_kind: OPENAI_RUNTIME_KIND.into(),
            run_id: generate_runtime_run_id(),
            session_id,
            flow_id,
            program: shell.program,
            args: shell.args,
            startup_timeout: DEFAULT_RUNTIME_RUN_STARTUP_TIMEOUT,
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
            supervisor_binary: state.inner().runtime_supervisor_binary_override().cloned(),
        },
    )?;

    let runtime_run = runtime_run_dto_from_snapshot(&launched);
    emit_runtime_run_updated(&app, Some(&runtime_run))?;
    Ok(runtime_run)
}

fn is_reconnectable_runtime_run(
    snapshot: &crate::db::project_store::RuntimeRunSnapshotRecord,
) -> bool {
    matches!(
        snapshot.run.status,
        crate::db::project_store::RuntimeRunStatus::Starting
            | crate::db::project_store::RuntimeRunStatus::Running
    ) && snapshot.run.transport.liveness
        == crate::db::project_store::RuntimeRunTransportLiveness::Reachable
}

fn ensure_runtime_run_auth_ready(phase: &RuntimeAuthPhase) -> CommandResult<()> {
    match phase {
        RuntimeAuthPhase::Authenticated => Ok(()),
        RuntimeAuthPhase::Starting
        | RuntimeAuthPhase::AwaitingBrowserCallback
        | RuntimeAuthPhase::AwaitingManualInput
        | RuntimeAuthPhase::ExchangingCode
        | RuntimeAuthPhase::Refreshing => Err(CommandError::retryable(
            "runtime_run_auth_in_progress",
            "Cadence cannot start a supervised runtime run until the selected project's authenticated runtime session finishes its auth transition.",
        )),
        RuntimeAuthPhase::Idle | RuntimeAuthPhase::Cancelled | RuntimeAuthPhase::Failed => {
            Err(CommandError::user_fixable(
                "runtime_run_auth_required",
                "Cadence cannot start a supervised runtime run until the selected project has an authenticated runtime session.",
            ))
        }
    }
}
