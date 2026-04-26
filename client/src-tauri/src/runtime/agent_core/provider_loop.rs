use sha2::{Digest, Sha256};

use super::*;

#[expect(
    clippy::too_many_arguments,
    reason = "The provider loop is the orchestration boundary for run identity, tool state, and cancellation."
)]
pub(crate) fn drive_provider_loop(
    provider: &dyn ProviderAdapter,
    mut messages: Vec<ProviderMessage>,
    controls: RuntimeRunControlStateDto,
    mut tool_registry: ToolRegistry,
    tool_runtime: &AutonomousToolRuntime,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    agent_session_id: &str,
    cancellation: &AgentRunCancellationToken,
) -> CommandResult<()> {
    let mut workspace_guard = AgentWorkspaceGuard::default();
    let mut usage_total = ProviderUsage::default();

    for turn_index in 0..MAX_PROVIDER_TURNS {
        cancellation.check_cancelled()?;
        touch_agent_run_heartbeat(repo_root, project_id, run_id)?;
        let turn_system_prompt = assemble_system_prompt_for_session(
            repo_root,
            Some(project_id),
            Some(agent_session_id),
            tool_registry.descriptors(),
        )?;
        let turn = ProviderTurnRequest {
            system_prompt: turn_system_prompt,
            messages: messages.clone(),
            tools: tool_registry.descriptors().to_vec(),
            turn_index,
            controls: controls.clone(),
        };

        let outcome = provider.stream_turn(&turn, &mut |event| {
            cancellation.check_cancelled()?;
            record_provider_stream_event(repo_root, project_id, run_id, event)
        })?;
        cancellation.check_cancelled()?;
        touch_agent_run_heartbeat(repo_root, project_id, run_id)?;

        match outcome {
            ProviderTurnOutcome::Complete { message, usage } => {
                merge_provider_usage(&mut usage_total, usage);
                if !message.trim().is_empty() {
                    append_message(
                        repo_root,
                        project_id,
                        run_id,
                        AgentMessageRole::Assistant,
                        message,
                    )?;
                }
                persist_provider_usage(
                    repo_root,
                    project_id,
                    run_id,
                    provider.provider_id(),
                    provider.model_id(),
                    &usage_total,
                )?;
                return Ok(());
            }
            ProviderTurnOutcome::ToolCalls {
                message,
                tool_calls,
                usage,
            } => {
                merge_provider_usage(&mut usage_total, usage);
                if tool_calls.is_empty() {
                    return Err(CommandError::system_fault(
                        "agent_provider_turn_invalid",
                        "Cadence received a provider tool-turn outcome without tool calls.",
                    ));
                }

                if !message.trim().is_empty() {
                    append_message(
                        repo_root,
                        project_id,
                        run_id,
                        AgentMessageRole::Assistant,
                        message.clone(),
                    )?;
                }

                if controls.active.plan_mode_required
                    && turn_index == 0
                    && !messages.iter().any(|message| {
                        matches!(
                            message,
                            ProviderMessage::Assistant { .. } | ProviderMessage::Tool { .. }
                        )
                    })
                {
                    return Err(record_plan_mode_action_required(
                        repo_root,
                        project_id,
                        run_id,
                        &tool_calls,
                    )?);
                }

                messages.push(ProviderMessage::Assistant {
                    content: message,
                    tool_calls: tool_calls.clone(),
                });

                for tool_call in tool_calls {
                    cancellation.check_cancelled()?;
                    let result = dispatch_tool_call(
                        &tool_registry,
                        tool_runtime,
                        repo_root,
                        project_id,
                        run_id,
                        &mut workspace_guard,
                        tool_call.clone(),
                    )?;
                    cancellation.check_cancelled()?;
                    let result_content = serde_json::to_string(&result).map_err(|error| {
                        CommandError::system_fault(
                            "agent_tool_result_serialize_failed",
                            format!("Cadence could not serialize owned-agent tool result: {error}"),
                        )
                    })?;
                    append_message(
                        repo_root,
                        project_id,
                        run_id,
                        AgentMessageRole::Tool,
                        result_content.clone(),
                    )?;
                    messages.push(ProviderMessage::Tool {
                        tool_call_id: result.tool_call_id.clone(),
                        tool_name: result.tool_name.clone(),
                        content: result_content,
                    });
                    touch_agent_run_heartbeat(repo_root, project_id, run_id)?;
                    if let Some(granted_tools) = granted_tools_from_tool_access_result(&result) {
                        tool_registry.expand_with_tool_names(granted_tools);
                    }
                }
            }
        }
    }

    Err(CommandError::retryable(
        "agent_provider_turn_limit_exceeded",
        format!(
            "Cadence stopped the owned-agent model loop after {MAX_PROVIDER_TURNS} provider turns to prevent an infinite tool loop."
        ),
    ))
}

