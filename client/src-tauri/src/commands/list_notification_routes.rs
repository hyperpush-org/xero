use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        map_notification_route_credential_readiness, map_notification_route_record,
        validate_non_empty, CommandResult, ListNotificationRoutesRequestDto,
        ListNotificationRoutesResponseDto, NotificationRouteKindDto,
    },
    db::project_store,
    notifications::{FileNotificationCredentialStore, NotificationRouteKind},
    state::DesktopState,
};

use super::runtime_support::resolve_project_root;

#[tauri::command]
pub fn list_notification_routes<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListNotificationRoutesRequestDto,
) -> CommandResult<ListNotificationRoutesResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let routes = project_store::load_notification_routes(&repo_root, &request.project_id)?;
    let credential_store_path = state.global_db_path(&app)?;
    let credential_store = FileNotificationCredentialStore::new(credential_store_path);
    let readiness_projector = credential_store.load_readiness_projector();

    Ok(ListNotificationRoutesResponseDto {
        routes: routes
            .into_iter()
            .map(|route| {
                let mut mapped_route = map_notification_route_record(route, None)?;
                let route_kind = map_notification_route_kind(&mapped_route.route_kind);
                let readiness = readiness_projector.project_route(
                    &mapped_route.project_id,
                    &mapped_route.route_id,
                    route_kind,
                );
                mapped_route.credential_readiness =
                    Some(map_notification_route_credential_readiness(readiness));
                Ok(mapped_route)
            })
            .collect::<CommandResult<Vec<_>>>()?,
    })
}

fn map_notification_route_kind(value: &NotificationRouteKindDto) -> NotificationRouteKind {
    match value {
        NotificationRouteKindDto::Telegram => NotificationRouteKind::Telegram,
        NotificationRouteKindDto::Discord => NotificationRouteKind::Discord,
    }
}
