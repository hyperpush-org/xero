use serde::{Deserialize, Serialize};

use crate::db::project_store;

use super::{
    error::{CommandError, CommandResult},
    workflow::{ResolveOperatorActionResponseDto, ResumeOperatorRunResponseDto},
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationRouteKindDto {
    Telegram,
    Discord,
}

impl NotificationRouteKindDto {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Telegram => "telegram",
            Self::Discord => "discord",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationRouteCredentialReadinessStatusDto {
    Ready,
    Missing,
    Malformed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteCredentialReadinessDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteCredentialReadinessDto {
    pub has_bot_token: bool,
    pub has_chat_id: bool,
    pub has_webhook_url: bool,
    pub ready: bool,
    pub status: NotificationRouteCredentialReadinessStatusDto,
    pub diagnostic: Option<NotificationRouteCredentialReadinessDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: NotificationRouteKindDto,
    pub route_target: String,
    pub enabled: bool,
    pub metadata_json: Option<String>,
    pub credential_readiness: Option<NotificationRouteCredentialReadinessDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationRoutesRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationRoutesResponseDto {
    pub routes: Vec<NotificationRouteDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteRequestDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub route_target: String,
    pub enabled: bool,
    pub metadata_json: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteResponseDto {
    pub route: NotificationRouteDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationRouteCredentialPayloadDto {
    pub bot_token: Option<String>,
    pub chat_id: Option<String>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteCredentialsRequestDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub credentials: NotificationRouteCredentialPayloadDto,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertNotificationRouteCredentialsResponseDto {
    pub project_id: String,
    pub route_id: String,
    pub route_kind: NotificationRouteKindDto,
    pub credential_scope: String,
    pub has_bot_token: bool,
    pub has_chat_id: bool,
    pub has_webhook_url: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationDispatchesRequestDto {
    pub project_id: String,
    pub action_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationDispatchOutcomeStatusDto {
    Sent,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordNotificationDispatchOutcomeRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub status: NotificationDispatchOutcomeStatusDto,
    pub attempted_at: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitNotificationReplyRequestDto {
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub responder_id: Option<String>,
    pub reply_text: String,
    pub decision: String,
    pub received_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationDispatchStatusDto {
    Pending,
    Sent,
    Failed,
    Claimed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NotificationReplyClaimStatusDto {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationDispatchDto {
    pub id: i64,
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub status: NotificationDispatchStatusDto,
    pub attempt_count: u32,
    pub last_attempt_at: Option<String>,
    pub delivered_at: Option<String>,
    pub claimed_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error_message: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationReplyClaimDto {
    pub id: i64,
    pub project_id: String,
    pub action_id: String,
    pub route_id: String,
    pub correlation_key: String,
    pub responder_id: Option<String>,
    pub status: NotificationReplyClaimStatusDto,
    pub rejection_code: Option<String>,
    pub rejection_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListNotificationDispatchesResponseDto {
    pub dispatches: Vec<NotificationDispatchDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RecordNotificationDispatchOutcomeResponseDto {
    pub dispatch: NotificationDispatchDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitNotificationReplyResponseDto {
    pub claim: NotificationReplyClaimDto,
    pub dispatch: NotificationDispatchDto,
    pub resolve_result: ResolveOperatorActionResponseDto,
    pub resume_result: Option<ResumeOperatorRunResponseDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncNotificationAdaptersRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAdapterErrorCountDto {
    pub code: String,
    pub count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAdapterDispatchAttemptDto {
    pub dispatch_id: i64,
    pub action_id: String,
    pub route_id: String,
    pub route_kind: String,
    pub outcome_status: NotificationDispatchStatusDto,
    pub diagnostic_code: String,
    pub diagnostic_message: String,
    pub durable_error_code: Option<String>,
    pub durable_error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationDispatchCycleSummaryDto {
    pub project_id: String,
    pub pending_count: u32,
    pub attempted_count: u32,
    pub sent_count: u32,
    pub failed_count: u32,
    pub attempt_limit: u32,
    pub attempts_truncated: bool,
    pub attempts: Vec<NotificationAdapterDispatchAttemptDto>,
    pub error_code_counts: Vec<NotificationAdapterErrorCountDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationAdapterReplyAttemptDto {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationReplyCycleSummaryDto {
    pub project_id: String,
    pub route_count: u32,
    pub polled_route_count: u32,
    pub message_count: u32,
    pub accepted_count: u32,
    pub rejected_count: u32,
    pub attempt_limit: u32,
    pub attempts_truncated: bool,
    pub attempts: Vec<NotificationAdapterReplyAttemptDto>,
    pub error_code_counts: Vec<NotificationAdapterErrorCountDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncNotificationAdaptersResponseDto {
    pub project_id: String,
    pub dispatch: NotificationDispatchCycleSummaryDto,
    pub replies: NotificationReplyCycleSummaryDto,
    pub synced_at: String,
}

pub(crate) fn map_notification_route_record(
    route: project_store::NotificationRouteRecord,
    credential_readiness: Option<NotificationRouteCredentialReadinessDto>,
) -> CommandResult<NotificationRouteDto> {
    Ok(NotificationRouteDto {
        project_id: route.project_id,
        route_id: route.route_id,
        route_kind: parse_notification_route_kind(
            &route.route_kind,
            "notification_route_decode_failed",
        )?,
        route_target: route.route_target,
        enabled: route.enabled,
        metadata_json: route.metadata_json,
        credential_readiness,
        created_at: route.created_at,
        updated_at: route.updated_at,
    })
}

pub(crate) fn map_notification_dispatch_record(
    dispatch: project_store::NotificationDispatchRecord,
) -> NotificationDispatchDto {
    NotificationDispatchDto {
        id: dispatch.id,
        project_id: dispatch.project_id,
        action_id: dispatch.action_id,
        route_id: dispatch.route_id,
        correlation_key: dispatch.correlation_key,
        status: map_notification_dispatch_status(dispatch.status),
        attempt_count: dispatch.attempt_count,
        last_attempt_at: dispatch.last_attempt_at,
        delivered_at: dispatch.delivered_at,
        claimed_at: dispatch.claimed_at,
        last_error_code: dispatch.last_error_code,
        last_error_message: dispatch.last_error_message,
        created_at: dispatch.created_at,
        updated_at: dispatch.updated_at,
    }
}

pub(crate) fn map_notification_reply_claim_record(
    claim: project_store::NotificationReplyClaimRecord,
) -> NotificationReplyClaimDto {
    NotificationReplyClaimDto {
        id: claim.id,
        project_id: claim.project_id,
        action_id: claim.action_id,
        route_id: claim.route_id,
        correlation_key: claim.correlation_key,
        responder_id: claim.responder_id,
        status: map_notification_reply_claim_status(claim.status),
        rejection_code: claim.rejection_code,
        rejection_message: claim.rejection_message,
        created_at: claim.created_at,
    }
}

pub(crate) fn parse_notification_route_kind(
    value: &str,
    code: &'static str,
) -> CommandResult<NotificationRouteKindDto> {
    match value.trim() {
        "telegram" => Ok(NotificationRouteKindDto::Telegram),
        "discord" => Ok(NotificationRouteKindDto::Discord),
        other => Err(CommandError::user_fixable(
            code,
            format!(
                "Xero does not support notification route kind `{other}`. Allowed kinds: telegram, discord."
            ),
        )),
    }
}

pub(crate) fn map_notification_route_credential_readiness(
    projection: crate::notifications::NotificationCredentialReadinessProjection,
) -> NotificationRouteCredentialReadinessDto {
    NotificationRouteCredentialReadinessDto {
        has_bot_token: projection.has_bot_token,
        has_chat_id: projection.has_chat_id,
        has_webhook_url: projection.has_webhook_url,
        ready: projection.ready,
        status: map_notification_route_credential_readiness_status(projection.status),
        diagnostic: projection.diagnostic.map(|diagnostic| {
            NotificationRouteCredentialReadinessDiagnosticDto {
                code: diagnostic.code,
                message: diagnostic.message,
                retryable: diagnostic.retryable,
            }
        }),
    }
}

fn map_notification_route_credential_readiness_status(
    status: crate::notifications::NotificationCredentialReadinessStatus,
) -> NotificationRouteCredentialReadinessStatusDto {
    match status {
        crate::notifications::NotificationCredentialReadinessStatus::Ready => {
            NotificationRouteCredentialReadinessStatusDto::Ready
        }
        crate::notifications::NotificationCredentialReadinessStatus::Missing => {
            NotificationRouteCredentialReadinessStatusDto::Missing
        }
        crate::notifications::NotificationCredentialReadinessStatus::Malformed => {
            NotificationRouteCredentialReadinessStatusDto::Malformed
        }
        crate::notifications::NotificationCredentialReadinessStatus::Unavailable => {
            NotificationRouteCredentialReadinessStatusDto::Unavailable
        }
    }
}

fn map_notification_dispatch_status(
    status: project_store::NotificationDispatchStatus,
) -> NotificationDispatchStatusDto {
    match status {
        project_store::NotificationDispatchStatus::Pending => {
            NotificationDispatchStatusDto::Pending
        }
        project_store::NotificationDispatchStatus::Sent => NotificationDispatchStatusDto::Sent,
        project_store::NotificationDispatchStatus::Failed => NotificationDispatchStatusDto::Failed,
        project_store::NotificationDispatchStatus::Claimed => {
            NotificationDispatchStatusDto::Claimed
        }
    }
}

fn map_notification_reply_claim_status(
    status: project_store::NotificationReplyClaimStatus,
) -> NotificationReplyClaimStatusDto {
    match status {
        project_store::NotificationReplyClaimStatus::Accepted => {
            NotificationReplyClaimStatusDto::Accepted
        }
        project_store::NotificationReplyClaimStatus::Rejected => {
            NotificationReplyClaimStatusDto::Rejected
        }
    }
}
