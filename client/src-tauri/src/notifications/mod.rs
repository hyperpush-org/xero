pub mod credential_store;
pub mod discord;
pub mod route_target;
pub mod service;
pub mod telegram;

pub use credential_store::{
    FileNotificationCredentialStore, NotificationCredentialReadinessDiagnostic,
    NotificationCredentialReadinessProjection, NotificationCredentialReadinessProjector,
    NotificationCredentialReadinessStatus, NotificationCredentialStoreEntry,
    NotificationCredentialStoreFile, NotificationCredentialUpsertInput,
    NotificationCredentialUpsertReceipt, NotificationInboundCursorEntry,
};
pub use discord::ReqwestDiscordTransport;
pub use route_target::{
    compose_notification_route_target, parse_notification_route_target,
    parse_notification_route_target_for_kind, ParsedNotificationRouteTarget,
};
pub use service::{
    NotificationAdapterDispatchAttempt, NotificationAdapterReplyAttempt,
    NotificationDispatchCycleResult, NotificationDispatchService, NotificationReplyCycleResult,
    DISPATCH_ATTEMPTED_DIAGNOSTIC, DISPATCH_FAILED_DIAGNOSTIC, REPLY_RECEIVED_DIAGNOSTIC,
    REPLY_REJECTED_DIAGNOSTIC,
};
pub use telegram::ReqwestTelegramTransport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotificationRouteKind {
    Telegram,
    Discord,
}

impl NotificationRouteKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
        }
    }

    pub fn parse(value: &str) -> Result<Self, NotificationAdapterError> {
        match value.trim() {
            "telegram" => Ok(Self::Telegram),
            "discord" => Ok(Self::Discord),
            other => Err(NotificationAdapterError::payload_invalid(format!(
                "Xero does not support notification route kind `{other}`."
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelegramRouteCredentials {
    pub bot_token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscordRouteCredentials {
    pub webhook_url: String,
    pub bot_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteCredentials {
    Telegram(TelegramRouteCredentials),
    Discord(DiscordRouteCredentials),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationInboundMessage {
    pub message_id: String,
    pub responder_id: Option<String>,
    pub received_at: String,
    pub body: String,
    pub context_action_id: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationInboundBatch {
    pub messages: Vec<NotificationInboundMessage>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedNotificationReplyEnvelope {
    pub decision: String,
    pub correlation_key: String,
    pub reply_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAdapterError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl NotificationAdapterError {
    pub fn new(code: impl Into<String>, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
        }
    }

    pub fn payload_invalid(message: impl Into<String>) -> Self {
        Self::new("notification_adapter_payload_invalid", message, false)
    }

    pub fn credentials_missing(message: impl Into<String>) -> Self {
        Self::new("notification_adapter_credentials_missing", message, false)
    }

    pub fn credentials_malformed(message: impl Into<String>) -> Self {
        Self::new("notification_adapter_credentials_malformed", message, false)
    }

    pub fn credentials_read_failed(message: impl Into<String>) -> Self {
        Self::new(
            "notification_adapter_credentials_read_failed",
            message,
            true,
        )
    }

    pub fn transport_failed(message: impl Into<String>) -> Self {
        Self::new("notification_adapter_transport_failed", message, true)
    }

    pub fn transport_timeout(message: impl Into<String>) -> Self {
        Self::new("notification_adapter_transport_timeout", message, true)
    }
}

pub trait NotificationCredentialResolver: Send + Sync {
    fn resolve_route_credentials(
        &self,
        project_id: &str,
        route_id: &str,
        route_kind: NotificationRouteKind,
    ) -> Result<RouteCredentials, NotificationAdapterError>;

    fn load_inbound_cursor(
        &self,
        _project_id: &str,
        _route_id: &str,
        _route_kind: NotificationRouteKind,
    ) -> Result<Option<String>, NotificationAdapterError> {
        Ok(None)
    }

    fn persist_inbound_cursor(
        &self,
        _project_id: &str,
        _route_id: &str,
        _route_kind: NotificationRouteKind,
        _cursor: &str,
    ) -> Result<(), NotificationAdapterError> {
        Ok(())
    }
}

pub trait TelegramTransport: Send + Sync {
    fn send_message(
        &self,
        credentials: &TelegramRouteCredentials,
        message: &str,
    ) -> Result<(), NotificationAdapterError>;

    fn fetch_replies(
        &self,
        _credentials: &TelegramRouteCredentials,
        _cursor: Option<&str>,
    ) -> Result<NotificationInboundBatch, NotificationAdapterError> {
        Ok(NotificationInboundBatch::default())
    }
}

pub trait DiscordTransport: Send + Sync {
    fn send_message(
        &self,
        credentials: &DiscordRouteCredentials,
        message: &str,
    ) -> Result<(), NotificationAdapterError>;

    fn fetch_replies(
        &self,
        _credentials: &DiscordRouteCredentials,
        _channel_target: &str,
        _cursor: Option<&str>,
    ) -> Result<NotificationInboundBatch, NotificationAdapterError> {
        Ok(NotificationInboundBatch::default())
    }
}
