use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use xero_agent_core::{
    PermissionProfileSandbox, SandboxApprovalSource, SandboxExecutionContext,
    SandboxExecutionMetadata, SandboxedProcessRequest, SandboxedProcessRunner, ToolCallInput,
    ToolExecutionContext, ToolExecutionControl, ToolExecutionError, ToolExtensionFixtureReport,
    ToolExtensionFixtureRun, ToolExtensionFixtureStatus, ToolExtensionManifest, ToolHandlerOutput,
    ToolMutability, ToolRegistryResult, ToolSandbox, ToolSandboxRequirement,
};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
};

pub const TOOL_EXTENSION_DIRECTORY_NAME: &str = "tool-extensions";
pub const TOOL_EXTENSION_MANIFEST_FILE: &str = "manifest.json";
const TOOL_EXTENSION_STATE_FILE: &str = "installation.json";
const TOOL_EXTENSION_STATE_SCHEMA: &str = "xero.tool_extension_installation.v1";
const TOOL_EXTENSION_CATALOG_SCHEMA: &str = "xero.agent_tool_extension_catalog.v1";
const TOOL_EXTENSION_PROCESS_CONTRACT_VERSION: u32 = 1;
const MAX_EXTENSION_MANIFEST_BYTES: u64 = 256 * 1024;
const EXTENSION_REQUEST_LIMIT_BYTES: usize = 1024 * 1024;
const EXTENSION_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;
const EXTENSION_FIXTURE_TIMEOUT_MS: u64 = if cfg!(test) { 2_000 } else { 5_000 };

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ToolExtensionInstallationState {
    schema: String,
    installation_hash: String,
    enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    granted_permission_id: Option<String>,
    installed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedToolExtension {
    pub manifest: ToolExtensionManifest,
    pub bundle_dir: PathBuf,
    pub installation_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionPermissionSummary {
    pub permission_id: String,
    pub label: String,
    pub effect_class: xero_agent_core::ToolEffectClass,
    pub risk_class: String,
    pub audit_label: String,
    pub mutability: ToolMutability,
    pub sandbox_requirement: ToolSandboxRequirement,
    pub approval_requirement: xero_agent_core::ToolApprovalRequirement,
    pub capability_tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionCatalogEntry {
    pub extension_id: String,
    pub label: String,
    pub tool_name: String,
    pub enabled: bool,
    pub eligible: bool,
    pub installation_hash: String,
    pub permission: ToolExtensionPermissionSummary,
    pub diagnostics: Vec<ToolExtensionDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionCatalog {
    pub schema: String,
    pub app_data_directory: String,
    pub extensions: Vec<ToolExtensionCatalogEntry>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolExtensionProcessRequest<'a> {
    contract_version: u32,
    extension_id: &'a str,
    tool_name: &'a str,
    tool_call_id: &'a str,
    context: &'a ToolExecutionContext,
    input: &'a JsonValue,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ToolExtensionProcessResponse {
    summary: String,
    output: JsonValue,
}

pub fn tool_extension_root(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join(TOOL_EXTENSION_DIRECTORY_NAME)
}

pub fn list_tool_extensions(
    app_data_dir: &Path,
    reserved_tool_names: &BTreeSet<String>,
) -> CommandResult<ToolExtensionCatalog> {
    let root = tool_extension_root(app_data_dir);
    let mut entries = read_installed_extensions(&root)?;
    apply_collision_diagnostics(&mut entries, reserved_tool_names);
    entries.sort_by(|left, right| left.extension_id.cmp(&right.extension_id));
    Ok(ToolExtensionCatalog {
        schema: TOOL_EXTENSION_CATALOG_SCHEMA.into(),
        app_data_directory: root.to_string_lossy().into_owned(),
        extensions: entries,
    })
}

pub fn install_tool_extension(
    app_data_dir: &Path,
    source_directory: &Path,
    reserved_tool_names: &BTreeSet<String>,
) -> CommandResult<ToolExtensionCatalog> {
    let source_directory = canonical_directory(source_directory)?;
    let root = tool_extension_root(app_data_dir);
    if source_directory.starts_with(&root) {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_source_managed",
            "Choose an extension bundle outside Xero's managed app-data directory.",
        ));
    }
    let loaded = load_bundle(&source_directory)?;
    validate_production_manifest(&loaded.manifest, reserved_tool_names)?;
    verify_extension_fixtures(&loaded)?;

    fs::create_dir_all(&root).map_err(|error| extension_io_error("create", &root, error))?;
    let stage = root.join(format!(
        ".install-{}-{}",
        std::process::id(),
        &loaded.installation_hash[..16]
    ));
    if stage.exists() {
        fs::remove_dir_all(&stage).map_err(|error| extension_io_error("reset", &stage, error))?;
    }
    fs::create_dir(&stage).map_err(|error| extension_io_error("create", &stage, error))?;
    copy_bundle_file(
        &source_directory.join(TOOL_EXTENSION_MANIFEST_FILE),
        &stage.join(TOOL_EXTENSION_MANIFEST_FILE),
    )?;
    copy_bundle_file(
        &source_directory.join(&loaded.manifest.runtime.executable),
        &stage.join(&loaded.manifest.runtime.executable),
    )?;
    let state = ToolExtensionInstallationState {
        schema: TOOL_EXTENSION_STATE_SCHEMA.into(),
        installation_hash: loaded.installation_hash.clone(),
        enabled: false,
        granted_permission_id: None,
        installed_at: now_timestamp(),
    };
    write_installation_state(&stage, &state)?;
    harden_installed_bundle(&root, &stage, &loaded.manifest.runtime.executable)?;

    let target = root.join(&loaded.manifest.extension_id);
    let backup = root.join(format!(".replace-{}", &loaded.manifest.extension_id));
    if backup.exists() {
        fs::remove_dir_all(&backup).map_err(|error| extension_io_error("reset", &backup, error))?;
    }
    if target.exists() {
        fs::rename(&target, &backup)
            .map_err(|error| extension_io_error("stage upgrade for", &target, error))?;
    }
    if let Err(error) = fs::rename(&stage, &target) {
        if backup.exists() {
            let _ = fs::rename(&backup, &target);
        }
        return Err(extension_io_error("install", &target, error));
    }
    if backup.exists() {
        fs::remove_dir_all(&backup)
            .map_err(|error| extension_io_error("finish upgrade for", &backup, error))?;
    }

    list_tool_extensions(app_data_dir, reserved_tool_names)
}

pub fn set_tool_extension_enabled(
    app_data_dir: &Path,
    extension_id: &str,
    enabled: bool,
    permission_id: Option<&str>,
    reserved_tool_names: &BTreeSet<String>,
) -> CommandResult<ToolExtensionCatalog> {
    validate_storage_identifier(extension_id)?;
    let bundle_dir = tool_extension_root(app_data_dir).join(extension_id);
    let loaded = load_bundle(&bundle_dir)?;
    if loaded.manifest.extension_id != extension_id {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_identity_mismatch",
            format!(
                "Installed extension directory `{extension_id}` contains manifest id `{}`.",
                loaded.manifest.extension_id
            ),
        ));
    }
    validate_production_manifest(&loaded.manifest, reserved_tool_names)?;
    let mut state = read_installation_state(&bundle_dir)?;
    if state.installation_hash != loaded.installation_hash {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_integrity_failed",
            format!(
                "Tool extension `{extension_id}` changed after verification. Reinstall it before enabling."
            ),
        ));
    }
    if enabled {
        let expected = loaded.manifest.permission.permission_id.as_str();
        if permission_id != Some(expected) {
            return Err(CommandError::user_fixable(
                "agent_tool_extension_permission_not_granted",
                format!(
                    "Enabling `{extension_id}` requires an explicit grant for permission `{expected}`."
                ),
            ));
        }
        verify_extension_fixtures(&loaded)?;
        state.granted_permission_id = Some(expected.into());
    } else {
        state.granted_permission_id = None;
    }
    state.enabled = enabled;
    write_installation_state(&bundle_dir, &state)?;

    let catalog = list_tool_extensions(app_data_dir, reserved_tool_names)?;
    if enabled {
        let entry = catalog
            .extensions
            .iter()
            .find(|entry| entry.extension_id == extension_id)
            .ok_or_else(|| {
                CommandError::system_fault(
                    "agent_tool_extension_enable_missing",
                    format!("Enabled extension `{extension_id}` disappeared during reload."),
                )
            })?;
        if !entry.eligible {
            state.enabled = false;
            state.granted_permission_id = None;
            write_installation_state(&bundle_dir, &state)?;
            let diagnostic = entry
                .diagnostics
                .first()
                .map(|diagnostic| diagnostic.message.clone())
                .unwrap_or_else(|| "The extension is not eligible for registration.".into());
            return Err(CommandError::user_fixable(
                "agent_tool_extension_enable_rejected",
                diagnostic,
            ));
        }
    }
    Ok(catalog)
}

