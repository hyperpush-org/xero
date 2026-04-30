use std::{path::Path, str::FromStr, thread, time::Duration};

use tauri::{
    ipc::{Channel, JavaScriptChannelId},
    AppHandle, Runtime, State, Webview,
};

use crate::{
    commands::{
        validate_non_empty, CommandError, CommandResult, RuntimeStreamItemDto,
        RuntimeStreamItemKind, RuntimeStreamTranscriptRole, RuntimeToolCallState,
        SubscribeRuntimeStreamRequestDto, SubscribeRuntimeStreamResponseDto,
    },
    db::project_store::{
        self, AgentEventRecord, AgentRunEventKind, AgentRunStatus, RuntimeRunSnapshotRecord,
        RuntimeRunStatus,
    },
    runtime::{subscribe_agent_events, AgentEventSubscription, OWNED_AGENT_SUPERVISOR_KIND},
    state::DesktopState,
};

use super::runtime_support::{load_persisted_runtime_run, resolve_project_root};

#[tauri::command]
pub fn subscribe_runtime_stream<R: Runtime>(
    app: AppHandle<R>,
    webview: Webview<R>,
    state: State<'_, DesktopState>,
    request: SubscribeRuntimeStreamRequestDto,
) -> CommandResult<SubscribeRuntimeStreamResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;

    let item_kinds = parse_requested_item_kinds(&request.item_kinds)?;
    let channel = resolve_channel(&webview, request.channel.as_deref())?;

    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let runtime_run =
        load_persisted_runtime_run(&repo_root, &request.project_id, &request.agent_session_id)?
            .filter(|snapshot| snapshot.run.supervisor_kind == OWNED_AGENT_SUPERVISOR_KIND)
            .ok_or_else(|| {
                CommandError::retryable(
                    "runtime_stream_run_unavailable",
                    "Xero cannot start a live runtime stream until the selected project has a Xero-owned agent run.",
                )
            })?;

    subscribe_owned_runtime_stream(&repo_root, &request, runtime_run, item_kinds, channel)
}

fn parse_requested_item_kinds(item_kinds: &[String]) -> CommandResult<Vec<RuntimeStreamItemKind>> {
    if item_kinds.is_empty() {
        return Err(CommandError::user_fixable(
            "invalid_request",
            "Field `itemKinds` must contain at least one allowed runtime stream item kind.",
        ));
    }

    let mut parsed = Vec::with_capacity(item_kinds.len());
    for kind in item_kinds {
        let kind = parse_runtime_stream_item_kind(kind)?;
        if !parsed.contains(&kind) {
            parsed.push(kind);
        }
    }

    Ok(parsed)
}

fn parse_runtime_stream_item_kind(value: &str) -> CommandResult<RuntimeStreamItemKind> {
    match value {
        "transcript" => Ok(RuntimeStreamItemKind::Transcript),
        "tool" => Ok(RuntimeStreamItemKind::Tool),
        "skill" => Ok(RuntimeStreamItemKind::Skill),
        "activity" => Ok(RuntimeStreamItemKind::Activity),
        "action_required" => Ok(RuntimeStreamItemKind::ActionRequired),
        "complete" => Ok(RuntimeStreamItemKind::Complete),
        "failure" => Ok(RuntimeStreamItemKind::Failure),
        other => Err(CommandError::user_fixable(
            "runtime_stream_item_kind_unsupported",
            format!(
                "Xero does not support runtime stream item kind `{other}`. Allowed kinds: {}.",
                RuntimeStreamItemDto::allowed_kind_names().join(", ")
            ),
        )),
    }
}

