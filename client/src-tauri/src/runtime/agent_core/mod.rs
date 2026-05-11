use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

mod consumed_artifacts;
mod context_package;
mod db_touchpoints;
mod environment_lifecycle;
mod evals;
mod events;
mod facade;
mod harness_contract;
mod harness_order;
mod memory_guardrails;
mod output_sections;
mod provider_adapters;
mod supervisor;

mod persistence;
mod provider_loop;
mod run;
mod state_machine;
mod synthetic_dispatch;
mod tool_descriptors;
mod tool_dispatch;
mod types;

pub use evals::{
    run_agent_definition_quality_eval_suite, run_agent_harness_eval_suite,
    run_custom_agent_simulation_harness, run_handoff_context_quality_eval_suite,
    run_no_redescription_needed_eval_suite, run_retrieval_memory_quality_eval_suite,
    run_test_agent_ci_eval, run_xero_quality_eval_suites, AgentDefinitionEvalFixtureKind,
    AgentDefinitionQualityCaseResult, AgentDefinitionQualityCoverage,
    AgentDefinitionQualityEvalReport, AgentDefinitionQualityMetrics, AgentDefinitionQualitySurface,
    AgentDefinitionQualityThresholds, AgentHarnessEvalCaseResult, AgentHarnessEvalCoverage,
    AgentHarnessEvalMetrics, AgentHarnessEvalReport, AgentHarnessEvalThresholds,
    CustomAgentSimulationCaseResult, CustomAgentSimulationCoverage,
    CustomAgentSimulationHarnessReport, CustomAgentSimulationSurface,
    HandoffContextQualityCaseResult, HandoffContextQualityCoverage,
    HandoffContextQualityEvalReport, HandoffContextQualityMetrics, HandoffContextQualitySurface,
    HarnessEvalFixtureKind, NoRedescriptionContinuityCaseResult, NoRedescriptionContinuityCoverage,
    NoRedescriptionContinuityEvalReport, NoRedescriptionContinuityMetrics,
    NoRedescriptionContinuitySurface, RetrievalMemoryQualityCaseResult,
    RetrievalMemoryQualityCoverage, RetrievalMemoryQualityEvalReport,
    RetrievalMemoryQualityMetrics, RetrievalMemoryQualitySurface, TestAgentCiEvalReport,
    TestAgentCiManifestOutcome, XeroQualityEvalReport,
};
pub use events::{publish_agent_event, subscribe_agent_events, AgentEventSubscription};
pub use facade::{
    DesktopAgentCoreRuntime, DesktopCancelRunRequest, DesktopCompactSessionRequest,
    DesktopContinueRunRequest, DesktopExportTraceRequest, DesktopForkSessionRequest,
    DesktopRejectActionRequest, DesktopRunDriveMode, DesktopStartRunRequest,
};
pub use harness_contract::{
    export_harness_contract, harness_contract_drift, HarnessAgentToolAccessSnapshot,
    HarnessContractDrift, HarnessContractExport, HarnessContractExportOptions,
    HarnessPromptFragmentSnapshot, HarnessPromptSnapshot, HarnessToolCapabilitySpec,
    HarnessToolCatalogSnapshot, HarnessToolRegistrySnapshot, HARNESS_CONTRACT_SCHEMA,
    HARNESS_CONTRACT_SCHEMA_VERSION,
};
pub use provider_adapters::{
    create_provider_adapter, AgentProviderConfig, AnthropicProviderConfig, BedrockProviderConfig,
    DeepSeekProviderConfig, OpenAiCodexResponsesProviderConfig, OpenAiCompatibleProviderConfig,
    OpenAiResponsesProviderConfig, VertexProviderConfig,
};
pub use run::*;
pub use supervisor::{
    cancelled_error, AgentRunCancellationToken, AgentRunLease, AgentRunSupervisor,
    AGENT_RUN_CANCELLED_CODE,
};
pub use types::*;

pub use consumed_artifacts::{consumed_artifacts_for, ConsumedArtifactEntry};
pub use db_touchpoints::{
    db_touchpoints_for_runtime_agent, DbTouchpointEntry, DbTouchpoints, TriggerRef,
};
pub use output_sections::{output_sections_for, OutputSectionEntry};

