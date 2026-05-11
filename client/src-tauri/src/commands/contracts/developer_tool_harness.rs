use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolPackSummaryDto {
    pub pack_id: String,
    pub label: String,
    pub policy_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolCatalogEntryDto {
    pub tool_name: String,
    pub group: String,
    pub description: String,
    pub tags: Vec<String>,
    pub schema_fields: Vec<String>,
    pub examples: Vec<String>,
    pub risk_class: String,
    pub effect_class: String,
    pub runtime_available: bool,
    pub allowed_runtime_agents: Vec<String>,
    pub activation_groups: Vec<String>,
    pub tool_packs: Vec<DeveloperToolPackSummaryDto>,
    pub input_schema: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolCatalogResponseDto {
    pub host_os: String,
    pub host_os_label: String,
    pub skill_tool_enabled: bool,
    pub entries: Vec<DeveloperToolCatalogEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolHarnessCallDto {
    pub tool_name: String,
    pub input: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolHarnessRunOptionsDto {
    #[serde(default)]
    pub stop_on_failure: Option<bool>,
    #[serde(default)]
    pub approve_writes: Option<bool>,
    #[serde(default)]
    pub operator_approve_all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolSyntheticRunRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub agent_session_id: Option<String>,
    pub calls: Vec<DeveloperToolHarnessCallDto>,
    #[serde(default)]
    pub options: Option<DeveloperToolHarnessRunOptionsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolHarnessCallResultDto {
    pub tool_call_id: String,
    pub tool_name: String,
    pub ok: bool,
    pub summary: String,
    pub output: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolSyntheticRunResponseDto {
    pub run_id: String,
    pub agent_session_id: String,
    pub stopped_early: bool,
    pub had_failure: bool,
    pub results: Vec<DeveloperToolHarnessCallResultDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolDryRunRequestDto {
    pub project_id: String,
    pub tool_name: String,
    pub input: JsonValue,
    #[serde(default)]
    pub tool_call_id: Option<String>,
    #[serde(default)]
    pub operator_approved: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolPolicyDecisionDto {
    pub action: String,
    pub code: String,
    pub explanation: String,
    pub risk_class: String,
    pub project_trust: String,
    pub network_intent: String,
    pub credential_sensitivity: String,
    pub prior_observation_required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolDryRunResponseDto {
    pub tool_call_id: String,
    pub tool_name: String,
    pub decoded: bool,
    pub policy_decision: DeveloperToolPolicyDecisionDto,
    pub sandbox_decision: JsonValue,
    pub sandbox_denied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolSequenceRecordDto {
    pub id: String,
    pub name: String,
    pub calls: Vec<DeveloperToolHarnessCallDto>,
    #[serde(default)]
    pub options: Option<DeveloperToolHarnessRunOptionsDto>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolSequenceListResponseDto {
    pub sequences: Vec<DeveloperToolSequenceRecordDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolSequenceUpsertRequestDto {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub calls: Vec<DeveloperToolHarnessCallDto>,
    #[serde(default)]
    pub options: Option<DeveloperToolHarnessRunOptionsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolSequenceDeleteRequestDto {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolModelRunRequestDto {
    pub project_id: String,
    pub tool_name: String,
    pub prompt: String,
    #[serde(default)]
    pub agent_session_id: Option<String>,
    #[serde(default)]
    pub runtime_agent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolModelRunResponseDto {
    pub run_id: String,
    pub agent_session_id: String,
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolHarnessProjectDto {
    pub project_id: String,
    pub display_name: String,
    pub root_path: String,
}
