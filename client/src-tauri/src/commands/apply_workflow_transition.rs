use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{
        map_workflow_automatic_dispatch_outcome, map_workflow_transition_event_record,
        runtime_support::{emit_project_updated, resolve_project_root},
        validate_non_empty, ApplyWorkflowTransitionRequestDto, ApplyWorkflowTransitionResponseDto,
        CommandError, CommandResult, ProjectUpdateReason,
    },
    db::project_store::{self, ApplyWorkflowTransitionRecord, WorkflowGateDecisionUpdate},
    state::DesktopState,
};

const MAX_WORKFLOW_TRANSITION_GATE_UPDATES: usize = 128;

#[tauri::command]
pub fn apply_workflow_transition<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ApplyWorkflowTransitionRequestDto,
) -> CommandResult<ApplyWorkflowTransitionResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_transition_request_size(&request)?;
    let transition = map_transition_request(&request)?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;

    let applied =
        project_store::apply_workflow_transition(&repo_root, &request.project_id, &transition)?;

    emit_project_updated(
        &app,
        &repo_root,
        &request.project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    Ok(ApplyWorkflowTransitionResponseDto {
        transition_event: map_workflow_transition_event_record(applied.transition_event),
        automatic_dispatch: map_workflow_automatic_dispatch_outcome(applied.automatic_dispatch),
        phases: applied.phases,
    })
}

fn validate_transition_request_size(
    request: &ApplyWorkflowTransitionRequestDto,
) -> CommandResult<()> {
    if request.gate_updates.len() > MAX_WORKFLOW_TRANSITION_GATE_UPDATES {
        return Err(CommandError::user_fixable(
            "workflow_transition_request_too_large",
            format!(
                "Cadence only accepts up to {MAX_WORKFLOW_TRANSITION_GATE_UPDATES} gate updates per transition request."
            ),
        ));
    }

    Ok(())
}

fn map_transition_request(
    request: &ApplyWorkflowTransitionRequestDto,
) -> CommandResult<ApplyWorkflowTransitionRecord> {
    Ok(ApplyWorkflowTransitionRecord {
        transition_id: request.transition_id.clone(),
        causal_transition_id: normalize_optional_non_empty(
            request.causal_transition_id.clone(),
            "causalTransitionId",
        )?,
        from_node_id: request.from_node_id.clone(),
        to_node_id: request.to_node_id.clone(),
        transition_kind: request.transition_kind.clone(),
        gate_decision: parse_transition_gate_decision(&request.gate_decision)?,
        gate_decision_context: normalize_optional_non_empty(
            request.gate_decision_context.clone(),
            "gateDecisionContext",
        )?,
        gate_updates: request
            .gate_updates
            .iter()
            .map(map_gate_update_request)
            .collect::<CommandResult<Vec<_>>>()?,
        occurred_at: request.occurred_at.clone(),
    })
}

fn map_gate_update_request(
    update: &crate::commands::WorkflowTransitionGateUpdateRequestDto,
) -> CommandResult<WorkflowGateDecisionUpdate> {
    Ok(WorkflowGateDecisionUpdate {
        gate_key: update.gate_key.clone(),
        gate_state: parse_workflow_gate_state(&update.gate_state)?,
        decision_context: normalize_optional_non_empty(
            update.decision_context.clone(),
            "decisionContext",
        )?,
    })
}

fn parse_workflow_gate_state(value: &str) -> CommandResult<project_store::WorkflowGateState> {
    match value.trim() {
        "pending" => Ok(project_store::WorkflowGateState::Pending),
        "satisfied" => Ok(project_store::WorkflowGateState::Satisfied),
        "blocked" => Ok(project_store::WorkflowGateState::Blocked),
        "skipped" => Ok(project_store::WorkflowGateState::Skipped),
        other => Err(CommandError::user_fixable(
            "workflow_transition_request_invalid",
            format!(
                "Cadence does not support workflow gate_state `{other}`. Allowed states: pending, satisfied, blocked, skipped."
            ),
        )),
    }
}

fn parse_transition_gate_decision(
    value: &str,
) -> CommandResult<project_store::WorkflowTransitionGateDecision> {
    match value.trim() {
        "approved" => Ok(project_store::WorkflowTransitionGateDecision::Approved),
        "rejected" => Ok(project_store::WorkflowTransitionGateDecision::Rejected),
        "blocked" => Ok(project_store::WorkflowTransitionGateDecision::Blocked),
        "not_applicable" => Ok(project_store::WorkflowTransitionGateDecision::NotApplicable),
        other => Err(CommandError::user_fixable(
            "workflow_transition_request_invalid",
            format!(
                "Cadence does not support gateDecision `{other}`. Allowed values: approved, rejected, blocked, not_applicable."
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
