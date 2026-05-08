use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    io::{self, Read},
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
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
    pub metadata: Box<SandboxExecutionMetadata>,
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

const DEFAULT_SANDBOX_RUNNER_TIMEOUT_MS: u64 = 60_000;
const DEFAULT_SANDBOX_OUTPUT_LIMIT_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SandboxedProcessStdin {
    Null,
    Piped,
}

#[derive(Debug, Clone)]
pub struct SandboxedProcessRequest {
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
    pub stdout_limit_bytes: usize,
    pub stderr_limit_bytes: usize,
    pub metadata: SandboxExecutionMetadata,
}

impl SandboxedProcessRequest {
    pub fn new(argv: Vec<String>, metadata: SandboxExecutionMetadata) -> Self {
        Self {
            argv,
            cwd: None,
            timeout_ms: Some(DEFAULT_SANDBOX_RUNNER_TIMEOUT_MS),
            stdout_limit_bytes: DEFAULT_SANDBOX_OUTPUT_LIMIT_BYTES,
            stderr_limit_bytes: DEFAULT_SANDBOX_OUTPUT_LIMIT_BYTES,
            metadata,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SandboxedProcessSpawnRequest {
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub stdin: SandboxedProcessStdin,
    pub metadata: SandboxExecutionMetadata,
}

impl SandboxedProcessSpawnRequest {
    pub fn new(argv: Vec<String>, metadata: SandboxExecutionMetadata) -> Self {
        Self {
            argv,
            cwd: None,
            stdin: SandboxedProcessStdin::Null,
            metadata,
        }
    }
}

#[derive(Debug)]
pub struct SandboxedProcess {
    pub child: Child,
    pub original_argv: Vec<String>,
    pub applied_argv: Vec<String>,
    pub cwd: Option<String>,
    pub metadata: SandboxExecutionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxedProcessOutput {
    pub original_argv: Vec<String>,
    pub applied_argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub metadata: SandboxExecutionMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SandboxedProcessError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub metadata: Box<SandboxExecutionMetadata>,
}

impl SandboxedProcessError {
    fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        mut metadata: SandboxExecutionMetadata,
        classification: SandboxExitClassification,
    ) -> Self {
        metadata.exit_classification = classification;
        if classification == SandboxExitClassification::DeniedBySandbox
            && metadata.blocked_reason.is_none()
        {
            metadata.blocked_reason = Some(message.into());
            let message = metadata.blocked_reason.clone().unwrap_or_default();
            return Self {
                code: code.into(),
                message,
                retryable,
                metadata: Box::new(metadata),
            };
        }
        Self {
            code: code.into(),
            message: message.into(),
            retryable,
            metadata: Box::new(metadata),
        }
    }
}

impl std::fmt::Display for SandboxedProcessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for SandboxedProcessError {}

#[derive(Debug, Clone, Default)]
pub struct SandboxedProcessRunner;

impl SandboxedProcessRunner {
    pub fn new() -> Self {
        Self
    }

    pub fn spawn(
        &self,
        request: SandboxedProcessSpawnRequest,
    ) -> Result<SandboxedProcess, SandboxedProcessError> {
        let mut prepared =
            prepare_sandboxed_spawn(request.argv, request.cwd, request.stdin, request.metadata)?;
        let mut command = Command::new(&prepared.applied_argv[0]);
        command
            .args(prepared.applied_argv.iter().skip(1))
            .stdin(match prepared.stdin {
                SandboxedProcessStdin::Null => Stdio::null(),
                SandboxedProcessStdin::Piped => Stdio::piped(),
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(cwd) = prepared.cwd.as_deref() {
            command.current_dir(cwd);
        }
        configure_sandboxed_process_group(&mut command);
        apply_sandboxed_environment(&mut command, &mut prepared.metadata);

        let child = command.spawn().map_err(|error| {
            sandbox_spawn_error(&prepared.original_argv, error, prepared.metadata.clone())
        })?;

        Ok(SandboxedProcess {
            child,
            original_argv: prepared.original_argv,
            applied_argv: prepared.applied_argv,
            cwd: prepared.cwd,
            metadata: prepared.metadata,
        })
    }

    pub fn run(
        &self,
        request: SandboxedProcessRequest,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<SandboxedProcessOutput, SandboxedProcessError> {
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(DEFAULT_SANDBOX_RUNNER_TIMEOUT_MS)
            .max(1);
        let mut process = self.spawn(SandboxedProcessSpawnRequest {
            argv: request.argv,
            cwd: request.cwd,
            stdin: SandboxedProcessStdin::Null,
            metadata: request.metadata,
        })?;
        let stdout = process.child.stdout.take().ok_or_else(|| {
            SandboxedProcessError::new(
                "sandboxed_process_stdout_missing",
                "Sandbox runner could not capture process stdout.",
                true,
                process.metadata.clone(),
                SandboxExitClassification::Unknown,
            )
        })?;
        let stderr = process.child.stderr.take().ok_or_else(|| {
            SandboxedProcessError::new(
                "sandboxed_process_stderr_missing",
                "Sandbox runner could not capture process stderr.",
                true,
                process.metadata.clone(),
                SandboxExitClassification::Unknown,
            )
        })?;
        let stdout_handle = spawn_sandbox_capture(stdout, request.stdout_limit_bytes);
        let stderr_handle = spawn_sandbox_capture(stderr, request.stderr_limit_bytes);
        let started_at = Instant::now();
        let timeout = Duration::from_millis(timeout_ms);
        let mut timed_out = false;
        let mut cancelled = false;

        let status = loop {
            match process.child.try_wait() {
                Ok(Some(status)) => {
                    cleanup_sandboxed_process_group(process.child.id());
                    break status;
                }
                Ok(None) if is_cancelled() => {
                    cancelled = true;
                    let status =
                        terminate_sandboxed_process_tree(&mut process.child).map_err(|error| {
                            SandboxedProcessError::new(
                                "sandboxed_process_cancel_failed",
                                format!(
                                    "Sandbox runner could not stop a cancelled process: {error}"
                                ),
                                true,
                                process.metadata.clone(),
                                SandboxExitClassification::Cancelled,
                            )
                        })?;
                    break status;
                }
                Ok(None) if started_at.elapsed() >= timeout => {
                    timed_out = true;
                    let status =
                        terminate_sandboxed_process_tree(&mut process.child).map_err(|error| {
                            SandboxedProcessError::new(
                                "sandboxed_process_timeout_cleanup_failed",
                                format!(
                                    "Sandbox runner could not stop a timed-out process: {error}"
                                ),
                                true,
                                process.metadata.clone(),
                                SandboxExitClassification::Timeout,
                            )
                        })?;
                    break status;
                }
                Ok(None) => thread::sleep(Duration::from_millis(10)),
                Err(error) => {
                    let _ = terminate_sandboxed_process_tree(&mut process.child);
                    return Err(SandboxedProcessError::new(
                        "sandboxed_process_wait_failed",
                        format!("Sandbox runner could not observe the process: {error}"),
                        true,
                        process.metadata,
                        SandboxExitClassification::Unknown,
                    ));
                }
            }
        };

        let stdout = join_sandbox_capture(stdout_handle, process.metadata.clone())?;
        let stderr = join_sandbox_capture(stderr_handle, process.metadata.clone())?;
        let stdout_text = decode_optional_output(&stdout.excerpt);
        let stderr_text = decode_optional_output(&stderr.excerpt);
        process.metadata.exit_classification =
            classify_sandbox_exit(status, timed_out, cancelled, stderr_text.as_deref());

        if timed_out {
            return Err(SandboxedProcessError::new(
                "sandboxed_process_timeout",
                format!("Sandbox runner timed out the process after {timeout_ms}ms."),
                true,
                process.metadata,
                SandboxExitClassification::Timeout,
            ));
        }
        if cancelled {
            return Err(SandboxedProcessError::new(
                "sandboxed_process_cancelled",
                "Sandbox runner cancelled the process.",
                true,
                process.metadata,
                SandboxExitClassification::Cancelled,
            ));
        }

        Ok(SandboxedProcessOutput {
            original_argv: process.original_argv,
            applied_argv: process.applied_argv,
            cwd: process.cwd,
            stdout: stdout_text,
            stderr: stderr_text,
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            exit_code: status.code(),
            timed_out,
            cancelled,
            metadata: process.metadata,
        })
    }
}

struct PreparedSandboxedSpawn {
    original_argv: Vec<String>,
    applied_argv: Vec<String>,
    cwd: Option<String>,
    stdin: SandboxedProcessStdin,
    metadata: SandboxExecutionMetadata,
}

fn prepare_sandboxed_spawn(
    argv: Vec<String>,
    cwd: Option<String>,
    stdin: SandboxedProcessStdin,
    metadata: SandboxExecutionMetadata,
) -> Result<PreparedSandboxedSpawn, SandboxedProcessError> {
    if argv.is_empty() || argv[0].trim().is_empty() {
        return Err(SandboxedProcessError::new(
            "sandboxed_process_invalid_argv",
            "Sandbox runner requires a non-empty argv[0].",
            false,
            metadata,
            SandboxExitClassification::NotRun,
        ));
    }
    if argv.iter().any(|argument| argument.contains('\0')) {
        return Err(SandboxedProcessError::new(
            "sandboxed_process_invalid_argv",
            "Sandbox runner refused an argv containing a NUL byte.",
            false,
            metadata,
            SandboxExitClassification::NotRun,
        ));
    }

    let mut applied_argv = match metadata.platform_plan.strategy {
        SandboxPlatformStrategy::DangerousUnrestricted => argv.clone(),
        SandboxPlatformStrategy::MacosSandboxExec => {
            if !command_available("sandbox-exec") {
                return Err(SandboxedProcessError::new(
                    "sandboxed_process_macos_unavailable",
                    "macOS sandbox-exec is not available on PATH, so Xero cannot launch this subprocess safely.",
                    false,
                    metadata,
                    SandboxExitClassification::DeniedBySandbox,
                ));
            }
            let profile_text = metadata
                .platform_plan
                .profile_text
                .clone()
                .or_else(|| metadata.platform_plan.argv_prefix.get(2).cloned())
                .ok_or_else(|| {
                    SandboxedProcessError::new(
                        "sandboxed_process_profile_missing",
                        "macOS sandbox metadata did not include a sandbox-exec profile.",
                        false,
                        metadata.clone(),
                        SandboxExitClassification::DeniedBySandbox,
                    )
                })?;
            let mut wrapped = vec!["sandbox-exec".into(), "-p".into(), profile_text];
            wrapped.extend(argv.iter().cloned());
            wrapped
        }
        SandboxPlatformStrategy::LinuxBubblewrap => {
            return Err(SandboxedProcessError::new(
                "sandboxed_process_linux_unavailable",
                "Linux sandbox execution requires bubblewrap support, which is not enabled for this subprocess runner yet.",
                false,
                metadata,
                SandboxExitClassification::DeniedBySandbox,
            ));
        }
        SandboxPlatformStrategy::WindowsRestrictedToken => {
            return Err(SandboxedProcessError::new(
                "sandboxed_process_windows_unavailable",
                "Windows restricted subprocess execution is not enabled for this subprocess runner yet.",
                false,
                metadata,
                SandboxExitClassification::DeniedBySandbox,
            ));
        }
        SandboxPlatformStrategy::PortablePreflightOnly => {
            return Err(SandboxedProcessError::new(
                "sandboxed_process_platform_unavailable",
                "This platform has no OS sandbox runner available for subprocess execution.",
                false,
                metadata,
                SandboxExitClassification::DeniedBySandbox,
            ));
        }
    };
    if applied_argv.is_empty() {
        applied_argv = argv.clone();
    }

    Ok(PreparedSandboxedSpawn {
        original_argv: argv,
        applied_argv,
        cwd,
        stdin,
        metadata,
    })
}

fn sandbox_spawn_error(
    argv: &[String],
    error: io::Error,
    metadata: SandboxExecutionMetadata,
) -> SandboxedProcessError {
    let command = argv.first().cloned().unwrap_or_else(|| "<empty>".into());
    let (code, retryable, classification) = match error.kind() {
        io::ErrorKind::NotFound => (
            "sandboxed_process_not_found",
            false,
            SandboxExitClassification::NotRun,
        ),
        io::ErrorKind::PermissionDenied => (
            "sandboxed_process_spawn_denied",
            false,
            SandboxExitClassification::DeniedBySandbox,
        ),
        _ => (
            "sandboxed_process_spawn_failed",
            true,
            SandboxExitClassification::Unknown,
        ),
    };
    SandboxedProcessError::new(
        code,
        format!("Sandbox runner could not launch `{command}`: {error}"),
        retryable,
        metadata,
        classification,
    )
}

fn apply_sandboxed_environment(command: &mut Command, metadata: &mut SandboxExecutionMetadata) {
    let approved = metadata
        .environment_redaction
        .preserved_keys
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut preserved_keys = Vec::new();
    let mut redacted_key_count = 0_usize;
    let mut secret_like_key_count = 0_usize;

    command.env_clear();
    for (key, value) in env::vars_os() {
        let Some(key_text) = key.to_str().map(str::to_owned) else {
            redacted_key_count += 1;
            continue;
        };
        if secret_like_environment_key(&key_text) {
            secret_like_key_count += 1;
        }
        if approved.contains(&key_text) {
            preserved_keys.push(key_text);
            command.env(key, value);
        } else {
            redacted_key_count += 1;
        }
    }

    if approved.contains("PATH") && env::var_os("PATH").is_none() {
        preserved_keys.push("PATH".into());
        command.env("PATH", default_sandboxed_path());
    }
    command.env("XERO_AGENT_SANITIZED_ENV", "1");
    preserved_keys.push("XERO_AGENT_SANITIZED_ENV".into());
    preserved_keys.sort();
    preserved_keys.dedup();

    metadata.environment_redaction = SandboxEnvironmentRedactionSummary {
        sanitized_environment: true,
        preserved_keys,
        redacted_key_count,
        secret_like_key_count,
    };
}

fn secret_like_environment_key(key: &str) -> bool {
    let normalized = key.to_ascii_uppercase();
    normalized.contains("TOKEN")
        || normalized.contains("SECRET")
        || normalized.contains("PASSWORD")
        || normalized.contains("CREDENTIAL")
        || normalized.contains("AUTH")
        || normalized.contains("COOKIE")
        || normalized.contains("SESSION")
        || normalized.ends_with("_KEY")
}

fn command_available(command: &str) -> bool {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return command_path.is_file();
    }
    let Some(paths) = env::var_os("PATH") else {
        return false;
    };
    for directory in env::split_paths(&paths) {
        let candidate = directory.join(command);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        {
            let candidate = directory.join(format!("{command}.exe"));
            if candidate.is_file() {
                return true;
            }
        }
    }
    false
}

fn default_sandboxed_path() -> &'static str {
    #[cfg(windows)]
    {
        r"C:\Windows\System32;C:\Windows;C:\Windows\System32\WindowsPowerShell\v1.0"
    }

    #[cfg(target_os = "macos")]
    {
        "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin"
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        "/usr/local/bin:/usr/bin:/bin:/usr/local/sbin:/usr/sbin:/sbin"
    }

    #[cfg(not(any(unix, windows)))]
    {
        ""
    }
}

#[derive(Debug)]
struct SandboxOutputCapture {
    excerpt: Vec<u8>,
    truncated: bool,
}

fn spawn_sandbox_capture(
    mut reader: impl Read + Send + 'static,
    max_capture_bytes: usize,
) -> thread::JoinHandle<io::Result<SandboxOutputCapture>> {
    thread::spawn(move || {
        let mut excerpt = Vec::new();
        let mut truncated = false;
        let mut buffer = [0_u8; 4096];
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            let remaining = max_capture_bytes.saturating_sub(excerpt.len());
            if remaining > 0 {
                let to_copy = remaining.min(read);
                excerpt.extend_from_slice(&buffer[..to_copy]);
                if to_copy < read {
                    truncated = true;
                }
            } else {
                truncated = true;
            }
        }
        Ok(SandboxOutputCapture { excerpt, truncated })
    })
}

fn join_sandbox_capture(
    handle: thread::JoinHandle<io::Result<SandboxOutputCapture>>,
    metadata: SandboxExecutionMetadata,
) -> Result<SandboxOutputCapture, SandboxedProcessError> {
    match handle.join() {
        Ok(Ok(capture)) => Ok(capture),
        Ok(Err(error)) => Err(SandboxedProcessError::new(
            "sandboxed_process_output_failed",
            format!("Sandbox runner could not capture process output: {error}"),
            true,
            metadata,
            SandboxExitClassification::Unknown,
        )),
        Err(_) => Err(SandboxedProcessError::new(
            "sandboxed_process_output_failed",
            "Sandbox runner could not join the process output capture thread.",
            true,
            metadata,
            SandboxExitClassification::Unknown,
        )),
    }
}

fn decode_optional_output(bytes: &[u8]) -> Option<String> {
    if bytes.is_empty() {
        return None;
    }
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn classify_sandbox_exit(
    status: ExitStatus,
    timed_out: bool,
    cancelled: bool,
    stderr: Option<&str>,
) -> SandboxExitClassification {
    if cancelled {
        return SandboxExitClassification::Cancelled;
    }
    if timed_out {
        return SandboxExitClassification::Timeout;
    }
    if status.success() {
        return SandboxExitClassification::Success;
    }
    if stderr.is_some_and(sandbox_denial_output) || exit_status_looks_like_sandbox_abort(status) {
        return SandboxExitClassification::DeniedBySandbox;
    }
    SandboxExitClassification::Failed
}

fn sandbox_denial_output(stderr: &str) -> bool {
    let normalized = stderr.to_ascii_lowercase();
    normalized.contains("operation not permitted")
        || normalized.contains("deny")
        || normalized.contains("sandbox")
}

#[cfg(unix)]
fn exit_status_looks_like_sandbox_abort(status: ExitStatus) -> bool {
    use std::os::unix::process::ExitStatusExt;
    status.signal() == Some(libc::SIGABRT)
}

#[cfg(not(unix))]
fn exit_status_looks_like_sandbox_abort(_status: ExitStatus) -> bool {
    false
}

fn configure_sandboxed_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP);
    }
}

fn terminate_sandboxed_process_tree(child: &mut Child) -> io::Result<ExitStatus> {
    let child_id = child.id();
    if let Some(status) = child.try_wait()? {
        cleanup_sandboxed_process_group(child_id);
        return Ok(status);
    }
    terminate_sandboxed_process_gracefully(child)?;
    if let Some(status) = wait_for_sandboxed_exit(child, Duration::from_millis(500))? {
        cleanup_sandboxed_process_group(child_id);
        return Ok(status);
    }
    terminate_sandboxed_process_forcefully(child)?;
    let status = child.wait()?;
    cleanup_sandboxed_process_group(child_id);
    Ok(status)
}

fn wait_for_sandboxed_exit(child: &mut Child, timeout: Duration) -> io::Result<Option<ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn cleanup_sandboxed_process_group(child_id: u32) {
    #[cfg(unix)]
    {
        let _ = signal_sandboxed_process_group(child_id, libc::SIGTERM);
        let deadline = Instant::now() + Duration::from_millis(100);
        while sandboxed_process_group_exists(child_id) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        if sandboxed_process_group_exists(child_id) {
            let _ = signal_sandboxed_process_group(child_id, libc::SIGKILL);
        }
    }

    #[cfg(windows)]
    {
        let _ = child_id;
    }
}

#[cfg(unix)]
fn terminate_sandboxed_process_gracefully(child: &mut Child) -> io::Result<()> {
    signal_sandboxed_process_group(child.id(), libc::SIGTERM)
}

#[cfg(unix)]
fn terminate_sandboxed_process_forcefully(child: &mut Child) -> io::Result<()> {
    if let Err(error) = signal_sandboxed_process_group(child.id(), libc::SIGKILL) {
        child.kill().or(Err(error))?;
    }
    Ok(())
}

#[cfg(unix)]
fn signal_sandboxed_process_group(child_id: u32, signal: libc::c_int) -> io::Result<()> {
    let process_group_id = -(child_id as libc::pid_t);
    let result = unsafe { libc::kill(process_group_id, signal) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}

#[cfg(unix)]
fn sandboxed_process_group_exists(child_id: u32) -> bool {
    let process_group_id = -(child_id as libc::pid_t);
    let result = unsafe { libc::kill(process_group_id, 0) };
    if result == 0 {
        return true;
    }
    let error = io::Error::last_os_error();
    error.raw_os_error() != Some(libc::ESRCH)
}

#[cfg(windows)]
fn terminate_sandboxed_process_gracefully(child: &mut Child) -> io::Result<()> {
    taskkill_sandboxed_process_tree(child, false)
}

#[cfg(windows)]
fn terminate_sandboxed_process_forcefully(child: &mut Child) -> io::Result<()> {
    if let Err(error) = taskkill_sandboxed_process_tree(child, true) {
        child.kill().or(Err(error))?;
    }
    Ok(())
}

#[cfg(windows)]
fn taskkill_sandboxed_process_tree(child: &Child, force: bool) -> io::Result<()> {
    let mut command = Command::new("taskkill");
    command.arg("/PID").arg(child.id().to_string()).arg("/T");
    if force {
        command.arg("/F");
    }
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("taskkill exited with status {status}"),
        ))
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
    let mut lines = vec!["(version 1)".to_string(), "(allow default)".to_string()];

