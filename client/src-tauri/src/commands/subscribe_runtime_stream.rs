use std::{path::Path, str::FromStr, thread, time::Duration};

use tauri::{
    ipc::{Channel, JavaScriptChannelId},
    AppHandle, Runtime, State, Webview,
};

use crate::{
    commands::{
        validate_non_empty, BrowserComputerUseActionStatusDto, BrowserComputerUseSurfaceDto,
        BrowserComputerUseToolResultSummaryDto, CommandError, CommandResult,
        CommandToolResultSummaryDto, FileToolResultSummaryDto, GitToolResultScopeDto,
        GitToolResultSummaryDto, McpCapabilityKindDto, McpCapabilityToolResultSummaryDto,
        RuntimeStreamItemDto, RuntimeStreamItemKind, RuntimeStreamTranscriptRole,
        RuntimeToolCallState, SubscribeRuntimeStreamRequestDto, SubscribeRuntimeStreamResponseDto,
        ToolResultSummaryDto, WebToolResultContentKindDto, WebToolResultSummaryDto,
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
            | AgentRunStatus::HandedOff
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
            item.text = payload_verbatim_string(&payload, "text");
            item.transcript_role = payload_transcript_role(&payload);
        }
        AgentRunEventKind::ReasoningSummary => {
            item.kind = RuntimeStreamItemKind::Activity;
            if payload.get("usage").is_some() {
                item.code = Some("owned_agent_usage".into());
                item.title = Some("Provider usage".into());
                item.detail = payload_string(&payload, "summary")
                    .or_else(|| Some("Provider usage updated.".into()));
                item.text = item.detail.clone();
            } else {
                item.code = Some("owned_agent_reasoning".into());
                item.title = Some("Reasoning".into());
                item.text = payload_verbatim_string(&payload, "summary");
                item.detail = payload_string(&payload, "summary")
                    .or_else(|| Some("Owned agent reasoning summary updated.".into()));
            }
        }
        AgentRunEventKind::ToolStarted => {
            item.kind = RuntimeStreamItemKind::Tool;
            item.tool_call_id = payload_string(&payload, "toolCallId");
            item.tool_name = payload_string(&payload, "toolName");
            item.tool_state = Some(RuntimeToolCallState::Running);
            item.detail = payload
                .get("input")
                .and_then(|input| tool_started_detail(item.tool_name.as_deref(), input))
                .or_else(|| {
                    item.tool_name
                        .as_ref()
                        .map(|tool_name| format!("Started `{tool_name}`."))
                });
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
            let ok = payload_bool(&payload, "ok").unwrap_or(false);
            item.tool_state = Some(if ok {
                RuntimeToolCallState::Succeeded
            } else {
                RuntimeToolCallState::Failed
            });
            item.detail = payload_string(&payload, "summary")
                .or_else(|| payload_string(&payload, "message"))
                .or_else(|| {
                    item.tool_name
                        .as_ref()
                        .map(|name| format!("Completed `{name}`."))
                });
            item.text = item.detail.clone();
            if ok {
                item.tool_summary = payload
                    .get("output")
                    .and_then(|output| tool_result_summary_from_output(output, ok));
            }
            item.code = payload_string(&payload, "code");
            item.message = payload_string(&payload, "message");
        }
        AgentRunEventKind::FileChanged => {
            item.kind = RuntimeStreamItemKind::Activity;
            item.code = Some("owned_agent_file_changed".into());
            let operation =
                payload_string(&payload, "operation").unwrap_or_else(|| "change".into());
            let path = payload_string(&payload, "path").unwrap_or_else(|| "unknown path".into());
            let actor = payload_string(&payload, "subagentId")
                .zip(payload_string(&payload, "subagentRole"))
                .map(|(subagent_id, role)| format!("{role} {subagent_id}"));
            item.title = Some("File changed".into());
            item.detail = payload_string(&payload, "toPath")
                .map(|to_path| format!("{operation}: {path} -> {to_path}"))
                .or_else(|| Some(format!("{operation}: {path}")))
                .map(|detail| match actor {
                    Some(actor) => format!("{detail} · {actor}"),
                    None => detail,
                });
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

fn tool_started_detail(tool_name: Option<&str>, input: &serde_json::Value) -> Option<String> {
    let mut parts = Vec::new();

    match tool_name.unwrap_or_default() {
        "read" => {
            push_value_part(&mut parts, "path", input, "path");
            push_value_part(&mut parts, "startLine", input, "startLine");
            push_value_part(&mut parts, "lineCount", input, "lineCount");
            push_value_part(&mut parts, "mode", input, "mode");
        }
        "search" => {
            push_value_part(&mut parts, "query", input, "query");
            push_value_part(&mut parts, "path", input, "path");
            push_value_part(&mut parts, "maxResults", input, "maxResults");
        }
        "find" => {
            push_value_part(&mut parts, "pattern", input, "pattern");
            push_value_part(&mut parts, "path", input, "path");
        }
        "list" => {
            push_value_part(&mut parts, "path", input, "path");
            push_value_part(&mut parts, "maxDepth", input, "maxDepth");
        }
        "command" | "command_session_start" => {
            push_value_part(&mut parts, "cwd", input, "cwd");
            push_value_part(&mut parts, "cmd", input, "argv");
            push_value_part(&mut parts, "timeoutMs", input, "timeoutMs");
        }
        "command_session_read" | "command_session_stop" => {
            push_value_part(&mut parts, "sessionId", input, "sessionId");
        }
        "git_diff" => {
            push_value_part(&mut parts, "scope", input, "scope");
        }
        "web_search" | "web_search_only" => {
            push_value_part(&mut parts, "query", input, "query");
            push_value_part(&mut parts, "resultCount", input, "resultCount");
        }
        "web_fetch" => {
            push_value_part(&mut parts, "url", input, "url");
            push_value_part(&mut parts, "maxChars", input, "maxChars");
        }
        _ => push_generic_input_parts(&mut parts, input),
    }

    if parts.is_empty() {
        push_generic_input_parts(&mut parts, input);
    }

    render_tool_detail_parts(parts)
}

fn push_generic_input_parts(parts: &mut Vec<String>, input: &serde_json::Value) {
    for (label, key) in [
        ("path", "path"),
        ("fromPath", "fromPath"),
        ("toPath", "toPath"),
        ("pattern", "pattern"),
        ("query", "query"),
        ("url", "url"),
        ("scope", "scope"),
        ("cwd", "cwd"),
        ("cmd", "argv"),
        ("action", "action"),
        ("serverId", "serverId"),
        ("name", "name"),
        ("uri", "uri"),
    ] {
        push_value_part(parts, label, input, key);
        if parts.len() >= 3 {
            break;
        }
    }
}

fn push_value_part(parts: &mut Vec<String>, label: &str, payload: &serde_json::Value, key: &str) {
    if let Some(value) = payload.get(key).and_then(render_json_scalar) {
        parts.push(format!("{label}: {value}"));
    }
}

fn render_json_scalar(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::String(value) => Some(value.trim().to_owned()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(|value| value.as_str().map(str::trim))
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            Some(joined)
        }
        _ => None,
    }
    .filter(|value| !value.is_empty())
    .map(|value| truncate_chars(&value, 160))
}

fn render_tool_detail_parts(parts: Vec<String>) -> Option<String> {
    if parts.is_empty() {
        return None;
    }

    Some(truncate_chars(&parts.join(", "), 240))
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_owned();
    }

    let keep_chars = max_chars.saturating_sub(3);
    format!("{}...", value.chars().take(keep_chars).collect::<String>())
}