fn record_plan_mode_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_calls: &[AgentToolCall],
) -> CommandResult<CommandError> {
    let tool_names = tool_calls
        .iter()
        .map(|tool_call| tool_call.tool_name.as_str())
        .collect::<Vec<_>>();
    let tool_list = tool_names.join(", ");
    let message = format!(
        "Plan mode is enabled, so Cadence paused before executing provider-requested tool call(s): {tool_list}. Ask the agent to provide or confirm a plan before resuming tool execution."
    );
    let action_id = sanitize_action_id("plan-mode-before-tools");
    record_action_request(
        repo_root,
        project_id,
        run_id,
        &action_id,
        "plan_mode",
        "Plan required",
        &message,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "actionId": action_id,
            "actionType": "plan_mode",
            "title": "Plan required",
            "code": "agent_plan_mode_requires_approval",
            "message": message,
            "toolNames": tool_names,
        }),
    )?;
    Ok(CommandError::new(
        "agent_plan_mode_requires_approval",
        CommandErrorClass::PolicyDenied,
        message,
        false,
    ))
}

fn record_provider_stream_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    event: ProviderStreamEvent,
) -> CommandResult<()> {
    match event {
        ProviderStreamEvent::MessageDelta(text) => append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::MessageDelta,
            json!({ "role": "assistant", "text": text }),
        )
        .map(|_| ()),
        ProviderStreamEvent::ReasoningSummary(text) => append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ReasoningSummary,
            json!({ "summary": text }),
        )
        .map(|_| ()),
        ProviderStreamEvent::ToolDelta {
            tool_call_id,
            tool_name,
            arguments_delta,
        } => append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ToolDelta,
            json!({
                "toolCallId": tool_call_id,
                "toolName": tool_name,
                "argumentsDelta": arguments_delta,
            }),
        )
        .map(|_| ()),
        ProviderStreamEvent::Usage(usage) => append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ReasoningSummary,
            json!({
                "summary": "Provider usage updated.",
                "usage": usage,
            }),
        )
        .map(|_| ()),
    }
}

