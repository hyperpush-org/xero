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
pub fn git_stage_paths<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitPathsRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    operations::stage_paths(&request.project_id, &request.paths, &registry_path)?;
    emit_status_changed(&app, &request.project_id, &registry_path);
    Ok(())
}

#[tauri::command]
pub fn git_unstage_paths<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitPathsRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    operations::unstage_paths(&request.project_id, &request.paths, &registry_path)?;
    emit_status_changed(&app, &request.project_id, &registry_path);
    Ok(())
}

#[tauri::command]
pub fn git_discard_changes<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitPathsRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    operations::discard_changes(&request.project_id, &request.paths, &registry_path)?;
    emit_status_changed(&app, &request.project_id, &registry_path);
    Ok(())
}

#[tauri::command]
pub fn git_commit<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitCommitRequestDto,
) -> CommandResult<GitCommitResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let response = operations::commit(&request.project_id, &request.message, &registry_path)?;
    emit_status_changed(&app, &request.project_id, &registry_path);
    Ok(response)
}

#[tauri::command]
pub fn git_fetch<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitRemoteRequestDto,
) -> CommandResult<GitFetchResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    operations::fetch(
        &request.project_id,
        request.remote.as_deref(),
        &registry_path,
    )
}

#[tauri::command]
pub fn git_pull<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitRemoteRequestDto,
) -> CommandResult<GitPullResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    let response = operations::pull(
        &request.project_id,
        request.remote.as_deref(),
        &registry_path,
    )?;
    if response.updated {
        emit_status_changed(&app, &request.project_id, &registry_path);
    }
    Ok(response)
}

#[tauri::command]
pub fn git_push<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GitRemoteRequestDto,
) -> CommandResult<GitPushResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let registry_path = state.global_db_path(&app)?;
    operations::push(
        &request.project_id,
        request.remote.as_deref(),
        &registry_path,
    )
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