fn tool_result_summary_from_output(
    output: &serde_json::Value,
    ok: bool,
) -> Option<ToolResultSummaryDto> {
    match payload_string(output, "kind")?.as_str() {
        "read" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            payload_usize(output, "lineCount"),
            None,
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "search" => Some(file_tool_summary(
            None,
            payload_string(output, "scope"),
            None,
            payload_usize(output, "totalMatches").or_else(|| payload_array_len(output, "matches")),
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "find" => Some(file_tool_summary(
            None,
            payload_string(output, "scope"),
            None,
            payload_array_len(output, "matches"),
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "edit" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            None,
            payload_usize(output, "replacementLen"),
            false,
        )),
        "write" | "delete" | "mkdir" | "hash" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            None,
            None,
            false,
        )),
        "patch" => Some(file_tool_summary(
            payload_string(output, "path").or_else(|| first_file_path(output)),
            None,
            None,
            payload_usize(output, "replacements"),
            false,
        )),
        "rename" => Some(file_tool_summary(
            payload_string(output, "fromPath"),
            payload_string(output, "toPath"),
            None,
            None,
            false,
        )),
        "list" => Some(file_tool_summary(
            payload_string(output, "path"),
            None,
            None,
            payload_array_len(output, "entries"),
            payload_bool(output, "truncated").unwrap_or(false),
        )),
        "command" => Some(command_tool_summary(output)),
        "command_session" => Some(command_session_tool_summary(output)),
        "git_status" => Some(ToolResultSummaryDto::Git(GitToolResultSummaryDto {
            scope: None,
            changed_files: payload_usize(output, "changedFiles").unwrap_or_default(),
            truncated: false,
            base_revision: None,
        })),
        "git_diff" => Some(ToolResultSummaryDto::Git(GitToolResultSummaryDto {
            scope: payload_string(output, "scope").and_then(|scope| git_scope_from_str(&scope)),
            changed_files: payload_usize(output, "changedFiles").unwrap_or_default(),
            truncated: payload_bool(output, "truncated").unwrap_or(false),
            base_revision: payload_string(output, "baseRevision"),
        })),
        "web_search" => Some(ToolResultSummaryDto::Web(WebToolResultSummaryDto {
            target: payload_string(output, "query")?,
            result_count: payload_array_len(output, "results"),
            final_url: None,
            content_kind: None,
            content_type: None,
            truncated: payload_bool(output, "truncated").unwrap_or(false),
        })),
        "web_fetch" => Some(ToolResultSummaryDto::Web(WebToolResultSummaryDto {
            target: payload_string(output, "url")?,
            result_count: None,
            final_url: payload_string(output, "finalUrl"),
            content_kind: payload_string(output, "contentKind")
                .and_then(|kind| web_content_kind_from_str(&kind)),
            content_type: payload_string(output, "contentType"),
            truncated: payload_bool(output, "truncated").unwrap_or(false),
        })),
        "browser" => Some(ToolResultSummaryDto::BrowserComputerUse(
            BrowserComputerUseToolResultSummaryDto {
                surface: BrowserComputerUseSurfaceDto::Browser,
                action: payload_string(output, "action")?,
                status: browser_status_from_ok(ok),
                target: payload_string(output, "url"),
                outcome: None,
            },
        )),
        "mcp" => mcp_capability_summary_from_output(output),
        _ => None,
    }
}