pub(crate) use context_package::*;
pub(crate) use environment_lifecycle::*;
pub(crate) use harness_order::*;
pub(crate) use memory_guardrails::*;
pub(crate) use persistence::*;
pub(crate) use synthetic_dispatch::{dispatch_synthetic_tool_calls, SyntheticDispatchOptions};
pub(crate) use tool_dispatch::dry_run_tool_call;
pub(crate) use tool_descriptors::builtin_tool_descriptors;
pub(crate) use provider_loop::*;
pub(crate) use state_machine::*;
pub(crate) use tool_descriptors::*;
pub(crate) use tool_dispatch::*;

use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use crate::{
    auth::now_timestamp,
    commands::{
        context_budget, default_runtime_agent_approval_mode, default_runtime_agent_id,
        ensure_runtime_agent_available, estimate_tokens, evaluate_compaction_policy,
        redact_session_context_text, resolve_context_limit, runtime_agent_allows_approval_mode,
        soul_prompt_fragment, AgentToolApplicationStyleDto,
        AgentToolApplicationStyleResolutionSourceDto, BrowserControlPreferenceDto, CommandError,
        CommandErrorClass, CommandResult, ResolvedAgentToolApplicationStyleDto, RuntimeAgentIdDto,
        RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto, RuntimeRunControlInputDto,
        RuntimeRunControlStateDto, SessionCompactionPolicyInput, SessionContextBudgetPressureDto,
        SessionContextPolicyActionDto, SoulSettingsDto,
    },
    db::project_store::{
        self, AgentEventRecord, AgentMessageRecord, AgentMessageRole, AgentRunEventKind,
        AgentRunSnapshotRecord, AgentRunStatus, AgentToolCallFinishRecord,
        AgentToolCallStartRecord, AgentToolCallState, NewAgentActionRequestRecord,
        NewAgentCheckpointRecord, NewAgentEventRecord, NewAgentFileChangeRecord,
        NewAgentMessageRecord, NewAgentRunRecord,
    },
    runtime::{
        autonomous_tool_runtime::{
            deferred_tool_catalog, emulator::emulator_schema, runtime_host_metadata,
            system_diagnostics_action_approval_id, tool_access_all_known_tools,
            tool_access_group_descriptors, tool_access_group_tools, tool_allowed_for_runtime_agent,
            tool_allowed_for_runtime_agent_with_policy, tool_available_on_current_host,
            tool_catalog_metadata_for_tool, tool_effect_class, AutonomousAgentToolPolicy,
            AutonomousHarnessRunnerAction, AutonomousHarnessRunnerRequest,
            AutonomousToolAccessGroup, AutonomousToolCatalogEntry, AutonomousToolEffectClass,
            RuntimeHostMetadata, AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX,
            AUTONOMOUS_TOOL_AGENT_COORDINATION, AUTONOMOUS_TOOL_BROWSER,
            AUTONOMOUS_TOOL_BROWSER_CONTROL, AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            AUTONOMOUS_TOOL_COMMAND_PROBE, AUTONOMOUS_TOOL_COMMAND_RUN,
            AUTONOMOUS_TOOL_COMMAND_SESSION, AUTONOMOUS_TOOL_COMMAND_VERIFY,
            AUTONOMOUS_TOOL_EMULATOR, AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
            AUTONOMOUS_TOOL_HARNESS_RUNNER, AUTONOMOUS_TOOL_MCP_CALL_TOOL,
            AUTONOMOUS_TOOL_MCP_GET_PROMPT, AUTONOMOUS_TOOL_MCP_LIST,
            AUTONOMOUS_TOOL_MCP_READ_RESOURCE, AUTONOMOUS_TOOL_PROJECT_CONTEXT,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET, AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH, AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE, AUTONOMOUS_TOOL_SOLANA_ALT,
            AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE, AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
            AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ, AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
            AUTONOMOUS_TOOL_SOLANA_CLUSTER, AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
            AUTONOMOUS_TOOL_SOLANA_CODAMA, AUTONOMOUS_TOOL_SOLANA_COST,
            AUTONOMOUS_TOOL_SOLANA_DEPLOY, AUTONOMOUS_TOOL_SOLANA_DOCS,
            AUTONOMOUS_TOOL_SOLANA_EXPLAIN, AUTONOMOUS_TOOL_SOLANA_IDL,
            AUTONOMOUS_TOOL_SOLANA_INDEXER, AUTONOMOUS_TOOL_SOLANA_LOGS,
            AUTONOMOUS_TOOL_SOLANA_PDA, AUTONOMOUS_TOOL_SOLANA_PROGRAM,
            AUTONOMOUS_TOOL_SOLANA_REPLAY, AUTONOMOUS_TOOL_SOLANA_SECRETS,
            AUTONOMOUS_TOOL_SOLANA_SIMULATE, AUTONOMOUS_TOOL_SOLANA_SQUADS,
            AUTONOMOUS_TOOL_SOLANA_TX, AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
            AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD, AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED, AUTONOMOUS_TOOL_WORKSPACE_INDEX,
        },
        redaction::{find_prohibited_persistence_content, redact_command_argv_for_persistence},
        AutonomousCommandOutputChunk, AutonomousDynamicToolRoute, AutonomousMacosAutomationAction,
        AutonomousMacosAutomationOutput, AutonomousMcpAction, AutonomousMcpRequest,
        AutonomousProcessManagerAction, AutonomousSubagentExecutor, AutonomousSubagentTask,
        AutonomousSystemDiagnosticsOutput, AutonomousTodoStatus, AutonomousToolOutput,
        AutonomousToolRequest, AutonomousToolResult, AutonomousToolRuntime,
        XeroAttachedSkillDiagnostic, XeroAttachedSkillRef, XeroAttachedSkillResolutionReport,
        XeroAttachedSkillResolutionRequest, XeroAttachedSkillResolutionSnapshot,
        XeroAttachedSkillResolutionStatus, XeroSkillToolContextPayload,
        AUTONOMOUS_TOOL_AGENT_DEFINITION, AUTONOMOUS_TOOL_CODE_INTEL, AUTONOMOUS_TOOL_COMMAND,
        AUTONOMOUS_TOOL_COMMAND_SESSION_READ, AUTONOMOUS_TOOL_COMMAND_SESSION_START,
        AUTONOMOUS_TOOL_COMMAND_SESSION_STOP, AUTONOMOUS_TOOL_DELETE, AUTONOMOUS_TOOL_EDIT,
        AUTONOMOUS_TOOL_FIND, AUTONOMOUS_TOOL_GIT_DIFF, AUTONOMOUS_TOOL_GIT_STATUS,
        AUTONOMOUS_TOOL_HASH, AUTONOMOUS_TOOL_LIST, AUTONOMOUS_TOOL_LSP,
        AUTONOMOUS_TOOL_MACOS_AUTOMATION, AUTONOMOUS_TOOL_MCP, AUTONOMOUS_TOOL_MKDIR,
        AUTONOMOUS_TOOL_NOTEBOOK_EDIT, AUTONOMOUS_TOOL_PATCH, AUTONOMOUS_TOOL_POWERSHELL,
        AUTONOMOUS_TOOL_PROCESS_MANAGER, AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_RENAME,
        AUTONOMOUS_TOOL_SEARCH, AUTONOMOUS_TOOL_SKILL, AUTONOMOUS_TOOL_SUBAGENT,
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS, AUTONOMOUS_TOOL_TODO, AUTONOMOUS_TOOL_TOOL_ACCESS,
        AUTONOMOUS_TOOL_TOOL_SEARCH, AUTONOMOUS_TOOL_WEB_FETCH, AUTONOMOUS_TOOL_WEB_SEARCH,
        AUTONOMOUS_TOOL_WRITE, OPENAI_CODEX_PROVIDER_ID,
    },
};

pub const OWNED_AGENT_SUPERVISOR_KIND: &str = "owned_agent";
pub const OWNED_AGENT_RUNTIME_KIND: &str = "owned_agent";
pub const FAKE_PROVIDER_ID: &str = "fake_provider";
const SYSTEM_PROMPT_VERSION: &str = "xero-owned-agent-v1";
const MAX_PROVIDER_TURNS: usize = 32;
const MAX_ROLLBACK_CONTENT_BYTES: u64 = 256 * 1024;
const INTERRUPTED_TOOL_CALL_CODE: &str = "agent_tool_call_interrupted";
const RERUNNABLE_APPROVED_TOOL_ERROR_CODES: &[&str] = &[
    "agent_file_write_requires_observation",
    "agent_file_changed_since_observed",
];
