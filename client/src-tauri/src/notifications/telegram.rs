use std::time::Duration;

use reqwest::blocking::Client;
use serde::Deserialize;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    auth::now_timestamp,
    notifications::{
        NotificationAdapterError, NotificationInboundBatch, NotificationInboundMessage,
        ParsedNotificationReplyEnvelope, TelegramRouteCredentials, TelegramTransport,
    },
};

const TELEGRAM_DEFAULT_TIMEOUT: Duration = Duration::from_secs(8);
const TELEGRAM_CORRELATION_KEY_HEX_LEN: usize = 32;

#[derive(Debug, Clone)]
pub struct ReqwestTelegramTransport {
    client: Client,
}

impl ReqwestTelegramTransport {
    pub fn new(timeout: Duration) -> Result<Self, NotificationAdapterError> {
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| {
                NotificationAdapterError::transport_failed(format!(
                    "Cadence could not initialize the Telegram notification client: {error}"
                ))
            })?;

        Ok(Self { client })
    }

    pub fn with_client(client: Client) -> Self {
        Self { client }
    }
}

impl Default for ReqwestTelegramTransport {
    fn default() -> Self {
        Self::new(TELEGRAM_DEFAULT_TIMEOUT)
            .expect("telegram transport default client should initialize")
    }
}

impl TelegramTransport for ReqwestTelegramTransport {
    fn send_message(
        &self,
        credentials: &TelegramRouteCredentials,
        message: &str,
    ) -> Result<(), NotificationAdapterError> {
        let endpoint = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            credentials.bot_token
        );

        let response = self
            .client
            .post(endpoint)
            .json(&serde_json::json!({
                "chat_id": credentials.chat_id,
                "text": message,
                "disable_web_page_preview": true,
            }))
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    NotificationAdapterError::transport_timeout(
                        "Cadence timed out while sending a Telegram notification dispatch.",
                    )
                } else {
                    NotificationAdapterError::transport_failed(
                        "Cadence could not send a Telegram notification dispatch.",
                    )
                }
            })?;

        let status = response.status();
        let body = response.text().unwrap_or_default();

        if !status.is_success() {
            let retryable = status.as_u16() == 429 || status.is_server_error();
            let message = format!(
                "Cadence received HTTP {} while sending a Telegram notification dispatch.",
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

        let payload: TelegramSendMessageResponse = serde_json::from_str(&body).map_err(|_| {
            NotificationAdapterError::payload_invalid(
                "Cadence could not decode Telegram sendMessage response payload.",
            )
        })?;

        if !payload.ok {
            return Err(NotificationAdapterError::transport_failed(
                "Cadence received a rejected Telegram sendMessage response.",
            ));
        }

        Ok(())
    }

    fn fetch_replies(
        &self,
        credentials: &TelegramRouteCredentials,
        cursor: Option<&str>,
    ) -> Result<NotificationInboundBatch, NotificationAdapterError> {
        let endpoint = format!(
            "https://api.telegram.org/bot{}/getUpdates",
            credentials.bot_token
        );

        let offset = cursor.map(parse_cursor_offset).transpose()?;
        let request_body = match offset {
            Some(offset) => serde_json::json!({
                "offset": offset,
                "timeout": 0,
                "allowed_updates": ["message"],
            }),
            None => serde_json::json!({
                "timeout": 0,
                "allowed_updates": ["message"],
            }),
        };

        let response = self
            .client
            .post(endpoint)
            .json(&request_body)
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    NotificationAdapterError::transport_timeout(
                        "Cadence timed out while polling Telegram notification replies.",
                    )
                } else {
                    NotificationAdapterError::transport_failed(
                        "Cadence could not poll Telegram notification replies.",
                    )
                }
            })?;

        let status = response.status();
        let body = response.text().unwrap_or_default();

        if !status.is_success() {
            let retryable = status.as_u16() == 429 || status.is_server_error();
            let message = format!(
                "Cadence received HTTP {} while polling Telegram notification replies.",
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

        let payload: TelegramGetUpdatesResponse = serde_json::from_str(&body).map_err(|_| {
            NotificationAdapterError::payload_invalid(
                "Cadence could not decode Telegram getUpdates response payload.",
            )
        })?;

        if !payload.ok {
            return Err(NotificationAdapterError::transport_failed(
                "Cadence received a rejected Telegram getUpdates response.",
            ));
        }

        let mut messages = Vec::new();
        let mut max_update_id: Option<i64> = None;

        for update in payload.result {
            max_update_id = Some(match max_update_id {
                Some(existing) => existing.max(update.update_id),
                None => update.update_id,
            });

            let Some(message) = update.message else {
                continue;
            };

            let body = message.text.unwrap_or_default();
            let context_action_id = message
                .reply_to_message
                .as_ref()
                .and_then(|reply| reply.text.as_deref())
                .and_then(extract_action_id_from_dispatch_message)
                .or_else(|| extract_action_id_from_dispatch_message(&body));

            messages.push(NotificationInboundMessage {
                message_id: message.message_id.to_string(),
                responder_id: message.from.map(|from| from.id.to_string()),
                received_at: message
                    .date
                    .map(unix_timestamp_to_rfc3339)
                    .unwrap_or_else(now_timestamp),
                body,
                context_action_id,
            });
        }

        Ok(NotificationInboundBatch {
            messages,
            next_cursor: max_update_id.map(|value| value.saturating_add(1).to_string()),
        })
    }
}

pub fn is_valid_cursor(value: &str) -> bool {
    parse_cursor_offset(value).is_ok()
}

pub fn parse_reply_envelope(
    body: &str,
) -> Result<ParsedNotificationReplyEnvelope, NotificationAdapterError> {
    parse_reply_envelope_for_channel("Telegram", body)
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

    suffix.len() == TELEGRAM_CORRELATION_KEY_HEX_LEN
        && suffix
            .chars()
            .all(|character| character.is_ascii_hexdigit())
        && suffix
            .chars()
            .all(|character| !character.is_ascii_uppercase())
}

fn parse_cursor_offset(value: &str) -> Result<i64, NotificationAdapterError> {
    let parsed = value.trim().parse::<i64>().map_err(|_| {
        NotificationAdapterError::payload_invalid(
            "Cadence requires numeric Telegram inbound cursor values.",
        )
    })?;

    if parsed < 0 {
        return Err(NotificationAdapterError::payload_invalid(
            "Cadence requires Telegram inbound cursor values greater than or equal to zero.",
        ));
    }

    Ok(parsed)
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

fn unix_timestamp_to_rfc3339(unix_timestamp: i64) -> String {
    OffsetDateTime::from_unix_timestamp(unix_timestamp)
        .ok()
        .and_then(|value| value.format(&Rfc3339).ok())
        .unwrap_or_else(now_timestamp)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramSendMessageResponse {
    ok: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramGetUpdatesResponse {
    ok: bool,
    #[serde(default)]
    result: Vec<TelegramUpdate>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramUpdate {
    update_id: i64,
    message: Option<TelegramUpdateMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramUpdateMessage {
    message_id: i64,
    date: Option<i64>,
    text: Option<String>,
    from: Option<TelegramUser>,
    reply_to_message: Option<TelegramReplyMessage>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramReplyMessage {
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TelegramUser {
    id: i64,
}
