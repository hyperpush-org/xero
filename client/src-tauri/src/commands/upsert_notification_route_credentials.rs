use tauri::{AppHandle, Runtime, State};
use url::Url;

use crate::{
    commands::{
        parse_notification_route_kind, validate_non_empty, CommandError, CommandResult,
        NotificationRouteCredentialPayloadDto, NotificationRouteKindDto,
        UpsertNotificationRouteCredentialsRequestDto,
        UpsertNotificationRouteCredentialsResponseDto,
    },
    db::project_store,
    notifications::{
        FileNotificationCredentialStore, NotificationAdapterError,
        NotificationCredentialUpsertInput, NotificationCredentialUpsertReceipt,
        NotificationRouteKind,
    },
    state::DesktopState,
};

use super::runtime_support::resolve_project_root;

const REQUEST_INVALID_CODE: &str = "notification_route_credentials_request_invalid";

#[tauri::command]
pub fn upsert_notification_route_credentials<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpsertNotificationRouteCredentialsRequestDto,
) -> CommandResult<UpsertNotificationRouteCredentialsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.route_id, "routeId")?;
    validate_non_empty(&request.route_kind, "routeKind")?;
    validate_non_empty(&request.updated_at, "updatedAt")?;

    let requested_route_kind = map_route_kind_dto(parse_notification_route_kind(
        &request.route_kind,
        REQUEST_INVALID_CODE,
    )?);
    let credentials = validate_credentials(requested_route_kind, request.credentials)?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let route = project_store::load_notification_routes(&repo_root, &request.project_id)?
        .into_iter()
        .find(|route| route.route_id == request.route_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "notification_route_not_found",
                format!(
                    "Cadence could not find notification route `{}` in project `{}`.",
                    request.route_id, request.project_id
                ),
            )
        })?;

    let persisted_route_kind = NotificationRouteKind::parse(route.route_kind.as_str()).map_err(|_| {
        CommandError::system_fault(
            "notification_route_decode_failed",
            format!(
                "Cadence found unsupported persisted notification route kind `{}` for route `{}` in project `{}`.",
                route.route_kind, request.route_id, request.project_id
            ),
        )
    })?;

    if persisted_route_kind != requested_route_kind {
        return Err(CommandError::user_fixable(
            REQUEST_INVALID_CODE,
            format!(
                "Cadence route `{}` in project `{}` is `{}` but credentials were submitted for `{}`.",
                request.route_id,
                request.project_id,
                persisted_route_kind.as_str(),
                requested_route_kind.as_str()
            ),
        ));
    }

    let credential_store_path = state.notification_credential_store_file(&app)?;
    let credential_store = FileNotificationCredentialStore::new(credential_store_path);
    let receipt = credential_store
        .upsert_route_credentials(
            &request.project_id,
            &request.route_id,
            requested_route_kind,
            credentials,
            &request.updated_at,
        )
        .map_err(command_error_from_adapter)?;

    Ok(map_upsert_receipt(receipt))
}

fn validate_credentials(
    route_kind: NotificationRouteKind,
    payload: NotificationRouteCredentialPayloadDto,
) -> CommandResult<NotificationCredentialUpsertInput> {
    let bot_token = normalize_optional_non_empty(payload.bot_token, "credentials.botToken")?;
    let chat_id = normalize_optional_non_empty(payload.chat_id, "credentials.chatId")?;
    let webhook_url = normalize_optional_non_empty(payload.webhook_url, "credentials.webhookUrl")?;

    match route_kind {
        NotificationRouteKind::Telegram => {
            if webhook_url.is_some() {
                return Err(CommandError::user_fixable(
                    REQUEST_INVALID_CODE,
                    "Telegram credentials must not include `credentials.webhookUrl`.",
                ));
            }

            let bot_token = bot_token.ok_or_else(|| {
                CommandError::user_fixable(
                    REQUEST_INVALID_CODE,
                    "Telegram credentials require non-empty `credentials.botToken`.",
                )
            })?;

            let chat_id = chat_id.ok_or_else(|| {
                CommandError::user_fixable(
                    REQUEST_INVALID_CODE,
                    "Telegram credentials require non-empty `credentials.chatId`.",
                )
            })?;

            Ok(NotificationCredentialUpsertInput::Telegram { bot_token, chat_id })
        }
        NotificationRouteKind::Discord => {
            if chat_id.is_some() {
                return Err(CommandError::user_fixable(
                    REQUEST_INVALID_CODE,
                    "Discord credentials must not include `credentials.chatId`.",
                ));
            }

            let webhook_url = webhook_url.ok_or_else(|| {
                CommandError::user_fixable(
                    REQUEST_INVALID_CODE,
                    "Discord credentials require non-empty `credentials.webhookUrl`.",
                )
            })?;

            validate_discord_webhook_url(&webhook_url)?;

            Ok(NotificationCredentialUpsertInput::Discord {
                webhook_url,
                bot_token,
            })
        }
    }
}

fn validate_discord_webhook_url(webhook_url: &str) -> CommandResult<()> {
    let parsed = Url::parse(webhook_url).map_err(|_| {
        CommandError::user_fixable(
            REQUEST_INVALID_CODE,
            "Field `credentials.webhookUrl` must be a valid URL for Discord routes.",
        )
    })?;

    if parsed.scheme() != "https" {
        return Err(CommandError::user_fixable(
            REQUEST_INVALID_CODE,
            "Field `credentials.webhookUrl` must use HTTPS for Discord routes.",
        ));
    }

    Ok(())
}

fn normalize_optional_non_empty(
    value: Option<String>,
    field: &'static str,
) -> CommandResult<Option<String>> {
    match value {
        Some(value) if value.trim().is_empty() => Err(CommandError::user_fixable(
            REQUEST_INVALID_CODE,
            format!("Field `{field}` must be a non-empty string when provided."),
        )),
        Some(value) => Ok(Some(value.trim().to_string())),
        None => Ok(None),
    }
}

fn map_route_kind_dto(value: NotificationRouteKindDto) -> NotificationRouteKind {
    match value {
        NotificationRouteKindDto::Telegram => NotificationRouteKind::Telegram,
        NotificationRouteKindDto::Discord => NotificationRouteKind::Discord,
    }
}

fn map_route_kind(value: NotificationRouteKind) -> NotificationRouteKindDto {
    match value {
        NotificationRouteKind::Telegram => NotificationRouteKindDto::Telegram,
        NotificationRouteKind::Discord => NotificationRouteKindDto::Discord,
    }
}

fn map_upsert_receipt(
    receipt: NotificationCredentialUpsertReceipt,
) -> UpsertNotificationRouteCredentialsResponseDto {
    UpsertNotificationRouteCredentialsResponseDto {
        project_id: receipt.project_id,
        route_id: receipt.route_id,
        route_kind: map_route_kind(receipt.route_kind),
        credential_scope: "app_local".into(),
        has_bot_token: receipt.has_bot_token,
        has_chat_id: receipt.has_chat_id,
        has_webhook_url: receipt.has_webhook_url,
        updated_at: receipt.updated_at,
    }
}

fn command_error_from_adapter(error: NotificationAdapterError) -> CommandError {
    if error.retryable {
        CommandError::retryable(error.code, error.message)
    } else {
        CommandError::user_fixable(error.code, error.message)
    }
}
