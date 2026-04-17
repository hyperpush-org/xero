use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;

use crate::{
    auth::now_timestamp,
    notifications::{
        DiscordRouteCredentials, DiscordTransport, NotificationAdapterError,
        NotificationInboundBatch, NotificationInboundMessage, ParsedNotificationReplyEnvelope,
    },
};

const DISCORD_DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);
const DISCORD_CORRELATION_KEY_HEX_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct ReqwestDiscordTransport {
    client: Client,
}

impl ReqwestDiscordTransport {
    pub fn new(timeout: Duration) -> Result<Self, NotificationAdapterError> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| {
                NotificationAdapterError::transport_failed(format!(
                    "Cadence could not initialize the Discord notification client: {error}"
                ))
            })?;

        Ok(Self { client })
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

impl Default for ReqwestDiscordTransport {
    fn default() -> Self {
        Self::new(DISCORD_DEFAULT_TIMEOUT)
            .expect("discord transport default client should initialize")
    }
}

impl DiscordTransport for ReqwestDiscordTransport {
    fn send_message(
        &self,
        credentials: &DiscordRouteCredentials,
        message: &str,
    ) -> Result<(), NotificationAdapterError> {
        let response = self
            .client
            .post(&credentials.webhook_url)
            .json(&serde_json::json!({ "content": message }))
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    NotificationAdapterError::transport_timeout(
                        "Cadence timed out while sending a Discord notification dispatch.",
                    )
                } else {
                    NotificationAdapterError::transport_failed(
                        "Cadence could not send a Discord notification dispatch.",
                    )
                }
            })?;

        let status = response.status();
        let body = response.text().unwrap_or_default();

        if !status.is_success() {
            let retryable = status.as_u16() == 429 || status.is_server_error();
            let message = format!(
                "Cadence received HTTP {} while sending a Discord notification dispatch.",
                status.as_u16()
            );

            return if retryable {
                Err(NotificationAdapterError::transport_failed(message))
            } else {
                Err(NotificationAdapterError::new(
                    "notification_adapter_transport_failed",
                    message,
                    false,
                ))
            };
        }

        if !body.trim().is_empty() {
            serde_json::from_str::<serde_json::Value>(&body).map_err(|_| {
                NotificationAdapterError::payload_invalid(
                    "Cadence could not decode Discord webhook response payload.",
                )
            })?;
        }

        Ok(())
    }

    fn fetch_replies(
        &self,
        credentials: &DiscordRouteCredentials,
        channel_target: &str,
        cursor: Option<&str>,
    ) -> Result<NotificationInboundBatch, NotificationAdapterError> {
        let channel_target = channel_target.trim();
        if channel_target.is_empty() {
            return Err(NotificationAdapterError::payload_invalid(
                "Cadence requires a non-empty Discord channel target to poll inbound replies.",
            ));
        }

        let bot_token = credentials
            .bot_token
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                NotificationAdapterError::credentials_missing(
                    "Cadence could not poll Discord replies because app-local `botToken` credentials are missing for this route.",
                )
            })?;

        let mut query: Vec<(&str, String)> = vec![("limit", "50".to_string())];
        if let Some(after_cursor) = cursor.map(str::trim).filter(|value| !value.is_empty()) {
            if !is_valid_cursor(after_cursor) {
                return Err(NotificationAdapterError::payload_invalid(
                    "Cadence requires Discord inbound cursor values to be numeric snowflake ids.",
                ));
            }

            query.push(("after", after_cursor.to_string()));
        }

        let endpoint = format!("https://discord.com/api/v10/channels/{channel_target}/messages");

        let response = self
            .client
            .get(endpoint)
            .header(reqwest::header::AUTHORIZATION, format!("Bot {bot_token}"))
            .query(&query)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    NotificationAdapterError::transport_timeout(
                        "Cadence timed out while polling Discord notification replies.",
                    )
                } else {
                    NotificationAdapterError::transport_failed(
                        "Cadence could not poll Discord notification replies.",
                    )
                }
            })?;

        let status = response.status();
        let body = response.text().unwrap_or_default();

        if !status.is_success() {
            let retryable = status.as_u16() == 429 || status.is_server_error();
            let message = format!(
                "Cadence received HTTP {} while polling Discord notification replies.",
                status.as_u16()
            );

            return if retryable {
                Err(NotificationAdapterError::transport_failed(message))
            } else {
                Err(NotificationAdapterError::new(
                    "notification_adapter_transport_failed",
                    message,
                    false,
                ))
            };
        }

        let mut payload: Vec<DiscordMessage> = serde_json::from_str(&body).map_err(|_| {
            NotificationAdapterError::payload_invalid(
                "Cadence could not decode Discord inbound message payload.",
            )
        })?;

        payload.sort_by(|left, right| {
            parse_snowflake_id(&left.id)
                .cmp(&parse_snowflake_id(&right.id))
                .then_with(|| left.id.cmp(&right.id))
        });

        let mut next_cursor: Option<String> = None;
        let mut messages = Vec::new();

        for message in payload {
            if message.id.trim().is_empty() {
                continue;
            }

            next_cursor = match next_cursor {
                Some(existing) => Some(max_snowflake_id(existing, message.id.clone())),
                None => Some(message.id.clone()),
            };

            let body = message.content.unwrap_or_default();
            let context_action_id = message
                .referenced_message
                .as_ref()
                .and_then(|referenced| referenced.content.as_deref())
                .and_then(extract_action_id_from_dispatch_message)
                .or_else(|| extract_action_id_from_dispatch_message(&body));

            messages.push(NotificationInboundMessage {
                message_id: message.id,
                responder_id: message
                    .author
                    .as_ref()
                    .and_then(|author| trim_to_option(Some(author.id.as_str()))),
                received_at: trim_to_option(message.timestamp.as_deref())
                    .unwrap_or_else(now_timestamp),
                body,
                context_action_id,
            });
        }

        Ok(NotificationInboundBatch {
            messages,
            next_cursor,
        })
    }
}

