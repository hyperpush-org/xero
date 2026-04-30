mod cache;
mod contract;
mod discovery;
mod inspection;
mod plugin;
mod runtime;
mod settings;
mod skill_tool;
mod source;

pub use cache::{
    AutonomousSkillCacheError, AutonomousSkillCacheInstallFile, AutonomousSkillCacheManifest,
    AutonomousSkillCacheManifestFile, AutonomousSkillCacheStatus, AutonomousSkillCacheStore,
    FilesystemAutonomousSkillCacheStore,
};
pub use contract::{
    merge_skill_source_records, validate_skill_source_state_transition, XeroSkillSourceKind,
    XeroSkillSourceLocator, XeroSkillSourceRecord, XeroSkillSourceScope, XeroSkillSourceState,
    XeroSkillTrustState, XERO_SKILL_SOURCE_CONTRACT_VERSION,
};
pub use discovery::{
    discover_bundled_skill_directory, discover_local_skill_directory,
    discover_plugin_skill_contribution, discover_project_skill_directory,
    load_discovered_skill_context, load_skill_context_from_directory, XeroDiscoveredSkill,
    XeroSkillDirectoryDiscovery, XeroSkillDiscoveryDiagnostic, PROJECT_SKILL_DIRECTORY,
};
pub use plugin::{
    discover_plugin_roots, normalize_plugin_contribution_id, normalize_plugin_id,
    parse_plugin_manifest, plugin_command_stable_id, plugin_trust_declaration_to_skill_trust,
    XeroDiscoveredPlugin, XeroPluginCommandApprovalPolicy, XeroPluginCommandAvailability,
    XeroPluginCommandContribution, XeroPluginCommandRiskLevel, XeroPluginCommandStatePolicy,
    XeroPluginDiscovery, XeroPluginDiscoveryDiagnostic, XeroPluginEntryKind,
    XeroPluginEntryLocation, XeroPluginManifest, XeroPluginRoot, XeroPluginSkillContribution,
    XeroPluginTrustDeclaration, XERO_PLUGIN_MANIFEST_FILE, XERO_PLUGIN_MANIFEST_SCHEMA_VERSION,
    XERO_PLUGIN_NESTED_MANIFEST_FILE,
};
pub use runtime::{
    AutonomousSkillDiscoverOutput, AutonomousSkillDiscoverRequest,
    AutonomousSkillDiscoveryCandidate, AutonomousSkillInstallOutput, AutonomousSkillInstallRequest,
    AutonomousSkillInvocationAsset, AutonomousSkillInvokeOutput, AutonomousSkillInvokeRequest,
    AutonomousSkillRegistryFailure, AutonomousSkillRegistryOperation, AutonomousSkillRegistrySink,
    AutonomousSkillRegistrySuccess, AutonomousSkillResolveOutput, AutonomousSkillResolveRequest,
    AutonomousSkillRuntime, AutonomousSkillRuntimeConfig, AutonomousSkillRuntimeLimits,
    AUTONOMOUS_SKILL_SOURCE_REF, AUTONOMOUS_SKILL_SOURCE_REPO, AUTONOMOUS_SKILL_SOURCE_ROOT,
};
pub use settings::{
    load_skill_source_settings_from_path, persist_skill_source_settings, SkillGithubSourceSetting,
    SkillLocalRootSetting, SkillPluginRootSetting, SkillProjectSourceSetting, SkillSourceSettings,
    SKILL_SOURCE_SETTINGS_SCHEMA_VERSION,
};
pub use skill_tool::{
    decide_skill_tool_access, model_may_discover_skill_source, sanitize_skill_tool_model_text,
    skill_tool_diagnostic_from_command_error, validate_skill_tool_context_payload,
    validate_skill_tool_input, validate_skill_tool_lifecycle_event, XeroSkillToolAccessDecision,
    XeroSkillToolAccessStatus, XeroSkillToolContextAsset, XeroSkillToolContextDocument,
    XeroSkillToolContextPayload, XeroSkillToolDiagnostic, XeroSkillToolDynamicAssetInput,
    XeroSkillToolInput, XeroSkillToolLifecycleEvent, XeroSkillToolLifecycleResult,
    XeroSkillToolOperation, XERO_SKILL_TOOL_CONTRACT_VERSION, XERO_SKILL_TOOL_DEFAULT_LIMIT,
    XERO_SKILL_TOOL_MAX_CONTEXT_ASSETS, XERO_SKILL_TOOL_MAX_CONTEXT_ASSET_BYTES,
    XERO_SKILL_TOOL_MAX_CONTEXT_MARKDOWN_BYTES, XERO_SKILL_TOOL_MAX_LIMIT,
    XERO_SKILL_TOOL_MAX_QUERY_CHARS,
};
pub use source::{
    AutonomousSkillSource, AutonomousSkillSourceEntryKind, AutonomousSkillSourceError,
    AutonomousSkillSourceFileRequest, AutonomousSkillSourceFileResponse,
    AutonomousSkillSourceMetadata, AutonomousSkillSourceTreeEntry,
    AutonomousSkillSourceTreeRequest, AutonomousSkillSourceTreeResponse,
    GithubAutonomousSkillSource,
};