pub fn remove_tool_extension(
    app_data_dir: &Path,
    extension_id: &str,
    reserved_tool_names: &BTreeSet<String>,
) -> CommandResult<ToolExtensionCatalog> {
    validate_storage_identifier(extension_id)?;
    let bundle_dir = tool_extension_root(app_data_dir).join(extension_id);
    if bundle_dir.exists() {
        fs::remove_dir_all(&bundle_dir)
            .map_err(|error| extension_io_error("remove", &bundle_dir, error))?;
    }
    list_tool_extensions(app_data_dir, reserved_tool_names)
}

pub fn load_enabled_tool_extensions(
    app_data_dir: &Path,
    reserved_tool_names: &BTreeSet<String>,
) -> CommandResult<Vec<LoadedToolExtension>> {
    let catalog = list_tool_extensions(app_data_dir, reserved_tool_names)?;
    let root = tool_extension_root(app_data_dir);
    let mut loaded = Vec::new();
    for entry in catalog
        .extensions
        .iter()
        .filter(|entry| entry.enabled && entry.eligible)
    {
        loaded.push(load_bundle(&root.join(&entry.extension_id))?);
    }
    Ok(loaded)
}

pub fn execute_tool_extension(
    extension: &LoadedToolExtension,
    context: &ToolExecutionContext,
    call: &ToolCallInput,
    control: &ToolExecutionControl,
    sandbox_metadata: SandboxExecutionMetadata,
) -> ToolRegistryResult<ToolHandlerOutput> {
    control.ensure_not_cancelled(&call.tool_name)?;
    let timeout_ms = control
        .remaining()
        .unwrap_or_else(|| Duration::from_millis(EXTENSION_FIXTURE_TIMEOUT_MS))
        .as_millis()
        .clamp(1, u128::from(u64::MAX)) as u64;
    run_extension_process(
        extension,
        context,
        call,
        timeout_ms,
        sandbox_metadata,
        true,
        || control.is_cancelled(),
    )
}

