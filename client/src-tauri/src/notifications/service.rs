use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
    time::Duration,
};

use tauri::{AppHandle, Runtime};

use crate::{
    auth::now_timestamp,
    commands::{
        submit_notification_reply::submit_notification_reply, CommandError, CommandResult,
        OperatorApprovalDto, SubmitNotificationReplyRequestDto, SubmitNotificationReplyResponseDto,
    },
    db::project_store::{
        self, NotificationDispatchOutcomeUpdateRecord, NotificationDispatchRecord,
        NotificationDispatchStatus, NotificationRouteRecord,
    },
    notifications::{
        discord, route_target::parse_notification_route_target_for_kind, telegram,
        DiscordTransport, FileNotificationCredentialStore, NotificationAdapterError,
        NotificationCredentialResolver, NotificationRouteKind, ReqwestDiscordTransport,
        ReqwestTelegramTransport, RouteCredentials, TelegramTransport,
    },
};

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
            .map_err(command_error_from_adapter)?;
        let discord = ReqwestDiscordTransport::new(DEFAULT_TRANSPORT_TIMEOUT)
            .map_err(command_error_from_adapter)?;

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

    pub fn dispatch_pending_for_project(
        &self,
        repo_root: &Path,
        project_id: &str,
    ) -> Result<NotificationDispatchCycleResult, CommandError> {
        if project_id.trim().is_empty() {
            return Err(CommandError::invalid_request("projectId"));
        }

        let pending_dispatches = project_store::load_pending_notification_dispatches(
            repo_root,
            project_id,
            Some(self.pending_batch_size),
        )?;

        let mut cycle_result = NotificationDispatchCycleResult {
            project_id: project_id.to_string(),
            pending_count: pending_dispatches.len() as u32,
            ..NotificationDispatchCycleResult::default()
        };

        if pending_dispatches.is_empty() {
            return Ok(cycle_result);
        }

        let routes = load_route_lookup(repo_root, project_id)?;
        let approvals = load_approval_lookup(repo_root, project_id)?;
        let mut persistence_errors = Vec::new();

        for dispatch in pending_dispatches {
            let attempted_at = now_timestamp();
            let route = routes.get(&dispatch.route_id);

            let send_result =
                self.send_dispatch(route, approvals.get(dispatch.action_id.as_str()), &dispatch);

            let (status, error_code, error_message, diagnostic_code, diagnostic_message) =
                match send_result {
                    Ok(()) => (
                        NotificationDispatchStatus::Sent,
                        None,
                        None,
                        DISPATCH_ATTEMPTED_DIAGNOSTIC.to_string(),
                        format!(
                            "Cadence sent notification dispatch `{}` for route `{}`.",
                            dispatch.id, dispatch.route_id
                        ),
                    ),
                    Err(error) => (
                        NotificationDispatchStatus::Failed,
                        Some(error.code.clone()),
                        Some(error.message.clone()),
                        DISPATCH_FAILED_DIAGNOSTIC.to_string(),
                        error.message,
                    ),
                };

            let persisted = project_store::record_notification_dispatch_outcome(
                repo_root,
                &NotificationDispatchOutcomeUpdateRecord {
                    project_id: dispatch.project_id.clone(),
                    action_id: dispatch.action_id.clone(),
                    route_id: dispatch.route_id.clone(),
                    status,
                    attempted_at,
                    error_code: error_code.clone(),
                    error_message: error_message.clone(),
                },
            );

            match persisted {
                Ok(dispatch_after_outcome) => {
                    cycle_result.attempted_count = cycle_result.attempted_count.saturating_add(1);
                    if dispatch_after_outcome.status == NotificationDispatchStatus::Sent {
                        cycle_result.sent_count = cycle_result.sent_count.saturating_add(1);
                    } else {
                        cycle_result.failed_count = cycle_result.failed_count.saturating_add(1);
                    }

                    cycle_result
                        .attempts
                        .push(NotificationAdapterDispatchAttempt {
                            dispatch_id: dispatch_after_outcome.id,
                            action_id: dispatch_after_outcome.action_id,
                            route_id: dispatch_after_outcome.route_id,
                            route_kind: route
                                .map(|route| route.route_kind.clone())
                                .unwrap_or_else(|| "unknown".into()),
                            outcome_status: dispatch_after_outcome.status,
                            diagnostic_code,
                            diagnostic_message,
                            durable_error_code: dispatch_after_outcome.last_error_code,
                            durable_error_message: dispatch_after_outcome.last_error_message,
                        });
                }
                Err(error) => {
                    persistence_errors.push(error.clone());
                    cycle_result.attempted_count = cycle_result.attempted_count.saturating_add(1);
                    cycle_result.failed_count = cycle_result.failed_count.saturating_add(1);
                    cycle_result
                        .attempts
                        .push(NotificationAdapterDispatchAttempt {
                            dispatch_id: dispatch.id,
                            action_id: dispatch.action_id,
                            route_id: dispatch.route_id,
                            route_kind: route
                                .map(|route| route.route_kind.clone())
                                .unwrap_or_else(|| "unknown".into()),
                            outcome_status: NotificationDispatchStatus::Failed,
                            diagnostic_code: DISPATCH_FAILED_DIAGNOSTIC.into(),
                            diagnostic_message:
                                "Cadence could not persist the notification dispatch outcome."
                                    .into(),
                            durable_error_code: Some(
                                "notification_adapter_outcome_persist_failed".into(),
                            ),
                            durable_error_message: Some(error.message),
                        });
                }
            }
        }

        if !persistence_errors.is_empty() {
            return Err(CommandError::retryable(
                "notification_adapter_outcome_persist_failed",
                "Cadence could not persist one or more notification dispatch outcomes.",
            ));
        }

        Ok(cycle_result)
    }

    pub fn ingest_replies_for_project<R: Runtime>(
        &self,
        app: AppHandle<R>,
        repo_root: &Path,
        project_id: &str,
    ) -> Result<NotificationReplyCycleResult, CommandError> {
        self.ingest_replies_for_project_with_submitter(repo_root, project_id, |request| {
            submit_notification_reply(app.clone(), request)
        })
    }

    pub fn ingest_replies_for_project_with_submitter<Submit>(
        &self,
        repo_root: &Path,
        project_id: &str,
        mut submit_reply: Submit,
    ) -> Result<NotificationReplyCycleResult, CommandError>
    where
        Submit: FnMut(
            SubmitNotificationReplyRequestDto,
        ) -> CommandResult<SubmitNotificationReplyResponseDto>,
    {
        if project_id.trim().is_empty() {
            return Err(CommandError::invalid_request("projectId"));
        }

        let routes = project_store::load_notification_routes(repo_root, project_id)?;
        let dispatch_lookup = load_dispatch_lookup(repo_root, project_id)?;

        let mut cycle = NotificationReplyCycleResult {
            project_id: project_id.to_string(),
            route_count: routes.len() as u32,
            ..NotificationReplyCycleResult::default()
        };

        for route in routes {
            if !route.enabled {
                continue;
            }
            cycle.polled_route_count = cycle.polled_route_count.saturating_add(1);

            let route_kind = match NotificationRouteKind::parse(route.route_kind.as_str()) {
                Ok(route_kind) => route_kind,
                Err(error) => {
                    record_reply_rejection(
                        &mut cycle,
                        &route,
                        None,
                        None,
                        error.code,
                        error.message,
                    );
                    continue;
                }
            };

            let channel_target = match parse_notification_route_target_for_kind(
                route_kind,
                route.route_target.as_str(),
            ) {
                Ok(route_target) => route_target.channel_target,
                Err(error) => {
                    record_reply_rejection(
                        &mut cycle,
                        &route,
                        None,
                        None,
                        error.code,
                        error.message,
                    );
                    continue;
                }
            };

            let credentials = match self.credential_store.resolve_route_credentials(
                project_id,
                route.route_id.as_str(),
                route_kind,
            ) {
                Ok(credentials) => credentials,
                Err(error) => {
                    record_reply_rejection(
                        &mut cycle,
                        &route,
                        None,
                        None,
                        error.code,
                        error.message,
                    );
                    continue;
                }
            };

            let stored_cursor = match self.credential_store.load_inbound_cursor(
                project_id,
                route.route_id.as_str(),
                route_kind,
            ) {
                Ok(cursor) => cursor,
                Err(error) => {
                    record_reply_rejection(
                        &mut cycle,
                        &route,
                        None,
                        None,
                        error.code,
                        error.message,
                    );
                    None
                }
            };

            let route_cursor = sanitize_route_cursor(route_kind, stored_cursor.as_deref());
            if stored_cursor.is_some() && route_cursor.is_none() {
                record_reply_rejection(
                    &mut cycle,
                    &route,
                    None,
                    None,
                    "notification_adapter_cursor_malformed".into(),
                    format!(
                        "Cadence reset malformed inbound cursor state for route `{}` to conservative replay mode.",
                        route.route_id
                    ),
                );
            }

            let fetched = match (route_kind, credentials) {
                (NotificationRouteKind::Telegram, RouteCredentials::Telegram(credentials)) => self
                    .telegram_transport
                    .fetch_replies(&credentials, route_cursor.as_deref()),
                (NotificationRouteKind::Discord, RouteCredentials::Discord(credentials)) => self
                    .discord_transport
                    .fetch_replies(
                        &credentials,
                        channel_target.as_str(),
                        route_cursor.as_deref(),
                    ),
                (expected_kind, _) => Err(NotificationAdapterError::credentials_malformed(format!(
                    "Cadence found malformed app-local credentials for route `{}` because expected `{}` credentials were missing.",
                    route.route_id,
                    expected_kind.as_str(),
                ))),
            };

            let mut inbound_batch = match fetched {
                Ok(batch) => batch,
                Err(error) => {
                    record_reply_rejection(
                        &mut cycle,
                        &route,
                        None,
                        None,
                        error.code,
                        error.message,
                    );
                    continue;
                }
            };

            if inbound_batch.messages.len() > MAX_REPLY_MESSAGES_PER_ROUTE {
                let dropped = inbound_batch.messages.len() - MAX_REPLY_MESSAGES_PER_ROUTE;
                inbound_batch
                    .messages
                    .truncate(MAX_REPLY_MESSAGES_PER_ROUTE);
                record_reply_rejection(
                    &mut cycle,
                    &route,
                    None,
                    None,
                    "notification_adapter_reply_batch_truncated".into(),
                    format!(
                        "Cadence truncated {dropped} inbound replies for route `{}` to keep duplicate floods bounded.",
                        route.route_id
                    ),
                );
            }

            let mut seen_message_ids = HashSet::new();
            for message in inbound_batch.messages {
                cycle.message_count = cycle.message_count.saturating_add(1);

                if !seen_message_ids.insert(message.message_id.clone()) {
                    record_reply_rejection(
                        &mut cycle,
                        &route,
                        None,
                        Some(message.message_id),
                        "notification_adapter_reply_duplicate".into(),
                        format!(
                            "Cadence skipped duplicate inbound reply message for route `{}`.",
                            route.route_id
                        ),
                    );
                    continue;
                }

                self.process_inbound_message(
                    &mut cycle,
                    &route,
                    route_kind,
                    project_id,
                    &dispatch_lookup,
                    message,
                    &mut submit_reply,
                );
            }

            if let Some(next_cursor) = inbound_batch.next_cursor.as_deref() {
                let next_cursor = next_cursor.trim();
                if !next_cursor.is_empty() && route_cursor.as_deref() != Some(next_cursor) {
                    if let Err(error) = self.credential_store.persist_inbound_cursor(
                        project_id,
                        route.route_id.as_str(),
                        route_kind,
                        next_cursor,
                    ) {
                        record_reply_rejection(
                            &mut cycle,
                            &route,
                            None,
                            None,
                            error.code,
                            error.message,
                        );
                    }
                }
            }
        }

        Ok(cycle)
    }

    fn process_inbound_message<Submit>(
        &self,
        cycle: &mut NotificationReplyCycleResult,
        route: &NotificationRouteRecord,
        route_kind: NotificationRouteKind,
        project_id: &str,
        dispatch_lookup: &HashMap<(String, String), String>,
        message: crate::notifications::NotificationInboundMessage,
        submit_reply: &mut Submit,
    ) where
        Submit: FnMut(
            SubmitNotificationReplyRequestDto,
        ) -> CommandResult<SubmitNotificationReplyResponseDto>,
    {
        let parsed = match route_kind {
            NotificationRouteKind::Telegram => {
                telegram::parse_reply_envelope(message.body.as_str())
            }
            NotificationRouteKind::Discord => discord::parse_reply_envelope(message.body.as_str()),
        };

        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(error) => {
                record_reply_rejection(
                    cycle,
                    route,
                    None,
                    Some(message.message_id),
                    error.code,
                    error.message,
                );
                return;
            }
        };

        let action_id = message.context_action_id.or_else(|| {
            dispatch_lookup
                .get(&(route.route_id.clone(), parsed.correlation_key.clone()))
                .cloned()
        });

        let Some(action_id) = action_id else {
            record_reply_rejection(
                cycle,
                route,
                None,
                Some(message.message_id),
                "notification_reply_correlation_invalid".into(),
                format!(
                    "Cadence rejected inbound route `{}` reply because no dispatch matched correlation key `{}`.",
                    route.route_id, parsed.correlation_key
                ),
            );
            return;
        };

        let request = SubmitNotificationReplyRequestDto {
            project_id: project_id.to_string(),
            action_id: action_id.clone(),
            route_id: route.route_id.clone(),
            correlation_key: parsed.correlation_key,
            responder_id: message.responder_id,
            reply_text: parsed.reply_text,
            decision: parsed.decision,
            received_at: normalize_received_at(message.received_at.as_str()),
        };

        match submit_reply(request) {
            Ok(_) => record_reply_acceptance(cycle, route, action_id, Some(message.message_id)),
            Err(error) => record_reply_rejection(
                cycle,
                route,
                Some(action_id),
                Some(message.message_id),
                error.code,
                error.message,
            ),
        }
    }

    fn send_dispatch(
        &self,
        route: Option<&NotificationRouteRecord>,
        action: Option<&OperatorApprovalDto>,
        dispatch: &NotificationDispatchRecord,
    ) -> Result<(), NotificationAdapterError> {
        let route = route.ok_or_else(|| {
            NotificationAdapterError::payload_invalid(format!(
                "Cadence could not send dispatch `{}` because route `{}` no longer exists.",
                dispatch.id, dispatch.route_id
            ))
        })?;

        if !route.enabled {
            return Err(NotificationAdapterError::new(
                "notification_adapter_route_disabled",
                format!(
                    "Cadence skipped dispatch `{}` because route `{}` is disabled.",
                    dispatch.id, dispatch.route_id
                ),
                false,
            ));
        }

        let route_kind = NotificationRouteKind::parse(route.route_kind.as_str())?;
        let route_target =
            parse_notification_route_target_for_kind(route_kind, route.route_target.as_str())?;
        let channel_target = route_target.channel_target;
        let message = format_dispatch_message(route_kind, &channel_target, action, dispatch);
        let credentials = self.credential_store.resolve_route_credentials(
            dispatch.project_id.as_str(),
            dispatch.route_id.as_str(),
            route_kind,
        )?;

        match (route_kind, credentials) {
            (NotificationRouteKind::Telegram, RouteCredentials::Telegram(credentials)) => {
                self.telegram_transport.send_message(&credentials, &message)
            }
            (NotificationRouteKind::Discord, RouteCredentials::Discord(credentials)) => {
                self.discord_transport.send_message(&credentials, &message)
            }
            (expected_kind, _) => Err(NotificationAdapterError::credentials_malformed(format!(
                "Cadence found malformed app-local credentials for route `{}` because expected `{}` credentials were missing.",
                dispatch.route_id,
                expected_kind.as_str(),
            ))),
        }
    }
}

