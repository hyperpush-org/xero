use std::{collections::HashSet, path::Path};

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{CommandResult, ListProjectsResponseDto, HARNESS_FIXTURE_PROJECT_ID},
    db, registry,
    state::DesktopState,
};

pub(crate) fn load_projects_from_registry(
    registry_path: &Path,
) -> CommandResult<ListProjectsResponseDto> {
    db::configure_project_database_paths(registry_path);
    let registry_projects = registry::read_project_summaries(registry_path)?;

    let mut projects = Vec::new();
    let mut seen_project_ids = HashSet::new();
    let mut seen_root_paths = HashSet::new();
    let mut live_root_records = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry_projects {
        if record.project.id == HARNESS_FIXTURE_PROJECT_ID {
            live_root_records.push(record.registry.clone());
            continue;
        }

        if !Path::new(&record.registry.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        live_root_records.push(record.registry.clone());

        if seen_project_ids.insert(record.project.id.clone())
            && seen_root_paths.insert(record.registry.root_path)
        {
            projects.push(record.project);
        }
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(registry_path, live_root_records);
    }

    Ok(ListProjectsResponseDto { projects })
}

#[tauri::command]
pub fn list_projects<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<ListProjectsResponseDto> {
    let registry_path = state.global_db_path(&app)?;
    load_projects_from_registry(&registry_path)
}