fn read_installed_extensions(root: &Path) -> CommandResult<Vec<ToolExtensionCatalogEntry>> {
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut directories = Vec::new();
    for entry in fs::read_dir(root).map_err(|error| extension_io_error("read", root, error))? {
        let entry = entry.map_err(|error| extension_io_error("read", root, error))?;
        let file_type = entry
            .file_type()
            .map_err(|error| extension_io_error("inspect", &entry.path(), error))?;
        if file_type.is_dir() && !entry.file_name().to_string_lossy().starts_with('.') {
            directories.push(entry.path());
        }
    }
    directories.sort();

    let mut entries = Vec::new();
    for directory in directories {
        let loaded = match load_bundle(&directory) {
            Ok(loaded) => loaded,
            Err(error) => {
                let extension_id = directory
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("invalid-extension")
                    .to_owned();
                entries.push(invalid_catalog_entry(extension_id, error));
                continue;
            }
        };
        let mut diagnostics = Vec::new();
        if let Err(error) = validate_production_manifest(&loaded.manifest, &BTreeSet::new()) {
            diagnostics.push(diagnostic_from_command_error(error));
        }
        let state = match read_installation_state(&directory) {
            Ok(state) => state,
            Err(error) => {
                diagnostics.push(diagnostic_from_command_error(error));
                ToolExtensionInstallationState {
                    schema: TOOL_EXTENSION_STATE_SCHEMA.into(),
                    installation_hash: String::new(),
                    enabled: false,
                    granted_permission_id: None,
                    installed_at: String::new(),
                }
            }
        };
        if directory.file_name().and_then(|name| name.to_str())
            != Some(loaded.manifest.extension_id.as_str())
        {
            diagnostics.push(ToolExtensionDiagnostic {
                code: "agent_tool_extension_identity_mismatch".into(),
                message: format!(
                    "Managed directory name does not match extension id `{}`.",
                    loaded.manifest.extension_id
                ),
            });
        }
        if state.installation_hash != loaded.installation_hash {
            diagnostics.push(ToolExtensionDiagnostic {
                code: "agent_tool_extension_integrity_failed".into(),
                message: format!(
                    "Tool extension `{}` changed after fixture verification and must be reinstalled.",
                    loaded.manifest.extension_id
                ),
            });
        }
        if state.enabled
            && state.granted_permission_id.as_deref()
                != Some(loaded.manifest.permission.permission_id.as_str())
        {
            diagnostics.push(ToolExtensionDiagnostic {
                code: "agent_tool_extension_permission_not_granted".into(),
                message: format!(
                    "Tool extension `{}` is not registered because its declared permission is not granted.",
                    loaded.manifest.extension_id
                ),
            });
        }
        let eligible = diagnostics.is_empty();
        entries.push(ToolExtensionCatalogEntry {
            extension_id: loaded.manifest.extension_id.clone(),
            label: loaded.manifest.label.clone(),
            tool_name: loaded.manifest.tool_name.clone(),
            enabled: state.enabled,
            eligible,
            installation_hash: loaded.installation_hash,
            permission: permission_summary(&loaded.manifest),
            diagnostics,
        });
    }
    Ok(entries)
}

fn apply_collision_diagnostics(
    entries: &mut [ToolExtensionCatalogEntry],
    reserved_tool_names: &BTreeSet<String>,
) {
    let mut tool_owners = BTreeMap::<String, Vec<usize>>::new();
    let mut permission_owners = BTreeMap::<String, Vec<usize>>::new();
    for (index, entry) in entries.iter().enumerate() {
        tool_owners
            .entry(entry.tool_name.clone())
            .or_default()
            .push(index);
        permission_owners
            .entry(entry.permission.permission_id.clone())
            .or_default()
            .push(index);
    }
    for (tool_name, owners) in tool_owners {
        if owners.len() > 1 || reserved_tool_names.contains(&tool_name) {
            for index in owners {
                entries[index].diagnostics.push(ToolExtensionDiagnostic {
                    code: "agent_tool_extension_name_collision".into(),
                    message: format!(
                        "Tool name `{tool_name}` collides with another registered capability. Choose a unique tool name."
                    ),
                });
                entries[index].eligible = false;
            }
        }
    }
    for (permission_id, owners) in permission_owners {
        if owners.len() > 1 {
            for index in owners {
                entries[index].diagnostics.push(ToolExtensionDiagnostic {
                    code: "agent_tool_extension_capability_collision".into(),
                    message: format!(
                        "Permission capability `{permission_id}` is declared by more than one extension. Choose a unique permission id."
                    ),
                });
                entries[index].eligible = false;
            }
        }
    }
}

