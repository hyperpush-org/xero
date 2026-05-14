use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Read, Write},
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
    sync::Arc,
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::{json, Value as JsonValue};

use crate::{
    AgentCoreStore, AgentRuntimeFacade, ApprovalDecisionRequest, CancelRunRequest,
    CompactSessionRequest, ContinueRunRequest, CoreError, CoreResult, EnvironmentLifecycleConfig,
    EnvironmentLifecycleService, ExportTraceRequest, FakeProviderRuntime, FileAgentCoreStore,
    ForkSessionRequest, MessageRole, NewContextManifest, NewMessageRecord, NewRunRecord,
    NewRuntimeEvent, OpenAiCompatibleProviderPreflightProbeRequest, PermissionProfileSandbox,
    ProductionRuntimeContract, ProjectTrustState, ProviderCapabilityCatalogInput,
    ProviderPreflightInput, ProviderPreflightRequiredFeatures, ProviderPreflightSnapshot,
    ProviderPreflightSource, ProviderSelection, ResumeRunRequest, RunSnapshot, RunStatus,
    RuntimeEventKind, RuntimeMessageProviderMetadata, RuntimeProviderToolCallMetadata,
    RuntimeTrace, RuntimeTraceContext, SandboxApprovalSource, SandboxExecutionContext,
    SandboxPlatform, StartRunRequest, StaticToolHandler, ToolApplicationKind,
    ToolApplicationMetadata, ToolApprovalRequirement, ToolBatchDispatchReport,
    ToolBatchDispatchSafety, ToolBudget, ToolCallInput, ToolDescriptorV2, ToolDispatchConfig,
    ToolDispatchFailure, ToolDispatchOutcome, ToolDispatchSuccess, ToolEffectClass,
    ToolExecutionContext, ToolExecutionError, ToolGroupExecutionMode, ToolHandlerOutput,
    ToolMutability, ToolPolicy, ToolPolicyDecision, ToolRegistryV2, ToolResultTruncationContract,
    ToolRollback, ToolSandboxRequirement, UserInputRequest,
};