pub(crate) fn provider_messages_from_snapshot(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<Vec<ProviderMessage>> {
    let active_compaction = project_store::load_active_agent_compaction(
        repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
    )?
    .filter(|compaction| {
        compaction.covered_run_ids.len() == 1 && compaction.covers_run(&snapshot.run.run_id)
    });
    if let Some(compaction) = active_compaction.as_ref() {
        if compaction.provider_id != snapshot.run.provider_id
            || compaction.model_id != snapshot.run.model_id
        {
            return Err(CommandError::user_fixable(
                "agent_compaction_provider_mismatch",
                format!(
                    "Cadence cannot replay compaction `{}` for run `{}` because it was created with `{}/{}` but the run uses `{}/{}`.",
                    compaction.compaction_id,
                    snapshot.run.run_id,
                    compaction.provider_id,
                    compaction.model_id,
                    snapshot.run.provider_id,
                    snapshot.run.model_id
                ),
            ));
        }
        let current_hash = replay_compaction_source_hash(snapshot, compaction)?;
        if current_hash != compaction.source_hash {
            return Err(CommandError::user_fixable(
                "agent_compaction_source_mismatch",
                format!(
                    "Cadence cannot replay compaction `{}` because the covered transcript changed after compaction. Refresh the Context panel and compact again before continuing.",
                    compaction.compaction_id
                ),
            ));
        }
    }

    let superseded_tool_message_ids = superseded_tool_message_ids(&snapshot.messages)?;
    let tool_calls_by_id = snapshot
        .tool_calls
        .iter()
        .map(|tool_call| {
            let input = serde_json::from_str::<JsonValue>(&tool_call.input_json).map_err(|error| {
                CommandError::system_fault(
                    "agent_transcript_tool_input_decode_failed",
                    format!(
                        "Cadence could not decode persisted tool input `{}` while rebuilding provider state: {error}",
                        tool_call.tool_call_id
                    ),
                )
            })?;
            Ok((
                tool_call.tool_call_id.clone(),
                AgentToolCall {
                    tool_call_id: tool_call.tool_call_id.clone(),
                    tool_name: tool_call.tool_name.clone(),
                    input,
                },
            ))
        })
        .collect::<CommandResult<BTreeMap<_, _>>>()?;

    let mut messages = Vec::new();
    if let Some(compaction) = active_compaction.as_ref() {
        messages.push(ProviderMessage::User {
            content: format!(
                "Compacted prior session context from Cadence. Raw transcript rows are still durable for search/export, but replay should use this summary plus the raw tail below.\n\n{}",
                compaction.summary
            ),
        });
    }
    for message in &snapshot.messages {
        if active_compaction
            .as_ref()
            .is_some_and(|compaction| compaction.covers_message_id(message.id))
        {
            continue;
        }
        match &message.role {
            AgentMessageRole::System => {}
            AgentMessageRole::Developer | AgentMessageRole::User => {
                messages.push(ProviderMessage::User {
                    content: message.content.clone(),
                });
            }
            AgentMessageRole::Assistant => {
                messages.push(ProviderMessage::Assistant {
                    content: message.content.clone(),
                    tool_calls: Vec::new(),
                });
            }
            AgentMessageRole::Tool => {
                if superseded_tool_message_ids.contains(&message.id) {
                    continue;
                }
                let result = serde_json::from_str::<AgentToolResult>(&message.content).map_err(
                    |error| {
                        CommandError::system_fault(
                            "agent_transcript_tool_result_decode_failed",
                            format!(
                                "Cadence could not decode persisted tool result while rebuilding provider state: {error}"
                            ),
                        )
                    },
                )?;
                if let Some(tool_call) = tool_calls_by_id.get(&result.tool_call_id).cloned() {
                    match messages.last_mut() {
                        Some(ProviderMessage::Assistant { tool_calls, .. })
                            if !tool_calls
                                .iter()
                                .any(|call| call.tool_call_id == result.tool_call_id) =>
                        {
                            tool_calls.push(tool_call);
                        }
                        _ => messages.push(ProviderMessage::Assistant {
                            content: String::new(),
                            tool_calls: vec![tool_call],
                        }),
                    }
                }
                messages.push(ProviderMessage::Tool {
                    tool_call_id: result.tool_call_id,
                    tool_name: result.tool_name,
                    content: message.content.clone(),
                });
            }
        }
    }

    Ok(messages)
}

fn replay_compaction_source_hash(
    snapshot: &AgentRunSnapshotRecord,
    compaction: &project_store::AgentCompactionRecord,
) -> CommandResult<String> {
    let mut hasher = Sha256::new();
    hasher.update(snapshot.run.run_id.as_bytes());
    hasher.update(snapshot.run.provider_id.as_bytes());
    hasher.update(snapshot.run.model_id.as_bytes());
    hasher.update(snapshot.run.prompt.as_bytes());
    for message in snapshot
        .messages
        .iter()
        .filter(|message| compaction.covers_message_id(message.id))
    {
        hasher.update(message.id.to_string().as_bytes());
        hasher.update(format!("{:?}", message.role).as_bytes());
        hasher.update(message.content.as_bytes());
    }
    if let (Some(start), Some(end)) = (
        compaction.covered_event_start_id,
        compaction.covered_event_end_id,
    ) {
        for event in snapshot
            .events
            .iter()
            .filter(|event| event.id >= start && event.id <= end)
        {
            hasher.update(event.id.to_string().as_bytes());
            hasher.update(event.run_id.as_bytes());
            hasher.update(format!("{:?}", event.event_kind).as_bytes());
            hasher.update(event.payload_json.as_bytes());
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn tool_registry_for_snapshot(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
    controls: &RuntimeRunControlStateDto,
    skill_tool_enabled: bool,
) -> CommandResult<ToolRegistry> {
    let prompt_context = snapshot
        .messages
        .iter()
        .filter(|message| {
            matches!(
                message.role,
                AgentMessageRole::Developer | AgentMessageRole::User
            )
        })
        .map(|message| message.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let prompt_context = if prompt_context.trim().is_empty() {
        snapshot.run.prompt.as_str()
    } else {
        prompt_context.as_str()
    };

    let mut registry = ToolRegistry::for_prompt_with_options(
        repo_root,
        prompt_context,
        controls,
        ToolRegistryOptions { skill_tool_enabled },
    );
    registry.expand_with_tool_names(granted_tools_from_snapshot(snapshot)?);
    Ok(registry)
}

fn granted_tools_from_snapshot(
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<BTreeSet<String>> {
    let mut granted_tools = BTreeSet::new();
    for tool_call in &snapshot.tool_calls {
        if tool_call.state != AgentToolCallState::Succeeded
            || tool_call.tool_name != AUTONOMOUS_TOOL_TOOL_ACCESS
        {
            continue;
        }
        let Some(result_json) = tool_call.result_json.as_deref() else {
            continue;
        };
        let result =
            serde_json::from_str::<AutonomousToolResult>(result_json).map_err(|error| {
                CommandError::system_fault(
                    "agent_tool_access_result_decode_failed",
                    format!(
                        "Cadence could not decode persisted tool-access result `{}`: {error}",
                        tool_call.tool_call_id
                    ),
                )
            })?;
        if let AutonomousToolOutput::ToolAccess(output) = result.output {
            granted_tools.extend(output.granted_tools);
        }
    }
    Ok(granted_tools)
}

fn granted_tools_from_tool_access_result(result: &AgentToolResult) -> Option<Vec<String>> {
    if result.tool_name != AUTONOMOUS_TOOL_TOOL_ACCESS || !result.ok {
        return None;
    }
    let result = serde_json::from_value::<AutonomousToolResult>(result.output.clone()).ok()?;
    match result.output {
        AutonomousToolOutput::ToolAccess(output) => Some(output.granted_tools),
        _ => None,
    }
}

fn superseded_tool_message_ids(messages: &[AgentMessageRecord]) -> CommandResult<BTreeSet<i64>> {
    let mut latest_by_tool_call_id = BTreeMap::new();
    let mut superseded = BTreeSet::new();
    for message in messages
        .iter()
        .filter(|message| message.role == AgentMessageRole::Tool)
    {
        let result = serde_json::from_str::<AgentToolResult>(&message.content).map_err(|error| {
            CommandError::system_fault(
                "agent_transcript_tool_result_decode_failed",
                format!(
                    "Cadence could not decode persisted tool result while checking replay supersession: {error}"
                ),
            )
        })?;
        if let Some(previous_id) = latest_by_tool_call_id.insert(result.tool_call_id, message.id) {
            superseded.insert(previous_id);
        }
    }
    Ok(superseded)
}

fn merge_provider_usage(total: &mut ProviderUsage, usage: Option<ProviderUsage>) {
    let Some(usage) = usage else {
        return;
    };
    total.input_tokens = total.input_tokens.saturating_add(usage.input_tokens);
    total.output_tokens = total.output_tokens.saturating_add(usage.output_tokens);
    total.total_tokens = total.total_tokens.saturating_add(usage.total_tokens);
}

fn persist_provider_usage(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    provider_id: &str,
    model_id: &str,
    usage: &ProviderUsage,
) -> CommandResult<()> {
    project_store::upsert_agent_usage(
        repo_root,
        &project_store::AgentUsageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            estimated_cost_micros: 0,
            updated_at: now_timestamp(),
        },
    )
}
