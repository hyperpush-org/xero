mod cache;
mod contract;
mod discovery;
mod inspection;
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
    merge_skill_source_records, validate_skill_source_state_transition, CadenceSkillSourceKind,
    CadenceSkillSourceLocator, CadenceSkillSourceRecord, CadenceSkillSourceScope,
    CadenceSkillSourceState, CadenceSkillTrustState, CADENCE_SKILL_SOURCE_CONTRACT_VERSION,
};
pub use discovery::{
    discover_bundled_skill_directory, discover_local_skill_directory,
    discover_project_skill_directory, load_discovered_skill_context,
    load_skill_context_from_directory, CadenceDiscoveredSkill, CadenceSkillDirectoryDiscovery,
    CadenceSkillDiscoveryDiagnostic, PROJECT_SKILL_DIRECTORY,
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
    SkillLocalRootSetting, SkillProjectSourceSetting, SkillSourceSettings,
    SKILL_SOURCE_SETTINGS_SCHEMA_VERSION,
};
pub use skill_tool::{
    decide_skill_tool_access, model_may_discover_skill_source,
    skill_tool_diagnostic_from_command_error, validate_skill_tool_context_payload,
    validate_skill_tool_input, validate_skill_tool_lifecycle_event, CadenceSkillToolAccessDecision,
    CadenceSkillToolAccessStatus, CadenceSkillToolContextAsset, CadenceSkillToolContextDocument,
    CadenceSkillToolContextPayload, CadenceSkillToolDiagnostic, CadenceSkillToolDynamicAssetInput,
    CadenceSkillToolInput, CadenceSkillToolLifecycleEvent, CadenceSkillToolLifecycleResult,
    CadenceSkillToolOperation, CADENCE_SKILL_TOOL_CONTRACT_VERSION,
    CADENCE_SKILL_TOOL_DEFAULT_LIMIT, CADENCE_SKILL_TOOL_MAX_CONTEXT_ASSETS,
    CADENCE_SKILL_TOOL_MAX_CONTEXT_ASSET_BYTES, CADENCE_SKILL_TOOL_MAX_CONTEXT_MARKDOWN_BYTES,
    CADENCE_SKILL_TOOL_MAX_LIMIT, CADENCE_SKILL_TOOL_MAX_QUERY_CHARS,
};
pub use source::{
    AutonomousSkillSource, AutonomousSkillSourceEntryKind, AutonomousSkillSourceError,
    AutonomousSkillSourceFileRequest, AutonomousSkillSourceFileResponse,
    AutonomousSkillSourceMetadata, AutonomousSkillSourceTreeEntry,
    AutonomousSkillSourceTreeRequest, AutonomousSkillSourceTreeResponse,
    GithubAutonomousSkillSource,
};
