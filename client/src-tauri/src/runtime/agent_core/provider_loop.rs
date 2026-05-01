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
    let mut workspace_guard =
        AgentWorkspaceGuard::new(tool_runtime.subagent_write_scope().cloned());
    let mut usage_total = ProviderUsage::default();
    let task_classification =
        classify_agent_task(&provider_messages_task_text(&messages), &controls);
    let mut verification_gate_prompt_count = 0_u8;

    for turn_index in 0..MAX_PROVIDER_TURNS {
        cancellation.check_cancelled()?;
        touch_agent_run_heartbeat(repo_root, project_id, run_id)?;
        let owned_process_summary = tool_runtime.owned_process_lifecycle_summary()?;
        let skill_contexts = skill_contexts_from_provider_messages(&messages)?;
        let turn_context_package = assemble_provider_context_package(
            ProviderContextPackageInput {
                repo_root,
                project_id,
                agent_session_id,
                run_id,
                runtime_agent_id: controls.active.runtime_agent_id,
                provider_id: provider.provider_id(),
                model_id: provider.model_id(),
                turn_index,
                browser_control_preference: tool_runtime.browser_control_preference(),
                soul_settings: Some(tool_runtime.soul_settings()),
                tools: tool_registry.descriptors(),
                messages: &messages,
                owned_process_summary: owned_process_summary.as_deref(),
            },
            skill_contexts,
        )?;
        let _manifest_id = turn_context_package.manifest.manifest_id.as_str();
        let _fragment_count = turn_context_package.compilation.fragments.len();
        record_tool_registry_snapshot(repo_root, project_id, run_id, turn_index, &tool_registry)?;
        let turn = ProviderTurnRequest {
            system_prompt: turn_context_package.system_prompt,
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
                let snapshot = project_store::load_agent_run(repo_root, project_id, run_id)?;
                let gate = if controls.active.runtime_agent_id.allows_verification_gate() {
                    evaluate_completion_gate(&snapshot, &message)
                } else {
                    VerificationGateDecision {
                        status: VerificationGateStatus::NotRequired,
                        message: format!(
                            "{} agent does not use command-based verification gates.",
                            controls.active.runtime_agent_id.label()
                        ),
                        evidence: None,
                        latest_file_change_event_id: None,
                    }
                };
                record_state_transition(
                    repo_root,
                    project_id,
                    run_id,
                    AgentStateTransition {
                        from: None,
                        to: AgentRunState::Summarize,
                        reason: "Provider returned a candidate final response.",
                        stop_reason: None,
                        extra: None,
                    },
                )?;
                record_completion_gate(repo_root, project_id, run_id, &gate)?;
                if gate.status == VerificationGateStatus::Required {
                    if provider_usage_has_tokens(&usage_total) {
                        persist_provider_usage(
                            repo_root,
                            project_id,
                            run_id,
                            provider.provider_id(),
                            provider.model_id(),
                            &usage_total,
                        )?;
                    }
                    if verification_gate_prompt_count == 0 {
                        verification_gate_prompt_count =
                            verification_gate_prompt_count.saturating_add(1);
                        let gate_prompt = verification_gate_prompt(&gate);
                        record_state_transition(
                            repo_root,
                            project_id,
                            run_id,
                            AgentStateTransition {
                                from: Some(AgentRunState::Summarize),
                                to: AgentRunState::Verify,
                                reason:
                                    "Completion was held until verification evidence is recorded.",
                                stop_reason: None,
                                extra: None,
                            },
                        )?;
                        append_message(
                            repo_root,
                            project_id,
                            run_id,
                            AgentMessageRole::Developer,
                            gate_prompt.clone(),
                        )?;
                        messages.push(ProviderMessage::User {
                            content: gate_prompt,
                        });
                        if controls.active.runtime_agent_id.allows_verification_gate() {
                            tool_registry.expand_with_tool_names([
                                AUTONOMOUS_TOOL_COMMAND,
                                AUTONOMOUS_TOOL_COMMAND_SESSION_START,
                                AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
                                AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
                            ]);
                        }
                        continue;
                    }
                    return Err(record_verification_action_required(
                        repo_root, project_id, run_id, &gate,
                    )?);
                }
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
                record_state_transition(
                    repo_root,
                    project_id,
                    run_id,
                    AgentStateTransition {
                        from: Some(AgentRunState::Summarize),
                        to: AgentRunState::Complete,
                        reason: "Completion gate passed.",
                        stop_reason: Some(AgentRunStopReason::Complete),
                        extra: Some(json!({
                            "verificationStatus": gate.status.as_str(),
                        })),
                    },
                )?;
                return Ok(());
            }
            ProviderTurnOutcome::ToolCalls {
                message,
                tool_calls,
                usage,
            } => {
                let received_usage = usage
                    .as_ref()
                    .map(provider_usage_has_tokens)
                    .unwrap_or(false);
                merge_provider_usage(&mut usage_total, usage);
                if received_usage {
                    persist_provider_usage(
                        repo_root,
                        project_id,
                        run_id,
                        provider.provider_id(),
                        provider.model_id(),
                        &usage_total,
                    )?;
                }
                if tool_calls.is_empty() {
                    return Err(CommandError::system_fault(
                        "agent_provider_turn_invalid",
                        "Xero received a provider tool-turn outcome without tool calls.",
                    ));
                }

                let snapshot = project_store::load_agent_run(repo_root, project_id, run_id)?;
                match evaluate_tool_batch_gate(
                    &snapshot,
                    &controls,
                    &task_classification,
                    &tool_calls,
                ) {
                    ToolBatchGate::Allow => {}
                    ToolBatchGate::RequirePlan { message } => {
                        record_plan_gate_message(
                            repo_root,
                            project_id,
                            run_id,
                            &message,
                            &task_classification,
                        )?;
                        append_message(
                            repo_root,
                            project_id,
                            run_id,
                            AgentMessageRole::Developer,
                            message.clone(),
                        )?;
                        messages.push(ProviderMessage::User { content: message });
                        continue;
                    }
                    ToolBatchGate::RequirePlanApproval { action_id, message } => {
                        return Err(record_plan_review_action_required(
                            repo_root, project_id, run_id, &action_id, &message,
                        )?);
                    }
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

                if tool_batch_contains_execution(&tool_calls) {
                    record_state_transition(
                        repo_root,
                        project_id,
                        run_id,
                        AgentStateTransition {
                            from: None,
                            to: AgentRunState::Execute,
                            reason: "Provider requested execution-capable tool calls.",
                            stop_reason: None,
                            extra: Some(json!({
                                "toolNames": tool_calls
                                    .iter()
                                    .map(|tool_call| tool_call.tool_name.as_str())
                                    .collect::<Vec<_>>(),
                            })),
                        },
                    )?;
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
                            format!("Xero could not serialize owned-agent tool result: {error}"),
                        )
                    })?;
                    record_plan_artifact_from_tool_result(repo_root, project_id, run_id, &result)?;
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
                        tool_registry
                            .expand_with_tool_names_from_runtime(granted_tools, tool_runtime)?;
                    }
                }
            }
        }
    }

    Err(CommandError::retryable(
        "agent_provider_turn_limit_exceeded",
        format!(
            "Xero stopped the owned-agent model loop after {MAX_PROVIDER_TURNS} provider turns to prevent an infinite tool loop."
        ),
    ))
}

