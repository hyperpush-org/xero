use super::*;

pub fn run_owned_agent_task(
    request: OwnedAgentRunRequest,
) -> CommandResult<AgentRunSnapshotRecord> {
    create_owned_agent_run(&request)?;
    drive_owned_agent_run(request, AgentRunCancellationToken::default())
}

pub fn create_owned_agent_run(
    request: &OwnedAgentRunRequest,
) -> CommandResult<AgentRunSnapshotRecord> {
    validate_prompt(&request.prompt)?;
    project_store::ensure_agent_session_active(
        &request.repo_root,
        &request.project_id,
        &request.agent_session_id,
    )?;

    let controls = runtime_controls_from_request(request.controls.as_ref());
    let tool_registry = ToolRegistry::for_prompt_with_options(
        &request.repo_root,
        &request.prompt,
        &controls,
        ToolRegistryOptions {
            skill_tool_enabled: request.tool_runtime.skill_tool_enabled(),
        },
    );
    let system_prompt = assemble_system_prompt_for_session(
        &request.repo_root,
        Some(&request.project_id),
        Some(&request.agent_session_id),
        tool_registry.descriptors(),
    )?;
    let provider = create_provider_adapter(request.provider_config.clone())?;
    let now = now_timestamp();

    project_store::insert_agent_run(
        &request.repo_root,
        &NewAgentRunRecord {
            project_id: request.project_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            run_id: request.run_id.clone(),
            provider_id: provider.provider_id().to_string(),
            model_id: provider.model_id().to_string(),
            prompt: request.prompt.clone(),
            system_prompt: system_prompt.clone(),
            now: now.clone(),
        },
    )?;

    append_message(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentMessageRole::System,
        system_prompt.clone(),
    )?;
    append_message(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentMessageRole::User,
        request.prompt.clone(),
    )?;
    append_event(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentRunEventKind::ValidationStarted,
        json!({
            "label": "repo_preflight",
            "fingerprint": repo_fingerprint(&request.repo_root),
        }),
    )?;
    append_event(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentRunEventKind::ValidationCompleted,
        json!({
            "label": "repo_preflight",
            "outcome": "passed",
        }),
    )?;

    project_store::update_agent_run_status(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentRunStatus::Running,
        None,
        &now_timestamp(),
    )?;

    project_store::load_agent_run(&request.repo_root, &request.project_id, &request.run_id)
}

pub fn drive_owned_agent_run(
    request: OwnedAgentRunRequest,
    cancellation: AgentRunCancellationToken,
) -> CommandResult<AgentRunSnapshotRecord> {
    cancellation.check_cancelled()?;
    let snapshot =
        project_store::load_agent_run(&request.repo_root, &request.project_id, &request.run_id)?;
    let controls = runtime_controls_from_request(request.controls.as_ref());
    let skill_tool_enabled = request.tool_runtime.skill_tool_enabled();
    let base_tool_runtime = request
        .tool_runtime
        .with_runtime_run_controls(controls.clone())
        .with_cancellation_token(cancellation.clone());
    let tool_registry =
        tool_registry_for_snapshot(&request.repo_root, &snapshot, &controls, skill_tool_enabled)?;
    let provider = create_provider_adapter(request.provider_config.clone())?;
    if provider.provider_id() != snapshot.run.provider_id
        || provider.model_id() != snapshot.run.model_id
    {
        return Err(CommandError::user_fixable(
            "agent_run_provider_mismatch",
            format!(
                "Cadence cannot drive run `{}` with provider `{}/{}` because the run was created with `{}/{}`.",
                request.run_id,
                provider.provider_id(),
                provider.model_id(),
                snapshot.run.provider_id,
                snapshot.run.model_id
            ),
        ));
    }
    let tool_runtime = tool_runtime_with_subagent_executor(
        base_tool_runtime,
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        &snapshot.run.agent_session_id,
        controls.clone(),
        request.provider_config.clone(),
        cancellation.clone(),
    );
    let messages = provider_messages_from_snapshot(&request.repo_root, &snapshot)?;

    match drive_provider_loop(
        provider.as_ref(),
        messages,
        controls.clone(),
        tool_registry,
        &tool_runtime,
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        &snapshot.run.agent_session_id,
        &cancellation,
    ) {
        Ok(()) => {
            append_event(
                &request.repo_root,
                &request.project_id,
                &request.run_id,
                AgentRunEventKind::RunCompleted,
                json!({ "summary": "Owned agent run completed." }),
            )?;
            project_store::update_agent_run_status(
                &request.repo_root,
                &request.project_id,
                &request.run_id,
                AgentRunStatus::Completed,
                None,
                &now_timestamp(),
            )
        }
        Err(error) => finish_owned_agent_drive_error(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
            error,
            &cancellation,
        ),
    }
}

