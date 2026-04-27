use serde::{Deserialize, Serialize};

use super::agent::AgentAutoCompactPreferenceDto;
use super::autonomous::{
    AutonomousSkillCacheStatusDto, AutonomousSkillLifecycleDiagnosticDto,
    AutonomousSkillLifecycleResultDto, AutonomousSkillLifecycleSourceDto,
    AutonomousSkillLifecycleStageDto, ToolResultSummaryDto,
};

fn is_false(value: &bool) -> bool {
    !*value
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAuthPhase {
    Idle,
    Starting,
    AwaitingBrowserCallback,
    AwaitingManualInput,
    ExchangingCode,
    Authenticated,
    Refreshing,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRunStatusDto {
    Starting,
    Running,
    Stale,
    Stopped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRunTransportLivenessDto {
    Unknown,
    Reachable,
    Unreachable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRunCheckpointKindDto {
    Bootstrap,
    State,
    Tool,
    ActionRequired,
    Diagnostic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunDiagnosticDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunTransportDto {
    pub kind: String,
    pub endpoint: String,
    pub liveness: RuntimeRunTransportLivenessDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunCheckpointDto {
    pub sequence: u32,
    pub kind: RuntimeRunCheckpointKindDto,
    pub summary: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeRunApprovalModeDto {
    Suggest,
    AutoEdit,
    Yolo,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunControlInputDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
    pub approval_mode: RuntimeRunApprovalModeDto,
    #[serde(default)]
    pub plan_mode_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunActiveControlSnapshotDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
    pub approval_mode: RuntimeRunApprovalModeDto,
    #[serde(default)]
    pub plan_mode_required: bool,
    pub revision: u32,
    pub applied_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunPendingControlSnapshotDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
    pub approval_mode: RuntimeRunApprovalModeDto,
    #[serde(default)]
    pub plan_mode_required: bool,
    pub revision: u32,
    pub queued_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queued_prompt_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunControlStateDto {
    pub active: RuntimeRunActiveControlSnapshotDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending: Option<RuntimeRunPendingControlSnapshotDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub supervisor_kind: String,
    pub status: RuntimeRunStatusDto,
    pub transport: RuntimeRunTransportDto,
    pub controls: RuntimeRunControlStateDto,
    pub started_at: String,
    pub last_heartbeat_at: Option<String>,
    pub last_checkpoint_sequence: u32,
    pub last_checkpoint_at: Option<String>,
    pub stopped_at: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeRunDiagnosticDto>,
    pub updated_at: String,
    pub checkpoints: Vec<RuntimeRunCheckpointDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSessionDto {
    pub project_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub flow_id: Option<String>,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub phase: RuntimeAuthPhase,
    pub callback_bound: Option<bool>,
    pub authorization_url: Option<String>,
    pub redirect_uri: Option<String>,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeDiagnosticDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionStatusDto {
    Active,
    Archived,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSessionLineageBoundaryKindDto {
    Run,
    Message,
    Checkpoint,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSessionLineageDiagnosticDto {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSessionLineageDto {
    pub lineage_id: String,
    pub project_id: String,
    pub child_agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_agent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_run_id: Option<String>,
    pub source_boundary_kind: AgentSessionLineageBoundaryKindDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_message_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_checkpoint_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_compaction_id: Option<String>,
    pub source_title: String,
    pub branch_title: String,
    pub replay_run_id: String,
    pub file_change_summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<AgentSessionLineageDiagnosticDto>,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentSessionDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub title: String,
    pub summary: String,
    pub status: AgentSessionStatusDto,
    pub selected: bool,
    pub created_at: String,
    pub updated_at: String,
    pub archived_at: Option<String>,
    pub last_run_id: Option<String>,
    pub last_runtime_kind: Option<String>,
    pub last_provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<AgentSessionLineageDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateAgentSessionRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAgentSessionsRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentSessionRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateAgentSessionRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArchiveAgentSessionRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreAgentSessionRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteAgentSessionRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListAgentSessionsResponseDto {
    pub sessions: Vec<AgentSessionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSettingsDto {
    pub provider_id: String,
    pub model_id: String,
    pub openrouter_api_key_configured: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub anthropic_api_key_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProfileReadinessStatusDto {
    Ready,
    Missing,
    Malformed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderProfileReadinessProofDto {
    #[serde(rename = "oauth_session", alias = "o_auth_session")]
    OAuthSession,
    StoredSecret,
    Local,
    Ambient,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileReadinessDto {
    pub ready: bool,
    pub status: ProviderProfileReadinessStatusDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proof: Option<ProviderProfileReadinessProofDto>,
    pub proof_updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileDto {
    pub profile_id: String,
    pub provider_id: String,
    pub runtime_kind: String,
    pub label: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub active: bool,
    pub readiness: ProviderProfileReadinessDto,
    pub migrated_from_legacy: bool,
    pub migrated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfilesMigrationDto {
    pub source: String,
    pub migrated_at: String,
    pub runtime_settings_updated_at: Option<String>,
    pub openrouter_credentials_updated_at: Option<String>,
    pub openai_auth_updated_at: Option<String>,
    pub openrouter_model_inferred: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfilesDto {
    pub active_profile_id: String,
    pub profiles: Vec<ProviderProfileDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub migration: Option<ProviderProfilesMigrationDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderModelCatalogSourceDto {
    Live,
    Cache,
    Manual,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderModelThinkingEffortDto {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelCatalogDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelThinkingCapabilityDto {
    pub supported: bool,
    #[serde(default)]
    pub effort_options: Vec<ProviderModelThinkingEffortDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<ProviderModelThinkingEffortDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelDto {
    pub model_id: String,
    pub display_name: String,
    pub thinking: ProviderModelThinkingCapabilityDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelCatalogDto {
    pub profile_id: String,
    pub provider_id: String,
    pub configured_model_id: String,
    pub source: ProviderModelCatalogSourceDto,
    pub fetched_at: Option<String>,
    pub last_success_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh_error: Option<ProviderModelCatalogDiagnosticDto>,
    pub models: Vec<ProviderModelDto>,
}

pub type RuntimeAuthStatusDto = RuntimeSessionDto;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeUpdatedPayloadDto {
    pub project_id: String,
    pub runtime_kind: String,
    pub provider_id: String,
    pub flow_id: Option<String>,
    pub session_id: Option<String>,
    pub account_id: Option<String>,
    pub auth_phase: RuntimeAuthPhase,
    pub last_error_code: Option<String>,
    pub last_error: Option<RuntimeDiagnosticDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunUpdatedPayloadDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub run: Option<RuntimeRunDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartOpenAiLoginRequestDto {
    pub project_id: String,
    pub profile_id: String,
    pub originator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitOpenAiCallbackRequestDto {
    pub project_id: String,
    pub profile_id: String,
    pub flow_id: String,
    pub manual_input: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetRuntimeRunRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertRuntimeSettingsRequestDto {
    pub provider_id: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub openrouter_api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anthropic_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertProviderProfileRequestDto {
    pub profile_id: String,
    pub provider_id: String,
    pub runtime_kind: String,
    pub label: String,
    pub model_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default)]
    pub activate: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetActiveProviderProfileRequestDto {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LogoutProviderProfileRequestDto {
    pub profile_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetProviderModelCatalogRequestDto {
    pub profile_id: String,
    #[serde(default)]
    pub force_refresh: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckProviderProfileRequestDto {
    pub profile_id: String,
    #[serde(default)]
    pub include_network: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileDiagnosticsDto {
    pub checked_at: String,
    pub profile_id: String,
    pub provider_id: String,
    pub validation_checks: Vec<crate::runtime::CadenceDiagnosticCheck>,
    pub reachability_checks: Vec<crate::runtime::CadenceDiagnosticCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_catalog: Option<ProviderModelCatalogDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunDoctorReportRequestDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<crate::runtime::CadenceDoctorReportMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRuntimeRunRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_controls: Option<RuntimeRunControlInputDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartRuntimeSessionRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_profile_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateRuntimeRunControlsRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<RuntimeRunControlInputDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_compact: Option<AgentAutoCompactPreferenceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StopRuntimeRunRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStreamItemKind {
    Transcript,
    Tool,
    Skill,
    Activity,
    ActionRequired,
    Complete,
    Failure,
}

impl RuntimeStreamItemKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::Tool => "tool",
            Self::Skill => "skill",
            Self::Activity => "activity",
            Self::ActionRequired => "action_required",
            Self::Complete => "complete",
            Self::Failure => "failure",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeToolCallState {
    Pending,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeStreamItemDto {
    pub kind: RuntimeStreamItemKind,
    pub run_id: String,
    pub sequence: u64,
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub text: Option<String>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_state: Option<RuntimeToolCallState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummaryDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_stage: Option<AutonomousSkillLifecycleStageDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_result: Option<AutonomousSkillLifecycleResultDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_source: Option<AutonomousSkillLifecycleSourceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_cache_status: Option<AutonomousSkillCacheStatusDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_diagnostic: Option<AutonomousSkillLifecycleDiagnosticDto>,
    pub action_id: Option<String>,
    pub boundary_id: Option<String>,
    pub action_type: Option<String>,
    pub title: Option<String>,
    pub detail: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub retryable: Option<bool>,
    pub created_at: String,
}

impl RuntimeStreamItemDto {
    pub const ALLOWED_KIND_NAMES: [&'static str; 7] = [
        "transcript",
        "tool",
        "skill",
        "activity",
        "action_required",
        "complete",
        "failure",
    ];

    pub fn allowed_kind_names() -> &'static [&'static str] {
        &Self::ALLOWED_KIND_NAMES
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscribeRuntimeStreamRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub channel: Option<String>,
    pub item_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscribeRuntimeStreamResponseDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub runtime_kind: String,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub subscribed_item_kinds: Vec<RuntimeStreamItemKind>,
}
