//! Managed Solana toolchain resolver + installer.
//!
//! The workbench prefers binaries bundled in the Tauri resource directory,
//! then binaries provisioned into the app data directory, and only then the
//! host PATH / common shell-profile install locations. That keeps Solana
//! workflows usable from a Finder-launched desktop app without asking the
//! user to pre-install every CLI globally.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use tauri::{path::BaseDirectory, AppHandle, Emitter, Manager, Runtime};

use crate::commands::solana::events::SOLANA_TOOLCHAIN_INSTALL_EVENT;
use crate::commands::{CommandError, CommandResult};

const PROBE_TIMEOUT_SECS: u64 = 5;
const INSTALL_TIMEOUT_SECS: u64 = 1_800;
const TOOLCHAIN_ROOT_ENV: &str = "XERO_SOLANA_TOOLCHAIN_ROOT";
const TOOLCHAIN_RESOURCE_ROOT_ENV: &str = "XERO_SOLANA_RESOURCE_ROOT";
const AGAVE_VERSION: &str = "v3.1.13";
const ANCHOR_VERSION: &str = "v1.0.0";
const ANCHOR_VERSION_FILENAME: &str = "1.0.0";

static INSTALLING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolProbe {
    pub present: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainStatus {
    pub solana_cli: ToolProbe,
    pub anchor: ToolProbe,
    pub cargo_build_sbf: ToolProbe,
    pub rust: ToolProbe,
    pub node: ToolProbe,
    pub pnpm: ToolProbe,
    pub surfpool: ToolProbe,
    pub trident: ToolProbe,
    pub codama: ToolProbe,
    pub solana_verify: ToolProbe,
    /// Windows-only probe for WSL2 presence. `None` on non-Windows.
    pub wsl2: Option<ToolProbe>,
    /// App-data toolchain root used by the first-run installer.
    pub managed_root: Option<String>,
    /// Tauri resource root for fully bundled / side-loaded toolchains.
    pub bundled_root: Option<String>,
    pub installing: bool,
    pub install_supported: bool,
    pub installable_components: Vec<ToolchainComponentStatus>,
}

impl ToolchainStatus {
    /// True when the bare-minimum binaries for starting any cluster are
    /// present. Used by the UI's "ready to go" summary.
    pub fn has_minimum_for_localnet(&self) -> bool {
        self.solana_cli.present
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ToolchainComponent {
    Agave,
    Anchor,
}

impl ToolchainComponent {
    pub fn label(self) -> &'static str {
        match self {
            ToolchainComponent::Agave => "Agave Solana tools",
            ToolchainComponent::Anchor => "Anchor CLI",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            ToolchainComponent::Agave => {
                "solana, solana-test-validator, cargo-build-sbf, and spl-token"
            }
            ToolchainComponent::Anchor => "anchor build and anchor idl publishing",
        }
    }

    fn primary_binary(self) -> &'static str {
        match self {
            ToolchainComponent::Agave => "solana",
            ToolchainComponent::Anchor => "anchor",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainComponentStatus {
    pub component: ToolchainComponent,
    pub label: String,
    pub detail: String,
    pub installed: bool,
    pub installable: bool,
    pub required: bool,
    pub path: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolchainInstallRequest {
    #[serde(default)]
    pub components: Vec<ToolchainComponent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainInstallStatus {
    pub in_progress: bool,
    pub managed_root: String,
    pub components: Vec<ToolchainComponentStatus>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolchainInstallPhase {
    Starting,
    Downloading,
    Installing,
    Verifying,
    Completed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainInstallEvent {
    pub component: Option<ToolchainComponent>,
    pub phase: ToolchainInstallPhase,
    pub message: Option<String>,
    pub progress: Option<f32>,
    pub error: Option<String>,
}

impl ToolchainInstallEvent {
    fn phase(component: Option<ToolchainComponent>, phase: ToolchainInstallPhase) -> Self {
        Self {
            component,
            phase,
            message: None,
            progress: None,
            error: None,
        }
    }

    fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    fn with_progress(mut self, progress: f32) -> Self {
        self.progress = Some(progress.clamp(0.0, 1.0));
        self
    }

    fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }
}

/// Register Tauri-specific roots in process env so background worker threads
/// and testable runner traits can resolve the same managed/bundled binaries.
pub fn configure_tauri_roots<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(root) = managed_root_for_app(app) {
        env::set_var(TOOLCHAIN_ROOT_ENV, root);
    }
    if let Ok(root) = app
        .path()
        .resolve("resources/solana-toolchain", BaseDirectory::Resource)
    {
        env::set_var(TOOLCHAIN_RESOURCE_ROOT_ENV, root);
    }
}

pub fn managed_root_for_app<R: Runtime>(app: &AppHandle<R>) -> CommandResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("solana-toolchain"))
        .map_err(|error| {
            CommandError::system_fault(
                "solana_toolchain_data_dir_unavailable",
                format!("Could not resolve app data dir for Solana toolchain: {error}"),
            )
        })
}

/// Probe every CLI the workbench cares about. Safe to call on any platform
/// — absent binaries return `present: false` rather than erroring.
pub fn probe() -> ToolchainStatus {
    let components = component_statuses();
    ToolchainStatus {
        solana_cli: probe_tool("solana", &["--version"]),
        anchor: probe_tool("anchor", &["--version"]),
        cargo_build_sbf: probe_tool("cargo-build-sbf", &["--version"]),
        rust: probe_tool("rustc", &["--version"]),
        node: probe_tool("node", &["--version"]),
        pnpm: probe_tool("pnpm", &["--version"]),
        surfpool: probe_tool("surfpool", &["--version"]),
        trident: probe_tool("trident", &["--version"]),
        codama: probe_tool("codama", &["--version"]),
        solana_verify: probe_tool("solana-verify", &["--version"]),
        wsl2: probe_wsl2(),
        managed_root: Some(path_to_string(&managed_root())),
        bundled_root: bundled_root().map(|p| path_to_string(&p)),
        installing: INSTALLING.load(Ordering::Acquire),
        install_supported: install_supported(),
        installable_components: components,
    }
}

pub fn install_status() -> ToolchainInstallStatus {
    ToolchainInstallStatus {
        in_progress: INSTALLING.load(Ordering::Acquire),
        managed_root: path_to_string(&managed_root()),
        components: component_statuses(),
    }
}

/// Tauri command implementation: install missing managed components into the
/// app data dir. When no component list is supplied, install the core pack.
pub fn install<R: Runtime>(
    app: &AppHandle<R>,
    request: ToolchainInstallRequest,
) -> CommandResult<ToolchainInstallStatus> {
    if INSTALLING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(CommandError::user_fixable(
            "solana_toolchain_install_already_running",
            "Solana toolchain installation is already running.",
        ));
    }
    let _guard = InstallGuard;

    configure_tauri_roots(app);
    let root = managed_root_for_app(app)?;
    fs::create_dir_all(&root).map_err(|error| {
        CommandError::system_fault(
            "solana_toolchain_mkdir_failed",
            format!("Could not create {}: {error}", root.display()),
        )
    })?;

    emit_install(
        app,
        ToolchainInstallEvent::phase(None, ToolchainInstallPhase::Starting)
            .with_message(format!("Installing Solana tools into {}", root.display())),
    );

    let mut components = request.components;
    if components.is_empty() {
        components = vec![ToolchainComponent::Agave, ToolchainComponent::Anchor];
    }
    components.sort_by_key(|component| match component {
        ToolchainComponent::Agave => 0,
        ToolchainComponent::Anchor => 1,
    });
    components.dedup();

    let result: CommandResult<ToolchainInstallStatus> = (|| {
        for component in components {
            match component {
                ToolchainComponent::Agave => install_agave(app, &root)?,
                ToolchainComponent::Anchor => install_anchor(app, &root)?,
            }
        }
        emit_install(
            app,
            ToolchainInstallEvent::phase(None, ToolchainInstallPhase::Completed)
                .with_message("Solana toolchain is ready."),
        );
        Ok(install_status())
    })();

    if let Err(error) = &result {
        emit_install(
            app,
            ToolchainInstallEvent::phase(None, ToolchainInstallPhase::Failed)
                .with_error(error.to_string()),
        );
    }

    result
}

struct InstallGuard;

impl Drop for InstallGuard {
    fn drop(&mut self) {
        INSTALLING.store(false, Ordering::Release);
    }
}

fn install_agave<R: Runtime>(app: &AppHandle<R>, root: &Path) -> CommandResult<()> {
    let component = ToolchainComponent::Agave;
    if component_managed_or_bundled(component) {
        emit_install(
            app,
            ToolchainInstallEvent::phase(Some(component), ToolchainInstallPhase::Skipped)
                .with_message("Agave Solana tools are already available."),
        );
        return Ok(());
    }

    let (file_name, url) = agave_installer_spec()?;
    let download_path = root.join("downloads").join(file_name);

    emit_install(
        app,
        ToolchainInstallEvent::phase(Some(component), ToolchainInstallPhase::Downloading)
            .with_message(format!("Downloading Agave installer {AGAVE_VERSION}.")),
    );
    download_with_progress(
        app,
        Some(component),
        &url,
        &download_path,
        ToolchainInstallPhase::Downloading,
    )?;
    make_executable(&download_path)?;

    let data_dir = root.join("agave").join("install");
    fs::create_dir_all(&data_dir).map_err(|error| {
        CommandError::system_fault(
            "solana_toolchain_mkdir_failed",
            format!("Could not create {}: {error}", data_dir.display()),
        )
    })?;

    emit_install(
        app,
        ToolchainInstallEvent::phase(Some(component), ToolchainInstallPhase::Installing)
            .with_message("Installing Agave release without modifying the user PATH."),
    );
    let mut install_cmd = Command::new(&download_path);
    install_cmd
        .arg("--no-modify-path")
        .arg("--data-dir")
        .arg(&data_dir)
        .arg(AGAVE_VERSION);
    let output = run_install_command(&mut install_cmd)?;
    if !output.status.success() {
        return Err(CommandError::system_fault(
            "solana_toolchain_agave_install_failed",
            format!(
                "Agave installer exited {:?}: {}",
                output.status.code(),
                trim_process_text(&output.stderr)
            ),
        ));
    }

    emit_install(
        app,
        ToolchainInstallEvent::phase(Some(component), ToolchainInstallPhase::Verifying)
            .with_message(
                "Verifying solana, cargo-build-sbf, solana-test-validator, and spl-token.",
            ),
    );
    for binary in [
        "solana",
        "cargo-build-sbf",
        "solana-test-validator",
        "spl-token",
    ] {
        let path = resolve_binary_in_dirs(binary, &managed_dirs()).ok_or_else(|| {
            CommandError::system_fault(
                "solana_toolchain_agave_verify_failed",
                format!("{binary} was not found after installing Agave."),
            )
        })?;
        let _ = run_version(&path, &["--version"]).ok_or_else(|| {
            CommandError::system_fault(
                "solana_toolchain_agave_verify_failed",
                format!("{binary} did not report a version after install."),
            )
        })?;
    }

    Ok(())
}

fn install_anchor<R: Runtime>(app: &AppHandle<R>, root: &Path) -> CommandResult<()> {
    let component = ToolchainComponent::Anchor;
    if component_managed_or_bundled(component) {
        emit_install(
            app,
            ToolchainInstallEvent::phase(Some(component), ToolchainInstallPhase::Skipped)
                .with_message("Anchor CLI is already available."),
        );
        return Ok(());
    }

    let (file_name, url) = anchor_binary_spec()?;
    let bin_dir = root.join("bin");
    let target = bin_dir.join(binary_file_name("anchor"));
    fs::create_dir_all(&bin_dir).map_err(|error| {
        CommandError::system_fault(
            "solana_toolchain_mkdir_failed",
            format!("Could not create {}: {error}", bin_dir.display()),
        )
    })?;

    emit_install(
        app,
        ToolchainInstallEvent::phase(Some(component), ToolchainInstallPhase::Downloading)
            .with_message(format!("Downloading Anchor CLI {ANCHOR_VERSION}.")),
    );
    download_with_progress(
        app,
        Some(component),
        &url,
        &target,
        ToolchainInstallPhase::Downloading,
    )?;
    make_executable(&target)?;

    emit_install(
        app,
        ToolchainInstallEvent::phase(Some(component), ToolchainInstallPhase::Verifying)
            .with_message(format!("Verifying {file_name}.")),
    );
    let version = run_version(&target, &["--version"]).ok_or_else(|| {
        CommandError::system_fault(
            "solana_toolchain_anchor_verify_failed",
            "Anchor CLI did not report a version after install.",
        )
    })?;
    if !version.contains(ANCHOR_VERSION_FILENAME) {
        return Err(CommandError::system_fault(
            "solana_toolchain_anchor_version_mismatch",
            format!("Anchor reported `{version}`, expected {ANCHOR_VERSION_FILENAME}."),
        ));
    }
    Ok(())
}

fn probe_wsl2() -> Option<ToolProbe> {
    if cfg!(target_os = "windows") {
        Some(probe_tool("wsl", &["--status"]))
    } else {
        None
    }
}

/// Look up `name` in bundled/managed roots first, then PATH and common shell
/// profile directories. If found, run it once for a one-line version string.
pub fn probe_tool(name: &str, version_args: &[&str]) -> ToolProbe {
    let Some(path) = resolve_binary(name) else {
        return ToolProbe::default();
    };

    let version = run_version(&path, version_args);
    ToolProbe {
        present: true,
        path: Some(path_to_string(&path)),
        version,
    }
}

pub fn resolve_binary(name: &str) -> Option<PathBuf> {
    if name.trim().is_empty() {
        return None;
    }
    if looks_like_path(name) {
        let path = PathBuf::from(name);
        return path.is_file().then_some(path);
    }

    let preferred_dirs = bundled_dirs()
        .into_iter()
        .chain(managed_dirs())
        .collect::<Vec<_>>();
    if let Some(candidate) = resolve_binary_in_dirs(name, &preferred_dirs) {
        return Some(candidate);
    }

    if let Ok(path_var) = env::var("PATH") {
        for entry in env::split_paths(&path_var) {
            if let Some(candidate) = candidate_in_dir(&entry, name) {
                return Some(candidate);
            }
        }
    }

    for extra in fallback_dirs() {
        if let Some(candidate) = candidate_in_dir(&extra, name) {
            return Some(candidate);
        }
    }

    None
}

pub fn resolve_command(program: &str) -> String {
    resolve_binary(program)
        .map(|path| path_to_string(&path))
        .unwrap_or_else(|| program.to_string())
}

/// Prepend bundled/managed bin directories to a child process PATH. Use this
/// even when the immediate binary is absolute because CLIs often shell out to
/// sibling tools (`anchor build` -> `cargo-build-sbf`, for example).
pub fn augment_command(cmd: &mut Command) {
    if let Some(path) = child_path_value() {
        cmd.env("PATH", path);
    }
}

pub fn child_envs() -> Vec<(OsString, OsString)> {
    child_path_value()
        .map(|path| vec![(OsString::from("PATH"), path)])
        .unwrap_or_default()
}

fn component_statuses() -> Vec<ToolchainComponentStatus> {
    [ToolchainComponent::Agave, ToolchainComponent::Anchor]
        .into_iter()
        .map(component_status)
        .collect()
}

fn component_status(component: ToolchainComponent) -> ToolchainComponentStatus {
    let probe = probe_tool(component.primary_binary(), &["--version"]);
    ToolchainComponentStatus {
        component,
        label: component.label().to_string(),
        detail: component.detail().to_string(),
        installed: component_available(component),
        installable: install_supported(),
        required: true,
        path: probe.path,
        version: probe.version,
    }
}

fn component_available(component: ToolchainComponent) -> bool {
    match component {
        ToolchainComponent::Agave => [
            "solana",
            "cargo-build-sbf",
            "solana-test-validator",
            "spl-token",
        ]
        .iter()
        .all(|binary| resolve_binary(binary).is_some()),
        ToolchainComponent::Anchor => resolve_binary("anchor").is_some(),
    }
}

fn component_managed_or_bundled(component: ToolchainComponent) -> bool {
    let dirs = bundled_dirs()
        .into_iter()
        .chain(managed_dirs())
        .collect::<Vec<_>>();
    match component {
        ToolchainComponent::Agave => ["solana", "cargo-build-sbf", "solana-test-validator"]
            .iter()
            .all(|binary| resolve_binary_in_dirs(binary, &dirs).is_some()),
        ToolchainComponent::Anchor => resolve_binary_in_dirs("anchor", &dirs).is_some(),
    }
}

fn install_supported() -> bool {
    agave_installer_spec().is_ok() && anchor_binary_spec().is_ok()
}

fn child_path_value() -> Option<OsString> {
    let mut paths = bundled_dirs()
        .into_iter()
        .chain(managed_dirs())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    if let Some(existing) = env::var_os("PATH") {
        paths.extend(env::split_paths(&existing));
    }
    env::join_paths(paths).ok()
}

fn bundled_dirs() -> Vec<PathBuf> {
    bundled_root()
        .map(|root| tool_dirs_from_root(&root))
        .unwrap_or_default()
}

fn managed_dirs() -> Vec<PathBuf> {
    tool_dirs_from_root(&managed_root())
}

fn bundled_root() -> Option<PathBuf> {
    env::var_os(TOOLCHAIN_RESOURCE_ROOT_ENV)
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

fn managed_root() -> PathBuf {
    if let Some(path) = env::var_os(TOOLCHAIN_ROOT_ENV) {
        return PathBuf::from(path);
    }
    dirs::data_dir()
        .map(|dir| dir.join("xero").join("solana").join("toolchain"))
        .unwrap_or_else(|| env::temp_dir().join("xero-solana-toolchain"))
}

fn tool_dirs_from_root(root: &Path) -> Vec<PathBuf> {
    vec![
        root.join("bin"),
        root.join("agave")
            .join("install")
            .join("active_release")
            .join("bin"),
        root.join("anchor").join("bin"),
        root.join("node").join("bin"),
        root.join("pnpm").join("bin"),
    ]
}

fn fallback_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(home) = env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/share/solana/install/active_release/bin"));
        dirs.push(home.join(".cargo/bin"));
        dirs.push(home.join(".avm/bin"));
        dirs.push(home.join(".nvm/versions/node"));
    }
    // Homebrew common locations (Apple Silicon + Intel).
    dirs.push(PathBuf::from("/opt/homebrew/bin"));
    dirs.push(PathBuf::from("/usr/local/bin"));
    dirs.push(PathBuf::from("/usr/bin"));
    dirs
}

fn candidate_in_dir(dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = dir.join(name);
    if direct.is_file() {
        return Some(direct);
    }
    if cfg!(target_os = "windows") {
        for suffix in ["exe", "cmd", "bat", "ps1"] {
            let named = dir.join(format!("{name}.{suffix}"));
            if named.is_file() {
                return Some(named);
            }
        }
    }
    None
}

fn resolve_binary_in_dirs(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    for dir in dirs {
        if let Some(candidate) = candidate_in_dir(dir, name) {
            return Some(candidate);
        }
    }
    None
}

fn looks_like_path(value: &str) -> bool {
    value.contains('/') || value.contains('\\') || Path::new(value).is_absolute()
}

fn run_version(path: &Path, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.args(args);
    augment_command(&mut cmd);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    let child = cmd.spawn().ok()?;
    let output = wait_with_timeout(child, Duration::from_secs(PROBE_TIMEOUT_SECS))?;

    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    first_non_empty_line(&combined).map(|s| s.trim().to_string())
}

fn run_install_command(cmd: &mut Command) -> CommandResult<std::process::Output> {
    augment_command(cmd);
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    let child = cmd.spawn().map_err(|error| {
        CommandError::system_fault(
            "solana_toolchain_install_spawn_failed",
            format!("Could not start installer: {error}"),
        )
    })?;
    wait_with_timeout(child, Duration::from_secs(INSTALL_TIMEOUT_SECS)).ok_or_else(|| {
        CommandError::retryable(
            "solana_toolchain_install_timeout",
            format!("Solana toolchain installer timed out after {INSTALL_TIMEOUT_SECS}s."),
        )
    })
}

fn wait_with_timeout(
    mut child: std::process::Child,
    timeout: Duration,
) -> Option<std::process::Output> {
    use std::thread;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(_) => return None,
        }
    }
    child.wait_with_output().ok()
}

