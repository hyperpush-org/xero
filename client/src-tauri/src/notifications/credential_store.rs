use std::{fs, io::Write, path::PathBuf};

use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;
use url::Url;

use crate::{
    auth::now_timestamp,
    notifications::{
        DiscordRouteCredentials, NotificationAdapterError, NotificationCredentialResolver,
        NotificationRouteKind, RouteCredentials, TelegramRouteCredentials,
    },
};

pub const NOTIFICATION_CREDENTIAL_STORE_FILE_NAME: &str = "notification-credentials.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotificationCredentialUpsertInput {
    Telegram {
        bot_token: String,
        chat_id: String,
    },
    Discord {
        webhook_url: String,
        bot_token: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationCredentialUpsertReceipt {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: NotificationRouteKind,
    pub has_bot_token: bool,
    pub has_chat_id: bool,
    pub has_webhook_url: bool,
    pub updated_at: String,
}

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

#[derive(Debug, Clone)]
pub struct FileNotificationCredentialStore {
    path: PathBuf,
}

impl FileNotificationCredentialStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn load_readiness_projector(&self) -> NotificationCredentialReadinessProjector {
        match self.load_store_file() {
            Ok(store) => NotificationCredentialReadinessProjector {
                store: Some(store),
                load_error: None,
            },
            Err(error) => NotificationCredentialReadinessProjector {
                store: None,
                load_error: Some(error),
            },
        }
    }

    pub fn upsert_route_credentials(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
        credentials: NotificationCredentialUpsertInput,
        updated_at: &str,
    ) -> Result<NotificationCredentialUpsertReceipt, NotificationAdapterError> {
        let project_id = require_identifier(project_id, "projectId")?;
        let route_id = require_identifier(route_id, "routeId")?;
        let updated_at = require_identifier(updated_at, "updatedAt")?;

        let (bot_token, chat_id, webhook_url) =
            sanitize_upsert_credentials(route_kind, credentials, &project_id, &route_id)?;

        let mut store = self.load_store_file()?;

        if let Some(existing) = store
            .routes
            .iter_mut()
            .find(|entry| entry.project_id == project_id && entry.route_id == route_id)
        {
            existing.route_kind = route_kind.as_str().to_string();
            existing.bot_token = bot_token.clone();
            existing.chat_id = chat_id.clone();
            existing.webhook_url = webhook_url.clone();
        } else {
            store.routes.push(NotificationCredentialStoreEntry {
                project_id: project_id.clone(),
                route_id: route_id.clone(),
                route_kind: route_kind.as_str().to_string(),
                bot_token: bot_token.clone(),
                chat_id: chat_id.clone(),
                webhook_url: webhook_url.clone(),
            });
        }

        self.write_store_file(&store)?;

        Ok(NotificationCredentialUpsertReceipt {
            project_id,
            route_id,
            route_kind,
            has_bot_token: bot_token
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            has_chat_id: chat_id
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            has_webhook_url: webhook_url
                .as_deref()
                .map(str::trim)
                .is_some_and(|value| !value.is_empty()),
            updated_at,
        })
    }

    fn load_store_file(&self) -> Result<NotificationCredentialStoreFile, NotificationAdapterError> {
        if !self.path.exists() {
            return Ok(NotificationCredentialStoreFile::default());
        }

        let contents = fs::read_to_string(&self.path).map_err(|error| {
            NotificationAdapterError::credentials_read_failed(format!(
                "Cadence could not read the app-local notification credential store at {}: {error}",
                self.path.display()
            ))
        })?;

        serde_json::from_str::<NotificationCredentialStoreFile>(&contents).map_err(|error| {
            NotificationAdapterError::credentials_malformed(format!(
                "Cadence could not decode the app-local notification credential store at {}: {error}",
                self.path.display()
            ))
        })
    }

    fn write_store_file(
        &self,
        store: &NotificationCredentialStoreFile,
    ) -> Result<(), NotificationAdapterError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                NotificationAdapterError::new(
                    "notification_adapter_credentials_directory_unavailable",
                    format!(
                        "Cadence could not prepare the app-local notification credential directory at {}: {error}",
                        parent.display()
                    ),
                    true,
                )
            })?;
        }

        let json = serde_json::to_string_pretty(store).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_encode_failed",
                format!(
                    "Cadence could not serialize the app-local notification credential store at {}: {error}",
                    self.path.display()
                ),
                false,
            )
        })?;

        let parent = self
            .path
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut temp_file = NamedTempFile::new_in(&parent).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not prepare a temporary app-local notification credential store file near {}: {error}",
                    self.path.display()
                ),
                true,
            )
        })?;

        temp_file.write_all(json.as_bytes()).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not write temporary app-local notification credential data near {}: {error}",
                    self.path.display()
                ),
                true,
            )
        })?;

        temp_file.as_file_mut().sync_all().map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not flush temporary app-local notification credential data near {}: {error}",
                    self.path.display()
                ),
                true,
            )
        })?;

        temp_file.persist(&self.path).map_err(|error| {
            NotificationAdapterError::new(
                "notification_adapter_credentials_write_failed",
                format!(
                    "Cadence could not persist the app-local notification credential store at {}: {}",
                    self.path.display(),
                    error.error
                ),
                true,
            )
        })?;

        Ok(())
    }
}

