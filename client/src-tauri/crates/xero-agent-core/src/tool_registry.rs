use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Component, Path},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc, Arc, Condvar, Mutex, OnceLock, Weak,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{NoopToolSandbox, SandboxExecutionMetadata, ToolSandbox};

const DEFAULT_TOOL_CALL_LIMIT: usize = 128;
const DEFAULT_TOOL_FAILURE_LIMIT: usize = 16;
const DEFAULT_REPEATED_EQUIVALENT_CALL_LIMIT: usize = 3;
const DEFAULT_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;
const DEFAULT_GROUP_WALL_CLOCK_MS: u64 = 120_000;
const DEFAULT_MUTATION_CLEANUP_MS: u64 = 2_000;
const DEFAULT_MAX_SUPERVISED_READ_ONLY_WORKERS: usize = 32;
const MUTATION_TERMINATION_GRACE: Duration = Duration::from_millis(25);
const MUTATION_SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(5);
const MAX_MUTATION_WORKER_MESSAGE_BYTES: usize = 32 * 1024 * 1024;
pub const TOOL_EXTENSION_MANIFEST_CONTRACT_VERSION: u32 = 1;
pub const MUTATION_EXECUTION_SCOPE_ATTRIBUTE: &str = "xero.mutation.scope";

static MUTATION_BOUNDARY_CHILD_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn mutation_boundary_child_active() -> bool {
    MUTATION_BOUNDARY_CHILD_ACTIVE.load(Ordering::SeqCst)
}

pub type ToolRegistryResult<T> = Result<T, ToolExecutionError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolEffectClass {
    Observe,
    FileRead,
    Search,
    Metadata,
    Retrieval,
    Diagnostics,
    WorkspaceMutation,
    AppStateMutation,
    CommandExecution,
    ExternalService,
    BrowserControl,
    DeviceControl,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolMutability {
    ReadOnly,
    Mutating,
}

impl ToolMutability {
    pub fn is_read_only(self) -> bool {
        self == Self::ReadOnly
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApplicationKind {
    Granular,
    Declarative,
    ReadOnlyBatch,
    MutatingBatch,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolBatchDispatchSafety {
    NotBatch,
    ParallelReadOnly,
    SequentialMutating,
    ToolOwnedAtomic,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolApplicationMetadata {
    pub family: String,
    pub kind: ToolApplicationKind,
    pub dispatch_safety: ToolBatchDispatchSafety,
    /// Batch-capable mutating tools should validate every target before writing,
    /// expose preview/dry-run behavior when possible, guard stale inputs, and
    /// report enough diff/summary detail for audit and recovery.
    #[serde(default)]
    pub safety_requirements: Vec<String>,
}

impl ToolApplicationMetadata {
    pub fn granular(family: impl Into<String>) -> Self {
        Self {
            family: family.into(),
            kind: ToolApplicationKind::Granular,
            dispatch_safety: ToolBatchDispatchSafety::NotBatch,
            safety_requirements: Vec::new(),
        }
    }
}

impl Default for ToolApplicationMetadata {
    fn default() -> Self {
        Self::granular("general")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolSandboxRequirement {
    None,
    ReadOnly,
    WorkspaceWrite,
    Network,
    FullLocal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolApprovalRequirement {
    Never,
    Policy,
    Always,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolErrorCategory {
    InvalidInput,
    PolicyDenied,
    ApprovalRequired,
    SandboxDenied,
    Timeout,
    ExternalDependencyMissing,
    ToolUnavailable,
    RetryableProviderToolFailure,
    BudgetExceeded,
    DoomLoopDetected,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolDoomLoopSignal {
    SameFailingToolRepeated,
    SameReadRepeatedWithoutNewContext,
    PendingTodosIgnoredAfterCompletionClaim,
    VerificationRepeatedWithoutChangedInputs,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResultTruncationContract {
    pub max_output_bytes: usize,
    pub preserve_json_shape: bool,
}

impl Default for ToolResultTruncationContract {
    fn default() -> Self {
        Self {
            max_output_bytes: DEFAULT_COMMAND_OUTPUT_BYTES,
            preserve_json_shape: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolResultTruncationMetadata {
    pub was_truncated: bool,
    pub original_bytes: usize,
    pub returned_bytes: usize,
    pub omitted_bytes: usize,
    pub max_output_bytes: usize,
}

impl ToolResultTruncationMetadata {
    fn unchanged(size: usize, max_output_bytes: usize) -> Self {
        Self {
            was_truncated: false,
            original_bytes: size,
            returned_bytes: size,
            omitted_bytes: 0,
            max_output_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolDescriptorV2 {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
    #[serde(default)]
    pub capability_tags: Vec<String>,
    #[serde(default)]
    pub application_metadata: ToolApplicationMetadata,
    pub effect_class: ToolEffectClass,
    pub mutability: ToolMutability,
    pub sandbox_requirement: ToolSandboxRequirement,
    pub approval_requirement: ToolApprovalRequirement,
    #[serde(default)]
    pub telemetry_attributes: BTreeMap<String, String>,
    pub result_truncation: ToolResultTruncationContract,
}

impl ToolDescriptorV2 {
    pub fn from_legacy_descriptor(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: JsonValue,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
            capability_tags: Vec::new(),
            application_metadata: ToolApplicationMetadata::default(),
            effect_class: ToolEffectClass::Observe,
            mutability: ToolMutability::ReadOnly,
            sandbox_requirement: ToolSandboxRequirement::None,
            approval_requirement: ToolApprovalRequirement::Policy,
            telemetry_attributes: BTreeMap::new(),
            result_truncation: ToolResultTruncationContract::default(),
        }
    }

    pub fn validate(&self) -> ToolRegistryResult<()> {
        if self.name.trim().is_empty() {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_descriptor_invalid",
                "Tool descriptors must include a non-empty name.",
            ));
        }
        if self.description.trim().is_empty() {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_descriptor_invalid",
                format!(
                    "Tool descriptor `{}` must include a description.",
                    self.name
                ),
            ));
        }
        if !self.input_schema.is_object() {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_descriptor_invalid",
                format!(
                    "Tool descriptor `{}` must provide an object-shaped input schema.",
                    self.name
                ),
            ));
        }
        if self.result_truncation.max_output_bytes == 0 {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_descriptor_invalid",
                format!(
                    "Tool descriptor `{}` must allow at least one output byte.",
                    self.name
                ),
            ));
        }

        let mut seen_tags = BTreeSet::new();
        for tag in &self.capability_tags {
            let normalized = tag.trim();
            if normalized.is_empty() || normalized != tag || !seen_tags.insert(normalized) {
                return Err(ToolExecutionError::invalid_input(
                    "agent_tool_descriptor_invalid",
                    format!(
                        "Tool descriptor `{}` has empty or duplicate capability tags.",
                        self.name
                    ),
                ));
            }
        }
        self.validate_application_metadata()?;
        Ok(())
    }

    fn validate_application_metadata(&self) -> ToolRegistryResult<()> {
        let metadata = &self.application_metadata;
        if metadata.family.trim().is_empty() {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_descriptor_invalid",
                format!(
                    "Tool descriptor `{}` must include an application metadata family.",
                    self.name
                ),
            ));
        }

        match metadata.kind {
            ToolApplicationKind::Granular => {
                if metadata.dispatch_safety != ToolBatchDispatchSafety::NotBatch {
                    return Err(ToolExecutionError::invalid_input(
                        "agent_tool_descriptor_invalid",
                        format!(
                            "Granular tool descriptor `{}` must use not_batch dispatch safety.",
                            self.name
                        ),
                    ));
                }
            }
            ToolApplicationKind::ReadOnlyBatch => {
                if !self.mutability.is_read_only() {
                    return Err(ToolExecutionError::invalid_input(
                        "agent_tool_descriptor_invalid",
                        format!(
                            "Read-only batch tool descriptor `{}` must be read-only.",
                            self.name
                        ),
                    ));
                }
                if metadata.dispatch_safety != ToolBatchDispatchSafety::ParallelReadOnly {
                    return Err(ToolExecutionError::invalid_input(
                        "agent_tool_descriptor_invalid",
                        format!(
                            "Read-only batch tool descriptor `{}` must use parallel_read_only dispatch safety.",
                            self.name
                        ),
                    ));
                }
                require_safety(metadata, &self.name, &["read_only"])?;
                require_safety(metadata, &self.name, &["bounded_results"])?;
            }
            ToolApplicationKind::Declarative | ToolApplicationKind::MutatingBatch => {
                if self.mutability == ToolMutability::Mutating {
                    if !matches!(
                        metadata.dispatch_safety,
                        ToolBatchDispatchSafety::SequentialMutating
                            | ToolBatchDispatchSafety::ToolOwnedAtomic
                    ) {
                        return Err(ToolExecutionError::invalid_input(
                            "agent_tool_descriptor_invalid",
                            format!(
                                "Mutating batch tool descriptor `{}` must use sequential_mutating or tool_owned_atomic dispatch safety.",
                                self.name
                            ),
                        ));
                    }
                    require_safety(
                        metadata,
                        &self.name,
                        &["supports_preview", "supports_dry_run"],
                    )?;
                    require_safety(
                        metadata,
                        &self.name,
                        &[
                            "validates_all_targets_before_writing",
                            "validates_targets_before_writing",
                        ],
                    )?;
                    require_safety(metadata, &self.name, &["reports_diff", "reports_summary"])?;
                }
            }
        }
        Ok(())
    }
}

fn require_safety(
    metadata: &ToolApplicationMetadata,
    tool_name: &str,
    accepted: &[&str],
) -> ToolRegistryResult<()> {
    if accepted.iter().any(|requirement| {
        metadata
            .safety_requirements
            .iter()
            .any(|item| item == requirement)
    }) {
        return Ok(());
    }
    Err(ToolExecutionError::invalid_input(
        "agent_tool_descriptor_invalid",
        format!(
            "Tool descriptor `{tool_name}` is missing required application safety metadata: one of {}.",
            accepted.join(", ")
        ),
    ))
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionPermissionManifest {
    pub permission_id: String,
    pub label: String,
    pub effect_class: ToolEffectClass,
    pub risk_class: String,
    pub audit_label: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExtensionRuntimeKind {
    Process,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionRuntimeManifest {
    pub kind: ToolExtensionRuntimeKind,
    pub executable: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionTestFixture {
    pub fixture_id: String,
    pub input: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_summary_contains: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolExtensionFixtureStatus {
    Passed,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionFixtureRun {
    pub fixture_id: String,
    pub status: ToolExtensionFixtureStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionFixtureReport {
    pub extension_id: String,
    pub tool_name: String,
    pub passed: bool,
    pub fixtures: Vec<ToolExtensionFixtureRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExtensionManifest {
    pub contract_version: u32,
    pub extension_id: String,
    pub tool_name: String,
    pub label: String,
    pub description: String,
    pub input_schema: JsonValue,
    pub permission: ToolExtensionPermissionManifest,
    pub mutability: ToolMutability,
    pub sandbox_requirement: ToolSandboxRequirement,
    pub approval_requirement: ToolApprovalRequirement,
    #[serde(default)]
    pub application_metadata: ToolApplicationMetadata,
    #[serde(default)]
    pub capability_tags: Vec<String>,
    #[serde(default)]
    pub test_fixtures: Vec<ToolExtensionTestFixture>,
    pub runtime: ToolExtensionRuntimeManifest,
}

impl ToolExtensionManifest {
    pub fn validate(&self) -> ToolRegistryResult<()> {
        if self.contract_version != TOOL_EXTENSION_MANIFEST_CONTRACT_VERSION {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_extension_contract_unsupported",
                format!(
                    "Tool extension `{}` uses contract version {}, but Xero supports {}.",
                    self.extension_id,
                    self.contract_version,
                    TOOL_EXTENSION_MANIFEST_CONTRACT_VERSION
                ),
            ));
        }
        validate_extension_identifier("extensionId", &self.extension_id)?;
        validate_extension_identifier("toolName", &self.tool_name)?;
        validate_extension_text("label", &self.label)?;
        validate_extension_text("description", &self.description)?;
        if !self.input_schema.is_object() {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_extension_schema_invalid",
                format!(
                    "Tool extension `{}` must provide an object-shaped input schema.",
                    self.extension_id
                ),
            ));
        }
        self.validate_permission()?;
        self.validate_runtime()?;
        let descriptor = self.descriptor();
        descriptor.validate()?;
        self.validate_fixtures()?;
        Ok(())
    }

    pub fn descriptor(&self) -> ToolDescriptorV2 {
        ToolDescriptorV2 {
            name: self.tool_name.clone(),
            description: self.description.clone(),
            input_schema: self.input_schema.clone(),
            capability_tags: self.capability_tags.clone(),
            application_metadata: self.application_metadata.clone(),
            effect_class: self.permission.effect_class.clone(),
            mutability: self.mutability,
            sandbox_requirement: self.sandbox_requirement,
            approval_requirement: self.approval_requirement,
            telemetry_attributes: BTreeMap::from([
                ("xero.extension.id".into(), self.extension_id.clone()),
                (
                    "xero.extension.permission_id".into(),
                    self.permission.permission_id.clone(),
                ),
                (
                    "xero.extension.audit_label".into(),
                    self.permission.audit_label.clone(),
                ),
                (
                    "xero.extension.risk_class".into(),
                    self.permission.risk_class.clone(),
                ),
                (
                    "xero.extension.contract_version".into(),
                    self.contract_version.to_string(),
                ),
            ]),
            result_truncation: ToolResultTruncationContract::default(),
        }
    }

    fn validate_permission(&self) -> ToolRegistryResult<()> {
        validate_extension_identifier("permissionId", &self.permission.permission_id)?;
        validate_extension_text("permission.label", &self.permission.label)?;
        validate_extension_identifier("permission.riskClass", &self.permission.risk_class)?;
        validate_extension_text("permission.auditLabel", &self.permission.audit_label)?;
        Ok(())
    }

    fn validate_runtime(&self) -> ToolRegistryResult<()> {
        let executable = self.runtime.executable.trim();
        let mut components = Path::new(executable).components();
        if executable.is_empty()
            || executable.len() != self.runtime.executable.len()
            || !matches!(components.next(), Some(Component::Normal(_)))
            || components.next().is_some()
        {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_extension_executable_invalid",
                format!(
                    "Tool extension `{}` must declare one bundle-local executable filename without path traversal.",
                    self.extension_id
                ),
            ));
        }
        if self
            .runtime
            .args
            .iter()
            .any(|argument| argument.contains('\0') || argument.len() > 4_096)
        {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_extension_arguments_invalid",
                format!(
                    "Tool extension `{}` declares an invalid runtime argument.",
                    self.extension_id
                ),
            ));
        }
        Ok(())
    }

    fn validate_fixtures(&self) -> ToolRegistryResult<()> {
        if self.test_fixtures.is_empty() {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_extension_fixture_missing",
                format!(
                    "Tool extension `{}` must declare at least one executable test fixture.",
                    self.extension_id
                ),
            ));
        }
        let mut seen = BTreeSet::new();
        for fixture in &self.test_fixtures {
            validate_extension_identifier("fixtureId", &fixture.fixture_id)?;
            if !seen.insert(fixture.fixture_id.as_str()) {
                return Err(ToolExecutionError::invalid_input(
                    "agent_tool_extension_fixture_duplicate",
                    format!(
                        "Tool extension `{}` declares duplicate fixture `{}`.",
                        self.extension_id, fixture.fixture_id
                    ),
                ));
            }
            if !fixture.input.is_object() {
                return Err(ToolExecutionError::invalid_input(
                    "agent_tool_extension_fixture_invalid",
                    format!(
                        "Tool extension `{}` fixture `{}` must provide object-shaped input.",
                        self.extension_id, fixture.fixture_id
                    ),
                ));
            }
            if fixture
                .expected_summary_contains
                .as_deref()
                .is_some_and(|value| value.trim().is_empty())
            {
                return Err(ToolExecutionError::invalid_input(
                    "agent_tool_extension_fixture_invalid",
                    format!(
                        "Tool extension `{}` fixture `{}` has an empty expected summary fragment.",
                        self.extension_id, fixture.fixture_id
                    ),
                ));
            }
        }
        Ok(())
    }
}

fn validate_extension_identifier(field: &str, value: &str) -> ToolRegistryResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() != value.len()
        || !trimmed.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | ':' | '/')
        })
    {
        return Err(ToolExecutionError::invalid_input(
            "agent_tool_extension_identifier_invalid",
            format!(
                "Tool extension field `{field}` must be a non-empty stable identifier using ASCII letters, numbers, '.', '-', '_', ':', or '/'."
            ),
        ));
    }
    Ok(())
}

