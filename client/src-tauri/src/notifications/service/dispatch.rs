use std::{collections::HashMap, path::Path};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, OperatorApprovalDto},
    db::project_store::{
        self, NotificationDispatchOutcomeUpdateRecord, NotificationDispatchRecord,
        NotificationDispatchStatus, NotificationRouteRecord,
    },
    notifications::{
        route_target::parse_notification_route_target_for_kind, NotificationAdapterError,
        NotificationRouteKind, RouteCredentials,
    },
};

use super::{
    NotificationAdapterDispatchAttempt, NotificationDispatchCycleResult,
    NotificationDispatchService, DISPATCH_ATTEMPTED_DIAGNOSTIC, DISPATCH_FAILED_DIAGNOSTIC,
};

impl<Credentials, Telegram, Discord> NotificationDispatchService<Credentials, Telegram, Discord>
where
    Credentials: crate::notifications::NotificationCredentialResolver,
    Telegram: crate::notifications::TelegramTransport,
    Discord: crate::notifications::DiscordTransport,
{
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
                            "Xero sent notification dispatch `{}` for route `{}`.",
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
                                "Xero could not persist the notification dispatch outcome.".into(),
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
                "Xero could not persist one or more notification dispatch outcomes.",
            ));
        }

        Ok(cycle_result)
    }

    fn send_dispatch(
        &self,
        route: Option<&NotificationRouteRecord>,
        action: Option<&OperatorApprovalDto>,
        dispatch: &NotificationDispatchRecord,
    ) -> Result<(), NotificationAdapterError> {
        let route = route.ok_or_else(|| {
            NotificationAdapterError::payload_invalid(format!(
                "Xero could not send dispatch `{}` because route `{}` no longer exists.",
                dispatch.id, dispatch.route_id
            ))
        })?;

        if !route.enabled {
            return Err(NotificationAdapterError::new(
                "notification_adapter_route_disabled",
                format!(
                    "Xero skipped dispatch `{}` because route `{}` is disabled.",
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
                "Xero found malformed app-local credentials for route `{}` because expected `{}` credentials were missing.",
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
        .unwrap_or("Xero paused and requires a coarse operator answer to continue.");

    format!(
        "Xero requires operator input.\n\nRoute: {}:{}\nAction ID: {}\nAction Type: {}\nTitle: {}\nDetail: {}\n\nCorrelation key: {}\nReply with one line:\napprove {} <answer>\nreject {} <answer>",
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
