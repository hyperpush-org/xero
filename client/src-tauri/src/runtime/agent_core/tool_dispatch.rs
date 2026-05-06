use super::*;
use crate::runtime::{
    redaction::redact_json_for_persistence, AutonomousSafetyPolicyAction,
    AutonomousSafetyPolicyDecision,
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use xero_agent_core::{
    PermissionProfileSandbox, ProjectTrustState, SandboxApprovalSource, SandboxExecutionContext,
    SandboxPlatform, ToolBatchDispatchReport, ToolBudget, ToolCallInput, ToolDescriptorV2,
    ToolDispatchConfig, ToolDispatchFailure, ToolDispatchOutcome, ToolDispatchSuccess,
    ToolErrorCategory, ToolExecutionContext, ToolExecutionControl, ToolExecutionError,
    ToolGroupExecutionMode, ToolHandler, ToolHandlerOutput, ToolPolicy, ToolPolicyDecision,
    ToolRegistryResult, ToolRegistryV2, ToolRollback, ToolSandbox, ToolSandboxResult,
};

#[derive(Debug, Default)]
pub(crate) struct AgentToolBatchDispatchOptions {
    approved_existing_write_call_ids: BTreeSet<String>,
    operator_approved_call_ids: BTreeSet<String>,
}

impl AgentToolBatchDispatchOptions {
    pub(crate) fn approved_existing_write(mut self, tool_call_id: impl Into<String>) -> Self {
        self.approved_existing_write_call_ids
            .insert(tool_call_id.into());
        self
    }

    pub(crate) fn operator_approved(mut self, tool_call_id: impl Into<String>) -> Self {
        self.operator_approved_call_ids.insert(tool_call_id.into());
        self
    }
}

#[derive(Debug)]
pub(crate) struct AgentToolBatchDispatchResult {
    pub(crate) results: Vec<AgentToolResult>,
    pub(crate) failure: Option<CommandError>,
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
    let mut options = AgentToolBatchDispatchOptions::default();
    if approved_existing_write {
        options = options.approved_existing_write(tool_call.tool_call_id.clone());
    }
    if operator_approved {
        options = options.operator_approved(tool_call.tool_call_id.clone());
    }
    let mut batch = dispatch_tool_batch_with_options(
        tool_registry,
        tool_runtime,
        repo_root,
        project_id,
        run_id,
        0,
        workspace_guard,
        vec![tool_call],
        options,
    )?;
    if let Some(error) = batch.failure {
        return Err(error);
    }
    batch.results.pop().ok_or_else(|| {
        CommandError::system_fault(
            "agent_tool_result_missing",
            "Xero dispatched a tool call but did not receive a tool result.",
        )
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "Tool dispatch is the handoff between provider-turn identity, persistence, and the runtime adapter."
)]
pub(crate) fn dispatch_tool_batch(
    tool_registry: &ToolRegistry,
    tool_runtime: &AutonomousToolRuntime,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    turn_index: usize,
    workspace_guard: &mut AgentWorkspaceGuard,
    tool_calls: Vec<AgentToolCall>,
) -> CommandResult<AgentToolBatchDispatchResult> {
    dispatch_tool_batch_with_options(
        tool_registry,
        tool_runtime,
        repo_root,
        project_id,
        run_id,
        turn_index,
        workspace_guard,
        tool_calls,
        AgentToolBatchDispatchOptions::default(),
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Tool dispatch is the handoff between provider-turn identity, persistence, and the runtime adapter."
)]
fn dispatch_tool_batch_with_options(
    tool_registry: &ToolRegistry,
    tool_runtime: &AutonomousToolRuntime,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    turn_index: usize,
    workspace_guard: &mut AgentWorkspaceGuard,
    tool_calls: Vec<AgentToolCall>,
    options: AgentToolBatchDispatchOptions,
) -> CommandResult<AgentToolBatchDispatchResult> {
    if tool_calls.is_empty() {
        return Ok(AgentToolBatchDispatchResult {
            results: Vec::new(),
            failure: None,
        });
    }

    for tool_call in &tool_calls {
        record_started_tool_call(
            repo_root,
            project_id,
            run_id,
            tool_call,
            options
                .approved_existing_write_call_ids
                .contains(&tool_call.tool_call_id)
                || options
                    .operator_approved_call_ids
                    .contains(&tool_call.tool_call_id),
        )?;
    }

    for tool_call in &tool_calls {
        if tool_registry.descriptor(&tool_call.tool_name).is_some() {
            continue;
        }

        let error = match tool_registry.validate_call(tool_call) {
            Ok(()) => CommandError::user_fixable(
                "agent_tool_call_unknown",
                format!(
                    "The owned-agent model requested unregistered tool `{}`.",
                    tool_call.tool_name
                ),
            ),
            Err(error) => error,
        };
        let _ =
            record_policy_decode_failure_event(repo_root, project_id, run_id, tool_call, &error);
        finish_failed_tool_call_with_dispatch(
            repo_root,
            project_id,
            run_id,
            tool_call,
            &error,
            Some(json!({
                "registryVersion": "tool_registry_v2",
                "preflight": "legacy_registry_descriptor_missing",
            })),
        )?;
        return Ok(AgentToolBatchDispatchResult {
            results: Vec::new(),
            failure: Some(error),
        });
    }

    let shared = Arc::new(AutonomousToolHandlerShared {
        legacy_registry: tool_registry.clone(),
        tool_runtime: tool_runtime.clone(),
        repo_root: repo_root.to_path_buf(),
        project_id: project_id.to_owned(),
        run_id: run_id.to_owned(),
        workspace_guard: Arc::new(Mutex::new(std::mem::take(workspace_guard))),
        write_preflight: Arc::new(Mutex::new(BTreeMap::new())),
        approved_existing_write_call_ids: options.approved_existing_write_call_ids,
        operator_approved_call_ids: options.operator_approved_call_ids,
    });

    let dispatch_result = (|| {
        let registry_v2 = build_tool_registry_v2(tool_registry, Arc::clone(&shared))?;
        let budget = tool_dispatch_budget(tool_runtime);
        let config = ToolDispatchConfig {
            budget: budget.clone(),
            policy: Arc::new(AutonomousPolicyAdapter::new(Arc::clone(&shared))),
            sandbox: Arc::new(ProductionToolSandbox::new(repo_root, tool_runtime)),
            rollback: Some(Arc::new(AgentToolRollback::new(Arc::clone(&shared)))),
            context: ToolExecutionContext {
                project_id: project_id.into(),
                run_id: run_id.into(),
                turn_index,
                context_epoch: format!("turn-{turn_index}"),
                telemetry_attributes: BTreeMap::from([
                    ("xero.dispatch.path".into(), "desktop_provider_loop".into()),
                    ("xero.dispatch.registry".into(), "tool_registry_v2".into()),
                ]),
            },
        };
        let calls = tool_calls
            .iter()
            .map(|tool_call| ToolCallInput {
                tool_call_id: tool_call.tool_call_id.clone(),
                tool_name: tool_call.tool_name.clone(),
                input: tool_call.input.clone(),
            })
            .collect::<Vec<_>>();
        let report = registry_v2.dispatch_batch(&calls, &config);
        persist_tool_batch_report(repo_root, project_id, run_id, report, &budget)
    })();

    restore_workspace_guard(&shared, workspace_guard)?;
    dispatch_result
}

fn build_tool_registry_v2(
    tool_registry: &ToolRegistry,
    shared: Arc<AutonomousToolHandlerShared>,
) -> CommandResult<ToolRegistryV2> {
    let mut registry_v2 = ToolRegistryV2::new();
    for descriptor in tool_registry.descriptors_v2() {
        registry_v2
            .register(AutonomousToolHandler {
                descriptor,
                shared: Arc::clone(&shared),
            })
            .map_err(tool_execution_error_to_command_error)?;
    }
    Ok(registry_v2)
}

fn restore_workspace_guard(
    shared: &AutonomousToolHandlerShared,
    workspace_guard: &mut AgentWorkspaceGuard,
) -> CommandResult<()> {
    let mut guard = shared.workspace_guard.lock().map_err(|_| {
        CommandError::system_fault(
            "agent_workspace_guard_lock_failed",
            "Xero could not restore owned-agent workspace guard state after tool dispatch.",
        )
    })?;
    *workspace_guard = std::mem::take(&mut *guard);
    Ok(())
}

fn record_started_tool_call(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call: &AgentToolCall,
    approved_replay: bool,
) -> CommandResult<()> {
    let started_at = now_timestamp();
    let (persisted_input, input_redacted) = redact_json_for_persistence(&tool_call.input);
    let input_json = serde_json::to_string(&persisted_input).map_err(|error| {
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
            started_at,
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
            "input": persisted_input,
            "inputRedacted": input_redacted,
            "approvedReplay": approved_replay,
            "dispatch": {
                "registryVersion": "tool_registry_v2",
            },
        }),
    )?;
    Ok(())
}

struct AutonomousToolHandlerShared {
    legacy_registry: ToolRegistry,
    tool_runtime: AutonomousToolRuntime,
    repo_root: PathBuf,
    project_id: String,
    run_id: String,
    workspace_guard: Arc<Mutex<AgentWorkspaceGuard>>,
    write_preflight: Arc<Mutex<BTreeMap<String, AgentToolWritePreflight>>>,
    approved_existing_write_call_ids: BTreeSet<String>,
    operator_approved_call_ids: BTreeSet<String>,
}

#[derive(Debug, Clone)]
struct AgentToolWritePreflight {
    request: AutonomousToolRequest,
    write_observations: Vec<AgentWorkspaceWriteObservation>,
    rollback_checkpoints: Vec<AgentRollbackCheckpoint>,
    auto_file_reservations: Vec<project_store::AgentFileReservationRecord>,
}

impl AutonomousToolHandlerShared {
    fn execute_tool_call(
        &self,
        call: &ToolCallInput,
        control: Option<&ToolExecutionControl>,
    ) -> ToolRegistryResult<ToolHandlerOutput> {
        let tool_call = AgentToolCall {
            tool_call_id: call.tool_call_id.clone(),
            tool_name: call.tool_name.clone(),
            input: call.input.clone(),
        };
        let request = self
            .legacy_registry
            .decode_call(&tool_call)
            .map_err(|error| {
                let _ = record_policy_decode_failure_event(
                    &self.repo_root,
                    &self.project_id,
                    &self.run_id,
                    &tool_call,
                    &error,
                );
                command_error_to_tool_execution_error(error)
            })?;
        if let AutonomousToolRequest::HarnessRunner(request) = &request {
            let (summary, output) = harness_runner_tool_output(&self.legacy_registry, request)
                .map_err(command_error_to_tool_execution_error)?;
            let mut handler_output = ToolHandlerOutput::new(summary, output);
            handler_output
                .telemetry_attributes
                .insert("xero.tool.handler".into(), "harness_runner".into());
            return Ok(handler_output);
        }
        let operator_approved = self
            .operator_approved_call_ids
            .contains(&tool_call.tool_call_id);
        let write_preflight = self
            .get_write_preflight(&tool_call.tool_call_id)
            .map_err(command_error_to_tool_execution_error)?;
        if let Some(preflight) = write_preflight.as_ref() {
            if std::mem::discriminant(&preflight.request) != std::mem::discriminant(&request) {
                return Err(ToolExecutionError::retryable(
                    "agent_tool_write_preflight_mismatch",
                    "Xero prepared write preflight metadata for a different tool action.",
                ));
            }
        }
        let tool_runtime = control
            .cloned()
            .map(|control| {
                self.tool_runtime
                    .clone()
                    .with_tool_execution_cancellation(Arc::new(move || control.is_cancelled()))
            })
            .unwrap_or_else(|| self.tool_runtime.clone());

        let tool_execution = match request {
            AutonomousToolRequest::Command(command_request) => {
                let mut emit_chunk = |chunk: &AutonomousCommandOutputChunk| {
                    let _ = record_command_output_chunk_event(
                        &self.repo_root,
                        &self.project_id,
                        &self.run_id,
                        &tool_call.tool_call_id,
                        &tool_call.tool_name,
                        chunk,
                    );
                };
                if operator_approved {
                    tool_runtime.command_with_operator_approval_and_output_callback(
                        command_request,
                        &mut emit_chunk,
                    )
                } else {
                    tool_runtime.command_with_output_callback(command_request, &mut emit_chunk)
                }
            }
            request if operator_approved => tool_runtime.execute_approved(request),
            request => tool_runtime.execute(request),
        };

        let tool_result = match tool_execution {
            Ok(tool_result) => tool_result,
            Err(error) => {
                if write_preflight.is_none() {
                    let _ = self.release_write_preflight(&tool_call.tool_call_id, "tool_failed");
                }
                return Err(command_error_to_tool_execution_error(error));
            }
        };

        let write_observations = write_preflight
            .as_ref()
            .map(|preflight| preflight.write_observations.as_slice())
            .unwrap_or(&[]);
        record_file_change_event(
            &self.repo_root,
            &self.project_id,
            &self.run_id,
            write_observations,
            &tool_result.output,
        )
        .map_err(command_error_to_tool_execution_error)?;
        record_command_output_event(
            &self.repo_root,
            &self.project_id,
            &self.run_id,
            &tool_call.tool_call_id,
            &tool_call.tool_name,
            &tool_result.output,
        )
        .map_err(command_error_to_tool_execution_error)?;
        record_rollback_checkpoints(
            &self.repo_root,
            &self.project_id,
            &self.run_id,
            &tool_call.tool_call_id,
            write_preflight
                .as_ref()
                .map(|preflight| preflight.rollback_checkpoints.as_slice())
                .unwrap_or(&[]),
        )
        .map_err(command_error_to_tool_execution_error)?;
        self.release_write_preflight(&tool_call.tool_call_id, "tool_completed")
            .map_err(command_error_to_tool_execution_error)?;
        {
            let mut guard = self.workspace_guard.lock().map_err(|_| {
                ToolExecutionError::retryable(
                    "agent_workspace_guard_lock_failed",
                    "Xero could not lock owned-agent workspace observation state.",
                )
            })?;
            guard
                .record_tool_output(&self.repo_root, &tool_result.output)
                .map_err(command_error_to_tool_execution_error)?;
        }

        let summary = tool_result.summary.clone();
        let output = serde_json::to_value(&tool_result).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_tool_result_serialize_failed",
                format!("Xero could not serialize owned-agent tool output: {error}"),
            )
        })?;
        let mut handler_output = ToolHandlerOutput::new(summary, output);
        handler_output
            .telemetry_attributes
            .insert("xero.tool.handler".into(), "autonomous_tool_runtime".into());
        Ok(handler_output)
    }

    fn prepare_write_preflight(
        &self,
        call: &ToolCallInput,
        request: AutonomousToolRequest,
    ) -> CommandResult<Option<JsonValue>> {
        let planned = planned_file_reservation_operations(&request)?;
        if planned.is_empty() {
            return Ok(None);
        }
        let approved_existing_write = self
            .approved_existing_write_call_ids
            .contains(&call.tool_call_id);
        let write_observations = {
            let guard = self.workspace_guard.lock().map_err(|_| {
                CommandError::system_fault(
                    "agent_workspace_guard_lock_failed",
                    "Xero could not lock owned-agent workspace observation state.",
                )
            })?;
            guard.validate_write_intent(&self.repo_root, &request, approved_existing_write)?
        };
        let rollback_checkpoints =
            rollback_checkpoints_for_request(&self.repo_root, &request, &write_observations)?;
        let auto_file_reservations = claim_file_reservations_for_request(
            &self.repo_root,
            &self.project_id,
            &self.run_id,
            &request,
        )?;
        let reservation_ids = auto_file_reservations
            .iter()
            .map(|reservation| reservation.reservation_id.clone())
            .collect::<Vec<_>>();
        let checkpoint_count = rollback_checkpoints.len();
        let reservation_count = auto_file_reservations.len();
        self.write_preflight
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "agent_write_preflight_lock_failed",
                    "Xero could not lock owned-agent write preflight state.",
                )
            })?
            .insert(
                call.tool_call_id.clone(),
                AgentToolWritePreflight {
                    request,
                    write_observations,
                    rollback_checkpoints,
                    auto_file_reservations,
                },
            );
        Ok(Some(json!({
            "kind": "agent_tool_write_preflight",
            "toolCallId": call.tool_call_id,
            "toolName": call.tool_name,
            "projectId": self.project_id,
            "runId": self.run_id,
            "checkpointCount": checkpoint_count,
            "reservationCount": reservation_count,
            "reservationIds": reservation_ids,
            "checkpointedAt": now_timestamp(),
        })))
    }

    fn get_write_preflight(
        &self,
        tool_call_id: &str,
    ) -> CommandResult<Option<AgentToolWritePreflight>> {
        self.write_preflight
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "agent_write_preflight_lock_failed",
                    "Xero could not lock owned-agent write preflight state.",
                )
            })
            .map(|preflight| preflight.get(tool_call_id).cloned())
    }

    fn take_write_preflight(
        &self,
        tool_call_id: &str,
    ) -> CommandResult<Option<AgentToolWritePreflight>> {
        self.write_preflight
            .lock()
            .map_err(|_| {
                CommandError::system_fault(
                    "agent_write_preflight_lock_failed",
                    "Xero could not lock owned-agent write preflight state.",
                )
            })
            .map(|mut preflight| preflight.remove(tool_call_id))
    }

    fn release_write_preflight(&self, tool_call_id: &str, reason: &str) -> CommandResult<()> {
        let preflight = self.get_write_preflight(tool_call_id)?;
        if let Some(preflight) = preflight {
            release_auto_file_reservations(
                &self.repo_root,
                &self.project_id,
                &self.run_id,
                &preflight.auto_file_reservations,
                reason,
            )?;
            let _ = self.take_write_preflight(tool_call_id)?;
        }
        Ok(())
    }
}

