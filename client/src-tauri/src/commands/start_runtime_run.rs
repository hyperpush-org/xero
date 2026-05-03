use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{validate_non_empty, CommandResult, RuntimeRunDto, StartRuntimeRunRequestDto},
    state::DesktopState,
};

use super::runtime_support::{launch_or_reconnect_runtime_run, runtime_run_dto_from_snapshot};

#[tauri::command]
pub fn start_runtime_run<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartRuntimeRunRequestDto,
) -> CommandResult<RuntimeRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;

    let outcome = launch_or_reconnect_runtime_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.agent_session_id,
        request.initial_controls,
        request.initial_prompt,
        request.initial_attachments,
    )?;

    Ok(runtime_run_dto_from_snapshot(&outcome.snapshot))
}
