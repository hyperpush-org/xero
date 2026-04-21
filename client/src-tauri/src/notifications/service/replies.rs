use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use tauri::{AppHandle, Runtime};

use crate::{
    auth::now_timestamp,
    commands::{
        submit_notification_reply::submit_notification_reply, CommandError, CommandResult,
        SubmitNotificationReplyRequestDto, SubmitNotificationReplyResponseDto,
    },
    db::project_store::{self, NotificationRouteRecord},
    notifications::{
        discord, route_target::parse_notification_route_target_for_kind, telegram,
        NotificationAdapterError, NotificationInboundMessage, NotificationRouteKind,
        RouteCredentials,
    },
};

use super::{
    NotificationAdapterReplyAttempt, NotificationDispatchService, NotificationReplyCycleResult,
    MAX_REPLY_MESSAGES_PER_ROUTE, REPLY_RECEIVED_DIAGNOSTIC, REPLY_REJECTED_DIAGNOSTIC,
};

impl<Credentials, Telegram, Discord> NotificationDispatchService<Credentials, Telegram, Discord>
where
    Credentials: crate::notifications::NotificationCredentialResolver,
    Telegram: crate::notifications::TelegramTransport,
    Discord: crate::notifications::DiscordTransport,
{
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
                (NotificationRouteKind::Telegram, RouteCredentials::Telegram(credentials)) => {
                    self.telegram_transport
                        .fetch_replies(&credentials, route_cursor.as_deref())
                }
                (NotificationRouteKind::Discord, RouteCredentials::Discord(credentials)) => {
                    self.discord_transport.fetch_replies(
                        &credentials,
                        channel_target.as_str(),
                        route_cursor.as_deref(),
                    )
                }
                (expected_kind, _) => {
                    Err(NotificationAdapterError::credentials_malformed(format!(
                        "Cadence found malformed app-local credentials for route `{}` because expected `{}` credentials were missing.",
                        route.route_id,
                        expected_kind.as_str(),
                    )))
                }
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
        message: NotificationInboundMessage,
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
