use url::Url;

use crate::notifications::{NotificationAdapterError, NotificationRouteKind};

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

pub(crate) fn require_identifier(
    value: &str,
    field: &str,
) -> Result<String, NotificationAdapterError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(NotificationAdapterError::payload_invalid(format!(
            "Xero requires non-empty `{field}` values before persisting app-local notification credentials.",
        )));
    }

    Ok(value.to_string())
}

type SanitizedRouteCredentials = (Option<String>, Option<String>, Option<String>);

pub(crate) fn sanitize_upsert_credentials(
    route_kind: NotificationRouteKind,
    credentials: NotificationCredentialUpsertInput,
    project_id: &str,
    route_id: &str,
) -> Result<SanitizedRouteCredentials, NotificationAdapterError> {
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
            "Xero requires `{}` credentials for route `{route_id}` in project `{project_id}`.",
            expected_kind.as_str()
        ))),
    }
}

pub(crate) fn require_non_empty(
    value: Option<&str>,
    field: &str,
    project_id: &str,
    route_id: &str,
) -> Result<String, NotificationAdapterError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(NotificationAdapterError::credentials_missing(format!(
            "Xero has no app-local `{field}` credential for notification route `{route_id}` in project `{project_id}`."
        )));
    };

    Ok(value.to_string())
}

pub(crate) fn validate_webhook_url(
    webhook_url: &str,
    project_id: &str,
    route_id: &str,
) -> Result<(), NotificationAdapterError> {
    let parsed = Url::parse(webhook_url).map_err(|_| {
        NotificationAdapterError::credentials_malformed(format!(
            "Xero found malformed `webhookUrl` credentials for route `{route_id}` in project `{project_id}`."
        ))
    })?;

    if parsed.scheme() != "https" {
        return Err(NotificationAdapterError::credentials_malformed(format!(
            "Xero requires `https` Discord webhook credentials for route `{route_id}` in project `{project_id}`."
        )));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        sanitize_upsert_credentials, validate_webhook_url, NotificationCredentialUpsertInput,
    };
    use crate::notifications::NotificationRouteKind;

    #[test]
    fn sanitize_upsert_credentials_trims_optional_discord_bot_token() {
        let (bot_token, chat_id, webhook_url) = sanitize_upsert_credentials(
            NotificationRouteKind::Discord,
            NotificationCredentialUpsertInput::Discord {
                webhook_url: "https://discord.com/api/webhooks/1/2".into(),
                bot_token: Some("  bot-token  ".into()),
            },
            "project-1",
            "route-1",
        )
        .expect("discord credentials should sanitize");

        assert_eq!(bot_token.as_deref(), Some("bot-token"));
        assert!(chat_id.is_none());
        assert_eq!(
            webhook_url.as_deref(),
            Some("https://discord.com/api/webhooks/1/2")
        );
    }

    #[test]
    fn validate_webhook_url_rejects_non_https_routes() {
        let error = validate_webhook_url(
            "http://discord.com/api/webhooks/1/2",
            "project-1",
            "route-1",
        )
        .expect_err("non-https webhook should fail closed");

        assert_eq!(error.code, "notification_adapter_credentials_malformed");
    }
}