    if profile.allows_workspace_write() {
        lines.push(format!(
            "(deny file-write* (require-not (subpath \"{workspace}\")))"
        ));
        lines.push(format!("(deny file-write* (subpath \"{workspace}/.git\"))"));
        lines.push(format!(
            "(deny file-write* (subpath \"{workspace}/.xero\"))"
        ));
        for root in &context.app_data_roots {
            lines.push(format!(
                "(deny file-write* (subpath \"{}\"))",
                escape_sandbox_string(root)
            ));
        }
        lines.push("(allow file-read* file-write* (literal \"/dev/null\"))".to_string());
    } else {
        lines.push("(deny file-write*)".to_string());
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
        metadata: Box::new(metadata.denied(reason)),
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
        assert!(profile.contains("(deny file-write* (require-not (subpath \"/repo\")))"));
        assert!(profile.contains("(allow file-read* file-write* (literal \"/dev/null\"))"));
    }

    #[test]
    fn sandbox_runner_reports_explicit_unavailable_on_non_macos_strategies() {
        let mut metadata = SandboxExecutionMetadata::unrestricted();
        metadata.profile = SandboxPermissionProfile::WorkspaceWrite;
        metadata.platform_plan = OsSandboxPlan {
            platform: SandboxPlatform::Linux,
            strategy: SandboxPlatformStrategy::LinuxBubblewrap,
            argv_prefix: Vec::new(),
            profile_text: None,
            explanation: "test linux plan".into(),
        };

        let error = SandboxedProcessRunner::new()
            .run(
                SandboxedProcessRequest::new(vec!["true".into()], metadata),
                || false,
            )
            .expect_err("linux plan should fail explicitly until bubblewrap is wired");

        assert_eq!(error.code, "sandboxed_process_linux_unavailable");
        assert_eq!(
            error.metadata.exit_classification,
            SandboxExitClassification::DeniedBySandbox
        );
    }