const DEFAULT_HEADLESS_PROVIDER_TIMEOUT_MS: u64 = 120_000;
const DEFAULT_HEADLESS_MAX_PROVIDER_TURNS: usize = 8;
const DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS: u64 = 120_000;
const MAX_HEADLESS_COMMAND_TIMEOUT_MS: u64 = 10 * 60 * 1_000;
const MAX_TOOL_OUTPUT_BYTES: usize = 128 * 1024;
const HEADLESS_TOOL_READ: &str = "read";
const HEADLESS_TOOL_LIST: &str = "list";
const HEADLESS_TOOL_WRITE: &str = "write";
const HEADLESS_TOOL_PATCH: &str = "patch";
const HEADLESS_TOOL_COMMAND: &str = "command";
const LEGACY_HEADLESS_MINI_TOOLS: &[&str] = &["read_file", "write_file", "list_files"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlessProviderExecutionConfig {
    Fake,
    OpenAiCompatible(OpenAiCompatibleHeadlessConfig),
    OpenAiCodexResponses(OpenAiCodexHeadlessConfig),
}

impl HeadlessProviderExecutionConfig {
    pub fn provider_id(&self) -> &str {
        match self {
            Self::Fake => "fake_provider",
            Self::OpenAiCompatible(config) => config.provider_id.as_str(),
            Self::OpenAiCodexResponses(config) => config.provider_id.as_str(),
        }
    }

    pub fn model_id(&self) -> &str {
        match self {
            Self::Fake => "fake-model",
            Self::OpenAiCompatible(config) => config.model_id.as_str(),
            Self::OpenAiCodexResponses(config) => config.model_id.as_str(),
        }
    }

    fn has_provider_credentials(&self) -> bool {
        match self {
            Self::Fake => true,
            Self::OpenAiCompatible(config) => {
                config
                    .api_key
                    .as_deref()
                    .is_some_and(|key| !key.trim().is_empty())
                    || is_local_http_endpoint(&config.base_url)
            }
            Self::OpenAiCodexResponses(config) => {
                !config.access_token.trim().is_empty() && !config.account_id.trim().is_empty()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCompatibleHeadlessConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub timeout_ms: u64,
    pub workspace_root: Option<PathBuf>,
    pub allow_workspace_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiCodexHeadlessConfig {
    pub provider_id: String,
    pub model_id: String,
    pub base_url: String,
    pub access_token: String,
    pub account_id: String,
    pub session_id: Option<String>,
    pub timeout_ms: u64,
    pub workspace_root: Option<PathBuf>,
    pub allow_workspace_writes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessRuntimeOptions {
    pub ci_mode: bool,
    pub max_provider_turns: usize,
    pub max_wall_time_ms: Option<u64>,
    pub max_tool_calls: Option<u64>,
    pub max_command_calls: Option<u64>,
    pub provider_preflight: Option<ProviderPreflightSnapshot>,
}

impl Default for HeadlessRuntimeOptions {
    fn default() -> Self {
        Self {
            ci_mode: false,
            max_provider_turns: DEFAULT_HEADLESS_MAX_PROVIDER_TURNS,
            max_wall_time_ms: None,
            max_tool_calls: None,
            max_command_calls: None,
            provider_preflight: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HeadlessProviderRuntime<S = FileAgentCoreStore> {
    store: S,
    provider: HeadlessProviderExecutionConfig,
    options: HeadlessRuntimeOptions,
}

impl<S> HeadlessProviderRuntime<S>
where
    S: AgentCoreStore,
{
    pub fn new(
        store: S,
        provider: HeadlessProviderExecutionConfig,
        options: HeadlessRuntimeOptions,
    ) -> Self {
        Self {
            store,
            provider,
            options,
        }
    }

    pub fn store(&self) -> S {
        self.store.clone()
    }

    fn start_real_run(&self, request: StartRunRequest) -> CoreResult<RunSnapshot> {
        self.validate_selected_provider(&request.provider)?;
        let runtime_contract = ProductionRuntimeContract::real_provider(
            "headless_provider_runtime",
            request.project_id.clone(),
            request.provider.provider_id.clone(),
            request.provider.model_id.clone(),
            self.store.runtime_store_descriptor(&request.project_id),
        );
        crate::validate_production_runtime_contract(&runtime_contract)?;
        let preflight = self.provider_preflight_snapshot()?;
        let blockers = crate::provider_preflight_blockers(&preflight);
        if let Some(blocker) = blockers.first() {
            return Err(CoreError::invalid_request(
                "agent_core_provider_preflight_blocked",
                format!(
                    "Headless real-provider execution is blocked because `{}` failed: {}",
                    blocker.code, blocker.message
                ),
            ));
        }
        if self.options.ci_mode {
            return Err(CoreError::invalid_request(
                "agent_core_ci_real_provider_blocked",
                "Headless CI mode requires explicit harness execution with `--provider fake_provider` until non-interactive write approvals are configured.",
            ));
        }

        let snapshot = self.store.insert_run(NewRunRecord {
            trace_id: None,
            project_id: request.project_id,
            agent_session_id: request.agent_session_id,
            run_id: request.run_id,
            provider_id: request.provider.provider_id,
            model_id: request.provider.model_id,
            prompt: request.prompt.clone(),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunStarted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "run_started",
            )),
            payload: json!({
                "status": "starting",
                "providerId": snapshot.provider_id,
                "modelId": snapshot.model_id,
                "execution": "production_real_provider",
                "providerPreflight": preflight.clone(),
            }),
        })?;

        let semantic_index_requirement_reasons =
            crate::semantic_workspace_prompt_requirement_reasons(&request.prompt);
        let semantic_index_required = !semantic_index_requirement_reasons.is_empty();
        let semantic_index_state = self
            .store
            .semantic_workspace_index_state(&snapshot.project_id);
        let lifecycle = EnvironmentLifecycleService::new(self.store.clone());
        let environment = lifecycle.start_environment(EnvironmentLifecycleConfig {
            environment_id: format!("env-{}-{}", snapshot.project_id, snapshot.run_id),
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            workspace_root: self
                .workspace_root()
                .map(|root| root.display().to_string())
                .unwrap_or_else(|| ".".into()),
            provider_credentials_required: true,
            provider_credentials_valid: self.provider.has_provider_credentials(),
            tool_packs: vec!["owned_agent_core".into(), "tool_registry_v2".into()],
            semantic_index_required,
            semantic_index_available: semantic_index_state.is_ready(),
            semantic_index_state,
            semantic_index_requirement_reasons,
            ..EnvironmentLifecycleConfig::local(&snapshot.project_id, &snapshot.run_id)
        })?;
        if !environment.state.is_ready() {
            return self.store.load_run(&snapshot.project_id, &snapshot.run_id);
        }

        self.store.append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::System,
            content: headless_system_prompt(self.workspace_root().as_deref()),
            provider_metadata: None,
        })?;
        self.store.append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::User,
            content: request.prompt.clone(),
            provider_metadata: None,
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::MessageDelta,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "user_message",
            )),
            payload: json!({ "role": "user", "text": request.prompt }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ValidationStarted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "provider_diagnostics_started",
            )),
            payload: json!({ "label": "provider_diagnostics" }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ValidationCompleted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "provider_diagnostics_completed",
            )),
            payload: json!({
                "label": "provider_diagnostics",
                "outcome": preflight.status.as_str(),
                "providerId": self.provider.provider_id(),
                "modelId": self.provider.model_id(),
                "providerPreflight": preflight.clone(),
            }),
        })?;

        let started = self
            .store
            .load_run(&snapshot.project_id, &snapshot.run_id)?;
        self.drive_real_turn(started, &preflight)?;
        self.store.load_run(&snapshot.project_id, &snapshot.run_id)
    }

    fn continue_real_run(&self, request: ContinueRunRequest) -> CoreResult<RunSnapshot> {
        let before = self.store.load_run(&request.project_id, &request.run_id)?;
        self.validate_selected_provider(&ProviderSelection {
            provider_id: before.provider_id.clone(),
            model_id: before.model_id.clone(),
        })?;
        self.store
            .update_run_status(&request.project_id, &request.run_id, RunStatus::Running)?;
        self.store.append_message(NewMessageRecord {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            role: MessageRole::User,
            content: request.prompt.clone(),
            provider_metadata: None,
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            event_kind: RuntimeEventKind::MessageDelta,
            trace: Some(RuntimeTraceContext::for_run(
                &before.trace_id,
                &request.run_id,
                "user_message",
            )),
            payload: json!({ "role": "user", "text": request.prompt }),
        })?;
        let preflight = self.provider_preflight_snapshot()?;
        let blockers = crate::provider_preflight_blockers(&preflight);
        if let Some(blocker) = blockers.first() {
            return Err(CoreError::invalid_request(
                "agent_core_provider_preflight_blocked",
                format!(
                    "Headless real-provider continuation is blocked because `{}` failed: {}",
                    blocker.code, blocker.message
                ),
            ));
        }
        self.store.append_event(NewRuntimeEvent {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            event_kind: RuntimeEventKind::ValidationCompleted,
            trace: Some(RuntimeTraceContext::for_run(
                &before.trace_id,
                &request.run_id,
                "provider_diagnostics_completed",
            )),
            payload: json!({
                "label": "provider_diagnostics",
                "outcome": preflight.status.as_str(),
                "providerId": self.provider.provider_id(),
                "modelId": self.provider.model_id(),
                "providerPreflight": preflight.clone(),
            }),
        })?;
        let snapshot = self.store.load_run(&request.project_id, &request.run_id)?;
        self.drive_real_turn(snapshot, &preflight)?;
        self.store.load_run(&request.project_id, &request.run_id)
    }

    fn drive_real_turn(
        &self,
        snapshot: RunSnapshot,
        provider_preflight: &ProviderPreflightSnapshot,
    ) -> CoreResult<()> {
        let provider_timeout_ms = match &self.provider {
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => config.timeout_ms,
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => config.timeout_ms,
            HeadlessProviderExecutionConfig::Fake => {
                return Err(CoreError::invalid_request(
                    "agent_core_provider_mismatch",
                    "The fake provider cannot drive a real provider turn.",
                ));
            }
        };
        let client = Client::builder()
            .timeout(Duration::from_millis(normalize_timeout(
                provider_timeout_ms,
            )))
            .build()
            .map_err(|error| {
                CoreError::system_fault(
                    "agent_core_provider_http_client_failed",
                    format!("Xero could not build the headless provider HTTP client: {error}"),
                )
            })?;
        let mut chat_messages = chat_messages_from_snapshot(&snapshot);
        let mut current = snapshot;
        let started_at = Instant::now();
        let mut tool_call_count = 0_u64;
        let mut command_call_count = 0_u64;
        for turn_index in 0..self.options.max_provider_turns {
            if let Some(max_wall_time_ms) = self.options.max_wall_time_ms {
                if started_at.elapsed().as_millis() as u64 > max_wall_time_ms {
                    return self.fail_real_provider_run(
                        &current,
                        "agent_core_headless_wall_time_exceeded",
                        "The headless real-provider loop exceeded its configured wall-time limit.",
                        "wall_time_limit_exceeded",
                        json!({
                            "limitMs": max_wall_time_ms,
                            "elapsedMs": started_at.elapsed().as_millis() as u64,
                        }),
                    );
                }
            }
            let workspace_root = self.workspace_root();
            let tool_runtime = HeadlessProductionToolRuntime::new(
                workspace_root.as_ref(),
                self.allow_workspace_writes(),
                self.app_data_roots_for_project(&current.project_id),
            )?;
            self.record_tool_registry_snapshot(&current, turn_index, &tool_runtime)?;
            self.record_provider_context_manifest(
                &current,
                turn_index,
                &tool_runtime,
                provider_preflight,
            )?;
            let response = match &self.provider {
                HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                    send_openai_compatible_chat(
                        &client,
                        config,
                        &chat_messages,
                        tool_runtime.openai_tool_definitions(),
                    )?
                }
                HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                    send_openai_codex_responses(
                        &client,
                        config,
                        &chat_messages,
                        tool_runtime.openai_response_tool_definitions(),
                    )?
                }
                HeadlessProviderExecutionConfig::Fake => {
                    unreachable!("fake provider rejected above")
                }
            };
            let content = response.content_text();
            let tool_calls = response.tool_calls;
            let next_tool_call_count = tool_call_count + tool_calls.len() as u64;
            if self
                .options
                .max_tool_calls
                .is_some_and(|limit| next_tool_call_count > limit)
            {
                return self.fail_real_provider_run(
                    &current,
                    "agent_core_headless_tool_call_limit_exceeded",
                    "The headless real-provider loop exceeded its configured tool-call limit.",
                    "tool_call_limit_exceeded",
                    json!({
                        "limit": self.options.max_tool_calls,
                        "attemptedToolCalls": next_tool_call_count,
                    }),
                );
            }
            let next_command_call_count = command_call_count
                + tool_calls
                    .iter()
                    .filter(|call| headless_tool_is_command(&call.name))
                    .count() as u64;
            if self
                .options
                .max_command_calls
                .is_some_and(|limit| next_command_call_count > limit)
            {
                return self.fail_real_provider_run(
                    &current,
                    "agent_core_headless_command_call_limit_exceeded",
                    "The headless real-provider loop exceeded its configured command-call limit.",
                    "command_call_limit_exceeded",
                    json!({
                        "limit": self.options.max_command_calls,
                        "attemptedCommandCalls": next_command_call_count,
                    }),
                );
            }

            if !content.trim().is_empty() {
                self.store.append_event(NewRuntimeEvent {
                    project_id: current.project_id.clone(),
                    run_id: current.run_id.clone(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: Some(RuntimeTraceContext::for_provider_turn(
                        &current.trace_id,
                        &current.run_id,
                        turn_index,
                    )),
                    payload: json!({ "role": "assistant", "text": content }),
                })?;
            }

            if tool_calls.is_empty() {
                if !content.trim().is_empty() {
                    self.store.append_message(NewMessageRecord {
                        project_id: current.project_id.clone(),
                        run_id: current.run_id.clone(),
                        role: MessageRole::Assistant,
                        content,
                        provider_metadata: None,
                    })?;
                }
                self.store.update_run_status(
                    &current.project_id,
                    &current.run_id,
                    RunStatus::Completed,
                )?;
                self.store.append_event(NewRuntimeEvent {
                    project_id: current.project_id.clone(),
                    run_id: current.run_id.clone(),
                    event_kind: RuntimeEventKind::RunCompleted,
                    trace: Some(RuntimeTraceContext::for_run(
                        &current.trace_id,
                        &current.run_id,
                        "run_completed",
                    )),
                    payload: json!({
                        "summary": "Headless real-provider run completed.",
                        "state": "complete",
                    }),
                })?;
                return Ok(());
            }

            let assistant_provider_message_id =
                provider_assistant_message_id(&current.run_id, turn_index);
            chat_messages.push(openai_assistant_message(&content, &tool_calls));
            self.store.append_message(NewMessageRecord {
                project_id: current.project_id.clone(),
                run_id: current.run_id.clone(),
                role: MessageRole::Assistant,
                content: content.clone(),
                provider_metadata: Some(RuntimeMessageProviderMetadata::assistant_tool_calls(
                    assistant_provider_message_id.clone(),
                    tool_calls
                        .iter()
                        .map(RuntimeProviderToolCallMetadata::from)
                        .collect(),
                )),
            })?;

            for result in self.dispatch_headless_tool_batch(
                &tool_runtime,
                &current,
                turn_index,
                &tool_calls,
                &assistant_provider_message_id,
            )? {
                let result_payload = serde_json::to_string(&result.payload).map_err(|error| {
                    CoreError::system_fault(
                        "agent_core_tool_result_encode_failed",
                        format!("Xero could not encode a headless tool result: {error}"),
                    )
                })?;
                self.store.append_message(NewMessageRecord {
                    project_id: current.project_id.clone(),
                    run_id: current.run_id.clone(),
                    role: MessageRole::Tool,
                    content: result_payload.clone(),
                    provider_metadata: Some(RuntimeMessageProviderMetadata::tool_result(
                        provider_tool_result_message_id(
                            &current.run_id,
                            turn_index,
                            &result.tool_call_id,
                        ),
                        result.tool_call_id.clone(),
                        result.tool_name.clone(),
                        result.parent_assistant_message_id.clone(),
                    )),
                })?;
                chat_messages.push(json!({
                    "role": "tool",
                    "tool_call_id": result.tool_call_id,
                    "content": result_payload,
                }));
            }
            tool_call_count = next_tool_call_count;
            command_call_count = next_command_call_count;
            current = self.store.load_run(&current.project_id, &current.run_id)?;
        }

        self.store
            .update_run_status(&current.project_id, &current.run_id, RunStatus::Failed)?;
        self.store.append_event(NewRuntimeEvent {
            project_id: current.project_id.clone(),
            run_id: current.run_id.clone(),
            event_kind: RuntimeEventKind::RunFailed,
            trace: Some(RuntimeTraceContext::for_run(
                &current.trace_id,
                &current.run_id,
                "turn_limit_exceeded",
            )),
            payload: json!({
                "code": "agent_core_headless_turn_limit_exceeded",
                "message": "The headless real-provider loop reached its turn limit.",
                "retryable": true,
            }),
        })?;
        Err(CoreError::invalid_request(
            "agent_core_headless_turn_limit_exceeded",
            "The headless real-provider loop reached its turn limit.",
        ))
    }

    fn fail_real_provider_run(
        &self,
        snapshot: &RunSnapshot,
        code: &'static str,
        message: &'static str,
        trace_label: &'static str,
        details: JsonValue,
    ) -> CoreResult<()> {
        self.store
            .update_run_status(&snapshot.project_id, &snapshot.run_id, RunStatus::Failed)?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunFailed,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                trace_label,
            )),
            payload: json!({
                "code": code,
                "message": message,
                "retryable": false,
                "details": details,
            }),
        })?;
        Err(CoreError::invalid_request(code, message))
    }

    fn dispatch_headless_tool_batch(
        &self,
        tool_runtime: &HeadlessProductionToolRuntime,
        snapshot: &RunSnapshot,
        turn_index: usize,
        tool_calls: &[OpenAiToolCall],
        parent_assistant_message_id: &str,
    ) -> CoreResult<Vec<HeadlessToolResultMessage>> {
        if tool_calls.is_empty() {
            return Ok(Vec::new());
        }

        let inputs = tool_calls
            .iter()
            .map(|call| ToolCallInput {
                tool_call_id: call.id.clone(),
                tool_name: call.name.clone(),
                input: call.arguments.clone(),
            })
            .collect::<Vec<_>>();
        for input in &inputs {
            let (persisted_input, input_redacted) =
                redacted_headless_tool_input(&input.tool_name, &input.input);
            self.store.append_event(NewRuntimeEvent {
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                event_kind: RuntimeEventKind::ToolStarted,
                trace: Some(RuntimeTraceContext::for_tool_call(
                    &snapshot.trace_id,
                    &snapshot.run_id,
                    &input.tool_call_id,
                )),
                payload: json!({
                    "toolCallId": input.tool_call_id,
                    "toolName": input.tool_name,
                    "turnIndex": turn_index,
                    "runtime": "production_real_provider",
                    "input": persisted_input,
                    "inputRedacted": input_redacted,
                    "dispatch": {
                        "registryVersion": "tool_registry_v2",
                        "providerLoop": "headless_production_provider_loop",
                    },
                }),
            })?;
        }

        let report = tool_runtime.dispatch_batch(
            &snapshot.project_id,
            &snapshot.run_id,
            turn_index,
            &inputs,
        )?;
        self.persist_headless_tool_report(snapshot, report, parent_assistant_message_id)
    }

    fn record_provider_context_manifest(
        &self,
        snapshot: &RunSnapshot,
        turn_index: usize,
        tool_runtime: &HeadlessProductionToolRuntime,
        provider_preflight: &ProviderPreflightSnapshot,
    ) -> CoreResult<()> {
        let manifest_id = format!("context-manifest-{}-{turn_index}", snapshot.run_id);
        let context_hash = headless_context_hash(snapshot, turn_index);
        let provider_preflight_hash = stable_provider_preflight_hash(provider_preflight);
        self.store.record_context_manifest(NewContextManifest {
            manifest_id: manifest_id.clone(),
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            run_id: snapshot.run_id.clone(),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            turn_index,
            context_hash: context_hash.clone(),
            trace: Some(RuntimeTraceContext::for_context_manifest(
                &snapshot.trace_id,
                &snapshot.run_id,
                &manifest_id,
                turn_index,
            )),
            manifest: json!({
                "kind": "provider_context_package",
                "schema": "xero.provider_context_package.v1",
                "schemaVersion": 1,
                "projectId": snapshot.project_id,
                "agentSessionId": snapshot.agent_session_id,
                "runId": snapshot.run_id,
                "providerId": snapshot.provider_id,
                "modelId": snapshot.model_id,
                "turnIndex": turn_index,
                "runtime": "production_real_provider",
                "workspaceRoot": self.workspace_root().map(|root| root.display().to_string()),
                "tools": tool_runtime.tool_names(),
                "executionRegistry": "tool_registry_v2",
                "providerPreflight": provider_preflight,
                "admittedProviderPreflightHash": provider_preflight_hash,
            }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ContextManifestRecorded,
            trace: Some(RuntimeTraceContext::for_storage_write(
                &snapshot.trace_id,
                &snapshot.run_id,
                "context_manifest",
                turn_index,
            )),
            payload: json!({
                "manifestId": manifest_id,
                "contextHash": context_hash,
                "turnIndex": turn_index,
                "runtime": "production_real_provider",
            }),
        })?;
        Ok(())
    }

    fn record_tool_registry_snapshot(
        &self,
        snapshot: &RunSnapshot,
        turn_index: usize,
        tool_runtime: &HeadlessProductionToolRuntime,
    ) -> CoreResult<()> {
        let descriptors = tool_runtime.descriptors();
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ToolRegistrySnapshot,
            trace: Some(RuntimeTraceContext::for_provider_turn(
                &snapshot.trace_id,
                &snapshot.run_id,
                turn_index,
            )),
            payload: json!({
                "kind": "active_tool_registry",
                "runtime": "production_real_provider",
                "providerLoop": "headless_production_provider_loop",
                "turnIndex": turn_index,
                "executionRegistry": "tool_registry_v2",
                "descriptorNames": descriptors.iter().map(|descriptor| descriptor.name.clone()).collect::<Vec<_>>(),
                "descriptorsV2": descriptors,
                "legacyMiniToolsAvailable": false,
                "unavailableMiniTools": LEGACY_HEADLESS_MINI_TOOLS,
            }),
        })?;
        Ok(())
    }

    fn persist_headless_tool_report(
        &self,
        snapshot: &RunSnapshot,
        report: ToolBatchDispatchReport,
        parent_assistant_message_id: &str,
    ) -> CoreResult<Vec<HeadlessToolResultMessage>> {
        let mut results = Vec::new();
        for group in report.groups {
            let persistence_group = HeadlessToolPersistenceGroup {
                mode: group.mode.clone(),
                elapsed_ms: group.elapsed_ms,
                timeout_error: group.timeout_error.as_ref(),
            };
            for outcome in group.outcomes {
                match outcome {
                    ToolDispatchOutcome::Succeeded(success) => {
                        self.persist_headless_tool_success(
                            snapshot,
                            success,
                            &persistence_group,
                            parent_assistant_message_id,
                            &mut results,
                        )?;
                    }
                    ToolDispatchOutcome::Failed(failure) => {
                        self.persist_headless_tool_failure(
                            snapshot,
                            failure,
                            &persistence_group,
                            parent_assistant_message_id,
                            &mut results,
                        )?;
                    }
                }
            }
        }
        Ok(results)
    }

    fn persist_headless_tool_success(
        &self,
        snapshot: &RunSnapshot,
        success: ToolDispatchSuccess,
        group: &HeadlessToolPersistenceGroup<'_>,
        parent_assistant_message_id: &str,
        results: &mut Vec<HeadlessToolResultMessage>,
    ) -> CoreResult<()> {
        let dispatch = headless_dispatch_success_metadata(
            &success,
            group.mode.clone(),
            group.elapsed_ms,
            group.timeout_error,
        );
        let tool_call_id = success.tool_call_id.clone();
        let tool_name = success.tool_name.clone();
        let provider_payload = json!({
            "toolCallId": tool_call_id,
            "toolName": tool_name,
            "ok": true,
            "summary": success.summary,
            "output": success.output,
            "parentAssistantMessageId": parent_assistant_message_id,
            "providerToolName": tool_name,
        });
        if tool_name == HEADLESS_TOOL_WRITE {
            self.record_headless_file_changed(
                snapshot,
                provider_payload["output"]["path"].clone(),
                "write",
                json!({
                    "bytes": provider_payload["output"]["bytes"].clone(),
                    "fileReservation": provider_payload["output"]["fileReservation"].clone(),
                    "rollback": provider_payload["output"]["rollback"].clone(),
                }),
            )?;
        } else if tool_name == HEADLESS_TOOL_PATCH {
            for path in provider_payload["output"]["changedFiles"]
                .as_array()
                .into_iter()
                .flatten()
            {
                self.record_headless_file_changed(snapshot, path.clone(), "patch", json!({}))?;
            }
        }
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ToolCompleted,
            trace: Some(RuntimeTraceContext::for_tool_call(
                &snapshot.trace_id,
                &snapshot.run_id,
                provider_payload["toolCallId"].as_str().unwrap_or_default(),
            )),
            payload: json!({
                "toolCallId": provider_payload["toolCallId"].clone(),
                "toolName": provider_payload["toolName"].clone(),
                "ok": true,
                "summary": provider_payload["summary"].clone(),
                "resultPreview": truncate_text(&provider_payload.to_string(), 2048),
                "dispatch": dispatch,
            }),
        })?;
        results.push(HeadlessToolResultMessage {
            tool_call_id: provider_payload["toolCallId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            tool_name,
            parent_assistant_message_id: parent_assistant_message_id.to_owned(),
            payload: provider_payload,
        });
        Ok(())
    }

    fn record_headless_file_changed(
        &self,
        snapshot: &RunSnapshot,
        path: JsonValue,
        operation: &'static str,
        dispatch_extra: JsonValue,
    ) -> CoreResult<()> {
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::FileChanged,
            trace: Some(RuntimeTraceContext::for_storage_write(
                &snapshot.trace_id,
                &snapshot.run_id,
                "workspace_file",
                snapshot.context_manifests.len(),
            )),
            payload: json!({
                "path": path,
                "operation": operation,
                "runtime": "production_real_provider",
                "dispatch": {
                    "registryVersion": "tool_registry_v2",
                    "details": dispatch_extra,
                },
            }),
        })?;
        Ok(())
    }

    fn persist_headless_tool_failure(
        &self,
        snapshot: &RunSnapshot,
        failure: ToolDispatchFailure,
        group: &HeadlessToolPersistenceGroup<'_>,
        parent_assistant_message_id: &str,
        results: &mut Vec<HeadlessToolResultMessage>,
    ) -> CoreResult<()> {
        let dispatch = headless_dispatch_failure_metadata(
            &failure,
            group.mode.clone(),
            group.elapsed_ms,
            group.timeout_error,
        );
        let tool_call_id = failure.tool_call_id.clone();
        let tool_name = failure.tool_name.clone();
        let provider_payload = json!({
            "toolCallId": tool_call_id,
            "toolName": tool_name,
            "ok": false,
            "error": {
                "category": failure.error.category,
                "code": failure.error.code,
                "message": failure.error.model_message,
                "retryable": failure.error.retryable,
            },
            "parentAssistantMessageId": parent_assistant_message_id,
            "providerToolName": tool_name,
        });
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ToolCompleted,
            trace: Some(RuntimeTraceContext::for_tool_call(
                &snapshot.trace_id,
                &snapshot.run_id,
                provider_payload["toolCallId"].as_str().unwrap_or_default(),
            )),
            payload: json!({
                "toolCallId": provider_payload["toolCallId"].clone(),
                "toolName": provider_payload["toolName"].clone(),
                "ok": false,
                "code": provider_payload["error"]["code"].clone(),
                "message": provider_payload["error"]["message"].clone(),
                "dispatch": dispatch,
            }),
        })?;
        results.push(HeadlessToolResultMessage {
            tool_call_id: provider_payload["toolCallId"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            tool_name,
            parent_assistant_message_id: parent_assistant_message_id.to_owned(),
            payload: provider_payload,
        });
        Ok(())
    }

    fn validate_selected_provider(&self, provider: &ProviderSelection) -> CoreResult<()> {
        if provider.provider_id != self.provider.provider_id() {
            return Err(CoreError::invalid_request(
                "agent_core_provider_mismatch",
                format!(
                    "Headless runtime was configured for provider `{}` but request selected `{}`.",
                    self.provider.provider_id(),
                    provider.provider_id
                ),
            ));
        }
        if provider.model_id != self.provider.model_id() {
            return Err(CoreError::invalid_request(
                "agent_core_model_mismatch",
                format!(
                    "Headless runtime was configured for model `{}` but request selected `{}`.",
                    self.provider.model_id(),
                    provider.model_id
                ),
            ));
        }
        Ok(())
    }

    fn workspace_root(&self) -> Option<PathBuf> {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => None,
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                config.workspace_root.clone()
            }
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                config.workspace_root.clone()
            }
        }
    }

    fn allow_workspace_writes(&self) -> bool {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => false,
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                config.allow_workspace_writes
            }
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                config.allow_workspace_writes
            }
        }
    }

    fn app_data_roots_for_project(&self, project_id: &str) -> Vec<String> {
        let descriptor = self.store.runtime_store_descriptor(project_id);
        let mut roots = BTreeSet::new();
        if let Some(root) = descriptor.root_path {
            roots.insert(root);
        }
        if let Some(database_path) = descriptor.database_path {
            if let Some(parent) = Path::new(&database_path).parent() {
                roots.insert(parent.display().to_string());
            }
        }
        roots.into_iter().collect()
    }

    fn provider_preflight_snapshot(&self) -> CoreResult<ProviderPreflightSnapshot> {
        if let Some(snapshot) = self.options.provider_preflight.clone() {
            return Ok(snapshot);
        }

        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => {
                Ok(crate::provider_preflight_snapshot(ProviderPreflightInput {
                    profile_id: "fake_provider".into(),
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                    source: ProviderPreflightSource::LiveProbe,
                    checked_at: crate::now_timestamp(),
                    age_seconds: Some(0),
                    ttl_seconds: None,
                    required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
                    capabilities: crate::provider_capability_catalog(
                        ProviderCapabilityCatalogInput {
                            provider_id: "fake_provider".into(),
                            model_id: "fake-model".into(),
                            catalog_source: "live".into(),
                            fetched_at: Some(crate::now_timestamp()),
                            last_success_at: Some(crate::now_timestamp()),
                            cache_age_seconds: Some(0),
                            cache_ttl_seconds: Some(crate::DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
                            credential_proof: Some("none_required".into()),
                            context_window_tokens: Some(128_000),
                            max_output_tokens: Some(16_384),
                            context_limit_source: Some("built_in_registry".into()),
                            context_limit_confidence: Some("high".into()),
                            thinking_supported: false,
                            thinking_efforts: Vec::new(),
                            thinking_default_effort: None,
                        },
                    ),
                    credential_ready: Some(true),
                    endpoint_reachable: Some(true),
                    model_available: Some(true),
                    streaming_route_available: Some(true),
                    tool_schema_accepted: Some(true),
                    reasoning_controls_accepted: None,
                    attachments_accepted: None,
                    context_limit_known: Some(true),
                    provider_error: None,
                }))
            }
            HeadlessProviderExecutionConfig::OpenAiCompatible(config) => {
                Ok(crate::run_openai_compatible_provider_preflight_probe(
                    OpenAiCompatibleProviderPreflightProbeRequest {
                        profile_id: config.provider_id.clone(),
                        provider_id: config.provider_id.clone(),
                        model_id: config.model_id.clone(),
                        required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(
                        ),
                        base_url: config.base_url.clone(),
                        api_key: config.api_key.clone(),
                        timeout_ms: config.timeout_ms,
                        credential_proof: config
                            .api_key
                            .as_deref()
                            .filter(|key| !key.trim().is_empty())
                            .map(|_| "api_key_env_recorded".to_string())
                            .or_else(|| {
                                is_local_http_endpoint(&config.base_url)
                                    .then(|| "local_endpoint".to_string())
                            }),
                        context_window_tokens: Some(128_000),
                        max_output_tokens: Some(16_384),
                        context_limit_source: Some("configured_default".into()),
                        context_limit_confidence: Some("medium".into()),
                        thinking_supported: false,
                        thinking_efforts: Vec::new(),
                        thinking_default_effort: None,
                    },
                ))
            }
            HeadlessProviderExecutionConfig::OpenAiCodexResponses(config) => {
                Ok(crate::provider_preflight_snapshot(ProviderPreflightInput {
                    profile_id: "benchmark-openai-codex-oauth".into(),
                    provider_id: config.provider_id.clone(),
                    model_id: config.model_id.clone(),
                    source: ProviderPreflightSource::LiveProbe,
                    checked_at: crate::now_timestamp(),
                    age_seconds: Some(0),
                    ttl_seconds: None,
                    required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
                    capabilities: crate::provider_capability_catalog(
                        ProviderCapabilityCatalogInput {
                            provider_id: config.provider_id.clone(),
                            model_id: config.model_id.clone(),
                            catalog_source: "app_oauth_session".into(),
                            fetched_at: Some(crate::now_timestamp()),
                            last_success_at: Some(crate::now_timestamp()),
                            cache_age_seconds: Some(0),
                            cache_ttl_seconds: Some(crate::DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
                            credential_proof: Some("app_data_openai_codex_session".into()),
                            context_window_tokens: Some(272_000),
                            max_output_tokens: Some(16_384),
                            context_limit_source: Some("configured_default".into()),
                            context_limit_confidence: Some("medium".into()),
                            thinking_supported: true,
                            thinking_efforts: vec!["low".into(), "medium".into(), "high".into()],
                            thinking_default_effort: Some("medium".into()),
                        },
                    ),
                    credential_ready: Some(self.provider.has_provider_credentials()),
                    endpoint_reachable: Some(true),
                    model_available: Some(true),
                    streaming_route_available: Some(true),
                    tool_schema_accepted: Some(true),
                    reasoning_controls_accepted: None,
                    attachments_accepted: None,
                    context_limit_known: Some(true),
                    provider_error: None,
                }))
            }
        }
    }

    fn fork_headless_session(&self, request: ForkSessionRequest) -> CoreResult<RunSnapshot> {
        let source = self
            .store
            .latest_run_for_session(&request.project_id, &request.source_agent_session_id)?;
        let run_id = generate_headless_id("run-fork");
        let forked = self.store.insert_run(NewRunRecord {
            trace_id: None,
            project_id: request.project_id.clone(),
            agent_session_id: request.target_agent_session_id.clone(),
            run_id: run_id.clone(),
            provider_id: source.provider_id.clone(),
            model_id: source.model_id.clone(),
            prompt: source.prompt.clone(),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: forked.project_id.clone(),
            run_id: forked.run_id.clone(),
            event_kind: RuntimeEventKind::RunStarted,
            trace: Some(RuntimeTraceContext::for_run(
                &forked.trace_id,
                &forked.run_id,
                "session_forked",
            )),
            payload: json!({
                "kind": "session_forked",
                "sourceAgentSessionId": request.source_agent_session_id,
                "targetAgentSessionId": request.target_agent_session_id,
                "sourceRunId": source.run_id,
                "sourceTraceId": source.trace_id,
            }),
        })?;
        for message in source.messages {
            self.store.append_message(NewMessageRecord {
                project_id: forked.project_id.clone(),
                run_id: forked.run_id.clone(),
                role: message.role,
                content: message.content,
                provider_metadata: message.provider_metadata,
            })?;
        }
        let manifest_id = format!("context-manifest-{}-fork", forked.run_id);
        self.store.record_context_manifest(NewContextManifest {
            manifest_id: manifest_id.clone(),
            project_id: forked.project_id.clone(),
            agent_session_id: forked.agent_session_id.clone(),
            run_id: forked.run_id.clone(),
            provider_id: forked.provider_id.clone(),
            model_id: forked.model_id.clone(),
            turn_index: 0,
            context_hash: headless_context_hash(&forked, 0),
            trace: Some(RuntimeTraceContext::for_context_manifest(
                &forked.trace_id,
                &forked.run_id,
                &manifest_id,
                0,
            )),
            manifest: json!({
                "kind": "session_fork",
                "schema": "xero.session_fork.v1",
                "sourceRunId": source.run_id,
                "sourceTraceId": source.trace_id,
            }),
        })?;
        self.store
            .update_run_status(&forked.project_id, &forked.run_id, RunStatus::Completed)?;
        self.store.load_run(&forked.project_id, &forked.run_id)
    }

    fn compact_headless_session(&self, request: CompactSessionRequest) -> CoreResult<RunSnapshot> {
        let snapshot = self
            .store
            .latest_run_for_session(&request.project_id, &request.agent_session_id)?;
        let turn_index = snapshot.context_manifests.len();
        let manifest_id = format!("context-manifest-{}-compact-{turn_index}", snapshot.run_id);
        let summary = compact_summary_from_snapshot(&snapshot);
        let context_hash = headless_context_hash(&snapshot, turn_index);
        self.store.record_context_manifest(NewContextManifest {
            manifest_id: manifest_id.clone(),
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            run_id: snapshot.run_id.clone(),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            turn_index,
            context_hash: context_hash.clone(),
            trace: Some(RuntimeTraceContext::for_context_manifest(
                &snapshot.trace_id,
                &snapshot.run_id,
                &manifest_id,
                turn_index,
            )),
            manifest: json!({
                "kind": "session_compaction_artifact",
                "schema": "xero.session_compaction_artifact.v1",
                "reason": request.reason,
                "summary": summary,
                "rawTailMessageCount": 6,
                "runtime": "headless_facade",
            }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::PolicyDecision,
            trace: Some(RuntimeTraceContext::for_storage_write(
                &snapshot.trace_id,
                &snapshot.run_id,
                "session_compaction",
                turn_index,
            )),
            payload: json!({
                "kind": "session_compaction",
                "action": "compacted",
                "manifestId": manifest_id,
                "reason": request.reason,
                "contextHash": context_hash,
            }),
        })?;
        self.store.load_run(&snapshot.project_id, &snapshot.run_id)
    }
}

impl<S> AgentRuntimeFacade for HeadlessProviderRuntime<S>
where
    S: AgentCoreStore,
{
    type StartRunRequest = StartRunRequest;
    type ContinueRunRequest = ContinueRunRequest;
    type UserInputRequest = UserInputRequest;
    type ApprovalRequest = ApprovalDecisionRequest;
    type RejectRequest = ApprovalDecisionRequest;
    type CancelRunRequest = CancelRunRequest;
    type ResumeRunRequest = ResumeRunRequest;
    type ForkSessionRequest = ForkSessionRequest;
    type CompactSessionRequest = CompactSessionRequest;
    type ExportTraceRequest = ExportTraceRequest;
    type Snapshot = RunSnapshot;
    type Trace = RuntimeTrace;
    type Error = CoreError;

    fn start_run(&self, request: StartRunRequest) -> CoreResult<RunSnapshot> {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => {
                FakeProviderRuntime::new(self.store.clone()).start_run(request)
            }
            HeadlessProviderExecutionConfig::OpenAiCompatible(_)
            | HeadlessProviderExecutionConfig::OpenAiCodexResponses(_) => {
                self.start_real_run(request)
            }
        }
    }

    fn continue_run(&self, request: ContinueRunRequest) -> CoreResult<RunSnapshot> {
        match &self.provider {
            HeadlessProviderExecutionConfig::Fake => {
                FakeProviderRuntime::new(self.store.clone()).continue_run(request)
            }
            HeadlessProviderExecutionConfig::OpenAiCompatible(_)
            | HeadlessProviderExecutionConfig::OpenAiCodexResponses(_) => {
                self.continue_real_run(request)
            }
        }
    }

    fn submit_user_input(&self, request: UserInputRequest) -> CoreResult<RunSnapshot> {
        self.continue_run(ContinueRunRequest {
            project_id: request.project_id,
            run_id: request.run_id,
            prompt: request.text,
        })
    }

    fn approve_action(&self, request: ApprovalDecisionRequest) -> CoreResult<RunSnapshot> {
        self.continue_run(ContinueRunRequest {
            project_id: request.project_id,
            run_id: request.run_id,
            prompt: request.response.unwrap_or_else(|| {
                format!(
                    "Approved action `{}` through headless facade.",
                    request.action_id
                )
            }),
        })
    }

    fn reject_action(&self, request: ApprovalDecisionRequest) -> CoreResult<RunSnapshot> {
        self.store.append_event(NewRuntimeEvent {
            project_id: request.project_id.clone(),
            run_id: request.run_id.clone(),
            event_kind: RuntimeEventKind::PolicyDecision,
            trace: None,
            payload: json!({
                "kind": "approval",
                "actionId": request.action_id,
                "decision": "rejected",
                "response": request.response,
                "runtime": "headless_facade",
            }),
        })?;
        self.store.load_run(&request.project_id, &request.run_id)
    }

    fn cancel_run(&self, request: CancelRunRequest) -> CoreResult<RunSnapshot> {
        self.store
            .update_run_status(&request.project_id, &request.run_id, RunStatus::Cancelled)
    }

    fn resume_run(&self, request: ResumeRunRequest) -> CoreResult<RunSnapshot> {
        self.continue_run(ContinueRunRequest {
            project_id: request.project_id,
            run_id: request.run_id,
            prompt: request.response,
        })
    }

    fn fork_session(&self, request: ForkSessionRequest) -> CoreResult<RunSnapshot> {
        self.fork_headless_session(request)
    }

    fn compact_session(&self, request: CompactSessionRequest) -> CoreResult<RunSnapshot> {
        self.compact_headless_session(request)
    }

    fn export_trace(&self, request: ExportTraceRequest) -> CoreResult<RuntimeTrace> {
        self.store
            .export_trace(&request.project_id, &request.run_id)
    }
}

