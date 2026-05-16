use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::{
    ApprovalDecisionRequest, CancelRunRequest, CompactSessionRequest, ContinueRunRequest,
    CoreError, CoreResult, EnvironmentDiagnostic, EnvironmentHealthCheck,
    EnvironmentLifecycleState, EnvironmentSetupStep, ExportTraceRequest, ForkSessionRequest,
    MessageRole, ProviderSelection, ResumeRunRequest, RunSnapshot, RunStatus, RuntimeEvent,
    RuntimeEventKind, RuntimeTrace, SandboxGroupingPolicy, StartRunRequest, UserInputRequest,
    CORE_PROTOCOL_VERSION,
};

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeTraceContext {
    pub trace_id: String,
    pub span_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    pub run_trace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_turn_trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_decision_trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub storage_write_trace_id: Option<String>,
}

impl RuntimeTraceContext {
    pub fn for_run(run_trace_id: &str, run_id: &str, span_label: &str) -> Self {
        Self {
            trace_id: run_trace_id.into(),
            span_id: runtime_span_id("run", &[run_id, span_label]),
            parent_span_id: None,
            run_trace_id: run_trace_id.into(),
            provider_turn_trace_id: None,
            tool_call_trace_id: None,
            approval_decision_trace_id: None,
            storage_write_trace_id: None,
        }
    }

    pub fn for_event(
        run_trace_id: &str,
        run_id: &str,
        event_id: i64,
        event_kind: &RuntimeEventKind,
    ) -> Self {
        Self {
            trace_id: run_trace_id.into(),
            span_id: runtime_span_id(
                "event",
                &[run_id, &event_id.to_string(), &format!("{event_kind:?}")],
            ),
            parent_span_id: Some(runtime_span_id("run", &[run_id, "events"])),
            run_trace_id: run_trace_id.into(),
            provider_turn_trace_id: None,
            tool_call_trace_id: None,
            approval_decision_trace_id: None,
            storage_write_trace_id: None,
        }
    }

    pub fn for_provider_turn(run_trace_id: &str, run_id: &str, turn_index: usize) -> Self {
        let provider_turn_trace_id = runtime_trace_id(
            "provider_turn",
            &[run_trace_id, run_id, &turn_index.to_string()],
        );
        Self {
            trace_id: run_trace_id.into(),
            span_id: runtime_span_id("provider_turn", &[run_id, &turn_index.to_string()]),
            parent_span_id: Some(runtime_span_id("run", &[run_id, "provider"])),
            run_trace_id: run_trace_id.into(),
            provider_turn_trace_id: Some(provider_turn_trace_id),
            tool_call_trace_id: None,
            approval_decision_trace_id: None,
            storage_write_trace_id: None,
        }
    }

    pub fn for_tool_call(run_trace_id: &str, run_id: &str, tool_call_id: &str) -> Self {
        let tool_call_trace_id =
            runtime_trace_id("tool_call", &[run_trace_id, run_id, tool_call_id]);
        Self {
            trace_id: run_trace_id.into(),
            span_id: runtime_span_id("tool_call", &[run_id, tool_call_id]),
            parent_span_id: Some(runtime_span_id("run", &[run_id, "tools"])),
            run_trace_id: run_trace_id.into(),
            provider_turn_trace_id: None,
            tool_call_trace_id: Some(tool_call_trace_id),
            approval_decision_trace_id: None,
            storage_write_trace_id: None,
        }
    }

    pub fn for_approval(run_trace_id: &str, run_id: &str, action_id: &str) -> Self {
        let approval_decision_trace_id =
            runtime_trace_id("approval", &[run_trace_id, run_id, action_id]);
        Self {
            trace_id: run_trace_id.into(),
            span_id: runtime_span_id("approval", &[run_id, action_id]),
            parent_span_id: Some(runtime_span_id("run", &[run_id, "approvals"])),
            run_trace_id: run_trace_id.into(),
            provider_turn_trace_id: None,
            tool_call_trace_id: None,
            approval_decision_trace_id: Some(approval_decision_trace_id),
            storage_write_trace_id: None,
        }
    }

    pub fn for_context_manifest(
        run_trace_id: &str,
        run_id: &str,
        manifest_id: &str,
        turn_index: usize,
    ) -> Self {
        let storage_write_trace_id =
            runtime_trace_id("context_manifest", &[run_trace_id, run_id, manifest_id]);
        Self {
            trace_id: run_trace_id.into(),
            span_id: runtime_span_id("context_manifest", &[run_id, manifest_id]),
            parent_span_id: Some(runtime_span_id(
                "provider_turn",
                &[run_id, &turn_index.to_string()],
            )),
            run_trace_id: run_trace_id.into(),
            provider_turn_trace_id: Some(runtime_trace_id(
                "provider_turn",
                &[run_trace_id, run_id, &turn_index.to_string()],
            )),
            tool_call_trace_id: None,
            approval_decision_trace_id: None,
            storage_write_trace_id: Some(storage_write_trace_id),
        }
    }

    pub fn for_storage_write(
        run_trace_id: &str,
        run_id: &str,
        storage_kind: &str,
        sequence: usize,
    ) -> Self {
        let storage_write_trace_id = runtime_trace_id(
            "storage_write",
            &[run_trace_id, run_id, storage_kind, &sequence.to_string()],
        );
        Self {
            trace_id: run_trace_id.into(),
            span_id: runtime_span_id(
                "storage_write",
                &[run_id, storage_kind, &sequence.to_string()],
            ),
            parent_span_id: Some(runtime_span_id("run", &[run_id, "storage"])),
            run_trace_id: run_trace_id.into(),
            provider_turn_trace_id: None,
            tool_call_trace_id: None,
            approval_decision_trace_id: None,
            storage_write_trace_id: Some(storage_write_trace_id),
        }
    }

