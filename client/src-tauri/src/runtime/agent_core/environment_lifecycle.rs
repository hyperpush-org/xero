use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::commands::{workspace_index::workspace_status_at_root, WorkspaceIndexStateDto};
use serde_json::{json, Value as JsonValue};
use xero_agent_core::{
    AgentCoreStore, ContextManifest as CoreContextManifest, CoreError, CoreResult,
    EnvironmentDiagnostic, EnvironmentGitHookSetup, EnvironmentLifecycleConfig,
    EnvironmentLifecycleExecutor, EnvironmentLifecycleService, EnvironmentLifecycleSnapshot,
    EnvironmentSemanticIndexState, EnvironmentSetupScript, EnvironmentSetupStepResult,
    MessageRole as CoreMessageRole, NewContextManifest, NewMessageRecord as CoreNewMessageRecord,
    NewRunRecord, NewRuntimeEvent, PermissionProfileSandbox, ProjectTrustState,
    RunSnapshot as CoreRunSnapshot, RunStatus as CoreRunStatus, RuntimeEvent as CoreRuntimeEvent,
    RuntimeEventKind as CoreRuntimeEventKind, RuntimeMessage as CoreRuntimeMessage,
    RuntimeTrace as CoreRuntimeTrace, RuntimeTraceContext, SandboxApprovalSource,
    SandboxExecutionContext, SandboxGroupingPolicy, SandboxPlatform, SandboxedProcessRequest,
    SandboxedProcessRunner, ToolApprovalRequirement, ToolCallInput, ToolDescriptorV2,
    ToolEffectClass, ToolExecutionContext, ToolMutability, ToolSandbox, ToolSandboxRequirement,
};

use super::*;

#[derive(Debug, Clone)]
pub(crate) struct DesktopAgentCoreStore {
    repo_root: PathBuf,
}

impl DesktopAgentCoreStore {
    pub(crate) fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct DesktopEnvironmentLifecycleExecutor {
    repo_root: PathBuf,
}

impl DesktopEnvironmentLifecycleExecutor {
    pub(crate) fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }
}

impl EnvironmentLifecycleExecutor for DesktopEnvironmentLifecycleExecutor {
    fn run_setup_script(
        &self,
        script: &EnvironmentSetupScript,
        config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        if script.command.is_empty() {
            return Err(EnvironmentDiagnostic::new(
                "agent_environment_setup_script_empty",
                format!(
                    "Setup script `{}` does not include a command to run.",
                    script.script_id
                ),
            )
            .with_next_action("Remove the setup script or add a trusted command."));
        }
        if !script.config_trust.is_trusted() {
            return Err(EnvironmentDiagnostic::new(
                "agent_environment_setup_script_untrusted",
                format!(
                    "Setup script `{}` is not from a trusted project or app configuration.",
                    script.script_id
                ),
            )
            .with_next_action("Move setup scripts into trusted project or app configuration."));
        }
        if !script.approval.is_satisfied() {
            return Err(EnvironmentDiagnostic::new(
                "agent_environment_setup_script_approval_required",
                format!(
                    "Setup script `{}` requires approval before sandboxed execution.",
                    script.script_id
                ),
            )
            .with_next_action("Approve the setup script, then rerun environment startup."));
        }

        let cwd = script
            .cwd
            .as_ref()
            .map(|cwd| self.repo_root.join(cwd))
            .unwrap_or_else(|| self.repo_root.clone());
        let descriptor = ToolDescriptorV2 {
            name: "environment_setup_script".into(),
            description: "Run an approved trusted setup script before an owned-agent run.".into(),
            input_schema: json!({ "type": "object" }),
            capability_tags: vec!["setup".into(), "subprocess".into()],
            effect_class: ToolEffectClass::CommandExecution,
            mutability: ToolMutability::Mutating,
            sandbox_requirement: ToolSandboxRequirement::FullLocal,
            approval_requirement: ToolApprovalRequirement::Always,
            telemetry_attributes: Default::default(),
            result_truncation: Default::default(),
        };
        let sandbox = PermissionProfileSandbox::new(SandboxExecutionContext {
            workspace_root: config.workspace_root.clone(),
            project_trust: ProjectTrustState::Trusted,
            approval_source: if script.approval.required {
                SandboxApprovalSource::Operator
            } else {
                SandboxApprovalSource::Policy
            },
            platform: SandboxPlatform::current(),
            preserved_environment_keys: vec![
                "PATH".into(),
                "HOME".into(),
                "USER".into(),
                "LOGNAME".into(),
                "SHELL".into(),
                "TMPDIR".into(),
                "TMP".into(),
                "TEMP".into(),
            ],
            ..SandboxExecutionContext::default()
        });
        let call = ToolCallInput {
            tool_call_id: format!("setup-script-{}", script.script_id),
            tool_name: descriptor.name.clone(),
            input: json!({
                "argv": &script.command,
                "cwd": cwd.to_string_lossy(),
                "scriptId": script.script_id,
            }),
        };
        let metadata = sandbox
            .evaluate(&descriptor, &call, &ToolExecutionContext::default())
            .map_err(|denied| {
                EnvironmentDiagnostic::new(denied.error.code, denied.error.message)
                    .with_next_action("Adjust the setup script or sandbox approval profile.")
            })?;
        let output = SandboxedProcessRunner::new()
            .run(
                SandboxedProcessRequest {
                    argv: script.command.clone(),
                    cwd: Some(cwd.to_string_lossy().into_owned()),
                    timeout_ms: Some(120_000),
                    stdout_limit_bytes: 16 * 1024,
                    stderr_limit_bytes: 16 * 1024,
                    metadata,
                },
                || false,
            )
            .map_err(|error| {
                EnvironmentDiagnostic::new(error.code, error.message)
                    .with_next_action("Review the setup script sandbox diagnostics.")
            })?;
        if output.exit_code != Some(0) {
            return Err(EnvironmentDiagnostic::new(
                "agent_environment_setup_script_failed",
                format!(
                    "Setup script `{}` exited with code {:?}.",
                    script.script_id, output.exit_code
                ),
            )
            .with_next_action("Fix the setup script before starting the owned-agent run."));
        }

        Ok(EnvironmentSetupStepResult {
            summary: format!(
                "Setup script `{}` completed under {:?} sandbox.",
                script.label, output.metadata.profile
            ),
        })
    }