fn load_bundle(bundle_dir: &Path) -> CommandResult<LoadedToolExtension> {
    let manifest_path = bundle_dir.join(TOOL_EXTENSION_MANIFEST_FILE);
    let metadata = fs::symlink_metadata(&manifest_path)
        .map_err(|error| extension_io_error("read", &manifest_path, error))?;
    if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_manifest_file_invalid",
            format!(
                "Tool extension manifest `{}` must be a regular file, not a symlink.",
                manifest_path.display()
            ),
        ));
    }
    if metadata.len() > MAX_EXTENSION_MANIFEST_BYTES {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_manifest_too_large",
            format!(
                "Tool extension manifest `{}` exceeds {MAX_EXTENSION_MANIFEST_BYTES} bytes.",
                manifest_path.display()
            ),
        ));
    }
    let bytes = fs::read(&manifest_path)
        .map_err(|error| extension_io_error("read", &manifest_path, error))?;
    let manifest = serde_json::from_slice::<ToolExtensionManifest>(&bytes).map_err(|error| {
        CommandError::user_fixable(
            "agent_tool_extension_manifest_malformed",
            format!(
                "Tool extension manifest `{}` is malformed: {error}",
                manifest_path.display()
            ),
        )
    })?;
    manifest.validate().map_err(core_error_to_command_error)?;
    let executable = bundle_dir.join(&manifest.runtime.executable);
    let executable_metadata = fs::symlink_metadata(&executable)
        .map_err(|error| extension_io_error("read", &executable, error))?;
    if !executable_metadata.file_type().is_file() || executable_metadata.file_type().is_symlink() {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_executable_invalid",
            format!(
                "Tool extension executable `{}` must be a regular file, not a symlink.",
                executable.display()
            ),
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if executable_metadata.permissions().mode() & 0o111 == 0 {
            return Err(CommandError::user_fixable(
                "agent_tool_extension_executable_not_runnable",
                format!(
                    "Tool extension executable `{}` is not marked executable.",
                    executable.display()
                ),
            ));
        }
    }
    let installation_hash = installation_hash(&manifest, &executable)?;
    Ok(LoadedToolExtension {
        manifest,
        bundle_dir: bundle_dir.to_path_buf(),
        installation_hash,
    })
}

fn validate_production_manifest(
    manifest: &ToolExtensionManifest,
    reserved_tool_names: &BTreeSet<String>,
) -> CommandResult<()> {
    manifest.validate().map_err(core_error_to_command_error)?;
    if manifest.mutability != ToolMutability::ReadOnly {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_mutating_isolation_unavailable",
            format!(
                "Tool extension `{}` is mutating. Xero refuses to register third-party mutations until rollback-complete isolation is available.",
                manifest.extension_id
            ),
        ));
    }
    if manifest.sandbox_requirement != ToolSandboxRequirement::ReadOnly {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_sandbox_invalid",
            format!(
                "Read-only tool extension `{}` must use the read_only sandbox profile.",
                manifest.extension_id
            ),
        ));
    }
    if reserved_tool_names.contains(&manifest.tool_name) {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_name_collision",
            format!(
                "Tool extension `{}` cannot use reserved tool name `{}`.",
                manifest.extension_id, manifest.tool_name
            ),
        ));
    }
    Ok(())
}

fn verify_extension_fixtures(
    extension: &LoadedToolExtension,
) -> CommandResult<ToolExtensionFixtureReport> {
    let descriptor = extension.manifest.descriptor();
    let context = ToolExecutionContext {
        project_id: "extension-install".into(),
        run_id: format!("verify:{}", extension.manifest.extension_id),
        turn_index: 0,
        context_epoch: extension.installation_hash.clone(),
        telemetry_attributes: BTreeMap::new(),
    };
    let sandbox = PermissionProfileSandbox::new(SandboxExecutionContext {
        workspace_root: extension.bundle_dir.to_string_lossy().into_owned(),
        app_data_roots: vec![extension.bundle_dir.to_string_lossy().into_owned()],
        approval_source: SandboxApprovalSource::Policy,
        preserved_environment_keys: vec!["PATH".into(), "HOME".into(), "TMPDIR".into()],
        ..SandboxExecutionContext::default()
    });
    let mut fixtures = Vec::new();
    for fixture in &extension.manifest.test_fixtures {
        let call = ToolCallInput {
            tool_call_id: format!("fixture:{}", fixture.fixture_id),
            tool_name: extension.manifest.tool_name.clone(),
            input: fixture.input.clone(),
        };
        let metadata = sandbox
            .evaluate(&descriptor, &call, &context)
            .map_err(|denied| core_error_to_command_error(denied.error))?;
        let result = run_extension_process(
            extension,
            &context,
            &call,
            EXTENSION_FIXTURE_TIMEOUT_MS,
            metadata,
            false,
            || false,
        );
        let fixture_run = match result {
            Ok(output) => {
                let expected_matches = fixture
                    .expected_summary_contains
                    .as_deref()
                    .map(|expected| output.summary.contains(expected))
                    .unwrap_or(true);
                if expected_matches {
                    ToolExtensionFixtureRun {
                        fixture_id: fixture.fixture_id.clone(),
                        status: ToolExtensionFixtureStatus::Passed,
                        summary: Some(output.summary),
                        diagnostic: None,
                    }
                } else {
                    ToolExtensionFixtureRun {
                        fixture_id: fixture.fixture_id.clone(),
                        status: ToolExtensionFixtureStatus::Failed,
                        summary: Some(output.summary),
                        diagnostic: Some(
                            "Fixture summary did not contain the required text.".into(),
                        ),
                    }
                }
            }
            Err(error) => ToolExtensionFixtureRun {
                fixture_id: fixture.fixture_id.clone(),
                status: ToolExtensionFixtureStatus::Failed,
                summary: None,
                diagnostic: Some(format!("{}: {}", error.code, error.message)),
            },
        };
        fixtures.push(fixture_run);
    }
    let report = ToolExtensionFixtureReport {
        extension_id: extension.manifest.extension_id.clone(),
        tool_name: extension.manifest.tool_name.clone(),
        passed: fixtures
            .iter()
            .all(|fixture| fixture.status == ToolExtensionFixtureStatus::Passed),
        fixtures,
    };
    if !report.passed {
        let failure = report
            .fixtures
            .iter()
            .find(|fixture| fixture.status == ToolExtensionFixtureStatus::Failed)
            .and_then(|fixture| fixture.diagnostic.as_deref())
            .unwrap_or("fixture failed");
        return Err(CommandError::user_fixable(
            "agent_tool_extension_fixture_failed",
            format!(
                "Tool extension `{}` failed required fixture verification: {failure}",
                extension.manifest.extension_id
            ),
        ));
    }
    Ok(report)
}

