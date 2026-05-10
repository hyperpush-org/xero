use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        project_usage_model_breakdown_dto, project_usage_totals_dto, validate_non_empty,
        CommandResult, ProjectIdRequestDto, ProjectUsageSummaryDto,
    },
    db::project_store,
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
    let repo_root = crate::runtime::resolve_imported_repo_root_from_registry(
        &registry_path,
        &request.project_id,
    )?;
    let (totals_record, breakdown_records) =
        project_store::project_usage_summary(&repo_root, &request.project_id)?;
    let totals = project_usage_totals_dto(totals_record);
    let by_model = breakdown_records
        .into_iter()
        .map(project_usage_model_breakdown_dto)
        .collect();

    Ok(ProjectUsageSummaryDto {
        project_id: request.project_id,
        totals,
        by_model,
    })
}