    fn setup_git_hook(
        &self,
        hook: &EnvironmentGitHookSetup,
        _config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        let hook_path = self.repo_root.join(&hook.script_path);
        if hook_path.exists() {
            return Ok(EnvironmentSetupStepResult {
                summary: format!("Git hook `{}` already exists.", hook.hook_name),
            });
        }
        Err(EnvironmentDiagnostic::new(
            "agent_environment_git_hook_setup_requires_approval",
            format!(
                "Git hook `{}` cannot be installed automatically without an approved sandboxed setup step.",
                hook.hook_name
            ),
        )
        .with_next_action("Approve the hook setup request, then rerun environment startup."))
    }

    fn setup_skills_plugins(
        &self,
        config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        if config.tool_packs.is_empty() {
            return Err(EnvironmentDiagnostic::new(
                "agent_environment_tool_packs_empty",
                "No tool packs were available for the selected agent run.",
            )
            .with_next_action("Check the agent definition and runtime tool policy."));
        }
        Ok(EnvironmentSetupStepResult {
            summary: format!(
                "{} tool pack descriptor(s) checked.",
                config.tool_packs.len()
            ),
        })
    }

    fn index_workspace(
        &self,
        config: &EnvironmentLifecycleConfig,
    ) -> Result<EnvironmentSetupStepResult, EnvironmentDiagnostic> {
        let status = workspace_index_state(&self.repo_root, &config.project_id);
        if config.semantic_index_required && !status.is_ready() {
            return Err(workspace_index_lifecycle_diagnostic(status));
        }
        Ok(EnvironmentSetupStepResult {
            summary: format!("Workspace index freshness checked: {}.", status.as_str()),
        })
    }
}

pub(crate) fn start_owned_agent_environment(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    provider_config: &AgentProviderConfig,
    tool_runtime: &AutonomousToolRuntime,
) -> CommandResult<EnvironmentLifecycleSnapshot> {
    let store = DesktopAgentCoreStore::new(repo_root);
    let executor = Arc::new(DesktopEnvironmentLifecycleExecutor::new(repo_root));
    let lifecycle = EnvironmentLifecycleService::with_executor(store, executor);
    let config = environment_config_for_owned_agent_run(
        repo_root,
        project_id,
        run_id,
        provider_config,
        tool_runtime,
    );
    lifecycle
        .start_environment(config)
        .map_err(command_error_from_core)
}

pub(crate) fn queue_environment_user_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    prompt: &str,
) -> CommandResult<AgentRunSnapshotRecord> {
    validate_prompt(prompt)?;
    let lifecycle_snapshot =
        project_store::load_agent_environment_lifecycle_snapshot(repo_root, project_id, run_id)?;
    let submitted_at = now_timestamp();
    project_store::insert_agent_environment_pending_message(
        repo_root,
        project_id,
        run_id,
        AgentMessageRole::User,
        prompt,
        &submitted_at,
    )?;
    let pending_message_count =
        project_store::count_undelivered_agent_environment_pending_messages(
            repo_root, project_id, run_id,
        )?;
    append_environment_lifecycle_update(
        repo_root,
        project_id,
        run_id,
        lifecycle_bookkeeping_payload(
            project_id,
            run_id,
            lifecycle_snapshot.as_ref(),
            None,
            pending_message_count,
            "Queued user message while environment starts.",
        )?,
    )?;
    project_store::load_agent_run(repo_root, project_id, run_id)
}