fn provider_messages_task_text(messages: &[ProviderMessage]) -> String {
    messages
        .iter()
        .filter_map(|message| match message {
            ProviderMessage::User { content } => Some(content.as_str()),
            ProviderMessage::Assistant { .. } | ProviderMessage::Tool { .. } => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn record_tool_registry_snapshot(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    turn_index: usize,
    registry: &ToolRegistry,
) -> CommandResult<()> {
    let descriptors = registry.descriptors();
    let tool_names = descriptors
        .iter()
        .map(|descriptor| descriptor.name.as_str())
        .collect::<Vec<_>>();
    let catalog = descriptors
        .iter()
        .filter_map(|descriptor| {
            tool_catalog_metadata_for_tool(&descriptor.name, true).or_else(|| {
                registry
                    .dynamic_routes()
                    .get(&descriptor.name)
                    .map(|route| dynamic_tool_catalog_metadata(descriptor, route))
            })
        })
        .collect::<Vec<_>>();
    let dynamic_routes = registry
        .dynamic_routes()
        .iter()
        .map(|(tool_name, route)| {
            json!({
                "toolName": tool_name,
                "route": route,
            })
        })
        .collect::<Vec<_>>();
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ToolRegistrySnapshot,
        json!({
            "kind": "active_tool_registry",
            "promptVersion": SYSTEM_PROMPT_VERSION,
            "turnIndex": turn_index,
            "runtimeAgentId": registry.runtime_agent_id().as_str(),
            "runtimeAgentLabel": registry.runtime_agent_id().label(),
            "toolNames": tool_names,
            "catalog": catalog,
            "dynamicRoutes": dynamic_routes,
            "descriptors": descriptors,
        }),
    )?;
    Ok(())
}

fn dynamic_tool_catalog_metadata(
    descriptor: &AgentToolDescriptor,
    route: &AutonomousDynamicToolRoute,
) -> JsonValue {
    match route {
        AutonomousDynamicToolRoute::McpTool {
            server_id,
            tool_name,
        } => json!({
            "toolName": descriptor.name.as_str(),
            "group": "mcp",
            "catalogKind": "mcp_tool",
            "activationGroups": ["mcp_invoke"],
            "activationTools": [descriptor.name.as_str()],
            "tags": ["mcp", "model_context_protocol", "tool", server_id, tool_name],
            "schemaFields": descriptor
                .input_schema
                .get("properties")
                .and_then(JsonValue::as_object)
                .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
                .unwrap_or_default(),
            "examples": [format!("Call `{}` after exact MCP activation.", descriptor.name)],
            "riskClass": "external_capability_invoke",
            "effectClass": "external_service",
            "allowedRuntimeAgents": [RuntimeAgentIdDto::Engineer.as_str(), RuntimeAgentIdDto::Debug.as_str()],
            "runtimeAvailable": true,
            "source": server_id,
            "trust": "connected_mcp_server",
            "approvalStatus": "allowed",
        }),
    }
}

fn record_verification_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    gate: &VerificationGateDecision,
) -> CommandResult<CommandError> {
    let action_id = sanitize_action_id("verification-required");
    let message = verification_gate_prompt(gate);
    record_state_transition(
        repo_root,
        project_id,
        run_id,
        AgentStateTransition {
            from: Some(AgentRunState::Verify),
            to: AgentRunState::ApprovalWait,
            reason: "Verification gate still lacked evidence after a retry.",
            stop_reason: Some(AgentRunStopReason::WaitingForApproval),
            extra: None,
        },
    )?;
    record_action_request(
        repo_root,
        project_id,
        run_id,
        &action_id,
        "verification_required",
        "Verification required",
        &message,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "actionId": action_id,
            "actionType": "verification_required",
            "title": "Verification required",
            "code": "agent_verification_required",
            "message": message,
            "stopReason": AgentRunStopReason::WaitingForApproval.as_str(),
            "state": AgentRunState::ApprovalWait.as_str(),
        }),
    )?;
    Ok(CommandError::new(
        "agent_verification_required",
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
                    "Xero cannot replay compaction `{}` for run `{}` because it was created with `{}/{}` but the run uses `{}/{}`.",
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
                    "Xero cannot replay compaction `{}` because the covered transcript changed after compaction. Refresh the Context panel and compact again before continuing.",
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
                        "Xero could not decode persisted tool input `{}` while rebuilding provider state: {error}",
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
                "Compacted prior session context from Xero. Raw transcript rows are still durable for search/export, but replay should use this summary plus the raw tail below.\n\n{}",
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
                                "Xero could not decode persisted tool result while rebuilding provider state: {error}"
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
    browser_control_preference: BrowserControlPreferenceDto,
    tool_runtime: Option<&AutonomousToolRuntime>,
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

    let prompt_registry = ToolRegistry::for_prompt_with_options(
        repo_root,
        prompt_context,
        controls,
        ToolRegistryOptions {
            skill_tool_enabled,
            browser_control_preference,
            runtime_agent_id: controls.active.runtime_agent_id,
        },
    );
    let options = ToolRegistryOptions {
        skill_tool_enabled,
        browser_control_preference,
        runtime_agent_id: controls.active.runtime_agent_id,
    };
    let latest_registry = latest_tool_registry_snapshot(snapshot)?;
    let mut registry = if let Some(latest_registry) = latest_registry {
        ToolRegistry::from_descriptors_with_dynamic_routes(
            latest_registry.descriptors,
            latest_registry.dynamic_routes,
            options,
        )
    } else {
        ToolRegistry::for_tool_names_with_options(prompt_registry.descriptor_names(), options)
    };
    registry.expand_with_tool_names(prompt_registry.descriptor_names());
    let granted_tools = granted_tools_from_snapshot(snapshot)?;
    if let Some(tool_runtime) = tool_runtime {
        registry.expand_with_tool_names_from_runtime(granted_tools, tool_runtime)?;
    } else {
        registry.expand_with_tool_names(granted_tools);
    }
    Ok(registry)
}

#[derive(Debug, Clone)]
struct PersistedToolRegistrySnapshot {
    descriptors: Vec<AgentToolDescriptor>,
    dynamic_routes: BTreeMap<String, AutonomousDynamicToolRoute>,
}

fn latest_tool_registry_snapshot(
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<Option<PersistedToolRegistrySnapshot>> {
    snapshot
        .events
        .iter()
        .rev()
        .filter(|event| event.event_kind == AgentRunEventKind::ToolRegistrySnapshot)
        .find_map(|event| {
            let payload = serde_json::from_str::<JsonValue>(&event.payload_json).ok()?;
            if payload.get("kind").and_then(JsonValue::as_str) != Some("active_tool_registry") {
                return None;
            }
            Some(payload)
        })
        .map(parse_tool_registry_snapshot_payload)
        .transpose()
}

fn parse_tool_registry_snapshot_payload(
    payload: JsonValue,
) -> CommandResult<PersistedToolRegistrySnapshot> {
    let descriptors = payload
        .get("descriptors")
        .cloned()
        .map(serde_json::from_value::<Vec<AgentToolDescriptor>>)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_tool_registry_snapshot_decode_failed",
                format!("Xero could not decode persisted tool descriptors: {error}"),
            )
        })?
        .unwrap_or_else(|| {
            payload
                .get("toolNames")
                .and_then(JsonValue::as_array)
                .into_iter()
                .flatten()
                .filter_map(JsonValue::as_str)
                .map(str::to_owned)
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|tool_name| {
                    ToolRegistry::for_tool_names(BTreeSet::from([tool_name.clone()]))
                        .descriptor(&tool_name)
                        .cloned()
                })
                .collect()
        });
    let mut dynamic_routes = BTreeMap::new();
    for route in payload
        .get("dynamicRoutes")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
    {
        let Some(tool_name) = route.get("toolName").and_then(JsonValue::as_str) else {
            continue;
        };
        let Some(route_value) = route.get("route").cloned() else {
            continue;
        };
        let route =
            serde_json::from_value::<AutonomousDynamicToolRoute>(route_value).map_err(|error| {
                CommandError::system_fault(
                    "agent_tool_registry_snapshot_decode_failed",
                    format!("Xero could not decode persisted dynamic tool route: {error}"),
                )
            })?;
        dynamic_routes.insert(tool_name.to_owned(), route);
    }
    Ok(PersistedToolRegistrySnapshot {
        descriptors,
        dynamic_routes,
    })
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
                        "Xero could not decode persisted tool-access result `{}`: {error}",
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

