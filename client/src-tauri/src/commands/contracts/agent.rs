use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::db::project_store::{
    agent_event_kind_sql_value, agent_message_role_sql_value, agent_run_status_sql_value,
    agent_tool_call_state_sql_value, AgentActionRequestRecord, AgentCheckpointRecord,
    AgentDefinitionRecord, AgentDefinitionVersionRecord, AgentEventRecord, AgentFileChangeRecord,
    AgentMessageRecord, AgentRunRecord, AgentRunSnapshotRecord, AgentToolCallRecord,
};

use super::runtime::{RuntimeAgentIdDto, RuntimeRunControlInputDto, RuntimeRunDiagnosticDto};

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
    ActionRequired,
    RunPaused,
    RunCompleted,
    RunFailed,
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
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<RuntimeRunControlInputDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SendAgentMessageRequestDto {
    pub run_id: String,
    pub prompt: String,
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
pub struct ResumeAgentRunRequestDto {
    pub run_id: String,
    pub response: String,
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
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDefinitionBaseCapabilityProfileDto {
    ObserveOnly,
    Engineering,
    Debugging,
    AgentBuilder,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentDefinitionVersionRequestDto {
    pub project_id: String,
    pub definition_id: String,
    pub version: u32,
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
        "archived" => AgentDefinitionLifecycleStateDto::Archived,
        _ => AgentDefinitionLifecycleStateDto::Active,
    }
}

fn parse_agent_definition_base_capability_profile(
    value: &str,
) -> AgentDefinitionBaseCapabilityProfileDto {
    match value {
        "engineering" => AgentDefinitionBaseCapabilityProfileDto::Engineering,
        "debugging" => AgentDefinitionBaseCapabilityProfileDto::Debugging,
        "agent_builder" => AgentDefinitionBaseCapabilityProfileDto::AgentBuilder,
        _ => AgentDefinitionBaseCapabilityProfileDto::ObserveOnly,
    }
}
