use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};

use crate::{NoopToolSandbox, SandboxExecutionMetadata, ToolSandbox};

const DEFAULT_TOOL_CALL_LIMIT: usize = 128;
const DEFAULT_TOOL_FAILURE_LIMIT: usize = 16;
const DEFAULT_REPEATED_EQUIVALENT_CALL_LIMIT: usize = 3;
const DEFAULT_COMMAND_OUTPUT_BYTES: usize = 64 * 1024;
const DEFAULT_GROUP_WALL_CLOCK_MS: u64 = 120_000;

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
            if tag.trim().is_empty() || !seen_tags.insert(tag) {
                return Err(ToolExecutionError::invalid_input(
                    "agent_tool_descriptor_invalid",
                    format!(
                        "Tool descriptor `{}` has empty or duplicate capability tags.",
                        self.name
                    ),
                ));
            }
        }
        Ok(())
    }
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
}

impl Default for ToolDispatchConfig {
    fn default() -> Self {
        Self {
            budget: ToolBudget::default(),
            policy: Arc::new(AllowAllToolPolicy),
            sandbox: Arc::new(NoopToolSandbox),
            rollback: None,
            context: ToolExecutionContext::default(),
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
}

impl Default for ToolBudget {
    fn default() -> Self {
        Self {
            max_tool_calls_per_turn: DEFAULT_TOOL_CALL_LIMIT,
            max_tool_failures_per_turn: DEFAULT_TOOL_FAILURE_LIMIT,
            max_repeated_equivalent_calls: DEFAULT_REPEATED_EQUIVALENT_CALL_LIMIT,
            max_command_output_bytes: DEFAULT_COMMAND_OUTPUT_BYTES,
            max_wall_clock_time_per_tool_group_ms: DEFAULT_GROUP_WALL_CLOCK_MS,
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
pub struct ToolRegistryV2 {
    handlers: BTreeMap<String, Arc<dyn ToolHandler>>,
}

impl ToolRegistryV2 {
    pub fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
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
            reports.push(ToolGroupDispatchReport {
                mode: group.mode,
                elapsed_ms: elapsed.as_millis(),
                outcomes,
                timeout_error,
            });
        }

        ToolBatchDispatchReport { groups: reports }
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
        let (result_tx, result_rx) = mpsc::channel();
        for (index, prepared_call) in prepared {
            pending.insert(
                index,
                PendingReadOnlyToolCall::from_prepared(&prepared_call),
            );
            let context = config.context.clone();
            let rollback = config.rollback.clone();
            let result_tx = result_tx.clone();
            thread::spawn(move || {
                let call = prepared_call.call.clone();
                let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    execute_prepared_call(prepared_call, &context, rollback.as_deref())
                }))
                .unwrap_or_else(|_| panic_failure_outcome(&call));
                let _ = result_tx.send((index, call, outcome));
            });
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
                let mut outcome =
                    execute_prepared_call(prepared, &config.context, config.rollback.as_deref());
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

    let raw_result = prepared
        .handler
        .execute_with_control(context, &prepared.call, &control);
    let post_hook_payload = prepared
        .handler
        .post_hook_payload(&prepared.call, &raw_result);
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
    if descriptor
        .input_schema
        .get("type")
        .and_then(JsonValue::as_str)
        .unwrap_or("object")
        != "object"
    {
        return Ok(());
    }
    let Some(input_object) = input.as_object() else {
        return Err(ToolExecutionError::invalid_input(
            "agent_tool_input_invalid",
            format!("Tool `{}` expects an object input.", descriptor.name),
        ));
    };

    if let Some(required) = descriptor
        .input_schema
        .get("required")
        .and_then(JsonValue::as_array)
    {
        for field in required.iter().filter_map(JsonValue::as_str) {
            if !input_object.contains_key(field) {
                return Err(ToolExecutionError::invalid_input(
                    "agent_tool_input_invalid",
                    format!(
                        "Tool `{}` input is missing required field `{field}`.",
                        descriptor.name
                    ),
                ));
            }
        }
    }

    let Some(properties) = descriptor
        .input_schema
        .get("properties")
        .and_then(JsonValue::as_object)
    else {
        return Ok(());
    };

    for (field, schema) in properties {
        let Some(value) = input_object.get(field) else {
            continue;
        };
        if let Some(expected_type) = schema.get("type").and_then(JsonValue::as_str) {
            let matches = match expected_type {
                "array" => value.is_array(),
                "boolean" => value.is_boolean(),
                "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
                "number" => value.is_number(),
                "object" => value.is_object(),
                "string" => value.is_string(),
                _ => true,
            };
            if !matches {
                return Err(ToolExecutionError::invalid_input(
                    "agent_tool_input_invalid",
                    format!(
                        "Tool `{}` field `{field}` must be `{expected_type}`.",
                        descriptor.name
                    ),
                ));
            }
        }
    }
    Ok(())
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
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use super::*;
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
    struct RecordingRollback {
        checkpoints: Mutex<usize>,
        rollbacks: Mutex<usize>,
    }

    impl ToolRollback for RecordingRollback {
        fn checkpoint_before(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
        ) -> ToolRegistryResult<Option<JsonValue>> {
            *self.checkpoints.lock().unwrap() += 1;
            Ok(Some(json!({ "checkpointFor": call.tool_call_id })))
        }

        fn rollback_after_failure(
            &self,
            call: &ToolCallInput,
            _descriptor: &ToolDescriptorV2,
            checkpoint: &JsonValue,
            _error: &ToolExecutionError,
        ) -> ToolRegistryResult<JsonValue> {
            *self.rollbacks.lock().unwrap() += 1;
            Ok(json!({ "rolledBack": call.tool_call_id, "checkpoint": checkpoint }))
        }
    }

    #[test]
    fn mutating_tool_failure_invokes_rollback_checkpoint() {
        let rollback = Arc::new(RecordingRollback::default());
        let mut registry = ToolRegistryV2::new();
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

        assert!(outcome.failure().unwrap().rollback_payload.is_some());
        assert_eq!(*rollback.checkpoints.lock().unwrap(), 1);
        assert_eq!(*rollback.rollbacks.lock().unwrap(), 1);
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