pub fn cancel_owned_agent_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<AgentRunSnapshotRecord> {
    mark_owned_agent_run_cancelled(repo_root, project_id, run_id)
}

pub fn append_user_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    prompt: String,
) -> CommandResult<AgentRunSnapshotRecord> {
    validate_prompt(&prompt)?;
    append_message(
        repo_root,
        project_id,
        run_id,
        AgentMessageRole::User,
        prompt.clone(),
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::MessageDelta,
        json!({ "role": "user", "text": prompt }),
    )?;
    project_store::load_agent_run(repo_root, project_id, run_id)
}

pub fn continue_owned_agent_run(
    request: ContinueOwnedAgentRunRequest,
) -> CommandResult<AgentRunSnapshotRecord> {
    prepare_owned_agent_continuation(&request)?;
    drive_owned_agent_continuation(request, AgentRunCancellationToken::default())
}

pub fn prepare_owned_agent_continuation(
    request: &ContinueOwnedAgentRunRequest,
) -> CommandResult<AgentRunSnapshotRecord> {
    validate_prompt(&request.prompt)?;
    let mut before =
        project_store::load_agent_run(&request.repo_root, &request.project_id, &request.run_id)?;
    if matches!(
        before.run.status,
        AgentRunStatus::Cancelling | AgentRunStatus::Cancelled
    ) {
        return Err(CommandError::user_fixable(
            "agent_run_not_resumable",
            format!(
                "Cadence cannot continue owned agent run `{}` because it is {:?}.",
                request.run_id, before.run.status
            ),
        ));
    }

    let provider = create_provider_adapter(request.provider_config.clone())?;
    if provider.provider_id() != before.run.provider_id
        || provider.model_id() != before.run.model_id
    {
        return Err(CommandError::user_fixable(
            "agent_run_provider_mismatch",
            format!(
                "Cadence cannot continue run `{}` with provider `{}/{}` because the run was created with `{}/{}`.",
                request.run_id,
                provider.provider_id(),
                provider.model_id(),
                before.run.provider_id,
                before.run.model_id
            ),
        ));
    }

    maybe_auto_compact_before_continuation(request, provider.as_ref(), &before)?;
    before =
        project_store::load_agent_run(&request.repo_root, &request.project_id, &request.run_id)?;
    ensure_context_budget_allows_continuation(request, &before)?;

    if request.answer_pending_actions {
        project_store::answer_pending_agent_action_requests(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
            &request.prompt,
        )?;
        before = project_store::load_agent_run(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
        )?;
        let tool_registry = ToolRegistry::builtin_with_options(ToolRegistryOptions {
            skill_tool_enabled: request.tool_runtime.skill_tool_enabled(),
        });
        let replay_tool_runtime = request
            .tool_runtime
            .clone()
            .with_runtime_run_controls(runtime_controls_from_request(request.controls.as_ref()));
        replay_answered_tool_action_requests(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
            &tool_registry,
            &replay_tool_runtime,
            &before,
        )?;
        before = project_store::load_agent_run(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
        )?;
    }

    mark_interrupted_tool_calls_before_continuation(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        &before,
    )?;

    append_message(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentMessageRole::User,
        request.prompt.clone(),
    )?;
    append_event(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentRunEventKind::MessageDelta,
        json!({ "role": "user", "text": request.prompt }),
    )?;
    project_store::update_agent_run_status(
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        AgentRunStatus::Running,
        None,
        &now_timestamp(),
    )
}