fn run_extension_process(
    extension: &LoadedToolExtension,
    context: &ToolExecutionContext,
    call: &ToolCallInput,
    timeout_ms: u64,
    sandbox_metadata: SandboxExecutionMetadata,
    require_enabled_integrity: bool,
    is_cancelled: impl Fn() -> bool,
) -> ToolRegistryResult<ToolHandlerOutput> {
    if require_enabled_integrity {
        verify_execution_integrity(extension)?;
    }
    let request = ToolExtensionProcessRequest {
        contract_version: TOOL_EXTENSION_PROCESS_CONTRACT_VERSION,
        extension_id: &extension.manifest.extension_id,
        tool_name: &extension.manifest.tool_name,
        tool_call_id: &call.tool_call_id,
        context,
        input: &call.input,
    };
    let mut stdin = serde_json::to_vec(&request).map_err(|error| {
        ToolExecutionError::invalid_input(
            "agent_tool_extension_request_encode_failed",
            format!("Xero could not encode the extension request: {error}"),
        )
    })?;
    if stdin.len() > EXTENSION_REQUEST_LIMIT_BYTES {
        return Err(ToolExecutionError::budget_exceeded(
            "agent_tool_extension_request_too_large",
            format!(
                "Tool extension `{}` request exceeds the {} byte process-input limit.",
                extension.manifest.extension_id, EXTENSION_REQUEST_LIMIT_BYTES
            ),
        ));
    }
    stdin.push(b'\n');
    let executable = extension
        .bundle_dir
        .join(&extension.manifest.runtime.executable);
    let mut argv = vec![executable.to_string_lossy().into_owned()];
    argv.extend(extension.manifest.runtime.args.iter().cloned());
    let output = SandboxedProcessRunner::new()
        .run_with_stdin(
            SandboxedProcessRequest {
                argv,
                cwd: Some(extension.bundle_dir.to_string_lossy().into_owned()),
                timeout_ms: Some(timeout_ms),
                stdout_limit_bytes: EXTENSION_OUTPUT_LIMIT_BYTES,
                stderr_limit_bytes: EXTENSION_OUTPUT_LIMIT_BYTES,
                metadata: sandbox_metadata,
            },
            stdin,
            is_cancelled,
        )
        .map_err(|error| {
            if error.code.contains("timeout") || error.code.contains("cancel") {
                ToolExecutionError::timeout(
                    "agent_tool_extension_timeout",
                    format!(
                        "Tool extension `{}` was terminated at its deadline.",
                        extension.manifest.extension_id
                    ),
                )
            } else {
                ToolExecutionError::unavailable(
                    "agent_tool_extension_process_unavailable",
                    format!(
                        "Tool extension `{}` could not start inside the required sandbox: {}",
                        extension.manifest.extension_id, error.message
                    ),
                )
            }
        })?;
    if output.exit_code != Some(0) {
        return Err(ToolExecutionError::unavailable(
            "agent_tool_extension_process_failed",
            format!(
                "Tool extension `{}` exited unsuccessfully and was contained outside the Xero process.",
                extension.manifest.extension_id
            ),
        ));
    }
    if output.stdout_truncated || output.stderr_truncated {
        return Err(ToolExecutionError::budget_exceeded(
            "agent_tool_extension_output_limit_exceeded",
            format!(
                "Tool extension `{}` exceeded the process output limit.",
                extension.manifest.extension_id
            ),
        ));
    }
    let stdout = output.stdout.unwrap_or_default();
    let response =
        serde_json::from_str::<ToolExtensionProcessResponse>(stdout.trim()).map_err(|error| {
            ToolExecutionError::invalid_input(
                "agent_tool_extension_response_malformed",
                format!(
                    "Tool extension `{}` returned malformed JSON: {error}",
                    extension.manifest.extension_id
                ),
            )
        })?;
    if response.summary.trim().is_empty() {
        return Err(ToolExecutionError::invalid_input(
            "agent_tool_extension_summary_missing",
            format!(
                "Tool extension `{}` returned an empty summary.",
                extension.manifest.extension_id
            ),
        ));
    }
    let mut handler_output = ToolHandlerOutput::new(response.summary, response.output);
    handler_output.telemetry_attributes.extend([
        (
            "xero.extension.id".into(),
            extension.manifest.extension_id.clone(),
        ),
        (
            "xero.extension.installation_hash".into(),
            extension.installation_hash.clone(),
        ),
        (
            "xero.extension.permission_id".into(),
            extension.manifest.permission.permission_id.clone(),
        ),
        (
            "xero.extension.execution".into(),
            "sandboxed_process".into(),
        ),
    ]);
    Ok(handler_output)
}

