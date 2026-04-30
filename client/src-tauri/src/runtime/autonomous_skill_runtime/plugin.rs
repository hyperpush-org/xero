use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::commands::{CommandError, CommandResult};

use super::{
    cache::sha256_hex,
    contract::XeroSkillTrustState,
    inspection::{normalize_relative_source_path, normalize_skill_id},
};

pub const XERO_PLUGIN_MANIFEST_FILE: &str = "xero-plugin.json";
pub const XERO_PLUGIN_NESTED_MANIFEST_FILE: &str = ".xero-plugin/plugin.json";
pub const XERO_PLUGIN_MANIFEST_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroPluginTrustDeclaration {
    Trusted,
    ApprovalRequired,
    Untrusted,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroPluginEntryKind {
    Skill,
    Command,
    Asset,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroPluginCommandAvailability {
    Always,
    ProjectOpen,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroPluginCommandRiskLevel {
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
pub enum XeroPluginCommandApprovalPolicy {
    NeverForObserveOnly,
    Required,
    PerInvocation,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum XeroPluginCommandStatePolicy {
    Ephemeral,
    Project,
    Plugin,
    External,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroPluginSkillContribution {
    pub id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroPluginCommandContribution {
    pub id: String,
    pub label: String,
    pub description: String,
    pub entry: String,
    pub availability: XeroPluginCommandAvailability,
    #[serde(default = "default_plugin_command_risk_level")]
    pub risk_level: XeroPluginCommandRiskLevel,
    #[serde(default = "default_plugin_command_approval_policy")]
    pub approval_policy: XeroPluginCommandApprovalPolicy,
    #[serde(default = "default_plugin_command_state_policy")]
    pub state_policy: XeroPluginCommandStatePolicy,
    #[serde(default = "default_plugin_command_redaction_required")]
    pub redaction_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroPluginEntryLocation {
    pub kind: XeroPluginEntryKind,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct XeroPluginManifest {
    #[serde(default = "plugin_manifest_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub trust_declaration: XeroPluginTrustDeclaration,
    #[serde(default)]
    pub skills: Vec<XeroPluginSkillContribution>,
    #[serde(default)]
    pub commands: Vec<XeroPluginCommandContribution>,
    #[serde(default)]
    pub entry_locations: Vec<XeroPluginEntryLocation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroPluginRoot {
    pub root_id: String,
    pub root_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroDiscoveredPlugin {
    pub plugin_id: String,
    pub root_id: String,
    pub root_path: String,
    pub plugin_root_path: PathBuf,
    pub manifest_path: String,
    pub manifest_hash: String,
    pub manifest: XeroPluginManifest,
    pub trust: XeroSkillTrustState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroPluginDiscoveryDiagnostic {
    pub code: String,
    pub message: String,
    pub root_id: Option<String>,
    pub relative_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XeroPluginDiscovery {
    pub plugins: Vec<XeroDiscoveredPlugin>,
    pub diagnostics: Vec<XeroPluginDiscoveryDiagnostic>,
}

impl XeroPluginManifest {
    pub fn validate_for_root(
        self,
        plugin_root: impl AsRef<Path>,
    ) -> CommandResult<XeroPluginManifest> {
        if self.schema_version != XERO_PLUGIN_MANIFEST_SCHEMA_VERSION {
            return Err(CommandError::user_fixable(
                "xero_plugin_manifest_version_unsupported",
                format!(
                    "Xero rejected plugin manifest schema version `{}` because only version `{XERO_PLUGIN_MANIFEST_SCHEMA_VERSION}` is supported.",
                    self.schema_version
                ),
            ));
        }

        let plugin_root = canonicalize_plugin_root(plugin_root.as_ref())?;
        let id = normalize_plugin_id(&self.id)?;
        let name = normalize_required(self.name, "name")?;
        let version = normalize_plugin_version(&self.version)?;
        let description = normalize_required(self.description, "description")?;

        let mut skill_ids = BTreeSet::new();
        let mut skills = Vec::with_capacity(self.skills.len());
        for skill in self.skills {
            let id = normalize_skill_id(&skill.id)?;
            if !skill_ids.insert(id.clone()) {
                return Err(CommandError::user_fixable(
                    "xero_plugin_manifest_duplicate_id",
                    format!("Xero rejected plugin `{}` because skill contribution `{id}` was duplicated.", self.id),
                ));
            }
            let path = normalize_plugin_relative_path(&skill.path)?;
            ensure_plugin_path_stays_inside(&plugin_root, &path, true)?;
            skills.push(XeroPluginSkillContribution { id, path });
        }

        let mut command_ids = BTreeSet::new();
        let mut commands = Vec::with_capacity(self.commands.len());
        for command in self.commands {
            let id = normalize_plugin_contribution_id(&command.id)?;
            if !command_ids.insert(id.clone()) {
                return Err(CommandError::user_fixable(
                    "xero_plugin_manifest_duplicate_id",
                    format!("Xero rejected plugin `{}` because command contribution `{id}` was duplicated.", self.id),
                ));
            }
            let label = normalize_required(command.label, "command.label")?;
            let description = normalize_required(command.description, "command.description")?;
            let entry = normalize_plugin_relative_path(&command.entry)?;
            ensure_plugin_path_stays_inside(&plugin_root, &entry, false)?;
            validate_plugin_command_policy(
                &id,
                &command.risk_level,
                &command.approval_policy,
                command.redaction_required,
            )?;
            commands.push(XeroPluginCommandContribution {
                id,
                label,
                description,
                entry,
                availability: command.availability,
                risk_level: command.risk_level,
                approval_policy: command.approval_policy,
                state_policy: command.state_policy,
                redaction_required: command.redaction_required,
            });
        }

        let mut entry_locations = Vec::with_capacity(self.entry_locations.len());
        let mut seen_locations = BTreeSet::new();
        for location in self.entry_locations {
            let path = normalize_plugin_relative_path(&location.path)?;
            if !seen_locations.insert((format!("{:?}", location.kind), path.clone())) {
                return Err(CommandError::user_fixable(
                    "xero_plugin_manifest_duplicate_id",
                    format!("Xero rejected plugin `{id}` because entry location `{path}` was duplicated."),
                ));
            }
            ensure_plugin_path_stays_inside(
                &plugin_root,
                &path,
                matches!(location.kind, XeroPluginEntryKind::Skill),
            )?;
            entry_locations.push(XeroPluginEntryLocation {
                kind: location.kind,
                path,
            });
        }

        Ok(XeroPluginManifest {
            schema_version: XERO_PLUGIN_MANIFEST_SCHEMA_VERSION,
            id,
            name,
            version,
            description,
            trust_declaration: self.trust_declaration,
            skills,
            commands,
            entry_locations,
        })
    }
}

pub fn parse_plugin_manifest(
    bytes: &[u8],
    plugin_root: impl AsRef<Path>,
) -> CommandResult<XeroPluginManifest> {
    let manifest = serde_json::from_slice::<XeroPluginManifest>(bytes).map_err(|error| {
        CommandError::user_fixable(
            "xero_plugin_manifest_invalid",
            format!("Xero could not decode plugin manifest: {error}"),
        )
    })?;
    manifest.validate_for_root(plugin_root)
}

pub fn discover_plugin_roots(
    roots: impl IntoIterator<Item = XeroPluginRoot>,
) -> CommandResult<XeroPluginDiscovery> {
    let mut plugins = Vec::new();
    let mut diagnostics = Vec::new();
    let mut seen_plugin_ids = BTreeSet::new();

    for root in roots {
        let root_id = normalize_plugin_contribution_id(&root.root_id)?;
        let root_path = root.root_path;
        if !root_path.is_dir() {
            diagnostics.push(XeroPluginDiscoveryDiagnostic {
                code: "xero_plugin_root_unavailable".into(),
                message: format!(
                    "Xero could not scan plugin root {} because it is not available.",
                    root_path.display()
                ),
                root_id: Some(root_id),
                relative_path: None,
            });
            continue;
        }
        let root_canonical = match fs::canonicalize(&root_path) {
            Ok(path) => path,
            Err(error) => {
                diagnostics.push(XeroPluginDiscoveryDiagnostic {
                    code: "xero_plugin_root_unavailable".into(),
                    message: format!(
                        "Xero could not resolve plugin root {}: {error}",
                        root_path.display()
                    ),
                    root_id: Some(root_id),
                    relative_path: None,
                });
                continue;
            }
        };

        let manifest_paths =
            collect_plugin_manifest_paths(&root_id, &root_canonical, &mut diagnostics)?;
        for manifest_path in manifest_paths {
            match inspect_plugin_manifest(&root_id, &root_canonical, &manifest_path) {
                Ok(plugin) => {
                    if !seen_plugin_ids.insert(plugin.plugin_id.clone()) {
                        diagnostics.push(XeroPluginDiscoveryDiagnostic {
                            code: "xero_plugin_duplicate_id".into(),
                            message: format!(
                                "Xero skipped duplicate plugin id `{}` from {}.",
                                plugin.plugin_id,
                                manifest_path.display()
                            ),
                            root_id: Some(root_id.clone()),
                            relative_path: relative_path(&root_canonical, &manifest_path).ok(),
                        });
                        continue;
                    }
                    plugins.push(plugin);
                }
                Err(error) => diagnostics.push(XeroPluginDiscoveryDiagnostic {
                    code: error.code,
                    message: error.message,
                    root_id: Some(root_id.clone()),
                    relative_path: relative_path(&root_canonical, &manifest_path).ok(),
                }),
            }
        }
    }

    plugins.sort_by(|left, right| left.plugin_id.cmp(&right.plugin_id));
    Ok(XeroPluginDiscovery {
        plugins,
        diagnostics,
    })
}

pub fn normalize_plugin_id(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("pluginId"));
    }
    if trimmed.starts_with('.') || trimmed.ends_with('.') || trimmed.contains("..") {
        return Err(CommandError::user_fixable(
            "xero_plugin_id_invalid",
            "Xero requires plugin ids to use stable lowercase segments separated by dots.",
        ));
    }
    if !trimmed.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || matches!(character, '.' | '-' | '_')
    }) {
        return Err(CommandError::user_fixable(
            "xero_plugin_id_invalid",
            "Xero requires plugin ids to be lowercase ASCII values.",
        ));
    }
    Ok(trimmed.to_owned())
}

pub fn normalize_plugin_contribution_id(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request("contributionId"));
    }
    if !trimmed.chars().all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
    }) {
        return Err(CommandError::user_fixable(
            "xero_plugin_contribution_id_invalid",
            "Xero requires plugin contribution ids to be lowercase kebab-case values.",
        ));
    }
    Ok(trimmed.to_owned())
}

pub fn plugin_trust_declaration_to_skill_trust(
    trust: &XeroPluginTrustDeclaration,
) -> XeroSkillTrustState {
    match trust {
        XeroPluginTrustDeclaration::Trusted => XeroSkillTrustState::Trusted,
        XeroPluginTrustDeclaration::ApprovalRequired => XeroSkillTrustState::ApprovalRequired,
        XeroPluginTrustDeclaration::Untrusted => XeroSkillTrustState::Untrusted,
        XeroPluginTrustDeclaration::Blocked => XeroSkillTrustState::Blocked,
    }
}

pub fn plugin_command_stable_id(plugin_id: &str, contribution_id: &str) -> CommandResult<String> {
    Ok(format!(
        "plugin:{}:command:{}",
        normalize_plugin_id(plugin_id)?,
        normalize_plugin_contribution_id(contribution_id)?
    ))
}

fn inspect_plugin_manifest(
    root_id: &str,
    root_canonical: &Path,
    manifest_path: &Path,
) -> CommandResult<XeroDiscoveredPlugin> {
    let plugin_root_path = manifest_path
        .parent()
        .and_then(|parent| {
            if parent.file_name().and_then(|name| name.to_str()) == Some(".xero-plugin") {
                parent.parent()
            } else {
                Some(parent)
            }
        })
        .ok_or_else(|| {
            CommandError::user_fixable(
                "xero_plugin_manifest_invalid",
                "Xero could not resolve the plugin root for a discovered manifest.",
            )
        })?;
    let plugin_root_path = fs::canonicalize(plugin_root_path).map_err(|error| {
        CommandError::retryable(
            "xero_plugin_root_unavailable",
            format!("Xero could not resolve plugin root: {error}"),
        )
    })?;
    if !plugin_root_path.starts_with(root_canonical) {
        return Err(CommandError::user_fixable(
            "xero_plugin_path_outside_root",
            "Xero rejected a plugin manifest because it resolves outside the configured plugin root.",
        ));
    }
    let manifest_bytes = fs::read(manifest_path).map_err(|error| {
        CommandError::retryable(
            "xero_plugin_manifest_read_failed",
            format!(
                "Xero could not read plugin manifest {}: {error}",
                manifest_path.display()
            ),
        )
    })?;
    let manifest = parse_plugin_manifest(&manifest_bytes, &plugin_root_path)?;
    let plugin_id = manifest.id.clone();
    Ok(XeroDiscoveredPlugin {
        plugin_id,
        root_id: root_id.to_owned(),
        root_path: root_canonical.display().to_string(),
        plugin_root_path,
        manifest_path: manifest_path.display().to_string(),
        manifest_hash: sha256_hex(&manifest_bytes),
        trust: plugin_trust_declaration_to_skill_trust(&manifest.trust_declaration),
        manifest,
    })
}

fn collect_plugin_manifest_paths(
    root_id: &str,
    root_canonical: &Path,
    diagnostics: &mut Vec<XeroPluginDiscoveryDiagnostic>,
) -> CommandResult<Vec<PathBuf>> {
    let mut paths = Vec::new();
    push_manifest_if_present(root_canonical, &mut paths);

    let mut entries = fs::read_dir(root_canonical)
        .map_err(|error| {
            CommandError::retryable(
                "xero_plugin_root_read_failed",
                format!(
                    "Xero could not enumerate plugin root {}: {error}",
                    root_canonical.display()
                ),
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            CommandError::retryable(
                "xero_plugin_root_read_failed",
                format!(
                    "Xero could not inspect an entry under plugin root {}: {error}",
                    root_canonical.display()
                ),
            )
        })?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            CommandError::retryable(
                "xero_plugin_root_read_failed",
                format!(
                    "Xero could not inspect plugin path {}: {error}",
                    path.display()
                ),
            )
        })?;
        if metadata.file_type().is_symlink() {
            diagnostics.push(XeroPluginDiscoveryDiagnostic {
                code: "xero_plugin_path_outside_root".into(),
                message: format!(
                    "Xero skipped {} because plugin scanning does not follow symlinks.",
                    path.display()
                ),
                root_id: Some(root_id.to_owned()),
                relative_path: relative_path(root_canonical, &path).ok(),
            });
            continue;
        }
        if metadata.is_dir() {
            let canonical = fs::canonicalize(&path).map_err(|error| {
                CommandError::retryable(
                    "xero_plugin_root_unavailable",
                    format!(
                        "Xero could not resolve plugin path {}: {error}",
                        path.display()
                    ),
                )
            })?;
            if !canonical.starts_with(root_canonical) {
                diagnostics.push(XeroPluginDiscoveryDiagnostic {
                    code: "xero_plugin_path_outside_root".into(),
                    message: format!(
                        "Xero skipped {} because it resolves outside the configured plugin root.",
                        path.display()
                    ),
                    root_id: Some(root_id.to_owned()),
                    relative_path: relative_path(root_canonical, &path).ok(),
                });
                continue;
            }
            push_manifest_if_present(&canonical, &mut paths);
        }
    }
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn push_manifest_if_present(plugin_root: &Path, paths: &mut Vec<PathBuf>) {
    let primary = plugin_root.join(XERO_PLUGIN_MANIFEST_FILE);
    if primary.is_file() {
        paths.push(primary);
        return;
    }
    let nested = plugin_root.join(XERO_PLUGIN_NESTED_MANIFEST_FILE);
    if nested.is_file() {
        paths.push(nested);
    }
}

fn canonicalize_plugin_root(root: &Path) -> CommandResult<PathBuf> {
    fs::canonicalize(root).map_err(|error| {
        CommandError::retryable(
            "xero_plugin_root_unavailable",
            format!(
                "Xero could not resolve plugin root {}: {error}",
                root.display()
            ),
        )
    })
}

fn ensure_plugin_path_stays_inside(
    plugin_root: &Path,
    relative: &str,
    expected_directory: bool,
) -> CommandResult<PathBuf> {
    let path = plugin_root.join(relative);
    let canonical = fs::canonicalize(&path).map_err(|error| {
        CommandError::user_fixable(
            "xero_plugin_entry_unavailable",
            format!("Xero could not resolve plugin entry `{relative}`: {error}"),
        )
    })?;
    if !canonical.starts_with(plugin_root) {
        return Err(CommandError::user_fixable(
            "xero_plugin_path_outside_root",
            format!("Xero rejected plugin entry `{relative}` because it resolves outside the plugin root."),
        ));
    }
    if expected_directory && !canonical.is_dir() {
        return Err(CommandError::user_fixable(
            "xero_plugin_entry_unavailable",
            format!("Xero expected plugin entry `{relative}` to be a directory."),
        ));
    }
    if !expected_directory && !canonical.is_file() {
        return Err(CommandError::user_fixable(
            "xero_plugin_entry_unavailable",
            format!("Xero expected plugin entry `{relative}` to be a file."),
        ));
    }
    Ok(canonical)
}

fn normalize_plugin_relative_path(value: &str) -> CommandResult<String> {
    normalize_relative_source_path(value).map_err(|error| {
        if error.code == "autonomous_skill_source_metadata_invalid" {
            CommandError::user_fixable(
                "xero_plugin_path_outside_root",
                "Xero requires plugin contribution paths to remain relative to the plugin root.",
            )
        } else {
            error
        }
    })
}

fn normalize_plugin_version(value: &str) -> CommandResult<String> {
    let trimmed = normalize_required(value.to_owned(), "version")?;
    let without_prerelease = trimmed
        .split_once('-')
        .map(|(core, _)| core)
        .unwrap_or(trimmed.as_str());
    let core = without_prerelease
        .split_once('+')
        .map(|(core, _)| core)
        .unwrap_or(without_prerelease);
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3
        || parts.iter().any(|part| {
            part.is_empty() || !part.chars().all(|character| character.is_ascii_digit())
        })
    {
        return Err(CommandError::user_fixable(
            "xero_plugin_version_invalid",
            "Xero requires plugin versions to use semantic `major.minor.patch` format.",
        ));
    }
    Ok(trimmed)
}

fn validate_plugin_command_policy(
    command_id: &str,
    risk_level: &XeroPluginCommandRiskLevel,
    approval_policy: &XeroPluginCommandApprovalPolicy,
    redaction_required: bool,
) -> CommandResult<()> {
    let observe_only = matches!(risk_level, XeroPluginCommandRiskLevel::Observe);
    if matches!(
        approval_policy,
        XeroPluginCommandApprovalPolicy::NeverForObserveOnly
    ) && !observe_only
    {
        return Err(CommandError::user_fixable(
            "xero_plugin_command_policy_invalid",
            format!(
                "Xero rejected plugin command `{command_id}` because only observe-risk commands may use never_for_observe_only approval."
            ),
        ));
    }

    if !redaction_required && !observe_only {
        return Err(CommandError::user_fixable(
            "xero_plugin_command_policy_invalid",
            format!(
                "Xero rejected plugin command `{command_id}` because non-observe extension commands must require output redaction before persistence."
            ),
        ));
    }

    Ok(())
}

const fn default_plugin_command_risk_level() -> XeroPluginCommandRiskLevel {
    XeroPluginCommandRiskLevel::Observe
}

const fn default_plugin_command_approval_policy() -> XeroPluginCommandApprovalPolicy {
    XeroPluginCommandApprovalPolicy::Required
}

const fn default_plugin_command_state_policy() -> XeroPluginCommandStatePolicy {
    XeroPluginCommandStatePolicy::Ephemeral
}

const fn default_plugin_command_redaction_required() -> bool {
    true
}

fn relative_path(root: &Path, path: &Path) -> CommandResult<String> {
    let relative = path.strip_prefix(root).map_err(|_| {
        CommandError::user_fixable(
            "xero_plugin_path_outside_root",
            "Xero rejected a plugin path outside the configured plugin root.",
        )
    })?;
    normalize_relative_source_path(&relative.to_string_lossy().replace('\\', "/"))
}

fn normalize_required(value: String, field: &'static str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(trimmed.to_owned())
}

const fn plugin_manifest_schema_version() -> u32 {
    XERO_PLUGIN_MANIFEST_SCHEMA_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plugin_command_policy_metadata_survives_manifest_validation() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(tempdir.path().join("commands")).expect("commands dir");
        fs::write(tempdir.path().join("commands/run.js"), "export default {}")
            .expect("command file");

        let manifest = parse_plugin_manifest(
            br#"{
                "id": "com.acme.tools",
                "name": "Acme Tools",
                "version": "1.0.0",
                "description": "Test plugin.",
                "trustDeclaration": "trusted",
                "commands": [
                    {
                        "id": "run-task",
                        "label": "Run Task",
                        "description": "Runs a task.",
                        "entry": "commands/run.js",
                        "availability": "project_open",
                        "riskLevel": "network",
                        "approvalPolicy": "per_invocation",
                        "statePolicy": "plugin",
                        "redactionRequired": true
                    }
                ]
            }"#,
            tempdir.path(),
        )
        .expect("valid manifest");

        let command = manifest.commands.first().expect("command");
        assert_eq!(command.risk_level, XeroPluginCommandRiskLevel::Network);
        assert_eq!(
            command.approval_policy,
            XeroPluginCommandApprovalPolicy::PerInvocation
        );
        assert_eq!(command.state_policy, XeroPluginCommandStatePolicy::Plugin);
        assert!(command.redaction_required);
    }

    #[test]
    fn plugin_command_policy_rejects_non_observe_without_approval() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(tempdir.path().join("commands")).expect("commands dir");
        fs::write(tempdir.path().join("commands/run.js"), "export default {}")
            .expect("command file");

        let error = parse_plugin_manifest(
            br#"{
                "id": "com.acme.tools",
                "name": "Acme Tools",
                "version": "1.0.0",
                "description": "Test plugin.",
                "trustDeclaration": "trusted",
                "commands": [
                    {
                        "id": "run-task",
                        "label": "Run Task",
                        "description": "Runs a task.",
                        "entry": "commands/run.js",
                        "availability": "project_open",
                        "riskLevel": "network",
                        "approvalPolicy": "never_for_observe_only",
                        "statePolicy": "ephemeral",
                        "redactionRequired": true
                    }
                ]
            }"#,
            tempdir.path(),
        )
        .expect_err("risky command without approval should fail");

        assert_eq!(error.code, "xero_plugin_command_policy_invalid");
    }
}
