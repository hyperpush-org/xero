use std::path::Path;

use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        project_usage_model_breakdown_dto, project_usage_totals_dto, validate_non_empty,
        CommandError, CommandResult, ProjectIdRequestDto, ProjectUsageSummaryDto,
        ProjectUsageTotalsDto,
    },
    db,
    db::project_store,
    registry::{self, RegistryProjectRecord},
    state::DesktopState,
};

#[tauri::command]
pub fn get_project_usage_summary<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ProjectIdRequestDto,
) -> CommandResult<ProjectUsageSummaryDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let registry_path = state.global_db_path(&app)?;
    db::configure_project_database_paths(&registry_path);
    let registry = registry::read_registry(&registry_path)?;

    let mut snapshot_candidates = Vec::new();
    let mut live_root_records = Vec::new();
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
        let root = Path::new(&root_path);
        let totals_record = match project_store::project_usage_totals(root, &project_id) {
            Ok(record) => record,
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
                continue;
            }
        };
        let breakdown_records = match project_store::project_usage_breakdown(root, &project_id) {
            Ok(records) => records,
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
                continue;
            }
        };

        let totals = project_usage_totals_dto(totals_record);
        let by_model = breakdown_records
            .into_iter()
            .map(project_usage_model_breakdown_dto)
            .collect();

        return Ok(ProjectUsageSummaryDto {
            project_id,
            totals,
            by_model,
        });
    }

    if let Some(error) = first_error {
        return Err(error);
    }

    // Project exists in the registry but every candidate root failed to load —
    // surface an empty summary so the UI can still render rather than crash.
    Ok(ProjectUsageSummaryDto {
        project_id: request.project_id,
        totals: ProjectUsageTotalsDto::default(),
        by_model: Vec::new(),
    })
}