fn first_non_empty_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

fn path_to_string(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn binary_file_name(name: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn emit_install<R: Runtime>(app: &AppHandle<R>, event: ToolchainInstallEvent) {
    let _ = app.emit(SOLANA_TOOLCHAIN_INSTALL_EVENT, event);
}

fn download_with_progress<R: Runtime>(
    app: &AppHandle<R>,
    component: Option<ToolchainComponent>,
    url: &str,
    target: &Path,
    phase: ToolchainInstallPhase,
) -> CommandResult<()> {
    let client = reqwest::blocking::Client::builder()
        .timeout(None)
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| {
            CommandError::system_fault(
                "solana_toolchain_http_client_failed",
                format!("Could not build HTTP client: {error}"),
            )
        })?;

    let mut response = client.get(url).send().map_err(|error| {
        CommandError::system_fault(
            "solana_toolchain_download_failed",
            format!("GET {url} failed: {error}"),
        )
    })?;
    if !response.status().is_success() {
        return Err(CommandError::system_fault(
            "solana_toolchain_download_failed",
            format!("GET {url} returned {}", response.status()),
        ));
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::system_fault(
                "solana_toolchain_mkdir_failed",
                format!("Could not create {}: {error}", parent.display()),
            )
        })?;
    }
    let mut file = fs::File::create(target).map_err(|error| {
        CommandError::system_fault(
            "solana_toolchain_download_open_failed",
            format!("Could not open {} for writing: {error}", target.display()),
        )
    })?;

    let total = response.content_length();
    let mut downloaded = 0u64;
    let mut last_emit = Instant::now();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = response.read(&mut buf).map_err(|error| {
            CommandError::system_fault(
                "solana_toolchain_download_read_failed",
                format!("Download read failed: {error}"),
            )
        })?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|error| {
            CommandError::system_fault(
                "solana_toolchain_download_write_failed",
                format!("Download write failed: {error}"),
            )
        })?;
        downloaded += n as u64;
        if last_emit.elapsed() >= Duration::from_millis(150) {
            last_emit = Instant::now();
            let mut event =
                ToolchainInstallEvent::phase(component, phase).with_message(match total {
                    Some(t) => format!("{} / {} MB", downloaded / 1_000_000, t / 1_000_000),
                    None => format!("{} MB", downloaded / 1_000_000),
                });
            if let Some(t) = total {
                event = event.with_progress(downloaded as f32 / t as f32);
            }
            emit_install(app, event);
        }
    }

    if let Some(total) = total {
        emit_install(
            app,
            ToolchainInstallEvent::phase(component, phase)
                .with_progress(1.0)
                .with_message(format!("{} / {} MB", total / 1_000_000, total / 1_000_000)),
        );
    }
    file.sync_all().ok();
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> CommandResult<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| {
            CommandError::system_fault(
                "solana_toolchain_metadata_failed",
                format!("Could not stat {}: {error}", path.display()),
            )
        })?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| {
        CommandError::system_fault(
            "solana_toolchain_chmod_failed",
            format!("Could not make {} executable: {error}", path.display()),
        )
    })
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> CommandResult<()> {
    Ok(())
}

