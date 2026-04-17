use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        map_notification_dispatch_record, validate_non_empty, CommandError, CommandResult,
        ListNotificationDispatchesRequestDto, ListNotificationDispatchesResponseDto,
    },
    db::project_store,
    state::DesktopState,
};

use super::runtime_support::resolve_project_root;

#[tauri::command]
pub fn list_notification_dispatches<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListNotificationDispatchesRequestDto,
) -> CommandResult<ListNotificationDispatchesResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let action_id = normalize_optional_non_empty(request.action_id, "actionId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let dispatches = project_store::load_notification_dispatches(
        &repo_root,
        &request.project_id,
        action_id.as_deref(),
    )?;

    Ok(ListNotificationDispatchesResponseDto {
        dispatches: dispatches
            .into_iter()
            .map(map_notification_dispatch_record)
            .collect(),
    })
}

fn normalize_optional_non_empty(
    value: Option<String>,
    field: &'static str,
) -> CommandResult<Option<String>> {
    match value {
        Some(value) if value.trim().is_empty() => Err(CommandError::invalid_request(field)),
        Some(value) => Ok(Some(value.trim().to_string())),
        None => Ok(None),
    }
}
