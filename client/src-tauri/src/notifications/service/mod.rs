use std::{path::PathBuf, time::Duration};

use crate::{
    commands::CommandError,
    db::project_store::NotificationDispatchStatus,
    notifications::{
        DiscordTransport, FileNotificationCredentialStore, NotificationCredentialResolver,
        ReqwestDiscordTransport, ReqwestTelegramTransport, TelegramTransport,
    },
};

mod dispatch;
mod errors;
mod replies;

pub const DISPATCH_ATTEMPTED_DIAGNOSTIC: &str = "notification_adapter_dispatch_attempted";
pub const DISPATCH_FAILED_DIAGNOSTIC: &str = "notification_adapter_dispatch_failed";
pub const REPLY_RECEIVED_DIAGNOSTIC: &str = "notification_adapter_reply_received";
pub const REPLY_REJECTED_DIAGNOSTIC: &str = "notification_adapter_reply_rejected";
const DEFAULT_PENDING_BATCH_SIZE: u32 = 64;
const DEFAULT_TRANSPORT_TIMEOUT: Duration = Duration::from_secs(8);
const MAX_REPLY_MESSAGES_PER_ROUTE: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAdapterDispatchAttempt {
    pub dispatch_id: i64,
    pub action_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub outcome_status: NotificationDispatchStatus,
    pub diagnostic_code: String,
    pub diagnostic_message: String,
    pub durable_error_code: Option<String>,
    pub durable_error_message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationDispatchCycleResult {
    pub project_id: String,
    pub pending_count: u32,
    pub attempted_count: u32,
    pub sent_count: u32,
    pub failed_count: u32,
    pub attempts: Vec<NotificationAdapterDispatchAttempt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationAdapterReplyAttempt {
    pub route_id: String,
    pub route_kind: String,
    pub action_id: Option<String>,
    pub message_id: Option<String>,
    pub accepted: bool,
    pub diagnostic_code: String,
    pub diagnostic_message: String,
    pub reply_code: Option<String>,
    pub reply_message: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NotificationReplyCycleResult {
    pub project_id: String,
    pub route_count: u32,
    pub polled_route_count: u32,
    pub message_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub attempts: Vec<NotificationAdapterReplyAttempt>,
}

#[derive(Debug, Clone)]
pub struct NotificationDispatchService<Credentials, Telegram, Discord> {
    credential_store: Credentials,
    telegram_transport: Telegram,
    discord_transport: Discord,
    pending_batch_size: u32,
}

impl
    NotificationDispatchService<
        FileNotificationCredentialStore,
        ReqwestTelegramTransport,
        ReqwestDiscordTransport,
    >
{
    pub fn from_credential_store_path(path: PathBuf) -> Result<Self, CommandError> {
        let telegram = ReqwestTelegramTransport::new(DEFAULT_TRANSPORT_TIMEOUT)
            .map_err(errors::command_error_from_adapter)?;
        let discord = ReqwestDiscordTransport::new(DEFAULT_TRANSPORT_TIMEOUT)
            .map_err(errors::command_error_from_adapter)?;

        Ok(Self::new(
            FileNotificationCredentialStore::new(path),
            telegram,
            discord,
        ))
    }
}

impl<Credentials, Telegram, Discord> NotificationDispatchService<Credentials, Telegram, Discord>
where
    Credentials: NotificationCredentialResolver,
    Telegram: TelegramTransport,
    Discord: DiscordTransport,
{
    pub fn new(
        credential_store: Credentials,
        telegram_transport: Telegram,
        discord_transport: Discord,
    ) -> Self {
        Self {
            credential_store,
            telegram_transport,
            discord_transport,
            pending_batch_size: DEFAULT_PENDING_BATCH_SIZE,
        }
    }

    pub fn with_pending_batch_size(mut self, pending_batch_size: u32) -> Self {
        self.pending_batch_size = pending_batch_size.max(1).min(DEFAULT_PENDING_BATCH_SIZE);
        self
    }
}