fn verify_execution_integrity(extension: &LoadedToolExtension) -> ToolRegistryResult<()> {
    let current = load_bundle(&extension.bundle_dir).map_err(command_error_to_core_error)?;
    let state =
        read_installation_state(&extension.bundle_dir).map_err(command_error_to_core_error)?;
    if current.installation_hash != extension.installation_hash
        || state.installation_hash != extension.installation_hash
        || !state.enabled
        || state.granted_permission_id.as_deref()
            != Some(extension.manifest.permission.permission_id.as_str())
    {
        return Err(ToolExecutionError::unavailable(
            "agent_tool_extension_integrity_failed",
            format!(
                "Tool extension `{}` changed, was disabled, or lost its permission grant after registration.",
                extension.manifest.extension_id
            ),
        ));
    }
    Ok(())
}

fn installation_hash(manifest: &ToolExtensionManifest, executable: &Path) -> CommandResult<String> {
    let mut hasher = Sha256::new();
    let manifest_bytes = serde_json::to_vec(manifest).map_err(|error| {
        CommandError::system_fault(
            "agent_tool_extension_hash_failed",
            format!("Xero could not encode an extension manifest for hashing: {error}"),
        )
    })?;
    hasher.update(manifest_bytes);
    let executable_bytes =
        fs::read(executable).map_err(|error| extension_io_error("hash", executable, error))?;
    hasher.update(executable_bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn read_installation_state(bundle_dir: &Path) -> CommandResult<ToolExtensionInstallationState> {
    let path = bundle_dir.join(TOOL_EXTENSION_STATE_FILE);
    let bytes = fs::read(&path).map_err(|error| extension_io_error("read", &path, error))?;
    let state =
        serde_json::from_slice::<ToolExtensionInstallationState>(&bytes).map_err(|error| {
            CommandError::user_fixable(
                "agent_tool_extension_state_malformed",
                format!(
                    "Tool extension state `{}` is malformed: {error}",
                    path.display()
                ),
            )
        })?;
    if state.schema != TOOL_EXTENSION_STATE_SCHEMA {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_state_schema_invalid",
            format!(
                "Tool extension state `{}` uses an unsupported schema.",
                path.display()
            ),
        ));
    }
    Ok(state)
}

fn write_installation_state(
    bundle_dir: &Path,
    state: &ToolExtensionInstallationState,
) -> CommandResult<()> {
    let path = bundle_dir.join(TOOL_EXTENSION_STATE_FILE);
    let bytes = serde_json::to_vec_pretty(state).map_err(|error| {
        CommandError::system_fault(
            "agent_tool_extension_state_encode_failed",
            format!("Xero could not encode extension installation state: {error}"),
        )
    })?;
    fs::write(&path, bytes).map_err(|error| extension_io_error("write", &path, error))?;
    Ok(())
}

fn copy_bundle_file(source: &Path, destination: &Path) -> CommandResult<()> {
    fs::copy(source, destination)
        .map(|_| ())
        .map_err(|error| extension_io_error("copy", source, error))
}

fn harden_installed_bundle(
    root: &Path,
    bundle_dir: &Path,
    executable_name: &str,
) -> CommandResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        for directory in [root, bundle_dir] {
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
                .map_err(|error| extension_io_error("harden", directory, error))?;
        }
        for path in [
            bundle_dir.join(TOOL_EXTENSION_MANIFEST_FILE),
            bundle_dir.join(TOOL_EXTENSION_STATE_FILE),
        ] {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))
                .map_err(|error| extension_io_error("harden", &path, error))?;
        }
        let executable = bundle_dir.join(executable_name);
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .map_err(|error| extension_io_error("harden", &executable, error))?;
    }
    #[cfg(not(unix))]
    let _ = (root, bundle_dir, executable_name);
    Ok(())
}

fn canonical_directory(path: &Path) -> CommandResult<PathBuf> {
    let canonical =
        fs::canonicalize(path).map_err(|error| extension_io_error("open", path, error))?;
    if !canonical.is_dir() {
        return Err(CommandError::user_fixable(
            "agent_tool_extension_source_invalid",
            format!("Extension source `{}` is not a directory.", path.display()),
        ));
    }
    Ok(canonical)
}

fn validate_storage_identifier(value: &str) -> CommandResult<()> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
    {
        return Err(CommandError::invalid_request("extensionId"));
    }
    Ok(())
}

fn permission_summary(manifest: &ToolExtensionManifest) -> ToolExtensionPermissionSummary {
    ToolExtensionPermissionSummary {
        permission_id: manifest.permission.permission_id.clone(),
        label: manifest.permission.label.clone(),
        effect_class: manifest.permission.effect_class.clone(),
        risk_class: manifest.permission.risk_class.clone(),
        audit_label: manifest.permission.audit_label.clone(),
        mutability: manifest.mutability,
        sandbox_requirement: manifest.sandbox_requirement,
        approval_requirement: manifest.approval_requirement,
        capability_tags: manifest.capability_tags.clone(),
    }
}

