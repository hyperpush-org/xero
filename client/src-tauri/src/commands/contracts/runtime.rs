use serde::{Deserialize, Serialize};
use xero_agent_core::{
    ProviderCapabilityCatalog, ProviderPreflightRequiredFeatures, ProviderPreflightSnapshot,
};

use super::agent::AgentAutoCompactPreferenceDto;
use super::autonomous::{
    AutonomousSkillCacheStatusDto, AutonomousSkillLifecycleDiagnosticDto,
    AutonomousSkillLifecycleResultDto, AutonomousSkillLifecycleSourceDto,
    AutonomousSkillLifecycleStageDto, ToolResultSummaryDto,
};
use super::code_history::CodePatchAvailabilityDto;
use super::error::CommandError;

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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolApplicationStyleDto {
    Conservative,
    #[default]
    Balanced,
    DeclarativeFirst,
}

impl AgentToolApplicationStyleDto {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Conservative => "conservative",
            Self::Balanced => "balanced",
            Self::DeclarativeFirst => "declarative_first",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolApplicationStyleResolutionSourceDto {
    GlobalDefault,
    ModelOverride,
}

impl AgentToolApplicationStyleResolutionSourceDto {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GlobalDefault => "global_default",
            Self::ModelOverride => "model_override",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolvedAgentToolApplicationStyleDto {
    pub provider_id: String,
    pub model_id: String,
    pub style: AgentToolApplicationStyleDto,
    pub source: AgentToolApplicationStyleResolutionSourceDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_updated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_updated_at: Option<String>,
}

impl Default for ResolvedAgentToolApplicationStyleDto {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            model_id: String::new(),
            style: AgentToolApplicationStyleDto::Balanced,
            source: AgentToolApplicationStyleResolutionSourceDto::GlobalDefault,
            global_updated_at: None,
            override_updated_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentIdDto {
    Ask,
    Plan,
    Engineer,
    Debug,
    Crawl,
    AgentCreate,
    Generalist,
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
    Valid,
    Active,
    Archived,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentBaseCapabilityProfileDto {
    ObserveOnly,
    Planning,
    RepositoryRecon,
    Engineering,
    Debugging,
    AgentBuilder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentPromptPolicyDto {
    Ask,
    Plan,
    Engineer,
    Debug,
    Crawl,
    AgentCreate,
    Generalist,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentToolPolicyDto {
    ObserveOnly,
    Planning,
    RepositoryRecon,
    Engineering,
    AgentBuilder,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAgentOutputContractDto {
    Answer,
    PlanPack,
    CrawlReport,
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
            Self::Plan => "plan",
            Self::Engineer => "engineer",
            Self::Debug => "debug",
            Self::Crawl => "crawl",
            Self::AgentCreate => "agent_create",
            Self::Generalist => "generalist",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::Plan => "Plan",
            Self::Engineer => "Engineer",
            Self::Debug => "Debug",
            Self::Crawl => "Crawl",
            Self::AgentCreate => "Agent Create",
            Self::Generalist => "Generalist",
        }
    }

    pub fn allows_plan_gate(&self) -> bool {
        matches!(self, Self::Engineer | Self::Debug | Self::Generalist)
    }

    pub fn allows_verification_gate(&self) -> bool {
        matches!(self, Self::Engineer | Self::Debug | Self::Generalist)
    }

    pub fn allows_engineering_tools(&self) -> bool {
        matches!(self, Self::Engineer | Self::Debug | Self::Generalist)
    }
}

pub fn default_runtime_agent_id() -> RuntimeAgentIdDto {
    RuntimeAgentIdDto::Ask
}

pub fn runtime_agent_is_available_for_context(
    agent_id: RuntimeAgentIdDto,
    _debug_build: bool,
    _test_binary: bool,
    _ci_env: Option<&str>,
    _manual_flag: Option<&str>,
) -> bool {
    let _ = agent_id;
    true
}

pub fn runtime_agent_is_available(agent_id: RuntimeAgentIdDto) -> bool {
    let _ = agent_id;
    true
}

pub fn ensure_runtime_agent_available(agent_id: RuntimeAgentIdDto) -> Result<(), CommandError> {
    if runtime_agent_is_available(agent_id) {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "runtime_agent_unavailable",
        format!(
            "Xero cannot start the {} agent because it is not available.",
            agent_id.label()
        ),
    ))
}

pub fn default_runtime_agent_approval_mode(
    agent_id: &RuntimeAgentIdDto,
) -> RuntimeRunApprovalModeDto {
    match agent_id {
        RuntimeAgentIdDto::Ask => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::Plan => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::Engineer => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::Debug => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::Crawl => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::AgentCreate => RuntimeRunApprovalModeDto::Suggest,
        RuntimeAgentIdDto::Generalist => RuntimeRunApprovalModeDto::Suggest,
    }
}

pub fn runtime_agent_allowed_approval_modes(
    agent_id: &RuntimeAgentIdDto,
) -> Vec<RuntimeRunApprovalModeDto> {
    match agent_id {
        RuntimeAgentIdDto::Ask
        | RuntimeAgentIdDto::Plan
        | RuntimeAgentIdDto::Crawl
        | RuntimeAgentIdDto::AgentCreate => {
            vec![RuntimeRunApprovalModeDto::Suggest]
        }
        RuntimeAgentIdDto::Engineer | RuntimeAgentIdDto::Debug | RuntimeAgentIdDto::Generalist => {
            vec![
                RuntimeRunApprovalModeDto::Suggest,
                RuntimeRunApprovalModeDto::AutoEdit,
                RuntimeRunApprovalModeDto::Yolo,
            ]
        }
    }
}

pub fn runtime_agent_allows_approval_mode(
    agent_id: &RuntimeAgentIdDto,
    approval_mode: &RuntimeRunApprovalModeDto,
) -> bool {
    match agent_id {
        RuntimeAgentIdDto::Ask
        | RuntimeAgentIdDto::Plan
        | RuntimeAgentIdDto::Crawl
        | RuntimeAgentIdDto::AgentCreate => {
            matches!(approval_mode, RuntimeRunApprovalModeDto::Suggest)
        }
        RuntimeAgentIdDto::Engineer | RuntimeAgentIdDto::Debug | RuntimeAgentIdDto::Generalist => {
            true
        }
    }
}

pub fn builtin_runtime_agent_descriptors() -> Vec<RuntimeAgentDescriptorDto> {
    [
        runtime_agent_descriptor(RuntimeAgentIdDto::Generalist),
        runtime_agent_descriptor(RuntimeAgentIdDto::Ask),
        runtime_agent_descriptor(RuntimeAgentIdDto::Plan),
        runtime_agent_descriptor(RuntimeAgentIdDto::Engineer),
        runtime_agent_descriptor(RuntimeAgentIdDto::Debug),
        runtime_agent_descriptor(RuntimeAgentIdDto::Crawl),
        runtime_agent_descriptor(RuntimeAgentIdDto::AgentCreate),
    ]
    .into_iter()
    .collect()
}

pub fn available_builtin_runtime_agent_descriptors() -> Vec<RuntimeAgentDescriptorDto> {
    builtin_runtime_agent_descriptors()
        .into_iter()
        .filter(|descriptor| runtime_agent_is_available(descriptor.id))
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
        RuntimeAgentIdDto::Plan => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 2,
            label: "Plan".into(),
            short_label: "Plan".into(),
            description: "Turn ambiguous work into an accepted, durable implementation plan without mutating repository files.".into(),
            task_purpose: "Interview the user, inspect project context when useful, draft a reproducible Plan Pack, and prepare Engineer handoff.".into(),
            scope: RuntimeAgentScopeDto::BuiltIn,
            lifecycle_state: RuntimeAgentLifecycleStateDto::Active,
            base_capability_profile: RuntimeAgentBaseCapabilityProfileDto::Planning,
            default_approval_mode: RuntimeRunApprovalModeDto::Suggest,
            allowed_approval_modes: runtime_agent_allowed_approval_modes(&agent_id),
            prompt_policy: RuntimeAgentPromptPolicyDto::Plan,
            tool_policy: RuntimeAgentToolPolicyDto::Planning,
            output_contract: RuntimeAgentOutputContractDto::PlanPack,
            allow_plan_gate: false,
            allow_verification_gate: false,
            allow_auto_compact: true,
        },
        RuntimeAgentIdDto::Engineer => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 2,
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
            version: 2,
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
        RuntimeAgentIdDto::Crawl => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 1,
            label: "Crawl".into(),
            short_label: "Crawl".into(),
            description: "Map an existing repository, identify stack, tests, commands, architecture, hot spots, and durable project facts without editing files.".into(),
            task_purpose: "Read brownfield repository context and produce a structured crawl report for durable project memory.".into(),
            scope: RuntimeAgentScopeDto::BuiltIn,
            lifecycle_state: RuntimeAgentLifecycleStateDto::Active,
            base_capability_profile: RuntimeAgentBaseCapabilityProfileDto::RepositoryRecon,
            default_approval_mode: RuntimeRunApprovalModeDto::Suggest,
            allowed_approval_modes: runtime_agent_allowed_approval_modes(&agent_id),
            prompt_policy: RuntimeAgentPromptPolicyDto::Crawl,
            tool_policy: RuntimeAgentToolPolicyDto::RepositoryRecon,
            output_contract: RuntimeAgentOutputContractDto::CrawlReport,
            allow_plan_gate: false,
            allow_verification_gate: false,
            allow_auto_compact: true,
        },
        RuntimeAgentIdDto::AgentCreate => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 2,
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
        RuntimeAgentIdDto::Generalist => RuntimeAgentDescriptorDto {
            id: agent_id,
            version: 1,
            label: "Generalist".into(),
            short_label: "Generalist".into(),
            description: "A do-anything agent with the full engineering toolset that recognises when a specialist agent would handle the task better and offers to route.".into(),
            task_purpose: "Handle any user request directly, or suggest routing to Plan, Engineer, or Debug when the request fits a specialist's scope.".into(),
            scope: RuntimeAgentScopeDto::BuiltIn,
            lifecycle_state: RuntimeAgentLifecycleStateDto::Active,
            base_capability_profile: RuntimeAgentBaseCapabilityProfileDto::Engineering,
            default_approval_mode: RuntimeRunApprovalModeDto::Suggest,
            allowed_approval_modes: runtime_agent_allowed_approval_modes(&agent_id),
            prompt_policy: RuntimeAgentPromptPolicyDto::Generalist,
            tool_policy: RuntimeAgentToolPolicyDto::Engineering,
            output_contract: RuntimeAgentOutputContractDto::Answer,
            allow_plan_gate: true,
            allow_verification_gate: true,
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
pub struct ProviderModelCatalogContractDiagnosticDto {
    pub code: String,
    pub message: String,
    pub severity: String,
    pub path: Vec<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_window_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit_source: Option<super::session_context::SessionContextLimitSourceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit_confidence: Option<super::session_context::SessionContextLimitConfidenceDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit_fetched_at: Option<String>,
    pub capabilities: ProviderCapabilityCatalog,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelCatalogDto {
    pub contract_version: u32,
    pub profile_id: String,
    pub provider_id: String,
    pub configured_model_id: String,
    pub source: ProviderModelCatalogSourceDto,
    pub capabilities: ProviderCapabilityCatalog,
    pub fetched_at: Option<String>,
    pub last_success_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_age_seconds: Option<i64>,
    pub cache_ttl_seconds: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh_error: Option<ProviderModelCatalogDiagnosticDto>,
    pub models: Vec<ProviderModelDto>,
    pub contract_diagnostics: Vec<ProviderModelCatalogContractDiagnosticDto>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreflightProviderProfileRequestDto {
    pub profile_id: String,
    #[serde(default)]
    pub force_refresh: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default = "default_provider_preflight_required_features")]
    pub required_features: ProviderPreflightRequiredFeatures,
}

pub type ProviderPreflightSnapshotDto = ProviderPreflightSnapshot;

fn default_provider_preflight_required_features() -> ProviderPreflightRequiredFeatures {
    ProviderPreflightRequiredFeatures::owned_agent_text_turn()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderProfileDiagnosticsDto {
    pub checked_at: String,
    pub profile_id: String,
    pub provider_id: String,
    pub validation_checks: Vec<crate::runtime::XeroDiagnosticCheck>,
    pub reachability_checks: Vec<crate::runtime::XeroDiagnosticCheck>,
    #[serde(default)]
    pub capability_checks: Vec<crate::runtime::XeroDiagnosticCheck>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_catalog: Option<ProviderModelCatalogDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preflight: Option<ProviderPreflightSnapshotDto>,
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
            vec![
                "generalist",
                "ask",
                "plan",
                "engineer",
                "debug",
                "crawl",
                "agent_create"
            ]
        );

        let plan = descriptors
            .iter()
            .find(|descriptor| descriptor.id == RuntimeAgentIdDto::Plan)
            .expect("Plan descriptor should be seeded");

        assert_eq!(plan.label, "Plan");
        assert_eq!(
            plan.base_capability_profile,
            RuntimeAgentBaseCapabilityProfileDto::Planning
        );
        assert_eq!(plan.prompt_policy, RuntimeAgentPromptPolicyDto::Plan);
        assert_eq!(plan.tool_policy, RuntimeAgentToolPolicyDto::Planning);
        assert_eq!(
            plan.output_contract,
            RuntimeAgentOutputContractDto::PlanPack
        );
        assert_eq!(
            plan.allowed_approval_modes,
            vec![RuntimeRunApprovalModeDto::Suggest]
        );
        assert!(!plan.allow_plan_gate);
        assert!(!plan.allow_verification_gate);
        assert!(!RuntimeAgentIdDto::Plan.allows_engineering_tools());
        assert!(runtime_agent_allows_approval_mode(
            &RuntimeAgentIdDto::Plan,
            &RuntimeRunApprovalModeDto::Suggest
        ));
        assert!(!runtime_agent_allows_approval_mode(
            &RuntimeAgentIdDto::Plan,
            &RuntimeRunApprovalModeDto::AutoEdit
        ));

        let crawl = descriptors
            .iter()
            .find(|descriptor| descriptor.id == RuntimeAgentIdDto::Crawl)
            .expect("Crawl descriptor should be seeded");

        assert_eq!(crawl.label, "Crawl");
        assert_eq!(
            crawl.base_capability_profile,
            RuntimeAgentBaseCapabilityProfileDto::RepositoryRecon
        );
        assert_eq!(crawl.prompt_policy, RuntimeAgentPromptPolicyDto::Crawl);
        assert_eq!(
            crawl.tool_policy,
            RuntimeAgentToolPolicyDto::RepositoryRecon
        );
        assert_eq!(
            crawl.output_contract,
            RuntimeAgentOutputContractDto::CrawlReport
        );
        assert_eq!(
            crawl.allowed_approval_modes,
            vec![RuntimeRunApprovalModeDto::Suggest]
        );
        assert!(!crawl.allow_plan_gate);
        assert!(!crawl.allow_verification_gate);
        assert!(!RuntimeAgentIdDto::Crawl.allows_engineering_tools());
        assert!(runtime_agent_allows_approval_mode(
            &RuntimeAgentIdDto::Crawl,
            &RuntimeRunApprovalModeDto::Suggest
        ));
        assert!(!runtime_agent_allows_approval_mode(
            &RuntimeAgentIdDto::Crawl,
            &RuntimeRunApprovalModeDto::AutoEdit
        ));

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
    fn runtime_agent_id_dto_serializes_and_deserializes() {
        assert_eq!(
            serde_json::to_string(&RuntimeAgentIdDto::Plan).expect("serialize Plan agent id"),
            r#""plan""#
        );
        assert_eq!(
            serde_json::from_str::<RuntimeAgentIdDto>(r#""plan""#)
                .expect("deserialize Plan agent id"),
            RuntimeAgentIdDto::Plan
        );

        let input = RuntimeRunControlInputDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            provider_profile_id: None,
            model_id: "test-model".into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Suggest,
            plan_mode_required: false,
        };
        let value = serde_json::to_value(&input).expect("serialize run controls");

        assert_eq!(value["runtimeAgentId"], "engineer");
        assert_eq!(
            serde_json::from_value::<RuntimeRunControlInputDto>(value)
                .expect("deserialize run controls")
                .runtime_agent_id,
            RuntimeAgentIdDto::Engineer
        );
    }

    #[test]
    fn runtime_agent_availability_returns_builtin_agents() {
        assert!(runtime_agent_is_available_for_context(
            RuntimeAgentIdDto::Plan,
            false,
            false,
            None,
            None
        ));
        assert!(runtime_agent_is_available_for_context(
            RuntimeAgentIdDto::Ask,
            false,
            false,
            None,
            None
        ));

        assert_eq!(
            available_builtin_runtime_agent_descriptors()
                .iter()
                .map(|descriptor| descriptor.id)
                .collect::<Vec<_>>(),
            vec![
                RuntimeAgentIdDto::Generalist,
                RuntimeAgentIdDto::Ask,
                RuntimeAgentIdDto::Plan,
                RuntimeAgentIdDto::Engineer,
                RuntimeAgentIdDto::Debug,
                RuntimeAgentIdDto::Crawl,
                RuntimeAgentIdDto::AgentCreate
            ]
        );
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
        assert_eq!(
            runtime_agent_allowed_approval_modes(&RuntimeAgentIdDto::Generalist),
            vec![
                RuntimeRunApprovalModeDto::Suggest,
                RuntimeRunApprovalModeDto::AutoEdit,
                RuntimeRunApprovalModeDto::Yolo
            ]
        );
    }

    #[test]
    fn builtin_runtime_agents_seed_generalist_descriptor() {
        let descriptors = builtin_runtime_agent_descriptors();
        let generalist = descriptors
            .iter()
            .find(|descriptor| descriptor.id == RuntimeAgentIdDto::Generalist)
            .expect("Generalist descriptor should be seeded");

        assert_eq!(generalist.label, "Generalist");
        assert_eq!(
            generalist.base_capability_profile,
            RuntimeAgentBaseCapabilityProfileDto::Engineering
        );
        assert_eq!(
            generalist.prompt_policy,
            RuntimeAgentPromptPolicyDto::Generalist
        );
        assert_eq!(
            generalist.tool_policy,
            RuntimeAgentToolPolicyDto::Engineering
        );
        assert_eq!(
            generalist.output_contract,
            RuntimeAgentOutputContractDto::Answer
        );
        assert!(generalist.allow_plan_gate);
        assert!(generalist.allow_verification_gate);
        assert!(RuntimeAgentIdDto::Generalist.allows_engineering_tools());
        assert!(runtime_agent_allows_approval_mode(
            &RuntimeAgentIdDto::Generalist,
            &RuntimeRunApprovalModeDto::Yolo
        ));
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
    Plan,
    Complete,
    Failure,
    SubagentLifecycle,
}

impl RuntimeStreamItemKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::Tool => "tool",
            Self::Skill => "skill",
            Self::Activity => "activity",
            Self::ActionRequired => "action_required",
            Self::Plan => "plan",
            Self::Complete => "complete",
            Self::Failure => "failure",
            Self::SubagentLifecycle => "subagent_lifecycle",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStreamPlanItemStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeStreamPlanItemDto {
    pub id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub status: RuntimeStreamPlanItemStatus,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActionAnswerShape {
    PlainText,
    TerminalInput,
    SingleChoice,
    MultiChoice,
    ShortText,
    LongText,
    Number,
    Date,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeActionRequiredOptionDto {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_sequence: Option<u64>,
    pub session_id: Option<String>,
    pub flow_id: Option<String>,
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_role: Option<RuntimeStreamTranscriptRole>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_state: Option<RuntimeToolCallState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_change_group_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_commit_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_workspace_epoch: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code_patch_availability: Option<CodePatchAvailabilityDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_summary: Option<ToolResultSummaryDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_result_preview: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer_shape: Option<RuntimeActionAnswerShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<Vec<RuntimeActionRequiredOptionDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_multiple: Option<bool>,
    pub title: Option<String>,
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_items: Option<Vec<RuntimeStreamPlanItemDto>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_last_changed_id: Option<String>,
    pub code: Option<String>,
    pub message: Option<String>,
    pub retryable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_role_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_used_tool_calls: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_max_tool_calls: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_used_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_used_cost_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_max_cost_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_result_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subagent_prompt: Option<String>,
    pub created_at: String,
}

impl RuntimeStreamItemDto {
    pub const ALLOWED_KIND_NAMES: [&'static str; 9] = [
        "transcript",
        "tool",
        "skill",
        "activity",
        "action_required",
        "plan",
        "complete",
        "failure",
        "subagent_lifecycle",
    ];

    pub fn allowed_kind_names() -> &'static [&'static str] {
        &Self::ALLOWED_KIND_NAMES
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeStreamViewStatusDto {
    Idle,
    Subscribing,
    Replaying,
    Live,
    Complete,
    Stale,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeStreamIssueDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeStreamViewSnapshotDto {
    pub schema: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub runtime_kind: String,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub subscribed_item_kinds: Vec<RuntimeStreamItemKind>,
    pub status: RuntimeStreamViewStatusDto,
    pub items: Vec<RuntimeStreamItemDto>,
    pub transcript_items: Vec<RuntimeStreamItemDto>,
    pub tool_calls: Vec<RuntimeStreamItemDto>,
    pub skill_items: Vec<RuntimeStreamItemDto>,
    pub activity_items: Vec<RuntimeStreamItemDto>,
    pub action_required: Vec<RuntimeStreamItemDto>,
    pub plan: Option<RuntimeStreamItemDto>,
    pub completion: Option<RuntimeStreamItemDto>,
    pub failure: Option<RuntimeStreamItemDto>,
    pub last_issue: Option<RuntimeStreamIssueDto>,
    pub last_item_at: Option<String>,
    pub last_sequence: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeStreamPatchDto {
    pub schema: String,
    pub item: RuntimeStreamItemDto,
    pub snapshot: RuntimeStreamViewSnapshotDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SubscribeRuntimeStreamRequestDto {
    pub project_id: String,
    pub agent_session_id: String,
    pub channel: Option<String>,
    pub item_kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_limit: Option<u16>,
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
