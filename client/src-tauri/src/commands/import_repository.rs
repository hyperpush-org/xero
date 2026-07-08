use std::path::Path;

use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, ImportRepositoryRequestDto, ImportRepositoryResponseDto,
        ProjectUpdateReason, ProjectUpdatedPayloadDto, RepositoryStatusChangedPayloadDto,
        PROJECT_UPDATED_EVENT, REPOSITORY_STATUS_CHANGED_EVENT,
    },
    db,
    git::{repository::resolve_repository, status},
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

#[tauri::command]
pub fn import_repository<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ImportRepositoryRequestDto,
) -> CommandResult<ImportRepositoryResponseDto> {
    validate_non_empty(&request.path, "path")?;

    let registry_path = state.global_db_path(&app)?;
    db::configure_project_database_paths(&registry_path);

    let (imported, repository_status) = match resolve_repository(&request.path) {
        Ok(repository) => (
            db::import_project(&repository, state.import_failpoints())?,
            repository.repository_status(),
        ),
        Err(error) if error.code == "git_repository_not_found" => {
            let imported = db::import_project_directory(
                Path::new(request.path.trim()),
                state.import_failpoints(),
            )?;
            let repository_status = status::empty_repository_status(imported.repository.clone());
            (imported, repository_status)
        }
        Err(error) => return Err(error),
    };

    let _registry_snapshot = registry::upsert_project(
        &registry_path,
        RegistryProjectRecord {
            project_id: imported.project.id.clone(),
            repository_id: imported.repository.id.clone(),
            root_path: imported.repository.root_path.clone(),
            is_git_repo: imported.repository.is_git_repo,
        },
        state.import_failpoints(),
    )?;

    let project_updated_payload = ProjectUpdatedPayloadDto {
        project: imported.project.clone(),
        reason: ProjectUpdateReason::Imported,
    };
    app.emit(PROJECT_UPDATED_EVENT, &project_updated_payload)
        .map_err(|error| {
            crate::commands::CommandError::retryable(
                "project_updated_emit_failed",
                format!(
                    "Xero imported the project but could not emit the project update event: {error}"
                ),
            )
        })?;

    let repository_status_payload = RepositoryStatusChangedPayloadDto {
        project_id: imported.project.id.clone(),
        repository_id: imported.repository.id.clone(),
        status: repository_status,
    };
    app.emit(REPOSITORY_STATUS_CHANGED_EVENT, &repository_status_payload)
        .map_err(|error| {
            crate::commands::CommandError::retryable(
                "repository_status_emit_failed",
                format!(
                    "Xero imported the project but could not emit the repository status event: {error}"
                ),
            )
        })?;

    crate::commands::remote_bridge::publish_remote_project_list_to_cloud(&app, state.inner());

    Ok(ImportRepositoryResponseDto {
        project: imported.project,
        repository: imported.repository,
    })
}
