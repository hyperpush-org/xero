use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        runtime_support::{emit_project_updated, resolve_project_root},
        validate_non_empty, CommandError, CommandResult, ProjectUpdateReason,
        ResolveOperatorActionRequestDto, ResolveOperatorActionResponseDto,
    },
    db::project_store::{self, OperatorApprovalDecision},
    state::DesktopState,
};

#[tauri::command]
pub fn resolve_operator_action<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ResolveOperatorActionRequestDto,
) -> CommandResult<ResolveOperatorActionResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.action_id, "actionId")?;

    let decision = parse_operator_action_decision(&request.decision)?;
    let user_answer = normalize_optional_non_empty(request.user_answer, "userAnswer")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    let resolved = project_store::resolve_operator_action(
        &repo_root,
        &request.project_id,
        &request.action_id,
        decision,
        user_answer.as_deref(),
    )?;

    emit_project_updated(
        &app,
        &repo_root,
        &request.project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    Ok(ResolveOperatorActionResponseDto {
        approval_request: resolved.approval_request,
        verification_record: resolved.verification_record,
    })
}

fn parse_operator_action_decision(value: &str) -> CommandResult<OperatorApprovalDecision> {
    match value.trim() {
        "approve" => Ok(OperatorApprovalDecision::Approved),
        "reject" => Ok(OperatorApprovalDecision::Rejected),
        other => Err(CommandError::user_fixable(
            "operator_action_decision_unsupported",
            format!(
                "Xero does not support operator decision `{other}`. Allowed decisions: approve, reject."
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
