mod agent_coordination;
mod agent_definition;
pub mod browser;
pub mod emulator;
mod environment_context;
mod filesystem;
mod git;
mod macos_automation;
mod policy;
mod priority_tools;
mod process;
mod process_manager;
mod project_context;
mod repo_scope;
mod skills;
pub mod solana;
mod system_diagnostics;
mod workspace_index;

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime};
use xero_agent_core::{
    domain_tool_pack_health_report, domain_tool_pack_ids_for_tool, domain_tool_pack_manifests,
    domain_tool_pack_tools, DomainToolPackHealthInput, DomainToolPackHealthReport,
    DomainToolPackManifest, SandboxExecutionMetadata,
};

use super::autonomous_web_runtime::{
    AutonomousWebConfig, AutonomousWebFetchContentKind, AutonomousWebFetchOutput,
    AutonomousWebFetchRequest, AutonomousWebRuntime, AutonomousWebSearchOutput,
    AutonomousWebSearchRequest, AUTONOMOUS_TOOL_WEB_FETCH, AUTONOMOUS_TOOL_WEB_SEARCH,
};

use super::autonomous_skill_runtime::{
    load_skill_source_settings_from_path, AutonomousSkillRuntime, AutonomousSkillRuntimeConfig,
    XeroSkillSourceKind, XeroSkillSourceState, XeroSkillToolAccessDecision,
    XeroSkillToolContextPayload, XeroSkillToolDiagnostic, XeroSkillToolInput,
    XeroSkillToolLifecycleEvent, XeroSkillToolOperation, XeroSkillTrustState,
};
use crate::{
    commands::{
        browser::load_browser_control_settings, default_soul_settings, load_soul_settings,
        BranchSummaryDto, BrowserControlPreferenceDto, CommandError, CommandResult,
        RepositoryDiffScope, RepositoryStatusEntryDto, ResolvedAgentToolApplicationStyleDto,
        RuntimeAgentIdDto, RuntimeRunApprovalModeDto, RuntimeRunControlStateDto, SoulSettingsDto,
    },
    db::project_store,
    runtime::redaction::find_prohibited_persistence_content,
    runtime::AgentRunCancellationToken,
    state::DesktopState,
};

pub use agent_coordination::{
    AutonomousAgentCoordinationAction, AutonomousAgentCoordinationOutput,
    AutonomousAgentCoordinationRequest,
};
pub use agent_definition::{
    AutonomousAgentAttachableSkillCatalog, AutonomousAgentAttachableSkillDiagnostic,
    AutonomousAgentAttachableSkillEntry, AutonomousAgentDefinitionAction,
    AutonomousAgentDefinitionOutput, AutonomousAgentDefinitionRequest,
    AutonomousAgentDefinitionSummary, AutonomousAgentDefinitionValidationDiagnostic,
    AutonomousAgentDefinitionValidationReport, AutonomousAgentDefinitionValidationStatus,
    AUTONOMOUS_TOOL_AGENT_DEFINITION,
};
pub use browser::{
    AutonomousBrowserAction, AutonomousBrowserOutput, AutonomousBrowserRequest, BrowserExecutor,
    UnavailableBrowserExecutor, AUTONOMOUS_TOOL_BROWSER,
};
pub use emulator::{
    AutonomousEmulatorAction, AutonomousEmulatorOutput, AutonomousEmulatorRequest,
    EmulatorExecutor, UnavailableEmulatorExecutor, AUTONOMOUS_TOOL_EMULATOR,
};
pub use environment_context::{
    AutonomousEnvironmentContextAction, AutonomousEnvironmentContextOutput,
    AutonomousEnvironmentContextRequest,
};
pub use project_context::{
    AutonomousProjectContextAction, AutonomousProjectContextMemory,
    AutonomousProjectContextMemoryKind, AutonomousProjectContextOutput,
    AutonomousProjectContextRecord, AutonomousProjectContextRecordImportance,
    AutonomousProjectContextRecordKind, AutonomousProjectContextRequest,
    AutonomousProjectContextResult,
};
pub use repo_scope::{resolve_imported_repo_root, resolve_imported_repo_root_from_registry};
pub use solana::{
    AutonomousSolanaAltAction, AutonomousSolanaAltRequest, AutonomousSolanaAuditAction,
    AutonomousSolanaAuditRequest, AutonomousSolanaClusterAction, AutonomousSolanaClusterRequest,
    AutonomousSolanaCodamaRequest, AutonomousSolanaCostAction, AutonomousSolanaCostRequest,
    AutonomousSolanaDeployRequest, AutonomousSolanaDocsAction, AutonomousSolanaDocsRequest,
    AutonomousSolanaDriftAction, AutonomousSolanaDriftRequest, AutonomousSolanaExplainRequest,
    AutonomousSolanaIdlAction, AutonomousSolanaIdlRequest, AutonomousSolanaIndexerAction,
    AutonomousSolanaIndexerRequest, AutonomousSolanaLogsAction, AutonomousSolanaLogsRequest,
    AutonomousSolanaOutput, AutonomousSolanaPdaAction, AutonomousSolanaPdaRequest,
    AutonomousSolanaProgramAction, AutonomousSolanaProgramRequest, AutonomousSolanaReplayAction,
    AutonomousSolanaReplayRequest, AutonomousSolanaSecretsAction, AutonomousSolanaSecretsRequest,
    AutonomousSolanaSimulateRequest, AutonomousSolanaSquadsRequest, AutonomousSolanaTxAction,
    AutonomousSolanaTxRequest, AutonomousSolanaUpgradeCheckRequest,
    AutonomousSolanaVerifiedBuildRequest, SolanaExecutor, StateSolanaExecutor,
    UnavailableSolanaExecutor, AUTONOMOUS_TOOL_SOLANA_ALT, AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE,
    AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL, AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ,
    AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC, AUTONOMOUS_TOOL_SOLANA_CLUSTER,
    AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT, AUTONOMOUS_TOOL_SOLANA_CODAMA,
    AUTONOMOUS_TOOL_SOLANA_COST, AUTONOMOUS_TOOL_SOLANA_DEPLOY, AUTONOMOUS_TOOL_SOLANA_DOCS,
    AUTONOMOUS_TOOL_SOLANA_EXPLAIN, AUTONOMOUS_TOOL_SOLANA_IDL, AUTONOMOUS_TOOL_SOLANA_INDEXER,
    AUTONOMOUS_TOOL_SOLANA_LOGS, AUTONOMOUS_TOOL_SOLANA_PDA, AUTONOMOUS_TOOL_SOLANA_PROGRAM,
    AUTONOMOUS_TOOL_SOLANA_REPLAY, AUTONOMOUS_TOOL_SOLANA_SECRETS, AUTONOMOUS_TOOL_SOLANA_SIMULATE,
    AUTONOMOUS_TOOL_SOLANA_SQUADS, AUTONOMOUS_TOOL_SOLANA_TX, AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
    AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
};
pub(crate) use system_diagnostics::system_diagnostics_action_approval_id;
pub use workspace_index::{
    AutonomousWorkspaceIndexAction, AutonomousWorkspaceIndexOutput, AutonomousWorkspaceIndexRequest,
};

pub const AUTONOMOUS_TOOL_READ: &str = "read";
pub const AUTONOMOUS_TOOL_SEARCH: &str = "search";
pub const AUTONOMOUS_TOOL_FIND: &str = "find";
pub const AUTONOMOUS_TOOL_GIT_STATUS: &str = "git_status";
pub const AUTONOMOUS_TOOL_GIT_DIFF: &str = "git_diff";
pub const AUTONOMOUS_TOOL_TOOL_ACCESS: &str = "tool_access";
pub const AUTONOMOUS_TOOL_HARNESS_RUNNER: &str = "harness_runner";
pub const AUTONOMOUS_TOOL_EDIT: &str = "edit";
pub const AUTONOMOUS_TOOL_WRITE: &str = "write";
pub const AUTONOMOUS_TOOL_PATCH: &str = "patch";
pub const AUTONOMOUS_TOOL_DELETE: &str = "delete";
pub const AUTONOMOUS_TOOL_RENAME: &str = "rename";
pub const AUTONOMOUS_TOOL_MKDIR: &str = "mkdir";
pub const AUTONOMOUS_TOOL_LIST: &str = "list";
pub const AUTONOMOUS_TOOL_HASH: &str = "file_hash";
pub const AUTONOMOUS_TOOL_COMMAND: &str = "command";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION_START: &str = "command_session_start";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION_READ: &str = "command_session_read";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION_STOP: &str = "command_session_stop";
pub const AUTONOMOUS_TOOL_COMMAND_PROBE: &str = "command_probe";
pub const AUTONOMOUS_TOOL_COMMAND_VERIFY: &str = "command_verify";
pub const AUTONOMOUS_TOOL_COMMAND_RUN: &str = "command_run";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION: &str = "command_session";
pub const AUTONOMOUS_TOOL_PROCESS_MANAGER: &str = "process_manager";
pub const AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS: &str = "system_diagnostics";
pub const AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE: &str = "system_diagnostics_observe";
pub const AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED: &str = "system_diagnostics_privileged";
pub const AUTONOMOUS_TOOL_MACOS_AUTOMATION: &str = "macos_automation";
pub const AUTONOMOUS_TOOL_MCP: &str = "mcp";
pub const AUTONOMOUS_TOOL_MCP_LIST: &str = "mcp_list";
pub const AUTONOMOUS_TOOL_MCP_READ_RESOURCE: &str = "mcp_read_resource";
pub const AUTONOMOUS_TOOL_MCP_GET_PROMPT: &str = "mcp_get_prompt";
pub const AUTONOMOUS_TOOL_MCP_CALL_TOOL: &str = "mcp_call_tool";
pub const AUTONOMOUS_TOOL_SUBAGENT: &str = "subagent";
pub const AUTONOMOUS_TOOL_TODO: &str = "todo";
pub const AUTONOMOUS_TOOL_NOTEBOOK_EDIT: &str = "notebook_edit";
pub const AUTONOMOUS_TOOL_CODE_INTEL: &str = "code_intel";
pub const AUTONOMOUS_TOOL_LSP: &str = "lsp";
pub const AUTONOMOUS_TOOL_POWERSHELL: &str = "powershell";
pub const AUTONOMOUS_TOOL_TOOL_SEARCH: &str = "tool_search";
pub const AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT: &str = "environment_context";
pub const AUTONOMOUS_TOOL_PROJECT_CONTEXT: &str = "project_context";
pub const AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH: &str = "project_context_search";
pub const AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET: &str = "project_context_get";
pub const AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD: &str = "project_context_record";
pub const AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE: &str = "project_context_update";
pub const AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH: &str = "project_context_refresh";
pub const AUTONOMOUS_TOOL_WORKSPACE_INDEX: &str = "workspace_index";
pub const AUTONOMOUS_TOOL_AGENT_COORDINATION: &str = "agent_coordination";
pub const AUTONOMOUS_TOOL_SKILL: &str = "skill";
pub const AUTONOMOUS_TOOL_BROWSER_OBSERVE: &str = "browser_observe";
pub const AUTONOMOUS_TOOL_BROWSER_CONTROL: &str = "browser_control";
pub const AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX: &str = "mcp__";

const DEFAULT_READ_LINE_COUNT: usize = 200;
const MAX_READ_LINE_COUNT: usize = 400;
const MAX_TEXT_FILE_BYTES: usize = 512 * 1024;
const MAX_SEARCH_QUERY_CHARS: usize = 256;
const MAX_SEARCH_RESULTS: usize = 100;
const MAX_SEARCH_PREVIEW_CHARS: usize = 200;
pub(super) const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 5_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 60_000;
const MAX_COMMAND_CAPTURE_BYTES: usize = 8 * 1024;
const MAX_COMMAND_EXCERPT_CHARS: usize = 2_000;
const DEFAULT_SUBAGENT_MAX_CHILD_AGENTS: usize = 6;
const DEFAULT_SUBAGENT_MAX_DEPTH: usize = 1;
const DEFAULT_SUBAGENT_MAX_CONCURRENT_CHILD_RUNS: usize = 3;
const DEFAULT_SUBAGENT_MAX_DELEGATED_TOOL_CALLS: usize = 40;
const DEFAULT_SUBAGENT_MAX_DELEGATED_TOKENS: u64 = 160_000;
const DEFAULT_SUBAGENT_MAX_DELEGATED_COST_MICROS: u64 = 250_000;

const TOOL_ACCESS_CORE_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_READ,
    AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_GIT_STATUS,
    AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_TOOL_ACCESS,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
    AUTONOMOUS_TOOL_WORKSPACE_INDEX,
    AUTONOMOUS_TOOL_AGENT_COORDINATION,
    AUTONOMOUS_TOOL_TODO,
    AUTONOMOUS_TOOL_LIST,
    AUTONOMOUS_TOOL_HASH,
];
const TOOL_ACCESS_MUTATION_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_EDIT,
    AUTONOMOUS_TOOL_WRITE,
    AUTONOMOUS_TOOL_PATCH,
    AUTONOMOUS_TOOL_DELETE,
    AUTONOMOUS_TOOL_RENAME,
    AUTONOMOUS_TOOL_MKDIR,
];
const TOOL_ACCESS_COMMAND_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_COMMAND_PROBE,
    AUTONOMOUS_TOOL_COMMAND_VERIFY,
    AUTONOMOUS_TOOL_COMMAND_RUN,
    AUTONOMOUS_TOOL_COMMAND_SESSION,
];
const TOOL_ACCESS_PROCESS_MANAGER_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_PROCESS_MANAGER];
const TOOL_ACCESS_SYSTEM_DIAGNOSTICS_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
    AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED,
];
const TOOL_ACCESS_MACOS_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_MACOS_AUTOMATION];
const TOOL_ACCESS_WEB_SEARCH_ONLY_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_WEB_SEARCH];
const TOOL_ACCESS_WEB_FETCH_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_WEB_FETCH];
const TOOL_ACCESS_BROWSER_OBSERVE_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_BROWSER_OBSERVE];
const TOOL_ACCESS_BROWSER_CONTROL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_BROWSER_CONTROL];
const TOOL_ACCESS_WEB_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_WEB_SEARCH,
    AUTONOMOUS_TOOL_WEB_FETCH,
    AUTONOMOUS_TOOL_BROWSER_OBSERVE,
    AUTONOMOUS_TOOL_BROWSER_CONTROL,
];
const TOOL_ACCESS_COMMAND_READONLY_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_COMMAND_PROBE,
    AUTONOMOUS_TOOL_COMMAND_VERIFY,
];
const TOOL_ACCESS_COMMAND_MUTATING_TOOLS: &[&str] =
    &[AUTONOMOUS_TOOL_COMMAND_RUN, AUTONOMOUS_TOOL_COMMAND_SESSION];
const TOOL_ACCESS_COMMAND_SESSION_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_COMMAND_SESSION];
const TOOL_ACCESS_REPOSITORY_RECON_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_READ,
    AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_GIT_STATUS,
    AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_TOOL_ACCESS,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
    AUTONOMOUS_TOOL_WORKSPACE_INDEX,
    AUTONOMOUS_TOOL_LIST,
    AUTONOMOUS_TOOL_HASH,
    AUTONOMOUS_TOOL_COMMAND_PROBE,
    AUTONOMOUS_TOOL_CODE_INTEL,
    AUTONOMOUS_TOOL_LSP,
    AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
    AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
];
const TOOL_ACCESS_PLANNING_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_READ,
    AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_GIT_STATUS,
    AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_TOOL_ACCESS,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
    AUTONOMOUS_TOOL_WORKSPACE_INDEX,
    AUTONOMOUS_TOOL_LIST,
    AUTONOMOUS_TOOL_HASH,
    AUTONOMOUS_TOOL_TODO,
];
const TOOL_ACCESS_EMULATOR_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_EMULATOR];
const TOOL_ACCESS_SOLANA_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_SOLANA_CLUSTER,
    AUTONOMOUS_TOOL_SOLANA_LOGS,
    AUTONOMOUS_TOOL_SOLANA_TX,
    AUTONOMOUS_TOOL_SOLANA_SIMULATE,
    AUTONOMOUS_TOOL_SOLANA_EXPLAIN,
    AUTONOMOUS_TOOL_SOLANA_ALT,
    AUTONOMOUS_TOOL_SOLANA_IDL,
    AUTONOMOUS_TOOL_SOLANA_CODAMA,
    AUTONOMOUS_TOOL_SOLANA_PDA,
    AUTONOMOUS_TOOL_SOLANA_PROGRAM,
    AUTONOMOUS_TOOL_SOLANA_DEPLOY,
    AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
    AUTONOMOUS_TOOL_SOLANA_SQUADS,
    AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
    AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
    AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
    AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ,
    AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE,
    AUTONOMOUS_TOOL_SOLANA_REPLAY,
    AUTONOMOUS_TOOL_SOLANA_INDEXER,
    AUTONOMOUS_TOOL_SOLANA_SECRETS,
    AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
    AUTONOMOUS_TOOL_SOLANA_COST,
    AUTONOMOUS_TOOL_SOLANA_DOCS,
];
const TOOL_ACCESS_AGENT_OPS_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_SUBAGENT];
const TOOL_ACCESS_MCP_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_MCP_LIST,
    AUTONOMOUS_TOOL_MCP_READ_RESOURCE,
    AUTONOMOUS_TOOL_MCP_GET_PROMPT,
    AUTONOMOUS_TOOL_MCP_CALL_TOOL,
];
const TOOL_ACCESS_MCP_LIST_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_MCP_LIST];
const TOOL_ACCESS_MCP_INVOKE_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_MCP_READ_RESOURCE,
    AUTONOMOUS_TOOL_MCP_GET_PROMPT,
    AUTONOMOUS_TOOL_MCP_CALL_TOOL,
];
const TOOL_ACCESS_INTELLIGENCE_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_CODE_INTEL, AUTONOMOUS_TOOL_LSP];
const TOOL_ACCESS_NOTEBOOK_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_NOTEBOOK_EDIT];
const TOOL_ACCESS_POWERSHELL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_POWERSHELL];
const TOOL_ACCESS_ENVIRONMENT_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT];
const TOOL_ACCESS_PROJECT_CONTEXT_WRITE_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH,
];
const TOOL_ACCESS_SKILL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_SKILL];
const TOOL_ACCESS_AGENT_DEFINITION_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_AGENT_DEFINITION];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ToolAccessGroupDefinition {
    name: &'static str,
    description: &'static str,
    tools: &'static [&'static str],
    risk_class: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousToolCatalogEntry {
    pub tool_name: &'static str,
    pub group: &'static str,
    pub description: &'static str,
    pub tags: &'static [&'static str],
    pub schema_fields: &'static [&'static str],
    pub examples: &'static [&'static str],
    pub risk_class: &'static str,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RuntimeHostMetadata {
    pub timestamp_utc: String,
    pub date_utc: String,
    pub operating_system: String,
    pub operating_system_label: String,
    pub architecture: String,
    pub family: String,
}

const TOOL_ACCESS_GROUP_DEFINITIONS: &[ToolAccessGroupDefinition] = &[
    ToolAccessGroupDefinition {
        name: "core",
        description: "Always-on repository inspection, git status, tool discovery, and planning tools.",
        tools: TOOL_ACCESS_CORE_TOOLS,
        risk_class: "observe",
    },
    ToolAccessGroupDefinition {
        name: "mutation",
        description: "Repo-scoped file creation and mutation tools with observation guards.",
        tools: TOOL_ACCESS_MUTATION_TOOLS,
        risk_class: "write",
    },
    ToolAccessGroupDefinition {
        name: "command_readonly",
        description: "Short repo-scoped probe and verification commands with narrowed argv policy.",
        tools: TOOL_ACCESS_COMMAND_READONLY_TOOLS,
        risk_class: "command",
    },
    ToolAccessGroupDefinition {
        name: "command_mutating",
        description: "Repo-scoped commands and command sessions that may change generated files or local state.",
        tools: TOOL_ACCESS_COMMAND_MUTATING_TOOLS,
        risk_class: "command_mutating",
    },
    ToolAccessGroupDefinition {
        name: "command_session",
        description: "Long-running command session start/read/stop tools.",
        tools: TOOL_ACCESS_COMMAND_SESSION_TOOLS,
        risk_class: "long_running_process",
    },
    ToolAccessGroupDefinition {
        name: "command",
        description: "Repo-scoped short and long-running command tools.",
        tools: TOOL_ACCESS_COMMAND_TOOLS,
        risk_class: "command",
    },
    ToolAccessGroupDefinition {
        name: "process_manager",
        description: "Xero-owned process lifecycle, output, and external process observation/control surfaces.",
        tools: TOOL_ACCESS_PROCESS_MANAGER_TOOLS,
        risk_class: "process_control",
    },
    ToolAccessGroupDefinition {
        name: "system_diagnostics",
        description: "Typed, bounded system diagnostics for process open files, resources, threads, logs, sampling, accessibility snapshots, and diagnostic bundles.",
        tools: TOOL_ACCESS_SYSTEM_DIAGNOSTICS_TOOLS,
        risk_class: "system_read",
    },
    ToolAccessGroupDefinition {
        name: "system_diagnostics_observe",
        description: "Read-only typed diagnostics for process open files, resources, threads, logs, and bounded bundles.",
        tools: &[AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE],
        risk_class: "system_read",
    },
    ToolAccessGroupDefinition {
        name: "system_diagnostics_privileged",
        description: "Approval-gated diagnostics actions such as process sampling and macOS accessibility snapshots.",
        tools: &[AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED],
        risk_class: "system_privileged",
    },
    ToolAccessGroupDefinition {
        name: "macos",
        description: "macOS permissions, app/window inspection, screenshots, and approval-gated automation.",
        tools: TOOL_ACCESS_MACOS_TOOLS,
        risk_class: "os_control",
    },
    ToolAccessGroupDefinition {
        name: "web_search_only",
        description: "Search the web without exposing page fetch or browser-control schemas.",
        tools: TOOL_ACCESS_WEB_SEARCH_ONLY_TOOLS,
        risk_class: "network",
    },
    ToolAccessGroupDefinition {
        name: "web_fetch",
        description: "Fetch HTTP/HTTPS text content without exposing browser-control schemas.",
        tools: TOOL_ACCESS_WEB_FETCH_TOOLS,
        risk_class: "network",
    },
    ToolAccessGroupDefinition {
        name: "browser_observe",
        description: "Observe the in-app browser with page text, URL, screenshots, console, network, accessibility, and state reads.",
        tools: TOOL_ACCESS_BROWSER_OBSERVE_TOOLS,
        risk_class: "browser_observe",
    },
    ToolAccessGroupDefinition {
        name: "browser_control",
        description: "Control the in-app browser with navigation, clicks, typing, storage, cookies, and tab actions.",
        tools: TOOL_ACCESS_BROWSER_CONTROL_TOOLS,
        risk_class: "browser_control",
    },
    ToolAccessGroupDefinition {
        name: "web",
        description: "Full web bundle: search, fetch, and in-app browser automation.",
        tools: TOOL_ACCESS_WEB_TOOLS,
        risk_class: "network_browser_control",
    },
    ToolAccessGroupDefinition {
        name: "emulator",
        description: "Mobile emulator/device automation.",
        tools: TOOL_ACCESS_EMULATOR_TOOLS,
        risk_class: "device_control",
    },
    ToolAccessGroupDefinition {
        name: "solana",
        description: "Solana cluster, program, audit, transaction, deploy, and documentation tools.",
        tools: TOOL_ACCESS_SOLANA_TOOLS,
        risk_class: "external_chain",
    },
    ToolAccessGroupDefinition {
        name: "agent_ops",
        description: "Subagent delegation tools; todo and tool_search stay in core.",
        tools: TOOL_ACCESS_AGENT_OPS_TOOLS,
        risk_class: "agent_delegation",
    },
    ToolAccessGroupDefinition {
        name: "mcp_list",
        description: "List connected MCP servers, tools, resources, and prompts.",
        tools: TOOL_ACCESS_MCP_LIST_TOOLS,
        risk_class: "external_capability_observe",
    },
    ToolAccessGroupDefinition {
        name: "mcp_invoke",
        description: "Invoke MCP tools, read resources, and get prompts through the app-local registry.",
        tools: TOOL_ACCESS_MCP_INVOKE_TOOLS,
        risk_class: "external_capability_invoke",
    },
    ToolAccessGroupDefinition {
        name: "mcp",
        description: "Full MCP bundle for listing and invoking connected server capabilities.",
        tools: TOOL_ACCESS_MCP_TOOLS,
        risk_class: "external_capability",
    },
    ToolAccessGroupDefinition {
        name: "project_context_write",
        description: "Runtime-owned app-data durable-context record, update, and freshness mutation actions.",
        tools: TOOL_ACCESS_PROJECT_CONTEXT_WRITE_TOOLS,
        risk_class: "runtime_state",
    },
    ToolAccessGroupDefinition {
        name: "intelligence",
        description: "Code symbols and diagnostics through native code intelligence and LSP.",
        tools: TOOL_ACCESS_INTELLIGENCE_TOOLS,
        risk_class: "observe",
    },
    ToolAccessGroupDefinition {
        name: "notebook",
        description: "Jupyter notebook cell editing.",
        tools: TOOL_ACCESS_NOTEBOOK_TOOLS,
        risk_class: "write",
    },
    ToolAccessGroupDefinition {
        name: "powershell",
        description: "PowerShell commands through repo-scoped command policy.",
        tools: TOOL_ACCESS_POWERSHELL_TOOLS,
        risk_class: "command",
    },
    ToolAccessGroupDefinition {
        name: "environment",
        description: "Read the redacted developer environment profile from app-global state.",
        tools: TOOL_ACCESS_ENVIRONMENT_TOOLS,
        risk_class: "observe",
    },
    ToolAccessGroupDefinition {
        name: "skills",
        description: "Discover, load, install, invoke, reload, and create Xero skills.",
        tools: TOOL_ACCESS_SKILL_TOOLS,
        risk_class: "skill_runtime",
    },
    ToolAccessGroupDefinition {
        name: "agent_builder",
        description: "Draft, validate, list, save, update, archive, and clone registry-backed agent definitions.",
        tools: TOOL_ACCESS_AGENT_DEFINITION_TOOLS,
        risk_class: "agent_definition_state",
    },
];

