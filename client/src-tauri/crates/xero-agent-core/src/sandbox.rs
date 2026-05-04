use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    ToolCallInput, ToolDescriptorV2, ToolEffectClass, ToolExecutionContext, ToolExecutionError,
    ToolMutability, ToolSandboxRequirement,
};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPermissionProfile {
    ReadOnly,
    WorkspaceWrite,
    WorkspaceWriteNetworkDenied,
    WorkspaceWriteNetworkAllowed,
    FullLocalWithApproval,
    DangerousUnrestricted,
}

impl SandboxPermissionProfile {
    pub fn for_descriptor(descriptor: &ToolDescriptorV2) -> Self {
        match descriptor.sandbox_requirement {
            ToolSandboxRequirement::None if descriptor.mutability == ToolMutability::ReadOnly => {
                Self::ReadOnly
            }
            ToolSandboxRequirement::None => Self::FullLocalWithApproval,
            ToolSandboxRequirement::ReadOnly => Self::ReadOnly,
            ToolSandboxRequirement::WorkspaceWrite => Self::WorkspaceWrite,
            ToolSandboxRequirement::Network => Self::WorkspaceWriteNetworkAllowed,
            ToolSandboxRequirement::FullLocal => Self::FullLocalWithApproval,
        }
    }

    pub const fn network_mode(self) -> SandboxNetworkMode {
        match self {
            Self::WorkspaceWriteNetworkAllowed
            | Self::FullLocalWithApproval
            | Self::DangerousUnrestricted => SandboxNetworkMode::Allowed,
            Self::ReadOnly | Self::WorkspaceWrite | Self::WorkspaceWriteNetworkDenied => {
                SandboxNetworkMode::Denied
            }
        }
    }

    pub const fn allows_workspace_write(self) -> bool {
        matches!(
            self,
            Self::WorkspaceWrite
                | Self::WorkspaceWriteNetworkDenied
                | Self::WorkspaceWriteNetworkAllowed
                | Self::FullLocalWithApproval
                | Self::DangerousUnrestricted
        )
    }

    pub const fn requires_project_trust(self) -> bool {
        !matches!(self, Self::ReadOnly | Self::DangerousUnrestricted)
    }