pub fn is_valid_cursor(value: &str) -> bool {
    parse_snowflake_id(value).is_some()
}

pub fn parse_reply_envelope(
    body: &str,
) -> Result<ParsedNotificationReplyEnvelope, NotificationAdapterError> {
    parse_reply_envelope_for_channel("Discord", body)
}

fn parse_reply_envelope_for_channel(
    channel_name: &str,
    body: &str,
) -> Result<ParsedNotificationReplyEnvelope, NotificationAdapterError> {
    let line = body
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            NotificationAdapterError::new(
                "notification_reply_request_invalid",
                format!(
                    "Cadence rejected a {channel_name} reply because the message body was empty."
                ),
                false,
            )
        })?;

    let (decision, correlation_key, reply_text) = split_reply_line(line).ok_or_else(|| {
        NotificationAdapterError::new(
            "notification_reply_request_invalid",
            format!(
                "Cadence rejected a {channel_name} reply because it must follow `<decision> <correlation-key> <answer>` grammar."
            ),
            false,
        )
    })?;

    if decision != "approve" && decision != "reject" {
        return Err(NotificationAdapterError::new(
            "notification_reply_decision_unsupported",
            format!(
                "Cadence rejected a {channel_name} reply because decision `{decision}` is unsupported. Allowed decisions: approve, reject."
            ),
            false,
        ));
    }

    if !is_valid_correlation_key(correlation_key) {
        return Err(NotificationAdapterError::new(
            "notification_reply_correlation_invalid",
            format!(
                "Cadence rejected a {channel_name} reply because correlation key `{correlation_key}` is malformed."
            ),
            false,
        ));
    }

    if reply_text.trim().is_empty() {
        return Err(NotificationAdapterError::new(
            "notification_reply_request_invalid",
            format!(
                "Cadence rejected a {channel_name} reply because required-input answers cannot be empty."
            ),
            false,
        ));
    }

    Ok(ParsedNotificationReplyEnvelope {
        decision: decision.to_string(),
        correlation_key: correlation_key.to_string(),
        reply_text: reply_text.trim().to_string(),
    })
}

fn split_reply_line(line: &str) -> Option<(&str, &str, &str)> {
    let line = line.trim();
    let decision_end = line.find(char::is_whitespace)?;
    let decision = line.get(..decision_end)?.trim();
    let remainder = line.get(decision_end..)?.trim_start();

    let correlation_end = remainder.find(char::is_whitespace)?;
    let correlation_key = remainder.get(..correlation_end)?.trim();
    let reply_text = remainder.get(correlation_end..)?.trim_start();

    Some((decision, correlation_key, reply_text))
}

fn is_valid_correlation_key(value: &str) -> bool {
    let value = value.trim();
    let Some(suffix) = value.strip_prefix("nfy:") else {
        return false;
    };

    suffix.len() == DISCORD_CORRELATION_KEY_HEX_LEN
        && suffix
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        && suffix
            .chars()
            .all(|character| !character.is_ascii_uppercase())
}

fn extract_action_id_from_dispatch_message(value: &str) -> Option<String> {
    value.lines().find_map(|line| {
        line.trim()
            .strip_prefix("Action ID:")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn trim_to_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn parse_snowflake_id(value: &str) -> Option<u128> {
    value.trim().parse::<u128>().ok().filter(|value| *value > 0)
}

fn max_snowflake_id(left: String, right: String) -> String {
    match (parse_snowflake_id(&left), parse_snowflake_id(&right)) {
        (Some(left), Some(right)) if right > left => right.to_string(),
        (Some(_), Some(_)) => left,
        (None, Some(_)) => right,
        _ => left,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscordMessage {
    id: String,
    content: Option<String>,
    timestamp: Option<String>,
    author: Option<DiscordAuthor>,
    referenced_message: Option<DiscordReferencedMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscordAuthor {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DiscordReferencedMessage {
    content: Option<String>,
}