fn ensure_context_budget_allows_continuation(
    request: &ContinueOwnedAgentRunRequest,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let Some(budget_tokens) =
        provider_context_budget_tokens(&snapshot.run.provider_id, &snapshot.run.model_id)
    else {
        return Ok(());
    };

    let estimate = estimate_continuation_context_tokens(request, snapshot)?;
    let budget = context_budget(estimate.estimated_tokens, Some(budget_tokens));
    if budget.pressure != SessionContextBudgetPressureDto::Over {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "agent_context_budget_exceeded",
        format!(
            "Cadence estimated {} tokens for run `{}` after this continuation, which exceeds the known {} token context budget for `{}/{}`. Open the Context panel, shorten the prompt, or start a fresh session before continuing.",
            budget.estimated_tokens,
            snapshot.run.run_id,
            budget_tokens,
            snapshot.run.provider_id,
            snapshot.run.model_id
        ),
    ))
}

fn maybe_auto_compact_before_continuation(
    request: &ContinueOwnedAgentRunRequest,
    provider: &dyn ProviderAdapter,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let Some(preference) = request.auto_compact.as_ref() else {
        return Ok(());
    };
    if !preference.enabled {
        return Ok(());
    }
    let active_compaction = project_store::load_active_agent_compaction(
        &request.repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
    )?;
    if active_compaction.is_some() {
        return Ok(());
    }
    let estimate = estimate_continuation_context_tokens(request, snapshot)?;
    let decision = evaluate_compaction_policy(SessionCompactionPolicyInput {
        manual_requested: false,
        auto_enabled: true,
        provider_supports_compaction: true,
        active_compaction_present: false,
        estimated_tokens: estimate.estimated_tokens,
        budget_tokens: estimate.budget_tokens,
        threshold_percent: preference.threshold_percent,
    });
    if decision.action != SessionContextPolicyActionDto::CompactNow {
        return Ok(());
    }
    let compaction = crate::commands::session_history::compact_session_history_with_provider(
        &request.repo_root,
        &snapshot.run.project_id,
        &snapshot.run.agent_session_id,
        Some(&snapshot.run.run_id),
        preference.raw_tail_message_count,
        project_store::AgentCompactionTrigger::Auto,
        &decision.reason_code,
        provider,
    )?;
    append_event(
        &request.repo_root,
        &snapshot.run.project_id,
        &snapshot.run.run_id,
        AgentRunEventKind::ValidationCompleted,
        json!({
            "label": "auto_compact",
            "outcome": "passed",
            "compactionId": compaction.compaction_id,
            "reasonCode": decision.reason_code,
            "estimatedTokens": estimate.estimated_tokens,
            "budgetTokens": estimate.budget_tokens,
        }),
    )?;
    Ok(())
}

struct ContinuationContextEstimate {
    estimated_tokens: u64,
    budget_tokens: Option<u64>,
}