fn validate_extension_text(field: &str, value: &str) -> ToolRegistryResult<()> {
    if value.trim().is_empty() {
        return Err(ToolExecutionError::invalid_input(
            "agent_tool_extension_text_invalid",
            format!("Tool extension field `{field}` must be non-empty."),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolCallInput {
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExecutionContext {
    pub project_id: String,
    pub run_id: String,
    pub turn_index: usize,
    #[serde(default)]
    pub context_epoch: String,
    #[serde(default)]
    pub telemetry_attributes: BTreeMap<String, String>,
}

impl Default for ToolExecutionContext {
    fn default() -> Self {
        Self {
            project_id: "project".into(),
            run_id: "run".into(),
            turn_index: 0,
            context_epoch: "initial".into(),
            telemetry_attributes: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl ToolCancellationToken {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for ToolCancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone)]
pub struct ToolExecutionControl {
    pub deadline: Option<Instant>,
    pub cancellation_token: ToolCancellationToken,
}

impl ToolExecutionControl {
    pub fn new(deadline: Option<Instant>, cancellation_token: ToolCancellationToken) -> Self {
        Self {
            deadline,
            cancellation_token,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancellation_token.is_cancelled()
            || self
                .deadline
                .is_some_and(|deadline| Instant::now() >= deadline)
    }

    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    pub fn ensure_not_cancelled(&self, tool_name: &str) -> ToolRegistryResult<()> {
        if self.is_cancelled() {
            return Err(ToolExecutionError::timeout(
                "agent_tool_call_cancelled",
                format!("Tool `{tool_name}` was cancelled because its group deadline expired."),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolHandlerOutput {
    pub summary: String,
    pub output: JsonValue,
    #[serde(default)]
    pub telemetry_attributes: BTreeMap<String, String>,
}

impl ToolHandlerOutput {
    pub fn new(summary: impl Into<String>, output: JsonValue) -> Self {
        Self {
            summary: summary.into(),
            output,
            telemetry_attributes: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {model_message}")]
pub struct ToolExecutionError {
    pub category: ToolErrorCategory,
    pub code: String,
    pub message: String,
    pub model_message: String,
    pub retryable: bool,
    #[serde(default)]
    pub telemetry_attributes: BTreeMap<String, String>,
}

impl ToolExecutionError {
    pub fn invalid_input(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::InvalidInput,
            code,
            message,
            false,
            "The tool input was invalid. Check the tool schema and try again.",
        )
    }

    pub fn policy_denied(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::PolicyDenied,
            code,
            message,
            false,
            "Xero denied the tool call under the active safety policy.",
        )
    }

    pub fn approval_required(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::ApprovalRequired,
            code,
            message,
            false,
            "The tool call is waiting for operator approval.",
        )
    }

    pub fn sandbox_denied(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::SandboxDenied,
            code,
            message,
            false,
            "Xero blocked the tool call in the active sandbox profile.",
        )
    }

    pub fn timeout(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::Timeout,
            code,
            message,
            true,
            "The tool group exceeded its wall-clock budget.",
        )
    }

    pub fn unavailable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::ToolUnavailable,
            code,
            message,
            false,
            "The requested tool is not available in the active registry.",
        )
    }

    pub fn budget_exceeded(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::BudgetExceeded,
            code,
            message,
            false,
            "Xero stopped tool execution because a tool budget was exceeded.",
        )
    }

    pub fn doom_loop(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::DoomLoopDetected,
            code,
            message,
            false,
            "Xero detected a repeated tool loop and stopped this path.",
        )
    }

    pub fn retryable(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(
            ToolErrorCategory::RetryableProviderToolFailure,
            code,
            message,
            true,
            "The tool failed in a retryable way. Change the input or gather new context before retrying.",
        )
    }

    pub fn new(
        category: ToolErrorCategory,
        code: impl Into<String>,
        message: impl Into<String>,
        retryable: bool,
        model_message: impl Into<String>,
    ) -> Self {
        Self {
            category,
            code: code.into(),
            message: message.into(),
            model_message: model_message.into(),
            retryable,
            telemetry_attributes: BTreeMap::new(),
        }
    }
}

impl Default for ToolExecutionControl {
    fn default() -> Self {
        Self::new(None, ToolCancellationToken::default())
    }
}

pub trait ToolHandler: Send + Sync {
    fn descriptor(&self) -> ToolDescriptorV2;

    /// Returns true when the handler controls process-local runtime state that
    /// cannot survive execution in a short-lived mutation worker. The handler
    /// still runs through policy, sandbox, checkpoint, rollback, budget, and
    /// cancellation gates, but executes in the supervising process.
    fn requires_parent_process_execution(&self, _call: &ToolCallInput) -> bool {
        false
    }

    fn validate_input(&self, input: &JsonValue) -> ToolRegistryResult<()> {
        validate_input_against_schema(&self.descriptor(), input)
    }

    fn pre_hook_payload(&self, call: &ToolCallInput) -> JsonValue {
        let descriptor = self.descriptor();
        json!({
            "toolCallId": call.tool_call_id,
            "toolName": call.tool_name,
            "effectClass": descriptor.effect_class,
            "mutability": descriptor.mutability,
            "sandboxRequirement": descriptor.sandbox_requirement,
            "approvalRequirement": descriptor.approval_requirement,
            "capabilityTags": descriptor.capability_tags,
            "applicationMetadata": descriptor.application_metadata,
        })
    }

    fn execute(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> ToolRegistryResult<ToolHandlerOutput>;

    fn execute_with_control(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
        control: &ToolExecutionControl,
    ) -> ToolRegistryResult<ToolHandlerOutput> {
        let _ = control;
        self.execute(context, call)
    }

    fn post_hook_payload(
        &self,
        call: &ToolCallInput,
        result: &Result<ToolHandlerOutput, ToolExecutionError>,
    ) -> JsonValue {
        json!({
            "toolCallId": call.tool_call_id,
            "toolName": call.tool_name,
            "ok": result.is_ok(),
        })
    }
}

type StaticToolExecute = dyn Fn(
        &ToolExecutionContext,
        &ToolCallInput,
        &ToolExecutionControl,
    ) -> ToolRegistryResult<ToolHandlerOutput>
    + Send
    + Sync;

pub struct StaticToolHandler {
    descriptor: ToolDescriptorV2,
    execute: Arc<StaticToolExecute>,
}

impl StaticToolHandler {
    pub fn new<F>(descriptor: ToolDescriptorV2, execute: F) -> Self
    where
        F: Fn(&ToolExecutionContext, &ToolCallInput) -> ToolRegistryResult<ToolHandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        Self {
            descriptor,
            execute: Arc::new(move |context, call, _control| execute(context, call)),
        }
    }

    pub fn new_cancellable<F>(descriptor: ToolDescriptorV2, execute: F) -> Self
    where
        F: Fn(
                &ToolExecutionContext,
                &ToolCallInput,
                &ToolExecutionControl,
            ) -> ToolRegistryResult<ToolHandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        Self {
            descriptor,
            execute: Arc::new(execute),
        }
    }

    pub fn try_from_extension_manifest<F>(
        manifest: ToolExtensionManifest,
        execute: F,
    ) -> ToolRegistryResult<Self>
    where
        F: Fn(&ToolExecutionContext, &ToolCallInput) -> ToolRegistryResult<ToolHandlerOutput>
            + Send
            + Sync
            + 'static,
    {
        manifest.validate()?;
        Ok(Self::new(manifest.descriptor(), execute))
    }

    pub fn verify_extension_test_fixtures(
        &self,
        manifest: &ToolExtensionManifest,
        context: &ToolExecutionContext,
    ) -> ToolRegistryResult<ToolExtensionFixtureReport> {
        manifest.validate()?;
        if self.descriptor.name != manifest.tool_name {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_extension_handler_mismatch",
                format!(
                    "Tool extension `{}` fixtures target `{}`, but the handler is registered as `{}`.",
                    manifest.extension_id, manifest.tool_name, self.descriptor.name
                ),
            ));
        }

        let fixtures = manifest
            .test_fixtures
            .iter()
            .map(|fixture| {
                let call = ToolCallInput {
                    tool_call_id: format!("fixture:{}", fixture.fixture_id),
                    tool_name: manifest.tool_name.clone(),
                    input: fixture.input.clone(),
                };
                match self.execute_with_control(context, &call, &ToolExecutionControl::default()) {
                    Ok(output) => {
                        let summary = output.summary;
                        if let Some(expected) = fixture.expected_summary_contains.as_deref() {
                            if !summary.contains(expected) {
                                return ToolExtensionFixtureRun {
                                    fixture_id: fixture.fixture_id.clone(),
                                    status: ToolExtensionFixtureStatus::Failed,
                                    summary: Some(summary),
                                    diagnostic: Some(format!(
                                        "Expected fixture summary to contain `{expected}`."
                                    )),
                                };
                            }
                        }
                        ToolExtensionFixtureRun {
                            fixture_id: fixture.fixture_id.clone(),
                            status: ToolExtensionFixtureStatus::Passed,
                            summary: Some(summary),
                            diagnostic: None,
                        }
                    }
                    Err(error) => ToolExtensionFixtureRun {
                        fixture_id: fixture.fixture_id.clone(),
                        status: ToolExtensionFixtureStatus::Failed,
                        summary: None,
                        diagnostic: Some(format!("{}: {}", error.code, error.message)),
                    },
                }
            })
            .collect::<Vec<_>>();
        let passed = fixtures
            .iter()
            .all(|fixture| fixture.status == ToolExtensionFixtureStatus::Passed);

        Ok(ToolExtensionFixtureReport {
            extension_id: manifest.extension_id.clone(),
            tool_name: manifest.tool_name.clone(),
            passed,
            fixtures,
        })
    }
}

impl ToolHandler for StaticToolHandler {
    fn descriptor(&self) -> ToolDescriptorV2 {
        self.descriptor.clone()
    }

    fn execute(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
    ) -> ToolRegistryResult<ToolHandlerOutput> {
        (self.execute)(context, call, &ToolExecutionControl::default())
    }

    fn execute_with_control(
        &self,
        context: &ToolExecutionContext,
        call: &ToolCallInput,
        control: &ToolExecutionControl,
    ) -> ToolRegistryResult<ToolHandlerOutput> {
        (self.execute)(context, call, control)
    }
}

#[derive(Clone)]
pub struct ToolDispatchConfig {
    pub budget: ToolBudget,
    pub policy: Arc<dyn ToolPolicy>,
    pub sandbox: Arc<dyn ToolSandbox>,
    pub rollback: Option<Arc<dyn ToolRollback>>,
    pub context: ToolExecutionContext,
    pub cancellation_check: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
}

impl Default for ToolDispatchConfig {
    fn default() -> Self {
        Self {
            budget: ToolBudget::default(),
            policy: Arc::new(AllowAllToolPolicy),
            sandbox: Arc::new(NoopToolSandbox),
            rollback: None,
            context: ToolExecutionContext::default(),
            cancellation_check: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolBudget {
    pub max_tool_calls_per_turn: usize,
    pub max_tool_failures_per_turn: usize,
    pub max_repeated_equivalent_calls: usize,
    pub max_command_output_bytes: usize,
    pub max_wall_clock_time_per_tool_group_ms: u64,
    pub max_mutation_cleanup_ms: u64,
}

impl Default for ToolBudget {
    fn default() -> Self {
        Self {
            max_tool_calls_per_turn: DEFAULT_TOOL_CALL_LIMIT,
            max_tool_failures_per_turn: DEFAULT_TOOL_FAILURE_LIMIT,
            max_repeated_equivalent_calls: DEFAULT_REPEATED_EQUIVALENT_CALL_LIMIT,
            max_command_output_bytes: DEFAULT_COMMAND_OUTPUT_BYTES,
            max_wall_clock_time_per_tool_group_ms: DEFAULT_GROUP_WALL_CLOCK_MS,
            max_mutation_cleanup_ms: DEFAULT_MUTATION_CLEANUP_MS,
        }
    }
}

#[derive(Debug, Default)]
pub struct ToolBudgetTracker {
    budget: ToolBudget,
    tool_calls: usize,
    tool_failures: usize,
    equivalent_calls: BTreeMap<String, usize>,
    doom_loop: ToolDoomLoopGuard,
}

impl ToolBudgetTracker {
    pub fn new(budget: ToolBudget) -> Self {
        Self {
            budget,
            tool_calls: 0,
            tool_failures: 0,
            equivalent_calls: BTreeMap::new(),
            doom_loop: ToolDoomLoopGuard::default(),
        }
    }

    pub fn record_call(&mut self, call: &ToolCallInput) -> ToolRegistryResult<()> {
        self.tool_calls = self.tool_calls.saturating_add(1);
        if self.tool_calls > self.budget.max_tool_calls_per_turn {
            return Err(ToolExecutionError::budget_exceeded(
                "agent_tool_budget_calls_exceeded",
                format!(
                    "The turn exceeded the maximum of {} tool call(s).",
                    self.budget.max_tool_calls_per_turn
                ),
            ));
        }

        let signature = tool_call_signature(call);
        let repeated = self
            .equivalent_calls
            .entry(signature)
            .and_modify(|count| *count = count.saturating_add(1))
            .or_insert(1);
        if *repeated > self.budget.max_repeated_equivalent_calls {
            return Err(ToolExecutionError::budget_exceeded(
                "agent_tool_budget_repeated_equivalent_calls",
                format!(
                    "The model repeated equivalent tool call `{}` more than {} time(s).",
                    call.tool_name, self.budget.max_repeated_equivalent_calls
                ),
            ));
        }
        Ok(())
    }

    pub fn record_failure(
        &mut self,
        call: &ToolCallInput,
        error: &ToolExecutionError,
    ) -> ToolRegistryResult<Option<ToolDoomLoopSignal>> {
        self.tool_failures = self.tool_failures.saturating_add(1);
        if self.tool_failures > self.budget.max_tool_failures_per_turn {
            return Err(ToolExecutionError::budget_exceeded(
                "agent_tool_budget_failures_exceeded",
                format!(
                    "The turn exceeded the maximum of {} tool failure(s).",
                    self.budget.max_tool_failures_per_turn
                ),
            ));
        }
        Ok(self.doom_loop.record_failure(call, error))
    }

    pub fn doom_loop_mut(&mut self) -> &mut ToolDoomLoopGuard {
        &mut self.doom_loop
    }
}

#[derive(Debug, Default)]
pub struct ToolDoomLoopGuard {
    failing_signatures: BTreeMap<String, usize>,
    read_signatures_by_context: BTreeMap<(String, String), usize>,
    verification_requests: BTreeMap<(String, String), usize>,
}

impl ToolDoomLoopGuard {
    pub fn record_failure(
        &mut self,
        call: &ToolCallInput,
        error: &ToolExecutionError,
    ) -> Option<ToolDoomLoopSignal> {
        if error.category == ToolErrorCategory::InvalidInput {
            return None;
        }
        let signature = tool_call_signature(call);
        let count = self
            .failing_signatures
            .entry(signature)
            .and_modify(|value| *value = value.saturating_add(1))
            .or_insert(1);
        (*count >= 2).then_some(ToolDoomLoopSignal::SameFailingToolRepeated)
    }

    pub fn record_file_read(
        &mut self,
        call: &ToolCallInput,
        context_epoch: impl Into<String>,
    ) -> Option<ToolDoomLoopSignal> {
        let key = (tool_call_signature(call), context_epoch.into());
        let count = self
            .read_signatures_by_context
            .entry(key)
            .and_modify(|value| *value = value.saturating_add(1))
            .or_insert(1);
        (*count >= 2).then_some(ToolDoomLoopSignal::SameReadRepeatedWithoutNewContext)
    }

    pub fn record_completion_claim(
        &self,
        pending_todo_count: usize,
        claimed_completion: bool,
    ) -> Option<ToolDoomLoopSignal> {
        (claimed_completion && pending_todo_count > 0)
            .then_some(ToolDoomLoopSignal::PendingTodosIgnoredAfterCompletionClaim)
    }

    pub fn record_verification_request(
        &mut self,
        verification_key: impl Into<String>,
        input_change_token: impl Into<String>,
    ) -> Option<ToolDoomLoopSignal> {
        let key = (verification_key.into(), input_change_token.into());
        let count = self
            .verification_requests
            .entry(key)
            .and_modify(|value| *value = value.saturating_add(1))
            .or_insert(1);
        (*count >= 2).then_some(ToolDoomLoopSignal::VerificationRepeatedWithoutChangedInputs)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "decision", content = "payload")]
pub enum ToolPolicyDecision {
    Allow,
    Deny { code: String, message: String },
    RequireApproval { action_id: String, message: String },
}

pub trait ToolPolicy: Send + Sync {
    fn evaluate(&self, descriptor: &ToolDescriptorV2, call: &ToolCallInput) -> ToolPolicyDecision;
}

#[derive(Debug, Clone, Copy)]
pub struct AllowAllToolPolicy;

impl ToolPolicy for AllowAllToolPolicy {
    fn evaluate(&self, descriptor: &ToolDescriptorV2, call: &ToolCallInput) -> ToolPolicyDecision {
        if descriptor.approval_requirement == ToolApprovalRequirement::Always {
            return ToolPolicyDecision::RequireApproval {
                action_id: format!("approve-tool-{}", call.tool_call_id),
                message: format!("Tool `{}` requires approval.", descriptor.name),
            };
        }
        ToolPolicyDecision::Allow
    }
}

#[derive(Debug, Clone, Default)]
pub struct StaticToolPolicy {
    denied_tools: BTreeMap<String, String>,
    approval_required_tools: BTreeMap<String, String>,
}

impl StaticToolPolicy {
    pub fn deny_tool(mut self, tool_name: impl Into<String>, reason: impl Into<String>) -> Self {
        self.denied_tools.insert(tool_name.into(), reason.into());
        self
    }

    pub fn require_approval_for_tool(
        mut self,
        tool_name: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        self.approval_required_tools
            .insert(tool_name.into(), reason.into());
        self
    }
}

impl ToolPolicy for StaticToolPolicy {
    fn evaluate(&self, descriptor: &ToolDescriptorV2, call: &ToolCallInput) -> ToolPolicyDecision {
        if let Some(reason) = self.denied_tools.get(&descriptor.name) {
            return ToolPolicyDecision::Deny {
                code: "agent_tool_policy_denied".into(),
                message: reason.clone(),
            };
        }
        if descriptor.approval_requirement == ToolApprovalRequirement::Always {
            return ToolPolicyDecision::RequireApproval {
                action_id: format!("approve-tool-{}", call.tool_call_id),
                message: format!("Tool `{}` requires approval.", descriptor.name),
            };
        }
        if let Some(reason) = self.approval_required_tools.get(&descriptor.name) {
            return ToolPolicyDecision::RequireApproval {
                action_id: format!("approve-tool-{}", call.tool_call_id),
                message: reason.clone(),
            };
        }
        ToolPolicyDecision::Allow
    }
}

pub trait ToolRollback: Send + Sync {
    /// Returns true when checkpoint and rollback callbacks depend on process-local state that is
    /// not safe to enter after `fork` (for example SQLite, async runtimes, or shared mutexes).
    /// The mutation handler remains isolated, while these bookkeeping callbacks run in the
    /// supervising parent process.
    fn requires_parent_process(&self) -> bool {
        false
    }

    fn checkpoint_before(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
    ) -> ToolRegistryResult<Option<JsonValue>>;

    fn rollback_after_failure(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
        checkpoint: &JsonValue,
        error: &ToolExecutionError,
    ) -> ToolRegistryResult<JsonValue>;

    fn recover_after_termination(
        &self,
        call: &ToolCallInput,
        descriptor: &ToolDescriptorV2,
        checkpoint: Option<&JsonValue>,
        error: &ToolExecutionError,
    ) -> ToolRegistryResult<JsonValue> {
        let checkpoint = checkpoint.ok_or_else(|| {
            ToolExecutionError::retryable(
                "agent_tool_mutation_checkpoint_unavailable",
                format!(
                    "Xero cannot automatically recover terminated tool `{}` because its checkpoint phase did not complete.",
                    call.tool_name
                ),
            )
        })?;
        self.rollback_after_failure(call, descriptor, checkpoint, error)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolDispatchSuccess {
    pub tool_call_id: String,
    pub tool_name: String,
    pub summary: String,
    pub output: JsonValue,
    pub truncation: ToolResultTruncationMetadata,
    pub pre_hook_payload: JsonValue,
    pub post_hook_payload: JsonValue,
    #[serde(default)]
    pub telemetry_attributes: BTreeMap<String, String>,
    pub elapsed_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_metadata: Option<SandboxExecutionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolDispatchFailure {
    pub tool_call_id: String,
    pub tool_name: String,
    pub error: ToolExecutionError,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub doom_loop_signal: Option<ToolDoomLoopSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_payload: Option<JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rollback_error: Option<ToolExecutionError>,
    pub pre_hook_payload: JsonValue,
    pub post_hook_payload: JsonValue,
    pub elapsed_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox_metadata: Option<SandboxExecutionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "state", content = "payload")]
pub enum ToolDispatchOutcome {
    Succeeded(ToolDispatchSuccess),
    Failed(ToolDispatchFailure),
}

impl ToolDispatchOutcome {
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Succeeded(_))
    }

    pub fn failure(&self) -> Option<&ToolDispatchFailure> {
        match self {
            Self::Failed(failure) => Some(failure),
            Self::Succeeded(_) => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ToolGroupExecutionMode {
    ParallelReadOnly,
    SequentialMutating,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolExecutionGroup {
    pub mode: ToolGroupExecutionMode,
    pub calls: Vec<ToolCallInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolGroupDispatchReport {
    pub mode: ToolGroupExecutionMode,
    pub elapsed_ms: u128,
    pub outcomes: Vec<ToolDispatchOutcome>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_error: Option<ToolExecutionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolBatchDispatchReport {
    pub groups: Vec<ToolGroupDispatchReport>,
}

#[derive(Default)]
struct ReadOnlyWorkerState {
    workers: BTreeMap<u64, Option<JoinHandle<()>>>,
    completed_worker_ids: BTreeSet<u64>,
}

struct ReadOnlyWorkerSupervisor {
    max_workers: usize,
    next_worker_id: AtomicU64,
    state: Mutex<ReadOnlyWorkerState>,
    capacity_available: Condvar,
}

impl ReadOnlyWorkerSupervisor {
    fn new(max_workers: usize) -> Self {
        Self {
            max_workers: max_workers.max(1),
            next_worker_id: AtomicU64::new(1),
            state: Mutex::new(ReadOnlyWorkerState::default()),
            capacity_available: Condvar::new(),
        }
    }

    fn spawn_until<F>(self: &Arc<Self>, deadline: Instant, job: F) -> ToolRegistryResult<u64>
    where
        F: FnOnce() + Send + 'static,
    {
        let mut job = Some(job);
        loop {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let completed_worker_ids = std::mem::take(&mut state.completed_worker_ids);
            let mut completed_handles = Vec::new();
            for worker_id in completed_worker_ids {
                match state.workers.get(&worker_id) {
                    Some(Some(_)) => {
                        if let Some(handle) = state.workers.remove(&worker_id).flatten() {
                            completed_handles.push(handle);
                        }
                    }
                    Some(None) => {
                        state.completed_worker_ids.insert(worker_id);
                    }
                    None => {}
                }
            }

            if state.workers.len() < self.max_workers {
                let worker_id = self.next_worker_id.fetch_add(1, Ordering::Relaxed);
                let Some(job) = job.take() else {
                    drop(state);
                    join_supervised_workers(completed_handles);
                    return Err(ToolExecutionError::unavailable(
                        "agent_tool_worker_job_missing",
                        "Xero lost a queued read-only tool job before it started.",
                    ));
                };
                state.workers.insert(worker_id, None);
                drop(state);
                join_supervised_workers(completed_handles);
                let completion = ReadOnlyWorkerCompletionNotifier {
                    worker_id,
                    supervisor: Arc::downgrade(self),
                };
                let handle = match thread::Builder::new()
                    .name(format!("xero-read-tool-{worker_id}"))
                    .spawn(move || {
                        let _completion = completion;
                        job();
                    }) {
                    Ok(handle) => handle,
                    Err(error) => {
                        let mut state = self
                            .state
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner());
                        state.workers.remove(&worker_id);
                        state.completed_worker_ids.remove(&worker_id);
                        drop(state);
                        self.capacity_available.notify_all();
                        return Err(ToolExecutionError::unavailable(
                            "agent_tool_worker_spawn_failed",
                            format!("Xero could not spawn a read-only tool worker: {error}"),
                        ));
                    }
                };
                let mut state = self
                    .state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.workers.insert(worker_id, Some(handle));
                let completed = state.completed_worker_ids.contains(&worker_id);
                drop(state);
                if completed {
                    self.capacity_available.notify_all();
                }
                return Ok(worker_id);
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let retained_worker_count = state.workers.len();
                drop(state);
                join_supervised_workers(completed_handles);
                return Err(ToolExecutionError::timeout(
                    "agent_tool_worker_capacity_exhausted",
                    format!(
                        "Xero retained {retained_worker_count} active or non-cooperative read-only tool workers, and capacity did not become available before the tool-group deadline."
                    ),
                ));
            }

            debug_assert!(completed_handles.is_empty());
            let (next_state, _) = self
                .capacity_available
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            drop(next_state);
        }
    }

    fn join(&self, worker_id: u64) {
        let handle = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.completed_worker_ids.remove(&worker_id);
            state.workers.remove(&worker_id).flatten()
        };
        if let Some(handle) = handle {
            self.capacity_available.notify_all();
            let _ = handle.join();
        }
    }

    fn mark_completed(&self, worker_id: u64) {
        let notify = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state.workers.contains_key(&worker_id) && state.completed_worker_ids.insert(worker_id)
        };
        if notify {
            self.capacity_available.notify_all();
        }
    }
}

struct ReadOnlyWorkerCompletionNotifier {
    worker_id: u64,
    supervisor: Weak<ReadOnlyWorkerSupervisor>,
}

impl Drop for ReadOnlyWorkerCompletionNotifier {
    fn drop(&mut self) {
        if let Some(supervisor) = self.supervisor.upgrade() {
            supervisor.mark_completed(self.worker_id);
        }
    }
}

fn join_supervised_workers(handles: Vec<JoinHandle<()>>) {
    for handle in handles {
        let _ = handle.join();
    }
}

fn global_read_only_worker_supervisor() -> Arc<ReadOnlyWorkerSupervisor> {
    // Production registries are rebuilt for each tool batch. Keep timed-out worker handles
    // process-wide so non-cooperative handlers cannot become untracked, unbounded threads.
    static SUPERVISOR: OnceLock<Arc<ReadOnlyWorkerSupervisor>> = OnceLock::new();
    Arc::clone(SUPERVISOR.get_or_init(|| {
        Arc::new(ReadOnlyWorkerSupervisor::new(
            DEFAULT_MAX_SUPERVISED_READ_ONLY_WORKERS,
        ))
    }))
}

#[derive(Clone)]
struct MutationQuarantine {
    project_id: String,
    call: ToolCallInput,
    descriptor: ToolDescriptorV2,
    checkpoint: Option<JsonValue>,
    error: ToolExecutionError,
    rollback_error: ToolExecutionError,
    phase: MutationWorkerPhase,
    rollback: Option<Arc<dyn ToolRollback>>,
}

#[derive(Default)]
struct MutationExecutionState {
    quarantines: BTreeMap<String, MutationQuarantine>,
}

fn mutation_execution_scope(context: &ToolExecutionContext) -> String {
    context
        .telemetry_attributes
        .get(MUTATION_EXECUTION_SCOPE_ATTRIBUTE)
        .filter(|scope| !scope.trim().is_empty())
        .map(|scope| format!("{}\u{1f}{scope}", context.project_id))
        .unwrap_or_else(|| context.project_id.clone())
}

fn global_mutation_execution_state() -> Arc<Mutex<MutationExecutionState>> {
    static MUTATION_STATE: OnceLock<Arc<Mutex<MutationExecutionState>>> = OnceLock::new();
    Arc::clone(
        MUTATION_STATE.get_or_init(|| Arc::new(Mutex::new(MutationExecutionState::default()))),
    )
}

pub struct ToolRegistryV2 {
    handlers: BTreeMap<String, Arc<dyn ToolHandler>>,
    read_only_worker_supervisor: Arc<ReadOnlyWorkerSupervisor>,
    mutation_execution_state: Arc<Mutex<MutationExecutionState>>,
    mutation_boundary: MutationBoundary,
}

#[derive(Clone, Copy)]
enum MutationBoundary {
    TerminableProcess,
    #[cfg(any(test, feature = "test-support"))]
    CooperativeTestHarness,
}

impl Default for ToolRegistryV2 {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolRegistryV2 {
    pub fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
            read_only_worker_supervisor: global_read_only_worker_supervisor(),
            mutation_execution_state: global_mutation_execution_state(),
            mutation_boundary: MutationBoundary::TerminableProcess,
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    #[doc(hidden)]
    pub fn with_cooperative_mutation_boundary_for_tests(mut self) -> Self {
        self.mutation_boundary = MutationBoundary::CooperativeTestHarness;
        self
    }

    #[cfg(test)]
    fn with_read_only_worker_limit(max_workers: usize) -> Self {
        Self {
            handlers: BTreeMap::new(),
            read_only_worker_supervisor: Arc::new(ReadOnlyWorkerSupervisor::new(max_workers)),
            mutation_execution_state: Arc::new(Mutex::new(MutationExecutionState::default())),
            mutation_boundary: MutationBoundary::TerminableProcess,
        }
    }

    pub fn register<H>(&mut self, handler: H) -> ToolRegistryResult<()>
    where
        H: ToolHandler + 'static,
    {
        self.register_arc(Arc::new(handler))
    }

    pub fn register_arc(&mut self, handler: Arc<dyn ToolHandler>) -> ToolRegistryResult<()> {
        let descriptor = handler.descriptor();
        descriptor.validate()?;
        if self.handlers.contains_key(&descriptor.name) {
            return Err(ToolExecutionError::invalid_input(
                "agent_tool_descriptor_duplicate",
                format!("Tool `{}` is already registered.", descriptor.name),
            ));
        }
        self.handlers.insert(descriptor.name, handler);
        Ok(())
    }

    pub fn descriptor(&self, tool_name: &str) -> Option<ToolDescriptorV2> {
        self.handlers
            .get(tool_name)
            .map(|handler| handler.descriptor())
    }

    pub fn descriptors(&self) -> Vec<ToolDescriptorV2> {
        self.handlers
            .values()
            .map(|handler| handler.descriptor())
            .collect()
    }

    pub fn plan_batch(&self, calls: &[ToolCallInput]) -> Vec<ToolExecutionGroup> {
        let mut groups = Vec::new();
        let mut read_only_calls = Vec::new();

        for call in calls {
            let is_read_only = self
                .descriptor(&call.tool_name)
                .map(|descriptor| descriptor.mutability.is_read_only())
                .unwrap_or(false);

            if is_read_only {
                read_only_calls.push(call.clone());
                continue;
            }

            if !read_only_calls.is_empty() {
                groups.push(ToolExecutionGroup {
                    mode: ToolGroupExecutionMode::ParallelReadOnly,
                    calls: std::mem::take(&mut read_only_calls),
                });
            }
            groups.push(ToolExecutionGroup {
                mode: ToolGroupExecutionMode::SequentialMutating,
                calls: vec![call.clone()],
            });
        }

        if !read_only_calls.is_empty() {
            groups.push(ToolExecutionGroup {
                mode: ToolGroupExecutionMode::ParallelReadOnly,
                calls: read_only_calls,
            });
        }
        groups
    }

    pub fn dispatch_call(
        &self,
        call: ToolCallInput,
        tracker: &mut ToolBudgetTracker,
        config: &ToolDispatchConfig,
    ) -> ToolDispatchOutcome {
        let deadline = group_deadline(&config.budget);
        self.dispatch_prepared(call, tracker, config, deadline)
    }

    pub fn dispatch_batch(
        &self,
        calls: &[ToolCallInput],
        config: &ToolDispatchConfig,
    ) -> ToolBatchDispatchReport {
        match self.dispatch_batch_with_group_hook::<std::convert::Infallible, _>(
            calls,
            config,
            |_| Ok(()),
        ) {
            Ok(report) => report,
            Err(error) => match error {},
        }
    }

    /// Dispatch a batch and run a synchronous hook after each execution group.
    ///
    /// Mutating calls are isolated into individual sequential groups. Consumers
    /// that persist parent-process state for isolated mutations can use this
    /// hook to make each successful mutation observable before the next group
    /// starts, without giving up parallel dispatch for read-only groups or
    /// resetting the shared batch budget.
    pub fn dispatch_batch_with_group_hook<E, F>(
        &self,
        calls: &[ToolCallInput],
        config: &ToolDispatchConfig,
        mut after_group: F,
    ) -> Result<ToolBatchDispatchReport, E>
    where
        F: FnMut(&mut ToolGroupDispatchReport) -> Result<(), E>,
    {
        let mut tracker = ToolBudgetTracker::new(config.budget.clone());
        let mut reports = Vec::new();

        for group in self.plan_batch(calls) {
            let started = Instant::now();
            let outcomes = match group.mode {
                ToolGroupExecutionMode::ParallelReadOnly => {
                    self.dispatch_read_only_group(&group.calls, &mut tracker, config)
                }
                ToolGroupExecutionMode::SequentialMutating => {
                    let deadline = group_deadline(&config.budget);
                    group
                        .calls
                        .into_iter()
                        .map(|call| self.dispatch_prepared(call, &mut tracker, config, deadline))
                        .collect()
                }
            };
            let elapsed = started.elapsed();
            let group_timed_out = outcomes.iter().any(outcome_is_timeout);
            let timeout_error = timeout_error_for_elapsed(elapsed, &config.budget, group_timed_out);
            let mut report = ToolGroupDispatchReport {
                mode: group.mode,
                elapsed_ms: elapsed.as_millis(),
                outcomes,
                timeout_error,
            };
            after_group(&mut report)?;
            reports.push(report);
        }

        Ok(ToolBatchDispatchReport { groups: reports })
    }

    fn dispatch_read_only_group(
        &self,
        calls: &[ToolCallInput],
        tracker: &mut ToolBudgetTracker,
        config: &ToolDispatchConfig,
    ) -> Vec<ToolDispatchOutcome> {
        let deadline = group_deadline(&config.budget);
        let mut prepared = Vec::new();
        let mut outcomes = calls
            .iter()
            .map(|_| None)
            .collect::<Vec<Option<ToolDispatchOutcome>>>();

        for (index, call) in calls.iter().cloned().enumerate() {
            let cancellation_token = ToolCancellationToken::new();
            match self.prepare_call(&call, tracker, config, deadline, cancellation_token.clone()) {
                Ok(prepared_call) => prepared.push((index, prepared_call)),
                Err(mut failure) => {
                    match tracker.record_failure(&call, &failure.error) {
                        Ok(signal) => failure.doom_loop_signal = signal,
                        Err(error) => failure.error = error,
                    }
                    outcomes[index] = Some(ToolDispatchOutcome::Failed(*failure));
                }
            }
        }

        let mut pending = BTreeMap::new();
        let mut worker_ids = BTreeMap::new();
        let (result_tx, result_rx) = mpsc::channel();
        for (index, prepared_call) in prepared {
            let pending_call = PendingReadOnlyToolCall::from_prepared(&prepared_call);
            let context = config.context.clone();
            let rollback = config.rollback.clone();
            let result_tx = result_tx.clone();
            let spawn_result = self
                .read_only_worker_supervisor
                .spawn_until(deadline, move || {
                    let call = prepared_call.call.clone();
                    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        execute_prepared_call(prepared_call, &context, rollback.as_deref())
                    }))
                    .unwrap_or_else(|_| panic_failure_outcome(&call));
                    let _ = result_tx.send((index, call, outcome));
                });
            match spawn_result {
                Ok(worker_id) => {
                    pending.insert(index, pending_call);
                    worker_ids.insert(index, worker_id);
                }
                Err(error) => {
                    let call = pending_call.call.clone();
                    let mut outcome = ToolDispatchOutcome::Failed(failure_from_pending_error(
                        pending_call,
                        error,
                    ));
                    if let ToolDispatchOutcome::Failed(failure) = &mut outcome {
                        match tracker.record_failure(&call, &failure.error) {
                            Ok(signal) => failure.doom_loop_signal = signal,
                            Err(error) => failure.error = error,
                        }
                    }
                    outcomes[index] = Some(outcome);
                }
            }
        }
        drop(result_tx);

        while !pending.is_empty() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match result_rx.recv_timeout(remaining) {
                Ok((index, call, mut outcome)) => {
                    pending.remove(&index);
                    if let Some(worker_id) = worker_ids.remove(&index) {
                        self.read_only_worker_supervisor.join(worker_id);
                    }
                    if let ToolDispatchOutcome::Failed(failure) = &mut outcome {
                        match tracker.record_failure(&call, &failure.error) {
                            Ok(signal) => failure.doom_loop_signal = signal,
                            Err(error) => failure.error = error,
                        }
                    }
                    outcomes[index] = Some(outcome);
                }
                Err(mpsc::RecvTimeoutError::Timeout) => break,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        for (index, pending_call) in pending {
            pending_call.cancellation_token.cancel();
            let call = pending_call.call.clone();
            let mut outcome = ToolDispatchOutcome::Failed(timeout_failure_from_pending(
                pending_call,
                &config.budget,
            ));
            if let ToolDispatchOutcome::Failed(failure) = &mut outcome {
                match tracker.record_failure(&call, &failure.error) {
                    Ok(signal) => failure.doom_loop_signal = signal,
                    Err(error) => failure.error = error,
                }
            }
            outcomes[index] = Some(outcome);
        }

        outcomes.into_iter().flatten().collect()
    }

    fn dispatch_prepared(
        &self,
        call: ToolCallInput,
        tracker: &mut ToolBudgetTracker,
        config: &ToolDispatchConfig,
        deadline: Instant,
    ) -> ToolDispatchOutcome {
        let cancellation_token = ToolCancellationToken::new();
        match self.prepare_call(&call, tracker, config, deadline, cancellation_token) {
            Ok(prepared) => {
                let parent_process_execution = prepared
                    .handler
                    .requires_parent_process_execution(&prepared.call);
                let mut outcome = if parent_process_execution {
                    execute_prepared_call(prepared, &config.context, config.rollback.as_deref())
                } else if prepared.descriptor.mutability == ToolMutability::Mutating {
                    match self.mutation_boundary {
                        MutationBoundary::TerminableProcess => {
                            let mut mutation_state = self
                                .mutation_execution_state
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner());
                            execute_mutating_call_in_boundary(prepared, config, &mut mutation_state)
                        }
                        #[cfg(any(test, feature = "test-support"))]
                        MutationBoundary::CooperativeTestHarness => execute_prepared_call(
                            prepared,
                            &config.context,
                            config.rollback.as_deref(),
                        ),
                    }
                } else {
                    execute_prepared_call(prepared, &config.context, config.rollback.as_deref())
                };
                if let ToolDispatchOutcome::Failed(failure) = &mut outcome {
                    match tracker.record_failure(&call, &failure.error) {
                        Ok(signal) => {
                            failure.doom_loop_signal = signal;
                        }
                        Err(error) => {
                            failure.error = error;
                        }
                    }
                }
                outcome
            }
            Err(mut failure) => {
                match tracker.record_failure(&call, &failure.error) {
                    Ok(signal) => failure.doom_loop_signal = signal,
                    Err(error) => failure.error = error,
                }
                ToolDispatchOutcome::Failed(*failure)
            }
        }
    }

    fn prepare_call(
        &self,
        call: &ToolCallInput,
        tracker: &mut ToolBudgetTracker,
        config: &ToolDispatchConfig,
        deadline: Instant,
        cancellation_token: ToolCancellationToken,
    ) -> Result<PreparedToolCall, Box<ToolDispatchFailure>> {
        let started = Instant::now();
        let Some(handler) = self.handlers.get(&call.tool_name).cloned() else {
            return Err(Box::new(failure_from_error(
                call,
                ToolExecutionError::unavailable(
                    "agent_tool_unavailable",
                    format!("Tool `{}` is not registered.", call.tool_name),
                ),
                json!({}),
                json!({ "ok": false }),
                started.elapsed(),
            )));
        };
        let descriptor = handler.descriptor();
        let pre_hook_payload = handler.pre_hook_payload(call);

        if let Err(error) = descriptor.validate() {
            return Err(Box::new(failure_from_error(
                call,
                error,
                pre_hook_payload,
                json!({ "ok": false }),
                started.elapsed(),
            )));
        }
        if let Err(error) = tracker.record_call(call) {
            return Err(Box::new(failure_from_error(
                call,
                error,
                pre_hook_payload,
                json!({ "ok": false }),
                started.elapsed(),
            )));
        }
        if let Err(error) = handler.validate_input(&call.input) {
            return Err(Box::new(failure_from_error(
                call,
                error,
                pre_hook_payload,
                json!({ "ok": false }),
                started.elapsed(),
            )));
        }

        match config.policy.evaluate(&descriptor, call) {
            ToolPolicyDecision::Allow => {
                let sandbox_metadata =
                    match config.sandbox.evaluate(&descriptor, call, &config.context) {
                        Ok(metadata) => metadata,
                        Err(denied) => {
                            let mut failure = failure_from_error(
                                call,
                                denied.error,
                                pre_hook_payload,
                                json!({ "ok": false }),
                                started.elapsed(),
                            );
                            failure.sandbox_metadata = Some(*denied.metadata);
                            return Err(Box::new(failure));
                        }
                    };

                Ok(PreparedToolCall {
                    call: call.clone(),
                    descriptor,
                    handler,
                    pre_hook_payload,
                    started,
                    budget: config.budget.clone(),
                    sandbox_metadata: Some(sandbox_metadata),
                    deadline: Some(deadline),
                    cancellation_token,
                })
            }
            ToolPolicyDecision::Deny { code, message } => Err(Box::new(failure_from_error(
                call,
                ToolExecutionError::policy_denied(code, message),
                pre_hook_payload,
                json!({ "ok": false }),
                started.elapsed(),
            ))),
            ToolPolicyDecision::RequireApproval { action_id, message } => {
                Err(Box::new(failure_from_error(
                    call,
                    ToolExecutionError::approval_required(action_id, message),
                    pre_hook_payload,
                    json!({ "ok": false }),
                    started.elapsed(),
                )))
            }
        }
    }
}

struct PreparedToolCall {
    call: ToolCallInput,
    descriptor: ToolDescriptorV2,
    handler: Arc<dyn ToolHandler>,
    pre_hook_payload: JsonValue,
    started: Instant,
    budget: ToolBudget,
    sandbox_metadata: Option<SandboxExecutionMetadata>,
    deadline: Option<Instant>,
    cancellation_token: ToolCancellationToken,
}

struct PendingReadOnlyToolCall {
    call: ToolCallInput,
    pre_hook_payload: JsonValue,
    started: Instant,
    sandbox_metadata: Option<SandboxExecutionMetadata>,
    cancellation_token: ToolCancellationToken,
}

impl PendingReadOnlyToolCall {
    fn from_prepared(prepared: &PreparedToolCall) -> Self {
        Self {
            call: prepared.call.clone(),
            pre_hook_payload: prepared.pre_hook_payload.clone(),
            started: prepared.started,
            sandbox_metadata: prepared.sandbox_metadata.clone(),
            cancellation_token: prepared.cancellation_token.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum MutationWorkerPhase {
    Starting,
    Checkpoint,
    Handler,
    PostHook,
    Rollback,
    Completed,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
enum MutationWorkerMessage {
    Phase(MutationWorkerPhase),
    Checkpoint(Option<JsonValue>),
    Outcome(Box<ToolDispatchOutcome>),
}

#[derive(Debug, Serialize, Deserialize)]
struct MutationRecoveryResult {
    result: ToolRegistryResult<JsonValue>,
}

struct PendingMutationToolCall {
    call: ToolCallInput,
    descriptor: ToolDescriptorV2,
    pre_hook_payload: JsonValue,
    started: Instant,
    sandbox_metadata: Option<SandboxExecutionMetadata>,
}

impl PendingMutationToolCall {
    fn from_prepared(prepared: &PreparedToolCall) -> Self {
        Self {
            call: prepared.call.clone(),
            descriptor: prepared.descriptor.clone(),
            pre_hook_payload: prepared.pre_hook_payload.clone(),
            started: prepared.started,
            sandbox_metadata: prepared.sandbox_metadata.clone(),
        }
    }

    fn terminated_failure(
        &self,
        phase: MutationWorkerPhase,
        cancelled: bool,
        crashed: bool,
    ) -> ToolDispatchFailure {
        let mut sandbox_metadata = self.sandbox_metadata.clone();
        if let Some(metadata) = sandbox_metadata.as_mut() {
            metadata.exit_classification = if crashed {
                crate::SandboxExitClassification::Unknown
            } else if cancelled {
                crate::SandboxExitClassification::Cancelled
            } else {
                crate::SandboxExitClassification::Timeout
            };
        }
        let code = if crashed {
            "agent_tool_mutation_worker_crashed"
        } else if cancelled {
            "agent_tool_mutation_cancelled"
        } else {
            "agent_tool_mutation_terminated"
        };
        let error = if crashed {
            ToolExecutionError::retryable(
                code,
                format!(
                    "Mutating tool `{}` exited unexpectedly while its isolated {phase:?} phase was running.",
                    self.call.tool_name
                ),
            )
        } else {
            ToolExecutionError::timeout(
                code,
                format!(
                    "Xero terminated mutating tool `{}` while its isolated {phase:?} phase was still running.",
                    self.call.tool_name
                ),
            )
        };
        failure_from_error_with_sandbox(
            &self.call,
            error,
            self.pre_hook_payload.clone(),
            json!({
                "ok": false,
                "mutationBoundary": "process",
                "phase": phase,
                "terminated": true,
                "cancelled": cancelled,
                "crashed": crashed,
            }),
            self.started.elapsed(),
            sandbox_metadata,
        )
    }

    fn unavailable_failure(self, error: ToolExecutionError) -> ToolDispatchFailure {
        failure_from_error_with_sandbox(
            &self.call,
            error,
            self.pre_hook_payload,
            json!({
                "ok": false,
                "mutationBoundary": "process",
                "phase": MutationWorkerPhase::Starting,
            }),
            self.started.elapsed(),
            self.sandbox_metadata,
        )
    }

    fn quarantined_failure(&self, quarantine: &MutationQuarantine) -> ToolDispatchFailure {
        let mut error = ToolExecutionError::retryable(
            "agent_tool_mutation_quarantined",
            format!(
                "Xero quarantined mutating tool `{}` because recovery for prior tool `{}` remains unresolved.",
                self.call.tool_name, quarantine.call.tool_name
            ),
        );
        error.telemetry_attributes.insert(
            "xero.mutation.quarantined_call_id".into(),
            quarantine.call.tool_call_id.clone(),
        );
        error.telemetry_attributes.insert(
            "xero.mutation.quarantined_phase".into(),
            format!("{:?}", quarantine.phase).to_ascii_lowercase(),
        );
        ToolDispatchFailure {
            tool_call_id: self.call.tool_call_id.clone(),
            tool_name: self.call.tool_name.clone(),
            error,
            doom_loop_signal: None,
            rollback_payload: None,
            rollback_error: Some(quarantine.rollback_error.clone()),
            pre_hook_payload: self.pre_hook_payload.clone(),
            post_hook_payload: json!({
                "ok": false,
                "mutationBoundary": "process",
                "quarantined": true,
                "blockedByToolCallId": quarantine.call.tool_call_id,
                "blockedByToolName": quarantine.call.tool_name,
                "phase": quarantine.phase,
            }),
            elapsed_ms: self.started.elapsed().as_millis(),
            sandbox_metadata: self.sandbox_metadata.clone(),
        }
    }
}

#[cfg(unix)]
fn execute_mutating_call_in_boundary(
    prepared: PreparedToolCall,
    config: &ToolDispatchConfig,
    mutation_state: &mut MutationExecutionState,
) -> ToolDispatchOutcome {
    let mutation_scope = mutation_execution_scope(&config.context);
    let pending = PendingMutationToolCall::from_prepared(&prepared);
    let deadline = prepared
        .deadline
        .unwrap_or_else(|| group_deadline(&config.budget));
    let mut audit_events = Vec::new();
    if let Some(quarantine) = mutation_state.quarantines.remove(&mutation_scope) {
        match recover_mutation_quarantine(&quarantine, config) {
            Ok(_) => audit_events.push("mutation_quarantine_recovered".to_string()),
            Err(rollback_error) => {
                let quarantine = MutationQuarantine {
                    rollback_error,
                    ..quarantine
                };
                let failure = pending.quarantined_failure(&quarantine);
                mutation_state
                    .quarantines
                    .insert(mutation_scope.clone(), quarantine);
                let mut outcome = ToolDispatchOutcome::Failed(failure);
                attach_mutation_boundary_metadata(
                    &mut outcome,
                    &pending.call,
                    None,
                    MutationWorkerPhase::Rollback,
                    &["mutation_quarantine_blocked".to_string()],
                    true,
                );
                return outcome;
            }
        }
    }
    let rollback_in_parent = config
        .rollback
        .as_deref()
        .is_some_and(ToolRollback::requires_parent_process);
    let parent_checkpoint = if rollback_in_parent {
        match config.rollback.as_deref() {
            Some(recorder) => match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                recorder.checkpoint_before(&prepared.call, &prepared.descriptor)
            })) {
                Ok(Ok(checkpoint)) => Some(checkpoint),
                Ok(Err(error)) => return checkpoint_failure_outcome(prepared, error),
                Err(_) => {
                    let error = ToolExecutionError::retryable(
                        "agent_tool_checkpoint_panicked",
                        format!(
                            "Mutation checkpoint for tool `{}` panicked in the supervising process.",
                            prepared.call.tool_name
                        ),
                    );
                    return checkpoint_failure_outcome(prepared, error);
                }
            },
            None => Some(None),
        }
    } else {
        None
    };
    let (parent_stream, child_stream) = match UnixStream::pair() {
        Ok(streams) => streams,
        Err(error) => {
            return ToolDispatchOutcome::Failed(pending.unavailable_failure(
                ToolExecutionError::unavailable(
                    "agent_tool_mutation_boundary_socket_failed",
                    format!("Xero could not create the mutation worker channel: {error}"),
                ),
            ));
        }
    };

    // SAFETY: `fork` establishes the isolated mutation worker. The child does not return into
    // the parent control flow and always terminates with `_exit` below.
    let child_pid = unsafe { libc::fork() };
    if child_pid < 0 {
        return ToolDispatchOutcome::Failed(pending.unavailable_failure(
            ToolExecutionError::unavailable(
                "agent_tool_mutation_boundary_spawn_failed",
                format!(
                    "Xero could not create the mutation worker process: {}",
                    std::io::Error::last_os_error()
                ),
            ),
        ));
    }

    if child_pid == 0 {
        drop(parent_stream);
        // SAFETY: the worker becomes the process-group leader for its own PID, so all
        // subprocesses it launches can be terminated as one mutation tree by the parent.
        let _ = unsafe { libc::setpgid(0, 0) };
        MUTATION_BOUNDARY_CHILD_ACTIVE.store(true, Ordering::SeqCst);
        let mut child_stream = child_stream;
        execute_mutation_worker(
            prepared,
            &config.context,
            if rollback_in_parent {
                None
            } else {
                config.rollback.as_deref()
            },
            parent_checkpoint.clone(),
            &mut child_stream,
        );
        // SAFETY: this is the fork child. `_exit` avoids unwinding parent-owned Rust state.
        unsafe { libc::_exit(0) };
    }

    drop(child_stream);
    // SAFETY: the positive child PID was returned by `fork`; racing the child's identical
    // `setpgid` is harmless and closes the window before supervision begins.
    let _ = unsafe { libc::setpgid(child_pid, child_pid) };
    audit_events.push("mutation_worker_spawned".to_string());
    let (message_tx, message_rx) = mpsc::channel();
    let reader = thread::spawn(move || read_mutation_worker_messages(parent_stream, message_tx));
    let mut phase = MutationWorkerPhase::Starting;
    let mut checkpoint = parent_checkpoint.flatten();
    let mut outcome = None;
    let mut cancelled = false;
    let mut worker_disconnected = false;
    let supervisor_deadline = deadline + MUTATION_TERMINATION_GRACE;

    while outcome.is_none() {
        if config
            .cancellation_check
            .as_ref()
            .is_some_and(|is_cancelled| is_cancelled())
        {
            cancelled = true;
            break;
        }
        // Give a cooperative worker one short, bounded drain window after the control deadline
        // so it can report its typed timeout and rollback result before forcible termination.
        let remaining = supervisor_deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        let wait = remaining.min(MUTATION_SUPERVISOR_POLL_INTERVAL);
        match message_rx.recv_timeout(wait) {
            Ok(MutationWorkerMessage::Phase(next_phase)) => {
                phase = next_phase;
                audit_events.push(format!(
                    "mutation_phase_{}",
                    mutation_worker_phase_name(next_phase)
                ));
            }
            Ok(MutationWorkerMessage::Checkpoint(worker_checkpoint)) => {
                checkpoint = worker_checkpoint;
                audit_events.push("mutation_checkpoint_recorded".to_string());
            }
            Ok(MutationWorkerMessage::Outcome(worker_outcome)) => outcome = Some(*worker_outcome),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                worker_disconnected = true;
                break;
            }
        }
    }

    if let Some(mut outcome) = outcome {
        let _ = wait_for_mutation_worker(child_pid, None);
        cleanup_mutation_process_group(child_pid);
        let _ = reader.join();
        if rollback_in_parent {
            if let (ToolDispatchOutcome::Failed(failure), Some(recorder), Some(checkpoint)) = (
                &mut outcome,
                config.rollback.as_deref(),
                checkpoint.as_ref(),
            ) {
                phase = MutationWorkerPhase::Rollback;
                audit_events.push("mutation_phase_rollback".to_string());
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    recorder.rollback_after_failure(
                        &pending.call,
                        &pending.descriptor,
                        checkpoint,
                        &failure.error,
                    )
                })) {
                    Ok(Ok(payload)) => failure.rollback_payload = Some(payload),
                    Ok(Err(error)) => failure.rollback_error = Some(error),
                    Err(_) => {
                        failure.rollback_error = Some(ToolExecutionError::retryable(
                            "agent_tool_rollback_panicked",
                            format!(
                                "Rollback for mutating tool `{}` panicked in the supervising process.",
                                pending.call.tool_name
                            ),
                        ));
                    }
                }
            }
        }
        let affected_paths = mutation_affected_paths(&pending.call.input, checkpoint.as_ref());
        if let ToolDispatchOutcome::Failed(failure) = &mut outcome {
            if failure.rollback_error.is_some() {
                let quarantine = MutationQuarantine {
                    project_id: config.context.project_id.clone(),
                    call: pending.call.clone(),
                    descriptor: pending.descriptor.clone(),
                    checkpoint,
                    error: failure.error.clone(),
                    rollback_error: failure.rollback_error.clone().unwrap_or_else(|| {
                        ToolExecutionError::retryable(
                            "agent_tool_rollback_unresolved",
                            "Mutation rollback did not reach a resolved state.",
                        )
                    }),
                    phase: MutationWorkerPhase::Rollback,
                    rollback: config.rollback.clone(),
                };
                match recover_mutation_quarantine(&quarantine, config) {
                    Ok(payload) => {
                        failure.rollback_payload = Some(payload);
                        failure.rollback_error = None;
                        audit_events.push("mutation_rollback_recovered".to_string());
                    }
                    Err(rollback_error) => {
                        failure.rollback_error = Some(rollback_error.clone());
                        mutation_state.quarantines.insert(
                            mutation_scope.clone(),
                            MutationQuarantine {
                                rollback_error,
                                ..quarantine
                            },
                        );
                        audit_events.push("mutation_quarantined".to_string());
                    }
                }
            }
        }
        attach_mutation_boundary_metadata(
            &mut outcome,
            &pending.call,
            Some(&affected_paths),
            phase,
            &audit_events,
            mutation_state.quarantines.contains_key(&mutation_scope),
        );
        return outcome;
    }

    terminate_mutation_process_tree(child_pid);
    let _ = reader.join();
    let crashed = worker_disconnected && Instant::now() < deadline;
    let mut failure = pending.terminated_failure(phase, cancelled, crashed);
    let affected_paths = mutation_affected_paths(&pending.call.input, checkpoint.as_ref());
    let quarantine = MutationQuarantine {
        project_id: config.context.project_id.clone(),
        call: pending.call.clone(),
        descriptor: pending.descriptor.clone(),
        checkpoint,
        error: failure.error.clone(),
        rollback_error: ToolExecutionError::retryable(
            "agent_tool_mutation_recovery_pending",
            "Mutation recovery has not completed yet.",
        ),
        phase,
        rollback: config.rollback.clone(),
    };
    match recover_mutation_quarantine(&quarantine, config) {
        Ok(payload) => {
            failure.rollback_payload = Some(payload);
            audit_events.push("mutation_rollback_recovered".to_string());
        }
        Err(rollback_error) => {
            failure.rollback_error = Some(rollback_error.clone());
            mutation_state.quarantines.insert(
                mutation_scope.clone(),
                MutationQuarantine {
                    rollback_error,
                    ..quarantine
                },
            );
            audit_events.push("mutation_quarantined".to_string());
        }
    }
    audit_events.push(if crashed {
        "mutation_worker_crashed".to_string()
    } else {
        "mutation_worker_terminated".to_string()
    });
    let mut outcome = ToolDispatchOutcome::Failed(failure);
    attach_mutation_boundary_metadata(
        &mut outcome,
        &pending.call,
        Some(&affected_paths),
        phase,
        &audit_events,
        mutation_state.quarantines.contains_key(&mutation_scope),
    );
    outcome
}

#[cfg(not(unix))]
fn execute_mutating_call_in_boundary(
    prepared: PreparedToolCall,
    _config: &ToolDispatchConfig,
    _mutation_state: &mut MutationExecutionState,
) -> ToolDispatchOutcome {
    let pending = PendingMutationToolCall::from_prepared(&prepared);
    ToolDispatchOutcome::Failed(pending.unavailable_failure(ToolExecutionError::unavailable(
        "agent_tool_mutation_boundary_unsupported",
        "Xero refuses to run a mutating handler on a platform without a terminable mutation worker boundary.",
    )))
}

#[cfg(unix)]
fn execute_mutation_worker(
    prepared: PreparedToolCall,
    context: &ToolExecutionContext,
    rollback: Option<&dyn ToolRollback>,
    parent_checkpoint: Option<Option<JsonValue>>,
    stream: &mut UnixStream,
) {
    let _ = send_mutation_worker_message(
        stream,
        &MutationWorkerMessage::Phase(MutationWorkerPhase::Checkpoint),
    );
    let control = ToolExecutionControl::new(prepared.deadline, prepared.cancellation_token.clone());
    if control.is_cancelled() {
        let outcome = ToolDispatchOutcome::Failed(timeout_failure_from_prepared(prepared));
        let _ = send_mutation_worker_message(
            stream,
            &MutationWorkerMessage::Outcome(Box::new(outcome)),
        );
        return;
    }

    let checkpoint = match parent_checkpoint {
        Some(checkpoint) => checkpoint,
        None => match rollback {
            Some(recorder) => {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    recorder.checkpoint_before(&prepared.call, &prepared.descriptor)
                })) {
                    Ok(Ok(checkpoint)) => checkpoint,
                    Ok(Err(error)) => {
                        let outcome = checkpoint_failure_outcome(prepared, error);
                        let _ = send_mutation_worker_message(
                            stream,
                            &MutationWorkerMessage::Outcome(Box::new(outcome)),
                        );
                        return;
                    }
                    Err(_) => {
                        let error = ToolExecutionError::retryable(
                        "agent_tool_checkpoint_panicked",
                        format!(
                            "Mutation checkpoint for tool `{}` panicked inside its isolated worker.",
                            prepared.call.tool_name
                        ),
                    );
                        let outcome = checkpoint_failure_outcome(prepared, error);
                        let _ = send_mutation_worker_message(
                            stream,
                            &MutationWorkerMessage::Outcome(Box::new(outcome)),
                        );
                        return;
                    }
                }
            }
            None => None,
        },
    };
    if let Err(error) = send_mutation_worker_message(
        stream,
        &MutationWorkerMessage::Checkpoint(checkpoint.clone()),
    ) {
        let publish_error = ToolExecutionError::retryable(
            "agent_tool_mutation_checkpoint_publish_failed",
            format!("Xero could not publish mutation recovery state before execution: {error}"),
        );
        let outcome = mutation_outcome_after_post_hook(
            prepared,
            rollback,
            checkpoint.as_ref(),
            Err(publish_error),
            json!({
                "ok": false,
                "preflight": "rollback_checkpoint_publish_failed",
            }),
            stream,
        );
        let _ = send_mutation_worker_message(
            stream,
            &MutationWorkerMessage::Outcome(Box::new(outcome)),
        );
        return;
    }

    let _ = send_mutation_worker_message(
        stream,
        &MutationWorkerMessage::Phase(MutationWorkerPhase::Handler),
    );
    let mut raw_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prepared
            .handler
            .execute_with_control(context, &prepared.call, &control)
    }))
    .unwrap_or_else(|_| Err(handler_panic_error(&prepared.call)));
    if raw_result.is_ok() && control.is_cancelled() {
        raw_result = Err(ToolExecutionError::timeout(
            "agent_tool_group_timeout",
            format!(
                "Tool `{}` completed after the tool-group wall-clock budget expired.",
                prepared.call.tool_name
            ),
        ));
    }

    let _ = send_mutation_worker_message(
        stream,
        &MutationWorkerMessage::Phase(MutationWorkerPhase::PostHook),
    );
    let post_hook_payload = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prepared
            .handler
            .post_hook_payload(&prepared.call, &raw_result)
    })) {
        Ok(payload) => payload,
        Err(_) => {
            raw_result = Err(ToolExecutionError::retryable(
                "agent_tool_post_hook_panicked",
                format!(
                    "Post-hook for mutating tool `{}` panicked inside its isolated worker.",
                    prepared.call.tool_name
                ),
            ));
            json!({
                "toolCallId": &prepared.call.tool_call_id,
                "toolName": &prepared.call.tool_name,
                "ok": false,
                "postHookPanicked": true,
            })
        }
    };

    let outcome = mutation_outcome_after_post_hook(
        prepared,
        rollback,
        checkpoint.as_ref(),
        raw_result,
        post_hook_payload,
        stream,
    );
    let _ = send_mutation_worker_message(
        stream,
        &MutationWorkerMessage::Phase(MutationWorkerPhase::Completed),
    );
    let _ =
        send_mutation_worker_message(stream, &MutationWorkerMessage::Outcome(Box::new(outcome)));
}