    pub fn is_valid(&self) -> bool {
        is_lower_hex_len(&self.trace_id, 32)
            && is_lower_hex_len(&self.span_id, 16)
            && self
                .parent_span_id
                .as_deref()
                .is_none_or(|span_id| is_lower_hex_len(span_id, 16))
            && self.run_trace_id == self.trace_id
            && self
                .provider_turn_trace_id
                .as_deref()
                .is_none_or(|trace_id| is_lower_hex_len(trace_id, 32))
            && self
                .tool_call_trace_id
                .as_deref()
                .is_none_or(|trace_id| is_lower_hex_len(trace_id, 32))
            && self
                .approval_decision_trace_id
                .as_deref()
                .is_none_or(|trace_id| is_lower_hex_len(trace_id, 32))
            && self
                .storage_write_trace_id
                .as_deref()
                .is_none_or(|trace_id| is_lower_hex_len(trace_id, 32))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSubmissionEnvelope {
    pub protocol_version: u32,
    pub submission_id: String,
    pub trace: RuntimeTraceContext,
    pub submitted_at: String,
    pub submission: RuntimeSubmission,
}

impl RuntimeSubmissionEnvelope {
    pub fn validate_protocol_version(&self) -> CoreResult<()> {
        ensure_runtime_protocol_version(self.protocol_version)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "payload")]
pub enum RuntimeSubmission {
    StartRun(StartRunRequest),
    ContinueRun(ContinueRunRequest),
    UserMessage(UserInputRequest),
    ApprovalDecision(ApprovalDecisionRequest),
    ToolPermissionGrant(ToolPermissionGrantRequest),
    Cancel(CancelRunRequest),
    Resume(ResumeRunRequest),
    Fork(ForkSessionRequest),
    Compact(CompactSessionRequest),
    ExportTrace(ExportTraceRequest),
    ProviderModelChange(ProviderModelChangeRequest),
    RuntimeSettingsChange(RuntimeSettingsChangeRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolPermissionGrantRequest {
    pub project_id: String,
    pub run_id: String,
    pub grant_id: String,
    pub tool_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProviderModelChangeRequest {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    pub provider: ProviderSelection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSettingsChangeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    pub settings: JsonValue,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSubmissionOutcome {
    pub protocol_version: u32,
    pub accepted_submission_id: String,
    pub trace: RuntimeTraceContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<RunSnapshot>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_export: Option<RuntimeTrace>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeProtocolEvent {
    pub protocol_version: u32,
    pub event_id: i64,
    pub project_id: String,
    pub run_id: String,
    pub event_kind: RuntimeEventKind,
    pub trace: RuntimeTraceContext,
    pub payload: RuntimeProtocolEventPayload,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    tag = "kind",
    content = "payload"
)]
pub enum RuntimeProtocolEventPayload {
    RunStarted {
        status: RunStatus,
        provider_id: String,
        model_id: String,
    },
    RunCompleted {
        summary: String,
        state: String,
    },
    RunFailed {
        code: String,
        message: String,
        retryable: bool,
    },
    StateTransition {
        from: String,
        to: String,
        reason: Option<String>,
    },
    MessageDelta {
        role: MessageRole,
        text: String,
    },
    ReasoningSummary {
        text: String,
    },
    ToolStarted {
        tool_call_id: String,
        tool_name: String,
    },
    ToolDelta {
        tool_call_id: String,
        text: String,
    },
    ToolCompleted {
        tool_call_id: String,
        outcome: String,
    },
    PolicyDecision {
        subject: String,
        decision: String,
        reason: Option<String>,
    },
    ApprovalRequired {
        action_id: String,
        boundary_id: Option<String>,
        action_type: String,
        title: String,
        detail: String,
    },
    PlanUpdated {
        summary: Option<String>,
        items: Vec<RuntimePlanItem>,
    },
    VerificationGate {
        status: String,
        summary: Option<String>,
    },
    ContextManifestRecorded {
        manifest_id: String,
        context_hash: String,
        turn_index: usize,
    },
    RetrievalPerformed {
        query: String,
        result_count: usize,
        source: Option<String>,
    },
    MemoryCandidateCaptured {
        candidate_id: String,
        candidate_kind: String,
        confidence: u8,
    },
    EnvironmentLifecycleUpdate {
        environment_id: String,
        state: EnvironmentLifecycleState,
        previous_state: Option<EnvironmentLifecycleState>,
        sandbox_id: Option<String>,
        sandbox_grouping_policy: SandboxGroupingPolicy,
        pending_message_count: usize,
        health_checks: Vec<EnvironmentHealthCheck>,
        setup_steps: Vec<EnvironmentSetupStep>,
        detail: Option<String>,
        diagnostic: Option<EnvironmentDiagnostic>,
    },
    SandboxLifecycleUpdate {
        sandbox_id: Option<String>,
        phase: String,
        detail: Option<String>,
    },
    ValidationStarted {
        label: String,
    },
    ValidationCompleted {
        label: String,
        outcome: String,
    },
    ToolRegistrySnapshot {
        tool_count: usize,
        tool_names: Vec<String>,
    },
    FileChanged {
        path: String,
        operation: String,
    },
    CommandOutput {
        tool_call_id: Option<String>,
        stream: String,
        text: String,
    },
    ToolPermissionGrant {
        grant_id: String,
        tool_name: String,
    },
    ProviderModelChanged {
        provider_id: String,
        model_id: String,
    },
    RuntimeSettingsChanged {
        summary: String,
    },
    RunPaused {
        reason: Option<String>,
    },
    Untyped {
        event_kind: RuntimeEventKind,
        payload: JsonValue,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimePlanItem {
    pub id: String,
    pub text: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplayedRunTimeline {
    pub protocol_version: u32,
    pub trace_id: String,
    pub project_id: String,
    pub run_id: String,
    pub status: RunStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing_segments: Vec<ReplayMissingTimelineSegment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub corrupt_segments: Vec<ReplayCorruptTimelineSegment>,
    pub items: Vec<ReplayedRunTimelineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplayMissingTimelineSegment {
    pub start_event_id: i64,
    pub end_event_id: i64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplayCorruptTimelineSegment {
    pub event_id: i64,
    pub code: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplayedRunTimelineItem {
    pub event_id: i64,
    pub event_kind: RuntimeEventKind,
    pub trace: RuntimeTraceContext,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    pub occurred_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CanonicalRuntimeTraceSnapshot {
    pub schema: String,
    pub protocol_version: u32,
    pub trace_id: String,
    pub trace: RuntimeTrace,
    pub timeline: ReplayedRunTimeline,
    pub diagnostics: TraceDiagnosticsReport,
    pub quality_gates: TraceQualityGateReport,
    pub production_readiness: ProductionReadinessReport,
    pub redaction_report: TraceRedactionReport,
    pub export_formats: Vec<RuntimeTraceExportFormat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTraceExportFormat {
    JsonTrace,
    MarkdownSummary,
    RedactedSupportBundle,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSupportBundle {
    pub schema: String,
    pub generated_from: String,
    pub protocol_version: u32,
    pub trace_id: String,
    pub run: RuntimeSupportRunMetadata,
    pub timeline: ReplayedRunTimeline,
    pub diagnostics: TraceDiagnosticsReport,
    pub quality_gates: TraceQualityGateReport,
    pub production_readiness: ProductionReadinessReport,
    pub redaction_report: TraceRedactionReport,
    pub redacted_trace: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeSupportRunMetadata {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub status: RunStatus,
    pub message_count: usize,
    pub event_count: usize,
    pub context_manifest_count: usize,
}

pub const PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS: &[&str] = &[
    "command -v protoc",
    "cargo fmt -p xero-agent-core -- --check",
    "cargo fmt -p xero-cli -- --check",
    "cargo test -p xero-agent-core provider_preflight -- --nocapture",
    "cargo test -p xero-agent-core tool_registry -- --nocapture",
    "cargo test -p xero-agent-core trace_quality -- --nocapture",
    "cargo test -p xero-cli real_provider_uses_project_store -- --nocapture",
    "cargo test -p xero-cli real_provider_uses_tool_registry_v2 -- --nocapture",
    "cargo test -p xero-cli live_provider_preflight_probe -- --nocapture",
    "cargo test -p xero-cli mcp_uses_canonical_runtime -- --nocapture",
    "cargo test -p xero-desktop --test agent_core_runtime provider_preflight_manifest_binding -- --nocapture",
    "cargo test -p xero-desktop --test agent_core_runtime tool_registry_v2_enforces_sandbox_denial -- --nocapture",
    "cargo test -p xero-desktop --test agent_core_runtime tool_group_timeout_interrupts_hung_read_only_handler -- --nocapture",
    "cargo test -p xero-desktop --test agent_core_runtime workspace_index_required_blocks_lifecycle -- --nocapture",
    "cargo test -p xero-desktop --test agent_core_runtime canonical_trace_passes_production_gates -- --nocapture",
    "pnpm --dir client vitest run src/lib/xero-model/provider-models.test.ts src/lib/xero-model/runtime-protocol.test.ts src/lib/xero-model/agent.test.ts src/features/xero/use-xero-desktop-state/runtime-stream.test.ts",
];

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductionReadinessStatus {
    Ready,
    Blocked,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductionReadinessFocusedTestStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductionReadinessFocusedTestResult {
    pub command: String,
    pub status: ProductionReadinessFocusedTestStatus,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_at: Option<String>,
}

impl ProductionReadinessFocusedTestResult {
    pub fn passed(command: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            status: ProductionReadinessFocusedTestStatus::Passed,
            summary: summary.into(),
            checked_at: None,
        }
    }

    pub fn failed(command: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            status: ProductionReadinessFocusedTestStatus::Failed,
            summary: summary.into(),
            checked_at: None,
        }
    }

    pub fn skipped(command: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            status: ProductionReadinessFocusedTestStatus::Skipped,
            summary: summary.into(),
            checked_at: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductionReadinessGateSummary {
    pub total: usize,
    pub passed: usize,
    pub warned: usize,
    pub failed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductionReadinessFocusedTestSummary {
    pub required_count: usize,
    pub provided_count: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub missing_required_commands: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProductionReadinessBlockerKind {
    TraceGate,
    FocusedTestFailed,
    FocusedTestSkipped,
    FocusedTestMissing,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductionReadinessBlocker {
    pub kind: ProductionReadinessBlockerKind,
    pub layer: TraceFailureLayer,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<TraceQualityGateCategory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProductionReadinessReport {
    pub schema: String,
    pub generated_from: String,
    pub protocol_version: u32,
    pub trace_id: String,
    pub status: ProductionReadinessStatus,
    pub gate_summary: ProductionReadinessGateSummary,
    pub focused_test_summary: ProductionReadinessFocusedTestSummary,
    pub failing_categories: Vec<TraceQualityGateCategory>,
    pub failing_layers: Vec<TraceFailureLayer>,
    pub blockers: Vec<ProductionReadinessBlocker>,
    pub focused_tests: Vec<ProductionReadinessFocusedTestResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceDiagnosticCategory {
    ContextPressure,
    ToolFailure,
    ToolDenial,
    ApprovalWait,
    SandboxDenial,
    ProviderRetry,
    RetrievalUsage,
    RedactionEvent,
    StorageError,
    ProviderCapabilityState,
    EnvironmentLifecycleState,
    MissingTimelineSegment,
    TraceExportConversionFailure,
    VerificationFailure,
    RuntimeFailure,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceDiagnosticSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TraceFailureLayer {
    Provider,
    Lifecycle,
    ContextAssembly,
    Retrieval,
    ToolSchema,
    ToolDispatch,
    Approval,
    Sandbox,
    Filesystem,
    Timeout,
    ProviderHistory,
    WorkspaceIndex,
    Verification,
    Storage,
    Redaction,
    Policy,
    Runtime,
    Protocol,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceDiagnosticSignal {
    pub category: TraceDiagnosticCategory,
    pub severity: TraceDiagnosticSeverity,
    pub trace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<i64>,
    pub layer: TraceFailureLayer,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceDiagnosticsReport {
    pub signals: Vec<TraceDiagnosticSignal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub most_likely_failing_layer: Option<TraceFailureLayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TraceQualityGateCategory {
    PromptInjectionRegression,
    EnvironmentLifecycleEvents,
    ProductionRuntimeStore,
    SandboxPolicy,
    SandboxDenialOutcome,
    ProviderCapabilityPreflight,
    ProviderPreflightSnapshot,
    ProviderPreflightManifestBinding,
    ToolRegistryV2Execution,
    SubprocessSandboxMetadata,
    ToolTimeoutMetadata,
    ProviderHistoryReplay,
    WorkspaceIndexLifecycle,
    ToolSchemaValidation,
    ContextManifestDeterminism,
    ContextManifestBeforeProviderTurn,
    EventProtocolSchemaSnapshot,
    SupportBundleRedaction,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TraceQualityGateStatus {
    Pass,
    Warn,
    Fail,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceQualityGateResult {
    pub category: TraceQualityGateCategory,
    pub status: TraceQualityGateStatus,
    pub trace_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<i64>,
    pub regression_category: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceQualityGateReport {
    pub passed: bool,
    pub gates: Vec<TraceQualityGateResult>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum TraceRedactionCategory {
    Secret,
    BearerHeader,
    OAuthToken,
    CredentialPath,
    PrivateKeyPath,
    SecretUrl,
    RawFileContents,
    UnapprovedMemoryText,
    RawTranscriptText,
}

impl TraceRedactionCategory {
    const fn marker(self) -> &'static str {
        match self {
            Self::Secret => "secret",
            Self::BearerHeader => "bearer_header",
            Self::OAuthToken => "oauth_token",
            Self::CredentialPath => "credential_path",
            Self::PrivateKeyPath => "private_key_path",
            Self::SecretUrl => "secret_url",
            Self::RawFileContents => "raw_file_contents",
            Self::UnapprovedMemoryText => "unapproved_memory_text",
            Self::RawTranscriptText => "raw_transcript_text",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TraceRedactionReport {
    pub applied_by_default: bool,
    pub redacted_value_count: usize,
    pub secret_like_value_count: usize,
    pub categories: Vec<TraceRedactionCategory>,
}

pub trait AgentProtocolRuntime {
    fn submit_protocol(
        &self,
        submission: RuntimeSubmissionEnvelope,
    ) -> CoreResult<RuntimeSubmissionOutcome>;
}

impl RuntimeTrace {
    pub fn from_snapshot(snapshot: RunSnapshot) -> CoreResult<Self> {
        let events = snapshot
            .events
            .iter()
            .map(RuntimeEvent::to_protocol_event)
            .collect::<CoreResult<Vec<_>>>()?;
        Ok(Self {
            protocol_version: CORE_PROTOCOL_VERSION,
            trace_id: snapshot.trace_id.clone(),
            snapshot,
            events,
        })
    }

    pub fn validate_protocol_version(&self) -> CoreResult<()> {
        ensure_runtime_protocol_version(self.protocol_version)
    }

    pub fn replay_timeline(&self) -> CoreResult<ReplayedRunTimeline> {
        self.validate_protocol_version()?;
        let mut events = if self.events.is_empty() {
            self.snapshot
                .events
                .iter()
                .map(RuntimeEvent::to_protocol_event)
                .collect::<CoreResult<Vec<_>>>()?
        } else {
            self.events.clone()
        };
        let mut corrupt_segments = Vec::new();
        events.sort_by_key(|event| event.event_id);
        let mut status = self.snapshot.status.clone();
        let mut items = Vec::with_capacity(events.len());
        let mut missing_segments = Vec::new();
        let mut previous_event_id = 0;

        for event in events {
            if event.event_id <= previous_event_id {
                corrupt_segments.push(ReplayCorruptTimelineSegment {
                    event_id: event.event_id,
                    code: "event_id_not_monotonic".into(),
                    summary: format!(
                        "Event id `{}` was not strictly after `{previous_event_id}` in the replay source.",
                        event.event_id
                    ),
                });
            } else if event.event_id > previous_event_id.saturating_add(1) {
                let start_event_id = previous_event_id.saturating_add(1);
                let end_event_id = event.event_id.saturating_sub(1);
                missing_segments.push(ReplayMissingTimelineSegment {
                    start_event_id,
                    end_event_id,
                    summary: format!(
                        "Replay detected a missing event range from `{start_event_id}` through `{end_event_id}` before event `{}`.",
                        event.event_id
                    ),
                });
            }
            previous_event_id = previous_event_id.max(event.event_id);

            match &event.payload {
                RuntimeProtocolEventPayload::RunStarted { status: next, .. } => {
                    status = next.clone();
                }
                RuntimeProtocolEventPayload::RunCompleted { .. } => {
                    status = RunStatus::Completed;
                }
                RuntimeProtocolEventPayload::RunFailed { .. } => {
                    status = RunStatus::Failed;
                }
                RuntimeProtocolEventPayload::RunPaused { .. } => {
                    status = RunStatus::Paused;
                }
                RuntimeProtocolEventPayload::StateTransition { to, .. } => {
                    if let Some(next) = run_status_from_wire(to) {
                        status = next;
                    }
                }
                _ => {}
            }

            items.push(ReplayedRunTimelineItem {
                event_id: event.event_id,
                event_kind: event.event_kind.clone(),
                trace: event.trace.clone(),
                label: replay_label(&event.payload),
                text: replay_text(&event.payload),
                occurred_at: event.occurred_at.clone(),
            });
        }

        Ok(ReplayedRunTimeline {
            protocol_version: self.protocol_version,
            trace_id: self.trace_id.clone(),
            project_id: self.snapshot.project_id.clone(),
            run_id: self.snapshot.run_id.clone(),
            status,
            missing_segments,
            corrupt_segments,
            items,
        })
    }

    pub fn canonical_snapshot(&self) -> CoreResult<CanonicalRuntimeTraceSnapshot> {
        let timeline = self.replay_timeline()?;
        let (redacted_trace, redaction_report) =
            redacted_support_value(&serde_json::to_value(self).map_err(|error| {
                CoreError::system_fault(
                    "agent_trace_encode_failed",
                    format!("Xero could not encode the runtime trace before redaction: {error}"),
                )
            })?);
        let redaction_failed = contains_unredacted_secret_like_value(&redacted_trace);
        let diagnostics = diagnose_trace(self, &timeline, &redaction_report);
        let quality_gates = quality_gates_for_trace(
            self,
            &timeline,
            &diagnostics,
            &redaction_report,
            redaction_failed,
        );
        let production_readiness = production_readiness_report_for_trace(
            self.protocol_version,
            &self.trace_id,
            &quality_gates,
            &diagnostics,
            Vec::new(),
        );

        Ok(CanonicalRuntimeTraceSnapshot {
            schema: "xero.canonical_runtime_trace_snapshot.v1".into(),
            protocol_version: self.protocol_version,
            trace_id: self.trace_id.clone(),
            trace: self.clone(),
            timeline,
            diagnostics,
            quality_gates,
            production_readiness,
            redaction_report,
            export_formats: vec![
                RuntimeTraceExportFormat::JsonTrace,
                RuntimeTraceExportFormat::MarkdownSummary,
                RuntimeTraceExportFormat::RedactedSupportBundle,
            ],
        })
    }

    pub fn redacted_support_bundle(&self) -> CoreResult<RuntimeSupportBundle> {
        let canonical = self.canonical_snapshot()?;
        let (redacted_trace, redaction_report) = redacted_support_value(&serde_json::to_value(self).map_err(|error| {
            CoreError::system_fault(
                "agent_trace_encode_failed",
                format!("Xero could not encode the runtime trace before support-bundle redaction: {error}"),
            )
        })?);
        let redacted_timeline = redacted_timeline_for_support(&canonical.timeline)?;
        let redacted_diagnostics = redacted_json_round_trip(
            &canonical.diagnostics,
            "agent_diagnostics_encode_failed",
            "agent_diagnostics_decode_failed",
            "runtime diagnostics",
        )?;
        let redacted_quality_gates = redacted_json_round_trip(
            &canonical.quality_gates,
            "agent_quality_gates_encode_failed",
            "agent_quality_gates_decode_failed",
            "runtime quality gates",
        )?;
        let redacted_production_readiness = redacted_json_round_trip(
            &canonical.production_readiness,
            "agent_production_readiness_encode_failed",
            "agent_production_readiness_decode_failed",
            "production readiness report",
        )?;

        Ok(RuntimeSupportBundle {
            schema: "xero.runtime_support_bundle.v1".into(),
            generated_from: "canonical_runtime_trace_snapshot".into(),
            protocol_version: canonical.protocol_version,
            trace_id: canonical.trace_id,
            run: RuntimeSupportRunMetadata {
                project_id: self.snapshot.project_id.clone(),
                agent_session_id: self.snapshot.agent_session_id.clone(),
                run_id: self.snapshot.run_id.clone(),
                provider_id: self.snapshot.provider_id.clone(),
                model_id: self.snapshot.model_id.clone(),
                status: self.snapshot.status.clone(),
                message_count: self.snapshot.messages.len(),
                event_count: self.snapshot.events.len(),
                context_manifest_count: self.snapshot.context_manifests.len(),
            },
            timeline: redacted_timeline,
            diagnostics: redacted_diagnostics,
            quality_gates: redacted_quality_gates,
            production_readiness: redacted_production_readiness,
            redaction_report,
            redacted_trace,
        })
    }

    pub fn production_readiness_report(
        &self,
        focused_tests: Vec<ProductionReadinessFocusedTestResult>,
    ) -> CoreResult<ProductionReadinessReport> {
        let canonical = self.canonical_snapshot()?;
        Ok(canonical.production_readiness_report(focused_tests))
    }

    pub fn to_markdown_summary(&self) -> CoreResult<String> {
        let bundle = self.redacted_support_bundle()?;
        let gate_status = if bundle.quality_gates.passed {
            "PASS"
        } else {
            "FAIL"
        };
        let readiness_status = match bundle.production_readiness.status {
            ProductionReadinessStatus::Ready => "READY",
            ProductionReadinessStatus::Blocked => "BLOCKED",
        };
        let failing_layer = bundle
            .diagnostics
            .most_likely_failing_layer
            .map(|layer| format!("{layer:?}"))
            .unwrap_or_else(|| "None".into());
        let mut lines = vec![
            format!("# Xero Run Trace `{}`", bundle.run.run_id),
            String::new(),
            format!("- Trace: `{}`", bundle.trace_id),
            format!(
                "- Provider/model: `{}/{}`",
                bundle.run.provider_id, bundle.run.model_id
            ),
            format!("- Status: `{:?}`", bundle.run.status),
            format!("- Quality gates: `{gate_status}`"),
            format!("- Production readiness: `{readiness_status}`"),
            format!("- Most likely failing layer: `{failing_layer}`"),
            format!("- Timeline items: {}", bundle.timeline.items.len()),
            format!(
                "- Missing timeline segments: {}",
                bundle.timeline.missing_segments.len()
            ),
            format!(
                "- Redacted values: {}",
                bundle.redaction_report.redacted_value_count
            ),
            String::new(),
            "## Timeline".into(),
        ];
        for item in bundle.timeline.items.iter().take(50) {
            lines.push(format!(
                "- #{} `{:?}` {}",
                item.event_id, item.event_kind, item.label
            ));
        }
        if bundle.timeline.items.len() > 50 {
            lines.push(format!(
                "- ... {} more item(s)",
                bundle.timeline.items.len() - 50
            ));
        }
        if !bundle.diagnostics.signals.is_empty() {
            lines.push(String::new());
            lines.push("## Diagnostics".into());
            for signal in &bundle.diagnostics.signals {
                lines.push(format!(
                    "- `{:?}` `{:?}`: {}",
                    signal.severity, signal.category, signal.summary
                ));
            }
        }
        lines.push(String::new());
        Ok(lines.join("\n"))
    }
}

impl CanonicalRuntimeTraceSnapshot {
    pub fn production_readiness_report(
        &self,
        focused_tests: Vec<ProductionReadinessFocusedTestResult>,
    ) -> ProductionReadinessReport {
        production_readiness_report_for_trace(
            self.protocol_version,
            &self.trace_id,
            &self.quality_gates,
            &self.diagnostics,
            focused_tests,
        )
    }
}

fn redacted_timeline_for_support(
    timeline: &ReplayedRunTimeline,
) -> CoreResult<ReplayedRunTimeline> {
    redacted_json_round_trip(
        timeline,
        "agent_timeline_encode_failed",
        "agent_timeline_decode_failed",
        "runtime timeline",
    )
}

fn redacted_json_round_trip<T>(
    value: &T,
    encode_code: &'static str,
    decode_code: &'static str,
    label: &'static str,
) -> CoreResult<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
{
    let value = serde_json::to_value(value).map_err(|error| {
        CoreError::system_fault(
            encode_code,
            format!("Xero could not encode the {label} before redaction: {error}"),
        )
    })?;
    let (redacted, _) = redacted_support_value(&value);
    serde_json::from_value(redacted).map_err(|error| {
        CoreError::system_fault(
            decode_code,
            format!("Xero could not decode the redacted {label}: {error}"),
        )
    })
}

fn diagnose_trace(
    trace: &RuntimeTrace,
    timeline: &ReplayedRunTimeline,
    redaction_report: &TraceRedactionReport,
) -> TraceDiagnosticsReport {
    let mut signals = Vec::new();

    for segment in &timeline.missing_segments {
        signals.push(TraceDiagnosticSignal {
            category: TraceDiagnosticCategory::MissingTimelineSegment,
            severity: TraceDiagnosticSeverity::Error,
            trace_id: trace.trace_id.clone(),
            event_id: Some(segment.start_event_id),
            layer: TraceFailureLayer::Protocol,
            summary: segment.summary.clone(),
        });
    }

    if redaction_report.redacted_value_count > 0 {
        signals.push(TraceDiagnosticSignal {
            category: TraceDiagnosticCategory::RedactionEvent,
            severity: TraceDiagnosticSeverity::Info,
            trace_id: trace.trace_id.clone(),
            event_id: None,
            layer: TraceFailureLayer::Redaction,
            summary: format!(
                "Support-bundle redaction removed {} value(s) across {} categor(ies).",
                redaction_report.redacted_value_count,
                redaction_report.categories.len()
            ),
        });
    }

    if trace
        .events
        .iter()
        .all(|event| event.event_kind != RuntimeEventKind::EnvironmentLifecycleUpdate)
    {
        signals.push(TraceDiagnosticSignal {
            category: TraceDiagnosticCategory::EnvironmentLifecycleState,
            severity: TraceDiagnosticSeverity::Error,
            trace_id: trace.trace_id.clone(),
            event_id: trace.events.first().map(|event| event.event_id),
            layer: TraceFailureLayer::Runtime,
            summary: "The trace has no persisted environment lifecycle event.".into(),
        });
    }

    for manifest in &trace.snapshot.context_manifests {
        if let Some(preflight) = provider_preflight_value(&manifest.manifest) {
            if provider_preflight_value_is_not_live_ready(preflight) {
                signals.push(TraceDiagnosticSignal {
                    category: TraceDiagnosticCategory::ProviderCapabilityState,
                    severity: TraceDiagnosticSeverity::Warning,
                    trace_id: trace.trace_id.clone(),
                    event_id: manifest.recorded_after_event_id,
                    layer: TraceFailureLayer::Provider,
                    summary: "Provider preflight snapshot is stale, unavailable, static, cached, warning, or failed.".into(),
                });
            }
        }
    }

    for event in &trace.events {
        let raw_payload = trace
            .snapshot
            .events
            .iter()
            .find(|raw| raw.id == event.event_id)
            .map(|raw| &raw.payload)
            .unwrap_or(&JsonValue::Null);
        match &event.payload {
            RuntimeProtocolEventPayload::ToolCompleted { outcome, .. } => {
                if text_indicates_failure(outcome) {
                    signals.push(event_signal(
                        trace,
                        event,
                        TraceDiagnosticCategory::ToolFailure,
                        TraceDiagnosticSeverity::Error,
                        TraceFailureLayer::ToolSchema,
                        format!("Tool completion outcome was `{outcome}`."),
                    ));
                }
            }
            RuntimeProtocolEventPayload::PolicyDecision {
                subject,
                decision,
                reason,
            } => {
                if text_indicates_denial(decision) {
                    let layer = if subject.to_ascii_lowercase().contains("sandbox") {
                        TraceFailureLayer::Sandbox
                    } else {
                        TraceFailureLayer::Policy
                    };
                    let category = if layer == TraceFailureLayer::Sandbox {
                        TraceDiagnosticCategory::SandboxDenial
                    } else {
                        TraceDiagnosticCategory::ToolDenial
                    };
                    signals.push(event_signal(
                        trace,
                        event,
                        category,
                        TraceDiagnosticSeverity::Warning,
                        layer,
                        reason.clone().unwrap_or_else(|| {
                            format!("Policy decision `{decision}` for `{subject}`.")
                        }),
                    ));
                }
            }
            RuntimeProtocolEventPayload::ApprovalRequired { title, .. } => {
                signals.push(event_signal(
                    trace,
                    event,
                    TraceDiagnosticCategory::ApprovalWait,
                    TraceDiagnosticSeverity::Warning,
                    TraceFailureLayer::Approval,
                    format!("Run waited for approval: {title}."),
                ));
            }
            RuntimeProtocolEventPayload::SandboxLifecycleUpdate { phase, detail, .. } => {
                if text_indicates_denial(phase)
                    || detail.as_deref().is_some_and(text_indicates_denial)
                {
                    signals.push(event_signal(
                        trace,
                        event,
                        TraceDiagnosticCategory::SandboxDenial,
                        TraceDiagnosticSeverity::Error,
                        TraceFailureLayer::Sandbox,
                        detail
                            .clone()
                            .unwrap_or_else(|| format!("Sandbox lifecycle phase `{phase}`.")),
                    ));
                }
            }
            RuntimeProtocolEventPayload::RetrievalPerformed { result_count, .. } => {
                signals.push(event_signal(
                    trace,
                    event,
                    TraceDiagnosticCategory::RetrievalUsage,
                    TraceDiagnosticSeverity::Info,
                    TraceFailureLayer::Retrieval,
                    format!("Retrieval contributed {result_count} record(s)."),
                ));
            }
            RuntimeProtocolEventPayload::ValidationCompleted { label, outcome } => {
                if text_indicates_failure(outcome) {
                    signals.push(event_signal(
                        trace,
                        event,
                        TraceDiagnosticCategory::VerificationFailure,
                        TraceDiagnosticSeverity::Error,
                        TraceFailureLayer::Verification,
                        format!("Verification `{label}` completed with `{outcome}`."),
                    ));
                }
            }
            RuntimeProtocolEventPayload::RunFailed { code, message, .. } => {
                signals.push(event_signal(
                    trace,
                    event,
                    TraceDiagnosticCategory::RuntimeFailure,
                    TraceDiagnosticSeverity::Error,
                    classify_failure_layer(code, message),
                    format!("{code}: {message}"),
                ));
            }
            _ => {}
        }

        if json_contains_provider_retry(raw_payload) {
            signals.push(event_signal(
                trace,
                event,
                TraceDiagnosticCategory::ProviderRetry,
                TraceDiagnosticSeverity::Warning,
                TraceFailureLayer::Provider,
                "Provider retry, rate-limit, or response-shape signal appeared in the raw event payload.".into(),
            ));
        }

        if json_contains_storage_error(raw_payload) {
            signals.push(event_signal(
                trace,
                event,
                TraceDiagnosticCategory::StorageError,
                TraceDiagnosticSeverity::Error,
                TraceFailureLayer::Storage,
                "Storage write/read error appeared in the raw event payload.".into(),
            ));
        }

        if provider_preflight_value(raw_payload)
            .is_some_and(provider_preflight_value_is_not_live_ready)
        {
            signals.push(event_signal(
                trace,
                event,
                TraceDiagnosticCategory::ProviderCapabilityState,
                TraceDiagnosticSeverity::Warning,
                TraceFailureLayer::Provider,
                "Provider capability state was stale, manual, cached, or unprobed.".into(),
            ));
        }
    }

    let most_likely_failing_layer = signals
        .iter()
        .find(|signal| signal.severity == TraceDiagnosticSeverity::Error)
        .or_else(|| {
            if trace.snapshot.status == RunStatus::Failed {
                signals
                    .iter()
                    .find(|signal| signal.severity == TraceDiagnosticSeverity::Warning)
            } else {
                None
            }
        })
        .map(|signal| signal.layer)
        .or_else(|| {
            if trace.snapshot.status == RunStatus::Failed {
                Some(TraceFailureLayer::Runtime)
            } else {
                None
            }
        });

    TraceDiagnosticsReport {
        signals,
        most_likely_failing_layer,
    }
}

fn event_signal(
    trace: &RuntimeTrace,
    event: &RuntimeProtocolEvent,
    category: TraceDiagnosticCategory,
    severity: TraceDiagnosticSeverity,
    layer: TraceFailureLayer,
    summary: String,
) -> TraceDiagnosticSignal {
    TraceDiagnosticSignal {
        category,
        severity,
        trace_id: trace.trace_id.clone(),
        event_id: Some(event.event_id),
        layer,
        summary,
    }
}

fn quality_gates_for_trace(
    trace: &RuntimeTrace,
    timeline: &ReplayedRunTimeline,
    diagnostics: &TraceDiagnosticsReport,
    redaction_report: &TraceRedactionReport,
    redaction_failed: bool,
) -> TraceQualityGateReport {
    let mut gates = vec![
        prompt_injection_gate(trace),
        environment_lifecycle_gate(trace),
        production_runtime_store_gate(trace),
        sandbox_policy_gate(trace, diagnostics),
        sandbox_denial_outcome_gate(trace),
        provider_capability_gate(trace, diagnostics),
        provider_preflight_snapshot_gate(trace),
        provider_preflight_manifest_binding_gate(trace),
        tool_registry_v2_execution_gate(trace),
        subprocess_sandbox_metadata_gate(trace),
        tool_timeout_metadata_gate(trace),
        provider_history_replay_gate(trace),
        workspace_index_lifecycle_gate(trace),
        tool_schema_gate(trace),
        context_manifest_gate(trace),
        context_manifest_before_provider_turn_gate(trace),
        event_protocol_gate(trace, timeline),
        support_bundle_redaction_gate(trace, redaction_report, redaction_failed),
    ];
    let passed = gates
        .iter()
        .all(|gate| gate.status != TraceQualityGateStatus::Fail);
    gates.sort_by_key(|gate| format!("{:?}", gate.category));
    TraceQualityGateReport { passed, gates }
}

fn production_readiness_report_for_trace(
    protocol_version: u32,
    trace_id: &str,
    quality_gates: &TraceQualityGateReport,
    diagnostics: &TraceDiagnosticsReport,
    focused_tests: Vec<ProductionReadinessFocusedTestResult>,
) -> ProductionReadinessReport {
    let gate_summary = ProductionReadinessGateSummary {
        total: quality_gates.gates.len(),
        passed: quality_gates
            .gates
            .iter()
            .filter(|gate| gate.status == TraceQualityGateStatus::Pass)
            .count(),
        warned: quality_gates
            .gates
            .iter()
            .filter(|gate| gate.status == TraceQualityGateStatus::Warn)
            .count(),
        failed: quality_gates
            .gates
            .iter()
            .filter(|gate| gate.status == TraceQualityGateStatus::Fail)
            .count(),
    };

    let mut failing_categories = quality_gates
        .gates
        .iter()
        .filter(|gate| gate.status == TraceQualityGateStatus::Fail)
        .map(|gate| gate.category.clone())
        .collect::<Vec<_>>();
    failing_categories.sort();

    let mut blockers = quality_gates
        .gates
        .iter()
        .filter(|gate| gate.status == TraceQualityGateStatus::Fail)
        .map(|gate| ProductionReadinessBlocker {
            kind: ProductionReadinessBlockerKind::TraceGate,
            layer: trace_gate_failure_layer(&gate.category, diagnostics),
            category: Some(gate.category.clone()),
            command: None,
            summary: gate.summary.clone(),
        })
        .collect::<Vec<_>>();

    let provided_commands = focused_tests
        .iter()
        .map(|test| test.command.trim())
        .filter(|command| !command.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>();
    let missing_required_commands = PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS
        .iter()
        .filter(|command| !provided_commands.contains::<str>(*command))
        .map(|command| (*command).to_owned())
        .collect::<Vec<_>>();

    let focused_test_summary = ProductionReadinessFocusedTestSummary {
        required_count: PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS.len(),
        provided_count: focused_tests.len(),
        passed: focused_tests
            .iter()
            .filter(|test| test.status == ProductionReadinessFocusedTestStatus::Passed)
            .count(),
        failed: focused_tests
            .iter()
            .filter(|test| test.status == ProductionReadinessFocusedTestStatus::Failed)
            .count(),
        skipped: focused_tests
            .iter()
            .filter(|test| test.status == ProductionReadinessFocusedTestStatus::Skipped)
            .count(),
        missing_required_commands: missing_required_commands.clone(),
    };

    for test in &focused_tests {
        match test.status {
            ProductionReadinessFocusedTestStatus::Passed => {}
            ProductionReadinessFocusedTestStatus::Failed => {
                blockers.push(ProductionReadinessBlocker {
                    kind: ProductionReadinessBlockerKind::FocusedTestFailed,
                    layer: TraceFailureLayer::Verification,
                    category: None,
                    command: Some(test.command.clone()),
                    summary: test.summary.clone(),
                });
            }
            ProductionReadinessFocusedTestStatus::Skipped => {
                blockers.push(ProductionReadinessBlocker {
                    kind: ProductionReadinessBlockerKind::FocusedTestSkipped,
                    layer: TraceFailureLayer::Verification,
                    category: None,
                    command: Some(test.command.clone()),
                    summary: test.summary.clone(),
                });
            }
        }
    }

    for command in &missing_required_commands {
        blockers.push(ProductionReadinessBlocker {
            kind: ProductionReadinessBlockerKind::FocusedTestMissing,
            layer: TraceFailureLayer::Verification,
            category: None,
            command: Some(command.clone()),
            summary: "Required production-readiness verification evidence was not attached.".into(),
        });
    }

    let failing_layers = blockers
        .iter()
        .map(|blocker| blocker.layer)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let status = if blockers.is_empty() && quality_gates.passed {
        ProductionReadinessStatus::Ready
    } else {
        ProductionReadinessStatus::Blocked
    };

    ProductionReadinessReport {
        schema: "xero.production_readiness_report.v1".into(),
        generated_from: "canonical_runtime_trace_snapshot_and_focused_tests".into(),
        protocol_version,
        trace_id: trace_id.to_owned(),
        status,
        gate_summary,
        focused_test_summary,
        failing_categories,
        failing_layers,
        blockers,
        focused_tests,
    }
}

fn trace_gate_failure_layer(
    category: &TraceQualityGateCategory,
    _diagnostics: &TraceDiagnosticsReport,
) -> TraceFailureLayer {
    match category {
        TraceQualityGateCategory::PromptInjectionRegression => TraceFailureLayer::Policy,
        TraceQualityGateCategory::EnvironmentLifecycleEvents => TraceFailureLayer::Lifecycle,
        TraceQualityGateCategory::ProductionRuntimeStore => TraceFailureLayer::Storage,
        TraceQualityGateCategory::SandboxPolicy
        | TraceQualityGateCategory::SandboxDenialOutcome
        | TraceQualityGateCategory::SubprocessSandboxMetadata => TraceFailureLayer::Sandbox,
        TraceQualityGateCategory::ProviderCapabilityPreflight
        | TraceQualityGateCategory::ProviderPreflightSnapshot => TraceFailureLayer::Provider,
        TraceQualityGateCategory::ProviderPreflightManifestBinding
        | TraceQualityGateCategory::ContextManifestDeterminism
        | TraceQualityGateCategory::ContextManifestBeforeProviderTurn => {
            TraceFailureLayer::ContextAssembly
        }
        TraceQualityGateCategory::ToolRegistryV2Execution => TraceFailureLayer::ToolDispatch,
        TraceQualityGateCategory::ToolTimeoutMetadata => TraceFailureLayer::Timeout,
        TraceQualityGateCategory::ProviderHistoryReplay => TraceFailureLayer::ProviderHistory,
        TraceQualityGateCategory::WorkspaceIndexLifecycle => TraceFailureLayer::WorkspaceIndex,
        TraceQualityGateCategory::ToolSchemaValidation => TraceFailureLayer::ToolSchema,
        TraceQualityGateCategory::EventProtocolSchemaSnapshot => TraceFailureLayer::Protocol,
        TraceQualityGateCategory::SupportBundleRedaction => TraceFailureLayer::Redaction,
    }
}

fn gate(
    trace: &RuntimeTrace,
    category: TraceQualityGateCategory,
    status: TraceQualityGateStatus,
    event_id: Option<i64>,
    regression_category: impl Into<String>,
    summary: impl Into<String>,
) -> TraceQualityGateResult {
    TraceQualityGateResult {
        category,
        status,
        trace_id: trace.trace_id.clone(),
        event_id,
        regression_category: regression_category.into(),
        summary: summary.into(),
    }
}

fn prompt_injection_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    let suspicious_event = trace
        .snapshot
        .events
        .iter()
        .find(|event| json_contains_prompt_injection(&event.payload));
    let denied = trace.events.iter().any(|event| match &event.payload {
        RuntimeProtocolEventPayload::PolicyDecision {
            subject, decision, ..
        } => {
            subject.to_ascii_lowercase().contains("prompt_injection")
                && text_indicates_denial(decision)
        }
        _ => false,
    });

    match (suspicious_event, denied) {
        (Some(event), false) => gate(
            trace,
            TraceQualityGateCategory::PromptInjectionRegression,
            TraceQualityGateStatus::Fail,
            Some(event.id),
            "prompt_injection",
            "Suspicious prompt-injection text reached the trace without a policy-denial event.",
        ),
        _ => gate(
            trace,
            TraceQualityGateCategory::PromptInjectionRegression,
            TraceQualityGateStatus::Pass,
            None,
            "prompt_injection",
            "No unhandled prompt-injection trace signal was found.",
        ),
    }
}

fn environment_lifecycle_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if trace
        .events
        .iter()
        .any(|event| event.event_kind == RuntimeEventKind::EnvironmentLifecycleUpdate)
    {
        return gate(
            trace,
            TraceQualityGateCategory::EnvironmentLifecycleEvents,
            TraceQualityGateStatus::Pass,
            None,
            "environment_lifecycle_events",
            "Environment lifecycle events are present in the canonical trace.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::EnvironmentLifecycleEvents,
        TraceQualityGateStatus::Fail,
        trace.events.first().map(|event| event.event_id),
        "environment_lifecycle_events",
        "The trace is missing persisted environment lifecycle events.",
    )
}

fn production_runtime_store_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if !is_real_owned_provider_trace(trace) {
        return gate(
            trace,
            TraceQualityGateCategory::ProductionRuntimeStore,
            TraceQualityGateStatus::Pass,
            None,
            "production_runtime_store",
            "Fake-provider and external-agent traces are outside the real owned-provider store gate.",
        );
    }

    if let Some(event) = trace
        .snapshot
        .events
        .iter()
        .find(|event| json_contains_headless_file_store_signal(&event.payload))
    {
        return gate(
            trace,
            TraceQualityGateCategory::ProductionRuntimeStore,
            TraceQualityGateStatus::Fail,
            Some(event.id),
            "production_runtime_store",
            "A real owned-provider trace used the headless file-backed runtime/store path.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::ProductionRuntimeStore,
        TraceQualityGateStatus::Pass,
        None,
        "production_runtime_store",
        "Real owned-provider trace did not expose file-backed headless state.",
    )
}

fn sandbox_policy_gate(
    trace: &RuntimeTrace,
    diagnostics: &TraceDiagnosticsReport,
) -> TraceQualityGateResult {
    if let Some(signal) = diagnostics.signals.iter().find(|signal| {
        signal.category == TraceDiagnosticCategory::SandboxDenial
            && signal.severity == TraceDiagnosticSeverity::Error
    }) {
        return gate(
            trace,
            TraceQualityGateCategory::SandboxPolicy,
            TraceQualityGateStatus::Fail,
            signal.event_id,
            "sandbox_policy",
            signal.summary.clone(),
        );
    }
    if trace
        .snapshot
        .events
        .iter()
        .any(|event| json_contains_dangerous_unrestricted_without_approval(&event.payload))
    {
        return gate(
            trace,
            TraceQualityGateCategory::SandboxPolicy,
            TraceQualityGateStatus::Fail,
            None,
            "sandbox_policy",
            "A dangerous unrestricted sandbox profile appeared without explicit approval metadata.",
        );
    }
    gate(
        trace,
        TraceQualityGateCategory::SandboxPolicy,
        TraceQualityGateStatus::Pass,
        None,
        "sandbox_policy",
        "Sandbox decisions are represented without unsafe unrestricted execution.",
    )
}

fn sandbox_denial_outcome_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if let Some(event) = trace
        .snapshot
        .events
        .iter()
        .find(|event| json_contains_sandbox_denial_success_shape(&event.payload))
    {
        return gate(
            trace,
            TraceQualityGateCategory::SandboxDenialOutcome,
            TraceQualityGateStatus::Fail,
            Some(event.id),
            "sandbox_denial_outcome",
            "Sandbox denial appeared with success-shaped tool metadata.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::SandboxDenialOutcome,
        TraceQualityGateStatus::Pass,
        None,
        "sandbox_denial_outcome",
        "Sandbox denials are not represented as successful tool outcomes.",
    )
}

fn provider_capability_gate(
    trace: &RuntimeTrace,
    diagnostics: &TraceDiagnosticsReport,
) -> TraceQualityGateResult {
    if let Some(signal) = diagnostics
        .signals
        .iter()
        .find(|signal| signal.category == TraceDiagnosticCategory::ProviderCapabilityState)
    {
        return gate(
            trace,
            TraceQualityGateCategory::ProviderCapabilityPreflight,
            TraceQualityGateStatus::Fail,
            signal.event_id,
            "provider_capability_preflight",
            signal.summary.clone(),
        );
    }
    gate(
        trace,
        TraceQualityGateCategory::ProviderCapabilityPreflight,
        TraceQualityGateStatus::Pass,
        None,
        "provider_capability_preflight",
        "Provider/model identity is present; capability snapshots remain owned by provider diagnostics.",
    )
}

fn provider_preflight_snapshot_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if trace
        .snapshot
        .context_manifests
        .iter()
        .any(|manifest| provider_preflight_value(&manifest.manifest).is_some())
    {
        return gate(
            trace,
            TraceQualityGateCategory::ProviderPreflightSnapshot,
            TraceQualityGateStatus::Pass,
            None,
            "provider_preflight_snapshot",
            "Provider preflight metadata is attached to the trace.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::ProviderPreflightSnapshot,
        TraceQualityGateStatus::Fail,
        trace.events.first().map(|event| event.event_id),
        "provider_preflight_snapshot",
        "No context manifest includes the provider preflight snapshot used for the provider turn.",
    )
}

fn provider_preflight_manifest_binding_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if trace.snapshot.context_manifests.is_empty() {
        return gate(
            trace,
            TraceQualityGateCategory::ProviderPreflightManifestBinding,
            TraceQualityGateStatus::Pass,
            None,
            "provider_preflight_manifest_binding",
            "No context manifest was available for preflight binding checks.",
        );
    }

    let Some(admitted_preflight) = admitted_provider_preflight_value(trace) else {
        return gate(
            trace,
            TraceQualityGateCategory::ProviderPreflightManifestBinding,
            TraceQualityGateStatus::Fail,
            trace.events.first().map(|event| event.event_id),
            "provider_preflight_manifest_binding",
            "A provider context manifest exists but no admitted provider preflight snapshot was recorded.",
        );
    };
    let admitted_hash = stable_json_fingerprint(admitted_preflight);

    for manifest in &trace.snapshot.context_manifests {
        let Some(manifest_preflight) = provider_preflight_value(&manifest.manifest) else {
            return gate(
                trace,
                TraceQualityGateCategory::ProviderPreflightManifestBinding,
                TraceQualityGateStatus::Fail,
                manifest.recorded_after_event_id,
                "provider_preflight_manifest_binding",
                format!(
                    "Context manifest `{}` lacks the admitted provider preflight snapshot.",
                    manifest.manifest_id
                ),
            );
        };
        let manifest_hash = manifest
            .manifest
            .get("providerPreflightHash")
            .or_else(|| manifest.manifest.get("admittedProviderPreflightHash"))
            .and_then(JsonValue::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| stable_json_fingerprint(manifest_preflight));
        if manifest_preflight != admitted_preflight && manifest_hash != admitted_hash {
            return gate(
                trace,
                TraceQualityGateCategory::ProviderPreflightManifestBinding,
                TraceQualityGateStatus::Fail,
                manifest.recorded_after_event_id,
                "provider_preflight_manifest_binding",
                format!(
                    "Context manifest `{}` does not match the admitted provider preflight snapshot.",
                    manifest.manifest_id
                ),
            );
        }
    }

    gate(
        trace,
        TraceQualityGateCategory::ProviderPreflightManifestBinding,
        TraceQualityGateStatus::Pass,
        None,
        "provider_preflight_manifest_binding",
        "Context manifests carry the admitted provider preflight snapshot or matching stable hash.",
    )
}

fn tool_registry_v2_execution_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    let Some(tool_event) = trace.snapshot.events.iter().find(|event| {
        matches!(
            event.event_kind,
            RuntimeEventKind::ToolStarted
                | RuntimeEventKind::ToolDelta
                | RuntimeEventKind::ToolCompleted
        )
    }) else {
        return gate(
            trace,
            TraceQualityGateCategory::ToolRegistryV2Execution,
            TraceQualityGateStatus::Pass,
            None,
            "tool_registry_v2_execution",
            "No tool execution appeared in the trace.",
        );
    };

    let has_registry_snapshot = trace.snapshot.events.iter().any(|event| {
        event.event_kind == RuntimeEventKind::ToolRegistrySnapshot
            && json_string_at(&event.payload, &["executionRegistry"]) == Some("tool_registry_v2")
    });
    let every_tool_event_has_v2_dispatch = trace
        .snapshot
        .events
        .iter()
        .filter(|event| {
            matches!(
                event.event_kind,
                RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolCompleted
            )
        })
        .all(|event| json_has_tool_registry_v2_dispatch(&event.payload));

    if has_registry_snapshot && every_tool_event_has_v2_dispatch {
        return gate(
            trace,
            TraceQualityGateCategory::ToolRegistryV2Execution,
            TraceQualityGateStatus::Pass,
            None,
            "tool_registry_v2_execution",
            "Tool execution is represented as ToolRegistryV2 dispatch.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::ToolRegistryV2Execution,
        TraceQualityGateStatus::Fail,
        Some(tool_event.id),
        "tool_registry_v2_execution",
        "Tool execution appeared without ToolRegistryV2 dispatch metadata.",
    )
}

fn subprocess_sandbox_metadata_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    let Some(event) = trace
        .snapshot
        .events
        .iter()
        .find(|event| subprocess_event_requires_sandbox_metadata(&event.payload))
    else {
        return gate(
            trace,
            TraceQualityGateCategory::SubprocessSandboxMetadata,
            TraceQualityGateStatus::Pass,
            None,
            "subprocess_sandbox_metadata",
            "No subprocess or external-agent execution required sandbox metadata.",
        );
    };

    if json_has_sandbox_metadata(&event.payload) {
        return gate(
            trace,
            TraceQualityGateCategory::SubprocessSandboxMetadata,
            TraceQualityGateStatus::Pass,
            None,
            "subprocess_sandbox_metadata",
            "Subprocess execution includes sandbox metadata.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::SubprocessSandboxMetadata,
        TraceQualityGateStatus::Fail,
        Some(event.id),
        "subprocess_sandbox_metadata",
        "Subprocess or external-agent execution appeared without sandbox metadata.",
    )
}

fn tool_timeout_metadata_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if let Some(event) = trace.snapshot.events.iter().find(|event| {
        json_contains_timeout_signal(&event.payload)
            && !json_has_timeout_cleanup_metadata(&event.payload)
    }) {
        return gate(
            trace,
            TraceQualityGateCategory::ToolTimeoutMetadata,
            TraceQualityGateStatus::Fail,
            Some(event.id),
            "tool_timeout_metadata",
            "Timeout appeared without deadline, budget, cancellation, or cleanup metadata.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::ToolTimeoutMetadata,
        TraceQualityGateStatus::Pass,
        None,
        "tool_timeout_metadata",
        "Timeout signals include enough metadata to prove cancellation and cleanup.",
    )
}

fn provider_history_replay_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if let Some(message) = trace.snapshot.messages.iter().find(|message| {
        message.role == MessageRole::Tool && !tool_message_has_replay_metadata(message)
    }) {
        return gate(
            trace,
            TraceQualityGateCategory::ProviderHistoryReplay,
            TraceQualityGateStatus::Fail,
            None,
            "provider_history_replay",
            format!(
                "Tool result message #{} is missing replayable tool-call protocol metadata.",
                message.id
            ),
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::ProviderHistoryReplay,
        TraceQualityGateStatus::Pass,
        None,
        "provider_history_replay",
        "Persisted tool result messages carry replayable provider tool-call ids.",
    )
}

fn workspace_index_lifecycle_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    if !trace_requires_semantic_workspace_index(trace) {
        return gate(
            trace,
            TraceQualityGateCategory::WorkspaceIndexLifecycle,
            TraceQualityGateStatus::Pass,
            None,
            "workspace_index_lifecycle",
            "Trace did not declare semantic workspace index as required.",
        );
    }

    if trace_has_ready_semantic_workspace_index(trace) {
        return gate(
            trace,
            TraceQualityGateCategory::WorkspaceIndexLifecycle,
            TraceQualityGateStatus::Pass,
            None,
            "workspace_index_lifecycle",
            "Required semantic workspace index readiness is represented in lifecycle events.",
        );
    }

    gate(
        trace,
        TraceQualityGateCategory::WorkspaceIndexLifecycle,
        TraceQualityGateStatus::Fail,
        trace.events.first().map(|event| event.event_id),
        "workspace_index_lifecycle",
        "Semantic workspace index was required but no ready lifecycle health evidence appeared.",
    )
}

fn tool_schema_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    for event in &trace.events {
        match &event.payload {
            RuntimeProtocolEventPayload::ToolStarted {
                tool_call_id,
                tool_name,
            } if tool_call_id.trim().is_empty() || tool_name.trim().is_empty() => {
                return gate(
                    trace,
                    TraceQualityGateCategory::ToolSchemaValidation,
                    TraceQualityGateStatus::Fail,
                    Some(event.event_id),
                    "tool_schema",
                    "Tool-started event was missing a tool call id or tool name.",
                );
            }
            RuntimeProtocolEventPayload::ToolRegistrySnapshot {
                tool_count,
                tool_names,
            } if *tool_count != tool_names.len() => {
                return gate(
                    trace,
                    TraceQualityGateCategory::ToolSchemaValidation,
                    TraceQualityGateStatus::Fail,
                    Some(event.event_id),
                    "tool_schema",
                    "Tool registry snapshot count did not match the listed tool names.",
                );
            }
            _ => {}
        }
    }
    gate(
        trace,
        TraceQualityGateCategory::ToolSchemaValidation,
        TraceQualityGateStatus::Pass,
        None,
        "tool_schema",
        "Tool call and registry events have stable identifiers.",
    )
}

fn context_manifest_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    let mut by_turn = BTreeMap::<usize, String>::new();
    for manifest in &trace.snapshot.context_manifests {
        if let Some(previous) = by_turn.insert(manifest.turn_index, manifest.context_hash.clone()) {
            if previous != manifest.context_hash {
                return gate(
                    trace,
                    TraceQualityGateCategory::ContextManifestDeterminism,
                    TraceQualityGateStatus::Fail,
                    None,
                    "context_manifest_determinism",
                    format!(
                        "Context manifest turn `{}` was recorded with more than one context hash.",
                        manifest.turn_index
                    ),
                );
            }
        }
    }

    for event in &trace.events {
        if let RuntimeProtocolEventPayload::ContextManifestRecorded {
            manifest_id,
            context_hash,
            turn_index,
        } = &event.payload
        {
            let matches_manifest = trace.snapshot.context_manifests.iter().any(|manifest| {
                &manifest.manifest_id == manifest_id
                    && &manifest.context_hash == context_hash
                    && &manifest.turn_index == turn_index
            });
            if !matches_manifest {
                return gate(
                    trace,
                    TraceQualityGateCategory::ContextManifestDeterminism,
                    TraceQualityGateStatus::Fail,
                    Some(event.event_id),
                    "context_manifest_determinism",
                    format!(
                        "Context manifest event `{manifest_id}` did not match the canonical manifest store."
                    ),
                );
            }
        }
    }

    gate(
        trace,
        TraceQualityGateCategory::ContextManifestDeterminism,
        TraceQualityGateStatus::Pass,
        None,
        "context_manifest_determinism",
        "Context manifest hashes are deterministic for each provider turn.",
    )
}

fn context_manifest_before_provider_turn_gate(trace: &RuntimeTrace) -> TraceQualityGateResult {
    let provider_events = trace
        .events
        .iter()
        .filter(|event| is_provider_turn_event(event))
        .collect::<Vec<_>>();
    if provider_events.is_empty() {
        return gate(
            trace,
            TraceQualityGateCategory::ContextManifestBeforeProviderTurn,
            TraceQualityGateStatus::Pass,
            None,
            "context_manifest_before_provider_turn",
            "No provider-turn event appeared in the trace.",
        );
    }

    if trace.snapshot.context_manifests.is_empty() {
        return gate(
            trace,
            TraceQualityGateCategory::ContextManifestBeforeProviderTurn,
            TraceQualityGateStatus::Fail,
            provider_events.first().map(|event| event.event_id),
            "context_manifest_before_provider_turn",
            "Provider-turn events appeared before any context manifest was recorded.",
        );
    }

    for event in provider_events {
        let Some(provider_turn_trace_id) = event.trace.provider_turn_trace_id.as_deref() else {
            continue;
        };
        let Some(manifest) = trace.snapshot.context_manifests.iter().find(|manifest| {
            manifest.trace.provider_turn_trace_id.as_deref() == Some(provider_turn_trace_id)
        }) else {
            return gate(
                trace,
                TraceQualityGateCategory::ContextManifestBeforeProviderTurn,
                TraceQualityGateStatus::Fail,
                Some(event.event_id),
                "context_manifest_before_provider_turn",
                "A provider-turn event did not have a matching context manifest for the same provider turn.",
            );
        };
        if manifest
            .recorded_after_event_id
            .is_some_and(|event_id| event_id >= event.event_id)
        {
            return gate(
                trace,
                TraceQualityGateCategory::ContextManifestBeforeProviderTurn,
                TraceQualityGateStatus::Fail,
                Some(event.event_id),
                "context_manifest_before_provider_turn",
                "A provider-turn event was recorded before its context manifest boundary.",
            );
        }
    }

    gate(
        trace,
        TraceQualityGateCategory::ContextManifestBeforeProviderTurn,
        TraceQualityGateStatus::Pass,
        None,
        "context_manifest_before_provider_turn",
        "Provider-turn events have context manifests recorded first.",
    )
}

fn event_protocol_gate(
    trace: &RuntimeTrace,
    timeline: &ReplayedRunTimeline,
) -> TraceQualityGateResult {
    if let Some(segment) = timeline.corrupt_segments.first() {
        return gate(
            trace,
            TraceQualityGateCategory::EventProtocolSchemaSnapshot,
            TraceQualityGateStatus::Fail,
            Some(segment.event_id),
            "event_protocol_schema",
            segment.summary.clone(),
        );
    }
    if let Some(segment) = timeline.missing_segments.first() {
        return gate(
            trace,
            TraceQualityGateCategory::EventProtocolSchemaSnapshot,
            TraceQualityGateStatus::Fail,
            Some(segment.start_event_id),
            "event_protocol_schema",
            segment.summary.clone(),
        );
    }
    if let Some(event) = trace.events.iter().find(|event| !event.trace.is_valid()) {
        return gate(
            trace,
            TraceQualityGateCategory::EventProtocolSchemaSnapshot,
            TraceQualityGateStatus::Fail,
            Some(event.event_id),
            "event_protocol_schema",
            "Runtime event trace context was malformed.",
        );
    }
    gate(
        trace,
        TraceQualityGateCategory::EventProtocolSchemaSnapshot,
        TraceQualityGateStatus::Pass,
        None,
        "event_protocol_schema",
        "Protocol event schema and trace ids are valid.",
    )
}

fn support_bundle_redaction_gate(
    trace: &RuntimeTrace,
    redaction_report: &TraceRedactionReport,
    redaction_failed: bool,
) -> TraceQualityGateResult {
    if redaction_failed {
        return gate(
            trace,
            TraceQualityGateCategory::SupportBundleRedaction,
            TraceQualityGateStatus::Fail,
            None,
            "support_bundle_redaction",
            "The redacted support bundle still contains a secret-like value.",
        );
    }
    gate(
        trace,
        TraceQualityGateCategory::SupportBundleRedaction,
        TraceQualityGateStatus::Pass,
        None,
        "support_bundle_redaction",
        format!(
            "Support bundle redaction is applied by default and redacted {} value(s).",
            redaction_report.redacted_value_count
        ),
    )
}

fn redacted_support_value(value: &JsonValue) -> (JsonValue, TraceRedactionReport) {
    let mut state = TraceRedactionState::default();
    let redacted = redact_json_value(value, None, false, &mut state);
    let mut categories = state.categories.into_iter().collect::<Vec<_>>();
    categories.sort();
    (
        redacted,
        TraceRedactionReport {
            applied_by_default: true,
            redacted_value_count: state.redacted_value_count,
            secret_like_value_count: state.secret_like_value_count,
            categories,
        },
    )
}

#[derive(Debug, Default)]
struct TraceRedactionState {
    redacted_value_count: usize,
    secret_like_value_count: usize,
    categories: BTreeSet<TraceRedactionCategory>,
}

fn redact_json_value(
    value: &JsonValue,
    key: Option<&str>,
    parent_unapproved_memory: bool,
    state: &mut TraceRedactionState,
) -> JsonValue {
    if let Some(category) = redaction_category_for_key(key, parent_unapproved_memory) {
        if value.is_string()
            && value
                .as_str()
                .is_some_and(|text| secret_like_string_category(text).is_some())
        {
            state.secret_like_value_count += 1;
        }
        state.redacted_value_count += 1;
        state.categories.insert(category);
        return JsonValue::String(format!("[REDACTED:{}]", category.marker()));
    }

    match value {
        JsonValue::Array(items) => JsonValue::Array(
            items
                .iter()
                .map(|item| redact_json_value(item, None, parent_unapproved_memory, state))
                .collect(),
        ),
        JsonValue::Object(map) => {
            let object_unapproved = parent_unapproved_memory || is_unapproved_memory_object(value);
            let redacted = map
                .iter()
                .map(|(child_key, child_value)| {
                    (
                        child_key.clone(),
                        redact_json_value(child_value, Some(child_key), object_unapproved, state),
                    )
                })
                .collect();
            JsonValue::Object(redacted)
        }
        JsonValue::String(text) => {
            if let Some(category) = secret_like_string_category(text) {
                state.secret_like_value_count += 1;
                state.redacted_value_count += 1;
                state.categories.insert(category);
                JsonValue::String(format!("[REDACTED:{}]", category.marker()))
            } else {
                JsonValue::String(text.clone())
            }
        }
        _ => value.clone(),
    }
}

fn redaction_category_for_key(
    key: Option<&str>,
    parent_unapproved_memory: bool,
) -> Option<TraceRedactionCategory> {
    let key = key?;
    let normalized = key.to_ascii_lowercase().replace(['_', '-'], "");
    if parent_unapproved_memory
        && matches!(
            normalized.as_str(),
            "text" | "content" | "memorytext" | "rawtext" | "summary"
        )
    {
        return Some(TraceRedactionCategory::UnapprovedMemoryText);
    }
    if normalized.contains("authorization") || normalized.contains("bearer") {
        return Some(TraceRedactionCategory::BearerHeader);
    }
    if normalized.contains("oauth")
        || normalized.contains("accesstoken")
        || normalized.contains("refreshtoken")
        || normalized.ends_with("token")
    {
        return Some(TraceRedactionCategory::OAuthToken);
    }
    if normalized.contains("privatekeypath") || normalized.contains("sshkeypath") {
        return Some(TraceRedactionCategory::PrivateKeyPath);
    }
    if normalized.contains("credentialpath") || normalized.contains("serviceaccountpath") {
        return Some(TraceRedactionCategory::CredentialPath);
    }
    if normalized.contains("secret")
        || normalized.contains("apikey")
        || normalized.contains("password")
        || normalized.contains("credential")
    {
        return Some(TraceRedactionCategory::Secret);
    }
    if matches!(
        normalized.as_str(),
        "content" | "text" | "delta" | "prompt" | "systemprompt" | "rawtranscript"
    ) {
        return Some(TraceRedactionCategory::RawTranscriptText);
    }
    if normalized.contains("filecontents")
        || normalized.contains("rawfile")
        || normalized.contains("rawcontent")
    {
        return Some(TraceRedactionCategory::RawFileContents);
    }
    None
}

fn is_unapproved_memory_object(value: &JsonValue) -> bool {
    let JsonValue::Object(map) = value else {
        return false;
    };
    let approved = map
        .get("approved")
        .or_else(|| map.get("memoryApproved"))
        .and_then(JsonValue::as_bool);
    approved == Some(false)
        && (map.contains_key("memoryText")
            || map.contains_key("text")
            || map.contains_key("content"))
}

fn contains_unredacted_secret_like_value(value: &JsonValue) -> bool {
    match value {
        JsonValue::Array(items) => items.iter().any(contains_unredacted_secret_like_value),
        JsonValue::Object(map) => map.values().any(contains_unredacted_secret_like_value),
        JsonValue::String(text) => {
            !text.starts_with("[REDACTED:") && secret_like_string_category(text).is_some()
        }
        _ => false,
    }
}

fn secret_like_string_category(text: &str) -> Option<TraceRedactionCategory> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("bearer ") {
        return Some(TraceRedactionCategory::BearerHeader);
    }
    if looks_like_secret_url(&lower) {
        return Some(TraceRedactionCategory::SecretUrl);
    }
    if lower.contains("/.ssh/")
        || lower.ends_with("/id_rsa")
        || lower.ends_with("/id_ed25519")
        || lower.ends_with(".pem")
        || lower.ends_with(".p8")
        || lower.contains("private_key")
    {
        return Some(TraceRedactionCategory::PrivateKeyPath);
    }
    if lower.contains("credentials.json")
        || lower.contains("service-account")
        || lower.contains("service_account")
        || lower.contains("application_default_credentials")
        || lower.contains("/.aws/credentials")
        || lower.ends_with("\\.aws\\credentials")
        || lower.contains("/.config/gcloud/")
        || lower.contains("\\appdata\\roaming\\gcloud\\")
        || lower.contains("/.azure/")
        || lower.contains("\\.azure\\")
        || lower.contains("/.kube/config")
        || lower.ends_with("\\.kube\\config")
    {
        return Some(TraceRedactionCategory::CredentialPath);
    }
    if lower.contains("api_key=")
        || lower.contains("apikey=")
        || lower.contains("access_token=")
        || lower.contains("refresh_token=")
        || lower.contains("client_secret=")
    {
        return Some(TraceRedactionCategory::Secret);
    }
    None
}

fn looks_like_secret_url(lower: &str) -> bool {
    (lower.starts_with("http://") || lower.starts_with("https://"))
        && (lower.contains("token=")
            || lower.contains("api_key=")
            || lower.contains("apikey=")
            || lower.contains("key=")
            || lower.contains("secret=")
            || lower.contains("access_token="))
}

fn text_indicates_failure(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("fail")
        || value.contains("error")
        || value.contains("denied")
        || value.contains("timeout")
}

fn text_indicates_denial(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.contains("deny")
        || value.contains("denied")
        || value.contains("reject")
        || value.contains("blocked")
        || value.contains("not_allowed")
}

fn classify_failure_layer(code: &str, message: &str) -> TraceFailureLayer {
    let text = format!("{code} {message}").to_ascii_lowercase();
    if text.contains("provider") || text.contains("rate_limit") || text.contains("model") {
        TraceFailureLayer::Provider
    } else if text.contains("lifecycle") || text.contains("environment") || text.contains("health")
    {
        TraceFailureLayer::Lifecycle
    } else if text.contains("context") || text.contains("manifest") || text.contains("compact") {
        TraceFailureLayer::ContextAssembly
    } else if text.contains("workspace_index") || text.contains("semantic index") {
        TraceFailureLayer::WorkspaceIndex
    } else if text.contains("retrieval") || text.contains("lance") || text.contains("index") {
        TraceFailureLayer::Retrieval
    } else if text.contains("tool dispatch") || text.contains("registry") {
        TraceFailureLayer::ToolDispatch
    } else if text.contains("tool") || text.contains("schema") {
        TraceFailureLayer::ToolSchema
    } else if text.contains("approval") || text.contains("action") {
        TraceFailureLayer::Approval
    } else if text.contains("sandbox") || text.contains("policy") {
        TraceFailureLayer::Sandbox
    } else if text.contains("file") || text.contains("fs") || text.contains("path") {
        TraceFailureLayer::Filesystem
    } else if text.contains("timeout") || text.contains("deadline") || text.contains("cleanup") {
        TraceFailureLayer::Timeout
    } else if text.contains("history") || text.contains("replay") || text.contains("tool_call_id") {
        TraceFailureLayer::ProviderHistory
    } else if text.contains("validation") || text.contains("verification") || text.contains("test")
    {
        TraceFailureLayer::Verification
    } else if text.contains("storage") || text.contains("sqlite") || text.contains("database") {
        TraceFailureLayer::Storage
    } else if text.contains("redact") {
        TraceFailureLayer::Redaction
    } else if text.contains("protocol") || text.contains("trace") {
        TraceFailureLayer::Protocol
    } else {
        TraceFailureLayer::Runtime
    }
}

fn json_contains_provider_retry(value: &JsonValue) -> bool {
    json_contains_any(
        value,
        &[
            "retry",
            "rate_limit",
            "rate limit",
            "response_shape",
            "malformed response",
        ],
    )
}

fn json_contains_storage_error(value: &JsonValue) -> bool {
    json_contains_any(
        value,
        &["storage_error", "storage write", "sqlite", "database error"],
    )
}

fn json_contains_prompt_injection(value: &JsonValue) -> bool {
    json_contains_any(
        value,
        &[
            "ignore previous instructions",
            "ignore all previous",
            "system prompt",
            "developer message",
            "exfiltrate",
        ],
    )
}

fn json_contains_dangerous_unrestricted_without_approval(value: &JsonValue) -> bool {
    json_contains_any(value, &["dangerous_unrestricted"])
        && !json_contains_any(value, &["operator", "approved", "approval"])
}

fn provider_preflight_value(value: &JsonValue) -> Option<&JsonValue> {
    value
        .get("providerPreflight")
        .or_else(|| value.get("provider_preflight"))
        .filter(|value| value.is_object())
}

fn provider_preflight_value_is_not_live_ready(value: &JsonValue) -> bool {
    let source = text_field(value, "source").unwrap_or_default();
    let status = text_field(value, "status").unwrap_or_default();
    let stale = value
        .get("stale")
        .and_then(JsonValue::as_bool)
        .unwrap_or(false);
    stale
        || matches!(source.as_str(), "static_manual" | "unavailable" | "")
        || matches!(status.as_str(), "failed" | "warning" | "skipped" | "")
}

fn admitted_provider_preflight_value(trace: &RuntimeTrace) -> Option<&JsonValue> {
    trace
        .snapshot
        .events
        .iter()
        .find_map(|event| provider_preflight_value(&event.payload))
}

fn stable_json_fingerprint(value: &JsonValue) -> String {
    let serialized = serde_json::to_string(value).unwrap_or_else(|_| "unserializable".into());
    runtime_trace_id("json", &[&serialized])
}

fn is_real_owned_provider_trace(trace: &RuntimeTrace) -> bool {
    let provider_id = trace.snapshot.provider_id.as_str();
    provider_id != "fake_provider" && !provider_id.starts_with("external_")
}

fn json_contains_headless_file_store_signal(value: &JsonValue) -> bool {
    json_contains_any(
        value,
        &[
            "headless_real_provider",
            "headless_file_store",
            "file_agent_core_store",
            "agent-core-runs.json",
        ],
    )
}

fn json_string_at<'a>(value: &'a JsonValue, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    current.as_str()
}

fn json_has_tool_registry_v2_dispatch(value: &JsonValue) -> bool {
    json_string_at(value, &["dispatch", "registryVersion"]) == Some("tool_registry_v2")
        || json_string_at(value, &["dispatch", "registry"]) == Some("tool_registry_v2")
        || json_string_at(value, &["dispatchRegistry"]) == Some("tool_registry_v2")
        || json_string_at(value, &["executionRegistry"]) == Some("tool_registry_v2")
}

fn json_contains_sandbox_denial_success_shape(value: &JsonValue) -> bool {
    match value {
        JsonValue::Array(items) => items.iter().any(json_contains_sandbox_denial_success_shape),
        JsonValue::Object(map) => {
            (json_has_sandbox_denial_metadata(value) && json_is_success_shaped(value))
                || map.values().any(json_contains_sandbox_denial_success_shape)
        }
        _ => false,
    }
}

fn json_has_sandbox_denial_metadata(value: &JsonValue) -> bool {
    json_contains_any(value, &["sandbox"])
        && json_contains_any(
            value,
            &["denied_by_sandbox", "deniedbysandbox", "denied by sandbox"],
        )
}

fn json_is_success_shaped(value: &JsonValue) -> bool {
    value.get("ok").and_then(JsonValue::as_bool) == Some(true)
        || ["status", "state", "outcome", "result"].iter().any(|key| {
            value
                .get(*key)
                .and_then(JsonValue::as_str)
                .is_some_and(|text| {
                    matches!(
                        text.to_ascii_lowercase().as_str(),
                        "ok" | "success" | "succeeded" | "completed" | "passed"
                    )
                })
        })
}

fn subprocess_event_requires_sandbox_metadata(value: &JsonValue) -> bool {
    let tool_name = text_field(value, "toolName")
        .or_else(|| text_field(value, "tool_name"))
        .unwrap_or_default()
        .to_ascii_lowercase();
    matches!(
        tool_name.as_str(),
        "command"
            | "powershell"
            | "command_session_start"
            | "process_manager"
            | "subagent"
            | "system_diagnostics"
    ) || json_contains_any(
        value,
        &[
            "subprocess",
            "external_agent",
            "external-agent",
            "process spawn",
            "command execution",
        ],
    )
}

fn json_has_sandbox_metadata(value: &JsonValue) -> bool {
    value
        .get("sandbox")
        .is_some_and(|sandbox| sandbox.is_object())
        || value
            .get("dispatch")
            .and_then(|dispatch| dispatch.get("sandbox"))
            .is_some_and(|sandbox| sandbox.is_object())
}

fn json_contains_timeout_signal(value: &JsonValue) -> bool {
    json_contains_any(value, &["timeout", "timed_out", "deadline"])
}

fn json_has_timeout_cleanup_metadata(value: &JsonValue) -> bool {
    json_contains_any(
        value,
        &[
            "timeoutms",
            "timeout_ms",
            "deadline",
            "budget",
            "cleanup",
            "cleanedup",
            "killed",
            "cancelled",
            "canceled",
            "processgroup",
            "wallclock",
            "elapsedms",
        ],
    )
}

fn tool_message_has_replay_metadata(message: &crate::RuntimeMessage) -> bool {
    if let Some(tool_result) = message
        .provider_metadata
        .as_ref()
        .and_then(|metadata| metadata.tool_result.as_ref())
    {
        return !tool_result.tool_call_id.trim().is_empty()
            && !tool_result.provider_tool_name.trim().is_empty()
            && !tool_result.parent_assistant_message_id.trim().is_empty();
    }

    let Ok(value) = serde_json::from_str::<JsonValue>(&message.content) else {
        return false;
    };
    let has_tool_call_id = text_field(&value, "toolCallId")
        .or_else(|| text_field(&value, "tool_call_id"))
        .is_some();
    let has_tool_name = text_field(&value, "providerToolName")
        .or_else(|| text_field(&value, "toolName"))
        .or_else(|| text_field(&value, "tool_name"))
        .is_some();
    let has_parent_assistant = text_field(&value, "parentAssistantMessageId")
        .or_else(|| text_field(&value, "assistantMessageId"))
        .or_else(|| text_field(&value, "assistantToolCallId"))
        .is_some();
    has_tool_call_id && has_tool_name && has_parent_assistant
}

fn trace_requires_semantic_workspace_index(trace: &RuntimeTrace) -> bool {
    trace
        .snapshot
        .events
        .iter()
        .any(|event| json_bool_key_is_true(&event.payload, &semantic_index_required_keys()))
        || trace.snapshot.context_manifests.iter().any(|manifest| {
            json_bool_key_is_true(&manifest.manifest, &semantic_index_required_keys())
        })
}

fn trace_has_ready_semantic_workspace_index(trace: &RuntimeTrace) -> bool {
    trace
        .snapshot
        .events
        .iter()
        .filter(|event| event.event_kind == RuntimeEventKind::EnvironmentLifecycleUpdate)
        .any(|event| {
            json_bool_key_is_true(&event.payload, &semantic_index_available_keys())
                || (json_contains_any(&event.payload, &["semantic", "workspace", "index"])
                    && json_contains_any(&event.payload, &["ready", "passed", "available"]))
        })
}

fn semantic_index_required_keys() -> [&'static str; 3] {
    [
        "semanticIndexRequired",
        "semantic_index_required",
        "workspaceIndexRequired",
    ]
}

fn semantic_index_available_keys() -> [&'static str; 3] {
    [
        "semanticIndexAvailable",
        "semantic_index_available",
        "workspaceIndexReady",
    ]
}

fn json_bool_key_is_true(value: &JsonValue, keys: &[&str]) -> bool {
    match value {
        JsonValue::Array(items) => items.iter().any(|item| json_bool_key_is_true(item, keys)),
        JsonValue::Object(map) => map.iter().any(|(key, value)| {
            (keys.iter().any(|candidate| key == candidate) && value.as_bool() == Some(true))
                || json_bool_key_is_true(value, keys)
        }),
        _ => false,
    }
}

fn is_provider_turn_event(event: &RuntimeProtocolEvent) -> bool {
    matches!(
        &event.payload,
        RuntimeProtocolEventPayload::MessageDelta {
            role: MessageRole::Assistant,
            ..
        } | RuntimeProtocolEventPayload::ReasoningSummary { .. }
            | RuntimeProtocolEventPayload::ToolDelta { .. }
            | RuntimeProtocolEventPayload::ToolStarted { .. }
            | RuntimeProtocolEventPayload::ToolCompleted { .. }
    )
}

fn json_contains_any(value: &JsonValue, needles: &[&str]) -> bool {
    match value {
        JsonValue::Array(items) => items.iter().any(|item| json_contains_any(item, needles)),
        JsonValue::Object(map) => map.iter().any(|(key, value)| {
            contains_any_lower(key, needles) || json_contains_any(value, needles)
        }),
        JsonValue::String(text) => contains_any_lower(text, needles),
        _ => false,
    }
}

fn contains_any_lower(value: &str, needles: &[&str]) -> bool {
    let value = value.to_ascii_lowercase();
    needles.iter().any(|needle| value.contains(needle))
}

impl RuntimeEvent {
    pub fn to_protocol_event(&self) -> CoreResult<RuntimeProtocolEvent> {
        ensure_runtime_protocol_version(CORE_PROTOCOL_VERSION)?;
        if !self.trace.is_valid() {
            return Err(CoreError::system_fault(
                "agent_protocol_trace_invalid",
                format!(
                    "Runtime event `{}` in run `{}` has an invalid trace context.",
                    self.id, self.run_id
                ),
            ));
        }
        Ok(RuntimeProtocolEvent {
            protocol_version: CORE_PROTOCOL_VERSION,
            event_id: self.id,
            project_id: self.project_id.clone(),
            run_id: self.run_id.clone(),
            event_kind: self.event_kind.clone(),
            trace: self.trace.clone(),
            payload: runtime_protocol_payload_from_json(&self.event_kind, &self.payload),
            occurred_at: self.created_at.clone(),
        })
    }
}

pub fn ensure_runtime_protocol_version(version: u32) -> CoreResult<()> {
    if version != CORE_PROTOCOL_VERSION {
        return Err(CoreError::invalid_request(
            "agent_protocol_version_mismatch",
            format!(
                "Xero agent protocol version `{version}` is not supported by this runtime; expected `{CORE_PROTOCOL_VERSION}`."
            ),
        ));
    }
    Ok(())
}

pub fn runtime_trace_id_for_run(project_id: &str, run_id: &str) -> String {
    runtime_trace_id("run", &[project_id, run_id])
}

pub fn runtime_trace_id(scope: &str, parts: &[&str]) -> String {
    format!(
        "{:016x}{:016x}",
        stable_hash(0xaf63bd4c8601b7df, scope, parts),
        stable_hash(0x9e3779b97f4a7c15, scope, parts)
    )
}

pub fn runtime_span_id(scope: &str, parts: &[&str]) -> String {
    format!("{:016x}", stable_hash(0xcbf29ce484222325, scope, parts))
}

fn runtime_protocol_payload_from_json(
    event_kind: &RuntimeEventKind,
    payload: &JsonValue,
) -> RuntimeProtocolEventPayload {
    match event_kind {
        RuntimeEventKind::RunStarted => RuntimeProtocolEventPayload::RunStarted {
            status: text_field(payload, "status")
                .as_deref()
                .and_then(run_status_from_wire)
                .unwrap_or(RunStatus::Running),
            provider_id: text_field(payload, "providerId").unwrap_or_default(),
            model_id: text_field(payload, "modelId").unwrap_or_default(),
        },
        RuntimeEventKind::MessageDelta => RuntimeProtocolEventPayload::MessageDelta {
            role: role_field(payload, "role").unwrap_or(MessageRole::Assistant),
            text: text_field(payload, "text")
                .or_else(|| text_field(payload, "delta"))
                .unwrap_or_default(),
        },
        RuntimeEventKind::ReasoningSummary => RuntimeProtocolEventPayload::ReasoningSummary {
            text: text_field(payload, "text").unwrap_or_default(),
        },
        RuntimeEventKind::ToolStarted => RuntimeProtocolEventPayload::ToolStarted {
            tool_call_id: text_field(payload, "toolCallId")
                .or_else(|| text_field(payload, "tool_call_id"))
                .unwrap_or_default(),
            tool_name: text_field(payload, "toolName")
                .or_else(|| text_field(payload, "tool_name"))
                .unwrap_or_default(),
        },
        RuntimeEventKind::ToolDelta => RuntimeProtocolEventPayload::ToolDelta {
            tool_call_id: text_field(payload, "toolCallId")
                .or_else(|| text_field(payload, "tool_call_id"))
                .unwrap_or_default(),
            text: text_field(payload, "text").unwrap_or_default(),
        },
        RuntimeEventKind::ToolCompleted => RuntimeProtocolEventPayload::ToolCompleted {
            tool_call_id: text_field(payload, "toolCallId")
                .or_else(|| text_field(payload, "tool_call_id"))
                .unwrap_or_default(),
            outcome: text_field(payload, "outcome")
                .or_else(|| text_field(payload, "status"))
                .unwrap_or_else(|| "completed".into()),
        },
        RuntimeEventKind::PolicyDecision => RuntimeProtocolEventPayload::PolicyDecision {
            subject: text_field(payload, "subject")
                .or_else(|| text_field(payload, "kind"))
                .unwrap_or_else(|| "runtime_policy".into()),
            decision: text_field(payload, "decision").unwrap_or_else(|| "unknown".into()),
            reason: text_field(payload, "reason"),
        },
        RuntimeEventKind::StateTransition => RuntimeProtocolEventPayload::StateTransition {
            from: text_field(payload, "from").unwrap_or_default(),
            to: text_field(payload, "to").unwrap_or_default(),
            reason: text_field(payload, "reason"),
        },
        RuntimeEventKind::ActionRequired | RuntimeEventKind::ApprovalRequired => {
            RuntimeProtocolEventPayload::ApprovalRequired {
                action_id: text_field(payload, "actionId")
                    .or_else(|| text_field(payload, "action_id"))
                    .unwrap_or_default(),
                boundary_id: text_field(payload, "boundaryId")
                    .or_else(|| text_field(payload, "boundary_id")),
                action_type: text_field(payload, "actionType")
                    .or_else(|| text_field(payload, "action_type"))
                    .unwrap_or_else(|| "approval".into()),
                title: text_field(payload, "title").unwrap_or_else(|| "Approval required".into()),
                detail: text_field(payload, "detail").unwrap_or_default(),
            }
        }
        RuntimeEventKind::PlanUpdated => RuntimeProtocolEventPayload::PlanUpdated {
            summary: text_field(payload, "summary"),
            items: payload
                .get("items")
                .and_then(JsonValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .enumerate()
                        .map(|(index, item)| RuntimePlanItem {
                            id: text_field(item, "id").unwrap_or_else(|| format!("item-{index}")),
                            text: text_field(item, "text").unwrap_or_default(),
                            status: text_field(item, "status").unwrap_or_else(|| "pending".into()),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        },
        RuntimeEventKind::VerificationGate => RuntimeProtocolEventPayload::VerificationGate {
            status: text_field(payload, "status").unwrap_or_else(|| "pending".into()),
            summary: text_field(payload, "summary"),
        },
        RuntimeEventKind::ContextManifestRecorded => {
            RuntimeProtocolEventPayload::ContextManifestRecorded {
                manifest_id: text_field(payload, "manifestId")
                    .or_else(|| text_field(payload, "manifest_id"))
                    .unwrap_or_default(),
                context_hash: text_field(payload, "contextHash")
                    .or_else(|| text_field(payload, "context_hash"))
                    .unwrap_or_default(),
                turn_index: payload
                    .get("turnIndex")
                    .or_else(|| payload.get("turn_index"))
                    .and_then(JsonValue::as_u64)
                    .unwrap_or_default() as usize,
            }
        }
        RuntimeEventKind::RetrievalPerformed => RuntimeProtocolEventPayload::RetrievalPerformed {
            query: text_field(payload, "query").unwrap_or_default(),
            result_count: payload
                .get("resultCount")
                .or_else(|| payload.get("result_count"))
                .and_then(JsonValue::as_u64)
                .unwrap_or_default() as usize,
            source: text_field(payload, "source"),
        },
        RuntimeEventKind::MemoryCandidateCaptured => {
            RuntimeProtocolEventPayload::MemoryCandidateCaptured {
                candidate_id: text_field(payload, "candidateId")
                    .or_else(|| text_field(payload, "candidate_id"))
                    .unwrap_or_default(),
                candidate_kind: text_field(payload, "candidateKind")
                    .or_else(|| text_field(payload, "candidate_kind"))
                    .unwrap_or_else(|| "project_fact".into()),
                confidence: payload
                    .get("confidence")
                    .and_then(JsonValue::as_u64)
                    .unwrap_or_default()
                    .min(100) as u8,
            }
        }
        RuntimeEventKind::EnvironmentLifecycleUpdate => {
            RuntimeProtocolEventPayload::EnvironmentLifecycleUpdate {
                environment_id: text_field(payload, "environmentId")
                    .or_else(|| text_field(payload, "environment_id"))
                    .unwrap_or_default(),
                state: text_field(payload, "state")
                    .as_deref()
                    .and_then(EnvironmentLifecycleState::from_wire)
                    .unwrap_or(EnvironmentLifecycleState::Created),
                previous_state: text_field(payload, "previousState")
                    .or_else(|| text_field(payload, "previous_state"))
                    .as_deref()
                    .and_then(EnvironmentLifecycleState::from_wire),
                sandbox_id: text_field(payload, "sandboxId")
                    .or_else(|| text_field(payload, "sandbox_id")),
                sandbox_grouping_policy: text_field(payload, "sandboxGroupingPolicy")
                    .or_else(|| text_field(payload, "sandbox_grouping_policy"))
                    .as_deref()
                    .and_then(SandboxGroupingPolicy::from_wire)
                    .unwrap_or(SandboxGroupingPolicy::None),
                pending_message_count: payload
                    .get("pendingMessageCount")
                    .or_else(|| payload.get("pending_message_count"))
                    .and_then(JsonValue::as_u64)
                    .unwrap_or_default() as usize,
                health_checks: payload
                    .get("healthChecks")
                    .or_else(|| payload.get("health_checks"))
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default(),
                setup_steps: payload
                    .get("setupSteps")
                    .or_else(|| payload.get("setup_steps"))
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default(),
                detail: text_field(payload, "detail"),
                diagnostic: payload
                    .get("diagnostic")
                    .cloned()
                    .and_then(|value| serde_json::from_value(value).ok()),
            }
        }
        RuntimeEventKind::SandboxLifecycleUpdate => {
            RuntimeProtocolEventPayload::SandboxLifecycleUpdate {
                sandbox_id: text_field(payload, "sandboxId")
                    .or_else(|| text_field(payload, "sandbox_id")),
                phase: text_field(payload, "phase").unwrap_or_else(|| "unknown".into()),
                detail: text_field(payload, "detail"),
            }
        }
        RuntimeEventKind::ValidationStarted => RuntimeProtocolEventPayload::ValidationStarted {
            label: text_field(payload, "label").unwrap_or_default(),
        },
        RuntimeEventKind::ValidationCompleted => RuntimeProtocolEventPayload::ValidationCompleted {
            label: text_field(payload, "label").unwrap_or_default(),
            outcome: text_field(payload, "outcome").unwrap_or_else(|| "unknown".into()),
        },
        RuntimeEventKind::ToolRegistrySnapshot => {
            let tool_names = payload
                .get("toolNames")
                .or_else(|| payload.get("tool_names"))
                .and_then(JsonValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            RuntimeProtocolEventPayload::ToolRegistrySnapshot {
                tool_count: payload
                    .get("toolCount")
                    .or_else(|| payload.get("tool_count"))
                    .and_then(JsonValue::as_u64)
                    .unwrap_or(tool_names.len() as u64) as usize,
                tool_names,
            }
        }
        RuntimeEventKind::FileChanged => RuntimeProtocolEventPayload::FileChanged {
            path: text_field(payload, "path").unwrap_or_default(),
            operation: text_field(payload, "operation").unwrap_or_else(|| "modified".into()),
        },
        RuntimeEventKind::CommandOutput => RuntimeProtocolEventPayload::CommandOutput {
            tool_call_id: text_field(payload, "toolCallId")
                .or_else(|| text_field(payload, "tool_call_id")),
            stream: text_field(payload, "stream").unwrap_or_else(|| "stdout".into()),
            text: text_field(payload, "text").unwrap_or_default(),
        },
        RuntimeEventKind::ToolPermissionGrant => RuntimeProtocolEventPayload::ToolPermissionGrant {
            grant_id: text_field(payload, "grantId")
                .or_else(|| text_field(payload, "grant_id"))
                .unwrap_or_default(),
            tool_name: text_field(payload, "toolName")
                .or_else(|| text_field(payload, "tool_name"))
                .unwrap_or_default(),
        },
        RuntimeEventKind::ProviderModelChanged => {
            RuntimeProtocolEventPayload::ProviderModelChanged {
                provider_id: text_field(payload, "providerId")
                    .or_else(|| text_field(payload, "provider_id"))
                    .unwrap_or_default(),
                model_id: text_field(payload, "modelId")
                    .or_else(|| text_field(payload, "model_id"))
                    .unwrap_or_default(),
            }
        }
        RuntimeEventKind::RuntimeSettingsChanged => {
            RuntimeProtocolEventPayload::RuntimeSettingsChanged {
                summary: text_field(payload, "summary").unwrap_or_default(),
            }
        }
        RuntimeEventKind::RunPaused => RuntimeProtocolEventPayload::RunPaused {
            reason: text_field(payload, "reason"),
        },
        RuntimeEventKind::RunCompleted => RuntimeProtocolEventPayload::RunCompleted {
            summary: text_field(payload, "summary").unwrap_or_default(),
            state: text_field(payload, "state").unwrap_or_else(|| "complete".into()),
        },
        RuntimeEventKind::RunFailed => RuntimeProtocolEventPayload::RunFailed {
            code: text_field(payload, "code").unwrap_or_else(|| "agent_run_failed".into()),
            message: text_field(payload, "message").unwrap_or_default(),
            retryable: payload
                .get("retryable")
                .and_then(JsonValue::as_bool)
                .unwrap_or(false),
        },
        RuntimeEventKind::SubagentLifecycle => RuntimeProtocolEventPayload::Untyped {
            event_kind: RuntimeEventKind::SubagentLifecycle,
            payload: payload.clone(),
        },
    }
}

fn replay_label(payload: &RuntimeProtocolEventPayload) -> String {
    match payload {
        RuntimeProtocolEventPayload::RunStarted { .. } => "Run started".into(),
        RuntimeProtocolEventPayload::RunCompleted { .. } => "Run completed".into(),
        RuntimeProtocolEventPayload::RunFailed { .. } => "Run failed".into(),
        RuntimeProtocolEventPayload::StateTransition { to, .. } => format!("State changed to {to}"),
        RuntimeProtocolEventPayload::MessageDelta { role, .. } => {
            format!("{role:?} message delta")
        }
        RuntimeProtocolEventPayload::ReasoningSummary { .. } => "Reasoning summary".into(),
        RuntimeProtocolEventPayload::ToolStarted { tool_name, .. } => {
            format!("Tool started: {tool_name}")
        }
        RuntimeProtocolEventPayload::ToolDelta { .. } => "Tool delta".into(),
        RuntimeProtocolEventPayload::ToolCompleted { .. } => "Tool completed".into(),
        RuntimeProtocolEventPayload::PolicyDecision { decision, .. } => {
            format!("Policy decision: {decision}")
        }
        RuntimeProtocolEventPayload::ApprovalRequired { title, .. } => title.clone(),
        RuntimeProtocolEventPayload::PlanUpdated { .. } => "Plan updated".into(),
        RuntimeProtocolEventPayload::VerificationGate { status, .. } => {
            format!("Verification gate: {status}")
        }
        RuntimeProtocolEventPayload::ContextManifestRecorded { manifest_id, .. } => {
            format!("Context manifest recorded: {manifest_id}")
        }
        RuntimeProtocolEventPayload::RetrievalPerformed { result_count, .. } => {
            format!("Retrieval performed: {result_count} results")
        }
        RuntimeProtocolEventPayload::MemoryCandidateCaptured { candidate_kind, .. } => {
            format!("Memory candidate captured: {candidate_kind}")
        }
        RuntimeProtocolEventPayload::EnvironmentLifecycleUpdate { state, .. } => {
            format!("Environment lifecycle: {}", state.as_str())
        }
        RuntimeProtocolEventPayload::SandboxLifecycleUpdate { phase, .. } => {
            format!("Sandbox lifecycle: {phase}")
        }
        RuntimeProtocolEventPayload::ValidationStarted { label } => {
            format!("Validation started: {label}")
        }
        RuntimeProtocolEventPayload::ValidationCompleted { label, outcome } => {
            format!("Validation completed: {label} {outcome}")
        }
        RuntimeProtocolEventPayload::ToolRegistrySnapshot { tool_count, .. } => {
            format!("Tool registry snapshot: {tool_count} tools")
        }
        RuntimeProtocolEventPayload::FileChanged { path, operation } => {
            format!("File {operation}: {path}")
        }
        RuntimeProtocolEventPayload::CommandOutput { stream, .. } => {
            format!("Command output: {stream}")
        }
        RuntimeProtocolEventPayload::ToolPermissionGrant { tool_name, .. } => {
            format!("Tool permission granted: {tool_name}")
        }
        RuntimeProtocolEventPayload::ProviderModelChanged {
            provider_id,
            model_id,
        } => format!("Provider model changed: {provider_id}/{model_id}"),
        RuntimeProtocolEventPayload::RuntimeSettingsChanged { .. } => {
            "Runtime settings changed".into()
        }
        RuntimeProtocolEventPayload::RunPaused { .. } => "Run paused".into(),
        RuntimeProtocolEventPayload::Untyped { event_kind, .. } => {
            format!("Untyped event: {event_kind:?}")
        }
    }
}

fn replay_text(payload: &RuntimeProtocolEventPayload) -> Option<String> {
    match payload {
        RuntimeProtocolEventPayload::MessageDelta { text, .. }
        | RuntimeProtocolEventPayload::ReasoningSummary { text }
        | RuntimeProtocolEventPayload::ToolDelta { text, .. }
        | RuntimeProtocolEventPayload::CommandOutput { text, .. } => Some(text.clone()),
        RuntimeProtocolEventPayload::RunCompleted { summary, .. } => Some(summary.clone()),
        RuntimeProtocolEventPayload::RunFailed { message, .. } => Some(message.clone()),
        RuntimeProtocolEventPayload::ApprovalRequired { detail, .. } => Some(detail.clone()),
        RuntimeProtocolEventPayload::EnvironmentLifecycleUpdate { detail, .. } => detail.clone(),
        RuntimeProtocolEventPayload::VerificationGate { summary, .. } => summary.clone(),
        RuntimeProtocolEventPayload::RuntimeSettingsChanged { summary } => Some(summary.clone()),
        _ => None,
    }
}

fn run_status_from_wire(value: &str) -> Option<RunStatus> {
    match value {
        "starting" => Some(RunStatus::Starting),
        "running" => Some(RunStatus::Running),
        "paused" => Some(RunStatus::Paused),
        "cancelling" => Some(RunStatus::Cancelling),
        "cancelled" => Some(RunStatus::Cancelled),
        "handed_off" => Some(RunStatus::HandedOff),
        "completed" => Some(RunStatus::Completed),
        "failed" => Some(RunStatus::Failed),
        _ => None,
    }
}

fn text_field(payload: &JsonValue, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
}

fn role_field(payload: &JsonValue, key: &str) -> Option<MessageRole> {
    match payload.get(key).and_then(JsonValue::as_str)? {
        "system" => Some(MessageRole::System),
        "developer" => Some(MessageRole::Developer),
        "user" => Some(MessageRole::User),
        "assistant" => Some(MessageRole::Assistant),
        "tool" => Some(MessageRole::Tool),
        _ => None,
    }
}

fn stable_hash(seed: u64, scope: &str, parts: &[&str]) -> u64 {
    let mut hash = FNV_OFFSET ^ seed;
    for byte in scope.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    for part in parts {
        hash ^= 0xff;
        hash = hash.wrapping_mul(FNV_PRIME);
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

fn is_lower_hex_len(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{ContextManifest, RunControls, RuntimeEvent};

    fn live_preflight_json() -> JsonValue {
        json!({
            "contractVersion": 1,
            "profileId": "fake_provider",
            "providerId": "fake_provider",
            "modelId": "fake-model",
            "source": "live_probe",
            "checkedAt": "2026-05-03T12:00:00Z",
            "ageSeconds": 0,
            "ttlSeconds": 21600,
            "stale": false,
            "requiredFeatures": {
                "streaming": true,
                "toolCalls": true,
                "reasoningControls": false,
                "attachments": false
            },
            "capabilities": {},
            "checks": [],
            "status": "passed"
        })
    }

    fn static_preflight_json() -> JsonValue {
        json!({
            "contractVersion": 1,
            "profileId": "openai_api-headless",
            "providerId": "openai_api",
            "modelId": "test-model",
            "source": "static_manual",
            "checkedAt": "2026-05-03T12:00:00Z",
            "ageSeconds": 0,
            "ttlSeconds": 21600,
            "stale": false,
            "requiredFeatures": {
                "streaming": true,
                "toolCalls": true,
                "reasoningControls": false,
                "attachments": false
            },
            "capabilities": {},
            "checks": [{
                "checkId": "provider-preflight-tool-schema",
                "status": "warning",
                "code": "provider_preflight_tool_schema",
                "message": "Tool support came from manual metadata.",
                "source": "static_manual",
                "retryable": false
            }],
            "status": "warning"
        })
    }

    fn live_real_preflight_json(provider_id: &str, model_id: &str) -> JsonValue {
        let mut preflight = live_preflight_json();
        preflight["profileId"] = json!(provider_id);
        preflight["providerId"] = json!(provider_id);
        preflight["modelId"] = json!(model_id);
        preflight
    }

    fn focused_test_matrix(
        status: ProductionReadinessFocusedTestStatus,
    ) -> Vec<ProductionReadinessFocusedTestResult> {
        PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS
            .iter()
            .map(|command| ProductionReadinessFocusedTestResult {
                command: (*command).into(),
                status,
                summary: "fixture evidence".into(),
                checked_at: Some("2026-05-05T12:00:00Z".into()),
            })
            .collect()
    }

    fn good_real_provider_trace() -> RuntimeTrace {
        let trace_id = runtime_trace_id_for_run("project-real", "run-good");
        let preflight = live_real_preflight_json("openai_api", "test-model");
        let manifest_trace =
            RuntimeTraceContext::for_context_manifest(&trace_id, "run-good", "manifest-good", 0);
        RuntimeTrace::from_snapshot(RunSnapshot {
            trace_id: trace_id.clone(),
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: "project-real".into(),
            agent_session_id: "session-real".into(),
            run_id: "run-good".into(),
            provider_id: "openai_api".into(),
            model_id: "test-model".into(),
            status: RunStatus::Completed,
            prompt: "Use a real provider tool.".into(),
            messages: vec![crate::RuntimeMessage {
                id: 1,
                project_id: "project-real".into(),
                run_id: "run-good".into(),
                role: MessageRole::Tool,
                content: r#"{"ok":true,"path":"tracked.txt"}"#.into(),
                provider_metadata: Some(crate::RuntimeMessageProviderMetadata::tool_result(
                    "provider-tool-result-run-good-0-call-read",
                    "call-read",
                    "read",
                    "provider-assistant-run-good-0",
                )),
                created_at: "2026-05-03T12:00:06Z".into(),
            }],
            events: vec![
                RuntimeEvent {
                    id: 1,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::RunStarted,
                    trace: RuntimeTraceContext::for_run(&trace_id, "run-good", "run_started"),
                    payload: json!({
                        "status": "running",
                        "providerId": "openai_api",
                        "modelId": "test-model",
                        "providerPreflight": preflight,
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 2,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::ValidationCompleted,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-good",
                        2,
                        &RuntimeEventKind::ValidationCompleted,
                    ),
                    payload: json!({
                        "label": "provider_preflight",
                        "outcome": "passed",
                        "providerPreflight": live_real_preflight_json("openai_api", "test-model"),
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 3,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::EnvironmentLifecycleUpdate,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-good",
                        3,
                        &RuntimeEventKind::EnvironmentLifecycleUpdate,
                    ),
                    payload: json!({
                        "state": "ready",
                        "semanticIndexRequired": false,
                        "sandboxGroupingPolicy": "none",
                    }),
                    created_at: "2026-05-03T12:00:01Z".into(),
                },
                RuntimeEvent {
                    id: 4,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::ContextManifestRecorded,
                    trace: manifest_trace.clone(),
                    payload: json!({
                        "manifestId": "manifest-good",
                        "contextHash": "context-hash",
                        "turnIndex": 0,
                    }),
                    created_at: "2026-05-03T12:00:02Z".into(),
                },
                RuntimeEvent {
                    id: 5,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::ToolRegistrySnapshot,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-good",
                        5,
                        &RuntimeEventKind::ToolRegistrySnapshot,
                    ),
                    payload: json!({
                        "toolCount": 1,
                        "toolNames": ["read"],
                        "executionRegistry": "tool_registry_v2",
                    }),
                    created_at: "2026-05-03T12:00:03Z".into(),
                },
                RuntimeEvent {
                    id: 6,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::ReasoningSummary,
                    trace: RuntimeTraceContext::for_provider_turn(&trace_id, "run-good", 0),
                    payload: json!({ "text": "Use the read tool." }),
                    created_at: "2026-05-03T12:00:04Z".into(),
                },
                RuntimeEvent {
                    id: 7,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::ToolStarted,
                    trace: RuntimeTraceContext::for_tool_call(&trace_id, "run-good", "call-read"),
                    payload: json!({
                        "toolCallId": "call-read",
                        "toolName": "read",
                        "dispatch": { "registryVersion": "tool_registry_v2" },
                    }),
                    created_at: "2026-05-03T12:00:05Z".into(),
                },
                RuntimeEvent {
                    id: 8,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::ToolCompleted,
                    trace: RuntimeTraceContext::for_tool_call(&trace_id, "run-good", "call-read"),
                    payload: json!({
                        "toolCallId": "call-read",
                        "toolName": "read",
                        "ok": true,
                        "outcome": "success",
                        "dispatch": { "registryVersion": "tool_registry_v2" },
                    }),
                    created_at: "2026-05-03T12:00:06Z".into(),
                },
                RuntimeEvent {
                    id: 9,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::MessageDelta,
                    trace: RuntimeTraceContext::for_provider_turn(&trace_id, "run-good", 0),
                    payload: json!({ "role": "assistant", "text": "Read tracked.txt." }),
                    created_at: "2026-05-03T12:00:07Z".into(),
                },
                RuntimeEvent {
                    id: 10,
                    project_id: "project-real".into(),
                    run_id: "run-good".into(),
                    event_kind: RuntimeEventKind::RunCompleted,
                    trace: RuntimeTraceContext::for_run(&trace_id, "run-good", "run_completed"),
                    payload: json!({ "summary": "done" }),
                    created_at: "2026-05-03T12:00:08Z".into(),
                },
            ],
            context_manifests: vec![ContextManifest {
                manifest_id: "manifest-good".into(),
                project_id: "project-real".into(),
                agent_session_id: "session-real".into(),
                run_id: "run-good".into(),
                provider_id: "openai_api".into(),
                model_id: "test-model".into(),
                turn_index: 0,
                context_hash: "context-hash".into(),
                recorded_after_event_id: Some(4),
                trace: manifest_trace,
                manifest: json!({
                    "kind": "provider_context_package",
                    "providerPreflight": live_real_preflight_json("openai_api", "test-model"),
                }),
                created_at: "2026-05-03T12:00:02Z".into(),
            }],
        })
        .expect("good trace")
    }

    #[test]
    fn submission_envelope_schema_snapshot_is_stable() {
        let envelope = RuntimeSubmissionEnvelope {
            protocol_version: CORE_PROTOCOL_VERSION,
            submission_id: "submission-1".into(),
            trace: RuntimeTraceContext::for_run(
                &runtime_trace_id_for_run("project-1", "run-1"),
                "run-1",
                "start_run",
            ),
            submitted_at: "2026-05-03T12:00:00Z".into(),
            submission: RuntimeSubmission::StartRun(StartRunRequest {
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-1".into(),
                prompt: "Implement phase 2.".into(),
                provider: ProviderSelection {
                    provider_id: "fake_provider".into(),
                    model_id: "fake-model".into(),
                },
                controls: Some(RunControls {
                    runtime_agent_id: "engineer".into(),
                    agent_definition_id: None,
                    agent_definition_version: None,
                    thinking_effort: None,
                    approval_mode: "suggest".into(),
                    plan_mode_required: true,
                }),
            }),
        };

        let actual = serde_json::to_string_pretty(&envelope).expect("serialize envelope");
        assert_eq!(
            actual,
            r#"{
  "protocolVersion": 3,
  "submissionId": "submission-1",
  "trace": {
    "traceId": "d4f540327c3af2abc99f37979596ec6d",
    "spanId": "ae5365d98d6c430c",
    "runTraceId": "d4f540327c3af2abc99f37979596ec6d"
  },
  "submittedAt": "2026-05-03T12:00:00Z",
  "submission": {
    "kind": "start_run",
    "payload": {
      "projectId": "project-1",
      "agentSessionId": "session-1",
      "runId": "run-1",
      "prompt": "Implement phase 2.",
      "provider": {
        "providerId": "fake_provider",
        "modelId": "fake-model"
      },
      "controls": {
        "runtimeAgentId": "engineer",
        "approvalMode": "suggest",
        "planModeRequired": true
      }
    }
  }
}"#
        );
    }

    #[test]
    fn event_envelope_schema_snapshot_is_stable() {
        let trace_id = runtime_trace_id_for_run("project-1", "run-1");
        let event = RuntimeEvent {
            id: 7,
            project_id: "project-1".into(),
            run_id: "run-1".into(),
            event_kind: RuntimeEventKind::ContextManifestRecorded,
            trace: RuntimeTraceContext::for_storage_write(
                &trace_id,
                "run-1",
                "context_manifest",
                0,
            ),
            payload: json!({
                "manifestId": "manifest-1",
                "contextHash": "abc123",
                "turnIndex": 0,
            }),
            created_at: "2026-05-03T12:00:01Z".into(),
        };

        let actual = serde_json::to_string_pretty(
            &event
                .to_protocol_event()
                .expect("event should convert to protocol event"),
        )
        .expect("serialize event");
        assert_eq!(
            actual,
            r#"{
  "protocolVersion": 3,
  "eventId": 7,
  "projectId": "project-1",
  "runId": "run-1",
  "eventKind": "context_manifest_recorded",
  "trace": {
    "traceId": "d4f540327c3af2abc99f37979596ec6d",
    "spanId": "97904fd6111fa2d4",
    "parentSpanId": "e24d46853cb84fa3",
    "runTraceId": "d4f540327c3af2abc99f37979596ec6d",
    "storageWriteTraceId": "17fdcd179cdb2ddab6d76a9810aaa850"
  },
  "payload": {
    "kind": "context_manifest_recorded",
    "payload": {
      "manifestId": "manifest-1",
      "contextHash": "abc123",
      "turnIndex": 0
    }
  },
  "occurredAt": "2026-05-03T12:00:01Z"
}"#
        );
    }

    #[test]
    fn protocol_version_mismatch_fails_explicitly() {
        let envelope = RuntimeSubmissionEnvelope {
            protocol_version: CORE_PROTOCOL_VERSION + 1,
            submission_id: "submission-1".into(),
            trace: RuntimeTraceContext::for_run(
                &runtime_trace_id_for_run("project-1", "run-1"),
                "run-1",
                "start_run",
            ),
            submitted_at: "2026-05-03T12:00:00Z".into(),
            submission: RuntimeSubmission::Cancel(CancelRunRequest {
                project_id: "project-1".into(),
                run_id: "run-1".into(),
            }),
        };

        let error = envelope
            .validate_protocol_version()
            .expect_err("future protocol version should fail");
        assert_eq!(error.code, "agent_protocol_version_mismatch");
    }

    #[test]
    fn support_bundle_redacts_raw_text_and_secret_like_values() {
        let trace_id = runtime_trace_id_for_run("project-1", "run-secret");
        let snapshot = RunSnapshot {
            trace_id: trace_id.clone(),
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-secret".into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            status: RunStatus::Failed,
            prompt: "Use https://example.test/callback?access_token=secret-token".into(),
            messages: vec![crate::RuntimeMessage {
                id: 1,
                project_id: "project-1".into(),
                run_id: "run-secret".into(),
                role: MessageRole::User,
                content: "Bearer secret-token".into(),
                provider_metadata: None,
                created_at: "2026-05-03T12:00:00Z".into(),
            }],
            events: vec![
                RuntimeEvent {
                    id: 1,
                    project_id: "project-1".into(),
                    run_id: "run-secret".into(),
                    event_kind: RuntimeEventKind::RunStarted,
                    trace: RuntimeTraceContext::for_run(&trace_id, "run-secret", "run_started"),
                    payload: json!({
                        "status": "running",
                        "providerId": "fake_provider",
                        "modelId": "fake-model",
                        "providerPreflight": live_preflight_json(),
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 2,
                    project_id: "project-1".into(),
                    run_id: "run-secret".into(),
                    event_kind: RuntimeEventKind::EnvironmentLifecycleUpdate,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-secret",
                        2,
                        &RuntimeEventKind::EnvironmentLifecycleUpdate,
                    ),
                    payload: json!({
                        "environmentId": "env-project-1-run-secret",
                        "state": "ready",
                        "sandboxGroupingPolicy": "none",
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 3,
                    project_id: "project-1".into(),
                    run_id: "run-secret".into(),
                    event_kind: RuntimeEventKind::CommandOutput,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-secret",
                        3,
                        &RuntimeEventKind::CommandOutput,
                    ),
                    payload: json!({
                        "stream": "stderr",
                        "text": "Bearer secret-token",
                        "credentialPath": "/Users/alice/.ssh/id_rsa",
                        "cloudCredentialPath": "/Users/alice/.aws/credentials"
                    }),
                    created_at: "2026-05-03T12:00:01Z".into(),
                },
                RuntimeEvent {
                    id: 4,
                    project_id: "project-1".into(),
                    run_id: "run-secret".into(),
                    event_kind: RuntimeEventKind::RunFailed,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-secret",
                        4,
                        &RuntimeEventKind::RunFailed,
                    ),
                    payload: json!({
                        "code": "provider_error",
                        "message": "Provider returned Bearer secret-token"
                    }),
                    created_at: "2026-05-03T12:00:02Z".into(),
                },
            ],
            context_manifests: vec![ContextManifest {
                manifest_id: "manifest-secret".into(),
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-secret".into(),
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
                turn_index: 0,
                context_hash: "context-hash".into(),
                recorded_after_event_id: Some(2),
                trace: RuntimeTraceContext::for_context_manifest(
                    &trace_id,
                    "run-secret",
                    "manifest-secret",
                    0,
                ),
                manifest: json!({
                    "kind": "provider_context_package",
                    "providerPreflight": live_preflight_json(),
                }),
                created_at: "2026-05-03T12:00:00Z".into(),
            }],
        };
        let trace = RuntimeTrace::from_snapshot(snapshot).expect("trace");
        let bundle = trace.redacted_support_bundle().expect("support bundle");
        let serialized = serde_json::to_string(&bundle).expect("serialize bundle");

        assert!(!serialized.contains("secret-token"));
        assert!(!serialized.contains("/Users/alice/.ssh/id_rsa"));
        assert!(!serialized.contains("/Users/alice/.aws/credentials"));
        assert!(serialized.contains("[REDACTED:"));
        assert!(bundle.quality_gates.passed);
        assert!(bundle.redaction_report.redacted_value_count > 0);
    }

    #[test]
    fn trace_quality_phase0_fails_for_headless_real_provider_static_preflight_and_tool_history() {
        let trace_id = runtime_trace_id_for_run("project-real", "run-real");
        let manifest_trace =
            RuntimeTraceContext::for_context_manifest(&trace_id, "run-real", "manifest-real", 0);
        let trace = RuntimeTrace::from_snapshot(RunSnapshot {
            trace_id: trace_id.clone(),
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: "project-real".into(),
            agent_session_id: "session-real".into(),
            run_id: "run-real".into(),
            provider_id: "openai_api".into(),
            model_id: "test-model".into(),
            status: RunStatus::Completed,
            prompt: "Use a real provider tool.".into(),
            messages: vec![crate::RuntimeMessage {
                id: 1,
                project_id: "project-real".into(),
                run_id: "run-real".into(),
                role: MessageRole::Tool,
                content: r#"{"ok":true,"path":"phase0.txt"}"#.into(),
                provider_metadata: None,
                created_at: "2026-05-03T12:00:03Z".into(),
            }],
            events: vec![
                RuntimeEvent {
                    id: 1,
                    project_id: "project-real".into(),
                    run_id: "run-real".into(),
                    event_kind: RuntimeEventKind::RunStarted,
                    trace: RuntimeTraceContext::for_run(&trace_id, "run-real", "run_started"),
                    payload: json!({
                        "status": "starting",
                        "providerId": "openai_api",
                        "modelId": "test-model",
                        "execution": "headless_real_provider",
                        "providerPreflight": static_preflight_json(),
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 2,
                    project_id: "project-real".into(),
                    run_id: "run-real".into(),
                    event_kind: RuntimeEventKind::EnvironmentLifecycleUpdate,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-real",
                        2,
                        &RuntimeEventKind::EnvironmentLifecycleUpdate,
                    ),
                    payload: json!({ "state": "ready" }),
                    created_at: "2026-05-03T12:00:01Z".into(),
                },
                RuntimeEvent {
                    id: 3,
                    project_id: "project-real".into(),
                    run_id: "run-real".into(),
                    event_kind: RuntimeEventKind::ToolStarted,
                    trace: RuntimeTraceContext::for_tool_call(&trace_id, "run-real", "call-write"),
                    payload: json!({
                        "toolCallId": "call-write",
                        "toolName": "write_file",
                        "runtime": "headless_real_provider",
                    }),
                    created_at: "2026-05-03T12:00:02Z".into(),
                },
                RuntimeEvent {
                    id: 4,
                    project_id: "project-real".into(),
                    run_id: "run-real".into(),
                    event_kind: RuntimeEventKind::ToolCompleted,
                    trace: RuntimeTraceContext::for_tool_call(&trace_id, "run-real", "call-write"),
                    payload: json!({
                        "toolCallId": "call-write",
                        "toolName": "write_file",
                        "ok": true,
                        "runtime": "headless_real_provider",
                    }),
                    created_at: "2026-05-03T12:00:03Z".into(),
                },
            ],
            context_manifests: vec![ContextManifest {
                manifest_id: "manifest-real".into(),
                project_id: "project-real".into(),
                agent_session_id: "session-real".into(),
                run_id: "run-real".into(),
                provider_id: "openai_api".into(),
                model_id: "test-model".into(),
                turn_index: 0,
                context_hash: "context-hash".into(),
                recorded_after_event_id: Some(2),
                trace: manifest_trace,
                manifest: json!({
                    "kind": "provider_context_package",
                    "providerPreflight": static_preflight_json(),
                }),
                created_at: "2026-05-03T12:00:01Z".into(),
            }],
        })
        .expect("trace");
        let canonical = trace.canonical_snapshot().expect("canonical trace");

        assert!(!canonical.quality_gates.passed);
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ProductionRuntimeStore,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ProviderCapabilityPreflight,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ToolRegistryV2Execution,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ProviderHistoryReplay,
            TraceQualityGateStatus::Fail,
        );
    }

    #[test]
    fn trace_quality_provider_history_replay_accepts_message_metadata() {
        let message = crate::RuntimeMessage {
            id: 1,
            project_id: "project-real".into(),
            run_id: "run-real".into(),
            role: MessageRole::Tool,
            content: r#"{"ok":true}"#.into(),
            provider_metadata: Some(crate::RuntimeMessageProviderMetadata::tool_result(
                "provider-tool-result-run-real-0-call-read",
                "call-read",
                "read",
                "provider-assistant-run-real-0",
            )),
            created_at: "2026-05-03T12:00:03Z".into(),
        };

        assert!(tool_message_has_replay_metadata(&message));
    }

    #[test]
    fn trace_quality_phase8_good_real_provider_trace_passes_release_gates_and_report() {
        let trace = good_real_provider_trace();
        let canonical = trace.canonical_snapshot().expect("canonical trace");

        assert!(
            canonical.quality_gates.passed,
            "unexpected failed gates: {:?}",
            canonical
                .quality_gates
                .gates
                .iter()
                .filter(|gate| gate.status == TraceQualityGateStatus::Fail)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            canonical.production_readiness.status,
            ProductionReadinessStatus::Blocked,
            "trace-only readiness must remain blocked until focused test evidence is attached"
        );

        let report = canonical.production_readiness_report(focused_test_matrix(
            ProductionReadinessFocusedTestStatus::Passed,
        ));
        assert_eq!(report.status, ProductionReadinessStatus::Ready);
        assert!(report.blockers.is_empty());
        assert_eq!(report.gate_summary.failed, 0);
        assert_eq!(
            report.focused_test_summary.missing_required_commands.len(),
            0
        );
    }

    #[test]
    fn trace_quality_phase8_readiness_report_blocks_missing_and_failed_focused_tests() {
        let trace = good_real_provider_trace();
        let canonical = trace.canonical_snapshot().expect("canonical trace");
        let missing_report = canonical.production_readiness_report(Vec::new());

        assert_eq!(missing_report.status, ProductionReadinessStatus::Blocked);
        assert_eq!(
            missing_report
                .focused_test_summary
                .missing_required_commands
                .len(),
            PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS.len()
        );
        assert!(missing_report.blockers.iter().any(|blocker| {
            blocker.kind == ProductionReadinessBlockerKind::FocusedTestMissing
                && blocker.layer == TraceFailureLayer::Verification
        }));

        let mut focused_tests = focused_test_matrix(ProductionReadinessFocusedTestStatus::Passed);
        focused_tests[0] = ProductionReadinessFocusedTestResult::failed(
            PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS[0],
            "protoc was missing",
        );
        let failed_report = canonical.production_readiness_report(focused_tests);

        assert_eq!(failed_report.status, ProductionReadinessStatus::Blocked);
        assert_eq!(failed_report.focused_test_summary.failed, 1);
        assert!(failed_report.blockers.iter().any(|blocker| {
            blocker.kind == ProductionReadinessBlockerKind::FocusedTestFailed
                && blocker.command.as_deref()
                    == Some(PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS[0])
        }));
    }

    #[test]
    fn trace_quality_phase0_fails_for_manifest_mismatch_sandbox_success_timeout_and_workspace() {
        let trace_id = runtime_trace_id_for_run("project-1", "run-phase0");
        let mut mismatched_preflight = live_preflight_json();
        mismatched_preflight["checkedAt"] = json!("2026-05-03T12:00:01Z");
        let trace = RuntimeTrace::from_snapshot(RunSnapshot {
            trace_id: trace_id.clone(),
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-phase0".into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            status: RunStatus::Failed,
            prompt: "Exercise Phase 0 gates.".into(),
            messages: Vec::new(),
            events: vec![
                RuntimeEvent {
                    id: 1,
                    project_id: "project-1".into(),
                    run_id: "run-phase0".into(),
                    event_kind: RuntimeEventKind::RunStarted,
                    trace: RuntimeTraceContext::for_run(&trace_id, "run-phase0", "run_started"),
                    payload: json!({
                        "status": "starting",
                        "providerId": "fake_provider",
                        "modelId": "fake-model",
                        "providerPreflight": live_preflight_json(),
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 2,
                    project_id: "project-1".into(),
                    run_id: "run-phase0".into(),
                    event_kind: RuntimeEventKind::EnvironmentLifecycleUpdate,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-phase0",
                        2,
                        &RuntimeEventKind::EnvironmentLifecycleUpdate,
                    ),
                    payload: json!({
                        "state": "starting",
                        "semanticIndexRequired": true,
                    }),
                    created_at: "2026-05-03T12:00:01Z".into(),
                },
                RuntimeEvent {
                    id: 3,
                    project_id: "project-1".into(),
                    run_id: "run-phase0".into(),
                    event_kind: RuntimeEventKind::ToolCompleted,
                    trace: RuntimeTraceContext::for_tool_call(
                        &trace_id,
                        "run-phase0",
                        "call-denied",
                    ),
                    payload: json!({
                        "toolCallId": "call-denied",
                        "toolName": "write_file",
                        "ok": true,
                        "sandbox": {
                            "metadata": {
                                "exitClassification": "denied_by_sandbox"
                            }
                        }
                    }),
                    created_at: "2026-05-03T12:00:02Z".into(),
                },
                RuntimeEvent {
                    id: 4,
                    project_id: "project-1".into(),
                    run_id: "run-phase0".into(),
                    event_kind: RuntimeEventKind::ToolCompleted,
                    trace: RuntimeTraceContext::for_tool_call(
                        &trace_id,
                        "run-phase0",
                        "call-timeout",
                    ),
                    payload: json!({
                        "toolCallId": "call-timeout",
                        "toolName": "read_file",
                        "outcome": "timeout",
                    }),
                    created_at: "2026-05-03T12:00:03Z".into(),
                },
            ],
            context_manifests: vec![ContextManifest {
                manifest_id: "manifest-phase0".into(),
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-phase0".into(),
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
                turn_index: 0,
                context_hash: "context-hash".into(),
                recorded_after_event_id: Some(2),
                trace: RuntimeTraceContext::for_context_manifest(
                    &trace_id,
                    "run-phase0",
                    "manifest-phase0",
                    0,
                ),
                manifest: json!({
                    "kind": "provider_context_package",
                    "semanticIndexRequired": true,
                    "providerPreflight": mismatched_preflight,
                }),
                created_at: "2026-05-03T12:00:01Z".into(),
            }],
        })
        .expect("trace");
        let canonical = trace.canonical_snapshot().expect("canonical trace");

        assert!(!canonical.quality_gates.passed);
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ProviderPreflightManifestBinding,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::SandboxDenialOutcome,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ToolTimeoutMetadata,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::WorkspaceIndexLifecycle,
            TraceQualityGateStatus::Fail,
        );
    }

    #[test]
    fn trace_quality_phase8_fails_with_specific_missing_trace_categories() {
        let trace_id = runtime_trace_id_for_run("project-1", "run-missing-observability");
        let trace = RuntimeTrace::from_snapshot(RunSnapshot {
            trace_id: trace_id.clone(),
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-missing-observability".into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            status: RunStatus::Failed,
            prompt: "Prompt".into(),
            messages: Vec::new(),
            events: vec![RuntimeEvent {
                id: 1,
                project_id: "project-1".into(),
                run_id: "run-missing-observability".into(),
                event_kind: RuntimeEventKind::ReasoningSummary,
                trace: RuntimeTraceContext::for_provider_turn(
                    &trace_id,
                    "run-missing-observability",
                    0,
                ),
                payload: json!({ "text": "Provider turn started without observability metadata." }),
                created_at: "2026-05-03T12:00:00Z".into(),
            }],
            context_manifests: Vec::new(),
        })
        .expect("trace");
        let canonical = trace.canonical_snapshot().expect("canonical trace");

        assert!(!canonical.quality_gates.passed);
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::EnvironmentLifecycleEvents,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ProviderPreflightSnapshot,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ContextManifestBeforeProviderTurn,
            TraceQualityGateStatus::Fail,
        );
    }

    #[test]
    fn trace_quality_phase8_fails_for_legacy_tool_dispatch_and_unsandboxed_subprocess() {
        let trace_id = runtime_trace_id_for_run("project-1", "run-legacy-tool");
        let manifest_trace = RuntimeTraceContext::for_context_manifest(
            &trace_id,
            "run-legacy-tool",
            "manifest-legacy-tool",
            0,
        );
        let trace = RuntimeTrace::from_snapshot(RunSnapshot {
            trace_id: trace_id.clone(),
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-legacy-tool".into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            status: RunStatus::Failed,
            prompt: "Prompt".into(),
            messages: Vec::new(),
            events: vec![
                RuntimeEvent {
                    id: 1,
                    project_id: "project-1".into(),
                    run_id: "run-legacy-tool".into(),
                    event_kind: RuntimeEventKind::RunStarted,
                    trace: RuntimeTraceContext::for_run(
                        &trace_id,
                        "run-legacy-tool",
                        "run_started",
                    ),
                    payload: json!({
                        "status": "running",
                        "providerId": "fake_provider",
                        "modelId": "fake-model",
                        "providerPreflight": live_preflight_json(),
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 2,
                    project_id: "project-1".into(),
                    run_id: "run-legacy-tool".into(),
                    event_kind: RuntimeEventKind::EnvironmentLifecycleUpdate,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-legacy-tool",
                        2,
                        &RuntimeEventKind::EnvironmentLifecycleUpdate,
                    ),
                    payload: json!({
                        "environmentId": "env-project-1-run-legacy-tool",
                        "state": "ready",
                        "sandboxGroupingPolicy": "none",
                    }),
                    created_at: "2026-05-03T12:00:00Z".into(),
                },
                RuntimeEvent {
                    id: 3,
                    project_id: "project-1".into(),
                    run_id: "run-legacy-tool".into(),
                    event_kind: RuntimeEventKind::ToolRegistrySnapshot,
                    trace: RuntimeTraceContext::for_event(
                        &trace_id,
                        "run-legacy-tool",
                        3,
                        &RuntimeEventKind::ToolRegistrySnapshot,
                    ),
                    payload: json!({
                        "toolCount": 1,
                        "toolNames": ["command"],
                        "executionRegistry": "legacy_registry",
                    }),
                    created_at: "2026-05-03T12:00:01Z".into(),
                },
                RuntimeEvent {
                    id: 4,
                    project_id: "project-1".into(),
                    run_id: "run-legacy-tool".into(),
                    event_kind: RuntimeEventKind::ToolStarted,
                    trace: RuntimeTraceContext::for_tool_call(
                        &trace_id,
                        "run-legacy-tool",
                        "tool-call-1",
                    ),
                    payload: json!({
                        "toolCallId": "tool-call-1",
                        "toolName": "command",
                        "subprocess": true,
                    }),
                    created_at: "2026-05-03T12:00:02Z".into(),
                },
                RuntimeEvent {
                    id: 5,
                    project_id: "project-1".into(),
                    run_id: "run-legacy-tool".into(),
                    event_kind: RuntimeEventKind::ToolCompleted,
                    trace: RuntimeTraceContext::for_tool_call(
                        &trace_id,
                        "run-legacy-tool",
                        "tool-call-1",
                    ),
                    payload: json!({
                        "toolCallId": "tool-call-1",
                        "toolName": "command",
                        "ok": false,
                        "message": "subprocess failed without sandbox metadata",
                    }),
                    created_at: "2026-05-03T12:00:03Z".into(),
                },
            ],
            context_manifests: vec![ContextManifest {
                manifest_id: "manifest-legacy-tool".into(),
                project_id: "project-1".into(),
                agent_session_id: "session-1".into(),
                run_id: "run-legacy-tool".into(),
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
                turn_index: 0,
                context_hash: "context-hash".into(),
                recorded_after_event_id: Some(2),
                trace: manifest_trace,
                manifest: json!({
                    "kind": "provider_context_package",
                    "providerPreflight": live_preflight_json(),
                }),
                created_at: "2026-05-03T12:00:00Z".into(),
            }],
        })
        .expect("trace");
        let canonical = trace.canonical_snapshot().expect("canonical trace");

        assert!(!canonical.quality_gates.passed);
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::ToolRegistryV2Execution,
            TraceQualityGateStatus::Fail,
        );
        assert_gate(
            &canonical.quality_gates,
            TraceQualityGateCategory::SubprocessSandboxMetadata,
            TraceQualityGateStatus::Fail,
        );
    }

    #[test]
    fn replay_timeline_reports_missing_event_ranges() {
        let trace_id = runtime_trace_id_for_run("project-1", "run-gap");
        let event_one = RuntimeEvent {
            id: 1,
            project_id: "project-1".into(),
            run_id: "run-gap".into(),
            event_kind: RuntimeEventKind::RunStarted,
            trace: RuntimeTraceContext::for_run(&trace_id, "run-gap", "run_started"),
            payload: json!({
                "status": "running",
                "providerId": "fake_provider",
                "modelId": "fake-model"
            }),
            created_at: "2026-05-03T12:00:00Z".into(),
        };
        let event_three = RuntimeEvent {
            id: 3,
            project_id: "project-1".into(),
            run_id: "run-gap".into(),
            event_kind: RuntimeEventKind::RunCompleted,
            trace: RuntimeTraceContext::for_run(&trace_id, "run-gap", "run_completed"),
            payload: json!({ "summary": "done", "state": "complete" }),
            created_at: "2026-05-03T12:00:02Z".into(),
        };
        let trace = RuntimeTrace::from_snapshot(RunSnapshot {
            trace_id,
            runtime_agent_id: "engineer".into(),
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            system_prompt: "test system prompt".into(),
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "run-gap".into(),
            provider_id: "fake_provider".into(),
            model_id: "fake-model".into(),
            status: RunStatus::Completed,
            prompt: "Prompt".into(),
            messages: Vec::new(),
            events: vec![event_one, event_three],
            context_manifests: Vec::new(),
        })
        .expect("trace");
        let canonical = trace.canonical_snapshot().expect("canonical trace");

        assert_eq!(canonical.timeline.missing_segments.len(), 1);
        assert_eq!(canonical.timeline.missing_segments[0].start_event_id, 2);
        assert!(!canonical.quality_gates.passed);
        assert!(canonical
            .diagnostics
            .signals
            .iter()
            .any(|signal| signal.category == TraceDiagnosticCategory::MissingTimelineSegment));
    }

    fn assert_gate(
        report: &TraceQualityGateReport,
        category: TraceQualityGateCategory,
        status: TraceQualityGateStatus,
    ) {
        let gate = report
            .gates
            .iter()
            .find(|gate| gate.category == category)
            .expect("gate should exist");
        assert_eq!(gate.status, status);
    }
}
