use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::db::project_store::{
    agent_event_kind_sql_value, agent_message_role_sql_value, agent_run_status_sql_value,
    agent_tool_call_state_sql_value, AgentActionRequestRecord, AgentCheckpointRecord,
    AgentDefinitionRecord, AgentDefinitionVersionRecord, AgentEventRecord, AgentFileChangeRecord,
    AgentMessageRecord, AgentRunRecord, AgentRunSnapshotRecord, AgentToolCallRecord,
};

use super::{
    runtime::{
        ProviderModelThinkingEffortDto, RuntimeAgentIdDto, RuntimeRunControlInputDto,
        RuntimeRunDiagnosticDto, StagedAgentAttachmentDto,
    },
    workflow_agents::AgentRefDto,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatusDto {
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
pub enum AgentMessageRoleDto {
    System,
    Developer,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunEventKindDto {
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
    RouteRequested,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolCallStateDto {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentMessageDto {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub role: AgentMessageRoleDto,
    pub content: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<AgentMessageAttachmentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMessageAttachmentKindDto {
    Image,
    Document,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentMessageAttachmentDto {
    pub id: i64,
    pub message_id: i64,
    pub kind: AgentMessageAttachmentKindDto,
    pub absolute_path: String,
    pub media_type: String,
    pub original_name: String,
    pub size_bytes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentRunEventDto {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub event_kind: AgentRunEventKindDto,
    pub payload: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolCallDto {
    pub project_id: String,
    pub run_id: String,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: JsonValue,
    pub state: AgentToolCallStateDto,
    pub result: Option<JsonValue>,
    pub error: Option<RuntimeRunDiagnosticDto>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentFileChangeDto {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub top_level_run_id: String,
    pub subagent_id: Option<String>,
    pub subagent_role: Option<String>,
    pub change_group_id: Option<String>,
    pub path: String,
    pub operation: String,
    pub old_hash: Option<String>,
    pub new_hash: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentCheckpointDto {
    pub id: i64,
    pub project_id: String,
    pub run_id: String,
    pub checkpoint_kind: String,
    pub summary: String,
    pub payload: Option<JsonValue>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentActionRequestDto {
    pub project_id: String,
    pub run_id: String,
    pub action_id: String,
    pub action_type: String,
    pub title: String,
    pub detail: String,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
    pub response: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentRunDto {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub lineage_kind: String,
    pub parent_run_id: Option<String>,
    pub parent_trace_id: Option<String>,
    pub parent_subagent_id: Option<String>,
    pub subagent_role: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub status: AgentRunStatusDto,
    pub prompt: String,
    pub system_prompt: String,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
    pub updated_at: String,
    pub messages: Vec<AgentMessageDto>,
    pub events: Vec<AgentRunEventDto>,
    pub tool_calls: Vec<AgentToolCallDto>,
    pub file_changes: Vec<AgentFileChangeDto>,
    pub checkpoints: Vec<AgentCheckpointDto>,
    pub action_requests: Vec<AgentActionRequestDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentRunSummaryDto {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub trace_id: String,
    pub lineage_kind: String,
    pub parent_run_id: Option<String>,
    pub parent_trace_id: Option<String>,
    pub parent_subagent_id: Option<String>,
    pub subagent_role: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub status: AgentRunStatusDto,
    pub prompt: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub cancelled_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartAgentTaskRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<RuntimeRunControlInputDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<StagedAgentAttachmentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendAgentMessageRequestDto {
    pub run_id: String,
    pub continuation_request_id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<StagedAgentAttachmentDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact: Option<AgentAutoCompactPreferenceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelAgentRunRequestDto {
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RejectAgentActionRequestDto {
    pub run_id: String,
    pub action_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeAgentRunRequestDto {
    pub run_id: String,
    pub continuation_request_id: String,
    pub response: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact: Option<AgentAutoCompactPreferenceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAutoCompactPreferenceDto {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub threshold_percent: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_tail_message_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentRunRequestDto {
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportAgentTraceRequestDto {
    pub run_id: String,
    #[serde(default)]
    pub include_support_bundle: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentTraceExportDto {
    pub trace: JsonValue,
    pub timeline: JsonValue,
    pub diagnostics: JsonValue,
    pub quality_gates: JsonValue,
    pub production_readiness: JsonValue,
    pub markdown_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub support_bundle: Option<JsonValue>,
    pub canonical_trace: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAgentRunsRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAgentRunsResponseDto {
    pub runs: Vec<AgentRunSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscribeAgentStreamRequestDto {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscribeAgentStreamResponseDto {
    pub run_id: String,
    pub replayed_event_count: usize,
}

pub fn agent_run_dto(snapshot: AgentRunSnapshotRecord) -> AgentRunDto {
    AgentRunDto {
        runtime_agent_id: snapshot.run.runtime_agent_id,
        agent_definition_id: snapshot.run.agent_definition_id.clone(),
        agent_definition_version: snapshot.run.agent_definition_version,
        project_id: snapshot.run.project_id.clone(),
        agent_session_id: snapshot.run.agent_session_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        trace_id: snapshot.run.trace_id.clone(),
        lineage_kind: snapshot.run.lineage_kind.clone(),
        parent_run_id: snapshot.run.parent_run_id.clone(),
        parent_trace_id: snapshot.run.parent_trace_id.clone(),
        parent_subagent_id: snapshot.run.parent_subagent_id.clone(),
        subagent_role: snapshot.run.subagent_role.clone(),
        provider_id: snapshot.run.provider_id.clone(),
        model_id: snapshot.run.model_id.clone(),
        status: agent_run_status_dto(&snapshot.run),
        prompt: snapshot.run.prompt.clone(),
        system_prompt: snapshot.run.system_prompt.clone(),
        started_at: snapshot.run.started_at.clone(),
        last_heartbeat_at: snapshot.run.last_heartbeat_at.clone(),
        completed_at: snapshot.run.completed_at.clone(),
        cancelled_at: snapshot.run.cancelled_at.clone(),
        last_error_code: snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.clone()),
        last_error: snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| RuntimeRunDiagnosticDto {
                code: error.code.clone(),
                message: error.message.clone(),
            }),
        updated_at: snapshot.run.updated_at.clone(),
        messages: snapshot
            .messages
            .into_iter()
            .map(agent_message_dto)
            .collect(),
        events: snapshot.events.into_iter().map(agent_event_dto).collect(),
        tool_calls: snapshot
            .tool_calls
            .into_iter()
            .map(agent_tool_call_dto)
            .collect(),
        file_changes: snapshot
            .file_changes
            .into_iter()
            .map(agent_file_change_dto)
            .collect(),
        checkpoints: snapshot
            .checkpoints
            .into_iter()
            .map(agent_checkpoint_dto)
            .collect(),
        action_requests: snapshot
            .action_requests
            .into_iter()
            .map(agent_action_request_dto)
            .collect(),
    }
}

pub fn agent_run_summary_dto(run: AgentRunRecord) -> AgentRunSummaryDto {
    let status = agent_run_status_dto(&run);
    AgentRunSummaryDto {
        runtime_agent_id: run.runtime_agent_id,
        agent_definition_id: run.agent_definition_id,
        agent_definition_version: run.agent_definition_version,
        project_id: run.project_id,
        agent_session_id: run.agent_session_id,
        run_id: run.run_id,
        trace_id: run.trace_id,
        lineage_kind: run.lineage_kind,
        parent_run_id: run.parent_run_id,
        parent_trace_id: run.parent_trace_id,
        parent_subagent_id: run.parent_subagent_id,
        subagent_role: run.subagent_role,
        provider_id: run.provider_id,
        model_id: run.model_id,
        status,
        prompt: run.prompt,
        started_at: run.started_at,
        completed_at: run.completed_at,
        cancelled_at: run.cancelled_at,
        last_error_code: run.last_error.as_ref().map(|error| error.code.clone()),
        last_error: run.last_error.map(|error| RuntimeRunDiagnosticDto {
            code: error.code,
            message: error.message,
        }),
        updated_at: run.updated_at,
    }
}

fn agent_message_dto(message: AgentMessageRecord) -> AgentMessageDto {
    let attachments = message
        .attachments
        .into_iter()
        .map(agent_message_attachment_dto)
        .collect();
    AgentMessageDto {
        id: message.id,
        project_id: message.project_id,
        run_id: message.run_id,
        role: serde_json::from_str(&format!(
            "\"{}\"",
            agent_message_role_sql_value(&message.role)
        ))
        .expect("agent message role should serialize to dto enum"),
        content: message.content,
        created_at: message.created_at,
        attachments,
    }
}

fn agent_message_attachment_dto(
    attachment: crate::db::project_store::AgentMessageAttachmentRecord,
) -> AgentMessageAttachmentDto {
    use crate::db::project_store::AgentMessageAttachmentKind;
    AgentMessageAttachmentDto {
        id: attachment.id,
        message_id: attachment.message_id,
        kind: match attachment.kind {
            AgentMessageAttachmentKind::Image => AgentMessageAttachmentKindDto::Image,
            AgentMessageAttachmentKind::Document => AgentMessageAttachmentKindDto::Document,
            AgentMessageAttachmentKind::Text => AgentMessageAttachmentKindDto::Text,
        },
        absolute_path: attachment.storage_path,
        media_type: attachment.media_type,
        original_name: attachment.original_name,
        size_bytes: attachment.size_bytes,
        width: attachment.width,
        height: attachment.height,
        created_at: attachment.created_at,
    }
}

pub fn agent_event_dto(event: AgentEventRecord) -> AgentRunEventDto {
    AgentRunEventDto {
        id: event.id,
        project_id: event.project_id,
        run_id: event.run_id,
        event_kind: serde_json::from_str(&format!(
            "\"{}\"",
            agent_event_kind_sql_value(&event.event_kind)
        ))
        .expect("agent event kind should serialize to dto enum"),
        payload: serde_json::from_str(&event.payload_json).unwrap_or(JsonValue::Null),
        created_at: event.created_at,
    }
}

fn agent_tool_call_dto(tool_call: AgentToolCallRecord) -> AgentToolCallDto {
    AgentToolCallDto {
        project_id: tool_call.project_id,
        run_id: tool_call.run_id,
        tool_call_id: tool_call.tool_call_id,
        tool_name: tool_call.tool_name,
        input: serde_json::from_str(&tool_call.input_json).unwrap_or(JsonValue::Null),
        state: serde_json::from_str(&format!(
            "\"{}\"",
            agent_tool_call_state_sql_value(&tool_call.state)
        ))
        .expect("agent tool-call state should serialize to dto enum"),
        result: tool_call
            .result_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        error: tool_call.error.map(|error| RuntimeRunDiagnosticDto {
            code: error.code,
            message: error.message,
        }),
        started_at: tool_call.started_at,
        completed_at: tool_call.completed_at,
    }
}

fn agent_file_change_dto(file_change: AgentFileChangeRecord) -> AgentFileChangeDto {
    AgentFileChangeDto {
        id: file_change.id,
        project_id: file_change.project_id,
        run_id: file_change.run_id,
        trace_id: file_change.trace_id,
        top_level_run_id: file_change.top_level_run_id,
        subagent_id: file_change.subagent_id,
        subagent_role: file_change.subagent_role,
        change_group_id: file_change.change_group_id,
        path: file_change.path,
        operation: file_change.operation,
        old_hash: file_change.old_hash,
        new_hash: file_change.new_hash,
        created_at: file_change.created_at,
    }
}

fn agent_checkpoint_dto(checkpoint: AgentCheckpointRecord) -> AgentCheckpointDto {
    AgentCheckpointDto {
        id: checkpoint.id,
        project_id: checkpoint.project_id,
        run_id: checkpoint.run_id,
        checkpoint_kind: checkpoint.checkpoint_kind,
        summary: checkpoint.summary,
        payload: checkpoint
            .payload_json
            .as_deref()
            .and_then(|value| serde_json::from_str(value).ok()),
        created_at: checkpoint.created_at,
    }
}

fn agent_action_request_dto(action_request: AgentActionRequestRecord) -> AgentActionRequestDto {
    AgentActionRequestDto {
        project_id: action_request.project_id,
        run_id: action_request.run_id,
        action_id: action_request.action_id,
        action_type: action_request.action_type,
        title: action_request.title,
        detail: action_request.detail,
        status: action_request.status,
        created_at: action_request.created_at,
        resolved_at: action_request.resolved_at,
        response: action_request.response,
    }
}

fn agent_run_status_dto(run: &AgentRunRecord) -> AgentRunStatusDto {
    serde_json::from_str(&format!("\"{}\"", agent_run_status_sql_value(&run.status)))
        .expect("agent run status should serialize to dto enum")
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDefinitionScopeDto {
    BuiltIn,
    GlobalCustom,
    ProjectCustom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDefinitionLifecycleStateDto {
    Draft,
    Valid,
    Active,
    Archived,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDefinitionBaseCapabilityProfileDto {
    ObserveOnly,
    ComputerUse,
    Planning,
    RepositoryRecon,
    Engineering,
    Debugging,
    AgentBuilder,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefaultModelDto {
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinitionSummaryDto {
    pub definition_id: String,
    pub current_version: u32,
    pub display_name: String,
    pub short_label: String,
    pub description: String,
    pub scope: AgentDefinitionScopeDto,
    pub lifecycle_state: AgentDefinitionLifecycleStateDto,
    pub base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    pub created_at: String,
    pub updated_at: String,
    pub is_built_in: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<AgentDefaultModelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinitionVersionSummaryDto {
    pub definition_id: String,
    pub version: u32,
    pub created_at: String,
    pub validation_status: Option<String>,
    pub validation_diagnostic_count: u32,
    pub snapshot: JsonValue,
    pub validation_report: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAgentDefinitionsRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAgentDefinitionsResponseDto {
    pub definitions: Vec<AgentDefinitionSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveAgentDefinitionRequestDto {
    pub project_id: String,
    pub definition_id: String,
    pub expected_current_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveAgentDefinitionRequestDto {
    pub project_id: String,
    pub definition: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<String>,
    /// When true, the runtime returns a pre-save approval review payload
    /// (xero.agent_definition_pre_save_review.v1) instead of persisting. Used
    /// by the canvas Save flow to show the operator what would change before
    /// they approve the write.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAgentDefinitionRequestDto {
    pub project_id: String,
    pub definition_id: String,
    pub definition: JsonValue,
    /// When true, the runtime returns a pre-save approval review payload
    /// instead of persisting. See SaveAgentDefinitionRequestDto::dry_run.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreviewAgentDefinitionRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub definition_id: Option<String>,
    pub definition: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetAgentDefaultModelRequestDto {
    pub project_id: String,
    pub r#ref: AgentRefDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<AgentDefaultModelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetAgentDefaultModelResponseDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<AgentDefaultModelDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDefinitionValidationStatusDto {
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinitionValidationDiagnosticDto {
    pub code: String,
    pub message: String,
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied_tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub denied_effect_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_capability_profile: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinitionValidationReportDto {
    pub status: AgentDefinitionValidationStatusDto,
    pub diagnostics: Vec<AgentDefinitionValidationDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDefinitionWriteResponseDto {
    pub applied: bool,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<AgentDefinitionSummaryDto>,
    pub validation: AgentDefinitionValidationReportDto,
    /// True when the runtime gated the write behind operator approval and the
    /// caller must re-issue with dry_run=false to actually persist.
    #[serde(default)]
    pub approval_required: bool,
    /// Structured pre-save review (xero.agent_definition_pre_save_review.v1)
    /// when approval_required is true; null when the call applied directly.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_review: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentDefinitionVersionRequestDto {
    pub project_id: String,
    pub definition_id: String,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentDefinitionVersionDiffRequestDto {
    pub project_id: String,
    pub definition_id: String,
    pub from_version: u32,
    pub to_version: u32,
}

pub fn agent_definition_summary_dto(record: AgentDefinitionRecord) -> AgentDefinitionSummaryDto {
    let is_built_in = record.scope == "built_in";
    AgentDefinitionSummaryDto {
        definition_id: record.definition_id,
        current_version: record.current_version,
        display_name: record.display_name,
        short_label: record.short_label,
        description: record.description,
        scope: parse_agent_definition_scope(&record.scope),
        lifecycle_state: parse_agent_definition_lifecycle_state(&record.lifecycle_state),
        base_capability_profile: parse_agent_definition_base_capability_profile(
            &record.base_capability_profile,
        ),
        created_at: record.created_at,
        updated_at: record.updated_at,
        is_built_in,
        default_model: None,
    }
}

pub fn agent_definition_version_summary_dto(
    record: AgentDefinitionVersionRecord,
) -> AgentDefinitionVersionSummaryDto {
    let validation_report = record.validation_report;
    let (validation_status, validation_diagnostic_count) = match validation_report.as_ref() {
        Some(report) => {
            let status = report
                .get("status")
                .and_then(JsonValue::as_str)
                .map(str::to_owned);
            let count = report
                .get("diagnostics")
                .and_then(JsonValue::as_array)
                .map(|items| items.len() as u32)
                .unwrap_or(0);
            (status, count)
        }
        None => (None, 0),
    };
    AgentDefinitionVersionSummaryDto {
        definition_id: record.definition_id,
        version: record.version,
        created_at: record.created_at,
        validation_status,
        validation_diagnostic_count,
        snapshot: record.snapshot,
        validation_report,
    }
}

fn parse_agent_definition_scope(value: &str) -> AgentDefinitionScopeDto {
    match value {
        "global_custom" => AgentDefinitionScopeDto::GlobalCustom,
        "project_custom" => AgentDefinitionScopeDto::ProjectCustom,
        _ => AgentDefinitionScopeDto::BuiltIn,
    }
}

fn parse_agent_definition_lifecycle_state(value: &str) -> AgentDefinitionLifecycleStateDto {
    match value {
        "draft" => AgentDefinitionLifecycleStateDto::Draft,
        "valid" => AgentDefinitionLifecycleStateDto::Valid,
        "archived" => AgentDefinitionLifecycleStateDto::Archived,
        "blocked" => AgentDefinitionLifecycleStateDto::Blocked,
        _ => AgentDefinitionLifecycleStateDto::Active,
    }
}

fn parse_agent_definition_base_capability_profile(
    value: &str,
) -> AgentDefinitionBaseCapabilityProfileDto {
    match value {
        "planning" => AgentDefinitionBaseCapabilityProfileDto::Planning,
        "computer_use" => AgentDefinitionBaseCapabilityProfileDto::ComputerUse,
        "repository_recon" => AgentDefinitionBaseCapabilityProfileDto::RepositoryRecon,
        "engineering" => AgentDefinitionBaseCapabilityProfileDto::Engineering,
        "debugging" => AgentDefinitionBaseCapabilityProfileDto::Debugging,
        "agent_builder" => AgentDefinitionBaseCapabilityProfileDto::AgentBuilder,
        _ => AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::project_store::{
        AgentMessageAttachmentKind, AgentMessageAttachmentRecord, AgentMessageRole,
        AgentRunDiagnosticRecord, AgentRunEventKind, AgentRunSnapshotRecord, AgentRunStatus,
        AgentToolCallState,
    };
    use serde_json::json;

    #[test]
    fn route_requested_events_are_exposed_through_the_agent_contract() {
        let dto = agent_event_dto(AgentEventRecord {
            id: 7,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind: AgentRunEventKind::RouteRequested,
            payload_json: r#"{"target":"debug"}"#.into(),
            created_at: "2026-07-18T12:00:00Z".into(),
        });

        assert_eq!(dto.event_kind, AgentRunEventKindDto::RouteRequested);
        assert_eq!(dto.payload["target"], "debug");
    }

    #[test]
    fn full_run_snapshot_conversion_preserves_lineage_diagnostics_and_artifacts() {
        let snapshot = AgentRunSnapshotRecord {
            run: run_record(AgentRunStatus::Failed),
            messages: vec![AgentMessageRecord {
                id: 1,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                role: AgentMessageRole::Assistant,
                content: "Attached evidence.".into(),
                provider_metadata_json: Some(r#"{"provider":"fixture"}"#.into()),
                created_at: "2026-07-18T12:00:01Z".into(),
                attachments: vec![
                    attachment(1, AgentMessageAttachmentKind::Image, Some(640), Some(480)),
                    attachment(2, AgentMessageAttachmentKind::Document, None, None),
                    attachment(3, AgentMessageAttachmentKind::Text, None, None),
                ],
            }],
            events: vec![AgentEventRecord {
                id: 2,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                event_kind: AgentRunEventKind::RunFailed,
                payload_json: "malformed".into(),
                created_at: "2026-07-18T12:00:02Z".into(),
            }],
            tool_calls: vec![
                AgentToolCallRecord {
                    project_id: "project-1".into(),
                    run_id: "run-1".into(),
                    tool_call_id: "tool-1".into(),
                    tool_name: "read".into(),
                    input_json: r#"{"path":"src/lib.rs"}"#.into(),
                    state: AgentToolCallState::Succeeded,
                    result_json: Some(r#"{"ok":true}"#.into()),
                    error: None,
                    started_at: "2026-07-18T12:00:03Z".into(),
                    completed_at: Some("2026-07-18T12:00:04Z".into()),
                },
                AgentToolCallRecord {
                    project_id: "project-1".into(),
                    run_id: "run-1".into(),
                    tool_call_id: "tool-2".into(),
                    tool_name: "write".into(),
                    input_json: "malformed".into(),
                    state: AgentToolCallState::Failed,
                    result_json: Some("malformed".into()),
                    error: Some(AgentRunDiagnosticRecord {
                        code: "write_failed".into(),
                        message: "Fixture write failed.".into(),
                    }),
                    started_at: "2026-07-18T12:00:05Z".into(),
                    completed_at: Some("2026-07-18T12:00:06Z".into()),
                },
            ],
            file_changes: vec![AgentFileChangeRecord {
                id: 3,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                trace_id: "trace-1".into(),
                top_level_run_id: "run-parent".into(),
                subagent_id: Some("subagent-1".into()),
                subagent_role: Some("worker".into()),
                change_group_id: Some("change-1".into()),
                path: "src/lib.rs".into(),
                operation: "edit".into(),
                old_hash: Some("old".into()),
                new_hash: Some("new".into()),
                created_at: "2026-07-18T12:00:07Z".into(),
            }],
            checkpoints: vec![AgentCheckpointRecord {
                id: 4,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                checkpoint_kind: "verification".into(),
                summary: "Checks complete.".into(),
                payload_json: Some(r#"{"passed":3}"#.into()),
                created_at: "2026-07-18T12:00:08Z".into(),
            }],
            action_requests: vec![AgentActionRequestRecord {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                action_id: "action-1".into(),
                action_type: "approval".into(),
                title: "Approve write".into(),
                detail: "Write src/lib.rs".into(),
                status: "rejected".into(),
                created_at: "2026-07-18T12:00:09Z".into(),
                resolved_at: Some("2026-07-18T12:00:10Z".into()),
                response: Some("Do not change it.".into()),
            }],
        };

        let dto = agent_run_dto(snapshot);

        assert_eq!(dto.status, AgentRunStatusDto::Failed);
        assert_eq!(dto.last_error_code.as_deref(), Some("fixture_error"));
        assert_eq!(
            dto.last_error.expect("run diagnostic").message,
            "Fixture failed."
        );
        assert_eq!(dto.parent_run_id.as_deref(), Some("run-parent"));
        assert_eq!(dto.messages[0].attachments.len(), 3);
        assert_eq!(
            dto.messages[0].attachments[0].kind,
            AgentMessageAttachmentKindDto::Image
        );
        assert_eq!(
            dto.messages[0].attachments[1].kind,
            AgentMessageAttachmentKindDto::Document
        );
        assert_eq!(
            dto.messages[0].attachments[2].kind,
            AgentMessageAttachmentKindDto::Text
        );
        assert_eq!(dto.events[0].payload, JsonValue::Null);
        assert_eq!(dto.tool_calls[0].input["path"], "src/lib.rs");
        assert_eq!(
            dto.tool_calls[0].result.as_ref().expect("tool result")["ok"],
            true
        );
        assert_eq!(dto.tool_calls[1].input, JsonValue::Null);
        assert!(dto.tool_calls[1].result.is_none());
        assert_eq!(
            dto.tool_calls[1].error.as_ref().expect("tool error").code,
            "write_failed"
        );
        assert_eq!(
            dto.file_changes[0].change_group_id.as_deref(),
            Some("change-1")
        );
        assert_eq!(
            dto.checkpoints[0]
                .payload
                .as_ref()
                .expect("checkpoint payload")["passed"],
            3
        );
        assert_eq!(
            dto.action_requests[0].response.as_deref(),
            Some("Do not change it.")
        );
    }

    #[test]
    fn run_summary_and_nested_enum_conversions_cover_every_durable_state() {
        let statuses = [
            (AgentRunStatus::Starting, AgentRunStatusDto::Starting),
            (AgentRunStatus::Running, AgentRunStatusDto::Running),
            (AgentRunStatus::Paused, AgentRunStatusDto::Paused),
            (AgentRunStatus::Cancelling, AgentRunStatusDto::Cancelling),
            (AgentRunStatus::Cancelled, AgentRunStatusDto::Cancelled),
            (AgentRunStatus::HandedOff, AgentRunStatusDto::HandedOff),
            (AgentRunStatus::Completed, AgentRunStatusDto::Completed),
            (AgentRunStatus::Failed, AgentRunStatusDto::Failed),
        ];
        for (status, expected) in statuses {
            let dto = agent_run_summary_dto(run_record(status));
            assert_eq!(dto.status, expected);
            assert_eq!(dto.last_error_code.as_deref(), Some("fixture_error"));
            assert_eq!(
                dto.last_error.expect("summary diagnostic").code,
                "fixture_error"
            );
        }

        for (role, expected) in [
            (AgentMessageRole::System, AgentMessageRoleDto::System),
            (AgentMessageRole::Developer, AgentMessageRoleDto::Developer),
            (AgentMessageRole::User, AgentMessageRoleDto::User),
            (AgentMessageRole::Assistant, AgentMessageRoleDto::Assistant),
            (AgentMessageRole::Tool, AgentMessageRoleDto::Tool),
        ] {
            let dto = agent_message_dto(AgentMessageRecord {
                id: 1,
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                role,
                content: "fixture".into(),
                provider_metadata_json: None,
                created_at: "2026-07-18T12:00:00Z".into(),
                attachments: Vec::new(),
            });
            assert_eq!(dto.role, expected);
        }

        for (state, expected) in [
            (AgentToolCallState::Pending, AgentToolCallStateDto::Pending),
            (AgentToolCallState::Running, AgentToolCallStateDto::Running),
            (
                AgentToolCallState::Succeeded,
                AgentToolCallStateDto::Succeeded,
            ),
            (AgentToolCallState::Failed, AgentToolCallStateDto::Failed),
        ] {
            let dto = agent_tool_call_dto(AgentToolCallRecord {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
                tool_call_id: "tool-1".into(),
                tool_name: "read".into(),
                input_json: "{}".into(),
                state,
                result_json: None,
                error: None,
                started_at: "2026-07-18T12:00:00Z".into(),
                completed_at: None,
            });
            assert_eq!(dto.state, expected);
        }
    }

    #[test]
    fn agent_definition_contracts_cover_all_persisted_variants_and_reports() {
        for (scope, expected) in [
            ("built_in", AgentDefinitionScopeDto::BuiltIn),
            ("global_custom", AgentDefinitionScopeDto::GlobalCustom),
            ("project_custom", AgentDefinitionScopeDto::ProjectCustom),
        ] {
            let dto =
                agent_definition_summary_dto(definition_record(scope, "active", "observe_only"));
            assert_eq!(dto.scope, expected);
            assert_eq!(dto.is_built_in, scope == "built_in");
            assert!(dto.default_model.is_none());
        }

        for (lifecycle, expected) in [
            ("draft", AgentDefinitionLifecycleStateDto::Draft),
            ("valid", AgentDefinitionLifecycleStateDto::Valid),
            ("active", AgentDefinitionLifecycleStateDto::Active),
            ("archived", AgentDefinitionLifecycleStateDto::Archived),
            ("blocked", AgentDefinitionLifecycleStateDto::Blocked),
        ] {
            assert_eq!(
                agent_definition_summary_dto(definition_record(
                    "project_custom",
                    lifecycle,
                    "observe_only",
                ))
                .lifecycle_state,
                expected
            );
        }

        for (profile, expected) in [
            (
                "observe_only",
                AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
            ),
            (
                "computer_use",
                AgentDefinitionBaseCapabilityProfileDto::ComputerUse,
            ),
            (
                "planning",
                AgentDefinitionBaseCapabilityProfileDto::Planning,
            ),
            (
                "repository_recon",
                AgentDefinitionBaseCapabilityProfileDto::RepositoryRecon,
            ),
            (
                "engineering",
                AgentDefinitionBaseCapabilityProfileDto::Engineering,
            ),
            (
                "debugging",
                AgentDefinitionBaseCapabilityProfileDto::Debugging,
            ),
            (
                "agent_builder",
                AgentDefinitionBaseCapabilityProfileDto::AgentBuilder,
            ),
        ] {
            assert_eq!(
                agent_definition_summary_dto(definition_record(
                    "project_custom",
                    "active",
                    profile,
                ))
                .base_capability_profile,
                expected
            );
        }

        let with_report = agent_definition_version_summary_dto(AgentDefinitionVersionRecord {
            definition_id: "custom-1".into(),
            version: 2,
            snapshot: json!({"schema": "xero.agent_definition.v1"}),
            validation_report: Some(json!({
                "status": "invalid",
                "diagnostics": [{"code": "one"}, {"code": "two"}],
            })),
            created_at: "2026-07-18T12:00:00Z".into(),
        });
        assert_eq!(with_report.validation_status.as_deref(), Some("invalid"));
        assert_eq!(with_report.validation_diagnostic_count, 2);
        assert!(with_report.validation_report.is_some());

        let without_report = agent_definition_version_summary_dto(AgentDefinitionVersionRecord {
            definition_id: "custom-1".into(),
            version: 1,
            snapshot: json!({}),
            validation_report: None,
            created_at: "2026-07-18T11:00:00Z".into(),
        });
        assert!(without_report.validation_status.is_none());
        assert_eq!(without_report.validation_diagnostic_count, 0);
    }

    fn run_record(status: AgentRunStatus) -> AgentRunRecord {
        AgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "engineer".into(),
            agent_definition_version: 3,
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-1".into(),
            trace_id: "trace-1".into(),
            lineage_kind: "subagent".into(),
            parent_run_id: Some("run-parent".into()),
            parent_trace_id: Some("trace-parent".into()),
            parent_subagent_id: Some("subagent-parent".into()),
            subagent_role: Some("worker".into()),
            provider_id: "fixture-provider".into(),
            model_id: "fixture-model".into(),
            status,
            prompt: "Audit the fixture.".into(),
            system_prompt: "fixture-system".into(),
            started_at: "2026-07-18T12:00:00Z".into(),
            last_heartbeat_at: Some("2026-07-18T12:00:01Z".into()),
            completed_at: Some("2026-07-18T12:01:00Z".into()),
            cancelled_at: None,
            last_error: Some(AgentRunDiagnosticRecord {
                code: "fixture_error".into(),
                message: "Fixture failed.".into(),
            }),
            updated_at: "2026-07-18T12:01:00Z".into(),
        }
    }

    fn attachment(
        id: i64,
        kind: AgentMessageAttachmentKind,
        width: Option<i64>,
        height: Option<i64>,
    ) -> AgentMessageAttachmentRecord {
        AgentMessageAttachmentRecord {
            id,
            message_id: 1,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            kind,
            storage_path: format!("/fixture/attachment-{id}"),
            media_type: "application/octet-stream".into(),
            original_name: format!("attachment-{id}"),
            size_bytes: 42,
            width,
            height,
            created_at: "2026-07-18T12:00:01Z".into(),
        }
    }

    fn definition_record(
        scope: &str,
        lifecycle_state: &str,
        base_capability_profile: &str,
    ) -> AgentDefinitionRecord {
        AgentDefinitionRecord {
            definition_id: "custom-1".into(),
            current_version: 2,
            display_name: "Custom agent".into(),
            short_label: "Custom".into(),
            description: "Fixture definition".into(),
            scope: scope.into(),
            lifecycle_state: lifecycle_state.into(),
            base_capability_profile: base_capability_profile.into(),
            created_at: "2026-07-18T11:00:00Z".into(),
            updated_at: "2026-07-18T12:00:00Z".into(),
        }
    }
}