fn invalid_catalog_entry(extension_id: String, error: CommandError) -> ToolExtensionCatalogEntry {
    ToolExtensionCatalogEntry {
        extension_id: extension_id.clone(),
        label: extension_id.clone(),
        tool_name: extension_id,
        enabled: false,
        eligible: false,
        installation_hash: String::new(),
        permission: ToolExtensionPermissionSummary {
            permission_id: "invalid".into(),
            label: "Invalid extension".into(),
            effect_class: xero_agent_core::ToolEffectClass::Observe,
            risk_class: "invalid".into(),
            audit_label: "invalid_extension".into(),
            mutability: ToolMutability::ReadOnly,
            sandbox_requirement: ToolSandboxRequirement::ReadOnly,
            approval_requirement: xero_agent_core::ToolApprovalRequirement::Always,
            capability_tags: Vec::new(),
        },
        diagnostics: vec![diagnostic_from_command_error(error)],
    }
}

fn diagnostic_from_command_error(error: CommandError) -> ToolExtensionDiagnostic {
    ToolExtensionDiagnostic {
        code: error.code,
        message: error.message,
    }
}

fn core_error_to_command_error(error: ToolExecutionError) -> CommandError {
    CommandError::user_fixable(error.code, error.message)
}

fn command_error_to_core_error(error: CommandError) -> ToolExecutionError {
    ToolExecutionError::unavailable(error.code, error.message)
}

