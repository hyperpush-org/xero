use std::{collections::HashSet, path::Path};

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{CommandResult, ListProjectsResponseDto},
    db::project_store,
    registry,
    state::DesktopState,
};

pub(crate) fn load_projects_from_registry(
    registry_path: &Path,
) -> CommandResult<ListProjectsResponseDto> {
    let registry = registry::read_registry(registry_path)?;

    let mut projects = Vec::new();
    let mut seen_project_ids = HashSet::new();
    let mut seen_root_paths = HashSet::new();
    let mut live_root_records = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry.projects {
        if !Path::new(&record.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        live_root_records.push(record.clone());

        match project_store::load_project_summary(Path::new(&record.root_path), &record.project_id)
        {
            Ok(project) => {
                if seen_project_ids.insert(project.id.clone())
                    && seen_root_paths.insert(record.root_path.clone())
                {
                    projects.push(project);
                }
            }
            Err(_error) => {
                // Keep startup alive for other projects. Direct snapshot requests will surface
                // typed errors tied to the affected registry root.
            }
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
    let registry_path = state.registry_file(&app)?;
    load_projects_from_registry(&registry_path)
}
