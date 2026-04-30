use super::*;

pub(crate) fn dispatch_tool_call(
    tool_registry: &ToolRegistry,
    tool_runtime: &AutonomousToolRuntime,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    workspace_guard: &mut AgentWorkspaceGuard,
    tool_call: AgentToolCall,
) -> CommandResult<AgentToolResult> {
    dispatch_tool_call_with_write_approval(
        tool_registry,
        tool_runtime,
        repo_root,
        project_id,
        run_id,
        workspace_guard,
        tool_call,
        false,
        false,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn dispatch_tool_call_with_write_approval(
    tool_registry: &ToolRegistry,
    tool_runtime: &AutonomousToolRuntime,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    workspace_guard: &mut AgentWorkspaceGuard,
    tool_call: AgentToolCall,
    approved_existing_write: bool,
    operator_approved: bool,
) -> CommandResult<AgentToolResult> {
    let started_at = now_timestamp();
    let input_json = serde_json::to_string(&tool_call.input).map_err(|error| {
        CommandError::system_fault(
            "agent_tool_input_serialize_failed",
            format!("Xero could not serialize owned-agent tool input: {error}"),
        )
    })?;
    project_store::start_agent_tool_call(
        repo_root,
        &AgentToolCallStartRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            tool_call_id: tool_call.tool_call_id.clone(),
            tool_name: tool_call.tool_name.clone(),
            input_json,
            started_at: started_at.clone(),
        },
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ToolStarted,
        json!({
            "toolCallId": tool_call.tool_call_id,
            "toolName": tool_call.tool_name,
            "input": tool_call.input,
            "approvedReplay": approved_existing_write || operator_approved,
        }),
    )?;

    let request = match tool_registry.decode_call(&tool_call) {
        Ok(request) => request,
        Err(error) => {
            finish_failed_tool_call(repo_root, project_id, run_id, &tool_call, &error)?;
            return Err(error);
        }
    };

    let write_observations =
        match workspace_guard.validate_write_intent(repo_root, &request, approved_existing_write) {
            Ok(observations) => observations,
            Err(error) => {
                finish_failed_tool_call(repo_root, project_id, run_id, &tool_call, &error)?;
                return Err(error);
            }
        };
    let rollback_checkpoints =
        rollback_checkpoints_for_request(repo_root, &request, &write_observations)?;

    let tool_execution = if operator_approved {
        tool_runtime.execute_approved(request)
    } else {
        tool_runtime.execute(request)
    };

    match tool_execution {
        Ok(tool_result) => {
            let output = serde_json::to_value(&tool_result).map_err(|error| {
                CommandError::system_fault(
                    "agent_tool_result_serialize_failed",
                    format!("Xero could not serialize owned-agent tool output: {error}"),
                )
            })?;
            let result_json = serde_json::to_string(&output).map_err(|error| {
                CommandError::system_fault(
                    "agent_tool_result_serialize_failed",
                    format!("Xero could not persist owned-agent tool output: {error}"),
                )
            })?;
            project_store::finish_agent_tool_call(
                repo_root,
                &AgentToolCallFinishRecord {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    tool_call_id: tool_call.tool_call_id.clone(),
                    state: AgentToolCallState::Succeeded,
                    result_json: Some(result_json),
                    error: None,
                    completed_at: now_timestamp(),
                },
            )?;
            record_file_change_event(
                repo_root,
                project_id,
                run_id,
                &write_observations,
                &tool_result.output,
            )?;
            record_command_output_event(repo_root, project_id, run_id, &tool_result.output)?;
            record_rollback_checkpoints(
                repo_root,
                project_id,
                run_id,
                &tool_call.tool_call_id,
                &rollback_checkpoints,
            )?;
            workspace_guard.record_tool_output(repo_root, &tool_result.output)?;
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::ToolCompleted,
                json!({
                    "toolCallId": tool_call.tool_call_id,
                    "toolName": tool_call.tool_name,
                    "ok": true,
                    "summary": tool_result.summary,
                    "output": output,
                }),
            )?;
            Ok(AgentToolResult {
                tool_call_id: tool_call.tool_call_id,
                tool_name: tool_call.tool_name,
                ok: true,
                summary: tool_result.summary,
                output,
            })
        }
        Err(error) => {
            finish_failed_tool_call(repo_root, project_id, run_id, &tool_call, &error)?;
            Err(error)
        }
    }
}

fn finish_failed_tool_call(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call: &AgentToolCall,
    error: &CommandError,
) -> CommandResult<()> {
    let diagnostic = project_store::AgentRunDiagnosticRecord {
        code: error.code.clone(),
        message: error.message.clone(),
    };
    project_store::finish_agent_tool_call(
        repo_root,
        &AgentToolCallFinishRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            tool_call_id: tool_call.tool_call_id.clone(),
            state: AgentToolCallState::Failed,
            result_json: None,
            error: Some(diagnostic),
            completed_at: now_timestamp(),
        },
    )?;

    if error.class == CommandErrorClass::PolicyDenied {
        record_action_request(
            repo_root,
            project_id,
            run_id,
            &format!("tool-{}", tool_call.tool_call_id),
            "safety_boundary",
            "Action required",
            &error.message,
        )?;
        append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::ActionRequired,
            json!({
                "toolCallId": tool_call.tool_call_id.clone(),
                "toolName": tool_call.tool_name.clone(),
                "code": error.code.clone(),
                "message": error.message.clone(),
            }),
        )?;
    }

    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ToolCompleted,
        json!({
            "toolCallId": tool_call.tool_call_id.clone(),
            "toolName": tool_call.tool_name.clone(),
            "ok": false,
            "code": error.code.clone(),
            "message": error.message.clone(),
        }),
    )?;
    Ok(())
}