#[cfg(unix)]
fn checkpoint_failure_outcome(
    prepared: PreparedToolCall,
    error: ToolExecutionError,
) -> ToolDispatchOutcome {
    let mut sandbox_metadata = prepared.sandbox_metadata;
    if let Some(metadata) = sandbox_metadata.as_mut() {
        metadata.exit_classification = exit_classification_from_error(&error);
    }
    ToolDispatchOutcome::Failed(ToolDispatchFailure {
        tool_call_id: prepared.call.tool_call_id,
        tool_name: prepared.call.tool_name,
        error,
        doom_loop_signal: None,
        rollback_payload: None,
        rollback_error: None,
        pre_hook_payload: prepared.pre_hook_payload,
        post_hook_payload: json!({
            "ok": false,
            "preflight": "rollback_checkpoint_failed",
        }),
        elapsed_ms: prepared.started.elapsed().as_millis(),
        sandbox_metadata,
    })
}

#[cfg(unix)]
fn mutation_outcome_after_post_hook(
    prepared: PreparedToolCall,
    rollback: Option<&dyn ToolRollback>,
    checkpoint: Option<&JsonValue>,
    raw_result: ToolRegistryResult<ToolHandlerOutput>,
    post_hook_payload: JsonValue,
    stream: &mut UnixStream,
) -> ToolDispatchOutcome {
    match raw_result {
        Ok(handler_output) => {
            let mut sandbox_metadata = prepared.sandbox_metadata;
            if let Some(metadata) = sandbox_metadata.as_mut() {
                metadata.exit_classification =
                    exit_classification_from_output(&handler_output.output);
            }
            let (output, truncation) = truncate_tool_output(
                handler_output.output,
                &prepared.descriptor,
                &prepared.budget,
            );
            let mut telemetry_attributes = prepared.descriptor.telemetry_attributes.clone();
            telemetry_attributes.extend(handler_output.telemetry_attributes);
            telemetry_attributes.extend(output_telemetry(&truncation));
            ToolDispatchOutcome::Succeeded(ToolDispatchSuccess {
                tool_call_id: prepared.call.tool_call_id,
                tool_name: prepared.call.tool_name,
                summary: handler_output.summary,
                output,
                truncation,
                pre_hook_payload: prepared.pre_hook_payload,
                post_hook_payload,
                telemetry_attributes,
                elapsed_ms: prepared.started.elapsed().as_millis(),
                sandbox_metadata,
            })
        }
        Err(error) => {
            let mut sandbox_metadata = prepared.sandbox_metadata;
            if let Some(metadata) = sandbox_metadata.as_mut() {
                metadata.exit_classification = exit_classification_from_error(&error);
            }
            let mut rollback_payload = None;
            let mut rollback_error = None;
            if let (Some(recorder), Some(checkpoint)) = (rollback, checkpoint) {
                let _ = send_mutation_worker_message(
                    stream,
                    &MutationWorkerMessage::Phase(MutationWorkerPhase::Rollback),
                );
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    recorder.rollback_after_failure(
                        &prepared.call,
                        &prepared.descriptor,
                        checkpoint,
                        &error,
                    )
                })) {
                    Ok(Ok(payload)) => rollback_payload = Some(payload),
                    Ok(Err(error)) => rollback_error = Some(error),
                    Err(_) => {
                        rollback_error = Some(ToolExecutionError::retryable(
                            "agent_tool_rollback_panicked",
                            format!(
                                "Rollback for mutating tool `{}` panicked inside its isolated worker.",
                                prepared.call.tool_name
                            ),
                        ));
                    }
                }
            }
            ToolDispatchOutcome::Failed(ToolDispatchFailure {
                tool_call_id: prepared.call.tool_call_id,
                tool_name: prepared.call.tool_name,
                error,
                doom_loop_signal: None,
                rollback_payload,
                rollback_error,
                pre_hook_payload: prepared.pre_hook_payload,
                post_hook_payload,
                elapsed_ms: prepared.started.elapsed().as_millis(),
                sandbox_metadata,
            })
        }
    }
}

#[cfg(unix)]
fn recover_mutation_quarantine(
    quarantine: &MutationQuarantine,
    config: &ToolDispatchConfig,
) -> ToolRegistryResult<JsonValue> {
    let rollback = quarantine
        .rollback
        .as_deref()
        .or_else(|| {
            (config.context.project_id == quarantine.project_id)
                .then_some(config.rollback.as_deref())
                .flatten()
        })
        .ok_or_else(|| {
            ToolExecutionError::retryable(
                "agent_tool_mutation_recovery_unavailable",
                format!(
                    "Xero cannot recover terminated tool `{}` because no rollback provider is configured.",
                    quarantine.call.tool_name
                ),
            )
        })?;
    if rollback.requires_parent_process() {
        return std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rollback.recover_after_termination(
                &quarantine.call,
                &quarantine.descriptor,
                quarantine.checkpoint.as_ref(),
                &quarantine.error,
            )
        }))
        .unwrap_or_else(|_| {
            Err(ToolExecutionError::retryable(
                "agent_tool_rollback_panicked",
                format!(
                    "Recovery rollback for tool `{}` panicked in the supervising process.",
                    quarantine.call.tool_name
                ),
            ))
        });
    }
    let (parent_stream, child_stream) = UnixStream::pair().map_err(|error| {
        ToolExecutionError::retryable(
            "agent_tool_recovery_boundary_socket_failed",
            format!("Xero could not create the recovery worker channel: {error}"),
        )
    })?;
    // SAFETY: recovery uses the same one-way fork-worker contract as mutation execution; the
    // child sends one bounded result and exits without returning through parent-owned state.
    let child_pid = unsafe { libc::fork() };
    if child_pid < 0 {
        return Err(ToolExecutionError::retryable(
            "agent_tool_recovery_boundary_spawn_failed",
            format!(
                "Xero could not create the recovery worker process: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    if child_pid == 0 {
        drop(parent_stream);
        // SAFETY: make the recovery worker its own process-group leader so a hung recovery tree
        // can be terminated without signaling the desktop process group.
        let _ = unsafe { libc::setpgid(0, 0) };
        MUTATION_BOUNDARY_CHILD_ACTIVE.store(true, Ordering::SeqCst);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rollback.recover_after_termination(
                &quarantine.call,
                &quarantine.descriptor,
                quarantine.checkpoint.as_ref(),
                &quarantine.error,
            )
        }))
        .unwrap_or_else(|_| {
            Err(ToolExecutionError::retryable(
                "agent_tool_rollback_panicked",
                format!(
                    "Recovery rollback for tool `{}` panicked inside its isolated worker.",
                    quarantine.call.tool_name
                ),
            ))
        });
        let mut child_stream = child_stream;
        let _ = write_framed_json(&mut child_stream, &MutationRecoveryResult { result });
        // SAFETY: this is the fork child. `_exit` avoids unwinding parent-owned Rust state.
        unsafe { libc::_exit(0) };
    }

    drop(child_stream);
    // SAFETY: the PID is the direct child returned by `fork`; this duplicates the child's
    // process-group setup to eliminate a parent-side race before timeout supervision.
    let _ = unsafe { libc::setpgid(child_pid, child_pid) };
    let (result_tx, result_rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        let _ = result_tx.send(read_mutation_recovery_result(parent_stream));
    });
    let timeout = Duration::from_millis(config.budget.max_mutation_cleanup_ms.max(1));
    let result = result_rx.recv_timeout(timeout);
    match result {
        Ok(Some(result)) => {
            let _ = wait_for_mutation_worker(child_pid, None);
            let _ = reader.join();
            result.result
        }
        Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
            terminate_mutation_process_tree(child_pid);
            let _ = reader.join();
            Err(ToolExecutionError::timeout(
                "agent_tool_rollback_terminated",
                format!(
                    "Recovery rollback for tool `{}` terminated without a result.",
                    quarantine.call.tool_name
                ),
            ))
        }
        Err(mpsc::RecvTimeoutError::Timeout) => {
            terminate_mutation_process_tree(child_pid);
            let _ = reader.join();
            Err(ToolExecutionError::timeout(
                "agent_tool_rollback_terminated",
                format!(
                    "Xero terminated recovery rollback for tool `{}` after {}ms.",
                    quarantine.call.tool_name, config.budget.max_mutation_cleanup_ms
                ),
            ))
        }
    }
}

#[cfg(not(unix))]
fn recover_mutation_quarantine(
    quarantine: &MutationQuarantine,
    _config: &ToolDispatchConfig,
) -> ToolRegistryResult<JsonValue> {
    Err(ToolExecutionError::unavailable(
        "agent_tool_mutation_recovery_unsupported",
        format!(
            "Xero cannot recover mutating tool `{}` on a platform without a terminable recovery worker boundary.",
            quarantine.call.tool_name
        ),
    ))
}

#[cfg(unix)]
fn read_mutation_recovery_result(mut stream: UnixStream) -> Option<MutationRecoveryResult> {
    let mut length_bytes = [0_u8; 4];
    stream.read_exact(&mut length_bytes).ok()?;
    let payload_len = u32::from_be_bytes(length_bytes) as usize;
    if payload_len > MAX_MUTATION_WORKER_MESSAGE_BYTES {
        return None;
    }
    let mut payload = vec![0_u8; payload_len];
    stream.read_exact(&mut payload).ok()?;
    serde_json::from_slice(&payload).ok()
}

#[cfg(unix)]
fn send_mutation_worker_message(
    stream: &mut UnixStream,
    message: &MutationWorkerMessage,
) -> std::io::Result<()> {
    write_framed_json(stream, message)
}

#[cfg(unix)]
fn write_framed_json(stream: &mut UnixStream, message: &impl Serialize) -> std::io::Result<()> {
    let payload = serde_json::to_vec(message).map_err(std::io::Error::other)?;
    if payload.len() > MAX_MUTATION_WORKER_MESSAGE_BYTES {
        return Err(std::io::Error::other(
            "mutation worker message exceeded the bounded IPC payload",
        ));
    }
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| std::io::Error::other("mutation worker message exceeded u32 length"))?;
    stream.write_all(&payload_len.to_be_bytes())?;
    stream.write_all(&payload)?;
    stream.flush()
}

#[cfg(unix)]
fn read_mutation_worker_messages(
    mut stream: UnixStream,
    sender: mpsc::Sender<MutationWorkerMessage>,
) {
    loop {
        let mut length_bytes = [0_u8; 4];
        if stream.read_exact(&mut length_bytes).is_err() {
            return;
        }
        let payload_len = u32::from_be_bytes(length_bytes) as usize;
        if payload_len > MAX_MUTATION_WORKER_MESSAGE_BYTES {
            return;
        }
        let mut payload = vec![0_u8; payload_len];
        if stream.read_exact(&mut payload).is_err() {
            return;
        }
        let Ok(message) = serde_json::from_slice(&payload) else {
            return;
        };
        if sender.send(message).is_err() {
            return;
        }
    }
}

#[cfg(unix)]
fn terminate_mutation_process_tree(child_pid: libc::pid_t) {
    let process_group = -child_pid;
    // SAFETY: the negative PID addresses only the isolated worker group created above.
    let _ = unsafe { libc::kill(process_group, libc::SIGTERM) };
    let reaped = wait_for_mutation_worker(child_pid, Some(MUTATION_TERMINATION_GRACE));
    if mutation_process_group_exists(child_pid) {
        // SAFETY: the worker group survived its grace period and must be forcibly terminated.
        let _ = unsafe { libc::kill(process_group, libc::SIGKILL) };
    }
    if !reaped {
        let _ = wait_for_mutation_worker(child_pid, None);
    }
    cleanup_mutation_process_group(child_pid);
}

#[cfg(unix)]
fn cleanup_mutation_process_group(child_pid: libc::pid_t) {
    if !mutation_process_group_exists(child_pid) {
        return;
    }
    let process_group = -child_pid;
    // SAFETY: the negative PID addresses only the isolated worker group created above.
    let _ = unsafe { libc::kill(process_group, libc::SIGTERM) };
    let deadline = Instant::now() + MUTATION_TERMINATION_GRACE;
    while mutation_process_group_exists(child_pid) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    if mutation_process_group_exists(child_pid) {
        // SAFETY: only descendants remaining in the isolated worker group are targeted.
        let _ = unsafe { libc::kill(process_group, libc::SIGKILL) };
    }
}

#[cfg(unix)]
fn mutation_process_group_exists(child_pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 performs an existence/permission probe and does not alter the process.
    let result = unsafe { libc::kill(-child_pid, 0) };
    if result == 0 {
        return true;
    }
    std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(unix)]
fn wait_for_mutation_worker(child_pid: libc::pid_t, timeout: Option<Duration>) -> bool {
    let deadline = timeout.map(|timeout| Instant::now() + timeout);
    loop {
        let mut status = 0;
        let flags = if timeout.is_some() { libc::WNOHANG } else { 0 };
        // SAFETY: `child_pid` is the direct child returned by `fork`, and `status` points to
        // writable storage for `waitpid`'s status word.
        let result = unsafe { libc::waitpid(child_pid, &mut status, flags) };
        if result == child_pid {
            return true;
        }
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::EINTR) {
                continue;
            }
            return true;
        }
        if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            return false;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

fn mutation_worker_phase_name(phase: MutationWorkerPhase) -> &'static str {
    match phase {
        MutationWorkerPhase::Starting => "starting",
        MutationWorkerPhase::Checkpoint => "checkpoint",
        MutationWorkerPhase::Handler => "handler",
        MutationWorkerPhase::PostHook => "post_hook",
        MutationWorkerPhase::Rollback => "rollback",
        MutationWorkerPhase::Completed => "completed",
    }
}