pub(crate) fn deliver_environment_pending_messages(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<()> {
    let pending = project_store::list_undelivered_agent_environment_pending_messages(
        repo_root, project_id, run_id,
    )?;
    if pending.is_empty() {
        return Ok(());
    }

    for message in &pending {
        append_message(
            repo_root,
            project_id,
            run_id,
            message.role.clone(),
            message.content.clone(),
        )?;
        append_event(
            repo_root,
            project_id,
            run_id,
            AgentRunEventKind::MessageDelta,
            json!({ "role": "user", "text": message.content }),
        )?;
    }
    let delivered_at = now_timestamp();
    let message_ids = pending.iter().map(|message| message.id).collect::<Vec<_>>();
    project_store::mark_agent_environment_pending_messages_delivered(
        repo_root,
        project_id,
        run_id,
        &message_ids,
        &delivered_at,
    )?;
    let lifecycle_snapshot =
        project_store::load_agent_environment_lifecycle_snapshot(repo_root, project_id, run_id)?;
    append_environment_lifecycle_update(
        repo_root,
        project_id,
        run_id,
        lifecycle_bookkeeping_payload(
            project_id,
            run_id,
            lifecycle_snapshot.as_ref(),
            Some("ready"),
            0,
            "Delivered queued user messages after environment readiness.",
        )?,
    )?;
    Ok(())
}

pub(crate) fn lifecycle_should_queue_user_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<bool> {
    let Some(snapshot) =
        project_store::load_agent_environment_lifecycle_snapshot(repo_root, project_id, run_id)?
    else {
        return Ok(false);
    };
    Ok(!matches!(
        snapshot.state.as_str(),
        "ready" | "failed" | "archived"
    ))
}

pub(crate) fn append_environment_lifecycle_update(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    payload: JsonValue,
) -> CommandResult<AgentEventRecord> {
    persist_environment_lifecycle_payload(
        repo_root,
        project_id,
        run_id,
        &payload,
        &now_timestamp(),
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::EnvironmentLifecycleUpdate,
        payload,
    )
}

fn environment_config_for_owned_agent_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    provider_config: &AgentProviderConfig,
    tool_runtime: &AutonomousToolRuntime,
) -> EnvironmentLifecycleConfig {
    let tool_pack_health = tool_runtime.tool_pack_health_reports();
    let mut tool_packs = tool_pack_health
        .iter()
        .filter(|report| report.enabled_by_policy)
        .map(|report| report.pack_id.clone())
        .collect::<Vec<_>>();
    if tool_packs.is_empty() {
        tool_packs.push("owned_agent_core".into());
    }

    let semantic_index_state = workspace_index_state(repo_root, project_id);
    let semantic_index_available = semantic_index_state.is_ready();
    let semantic_index_requirement_reasons = semantic_index_requirement_reasons_for_owned_agent_run(
        repo_root,
        project_id,
        run_id,
        &tool_packs,
    );
    let semantic_index_required = !semantic_index_requirement_reasons.is_empty();
    let provider_credentials_required = !matches!(provider_config, AgentProviderConfig::Fake);
    let provider_credentials_valid = provider_config_has_credentials(provider_config);

    EnvironmentLifecycleConfig {
        environment_id: format!("env-{project_id}-{run_id}"),
        project_id: project_id.into(),
        run_id: run_id.into(),
        workspace_root: repo_root.to_string_lossy().into_owned(),
        sandbox_id: None,
        sandbox_grouping_policy: SandboxGroupingPolicy::DedicatedPerSession,
        setup_scripts: Vec::new(),
        git_hooks: Vec::new(),
        required_binaries: required_binaries_for_tool_packs(&tool_pack_health),
        git_state_required: false,
        provider_credentials_required,
        provider_credentials_valid,
        tool_packs,
        semantic_index_required,
        semantic_index_available,
        semantic_index_state,
        semantic_index_requirement_reasons,
        project_instructions_loaded: repo_root.join("AGENTS.md").exists()
            || repo_root.join(".agents.md").exists()
            || repo_root.is_dir(),
    }
}

fn required_binaries_for_tool_packs(
    reports: &[xero_agent_core::DomainToolPackHealthReport],
) -> Vec<String> {
    let mut binaries = reports
        .iter()
        .filter(|report| report.enabled_by_policy)
        .flat_map(|report| report.missing_prerequisites.iter())
        .filter_map(|prerequisite| match prerequisite.as_str() {
            "adb" => Some("adb".into()),
            "xcrun" => Some("xcrun".into()),
            "solana" => Some("solana".into()),
            "anchor" => Some("anchor".into()),
            _ => None,
        })
        .collect::<Vec<String>>();
    binaries.sort();
    binaries.dedup();
    binaries
}

fn provider_config_has_credentials(config: &AgentProviderConfig) -> bool {
    match config {
        AgentProviderConfig::Fake => true,
        AgentProviderConfig::OpenAiResponses(config) => !config.api_key.trim().is_empty(),
        AgentProviderConfig::OpenAiCodexResponses(config) => {
            !config.access_token.trim().is_empty() && !config.account_id.trim().is_empty()
        }
        AgentProviderConfig::OpenAiCompatible(config) => {
            config
                .api_key
                .as_deref()
                .map(str::trim)
                .is_some_and(|key| !key.is_empty())
                || config.provider_id == crate::runtime::OLLAMA_PROVIDER_ID
        }
        AgentProviderConfig::Anthropic(config) => !config.api_key.trim().is_empty(),
        AgentProviderConfig::Bedrock(config) => {
            !config.region.trim().is_empty() && binary_available("aws")
        }
        AgentProviderConfig::Vertex(config) => {
            !config.project_id.trim().is_empty() && !config.region.trim().is_empty()
        }
    }
}

fn semantic_index_requirement_reasons_for_owned_agent_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_packs: &[String],
) -> Vec<String> {
    let mut reasons = project_store::load_agent_run(repo_root, project_id, run_id)
        .map(|snapshot| {
            let mut reasons = xero_agent_core::semantic_workspace_prompt_requirement_reasons(
                &snapshot.run.prompt,
            );
            if let Ok(definition_snapshot) =
                load_agent_definition_snapshot_for_run(repo_root, &snapshot.run)
            {
                reasons.extend(semantic_index_requirement_reasons_from_definition(
                    &definition_snapshot,
                ));
            }
            reasons
        })
        .unwrap_or_default();
    if !reasons.is_empty() && tool_packs.iter().any(|pack| pack == "project_context") {
        reasons.push("active project-context tool pack exposes semantic workspace search".into());
    }
    reasons.sort();
    reasons.dedup();
    reasons
}

fn semantic_index_requirement_reasons_from_definition(snapshot: &JsonValue) -> Vec<String> {
    let mut reasons = Vec::new();
    let retrieval_defaults = snapshot
        .get("retrievalDefaults")
        .unwrap_or(&JsonValue::Null);
    for key in [
        "required",
        "projectContextRequired",
        "projectContextRetrievalRequired",
        "semanticIndexRequired",
        "semanticWorkspaceRequired",
        "semanticWorkspaceSearchRequired",
        "semanticSearchRequired",
        "workspaceIndexRequired",
    ] {
        if retrieval_defaults
            .get(key)
            .and_then(JsonValue::as_bool)
            .unwrap_or(false)
        {
            reasons.push(format!(
                "agent definition retrievalDefaults.{key} is required"
            ));
        }
    }
    if snapshot
        .get("projectDataPolicy")
        .and_then(|policy| policy.get("retrievalRequired"))
        .and_then(JsonValue::as_bool)
        .unwrap_or(false)
    {
        reasons.push("agent definition projectDataPolicy.retrievalRequired is true".into());
    }
    reasons
}

