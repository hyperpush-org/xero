use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        map_notification_dispatch_record,
        runtime_support::{emit_project_updated, resolve_project_root},
        validate_non_empty, CommandError, CommandResult, NotificationDispatchOutcomeStatusDto,
        ProjectUpdateReason, RecordNotificationDispatchOutcomeRequestDto,
        RecordNotificationDispatchOutcomeResponseDto,
    },
    db::project_store::{self, NotificationDispatchOutcomeUpdateRecord},
    state::DesktopState,
};

#[tauri::command]
pub fn record_notification_dispatch_outcome<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RecordNotificationDispatchOutcomeRequestDto,
) -> CommandResult<RecordNotificationDispatchOutcomeResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.action_id, "actionId")?;
    validate_non_empty(&request.route_id, "routeId")?;
    validate_non_empty(&request.attempted_at, "attemptedAt")?;

    let error_code = normalize_optional_non_empty(request.error_code, "errorCode")?;
    let error_message = normalize_optional_non_empty(request.error_message, "errorMessage")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let dispatch = project_store::record_notification_dispatch_outcome(
        &repo_root,
        &NotificationDispatchOutcomeUpdateRecord {
            project_id: request.project_id.clone(),
            action_id: request.action_id,
            route_id: request.route_id,
            status: map_status(request.status),
            attempted_at: request.attempted_at,
            error_code,
            error_message,
        },
    )?;

    emit_project_updated(
        &app,
        &repo_root,
        &request.project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    Ok(RecordNotificationDispatchOutcomeResponseDto {
        dispatch: map_notification_dispatch_record(dispatch),
    })
}

fn map_status(
    status: NotificationDispatchOutcomeStatusDto,
) -> project_store::NotificationDispatchStatus {
    match status {
        NotificationDispatchOutcomeStatusDto::Sent => {
            project_store::NotificationDispatchStatus::Sent
        }
        NotificationDispatchOutcomeStatusDto::Failed => {
            project_store::NotificationDispatchStatus::Failed
        }
    }
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