fn attach_mutation_boundary_metadata(
    outcome: &mut ToolDispatchOutcome,
    call: &ToolCallInput,
    affected_paths: Option<&[String]>,
    phase: MutationWorkerPhase,
    audit_events: &[String],
    quarantined: bool,
) {
    let affected_paths = affected_paths
        .map(<[String]>::to_vec)
        .unwrap_or_else(|| mutation_affected_paths(&call.input, None));
    let (exit_classification, rollback_state) = match outcome {
        ToolDispatchOutcome::Succeeded(_) => ("success", "not_required"),
        ToolDispatchOutcome::Failed(failure) => {
            let exit_classification = if failure.error.code == "agent_tool_mutation_cancelled" {
                "cancelled"
            } else if failure.error.code == "agent_tool_mutation_worker_crashed" {
                "crashed"
            } else if failure.error.category == ToolErrorCategory::Timeout {
                "timeout"
            } else if failure.error.code.ends_with("_panicked") {
                "panic"
            } else {
                "failed"
            };
            let rollback_state = if failure.rollback_payload.is_some() {
                "succeeded"
            } else if failure.rollback_error.is_some() {
                if quarantined {
                    "unresolved"
                } else {
                    "failed"
                }
            } else {
                "not_required"
            };
            (exit_classification, rollback_state)
        }
    };
    let report = json!({
        "isolationKind": "process",
        "phase": phase,
        "exitClassification": exit_classification,
        "rollbackState": rollback_state,
        "affectedPaths": &affected_paths,
        "auditEvents": audit_events,
        "quarantined": quarantined,
    });
    let affected_paths_json =
        serde_json::to_string(&affected_paths).unwrap_or_else(|_| "[]".into());
    let audit_events_json = serde_json::to_string(audit_events).unwrap_or_else(|_| "[]".into());

    match outcome {
        ToolDispatchOutcome::Succeeded(success) => {
            success.post_hook_payload = post_hook_with_mutation_report(
                std::mem::take(&mut success.post_hook_payload),
                report,
            );
            success.telemetry_attributes.insert(
                "xero.mutation.exit_classification".into(),
                exit_classification.into(),
            );
            success
                .telemetry_attributes
                .insert("xero.mutation.rollback_state".into(), rollback_state.into());
            success
                .telemetry_attributes
                .insert("xero.mutation.affected_paths".into(), affected_paths_json);
            success
                .telemetry_attributes
                .insert("xero.mutation.audit_events".into(), audit_events_json);
        }
        ToolDispatchOutcome::Failed(failure) => {
            failure.post_hook_payload = post_hook_with_mutation_report(
                std::mem::take(&mut failure.post_hook_payload),
                report,
            );
            failure.error.telemetry_attributes.insert(
                "xero.mutation.exit_classification".into(),
                exit_classification.into(),
            );
            failure
                .error
                .telemetry_attributes
                .insert("xero.mutation.rollback_state".into(), rollback_state.into());
            failure
                .error
                .telemetry_attributes
                .insert("xero.mutation.affected_paths".into(), affected_paths_json);
            failure
                .error
                .telemetry_attributes
                .insert("xero.mutation.audit_events".into(), audit_events_json);
        }
    }
}

fn post_hook_with_mutation_report(mut post_hook: JsonValue, report: JsonValue) -> JsonValue {
    if let Some(object) = post_hook.as_object_mut() {
        object.insert("mutationBoundary".into(), report);
        return post_hook;
    }
    json!({
        "handlerPostHook": post_hook,
        "mutationBoundary": report,
    })
}

fn mutation_affected_paths(input: &JsonValue, checkpoint: Option<&JsonValue>) -> Vec<String> {
    fn collect(value: &JsonValue, parent_key: Option<&str>, paths: &mut BTreeSet<String>) {
        const PATH_KEYS: &[&str] = &[
            "path",
            "paths",
            "from",
            "to",
            "cwd",
            "file",
            "files",
            "sourcePath",
            "destinationPath",
            "pathBefore",
            "pathAfter",
            "writeSet",
        ];
        if parent_key.is_some_and(|key| PATH_KEYS.contains(&key)) {
            match value {
                JsonValue::String(path) if !path.trim().is_empty() => {
                    paths.insert(path.trim().to_string());
                }
                JsonValue::Array(values) => {
                    for value in values {
                        if let Some(path) = value
                            .as_str()
                            .map(str::trim)
                            .filter(|path| !path.is_empty())
                        {
                            paths.insert(path.to_string());
                        }
                    }
                }
                _ => {}
            }
        }
        match value {
            JsonValue::Object(object) => {
                for (key, value) in object {
                    collect(value, Some(key), paths);
                }
            }
            JsonValue::Array(values) => {
                for value in values {
                    collect(value, parent_key, paths);
                }
            }
            _ => {}
        }
    }

    let mut paths = BTreeSet::new();
    collect(input, None, &mut paths);
    if let Some(checkpoint) = checkpoint {
        collect(checkpoint, None, &mut paths);
    }
    paths.into_iter().collect()
}

fn execute_prepared_call(
    prepared: PreparedToolCall,
    context: &ToolExecutionContext,
    rollback: Option<&dyn ToolRollback>,
) -> ToolDispatchOutcome {
    let control = ToolExecutionControl::new(prepared.deadline, prepared.cancellation_token.clone());
    if control.is_cancelled() {
        return ToolDispatchOutcome::Failed(timeout_failure_from_prepared(prepared));
    }

    let checkpoint = if prepared.descriptor.mutability == ToolMutability::Mutating {
        match rollback {
            Some(recorder) => {
                match recorder.checkpoint_before(&prepared.call, &prepared.descriptor) {
                    Ok(checkpoint) => checkpoint,
                    Err(error) => {
                        let mut sandbox_metadata = prepared.sandbox_metadata;
                        if let Some(metadata) = sandbox_metadata.as_mut() {
                            metadata.exit_classification = exit_classification_from_error(&error);
                        }
                        return ToolDispatchOutcome::Failed(ToolDispatchFailure {
                            tool_call_id: prepared.call.tool_call_id,
                            tool_name: prepared.call.tool_name,
                            error,
                            doom_loop_signal: None,
                            rollback_payload: None,
                            rollback_error: None,
                            pre_hook_payload: prepared.pre_hook_payload,
                            post_hook_payload: json!({
                                "ok": false,
                                "preflight": "rollback_checkpoint_failed",
                            }),
                            elapsed_ms: prepared.started.elapsed().as_millis(),
                            sandbox_metadata,
                        });
                    }
                }
            }
            None => None,
        }
    } else {
        None
    };

    if control.is_cancelled() {
        return ToolDispatchOutcome::Failed(timeout_failure_from_prepared(prepared));
    }

    let mut raw_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prepared
            .handler
            .execute_with_control(context, &prepared.call, &control)
    }))
    .unwrap_or_else(|_| Err(handler_panic_error(&prepared.call)));
    if raw_result.is_ok() && control.is_cancelled() {
        raw_result = Err(ToolExecutionError::timeout(
            "agent_tool_group_timeout",
            format!(
                "Tool `{}` completed after the tool-group wall-clock budget expired.",
                prepared.call.tool_name
            ),
        ));
    }
    let post_hook_payload = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        prepared
            .handler
            .post_hook_payload(&prepared.call, &raw_result)
    })) {
        Ok(payload) => payload,
        Err(_) => {
            raw_result = Err(ToolExecutionError::retryable(
                "agent_tool_post_hook_panicked",
                format!(
                    "Post-hook for tool `{}` panicked while finalizing its result.",
                    prepared.call.tool_name
                ),
            ));
            json!({
            "toolCallId": &prepared.call.tool_call_id,
            "toolName": &prepared.call.tool_name,
            "ok": false,
            "postHookPanicked": true,
            })
        }
    };
    match raw_result {
        Ok(handler_output) => {
            let mut sandbox_metadata = prepared.sandbox_metadata;
            if let Some(metadata) = sandbox_metadata.as_mut() {
                metadata.exit_classification =
                    exit_classification_from_output(&handler_output.output);
            }
            let (output, truncation) = truncate_tool_output(
                handler_output.output,
                &prepared.descriptor,
                &prepared.budget,
            );
            let mut telemetry_attributes = prepared.descriptor.telemetry_attributes.clone();
            telemetry_attributes.extend(handler_output.telemetry_attributes);
            telemetry_attributes.extend(output_telemetry(&truncation));
            ToolDispatchOutcome::Succeeded(ToolDispatchSuccess {
                tool_call_id: prepared.call.tool_call_id,
                tool_name: prepared.call.tool_name,
                summary: handler_output.summary,
                output,
                truncation,
                pre_hook_payload: prepared.pre_hook_payload,
                post_hook_payload,
                telemetry_attributes,
                elapsed_ms: prepared.started.elapsed().as_millis(),
                sandbox_metadata,
            })
        }
        Err(error) => {
            let mut sandbox_metadata = prepared.sandbox_metadata;
            if let Some(metadata) = sandbox_metadata.as_mut() {
                metadata.exit_classification = exit_classification_from_error(&error);
            }
            let mut rollback_payload = None;
            let mut rollback_error = None;
            if let (Some(recorder), Some(checkpoint)) = (rollback, checkpoint.as_ref()) {
                match recorder.rollback_after_failure(
                    &prepared.call,
                    &prepared.descriptor,
                    checkpoint,
                    &error,
                ) {
                    Ok(payload) => rollback_payload = Some(payload),
                    Err(error) => rollback_error = Some(error),
                }
            }
            ToolDispatchOutcome::Failed(ToolDispatchFailure {
                tool_call_id: prepared.call.tool_call_id,
                tool_name: prepared.call.tool_name,
                error,
                doom_loop_signal: None,
                rollback_payload,
                rollback_error,
                pre_hook_payload: prepared.pre_hook_payload,
                post_hook_payload,
                elapsed_ms: prepared.started.elapsed().as_millis(),
                sandbox_metadata,
            })
        }
    }
}

fn timeout_failure_from_prepared(prepared: PreparedToolCall) -> ToolDispatchFailure {
    let mut sandbox_metadata = prepared.sandbox_metadata;
    if let Some(metadata) = sandbox_metadata.as_mut() {
        metadata.exit_classification = crate::SandboxExitClassification::Timeout;
    }
    failure_from_error_with_sandbox(
        &prepared.call,
        ToolExecutionError::timeout(
            "agent_tool_group_timeout",
            format!(
                "Tool `{}` exceeded the tool-group wall-clock budget before it completed.",
                prepared.call.tool_name
            ),
        ),
        prepared.pre_hook_payload,
        json!({ "ok": false, "timedOut": true, "cancelled": true }),
        prepared.started.elapsed(),
        sandbox_metadata,
    )
}

fn timeout_failure_from_pending(
    pending: PendingReadOnlyToolCall,
    budget: &ToolBudget,
) -> ToolDispatchFailure {
    let mut sandbox_metadata = pending.sandbox_metadata;
    if let Some(metadata) = sandbox_metadata.as_mut() {
        metadata.exit_classification = crate::SandboxExitClassification::Timeout;
    }
    failure_from_error_with_sandbox(
        &pending.call,
        ToolExecutionError::timeout(
            "agent_tool_group_timeout",
            format!(
                "Tool `{}` exceeded the {}ms read-only tool-group wall-clock budget.",
                pending.call.tool_name, budget.max_wall_clock_time_per_tool_group_ms
            ),
        ),
        pending.pre_hook_payload,
        json!({ "ok": false, "timedOut": true, "cancelled": true }),
        pending.started.elapsed(),
        sandbox_metadata,
    )
}

fn failure_from_pending_error(
    pending: PendingReadOnlyToolCall,
    error: ToolExecutionError,
) -> ToolDispatchFailure {
    let mut sandbox_metadata = pending.sandbox_metadata;
    if let Some(metadata) = sandbox_metadata.as_mut() {
        metadata.exit_classification = exit_classification_from_error(&error);
    }
    failure_from_error_with_sandbox(
        &pending.call,
        error,
        pending.pre_hook_payload,
        json!({ "ok": false, "workerStarted": false }),
        pending.started.elapsed(),
        sandbox_metadata,
    )
}

fn panic_failure_outcome(call: &ToolCallInput) -> ToolDispatchOutcome {
    ToolDispatchOutcome::Failed(failure_from_error(
        call,
        ToolExecutionError::retryable(
            "agent_tool_thread_panicked",
            "A read-only tool worker panicked.",
        ),
        json!({}),
        json!({ "ok": false, "panicked": true }),
        Duration::from_millis(0),
    ))
}

fn handler_panic_error(call: &ToolCallInput) -> ToolExecutionError {
    ToolExecutionError::retryable(
        "agent_tool_handler_panicked",
        format!(
            "Tool `{}` panicked while executing its handler.",
            call.tool_name
        ),
    )
}

fn exit_classification_from_output(output: &JsonValue) -> crate::SandboxExitClassification {
    if bool_field_recursive(output, &["timedOut", "timed_out"]).unwrap_or(false) {
        return crate::SandboxExitClassification::Timeout;
    }

    if bool_field_recursive(output, &["spawned"]).is_some_and(|spawned| !spawned) {
        return crate::SandboxExitClassification::NotRun;
    }

    match int_field_recursive(output, &["exitCode", "exit_code"]) {
        Some(0) => crate::SandboxExitClassification::Success,
        Some(_) => crate::SandboxExitClassification::Failed,
        None => crate::SandboxExitClassification::Success,
    }
}

fn exit_classification_from_error(error: &ToolExecutionError) -> crate::SandboxExitClassification {
    match error.category {
        ToolErrorCategory::SandboxDenied => crate::SandboxExitClassification::DeniedBySandbox,
        ToolErrorCategory::Timeout => crate::SandboxExitClassification::Timeout,
        _ => crate::SandboxExitClassification::Failed,
    }
}

fn int_field_recursive(value: &JsonValue, field_names: &[&str]) -> Option<i64> {
    match value {
        JsonValue::Object(fields) => {
            for field_name in field_names {
                if let Some(value) = fields.get(*field_name) {
                    if let Some(number) = value.as_i64() {
                        return Some(number);
                    }
                    if let Some(number) = value.as_u64().and_then(|value| i64::try_from(value).ok())
                    {
                        return Some(number);
                    }
                }
            }
            fields
                .values()
                .find_map(|value| int_field_recursive(value, field_names))
        }
        JsonValue::Array(items) => items
            .iter()
            .find_map(|value| int_field_recursive(value, field_names)),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) | JsonValue::String(_) => None,
    }
}

fn bool_field_recursive(value: &JsonValue, field_names: &[&str]) -> Option<bool> {
    match value {
        JsonValue::Object(fields) => {
            for field_name in field_names {
                if let Some(value) = fields.get(*field_name).and_then(JsonValue::as_bool) {
                    return Some(value);
                }
            }
            fields
                .values()
                .find_map(|value| bool_field_recursive(value, field_names))
        }
        JsonValue::Array(items) => items
            .iter()
            .find_map(|value| bool_field_recursive(value, field_names)),
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) | JsonValue::String(_) => None,
    }
}

fn output_telemetry(truncation: &ToolResultTruncationMetadata) -> BTreeMap<String, String> {
    BTreeMap::from([
        (
            "xero.tool.truncated".into(),
            truncation.was_truncated.to_string(),
        ),
        (
            "xero.tool.returned_bytes".into(),
            truncation.returned_bytes.to_string(),
        ),
    ])
}

fn failure_from_error(
    call: &ToolCallInput,
    error: ToolExecutionError,
    pre_hook_payload: JsonValue,
    post_hook_payload: JsonValue,
    elapsed: Duration,
) -> ToolDispatchFailure {
    failure_from_error_with_sandbox(
        call,
        error,
        pre_hook_payload,
        post_hook_payload,
        elapsed,
        None,
    )
}

fn failure_from_error_with_sandbox(
    call: &ToolCallInput,
    error: ToolExecutionError,
    pre_hook_payload: JsonValue,
    post_hook_payload: JsonValue,
    elapsed: Duration,
    sandbox_metadata: Option<SandboxExecutionMetadata>,
) -> ToolDispatchFailure {
    ToolDispatchFailure {
        tool_call_id: call.tool_call_id.clone(),
        tool_name: call.tool_name.clone(),
        error,
        doom_loop_signal: None,
        rollback_payload: None,
        rollback_error: None,
        pre_hook_payload,
        post_hook_payload,
        elapsed_ms: elapsed.as_millis(),
        sandbox_metadata,
    }
}

fn group_deadline(budget: &ToolBudget) -> Instant {
    Instant::now()
        .checked_add(Duration::from_millis(
            budget.max_wall_clock_time_per_tool_group_ms,
        ))
        .unwrap_or_else(Instant::now)
}

fn outcome_is_timeout(outcome: &ToolDispatchOutcome) -> bool {
    outcome
        .failure()
        .is_some_and(|failure| failure.error.category == ToolErrorCategory::Timeout)
}

fn timeout_error_for_elapsed(
    elapsed: Duration,
    budget: &ToolBudget,
    group_timed_out: bool,
) -> Option<ToolExecutionError> {
    let limit = Duration::from_millis(budget.max_wall_clock_time_per_tool_group_ms);
    (group_timed_out || elapsed > limit).then(|| {
        ToolExecutionError::timeout(
            "agent_tool_group_timeout",
            format!(
                "Tool group took {}ms, exceeding the {}ms budget.",
                elapsed.as_millis(),
                budget.max_wall_clock_time_per_tool_group_ms
            ),
        )
    })
}

fn truncate_tool_output(
    output: JsonValue,
    descriptor: &ToolDescriptorV2,
    budget: &ToolBudget,
) -> (JsonValue, ToolResultTruncationMetadata) {
    let max_output_bytes = descriptor
        .result_truncation
        .max_output_bytes
        .min(budget.max_command_output_bytes);
    let serialized = serde_json::to_string(&output).unwrap_or_else(|_| "null".into());
    let original_bytes = serialized.len();
    if original_bytes <= max_output_bytes {
        return (
            output,
            ToolResultTruncationMetadata::unchanged(original_bytes, max_output_bytes),
        );
    }

    if descriptor.result_truncation.preserve_json_shape {
        let shaped = truncate_preserving_json_shape(output, max_output_bytes);
        let returned_bytes = serde_json::to_string(&shaped)
            .map(|value| value.len())
            .unwrap_or(0);
        return (
            shaped,
            ToolResultTruncationMetadata {
                was_truncated: true,
                original_bytes,
                returned_bytes,
                omitted_bytes: original_bytes.saturating_sub(returned_bytes),
                max_output_bytes,
            },
        );
    }

    let preview = truncate_utf8(&serialized, max_output_bytes);
    let returned_bytes = preview.len();
    (
        json!({
            "xeroTruncated": true,
            "preview": preview,
            "originalBytes": original_bytes,
            "returnedBytes": returned_bytes,
            "omittedBytes": original_bytes.saturating_sub(returned_bytes),
        }),
        ToolResultTruncationMetadata {
            was_truncated: true,
            original_bytes,
            returned_bytes,
            omitted_bytes: original_bytes.saturating_sub(returned_bytes),
            max_output_bytes,
        },
    )
}

fn truncate_utf8(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    value[..end].to_string()
}

fn truncate_preserving_json_shape(mut output: JsonValue, max_output_bytes: usize) -> JsonValue {
    let mut string_budget = max_output_bytes.saturating_div(2).clamp(64, 4096);
    loop {
        let mut changed = false;
        truncate_json_value_in_place(&mut output, string_budget, &mut changed);
        if serde_json::to_string(&output)
            .map(|serialized| serialized.len() <= max_output_bytes)
            .unwrap_or(true)
        {
            break;
        }
        if string_budget <= 32 || !changed {
            add_shape_truncation_marker(&mut output, max_output_bytes);
            break;
        }
        string_budget /= 2;
    }
    add_shape_truncation_marker(&mut output, max_output_bytes);
    output
}

fn truncate_json_value_in_place(value: &mut JsonValue, string_budget: usize, changed: &mut bool) {
    match value {
        JsonValue::String(text) => {
            if text.len() > string_budget {
                let omitted = text.len().saturating_sub(string_budget);
                let mut truncated = truncate_utf8(text, string_budget);
                truncated.push_str(&format!("\n...[truncated {omitted} byte(s)]"));
                *text = truncated;
                *changed = true;
            }
        }
        JsonValue::Array(items) => {
            let max_items = 64;
            if items.len() > max_items {
                let omitted = items.len().saturating_sub(max_items);
                items.truncate(max_items);
                items.push(json!({
                    "xeroTruncatedArrayItems": omitted,
                }));
                *changed = true;
            }
            for item in items {
                truncate_json_value_in_place(item, string_budget, changed);
            }
        }
        JsonValue::Object(fields) => {
            for item in fields.values_mut() {
                truncate_json_value_in_place(item, string_budget, changed);
            }
        }
        JsonValue::Null | JsonValue::Bool(_) | JsonValue::Number(_) => {}
    }
}

fn add_shape_truncation_marker(output: &mut JsonValue, max_output_bytes: usize) {
    let marker = json!({
        "wasTruncated": true,
        "maxOutputBytes": max_output_bytes,
        "contract": "preserve_json_shape",
    });
    match output {
        JsonValue::Object(fields) => {
            fields.insert("xeroTruncation".into(), marker);
        }
        _ => {
            *output = json!({
                "value": output.clone(),
                "xeroTruncation": marker,
            });
        }
    }
}

fn validate_input_against_schema(
    descriptor: &ToolDescriptorV2,
    input: &JsonValue,
) -> ToolRegistryResult<()> {
    validate_json_value_against_schema(descriptor, "$", &descriptor.input_schema, input)
}

fn validate_json_value_against_schema(
    descriptor: &ToolDescriptorV2,
    path: &str,
    schema: &JsonValue,
    value: &JsonValue,
) -> ToolRegistryResult<()> {
    if let Some(branches) = schema.get("oneOf").and_then(JsonValue::as_array) {
        if branches.iter().any(|branch| {
            validate_json_value_against_schema(descriptor, path, branch, value).is_ok()
        }) {
            return Ok(());
        }
        return schema_error(
            descriptor,
            path,
            "must match at least one declared schema branch.",
        );
    }

    if let Some(branches) = schema.get("anyOf").and_then(JsonValue::as_array) {
        if branches.iter().any(|branch| {
            validate_json_value_against_schema(descriptor, path, branch, value).is_ok()
        }) {
            return Ok(());
        }
        return schema_error(
            descriptor,
            path,
            "must match at least one declared schema branch.",
        );
    }

    if let Some(branches) = schema.get("allOf").and_then(JsonValue::as_array) {
        for branch in branches {
            validate_json_value_against_schema(descriptor, path, branch, value)?;
        }
    }

    if let Some(enum_values) = schema.get("enum").and_then(JsonValue::as_array) {
        if !enum_values.iter().any(|allowed| allowed == value) {
            return schema_error(
                descriptor,
                path,
                format!("must be one of {}.", enum_values_for_message(enum_values)),
            );
        }
    }

    if let Some(expected_type) = schema.get("type").and_then(JsonValue::as_str) {
        if !schema_type_matches(expected_type, value) {
            return schema_error(
                descriptor,
                path,
                format!("must be `{expected_type}`, got {}.", json_type_name(value)),
            );
        }
    }

    if value.is_object()
        && (schema.get("properties").is_some()
            || schema.get("required").is_some()
            || schema.get("additionalProperties").is_some())
    {
        validate_object_against_schema(descriptor, path, schema, value)?;
    }

    if value.is_array() {
        validate_array_against_schema(descriptor, path, schema, value)?;
    }

    if value.is_number() {
        validate_number_bounds(descriptor, path, schema, value)?;
    }

    if value.is_string() {
        validate_string_bounds(descriptor, path, schema, value)?;
    }

    Ok(())
}

fn validate_object_against_schema(
    descriptor: &ToolDescriptorV2,
    path: &str,
    schema: &JsonValue,
    value: &JsonValue,
) -> ToolRegistryResult<()> {
    let Some(object) = value.as_object() else {
        return schema_error(descriptor, path, "must be `object`.");
    };

    if let Some(required) = schema.get("required").and_then(JsonValue::as_array) {
        for field in required.iter().filter_map(JsonValue::as_str) {
            if !object.contains_key(field) {
                return schema_error(
                    descriptor,
                    path,
                    format!("is missing required field `{field}`."),
                );
            }
        }
    }

    let properties = schema.get("properties").and_then(JsonValue::as_object);
    if let Some(properties) = properties {
        for (field, field_schema) in properties {
            let Some(field_value) = object.get(field) else {
                continue;
            };
            let field_path = child_path(path, field);
            validate_json_value_against_schema(descriptor, &field_path, field_schema, field_value)?;
        }
    }

    match schema.get("additionalProperties") {
        Some(JsonValue::Bool(false)) => {
            for field in object.keys() {
                let declared = properties
                    .map(|properties| properties.contains_key(field))
                    .unwrap_or(false);
                if !declared {
                    return schema_error(
                        descriptor,
                        &child_path(path, field),
                        "is not declared by this tool schema.",
                    );
                }
            }
        }
        Some(additional_schema) if additional_schema.is_object() => {
            for (field, field_value) in object {
                if properties.is_some_and(|properties| properties.contains_key(field)) {
                    continue;
                }
                let field_path = child_path(path, field);
                validate_json_value_against_schema(
                    descriptor,
                    &field_path,
                    additional_schema,
                    field_value,
                )?;
            }
        }
        _ => {}
    }

    Ok(())
}

fn validate_array_against_schema(
    descriptor: &ToolDescriptorV2,
    path: &str,
    schema: &JsonValue,
    value: &JsonValue,
) -> ToolRegistryResult<()> {
    let Some(items) = value.as_array() else {
        return schema_error(descriptor, path, "must be `array`.");
    };
    if let Some(min_items) = schema.get("minItems").and_then(JsonValue::as_u64) {
        if items.len() < min_items as usize {
            return schema_error(
                descriptor,
                path,
                format!("must contain at least {min_items} item(s)."),
            );
        }
    }
    if let Some(max_items) = schema.get("maxItems").and_then(JsonValue::as_u64) {
        if items.len() > max_items as usize {
            return schema_error(
                descriptor,
                path,
                format!("must contain at most {max_items} item(s)."),
            );
        }
    }
    if let Some(item_schema) = schema.get("items").filter(|schema| schema.is_object()) {
        for (index, item) in items.iter().enumerate() {
            validate_json_value_against_schema(
                descriptor,
                &format!("{path}[{index}]"),
                item_schema,
                item,
            )?;
        }
    }
    Ok(())
}

fn validate_number_bounds(
    descriptor: &ToolDescriptorV2,
    path: &str,
    schema: &JsonValue,
    value: &JsonValue,
) -> ToolRegistryResult<()> {
    let Some(actual) = value.as_f64() else {
        return Ok(());
    };
    if let Some(minimum) = schema.get("minimum").and_then(JsonValue::as_f64) {
        if actual < minimum {
            return schema_error(
                descriptor,
                path,
                format!("must be greater than or equal to {minimum}."),
            );
        }
    }
    if let Some(maximum) = schema.get("maximum").and_then(JsonValue::as_f64) {
        if actual > maximum {
            return schema_error(
                descriptor,
                path,
                format!("must be less than or equal to {maximum}."),
            );
        }
    }
    Ok(())
}

fn validate_string_bounds(
    descriptor: &ToolDescriptorV2,
    path: &str,
    schema: &JsonValue,
    value: &JsonValue,
) -> ToolRegistryResult<()> {
    let Some(actual) = value.as_str() else {
        return Ok(());
    };
    if let Some(min_length) = schema.get("minLength").and_then(JsonValue::as_u64) {
        if actual.chars().count() < min_length as usize {
            return schema_error(
                descriptor,
                path,
                format!("must contain at least {min_length} character(s)."),
            );
        }
    }
    if let Some(max_length) = schema.get("maxLength").and_then(JsonValue::as_u64) {
        if actual.chars().count() > max_length as usize {
            return schema_error(
                descriptor,
                path,
                format!("must contain at most {max_length} character(s)."),
            );
        }
    }
    Ok(())
}

fn schema_type_matches(expected_type: &str, value: &JsonValue) -> bool {
    match expected_type {
        "array" => value.is_array(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "object" => value.is_object(),
        "string" => value.is_string(),
        _ => true,
    }
}

fn schema_error(
    descriptor: &ToolDescriptorV2,
    path: &str,
    message: impl Into<String>,
) -> ToolRegistryResult<()> {
    Err(ToolExecutionError::invalid_input(
        "agent_tool_input_invalid",
        format!(
            "Tool `{}` input at `{path}` {}",
            descriptor.name,
            message.into()
        ),
    ))
}

fn child_path(parent: &str, child: &str) -> String {
    if parent == "$" {
        format!("$.{child}")
    } else {
        format!("{parent}.{child}")
    }
}

fn enum_values_for_message(values: &[JsonValue]) -> String {
    values
        .iter()
        .map(stable_json_signature)
        .collect::<Vec<_>>()
        .join(", ")
}

fn json_type_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "boolean",
        JsonValue::Number(number) if number.as_i64().is_some() || number.as_u64().is_some() => {
            "integer"
        }
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
}

fn tool_call_signature(call: &ToolCallInput) -> String {
    format!("{}\0{}", call.tool_name, stable_json_signature(&call.input))
}

