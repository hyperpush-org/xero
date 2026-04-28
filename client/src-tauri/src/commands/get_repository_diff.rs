use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, RepositoryDiffRequestDto, RepositoryDiffResponseDto,
    },
    git::diff,
    state::DesktopState,
};

#[tauri::command]
pub fn get_repository_diff<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RepositoryDiffRequestDto,
) -> CommandResult<RepositoryDiffResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.global_db_path(&app)?;
    diff::load_repository_diff(&request.project_id, request.scope, &registry_path)
}