    #[cfg(unix)]
    #[test]
    fn sandbox_runner_cleans_up_process_group_on_timeout() {
        let workspace = unique_test_dir("timeout-cleanup");
        let marker = Path::new(&workspace).join("leaked-after-timeout.txt");
        let error = SandboxedProcessRunner::new()
            .run(
                SandboxedProcessRequest {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        format!(
                            "sleep 1; printf leaked > {}",
                            shell_quote(&marker.display().to_string())
                        ),
                    ],
                    cwd: Some(workspace.clone()),
                    timeout_ms: Some(30),
                    stdout_limit_bytes: 1024,
                    stderr_limit_bytes: 1024,
                    metadata: SandboxExecutionMetadata::unrestricted(),
                },
                || false,
            )
            .expect_err("sandbox runner should time out the subprocess");

        assert_eq!(error.code, "sandboxed_process_timeout");
        assert_eq!(
            error.metadata.exit_classification,
            SandboxExitClassification::Timeout
        );
        std::thread::sleep(Duration::from_millis(1_200));
        assert!(
            !marker.exists(),
            "timeout cleanup must kill the subprocess group before it can keep running"
        );
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sandbox_runner_enforces_workspace_write_boundary_on_macos() {
        let workspace = unique_test_dir("workspace-boundary");
        let outside = unique_test_path("outside-write");
        let metadata = runner_metadata(
            &workspace,
            SandboxPermissionProfile::FullLocalWithApproval,
            SandboxNetworkMode::Allowed,
        );

        let output = SandboxedProcessRunner::new()
            .run(
                SandboxedProcessRequest {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        format!("printf escaped > {}", shell_quote(&outside)),
                    ],
                    cwd: Some(workspace.clone()),
                    timeout_ms: Some(2_000),
                    stdout_limit_bytes: 1024,
                    stderr_limit_bytes: 1024,
                    metadata,
                },
                || false,
            )
            .expect("sandboxed command should launch and be denied by the OS sandbox");

