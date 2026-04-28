use std::{collections::BTreeMap, path::Path};

use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        validate_non_empty, CommandResult, NotificationAdapterDispatchAttemptDto,
        NotificationAdapterErrorCountDto, NotificationAdapterReplyAttemptDto,
        NotificationDispatchCycleSummaryDto, NotificationDispatchStatusDto,
        NotificationReplyCycleSummaryDto, SyncNotificationAdaptersRequestDto,
        SyncNotificationAdaptersResponseDto,
    },
    db::project_store::NotificationDispatchStatus,
    notifications::{
        service::{NotificationDispatchCycleResult, NotificationReplyCycleResult},
        DiscordTransport, NotificationCredentialResolver, NotificationDispatchService,
        TelegramTransport,
    },
    state::DesktopState,
};

use super::runtime_support::resolve_project_root;

const MAX_DISPATCH_ATTEMPTS_IN_RESPONSE: usize = 64;
const MAX_REPLY_ATTEMPTS_IN_RESPONSE: usize = 256;

#[tauri::command]
pub fn sync_notification_adapters<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SyncNotificationAdaptersRequestDto,
) -> CommandResult<SyncNotificationAdaptersResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let credential_store_path = state.global_db_path(&app)?;
    let service = NotificationDispatchService::from_credential_store_path(credential_store_path)?;

    sync_notification_adapters_with_service(app, &repo_root, &request.project_id, &service)
}

pub fn sync_notification_adapters_with_service<R, Credentials, Telegram, Discord>(
    app: AppHandle<R>,
    repo_root: &Path,
    project_id: &str,
    service: &NotificationDispatchService<Credentials, Telegram, Discord>,
) -> CommandResult<SyncNotificationAdaptersResponseDto>
where
    R: Runtime,
    Credentials: NotificationCredentialResolver,
    Telegram: TelegramTransport,
    Discord: DiscordTransport,
{
    validate_non_empty(project_id, "projectId")?;

    let dispatch_cycle = service.dispatch_pending_for_project(repo_root, project_id)?;
    let reply_cycle = service.ingest_replies_for_project(app, repo_root, project_id)?;

    Ok(SyncNotificationAdaptersResponseDto {
        project_id: project_id.to_string(),
        dispatch: map_dispatch_cycle(dispatch_cycle),
        replies: map_reply_cycle(reply_cycle),
        synced_at: now_timestamp(),
    })
}

fn map_dispatch_cycle(
    cycle: NotificationDispatchCycleResult,
) -> NotificationDispatchCycleSummaryDto {
    let total_attempts = cycle.attempts.len();
    let error_code_counts = collect_error_code_counts(
        cycle
            .attempts
            .iter()
            .filter_map(|attempt| attempt.durable_error_code.as_deref()),
    );

    NotificationDispatchCycleSummaryDto {
        project_id: cycle.project_id,
        pending_count: cycle.pending_count,
        attempted_count: cycle.attempted_count,
        sent_count: cycle.sent_count,
        failed_count: cycle.failed_count,
        attempt_limit: MAX_DISPATCH_ATTEMPTS_IN_RESPONSE as u32,
        attempts_truncated: total_attempts > MAX_DISPATCH_ATTEMPTS_IN_RESPONSE,
        attempts: cycle
            .attempts
            .into_iter()
            .take(MAX_DISPATCH_ATTEMPTS_IN_RESPONSE)
            .map(|attempt| NotificationAdapterDispatchAttemptDto {
                dispatch_id: attempt.dispatch_id,
                action_id: attempt.action_id,
                route_id: attempt.route_id,
                route_kind: normalize_route_kind(attempt.route_kind),
                outcome_status: map_dispatch_status(attempt.outcome_status),
                diagnostic_code: attempt.diagnostic_code,
                diagnostic_message: attempt.diagnostic_message,
                durable_error_code: normalize_optional_text(attempt.durable_error_code),
                durable_error_message: normalize_optional_text(attempt.durable_error_message),
            })
            .collect(),
        error_code_counts,
    }
}

fn map_reply_cycle(cycle: NotificationReplyCycleResult) -> NotificationReplyCycleSummaryDto {
    let total_attempts = cycle.attempts.len();
    let error_code_counts = collect_error_code_counts(
        cycle
            .attempts
            .iter()
            .filter_map(|attempt| attempt.reply_code.as_deref()),
    );

    NotificationReplyCycleSummaryDto {
        project_id: cycle.project_id,
        route_count: cycle.route_count,
        polled_route_count: cycle.polled_route_count,
        message_count: cycle.message_count,
        accepted_count: cycle.accepted_count,
        rejected_count: cycle.rejected_count,
        attempt_limit: MAX_REPLY_ATTEMPTS_IN_RESPONSE as u32,
        attempts_truncated: total_attempts > MAX_REPLY_ATTEMPTS_IN_RESPONSE,
        attempts: cycle
            .attempts
            .into_iter()
            .take(MAX_REPLY_ATTEMPTS_IN_RESPONSE)
            .map(|attempt| NotificationAdapterReplyAttemptDto {
                route_id: attempt.route_id,
                route_kind: normalize_route_kind(attempt.route_kind),
                action_id: normalize_optional_text(attempt.action_id),
                message_id: normalize_optional_text(attempt.message_id),
                accepted: attempt.accepted,
                diagnostic_code: attempt.diagnostic_code,
                diagnostic_message: attempt.diagnostic_message,
                reply_code: normalize_optional_text(attempt.reply_code),
                reply_message: normalize_optional_text(attempt.reply_message),
            })
            .collect(),
        error_code_counts,
    }
}

fn map_dispatch_status(status: NotificationDispatchStatus) -> NotificationDispatchStatusDto {
    match status {
        NotificationDispatchStatus::Pending => NotificationDispatchStatusDto::Pending,
        NotificationDispatchStatus::Sent => NotificationDispatchStatusDto::Sent,
        NotificationDispatchStatus::Failed => NotificationDispatchStatusDto::Failed,
        NotificationDispatchStatus::Claimed => NotificationDispatchStatusDto::Claimed,
    }
}

fn collect_error_code_counts<'a>(
    codes: impl Iterator<Item = &'a str>,
) -> Vec<NotificationAdapterErrorCountDto> {
    let mut counts = BTreeMap::<String, u32>::new();

    for code in codes {
        let trimmed = code.trim();
        if trimmed.is_empty() {
            continue;
        }

        counts
            .entry(trimmed.to_string())
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
    }

    counts
        .into_iter()
        .map(|(code, count)| NotificationAdapterErrorCountDto { code, count })
        .collect()
}

fn normalize_route_kind(value: String) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}

fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