fn load_route_lookup(
    repo_root: &Path,
    project_id: &str,
) -> Result<HashMap<String, NotificationRouteRecord>, CommandError> {
    let routes = project_store::load_notification_routes(repo_root, project_id)?;
    Ok(routes
        .into_iter()
        .map(|route| (route.route_id.clone(), route))
        .collect())
}

fn load_approval_lookup(
    repo_root: &Path,
    project_id: &str,
) -> Result<HashMap<String, OperatorApprovalDto>, CommandError> {
    let snapshot = project_store::load_project_snapshot(repo_root, project_id)?.snapshot;
    Ok(snapshot
        .approval_requests
        .into_iter()
        .map(|approval| (approval.action_id.clone(), approval))
        .collect())
}

fn load_dispatch_lookup(
    repo_root: &Path,
    project_id: &str,
) -> Result<HashMap<(String, String), String>, CommandError> {
    let dispatches = project_store::load_notification_dispatches(repo_root, project_id, None)?;
    let mut lookup = HashMap::new();

    for dispatch in dispatches {
        lookup
            .entry((dispatch.route_id.clone(), dispatch.correlation_key.clone()))
            .or_insert(dispatch.action_id);
    }

    Ok(lookup)
}

fn sanitize_route_cursor(route_kind: NotificationRouteKind, value: Option<&str>) -> Option<String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty())?;

    let valid = match route_kind {
        NotificationRouteKind::Telegram => telegram::is_valid_cursor(value),
        NotificationRouteKind::Discord => discord::is_valid_cursor(value),
    };

    if valid {
        Some(value.to_string())
    } else {
        None
    }
}

