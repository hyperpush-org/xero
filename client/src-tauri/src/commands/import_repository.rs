use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandResult, ImportRepositoryRequestDto, ImportRepositoryResponseDto,
        ProjectUpdateReason, ProjectUpdatedPayloadDto, RepositoryStatusChangedPayloadDto,
        PROJECT_UPDATED_EVENT, REPOSITORY_STATUS_CHANGED_EVENT,
    },
    db,
    git::repository::{ensure_cadence_excluded, resolve_repository},
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

    let repository = resolve_repository(&request.path)?;
    ensure_cadence_excluded(&repository, state.import_failpoints())?;

    let imported = db::import_project(&repository, state.import_failpoints())?;
    let registry_path = state.registry_file(&app)?;
    let _registry_snapshot = registry::upsert_project(
        &registry_path,
        RegistryProjectRecord {
            project_id: imported.project.id.clone(),
            repository_id: imported.repository.id.clone(),
            root_path: imported.repository.root_path.clone(),
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
                format!("Cadence imported the repo but could not emit the project update event: {error}"),
            )
        })?;

    let repository_status_payload = RepositoryStatusChangedPayloadDto {
        project_id: imported.project.id.clone(),
        repository_id: imported.repository.id.clone(),
        status: repository.repository_status(),
    };
    app.emit(REPOSITORY_STATUS_CHANGED_EVENT, &repository_status_payload)
        .map_err(|error| {
            crate::commands::CommandError::retryable(
                "repository_status_emit_failed",
                format!("Cadence imported the repo but could not emit the repository status event: {error}"),
            )
        })?;

    Ok(ImportRepositoryResponseDto {
        project: imported.project,
        repository: imported.repository,
    })
}
