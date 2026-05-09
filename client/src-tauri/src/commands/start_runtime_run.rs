use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{validate_non_empty, CommandResult, RuntimeRunDto, StartRuntimeRunRequestDto},
    state::DesktopState,
};

use super::runtime_support::{launch_or_reconnect_runtime_run, runtime_run_dto_from_snapshot};

#[tauri::command]
pub async fn start_runtime_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartRuntimeRunRequestDto,
) -> CommandResult<RuntimeRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;

    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || start_runtime_run_blocking(app, state, request))
        .await
        .map_err(|error| {
            crate::commands::CommandError::system_fault(
                "runtime_run_start_task_failed",
                format!("Xero could not finish background runtime-run start work: {error}"),
            )
        })?
}

fn start_runtime_run_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: StartRuntimeRunRequestDto,
) -> CommandResult<RuntimeRunDto> {
    let outcome = launch_or_reconnect_runtime_run(
        &app,
        &state,
        &request.project_id,
        &request.agent_session_id,
        request.initial_controls,
        request.initial_prompt,
        request.initial_attachments,
    )?;

    Ok(runtime_run_dto_from_snapshot(&outcome.snapshot))
}