pub(crate) fn skill_contexts_from_provider_messages(
    messages: &[ProviderMessage],
) -> CommandResult<Vec<XeroSkillToolContextPayload>> {
    let mut contexts = Vec::new();
    for message in messages {
        let ProviderMessage::Tool { content, .. } = message else {
            continue;
        };
        let result = serde_json::from_str::<AgentToolResult>(content).map_err(|error| {
            CommandError::system_fault(
                "agent_skill_context_tool_result_decode_failed",
                format!(
                    "Xero could not decode persisted tool result while collecting skill prompt context: {error}"
                ),
            )
        })?;
        if !result.ok {
            continue;
        }
        let result =
            serde_json::from_value::<AutonomousToolResult>(result.output).map_err(|error| {
                CommandError::system_fault(
                    "agent_skill_context_output_decode_failed",
                    format!(
                        "Xero could not decode owned-agent tool output while collecting skill prompt context: {error}"
                    ),
                )
            })?;
        if let AutonomousToolOutput::Skill(output) = result.output {
            if let Some(context) = output.context {
                contexts.push(context);
            }
        }
    }
    Ok(contexts)
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
                    "Xero could not decode persisted tool result while checking replay supersession: {error}"
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
    total.cache_read_tokens = total
        .cache_read_tokens
        .saturating_add(usage.cache_read_tokens);
    total.cache_creation_tokens = total
        .cache_creation_tokens
        .saturating_add(usage.cache_creation_tokens);
    total.reported_cost_micros = match (total.reported_cost_micros, usage.reported_cost_micros) {
        (Some(total_cost), Some(next_cost)) => Some(total_cost.saturating_add(next_cost)),
        (Some(total_cost), None) => Some(total_cost),
        (None, Some(next_cost)) => Some(next_cost),
        (None, None) => None,
    };
}

