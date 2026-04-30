use std::cmp::Ordering;

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::db::project_store::{
    agent_run_status_sql_value, AgentCompactionRecord, AgentCompactionTrigger, AgentMemoryKind,
    AgentMemoryRecord, AgentMemoryReviewState, AgentMemoryScope, AgentRunEventKind, AgentRunRecord,
    AgentRunSnapshotRecord, AgentRunStatus, AgentSessionRecord, AgentSessionStatus,
    AgentToolCallState, AgentUsageRecord,
};

use super::runtime::{
    AgentSessionDto, AgentSessionLineageBoundaryKindDto, AgentSessionLineageDto,
    RuntimeStreamItemDto, RuntimeStreamItemKind, RuntimeToolCallState,
};

pub const XERO_SESSION_CONTEXT_CONTRACT_VERSION: u32 = 1;

const REDACTED_TEXT: &str = "Xero redacted sensitive session-context text.";
const REDACTED_PATH: &str = "[redacted-path]";
const DEFAULT_AUTO_COMPACT_THRESHOLD_PERCENT: u8 = 80;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionTranscriptScopeDto {
    Run,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionTranscriptExportFormatDto {
    Markdown,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionTranscriptSourceKindDto {
    OwnedAgent,
    RuntimeStream,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionTranscriptItemKindDto {
    Message,
    Reasoning,
    ToolCall,
    ToolResult,
    FileChange,
    Checkpoint,
    ActionRequest,
    Activity,
    Complete,
    Failure,
    Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionTranscriptActorDto {
    System,
    Developer,
    User,
    Assistant,
    Tool,
    Runtime,
    Xero,
    Operator,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionTranscriptToolStateDto {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionContextRedactionClassDto {
    Public,
    LocalPath,
    Secret,
    RawPayload,
    Transcript,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionContextRedactionDto {
    pub redaction_class: SessionContextRedactionClassDto,
    pub redacted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl SessionContextRedactionDto {
    pub fn public() -> Self {
        Self {
            redaction_class: SessionContextRedactionClassDto::Public,
            redacted: false,
            reason: None,
        }
    }

    fn redacted(
        redaction_class: SessionContextRedactionClassDto,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            redaction_class,
            redacted: true,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionUsageTotalsDto {
    pub project_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub estimated_cost_micros: u64,
    pub source: SessionUsageSourceDto,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionUsageSourceDto {
    Provider,
    Estimated,
    Mixed,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionTranscriptItemDto {
    pub contract_version: u32,
    pub item_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub source_kind: SessionTranscriptSourceKindDto,
    pub source_table: String,
    pub source_id: String,
    pub sequence: u64,
    pub created_at: String,
    pub kind: SessionTranscriptItemKindDto,
    pub actor: SessionTranscriptActorDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_state: Option<SessionTranscriptToolStateDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checkpoint_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunTranscriptSummaryDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: String,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub item_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_totals: Option<SessionUsageTotalsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunTranscriptDto {
    pub contract_version: u32,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: String,
    pub source_kind: SessionTranscriptSourceKindDto,
    pub started_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub items: Vec<SessionTranscriptItemDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_totals: Option<SessionUsageTotalsDto>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionTranscriptStatusDto {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionTranscriptDto {
    pub contract_version: u32,
    pub project_id: String,
    pub agent_session_id: String,
    pub title: String,
    pub summary: String,
    pub status: AgentSessionTranscriptStatusDto,
    pub archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archived_at: Option<String>,
    pub runs: Vec<RunTranscriptSummaryDto>,
    pub items: Vec<SessionTranscriptItemDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_totals: Option<SessionUsageTotalsDto>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionTranscriptExportPayloadDto {
    pub contract_version: u32,
    pub export_id: String,
    pub generated_at: String,
    pub scope: SessionTranscriptScopeDto,
    pub format: SessionTranscriptExportFormatDto,
    pub transcript: SessionTranscriptDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_snapshot: Option<SessionContextSnapshotDto>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionTranscriptSearchResultSnippetDto {
    pub contract_version: u32,
    pub result_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub item_id: String,
    pub archived: bool,
    pub rank: u32,
    pub matched_fields: Vec<String>,
    pub snippet: String,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetSessionTranscriptRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExportSessionTranscriptRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub format: SessionTranscriptExportFormatDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionTranscriptExportResponseDto {
    pub payload: SessionTranscriptExportPayloadDto,
    pub content: String,
    pub mime_type: String,
    pub suggested_file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveSessionTranscriptExportRequestDto {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchSessionTranscriptsRequestDto {
    pub project_id: String,
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchSessionTranscriptsResponseDto {
    pub project_id: String,
    pub query: String,
    pub results: Vec<SessionTranscriptSearchResultSnippetDto>,
    pub total: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetSessionContextSnapshotRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactSessionHistoryRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_tail_message_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionContextContributorKindDto {
    SystemPrompt,
    InstructionFile,
    ApprovedMemory,
    CompactionSummary,
    ConversationTail,
    ToolResult,
    ToolDescriptor,
    ProviderUsage,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionContextBudgetPressureDto {
    Unknown,
    Low,
    Medium,
    High,
    Over,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionContextBudgetDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_tokens: Option<u64>,
    pub estimated_tokens: u64,
    pub estimation_source: SessionUsageSourceDto,
    pub pressure: SessionContextBudgetPressureDto,
    pub known_provider_budget: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionContextContributorDto {
    pub contributor_id: String,
    pub kind: SessionContextContributorKindDto,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub sequence: u64,
    pub estimated_tokens: u64,
    pub estimated_chars: u64,
    pub included: bool,
    pub model_visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionContextSnapshotDto {
    pub contract_version: u32,
    pub snapshot_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub provider_id: String,
    pub model_id: String,
    pub generated_at: String,
    pub budget: SessionContextBudgetDto,
    pub contributors: Vec<SessionContextContributorDto>,
    pub policy_decisions: Vec<SessionContextPolicyDecisionDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_totals: Option<SessionUsageTotalsDto>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionCompactionDiagnosticDto {
    pub code: String,
    pub message: String,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionCompactionRecordDto {
    pub contract_version: u32,
    pub compaction_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub source_run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub summary: String,
    pub covered_run_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered_message_start_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered_message_end_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered_event_start_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub covered_event_end_id: Option<i64>,
    pub source_hash: String,
    pub input_tokens: u64,
    pub summary_tokens: u64,
    pub raw_tail_message_count: u32,
    pub policy_reason: String,
    pub trigger: SessionCompactionTriggerDto,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<SessionCompactionDiagnosticDto>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub superseded_at: Option<String>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompactSessionHistoryResponseDto {
    pub compaction: SessionCompactionRecordDto,
    pub context_snapshot: SessionContextSnapshotDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionContextPolicyDecisionKindDto {
    Compaction,
    MemoryInjection,
    InstructionFile,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionContextPolicyActionDto {
    None,
    CompactNow,
    Blocked,
    Skipped,
    InjectMemory,
    ExcludeMemory,
    IncludeInstruction,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionCompactionTriggerDto {
    Manual,
    Auto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionContextPolicyDecisionDto {
    pub contract_version: u32,
    pub decision_id: String,
    pub kind: SessionContextPolicyDecisionKindDto,
    pub action: SessionContextPolicyActionDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger: Option<SessionCompactionTriggerDto>,
    pub reason_code: String,
    pub message: String,
    pub raw_transcript_preserved: bool,
    pub model_visible: bool,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionCompactionPolicyInput {
    pub manual_requested: bool,
    pub auto_enabled: bool,
    pub provider_supports_compaction: bool,
    pub active_compaction_present: bool,
    pub estimated_tokens: u64,
    pub budget_tokens: Option<u64>,
    pub threshold_percent: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryScopeDto {
    Project,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryKindDto {
    ProjectFact,
    UserPreference,
    Decision,
    SessionSummary,
    Troubleshooting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMemoryReviewStateDto {
    Candidate,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionMemoryDiagnosticDto {
    pub code: String,
    pub message: String,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionMemoryRecordDto {
    pub contract_version: u32,
    pub memory_id: String,
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    pub scope: SessionMemoryScopeDto,
    pub kind: SessionMemoryKindDto,
    pub text: String,
    pub review_state: SessionMemoryReviewStateDto,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<String>,
    pub source_item_ids: Vec<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<SessionMemoryDiagnosticDto>,
    pub redaction: SessionContextRedactionDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListSessionMemoriesRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    #[serde(default)]
    pub include_disabled: bool,
    #[serde(default)]
    pub include_rejected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListSessionMemoriesResponseDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_session_id: Option<String>,
    pub memories: Vec<SessionMemoryRecordDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractSessionMemoryCandidatesRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExtractSessionMemoryCandidatesResponseDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub memories: Vec<SessionMemoryRecordDto>,
    pub created_count: usize,
    pub skipped_duplicate_count: usize,
    pub rejected_count: usize,
    pub diagnostics: Vec<SessionMemoryDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSessionMemoryRequestDto {
    pub project_id: String,
    pub memory_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_state: Option<SessionMemoryReviewStateDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteSessionMemoryRequestDto {
    pub project_id: String,
    pub memory_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BranchAgentSessionRequestDto {
    pub project_id: String,
    pub source_agent_session_id: String,
    pub source_run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default = "default_selected_branch")]
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RewindAgentSessionRequestDto {
    pub project_id: String,
    pub source_agent_session_id: String,
    pub source_run_id: String,
    pub boundary_kind: AgentSessionLineageBoundaryKindDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_checkpoint_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default = "default_selected_branch")]
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSessionBranchResponseDto {
    pub session: AgentSessionDto,
    pub lineage: AgentSessionLineageDto,
    pub replay_run_id: String,
}

fn default_selected_branch() -> bool {
    true
}

pub fn usage_totals_from_agent_usage(record: &AgentUsageRecord) -> SessionUsageTotalsDto {
    SessionUsageTotalsDto {
        project_id: record.project_id.clone(),
        run_id: record.run_id.clone(),
        provider_id: record.provider_id.clone(),
        model_id: record.model_id.clone(),
        input_tokens: record.input_tokens,
        output_tokens: record.output_tokens,
        total_tokens: record.total_tokens,
        estimated_cost_micros: record.estimated_cost_micros,
        source: SessionUsageSourceDto::Provider,
        updated_at: record.updated_at.clone(),
    }
}

pub fn session_compaction_record_dto(record: &AgentCompactionRecord) -> SessionCompactionRecordDto {
    let (summary, summary_redaction) = sanitize_context_text(&record.summary);
    let diagnostic = record.diagnostic.as_ref().map(|diagnostic| {
        let (message, message_redaction) = sanitize_context_text(&diagnostic.message);
        SessionCompactionDiagnosticDto {
            code: diagnostic.code.clone(),
            message,
            redaction: message_redaction,
        }
    });
    let diagnostic_redaction = diagnostic
        .as_ref()
        .map(|diagnostic| diagnostic.redaction.clone())
        .unwrap_or_else(SessionContextRedactionDto::public);
    SessionCompactionRecordDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        compaction_id: record.compaction_id.clone(),
        project_id: record.project_id.clone(),
        agent_session_id: record.agent_session_id.clone(),
        source_run_id: record.source_run_id.clone(),
        provider_id: record.provider_id.clone(),
        model_id: record.model_id.clone(),
        summary,
        covered_run_ids: record.covered_run_ids.clone(),
        covered_message_start_id: record.covered_message_start_id,
        covered_message_end_id: record.covered_message_end_id,
        covered_event_start_id: record.covered_event_start_id,
        covered_event_end_id: record.covered_event_end_id,
        source_hash: record.source_hash.clone(),
        input_tokens: record.input_tokens,
        summary_tokens: record.summary_tokens,
        raw_tail_message_count: record.raw_tail_message_count,
        policy_reason: record.policy_reason.clone(),
        trigger: match record.trigger {
            AgentCompactionTrigger::Manual => SessionCompactionTriggerDto::Manual,
            AgentCompactionTrigger::Auto => SessionCompactionTriggerDto::Auto,
        },
        active: record.active,
        diagnostic,
        created_at: record.created_at.clone(),
        superseded_at: record.superseded_at.clone(),
        redaction: strongest_redaction(&summary_redaction, &diagnostic_redaction),
    }
}

pub fn session_memory_record_dto(record: &AgentMemoryRecord) -> SessionMemoryRecordDto {
    let (text, text_redaction) = sanitize_context_text(&record.text);
    let diagnostic = record.diagnostic.as_ref().map(|diagnostic| {
        let (message, redaction) = sanitize_context_text(&diagnostic.message);
        SessionMemoryDiagnosticDto {
            code: diagnostic.code.clone(),
            message,
            redaction,
        }
    });
    let diagnostic_redaction = diagnostic
        .as_ref()
        .map(|diagnostic| diagnostic.redaction.clone())
        .unwrap_or_else(SessionContextRedactionDto::public);
    SessionMemoryRecordDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        memory_id: record.memory_id.clone(),
        project_id: record.project_id.clone(),
        agent_session_id: record.agent_session_id.clone(),
        scope: match record.scope {
            AgentMemoryScope::Project => SessionMemoryScopeDto::Project,
            AgentMemoryScope::Session => SessionMemoryScopeDto::Session,
        },
        kind: match record.kind {
            AgentMemoryKind::ProjectFact => SessionMemoryKindDto::ProjectFact,
            AgentMemoryKind::UserPreference => SessionMemoryKindDto::UserPreference,
            AgentMemoryKind::Decision => SessionMemoryKindDto::Decision,
            AgentMemoryKind::SessionSummary => SessionMemoryKindDto::SessionSummary,
            AgentMemoryKind::Troubleshooting => SessionMemoryKindDto::Troubleshooting,
        },
        text,
        review_state: match record.review_state {
            AgentMemoryReviewState::Candidate => SessionMemoryReviewStateDto::Candidate,
            AgentMemoryReviewState::Approved => SessionMemoryReviewStateDto::Approved,
            AgentMemoryReviewState::Rejected => SessionMemoryReviewStateDto::Rejected,
        },
        enabled: record.enabled,
        confidence: record.confidence,
        source_run_id: record.source_run_id.clone(),
        source_item_ids: record.source_item_ids.clone(),
        created_at: record.created_at.clone(),
        updated_at: record.updated_at.clone(),
        diagnostic,
        redaction: strongest_redaction(&text_redaction, &diagnostic_redaction),
    }
}

pub fn session_memory_diagnostic_dto(
    code: impl Into<String>,
    message: impl AsRef<str>,
) -> SessionMemoryDiagnosticDto {
    let (message, redaction) = sanitize_context_text(message.as_ref());
    SessionMemoryDiagnosticDto {
        code: code.into(),
        message,
        redaction,
    }
}

pub fn redact_session_context_text(value: &str) -> (String, SessionContextRedactionDto) {
    sanitize_context_text(value)
}

pub fn run_transcript_from_agent_snapshot(
    snapshot: &AgentRunSnapshotRecord,
    usage: Option<&AgentUsageRecord>,
) -> RunTranscriptDto {
    let usage_totals = usage.map(usage_totals_from_agent_usage);
    let mut candidates = Vec::new();
    let mut sequence = 1_u64;

    let (prompt, prompt_redaction) = sanitize_context_text(&snapshot.run.prompt);
    candidates.push(TimelineCandidate {
        created_at: snapshot.run.started_at.clone(),
        source_rank: 5,
        source_id: 0,
        item: SessionTranscriptItemDto {
            contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
            item_id: format!("run_prompt:{}", snapshot.run.run_id),
            project_id: snapshot.run.project_id.clone(),
            agent_session_id: snapshot.run.agent_session_id.clone(),
            run_id: snapshot.run.run_id.clone(),
            provider_id: snapshot.run.provider_id.clone(),
            model_id: snapshot.run.model_id.clone(),
            source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
            source_table: "agent_runs".into(),
            source_id: snapshot.run.run_id.clone(),
            sequence: 0,
            created_at: snapshot.run.started_at.clone(),
            kind: SessionTranscriptItemKindDto::Message,
            actor: SessionTranscriptActorDto::User,
            title: Some("Run prompt".into()),
            text: Some(prompt),
            summary: None,
            tool_call_id: None,
            tool_name: None,
            tool_state: None,
            file_path: None,
            checkpoint_kind: None,
            action_id: None,
            redaction: prompt_redaction,
        },
    });

    for message in &snapshot.messages {
        let (text, redaction) = sanitize_context_text(&message.content);
        candidates.push(TimelineCandidate {
            created_at: message.created_at.clone(),
            source_rank: 10,
            source_id: message.id,
            item: SessionTranscriptItemDto {
                contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
                item_id: format!("message:{}", message.id),
                project_id: message.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: message.run_id.clone(),
                provider_id: snapshot.run.provider_id.clone(),
                model_id: snapshot.run.model_id.clone(),
                source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
                source_table: "agent_messages".into(),
                source_id: message.id.to_string(),
                sequence: 0,
                created_at: message.created_at.clone(),
                kind: SessionTranscriptItemKindDto::Message,
                actor: actor_from_message_role(&message.role),
                title: Some(format!("{:?} message", message.role)),
                text: Some(text),
                summary: None,
                tool_call_id: None,
                tool_name: None,
                tool_state: None,
                file_path: None,
                checkpoint_kind: None,
                action_id: None,
                redaction,
            },
        });
    }

    for event in &snapshot.events {
        let payload =
            serde_json::from_str::<JsonValue>(&event.payload_json).unwrap_or(JsonValue::Null);
        let (title, text, summary, redaction) = transcript_parts_from_event(event, &payload);
        candidates.push(TimelineCandidate {
            created_at: event.created_at.clone(),
            source_rank: 20,
            source_id: event.id,
            item: SessionTranscriptItemDto {
                contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
                item_id: format!("event:{}", event.id),
                project_id: event.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: event.run_id.clone(),
                provider_id: snapshot.run.provider_id.clone(),
                model_id: snapshot.run.model_id.clone(),
                source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
                source_table: "agent_events".into(),
                source_id: event.id.to_string(),
                sequence: 0,
                created_at: event.created_at.clone(),
                kind: transcript_kind_from_event(&event.event_kind),
                actor: actor_from_event(&event.event_kind),
                title,
                text,
                summary,
                tool_call_id: payload_string(&payload, "toolCallId"),
                tool_name: payload_string(&payload, "toolName"),
                tool_state: transcript_tool_state_from_event(event, &payload),
                file_path: sanitize_optional_path(payload_string(&payload, "path")).0,
                checkpoint_kind: None,
                action_id: payload_string(&payload, "actionId"),
                redaction,
            },
        });
    }

    for tool_call in &snapshot.tool_calls {
        let (summary, redaction) = tool_call_summary(tool_call);
        candidates.push(TimelineCandidate {
            created_at: tool_call.started_at.clone(),
            source_rank: 30,
            source_id: 0,
            item: SessionTranscriptItemDto {
                contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
                item_id: format!("tool_call:{}", tool_call.tool_call_id),
                project_id: tool_call.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: tool_call.run_id.clone(),
                provider_id: snapshot.run.provider_id.clone(),
                model_id: snapshot.run.model_id.clone(),
                source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
                source_table: "agent_tool_calls".into(),
                source_id: tool_call.tool_call_id.clone(),
                sequence: 0,
                created_at: tool_call.started_at.clone(),
                kind: SessionTranscriptItemKindDto::ToolCall,
                actor: SessionTranscriptActorDto::Tool,
                title: Some(format!("Tool call `{}`", tool_call.tool_name)),
                text: None,
                summary: Some(summary),
                tool_call_id: Some(tool_call.tool_call_id.clone()),
                tool_name: Some(tool_call.tool_name.clone()),
                tool_state: Some(tool_state_from_agent_tool_call(&tool_call.state)),
                file_path: None,
                checkpoint_kind: None,
                action_id: None,
                redaction,
            },
        });
    }

    for file_change in &snapshot.file_changes {
        let (path, path_redaction) = sanitize_path(&file_change.path);
        candidates.push(TimelineCandidate {
            created_at: file_change.created_at.clone(),
            source_rank: 40,
            source_id: file_change.id,
            item: SessionTranscriptItemDto {
                contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
                item_id: format!("file_change:{}", file_change.id),
                project_id: file_change.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: file_change.run_id.clone(),
                provider_id: snapshot.run.provider_id.clone(),
                model_id: snapshot.run.model_id.clone(),
                source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
                source_table: "agent_file_changes".into(),
                source_id: file_change.id.to_string(),
                sequence: 0,
                created_at: file_change.created_at.clone(),
                kind: SessionTranscriptItemKindDto::FileChange,
                actor: SessionTranscriptActorDto::Xero,
                title: Some("File changed".into()),
                text: None,
                summary: Some(format!("{}: {}", file_change.operation, path)),
                tool_call_id: None,
                tool_name: None,
                tool_state: None,
                file_path: Some(path),
                checkpoint_kind: None,
                action_id: None,
                redaction: path_redaction,
            },
        });
    }

    for checkpoint in &snapshot.checkpoints {
        let (summary, redaction) = sanitize_context_text(&checkpoint.summary);
        candidates.push(TimelineCandidate {
            created_at: checkpoint.created_at.clone(),
            source_rank: 50,
            source_id: checkpoint.id,
            item: SessionTranscriptItemDto {
                contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
                item_id: format!("checkpoint:{}", checkpoint.id),
                project_id: checkpoint.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: checkpoint.run_id.clone(),
                provider_id: snapshot.run.provider_id.clone(),
                model_id: snapshot.run.model_id.clone(),
                source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
                source_table: "agent_checkpoints".into(),
                source_id: checkpoint.id.to_string(),
                sequence: 0,
                created_at: checkpoint.created_at.clone(),
                kind: SessionTranscriptItemKindDto::Checkpoint,
                actor: SessionTranscriptActorDto::Xero,
                title: Some("Checkpoint".into()),
                text: None,
                summary: Some(summary),
                tool_call_id: None,
                tool_name: None,
                tool_state: None,
                file_path: None,
                checkpoint_kind: Some(checkpoint.checkpoint_kind.clone()),
                action_id: None,
                redaction,
            },
        });
    }

    for action in &snapshot.action_requests {
        let (detail, redaction) = sanitize_context_text(&action.detail);
        candidates.push(TimelineCandidate {
            created_at: action.created_at.clone(),
            source_rank: 60,
            source_id: 0,
            item: SessionTranscriptItemDto {
                contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
                item_id: format!("action_request:{}", action.action_id),
                project_id: action.project_id.clone(),
                agent_session_id: snapshot.run.agent_session_id.clone(),
                run_id: action.run_id.clone(),
                provider_id: snapshot.run.provider_id.clone(),
                model_id: snapshot.run.model_id.clone(),
                source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
                source_table: "agent_action_requests".into(),
                source_id: action.action_id.clone(),
                sequence: 0,
                created_at: action.created_at.clone(),
                kind: SessionTranscriptItemKindDto::ActionRequest,
                actor: SessionTranscriptActorDto::Operator,
                title: Some(action.title.clone()),
                text: Some(detail),
                summary: Some(action.status.clone()),
                tool_call_id: None,
                tool_name: None,
                tool_state: None,
                file_path: None,
                checkpoint_kind: None,
                action_id: Some(action.action_id.clone()),
                redaction,
            },
        });
    }

    candidates.sort_by(compare_timeline_candidates);
    let items = candidates
        .into_iter()
        .map(|mut candidate| {
            candidate.item.sequence = sequence;
            sequence = sequence.saturating_add(1);
            candidate.item
        })
        .collect::<Vec<_>>();

    let redaction = combine_redactions(items.iter().map(|item| &item.redaction));
    RunTranscriptDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        project_id: snapshot.run.project_id.clone(),
        agent_session_id: snapshot.run.agent_session_id.clone(),
        run_id: snapshot.run.run_id.clone(),
        provider_id: snapshot.run.provider_id.clone(),
        model_id: snapshot.run.model_id.clone(),
        status: agent_run_status_sql_value(&snapshot.run.status).into(),
        source_kind: SessionTranscriptSourceKindDto::OwnedAgent,
        started_at: snapshot.run.started_at.clone(),
        completed_at: terminal_time_for_agent_run(&snapshot.run),
        items,
        usage_totals,
        redaction,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_transcript_from_runtime_stream_items(
    project_id: impl Into<String>,
    agent_session_id: impl Into<String>,
    provider_id: impl Into<String>,
    model_id: impl Into<String>,
    status: impl Into<String>,
    started_at: impl Into<String>,
    completed_at: Option<String>,
    items: &[RuntimeStreamItemDto],
) -> RunTranscriptDto {
    let project_id = project_id.into();
    let agent_session_id = agent_session_id.into();
    let provider_id = provider_id.into();
    let model_id = model_id.into();
    let status = status.into();
    let started_at = started_at.into();
    let run_id = items
        .first()
        .map(|item| item.run_id.clone())
        .unwrap_or_else(|| "runtime-run-unavailable".into());
    let mut transcript_items = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            runtime_stream_transcript_item(
                &project_id,
                &agent_session_id,
                &provider_id,
                &model_id,
                index,
                item,
            )
        })
        .collect::<Vec<_>>();
    transcript_items.sort_by(|left, right| {
        left.sequence
            .cmp(&right.sequence)
            .then_with(|| left.item_id.cmp(&right.item_id))
    });
    for (index, item) in transcript_items.iter_mut().enumerate() {
        item.sequence = (index as u64).saturating_add(1);
    }
    let redaction = combine_redactions(transcript_items.iter().map(|item| &item.redaction));
    RunTranscriptDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        project_id,
        agent_session_id,
        run_id,
        provider_id,
        model_id,
        status,
        source_kind: SessionTranscriptSourceKindDto::RuntimeStream,
        started_at,
        completed_at,
        items: transcript_items,
        usage_totals: None,
        redaction,
    }
}

pub fn session_transcript_from_runs(
    session: &AgentSessionRecord,
    runs: Vec<RunTranscriptDto>,
) -> SessionTranscriptDto {
    let (title, title_redaction) = sanitize_context_text(&session.title);
    let (summary, summary_redaction) = sanitize_context_text(&session.summary);
    let mut run_summaries = runs
        .iter()
        .map(|run| RunTranscriptSummaryDto {
            project_id: run.project_id.clone(),
            agent_session_id: run.agent_session_id.clone(),
            run_id: run.run_id.clone(),
            provider_id: run.provider_id.clone(),
            model_id: run.model_id.clone(),
            status: run.status.clone(),
            started_at: run.started_at.clone(),
            completed_at: run.completed_at.clone(),
            item_count: run.items.len(),
            usage_totals: run.usage_totals.clone(),
        })
        .collect::<Vec<_>>();
    run_summaries.sort_by(|left, right| {
        left.started_at
            .cmp(&right.started_at)
            .then_with(|| left.run_id.cmp(&right.run_id))
    });

    let mut items = runs
        .into_iter()
        .flat_map(|run| run.items.into_iter())
        .collect::<Vec<_>>();
    items.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.run_id.cmp(&right.run_id))
            .then_with(|| left.sequence.cmp(&right.sequence))
    });
    for (index, item) in items.iter_mut().enumerate() {
        item.sequence = (index as u64).saturating_add(1);
    }

    let redaction = combine_redactions(
        items
            .iter()
            .map(|item| &item.redaction)
            .chain([&title_redaction, &summary_redaction]),
    );
    let usage_totals = aggregate_usage_totals(&run_summaries);
    SessionTranscriptDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        project_id: session.project_id.clone(),
        agent_session_id: session.agent_session_id.clone(),
        title,
        summary,
        status: match session.status {
            AgentSessionStatus::Active => AgentSessionTranscriptStatusDto::Active,
            AgentSessionStatus::Archived => AgentSessionTranscriptStatusDto::Archived,
        },
        archived: matches!(session.status, AgentSessionStatus::Archived),
        archived_at: session.archived_at.clone(),
        runs: run_summaries,
        items,
        usage_totals,
        redaction,
    }
}

pub fn evaluate_compaction_policy(
    input: SessionCompactionPolicyInput,
) -> SessionContextPolicyDecisionDto {
    if input.manual_requested {
        if input.provider_supports_compaction {
            return policy_decision(
                "compaction:manual:ready",
                SessionContextPolicyActionDto::CompactNow,
                Some(SessionCompactionTriggerDto::Manual),
                "manual_compact_requested",
                "Manual compact can run. Raw transcript rows remain durable for search and export.",
                true,
                false,
            );
        }
        return policy_decision(
            "compaction:manual:blocked",
            SessionContextPolicyActionDto::Blocked,
            Some(SessionCompactionTriggerDto::Manual),
            "manual_compact_provider_unavailable",
            "Manual compact is unavailable because the active provider cannot produce a compaction summary.",
            true,
            false,
        );
    }

    if input.active_compaction_present {
        return policy_decision(
            "compaction:auto:already_active",
            SessionContextPolicyActionDto::None,
            Some(SessionCompactionTriggerDto::Auto),
            "active_compaction_present",
            "An active compaction summary is already available for replay.",
            true,
            true,
        );
    }

    if !input.auto_enabled {
        return policy_decision(
            "compaction:auto:disabled",
            SessionContextPolicyActionDto::Skipped,
            Some(SessionCompactionTriggerDto::Auto),
            "auto_compact_disabled",
            "Auto-compact is disabled for this session.",
            true,
            false,
        );
    }

    let Some(budget_tokens) = input.budget_tokens else {
        return policy_decision(
            "compaction:auto:unknown_budget",
            SessionContextPolicyActionDto::Skipped,
            Some(SessionCompactionTriggerDto::Auto),
            "context_budget_unknown",
            "Xero cannot decide auto-compact pressure because the provider context budget is unknown.",
            true,
            false,
        );
    };

    let threshold_percent = input
        .threshold_percent
        .unwrap_or(DEFAULT_AUTO_COMPACT_THRESHOLD_PERCENT)
        .clamp(1, 100);
    let threshold_tokens = budget_tokens
        .saturating_mul(u64::from(threshold_percent))
        .saturating_add(99)
        / 100;
    if input.estimated_tokens < threshold_tokens {
        return policy_decision(
            "compaction:auto:below_threshold",
            SessionContextPolicyActionDto::None,
            Some(SessionCompactionTriggerDto::Auto),
            "below_auto_compact_threshold",
            "The session is below the configured context-pressure threshold.",
            true,
            false,
        );
    }

    if input.provider_supports_compaction {
        policy_decision(
            "compaction:auto:ready",
            SessionContextPolicyActionDto::CompactNow,
            Some(SessionCompactionTriggerDto::Auto),
            "auto_compact_threshold_reached",
            "Auto-compact should run before the next provider turn. Raw transcript rows remain durable for search and export.",
            true,
            false,
        )
    } else {
        policy_decision(
            "compaction:auto:blocked",
            SessionContextPolicyActionDto::Blocked,
            Some(SessionCompactionTriggerDto::Auto),
            "auto_compact_provider_unavailable",
            "Auto-compact threshold was reached, but the active provider cannot produce a compaction summary.",
            true,
            false,
        )
    }
}

pub fn approved_memory_context_contributors(
    memories: &[SessionMemoryRecordDto],
    memory_enabled: bool,
) -> Vec<SessionContextContributorDto> {
    if !memory_enabled {
        return Vec::new();
    }

    let mut approved = memories
        .iter()
        .filter(|memory| {
            memory.enabled && memory.review_state == SessionMemoryReviewStateDto::Approved
        })
        .cloned()
        .collect::<Vec<_>>();
    approved.sort_by(|left, right| {
        memory_scope_rank(&left.scope)
            .cmp(&memory_scope_rank(&right.scope))
            .then_with(|| memory_kind_rank(&left.kind).cmp(&memory_kind_rank(&right.kind)))
            .then_with(|| left.created_at.cmp(&right.created_at))
            .then_with(|| left.memory_id.cmp(&right.memory_id))
    });

    approved
        .into_iter()
        .enumerate()
        .map(|(index, memory)| {
            let (text, text_redaction) = sanitize_context_text(&memory.text);
            SessionContextContributorDto {
                contributor_id: format!("memory:{}", memory.memory_id),
                kind: SessionContextContributorKindDto::ApprovedMemory,
                label: memory_label(&memory),
                project_id: Some(memory.project_id),
                agent_session_id: memory.agent_session_id,
                run_id: memory.source_run_id,
                source_id: Some(memory.memory_id),
                sequence: (index as u64).saturating_add(1),
                estimated_tokens: estimate_tokens(&text),
                estimated_chars: text.chars().count() as u64,
                included: true,
                model_visible: true,
                text: Some(text),
                redaction: strongest_redaction(&memory.redaction, &text_redaction),
            }
        })
        .collect()
}

pub fn context_budget(
    estimated_tokens: u64,
    budget_tokens: Option<u64>,
) -> SessionContextBudgetDto {
    context_budget_with_source(
        estimated_tokens,
        budget_tokens,
        SessionUsageSourceDto::Estimated,
    )
}

pub fn context_budget_with_source(
    estimated_tokens: u64,
    budget_tokens: Option<u64>,
    estimation_source: SessionUsageSourceDto,
) -> SessionContextBudgetDto {
    let Some(budget) = budget_tokens else {
        return SessionContextBudgetDto {
            budget_tokens: None,
            estimated_tokens,
            estimation_source,
            pressure: SessionContextBudgetPressureDto::Unknown,
            known_provider_budget: false,
        };
    };

    let percent = if budget == 0 {
        101
    } else {
        estimated_tokens.saturating_mul(100) / budget
    };
    let pressure = match percent {
        0..=49 => SessionContextBudgetPressureDto::Low,
        50..=79 => SessionContextBudgetPressureDto::Medium,
        80..=100 => SessionContextBudgetPressureDto::High,
        _ => SessionContextBudgetPressureDto::Over,
    };
    SessionContextBudgetDto {
        budget_tokens: Some(budget),
        estimated_tokens,
        estimation_source,
        pressure,
        known_provider_budget: true,
    }
}

pub fn provider_context_budget_tokens(provider_id: &str, model_id: &str) -> Option<u64> {
    let provider = provider_id.trim().to_ascii_lowercase();
    let model = model_id.trim().to_ascii_lowercase();
    if provider.is_empty()
        || model.is_empty()
        || provider == "unavailable"
        || model == "unavailable"
    {
        return None;
    }

    if model.contains("gemini-1.5-pro")
        || model.contains("gemini-1.5-flash")
        || model.contains("gemini-2.")
    {
        return Some(1_000_000);
    }
    if model.contains("claude")
        || provider == "anthropic"
        || provider == "bedrock"
        || provider == "vertex"
    {
        return Some(200_000);
    }
    if model.contains("gpt-5")
        || model.contains("gpt-4.1")
        || model.contains("gpt-4o")
        || model.contains("o3")
        || model.contains("o4")
        || provider == "openai_codex"
        || (provider == "github_models" && model.contains("gpt"))
        || model.contains("mistral-large")
        || model.contains("codestral")
    {
        return Some(128_000);
    }

    None
}

pub fn validate_run_transcript_contract(transcript: &RunTranscriptDto) -> Result<(), String> {
    if transcript.contract_version != XERO_SESSION_CONTEXT_CONTRACT_VERSION {
        return Err("run transcript contract version is unsupported".into());
    }
    let mut previous_sequence = 0_u64;
    for item in &transcript.items {
        if item.project_id != transcript.project_id {
            return Err("transcript item project id must match the run transcript".into());
        }
        if item.agent_session_id != transcript.agent_session_id {
            return Err("transcript item session id must match the run transcript".into());
        }
        if item.run_id != transcript.run_id {
            return Err("transcript item run id must match the run transcript".into());
        }
        if item.provider_id != transcript.provider_id || item.model_id != transcript.model_id {
            return Err("transcript item provider/model must match the run transcript".into());
        }
        if item.sequence <= previous_sequence {
            return Err("transcript item sequences must be strictly increasing".into());
        }
        previous_sequence = item.sequence;
    }
    ensure_secret_free_json(transcript)
}

pub fn validate_session_transcript_contract(
    transcript: &SessionTranscriptDto,
) -> Result<(), String> {
    if transcript.contract_version != XERO_SESSION_CONTEXT_CONTRACT_VERSION {
        return Err("session transcript contract version is unsupported".into());
    }
    if transcript.archived && transcript.archived_at.is_none() {
        return Err("archived session transcripts must include archived_at".into());
    }
    if matches!(transcript.status, AgentSessionTranscriptStatusDto::Archived)
        && !transcript.archived
    {
        return Err("archived session transcripts must set archived=true".into());
    }

    let mut previous_sequence = 0_u64;
    for item in &transcript.items {
        if item.project_id != transcript.project_id {
            return Err("transcript item project id must match the session transcript".into());
        }
        if item.agent_session_id != transcript.agent_session_id {
            return Err("transcript item session id must match the session transcript".into());
        }
        if item.sequence <= previous_sequence {
            return Err("transcript item sequences must be strictly increasing".into());
        }
        previous_sequence = item.sequence;
    }
    ensure_secret_free_json(transcript)
}

pub fn validate_export_payload_contract(
    payload: &SessionTranscriptExportPayloadDto,
) -> Result<(), String> {
    if payload.contract_version != XERO_SESSION_CONTEXT_CONTRACT_VERSION {
        return Err("session transcript export contract version is unsupported".into());
    }
    validate_session_transcript_contract(&payload.transcript)?;
    ensure_secret_free_json(payload)
}

pub fn validate_context_snapshot_contract(
    snapshot: &SessionContextSnapshotDto,
) -> Result<(), String> {
    if snapshot.contract_version != XERO_SESSION_CONTEXT_CONTRACT_VERSION {
        return Err("context snapshot contract version is unsupported".into());
    }
    let mut previous_sequence = 0_u64;
    for contributor in &snapshot.contributors {
        if contributor.sequence <= previous_sequence {
            return Err("context contributor sequences must be strictly increasing".into());
        }
        previous_sequence = contributor.sequence;
        if contributor.model_visible && !contributor.included {
            return Err("model-visible contributors must also be included".into());
        }
    }
    ensure_secret_free_json(snapshot)
}

pub fn validate_session_compaction_record_contract(
    compaction: &SessionCompactionRecordDto,
) -> Result<(), String> {
    if compaction.contract_version != XERO_SESSION_CONTEXT_CONTRACT_VERSION {
        return Err("session compaction contract version is unsupported".into());
    }
    if compaction.covered_run_ids.is_empty() {
        return Err("session compaction records must cover at least one run".into());
    }
    if let (Some(start), Some(end)) = (
        compaction.covered_message_start_id,
        compaction.covered_message_end_id,
    ) {
        if start <= 0 || start > end {
            return Err("session compaction message range is invalid".into());
        }
    }
    if let (Some(start), Some(end)) = (
        compaction.covered_event_start_id,
        compaction.covered_event_end_id,
    ) {
        if start <= 0 || start > end {
            return Err("session compaction event range is invalid".into());
        }
    }
    if compaction.source_hash.len() != 64
        || !compaction
            .source_hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err("session compaction source hash must be lowercase SHA-256".into());
    }
    ensure_secret_free_json(compaction)
}

pub fn validate_session_memory_record_contract(
    memory: &SessionMemoryRecordDto,
) -> Result<(), String> {
    if memory.contract_version != XERO_SESSION_CONTEXT_CONTRACT_VERSION {
        return Err("session memory contract version is unsupported".into());
    }
    if memory.text.trim().is_empty() {
        return Err("session memory text must not be empty".into());
    }
    match memory.scope {
        SessionMemoryScopeDto::Project if memory.agent_session_id.is_some() => {
            return Err("project memory must not be session scoped".into());
        }
        SessionMemoryScopeDto::Session if memory.agent_session_id.is_none() => {
            return Err("session memory must include an agent session id".into());
        }
        _ => {}
    }
    if memory.review_state != SessionMemoryReviewStateDto::Approved && memory.enabled {
        return Err("only approved memories can be enabled".into());
    }
    if let Some(confidence) = memory.confidence {
        if confidence > 100 {
            return Err("session memory confidence must be between 0 and 100".into());
        }
    }
    ensure_secret_free_json(memory)
}

struct TimelineCandidate {
    created_at: String,
    source_rank: u8,
    source_id: i64,
    item: SessionTranscriptItemDto,
}

fn compare_timeline_candidates(left: &TimelineCandidate, right: &TimelineCandidate) -> Ordering {
    left.created_at
        .cmp(&right.created_at)
        .then_with(|| left.source_rank.cmp(&right.source_rank))
        .then_with(|| left.source_id.cmp(&right.source_id))
        .then_with(|| left.item.item_id.cmp(&right.item.item_id))
}

fn actor_from_message_role(
    role: &crate::db::project_store::AgentMessageRole,
) -> SessionTranscriptActorDto {
    match role {
        crate::db::project_store::AgentMessageRole::System => SessionTranscriptActorDto::System,
        crate::db::project_store::AgentMessageRole::Developer => {
            SessionTranscriptActorDto::Developer
        }
        crate::db::project_store::AgentMessageRole::User => SessionTranscriptActorDto::User,
        crate::db::project_store::AgentMessageRole::Assistant => {
            SessionTranscriptActorDto::Assistant
        }
        crate::db::project_store::AgentMessageRole::Tool => SessionTranscriptActorDto::Tool,
    }
}

fn transcript_kind_from_event(kind: &AgentRunEventKind) -> SessionTranscriptItemKindDto {
    match kind {
        AgentRunEventKind::MessageDelta => SessionTranscriptItemKindDto::Message,
        AgentRunEventKind::ReasoningSummary => SessionTranscriptItemKindDto::Reasoning,
        AgentRunEventKind::ToolStarted | AgentRunEventKind::ToolDelta => {
            SessionTranscriptItemKindDto::ToolCall
        }
        AgentRunEventKind::ToolCompleted => SessionTranscriptItemKindDto::ToolResult,
        AgentRunEventKind::FileChanged => SessionTranscriptItemKindDto::FileChange,
        AgentRunEventKind::ActionRequired => SessionTranscriptItemKindDto::ActionRequest,
        AgentRunEventKind::RunCompleted => SessionTranscriptItemKindDto::Complete,
        AgentRunEventKind::RunFailed => SessionTranscriptItemKindDto::Failure,
        AgentRunEventKind::CommandOutput
        | AgentRunEventKind::ValidationStarted
        | AgentRunEventKind::ValidationCompleted => SessionTranscriptItemKindDto::Activity,
    }
}

fn actor_from_event(kind: &AgentRunEventKind) -> SessionTranscriptActorDto {
    match kind {
        AgentRunEventKind::MessageDelta | AgentRunEventKind::ReasoningSummary => {
            SessionTranscriptActorDto::Assistant
        }
        AgentRunEventKind::ToolStarted
        | AgentRunEventKind::ToolDelta
        | AgentRunEventKind::ToolCompleted => SessionTranscriptActorDto::Tool,
        AgentRunEventKind::ActionRequired => SessionTranscriptActorDto::Operator,
        _ => SessionTranscriptActorDto::Xero,
    }
}

fn transcript_parts_from_event(
    event: &crate::db::project_store::AgentEventRecord,
    payload: &JsonValue,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    SessionContextRedactionDto,
) {
    let title = match event.event_kind {
        AgentRunEventKind::MessageDelta => Some("Message delta".into()),
        AgentRunEventKind::ReasoningSummary => Some("Reasoning".into()),
        AgentRunEventKind::ToolStarted => Some("Tool started".into()),
        AgentRunEventKind::ToolDelta => Some("Tool arguments".into()),
        AgentRunEventKind::ToolCompleted => Some("Tool completed".into()),
        AgentRunEventKind::FileChanged => Some("File changed".into()),
        AgentRunEventKind::CommandOutput => Some("Command output".into()),
        AgentRunEventKind::ValidationStarted => Some("Validation started".into()),
        AgentRunEventKind::ValidationCompleted => Some("Validation completed".into()),
        AgentRunEventKind::ActionRequired => {
            payload_string(payload, "title").or_else(|| Some("Action required".into()))
        }
        AgentRunEventKind::RunCompleted => Some("Run completed".into()),
        AgentRunEventKind::RunFailed => Some("Run failed".into()),
    };
    let raw_text = payload_string(payload, "text")
        .or_else(|| payload_string(payload, "summary"))
        .or_else(|| payload_string(payload, "detail"))
        .or_else(|| payload_string(payload, "message"));
    let (text, text_redaction) = raw_text
        .as_deref()
        .map(sanitize_context_text)
        .map(|(text, redaction)| (Some(text), redaction))
        .unwrap_or_else(|| (None, SessionContextRedactionDto::public()));
    let (summary, summary_redaction) = summarize_json_payload(payload);
    let redaction = strongest_redaction(&text_redaction, &summary_redaction);
    (title, text, summary, redaction)
}

fn transcript_tool_state_from_event(
    event: &crate::db::project_store::AgentEventRecord,
    payload: &JsonValue,
) -> Option<SessionTranscriptToolStateDto> {
    match event.event_kind {
        AgentRunEventKind::ToolStarted | AgentRunEventKind::ToolDelta => {
            Some(SessionTranscriptToolStateDto::Running)
        }
        AgentRunEventKind::ToolCompleted => {
            if payload
                .get("ok")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false)
            {
                Some(SessionTranscriptToolStateDto::Succeeded)
            } else {
                Some(SessionTranscriptToolStateDto::Failed)
            }
        }
        _ => None,
    }
}

fn tool_state_from_agent_tool_call(state: &AgentToolCallState) -> SessionTranscriptToolStateDto {
    match state {
        AgentToolCallState::Pending => SessionTranscriptToolStateDto::Pending,
        AgentToolCallState::Running => SessionTranscriptToolStateDto::Running,
        AgentToolCallState::Succeeded => SessionTranscriptToolStateDto::Succeeded,
        AgentToolCallState::Failed => SessionTranscriptToolStateDto::Failed,
    }
}

fn runtime_stream_transcript_item(
    project_id: &str,
    agent_session_id: &str,
    provider_id: &str,
    model_id: &str,
    index: usize,
    item: &RuntimeStreamItemDto,
) -> SessionTranscriptItemDto {
    let raw_text = item
        .text
        .as_deref()
        .or(item.detail.as_deref())
        .or(item.message.as_deref());
    let (text, text_redaction) = raw_text
        .map(sanitize_context_text)
        .map(|(text, redaction)| (Some(text), redaction))
        .unwrap_or_else(|| (None, SessionContextRedactionDto::public()));
    let title = item
        .title
        .as_deref()
        .map(sanitize_context_text)
        .map(|(value, _)| value);
    let redaction = text_redaction;
    SessionTranscriptItemDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        item_id: format!("runtime_stream:{}:{}", item.run_id, item.sequence),
        project_id: project_id.into(),
        agent_session_id: agent_session_id.into(),
        run_id: item.run_id.clone(),
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        source_kind: SessionTranscriptSourceKindDto::RuntimeStream,
        source_table: "runtime_stream_items".into(),
        source_id: item.sequence.to_string(),
        sequence: item.sequence.max((index as u64).saturating_add(1)),
        created_at: item.created_at.clone(),
        kind: transcript_kind_from_runtime_stream(&item.kind),
        actor: actor_from_runtime_stream(&item.kind),
        title,
        text,
        summary: None,
        tool_call_id: item.tool_call_id.clone(),
        tool_name: item.tool_name.clone(),
        tool_state: item.tool_state.as_ref().map(tool_state_from_runtime_stream),
        file_path: None,
        checkpoint_kind: None,
        action_id: item.action_id.clone(),
        redaction,
    }
}

fn transcript_kind_from_runtime_stream(
    kind: &RuntimeStreamItemKind,
) -> SessionTranscriptItemKindDto {
    match kind {
        RuntimeStreamItemKind::Transcript => SessionTranscriptItemKindDto::Message,
        RuntimeStreamItemKind::Tool => SessionTranscriptItemKindDto::ToolResult,
        RuntimeStreamItemKind::Skill | RuntimeStreamItemKind::Activity => {
            SessionTranscriptItemKindDto::Activity
        }
        RuntimeStreamItemKind::ActionRequired => SessionTranscriptItemKindDto::ActionRequest,
        RuntimeStreamItemKind::Complete => SessionTranscriptItemKindDto::Complete,
        RuntimeStreamItemKind::Failure => SessionTranscriptItemKindDto::Failure,
    }
}

fn actor_from_runtime_stream(kind: &RuntimeStreamItemKind) -> SessionTranscriptActorDto {
    match kind {
        RuntimeStreamItemKind::Transcript => SessionTranscriptActorDto::Assistant,
        RuntimeStreamItemKind::Tool | RuntimeStreamItemKind::Skill => {
            SessionTranscriptActorDto::Tool
        }
        RuntimeStreamItemKind::ActionRequired => SessionTranscriptActorDto::Operator,
        _ => SessionTranscriptActorDto::Runtime,
    }
}

fn tool_state_from_runtime_stream(state: &RuntimeToolCallState) -> SessionTranscriptToolStateDto {
    match state {
        RuntimeToolCallState::Pending => SessionTranscriptToolStateDto::Pending,
        RuntimeToolCallState::Running => SessionTranscriptToolStateDto::Running,
        RuntimeToolCallState::Succeeded => SessionTranscriptToolStateDto::Succeeded,
        RuntimeToolCallState::Failed => SessionTranscriptToolStateDto::Failed,
    }
}

fn tool_call_summary(
    tool_call: &crate::db::project_store::AgentToolCallRecord,
) -> (String, SessionContextRedactionDto) {
    let state = format!("{:?}", tool_call.state).to_ascii_lowercase();
    let base = format!("{} ended {state}.", tool_call.tool_name);
    if let Some(error) = tool_call.error.as_ref() {
        let (message, redaction) = sanitize_context_text(&error.message);
        return (format!("{base} {message}"), redaction);
    }
    if let Some(result_json) = tool_call.result_json.as_deref() {
        let (summary, redaction) = sanitize_context_text(&summarize_json_text(result_json));
        return (format!("{base} {summary}"), redaction);
    }
    (base, SessionContextRedactionDto::public())
}

fn summarize_json_payload(value: &JsonValue) -> (Option<String>, SessionContextRedactionDto) {
    if value.is_null() {
        return (None, SessionContextRedactionDto::public());
    }
    let summary = summarize_json_text(&value.to_string());
    let (summary, redaction) = sanitize_context_text(&summary);
    (Some(summary), redaction)
}

fn summarize_json_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() <= 320 {
        trimmed.into()
    } else {
        let truncated = trimmed.chars().take(320).collect::<String>();
        format!("{truncated}...")
    }
}

fn payload_string(payload: &JsonValue, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn sanitize_context_text(value: &str) -> (String, SessionContextRedactionDto) {
    if let Some(reason) = find_session_context_sensitive_content(value) {
        let class = if reason.contains("prompt-injection") || reason.contains("transcript") {
            SessionContextRedactionClassDto::Transcript
        } else if reason.contains("payload") {
            SessionContextRedactionClassDto::RawPayload
        } else {
            SessionContextRedactionClassDto::Secret
        };
        return (
            REDACTED_TEXT.into(),
            SessionContextRedactionDto::redacted(class, reason),
        );
    }
    if looks_like_secret_bearing_path(value) {
        return (
            REDACTED_PATH.into(),
            SessionContextRedactionDto::redacted(
                SessionContextRedactionClassDto::LocalPath,
                "local secret-bearing path",
            ),
        );
    }
    (value.into(), SessionContextRedactionDto::public())
}

fn sanitize_path(value: &str) -> (String, SessionContextRedactionDto) {
    if looks_like_secret_bearing_path(value)
        || find_session_context_sensitive_content(value).is_some()
    {
        return (
            REDACTED_PATH.into(),
            SessionContextRedactionDto::redacted(
                SessionContextRedactionClassDto::LocalPath,
                "local or secret-bearing path",
            ),
        );
    }
    (value.into(), SessionContextRedactionDto::public())
}

fn sanitize_optional_path(value: Option<String>) -> (Option<String>, SessionContextRedactionDto) {
    value
        .map(|path| {
            let (path, redaction) = sanitize_path(&path);
            (Some(path), redaction)
        })
        .unwrap_or_else(|| (None, SessionContextRedactionDto::public()))
}

fn looks_like_secret_bearing_path(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.contains("/.ssh/")
        || normalized.contains("/.aws/")
        || normalized.contains("/.config/")
        || normalized.contains(".env")
        || normalized.contains("credentials")
        || normalized.contains("keychain")
}

fn find_session_context_sensitive_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();

    if looks_like_prompt_injection_text(&normalized) {
        return Some("prompt-injection-shaped memory text");
    }

    if looks_like_endpoint_credential(value) {
        return Some("endpoint credential material");
    }

    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("api_key")
        || normalized.contains("api-key")
        || normalized.contains("apikey")
        || normalized.contains("api key")
        || normalized.contains("anthropic_api_key")
        || normalized.contains("authorization=")
        || normalized.contains("auth token")
        || normalized.contains("authtoken")
        || normalized.contains("aws_access_key_id")
        || normalized.contains("aws_secret_access_key")
        || normalized.contains("aws_session_token")
        || normalized.contains("_auth")
        || normalized.contains("authorization:")
        || normalized.contains("\"authorization\"")
        || normalized.contains("bearer ")
        || normalized.contains("bearer:")
        || normalized.contains("bearer=")
        || normalized.contains("client_secret")
        || normalized.contains("client-secret")
        || normalized.contains("github_token")
        || normalized.contains("google_oauth_access_token")
        || normalized.contains("oauth")
        || normalized.contains("openai_api_key")
        || normalized.contains("password")
        || normalized.contains("private key")
        || normalized.contains("private_key")
        || normalized.contains("private-key")
        || normalized.contains("secret")
        || normalized.contains("session_id=")
        || normalized.contains("session_id\":\"")
        || normalized.contains("session_token")
        || normalized.contains("session-token")
        || normalized.contains("token=")
        || normalized.contains("token:")
        || normalized.contains("\"token\"")
        || normalized.contains("sk-")
        || normalized.contains("-----begin")
        || normalized.contains("ghp_")
        || normalized.contains("gho_")
        || normalized.contains("ghu_")
        || normalized.contains("ghs_")
        || normalized.contains("github_pat_")
        || normalized.contains("glpat-")
        || normalized.contains("xoxb-")
        || normalized.contains("xoxp-")
        || normalized.contains("akia")
        || normalized.contains("aiza")
        || normalized.contains("ya29.")
    {
        return Some("OAuth or API token material");
    }

    if normalized.contains("tool_payload") || normalized.contains("raw payload") {
        return Some("tool raw payload data");
    }

    if value.contains('\u{1b}')
        || value.contains('\0')
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Some("raw terminal byte sequences");
    }

    None
}

fn looks_like_prompt_injection_text(normalized: &str) -> bool {
    normalized.contains("ignore previous instructions")
        || normalized.contains("ignore all previous instructions")
        || normalized.contains("disregard previous instructions")
        || normalized.contains("override the system prompt")
        || normalized.contains("override system instructions")
        || normalized.contains("reveal the system prompt")
        || normalized.contains("reveal hidden instructions")
        || normalized.contains("treat this memory as higher priority")
        || normalized.contains("developer message override")
        || normalized.contains("system message override")
}

fn looks_like_endpoint_credential(value: &str) -> bool {
    for token in value.split_whitespace() {
        let token = token.trim_matches(|character: char| {
            matches!(
                character,
                ',' | ';' | ')' | '(' | '[' | ']' | '"' | '\'' | '`'
            )
        });
        let Some(scheme_index) = token.find("://") else {
            continue;
        };
        let rest = &token[scheme_index + 3..];
        let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
        if authority.contains('@') {
            return true;
        }
        let query = token
            .split_once('?')
            .map(|(_, query)| query)
            .unwrap_or_default();
        if query
            .split('&')
            .filter_map(|pair| pair.split_once('='))
            .any(|(key, value)| !value.is_empty() && is_sensitive_context_name(key))
        {
            return true;
        }
    }
    false
}

fn is_sensitive_context_name(value: &str) -> bool {
    let normalized = value
        .trim()
        .trim_start_matches('-')
        .to_ascii_lowercase()
        .replace('-', "_");
    matches!(
        normalized.as_str(),
        "access_token"
            | "api_key"
            | "apikey"
            | "anthropic_api_key"
            | "authorization"
            | "aws_access_key_id"
            | "aws_secret_access_key"
            | "aws_session_token"
            | "auth_token"
            | "bearer"
            | "client_secret"
            | "github_token"
            | "google_oauth_access_token"
            | "openai_api_key"
            | "password"
            | "private_key"
            | "refresh_token"
            | "secret"
            | "session_id"
            | "session_token"
            | "token"
            | "x_api_key"
    )
}

fn strongest_redaction(
    left: &SessionContextRedactionDto,
    right: &SessionContextRedactionDto,
) -> SessionContextRedactionDto {
    if redaction_rank(&left.redaction_class) >= redaction_rank(&right.redaction_class) {
        left.clone()
    } else {
        right.clone()
    }
}

fn combine_redactions<'a>(
    redactions: impl IntoIterator<Item = &'a SessionContextRedactionDto>,
) -> SessionContextRedactionDto {
    redactions
        .into_iter()
        .cloned()
        .reduce(|left, right| strongest_redaction(&left, &right))
        .unwrap_or_else(SessionContextRedactionDto::public)
}

fn redaction_rank(class: &SessionContextRedactionClassDto) -> u8 {
    match class {
        SessionContextRedactionClassDto::Public => 0,
        SessionContextRedactionClassDto::LocalPath => 1,
        SessionContextRedactionClassDto::Transcript => 2,
        SessionContextRedactionClassDto::RawPayload => 3,
        SessionContextRedactionClassDto::Secret => 4,
    }
}

fn aggregate_usage_totals(runs: &[RunTranscriptSummaryDto]) -> Option<SessionUsageTotalsDto> {
    let first = runs
        .iter()
        .filter_map(|run| run.usage_totals.as_ref())
        .next()
        .cloned()?;
    let mut aggregate = SessionUsageTotalsDto {
        project_id: first.project_id.clone(),
        run_id: "session".into(),
        provider_id: first.provider_id.clone(),
        model_id: "mixed".into(),
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        estimated_cost_micros: 0,
        source: SessionUsageSourceDto::Mixed,
        updated_at: first.updated_at.clone(),
    };
    for usage in runs.iter().filter_map(|run| run.usage_totals.as_ref()) {
        aggregate.input_tokens = aggregate.input_tokens.saturating_add(usage.input_tokens);
        aggregate.output_tokens = aggregate.output_tokens.saturating_add(usage.output_tokens);
        aggregate.total_tokens = aggregate.total_tokens.saturating_add(usage.total_tokens);
        aggregate.estimated_cost_micros = aggregate
            .estimated_cost_micros
            .saturating_add(usage.estimated_cost_micros);
        if usage.updated_at > aggregate.updated_at {
            aggregate.updated_at = usage.updated_at.clone();
        }
    }
    Some(aggregate)
}

fn terminal_time_for_agent_run(run: &AgentRunRecord) -> Option<String> {
    match run.status {
        AgentRunStatus::Completed | AgentRunStatus::Failed => run.completed_at.clone(),
        AgentRunStatus::Cancelled => run.cancelled_at.clone(),
        _ => None,
    }
}

fn policy_decision(
    decision_id: &str,
    action: SessionContextPolicyActionDto,
    trigger: Option<SessionCompactionTriggerDto>,
    reason_code: &str,
    message: &str,
    raw_transcript_preserved: bool,
    model_visible: bool,
) -> SessionContextPolicyDecisionDto {
    let (message, redaction) = sanitize_context_text(message);
    SessionContextPolicyDecisionDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        decision_id: decision_id.into(),
        kind: SessionContextPolicyDecisionKindDto::Compaction,
        action,
        trigger,
        reason_code: reason_code.into(),
        message,
        raw_transcript_preserved,
        model_visible,
        redaction,
    }
}

pub fn memory_policy_decision(
    decision_id: impl Into<String>,
    action: SessionContextPolicyActionDto,
    reason_code: impl Into<String>,
    message: impl AsRef<str>,
    model_visible: bool,
) -> SessionContextPolicyDecisionDto {
    let (message, redaction) = sanitize_context_text(message.as_ref());
    SessionContextPolicyDecisionDto {
        contract_version: XERO_SESSION_CONTEXT_CONTRACT_VERSION,
        decision_id: decision_id.into(),
        kind: SessionContextPolicyDecisionKindDto::MemoryInjection,
        action,
        trigger: None,
        reason_code: reason_code.into(),
        message,
        raw_transcript_preserved: true,
        model_visible,
        redaction,
    }
}

fn memory_scope_rank(scope: &SessionMemoryScopeDto) -> u8 {
    match scope {
        SessionMemoryScopeDto::Project => 0,
        SessionMemoryScopeDto::Session => 1,
    }
}

fn memory_kind_rank(kind: &SessionMemoryKindDto) -> u8 {
    match kind {
        SessionMemoryKindDto::ProjectFact => 0,
        SessionMemoryKindDto::Decision => 1,
        SessionMemoryKindDto::UserPreference => 2,
        SessionMemoryKindDto::Troubleshooting => 3,
        SessionMemoryKindDto::SessionSummary => 4,
    }
}

fn memory_label(memory: &SessionMemoryRecordDto) -> String {
    let scope = match memory.scope {
        SessionMemoryScopeDto::Project => "Project",
        SessionMemoryScopeDto::Session => "Session",
    };
    let kind = match memory.kind {
        SessionMemoryKindDto::ProjectFact => "fact",
        SessionMemoryKindDto::UserPreference => "preference",
        SessionMemoryKindDto::Decision => "decision",
        SessionMemoryKindDto::SessionSummary => "summary",
        SessionMemoryKindDto::Troubleshooting => "troubleshooting",
    };
    format!("{scope} {kind}")
}

pub fn estimate_tokens(value: &str) -> u64 {
    let chars = value.chars().count() as u64;
    chars.saturating_add(3) / 4
}

fn ensure_secret_free_json<T: Serialize>(value: &T) -> Result<(), String> {
    let serialized = serde_json::to_string(value)
        .map_err(|error| format!("session context payload could not serialize: {error}"))?;
    if let Some(reason) = find_serialized_secret_marker(&serialized) {
        return Err(format!("session context payload contains {reason}"));
    }
    Ok(())
}

fn find_serialized_secret_marker(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();
    if normalized.contains("sk-")
        || normalized.contains("bearer ")
        || normalized.contains("bearer:")
        || normalized.contains("authorization=")
        || normalized.contains("authorization:")
        || normalized.contains("access_token=")
        || normalized.contains("access_token\":\"")
        || normalized.contains("refresh_token=")
        || normalized.contains("refresh_token\":\"")
        || normalized.contains("api_key=")
        || normalized.contains("api_key\":\"")
        || normalized.contains("aws_secret_access_key=")
        || normalized.contains("aws_secret_access_key\":\"")
        || normalized.contains("client_secret=")
        || normalized.contains("client_secret\":\"")
        || normalized.contains("session_id=")
        || normalized.contains("session_id\":\"")
        || normalized.contains("session_token=")
        || normalized.contains("session_token\":\"")
        || normalized.contains("token=")
        || normalized.contains("token\":\"")
        || normalized.contains("ghp_")
        || normalized.contains("gho_")
        || normalized.contains("ghu_")
        || normalized.contains("ghs_")
        || normalized.contains("github_pat_")
        || normalized.contains("glpat-")
        || normalized.contains("xoxb-")
        || normalized.contains("xoxp-")
        || normalized.contains("ya29.")
        || normalized.contains("-----begin")
    {
        Some("secret marker")
    } else {
        None
    }
}