    pub const fn requires_approval(self) -> bool {
        matches!(self, Self::FullLocalWithApproval)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxNetworkMode {
    Denied,
    Allowed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTrustState {
    Trusted,
    UserApproved,
    ApprovalRequired,
    Untrusted,
    Blocked,
}

impl ProjectTrustState {
    pub const fn allows_privileged_tools(self) -> bool {
        matches!(self, Self::Trusted | Self::UserApproved)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxApprovalSource {
    None,
    Policy,
    Operator,
    DangerousUnrestricted,
}

impl SandboxApprovalSource {
    pub const fn satisfies_full_local(self) -> bool {
        matches!(
            self,
            Self::Policy | Self::Operator | Self::DangerousUnrestricted
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPlatform {
    Macos,
    Linux,
    Windows,
    Unsupported,
}

impl SandboxPlatform {
    pub const fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::Macos
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Unsupported
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxPlatformStrategy {
    MacosSandboxExec,
    LinuxBubblewrap,
    WindowsRestrictedToken,
    PortablePreflightOnly,
    DangerousUnrestricted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum SandboxExitClassification {
    NotRun,
    Success,
    Failed,
    DeniedBySandbox,
    Timeout,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxEnvironmentRedactionSummary {
    pub sanitized_environment: bool,
    pub preserved_keys: Vec<String>,
    pub redacted_key_count: usize,
    pub secret_like_key_count: usize,
}

impl Default for SandboxEnvironmentRedactionSummary {
    fn default() -> Self {
        Self {
            sanitized_environment: true,
            preserved_keys: Vec::new(),
            redacted_key_count: 0,
            secret_like_key_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxInternalStateProtection {
    pub git_mutation_allowed: bool,
    pub app_data_state_protected: bool,
    pub legacy_xero_state_policy: String,
}

impl Default for SandboxInternalStateProtection {
    fn default() -> Self {
        Self {
            git_mutation_allowed: false,
            app_data_state_protected: true,
            legacy_xero_state_policy: "read_only_unless_migration".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OsSandboxPlan {
    pub platform: SandboxPlatform,
    pub strategy: SandboxPlatformStrategy,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub argv_prefix: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_text: Option<String>,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxExecutionMetadata {
    pub profile: SandboxPermissionProfile,
    pub network_mode: SandboxNetworkMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub readable_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub writable_paths: Vec<String>,
    pub environment_redaction: SandboxEnvironmentRedactionSummary,
    pub approval_source: SandboxApprovalSource,
    pub exit_classification: SandboxExitClassification,
    pub platform_plan: OsSandboxPlan,
    pub internal_state: SandboxInternalStateProtection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

impl SandboxExecutionMetadata {
    pub fn unrestricted() -> Self {
        let profile = SandboxPermissionProfile::DangerousUnrestricted;
        Self {
            profile,
            network_mode: profile.network_mode(),
            readable_paths: Vec::new(),
            writable_paths: Vec::new(),
            environment_redaction: SandboxEnvironmentRedactionSummary::default(),
            approval_source: SandboxApprovalSource::DangerousUnrestricted,
            exit_classification: SandboxExitClassification::NotRun,
            platform_plan: OsSandboxPlan {
                platform: SandboxPlatform::current(),
                strategy: SandboxPlatformStrategy::DangerousUnrestricted,
                argv_prefix: Vec::new(),
                profile_text: None,
                explanation: "Dangerous unrestricted mode bypasses OS sandbox wrapping.".into(),
            },
            internal_state: SandboxInternalStateProtection {
                git_mutation_allowed: true,
                app_data_state_protected: false,
                legacy_xero_state_policy: "unrestricted".into(),
            },
            blocked_reason: None,
        }
    }

    fn denied(mut self, reason: impl Into<String>) -> Self {
        self.exit_classification = SandboxExitClassification::DeniedBySandbox;
        self.blocked_reason = Some(reason.into());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxExecutionContext {
    pub workspace_root: String,
    #[serde(default)]
    pub app_data_roots: Vec<String>,
    pub project_trust: ProjectTrustState,
    pub approval_source: SandboxApprovalSource,
    pub platform: SandboxPlatform,
    #[serde(default)]
    pub explicit_git_mutation_allowed: bool,
    #[serde(default)]
    pub legacy_xero_migration_allowed: bool,
    #[serde(default)]
    pub preserved_environment_keys: Vec<String>,
}

impl Default for SandboxExecutionContext {
    fn default() -> Self {
        Self {
            workspace_root: ".".into(),
            app_data_roots: Vec::new(),
            project_trust: ProjectTrustState::Trusted,
            approval_source: SandboxApprovalSource::None,
            platform: SandboxPlatform::current(),
            explicit_git_mutation_allowed: false,
            legacy_xero_migration_allowed: false,
            preserved_environment_keys: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxExecutionDenied {
    pub error: ToolExecutionError,
    pub metadata: SandboxExecutionMetadata,
}

pub type ToolSandboxResult = Result<SandboxExecutionMetadata, SandboxExecutionDenied>;

pub trait ToolSandbox: Send + Sync {
    fn evaluate(
        &self,
        descriptor: &ToolDescriptorV2,
        call: &ToolCallInput,
        context: &ToolExecutionContext,
    ) -> ToolSandboxResult;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct NoopToolSandbox;

impl ToolSandbox for NoopToolSandbox {
    fn evaluate(
        &self,
        _descriptor: &ToolDescriptorV2,
        _call: &ToolCallInput,
        _context: &ToolExecutionContext,
    ) -> ToolSandboxResult {
        Ok(SandboxExecutionMetadata::unrestricted())
    }
}

#[derive(Debug, Clone, Default)]
pub struct PermissionProfileSandbox {
    context: SandboxExecutionContext,
    profile_overrides: BTreeMap<String, SandboxPermissionProfile>,
}

impl PermissionProfileSandbox {
    pub fn new(context: SandboxExecutionContext) -> Self {
        Self {
            context,
            profile_overrides: BTreeMap::new(),
        }
    }

    pub fn with_profile_override(
        mut self,
        tool_name: impl Into<String>,
        profile: SandboxPermissionProfile,
    ) -> Self {
        self.profile_overrides.insert(tool_name.into(), profile);
        self
    }

    pub fn context(&self) -> &SandboxExecutionContext {
        &self.context
    }

    fn profile_for(&self, descriptor: &ToolDescriptorV2) -> SandboxPermissionProfile {
        self.profile_overrides
            .get(&descriptor.name)
            .copied()
            .unwrap_or_else(|| SandboxPermissionProfile::for_descriptor(descriptor))
    }
}

impl ToolSandbox for PermissionProfileSandbox {
    fn evaluate(
        &self,
        descriptor: &ToolDescriptorV2,
        call: &ToolCallInput,
        _context: &ToolExecutionContext,
    ) -> ToolSandboxResult {
        let profile = self.profile_for(descriptor);
        let path_access = SandboxPathAccess::from_tool_call(descriptor, call);
        let metadata = sandbox_metadata(profile, &self.context, &path_access);

        if profile.requires_project_trust() && !self.context.project_trust.allows_privileged_tools()
        {
            let reason = format!(
                "Sandbox profile `{profile:?}` requires a trusted project before write or command tools can run."
            );
            return deny(metadata, "agent_sandbox_project_untrusted", reason);
        }

        if profile.requires_approval() && !self.context.approval_source.satisfies_full_local() {
            let reason = format!(
                "Sandbox profile `{profile:?}` requires explicit policy or operator approval before full local access."
            );
            return deny(metadata, "agent_sandbox_approval_required", reason);
        }

        if !profile.allows_workspace_write() && !path_access.write_paths.is_empty() {
            let reason =
                format!("Sandbox profile `{profile:?}` does not allow workspace mutations.");
            return deny(metadata, "agent_sandbox_write_denied", reason);
        }

        if profile.network_mode() == SandboxNetworkMode::Denied && path_access.network_intent {
            let reason = "Sandbox profile denies network access for this command or tool call.";
            return deny(metadata, "agent_sandbox_network_denied", reason);
        }

        for path in &path_access.write_paths {
            if let Err(reason) = validate_write_path(path, &self.context) {
                return deny(metadata, "agent_sandbox_path_denied", reason);
            }
        }

        Ok(metadata)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct SandboxPathAccess {
    read_paths: Vec<String>,
    write_paths: Vec<String>,
    network_intent: bool,
}

impl SandboxPathAccess {
    fn from_tool_call(descriptor: &ToolDescriptorV2, call: &ToolCallInput) -> Self {
        let mut access = Self::default();
        let extracted_paths = extract_path_values(&call.input);

        if descriptor.mutability == ToolMutability::Mutating
            || matches!(
                descriptor.effect_class,
                ToolEffectClass::WorkspaceMutation | ToolEffectClass::CommandExecution
            )
        {
            access.write_paths.extend(extracted_paths);
        } else {
            access.read_paths.extend(extracted_paths);
        }

        access.network_intent = descriptor.sandbox_requirement == ToolSandboxRequirement::Network
            || matches!(descriptor.effect_class, ToolEffectClass::ExternalService)
            || command_input_has_network_intent(&call.input);
        access
    }
}

fn sandbox_metadata(
    profile: SandboxPermissionProfile,
    context: &SandboxExecutionContext,
    path_access: &SandboxPathAccess,
) -> SandboxExecutionMetadata {
    let mut readable_paths = dedupe(path_access.read_paths.clone());
    if readable_paths.is_empty() && profile != SandboxPermissionProfile::DangerousUnrestricted {
        readable_paths.push(context.workspace_root.clone());
    }

    SandboxExecutionMetadata {
        profile,
        network_mode: profile.network_mode(),
        readable_paths,
        writable_paths: dedupe(path_access.write_paths.clone()),
        environment_redaction: SandboxEnvironmentRedactionSummary {
            sanitized_environment: true,
            preserved_keys: context.preserved_environment_keys.clone(),
            redacted_key_count: 0,
            secret_like_key_count: 0,
        },
        approval_source: context.approval_source,
        exit_classification: SandboxExitClassification::NotRun,
        platform_plan: platform_plan(profile, context),
        internal_state: SandboxInternalStateProtection {
            git_mutation_allowed: context.explicit_git_mutation_allowed,
            app_data_state_protected: true,
            legacy_xero_state_policy: if context.legacy_xero_migration_allowed {
                "migration_allowed".into()
            } else {
                "read_only_unless_migration".into()
            },
        },
        blocked_reason: None,
    }
}

fn platform_plan(
    profile: SandboxPermissionProfile,
    context: &SandboxExecutionContext,
) -> OsSandboxPlan {
    if profile == SandboxPermissionProfile::DangerousUnrestricted {
        return SandboxExecutionMetadata::unrestricted().platform_plan;
    }

    match context.platform {
        SandboxPlatform::Macos => {
            let profile_text = macos_sandbox_exec_profile(profile, context);
            OsSandboxPlan {
                platform: context.platform,
                strategy: SandboxPlatformStrategy::MacosSandboxExec,
                argv_prefix: vec!["sandbox-exec".into(), "-p".into(), profile_text.clone()],
                profile_text: Some(profile_text),
                explanation: "macOS commands run through sandbox-exec with workspace file and network boundaries.".into(),
            }
        }
        SandboxPlatform::Linux => OsSandboxPlan {
            platform: context.platform,
            strategy: SandboxPlatformStrategy::LinuxBubblewrap,
            argv_prefix: Vec::new(),
            profile_text: None,
            explanation: "Linux commands should run through bubblewrap when available; portable preflight remains active before spawn.".into(),
        },
        SandboxPlatform::Windows => OsSandboxPlan {
            platform: context.platform,
            strategy: SandboxPlatformStrategy::WindowsRestrictedToken,
            argv_prefix: Vec::new(),
            profile_text: None,
            explanation: "Windows commands should run with restricted process/token settings; portable preflight remains active before spawn.".into(),
        },
        SandboxPlatform::Unsupported => OsSandboxPlan {
            platform: context.platform,
            strategy: SandboxPlatformStrategy::PortablePreflightOnly,
            argv_prefix: Vec::new(),
            profile_text: None,
            explanation: "This platform has portable sandbox preflight checks but no OS wrapper strategy yet.".into(),
        },
    }
}

fn macos_sandbox_exec_profile(
    profile: SandboxPermissionProfile,
    context: &SandboxExecutionContext,
) -> String {
    let workspace = escape_sandbox_string(&context.workspace_root);
    let mut lines = vec![
        "(version 1)".to_string(),
        "(deny default)".to_string(),
        "(allow process*)".to_string(),
        "(allow sysctl-read)".to_string(),
        "(allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/System\") (subpath \"/Library\"))".to_string(),
        format!("(allow file-read* (subpath \"{workspace}\"))"),
        format!("(deny file-write* (subpath \"{workspace}/.git\"))"),
        format!("(deny file-write* (subpath \"{workspace}/.xero\"))"),
    ];

    for root in &context.app_data_roots {
        lines.push(format!(
            "(deny file-write* (subpath \"{}\"))",
            escape_sandbox_string(root)
        ));
    }

    if profile.allows_workspace_write() {
        lines.push(format!("(allow file-write* (subpath \"{workspace}\"))"));
    }

    if profile.network_mode() == SandboxNetworkMode::Allowed {
        lines.push("(allow network*)".to_string());
    } else {
        lines.push("(deny network*)".to_string());
    }

    lines.join("\n")
}

fn validate_write_path(path: &str, context: &SandboxExecutionContext) -> Result<(), String> {
    let normalized = normalize_user_path(path)?;
    let mut protected_components = normalized.components.clone();
    if normalized.is_absolute {
        let absolute = normalized.rendered.as_str();
        if context
            .app_data_roots
            .iter()
            .map(|root| normalize_absolute(root))
            .any(|root| path_starts_with(absolute, &root))
        {
            return Err(format!(
                "Sandbox denied write `{path}` because OS app-data state is not an ordinary project working file."
            ));
        }

        let workspace = normalize_absolute(&context.workspace_root);
        if !path_starts_with(absolute, &workspace) {
            return Err(format!(
                "Sandbox denied write `{path}` because it is outside the workspace root."
            ));
        }

        protected_components = absolute
            .strip_prefix(&workspace)
            .unwrap_or_default()
            .trim_start_matches('/')
            .split('/')
            .filter(|component| !component.is_empty() && *component != ".")
            .map(str::to_owned)
            .collect();
    }

    if protected_components
        .first()
        .is_some_and(|part| part == ".git")
        && !context.explicit_git_mutation_allowed
    {
        return Err(format!(
            "Sandbox denied write `{path}` because `.git` mutation requires explicit policy."
        ));
    }

    if protected_components
        .first()
        .is_some_and(|part| part == ".xero")
        && !context.legacy_xero_migration_allowed
    {
        return Err(format!(
            "Sandbox denied write `{path}` because `.xero/` is legacy repo-local state and is read-only unless a planned migration allows it."
        ));
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedUserPath {
    rendered: String,
    components: Vec<String>,
    is_absolute: bool,
}

fn normalize_user_path(path: &str) -> Result<NormalizedUserPath, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Sandbox denied an empty path.".into());
    }

    let slash_path = trimmed.replace('\\', "/");
    let is_windows_absolute = slash_path
        .as_bytes()
        .get(1)
        .is_some_and(|value| *value == b':');
    let is_absolute = slash_path.starts_with('/') || is_windows_absolute;
    let mut components = Vec::new();

    for component in slash_path.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            return Err(format!(
                "Sandbox denied path `{path}` because it escapes the workspace root."
            ));
        }
        components.push(component.to_string());
    }

    if components.is_empty() {
        return Err(format!(
            "Sandbox denied path `{path}` because it does not name a workspace file."
        ));
    }

    Ok(NormalizedUserPath {
        rendered: if is_absolute {
            slash_path
        } else {
            components.join("/")
        },
        components,
        is_absolute,
    })
}

fn extract_path_values(input: &JsonValue) -> Vec<String> {
    let mut paths = Vec::new();
    extract_path_values_inner(input, None, &mut paths);
    paths
}

fn extract_path_values_inner(value: &JsonValue, key: Option<&str>, paths: &mut Vec<String>) {
    match value {
        JsonValue::String(text) if key.is_some_and(is_path_field_name) => {
            paths.push(text.clone());
        }
        JsonValue::Array(items) => {
            for item in items {
                extract_path_values_inner(item, key, paths);
            }
        }
        JsonValue::Object(fields) => {
            for (field, value) in fields {
                extract_path_values_inner(value, Some(field), paths);
            }
        }
        _ => {}
    }
}

fn is_path_field_name(key: &str) -> bool {
    matches!(
        key,
        "path"
            | "cwd"
            | "fromPath"
            | "toPath"
            | "from_path"
            | "to_path"
            | "absolutePath"
            | "absolute_path"
    )
}

fn command_input_has_network_intent(input: &JsonValue) -> bool {
    let Some(argv) = input.get("argv").and_then(JsonValue::as_array) else {
        return string_values(input)
            .iter()
            .any(|value| looks_like_network(value));
    };

    let argv = argv
        .iter()
        .filter_map(JsonValue::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if argv.is_empty() {
        return false;
    }
    let program = Path::new(&argv[0])
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(&argv[0])
        .to_ascii_lowercase();
    matches!(
        program.as_str(),
        "curl"
            | "wget"
            | "nc"
            | "netcat"
            | "ssh"
            | "scp"
            | "sftp"
            | "ftp"
            | "ping"
            | "dig"
            | "nslookup"
    ) || argv.iter().any(|value| looks_like_network(value))
}

fn string_values(value: &JsonValue) -> Vec<String> {
    match value {
        JsonValue::String(text) => vec![text.clone()],
        JsonValue::Array(items) => items.iter().flat_map(string_values).collect(),
        JsonValue::Object(fields) => fields.values().flat_map(string_values).collect(),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => Vec::new(),
    }
}

fn looks_like_network(value: &str) -> bool {
    let normalized = value.to_ascii_lowercase();
    normalized.starts_with("http://")
        || normalized.starts_with("https://")
        || normalized.starts_with("ssh://")
        || normalized.contains(" curl ")
        || normalized.contains(" wget ")
}

fn deny(
    metadata: SandboxExecutionMetadata,
    code: impl Into<String>,
    reason: impl Into<String>,
) -> ToolSandboxResult {
    let reason = reason.into();
    Err(SandboxExecutionDenied {
        error: ToolExecutionError::sandbox_denied(code, reason.clone()),
        metadata: metadata.denied(reason),
    })
}

fn dedupe(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.clone()))
        .collect()
}

fn normalize_absolute(path: &str) -> String {
    path.trim()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_string()
}

fn path_starts_with(path: &str, root: &str) -> bool {
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|suffix| suffix.starts_with('/'))
}

fn escape_sandbox_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn descriptor(
        name: &str,
        effect_class: ToolEffectClass,
        mutability: ToolMutability,
        sandbox_requirement: ToolSandboxRequirement,
    ) -> ToolDescriptorV2 {
        ToolDescriptorV2 {
            name: name.into(),
            description: "Test descriptor.".into(),
            input_schema: json!({ "type": "object" }),
            capability_tags: Vec::new(),
            effect_class,
            mutability,
            sandbox_requirement,
            approval_requirement: crate::ToolApprovalRequirement::Policy,
            telemetry_attributes: BTreeMap::new(),
            result_truncation: crate::ToolResultTruncationContract::default(),
        }
    }

    fn call(input: JsonValue) -> ToolCallInput {
        ToolCallInput {
            tool_call_id: "call-1".into(),
            tool_name: "tool".into(),
            input,
        }
    }

    fn sandbox() -> PermissionProfileSandbox {
        PermissionProfileSandbox::new(SandboxExecutionContext {
            workspace_root: "/repo".into(),
            app_data_roots: vec!["/Users/example/Library/Application Support/Xero".into()],
            project_trust: ProjectTrustState::Trusted,
            approval_source: SandboxApprovalSource::Operator,
            platform: SandboxPlatform::Macos,
            explicit_git_mutation_allowed: false,
            legacy_xero_migration_allowed: false,
            preserved_environment_keys: vec!["PATH".into()],
        })
    }

    #[test]
    fn permission_profiles_define_network_and_write_modes() {
        assert_eq!(
            SandboxPermissionProfile::ReadOnly.network_mode(),
            SandboxNetworkMode::Denied
        );
        assert!(!SandboxPermissionProfile::ReadOnly.allows_workspace_write());
        assert_eq!(
            SandboxPermissionProfile::WorkspaceWriteNetworkAllowed.network_mode(),
            SandboxNetworkMode::Allowed
        );
        assert!(SandboxPermissionProfile::WorkspaceWrite.allows_workspace_write());
        assert!(SandboxPermissionProfile::FullLocalWithApproval.requires_approval());
    }

    #[test]
    fn sandbox_denies_workspace_write_escape() {
        let descriptor = descriptor(
            "write",
            ToolEffectClass::WorkspaceMutation,
            ToolMutability::Mutating,
            ToolSandboxRequirement::WorkspaceWrite,
        );

        let denied = sandbox()
            .evaluate(
                &descriptor,
                &call(json!({ "path": "../outside.txt" })),
                &ToolExecutionContext::default(),
            )
            .expect_err("workspace escape should fail at sandbox layer");

        assert_eq!(
            denied.error.category,
            crate::ToolErrorCategory::SandboxDenied
        );
        assert_eq!(denied.error.code, "agent_sandbox_path_denied");
        assert_eq!(
            denied.metadata.exit_classification,
            SandboxExitClassification::DeniedBySandbox
        );
    }

    #[test]
    fn sandbox_denies_network_command_under_network_denied_profile() {
        let descriptor = descriptor(
            "command",
            ToolEffectClass::CommandExecution,
            ToolMutability::Mutating,
            ToolSandboxRequirement::WorkspaceWrite,
        );

        let denied = sandbox()
            .evaluate(
                &descriptor,
                &call(json!({ "argv": ["curl", "https://example.com"] })),
                &ToolExecutionContext::default(),
            )
            .expect_err("network command should fail before spawn");

        assert_eq!(
            denied.error.category,
            crate::ToolErrorCategory::SandboxDenied
        );
        assert_eq!(denied.error.code, "agent_sandbox_network_denied");
        assert_eq!(denied.metadata.network_mode, SandboxNetworkMode::Denied);
    }

    #[test]
    fn sandbox_protects_git_legacy_xero_and_app_data_writes() {
        let descriptor = descriptor(
            "write",
            ToolEffectClass::WorkspaceMutation,
            ToolMutability::Mutating,
            ToolSandboxRequirement::WorkspaceWrite,
        );
        for (path, expected) in [
            (".git/config", ".git"),
            (".xero/state.json", ".xero/"),
            (
                "/Users/example/Library/Application Support/Xero/project.db",
                "app-data",
            ),
        ] {
            let denied = sandbox()
                .evaluate(
                    &descriptor,
                    &call(json!({ "path": path })),
                    &ToolExecutionContext::default(),
                )
                .expect_err("internal state path should be protected");
            assert!(denied.error.message.contains(expected));
        }
    }

    #[test]
    fn sandbox_denies_privileged_tools_for_untrusted_project() {
        let descriptor = descriptor(
            "command",
            ToolEffectClass::CommandExecution,
            ToolMutability::Mutating,
            ToolSandboxRequirement::WorkspaceWrite,
        );
        let sandbox = PermissionProfileSandbox::new(SandboxExecutionContext {
            project_trust: ProjectTrustState::Untrusted,
            ..SandboxExecutionContext::default()
        });

        let denied = sandbox
            .evaluate(
                &descriptor,
                &call(json!({ "argv": ["echo", "hello"] })),
                &ToolExecutionContext::default(),
            )
            .expect_err("untrusted project should not run command tools");

        assert_eq!(denied.error.code, "agent_sandbox_project_untrusted");
    }

    #[test]
    fn macos_plan_renders_sandbox_exec_profile_with_network_denied() {
        let descriptor = descriptor(
            "command",
            ToolEffectClass::CommandExecution,
            ToolMutability::Mutating,
            ToolSandboxRequirement::WorkspaceWrite,
        );

        let metadata = sandbox()
            .evaluate(
                &descriptor,
                &call(json!({ "argv": ["echo", "hello"] })),
                &ToolExecutionContext::default(),
            )
            .expect("command should be sandboxed");

        assert_eq!(
            metadata.platform_plan.strategy,
            SandboxPlatformStrategy::MacosSandboxExec
        );
        let profile = metadata.platform_plan.profile_text.expect("macOS profile");
        assert!(profile.contains("(deny network*)"));
        assert!(profile.contains("(deny file-write* (subpath \"/repo/.git\"))"));
        assert!(profile.contains("(allow file-write* (subpath \"/repo\"))"));
    }
}