fn agave_installer_spec() -> CommandResult<(&'static str, String)> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let file = "agave-install-init-aarch64-apple-darwin";
        return Ok((
            file,
            github_release_url("anza-xyz/agave", AGAVE_VERSION, file),
        ));
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let file = "agave-install-init-x86_64-apple-darwin";
        return Ok((
            file,
            github_release_url("anza-xyz/agave", AGAVE_VERSION, file),
        ));
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let file = "agave-install-init-x86_64-unknown-linux-gnu";
        return Ok((
            file,
            github_release_url("anza-xyz/agave", AGAVE_VERSION, file),
        ));
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let file = "agave-install-init-x86_64-pc-windows-msvc.exe";
        return Ok((
            file,
            github_release_url("anza-xyz/agave", AGAVE_VERSION, file),
        ));
    }
    #[allow(unreachable_code)]
    Err(CommandError::user_fixable(
        "solana_toolchain_unsupported_host",
        "No managed Agave build is available for this OS / architecture.",
    ))
}

fn anchor_binary_spec() -> CommandResult<(&'static str, String)> {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        let file = "anchor-1.0.0-aarch64-apple-darwin";
        return Ok((
            file,
            github_release_url("solana-foundation/anchor", ANCHOR_VERSION, file),
        ));
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        let file = "anchor-1.0.0-x86_64-apple-darwin";
        return Ok((
            file,
            github_release_url("solana-foundation/anchor", ANCHOR_VERSION, file),
        ));
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        let file = "anchor-1.0.0-x86_64-unknown-linux-gnu";
        return Ok((
            file,
            github_release_url("solana-foundation/anchor", ANCHOR_VERSION, file),
        ));
    }
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        let file = "anchor-1.0.0-x86_64-pc-windows-msvc.exe";
        return Ok((
            file,
            github_release_url("solana-foundation/anchor", ANCHOR_VERSION, file),
        ));
    }
    #[allow(unreachable_code)]
    Err(CommandError::user_fixable(
        "solana_toolchain_unsupported_host",
        "No managed Anchor build is available for this OS / architecture.",
    ))
}

