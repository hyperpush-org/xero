use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    io::{self, Read, Write},
    path::Path,
    process::{Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    ToolCallInput, ToolDescriptorV2, ToolEffectClass, ToolExecutionContext, ToolExecutionError,
    ToolMutability, ToolSandboxRequirement,
};

const SANDBOX_STREAM_DRAIN_TIMEOUT: Duration = Duration::from_millis(500);

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
        self.run_internal(request, None, is_cancelled)
    }

    pub fn run_with_stdin(
        &self,
        request: SandboxedProcessRequest,
        stdin_bytes: Vec<u8>,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<SandboxedProcessOutput, SandboxedProcessError> {
        self.run_internal(request, Some(stdin_bytes), is_cancelled)
    }

    fn run_internal(
        &self,
        request: SandboxedProcessRequest,
        stdin_bytes: Option<Vec<u8>>,
        is_cancelled: impl Fn() -> bool,
    ) -> Result<SandboxedProcessOutput, SandboxedProcessError> {
        let timeout_ms = request
            .timeout_ms
            .unwrap_or(DEFAULT_SANDBOX_RUNNER_TIMEOUT_MS)
            .max(1);
        let mut process = self.spawn(SandboxedProcessSpawnRequest {
            argv: request.argv,
            cwd: request.cwd,
            stdin: if stdin_bytes.is_some() {
                SandboxedProcessStdin::Piped
            } else {
                SandboxedProcessStdin::Null
            },
            metadata: request.metadata,
        })?;
        let stdin_handle = if let Some(stdin_bytes) = stdin_bytes {
            let mut stdin = process.child.stdin.take().ok_or_else(|| {
                SandboxedProcessError::new(
                    "sandboxed_process_stdin_missing",
                    "Sandbox runner could not open process stdin.",
                    true,
                    process.metadata.clone(),
                    SandboxExitClassification::Unknown,
                )
            })?;
            Some(thread::spawn(move || stdin.write_all(&stdin_bytes)))
        } else {
            None
        };
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

        let stream_drain_deadline = Instant::now() + SANDBOX_STREAM_DRAIN_TIMEOUT;
        let stdout = finish_sandbox_capture(stdout_handle, stream_drain_deadline);
        let stderr = finish_sandbox_capture(stderr_handle, stream_drain_deadline);
        if let Some(handle) = stdin_handle {
            let stdin_result = if handle.is_finished() {
                Some(handle.join())
            } else {
                let deadline = stream_drain_deadline;
                while !handle.is_finished() && Instant::now() < deadline {
                    thread::sleep(Duration::from_millis(10));
                }
                handle.is_finished().then(|| handle.join())
            };
            match stdin_result {
                Some(Ok(Ok(()))) => {}
                Some(Ok(Err(_))) | None if timed_out || cancelled => {}
                Some(Ok(Err(error))) => {
                    return Err(SandboxedProcessError::new(
                        "sandboxed_process_stdin_write_failed",
                        format!("Sandbox runner could not write process stdin: {error}"),
                        true,
                        process.metadata,
                        SandboxExitClassification::Unknown,
                    ));
                }
                Some(Err(_)) => {
                    return Err(SandboxedProcessError::new(
                        "sandboxed_process_stdin_writer_panicked",
                        "Sandbox runner's stdin writer panicked.",
                        true,
                        process.metadata,
                        SandboxExitClassification::Unknown,
                    ));
                }
                None => {
                    return Err(SandboxedProcessError::new(
                        "sandboxed_process_stdin_write_incomplete",
                        "Sandbox runner stopped waiting for process stdin after the bounded drain deadline.",
                        true,
                        process.metadata,
                        SandboxExitClassification::Unknown,
                    ));
                }
            }
        }
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
        if stdout.drain_incomplete || stderr.drain_incomplete {
            return Err(SandboxedProcessError::new(
                "sandboxed_process_output_incomplete",
                format!(
                    "Sandbox runner stopped draining process output after the bounded deadline (stdout bytes: {}, stderr bytes: {}).",
                    stdout.excerpt.len(),
                    stderr.excerpt.len()
                ),
                true,
                process.metadata,
                SandboxExitClassification::Unknown,
            ));
        }
        if stdout.read_error.is_some() || stderr.read_error.is_some() {
            return Err(SandboxedProcessError::new(
                "sandboxed_process_output_failed",
                format!(
                    "Sandbox runner could not completely read process output (stdout: {}; stderr: {}).",
                    stdout.read_error.as_deref().unwrap_or("ok"),
                    stderr.read_error.as_deref().unwrap_or("ok")
                ),
                true,
                process.metadata,
                SandboxExitClassification::Unknown,
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

#[derive(Debug, Clone)]
struct SandboxOutputCapture {
    excerpt: Vec<u8>,
    truncated: bool,
    drain_incomplete: bool,
    read_error: Option<String>,
}

#[derive(Debug)]
struct SandboxCaptureState {
    capture: SandboxOutputCapture,
    completed: bool,
}

struct SandboxCaptureReader {
    state: Arc<(Mutex<SandboxCaptureState>, Condvar)>,
    stop: Arc<AtomicBool>,
    thread: thread::JoinHandle<()>,
}

trait PollableSandboxStream: Read + Send + 'static {
    fn wait_until_readable(&self, timeout: Duration) -> io::Result<bool>;
}

#[cfg(unix)]
fn unix_sandbox_stream_readable(
    stream: &impl std::os::fd::AsRawFd,
    timeout: Duration,
) -> io::Result<bool> {
    let mut descriptor = libc::pollfd {
        fd: stream.as_raw_fd(),
        events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
        revents: 0,
    };
    loop {
        let result = unsafe {
            libc::poll(
                &mut descriptor,
                1,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result >= 0 {
            return Ok(result > 0);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(windows)]
fn windows_sandbox_stream_readable(
    stream: &impl std::os::windows::io::AsRawHandle,
    timeout: Duration,
) -> io::Result<bool> {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "PeekNamedPipe"]
        fn peek_named_pipe(
            pipe: *mut c_void,
            buffer: *mut c_void,
            buffer_size: u32,
            bytes_read: *mut u32,
            total_bytes_available: *mut u32,
            bytes_left_this_message: *mut u32,
        ) -> i32;
    }

    const ERROR_BROKEN_PIPE: i32 = 109;
    let deadline = Instant::now() + timeout;
    loop {
        let mut available = 0_u32;
        let result = unsafe {
            peek_named_pipe(
                stream.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if result != 0 {
            if available > 0 {
                return Ok(true);
            }
        } else {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE) {
                return Ok(true);
            }
            return Err(error);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

impl PollableSandboxStream for ChildStdout {
    fn wait_until_readable(&self, timeout: Duration) -> io::Result<bool> {
        #[cfg(unix)]
        {
            unix_sandbox_stream_readable(self, timeout)
        }
        #[cfg(windows)]
        {
            windows_sandbox_stream_readable(self, timeout)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = timeout;
            Ok(true)
        }
    }
}

impl PollableSandboxStream for ChildStderr {
    fn wait_until_readable(&self, timeout: Duration) -> io::Result<bool> {
        #[cfg(unix)]
        {
            unix_sandbox_stream_readable(self, timeout)
        }
        #[cfg(windows)]
        {
            windows_sandbox_stream_readable(self, timeout)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = timeout;
            Ok(true)
        }
    }
}

fn spawn_sandbox_capture(
    mut reader: impl PollableSandboxStream,
    max_capture_bytes: usize,
) -> SandboxCaptureReader {
    let state = Arc::new((
        Mutex::new(SandboxCaptureState {
            capture: SandboxOutputCapture {
                excerpt: Vec::new(),
                truncated: false,
                drain_incomplete: false,
                read_error: None,
            },
            completed: false,
        }),
        Condvar::new(),
    ));
    let reader_state = state.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = stop.clone();
    let thread = thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            if reader_stop.load(Ordering::Acquire) {
                let (state, completed) = &*reader_state;
                let mut state = state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.completed = true;
                completed.notify_all();
                return;
            }
            match reader.wait_until_readable(Duration::from_millis(25)) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    let (state, completed) = &*reader_state;
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.capture.read_error = Some(error.to_string());
                    state.completed = true;
                    completed.notify_all();
                    return;
                }
            }
            let read = match reader.read(&mut buffer) {
                Ok(read) => read,
                Err(error) => {
                    let (state, completed) = &*reader_state;
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.capture.read_error = Some(error.to_string());
                    state.completed = true;
                    completed.notify_all();
                    return;
                }
            };
            if read == 0 {
                let (state, completed) = &*reader_state;
                let mut state = state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.completed = true;
                completed.notify_all();
                return;
            }
            let (state, _) = &*reader_state;
            let mut state = state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let remaining = max_capture_bytes.saturating_sub(state.capture.excerpt.len());
            if remaining > 0 {
                let to_copy = remaining.min(read);
                state.capture.excerpt.extend_from_slice(&buffer[..to_copy]);
                if to_copy < read {
                    state.capture.truncated = true;
                }
            } else {
                state.capture.truncated = true;
            }
        }
    });
    SandboxCaptureReader {
        state,
        stop,
        thread,
    }
}

fn finish_sandbox_capture(reader: SandboxCaptureReader, deadline: Instant) -> SandboxOutputCapture {
    let SandboxCaptureReader {
        state,
        stop,
        thread,
    } = reader;
    let (capture_state, completed) = &*state;
    let mut state = capture_state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    while !state.completed {
        let now = Instant::now();
        if now >= deadline {
            state.capture.truncated = true;
            state.capture.drain_incomplete = true;
            break;
        }
        let (next_state, wait_result) = completed
            .wait_timeout(state, deadline.saturating_duration_since(now))
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state = next_state;
        if wait_result.timed_out() && !state.completed {
            state.capture.truncated = true;
            state.capture.drain_incomplete = true;
            break;
        }
    }
    drop(state);
    stop.store(true, Ordering::Release);
    let reader_panicked = thread.join().is_err();
    let mut capture = capture_state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .capture
        .clone();
    if reader_panicked {
        capture.truncated = true;
        capture.drain_incomplete = true;
        capture.read_error = Some("sandbox output reader panicked".into());
    }
    capture
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

pub(crate) fn configure_sandboxed_process_group(command: &mut Command) {
    if crate::mutation_boundary_child_active() {
        return;
    }

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

pub(crate) fn cleanup_sandboxed_process_group(child_id: u32) {
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

        let read_only_command =
            matches!(descriptor.effect_class, ToolEffectClass::CommandExecution)
                && descriptor.sandbox_requirement == ToolSandboxRequirement::ReadOnly;
        if descriptor.mutability == ToolMutability::Mutating
            || matches!(descriptor.effect_class, ToolEffectClass::WorkspaceMutation)
            || (matches!(descriptor.effect_class, ToolEffectClass::CommandExecution)
                && !read_only_command)
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

    // Match case-insensitively: on macOS's case-insensitive filesystem `.GIT`/`.Xero` reach
    // the same protected directories, so an exact match would let the write through the guard.
    if protected_components
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case(".git"))
        && !context.explicit_git_mutation_allowed
    {
        return Err(format!(
            "Sandbox denied write `{path}` because `.git` mutation requires explicit policy."
        ));
    }

    if protected_components
        .first()
        .is_some_and(|part| part.eq_ignore_ascii_case(".xero"))
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
        return string_values(input)
            .iter()
            .any(|value| looks_like_network(value));
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
            application_metadata: Default::default(),
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
    fn network_intent_detection_fails_closed_for_empty_or_malformed_argv() {
        for input in [
            json!({ "argv": [], "url": "https://example.com" }),
            json!({ "argv": [null, 42], "endpoint": "ssh://example.com" }),
        ] {
            assert!(command_input_has_network_intent(&input));
        }
    }

    #[test]
    fn sandbox_value_contracts_cover_profiles_defaults_overrides_and_platform_plans() {
        let descriptors = [
            (
                descriptor(
                    "none-read",
                    ToolEffectClass::Observe,
                    ToolMutability::ReadOnly,
                    ToolSandboxRequirement::None,
                ),
                SandboxPermissionProfile::ReadOnly,
            ),
            (
                descriptor(
                    "none-write",
                    ToolEffectClass::WorkspaceMutation,
                    ToolMutability::Mutating,
                    ToolSandboxRequirement::None,
                ),
                SandboxPermissionProfile::FullLocalWithApproval,
            ),
            (
                descriptor(
                    "read",
                    ToolEffectClass::Observe,
                    ToolMutability::ReadOnly,
                    ToolSandboxRequirement::ReadOnly,
                ),
                SandboxPermissionProfile::ReadOnly,
            ),
            (
                descriptor(
                    "write",
                    ToolEffectClass::WorkspaceMutation,
                    ToolMutability::Mutating,
                    ToolSandboxRequirement::WorkspaceWrite,
                ),
                SandboxPermissionProfile::WorkspaceWrite,
            ),
            (
                descriptor(
                    "network",
                    ToolEffectClass::ExternalService,
                    ToolMutability::Mutating,
                    ToolSandboxRequirement::Network,
                ),
                SandboxPermissionProfile::WorkspaceWriteNetworkAllowed,
            ),
            (
                descriptor(
                    "local",
                    ToolEffectClass::CommandExecution,
                    ToolMutability::Mutating,
                    ToolSandboxRequirement::FullLocal,
                ),
                SandboxPermissionProfile::FullLocalWithApproval,
            ),
        ];
        for (descriptor, expected) in &descriptors {
            assert_eq!(
                SandboxPermissionProfile::for_descriptor(descriptor),
                *expected
            );
        }

        for (profile, network, writes, trust, approval) in [
            (
                SandboxPermissionProfile::ReadOnly,
                SandboxNetworkMode::Denied,
                false,
                false,
                false,
            ),
            (
                SandboxPermissionProfile::WorkspaceWrite,
                SandboxNetworkMode::Denied,
                true,
                true,
                false,
            ),
            (
                SandboxPermissionProfile::WorkspaceWriteNetworkDenied,
                SandboxNetworkMode::Denied,
                true,
                true,
                false,
            ),
            (
                SandboxPermissionProfile::WorkspaceWriteNetworkAllowed,
                SandboxNetworkMode::Allowed,
                true,
                true,
                false,
            ),
            (
                SandboxPermissionProfile::FullLocalWithApproval,
                SandboxNetworkMode::Allowed,
                true,
                true,
                true,
            ),
            (
                SandboxPermissionProfile::DangerousUnrestricted,
                SandboxNetworkMode::Allowed,
                true,
                false,
                false,
            ),
        ] {
            assert_eq!(profile.network_mode(), network);
            assert_eq!(profile.allows_workspace_write(), writes);
            assert_eq!(profile.requires_project_trust(), trust);
            assert_eq!(profile.requires_approval(), approval);
        }

        assert!(ProjectTrustState::Trusted.allows_privileged_tools());
        assert!(ProjectTrustState::UserApproved.allows_privileged_tools());
        assert!(!ProjectTrustState::ApprovalRequired.allows_privileged_tools());
        assert!(!ProjectTrustState::Untrusted.allows_privileged_tools());
        assert!(!ProjectTrustState::Blocked.allows_privileged_tools());
        assert!(!SandboxApprovalSource::None.satisfies_full_local());
        assert!(SandboxApprovalSource::Policy.satisfies_full_local());
        assert!(SandboxApprovalSource::Operator.satisfies_full_local());
        assert!(SandboxApprovalSource::DangerousUnrestricted.satisfies_full_local());

        let unrestricted = SandboxExecutionMetadata::unrestricted();
        assert_eq!(
            unrestricted.platform_plan.strategy,
            SandboxPlatformStrategy::DangerousUnrestricted
        );
        assert!(unrestricted.internal_state.git_mutation_allowed);
        assert!(!unrestricted.internal_state.app_data_state_protected);
        assert!(SandboxEnvironmentRedactionSummary::default().sanitized_environment);
        assert!(SandboxInternalStateProtection::default().app_data_state_protected);

        let noop = NoopToolSandbox.evaluate(
            &descriptors[0].0,
            &call(json!({})),
            &ToolExecutionContext::default(),
        );
        assert_eq!(
            noop.expect("noop sandbox").profile,
            SandboxPermissionProfile::DangerousUnrestricted
        );

        let request = SandboxedProcessRequest::new(
            vec!["true".into()],
            SandboxExecutionMetadata::unrestricted(),
        );
        assert_eq!(request.timeout_ms, Some(DEFAULT_SANDBOX_RUNNER_TIMEOUT_MS));
        assert_eq!(request.stdout_limit_bytes, DEFAULT_SANDBOX_OUTPUT_LIMIT_BYTES);
        let spawn = SandboxedProcessSpawnRequest::new(
            vec!["true".into()],
            SandboxExecutionMetadata::unrestricted(),
        );
        assert_eq!(spawn.stdin, SandboxedProcessStdin::Null);

        for (platform, strategy) in [
            (SandboxPlatform::Linux, SandboxPlatformStrategy::LinuxBubblewrap),
            (
                SandboxPlatform::Windows,
                SandboxPlatformStrategy::WindowsRestrictedToken,
            ),
            (
                SandboxPlatform::Unsupported,
                SandboxPlatformStrategy::PortablePreflightOnly,
            ),
        ] {
            let context = SandboxExecutionContext {
                platform,
                ..SandboxExecutionContext::default()
            };
            assert_eq!(
                platform_plan(SandboxPermissionProfile::WorkspaceWrite, &context).strategy,
                strategy
            );
        }

        let context = SandboxExecutionContext {
            workspace_root: "/repo with \"quotes\"".into(),
            app_data_roots: vec!["/secret\\state".into()],
            platform: SandboxPlatform::Macos,
            legacy_xero_migration_allowed: true,
            ..SandboxExecutionContext::default()
        };
        let macos = platform_plan(SandboxPermissionProfile::ReadOnly, &context);
        assert!(macos.profile_text.expect("macOS profile").contains("deny file-write"));
        assert_eq!(
            platform_plan(SandboxPermissionProfile::DangerousUnrestricted, &context).strategy,
            SandboxPlatformStrategy::DangerousUnrestricted
        );

        let overridden = PermissionProfileSandbox::new(context)
            .with_profile_override("none-read", SandboxPermissionProfile::DangerousUnrestricted);
        assert_eq!(overridden.context().workspace_root, "/repo with \"quotes\"");
        assert_eq!(
            overridden
                .evaluate(
                    &descriptors[0].0,
                    &call(json!({})),
                    &ToolExecutionContext::default(),
                )
                .expect("profile override")
                .profile,
            SandboxPermissionProfile::DangerousUnrestricted
        );
    }

    #[test]
    fn sandbox_process_preflight_and_spawn_errors_are_typed_for_every_strategy() {
        for argv in [Vec::new(), vec![" ".into()], vec!["bad\0argv".into()]] {
            let error = match prepare_sandboxed_spawn(
                    argv,
                    None,
                    SandboxedProcessStdin::Null,
                    SandboxExecutionMetadata::unrestricted(),
                ) {
                Err(error) => error,
                Ok(_) => panic!("invalid argv must be rejected"),
            };
            assert_eq!(error.code, "sandboxed_process_invalid_argv");
        }

        for (strategy, code) in [
            (
                SandboxPlatformStrategy::LinuxBubblewrap,
                "sandboxed_process_linux_unavailable",
            ),
            (
                SandboxPlatformStrategy::WindowsRestrictedToken,
                "sandboxed_process_windows_unavailable",
            ),
            (
                SandboxPlatformStrategy::PortablePreflightOnly,
                "sandboxed_process_platform_unavailable",
            ),
        ] {
            let mut metadata = SandboxExecutionMetadata::unrestricted();
            metadata.platform_plan.strategy = strategy;
            let error = match prepare_sandboxed_spawn(
                vec!["true".into()],
                None,
                SandboxedProcessStdin::Null,
                metadata,
            ) {
                Err(error) => error,
                Ok(_) => panic!("unsupported strategy must be rejected"),
            };
            assert_eq!(error.code, code);
            assert_eq!(
                error.metadata.exit_classification,
                SandboxExitClassification::DeniedBySandbox
            );
        }

        for (kind, code, retryable, classification) in [
            (
                io::ErrorKind::NotFound,
                "sandboxed_process_not_found",
                false,
                SandboxExitClassification::NotRun,
            ),
            (
                io::ErrorKind::PermissionDenied,
                "sandboxed_process_spawn_denied",
                false,
                SandboxExitClassification::DeniedBySandbox,
            ),
            (
                io::ErrorKind::Other,
                "sandboxed_process_spawn_failed",
                true,
                SandboxExitClassification::Unknown,
            ),
        ] {
            let error = sandbox_spawn_error(
                &["fixture-command".into()],
                io::Error::new(kind, "fixture failure"),
                SandboxExecutionMetadata::unrestricted(),
            );
            assert_eq!(error.code, code);
            assert_eq!(error.retryable, retryable);
            assert_eq!(error.metadata.exit_classification, classification);
            assert!(error.to_string().contains(code));
        }

        let actual = SandboxedProcessRunner::new()
            .spawn(SandboxedProcessSpawnRequest::new(
                vec!["xero-command-that-does-not-exist".into()],
                SandboxExecutionMetadata::unrestricted(),
            ))
            .expect_err("missing executable");
        assert_eq!(actual.code, "sandboxed_process_not_found");
    }

    #[cfg(unix)]
    #[test]
    fn sandbox_runner_classifies_success_failure_cancellation_and_truncation() {
        let runner = SandboxedProcessRunner::new();
        let output = runner
            .run(
                SandboxedProcessRequest {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        "printf abcdef; printf warning >&2".into(),
                    ],
                    cwd: None,
                    timeout_ms: Some(1_000),
                    stdout_limit_bytes: 3,
                    stderr_limit_bytes: 4,
                    metadata: SandboxExecutionMetadata::unrestricted(),
                },
                || false,
            )
            .expect("bounded output");
        assert_eq!(output.stdout.as_deref(), Some("abc"));
        assert_eq!(output.stderr.as_deref(), Some("warn"));
        assert!(output.stdout_truncated);
        assert!(output.stderr_truncated);
        assert_eq!(
            output.metadata.exit_classification,
            SandboxExitClassification::Success
        );

        let failed = runner
            .run(
                SandboxedProcessRequest {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        "printf sandbox-deny >&2; exit 9".into(),
                    ],
                    cwd: None,
                    timeout_ms: Some(1_000),
                    stdout_limit_bytes: 128,
                    stderr_limit_bytes: 128,
                    metadata: SandboxExecutionMetadata::unrestricted(),
                },
                || false,
            )
            .expect("failed process is an observed output");
        assert_eq!(failed.exit_code, Some(9));
        assert_eq!(
            failed.metadata.exit_classification,
            SandboxExitClassification::DeniedBySandbox
        );

        let cancelled = runner
            .run(
                SandboxedProcessRequest {
                    argv: vec!["/bin/sh".into(), "-c".into(), "sleep 5".into()],
                    cwd: None,
                    timeout_ms: Some(1_000),
                    stdout_limit_bytes: 128,
                    stderr_limit_bytes: 128,
                    metadata: SandboxExecutionMetadata::unrestricted(),
                },
                || true,
            )
            .expect_err("cancelled process");
        assert_eq!(cancelled.code, "sandboxed_process_cancelled");
        assert_eq!(
            cancelled.metadata.exit_classification,
            SandboxExitClassification::Cancelled
        );
    }

    #[test]
    fn sandbox_path_network_and_policy_helpers_cover_nested_inputs_and_opt_ins() {
        assert!(normalize_user_path("").is_err());
        assert!(normalize_user_path("../escape").is_err());
        assert_eq!(
            normalize_user_path("./src\\lib.rs")
                .expect("relative path")
                .rendered,
            "src/lib.rs"
        );
        assert!(normalize_user_path("C:\\repo\\file.txt")
            .expect("Windows path")
            .is_absolute);

        let nested = json!({
            "path": "a",
            "nested": {
                "fromPath": ["b", "c"],
                "absolute_path": "d",
                "ignored": "e"
            }
        });
        let mut extracted_paths = extract_path_values(&nested);
        extracted_paths.sort();
        assert_eq!(extracted_paths, vec!["a", "b", "c", "d"]);
        for key in [
            "path",
            "cwd",
            "fromPath",
            "toPath",
            "from_path",
            "to_path",
            "absolutePath",
            "absolute_path",
        ] {
            assert!(is_path_field_name(key));
        }
        assert!(!is_path_field_name("url"));

        for program in [
            "curl", "wget", "nc", "netcat", "ssh", "scp", "sftp", "ftp", "ping", "dig",
            "nslookup",
        ] {
            assert!(command_input_has_network_intent(
                &json!({ "argv": [program] })
            ));
        }
        assert!(command_input_has_network_intent(
            &json!({ "note": "run curl https://example.com" })
        ));
        assert!(!command_input_has_network_intent(
            &json!({ "argv": ["echo", "local"] })
        ));
        assert_eq!(string_values(&json!(["a", true, 1, null, { "x": "b" }])), vec!["a", "b"]);
        assert_eq!(dedupe(vec!["b".into(), "a".into(), "b".into()]), vec!["b", "a"]);
        assert_eq!(normalize_absolute(" C:\\repo\\ "), "C:/repo");
        assert!(path_starts_with("/repo/src", "/repo"));
        assert!(!path_starts_with("/repository", "/repo"));
        assert_eq!(escape_sandbox_string("a\\b\"c"), "a\\\\b\\\"c");

        let permissive = SandboxExecutionContext {
            workspace_root: "/repo".into(),
            explicit_git_mutation_allowed: true,
            legacy_xero_migration_allowed: true,
            ..SandboxExecutionContext::default()
        };
        validate_write_path("/repo/.git/config", &permissive).expect("explicit git opt-in");
        validate_write_path("/repo/.XERO/state", &permissive).expect("migration opt-in");
        validate_write_path("/repo/src/lib.rs", &permissive).expect("workspace write");

        let full_local = descriptor(
            "full-local",
            ToolEffectClass::CommandExecution,
            ToolMutability::Mutating,
            ToolSandboxRequirement::FullLocal,
        );
        let approval_denied = PermissionProfileSandbox::new(SandboxExecutionContext {
            workspace_root: "/repo".into(),
            approval_source: SandboxApprovalSource::None,
            ..SandboxExecutionContext::default()
        })
        .evaluate(
            &full_local,
            &call(json!({ "argv": ["echo", "local"] })),
            &ToolExecutionContext::default(),
        )
        .expect_err("full-local requires approval");
        assert_eq!(approval_denied.error.code, "agent_sandbox_approval_required");

        let write_descriptor = descriptor(
            "write",
            ToolEffectClass::WorkspaceMutation,
            ToolMutability::Mutating,
            ToolSandboxRequirement::WorkspaceWrite,
        );
        let write_denied = PermissionProfileSandbox::new(SandboxExecutionContext {
            workspace_root: "/repo".into(),
            ..SandboxExecutionContext::default()
        })
        .with_profile_override("write", SandboxPermissionProfile::ReadOnly)
        .evaluate(
            &write_descriptor,
            &call(json!({ "path": "src/lib.rs" })),
            &ToolExecutionContext::default(),
        )
        .expect_err("read-only override denies mutation");
        assert_eq!(write_denied.error.code, "agent_sandbox_write_denied");
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
    fn sandbox_allows_command_workspace_root_cwd_shorthand() {
        let descriptor = descriptor(
            "command",
            ToolEffectClass::CommandExecution,
            ToolMutability::Mutating,
            ToolSandboxRequirement::WorkspaceWrite,
        );

        sandbox()
            .evaluate(
                &descriptor,
                &call(json!({ "argv": ["true"], "cwd": "." })),
                &ToolExecutionContext::default(),
            )
            .expect("workspace root cwd shorthand should be allowed");
    }

    #[test]
    fn sandbox_treats_readonly_command_paths_as_read_access() {
        let descriptor = descriptor(
            "command_probe",
            ToolEffectClass::CommandExecution,
            ToolMutability::ReadOnly,
            ToolSandboxRequirement::ReadOnly,
        );

        let metadata = sandbox()
            .evaluate(
                &descriptor,
                &call(json!({ "path": "/linked/project" })),
                &ToolExecutionContext::default(),
            )
            .expect("read-only command probe should be sandboxed as observation");

        assert_eq!(metadata.profile, SandboxPermissionProfile::ReadOnly);
        assert!(metadata.readable_paths.contains(&"/linked/project".into()));
        assert!(metadata.writable_paths.is_empty());
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

    #[test]
    fn sandbox_capture_deadline_preserves_partial_output_without_blocking() {
        struct StalledAfterPrefixReader {
            prefix: Option<Vec<u8>>,
            release: Arc<(Mutex<bool>, Condvar)>,
            terminated: Arc<AtomicBool>,
        }

        impl Drop for StalledAfterPrefixReader {
            fn drop(&mut self) {
                self.terminated.store(true, Ordering::Release);
            }
        }

        impl Read for StalledAfterPrefixReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if let Some(prefix) = self.prefix.take() {
                    buffer[..prefix.len()].copy_from_slice(&prefix);
                    return Ok(prefix.len());
                }
                let (released, wake) = &*self.release;
                let mut released = released
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                while !*released {
                    released = wake
                        .wait(released)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                Ok(0)
            }
        }

        impl PollableSandboxStream for StalledAfterPrefixReader {
            fn wait_until_readable(&self, _timeout: Duration) -> io::Result<bool> {
                if self.prefix.is_some() {
                    return Ok(true);
                }
                Ok(*self
                    .release
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()))
            }
        }

        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let terminated = Arc::new(AtomicBool::new(false));
        let reader = spawn_sandbox_capture(
            StalledAfterPrefixReader {
                prefix: Some(b"partial".to_vec()),
                release: release.clone(),
                terminated: terminated.clone(),
            },
            1024,
        );
        let prefix_deadline = Instant::now() + Duration::from_secs(1);
        loop {
            let state = reader
                .state
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.capture.excerpt == b"partial" {
                break;
            }
            assert!(Instant::now() < prefix_deadline);
            drop(state);
            thread::sleep(Duration::from_millis(5));
        }

        let started = Instant::now();
        let capture = finish_sandbox_capture(reader, Instant::now() + Duration::from_millis(40));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(capture.excerpt, b"partial");
        assert!(capture.truncated);
        assert!(capture.drain_incomplete);
        assert!(capture.read_error.is_none());
        assert!(
            terminated.load(Ordering::Acquire),
            "bounded drain must join the retained-pipe reader"
        );

        let (released, wake) = &*release;
        *released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        wake.notify_all();
    }

    #[test]
    fn sandbox_capture_reports_wait_read_and_reader_panic_failures() {
        #[derive(Clone, Copy)]
        enum FailureMode {
            Wait,
            Read,
            Panic,
        }

        struct FailingReader(FailureMode);

        impl Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
                match self.0 {
                    FailureMode::Read => Err(io::Error::other("fixture read failed")),
                    FailureMode::Wait => unreachable!("wait failure must prevent reads"),
                    FailureMode::Panic => unreachable!("reader panic must happen while polling"),
                }
            }
        }

        impl PollableSandboxStream for FailingReader {
            fn wait_until_readable(&self, _timeout: Duration) -> io::Result<bool> {
                match self.0 {
                    FailureMode::Wait => Err(io::Error::other("fixture wait failed")),
                    FailureMode::Read => Ok(true),
                    FailureMode::Panic => panic!("fixture reader panic"),
                }
            }
        }

        for (mode, expected_error, reader_panicked) in [
            (FailureMode::Wait, "fixture wait failed", false),
            (FailureMode::Read, "fixture read failed", false),
            (FailureMode::Panic, "sandbox output reader panicked", true),
        ] {
            let capture = finish_sandbox_capture(
                spawn_sandbox_capture(FailingReader(mode), 32),
                Instant::now() + Duration::from_millis(100),
            );
            assert_eq!(capture.read_error.as_deref(), Some(expected_error));
            assert_eq!(capture.drain_incomplete, reader_panicked);
            assert_eq!(capture.truncated, reader_panicked);
        }
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

    #[cfg(unix)]
    #[test]
    fn sandbox_runner_times_out_when_child_does_not_read_large_stdin() {
        let started = Instant::now();
        let error = SandboxedProcessRunner::new()
            .run_with_stdin(
                SandboxedProcessRequest {
                    argv: vec!["/bin/sh".into(), "-c".into(), "sleep 10".into()],
                    cwd: None,
                    timeout_ms: Some(30),
                    stdout_limit_bytes: 1024,
                    stderr_limit_bytes: 1024,
                    metadata: SandboxExecutionMetadata::unrestricted(),
                },
                vec![b'x'; 2 * 1024 * 1024],
                || false,
            )
            .expect_err("blocked stdin must not bypass the process deadline");

        assert_eq!(error.code, "sandboxed_process_timeout");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn sandbox_runner_delivers_piped_stdin_to_child() {
        let output = SandboxedProcessRunner::new()
            .run_with_stdin(
                SandboxedProcessRequest {
                    argv: vec![
                        "/bin/sh".into(),
                        "-c".into(),
                        "read value; printf '%s' \"$value\"".into(),
                    ],
                    cwd: None,
                    timeout_ms: Some(1_000),
                    stdout_limit_bytes: 1024,
                    stderr_limit_bytes: 1024,
                    metadata: SandboxExecutionMetadata::unrestricted(),
                },
                b"hello-extension\n".to_vec(),
                || false,
            )
            .expect("sandbox runner should deliver stdin");

        assert_eq!(output.stdout.as_deref(), Some("hello-extension"));
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