pub fn tool_access_group_tools(group: &str) -> Option<&'static [&'static str]> {
    tool_access_group_definition(group.trim()).map(|definition| definition.tools)
}

pub fn tool_access_all_known_tools() -> std::collections::BTreeSet<&'static str> {
    TOOL_ACCESS_GROUP_DEFINITIONS
        .iter()
        .flat_map(|definition| definition.tools.iter().copied())
        .collect()
}

pub fn tool_access_group_descriptors() -> Vec<AutonomousToolAccessGroup> {
    TOOL_ACCESS_GROUP_DEFINITIONS
        .iter()
        .map(|definition| AutonomousToolAccessGroup {
            name: definition.name.into(),
            description: definition.description.into(),
            tools: definition
                .tools
                .iter()
                .map(|tool| (*tool).to_owned())
                .collect(),
            risk_class: definition.risk_class.into(),
        })
        .collect()
}

fn tool_access_group_definition(group: &str) -> Option<&'static ToolAccessGroupDefinition> {
    TOOL_ACCESS_GROUP_DEFINITIONS
        .iter()
        .find(|definition| definition.name == group)
}

pub fn tool_catalog_activation_groups(tool_name: &str) -> Vec<String> {
    TOOL_ACCESS_GROUP_DEFINITIONS
        .iter()
        .filter(|definition| definition.tools.contains(&tool_name))
        .map(|definition| definition.name.to_owned())
        .collect()
}