fn provider_usage_has_tokens(usage: &ProviderUsage) -> bool {
    usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.total_tokens > 0
        || usage.cache_read_tokens > 0
        || usage.cache_creation_tokens > 0
        || usage
            .reported_cost_micros
            .is_some_and(|reported_cost| reported_cost > 0)
}

fn persist_provider_usage(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    provider_id: &str,
    model_id: &str,
    usage: &ProviderUsage,
) -> CommandResult<()> {
    let estimated_cost_micros = usage.reported_cost_micros.unwrap_or_else(|| {
        crate::runtime::pricing::estimate_cost_micros(
            provider_id,
            model_id,
            crate::runtime::pricing::UsageForPricing {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                cache_read_tokens: usage.cache_read_tokens,
                cache_creation_tokens: usage.cache_creation_tokens,
            },
        )
    });
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
            cache_read_tokens: usage.cache_read_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            estimated_cost_micros,
            updated_at: now_timestamp(),
        },
    )?;
    crate::runtime::usage_events::emit_agent_usage_updated(project_id, run_id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_provider_usage_sums_reported_costs() {
        let mut total = ProviderUsage::default();

        merge_provider_usage(
            &mut total,
            Some(ProviderUsage {
                input_tokens: 10,
                total_tokens: 10,
                reported_cost_micros: Some(25),
                ..ProviderUsage::default()
            }),
        );
        merge_provider_usage(
            &mut total,
            Some(ProviderUsage {
                output_tokens: 5,
                total_tokens: 5,
                reported_cost_micros: Some(75),
                ..ProviderUsage::default()
            }),
        );

        assert_eq!(total.input_tokens, 10);
        assert_eq!(total.output_tokens, 5);
        assert_eq!(total.total_tokens, 15);
        assert_eq!(total.reported_cost_micros, Some(100));
        assert!(provider_usage_has_tokens(&total));
    }
}