fn workspace_index_state(repo_root: &Path, project_id: &str) -> EnvironmentSemanticIndexState {
    workspace_status_at_root(repo_root, project_id)
        .map(|status| workspace_index_state_from_dto(status.state))
        .unwrap_or(EnvironmentSemanticIndexState::Unavailable)
}

fn workspace_index_state_from_dto(state: WorkspaceIndexStateDto) -> EnvironmentSemanticIndexState {
    match state {
        WorkspaceIndexStateDto::Ready => EnvironmentSemanticIndexState::Ready,
        WorkspaceIndexStateDto::Indexing => EnvironmentSemanticIndexState::Indexing,
        WorkspaceIndexStateDto::Stale => EnvironmentSemanticIndexState::Stale,
        WorkspaceIndexStateDto::Empty => EnvironmentSemanticIndexState::Empty,
        WorkspaceIndexStateDto::Failed => EnvironmentSemanticIndexState::Failed,
    }
}

fn workspace_index_lifecycle_diagnostic(
    state: EnvironmentSemanticIndexState,
) -> EnvironmentDiagnostic {
    let (code, message, next_action) = match state {
        EnvironmentSemanticIndexState::Ready => (
            "agent_environment_workspace_index_ready",
            "Workspace index is ready for semantic retrieval.",
            "Continue the agent run.",
        ),
        EnvironmentSemanticIndexState::Indexing => (
            "agent_environment_workspace_index_indexing",
            "Workspace index is currently rebuilding.",
            "Wait for workspace indexing to finish before starting a semantic-search-required run.",
        ),
        EnvironmentSemanticIndexState::Stale => (
            "agent_environment_workspace_index_stale",
            "Workspace index is stale for semantic retrieval.",
            "Run workspace indexing before starting a semantic-search-required agent run.",
        ),
        EnvironmentSemanticIndexState::Empty => (
            "agent_environment_workspace_index_empty",
            "Workspace index has not been built yet for semantic retrieval.",
            "Run workspace indexing before starting a semantic-search-required agent run.",
        ),
        EnvironmentSemanticIndexState::Failed => (
            "agent_environment_workspace_index_failed",
            "Workspace index failed during the previous rebuild.",
            "Review workspace-index diagnostics, repair the failure, and reindex.",
        ),
        EnvironmentSemanticIndexState::Unavailable => (
            "agent_environment_workspace_index_unavailable",
            "Workspace index state is unavailable from app-data project state.",
            "Repair app-data project state permissions and re-run workspace indexing.",
        ),
    };
    EnvironmentDiagnostic::new(code, message).with_next_action(next_action)
}

fn binary_available(binary: &str) -> bool {
    let workspace_root = std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .to_string_lossy()
        .into_owned();
    let descriptor = ToolDescriptorV2 {
        name: "environment_binary_probe".into(),
        description: "Probe a required environment binary through the sandbox runner.".into(),
        input_schema: json!({ "type": "object" }),
        capability_tags: vec!["environment".into(), "subprocess".into()],
        effect_class: ToolEffectClass::CommandExecution,
        mutability: ToolMutability::ReadOnly,
        sandbox_requirement: ToolSandboxRequirement::ReadOnly,
        approval_requirement: ToolApprovalRequirement::Never,
        telemetry_attributes: Default::default(),
        result_truncation: Default::default(),
    };
    let sandbox = PermissionProfileSandbox::new(SandboxExecutionContext {
        workspace_root,
        project_trust: ProjectTrustState::Trusted,
        approval_source: SandboxApprovalSource::None,
        platform: SandboxPlatform::current(),
        preserved_environment_keys: vec!["PATH".into()],
        ..SandboxExecutionContext::default()
    });
    let argv = vec![binary.to_owned(), "--version".into()];
    let call = ToolCallInput {
        tool_call_id: format!("binary-probe-{binary}"),
        tool_name: descriptor.name.clone(),
        input: json!({ "argv": &argv }),
    };
    let Ok(metadata) = sandbox.evaluate(&descriptor, &call, &ToolExecutionContext::default())
    else {
        return false;
    };
    SandboxedProcessRunner::new()
        .run(
            SandboxedProcessRequest {
                argv,
                cwd: None,
                timeout_ms: Some(2_000),
                stdout_limit_bytes: 1024,
                stderr_limit_bytes: 1024,
                metadata,
            },
            || false,
        )
        .is_ok()
}