#[derive(Debug, Clone)]
struct OpenAiToolCall {
    id: String,
    name: String,
    arguments: JsonValue,
}

impl From<&OpenAiToolCall> for RuntimeProviderToolCallMetadata {
    fn from(call: &OpenAiToolCall) -> Self {
        Self {
            tool_call_id: call.id.clone(),
            provider_tool_name: call.name.clone(),
            arguments: call.arguments.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct HeadlessToolPersistenceGroup<'a> {
    mode: ToolGroupExecutionMode,
    elapsed_ms: u128,
    timeout_error: Option<&'a ToolExecutionError>,
}

#[derive(Debug, Clone)]
struct HeadlessToolResultMessage {
    tool_call_id: String,
    tool_name: String,
    parent_assistant_message_id: String,
    payload: JsonValue,
}

#[derive(Debug, Clone)]
struct HeadlessCommandOutput {
    argv: Vec<String>,
    cwd: String,
    stdout: Option<String>,
    stderr: Option<String>,
    stdout_truncated: bool,
    stderr_truncated: bool,
    exit_code: Option<i32>,
    timed_out: bool,
    elapsed_ms: u64,
    context_epoch: String,
    tool_call_id: String,
}

#[derive(Debug, Clone)]
struct LimitedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

#[derive(Debug, Clone)]
struct OpenAiProviderMessage {
    content: JsonValue,
    tool_calls: Vec<OpenAiToolCall>,
}

impl OpenAiProviderMessage {
    fn content_text(&self) -> String {
        match &self.content {
            JsonValue::String(text) => text.clone(),
            JsonValue::Array(items) => items
                .iter()
                .filter_map(|item| {
                    item.get("text")
                        .and_then(JsonValue::as_str)
                        .or_else(|| item.get("content").and_then(JsonValue::as_str))
                })
                .collect::<Vec<_>>()
                .join(""),
            JsonValue::Null => String::new(),
            other => other.to_string(),
        }
    }
}

fn provider_assistant_message_id(run_id: &str, turn_index: usize) -> String {
    format!("provider-assistant-{run_id}-{turn_index}")
}

fn provider_tool_result_message_id(run_id: &str, turn_index: usize, tool_call_id: &str) -> String {
    format!("provider-tool-result-{run_id}-{turn_index}-{tool_call_id}")
}

#[derive(Debug, Clone)]
struct HeadlessProductionToolRuntime {
    workspace_root: PathBuf,
    allow_workspace_writes: bool,
    app_data_roots: Vec<String>,
}

impl HeadlessProductionToolRuntime {
    fn new(
        workspace_root: Option<&PathBuf>,
        allow_workspace_writes: bool,
        app_data_roots: Vec<String>,
    ) -> CoreResult<Self> {
        let workspace_root = workspace_root.ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_headless_workspace_missing",
                "Production Tool Registry V2 dispatch requires a registered workspace root.",
            )
        })?;
        let workspace_root = fs::canonicalize(workspace_root).map_err(|error| {
            CoreError::invalid_request(
                "agent_core_headless_workspace_unavailable",
                format!(
                    "Workspace root `{}` is unavailable: {error}",
                    workspace_root.display()
                ),
            )
        })?;
        Ok(Self {
            workspace_root,
            allow_workspace_writes,
            app_data_roots,
        })
    }

    fn descriptors(&self) -> Vec<ToolDescriptorV2> {
        let mut descriptors = vec![headless_read_descriptor(), headless_list_descriptor()];
        if self.allow_workspace_writes {
            descriptors.push(headless_write_descriptor());
            descriptors.push(headless_patch_descriptor());
            descriptors.push(headless_command_descriptor());
        }
        descriptors
    }

    fn tool_names(&self) -> Vec<String> {
        self.descriptors()
            .into_iter()
            .map(|descriptor| descriptor.name)
            .collect()
    }

    fn openai_tool_definitions(&self) -> Vec<JsonValue> {
        self.descriptors()
            .into_iter()
            .map(openai_tool_definition_from_descriptor)
            .collect()
    }

    fn openai_response_tool_definitions(&self) -> Vec<JsonValue> {
        self.descriptors()
            .into_iter()
            .map(openai_response_tool_definition_from_descriptor)
            .collect()
    }

    fn dispatch_batch(
        &self,
        project_id: &str,
        run_id: &str,
        turn_index: usize,
        inputs: &[ToolCallInput],
    ) -> CoreResult<ToolBatchDispatchReport> {
        let registry = self.build_registry()?;
        let budget = ToolBudget {
            max_command_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            ..ToolBudget::default()
        };
        let config = ToolDispatchConfig {
            budget,
            policy: Arc::new(HeadlessProductionToolPolicy {
                allow_workspace_writes: self.allow_workspace_writes,
            }),
            sandbox: Arc::new(PermissionProfileSandbox::new(SandboxExecutionContext {
                workspace_root: self.workspace_root.display().to_string(),
                app_data_roots: self.app_data_roots.clone(),
                project_trust: ProjectTrustState::Trusted,
                approval_source: if self.allow_workspace_writes {
                    SandboxApprovalSource::Policy
                } else {
                    SandboxApprovalSource::None
                },
                platform: SandboxPlatform::current(),
                preserved_environment_keys: vec!["PATH".into()],
                ..SandboxExecutionContext::default()
            })),
            rollback: Some(Arc::new(HeadlessFileRollback {
                workspace_root: self.workspace_root.clone(),
            })),
            context: ToolExecutionContext {
                project_id: project_id.into(),
                run_id: run_id.into(),
                turn_index,
                context_epoch: format!("turn-{turn_index}"),
                telemetry_attributes: BTreeMap::from([
                    (
                        "xero.dispatch.path".into(),
                        "headless_production_provider_loop".into(),
                    ),
                    ("xero.dispatch.registry".into(), "tool_registry_v2".into()),
                ]),
            },
        };
        Ok(registry.dispatch_batch(inputs, &config))
    }

    fn build_registry(&self) -> CoreResult<ToolRegistryV2> {
        let mut registry = ToolRegistryV2::new();

        let read_runtime = self.clone();
        registry
            .register(StaticToolHandler::new_cancellable(
                headless_read_descriptor(),
                move |_context, call, control| {
                    control.ensure_not_cancelled(&call.tool_name)?;
                    let output = read_runtime.read(call)?;
                    control.ensure_not_cancelled(&call.tool_name)?;
                    Ok(output)
                },
            ))
            .map_err(tool_execution_error_to_core_error)?;

        let list_runtime = self.clone();
        registry
            .register(StaticToolHandler::new_cancellable(
                headless_list_descriptor(),
                move |_context, call, control| {
                    control.ensure_not_cancelled(&call.tool_name)?;
                    let output = list_runtime.list(call)?;
                    control.ensure_not_cancelled(&call.tool_name)?;
                    Ok(output)
                },
            ))
            .map_err(tool_execution_error_to_core_error)?;

        if self.allow_workspace_writes {
            let write_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_write_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = write_runtime.write(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;

            let patch_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_patch_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = patch_runtime.apply_patch(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;

            let command_runtime = self.clone();
            registry
                .register(StaticToolHandler::new_cancellable(
                    headless_command_descriptor(),
                    move |context, call, control| {
                        control.ensure_not_cancelled(&call.tool_name)?;
                        let output = command_runtime.command(context, call)?;
                        control.ensure_not_cancelled(&call.tool_name)?;
                        Ok(output)
                    },
                ))
                .map_err(tool_execution_error_to_core_error)?;
        }

        Ok(registry)
    }

    fn read(&self, call: &ToolCallInput) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let path = required_tool_string(&call.input, "path")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, false)
            .map_err(core_error_to_tool_execution_error)?;
        let content = fs::read_to_string(&resolved).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_read_failed",
                format!("Xero could not read `{path}`: {error}"),
            )
        })?;
        Ok(ToolHandlerOutput::new(
            format!("Read `{path}` through Tool Registry V2."),
            json!({
                "ok": true,
                "path": path,
                "content": truncate_text(&content, MAX_TOOL_OUTPUT_BYTES),
                "truncated": content.len() > MAX_TOOL_OUTPUT_BYTES,
            }),
        ))
    }

    fn list(&self, call: &ToolCallInput) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let prefix = call
            .input
            .get("path")
            .and_then(JsonValue::as_str)
            .unwrap_or(".");
        let start = resolve_workspace_path_for_root(&self.workspace_root, prefix, false)
            .map_err(core_error_to_tool_execution_error)?;
        let mut files = Vec::new();
        collect_workspace_files(&self.workspace_root, &start, &mut files, 200)
            .map_err(core_error_to_tool_execution_error)?;
        Ok(ToolHandlerOutput::new(
            format!("Listed files below `{prefix}` through Tool Registry V2."),
            json!({
                "ok": true,
                "root": self.workspace_root.display().to_string(),
                "path": prefix,
                "files": files,
                "truncated": files.len() >= 200,
            }),
        ))
    }

    fn write(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let path = required_tool_string(&call.input, "path")?;
        let content = required_tool_string(&call.input, "content")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, true)
            .map_err(core_error_to_tool_execution_error)?;
        let rollback = rollback_checkpoint_metadata(path, &resolved);
        let file_reservation = file_reservation_metadata(context, call, path);
        if let Some(parent) = resolved.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_write_prepare_failed",
                    format!(
                        "Xero could not prepare `{}` for writing: {error}",
                        parent.display()
                    ),
                )
            })?;
        }
        fs::write(&resolved, content.as_bytes()).map_err(|error| {
            ToolExecutionError::retryable(
                "agent_core_headless_write_failed",
                format!("Xero could not write `{path}`: {error}"),
            )
        })?;
        Ok(ToolHandlerOutput::new(
            format!("Wrote `{path}` through Tool Registry V2."),
            json!({
                "ok": true,
                "path": path,
                "bytes": content.len(),
                "rollback": rollback,
                "fileReservation": file_reservation,
            }),
        ))
    }

    fn apply_patch(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let patch = required_tool_string(&call.input, "patch")?;
        if patch.trim().is_empty() {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_patch_empty",
                "Patch input cannot be empty.",
            ));
        }
        let changed_files = patch_changed_paths(patch);
        for path in &changed_files {
            let _ = resolve_workspace_path_for_root(&self.workspace_root, path, true)
                .map_err(core_error_to_tool_execution_error)?;
        }

        let check = self.run_git_apply(context, call, patch, true)?;
        if check.exit_code != Some(0) {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_patch_check_failed",
                format!(
                    "Patch did not apply cleanly: {}",
                    check.stderr.as_deref().unwrap_or_default()
                ),
            ));
        }
        let applied = self.run_git_apply(context, call, patch, false)?;
        if applied.exit_code != Some(0) {
            return Err(ToolExecutionError::retryable(
                "agent_core_headless_patch_apply_failed",
                format!(
                    "Patch application failed: {}",
                    applied.stderr.as_deref().unwrap_or_default()
                ),
            ));
        }

        Ok(ToolHandlerOutput::new(
            format!("Applied patch touching {} file(s).", changed_files.len()),
            json!({
                "ok": true,
                "changedFiles": changed_files,
                "stdout": truncate_text(applied.stdout.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "stderr": truncate_text(applied.stderr.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "exitCode": applied.exit_code,
                "patchBytes": patch.len(),
                "patchRedacted": true,
            }),
        ))
    }

    fn run_git_apply(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
        patch: &str,
        check_only: bool,
    ) -> Result<HeadlessCommandOutput, ToolExecutionError> {
        let mut argv = vec![
            "git".to_string(),
            "apply".to_string(),
            "--whitespace=nowarn".to_string(),
        ];
        if check_only {
            argv.push("--check".into());
        }
        argv.push("-".into());
        self.run_process(
            context,
            call,
            argv,
            self.workspace_root.clone(),
            Some(patch.as_bytes()),
            Some(DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS),
        )
    }

    fn command(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> Result<ToolHandlerOutput, ToolExecutionError> {
        let argv = required_tool_string_array(&call.input, "argv")?;
        let cwd = call
            .input
            .get("cwd")
            .and_then(JsonValue::as_str)
            .unwrap_or(".");
        let cwd_path = resolve_workspace_path_for_root(&self.workspace_root, cwd, false)
            .map_err(core_error_to_tool_execution_error)?;
        if !cwd_path.is_dir() {
            return Err(ToolExecutionError::invalid_input(
                "agent_core_headless_command_cwd_invalid",
                format!("Command cwd `{cwd}` is not a directory."),
            ));
        }
        let timeout_ms = call
            .input
            .get("timeoutMs")
            .and_then(JsonValue::as_u64)
            .map(|value| value.clamp(1_000, MAX_HEADLESS_COMMAND_TIMEOUT_MS))
            .or(Some(DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS));
        let output = self.run_process(context, call, argv, cwd_path, None, timeout_ms)?;
        let ok = output.exit_code == Some(0) && !output.timed_out;
        Ok(ToolHandlerOutput::new(
            if ok {
                "Command completed successfully through Tool Registry V2."
            } else {
                "Command completed with a non-zero status through Tool Registry V2."
            },
            json!({
                "ok": ok,
                "argv": output.argv,
                "cwd": output.cwd,
                "stdout": truncate_text(output.stdout.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "stderr": truncate_text(output.stderr.as_deref().unwrap_or_default(), MAX_TOOL_OUTPUT_BYTES),
                "stdoutTruncated": output.stdout_truncated,
                "stderrTruncated": output.stderr_truncated,
                "exitCode": output.exit_code,
                "timedOut": output.timed_out,
                "elapsedMs": output.elapsed_ms,
                "contextEpoch": output.context_epoch,
                "toolCallId": output.tool_call_id,
            }),
        ))
    }

    fn run_process(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
        argv: Vec<String>,
        cwd: PathBuf,
        stdin: Option<&[u8]>,
        timeout_ms: Option<u64>,
    ) -> Result<HeadlessCommandOutput, ToolExecutionError> {
        validate_headless_argv(&argv)?;
        let mut child = Command::new(&argv[0])
            .args(argv.iter().skip(1))
            .current_dir(&cwd)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_command_spawn_failed",
                    format!("Xero could not launch `{}`: {error}", argv[0]),
                )
            })?;

        if let Some(bytes) = stdin {
            let mut child_stdin = child.stdin.take().ok_or_else(|| {
                ToolExecutionError::retryable(
                    "agent_core_headless_command_stdin_missing",
                    "Xero could not open stdin for the command.",
                )
            })?;
            child_stdin.write_all(bytes).map_err(|error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_command_stdin_failed",
                    format!("Xero could not write command stdin: {error}"),
                )
            })?;
        }

        let stdout = child.stdout.take().ok_or_else(|| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stdout_missing",
                "Xero could not capture command stdout.",
            )
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stderr_missing",
                "Xero could not capture command stderr.",
            )
        })?;
        let stdout_handle =
            thread::spawn(move || read_limited_output(stdout, MAX_TOOL_OUTPUT_BYTES));
        let stderr_handle =
            thread::spawn(move || read_limited_output(stderr, MAX_TOOL_OUTPUT_BYTES));
        let started_at = Instant::now();
        let timeout =
            Duration::from_millis(timeout_ms.unwrap_or(DEFAULT_HEADLESS_COMMAND_TIMEOUT_MS));
        let mut timed_out = false;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if started_at.elapsed() >= timeout => {
                    timed_out = true;
                    let _ = child.kill();
                    break child.wait().map_err(|error| {
                        ToolExecutionError::retryable(
                            "agent_core_headless_command_wait_failed",
                            format!("Xero could not wait for a timed-out command: {error}"),
                        )
                    })?;
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = child.kill();
                    return Err(ToolExecutionError::retryable(
                        "agent_core_headless_command_wait_failed",
                        format!("Xero could not observe command execution: {error}"),
                    ));
                }
            }
        };
        let stdout = stdout_handle.join().map_err(|_| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stdout_failed",
                "Xero could not join stdout capture.",
            )
        })?;
        let stderr = stderr_handle.join().map_err(|_| {
            ToolExecutionError::retryable(
                "agent_core_headless_command_stderr_failed",
                "Xero could not join stderr capture.",
            )
        })?;

        Ok(HeadlessCommandOutput {
            argv,
            cwd: cwd.display().to_string(),
            stdout: Some(String::from_utf8_lossy(&stdout.bytes).into_owned()),
            stderr: Some(String::from_utf8_lossy(&stderr.bytes).into_owned()),
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            exit_code: status.code(),
            timed_out,
            elapsed_ms: started_at.elapsed().as_millis() as u64,
            context_epoch: context.context_epoch.clone(),
            tool_call_id: call.tool_call_id.clone(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct HeadlessProductionToolPolicy {
    allow_workspace_writes: bool,
}

impl ToolPolicy for HeadlessProductionToolPolicy {
    fn evaluate(&self, descriptor: &ToolDescriptorV2, _call: &ToolCallInput) -> ToolPolicyDecision {
        if descriptor.mutability == ToolMutability::Mutating && !self.allow_workspace_writes {
            return ToolPolicyDecision::Deny {
                code: "agent_core_headless_write_not_approved".into(),
                message: "Headless real-provider writes are disabled for this run.".into(),
            };
        }
        ToolPolicyDecision::Allow
    }
}

#[derive(Debug, Clone)]
struct HeadlessFileRollback {
    workspace_root: PathBuf,
}

impl ToolRollback for HeadlessFileRollback {
    fn checkpoint_before(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
    ) -> Result<Option<JsonValue>, ToolExecutionError> {
        if descriptor.name != HEADLESS_TOOL_WRITE {
            return Ok(None);
        }
        let path = required_tool_string(&call.input, "path")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, true)
            .map_err(core_error_to_tool_execution_error)?;
        let bytes = fs::read(&resolved).ok();
        Ok(Some(json!({
            "kind": "file_rollback_checkpoint",
            "path": path,
            "existed": bytes.is_some(),
            "contentBytes": bytes,
        })))
    }

    fn rollback_after_failure(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
        checkpoint: &JsonValue,
        error: &ToolExecutionError,
    ) -> Result<JsonValue, ToolExecutionError> {
        if descriptor.name != HEADLESS_TOOL_WRITE {
            return Ok(json!({ "kind": "rollback_not_required" }));
        }
        let path = required_tool_string(&call.input, "path")?;
        let resolved = resolve_workspace_path_for_root(&self.workspace_root, path, true)
            .map_err(core_error_to_tool_execution_error)?;
        let existed = checkpoint
            .get("existed")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false);
        if existed {
            let bytes = checkpoint
                .get("contentBytes")
                .and_then(JsonValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(JsonValue::as_u64)
                        .filter_map(|value| u8::try_from(value).ok())
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            fs::write(&resolved, bytes).map_err(|write_error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_rollback_failed",
                    format!("Xero could not restore `{path}` after tool failure: {write_error}"),
                )
            })?;
        } else if resolved.exists() {
            fs::remove_file(&resolved).map_err(|remove_error| {
                ToolExecutionError::retryable(
                    "agent_core_headless_rollback_failed",
                    format!(
                        "Xero could not remove newly-created `{path}` after tool failure: {remove_error}"
                    ),
                )
            })?;
        }
        Ok(json!({
            "kind": "file_rollback",
            "path": path,
            "restored": true,
            "triggerErrorCode": error.code,
        }))
    }
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatCompletionChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionChoice {
    message: JsonValue,
}

fn send_openai_compatible_chat(
    client: &Client,
    config: &OpenAiCompatibleHeadlessConfig,
    messages: &[JsonValue],
    tools: Vec<JsonValue>,
) -> CoreResult<OpenAiProviderMessage> {
    let url = openai_compatible_chat_url(&config.base_url)?;
    let body = json!({
        "model": config.model_id,
        "messages": messages,
        "tools": tools,
        "tool_choice": "auto",
        "stream": false,
    });
    let mut request = client.post(url).json(&body);
    if let Some(api_key) = config
        .api_key
        .as_deref()
        .filter(|key| !key.trim().is_empty())
    {
        request = request.bearer_auth(api_key);
    }
    let response = request.send().map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_request_failed",
            format!(
                "Headless provider `{}` request failed: {error}",
                config.provider_id
            ),
        )
    })?;
    let status = response.status();
    let text = response.text().map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_response_read_failed",
            format!("Xero could not read provider response body: {error}"),
        )
    })?;
    if !status.is_success() {
        return Err(CoreError::invalid_request(
            "agent_core_provider_status_failed",
            format!(
                "Headless provider `{}` returned HTTP {}: {}",
                config.provider_id,
                status.as_u16(),
                truncate_text(&text, 2048)
            ),
        ));
    }
    let decoded = serde_json::from_str::<ChatCompletionResponse>(&text).map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_response_decode_failed",
            format!("Xero could not decode provider response JSON: {error}"),
        )
    })?;
    let message = decoded
        .choices
        .into_iter()
        .next()
        .map(|choice| choice.message)
        .ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_provider_choice_missing",
                "Provider response did not include a completion choice.",
            )
        })?;
    Ok(OpenAiProviderMessage {
        content: message.get("content").cloned().unwrap_or(JsonValue::Null),
        tool_calls: parse_openai_tool_calls(&message)?,
    })
}