fn file_tool_summary(
    path: Option<String>,
    scope: Option<String>,
    line_count: Option<usize>,
    match_count: Option<usize>,
    truncated: bool,
) -> ToolResultSummaryDto {
    ToolResultSummaryDto::File(FileToolResultSummaryDto {
        path,
        scope,
        line_count,
        match_count,
        truncated,
    })
}

fn command_tool_summary(output: &serde_json::Value) -> ToolResultSummaryDto {
    ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
        exit_code: payload_i32(output, "exitCode"),
        timed_out: payload_bool(output, "timedOut").unwrap_or(false),
        stdout_truncated: payload_bool(output, "stdoutTruncated").unwrap_or(false),
        stderr_truncated: payload_bool(output, "stderrTruncated").unwrap_or(false),
        stdout_redacted: payload_bool(output, "stdoutRedacted").unwrap_or(false),
        stderr_redacted: payload_bool(output, "stderrRedacted").unwrap_or(false),
    })
}

fn command_session_tool_summary(output: &serde_json::Value) -> ToolResultSummaryDto {
    ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
        exit_code: payload_i32(output, "exitCode"),
        timed_out: false,
        stdout_truncated: command_session_stream_bool(output, "stdout", "truncated"),
        stderr_truncated: command_session_stream_bool(output, "stderr", "truncated"),
        stdout_redacted: command_session_stream_bool(output, "stdout", "redacted"),
        stderr_redacted: command_session_stream_bool(output, "stderr", "redacted"),
    })
}

fn command_session_stream_bool(output: &serde_json::Value, stream: &str, key: &str) -> bool {
    output
        .get("chunks")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .any(|chunk| {
            payload_string(chunk, "stream").as_deref() == Some(stream)
                && payload_bool(chunk, key).unwrap_or(false)
        })
}

fn mcp_capability_summary_from_output(output: &serde_json::Value) -> Option<ToolResultSummaryDto> {
    let action = payload_string(output, "action")?;
    let capability_kind = match action.as_str() {
        "invoke_tool" => McpCapabilityKindDto::Tool,
        "read_resource" => McpCapabilityKindDto::Resource,
        "get_prompt" => McpCapabilityKindDto::Prompt,
        _ => return None,
    };
    let capability_name = payload_string(output, "capabilityName")?;

    Some(ToolResultSummaryDto::McpCapability(
        McpCapabilityToolResultSummaryDto {
            server_id: payload_string(output, "serverId")?,
            capability_kind,
            capability_id: capability_name.clone(),
            capability_name: Some(capability_name),
        },
    ))
}

