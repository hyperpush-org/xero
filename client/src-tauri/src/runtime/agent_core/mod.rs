use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

mod events;
mod provider_adapters;
mod supervisor;

pub use events::{publish_agent_event, subscribe_agent_events, AgentEventSubscription};
pub use provider_adapters::{
    create_provider_adapter, AgentProviderConfig, AnthropicProviderConfig, BedrockProviderConfig,
    OpenAiCompatibleProviderConfig, OpenAiResponsesProviderConfig, VertexProviderConfig,
};
pub use supervisor::{
    cancelled_error, AgentRunCancellationToken, AgentRunLease, AgentRunSupervisor,
    AGENT_RUN_CANCELLED_CODE,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use crate::{
    auth::now_timestamp,
    commands::{
        CommandError, CommandErrorClass, CommandResult, RuntimeRunActiveControlSnapshotDto,
        RuntimeRunApprovalModeDto, RuntimeRunControlInputDto, RuntimeRunControlStateDto,
    },
    db::project_store::{
        self, AgentEventRecord, AgentMessageRecord, AgentMessageRole, AgentRunEventKind,
        AgentRunSnapshotRecord, AgentRunStatus, AgentToolCallFinishRecord,
        AgentToolCallStartRecord, AgentToolCallState, NewAgentActionRequestRecord,
        NewAgentCheckpointRecord, NewAgentEventRecord, NewAgentFileChangeRecord,
        NewAgentMessageRecord, NewAgentRunRecord,
    },
    runtime::{
        autonomous_tool_runtime::{
            emulator::emulator_schema, tool_access_all_known_tools, tool_access_group_tools,
            AUTONOMOUS_TOOL_BROWSER, AUTONOMOUS_TOOL_EMULATOR, AUTONOMOUS_TOOL_SOLANA_ALT,
            AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE, AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
            AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ, AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
            AUTONOMOUS_TOOL_SOLANA_CLUSTER, AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
            AUTONOMOUS_TOOL_SOLANA_CODAMA, AUTONOMOUS_TOOL_SOLANA_COST,
            AUTONOMOUS_TOOL_SOLANA_DEPLOY, AUTONOMOUS_TOOL_SOLANA_DOCS,
            AUTONOMOUS_TOOL_SOLANA_EXPLAIN, AUTONOMOUS_TOOL_SOLANA_IDL,
            AUTONOMOUS_TOOL_SOLANA_INDEXER, AUTONOMOUS_TOOL_SOLANA_LOGS,
            AUTONOMOUS_TOOL_SOLANA_PDA, AUTONOMOUS_TOOL_SOLANA_PROGRAM,
            AUTONOMOUS_TOOL_SOLANA_REPLAY, AUTONOMOUS_TOOL_SOLANA_SECRETS,
            AUTONOMOUS_TOOL_SOLANA_SIMULATE, AUTONOMOUS_TOOL_SOLANA_SQUADS,
            AUTONOMOUS_TOOL_SOLANA_TX, AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
            AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
        },
        redaction::{find_prohibited_persistence_content, redact_command_argv_for_persistence},
        AutonomousSubagentExecutor, AutonomousSubagentTask, AutonomousToolOutput,
        AutonomousToolRequest, AutonomousToolResult, AutonomousToolRuntime,
        AUTONOMOUS_TOOL_CODE_INTEL, AUTONOMOUS_TOOL_COMMAND, AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
        AUTONOMOUS_TOOL_COMMAND_SESSION_START, AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
        AUTONOMOUS_TOOL_DELETE, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_FIND,
        AUTONOMOUS_TOOL_GIT_DIFF, AUTONOMOUS_TOOL_GIT_STATUS, AUTONOMOUS_TOOL_HASH,
        AUTONOMOUS_TOOL_LIST, AUTONOMOUS_TOOL_LSP, AUTONOMOUS_TOOL_MCP, AUTONOMOUS_TOOL_MKDIR,
        AUTONOMOUS_TOOL_NOTEBOOK_EDIT, AUTONOMOUS_TOOL_PATCH, AUTONOMOUS_TOOL_POWERSHELL,
        AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_RENAME, AUTONOMOUS_TOOL_SEARCH,
        AUTONOMOUS_TOOL_SUBAGENT, AUTONOMOUS_TOOL_TODO, AUTONOMOUS_TOOL_TOOL_ACCESS,
        AUTONOMOUS_TOOL_TOOL_SEARCH, AUTONOMOUS_TOOL_WEB_FETCH, AUTONOMOUS_TOOL_WEB_SEARCH,
        AUTONOMOUS_TOOL_WRITE, OPENAI_CODEX_PROVIDER_ID,
    },
};

pub const OWNED_AGENT_SUPERVISOR_KIND: &str = "owned_agent";
pub const OWNED_AGENT_RUNTIME_KIND: &str = "owned_agent";
pub const FAKE_PROVIDER_ID: &str = "fake_provider";
const SYSTEM_PROMPT_VERSION: &str = "cadence-owned-agent-v1";
const MAX_PROVIDER_TURNS: usize = 32;
const MAX_ROLLBACK_CONTENT_BYTES: u64 = 256 * 1024;
const INTERRUPTED_TOOL_CALL_CODE: &str = "agent_tool_call_interrupted";
const RERUNNABLE_APPROVED_TOOL_ERROR_CODES: &[&str] = &[
    "agent_file_write_requires_observation",
    "agent_file_changed_since_observed",
];

#[derive(Debug, Clone)]
pub struct OwnedAgentRunRequest {
    pub repo_root: PathBuf,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub prompt: String,
    pub controls: Option<RuntimeRunControlInputDto>,
    pub tool_runtime: AutonomousToolRuntime,
    pub provider_config: AgentProviderConfig,
}

#[derive(Debug, Clone)]
pub struct ContinueOwnedAgentRunRequest {
    pub repo_root: PathBuf,
    pub project_id: String,
    pub run_id: String,
    pub prompt: String,
    pub controls: Option<RuntimeRunControlInputDto>,
    pub tool_runtime: AutonomousToolRuntime,
    pub provider_config: AgentProviderConfig,
    pub answer_pending_actions: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolRegistry {
    descriptors: Vec<AgentToolDescriptor>,
}

impl ToolRegistry {
    pub fn builtin() -> Self {
        Self {
            descriptors: builtin_tool_descriptors(),
        }
    }

    pub fn for_prompt(
        repo_root: &Path,
        prompt: &str,
        controls: &RuntimeRunControlStateDto,
    ) -> Self {
        Self::for_tool_names(select_tool_names_for_prompt(repo_root, prompt, controls))
    }

    pub fn for_tool_names(tool_names: BTreeSet<String>) -> Self {
        let descriptors = builtin_tool_descriptors()
            .into_iter()
            .filter(|descriptor| tool_names.contains(descriptor.name.as_str()))
            .collect();
        Self { descriptors }
    }

    pub fn descriptors(&self) -> &[AgentToolDescriptor] {
        &self.descriptors
    }

    pub fn into_descriptors(self) -> Vec<AgentToolDescriptor> {
        self.descriptors
    }

    pub fn descriptor(&self, name: &str) -> Option<&AgentToolDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.name == name)
    }

    pub fn descriptor_names(&self) -> BTreeSet<String> {
        self.descriptors
            .iter()
            .map(|descriptor| descriptor.name.clone())
            .collect()
    }

    pub fn expand_with_tool_names<I, S>(&mut self, tool_names: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut names = self.descriptor_names();
        for tool_name in tool_names {
            names.insert(tool_name.as_ref().to_owned());
        }
        *self = Self::for_tool_names(names);
    }

    pub fn decode_call(&self, tool_call: &AgentToolCall) -> CommandResult<AutonomousToolRequest> {
        if self.descriptor(&tool_call.tool_name).is_none() {
            return Err(CommandError::user_fixable(
                "agent_tool_call_unknown",
                format!(
                    "The owned-agent model requested unregistered tool `{}`.",
                    tool_call.tool_name
                ),
            ));
        }

        let request_value = json!({
            "tool": tool_call.tool_name,
            "input": tool_call.input,
        });
        serde_json::from_value::<AutonomousToolRequest>(request_value).map_err(|error| {
            CommandError::user_fixable(
                "agent_tool_call_invalid",
                format!(
                    "Cadence could not decode owned-agent tool call `{}` for `{}`: {error}",
                    tool_call.tool_call_id, tool_call.tool_name
                ),
            )
        })
    }

    pub fn validate_call(&self, tool_call: &AgentToolCall) -> CommandResult<()> {
        self.decode_call(tool_call).map(|_| ())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolCall {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolResult {
    pub tool_call_id: String,
    pub tool_name: String,
    pub ok: bool,
    pub summary: String,
    pub output: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AgentSafetyDecision {
    Allow { reason: String },
    RequireApproval { reason: String },
    Deny { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderStreamEvent {
    MessageDelta(String),
    ReasoningSummary(String),
    ToolDelta {
        tool_call_id: Option<String>,
        tool_name: Option<String>,
        arguments_delta: String,
    },
    Usage(ProviderUsage),
}

pub trait ProviderAdapter {
    fn provider_id(&self) -> &str;
    fn model_id(&self) -> &str;
    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome>;
}

#[derive(Debug, Clone)]
pub struct ProviderTurnRequest {
    pub system_prompt: String,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<AgentToolDescriptor>,
    pub turn_index: usize,
    pub controls: RuntimeRunControlStateDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "role")]
pub enum ProviderMessage {
    User {
        content: String,
    },
    Assistant {
        content: String,
        tool_calls: Vec<AgentToolCall>,
    },
    Tool {
        tool_call_id: String,
        tool_name: String,
        content: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderTurnOutcome {
    Complete {
        message: String,
        usage: Option<ProviderUsage>,
    },
    ToolCalls {
        message: String,
        tool_calls: Vec<AgentToolCall>,
        usage: Option<ProviderUsage>,
    },
}

#[derive(Debug, Clone, Copy)]
pub struct FakeProviderAdapter;

impl ProviderAdapter for FakeProviderAdapter {
    fn provider_id(&self) -> &str {
        OPENAI_CODEX_PROVIDER_ID
    }

    fn model_id(&self) -> &str {
        OPENAI_CODEX_PROVIDER_ID
    }

    fn stream_turn(
        &self,
        request: &ProviderTurnRequest,
        emit: &mut dyn FnMut(ProviderStreamEvent) -> CommandResult<()>,
    ) -> CommandResult<ProviderTurnOutcome> {
        emit(ProviderStreamEvent::ReasoningSummary(format!(
            "Loaded {} owned tool descriptor(s) under {}.",
            request.tools.len(),
            SYSTEM_PROMPT_VERSION
        )))?;

        if request
            .messages
            .iter()
            .any(|message| matches!(message, ProviderMessage::Tool { .. }))
        {
            let message =
                "Owned agent run completed through the Cadence model-loop scaffold.".to_string();
            emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
            return Ok(ProviderTurnOutcome::Complete {
                message,
                usage: Some(ProviderUsage::default()),
            });
        }

        let user_prompt = request
            .messages
            .iter()
            .find_map(|message| match message {
                ProviderMessage::User { content } => Some(content.as_str()),
                _ => None,
            })
            .unwrap_or_default();
        let tool_calls = parse_fake_tool_directives(user_prompt);
        let message = "Cadence owned-agent runtime accepted the task.".to_string();
        emit(ProviderStreamEvent::MessageDelta(message.clone()))?;
        if tool_calls.is_empty() {
            Ok(ProviderTurnOutcome::Complete {
                message,
                usage: Some(ProviderUsage::default()),
            })
        } else {
            Ok(ProviderTurnOutcome::ToolCalls {
                message,
                tool_calls,
                usage: Some(ProviderUsage::default()),
            })
        }
    }
}

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
    let tool_registry = ToolRegistry::for_prompt(&request.repo_root, &request.prompt, &controls);
    let system_prompt = assemble_system_prompt(&request.repo_root, tool_registry.descriptors())?;
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
    let base_tool_runtime = request
        .tool_runtime
        .with_runtime_run_controls(controls.clone())
        .with_cancellation_token(cancellation.clone());
    let tool_registry = tool_registry_for_snapshot(&request.repo_root, &snapshot, &controls)?;
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
    let messages = provider_messages_from_snapshot(&snapshot)?;

    match drive_provider_loop(
        provider.as_ref(),
        messages,
        controls.clone(),
        tool_registry,
        &tool_runtime,
        &request.repo_root,
        &request.project_id,
        &request.run_id,
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
        let tool_registry = ToolRegistry::builtin();
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
    let messages = provider_messages_from_snapshot(&snapshot)?;
    let controls = runtime_controls_from_request(request.controls.as_ref());
    let base_tool_runtime = request
        .tool_runtime
        .with_runtime_run_controls(controls.clone())
        .with_cancellation_token(cancellation.clone());
    let tool_registry = tool_registry_for_snapshot(&request.repo_root, &snapshot, &controls)?;
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
            replay_kind == AnsweredToolReplayKind::OperatorApprovedCommand,
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

#[allow(clippy::too_many_arguments)]
fn drive_provider_loop(
    provider: &dyn ProviderAdapter,
    mut messages: Vec<ProviderMessage>,
    controls: RuntimeRunControlStateDto,
    mut tool_registry: ToolRegistry,
    tool_runtime: &AutonomousToolRuntime,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    cancellation: &AgentRunCancellationToken,
) -> CommandResult<()> {
    let mut workspace_guard = AgentWorkspaceGuard::default();
    let mut usage_total = ProviderUsage::default();

    for turn_index in 0..MAX_PROVIDER_TURNS {
        cancellation.check_cancelled()?;
        touch_agent_run_heartbeat(repo_root, project_id, run_id)?;
        let turn_system_prompt = assemble_system_prompt(repo_root, tool_registry.descriptors())?;
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

fn provider_messages_from_snapshot(
    snapshot: &AgentRunSnapshotRecord,
) -> CommandResult<Vec<ProviderMessage>> {
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
    for message in &snapshot.messages {
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

fn tool_registry_for_snapshot(
    repo_root: &Path,
    snapshot: &AgentRunSnapshotRecord,
    controls: &RuntimeRunControlStateDto,
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

    let mut registry = ToolRegistry::for_prompt(repo_root, prompt_context, controls);
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

fn dispatch_tool_call(
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
fn dispatch_tool_call_with_write_approval(
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
            format!("Cadence could not serialize owned-agent tool input: {error}"),
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

    let old_hash =
        match workspace_guard.validate_write_intent(repo_root, &request, approved_existing_write) {
            Ok(hash) => hash,
            Err(error) => {
                finish_failed_tool_call(repo_root, project_id, run_id, &tool_call, &error)?;
                return Err(error);
            }
        };
    let rollback_checkpoint =
        rollback_checkpoint_for_request(repo_root, &request, old_hash.as_deref())?;

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
                    format!("Cadence could not serialize owned-agent tool output: {error}"),
                )
            })?;
            let result_json = serde_json::to_string(&output).map_err(|error| {
                CommandError::system_fault(
                    "agent_tool_result_serialize_failed",
                    format!("Cadence could not persist owned-agent tool output: {error}"),
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
            record_file_change_event(repo_root, project_id, run_id, old_hash, &tool_result.output)?;
            record_command_output_event(repo_root, project_id, run_id, &tool_result.output)?;
            record_rollback_checkpoint(
                repo_root,
                project_id,
                run_id,
                &tool_call.tool_call_id,
                rollback_checkpoint.as_ref(),
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

fn assemble_system_prompt(
    repo_root: &Path,
    tools: &[AgentToolDescriptor],
) -> CommandResult<String> {
    let agents_instructions = fs::read_to_string(repo_root.join("AGENTS.md")).unwrap_or_default();
    let tool_names = tools
        .iter()
        .map(|tool| tool.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "{SYSTEM_PROMPT_VERSION}\n\nYou are Cadence's owned software-building agent. Work directly in the imported repository, use tools for filesystem and command work, record evidence, and stop only when the task is done or a configured safety boundary requires user input.\n\nOperate like a production coding agent: inspect before editing, respect a dirty worktree, keep changes scoped, prefer `rg` for search, run focused verification when behavior changes, and summarize concrete evidence before completion. Before modifying an existing file, read or hash the target in the current run so Cadence can detect stale writes safely.\n\nAvailable tools: {tool_names}\n\nIf a relevant capability is not currently available, call `tool_access` to request the smallest needed tool group before proceeding. If the `lsp` tool reports an `installSuggestion`, ask the user before running any candidate install command; use the command tool only after consent and normal operator approval.\n\nRepository instructions:\n{}",
        if agents_instructions.trim().is_empty() {
            "(none)"
        } else {
            agents_instructions.trim()
        }
    ))
}

fn select_tool_names_for_prompt(
    repo_root: &Path,
    prompt: &str,
    _controls: &RuntimeRunControlStateDto,
) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    add_tool_group(&mut names, "core");

    let lowered = prompt.to_lowercase();
    names.extend(explicit_tool_names_from_prompt(&lowered));

    if contains_any(
        &lowered,
        &[
            "implement",
            "continue",
            "fix",
            "bug",
            "change",
            "update",
            "edit",
            "write",
            "add ",
            "remove",
            "delete",
            "rename",
            "refactor",
            "migrate",
            "production ready",
            "build",
            "create",
            "scaffold",
        ],
    ) {
        add_tool_group(&mut names, "mutation");
    }

    if contains_any(
        &lowered,
        &[
            "implement",
            "continue",
            "fix",
            "test",
            "audit",
            "review",
            "inspect",
            "investigate",
            "diagnose",
            "verify",
            "run ",
            "cargo",
            "pnpm",
            "npm",
            "build",
            "lint",
            "compile",
            "debug",
            "production ready",
            "production standards",
            "security",
        ],
    ) {
        add_tool_group(&mut names, "command");
    }

    if contains_any(
        &lowered,
        &[
            "browser",
            "frontend",
            "ui",
            "web",
            "playwright",
            "screenshot",
            "localhost",
            "url",
            "docs",
            "documentation",
            "internet",
            "latest",
        ],
    ) {
        add_tool_group(&mut names, "web");
    }

    if contains_any(
        &lowered,
        &[
            "mcp",
            "model context protocol",
            "resource",
            "prompt template",
            "invoke tool",
        ],
    ) {
        add_tool_group(&mut names, "mcp");
    }

    if contains_any(
        &lowered,
        &[
            "subagent",
            "sub-agent",
            "delegate",
            "todo",
            "task list",
            "tool search",
            "deferred tool",
        ],
    ) {
        add_tool_group(&mut names, "agent_ops");
    }

    if contains_any(&lowered, &["notebook", "jupyter", ".ipynb", "cell"]) {
        add_tool_group(&mut names, "notebook");
    }

    if contains_any(
        &lowered,
        &[
            "lsp",
            "symbol",
            "symbols",
            "diagnostic",
            "diagnostics",
            "code intelligence",
            "code-intelligence",
        ],
    ) {
        add_tool_group(&mut names, "intelligence");
    }

    if contains_any(&lowered, &["powershell", "pwsh", "windows shell"]) {
        add_tool_group(&mut names, "powershell");
    }

    if contains_any(
        &lowered,
        &[
            "emulator",
            "simulator",
            "mobile",
            "android",
            "ios",
            "app use",
            "app automation",
            "device",
            "tap",
            "swipe",
        ],
    ) {
        add_tool_group(&mut names, "emulator");
    }

    if contains_any(
        &lowered,
        &[
            "solana",
            "anchor",
            "spl token",
            "program id",
            "validator",
            "squads",
            "codama",
            " pda",
            " idl",
            "metaplex",
            "jupiter",
        ],
    ) || looks_like_solana_workspace(repo_root)
    {
        add_tool_group(&mut names, "solana");
    }

    let known_tools = tool_access_all_known_tools();
    names.retain(|name| known_tools.contains(name.as_str()));
    names
}

fn add_tool_group(names: &mut BTreeSet<String>, group: &str) {
    if let Some(tools) = tool_access_group_tools(group) {
        names.extend(tools.iter().map(|tool| (*tool).to_owned()));
    }
}

fn explicit_tool_names_from_prompt(prompt: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for line in prompt.lines().map(str::trim) {
        match line {
            "tool:git_status" => {
                names.insert(AUTONOMOUS_TOOL_GIT_STATUS.into());
            }
            line if line.starts_with("tool:read ") => {
                names.insert(AUTONOMOUS_TOOL_READ.into());
            }
            line if line.starts_with("tool:search ") => {
                names.insert(AUTONOMOUS_TOOL_SEARCH.into());
            }
            line if line.starts_with("tool:list ") => {
                names.insert(AUTONOMOUS_TOOL_LIST.into());
            }
            line if line.starts_with("tool:hash ") => {
                names.insert(AUTONOMOUS_TOOL_HASH.into());
            }
            line if line.starts_with("tool:write ") => {
                names.insert(AUTONOMOUS_TOOL_WRITE.into());
            }
            line if line.starts_with("tool:mkdir ") => {
                names.insert(AUTONOMOUS_TOOL_MKDIR.into());
            }
            line if line.starts_with("tool:delete ") => {
                names.insert(AUTONOMOUS_TOOL_DELETE.into());
            }
            line if line.starts_with("tool:rename ") => {
                names.insert(AUTONOMOUS_TOOL_RENAME.into());
            }
            line if line.starts_with("tool:patch ") => {
                names.insert(AUTONOMOUS_TOOL_PATCH.into());
            }
            line if line.starts_with("tool:command_") => {
                names.insert(AUTONOMOUS_TOOL_COMMAND.into());
            }
            line if line.starts_with("tool:mcp_") => {
                names.insert(AUTONOMOUS_TOOL_MCP.into());
            }
            line if line.starts_with("tool:subagent ") => {
                names.insert(AUTONOMOUS_TOOL_SUBAGENT.into());
            }
            line if line.starts_with("tool:todo_") => {
                names.insert(AUTONOMOUS_TOOL_TODO.into());
            }
            line if line.starts_with("tool:notebook_edit ") => {
                names.insert(AUTONOMOUS_TOOL_NOTEBOOK_EDIT.into());
            }
            line if line.starts_with("tool:code_intel_") => {
                names.insert(AUTONOMOUS_TOOL_CODE_INTEL.into());
            }
            line if line.starts_with("tool:lsp_") => {
                names.insert(AUTONOMOUS_TOOL_LSP.into());
            }
            line if line.starts_with("tool:powershell ") => {
                names.insert(AUTONOMOUS_TOOL_POWERSHELL.into());
            }
            line if line.starts_with("tool:tool_search ") => {
                names.insert(AUTONOMOUS_TOOL_TOOL_SEARCH.into());
            }
            _ => {}
        }
    }
    names
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn looks_like_solana_workspace(repo_root: &Path) -> bool {
    repo_root.join("Anchor.toml").is_file()
        || repo_root.join("programs").is_dir() && repo_root.join("tests").is_dir()
        || repo_root.join("idl").is_dir() && repo_root.join("target/deploy").is_dir()
}

fn builtin_tool_descriptors() -> Vec<AgentToolDescriptor> {
    let mut descriptors = vec![
        descriptor(
            AUTONOMOUS_TOOL_READ,
            "Read a UTF-8 text file by repo-relative path.",
            object_schema(
                &["path"],
                &[
                    ("path", string_schema("Repo-relative file path to read.")),
                    (
                        "startLine",
                        integer_schema("1-based starting line. Defaults to 1."),
                    ),
                    (
                        "lineCount",
                        integer_schema("Maximum number of lines to return."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_SEARCH,
            "Search text across repo-scoped files.",
            object_schema(
                &["query"],
                &[
                    ("query", string_schema("Literal text query to search for.")),
                    (
                        "path",
                        string_schema("Optional repo-relative directory scope."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_FIND,
            "Find glob/pattern matches in repo-scoped files.",
            object_schema(
                &["pattern"],
                &[
                    ("pattern", string_schema("Glob or path pattern to find.")),
                    (
                        "path",
                        string_schema("Optional repo-relative directory scope."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_GIT_STATUS,
            "Inspect repository status.",
            object_schema(&[], &[]),
        ),
        descriptor(
            AUTONOMOUS_TOOL_GIT_DIFF,
            "Inspect repository diffs.",
            object_schema(
                &["scope"],
                &[(
                    "scope",
                    enum_schema(
                        "Diff scope to inspect.",
                        &["staged", "unstaged", "worktree"],
                    ),
                )],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_TOOL_ACCESS,
            "List or request additional tool groups when the current task requires a hidden capability.",
            object_schema(
                &["action"],
                &[
                    (
                        "action",
                        enum_schema(
                            "Tool-access action to execute.",
                            &["list", "request"],
                        ),
                    ),
                    (
                        "groups",
                        json!({
                            "type": "array",
                            "description": "Optional tool groups to request. Known groups: core, mutation, command, web, emulator, solana, agent_ops, mcp, intelligence, notebook, powershell.",
                            "items": { "type": "string" }
                        }),
                    ),
                    (
                        "tools",
                        json!({
                            "type": "array",
                            "description": "Optional specific tool names to request.",
                            "items": { "type": "string" }
                        }),
                    ),
                    (
                        "reason",
                        string_schema("Brief reason the additional capability is needed."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_EDIT,
            "Apply an exact expected-text line-range edit.",
            object_schema(
                &["path", "startLine", "endLine", "expected", "replacement"],
                &[
                    ("path", string_schema("Repo-relative file path to edit.")),
                    (
                        "startLine",
                        integer_schema("1-based first line to replace."),
                    ),
                    ("endLine", integer_schema("1-based final line to replace.")),
                    (
                        "expected",
                        string_schema("Exact current text expected in the selected range."),
                    ),
                    (
                        "replacement",
                        string_schema("Replacement text for the selected range."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_WRITE,
            "Write a UTF-8 text file by repo-relative path.",
            object_schema(
                &["path", "content"],
                &[
                    ("path", string_schema("Repo-relative file path to write.")),
                    ("content", string_schema("Complete UTF-8 file contents.")),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_PATCH,
            "Patch a UTF-8 text file by replacing exact search text.",
            object_schema(
                &["path", "search", "replace"],
                &[
                    ("path", string_schema("Repo-relative file path to patch.")),
                    ("search", string_schema("Exact text to replace.")),
                    ("replace", string_schema("Replacement text.")),
                    (
                        "replaceAll",
                        boolean_schema("Replace every match instead of exactly one match."),
                    ),
                    (
                        "expectedHash",
                        string_schema("Optional lowercase SHA-256 expected current file hash."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_DELETE,
            "Delete a repo-relative file or, with recursive=true, directory.",
            object_schema(
                &["path"],
                &[
                    ("path", string_schema("Repo-relative path to delete.")),
                    (
                        "recursive",
                        boolean_schema("Required for directory deletion."),
                    ),
                    (
                        "expectedHash",
                        string_schema("Optional lowercase SHA-256 expected file hash."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_RENAME,
            "Rename or move a repo-relative path.",
            object_schema(
                &["fromPath", "toPath"],
                &[
                    (
                        "fromPath",
                        string_schema("Existing repo-relative source path."),
                    ),
                    (
                        "toPath",
                        string_schema("New repo-relative destination path."),
                    ),
                    (
                        "expectedHash",
                        string_schema("Optional lowercase SHA-256 expected source file hash."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_MKDIR,
            "Create a repo-relative directory and missing parents.",
            object_schema(
                &["path"],
                &[(
                    "path",
                    string_schema("Repo-relative directory path to create."),
                )],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_LIST,
            "List repo-scoped files.",
            object_schema(
                &[],
                &[
                    (
                        "path",
                        string_schema("Optional repo-relative directory or file scope."),
                    ),
                    (
                        "maxDepth",
                        integer_schema("Maximum recursion depth from the scope."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_HASH,
            "Hash a repo-relative file with SHA-256.",
            object_schema(
                &["path"],
                &[("path", string_schema("Repo-relative file path to hash."))],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_COMMAND,
            "Run a repo-scoped command.",
            object_schema(
                &["argv"],
                &[
                    (
                        "argv",
                        json!({
                            "type": "array",
                            "description": "Command argv. The first item is the executable.",
                            "items": { "type": "string" },
                            "minItems": 1
                        }),
                    ),
                    (
                        "cwd",
                        string_schema("Optional repo-relative working directory."),
                    ),
                    (
                        "timeoutMs",
                        integer_schema("Optional timeout in milliseconds."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_COMMAND_SESSION_START,
            "Start a repo-scoped long-running command session and capture live output chunks.",
            object_schema(
                &["argv"],
                &[
                    (
                        "argv",
                        json!({
                            "type": "array",
                            "description": "Command argv. The first item is the executable.",
                            "items": { "type": "string" },
                            "minItems": 1
                        }),
                    ),
                    (
                        "cwd",
                        string_schema("Optional repo-relative working directory."),
                    ),
                    (
                        "timeoutMs",
                        integer_schema("Optional startup timeout in milliseconds."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
            "Read new output chunks and exit state from a command session.",
            object_schema(
                &["sessionId"],
                &[
                    ("sessionId", string_schema("Command session handle.")),
                    (
                        "afterSequence",
                        integer_schema("Only return output chunks after this sequence."),
                    ),
                    (
                        "maxBytes",
                        integer_schema("Maximum output bytes to return."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
            "Stop a command session and return its final captured output chunks.",
            object_schema(
                &["sessionId"],
                &[("sessionId", string_schema("Command session handle."))],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_MCP,
            "List connected MCP servers or invoke MCP tools, resources, and prompts through the app-local registry.",
            object_schema(
                &["action"],
                &[
                    (
                        "action",
                        enum_schema(
                            "MCP action to execute.",
                            &[
                                "list_servers",
                                "list_tools",
                                "list_resources",
                                "list_prompts",
                                "invoke_tool",
                                "read_resource",
                                "get_prompt",
                            ],
                        ),
                    ),
                    ("serverId", string_schema("MCP server id for capability actions.")),
                    ("name", string_schema("Tool or prompt name for invocation actions.")),
                    ("uri", string_schema("Resource URI for read_resource.")),
                    (
                        "arguments",
                        json!({
                            "type": "object",
                            "description": "Optional MCP arguments object.",
                            "additionalProperties": true
                        }),
                    ),
                    (
                        "timeoutMs",
                        integer_schema("Optional timeout in milliseconds."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_SUBAGENT,
            "Spawn a built-in model-routed subagent for explore, plan, general, or verification work.",
            object_schema(
                &["agentType", "prompt"],
                &[
                    (
                        "agentType",
                        enum_schema(
                            "Built-in subagent type.",
                            &["explore", "plan", "general", "verify"],
                        ),
                    ),
                    ("prompt", string_schema("Focused task for the subagent.")),
                    (
                        "modelId",
                        string_schema("Optional model route requested for this subagent."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_TODO,
            "Maintain model-visible planning state for the current owned-agent run.",
            object_schema(
                &["action"],
                &[
                    (
                        "action",
                        enum_schema(
                            "Todo action to execute.",
                            &["list", "upsert", "complete", "delete", "clear"],
                        ),
                    ),
                    ("id", string_schema("Todo id for update, complete, or delete.")),
                    ("title", string_schema("Todo title for upsert.")),
                    ("notes", string_schema("Optional todo notes for upsert.")),
                    (
                        "status",
                        enum_schema(
                            "Todo status for upsert.",
                            &["pending", "in_progress", "completed"],
                        ),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_NOTEBOOK_EDIT,
            "Edit a Jupyter notebook cell source by cell index.",
            object_schema(
                &["path", "cellIndex", "replacementSource"],
                &[
                    ("path", string_schema("Repo-relative .ipynb path.")),
                    ("cellIndex", integer_schema("Zero-based notebook cell index.")),
                    (
                        "expectedSource",
                        string_schema("Optional exact current source guard."),
                    ),
                    (
                        "replacementSource",
                        string_schema("Replacement source text for the cell."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_CODE_INTEL,
            "Inspect source symbols or JSON diagnostics without requiring command execution.",
            object_schema(
                &["action"],
                &[
                    (
                        "action",
                        enum_schema(
                            "Code intelligence action.",
                            &["symbols", "diagnostics"],
                        ),
                    ),
                    ("query", string_schema("Optional symbol query.")),
                    ("path", string_schema("Optional repo-relative file or directory scope.")),
                    ("limit", integer_schema("Maximum result count.")),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_LSP,
            "Inspect language-server availability and resolve source symbols or diagnostics through LSP with native fallback.",
            object_schema(
                &["action"],
                &[
                    (
                        "action",
                        enum_schema(
                            "LSP action to execute.",
                            &["servers", "symbols", "diagnostics"],
                        ),
                    ),
                    ("query", string_schema("Optional symbol query.")),
                    ("path", string_schema("Optional repo-relative file or directory scope.")),
                    ("limit", integer_schema("Maximum result count.")),
                    ("serverId", string_schema("Optional known LSP server id to force.")),
                    (
                        "timeoutMs",
                        integer_schema("Optional LSP server timeout in milliseconds."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_POWERSHELL,
            "Run PowerShell through the same repo-scoped command policy used for shell commands.",
            object_schema(
                &["script"],
                &[
                    ("script", string_schema("PowerShell script text to run.")),
                    ("cwd", string_schema("Optional repo-relative working directory.")),
                    (
                        "timeoutMs",
                        integer_schema("Optional timeout in milliseconds."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_TOOL_SEARCH,
            "Search deferred autonomous tool capabilities by name, group, or description.",
            object_schema(
                &["query"],
                &[
                    ("query", string_schema("Tool capability query.")),
                    ("limit", integer_schema("Maximum result count.")),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_WEB_SEARCH,
            "Search the web through the configured backend.",
            object_schema(
                &["query"],
                &[
                    ("query", string_schema("Web search query.")),
                    (
                        "resultCount",
                        integer_schema("Maximum number of search results to return."),
                    ),
                    (
                        "timeoutMs",
                        integer_schema("Optional timeout in milliseconds."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_WEB_FETCH,
            "Fetch a text or HTML URL.",
            object_schema(
                &["url"],
                &[
                    ("url", string_schema("HTTP or HTTPS URL to fetch.")),
                    (
                        "maxChars",
                        integer_schema("Maximum number of characters to return."),
                    ),
                    (
                        "timeoutMs",
                        integer_schema("Optional timeout in milliseconds."),
                    ),
                ],
            ),
        ),
        descriptor(
            AUTONOMOUS_TOOL_BROWSER,
            "Drive the in-app browser automation surface.",
            browser_schema(),
        ),
        descriptor(
            AUTONOMOUS_TOOL_EMULATOR,
            "Drive mobile emulator and app automation: device lifecycle, screenshots, UI inspection, touch/type/key input, app install/launch/terminate, location, push notifications, and logs.",
            emulator_schema(),
        ),
    ];

    descriptors.extend(solana_tool_descriptors());
    descriptors
}

fn descriptor(name: &str, description: &str, input_schema: JsonValue) -> AgentToolDescriptor {
    AgentToolDescriptor {
        name: name.into(),
        description: description.into(),
        input_schema,
    }
}

fn object_schema(required: &[&str], properties: &[(&str, JsonValue)]) -> JsonValue {
    let mut properties_map = JsonMap::new();
    for (name, schema) in properties {
        properties_map.insert((*name).into(), schema.clone());
    }

    json!({
        "type": "object",
        "additionalProperties": false,
        "required": required,
        "properties": properties_map,
    })
}

fn string_schema(description: &str) -> JsonValue {
    json!({
        "type": "string",
        "description": description,
    })
}

fn integer_schema(description: &str) -> JsonValue {
    json!({
        "type": "integer",
        "minimum": 0,
        "description": description,
    })
}

fn boolean_schema(description: &str) -> JsonValue {
    json!({
        "type": "boolean",
        "description": description,
    })
}

fn enum_schema(description: &str, values: &[&str]) -> JsonValue {
    json!({
        "type": "string",
        "description": description,
        "enum": values,
    })
}

fn browser_schema() -> JsonValue {
    object_schema(
        &["action"],
        &[
            (
                "action",
                enum_schema(
                    "Browser action to execute.",
                    &[
                        "open",
                        "tab_open",
                        "navigate",
                        "back",
                        "forward",
                        "reload",
                        "stop",
                        "click",
                        "type",
                        "scroll",
                        "press_key",
                        "read_text",
                        "query",
                        "wait_for_selector",
                        "wait_for_load",
                        "current_url",
                        "history_state",
                        "screenshot",
                        "cookies_get",
                        "cookies_set",
                        "storage_read",
                        "storage_write",
                        "storage_clear",
                        "tab_list",
                        "tab_close",
                        "tab_focus",
                    ],
                ),
            ),
            ("url", string_schema("URL for open, tab_open, or navigate.")),
            (
                "selector",
                string_schema("CSS selector for DOM-targeted actions."),
            ),
            ("text", string_schema("Text for the type action.")),
            (
                "append",
                boolean_schema("Append instead of replacing typed text."),
            ),
            ("x", integer_schema("Horizontal scroll offset.")),
            ("y", integer_schema("Vertical scroll offset.")),
            ("key", string_schema("Keyboard key to press.")),
            ("limit", integer_schema("Maximum number of query results.")),
            (
                "visible",
                boolean_schema("Whether wait_for_selector requires visibility."),
            ),
            ("cookie", string_schema("Cookie string for cookies_set.")),
            ("area", enum_schema("Storage area.", &["local", "session"])),
            ("value", string_schema("Storage value for storage_write.")),
            ("tabId", string_schema("Browser tab id.")),
            (
                "timeoutMs",
                integer_schema("Optional timeout in milliseconds."),
            ),
        ],
    )
}

fn solana_tool_descriptors() -> Vec<AgentToolDescriptor> {
    [
        (
            AUTONOMOUS_TOOL_SOLANA_CLUSTER,
            "Manage and inspect local Solana clusters.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_LOGS,
            "Fetch, inspect, subscribe to, or stop Solana logs.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_TX,
            "Build, send, price, or inspect Solana transactions.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_SIMULATE,
            "Simulate a Solana transaction request.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_EXPLAIN,
            "Explain Solana transactions or program behavior.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_ALT,
            "Create, extend, or resolve address lookup tables.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_IDL,
            "Load, fetch, publish, or inspect Solana IDLs.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_CODAMA,
            "Generate Codama client artifacts from an IDL.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_PDA,
            "Derive or analyze Solana program-derived addresses.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_PROGRAM,
            "Build, inspect, or scaffold Solana programs.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_DEPLOY,
            "Deploy a Solana program through Cadence safety gates.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
            "Check Solana program upgrade safety.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_SQUADS,
            "Create or inspect Squads governance proposals.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
            "Run or inspect verified Solana builds.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
            "Run static Solana audit checks.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
            "Run external Solana audit analyzers.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ,
            "Run Solana fuzzing audit flows.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE,
            "Run Solana audit coverage checks.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_REPLAY,
            "Replay Solana transactions or scenarios.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_INDEXER,
            "Scaffold or run local Solana indexers.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_SECRETS,
            "Scan Solana projects for secret leakage.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
            "Check Solana cluster drift.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_COST,
            "Estimate or inspect Solana transaction costs.",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_DOCS,
            "Retrieve Solana development documentation snippets.",
        ),
    ]
    .into_iter()
    .map(|(name, description)| descriptor(name, description, json!({ "type": "object" })))
    .collect()
}

fn runtime_controls_from_request(
    controls: Option<&RuntimeRunControlInputDto>,
) -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            model_id: controls
                .map(|controls| controls.model_id.clone())
                .unwrap_or_else(|| OPENAI_CODEX_PROVIDER_ID.into()),
            thinking_effort: controls.and_then(|controls| controls.thinking_effort.clone()),
            approval_mode: controls
                .map(|controls| controls.approval_mode.clone())
                .unwrap_or(RuntimeRunApprovalModeDto::Yolo),
            plan_mode_required: controls
                .map(|controls| controls.plan_mode_required)
                .unwrap_or(false),
            revision: 1,
            applied_at: now_timestamp(),
        },
        pending: None,
    }
}

fn parse_fake_tool_directives(prompt: &str) -> Vec<AgentToolCall> {
    let mut calls = Vec::new();
    for line in prompt.lines().map(str::trim) {
        if let Some(path) = line.strip_prefix("tool:read ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-read-{}", calls.len() + 1),
                tool_name: "read".into(),
                input: json!({ "path": path.trim(), "startLine": 1, "lineCount": 40 }),
            });
            continue;
        }
        if line == "tool:git_status" {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-git-status-{}", calls.len() + 1),
                tool_name: "git_status".into(),
                input: json!({}),
            });
            continue;
        }
        if let Some(group) = line.strip_prefix("tool:access ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-tool-access-{}", calls.len() + 1),
                tool_name: "tool_access".into(),
                input: json!({ "action": "request", "groups": [group.trim()] }),
            });
            continue;
        }
        if let Some(query) = line.strip_prefix("tool:tool_search ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-tool-search-{}", calls.len() + 1),
                tool_name: "tool_search".into(),
                input: json!({ "query": query.trim(), "limit": 10 }),
            });
            continue;
        }
        if let Some(title) = line.strip_prefix("tool:todo_upsert ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-todo-{}", calls.len() + 1),
                tool_name: "todo".into(),
                input: json!({ "action": "upsert", "title": title.trim() }),
            });
            continue;
        }
        if let Some(id) = line.strip_prefix("tool:todo_complete ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-todo-{}", calls.len() + 1),
                tool_name: "todo".into(),
                input: json!({ "action": "complete", "id": id.trim() }),
            });
            continue;
        }
        if let Some(prompt) = line.strip_prefix("tool:subagent ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-subagent-{}", calls.len() + 1),
                tool_name: "subagent".into(),
                input: json!({ "agentType": "explore", "prompt": prompt.trim() }),
            });
            continue;
        }
        if line == "tool:mcp_list" {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-mcp-{}", calls.len() + 1),
                tool_name: "mcp".into(),
                input: json!({ "action": "list_servers" }),
            });
            continue;
        }
        if let Some(server_id) = line.strip_prefix("tool:mcp_list_tools ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-mcp-{}", calls.len() + 1),
                tool_name: "mcp".into(),
                input: json!({ "action": "list_tools", "serverId": server_id.trim() }),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("tool:code_intel_symbols ") {
            let (path, query) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-code-intel-{}", calls.len() + 1),
                tool_name: "code_intel".into(),
                input: json!({
                    "action": "symbols",
                    "path": path,
                    "query": query,
                    "limit": 20
                }),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("tool:lsp_symbols ") {
            let (path, query) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-lsp-{}", calls.len() + 1),
                tool_name: "lsp".into(),
                input: json!({
                    "action": "symbols",
                    "path": path,
                    "query": query,
                    "limit": 20
                }),
            });
            continue;
        }
        if line == "tool:lsp_servers" {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-lsp-{}", calls.len() + 1),
                tool_name: "lsp".into(),
                input: json!({ "action": "servers" }),
            });
            continue;
        }
        if let Some(query) = line.strip_prefix("tool:search ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-search-{}", calls.len() + 1),
                tool_name: "search".into(),
                input: json!({ "query": query.trim() }),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("tool:write ") {
            let (path, content) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-write-{}", calls.len() + 1),
                tool_name: "write".into(),
                input: json!({ "path": path, "content": content }),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("tool:hash ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-hash-{}", calls.len() + 1),
                tool_name: "file_hash".into(),
                input: json!({ "path": path.trim() }),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("tool:list ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-list-{}", calls.len() + 1),
                tool_name: "list".into(),
                input: json!({ "path": path.trim(), "maxDepth": 2 }),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("tool:mkdir ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-mkdir-{}", calls.len() + 1),
                tool_name: "mkdir".into(),
                input: json!({ "path": path.trim() }),
            });
            continue;
        }
        if let Some(path) = line.strip_prefix("tool:delete ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-delete-{}", calls.len() + 1),
                tool_name: "delete".into(),
                input: json!({ "path": path.trim() }),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("tool:rename ") {
            let (from_path, to_path) = rest.trim().split_once(' ').unwrap_or((rest.trim(), ""));
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-rename-{}", calls.len() + 1),
                tool_name: "rename".into(),
                input: json!({ "fromPath": from_path, "toPath": to_path }),
            });
            continue;
        }
        if let Some(rest) = line.strip_prefix("tool:patch ") {
            let mut parts = rest.trim().splitn(3, ' ');
            let path = parts.next().unwrap_or_default();
            let search = parts.next().unwrap_or_default();
            let replace = parts.next().unwrap_or_default();
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-patch-{}", calls.len() + 1),
                tool_name: "patch".into(),
                input: json!({ "path": path, "search": search, "replace": replace }),
            });
            continue;
        }
        if let Some(text) = line.strip_prefix("tool:command_echo ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-command-{}", calls.len() + 1),
                tool_name: "command".into(),
                input: json!({ "argv": ["echo", text.trim()] }),
            });
            continue;
        }
        if let Some(script) = line.strip_prefix("tool:command_sh ") {
            calls.push(AgentToolCall {
                tool_call_id: format!("tool-call-command-{}", calls.len() + 1),
                tool_name: "command".into(),
                input: json!({ "argv": ["sh", "-c", script.trim()] }),
            });
        }
    }
    calls
}

fn append_message(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    role: AgentMessageRole,
    content: String,
) -> CommandResult<AgentMessageRecord> {
    project_store::append_agent_message(
        repo_root,
        &NewAgentMessageRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            role,
            content,
            created_at: now_timestamp(),
        },
    )
}

fn append_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    event_kind: AgentRunEventKind,
    payload: JsonValue,
) -> CommandResult<AgentEventRecord> {
    let payload_json = serde_json::to_string(&payload).map_err(|error| {
        CommandError::system_fault(
            "agent_event_serialize_failed",
            format!("Cadence could not serialize owned-agent event payload: {error}"),
        )
    })?;
    let event = project_store::append_agent_event(
        repo_root,
        &NewAgentEventRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            event_kind,
            payload_json,
            created_at: now_timestamp(),
        },
    )?;
    publish_agent_event(event.clone());
    Ok(event)
}

fn touch_agent_run_heartbeat(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<()> {
    project_store::touch_agent_run_heartbeat(repo_root, project_id, run_id, &now_timestamp())
}

fn repo_fingerprint(repo_root: &Path) -> JsonValue {
    match git2::Repository::discover(repo_root) {
        Ok(repository) => {
            let head = repository
                .head()
                .ok()
                .and_then(|head| head.target())
                .map(|oid| oid.to_string());
            let dirty = repository
                .statuses(None)
                .map(|statuses| !statuses.is_empty())
                .unwrap_or(false);
            json!({
                "kind": "git",
                "head": head,
                "dirty": dirty,
            })
        }
        Err(_) => json!({ "kind": "filesystem" }),
    }
}

fn record_file_change_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    old_hash: Option<String>,
    output: &AutonomousToolOutput,
) -> CommandResult<()> {
    let (operation, path) = match output {
        AutonomousToolOutput::Write(output) => (
            if output.created { "create" } else { "write" },
            output.path.as_str(),
        ),
        AutonomousToolOutput::Edit(output) => ("edit", output.path.as_str()),
        AutonomousToolOutput::Patch(output) => ("patch", output.path.as_str()),
        AutonomousToolOutput::NotebookEdit(output) => ("notebook_edit", output.path.as_str()),
        AutonomousToolOutput::Delete(output) => ("delete", output.path.as_str()),
        AutonomousToolOutput::Rename(output) => ("rename", output.from_path.as_str()),
        AutonomousToolOutput::Mkdir(output) => ("mkdir", output.path.as_str()),
        _ => return Ok(()),
    };

    let new_hash_path = match output {
        AutonomousToolOutput::Rename(output) => output.to_path.as_str(),
        _ => path,
    };
    let new_hash = file_hash_if_present(repo_root, new_hash_path)?;
    let to_path = match output {
        AutonomousToolOutput::Rename(output) => Some(output.to_path.clone()),
        _ => None,
    };
    project_store::append_agent_file_change(
        repo_root,
        &NewAgentFileChangeRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            path: path.into(),
            operation: operation.into(),
            old_hash: old_hash.clone(),
            new_hash: new_hash.clone(),
            created_at: now_timestamp(),
        },
    )?;

    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::FileChanged,
        json!({
            "path": path,
            "operation": operation,
            "toPath": to_path,
            "oldHash": old_hash,
            "newHash": new_hash,
        }),
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
struct AgentRollbackCheckpoint {
    path: String,
    operation: String,
    old_hash: Option<String>,
    old_content_base64: Option<String>,
    old_content_omitted_reason: Option<String>,
    old_content_bytes: Option<u64>,
}

fn rollback_checkpoint_for_request(
    repo_root: &Path,
    request: &AutonomousToolRequest,
    old_hash: Option<&str>,
) -> CommandResult<Option<AgentRollbackCheckpoint>> {
    let (path, operation) = match request {
        AutonomousToolRequest::Edit(request) => (request.path.as_str(), "edit"),
        AutonomousToolRequest::Patch(request) => (request.path.as_str(), "patch"),
        AutonomousToolRequest::NotebookEdit(request) => (request.path.as_str(), "notebook_edit"),
        AutonomousToolRequest::Delete(request) => (request.path.as_str(), "delete"),
        AutonomousToolRequest::Rename(request) => (request.from_path.as_str(), "rename"),
        AutonomousToolRequest::Write(request) if old_hash.is_some() => {
            (request.path.as_str(), "write")
        }
        AutonomousToolRequest::Write(request) => (request.path.as_str(), "create"),
        _ => return Ok(None),
    };
    let Some(path_key) = relative_path_key(path) else {
        return Ok(None);
    };
    let old_content = match old_hash {
        Some(_) => capture_rollback_content(repo_root, &path_key)?,
        None => RollbackContentCapture::NotNeeded,
    };
    let (old_content_base64, old_content_omitted_reason, old_content_bytes) = match old_content {
        RollbackContentCapture::Captured { base64, bytes } => (Some(base64), None, Some(bytes)),
        RollbackContentCapture::Omitted { reason, bytes } => (None, Some(reason), bytes),
        RollbackContentCapture::NotNeeded => (None, None, None),
    };

    Ok(Some(AgentRollbackCheckpoint {
        path: path_key,
        operation: operation.into(),
        old_hash: old_hash.map(ToOwned::to_owned),
        old_content_base64,
        old_content_omitted_reason,
        old_content_bytes,
    }))
}

fn record_rollback_checkpoint(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_call_id: &str,
    checkpoint: Option<&AgentRollbackCheckpoint>,
) -> CommandResult<()> {
    let Some(checkpoint) = checkpoint else {
        return Ok(());
    };

    let payload_json = serde_json::to_string(&json!({
        "kind": "file_rollback",
        "toolCallId": tool_call_id,
        "path": checkpoint.path.clone(),
        "operation": checkpoint.operation.clone(),
        "oldHash": checkpoint.old_hash.clone(),
        "oldContentBase64": checkpoint.old_content_base64.clone(),
        "oldContentOmittedReason": checkpoint.old_content_omitted_reason.clone(),
        "oldContentBytes": checkpoint.old_content_bytes,
    }))
    .map_err(|error| {
        CommandError::system_fault(
            "agent_checkpoint_payload_serialize_failed",
            format!("Cadence could not serialize owned-agent rollback checkpoint: {error}"),
        )
    })?;

    project_store::append_agent_checkpoint(
        repo_root,
        &NewAgentCheckpointRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            checkpoint_kind: "tool".into(),
            summary: format!("Rollback data for `{}`.", checkpoint.path),
            payload_json: Some(payload_json),
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RollbackContentCapture {
    Captured { base64: String, bytes: u64 },
    Omitted { reason: String, bytes: Option<u64> },
    NotNeeded,
}

fn capture_rollback_content(
    repo_root: &Path,
    repo_relative_path: &str,
) -> CommandResult<RollbackContentCapture> {
    use base64::Engine as _;

    let Some(relative_path) = safe_relative_path(repo_relative_path) else {
        return Err(CommandError::new(
            "agent_file_path_invalid",
            CommandErrorClass::PolicyDenied,
            format!(
                "Cadence refused to capture rollback data for `{repo_relative_path}` because it is not a safe repo-relative path."
            ),
            false,
        ));
    };
    let path = repo_root.join(relative_path);
    if is_sensitive_rollback_path(repo_relative_path) {
        return Ok(RollbackContentCapture::Omitted {
            reason: "sensitive_path".into(),
            bytes: fs::metadata(&path).ok().map(|metadata| metadata.len()),
        });
    }
    let metadata = fs::metadata(&path).map_err(|error| {
        CommandError::retryable(
            "agent_rollback_read_failed",
            format!(
                "Cadence could not inspect rollback data for {}: {error}",
                path.display()
            ),
        )
    })?;
    if metadata.len() > MAX_ROLLBACK_CONTENT_BYTES {
        return Ok(RollbackContentCapture::Omitted {
            reason: "file_too_large".into(),
            bytes: Some(metadata.len()),
        });
    }
    let bytes = fs::read(&path).map_err(|error| {
        CommandError::retryable(
            "agent_rollback_read_failed",
            format!(
                "Cadence could not capture rollback data for {}: {error}",
                path.display()
            ),
        )
    })?;
    let text = String::from_utf8_lossy(&bytes);
    if find_prohibited_persistence_content(&text).is_some() {
        return Ok(RollbackContentCapture::Omitted {
            reason: "sensitive_content".into(),
            bytes: Some(bytes.len() as u64),
        });
    }
    Ok(RollbackContentCapture::Captured {
        base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
        bytes: bytes.len() as u64,
    })
}

fn is_sensitive_rollback_path(repo_relative_path: &str) -> bool {
    let normalized = repo_relative_path.replace('\\', "/").to_ascii_lowercase();
    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(normalized.as_str());

    file_name == ".env"
        || file_name.starts_with(".env.")
        || matches!(
            file_name,
            "id_rsa"
                | "id_dsa"
                | "id_ecdsa"
                | "id_ed25519"
                | "credentials"
                | "credentials.json"
                | "secrets.json"
                | "service-account.json"
        )
        || normalized.contains("/.ssh/")
        || normalized.contains("/.aws/")
        || normalized.contains("/.gnupg/")
        || normalized.contains("secret")
        || normalized.contains("credential")
        || normalized.contains("private_key")
        || normalized.contains("private-key")
        || normalized.ends_with(".pem")
        || normalized.ends_with(".key")
        || normalized.ends_with(".p12")
        || normalized.ends_with(".pfx")
}

fn record_command_output_event(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    output: &AutonomousToolOutput,
) -> CommandResult<()> {
    match output {
        AutonomousToolOutput::Command(output) => {
            let argv = redact_command_argv_for_persistence(&output.argv);
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "argv": argv.clone(),
                    "cwd": output.cwd.clone(),
                    "stdout": output.stdout.clone(),
                    "stderr": output.stderr.clone(),
                    "stdoutTruncated": output.stdout_truncated,
                    "stderrTruncated": output.stderr_truncated,
                    "stdoutRedacted": output.stdout_redacted,
                    "stderrRedacted": output.stderr_redacted,
                    "exitCode": output.exit_code,
                    "timedOut": output.timed_out,
                    "spawned": output.spawned,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned {
                record_command_action_required(
                    repo_root,
                    project_id,
                    run_id,
                    "command",
                    &argv,
                    &output.policy.reason,
                    &output.policy.code,
                )?;
            }
        }
        AutonomousToolOutput::CommandSession(output) => {
            let argv = redact_command_argv_for_persistence(&output.argv);
            append_event(
                repo_root,
                project_id,
                run_id,
                AgentRunEventKind::CommandOutput,
                json!({
                    "operation": output.operation.clone(),
                    "sessionId": output.session_id.clone(),
                    "argv": argv.clone(),
                    "cwd": output.cwd.clone(),
                    "running": output.running,
                    "exitCode": output.exit_code,
                    "spawned": output.spawned,
                    "chunks": output.chunks.clone(),
                    "nextSequence": output.next_sequence,
                    "policy": output.policy.clone(),
                }),
            )?;

            if !output.spawned {
                if let Some(policy) = output.policy.as_ref() {
                    record_command_action_required(
                        repo_root,
                        project_id,
                        run_id,
                        "command_session_start",
                        &argv,
                        &policy.reason,
                        &policy.code,
                    )?;
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn record_command_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    tool_name: &str,
    argv: &[String],
    reason: &str,
    code: &str,
) -> CommandResult<()> {
    record_action_request(
        repo_root,
        project_id,
        run_id,
        &format!("command-{}", argv.join("-")),
        "command_approval",
        "Command requires review",
        reason,
    )?;
    append_event(
        repo_root,
        project_id,
        run_id,
        AgentRunEventKind::ActionRequired,
        json!({
            "reason": reason,
            "code": code,
            "toolName": tool_name,
            "argv": argv,
        }),
    )?;
    Ok(())
}

fn record_action_request(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    action_type: &str,
    title: &str,
    detail: &str,
) -> CommandResult<()> {
    project_store::append_agent_action_request(
        repo_root,
        &NewAgentActionRequestRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            action_id: sanitize_action_id(action_id),
            action_type: action_type.into(),
            title: title.into(),
            detail: detail.into(),
            created_at: now_timestamp(),
        },
    )?;
    Ok(())
}

fn sanitize_action_id(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '-'
            }
        })
        .collect()
}

#[derive(Debug, Default)]
struct AgentWorkspaceGuard {
    observed_hashes: BTreeMap<String, Option<String>>,
}

impl AgentWorkspaceGuard {
    fn validate_write_intent(
        &self,
        repo_root: &Path,
        request: &AutonomousToolRequest,
        approved_existing_write: bool,
    ) -> CommandResult<Option<String>> {
        let Some(path) = planned_file_change_path(request) else {
            return Ok(None);
        };
        let Some(path_key) = relative_path_key(path) else {
            return Err(CommandError::new(
                "agent_file_path_invalid",
                CommandErrorClass::PolicyDenied,
                format!("Cadence refused to modify `{path}` because it is not a safe repo-relative path."),
                false,
            ));
        };

        let current_hash = file_hash_if_present(repo_root, &path_key)?;
        if approved_existing_write {
            return Ok(current_hash);
        }
        match (&current_hash, self.observed_hashes.get(&path_key)) {
            (None, _) => Ok(None),
            (Some(_), None) => Err(CommandError::new(
                "agent_file_write_requires_observation",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Cadence refused to modify `{path_key}` because the owned agent has not read this existing file during the run."
                ),
                false,
            )),
            (Some(current_hash), Some(observed_hash)) if observed_hash.as_ref() == Some(current_hash) => {
                Ok(Some(current_hash.clone()))
            }
            (Some(_), Some(observed_hash)) => Err(CommandError::new(
                "agent_file_changed_since_observed",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Cadence refused to modify `{path_key}` because the file changed after the owned agent last observed it (last observed hash: {}).",
                    observed_hash
                        .as_deref()
                        .unwrap_or("absent")
                ),
                false,
            )),
        }
    }

    fn record_tool_output(
        &mut self,
        repo_root: &Path,
        output: &AutonomousToolOutput,
    ) -> CommandResult<()> {
        let path = match output {
            AutonomousToolOutput::Read(output) => output.path.as_str(),
            AutonomousToolOutput::Edit(output) => output.path.as_str(),
            AutonomousToolOutput::Write(output) => output.path.as_str(),
            AutonomousToolOutput::Patch(output) => output.path.as_str(),
            AutonomousToolOutput::NotebookEdit(output) => output.path.as_str(),
            AutonomousToolOutput::Delete(output) => output.path.as_str(),
            AutonomousToolOutput::Rename(output) => output.from_path.as_str(),
            AutonomousToolOutput::Hash(output) => output.path.as_str(),
            _ => return Ok(()),
        };
        let Some(path_key) = relative_path_key(path) else {
            return Ok(());
        };
        let hash = file_hash_if_present(repo_root, &path_key)?;
        self.observed_hashes.insert(path_key, hash);
        if let AutonomousToolOutput::Rename(output) = output {
            if let Some(to_path_key) = relative_path_key(&output.to_path) {
                let hash = file_hash_if_present(repo_root, &to_path_key)?;
                self.observed_hashes.insert(to_path_key, hash);
            }
        }
        Ok(())
    }
}

fn planned_file_change_path(request: &AutonomousToolRequest) -> Option<&str> {
    match request {
        AutonomousToolRequest::Edit(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Write(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Patch(request) => Some(request.path.as_str()),
        AutonomousToolRequest::NotebookEdit(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Delete(request) => Some(request.path.as_str()),
        AutonomousToolRequest::Rename(request) => Some(request.from_path.as_str()),
        _ => None,
    }
}

fn relative_path_key(value: &str) -> Option<String> {
    let relative = safe_relative_path(value)?;
    Some(
        relative
            .components()
            .filter_map(|component| match component {
                Component::Normal(segment) => segment.to_str(),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("/"),
    )
}

fn file_hash_if_present(
    repo_root: &Path,
    repo_relative_path: &str,
) -> CommandResult<Option<String>> {
    let Some(relative_path) = safe_relative_path(repo_relative_path) else {
        return Ok(None);
    };
    let path = repo_root.join(relative_path);
    match fs::read(&path) {
        Ok(bytes) => Ok(Some(sha256_hex(&bytes))),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::IsADirectory
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(CommandError::retryable(
            "agent_file_hash_read_failed",
            format!(
                "Cadence could not hash owned-agent file change target {}: {error}",
                path.display()
            ),
        )),
    }
}

fn safe_relative_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return None;
    }

    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => sanitized.push(segment),
            Component::CurDir => {}
            _ => return None,
        }
    }

    (!sanitized.as_os_str().is_empty()).then_some(sanitized)
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
}

fn validate_prompt(prompt: &str) -> CommandResult<()> {
    if prompt.trim().is_empty() {
        return Err(CommandError::invalid_request("prompt"));
    }
    Ok(())
}