fn lifecycle_bookkeeping_payload(
    project_id: &str,
    run_id: &str,
    snapshot: Option<&project_store::AgentEnvironmentLifecycleSnapshotRecord>,
    state_override: Option<&str>,
    pending_message_count: i64,
    detail: &str,
) -> CommandResult<JsonValue> {
    let mut payload = snapshot
        .and_then(|snapshot| serde_json::from_str::<JsonValue>(&snapshot.snapshot_json).ok())
        .filter(JsonValue::is_object)
        .unwrap_or_else(|| {
            json!({
                "schema": xero_agent_core::ENVIRONMENT_LIFECYCLE_SCHEMA,
                "environmentId": snapshot
                    .map(|snapshot| snapshot.environment_id.clone())
                    .unwrap_or_else(|| format!("env-{project_id}-{run_id}")),
                "state": snapshot
                    .map(|snapshot| snapshot.state.clone())
                    .unwrap_or_else(|| "created".into()),
                "previousState": snapshot.and_then(|snapshot| snapshot.previous_state.clone()),
                "sandboxId": JsonValue::Null,
                "sandboxGroupingPolicy": "none",
                "healthChecks": [],
                "setupSteps": [],
                "semanticIndexRequired": false,
                "semanticIndexAvailable": false,
                "semanticIndexState": "unavailable",
                "semanticIndexRequirementReasons": [],
                "diagnostic": JsonValue::Null,
            })
        });
    let object = payload.as_object_mut().ok_or_else(|| {
        CommandError::system_fault(
            "agent_environment_lifecycle_payload_invalid",
            "Xero could not update the environment lifecycle payload.",
        )
    })?;
    object.insert(
        "schema".into(),
        JsonValue::String(xero_agent_core::ENVIRONMENT_LIFECYCLE_SCHEMA.into()),
    );
    object.entry("environmentId").or_insert_with(|| {
        JsonValue::String(
            snapshot
                .map(|snapshot| snapshot.environment_id.clone())
                .unwrap_or_else(|| format!("env-{project_id}-{run_id}")),
        )
    });
    if let Some(state) = state_override {
        let previous_state = object
            .get("state")
            .and_then(JsonValue::as_str)
            .unwrap_or("created")
            .to_string();
        object.insert("state".into(), JsonValue::String(state.into()));
        object.insert("previousState".into(), JsonValue::String(previous_state));
    } else {
        object.entry("state").or_insert_with(|| {
            JsonValue::String(
                snapshot
                    .map(|snapshot| snapshot.state.clone())
                    .unwrap_or_else(|| "created".into()),
            )
        });
        object.entry("previousState").or_insert_with(|| {
            snapshot
                .and_then(|snapshot| snapshot.previous_state.clone())
                .map(JsonValue::String)
                .unwrap_or(JsonValue::Null)
        });
    }
    object.insert(
        "pendingMessageCount".into(),
        JsonValue::Number(pending_message_count.into()),
    );
    object.insert("detail".into(), JsonValue::String(detail.into()));
    object.entry("sandboxId").or_insert(JsonValue::Null);
    object
        .entry("sandboxGroupingPolicy")
        .or_insert_with(|| JsonValue::String("none".into()));
    object.entry("healthChecks").or_insert_with(|| json!([]));
    object.entry("setupSteps").or_insert_with(|| json!([]));
    object
        .entry("semanticIndexRequired")
        .or_insert(JsonValue::Bool(false));
    object
        .entry("semanticIndexAvailable")
        .or_insert(JsonValue::Bool(false));
    object
        .entry("semanticIndexState")
        .or_insert_with(|| JsonValue::String("unavailable".into()));
    object
        .entry("semanticIndexRequirementReasons")
        .or_insert_with(|| json!([]));
    object.entry("diagnostic").or_insert(JsonValue::Null);
    Ok(payload)
}

fn persist_environment_lifecycle_payload(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    payload: &JsonValue,
    updated_at: &str,
) -> CommandResult<()> {
    let health_checks = payload
        .get("healthChecks")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let setup_steps = payload
        .get("setupSteps")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let diagnostic = payload
        .get("diagnostic")
        .filter(|diagnostic| !diagnostic.is_null())
        .cloned();
    project_store::upsert_agent_environment_lifecycle_snapshot(
        repo_root,
        &project_store::NewAgentEnvironmentLifecycleSnapshotRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            environment_id: payload_text(payload, "environmentId")
                .unwrap_or_else(|| format!("env-{project_id}-{run_id}")),
            state: payload_text(payload, "state").unwrap_or_else(|| "created".into()),
            previous_state: payload_text(payload, "previousState"),
            pending_message_count: payload
                .get("pendingMessageCount")
                .and_then(JsonValue::as_i64)
                .unwrap_or_default(),
            health_checks_json: serde_json::to_string(&health_checks).map_err(|error| {
                CommandError::system_fault(
                    "agent_environment_lifecycle_snapshot_serialize_failed",
                    format!("Xero could not serialize lifecycle health checks: {error}"),
                )
            })?,
            setup_steps_json: serde_json::to_string(&setup_steps).map_err(|error| {
                CommandError::system_fault(
                    "agent_environment_lifecycle_snapshot_serialize_failed",
                    format!("Xero could not serialize lifecycle setup steps: {error}"),
                )
            })?,
            diagnostic_json: diagnostic
                .map(|diagnostic| serde_json::to_string(&diagnostic))
                .transpose()
                .map_err(|error| {
                    CommandError::system_fault(
                        "agent_environment_lifecycle_snapshot_serialize_failed",
                        format!("Xero could not serialize lifecycle diagnostic: {error}"),
                    )
                })?,
            snapshot_json: serde_json::to_string(payload).map_err(|error| {
                CommandError::system_fault(
                    "agent_environment_lifecycle_snapshot_serialize_failed",
                    format!("Xero could not serialize lifecycle snapshot: {error}"),
                )
            })?,
            updated_at: updated_at.into(),
        },
    )?;
    Ok(())
}

impl AgentCoreStore for DesktopAgentCoreStore {
    fn semantic_workspace_index_state(&self, project_id: &str) -> EnvironmentSemanticIndexState {
        workspace_index_state(&self.repo_root, project_id)
    }

    fn insert_run(&self, _run: NewRunRecord) -> CoreResult<CoreRunSnapshot> {
        Err(CoreError::unsupported("desktop_insert_run"))
    }

    fn load_run(&self, project_id: &str, run_id: &str) -> CoreResult<CoreRunSnapshot> {
        let snapshot = project_store::load_agent_run(&self.repo_root, project_id, run_id)
            .map_err(core_error_from_command)?;
        let context_manifests = project_store::list_agent_context_manifests_for_run(
            &self.repo_root,
            project_id,
            run_id,
        )
        .map_err(core_error_from_command)?;
        Ok(core_snapshot_from_desktop(snapshot, context_manifests))
    }

    fn append_message(&self, message: CoreNewMessageRecord) -> CoreResult<CoreRunSnapshot> {
        append_message(
            &self.repo_root,
            &message.project_id,
            &message.run_id,
            desktop_message_role_from_core(&message.role),
            message.content,
        )
        .map_err(core_error_from_command)?;
        self.load_run(&message.project_id, &message.run_id)
    }

