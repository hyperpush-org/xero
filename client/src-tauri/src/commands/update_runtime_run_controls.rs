use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, RuntimeRunDto,
        UpdateRuntimeRunControlsRequestDto,
    },
    runtime::{
        update_runtime_run_controls as update_supervised_runtime_run_controls,
        RuntimeSupervisorUpdateControlsRequest,
    },
    state::DesktopState,
};

use super::runtime_support::{
    emit_runtime_run_updated_if_changed, load_persisted_runtime_run,
    normalize_requested_runtime_run_controls, resolve_project_root, runtime_run_dto_from_snapshot,
    DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
};

#[tauri::command]
pub fn update_runtime_run_controls<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateRuntimeRunControlsRequestDto,
) -> CommandResult<RuntimeRunDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let before = load_persisted_runtime_run(&repo_root, &request.project_id)?;
    let Some(existing) = before.as_ref() else {
        return Err(CommandError::retryable(
            "runtime_run_missing",
            format!(
                "Cadence cannot queue runtime-run controls because project `{}` has no durable runtime run.",
                request.project_id
            ),
        ));
    };

    if existing.run.run_id != request.run_id {
        return Err(CommandError::user_fixable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to queue controls for run `{}` because project `{}` is currently bound to durable run `{}`.",
                request.run_id, request.project_id, existing.run.run_id
            ),
        ));
    }

    let normalized_controls = request
        .controls
        .as_ref()
        .map(|controls| normalize_requested_runtime_run_controls(&app, state.inner(), controls))
        .transpose()?;

    let after = update_supervised_runtime_run_controls(
        state.inner(),
        RuntimeSupervisorUpdateControlsRequest {
            project_id: request.project_id.clone(),
            repo_root,
            run_id: request.run_id.clone(),
            controls: normalized_controls,
            prompt: request.prompt.clone(),
            control_timeout: DEFAULT_RUNTIME_RUN_CONTROL_TIMEOUT,
        },
    )?;
    emit_runtime_run_updated_if_changed(&app, &request.project_id, &before, &Some(after.clone()))?;

    Ok(runtime_run_dto_from_snapshot(&after))
}