struct AutonomousToolHandler {
    descriptor: ToolDescriptorV2,
    shared: Arc<AutonomousToolHandlerShared>,
}

impl ToolHandler for AutonomousToolHandler {
    fn descriptor(&self) -> ToolDescriptorV2 {
        self.descriptor.clone()
    }

    fn execute(
        &self,
        _context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> ToolRegistryResult<ToolHandlerOutput> {
        self.shared.execute_tool_call(call, None)
    }

    fn execute_with_control(
        &self,
        _context: &ToolExecutionContext,
        call: &ToolCallInput,
        control: &ToolExecutionControl,
    ) -> ToolRegistryResult<ToolHandlerOutput> {
        control.ensure_not_cancelled(&call.tool_name)?;
        match self.shared.execute_tool_call(call, Some(control)) {
            Ok(output) => {
                control.ensure_not_cancelled(&call.tool_name)?;
                Ok(output)
            }
            Err(error) if control.is_cancelled() => {
                control.ensure_not_cancelled(&call.tool_name)?;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }
}

struct AutonomousPolicyAdapter {
    shared: Arc<AutonomousToolHandlerShared>,
}

impl AutonomousPolicyAdapter {
    fn new(shared: Arc<AutonomousToolHandlerShared>) -> Self {
        Self { shared }
    }

    fn evaluate_call(
        &self,
        descriptor: &ToolDescriptorV2,
        call: &ToolCallInput,
    ) -> CommandResult<AutonomousSafetyPolicyDecision> {
        let tool_call = AgentToolCall {
            tool_call_id: call.tool_call_id.clone(),
            tool_name: call.tool_name.clone(),
            input: call.input.clone(),
        };
        let request = self.shared.legacy_registry.decode_call(&tool_call)?;
        let input_sha256 = sha256_json(&tool_call.input)?;
        let operator_approved = self
            .shared
            .operator_approved_call_ids
            .contains(&tool_call.tool_call_id);
        let decision = self.shared.tool_runtime.evaluate_safety_policy(
            &descriptor.name,
            &tool_call.input,
            &request,
            operator_approved,
            &input_sha256,
        )?;
        record_policy_decision_event(
            &self.shared.repo_root,
            &self.shared.project_id,
            &self.shared.run_id,
            &tool_call,
            &decision,
        )?;
        Ok(decision)
    }
}

impl ToolPolicy for AutonomousPolicyAdapter {
    fn evaluate(&self, descriptor: &ToolDescriptorV2, call: &ToolCallInput) -> ToolPolicyDecision {
        match self.evaluate_call(descriptor, call) {
            Ok(decision) => match decision.action {
                AutonomousSafetyPolicyAction::Allow => ToolPolicyDecision::Allow,
                AutonomousSafetyPolicyAction::RequireApproval
                    if policy_approval_is_reported_by_handler(call) =>
                {
                    ToolPolicyDecision::Allow
                }
                AutonomousSafetyPolicyAction::RequireApproval => {
                    ToolPolicyDecision::RequireApproval {
                        action_id: format!("approve-tool-{}", call.tool_call_id),
                        message: decision.explanation,
                    }
                }
                AutonomousSafetyPolicyAction::Deny => ToolPolicyDecision::Deny {
                    code: decision.code,
                    message: decision.explanation,
                },
            },
            Err(error) => {
                let tool_call = AgentToolCall {
                    tool_call_id: call.tool_call_id.clone(),
                    tool_name: call.tool_name.clone(),
                    input: call.input.clone(),
                };
                let _ = record_policy_decode_failure_event(
                    &self.shared.repo_root,
                    &self.shared.project_id,
                    &self.shared.run_id,
                    &tool_call,
                    &error,
                );
                ToolPolicyDecision::Deny {
                    code: error.code,
                    message: error.message,
                }
            }
        }
    }
}

#[derive(Debug)]
struct ProductionToolSandbox {
    inner: PermissionProfileSandbox,
}

impl ProductionToolSandbox {
    fn new(repo_root: &Path, tool_runtime: &AutonomousToolRuntime) -> Self {
        let mut app_data_roots = tool_runtime
            .environment_profile_database_path
            .as_ref()
            .and_then(|path| path.parent())
            .map(|path| vec![path.to_string_lossy().into_owned()])
            .unwrap_or_default();
        app_data_roots.push(
            crate::db::project_app_data_dir_for_repo(repo_root)
                .to_string_lossy()
                .into_owned(),
        );
        app_data_roots.sort();
        app_data_roots.dedup();
        Self {
            inner: PermissionProfileSandbox::new(SandboxExecutionContext {
                workspace_root: repo_root.to_string_lossy().into_owned(),
                app_data_roots,
                project_trust: ProjectTrustState::Trusted,
                approval_source: SandboxApprovalSource::Policy,
                platform: SandboxPlatform::current(),
                preserved_environment_keys: vec!["PATH".into()],
                ..SandboxExecutionContext::default()
            }),
        }
    }
}

impl ToolSandbox for ProductionToolSandbox {
    fn evaluate(
        &self,
        descriptor: &ToolDescriptorV2,
        call: &ToolCallInput,
        context: &ToolExecutionContext,
    ) -> ToolSandboxResult {
        self.inner.evaluate(descriptor, call, context)
    }
}

struct AgentToolRollback {
    shared: Arc<AutonomousToolHandlerShared>,
}

impl AgentToolRollback {
    fn new(shared: Arc<AutonomousToolHandlerShared>) -> Self {
        Self { shared }
    }
}

impl ToolRollback for AgentToolRollback {
    fn checkpoint_before(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
    ) -> ToolRegistryResult<Option<JsonValue>> {
        if descriptor.mutability.is_read_only() {
            return Ok(None);
        }
        let tool_call = AgentToolCall {
            tool_call_id: call.tool_call_id.clone(),
            tool_name: call.tool_name.clone(),
            input: call.input.clone(),
        };
        let request = self
            .shared
            .legacy_registry
            .decode_call(&tool_call)
            .map_err(command_error_to_tool_execution_error)?;
        self.shared
            .prepare_write_preflight(call, request)
            .map_err(command_error_to_tool_execution_error)
    }

    fn rollback_after_failure(
        &self,
        call: &ToolCallInput,
        _descriptor: &ToolDescriptorV2,
        checkpoint: &JsonValue,
        error: &ToolExecutionError,
    ) -> ToolRegistryResult<JsonValue> {
        let preflight = self
            .shared
            .take_write_preflight(&call.tool_call_id)
            .map_err(command_error_to_tool_execution_error)?;
        let Some(preflight) = preflight else {
            return Ok(json!({
                "kind": "agent_tool_dispatch_failure_checkpoint",
                "toolCallId": call.tool_call_id,
                "toolName": call.tool_name,
                "checkpoint": checkpoint,
                "failure": tool_execution_error_json(error),
                "workspaceRollback": "not_applicable",
            }));
        };
        let checkpoint_record = record_rollback_checkpoints(
            &self.shared.repo_root,
            &self.shared.project_id,
            &self.shared.run_id,
            &call.tool_call_id,
            &preflight.rollback_checkpoints,
        )
        .map(|()| json!({ "recorded": true }))
        .unwrap_or_else(|error| {
            json!({
                "recorded": false,
                "code": error.code,
                "message": error.message,
            })
        });
        let rollback_outcome =
            restore_rollback_checkpoints(&self.shared.repo_root, &preflight.rollback_checkpoints);
        let release_outcome = release_auto_file_reservations(
            &self.shared.repo_root,
            &self.shared.project_id,
            &self.shared.run_id,
            &preflight.auto_file_reservations,
            "tool_failed_rollback",
        );
        let rollback_outcome = rollback_outcome.map_err(command_error_to_tool_execution_error)?;
        release_outcome.map_err(command_error_to_tool_execution_error)?;
        Ok(json!({
            "kind": "agent_tool_dispatch_failure_checkpoint",
            "toolCallId": call.tool_call_id,
            "toolName": call.tool_name,
            "checkpoint": checkpoint,
            "failure": tool_execution_error_json(error),
            "checkpointRecord": checkpoint_record,
            "workspaceRollback": rollback_outcome,
        }))
    }
}

fn policy_approval_is_reported_by_handler(call: &ToolCallInput) -> bool {
    matches!(
        call.tool_name.as_str(),
        AUTONOMOUS_TOOL_COMMAND
            | AUTONOMOUS_TOOL_COMMAND_PROBE
            | AUTONOMOUS_TOOL_COMMAND_VERIFY
            | AUTONOMOUS_TOOL_COMMAND_RUN
            | AUTONOMOUS_TOOL_COMMAND_SESSION
            | AUTONOMOUS_TOOL_COMMAND_SESSION_START
            | AUTONOMOUS_TOOL_PROCESS_MANAGER
            | AUTONOMOUS_TOOL_POWERSHELL
    )
}

fn tool_dispatch_budget(_tool_runtime: &AutonomousToolRuntime) -> ToolBudget {
    ToolBudget {
        max_command_output_bytes: 2 * 1024 * 1024,
        ..ToolBudget::default()
    }
}

fn persist_tool_batch_report(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    report: ToolBatchDispatchReport,
    budget: &ToolBudget,
) -> CommandResult<AgentToolBatchDispatchResult> {
    let mut results = Vec::new();
    let mut failure = None;
    for group in report.groups {
        let group_mode = group.mode.clone();
        let group_elapsed_ms = group.elapsed_ms;
        let timeout_error = group.timeout_error.clone();
        for outcome in group.outcomes {
            match outcome {
                ToolDispatchOutcome::Succeeded(success) => {
                    results.push(persist_tool_dispatch_success(
                        repo_root,
                        project_id,
                        run_id,
                        success,
                        group_mode.clone(),
                        group_elapsed_ms,
                        timeout_error.as_ref(),
                        budget,
                    )?);
                }
                ToolDispatchOutcome::Failed(failed) => {
                    let error = persist_tool_dispatch_failure(
                        repo_root,
                        project_id,
                        run_id,
                        failed,
                        group_mode.clone(),
                        group_elapsed_ms,
                        timeout_error.as_ref(),
                        budget,
                    )?;
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
            }
        }
    }
    Ok(AgentToolBatchDispatchResult { results, failure })
}

#[expect(
    clippy::too_many_arguments,
    reason = "The persisted event includes both per-call and per-group V2 dispatch metadata."
)]
fn persist_tool_dispatch_success(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    success: ToolDispatchSuccess,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
    budget: &ToolBudget,
) -> CommandResult<AgentToolResult> {
    let result_json = serde_json::to_string(&success.output).map_err(|error| {
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
            tool_call_id: success.tool_call_id.clone(),
            state: AgentToolCallState::Succeeded,
            result_json: Some(result_json),
            error: None,
            completed_at: now_timestamp(),
        },
    )?;

    let dispatch = dispatch_success_metadata_json(
        &success,
        group_mode,
        group_elapsed_ms,
        timeout_error,
        budget,
    );
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ToolCompleted,
        json!({
            "toolCallId": success.tool_call_id,
            "toolName": success.tool_name,
            "ok": true,
            "summary": success.summary,
            "output": success.output,
            "dispatch": dispatch,
        }),
    )?;

    Ok(AgentToolResult {
        tool_call_id: success.tool_call_id,
        tool_name: success.tool_name,
        ok: true,
        summary: success.summary,
        output: success.output,
        parent_assistant_message_id: None,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "The persisted event includes both per-call and per-group V2 dispatch metadata."
)]
fn persist_tool_dispatch_failure(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    failure: ToolDispatchFailure,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
    budget: &ToolBudget,
) -> CommandResult<CommandError> {
    let command_error = tool_execution_error_ref_to_command_error(&failure.error);
    let dispatch = dispatch_failure_metadata_json(
        &failure,
        group_mode,
        group_elapsed_ms,
        timeout_error,
        budget,
    );
    finish_failed_tool_call_with_dispatch(
        repo_root,
        project_id,
        run_id,
        &AgentToolCall {
            tool_call_id: failure.tool_call_id.clone(),
            tool_name: failure.tool_name.clone(),
            input: json!({}),
        },
        &command_error,
        Some(dispatch),
    )?;
    Ok(command_error)
}

fn dispatch_success_metadata_json(
    success: &ToolDispatchSuccess,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
    budget: &ToolBudget,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "groupMode": group_mode,
        "groupElapsedMs": group_elapsed_ms,
        "elapsedMs": success.elapsed_ms,
        "truncation": success.truncation,
        "sandbox": success.sandbox_metadata,
        "budget": budget_metadata_json(budget),
        "telemetry": success.telemetry_attributes,
        "preHook": success.pre_hook_payload,
        "postHook": success.post_hook_payload,
        "timeout": timeout_error.map(tool_execution_error_json),
    })
}

