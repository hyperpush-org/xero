use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, GitCommitRequestDto, GitCommitResponseDto,
        GitFetchResponseDto, GitPathsRequestDto, GitPullResponseDto, GitPushResponseDto,
        GitRemoteRequestDto, RepositoryStatusChangedPayloadDto, REPOSITORY_STATUS_CHANGED_EVENT,
    },
    git::{operations, status},
    state::DesktopState,
};

#[tauri::command]
pub async fn git_stage_paths<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitPathsRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();

    let project_id = request.project_id;
    let paths = request.paths;
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();
    jobs.run_blocking_project_lane(project_id.clone(), "git", "git stage", move || {
        operations::stage_paths(&operation_project_id, &paths, &operation_registry_path)
    })
    .await?;
    emit_status_changed(&app, &project_id, &registry_path);
    Ok(())
}

#[tauri::command]
pub async fn git_unstage_paths<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitPathsRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();

    let project_id = request.project_id;
    let paths = request.paths;
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();
    jobs.run_blocking_project_lane(project_id.clone(), "git", "git unstage", move || {
        operations::unstage_paths(&operation_project_id, &paths, &operation_registry_path)
    })
    .await?;
    emit_status_changed(&app, &project_id, &registry_path);
    Ok(())
}

#[tauri::command]
pub async fn git_discard_changes<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitPathsRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();

    let project_id = request.project_id;
    let paths = request.paths;
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();
    jobs.run_blocking_project_lane(project_id.clone(), "git", "git discard", move || {
        operations::discard_changes(&operation_project_id, &paths, &operation_registry_path)
    })
    .await?;
    emit_status_changed(&app, &project_id, &registry_path);
    Ok(())
}

#[tauri::command]
pub async fn git_commit<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitCommitRequestDto,
) -> CommandResult<GitCommitResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();

    let project_id = request.project_id;
    let message = request.message;
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();
    let response = jobs
        .run_blocking_project_lane(project_id.clone(), "git", "git commit", move || {
            operations::commit(&operation_project_id, &message, &operation_registry_path)
        })
        .await?;
    emit_status_changed(&app, &project_id, &registry_path);
    Ok(response)
}

#[tauri::command]
pub async fn git_fetch<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitRemoteRequestDto,
) -> CommandResult<GitFetchResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();

    let project_id = request.project_id;
    let remote = request.remote;
    jobs.run_blocking_project_lane(project_id.clone(), "git", "git fetch", move || {
        operations::fetch(&project_id, remote.as_deref(), &registry_path)
    })
    .await
}

#[tauri::command]
pub async fn git_pull<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitRemoteRequestDto,
) -> CommandResult<GitPullResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();

    let project_id = request.project_id;
    let remote = request.remote;
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();
    let response = jobs
        .run_blocking_project_lane(project_id.clone(), "git", "git pull", move || {
            operations::pull(
                &operation_project_id,
                remote.as_deref(),
                &operation_registry_path,
            )
        })
        .await?;
    if response.updated {
        emit_status_changed(&app, &project_id, &registry_path);
    }
    Ok(response)
}

#[tauri::command]
pub async fn git_push<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitRemoteRequestDto,
) -> CommandResult<GitPushResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();

    let project_id = request.project_id;
    let remote = request.remote;
    jobs.run_blocking_project_lane(project_id.clone(), "git", "git push", move || {
        operations::push(&project_id, remote.as_deref(), &registry_path)
    })
    .await
}

fn emit_status_changed<R: Runtime>(
    app: &AppHandle<R>,
    project_id: &str,
    registry_path: &std::path::Path,
) {
    let Ok(status) = status::load_repository_status(project_id, registry_path) else {
        return;
    };
    let payload = RepositoryStatusChangedPayloadDto {
        project_id: status.repository.project_id.clone(),
        repository_id: status.repository.id.clone(),
        status,
    };
    let _ = app.emit(REPOSITORY_STATUS_CHANGED_EVENT, &payload);
}