fn stable_json_signature(value: &JsonValue) -> String {
    match value {
        JsonValue::Null => "null".into(),
        JsonValue::Bool(value) => value.to_string(),
        JsonValue::Number(value) => value.to_string(),
        JsonValue::String(value) => format!("{value:?}"),
        JsonValue::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(stable_json_signature)
                .collect::<Vec<_>>()
                .join(",")
        ),
        JsonValue::Object(map) => {
            let fields = map
                .iter()
                .map(|(key, value)| format!("{key:?}:{}", stable_json_signature(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Condvar, Mutex};
    use std::time::{Duration, Instant};

    use super::*;

    fn read_only_supervisor_test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn isolated_tool_registry() -> ToolRegistryV2 {
        ToolRegistryV2::with_read_only_worker_limit(DEFAULT_MAX_SUPERVISED_READ_ONLY_WORKERS)
    }
    use crate::{
        PermissionProfileSandbox, ProjectTrustState, SandboxApprovalSource,
        SandboxExecutionContext, SandboxExitClassification, SandboxNetworkMode,
        SandboxPermissionProfile, SandboxPlatform,
    };

    fn descriptor(name: &str, mutability: ToolMutability) -> ToolDescriptorV2 {
        ToolDescriptorV2 {
            name: name.into(),
            description: format!("Test tool {name}."),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "value": { "type": "string" }
                },
                "required": ["path"]
            }),
            capability_tags: vec!["test".into()],
            application_metadata: ToolApplicationMetadata::default(),
            effect_class: if mutability == ToolMutability::ReadOnly {
                ToolEffectClass::FileRead
            } else {
                ToolEffectClass::WorkspaceMutation
            },
            mutability,
            sandbox_requirement: if mutability == ToolMutability::ReadOnly {
                ToolSandboxRequirement::ReadOnly
            } else {
                ToolSandboxRequirement::WorkspaceWrite
            },
            approval_requirement: ToolApprovalRequirement::Policy,
            telemetry_attributes: BTreeMap::from([("xero.tool.kind".into(), "test".into())]),
            result_truncation: ToolResultTruncationContract {
                max_output_bytes: 256,
                preserve_json_shape: false,
            },
        }
    }

    fn call(id: &str, tool_name: &str, path: &str) -> ToolCallInput {
        ToolCallInput {
            tool_call_id: id.into(),
            tool_name: tool_name.into(),
            input: json!({ "path": path }),
        }
    }

    fn command_call(id: &str, tool_name: &str, argv: &[&str]) -> ToolCallInput {
        ToolCallInput {
            tool_call_id: id.into(),
            tool_name: tool_name.into(),
            input: json!({ "argv": argv }),
        }
    }

    fn extension_manifest() -> ToolExtensionManifest {
        ToolExtensionManifest {
            contract_version: TOOL_EXTENSION_MANIFEST_CONTRACT_VERSION,
            extension_id: "acme.release_notes".into(),
            tool_name: "acme.release_notes.generate".into(),
            label: "Release Notes".into(),
            description: "Generate release-note draft data from approved changelog input.".into(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "changeId": { "type": "string", "minLength": 1 }
                },
                "required": ["changeId"]
            }),
            permission: ToolExtensionPermissionManifest {
                permission_id: "release_notes_generate".into(),
                label: "Generate release notes".into(),
                effect_class: ToolEffectClass::ExternalService,
                risk_class: "external_review".into(),
                audit_label: "release_notes_external_generation".into(),
            },
            mutability: ToolMutability::ReadOnly,
            sandbox_requirement: ToolSandboxRequirement::Network,
            approval_requirement: ToolApprovalRequirement::Policy,
            application_metadata: ToolApplicationMetadata::default(),
            capability_tags: vec!["extension".into(), "release_notes".into()],
            test_fixtures: vec![ToolExtensionTestFixture {
                fixture_id: "happy_path".into(),
                input: json!({ "changeId": "change-1" }),
                expected_summary_contains: Some("generated".into()),
            }],
            runtime: ToolExtensionRuntimeManifest {
                kind: ToolExtensionRuntimeKind::Process,
                executable: "handler".into(),
                args: Vec::new(),
            },
        }
    }

    fn sandbox_config() -> ToolDispatchConfig {
        ToolDispatchConfig {
            sandbox: Arc::new(PermissionProfileSandbox::new(SandboxExecutionContext {
                workspace_root: "/repo".into(),
                project_trust: ProjectTrustState::Trusted,
                approval_source: SandboxApprovalSource::Operator,
                platform: SandboxPlatform::Macos,
                preserved_environment_keys: vec!["PATH".into()],
                ..SandboxExecutionContext::default()
            })),
            ..ToolDispatchConfig::default()
        }
    }

    #[test]
    fn register_rejects_invalid_descriptor_input() {
        let mut registry = ToolRegistryV2::new();
        let mut bad = descriptor("", ToolMutability::ReadOnly);
        bad.name.clear();

        let error = registry
            .register(StaticToolHandler::new(bad, |_context, _call| {
                Ok(ToolHandlerOutput::new("ok", json!({})))
            }))
            .expect_err("empty tool names must be rejected");

        assert_eq!(error.category, ToolErrorCategory::InvalidInput);
    }

    #[test]
    fn descriptor_validation_rejects_noncanonical_capability_tags() {
        for capability_tags in [
            vec![" test".into()],
            vec!["test ".into()],
            vec!["test".into(), "test".into()],
            vec!["".into()],
        ] {
            let mut candidate = descriptor("tagged_tool", ToolMutability::ReadOnly);
            candidate.capability_tags = capability_tags;

            let error = candidate
                .validate()
                .expect_err("capability tags must be nonblank, canonical, and unique");
            assert_eq!(error.code, "agent_tool_descriptor_invalid");
        }
    }

    #[test]
    fn descriptor_validation_covers_every_application_safety_contract() {
        let legacy = ToolDescriptorV2::from_legacy_descriptor(
            "legacy_observe",
            "Observe legacy state.",
            json!({"type": "object"}),
        );
        assert_eq!(legacy.application_metadata, ToolApplicationMetadata::default());
        assert_eq!(legacy.effect_class, ToolEffectClass::Observe);
        assert_eq!(legacy.mutability, ToolMutability::ReadOnly);
        assert_eq!(legacy.sandbox_requirement, ToolSandboxRequirement::None);
        assert_eq!(legacy.approval_requirement, ToolApprovalRequirement::Policy);
        legacy.validate().expect("legacy descriptor defaults are valid");

        let mut invalid_cases = Vec::new();

        let mut missing_description = descriptor("missing_description", ToolMutability::ReadOnly);
        missing_description.description = "  ".into();
        invalid_cases.push(missing_description);

        let mut non_object_schema = descriptor("non_object", ToolMutability::ReadOnly);
        non_object_schema.input_schema = json!("string");
        invalid_cases.push(non_object_schema);

        let mut zero_output = descriptor("zero_output", ToolMutability::ReadOnly);
        zero_output.result_truncation.max_output_bytes = 0;
        invalid_cases.push(zero_output);

        let mut missing_family = descriptor("missing_family", ToolMutability::ReadOnly);
        missing_family.application_metadata.family = " ".into();
        invalid_cases.push(missing_family);

        let mut granular_batch = descriptor("granular_batch", ToolMutability::ReadOnly);
        granular_batch.application_metadata.dispatch_safety =
            ToolBatchDispatchSafety::ParallelReadOnly;
        invalid_cases.push(granular_batch);

        let mut mutating_read_batch = descriptor("mutating_read_batch", ToolMutability::Mutating);
        mutating_read_batch.application_metadata = ToolApplicationMetadata {
            family: "search".into(),
            kind: ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec!["read_only".into(), "bounded_results".into()],
        };
        invalid_cases.push(mutating_read_batch);

        let mut sequential_read_batch = descriptor("sequential_read_batch", ToolMutability::ReadOnly);
        sequential_read_batch.application_metadata = ToolApplicationMetadata {
            family: "search".into(),
            kind: ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: ToolBatchDispatchSafety::SequentialMutating,
            safety_requirements: vec!["read_only".into(), "bounded_results".into()],
        };
        invalid_cases.push(sequential_read_batch);

        let mut parallel_mutation = descriptor("parallel_mutation", ToolMutability::Mutating);
        parallel_mutation.application_metadata = ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::MutatingBatch,
            dispatch_safety: ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec![
                "supports_preview".into(),
                "validates_all_targets_before_writing".into(),
                "reports_summary".into(),
            ],
        };
        invalid_cases.push(parallel_mutation);

        let mut missing_target_validation = descriptor("missing_validation", ToolMutability::Mutating);
        missing_target_validation.application_metadata = ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::Declarative,
            dispatch_safety: ToolBatchDispatchSafety::ToolOwnedAtomic,
            safety_requirements: vec!["supports_dry_run".into(), "reports_diff".into()],
        };
        invalid_cases.push(missing_target_validation);

        let mut missing_report = descriptor("missing_report", ToolMutability::Mutating);
        missing_report.application_metadata = ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::Declarative,
            dispatch_safety: ToolBatchDispatchSafety::ToolOwnedAtomic,
            safety_requirements: vec![
                "supports_dry_run".into(),
                "validates_targets_before_writing".into(),
            ],
        };
        invalid_cases.push(missing_report);

        for candidate in invalid_cases {
            let error = candidate
                .validate()
                .expect_err("invalid application safety contract must fail closed");
            assert_eq!(error.code, "agent_tool_descriptor_invalid");
        }

        let mut valid_read_batch = descriptor("valid_read_batch", ToolMutability::ReadOnly);
        valid_read_batch.application_metadata = ToolApplicationMetadata {
            family: "search".into(),
            kind: ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec!["read_only".into(), "bounded_results".into()],
        };
        valid_read_batch.validate().expect("valid read-only batch");

        let mut valid_mutating_batch = descriptor("valid_mutating_batch", ToolMutability::Mutating);
        valid_mutating_batch.application_metadata = ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::MutatingBatch,
            dispatch_safety: ToolBatchDispatchSafety::SequentialMutating,
            safety_requirements: vec![
                "supports_preview".into(),
                "validates_all_targets_before_writing".into(),
                "reports_summary".into(),
            ],
        };
        valid_mutating_batch
            .validate()
            .expect("valid mutating batch");
    }

    #[test]
    fn register_rejects_tool_application_batch_descriptors_without_safety_metadata() {
        let mut registry = ToolRegistryV2::new();
        let mut search = descriptor("search_batch", ToolMutability::ReadOnly);
        search.application_metadata = ToolApplicationMetadata {
            family: "discovery".into(),
            kind: ToolApplicationKind::ReadOnlyBatch,
            dispatch_safety: ToolBatchDispatchSafety::ParallelReadOnly,
            safety_requirements: vec!["read_only".into()],
        };

        let error = registry
            .register(StaticToolHandler::new(search, |_context, _call| {
                Ok(ToolHandlerOutput::new("ok", json!({})))
            }))
            .expect_err("read-only batches must describe bounded results");
        assert_eq!(error.category, ToolErrorCategory::InvalidInput);

        let mut patch = descriptor("patch_batch", ToolMutability::Mutating);
        patch.application_metadata = ToolApplicationMetadata {
            family: "edit".into(),
            kind: ToolApplicationKind::Declarative,
            dispatch_safety: ToolBatchDispatchSafety::ToolOwnedAtomic,
            safety_requirements: vec![
                "validates_all_targets_before_writing".into(),
                "reports_diff".into(),
            ],
        };
        let error = registry
            .register(StaticToolHandler::new(patch, |_context, _call| {
                Ok(ToolHandlerOutput::new("ok", json!({})))
            }))
            .expect_err("mutating declarative tools must describe preview or dry-run safety");
        assert_eq!(error.category, ToolErrorCategory::InvalidInput);
    }

    #[test]
    fn pre_hook_payload_includes_action_level_policy_metadata() {
        let mut descriptor = descriptor("command_verify", ToolMutability::Mutating);
        descriptor.effect_class = ToolEffectClass::CommandExecution;
        descriptor.sandbox_requirement = ToolSandboxRequirement::WorkspaceWrite;
        descriptor.approval_requirement = ToolApprovalRequirement::Policy;
        let handler = StaticToolHandler::new(descriptor, |_context, _call| {
            Ok(ToolHandlerOutput::new("ok", json!({})))
        });

        let payload = handler.pre_hook_payload(&ToolCallInput {
            tool_call_id: "call-verify".into(),
            tool_name: "command_verify".into(),
            input: json!({ "argv": ["cargo", "test"] }),
        });

        assert_eq!(payload["toolName"], json!("command_verify"));
        assert_eq!(payload["effectClass"], json!("command_execution"));
        assert_eq!(payload["sandboxRequirement"], json!("workspace_write"));
        assert_eq!(payload["approvalRequirement"], json!("policy"));
    }

    #[test]
    fn s20_extension_manifest_registers_permissioned_non_builtin_tools() {
        let manifest = extension_manifest();
        let descriptor = manifest.descriptor();
        assert_eq!(
            descriptor.telemetry_attributes["xero.extension.audit_label"],
            "release_notes_external_generation"
        );
        assert_eq!(descriptor.effect_class, ToolEffectClass::ExternalService);

        let mut registry = ToolRegistryV2::new();
        registry
            .register(
                StaticToolHandler::try_from_extension_manifest(manifest, |_context, call| {
                    Ok(ToolHandlerOutput::new(
                        format!(
                            "generated {}",
                            call.input["changeId"].as_str().unwrap_or("unknown")
                        ),
                        json!({"draftId": "draft-1"}),
                    ))
                })
                .expect("extension handler"),
            )
            .expect("register extension tool");

        let report = registry.dispatch_batch(
            &[ToolCallInput {
                tool_call_id: "call-extension-1".into(),
                tool_name: "acme.release_notes.generate".into(),
                input: json!({ "changeId": "change-1" }),
            }],
            &ToolDispatchConfig::default(),
        );

        let ToolDispatchOutcome::Succeeded(success) = &report.groups[0].outcomes[0] else {
            panic!("extension tool should succeed");
        };
        assert_eq!(success.summary, "generated change-1");
        assert_eq!(
            success.pre_hook_payload["effectClass"],
            json!("external_service")
        );
        assert_eq!(
            success
                .telemetry_attributes
                .get("xero.extension.permission_id"),
            Some(&"release_notes_generate".to_string())
        );
    }

    #[test]
    fn s20_extension_manifest_validation_rejects_unsafe_or_untestable_manifests() {
        let mut bad = extension_manifest();
        bad.permission.permission_id = " bad permission ".into();
        let error = bad.validate().expect_err("permission id should be stable");
        assert_eq!(error.code, "agent_tool_extension_identifier_invalid");

        let mut untestable = extension_manifest();
        untestable.test_fixtures.clear();
        let error = untestable
            .validate()
            .expect_err("extension manifests should declare fixtures");
        assert_eq!(error.code, "agent_tool_extension_fixture_missing");

        let mut duplicate_fixture = extension_manifest();
        duplicate_fixture
            .test_fixtures
            .push(ToolExtensionTestFixture {
                fixture_id: "happy_path".into(),
                input: json!({ "changeId": "change-2" }),
                expected_summary_contains: Some("generated".into()),
            });
        let error = duplicate_fixture
            .validate()
            .expect_err("duplicate fixtures should fail");
        assert_eq!(error.code, "agent_tool_extension_fixture_duplicate");
    }

    #[test]
    fn extension_manifest_validation_covers_contract_schema_runtime_and_fixture_boundaries() {
        let mut cases = Vec::new();

        let mut unsupported = extension_manifest();
        unsupported.contract_version += 1;
        cases.push((unsupported, "agent_tool_extension_contract_unsupported"));

        let mut blank_label = extension_manifest();
        blank_label.label = " ".into();
        cases.push((blank_label, "agent_tool_extension_text_invalid"));

        let mut blank_description = extension_manifest();
        blank_description.description.clear();
        cases.push((blank_description, "agent_tool_extension_text_invalid"));

        let mut invalid_schema = extension_manifest();
        invalid_schema.input_schema = json!([]);
        cases.push((invalid_schema, "agent_tool_extension_schema_invalid"));

        let mut blank_permission_label = extension_manifest();
        blank_permission_label.permission.label = "\t".into();
        cases.push((blank_permission_label, "agent_tool_extension_text_invalid"));

        let mut blank_audit_label = extension_manifest();
        blank_audit_label.permission.audit_label.clear();
        cases.push((blank_audit_label, "agent_tool_extension_text_invalid"));

        for executable in ["", " handler", "../handler", "bin/handler"] {
            let mut invalid_executable = extension_manifest();
            invalid_executable.runtime.executable = executable.into();
            cases.push((
                invalid_executable,
                "agent_tool_extension_executable_invalid",
            ));
        }

        let mut nul_argument = extension_manifest();
        nul_argument.runtime.args = vec!["bad\0argument".into()];
        cases.push((nul_argument, "agent_tool_extension_arguments_invalid"));

        let mut oversized_argument = extension_manifest();
        oversized_argument.runtime.args = vec!["x".repeat(4_097)];
        cases.push((
            oversized_argument,
            "agent_tool_extension_arguments_invalid",
        ));

        let mut invalid_fixture_input = extension_manifest();
        invalid_fixture_input.test_fixtures[0].input = json!("not-an-object");
        cases.push((
            invalid_fixture_input,
            "agent_tool_extension_fixture_invalid",
        ));

        let mut blank_fixture_summary = extension_manifest();
        blank_fixture_summary.test_fixtures[0].expected_summary_contains = Some("  ".into());
        cases.push((
            blank_fixture_summary,
            "agent_tool_extension_fixture_invalid",
        ));

        for (manifest, expected_code) in cases {
            let error = manifest
                .validate()
                .expect_err("invalid extension boundary must fail closed");
            assert_eq!(error.code, expected_code);
        }
    }

    #[test]
    fn execution_controls_budgets_and_static_policy_fail_closed_at_boundaries() {
        let cancellation = ToolCancellationToken::new();
        let control = ToolExecutionControl::new(
            Some(Instant::now() + Duration::from_secs(1)),
            cancellation.clone(),
        );
        assert!(control.remaining().is_some_and(|remaining| !remaining.is_zero()));
        control
            .ensure_not_cancelled("read_file")
            .expect("future deadline remains runnable");
        cancellation.cancel();
        let cancelled = control
            .ensure_not_cancelled("read_file")
            .expect_err("explicit cancellation must stop the tool");
        assert_eq!(cancelled.category, ToolErrorCategory::Timeout);
        assert_eq!(cancelled.code, "agent_tool_call_cancelled");

        let expired = ToolExecutionControl::new(
            Some(Instant::now() - Duration::from_millis(1)),
            ToolCancellationToken::new(),
        );
        assert_eq!(expired.remaining(), Some(Duration::ZERO));
        assert!(expired.is_cancelled());

        let mut call_budget = ToolBudget::default();
        call_budget.max_tool_calls_per_turn = 1;
        let mut tracker = ToolBudgetTracker::new(call_budget);
        let first = call("budget-1", "read_file", "one.txt");
        tracker.record_call(&first).expect("first budgeted call");
        let exhausted = tracker
            .record_call(&call("budget-2", "read_file", "two.txt"))
            .expect_err("second call must exceed a one-call budget");
        assert_eq!(exhausted.code, "agent_tool_budget_calls_exceeded");

        let mut failure_budget = ToolBudget::default();
        failure_budget.max_tool_failures_per_turn = 0;
        let mut tracker = ToolBudgetTracker::new(failure_budget);
        let exhausted = tracker
            .record_failure(
                &first,
                &ToolExecutionError::retryable("fixture_failure", "fixture failed"),
            )
            .expect_err("first failure must exceed a zero-failure budget");
        assert_eq!(exhausted.code, "agent_tool_budget_failures_exceeded");
        assert_eq!(
            tracker
                .doom_loop_mut()
                .record_completion_claim(1, true),
            Some(ToolDoomLoopSignal::PendingTodosIgnoredAfterCompletionClaim)
        );
        assert_eq!(
            ToolExecutionError::doom_loop("fixture_loop", "looped").category,
            ToolErrorCategory::DoomLoopDetected
        );

        let mut always = descriptor("always", ToolMutability::ReadOnly);
        always.approval_requirement = ToolApprovalRequirement::Always;
        assert!(matches!(
            AllowAllToolPolicy.evaluate(&always, &call("policy-1", "always", "one")),
            ToolPolicyDecision::RequireApproval { .. }
        ));

        let policy = StaticToolPolicy::default()
            .deny_tool("denied", "blocked by fixture")
            .require_approval_for_tool("review", "review fixture");
        assert!(matches!(
            policy.evaluate(
                &descriptor("denied", ToolMutability::ReadOnly),
                &call("policy-2", "denied", "one")
            ),
            ToolPolicyDecision::Deny { .. }
        ));
        assert!(matches!(
            policy.evaluate(
                &descriptor("review", ToolMutability::ReadOnly),
                &call("policy-3", "review", "one")
            ),
            ToolPolicyDecision::RequireApproval { .. }
        ));
        assert_eq!(
            policy.evaluate(
                &descriptor("allowed", ToolMutability::ReadOnly),
                &call("policy-4", "allowed", "one")
            ),
            ToolPolicyDecision::Allow
        );
    }

    #[test]
    fn registry_internal_contract_helpers_cover_recovery_classification_and_shape_boundaries() {
        let helper_descriptor = descriptor("helper", ToolMutability::ReadOnly);
        let call = call("helper-call", "helper", "src/lib.rs");
        let handler = StaticToolHandler::new(helper_descriptor.clone(), |_context, _call| {
            Ok(ToolHandlerOutput::new(
                "helper output",
                json!({ "exitCode": 0 }),
            ))
        });
        assert!(!handler.requires_parent_process_execution(&call));
        handler
            .validate_input(&call.input)
            .expect("handler schema validation");
        assert_eq!(handler.pre_hook_payload(&call)["toolName"], "helper");
        let output = handler
            .execute(&ToolExecutionContext::default(), &call)
            .expect("default execute path");
        assert_eq!(output.summary, "helper output");
        assert_eq!(handler.post_hook_payload(&call, &Ok(output))["ok"], true);

        let rollback = RecordingRollback;
        assert!(!rollback.requires_parent_process());
        assert_eq!(
            rollback
                .recover_after_termination(
                    &call,
                    &helper_descriptor,
                    None,
                    &ToolExecutionError::retryable("terminated", "terminated"),
                )
                .expect_err("recovery requires a checkpoint")
                .code,
            "agent_tool_mutation_checkpoint_unavailable"
        );
        assert_eq!(
            rollback
                .recover_after_termination(
                    &call,
                    &helper_descriptor,
                    Some(&json!({ "path": "src/lib.rs" })),
                    &ToolExecutionError::retryable("terminated", "terminated"),
                )
                .expect("rollback recovery")["rolledBack"],
            "helper-call"
        );

        let manifest = extension_manifest();
        let mismatch_handler = StaticToolHandler::new(
            descriptor("other", ToolMutability::ReadOnly),
            |_context, _call| Ok(ToolHandlerOutput::new("unused", json!({}))),
        );
        assert_eq!(
            mismatch_handler
                .verify_extension_test_fixtures(&manifest, &ToolExecutionContext::default())
                .expect_err("handler/manifest mismatch")
                .code,
            "agent_tool_extension_handler_mismatch"
        );
        let failing_fixture_handler = StaticToolHandler::try_from_extension_manifest(
            manifest.clone(),
            |_context, _call| Err(ToolExecutionError::retryable("fixture_failed", "failed")),
        )
        .expect("extension handler");
        let report = failing_fixture_handler
            .verify_extension_test_fixtures(&manifest, &ToolExecutionContext::default())
            .expect("fixture report");
        assert!(!report.passed);
        assert_eq!(report.fixtures[0].status, ToolExtensionFixtureStatus::Failed);

        let paths = mutation_affected_paths(
            &json!({
                "path": " src/lib.rs ",
                "paths": ["src/main.rs", "", 1],
                "nested": { "sourcePath": "src/old.rs", "ignored": "secret" },
                "writeSet": [{ "pathAfter": "src/generated.rs" }]
            }),
            Some(&json!({
                "pathBefore": "src/original.rs",
                "files": ["src/lib.rs", "src/extra.rs"]
            })),
        );
        assert_eq!(
            paths,
            vec![
                "src/extra.rs",
                "src/generated.rs",
                "src/lib.rs",
                "src/main.rs",
                "src/old.rs",
                "src/original.rs",
            ]
        );

        for (value, expected) in [
            (
                json!({ "timedOut": true }),
                SandboxExitClassification::Timeout,
            ),
            (
                json!({ "spawned": false }),
                SandboxExitClassification::NotRun,
            ),
            (
                json!({ "nested": [{ "exit_code": 0 }] }),
                SandboxExitClassification::Success,
            ),
            (
                json!({ "exitCode": 7 }),
                SandboxExitClassification::Failed,
            ),
            (json!({}), SandboxExitClassification::Success),
        ] {
            assert_eq!(exit_classification_from_output(&value), expected);
        }
        assert_eq!(
            exit_classification_from_error(&ToolExecutionError::sandbox_denied(
                "denied", "denied"
            )),
            SandboxExitClassification::DeniedBySandbox
        );
        assert_eq!(
            exit_classification_from_error(&ToolExecutionError::timeout("timeout", "timeout")),
            SandboxExitClassification::Timeout
        );
        assert_eq!(
            exit_classification_from_error(&ToolExecutionError::retryable("failed", "failed")),
            SandboxExitClassification::Failed
        );
        assert_eq!(
            int_field_recursive(
                &json!({ "nested": [{ "exitCode": -2 }] }),
                &["exitCode"]
            ),
            Some(-2)
        );
        assert_eq!(
            bool_field_recursive(&json!([{ "nested": { "ok": false } }]), &["ok"]),
            Some(false)
        );
        assert_eq!(int_field_recursive(&json!(true), &["exitCode"]), None);
        assert_eq!(bool_field_recursive(&json!(1), &["ok"]), None);

        let unchanged = ToolResultTruncationMetadata::unchanged(8, 16);
        let telemetry = output_telemetry(&unchanged);
        assert_eq!(telemetry["xero.tool.truncated"], "false");
        assert_eq!(telemetry["xero.tool.returned_bytes"], "8");
        assert!(group_deadline(&ToolBudget::default()) > Instant::now());
        assert!(timeout_error_for_elapsed(
            Duration::from_secs(2),
            &ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 1,
                ..ToolBudget::default()
            },
            false,
        )
        .is_some());
        assert!(timeout_error_for_elapsed(Duration::ZERO, &ToolBudget::default(), false).is_none());

        let timeout_outcome = ToolDispatchOutcome::Failed(failure_from_error(
            &call,
            ToolExecutionError::timeout("timeout", "timeout"),
            json!({}),
            json!({}),
            Duration::ZERO,
        ));
        assert!(outcome_is_timeout(&timeout_outcome));
        assert_eq!(
            panic_failure_outcome(&call)
                .failure()
                .expect("panic failure")
                .error
                .code,
            "agent_tool_thread_panicked"
        );
        assert_eq!(handler_panic_error(&call).code, "agent_tool_handler_panicked");

        let mut large = json!({
            "items": (0..70)
                .map(|index| json!({ "value": format!("{index}-{}", "x".repeat(80)) }))
                .collect::<Vec<_>>()
        });
        let mut changed = false;
        truncate_json_value_in_place(&mut large, 16, &mut changed);
        assert!(changed);
        assert_eq!(large["items"].as_array().expect("items").len(), 65);
        let scalar = truncate_preserving_json_shape(json!("x".repeat(500)), 80);
        assert_eq!(scalar["xeroTruncation"]["wasTruncated"], true);
        assert_eq!(truncate_utf8("éclair", 1), "");
        assert_eq!(truncate_utf8("short", 10), "short");
    }

    #[test]
    fn direct_mutation_post_hook_panic_matches_isolated_worker_failure_semantics() {
        let mut registry = isolated_tool_registry();
        registry
            .register(PanickingPostHook {
                descriptor: descriptor("direct-post-panic", ToolMutability::Mutating),
            })
            .expect("register panicking post-hook");
        let call = call(
            "direct-post-panic-call",
            "direct-post-panic",
            "src/lib.rs",
        );
        let config = ToolDispatchConfig::default();
        let prepared = registry
            .prepare_call(
                &call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                ToolCancellationToken::new(),
            )
            .expect("prepare direct mutation");

        let outcome = execute_prepared_call(
            prepared,
            &config.context,
            Some(&RecordingRollback),
        );

        let failure = outcome.failure().expect("post-hook panic must fail closed");
        assert_eq!(failure.error.code, "agent_tool_post_hook_panicked");
        assert!(failure.rollback_payload.is_some());
        assert_eq!(failure.post_hook_payload["postHookPanicked"], true);
    }

    #[test]
    fn direct_prepared_and_pending_failures_preserve_typed_supervision_metadata() {
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("direct-helper", ToolMutability::Mutating),
                |_context, call| {
                    if call.input.get("fail").is_some() {
                        Err(ToolExecutionError::retryable("handler_failed", "failed"))
                    } else {
                        Ok(ToolHandlerOutput::new(
                            "direct success",
                            json!({ "exitCode": 0 }),
                        ))
                    }
                },
            ))
            .expect("register direct helper");
        let config = ToolDispatchConfig::default();
        let prepare = |call: &ToolCallInput, cancellation_token: ToolCancellationToken| {
            registry
                .prepare_call(
                    call,
                    &mut ToolBudgetTracker::new(config.budget.clone()),
                    &config,
                    Instant::now() + Duration::from_secs(1),
                    cancellation_token,
                )
                .expect("prepare helper call")
        };

        let checkpoint_call = call(
            "checkpoint-failure",
            "direct-helper",
            "src/checkpoint.rs",
        );
        let checkpoint_failure = execute_prepared_call(
            prepare(&checkpoint_call, ToolCancellationToken::new()),
            &config.context,
            Some(&FailingCheckpoint),
        );
        assert_eq!(
            checkpoint_failure
                .failure()
                .expect("checkpoint failure")
                .error
                .code,
            "checkpoint_denied"
        );

        let success_call = call("direct-success", "direct-helper", "src/success.rs");
        let success = execute_prepared_call(
            prepare(&success_call, ToolCancellationToken::new()),
            &config.context,
            None,
        );
        assert!(matches!(success, ToolDispatchOutcome::Succeeded(_)));

        let mut failure_call = call("direct-failure", "direct-helper", "src/failure.rs");
        failure_call.input["fail"] = json!(true);
        let failed = execute_prepared_call(
            prepare(&failure_call, ToolCancellationToken::new()),
            &config.context,
            Some(&RecordingRollback),
        );
        let failure = failed.failure().expect("handler failure");
        assert_eq!(failure.error.code, "handler_failed");
        assert!(failure.rollback_payload.is_some());

        let cancellation = ToolCancellationToken::new();
        let cancelled_prepared = prepare(
            &call("direct-cancelled", "direct-helper", "src/cancelled.rs"),
            cancellation.clone(),
        );
        cancellation.cancel();
        let cancelled = execute_prepared_call(cancelled_prepared, &config.context, None);
        assert_eq!(
            cancelled.failure().expect("cancelled prepared call").error.code,
            "agent_tool_group_timeout"
        );

        let pending_prepared = prepare(
            &call("pending", "direct-helper", "src/pending.rs"),
            ToolCancellationToken::new(),
        );
        let pending = PendingReadOnlyToolCall::from_prepared(&pending_prepared);
        let timeout = timeout_failure_from_pending(pending, &config.budget);
        assert_eq!(timeout.error.code, "agent_tool_group_timeout");
        let pending = PendingReadOnlyToolCall::from_prepared(&pending_prepared);
        let unavailable = failure_from_pending_error(
            pending,
            ToolExecutionError::unavailable("worker_unavailable", "worker unavailable"),
        );
        assert_eq!(unavailable.error.code, "worker_unavailable");

        let pending_mutation = PendingMutationToolCall::from_prepared(&pending_prepared);
        for (cancelled, crashed, code, classification) in [
            (
                false,
                false,
                "agent_tool_mutation_terminated",
                SandboxExitClassification::Timeout,
            ),
            (
                true,
                false,
                "agent_tool_mutation_cancelled",
                SandboxExitClassification::Cancelled,
            ),
            (
                false,
                true,
                "agent_tool_mutation_worker_crashed",
                SandboxExitClassification::Unknown,
            ),
        ] {
            let failure = pending_mutation.terminated_failure(
                MutationWorkerPhase::Handler,
                cancelled,
                crashed,
            );
            assert_eq!(failure.error.code, code);
            assert_eq!(
                failure
                    .sandbox_metadata
                    .expect("sandbox metadata")
                    .exit_classification,
                classification
            );
        }
        let unavailable = PendingMutationToolCall::from_prepared(&pending_prepared)
            .unavailable_failure(ToolExecutionError::unavailable(
                "boundary_unavailable",
                "boundary unavailable",
            ));
        assert_eq!(unavailable.error.code, "boundary_unavailable");

        let prior_call = call("prior", "direct-helper", "src/prior.rs");
        let quarantine = MutationQuarantine {
            project_id: config.context.project_id.clone(),
            call: prior_call,
            descriptor: descriptor("direct-helper", ToolMutability::Mutating),
            checkpoint: Some(json!({ "path": "src/prior.rs" })),
            error: ToolExecutionError::timeout("prior_timeout", "prior timeout"),
            rollback_error: ToolExecutionError::retryable(
                "rollback_failed",
                "rollback failed",
            ),
            phase: MutationWorkerPhase::Rollback,
            rollback: None,
        };
        let quarantined = pending_mutation.quarantined_failure(&quarantine);
        assert_eq!(quarantined.error.code, "agent_tool_mutation_quarantined");
        assert_eq!(
            quarantined.error.telemetry_attributes["xero.mutation.quarantined_call_id"],
            "prior"
        );

        let plain_scope = mutation_execution_scope(&ToolExecutionContext {
            project_id: "project-1".into(),
            ..ToolExecutionContext::default()
        });
        assert_eq!(plain_scope, "project-1");
        let scoped = mutation_execution_scope(&ToolExecutionContext {
            project_id: "project-1".into(),
            telemetry_attributes: BTreeMap::from([(
                MUTATION_EXECUTION_SCOPE_ATTRIBUTE.into(),
                "agent-2".into(),
            )]),
            ..ToolExecutionContext::default()
        });
        assert_eq!(scoped, "project-1\u{1f}agent-2");
    }

    #[test]
    fn s20_extension_fixture_verifier_executes_declared_fixtures() {
        let manifest = extension_manifest();
        let handler =
            StaticToolHandler::try_from_extension_manifest(manifest.clone(), |_context, call| {
                Ok(ToolHandlerOutput::new(
                    format!(
                        "generated {}",
                        call.input["changeId"].as_str().unwrap_or("unknown")
                    ),
                    json!({"draftId": "draft-1"}),
                ))
            })
            .expect("extension handler");

        let report = handler
            .verify_extension_test_fixtures(&manifest, &ToolExecutionContext::default())
            .expect("fixture report");

        assert!(report.passed);
        assert_eq!(report.extension_id, "acme.release_notes");
        assert_eq!(report.fixtures.len(), 1);
        assert_eq!(
            report.fixtures[0].status,
            ToolExtensionFixtureStatus::Passed
        );
        assert_eq!(
            report.fixtures[0].summary.as_deref(),
            Some("generated change-1")
        );
    }

    #[test]
    fn s20_extension_fixture_verifier_reports_summary_mismatches() {
        let manifest = extension_manifest();
        let handler =
            StaticToolHandler::try_from_extension_manifest(manifest.clone(), |_context, _call| {
                Ok(ToolHandlerOutput::new("ignored input", json!({})))
            })
            .expect("extension handler");

        let report = handler
            .verify_extension_test_fixtures(&manifest, &ToolExecutionContext::default())
            .expect("fixture report");

        assert!(!report.passed);
        assert_eq!(
            report.fixtures[0].status,
            ToolExtensionFixtureStatus::Failed
        );
        assert_eq!(report.fixtures[0].summary.as_deref(), Some("ignored input"));
        assert!(report.fixtures[0]
            .diagnostic
            .as_deref()
            .is_some_and(|diagnostic| diagnostic.contains("generated")));
    }

    #[test]
    fn s20_extension_tool_policy_denials_block_before_execution() {
        let mut registry = ToolRegistryV2::new();
        let executed = Arc::new(AtomicBool::new(false));
        let executed_for_handler = Arc::clone(&executed);
        registry
            .register(
                StaticToolHandler::try_from_extension_manifest(
                    extension_manifest(),
                    move |_context, _call| {
                        executed_for_handler.store(true, Ordering::SeqCst);
                        Ok(ToolHandlerOutput::new("generated", json!({})))
                    },
                )
                .expect("extension handler"),
            )
            .expect("register extension tool");

        let config = ToolDispatchConfig {
            policy: Arc::new(StaticToolPolicy::default().deny_tool(
                "acme.release_notes.generate",
                "Extension permission was not granted for this custom agent.",
            )),
            ..ToolDispatchConfig::default()
        };
        let report = registry.dispatch_batch(
            &[ToolCallInput {
                tool_call_id: "call-extension-denied".into(),
                tool_name: "acme.release_notes.generate".into(),
                input: json!({ "changeId": "change-1" }),
            }],
            &config,
        );

        let failure = report.groups[0].outcomes[0].failure().expect("denied");
        assert_eq!(failure.error.category, ToolErrorCategory::PolicyDenied);
        assert!(!executed.load(Ordering::SeqCst));
    }

    #[test]
    fn dispatch_rejects_invalid_tool_input_against_schema() {
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("read_file", ToolMutability::ReadOnly),
                |_context, _call| Ok(ToolHandlerOutput::new("ok", json!({}))),
            ))
            .expect("register read tool");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            ToolCallInput {
                tool_call_id: "call-1".into(),
                tool_name: "read_file".into(),
                input: json!({ "path": 42 }),
            },
            &mut tracker,
            &ToolDispatchConfig::default(),
        );

        assert_eq!(
            outcome.failure().unwrap().error.category,
            ToolErrorCategory::InvalidInput
        );
    }

    #[test]
    fn dispatch_rejects_enum_violations_and_undeclared_properties() {
        let mut descriptor = descriptor("project_context_search", ToolMutability::ReadOnly);
        descriptor.input_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["search_project_records", "search_approved_memory"]
                },
                "query": { "type": "string" }
            }
        });
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(descriptor, |_context, _call| {
                Ok(ToolHandlerOutput::new("ok", json!({})))
            }))
            .expect("register project context search");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let invalid_enum = registry.dispatch_call(
            ToolCallInput {
                tool_call_id: "call-invalid-enum".into(),
                tool_name: "project_context_search".into(),
                input: json!({ "action": "delete_everything", "query": "context" }),
            },
            &mut tracker,
            &ToolDispatchConfig::default(),
        );
        assert_eq!(
            invalid_enum.failure().unwrap().error.category,
            ToolErrorCategory::InvalidInput
        );

        let unexpected_field = registry.dispatch_call(
            ToolCallInput {
                tool_call_id: "call-extra".into(),
                tool_name: "project_context_search".into(),
                input: json!({
                    "action": "search_project_records",
                    "query": "context",
                    "surprise": true
                }),
            },
            &mut tracker,
            &ToolDispatchConfig::default(),
        );
        assert_eq!(
            unexpected_field.failure().unwrap().error.category,
            ToolErrorCategory::InvalidInput
        );
    }

    #[test]
    fn dispatch_validates_nested_objects_arrays_and_integer_bounds() {
        let mut descriptor = descriptor("structured_tool", ToolMutability::ReadOnly);
        descriptor.input_schema = json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["config", "paths", "limit"],
            "properties": {
                "config": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["mode"],
                    "properties": {
                        "mode": { "type": "string", "enum": ["fast", "thorough"] },
                        "dryRun": { "type": "boolean" }
                    }
                },
                "paths": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 3,
                    "items": { "type": "string" }
                },
                "limit": { "type": "integer", "minimum": 1, "maximum": 5 }
            }
        });
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(descriptor, |_context, _call| {
                Ok(ToolHandlerOutput::new("ok", json!({ "accepted": true })))
            }))
            .expect("register structured tool");

        let valid = registry.dispatch_call(
            ToolCallInput {
                tool_call_id: "call-valid".into(),
                tool_name: "structured_tool".into(),
                input: json!({
                    "config": { "mode": "fast", "dryRun": true },
                    "paths": ["src/lib.rs"],
                    "limit": 3
                }),
            },
            &mut ToolBudgetTracker::new(ToolBudget::default()),
            &ToolDispatchConfig::default(),
        );
        assert!(matches!(valid, ToolDispatchOutcome::Succeeded(_)));

        for (id, input) in [
            (
                "call-bad-array",
                json!({
                    "config": { "mode": "fast" },
                    "paths": ["src/lib.rs", 42],
                    "limit": 3
                }),
            ),
            (
                "call-bad-nested",
                json!({
                    "config": { "mode": "sideways" },
                    "paths": ["src/lib.rs"],
                    "limit": 3
                }),
            ),
            (
                "call-bad-bound",
                json!({
                    "config": { "mode": "fast" },
                    "paths": ["src/lib.rs"],
                    "limit": 9
                }),
            ),
        ] {
            let outcome = registry.dispatch_call(
                ToolCallInput {
                    tool_call_id: id.into(),
                    tool_name: "structured_tool".into(),
                    input,
                },
                &mut ToolBudgetTracker::new(ToolBudget::default()),
                &ToolDispatchConfig::default(),
            );
            assert_eq!(
                outcome.failure().unwrap().error.category,
                ToolErrorCategory::InvalidInput
            );
        }
    }

    #[test]
    fn dispatch_validates_schema_composition_dynamic_properties_and_all_bounds() {
        let mut schema_descriptor = descriptor("schema_matrix", ToolMutability::ReadOnly);
        schema_descriptor.input_schema = json!({
            "type": "object",
            "required": ["choice", "combined", "extras", "items", "ratio", "nullable"],
            "properties": {
                "choice": {
                    "anyOf": [
                        {"type": "string", "enum": ["auto", "manual"]},
                        {"type": "integer", "minimum": 1, "maximum": 2}
                    ]
                },
                "combined": {
                    "allOf": [
                        {"type": "string"},
                        {"minLength": 2},
                        {"maxLength": 4}
                    ]
                },
                "extras": {
                    "type": "object",
                    "properties": {"known": {"type": "string"}},
                    "additionalProperties": {"type": "string", "minLength": 1}
                },
                "items": {
                    "type": "array",
                    "minItems": 1,
                    "maxItems": 2,
                    "items": {"type": "number", "minimum": 0.0, "maximum": 1.0}
                },
                "ratio": {"type": "number", "minimum": 0.0, "maximum": 1.0},
                "nullable": {"oneOf": [{"type": "null"}, {"type": "boolean"}]}
            },
            "additionalProperties": false
        });
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                schema_descriptor,
                |_context, _call| Ok(ToolHandlerOutput::new("ok", json!({}))),
            ))
            .expect("register schema matrix");

        let valid = json!({
            "choice": 2,
            "combined": "okay",
            "extras": {"known": "yes", "dynamic": "value"},
            "items": [0.0, 1.0],
            "ratio": 0.5,
            "nullable": null
        });
        let outcome = registry.dispatch_call(
            ToolCallInput {
                tool_call_id: "schema-valid".into(),
                tool_name: "schema_matrix".into(),
                input: valid.clone(),
            },
            &mut ToolBudgetTracker::new(ToolBudget::default()),
            &ToolDispatchConfig::default(),
        );
        assert!(matches!(outcome, ToolDispatchOutcome::Succeeded(_)));

        let invalid_inputs = [
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [0.5],
                "ratio": 0.5
            }),
            json!({
                "choice": "invalid", "combined": "okay", "extras": {}, "items": [0.5],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "x", "extras": {}, "items": [0.5],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "excess", "extras": {}, "items": [0.5],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {"dynamic": 42}, "items": [0.5],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {"dynamic": ""}, "items": [0.5],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [0.1, 0.2, 0.3],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [-0.1],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [1.1],
                "ratio": 0.5, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [0.5],
                "ratio": -0.1, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [0.5],
                "ratio": 1.1, "nullable": true
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [0.5],
                "ratio": 0.5, "nullable": "no"
            }),
            json!({
                "choice": 2, "combined": "okay", "extras": {}, "items": [0.5],
                "ratio": 0.5, "nullable": true, "unknown": false
            }),
        ];

        for (index, input) in invalid_inputs.into_iter().enumerate() {
            let outcome = registry.dispatch_call(
                ToolCallInput {
                    tool_call_id: format!("schema-invalid-{index}"),
                    tool_name: "schema_matrix".into(),
                    input,
                },
                &mut ToolBudgetTracker::new(ToolBudget::default()),
                &ToolDispatchConfig::default(),
            );
            assert_eq!(
                outcome.failure().expect("schema failure").error.category,
                ToolErrorCategory::InvalidInput,
                "invalid schema fixture {index}"
            );
        }
    }

    #[test]
    fn dispatch_reports_policy_denial_before_execution() {
        let executed = Arc::new(Mutex::new(false));
        let executed_for_handler = Arc::clone(&executed);
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("write_file", ToolMutability::Mutating),
                move |_context, _call| {
                    *executed_for_handler.lock().unwrap() = true;
                    Ok(ToolHandlerOutput::new("wrote", json!({})))
                },
            ))
            .expect("register write tool");
        let config = ToolDispatchConfig {
            policy: Arc::new(
                StaticToolPolicy::default()
                    .deny_tool("write_file", "Writes are denied in this test."),
            ),
            ..ToolDispatchConfig::default()
        };
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            call("call-1", "write_file", "src/lib.rs"),
            &mut tracker,
            &config,
        );

        assert_eq!(
            outcome.failure().unwrap().error.category,
            ToolErrorCategory::PolicyDenied
        );
        assert!(!*executed.lock().unwrap());
    }

    #[test]
    fn sandbox_denies_write_escape_even_when_policy_allows() {
        let executed = Arc::new(Mutex::new(false));
        let executed_for_handler = Arc::clone(&executed);
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("write_file", ToolMutability::Mutating),
                move |_context, _call| {
                    *executed_for_handler.lock().unwrap() = true;
                    Ok(ToolHandlerOutput::new("wrote", json!({})))
                },
            ))
            .expect("register write tool");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            call("call-1", "write_file", "../outside.txt"),
            &mut tracker,
            &sandbox_config(),
        );

        let failure = outcome.failure().expect("sandbox failure");
        assert_eq!(failure.error.category, ToolErrorCategory::SandboxDenied);
        assert_eq!(failure.error.code, "agent_sandbox_path_denied");
        assert_eq!(
            failure
                .sandbox_metadata
                .as_ref()
                .expect("sandbox metadata")
                .exit_classification,
            SandboxExitClassification::DeniedBySandbox
        );
        assert!(!*executed.lock().unwrap());
    }

    #[test]
    fn sandbox_denies_network_command_before_handler_execution() {
        let executed = Arc::new(Mutex::new(false));
        let executed_for_handler = Arc::clone(&executed);
        let mut command_descriptor = descriptor("command", ToolMutability::Mutating);
        command_descriptor.effect_class = ToolEffectClass::CommandExecution;
        command_descriptor.sandbox_requirement = ToolSandboxRequirement::WorkspaceWrite;
        command_descriptor.input_schema = json!({
            "type": "object",
            "properties": {
                "argv": {
                    "type": "array"
                }
            },
            "required": ["argv"]
        });
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                command_descriptor,
                move |_context, _call| {
                    *executed_for_handler.lock().unwrap() = true;
                    Ok(ToolHandlerOutput::new("ran", json!({ "exitCode": 0 })))
                },
            ))
            .expect("register command tool");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            command_call("call-1", "command", &["curl", "https://example.com"]),
            &mut tracker,
            &sandbox_config(),
        );

        let failure = outcome.failure().expect("sandbox failure");
        assert_eq!(failure.error.category, ToolErrorCategory::SandboxDenied);
        assert_eq!(failure.error.code, "agent_sandbox_network_denied");
        let metadata = failure.sandbox_metadata.as_ref().expect("sandbox metadata");
        assert_eq!(metadata.network_mode, SandboxNetworkMode::Denied);
        assert_eq!(metadata.profile, SandboxPermissionProfile::WorkspaceWrite);
        assert!(!*executed.lock().unwrap());
    }

    #[test]
    fn dispatch_reports_approval_waiting_without_execution() {
        let mut descriptor = descriptor("dangerous_write", ToolMutability::Mutating);
        descriptor.approval_requirement = ToolApprovalRequirement::Always;
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(descriptor, |_context, _call| {
                Ok(ToolHandlerOutput::new("wrote", json!({})))
            }))
            .expect("register approval tool");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            call("call-1", "dangerous_write", "src/lib.rs"),
            &mut tracker,
            &ToolDispatchConfig::default(),
        );

        assert_eq!(
            outcome.failure().unwrap().error.category,
            ToolErrorCategory::ApprovalRequired
        );
    }

    #[derive(Debug, Default)]
    struct RecordingRollback;

    impl ToolRollback for RecordingRollback {
        fn checkpoint_before(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            Ok(Some(json!({ "checkpointFor": call.tool_call_id })))
        }

        fn rollback_after_failure(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            Ok(json!({ "rolledBack": call.tool_call_id, "checkpoint": checkpoint }))
        }
    }

    #[cfg(unix)]
    #[test]
    fn mutation_worker_ipc_reports_the_complete_success_lifecycle_in_process() {
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("worker_success", ToolMutability::Mutating),
                |_context, call| {
                    Ok(ToolHandlerOutput::new(
                        "worker succeeded",
                        json!({"path": call.input["path"]}),
                    ))
                },
            ))
            .expect("register worker success tool");
        let config = ToolDispatchConfig::default();
        let worker_call = call("worker-success-1", "worker_success", "src/lib.rs");
        let prepared = registry
            .prepare_call(
                &worker_call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                ToolCancellationToken::new(),
            )
            .expect("prepare mutation worker call");
        let (parent_stream, mut worker_stream) =
            UnixStream::pair().expect("mutation worker fixture socket");

        execute_mutation_worker(
            prepared,
            &config.context,
            None,
            None,
            &mut worker_stream,
        );
        drop(worker_stream);
        let (sender, receiver) = mpsc::channel();
        read_mutation_worker_messages(parent_stream, sender);
        let messages = receiver.into_iter().collect::<Vec<_>>();

        assert!(matches!(
            messages.as_slice(),
            [
                MutationWorkerMessage::Phase(MutationWorkerPhase::Checkpoint),
                MutationWorkerMessage::Checkpoint(None),
                MutationWorkerMessage::Phase(MutationWorkerPhase::Handler),
                MutationWorkerMessage::Phase(MutationWorkerPhase::PostHook),
                MutationWorkerMessage::Phase(MutationWorkerPhase::Completed),
                MutationWorkerMessage::Outcome(_),
            ]
        ));
        let MutationWorkerMessage::Outcome(outcome) = messages.last().expect("worker outcome")
        else {
            panic!("last worker message must be the outcome");
        };
        let ToolDispatchOutcome::Succeeded(success) = outcome.as_ref() else {
            panic!("worker mutation should succeed");
        };
        assert_eq!(success.summary, "worker succeeded");
        assert_eq!(success.output["path"], "src/lib.rs");
    }

    #[cfg(unix)]
    #[test]
    fn mutation_worker_ipc_reports_checkpoint_and_rollback_on_handler_failure() {
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("worker_failure", ToolMutability::Mutating),
                |_context, _call| {
                    Err(ToolExecutionError::retryable(
                        "worker_fixture_failed",
                        "worker fixture failed after its checkpoint",
                    ))
                },
            ))
            .expect("register worker failure tool");
        let config = ToolDispatchConfig::default();
        let worker_call = call("worker-failure-1", "worker_failure", "src/lib.rs");
        let prepared = registry
            .prepare_call(
                &worker_call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                ToolCancellationToken::new(),
            )
            .expect("prepare failing mutation worker call");
        let (parent_stream, mut worker_stream) =
            UnixStream::pair().expect("mutation worker fixture socket");
        let rollback = RecordingRollback;

        execute_mutation_worker(
            prepared,
            &config.context,
            Some(&rollback),
            None,
            &mut worker_stream,
        );
        drop(worker_stream);
        let (sender, receiver) = mpsc::channel();
        read_mutation_worker_messages(parent_stream, sender);
        let messages = receiver.into_iter().collect::<Vec<_>>();

        assert!(matches!(
            messages.as_slice(),
            [
                MutationWorkerMessage::Phase(MutationWorkerPhase::Checkpoint),
                MutationWorkerMessage::Checkpoint(Some(_)),
                MutationWorkerMessage::Phase(MutationWorkerPhase::Handler),
                MutationWorkerMessage::Phase(MutationWorkerPhase::PostHook),
                MutationWorkerMessage::Phase(MutationWorkerPhase::Rollback),
                MutationWorkerMessage::Phase(MutationWorkerPhase::Completed),
                MutationWorkerMessage::Outcome(_),
            ]
        ));
        let MutationWorkerMessage::Outcome(outcome) = messages.last().expect("worker outcome")
        else {
            panic!("last worker message must be the outcome");
        };
        let failure = outcome.failure().expect("worker mutation should fail");
        assert_eq!(failure.error.code, "worker_fixture_failed");
        assert_eq!(
            failure.rollback_payload.as_ref().map(|payload| &payload["rolledBack"]),
            Some(&json!("worker-failure-1"))
        );
        assert!(failure.rollback_error.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn mutation_worker_ipc_contains_checkpoint_errors_and_panics() {
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("checkpoint_worker", ToolMutability::Mutating),
                |_context, _call| Ok(ToolHandlerOutput::new("must not execute", json!({}))),
            ))
            .expect("register checkpoint worker tool");
        let config = ToolDispatchConfig::default();
        let failing = FailingCheckpoint;
        let panicking = PanickingCheckpoint;

        for (index, rollback, expected_code) in [
            (
                1,
                &failing as &dyn ToolRollback,
                "checkpoint_denied",
            ),
            (
                2,
                &panicking as &dyn ToolRollback,
                "agent_tool_checkpoint_panicked",
            ),
        ] {
            let worker_call = call(
                &format!("checkpoint-worker-{index}"),
                "checkpoint_worker",
                "src/lib.rs",
            );
            let prepared = registry
                .prepare_call(
                    &worker_call,
                    &mut ToolBudgetTracker::new(config.budget.clone()),
                    &config,
                    Instant::now() + Duration::from_secs(1),
                    ToolCancellationToken::new(),
                )
                .expect("prepare checkpoint mutation worker call");
            let (parent_stream, mut worker_stream) =
                UnixStream::pair().expect("mutation worker fixture socket");

            execute_mutation_worker(
                prepared,
                &config.context,
                Some(rollback),
                None,
                &mut worker_stream,
            );
            drop(worker_stream);
            let (sender, receiver) = mpsc::channel();
            read_mutation_worker_messages(parent_stream, sender);
            let mut messages = receiver.into_iter().collect::<Vec<_>>();
            let MutationWorkerMessage::Outcome(outcome) =
                messages.pop().expect("checkpoint worker outcome")
            else {
                panic!("last worker message must be the outcome");
            };
            assert_eq!(
                outcome.failure().expect("checkpoint must fail").error.code,
                expected_code
            );
            assert!(matches!(
                messages.as_slice(),
                [MutationWorkerMessage::Phase(MutationWorkerPhase::Checkpoint)]
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn mutation_worker_ipc_fails_closed_before_handler_on_cancellation_and_publish_failure() {
        #[derive(Debug)]
        struct TrackingRollback {
            rolled_back: Arc<AtomicBool>,
        }

        impl ToolRollback for TrackingRollback {
            fn checkpoint_before(
                &self,
                call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
            ) -> ToolRegistryResult<Option<JsonValue>> {
                Ok(Some(json!({ "checkpointFor": call.tool_call_id })))
            }

            fn rollback_after_failure(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                _checkpoint: &JsonValue,
                error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                self.rolled_back.store(true, Ordering::SeqCst);
                assert_eq!(
                    error.code,
                    "agent_tool_mutation_checkpoint_publish_failed"
                );
                Ok(json!({ "rolledBack": true }))
            }
        }

        let executed = Arc::new(AtomicBool::new(false));
        let executed_for_handler = Arc::clone(&executed);
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("worker_preflight", ToolMutability::Mutating),
                move |_context, _call| {
                    executed_for_handler.store(true, Ordering::SeqCst);
                    Ok(ToolHandlerOutput::new("must not execute", json!({})))
                },
            ))
            .expect("register preflight worker tool");
        let config = ToolDispatchConfig::default();

        let cancellation = ToolCancellationToken::new();
        let cancelled_call = call(
            "worker-cancelled-1",
            "worker_preflight",
            "src/cancelled.rs",
        );
        let cancelled = registry
            .prepare_call(
                &cancelled_call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                cancellation.clone(),
            )
            .expect("prepare cancelled worker call");
        cancellation.cancel();
        let (parent_stream, mut worker_stream) =
            UnixStream::pair().expect("cancelled worker fixture socket");
        execute_mutation_worker(
            cancelled,
            &config.context,
            None,
            None,
            &mut worker_stream,
        );
        drop(worker_stream);
        let (sender, receiver) = mpsc::channel();
        read_mutation_worker_messages(parent_stream, sender);
        let messages = receiver.into_iter().collect::<Vec<_>>();
        let MutationWorkerMessage::Outcome(outcome) = messages.last().expect("cancelled outcome")
        else {
            panic!("cancelled worker must publish an outcome");
        };
        assert_eq!(
            outcome.failure().expect("cancelled worker must fail").error.code,
            "agent_tool_group_timeout"
        );
        assert!(!executed.load(Ordering::SeqCst));

        let publish_call = call(
            "worker-publish-1",
            "worker_preflight",
            "src/publish.rs",
        );
        let prepared = registry
            .prepare_call(
                &publish_call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                ToolCancellationToken::new(),
            )
            .expect("prepare checkpoint publication call");
        let rolled_back = Arc::new(AtomicBool::new(false));
        let rollback = TrackingRollback {
            rolled_back: Arc::clone(&rolled_back),
        };
        let (peer, mut disconnected_worker) =
            UnixStream::pair().expect("disconnected worker fixture socket");
        drop(peer);
        execute_mutation_worker(
            prepared,
            &config.context,
            Some(&rollback),
            None,
            &mut disconnected_worker,
        );

        assert!(rolled_back.load(Ordering::SeqCst));
        assert!(!executed.load(Ordering::SeqCst));
    }

    #[cfg(unix)]
    #[test]
    fn mutation_worker_ipc_contains_post_hook_and_rollback_failures() {
        #[derive(Debug)]
        struct FailingRollback;

        impl ToolRollback for FailingRollback {
            fn checkpoint_before(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
            ) -> ToolRegistryResult<Option<JsonValue>> {
                Ok(Some(json!({ "checkpoint": true })))
            }

            fn rollback_after_failure(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                _checkpoint: &JsonValue,
                _error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                Err(ToolExecutionError::retryable(
                    "rollback_fixture_failed",
                    "rollback fixture failed",
                ))
            }
        }

        let config = ToolDispatchConfig::default();
        let mut post_registry = isolated_tool_registry();
        post_registry
            .register(PanickingPostHook {
                descriptor: descriptor("worker_post_panic", ToolMutability::Mutating),
            })
            .expect("register worker post-hook fixture");
        let post_call = call(
            "worker-post-panic-1",
            "worker_post_panic",
            "src/post.rs",
        );
        let prepared = post_registry
            .prepare_call(
                &post_call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                ToolCancellationToken::new(),
            )
            .expect("prepare post-hook worker call");
        let (parent_stream, mut worker_stream) =
            UnixStream::pair().expect("post-hook worker fixture socket");
        execute_mutation_worker(
            prepared,
            &config.context,
            Some(&RecordingRollback),
            None,
            &mut worker_stream,
        );
        drop(worker_stream);
        let (sender, receiver) = mpsc::channel();
        read_mutation_worker_messages(parent_stream, sender);
        let messages = receiver.into_iter().collect::<Vec<_>>();
        let MutationWorkerMessage::Outcome(outcome) = messages.last().expect("post-hook outcome")
        else {
            panic!("post-hook worker must publish an outcome");
        };
        let failure = outcome.failure().expect("post-hook panic must fail");
        assert_eq!(failure.error.code, "agent_tool_post_hook_panicked");
        assert!(failure.rollback_payload.is_some());

        let mut failure_registry = isolated_tool_registry();
        failure_registry
            .register(StaticToolHandler::new(
                descriptor("worker_rollback", ToolMutability::Mutating),
                |_context, _call| {
                    Err(ToolExecutionError::retryable(
                        "handler_fixture_failed",
                        "handler fixture failed",
                    ))
                },
            ))
            .expect("register rollback worker fixture");
        let failing = FailingRollback;
        let panicking = PanickingRollback;

        for (index, rollback, expected_code) in [
            (
                1,
                &failing as &dyn ToolRollback,
                "rollback_fixture_failed",
            ),
            (
                2,
                &panicking as &dyn ToolRollback,
                "agent_tool_rollback_panicked",
            ),
        ] {
            let worker_call = call(
                &format!("worker-rollback-{index}"),
                "worker_rollback",
                "src/rollback.rs",
            );
            let prepared = failure_registry
                .prepare_call(
                    &worker_call,
                    &mut ToolBudgetTracker::new(config.budget.clone()),
                    &config,
                    Instant::now() + Duration::from_secs(1),
                    ToolCancellationToken::new(),
                )
                .expect("prepare rollback worker call");
            let (parent_stream, mut worker_stream) =
                UnixStream::pair().expect("rollback worker fixture socket");
            execute_mutation_worker(
                prepared,
                &config.context,
                Some(rollback),
                None,
                &mut worker_stream,
            );
            drop(worker_stream);
            let (sender, receiver) = mpsc::channel();
            read_mutation_worker_messages(parent_stream, sender);
            let messages = receiver.into_iter().collect::<Vec<_>>();
            let MutationWorkerMessage::Outcome(outcome) =
                messages.last().expect("rollback outcome")
            else {
                panic!("rollback worker must publish an outcome");
            };
            assert_eq!(
                outcome
                    .failure()
                    .expect("handler and rollback must fail")
                    .rollback_error
                    .as_ref()
                    .expect("typed rollback error")
                    .code,
                expected_code
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn mutation_quarantine_recovery_contains_missing_parent_and_panicking_providers() {
        #[derive(Debug)]
        struct ParentRecovery {
            panics: bool,
        }

        impl ToolRollback for ParentRecovery {
            fn requires_parent_process(&self) -> bool {
                true
            }

            fn checkpoint_before(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
            ) -> ToolRegistryResult<Option<JsonValue>> {
                Ok(None)
            }

            fn rollback_after_failure(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                _checkpoint: &JsonValue,
                _error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                unreachable!("recovery fixture overrides termination recovery")
            }

            fn recover_after_termination(
                &self,
                call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                checkpoint: Option<&JsonValue>,
                _error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                if self.panics {
                    panic!("parent recovery fixture panic");
                }
                Ok(json!({
                    "recovered": call.tool_call_id,
                    "checkpoint": checkpoint,
                }))
            }
        }

        let config = ToolDispatchConfig::default();
        let recovery_call = call(
            "quarantine-recovery-1",
            "recover_mutation",
            "src/recovery.rs",
        );
        let quarantine = |rollback: Option<Arc<dyn ToolRollback>>| MutationQuarantine {
            project_id: config.context.project_id.clone(),
            call: recovery_call.clone(),
            descriptor: descriptor("recover_mutation", ToolMutability::Mutating),
            checkpoint: Some(json!({ "before": true })),
            error: ToolExecutionError::timeout("terminated", "worker terminated"),
            rollback_error: ToolExecutionError::retryable("pending", "recovery pending"),
            phase: MutationWorkerPhase::Rollback,
            rollback,
        };

        let missing = recover_mutation_quarantine(&quarantine(None), &config)
            .expect_err("missing recovery provider must fail closed");
        assert_eq!(missing.code, "agent_tool_mutation_recovery_unavailable");

        let recovered = recover_mutation_quarantine(
            &quarantine(Some(Arc::new(ParentRecovery { panics: false }))),
            &config,
        )
        .expect("parent-process recovery should succeed");
        assert_eq!(recovered["recovered"], "quarantine-recovery-1");
        assert_eq!(recovered["checkpoint"]["before"], true);

        let panicked = recover_mutation_quarantine(
            &quarantine(Some(Arc::new(ParentRecovery { panics: true }))),
            &config,
        )
        .expect_err("parent-process recovery panic must be contained");
        assert_eq!(panicked.code, "agent_tool_rollback_panicked");
    }

    #[cfg(unix)]
    #[test]
    fn mutation_ipc_readers_reject_truncated_oversized_and_malformed_frames() {
        fn worker_messages_for(payload: &[u8]) -> Vec<MutationWorkerMessage> {
            let (mut writer, reader) = UnixStream::pair().expect("worker IPC fixture socket");
            writer.write_all(payload).expect("write worker IPC fixture");
            drop(writer);
            let (sender, receiver) = mpsc::channel();
            read_mutation_worker_messages(reader, sender);
            receiver.into_iter().collect()
        }

        assert!(worker_messages_for(&4_u32.to_be_bytes()).is_empty());
        assert!(worker_messages_for(
            &u32::try_from(MAX_MUTATION_WORKER_MESSAGE_BYTES + 1)
                .expect("bounded IPC constant")
                .to_be_bytes()
        )
        .is_empty());
        let mut malformed = 1_u32.to_be_bytes().to_vec();
        malformed.push(b'{');
        assert!(worker_messages_for(&malformed).is_empty());

        let (mut writer, reader) = UnixStream::pair().expect("disconnected receiver socket");
        send_mutation_worker_message(
            &mut writer,
            &MutationWorkerMessage::Phase(MutationWorkerPhase::Handler),
        )
        .expect("write valid worker message");
        drop(writer);
        let (sender, receiver) = mpsc::channel();
        drop(receiver);
        read_mutation_worker_messages(reader, sender);

        for payload in [
            Vec::new(),
            u32::try_from(MAX_MUTATION_WORKER_MESSAGE_BYTES + 1)
                .expect("bounded recovery constant")
                .to_be_bytes()
                .to_vec(),
            {
                let mut payload = 2_u32.to_be_bytes().to_vec();
                payload.push(b'{');
                payload
            },
        ] {
            let (mut writer, reader) = UnixStream::pair().expect("recovery IPC fixture socket");
            writer
                .write_all(&payload)
                .expect("write recovery IPC fixture");
            drop(writer);
            assert!(read_mutation_recovery_result(reader).is_none());
        }
    }

    #[test]
    fn registry_schema_and_mutation_metadata_helpers_cover_defensive_boundaries() {
        let descriptor = descriptor("schema_helper", ToolMutability::ReadOnly);
        for schema in [
            json!({ "oneOf": [{ "type": "string" }] }),
            json!({ "anyOf": [{ "type": "boolean" }] }),
            json!({ "allOf": [{ "type": "string" }] }),
        ] {
            assert!(validate_json_value_against_schema(
                &descriptor,
                "$",
                &schema,
                &json!(42)
            )
            .is_err());
        }
        assert!(validate_object_against_schema(
            &descriptor,
            "$",
            &json!({}),
            &json!([])
        )
        .is_err());
        assert!(validate_array_against_schema(
            &descriptor,
            "$",
            &json!({}),
            &json!({})
        )
        .is_err());
        assert!(validate_number_bounds(
            &descriptor,
            "$",
            &json!({ "minimum": 1 }),
            &json!("not-a-number")
        )
        .is_ok());
        assert!(validate_string_bounds(
            &descriptor,
            "$",
            &json!({ "minLength": 1 }),
            &json!(false)
        )
        .is_ok());

        let additional_schema = json!({
            "type": "object",
            "properties": {},
            "additionalProperties": { "type": "string" },
        });
        assert!(validate_json_value_against_schema(
            &descriptor,
            "$",
            &additional_schema,
            &json!({ "extra": 1 })
        )
        .is_err());
        for (schema, value) in [
            (json!({ "type": "array", "minItems": 2 }), json!([1])),
            (json!({ "type": "array", "maxItems": 1 }), json!([1, 2])),
            (json!({ "type": "number", "minimum": 2 }), json!(1)),
            (json!({ "type": "number", "maximum": 2 }), json!(3)),
            (json!({ "type": "string", "minLength": 2 }), json!("x")),
            (json!({ "type": "string", "maxLength": 1 }), json!("xx")),
        ] {
            assert!(validate_json_value_against_schema(
                &descriptor,
                "$",
                &schema,
                &value
            )
            .is_err());
        }

        for (expected_type, value) in [
            ("array", json!([])),
            ("boolean", json!(true)),
            ("integer", json!(1)),
            ("null", JsonValue::Null),
            ("number", json!(1.5)),
            ("object", json!({})),
            ("string", json!("value")),
            ("extension-defined", json!(false)),
        ] {
            assert!(schema_type_matches(expected_type, &value));
            assert!(!json_type_name(&value).is_empty());
            assert!(!stable_json_signature(&value).is_empty());
        }
        assert_eq!(child_path("$.nested", "field"), "$.nested.field");

        let helper_call = call("metadata-helper", "schema_helper", "src/helper.rs");
        let mut failure = failure_from_error(
            &helper_call,
            ToolExecutionError::retryable("metadata_failed", "metadata failed"),
            json!({}),
            json!("scalar post-hook"),
            Duration::ZERO,
        );
        failure.rollback_error = Some(ToolExecutionError::retryable(
            "rollback_failed",
            "rollback failed",
        ));
        let mut outcome = ToolDispatchOutcome::Failed(failure);
        attach_mutation_boundary_metadata(
            &mut outcome,
            &helper_call,
            Some(&["src/helper.rs".into()]),
            MutationWorkerPhase::Rollback,
            &["fixture_event".into()],
            false,
        );
        let failure = outcome.failure().expect("metadata helper failure");
        assert_eq!(
            failure.post_hook_payload["mutationBoundary"]["rollbackState"],
            "failed"
        );
        assert_eq!(
            failure.post_hook_payload["handlerPostHook"],
            "scalar post-hook"
        );
    }

    #[cfg(unix)]
    #[test]
    fn parent_process_mutation_bookkeeping_contains_checkpoint_and_rollback_failures() {
        #[derive(Debug)]
        struct ParentCheckpointFailure {
            panics: bool,
        }

        impl ToolRollback for ParentCheckpointFailure {
            fn requires_parent_process(&self) -> bool {
                true
            }

            fn checkpoint_before(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
            ) -> ToolRegistryResult<Option<JsonValue>> {
                if self.panics {
                    panic!("parent checkpoint fixture panic");
                }
                Err(ToolExecutionError::retryable(
                    "parent_checkpoint_failed",
                    "parent checkpoint fixture failed",
                ))
            }

            fn rollback_after_failure(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                _checkpoint: &JsonValue,
                _error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                unreachable!("failed checkpoints must never reach rollback")
            }
        }

        #[derive(Debug)]
        struct ParentRollbackFailure {
            panics: bool,
        }

        impl ToolRollback for ParentRollbackFailure {
            fn requires_parent_process(&self) -> bool {
                true
            }

            fn checkpoint_before(
                &self,
                call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
            ) -> ToolRegistryResult<Option<JsonValue>> {
                Ok(Some(json!({ "checkpointFor": call.tool_call_id })))
            }

            fn rollback_after_failure(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                _checkpoint: &JsonValue,
                _error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                if self.panics {
                    panic!("parent rollback fixture panic");
                }
                Err(ToolExecutionError::retryable(
                    "parent_rollback_failed",
                    "parent rollback fixture failed",
                ))
            }
        }

        for (index, panics, expected_code) in [
            (1, false, "parent_checkpoint_failed"),
            (2, true, "agent_tool_checkpoint_panicked"),
        ] {
            let executed = Arc::new(AtomicBool::new(false));
            let executed_for_handler = Arc::clone(&executed);
            let mut registry = isolated_tool_registry();
            registry
                .register(StaticToolHandler::new(
                    descriptor("parent_checkpoint", ToolMutability::Mutating),
                    move |_context, _call| {
                        executed_for_handler.store(true, Ordering::SeqCst);
                        Ok(ToolHandlerOutput::new("must not execute", json!({})))
                    },
                ))
                .expect("register parent checkpoint fixture");
            let config = ToolDispatchConfig {
                rollback: Some(Arc::new(ParentCheckpointFailure { panics })),
                ..ToolDispatchConfig::default()
            };

            let outcome = registry.dispatch_call(
                call(
                    &format!("parent-checkpoint-{index}"),
                    "parent_checkpoint",
                    "src/checkpoint.rs",
                ),
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
            );

            assert_eq!(
                outcome
                    .failure()
                    .expect("checkpoint failure must fail dispatch")
                    .error
                    .code,
                expected_code
            );
            assert!(!executed.load(Ordering::SeqCst));
        }

        for (index, panics, expected_code) in [
            (1, false, "parent_rollback_failed"),
            (2, true, "agent_tool_rollback_panicked"),
        ] {
            let mut registry = isolated_tool_registry();
            registry
                .register(StaticToolHandler::new(
                    descriptor("parent_rollback", ToolMutability::Mutating),
                    |_context, _call| {
                        Err(ToolExecutionError::retryable(
                            "parent_handler_failed",
                            "parent handler fixture failed",
                        ))
                    },
                ))
                .expect("register parent rollback fixture");
            let config = ToolDispatchConfig {
                rollback: Some(Arc::new(ParentRollbackFailure { panics })),
                ..ToolDispatchConfig::default()
            };

            let outcome = registry.dispatch_call(
                call(
                    &format!("parent-rollback-{index}"),
                    "parent_rollback",
                    "src/rollback.rs",
                ),
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
            );

            let failure = outcome.failure().expect("rollback failure must fail dispatch");
            assert_eq!(failure.error.code, "parent_handler_failed");
            assert_eq!(
                failure
                    .rollback_error
                    .as_ref()
                    .expect("rollback error must be preserved")
                    .code,
                expected_code
            );
            assert_eq!(
                failure.post_hook_payload["mutationBoundary"]["rollbackState"],
                "unresolved"
            );
            assert_eq!(
                failure.post_hook_payload["mutationBoundary"]["quarantined"],
                true
            );
        }
    }

    #[test]
    fn registry_supervision_budget_and_cancellation_edges_fail_closed() {
        struct MutableDescriptorHandler {
            descriptor: ToolDescriptorV2,
            invalid: Arc<AtomicBool>,
        }

        impl ToolHandler for MutableDescriptorHandler {
            fn descriptor(&self) -> ToolDescriptorV2 {
                let mut descriptor = self.descriptor.clone();
                if self.invalid.load(Ordering::SeqCst) {
                    descriptor.description.clear();
                }
                descriptor
            }

            fn execute(
                &self,
                _context: &ToolExecutionContext,
                _call: &ToolCallInput,
            ) -> ToolRegistryResult<ToolHandlerOutput> {
                Ok(ToolHandlerOutput::new("must not execute", json!({})))
            }
        }

        #[derive(Debug)]
        struct CancellingCheckpoint {
            cancellation: ToolCancellationToken,
        }

        impl ToolRollback for CancellingCheckpoint {
            fn checkpoint_before(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
            ) -> ToolRegistryResult<Option<JsonValue>> {
                self.cancellation.cancel();
                Ok(Some(json!({ "checkpoint": true })))
            }

            fn rollback_after_failure(
                &self,
                _call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                _checkpoint: &JsonValue,
                _error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                Ok(json!({ "rolledBack": true }))
            }
        }

        let invalid = Arc::new(AtomicBool::new(false));
        let dynamic_handler = Arc::new(MutableDescriptorHandler {
            descriptor: descriptor("dynamic_descriptor", ToolMutability::ReadOnly),
            invalid: Arc::clone(&invalid),
        });
        let mut dynamic_registry = isolated_tool_registry();
        dynamic_registry
            .register_arc(dynamic_handler)
            .expect("register initially valid dynamic descriptor");
        invalid.store(true, Ordering::SeqCst);
        let config = ToolDispatchConfig::default();
        let outcome = dynamic_registry.dispatch_call(
            call(
                "dynamic-descriptor-call",
                "dynamic_descriptor",
                "src/dynamic.rs",
            ),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );
        assert_eq!(
            outcome
                .failure()
                .expect("descriptor must be revalidated before dispatch")
                .error
                .code,
            "agent_tool_descriptor_invalid"
        );

        let mutation_executed = Arc::new(AtomicBool::new(false));
        let mutation_executed_for_handler = Arc::clone(&mutation_executed);
        let mut cancellation_registry = isolated_tool_registry();
        cancellation_registry
            .register(StaticToolHandler::new(
                descriptor("cancel_after_checkpoint", ToolMutability::Mutating),
                move |_context, _call| {
                    mutation_executed_for_handler.store(true, Ordering::SeqCst);
                    Ok(ToolHandlerOutput::new("must not execute", json!({})))
                },
            ))
            .expect("register checkpoint cancellation fixture");
        let cancellation = ToolCancellationToken::new();
        let cancellation_call = call(
            "cancel-after-checkpoint-call",
            "cancel_after_checkpoint",
            "src/cancel.rs",
        );
        let prepared = cancellation_registry
            .prepare_call(
                &cancellation_call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                cancellation.clone(),
            )
            .expect("prepare checkpoint cancellation fixture");
        let outcome = execute_prepared_call(
            prepared,
            &config.context,
            Some(&CancellingCheckpoint { cancellation }),
        );
        assert_eq!(
            outcome
                .failure()
                .expect("checkpoint cancellation must fail")
                .error
                .code,
            "agent_tool_group_timeout"
        );
        assert!(!mutation_executed.load(Ordering::SeqCst));

        let mut late_registry = isolated_tool_registry();
        late_registry
            .register(StaticToolHandler::new_cancellable(
                descriptor("cancel_during_handler", ToolMutability::ReadOnly),
                |_context, _call, control| {
                    control.cancellation_token.cancel();
                    Ok(ToolHandlerOutput::new("late success", json!({})))
                },
            ))
            .expect("register late cancellation fixture");
        let late_call = call(
            "cancel-during-handler-call",
            "cancel_during_handler",
            "src/late.rs",
        );
        let prepared = late_registry
            .prepare_call(
                &late_call,
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
                Instant::now() + Duration::from_secs(1),
                ToolCancellationToken::new(),
            )
            .expect("prepare late cancellation fixture");
        assert_eq!(
            execute_prepared_call(prepared, &config.context, None)
                .failure()
                .expect("late success must be rejected")
                .error
                .code,
            "agent_tool_group_timeout"
        );

        let rollback_error = RecordingRollback
            .recover_after_termination(
                &cancellation_call,
                &descriptor("cancel_after_checkpoint", ToolMutability::Mutating),
                None,
                &ToolExecutionError::timeout("terminated", "terminated"),
            )
            .expect_err("default recovery requires a checkpoint");
        assert_eq!(
            rollback_error.code,
            "agent_tool_mutation_checkpoint_unavailable"
        );

        let supervisor = Arc::new(ReadOnlyWorkerSupervisor::new(1));
        let handle = thread::spawn(|| {});
        supervisor
            .state
            .lock()
            .expect("supervisor state")
            .workers
            .insert(7, Some(handle));
        supervisor.mark_completed(7);
        supervisor.join(7);
        supervisor.mark_completed(999);
        drop(ReadOnlyWorkerCompletionNotifier {
            worker_id: 999,
            supervisor: Weak::new(),
        });
        assert!(supervisor
            .state
            .lock()
            .expect("supervisor state after join")
            .workers
            .is_empty());

        let mut batch_registry = ToolRegistryV2::with_read_only_worker_limit(2);
        batch_registry
            .register(StaticToolHandler::new(
                descriptor("batch_failure", ToolMutability::ReadOnly),
                |_context, _call| {
                    Err(ToolExecutionError::retryable(
                        "batch_fixture_failed",
                        "batch fixture failed",
                    ))
                },
            ))
            .expect("register batch failure fixture");
        batch_registry
            .register(StaticToolHandler::new(
                descriptor("batch_timeout", ToolMutability::ReadOnly),
                |_context, _call| {
                    thread::sleep(Duration::from_millis(60));
                    Ok(ToolHandlerOutput::new("late", json!({})))
                },
            ))
            .expect("register batch timeout fixture");
        let batch_config = ToolDispatchConfig {
            budget: ToolBudget {
                max_tool_failures_per_turn: 0,
                max_wall_clock_time_per_tool_group_ms: 10,
                ..ToolBudget::default()
            },
            ..ToolDispatchConfig::default()
        };
        let report = batch_registry.dispatch_batch(
            &[
                ToolCallInput {
                    tool_call_id: "batch-invalid".into(),
                    tool_name: "batch_failure".into(),
                    input: json!({}),
                },
                call("batch-failure", "batch_failure", "src/failure.rs"),
                call("batch-timeout", "batch_timeout", "src/timeout.rs"),
            ],
            &batch_config,
        );
        let outcomes = &report.groups[0].outcomes;
        assert_eq!(outcomes.len(), 3);
        assert!(outcomes.iter().all(|outcome| {
            outcome
                .failure()
                .is_some_and(|failure| failure.error.code == "agent_tool_budget_failures_exceeded")
        }));
    }

    #[test]
    fn mutating_tool_failure_invokes_rollback_checkpoint() {
        let rollback = Arc::new(RecordingRollback);
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("patch", ToolMutability::Mutating),
                |_context, _call| {
                    Err(ToolExecutionError::retryable(
                        "patch_failed",
                        "Patch failed after checkpoint.",
                    ))
                },
            ))
            .expect("register patch tool");
        let config = ToolDispatchConfig {
            rollback: Some(rollback.clone()),
            ..ToolDispatchConfig::default()
        };
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome =
            registry.dispatch_call(call("call-1", "patch", "src/lib.rs"), &mut tracker, &config);

        assert_eq!(
            outcome.failure().unwrap().rollback_payload,
            Some(json!({
                "rolledBack": "call-1",
                "checkpoint": { "checkpointFor": "call-1" }
            }))
        );
    }

    #[test]
    fn mutating_tool_success_after_deadline_is_rejected_and_rolled_back() {
        let rollback = Arc::new(RecordingRollback);
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new_cancellable(
                descriptor("patch", ToolMutability::Mutating),
                |_context, _call, control| {
                    while !control.is_cancelled() {
                        thread::yield_now();
                    }
                    Ok(ToolHandlerOutput::new("late write", json!({ "ok": true })))
                },
            ))
            .expect("register patch tool");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 10,
                ..ToolBudget::default()
            },
            rollback: Some(rollback.clone()),
            ..ToolDispatchConfig::default()
        };
        let mut tracker = ToolBudgetTracker::new(config.budget.clone());

        let outcome =
            registry.dispatch_call(call("call-1", "patch", "src/lib.rs"), &mut tracker, &config);

        let failure = outcome.failure().expect("late success must fail");
        assert_eq!(failure.error.category, ToolErrorCategory::Timeout);
        assert_eq!(failure.error.code, "agent_tool_group_timeout");
        assert_eq!(
            failure.rollback_payload,
            Some(json!({
                "rolledBack": "call-1",
                "checkpoint": { "checkpointFor": "call-1" }
            }))
        );
    }

    #[test]
    fn mutating_tool_handler_panic_is_contained_and_rolled_back() {
        let rollback = Arc::new(RecordingRollback);
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("patch", ToolMutability::Mutating),
                |_context, _call| -> ToolRegistryResult<ToolHandlerOutput> {
                    panic!("handler panic must not escape dispatch")
                },
            ))
            .expect("register patch tool");
        let config = ToolDispatchConfig {
            rollback: Some(rollback.clone()),
            ..ToolDispatchConfig::default()
        };
        let mut tracker = ToolBudgetTracker::new(config.budget.clone());

        let outcome =
            registry.dispatch_call(call("call-1", "patch", "src/lib.rs"), &mut tracker, &config);

        let failure = outcome.failure().expect("panicking handler must fail");
        assert_eq!(failure.error.code, "agent_tool_handler_panicked");
        assert_eq!(
            failure.rollback_payload,
            Some(json!({
                "rolledBack": "call-1",
                "checkpoint": { "checkpointFor": "call-1" }
            }))
        );
    }

    #[test]
    fn non_cooperative_mutating_handler_is_terminated_within_group_budget() {
        let release = Arc::new(AtomicBool::new(false));
        let release_for_handler = Arc::clone(&release);
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("hung_patch", ToolMutability::Mutating),
                move |_context, _call| {
                    while !release_for_handler.load(Ordering::SeqCst) {
                        thread::park_timeout(Duration::from_millis(1));
                    }
                    Ok(ToolHandlerOutput::new("released", json!({ "ok": true })))
                },
            ))
            .expect("register hung patch");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 20,
                ..ToolBudget::default()
            },
            ..ToolDispatchConfig::default()
        };
        let (result_tx, result_rx) = mpsc::channel();

        let dispatch = thread::spawn(move || {
            let outcome = registry.dispatch_call(
                call("call-hung", "hung_patch", "src/lib.rs"),
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
            );
            let _ = result_tx.send(outcome);
        });
        let outcome = result_rx.recv_timeout(Duration::from_millis(100));
        release.store(true, Ordering::SeqCst);
        dispatch.join().expect("join dispatch");

        let failure = outcome
            .expect("mutation boundary must terminate a non-cooperative handler")
            .failure()
            .cloned()
            .expect("terminated mutation must fail");
        assert_eq!(failure.error.code, "agent_tool_mutation_terminated");
        assert_eq!(failure.error.category, ToolErrorCategory::Timeout);
    }

    #[derive(Debug)]
    struct SlowAllowPolicy {
        delay: Duration,
    }

    impl ToolPolicy for SlowAllowPolicy {
        fn evaluate(
            &self,
            _descriptor: &ToolDescriptorV2,
            _call: &ToolCallInput,
        ) -> ToolPolicyDecision {
            thread::sleep(self.delay);
            ToolPolicyDecision::Allow
        }
    }

    #[test]
    fn mutation_boundary_preserves_deadline_spent_during_preparation() {
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("hung_after_preparation", ToolMutability::Mutating),
                |_context, _call| loop {
                    thread::park_timeout(Duration::from_millis(1));
                },
            ))
            .expect("register mutation");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 200,
                ..ToolBudget::default()
            },
            policy: Arc::new(SlowAllowPolicy {
                delay: Duration::from_millis(170),
            }),
            ..ToolDispatchConfig::default()
        };
        let started = Instant::now();

        let outcome = registry.dispatch_call(
            call(
                "call-slow-preparation",
                "hung_after_preparation",
                "src/lib.rs",
            ),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );

        let failure = outcome.failure().expect("hung mutation must time out");
        assert_eq!(failure.error.code, "agent_tool_mutation_terminated");
        assert!(
            started.elapsed() < Duration::from_millis(300),
            "mutation supervision restarted the group deadline after preparation"
        );
    }

    #[derive(Debug)]
    struct PanickingCheckpoint;

    impl ToolRollback for PanickingCheckpoint {
        fn checkpoint_before(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            panic!("checkpoint panic must not escape dispatch")
        }

        fn rollback_after_failure(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            _checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            Ok(json!({ "unexpected": true }))
        }
    }

    #[test]
    fn mutating_checkpoint_panic_is_a_typed_failure() {
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("patch", ToolMutability::Mutating),
                |_context, _call| Ok(ToolHandlerOutput::new("patched", json!({}))),
            ))
            .expect("register patch");
        let config = ToolDispatchConfig {
            rollback: Some(Arc::new(PanickingCheckpoint)),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call("call-checkpoint-panic", "patch", "src/lib.rs"),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );

        let failure = outcome.failure().expect("checkpoint panic must fail");
        assert_eq!(failure.error.code, "agent_tool_checkpoint_panicked");
    }

    struct PanickingPostHook {
        descriptor: ToolDescriptorV2,
    }

    impl ToolHandler for PanickingPostHook {
        fn descriptor(&self) -> ToolDescriptorV2 {
            self.descriptor.clone()
        }

        fn execute(
            &self,
            _context: &ToolExecutionContext,
            _call: &ToolCallInput,
        ) -> ToolRegistryResult<ToolHandlerOutput> {
            Ok(ToolHandlerOutput::new("patched", json!({ "ok": true })))
        }

        fn post_hook_payload(
            &self,
            _call: &ToolCallInput,
            _result: &Result<ToolHandlerOutput, ToolExecutionError>,
        ) -> JsonValue {
            panic!("post-hook panic must not be treated as success")
        }
    }

    #[test]
    fn mutating_post_hook_panic_is_a_typed_failure_and_rolls_back() {
        let rollback = Arc::new(RecordingRollback);
        let mut registry = isolated_tool_registry();
        registry
            .register(PanickingPostHook {
                descriptor: descriptor("patch", ToolMutability::Mutating),
            })
            .expect("register patch");
        let config = ToolDispatchConfig {
            rollback: Some(rollback.clone()),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call("call-post-hook-panic", "patch", "src/lib.rs"),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );

        let failure = outcome.failure().expect("post-hook panic must fail");
        assert_eq!(failure.error.code, "agent_tool_post_hook_panicked");
        assert!(failure.rollback_payload.is_some());
    }

    #[derive(Debug)]
    struct PanickingRollback;

    impl ToolRollback for PanickingRollback {
        fn checkpoint_before(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            Ok(Some(json!({ "checkpoint": true })))
        }

        fn rollback_after_failure(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            _checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            panic!("rollback panic must not escape dispatch")
        }
    }

    #[test]
    fn mutating_rollback_panic_is_a_typed_failure() {
        let mut registry = isolated_tool_registry();
        registry
            .register(StaticToolHandler::new(
                descriptor("patch", ToolMutability::Mutating),
                |_context, _call| {
                    Err(ToolExecutionError::retryable(
                        "patch_failed",
                        "patch failed after checkpoint",
                    ))
                },
            ))
            .expect("register patch");
        let config = ToolDispatchConfig {
            rollback: Some(Arc::new(PanickingRollback)),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call("call-rollback-panic", "patch", "src/lib.rs"),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );

        let failure = outcome.failure().expect("rollback panic must fail");
        assert_eq!(
            failure
                .rollback_error
                .as_ref()
                .expect("typed rollback failure")
                .code,
            "agent_tool_rollback_panicked"
        );
    }

    #[derive(Debug)]
    struct HangingCheckpoint {
        recovery_marker: std::path::PathBuf,
    }

    impl ToolRollback for HangingCheckpoint {
        fn checkpoint_before(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            loop {
                thread::park_timeout(Duration::from_millis(1));
            }
        }

        fn rollback_after_failure(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            _checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            Ok(json!({ "unexpected": true }))
        }

        fn recover_after_termination(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            _checkpoint: Option<&JsonValue>,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            if self.recovery_marker.exists() {
                Ok(json!({ "recovered": call.tool_call_id }))
            } else {
                Err(ToolExecutionError::retryable(
                    "checkpoint_recovery_not_ready",
                    "Checkpoint recovery is not ready yet.",
                ))
            }
        }
    }

    #[derive(Debug)]
    struct RecoverWithoutCheckpointRollback;

    impl ToolRollback for RecoverWithoutCheckpointRollback {
        fn checkpoint_before(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            Ok(Some(json!({ "checkpointFor": call.tool_call_id })))
        }

        fn rollback_after_failure(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            Ok(json!({ "rolledBack": call.tool_call_id, "checkpoint": checkpoint }))
        }

        fn recover_after_termination(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            _checkpoint: Option<&JsonValue>,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            Ok(json!({ "recovered": call.tool_call_id }))
        }
    }

    #[cfg(unix)]
    #[test]
    fn checkpoint_hang_quarantines_mutations_until_recovery_succeeds() {
        let recovery_marker = std::env::temp_dir().join(format!(
            "xero-checkpoint-recovery-ready-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&recovery_marker);
        let hanging_checkpoint = Arc::new(HangingCheckpoint {
            recovery_marker: recovery_marker.clone(),
        });
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(StaticToolHandler::new(
                descriptor("first_mutation", ToolMutability::Mutating),
                |_context, _call| Ok(ToolHandlerOutput::new("first", json!({}))),
            ))
            .expect("register first mutation");
        registry
            .register(StaticToolHandler::new(
                descriptor("later_mutation", ToolMutability::Mutating),
                |_context, _call| Ok(ToolHandlerOutput::new("later", json!({}))),
            ))
            .expect("register later mutation");
        let budget = ToolBudget {
            max_wall_clock_time_per_tool_group_ms: 20,
            max_mutation_cleanup_ms: 20,
            ..ToolBudget::default()
        };
        let first_config = ToolDispatchConfig {
            budget: budget.clone(),
            rollback: Some(hanging_checkpoint.clone()),
            ..ToolDispatchConfig::default()
        };

        let first = registry.dispatch_call(
            call("call-checkpoint-hang", "first_mutation", "src/first.rs"),
            &mut ToolBudgetTracker::new(budget.clone()),
            &first_config,
        );
        let blocked = registry.dispatch_call(
            call("call-blocked", "later_mutation", "src/later.rs"),
            &mut ToolBudgetTracker::new(budget.clone()),
            &ToolDispatchConfig {
                budget: budget.clone(),
                rollback: Some(Arc::new(RecordingRollback)),
                ..ToolDispatchConfig::default()
            },
        );
        std::fs::write(&recovery_marker, "ready").expect("mark checkpoint recovery ready");
        let recovered = registry.dispatch_call(
            call("call-recovered", "later_mutation", "src/later.rs"),
            &mut ToolBudgetTracker::new(budget.clone()),
            &ToolDispatchConfig {
                budget,
                rollback: Some(Arc::new(RecoverWithoutCheckpointRollback)),
                ..ToolDispatchConfig::default()
            },
        );
        let _ = std::fs::remove_file(recovery_marker);

        assert_eq!(
            first.failure().expect("checkpoint hang failure").error.code,
            "agent_tool_mutation_terminated"
        );
        assert_eq!(
            blocked.failure().expect("quarantine failure").error.code,
            "agent_tool_mutation_quarantined"
        );
        assert!(recovered.is_success());
    }

    #[derive(Debug)]
    struct HangingRollbackUntilMarker {
        recovery_marker: std::path::PathBuf,
    }

    impl ToolRollback for HangingRollbackUntilMarker {
        fn checkpoint_before(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            Ok(Some(json!({ "checkpointFor": call.tool_call_id })))
        }

        fn rollback_after_failure(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            _checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            if self.recovery_marker.exists() {
                return Ok(json!({ "recovered": true }));
            }
            loop {
                thread::park_timeout(Duration::from_millis(1));
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn rollback_hang_quarantines_only_later_mutations_in_the_same_scope() {
        let recovery_marker = std::env::temp_dir().join(format!(
            "xero-rollback-recovery-ready-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&recovery_marker);
        let hanging_rollback = Arc::new(HangingRollbackUntilMarker {
            recovery_marker: recovery_marker.clone(),
        });
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        for tool_name in ["failing_mutation", "later_mutation"] {
            registry
                .register(StaticToolHandler::new(
                    descriptor(tool_name, ToolMutability::Mutating),
                    move |_context, _call| {
                        if tool_name == "failing_mutation" {
                            Err(ToolExecutionError::retryable(
                                "mutation_failed",
                                "mutation failed after checkpoint",
                            ))
                        } else {
                            Ok(ToolHandlerOutput::new("later", json!({})))
                        }
                    },
                ))
                .expect("register mutation");
        }
        let budget = ToolBudget {
            max_wall_clock_time_per_tool_group_ms: 20,
            max_mutation_cleanup_ms: 20,
            ..ToolBudget::default()
        };
        let mutation_context = |scope: &str| ToolExecutionContext {
            project_id: "shared-project".into(),
            telemetry_attributes: BTreeMap::from([(
                MUTATION_EXECUTION_SCOPE_ATTRIBUTE.into(),
                scope.into(),
            )]),
            ..ToolExecutionContext::default()
        };
        let hanging_config = ToolDispatchConfig {
            budget: budget.clone(),
            rollback: Some(hanging_rollback),
            context: mutation_context("workspace-a"),
            ..ToolDispatchConfig::default()
        };

        let failed = registry.dispatch_call(
            call("call-rollback-hang", "failing_mutation", "src/first.rs"),
            &mut ToolBudgetTracker::new(budget.clone()),
            &hanging_config,
        );
        let blocked = registry.dispatch_call(
            call("call-blocked", "later_mutation", "src/later.rs"),
            &mut ToolBudgetTracker::new(budget.clone()),
            &hanging_config,
        );
        let other_scope = registry.dispatch_call(
            call("call-other-scope", "later_mutation", "src/later.rs"),
            &mut ToolBudgetTracker::new(budget.clone()),
            &ToolDispatchConfig {
                context: mutation_context("workspace-b"),
                ..hanging_config.clone()
            },
        );
        std::fs::write(&recovery_marker, "ready").expect("mark rollback recovery ready");
        let recovered = registry.dispatch_call(
            call("call-recovered", "later_mutation", "src/later.rs"),
            &mut ToolBudgetTracker::new(budget.clone()),
            &ToolDispatchConfig {
                budget,
                rollback: Some(Arc::new(RecoverWithoutCheckpointRollback)),
                context: mutation_context("workspace-a"),
                ..ToolDispatchConfig::default()
            },
        );
        let _ = std::fs::remove_file(recovery_marker);

        assert_eq!(
            failed
                .failure()
                .and_then(|failure| failure.rollback_error.as_ref())
                .expect("terminated rollback error")
                .code,
            "agent_tool_rollback_terminated"
        );
        assert_eq!(
            blocked.failure().expect("quarantine failure").error.code,
            "agent_tool_mutation_quarantined"
        );
        assert!(other_scope.is_success());
        assert!(recovered.is_success());
    }

    struct HangingPostHook {
        descriptor: ToolDescriptorV2,
    }

    impl ToolHandler for HangingPostHook {
        fn descriptor(&self) -> ToolDescriptorV2 {
            self.descriptor.clone()
        }

        fn execute(
            &self,
            _context: &ToolExecutionContext,
            _call: &ToolCallInput,
        ) -> ToolRegistryResult<ToolHandlerOutput> {
            Ok(ToolHandlerOutput::new("mutated", json!({ "ok": true })))
        }

        fn post_hook_payload(
            &self,
            _call: &ToolCallInput,
            _result: &Result<ToolHandlerOutput, ToolExecutionError>,
        ) -> JsonValue {
            loop {
                thread::park_timeout(Duration::from_millis(1));
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn post_hook_hang_is_terminated_and_recovered() {
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(HangingPostHook {
                descriptor: descriptor("post_hook_hang", ToolMutability::Mutating),
            })
            .expect("register post-hook hang");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 20,
                max_mutation_cleanup_ms: 50,
                ..ToolBudget::default()
            },
            rollback: Some(Arc::new(RecordingRollback)),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call("call-post-hook-hang", "post_hook_hang", "src/lib.rs"),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );

        let failure = outcome.failure().expect("post-hook hang failure");
        assert_eq!(failure.error.code, "agent_tool_mutation_terminated");
        assert_eq!(
            failure.rollback_payload,
            Some(json!({
                "rolledBack": "call-post-hook-hang",
                "checkpoint": { "checkpointFor": "call-post-hook-hang" }
            }))
        );
    }

    #[cfg(unix)]
    #[test]
    fn terminated_mutation_cannot_perform_a_late_thread_write() {
        let path = std::env::temp_dir().join(format!(
            "xero-mutation-late-thread-write-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let path_for_handler = path.clone();
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(StaticToolHandler::new(
                descriptor("late_write", ToolMutability::Mutating),
                move |_context, _call| {
                    let path = path_for_handler.clone();
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(120));
                        let _ = std::fs::write(path, "late");
                    });
                    loop {
                        thread::park_timeout(Duration::from_millis(1));
                    }
                },
            ))
            .expect("register late write");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 20,
                max_mutation_cleanup_ms: 50,
                ..ToolBudget::default()
            },
            rollback: Some(Arc::new(RecoverWithoutCheckpointRollback)),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call("call-late-write", "late_write", "src/lib.rs"),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );
        thread::sleep(Duration::from_millis(180));

        let failure = outcome.failure().expect("terminated late write");
        assert_eq!(failure.error.code, "agent_tool_mutation_terminated");
        assert_eq!(
            failure.post_hook_payload["mutationBoundary"]["exitClassification"],
            json!("timeout")
        );
        assert_eq!(
            failure.post_hook_payload["mutationBoundary"]["rollbackState"],
            json!("succeeded")
        );
        assert_eq!(
            failure.post_hook_payload["mutationBoundary"]["affectedPaths"],
            json!(["src/lib.rs"])
        );
        assert!(failure.post_hook_payload["mutationBoundary"]["auditEvents"]
            .as_array()
            .is_some_and(|events| events.contains(&json!("mutation_worker_terminated"))));
        assert!(!path.exists(), "terminated worker must not write later");
    }

    #[cfg(unix)]
    #[test]
    fn terminated_mutation_kills_spawned_process_descendants() {
        let path = std::env::temp_dir().join(format!(
            "xero-mutation-descendant-write-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        let path_for_handler = path.clone();
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(StaticToolHandler::new(
                descriptor("descendant_write", ToolMutability::Mutating),
                move |_context, _call| {
                    let status = std::process::Command::new("sh")
                        .arg("-c")
                        .arg("(sleep 0.12; printf late > \"$1\") & wait")
                        .arg("xero-mutation-test")
                        .arg(&path_for_handler)
                        .status()
                        .map_err(|error| {
                            ToolExecutionError::retryable(
                                "descendant_spawn_failed",
                                error.to_string(),
                            )
                        })?;
                    Ok(ToolHandlerOutput::new(
                        "descendant exited",
                        json!({ "success": status.success() }),
                    ))
                },
            ))
            .expect("register descendant write");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 20,
                max_mutation_cleanup_ms: 50,
                ..ToolBudget::default()
            },
            rollback: Some(Arc::new(RecoverWithoutCheckpointRollback)),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call("call-descendant-write", "descendant_write", "src/lib.rs"),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );
        thread::sleep(Duration::from_millis(180));

        assert_eq!(
            outcome
                .failure()
                .expect("terminated descendant write")
                .error
                .code,
            "agent_tool_mutation_terminated"
        );
        assert!(
            !path.exists(),
            "terminated process descendant must not write later"
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_cancellation_terminates_non_cooperative_mutation_promptly() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_config = Arc::clone(&cancelled);
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(StaticToolHandler::new(
                descriptor("cancelled_mutation", ToolMutability::Mutating),
                |_context, _call| loop {
                    thread::park_timeout(Duration::from_millis(1));
                },
            ))
            .expect("register cancelled mutation");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 5_000,
                max_mutation_cleanup_ms: 50,
                ..ToolBudget::default()
            },
            rollback: Some(Arc::new(RecoverWithoutCheckpointRollback)),
            cancellation_check: Some(Arc::new(move || {
                cancelled_for_config.load(Ordering::SeqCst)
            })),
            ..ToolDispatchConfig::default()
        };
        let (result_tx, result_rx) = mpsc::channel();
        let started = Instant::now();
        let dispatch = thread::spawn(move || {
            let outcome = registry.dispatch_call(
                call(
                    "call-cancelled-mutation",
                    "cancelled_mutation",
                    "src/lib.rs",
                ),
                &mut ToolBudgetTracker::new(config.budget.clone()),
                &config,
            );
            let _ = result_tx.send(outcome);
        });
        thread::sleep(Duration::from_millis(20));
        cancelled.store(true, Ordering::SeqCst);
        let outcome = result_rx
            .recv_timeout(Duration::from_millis(200))
            .expect("cancellation must terminate mutation promptly");
        dispatch.join().expect("join cancelled mutation");

        assert!(started.elapsed() < Duration::from_millis(200));
        assert_eq!(
            outcome
                .failure()
                .expect("cancelled mutation failure")
                .error
                .code,
            "agent_tool_mutation_cancelled"
        );
    }

    #[cfg(unix)]
    #[test]
    fn mutation_worker_crash_is_typed_and_recovered() {
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(StaticToolHandler::new(
                descriptor("crashing_mutation", ToolMutability::Mutating),
                |_context, _call| {
                    // SAFETY: this handler runs only in the isolated mutation test worker and
                    // intentionally exits it to exercise crash classification and recovery.
                    unsafe { libc::_exit(42) }
                },
            ))
            .expect("register crashing mutation");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 1_000,
                max_mutation_cleanup_ms: 50,
                ..ToolBudget::default()
            },
            rollback: Some(Arc::new(RecoverWithoutCheckpointRollback)),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call("call-crashing-mutation", "crashing_mutation", "src/lib.rs"),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );

        let failure = outcome.failure().expect("worker crash failure");
        assert_eq!(failure.error.code, "agent_tool_mutation_worker_crashed");
        assert_eq!(
            failure.post_hook_payload["mutationBoundary"]["exitClassification"],
            json!("crashed")
        );
        assert!(failure.rollback_payload.is_some());
    }

    #[derive(Debug)]
    struct FailingCheckpoint;

    impl ToolRollback for FailingCheckpoint {
        fn checkpoint_before(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            Err(ToolExecutionError::policy_denied(
                "checkpoint_denied",
                "checkpoint denied before handler execution",
            ))
        }

        fn rollback_after_failure(
            &self,
            _call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            _checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            Ok(json!({ "unexpected": true }))
        }
    }

    #[test]
    fn mutating_preflight_error_blocks_handler_execution() {
        let executed = Arc::new(Mutex::new(false));
        let executed_for_handler = Arc::clone(&executed);
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("write_file", ToolMutability::Mutating),
                move |_context, _call| {
                    *executed_for_handler.lock().unwrap() = true;
                    Ok(ToolHandlerOutput::new("write", json!({ "ok": true })))
                },
            ))
            .expect("register write");
        let config = ToolDispatchConfig {
            rollback: Some(Arc::new(FailingCheckpoint)),
            ..ToolDispatchConfig::default()
        };
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            call("call-1", "write_file", "src/lib.rs"),
            &mut tracker,
            &config,
        );

        let failure = outcome.failure().expect("preflight failure");
        assert_eq!(failure.error.category, ToolErrorCategory::PolicyDenied);
        assert_eq!(failure.error.code, "checkpoint_denied");
        assert!(!*executed.lock().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn process_local_mutation_bookkeeping_runs_in_the_supervising_parent() {
        #[derive(Debug)]
        struct ParentProcessRollback {
            checkpoint_pid: Arc<AtomicU64>,
            rollback_pid: Arc<AtomicU64>,
        }

        impl ToolRollback for ParentProcessRollback {
            fn requires_parent_process(&self) -> bool {
                true
            }

            fn checkpoint_before(
                &self,
                call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
            ) -> ToolRegistryResult<Option<JsonValue>> {
                self.checkpoint_pid
                    .store(u64::from(std::process::id()), Ordering::SeqCst);
                Ok(Some(json!({ "checkpointFor": call.tool_call_id })))
            }

            fn rollback_after_failure(
                &self,
                call: &ToolCallInput,
                _descriptor: &ToolDescriptorV2,
                checkpoint: &JsonValue,
                _error: &ToolExecutionError,
            ) -> ToolRegistryResult<JsonValue> {
                self.rollback_pid
                    .store(u64::from(std::process::id()), Ordering::SeqCst);
                Ok(json!({ "rolledBack": call.tool_call_id, "checkpoint": checkpoint }))
            }
        }

        let checkpoint_pid = Arc::new(AtomicU64::new(0));
        let rollback_pid = Arc::new(AtomicU64::new(0));
        let rollback = Arc::new(ParentProcessRollback {
            checkpoint_pid: Arc::clone(&checkpoint_pid),
            rollback_pid: Arc::clone(&rollback_pid),
        });
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(StaticToolHandler::new(
                descriptor("parent_bookkeeping", ToolMutability::Mutating),
                |_context, _call| {
                    Err(ToolExecutionError::retryable(
                        "expected_mutation_failure",
                        "exercise parent-side rollback",
                    ))
                },
            ))
            .expect("register parent-bookkeeping mutation");
        let config = ToolDispatchConfig {
            rollback: Some(rollback),
            ..ToolDispatchConfig::default()
        };

        let outcome = registry.dispatch_call(
            call(
                "call-parent-bookkeeping",
                "parent_bookkeeping",
                "src/lib.rs",
            ),
            &mut ToolBudgetTracker::new(config.budget.clone()),
            &config,
        );

        let parent_pid = u64::from(std::process::id());
        assert_eq!(checkpoint_pid.load(Ordering::SeqCst), parent_pid);
        assert_eq!(rollback_pid.load(Ordering::SeqCst), parent_pid);
        assert_eq!(
            outcome
                .failure()
                .and_then(|failure| failure.rollback_payload.as_ref()),
            Some(&json!({
                "rolledBack": "call-parent-bookkeeping",
                "checkpoint": { "checkpointFor": "call-parent-bookkeeping" }
            }))
        );
    }

    #[test]
    fn repeated_failing_tool_sets_doom_loop_signal() {
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("search", ToolMutability::ReadOnly),
                |_context, _call| {
                    Err(ToolExecutionError::retryable(
                        "search_failed",
                        "Search backend failed.",
                    ))
                },
            ))
            .expect("register search tool");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());
        let config = ToolDispatchConfig::default();

        let _ = registry.dispatch_call(call("call-1", "search", "src"), &mut tracker, &config);
        let outcome =
            registry.dispatch_call(call("call-2", "search", "src"), &mut tracker, &config);

        assert_eq!(
            outcome.failure().unwrap().doom_loop_signal,
            Some(ToolDoomLoopSignal::SameFailingToolRepeated)
        );
    }

    #[test]
    fn doom_loop_guard_detects_repeated_reads_pending_todos_and_verification_loops() {
        let mut guard = ToolDoomLoopGuard::default();
        let read = call("call-1", "read_file", "src/lib.rs");

        assert_eq!(guard.record_file_read(&read, "epoch-1"), None);
        assert_eq!(
            guard.record_file_read(&read, "epoch-1"),
            Some(ToolDoomLoopSignal::SameReadRepeatedWithoutNewContext)
        );
        assert_eq!(
            guard.record_completion_claim(1, true),
            Some(ToolDoomLoopSignal::PendingTodosIgnoredAfterCompletionClaim)
        );
        assert_eq!(
            guard.record_verification_request("cargo-test", "unchanged"),
            None
        );
        assert_eq!(
            guard.record_verification_request("cargo-test", "unchanged"),
            Some(ToolDoomLoopSignal::VerificationRepeatedWithoutChangedInputs)
        );
    }

    #[test]
    fn repeated_equivalent_calls_stop_at_budget() {
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("read_file", ToolMutability::ReadOnly),
                |_context, _call| Ok(ToolHandlerOutput::new("read", json!({ "ok": true }))),
            ))
            .expect("register read tool");
        let mut tracker = ToolBudgetTracker::new(ToolBudget {
            max_repeated_equivalent_calls: 1,
            ..ToolBudget::default()
        });
        let config = ToolDispatchConfig::default();

        let _ = registry.dispatch_call(
            call("call-1", "read_file", "src/lib.rs"),
            &mut tracker,
            &config,
        );
        let outcome = registry.dispatch_call(
            call("call-2", "read_file", "src/lib.rs"),
            &mut tracker,
            &config,
        );

        assert_eq!(
            outcome.failure().unwrap().error.category,
            ToolErrorCategory::BudgetExceeded
        );
    }

    #[test]
    fn read_only_tools_are_dispatched_in_parallel_groups() {
        let _guard = read_only_supervisor_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let active = Arc::new(Mutex::new(0usize));
        let max_active = Arc::new(Mutex::new(0usize));
        let mut registry = ToolRegistryV2::new();
        for name in ["read_a", "read_b"] {
            let active = Arc::clone(&active);
            let max_active = Arc::clone(&max_active);
            registry
                .register(StaticToolHandler::new(
                    descriptor(name, ToolMutability::ReadOnly),
                    move |_context, _call| {
                        {
                            let mut active = active.lock().unwrap();
                            *active += 1;
                            let mut max_active = max_active.lock().unwrap();
                            *max_active = (*max_active).max(*active);
                        }
                        thread::sleep(Duration::from_millis(40));
                        *active.lock().unwrap() -= 1;
                        Ok(ToolHandlerOutput::new("read", json!({ "ok": true })))
                    },
                ))
                .expect("register read tool");
        }

        let report = registry.dispatch_batch(
            &[call("call-1", "read_a", "a"), call("call-2", "read_b", "b")],
            &ToolDispatchConfig::default(),
        );

        assert_eq!(
            report.groups[0].mode,
            ToolGroupExecutionMode::ParallelReadOnly
        );
        assert_eq!(*max_active.lock().unwrap(), 2);
    }

    #[test]
    fn tool_group_timeout_interrupts_hung_read_only_handler() {
        let _guard = read_only_supervisor_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut registry = ToolRegistryV2::new();
        for name in ["slow_a", "slow_b"] {
            registry
                .register(StaticToolHandler::new(
                    descriptor(name, ToolMutability::ReadOnly),
                    |_context, _call| {
                        thread::sleep(Duration::from_millis(150));
                        Ok(ToolHandlerOutput::new("late", json!({ "ok": true })))
                    },
                ))
                .expect("register slow read tool");
        }
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 20,
                ..ToolBudget::default()
            },
            ..ToolDispatchConfig::default()
        };

        let started = Instant::now();
        let report = registry.dispatch_batch(
            &[call("call-1", "slow_a", "a"), call("call-2", "slow_b", "b")],
            &config,
        );

        assert!(
            started.elapsed() < Duration::from_millis(80),
            "read-only group budget must interrupt hung handlers instead of waiting for them"
        );
        assert!(report.groups[0].outcomes.iter().all(|outcome| {
            outcome
                .failure()
                .is_some_and(|failure| failure.error.category == ToolErrorCategory::Timeout)
        }));
    }

    #[test]
    fn cancellable_handler_observes_group_deadline_control() {
        let _guard = read_only_supervisor_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let observed_cancel = Arc::new(Mutex::new(false));
        let observed_cancel_for_handler = Arc::clone(&observed_cancel);
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new_cancellable(
                descriptor("slow_probe", ToolMutability::ReadOnly),
                move |_context, call, control| {
                    while !control.is_cancelled() {
                        thread::sleep(Duration::from_millis(5));
                    }
                    *observed_cancel_for_handler.lock().unwrap() = true;
                    Err(ToolExecutionError::timeout(
                        "slow_probe_cancelled",
                        format!("{} saw cancellation", call.tool_name),
                    ))
                },
            ))
            .expect("register cancellable probe");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 20,
                ..ToolBudget::default()
            },
            ..ToolDispatchConfig::default()
        };

        let report = registry.dispatch_batch(&[call("call-1", "slow_probe", "a")], &config);

        assert!(report.groups[0].outcomes[0]
            .failure()
            .is_some_and(|failure| failure.error.category == ToolErrorCategory::Timeout));
        for _ in 0..20 {
            if *observed_cancel.lock().unwrap() {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        assert!(*observed_cancel.lock().unwrap());
    }

    #[derive(Debug, Default)]
    struct CooperativeReadWave {
        started: usize,
        active: usize,
        max_active: usize,
        released: bool,
    }

    #[test]
    fn cooperative_read_only_work_queues_above_supervisor_capacity() {
        let _guard = read_only_supervisor_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let wave = Arc::new((Mutex::new(CooperativeReadWave::default()), Condvar::new()));
        let wave_for_handler = Arc::clone(&wave);
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("cooperative_read", ToolMutability::ReadOnly),
                move |_context, _call| {
                    let (lock, ready) = &*wave_for_handler;
                    let mut state = lock.lock().unwrap();
                    state.started += 1;
                    state.active += 1;
                    state.max_active = state.max_active.max(state.active);
                    ready.notify_all();
                    while !state.released {
                        state = ready.wait(state).unwrap();
                    }
                    state.active -= 1;
                    Ok(ToolHandlerOutput::new("read", json!({ "ok": true })))
                },
            ))
            .expect("register cooperative read");
        let wave_for_release = Arc::clone(&wave);
        let release = thread::spawn(move || {
            let (lock, ready) = &*wave_for_release;
            let state = lock.lock().unwrap();
            let (mut state, _) = ready
                .wait_timeout_while(state, Duration::from_millis(500), |state| {
                    state.started < DEFAULT_MAX_SUPERVISED_READ_ONLY_WORKERS
                })
                .unwrap();
            drop(state);
            thread::sleep(Duration::from_millis(20));
            state = lock.lock().unwrap();
            state.released = true;
            ready.notify_all();
        });
        let call_count = DEFAULT_MAX_SUPERVISED_READ_ONLY_WORKERS + 1;
        let calls = (0..call_count)
            .map(|index| {
                call(
                    &format!("call-{index}"),
                    "cooperative_read",
                    &format!("path-{index}"),
                )
            })
            .collect::<Vec<_>>();
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 1_500,
                ..ToolBudget::default()
            },
            ..ToolDispatchConfig::default()
        };

        let report = registry.dispatch_batch(&calls, &config);
        release.join().expect("release cooperative reads");

        assert_eq!(report.groups.len(), 1);
        assert_eq!(report.groups[0].outcomes.len(), call_count);
        assert!(report.groups[0]
            .outcomes
            .iter()
            .all(ToolDispatchOutcome::is_success));
        assert!(
            wave.as_ref().0.lock().unwrap().max_active <= DEFAULT_MAX_SUPERVISED_READ_ONLY_WORKERS
        );
    }

    #[test]
    fn non_cooperative_read_only_worker_is_retained_and_bounded() {
        let release = Arc::new(AtomicBool::new(false));
        let finished = Arc::new(AtomicBool::new(false));
        let release_for_handler = Arc::clone(&release);
        let finished_for_handler = Arc::clone(&finished);
        let mut registry = ToolRegistryV2::with_read_only_worker_limit(1);
        registry
            .register(StaticToolHandler::new(
                descriptor("stuck_read", ToolMutability::ReadOnly),
                move |_context, _call| {
                    while !release_for_handler.load(Ordering::SeqCst) {
                        thread::park_timeout(Duration::from_millis(1));
                    }
                    finished_for_handler.store(true, Ordering::SeqCst);
                    Ok(ToolHandlerOutput::new("released", json!({ "ok": true })))
                },
            ))
            .expect("register stuck read");
        registry
            .register(StaticToolHandler::new(
                descriptor("fast_read", ToolMutability::ReadOnly),
                |_context, _call| Ok(ToolHandlerOutput::new("fast", json!({ "ok": true }))),
            ))
            .expect("register fast read");
        let config = ToolDispatchConfig {
            budget: ToolBudget {
                max_wall_clock_time_per_tool_group_ms: 10,
                ..ToolBudget::default()
            },
            ..ToolDispatchConfig::default()
        };

        let first = registry.dispatch_batch(&[call("call-1", "stuck_read", "a")], &config);
        let second = registry.dispatch_batch(&[call("call-2", "fast_read", "b")], &config);
        release.store(true, Ordering::SeqCst);
        let cleanup_deadline = Instant::now() + Duration::from_secs(1);
        while !finished.load(Ordering::SeqCst) && Instant::now() < cleanup_deadline {
            thread::yield_now();
        }
        let third = registry.dispatch_batch(&[call("call-3", "fast_read", "c")], &config);

        assert_eq!(
            first.groups[0].outcomes[0].failure().unwrap().error.code,
            "agent_tool_group_timeout"
        );
        assert_eq!(
            second.groups[0].outcomes[0]
                .failure()
                .expect("supervisor must reject an unbounded replacement worker")
                .error
                .code,
            "agent_tool_worker_capacity_exhausted"
        );
        assert!(finished.load(Ordering::SeqCst));
        assert!(third.groups[0].outcomes[0].is_success());
    }

    #[test]
    fn mutating_tools_are_split_into_sequential_groups() {
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(
                descriptor("read_file", ToolMutability::ReadOnly),
                |_context, _call| Ok(ToolHandlerOutput::new("read", json!({}))),
            ))
            .expect("register read");
        registry
            .register(StaticToolHandler::new(
                descriptor("write_file", ToolMutability::Mutating),
                |_context, _call| Ok(ToolHandlerOutput::new("write", json!({}))),
            ))
            .expect("register write");

        let groups = registry.plan_batch(&[
            call("call-1", "read_file", "a"),
            call("call-2", "write_file", "b"),
            call("call-3", "read_file", "c"),
        ]);

        assert_eq!(
            groups
                .iter()
                .map(|group| group.mode.clone())
                .collect::<Vec<_>>(),
            vec![
                ToolGroupExecutionMode::ParallelReadOnly,
                ToolGroupExecutionMode::SequentialMutating,
                ToolGroupExecutionMode::ParallelReadOnly,
            ]
        );
    }

    #[test]
    fn group_hook_runs_before_the_next_mutating_group() {
        let lifecycle = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut registry = ToolRegistryV2::new().with_cooperative_mutation_boundary_for_tests();
        for (tool_name, expected_prior_hook) in
            [("write_first", None), ("write_second", Some("hook:call-1"))]
        {
            let lifecycle = Arc::clone(&lifecycle);
            registry
                .register(StaticToolHandler::new(
                    descriptor(tool_name, ToolMutability::Mutating),
                    move |_context, call| {
                        if let Some(expected) = expected_prior_hook {
                            assert!(lifecycle
                                .lock()
                                .unwrap()
                                .iter()
                                .any(|item| item == expected));
                        }
                        lifecycle
                            .lock()
                            .unwrap()
                            .push(format!("execute:{}", call.tool_call_id));
                        Ok(ToolHandlerOutput::new("write", json!({})))
                    },
                ))
                .expect("register mutating tool");
        }

        let lifecycle_for_hook = Arc::clone(&lifecycle);
        let report = registry
            .dispatch_batch_with_group_hook(
                &[
                    call("call-1", "write_first", "a"),
                    call("call-2", "write_second", "b"),
                ],
                &ToolDispatchConfig::default(),
                move |group| {
                    let ToolDispatchOutcome::Succeeded(success) = &group.outcomes[0] else {
                        panic!("mutation should succeed");
                    };
                    lifecycle_for_hook
                        .lock()
                        .unwrap()
                        .push(format!("hook:{}", success.tool_call_id));
                    Ok::<(), ()>(())
                },
            )
            .expect("group hook succeeds");

        assert_eq!(report.groups.len(), 2);
        assert_eq!(
            *lifecycle.lock().unwrap(),
            vec![
                "execute:call-1",
                "hook:call-1",
                "execute:call-2",
                "hook:call-2",
            ]
        );
    }

    #[test]
    fn tool_result_includes_structured_truncation_metadata() {
        let mut descriptor = descriptor("read_file", ToolMutability::ReadOnly);
        descriptor.result_truncation.max_output_bytes = 20;
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(descriptor, |_context, _call| {
                Ok(ToolHandlerOutput::new(
                    "read",
                    json!({ "text": "abcdefghijklmnopqrstuvwxyz" }),
                ))
            }))
            .expect("register read");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            call("call-1", "read_file", "src/lib.rs"),
            &mut tracker,
            &ToolDispatchConfig::default(),
        );

        let ToolDispatchOutcome::Succeeded(success) = outcome else {
            panic!("expected success");
        };
        assert!(success.truncation.was_truncated);
        assert!(success.output.get("xeroTruncated").is_some());
        assert_eq!(
            success
                .sandbox_metadata
                .expect("sandbox metadata")
                .exit_classification,
            SandboxExitClassification::Success
        );
    }

    #[test]
    fn structured_truncation_preserves_json_object_shape_when_requested() {
        let mut descriptor = descriptor("structured_read", ToolMutability::ReadOnly);
        descriptor.result_truncation.max_output_bytes = 220;
        descriptor.result_truncation.preserve_json_shape = true;
        let mut registry = ToolRegistryV2::new();
        registry
            .register(StaticToolHandler::new(descriptor, |_context, _call| {
                Ok(ToolHandlerOutput::new(
                    "read",
                    json!({
                        "path": "src/lib.rs",
                        "content": "a".repeat(2000),
                    }),
                ))
            }))
            .expect("register structured read");
        let mut tracker = ToolBudgetTracker::new(ToolBudget::default());

        let outcome = registry.dispatch_call(
            call("call-1", "structured_read", "src/lib.rs"),
            &mut tracker,
            &ToolDispatchConfig::default(),
        );

        let ToolDispatchOutcome::Succeeded(success) = outcome else {
            panic!("expected success");
        };
        assert!(success.truncation.was_truncated);
        assert_eq!(success.output["path"], json!("src/lib.rs"));
        assert!(success.output["content"].as_str().is_some());
        assert!(success.output.get("xeroTruncation").is_some());
        assert!(success.output.get("xeroTruncated").is_none());
    }
}