impl NotificationCredentialResolver for FileNotificationCredentialStore {
    fn resolve_route_credentials(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
    ) -> Result<RouteCredentials, NotificationAdapterError> {
        let store = self.load_store_file()?;

        let Some(entry) = store
            .routes
            .into_iter()
            .find(|entry| entry.project_id == project_id && entry.route_id == route_id)
        else {
            return Err(NotificationAdapterError::credentials_missing(format!(
                "Cadence has no app-local credentials for notification route `{route_id}` in project `{project_id}`."
            )));
        };

        let parsed_entry_kind =
            NotificationRouteKind::parse(entry.route_kind.as_str()).map_err(|_| {
                NotificationAdapterError::credentials_malformed(format!(
                    "Cadence found malformed app-local credentials for route `{route_id}` because `route_kind` was not `telegram` or `discord`."
                ))
            })?;

        if parsed_entry_kind != route_kind {
            return Err(NotificationAdapterError::credentials_malformed(format!(
                "Cadence found app-local credentials for route `{route_id}` with kind `{}` but expected `{}`.",
                parsed_entry_kind.as_str(),
                route_kind.as_str()
            )));
        }

        match route_kind {
            NotificationRouteKind::Telegram => {
                let bot_token = require_non_empty(
                    entry.bot_token.as_deref(),
                    "botToken",
                    project_id,
                    route_id,
                )?;
                let chat_id =
                    require_non_empty(entry.chat_id.as_deref(), "chatId", project_id, route_id)?;

                Ok(RouteCredentials::Telegram(TelegramRouteCredentials {
                    bot_token,
                    chat_id,
                }))
            }
            NotificationRouteKind::Discord => {
                let webhook_url = require_non_empty(
                    entry.webhook_url.as_deref(),
                    "webhookUrl",
                    project_id,
                    route_id,
                )?;
                validate_webhook_url(&webhook_url, project_id, route_id)?;

                let bot_token = entry
                    .bot_token
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(ToString::to_string);

                Ok(RouteCredentials::Discord(DiscordRouteCredentials {
                    webhook_url,
                    bot_token,
                }))
            }
        }
    }

    fn load_inbound_cursor(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
    ) -> Result<Option<String>, NotificationAdapterError> {
        let store = self.load_store_file()?;
        Ok(store
            .inbound_cursors
            .into_iter()
            .find(|entry| {
                entry.project_id == project_id
                    && entry.route_id == route_id
                    && entry.route_kind == route_kind.as_str()
            })
            .map(|entry| entry.cursor))
    }