fn github_release_url(repo: &str, version: &str, file: &str) -> String {
    format!("https://github.com/{repo}/releases/download/{version}/{file}")
}

fn trim_process_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let trimmed = text.trim();
    if trimmed.len() <= 2_000 {
        trimmed.to_string()
    } else {
        format!("{}...[truncated]", &trimmed[..2_000])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_missing_tool_returns_absent() {
        let tool = probe_tool("this-binary-should-never-exist-xyz123", &["--version"]);
        assert!(!tool.present);
        assert!(tool.path.is_none());
        assert!(tool.version.is_none());
    }

    #[test]
    fn first_non_empty_line_skips_leading_whitespace() {
        assert_eq!(first_non_empty_line("\n\n  v1.2.3\nextra"), Some("v1.2.3"));
        assert_eq!(first_non_empty_line(""), None);
    }

    #[test]
    fn probe_returns_all_fields() {
        // We don't care whether any particular binary is present on the
        // CI host — just that the struct is populated and serializable.
        let status = probe();
        let json = serde_json::to_string(&status).expect("serializable");
        assert!(json.contains("\"solanaCli\""));
        assert!(json.contains("\"anchor\""));
        assert!(json.contains("\"rust\""));
        assert!(json.contains("\"node\""));
        assert!(json.contains("\"managedRoot\""));
        assert!(json.contains("\"installableComponents\""));
        if cfg!(target_os = "windows") {
            assert!(status.wsl2.is_some());
        } else {
            assert!(status.wsl2.is_none());
        }
    }

    #[test]
    fn has_minimum_for_localnet_requires_solana_cli() {
        let mut status = ToolchainStatus::default();
        assert!(!status.has_minimum_for_localnet());
        status.solana_cli.present = true;
        assert!(status.has_minimum_for_localnet());
    }

    #[test]
    fn root_dirs_include_expected_agave_layout() {
        let root = PathBuf::from("/tmp/xero-solana");
        let dirs = tool_dirs_from_root(&root);
        assert!(dirs.contains(
            &root
                .join("agave")
                .join("install")
                .join("active_release")
                .join("bin")
        ));
        assert!(dirs.contains(&root.join("bin")));
    }

    #[test]
    fn resolve_command_keeps_unknown_binary_name() {
        assert_eq!(
            resolve_command("this-binary-should-never-exist-xyz123"),
            "this-binary-should-never-exist-xyz123"
        );
    }
}
