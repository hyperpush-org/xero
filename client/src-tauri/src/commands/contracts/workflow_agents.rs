use serde::{Deserialize, Serialize};

use super::agent::{
    AgentDefinitionBaseCapabilityProfileDto, AgentDefinitionLifecycleStateDto,
    AgentDefinitionScopeDto,
};
use super::runtime::{
    RuntimeAgentIdDto, RuntimeAgentOutputContractDto, RuntimeAgentPromptPolicyDto,
    RuntimeAgentToolPolicyDto, RuntimeRunApprovalModeDto,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AgentRefDto {
    BuiltIn {
        runtime_agent_id: RuntimeAgentIdDto,
        version: u32,
    },
    Custom {
        definition_id: String,
        version: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentSummaryDto {
    pub r#ref: AgentRefDto,
    pub display_name: String,
    pub short_label: String,
    pub description: String,
    pub scope: AgentDefinitionScopeDto,
    pub lifecycle_state: AgentDefinitionLifecycleStateDto,
    pub base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    pub last_used_at: Option<String>,
    pub use_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentHeaderDto {
    pub display_name: String,
    pub short_label: String,
    pub description: String,
    pub task_purpose: String,
    pub scope: AgentDefinitionScopeDto,
    pub lifecycle_state: AgentDefinitionLifecycleStateDto,
    pub base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    pub default_approval_mode: RuntimeRunApprovalModeDto,
    pub allowed_approval_modes: Vec<RuntimeRunApprovalModeDto>,
    pub allow_plan_gate: bool,
    pub allow_verification_gate: bool,
    pub allow_auto_compact: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentPromptRoleDto {
    System,
    Developer,
    Task,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentPromptDto {
    pub id: String,
    pub label: String,
    pub role: AgentPromptRoleDto,
    pub policy: Option<RuntimeAgentPromptPolicyDto>,
    pub source: String,
    pub body: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolEffectClassDto {
    Observe,
    RuntimeState,
    Write,
    DestructiveWrite,
    Command,
    ProcessControl,
    BrowserControl,
    DeviceControl,
    ExternalService,
    SkillRuntime,
    AgentDelegation,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolSummaryDto {
    pub name: String,
    pub group: String,
    pub description: String,
    pub effect_class: AgentToolEffectClassDto,
    pub risk_class: String,
    pub tags: Vec<String>,
    pub schema_fields: Vec<String>,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentDbTouchpointKindDto {
    Read,
    Write,
    Encouraged,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTriggerLifecycleEventDto {
    StateTransition,
    PlanUpdate,
    MessagePersisted,
    ToolCall,
    FileEdit,
    RunStart,
    RunComplete,
    ApprovalDecision,
    VerificationGate,
    DefinitionPersisted,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum AgentTriggerRefDto {
    Tool {
        name: String,
    },
    OutputSection {
        id: String,
    },
    Lifecycle {
        event: AgentTriggerLifecycleEventDto,
    },
    UpstreamArtifact {
        id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDbTouchpointDetailDto {
    pub table: String,
    pub kind: AgentDbTouchpointKindDto,
    pub purpose: String,
    pub triggers: Vec<AgentTriggerRefDto>,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentDbTouchpointsDto {
    pub reads: Vec<AgentDbTouchpointDetailDto>,
    pub writes: Vec<AgentDbTouchpointDetailDto>,
    pub encouraged: Vec<AgentDbTouchpointDetailDto>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentOutputSectionEmphasisDto {
    Core,
    Standard,
    Optional,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentOutputSectionDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub emphasis: AgentOutputSectionEmphasisDto,
    pub produced_by_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentOutputContractDto {
    pub contract: RuntimeAgentOutputContractDto,
    pub label: String,
    pub description: String,
    pub sections: Vec<AgentOutputSectionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentConsumedArtifactDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub source_agent: RuntimeAgentIdDto,
    pub contract: RuntimeAgentOutputContractDto,
    pub sections: Vec<String>,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentDetailDto {
    pub r#ref: AgentRefDto,
    pub header: AgentHeaderDto,
    pub prompt_policy: Option<RuntimeAgentPromptPolicyDto>,
    pub tool_policy: Option<RuntimeAgentToolPolicyDto>,
    pub prompts: Vec<AgentPromptDto>,
    pub tools: Vec<AgentToolSummaryDto>,
    pub db_touchpoints: AgentDbTouchpointsDto,
    pub output: AgentOutputContractDto,
    pub consumes: Vec<AgentConsumedArtifactDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListWorkflowAgentsRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListWorkflowAgentsResponseDto {
    pub agents: Vec<WorkflowAgentSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetWorkflowAgentDetailRequestDto {
    pub project_id: String,
    pub r#ref: AgentRefDto,
}

// Catalog entries the canvas authoring UI uses to populate pickers. The shapes
// mirror the in-detail summaries (AgentToolSummaryDto / AgentDbTouchpointDetailDto
// / AgentConsumedArtifactDto) but are surfaced unfiltered so the user sees every
// allowable choice, not the per-agent filtered subset.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringDbTableDto {
    pub table: String,
    pub purpose: String,
    pub columns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringUpstreamArtifactDto {
    pub source_agent: RuntimeAgentIdDto,
    pub source_agent_label: String,
    pub contract: RuntimeAgentOutputContractDto,
    pub contract_label: String,
    pub label: String,
    pub description: String,
    pub sections: Vec<AgentOutputSectionDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringToolCategoryDto {
    // Stable group identifier (matches AgentToolSummaryDto.group). The drag
    // picker shows label; the snapshot stores tools by name.
    pub id: String,
    pub label: String,
    pub description: String,
    // Tools in this category, in catalog order.
    pub tools: Vec<AgentToolSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringCatalogDto {
    pub tools: Vec<AgentToolSummaryDto>,
    pub tool_categories: Vec<AgentAuthoringToolCategoryDto>,
    pub db_tables: Vec<AgentAuthoringDbTableDto>,
    pub upstream_artifacts: Vec<AgentAuthoringUpstreamArtifactDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentAuthoringCatalogRequestDto {
    pub project_id: String,
}

pub fn output_contract_label(contract: RuntimeAgentOutputContractDto) -> &'static str {
    match contract {
        RuntimeAgentOutputContractDto::Answer => "Answer",
        RuntimeAgentOutputContractDto::PlanPack => "Plan Pack",
        RuntimeAgentOutputContractDto::CrawlReport => "Crawl Report",
        RuntimeAgentOutputContractDto::EngineeringSummary => "Engineering Summary",
        RuntimeAgentOutputContractDto::DebugSummary => "Debug Summary",
        RuntimeAgentOutputContractDto::AgentDefinitionDraft => "Agent Definition Draft",
        RuntimeAgentOutputContractDto::HarnessTestReport => "Harness Test Report",
    }
}

pub fn output_contract_description(contract: RuntimeAgentOutputContractDto) -> &'static str {
    match contract {
        RuntimeAgentOutputContractDto::Answer => {
            "Direct chat answer with citations and uncertainty calls; no repository or process mutation."
        }
        RuntimeAgentOutputContractDto::PlanPack => {
            "Schema xero.plan_pack.v1 with goal, constraints, decisions, slices, build handoff, risks, and open questions."
        }
        RuntimeAgentOutputContractDto::CrawlReport => {
            "Schema xero.project_crawl.report.v1 with techStack, commands, architecture, hotspots, and freshness."
        }
        RuntimeAgentOutputContractDto::EngineeringSummary => {
            "Summary of files changed, verification run, blockers, and durable handoff context."
        }
        RuntimeAgentOutputContractDto::DebugSummary => {
            "Symptom, root cause, fix, files changed, verification, saved debugging knowledge, remaining risks."
        }
        RuntimeAgentOutputContractDto::AgentDefinitionDraft => {
            "Reviewable agent-definition draft with capabilities, safety limits, validation diagnostics, and persisted version on activation."
        }
        RuntimeAgentOutputContractDto::HarnessTestReport => {
            "Deterministic Test-agent harness validation report compared against the canonical manifest."
        }
    }
}