    fn persist_inbound_cursor(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
        cursor: &str,
    ) -> Result<(), NotificationAdapterError> {
        let cursor = cursor.trim();
        if cursor.is_empty() {
            return Err(NotificationAdapterError::payload_invalid(
                "Cadence requires non-empty inbound cursor values before persisting adapter replay state.",
            ));
        }

        let mut store = self.load_store_file()?;
        let updated_at = now_timestamp();

        if let Some(existing) = store.inbound_cursors.iter_mut().find(|entry| {
            entry.project_id == project_id
                && entry.route_id == route_id
                && entry.route_kind == route_kind.as_str()
        }) {
            existing.cursor = cursor.to_string();
            existing.updated_at = updated_at;
        } else {
            store.inbound_cursors.push(NotificationInboundCursorEntry {
                project_id: project_id.to_string(),
                route_id: route_id.to_string(),
                route_kind: route_kind.as_str().to_string(),
                cursor: cursor.to_string(),
                updated_at,
            });
        }

        self.write_store_file(&store)
    }
}

impl NotificationCredentialReadinessProjector {
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
        "notification_adapter_credentials_missing" => NotificationCredentialReadinessStatus::Missing,
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

fn require_identifier(value: &str, field: &str) -> Result<String, NotificationAdapterError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(NotificationAdapterError::payload_invalid(format!(
            "Cadence requires non-empty `{field}` values before persisting app-local notification credentials.",
        )));
    }

    Ok(value.to_string())
}

fn sanitize_upsert_credentials(
    route_kind: NotificationRouteKind,
    credentials: NotificationCredentialUpsertInput,
    project_id: &str,
    route_id: &str,
) -> Result<(Option<String>, Option<String>, Option<String>), NotificationAdapterError> {
    match (route_kind, credentials) {
        (
            NotificationRouteKind::Telegram,
            NotificationCredentialUpsertInput::Telegram { bot_token, chat_id },
        ) => {
            let bot_token =
                require_non_empty(Some(bot_token.as_str()), "botToken", project_id, route_id)?;
            let chat_id =
                require_non_empty(Some(chat_id.as_str()), "chatId", project_id, route_id)?;
            Ok((Some(bot_token), Some(chat_id), None))
        }
        (
            NotificationRouteKind::Discord,
            NotificationCredentialUpsertInput::Discord {
                webhook_url,
                bot_token,
            },
        ) => {
            let webhook_url = require_non_empty(
                Some(webhook_url.as_str()),
                "webhookUrl",
                project_id,
                route_id,
            )?;
            validate_webhook_url(&webhook_url, project_id, route_id)?;
            let bot_token = bot_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string);
            Ok((bot_token, None, Some(webhook_url)))
        }
        (expected_kind, _) => Err(NotificationAdapterError::payload_invalid(format!(
            "Cadence requires `{}` credentials for route `{route_id}` in project `{project_id}`.",
            expected_kind.as_str()
        ))),
    }
}

fn require_non_empty(
    value: Option<&str>,
    field: &str,
    project_id: &str,
    route_id: &str,
) -> Result<String, NotificationAdapterError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(NotificationAdapterError::credentials_missing(format!(
            "Cadence has no app-local `{field}` credential for notification route `{route_id}` in project `{project_id}`."
        )));
    };

    Ok(value.to_string())
}

fn validate_webhook_url(
    webhook_url: &str,
    project_id: &str,
    route_id: &str,
) -> Result<(), NotificationAdapterError> {
    let parsed = Url::parse(webhook_url).map_err(|_| {
        NotificationAdapterError::credentials_malformed(format!(
            "Cadence found malformed `webhookUrl` credentials for route `{route_id}` in project `{project_id}`."
        ))
    })?;

    if parsed.scheme() != "https" {
        return Err(NotificationAdapterError::credentials_malformed(format!(
            "Cadence requires `https` Discord webhook credentials for route `{route_id}` in project `{project_id}`."
        )));
    }

    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationCredentialStoreFile {
    #[serde(default)]
    pub routes: Vec<NotificationCredentialStoreEntry>,
    #[serde(default)]
    pub inbound_cursors: Vec<NotificationInboundCursorEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationCredentialStoreEntry {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationInboundCursorEntry {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub cursor: String,
    pub updated_at: String,
}
