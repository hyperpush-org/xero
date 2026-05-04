use super::*;

pub(crate) const PLAN_REVIEW_ACTION_ID: &str = "plan-mode-before-execution";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentRunState {
    Intake,
    ContextGather,
    Plan,
    ApprovalWait,
    Execute,
    Verify,
    Summarize,
    Blocked,
    Complete,
}

impl AgentRunState {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Intake => "intake",
            Self::ContextGather => "context_gather",
            Self::Plan => "plan",
            Self::ApprovalWait => "approval_wait",
            Self::Execute => "execute",
            Self::Verify => "verify",
            Self::Summarize => "summarize",
            Self::Blocked => "blocked",
            Self::Complete => "complete",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AgentRunStopReason {
    Complete,
    Blocked,
    WaitingForApproval,
    ContextOverBudget,
    ProviderFailure,
    Cancelled,
    HarnessFault,
}

impl AgentRunStopReason {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Blocked => "blocked",
            Self::WaitingForApproval => "waiting_for_approval",
            Self::ContextOverBudget => "context_over_budget",
            Self::ProviderFailure => "provider_failure",
            Self::Cancelled => "cancelled",
            Self::HarnessFault => "harness_fault",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentTaskClassification {
    pub requires_plan: bool,
    pub score: u8,
    pub reason_codes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolBatchGate {
    Allow,
    RequirePlan { message: String },
    RequirePlanApproval { action_id: String, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VerificationGateStatus {
    NotRequired,
    Satisfied,
    UnableToVerify,
    Required,
}

impl VerificationGateStatus {
    pub(crate) const fn as_str(&self) -> &'static str {
        match self {
            Self::NotRequired => "not_required",
            Self::Satisfied => "satisfied",
            Self::UnableToVerify => "unable_to_verify",
            Self::Required => "required",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerificationGateDecision {
    pub status: VerificationGateStatus,
    pub message: String,
    pub evidence: Option<String>,
    pub latest_file_change_event_id: Option<i64>,
}

pub(crate) fn classify_agent_task(
    prompt: &str,
    controls: &RuntimeRunControlStateDto,
) -> AgentTaskClassification {
    if !controls.active.runtime_agent_id.allows_plan_gate() {
        return AgentTaskClassification {
            requires_plan: false,
            score: 0,
            reason_codes: vec!["agent_plan_gate_disabled".into()],
        };
    }

    let mut score = 0_u8;
    let mut reason_codes = Vec::new();
    let task_text = prompt
        .lines()
        .filter(|line| !line.trim_start().starts_with("tool:"))
        .collect::<Vec<_>>()
        .join("\n");
    let task_text = if task_text.trim().is_empty() {
        prompt
    } else {
        task_text.as_str()
    };
    let lowered = task_text.to_ascii_lowercase();

    if controls.active.plan_mode_required {
        score = score.saturating_add(4);
        reason_codes.push("operator_plan_mode".into());
    }
    if contains_any(
        &lowered,
        &[
            "implement",
            "change",
            "modify",
            "edit",
            "write",
            "fix",
            "refactor",
            "migrate",
            "add ",
            "remove ",
            "delete ",
            "replace ",
        ],
    ) {
        score = score.saturating_add(2);
        reason_codes.push("code_change_intent".into());
    }
    if contains_any(
        &lowered,
        &[
            "milestone",
            "production",
            "end-to-end",
            "e2e",
            "architecture",
            "state machine",
            "orchestrator",
            "runtime",
            "harness",
        ],
    ) {
        score = score.saturating_add(2);
        reason_codes.push("complex_scope".into());
    }
    if contains_any(
        &lowered,
        &[
            "multi-file",
            "multiple files",
            "cross-module",
            "frontend and backend",
            "database",
            "migration",
            "auth",
            "security",
            "permission",
            "approval",
            "sandbox",
            "credentials",
        ],
    ) {
        score = score.saturating_add(2);
        reason_codes.push("high_risk_or_multi_file".into());
    }
    if task_text.chars().count() > 180 {
        score = score.saturating_add(1);
        reason_codes.push("long_prompt".into());
    }

    reason_codes.sort();
    reason_codes.dedup();

    AgentTaskClassification {
        requires_plan: score >= 3,
        score,
        reason_codes,
    }
}

pub(crate) fn record_initial_state_artifacts(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    prompt: &str,
    controls: &RuntimeRunControlStateDto,
) -> CommandResult<AgentTaskClassification> {
    let classification = classify_agent_task(prompt, controls);
    record_state_transition(
        repo_root,
        project_id,
        run_id,
        AgentStateTransition {
            from: None,
            to: AgentRunState::Intake,
            reason: "Owned-agent run accepted.",
            stop_reason: None,
            extra: Some(json!({
                "taskClassification": {
                    "requiresPlan": classification.requires_plan,
                    "score": classification.score,
                    "reasonCodes": classification.reason_codes,
                },
            })),
        },
    )?;
    record_state_transition(
        repo_root,
        project_id,
        run_id,
        AgentStateTransition {
            from: Some(AgentRunState::Intake),
            to: AgentRunState::ContextGather,
            reason: "Repository preflight passed; gathering task context.",
            stop_reason: None,
            extra: None,
        },
    )?;
    if classification.requires_plan {
        record_state_transition(
            repo_root,
            project_id,
            run_id,
            AgentStateTransition {
                from: Some(AgentRunState::ContextGather),
                to: AgentRunState::Plan,
                reason: "Task classification requires a structured plan before execution.",
                stop_reason: None,
                extra: Some(json!({
                    "reasonCodes": classification.reason_codes,
                })),
            },
        )?;
    }
    Ok(classification)
}

pub(crate) fn evaluate_tool_batch_gate(
    snapshot: &AgentRunSnapshotRecord,
    controls: &RuntimeRunControlStateDto,
    classification: &AgentTaskClassification,
    tool_calls: &[AgentToolCall],
) -> ToolBatchGate {
    if !controls.active.runtime_agent_id.allows_plan_gate() {
        return ToolBatchGate::Allow;
    }

    if !tool_calls
        .iter()
        .any(|tool_call| is_execution_tool(&tool_call.tool_name))
    {
        return ToolBatchGate::Allow;
    }

    if !(classification.requires_plan || controls.active.plan_mode_required) {
        return ToolBatchGate::Allow;
    }

    if !snapshot_has_plan_artifact(snapshot) {
        let tool_names = tool_calls
            .iter()
            .filter(|tool_call| is_execution_tool(&tool_call.tool_name))
            .map(|tool_call| tool_call.tool_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return ToolBatchGate::RequirePlan {
            message: format!(
                "Xero plan gate: this task is classified as complex before execution tool(s) `{tool_names}`. Create or update the structured plan with the `todo` tool first, then continue with execution."
            ),
        };
    }

    if controls.active.plan_mode_required && !snapshot_has_answered_plan_review(snapshot) {
        let tool_names = tool_calls
            .iter()
            .filter(|tool_call| is_execution_tool(&tool_call.tool_name))
            .map(|tool_call| tool_call.tool_name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        return ToolBatchGate::RequirePlanApproval {
            action_id: sanitize_action_id(PLAN_REVIEW_ACTION_ID),
            message: format!(
                "Plan mode is enabled. Xero paused before execution tool(s): {tool_names}. Review the current structured plan, then resume the run when execution is approved."
            ),
        };
    }

    ToolBatchGate::Allow
}

pub(crate) fn tool_batch_contains_execution(tool_calls: &[AgentToolCall]) -> bool {
    tool_calls
        .iter()
        .any(|tool_call| is_execution_tool(&tool_call.tool_name))
}

pub(crate) fn record_plan_gate_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    message: &str,
    classification: &AgentTaskClassification,
) -> CommandResult<()> {
    record_state_transition(
        repo_root,
        project_id,
        run_id,
        AgentStateTransition {
            from: Some(AgentRunState::ContextGather),
            to: AgentRunState::Plan,
            reason: "Execution was held until a structured plan is declared.",
            stop_reason: None,
            extra: Some(json!({
                "reasonCodes": classification.reason_codes,
            })),
        },
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::VerificationGate,
        json!({
            "kind": "plan_gate",
            "status": "required",
            "message": message,
        }),
    )?;
    Ok(())
}

pub(crate) fn record_plan_review_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    message: &str,
) -> CommandResult<CommandError> {
    record_state_transition(
        repo_root,
        project_id,
        run_id,
        AgentStateTransition {
            from: Some(AgentRunState::Plan),
            to: AgentRunState::ApprovalWait,
            reason: "Plan mode requires operator review before execution.",
            stop_reason: Some(AgentRunStopReason::WaitingForApproval),
            extra: None,
        },
    )?;
    record_action_request(
        repo_root,
        project_id,
        run_id,
        action_id,
        "review_plan",
        "Review plan before execution",
        message,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "actionId": action_id,
            "actionType": "review_plan",
            "title": "Review plan before execution",
            "code": "agent_plan_mode_requires_approval",
            "message": message,
            "stopReason": AgentRunStopReason::WaitingForApproval.as_str(),
            "state": AgentRunState::ApprovalWait.as_str(),
        }),
    )?;
    Ok(CommandError::new(
        "agent_plan_mode_requires_approval",
        CommandErrorClass::PolicyDenied,
        message,
        false,
    ))
}

pub(crate) fn record_plan_artifact_from_tool_result(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    result: &AgentToolResult,
) -> CommandResult<bool> {
    if result.tool_name != AUTONOMOUS_TOOL_TODO || !result.ok {
        return Ok(false);
    }

    let tool_result = serde_json::from_value::<AutonomousToolResult>(result.output.clone())
        .map_err(|error| {
            CommandError::system_fault(
                "agent_plan_artifact_decode_failed",
                format!("Xero could not decode todo output for the structured plan: {error}"),
            )
        })?;
    let AutonomousToolOutput::Todo(output) = tool_result.output else {
        return Ok(false);
    };
    if output.items.is_empty() {
        return Ok(false);
    }

    let total = output.items.len();
    let completed = output
        .items
        .iter()
        .filter(|item| item.status == AutonomousTodoStatus::Completed)
        .count();
    let in_progress = output
        .items
        .iter()
        .filter(|item| item.status == AutonomousTodoStatus::InProgress)
        .count();
    let pending = total.saturating_sub(completed).saturating_sub(in_progress);
    let payload = json!({
        "kind": "structured_plan",
        "sourceToolCallId": result.tool_call_id,
        "action": output.action,
        "total": total,
        "pending": pending,
        "inProgress": in_progress,
        "completed": completed,
        "items": output.items,
        "changedItem": output.changed_item,
    });

    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::PlanUpdated,
        payload.clone(),
    )?;
    let payload_json = serde_json::to_string(&payload).map_err(|error| {
        CommandError::system_fault(
            "agent_plan_checkpoint_serialize_failed",
            format!("Xero could not serialize the structured plan checkpoint: {error}"),
        )
    })?;
    project_store::append_agent_checkpoint(
        repo_root,
        &NewAgentCheckpointRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            checkpoint_kind: "plan".into(),
            summary: format!("Structured plan updated with {total} item(s)."),
            payload_json: Some(payload_json),
            created_at: now_timestamp(),
        },
    )?;
    Ok(true)
}

pub(crate) fn evaluate_completion_gate(
    snapshot: &AgentRunSnapshotRecord,
    final_message: &str,
) -> VerificationGateDecision {
    if snapshot.file_changes.is_empty() {
        return VerificationGateDecision {
            status: VerificationGateStatus::NotRequired,
            message: "No file changes were recorded for this run.".into(),
            evidence: None,
            latest_file_change_event_id: None,
        };
    }

    let latest_file_change_event_id = snapshot
        .events
        .iter()
        .filter(|event| event.event_kind == AgentRunEventKind::FileChanged)
        .map(|event| event.id)
        .max();
    let after_event_id = latest_file_change_event_id.unwrap_or(0);
    if let Some(evidence) = verification_evidence_after(snapshot, after_event_id) {
        return VerificationGateDecision {
            status: VerificationGateStatus::Satisfied,
            message: "Verification evidence was recorded after the latest file change.".into(),
            evidence: Some(evidence),
            latest_file_change_event_id,
        };
    }

    if final_message_declares_unable_to_verify(final_message) {
        return VerificationGateDecision {
            status: VerificationGateStatus::UnableToVerify,
            message: "The final response explicitly declared that verification could not be run."
                .into(),
            evidence: Some(trim_gate_evidence(final_message)),
            latest_file_change_event_id,
        };
    }

    VerificationGateDecision {
        status: VerificationGateStatus::Required,
        message: "This run changed files, but no verification evidence was recorded after the latest file change.".into(),
        evidence: None,
        latest_file_change_event_id,
    }
}

pub(crate) fn record_completion_gate(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    decision: &VerificationGateDecision,
) -> CommandResult<()> {
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::VerificationGate,
        json!({
            "kind": "completion_verification_gate",
            "status": decision.status.as_str(),
            "message": decision.message,
            "evidence": decision.evidence,
            "latestFileChangeEventId": decision.latest_file_change_event_id,
        }),
    )?;
    Ok(())
}

pub(crate) fn verification_gate_prompt(decision: &VerificationGateDecision) -> String {
    format!(
        "Xero verification gate: {} Run a focused verification command now, or if verification is impossible, respond with `Unable to verify: <specific reason>`.",
        decision.message
    )
}

pub(crate) struct AgentStateTransition {
    pub(crate) from: Option<AgentRunState>,
    pub(crate) to: AgentRunState,
    pub(crate) reason: &'static str,
    pub(crate) stop_reason: Option<AgentRunStopReason>,
    pub(crate) extra: Option<JsonValue>,
}

pub(crate) fn record_state_transition(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    transition: AgentStateTransition,
) -> CommandResult<()> {
    let mut payload = JsonMap::new();
    payload.insert("kind".into(), json!("agent_state_transition"));
    payload.insert(
        "from".into(),
        json!(transition.from.map(AgentRunState::as_str)),
    );
    payload.insert("to".into(), json!(transition.to.as_str()));
    payload.insert("reason".into(), json!(transition.reason));
    if let Some(stop_reason) = transition.stop_reason {
        payload.insert("stopReason".into(), json!(stop_reason.as_str()));
    }
    if let Some(extra) = transition.extra {
        if let Some(extra) = extra.as_object() {
            for (key, value) in extra {
                payload.insert(key.clone(), value.clone());
            }
        }
    }
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::StateTransition,
        JsonValue::Object(payload),
    )?;
    Ok(())
}

pub(crate) fn stop_reason_for_error(error: &CommandError) -> AgentRunStopReason {
    if error.code == AGENT_RUN_CANCELLED_CODE {
        return AgentRunStopReason::Cancelled;
    }
    if error.code == "agent_context_budget_exceeded" {
        return AgentRunStopReason::ContextOverBudget;
    }
    if error.code == "agent_tool_boundary_violation" {
        return AgentRunStopReason::Blocked;
    }
    if error.class == CommandErrorClass::PolicyDenied {
        return AgentRunStopReason::WaitingForApproval;
    }
    match error.class {
        CommandErrorClass::SystemFault => AgentRunStopReason::HarnessFault,
        CommandErrorClass::Retryable => AgentRunStopReason::ProviderFailure,
        CommandErrorClass::UserFixable => AgentRunStopReason::Blocked,
        CommandErrorClass::PolicyDenied => AgentRunStopReason::WaitingForApproval,
    }
}

pub(crate) fn error_should_pause(snapshot: &AgentRunSnapshotRecord, error: &CommandError) -> bool {
    if error.class == CommandErrorClass::PolicyDenied {
        return snapshot
            .action_requests
            .iter()
            .any(|action| action.status == "pending");
    }
    matches!(error.code.as_str(), "agent_verification_required")
}

fn snapshot_has_plan_artifact(snapshot: &AgentRunSnapshotRecord) -> bool {
    snapshot
        .checkpoints
        .iter()
        .any(|checkpoint| checkpoint.checkpoint_kind == "plan")
        || snapshot
            .events
            .iter()
            .any(|event| event.event_kind == AgentRunEventKind::PlanUpdated)
}

fn snapshot_has_answered_plan_review(snapshot: &AgentRunSnapshotRecord) -> bool {
    snapshot.action_requests.iter().any(|action| {
        action.action_id == sanitize_action_id(PLAN_REVIEW_ACTION_ID)
            && action.action_type == "review_plan"
            && matches!(action.status.as_str(), "answered" | "approved")
    })
}

fn is_execution_tool(tool_name: &str) -> bool {
    !matches!(
        tool_name,
        AUTONOMOUS_TOOL_READ
            | AUTONOMOUS_TOOL_SEARCH
            | AUTONOMOUS_TOOL_FIND
            | AUTONOMOUS_TOOL_GIT_STATUS
            | AUTONOMOUS_TOOL_GIT_DIFF
            | AUTONOMOUS_TOOL_LIST
            | AUTONOMOUS_TOOL_HASH
            | AUTONOMOUS_TOOL_TOOL_ACCESS
            | AUTONOMOUS_TOOL_TOOL_SEARCH
            | AUTONOMOUS_TOOL_PROJECT_CONTEXT
            | AUTONOMOUS_TOOL_WORKSPACE_INDEX
            | AUTONOMOUS_TOOL_TODO
            | AUTONOMOUS_TOOL_WEB_SEARCH
            | AUTONOMOUS_TOOL_WEB_FETCH
            | AUTONOMOUS_TOOL_CODE_INTEL
            | AUTONOMOUS_TOOL_LSP
    )
}

fn verification_evidence_after(
    snapshot: &AgentRunSnapshotRecord,
    after_event_id: i64,
) -> Option<String> {
    for event in snapshot
        .events
        .iter()
        .filter(|event| event.id > after_event_id)
    {
        if event.event_kind != AgentRunEventKind::CommandOutput {
            continue;
        }
        let Ok(payload) = serde_json::from_str::<JsonValue>(&event.payload_json) else {
            continue;
        };
        if !payload
            .get("spawned")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        if payload.get("exitCode").is_none() {
            continue;
        }
        let argv = payload
            .get("argv")
            .and_then(JsonValue::as_array)
            .map(|argv| {
                argv.iter()
                    .filter_map(JsonValue::as_str)
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .filter(|argv| !argv.trim().is_empty())
            .unwrap_or_else(|| "command output".into());
        let exit_code = payload
            .get("exitCode")
            .and_then(JsonValue::as_i64)
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unknown".into());
        return Some(format!("{argv} exited with code {exit_code}."));
    }
    None
}

fn final_message_declares_unable_to_verify(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    normalized.contains("unable to verify")
        || normalized.contains("could not verify")
        || normalized.contains("couldn't verify")
        || (normalized.contains("verification")
            && (normalized.contains("could not be run")
                || normalized.contains("couldn't be run")
                || normalized.contains("not run")
                || normalized.contains("was not run"))
            && (normalized.contains("because")
                || normalized.contains("blocked")
                || normalized.contains("unavailable")
                || normalized.contains("missing")))
}

fn trim_gate_evidence(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.chars().count() <= 240 {
        return trimmed.into();
    }
    let mut output = trimmed.chars().take(240).collect::<String>();
    output.push_str("...");
    output
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn controls(plan_mode_required: bool) -> RuntimeRunControlStateDto {
        RuntimeRunControlStateDto {
            active: RuntimeRunActiveControlSnapshotDto {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                provider_profile_id: None,
                model_id: "fake".into(),
                thinking_effort: None,
                approval_mode: RuntimeRunApprovalModeDto::Yolo,
                plan_mode_required,
                revision: 1,
                applied_at: "2026-04-30T00:00:00Z".into(),
            },
            pending: None,
        }
    }

    fn empty_snapshot() -> AgentRunSnapshotRecord {
        AgentRunSnapshotRecord {
            run: project_store::AgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: "engineer".into(),
                agent_definition_version: project_store::BUILTIN_AGENT_DEFINITION_VERSION,
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
                trace_id: "0123456789abcdef0123456789abcdef".into(),
                lineage_kind: "top_level".into(),
                parent_run_id: None,
                parent_trace_id: None,
                parent_subagent_id: None,
                subagent_role: None,
                provider_id: "fake".into(),
                model_id: "fake".into(),
                status: AgentRunStatus::Running,
                prompt: "prompt".into(),
                system_prompt: "system".into(),
                started_at: "2026-04-30T00:00:00Z".into(),
                last_heartbeat_at: None,
                completed_at: None,
                cancelled_at: None,
                last_error: None,
                updated_at: "2026-04-30T00:00:00Z".into(),
            },
            messages: Vec::new(),
            events: Vec::new(),
            tool_calls: Vec::new(),
            file_changes: Vec::new(),
            checkpoints: Vec::new(),
            action_requests: Vec::new(),
        }
    }

    #[test]
    fn classifier_requires_plan_for_production_milestone_work() {
        let classification = classify_agent_task(
            "Implement milestone 4 to production standards",
            &controls(false),
        );

        assert!(classification.requires_plan);
        assert!(classification
            .reason_codes
            .contains(&"code_change_intent".to_string()));
        assert!(classification
            .reason_codes
            .contains(&"complex_scope".to_string()));
    }

    #[test]
    fn gate_holds_execution_tools_until_plan_exists() {
        let snapshot = empty_snapshot();
        let classification =
            classify_agent_task("Implement the runtime state machine", &controls(false));
        let gate = evaluate_tool_batch_gate(
            &snapshot,
            &controls(false),
            &classification,
            &[AgentToolCall {
                tool_call_id: "call-1".into(),
                tool_name: AUTONOMOUS_TOOL_EDIT.into(),
                input: json!({ "path": "src/lib.rs" }),
            }],
        );

        assert!(matches!(gate, ToolBatchGate::RequirePlan { .. }));
    }

    #[test]
    fn debug_agent_uses_engineering_plan_gate() {
        let snapshot = empty_snapshot();
        let mut debug_controls = controls(true);
        debug_controls.active.runtime_agent_id = RuntimeAgentIdDto::Debug;
        let classification = classify_agent_task(
            "Debug the failing runtime state machine test",
            &debug_controls,
        );
        let gate = evaluate_tool_batch_gate(
            &snapshot,
            &debug_controls,
            &classification,
            &[AgentToolCall {
                tool_call_id: "call-1".into(),
                tool_name: AUTONOMOUS_TOOL_COMMAND.into(),
                input: json!({ "argv": ["pnpm", "test"] }),
            }],
        );

        assert!(classification.requires_plan);
        assert!(matches!(gate, ToolBatchGate::RequirePlan { .. }));
    }

    #[test]
    fn ask_agent_does_not_require_engineering_plan_gate() {
        let snapshot = empty_snapshot();
        let mut ask_controls = controls(true);
        ask_controls.active.runtime_agent_id = RuntimeAgentIdDto::Ask;
        ask_controls.active.approval_mode = RuntimeRunApprovalModeDto::Suggest;
        let classification =
            classify_agent_task("Implement the runtime state machine", &ask_controls);
        let gate = evaluate_tool_batch_gate(
            &snapshot,
            &ask_controls,
            &classification,
            &[AgentToolCall {
                tool_call_id: "call-1".into(),
                tool_name: AUTONOMOUS_TOOL_READ.into(),
                input: json!({ "path": "src/lib.rs" }),
            }],
        );

        assert!(!classification.requires_plan);
        assert!(matches!(gate, ToolBatchGate::Allow));
    }

    #[test]
    fn completion_gate_requires_evidence_after_file_changes() {
        let mut snapshot = empty_snapshot();
        snapshot
            .file_changes
            .push(project_store::AgentFileChangeRecord {
                id: 1,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                trace_id: "0123456789abcdef0123456789abcdef".into(),
                top_level_run_id: "run-1".into(),
                subagent_id: None,
                subagent_role: None,
                path: "src/lib.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-04-30T00:00:01Z".into(),
            });
        snapshot.events.push(project_store::AgentEventRecord {
            id: 1,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind: AgentRunEventKind::FileChanged,
            payload_json: "{}".into(),
            created_at: "2026-04-30T00:00:01Z".into(),
        });

        let decision = evaluate_completion_gate(&snapshot, "Done.");

        assert_eq!(decision.status, VerificationGateStatus::Required);
    }

    #[test]
    fn completion_gate_accepts_command_evidence_after_latest_change() {
        let mut snapshot = empty_snapshot();
        snapshot
            .file_changes
            .push(project_store::AgentFileChangeRecord {
                id: 1,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                trace_id: "0123456789abcdef0123456789abcdef".into(),
                top_level_run_id: "run-1".into(),
                subagent_id: None,
                subagent_role: None,
                path: "src/lib.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-04-30T00:00:01Z".into(),
            });
        snapshot.events.push(project_store::AgentEventRecord {
            id: 1,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind: AgentRunEventKind::FileChanged,
            payload_json: "{}".into(),
            created_at: "2026-04-30T00:00:01Z".into(),
        });
        snapshot.events.push(project_store::AgentEventRecord {
            id: 2,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind: AgentRunEventKind::CommandOutput,
            payload_json: r#"{"argv":["cargo","test"],"spawned":true,"exitCode":0}"#.into(),
            created_at: "2026-04-30T00:00:02Z".into(),
        });

        let decision = evaluate_completion_gate(&snapshot, "Done.");

        assert_eq!(decision.status, VerificationGateStatus::Satisfied);
        assert_eq!(
            decision.evidence.as_deref(),
            Some("cargo test exited with code 0.")
        );
    }

    #[test]
    fn completion_gate_accepts_explicit_unable_to_verify_reason() {
        let mut snapshot = empty_snapshot();
        snapshot
            .file_changes
            .push(project_store::AgentFileChangeRecord {
                id: 1,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                trace_id: "0123456789abcdef0123456789abcdef".into(),
                top_level_run_id: "run-1".into(),
                subagent_id: None,
                subagent_role: None,
                path: "src/lib.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-04-30T00:00:01Z".into(),
            });
        snapshot.events.push(project_store::AgentEventRecord {
            id: 1,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind: AgentRunEventKind::FileChanged,
            payload_json: "{}".into(),
            created_at: "2026-04-30T00:00:01Z".into(),
        });

        let decision = evaluate_completion_gate(
            &snapshot,
            "Unable to verify: protoc is not installed on PATH.",
        );

        assert_eq!(decision.status, VerificationGateStatus::UnableToVerify);
    }
}