fn first_file_path(output: &serde_json::Value) -> Option<String> {
    output
        .get("files")
        .and_then(serde_json::Value::as_array)?
        .first()
        .and_then(|file| payload_string(file, "path"))
}

fn payload_usize(payload: &serde_json::Value, key: &str) -> Option<usize> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
}

fn payload_i32(payload: &serde_json::Value, key: &str) -> Option<i32> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn payload_array_len(payload: &serde_json::Value, key: &str) -> Option<usize> {
    payload
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
}

fn git_scope_from_str(scope: &str) -> Option<GitToolResultScopeDto> {
    match scope {
        "staged" => Some(GitToolResultScopeDto::Staged),
        "unstaged" => Some(GitToolResultScopeDto::Unstaged),
        "worktree" => Some(GitToolResultScopeDto::Worktree),
        _ => None,
    }
}

fn web_content_kind_from_str(kind: &str) -> Option<WebToolResultContentKindDto> {
    match kind {
        "html" => Some(WebToolResultContentKindDto::Html),
        "plain_text" => Some(WebToolResultContentKindDto::PlainText),
        _ => None,
    }
}

fn browser_status_from_ok(ok: bool) -> BrowserComputerUseActionStatusDto {
    if ok {
        BrowserComputerUseActionStatusDto::Succeeded
    } else {
        BrowserComputerUseActionStatusDto::Failed
    }
}