#[derive(Debug, Default)]
struct PartialOpenAiResponseToolCall {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

fn send_openai_codex_responses(
    client: &Client,
    config: &OpenAiCodexHeadlessConfig,
    messages: &[JsonValue],
    tools: Vec<JsonValue>,
) -> CoreResult<OpenAiProviderMessage> {
    let url = openai_codex_responses_url(&config.base_url)?;
    let mut body = json!({
        "model": config.model_id,
        "store": false,
        "stream": true,
        "instructions": openai_codex_instructions(messages),
        "input": openai_codex_response_input(messages)?,
        "text": { "verbosity": "medium" },
        "include": ["reasoning.encrypted_content"],
        "tool_choice": "auto",
        "parallel_tool_calls": true,
    });
    if !tools.is_empty() {
        body.as_object_mut()
            .expect("OpenAI Codex request body is an object")
            .insert("tools".into(), JsonValue::Array(tools));
    }
    let mut request = client
        .post(url)
        .bearer_auth(config.access_token.trim())
        .header("chatgpt-account-id", config.account_id.trim())
        .header("OpenAI-Beta", "responses=experimental")
        .header("originator", "pi")
        .header(
            "user-agent",
            format!("pi ({}; {})", std::env::consts::OS, std::env::consts::ARCH),
        )
        .header("accept", "text/event-stream")
        .json(&body);
    if let Some(session_id) = config
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|session_id| !session_id.is_empty())
    {
        request = request.header("session_id", session_id);
    }
    let response = request.send().map_err(|error| {
        CoreError::system_fault(
            "agent_core_provider_request_failed",
            format!(
                "Headless provider `{}` request failed: {error}",
                config.provider_id
            ),
        )
    })?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().unwrap_or_else(|_| String::new());
        return Err(CoreError::invalid_request(
            "agent_core_provider_status_failed",
            format!(
                "Headless provider `{}` returned HTTP {}: {}",
                config.provider_id,
                status.as_u16(),
                truncate_text(&text, 2048)
            ),
        ));
    }
    parse_openai_codex_responses_sse(&config.provider_id, response)
}