pub fn tool_catalog_metadata_for_tool(
    tool_name: &str,
    skill_tool_enabled: bool,
) -> Option<JsonValue> {
    deferred_tool_catalog(skill_tool_enabled)
        .into_iter()
        .find(|entry| entry.tool_name == tool_name)
        .map(|entry| {
            let tool_pack_ids = domain_tool_pack_ids_for_tool(entry.tool_name);
            let tool_packs = tool_pack_ids
                .iter()
                .filter_map(|pack_id| {
                    xero_agent_core::domain_tool_pack_manifest(pack_id).map(|manifest| {
                        json!({
                            "packId": manifest.pack_id,
                            "label": manifest.label,
                            "policyProfile": manifest.policy_profile,
                        })
                    })
                })
                .collect::<Vec<_>>();
            json!({
                "toolName": entry.tool_name,
                "group": entry.group,
                "toolPackIds": tool_pack_ids,
                "toolPacks": tool_packs,
                "activationGroups": tool_catalog_activation_groups(entry.tool_name),
                "activationTools": [entry.tool_name],
                "tags": entry.tags,
                "schemaFields": entry.schema_fields,
                "examples": entry.examples,
                "riskClass": entry.risk_class,
                "effectClass": tool_effect_class(entry.tool_name).as_str(),
                "allowedRuntimeAgents": allowed_runtime_agent_labels(entry.tool_name),
                "runtimeAvailable": tool_available_on_current_host(entry.tool_name),
            })
        })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutonomousToolEffectClass {
    Observe,
    RuntimeState,
    Write,
    DestructiveWrite,
    Command,
    ProcessControl,
    BrowserControl,
    DeviceControl,
    ExternalService,
    SkillRuntime,
    AgentDelegation,
    Unknown,
}

impl AutonomousToolEffectClass {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::RuntimeState => "runtime_state",
            Self::Write => "write",
            Self::DestructiveWrite => "destructive_write",
            Self::Command => "command",
            Self::ProcessControl => "process_control",
            Self::BrowserControl => "browser_control",
            Self::DeviceControl => "device_control",
            Self::ExternalService => "external_service",
            Self::SkillRuntime => "skill_runtime",
            Self::AgentDelegation => "agent_delegation",
            Self::Unknown => "unknown",
        }
    }

    pub const fn is_ask_observe_only(self) -> bool {
        matches!(self, Self::Observe)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousAgentToolPolicy {
    allowed_effect_classes: BTreeSet<String>,
    allowed_tools: BTreeSet<String>,
    denied_tools: BTreeSet<String>,
    allowed_tool_packs: BTreeSet<String>,
    denied_tool_packs: BTreeSet<String>,
    allowed_mcp_servers: BTreeSet<String>,
    denied_mcp_servers: BTreeSet<String>,
    allowed_dynamic_tools: BTreeSet<String>,
    denied_dynamic_tools: BTreeSet<String>,
    external_service_allowed: bool,
    browser_control_allowed: bool,
    skill_runtime_allowed: bool,
    subagent_allowed: bool,
    allowed_subagent_roles: BTreeSet<String>,
    denied_subagent_roles: BTreeSet<String>,
    command_allowed: bool,
    destructive_write_allowed: bool,
}

impl AutonomousAgentToolPolicy {
    pub fn for_subagent_role(
        role: AutonomousSubagentRole,
        parent_policy: Option<&AutonomousAgentToolPolicy>,
        skill_tool_enabled: bool,
    ) -> Self {
        let role_policy = Self::from_subagent_role(role);
        let allowed_tools = deferred_tool_catalog(skill_tool_enabled)
            .into_iter()
            .map(|entry| entry.tool_name.to_owned())
            .filter(|tool| role_policy.allows_tool(tool))
            .filter(|tool| {
                parent_policy
                    .map(|policy| policy.allows_tool(tool))
                    .unwrap_or(true)
            })
            .collect::<BTreeSet<_>>();
        Self::from_allowed_tools(allowed_tools)
    }

    pub fn intersect_optional(
        left: Option<&AutonomousAgentToolPolicy>,
        right: Option<&AutonomousAgentToolPolicy>,
        skill_tool_enabled: bool,
    ) -> Option<Self> {
        match (left, right) {
            (Some(left), Some(right)) => {
                let allowed_tools = deferred_tool_catalog(skill_tool_enabled)
                    .into_iter()
                    .map(|entry| entry.tool_name.to_owned())
                    .filter(|tool| left.allows_tool(tool) && right.allows_tool(tool))
                    .collect::<BTreeSet<_>>();
                Some(Self::from_allowed_tools(allowed_tools))
            }
            (Some(policy), None) | (None, Some(policy)) => Some(policy.clone()),
            (None, None) => None,
        }
    }

    pub fn from_definition_snapshot(snapshot: &JsonValue) -> Option<Self> {
        let value = snapshot.get("toolPolicy")?;
        if let Some(label) = value.as_str() {
            return Some(Self::from_policy_label(label));
        }
        let object = value.as_object()?;
        let mut allowed_tools = string_set_from_json(object.get("allowedTools"));
        let allowed_tool_packs = string_set_from_json(object.get("allowedToolPacks"));
        for pack_id in &allowed_tool_packs {
            if let Some(tools) = domain_tool_pack_tools(pack_id) {
                allowed_tools.extend(tools);
            }
        }
        for group in string_set_from_json(object.get("allowedToolGroups")) {
            if let Some(tools) = tool_access_group_tools(&group) {
                allowed_tools.extend(tools.iter().map(|tool| (*tool).to_owned()));
            }
        }
        let mut denied_tools = string_set_from_json(object.get("deniedTools"));
        let denied_tool_packs = string_set_from_json(object.get("deniedToolPacks"));
        for pack_id in &denied_tool_packs {
            if let Some(tools) = domain_tool_pack_tools(pack_id) {
                denied_tools.extend(tools);
            }
        }
        Some(Self {
            allowed_effect_classes: string_set_from_json(object.get("allowedEffectClasses")),
            allowed_tools,
            denied_tools,
            allowed_tool_packs,
            denied_tool_packs,
            allowed_mcp_servers: string_set_from_json(object.get("allowedMcpServers")),
            denied_mcp_servers: string_set_from_json(object.get("deniedMcpServers")),
            allowed_dynamic_tools: string_set_from_json(object.get("allowedDynamicTools")),
            denied_dynamic_tools: string_set_from_json(object.get("deniedDynamicTools")),
            external_service_allowed: json_bool(object.get("externalServiceAllowed")),
            browser_control_allowed: json_bool(object.get("browserControlAllowed")),
            skill_runtime_allowed: json_bool(object.get("skillRuntimeAllowed")),
            subagent_allowed: json_bool(object.get("subagentAllowed")),
            allowed_subagent_roles: string_set_from_json(object.get("allowedSubagentRoles")),
            denied_subagent_roles: string_set_from_json(object.get("deniedSubagentRoles")),
            command_allowed: json_bool(object.get("commandAllowed")),
            destructive_write_allowed: json_bool(object.get("destructiveWriteAllowed")),
        })
    }

    fn from_policy_label(label: &str) -> Self {
        match label.trim() {
            "engineering" => Self {
                allowed_effect_classes: [
                    "observe",
                    "runtime_state",
                    "write",
                    "destructive_write",
                    "command",
                    "process_control",
                ]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
                allowed_tools: BTreeSet::new(),
                denied_tools: BTreeSet::new(),
                allowed_tool_packs: BTreeSet::new(),
                denied_tool_packs: BTreeSet::new(),
                allowed_mcp_servers: BTreeSet::new(),
                denied_mcp_servers: BTreeSet::new(),
                allowed_dynamic_tools: BTreeSet::new(),
                denied_dynamic_tools: BTreeSet::new(),
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
                allowed_subagent_roles: BTreeSet::new(),
                denied_subagent_roles: BTreeSet::new(),
                command_allowed: true,
                destructive_write_allowed: true,
            },
            "agent_builder" => Self {
                allowed_effect_classes: ["observe", "runtime_state"]
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect(),
                allowed_tools: [AUTONOMOUS_TOOL_AGENT_DEFINITION.to_string()].into(),
                denied_tools: BTreeSet::new(),
                allowed_tool_packs: BTreeSet::new(),
                denied_tool_packs: BTreeSet::new(),
                allowed_mcp_servers: BTreeSet::new(),
                denied_mcp_servers: BTreeSet::new(),
                allowed_dynamic_tools: BTreeSet::new(),
                denied_dynamic_tools: BTreeSet::new(),
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
                allowed_subagent_roles: BTreeSet::new(),
                denied_subagent_roles: BTreeSet::new(),
                command_allowed: false,
                destructive_write_allowed: false,
            },
            "repository_recon" => Self {
                allowed_effect_classes: BTreeSet::new(),
                allowed_tools: TOOL_ACCESS_REPOSITORY_RECON_TOOLS
                    .iter()
                    .map(|tool| (*tool).to_owned())
                    .collect(),
                denied_tools: BTreeSet::new(),
                allowed_tool_packs: BTreeSet::new(),
                denied_tool_packs: BTreeSet::new(),
                allowed_mcp_servers: BTreeSet::new(),
                denied_mcp_servers: BTreeSet::new(),
                allowed_dynamic_tools: BTreeSet::new(),
                denied_dynamic_tools: BTreeSet::new(),
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
                allowed_subagent_roles: BTreeSet::new(),
                denied_subagent_roles: BTreeSet::new(),
                command_allowed: true,
                destructive_write_allowed: false,
            },
            "planning" => Self {
                allowed_effect_classes: BTreeSet::new(),
                allowed_tools: TOOL_ACCESS_PLANNING_TOOLS
                    .iter()
                    .map(|tool| (*tool).to_owned())
                    .collect(),
                denied_tools: BTreeSet::new(),
                allowed_tool_packs: BTreeSet::new(),
                denied_tool_packs: BTreeSet::new(),
                allowed_mcp_servers: BTreeSet::new(),
                denied_mcp_servers: BTreeSet::new(),
                allowed_dynamic_tools: BTreeSet::new(),
                denied_dynamic_tools: BTreeSet::new(),
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
                allowed_subagent_roles: BTreeSet::new(),
                denied_subagent_roles: BTreeSet::new(),
                command_allowed: false,
                destructive_write_allowed: false,
            },
            _ => Self {
                allowed_effect_classes: ["observe"].into_iter().map(ToOwned::to_owned).collect(),
                allowed_tools: BTreeSet::new(),
                denied_tools: BTreeSet::new(),
                allowed_tool_packs: BTreeSet::new(),
                denied_tool_packs: BTreeSet::new(),
                allowed_mcp_servers: BTreeSet::new(),
                denied_mcp_servers: BTreeSet::new(),
                allowed_dynamic_tools: BTreeSet::new(),
                denied_dynamic_tools: BTreeSet::new(),
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
                allowed_subagent_roles: BTreeSet::new(),
                denied_subagent_roles: BTreeSet::new(),
                command_allowed: false,
                destructive_write_allowed: false,
            },
        }
    }

    fn from_subagent_role(role: AutonomousSubagentRole) -> Self {
        match role {
            AutonomousSubagentRole::Engineer => Self::from_groups_and_tools(
                &[
                    "core",
                    "mutation",
                    "command_readonly",
                    "intelligence",
                    "environment",
                ],
                &[],
                &[
                    "observe",
                    "runtime_state",
                    "write",
                    "destructive_write",
                    "command",
                ],
                RiskyToolOptIns {
                    command_allowed: true,
                    destructive_write_allowed: true,
                    ..RiskyToolOptIns::default()
                },
            ),
            AutonomousSubagentRole::Debugger => Self::from_groups_and_tools(
                &[
                    "core",
                    "mutation",
                    "command_readonly",
                    "command_session",
                    "intelligence",
                    "system_diagnostics",
                    "environment",
                ],
                &[],
                &[
                    "observe",
                    "runtime_state",
                    "write",
                    "destructive_write",
                    "command",
                    "process_control",
                ],
                RiskyToolOptIns {
                    command_allowed: true,
                    destructive_write_allowed: true,
                    ..RiskyToolOptIns::default()
                },
            ),
            AutonomousSubagentRole::Planner
            | AutonomousSubagentRole::Researcher
            | AutonomousSubagentRole::Reviewer
            | AutonomousSubagentRole::Database => Self::from_groups_and_tools(
                &["core", "intelligence", "environment"],
                &[],
                &["observe", "runtime_state"],
                RiskyToolOptIns::default(),
            ),
            AutonomousSubagentRole::AgentBuilder => Self::from_groups_and_tools(
                &["core", "agent_builder"],
                &[],
                &["observe", "runtime_state"],
                RiskyToolOptIns::default(),
            ),
            AutonomousSubagentRole::Browser => Self::from_groups_and_tools(
                &["core", "web", "browser_observe", "intelligence"],
                &[],
                &[
                    "observe",
                    "runtime_state",
                    "browser_control",
                    "external_service",
                ],
                RiskyToolOptIns {
                    browser_control_allowed: true,
                    external_service_allowed: true,
                    ..RiskyToolOptIns::default()
                },
            ),
            AutonomousSubagentRole::Emulator => Self::from_groups_and_tools(
                &["core", "emulator", "intelligence"],
                &[],
                &["observe", "runtime_state", "device_control"],
                RiskyToolOptIns::default(),
            ),
            AutonomousSubagentRole::Solana => Self::from_groups_and_tools(
                &["core", "solana", "intelligence"],
                &[],
                &["observe", "runtime_state", "external_service", "command"],
                RiskyToolOptIns {
                    external_service_allowed: true,
                    command_allowed: true,
                    ..RiskyToolOptIns::default()
                },
            ),
        }
    }

    fn from_groups_and_tools(
        groups: &[&str],
        tools: &[&str],
        _effect_classes: &[&str],
        opt_ins: RiskyToolOptIns,
    ) -> Self {
        let mut allowed_tools = tools
            .iter()
            .map(|tool| (*tool).to_owned())
            .collect::<BTreeSet<_>>();
        for group in groups {
            if let Some(group_tools) = tool_access_group_tools(group) {
                allowed_tools.extend(group_tools.iter().map(|tool| (*tool).to_owned()));
            }
        }
        Self {
            allowed_effect_classes: BTreeSet::new(),
            allowed_tools,
            denied_tools: BTreeSet::new(),
            allowed_tool_packs: BTreeSet::new(),
            denied_tool_packs: BTreeSet::new(),
            allowed_mcp_servers: BTreeSet::new(),
            denied_mcp_servers: BTreeSet::new(),
            allowed_dynamic_tools: BTreeSet::new(),
            denied_dynamic_tools: BTreeSet::new(),
            external_service_allowed: opt_ins.external_service_allowed,
            browser_control_allowed: opt_ins.browser_control_allowed,
            skill_runtime_allowed: opt_ins.skill_runtime_allowed,
            subagent_allowed: opt_ins.subagent_allowed,
            allowed_subagent_roles: BTreeSet::new(),
            denied_subagent_roles: BTreeSet::new(),
            command_allowed: opt_ins.command_allowed,
            destructive_write_allowed: opt_ins.destructive_write_allowed,
        }
    }

    fn from_allowed_tools(allowed_tools: BTreeSet<String>) -> Self {
        let mut policy = Self {
            allowed_effect_classes: BTreeSet::new(),
            allowed_tools,
            denied_tools: BTreeSet::new(),
            allowed_tool_packs: BTreeSet::new(),
            denied_tool_packs: BTreeSet::new(),
            allowed_mcp_servers: BTreeSet::new(),
            denied_mcp_servers: BTreeSet::new(),
            allowed_dynamic_tools: BTreeSet::new(),
            denied_dynamic_tools: BTreeSet::new(),
            external_service_allowed: false,
            browser_control_allowed: false,
            skill_runtime_allowed: false,
            subagent_allowed: false,
            allowed_subagent_roles: BTreeSet::new(),
            denied_subagent_roles: BTreeSet::new(),
            command_allowed: false,
            destructive_write_allowed: false,
        };
        for tool in policy.allowed_tools.iter() {
            match tool_effect_class(tool) {
                AutonomousToolEffectClass::ExternalService => {
                    policy.external_service_allowed = true;
                }
                AutonomousToolEffectClass::BrowserControl => {
                    policy.browser_control_allowed = true;
                }
                AutonomousToolEffectClass::SkillRuntime => {
                    policy.skill_runtime_allowed = true;
                }
                AutonomousToolEffectClass::AgentDelegation => {
                    policy.subagent_allowed = true;
                }
                AutonomousToolEffectClass::Command | AutonomousToolEffectClass::ProcessControl => {
                    policy.command_allowed = true;
                }
                AutonomousToolEffectClass::DestructiveWrite => {
                    policy.destructive_write_allowed = true;
                }
                _ => {}
            }
        }
        policy
    }

    pub fn allows_tool(&self, tool_name: &str) -> bool {
        if self.denied_tools.contains(tool_name) {
            return false;
        }
        if tool_name.starts_with(AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX)
            && !self.allows_dynamic_tool_name(tool_name)
        {
            return false;
        }
        if self.allowed_tools.contains(tool_name) {
            return self.risky_effect_opted_in(tool_effect_class(tool_name));
        }
        let effect_class = tool_effect_class(tool_name);
        self.allowed_effect_classes.contains(effect_class.as_str())
            && self.risky_effect_opted_in(effect_class)
    }

    fn risky_effect_opted_in(&self, effect_class: AutonomousToolEffectClass) -> bool {
        match effect_class {
            AutonomousToolEffectClass::ExternalService => self.external_service_allowed,
            AutonomousToolEffectClass::BrowserControl => self.browser_control_allowed,
            AutonomousToolEffectClass::SkillRuntime => self.skill_runtime_allowed,
            AutonomousToolEffectClass::AgentDelegation => self.subagent_allowed,
            AutonomousToolEffectClass::Command | AutonomousToolEffectClass::ProcessControl => {
                self.command_allowed
            }
            AutonomousToolEffectClass::DestructiveWrite => self.destructive_write_allowed,
            _ => true,
        }
    }

    pub fn allows_mcp_server(&self, server_id: &str) -> bool {
        let server_id = server_id.trim();
        if server_id.is_empty() {
            return true;
        }
        if self.denied_mcp_servers.contains(server_id) {
            return false;
        }
        self.allowed_mcp_servers.is_empty() || self.allowed_mcp_servers.contains(server_id)
    }

    pub fn allows_mcp_request(&self, request: &AutonomousMcpRequest) -> bool {
        request
            .server_id
            .as_deref()
            .map(|server_id| self.allows_mcp_server(server_id))
            .unwrap_or(true)
    }

    pub fn allows_dynamic_tool_descriptor(
        &self,
        descriptor: &AutonomousDynamicToolDescriptor,
    ) -> bool {
        self.allows_dynamic_tool_name(&descriptor.name)
            && self.allows_mcp_server(&descriptor.server_id)
    }

    pub fn allows_dynamic_tool_route(
        &self,
        tool_name: &str,
        route: &AutonomousDynamicToolRoute,
    ) -> bool {
        if !self.allows_dynamic_tool_name(tool_name) {
            return false;
        }
        match route {
            AutonomousDynamicToolRoute::McpTool { server_id, .. } => {
                self.allows_mcp_server(server_id)
            }
        }
    }

    fn allows_dynamic_tool_name(&self, tool_name: &str) -> bool {
        if self.denied_dynamic_tools.contains(tool_name) {
            return false;
        }
        self.allowed_dynamic_tools.is_empty() || self.allowed_dynamic_tools.contains(tool_name)
    }

    pub fn allows_subagent_role(&self, role: AutonomousSubagentRole) -> bool {
        if !self.allows_tool(AUTONOMOUS_TOOL_SUBAGENT) {
            return false;
        }
        let role_id = role.as_str();
        if self.denied_subagent_roles.contains(role_id) {
            return false;
        }
        !self.allowed_subagent_roles.is_empty() && self.allowed_subagent_roles.contains(role_id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousAgentWorkflowPolicy {
    start_phase_id: String,
    phases: Vec<AutonomousAgentWorkflowPhase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutonomousAgentWorkflowPhase {
    id: String,
    title: String,
    allowed_tools: BTreeSet<String>,
    required_checks: Vec<AutonomousAgentWorkflowCondition>,
    retry_limit: Option<usize>,
    branches: Vec<AutonomousAgentWorkflowBranch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AutonomousAgentWorkflowBranch {
    target_phase_id: String,
    condition: AutonomousAgentWorkflowCondition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum AutonomousAgentWorkflowCondition {
    Always,
    TodoCompleted { todo_id: String },
    ToolSucceeded { tool_name: String, min_count: usize },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AutonomousAgentWorkflowRuntimeState {
    current_phase_id: String,
    tool_successes: BTreeMap<String, usize>,
    phase_failures: BTreeMap<String, usize>,
}

impl AutonomousAgentWorkflowPolicy {
    pub fn from_definition_snapshot(snapshot: &JsonValue) -> Option<Self> {
        let object = snapshot.get("workflowStructure")?.as_object()?;
        let phases_json = object.get("phases")?.as_array()?;
        let phases = phases_json
            .iter()
            .filter_map(AutonomousAgentWorkflowPhase::from_json)
            .collect::<Vec<_>>();
        if phases.is_empty() {
            return None;
        }
        let start_phase_id = object
            .get("startPhaseId")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| phases[0].id.clone());
        phases
            .iter()
            .any(|phase| phase.id == start_phase_id)
            .then_some(Self {
                start_phase_id,
                phases,
            })
    }

    fn initial_state(&self) -> AutonomousAgentWorkflowRuntimeState {
        AutonomousAgentWorkflowRuntimeState {
            current_phase_id: self.start_phase_id.clone(),
            tool_successes: BTreeMap::new(),
            phase_failures: BTreeMap::new(),
        }
    }

    fn phase(&self, phase_id: &str) -> Option<&AutonomousAgentWorkflowPhase> {
        self.phases.iter().find(|phase| phase.id == phase_id)
    }

    fn next_sequential_phase(&self, phase_id: &str) -> Option<&AutonomousAgentWorkflowPhase> {
        let index = self.phases.iter().position(|phase| phase.id == phase_id)?;
        self.phases.get(index.saturating_add(1))
    }

    fn advance_state(
        &self,
        state: &mut AutonomousAgentWorkflowRuntimeState,
        todos: &BTreeMap<String, AutonomousTodoItem>,
    ) {
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(state.current_phase_id.clone()) {
                break;
            }
            let Some(phase) = self.phase(&state.current_phase_id) else {
                state.current_phase_id = self.start_phase_id.clone();
                break;
            };
            if !phase.required_checks_satisfied(state, todos) {
                break;
            }
            if let Some(branch) = phase
                .branches
                .iter()
                .find(|branch| branch.condition.satisfied(state, todos))
            {
                if branch.target_phase_id == phase.id {
                    break;
                }
                state.current_phase_id = branch.target_phase_id.clone();
                continue;
            }
            let Some(next_phase) = self.next_sequential_phase(&phase.id) else {
                break;
            };
            state.current_phase_id = next_phase.id.clone();
        }
    }
}

impl AutonomousAgentWorkflowPhase {
    fn from_json(value: &JsonValue) -> Option<Self> {
        let object = value.as_object()?;
        let id = json_non_empty_string(object.get("id"))?;
        let title = json_non_empty_string(object.get("title")).unwrap_or_else(|| id.clone());
        let allowed_tools = object
            .get("allowedTools")
            .and_then(JsonValue::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| json_non_empty_string(Some(item)))
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        let required_checks = object
            .get("requiredChecks")
            .and_then(JsonValue::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(AutonomousAgentWorkflowCondition::from_json)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let retry_limit = object
            .get("retryLimit")
            .and_then(JsonValue::as_u64)
            .map(|value| value as usize);
        let branches = object
            .get("branches")
            .and_then(JsonValue::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(AutonomousAgentWorkflowBranch::from_json)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        Some(Self {
            id,
            title,
            allowed_tools,
            required_checks,
            retry_limit,
            branches,
        })
    }

    fn required_checks_satisfied(
        &self,
        state: &AutonomousAgentWorkflowRuntimeState,
        todos: &BTreeMap<String, AutonomousTodoItem>,
    ) -> bool {
        self.required_checks
            .iter()
            .all(|condition| condition.satisfied(state, todos))
    }

    fn allows_tool(&self, tool_name: &str) -> bool {
        self.allowed_tools.is_empty()
            || self.allowed_tools.contains(tool_name)
            || matches!(
                tool_name,
                AUTONOMOUS_TOOL_TODO | AUTONOMOUS_TOOL_TOOL_SEARCH | AUTONOMOUS_TOOL_TOOL_ACCESS
            )
    }
}

impl AutonomousAgentWorkflowBranch {
    fn from_json(value: &JsonValue) -> Option<Self> {
        let object = value.as_object()?;
        Some(Self {
            target_phase_id: json_non_empty_string(object.get("targetPhaseId"))?,
            condition: AutonomousAgentWorkflowCondition::from_json(object.get("condition")?)?,
        })
    }
}

impl AutonomousAgentWorkflowCondition {
    fn from_json(value: &JsonValue) -> Option<Self> {
        let object = value.as_object()?;
        match object.get("kind")?.as_str()?.trim() {
            "always" => Some(Self::Always),
            "todo_completed" => Some(Self::TodoCompleted {
                todo_id: normalize_workflow_id(object.get("todoId")?)?,
            }),
            "tool_succeeded" => Some(Self::ToolSucceeded {
                tool_name: json_non_empty_string(object.get("toolName"))?,
                min_count: object
                    .get("minCount")
                    .and_then(JsonValue::as_u64)
                    .filter(|count| *count > 0)
                    .unwrap_or(1) as usize,
            }),
            _ => None,
        }
    }

    fn satisfied(
        &self,
        state: &AutonomousAgentWorkflowRuntimeState,
        todos: &BTreeMap<String, AutonomousTodoItem>,
    ) -> bool {
        match self {
            Self::Always => true,
            Self::TodoCompleted { todo_id } => todos
                .get(todo_id)
                .is_some_and(|item| item.status == AutonomousTodoStatus::Completed),
            Self::ToolSucceeded {
                tool_name,
                min_count,
            } => state.tool_successes.get(tool_name).copied().unwrap_or(0) >= *min_count,
        }
    }
}

fn json_non_empty_string(value: Option<&JsonValue>) -> Option<String> {
    value
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_workflow_id(value: &JsonValue) -> Option<String> {
    json_non_empty_string(Some(value)).and_then(|value| {
        let normalized = value
            .trim()
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.') {
                    character.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .trim_matches('-')
            .to_string();
        (!normalized.is_empty()).then_some(normalized)
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct RiskyToolOptIns {
    external_service_allowed: bool,
    browser_control_allowed: bool,
    skill_runtime_allowed: bool,
    subagent_allowed: bool,
    command_allowed: bool,
    destructive_write_allowed: bool,
}

fn string_set_from_json(value: Option<&JsonValue>) -> BTreeSet<String> {
    value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn json_bool(value: Option<&JsonValue>) -> bool {
    value.and_then(JsonValue::as_bool).unwrap_or(false)
}

fn executable_on_path(command: &str) -> bool {
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

pub fn runtime_host_metadata() -> RuntimeHostMetadata {
    let timestamp_utc = crate::auth::now_timestamp();
    let date_utc = timestamp_utc
        .split_once('T')
        .map(|(date, _)| date)
        .unwrap_or(timestamp_utc.as_str())
        .to_owned();
    RuntimeHostMetadata {
        timestamp_utc,
        date_utc,
        operating_system: env::consts::OS.into(),
        operating_system_label: current_host_os_label().into(),
        architecture: env::consts::ARCH.into(),
        family: env::consts::FAMILY.into(),
    }
}

fn current_host_os_label() -> &'static str {
    match env::consts::OS {
        "macos" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        "ios" => "iOS",
        "android" => "Android",
        _ => "Other",
    }
}

pub fn tool_available_on_current_host(tool: &str) -> bool {
    match tool {
        AUTONOMOUS_TOOL_MACOS_AUTOMATION | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED => {
            cfg!(target_os = "macos")
        }
        AUTONOMOUS_TOOL_POWERSHELL => cfg!(target_os = "windows"),
        _ => true,
    }
}

pub fn tool_effect_class(tool_name: &str) -> AutonomousToolEffectClass {
    if tool_name.starts_with(AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX) {
        return AutonomousToolEffectClass::ExternalService;
    }
    match tool_name {
        AUTONOMOUS_TOOL_READ
        | AUTONOMOUS_TOOL_SEARCH
        | AUTONOMOUS_TOOL_FIND
        | AUTONOMOUS_TOOL_GIT_STATUS
        | AUTONOMOUS_TOOL_GIT_DIFF
        | AUTONOMOUS_TOOL_LIST
        | AUTONOMOUS_TOOL_HASH
        | AUTONOMOUS_TOOL_HARNESS_RUNNER
        | AUTONOMOUS_TOOL_CODE_INTEL
        | AUTONOMOUS_TOOL_LSP
        | AUTONOMOUS_TOOL_TOOL_SEARCH
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET
        | AUTONOMOUS_TOOL_WORKSPACE_INDEX
        | AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT
        | AUTONOMOUS_TOOL_BROWSER_OBSERVE
        | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE => AutonomousToolEffectClass::Observe,
        AUTONOMOUS_TOOL_TOOL_ACCESS
        | AUTONOMOUS_TOOL_TODO
        | AUTONOMOUS_TOOL_AGENT_COORDINATION
        | AUTONOMOUS_TOOL_AGENT_DEFINITION
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH => AutonomousToolEffectClass::RuntimeState,
        AUTONOMOUS_TOOL_WRITE
        | AUTONOMOUS_TOOL_EDIT
        | AUTONOMOUS_TOOL_PATCH
        | AUTONOMOUS_TOOL_RENAME
        | AUTONOMOUS_TOOL_MKDIR
        | AUTONOMOUS_TOOL_NOTEBOOK_EDIT => AutonomousToolEffectClass::Write,
        AUTONOMOUS_TOOL_DELETE => AutonomousToolEffectClass::DestructiveWrite,
        AUTONOMOUS_TOOL_COMMAND
        | AUTONOMOUS_TOOL_COMMAND_PROBE
        | AUTONOMOUS_TOOL_COMMAND_VERIFY
        | AUTONOMOUS_TOOL_COMMAND_RUN
        | AUTONOMOUS_TOOL_COMMAND_SESSION
        | AUTONOMOUS_TOOL_COMMAND_SESSION_START
        | AUTONOMOUS_TOOL_COMMAND_SESSION_READ
        | AUTONOMOUS_TOOL_COMMAND_SESSION_STOP
        | AUTONOMOUS_TOOL_POWERSHELL => AutonomousToolEffectClass::Command,
        AUTONOMOUS_TOOL_PROCESS_MANAGER
        | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS
        | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED
        | AUTONOMOUS_TOOL_MACOS_AUTOMATION => AutonomousToolEffectClass::ProcessControl,
        AUTONOMOUS_TOOL_WEB_SEARCH
        | AUTONOMOUS_TOOL_WEB_FETCH
        | AUTONOMOUS_TOOL_MCP
        | AUTONOMOUS_TOOL_MCP_LIST
        | AUTONOMOUS_TOOL_MCP_READ_RESOURCE
        | AUTONOMOUS_TOOL_MCP_GET_PROMPT
        | AUTONOMOUS_TOOL_MCP_CALL_TOOL => AutonomousToolEffectClass::ExternalService,
        AUTONOMOUS_TOOL_BROWSER | AUTONOMOUS_TOOL_BROWSER_CONTROL => {
            AutonomousToolEffectClass::BrowserControl
        }
        AUTONOMOUS_TOOL_EMULATOR => AutonomousToolEffectClass::DeviceControl,
        AUTONOMOUS_TOOL_SUBAGENT => AutonomousToolEffectClass::AgentDelegation,
        AUTONOMOUS_TOOL_SKILL => AutonomousToolEffectClass::SkillRuntime,
        AUTONOMOUS_TOOL_SOLANA_LOGS
        | AUTONOMOUS_TOOL_SOLANA_TX
        | AUTONOMOUS_TOOL_SOLANA_EXPLAIN
        | AUTONOMOUS_TOOL_SOLANA_ALT
        | AUTONOMOUS_TOOL_SOLANA_IDL
        | AUTONOMOUS_TOOL_SOLANA_PDA
        | AUTONOMOUS_TOOL_SOLANA_PROGRAM
        | AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK
        | AUTONOMOUS_TOOL_SOLANA_SQUADS
        | AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE
        | AUTONOMOUS_TOOL_SOLANA_SECRETS
        | AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT
        | AUTONOMOUS_TOOL_SOLANA_COST
        | AUTONOMOUS_TOOL_SOLANA_DOCS
        | AUTONOMOUS_TOOL_SOLANA_CLUSTER
        | AUTONOMOUS_TOOL_SOLANA_SIMULATE
        | AUTONOMOUS_TOOL_SOLANA_REPLAY
        | AUTONOMOUS_TOOL_SOLANA_INDEXER
        | AUTONOMOUS_TOOL_SOLANA_CODAMA
        | AUTONOMOUS_TOOL_SOLANA_DEPLOY
        | AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD
        | AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC
        | AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL
        | AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ => AutonomousToolEffectClass::ExternalService,
        _ => AutonomousToolEffectClass::Unknown,
    }
}

pub fn tool_allowed_for_runtime_agent(agent_id: RuntimeAgentIdDto, tool_name: &str) -> bool {
    if tool_name == AUTONOMOUS_TOOL_HARNESS_RUNNER {
        return false;
    }
    if tool_name == AUTONOMOUS_TOOL_AGENT_DEFINITION {
        return agent_id == RuntimeAgentIdDto::AgentCreate;
    }
    match agent_id {
        RuntimeAgentIdDto::Engineer | RuntimeAgentIdDto::Debug => true,
        RuntimeAgentIdDto::Plan => TOOL_ACCESS_PLANNING_TOOLS.contains(&tool_name),
        RuntimeAgentIdDto::Crawl => TOOL_ACCESS_REPOSITORY_RECON_TOOLS.contains(&tool_name),
        RuntimeAgentIdDto::Ask | RuntimeAgentIdDto::AgentCreate => {
            matches!(tool_name, AUTONOMOUS_TOOL_TOOL_ACCESS)
                || tool_effect_class(tool_name).is_ask_observe_only()
        }
    }
}

pub fn tool_allowed_for_runtime_agent_with_policy(
    agent_id: RuntimeAgentIdDto,
    tool_name: &str,
    agent_tool_policy: Option<&AutonomousAgentToolPolicy>,
) -> bool {
    tool_allowed_for_runtime_agent(agent_id, tool_name)
        && agent_tool_policy
            .map(|policy| policy.allows_tool(tool_name))
            .unwrap_or(true)
}

pub fn allowed_runtime_agent_labels(tool_name: &str) -> Vec<&'static str> {
    let mut agents = Vec::new();
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Ask, tool_name) {
        agents.push(RuntimeAgentIdDto::Ask.as_str());
    }
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Plan, tool_name) {
        agents.push(RuntimeAgentIdDto::Plan.as_str());
    }
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Engineer, tool_name) {
        agents.push(RuntimeAgentIdDto::Engineer.as_str());
    }
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Debug, tool_name) {
        agents.push(RuntimeAgentIdDto::Debug.as_str());
    }
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Crawl, tool_name) {
        agents.push(RuntimeAgentIdDto::Crawl.as_str());
    }
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::AgentCreate, tool_name) {
        agents.push(RuntimeAgentIdDto::AgentCreate.as_str());
    }
    agents
}

pub fn deferred_tool_catalog(skill_tool_enabled: bool) -> Vec<AutonomousToolCatalogEntry> {
    let mut catalog = vec![
        catalog_entry(
            AUTONOMOUS_TOOL_READ,
            "core",
            "Read a repo-relative file as text, image preview, binary metadata, byte range, or line-hash anchored text.",
            &["file", "inspect", "read", "line_hash", "image", "binary"],
            &["path", "systemPath", "mode", "startLine", "lineCount", "byteOffset", "byteCount", "includeLineHashes"],
            &["Read src/lib.rs with line hashes before editing.", "Inspect an image preview in the imported repo."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_SEARCH,
            "core",
            "Search repo-scoped files with regex or literal matching, globs, context lines, hidden/ignored controls, and deterministic capped results.",
            &["file", "search", "regex", "grep", "ripgrep", "code"],
            &["query", "path", "regex", "ignoreCase", "includeHidden", "includeIgnored", "includeGlobs", "excludeGlobs", "contextLines", "maxResults"],
            &["Search for a symbol before editing.", "Find TODO references with context lines."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_FIND,
            "core",
            "Find glob/pattern matches in repo-scoped files with optional bounded recursion depth.",
            &["file", "glob", "find", "tree"],
            &["pattern", "path", "maxDepth"],
            &[
                "Find **/*.rs files under src-tauri.",
                "Find top-level package manifests with maxDepth 2.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_GIT_STATUS,
            "core",
            "Inspect repository status.",
            &["git", "status", "dirty_worktree"],
            &[],
            &["Check dirty worktree state before editing."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_GIT_DIFF,
            "core",
            "Inspect repository diffs.",
            &["git", "diff", "changes", "review"],
            &["scope"],
            &["Review unstaged changes before final summary."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            "core",
            "Search source-cited durable project records, approved memory, handoffs, decisions, constraints, questions, blockers, and current context manifests with freshness evidence.",
            &["context", "memory", "records", "handoff", "retrieval", "citations"],
            &[
                "action",
                "query",
                "recordKinds",
                "memoryKinds",
                "tags",
                "relatedPaths",
                "limit",
            ],
            &["Search project records before prior-work-sensitive tasks."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
            "core",
            "Read one source-cited durable project record or approved memory item by id.",
            &["context", "memory", "records", "read", "citations"],
            &["action", "recordId", "memoryId"],
            &["Read a cited prior decision by project record id."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            "project_context_write",
            "Record or propose runtime-owned durable project context in OS app-data state.",
            &["context", "memory", "record", "app_data", "runtime_state"],
            &[
                "action",
                "title",
                "summary",
                "text",
                "recordKind",
                "importance",
                "confidence",
                "tags",
                "relatedPaths",
                "sourceItemIds",
                "contentJson",
            ],
            &["Record a durable finding after verification."],
            "runtime_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE,
            "project_context_write",
            "Update runtime-owned durable project context or approved memory in OS app-data state.",
            &["context", "memory", "update", "app_data", "runtime_state"],
            &[
                "recordId",
                "memoryId",
                "title",
                "summary",
                "text",
                "recordKind",
                "importance",
                "confidence",
                "tags",
                "relatedPaths",
                "sourceItemIds",
                "contentJson",
            ],
            &["Update stale context after checking current file evidence."],
            "runtime_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH,
            "project_context_write",
            "Refresh durable-context freshness evidence for specific project records or approved memory ids.",
            &["context", "freshness", "app_data", "runtime_state"],
            &["recordId", "memoryId", "recordIds", "memoryIds"],
            &["Refresh stale project-context evidence after inspecting files."],
            "runtime_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_WORKSPACE_INDEX,
            "core",
            "Query the local app-data semantic workspace index for code search, symbol lookup, related tests, change impact, and index freshness.",
            &[
                "workspace",
                "semantic",
                "index",
                "code_search",
                "symbols",
                "tests",
                "impact",
                "local",
            ],
            &["action", "query", "path", "limit"],
            &[
                "Find files related to runtime protocol events.",
                "Discover tests related to settings dialog changes.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_AGENT_COORDINATION,
            "core",
            "Read and manage the temporary app-data active-agent coordination bus, including presence, recent activity, and advisory file reservations.",
            &[
                "agent",
                "coordination",
                "presence",
                "reservation",
                "conflict",
                "lease",
                "active_run",
                "mailbox",
                "swarm",
            ],
            &[
                "action",
                "path",
                "paths",
                "operation",
                "note",
                "overrideReason",
                "reservationId",
                "releaseReason",
                "itemType",
                "itemId",
                "targetAgentSessionId",
                "targetRunId",
                "targetRole",
                "title",
                "body",
                "priority",
                "ttlSeconds",
                "summary",
                "limit",
            ],
            &[
                "Check whether another active run owns an overlapping file reservation.",
                "Publish a temporary blocker or question for sibling agents.",
                "Promote a mailbox finding to a durable-context review candidate.",
            ],
            "coordination_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_TOOL_ACCESS,
            "core",
            "List or activate deferred tool groups and exact tools.",
            &["tool", "activation", "capability", "registry"],
            &["action", "groups", "tools", "reason"],
            &["Request command_readonly before running tests.", "Activate solana_alt after tool_search finds it."],
            "registry_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_TOOL_SEARCH,
            "core",
            "Search deferred tool capabilities before requesting activation.",
            &["tool", "search", "discovery", "catalog", "bm25", "capability"],
            &["query", "limit"],
            &["Search for address lookup table tools.", "Find the smallest browser observation capability."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_AGENT_DEFINITION,
            "agent_builder",
            "Draft, validate, list, save, update, archive, and clone registry-backed custom agent definitions in app-data-backed state.",
            &[
                "agent",
                "definition",
                "custom_agent",
                "registry",
                "validation",
                "app_data",
            ],
            &[
                "action",
                "definitionId",
                "sourceDefinitionId",
                "includeArchived",
                "definition",
            ],
            &[
                "Validate a least-privilege custom agent definition.",
                "Save an approved custom agent definition after operator approval.",
            ],
            "agent_definition_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
            "environment",
            "Read compact, redacted developer-environment facts from the app-global environment profile.",
            &[
                "environment",
                "installed tools",
                "cli",
                "package manager",
                "language runtime",
                "PATH",
                "protoc",
                "node",
                "rust",
                "python",
                "solana",
                "docker",
                "mobile",
            ],
            &["action", "toolIds", "category", "capabilityIds"],
            &[
                "Get a summary before diagnosing command not found.",
                "Check whether protoc, node, rust, or solana tooling is present.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_TODO,
            "core",
            "Maintain model-visible planning state for the current owned-agent run, including Debug evidence ledgers.",
            &["plan", "todo", "task", "state", "debug", "evidence"],
            &[
                "action",
                "id",
                "title",
                "notes",
                "status",
                "mode",
                "debugStage",
                "evidence",
                "phaseId",
                "phaseTitle",
                "sliceId",
                "handoffNote",
            ],
            &[
                "Track inspect, edit, verify steps for a multi-file change.",
                "Record Debug symptom, hypothesis, experiment, root_cause, fix, and verification evidence.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_LIST,
            "core",
            "List repo-scoped files.",
            &["file", "list", "tree", "directory"],
            &["path", "maxDepth"],
            &["List top-level project directories."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_HASH,
            "core",
            "Hash a repo-relative file with SHA-256.",
            &["file", "hash", "sha256", "stale_write"],
            &["path"],
            &["Hash a file before guarded mutation."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_WRITE,
            "mutation",
            "Write a UTF-8 text file by repo-relative path.",
            &["file", "write", "create", "replace"],
            &["path", "content"],
            &["Create a new generated source file."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_EDIT,
            "mutation",
            "Apply an exact expected-text line-range edit with optional file and line hash anchors.",
            &["file", "edit", "line", "expected_text", "hash_guard"],
            &["path", "startLine", "endLine", "expected", "replacement", "expectedHash", "startLineHash", "endLineHash"],
            &["Replace a small function body after reading it."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PATCH,
            "mutation",
            "Apply a canonical UTF-8 text patch with preview, expected-hash guards, exact diagnostics, and multi-file support.",
            &["file", "patch", "replace", "exact_text", "multi_file", "preview", "hash_guard"],
            &["path", "search", "replace", "replaceAll", "expectedHash", "preview", "operations"],
            &["Preview a multi-file patch before writing.", "Replace an exact import statement with an expected hash."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_DELETE,
            "mutation",
            "Delete a repo-relative file or directory.",
            &["file", "delete", "remove"],
            &["path", "recursive", "expectedHash"],
            &["Delete an obsolete generated file with an expected hash."],
            "write_destructive",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_RENAME,
            "mutation",
            "Rename or move a repo-relative path.",
            &["file", "rename", "move"],
            &["fromPath", "toPath", "expectedHash"],
            &["Move a source file inside the repo."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_MKDIR,
            "mutation",
            "Create a repo-relative directory and missing parents.",
            &["file", "directory", "mkdir", "create"],
            &["path"],
            &["Create a new feature directory."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_PROBE,
            "command",
            "Run a narrowly allowlisted repo-scoped discovery command.",
            &["command", "probe", "diagnostic", "git", "rg", "metadata"],
            &["argv", "cwd", "timeoutMs"],
            &["Run git status or cargo metadata for local discovery."],
            "command_probe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_VERIFY,
            "command",
            "Run a narrowly allowlisted repo-scoped verification command for tests, checks, lint, build, or formatting verification.",
            &["command", "verify", "test", "lint", "build", "cargo", "npm", "pnpm"],
            &["argv", "cwd", "timeoutMs"],
            &["Run cargo test for the changed crate.", "Run pnpm test for a scoped package."],
            "command_verify",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_RUN,
            "command",
            "Run a repo-scoped command that is not covered by probe or verification policy.",
            &["command", "shell", "run", "script"],
            &["argv", "cwd", "timeoutMs"],
            &["Run a one-off repo-scoped helper after approval policy allows it."],
            "command",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_SESSION,
            "command",
            "Start, read, or stop a repo-scoped long-running command session.",
            &["command", "session", "long_running", "dev_server", "watch"],
            &["action", "argv", "cwd", "timeoutMs", "sessionId", "afterSequence", "maxBytes"],
            &["Start a dev server or watcher, then read and stop it."],
            "long_running_process",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            "process_manager",
            "Manage Xero-owned long-running, interactive, grouped, restartable, and async-job processes, plus system process visibility and approval-gated external signaling.",
            &["process", "pty", "async", "background", "output", "signal", "port"],
            &["action", "processId", "groupId", "argv", "input", "timeoutMs", "maxBytes"],
            &["Start an async build and await completion.", "Inspect system ports."],
            "process_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
            "system_diagnostics_observe",
            "Run read-only typed diagnostics including process open-file inspection, resource snapshots, thread lists, unified logs, and bounded diagnostic bundles.",
            &[
                "diagnostics",
                "process",
                "lsof",
                "open files",
                "file descriptors",
                "sockets",
                "threads",
                "sample",
                "unified log",
                "hung process",
                "port conflict",
                "tauri",
                "high cpu",
            ],
            &[
                "action",
                "preset",
                "pid",
                "processName",
                "limit",
                "filter",
                "fdKinds",
                "includeSockets",
                "includeFiles",
                "includeDeleted",
                "durationMs",
                "intervalMs",
            ],
            &[
                "Inspect open files for a PID without raw shell text.",
                "Request a bounded read-only diagnostics bundle for a hung process.",
            ],
            "system_read",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED,
            "system_diagnostics_privileged",
            "Run approval-gated diagnostics that may capture sensitive runtime or UI state, such as process samples and macOS accessibility snapshots.",
            &["diagnostics", "process", "sample", "accessibility", "macos", "approval"],
            &[
                "action",
                "pid",
                "processName",
                "bundleId",
                "appName",
                "windowId",
                "durationMs",
                "intervalMs",
                "sampleCount",
                "maxDepth",
                "focusedOnly",
                "attributes",
            ],
            &["Sample a hung process after operator approval."],
            "system_privileged",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_MACOS_AUTOMATION,
            "macos",
            "macOS app/system automation: check permissions, list/launch/activate/quit apps, list/focus windows, and capture approval-gated screenshots.",
            &["macos", "desktop", "window", "app", "screenshot", "permission"],
            &["action", "bundleId", "appName", "windowId", "target"],
            &["List running apps.", "Capture a screenshot after approval."],
            "os_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_WEB_SEARCH,
            "web",
            "Search the web through the configured backend.",
            &["web", "search", "internet", "docs", "latest"],
            &["query", "limit"],
            &["Search current official documentation."],
            "network",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_WEB_FETCH,
            "web",
            "Fetch HTTP or HTTPS text content.",
            &["web", "fetch", "http", "docs", "page"],
            &["url", "contentKind", "maxBytes"],
            &["Fetch a documentation page after search."],
            "network",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            "browser_observe",
            "Observe the in-app browser with page text, URL, screenshots, console, network, accessibility, tabs, and safe state reads.",
            &["browser", "frontend", "ui", "dom", "screenshot", "accessibility", "console", "network", "storage", "cookies"],
            &["action", "url", "selector", "text", "timeoutMs", "tabId", "area", "key"],
            &["Observe current URL and page text.", "Capture an accessibility tree."],
            "browser_observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            "browser_control",
            "Control the in-app browser with navigation, clicks, typing, key presses, scroll, cookie/storage writes, tab focus/close, and state restore.",
            &["browser", "frontend", "ui", "dom", "click", "type", "navigation", "storage", "cookies"],
            &["action", "url", "selector", "text", "timeoutMs", "tabId", "area", "key"],
            &["Click and type into a local app after activation."],
            "browser_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_EMULATOR,
            "emulator",
            "Drive mobile emulator automation.",
            &["emulator", "mobile", "android", "ios", "device", "tap", "swipe", "screenshot"],
            &["action", "input"],
            &["Launch an app and capture a screenshot."],
            "device_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_MCP_LIST,
            "mcp_list",
            "List connected MCP servers, tools, resources, and prompts over stdio, HTTP, or SSE without invoking capabilities.",
            &["mcp", "model_context_protocol", "tool", "resource", "prompt", "external"],
            &["action", "serverId", "timeoutMs"],
            &["List MCP server tools."],
            "external_capability_observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_MCP_READ_RESOURCE,
            "mcp_invoke",
            "Read a resource from a connected MCP server.",
            &["mcp", "model_context_protocol", "resource", "external"],
            &["serverId", "uri", "timeoutMs"],
            &["Read a named MCP resource after activation."],
            "external_capability_observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_MCP_GET_PROMPT,
            "mcp_invoke",
            "Get a prompt from a connected MCP server.",
            &["mcp", "model_context_protocol", "prompt", "external"],
            &["serverId", "name", "arguments", "timeoutMs"],
            &["Get a named MCP prompt after activation."],
            "external_capability_observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_MCP_CALL_TOOL,
            "mcp_invoke",
            "Call a tool on a connected MCP server.",
            &["mcp", "model_context_protocol", "tool", "external"],
            &["serverId", "name", "arguments", "timeoutMs"],
            &["Invoke a named MCP tool after activation."],
            "external_capability_invoke",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_SUBAGENT,
            "agent_ops",
            "Manage pane-contained child agents with role-scoped tool policy and trace lineage.",
            &["agent", "subagent", "delegate", "explore", "verify", "parallel"],
            &["action", "taskId", "role", "prompt", "modelId", "writeSet", "decision", "maxToolCalls"],
            &["Spawn a researcher for a bounded codebase question.", "Poll an engineer and integrate its result with a parent decision."],
            "agent_delegation",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_NOTEBOOK_EDIT,
            "notebook",
            "Edit a Jupyter notebook cell source.",
            &["notebook", "jupyter", "ipynb", "cell", "edit"],
            &["path", "cellIndex", "expectedSource", "replacementSource"],
            &["Replace a notebook cell after reading it."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_CODE_INTEL,
            "intelligence",
            "Inspect source symbols or JSON diagnostics without requiring command execution.",
            &["code", "symbol", "diagnostic", "intelligence", "static"],
            &["action", "query", "path", "limit"],
            &["Find symbols named greet.", "Read diagnostics for a source file."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_LSP,
            "intelligence",
            "Inspect language-server availability and resolve source symbols or diagnostics through LSP with native fallback.",
            &["lsp", "language_server", "symbol", "diagnostic", "install_suggestion"],
            &["action", "query", "path", "limit", "serverId", "timeoutMs"],
            &["List available language servers.", "Resolve symbol references with LSP."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_POWERSHELL,
            "powershell",
            "Run PowerShell through the same repo-scoped command policy used for shell commands.",
            &["powershell", "pwsh", "windows", "command", "script"],
            &["script", "cwd", "timeoutMs"],
            &["Run a Windows shell diagnostic inside the repo."],
            "command",
        ),
    ];

    catalog.extend(solana_tool_catalog_entries());

    if skill_tool_enabled {
        catalog.push(catalog_entry(
            AUTONOMOUS_TOOL_SKILL,
            "skills",
            "Discover, resolve, install, invoke, reload, or create Xero skills.",
            &["skill", "skills", "capability", "instructions", "plugin"],
            &["operation", "query", "sourceId", "context", "limit"],
            &[
                "Discover relevant skills before implementation.",
                "Invoke a trusted skill as bounded prompt context.",
            ],
            "skill_runtime",
        ));
    }

    catalog
}

fn catalog_entry(
    tool_name: &'static str,
    group: &'static str,
    description: &'static str,
    tags: &'static [&'static str],
    schema_fields: &'static [&'static str],
    examples: &'static [&'static str],
    risk_class: &'static str,
) -> AutonomousToolCatalogEntry {
    AutonomousToolCatalogEntry {
        tool_name,
        group,
        description,
        tags,
        schema_fields,
        examples,
        risk_class,
    }
}

fn solana_tool_catalog_entries() -> Vec<AutonomousToolCatalogEntry> {
    [
        (
            AUTONOMOUS_TOOL_SOLANA_CLUSTER,
            "Inspect and control local or forked Solana clusters.",
            &["cluster", "validator", "localnet"][..],
            &["action", "clusterId"][..],
            &["List local Solana clusters."][..],
            "external_chain_control",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_LOGS,
            "Read Solana validator, program, and transaction logs.",
            &["logs", "validator", "program", "transaction"][..],
            &["action", "clusterId", "signature", "programId"][..],
            &["Read validator logs for a failing transaction."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_TX,
            "Inspect Solana transaction signatures, statuses, and metadata.",
            &["transaction", "signature", "status", "metadata"][..],
            &["action", "signature", "clusterId"][..],
            &["Inspect a confirmed transaction signature."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_SIMULATE,
            "Simulate Solana transactions before sending them.",
            &["simulate", "transaction", "preflight"][..],
            &["transaction", "clusterId", "accounts"][..],
            &["Simulate a transaction before deploy work."][..],
            "external_chain_simulation",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_EXPLAIN,
            "Explain Solana transactions, errors, and account changes.",
            &["explain", "error", "account", "transaction"][..],
            &["signature", "error", "logs"][..],
            &["Explain a custom program error."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_ALT,
            "Inspect Solana address lookup table data.",
            &["alt", "address_lookup_table", "lookup", "address"][..],
            &["address", "clusterId"][..],
            &["Inspect an address lookup table."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_IDL,
            "Inspect Anchor IDLs and generated Solana interface metadata.",
            &["idl", "anchor", "interface", "schema"][..],
            &["action", "path", "programId"][..],
            &["Compare an Anchor IDL against generated clients."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_CODAMA,
            "Run Codama schema and client-generation helpers.",
            &["codama", "client", "generate", "schema"][..],
            &["action", "path", "outputDir"][..],
            &["Generate clients from a Codama schema."][..],
            "write",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_PDA,
            "Derive and inspect Solana program-derived addresses.",
            &["pda", "seed", "program_derived_address", "bump"][..],
            &["programId", "seeds", "clusterId"][..],
            &["Derive a PDA from known seeds."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_PROGRAM,
            "Inspect Solana program metadata and build state.",
            &["program", "metadata", "build", "anchor"][..],
            &["action", "programId", "manifestPath"][..],
            &["Inspect program build metadata."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_DEPLOY,
            "Run Solana deploy planning and guarded deploy actions.",
            &["deploy", "program", "upgrade", "authority"][..],
            &["action", "clusterId", "programPath"][..],
            &["Plan a guarded program deploy."][..],
            "external_chain_mutation",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
            "Check Solana upgrade authority and deployment safety.",
            &["upgrade", "authority", "safety", "deploy"][..],
            &["programId", "clusterId", "idlPath"][..],
            &["Check upgrade authority before deploy."][..],
            "external_chain_observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_SQUADS,
            "Inspect Squads multisig proposals and governance state.",
            &["squads", "multisig", "governance", "proposal"][..],
            &["action", "vault", "proposalId"][..],
            &["Inspect a Squads proposal."][..],
            "external_chain_observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
            "Run verified-build checks for Solana programs.",
            &["verified_build", "build", "program", "audit"][..],
            &["manifestPath", "programId"][..],
            &["Run verified build checks."][..],
            "command",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
            "Run static audit checks for Solana programs.",
            &["audit", "static", "security", "program"][..],
            &["path", "programId"][..],
            &["Run a static Solana audit."][..],
            "command",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
            "Run external-reference audit checks for Solana programs.",
            &["audit", "external", "reference", "security"][..],
            &["path", "programId"][..],
            &["Audit external references."][..],
            "network",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ,
            "Run fuzzing-oriented audit checks for Solana programs.",
            &["audit", "fuzz", "security", "test"][..],
            &["path", "target"][..],
            &["Run fuzzing audit checks."][..],
            "command",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE,
            "Inspect Solana audit and test coverage evidence.",
            &["audit", "coverage", "test", "evidence"][..],
            &["path", "programId"][..],
            &["Inspect Solana coverage evidence."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_REPLAY,
            "Replay Solana transactions or ledger events.",
            &["replay", "transaction", "ledger"][..],
            &["signature", "clusterId", "slot"][..],
            &["Replay a transaction for debugging."][..],
            "external_chain_simulation",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_INDEXER,
            "Inspect and manage Solana indexer state.",
            &["indexer", "state", "events", "accounts"][..],
            &["action", "indexerId", "programId"][..],
            &["Inspect indexer lag."][..],
            "external_chain_observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_SECRETS,
            "Inspect Solana secret references without exposing raw values.",
            &["secret", "keypair", "wallet", "redaction"][..],
            &["action", "scope"][..],
            &["Check whether a deploy keypair reference is configured."][..],
            "secret_reference",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
            "Detect drift between expected and live Solana cluster state.",
            &["drift", "cluster", "program", "state"][..],
            &["clusterId", "trackedPrograms"][..],
            &["Detect localnet drift against expected programs."][..],
            "external_chain_observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_COST,
            "Estimate Solana transaction, account, and runtime costs.",
            &["cost", "rent", "compute", "fee"][..],
            &["action", "transaction", "accounts"][..],
            &["Estimate account rent and transaction fee."][..],
            "observe",
        ),
        (
            AUTONOMOUS_TOOL_SOLANA_DOCS,
            "Search and retrieve Solana documentation guidance.",
            &["docs", "documentation", "solana", "anchor"][..],
            &["query", "topic", "limit"][..],
            &["Search Solana documentation for address lookup tables."][..],
            "network",
        ),
    ]
    .into_iter()
    .map(
        |(tool_name, description, tags, schema_fields, examples, risk_class)| {
            catalog_entry(
                tool_name,
                "solana",
                description,
                tags,
                schema_fields,
                examples,
                risk_class,
            )
        },
    )
    .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousToolRuntimeLimits {
    pub default_read_line_count: usize,
    pub max_read_line_count: usize,
    pub max_text_file_bytes: usize,
    pub max_search_query_chars: usize,
    pub max_search_results: usize,
    pub max_search_preview_chars: usize,
    pub default_command_timeout_ms: u64,
    pub max_command_timeout_ms: u64,
    pub max_command_capture_bytes: usize,
    pub max_command_excerpt_chars: usize,
}

impl Default for AutonomousToolRuntimeLimits {
    fn default() -> Self {
        Self {
            default_read_line_count: DEFAULT_READ_LINE_COUNT,
            max_read_line_count: MAX_READ_LINE_COUNT,
            max_text_file_bytes: MAX_TEXT_FILE_BYTES,
            max_search_query_chars: MAX_SEARCH_QUERY_CHARS,
            max_search_results: MAX_SEARCH_RESULTS,
            max_search_preview_chars: MAX_SEARCH_PREVIEW_CHARS,
            default_command_timeout_ms: DEFAULT_COMMAND_TIMEOUT_MS,
            max_command_timeout_ms: MAX_COMMAND_TIMEOUT_MS,
            max_command_capture_bytes: MAX_COMMAND_CAPTURE_BYTES,
            max_command_excerpt_chars: MAX_COMMAND_EXCERPT_CHARS,
        }
    }
}

#[derive(Clone)]
pub struct AutonomousToolRuntime {
    pub(super) repo_root: PathBuf,
    pub(super) limits: AutonomousToolRuntimeLimits,
    pub(super) web_runtime: AutonomousWebRuntime,
    pub(super) command_controls: Option<RuntimeRunControlStateDto>,
    pub(super) agent_tool_policy: Option<AutonomousAgentToolPolicy>,
    pub(super) agent_workflow_policy: Option<AutonomousAgentWorkflowPolicy>,
    pub(super) agent_workflow_state: Arc<Mutex<AutonomousAgentWorkflowRuntimeState>>,
    pub(super) agent_run_context: Option<AutonomousAgentRunContext>,
    pub(super) tool_application_policy: ResolvedAgentToolApplicationStyleDto,
    pub(super) browser_control_preference: BrowserControlPreferenceDto,
    pub(super) soul_settings: SoulSettingsDto,
    pub(super) browser_executor: Option<Arc<dyn BrowserExecutor>>,
    pub(super) emulator_executor: Option<Arc<dyn EmulatorExecutor>>,
    pub(super) solana_executor: Option<Arc<dyn SolanaExecutor>>,
    pub(super) cancellation_token: Option<AgentRunCancellationToken>,
    pub(super) tool_execution_cancelled: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub(super) mcp_registry_path: Option<PathBuf>,
    pub(super) environment_profile_database_path: Option<PathBuf>,
    pub(super) todo_items: Arc<Mutex<BTreeMap<String, AutonomousTodoItem>>>,
    pub(super) subagent_tasks: Arc<Mutex<BTreeMap<String, AutonomousSubagentTask>>>,
    pub(super) subagent_executor: Option<Arc<dyn AutonomousSubagentExecutor>>,
    pub(super) subagent_execution_depth: usize,
    pub(super) subagent_write_scope: Option<AutonomousSubagentWriteScope>,
    pub(super) subagent_limits: AutonomousSubagentLimits,
    pub(super) delegated_tool_call_budget: Option<AutonomousDelegatedToolCallBudget>,
    pub(super) delegated_usage_budget: Option<AutonomousDelegatedUsageBudget>,
    pub(super) skill_tool: Option<AutonomousSkillToolRuntime>,
    process_sessions: Arc<process::ProcessSessionRegistry>,
    owned_processes: Arc<process_manager::OwnedProcessRegistry>,
}

impl std::fmt::Debug for AutonomousToolRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutonomousToolRuntime")
            .field("repo_root", &self.repo_root)
            .field("limits", &self.limits)
            .field("command_controls", &self.command_controls)
            .field("agent_tool_policy", &self.agent_tool_policy)
            .field("agent_workflow_policy", &self.agent_workflow_policy)
            .field("agent_run_context", &self.agent_run_context)
            .field("tool_application_policy", &self.tool_application_policy)
            .field(
                "browser_control_preference",
                &self.browser_control_preference,
            )
            .field("soul_settings", &self.soul_settings)
            .field("mcp_registry_path", &self.mcp_registry_path)
            .field(
                "tool_execution_cancelled",
                &self.tool_execution_cancelled.is_some(),
            )
            .field("subagent_execution_depth", &self.subagent_execution_depth)
            .field("subagent_write_scope", &self.subagent_write_scope)
            .field("subagent_limits", &self.subagent_limits)
            .field("delegated_usage_budget", &self.delegated_usage_budget)
            .field("skill_tool_enabled", &self.skill_tool.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousAgentRunContext {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousSubagentWriteScope {
    pub role: AutonomousSubagentRole,
    pub write_set: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutonomousSubagentLimits {
    pub max_child_agents: usize,
    pub max_depth: usize,
    pub max_concurrent_child_runs: usize,
    pub max_delegated_tool_calls: usize,
    pub max_delegated_tokens: u64,
    pub max_delegated_cost_micros: u64,
}

impl Default for AutonomousSubagentLimits {
    fn default() -> Self {
        Self {
            max_child_agents: DEFAULT_SUBAGENT_MAX_CHILD_AGENTS,
            max_depth: DEFAULT_SUBAGENT_MAX_DEPTH,
            max_concurrent_child_runs: DEFAULT_SUBAGENT_MAX_CONCURRENT_CHILD_RUNS,
            max_delegated_tool_calls: DEFAULT_SUBAGENT_MAX_DELEGATED_TOOL_CALLS,
            max_delegated_tokens: DEFAULT_SUBAGENT_MAX_DELEGATED_TOKENS,
            max_delegated_cost_micros: DEFAULT_SUBAGENT_MAX_DELEGATED_COST_MICROS,
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct AutonomousDelegatedToolCallBudget {
    owner: String,
    remaining: Arc<Mutex<usize>>,
}

#[derive(Debug, Clone)]
pub(super) struct AutonomousDelegatedUsageBudget {
    owner: String,
    max_tokens: u64,
    max_cost_micros: u64,
}

pub trait AutonomousSubagentExecutor: Send + Sync {
    fn execute_subagent(
        &self,
        task: AutonomousSubagentTask,
        task_store: Arc<Mutex<BTreeMap<String, AutonomousSubagentTask>>>,
    ) -> CommandResult<AutonomousSubagentTask>;

    fn cancel_subagent(
        &self,
        task: &AutonomousSubagentTask,
    ) -> CommandResult<AutonomousSubagentTask>;

    fn send_subagent_input(
        &self,
        task: &AutonomousSubagentTask,
        text: &str,
    ) -> CommandResult<AutonomousSubagentTask>;

    fn export_subagent_trace(&self, task: &AutonomousSubagentTask) -> CommandResult<JsonValue>;
}

impl AutonomousToolRuntime {
    pub fn new(repo_root: impl AsRef<Path>) -> CommandResult<Self> {
        Self::with_limits_and_web_config(
            repo_root,
            AutonomousToolRuntimeLimits::default(),
            AutonomousWebConfig::for_platform(),
        )
    }

    pub fn with_limits(
        repo_root: impl AsRef<Path>,
        limits: AutonomousToolRuntimeLimits,
    ) -> CommandResult<Self> {
        Self::with_limits_and_web_config(repo_root, limits, AutonomousWebConfig::for_platform())
    }

    pub fn with_limits_and_web_config(
        repo_root: impl AsRef<Path>,
        limits: AutonomousToolRuntimeLimits,
        web_config: AutonomousWebConfig,
    ) -> CommandResult<Self> {
        let repo_root = repo_root.as_ref();
        let canonical_root = fs::canonicalize(repo_root).map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => CommandError::project_not_found(),
            _ => CommandError::system_fault(
                "autonomous_tool_repo_root_unavailable",
                format!(
                    "Xero could not access the imported repository root at {}: {error}",
                    repo_root.display()
                ),
            ),
        })?;

        if !canonical_root.is_dir() {
            return Err(CommandError::user_fixable(
                "autonomous_tool_repo_root_invalid",
                format!(
                    "Imported repository root {} is not a directory.",
                    canonical_root.display()
                ),
            ));
        }

        Ok(Self {
            repo_root: canonical_root,
            limits,
            web_runtime: AutonomousWebRuntime::new(web_config),
            command_controls: None,
            agent_tool_policy: None,
            agent_workflow_policy: None,
            agent_workflow_state: Arc::new(Mutex::new(
                AutonomousAgentWorkflowRuntimeState::default(),
            )),
            agent_run_context: None,
            tool_application_policy: ResolvedAgentToolApplicationStyleDto::default(),
            browser_control_preference: BrowserControlPreferenceDto::Default,
            soul_settings: default_soul_settings(),
            browser_executor: None,
            emulator_executor: None,
            solana_executor: None,
            cancellation_token: None,
            tool_execution_cancelled: None,
            mcp_registry_path: None,
            environment_profile_database_path: None,
            todo_items: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_tasks: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_executor: None,
            subagent_execution_depth: 0,
            subagent_write_scope: None,
            subagent_limits: AutonomousSubagentLimits::default(),
            delegated_tool_call_budget: None,
            delegated_usage_budget: None,
            skill_tool: None,
            process_sessions: Arc::new(process::ProcessSessionRegistry::default()),
            owned_processes: Arc::new(process_manager::OwnedProcessRegistry::default()),
        })
    }

    pub fn with_browser_executor(mut self, executor: Arc<dyn BrowserExecutor>) -> Self {
        self.browser_executor = Some(executor);
        self
    }

    pub fn browser_executor(&self) -> Option<&Arc<dyn BrowserExecutor>> {
        self.browser_executor.as_ref()
    }

    pub fn with_browser_control_preference(
        mut self,
        preference: BrowserControlPreferenceDto,
    ) -> Self {
        self.browser_control_preference = preference;
        self
    }

    pub fn browser_control_preference(&self) -> BrowserControlPreferenceDto {
        self.browser_control_preference
    }

    pub fn with_tool_application_policy(
        mut self,
        policy: ResolvedAgentToolApplicationStyleDto,
    ) -> Self {
        self.tool_application_policy = policy;
        self
    }

    pub fn tool_application_policy(&self) -> &ResolvedAgentToolApplicationStyleDto {
        &self.tool_application_policy
    }

    pub fn with_soul_settings(mut self, settings: SoulSettingsDto) -> Self {
        self.soul_settings = settings;
        self
    }

    pub fn soul_settings(&self) -> &SoulSettingsDto {
        &self.soul_settings
    }

    pub fn with_emulator_executor(mut self, executor: Arc<dyn EmulatorExecutor>) -> Self {
        self.emulator_executor = Some(executor);
        self
    }

    pub fn emulator_executor(&self) -> Option<&Arc<dyn EmulatorExecutor>> {
        self.emulator_executor.as_ref()
    }

    pub fn with_solana_executor(mut self, executor: Arc<dyn SolanaExecutor>) -> Self {
        self.solana_executor = Some(executor);
        self
    }

    pub fn solana_executor(&self) -> Option<&Arc<dyn SolanaExecutor>> {
        self.solana_executor.as_ref()
    }

    pub fn for_project<R: Runtime>(
        app: &AppHandle<R>,
        state: &DesktopState,
        project_id: &str,
    ) -> CommandResult<Self> {
        let browser_executor = browser::tauri_browser_executor(app.clone(), state.clone());
        let repo_root = resolve_imported_repo_root(app, state, project_id)?;
        let browser_control_preference = load_browser_control_settings(app, state)?.preference;
        let soul_settings = load_soul_settings(app, state)?;
        let skill_settings = load_skill_source_settings_from_path(&state.global_db_path(app)?)?;
        let skill_runtime_config = AutonomousSkillRuntimeConfig {
            default_source_repo: skill_settings.github.repo.clone(),
            default_source_ref: skill_settings.github.reference.clone(),
            default_source_root: skill_settings.github.root.clone(),
            ..AutonomousSkillRuntimeConfig::for_platform()
        };
        let skill_cache_root = state.autonomous_skill_cache_dir(app)?;
        let skill_runtime = AutonomousSkillRuntime::new(skill_runtime_config, skill_cache_root)
            .with_installed_skill_registry(Arc::new(
                crate::db::project_store::ProjectStoreInstalledSkillRegistry::project(
                    repo_root.clone(),
                    project_id.to_owned(),
                )
                .expect("project id already validated by imported project registry"),
            ));
        let local_skill_roots = skill_settings
            .enabled_local_roots()
            .into_iter()
            .map(|root| AutonomousLocalSkillRoot {
                root_id: root.root_id,
                root_path: PathBuf::from(root.path),
            })
            .collect::<Vec<_>>();
        let plugin_roots = skill_settings
            .enabled_plugin_roots()
            .into_iter()
            .map(|root| AutonomousPluginRoot {
                root_id: root.root_id,
                root_path: PathBuf::from(root.path),
            })
            .collect::<Vec<_>>();
        let runtime = Self::with_limits_and_web_config(
            repo_root,
            AutonomousToolRuntimeLimits::default(),
            state.autonomous_web_config(),
        )?
        .with_browser_control_preference(browser_control_preference)
        .with_soul_settings(soul_settings)
        .with_browser_executor(browser_executor)
        .with_emulator_executor(emulator::tauri_emulator_executor(app.clone()))
        .with_mcp_registry_path(state.global_db_path(app)?)
        .with_environment_profile_database_path(state.global_db_path(app)?)
        .with_skill_tool_config(
            project_id.to_owned(),
            skill_runtime,
            default_bundled_skill_roots(app),
            local_skill_roots,
            skill_settings.project_discovery_enabled(project_id),
            skill_settings.github.enabled,
            plugin_roots,
        );

        let runtime = match app.try_state::<crate::commands::SolanaState>() {
            Some(solana_state) => runtime.with_solana_executor(Arc::new(
                StateSolanaExecutor::from_state(solana_state.inner()),
            )),
            None => runtime,
        };

        Ok(runtime)
    }

    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    pub fn limits(&self) -> AutonomousToolRuntimeLimits {
        self.limits
    }

    pub fn with_runtime_run_controls(mut self, controls: RuntimeRunControlStateDto) -> Self {
        self.command_controls = Some(controls);
        self
    }

    pub fn with_agent_tool_policy(mut self, policy: Option<AutonomousAgentToolPolicy>) -> Self {
        self.agent_tool_policy = policy;
        self
    }

    pub fn agent_tool_policy(&self) -> Option<&AutonomousAgentToolPolicy> {
        self.agent_tool_policy.as_ref()
    }

    pub fn with_agent_workflow_policy(
        mut self,
        policy: Option<AutonomousAgentWorkflowPolicy>,
    ) -> Self {
        let state = policy
            .as_ref()
            .map(AutonomousAgentWorkflowPolicy::initial_state)
            .unwrap_or_default();
        self.agent_workflow_policy = policy;
        self.agent_workflow_state = Arc::new(Mutex::new(state));
        self
    }

    pub fn with_agent_run_context(
        mut self,
        project_id: impl Into<String>,
        agent_session_id: impl Into<String>,
        run_id: impl Into<String>,
    ) -> Self {
        self.agent_run_context = Some(AutonomousAgentRunContext {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
        });
        self
    }

    pub fn with_durable_subagent_tasks_for_run(
        mut self,
        repo_root: &Path,
        project_id: &str,
        parent_run_id: &str,
    ) -> CommandResult<Self> {
        let tasks = load_subagent_tasks_for_parent(repo_root, project_id, parent_run_id)?;
        self.subagent_tasks = Arc::new(Mutex::new(tasks));
        Ok(self)
    }

    pub fn agent_run_context(&self) -> Option<&AutonomousAgentRunContext> {
        self.agent_run_context.as_ref()
    }

    pub fn runtime_run_controls(&self) -> Option<&RuntimeRunControlStateDto> {
        self.command_controls.as_ref()
    }

    pub fn with_cancellation_token(mut self, token: AgentRunCancellationToken) -> Self {
        self.cancellation_token = Some(token);
        self
    }

    pub fn with_tool_execution_cancellation(
        mut self,
        is_cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Self {
        self.tool_execution_cancelled = Some(is_cancelled);
        self
    }

    pub fn with_mcp_registry_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.mcp_registry_path = Some(path.into());
        self
    }

    pub fn with_environment_profile_database_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.environment_profile_database_path = Some(path.into());
        self
    }

    pub fn with_subagent_executor(mut self, executor: Arc<dyn AutonomousSubagentExecutor>) -> Self {
        self.subagent_executor = Some(executor);
        self
    }

    pub fn without_subagent_executor(mut self) -> Self {
        self.subagent_executor = None;
        self
    }

    pub fn with_subagent_execution_depth(mut self, depth: usize) -> Self {
        self.subagent_execution_depth = depth;
        self
    }

    pub fn with_subagent_limits(mut self, limits: AutonomousSubagentLimits) -> Self {
        self.subagent_limits = limits;
        self
    }

    pub fn with_subagent_write_scope(
        mut self,
        role: AutonomousSubagentRole,
        write_set: Vec<String>,
    ) -> Self {
        self.subagent_write_scope = Some(AutonomousSubagentWriteScope { role, write_set });
        self
    }

    pub fn subagent_write_scope(&self) -> Option<&AutonomousSubagentWriteScope> {
        self.subagent_write_scope.as_ref()
    }

    pub fn with_delegated_tool_call_budget(
        mut self,
        owner: impl Into<String>,
        max_tool_calls: usize,
    ) -> Self {
        self.delegated_tool_call_budget = Some(AutonomousDelegatedToolCallBudget {
            owner: owner.into(),
            remaining: Arc::new(Mutex::new(max_tool_calls)),
        });
        self
    }

    pub fn with_delegated_provider_usage_budget(
        mut self,
        owner: impl Into<String>,
        max_tokens: u64,
        max_cost_micros: u64,
    ) -> Self {
        self.delegated_usage_budget = Some(AutonomousDelegatedUsageBudget {
            owner: owner.into(),
            max_tokens,
            max_cost_micros,
        });
        self
    }

    pub(crate) fn delegated_provider_usage_budget(&self) -> Option<(&str, u64, u64)> {
        self.delegated_usage_budget.as_ref().map(|budget| {
            (
                budget.owner.as_str(),
                budget.max_tokens,
                budget.max_cost_micros,
            )
        })
    }

    pub fn with_skill_tool(
        mut self,
        project_id: impl Into<String>,
        github_runtime: AutonomousSkillRuntime,
        bundled_roots: Vec<AutonomousBundledSkillRoot>,
        local_roots: Vec<AutonomousLocalSkillRoot>,
    ) -> Self {
        self = self.with_skill_tool_config(
            project_id,
            github_runtime,
            bundled_roots,
            local_roots,
            true,
            true,
            Vec::new(),
        );
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub fn with_skill_tool_config(
        mut self,
        project_id: impl Into<String>,
        github_runtime: AutonomousSkillRuntime,
        bundled_roots: Vec<AutonomousBundledSkillRoot>,
        local_roots: Vec<AutonomousLocalSkillRoot>,
        project_skills_enabled: bool,
        github_enabled: bool,
        plugin_roots: Vec<AutonomousPluginRoot>,
    ) -> Self {
        let project_id = project_id.into();
        let project_app_data_dir = crate::db::project_app_data_dir_for_repo(&self.repo_root);
        self.skill_tool = Some(AutonomousSkillToolRuntime::new(
            AutonomousSkillToolRuntimeConfig {
                project_id,
                project_app_data_dir,
                github_runtime,
                bundled_roots,
                local_roots,
                project_skills_enabled,
                github_enabled,
                plugin_roots,
            },
        ));
        self
    }

    pub fn with_skill_tool_disabled(mut self) -> Self {
        self.skill_tool = None;
        self
    }

    pub fn skill_tool_enabled(&self) -> bool {
        self.skill_tool.is_some()
    }

    fn enforce_agent_workflow_before_tool(&self, tool_name: &str) -> CommandResult<()> {
        let Some(policy) = self.agent_workflow_policy.as_ref() else {
            return Ok(());
        };
        let todos = self.todo_items.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_todo_lock_failed",
                "Xero could not lock the owned-agent todo store.",
            )
        })?;
        let mut state = self.agent_workflow_state.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_workflow_state_lock_failed",
                "Xero could not lock the custom-agent workflow state.",
            )
        })?;
        policy.advance_state(&mut state, &todos);
        let Some(phase) = policy.phase(&state.current_phase_id) else {
            return Err(CommandError::policy_denied(
                "Xero stopped this custom workflow because its current phase is not declared.",
            ));
        };
        if let Some(retry_limit) = phase.retry_limit {
            let failures = state
                .phase_failures
                .get(&phase.id)
                .copied()
                .unwrap_or_default();
            if failures > retry_limit {
                return Err(CommandError::policy_denied(format!(
                    "Xero stopped custom workflow phase `{}` because its retryLimit of {} was exceeded.",
                    phase.title, retry_limit
                )));
            }
        }
        if !phase.allows_tool(tool_name) {
            return Err(CommandError::policy_denied(format!(
                "Xero refused tool `{tool_name}` because custom workflow phase `{}` has not satisfied its required gates.",
                phase.title
            )));
        }
        Ok(())
    }

    fn record_agent_workflow_after_tool(
        &self,
        tool_name: &str,
        succeeded: bool,
    ) -> CommandResult<()> {
        let Some(policy) = self.agent_workflow_policy.as_ref() else {
            return Ok(());
        };
        let todos = self.todo_items.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_todo_lock_failed",
                "Xero could not lock the owned-agent todo store.",
            )
        })?;
        let mut state = self.agent_workflow_state.lock().map_err(|_| {
            CommandError::system_fault(
                "agent_workflow_state_lock_failed",
                "Xero could not lock the custom-agent workflow state.",
            )
        })?;
        if succeeded {
            *state
                .tool_successes
                .entry(tool_name.to_owned())
                .or_default() += 1;
        } else {
            let current_phase_id = state.current_phase_id.clone();
            *state.phase_failures.entry(current_phase_id).or_default() += 1;
        }
        policy.advance_state(&mut state, &todos);
        Ok(())
    }

    fn check_cancelled(&self) -> CommandResult<()> {
        if self.is_cancelled() {
            if let Some(token) = &self.cancellation_token {
                token.check_cancelled()?;
            }
            return Err(crate::runtime::cancelled_error());
        }
        Ok(())
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .as_ref()
            .is_some_and(AgentRunCancellationToken::is_cancelled)
            || self
                .tool_execution_cancelled
                .as_ref()
                .is_some_and(|is_cancelled| is_cancelled())
    }

    pub fn harness_runner(
        &self,
        _request: AutonomousHarnessRunnerRequest,
    ) -> CommandResult<AutonomousToolResult> {
        Err(CommandError::user_fixable(
            "autonomous_harness_runner_requires_agent_core",
            "The harness_runner tool needs the active Tool Registry V2 manifest and can only run through the owned-agent provider loop.",
        ))
    }

    pub fn execute(&self, request: AutonomousToolRequest) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        self.consume_delegated_tool_call_budget()?;
        let tool_name = request.tool_name();
        self.enforce_agent_workflow_before_tool(tool_name)?;
        let result = self.execute_without_workflow(request);
        self.record_agent_workflow_after_tool(tool_name, result.is_ok())?;
        result
    }

    fn execute_without_workflow(
        &self,
        request: AutonomousToolRequest,
    ) -> CommandResult<AutonomousToolResult> {
        match request {
            AutonomousToolRequest::Read(request) => self.read(request),
            AutonomousToolRequest::Search(request) => self.search(request),
            AutonomousToolRequest::Find(request) => self.find(request),
            AutonomousToolRequest::GitStatus(request) => self.git_status(request),
            AutonomousToolRequest::GitDiff(request) => self.git_diff(request),
            AutonomousToolRequest::ToolAccess(request) => self.tool_access(request),
            AutonomousToolRequest::HarnessRunner(request) => self.harness_runner(request),
            AutonomousToolRequest::WebSearch(request) => self.web_search(request),
            AutonomousToolRequest::WebFetch(request) => self.web_fetch(request),
            AutonomousToolRequest::Edit(request) => self.edit(request),
            AutonomousToolRequest::Write(request) => self.write(request),
            AutonomousToolRequest::Patch(request) => self.patch(request),
            AutonomousToolRequest::Delete(request) => self.delete(request),
            AutonomousToolRequest::Rename(request) => self.rename(request),
            AutonomousToolRequest::Mkdir(request) => self.mkdir(request),
            AutonomousToolRequest::List(request) => self.list(request),
            AutonomousToolRequest::Hash(request) => self.hash(request),
            AutonomousToolRequest::Command(request) => self.command(request),
            AutonomousToolRequest::CommandSessionStart(request) => {
                self.command_session_start(request)
            }
            AutonomousToolRequest::CommandSessionRead(request) => {
                self.command_session_read(request)
            }
            AutonomousToolRequest::CommandSessionStop(request) => {
                self.command_session_stop(request)
            }
            AutonomousToolRequest::ProcessManager(request) => self.process_manager(request),
            AutonomousToolRequest::SystemDiagnostics(request) => self.system_diagnostics(request),
            AutonomousToolRequest::MacosAutomation(request) => self.macos_automation(request),
            AutonomousToolRequest::Mcp(request) => self.mcp(request),
            AutonomousToolRequest::Subagent(request) => self.subagent(request),
            AutonomousToolRequest::Todo(request) => self.todo(request),
            AutonomousToolRequest::NotebookEdit(request) => self.notebook_edit(request),
            AutonomousToolRequest::CodeIntel(request) => self.code_intel(request),
            AutonomousToolRequest::Lsp(request) => self.lsp(request),
            AutonomousToolRequest::PowerShell(request) => self.powershell(request),
            AutonomousToolRequest::ToolSearch(request) => self.tool_search(request),
            AutonomousToolRequest::EnvironmentContext(request) => self.environment_context(request),
            AutonomousToolRequest::ProjectContext(request) => self.project_context(request),
            AutonomousToolRequest::WorkspaceIndex(request) => self.workspace_index(request),
            AutonomousToolRequest::AgentCoordination(request) => self.agent_coordination(request),
            AutonomousToolRequest::AgentDefinition(request) => self.agent_definition(request),
            AutonomousToolRequest::Skill(request) => self.skill(request),
            AutonomousToolRequest::Browser(request) => self.browser(request),
            AutonomousToolRequest::Emulator(request) => self.emulator(request),
            AutonomousToolRequest::SolanaCluster(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_CLUSTER, |executor| {
                    executor.cluster(request)
                }),
            AutonomousToolRequest::SolanaLogs(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_LOGS, |executor| {
                    executor.logs(request)
                }),
            AutonomousToolRequest::SolanaTx(request) => {
                self.solana(AUTONOMOUS_TOOL_SOLANA_TX, |executor| executor.tx(request))
            }
            AutonomousToolRequest::SolanaSimulate(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_SIMULATE, |executor| {
                    executor.simulate(request)
                }),
            AutonomousToolRequest::SolanaExplain(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_EXPLAIN, |executor| {
                    executor.explain(request)
                }),
            AutonomousToolRequest::SolanaAlt(request) => {
                self.solana(AUTONOMOUS_TOOL_SOLANA_ALT, |executor| executor.alt(request))
            }
            AutonomousToolRequest::SolanaIdl(request) => {
                self.solana(AUTONOMOUS_TOOL_SOLANA_IDL, |executor| executor.idl(request))
            }
            AutonomousToolRequest::SolanaCodama(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_CODAMA, |executor| {
                    executor.codama(request)
                }),
            AutonomousToolRequest::SolanaPda(request) => {
                self.solana(AUTONOMOUS_TOOL_SOLANA_PDA, |executor| executor.pda(request))
            }
            AutonomousToolRequest::SolanaProgram(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_PROGRAM, |executor| {
                    executor.program(request)
                }),
            AutonomousToolRequest::SolanaDeploy(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_DEPLOY, |executor| {
                    executor.deploy(request)
                }),
            AutonomousToolRequest::SolanaUpgradeCheck(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK, |executor| {
                    executor.upgrade_check(request)
                }),
            AutonomousToolRequest::SolanaSquads(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_SQUADS, |executor| {
                    executor.squads(request)
                }),
            AutonomousToolRequest::SolanaVerifiedBuild(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD, |executor| {
                    executor.verified_build(request)
                }),
            AutonomousToolRequest::SolanaAuditStatic(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC, |executor| {
                    executor.audit(request)
                }),
            AutonomousToolRequest::SolanaAuditExternal(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL, |executor| {
                    executor.audit(request)
                }),
            AutonomousToolRequest::SolanaAuditFuzz(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ, |executor| {
                    executor.audit(request)
                }),
            AutonomousToolRequest::SolanaAuditCoverage(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE, |executor| {
                    executor.audit(request)
                }),
            AutonomousToolRequest::SolanaReplay(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_REPLAY, |executor| {
                    executor.replay(request)
                }),
            AutonomousToolRequest::SolanaIndexer(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_INDEXER, |executor| {
                    executor.indexer(request)
                }),
            AutonomousToolRequest::SolanaSecrets(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_SECRETS, |executor| {
                    executor.secrets(request)
                }),
            AutonomousToolRequest::SolanaClusterDrift(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT, |executor| {
                    executor.drift(request)
                }),
            AutonomousToolRequest::SolanaCost(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_COST, |executor| {
                    executor.cost(request)
                }),
            AutonomousToolRequest::SolanaDocs(request) => self
                .solana(AUTONOMOUS_TOOL_SOLANA_DOCS, |executor| {
                    executor.docs(request)
                }),
        }
    }

    fn consume_delegated_tool_call_budget(&self) -> CommandResult<()> {
        let Some(budget) = self.delegated_tool_call_budget.as_ref() else {
            return Ok(());
        };
        let mut remaining = budget.remaining.lock().map_err(|_| {
            CommandError::system_fault(
                "autonomous_tool_subagent_budget_lock_failed",
                "Xero could not lock the subagent delegated tool-call budget.",
            )
        })?;
        if *remaining == 0 {
            return Err(CommandError::user_fixable(
                "autonomous_tool_subagent_tool_budget_exhausted",
                format!(
                    "Xero stopped subagent `{}` because its delegated tool-call budget is exhausted.",
                    budget.owner
                ),
            ));
        }
        *remaining -= 1;
        Ok(())
    }

    pub fn execute_approved(
        &self,
        request: AutonomousToolRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        let tool_name = request.tool_name();
        self.enforce_agent_workflow_before_tool(tool_name)?;
        let result = match request {
            AutonomousToolRequest::Read(request) => self.read_with_operator_approval(request),
            AutonomousToolRequest::Command(request) => self.command_with_operator_approval(request),
            AutonomousToolRequest::CommandSessionStart(request) => {
                self.command_session_start_with_operator_approval(request)
            }
            AutonomousToolRequest::PowerShell(request) => {
                self.powershell_with_operator_approval(request)
            }
            AutonomousToolRequest::ProcessManager(request) => {
                self.process_manager_with_operator_approval(request)
            }
            AutonomousToolRequest::SystemDiagnostics(request) => {
                self.system_diagnostics_with_operator_approval(request)
            }
            AutonomousToolRequest::MacosAutomation(request) => {
                self.macos_automation_with_operator_approval(request)
            }
            AutonomousToolRequest::AgentDefinition(request) => {
                self.agent_definition_with_operator_approval(request)
            }
            request => self.execute_without_workflow(request),
        };
        self.record_agent_workflow_after_tool(tool_name, result.is_ok())?;
        result
    }

    fn solana<F>(&self, tool_name: &'static str, run: F) -> CommandResult<AutonomousToolResult>
    where
        F: FnOnce(&dyn SolanaExecutor) -> CommandResult<AutonomousSolanaOutput>,
    {
        let executor = self.solana_executor.as_ref().ok_or_else(|| {
            CommandError::policy_denied(
                "Solana actions require the desktop runtime; no SolanaState is wired.",
            )
        })?;
        let output = run(executor.as_ref())?;
        let summary = format!(
            "Executed Solana action `{}` with `{tool_name}`.",
            output.action
        );
        Ok(AutonomousToolResult {
            tool_name: tool_name.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Solana(output),
        })
    }

    pub fn browser(
        &self,
        request: AutonomousBrowserRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let executor = self.browser_executor.as_ref().ok_or_else(|| {
            CommandError::policy_denied(
                "Browser actions require the desktop runtime; no executor is wired.",
            )
        })?;
        let action_summary = format!("Browser action {:?}", request.action);
        let output = executor.execute(request.action)?;
        let summary = if let Some(url) = &output.url {
            format!("Executed browser action `{}` on `{}`.", output.action, url)
        } else {
            format!(
                "Executed browser action `{}` ({action_summary}).",
                output.action
            )
        };
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_BROWSER.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Browser(output),
        })
    }

    pub fn web_search(
        &self,
        request: AutonomousWebSearchRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = self.web_runtime.search(request)?;
        let result_count = output.results.len();
        let summary = if result_count == 0 {
            format!("Web search returned 0 result(s) for `{}`.", output.query)
        } else if output.truncated {
            format!(
                "Web search returned {result_count} result(s) for `{}` (truncated).",
                output.query
            )
        } else {
            format!(
                "Web search returned {result_count} result(s) for `{}`.",
                output.query
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WEB_SEARCH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::WebSearch(output),
        })
    }

    pub fn tool_access(
        &self,
        request: AutonomousToolAccessRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = match request.action {
            AutonomousToolAccessAction::List => AutonomousToolAccessOutput {
                action: "list".into(),
                granted_tools: Vec::new(),
                denied_tools: Vec::new(),
                available_groups: self.available_tool_access_groups(),
                available_tool_packs: self.available_tool_pack_manifests(),
                tool_pack_health: self.tool_pack_health_reports(),
                exposure_diagnostics: Some(Self::tool_access_exposure_diagnostics(None)),
                message:
                    "Available tool groups returned. Request a group or specific tool by name."
                        .into(),
            },
            AutonomousToolAccessAction::Request => {
                let mut requested = std::collections::BTreeSet::new();
                let mut denied = std::collections::BTreeSet::new();
                for group in request.groups {
                    match tool_access_group_tools(&group) {
                        Some(tools) => {
                            for tool in tools {
                                if self.tool_available_by_runtime(tool)
                                    && self.tool_allowed_by_active_agent(tool)
                                {
                                    requested.insert((*tool).to_owned());
                                } else {
                                    denied.insert(group.clone());
                                }
                            }
                        }
                        None => {
                            denied.insert(group);
                        }
                    }
                }

                let known_tools = tool_access_all_known_tools();
                for tool in request.tools {
                    let runtime_tool_available = known_tools.contains(tool.as_str())
                        && self.tool_available_by_runtime(tool.as_str())
                        && self.tool_allowed_by_active_agent(tool.as_str());
                    let dynamic_tool_available =
                        self.active_runtime_agent_id().allows_engineering_tools()
                            && self
                                .agent_tool_policy
                                .as_ref()
                                .map(|policy| policy.allows_tool(&tool))
                                .unwrap_or(true)
                            && self.dynamic_tool_descriptor(&tool)?.is_some();
                    if runtime_tool_available || dynamic_tool_available {
                        requested.insert(tool);
                    } else {
                        denied.insert(tool);
                    }
                }

                AutonomousToolAccessOutput {
                    action: "request".into(),
                    granted_tools: requested.into_iter().collect(),
                    denied_tools: denied.into_iter().collect(),
                    available_groups: self.available_tool_access_groups(),
                    available_tool_packs: self.available_tool_pack_manifests(),
                    tool_pack_health: self.tool_pack_health_reports(),
                    exposure_diagnostics: Some(Self::tool_access_exposure_diagnostics(
                        request.reason.as_deref(),
                    )),
                    message: "Requested tools will be exposed on the next provider turn.".into(),
                }
            }
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_TOOL_ACCESS.into(),
            summary: output.message.clone(),
            command_result: None,
            output: AutonomousToolOutput::ToolAccess(output),
        })
    }

    fn available_tool_access_groups(&self) -> Vec<AutonomousToolAccessGroup> {
        tool_access_group_descriptors()
            .into_iter()
            .filter_map(|mut group| {
                group.tools.retain(|tool| {
                    self.tool_available_by_runtime(tool) && self.tool_allowed_by_active_agent(tool)
                });
                if group.tools.is_empty() {
                    return None;
                }
                if group.name == "skills"
                    && !group
                        .tools
                        .iter()
                        .any(|tool| self.tool_available_by_runtime(tool))
                {
                    return None;
                }
                Some(group)
            })
            .collect()
    }

    fn tool_access_exposure_diagnostics(reason: Option<&str>) -> JsonValue {
        json!({
            "schema": "xero.tool_exposure_diagnostics.v1",
            "planner": "capability_planner_v1",
            "requestReason": reason,
            "traceLocation": "ToolRegistrySnapshot.exposurePlan",
            "activationTraceEvent": "PolicyDecision(kind=tool_exposure_activation)",
            "reasonSources": [
                "startup_core",
                "planner_classification",
                "user_explicit_tool_marker",
                "custom_policy",
                "tool_access_request",
                "verification_gate"
            ],
        })
    }

    fn available_tool_pack_manifests(&self) -> Vec<DomainToolPackManifest> {
        domain_tool_pack_manifests()
            .into_iter()
            .filter(|manifest| self.tool_pack_enabled_by_policy(manifest))
            .collect()
    }

    pub fn tool_pack_health_reports(&self) -> Vec<DomainToolPackHealthReport> {
        domain_tool_pack_manifests()
            .into_iter()
            .map(|manifest| {
                let available_prerequisites = self.available_prerequisites_for_tool_pack(&manifest);
                let input = DomainToolPackHealthInput {
                    pack_id: manifest.pack_id.clone(),
                    enabled_by_policy: self.tool_pack_enabled_by_policy(&manifest),
                    available_prerequisites,
                    checked_at: crate::auth::now_timestamp(),
                };
                domain_tool_pack_health_report(&manifest, &input)
            })
            .collect()
    }

    fn tool_pack_enabled_by_policy(&self, manifest: &DomainToolPackManifest) -> bool {
        manifest.tools.iter().any(|tool| {
            self.tool_available_by_runtime(tool) && self.tool_allowed_by_active_agent(tool)
        })
    }

    fn available_prerequisites_for_tool_pack(
        &self,
        manifest: &DomainToolPackManifest,
    ) -> Vec<String> {
        let mut available = BTreeSet::new();
        for prerequisite in &manifest.prerequisites {
            if self
                .tool_pack_prerequisite_available(&manifest.pack_id, &prerequisite.prerequisite_id)
            {
                available.insert(prerequisite.prerequisite_id.clone());
            }
        }
        available.into_iter().collect()
    }

    fn tool_pack_prerequisite_available(&self, pack_id: &str, prerequisite_id: &str) -> bool {
        match (pack_id, prerequisite_id) {
            ("browser", "desktop_browser_executor") => self.browser_executor.is_some(),
            ("browser", "webview_runtime") => self.browser_executor.is_some(),
            ("emulator", "desktop_emulator_executor") => self.emulator_executor.is_some(),
            ("emulator", "adb") => executable_on_path("adb"),
            ("emulator", "xcrun") => executable_on_path("xcrun"),
            ("solana", "solana_state_executor") => self.solana_executor.is_some(),
            ("solana", "solana") => executable_on_path("solana"),
            ("solana", "anchor") => executable_on_path("anchor"),
            ("os_automation", "desktop_runtime") => true,
            ("os_automation", "macos_platform") => cfg!(target_os = "macos"),
            ("os_automation", "accessibility_permission")
            | ("os_automation", "screen_recording_permission") => false,
            ("project_context", "repo_root") => self.repo_root.is_dir(),
            ("project_context", "app_data_store") => {
                self.agent_run_context.is_some()
                    || self.environment_profile_database_path.is_some()
                    || self.skill_tool_enabled()
            }
            ("project_context", "workspace_index_store") => {
                crate::db::project_app_data_dir_for_repo(&self.repo_root)
                    .join("workspace-index")
                    .exists()
            }
            _ => false,
        }
    }

    pub(super) fn active_runtime_agent_id(&self) -> RuntimeAgentIdDto {
        self.command_controls
            .as_ref()
            .map(|controls| controls.active.runtime_agent_id)
            .unwrap_or(RuntimeAgentIdDto::Ask)
    }

    fn tool_allowed_by_active_agent(&self, tool: &str) -> bool {
        tool_allowed_for_runtime_agent_with_policy(
            self.active_runtime_agent_id(),
            tool,
            self.agent_tool_policy.as_ref(),
        )
    }

    fn tool_available_by_runtime(&self, tool: &str) -> bool {
        if !tool_available_on_current_host(tool) {
            return false;
        }
        if tool == AUTONOMOUS_TOOL_SKILL {
            return self.skill_tool_enabled();
        }
        let pack_ids = domain_tool_pack_ids_for_tool(tool);
        if pack_ids.iter().any(|pack_id| pack_id == "browser") {
            return self.browser_executor.is_some();
        }
        if pack_ids.iter().any(|pack_id| pack_id == "emulator") {
            return self.emulator_executor.is_some();
        }
        if pack_ids.iter().any(|pack_id| pack_id == "solana") {
            return self.solana_executor.is_some();
        }
        true
    }

    pub fn web_fetch(
        &self,
        request: AutonomousWebFetchRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let output = self.web_runtime.fetch(request)?;
        let kind = match output.content_kind {
            AutonomousWebFetchContentKind::Html => "HTML",
            AutonomousWebFetchContentKind::PlainText => "plain-text",
        };
        let summary = if output.truncated {
            format!(
                "Fetched {kind} content from `{}` via `{}` (truncated).",
                output.url, output.final_url
            )
        } else {
            format!(
                "Fetched {kind} content from `{}` via `{}`.",
                output.url, output.final_url
            )
        };

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_WEB_FETCH.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::WebFetch(output),
        })
    }

    pub fn emulator(
        &self,
        request: AutonomousEmulatorRequest,
    ) -> CommandResult<AutonomousToolResult> {
        let executor = self.emulator_executor.as_ref().ok_or_else(|| {
            CommandError::policy_denied(
                "Emulator actions require the desktop runtime; no executor is wired.",
            )
        })?;
        let output = executor.execute(request.action, request.input)?;
        let summary = format!("Executed emulator action `{}`.", output.action);
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_EMULATOR.into(),
            summary,
            command_result: None,
            output: AutonomousToolOutput::Emulator(output),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "tool", content = "input")]
pub enum AutonomousToolRequest {
    Read(AutonomousReadRequest),
    Search(AutonomousSearchRequest),
    Find(AutonomousFindRequest),
    GitStatus(AutonomousGitStatusRequest),
    GitDiff(AutonomousGitDiffRequest),
    ToolAccess(AutonomousToolAccessRequest),
    HarnessRunner(AutonomousHarnessRunnerRequest),
    WebSearch(AutonomousWebSearchRequest),
    WebFetch(AutonomousWebFetchRequest),
    Edit(AutonomousEditRequest),
    Write(AutonomousWriteRequest),
    Patch(AutonomousPatchRequest),
    Delete(AutonomousDeleteRequest),
    Rename(AutonomousRenameRequest),
    Mkdir(AutonomousMkdirRequest),
    List(AutonomousListRequest),
    #[serde(rename = "file_hash")]
    Hash(AutonomousHashRequest),
    Command(AutonomousCommandRequest),
    CommandSessionStart(AutonomousCommandSessionStartRequest),
    CommandSessionRead(AutonomousCommandSessionReadRequest),
    CommandSessionStop(AutonomousCommandSessionStopRequest),
    ProcessManager(AutonomousProcessManagerRequest),
    SystemDiagnostics(AutonomousSystemDiagnosticsRequest),
    MacosAutomation(AutonomousMacosAutomationRequest),
    Mcp(AutonomousMcpRequest),
    Subagent(AutonomousSubagentRequest),
    Todo(AutonomousTodoRequest),
    NotebookEdit(AutonomousNotebookEditRequest),
    CodeIntel(AutonomousCodeIntelRequest),
    Lsp(AutonomousLspRequest),
    #[serde(rename = "powershell")]
    PowerShell(AutonomousPowerShellRequest),
    ToolSearch(AutonomousToolSearchRequest),
    EnvironmentContext(AutonomousEnvironmentContextRequest),
    ProjectContext(AutonomousProjectContextRequest),
    WorkspaceIndex(AutonomousWorkspaceIndexRequest),
    AgentCoordination(AutonomousAgentCoordinationRequest),
    AgentDefinition(AutonomousAgentDefinitionRequest),
    Skill(XeroSkillToolInput),
    Browser(AutonomousBrowserRequest),
    Emulator(AutonomousEmulatorRequest),
    SolanaCluster(AutonomousSolanaClusterRequest),
    SolanaLogs(AutonomousSolanaLogsRequest),
    SolanaTx(AutonomousSolanaTxRequest),
    SolanaSimulate(AutonomousSolanaSimulateRequest),
    SolanaExplain(AutonomousSolanaExplainRequest),
    SolanaAlt(AutonomousSolanaAltRequest),
    SolanaIdl(AutonomousSolanaIdlRequest),
    SolanaCodama(AutonomousSolanaCodamaRequest),
    SolanaPda(AutonomousSolanaPdaRequest),
    SolanaProgram(AutonomousSolanaProgramRequest),
    SolanaDeploy(AutonomousSolanaDeployRequest),
    SolanaUpgradeCheck(AutonomousSolanaUpgradeCheckRequest),
    SolanaSquads(AutonomousSolanaSquadsRequest),
    SolanaVerifiedBuild(AutonomousSolanaVerifiedBuildRequest),
    SolanaAuditStatic(AutonomousSolanaAuditRequest),
    SolanaAuditExternal(AutonomousSolanaAuditRequest),
    SolanaAuditFuzz(AutonomousSolanaAuditRequest),
    SolanaAuditCoverage(AutonomousSolanaAuditRequest),
    SolanaReplay(AutonomousSolanaReplayRequest),
    SolanaIndexer(AutonomousSolanaIndexerRequest),
    SolanaSecrets(AutonomousSolanaSecretsRequest),
    SolanaClusterDrift(AutonomousSolanaDriftRequest),
    SolanaCost(AutonomousSolanaCostRequest),
    SolanaDocs(AutonomousSolanaDocsRequest),
}

impl AutonomousToolRequest {
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::Read(_) => AUTONOMOUS_TOOL_READ,
            Self::Search(_) => AUTONOMOUS_TOOL_SEARCH,
            Self::Find(_) => AUTONOMOUS_TOOL_FIND,
            Self::GitStatus(_) => AUTONOMOUS_TOOL_GIT_STATUS,
            Self::GitDiff(_) => AUTONOMOUS_TOOL_GIT_DIFF,
            Self::ToolAccess(_) => AUTONOMOUS_TOOL_TOOL_ACCESS,
            Self::HarnessRunner(_) => AUTONOMOUS_TOOL_HARNESS_RUNNER,
            Self::WebSearch(_) => AUTONOMOUS_TOOL_WEB_SEARCH,
            Self::WebFetch(_) => AUTONOMOUS_TOOL_WEB_FETCH,
            Self::Edit(_) => AUTONOMOUS_TOOL_EDIT,
            Self::Write(_) => AUTONOMOUS_TOOL_WRITE,
            Self::Patch(_) => AUTONOMOUS_TOOL_PATCH,
            Self::Delete(_) => AUTONOMOUS_TOOL_DELETE,
            Self::Rename(_) => AUTONOMOUS_TOOL_RENAME,
            Self::Mkdir(_) => AUTONOMOUS_TOOL_MKDIR,
            Self::List(_) => AUTONOMOUS_TOOL_LIST,
            Self::Hash(_) => AUTONOMOUS_TOOL_HASH,
            Self::Command(_) => AUTONOMOUS_TOOL_COMMAND,
            Self::CommandSessionStart(_) => AUTONOMOUS_TOOL_COMMAND_SESSION_START,
            Self::CommandSessionRead(_) => AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
            Self::CommandSessionStop(_) => AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
            Self::ProcessManager(_) => AUTONOMOUS_TOOL_PROCESS_MANAGER,
            Self::SystemDiagnostics(_) => AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS,
            Self::MacosAutomation(_) => AUTONOMOUS_TOOL_MACOS_AUTOMATION,
            Self::Mcp(_) => AUTONOMOUS_TOOL_MCP,
            Self::Subagent(_) => AUTONOMOUS_TOOL_SUBAGENT,
            Self::Todo(_) => AUTONOMOUS_TOOL_TODO,
            Self::NotebookEdit(_) => AUTONOMOUS_TOOL_NOTEBOOK_EDIT,
            Self::CodeIntel(_) => AUTONOMOUS_TOOL_CODE_INTEL,
            Self::Lsp(_) => AUTONOMOUS_TOOL_LSP,
            Self::PowerShell(_) => AUTONOMOUS_TOOL_POWERSHELL,
            Self::ToolSearch(_) => AUTONOMOUS_TOOL_TOOL_SEARCH,
            Self::EnvironmentContext(_) => AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
            Self::ProjectContext(request) => project_context_tool_name(request.action),
            Self::WorkspaceIndex(_) => AUTONOMOUS_TOOL_WORKSPACE_INDEX,
            Self::AgentCoordination(_) => AUTONOMOUS_TOOL_AGENT_COORDINATION,
            Self::AgentDefinition(_) => AUTONOMOUS_TOOL_AGENT_DEFINITION,
            Self::Skill(_) => AUTONOMOUS_TOOL_SKILL,
            Self::Browser(request) => browser_tool_name(&request.action),
            Self::Emulator(_) => AUTONOMOUS_TOOL_EMULATOR,
            Self::SolanaCluster(_) => AUTONOMOUS_TOOL_SOLANA_CLUSTER,
            Self::SolanaLogs(_) => AUTONOMOUS_TOOL_SOLANA_LOGS,
            Self::SolanaTx(_) => AUTONOMOUS_TOOL_SOLANA_TX,
            Self::SolanaSimulate(_) => AUTONOMOUS_TOOL_SOLANA_SIMULATE,
            Self::SolanaExplain(_) => AUTONOMOUS_TOOL_SOLANA_EXPLAIN,
            Self::SolanaAlt(_) => AUTONOMOUS_TOOL_SOLANA_ALT,
            Self::SolanaIdl(_) => AUTONOMOUS_TOOL_SOLANA_IDL,
            Self::SolanaCodama(_) => AUTONOMOUS_TOOL_SOLANA_CODAMA,
            Self::SolanaPda(_) => AUTONOMOUS_TOOL_SOLANA_PDA,
            Self::SolanaProgram(_) => AUTONOMOUS_TOOL_SOLANA_PROGRAM,
            Self::SolanaDeploy(_) => AUTONOMOUS_TOOL_SOLANA_DEPLOY,
            Self::SolanaUpgradeCheck(_) => AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
            Self::SolanaSquads(_) => AUTONOMOUS_TOOL_SOLANA_SQUADS,
            Self::SolanaVerifiedBuild(_) => AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
            Self::SolanaAuditStatic(_) => AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
            Self::SolanaAuditExternal(_) => AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
            Self::SolanaAuditFuzz(_) => AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ,
            Self::SolanaAuditCoverage(_) => AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE,
            Self::SolanaReplay(_) => AUTONOMOUS_TOOL_SOLANA_REPLAY,
            Self::SolanaIndexer(_) => AUTONOMOUS_TOOL_SOLANA_INDEXER,
            Self::SolanaSecrets(_) => AUTONOMOUS_TOOL_SOLANA_SECRETS,
            Self::SolanaClusterDrift(_) => AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
            Self::SolanaCost(_) => AUTONOMOUS_TOOL_SOLANA_COST,
            Self::SolanaDocs(_) => AUTONOMOUS_TOOL_SOLANA_DOCS,
        }
    }
}

fn project_context_tool_name(action: AutonomousProjectContextAction) -> &'static str {
    match action {
        AutonomousProjectContextAction::SearchProjectRecords
        | AutonomousProjectContextAction::SearchApprovedMemory
        | AutonomousProjectContextAction::ListRecentHandoffs
        | AutonomousProjectContextAction::ListActiveDecisionsConstraints
        | AutonomousProjectContextAction::ListOpenQuestionsBlockers
        | AutonomousProjectContextAction::ExplainCurrentContextPackage => {
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH
        }
        AutonomousProjectContextAction::GetProjectRecord
        | AutonomousProjectContextAction::GetMemory => AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
        AutonomousProjectContextAction::RecordContext
        | AutonomousProjectContextAction::ProposeRecordCandidate => {
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD
        }
        AutonomousProjectContextAction::UpdateContext => AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE,
        AutonomousProjectContextAction::RefreshFreshness => AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH,
    }
}

fn browser_tool_name(action: &AutonomousBrowserAction) -> &'static str {
    match action {
        AutonomousBrowserAction::Open { .. }
        | AutonomousBrowserAction::TabOpen { .. }
        | AutonomousBrowserAction::Navigate { .. }
        | AutonomousBrowserAction::Back
        | AutonomousBrowserAction::Forward
        | AutonomousBrowserAction::Reload
        | AutonomousBrowserAction::Stop
        | AutonomousBrowserAction::Click { .. }
        | AutonomousBrowserAction::Type { .. }
        | AutonomousBrowserAction::Scroll { .. }
        | AutonomousBrowserAction::PressKey { .. }
        | AutonomousBrowserAction::CookiesSet { .. }
        | AutonomousBrowserAction::StorageWrite { .. }
        | AutonomousBrowserAction::StorageClear { .. }
        | AutonomousBrowserAction::StateRestore { .. }
        | AutonomousBrowserAction::TabClose { .. }
        | AutonomousBrowserAction::TabFocus { .. } => AUTONOMOUS_TOOL_BROWSER_CONTROL,
        AutonomousBrowserAction::ReadText { .. }
        | AutonomousBrowserAction::Query { .. }
        | AutonomousBrowserAction::WaitForSelector { .. }
        | AutonomousBrowserAction::WaitForLoad { .. }
        | AutonomousBrowserAction::CurrentUrl
        | AutonomousBrowserAction::HistoryState
        | AutonomousBrowserAction::Screenshot
        | AutonomousBrowserAction::CookiesGet
        | AutonomousBrowserAction::StorageRead { .. }
        | AutonomousBrowserAction::ConsoleLogs { .. }
        | AutonomousBrowserAction::NetworkSummary { .. }
        | AutonomousBrowserAction::AccessibilityTree { .. }
        | AutonomousBrowserAction::StateSnapshot { .. }
        | AutonomousBrowserAction::HarnessExtensionContract
        | AutonomousBrowserAction::TabList => AUTONOMOUS_TOOL_BROWSER_OBSERVE,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadRequest {
    pub path: String,
    #[serde(default)]
    pub system_path: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<AutonomousReadMode>,
    pub start_line: Option<usize>,
    pub line_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<usize>,
    #[serde(default)]
    pub include_line_hashes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchRequest {
    pub query: String,
    pub path: Option<String>,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub ignore_case: bool,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_lines: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousReadMode {
    Auto,
    Text,
    Image,
    BinaryMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFindRequest {
    pub pattern: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitStatusRequest {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitDiffRequest {
    pub scope: RepositoryDiffScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousToolAccessAction {
    List,
    Request,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolAccessRequest {
    pub action: AutonomousToolAccessAction,
    #[serde(default)]
    pub groups: Vec<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEditRequest {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub expected: String,
    pub replacement: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWriteRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPatchRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
    #[serde(default)]
    pub replace_all: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<AutonomousPatchOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPatchOperation {
    pub path: String,
    pub search: String,
    pub replace: String,
    #[serde(default)]
    pub replace_all: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDeleteRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRenameRequest {
    pub from_path: String,
    pub to_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMkdirRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHashRequest {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandRequest {
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandSessionStartRequest {
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandSessionReadRequest {
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_sequence: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandSessionStopRequest {
    pub session_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessManagerAction {
    Start,
    List,
    Status,
    Output,
    Digest,
    WaitForReady,
    Highlights,
    Send,
    SendAndWait,
    Run,
    Env,
    Signal,
    Kill,
    Restart,
    GroupStatus,
    GroupKill,
    AsyncStart,
    AsyncAwait,
    AsyncCancel,
    SystemProcessList,
    SystemProcessTree,
    SystemPortList,
    SystemSignal,
    SystemKillTree,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessOwnershipScope {
    XeroOwned,
    External,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessActionRiskLevel {
    Observe,
    RunOwned,
    SignalOwned,
    SignalExternal,
    PersistentBackground,
    SystemRead,
    OsAutomation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessManagerRequest {
    pub action: AutonomousProcessManagerAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default)]
    pub shell_mode: bool,
    #[serde(default)]
    pub interactive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ownership: Option<AutonomousProcessOwnershipScope>,
    #[serde(default)]
    pub persistent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_cursor: Option<u64>,
    #[serde(default)]
    pub since_last_read: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail_lines: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<AutonomousProcessOutputStream>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSystemDiagnosticsAction {
    ProcessOpenFiles,
    ProcessResourceSnapshot,
    ProcessThreads,
    ProcessSample,
    SystemLogQuery,
    MacosAccessibilitySnapshot,
    DiagnosticsBundle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSystemDiagnosticsPreset {
    HungProcess,
    PortConflict,
    TauriWindowIssue,
    MacosAppFocusIssue,
    HighCpuProcess,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSystemDiagnosticsFdKind {
    Cwd,
    Executable,
    File,
    Directory,
    Socket,
    Pipe,
    Device,
    Deleted,
    Other,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSystemDiagnosticsArtifactMode {
    None,
    Summary,
    Full,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSystemDiagnosticsLogLevel {
    Debug,
    Info,
    Notice,
    Error,
    Fault,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemDiagnosticsRequest {
    pub action: AutonomousSystemDiagnosticsAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<AutonomousSystemDiagnosticsPreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(default)]
    pub include_children: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_mode: Option<AutonomousSystemDiagnosticsArtifactMode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fd_kinds: Vec<AutonomousSystemDiagnosticsFdKind>,
    #[serde(default)]
    pub include_sockets: bool,
    #[serde(default)]
    pub include_files: bool,
    #[serde(default)]
    pub include_deleted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_count: Option<u32>,
    #[serde(default)]
    pub include_ports: bool,
    #[serde(default)]
    pub include_threads_summary: bool,
    #[serde(default)]
    pub include_wait_channel: bool,
    #[serde(default)]
    pub include_stack_hints: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_artifact_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<AutonomousSystemDiagnosticsLogLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subsystem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_contains: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_predicate: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    #[serde(default)]
    pub focused_only: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousMacosAutomationAction {
    MacPermissions,
    MacAppList,
    MacAppLaunch,
    MacAppActivate,
    MacAppQuit,
    MacWindowList,
    MacWindowFocus,
    MacScreenshot,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousMacosScreenshotTarget {
    Screen,
    Window,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosAutomationRequest {
    pub action: AutonomousMacosAutomationAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monitor_id: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_target: Option<AutonomousMacosScreenshotTarget>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousMcpAction {
    ListServers,
    ListTools,
    ListResources,
    ListPrompts,
    InvokeTool,
    ReadResource,
    GetPrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMcpRequest {
    pub action: AutonomousMcpAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSubagentAction {
    Spawn,
    Status,
    SendInput,
    Wait,
    FollowUp,
    Interrupt,
    Cancel,
    Close,
    Integrate,
    ExportTrace,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSubagentRole {
    Engineer,
    Debugger,
    Planner,
    Researcher,
    Reviewer,
    AgentBuilder,
    Browser,
    Emulator,
    Solana,
    Database,
}

impl AutonomousSubagentRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Engineer => "engineer",
            Self::Debugger => "debugger",
            Self::Planner => "planner",
            Self::Researcher => "researcher",
            Self::Reviewer => "reviewer",
            Self::AgentBuilder => "agent_builder",
            Self::Browser => "browser",
            Self::Emulator => "emulator",
            Self::Solana => "solana",
            Self::Database => "database",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Engineer => "Engineer",
            Self::Debugger => "Debugger",
            Self::Planner => "Planner",
            Self::Researcher => "Researcher",
            Self::Reviewer => "Reviewer",
            Self::AgentBuilder => "Agent Builder",
            Self::Browser => "Browser specialist",
            Self::Emulator => "Emulator specialist",
            Self::Solana => "Solana specialist",
            Self::Database => "Database specialist",
        }
    }

    pub const fn allows_write_set(self) -> bool {
        matches!(self, Self::Engineer | Self::Debugger)
    }

    pub const fn requires_write_set(self) -> bool {
        matches!(self, Self::Engineer)
    }

    pub const fn verification_contract(self) -> &'static str {
        match self {
            Self::Engineer => "Verify scoped edits before reporting completion.",
            Self::Debugger => "State the reproduction, evidence, fix, and verification result.",
            Self::Planner => "Return an actionable plan with assumptions and unresolved risks.",
            Self::Researcher => "Return source-grounded findings without mutating files.",
            Self::Reviewer => "Lead with concrete findings, risks, and test gaps.",
            Self::AgentBuilder => {
                "Return a least-privilege agent definition proposal and validation notes."
            }
            Self::Browser => {
                "Return browser evidence, selectors, screenshots, or accessibility findings."
            }
            Self::Emulator => {
                "Return device state, reproduction steps, and bounded automation evidence."
            }
            Self::Solana => {
                "Return cluster/program evidence and safety notes for chain-affecting work."
            }
            Self::Database => {
                "Return schema/query findings, migration risks, and verification notes."
            }
        }
    }
}

fn autonomous_subagent_role_from_str(value: &str) -> Option<AutonomousSubagentRole> {
    match value {
        "engineer" => Some(AutonomousSubagentRole::Engineer),
        "debugger" => Some(AutonomousSubagentRole::Debugger),
        "planner" => Some(AutonomousSubagentRole::Planner),
        "researcher" => Some(AutonomousSubagentRole::Researcher),
        "reviewer" => Some(AutonomousSubagentRole::Reviewer),
        "agent_builder" => Some(AutonomousSubagentRole::AgentBuilder),
        "browser" => Some(AutonomousSubagentRole::Browser),
        "emulator" => Some(AutonomousSubagentRole::Emulator),
        "solana" => Some(AutonomousSubagentRole::Solana),
        "database" => Some(AutonomousSubagentRole::Database),
        _ => None,
    }
}

pub(super) fn persist_subagent_task_for_parent(
    repo_root: &Path,
    project_id: &str,
    parent_run_id: &str,
    task: &AutonomousSubagentTask,
) -> CommandResult<()> {
    let record = agent_subagent_task_record_from_task(project_id, parent_run_id, task)?;
    project_store::upsert_agent_subagent_task(repo_root, &record)
}

pub(super) fn load_subagent_tasks_for_parent(
    repo_root: &Path,
    project_id: &str,
    parent_run_id: &str,
) -> CommandResult<BTreeMap<String, AutonomousSubagentTask>> {
    let records =
        project_store::list_agent_subagent_tasks_for_parent(repo_root, project_id, parent_run_id)?;
    records
        .into_iter()
        .map(|record| {
            let task = autonomous_subagent_task_from_record(record)?;
            Ok((task.subagent_id.clone(), task))
        })
        .collect()
}

fn agent_subagent_task_record_from_task(
    project_id: &str,
    parent_run_id: &str,
    task: &AutonomousSubagentTask,
) -> CommandResult<project_store::AgentSubagentTaskRecord> {
    let write_set_json = serde_json::to_string(&task.write_set).map_err(|error| {
        CommandError::system_fault(
            "agent_subagent_write_set_serialize_failed",
            format!("Xero could not serialize subagent writeSet state: {error}"),
        )
    })?;
    let input_log_json = serde_json::to_string(&task.input_log).map_err(|error| {
        CommandError::system_fault(
            "agent_subagent_input_log_serialize_failed",
            format!("Xero could not serialize subagent input log: {error}"),
        )
    })?;
    let budget_diagnostic_json = task
        .budget_diagnostic
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_subagent_budget_diagnostic_serialize_failed",
                format!("Xero could not serialize subagent budget diagnostics: {error}"),
            )
        })?;
    Ok(project_store::AgentSubagentTaskRecord {
        project_id: project_id.into(),
        parent_run_id: task
            .parent_run_id
            .as_deref()
            .unwrap_or(parent_run_id)
            .into(),
        subagent_id: task.subagent_id.clone(),
        role: task.role.as_str().into(),
        role_label: task.role_label.clone(),
        prompt_hash: subagent_prompt_hash(&task.prompt),
        prompt_preview: subagent_prompt_preview(&task.prompt),
        model_id: task.model_id.clone(),
        write_set_json,
        verification_contract: task.verification_contract.clone(),
        depth: task.depth as u64,
        max_tool_calls: task.max_tool_calls as u64,
        max_tokens: task.max_tokens,
        max_cost_micros: task.max_cost_micros,
        used_tool_calls: task.used_tool_calls as u64,
        used_tokens: task.used_tokens,
        used_cost_micros: task.used_cost_micros,
        budget_status: if task.budget_status.trim().is_empty() {
            "within_budget".into()
        } else {
            task.budget_status.clone()
        },
        budget_diagnostic_json,
        status: task.status.clone(),
        created_at: task.created_at.clone(),
        started_at: task.started_at.clone(),
        completed_at: task.completed_at.clone(),
        cancelled_at: task.cancelled_at.clone(),
        integrated_at: task.integrated_at.clone(),
        child_run_id: task.run_id.clone(),
        child_trace_id: task.trace_id.clone(),
        parent_trace_id: task.parent_trace_id.clone(),
        input_log_json,
        result_summary: task.result_summary.clone(),
        result_artifact: task.result_artifact.clone(),
        parent_decision: task.parent_decision.clone(),
        latest_summary: task.result_summary.clone(),
        updated_at: crate::auth::now_timestamp(),
    })
}

fn autonomous_subagent_task_from_record(
    record: project_store::AgentSubagentTaskRecord,
) -> CommandResult<AutonomousSubagentTask> {
    let role = autonomous_subagent_role_from_str(&record.role).ok_or_else(|| {
        CommandError::system_fault(
            "agent_subagent_role_decode_failed",
            format!(
                "Xero could not decode durable subagent role `{}`.",
                record.role
            ),
        )
    })?;
    let write_set =
        serde_json::from_str::<Vec<String>>(&record.write_set_json).map_err(|error| {
            CommandError::system_fault(
                "agent_subagent_write_set_decode_failed",
                format!("Xero could not decode durable subagent writeSet state: {error}"),
            )
        })?;
    let input_log =
        serde_json::from_str::<Vec<AutonomousSubagentInputRecord>>(&record.input_log_json)
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_subagent_input_log_decode_failed",
                    format!("Xero could not decode durable subagent input log: {error}"),
                )
            })?;
    let budget_diagnostic = record
        .budget_diagnostic_json
        .as_deref()
        .map(serde_json::from_str::<JsonValue>)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_subagent_budget_diagnostic_decode_failed",
                format!("Xero could not decode durable subagent budget diagnostics: {error}"),
            )
        })?;
    Ok(AutonomousSubagentTask {
        subagent_id: record.subagent_id,
        role,
        role_label: record.role_label,
        prompt: record.prompt_preview,
        model_id: record.model_id,
        write_set,
        verification_contract: record.verification_contract,
        depth: record.depth as usize,
        max_tool_calls: record.max_tool_calls as usize,
        max_tokens: record.max_tokens,
        max_cost_micros: record.max_cost_micros,
        used_tool_calls: record.used_tool_calls as usize,
        used_tokens: record.used_tokens,
        used_cost_micros: record.used_cost_micros,
        budget_status: record.budget_status,
        budget_diagnostic,
        status: record.status,
        created_at: record.created_at,
        started_at: record.started_at,
        completed_at: record.completed_at,
        cancelled_at: record.cancelled_at,
        integrated_at: record.integrated_at,
        run_id: record.child_run_id,
        trace_id: record.child_trace_id,
        parent_run_id: Some(record.parent_run_id),
        parent_trace_id: record.parent_trace_id,
        input_log,
        result_summary: record.result_summary,
        result_artifact: record.result_artifact,
        parent_decision: record.parent_decision,
    })
}

fn subagent_prompt_hash(prompt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn subagent_prompt_preview(prompt: &str) -> String {
    if let Some(reason) = find_prohibited_persistence_content(prompt) {
        return format!("[REDACTED: {reason}]");
    }
    prompt
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\n' | '\r' | '\t'))
        .take(512)
        .collect::<String>()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentRequest {
    pub action: AutonomousSubagentAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<AutonomousSubagentRole>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_micros: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_set: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentInputRecord {
    pub kind: String,
    pub text: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentTask {
    pub subagent_id: String,
    pub role: AutonomousSubagentRole,
    pub role_label: String,
    pub prompt: String,
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_set: Vec<String>,
    pub verification_contract: String,
    pub depth: usize,
    pub max_tool_calls: usize,
    pub max_tokens: u64,
    pub max_cost_micros: u64,
    #[serde(default)]
    pub used_tool_calls: usize,
    #[serde(default)]
    pub used_tokens: u64,
    #[serde(default)]
    pub used_cost_micros: u64,
    #[serde(default)]
    pub budget_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub budget_diagnostic: Option<JsonValue>,
    pub status: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancelled_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub integrated_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_trace_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_log: Vec<AutonomousSubagentInputRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_artifact: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_decision: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousTodoAction {
    List,
    Upsert,
    Complete,
    Delete,
    Clear,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousTodoStatus {
    Pending,
    InProgress,
    Completed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousHarnessRunnerAction {
    Manifest,
    CompareReport,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHarnessRunnerRequest {
    pub action: AutonomousHarnessRunnerAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_report: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHarnessRunnerOutput {
    pub schema: String,
    pub action: AutonomousHarnessRunnerAction,
    pub passed: bool,
    pub summary: String,
    pub manifest_version: String,
    pub manifest_signature: String,
    pub item_count: usize,
    pub comparison: JsonValue,
    pub items: Vec<JsonValue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousTodoMode {
    Plan,
    DebugEvidence,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDebugEvidenceStage {
    Symptom,
    Reproduction,
    Hypothesis,
    Experiment,
    RootCause,
    Fix,
    Verification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousTodoRequest {
    pub action: AutonomousTodoAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<AutonomousTodoStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<AutonomousTodoMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_stage: Option<AutonomousDebugEvidenceStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousTodoItem {
    pub id: String,
    pub title: String,
    pub notes: Option<String>,
    pub status: AutonomousTodoStatus,
    pub mode: AutonomousTodoMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug_stage: Option<AutonomousDebugEvidenceStage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slice_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub handoff_note: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousNotebookEditRequest {
    pub path: String,
    pub cell_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source: Option<String>,
    pub replacement_source: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousCodeIntelAction {
    Symbols,
    Diagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCodeIntelRequest {
    pub action: AutonomousCodeIntelAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousLspAction {
    Servers,
    Symbols,
    Diagnostics,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousLspRequest {
    pub action: AutonomousLspAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPowerShellRequest {
    pub script: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolSearchRequest {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousCommandPolicyOutcome {
    Allowed,
    Escalated,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousCommandPolicyProfile {
    ReadOnlyVerification,
    GeneratedFileMutation,
    DependencyInstallation,
    ExternalNetwork,
    DestructiveOperation,
    GeneralExecution,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSafetyPolicyAction {
    Allow,
    RequireApproval,
    Deny,
}

impl AutonomousSafetyPolicyAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::RequireApproval => "require_approval",
            Self::Deny => "deny",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSafetyApprovalGrant {
    pub scope: String,
    pub expires: String,
    pub replay_rule: String,
    pub input_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSafetyPolicyDecision {
    pub action: AutonomousSafetyPolicyAction,
    pub code: String,
    pub explanation: String,
    pub tool_name: String,
    pub risk_class: String,
    pub approval_mode: Option<RuntimeRunApprovalModeDto>,
    pub project_trust: String,
    pub network_intent: String,
    pub credential_sensitivity: String,
    pub os_target: Option<String>,
    pub prior_observation_required: bool,
    pub approval_grant: Option<AutonomousSafetyApprovalGrant>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandPolicyTrace {
    pub outcome: AutonomousCommandPolicyOutcome,
    pub approval_mode: RuntimeRunApprovalModeDto,
    pub profile: AutonomousCommandPolicyProfile,
    pub code: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolCommandResult {
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub summary: String,
    pub policy: AutonomousCommandPolicyTrace,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolResult {
    pub tool_name: String,
    pub summary: String,
    pub command_result: Option<AutonomousToolCommandResult>,
    pub output: AutonomousToolOutput,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
#[expect(
    clippy::large_enum_variant,
    reason = "Tool outputs are serialized immediately at an API boundary; boxing one variant would churn every matcher and fixture."
)]
pub enum AutonomousToolOutput {
    Read(AutonomousReadOutput),
    Search(AutonomousSearchOutput),
    Find(AutonomousFindOutput),
    GitStatus(AutonomousGitStatusOutput),
    GitDiff(AutonomousGitDiffOutput),
    ToolAccess(AutonomousToolAccessOutput),
    HarnessRunner(AutonomousHarnessRunnerOutput),
    WebSearch(AutonomousWebSearchOutput),
    WebFetch(AutonomousWebFetchOutput),
    Edit(AutonomousEditOutput),
    Write(AutonomousWriteOutput),
    Patch(AutonomousPatchOutput),
    Delete(AutonomousDeleteOutput),
    Rename(AutonomousRenameOutput),
    Mkdir(AutonomousMkdirOutput),
    List(AutonomousListOutput),
    Hash(AutonomousHashOutput),
    Command(AutonomousCommandOutput),
    CommandSession(AutonomousCommandSessionOutput),
    ProcessManager(AutonomousProcessManagerOutput),
    SystemDiagnostics(AutonomousSystemDiagnosticsOutput),
    MacosAutomation(AutonomousMacosAutomationOutput),
    Mcp(AutonomousMcpOutput),
    Subagent(AutonomousSubagentOutput),
    Todo(AutonomousTodoOutput),
    NotebookEdit(AutonomousNotebookEditOutput),
    CodeIntel(AutonomousCodeIntelOutput),
    Lsp(AutonomousLspOutput),
    ToolSearch(AutonomousToolSearchOutput),
    EnvironmentContext(AutonomousEnvironmentContextOutput),
    ProjectContext(AutonomousProjectContextOutput),
    WorkspaceIndex(AutonomousWorkspaceIndexOutput),
    AgentCoordination(AutonomousAgentCoordinationOutput),
    AgentDefinition(AutonomousAgentDefinitionOutput),
    Skill(AutonomousSkillToolOutput),
    Browser(AutonomousBrowserOutput),
    Emulator(AutonomousEmulatorOutput),
    Solana(AutonomousSolanaOutput),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadOutput {
    pub path: String,
    pub start_line: usize,
    pub line_count: usize,
    pub total_lines: usize,
    pub truncated: bool,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_kind: Option<AutonomousReadContentKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_hashes: Vec<AutonomousReadLineHash>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoding: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<AutonomousLineEnding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_bom: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_width: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_height: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_base64: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_bytes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub binary_excerpt_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchOutput {
    pub query: String,
    pub scope: Option<String>,
    pub matches: Vec<AutonomousSearchMatch>,
    pub scanned_files: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_matches: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_files: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default)]
    pub regex: bool,
    #[serde(default)]
    pub ignore_case: bool,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_globs: Vec<String>,
    #[serde(default)]
    pub context_lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFindOutput {
    pub pattern: String,
    pub scope: Option<String>,
    pub matches: Vec<String>,
    pub scanned_files: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchMatch {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub preview: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub match_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_before: Vec<AutonomousSearchContextLine>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_after: Vec<AutonomousSearchContextLine>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousReadContentKind {
    Text,
    Image,
    BinaryMetadata,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousLineEnding {
    None,
    Lf,
    Crlf,
    Mixed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadLineHash {
    pub line: usize,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchContextLine {
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitStatusOutput {
    pub branch: Option<BranchSummaryDto>,
    pub entries: Vec<RepositoryStatusEntryDto>,
    pub changed_files: usize,
    pub has_staged_changes: bool,
    pub has_unstaged_changes: bool,
    pub has_untracked_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousGitDiffOutput {
    pub scope: RepositoryDiffScope,
    pub branch: Option<BranchSummaryDto>,
    pub changed_files: usize,
    pub patch: String,
    pub truncated: bool,
    pub base_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolAccessGroup {
    pub name: String,
    pub description: String,
    pub tools: Vec<String>,
    pub risk_class: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolAccessOutput {
    pub action: String,
    pub granted_tools: Vec<String>,
    pub denied_tools: Vec<String>,
    pub available_groups: Vec<AutonomousToolAccessGroup>,
    #[serde(default)]
    pub available_tool_packs: Vec<DomainToolPackManifest>,
    #[serde(default)]
    pub tool_pack_health: Vec<DomainToolPackHealthReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exposure_diagnostics: Option<JsonValue>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousEditOutput {
    pub path: String,
    pub start_line: usize,
    pub end_line: usize,
    pub replacement_len: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<AutonomousLineEnding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bom_preserved: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWriteOutput {
    pub path: String,
    pub created: bool,
    pub bytes_written: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPatchOutput {
    pub path: String,
    pub replacements: usize,
    pub bytes_written: usize,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<AutonomousPatchFileOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<AutonomousPatchFailure>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_ending: Option<AutonomousLineEnding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bom_preserved: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPatchFileOutput {
    pub path: String,
    pub replacements: usize,
    pub bytes_written: usize,
    pub old_hash: String,
    pub new_hash: String,
    pub diff: String,
    pub line_ending: AutonomousLineEnding,
    pub bom_preserved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPatchFailure {
    pub operation_index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDeleteOutput {
    pub path: String,
    pub recursive: bool,
    pub existed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRenameOutput {
    pub from_path: String,
    pub to_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMkdirOutput {
    pub path: String,
    pub created: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListEntry {
    pub path: String,
    pub kind: String,
    pub bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListOutput {
    pub path: String,
    pub entries: Vec<AutonomousListEntry>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHashOutput {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandOutput {
    pub argv: Vec<String>,
    pub cwd: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub spawned: bool,
    pub policy: AutonomousCommandPolicyTrace,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxExecutionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousCommandSessionOperation {
    Start,
    Read,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousCommandSessionStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandOutputChunk {
    pub stream: AutonomousCommandSessionStream,
    pub text: Option<String>,
    pub truncated: bool,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandSessionChunk {
    pub sequence: u64,
    pub stream: AutonomousCommandSessionStream,
    pub text: Option<String>,
    pub truncated: bool,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandSessionOutput {
    pub operation: AutonomousCommandSessionOperation,
    pub session_id: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub running: bool,
    pub exit_code: Option<i32>,
    pub spawned: bool,
    pub chunks: Vec<AutonomousCommandSessionChunk>,
    pub next_sequence: u64,
    pub policy: Option<AutonomousCommandPolicyTrace>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxExecutionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessOwner {
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub repo_id: Option<String>,
    pub user_id: Option<String>,
    pub scope: AutonomousProcessOwnershipScope,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessCommandMetadata {
    pub argv: Vec<String>,
    pub shell_mode: bool,
    pub cwd: String,
    pub sanitized_env: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessStdinState {
    Unavailable,
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessStatus {
    Starting,
    Running,
    Ready,
    Exited,
    Failed,
    Killing,
    Killed,
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessReadinessDetector {
    OutputRegex,
    PortOpen,
    HttpUrl,
    ProcessExit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessReadinessState {
    pub ready: bool,
    pub detector: Option<AutonomousProcessReadinessDetector>,
    pub matched: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessOutputStream {
    Stdout,
    Stderr,
    Combined,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessOutputChunk {
    pub cursor: u64,
    pub stream: AutonomousProcessOutputStream,
    pub text: Option<String>,
    pub truncated: bool,
    pub redacted: bool,
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessMetadata {
    pub process_id: String,
    pub pid: Option<u32>,
    pub parent_pid: Option<u32>,
    pub process_group_id: Option<i64>,
    pub process_name: Option<String>,
    pub executable_path: Option<String>,
    pub label: Option<String>,
    pub process_type: Option<String>,
    pub group: Option<String>,
    pub owner: AutonomousProcessOwner,
    pub command: AutonomousProcessCommandMetadata,
    pub stdin_state: AutonomousProcessStdinState,
    pub status: AutonomousProcessStatus,
    pub started_at: Option<String>,
    pub exited_at: Option<String>,
    pub exit_code: Option<i32>,
    pub output_cursor: Option<u64>,
    pub detected_urls: Vec<String>,
    pub detected_ports: Vec<u16>,
    pub recent_errors: Vec<String>,
    pub recent_warnings: Vec<String>,
    pub recent_stack_traces: Vec<String>,
    pub status_changes: Vec<String>,
    pub readiness: AutonomousProcessReadinessState,
    pub restart_count: u32,
    pub last_restart_reason: Option<String>,
    pub async_job: bool,
    pub timeout_ms: Option<u64>,
    pub output_artifact: Option<AutonomousProcessOutputArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessOutputArtifact {
    pub path: String,
    pub byte_count: usize,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemPort {
    pub protocol: String,
    pub local_addr: String,
    pub local_port: u16,
    pub state: String,
    pub pid: Option<u32>,
    pub process_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessHighlightKind {
    Url,
    Port,
    Warning,
    Error,
    StackTrace,
    StatusChange,
    Readiness,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessHighlight {
    pub process_id: String,
    pub kind: AutonomousProcessHighlightKind,
    pub text: String,
    pub stream: Option<AutonomousProcessOutputStream>,
    pub cursor: Option<u64>,
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessOutputLimits {
    pub recent_output_ring_bytes: usize,
    pub recent_output_ring_chunks: usize,
    pub full_output_artifact_threshold_bytes: usize,
    pub excerpt_bytes: usize,
    pub cursor_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessPersistenceContract {
    pub persist_metadata: bool,
    pub persist_output_chunks: bool,
    pub redact_before_persistence: bool,
    pub persist_policy_trace: bool,
    pub full_output_artifacts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessLifecycleContract {
    pub app_shutdown: String,
    pub thread_switch: String,
    pub session_compaction: String,
    pub crash_recovery: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessManagerContract {
    pub phase: String,
    pub supported_actions: Vec<AutonomousProcessManagerAction>,
    pub ownership_fields: Vec<String>,
    pub risk_levels: Vec<AutonomousProcessActionRiskLevel>,
    pub output_limits: AutonomousProcessOutputLimits,
    pub persistence: AutonomousProcessPersistenceContract,
    pub lifecycle: AutonomousProcessLifecycleContract,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessManagerPolicyTrace {
    pub risk_level: AutonomousProcessActionRiskLevel,
    pub approval_required: bool,
    pub code: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousProcessManagerOutput {
    pub action: AutonomousProcessManagerAction,
    pub phase: String,
    pub spawned: bool,
    pub process_id: Option<String>,
    pub processes: Vec<AutonomousProcessMetadata>,
    pub system_ports: Vec<AutonomousSystemPort>,
    pub chunks: Vec<AutonomousProcessOutputChunk>,
    pub next_cursor: Option<u64>,
    pub digest: Option<String>,
    pub highlights: Vec<AutonomousProcessHighlight>,
    pub policy: AutonomousProcessManagerPolicyTrace,
    pub contract: AutonomousProcessManagerContract,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemDiagnosticsTarget {
    pub pid: Option<u32>,
    pub process_name: Option<String>,
    pub bundle_id: Option<String>,
    pub app_name: Option<String>,
    pub window_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemDiagnosticsPolicyTrace {
    pub risk_level: AutonomousProcessActionRiskLevel,
    pub approval_required: bool,
    pub code: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemDiagnosticsArtifact {
    pub path: String,
    pub byte_count: usize,
    pub redacted: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemDiagnosticsDiagnostic {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemDiagnosticsRow {
    pub row_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fd_kind: Option<AutonomousSystemDiagnosticsFdKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protocol: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_addr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_addr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_percent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub virtual_memory_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub priority: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wait_channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subsystem: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub platform: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSystemDiagnosticsOutput {
    pub action: AutonomousSystemDiagnosticsAction,
    pub platform_supported: bool,
    pub performed: bool,
    pub target: AutonomousSystemDiagnosticsTarget,
    pub policy: AutonomousSystemDiagnosticsPolicyTrace,
    pub summary: String,
    pub rows: Vec<AutonomousSystemDiagnosticsRow>,
    pub truncated: bool,
    pub redacted: bool,
    pub artifact: Option<AutonomousSystemDiagnosticsArtifact>,
    pub diagnostics: Vec<AutonomousSystemDiagnosticsDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousMacosPermissionStatus {
    Granted,
    Denied,
    Unknown,
    Unsupported,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosPermission {
    pub name: String,
    pub status: AutonomousMacosPermissionStatus,
    pub required_for: Vec<AutonomousMacosAutomationAction>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosApp {
    pub name: String,
    pub bundle_id: Option<String>,
    pub pid: Option<u32>,
    pub active: bool,
    pub hidden: bool,
    pub terminated: bool,
    pub bundle_path: Option<String>,
    pub executable_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosWindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosWindow {
    pub window_id: u32,
    pub pid: Option<u32>,
    pub app_name: String,
    pub title: String,
    pub active: bool,
    pub minimized: bool,
    pub bounds: AutonomousMacosWindowBounds,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosScreenshot {
    pub path: String,
    pub target: AutonomousMacosScreenshotTarget,
    pub width: u32,
    pub height: u32,
    pub byte_count: usize,
    pub window_id: Option<u32>,
    pub monitor_id: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosAutomationPolicyTrace {
    pub risk_level: AutonomousProcessActionRiskLevel,
    pub approval_required: bool,
    pub code: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMacosAutomationOutput {
    pub action: AutonomousMacosAutomationAction,
    pub phase: String,
    pub platform_supported: bool,
    pub performed: bool,
    pub apps: Vec<AutonomousMacosApp>,
    pub windows: Vec<AutonomousMacosWindow>,
    pub permissions: Vec<AutonomousMacosPermission>,
    pub screenshot: Option<AutonomousMacosScreenshot>,
    pub policy: AutonomousMacosAutomationPolicyTrace,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMcpServerSummary {
    pub server_id: String,
    pub name: String,
    pub transport: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMcpOutput {
    pub action: AutonomousMcpAction,
    pub servers: Vec<AutonomousMcpServerSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentOutput {
    pub task: AutonomousSubagentTask,
    pub active_tasks: Vec<AutonomousSubagentTask>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousTodoOutput {
    pub action: AutonomousTodoAction,
    pub items: Vec<AutonomousTodoItem>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_item: Option<AutonomousTodoItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousNotebookEditOutput {
    pub path: String,
    pub cell_index: usize,
    pub cell_type: String,
    pub old_source_chars: usize,
    pub new_source_chars: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCodeSymbol {
    pub path: String,
    pub line: usize,
    pub kind: String,
    pub name: String,
    pub preview: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCodeDiagnostic {
    pub path: String,
    pub line: usize,
    pub column: usize,
    pub severity: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCodeIntelOutput {
    pub action: AutonomousCodeIntelAction,
    pub symbols: Vec<AutonomousCodeSymbol>,
    pub diagnostics: Vec<AutonomousCodeDiagnostic>,
    pub scanned_files: usize,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousLspServerStatus {
    pub server_id: String,
    pub language: String,
    pub command: String,
    pub args: Vec<String>,
    pub available: bool,
    pub supports_symbols: bool,
    pub supports_diagnostics: bool,
    pub bundled: bool,
    pub bundle_note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_suggestion: Option<AutonomousLspInstallSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousLspInstallSuggestion {
    pub server_id: String,
    pub language: String,
    pub reason: String,
    pub candidate_commands: Vec<AutonomousLspInstallCommand>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousLspInstallCommand {
    pub label: String,
    pub argv: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousLspOutput {
    pub action: AutonomousLspAction,
    pub mode: String,
    pub servers: Vec<AutonomousLspServerStatus>,
    pub symbols: Vec<AutonomousCodeSymbol>,
    pub diagnostics: Vec<AutonomousCodeDiagnostic>,
    pub scanned_files: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lsp_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install_suggestion: Option<AutonomousLspInstallSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolSearchMatch {
    pub tool_name: String,
    pub group: String,
    pub catalog_kind: String,
    pub description: String,
    pub score: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_pack_ids: Vec<String>,
    pub activation_groups: Vec<String>,
    pub activation_tools: Vec<String>,
    pub tags: Vec<String>,
    pub schema_fields: Vec<String>,
    pub examples: Vec<String>,
    pub risk_class: String,
    pub runtime_available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolSearchOutput {
    pub query: String,
    pub matches: Vec<AutonomousToolSearchMatch>,
    pub truncated: bool,
    pub searched_catalog_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousLocalSkillRoot {
    pub root_id: String,
    pub root_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousBundledSkillRoot {
    pub bundle_id: String,
    pub version: String,
    pub root_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousPluginRoot {
    pub root_id: String,
    pub root_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum AutonomousDynamicToolRoute {
    McpTool {
        server_id: String,
        tool_name: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDynamicToolDescriptor {
    pub name: String,
    pub description: String,
    pub input_schema: JsonValue,
    pub route: AutonomousDynamicToolRoute,
    pub server_id: String,
    pub capability_name: String,
    pub risk_class: String,
}

#[derive(Clone)]
struct AutonomousSkillToolRuntimeConfig {
    project_id: String,
    project_app_data_dir: PathBuf,
    github_runtime: AutonomousSkillRuntime,
    bundled_roots: Vec<AutonomousBundledSkillRoot>,
    local_roots: Vec<AutonomousLocalSkillRoot>,
    project_skills_enabled: bool,
    github_enabled: bool,
    plugin_roots: Vec<AutonomousPluginRoot>,
}

#[derive(Clone)]
pub(super) struct AutonomousSkillToolRuntime {
    project_id: String,
    project_app_data_dir: PathBuf,
    github_runtime: AutonomousSkillRuntime,
    bundled_roots: Vec<AutonomousBundledSkillRoot>,
    local_roots: Vec<AutonomousLocalSkillRoot>,
    project_skills_enabled: bool,
    github_enabled: bool,
    plugin_roots: Vec<AutonomousPluginRoot>,
    discovery_cache: Arc<Mutex<BTreeMap<String, skills::CachedSkillToolCandidate>>>,
}

impl AutonomousSkillToolRuntime {
    fn new(config: AutonomousSkillToolRuntimeConfig) -> Self {
        Self {
            project_id: config.project_id,
            project_app_data_dir: config.project_app_data_dir,
            github_runtime: config.github_runtime,
            bundled_roots: config.bundled_roots,
            local_roots: config.local_roots,
            project_skills_enabled: config.project_skills_enabled,
            github_enabled: config.github_enabled,
            plugin_roots: config.plugin_roots,
            discovery_cache: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

fn default_bundled_skill_roots<R: Runtime>(app: &AppHandle<R>) -> Vec<AutonomousBundledSkillRoot> {
    app.path()
        .resource_dir()
        .ok()
        .map(|root| root.join("skills"))
        .filter(|root| root.is_dir())
        .map(|root| {
            vec![AutonomousBundledSkillRoot {
                bundle_id: "xero".into(),
                version: env!("CARGO_PKG_VERSION").into(),
                root_path: root,
            }]
        })
        .unwrap_or_default()
}

impl std::fmt::Debug for AutonomousSkillToolRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AutonomousSkillToolRuntime")
            .field("project_id", &self.project_id)
            .field("project_app_data_dir", &self.project_app_data_dir)
            .field("bundled_roots", &self.bundled_roots)
            .field("local_roots", &self.local_roots)
            .field("plugin_roots", &self.plugin_roots)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSkillToolStatus {
    Succeeded,
    Unavailable,
    ApprovalRequired,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillToolCandidate {
    pub source_id: String,
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub source_kind: XeroSkillSourceKind,
    pub state: XeroSkillSourceState,
    pub trust: XeroSkillTrustState,
    pub enabled: bool,
    pub installed: bool,
    pub user_invocable: Option<bool>,
    pub version_hash: Option<String>,
    pub cache_key: Option<String>,
    pub access: XeroSkillToolAccessDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillToolOutput {
    pub operation: XeroSkillToolOperation,
    pub status: AutonomousSkillToolStatus,
    pub message: String,
    pub candidates: Vec<AutonomousSkillToolCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<AutonomousSkillToolCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<XeroSkillToolContextPayload>,
    #[serde(default)]
    pub lifecycle_events: Vec<XeroSkillToolLifecycleEvent>,
    #[serde(default)]
    pub diagnostics: Vec<XeroSkillToolDiagnostic>,
    pub truncated: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::commands::RuntimeAgentIdDto;

    #[test]
    fn crawl_runtime_agent_uses_exact_repository_recon_tool_allowlist() {
        let expected: BTreeSet<&str> = TOOL_ACCESS_REPOSITORY_RECON_TOOLS.iter().copied().collect();
        let observed: BTreeSet<&str> = deferred_tool_catalog(true)
            .into_iter()
            .filter(|entry| {
                tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Crawl, entry.tool_name)
            })
            .map(|entry| entry.tool_name)
            .collect();

        assert_eq!(observed, expected);
        for blocked_tool in [
            AUTONOMOUS_TOOL_EDIT,
            AUTONOMOUS_TOOL_WRITE,
            AUTONOMOUS_TOOL_PATCH,
            AUTONOMOUS_TOOL_DELETE,
            AUTONOMOUS_TOOL_RENAME,
            AUTONOMOUS_TOOL_MKDIR,
            AUTONOMOUS_TOOL_COMMAND_VERIFY,
            AUTONOMOUS_TOOL_COMMAND_RUN,
            AUTONOMOUS_TOOL_COMMAND_SESSION,
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            AUTONOMOUS_TOOL_WEB_FETCH,
            AUTONOMOUS_TOOL_MCP_LIST,
            AUTONOMOUS_TOOL_MCP_CALL_TOOL,
            AUTONOMOUS_TOOL_SUBAGENT,
            AUTONOMOUS_TOOL_SKILL,
            AUTONOMOUS_TOOL_AGENT_DEFINITION,
        ] {
            assert!(
                !tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Crawl, blocked_tool),
                "Crawl should not be allowed to use {blocked_tool}"
            );
        }
    }

    #[test]
    fn plan_runtime_agent_uses_exact_planning_tool_allowlist() {
        let expected: BTreeSet<&str> = TOOL_ACCESS_PLANNING_TOOLS.iter().copied().collect();
        let observed: BTreeSet<&str> = deferred_tool_catalog(true)
            .into_iter()
            .filter(|entry| {
                tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Plan, entry.tool_name)
            })
            .map(|entry| entry.tool_name)
            .collect();

        assert_eq!(observed, expected);
        for blocked_tool in [
            AUTONOMOUS_TOOL_EDIT,
            AUTONOMOUS_TOOL_WRITE,
            AUTONOMOUS_TOOL_PATCH,
            AUTONOMOUS_TOOL_DELETE,
            AUTONOMOUS_TOOL_RENAME,
            AUTONOMOUS_TOOL_MKDIR,
            AUTONOMOUS_TOOL_COMMAND_PROBE,
            AUTONOMOUS_TOOL_COMMAND_VERIFY,
            AUTONOMOUS_TOOL_COMMAND_RUN,
            AUTONOMOUS_TOOL_COMMAND_SESSION,
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            AUTONOMOUS_TOOL_WEB_SEARCH,
            AUTONOMOUS_TOOL_WEB_FETCH,
            AUTONOMOUS_TOOL_MCP_LIST,
            AUTONOMOUS_TOOL_MCP_CALL_TOOL,
            AUTONOMOUS_TOOL_SUBAGENT,
            AUTONOMOUS_TOOL_SKILL,
            AUTONOMOUS_TOOL_AGENT_DEFINITION,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH,
        ] {
            assert!(
                !tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Plan, blocked_tool),
                "Plan should not be allowed to use {blocked_tool}"
            );
        }
    }

    #[test]
    fn crawl_repository_recon_policy_keeps_readonly_command_access_but_blocks_mutation() {
        let policy = AutonomousAgentToolPolicy::from_policy_label("repository_recon");

        for allowed_tool in [
            AUTONOMOUS_TOOL_READ,
            AUTONOMOUS_TOOL_SEARCH,
            AUTONOMOUS_TOOL_GIT_STATUS,
            AUTONOMOUS_TOOL_COMMAND_PROBE,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
            AUTONOMOUS_TOOL_WORKSPACE_INDEX,
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
        ] {
            assert!(
                tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::Crawl,
                    allowed_tool,
                    Some(&policy),
                ),
                "repository_recon should allow {allowed_tool}"
            );
        }

        for blocked_tool in [
            AUTONOMOUS_TOOL_EDIT,
            AUTONOMOUS_TOOL_WRITE,
            AUTONOMOUS_TOOL_DELETE,
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            AUTONOMOUS_TOOL_COMMAND_VERIFY,
            AUTONOMOUS_TOOL_COMMAND_RUN,
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            AUTONOMOUS_TOOL_WEB_SEARCH,
            AUTONOMOUS_TOOL_SKILL,
        ] {
            assert!(
                !tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::Crawl,
                    blocked_tool,
                    Some(&policy),
                ),
                "repository_recon should block {blocked_tool}"
            );
        }
    }

    #[test]
    fn planning_policy_allows_todo_but_blocks_mutation_and_commands() {
        let policy = AutonomousAgentToolPolicy::from_policy_label("planning");

        for allowed_tool in [
            AUTONOMOUS_TOOL_READ,
            AUTONOMOUS_TOOL_SEARCH,
            AUTONOMOUS_TOOL_GIT_STATUS,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            AUTONOMOUS_TOOL_WORKSPACE_INDEX,
            AUTONOMOUS_TOOL_TODO,
        ] {
            assert!(
                tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::Plan,
                    allowed_tool,
                    Some(&policy),
                ),
                "planning should allow {allowed_tool}"
            );
        }

        for blocked_tool in [
            AUTONOMOUS_TOOL_EDIT,
            AUTONOMOUS_TOOL_WRITE,
            AUTONOMOUS_TOOL_PATCH,
            AUTONOMOUS_TOOL_DELETE,
            AUTONOMOUS_TOOL_COMMAND_PROBE,
            AUTONOMOUS_TOOL_COMMAND_VERIFY,
            AUTONOMOUS_TOOL_COMMAND_RUN,
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            AUTONOMOUS_TOOL_WEB_SEARCH,
            AUTONOMOUS_TOOL_CODE_INTEL,
            AUTONOMOUS_TOOL_LSP,
            AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
            AUTONOMOUS_TOOL_SKILL,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE,
        ] {
            assert!(
                !tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::Plan,
                    blocked_tool,
                    Some(&policy),
                ),
                "planning should block {blocked_tool}"
            );
        }
    }

    #[test]
    fn agent_definition_object_policy_allows_broad_effect_classes_when_opted_in() {
        let policy = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedEffectClasses": ["observe", "command"],
                "allowedTools": [],
                "deniedTools": [],
                "allowedToolPacks": [],
                "deniedToolPacks": [],
                "commandAllowed": true,
                "destructiveWriteAllowed": false
            }
        }))
        .expect("policy");

        assert!(policy.allows_tool(AUTONOMOUS_TOOL_READ));
        assert!(policy.allows_tool(AUTONOMOUS_TOOL_COMMAND_PROBE));
        assert!(!policy.allows_tool(AUTONOMOUS_TOOL_WRITE));
    }

    #[test]
    fn agent_definition_object_policy_denials_override_allowed_tools() {
        let policy = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedEffectClasses": ["observe"],
                "allowedTools": [AUTONOMOUS_TOOL_READ],
                "deniedTools": [AUTONOMOUS_TOOL_READ],
                "allowedToolPacks": [],
                "deniedToolPacks": [],
                "commandAllowed": false,
                "destructiveWriteAllowed": false
            }
        }))
        .expect("policy");

        assert!(!policy.allows_tool(AUTONOMOUS_TOOL_READ));
        assert!(policy.allows_tool(AUTONOMOUS_TOOL_SEARCH));
    }

    #[test]
    fn s21_agent_definition_tool_packs_expand_and_intersect_with_runtime_policy() {
        let allowed = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedEffectClasses": [],
                "allowedTools": [],
                "deniedTools": [],
                "allowedToolPacks": ["project_context"],
                "deniedToolPacks": [],
                "commandAllowed": false,
                "destructiveWriteAllowed": false,
                "skillRuntimeAllowed": false
            }
        }))
        .expect("allowed pack policy");

        assert!(allowed.allowed_tool_packs.contains("project_context"));
        assert!(tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            Some(&allowed),
        ));
        assert!(tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            Some(&allowed),
        ));
        assert!(!tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_SKILL,
            Some(&allowed),
        ));
        assert!(!tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Ask,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            Some(&allowed),
        ));

        let denied = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedEffectClasses": ["observe"],
                "allowedTools": [],
                "deniedTools": [],
                "allowedToolPacks": [],
                "deniedToolPacks": ["project_context"],
                "commandAllowed": false,
                "destructiveWriteAllowed": false,
                "skillRuntimeAllowed": false
            }
        }))
        .expect("denied pack policy");

        assert!(denied.denied_tool_packs.contains("project_context"));
        assert!(!tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            Some(&denied),
        ));
        assert!(tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_READ,
            Some(&denied),
        ));
    }

    #[test]
    fn s23_custom_agent_subagent_policy_requires_declared_child_roles() {
        let policy = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedEffectClasses": ["agent_delegation"],
                "allowedTools": [AUTONOMOUS_TOOL_SUBAGENT],
                "deniedTools": [],
                "allowedToolPacks": [],
                "deniedToolPacks": [],
                "subagentAllowed": true,
                "allowedSubagentRoles": ["reviewer"],
                "deniedSubagentRoles": ["browser"]
            }
        }))
        .expect("subagent policy");

        assert!(policy.allows_tool(AUTONOMOUS_TOOL_SUBAGENT));
        assert!(policy.allows_subagent_role(AutonomousSubagentRole::Reviewer));
        assert!(!policy.allows_subagent_role(AutonomousSubagentRole::Engineer));
        assert!(!policy.allows_subagent_role(AutonomousSubagentRole::Browser));

        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_agent_tool_policy(Some(policy));
        let allowed = runtime
            .subagent(AutonomousSubagentRequest {
                action: AutonomousSubagentAction::Spawn,
                task_id: None,
                role: Some(AutonomousSubagentRole::Reviewer),
                prompt: Some("Review the proposed change and summarize risks.".into()),
                model_id: None,
                timeout_ms: None,
                max_tool_calls: None,
                max_tokens: None,
                max_cost_micros: None,
                write_set: Vec::new(),
                decision: None,
            })
            .expect("declared role can spawn");
        match allowed.output {
            AutonomousToolOutput::Subagent(output) => {
                assert_eq!(output.task.role, AutonomousSubagentRole::Reviewer);
                assert_eq!(output.task.status, "registered");
            }
            output => panic!("unexpected output: {output:?}"),
        }

        let denied = runtime
            .subagent(AutonomousSubagentRequest {
                action: AutonomousSubagentAction::Spawn,
                task_id: None,
                role: Some(AutonomousSubagentRole::Engineer),
                prompt: Some("Edit the implementation.".into()),
                model_id: None,
                timeout_ms: None,
                max_tool_calls: None,
                max_tokens: None,
                max_cost_micros: None,
                write_set: vec!["src/lib.rs".into()],
                decision: None,
            })
            .expect_err("undeclared role is blocked");
        assert_eq!(denied.code, "policy_denied");
        assert!(denied.message.contains("allowedSubagentRoles"));
    }

    #[test]
    fn s22_custom_workflow_fails_closed_until_required_gate_is_satisfied() {
        let policy = AutonomousAgentWorkflowPolicy::from_definition_snapshot(&json!({
            "workflowStructure": {
                "startPhaseId": "inspect",
                "phases": [
                    {
                        "id": "inspect",
                        "title": "Inspect",
                        "allowedTools": [AUTONOMOUS_TOOL_READ, AUTONOMOUS_TOOL_TODO],
                        "requiredChecks": [
                            {"kind": "todo_completed", "todoId": "inspect_done"}
                        ],
                        "retryLimit": 1,
                        "branches": [
                            {
                                "targetPhaseId": "edit",
                                "condition": {"kind": "todo_completed", "todoId": "inspect_done"}
                            }
                        ]
                    },
                    {
                        "id": "edit",
                        "title": "Edit",
                        "allowedTools": [AUTONOMOUS_TOOL_WRITE],
                        "requiredChecks": []
                    }
                ]
            }
        }))
        .expect("workflow policy");
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_agent_workflow_policy(Some(policy));

        let denied = runtime
            .execute(AutonomousToolRequest::Write(AutonomousWriteRequest {
                path: "notes.txt".into(),
                content: "premature write\n".into(),
            }))
            .expect_err("write is gated before inspect todo completes");
        assert_eq!(denied.code, "policy_denied");
        assert!(denied.message.contains("required gates"));
        assert!(!tempdir.path().join("notes.txt").exists());

        runtime
            .execute(AutonomousToolRequest::Todo(AutonomousTodoRequest {
                action: AutonomousTodoAction::Upsert,
                id: Some("inspect_done".into()),
                title: Some("Inspection gate satisfied".into()),
                notes: None,
                status: Some(AutonomousTodoStatus::Completed),
                mode: None,
                debug_stage: None,
                evidence: Some("Read required context.".into()),
                phase_id: Some("inspect".into()),
                phase_title: Some("Inspect".into()),
                slice_id: None,
                handoff_note: None,
            }))
            .expect("complete gate todo");

        runtime
            .execute(AutonomousToolRequest::Write(AutonomousWriteRequest {
                path: "notes.txt".into(),
                content: "gated write\n".into(),
            }))
            .expect("write allowed after workflow gate");
        assert_eq!(
            std::fs::read_to_string(tempdir.path().join("notes.txt")).expect("read written file"),
            "gated write\n"
        );
    }

    #[test]
    fn s24_external_service_and_browser_control_require_explicit_policy_flags() {
        let denied = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedEffectClasses": ["observe", "external_service", "browser_control"],
                "allowedTools": [],
                "deniedTools": [],
                "allowedToolPacks": [],
                "deniedToolPacks": [],
                "externalServiceAllowed": false,
                "browserControlAllowed": false
            }
        }))
        .expect("denied risky policy");

        assert!(!tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_WEB_FETCH,
            Some(&denied),
        ));
        assert!(!tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            Some(&denied),
        ));

        let allowed = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedEffectClasses": ["observe", "external_service", "browser_control"],
                "allowedTools": [],
                "deniedTools": [],
                "allowedToolPacks": [],
                "deniedToolPacks": [],
                "externalServiceAllowed": true,
                "browserControlAllowed": true
            }
        }))
        .expect("allowed risky policy");

        assert!(tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_WEB_FETCH,
            Some(&allowed),
        ));
        assert!(tool_allowed_for_runtime_agent_with_policy(
            RuntimeAgentIdDto::Engineer,
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            Some(&allowed),
        ));
    }
}
