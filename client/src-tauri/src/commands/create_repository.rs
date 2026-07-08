use std::{fs, path::PathBuf};

use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, CreateRepositoryRequestDto,
        ImportRepositoryResponseDto, ProjectUpdateReason, ProjectUpdatedPayloadDto,
        RepositoryStatusChangedPayloadDto, PROJECT_UPDATED_EVENT, REPOSITORY_STATUS_CHANGED_EVENT,
    },
    db,
    git::status,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

#[tauri::command]
pub fn create_repository<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CreateRepositoryRequestDto,
) -> CommandResult<ImportRepositoryResponseDto> {
    validate_non_empty(&request.parent_path, "parentPath")?;
    validate_non_empty(&request.name, "name")?;

    let trimmed_name = request.name.trim();
    if trimmed_name.contains('/') || trimmed_name.contains('\\') {
        return Err(CommandError::user_fixable(
            "create_repository_invalid_name",
            "Project name cannot contain slashes.",
        ));
    }

    let parent_path = PathBuf::from(request.parent_path.trim());
    if !parent_path.is_dir() {
        return Err(CommandError::user_fixable(
            "create_repository_parent_missing",
            format!(
                "Xero cannot create a project inside `{}` because that folder does not exist.",
                parent_path.display()
            ),
        ));
    }

    let project_path = parent_path.join(trimmed_name);
    if project_path.exists() {
        return Err(CommandError::user_fixable(
            "create_repository_exists",
            format!(
                "A folder named `{}` already exists inside `{}`. Pick a different name.",
                trimmed_name,
                parent_path.display()
            ),
        ));
    }

    fs::create_dir(&project_path).map_err(|error| {
        CommandError::system_fault(
            "create_repository_create_dir_failed",
            format!(
                "Xero could not create the project folder at `{}`: {error}",
                project_path.display()
            ),
        )
    })?;

    let registry_path = state.global_db_path(&app)?;
    db::configure_project_database_paths(&registry_path);

    let imported = db::import_project_directory_with_origin(
        &project_path,
        db::ProjectOrigin::Greenfield,
        state.import_failpoints(),
    )?;
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
            CommandError::retryable(
                "project_updated_emit_failed",
                format!(
                    "Xero created the project but could not emit the project update event: {error}"
                ),
            )
        })?;

    let repository_status_payload = RepositoryStatusChangedPayloadDto {
        project_id: imported.project.id.clone(),
        repository_id: imported.repository.id.clone(),
        status: status::empty_repository_status(imported.repository.clone()),
    };
    app.emit(REPOSITORY_STATUS_CHANGED_EVENT, &repository_status_payload)
        .map_err(|error| {
            CommandError::retryable(
                "repository_status_emit_failed",
                format!(
                    "Xero created the project but could not emit the repository status event: {error}"
                ),
            )
        })?;

    crate::commands::remote_bridge::publish_remote_project_list_to_cloud(&app, state.inner());

    Ok(ImportRepositoryResponseDto {
        project: imported.project,
        repository: imported.repository,
    })
}