fn openai_codex_responses_url(base_url: &str) -> CoreResult<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_missing",
            "A provider base URL is required for headless OpenAI OAuth execution.",
        ));
    }
    if trimmed.starts_with("http://") && !is_local_http_endpoint(trimmed) {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_insecure",
            "Headless OpenAI OAuth HTTP endpoints are only allowed for localhost.",
        ));
    }
    let url = if trimmed.ends_with("/codex/responses") {
        trimmed.to_owned()
    } else if trimmed.ends_with("/codex") {
        format!("{trimmed}/responses")
    } else {
        format!("{trimmed}/codex/responses")
    };
    Ok(url)
}

fn openai_codex_instructions(messages: &[JsonValue]) -> String {
    messages
        .iter()
        .find(|message| message.get("role").and_then(JsonValue::as_str) == Some("system"))
        .and_then(|message| message.get("content").and_then(JsonValue::as_str))
        .unwrap_or_default()
        .to_string()
}

fn openai_codex_response_input(messages: &[JsonValue]) -> CoreResult<Vec<JsonValue>> {
    let mut input = Vec::new();
    for (index, message) in messages.iter().enumerate() {
        match message
            .get("role")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "system" => {}
            "user" => {
                let content = message
                    .get("content")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default();
                input.push(json!({
                    "role": "user",
                    "content": [{ "type": "input_text", "text": content }],
                }));
            }
            "assistant" => {
                if let Some(content) = message
                    .get("content")
                    .and_then(JsonValue::as_str)
                    .filter(|content| !content.trim().is_empty())
                {
                    input.push(json!({
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": content,
                            "annotations": [],
                        }],
                        "status": "completed",
                        "id": format!("msg_{index}"),
                    }));
                }
                for tool_call in message
                    .get("tool_calls")
                    .and_then(JsonValue::as_array)
                    .into_iter()
                    .flatten()
                {
                    let function = tool_call.get("function").unwrap_or(&JsonValue::Null);
                    input.push(json!({
                        "type": "function_call",
                        "call_id": tool_call.get("id").and_then(JsonValue::as_str).unwrap_or("call"),
                        "name": function.get("name").and_then(JsonValue::as_str).unwrap_or("unknown"),
                        "arguments": function.get("arguments").and_then(JsonValue::as_str).unwrap_or("{}"),
                    }));
                }
            }
            "tool" => {
                input.push(json!({
                    "type": "function_call_output",
                    "call_id": message
                        .get("tool_call_id")
                        .and_then(JsonValue::as_str)
                        .unwrap_or("call"),
                    "output": message
                        .get("content")
                        .and_then(JsonValue::as_str)
                        .unwrap_or_default(),
                }));
            }
            other => {
                return Err(CoreError::invalid_request(
                    "agent_core_provider_message_role_invalid",
                    format!("Cannot encode provider message role `{other}` for OpenAI OAuth."),
                ));
            }
        }
    }
    Ok(input)
}