fn extension_io_error(operation: &str, path: &Path, error: std::io::Error) -> CommandError {
    CommandError::system_fault(
        "agent_tool_extension_io_failed",
        format!(
            "Xero could not {operation} tool-extension path `{}`: {error}",
            path.display()
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use serde_json::json;
    use tempfile::TempDir;

    use super::*;

    fn write_bundle(
        root: &Path,
        extension_id: &str,
        tool_name: &str,
        permission_id: &str,
        mutability: &str,
        handler: &str,
    ) -> PathBuf {
        let source = root.join(extension_id);
        fs::create_dir_all(&source).expect("create bundle");
        let sandbox = if mutability == "read_only" {
            "read_only"
        } else {
            "workspace_write"
        };
        fs::write(
            source.join(TOOL_EXTENSION_MANIFEST_FILE),
            serde_json::to_vec_pretty(&json!({
                "contractVersion": 1,
                "extensionId": extension_id,
                "toolName": tool_name,
                "label": "Test extension",
                "description": "Exercises the production extension loader.",
                "inputSchema": {
                    "type": "object",
                    "properties": { "query": { "type": "string" } },
                    "required": ["query"]
                },
                "permission": {
                    "permissionId": permission_id,
                    "label": "Read test input",
                    "effectClass": if mutability == "read_only" { "observe" } else { "workspace_mutation" },
                    "riskClass": "low",
                    "auditLabel": "test_extension"
                },
                "mutability": mutability,
                "sandboxRequirement": sandbox,
                "approvalRequirement": "policy",
                "capabilityTags": ["test_extension"],
                "testFixtures": [{
                    "fixtureId": "basic",
                    "input": { "query": "hello" },
                    "expectedSummaryContains": "hello"
                }],
                "runtime": {
                    "kind": "process",
                    "executable": "handler",
                    "args": []
                }
            }))
            .expect("manifest json"),
        )
        .expect("write manifest");
        let handler_path = source.join("handler");
        let mut file = fs::File::create(&handler_path).expect("create handler");
        file.write_all(handler.as_bytes()).expect("write handler");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&handler_path, fs::Permissions::from_mode(0o700))
                .expect("make executable");
        }
        source
    }

    const ECHO_HANDLER: &str = r#"#!/bin/sh
read request
query=$(printf '%s' "$request" | sed -n 's/.*"query":"\([^"]*\)".*/\1/p')
printf '{"summary":"read %s","output":{"query":"%s"}}\n' "$query" "$query"
"#;

    #[cfg(target_os = "macos")]
    #[test]
    fn install_enable_and_reload_read_only_extension_from_app_data() {
        let temp = TempDir::new().expect("tempdir");
        let source_root = temp.path().join("sources");
        let app_data = temp.path().join("app-data");
        let source = write_bundle(
            &source_root,
            "demo.read",
            "demo_read",
            "demo_read_permission",
            "read_only",
            ECHO_HANDLER,
        );

        let installed = install_tool_extension(&app_data, &source, &BTreeSet::new())
            .expect("install extension");
        assert!(!installed.extensions[0].enabled);
        let enabled = set_tool_extension_enabled(
            &app_data,
            "demo.read",
            true,
            Some("demo_read_permission"),
            &BTreeSet::new(),
        )
        .expect("enable extension");
        assert!(enabled.extensions[0].eligible);
        assert_eq!(
            load_enabled_tool_extensions(&app_data, &BTreeSet::new())
                .expect("reload extensions")
                .len(),
            1
        );
        let mut registry = crate::runtime::ToolRegistry::builtin_with_options(
            crate::runtime::ToolRegistryOptions {
                runtime_agent_id: crate::commands::RuntimeAgentIdDto::Engineer,
                ..crate::runtime::ToolRegistryOptions::default()
            },
        );
        registry
            .refresh_enabled_tool_extensions_from(&app_data)
            .expect("register verified extension");
        let descriptor = registry
            .descriptors_v2()
            .into_iter()
            .find(|descriptor| descriptor.name == "demo_read")
            .expect("extension descriptor");
        assert_eq!(
            descriptor
                .telemetry_attributes
                .get("xero.extension.permission_id")
                .map(String::as_str),
            Some("demo_read_permission")
        );

        let upgraded = install_tool_extension(&app_data, &source, &BTreeSet::new())
            .expect("upgrade extension");
        assert!(!upgraded.extensions[0].enabled);
        let removed = remove_tool_extension(&app_data, "demo.read", &BTreeSet::new())
            .expect("remove extension");
        assert!(removed.extensions.is_empty());
    }

    #[test]
    fn malformed_manifest_is_rejected_before_installation() {
        let temp = TempDir::new().expect("tempdir");
        let source = temp.path().join("malformed");
        fs::create_dir_all(&source).expect("create source");
        fs::write(source.join(TOOL_EXTENSION_MANIFEST_FILE), b"{")
            .expect("write malformed manifest");

        let error =
            install_tool_extension(&temp.path().join("app-data"), &source, &BTreeSet::new())
                .expect_err("malformed extension must be rejected");

        assert_eq!(error.code, "agent_tool_extension_manifest_malformed");
    }

    #[test]
    fn built_in_tool_name_collision_is_rejected_before_execution() {
        let temp = TempDir::new().expect("tempdir");
        let source = write_bundle(
            &temp.path().join("sources"),
            "demo.collision",
            "read",
            "demo_collision_permission",
            "read_only",
            ECHO_HANDLER,
        );

        let error = install_tool_extension(
            &temp.path().join("app-data"),
            &source,
            &BTreeSet::from(["read".into()]),
        )
        .expect_err("built-in collision must be rejected");

        assert_eq!(error.code, "agent_tool_extension_name_collision");
    }

    #[test]
    fn mutating_extension_fails_closed_before_registration() {
        let temp = TempDir::new().expect("tempdir");
        let source = write_bundle(
            &temp.path().join("sources"),
            "demo.write",
            "demo_write",
            "demo_write_permission",
            "mutating",
            ECHO_HANDLER,
        );

        let error =
            install_tool_extension(&temp.path().join("app-data"), &source, &BTreeSet::new())
                .expect_err("mutating extension must fail closed");

        assert_eq!(
            error.code,
            "agent_tool_extension_mutating_isolation_unavailable"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn permission_grant_must_match_declared_permission() {
        let temp = TempDir::new().expect("tempdir");
        let app_data = temp.path().join("app-data");
        let source = write_bundle(
            &temp.path().join("sources"),
            "demo.denied",
            "demo_denied",
            "demo_permission",
            "read_only",
            ECHO_HANDLER,
        );
        install_tool_extension(&app_data, &source, &BTreeSet::new()).expect("install");

        let error = set_tool_extension_enabled(
            &app_data,
            "demo.denied",
            true,
            Some("wrong_permission"),
            &BTreeSet::new(),
        )
        .expect_err("permission mismatch must be denied");

        assert_eq!(error.code, "agent_tool_extension_permission_not_granted");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn hung_extension_is_force_terminated_during_fixture_verification() {
        let temp = TempDir::new().expect("tempdir");
        let source = write_bundle(
            &temp.path().join("sources"),
            "demo.hung",
            "demo_hung",
            "demo_hung_permission",
            "read_only",
            "#!/bin/sh\nread request\nsleep 10\n",
        );

        let error =
            install_tool_extension(&temp.path().join("app-data"), &source, &BTreeSet::new())
                .expect_err("hung fixture must fail");

        assert_eq!(error.code, "agent_tool_extension_fixture_failed");
        assert!(error.message.contains("terminated"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn panicking_extension_is_contained_during_fixture_verification() {
        let temp = TempDir::new().expect("tempdir");
        let source = write_bundle(
            &temp.path().join("sources"),
            "demo.panic",
            "demo_panic",
            "demo_panic_permission",
            "read_only",
            "#!/bin/sh\nread request\nexit 101\n",
        );

        let error =
            install_tool_extension(&temp.path().join("app-data"), &source, &BTreeSet::new())
                .expect_err("panicking fixture must fail");

        assert_eq!(error.code, "agent_tool_extension_fixture_failed");
        assert!(error.message.contains("contained"));
    }

    #[test]
    fn duplicate_tool_and_permission_capabilities_are_not_eligible() {
        let mut entries = vec![
            invalid_catalog_entry(
                "one".into(),
                CommandError::user_fixable("placeholder", "placeholder"),
            ),
            invalid_catalog_entry(
                "two".into(),
                CommandError::user_fixable("placeholder", "placeholder"),
            ),
        ];
        for entry in &mut entries {
            entry.tool_name = "same_tool".into();
            entry.permission.permission_id = "same_permission".into();
            entry.diagnostics.clear();
            entry.eligible = true;
        }

        apply_collision_diagnostics(&mut entries, &BTreeSet::new());

        assert!(entries.iter().all(|entry| !entry.eligible));
        assert!(entries.iter().all(|entry| entry
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "agent_tool_extension_capability_collision")));
    }
}