    fn append_event(&self, event: NewRuntimeEvent) -> CoreResult<CoreRuntimeEvent> {
        let desktop_kind = desktop_event_kind_from_core(&event.event_kind);
        let payload = event.payload;
        let persisted = if desktop_kind == AgentRunEventKind::EnvironmentLifecycleUpdate {
            append_environment_lifecycle_update(
                &self.repo_root,
                &event.project_id,
                &event.run_id,
                payload.clone(),
            )
            .map_err(core_error_from_command)?
        } else {
            append_event(
                &self.repo_root,
                &event.project_id,
                &event.run_id,
                desktop_kind.clone(),
                payload.clone(),
            )
            .map_err(core_error_from_command)?
        };
        if desktop_kind == AgentRunEventKind::RunFailed {
            let diagnostic = project_store::AgentRunDiagnosticRecord {
                code: payload_text(&payload, "code").unwrap_or_else(|| "agent_run_failed".into()),
                message: payload_text(&payload, "message")
                    .unwrap_or_else(|| "Owned-agent run failed.".into()),
            };
            project_store::update_agent_run_status(
                &self.repo_root,
                &event.project_id,
                &event.run_id,
                AgentRunStatus::Failed,
                Some(diagnostic),
                &persisted.created_at,
            )
            .map_err(core_error_from_command)?;
        }
        let trace_id = xero_agent_core::runtime_trace_id_for_run(&event.project_id, &event.run_id);
        let event_kind = event.event_kind;
        Ok(CoreRuntimeEvent {
            id: persisted.id,
            project_id: persisted.project_id,
            run_id: persisted.run_id.clone(),
            event_kind: event_kind.clone(),
            trace: event.trace.unwrap_or_else(|| {
                RuntimeTraceContext::for_event(
                    &trace_id,
                    &persisted.run_id,
                    persisted.id,
                    &event_kind,
                )
            }),
            payload,
            created_at: persisted.created_at,
        })
    }

    fn record_context_manifest(
        &self,
        manifest: NewContextManifest,
    ) -> CoreResult<CoreContextManifest> {
        let snapshot =
            project_store::load_agent_run(&self.repo_root, &manifest.project_id, &manifest.run_id)
                .map_err(core_error_from_command)?;
        let record = project_store::insert_agent_context_manifest(
            &self.repo_root,
            &project_store::NewAgentContextManifestRecord {
                manifest_id: manifest.manifest_id.clone(),
                project_id: manifest.project_id.clone(),
                agent_session_id: manifest.agent_session_id.clone(),
                run_id: Some(manifest.run_id.clone()),
                runtime_agent_id: snapshot.run.runtime_agent_id,
                agent_definition_id: snapshot.run.agent_definition_id,
                agent_definition_version: snapshot.run.agent_definition_version,
                provider_id: Some(manifest.provider_id.clone()),
                model_id: Some(manifest.model_id.clone()),
                request_kind: project_store::AgentContextManifestRequestKind::ProviderTurn,
                policy_action: project_store::AgentContextPolicyAction::ContinueNow,
                policy_reason_code: "desktop_core_store".into(),
                budget_tokens: None,
                estimated_tokens: 0,
                pressure: project_store::AgentContextBudgetPressure::Unknown,
                context_hash: manifest.context_hash.clone(),
                included_contributors: Vec::new(),
                excluded_contributors: Vec::new(),
                retrieval_query_ids: Vec::new(),
                retrieval_result_ids: Vec::new(),
                compaction_id: None,
                handoff_id: None,
                redaction_state: project_store::AgentContextRedactionState::Clean,
                manifest: manifest.manifest.clone(),
                created_at: now_timestamp(),
            },
        )
        .map_err(core_error_from_command)?;
        Ok(CoreContextManifest {
            manifest_id: record.manifest_id,
            project_id: record.project_id,
            agent_session_id: record.agent_session_id,
            run_id: record.run_id.unwrap_or_default(),
            provider_id: record.provider_id.unwrap_or_default(),
            model_id: record.model_id.unwrap_or_default(),
            turn_index: manifest.turn_index,
            context_hash: record.context_hash,
            recorded_after_event_id: None,
            trace: manifest.trace.unwrap_or_else(|| {
                RuntimeTraceContext::for_context_manifest(
                    &xero_agent_core::runtime_trace_id_for_run(
                        &manifest.project_id,
                        &manifest.run_id,
                    ),
                    &manifest.run_id,
                    &manifest.manifest_id,
                    manifest.turn_index,
                )
            }),
            manifest: record.manifest,
            created_at: record.created_at,
        })
    }

    fn update_run_status(
        &self,
        project_id: &str,
        run_id: &str,
        status: CoreRunStatus,
    ) -> CoreResult<CoreRunSnapshot> {
        project_store::update_agent_run_status(
            &self.repo_root,
            project_id,
            run_id,
            desktop_status_from_core(&status),
            None,
            &now_timestamp(),
        )
        .map_err(core_error_from_command)?;
        self.load_run(project_id, run_id)
    }

    fn export_trace(&self, project_id: &str, run_id: &str) -> CoreResult<CoreRuntimeTrace> {
        let snapshot = self.load_run(project_id, run_id)?;
        CoreRuntimeTrace::from_snapshot(snapshot)
    }
}

fn core_snapshot_from_desktop(
    snapshot: AgentRunSnapshotRecord,
    context_manifests: Vec<project_store::AgentContextManifestRecord>,
) -> CoreRunSnapshot {
    CoreRunSnapshot {
        trace_id: snapshot.run.trace_id.clone(),
        project_id: snapshot.run.project_id.clone(),
        agent_session_id: snapshot.run.agent_session_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        provider_id: snapshot.run.provider_id.clone(),
        model_id: snapshot.run.model_id.clone(),
        status: core_status_from_desktop(&snapshot.run.status),
        prompt: snapshot.run.prompt,
        messages: snapshot
            .messages
            .into_iter()
            .map(core_message_from_desktop)
            .collect(),
        events: snapshot
            .events
            .into_iter()
            .map(core_event_from_desktop)
            .collect(),
        context_manifests: context_manifests
            .into_iter()
            .map(core_context_manifest_from_desktop)
            .collect(),
    }
}

