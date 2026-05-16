use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

mod environment_lifecycle;
mod headless_runtime;
mod production_runtime;
mod protocol;
mod provider_capabilities;
mod provider_preflight;
mod sandbox;
mod tool_packs;
mod tool_registry;

pub use environment_lifecycle::*;
pub use headless_runtime::*;
pub use production_runtime::*;
pub use protocol::*;
pub use provider_capabilities::*;
pub use provider_preflight::*;
pub use sandbox::*;
pub use tool_packs::*;
pub use tool_registry::*;

pub const CORE_PROTOCOL_VERSION: u32 = 3;

pub type CoreResult<T> = Result<T, CoreError>;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct CoreError {
    pub code: String,
    pub message: String,
}

impl CoreError {
    pub fn invalid_request(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn system_fault(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    pub fn unsupported(operation: &str) -> Self {
        Self {
            code: "agent_core_operation_unsupported".into(),
            message: format!("The reusable agent core facade does not implement `{operation}`."),
        }
    }
}

pub trait AgentRuntimeFacade {
    type StartRunRequest;
    type ContinueRunRequest;
    type UserInputRequest;
    type ApprovalRequest;
    type RejectRequest;
    type CancelRunRequest;
    type ResumeRunRequest;
    type ForkSessionRequest;
    type CompactSessionRequest;
    type ExportTraceRequest;
    type Snapshot;
    type Trace;
    type Error;

    fn start_run(&self, request: Self::StartRunRequest) -> Result<Self::Snapshot, Self::Error>;

    fn continue_run(
        &self,
        request: Self::ContinueRunRequest,
    ) -> Result<Self::Snapshot, Self::Error>;

    fn submit_user_input(
        &self,
        request: Self::UserInputRequest,
    ) -> Result<Self::Snapshot, Self::Error>;

    fn approve_action(&self, request: Self::ApprovalRequest)
        -> Result<Self::Snapshot, Self::Error>;

    fn reject_action(&self, request: Self::RejectRequest) -> Result<Self::Snapshot, Self::Error>;

    fn cancel_run(&self, request: Self::CancelRunRequest) -> Result<Self::Snapshot, Self::Error>;

    fn resume_run(&self, request: Self::ResumeRunRequest) -> Result<Self::Snapshot, Self::Error>;

    fn fork_session(
        &self,
        request: Self::ForkSessionRequest,
    ) -> Result<Self::Snapshot, Self::Error>;

    fn compact_session(
        &self,
        request: Self::CompactSessionRequest,
    ) -> Result<Self::Snapshot, Self::Error>;

    fn export_trace(&self, request: Self::ExportTraceRequest) -> Result<Self::Trace, Self::Error>;
}

pub trait AgentCoreStore: Clone + Send + Sync + 'static {
    fn runtime_store_descriptor(&self, project_id: &str) -> RuntimeStoreDescriptor {
        RuntimeStoreDescriptor::in_memory_harness(project_id)
    }

    fn semantic_workspace_index_state(&self, _project_id: &str) -> EnvironmentSemanticIndexState {
        EnvironmentSemanticIndexState::Unavailable
    }

    fn insert_run(&self, run: NewRunRecord) -> CoreResult<RunSnapshot>;
    fn load_run(&self, project_id: &str, run_id: &str) -> CoreResult<RunSnapshot>;
    fn append_message(&self, message: NewMessageRecord) -> CoreResult<RunSnapshot>;
    fn append_event(&self, event: NewRuntimeEvent) -> CoreResult<RuntimeEvent>;
    fn record_context_manifest(&self, manifest: NewContextManifest) -> CoreResult<ContextManifest>;
    fn update_run_status(
        &self,
        project_id: &str,
        run_id: &str,
        status: RunStatus,
    ) -> CoreResult<RunSnapshot>;
    fn export_trace(&self, project_id: &str, run_id: &str) -> CoreResult<RuntimeTrace>;

    fn latest_run_for_session(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> CoreResult<RunSnapshot> {
        let _ = (project_id, agent_session_id);
        Err(CoreError::unsupported("latest_run_for_session"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderSelection {
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunControls {
    pub runtime_agent_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<String>,
    pub approval_mode: String,
    pub plan_mode_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRunRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub prompt: String,
    pub provider: ProviderSelection,
    pub controls: Option<RunControls>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContinueRunRequest {
    pub project_id: String,
    pub run_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserInputRequest {
    pub project_id: String,
    pub run_id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ApprovalDecisionRequest {
    pub project_id: String,
    pub run_id: String,
    pub action_id: String,
    pub response: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelRunRequest {
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeRunRequest {
    pub project_id: String,
    pub run_id: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForkSessionRequest {
    pub project_id: String,
    pub source_agent_session_id: String,
    pub target_agent_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactSessionRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportTraceRequest {
    pub project_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Starting,
    Running,
    Paused,
    Cancelling,
    Cancelled,
    HandedOff,
    Completed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventKind {
    RunStarted,
    MessageDelta,
    ReasoningSummary,
    ToolStarted,
    ToolDelta,
    ToolCompleted,
    FileChanged,
    CommandOutput,
    ValidationStarted,
    ValidationCompleted,
    ToolRegistrySnapshot,
    PolicyDecision,
    StateTransition,
    PlanUpdated,
    VerificationGate,
    ContextManifestRecorded,
    RetrievalPerformed,
    MemoryCandidateCaptured,
    EnvironmentLifecycleUpdate,
    SandboxLifecycleUpdate,
    ActionRequired,
    ApprovalRequired,
    ToolPermissionGrant,
    ProviderModelChanged,
    RuntimeSettingsChanged,
    RunPaused,
    RunCompleted,
    RunFailed,
    SubagentLifecycle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeMessage {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub role: MessageRole,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_metadata: Option<RuntimeMessageProviderMetadata>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeMessageProviderMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_message_id: Option<String>,
    #[serde(
        default,
        alias = "reasoning_content",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_content: Option<String>,
    #[serde(
        default,
        alias = "reasoning_details",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning_details: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assistant_tool_calls: Vec<RuntimeProviderToolCallMetadata>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result: Option<RuntimeProviderToolResultMetadata>,
}

impl RuntimeMessageProviderMetadata {
    pub fn assistant_tool_calls(
        provider_message_id: impl Into<String>,
        tool_calls: Vec<RuntimeProviderToolCallMetadata>,
    ) -> Self {
        Self {
            provider_message_id: Some(provider_message_id.into()),
            reasoning_content: None,
            reasoning_details: None,
            assistant_tool_calls: tool_calls,
            tool_result: None,
        }
    }

    pub fn assistant_turn(
        provider_message_id: impl Into<String>,
        reasoning_content: Option<String>,
        reasoning_details: Option<JsonValue>,
        tool_calls: Vec<RuntimeProviderToolCallMetadata>,
    ) -> Self {
        Self {
            provider_message_id: Some(provider_message_id.into()),
            reasoning_content,
            reasoning_details,
            assistant_tool_calls: tool_calls,
            tool_result: None,
        }
    }

    pub fn tool_result(
        provider_message_id: impl Into<String>,
        tool_call_id: impl Into<String>,
        provider_tool_name: impl Into<String>,
        parent_assistant_message_id: impl Into<String>,
    ) -> Self {
        Self {
            provider_message_id: Some(provider_message_id.into()),
            reasoning_content: None,
            reasoning_details: None,
            assistant_tool_calls: Vec::new(),
            tool_result: Some(RuntimeProviderToolResultMetadata {
                tool_call_id: tool_call_id.into(),
                provider_tool_name: provider_tool_name.into(),
                parent_assistant_message_id: parent_assistant_message_id.into(),
            }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeProviderToolCallMetadata {
    pub tool_call_id: String,
    pub provider_tool_name: String,
    pub arguments: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeProviderToolResultMetadata {
    pub tool_call_id: String,
    pub provider_tool_name: String,
    pub parent_assistant_message_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeEvent {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub event_kind: RuntimeEventKind,
    pub trace: RuntimeTraceContext,
    pub payload: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextManifest {
    pub manifest_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub turn_index: usize,
    pub context_hash: String,
    pub recorded_after_event_id: Option<i64>,
    pub trace: RuntimeTraceContext,
    pub manifest: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunSnapshot {
    pub trace_id: String,
    pub runtime_agent_id: String,
    pub agent_definition_id: String,
    pub agent_definition_version: i64,
    pub system_prompt: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: RunStatus,
    pub prompt: String,
    pub messages: Vec<RuntimeMessage>,
    pub events: Vec<RuntimeEvent>,
    pub context_manifests: Vec<ContextManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeTrace {
    pub protocol_version: u32,
    pub trace_id: String,
    pub snapshot: RunSnapshot,
    pub events: Vec<RuntimeProtocolEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRunRecord {
    pub trace_id: Option<String>,
    pub runtime_agent_id: String,
    pub agent_definition_id: String,
    pub agent_definition_version: i64,
    pub system_prompt: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewMessageRecord {
    pub project_id: String,
    pub run_id: String,
    pub role: MessageRole,
    pub content: String,
    pub provider_metadata: Option<RuntimeMessageProviderMetadata>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRuntimeEvent {
    pub project_id: String,
    pub run_id: String,
    pub event_kind: RuntimeEventKind,
    pub trace: Option<RuntimeTraceContext>,
    pub payload: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewContextManifest {
    pub manifest_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub turn_index: usize,
    pub context_hash: String,
    pub trace: Option<RuntimeTraceContext>,
    pub manifest: JsonValue,
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryAgentCoreStore {
    inner: Arc<Mutex<InMemoryAgentCoreState>>,
}

#[derive(Debug, Default)]
struct InMemoryAgentCoreState {
    runs: BTreeMap<(String, String), StoredRun>,
    next_message_id: i64,
    next_event_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRun {
    trace_id: String,
    #[serde(default = "default_runtime_agent_id")]
    runtime_agent_id: String,
    #[serde(default = "default_agent_definition_id")]
    agent_definition_id: String,
    #[serde(default = "default_agent_definition_version")]
    agent_definition_version: i64,
    #[serde(default = "default_system_prompt")]
    system_prompt: String,
    project_id: String,
    agent_session_id: String,
    run_id: String,
    provider_id: String,
    model_id: String,
    status: RunStatus,
    prompt: String,
    messages: Vec<RuntimeMessage>,
    events: Vec<RuntimeEvent>,
    context_manifests: Vec<ContextManifest>,
}

fn default_runtime_agent_id() -> String {
    "engineer".into()
}

fn default_agent_definition_id() -> String {
    "engineer".into()
}

fn default_agent_definition_version() -> i64 {
    1
}

fn default_system_prompt() -> String {
    "Xero CLI production runtime.".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunSummary {
    pub trace_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: RunStatus,
    pub prompt: String,
    pub message_count: usize,
    pub event_count: usize,
    pub context_manifest_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl AgentCoreStore for InMemoryAgentCoreStore {
    fn insert_run(&self, run: NewRunRecord) -> CoreResult<RunSnapshot> {
        validate_required(&run.project_id, "projectId")?;
        validate_required(&run.agent_session_id, "agentSessionId")?;
        validate_required(&run.run_id, "runId")?;
        validate_required(&run.prompt, "prompt")?;
        validate_required(&run.provider_id, "providerId")?;
        validate_required(&run.model_id, "modelId")?;
        validate_required(&run.runtime_agent_id, "runtimeAgentId")?;
        validate_required(&run.agent_definition_id, "agentDefinitionId")?;
        validate_required(&run.system_prompt, "systemPrompt")?;

        let mut state = self.lock_state()?;
        let key = (run.project_id.clone(), run.run_id.clone());
        if state.runs.contains_key(&key) {
            return Err(CoreError::invalid_request(
                "agent_core_run_exists",
                format!(
                    "Run `{}` already exists in project `{}`.",
                    run.run_id, run.project_id
                ),
            ));
        }
        state.runs.insert(
            key.clone(),
            StoredRun {
                trace_id: run
                    .trace_id
                    .unwrap_or_else(|| runtime_trace_id_for_run(&run.project_id, &run.run_id)),
                runtime_agent_id: run.runtime_agent_id,
                agent_definition_id: run.agent_definition_id,
                agent_definition_version: run.agent_definition_version,
                system_prompt: run.system_prompt,
                project_id: run.project_id,
                agent_session_id: run.agent_session_id,
                run_id: run.run_id,
                provider_id: run.provider_id,
                model_id: run.model_id,
                status: RunStatus::Starting,
                prompt: run.prompt,
                messages: Vec::new(),
                events: Vec::new(),
                context_manifests: Vec::new(),
            },
        );
        state.snapshot_for_key(&key)
    }

    fn load_run(&self, project_id: &str, run_id: &str) -> CoreResult<RunSnapshot> {
        let state = self.lock_state()?;
        state.snapshot_for_key(&(project_id.to_string(), run_id.to_string()))
    }

    fn append_message(&self, message: NewMessageRecord) -> CoreResult<RunSnapshot> {
        validate_required(&message.project_id, "projectId")?;
        validate_required(&message.run_id, "runId")?;
        let mut state = self.lock_state()?;
        let key = (message.project_id.clone(), message.run_id.clone());
        let id = state.next_message_id.saturating_add(1);
        state.next_message_id = id;
        let run = state.run_mut(&key)?;
        run.messages.push(RuntimeMessage {
            id,
            project_id: message.project_id,
            run_id: message.run_id,
            role: message.role,
            content: message.content,
            provider_metadata: message.provider_metadata,
            created_at: now_timestamp(),
        });
        state.snapshot_for_key(&key)
    }

    fn append_event(&self, event: NewRuntimeEvent) -> CoreResult<RuntimeEvent> {
        validate_required(&event.project_id, "projectId")?;
        validate_required(&event.run_id, "runId")?;
        let mut state = self.lock_state()?;
        let key = (event.project_id.clone(), event.run_id.clone());
        let id = state.next_event_id.saturating_add(1);
        state.next_event_id = id;
        let run_trace_id = state.run_mut(&key)?.trace_id.clone();
        let trace = event.trace.unwrap_or_else(|| {
            RuntimeTraceContext::for_event(&run_trace_id, &event.run_id, id, &event.event_kind)
        });
        let runtime_event = RuntimeEvent {
            id,
            project_id: event.project_id,
            run_id: event.run_id,
            event_kind: event.event_kind,
            trace,
            payload: event.payload,
            created_at: now_timestamp(),
        };
        state.run_mut(&key)?.events.push(runtime_event.clone());
        Ok(runtime_event)
    }

    fn record_context_manifest(&self, manifest: NewContextManifest) -> CoreResult<ContextManifest> {
        validate_required(&manifest.project_id, "projectId")?;
        validate_required(&manifest.agent_session_id, "agentSessionId")?;
        validate_required(&manifest.run_id, "runId")?;
        validate_required(&manifest.manifest_id, "manifestId")?;
        validate_required(&manifest.context_hash, "contextHash")?;
        let mut state = self.lock_state()?;
        let key = (manifest.project_id.clone(), manifest.run_id.clone());
        let recorded_after_event_id = state
            .runs
            .get(&key)
            .and_then(|run| run.events.last().map(|event| event.id));
        let run_trace_id = state.run_mut(&key)?.trace_id.clone();
        let trace = manifest.trace.unwrap_or_else(|| {
            RuntimeTraceContext::for_context_manifest(
                &run_trace_id,
                &manifest.run_id,
                &manifest.manifest_id,
                manifest.turn_index,
            )
        });
        let context_manifest = ContextManifest {
            manifest_id: manifest.manifest_id,
            project_id: manifest.project_id,
            agent_session_id: manifest.agent_session_id,
            run_id: manifest.run_id,
            provider_id: manifest.provider_id,
            model_id: manifest.model_id,
            turn_index: manifest.turn_index,
            context_hash: manifest.context_hash,
            recorded_after_event_id,
            trace,
            manifest: manifest.manifest,
            created_at: now_timestamp(),
        };
        state
            .run_mut(&key)?
            .context_manifests
            .push(context_manifest.clone());
        Ok(context_manifest)
    }

    fn update_run_status(
        &self,
        project_id: &str,
        run_id: &str,
        status: RunStatus,
    ) -> CoreResult<RunSnapshot> {
        let mut state = self.lock_state()?;
        let key = (project_id.to_string(), run_id.to_string());
        state.run_mut(&key)?.status = status;
        state.snapshot_for_key(&key)
    }

    fn export_trace(&self, project_id: &str, run_id: &str) -> CoreResult<RuntimeTrace> {
        RuntimeTrace::from_snapshot(self.load_run(project_id, run_id)?)
    }

    fn latest_run_for_session(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> CoreResult<RunSnapshot> {
        validate_required(project_id, "projectId")?;
        validate_required(agent_session_id, "agentSessionId")?;
        let state = self.lock_state()?;
        let mut matches = state
            .runs
            .values()
            .filter(|run| run.project_id == project_id && run.agent_session_id == agent_session_id)
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .summary()
                .updated_at
                .cmp(&left.summary().updated_at)
                .then_with(|| right.summary().started_at.cmp(&left.summary().started_at))
                .then_with(|| right.run_id.cmp(&left.run_id))
        });
        matches.first().map(|run| run.snapshot()).ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_session_run_not_found",
                format!(
                    "No run was found for session `{agent_session_id}` in project `{project_id}`."
                ),
            )
        })
    }
}

impl InMemoryAgentCoreStore {
    fn lock_state(&self) -> CoreResult<std::sync::MutexGuard<'_, InMemoryAgentCoreState>> {
        self.inner.lock().map_err(|_| {
            CoreError::system_fault(
                "agent_core_store_lock_failed",
                "The in-memory agent core store lock was poisoned.",
            )
        })
    }
}

impl InMemoryAgentCoreState {
    fn run_mut(&mut self, key: &(String, String)) -> CoreResult<&mut StoredRun> {
        self.runs.get_mut(key).ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_run_not_found",
                format!("Run `{}` was not found in project `{}`.", key.1, key.0),
            )
        })
    }

    fn snapshot_for_key(&self, key: &(String, String)) -> CoreResult<RunSnapshot> {
        let run = self.runs.get(key).ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_run_not_found",
                format!("Run `{}` was not found in project `{}`.", key.1, key.0),
            )
        })?;
        Ok(run.snapshot())
    }
}

impl StoredRun {
    fn snapshot(&self) -> RunSnapshot {
        RunSnapshot {
            trace_id: self.trace_id.clone(),
            runtime_agent_id: self.runtime_agent_id.clone(),
            agent_definition_id: self.agent_definition_id.clone(),
            agent_definition_version: self.agent_definition_version,
            system_prompt: self.system_prompt.clone(),
            project_id: self.project_id.clone(),
            agent_session_id: self.agent_session_id.clone(),
            run_id: self.run_id.clone(),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            status: self.status.clone(),
            prompt: self.prompt.clone(),
            messages: self.messages.clone(),
            events: self.events.clone(),
            context_manifests: self.context_manifests.clone(),
        }
    }

    fn summary(&self) -> RunSummary {
        RunSummary {
            trace_id: self.trace_id.clone(),
            project_id: self.project_id.clone(),
            agent_session_id: self.agent_session_id.clone(),
            run_id: self.run_id.clone(),
            provider_id: self.provider_id.clone(),
            model_id: self.model_id.clone(),
            status: self.status.clone(),
            prompt: self.prompt.clone(),
            message_count: self.messages.len(),
            event_count: self.events.len(),
            context_manifest_count: self.context_manifests.len(),
            started_at: self.events.first().map(|event| event.created_at.clone()),
            updated_at: self
                .events
                .last()
                .map(|event| event.created_at.clone())
                .or_else(|| {
                    self.messages
                        .last()
                        .map(|message| message.created_at.clone())
                }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct FileAgentCoreStore {
    path: Arc<PathBuf>,
    inner: Arc<Mutex<InMemoryAgentCoreState>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PersistedAgentCoreState {
    schema_version: u32,
    next_message_id: i64,
    next_event_id: i64,
    runs: Vec<StoredRun>,
}

const FILE_AGENT_CORE_STORE_SCHEMA_VERSION: u32 = 2;

impl FileAgentCoreStore {
    pub fn open(path: impl Into<PathBuf>) -> CoreResult<Self> {
        let path = path.into();
        let state = if path.exists() {
            read_file_agent_core_state(&path)?
        } else {
            InMemoryAgentCoreState::default()
        };
        Ok(Self {
            path: Arc::new(path),
            inner: Arc::new(Mutex::new(state)),
        })
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn list_runs(&self) -> CoreResult<Vec<RunSummary>> {
        let state = self.lock_state()?;
        let mut summaries = state
            .runs
            .values()
            .map(StoredRun::summary)
            .collect::<Vec<_>>();
        summaries.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.started_at.cmp(&left.started_at))
                .then_with(|| left.project_id.cmp(&right.project_id))
                .then_with(|| left.run_id.cmp(&right.run_id))
        });
        Ok(summaries)
    }

    pub fn list_project_runs(&self, project_id: &str) -> CoreResult<Vec<RunSummary>> {
        Ok(self
            .list_runs()?
            .into_iter()
            .filter(|summary| summary.project_id == project_id)
            .collect())
    }

    pub fn latest_run_for_session(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> CoreResult<RunSnapshot> {
        validate_required(project_id, "projectId")?;
        validate_required(agent_session_id, "agentSessionId")?;
        let state = self.lock_state()?;
        let mut matches = state
            .runs
            .values()
            .filter(|run| run.project_id == project_id && run.agent_session_id == agent_session_id)
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            right
                .summary()
                .updated_at
                .cmp(&left.summary().updated_at)
                .then_with(|| right.summary().started_at.cmp(&left.summary().started_at))
                .then_with(|| right.run_id.cmp(&left.run_id))
        });
        matches.first().map(|run| run.snapshot()).ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_session_run_not_found",
                format!(
                    "No run was found for session `{agent_session_id}` in project `{project_id}`."
                ),
            )
        })
    }

    pub fn load_run_by_id(&self, run_id: &str) -> CoreResult<RunSnapshot> {
        validate_required(run_id, "runId")?;
        let state = self.lock_state()?;
        let mut matches = state
            .runs
            .values()
            .filter(|run| run.run_id == run_id)
            .map(StoredRun::snapshot)
            .collect::<Vec<_>>();
        match matches.len() {
            0 => Err(CoreError::invalid_request(
                "agent_core_run_not_found",
                format!("Run `{run_id}` was not found."),
            )),
            1 => Ok(matches.remove(0)),
            _ => Err(CoreError::invalid_request(
                "agent_core_run_ambiguous",
                format!("Run `{run_id}` exists in multiple projects. Pass `--project-id`."),
            )),
        }
    }

    fn lock_state(&self) -> CoreResult<std::sync::MutexGuard<'_, InMemoryAgentCoreState>> {
        self.inner.lock().map_err(|_| {
            CoreError::system_fault(
                "agent_core_store_lock_failed",
                "The file-backed agent core store lock was poisoned.",
            )
        })
    }

    fn persist_locked(&self, state: &InMemoryAgentCoreState) -> CoreResult<()> {
        let parent = self.path.parent().ok_or_else(|| {
            CoreError::invalid_request(
                "agent_core_store_path_invalid",
                format!(
                    "The file-backed agent core store path `{}` has no parent directory.",
                    self.path.display()
                ),
            )
        })?;
        fs::create_dir_all(parent).map_err(|error| {
            CoreError::system_fault(
                "agent_core_store_prepare_failed",
                format!(
                    "Xero could not create the headless agent store directory `{}`: {error}",
                    parent.display()
                ),
            )
        })?;

        let persisted = PersistedAgentCoreState {
            schema_version: FILE_AGENT_CORE_STORE_SCHEMA_VERSION,
            next_message_id: state.next_message_id,
            next_event_id: state.next_event_id,
            runs: state.runs.values().cloned().collect(),
        };
        let payload = serde_json::to_vec_pretty(&persisted).map_err(|error| {
            CoreError::system_fault(
                "agent_core_store_encode_failed",
                format!("Xero could not encode the headless agent store: {error}"),
            )
        })?;
        let tmp_path = self.path.with_extension("json.tmp");
        fs::write(&tmp_path, payload).map_err(|error| {
            CoreError::system_fault(
                "agent_core_store_write_failed",
                format!(
                    "Xero could not write the headless agent store `{}`: {error}",
                    tmp_path.display()
                ),
            )
        })?;
        #[cfg(target_os = "windows")]
        if self.path.exists() {
            fs::remove_file(self.path.as_ref()).map_err(|error| {
                CoreError::system_fault(
                    "agent_core_store_replace_failed",
                    format!(
                        "Xero could not replace the headless agent store `{}`: {error}",
                        self.path.display()
                    ),
                )
            })?;
        }
        fs::rename(&tmp_path, self.path.as_ref()).map_err(|error| {
            CoreError::system_fault(
                "agent_core_store_commit_failed",
                format!(
                    "Xero could not commit the headless agent store `{}`: {error}",
                    self.path.display()
                ),
            )
        })
    }
}

impl AgentCoreStore for FileAgentCoreStore {
    fn runtime_store_descriptor(&self, project_id: &str) -> RuntimeStoreDescriptor {
        RuntimeStoreDescriptor::file_backed_headless_json(project_id, self.path.as_ref().clone())
    }

    fn insert_run(&self, run: NewRunRecord) -> CoreResult<RunSnapshot> {
        validate_required(&run.project_id, "projectId")?;
        validate_required(&run.agent_session_id, "agentSessionId")?;
        validate_required(&run.run_id, "runId")?;
        validate_required(&run.prompt, "prompt")?;
        validate_required(&run.provider_id, "providerId")?;
        validate_required(&run.model_id, "modelId")?;
        validate_required(&run.runtime_agent_id, "runtimeAgentId")?;
        validate_required(&run.agent_definition_id, "agentDefinitionId")?;
        validate_required(&run.system_prompt, "systemPrompt")?;

        let mut state = self.lock_state()?;
        let key = (run.project_id.clone(), run.run_id.clone());
        if state.runs.contains_key(&key) {
            return Err(CoreError::invalid_request(
                "agent_core_run_exists",
                format!(
                    "Run `{}` already exists in project `{}`.",
                    run.run_id, run.project_id
                ),
            ));
        }
        state.runs.insert(
            key.clone(),
            StoredRun {
                trace_id: run
                    .trace_id
                    .unwrap_or_else(|| runtime_trace_id_for_run(&run.project_id, &run.run_id)),
                runtime_agent_id: run.runtime_agent_id,
                agent_definition_id: run.agent_definition_id,
                agent_definition_version: run.agent_definition_version,
                system_prompt: run.system_prompt,
                project_id: run.project_id,
                agent_session_id: run.agent_session_id,
                run_id: run.run_id,
                provider_id: run.provider_id,
                model_id: run.model_id,
                status: RunStatus::Starting,
                prompt: run.prompt,
                messages: Vec::new(),
                events: Vec::new(),
                context_manifests: Vec::new(),
            },
        );
        let snapshot = state.snapshot_for_key(&key)?;
        self.persist_locked(&state)?;
        Ok(snapshot)
    }

    fn load_run(&self, project_id: &str, run_id: &str) -> CoreResult<RunSnapshot> {
        let state = self.lock_state()?;
        state.snapshot_for_key(&(project_id.to_string(), run_id.to_string()))
    }

    fn append_message(&self, message: NewMessageRecord) -> CoreResult<RunSnapshot> {
        validate_required(&message.project_id, "projectId")?;
        validate_required(&message.run_id, "runId")?;
        let mut state = self.lock_state()?;
        let key = (message.project_id.clone(), message.run_id.clone());
        let id = state.next_message_id.saturating_add(1);
        state.next_message_id = id;
        let run = state.run_mut(&key)?;
        run.messages.push(RuntimeMessage {
            id,
            project_id: message.project_id,
            run_id: message.run_id,
            role: message.role,
            content: message.content,
            provider_metadata: message.provider_metadata,
            created_at: now_timestamp(),
        });
        let snapshot = state.snapshot_for_key(&key)?;
        self.persist_locked(&state)?;
        Ok(snapshot)
    }

    fn append_event(&self, event: NewRuntimeEvent) -> CoreResult<RuntimeEvent> {
        validate_required(&event.project_id, "projectId")?;
        validate_required(&event.run_id, "runId")?;
        let mut state = self.lock_state()?;
        let key = (event.project_id.clone(), event.run_id.clone());
        let id = state.next_event_id.saturating_add(1);
        state.next_event_id = id;
        let run_trace_id = state.run_mut(&key)?.trace_id.clone();
        let trace = event.trace.unwrap_or_else(|| {
            RuntimeTraceContext::for_event(&run_trace_id, &event.run_id, id, &event.event_kind)
        });
        let runtime_event = RuntimeEvent {
            id,
            project_id: event.project_id,
            run_id: event.run_id,
            event_kind: event.event_kind,
            trace,
            payload: event.payload,
            created_at: now_timestamp(),
        };
        state.run_mut(&key)?.events.push(runtime_event.clone());
        self.persist_locked(&state)?;
        Ok(runtime_event)
    }

    fn record_context_manifest(&self, manifest: NewContextManifest) -> CoreResult<ContextManifest> {
        validate_required(&manifest.project_id, "projectId")?;
        validate_required(&manifest.agent_session_id, "agentSessionId")?;
        validate_required(&manifest.run_id, "runId")?;
        validate_required(&manifest.manifest_id, "manifestId")?;
        validate_required(&manifest.context_hash, "contextHash")?;
        let mut state = self.lock_state()?;
        let key = (manifest.project_id.clone(), manifest.run_id.clone());
        let recorded_after_event_id = state
            .runs
            .get(&key)
            .and_then(|run| run.events.last().map(|event| event.id));
        let run_trace_id = state.run_mut(&key)?.trace_id.clone();
        let trace = manifest.trace.unwrap_or_else(|| {
            RuntimeTraceContext::for_context_manifest(
                &run_trace_id,
                &manifest.run_id,
                &manifest.manifest_id,
                manifest.turn_index,
            )
        });
        let context_manifest = ContextManifest {
            manifest_id: manifest.manifest_id,
            project_id: manifest.project_id,
            agent_session_id: manifest.agent_session_id,
            run_id: manifest.run_id,
            provider_id: manifest.provider_id,
            model_id: manifest.model_id,
            turn_index: manifest.turn_index,
            context_hash: manifest.context_hash,
            recorded_after_event_id,
            trace,
            manifest: manifest.manifest,
            created_at: now_timestamp(),
        };
        state
            .run_mut(&key)?
            .context_manifests
            .push(context_manifest.clone());
        self.persist_locked(&state)?;
        Ok(context_manifest)
    }

    fn update_run_status(
        &self,
        project_id: &str,
        run_id: &str,
        status: RunStatus,
    ) -> CoreResult<RunSnapshot> {
        let mut state = self.lock_state()?;
        let key = (project_id.to_string(), run_id.to_string());
        state.run_mut(&key)?.status = status;
        let snapshot = state.snapshot_for_key(&key)?;
        self.persist_locked(&state)?;
        Ok(snapshot)
    }

    fn export_trace(&self, project_id: &str, run_id: &str) -> CoreResult<RuntimeTrace> {
        RuntimeTrace::from_snapshot(self.load_run(project_id, run_id)?)
    }

    fn latest_run_for_session(
        &self,
        project_id: &str,
        agent_session_id: &str,
    ) -> CoreResult<RunSnapshot> {
        self.latest_run_for_session(project_id, agent_session_id)
    }
}

fn read_file_agent_core_state(path: &Path) -> CoreResult<InMemoryAgentCoreState> {
    let bytes = fs::read(path).map_err(|error| {
        CoreError::system_fault(
            "agent_core_store_read_failed",
            format!(
                "Xero could not read the headless agent store `{}`: {error}",
                path.display()
            ),
        )
    })?;
    if bytes.is_empty() {
        return Ok(InMemoryAgentCoreState::default());
    }
    let persisted = serde_json::from_slice::<PersistedAgentCoreState>(&bytes).map_err(|error| {
        CoreError::system_fault(
            "agent_core_store_decode_failed",
            format!(
                "Xero could not decode the headless agent store `{}`: {error}",
                path.display()
            ),
        )
    })?;
    if persisted.schema_version != FILE_AGENT_CORE_STORE_SCHEMA_VERSION {
        return Err(CoreError::invalid_request(
            "agent_core_store_schema_unsupported",
            format!(
                "Headless agent store schema `{}` is not supported; expected `{}`.",
                persisted.schema_version, FILE_AGENT_CORE_STORE_SCHEMA_VERSION
            ),
        ));
    }

    let mut state = InMemoryAgentCoreState {
        runs: BTreeMap::new(),
        next_message_id: persisted.next_message_id,
        next_event_id: persisted.next_event_id,
    };
    for run in persisted.runs {
        let key = (run.project_id.clone(), run.run_id.clone());
        if state.runs.insert(key.clone(), run).is_some() {
            return Err(CoreError::system_fault(
                "agent_core_store_duplicate_run",
                format!(
                    "Headless agent store `{}` contains duplicate run `{}` in project `{}`.",
                    path.display(),
                    key.1,
                    key.0
                ),
            ));
        }
    }
    Ok(state)
}

#[derive(Debug, Clone)]
pub struct FakeProviderRuntime<S = InMemoryAgentCoreStore> {
    store: S,
}

fn fake_provider_preflight_snapshot() -> ProviderPreflightSnapshot {
    provider_preflight_snapshot(ProviderPreflightInput {
        profile_id: "fake_provider".into(),
        provider_id: "fake_provider".into(),
        model_id: "fake-model".into(),
        source: ProviderPreflightSource::LiveProbe,
        checked_at: now_timestamp(),
        age_seconds: Some(0),
        ttl_seconds: None,
        required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        capabilities: provider_capability_catalog(ProviderCapabilityCatalogInput {
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            catalog_source: "live".into(),
            fetched_at: Some(now_timestamp()),
            last_success_at: Some(now_timestamp()),
            cache_age_seconds: Some(0),
            cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
            credential_proof: Some("none_required".into()),
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            context_limit_source: Some("built_in_registry".into()),
            context_limit_confidence: Some("high".into()),
            thinking_supported: false,
            thinking_efforts: Vec::new(),
            thinking_default_effort: None,
        }),
        credential_ready: Some(true),
        endpoint_reachable: Some(true),
        model_available: Some(true),
        streaming_route_available: Some(true),
        tool_schema_accepted: Some(true),
        reasoning_controls_accepted: None,
        attachments_accepted: None,
        context_limit_known: Some(true),
        provider_error: None,
    })
}

impl Default for FakeProviderRuntime<InMemoryAgentCoreStore> {
    fn default() -> Self {
        Self {
            store: InMemoryAgentCoreStore::default(),
        }
    }
}

impl<S> FakeProviderRuntime<S>
where
    S: AgentCoreStore,
{
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn store(&self) -> S {
        self.store.clone()
    }

    fn drive_fake_turn(
        &self,
        snapshot: &RunSnapshot,
        prompt: &str,
        turn_index: usize,
    ) -> CoreResult<()> {
        let provider_preflight = fake_provider_preflight_snapshot();
        self.store.record_context_manifest(NewContextManifest {
            manifest_id: format!("context-manifest-{}-{turn_index}", snapshot.run_id),
            project_id: snapshot.project_id.clone(),
            agent_session_id: snapshot.agent_session_id.clone(),
            run_id: snapshot.run_id.clone(),
            provider_id: snapshot.provider_id.clone(),
            model_id: snapshot.model_id.clone(),
            turn_index,
            context_hash: stable_context_hash(snapshot, prompt, turn_index),
            trace: Some(RuntimeTraceContext::for_context_manifest(
                &snapshot.trace_id,
                &snapshot.run_id,
                &format!("context-manifest-{}-{turn_index}", snapshot.run_id),
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
                "rawContextInjected": false,
                "promptChars": prompt.len(),
                "providerPreflight": provider_preflight,
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
                "manifestId": format!("context-manifest-{}-{turn_index}", snapshot.run_id),
                "contextHash": stable_context_hash(snapshot, prompt, turn_index),
                "turnIndex": turn_index,
            }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ReasoningSummary,
            trace: Some(RuntimeTraceContext::for_provider_turn(
                &snapshot.trace_id,
                &snapshot.run_id,
                turn_index,
            )),
            payload: json!({
                "text": "Loaded reusable Xero fake-provider harness context."
            }),
        })?;
        self.store.append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::Assistant,
            content: "Owned agent run completed through the reusable Xero core facade.".into(),
            provider_metadata: None,
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::MessageDelta,
            trace: Some(RuntimeTraceContext::for_provider_turn(
                &snapshot.trace_id,
                &snapshot.run_id,
                turn_index,
            )),
            payload: json!({
                "role": "assistant",
                "text": "Owned agent run completed through the reusable Xero core facade."
            }),
        })?;
        self.store.update_run_status(
            &snapshot.project_id,
            &snapshot.run_id,
            RunStatus::Completed,
        )?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::RunCompleted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "run_completed",
            )),
            payload: json!({
                "summary": "Reusable fake-provider run completed.",
                "state": "complete"
            }),
        })?;
        Ok(())
    }
}

impl<S> AgentRuntimeFacade for FakeProviderRuntime<S>
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
        if request.provider.provider_id != FAKE_PROVIDER_ID {
            return Err(CoreError::invalid_request(
                "agent_core_fake_provider_explicit_required",
                format!(
                    "Harness execution requires provider `{FAKE_PROVIDER_ID}`, got `{}`.",
                    request.provider.provider_id
                ),
            ));
        }
        let runtime_contract = ProductionRuntimeContract::fake_provider_harness(
            "headless_harness",
            request.project_id.clone(),
            request.provider.model_id.clone(),
            self.store.runtime_store_descriptor(&request.project_id),
        );
        validate_production_runtime_contract(&runtime_contract)?;
        let provider_preflight = fake_provider_preflight_snapshot();
        let runtime_agent_id = request
            .controls
            .as_ref()
            .map(|controls| controls.runtime_agent_id.trim())
            .filter(|id| !id.is_empty())
            .unwrap_or("engineer")
            .to_owned();
        let agent_definition_id = request
            .controls
            .as_ref()
            .and_then(|controls| controls.agent_definition_id.as_deref())
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .unwrap_or(runtime_agent_id.as_str())
            .to_owned();
        let agent_definition_version = request
            .controls
            .as_ref()
            .and_then(|controls| controls.agent_definition_version)
            .filter(|version| *version > 0)
            .unwrap_or(1);
        let snapshot = self.store.insert_run(NewRunRecord {
            trace_id: None,
            runtime_agent_id,
            agent_definition_id,
            agent_definition_version,
            system_prompt: "Reusable Xero fake-provider system prompt.".into(),
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
                "runtimeContract": production_runtime_trace_metadata(&runtime_contract),
                "providerPreflight": provider_preflight,
            }),
        })?;
        let lifecycle = EnvironmentLifecycleService::new(self.store.clone());
        let environment = lifecycle.start_environment(EnvironmentLifecycleConfig {
            environment_id: format!("env-{}-{}", snapshot.project_id, snapshot.run_id),
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            provider_credentials_required: false,
            provider_credentials_valid: true,
            tool_packs: vec!["owned_agent_core".into()],
            ..EnvironmentLifecycleConfig::local(&snapshot.project_id, &snapshot.run_id)
        })?;
        if !environment.state.is_ready() {
            return self.store.load_run(&snapshot.project_id, &snapshot.run_id);
        }
        self.store.append_message(NewMessageRecord {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            role: MessageRole::System,
            content: "Reusable Xero fake-provider system prompt.".into(),
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
                "validation_started",
            )),
            payload: json!({ "label": "repo_preflight" }),
        })?;
        self.store.append_event(NewRuntimeEvent {
            project_id: snapshot.project_id.clone(),
            run_id: snapshot.run_id.clone(),
            event_kind: RuntimeEventKind::ValidationCompleted,
            trace: Some(RuntimeTraceContext::for_run(
                &snapshot.trace_id,
                &snapshot.run_id,
                "validation_completed",
            )),
            payload: json!({ "label": "repo_preflight", "outcome": "passed" }),
        })?;
        let started = self
            .store
            .load_run(&snapshot.project_id, &snapshot.run_id)?;
        self.drive_fake_turn(&started, &request.prompt, 0)?;
        self.store.load_run(&snapshot.project_id, &snapshot.run_id)
    }

    fn continue_run(&self, request: ContinueRunRequest) -> CoreResult<RunSnapshot> {
        let before = self.store.load_run(&request.project_id, &request.run_id)?;
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
        let turn_index = before.context_manifests.len();
        let snapshot = self.store.load_run(&request.project_id, &request.run_id)?;
        self.drive_fake_turn(&snapshot, &request.prompt, turn_index)?;
        self.store.load_run(&request.project_id, &request.run_id)
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
                    "Approved action `{}` through reusable core facade.",
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
            trace: Some(RuntimeTraceContext::for_approval(
                &self
                    .store
                    .load_run(&request.project_id, &request.run_id)?
                    .trace_id,
                &request.run_id,
                &request.action_id,
            )),
            payload: json!({
                "kind": "approval",
                "actionId": request.action_id,
                "decision": "rejected",
                "response": request.response,
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

    fn fork_session(&self, _request: ForkSessionRequest) -> CoreResult<RunSnapshot> {
        Err(CoreError::unsupported("fork_session"))
    }

    fn compact_session(&self, request: CompactSessionRequest) -> CoreResult<RunSnapshot> {
        let _ = request;
        Err(CoreError::unsupported("compact_session"))
    }

    fn export_trace(&self, request: ExportTraceRequest) -> CoreResult<RuntimeTrace> {
        self.store
            .export_trace(&request.project_id, &request.run_id)
    }
}

impl<S> AgentProtocolRuntime for FakeProviderRuntime<S>
where
    S: AgentCoreStore,
{
    fn submit_protocol(
        &self,
        submission: RuntimeSubmissionEnvelope,
    ) -> CoreResult<RuntimeSubmissionOutcome> {
        submission.validate_protocol_version()?;
        if !submission.trace.is_valid() {
            return Err(CoreError::invalid_request(
                "agent_protocol_trace_invalid",
                "The protocol submission trace context is malformed.",
            ));
        }

        let accepted_submission_id = submission.submission_id;
        let trace = submission.trace;
        let mut trace_export = None;
        let snapshot = match submission.submission {
            RuntimeSubmission::StartRun(request) => Some(self.start_run(request)?),
            RuntimeSubmission::ContinueRun(request) => Some(self.continue_run(request)?),
            RuntimeSubmission::UserMessage(request) => Some(self.submit_user_input(request)?),
            RuntimeSubmission::ApprovalDecision(request) => Some(self.approve_action(request)?),
            RuntimeSubmission::Cancel(request) => Some(self.cancel_run(request)?),
            RuntimeSubmission::Resume(request) => Some(self.resume_run(request)?),
            RuntimeSubmission::ExportTrace(request) => {
                trace_export = Some(self.export_trace(request)?);
                None
            }
            RuntimeSubmission::ToolPermissionGrant(request) => {
                let run = self.store.load_run(&request.project_id, &request.run_id)?;
                self.store.append_event(NewRuntimeEvent {
                    project_id: request.project_id.clone(),
                    run_id: request.run_id.clone(),
                    event_kind: RuntimeEventKind::ToolPermissionGrant,
                    trace: Some(RuntimeTraceContext::for_run(
                        &run.trace_id,
                        &request.run_id,
                        "tool_permission_grant",
                    )),
                    payload: json!({
                        "grantId": request.grant_id,
                        "toolName": request.tool_name,
                        "expiresAt": request.expires_at,
                    }),
                })?;
                Some(self.store.load_run(&request.project_id, &request.run_id)?)
            }
            RuntimeSubmission::ProviderModelChange(request) => {
                if let Some(run_id) = request.run_id {
                    let run = self.store.load_run(&request.project_id, &run_id)?;
                    self.store.append_event(NewRuntimeEvent {
                        project_id: request.project_id.clone(),
                        run_id: run_id.clone(),
                        event_kind: RuntimeEventKind::ProviderModelChanged,
                        trace: Some(RuntimeTraceContext::for_run(
                            &run.trace_id,
                            &run_id,
                            "provider_model_changed",
                        )),
                        payload: json!({
                            "providerId": request.provider.provider_id,
                            "modelId": request.provider.model_id,
                            "reason": request.reason,
                        }),
                    })?;
                    Some(self.store.load_run(&request.project_id, &run_id)?)
                } else {
                    None
                }
            }
            RuntimeSubmission::RuntimeSettingsChange(_) => None,
            RuntimeSubmission::Fork(request) => Some(self.fork_session(request)?),
            RuntimeSubmission::Compact(request) => Some(self.compact_session(request)?),
        };

        Ok(RuntimeSubmissionOutcome {
            protocol_version: CORE_PROTOCOL_VERSION,
            accepted_submission_id,
            trace,
            snapshot,
            trace_export,
        })
    }
}

fn validate_required(value: &str, field: &str) -> CoreResult<()> {
    if value.trim().is_empty() {
        return Err(CoreError::invalid_request(
            "agent_core_required_field_missing",
            format!("`{field}` is required."),
        ));
    }
    Ok(())
}

fn stable_context_hash(snapshot: &RunSnapshot, prompt: &str, turn_index: usize) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in format!(
        "{}:{}:{}:{}",
        snapshot.project_id, snapshot.run_id, turn_index, prompt
    )
    .bytes()
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

fn now_timestamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    format!("unix:{seconds}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_provider_records_manifest_before_provider_events() {
        let runtime = FakeProviderRuntime::default();

        let snapshot = runtime
            .start_run(StartRunRequest {
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
                prompt: "Explain phase one.".into(),
                provider: ProviderSelection {
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                },
                controls: Some(RunControls {
                    runtime_agent_id: "engineer".into(),
                    agent_definition_id: Some("engineer".into()),
                    agent_definition_version: Some(1),
                    thinking_effort: None,
                    approval_mode: "yolo".into(),
                    plan_mode_required: false,
                }),
            })
            .expect("fake run should complete");

        assert_eq!(snapshot.status, RunStatus::Completed);
        assert!(snapshot.events.iter().any(|event| event.event_kind
            == RuntimeEventKind::EnvironmentLifecycleUpdate
            && event.payload.get("state").and_then(JsonValue::as_str) == Some("ready")));
        let manifest = snapshot
            .context_manifests
            .first()
            .expect("provider turn context manifest");
        assert_eq!(manifest.turn_index, 0);
        let recorded_after_event_id = manifest
            .recorded_after_event_id
            .expect("manifest should record its event boundary");
        assert!(
            snapshot
                .events
                .iter()
                .find(|event| event.event_kind == RuntimeEventKind::ReasoningSummary)
                .expect("provider reasoning event")
                .id
                > recorded_after_event_id,
            "provider events must happen after context persistence"
        );
        assert_eq!(snapshot.trace_id.len(), 32);
        assert!(snapshot.events.iter().all(|event| event.trace.is_valid()));
    }
}