        assert_ne!(output.exit_code, Some(0));
        assert!(!Path::new(&outside).exists());
        assert_eq!(
            output.metadata.exit_classification,
            SandboxExitClassification::DeniedBySandbox
        );
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sandbox_runner_enforces_network_denied_profile_on_macos() {
        if !command_available("/usr/bin/curl") {
            return;
        }
        let workspace = unique_test_dir("network-denied");
        let metadata = runner_metadata(
            &workspace,
            SandboxPermissionProfile::WorkspaceWriteNetworkDenied,
            SandboxNetworkMode::Denied,
        );

        let output = SandboxedProcessRunner::new()
            .run(
                SandboxedProcessRequest {
                    argv: vec![
                        "/usr/bin/curl".into(),
                        "--max-time".into(),
                        "1".into(),
                        "https://example.com".into(),
                    ],
                    cwd: Some(workspace.clone()),
                    timeout_ms: Some(3_000),
                    stdout_limit_bytes: 1024,
                    stderr_limit_bytes: 2048,
                    metadata,
                },
                || false,
            )
            .expect("sandboxed curl should launch and fail under network denial");

        assert_ne!(output.exit_code, Some(0));
        assert_eq!(output.metadata.network_mode, SandboxNetworkMode::Denied);
        assert!(matches!(
            output.metadata.exit_classification,
            SandboxExitClassification::DeniedBySandbox | SandboxExitClassification::Failed
        ));
        let _ = std::fs::remove_dir_all(workspace);
    }