fn core_message_from_desktop(message: AgentMessageRecord) -> CoreRuntimeMessage {
    CoreRuntimeMessage {
        id: message.id,
        project_id: message.project_id,
        run_id: message.run_id,
        role: core_message_role_from_desktop(&message.role),
        content: message.content,
        provider_metadata: None,
        created_at: message.created_at,
    }
}

fn core_event_from_desktop(event: AgentEventRecord) -> CoreRuntimeEvent {
    let payload = serde_json::from_str::<JsonValue>(&event.payload_json).unwrap_or(JsonValue::Null);
    let trace_id = xero_agent_core::runtime_trace_id_for_run(&event.project_id, &event.run_id);
    let event_kind = core_event_kind_from_desktop(&event.event_kind);
    CoreRuntimeEvent {
        id: event.id,
        project_id: event.project_id,
        run_id: event.run_id.clone(),
        event_kind: event_kind.clone(),
        trace: RuntimeTraceContext::for_event(&trace_id, &event.run_id, event.id, &event_kind),
        payload,
        created_at: event.created_at,
    }
}

fn core_context_manifest_from_desktop(
    manifest: project_store::AgentContextManifestRecord,
) -> CoreContextManifest {
    let project_id = manifest.project_id.clone();
    let run_id = manifest.run_id.clone().unwrap_or_default();
    let manifest_id = manifest.manifest_id.clone();
    let trace_id = xero_agent_core::runtime_trace_id_for_run(&project_id, &run_id);
    let turn_index = manifest
        .manifest
        .get("turnIndex")
        .and_then(JsonValue::as_u64)
        .unwrap_or_default() as usize;
    CoreContextManifest {
        manifest_id: manifest.manifest_id,
        project_id: manifest.project_id,
        agent_session_id: manifest.agent_session_id,
        run_id: run_id.clone(),
        provider_id: manifest.provider_id.unwrap_or_default(),
        model_id: manifest.model_id.unwrap_or_default(),
        turn_index,
        context_hash: manifest.context_hash,
        recorded_after_event_id: None,
        trace: RuntimeTraceContext::for_context_manifest(
            &trace_id,
            &run_id,
            &manifest_id,
            turn_index,
        ),
        manifest: manifest.manifest,
        created_at: manifest.created_at,
    }
}

fn core_status_from_desktop(status: &AgentRunStatus) -> CoreRunStatus {
    match status {
        AgentRunStatus::Starting => CoreRunStatus::Starting,
        AgentRunStatus::Running => CoreRunStatus::Running,
        AgentRunStatus::Paused => CoreRunStatus::Paused,
        AgentRunStatus::Cancelling => CoreRunStatus::Cancelling,
        AgentRunStatus::Cancelled => CoreRunStatus::Cancelled,
        AgentRunStatus::HandedOff => CoreRunStatus::HandedOff,
        AgentRunStatus::Completed => CoreRunStatus::Completed,
        AgentRunStatus::Failed => CoreRunStatus::Failed,
    }
}

fn desktop_status_from_core(status: &CoreRunStatus) -> AgentRunStatus {
    match status {
        CoreRunStatus::Starting => AgentRunStatus::Starting,
        CoreRunStatus::Running => AgentRunStatus::Running,
        CoreRunStatus::Paused => AgentRunStatus::Paused,
        CoreRunStatus::Cancelling => AgentRunStatus::Cancelling,
        CoreRunStatus::Cancelled => AgentRunStatus::Cancelled,
        CoreRunStatus::HandedOff => AgentRunStatus::HandedOff,
        CoreRunStatus::Completed => AgentRunStatus::Completed,
        CoreRunStatus::Failed => AgentRunStatus::Failed,
    }
}

fn core_message_role_from_desktop(role: &AgentMessageRole) -> CoreMessageRole {
    match role {
        AgentMessageRole::System => CoreMessageRole::System,
        AgentMessageRole::Developer => CoreMessageRole::Developer,
        AgentMessageRole::User => CoreMessageRole::User,
        AgentMessageRole::Assistant => CoreMessageRole::Assistant,
        AgentMessageRole::Tool => CoreMessageRole::Tool,
    }
}

fn desktop_message_role_from_core(role: &CoreMessageRole) -> AgentMessageRole {
    match role {
        CoreMessageRole::System => AgentMessageRole::System,
        CoreMessageRole::Developer => AgentMessageRole::Developer,
        CoreMessageRole::User => AgentMessageRole::User,
        CoreMessageRole::Assistant => AgentMessageRole::Assistant,
        CoreMessageRole::Tool => AgentMessageRole::Tool,
    }
}