fn subscribe_owned_runtime_stream(
    repo_root: &Path,
    request: &SubscribeRuntimeStreamRequestDto,
    runtime_run: RuntimeRunSnapshotRecord,
    item_kinds: Vec<RuntimeStreamItemKind>,
    channel: Channel<RuntimeStreamItemDto>,
) -> CommandResult<SubscribeRuntimeStreamResponseDto> {
    let run_id = runtime_run.run.run_id.clone();
    let runtime_terminal = matches!(
        runtime_run.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    );
    let session_id = format!("owned-agent:{run_id}");
    let subscription = subscribe_agent_events(&request.project_id, &run_id);
    let (last_event_id, terminal) = replay_owned_agent_events(
        repo_root,
        &request.project_id,
        &run_id,
        &session_id,
        &item_kinds,
        &channel,
    )?;

    if !terminal && !runtime_terminal {
        let requested_item_kinds = item_kinds.clone();
        let project_id = request.project_id.clone();
        let run_id_for_thread = run_id.clone();
        let session_id_for_thread = session_id.clone();
        thread::spawn(move || {
            stream_live_owned_agent_events(
                subscription,
                channel,
                project_id,
                run_id_for_thread,
                session_id_for_thread,
                requested_item_kinds,
                last_event_id,
            );
        });
    }

    Ok(SubscribeRuntimeStreamResponseDto {
        project_id: request.project_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        runtime_kind: runtime_run.run.runtime_kind,
        run_id,
        session_id,
        flow_id: None,
        subscribed_item_kinds: item_kinds,
    })
}

fn replay_owned_agent_events(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    session_id: &str,
    item_kinds: &[RuntimeStreamItemKind],
    channel: &Channel<RuntimeStreamItemDto>,
) -> CommandResult<(i64, bool)> {
    let snapshot = match project_store::load_agent_run(repo_root, project_id, run_id) {
        Ok(snapshot) => snapshot,
        Err(error) if error.code == "agent_run_not_found" => return Ok((0, false)),
        Err(error) => return Err(error),
    };
    let terminal = matches!(
        snapshot.run.status,
        AgentRunStatus::Paused
            | AgentRunStatus::Cancelled
            | AgentRunStatus::Completed
            | AgentRunStatus::Failed
    );
    let mut last_event_id = 0;
    for event in snapshot.events {
        last_event_id = last_event_id.max(event.id);
        if let Some(item) = owned_agent_event_runtime_item(event, session_id, None) {
            if should_emit_owned_runtime_item(item_kinds, &item.kind) {
                channel.send(item).map_err(|error| {
                    CommandError::retryable(
                        "runtime_stream_channel_closed",
                        format!(
                            "Xero could not deliver an owned-agent runtime stream replay item because the desktop channel closed: {error}"
                        ),
                    )
                })?;
            }
        }
    }
    Ok((last_event_id, terminal))
}

fn stream_live_owned_agent_events(
    subscription: AgentEventSubscription,
    channel: Channel<RuntimeStreamItemDto>,
    project_id: String,
    run_id: String,
    session_id: String,
    item_kinds: Vec<RuntimeStreamItemKind>,
    mut last_event_id: i64,
) {
    const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
    loop {
        let event = match subscription.recv_timeout(IDLE_TIMEOUT) {
            Ok(event) => event,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        };
        if event.project_id != project_id || event.run_id != run_id || event.id <= last_event_id {
            continue;
        }
        let terminal = matches!(
            event.event_kind,
            AgentRunEventKind::RunPaused
                | AgentRunEventKind::RunCompleted
                | AgentRunEventKind::RunFailed
        );
        last_event_id = event.id;
        if let Some(item) = owned_agent_event_runtime_item(event, &session_id, None) {
            if should_emit_owned_runtime_item(&item_kinds, &item.kind)
                && channel.send(item).is_err()
            {
                return;
            }
        }
        if terminal {
            break;
        }
    }
}

