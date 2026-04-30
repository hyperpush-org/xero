use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, AutonomousRunStateDto, CommandResult, StartAutonomousRunRequestDto,
    },
    state::DesktopState,
};

use super::runtime_support::{
    launch_or_reconnect_runtime_run, sync_autonomous_run_state, AutonomousSyncIntent,
};

#[tauri::command]
pub fn start_autonomous_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: StartAutonomousRunRequestDto,
) -> CommandResult<AutonomousRunStateDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;

    let outcome = launch_or_reconnect_runtime_run(
        &app,
        state.inner(),
        &request.project_id,
        &request.agent_session_id,
        request.initial_controls,
        request.initial_prompt,
    )?;

    sync_autonomous_run_state(
        &outcome.repo_root,
        &request.project_id,
        &request.agent_session_id,
        Some(&outcome.snapshot),
        if outcome.reconnected {
            AutonomousSyncIntent::DuplicateStart
        } else {
            AutonomousSyncIntent::Observe
        },
    )
}
