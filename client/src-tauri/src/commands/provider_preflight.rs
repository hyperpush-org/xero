use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, PreflightProviderProfileRequestDto,
        ProviderPreflightSnapshotDto,
    },
    provider_preflight::run_selected_provider_preflight,
    state::DesktopState,
};

#[tauri::command]
pub fn preflight_provider_profile<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: PreflightProviderProfileRequestDto,
) -> CommandResult<ProviderPreflightSnapshotDto> {
    validate_non_empty(&request.profile_id, "profileId")?;
    run_selected_provider_preflight(
        &app,
        state.inner(),
        &request.profile_id,
        request.model_id.as_deref(),
        request.force_refresh,
        request.required_features,
    )
}
