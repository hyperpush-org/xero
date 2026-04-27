use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceKindDto {
    Bundled,
    Local,
    Project,
    Github,
    Dynamic,
    Mcp,
    Plugin,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceScopeDto {
    Global,
    Project,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillSourceStateDto {
    Discoverable,
    Installed,
    Enabled,
    Disabled,
    Stale,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SkillTrustStateDto {
    Trusted,
    UserApproved,
    ApprovalRequired,
    Untrusted,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillSourceMetadataDto {
    pub label: String,
    pub repo: Option<String>,
    pub reference: Option<String>,
    pub path: Option<String>,
    pub root_id: Option<String>,
    pub root_path: Option<String>,
    pub relative_path: Option<String>,
    pub bundle_id: Option<String>,
    pub plugin_id: Option<String>,
    pub server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginCommandAvailabilityDto {
    Always,
    ProjectOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginCommandRiskLevelDto {
    Observe,
    ProjectRead,
    ProjectWrite,
    RunOwned,
    Network,
    SystemRead,
    OsAutomation,
    SignalExternal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginCommandApprovalPolicyDto {
    NeverForObserveOnly,
    Required,
    PerInvocation,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginCommandStatePolicyDto {
    Ephemeral,
    Project,
    Plugin,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRootDto {
    pub root_id: String,
    pub path: String,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginSkillContributionDto {
    pub contribution_id: String,
    pub skill_id: String,
    pub path: String,
    pub source_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginCommandContributionDto {
    pub command_id: String,
    pub plugin_id: String,
    pub contribution_id: String,
    pub label: String,
    pub description: String,
    pub entry: String,
    pub availability: PluginCommandAvailabilityDto,
    pub risk_level: PluginCommandRiskLevelDto,
    pub approval_policy: PluginCommandApprovalPolicyDto,
    pub state_policy: PluginCommandStatePolicyDto,
    pub redaction_required: bool,
    pub state: SkillSourceStateDto,
    pub trust: SkillTrustStateDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRegistryEntryDto {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub root_id: String,
    pub root_path: String,
    pub plugin_root_path: String,
    pub manifest_path: String,
    pub manifest_hash: String,
    pub state: SkillSourceStateDto,
    pub trust: SkillTrustStateDto,
    pub enabled: bool,
    pub skill_count: usize,
    pub command_count: usize,
    pub skills: Vec<PluginSkillContributionDto>,
    pub commands: Vec<PluginCommandContributionDto>,
    pub last_reloaded_at: Option<String>,
    pub last_diagnostic: Option<PluginDiagnosticDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSkillDiagnosticDto {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub recorded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillRegistryEntryDto {
    pub source_id: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub source_kind: SkillSourceKindDto,
    pub scope: SkillSourceScopeDto,
    pub project_id: Option<String>,
    pub source_state: SkillSourceStateDto,
    pub trust_state: SkillTrustStateDto,
    pub enabled: bool,
    pub installed: bool,
    pub user_invocable: Option<bool>,
    pub version_hash: Option<String>,
    pub last_used_at: Option<String>,
    pub last_diagnostic: Option<InstalledSkillDiagnosticDto>,
    pub source: SkillSourceMetadataDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillDiscoveryDiagnosticDto {
    pub code: String,
    pub message: String,
    pub relative_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillLocalRootDto {
    pub root_id: String,
    pub path: String,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillGithubSourceDto {
    pub repo: String,
    pub reference: String,
    pub root: String,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillProjectSourceDto {
    pub project_id: String,
    pub enabled: bool,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillSourceSettingsDto {
    pub local_roots: Vec<SkillLocalRootDto>,
    pub plugin_roots: Vec<PluginRootDto>,
    pub github: SkillGithubSourceDto,
    pub projects: Vec<SkillProjectSourceDto>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillRegistryDto {
    pub project_id: Option<String>,
    pub entries: Vec<SkillRegistryEntryDto>,
    pub plugins: Vec<PluginRegistryEntryDto>,
    pub plugin_commands: Vec<PluginCommandContributionDto>,
    pub sources: SkillSourceSettingsDto,
    pub diagnostics: Vec<SkillDiscoveryDiagnosticDto>,
    pub reloaded_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListSkillRegistryRequestDto {
    pub project_id: Option<String>,
    pub query: Option<String>,
    #[serde(default)]
    pub include_unavailable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetSkillEnabledRequestDto {
    pub project_id: String,
    pub source_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveSkillRequestDto {
    pub project_id: String,
    pub source_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertSkillLocalRootRequestDto {
    pub root_id: Option<String>,
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemoveSkillLocalRootRequestDto {
    pub root_id: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProjectSkillSourceRequestDto {
    pub project_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateGithubSkillSourceRequestDto {
    pub repo: String,
    pub reference: String,
    pub root: String,
    pub enabled: bool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertPluginRootRequestDto {
    pub root_id: Option<String>,
    pub path: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemovePluginRootRequestDto {
    pub root_id: String,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SetPluginEnabledRequestDto {
    pub project_id: String,
    pub plugin_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RemovePluginRequestDto {
    pub project_id: String,
    pub plugin_id: String,
}

const fn default_true() -> bool {
    true
}
