use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{validate_non_empty, CommandResult, GetRuntimeRunRequestDto, RuntimeRunDto},
    state::DesktopState,
};

use super::runtime_support::{
    emit_runtime_run_updated_if_changed, load_persisted_runtime_run, load_runtime_run_status,
    resolve_project_root, runtime_run_dto_from_snapshot,
};

#[tauri::command]
pub fn get_runtime_run<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: GetRuntimeRunRequestDto,
) -> CommandResult<Option<RuntimeRunDto>> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let before = load_persisted_runtime_run(&repo_root, &request.project_id)?;
    let after = load_runtime_run_status(state.inner(), &repo_root, &request.project_id)?;
    emit_runtime_run_updated_if_changed(&app, &request.project_id, &before, &after)?;

    Ok(after.as_ref().map(runtime_run_dto_from_snapshot))
}