fn dispatch_failure_metadata_json(
    failure: &ToolDispatchFailure,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
    budget: &ToolBudget,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "groupMode": group_mode,
        "groupElapsedMs": group_elapsed_ms,
        "elapsedMs": failure.elapsed_ms,
        "typedErrorCategory": failure.error.category,
        "modelMessage": failure.error.model_message,
        "retryable": failure.error.retryable,
        "doomLoopSignal": failure.doom_loop_signal,
        "rollbackPayload": failure.rollback_payload,
        "rollbackError": failure.rollback_error.as_ref().map(tool_execution_error_json),
        "sandbox": failure.sandbox_metadata,
        "budget": budget_metadata_json(budget),
        "preHook": failure.pre_hook_payload,
        "postHook": failure.post_hook_payload,
        "timeout": timeout_error.map(tool_execution_error_json),
    })
}

fn budget_metadata_json(budget: &ToolBudget) -> JsonValue {
    json!({
        "maxToolCallsPerTurn": budget.max_tool_calls_per_turn,
        "maxToolFailuresPerTurn": budget.max_tool_failures_per_turn,
        "maxRepeatedEquivalentCalls": budget.max_repeated_equivalent_calls,
        "maxCommandOutputBytes": budget.max_command_output_bytes,
        "maxWallClockTimePerToolGroupMs": budget.max_wall_clock_time_per_tool_group_ms,
    })
}

