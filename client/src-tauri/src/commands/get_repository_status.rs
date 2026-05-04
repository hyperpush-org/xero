use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, ProjectIdRequestDto, RepositoryStatusResponseDto,
    },
    git::status,
    state::DesktopState,
};

#[tauri::command]
pub async fn get_repository_status<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<RepositoryStatusResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    drop(app);

    jobs.run_blocking_latest(
        format!("repository-status:{project_id}"),
        "repository status",
        move |cancellation| {
            cancellation.check_cancelled("repository status")?;
            status::load_repository_status(&project_id, &registry_path)
        },
    )
    .await
}