fn core_event_kind_from_desktop(kind: &AgentRunEventKind) -> CoreRuntimeEventKind {
    match kind {
        AgentRunEventKind::RunStarted => CoreRuntimeEventKind::RunStarted,
        AgentRunEventKind::MessageDelta => CoreRuntimeEventKind::MessageDelta,
        AgentRunEventKind::ReasoningSummary => CoreRuntimeEventKind::ReasoningSummary,
        AgentRunEventKind::ToolStarted => CoreRuntimeEventKind::ToolStarted,
        AgentRunEventKind::ToolDelta => CoreRuntimeEventKind::ToolDelta,
        AgentRunEventKind::ToolCompleted => CoreRuntimeEventKind::ToolCompleted,
        AgentRunEventKind::FileChanged => CoreRuntimeEventKind::FileChanged,
        AgentRunEventKind::CommandOutput => CoreRuntimeEventKind::CommandOutput,
        AgentRunEventKind::ValidationStarted => CoreRuntimeEventKind::ValidationStarted,
        AgentRunEventKind::ValidationCompleted => CoreRuntimeEventKind::ValidationCompleted,
        AgentRunEventKind::ToolRegistrySnapshot => CoreRuntimeEventKind::ToolRegistrySnapshot,
        AgentRunEventKind::PolicyDecision => CoreRuntimeEventKind::PolicyDecision,
        AgentRunEventKind::StateTransition => CoreRuntimeEventKind::StateTransition,
        AgentRunEventKind::PlanUpdated => CoreRuntimeEventKind::PlanUpdated,
        AgentRunEventKind::VerificationGate => CoreRuntimeEventKind::VerificationGate,
        AgentRunEventKind::ContextManifestRecorded => CoreRuntimeEventKind::ContextManifestRecorded,
        AgentRunEventKind::RetrievalPerformed => CoreRuntimeEventKind::RetrievalPerformed,
        AgentRunEventKind::MemoryCandidateCaptured => CoreRuntimeEventKind::MemoryCandidateCaptured,
        AgentRunEventKind::EnvironmentLifecycleUpdate => {
            CoreRuntimeEventKind::EnvironmentLifecycleUpdate
        }
        AgentRunEventKind::SandboxLifecycleUpdate => CoreRuntimeEventKind::SandboxLifecycleUpdate,
        AgentRunEventKind::ActionRequired => CoreRuntimeEventKind::ActionRequired,
        AgentRunEventKind::ApprovalRequired => CoreRuntimeEventKind::ApprovalRequired,
        AgentRunEventKind::ToolPermissionGrant => CoreRuntimeEventKind::ToolPermissionGrant,
        AgentRunEventKind::ProviderModelChanged => CoreRuntimeEventKind::ProviderModelChanged,
        AgentRunEventKind::RuntimeSettingsChanged => CoreRuntimeEventKind::RuntimeSettingsChanged,
        AgentRunEventKind::RunPaused => CoreRuntimeEventKind::RunPaused,
        AgentRunEventKind::RunCompleted => CoreRuntimeEventKind::RunCompleted,
        AgentRunEventKind::RunFailed => CoreRuntimeEventKind::RunFailed,
    }
}

fn desktop_event_kind_from_core(kind: &CoreRuntimeEventKind) -> AgentRunEventKind {
    match kind {
        CoreRuntimeEventKind::MessageDelta => AgentRunEventKind::MessageDelta,
        CoreRuntimeEventKind::ReasoningSummary => AgentRunEventKind::ReasoningSummary,
        CoreRuntimeEventKind::ToolStarted => AgentRunEventKind::ToolStarted,
        CoreRuntimeEventKind::ToolDelta => AgentRunEventKind::ToolDelta,
        CoreRuntimeEventKind::ToolCompleted => AgentRunEventKind::ToolCompleted,
        CoreRuntimeEventKind::FileChanged => AgentRunEventKind::FileChanged,
        CoreRuntimeEventKind::CommandOutput => AgentRunEventKind::CommandOutput,
        CoreRuntimeEventKind::ValidationStarted => AgentRunEventKind::ValidationStarted,
        CoreRuntimeEventKind::ValidationCompleted => AgentRunEventKind::ValidationCompleted,
        CoreRuntimeEventKind::ToolRegistrySnapshot => AgentRunEventKind::ToolRegistrySnapshot,
        CoreRuntimeEventKind::PolicyDecision => AgentRunEventKind::PolicyDecision,
        CoreRuntimeEventKind::StateTransition => AgentRunEventKind::StateTransition,
        CoreRuntimeEventKind::PlanUpdated => AgentRunEventKind::PlanUpdated,
        CoreRuntimeEventKind::VerificationGate => AgentRunEventKind::VerificationGate,
        CoreRuntimeEventKind::EnvironmentLifecycleUpdate => {
            AgentRunEventKind::EnvironmentLifecycleUpdate
        }
        CoreRuntimeEventKind::ActionRequired => AgentRunEventKind::ActionRequired,
        CoreRuntimeEventKind::ApprovalRequired => AgentRunEventKind::ApprovalRequired,
        CoreRuntimeEventKind::RunPaused => AgentRunEventKind::RunPaused,
        CoreRuntimeEventKind::RunCompleted => AgentRunEventKind::RunCompleted,
        CoreRuntimeEventKind::RunFailed => AgentRunEventKind::RunFailed,
        CoreRuntimeEventKind::RunStarted => AgentRunEventKind::RunStarted,
        CoreRuntimeEventKind::ContextManifestRecorded => AgentRunEventKind::ContextManifestRecorded,
        CoreRuntimeEventKind::RetrievalPerformed => AgentRunEventKind::RetrievalPerformed,
        CoreRuntimeEventKind::MemoryCandidateCaptured => AgentRunEventKind::MemoryCandidateCaptured,
        CoreRuntimeEventKind::SandboxLifecycleUpdate => AgentRunEventKind::SandboxLifecycleUpdate,
        CoreRuntimeEventKind::ToolPermissionGrant => AgentRunEventKind::ToolPermissionGrant,
        CoreRuntimeEventKind::ProviderModelChanged => AgentRunEventKind::ProviderModelChanged,
        CoreRuntimeEventKind::RuntimeSettingsChanged => AgentRunEventKind::RuntimeSettingsChanged,
    }
}

fn payload_text(payload: &JsonValue, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn core_error_from_command(error: CommandError) -> CoreError {
    match error.class {
        CommandErrorClass::SystemFault => CoreError::system_fault(error.code, error.message),
        _ => CoreError::invalid_request(error.code, error.message),
    }
}

fn command_error_from_core(error: CoreError) -> CommandError {
    if error.code.ends_with("_failed") || error.code.contains("lock") {
        CommandError::system_fault(error.code, error.message)
    } else {
        CommandError::user_fixable(error.code, error.message)
    }
}
