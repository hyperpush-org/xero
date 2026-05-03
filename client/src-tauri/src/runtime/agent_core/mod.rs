use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

mod context_package;
mod evals;
mod events;
mod provider_adapters;
mod supervisor;

mod persistence;
mod provider_loop;
mod run;
mod state_machine;
mod tool_descriptors;
mod tool_dispatch;
mod types;

pub use evals::{
    run_agent_definition_quality_eval_suite, run_agent_harness_eval_suite,
    run_xero_quality_eval_suites, AgentDefinitionEvalFixtureKind, AgentDefinitionQualityCaseResult,
    AgentDefinitionQualityCoverage, AgentDefinitionQualityEvalReport,
    AgentDefinitionQualityMetrics, AgentDefinitionQualitySurface, AgentDefinitionQualityThresholds,
    AgentHarnessEvalCaseResult, AgentHarnessEvalCoverage, AgentHarnessEvalMetrics,
    AgentHarnessEvalReport, AgentHarnessEvalThresholds, HarnessEvalFixtureKind,
    XeroQualityEvalReport,
};
pub use events::{publish_agent_event, subscribe_agent_events, AgentEventSubscription};
pub use provider_adapters::{
    create_provider_adapter, AgentProviderConfig, AnthropicProviderConfig, BedrockProviderConfig,
    OpenAiCodexResponsesProviderConfig, OpenAiCompatibleProviderConfig,
    OpenAiResponsesProviderConfig, VertexProviderConfig,
};
pub use run::*;
pub use supervisor::{
    cancelled_error, AgentRunCancellationToken, AgentRunLease, AgentRunSupervisor,
    AGENT_RUN_CANCELLED_CODE,
};
pub use types::*;

pub(crate) use context_package::*;
pub(crate) use persistence::*;
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
        estimate_tokens, evaluate_compaction_policy, redact_session_context_text,
        resolve_context_limit, runtime_agent_allows_approval_mode, soul_prompt_fragment,
        BrowserControlPreferenceDto, CommandError, CommandErrorClass, CommandResult,
        RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlInputDto, RuntimeRunControlStateDto, SessionCompactionPolicyInput,
        SessionContextBudgetPressureDto, SessionContextPolicyActionDto, SoulSettingsDto,
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
            emulator::emulator_schema, system_diagnostics_action_approval_id,
            tool_access_all_known_tools, tool_access_group_tools,
            tool_allowed_for_runtime_agent_with_policy, tool_catalog_metadata_for_tool,
            AutonomousAgentToolPolicy, AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX, AUTONOMOUS_TOOL_BROWSER,
            AUTONOMOUS_TOOL_EMULATOR, AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT, AUTONOMOUS_TOOL_SOLANA_ALT,
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
            AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
        },
        redaction::{find_prohibited_persistence_content, redact_command_argv_for_persistence},
        AutonomousDynamicToolRoute, AutonomousMacosAutomationAction,
        AutonomousMacosAutomationOutput, AutonomousMcpAction, AutonomousMcpRequest,
        AutonomousProcessManagerAction, AutonomousSubagentExecutor, AutonomousSubagentTask,
        AutonomousSystemDiagnosticsOutput, AutonomousTodoStatus, AutonomousToolOutput,
        AutonomousToolRequest, AutonomousToolResult, AutonomousToolRuntime,
        XeroSkillToolContextPayload, AUTONOMOUS_TOOL_AGENT_DEFINITION, AUTONOMOUS_TOOL_CODE_INTEL,
        AUTONOMOUS_TOOL_COMMAND, AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
        AUTONOMOUS_TOOL_COMMAND_SESSION_START, AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
        AUTONOMOUS_TOOL_DELETE, AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_FIND,
        AUTONOMOUS_TOOL_GIT_DIFF, AUTONOMOUS_TOOL_GIT_STATUS, AUTONOMOUS_TOOL_HASH,
        AUTONOMOUS_TOOL_LIST, AUTONOMOUS_TOOL_LSP, AUTONOMOUS_TOOL_MACOS_AUTOMATION,
        AUTONOMOUS_TOOL_MCP, AUTONOMOUS_TOOL_MKDIR, AUTONOMOUS_TOOL_NOTEBOOK_EDIT,
        AUTONOMOUS_TOOL_PATCH, AUTONOMOUS_TOOL_POWERSHELL, AUTONOMOUS_TOOL_PROCESS_MANAGER,
        AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_RENAME, AUTONOMOUS_TOOL_SEARCH,
        AUTONOMOUS_TOOL_SKILL, AUTONOMOUS_TOOL_SUBAGENT, AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS,
        AUTONOMOUS_TOOL_TODO, AUTONOMOUS_TOOL_TOOL_ACCESS, AUTONOMOUS_TOOL_TOOL_SEARCH,
        AUTONOMOUS_TOOL_WEB_FETCH, AUTONOMOUS_TOOL_WEB_SEARCH, AUTONOMOUS_TOOL_WRITE,
        OPENAI_CODEX_PROVIDER_ID,
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
