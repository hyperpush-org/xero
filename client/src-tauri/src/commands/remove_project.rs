use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{validate_non_empty, CommandResult, ListProjectsResponseDto, ProjectIdRequestDto},
    registry,
    state::DesktopState,
};

use super::list_projects::load_projects_from_registry;

#[tauri::command]
pub fn remove_project<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<ListProjectsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.registry_file(&app)?;
    let registry_snapshot = registry::read_registry(&registry_path)?;
    let original_count = registry_snapshot.projects.len();
    let remaining_projects = registry_snapshot
        .projects
        .into_iter()
        .filter(|record| record.project_id != request.project_id)
        .collect::<Vec<_>>();

    if remaining_projects.len() != original_count {
        registry::replace_projects(&registry_path, remaining_projects)?;
    }

    load_projects_from_registry(&registry_path)
}
