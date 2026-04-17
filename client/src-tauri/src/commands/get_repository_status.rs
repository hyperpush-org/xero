use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, ProjectIdRequestDto, RepositoryStatusResponseDto,
    },
    git::status,
    state::DesktopState,
};

#[tauri::command]
pub fn get_repository_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<RepositoryStatusResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.registry_file(&app)?;
    status::load_repository_status(&request.project_id, &registry_path)
}