fn estimate_continuation_context_tokens(
    request: &ContinueOwnedAgentRunRequest,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<ContinuationContextEstimate> {
    let budget_tokens =
        provider_context_budget_tokens(&snapshot.run.provider_id, &snapshot.run.model_id);
    let controls = runtime_controls_from_request(request.controls.as_ref());
    let tool_registry = tool_registry_for_snapshot(
        &request.repo_root,
        snapshot,
        &controls,
        request.tool_runtime.skill_tool_enabled(),
    )?;
    let system_prompt = assemble_system_prompt_for_session(
        &request.repo_root,
        Some(&snapshot.run.project_id),
        Some(&snapshot.run.agent_session_id),
        tool_registry.descriptors(),
    )?;
    let provider_messages = provider_messages_from_snapshot(&request.repo_root, snapshot)?;
    let message_tokens = provider_messages.iter().try_fold(0_u64, |total, message| {
        let serialized = serde_json::to_string(message).map_err(|error| {
            CommandError::system_fault(
                "agent_context_message_serialize_failed",
                format!(
                    "Cadence could not estimate context size for run `{}`: {error}",
                    snapshot.run.run_id
                ),
            )
        })?;
        Ok(total.saturating_add(estimate_tokens(&serialized)))
    })?;
    let tool_descriptor_tokens =
        tool_registry
            .descriptors()
            .iter()
            .try_fold(0_u64, |total, descriptor| {
                let serialized = serde_json::to_string(descriptor).map_err(|error| {
                    CommandError::system_fault(
                        "agent_context_tool_descriptor_serialize_failed",
                        format!(
                            "Cadence could not estimate tool context size for run `{}`: {error}",
                            snapshot.run.run_id
                        ),
                    )
                })?;
                Ok(total.saturating_add(estimate_tokens(&serialized)))
            })?;
    let estimated_tokens = estimate_tokens(&system_prompt)
        .saturating_add(message_tokens)
        .saturating_add(tool_descriptor_tokens)
        .saturating_add(estimate_tokens(&request.prompt));
    Ok(ContinuationContextEstimate {
        estimated_tokens,
        budget_tokens,
    })
}

pub fn drive_owned_agent_continuation(
    request: ContinueOwnedAgentRunRequest,
    cancellation: AgentRunCancellationToken,
) -> CommandResult<AgentRunSnapshotRecord> {
    cancellation.check_cancelled()?;
    let before =
        project_store::load_agent_run(&request.repo_root, &request.project_id, &request.run_id)?;
    if matches!(
        before.run.status,
        AgentRunStatus::Cancelling | AgentRunStatus::Cancelled
    ) {
        return Err(CommandError::user_fixable(
            "agent_run_not_resumable",
            format!(
                "Cadence cannot continue owned agent run `{}` because it is {:?}.",
                request.run_id, before.run.status
            ),
        ));
    }

    let provider_config = request.provider_config.clone();
    let provider = create_provider_adapter(provider_config.clone())?;
    if provider.provider_id() != before.run.provider_id
        || provider.model_id() != before.run.model_id
    {
        return Err(CommandError::user_fixable(
            "agent_run_provider_mismatch",
            format!(
                "Cadence cannot continue run `{}` with provider `{}/{}` because the run was created with `{}/{}`.",
                request.run_id,
                provider.provider_id(),
                provider.model_id(),
                before.run.provider_id,
                before.run.model_id
            ),
        ));
    }
    let snapshot =
        project_store::load_agent_run(&request.repo_root, &request.project_id, &request.run_id)?;
    let messages = provider_messages_from_snapshot(&request.repo_root, &snapshot)?;
    let controls = runtime_controls_from_request(request.controls.as_ref());
    let skill_tool_enabled = request.tool_runtime.skill_tool_enabled();
    let base_tool_runtime = request
        .tool_runtime
        .with_runtime_run_controls(controls.clone())
        .with_cancellation_token(cancellation.clone());
    let tool_registry =
        tool_registry_for_snapshot(&request.repo_root, &snapshot, &controls, skill_tool_enabled)?;
    let tool_runtime = tool_runtime_with_subagent_executor(
        base_tool_runtime,
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        &snapshot.run.agent_session_id,
        controls.clone(),
        provider_config,
        cancellation.clone(),
    );
    match drive_provider_loop(
        provider.as_ref(),
        messages,
        controls,
        tool_registry,
        &tool_runtime,
        &request.repo_root,
        &request.project_id,
        &request.run_id,
        &snapshot.run.agent_session_id,
        &cancellation,
    ) {
        Ok(()) => {
            append_event(
                &request.repo_root,
                &request.project_id,
                &request.run_id,
                AgentRunEventKind::RunCompleted,
                json!({ "summary": "Owned agent run continued and completed." }),
            )?;
            project_store::update_agent_run_status(
                &request.repo_root,
                &request.project_id,
                &request.run_id,
                AgentRunStatus::Completed,
                None,
                &now_timestamp(),
            )
        }
        Err(error) => finish_owned_agent_drive_error(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
            error,
            &cancellation,
        ),
    }
}

fn replay_answered_tool_action_requests(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_registry: &ToolRegistry,
    tool_runtime: &AutonomousToolRuntime,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    let answered_tool_action_ids = snapshot
        .action_requests
        .iter()
        .filter(|action| action.status == "answered")
        .map(|action| action.action_id.as_str())
        .collect::<BTreeSet<_>>();
    if answered_tool_action_ids.is_empty() {
        return Ok(());
    }

    let mut workspace_guard = AgentWorkspaceGuard::default();
    for tool_call in &snapshot.tool_calls {
        let Some(replay_kind) = answered_tool_replay_kind(tool_call, &answered_tool_action_ids)?
        else {
            continue;
        };
        let input = serde_json::from_str::<JsonValue>(&tool_call.input_json).map_err(|error| {
            CommandError::system_fault(
                "agent_tool_replay_input_decode_failed",
                format!(
                    "Cadence could not decode approved tool call `{}` before replay: {error}",
                    tool_call.tool_call_id
                ),
            )
        })?;
        let result = dispatch_tool_call_with_write_approval(
            tool_registry,
            tool_runtime,
            repo_root,
            project_id,
            run_id,
            &mut workspace_guard,
            AgentToolCall {
                tool_call_id: tool_call.tool_call_id.clone(),
                tool_name: tool_call.tool_name.clone(),
                input,
            },
            replay_kind == AnsweredToolReplayKind::ApprovedExistingWrite,
            matches!(
                replay_kind,
                AnsweredToolReplayKind::OperatorApprovedCommand
                    | AnsweredToolReplayKind::OperatorApprovedSystemRead
            ),
        )?;
        let result_content = serde_json::to_string(&result).map_err(|error| {
            CommandError::system_fault(
                "agent_tool_result_serialize_failed",
                format!("Cadence could not serialize approved owned-agent tool result: {error}"),
            )
        })?;
        append_message(
            repo_root,
            project_id,
            run_id,
            AgentMessageRole::Tool,
            result_content,
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnsweredToolReplayKind {
    ApprovedExistingWrite,
    OperatorApprovedCommand,
    OperatorApprovedSystemRead,
}

fn answered_tool_replay_kind(
    tool_call: &project_store::AgentToolCallRecord,
    answered_tool_action_ids: &BTreeSet<&str>,
) -> CommandResult<Option<AnsweredToolReplayKind>> {
    let action_id = sanitize_action_id(&format!("tool-{}", tool_call.tool_call_id));
    if tool_call.state == AgentToolCallState::Failed
        && answered_tool_action_ids.contains(action_id.as_str())
        && tool_call.error.as_ref().is_some_and(|error| {
            RERUNNABLE_APPROVED_TOOL_ERROR_CODES
                .iter()
                .any(|code| *code == error.code)
        })
    {
        return Ok(Some(AnsweredToolReplayKind::ApprovedExistingWrite));
    }

    if tool_call.state == AgentToolCallState::Failed
        && answered_tool_action_ids.contains(action_id.as_str())
        && tool_call
            .error
            .as_ref()
            .is_some_and(|error| error.code == "autonomous_tool_system_read_requires_approval")
    {
        return Ok(Some(AnsweredToolReplayKind::OperatorApprovedSystemRead));
    }

    if tool_call.state == AgentToolCallState::Succeeded
        && command_approval_action_id_for_tool_call(tool_call)?
            .as_deref()
            .is_some_and(|action_id| answered_tool_action_ids.contains(action_id))
    {
        return Ok(Some(AnsweredToolReplayKind::OperatorApprovedCommand));
    }

    Ok(None)
}

fn command_approval_action_id_for_tool_call(
    tool_call: &project_store::AgentToolCallRecord,
) -> CommandResult<Option<String>> {
    let Some(result_json) = tool_call.result_json.as_deref() else {
        return Ok(None);
    };
    let result = serde_json::from_str::<AutonomousToolResult>(result_json).map_err(|error| {
        CommandError::system_fault(
            "agent_tool_replay_result_decode_failed",
            format!(
                "Cadence could not decode tool call `{}` while checking approval replay state: {error}",
                tool_call.tool_call_id
            ),
        )
    })?;

    let argv = match result.output {
        AutonomousToolOutput::Command(output) if !output.spawned => output.argv,
        AutonomousToolOutput::CommandSession(output) if !output.spawned => output.argv,
        AutonomousToolOutput::ProcessManager(output)
            if !output.spawned
                && matches!(
                    output.action,
                    AutonomousProcessManagerAction::Start
                        | AutonomousProcessManagerAction::AsyncStart
                ) =>
        {
            let Some(process) = output.processes.first() else {
                return Ok(None);
            };
            process.command.argv.clone()
        }
        _ => return Ok(None),
    };
    Ok(Some(sanitize_action_id(&format!(
        "command-{}",
        argv.join("-")
    ))))
}

fn mark_interrupted_tool_calls_before_continuation(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<()> {
    for tool_call in snapshot.tool_calls.iter().filter(|tool_call| {
        matches!(
            tool_call.state,
            AgentToolCallState::Pending | AgentToolCallState::Running
        )
    }) {
        let message = format!(
            "Cadence marked tool call `{}` interrupted before resuming owned-agent run `{}`.",
            tool_call.tool_call_id, run_id
        );
        project_store::finish_agent_tool_call(
            repo_root,
            &AgentToolCallFinishRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                tool_call_id: tool_call.tool_call_id.clone(),
                state: AgentToolCallState::Failed,
                result_json: None,
                error: Some(project_store::AgentRunDiagnosticRecord {
                    code: INTERRUPTED_TOOL_CALL_CODE.into(),
                    message: message.clone(),
                }),
                completed_at: now_timestamp(),
            },
        )?;
        append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ToolCompleted,
            json!({
                "toolCallId": tool_call.tool_call_id,
                "toolName": tool_call.tool_name,
                "ok": false,
                "code": INTERRUPTED_TOOL_CALL_CODE,
                "message": message,
            }),
        )?;
    }
    Ok(())
}

fn finish_owned_agent_drive_error(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    error: CommandError,
    cancellation: &AgentRunCancellationToken,
) -> CommandResult<AgentRunSnapshotRecord> {
    if cancellation.is_cancelled() || error.code == AGENT_RUN_CANCELLED_CODE {
        return mark_owned_agent_run_cancelled(repo_root, project_id, run_id);
    }

    let diagnostic = project_store::AgentRunDiagnosticRecord {
        code: error.code.clone(),
        message: error.message.clone(),
    };
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::RunFailed,
        json!({
            "code": error.code,
            "message": error.message,
            "retryable": error.retryable,
        }),
    )?;
    project_store::update_agent_run_status(
        repo_root,
        project_id,
        run_id,
        AgentRunStatus::Failed,
        Some(diagnostic),
        &now_timestamp(),
    )
}

fn mark_owned_agent_run_cancelled(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<AgentRunSnapshotRecord> {
    if let Ok(snapshot) = project_store::load_agent_run(repo_root, project_id, run_id) {
        if snapshot.run.status == AgentRunStatus::Cancelled {
            return Ok(snapshot);
        }
    }

    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::RunFailed,
        json!({ "code": AGENT_RUN_CANCELLED_CODE, "message": "Owned agent run was cancelled." }),
    )?;
    project_store::update_agent_run_status(
        repo_root,
        project_id,
        run_id,
        AgentRunStatus::Cancelled,
        None,
        &now_timestamp(),
    )
}

#[allow(clippy::too_many_arguments)]
fn tool_runtime_with_subagent_executor(
    tool_runtime: AutonomousToolRuntime,
    repo_root: &Path,
    project_id: &str,
    parent_run_id: &str,
    agent_session_id: &str,
    controls: RuntimeRunControlStateDto,
    provider_config: AgentProviderConfig,
    cancellation: AgentRunCancellationToken,
) -> AutonomousToolRuntime {
    if tool_runtime.subagent_execution_depth > 0 {
        return tool_runtime;
    }
    let child_tool_runtime = tool_runtime
        .clone()
        .without_subagent_executor()
        .with_subagent_execution_depth(tool_runtime.subagent_execution_depth + 1);
    tool_runtime.with_subagent_executor(Arc::new(OwnedAgentSubagentExecutor {
        repo_root: repo_root.to_path_buf(),
        project_id: project_id.to_owned(),
        parent_run_id: parent_run_id.to_owned(),
        agent_session_id: agent_session_id.to_owned(),
        controls,
        provider_config,
        tool_runtime: child_tool_runtime,
        cancellation,
    }))
}

#[derive(Clone)]
struct OwnedAgentSubagentExecutor {
    repo_root: PathBuf,
    project_id: String,
    parent_run_id: String,
    agent_session_id: String,
    controls: RuntimeRunControlStateDto,
    provider_config: AgentProviderConfig,
    tool_runtime: AutonomousToolRuntime,
    cancellation: AgentRunCancellationToken,
}

impl AutonomousSubagentExecutor for OwnedAgentSubagentExecutor {
    fn execute_subagent(
        &self,
        mut task: AutonomousSubagentTask,
    ) -> CommandResult<AutonomousSubagentTask> {
        self.cancellation.check_cancelled()?;
        let child_run_id =
            sanitize_action_id(&format!("{}-{}", self.parent_run_id, task.subagent_id));
        let model_id = task
            .model_id
            .as_deref()
            .unwrap_or(self.controls.active.model_id.as_str())
            .to_owned();
        let provider_config = route_provider_config_model(self.provider_config.clone(), &model_id);
        let prompt = subagent_prompt(&task, &self.parent_run_id);
        let request = OwnedAgentRunRequest {
            repo_root: self.repo_root.clone(),
            project_id: self.project_id.clone(),
            agent_session_id: self.agent_session_id.clone(),
            run_id: child_run_id.clone(),
            prompt,
            controls: Some(RuntimeRunControlInputDto {
                provider_profile_id: self.controls.active.provider_profile_id.clone(),
                model_id,
                thinking_effort: self.controls.active.thinking_effort.clone(),
                approval_mode: self.controls.active.approval_mode.clone(),
                plan_mode_required: false,
            }),
            tool_runtime: self.tool_runtime.clone(),
            provider_config,
        };

        create_owned_agent_run(&request)?;
        let snapshot = drive_owned_agent_run(request, self.cancellation.clone())?;
        task.run_id = Some(child_run_id);
        task.completed_at = Some(now_timestamp());
        task.status = match snapshot.run.status {
            AgentRunStatus::Completed => "completed".into(),
            AgentRunStatus::Cancelled => "cancelled".into(),
            AgentRunStatus::Failed => "failed".into(),
            _ => format!("{:?}", snapshot.run.status).to_ascii_lowercase(),
        };
        task.result_summary = Some(subagent_result_summary(&snapshot));
        Ok(task)
    }
}

fn route_provider_config_model(
    mut provider_config: AgentProviderConfig,
    model_id: &str,
) -> AgentProviderConfig {
    if model_id.trim().is_empty() {
        return provider_config;
    }
    match &mut provider_config {
        AgentProviderConfig::Fake => {}
        AgentProviderConfig::OpenAiResponses(config) => config.model_id = model_id.into(),
        AgentProviderConfig::OpenAiCompatible(config) => config.model_id = model_id.into(),
        AgentProviderConfig::Anthropic(config) => config.model_id = model_id.into(),
        AgentProviderConfig::Bedrock(config) => config.model_id = model_id.into(),
        AgentProviderConfig::Vertex(config) => config.model_id = model_id.into(),
    }
    provider_config
}

fn subagent_prompt(task: &AutonomousSubagentTask, parent_run_id: &str) -> String {
    format!(
        "You are a {:?} subagent for parent owned-agent run `{parent_run_id}`. Work only on this focused task, return concise findings, and do not change files unless the task explicitly requires it.\n\n{}",
        task.agent_type,
        task.prompt
    )
}

fn subagent_result_summary(snapshot: &AgentRunSnapshotRecord) -> String {
    if let Some(error) = snapshot.run.last_error.as_ref() {
        return format!("{}: {}", error.code, error.message);
    }
    snapshot
        .messages
        .iter()
        .rev()
        .find(|message| message.role == AgentMessageRole::Assistant)
        .map(|message| message.content.trim().to_owned())
        .filter(|content| !content.is_empty())
        .unwrap_or_else(|| {
            format!(
                "Subagent run finished with status {:?}.",
                snapshot.run.status
            )
        })
}
