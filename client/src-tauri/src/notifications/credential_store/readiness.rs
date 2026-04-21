use crate::notifications::{NotificationAdapterError, NotificationRouteKind};

use super::{file_store::NotificationCredentialStoreFile, validation::validate_webhook_url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationCredentialReadinessStatus {
    Ready,
    Missing,
    Malformed,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCredentialReadinessDiagnostic {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCredentialReadinessProjection {
    pub has_bot_token: bool,
    pub has_chat_id: bool,
    pub has_webhook_url: bool,
    pub ready: bool,
    pub status: NotificationCredentialReadinessStatus,
    pub diagnostic: Option<NotificationCredentialReadinessDiagnostic>,
}

impl NotificationCredentialReadinessProjection {
    fn ready(has_bot_token: bool, has_chat_id: bool, has_webhook_url: bool) -> Self {
        Self {
            has_bot_token,
            has_chat_id,
            has_webhook_url,
            ready: true,
            status: NotificationCredentialReadinessStatus::Ready,
            diagnostic: None,
        }
    }

    fn fail_closed(
        has_bot_token: bool,
        has_chat_id: bool,
        has_webhook_url: bool,
        status: NotificationCredentialReadinessStatus,
        error: NotificationAdapterError,
    ) -> Self {
        Self {
            has_bot_token,
            has_chat_id,
            has_webhook_url,
            ready: false,
            status,
            diagnostic: Some(NotificationCredentialReadinessDiagnostic {
                code: error.code,
                message: error.message,
                retryable: error.retryable,
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NotificationCredentialReadinessProjector {
    store: Option<NotificationCredentialStoreFile>,
    load_error: Option<NotificationAdapterError>,
}

impl NotificationCredentialReadinessProjector {
    pub(crate) fn from_store_result(
        result: Result<NotificationCredentialStoreFile, NotificationAdapterError>,
    ) -> Self {
        match result {
            Ok(store) => Self {
                store: Some(store),
                load_error: None,
            },
            Err(error) => Self {
                store: None,
                load_error: Some(error),
            },
        }
    }

    pub fn project_route(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
    ) -> NotificationCredentialReadinessProjection {
        if let Some(error) = self.load_error.clone() {
            return projection_from_store_error(error);
        }

        let Some(store) = self.store.as_ref() else {
            return projection_from_store_error(NotificationAdapterError::credentials_read_failed(
                "Cadence could not load app-local credential readiness projection state.",
            ));
        };

        project_route_readiness_from_store(store, project_id, route_id, route_kind)
    }
}

fn project_route_readiness_from_store(
    store: &NotificationCredentialStoreFile,
    project_id: &str,
    route_id: &str,
    route_kind: NotificationRouteKind,
) -> NotificationCredentialReadinessProjection {
    let Some(entry) = store
        .routes
        .iter()
        .find(|entry| entry.project_id == project_id && entry.route_id == route_id)
    else {
        return NotificationCredentialReadinessProjection::fail_closed(
            false,
            false,
            false,
            NotificationCredentialReadinessStatus::Missing,
            NotificationAdapterError::credentials_missing(format!(
                "Cadence has no app-local credentials for notification route `{route_id}` in project `{project_id}`."
            )),
        );
    };

    let parsed_entry_kind = match NotificationRouteKind::parse(entry.route_kind.as_str()) {
        Ok(kind) => kind,
        Err(_) => {
            return NotificationCredentialReadinessProjection::fail_closed(
                false,
                false,
                false,
                NotificationCredentialReadinessStatus::Malformed,
                NotificationAdapterError::credentials_malformed(format!(
                    "Cadence found malformed app-local credentials for route `{route_id}` because `route_kind` was not `telegram` or `discord`."
                )),
            )
        }
    };

    if parsed_entry_kind != route_kind {
        return NotificationCredentialReadinessProjection::fail_closed(
            false,
            false,
            false,
            NotificationCredentialReadinessStatus::Malformed,
            NotificationAdapterError::credentials_malformed(format!(
                "Cadence found app-local credentials for route `{route_id}` with kind `{}` but expected `{}`.",
                parsed_entry_kind.as_str(),
                route_kind.as_str()
            )),
        );
    }

    let has_bot_token = has_non_empty_optional(entry.bot_token.as_deref());
    let has_chat_id = has_non_empty_optional(entry.chat_id.as_deref());
    let has_webhook_url = has_non_empty_optional(entry.webhook_url.as_deref());

    match route_kind {
        NotificationRouteKind::Telegram => {
            if has_webhook_url {
                return NotificationCredentialReadinessProjection::fail_closed(
                    has_bot_token,
                    has_chat_id,
                    has_webhook_url,
                    NotificationCredentialReadinessStatus::Malformed,
                    NotificationAdapterError::credentials_malformed(format!(
                        "Cadence found malformed app-local credentials for route `{route_id}` because Telegram routes must not define `webhookUrl`."
                    )),
                );
            }

            if has_bot_token && has_chat_id {
                return NotificationCredentialReadinessProjection::ready(
                    has_bot_token,
                    has_chat_id,
                    has_webhook_url,
                );
            }

            NotificationCredentialReadinessProjection::fail_closed(
                has_bot_token,
                has_chat_id,
                has_webhook_url,
                NotificationCredentialReadinessStatus::Missing,
                NotificationAdapterError::credentials_missing(format!(
                    "Cadence requires both app-local `botToken` and `chatId` credentials for Telegram route `{route_id}` in project `{project_id}`."
                )),
            )
        }
        NotificationRouteKind::Discord => {
            if has_chat_id {
                return NotificationCredentialReadinessProjection::fail_closed(
                    has_bot_token,
                    has_chat_id,
                    has_webhook_url,
                    NotificationCredentialReadinessStatus::Malformed,
                    NotificationAdapterError::credentials_malformed(format!(
                        "Cadence found malformed app-local credentials for route `{route_id}` because Discord routes must not define `chatId`."
                    )),
                );
            }

            if let Some(webhook_url) = entry
                .webhook_url
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                if let Err(error) = validate_webhook_url(webhook_url, project_id, route_id) {
                    return NotificationCredentialReadinessProjection::fail_closed(
                        has_bot_token,
                        has_chat_id,
                        has_webhook_url,
                        NotificationCredentialReadinessStatus::Malformed,
                        error,
                    );
                }
            }

            if has_webhook_url && has_bot_token {
                return NotificationCredentialReadinessProjection::ready(
                    has_bot_token,
                    has_chat_id,
                    has_webhook_url,
                );
            }

            NotificationCredentialReadinessProjection::fail_closed(
                has_bot_token,
                has_chat_id,
                has_webhook_url,
                NotificationCredentialReadinessStatus::Missing,
                NotificationAdapterError::credentials_missing(format!(
                    "Cadence requires app-local `webhookUrl` and `botToken` credentials for Discord route `{route_id}` in project `{project_id}` to support autonomous dispatch + reply handling."
                )),
            )
        }
    }
}

fn projection_from_store_error(
    error: NotificationAdapterError,
) -> NotificationCredentialReadinessProjection {
    let status = match error.code.as_str() {
        "notification_adapter_credentials_missing" => {
            NotificationCredentialReadinessStatus::Missing
        }
        "notification_adapter_credentials_malformed" => {
            NotificationCredentialReadinessStatus::Malformed
        }
        "notification_adapter_credentials_read_failed" => {
            NotificationCredentialReadinessStatus::Unavailable
        }
        _ => NotificationCredentialReadinessStatus::Unavailable,
    };

    NotificationCredentialReadinessProjection::fail_closed(false, false, false, status, error)
}

fn has_non_empty_optional(value: Option<&str>) -> bool {
    value.map(str::trim).is_some_and(|value| !value.is_empty())
}