fn payload_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn payload_verbatim_string(payload: &serde_json::Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(|value| value.as_str())
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
        assert_eq!(tool.detail.as_deref(), Some("nope"));

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
    fn owned_agent_event_projection_keeps_reasoning_text_visible() {
        let reasoning = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":"I should inspect the latest build output"}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("reasoning item");

        assert_eq!(reasoning.kind, RuntimeStreamItemKind::Activity);
        assert_eq!(reasoning.code.as_deref(), Some("owned_agent_reasoning"));
        assert_eq!(reasoning.title.as_deref(), Some("Reasoning"));
        assert_eq!(
            reasoning.text.as_deref(),
            Some("I should inspect the latest build output")
        );
        assert_eq!(
            reasoning.detail.as_deref(),
            Some("I should inspect the latest build output")
        );

        let whitespace_delta = owned_agent_event_runtime_item(
            event(AgentRunEventKind::ReasoningSummary, r#"{"summary":"\n\n"}"#),
            "owned-agent:run-1",
            None,
        )
        .expect("reasoning whitespace item");

        assert_eq!(
            whitespace_delta.code.as_deref(),
            Some("owned_agent_reasoning")
        );
        assert_eq!(whitespace_delta.text.as_deref(), Some("\n\n"));
        assert_eq!(
            whitespace_delta.detail.as_deref(),
            Some("Owned agent reasoning summary updated.")
        );

        let usage = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ReasoningSummary,
                r#"{"summary":"Provider usage updated.","usage":{"totalTokens":12}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("usage activity item");

        assert_eq!(usage.kind, RuntimeStreamItemKind::Activity);
        assert_eq!(usage.code.as_deref(), Some("owned_agent_usage"));
        assert_eq!(usage.title.as_deref(), Some("Provider usage"));
    }

    #[test]
    fn owned_agent_tool_started_projection_carries_concise_input_detail() {
        let read = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolStarted,
                r#"{"toolCallId":"call-read","toolName":"read","input":{"path":"client/components/xero/agent-runtime.tsx","startLine":12,"lineCount":40,"token":"[REDACTED]"}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("read tool item");
        assert_eq!(
            read.detail.as_deref(),
            Some("path: client/components/xero/agent-runtime.tsx, startLine: 12, lineCount: 40")
        );

        let command = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolStarted,
                r#"{"toolCallId":"call-command","toolName":"command","input":{"cwd":"client","argv":["pnpm","test","--run","agent-runtime.test.tsx"],"timeoutMs":120000}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("command tool item");
        assert_eq!(
            command.detail.as_deref(),
            Some("cwd: client, cmd: pnpm test --run agent-runtime.test.tsx, timeoutMs: 120000")
        );
    }

    #[test]
    fn owned_agent_tool_completed_projection_maps_summary_into_detail() {
        let tool = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-1","toolName":"read","ok":true,"summary":"Read 2 line(s) from `client/src/lib.rs`.","output":{"kind":"read","path":"client/src/lib.rs","lineCount":2,"truncated":false}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("tool item");

        assert_eq!(
            tool.detail.as_deref(),
            Some("Read 2 line(s) from `client/src/lib.rs`.")
        );
        assert_eq!(tool.text, tool.detail);
    }

    #[test]
    fn owned_agent_tool_completed_projection_derives_file_summaries() {
        let read = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-read","toolName":"read","ok":true,"summary":"read","output":{"kind":"read","path":"client/src/lib.rs","lineCount":2,"truncated":false}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("read tool item");
        assert_eq!(
            read.tool_summary,
            Some(ToolResultSummaryDto::File(FileToolResultSummaryDto {
                path: Some("client/src/lib.rs".into()),
                scope: None,
                line_count: Some(2),
                match_count: None,
                truncated: false,
            }))
        );

        let search = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-search","toolName":"search","ok":true,"summary":"search","output":{"kind":"search","query":"appendTranscriptDelta","scope":"client","matches":[{},{}],"totalMatches":4,"truncated":true}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("search tool item");
        assert_eq!(
            search.tool_summary,
            Some(ToolResultSummaryDto::File(FileToolResultSummaryDto {
                path: None,
                scope: Some("client".into()),
                line_count: None,
                match_count: Some(4),
                truncated: true,
            }))
        );

        let find = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-find","toolName":"find","ok":true,"summary":"find","output":{"kind":"find","pattern":"*.rs","scope":"client/src-tauri","matches":["src/lib.rs","src/main.rs"],"truncated":false}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("find tool item");
        assert_eq!(
            find.tool_summary,
            Some(ToolResultSummaryDto::File(FileToolResultSummaryDto {
                path: None,
                scope: Some("client/src-tauri".into()),
                line_count: None,
                match_count: Some(2),
                truncated: false,
            }))
        );
    }

    #[test]
    fn owned_agent_tool_completed_projection_derives_command_git_and_web_summaries() {
        let command = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-command","toolName":"command","ok":true,"summary":"command","output":{"kind":"command","argv":["pnpm","test"],"cwd":"client","exitCode":0,"timedOut":false,"stdoutTruncated":true,"stderrTruncated":false,"stdoutRedacted":false,"stderrRedacted":true}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("command tool item");
        assert_eq!(
            command.tool_summary,
            Some(ToolResultSummaryDto::Command(CommandToolResultSummaryDto {
                exit_code: Some(0),
                timed_out: false,
                stdout_truncated: true,
                stderr_truncated: false,
                stdout_redacted: false,
                stderr_redacted: true,
            }))
        );

        let git = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-git","toolName":"git_diff","ok":true,"summary":"git","output":{"kind":"git_diff","scope":"worktree","changedFiles":3,"truncated":true,"baseRevision":"HEAD~1"}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("git tool item");
        assert_eq!(
            git.tool_summary,
            Some(ToolResultSummaryDto::Git(GitToolResultSummaryDto {
                scope: Some(GitToolResultScopeDto::Worktree),
                changed_files: 3,
                truncated: true,
                base_revision: Some("HEAD~1".into()),
            }))
        );

        let web = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::ToolCompleted,
                r#"{"toolCallId":"call-web","toolName":"web_fetch","ok":true,"summary":"web","output":{"kind":"web_fetch","url":"https://example.com","finalUrl":"https://www.example.com/","contentKind":"html","contentType":"text/html","truncated":false}}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("web tool item");
        assert_eq!(
            web.tool_summary,
            Some(ToolResultSummaryDto::Web(WebToolResultSummaryDto {
                target: "https://example.com".into(),
                result_count: None,
                final_url: Some("https://www.example.com/".into()),
                content_kind: Some(WebToolResultContentKindDto::Html),
                content_type: Some("text/html".into()),
                truncated: false,
            }))
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

    #[test]
    fn owned_agent_event_projection_preserves_transcript_delta_whitespace() {
        let assistant = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":" instructions and natural wrapping "}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("assistant transcript item");

        assert_eq!(
            assistant.text.as_deref(),
            Some(" instructions and natural wrapping ")
        );

        let space_only = owned_agent_event_runtime_item(
            event(
                AgentRunEventKind::MessageDelta,
                r#"{"role":"assistant","text":" "}"#,
            ),
            "owned-agent:run-1",
            None,
        )
        .expect("assistant transcript item");

        assert_eq!(space_only.text.as_deref(), Some(" "));
    }
}