    #[cfg(target_os = "macos")]
    fn runner_metadata(
        workspace: &str,
        profile: SandboxPermissionProfile,
        network_mode: SandboxNetworkMode,
    ) -> SandboxExecutionMetadata {
        let context = SandboxExecutionContext {
            workspace_root: workspace.into(),
            project_trust: ProjectTrustState::Trusted,
            approval_source: SandboxApprovalSource::Operator,
            platform: SandboxPlatform::Macos,
            preserved_environment_keys: vec!["PATH".into()],
            ..SandboxExecutionContext::default()
        };
        let mut metadata = sandbox_metadata(profile, &context, &SandboxPathAccess::default());
        metadata.network_mode = network_mode;
        metadata.platform_plan = platform_plan(profile, &context);
        metadata
    }

    #[cfg(target_os = "macos")]
    fn unique_test_dir(label: &str) -> String {
        let path = unique_test_path(label);
        std::fs::create_dir_all(&path).expect("create temp workspace");
        std::fs::canonicalize(&path)
            .expect("canonical temp workspace")
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(target_os = "macos")]
    fn unique_test_path(label: &str) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        std::env::temp_dir()
            .join(format!("xero-sandbox-{label}-{nanos}"))
            .to_string_lossy()
            .into_owned()
    }

    #[cfg(target_os = "macos")]
    fn shell_quote(path: &str) -> String {
        format!("'{}'", path.replace('\'', "'\\''"))
    }
}
