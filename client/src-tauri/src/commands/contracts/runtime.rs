use serde::{Deserialize, Serialize};

use super::agent::AgentAutoCompactPreferenceDto;
use super::autonomous::{
    AutonomousSkillCacheStatusDto, AutonomousSkillLifecycleDiagnosticDto,
    AutonomousSkillLifecycleResultDto, AutonomousSkillLifecycleSourceDto,
    AutonomousSkillLifecycleStageDto, ToolResultSummaryDto,
};

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentIdDto {
    Ask,
    Engineer,
    Debug,
    AgentCreate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentScopeDto {
    BuiltIn,
    GlobalCustom,
    ProjectCustom,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentLifecycleStateDto {
    Draft,
    Active,
    Archived,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentBaseCapabilityProfileDto {
    ObserveOnly,
    Engineering,
    Debugging,
    AgentBuilder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentPromptPolicyDto {
    Ask,
    Engineer,
    Debug,
    AgentCreate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentToolPolicyDto {
    ObserveOnly,
    Engineering,
    AgentBuilder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentOutputContractDto {
    Answer,
    EngineeringSummary,
    DebugSummary,
    AgentDefinitionDraft,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeAgentDescriptorDto {
    pub id: RuntimeAgentIdDto,
    pub version: u32,
    pub label: String,
    pub short_label: String,
    pub description: String,
    pub task_purpose: String,
    pub scope: RuntimeAgentScopeDto,
    pub lifecycle_state: RuntimeAgentLifecycleStateDto,
    pub base_capability_profile: RuntimeAgentBaseCapabilityProfileDto,
    pub default_approval_mode: RuntimeRunApprovalModeDto,
    pub allowed_approval_modes: Vec<RuntimeRunApprovalModeDto>,
    pub prompt_policy: RuntimeAgentPromptPolicyDto,
    pub tool_policy: RuntimeAgentToolPolicyDto,
    pub output_contract: RuntimeAgentOutputContractDto,
    pub allow_plan_gate: bool,
    pub allow_verification_gate: bool,
    pub allow_auto_compact: bool,
}

impl RuntimeAgentIdDto {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::Engineer => "engineer",
            Self::Debug => "debug",
            Self::AgentCreate => "agent_create",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Engineer => "Engineer",
            Self::Debug => "Debug",
            Self::AgentCreate => "Agent Create",
        }
    }

    pub fn allows_plan_gate(&self) -> bool {
        matches!(self, Self::Engineer | Self::Debug)
    }

    pub fn allows_verification_gate(&self) -> bool {
        matches!(self, Self::Engineer | Self::Debug)
    }

    pub fn allows_engineering_tools(&self) -> bool {
        matches!(self, Self::Engineer | Self::Debug)
    }
}

pub fn default_runtime_agent_id() -> RuntimeAgentIdDto {
    RuntimeAgentIdDto::Ask
}

pub fn default_runtime_agent_approval_mode(
    agent_id: &RuntimeAgentIdDto,
) -> RuntimeRunApprovalModeDto {
    match agent_id {
        RuntimeAgentIdDto::Ask => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::Engineer => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::Debug => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::AgentCreate => RuntimeRunApprovalModeDto::Suggest,
    }
}

pub fn runtime_agent_allowed_approval_modes(
    agent_id: &RuntimeAgentIdDto,
) -> Vec<RuntimeRunApprovalModeDto> {
    match agent_id {
        RuntimeAgentIdDto::Ask | RuntimeAgentIdDto::AgentCreate => {
            vec![RuntimeRunApprovalModeDto::Suggest]
        }
        RuntimeAgentIdDto::Engineer | RuntimeAgentIdDto::Debug => vec![
            RuntimeRunApprovalModeDto::Suggest,
            RuntimeRunApprovalModeDto::AutoEdit,
            RuntimeRunApprovalModeDto::Yolo,
        ],
    }
}

pub fn runtime_agent_allows_approval_mode(
    agent_id: &RuntimeAgentIdDto,
    approval_mode: &RuntimeRunApprovalModeDto,
) -> bool {
    match agent_id {
        RuntimeAgentIdDto::Ask | RuntimeAgentIdDto::AgentCreate => {
            matches!(approval_mode, RuntimeRunApprovalModeDto::Suggest)
        }
        RuntimeAgentIdDto::Engineer | RuntimeAgentIdDto::Debug => true,
    }
}

pub fn builtin_runtime_agent_descriptors() -> Vec<RuntimeAgentDescriptorDto> {
    [
        runtime_agent_descriptor(RuntimeAgentIdDto::Ask),
        runtime_agent_descriptor(RuntimeAgentIdDto::Engineer),
        runtime_agent_descriptor(RuntimeAgentIdDto::Debug),
        runtime_agent_descriptor(RuntimeAgentIdDto::AgentCreate),
    ]
    .into_iter()
    .collect()
}

pub fn runtime_agent_descriptor(agent_id: RuntimeAgentIdDto) -> RuntimeAgentDescriptorDto {
    match agent_id {
        RuntimeAgentIdDto::Ask => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 1,
            label: "Ask".into(),
            short_label: "Ask".into(),
            description: "Answer questions about the project without mutating files, app state, processes, or external services.".into(),
            task_purpose: "Answer in chat using audited observe-only tools when grounding is needed.".into(),
            scope: RuntimeAgentScopeDto::BuiltIn,
            lifecycle_state: RuntimeAgentLifecycleStateDto::Active,
            base_capability_profile: RuntimeAgentBaseCapabilityProfileDto::ObserveOnly,
            default_approval_mode: RuntimeRunApprovalModeDto::Suggest,
            allowed_approval_modes: runtime_agent_allowed_approval_modes(&agent_id),
            prompt_policy: RuntimeAgentPromptPolicyDto::Ask,
            tool_policy: RuntimeAgentToolPolicyDto::ObserveOnly,
            output_contract: RuntimeAgentOutputContractDto::Answer,
            allow_plan_gate: false,
            allow_verification_gate: false,
            allow_auto_compact: true,
        },
        RuntimeAgentIdDto::Engineer => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 1,
            label: "Engineer".into(),
            short_label: "Build".into(),
            description: "Implement repository changes with the existing software-building toolset and safety gates.".into(),
            task_purpose: "Inspect, plan when needed, edit, verify, and summarize engineering work.".into(),
            scope: RuntimeAgentScopeDto::BuiltIn,
            lifecycle_state: RuntimeAgentLifecycleStateDto::Active,
            base_capability_profile: RuntimeAgentBaseCapabilityProfileDto::Engineering,
            default_approval_mode: RuntimeRunApprovalModeDto::Suggest,
            allowed_approval_modes: runtime_agent_allowed_approval_modes(&agent_id),
            prompt_policy: RuntimeAgentPromptPolicyDto::Engineer,
            tool_policy: RuntimeAgentToolPolicyDto::Engineering,
            output_contract: RuntimeAgentOutputContractDto::EngineeringSummary,
            allow_plan_gate: true,
            allow_verification_gate: true,
            allow_auto_compact: true,
        },
        RuntimeAgentIdDto::Debug => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 1,
            label: "Debug".into(),
            short_label: "Debug".into(),
            description: "Investigate failures with structured evidence, hypotheses, fixes, verification, and durable debugging memory.".into(),
            task_purpose: "Reproduce, gather evidence, test hypotheses, isolate root cause, fix, verify, and preserve reusable debugging knowledge.".into(),
            scope: RuntimeAgentScopeDto::BuiltIn,
            lifecycle_state: RuntimeAgentLifecycleStateDto::Active,
            base_capability_profile: RuntimeAgentBaseCapabilityProfileDto::Debugging,
            default_approval_mode: RuntimeRunApprovalModeDto::Suggest,
            allowed_approval_modes: runtime_agent_allowed_approval_modes(&agent_id),
            prompt_policy: RuntimeAgentPromptPolicyDto::Debug,
            tool_policy: RuntimeAgentToolPolicyDto::Engineering,
            output_contract: RuntimeAgentOutputContractDto::DebugSummary,
            allow_plan_gate: true,
            allow_verification_gate: true,
            allow_auto_compact: true,
        },
        RuntimeAgentIdDto::AgentCreate => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 1,
            label: "Agent Create".into(),
            short_label: "Create".into(),
            description: "Interview the user, validate custom agent definitions, and save approved definitions without mutating repositories.".into(),
            task_purpose: "Gather intent, clarify scope, propose least-privilege capabilities, validate definitions, and persist approved custom agents.".into(),
            scope: RuntimeAgentScopeDto::BuiltIn,
            lifecycle_state: RuntimeAgentLifecycleStateDto::Active,
            base_capability_profile: RuntimeAgentBaseCapabilityProfileDto::AgentBuilder,
            default_approval_mode: RuntimeRunApprovalModeDto::Suggest,
            allowed_approval_modes: runtime_agent_allowed_approval_modes(&agent_id),
            prompt_policy: RuntimeAgentPromptPolicyDto::AgentCreate,
            tool_policy: RuntimeAgentToolPolicyDto::AgentBuilder,
            output_contract: RuntimeAgentOutputContractDto::AgentDefinitionDraft,
            allow_plan_gate: false,
            allow_verification_gate: false,
            allow_auto_compact: true,
        },
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeRunControlInputDto {
    pub runtime_agent_id: RuntimeAgentIdDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_id: Option<String>,
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
    pub runtime_agent_id: RuntimeAgentIdDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_version: Option<u32>,
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
    pub runtime_agent_id: RuntimeAgentIdDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_definition_version: Option<u32>,
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
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderAuthSessionDto {
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
pub struct AutoNameAgentSessionRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub controls: Option<RuntimeRunControlInputDto>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCredentialKindDto {
    ApiKey,
    #[serde(rename = "oauth_session", alias = "o_auth_session")]
    OAuthSession,
    Local,
    Ambient,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCredentialReadinessProofDto {
    #[serde(rename = "oauth_session", alias = "o_auth_session")]
    OAuthSession,
    StoredSecret,
    Local,
    Ambient,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderCredentialDto {
    pub provider_id: String,
    pub kind: ProviderCredentialKindDto,
    /// Bool projection — the frontend never sees the secret value.
    pub has_api_key: bool,
    pub oauth_account_id: Option<String>,
    pub oauth_session_id: Option<String>,
    /// Whether an OAuth access token is currently stored (used by sign-in
    /// state UI without exposing the token itself).
    pub has_oauth_access_token: bool,
    pub oauth_expires_at: Option<i64>,
    pub base_url: Option<String>,
    pub api_version: Option<String>,
    pub region: Option<String>,
    pub project_id: Option<String>,
    pub default_model_id: Option<String>,
    pub readiness_proof: ProviderCredentialReadinessProofDto,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderCredentialsSnapshotDto {
    pub credentials: Vec<ProviderCredentialDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertProviderCredentialRequestDto {
    pub provider_id: String,
    pub kind: ProviderCredentialKindDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteProviderCredentialRequestDto {
    pub provider_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartOAuthLoginRequestDto {
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CompleteOAuthCallbackRequestDto {
    pub provider_id: String,
    pub flow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manual_input: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub originator: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubmitOpenAiCallbackRequestDto {
    pub flow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    pub validation_checks: Vec<crate::runtime::XeroDiagnosticCheck>,
    pub reachability_checks: Vec<crate::runtime::XeroDiagnosticCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_catalog: Option<ProviderModelCatalogDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunDoctorReportRequestDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<crate::runtime::XeroDoctorReportMode>,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initial_attachments: Vec<StagedAgentAttachmentDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAttachmentKindDto {
    Image,
    Document,
    Text,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StagedAgentAttachmentDto {
    pub kind: AgentAttachmentKindDto,
    pub absolute_path: String,
    pub media_type: String,
    pub original_name: String,
    pub size_bytes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StageAgentAttachmentRequestDto {
    pub project_id: String,
    pub run_id: String,
    pub original_name: String,
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DiscardAgentAttachmentRequestDto {
    pub project_id: String,
    pub absolute_path: String,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<StagedAgentAttachmentDto>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_runtime_agents_seed_agent_create_descriptor() {
        let descriptors = builtin_runtime_agent_descriptors();

        assert_eq!(
            descriptors
                .iter()
                .map(|descriptor| descriptor.id.as_str())
                .collect::<Vec<_>>(),
            vec!["ask", "engineer", "debug", "agent_create"]
        );

        let agent_create = descriptors
            .iter()
            .find(|descriptor| descriptor.id == RuntimeAgentIdDto::AgentCreate)
            .expect("Agent Create descriptor should be seeded");

        assert_eq!(agent_create.label, "Agent Create");
        assert_eq!(
            agent_create.base_capability_profile,
            RuntimeAgentBaseCapabilityProfileDto::AgentBuilder
        );
        assert_eq!(
            agent_create.allowed_approval_modes,
            vec![RuntimeRunApprovalModeDto::Suggest]
        );
        assert!(!agent_create.allow_plan_gate);
        assert!(!agent_create.allow_verification_gate);
        assert!(!RuntimeAgentIdDto::AgentCreate.allows_engineering_tools());
        assert!(runtime_agent_allows_approval_mode(
            &RuntimeAgentIdDto::AgentCreate,
            &RuntimeRunApprovalModeDto::Suggest
        ));
        assert!(!runtime_agent_allows_approval_mode(
            &RuntimeAgentIdDto::AgentCreate,
            &RuntimeRunApprovalModeDto::AutoEdit
        ));
    }

    #[test]
    fn engineering_agents_allow_all_approval_modes() {
        assert_eq!(
            runtime_agent_allowed_approval_modes(&RuntimeAgentIdDto::Engineer),
            vec![
                RuntimeRunApprovalModeDto::Suggest,
                RuntimeRunApprovalModeDto::AutoEdit,
                RuntimeRunApprovalModeDto::Yolo
            ]
        );
        assert_eq!(
            runtime_agent_allowed_approval_modes(&RuntimeAgentIdDto::Debug),
            vec![
                RuntimeRunApprovalModeDto::Suggest,
                RuntimeRunApprovalModeDto::AutoEdit,
                RuntimeRunApprovalModeDto::Yolo
            ]
        );
    }
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
#[serde(rename_all = "snake_case")]
pub enum RuntimeStreamTranscriptRole {
    User,
    Assistant,
    System,
    Tool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_role: Option<RuntimeStreamTranscriptRole>,
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