fn tool_execution_error_json(error: &ToolExecutionError) -> JsonValue {
    json!({
        "category": &error.category,
        "code": &error.code,
        "message": &error.message,
        "modelMessage": &error.model_message,
        "retryable": error.retryable,
        "telemetry": &error.telemetry_attributes,
    })
}

fn command_error_to_tool_execution_error(error: CommandError) -> ToolExecutionError {
    let (category, model_message) = match error.class {
        CommandErrorClass::PolicyDenied => (
            ToolErrorCategory::PolicyDenied,
            "Xero denied the tool call under the active safety policy.",
        ),
        CommandErrorClass::Retryable => (
            ToolErrorCategory::RetryableProviderToolFailure,
            "The tool failed in a retryable way. Change the input or gather new context before retrying.",
        ),
        CommandErrorClass::SystemFault => (
            ToolErrorCategory::RetryableProviderToolFailure,
            "The tool failed because Xero hit an internal runtime fault.",
        ),
        CommandErrorClass::UserFixable => (
            ToolErrorCategory::InvalidInput,
            "The tool input was invalid or unavailable in the active runtime.",
        ),
    };
    ToolExecutionError::new(
        category,
        error.code,
        error.message,
        error.retryable,
        model_message,
    )
}

fn tool_execution_error_to_command_error(error: ToolExecutionError) -> CommandError {
    tool_execution_error_ref_to_command_error(&error)
}