fn parse_openai_codex_responses_sse(
    provider_id: &str,
    response: reqwest::blocking::Response,
) -> CoreResult<OpenAiProviderMessage> {
    let mut message = String::new();
    let mut partial_calls = BTreeMap::<usize, PartialOpenAiResponseToolCall>::new();
    let mut completed_call_count = 0_usize;
    for line in BufReader::new(response).lines() {
        let line = line.map_err(|error| {
            CoreError::system_fault(
                "agent_core_provider_stream_read_failed",
                format!("Xero lost the {provider_id} Responses stream: {error}"),
            )
        })?;
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let value: JsonValue = serde_json::from_str(data).map_err(|error| {
            CoreError::system_fault(
                "agent_core_provider_stream_decode_failed",
                format!("Xero could not decode a {provider_id} Responses chunk: {error}"),
            )
        })?;
        match value
            .get("type")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
        {
            "error" | "response.failed" => {
                return Err(CoreError::invalid_request(
                    "agent_core_provider_response_failed",
                    truncate_text(&value.to_string(), 2048),
                ));
            }
            "response.output_text.delta" => {
                if let Some(delta) = value.get("delta").and_then(JsonValue::as_str) {
                    message.push_str(delta);
                }
            }
            "response.function_call_arguments.delta" => {
                let index = value
                    .get("output_index")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or(completed_call_count as u64) as usize;
                if let Some(delta) = value.get("delta").and_then(JsonValue::as_str) {
                    partial_calls
                        .entry(index)
                        .or_default()
                        .arguments
                        .push_str(delta);
                }
            }
            "response.output_item.added" => {
                apply_openai_codex_function_call_item(
                    &mut partial_calls,
                    &value,
                    completed_call_count,
                );
            }
            "response.output_item.done" => {
                if apply_openai_codex_function_call_item(
                    &mut partial_calls,
                    &value,
                    completed_call_count,
                ) {
                    completed_call_count = completed_call_count.saturating_add(1);
                }
            }
            _ => {}
        }
    }
    let tool_calls = partial_calls
        .into_iter()
        .map(|(index, partial)| {
            let name = partial.name.ok_or_else(|| {
                CoreError::invalid_request(
                    "agent_core_provider_tool_name_missing",
                    format!(
                        "Xero received an OpenAI OAuth tool call at index {index} without a name."
                    ),
                )
            })?;
            let id = partial
                .id
                .unwrap_or_else(|| format!("{provider_id}-tool-call-{}", index + 1));
            let arguments = if partial.arguments.trim().is_empty() {
                JsonValue::Object(serde_json::Map::new())
            } else {
                serde_json::from_str(&partial.arguments).map_err(|error| {
                    CoreError::invalid_request(
                        "agent_core_provider_tool_arguments_invalid",
                        format!(
                            "Xero could not decode OpenAI OAuth tool call `{name}` arguments as JSON: {error}"
                        ),
                    )
                })?
            };
            Ok(OpenAiToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect::<CoreResult<Vec<_>>>()?;
    Ok(OpenAiProviderMessage {
        content: json!(message),
        tool_calls,
    })
}

fn apply_openai_codex_function_call_item(
    partial_calls: &mut BTreeMap<usize, PartialOpenAiResponseToolCall>,
    value: &JsonValue,
    fallback_index: usize,
) -> bool {
    let Some(item) = value.get("item") else {
        return false;
    };
    if item.get("type").and_then(JsonValue::as_str) != Some("function_call") {
        return false;
    }
    let index = value
        .get("output_index")
        .and_then(JsonValue::as_u64)
        .unwrap_or(fallback_index as u64) as usize;
    let partial = partial_calls.entry(index).or_default();
    if let Some(call_id) = item.get("call_id").and_then(JsonValue::as_str) {
        partial.id = Some(call_id.to_string());
    }
    if let Some(name) = item.get("name").and_then(JsonValue::as_str) {
        partial.name = Some(name.to_string());
    }
    if partial.arguments.is_empty() {
        if let Some(arguments) = item.get("arguments").and_then(JsonValue::as_str) {
            partial.arguments.push_str(arguments);
        }
    }
    true
}

fn parse_openai_tool_calls(message: &JsonValue) -> CoreResult<Vec<OpenAiToolCall>> {
    let Some(calls) = message.get("tool_calls").and_then(JsonValue::as_array) else {
        return Ok(Vec::new());
    };
    calls
        .iter()
        .map(|call| {
            let id = call
                .get("id")
                .and_then(JsonValue::as_str)
                .unwrap_or("call")
                .to_string();
            let function = call.get("function").ok_or_else(|| {
                CoreError::invalid_request(
                    "agent_core_provider_tool_call_invalid",
                    "Provider tool call was missing its function payload.",
                )
            })?;
            let name = function
                .get("name")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    CoreError::invalid_request(
                        "agent_core_provider_tool_name_missing",
                        "Provider tool call was missing a function name.",
                    )
                })?
                .to_string();
            let raw_arguments = function
                .get("arguments")
                .and_then(JsonValue::as_str)
                .unwrap_or("{}");
            let arguments = serde_json::from_str(raw_arguments).map_err(|error| {
                CoreError::invalid_request(
                    "agent_core_provider_tool_arguments_invalid",
                    format!("Provider tool call `{name}` arguments were not JSON: {error}"),
                )
            })?;
            Ok(OpenAiToolCall {
                id,
                name,
                arguments,
            })
        })
        .collect()
}

pub fn replayable_openai_chat_messages_from_snapshot(snapshot: &RunSnapshot) -> Vec<JsonValue> {
    chat_messages_from_snapshot(snapshot)
}

fn chat_messages_from_snapshot(snapshot: &RunSnapshot) -> Vec<JsonValue> {
    let mut messages = vec![json!({
        "role": "system",
        "content": headless_system_prompt(None),
    })];
    for message in &snapshot.messages {
        match message.role {
            MessageRole::System => {
                messages[0] = json!({ "role": "system", "content": message.content.clone() });
            }
            MessageRole::Developer | MessageRole::User => {
                messages.push(json!({ "role": "user", "content": message.content.clone() }));
            }
            MessageRole::Assistant => {
                messages.push(openai_assistant_message_from_runtime_message(message));
            }
            MessageRole::Tool => {
                messages.push(openai_tool_message_from_runtime_message(message));
            }
        }
    }
    messages
}

fn openai_assistant_message_from_runtime_message(message: &crate::RuntimeMessage) -> JsonValue {
    let Some(metadata) = message.provider_metadata.as_ref() else {
        return json!({ "role": "assistant", "content": message.content.clone() });
    };
    if metadata.assistant_tool_calls.is_empty() {
        return json!({ "role": "assistant", "content": message.content.clone() });
    }
    json!({
        "role": "assistant",
        "content": if message.content.is_empty() { JsonValue::Null } else { json!(message.content.clone()) },
        "tool_calls": metadata.assistant_tool_calls.iter().map(|call| {
            json!({
                "id": call.tool_call_id,
                "type": "function",
                "function": {
                    "name": call.provider_tool_name,
                    "arguments": call.arguments.to_string(),
                }
            })
        }).collect::<Vec<_>>(),
    })
}

fn openai_tool_message_from_runtime_message(message: &crate::RuntimeMessage) -> JsonValue {
    if let Some(tool_result) = message
        .provider_metadata
        .as_ref()
        .and_then(|metadata| metadata.tool_result.as_ref())
    {
        return json!({
            "role": "tool",
            "tool_call_id": tool_result.tool_call_id,
            "content": message.content.clone(),
        });
    }

    let parsed = serde_json::from_str::<JsonValue>(&message.content).unwrap_or(JsonValue::Null);
    if let Some(tool_call_id) = parsed
        .get("toolCallId")
        .or_else(|| parsed.get("tool_call_id"))
        .and_then(JsonValue::as_str)
    {
        return json!({
            "role": "tool",
            "tool_call_id": tool_call_id,
            "content": message.content.clone(),
        });
    }

    json!({ "role": "tool", "content": message.content.clone() })
}

fn openai_assistant_message(content: &str, tool_calls: &[OpenAiToolCall]) -> JsonValue {
    json!({
        "role": "assistant",
        "content": if content.is_empty() { JsonValue::Null } else { json!(content) },
        "tool_calls": tool_calls.iter().map(|call| {
            json!({
                "id": call.id,
                "type": "function",
                "function": {
                    "name": call.name,
                    "arguments": call.arguments.to_string(),
                }
            })
        }).collect::<Vec<_>>(),
    })
}

fn headless_read_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_READ.into(),
        description: "Read a UTF-8 text file from the registered workspace.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "required": ["path"],
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into()],
        application_metadata: ToolApplicationMetadata::granular("file"),
        effect_class: ToolEffectClass::FileRead,
        mutability: ToolMutability::ReadOnly,
        sandbox_requirement: ToolSandboxRequirement::ReadOnly,
        approval_requirement: ToolApprovalRequirement::Never,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_read".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_list_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_LIST.into(),
        description: "List files below a directory in the registered workspace.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" }
            },
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into()],
        application_metadata: ToolApplicationMetadata {
            family: "search".into(),
            kind: ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec!["bounded_results".into(), "read_only".into()],
        },
        effect_class: ToolEffectClass::Search,
        mutability: ToolMutability::ReadOnly,
        sandbox_requirement: ToolSandboxRequirement::ReadOnly,
        approval_requirement: ToolApprovalRequirement::Never,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_list".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_write_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_WRITE.into(),
        description: "Write a UTF-8 text file inside the registered workspace.".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "content": { "type": "string" }
            },
            "required": ["path", "content"],
            "additionalProperties": false
        }),
        capability_tags: vec!["workspace".into(), "filesystem".into(), "mutation".into()],
        application_metadata: ToolApplicationMetadata::granular("edit"),
        effect_class: ToolEffectClass::WorkspaceMutation,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_file_write".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_patch_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_PATCH.into(),
        description: "Apply a unified diff patch inside the registered workspace using git apply."
            .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "patch": { "type": "string" }
            },
            "required": ["patch"],
            "additionalProperties": false
        }),
        capability_tags: vec![
            "workspace".into(),
            "filesystem".into(),
            "patch".into(),
            "mutation".into(),
        ],
        application_metadata: ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::Declarative,
            dispatch_safety: ToolBatchDispatchSafety::ToolOwnedAtomic,
            safety_requirements: vec![
                "supports_preview".into(),
                "validates_targets_before_writing".into(),
                "reports_diff".into(),
            ],
        },
        effect_class: ToolEffectClass::WorkspaceMutation,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_patch_apply".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn headless_command_descriptor() -> ToolDescriptorV2 {
    ToolDescriptorV2 {
        name: HEADLESS_TOOL_COMMAND.into(),
        description:
            "Run a bounded command in the registered workspace under Xero's benchmark policy."
                .into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "argv": {
                    "type": "array",
                    "items": { "type": "string" },
                    "minItems": 1
                },
                "cwd": { "type": "string" },
                "timeoutMs": {
                    "type": "integer",
                    "minimum": 1000,
                    "maximum": MAX_HEADLESS_COMMAND_TIMEOUT_MS
                }
            },
            "required": ["argv"],
            "additionalProperties": false
        }),
        capability_tags: vec![
            "workspace".into(),
            "command".into(),
            "terminal_bench".into(),
        ],
        application_metadata: ToolApplicationMetadata::granular("command"),
        effect_class: ToolEffectClass::CommandExecution,
        mutability: ToolMutability::Mutating,
        sandbox_requirement: ToolSandboxRequirement::WorkspaceWrite,
        approval_requirement: ToolApprovalRequirement::Policy,
        telemetry_attributes: BTreeMap::from([
            ("xero.tool.kind".into(), "workspace_command".into()),
            ("xero.tool.registry".into(), "tool_registry_v2".into()),
        ]),
        result_truncation: ToolResultTruncationContract {
            max_output_bytes: MAX_TOOL_OUTPUT_BYTES,
            preserve_json_shape: false,
        },
    }
}