fn owned_agent_event_runtime_item(
    event: AgentEventRecord,
    session_id: &str,
    flow_id: Option<String>,
) -> Option<RuntimeStreamItemDto> {
    let event_id = event.id;
    let event_kind = event.event_kind.clone();
    let payload = serde_json::from_str::<serde_json::Value>(&event.payload_json).unwrap_or_else(
        |error| {
            serde_json::json!({
                "code": "owned_agent_event_decode_failed",
                "message": format!("Xero could not decode owned-agent event payload {event_id}: {error}"),
                "retryable": false,
            })
        },
    );
    let mut item = RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Activity,
        run_id: event.run_id.clone(),
        sequence: event_id.max(0) as u64,
        session_id: Some(session_id.to_string()),
        flow_id,
        text: None,
        transcript_role: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        tool_summary: None,
        skill_id: None,
        skill_stage: None,
        skill_result: None,
        skill_source: None,
        skill_cache_status: None,
        skill_diagnostic: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        title: None,
        detail: None,
        code: None,
        message: None,
        retryable: None,
        created_at: event.created_at,
    };

    match event_kind {
        AgentRunEventKind::MessageDelta => {
            item.kind = RuntimeStreamItemKind::Transcript;
            item.text = payload_string(&payload, "text");
            item.transcript_role = payload_transcript_role(&payload);
        }
        AgentRunEventKind::ReasoningSummary => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_reasoning".into());
            item.title = Some("Reasoning".into());
            item.detail = payload_string(&payload, "summary")
                .or_else(|| Some("Owned agent reasoning summary updated.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ToolStarted => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            item.tool_state = Some(RuntimeToolCallState::Running);
            item.text = item
                .tool_name
                .as_ref()
                .map(|tool_name| format!("Started `{tool_name}`."));
        }
        AgentRunEventKind::ToolDelta => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_tool_delta".into());
            item.title = Some("Tool arguments".into());
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            item.detail = payload_string(&payload, "argumentsDelta")
                .or_else(|| Some("Provider streamed tool-call arguments.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ToolCompleted => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            item.tool_state = Some(if payload_bool(&payload, "ok").unwrap_or(false) {
                RuntimeToolCallState::Succeeded
            } else {
                RuntimeToolCallState::Failed
            });
            item.text = payload_string(&payload, "summary")
                .or_else(|| payload_string(&payload, "message"))
                .or_else(|| {
                    item.tool_name
                        .as_ref()
                        .map(|name| format!("Completed `{name}`."))
                });
            item.code = payload_string(&payload, "code");
            item.message = payload_string(&payload, "message");
        }
        AgentRunEventKind::FileChanged => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_file_changed".into());
            let operation =
                payload_string(&payload, "operation").unwrap_or_else(|| "change".into());
            let path = payload_string(&payload, "path").unwrap_or_else(|| "unknown path".into());
            item.title = Some("File changed".into());
            item.detail = payload_string(&payload, "toPath")
                .map(|to_path| format!("{operation}: {path} -> {to_path}"))
                .or_else(|| Some(format!("{operation}: {path}")));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::CommandOutput => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_command_output".into());
            item.title = Some("Command output".into());
            item.detail = Some(command_output_summary(&payload));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ValidationStarted => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_validation_started".into());
            item.title = Some("Validation started".into());
            item.detail = payload_string(&payload, "label")
                .map(|label| format!("Validation started: {label}."))
                .or_else(|| Some("Owned agent validation started.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ValidationCompleted => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_validation_completed".into());
            let label = payload_string(&payload, "label").unwrap_or_else(|| "validation".into());
            let outcome = payload_string(&payload, "outcome").unwrap_or_else(|| "completed".into());
            item.title = Some("Validation completed".into());
            item.detail = Some(format!("Validation {outcome}: {label}."));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ToolRegistrySnapshot => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_tool_registry_snapshot".into());
            item.title = Some("Tool registry".into());
            let count = payload
                .get("toolNames")
                .and_then(serde_json::Value::as_array)
                .map(Vec::len)
                .unwrap_or_default();
            let turn = payload
                .get("turnIndex")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            item.detail = Some(format!(
                "Provider turn {turn} has {count} active tool descriptor(s)."
            ));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::PolicyDecision => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = payload_string(&payload, "code")
                .or_else(|| Some("owned_agent_policy_decision".into()));
            item.title = Some("Policy decision".into());
            let action = payload_string(&payload, "action").unwrap_or_else(|| "allow".into());
            let tool = payload_string(&payload, "toolName").unwrap_or_else(|| "tool".into());
            let explanation = payload_string(&payload, "explanation")
                .unwrap_or_else(|| "Central safety policy evaluated the tool call.".into());
            item.detail = Some(format!("{action}: {tool}: {explanation}"));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::StateTransition => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_state_transition".into());
            item.title = Some("Agent state".into());
            let to = payload_string(&payload, "to").unwrap_or_else(|| "unknown".into());
            let reason =
                payload_string(&payload, "reason").unwrap_or_else(|| "State changed.".into());
            item.detail = Some(format!("{to}: {reason}"));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::PlanUpdated => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_plan_updated".into());
            item.title = Some("Plan updated".into());
            let total = payload
                .get("total")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            let completed = payload
                .get("completed")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or_default();
            item.detail = Some(format!(
                "Structured plan has {total} item(s), {completed} completed."
            ));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::VerificationGate => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_verification_gate".into());
            item.title = Some("Verification gate".into());
            item.detail = payload_string(&payload, "message")
                .or_else(|| Some("Completion verification gate evaluated.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::ActionRequired => {
            item.kind = RuntimeStreamItemKind::ActionRequired;
            item.action_id = payload_string(&payload, "actionId")
                .or_else(|| Some(format!("owned-agent-action-{event_id}")));
            item.boundary_id = Some("owned_agent".into());
            item.action_type =
                payload_string(&payload, "actionType").or_else(|| Some("operator_review".into()));
            item.title =
                payload_string(&payload, "title").or_else(|| Some("Action required".into()));
            item.detail = payload_string(&payload, "detail")
                .or_else(|| payload_string(&payload, "message"))
                .or_else(|| payload_string(&payload, "reason"))
                .or_else(|| Some("Owned agent requires operator input before continuing.".into()));
            item.code = payload_string(&payload, "code");
            item.message = payload_string(&payload, "message");
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RunPaused => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code =
                payload_string(&payload, "code").or_else(|| Some("owned_agent_paused".into()));
            item.title = Some("Run paused".into());
            item.detail = payload_string(&payload, "message")
                .or_else(|| Some("Owned agent run paused.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RunCompleted => {
            item.kind = RuntimeStreamItemKind::Complete;
            item.detail = payload_string(&payload, "summary")
                .or_else(|| Some("Owned agent run completed.".into()));
            item.text = item.detail.clone();
        }
        AgentRunEventKind::RunFailed => {
            item.kind = RuntimeStreamItemKind::Failure;
            item.code =
                payload_string(&payload, "code").or_else(|| Some("owned_agent_failed".into()));
            item.message = payload_string(&payload, "message")
                .or_else(|| Some("Owned agent run failed.".into()));
            item.retryable = payload_bool(&payload, "retryable").or(Some(false));
            item.text = item.message.clone();
        }
    }

    Some(item)
}

fn should_emit_owned_runtime_item(
    requested: &[RuntimeStreamItemKind],
    kind: &RuntimeStreamItemKind,
) -> bool {
    kind == &RuntimeStreamItemKind::Failure || requested.contains(kind)
}

fn payload_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn payload_bool(payload: &serde_json::Value, key: &str) -> Option<bool> {
    payload.get(key).and_then(|value| value.as_bool())
}

fn payload_transcript_role(payload: &serde_json::Value) -> Option<RuntimeStreamTranscriptRole> {
    match payload_string(payload, "role")?.as_str() {
        "user" => Some(RuntimeStreamTranscriptRole::User),
        "assistant" => Some(RuntimeStreamTranscriptRole::Assistant),
        "system" => Some(RuntimeStreamTranscriptRole::System),
        "tool" => Some(RuntimeStreamTranscriptRole::Tool),
        _ => None,
    }
}

fn command_output_summary(payload: &serde_json::Value) -> String {
    let argv = payload
        .get("argv")
        .and_then(|value| value.as_array())
        .map(|parts| {
            parts
                .iter()
                .filter_map(|value| value.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .filter(|command| !command.trim().is_empty())
        .unwrap_or_else(|| "command".into());
    if let Some(operation) = payload_string(payload, "operation") {
        return format!("Command session {operation}: {argv}.");
    }
    if payload_bool(payload, "timedOut").unwrap_or(false) {
        return format!("Command timed out: {argv}.");
    }
    match payload.get("exitCode").and_then(|value| value.as_i64()) {
        Some(code) => format!("Command exited with status {code}: {argv}."),
        None => format!("Command output: {argv}."),
    }
}

fn resolve_channel<R: Runtime>(
    webview: &Webview<R>,
    raw_channel: Option<&str>,
) -> CommandResult<Channel<RuntimeStreamItemDto>> {
    let Some(raw_channel) = raw_channel else {
        return Err(CommandError::user_fixable(
            "runtime_stream_channel_missing",
            "Xero requires a runtime stream channel before it can start streaming selected-project runtime items.",
        ));
    };

    let channel_id = JavaScriptChannelId::from_str(raw_channel).map_err(|_| {
        CommandError::user_fixable(
            "runtime_stream_channel_invalid",
            "Xero received an invalid runtime stream channel handle from the desktop shell.",
        )
    })?;

    Ok(channel_id.channel_on(webview.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event_kind: AgentRunEventKind, payload_json: &str) -> AgentEventRecord {
        AgentEventRecord {
            id: 42,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind,
            payload_json: payload_json.into(),
            created_at: "2026-04-24T00:00:00Z".into(),
        }
    }

    #[test]
    fn owned_agent_event_projection_maps_tool_and_action_items() {
        let tool = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-1","toolName":"read","ok":false,"code":"tool_failed","message":"nope"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("tool item");
        assert_eq!(tool.kind, RuntimeStreamItemKind::Tool);
        assert_eq!(tool.tool_call_id.as_deref(), Some("call-1"));
        assert_eq!(tool.tool_state, Some(RuntimeToolCallState::Failed));
        assert_eq!(tool.code.as_deref(), Some("tool_failed"));

        let action = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ActionRequired,
                r#"{"actionId":"plan-mode-before-tools","actionType":"plan_mode","title":"Plan required","message":"pause"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("action item");
        assert_eq!(action.kind, RuntimeStreamItemKind::ActionRequired);
        assert_eq!(action.action_id.as_deref(), Some("plan-mode-before-tools"));
        assert_eq!(action.boundary_id.as_deref(), Some("owned_agent"));
        assert_eq!(action.action_type.as_deref(), Some("plan_mode"));
        assert_eq!(action.detail.as_deref(), Some("pause"));

        let fallback_action = owned_agent_event_runtime_item(
            event(AgentRunEventKind::ActionRequired, r#"{}"#),
            "owned-agent:run-1",
            None,
        )
        .expect("fallback action item");
        assert_eq!(
            fallback_action.detail.as_deref(),
            Some("Owned agent requires operator input before continuing.")
        );
    }

    #[test]
    fn owned_agent_event_projection_populates_strict_activity_and_complete_fields() {
        let activity = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":"Checked repository instructions."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("activity item");
        assert_eq!(activity.kind, RuntimeStreamItemKind::Activity);
        assert_eq!(activity.code.as_deref(), Some("owned_agent_reasoning"));
        assert_eq!(activity.title.as_deref(), Some("Reasoning"));
        assert_eq!(
            activity.detail.as_deref(),
            Some("Checked repository instructions.")
        );

        let complete = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::RunCompleted,
                r#"{"summary":"Owned agent run completed."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("complete item");
        assert_eq!(complete.kind, RuntimeStreamItemKind::Complete);
        assert_eq!(
            complete.detail.as_deref(),
            Some("Owned agent run completed.")
        );
    }

    #[test]
    fn owned_agent_event_projection_preserves_transcript_role() {
        let user = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"user","text":"Review this diff."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("user transcript item");

        assert_eq!(user.kind, RuntimeStreamItemKind::Transcript);
        assert_eq!(
            user.transcript_role,
            Some(RuntimeStreamTranscriptRole::User)
        );
        assert_eq!(user.text.as_deref(), Some("Review this diff."));

        let assistant = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":"I'll take a look."}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("assistant transcript item");

        assert_eq!(
            assistant.transcript_role,
            Some(RuntimeStreamTranscriptRole::Assistant)
        );
    }
}
