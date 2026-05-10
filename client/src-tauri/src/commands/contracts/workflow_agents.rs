use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use super::agent::{
    AgentDefinitionBaseCapabilityProfileDto, AgentDefinitionLifecycleStateDto,
    AgentDefinitionScopeDto,
};
use super::runtime::{
    RuntimeAgentIdDto, RuntimeAgentOutputContractDto, RuntimeAgentPromptPolicyDto,
    RuntimeAgentToolPolicyDto, RuntimeRunApprovalModeDto,
};
use super::skills::{
    SkillSourceKindDto, SkillSourceScopeDto, SkillSourceStateDto, SkillTrustStateDto,
};
use xero_agent_core::{DomainToolPackHealthReport, DomainToolPackManifest};

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
pub enum AgentAuthoringAvailabilityStatusDto {
    Available,
    RequiresProfileChange,
    Unavailable,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringProfileAvailabilityDto {
    pub subject_kind: String,
    pub subject_id: String,
    pub base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    pub status: AgentAuthoringAvailabilityStatusDto,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_profile: Option<AgentDefinitionBaseCapabilityProfileDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolPolicyDetailsDto {
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub denied_tools: Vec<String>,
    #[serde(default)]
    pub allowed_tool_packs: Vec<String>,
    #[serde(default)]
    pub denied_tool_packs: Vec<String>,
    #[serde(default)]
    pub allowed_tool_groups: Vec<String>,
    #[serde(default)]
    pub denied_tool_groups: Vec<String>,
    #[serde(default)]
    pub allowed_effect_classes: Vec<AgentToolEffectClassDto>,
    #[serde(default)]
    pub external_service_allowed: bool,
    #[serde(default)]
    pub browser_control_allowed: bool,
    #[serde(default)]
    pub skill_runtime_allowed: bool,
    #[serde(default)]
    pub subagent_allowed: bool,
    #[serde(default)]
    pub allowed_subagent_roles: Vec<String>,
    #[serde(default)]
    pub denied_subagent_roles: Vec<String>,
    #[serde(default)]
    pub command_allowed: bool,
    #[serde(default)]
    pub destructive_write_allowed: bool,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAttachedSkillAvailabilityStatusDto {
    Available,
    Unavailable,
    Stale,
    Blocked,
    Missing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAttachedSkillDto {
    pub id: String,
    pub source_id: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub source_kind: SkillSourceKindDto,
    pub scope: SkillSourceScopeDto,
    pub version_hash: String,
    pub include_supporting_assets: bool,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_state: Option<SkillSourceStateDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_state: Option<SkillTrustStateDto>,
    pub availability_status: AgentAttachedSkillAvailabilityStatusDto,
    pub availability_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repair_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentDetailDto {
    pub r#ref: AgentRefDto,
    pub header: AgentHeaderDto,
    pub prompt_policy: Option<RuntimeAgentPromptPolicyDto>,
    pub tool_policy: Option<RuntimeAgentToolPolicyDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_policy_details: Option<AgentToolPolicyDetailsDto>,
    pub prompts: Vec<AgentPromptDto>,
    pub tools: Vec<AgentToolSummaryDto>,
    pub db_touchpoints: AgentDbTouchpointsDto,
    pub output: AgentOutputContractDto,
    pub consumes: Vec<AgentConsumedArtifactDto>,
    pub attached_skills: Vec<AgentAttachedSkillDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authoring_graph: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub graph_projection: Option<WorkflowAgentGraphProjectionDto>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetWorkflowAgentGraphProjectionRequestDto {
    pub project_id: String,
    pub r#ref: AgentRefDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentGraphPositionDto {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowAgentGraphMarkerDto {
    Arrow,
    ArrowClosed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentGraphNodeDto {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    pub position: WorkflowAgentGraphPositionDto,
    pub data: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draggable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selectable: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drag_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentGraphEdgeDto {
    pub id: String,
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub edge_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_handle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_handle: Option<String>,
    pub data: JsonValue,
    pub class_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub marker: Option<WorkflowAgentGraphMarkerDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentGraphGroupDto {
    pub key: String,
    pub label: String,
    pub kind: String,
    pub order: i32,
    pub node_ids: Vec<String>,
    #[serde(default)]
    pub source_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkflowAgentGraphProjectionDto {
    pub schema: String,
    pub nodes: Vec<WorkflowAgentGraphNodeDto>,
    pub edges: Vec<WorkflowAgentGraphEdgeDto>,
    pub groups: Vec<WorkflowAgentGraphGroupDto>,
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
pub struct AgentAuthoringAttachableSkillDto {
    pub attachment_id: String,
    pub source_id: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub source_kind: SkillSourceKindDto,
    pub scope: SkillSourceScopeDto,
    pub version_hash: String,
    pub source_state: SkillSourceStateDto,
    pub trust_state: SkillTrustStateDto,
    pub availability_status: AgentAttachedSkillAvailabilityStatusDto,
    pub attachment: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringSkillSearchResultDto {
    pub source: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installs: Option<u64>,
    pub is_official: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAuthoringPolicyControlKindDto {
    Context,
    Memory,
    Retrieval,
    Handoff,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAuthoringPolicyControlValueKindDto {
    Boolean,
    PositiveInteger,
    StringArray,
    Object,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringPolicyControlDto {
    pub id: String,
    pub kind: AgentAuthoringPolicyControlKindDto,
    pub label: String,
    pub description: String,
    pub snapshot_path: String,
    pub value_kind: AgentAuthoringPolicyControlValueKindDto,
    pub default_value: JsonValue,
    pub runtime_effect: String,
    pub review_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringTemplateDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub task_kind: String,
    pub base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    pub definition: JsonValue,
    pub examples: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentAuthoringCreationFlowEntryKindDto {
    Template,
    DescribeIntent,
    ComposeTemplates,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringCreationFlowDto {
    pub id: String,
    pub label: String,
    pub description: String,
    pub entry_kind: AgentAuthoringCreationFlowEntryKindDto,
    pub task_kind: String,
    pub template_ids: Vec<String>,
    pub intent_prompt: String,
    pub expected_output_contract: RuntimeAgentOutputContractDto,
    pub base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringConstraintExplanationDto {
    pub id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub base_capability_profile: AgentDefinitionBaseCapabilityProfileDto,
    pub status: AgentAuthoringAvailabilityStatusDto,
    pub message: String,
    pub resolution: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_profile: Option<AgentDefinitionBaseCapabilityProfileDto>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringCatalogDiagnosticDto {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentAuthoringCatalogDto {
    pub contract_version: u32,
    pub tools: Vec<AgentToolSummaryDto>,
    pub tool_categories: Vec<AgentAuthoringToolCategoryDto>,
    pub db_tables: Vec<AgentAuthoringDbTableDto>,
    pub upstream_artifacts: Vec<AgentAuthoringUpstreamArtifactDto>,
    pub attachable_skills: Vec<AgentAuthoringAttachableSkillDto>,
    pub policy_controls: Vec<AgentAuthoringPolicyControlDto>,
    pub templates: Vec<AgentAuthoringTemplateDto>,
    pub creation_flows: Vec<AgentAuthoringCreationFlowDto>,
    pub profile_availability: Vec<AgentAuthoringProfileAvailabilityDto>,
    pub constraint_explanations: Vec<AgentAuthoringConstraintExplanationDto>,
    pub diagnostics: Vec<AgentAuthoringCatalogDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentAuthoringCatalogRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchAgentAuthoringSkillsRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    pub offset: usize,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchAgentAuthoringSkillsResponseDto {
    pub entries: Vec<AgentAuthoringSkillSearchResultDto>,
    pub offset: usize,
    pub limit: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_offset: Option<usize>,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResolveAgentAuthoringSkillRequestDto {
    pub project_id: String,
    pub source: String,
    pub skill_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentToolPackCatalogDto {
    pub schema: String,
    pub project_id: String,
    pub tool_packs: Vec<DomainToolPackManifest>,
    pub available_pack_ids: Vec<String>,
    pub health_reports: Vec<DomainToolPackHealthReport>,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GetAgentToolPackCatalogRequestDto {
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
