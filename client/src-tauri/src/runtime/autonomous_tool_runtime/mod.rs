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

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Manager, Runtime};

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
        RepositoryDiffScope, RepositoryStatusEntryDto, RuntimeAgentIdDto,
        RuntimeRunApprovalModeDto, RuntimeRunControlStateDto, SoulSettingsDto,
    },
    runtime::AgentRunCancellationToken,
    state::DesktopState,
};

pub use agent_definition::{
    AutonomousAgentDefinitionAction, AutonomousAgentDefinitionOutput,
    AutonomousAgentDefinitionRequest, AutonomousAgentDefinitionSummary,
    AutonomousAgentDefinitionValidationDiagnostic, AutonomousAgentDefinitionValidationReport,
    AutonomousAgentDefinitionValidationStatus, AUTONOMOUS_TOOL_AGENT_DEFINITION,
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

pub const AUTONOMOUS_TOOL_READ: &str = "read";
pub const AUTONOMOUS_TOOL_SEARCH: &str = "search";
pub const AUTONOMOUS_TOOL_FIND: &str = "find";
pub const AUTONOMOUS_TOOL_GIT_STATUS: &str = "git_status";
pub const AUTONOMOUS_TOOL_GIT_DIFF: &str = "git_diff";
pub const AUTONOMOUS_TOOL_TOOL_ACCESS: &str = "tool_access";
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
pub const AUTONOMOUS_TOOL_PROCESS_MANAGER: &str = "process_manager";
pub const AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS: &str = "system_diagnostics";
pub const AUTONOMOUS_TOOL_MACOS_AUTOMATION: &str = "macos_automation";
pub const AUTONOMOUS_TOOL_MCP: &str = "mcp";
pub const AUTONOMOUS_TOOL_SUBAGENT: &str = "subagent";
pub const AUTONOMOUS_TOOL_TODO: &str = "todo";
pub const AUTONOMOUS_TOOL_NOTEBOOK_EDIT: &str = "notebook_edit";
pub const AUTONOMOUS_TOOL_CODE_INTEL: &str = "code_intel";
pub const AUTONOMOUS_TOOL_LSP: &str = "lsp";
pub const AUTONOMOUS_TOOL_POWERSHELL: &str = "powershell";
pub const AUTONOMOUS_TOOL_TOOL_SEARCH: &str = "tool_search";
pub const AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT: &str = "environment_context";
pub const AUTONOMOUS_TOOL_PROJECT_CONTEXT: &str = "project_context";
pub const AUTONOMOUS_TOOL_SKILL: &str = "skill";
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

const TOOL_ACCESS_CORE_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_READ,
    AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_GIT_STATUS,
    AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_TOOL_ACCESS,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT,
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
    AUTONOMOUS_TOOL_COMMAND,
    AUTONOMOUS_TOOL_COMMAND_SESSION_START,
    AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
    AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
];
const TOOL_ACCESS_PROCESS_MANAGER_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_PROCESS_MANAGER];
const TOOL_ACCESS_SYSTEM_DIAGNOSTICS_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS];
const TOOL_ACCESS_MACOS_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_MACOS_AUTOMATION];
const TOOL_ACCESS_WEB_SEARCH_ONLY_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_WEB_SEARCH];
const TOOL_ACCESS_WEB_FETCH_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_WEB_FETCH];
const TOOL_ACCESS_BROWSER_OBSERVE_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_BROWSER];
const TOOL_ACCESS_BROWSER_CONTROL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_BROWSER];
const TOOL_ACCESS_WEB_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_WEB_SEARCH,
    AUTONOMOUS_TOOL_WEB_FETCH,
    AUTONOMOUS_TOOL_BROWSER,
];
const TOOL_ACCESS_COMMAND_READONLY_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_COMMAND];
const TOOL_ACCESS_COMMAND_MUTATING_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_COMMAND,
    AUTONOMOUS_TOOL_COMMAND_SESSION_START,
    AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
    AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
];
const TOOL_ACCESS_COMMAND_SESSION_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_COMMAND_SESSION_START,
    AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
    AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
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
const TOOL_ACCESS_MCP_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_MCP];
const TOOL_ACCESS_MCP_LIST_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_MCP];
const TOOL_ACCESS_MCP_INVOKE_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_MCP];
const TOOL_ACCESS_INTELLIGENCE_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_CODE_INTEL, AUTONOMOUS_TOOL_LSP];
const TOOL_ACCESS_NOTEBOOK_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_NOTEBOOK_EDIT];
const TOOL_ACCESS_POWERSHELL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_POWERSHELL];
const TOOL_ACCESS_ENVIRONMENT_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT];
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
        description: "Short repo-scoped commands for tests, builds, linting, and diagnostics.",
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
            json!({
                "toolName": entry.tool_name,
                "group": entry.group,
                "activationGroups": tool_catalog_activation_groups(entry.tool_name),
                "activationTools": [entry.tool_name],
                "tags": entry.tags,
                "schemaFields": entry.schema_fields,
                "examples": entry.examples,
                "riskClass": entry.risk_class,
                "effectClass": tool_effect_class(entry.tool_name).as_str(),
                "allowedRuntimeAgents": allowed_runtime_agent_labels(entry.tool_name),
                "runtimeAvailable": true,
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
    external_service_allowed: bool,
    browser_control_allowed: bool,
    skill_runtime_allowed: bool,
    subagent_allowed: bool,
    command_allowed: bool,
    destructive_write_allowed: bool,
}

impl AutonomousAgentToolPolicy {
    pub fn from_definition_snapshot(snapshot: &JsonValue) -> Option<Self> {
        let value = snapshot.get("toolPolicy")?;
        if let Some(label) = value.as_str() {
            return Some(Self::from_policy_label(label));
        }
        let object = value.as_object()?;
        let mut allowed_tools = string_set_from_json(object.get("allowedTools"));
        for group in string_set_from_json(object.get("allowedToolGroups")) {
            if let Some(tools) = tool_access_group_tools(&group) {
                allowed_tools.extend(tools.iter().map(|tool| (*tool).to_owned()));
            }
        }
        Some(Self {
            allowed_effect_classes: string_set_from_json(object.get("allowedEffectClasses")),
            allowed_tools,
            denied_tools: string_set_from_json(object.get("deniedTools")),
            external_service_allowed: json_bool(object.get("externalServiceAllowed")),
            browser_control_allowed: json_bool(object.get("browserControlAllowed")),
            skill_runtime_allowed: json_bool(object.get("skillRuntimeAllowed")),
            subagent_allowed: json_bool(object.get("subagentAllowed")),
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
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
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
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
                command_allowed: false,
                destructive_write_allowed: false,
            },
            _ => Self {
                allowed_effect_classes: ["observe"].into_iter().map(ToOwned::to_owned).collect(),
                allowed_tools: BTreeSet::new(),
                denied_tools: BTreeSet::new(),
                external_service_allowed: false,
                browser_control_allowed: false,
                skill_runtime_allowed: false,
                subagent_allowed: false,
                command_allowed: false,
                destructive_write_allowed: false,
            },
        }
    }

    pub fn allows_tool(&self, tool_name: &str) -> bool {
        if self.denied_tools.contains(tool_name) {
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
        | AUTONOMOUS_TOOL_CODE_INTEL
        | AUTONOMOUS_TOOL_LSP
        | AUTONOMOUS_TOOL_TOOL_SEARCH
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT
        | AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT => AutonomousToolEffectClass::Observe,
        AUTONOMOUS_TOOL_TOOL_ACCESS | AUTONOMOUS_TOOL_TODO | AUTONOMOUS_TOOL_AGENT_DEFINITION => {
            AutonomousToolEffectClass::RuntimeState
        }
        AUTONOMOUS_TOOL_WRITE
        | AUTONOMOUS_TOOL_EDIT
        | AUTONOMOUS_TOOL_PATCH
        | AUTONOMOUS_TOOL_RENAME
        | AUTONOMOUS_TOOL_MKDIR
        | AUTONOMOUS_TOOL_NOTEBOOK_EDIT => AutonomousToolEffectClass::Write,
        AUTONOMOUS_TOOL_DELETE => AutonomousToolEffectClass::DestructiveWrite,
        AUTONOMOUS_TOOL_COMMAND
        | AUTONOMOUS_TOOL_COMMAND_SESSION_START
        | AUTONOMOUS_TOOL_COMMAND_SESSION_READ
        | AUTONOMOUS_TOOL_COMMAND_SESSION_STOP
        | AUTONOMOUS_TOOL_POWERSHELL => AutonomousToolEffectClass::Command,
        AUTONOMOUS_TOOL_PROCESS_MANAGER
        | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS
        | AUTONOMOUS_TOOL_MACOS_AUTOMATION => AutonomousToolEffectClass::ProcessControl,
        AUTONOMOUS_TOOL_WEB_SEARCH | AUTONOMOUS_TOOL_WEB_FETCH | AUTONOMOUS_TOOL_MCP => {
            AutonomousToolEffectClass::ExternalService
        }
        AUTONOMOUS_TOOL_BROWSER => AutonomousToolEffectClass::BrowserControl,
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
    if tool_name == AUTONOMOUS_TOOL_AGENT_DEFINITION {
        return agent_id == RuntimeAgentIdDto::AgentCreate;
    }
    match agent_id {
        RuntimeAgentIdDto::Engineer | RuntimeAgentIdDto::Debug => true,
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

fn allowed_runtime_agent_labels(tool_name: &str) -> Vec<&'static str> {
    let mut agents = Vec::new();
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Ask, tool_name) {
        agents.push(RuntimeAgentIdDto::Ask.as_str());
    }
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Engineer, tool_name) {
        agents.push(RuntimeAgentIdDto::Engineer.as_str());
    }
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Debug, tool_name) {
        agents.push(RuntimeAgentIdDto::Debug.as_str());
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
            "Find glob/pattern matches in repo-scoped files.",
            &["file", "glob", "find", "tree"],
            &["pattern", "path"],
            &["Find **/*.rs files under src-tauri."],
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
            AUTONOMOUS_TOOL_PROJECT_CONTEXT,
            "core",
            "Search, read, record, update, and refresh source-cited durable project context, approved memory, handoffs, and current context manifests with freshness evidence.",
            &["context", "memory", "records", "handoff", "retrieval", "citations"],
            &[
                "action",
                "query",
                "recordId",
                "memoryId",
                "recordKinds",
                "memoryKinds",
                "tags",
                "relatedPaths",
                "limit",
                "title",
                "summary",
                "text",
            ],
            &[
                "Search project records before prior-work-sensitive tasks.",
                "Record or update durable context after a durable finding.",
            ],
            "observe",
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
            "Maintain model-visible planning state for the current owned-agent run.",
            &["plan", "todo", "task", "state"],
            &["action", "id", "title", "notes", "status"],
            &["Track inspect, edit, verify steps for a multi-file change."],
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
            AUTONOMOUS_TOOL_COMMAND,
            "command",
            "Run a repo-scoped command.",
            &["command", "shell", "test", "lint", "build", "cargo", "npm", "pnpm"],
            &["argv", "cwd", "timeoutMs"],
            &["Run npm test.", "Run cargo test for the changed crate."],
            "command",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_SESSION_START,
            "command",
            "Start a repo-scoped long-running command session and capture live output chunks.",
            &["command", "session", "long_running", "dev_server", "watch"],
            &["argv", "cwd", "timeoutMs"],
            &["Start a dev server or watcher."],
            "long_running_process",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
            "command",
            "Read new output chunks and exit state from a command session.",
            &["command", "session", "output", "read"],
            &["sessionId", "afterSequence", "maxBytes"],
            &["Read new output from a running test watcher."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
            "command",
            "Stop a command session and return final captured output chunks.",
            &["command", "session", "stop", "cleanup"],
            &["sessionId"],
            &["Stop a dev server after verification."],
            "process_control",
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
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS,
            "system_diagnostics",
            "Run typed, policy-aware advanced diagnostics including process open-file inspection, process resource snapshots, thread lists, sampling, unified logs, accessibility snapshots, and diagnostic bundles.",
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
                "accessibility",
                "hung process",
                "port conflict",
                "tauri",
                "macos focus",
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
                "Request a bounded diagnostics bundle for a hung process.",
            ],
            "system_read",
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
            AUTONOMOUS_TOOL_BROWSER,
            "web",
            "Drive the in-app browser with navigation, DOM actions, screenshots, diagnostics, accessibility snapshots, and state save/restore.",
            &["browser", "frontend", "ui", "dom", "screenshot", "accessibility", "console", "network", "storage", "cookies"],
            &["action", "url", "selector", "text", "timeoutMs", "tabId", "area", "key"],
            &["Observe current URL and page text.", "Click and type into a local app after activation."],
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
            AUTONOMOUS_TOOL_MCP,
            "mcp",
            "List and invoke connected MCP tools, resources, and prompts over stdio, HTTP, or SSE.",
            &["mcp", "model_context_protocol", "tool", "resource", "prompt", "external"],
            &["action", "serverId", "name", "uri", "arguments", "timeoutMs"],
            &["List MCP server tools.", "Invoke a named MCP tool after activation."],
            "external_capability",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_SUBAGENT,
            "agent_ops",
            "Manage async model-routed subagent tasks.",
            &["agent", "subagent", "delegate", "explore", "verify", "parallel"],
            &["action", "taskId", "role", "prompt", "modelId", "writeSet", "decision"],
            &["Spawn an explorer for a bounded codebase question.", "Poll a worker and integrate its result with a parent decision."],
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
    pub(super) agent_run_context: Option<AutonomousAgentRunContext>,
    pub(super) browser_control_preference: BrowserControlPreferenceDto,
    pub(super) soul_settings: SoulSettingsDto,
    pub(super) browser_executor: Option<Arc<dyn BrowserExecutor>>,
    pub(super) emulator_executor: Option<Arc<dyn EmulatorExecutor>>,
    pub(super) solana_executor: Option<Arc<dyn SolanaExecutor>>,
    pub(super) cancellation_token: Option<AgentRunCancellationToken>,
    pub(super) mcp_registry_path: Option<PathBuf>,
    pub(super) environment_profile_database_path: Option<PathBuf>,
    pub(super) todo_items: Arc<Mutex<BTreeMap<String, AutonomousTodoItem>>>,
    pub(super) subagent_tasks: Arc<Mutex<BTreeMap<String, AutonomousSubagentTask>>>,
    pub(super) subagent_executor: Option<Arc<dyn AutonomousSubagentExecutor>>,
    pub(super) subagent_execution_depth: usize,
    pub(super) subagent_write_scope: Option<AutonomousSubagentWriteScope>,
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
            .field("agent_run_context", &self.agent_run_context)
            .field(
                "browser_control_preference",
                &self.browser_control_preference,
            )
            .field("soul_settings", &self.soul_settings)
            .field("mcp_registry_path", &self.mcp_registry_path)
            .field("subagent_execution_depth", &self.subagent_execution_depth)
            .field("subagent_write_scope", &self.subagent_write_scope)
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
            agent_run_context: None,
            browser_control_preference: BrowserControlPreferenceDto::Default,
            soul_settings: default_soul_settings(),
            browser_executor: None,
            emulator_executor: None,
            solana_executor: None,
            cancellation_token: None,
            mcp_registry_path: None,
            environment_profile_database_path: None,
            todo_items: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_tasks: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_executor: None,
            subagent_execution_depth: 0,
            subagent_write_scope: None,
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

    fn check_cancelled(&self) -> CommandResult<()> {
        if let Some(token) = &self.cancellation_token {
            token.check_cancelled()?;
        }
        Ok(())
    }

    fn is_cancelled(&self) -> bool {
        self.cancellation_token
            .as_ref()
            .is_some_and(AgentRunCancellationToken::is_cancelled)
    }

    pub fn execute(&self, request: AutonomousToolRequest) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        match request {
            AutonomousToolRequest::Read(request) => self.read(request),
            AutonomousToolRequest::Search(request) => self.search(request),
            AutonomousToolRequest::Find(request) => self.find(request),
            AutonomousToolRequest::GitStatus(request) => self.git_status(request),
            AutonomousToolRequest::GitDiff(request) => self.git_diff(request),
            AutonomousToolRequest::ToolAccess(request) => self.tool_access(request),
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

    pub fn execute_approved(
        &self,
        request: AutonomousToolRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        match request {
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
            request => self.execute(request),
        }
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
        tool != AUTONOMOUS_TOOL_SKILL || self.skill_tool_enabled()
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
    pub path: Option<String>,
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
    Cancel,
    Integrate,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousSubagentRole {
    Explorer,
    Worker,
    Verifier,
    Reviewer,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_set: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentTask {
    pub subagent_id: String,
    pub role: AutonomousSubagentRole,
    pub prompt: String,
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub write_set: Vec<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousTodoItem {
    pub id: String,
    pub title: String,
    pub notes: Option<String>,
    pub status: AutonomousTodoStatus,
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
