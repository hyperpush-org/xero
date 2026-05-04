use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, RepositoryDiffRequestDto, RepositoryDiffResponseDto,
    },
    git::diff,
    state::DesktopState,
};

#[tauri::command]
pub async fn get_repository_diff<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RepositoryDiffRequestDto,
) -> CommandResult<RepositoryDiffResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    let scope = request.scope;
    let key = format!("repository-diff:{project_id}:{scope:?}");
    drop(app);

    jobs.run_blocking_latest(key, "repository diff", move |cancellation| {
        cancellation.check_cancelled("repository diff")?;
        diff::load_repository_diff(&project_id, scope, &registry_path)
    })
    .await
}
