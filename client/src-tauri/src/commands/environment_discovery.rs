use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{CommandResult, EnvironmentDiscoveryStatus},
    environment::service,
    state::DesktopState,
};

#[tauri::command]
pub fn get_environment_discovery_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<EnvironmentDiscoveryStatus> {
    service::environment_discovery_status(&state.global_db_path(&app)?)
}

#[tauri::command]
pub fn start_environment_discovery<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<EnvironmentDiscoveryStatus> {
    service::start_environment_discovery(state.global_db_path(&app)?)
}
