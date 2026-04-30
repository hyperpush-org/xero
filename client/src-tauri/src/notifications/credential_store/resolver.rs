use crate::{
    auth::now_timestamp,
    notifications::{
        DiscordRouteCredentials, FileNotificationCredentialStore, NotificationAdapterError,
        NotificationCredentialResolver, NotificationRouteKind, RouteCredentials,
        TelegramRouteCredentials,
    },
};

use super::{
    file_store::NotificationInboundCursorEntry,
    validation::{require_non_empty, validate_webhook_url},
};

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
                "Xero has no app-local credentials for notification route `{route_id}` in project `{project_id}`."
            )));
        };

        let parsed_entry_kind =
            NotificationRouteKind::parse(entry.route_kind.as_str()).map_err(|_| {
                NotificationAdapterError::credentials_malformed(format!(
                    "Xero found malformed app-local credentials for route `{route_id}` because `route_kind` was not `telegram` or `discord`."
                ))
            })?;

        if parsed_entry_kind != route_kind {
            return Err(NotificationAdapterError::credentials_malformed(format!(
                "Xero found app-local credentials for route `{route_id}` with kind `{}` but expected `{}`.",
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
                "Xero requires non-empty inbound cursor values before persisting adapter replay state.",
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
