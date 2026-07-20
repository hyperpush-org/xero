use std::{
    collections::{BTreeMap, BTreeSet},
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
    /// Immutable Agent definition selected for this run. The reusable headless
    /// runtime uses the snapshot to enforce the same prompt, tool-policy, and
    /// Stage boundaries as the desktop runtime instead of treating the
    /// definition id as display-only metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_snapshot: Option<JsonValue>,
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
    AssistantCandidate,
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
    RouteRequested,
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

#[derive(Debug, Clone, Default)]
struct InMemoryAgentCoreState {
    runs: BTreeMap<(String, String), StoredRun>,
    next_message_id: i64,
    next_event_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredRun {
    trace_id: String,
    runtime_agent_id: String,
    agent_definition_id: String,
    agent_definition_version: i64,
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
        validate_agent_definition_version(run.agent_definition_version)?;
        validate_optional_run_trace_id(run.trace_id.as_deref())?;

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
        validate_required(project_id, "projectId")?;
        validate_required(run_id, "runId")?;
        let state = self.lock_state()?;
        state.snapshot_for_key(&(project_id.to_string(), run_id.to_string()))
    }

    fn append_message(&self, message: NewMessageRecord) -> CoreResult<RunSnapshot> {
        validate_required(&message.project_id, "projectId")?;
        validate_required(&message.run_id, "runId")?;
        let mut state = self.lock_state()?;
        let key = (message.project_id.clone(), message.run_id.clone());
        state.run_mut(&key)?;
        let id = allocate_record_id(&mut state.next_message_id, "message")?;
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
        let run_trace_id = state.run_mut(&key)?.trace_id.clone();
        if let Some(trace) = event.trace.as_ref() {
            validate_stored_trace_context(trace, &run_trace_id)?;
        }
        let id = allocate_record_id(&mut state.next_event_id, "event")?;
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
        validate_required(&manifest.provider_id, "providerId")?;
        validate_required(&manifest.model_id, "modelId")?;
        validate_required(&manifest.context_hash, "contextHash")?;
        let mut state = self.lock_state()?;
        let key = (manifest.project_id.clone(), manifest.run_id.clone());
        let recorded_after_event_id = state
            .runs
            .get(&key)
            .and_then(|run| run.events.last().map(|event| event.id));
        let (run_trace_id, run_agent_session_id) = {
            let run = state.run_mut(&key)?;
            (run.trace_id.clone(), run.agent_session_id.clone())
        };
        validate_manifest_run_session(&manifest, &run_agent_session_id)?;
        validate_manifest_identity_available(&state, &manifest.project_id, &manifest.manifest_id)?;
        if let Some(trace) = manifest.trace.as_ref() {
            validate_context_manifest_trace(
                trace,
                &run_trace_id,
                &manifest.run_id,
                &manifest.manifest_id,
                manifest.turn_index,
            )?;
        }
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
        validate_required(project_id, "projectId")?;
        validate_required(run_id, "runId")?;
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
        if path.as_os_str().is_empty() {
            return Err(CoreError::invalid_request(
                "agent_core_store_path_invalid",
                "The file-backed agent core store path cannot be empty.",
            ));
        }
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
        validate_required(project_id, "projectId")?;
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
            if let Err(error) = fs::remove_file(self.path.as_ref()) {
                let _ = fs::remove_file(&tmp_path);
                return Err(CoreError::system_fault(
                    "agent_core_store_replace_failed",
                    format!(
                        "Xero could not replace the headless agent store `{}`: {error}",
                        self.path.display()
                    ),
                ));
            }
        }
        if let Err(error) = fs::rename(&tmp_path, self.path.as_ref()) {
            let _ = fs::remove_file(&tmp_path);
            return Err(CoreError::system_fault(
                "agent_core_store_commit_failed",
                format!(
                    "Xero could not commit the headless agent store `{}`: {error}",
                    self.path.display()
                ),
            ));
        }
        Ok(())
    }

    fn persist_or_restore(
        &self,
        state: &mut InMemoryAgentCoreState,
        previous: InMemoryAgentCoreState,
    ) -> CoreResult<()> {
        if let Err(error) = self.persist_locked(state) {
            *state = previous;
            return Err(error);
        }
        Ok(())
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
        validate_agent_definition_version(run.agent_definition_version)?;
        validate_optional_run_trace_id(run.trace_id.as_deref())?;

        let mut state = self.lock_state()?;
        let previous = state.clone();
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
        self.persist_or_restore(&mut state, previous)?;
        Ok(snapshot)
    }

    fn load_run(&self, project_id: &str, run_id: &str) -> CoreResult<RunSnapshot> {
        validate_required(project_id, "projectId")?;
        validate_required(run_id, "runId")?;
        let state = self.lock_state()?;
        state.snapshot_for_key(&(project_id.to_string(), run_id.to_string()))
    }

    fn append_message(&self, message: NewMessageRecord) -> CoreResult<RunSnapshot> {
        validate_required(&message.project_id, "projectId")?;
        validate_required(&message.run_id, "runId")?;
        let mut state = self.lock_state()?;
        let previous = state.clone();
        let key = (message.project_id.clone(), message.run_id.clone());
        state.run_mut(&key)?;
        let id = allocate_record_id(&mut state.next_message_id, "message")?;
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
        self.persist_or_restore(&mut state, previous)?;
        Ok(snapshot)
    }

    fn append_event(&self, event: NewRuntimeEvent) -> CoreResult<RuntimeEvent> {
        validate_required(&event.project_id, "projectId")?;
        validate_required(&event.run_id, "runId")?;
        let mut state = self.lock_state()?;
        let previous = state.clone();
        let key = (event.project_id.clone(), event.run_id.clone());
        let run_trace_id = state.run_mut(&key)?.trace_id.clone();
        if let Some(trace) = event.trace.as_ref() {
            validate_stored_trace_context(trace, &run_trace_id)?;
        }
        let id = allocate_record_id(&mut state.next_event_id, "event")?;
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
        self.persist_or_restore(&mut state, previous)?;
        Ok(runtime_event)
    }

    fn record_context_manifest(&self, manifest: NewContextManifest) -> CoreResult<ContextManifest> {
        validate_required(&manifest.project_id, "projectId")?;
        validate_required(&manifest.agent_session_id, "agentSessionId")?;
        validate_required(&manifest.run_id, "runId")?;
        validate_required(&manifest.manifest_id, "manifestId")?;
        validate_required(&manifest.provider_id, "providerId")?;
        validate_required(&manifest.model_id, "modelId")?;
        validate_required(&manifest.context_hash, "contextHash")?;
        let mut state = self.lock_state()?;
        let previous = state.clone();
        let key = (manifest.project_id.clone(), manifest.run_id.clone());
        let recorded_after_event_id = state
            .runs
            .get(&key)
            .and_then(|run| run.events.last().map(|event| event.id));
        let (run_trace_id, run_agent_session_id) = {
            let run = state.run_mut(&key)?;
            (run.trace_id.clone(), run.agent_session_id.clone())
        };
        validate_manifest_run_session(&manifest, &run_agent_session_id)?;
        validate_manifest_identity_available(&state, &manifest.project_id, &manifest.manifest_id)?;
        if let Some(trace) = manifest.trace.as_ref() {
            validate_context_manifest_trace(
                trace,
                &run_trace_id,
                &manifest.run_id,
                &manifest.manifest_id,
                manifest.turn_index,
            )?;
        }
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
        self.persist_or_restore(&mut state, previous)?;
        Ok(context_manifest)
    }

    fn update_run_status(
        &self,
        project_id: &str,
        run_id: &str,
        status: RunStatus,
    ) -> CoreResult<RunSnapshot> {
        validate_required(project_id, "projectId")?;
        validate_required(run_id, "runId")?;
        let mut state = self.lock_state()?;
        let previous = state.clone();
        let key = (project_id.to_string(), run_id.to_string());
        state.run_mut(&key)?.status = status;
        let snapshot = state.snapshot_for_key(&key)?;
        self.persist_or_restore(&mut state, previous)?;
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

    let mut message_ids = BTreeSet::new();
    let mut event_ids = BTreeSet::new();
    let mut manifest_ids = BTreeSet::new();
    let mut max_message_id = 0;
    let mut max_event_id = 0;
    for run in &persisted.runs {
        validate_persisted_run(path, run)?;
        for message in &run.messages {
            if message.id <= 0 || !message_ids.insert(message.id) {
                return Err(file_store_invariant_error(
                    "agent_core_store_id_counter_invalid",
                    path,
                    format!("message ID `{}` is non-positive or duplicated", message.id),
                ));
            }
            max_message_id = max_message_id.max(message.id);
        }
        for event in &run.events {
            if event.id <= 0 || !event_ids.insert(event.id) {
                return Err(file_store_invariant_error(
                    "agent_core_store_id_counter_invalid",
                    path,
                    format!("event ID `{}` is non-positive or duplicated", event.id),
                ));
            }
            max_event_id = max_event_id.max(event.id);
        }
        for manifest in &run.context_manifests {
            if !manifest_ids.insert((run.project_id.clone(), manifest.manifest_id.clone())) {
                return Err(file_store_invariant_error(
                    "agent_core_store_duplicate_manifest",
                    path,
                    format!(
                        "context manifest `{}` is duplicated in project `{}`",
                        manifest.manifest_id, run.project_id
                    ),
                ));
            }
        }
    }
    if persisted.next_message_id < max_message_id
        || persisted.next_event_id < max_event_id
        || persisted.next_message_id < 0
        || persisted.next_event_id < 0
    {
        return Err(file_store_invariant_error(
            "agent_core_store_id_counter_invalid",
            path,
            format!(
                "record counters ({}, {}) trail persisted message/event IDs ({max_message_id}, {max_event_id})",
                persisted.next_message_id, persisted.next_event_id
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

fn validate_persisted_run(path: &Path, run: &StoredRun) -> CoreResult<()> {
    let missing_field = [
        ("projectId", run.project_id.as_str()),
        ("agentSessionId", run.agent_session_id.as_str()),
        ("runId", run.run_id.as_str()),
        ("prompt", run.prompt.as_str()),
        ("providerId", run.provider_id.as_str()),
        ("modelId", run.model_id.as_str()),
        ("runtimeAgentId", run.runtime_agent_id.as_str()),
        ("agentDefinitionId", run.agent_definition_id.as_str()),
        ("systemPrompt", run.system_prompt.as_str()),
    ]
    .into_iter()
    .find(|(_, value)| value.trim().is_empty())
    .map(|(field, _)| field);
    if let Some(field) = missing_field {
        return Err(file_store_invariant_error(
            "agent_core_store_record_mismatch",
            path,
            format!("run `{}` has a blank required field `{field}`", run.run_id),
        ));
    }
    if run.agent_definition_version <= 0 {
        return Err(file_store_invariant_error(
            "agent_core_store_record_mismatch",
            path,
            format!(
                "run `{}` has a non-positive agent definition version",
                run.run_id
            ),
        ));
    }
    if validate_optional_run_trace_id(Some(&run.trace_id)).is_err() {
        return Err(file_store_invariant_error(
            "agent_core_store_trace_invalid",
            path,
            format!("run `{}` has an invalid trace ID", run.run_id),
        ));
    }
    for message in &run.messages {
        if message.project_id != run.project_id || message.run_id != run.run_id {
            return Err(file_store_invariant_error(
                "agent_core_store_record_mismatch",
                path,
                format!(
                    "message `{}` does not belong to run `{}` in project `{}`",
                    message.id, run.run_id, run.project_id
                ),
            ));
        }
        if message.created_at.trim().is_empty() {
            return Err(file_store_invariant_error(
                "agent_core_store_record_mismatch",
                path,
                format!("message `{}` has a blank creation timestamp", message.id),
            ));
        }
    }
    for event in &run.events {
        if event.project_id != run.project_id || event.run_id != run.run_id {
            return Err(file_store_invariant_error(
                "agent_core_store_record_mismatch",
                path,
                format!(
                    "event `{}` does not belong to run `{}` in project `{}`",
                    event.id, run.run_id, run.project_id
                ),
            ));
        }
        if event.created_at.trim().is_empty() {
            return Err(file_store_invariant_error(
                "agent_core_store_record_mismatch",
                path,
                format!("event `{}` has a blank creation timestamp", event.id),
            ));
        }
        if validate_stored_trace_context(&event.trace, &run.trace_id).is_err() {
            return Err(file_store_invariant_error(
                "agent_core_store_trace_invalid",
                path,
                format!("event `{}` has an invalid or cross-run trace", event.id),
            ));
        }
    }
    for manifest in &run.context_manifests {
        let missing_field = [
            ("manifestId", manifest.manifest_id.as_str()),
            ("providerId", manifest.provider_id.as_str()),
            ("modelId", manifest.model_id.as_str()),
            ("contextHash", manifest.context_hash.as_str()),
            ("createdAt", manifest.created_at.as_str()),
        ]
        .into_iter()
        .find(|(_, value)| value.trim().is_empty())
        .map(|(field, _)| field);
        if let Some(field) = missing_field {
            return Err(file_store_invariant_error(
                "agent_core_store_record_mismatch",
                path,
                format!(
                    "context manifest `{}` has a blank required field `{field}`",
                    manifest.manifest_id
                ),
            ));
        }
        if manifest.project_id != run.project_id
            || manifest.run_id != run.run_id
            || manifest.agent_session_id != run.agent_session_id
        {
            return Err(file_store_invariant_error(
                "agent_core_store_record_mismatch",
                path,
                format!(
                    "context manifest `{}` does not belong to run `{}` in project `{}`",
                    manifest.manifest_id, run.run_id, run.project_id
                ),
            ));
        }
        if manifest
            .recorded_after_event_id
            .is_some_and(|event_id| !run.events.iter().any(|event| event.id == event_id))
        {
            return Err(file_store_invariant_error(
                "agent_core_store_record_mismatch",
                path,
                format!(
                    "context manifest `{}` references an event outside run `{}`",
                    manifest.manifest_id, run.run_id
                ),
            ));
        }
        if validate_context_manifest_trace(
            &manifest.trace,
            &run.trace_id,
            &manifest.run_id,
            &manifest.manifest_id,
            manifest.turn_index,
        )
        .is_err()
        {
            return Err(file_store_invariant_error(
                "agent_core_store_trace_invalid",
                path,
                format!(
                    "context manifest `{}` has an invalid or cross-run trace",
                    manifest.manifest_id
                ),
            ));
        }
    }
    Ok(())
}

fn file_store_invariant_error(
    code: &'static str,
    path: &Path,
    detail: impl std::fmt::Display,
) -> CoreError {
    CoreError::system_fault(
        code,
        format!(
            "Headless agent store `{}` violates persisted invariants: {detail}.",
            path.display()
        ),
    )
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
            input_modalities: Vec::new(),
            input_modalities_source: Some("unknown".into()),
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
        identity: &headless_runtime::HeadlessRunIdentity,
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
        if let Some((stage_id, gate_message)) = identity.incomplete_stage_gate(snapshot) {
            self.store.append_event(NewRuntimeEvent {
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                event_kind: RuntimeEventKind::AssistantCandidate,
                trace: Some(RuntimeTraceContext::for_provider_turn(
                    &snapshot.trace_id,
                    &snapshot.run_id,
                    turn_index,
                )),
                payload: json!({
                    "state": "superseded",
                    "disposition": "stage_gate",
                    "stageId": stage_id,
                    "text": "Owned agent run completed through the reusable Xero core facade.",
                }),
            })?;
            self.store.append_event(NewRuntimeEvent {
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                event_kind: RuntimeEventKind::VerificationGate,
                trace: Some(RuntimeTraceContext::for_run(
                    &snapshot.trace_id,
                    &snapshot.run_id,
                    "stage_gate_blocked",
                )),
                payload: json!({
                    "state": "blocked",
                    "stageId": stage_id,
                    "message": gate_message,
                    "turnIndex": turn_index,
                }),
            })?;
            self.store.update_run_status(
                &snapshot.project_id,
                &snapshot.run_id,
                RunStatus::Failed,
            )?;
            self.store.append_event(NewRuntimeEvent {
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                event_kind: RuntimeEventKind::RunFailed,
                trace: Some(RuntimeTraceContext::for_run(
                    &snapshot.trace_id,
                    &snapshot.run_id,
                    "stage_gate_incomplete",
                )),
                payload: json!({
                    "code": "agent_core_fake_provider_stage_evidence_required",
                    "message": "The deterministic fake provider cannot complete an Agent Stage that requires tool or TODO evidence.",
                    "retryable": false,
                    "stageId": stage_id,
                }),
            })?;
            return Err(CoreError::invalid_request(
                "agent_core_fake_provider_stage_evidence_required",
                "The deterministic fake provider cannot complete an Agent Stage that requires tool or TODO evidence.",
            ));
        }
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
        let identity = headless_runtime::HeadlessRunIdentity::from_request(&request, None)?;
        let definition_runtime = identity.definition_runtime_json();
        let snapshot = self.store.insert_run(NewRunRecord {
            trace_id: None,
            runtime_agent_id: identity.runtime_agent_id.clone(),
            agent_definition_id: identity.agent_definition_id.clone(),
            agent_definition_version: identity.agent_definition_version,
            system_prompt: identity.system_prompt.clone(),
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
                "thinkingEffort": identity.thinking_effort.clone(),
                "approvalMode": identity.approval_mode.clone(),
                "agentDefinitionRuntime": definition_runtime,
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
            content: identity.system_prompt.clone(),
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
        self.drive_fake_turn(&started, &request.prompt, 0, &identity)?;
        self.store.load_run(&snapshot.project_id, &snapshot.run_id)
    }

    fn continue_run(&self, request: ContinueRunRequest) -> CoreResult<RunSnapshot> {
        validate_required(&request.prompt, "prompt")?;
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
        let identity = headless_runtime::HeadlessRunIdentity::from_snapshot(&snapshot)?;
        self.drive_fake_turn(&snapshot, &request.prompt, turn_index, &identity)?;
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
        validate_required(&request.project_id, "projectId")?;
        validate_required(&request.run_id, "runId")?;
        validate_required(&request.action_id, "actionId")?;
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
        validate_required(&request.project_id, "projectId")?;
        validate_required(&request.run_id, "runId")?;
        validate_required(&request.action_id, "actionId")?;
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
        validate_required(&submission.submission_id, "submissionId")?;
        validate_required(&submission.submitted_at, "submittedAt")?;
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
                validate_required(&request.project_id, "projectId")?;
                validate_required(&request.run_id, "runId")?;
                validate_required(&request.grant_id, "grantId")?;
                validate_required(&request.tool_name, "toolName")?;
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
                validate_required(&request.project_id, "projectId")?;
                validate_required(&request.provider.provider_id, "providerId")?;
                validate_required(&request.provider.model_id, "modelId")?;
                if let Some(run_id) = request.run_id.as_deref() {
                    validate_required(run_id, "runId")?;
                }
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
            RuntimeSubmission::RuntimeSettingsChange(request) => {
                if let Some(project_id) = request.project_id.as_deref() {
                    validate_required(project_id, "projectId")?;
                }
                None
            }
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

fn allocate_record_id(counter: &mut i64, record_kind: &str) -> CoreResult<i64> {
    let id = counter.checked_add(1).ok_or_else(|| {
        CoreError::system_fault(
            "agent_core_store_id_counter_exhausted",
            format!("The headless agent store exhausted its {record_kind} ID sequence."),
        )
    })?;
    *counter = id;
    Ok(id)
}

fn validate_agent_definition_version(version: i64) -> CoreResult<()> {
    if version <= 0 {
        return Err(CoreError::invalid_request(
            "agent_core_definition_version_invalid",
            "`agentDefinitionVersion` must be greater than zero.",
        ));
    }
    Ok(())
}

fn validate_manifest_identity_available(
    state: &InMemoryAgentCoreState,
    project_id: &str,
    manifest_id: &str,
) -> CoreResult<()> {
    if state.runs.values().any(|run| {
        run.project_id == project_id
            && run
                .context_manifests
                .iter()
                .any(|manifest| manifest.manifest_id == manifest_id)
    }) {
        return Err(CoreError::invalid_request(
            "agent_core_manifest_exists",
            format!("Context manifest `{manifest_id}` already exists in project `{project_id}`."),
        ));
    }
    Ok(())
}

fn validate_manifest_run_session(
    manifest: &NewContextManifest,
    run_agent_session_id: &str,
) -> CoreResult<()> {
    if manifest.agent_session_id != run_agent_session_id {
        return Err(CoreError::invalid_request(
            "agent_core_manifest_session_mismatch",
            format!(
                "Context manifest `{}` belongs to session `{}`, but run `{}` belongs to session `{run_agent_session_id}`.",
                manifest.manifest_id, manifest.agent_session_id, manifest.run_id
            ),
        ));
    }
    Ok(())
}

fn validate_context_manifest_trace(
    trace: &RuntimeTraceContext,
    run_trace_id: &str,
    run_id: &str,
    manifest_id: &str,
    turn_index: usize,
) -> CoreResult<()> {
    let expected =
        RuntimeTraceContext::for_context_manifest(run_trace_id, run_id, manifest_id, turn_index);
    if trace != &expected {
        return Err(CoreError::invalid_request(
            "agent_core_trace_invalid",
            "A context manifest trace must match its run, manifest, and turn identities.",
        ));
    }
    Ok(())
}

fn validate_optional_run_trace_id(trace_id: Option<&str>) -> CoreResult<()> {
    if trace_id.is_none_or(|trace_id| {
        trace_id.len() == 32
            && trace_id
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    }) {
        return Ok(());
    }
    Err(CoreError::invalid_request(
        "agent_core_trace_invalid",
        "An explicit run trace ID must be exactly 32 lowercase hexadecimal characters.",
    ))
}

fn validate_stored_trace_context(
    trace: &RuntimeTraceContext,
    run_trace_id: &str,
) -> CoreResult<()> {
    if trace.is_valid() && trace.trace_id == run_trace_id {
        return Ok(());
    }
    Err(CoreError::invalid_request(
        "agent_core_trace_invalid",
        "A stored trace context must be valid and belong to the target run.",
    ))
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

    fn unique_test_root(label: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "xero-core-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos()
        ));
        fs::create_dir_all(&root).expect("create temp dir");
        root
    }

    fn new_run(project_id: &str, run_id: &str) -> NewRunRecord {
        NewRunRecord {
            trace_id: None,
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "Fixture system prompt.".into(),
            project_id: project_id.into(),
            agent_session_id: "session-1".into(),
            run_id: run_id.into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            prompt: "Exercise the durable store.".into(),
        }
    }

    #[test]
    fn file_store_failed_insert_does_not_leak_an_in_memory_run() {
        let root = unique_test_root("store-failed-insert");
        let blocked_parent = root.join("not-a-directory");
        fs::write(&blocked_parent, "fixture").expect("write blocking parent file");
        let store = FileAgentCoreStore::open(blocked_parent.join("agent-runs.json"))
            .expect("open store before persistence");

        assert_eq!(
            store
                .insert_run(new_run("project-1", "run-1"))
                .expect_err("persistence must fail")
                .code,
            "agent_core_store_prepare_failed"
        );
        assert_eq!(
            store
                .load_run("project-1", "run-1")
                .expect_err("failed insert must be rolled back in memory")
                .code,
            "agent_core_run_not_found"
        );
        fs::remove_file(blocked_parent).expect("remove blocking parent file");
        fs::remove_dir(root).expect("remove temp dir");
    }

    #[test]
    fn file_store_rejects_an_empty_storage_path_at_open() {
        let error = FileAgentCoreStore::open(PathBuf::new())
            .expect_err("an empty file-store path must fail before mutation");

        assert_eq!(error.code, "agent_core_store_path_invalid");
    }

    #[test]
    fn file_store_persists_a_relative_storage_path() {
        let path = PathBuf::from(format!(
            "xero-core-relative-store-{}-{}.json",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("test clock")
                .as_nanos()
        ));
        let store = FileAgentCoreStore::open(&path).expect("open relative file store");

        store
            .insert_run(new_run("project-relative", "run-1"))
            .expect("persist relative file store");
        assert_eq!(
            FileAgentCoreStore::open(&path)
                .expect("reopen relative file store")
                .load_run("project-relative", "run-1")
                .expect("reload relative file-store run")
                .run_id,
            "run-1"
        );

        fs::remove_file(path).expect("remove relative file-store fixture");
    }

    #[test]
    fn file_store_rejects_blank_project_run_list_identity() {
        let root = unique_test_root("file-store-blank-project-list");
        let path = root.join("agent-runs.json");
        let store = FileAgentCoreStore::open(&path).expect("open project list store");

        let error = store
            .list_project_runs(" \n ")
            .expect_err("blank project run-list identity must fail validation");

        assert_eq!(error.code, "agent_core_required_field_missing");
        fs::remove_dir_all(root).expect("remove project list fixtures");
    }

    #[test]
    fn file_store_failed_commit_removes_temporary_payload_and_rolls_back_memory() {
        let root = unique_test_root("file-store-commit-cleanup");
        let path = root.join("agent-runs.json");
        let temporary_path = path.with_extension("json.tmp");
        let store = FileAgentCoreStore::open(&path).expect("open file store");
        fs::create_dir(&path).expect("block final store path with a directory");

        let error = store
            .insert_run(new_run("project-1", "run-commit-failure"))
            .expect_err("rename over a directory must fail");

        assert_eq!(error.code, "agent_core_store_commit_failed");
        assert!(
            !temporary_path.exists(),
            "a failed atomic commit must not leave its temporary payload behind"
        );
        assert_eq!(
            store
                .load_run("project-1", "run-commit-failure")
                .expect_err("failed commit must roll back in-memory state")
                .code,
            "agent_core_run_not_found"
        );

        fs::remove_dir_all(root).expect("remove temp store");
    }

    #[test]
    fn file_store_public_lookup_listing_and_validation_boundaries_are_deterministic() {
        let root = unique_test_root("file-store-public-boundaries");
        let path = root.join("agent-runs.json");
        let store = FileAgentCoreStore::open(&path).expect("open file store");

        let mut project_a_shared = new_run("project-a", "run-shared");
        project_a_shared.agent_session_id = "session-a".into();
        let mut project_b_shared = new_run("project-b", "run-shared");
        project_b_shared.agent_session_id = "session-b".into();
        let mut unique = new_run("project-a", "run-unique");
        unique.agent_session_id = "session-unique".into();
        store
            .insert_run(project_a_shared.clone())
            .expect("insert project-a shared run");
        store
            .insert_run(project_b_shared)
            .expect("insert project-b shared run");
        store.insert_run(unique).expect("insert unique run");

        assert_eq!(
            store
                .insert_run(project_a_shared)
                .expect_err("duplicate file-backed run")
                .code,
            "agent_core_run_exists"
        );
        assert_eq!(store.list_runs().expect("list all runs").len(), 3);
        assert_eq!(
            store
                .list_project_runs("project-a")
                .expect("list project runs")
                .len(),
            2
        );
        assert_eq!(
            store
                .load_run_by_id("run-unique")
                .expect("load globally unique run")
                .project_id,
            "project-a"
        );
        assert_eq!(
            store
                .load_run_by_id("run-shared")
                .expect_err("cross-project run id must be ambiguous")
                .code,
            "agent_core_run_ambiguous"
        );
        for (run_id, expected_code) in [
            ("missing", "agent_core_run_not_found"),
            (" \n ", "agent_core_required_field_missing"),
        ] {
            assert_eq!(
                store
                    .load_run_by_id(run_id)
                    .expect_err("invalid global run lookup")
                    .code,
                expected_code
            );
        }

        let latest = <FileAgentCoreStore as AgentCoreStore>::latest_run_for_session(
            &store,
            "project-a",
            "session-unique",
        )
        .expect("load latest session run through store trait");
        assert_eq!(latest.run_id, "run-unique");
        for (project_id, session_id, expected_code) in [
            ("", "session-unique", "agent_core_required_field_missing"),
            ("project-a", "\t", "agent_core_required_field_missing"),
            (
                "project-a",
                "session-missing",
                "agent_core_session_run_not_found",
            ),
        ] {
            assert_eq!(
                store
                    .latest_run_for_session(project_id, session_id)
                    .expect_err("invalid latest-session lookup")
                    .code,
                expected_code
            );
        }

        let snapshot = store
            .append_message(NewMessageRecord {
                project_id: "project-a".into(),
                run_id: "run-unique".into(),
                role: MessageRole::User,
                content: "message-only update".into(),
                provider_metadata: None,
            })
            .expect("append message-only update");
        assert_eq!(snapshot.messages.len(), 1);
        let summary = store
            .list_project_runs("project-a")
            .expect("list updated project runs")
            .into_iter()
            .find(|summary| summary.run_id == "run-unique")
            .expect("updated summary");
        assert!(summary.started_at.is_none());
        assert!(summary.updated_at.is_some());
        assert_eq!(summary.message_count, 1);

        let descriptor = store.runtime_store_descriptor("project-a");
        assert_eq!(descriptor.kind, RuntimeStoreKind::FileBackedHeadlessJson);
        assert_eq!(
            descriptor.root_path,
            path.parent().map(|parent| parent.display().to_string())
        );

        let unreadable_path = root.join("directory-store");
        fs::create_dir(&unreadable_path).expect("create unreadable store directory");
        assert_eq!(
            FileAgentCoreStore::open(&unreadable_path)
                .expect_err("a directory cannot be decoded as a store")
                .code,
            "agent_core_store_read_failed"
        );

        fs::remove_dir_all(root).expect("remove public-boundary store fixtures");
    }

    #[test]
    fn file_store_rolls_back_every_failed_mutation_and_reopens_committed_state() {
        fn block_parent(store_path: &Path) -> PathBuf {
            let parent = store_path.parent().expect("store parent");
            let backup = parent.with_extension("backup");
            fs::rename(parent, &backup).expect("move store parent");
            fs::write(parent, "blocking file").expect("block store parent");
            backup
        }

        fn restore_parent(store_path: &Path, backup: &Path) {
            let parent = store_path.parent().expect("store parent");
            fs::remove_file(parent).expect("remove blocking file");
            fs::rename(backup, parent).expect("restore store parent");
        }

        let root = unique_test_root("store-atomic-mutations");
        let path = root.join("state").join("agent-runs.json");
        let store = FileAgentCoreStore::open(&path).expect("open file store");
        store
            .insert_run(new_run("project-1", "run-1"))
            .expect("insert fixture run");

        let backup = block_parent(&path);
        assert_eq!(
            store
                .append_message(NewMessageRecord {
                    project_id: "project-1".into(),
                    run_id: "run-1".into(),
                    role: MessageRole::User,
                    content: "not durable".into(),
                    provider_metadata: None,
                })
                .expect_err("message persistence failure")
                .code,
            "agent_core_store_prepare_failed"
        );
        assert!(store
            .load_run("project-1", "run-1")
            .unwrap()
            .messages
            .is_empty());
        restore_parent(&path, &backup);
        store
            .append_message(NewMessageRecord {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                role: MessageRole::User,
                content: "durable".into(),
                provider_metadata: None,
            })
            .expect("persist message");

        let backup = block_parent(&path);
        assert_eq!(
            store
                .append_event(NewRuntimeEvent {
                    project_id: "project-1".into(),
                    run_id: "run-1".into(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: None,
                    payload: json!({"text": "not durable"}),
                })
                .expect_err("event persistence failure")
                .code,
            "agent_core_store_prepare_failed"
        );
        assert!(store
            .load_run("project-1", "run-1")
            .unwrap()
            .events
            .is_empty());
        restore_parent(&path, &backup);
        let event = store
            .append_event(NewRuntimeEvent {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                event_kind: RuntimeEventKind::MessageDelta,
                trace: None,
                payload: json!({"text": "durable"}),
            })
            .expect("persist event");
        assert_eq!(event.id, 1);

        let manifest = || NewContextManifest {
            manifest_id: "manifest-1".into(),
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            turn_index: 0,
            context_hash: "hash-1".into(),
            trace: None,
            manifest: json!({"schema": "fixture.v1"}),
        };
        let backup = block_parent(&path);
        assert_eq!(
            store
                .record_context_manifest(manifest())
                .expect_err("manifest persistence failure")
                .code,
            "agent_core_store_prepare_failed"
        );
        assert!(store
            .load_run("project-1", "run-1")
            .unwrap()
            .context_manifests
            .is_empty());
        restore_parent(&path, &backup);
        let persisted_manifest = store
            .record_context_manifest(manifest())
            .expect("persist manifest");
        assert_eq!(persisted_manifest.recorded_after_event_id, Some(1));

        let backup = block_parent(&path);
        assert_eq!(
            store
                .update_run_status("project-1", "run-1", RunStatus::Completed)
                .expect_err("status persistence failure")
                .code,
            "agent_core_store_prepare_failed"
        );
        assert_eq!(
            store.load_run("project-1", "run-1").unwrap().status,
            RunStatus::Starting
        );
        restore_parent(&path, &backup);
        store
            .update_run_status("project-1", "run-1", RunStatus::Completed)
            .expect("persist status");

        let reopened = FileAgentCoreStore::open(&path).expect("reopen file store");
        let snapshot = reopened
            .load_run("project-1", "run-1")
            .expect("load reopened run");
        assert_eq!(snapshot.status, RunStatus::Completed);
        assert_eq!(snapshot.messages.len(), 1);
        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.context_manifests.len(), 1);
        assert_eq!(reopened.path(), path);
        assert_eq!(reopened.list_runs().unwrap().len(), 1);
        assert_eq!(reopened.list_project_runs("project-1").unwrap().len(), 1);
        assert!(reopened.list_project_runs("other").unwrap().is_empty());
        assert_eq!(
            reopened
                .export_trace("project-1", "run-1")
                .expect("export reopened trace")
                .snapshot,
            snapshot
        );

        fs::remove_dir_all(root).expect("remove temp store");
    }

    #[test]
    fn rejected_in_memory_writes_do_not_consume_message_or_event_ids() {
        let store = InMemoryAgentCoreStore::default();
        store
            .insert_run(new_run("project-1", "run-1"))
            .expect("insert fixture run");

        assert_eq!(
            store
                .append_message(NewMessageRecord {
                    project_id: "project-1".into(),
                    run_id: "missing-run".into(),
                    role: MessageRole::User,
                    content: "missing".into(),
                    provider_metadata: None,
                })
                .expect_err("missing message run")
                .code,
            "agent_core_run_not_found"
        );
        let snapshot = store
            .append_message(NewMessageRecord {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                role: MessageRole::User,
                content: "first".into(),
                provider_metadata: None,
            })
            .expect("append first durable message");
        assert_eq!(snapshot.messages[0].id, 1);

        assert_eq!(
            store
                .append_event(NewRuntimeEvent {
                    project_id: "project-1".into(),
                    run_id: "missing-run".into(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: None,
                    payload: json!({}),
                })
                .expect_err("missing event run")
                .code,
            "agent_core_run_not_found"
        );
        let event = store
            .append_event(NewRuntimeEvent {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                event_kind: RuntimeEventKind::MessageDelta,
                trace: None,
                payload: json!({}),
            })
            .expect("append first durable event");
        assert_eq!(event.id, 1);
    }

    #[test]
    fn stores_reject_non_positive_agent_definition_versions_without_mutation() {
        fn assert_invalid_versions_rejected<S: AgentCoreStore>(store: S, project_id: &str) {
            for version in [0, -1] {
                let run_id = format!("run-version-{version}");
                let mut run = new_run(project_id, &run_id);
                run.agent_definition_version = version;
                let error = store
                    .insert_run(run)
                    .expect_err("non-positive agent definition version must fail");
                assert_eq!(
                    error.code, "agent_core_definition_version_invalid",
                    "version {version}"
                );
                assert_eq!(
                    store
                        .load_run(project_id, &run_id)
                        .expect_err("rejected definition version must not insert a run")
                        .code,
                    "agent_core_run_not_found"
                );
            }
        }

        assert_invalid_versions_rejected(InMemoryAgentCoreStore::default(), "project-memory");

        let root = unique_test_root("file-store-definition-version");
        let path = root.join("agent-runs.json");
        let file_store = FileAgentCoreStore::open(&path).expect("open definition version store");
        assert_invalid_versions_rejected(file_store, "project-file");
        assert!(FileAgentCoreStore::open(&path)
            .expect("reopen definition version store")
            .list_project_runs("project-file")
            .expect("list reopened definition version fixtures")
            .is_empty());
        fs::remove_dir_all(root).expect("remove definition version fixtures");
    }

    #[test]
    fn stores_reject_blank_run_lookup_identifiers() {
        fn assert_blank_lookup_rejected<S: AgentCoreStore>(store: S) {
            for (project_id, run_id) in [(" \n ", "run-1"), ("project-1", "\t")] {
                let error = store
                    .load_run(project_id, run_id)
                    .expect_err("blank run lookup identity must fail validation");
                assert_eq!(error.code, "agent_core_required_field_missing");
            }
        }

        assert_blank_lookup_rejected(InMemoryAgentCoreStore::default());

        let root = unique_test_root("file-store-blank-lookup");
        let path = root.join("agent-runs.json");
        let file_store = FileAgentCoreStore::open(&path).expect("open blank lookup store");
        assert_blank_lookup_rejected(file_store);
        fs::remove_dir_all(root).expect("remove blank lookup fixtures");
    }

    #[test]
    fn stores_reject_blank_status_update_identifiers_without_mutation() {
        fn assert_blank_update_rejected<S: AgentCoreStore>(store: S, project_id: &str) {
            store
                .insert_run(new_run(project_id, "run-1"))
                .expect("insert blank status update fixture");
            for (update_project_id, run_id) in [(" \n ", "run-1"), (project_id, "\t")] {
                let error = store
                    .update_run_status(update_project_id, run_id, RunStatus::Completed)
                    .expect_err("blank status update identity must fail validation");
                assert_eq!(error.code, "agent_core_required_field_missing");
            }
            assert_eq!(
                store
                    .load_run(project_id, "run-1")
                    .expect("reload blank status update fixture")
                    .status,
                RunStatus::Starting
            );
        }

        assert_blank_update_rejected(InMemoryAgentCoreStore::default(), "project-memory");

        let root = unique_test_root("file-store-blank-status-update");
        let path = root.join("agent-runs.json");
        let file_store = FileAgentCoreStore::open(&path).expect("open blank status update store");
        assert_blank_update_rejected(file_store, "project-file");
        assert_eq!(
            FileAgentCoreStore::open(&path)
                .expect("reopen blank status update store")
                .load_run("project-file", "run-1")
                .expect("reload reopened blank status update fixture")
                .status,
            RunStatus::Starting
        );
        fs::remove_dir_all(root).expect("remove blank status update fixtures");
    }

    #[test]
    fn stores_reject_duplicate_context_manifest_identity_without_mutation() {
        fn assert_duplicate_rejected<S: AgentCoreStore>(store: S, project_id: &str) {
            store
                .insert_run(new_run(project_id, "run-1"))
                .expect("insert manifest identity fixture");
            let manifest =
                |run_id: &str, context_hash: &str, turn_index: usize| NewContextManifest {
                    manifest_id: "manifest-identity-1".into(),
                    project_id: project_id.into(),
                    agent_session_id: "session-1".into(),
                    run_id: run_id.into(),
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                    turn_index,
                    context_hash: context_hash.into(),
                    trace: None,
                    manifest: json!({"turnIndex": turn_index}),
                };

            store
                .record_context_manifest(manifest("run-1", "hash-first", 0))
                .expect("persist first manifest identity");
            let error = store
                .record_context_manifest(manifest("run-1", "hash-conflict", 1))
                .expect_err("duplicate manifest identity must fail");
            assert_eq!(error.code, "agent_core_manifest_exists");

            let snapshot = store
                .load_run(project_id, "run-1")
                .expect("reload manifest identity fixture");
            assert_eq!(snapshot.context_manifests.len(), 1);
            assert_eq!(snapshot.context_manifests[0].context_hash, "hash-first");
            assert_eq!(snapshot.context_manifests[0].turn_index, 0);

            store
                .insert_run(new_run(project_id, "run-2"))
                .expect("insert second manifest identity fixture run");
            let error = store
                .record_context_manifest(manifest("run-2", "hash-other-run", 0))
                .expect_err("manifest identity must be unique across a project");
            assert_eq!(error.code, "agent_core_manifest_exists");
            assert!(store
                .load_run(project_id, "run-2")
                .expect("reload second manifest identity fixture")
                .context_manifests
                .is_empty());
        }

        assert_duplicate_rejected(InMemoryAgentCoreStore::default(), "project-memory");

        let root = unique_test_root("file-store-manifest-identity");
        let path = root.join("agent-runs.json");
        let file_store = FileAgentCoreStore::open(&path).expect("open file manifest store");
        assert_duplicate_rejected(file_store, "project-file");
        assert_eq!(
            FileAgentCoreStore::open(&path)
                .expect("reopen file manifest store")
                .load_run("project-file", "run-1")
                .expect("reload reopened manifest identity fixture")
                .context_manifests
                .len(),
            1
        );
        fs::remove_dir_all(root).expect("remove manifest identity fixtures");
    }

    #[test]
    fn stores_reject_cross_session_context_manifests_without_mutation() {
        fn assert_cross_session_manifest_rejected<S: AgentCoreStore>(store: S, project_id: &str) {
            store
                .insert_run(new_run(project_id, "run-1"))
                .expect("insert manifest ownership fixture");

            let error = store
                .record_context_manifest(NewContextManifest {
                    manifest_id: "manifest-cross-session".into(),
                    project_id: project_id.into(),
                    agent_session_id: "session-other".into(),
                    run_id: "run-1".into(),
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                    turn_index: 0,
                    context_hash: "hash-cross-session".into(),
                    trace: None,
                    manifest: json!({}),
                })
                .expect_err("manifest session must match its owning run");
            assert_eq!(error.code, "agent_core_manifest_session_mismatch");
            assert!(store
                .load_run(project_id, "run-1")
                .expect("reload manifest ownership fixture")
                .context_manifests
                .is_empty());
        }

        assert_cross_session_manifest_rejected(InMemoryAgentCoreStore::default(), "project-memory");

        let root = unique_test_root("file-store-manifest-ownership");
        let path = root.join("agent-runs.json");
        let file_store = FileAgentCoreStore::open(&path).expect("open manifest ownership store");
        assert_cross_session_manifest_rejected(file_store, "project-file");
        assert!(FileAgentCoreStore::open(&path)
            .expect("reopen manifest ownership store")
            .load_run("project-file", "run-1")
            .expect("reload reopened manifest ownership fixture")
            .context_manifests
            .is_empty());
        fs::remove_dir_all(root).expect("remove manifest ownership fixtures");
    }

    #[test]
    fn stores_reject_context_manifests_with_blank_provider_identity() {
        fn assert_blank_provider_identity_rejected<S: AgentCoreStore>(store: S, project_id: &str) {
            store
                .insert_run(new_run(project_id, "run-1"))
                .expect("insert manifest provider fixture");

            for (label, provider_id, model_id) in [
                ("provider", " \n ", "fake-model"),
                ("model", "fake_provider", "\t"),
            ] {
                let error = store
                    .record_context_manifest(NewContextManifest {
                        manifest_id: format!("manifest-blank-{label}"),
                        project_id: project_id.into(),
                        agent_session_id: "session-1".into(),
                        run_id: "run-1".into(),
                        provider_id: provider_id.into(),
                        model_id: model_id.into(),
                        turn_index: 0,
                        context_hash: format!("hash-blank-{label}"),
                        trace: None,
                        manifest: json!({}),
                    })
                    .expect_err("blank manifest provider identity must fail");
                assert_eq!(error.code, "agent_core_required_field_missing", "{label}");
            }

            assert!(store
                .load_run(project_id, "run-1")
                .expect("reload manifest provider fixture")
                .context_manifests
                .is_empty());
        }

        assert_blank_provider_identity_rejected(
            InMemoryAgentCoreStore::default(),
            "project-memory",
        );

        let root = unique_test_root("file-store-manifest-provider");
        let path = root.join("agent-runs.json");
        let file_store = FileAgentCoreStore::open(&path).expect("open manifest provider store");
        assert_blank_provider_identity_rejected(file_store, "project-file");
        assert!(FileAgentCoreStore::open(&path)
            .expect("reopen manifest provider store")
            .load_run("project-file", "run-1")
            .expect("reload reopened manifest provider fixture")
            .context_manifests
            .is_empty());
        fs::remove_dir_all(root).expect("remove manifest provider fixtures");
    }

    #[test]
    fn stores_reject_context_manifest_traces_for_a_different_manifest() {
        fn assert_manifest_trace_rejected<S: AgentCoreStore>(store: S, project_id: &str) {
            let snapshot = store
                .insert_run(new_run(project_id, "run-1"))
                .expect("insert manifest trace fixture");
            let trace = RuntimeTraceContext::for_context_manifest(
                &snapshot.trace_id,
                "run-1",
                "manifest-other",
                0,
            );

            let error = store
                .record_context_manifest(NewContextManifest {
                    manifest_id: "manifest-target".into(),
                    project_id: project_id.into(),
                    agent_session_id: "session-1".into(),
                    run_id: "run-1".into(),
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                    turn_index: 0,
                    context_hash: "hash-target".into(),
                    trace: Some(trace),
                    manifest: json!({}),
                })
                .expect_err("manifest trace identity must match its record");
            assert_eq!(error.code, "agent_core_trace_invalid");
            assert!(store
                .load_run(project_id, "run-1")
                .expect("reload manifest trace fixture")
                .context_manifests
                .is_empty());
        }

        assert_manifest_trace_rejected(InMemoryAgentCoreStore::default(), "project-memory");

        let root = unique_test_root("file-store-manifest-trace-identity");
        let path = root.join("agent-runs.json");
        let file_store = FileAgentCoreStore::open(&path).expect("open manifest trace store");
        assert_manifest_trace_rejected(file_store, "project-file");
        assert!(FileAgentCoreStore::open(&path)
            .expect("reopen manifest trace store")
            .load_run("project-file", "run-1")
            .expect("reload reopened manifest trace fixture")
            .context_manifests
            .is_empty());
        fs::remove_dir_all(root).expect("remove manifest trace fixtures");
    }

    #[test]
    fn stores_reject_invalid_or_cross_run_trace_contexts_without_mutation() {
        let store = InMemoryAgentCoreStore::default();
        let mut invalid_run = new_run("project-1", "run-invalid-trace");
        invalid_run.trace_id = Some("not-a-runtime-trace".into());
        assert_eq!(
            store
                .insert_run(invalid_run)
                .expect_err("malformed explicit run trace must fail")
                .code,
            "agent_core_trace_invalid"
        );

        let snapshot = store
            .insert_run(new_run("project-1", "run-1"))
            .expect("insert trace validation fixture");
        let foreign_trace_id = runtime_trace_id_for_run("project-1", "run-foreign");
        let foreign_event_trace =
            RuntimeTraceContext::for_run(&foreign_trace_id, "run-1", "foreign_event");
        assert!(foreign_event_trace.is_valid());
        assert_eq!(
            store
                .append_event(NewRuntimeEvent {
                    project_id: "project-1".into(),
                    run_id: "run-1".into(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: Some(foreign_event_trace),
                    payload: json!({}),
                })
                .expect_err("cross-run event trace must fail")
                .code,
            "agent_core_trace_invalid"
        );

        let mut malformed_manifest_trace = RuntimeTraceContext::for_context_manifest(
            &snapshot.trace_id,
            "run-1",
            "manifest-invalid",
            0,
        );
        malformed_manifest_trace.span_id = "malformed".into();
        assert_eq!(
            store
                .record_context_manifest(NewContextManifest {
                    manifest_id: "manifest-invalid".into(),
                    project_id: "project-1".into(),
                    agent_session_id: "session-1".into(),
                    run_id: "run-1".into(),
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                    turn_index: 0,
                    context_hash: "hash-invalid".into(),
                    trace: Some(malformed_manifest_trace),
                    manifest: json!({}),
                })
                .expect_err("malformed manifest trace must fail")
                .code,
            "agent_core_trace_invalid"
        );

        let unchanged = store
            .load_run("project-1", "run-1")
            .expect("reload trace validation fixture");
        assert!(unchanged.events.is_empty());
        assert!(unchanged.context_manifests.is_empty());
        assert_eq!(
            store
                .append_event(NewRuntimeEvent {
                    project_id: "project-1".into(),
                    run_id: "run-1".into(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: None,
                    payload: json!({}),
                })
                .expect("valid generated event trace")
                .id,
            1,
            "rejected event trace must not consume an event id"
        );

        let root = unique_test_root("file-store-trace-validation");
        let file_store = FileAgentCoreStore::open(root.join("agent-runs.json"))
            .expect("open file trace validation store");
        let mut invalid_file_run = new_run("project-file", "run-invalid-trace");
        invalid_file_run.trace_id = Some("ABCDEF0123456789ABCDEF0123456789".into());
        assert_eq!(
            file_store
                .insert_run(invalid_file_run)
                .expect_err("uppercase file-store trace must fail")
                .code,
            "agent_core_trace_invalid"
        );
        let file_snapshot = file_store
            .insert_run(new_run("project-file", "run-1"))
            .expect("insert file trace validation fixture");
        let mut malformed_event_trace =
            RuntimeTraceContext::for_run(&file_snapshot.trace_id, "run-1", "malformed_event");
        malformed_event_trace.parent_span_id = Some("bad-parent".into());
        assert_eq!(
            file_store
                .append_event(NewRuntimeEvent {
                    project_id: "project-file".into(),
                    run_id: "run-1".into(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: Some(malformed_event_trace),
                    payload: json!({}),
                })
                .expect_err("malformed file-store event trace must fail")
                .code,
            "agent_core_trace_invalid"
        );
        let foreign_manifest_trace = RuntimeTraceContext::for_context_manifest(
            &runtime_trace_id_for_run("project-file", "run-foreign"),
            "run-1",
            "manifest-foreign",
            0,
        );
        assert_eq!(
            file_store
                .record_context_manifest(NewContextManifest {
                    manifest_id: "manifest-foreign".into(),
                    project_id: "project-file".into(),
                    agent_session_id: "session-1".into(),
                    run_id: "run-1".into(),
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                    turn_index: 0,
                    context_hash: "hash-foreign".into(),
                    trace: Some(foreign_manifest_trace),
                    manifest: json!({}),
                })
                .expect_err("cross-run file-store manifest trace must fail")
                .code,
            "agent_core_trace_invalid"
        );
        let unchanged_file = file_store
            .load_run("project-file", "run-1")
            .expect("reload file trace validation fixture");
        assert!(unchanged_file.events.is_empty());
        assert!(unchanged_file.context_manifests.is_empty());
        fs::remove_dir_all(root).expect("remove file trace validation fixtures");
    }

    #[test]
    fn fake_provider_rejects_blank_continuations_without_mutating_the_run() {
        let runtime = FakeProviderRuntime::default();
        let before = runtime
            .start_run(StartRunRequest {
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-blank-continuation".into(),
                prompt: "Start the fixture run.".into(),
                provider: ProviderSelection {
                    provider_id: FAKE_PROVIDER_ID.into(),
                    model_id: "fake-model".into(),
                },
                controls: None,
            })
            .expect("start fixture run");

        let error = runtime
            .continue_run(ContinueRunRequest {
                project_id: before.project_id.clone(),
                run_id: before.run_id.clone(),
                prompt: " \n ".into(),
            })
            .expect_err("blank continuation must be rejected");

        assert_eq!(error.code, "agent_core_required_field_missing");
        assert_eq!(
            runtime
                .store()
                .load_run(&before.project_id, &before.run_id)
                .expect("reload fixture run"),
            before,
            "a rejected continuation must not append messages, events, or manifests"
        );
    }

    #[test]
    fn stores_validate_inputs_lookup_boundaries_and_corrupt_file_state() {
        let store = InMemoryAgentCoreStore::default();
        assert_eq!(
            store.runtime_store_descriptor("project-1").kind,
            RuntimeStoreKind::InMemoryHarness
        );
        assert_eq!(
            store.semantic_workspace_index_state("project-1"),
            EnvironmentSemanticIndexState::Unavailable
        );

        let valid = new_run("project-1", "run-1");
        for invalid in [
            NewRunRecord {
                project_id: " ".into(),
                ..valid.clone()
            },
            NewRunRecord {
                agent_session_id: "".into(),
                ..valid.clone()
            },
            NewRunRecord {
                run_id: "".into(),
                ..valid.clone()
            },
            NewRunRecord {
                prompt: "".into(),
                ..valid.clone()
            },
            NewRunRecord {
                provider_id: "".into(),
                ..valid.clone()
            },
            NewRunRecord {
                model_id: "".into(),
                ..valid.clone()
            },
            NewRunRecord {
                runtime_agent_id: "".into(),
                ..valid.clone()
            },
            NewRunRecord {
                agent_definition_id: "".into(),
                ..valid.clone()
            },
            NewRunRecord {
                system_prompt: "".into(),
                ..valid.clone()
            },
        ] {
            assert_eq!(
                store.insert_run(invalid).expect_err("invalid run").code,
                "agent_core_required_field_missing"
            );
        }
        store.insert_run(valid.clone()).expect("insert valid run");
        assert_eq!(
            store.insert_run(valid).expect_err("duplicate run").code,
            "agent_core_run_exists"
        );
        assert_eq!(
            store.load_run("project-1", "missing").unwrap_err().code,
            "agent_core_run_not_found"
        );
        assert_eq!(
            store
                .latest_run_for_session("", "session-1")
                .unwrap_err()
                .code,
            "agent_core_required_field_missing"
        );
        assert_eq!(
            store
                .latest_run_for_session("project-1", "missing")
                .unwrap_err()
                .code,
            "agent_core_session_run_not_found"
        );

        let root = unique_test_root("store-corruption");
        let path = root.join("agent-runs.json");
        fs::write(&path, []).expect("write empty state");
        assert!(FileAgentCoreStore::open(&path)
            .unwrap()
            .list_runs()
            .unwrap()
            .is_empty());
        fs::write(&path, "malformed").expect("write malformed state");
        assert_eq!(
            FileAgentCoreStore::open(&path).unwrap_err().code,
            "agent_core_store_decode_failed"
        );
        fs::write(
            &path,
            json!({"schemaVersion": 1, "nextMessageId": 0, "nextEventId": 0, "runs": []})
                .to_string(),
        )
        .expect("write unsupported state");
        assert_eq!(
            FileAgentCoreStore::open(&path).unwrap_err().code,
            "agent_core_store_schema_unsupported"
        );

        fs::remove_file(&path).expect("remove unsupported state");
        let file_store = FileAgentCoreStore::open(&path).expect("open replacement store");
        file_store
            .insert_run(new_run("project-1", "run-duplicate"))
            .expect("insert duplicate fixture source");
        let mut payload: JsonValue =
            serde_json::from_slice(&fs::read(&path).expect("read state")).expect("decode state");
        let duplicate = payload["runs"][0].clone();
        payload["runs"].as_array_mut().unwrap().push(duplicate);
        fs::write(&path, serde_json::to_vec(&payload).unwrap()).expect("write duplicate state");
        assert_eq!(
            FileAgentCoreStore::open(&path).unwrap_err().code,
            "agent_core_store_duplicate_run"
        );
        fs::remove_dir_all(root).expect("remove corrupt store fixtures");
    }

    #[test]
    fn file_store_rejects_corrupt_trace_ownership_and_id_counters_on_reopen() {
        let root = unique_test_root("store-invariant-corruption");
        let path = root.join("agent-runs.json");
        let store = FileAgentCoreStore::open(&path).expect("open invariant fixture store");
        let snapshot = store
            .insert_run(new_run("project-1", "run-1"))
            .expect("insert invariant fixture run");
        store
            .append_message(NewMessageRecord {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                role: MessageRole::User,
                content: "fixture".into(),
                provider_metadata: None,
            })
            .expect("append invariant fixture message");
        store
            .append_event(NewRuntimeEvent {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                event_kind: RuntimeEventKind::MessageDelta,
                trace: None,
                payload: json!({}),
            })
            .expect("append invariant fixture event");
        store
            .record_context_manifest(NewContextManifest {
                manifest_id: "manifest-1".into(),
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
                turn_index: 0,
                context_hash: "hash-1".into(),
                trace: None,
                manifest: json!({}),
            })
            .expect("append invariant fixture manifest");
        store
            .insert_run(new_run("project-1", "run-2"))
            .expect("insert second invariant fixture run");
        store
            .record_context_manifest(NewContextManifest {
                manifest_id: "manifest-2".into(),
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-2".into(),
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
                turn_index: 0,
                context_hash: "hash-2".into(),
                trace: None,
                manifest: json!({}),
            })
            .expect("append second invariant fixture manifest");
        let original: JsonValue =
            serde_json::from_slice(&fs::read(&path).expect("read invariant fixture store"))
                .expect("decode invariant fixture store");

        let foreign_trace_id = runtime_trace_id_for_run("project-1", "run-foreign");
        let cases = [
            (
                "invalid-run-trace",
                "agent_core_store_trace_invalid",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["traceId"] = json!("not-a-trace");
                }) as Box<dyn Fn(&mut JsonValue)>,
            ),
            (
                "foreign-event-trace",
                "agent_core_store_trace_invalid",
                Box::new(move |payload: &mut JsonValue| {
                    payload["runs"][0]["events"][0]["trace"]["traceId"] = json!(foreign_trace_id);
                    payload["runs"][0]["events"][0]["trace"]["runTraceId"] =
                        json!(foreign_trace_id);
                }),
            ),
            (
                "manifest-trace-identity",
                "agent_core_store_trace_invalid",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["contextManifests"][0]["trace"]["spanId"] =
                        json!("0000000000000000");
                }),
            ),
            (
                "message-owner",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["messages"][0]["projectId"] = json!("project-other");
                }),
            ),
            (
                "blank-message-timestamp",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["messages"][0]["createdAt"] = json!(" \n ");
                }),
            ),
            (
                "blank-event-timestamp",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["events"][0]["createdAt"] = json!("\t");
                }),
            ),
            (
                "blank-manifest-timestamp",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["contextManifests"][0]["createdAt"] = json!("");
                }),
            ),
            (
                "blank-run-identity",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["runId"] = json!(" \n ");
                    for collection in ["messages", "events", "contextManifests"] {
                        for record in payload["runs"][0][collection]
                            .as_array_mut()
                            .expect("persisted record collection")
                        {
                            record["runId"] = json!(" \n ");
                        }
                    }
                }),
            ),
            (
                "missing-runtime-agent-id",
                "agent_core_store_decode_failed",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]
                        .as_object_mut()
                        .expect("persisted run")
                        .remove("runtimeAgentId");
                }),
            ),
            (
                "missing-agent-definition-id",
                "agent_core_store_decode_failed",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]
                        .as_object_mut()
                        .expect("persisted run")
                        .remove("agentDefinitionId");
                }),
            ),
            (
                "missing-agent-definition-version",
                "agent_core_store_decode_failed",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]
                        .as_object_mut()
                        .expect("persisted run")
                        .remove("agentDefinitionVersion");
                }),
            ),
            (
                "missing-system-prompt",
                "agent_core_store_decode_failed",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]
                        .as_object_mut()
                        .expect("persisted run")
                        .remove("systemPrompt");
                }),
            ),
            (
                "duplicate-project-manifest",
                "agent_core_store_duplicate_manifest",
                Box::new(|payload: &mut JsonValue| {
                    let manifest_id = payload["runs"][0]["contextManifests"][0]["manifestId"]
                        .as_str()
                        .expect("fixture manifest ID")
                        .to_string();
                    let run_trace_id = payload["runs"][1]["traceId"]
                        .as_str()
                        .expect("fixture run trace ID")
                        .to_string();
                    payload["runs"][1]["contextManifests"][0]["manifestId"] = json!(manifest_id);
                    payload["runs"][1]["contextManifests"][0]["trace"] =
                        serde_json::to_value(RuntimeTraceContext::for_context_manifest(
                            &run_trace_id,
                            "run-2",
                            "manifest-1",
                            0,
                        ))
                        .expect("encode duplicate manifest trace");
                }),
            ),
            (
                "blank-manifest-id",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["contextManifests"][0]["manifestId"] = json!(" \n ");
                }),
            ),
            (
                "blank-manifest-provider",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["contextManifests"][0]["providerId"] = json!("\t");
                }),
            ),
            (
                "blank-manifest-model",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["contextManifests"][0]["modelId"] = json!("");
                }),
            ),
            (
                "blank-manifest-context-hash",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["contextManifests"][0]["contextHash"] = json!("  ");
                }),
            ),
            (
                "manifest-event-boundary",
                "agent_core_store_record_mismatch",
                Box::new(|payload: &mut JsonValue| {
                    payload["runs"][0]["contextManifests"][0]["recordedAfterEventId"] = json!(999);
                }),
            ),
            (
                "event-counter",
                "agent_core_store_id_counter_invalid",
                Box::new(|payload: &mut JsonValue| {
                    payload["nextEventId"] = json!(0);
                }),
            ),
            (
                "message-counter",
                "agent_core_store_id_counter_invalid",
                Box::new(|payload: &mut JsonValue| {
                    payload["nextMessageId"] = json!(0);
                }),
            ),
        ];

        for (label, expected_code, mutate) in cases {
            let mut payload = original.clone();
            mutate(&mut payload);
            fs::write(&path, serde_json::to_vec_pretty(&payload).unwrap())
                .expect("write corrupt invariant fixture");
            assert_eq!(
                FileAgentCoreStore::open(&path).expect_err(label).code,
                expected_code,
                "corruption fixture `{label}`"
            );
        }

        fs::write(&path, serde_json::to_vec_pretty(&original).unwrap())
            .expect("restore valid invariant fixture");
        assert_eq!(
            FileAgentCoreStore::open(&path)
                .expect("valid invariant fixture must reopen")
                .load_run("project-1", "run-1")
                .expect("reload invariant fixture run")
                .trace_id,
            snapshot.trace_id
        );
        fs::remove_dir_all(root).expect("remove invariant corruption fixtures");
    }

    #[test]
    fn file_store_fails_closed_when_record_id_counters_are_exhausted() {
        let root = unique_test_root("store-counter-exhaustion");
        let path = root.join("agent-runs.json");
        let store = FileAgentCoreStore::open(&path).expect("open counter exhaustion store");
        store
            .insert_run(new_run("project-1", "run-1"))
            .expect("insert counter exhaustion run");
        let mut payload: JsonValue =
            serde_json::from_slice(&fs::read(&path).expect("read counter exhaustion store"))
                .expect("decode counter exhaustion store");
        payload["nextMessageId"] = json!(i64::MAX);
        payload["nextEventId"] = json!(i64::MAX);
        fs::write(&path, serde_json::to_vec_pretty(&payload).unwrap())
            .expect("write exhausted counters");

        let exhausted = FileAgentCoreStore::open(&path).expect("reopen exhausted counters");
        let message_error = exhausted
            .append_message(NewMessageRecord {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                role: MessageRole::User,
                content: "must not persist".into(),
                provider_metadata: None,
            })
            .expect_err("exhausted message counter must fail");
        assert_eq!(message_error.code, "agent_core_store_id_counter_exhausted");
        let event_error = exhausted
            .append_event(NewRuntimeEvent {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                event_kind: RuntimeEventKind::MessageDelta,
                trace: None,
                payload: json!({}),
            })
            .expect_err("exhausted event counter must fail");
        assert_eq!(event_error.code, "agent_core_store_id_counter_exhausted");

        let unchanged = exhausted
            .load_run("project-1", "run-1")
            .expect("reload exhausted counter run");
        assert!(unchanged.messages.is_empty());
        assert!(unchanged.events.is_empty());
        fs::remove_dir_all(root).expect("remove counter exhaustion fixtures");
    }

    #[test]
    fn provider_metadata_and_fake_facade_cover_all_control_operations() {
        let tool_call = RuntimeProviderToolCallMetadata {
            tool_call_id: "call-1".into(),
            provider_tool_name: "read".into(),
            arguments: json!({"path": "README.md"}),
        };
        let calls = RuntimeMessageProviderMetadata::assistant_tool_calls(
            "message-1",
            vec![tool_call.clone()],
        );
        assert_eq!(calls.provider_message_id.as_deref(), Some("message-1"));
        assert_eq!(calls.assistant_tool_calls, vec![tool_call.clone()]);
        let turn = RuntimeMessageProviderMetadata::assistant_turn(
            "message-2",
            Some("reasoning".into()),
            Some(json!({"encrypted": true})),
            vec![tool_call],
        );
        assert_eq!(turn.reasoning_content.as_deref(), Some("reasoning"));
        assert_eq!(turn.reasoning_details.unwrap()["encrypted"], true);
        let result =
            RuntimeMessageProviderMetadata::tool_result("message-3", "call-1", "read", "message-2");
        assert_eq!(
            result.tool_result.unwrap().parent_assistant_message_id,
            "message-2"
        );

        let runtime = FakeProviderRuntime::default();
        let start = |run_id: &str, provider_id: &str, controls: Option<RunControls>| {
            runtime.start_run(StartRunRequest {
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: run_id.into(),
                prompt: "Start fixture run.".into(),
                provider: ProviderSelection {
                    provider_id: provider_id.into(),
                    model_id: "fake-model".into(),
                },
                controls,
            })
        };
        assert_eq!(
            start("wrong-provider", "openai", None).unwrap_err().code,
            "agent_core_fake_provider_explicit_required"
        );
        let snapshot = start(
            "run-controls",
            "fake_provider",
            Some(RunControls {
                runtime_agent_id: " ".into(),
                agent_definition_id: Some(" ".into()),
                agent_definition_version: Some(0),
                agent_definition_snapshot: None,
                thinking_effort: None,
                approval_mode: "suggest".into(),
                plan_mode_required: false,
            }),
        )
        .expect("start defaulted controls run");
        assert_eq!(snapshot.runtime_agent_id, "engineer");
        assert_eq!(snapshot.agent_definition_id, "engineer");
        assert_eq!(snapshot.agent_definition_version, 1);

        let continued = runtime
            .continue_run(ContinueRunRequest {
                project_id: "project-1".into(),
                run_id: "run-controls".into(),
                prompt: "Continue fixture.".into(),
            })
            .expect("continue fake run");
        assert_eq!(continued.context_manifests.len(), 2);
        runtime
            .submit_user_input(UserInputRequest {
                project_id: "project-1".into(),
                run_id: "run-controls".into(),
                text: "User input.".into(),
            })
            .expect("submit user input");
        runtime
            .approve_action(ApprovalDecisionRequest {
                project_id: "project-1".into(),
                run_id: "run-controls".into(),
                action_id: "action-1".into(),
                response: None,
            })
            .expect("approve with generated response");
        let rejected = runtime
            .reject_action(ApprovalDecisionRequest {
                project_id: "project-1".into(),
                run_id: "run-controls".into(),
                action_id: "action-2".into(),
                response: Some("No".into()),
            })
            .expect("reject action");
        assert_eq!(
            rejected.events.last().unwrap().event_kind,
            RuntimeEventKind::PolicyDecision
        );
        assert_eq!(
            runtime
                .cancel_run(CancelRunRequest {
                    project_id: "project-1".into(),
                    run_id: "run-controls".into(),
                })
                .unwrap()
                .status,
            RunStatus::Cancelled
        );
        assert_eq!(
            runtime
                .resume_run(ResumeRunRequest {
                    project_id: "project-1".into(),
                    run_id: "run-controls".into(),
                    response: "Resume fixture.".into(),
                })
                .unwrap()
                .status,
            RunStatus::Completed
        );
        assert_eq!(
            runtime
                .export_trace(ExportTraceRequest {
                    project_id: "project-1".into(),
                    run_id: "run-controls".into(),
                })
                .unwrap()
                .protocol_version,
            CORE_PROTOCOL_VERSION
        );
        assert_eq!(
            runtime
                .fork_session(ForkSessionRequest {
                    project_id: "project-1".into(),
                    source_agent_session_id: "session-1".into(),
                    target_agent_session_id: "session-2".into(),
                })
                .unwrap_err()
                .code,
            "agent_core_operation_unsupported"
        );
        assert_eq!(
            runtime
                .compact_session(CompactSessionRequest {
                    project_id: "project-1".into(),
                    agent_session_id: "session-1".into(),
                    reason: "fixture".into(),
                })
                .unwrap_err()
                .code,
            "agent_core_operation_unsupported"
        );
    }

    #[test]
    fn fake_provider_honors_custom_agent_identity_and_fails_closed_on_stage_evidence() {
        let runtime = FakeProviderRuntime::default();
        let definition = json!({
            "id": "fixture-observer",
            "version": 3,
            "lifecycleState": "active",
            "displayName": "Fixture Observer",
            "baseCapabilityProfile": "ask",
            "allowedApprovalModes": ["suggest"],
            "toolPolicy": { "allowedTools": ["read"] },
            "prompts": [{
                "role": "system",
                "body": "CUSTOM_FAKE_AGENT_PROMPT_MARKER"
            }]
        });
        let request = |run_id: &str, snapshot: JsonValue| StartRunRequest {
            project_id: "project-fake-agent".into(),
            agent_session_id: "session-fake-agent".into(),
            run_id: run_id.into(),
            prompt: "Inspect the fixture.".into(),
            provider: ProviderSelection {
                provider_id: FAKE_PROVIDER_ID.into(),
                model_id: "fake-model".into(),
            },
            controls: Some(RunControls {
                runtime_agent_id: "fixture-observer".into(),
                agent_definition_id: Some("fixture-observer".into()),
                agent_definition_version: Some(3),
                agent_definition_snapshot: Some(snapshot),
                thinking_effort: Some("low".into()),
                approval_mode: "suggest".into(),
                plan_mode_required: false,
            }),
        };

        let completed = runtime
            .start_run(request("fake-custom-agent", definition.clone()))
            .expect("custom Agent without required Stage evidence should complete");
        assert_eq!(completed.status, RunStatus::Completed);
        assert!(completed.system_prompt.contains("CUSTOM_FAKE_AGENT_PROMPT_MARKER"));
        let started = completed
            .events
            .iter()
            .find(|event| event.event_kind == RuntimeEventKind::RunStarted)
            .expect("run-started Agent policy");
        assert_eq!(started.payload["approvalMode"], "suggest");
        assert_eq!(
            started.payload["agentDefinitionRuntime"]["definitionId"],
            "fixture-observer"
        );

        let mut staged = definition;
        staged["workflowStructure"] = json!({
            "startPhaseId": "inspect",
            "phases": [{
                "id": "inspect",
                "title": "Inspect",
                "allowedTools": ["read"],
                "requiredChecks": [{
                    "kind": "tool_succeeded",
                    "toolName": "read",
                    "minCount": 1
                }]
            }]
        });
        let error = runtime
            .start_run(request("fake-staged-agent", staged))
            .expect_err("fake final response must not bypass required Stage evidence");
        assert_eq!(
            error.code,
            "agent_core_fake_provider_stage_evidence_required"
        );
        let failed = runtime
            .store()
            .load_run("project-fake-agent", "fake-staged-agent")
            .expect("failed Stage run remains inspectable");
        assert_eq!(failed.status, RunStatus::Failed);
        assert!(failed.events.iter().any(|event| {
            event.event_kind == RuntimeEventKind::VerificationGate
                && event.payload["state"] == "blocked"
                && event.payload["stageId"] == "inspect"
        }));
        assert!(!failed
            .messages
            .iter()
            .any(|message| message.role == MessageRole::Assistant));
    }

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
                    agent_definition_snapshot: None,
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