fn tool_execution_error_ref_to_command_error(error: &ToolExecutionError) -> CommandError {
    let class = match error.category {
        ToolErrorCategory::PolicyDenied
        | ToolErrorCategory::ApprovalRequired
        | ToolErrorCategory::SandboxDenied => CommandErrorClass::PolicyDenied,
        ToolErrorCategory::Timeout | ToolErrorCategory::RetryableProviderToolFailure => {
            CommandErrorClass::Retryable
        }
        ToolErrorCategory::InvalidInput
        | ToolErrorCategory::ExternalDependencyMissing
        | ToolErrorCategory::ToolUnavailable
        | ToolErrorCategory::BudgetExceeded
        | ToolErrorCategory::DoomLoopDetected => CommandErrorClass::UserFixable,
    };
    CommandError::new(
        error.code.clone(),
        class,
        error.message.clone(),
        error.retryable,
    )
}

fn claim_file_reservations_for_request(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    request: &AutonomousToolRequest,
) -> CommandResult<Vec<project_store::AgentFileReservationRecord>> {
    let planned = planned_file_reservation_operations(request)?;
    if planned.is_empty() {
        return Ok(Vec::new());
    }
    let paths = planned
        .iter()
        .map(|(path, _)| path.clone())
        .collect::<Vec<_>>();
    let now = now_timestamp();
    if project_store::has_active_agent_file_reservation_for_paths(
        repo_root, project_id, run_id, &paths, &now,
    )? {
        return Ok(Vec::new());
    }
    let operation = planned
        .iter()
        .map(|(_, operation)| *operation)
        .find(|operation| {
            *operation == project_store::AgentCoordinationReservationOperation::Writing
        })
        .unwrap_or(project_store::AgentCoordinationReservationOperation::Editing);
    let claim = project_store::claim_agent_file_reservations(
        repo_root,
        &project_store::ClaimAgentFileReservationRequest {
            project_id: project_id.into(),
            owner_run_id: run_id.into(),
            paths,
            operation,
            note: Some("Owned-agent write intent.".into()),
            override_reason: None,
            claimed_at: now,
            lease_seconds: None,
        },
    )?;
    if claim.conflicts.is_empty() {
        return Ok(claim.claimed);
    }
    let conflict_summary = claim
        .conflicts
        .iter()
        .take(5)
        .map(|conflict| {
            format!(
                "`{}` overlaps reservation `{}` by run `{}`",
                conflict.requested_path,
                conflict.reservation.path,
                conflict
                    .reservation
                    .owner_child_run_id
                    .as_deref()
                    .unwrap_or(conflict.reservation.owner_run_id.as_str())
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    Err(CommandError::new(
        "agent_file_reservation_conflict",
        CommandErrorClass::PolicyDenied,
        format!(
            "Xero found active file reservation conflict(s) before this write: {conflict_summary}. Check or claim with the `agent_coordination` tool and provide an overrideReason if you intentionally need overlapping work."
        ),
        false,
    ))
}

fn release_auto_file_reservations(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    reservations: &[project_store::AgentFileReservationRecord],
    reason: &str,
) -> CommandResult<()> {
    if reservations.is_empty() {
        return Ok(());
    }
    let released_at = now_timestamp();
    for reservation in reservations {
        project_store::release_agent_file_reservations(
            repo_root,
            &project_store::ReleaseAgentFileReservationRequest {
                project_id: project_id.into(),
                owner_run_id: run_id.into(),
                reservation_id: Some(reservation.reservation_id.clone()),
                paths: Vec::new(),
                release_reason: reason.into(),
                released_at: released_at.clone(),
            },
        )?;
    }
    Ok(())
}

fn record_policy_decode_failure_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call: &AgentToolCall,
    error: &CommandError,
) -> CommandResult<()> {
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::PolicyDecision,
        json!({
            "toolCallId": tool_call.tool_call_id,
            "toolName": tool_call.tool_name,
            "action": "deny",
            "code": error.code,
            "explanation": error.message,
            "riskClass": "invalid_tool_call",
            "approvalMode": null,
            "projectTrust": "imported_project",
            "networkIntent": "unknown",
            "credentialSensitivity": "unknown",
            "osTarget": null,
            "priorObservationRequired": false,
            "approvalGrant": null,
        }),
    )?;
    Ok(())
}

fn sha256_json(value: &JsonValue) -> CommandResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        CommandError::system_fault(
            "agent_tool_input_hash_failed",
            format!("Xero could not hash owned-agent tool input: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn record_policy_decision_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call: &AgentToolCall,
    decision: &AutonomousSafetyPolicyDecision,
) -> CommandResult<()> {
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::PolicyDecision,
        json!({
            "toolCallId": tool_call.tool_call_id,
            "toolName": tool_call.tool_name,
            "action": decision.action.as_str(),
            "code": decision.code,
            "explanation": decision.explanation,
            "riskClass": decision.risk_class,
            "approvalMode": decision.approval_mode,
            "projectTrust": decision.project_trust,
            "networkIntent": decision.network_intent,
            "credentialSensitivity": decision.credential_sensitivity,
            "osTarget": decision.os_target,
            "priorObservationRequired": decision.prior_observation_required,
            "approvalGrant": decision.approval_grant,
        }),
    )?;
    Ok(())
}

fn finish_failed_tool_call_with_dispatch(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call: &AgentToolCall,
    error: &CommandError,
    dispatch: Option<JsonValue>,
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

    let mut payload = json!({
        "toolCallId": tool_call.tool_call_id.clone(),
        "toolName": tool_call.tool_name.clone(),
        "ok": false,
        "code": error.code.clone(),
        "message": error.message.clone(),
    });
    if let (Some(object), Some(dispatch)) = (payload.as_object_mut(), dispatch) {
        object.insert("dispatch".into(), dispatch);
    }
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ToolCompleted,
        payload,
    )?;
    Ok(())
}