fn openai_tool_definition_from_descriptor(descriptor: ToolDescriptorV2) -> JsonValue {
    json!({
        "type": "function",
        "function": {
            "name": descriptor.name,
            "description": descriptor.description,
            "parameters": descriptor.input_schema,
        }
    })
}

fn openai_response_tool_definition_from_descriptor(descriptor: ToolDescriptorV2) -> JsonValue {
    json!({
        "type": "function",
        "name": descriptor.name,
        "description": descriptor.description,
        "parameters": descriptor.input_schema,
        "strict": JsonValue::Null,
    })
}

fn redacted_headless_tool_input(tool_name: &str, input: &JsonValue) -> (JsonValue, bool) {
    let Some(object) = input.as_object() else {
        return (input.clone(), false);
    };
    let mut redacted = object.clone();
    match tool_name {
        HEADLESS_TOOL_WRITE => {
            if let Some(content) = object.get("content").and_then(JsonValue::as_str) {
                redacted.insert(
                    "content".into(),
                    json!({
                        "redacted": true,
                        "bytes": content.len(),
                    }),
                );
                return (JsonValue::Object(redacted), true);
            }
        }
        HEADLESS_TOOL_PATCH => {
            if let Some(patch) = object.get("patch").and_then(JsonValue::as_str) {
                redacted.insert(
                    "patch".into(),
                    json!({
                        "redacted": true,
                        "bytes": patch.len(),
                        "changedFiles": patch_changed_paths(patch),
                    }),
                );
                return (JsonValue::Object(redacted), true);
            }
        }
        _ => {}
    }
    (JsonValue::Object(redacted), false)
}

fn headless_dispatch_success_metadata(
    success: &ToolDispatchSuccess,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "providerLoop": "headless_production_provider_loop",
        "groupMode": group_mode,
        "groupElapsedMs": group_elapsed_ms,
        "elapsedMs": success.elapsed_ms,
        "truncation": success.truncation.clone(),
        "sandbox": success.sandbox_metadata.clone(),
        "telemetry": success.telemetry_attributes.clone(),
        "preHook": success.pre_hook_payload.clone(),
        "postHook": success.post_hook_payload.clone(),
        "fileReservation": success.output.get("fileReservation").cloned(),
        "rollback": success.output.get("rollback").cloned(),
        "timeout": timeout_error.map(tool_execution_error_json),
    })
}

fn headless_dispatch_failure_metadata(
    failure: &ToolDispatchFailure,
    group_mode: ToolGroupExecutionMode,
    group_elapsed_ms: u128,
    timeout_error: Option<&ToolExecutionError>,
) -> JsonValue {
    json!({
        "registryVersion": "tool_registry_v2",
        "providerLoop": "headless_production_provider_loop",
        "groupMode": group_mode,
        "groupElapsedMs": group_elapsed_ms,
        "elapsedMs": failure.elapsed_ms,
        "typedErrorCategory": failure.error.category.clone(),
        "modelMessage": failure.error.model_message.clone(),
        "retryable": failure.error.retryable,
        "doomLoopSignal": failure.doom_loop_signal.clone(),
        "rollbackPayload": failure.rollback_payload.clone(),
        "rollbackError": failure.rollback_error.as_ref().map(tool_execution_error_json),
        "sandbox": failure.sandbox_metadata.clone(),
        "preHook": failure.pre_hook_payload.clone(),
        "postHook": failure.post_hook_payload.clone(),
        "timeout": timeout_error.map(tool_execution_error_json),
    })
}

fn tool_execution_error_json(error: &ToolExecutionError) -> JsonValue {
    json!({
        "category": &error.category,
        "code": &error.code,
        "message": &error.message,
        "modelMessage": &error.model_message,
        "retryable": error.retryable,
    })
}

fn required_tool_string<'a>(
    input: &'a JsonValue,
    key: &str,
) -> Result<&'a str, ToolExecutionError> {
    input
        .get(key)
        .and_then(JsonValue::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ToolExecutionError::invalid_input(
                "agent_core_headless_tool_argument_missing",
                format!("Tool argument `{key}` is required."),
            )
        })
}

