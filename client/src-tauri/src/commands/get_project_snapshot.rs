use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, ProjectIdRequestDto,
        ProjectSnapshotResponseDto,
    },
    db::project_store,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

#[tauri::command]
pub fn get_project_snapshot<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<ProjectSnapshotResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.registry_file(&app)?;
    let registry = registry::read_registry(&registry_path)?;
    let mut live_root_records = Vec::new();
    let mut snapshot_candidates = Vec::new();
    let mut pruned_stale_roots = false;

    for record in registry.projects {
        if !Path::new(&record.root_path).is_dir() {
            pruned_stale_roots = true;
            continue;
        }

        if record.project_id == request.project_id {
            snapshot_candidates.push(record.clone());
        }
        live_root_records.push(record);
    }

    if pruned_stale_roots {
        let _ = registry::replace_projects(&registry_path, live_root_records);
    }

    if snapshot_candidates.is_empty() {
        return Err(CommandError::project_not_found());
    }

    let mut first_error: Option<CommandError> = None;

    for RegistryProjectRecord {
        project_id,
        root_path,
        ..
    } in snapshot_candidates
    {
        match project_store::load_project_snapshot(Path::new(&root_path), &project_id) {
            Ok(record) => return Ok(record.snapshot),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    Err(first_error.unwrap_or_else(CommandError::project_not_found))
}
