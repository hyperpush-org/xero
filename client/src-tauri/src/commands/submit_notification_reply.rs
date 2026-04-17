use tauri::{AppHandle, Manager, Runtime};

use crate::{
    commands::{
        map_notification_dispatch_record, map_notification_reply_claim_record,
        resolve_operator_action::resolve_operator_action, resume_operator_run::resume_operator_run,
        runtime_support::resolve_project_root, validate_non_empty, CommandError, CommandResult,
        ResolveOperatorActionRequestDto, ResumeOperatorRunRequestDto,
        SubmitNotificationReplyRequestDto, SubmitNotificationReplyResponseDto,
    },
    db::project_store::{self, NotificationReplyClaimRequestRecord},
    state::DesktopState,
};

#[tauri::command]
pub fn submit_notification_reply<R: Runtime>(
    app: AppHandle<R>,
    request: SubmitNotificationReplyRequestDto,
) -> CommandResult<SubmitNotificationReplyResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.action_id, "actionId")?;
    validate_non_empty(&request.route_id, "routeId")?;
    validate_non_empty(&request.correlation_key, "correlationKey")?;
    validate_non_empty(&request.reply_text, "replyText")?;
    validate_non_empty(&request.decision, "decision")?;
    validate_non_empty(&request.received_at, "receivedAt")?;

    let decision = parse_decision(&request.decision)?;
    let reply_text = request.reply_text.trim().to_string();
    let responder_id = normalize_optional_non_empty(request.responder_id, "responderId")?;

    let state = app.state::<DesktopState>();
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    let claimed = project_store::claim_notification_reply(
        &repo_root,
        &NotificationReplyClaimRequestRecord {
            project_id: request.project_id.clone(),
            action_id: request.action_id.clone(),
            route_id: request.route_id,
            correlation_key: request.correlation_key,
            responder_id,
            reply_text: reply_text.clone(),
            received_at: request.received_at,
        },
    )?;

    let resolve_result = resolve_operator_action(
        app.clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: request.project_id.clone(),
            action_id: request.action_id.clone(),
            decision: decision.to_string(),
            user_answer: Some(reply_text.clone()),
        },
    )?;

    let resume_result = if decision == "approve" {
        Some(resume_operator_run(
            app.clone(),
            app.state::<DesktopState>(),
            ResumeOperatorRunRequestDto {
                project_id: request.project_id,
                action_id: request.action_id,
                user_answer: Some(reply_text),
            },
        )?)
    } else {
        None
    };

    Ok(SubmitNotificationReplyResponseDto {
        claim: map_notification_reply_claim_record(claimed.claim),
        dispatch: map_notification_dispatch_record(claimed.dispatch),
        resolve_result,
        resume_result,
    })
}

fn parse_decision(value: &str) -> CommandResult<&'static str> {
    match value.trim() {
        "approve" => Ok("approve"),
        "reject" => Ok("reject"),
        other => Err(CommandError::user_fixable(
            "notification_reply_decision_unsupported",
            format!(
                "Cadence does not support notification reply decision `{other}`. Allowed decisions: approve, reject."
            ),
        )),
    }
}

fn normalize_optional_non_empty(
    value: Option<String>,
    field: &'static str,
) -> CommandResult<Option<String>> {
    match value {
        Some(value) if value.trim().is_empty() => Err(CommandError::invalid_request(field)),
        Some(value) => Ok(Some(value.trim().to_string())),
        None => Ok(None),
    }
}