fn required_tool_string_array(
    input: &JsonValue,
    key: &str,
) -> Result<Vec<String>, ToolExecutionError> {
    let values = input
        .get(key)
        .and_then(JsonValue::as_array)
        .ok_or_else(|| {
            ToolExecutionError::invalid_input(
                "agent_core_headless_tool_argument_missing",
                format!("Tool argument `{key}` must be a non-empty string array."),
            )
        })?;
    let values = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .filter(|text| !text.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    ToolExecutionError::invalid_input(
                        "agent_core_headless_tool_argument_invalid",
                        format!("Tool argument `{key}` must contain only non-empty strings."),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() {
        return Err(ToolExecutionError::invalid_input(
            "agent_core_headless_tool_argument_invalid",
            format!("Tool argument `{key}` must contain at least one value."),
        ));
    }
    Ok(values)
}

fn validate_headless_argv(argv: &[String]) -> Result<(), ToolExecutionError> {
    if argv.is_empty() || argv[0].trim().is_empty() {
        return Err(ToolExecutionError::invalid_input(
            "agent_core_headless_command_argv_invalid",
            "Command argv must include a program name.",
        ));
    }
    if argv.iter().any(|part| part.contains('\0')) {
        return Err(ToolExecutionError::invalid_input(
            "agent_core_headless_command_argv_invalid",
            "Command argv cannot contain NUL bytes.",
        ));
    }
    Ok(())
}

fn read_limited_output<R>(mut reader: R, limit: usize) -> LimitedOutput
where
    R: Read,
{
    let mut bytes = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let Ok(read) = reader.read(&mut buffer) else {
            break;
        };
        if read == 0 {
            break;
        }
        let remaining = limit.saturating_sub(bytes.len());
        if remaining > 0 {
            let take = read.min(remaining);
            bytes.extend_from_slice(&buffer[..take]);
        }
        if read > remaining {
            truncated = true;
        }
    }
    LimitedOutput { bytes, truncated }
}

fn patch_changed_paths(patch: &str) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for line in patch.lines() {
        let candidate = line
            .strip_prefix("+++ b/")
            .or_else(|| line.strip_prefix("--- a/"))
            .or_else(|| {
                line.strip_prefix("diff --git a/").and_then(|rest| {
                    rest.split_once(" b/")
                        .map(|(_left, right)| right.split_whitespace().next().unwrap_or(right))
                })
            });
        if let Some(path) = candidate {
            let path = path.trim();
            if !path.is_empty() && path != "/dev/null" {
                paths.insert(path.to_owned());
            }
        }
    }
    paths.into_iter().collect()
}

fn headless_tool_is_command(tool_name: &str) -> bool {
    tool_name == HEADLESS_TOOL_COMMAND
}

fn rollback_checkpoint_metadata(path: &str, resolved: &Path) -> JsonValue {
    match fs::read(resolved) {
        Ok(bytes) => json!({
            "kind": "file_rollback",
            "path": path,
            "existed": true,
            "bytes": bytes.len(),
            "stableHash": stable_bytes_hash(&bytes),
            "contentRedacted": true,
        }),
        Err(_) => json!({
            "kind": "file_rollback",
            "path": path,
            "existed": false,
            "bytes": 0,
            "stableHash": JsonValue::Null,
            "contentRedacted": true,
        }),
    }
}

fn file_reservation_metadata(
    context: &ToolExecutionContext,
    call: &ToolCallInput,
    path: &str,
) -> JsonValue {
    json!({
        "kind": "file_reservation",
        "reservationId": format!("reservation-{}-{}", context.run_id, call.tool_call_id),
        "ownerRunId": context.run_id,
        "toolCallId": call.tool_call_id,
        "path": path,
        "operation": "writing",
        "status": "claimed_for_turn",
        "conflictPolicy": "deny_without_override",
    })
}

fn core_error_to_tool_execution_error(error: CoreError) -> ToolExecutionError {
    if error.code.contains("denied") || error.code.contains("protected") {
        return ToolExecutionError::policy_denied(error.code, error.message);
    }
    if error.code.contains("missing")
        || error.code.contains("invalid")
        || error.code.contains("unavailable")
    {
        return ToolExecutionError::invalid_input(error.code, error.message);
    }
    ToolExecutionError::retryable(error.code, error.message)
}

fn tool_execution_error_to_core_error(error: ToolExecutionError) -> CoreError {
    CoreError::system_fault(error.code, error.message)
}

fn headless_system_prompt(workspace_root: Option<&Path>) -> String {
    let workspace = workspace_root
        .map(|root| root.display().to_string())
        .unwrap_or_else(|| "the configured workspace".into());
    format!(
        "You are Xero's headless owned-agent runtime. Use the production Tool Registry V2 tools when you need to inspect, edit, patch, or verify files in {workspace}: `read`, `list`, and, when available, `write`, `patch`, and `command`. Keep task deliverables inside the workspace, never touch .git or .xero, and use scratch locations such as /tmp for build outputs, compiled binaries, downloaded helpers, and verification debris when the command runner allows it. Before probing fragile recovery inputs, copy them first and work from the copies when possible. Before finishing, remove temporary files you created inside the workspace and leave only task-requested deliverables. Avoid network access unless the benchmark policy explicitly allows it, and finish with a concise summary."
    )
}

fn openai_compatible_chat_url(base_url: &str) -> CoreResult<String> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_missing",
            "A provider base URL is required for headless real-provider execution.",
        ));
    }
    if trimmed.starts_with("http://") && !is_local_http_endpoint(trimmed) {
        return Err(CoreError::invalid_request(
            "agent_core_provider_base_url_insecure",
            "Headless real-provider HTTP endpoints are only allowed for localhost.",
        ));
    }
    let url = if trimmed.ends_with("/chat/completions") {
        trimmed.to_owned()
    } else {
        format!("{trimmed}/chat/completions")
    };
    Ok(url)
}

fn is_local_http_endpoint(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    lower.starts_with("http://localhost")
        || lower.starts_with("http://127.")
        || lower.starts_with("http://[::1]")
        || lower.starts_with("http://0.0.0.0")
}

fn normalize_timeout(timeout_ms: u64) -> u64 {
    if timeout_ms == 0 {
        DEFAULT_HEADLESS_PROVIDER_TIMEOUT_MS
    } else {
        timeout_ms
    }
}

fn resolve_workspace_path_for_root(
    root: &Path,
    requested: &str,
    allow_missing_leaf: bool,
) -> CoreResult<PathBuf> {
    let root = fs::canonicalize(root).map_err(|error| {
        CoreError::invalid_request(
            "agent_core_headless_workspace_unavailable",
            format!(
                "Workspace root `{}` is unavailable: {error}",
                root.display()
            ),
        )
    })?;
    let requested_path = Path::new(requested);
    if requested_path
        .components()
        .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err(CoreError::invalid_request(
            "agent_core_headless_path_denied",
            format!("Path `{requested}` must stay inside the workspace."),
        ));
    }
    let joined = if requested_path.is_absolute() {
        requested_path.to_path_buf()
    } else {
        root.join(requested_path)
    };
    if requested_path
        .components()
        .any(|component| match component {
            Component::Normal(value) => value
                .to_str()
                .is_some_and(|part| matches!(part, ".git" | ".xero")),
            _ => false,
        })
    {
        return Err(CoreError::invalid_request(
            "agent_core_headless_path_protected",
            format!("Path `{requested}` targets protected workspace state."),
        ));
    }
    let check_path = if allow_missing_leaf {
        let mut candidate = joined.parent().unwrap_or(root.as_path()).to_path_buf();
        while !candidate.exists() && candidate != root {
            candidate = candidate.parent().unwrap_or(root.as_path()).to_path_buf();
        }
        candidate
    } else {
        joined.clone()
    };
    let canonical_check = fs::canonicalize(&check_path).map_err(|error| {
        CoreError::invalid_request(
            "agent_core_headless_path_unavailable",
            format!("Path `{requested}` could not be resolved: {error}"),
        )
    })?;
    if !canonical_check.starts_with(&root) {
        return Err(CoreError::invalid_request(
            "agent_core_headless_path_denied",
            format!("Path `{requested}` escapes the approved workspace."),
        ));
    }
    Ok(joined)
}

fn collect_workspace_files(
    root: &Path,
    dir: &Path,
    out: &mut Vec<String>,
    limit: usize,
) -> CoreResult<()> {
    if out.len() >= limit {
        return Ok(());
    }
    for entry in fs::read_dir(dir).map_err(|error| {
        CoreError::invalid_request(
            "agent_core_headless_list_failed",
            format!("Xero could not list `{}`: {error}", dir.display()),
        )
    })? {
        if out.len() >= limit {
            break;
        }
        let entry = entry.map_err(|error| {
            CoreError::invalid_request(
                "agent_core_headless_list_failed",
                format!("Xero could not read a directory entry: {error}"),
            )
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), ".git" | ".xero" | "target" | "node_modules") {
            continue;
        }
        if path.is_dir() {
            collect_workspace_files(root, &path, out, limit)?;
        } else if let Ok(relative) = path.strip_prefix(root) {
            out.push(relative.to_string_lossy().to_string());
        }
    }
    Ok(())
}

fn compact_summary_from_snapshot(snapshot: &RunSnapshot) -> String {
    snapshot
        .messages
        .iter()
        .rev()
        .take(12)
        .rev()
        .map(|message| {
            format!(
                "{:?}: {}",
                message.role,
                truncate_text(message.content.as_str(), 500)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn headless_context_hash(snapshot: &RunSnapshot, turn_index: usize) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!(
        "{}:{}:{}:{}:{}",
        snapshot.project_id,
        snapshot.agent_session_id,
        snapshot.run_id,
        snapshot.prompt,
        turn_index
    )
    .bytes()
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn stable_provider_preflight_hash(snapshot: &ProviderPreflightSnapshot) -> String {
    let serialized = serde_json::to_string(snapshot).unwrap_or_else(|_| "unserializable".into());
    crate::runtime_trace_id("provider-preflight", &[&serialized])
}

fn stable_bytes_hash(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn generate_headless_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    format!("{prefix}-{millis}-{}", std::process::id())
}

fn truncate_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.into();
    }
    let mut end = max_bytes.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &value[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headless_workspace_path_accepts_absolute_paths_inside_root() {
        let root = unique_test_dir("headless-absolute-root");
        fs::create_dir_all(root.join("logs")).expect("create logs dir");

        assert_eq!(
            resolve_workspace_path_for_root(&root, &root.display().to_string(), false)
                .expect("absolute root should resolve"),
            root.clone()
        );
        assert_eq!(
            resolve_workspace_path_for_root(
                &root,
                &root.join("logs").display().to_string(),
                false
            )
            .expect("absolute child should resolve"),
            root.join("logs")
        );
    }

    #[test]
    fn headless_workspace_path_rejects_absolute_paths_outside_root() {
        let root = unique_test_dir("headless-absolute-root-deny");
        let outside = root
            .parent()
            .expect("test root has parent")
            .join("outside-file");

        let error = resolve_workspace_path_for_root(&root, &outside.display().to_string(), true)
            .expect_err("absolute outside path should be denied");

        assert_eq!(error.code, "agent_core_headless_path_denied");
    }

    #[test]
    fn headless_workspace_path_protects_absolute_legacy_state() {
        let root = unique_test_dir("headless-absolute-protected");
        fs::create_dir_all(root.join(".xero")).expect("create legacy state dir");

        let error =
            resolve_workspace_path_for_root(&root, &root.join(".xero").display().to_string(), false)
                .expect_err("absolute protected state should be denied");

        assert_eq!(error.code, "agent_core_headless_path_protected");
    }

    #[test]
    fn headless_system_prompt_preserves_benchmark_workspace_hygiene() {
        let prompt = headless_system_prompt(Some(Path::new("/app")));

        assert!(prompt.contains("/tmp"));
        assert!(prompt.contains("Before probing fragile recovery inputs"));
        assert!(prompt.contains("remove temporary files"));
        assert!(prompt.contains("leave only task-requested deliverables"));
    }

    #[test]
    fn openai_codex_responses_url_normalizes_backend_base() {
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api").expect("url"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api/codex").expect("url"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert_eq!(
            openai_codex_responses_url("https://chatgpt.com/backend-api/codex/responses")
                .expect("url"),
            "https://chatgpt.com/backend-api/codex/responses"
        );
        assert!(openai_codex_responses_url("http://example.com").is_err());
    }

    #[test]
    fn openai_codex_response_input_encodes_tool_round_trip() {
        let input = openai_codex_response_input(&[
            json!({"role": "system", "content": "system instructions"}),
            json!({"role": "user", "content": "fix the file"}),
            json!({
                "role": "assistant",
                "content": "I will inspect it.",
                "tool_calls": [{
                    "id": "call-1",
                    "function": {
                        "name": "read_file",
                        "arguments": "{\"path\":\"README.md\"}"
                    }
                }]
            }),
            json!({"role": "tool", "tool_call_id": "call-1", "content": "contents"}),
        ])
        .expect("input");

        assert_eq!(input.len(), 4);
        assert_eq!(input[0]["content"][0]["type"], json!("input_text"));
        assert_eq!(input[1]["type"], json!("message"));
        assert_eq!(input[2]["type"], json!("function_call"));
        assert_eq!(input[2]["call_id"], json!("call-1"));
        assert_eq!(input[3]["type"], json!("function_call_output"));
        assert_eq!(input[3]["output"], json!("contents"));
    }

    fn unique_test_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let path = std::env::temp_dir().join(format!("xero-{label}-{nanos}"));
        fs::create_dir_all(&path).expect("create temp workspace");
        fs::canonicalize(path).expect("canonical temp workspace")
    }
}