fn normalize_received_at(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        now_timestamp()
    } else {
        value.to_string()
    }
}

fn record_reply_acceptance(
    cycle: &mut NotificationReplyCycleResult,
    route: &NotificationRouteRecord,
    action_id: String,
    message_id: Option<String>,
) {
    cycle.accepted_count = cycle.accepted_count.saturating_add(1);
    cycle.attempts.push(NotificationAdapterReplyAttempt {
        route_id: route.route_id.clone(),
        route_kind: route.route_kind.clone(),
        action_id: Some(action_id),
        message_id,
        accepted: true,
        diagnostic_code: REPLY_RECEIVED_DIAGNOSTIC.into(),
        diagnostic_message: format!(
            "Cadence accepted inbound reply for route `{}` and resumed the correlated action through the existing broker path.",
            route.route_id
        ),
        reply_code: None,
        reply_message: None,
    });
}

fn record_reply_rejection(
    cycle: &mut NotificationReplyCycleResult,
    route: &NotificationRouteRecord,
    action_id: Option<String>,
    message_id: Option<String>,
    code: String,
    message: String,
) {
    cycle.rejected_count = cycle.rejected_count.saturating_add(1);
    cycle.attempts.push(NotificationAdapterReplyAttempt {
        route_id: route.route_id.clone(),
        route_kind: route.route_kind.clone(),
        action_id,
        message_id,
        accepted: false,
        diagnostic_code: REPLY_REJECTED_DIAGNOSTIC.into(),
        diagnostic_message: format!(
            "Cadence rejected inbound reply for route `{}` with `{}`.",
            route.route_id, code
        ),
        reply_code: Some(code),
        reply_message: Some(message),
    });
}

fn format_dispatch_message(
    route_kind: NotificationRouteKind,
    channel_target: &str,
    action: Option<&OperatorApprovalDto>,
    dispatch: &NotificationDispatchRecord,
) -> String {
    let action_type = action
        .map(|action| action.action_type.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("terminal_input_required");
    let title = action
        .map(|action| action.title.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("Operator input required");
    let detail = action
        .map(|action| action.detail.trim())
        .filter(|value| !value.is_empty())
        .unwrap_or("Cadence paused and requires a coarse operator answer to continue.");

    format!(
        "Cadence requires operator input.\n\nRoute: {}:{}\nAction ID: {}\nAction Type: {}\nTitle: {}\nDetail: {}\n\nCorrelation key: {}\nReply with one line:\napprove {} <answer>\nreject {} <answer>",
        route_kind.as_str(),
        channel_target,
        dispatch.action_id,
        action_type,
        title,
        detail,
        dispatch.correlation_key,
        dispatch.correlation_key,
        dispatch.correlation_key,
    )
}

fn command_error_from_adapter(error: NotificationAdapterError) -> CommandError {
    if error.retryable {
        CommandError::retryable(error.code, error.message)
    } else {
        CommandError::user_fixable(error.code, error.message)
    }
}
