use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        map_notification_route_record, parse_notification_route_kind,
        runtime_support::{emit_project_updated, resolve_project_root},
        validate_non_empty, CommandError, CommandResult, ProjectUpdateReason,
        UpsertNotificationRouteRequestDto, UpsertNotificationRouteResponseDto,
    },
    db::project_store::{self, NotificationRouteUpsertRecord},
    state::DesktopState,
};

#[tauri::command]
pub fn upsert_notification_route<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertNotificationRouteRequestDto,
) -> CommandResult<UpsertNotificationRouteResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.route_id, "routeId")?;
    validate_non_empty(&request.route_target, "routeTarget")?;
    validate_non_empty(&request.updated_at, "updatedAt")?;

    let route_kind =
        parse_notification_route_kind(&request.route_kind, "notification_route_request_invalid")?;
    let metadata_json = normalize_optional_non_empty(request.metadata_json, "metadataJson")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let route = project_store::upsert_notification_route(
        &repo_root,
        &NotificationRouteUpsertRecord {
            project_id: request.project_id.clone(),
            route_id: request.route_id,
            route_kind: route_kind.as_str().to_string(),
            route_target: request.route_target,
            enabled: request.enabled,
            metadata_json,
            updated_at: request.updated_at,
        },
    )?;

    emit_project_updated(
        &app,
        &repo_root,
        &request.project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    Ok(UpsertNotificationRouteResponseDto {
        route: map_notification_route_record(route, None)?,
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
